# Issue 26 — Investigation: a WASM/Web port (`wasm32-unknown-unknown`)

Status: **investigation/spec** (no code landed). This file is the decision
surface: what blocks, what substitutes, what stays. The port discipline
(literal 1:1, determinism-first) applies double here: the browser is the one
platform where our byte-exactness guarantees are at risk from the substrate
itself.

## What already works in our favor

- **winit 0.30 has a wasm32 backend** (`web-sys`/`EventLoopExtWebSys`) — the
  event-loop shape we ported (25b's `ApplicationHandler`) is the same.
- **glow supports WebGL2** (the same `glow::Context` API over
  `WebGl2RenderingContext`). Our renderer is glow-only — no glutin on the
  web path (glutin does NOT support wasm; the 25c GLES work is a preview:
  WebGL2 ≈ GLES 3.0, the shader translation (`translate_for_gles`, issue
  25b SG3) is likely reusable verbatim).
- **fontdue** (pure Rust) — the text pipeline cross-compiles.
- **The game core is platform-clean**: oh-game/oh-data/oh-config/oh-menu*
  have no platform deps (proven by the headless tests); the android work
  (25b) already split the platform glue from the game loop — the web entry
  is the third consumer of the same `WindowedClient` pipeline.

## Blockers and substitutions (the real decisions)

1. **LuaJIT does not run on wasm.** mlua's `luajit` feature is the 07-lane
   foundation (285 API functions, `%.14g` formatting D-005, writeVariable
   semantics). Options, in fidelity order:
   a. **Luau** (mlua `luau` feature) — mlua supports it; different VM =
      different float formatting/GC timing. The 07 API surface would need
      re-verification against the C++ oracle corpus (oh-parity's Lua
      vectors). NOT byte-compatible; behavioral compatibility only.
   b. **Wasm Lua 5.4** (mlua `lua54` + a wasm-compatible `lua54-src`... not
      available; would need a hand-built Lua C shim via emscripten) — heavy.
   c. **Rust Lua interpreter** (e.g. `hematita`-class) — no.
   d. **Defer**: web build ships WITHOUT user Lua levels (the built-in
      levels' logic is ported Rust where the C++ had it in Lua? No — level
      logic IS Lua). So (d) is not viable for a real game; **(a) Luau is the
      only realistic route**, with an explicit D-decision: "web Lua is
      behavioral, not byte-exact; determinism tests gate the desktop stack
      only".
2. **flate2's `zlib` backend (D-006)**: `libz-sys` does not build for wasm.
   The CZHEX/replay byte-exactness pin was verified against system zlib.
   Substitution: `miniz_oxide` — its deflate output is *usually*
   zlib-compatible but NOT guaranteed byte-identical to `compress2(level 9)`.
   Decision needed: re-pin the oracle corpus against miniz_oxide for the web
   target (replays stay self-consistent on web; desktop replays remain the
   canonical wire format — cross-platform replay exchange becomes
   best-effort, documented).
3. **Filesystem**: no CWD, no `std::fs`. oh-assets has 13 `fs::` sites; the
   android extraction (25b SG4) already centralized content acquisition —
   the web analogue is a **fetch-into-memory VFS**: `_RELEASE` packed into
   the site (or a single `.zip` fetched once), an `oh-vfs` trait
   (`read(path) -> Vec<u8>`, `exists`, `list_dir`) implemented by
   `RealFs` (desktop/android) and `MemoryVfs` (web). That trait does not
   exist yet — it is the main *refactor* this issue needs (oh-assets +
   oh-config's file reads route through it; behavior-identical on native,
   gate-enforced).
4. **std::thread** (2 sites in the render/app path — the audio mixer and
   the 25b extraction?): wasm is single-threaded (without COOP/COEP +
   atomics). Audit + cfg-gate; the game loop is already single-threaded.
5. **The net stack stays OFF on web** (sodiumoxide/libsodium-sys, rusqlite
   bundled, std::net — none build/behave on wasm). Feature-gate
   `oh-net`/`oh-server`/`oh-steam`/`oh-discord` out of the web build (the
   default-off pattern from Steam/Discord is the precedent). The online
   *client* could someday be a JS-side fetch against the same server
   protocol — explicitly out of scope for issue 26 v1.
6. **Audio**: rodio/cpal have no wasm backend (cpal has an experimental
   webaudio... not rodio 0.23). The web target needs an AudioContext
   WebAudio sink behind the existing `AudioDevice` trait — a new small
   surface (issue-10-remainder-class), v1 can ship silent + documented.
7. **egui console**: egui supports wasm — the 014b overlay should carry
   over; keep it out of the v1 gate scope.

## Build/ship shape

- Target: `wasm32-unknown-unknown`, `wasm-bindgen` + `web-sys`; serve with
  `trunk` (or wasm-pack) — a static site (`index.html` + the wasm + the
  packed content). No emscripten (winit/glow need no emsdk; emscripten
  would only be for the (2b) Lua-C route).
- CI: a web build job is `cargo check --target wasm32-unknown-unknown -p
  oh-web` (compile-green) + optionally a playwright smoke later.

## Proposed sub-lanes (issue 26, if green-lit)

| Lane | Scope |
|---|---|
| 26a | the `oh-vfs` trait + oh-assets/oh-config routing (native behavior-identical, gate-enforced) |
| 26b | the Luau decision + the 07 API re-verification harness (oh-parity Lua vectors against Luau) — the go/no-go gate |
| 26c | the oh-web crate: winit-web entry + glow/WebGL2 + translate_for_gles reuse |
| 26d | WebAudio sink (or silent-v1), flate2/miniz re-pin decision, content packing |

## Decision requests (D-028 candidates — user call)

1. ~~Luau-instead-of-LuaJIT on web only (behavioral-compat; desktop stays
   LuaJIT-locked)?~~ **MOVED to issue 30** (`30-lua-runtime.md`): the LuaJIT
   version skew + the K3 Android hang make this a cross-platform VM decision,
   not a web-only one. See that doc's option matrix + go/no-go spike.
2. miniz_oxide replay deflate on web only (web replays self-consistent,
   cross-platform replay exchange documented best-effort)?
3. v1 scope: no net/no steam/discord/no audio (silent) — acceptable?

## Verification

- Desktop gate stays 4/4 + 698 (the VFS refactor is native-identical).
- `cargo check -p oh-web --target wasm32-unknown-unknown` green.
- A served page that boots to the menu (manual/browser check; playwright
  smoke is a follow-up).
