// issue 029k1 (K3): hot-loop driver for the babysteps `onUpdate` hang.
//
// Links against a LuaJIT static library (vendored upstream 2.1.0-beta3
// lineage OR the C++'s pinned 4dc5ffbf — see build.sh), loads the real
// _RELEASE babysteps.lua with stubbed game API, and hammers `onUpdate`
// exactly like the game's per-frame `call_on_update`:
//
//   * `levelTime` advances by --dt (default 1/360 s, the phone's
//     free-run frame cadence) on every call, so the tutorial state
//     machine lingers in each step the way real play does (no input is
//     fed — the 029a1 ANR happened before any input was needed, in the
//     step0/step1 welcome state);
//   * each iteration is a fresh C-side `lua_pcall` of `onUpdate` — the
//     same call shape as `HexagonGame::update` -> `call_on_update`;
//   * progress (iteration count, elapsed, number of compiled traces) is
//     printed every --report iterations; a SIGALRM watchdog aborts with
//     exit 3 if the progress print itself stops coming (the
//     trace-compile-hang signature: CPU pegged, iteration counter frozen).
//
// Exit codes: 0 = loop completed (level killed via e_kill, or N reached);
// 3 = watchdog fired (HANG — no progress for the alarm window);
// 4 = Lua runtime error (aborted early, message printed).
//
// Usage: driver <babysteps.lua> [--iters N] [--dt s] [--jit-off]
//        [--report N] [--watchdog s]
//
// The --jit-off flag calls `jit.off()` before the loop: this is the cheap
// K3 experiment ("does the interpreter-only path play babysteps?").

#include <lua.h>
#include <lauxlib.h>
#include <lualib.h>
#include <luajit.h>
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

// Stubs for the C++ game API surface babysteps.lua references. Every
// function is a no-op (or a counter) — the point is the JIT-visible shape
// of `onUpdate` (its branches, the HREFs on the "yes!"/"nope" string
// constants, the per-frame C calls), not game behavior.
static const char *STUBS =
    "u_execDependencyScript = function() return true end\n"
    "u_rndInt = function(a, b) return math.random(a, b) end\n"
    "u_clearWalls = function() end\n"
    "steam_unlockAchievement = function() end\n"
    "a_playSound = function() end\n"
    "a_playPackSound = function() end\n"
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
    "l_resetTime = function() levelTime = 0 end\n"
    "l_getLevelTime = function() return levelTime end\n"
    "l_addTracked = function() end\n"
    "l_clearTracked = function() end\n"
    "l_setIncEnabled = function() end\n"
    "e_messageAddImportant = function() end\n"
    "e_stopTimeS = function() end\n"
    "e_kill = function() killed = true end\n"
    "levelTime = 0\n"
    "killed = false\n"
    "function shuffle(t)\n"
    "  for i = #t, 2, -1 do local j = math.random(i) t[i], t[j] = t[j], t[i] end\n"
    "end\n";

static int run(lua_State *L, const char *what, const char *code, int results) {
    if (luaL_loadbuffer(L, code, strlen(code), what) != LUA_OK ||
        lua_pcall(L, 0, results, 0) != LUA_OK) {
        fprintf(stderr, "K3-DRIVER: %s failed: %s\n", what, lua_tostring(L, -1));
        return 1;
    }
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr,
                "usage: %s <level.lua> [--iters N] [--dt s] [--jit-off] "
                "[--report N] [--watchdog s]\n",
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
    }

    lua_State *L = luaL_newstate();
    luaL_openlibs(L);
    printf("K3-DRIVER: %s, LUA_VERSION=%s\n", LUAJIT_VERSION, LUA_VERSION);
    if (jit_off) {
        if (run(L, "jit.off", "jit.off()", 0)) return 4;
        printf("K3-DRIVER: JIT OFF (interpreter only)\n");
    }
    if (run(L, "stubs", STUBS, 0)) return 4;

    // Load the real level script (u_execDependencyScript is stubbed, so
    // the three dependency includes are no-ops; everything else resolves).
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

    for (long i = 1; i <= iters; i++) {
        g_level_time += g_dt;
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

        // The tutorial completes (e_kill) when levelTime passes 45 s —
        // stop early like the game would.
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
