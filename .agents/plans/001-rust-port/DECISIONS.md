# DECISIONS — SSVOpenHexagon Rust port

A running log of binding technical decisions, one per settled question. Each entry
records: the question, the decision, the evidence (real source), and the issue that
landed it. Decisions here are authoritative for workers; a worker that finds code
contradicting one must report it rather than silently diverge.

## D-000 — Port is literal, arithmetic-exact

- **Decision:** mirror C++ structure; `float`→`f32`, `double`→`f64`; no
  reordering of floating-point expressions; no "clean" API redesigns.
- **Evidence:** replay determinism depends on exact evaluation (plan 001 §Ground rules).
- **Landed by:** all issues.

## D-001 — Workspace layout

- **Decision:** single Cargo workspace at repo root; crates under `crates/` as
  listed in `PLAN.md`; binary crates `open-hexagon`, `oh-server-control`,
  `oh-workshop-uploader`.
- **Landed by:** issue 01.

## D-002 — Vendored C++ libs are folded, not crate-per-lib

- **Decision:** SSVUtils/SSVStart/SSVMenuSystem analogues live inside
  `oh-math`/`oh-timeline`/`oh-render`/`oh-menu-system`, not as standalone crates
  (they are thin and only used here).
- **Landed by:** issues 01, 10, 12.

## D-003 — PCG32 must be bit-exact

- **Decision:** reproduce `pcg32_fast` exactly (state/multiplier/increment/seeding)
  plus the exact `std::uniform_int_distribution` / `uniform_real_distribution`
  semantics used, so replay RNG streams match the C++ build.
- **Evidence:** mapping/`libs-ssv.md` (PCG section) + mapping/`gameplay.md`.
- **Landed by:** issue 01.

## D-004 — Third-party substitution table

- **Status:** candidates recorded in `PLAN.md`; each is finalised by the issue that
  first depends on it, and the decision is written here with evidence.
- Pending: SFML analogue (issue 10), Lua runtime (06), JSON (03), zlib (02),
  sodium (16), SQLite (17), ImGui (14), Steam (20), Discord (21).

## D-004 (SFML entry) — render backend: real-GPU stack, NOT `rsfml`

- **Question:** which Rust backend implements the SFML analogue `oh-render`
  (window / input / vertex-batch / texture / font / text / view / shader / audio
  device)? `rsfml` was the plan's *primary* candidate; `winit`+`glow`+`rodio`
  (±`fontdue`/`ab_glyph`) was the fallback.
- **Decision:** use a **real-OpenGL** stack — `winit` 0.30 (window / event loop /
  input platform backend), `glutin` 0.32 + `glutin-winit` 0.5 + `glow` 0.18
  (GL context + GL function loader), `rodio` 0.23 (audio device surface),
  `fontdue` 0.9 (TTF metrics + glyph rasterization → GL glyph atlas), plus
  `oh-config`'s `Key` / `Button` (the exact SFML `Keyboard::Key` /
  `Mouse::Button` integers) via a dependency. A self-written CPU software
  rasterizer was **rejected** for the offscreen path.
- **Evidence (all verified on this box, 2026-07):**
  - **`rsfml` is not a published crate.** `cargo add rsfml` (and `-graphics`,
    `-window`, `-audio`, `-system`, `-network`) → `the crate 'rsfml' could not
    be found in registry index`; none of the crates appear under
    `~/.cargo/registry/src/**`. The plan's primary candidate does not exist on
    crates.io, so there is no `rsfml` source to read and no fallback decision
    could be grounded in it.
  - **The fallback crates resolve and compile:** `winit` 0.30.13 (builds
    headless on this box in ~12s), `glutin` 0.32.3, `glutin-winit` 0.5.0,
    `glow` 0.18.0, `rodio` 0.23.0, `fontdue` 0.9.4, `ab_glyph` 0.2.32.
  - **Headless offscreen GL is infeasible here:** no `OSMesa`, and no usable
    offscreen EGL context (only Chrome's own `libEGL` is present; X11 pkg-config
    is available, but there is no display in the test environment). A pure
    `glow` offscreen target therefore cannot back the required
    "offscreen render target → draw a triangle → `glReadPixels`" headless test.
  - **GLSL is a hard product requirement, not optional.** Pack content ships
    GLSL fragment shaders (`_RELEASE/Packs/*/Shaders/*.frag`: `pixelate.frag`,
    `text.frag`, `gradient.frag`, `steamart.frag`) and the game binds one per
    `RenderStage` (mapping/rendering-menu.md §1/§9, the `shdr_*` Lua API,
    `assets.getShaderByShaderId`). A CPU rasterizer cannot execute these at all,
    so it would silently break third-party packs and issue 11's shader work. The
    backend must run **real GL**.
- **How the headless-test tension is resolved** (do NOT weaken the product for
  testability): the pure-logic surface is tested headless with **no GL context**
  — vertex/quad emission order + counts, text-layout metrics, the exact
  key/button integer tables, input-state transitions, camera/world↔screen math,
  and `RenderStates`/primitive mapping. The actual GPU path is covered by ONE
  smoke test that (a) tries to create a context, (b) if a display/context is
  unavailable it **SKIPS with a printed reason** (not a silent pass, not a
  `#[ignore]`), (c) when a context IS available (dev box / CI with a display) it
  does the offscreen draw + `glReadPixels` colour/position readback.
- **Enum compatibility surface:** `Key`/`Button` are reused from `oh-config`
  (`crates/oh-render` depends on `oh-config`) so the in-memory enum integers
  stay exactly `Unknown=-1 .. Pause=100, KeyCount=101` and `Left=0 ..
  XButton2=4, ButtonCount=5` (oracle-vectors.md).
- **Cross-compile:** `x86_64-pc-windows-gnu` is **NOT RUN** here (target/
  rustup absent); issue 22/CI owns the Windows build. `winit`+`glutin` are
  cross-platform by design.
- **Landed by:** issue 10 (`crates/oh-render`).

## D-005 — In-repo skills are written from real source

- **Decision:** every library (vendored C++, Rust crate, system lib) gets a skill
  in `.agents/skills/` written from its real source; skills are authoritative for
  workers; a code/skill contradiction triggers a minimal factual skill correction
  reported under "skill corrections".
- **Landed by:** each issue that first touches a library.

## D-006 — Worker dispatch settings (infrastructure)

- **Decision:** dispatch implementation workers at `thinking=medium`
  (`model: "local/local:medium"`) and keep concurrency at 1–2.
- **Evidence:** during plan 001 execution, `xhigh` thinking produced per-worker
  contexts of 7.7M tokens and hour-long silent generations; the `tinyllb` KV
  bin-pack admission gate then wedged (`current_kv=0.0` but
  `ema_kv_per_flow=0.9059 > max_threshold=0.90`), holding **every** request
  forever — workers reported "no activity" while consuming **0 tokens**.
  Restarting `tinyllb` (`systemctl --user restart tinyllb.service`) cleared it
  and latency went 55s → 0.3s. Diagnosis used
  `journalctl --user -u tinyllb.service` (look for
  `kv bin-pack: holding — projected over threshold`) and
  `curl :8000/metrics | grep num_running_reqs` (direct-to-engine vs through-LB
  latency comparison).
- **Do NOT edit `~/.config/tinyllb/config.yaml`** to work around this — it is the
  user's infrastructure. Reduce worker thinking instead, and restart the service
  if it wedges.


## D-007 — The recovery script must not be run under in-flight work

- **Decision:** `tools/infra/restart_lb_if_wedged.sh` is report-only by default
  (restart requires `--restart`), requires the deadlock log signature
  (`admit=false` **with** `current_kv=0.0`), refuses to restart when agent work is
  in flight, and uses a long (90s) probe timeout.
- **Evidence / mistake made:** the first version used a 12s probe and restarted
  unconditionally. 12s is shorter than healthy latency (observed 35s), so it
  reported "WEDGED" for a working backend, and the restart **aborted a live
  worker mid-request**, destroying the issue-004 run in progress. A restart
  kills every in-flight generation.
- **Rule:** a slow-but-answering backend is not wedged; judge by the log
  signature. Never restart while an agent is working.

## D-008 — JSON parser divergences (Rust is stricter than the JsonCpp fork)

Verified from real source by the `oh-data` skills pass
(`.agents/skills/lib-jsoncpp.md`, `.agents/skills/lib-serde-json.md`); the C++
fork is `include/SSVOpenHexagon/SSVUtilsJson/JsonCpp/jsoncpp.inl` (pinned).

| # | Divergence | C++ fork | Rust (`oh-data`) | Impact |
|---|---|---|---|---|
| 1 | Trailing input after the root value | **Accepted** — `parse` does `readValue()` then only skips comments; no end-of-input check (`jsoncpp.inl:126-131`) | `serde_json::from_str` rejects it ("trailing characters") | Rust rejects some inputs the game accepts. No shipped file does this; keep, but note. |
| 2 | Non-finite reals (`1e999`) | Accepted → `inf` (`decodeDouble` has no finiteness check) | `Number::from_f64` returns `None` → parse error | Rust rejects. No shipped file does this. |
| 3 | Trailing commas | Rejected: objects `"Missing '}' or object member name"` (`:391`), arrays the generic syntax error (`:170`). **The fork has no `allowTrailingCommas_`.** | Rejected, different wording | Behaviour matches. |
| 4 | Unterminated `/*` | Rejected (`tokenError`) | Rejected | Matches. |
| 5 | Comment positions | Accepted before root, after root, before object keys, after object separators, after array elements. **NOT** between an object key string and `:` | Reproduced by `strip_comments` in `crates/oh-data/src/json.rs` | Pinned by the `comments.rs` tests. |
| 6 | Float **writing** | `sprintf("%#.16g")` + zero-trim (`jsoncpp.inl:1876-1902`) | `oh-data` reimplements `%#.16g`-and-trim, deliberately **bypassing** `serde_json::to_string` (serde_json 1.0.154 serializes floats via the `zmij` shortest-round-trip algorithm, `ser.rs:1716-1724`) | **Do not substitute `serde_json::to_string` on the write path** — it would change the output bytes. |

- **Decision:** keep the stricter Rust behaviour for 1–2 (no shipped file exercises
  them), pin 3–5 with tests, and treat 6 as a hard rule for any future write path.
- **Landed by:** issue 003; recorded here so later config/profile writers comply.

## D-009 — Lua number→string spelling is `%.14g`, not Rust's default

- **Decision:** number→string conversion in `oh-lua` re-implements C `%.14g`
  (`lua_number2str`), and must NOT use Rust's `f64::to_string` or LuaJIT's
  Lua-side `tostring`.
- **Evidence:** the pinned LuaJIT (`vittorioromeo/LuaJIT@4dc5ffb`,
  `src/luaconf.h:120`) defines `LUA_NUMBER_FMT "%.14g"` and
  `lua_number2str(s,n) = sprintf(s, LUA_NUMBER_FMT, n)`. Verified against gcc:
  `1e20→"1e+20"`, `1.0/3.0→"0.33333333333333"`, `1e15→"1e+15"`,
  `1234567890123456→"1.2345678901235e+15"`. LuaJIT's Lua-side `tostring` prints
  17 significant digits and would diverge (`1e+20` vs `100000000000000000000`).
- **Landed by:** issue 006 (`crates/oh-lua/src/convert.rs::lua_number2str`, test
  `lua_number2str_matches_c_g14`).

## D-010 — `LuaContext::writeVariable("a.b.c")` does NOT create intermediates

- **Decision:** a dotted write whose table part does not exist **throws
  `VariableDoesntExist`**; it does not create intermediate tables. (The issue-006
  contract's summary wording said "creates intermediate tables" — the C++ does not,
  so the C++ wins per the spec's own Exactness clause.)
- **Evidence:** `src/SSVOpenHexagon/Utils/LuaWrapper.cpp:167-205` — `_setGlobal`
  splits on the last `.`, calls `_getGlobal(tableName)`, and throws
  `VariableDoesntExistException` if that is not a table.
- **Landed by:** issue 006 (`crates/oh-lua/src/context.rs`).

## D-011 — Render backend: real GL (`winit`+`glutin`+`glow`), not a software rasterizer

- **Decision (D-004's SFML entry, ruled 2026-10-15):** the SFML analogue is
  `winit` (window/event loop/input) + `glutin` (GL context) + `glow` (GL bindings),
  `rodio` (audio device surface), `fontdue`/`ab_glyph` (fonts, uploaded as GL
  textures), with `oh-config`'s `Key`/`Button` reused for the exact SFML enum
  integers.
- **`rsfml` is REJECTED as unimplementable:** it is the plan's primary candidate
  but **is not on crates.io** (`cargo add rsfml` fails; absent from
  `~/.cargo/registry/src/**`). Verified on this box.
- **A self-written CPU software rasterizer was proposed and REJECTED.** Rationale:
  **pack content ships GLSL fragment shaders** (`_RELEASE/Packs/*/Shaders/*.frag` —
  `pixelate.frag`, `text.frag`, `gradient.frag`, ...), the game binds a fragment
  shader per `RenderStage`, and the `shdr_*` Lua API sets their uniforms. A CPU
  rasterizer cannot execute them, so "fully headless" would silently break third-
  party packs. **A portability/testability concern must not delete a product
  requirement.**
- **Headless testing policy (resolves the tension without weakening the product):**
  1. the pure-logic surface (vertex/quad emission order and counts, text metrics,
     enum tables, input transitions, camera math, RenderStates mapping) is tested
     headless with real assertions;
  2. the GPU path gets ONE smoke test that creates a context, does an offscreen
     draw + `glReadPixels` readback when a display/context is available, and
     otherwise **skips with a printed reason** — never a silent pass and never a
     fake rasterizer standing in for GL.
- **Landed by:** issue 010.

## D-012 — Issue contracts are claims; the C++ source is the contract of record

- **Decision:** when a contract/plan statement conflicts with the real C++ source,
  the source wins and the contract is corrected in-place with a dated note.
- **Evidence (two cases, same session):**
  1. issue 006's contract said `writeVariable("a.b.c")` "creates intermediate
     tables" — `LuaWrapper.cpp:167-205` shows it **throws** `VariableDoesntExist`
     (D-010);
  2. issue 012's contract said the `Controller` maps "key bindings → item actions"
     — the real `Controller.hpp` has **no** key handling (only `enableItemWhen` +
     `update`); key mapping lives in `MenuGame` (issue 013).
  Both times the worker followed the source and reported the discrepancy, which is
  the required behaviour.
- **Rule for spec authors (me):** a claim about what a library does must be derived
  from its source, not from its name or its plan prose — the same rule as
  "a plausible claim about a language is still a claim".
- **Landed by:** issues 006/012; recorded here.

## D-013 — Engine/Lua state sharing: one store, reentrant lock

- **Context:** `oh-lua` depends on `oh-game` (its `api/state.rs` builds `GameApiState`
  from gameplay types), so `oh-game -> oh-lua` as a *normal* dependency is a cycle.
  But the 285 Lua lambdas and the engine update loop must mutate **one** storage of
  `level_status`/`status`/`walls`/`rng`/timelines — copies diverge and replay parity
  breaks (`l_setPulseMin`, `t_*` writes lost).
- **Decision:**
  1. Extract the shared data + the `GameApi`/`AssetsApi` traits + a
     `ReentrantMutex<T>` into `oh_game::lua_state`.
  2. `oh-lua`'s `GameApiState` becomes `{ shared: SharedState, run_lua_file, assets,
     backend }` + `Deref<Target = SharedState>`; the `State` alias becomes
     `Arc<ReentrantMutex<GameApiState>>`; the 285 lambda bodies stay unchanged.
  3. `HexagonGame<S>` is generic over `S: Deref<Target = SharedState>`; Lua
     callbacks are injected via a `ScriptHost` trait defined in `oh-game` and
     implemented by the composition (never a normal `oh-game -> oh-lua` dependency).
  4. `oh-game` may dev-depend on `oh-lua` + `oh-assets` so tests can compose the
     full stack against the real `_RELEASE` packs.
- **Reentrancy is required, not stylistic:** `t_eval`/`e_eval`/`ct_eval` append
  deferred closures that run Lua **while the state guard is held**
  (`crates/oh-lua/src/api/main_timeline.rs:37-40`), and lambdas call
  `lock(&st).backend.*` at 148 sites. A plain `std::sync::Mutex` self-deadlocks.
- **Verified on this box:** the dev-dependency cycle is legal (built a two-crate
  workspace with `a -> b` normal + `b -> a` dev; `cargo build` succeeded);
  `mlua`'s `send` feature is NOT enabled, so `Send`/`Sync` is not forced.
- **Guardrails required of the implementation:** keep all of issue 007's tests green
  (285-name exact-set, api.rs, runtime.rs, menu-context); prove reentrancy with a
  test that re-enters through a lambda AND a test that the depth counter returns to
  zero; document the `unsafe` raw-pointer contract and drop order explicitly.
- **Landed by:** issue 009.
