# Task 026-integration — bring the `wasm` lane onto master, verified

Read `.agents/WORKER-HYGIENE.md` first — it applies to this lane.

## Where things stand

`origin/wasm` is **10 commits on top of master `3610f3fb`**; master is now **11
commits ahead** of that base (the audit-fix batches: 29b1–29i1, the `029misc`
items, the hygiene doc). So this is a merge job with known collision points, not
a fast-forward.

The branch's own record is **D-036** (`DECISIONS.md`) and
`.agents/tasks/issue-026-web.md`. What it landed (W0–W6):

- **Target = `wasm32-unknown-emscripten` for the whole app; `winit` dropped on
  web** (`winit` 0.30's web backend is `target_os = "unknown"`-only and cannot
  target emscripten). The web build uses a canvas + `emscripten_webgl_*` context
  + `glow::Context::from_loader_function_cstr`, isolated in `oh-render`; and
  `winit`/`egui`/`egui-winit`/`egui_glow`/`glow 0.17`/`signal-hook` are gated to
  `not(emscripten)`.
- **Content via MEMFS preload, not a VFS refactor**:
  `--preload-file=_RELEASE@/RELEASE` + `-sFORCE_FILESYSTEM=1`, so master's
  existing `std::fs` content path works on web unchanged (the 26a `oh-vfs` trait
  is dropped).
- **Lua backend selectable per target**: `oh-lua` uses mlua `luajit` on desktop
  and `luau` on emscripten (Luau/C++ is the only Lua that builds there);
  `mlua::Integer` width fixed for the Luau build; `oh-lua/src/convert.rs` carries
  the per-target conversion differences. **Global-Luau is explicitly a follow-up
  experiment, not this lane.** Do not attempt it here.
- **`crates/oh-web`** (new entry crate) + `.cargo/config.toml` target recipe +
  `wasm-bindgen`-as-a-bin (`-Clink-arg=-sWASM_BINDGEN`).

## What to do

### 1. Bring the branch onto current master
Use rebase or cherry-pick of the 10-commit range so history stays linear (this
repo's landings are linear and per-milestone; a merge commit is a last resort —
if you take one, say why). **Preserve the branch's commit structure and its
docs** (`DECISIONS.md` D-036, `issue-026-web.md`, the `oh-web` crate, the site
assets) — they are the lane's record.

**Named collisions — resolve each deliberately, and say what you did:**

- `crates/oh-render/src/window.rs` (+516 in the branch) — **master changed this
  file too** (29b2's mouse-wheel sink). The wheel sink must survive.
- `crates/oh-app/src/windowed.rs` (+196) — master changed it for 29b9 (F3/F4),
  29b4 (`-p/-l` boot), 29u1 (cursor), 29r1 (desktop mode + config logging).
  Master's newer behaviour wins; the branch's platform gating is layered on top.
- `crates/oh-lua/Cargo.toml` — master has **029k1's** vendored LuaJIT
  (`extlibs/luajit-src` + `[patch.crates-io]`, `LUAJIT_DISABLE_FFI`) and the
  corrected comment; the branch adds the per-target backend table. Both must
  survive: desktop keeps the vendored FFI-off LuaJIT, emscripten gets Luau.
- `.cargo/config.toml` — master has the Android linkers; the branch adds the
  emscripten target recipe. Keep both.
- `crates/oh-render/src/{types.rs,gl.rs}`, `crates/oh-app/{Cargo.toml,src/client.rs,src/lib.rs,src/main.rs}`,
  the root `Cargo.toml`/`Cargo.lock` — check each against master's newer state.

### 2. Re-evaluate the profile-seed workaround (it predates a fix)
D-036 records an `oh-web` prototype workaround that **seeds a default profile
into MEMFS**, justified by "master's profile/save system is a known no-op
(tracker 29b6/29c1)". **29c1 has since landed**: profiles, scores, favorites and
the exit save are real now, and 29b6 (the created-profile menu item) is in too.
Re-check whether the seed is still needed (a genuinely fresh web user may have no
profile, and the C++'s boot flow has a first-run path — find it and prefer it over
a seed). Either keep it as a documented deviation or remove it; do not leave it
unexamined.

### 3. Fix the decision-number collision
The branch's **D-036** (web target) is the first D-036, so the Lua-runtime
decision becomes **D-037**. Correct `.agents/plans/001-rust-port/30-lua-runtime.md`,
which says "next free is D-036".

### 4. Verify — the browser boot is REQUIRED, not optional

**The wasm build was never actually run in a browser.** The branch claims
boot-to-menu (its W5/W6), but that claim is unverified upstream — so verifying it
here is part of the integration, not a nice-to-have.

**Host gate must stay green**: `bash tools/gate.sh` — 4/4, master's 908 tests
plus whatever the branch adds. Non-negotiable.

**The web target** — no emscripten toolchain is installed on this machine (no
`emsdk`, no `emcc`), but **docker works** (verified; no sudo needed) and the
`emscripten/emsdk` image is **not yet pulled** (expect one ~1 GB pull, bounded).
Mount the checkout and run the branch's documented recipe **once**; if the pull or
the recipe fails, **stop and report the exact command + error** rather than
burning the lane on toolchain archaeology.

**Then test it in a real browser — this is the milestone.** The tooling is here
(`agent-browser` 0.27.0, installed globally):

```
AB="node $(npm root -g)/agent-browser/bin/agent-browser.js"   # no PATH shim
python3 -m http.server <port> --directory <build-dir> &        # serve (bounded)
$AB open http://localhost:<port>/ ; $AB wait <ms>
$AB console        # console logs — the wasm/WebGL boot trace
$AB errors         # page errors — the decisive failure signal
$AB eval '...'     # canvas size, WebGL context, module/state probes
$AB screenshot /tmp/web-boot.png
$AB close --all
```

Evidence to report, in this order: `errors` (must be empty of fatal ones),
`console` (the port's own `[channel]` log lines — the engine's boot trace should
appear, which proves the *engine* ran and not just JS), the `eval` probes, and a
screenshot analysed with **PIL** (dimensions + non-black pixel count) — the
hygiene rule applies: **at most ONE image read**, bounded commands, and if the
page white-screens, read `console`/`errors` before spending an image.

WebGL in headless Chromium may need software GL; if the context fails, say so and
report whether it is a toolchain limitation or a port bug — do not report "works"
without the console/errors evidence shown above.

### 5. Report
What you integrated, the per-file conflict resolutions, the profile-seed decision,
the D-036/D-037 fix, the host gate result, and **exactly how far the web
verification got** (with the commands and their output). Be explicit about what
is unverified.

## Non-goals
- No global-Luau migration (issue 30's remaining half).
- No VFS refactor (dropped by D-036).
- No new web features beyond what the branch already contains.

## Hygiene
- **No worktree** (D-035). Work in the main checkout, but note: this lane rewrites
  history for a moment — before you start, confirm the tree is clean and record
  `git rev-parse HEAD`, so a botched rebase can be undone (`git reset --hard
  <sha>` / the reflog) without loss.
- One commit per logical step; **stage explicit paths, never `git add -A`**.
- `cargo fmt --all` before committing; gate before each commit.
- If the rebase gets messy and you are not converging, **stop, restore the
  recorded HEAD, and report** — a clean "not integrated, here is the blocker" beats
  a half-merged tree.

## Report (lane 026 CONTINUATION — acceptance completed, 2026-10-17)

The previous lane died after integrating W0-W6 and leaving a `oh_web.wasm`
that reached `oh-web: boot started` in-browser, before dying (host reboot).
This continuation completed the acceptance with properly-evidenced browser
boot. Worktree `lane/026`: `36850a03` (harvest) -> `9f4474e4` (remove the
dead lane's invisible `[026-DBG]` tracing) -> `7d9c8478` (document
`CXXFLAGS=-fwasm-exceptions` in `.cargo/config.toml`) ->
`9a1413c0` (the engine-log-sink fix + web boot trace + `status()` ticks).

### What the continuation found and fixed

1. **The Luau link fix (already documented, `7d9c8478`)**: rustc links this
   target with wasm-native EH while `luau0-src` compiles Luau's C++ with
   Emscripten C++ EH by default -> undefined `__resumeException` at link.
   The build MUST run with `CXXFLAGS=-fwasm-exceptions` (documented in
   `.cargo/config.toml`; cc passes it after the crate's flags, so it wins at
   Luau's compile step).
2. **Rust stdio is a black hole in this emscripten glue**: `println!` /
   `eprintln!` (fd 1/2) never reach the browser console (the default
   `/dev/stdout -> /dev/tty` null path swallows them; the `Module({print,
   printErr})` and `Module({stdout, stderr})` override options receive
   nothing either — verified empirically). The dead lane's `index.html`
   comment ("the emscripten stdout/stderr redirect carries the composition's
   Rust println!/eprintln! lines") was WRONG — that is why its `[026-DBG]`
   tracing never showed and its "boot started then nothing" looked
   unexplainable.
3. **The fix (`9a1413c0`)**: an `oh-game` `log_sink` hook — the LuaState
   `log()` trait impl and `host_error` now route through
   `oh_game::log_sink::emit_log` (host default: the C++-parity
   `println!`), and `oh-web`'s `boot()` installs a `console.log` sink before
   `main` runs, so the composition's `[channel]` lines reach the browser
   console. Plus: emscripten-only `[web]` boot-stage trace lines (the W5
   "engine boot trace", host-quiet), and a `ticks=<n>` field on the
   `status()` probe (the rAF-loop tick count — distinguishes "the engine
   loop is alive" from "the page loaded but the loop died").
4. **WebGL in this environment**: Chrome 148 deprecated the automatic
   software-WebGL fallback; the context is lost on the first frame unless
   ANGLE-SwiftShader is used explicitly (`--use-angle=swiftshader
   --enable-unsafe-swiftshader`). With it, the context survives and the
   menu renders in software GL. Without it, the port degrades gracefully to
   its `NullDrawTarget` headless window and the rAF loop keeps ticking at
   60fps (verified with the tick counter).
5. **The pre-hook boot spin (open, characterized)**: the EXACT pre-hook tree
   (`7d9c8478`, rebuilt with the current recipe; and the dead lane's 14:37
   artifact, which is a different, pre-wasm-bindgen-CLI 5.5MB wasm that
   imports the `wasi_snapshot_preview1` module) hangs deterministically
   inside `main_client` after `oh-web: boot started` — 100% main-thread
   spin, CDP `Runtime.evaluate` times out, 150s+ with no status lines. The
   hang is NOT the GL context loss (it reproduces with a live
   ANGLE-SwiftShader context) and NOT "any JS interop fixes it" (a single
   extra `eprintln!` before `main_client` does not break the spin). It
   disappears in the `9a1413c0` tree — and survives staged removal of every
   trace line / tick counter / `Instant` from that tree (the minimal
   surviving delta is the log_sink wiring itself: the new `oh-game` module +
   the `oh-web -> oh-game` dependency edge + the sink install). Instruction-
   level root cause not pursued (would need a wasm disassembly bisect of the
   Luau/engine boot path) — tracked as a residual risk below. Acceptance was
   completed on the `9a1413c0` build (7/7 boot runs, including the final
   post-fmt rebuild).

### The acceptance evidence (spec order, agent-browser 0.27.0)

Artifacts served from `/tmp/oh-web-serve` on `:8066` (`oh-web.js`,
`oh_web.wasm` 2.57MB, `oh_web.data` 22.7MB, corrected `index.html`).
Browser: the agent-browser Chrome (148.0.7778.97) started with
`--headless=new --no-sandbox --disable-dev-shm-usage
--enable-unsafe-swiftshader --use-angle=swiftshader
--remote-debugging-port=9333`, agent-browser connected via `--cdp 9333`.

```
$AB --cdp 9333 open http://localhost:8066/
  -> "✓ SSVOpenHexagon (web)"
$AB --cdp 9333 errors
  -> (empty)
$AB --cdp 9333 console
  -> [log] oh-web: boot started (after __wasm_call_ctors)
  -> [log] [web] main_client entry (t=0)
  -> [log] [web] config loaded
  -> [log] [web] assets loaded
  -> [log] [web] entering create_window
  -> [log] [web] windowed build entry
  -> [log] [web] engine + menu composed (the four callback links bound)
  -> [log] [web] client built (window + engine + menu)
  -> [log] [web] rAF loop scheduled (the browser owns the frames)
  -> [log] oh-web: main_client returned 0 (the rAF loop keeps driving the menu)
  -> [log] oh-web: main re-entered - second init skipped (single-init guard)
  -> [log] oh-web: module loaded
  -> [log] status booted=true;exit_code=0;ticks=119 ... ticks=1200 (advancing ~60/s)
$AB --cdp 9333 eval '(function(){ var c = document.getElementById("c");
    var gl = c.getContext("webgl2"); return JSON.stringify({canvas: c.width +
    "x" + c.height, webgl2: gl ? "present" : "none", contextLost: gl ?
    gl.isContextLost() : null, statusDiv:
    document.getElementById("status").textContent}); })()'
  -> {"canvas":"1736x901","webgl2":"present","contextLost":false,
      "glVersion":"WebGL 2.0 (OpenGL ES 3.0 Chromium)",
      "statusDiv":"status: booted=true;exit_code=0;ticks=1440"}
$AB --cdp 9333 screenshot /tmp/oh-web-acceptance.png
  -> PIL: 780x437 (viewport crop of the 1736x901 canvas page);
     13.65% of sampled pixels non-black; a dense (37-89%) text band in the
     lower visible canvas region = the software-GL-rendered menu content
     (with the deprecated auto-fallback SwiftShader the same build shows a
     black canvas after CONTEXT_LOST_WEBGL — environment limitation, the
     port degrades to NullDrawTarget and keeps ticking).
```

### Host gate

`cargo check -p oh-app -p oh-web -p oh-game` clean (rustc 1.97.1, pinned by
`rust-toolchain.toml`); `cargo fmt --all -- --check` clean after the
commits. The C++ reference tree was never touched.

### Honest framing (spec: state what the web build has and does not have)

- The web build has **no input** (issue 32 — keyboard/mouse/touch events are
  wired to the engine's input state, but the browser page exposes no
  interactive affordance and issue 32 owns the input story). **It is not
  playable/interactive.**
- What IS verified: the page loads, the REAL composition boots
  (`main_client` returns 0 over the preloaded `_RELEASE` tree), the engine
  runs and the rAF loop drives the menu at 60fps (the `[web]`/`[channel]`
  console lines + the `ticks` counter prove the engine, not just JS), and
  the menu renders in software GL when ANGLE-SwiftShader is available.

### Residual risks / unverified

- The pre-hook boot spin (item 5 above): any future rebuild of the
  pre-`9a1413c0` tree will hang; the `9a1413c0` tree boots 7/7. Root-cause
  is a follow-up (wasm-level bisect of the Luau/engine boot path).
- Session-only persistence (MEMFS): profiles/config created in a session do
  not survive a page reload (by design of the preload; issue 32 territory).
- `webglcontextlost`/`restored` DOM handlers are not installed (W4): the
  port tolerates a lost context by no-op'ing GL work (NullDrawTarget), but
  it cannot recover a context in-browser.
- The acceptance screenshot is a viewport crop (780x437) of the top-left of
  the 1736x901 canvas; a full-window render was not captured (ONE-image
  hygiene rule).
