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

**pending P0.3:** the actual web render path — WebGL2 init through the
windowing stack, a minimal `oh-web` `cargo check`/build for
`wasm32-unknown-unknown`, and a served page booting to the menu — has **not yet
been exercised**. `wasm-bindgen`/`wasm-pack` are absent here (the plan's
`Build/ship shape` assumes `trunk`, which is present, but the bindgen toolchain
is not confirmed). Do not cite these as verified.

## Phase 0 status (2026-10-16)

- **P0.1 (replay compression interchange): PASS** — miniz_oxide ↔ zlib
  lossless; see `.agents/skills/lib-flate2.md`.
- **P0.2 (core cross-compile + Luau gate): core PASS; Luau gate =
  toolchain-gated.** The four pure-Rust crates cross-compile clean; the Luau
  go/no-go is blocked on a wasm C++ toolchain (wasi-sdk for `wasm32-wasip1`, or
  emscripten for `wasm32-unknown-unknown`) — see `.agents/skills/lib-mlua.md`
  §WASM and DECISIONS.md D-028a.
- **P0.3 / P0.4: pending** — the web/browser rendering side (WebGL2 init, a
  minimal `oh-web` compile, a served page booting to the menu) and the replay
  corpus run against native vs web impls are not yet exercised.
