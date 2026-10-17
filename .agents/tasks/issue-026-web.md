# Task 026 — the wasm/web port, clean restart (on top of master)

Status: **plan authored after the research branch (`wasm-backup-prewas`) was
parked as research only.** Master is authoritative; this lane layers a *thin*
web platform on top of it.

## Goal

Boot the real engine in a browser (the D-028 decisive milestone), with the
**smallest possible footprint on the shared core**, mirroring the existing
Android pattern:

* **Shared core** (untouched by platform concerns): `oh-game`, `oh-data`,
  `oh-config`, `oh-assets`, `oh-audio`, `oh-menu`, `oh-menu-system`,
  `oh-math`, `oh-rng`, `oh-replay`, `oh-lua`.
* **Platform layer** (window/GL/input/audio): `oh-render`, `oh-app`.
* **Small platform-entry crates** over the shared `oh-app` pipeline:
  `oh-android` (exists) and a new **`oh-web`** (this lane).

## What the research branch proved (reuse the facts, not the churn)

1. **Target = `wasm32-unknown-emscripten`** (not `wasm32-unknown-unknown`).
   `winit 0.30`'s web backend is `target_os="unknown"`-only and **cannot**
   target emscripten; `mlua`-Luau (C++) builds **only** on emscripten.
2. **`wasm-bindgen` now supports emscripten**: the crate must be a **bin**
   (a `cdylib` links as a side module with no JS loader) and link with
   **`-Clink-arg=-sWASM_BINDGEN`**; emcc then runs the `wasm-bindgen` CLI.
3. **glow on emscripten** compiles its `native` module (no `from_webgl2_context`):
   build it with `Context::from_loader_function_cstr(|n| emscripten_webgl_get_proc_address(n))`
   after creating + making current a WebGL2 context via the `emscripten_webgl_*`
   C API (`emscripten_webgl_create_context("#c", attrs)`, `major_version=2`).
4. **Build recipe** (root `.cargo/config.toml`, target-scoped):
   `linker = "emcc.exe"`; rustflags `-Cpanic=abort`,
   `-Cllvm-args=-enable-emscripten-cxx-exceptions=0`,
   `-Crelocation-model=static`, `-Clink-arg=-sDEFAULT_TO_CXX`,
   `-Clink-arg=-sWASM_BINDGEN`, `-Clink-arg=-Wno-experimental`,
   `-Clink-arg=-sMODULARIZE`, `-Clink-arg=-sEXPORT_ES6`,
   `-Clink-arg=-sMAX_WEBGL_VERSION=2`, `-Clink-arg=-sMIN_WEBGL_VERSION=2`.
5. **Two mandatory gotchas**: (a) `-sDEFAULT_TO_CXX` (link the C++ EH runtime);
   (b) call **`__wasm_call_ctors()` as the FIRST statement of `main()`**
   (or Luau traps: the `-sWASM_BINDGEN` glue runs Rust `main` before the C++
   static ctors). `CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions` is
   required for the C++ compile; `cargo clean -p mlua-sys` once after changing it.
6. **Content = `--preload-file _RELEASE@/RELEASE` + `-sFORCE_FILESYSTEM=1`.**
   Emscripten ships a real libc + MEMFS, so the **existing `std::fs` content
   path works unchanged** on web — **no VFS refactor is needed**.
7. **`web-sys` listeners are needed for input**; master's new **`TextEntered`
   sink** (`Window::set_entered_text_sink`, D-031/issue 025d) is the right seam
   for web text entry — feed it from the web keydown path.

## Explicitly NOT doing (the research branch's churn)

* **No `oh-vfs` refactor** (26a). It removed `Assets`' on-disk path, which
  collides with master's device-verified `oh-audio` (`get_sound_path() -> &Path`);
  and it is unnecessary given (6). Research-only.
* No renumbering/wrestling with master's `D-028..D-031`; our web decisions get
  fresh, free IDs.
* No scattered `cfg` soup: emscripten specifics live in **isolated modules**
  (a `web` backend module in `oh-render`; a `web` entry module/crate), exactly
  like the Android split.

## Sub-goals (each ends green; commit after each)

Every sub-goal must keep the **desktop gate unchanged**
(`cargo test --workspace` = only the known Windows-environment failures) and
must not regress master's Android path.

### Landed status (2026-10-17, on `wasm` over `origin/master`)

W0, W1, W2, W3, W5, W6 are **LANDED** (commit hashes below). W4's scope was
**absorbed** into W2 (`oh-net`/`oh-steam`/`oh-audio` gated) and W3
(`oh-discord` + `oh-lua` gated) rather than landing as its own commit.
**26a (`oh-vfs`) is DROPPED**: emscripten's MEMFS makes master's `std::fs`
content path work on web unchanged, so no VFS refactor is needed — the whole
26a research branch stays parked in `wasm-backup-prewas`.

* **W0 — `oh-web` crate skeleton — LANDED `29f3824e`.** New `crates/oh-web` (bin), deps only under
  `[target.'cfg(target_os = "emscripten")'.dependencies]` (the `oh-android`
  pattern: empty/no-op on other targets so the desktop gate stays green); add
  the emscripten table to the root `.cargo/config.toml`; add the workspace
  member. A trivial `main` + `index.html`. **Acceptance:** `cargo build
  --workspace` green; `cargo build -p oh-web --target wasm32-unknown-emscripten`
  emits `.js`/`.wasm`; loads in a browser (blank canvas / a clear).
* **W1 — `oh-render` emscripten backend (isolated) — LANDED `dad120b8` + `966f7637`.** A dedicated web backend
  module + thin gates on `Window`/`GlParts`' winit/glutin bootstrap; keep the
  `glow` loader route (3); **preserve master's input work** (Enter mapping,
  `entered_text_sink`, swap interval) and **feed the sink from the web
  keydown path** so text entry works. **Acceptance:** `cargo check -p oh-render
  --target wasm32-unknown-emscripten` green; desktop byte-identical (cfg-only
  diff audit).
* **W2 — `oh-app` web support (minimal) — LANDED `324e2527`.** Gate `winit`/`egui`/`egui-winit`/
  `egui_glow`/`glow0.17`/`signal-hook` to non-emscripten; gate the egui console
  + SIGINT + window icon; provide the minimal seam so `oh-web` can drive the
  shared `WindowedClient`. **Acceptance:** `cargo check -p oh-app --target
  wasm32-unknown-emscripten` green; desktop unchanged.
* **W3 — Lua backend as a feature (Luau selectable everywhere) — LANDED `f6906368`.** `oh-lua`:
  mlua backend by feature — default `luajit`; a `luau` feature used on
  emscripten (and flippable globally later). **Acceptance:** desktop default
  unchanged; emscripten builds with `luau`; note the global-Luau experiment as
  a follow-up (per the user: Luau may go everywhere after testing).
* **W4 — non-core subsystems off on web — ABSORBED** into W2 (`oh-net`/`oh-steam`/`oh-audio`) + W3 (`oh-discord`/`oh-lua`). Gate `oh-net`/`oh-steam`/
  `oh-discord` and audio (`oh-audio`/`rodio`) for emscripten (online + audio are
  out of the web prototype's scope; audio may be silent). **Acceptance:** the
  emscripten link succeeds; desktop unchanged.
* **W5 — content + boot to the menu — LANDED `30c0dab1`.** `_RELEASE` preloaded into MEMFS (6);
  `oh-web` boots the shared composition to the **menu**. **Acceptance:** verified
  in Chromium via `agent-browser` (loading screen → menu).
* **W6 — playable session — LANDED `352a8ae0`.** `oh-web` seeds a default
   profile into MEMFS (a documented prototype workaround for master's known
   profile/save no-op, tracker 29b6/29c1) so the game boots past the blocking
   PROFILE CREATION. Reach a real gameplay state (input incl. text
   entry). **Acceptance:** a representative game session in a browser.
   **RE-EVALUATED AT INTEGRATION (lane 026, onto master `86ea8eb2`):** the
   seed is REMOVED — 029c1 landed the real profile system after this commit
   was written (`Assets::profile_create` writes + reloads the store), so the
   web target goes through the real first-run path over MEMFS (a fresh user
   boots to the C++'s own PROFILE CREATION state; a created profile persists
   for the session). The seed is recorded here as the historical W6 state;
   see the D-036 amendment + the `## Report` below.

### Verification

- **Browser-verified (Chromium):** the menu loading screen renders —
  `PACKS LOADED 8 / LEVELS LOADED 37 / ASSETS LOADED 61 / NO LOAD ERRORS`
  (W5). Text entry works in-browser (W1's fix: web `TextEntered` is
  independent of the SFML key-code map).
- **NOT yet verified:** navigating the main menu → level selection → a level
  in the browser (agent-browser automation was flaky; master's profile flow is
  queued 29b6/29c1).
- **Desktop/Android unaffected:** `cargo build --workspace` green;
  `cargo test --workspace --no-fail-fast` = the 8 known Windows-environment
  failures; clippy/fmt clean. Every target-gate is `cfg`-only (desktop no-op).
- **Artifacts (debug):** `oh-web.js` ~358KB, `oh_web.wasm` ~59MB,
  `oh_web.data` ~32.6MB. Toolchain: emsdk 6.0.12; the recipe + the two
  gotchas (`-sDEFAULT_TO_CXX`; `__wasm_call_ctors()` FIRST in `main()`) are in
  `.agents/skills/lib-web-stack.md` §26c.

## Constraints

* Master is authoritative; where a shared file must change, prefer the smallest
  `cfg` addition, keep emscripten logic in its own module, and prove the
  desktop/android paths are a no-op.
* No `_RELEASE/` edits. No dependency-version bumps beyond the web stack
  (`wasm-bindgen 0.2.129` / `js-sys`+`web-sys 0.3.106`, already in the lockfile
  from the research branch) and the emscripten `mlua` `luau` feature.
* Commit per green sub-goal. Rebase/squash only (no merge commits).
* Reference the research branch (`wasm-backup-prewas`) for anything reusable;
  do not re-derive the recipe/gotchas.

## Report

Sub-goal-by-sub-goal status, commands, commit hashes, the desktop-gate tail, the
`oh-render`/`oh-app` web seams as landed, the `oh-web` recipe, and the browser
evidence for W5/W6.
