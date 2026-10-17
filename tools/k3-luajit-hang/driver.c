// K3 (issue 029k1) host-side driver for the babysteps LuaJIT-hang
// experiments. Loads the real level script plus its real dependency
// scripts (Packs/base/Scripts/{utils,common,commonpatterns}.lua) with the
// game API stubbed, then drives `onUpdate` from C per-frame (exactly what
// `oh_game::lua_call::call_on_update` does) plus the scripted tutorial
// `onInput` events, so the full babysteps state machine (steps 0 -> 6,
// including `e_kill` at levelTime > 45 after the mid-level `l_resetTime`)
// runs, `onStep` fires and the real pattern-spawn functions execute.
//
// Usage:
//   driver <level.lua> [--iters N] [--dt s] [--jit-off] [--report N]
//         [--watchdog s] [--pack-root DIR]
//
// Exit codes: 0 ok (completed or tutorial-killed), 2 usage, 3 watchdog
// (no progress for `--watchdog` seconds — the hang signature), 4 Lua error.

#include <luajit.h>
#include <lauxlib.h>
#include <lualib.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <signal.h>
#include <time.h>

static long g_iter = 0;
static double g_level_time = 0.0;
static double g_dt;
static long g_report_every = 20000;
static volatile sig_atomic_t g_alarm_armed = 0;
static double g_start_wall;
static long g_dump_iter = 0;

// NDK clang emulates TLS by default for static Android executables
// (`__emutls_v.*`), which would leave Bionic's own 8-byte-aligned static TLS
// (its `__libc_tls_generation_copy`) as the only executable PT_TLS segment.
// NDK 27's Bionic aborts static aarch64 executables whose executable TLS
// segment is not 64-aligned with zero skew. A real local-exec TLS variable
// with 64-byte alignment (referenced below so it cannot be eliminated)
// forces the linker to emit a 64-aligned, zero-skew PT_TLS that Bionic
// accepts (build.sh passes -fno-emulated-tls -ftls-model=local-exec for
// aarch64). See tls64.py / README for the history of this quirk.
__attribute__((aligned(64))) static __thread char k3_tls_align_probe[8] = {1};
static void k3_tls_probe_touch(void) {
    volatile char v = k3_tls_align_probe[0];
    (void)v;
}

// Timeline model (enough for babysteps' onStep cadence): entries are
// scheduled at `t_wait`-advanced cursor positions; `w_wall` marks an entry
// at the current cursor; the timeline is "occupied" until levelTime passes
// the last entry's time (then onStep may fire again, like the game).
static double g_tl_cursor = 0.0;
static double g_tl_free_at = 0.0;
static const char *g_pack_root = ".";

static double now_s(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec * 1e-9;
}

static void on_alarm(int sig) {
    (void)sig;
    if (!g_alarm_armed) return;
    char buf[256];
    int n = snprintf(buf, sizeof buf,
                     "K3-DRIVER WATCHDOG: no progress for the alarm window; "
                     "iter=%ld levelTime=%.3f\n",
                     (long)g_iter, g_level_time);
    ssize_t w = write(2, buf, (size_t)n);
    (void)w;  // best effort: nothing sensible to do in a signal handler
    _exit(3);
}

// ---- game API implemented in C (needs driver state) -----------------------

static int cb_t_wait(lua_State *L) {
    g_tl_cursor += lua_tonumber(L, 1);
    return 0;
}

static int cb_tl_entry(lua_State *L) {  // w_wall / w_owall: an entry now
    (void)L;
    g_tl_free_at = g_tl_cursor;
    return 0;
}

static int cb_num_entries(lua_State *L) {
    lua_pushnumber(L, g_level_time < g_tl_free_at ? 1 : 0);
    return 1;
}

static int cb_next_step(lua_State *L) {
    // next 30 s step boundary at/after current levelTime
    lua_pushnumber(L, 30.0 * ((long)(g_level_time / 30.0) + 1));
    return 1;
}

static int cb_reset_time(lua_State *L) {
    g_level_time = 0.0;
    g_tl_cursor = 0.0;
    g_tl_free_at = 0.0;
    return 0;
}

static int cb_level_time(lua_State *L) {
    lua_pushnumber(L, g_level_time);
    return 1;
}

// u_execDependencyScript(disambiguator, name, author, file) /
// u_execScript(file): load and run <pack-root>/Packs/base/Scripts/<file>.
// (babysteps' dependencies all resolve into the "base" pack.)
static int run_pack_script(lua_State *L, const char *file) {
    char path[1024];
    snprintf(path, sizeof path, "%s/Packs/base/Scripts/%s", g_pack_root, file);
    FILE *f = fopen(path, "rb");
    if (!f) return luaL_error(L, "K3: cannot open %s", path);
    if (fseek(f, 0, SEEK_END) != 0) { fclose(f); return luaL_error(L, "K3: seek %s", path); }
    long len = ftell(f);
    if (len < 0) { fclose(f); return luaL_error(L, "K3: tell %s", path); }
    rewind(f);
    char *buf = malloc((size_t)len + 1);
    if (!buf || fread(buf, 1, (size_t)len, f) != (size_t)len) {
        fclose(f); free(buf);
        return luaL_error(L, "K3: read %s", path);
    }
    fclose(f);
    buf[len] = 0;
    fprintf(stderr, "K3-DRIVER: loaded dependency %s (%ld bytes)\n", path, (long)len);
    int rc = luaL_loadbuffer(L, buf, (size_t)len, path);
    if (rc == LUA_OK) rc = lua_pcall(L, 0, 0, 0);
    free(buf);
    return rc;  // 0 or an error left on the stack
}

static int cb_exec_dep_script(lua_State *L) {
    // args: disambiguator, name, author, file
    return run_pack_script(L, luaL_checkstring(L, 4));
}

static int cb_exec_script(lua_State *L) {
    return run_pack_script(L, luaL_checkstring(L, 1));
}

static void register_c_api(lua_State *L) {
    struct { const char *name; lua_CFunction fn; } cbs[] = {
        {"t_wait", cb_t_wait},
        {"w_wall", cb_tl_entry},
        {"w_owall", cb_tl_entry},
        {"l_getNumTimelineEntries", cb_num_entries},
        {"l_getNextTimelineStep", cb_next_step},
        {"l_getLevelTime", cb_level_time},
        {"l_resetTime", cb_reset_time},
        {"u_execDependencyScript", cb_exec_dep_script},
        {"u_execScript", cb_exec_script},
    };
    for (size_t i = 0; i < sizeof cbs / sizeof cbs[0]; i++) {
        lua_pushcfunction(L, cbs[i].fn);
        lua_setglobal(L, cbs[i].name);
    }
}

// ---- Lua-side stubs (pure no-ops / constants) ----------------------------

static const char *STUBS =
    "u_rndInt = function(a, b) return math.random(a, b) end\n"
    "u_rndIntUpper = function(upper) return math.random(1, upper) end\n"
    "u_rndReal = function() return math.random() end\n"
    "u_clearWalls = function() end\n"
    "steam_unlockAchievement = function() end\n"
    "a_playSound = function() end\n"
    "a_playPackSound = function() end\n"
    "e_messageAdd = function() end\n"
    "e_messageAddImportant = function() end\n"
    "e_stopTimeS = function() end\n"
    "e_kill = function() killed = true end\n"
    "l_setSpeedMult = function() end\n"
    "l_setSpeedInc = function() end\n"
    "l_setRotationSpeed = function() end\n"
    "l_setRotationSpeedMax = function() end\n"
    "l_setRotationSpeedInc = function() end\n"
    "l_setDelayMult = function() end\n"
    "l_setDelayInc = function() end\n"
    "l_setFastSpin = function() end\n"
    "l_setSides = function() end\n"
    "l_setSidesMin = function() end\n"
    "l_setSidesMax = function() end\n"
    "l_setIncTime = function() end\n"
    "l_setTutorialMode = function() end\n"
    "l_setBeatPulseMax = function() end\n"
    "l_setBeatPulseDelayMax = function() end\n"
    "l_setBeatPulseSpeedMult = function() end\n"
    "l_addTracked = function() end\n"
    "l_clearTracked = function() end\n"
    "l_setIncEnabled = function() end\n"
    "l_setSwapEnabled = function() end\n"
    "l_getSides = function() return 6 end\n"
    "l_getDelayMult = function() return 1.65 end\n"
    "l_getRotation = function() return 0 end\n"
    "l_getSwapEnabled = function() return false end\n"
    "u_getDifficultyMult = function() return 1 end\n"
    "u_getSpeedMultDM = function() return 0.65 end\n"
    "u_getDelayMultDM = function() return 1.65 end\n"
    "u_getPlayerAngle = function() return 0 end\n"
    "u_getVersionMajor = function() return 1 end\n"
    "u_getVersionMinor = function() return 12 end\n"
    "u_getVersionMicro = function() return 1 end\n"
    "levelTime = 0\n"
    "killed = false\n"
    "function shuffle(t)\n"
    "  for i = #t, 2, -1 do local j = math.random(i) t[i], t[j] = t[j], t[i] end\n"
    "end\n";

// The scripted tutorial input (babysteps steps 0..6). The state machine
// needs: (a) both rotations while step1, (b) focus+both rotations while
// step25. After step3 the mid-level l_resetTime restarts levelTime and the
// level runs unattended to levelTime > 45 where e_kill fires.
struct input_event { double t; int move; int focus; };
static const struct input_event INPUT_EVENTS[] = {
    { 0.30, -1, 0 },
    { 0.30,  1, 0 },
    { 0.62, -1, 1 },
    { 0.62,  1, 1 },
};
static size_t g_input_done = 0;

static int run(lua_State *L, const char *what, const char *code, int results) {
    if (luaL_loadbuffer(L, code, strlen(code), what) != LUA_OK ||
        lua_pcall(L, 0, results, 0) != LUA_OK) {
        fprintf(stderr, "K3-DRIVER: %s failed: %s\n", what, lua_tostring(L, -1));
        return 1;
    }
    return 0;
}

static int call_on_input(lua_State *L, double t, int move, int focus) {
    lua_getglobal(L, "onInput");
    if (lua_isnil(L, -1)) { lua_pop(L, 1); return 0; }
    lua_pushnumber(L, t);
    lua_pushnumber(L, (double)move);
    lua_pushboolean(L, focus);
    lua_pushnumber(L, 0);  // mSwap
    int rc = lua_pcall(L, 4, 0, 0);
    if (rc != LUA_OK) {
        fprintf(stderr, "K3-DRIVER: onInput failed at t=%.2f: %s\n",
                t, lua_tostring(L, -1));
        lua_pop(L, 1);
        return 1;
    }
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr,
                "usage: %s <level.lua> [--iters N] [--dt s] [--jit-off] "
                "[--report N] [--watchdog s] [--pack-root DIR] [--dump N]\n",
                argv[0]);
        return 2;
    }

    long iters = 3000000;
    g_dt = 1.0 / 360.0;
    int jit_off = 0;
    long watchdog_s = 300;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--iters") == 0 && i + 1 < argc)
            iters = atol(argv[++i]);
        else if (strcmp(argv[i], "--dt") == 0 && i + 1 < argc)
            g_dt = atof(argv[++i]);
        else if (strcmp(argv[i], "--jit-off") == 0)
            jit_off = 1;
        else if (strcmp(argv[i], "--report") == 0 && i + 1 < argc)
            g_report_every = atol(argv[++i]);
        else if (strcmp(argv[i], "--watchdog") == 0 && i + 1 < argc)
            watchdog_s = atol(argv[++i]);
        else if (strcmp(argv[i], "--pack-root") == 0 && i + 1 < argc)
            g_pack_root = argv[++i];
        else if (strcmp(argv[i], "--dump") == 0 && i + 1 < argc)
            g_dump_iter = atol(argv[++i]);
    }

    lua_State *L = luaL_newstate();
    luaL_openlibs(L);
    k3_tls_probe_touch();
    printf("K3-DRIVER: %s, LUA_VERSION=%s\n", LUAJIT_VERSION, LUA_VERSION);
    if (jit_off) {
        if (run(L, "jit.off", "jit.off()", 0)) return 4;
        printf("K3-DRIVER: JIT OFF (interpreter only)\n");
    }
    if (run(L, "stubs", STUBS, 0)) return 4;
    register_c_api(L);

    // Load the real level script (u_execDependencyScript loads the real
    // Packs/base/Scripts/*.lua from --pack-root, default cwd).
    FILE *f = fopen(argv[1], "rb");
    if (!f) {
        perror("open level");
        return 2;
    }
    char *src = malloc(1 << 20);
    size_t n = fread(src, 1, (1 << 20) - 1, f);
    fclose(f);
    if (luaL_loadbuffer(L, src, n, "@level") != LUA_OK ||
        lua_pcall(L, 0, 0, 0) != LUA_OK) {
        fprintf(stderr, "K3-DRIVER: level load failed: %s\n", lua_tostring(L, -1));
        return 4;
    }
    free(src);
    if (run(L, "onInit", "if onInit then onInit() end", 0)) return 4;

    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_handler = on_alarm;
    sigaction(SIGALRM, &sa, NULL);

    g_start_wall = now_s();
    const struct input_event *ev = INPUT_EVENTS;
    const struct input_event *ev_end =
        INPUT_EVENTS + sizeof INPUT_EVENTS / sizeof INPUT_EVENTS[0];

    for (long i = 1; i <= iters; i++) {
        g_level_time += g_dt;
        while (ev < ev_end && g_level_time >= ev->t) {
            if (call_on_input(L, ev->t, ev->move, ev->focus)) return 4;
            ev++;
        }
        // The game resolves `onUpdate` from the level environment every
        // frame — do the same (a held C-stack reference is consumed by
        // each pcall).
        lua_getglobal(L, "onUpdate");
        lua_pushnumber(L, g_level_time);
        int rc = lua_pcall(L, 1, 0, 0);
        if (rc != LUA_OK) {
            fprintf(stderr, "K3-DRIVER: onUpdate failed at iter %ld: %s\n",
                    i, lua_tostring(L, -1));
            return 4;
        }
        g_iter = i;

        if (g_dump_iter != 0 && i == g_dump_iter) {
            static const char *state_vars[] = {
                "step0", "step1", "step2", "step25", "step3", "step35",
                "step4", "step5", "step6", "step0_trRotateLeft",
                "step0_trRotateRight", "step1_trFocus", "levelTime",
            };
            fprintf(stderr, "K3-DRIVER: state dump at iter %ld:\n", i);
            for (size_t v = 0; v < sizeof state_vars / sizeof state_vars[0]; v++) {
                lua_getglobal(L, state_vars[v]);
                char val[128];
                if (lua_isboolean(L, -1)) snprintf(val, sizeof val, "%s",
                                                 lua_toboolean(L, -1) ? "true" : "false");
                else if (lua_isnumber(L, -1))
                    snprintf(val, sizeof val, "%.3f", lua_tonumber(L, -1));
                else if (lua_isstring(L, -1)) snprintf(val, sizeof val, "\"%s\"",
                                                       lua_tostring(L, -1));
                else if (lua_isnil(L, -1)) snprintf(val, sizeof val, "nil");
                else snprintf(val, sizeof val, "<? %s ?>", lua_typename(L, lua_type(L, -1)));
                fprintf(stderr, "  %-20s = %s\n", state_vars[v], val);
                lua_pop(L, 1);
            }
            fflush(stderr);
        }

        if (i % g_report_every == 0) {
            double wall = now_s();
            // Count compiled traces (JIT on): a frozen counter with a
            // spinning CPU is the trace-compile-hang signature.
            // (Trace numbers start at 1; aborted traces free their number
            // for reuse, so this is a lower bound — enough for the hang
            // signal.)
            int ntr = -1;
            if (run(L, "trace-count",
                    "local ok, util = pcall(require, 'jit.util') "
                    "if ok then "
                    "  local c = 0 "
                    "  while util.traceinfo(c + 1) ~= nil do c = c + 1 end "
                    "  return c "
                    "end "
                    "return -1",
                    1) == 0) {
                ntr = (int)lua_tointeger(L, -1);
                lua_pop(L, 1);
            }
            printf("iter=%-8ld wall=%8.2fs levelTime=%8.3f traces=%d\n",
                   i, wall, g_level_time, ntr);
            fflush(stdout);
            // (Re)arm the watchdog for the next window.
            alarm((unsigned)watchdog_s);
            g_alarm_armed = 1;
        }

        // The tutorial completes (e_kill at levelTime > 45) — stop early
        // like the game would.
        lua_getglobal(L, "killed");
        int killed = lua_toboolean(L, -1);
        lua_pop(L, 1);
        if (killed) {
            printf("K3-DRIVER: level killed (tutorial complete) at iter %ld, wall %.2fs\n",
                   i, now_s() - g_start_wall);
            return 0;
        }
    }
    printf("K3-DRIVER: completed %ld iterations in %.2fs\n", iters, now_s() - g_start_wall);
    return 0;
}
