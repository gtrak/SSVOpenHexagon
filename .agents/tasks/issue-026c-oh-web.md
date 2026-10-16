# Task 026c — the `oh-web` crate (Phase 2 / lane 26c) — the playable vertical slice

Phase 2 of issue 26 (`.agents/plans/001-rust-port/26-wasm-web.md`, the
"playable vertical slice" phase; the plan's Phase 2, exit "a real gameplay loop
works, not merely `cargo check`"). This is the **`oh-web` crate** — a
`wasm32-unknown-emscripten` **binary** that boots the real engine in a browser:
content, scripts, render, input, and a **representative gameplay state**. The
deliverable is the D-028 decisive milestone: **load a level, run its Lua,
render, take input, and reach a representative gameplay state in a real
browser** — verified via `agent-browser`, not `cargo check`.

**Architecture is ALREADY DECIDED (D-029) — do not re-decide it:** the whole web
app targets **`wasm32-unknown-emscripten`** — Lua + engine + `web-sys` + `glow`
in **ONE module** (the Lua VM and the engine share a module with direct calls,
no cross-module bridge). **`winit` is dropped on the web path** (its event loop
is a plain `request_animation_frame`; `winit 0.30.13` cannot build for
emscripten — see §1). This supersedes the 26 plan's original
`wasm32-unknown-unknown` + winit "Build/ship shape".

Port discipline (literal 1:1, determinism-first) applies double here: the browser
is the one platform where our byte-exactness guarantees are at risk from the
substrate itself. The web lane is a **bounded portability experiment** (D-028),
and the single hard invariant is that **the desktop path stays
behavior-identical** — every target-gate this lane adds must not change native
behavior. The desktop gate (`cargo test --workspace`) is the proof at every
landing point.

## 1. Verified premises (state as premises, with pointers)

These are settled, independently supervisor-verified (2026-10-16). Cite them; do
not re-derive them.

- **26c.1 PASS — emscripten web boot.** A **`wasm32-unknown-emscripten`**
  **binary** + `-sWASM_BINDGEN` boots in a real browser (Chromium, via
  `agent-browser`), creates a **WebGL2** context, constructs a working
  **`glow`** context, runs a `request_animation_frame` loop, and exposes a
  callable `#[wasm_bindgen]` export (console: raw-webgl2 clear OK, glow clear OK
  → `status()=glow-ok`, "render loop started"; the 640×480 canvas renders
  orange). The full build recipe + the two 26c.1 gotchas + the glow loader route
  are recorded verbatim in `.agents/skills/lib-web-stack.md`
  §"26c.1 — Emscripten target (verified)"; also
  `.agents/plans/001-rust-port/26-wasm-web.md` §"26c.1 — emscripten web boot:
  PASS" and `DECISIONS.md` **D-029**.
- **26c.2 PASS — single-module (Luau + render).** **Luau (mlua, vendored C++) +
  `wasm-bindgen`/`web-sys` + `glow` all link AND run in ONE
  `wasm32-unknown-emscripten` module** (console: `glow-ok;luau-ok;acc=9 d=42 q=3`;
  canvas renders orange; debug `.js` ≈ 317 KB, `.wasm` ≈ 19.5 MB). This confirms
  D-029's single-module premise. Recorded verbatim in
  `.agents/skills/lib-web-stack.md` §"26c.2 — Luau + wasm-bindgen + glow in one
  emscripten module (verified)" and `DECISIONS.md` **D-029** (with the 26c.2
  confirmation). **Two load-bearing integration facts (mandatory — §7):**
  (1) **`-Clink-arg=-sDEFAULT_TO_CXX` is REQUIRED to link** (emcc's linker driver
  does not auto-link the C++ EH runtime for the mixed Rust+C++ module; linking
  with `-fwasm-exceptions` is NOT needed — emscripten 6.0.x's prebuilt
  libcxxabi already provides those symbols); (2) **the `__wasm_call_ctors`
  init-order trap** — with `-sWASM_BINDGEN` the wasm-bindgen glue registers Rust
  `main` via `addOnInit`, and emscripten's `initRuntime()` runs the onInit
  callbacks **BEFORE** `wasmExports['__wasm_call_ctors']()`, so Rust `main` (and
  Luau's `Lua::new()`) runs before C++ static initializers → Luau's
  `kSizeClassConfig` (`lmem.cpp:192`) is zero-initialized → `blockSize == 0` → a
  **divide-by-zero wasm trap** in `newclasspage` (`lmem.cpp:285`) — a SILENT halt.
  **FIX:** call `__wasm_call_ctors()` (declared `extern "C"`) as the FIRST
  statement of `main()`.
- **26a PASS — the VFS.** `oh-vfs` exists: the `Vfs` trait (a
  `Send + Sync + Debug` object-safe provider: `read` / `is_dir` / `is_file` /
  `list_dir` / `write`), `RealFs` (`#[cfg(not(target_arch = "wasm32"))]`), and
  `MemoryVfs` (pure, wasm-cross-compiles). `oh-assets` and `oh-config` route
  **every** content fs op through it (no `std::fs` in their `src/`). See
  `.agents/tasks/issue-026a-oh-vfs.md`.

**Implication (not a blocker, from D-029):** Emscripten provides a **libc +
in-memory FS**, so `std::fs` / `std::time` / `std::env` actually WORK on this
target (content *could* be preloaded into MEMFS). The intended content path is
still fetch→`MemoryVfs` (26c SG5); `RealFs` is not automatically excluded on
emscripten — the web path deliberately avoids it so no `std::fs` remains on the
emscripten code path.

## 2. The 26c.3 compile triage (the inventory)

Verified by `cargo check --target wasm32-unknown-emscripten` (2026-10-16, `wasm`
branch). This is the exact work list: the crates that already build, and the
crates that do not, with the precise blocker each.

**GREEN for emscripten (no work):** `oh-math`, `oh-rng`, `oh-vfs`, `oh-data`,
`oh-config`, `oh-assets`, `oh-menu-system`, `oh-menu`, `oh-parity`, `oh-replay`,
`oh-net`, `oh-server-control`, `oh-workshop-uploader`.

> **Surprise (record it):** emcc builds the C deps **from source** —
> `libz-sys`, `libsodium-sys`, and `libsqlite3-sys` all compile under emscripten
> on this box. So `oh-replay` (zlib) and `oh-net` (sodium + rusqlite) **compile**
> for emscripten, and `rodio` / `cpal` (audio) compile too. The net/DB/Steam/
> Discord crates being *compile*-green does **not** mean they are *usable* on
> web — they are still feature-gated OFF (runtime/backend concerns, §3/§5). The
> blockers below are the ones that actually fail to compile.

**FAIL, with the exact blocker:**

| crate | exact blocker (emscripten) |
|---|---|
| **`oh-render`** | (1) `glutin 0.32.3` `compile_error!("Please select at least one api backend")` — glutin has **no emscripten backend**; (2) `winit 0.30.13` `compile_error!` — its web backend is `target_os = "unknown"`-only (`build.rs:11` gates `web_platform = { all(target_family = "wasm", target_os = "unknown") }`), so emscripten (`target_os = "emscripten"`) is unsupported. **`rodio`/`cpal` are NOT blockers** (they compile). |
| **`oh-lua`** | `mlua` is pinned `features = ["luajit", "vendored"]` → `mlua-sys`/`luajit-src`'s build script needs `make` (a LuaJIT **C** build; the JIT is not wasm-viable). Plus it **inherits `oh-game → oh-discord`** (the uuid error below). |
| **`oh-discord`** | `discord-rich-presence 1.1.0 → uuid 0.8.2` **E0599** (`Uuid::new_v4` — uuid's `v4` module is cfg-gated **out** on emscripten). |
| **`oh-game`** | pulls **`oh-discord` unconditionally** (a hard dependency; the `Option<Arc<dyn Discord>>` seam already exists) → **inherits** the uuid E0599. |
| **`oh-steam`** | inherits `oh-game → oh-discord` (the uuid error). |
| **`oh-app`** | inherits all of the above; pulls `oh-net`, `oh-steam`, `oh-discord` unconditionally; `egui-winit` needs `winit` (compile-error on emscripten); the `-server` path uses `std::net` + rusqlite (`dispatch.rs`). |

**Note:** direct `std::{fs,thread,net,time}` uses are **compile-fine** on
emscripten (they resolve to the emscripten libc / in-memory FS / runtime
shims). They are **runtime/backend** concerns, not compile blockers — the
compile failures above are all from C/C++ build scripts or from crates whose
backend is `target_os = "unknown"`-only.

## 3. The web architecture (what already works in our favor + the design)

- **Target + module shape (D-029):** `wasm32-unknown-emscripten`, ONE module.
  `wasm-bindgen` must be a **`bin` crate** (a `cdylib` links as an Emscripten
  *side module* with no JS loader → undefined `__wbindgen_*`) plus
  `-Clink-arg=-sWASM_BINDGEN`; `emcc` then runs the `wasm-bindgen` CLI itself
  over the linked wasm. Requires Emscripten ≥ 6.0.10 (this box: emsdk 6.0.12).
  **`winit` is dropped** — the event loop is a plain `request_animation_frame`
  (verified in 26c.1).
- **GL route (the 26c.1 verified path):** `glow 0.18` on emscripten compiles its
  **`native`** module (not `web_sys`), so `Context::from_webgl2_context` is
  unavailable. The working route is
  `glow::Context::from_loader_function_cstr(|n: &CStr|
  emscripten_webgl_get_proc_address(n.as_ptr()))` after creating + making current
  an emscripten WebGL2 context via the emscripten C API:
  `emscripten_webgl_init_context_attributes` (with `major_version = 2`) →
  `emscripten_webgl_create_context` (takes a **CSS selector**, `"#c"`, not a
  bare id) → `emscripten_webgl_make_context_current`. The WebGL2 context must be
  created **BEFORE** `emscripten_webgl_get_proc_address` resolves (it dispatches
  off emscripten's current `GLctx`).
- **Window / input / loop backend:** a new emscripten backend in `oh-render`
  replacing the winit/glutin path — a canvas element + `emscripten_webgl_*`
  (GL) + `web-sys` DOM events (input) + a `request_animation_frame` loop.
  **Keep** `raw-window-handle`, `fontdue` (pure Rust — the text pipeline
  cross-compiles), and `png` on the web path.
- **Shader translation:** reuse **`translate_for_gles`** (issue 25b SG3) for
  WebGL2 shaders — WebGL2 ≈ GLES 3.0, and pack `.frag` files have no `#version`
  (desktop defaults, `gl_FragColor`), so the GLES preamble is the same
  translation the Android lane proved.
- **Content path:** fetch `_RELEASE` into a **`MemoryVfs`** (or preload into
  emscripten MEMFS) and feed `oh-assets` / `oh-config` through `oh-vfs` (26c
  SG5). Every `.lua`/content read on web goes through `oh-vfs` / in-memory bytes
  — **no `std::fs`** on the emscripten code path.
- **Lua path:** route `.lua` loads from **in-memory bytes** (`lua.load(<source
  string>)` — the verified path), NOT `load_from_file` (mlua's own
  `// TODO: figure out why emscripten fails on file operations`).
- **Audio:** `rodio`/`cpal` compile on emscripten but have no usable wasm
  backend; **v1 may ship silent** (documented) per D-028c (a WebAudio sink behind
  the existing `AudioDevice` trait is 26d).
- **Single-thread:** the game loop is already single-threaded; the `std::thread`
  uses (audio mixer / the 25b extraction) are runtime concerns — wasm is
  single-threaded (without COOP/COEP + atomics). The web entry is the **third
  consumer** of the same `WindowedClient` pipeline the android lane (25b) split
  the platform glue from.

## 4. Acceptance criteria for 26c

1. **`cargo check -p oh-web --target wasm32-unknown-emscripten` green**, and the
   combined bin **builds + loads in a browser** (emsdk 6.0.12 recipe, §1/D-029).
2. **Desktop gate stays green** (the target-gates must not change native
   behavior): `cargo test --workspace` **unchanged** — only the 8 known
   Windows-environment failures; no new failures. Every target-gate added by this
   lane must be a no-op on the native build.
3. **Every `.lua` / content read on web goes through `oh-vfs` / in-memory bytes**
   — no `std::fs` / `load_from_file` on the emscripten code path (the `.lua`
   bytes come from the `MemoryVfs`/MEMFS content).
4. **The end-to-end milestone (browser gameplay) is the final acceptance** — the
   D-028 decisive milestone: a **real playable gameplay session in a browser** —
   load a level, run its Lua, render, and take input — **verified in Chromium via
   `agent-browser`** (not `cargo check`).

## 5. Out of scope

- **Net / Steam / Discord online features** — the online *client* is a follow-up;
  these are feature-gated OFF on web (they compile on emscripten per §2 but are
  not wired up / not exercised in v1).
- **WebAudio** — audio may be **silent in v1** (documented) per D-028c; a WebAudio
  sink behind the `AudioDevice` trait is 26d.
- **egui console** — can be gated OFF on web (it is debug/window-gated and
  irrelevant to replay determinism, D-022/D-023); not part of the v1 gate.
- **No desktop behavior change** — the target-gates are behavior-identical on
  native; the desktop gate is the proof.
- **No `_RELEASE/` content edits** and no dependency-version bumps.
- **Content-packing specifics** (how `_RELEASE` is shipped as a site asset /
  single `.zip`) and the **replay corpus run** (the original P0.4 body) are
  26d / a separate item — this lane proves the vertical slice, not the ship
  packaging.

## 6. Sub-goals (commit after EACH green one)

Each sub-goal ends **green** (desktop gate unchanged + the emscripten target
moves forward); commit after each.

1. **`oh-render` web backend (the emscripten gate).** Target-gate `glutin` +
   `winit` to `not(target_os = "emscripten")`; add a web window/GL/loop/input
   backend for emscripten — a canvas element + `emscripten_webgl_*` (GL context
   per §3) + `web-sys` DOM events (input) + a `request_animation_frame` loop;
   build `glow` via `from_loader_function_cstr` (the §3 route); keep
   `raw-window-handle` / `fontdue` / `png`. Reuse **`translate_for_gles`** (25b)
   for the WebGL2 shaders. Desktop `oh-render` unchanged.
2. **`oh-lua` on web = Luau.** Feature/target-gate `mlua` — `luajit` on desktop,
   `luau` + `vendored` on emscripten (`vendored` is REQUIRED on wasm —
   `mlua-sys/build/find_normal.rs` panics without it). Route `.lua` loads from
   **in-memory bytes** (`lua.load(<source string>)`), not `load_from_file`.
   Honor the 26b caveats: Luau **string interpolation `` `{x}` `` is INERT**
   (immaterial for the shipped corpus — a repo-wide scan found no interpolation);
   and the mandatory build recipe (`CXXFLAGS_wasm32_unknown_emscripten =
   -fwasm-exceptions` + `-sDEFAULT_TO_CXX` + `__wasm_call_ctors`, §7).
3. **discord/steam optional.** Gate **`oh-discord`** (→ the `uuid` E0599) **OFF**
   for emscripten (the `NoDiscord` arm); make **`oh-game`'s `oh-discord`** an
   **optional feature** (the `Option<Arc<dyn Discord>>` seam already exists) so
   `oh-game` / `oh-steam` / `oh-lua` no longer inherit the uuid error on web.
   Desktop keeps Discord ON by default (unchanged).
4. **`oh-app` gates + the `oh-web` bin.** Gate the **`-server` / `std::net`**
   path, the **`egui-winit`** hook, and the **winit windowed loop** for
   emscripten; add the **`oh-web`** crate — a **binary** (`src/main.rs`, no
   `[lib]`) with `-sWASM_BINDGEN` and **`__wasm_call_ctors()` as the FIRST
   statement of `main()`** — using the **verbatim `.cargo/config.toml` / env
   recipe** from §26c.1 / §26c.2 (D-029). Resolve the 26c.1/26c.2 "main ran
   twice" artifact (ensure a single init).
5. **content path.** Fetch `_RELEASE` into a **`MemoryVfs`** (or preload into
   emscripten MEMFS) and feed `oh-assets` / `oh-config` through `oh-vfs`; **boot
   to the menu** (the menu is the first representative state — no gameplay yet).
6. **exit — the playable session.** A **real playable gameplay session in a
   browser** (the D-028 milestone): load a level, run its Lua, render, and take
   input; **verified in Chromium via `agent-browser`**.

## 7. Constraints

- **Commit after every green sub-goal.** Never leave work uncommitted.
- **Desktop gate is sacred**: `cargo test --workspace` unchanged (only the 8
  known Windows-environment failures) at every landing point. The desktop path
  must be **behavior-identical** — every target-gate is a no-op on native.
- **The two 26c.2 gotchas are MANDATORY** (do not rediscover them):
  1. **`-Clink-arg=-sDEFAULT_TO_CXX`** in the web crate's `.cargo/config.toml`
     (link the C++ EH runtime for the mixed Rust+C++ module).
  2. **`__wasm_call_ctors()` as the FIRST statement of `main()`** (declared
     `extern "C"`) — otherwise Luau traps on a silent divide-by-zero before C++
     static initializers run.
- **The mandatory emscripten env recipe** (every build, from 26b/26c.1/26c.2):
  `EMSDK` / `EM_CONFIG`; `CC`/`CXX`/`AR_wasm32_unknown_emscripten` =
  `emcc.exe` / `em++.exe` / `llvm-ar.exe`;
  `CXXFLAGS_wasm32_unknown_emscripten = -fwasm-exceptions` (the key one — the
  rule for ALL C++ in this target); `PATH` includes emsdk's
  `upstream\emscripten`, `upstream\bin`, and `~/.cargo/bin` (for the
  `wasm-bindgen` CLI). After changing
  `CXXFLAGS_wasm32_unknown_emscripten`, run `cargo clean -p mlua-sys` once
  (staleness trap — no `rerun-if-env-changed`). emsdk 6.0.12 / node 24.19.0.
- **Do not modify `_RELEASE/`** tracked content. Do not bump dependency versions
  (the web stack — `wasm-bindgen 0.2.129` / `js-sys 0.3.106` / `web-sys
  0.3.106` / `glow 0.18` — is already pinned by the verified spikes; `mlua`
  `luau` + `vendored` per 26b).
- f32/f64, literal-port discipline; the byte-sorted discovery order and the exact
  `LoadInfo`/config error strings are pinned (inherited from 26a) — do not
  "improve" them. No servers, no background processes.

## 8. Report

Sub-goal-by-sub-goal status, commands, commit hashes, and the desktop gate tail
(proving no new failures). Record: the `oh-render` emscripten backend as landed
(canvas + `emscripten_webgl_*` + `web-sys` events + `request_animation_frame`;
the `glow::from_loader_function_cstr` route; the `translate_for_gles` reuse);
the `oh-web` crate as landed (the verbatim `.cargo/config.toml` + env + the
`__wasm_call_ctors` fix + the single-init resolution); the `mlua`
`luajit`/`luau` target-gate; and the `oh-discord`/`oh-game` optional-feature
change. Confirm that **every `.lua` / content read on web goes through
`oh-vfs` / in-memory bytes** (no `std::fs` on the emscripten path). Provide the
**browser-console evidence** for (a) the **menu boot** (SG5) and (b) the **gameplay
session** (SG6, the D-028 milestone) — the `agent-browser` console + a screenshot
each, mirroring the 26c.1/26c.2 console captures. Close with the explicit
out-of-scope list (what 26d still must do: the WebAudio sink, content-packing
specifics, and the replay corpus run).
