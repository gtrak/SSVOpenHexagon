# lib-web-stack — the wasm/web platform stack (Phase 0 scope, issue 26)

This skill records the **wasm/web substrate** as far as **Phase 0** of issue 26
(`.agents/plans/001-rust-port/26-wasm-web.md`) established it on this box
(2026-10-16). It is a **toolchain/compatibility** record, not a crate skill: it
states which targets exist, what cross-compiles, and which C dependencies need a
wasm C/C++ toolchain. The Rust platform stack itself (`winit`+`glutin`+`glow`+
`rodio`+`fontdue`) is documented in `.agents/skills/lib-render-stack.md`; this
skill records only the web-side deltas and the Phase 0 evidence.

**Scope discipline:** items that Phase 0 has NOT yet exercised are marked
**"pending P0.3"** (the web/browser rendering side) explicitly — do not treat
them as verified.

## Targets and local tooling (measured on this box, 2026-10-16)

- `rustup target list --installed`: **`wasm32-unknown-unknown`**,
  **`wasm32-wasip1`**, `x86_64-pc-windows-msvc`. (Both wasm targets are present.)
- **`trunk` 0.20.3** is installed (the static-site build/serve tool the plan's
  "Build/ship shape" names).
- **`wasm-bindgen` and `wasm-pack` are NOT installed** (command-not-found).
- **node v24.13.0** is installed (the `node:wasi` host is available for a
  wasip1 smoke).
- **`wasmtime` / `wasmer` are NOT installed** (no standalone wasm runtime here;
  browser checks are the execution path, via `agent-browser`).
- **`agent-browser` 0.39.1** is available for browser checks (the P0.3/P0.4
  execution vehicle once a page is served).

## The two targets (what each can and cannot do)

- **`wasm32-unknown-unknown`** — no libc/C standard library. Any C/C++
  dependency that links a C runtime or a C++ standard library **cannot** build
  here without **emscripten/emsdk** (which supplies its own libc/libcxx). This
  is the target the plan's "Build/ship shape" names (`wasm-bindgen` + `web-sys`
  + `trunk`), but it is the one where the C/C++ dependencies break (below).
- **`wasm32-wasip1`** — has **wasi-libc** (a C runtime) but **no C++ standard
  library** (no libc++). C deps that only need libc (zlib) can build here given
  a wasi C compiler; C++ deps (Luau) need **wasi-sdk** (clang++ + wasi-libc +
  libc++). This is the viable path for the Luau gate (D-028a) — see
  `.agents/skills/lib-mlua.md` §WASM.

## Rust-core cross-compile result (P0.2, measured)

`cargo check --target wasm32-unknown-unknown -p <crate>`:

| crate | result | why |
|---|---|---|
| `oh-math`, `oh-rng`, `oh-data`, `oh-config` | **PASS** (all four clean) | pure Rust, no C deps |
| `oh-replay` | **FAIL** | `libz-sys` build script: `failed to find tool "clang"` — the zlib C build needs a wasm C compiler (the D-028b substitution point; web uses miniz_oxide — see `.agents/skills/lib-flate2.md`) |
| `oh-lua` | **FAIL** | (1) `getrandom 0.2.17` `compile_error!` "the wasm\*-unknown-unknown targets are not supported by default, you may need to enable the `js` feature" (`getrandom-0.2.17/src/lib.rs:346`), reached via `oh-lua → oh-game → oh-discord → discord-rich-presence 1.1.0 → uuid 0.8.2 → getrandom 0.2.17`; (2) `libz-sys` clang (as `oh-replay`); plus a latent (3): `lua-src`/`luajit-src` (LuaJIT C) would need a wasm C compiler |

## C-toolchain requirement matrix (C deps that do not cross-compile by default)

| C dependency | crate path | needs, on `wasm32-unknown-unknown` | needs, on `wasm32-wasip1` |
|---|---|---|---|
| zlib | flate2 `zlib` → **libz-sys 1.1.30** (`build.rs` `cfg.compile("z")`) | a wasm C compiler (clang) — or substitute **miniz_oxide** (pure Rust; the D-028b choice) | a wasi C compiler (wasi-sdk clang); libc present |
| LuaJIT | mlua `luajit`+`vendored` → **luajit-src 210.5.12+a4f56a4** (C) | emscripten/emsdk | wasi-sdk (C); LuaJIT is 2.1 and historically no-wasm |
| Luau | mlua `luau` → **luau0-src 0.12.3+luau663** (C++) | **emscripten/emsdk** (needs a C++ stdlib; libc++/libc absent) | **wasi-sdk** (clang++ + wasi-libc + libc++); see `.agents/skills/lib-mlua.md` §WASM |
| libsodium | sodiumoxide 0.2.7 → **libsodium-sys** (C, autoconf) | emscripten/emsdk (autoconf cross); the D-004 sodium entry's `--host=<triple>` rejection is the same class of blocker | wasi-sdk; **but** the net stack is gated OFF on web (see below) |
| SQLite | rusqlite `bundled` → **libsqlite3-sys** (C) | emscripten/emsdk | wasi-sdk; **but** the DB is server-side — gated OFF on web |

**Net/DB/Steam/Discord are feature-gated OFF on the web build** (the plan's
blocker #5: sodiumoxide/libsodium-sys, rusqlite bundled, `std::net` do not
build/behave on wasm). The default-off pattern from Steam/Discord is the
precedent — so the libsodium + SQLite rows above are moot for the web build and
are listed only for completeness of the matrix.

## The render stack on web (carried from `lib-render-stack.md`)

- **`winit` 0.30.13** has a **wasm32 backend** (`web-sys` /
  `EventLoopExtWebSys`) — the event-loop shape ported in 25b's
  `ApplicationHandler` is the same (`lib-render-stack.md` pins winit 0.30.13,
  `Cargo.lock:2643-2646`).
- **`glow` 0.18.0** supports **WebGL2** (the same `glow::Context` API over
  `WebGl2RenderingContext`). Our renderer is glow-only; the web path has **no
  glutin** (glutin does not support wasm). The 25c GLES shader translation
  (`translate_for_gles`, issue 25b SG3) is likely reusable (WebGL2 ≈ GLES 3.0).
- **`fontdue` 0.9.4** is pure Rust — the text pipeline cross-compiles.
- **Audio**: rodio/cpal have no wasm backend (the plan's blocker #6); v1 may
  ship silent + documented (D-028c).

### P0.3 — web render path (verified)

Executed 2026-10-16 in throwaway crates under
`C:\Users\garyt\AppData\Local\Temp\opencode\` (outside the repo; no repo source
touched). Note: the standalone `wasm-bindgen`/`wasm-pack` CLIs are still absent,
but that is **not** a blocker — `trunk` bundles and drives `wasm-bindgen`
itself, so both spikes built fine through `trunk build`.

**Plain path — VERIFIED end-to-end (built + run in a real browser).** Spike
crate `oh-web-spike` (`C:\Users\garyt\AppData\Local\Temp\opencode\oh-web-spike`,
`cdylib`): deps `wasm-bindgen 0.2.129`, `js-sys 0.3.106`, `web-sys 0.3.106`
(features `Window`/`Document`/`Element`/`HtmlElement`/`HtmlCanvasElement`/
`WebGl2RenderingContext`/`console`), `glow 0.18.0`.
- `cargo build --target wasm32-unknown-unknown` → clean; `trunk build` →
  success (emits `dist/`).
- `src/lib.rs` (`#[wasm_bindgen(start)]`) grabs `#canvas`, gets a
  `WebGl2RenderingContext` via `canvas.get_context("webgl2")`, wraps it in
  `glow::Context::from_webgl2_context`, then `clear_color(0.1,0.4,0.8,1.0)` +
  `clear`; `index.html` carries `<canvas id="canvas" width="640" height="480">`
  + a `#status` div.
- **Browser check (supervisor, `agent-browser` on Chromium):** `#status` read
  `WebGL2 OK — WebGL 2.0 (OpenGL ES 3.0 Chromium) — cleared 640x480 to
  rgb(26,102,204)`; console clean (one benign deprecation warning, no errors);
  screenshot (`p0_3.png`) shows the blue-cleared 640×480 canvas.

**winit-web backend — VERIFIED live (built + run in a real browser).** Spike
crate `oh-web-winit-spike`
(`C:\Users\garyt\AppData\Local\Temp\opencode\oh-web-winit-spike`): adds
`winit = "0.30"` (resolves to **winit 0.30.13**) on the same `web-sys`/`glow`
set. `src/lib.rs` builds an `EventLoop`
(`EventLoop::with_user_event().build()`), and in the `ApplicationHandler`
`resumed` hook (the web loop emits `NewEvents(Init)` then `Resumed` at start)
creates a `Window` bound to the page's `#canvas` via
`WindowAttributesExtWebSys::with_canvas(Some(dom_canvas))`, pulls the canvas
back with `WindowExtWebSys::canvas()`, and clears through `glow`; the loop is
started with `EventLoopExtWebSys::spawn_app(app)` (returns immediately — **no**
blocking loop). All web-specific traits live in
`winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys,
WindowExtWebSys}` (not `winit::event_loop`).
- `cargo build --target wasm32-unknown-unknown` → **PASS** (winit 0.30.13 +
  deps cross-compile clean); `trunk build` → **success** (emits `dist/` with
  `index.html` + `*_bg.wasm` + `*.js`).
- **Browser check (supervisor, `agent-browser` on Chromium):** the served page's
  `#status` read `winit-web OK — WebGL 2.0 (OpenGL ES 3.0 Chromium) — cleared
  640x480 to rgb(26,102,204)`; the console had **no errors**; the screenshot
  shows the blue-cleared 640×480 canvas. The winit 0.30 web windowing layer is
  therefore **verified live**.

Net: the former "pending P0.3" item is now fully resolved — **both** web render
paths are **verified live** in a real browser: the plain `wasm-bindgen`+`glow`
WebGL2 path (direct `WebGl2RenderingContext`) **and** the `winit` 0.30 web
windowing layer (`EventLoopExtWebSys::spawn_app` + a `Window` bound to the page
canvas via `WindowAttributesExtWebSys::with_canvas`). P0.3 is complete.

## 26c.1 — Emscripten target (verified)

**Issue 26c.1 spike = PASS** (supervisor-verified live in Chromium via
`agent-browser`, 2026-10-16): a **`wasm32-unknown-emscripten`** module boots in a
real browser, creates a **WebGL2** context, constructs a working **`glow`**
context, runs a `request_animation_frame` loop, and exposes a callable
`#[wasm_bindgen]` export. Browser console:

```
emscripten: webgl2 create handle=1 make_context_current->0 (0=ok)
(a) raw webgl2: clear blue (0.1,0.4,0.8) OK
(b) glow: clear orange (204,102,26) OK -> status()=glow-ok
render loop started (request_animation_frame)
status glow-ok
```

Screenshot: the 640x480 canvas renders orange. Artifacts:
`oh_web_emscripten_spike.js` (266 KB) + `.wasm` (7.5 MB debug).

**Architecture decision (D-029):** the web build targets
`wasm32-unknown-emscripten` for the WHOLE app (Lua + engine + `web-sys` +
`glow` in ONE module), so the Lua VM and the engine share a module with direct
calls (no cross-module bridge). **`winit` is dropped on the web path** — see
the winit exclusion below and DECISIONS.md D-029.

**Evidence (hard):**
- `winit 0.30.13` cannot build for emscripten: `build.rs:11` gates
  `web_platform = { all(target_family = "wasm", target_os = "unknown") }`;
  emscripten is `target_os="emscripten"` →
  `compile_error!("The platform you're compiling for is not supported by
  winit")`. (So winit-web is **`wasm32-unknown-unknown`-only**.)
- `wasm-bindgen` NOW supports `wasm32-unknown-emscripten` (current docs; the
  old sunset-domain page said "no plans" and is STALE) — it must be a **`bin`
  crate** (a `cdylib` links as an Emscripten *side module* with no JS loader →
  undefined `__wbindgen_*`) plus **`-Clink-arg=-sWASM_BINDGEN`**; `emcc` then
  runs the `wasm-bindgen` CLI itself over the linked wasm. Requires Emscripten
  ≥ 6.0.10.
- `glow 0.18` on emscripten compiles its **`native`** module (not `web_sys`),
  so `Context::from_webgl2_context` is unavailable; the working route is
  `glow::Context::from_loader_function_cstr(|n: &CStr|
  emscripten_webgl_get_proc_address(n.as_ptr()))` after creating + making
  current an emscripten WebGL2 context.
- `mlua`-Luau requires emscripten (26b PASS).

**Exact 26c.1 build recipe (verbatim).** Crate is a **binary** (`src/main.rs`,
no `[lib]`) with `rust-toolchain.toml` pinning `channel = "1.97.1"`.
`.cargo/config.toml`:

```toml
[build]
target = "wasm32-unknown-emscripten"

[target.wasm32-unknown-emscripten]
# emsdk 6.0.x ships emcc.exe (no emcc.bat); point the linker at it explicitly.
linker = "emcc.exe"
rustflags = [
  "-Cpanic=abort",
  "-Cllvm-args=-enable-emscripten-cxx-exceptions=0",
  "-Crelocation-model=static",
  "-Clink-arg=-sWASM_BINDGEN",
  "-Clink-arg=-Wno-experimental",
  "-Clink-arg=-sMODULARIZE",
  "-Clink-arg=-sEXPORT_ES6",
  "-Clink-arg=-sMAX_WEBGL_VERSION=2",
  "-Clink-arg=-sMIN_WEBGL_VERSION=2",
]
```

Deps: `wasm-bindgen = "0.2.129"`, `js-sys = "0.3.106"`,
`web-sys = { version = "0.3.106", features = ["Window","Document","Element",
"HtmlElement","HtmlCanvasElement","WebGl2RenderingContext","console",
"Performance"] }`, `glow = "0.18"`.

**Two gotchas that cost iterations (record them):**
1. `-sMAX_WEBGL_VERSION=2` (and `-sMIN_WEBGL_VERSION=2`) are REQUIRED — without
   them emscripten logs `WebGL 2 requested but only WebGL 1 is supported` and
   `emscripten_webgl_create_context` returns 0.
2. `emscripten_webgl_create_context` takes a **CSS selector**, not a bare
   element id: `"#c"` works, `"c"` returns 0 (it does `querySelector("c")`).

Also: the WebGL2 context must be created via the emscripten C API
(`emscripten_webgl_init_context_attributes` with `major_version=2` →
`emscripten_webgl_create_context` → `emscripten_webgl_make_context_current`)
BEFORE `emscripten_webgl_get_proc_address` resolves (it dispatches off
emscripten's current `GLctx`).

**emsdk env (every build):** `EMSDK=<emsdk>`, `EM_CONFIG=<emsdk>\.emscripten`,
`CC/CXX/AR_wasm32_unknown_emscripten` = `emcc.exe`/`em++.exe`/`llvm-ar.exe`,
and `PATH` must include emsdk's `upstream\emscripten`, `upstream\bin`, and
`~/.cargo/bin` (for the `wasm-bindgen` CLI). emsdk 6.0.12 / node 24.19.0.

**Implications (not blockers):**
- Emscripten provides a **libc + in-memory FS**, so `std::fs`/`std::time`/
  `std::env` actually WORK on this target (content could be preloaded into
  MEMFS). The plan's fetch→`MemoryVfs` remains the intended content path, but
  `RealFs` is not automatically excluded on emscripten.
- **Open risk to verify in 26c.3:** the wasm-bindgen recipe's
  `-Cllvm-args=-enable-emscripten-cxx-exceptions=0` (needed for Rust
  `panic=abort`) may CONFLICT with Luau's C++ exceptions
  (`CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions`, mandatory per 26b)
  when both are linked into ONE module — must be tested, not assumed.
- Minor spike artifact: `main` ran twice in the spike (two context handles) —
  to resolve when structuring `oh-web`.

## 26c.2 — Luau + wasm-bindgen + glow in one emscripten module (verified)

**Issue 26c.2 spike = PASS** (supervisor-verified live in Chromium via
`agent-browser`, 2026-10-16): **Luau (mlua, vendored C++) +
`wasm-bindgen`/`web-sys` + `glow` all link AND run in ONE
`wasm32-unknown-emscripten` module.** Browser console:

```
emscripten: webgl2 create handle=1 make_context_current->0 (0=ok)
(a) raw webgl2: clear blue (0.1,0.4,0.8) OK
(b) glow: clear orange (204,102,26) OK -> status()=glow-ok
(c) luau: acc=9 d=42 q=3 -> status() includes luau-ok
render loop started (request_animation_frame)
status glow-ok;luau-ok;acc=9 d=42 q=3
```

Canvas renders (orange). This **confirms D-029's single-module premise** (Lua +
engine in one module, no cross-module bridge). Artifacts (debug): `.js` ≈ 317 KB,
`.wasm` ≈ 19.5 MB.

**The two load-bearing integration facts (non-obvious; will bite `oh-web`):**

1. **`-Clink-arg=-sDEFAULT_TO_CXX` is REQUIRED to link.** emcc's linker driver
   does NOT auto-link the C++ exception runtime (libcxxabi) for the mixed
   Rust+C++ module, so Luau's objects (built with `-fwasm-exceptions`) leave
   `__gxx_wasm_personality_v0`, `__cxa_throw`, `__cxa_begin_catch`,
   `std::terminate`, `typeinfo for std::exception`, `operator delete`…
   undefined. emcc's own hint says to pass `-sDEFAULT_TO_CXX`. (Linking with
   `-fwasm-exceptions` is NOT needed — emscripten 6.0.x's prebuilt libcxxabi
   already provides those symbols.)
2. **CRITICAL init-order bug — the actual cause of a Luau runtime trap, and the
   fix.** With `-sWASM_BINDGEN`, the wasm-bindgen glue registers Rust `main` via
   `addOnInit(initBindgen)`, and emscripten's `initRuntime()` runs the onInit
   callbacks **BEFORE** `wasmExports['__wasm_call_ctors']()`. So Rust `main` — and
   therefore Luau's `Lua::new()` — runs **before C++ static initializers**. Luau's
   `kSizeClassConfig` (a `const` global with a non-trivial constructor,
   `lmem.cpp:192`) is then zero-initialized → `blockSize == 0` → an integer
   **divide-by-zero trap** in `newclasspage` (`lmem.cpp:285`) via
   `lua_newstate` → `Lua::new()`. It is a wasm trap, so it appears as a SILENT
   halt (no Rust `Err`) — which is why it was hard to spot. **FIX:** Rust owns the
   entry point here, so call the C++ static initializers explicitly:
   `extern "C" { fn __wasm_call_ctors(); }` and call `__wasm_call_ctors()` as the
   FIRST statement of `main()`. After that, Luau runs. (Reproduced identically
   under node: `RuntimeError: divide by zero at newclasspage … lua_newstate`.)
   - The `-Cllvm-args=-enable-emscripten-cxx-exceptions=0` flag is NOT the
     culprit — it only affects Rust codegen and **coexists** with Luau's
     `-fwasm-exceptions`.
3. Minor spike artifact (note for `oh-web`): `main` ran twice (two context
   handles) in the spike; resolve when structuring `oh-web` (ensure a single
   init).

**Final minimal working `.cargo/config.toml` (record verbatim):**

```toml
[build]
target = "wasm32-unknown-emscripten"

[target.wasm32-unknown-emscripten]
linker = "emcc.exe"
rustflags = [
  "-Cpanic=abort",
  "-Cllvm-args=-enable-emscripten-cxx-exceptions=0",
  "-Crelocation-model=static",
  "-Clink-arg=-sDEFAULT_TO_CXX",
  "-Clink-arg=-sWASM_BINDGEN",
  "-Clink-arg=-Wno-experimental",
  "-Clink-arg=-sMODULARIZE",
  "-Clink-arg=-sEXPORT_ES6",
  "-Clink-arg=-sMAX_WEBGL_VERSION=2",
  "-Clink-arg=-sMIN_WEBGL_VERSION=2",
]
```

Deps include `mlua = { version = "0.10", features = ["luau","vendored"] }`;
env `CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions`; `cargo clean -p
mlua-sys` once after changing that env.

## Phase 0 status (2026-10-16)

- **P0.1 (replay compression interchange): PASS** — miniz_oxide ↔ zlib
  lossless; see `.agents/skills/lib-flate2.md`.
- **P0.2 (core cross-compile + Luau gate): core PASS; Luau gate =
  toolchain-gated.** The four pure-Rust crates cross-compile clean; the Luau
  go/no-go is blocked on a wasm C++ toolchain (wasi-sdk for `wasm32-wasip1`, or
  emscripten for `wasm32-unknown-unknown`) — see `.agents/skills/lib-mlua.md`
  §WASM and DECISIONS.md D-028a.
- **P0.3: web render path VERIFIED (both paths, live)** — the plain
  `wasm-bindgen`+`glow` WebGL2 path **and** the `winit` 0.30 web windowing layer
  (`EventLoopExtWebSys::spawn_app`, a `Window` bound to the page canvas via
  `WindowAttributesExtWebSys::with_canvas`), each browser-checked via
  `agent-browser` on a served page (see §"P0.3 — web render path (verified)").
  **P0.4 (VFS contract inventory): COMPLETE** — the asset/config filesystem
  inventory + the minimal `oh-vfs` contract (asset/configuration provider, not a
  general filesystem) is specified in
  `.agents/tasks/issue-026a-oh-vfs.md` (the Phase 1 / 26a prerequisite). The
  **replay corpus run** against native vs web impls (the original P0.4 body) is
  still pending.
