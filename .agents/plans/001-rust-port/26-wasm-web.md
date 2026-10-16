# Issue 26 — Investigation: a WASM/Web port (`wasm32-unknown-unknown`)

Status: **approved for 26a + a Phase 0 feasibility spike** (no code landed
yet). Phase 1+ is gated on Phase 0 evidence; v1 is a **feasibility prototype**,
not a product commitment (D-028c). This file is the decision surface: what
blocks, what substitutes, what stays. The port discipline (literal 1:1,
determinism-first) applies double here: the browser is the one platform where
our byte-exactness guarantees are at risk from the substrate itself.

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
      **D-028a gate (conditional yes):** a real go/no-go with three outcomes
      — **PASS** (corpus + targeted adversarial tests establish acceptable
      compatibility) / **CONDITIONAL PASS** (differences identified, bounded,
      explicitly documented) / **FAIL** (scripts rely on unreproducible
      behavior). Distinguish **API parity / semantic parity / runtime
      parity**. **Do NOT assume** mlua's Luau backend builds/executes on
      `wasm32-unknown-unknown` — a minimal **compile-and-execute** test is
      required (Phase 0).
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
   **Replay contract (D-028b):** replays are exchanged as an **encrypted
   compressed blob** — `ClientToServer::CTSPCompressedReplay` →
   `send_encrypted` (`crates/oh-net/src/client.rs`); the server calls
   `decompress_replay_file` (`crates/oh-net/src/server.rs`). There is **no
   hash/signature/checksum over the compressed payload** in `oh-replay`/
   `oh-net` (the `levelValidator` is a separate field). So the promise is
   **"same replay semantics — decompression recovers the same logical
   replay"**, NOT byte-identity; byte-identity is asserted only by the
   desktop oracle pins in `oh-parity`. **Do not weaken replay compatibility
   until a concrete incompatibility is demonstrated** — *test* the
   substitution (miniz_oxide compress ↔ zlib decompress and vice-versa), do
   not assume best-effort.
3. **Filesystem**: no CWD, no `std::fs`. oh-assets has 13 `fs::` sites; the
   android extraction (25b SG4) already centralized content acquisition —
   the web analogue is a **fetch-into-memory VFS**: `_RELEASE` packed into
   the site (or a single `.zip` fetched once), an `oh-vfs` trait
   (`read(path) -> Vec<u8>`, `exists`, `list_dir`) implemented by
   `RealFs` (desktop/android) and `MemoryVfs` (web). That trait does not
   exist yet — it is the main *refactor* this issue needs (oh-assets +
   oh-config's file reads route through it; behavior-identical on native,
   gate-enforced).
   **Smallest contract (user guidance):** prefer the *smallest* contract that
   matches actual use — an **asset/configuration provider**, not a general
   filesystem. The browser needs only resolve-known-path / read /
   load-config-and-defaults / predictable missing-asset handling. The
   interface must **not force eager loading** (startup latency + memory).
   Track the **native regression burden** of this refactor explicitly — it is
   the largest reusable change even if the browser port is later abandoned.
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

## Decisions (D-028a/b/c — user call, resolved)

1. **D-028a — Web Lua runtime (Luau):** conditional yes. Acceptable only where
   script behavior is explicitly tested and the `oh-parity` corpus establishes
   that boundary; desktop stays LuaJIT-locked. Distinguish API / semantic /
   runtime parity. Lane 26b is a real gate (PASS / CONDITIONAL PASS / FAIL);
   do NOT assume mlua's Luau backend builds/executes on
   `wasm32-unknown-unknown` (a compile-and-execute test is required).
2. **D-028b — Web replay compression (miniz_oxide):** yes, with a clearer
   replay contract. The exchange is an encrypted compressed blob with no
   hash/checksum over the payload, so the contract is "same replay semantics —
   decompression recovers the same logical replay", not byte-identity
   (byte-identity is pinned only by the desktop `oh-parity` oracle). Do not
   weaken replay compatibility until a concrete incompatibility is demonstrated
   — test the substitution, don't assume best-effort.
3. **D-028c — v1 scope:** prototype yes. v1 = no net/Steam/Discord; audio may
   be silent only if justified. v1 is a feasibility prototype, not a shipping
   commitment; prove rendering + the game loop first.

## Sequencing (approved)

The port is a gated progression; each phase has an explicit exit. Phase 0
proves the risky assumptions **before** any broad refactoring.

- **Phase 0 — prove the risky assumptions** (before broad refactoring):
  compile a minimal `oh-web` binary for `wasm32-unknown-unknown`; execute a
  representative script with Luau; test WebGL2 init through the windowing
  stack; run the replay corpus against native and web impls.
  *Exit: no fundamental toolchain/compatibility blocker.*
- **Phase 1 — extract the VFS**: smallest useful abstraction, native behavior
  retained. *Exit: native builds/tests pass; asset/config behavior
  unchanged.* (Note: the largest reusable change even if the browser port is
  later abandoned.)
- **Phase 2 — playable vertical slice**: load real content, run scripts,
  render, input, reach a representative gameplay state in a browser.
  *Exit: a real gameplay loop works, not merely `cargo check`.*
- **Phase 3 — ship/no-ship decision**: evaluate startup time, memory,
  rendering correctness, browser support, replay compatibility,
  silent-operation acceptability. *Exit: explicit ship/no-ship decision.*

Defer audio / content-packing / peripheral browser features until Phase 2
succeeds.

## Verification

- Desktop gate stays 4/4 + 698 (the VFS refactor is native-identical).
- `cargo check -p oh-web --target wasm32-unknown-unknown` green.
- A served page that boots to the menu (manual/browser check; playwright
  smoke is a follow-up).
