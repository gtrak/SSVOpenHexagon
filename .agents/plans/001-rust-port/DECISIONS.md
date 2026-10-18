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
  sodium (16), ImGui (14), Steam (20), Discord (21).
- Finalised: SQLite (17) — see D-004 (SQLite entry) below.

## D-004 (SQLite entry) — `rusqlite` (bundled) for the server DB

- **Question:** which Rust crate implements the C++ `Online/Database` storage
  (sqlite_orm over the vendored SQLite C API, file `ohdb.sqlite`)?
- **Decision:** `rusqlite` with the **`bundled`** feature (workspace dep,
  pinned `0.31`). The schema DDL and every query are hand-transcribed from
  `src/SSVOpenHexagon/Online/Database.cpp` (sqlite_orm's *generated* SQL is
  the reference — see `crates/oh-net/src/database.rs` +
  `.agents/skills/lib-sqlite.md`), so the Rust `Database` and the C++ server
  read/write the same `ohdb.sqlite` file format.
- **Why `bundled`:** it compiles the SQLite C engine from source via
  `libsqlite3-sys`, mirroring the `libsodium-sys` build-from-source pattern
  (the D-004 sodium entry) and removing a system-sqlite dev dependency from
  the build (this box has no `libsqlite3` development package — only the
  CLI). The bundled engine's version differs from the vendored 3.36.0, but
  only the on-disk format matters (the schema DDL is what sqlite_orm writes
  verbatim into `sqlite_master`, pinned byte-for-byte by
  `tools/oracle/db_probe.cpp` + `crates/oh-net/tests/database.rs`).
- **Evidence:** `crates/oh-net/tests/database.rs` (the `.schema` diff + the
  per-function round-trips), `Cargo.toml` (workspace dep + the substitution
  note).

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
  `rodio` (audio device surface), `fontdue` (fonts, uploaded as GL textures), with
  `oh-config`'s `Key`/`Button` reused for the exact SFML enum integers.
  **Corrections verified against the landed tree (issue 011):** `glutin-winit` is
  NOT used — `oh-render` drives `glutin::display::Display` directly with
  `raw-window-handle` 0.6 (`grep -c glutin-winit Cargo.lock crates/oh-render/Cargo.toml`
  = 0); and `ab_glyph` is not in the dependency tree (fontdue is the actual choice).
  Exact pinned versions: winit 0.30.13, glutin 0.32.3, glow 0.18.0, rodio 0.23.0,
  fontdue 0.9.4, raw-window-handle 0.6.2 (`.agents/skills/lib-render-stack.md`).
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

## D-014 — Verify a worker's FILE CITATION, not just its claim

- **Decision:** when a worker reports "I found X in <file>:<lines>", open that
  file at those lines. A correct semantic claim with a wrong citation is still a
  defect in the record, and the citation is what a later reader trusts.
- **Evidence (issue 011, 2026-10-15):** the worker reported finding the
  `state >= States::SMain` menu-hexagon background gate "in `HGGraphics.cpp:122-141`".
  The code it wrote is correct, and the gate does exist — but it is in
  **`MenuGame.cpp:5773-5780`** (`const bool mainOrAbove{state >= States::SMain};`
  → `styleData.drawBackgroundMenu(...)`), while `HGGraphics.cpp:122-141` is the
  wall/custom-wall quad reservation loop. Its in-code comment was right; only the
  reported citation was wrong, which is why the claim survived review.
- **Rule:** cited file:line pairs in a report, a commit message, or a mapping doc
  get checked the same way a number does. (Companion to the plan-process rule that
  a line number is not evidence of which function it is in.)
- **Also verified accurate in the same pass:** the `death_shakeCamera` errata
  (`HexagonGame.cpp:822-853`: `status.cameraShake = 45.f * multiplier`, no
  `preShakeCenters`) and `death_flashEffect` being unconditional.

## D-015 — A reflexive `Deref<Target = Self>` impl is a bug, not a trick

- **What happened:** issue 009's WIP added
  `impl Deref for SharedState { type Target = Self; fn deref(&self) -> &Self { self } }`
  (plus `DerefMut`) so that `SharedState` itself would satisfy the engine's
  `S: Deref<Target = SharedState>` bound. Two methods
  (`get_dependent_script_filename` → `self.get_dependent_asset_filename(...)`,
  `lua_state.rs:347-355`) then failed with
  `E0055: reached the recursion limit while auto-dereferencing \`SharedState\``.
- **Mechanism (reproduced in a minimal scratch crate):** with a reflexive `Deref`
  in scope, a method call on `self` that could also resolve through `Deref`
  enters autoref-deref search, which follows the impl into itself — infinitely.
  Removing the impl compiled immediately. **UFCS (`SharedState::inner(self, ..)`)
  does NOT avoid it**; the impl itself is the defect.
- **Fix:** delete the reflexive impl and give the engine a dedicated accessor
  trait (`fn shared(&self) -> &SharedState` / `shared_mut`) implemented for both
  `SharedState` and the real wrapper. A *wrapper's* `Deref<Target = SharedState>`
  (e.g. `GameApiState`'s) is fine — it is the self-targeting impl that is broken.
- **General rule:** never `impl Deref for T { type Target = Self }`. If a type must
  satisfy a "derefs to X" bound *as itself*, express that with an accessor trait.
- **Landed by:** issue 009.

## D-016 — Concurrency: one subagent at a time (user directive, 2026-10-15)

- **Decision:** run **one** implementation worker at a time from now on, not two.
  (The two in flight at the time of the directive — issue 009's fix pass and issue
  013 — are allowed to finish; the next dispatch is single.)
- **Why it matters here:** every workspace-wide signal becomes ambiguous with two
  writers — the gate is red from whichever lane's uncommitted WIP happens to be in
  the tree, so each worker needs the clean-checkout technique
  (`git archive HEAD | tar -x -C /tmp/vN`) to get an honest result, and every
  "the build is broken" report has to be triaged by crate before it means anything.
  One writer per tree also removes the class of mistake where a worker proposes
  moving a sibling's live files.
- **Sequencing consequence:** the plan's issue graph still allows parallel lanes
  (03/04/05, 16/17, etc.), but they will be run in sequence. Expect longer wall
  clock; the compensating benefit is that each commit is individually verified on a
  clean checkout without cross-lane contamination.

## D-017 — Committing another lane's work in a live tree: stage only that lane's paths

- **Situation:** a worker timed out leaving a whole new crate untracked
  (`crates/oh-menu/`, 4,295 lines) while ANOTHER worker was still live in the same
  working tree with its own uncommitted edit (`crates/oh-game/src/lib.rs`).
  The untracked work had to be preserved, but its author was gone.
- **What is safe:** `git add <exact paths>` for the dead lane only, then commit.
  A commit does not modify the working tree, and an index operation that never
  names the live lane's file leaves that file byte-identical. Verified after the
  commit: `git status` still showed the live worker's `M crates/oh-game/src/lib.rs`
  and its edit was intact.
- **What is NOT safe:** `git add -A` / `git add .` (it would stage the live lane's
  in-progress file), `git stash` (removes it from the tree entirely — this nearly
  destroyed a worker's work in an earlier session), `git checkout <ref> -- <paths>`,
  and `git clean`.
- **General rule:** in a shared tree, the orchestrator's write operations may only
  name paths owned by a *dead* lane. Never a glob, never a stash, never a checkout.

## D-018 — `test/ReplayExecution.t.cpp` does NOT encode expected scores (corrects the 09 contract)

- **The claim (plan 09, "Notes / risks"):** "The C++ `test/ReplayExecution.t.cpp`
  encodes expected scores for known level/seed/replay combos — port those numbers
  verbatim as the pin." The task spec (`issue-009-impl.md`, verification item 3)
  repeats it.
- **The source (verified this session):** the file has exactly three assertions
  (`grep -n TEST_ASSERT test/ReplayExecution.t.cpp` → lines 81, 104, 109):
  - `TEST_ASSERT(rf.has_value())` — a replay was produced;
  - `TEST_ASSERT(score2.has_value())` — replay playback terminated;
  - `TEST_ASSERT_EQ(score, replayPlayedTimeSeconds)` (line 109) — the **live run's**
    `playedTimeSeconds` equals the **replay run's** `playedTimeSeconds`.
  There is **no numeric literal anywhere in the file**. `TEST_ASSERT_EQ` is
  `TEST_ASSERT_OP(lhs, ==, rhs)` (`test/TestUtils.hpp:174`): a plain `==` on the two
  *computed* doubles.
- **What that means:** the C++ test is a **self-consistency** check — "a deterministic
  engine replays its own recording to the identical played-time" — not an oracle. It
  cannot supply a C++-derived expectation, and no amount of porting will make it one.
  A *score* pin would have to come from a full C++ game build (SFML+LuaJIT+ImGui),
  which the plan already records as unavailable (09 "Notes on getting C++
  expectations", same as D-004/D-011's situation).
- **Consequence for the 09 harness:** the honest evidence is
  (a) the ported `TEST_ASSERT_EQ(score, replay_played_time)` self-consistency
  property (live run == replay run, both `differentHG` variants), clearly labelled
  **self-consistent, NOT C++-verified**; plus
  (b) whatever *is* C++-derived: the existing `oracle_gameplay.rs` vectors and
  per-frame step math via `tools/oracle/` probes. Never present (a) as C++-verified
  (this is the plan's own instruction, 09 "Notes").
- **Also false in the same note:** "the input vector is generated from a C++ run" —
  the C++ test generates inputs in-process from `getRndBool()`/RNG
  (`executeRandomInputs` at line 40-51, `alwaysSpinRight`), i.e. from the engine's own
  RNG, again self-consistent — so there is no external input vector to import either.
- **CORRECTION (my own first draft of this entry was wrong — D-014):** I initially
  wrote that the packs/levels the test names are not shipped. They **are**. Verified
  by re-deriving `buildPackId` (`Utils/BuildPackId.cpp`: space→underscore on each
  field, joined by `_`, trailing version) against the shipped `pack.json`s:
  `_RELEASE/Packs/cube/pack.json` = {disambiguator `ohvrvanilla`, author
  `vittorio romeo`, name `cube`, version 1} → packId
  `ohvrvanilla_vittorio_romeo_cube_1` (matches the test's `packs[0]`), and
  `_RELEASE/Packs/cube/Levels/apeirogon.json` → levelId
  `ohvrvanilla_vittorio_romeo_cube_1_apeirogon` (matches the test's `levels[0]`);
  likewise `experimental` → `ohvrvanilla_vittorio_romeo_experimental_1` +
  `autotest0`. So the harness **can** use the real shipped packs and the exact
  level ids the C++ test uses. Level-id format is `packId + "_" + levelId`
  (`MenuGame.cpp:4840`); the score validator appends `_m_<diffMult>`
  (`LevelValidator.cpp:16`, `LevelData.cpp:42`).
- **Rule (D-012 restated):** when a contract makes a claim about a source file, the
  source wins. Contributed by orchestrator verification before dispatch, so the
  worker is not sent chasing numbers that do not exist.

## D-019 — `setLevelData` must reset the timeline RUNNERS, not only the timelines (replay-determinism bug)

- **Situation:** issue 009 step (c) landed the determinism harness against the
  `test/ReplayExecution.t.cpp` level set only (2 pairs: `cube`/`apeirogon`,
  `experimental`/`autotest0`). `09-game-loop.md` also requires porting
  `test/ReplayExecutionBenchmark.t.cpp`, which uses **4** pairs — adding
  `orthoplex`/`arcadia` and `orthoplex`/`bipolarity` — and 100 iterations.
- **What I found while verifying (orchestrator, independent scratch runs):** the
  4th pair, `ohvrvanilla_vittorio_romeo_orthoplex_1_bipolarity`, breaks the
  self-consistency property on the **same-engine** variant: `live=1.05`,
  `replay=0.85`. The 2-pair harness could not have caught it.
- **Discrimination (which isolated it to engine re-use, not the replay format):**
  - replay on a **fresh** engine: `live=1.05 replay=1.05` (match);
  - replay on the **same** engine that produced the live run: `live=1.05
    replay=0.85` (mismatch);
  - three consecutive replays on one engine: **identical** (`1.2208…` ×3) — so
    playback itself is deterministic; the *live* run is what changes after a
    previous `newGame` (same-engine live = `1.3875, 0.85, 0.85`; fresh-engine
    live = `1.3875` ×3).
- **Root cause (verified):** C++ `setLevelData` resets each timeline **and** its
  runner — `eventTimeline.clear(); eventTimelineRunner = {};`
  (`HexagonGame.cpp:668-669`), `timeline.clear(); timelineRunner = {};`
  (`:686-687`), `messageTimeline… + messageTimelineRunner = {};` (`:672-673`).
  The port cleared the three timelines and reset only
  `message_timeline_runner`; `main_timeline_runner` and `event_timeline_runner`
  kept their `current_idx` / `wait_start_tp` (`oh-game/src/timeline.rs:102-105`).
  `bipolarity.lua`'s `onInit` ends with `t_wait(12 / getMusicDMSyncFactor())` and
  its `onStep` schedules `e_wait(...)`s, so the first wait of the next game
  measured against the **previous** game's `wait_start_tp`: the spawn schedule
  shifted and the death time moved. Levels that wait early and heavily on the
  timelines (`bipolarity`, `arcadia`) expose it; `apeirogon`/`autotest0` happen
  not to.
- **Fix:** reset all three runners in `set_level_data_inner`, in the C++ order
  (`hexagon_game.rs`, next to each `*.clear()`). Two statements.
- **Rule:** when the C++ pairs `x.clear()` with `xRunner = {}`, the port must do
  BOTH. A cleared timeline with a stale runner is silent state leakage across
  `newGame` (and `newGame` is the restart path in the real game too — this is a
  1:1 fidelity gap (D-000), not a test artifact).
- **Process lesson:** the coverage requirement in `09-game-loop.md` (port BOTH
  replay tests, i.e. the 4-pair set) is what would have caught this at review
  time. A harness that passes on a subset is not evidence for the whole issue.
- **Correction of my own first draft (D-014):** I first recorded this as "the Lua
  context must be fresh per `newGame`" and blamed the reused `LuaContext` keeping
  `floatingWalls`. Scratch probes disproved it (`#floatingWalls` was re-assigned
  by the script itself, and a level with no custom walls reproduced nothing).
  The `lua = Lua::LuaContext{}` difference (`:731`) is real and still worth
  modelling, but it is **not** the cause of this failure.

## D-020 — Where the `Renderer`-contract wiring lives: an `oh-render → oh-game` adapter (issue 15), not issue 09

- **Context:** issue 09's contract says "Provide a `Renderer` trait ... so `oh-game`
  does not depend on the render backend (issue 10); `oh-render` implements it."
  `oh-game` has the trait (`crates/oh-game/src/renderer.rs`: `Renderer` +
  `RecordingRenderer`), and the engine consumes it where the loop needs a draw
  sink (flash batch, touch positions, FPS). What is NOT done is the other
  direction: nothing implements `oh_game::renderer::Renderer` for the real stack,
  and nobody drives `oh_render::graphics::draw_frame` from `HexagonGame::draw()`.
- **Why it cannot go in `oh-game`:** the dependency must stay `oh-game → (nothing
  render-y)`; `oh-render` already exists with `oh-config`/`oh-math` only, and
  `oh-game` must not gain `oh-render`. The adapter therefore belongs in a crate
  that depends on BOTH — i.e. the composition.
- **Why it cannot go in `oh-render` alone:** `draw_frame` needs the game's
  per-frame inputs (config, `status`, `levelStatus`, `styleData`, the Lua
  `onRenderStage` hook, the non-game RNG, particle state). Issue 11 encoded that
  as `DrawState` + `FrameInput` and left explicit `TODO(issue-009-wiring)` markers
  naming the exact C++ call sites (`graphics.rs:258, 384, 386, 701, 804, 1063,
  1108, 1245, 1374`; `batching.rs:20`). `oh-render` should not depend on
  `oh-game` either (that would invert the layering the plan chose).
- **Decision:** the wiring is the **composition's** job and lands with issue 15
  (`oh-app`, which already depends on 09 + 11 and owns the app wiring), as an
  adapter crate/module implementing `oh_game::renderer::Renderer` by filling
  `FrameInput`/`DrawState` from the engine and calling `draw_frame`. It can be
  pulled forward into a dedicated issue if issue 15 turns out too large.
- **Consequence for issue 09's status:** 09 is LANDED for its own scope (the
  update loop + its verification harnesses). The un-wired `TODO(issue-009-wiring)`
  markers are the *integration* debt they name, tracked against 15 — they are not
  a defect in the loop, and no `oh-game` behaviour depends on them (headless
  determinism is unaffected, which is why the D-019 bug was findable at all).

## D-021 — `_RELEASE/luadocs.md` is a STALE 2021 capture: NOT a valid oracle for issue 14

- **The temptation:** `_RELEASE/luadocs.md` (69 KB, `-printLuaDocs` stdout with real
  Steam/Discord boot log lines above the docs) looks like a perfect oracle for
  issue 14's `print_docs` — it has exactly the C++ format
  (`## <prefixHeader>` then `* **\`ret name(args)\`**: docs`) and 252 entries.
- **It is stale.** It was committed once, `3b166a84` "Refacotring", **2021-03-29**.
  Verified against the *current* source:
  - `LuaMetadata.hpp` defines `NUM_CATEGORIES = 11` with `prefixCategories{u_, a_,
    t_, e_, l_, s_, w_, cw_, ct_, shdr_, Miscellaneous}`, and the file's own
    comment says "There are 9 categories" (stale comment). The capture contains
    only **9** `## ` headers — `## Custom Timeline Functions (ct_)` and
    `## Shader Functions (shdr_)` are **absent** (0 occurrences of `ct_`/`shdr_`).
  - The 252 captured names vs the **285** currently registered
    (`oh-lua/tests/api.rs::REGISTERED_NAMES`): 33 missing = the 8 `ct_*` + 13
    `shdr_*` + 12 others. Each of those 12 has a real `.doc(...)` in the current
    C++ (checked `u_getWidth`, `u_isHeadless`, `u_setFlashColor`,
    `l_forceSetPulse`, `l_removeTracked`) and was introduced **after** the
    capture: `u_getWidth` 2022-06-09, `u_isHeadless` 2023-02-22,
    `u_setFlashColor` 2021-11-14, `l_forceSetPulse` 2022-10-05
    (`git log -S` on the name).
- **Consequence for issue 14:** do NOT pin `print_docs` output against
  `_RELEASE/luadocs.md`. The correct oracle is the **current source**: the 11
  `prefixHeaders` strings (verbatim templates) + the `.arg`/`.doc` chains at the
  `addLuaFn`/`makeLuaAccessor`/`sdColorGetter` call sites. `print_docs`'s expected
  entry count is the number of documented functions in the *current* source, which
  is NOT 252 and must be re-derived (a good check: the category headers must be
  11, not 9).
- **Rule (D-014/D-018 family):** an artifact that looks like an oracle must be
  dated against the source before it is used as one. `_RELEASE/luadocs.md` is a
  historical build output, not a contract.

## D-022 — ImGui analogue for issue 14: `egui` 0.37 (+ `egui-winit`, `egui_glow`), verified compatible with the D-011 stack

- **D-004 left this as "assess".** Assessed, with a real build (not docs):
  `cargo new` + `egui = "0.37"`, `egui-winit = { version = "0.37",
  default-features = false, features = ["links"] }`, `egui_glow = "0.37"`,
  `winit = "0.30"`, `glow = "0.18"` → **builds clean** and the resolved lockfile
  keeps `winit 0.30.13` (the D-011 version), so the console can share the existing
  window/event-loop stack.
- **Caveat to handle in the lane (not a blocker):** `egui_glow 0.37` depends on
  **`glow 0.17`**, while `oh-render` uses **`glow 0.18`** — both end up in the
  lockfile. They are different Rust types over the same GL context, so the
  integration must not try to pass `oh-render`'s `glow::Context` straight into
  `egui_glow`; give the console its own `glow 0.17` context/loader for the shared
  GL context (or bridge the function-pointer tables). Record what you actually did.
- **Scope discipline (what the console actually needs):** the C++
  `postUpdate_ImguiLuaConsole` is ~190 lines and `drawImguiLuaConsole` is a
  `window == nullptr` early-return + `Imgui::render`. The ImGui surface used is
  small and listed exactly in the source: `Begin/BeginChild/EndChild`, `Text/
  TextUnformatted`, `Separator`, `SameLine`, `PushStyleVar/PopStyleVar`
  (`ImGuiStyleVar_ItemSpacing`), `PushStyleColor/PopStyleColor` (`ImGuiCol_Text`),
  `SetScrollHereY`, `SetNextWindowSize` (`ImGuiCond_FirstUseEver`),
  `GetStyle/GetFrameHeightWithSpacing`, `InputText` with
  `EnterReturnsTrue | CallbackCompletion | CallbackHistory` and an
  `ImGuiInputTextCallbackData` callback (`ilcTextEditCallback`: completion,
  history Up/Down, the `?` Lua-docs prefix), `Checkbox`, `DragFloat`,
  `SetItemDefaultFocus`, `SetKeyboardFocusHere`. `egui` covers all of these
  (`egui::TextEdit::singleline` + `TextEditOutput.cursor_range` /
  `TextEditState::store` for completion+history; `egui::Window`/`ScrollArea`/
  `Frame` for the layout).
- **Behaviour over pixels (per the 14 contract):** the console is a developer tool.
  Command history, tracked variables, tab completion, error display and the F1
  toggle must match; exact pixel layout must not be chased.
- **The console is gated twice** — `window == nullptr` returns, and
  `!Config::getDebug() || Config::getOfficial()` forces it hidden — so it is
  **irrelevant to headless determinism**, and its input suppression
  (`imguiLuaConsoleHasInput()` gating `updateInput`, C++ `HGUpdate.cpp:649`) only
  matters when it is actually shown. Keep it behind the same gates; do not let it
  perturb replay paths.

## D-023 — Issue 14 scope corrections: the console belongs to `HexagonGame`, and the C++ metadata pipeline is compiled OUT by default

Both found by reading the source while preparing issue 14 (D-012: source beats contract).

**(a) The console is HEXAGONGAME's, not MenuGame's.** The 14 contract's file table says
`crates/oh-menu/src/lua_console.rs`. Wrong: every piece of it lives on `HexagonGame`
— the state (`ilcCmdLog`, `ilcHistory`, `ilcHistoryPos`, `ilcCmdBuffer`,
`ilcTrackBuffer`, `ilcShowConsole`, `ilcShowConsoleNext`, `ilcLuaTracked`,
`ilcLuaTrackedNames`, `ilcLuaTrackedResults`, `ilcDeltaClock`:
`include/.../HexagonGame.hpp:108-118`), the methods
(`postUpdate_ImguiLuaConsole` `HGUpdate.cpp:1287`, `ilcTextEditCallback` `:1149`,
`drawImguiLuaConsole` `HGGraphics.cpp:318`), and the gate
(`imguiLuaConsoleHasInput()` `HexagonGame.cpp:272` gating `HexagonGame::updateInput`
`HGUpdate.cpp:649`). `MenuGame.cpp` has no `ilc*`/imgui-console code at all.
⇒ the port belongs in `crates/oh-game` (plus the `oh-app`/render wiring), NOT `oh-menu`.

**(b) The metadata collection is a BUILD FLAG that is OFF.** `CMakeLists.txt:11-17` is
`if(FALSE) ... set(SSVOH_PRODUCE_LUA_METADATA TRUE) ... else() set(... FALSE) endif()`.
Every part of the pipeline is `#ifdef SSVOH_PRODUCE_LUA_METADATA`:
`LuaMetadataProxy::~LuaMetadataProxy` body (`LuaMetadataProxy.cpp:160-179` — with the
flag off the destructor body is **empty**, so `addFnEntry` is NEVER called),
`arg()`/`doc()` (returning without storing), and the type-to-string machinery
(`LuaMetadataProxy.hpp:48,97,105`). Consequently, **in the shipped build
`getMetadata()` has zero entries**: `printDocs` prints 11 category headers and no
function entries, `getAllFunctionNames()` returns an empty vector, and
`getDocsForFunction(name)` returns `"UNKNOWN FUNCTION"` for every name — so the
console's `?fn` command and `!help`'s doc lookup are inert as shipped.
- **This is why `_RELEASE/luadocs.md` is a 2021 artifact and cannot be reproduced**
  (D-021): it was captured from a build made while the flag was still `TRUE`. The
  flag was disabled later; the capture was never regenerated.
- **Consequence for issue 14:** port BOTH behaviours, and make the production side a
  **build-time "produce metadata" mode**, faithful to `#ifdef`. The default (flag off)
  must yield the empty-metadata behaviour — printing the 11 headers only, an empty
  name list, and `"UNKNOWN FUNCTION"`. The flag-on mode must produce the real docs (the
  `oh-lua` `LuaMetadata` infrastructure for it already exists: `add_fn_entry`,
  `for_fn_entries`, `PREFIX_HEADERS` (11, verbatim), `resolve_arg_names`).
  Do NOT pin output against `_RELEASE/luadocs.md`, and do NOT invent a function-count
  expectation for the default build — the default build documents **zero** functions.
- **`print_docs` is reachable headlessly** (`mainPrintLuaDocs`, `main.cpp:167-189`:
  headless assets + a null-window `HexagonGame` + `initLuaAndPrintDocs`), so it IS
  testable in this port without a window. The console itself is window-gated and
  debug-gated, so it does not touch replay determinism.

## D-024 — The metadata divergence is real but dev-tool-only; fix it in issue 14 (not now)

- **The divergence:** `oh-lua`'s `LuaMetadataProxy::drop` (`metadata.rs:279-286`)
  calls `get_metadata().add_fn_entry(...)` **unconditionally**, i.e. it ports the
  C++ **flag-ON** behaviour, while the shipped C++ build (D-023: the flag is
  `FALSE`) records **nothing**. `crates/oh-lua/tests/runtime.rs:651`
  (`metadata_entries_recorded_with_docs_and_args`) asserts the flag-ON behaviour,
  and that test is correct for the infrastructure — the collector itself is
  faithfully ported.
- **Severity: low, and deliberately deferred.** Verified that nothing in
  `crates/oh-game/src` or the 285 API lambdas reads the metadata store — its only
  two consumers are the Lua console's tab completion (`HGUpdate.cpp:1178`,
  `getAllFunctionNames`) and its `?fn` command (`:1509`, `getDocsForFunction`),
  both of which live in issue 14 and are debug/window-gated (D-022/D-023). So the
  divergence cannot affect replay determinism, the 285-name API surface, or any
  landed issue's tests; changing it now would churn `oh-lua` for no behavioural
  gain and would have to be undone by 14 anyway.
- **Decision:** issue 14 owns making the *presentation* honour the flag. The
  cleanest faithful shape: keep the collector as-is (it IS the flag-on pipeline)
  and put the flag where the C++ puts it — at the point of use — so the DEFAULT
  build behaves as D-023 describes: `print_docs` emits the 11 category headers and
  **zero** entries, `get_all_function_names()` is empty, `get_docs_for_function`
  returns `"UNKNOWN FUNCTION"`; and the produce-metadata mode yields the real docs.
  Whether that is a cargo feature, a runtime toggle on the metadata store, or a
  build-time constant is issue 14's call — record what you chose and why.
- **Rule:** a divergence found while preparing an issue belongs in that issue's
  record (D-012 family) as long as it is proven not to affect landed work. This one
  is recorded rather than fixed so the finding is not lost.

## D-025 — Worktree lanes land as SQUASHED single commits; harvest the branch ref immediately on completion

- **User directive (2024): "squash don't merge"** for landing worker lanes. Each
  worktree lane's branch is squashed into ONE commit on master (the lane's
  internal milestone commits remain visible in the worker's report; master gets
  a single reviewed, gated commit). First applied to the 15c wiring lane
  (`e2f4f49b`, squashed from `69166be6..5973df02`).
- **Runner ref-cleanup hazard (observed twice):** when a worktree run ends —
  success OR failure — the runner deletes the worktree AND its branch ref. On
  failure that destroys uncommitted work permanently; on success the commits
  survive only as dangling objects (worktrees share the main object DB). Both
  times `git cat-file -t <hash>` + `git merge-base --is-ancestor` + ff/merge
  recovered the work. RULE: on every worker completion notification, harvest
  the reported commit hashes IMMEDIATELY (`git cat-file -t`, then
  `git merge --squash`).
- Timeouts: the config default (30 min) killed five lanes with uncommitted
  work; all dispatches now pass explicit `timeoutMs: 5400000`.

## D-026 — Android port decisions

- **aarch64-only first**: LuaJIT on armv7 lacks GC64 (compatibility swamp);
  x86_64-android is emulator-only. armv7 documented as a follow-up if wanted.
- **native-activity** over game-activity (winit 0.30 + android-activity 0.6.1
  requires one of the two features; native-activity is the smaller surface).
- **API 28** target (GameActivity-free, covers modern devices; the NDK per-API
  clang wrapper doubles as the linker — `.cargo/config.toml` +
  `tools/android-env.sh` for the cc-rs CC/AR vars; cargo-ndk's `-- cargo`
  delegation mis-runs in this environment, so the manual config is canonical).
- **Storage**: `_RELEASE` ships inside the APK assets and extracts to the
  app-internal dir on first run; the composition resolves the game root
  there (Android has no CWD).
- **Shader risk flagged**: pack `.frag` files have no `#version` (desktop
  defaults, `gl_FragColor`) — GLES 3.0 needs a translation preamble; the
  render lane must assess (SFML did this internally on Android).

## D-027 — windows-testing integration (2026-10-16)

The `windows-testing` branch (upstream, two commits by Gary Trakhman,
verified on a live NVIDIA GL 3.3 display) was cherry-picked onto master
with authorship preserved (`8e461c90`, `3bf42b7d`) and reconciled with
master's own on-device fixes:

- **Same defects found twice** (independent discovery, kept once):
  the never-wired back-buffer swap (windows: `Window::swap_buffers`
  delegating to master's `GlParts::present`; one implementation), the
  never-set `ControlFlow` (master's Poll + windows' WaitUntil FPS pacing),
  and the 12-byte `sf::Vertex` VAO layout (25c lane's SG5 audit — the
  windows branch's `403b76e5` carries the same fix).
- **Taken from windows-testing** (master lacked them): the WGL
  `build(Some(window_handle))` context-creation fix (the desktop window
  never appeared on Windows), the NDC-y negation (the whole scene was
  vertically mirrored), the menu camera centering (`w/2,h/2` — the C++
  `sf::View{FloatRect}` semantics), the chdir-to-executable CWD
  resolution, and `gl_text.rs` (the display-verified text pipeline on
  `GlParts`).
- Reconciliation with the in-flight 25c lane (its own text/atlas
  implementation) is decided at 25c's landing: one text pipeline wins,
  the other is retired with its tests.

## D-028 — wasm/web investigation opened (issue 26)

`26-wasm-web.md` is the investigation record. Decision requests for the
user: (1) Luau instead of LuaJIT on web only (behavioral compat — LuaJIT
does not run on wasm); (2) miniz_oxide replay deflate on web only (the
D-006 zlib pin is system-zlib, libz-sys does not build on wasm);
(3) v1 scope without net/steam/discord/audio. The main refactor is an
`oh-vfs` trait (fetch-into-memory VFS on web) routing oh-assets/oh-config.

## D-029 — wasm/web work happens off master (user-driven)

The user is building the wasm/web port on a separate branch with a
different agent (2026-10-16). Master/orchestration lanes stay on the
Android port + the native trunk; `26-wasm-web.md` (D-028) is the reference
spec on master. Lanes must NOT create wasm branches or start the 26x
sub-lanes without an explicit go — any web-side refactor that would touch
shared crates (the proposed `oh-vfs` routing of oh-assets/oh-config) is
the user's branch's call, and master-side changes there need coordination.

## D-030 — issue 25 (Android) LANDED; real hardware replaced the emulator

Sub-lanes 25a (workspace aarch64 check), 25b (android_main + EGL/GLES +
assets + touch + APK), 25c (the GL draw passes: sprite/text/rectangle —
the recovered lane plus the on-device fixes), 25d (the boot-screen input:
the android Enter shape + the C++ TextEntered dispatch) are all landed and
verified on a moto g power 2021 (Android 11, arm64, WiFi adb
192.168.10.138:38721): fresh install -> boot screens (tap AND Enter) ->
profile creation (text entry) -> main menu, with the full GL render path
(menu text, bars, hexagon) confirmed by screenshot.

Emulator policy stands (D-026/29 lineage): the emulator is torn down; its
software-only bounded launcher (`tools/infra/emulator-start.sh`) remains
a contingency (the 28.8 GB gfxstream OOM and the CUDA-collision incident).
Real hardware is the sanctioned on-device verification target. The
25c-failure postmortem's worker hygiene (commit every ~10-15 tool calls,
histograms over image reads, stop-and-report on long sessions) is part of
every 025x spec from 025d on and worked: 025d finished in 1/3 the context
of the lane that died.

## D-031 — Android text entry: the IME route first (the C++ has no reference)

The C++ has NO Android text-entry implementation to transcribe: its
`SSVOH_ANDROID` guards only exclude Discord/Steam/Imgui, and text input came
from SFML's own Android layer (the IME wiring lived in SFML, not in this
repo). So the port needs a deliberate platform addition — a decision, not a
transcription.

Route chosen (in order):
1. **The system IME** via `AndroidApp::show_soft_input(force=true)` /
   `hide_soft_input`, called on the game's own text-entry transitions (the
   already-ported `isEnteringText()` gate gives exact enter/leave points),
   plus the activity's `windowSoftInputMode` (`stateVisible|adjustResize` —
   cargo-apk 0.10's activity metadata has no such field, so it goes through
   the `[package.metadata.android] android_manifest` manifest override or a
   custom manifest). Characters then arrive the way they already do:
   android-activity's `TextInputState` → the `KeyboardInput.text` field →
   the 025d `TextEntered` sink → `enteredChars` (that half is DONE and
   verified on device by 025d).
2. **Fallback, only if (1) fails on hardware** (IME shows but no characters
   arrive — a known NativeActivity/InputConnection hazard): a small in-game
   touch keyboard drawn with the existing GL text/rectangle passes and
   driven by the existing touch pipeline. Higher cost, zero platform risk,
   and it would also serve desktop-less builds. The operator explicitly
   accepts either outcome, so (1) is tried first and the fallback is
   escalated on evidence rather than assumed.

## D-032 — Input zones are platform-neutral by design (wasm + desktop mouse parity)

The 9-zone touch grid (the C++ `MenuGame::update()` block, MenuGame.cpp:2795-2835,
which our 25b lane dropped as an "out of scope" deviation — see D-033) must be
implemented in the MENU layer (`crates/oh-menu`), reading a finger-position/count
seam, never in the android/winit layer. Reason (operator): the same zone behavior
is wanted in the WASM build and on DESKTOP via mouse events, so the zone logic has
to sit where all three platforms can feed it.

Consequences:
- `oh-menu` owns the grid math + dispatch (ok/left/right/up/down), 1:1 with the
  C++, including the one-finger rule and the `touchDelay` debounce.
- Platforms provide finger positions: android touch (window seam, done in 25b SG5),
  desktop — a later step, mouse press/release mapped to a synthetic single finger,
  gated so it cannot double-fire with the existing menu mouse handling,
  wasm — same seam when the wasm branch merges (D-029).
- Any future platform input work must feed this seam rather than adding
  platform branches inside the menu.

## D-033 — Deferred lane plan (operator-ordered queue)

Recorded 2026-10-16 as the intake for work that is deliberately NOT
started yet, so nothing lives only in conversation:

1. **028a deviations inventory** — IN FLIGHT (worktree d75260e1).
2. **025f the 9-zone grid + IME edge** — IN FLIGHT (worktree 5456744c).
3. **028b deviations triage + `tools/check-deviations.sh`** — QUEUED.
   Gate-wired check that no deviation marker lacks a register row.
4. **28c… the FIX list** — QUEUED, one lane per cluster, load-bearing
   first; every lane must add its register row in the same commit
   (issue 28's standing rule).
5. **Desktop + wasm mouse→finger parity for the zone grid** — QUEUED
   (D-032). Desktop: mouse press/release → a synthetic single finger,
   gated so it cannot double-fire with existing menu mouse handling.
   wasm: same seam, after the wasm branch merges (D-029).
6. **A full playable-run proof on the phone** — QUEUED (LEVEL SELECT →
   level start → hexagon/timer/score; the touch-zone work unblocks level
   navigation). This is the last known Android gap.
7. **Mapping-doc rows for the SFML-fork Android reference** — QUEUED
   (issue 27's guard): `mapping/sfml-android-fork.md` exists; the
   `TextEntered`-derivation divergence and the touch/zone reference need
   rows in `mapping/rendering-menu.md` too.

## D-034 — Input/window deviations are FIXES, not accepted gaps

Operator ruling (2026-10-16): the Tier-1/Tier-2 deviations found by the
orchestrator's sweep have **no good reason to exist** and are queued as
fixes, not accepted. The register is therefore a FIX LIST, and the audit is
extended from marker-grep to real code coverage (issue 29).

Queued fixes, in dispatch order (each lands with its register row in the
same commit — issue 28's standing rule):

| Order | Lane | Deviation | Notes |
|---|---|---|---|
| 1 | 025f | 9-zone touch grid | IN FLIGHT |
| 2 | 029j | **Gamepad/gamepad dispatch** — `gilrs` 0.11 poller feeding the already-ported `oh-config` joystick state machine + the 11 `Joystick::risingEdge` handlers (MenuGame.cpp:2737-2782) + the HexagonGame joystick sites | biggest real gap: a controller currently does nothing |
| 3 | 029m | **Menu mouse hover** (`mouseHovering`, 4 C++ sites) — gates left/right actions | mouse UX |
| 4 | 029w | **Window (re)creation** (`mustRecreate`→`recreateWindow`, GameWindow.hpp:123,179-190) for fullscreen/size/AA/vsync changes | options-menu visible |
| 5 | 029f | **Favorites UI** (drawer + toggle, MenuGame.cpp:4972,5198) | feature completeness |
| 6 | 029t | `GameTimer::refresh` audit (who sets `nextImpl`) | correctness |
| 7 | 028c… | the remaining FIX list from the triaged register | risk-ranked |

Operator directive also: after the fixes, **finish the wasm branch**.

## D-035 — No worktrees for lanes; single-writer discipline (operator directive)
**AMENDED (operator directive, 2026-10-17): lanes run in PERSISTENT WORKTREES.**
The single-shared-checkout rule below is superseded for parallel lanes. One
stable branch + worktree per lane (`tools/lane-worktree.sh create <lane>` →
`../oh-lanes/<lane>`, branch `lane/<lane>`), giving each lane its own files and
its own index. The operator's reason: the earlier worktree pain came from
*ephemeral* worktrees whose refs were deleted while lanes were live (four
dangling-commit recoveries); a **persistent** branch/worktree per lane has
neither that failure mode nor the shared-index hazard (an orchestrator
`git add -A` once swept a lane's staged edits and deleted
`build/copyArtifacts.bash` from the C++ reference tree).

Disciplines that make it safe:
- one writer per worktree; a lane never touches the main checkout;
- the orchestrator lands a lane by squash/cherry-pick on master (D-025) and only
  then may remove the worktree — `lane-worktree.sh remove` REFUSES unless the
  branch is merged, and nothing is force-deleted while a lane is alive;
- each lane gets its own `CARGO_TARGET_DIR` (separate worktrees have different
  absolute source paths, so a shared target dir would thrash cargo's
  fingerprints), with `CARGO_PROFILE_*_DEBUG=0` because the main target/ is 63 GB
  of mostly debug info and three of those would fill the disk;
- prefer targeted `cargo test -p <crate>` while lanes run; the orchestrator runs
  the authoritative full gate when the tree is quiescent.

The original text follows for the record.


Operator directive (2026-10-16): **do not use worktrees going forward.**
Workers run in the main checkout. Consequences, which the orchestrator must
enforce:

- **One writer at a time.** Never dispatch two lanes that both touch the
  repo. Read-only lanes (scouts/reviewers reading files) may run alongside
  a writer only if they never write; the audit workflow's children are
  read-only except their own report file, and even that must be serialized
  against a writer's commit.
- **Commit early, always.** With no worktree the lane's uncommitted work
  sits in the shared tree, so a timeout/crash can collide with the
  orchestrator's own commits. Lanes must commit every few tool calls.
- **Recovery is simpler**: dangling commits + `git status` in the main tree
  replace worktree branches; the runner's `worktree-diffs` artifact no
  longer applies.
- Landing: squash into master (D-025) still applies for multi-commit lanes.

## D-036 — Web target: `wasm32-unknown-emscripten` for the whole app (issue 026 clean-restart lane)

The clean-restart web port (spec `.agents/tasks/issue-026-web.md`, on `wasm`
over `origin/master`) settled these:

- **Target = `wasm32-unknown-emscripten` for the whole app; `winit` dropped on
  web.** `winit` 0.30's web backend is `target_os = "unknown"`-only and cannot
  target emscripten, so the web build uses a dedicated canvas +
  `emscripten_webgl_*` context + `glow::Context::from_loader_function_cstr`
  backend (isolated in `oh-render`), with `winit`/`egui`/`egui-winit`/
  `egui_glow`/`glow0.17`/`signal-hook` gated to `not(emscripten)`.
- **Content via MEMFS preload, NOT a VFS refactor.**
  `--preload-file=_RELEASE@/RELEASE` + `-sFORCE_FILESYSTEM=1` makes
  emscripten's MEMFS serve the game root, so master's existing `std::fs`
  content path works on web unchanged — the 26a `oh-vfs` trait is DROPPED (its
  whole research branch stays parked in `wasm-backup-prewas`).
- **Lua backend selectable (Luau on web, LuaJIT desktop).** `oh-lua` picks the
  mlua backend by target: `luajit` on desktop, `luau` on emscripten (Luau/C++
  is the only Lua that builds on emscripten); `mlua::Integer` width fixed for
  the Luau build. Global-Luau remains a follow-up experiment.
- **Profile-seed workaround for 29b6/29c1.** Master's profile/save system is a
  known no-op (tracker 29b6/29c1), which blocks boot at PROFILE CREATION on
  web; `oh-web` seeds a default profile into MEMFS as a documented prototype
  workaround so the game boots to the menu.

- **Evidence:**
  - `winit`'s `target_os = "unknown"` web gate (its web backend cannot target
    emscripten) — forces the canvas/WebGL backend.
  - `wasm-bindgen` now supports emscripten: the crate must be a **bin** and
    link with `-sWASM_BINDGEN` (emcc then runs the wasm-bindgen CLI).
  - The two mandatory gotchas: (a) `-sDEFAULT_TO_CXX` (link the C++ EH
    runtime); (b) call `__wasm_call_ctors()` as the FIRST statement of
    `main()` (or Luau traps — the `-sWASM_BINDGEN` glue runs Rust `main`
    before the C++ static ctors). Full recipe + gotchas:
    `.agents/skills/lib-web-stack.md` §26c.
- **Landed by:** issue 026 (W0 `29f3824e`, W1 `dad120b8`+`966f7637`, W2
  `324e2527`, W3 `f6906368`, W5 `30c0dab1`, W6 `352a8ae0`; plan `56747b2f`).
  Browser-verified: the menu loading screen renders (PACKS 8 / LEVELS 37 /
  ASSETS 61 / NO LOAD ERRORS). Not yet verified: menu → level select → a
  level. Desktop/Android unaffected (every gate `cfg`-only).

**AMENDED (issue 026 integration onto master `86ea8eb2`)** — two parts were
superseded by master work that landed after this decision was written:

- **Lua backend selection**: the per-target mlua pinning is replaced by
  030a's `backend-*` features ("features first, target default after"): the
  web build is `cargo build -p oh-web --target wasm32-unknown-emscripten
  --no-default-features --features backend-luau` (the features are forwarded
  `oh-web -> oh-app -> oh-lua`); desktop keeps the `backend-luajit` default
  (029k1's vendored FFI-off LuaJIT). The split is temporary — 030c's
  three-way parity proof may unify on Luau, which deletes it by construction
  (issue 30 §12's operator direction).
- **Profile-seed workaround (REMOVED)**: 029c1 landed the real profile system
  (`Assets::profile_create` writes `Profiles/<name>.json` + reloads the store,
  `ProfileSaveGuard`'s exit save) — on web `std::fs` is emscripten MEMFS, so
  the REAL first-run path works in-browser (a fresh user boots to the C++'s
  own PROFILE CREATION state; a created profile persists for the session).
  `oh-web` no longer seeds a default profile (the seed skipped the C++
  first-run state — a deviation).

## D-037 — Unify on Luau: the LuaJIT machinery goes away (operator directive, 2026-10-17)

Operator: *"will we still keep parity with c++ without rust luajit built in? if so, go
ahead"* — and the answer is YES, on direct evidence rather than by transitivity.

**The decision.** The port ships ONE Lua VM: **Luau** (mlua's `luau` feature, statically
built from `luau0-src`). `backend-luajit`, `extlibs/luajit-src`, the root
`[patch.crates-io]`, the `ffi_stub.c` link shim, the Android `cshim.c` `__clear_cache`
shim, and the per-target Lua selection are all deleted. This **supersedes D-036's
per-target split** (LuaJIT desktop / Luau emscripten) — there is nothing left to split.

**Why parity survives.** All three pieces of evidence are already landed:
- The C++ oracle (030b) fingerprints **game state** — RNG state, walls, player, score,
  customScore, status timers, entities — NOT Lua internals. It is VM-agnostic by
  construction and keeps working unchanged after the switch.
- The Luau column was verified against the C++ **directly**: 030c ran the committed
  `cpp_fingerprints.json` (6 cases, **3627 frames**) under `backend-luau` and every frame
  matched — the same result the LuaJIT column produced. So the configuration we will SHIP
  is the configuration that is compared against the C++, which is *stronger* than shipping
  LuaJIT and inferring.
- Every measured VM difference is non-observable in gameplay: 030a's probes put the
  divergence surface at error chunk prefixes, number spellings, sandbox presence and
  Integer width — none of them fingerprinted — while `%.14g` and number→string, the two
  that could reach game-visible text, are **identical**. `math.random` is engine-shimmed
  and deterministic, and replays store input bits plus engine RNG, so they are
  VM-independent (43/43 cases and 3627 frames both confirm it).

**Residual risks, named rather than buried.** (1) **Third-party content** — there is no
such corpus in this repo; mitigated by the zero census, by Luau 0.663 keeping most 5.1-isms
natively (030c's per-ism classification), by the compat prelude, and by a one-line Rust
binding for `loadstring` — the single ism that is not Lua-emulatable (Luau's base library
has no chunk compiler at all). (2) **`pairs`/`next` iteration order is genuinely
VM-dependent**, so a third-party level that DEPENDS on iteration order could diverge from
the C++'s LuaJIT (such a level is already fragile — both VMs may vary it — but this is a
real hole and belongs in the record). (3) Error text and tracebacks differ: logs and the
console only, never gameplay or replays.

**Deliberate casualty.** 030a's A/B harness loses its LuaJIT arm, because keeping that arm
alive means keeping the machinery this decision exists to delete. The harness survives as
regression pins against the C++ vectors (030c's per-case table), plus the dialect census,
the VM probes and the prelude tests. The ground truth becomes the C++ column.

**Consequences closed by this.** **K3** (the arm64 `lj_asm_trace` ANR — the operator's own
"hangs non-responsive") and **K4** (LuaJIT version skew) become moot BY CONSTRUCTION, and
deviation **F29** (the `jit.off()` mitigation) is deleted with them: the shipping
configuration has no `jit` table left to disable. The Android `__clear_cache` shim goes too.

## D-038 — The C++ tree lives in `cpp-original/`; root `src`/`include` symlinks keep the citation corpus resolving (038 reorg)

- **Question:** the operator asked to "move all the c++ folder structure into a
  cpp-original subfolder" — while (a) the C++ tree is the citation reference
  (D-014; `.agents/audits/*.md` are IMMUTABLE, so the paths they cite must keep
  resolving), (b) the oracle builds it in a container from the mounted tree, and
  (c) `_RELEASE/` is shared game data the Rust tests / `tools/package.sh` / the
  release layout read at the ROOT.
- **Decision (the layout):**
  1. Every upstream/C++ entry moved into `cpp-original/`: `src/`, `include/`,
     `extlibs/` (git submodules; the `.gitmodules` paths moved with the gitlinks
     in the same commit), `CMakeLists.txt`, `build.sh`, `build/`, `buildlx/`,
     `buildrel/`, `clangformat_everthing.sh`, `prepare_release*.sh`,
     `run_prepared_release_test.sh`, `test/` (CMake `add_subdirectory`),
     `public/` (CMake source glob), `art/` (CMake icon `.res`), `old/`, `misc/`,
     `release/`, `server/`, `vbox/`, `webpage/`, `.clang-format`, and the
     upstream `README.md`. `ISSUE-019-REPORT.md` is port-authored: it lives
     under `.agents/`.
  2. **TWO root citation symlinks — `src -> cpp-original/src` and
     `include -> cpp-original/include` — and only those** (the orchestrator
     ruled the set minimal; a third for the legacy C++ CI was declined). The
     ~344 live `src/SSVOpenHexagon/…` / `include/SSVOpenHexagon/…` citations
     (plans 133, crates 117, audits 52, tasks 42 at this HEAD) keep resolving —
     that is the load, and it includes the 52 audit citations that cannot be
     rewritten (hygiene §7).
  3. **`cpp-original/_RELEASE -> ../_RELEASE` is C++-SIDE BUILD GLUE, not a
     citation symlink and NOT a second source of truth for the game data.** The
     real `_RELEASE/` is the root directory, untouched; the moved
     `CMakeLists.txt` links `${CMAKE_SOURCE_DIR}/_RELEASE/libsteam_api.so` (+
     `libdiscord_game_sdk.so`, `libsdkencryptedappticket.so` — the tracked
     loader stubs) on every non-Android build, so the moved tree cannot link
     without the pointer, and the CMakeLists content may not be edited (move,
     not edit). Writes through it land in the real root `_RELEASE/`
     (write-through verified); the C++-installed binary set
     (`SSVOpenHexagon`, `SSVOpenHexagonLinux`, `OHWorkshopUploader*`,
     `OHServerControl*`) is gitignored, so a C++ build cannot leave
     commit-able artifacts in the data dir.
- **Evidence (all at the new path, 2026-10-18):** gate green (5/5 stages);
  `tools/parity.sh` green (probe compiles the real moved sources; C++ corpus
  byte-identical); the oracle end to end — `init` (5 submodules at
  `cpp-original/extlibs/*`), `seed` (6 CPM pins), `configure`
  (`cmake -S /src/cpp-original`), `instrument` (`git apply` gains
  `--directory=cpp-original`: it refuses to follow the root symlinks,
  "beyond a symbolic link" — so `fingerprint.patch` stays 030b's untouched
  artifact), full instrumented build, and all six fingerprint replays:
  **3627/3627 frames match the committed C++ column**
  (`.agents/plans/001-rust-port/parity/cpp_fingerprints.json`), then
  deinstrumented back to pristine. Symlinks verified for a plain read,
  `grep -r`, and from inside the oracle container's mount
  (`readlink -f /src/cpp-original/_RELEASE` = `/src/_RELEASE`;
  `grep -rln "namespace hg" /src/include` = 79 files = the real path's count).
  A 20-citation random sample (116 distinct citation-form files, `shuf -n 20`,
  `test -f` from the root) resolves 20/20.
- **Known consequences (recorded, not papered over):** citations of other
  forms no longer resolve from the root — `extlibs/…` (63), `public/…` (16),
  `test/*.t.cpp|hpp` (28), `CMakeLists.txt` (25), `prepare_release*.sh` (2),
  `build/copyArtifacts.bash` (1) — every such file exists under
  `cpp-original/` (the census method is in the task report). The upstream C++
  CI (`.github/workflows/ci_linux.yml`, `ci_windows.yml` — the C++ project's
  own workflows, not the port's) assumes a root `CMakeLists.txt` + `cd build`;
  that is lane 037's territory and the operator's to fix ("actions should work
  out"). Windows-runner symlink materialisation for the two root symlinks:
  UNVERIFIED (no Windows checkout exercised).
- **Landed by:** issue 038 (lane 038, branch `lane/038`).

## D-039 — Web persistence: user data in IndexedDB through a small IDBFS mount, never over `/RELEASE` (issue 048)

- **Question:** the operator asked "is data persisted in localstorage on the
  wasm build?" — the verified answer before writing the issue was **no,
  nothing persists at all** (the base dir `/RELEASE` is the `_RELEASE` tree
  preloaded into emscripten MEMFS; every profile, score and replay died on
  reload).
- **Decision:**
  1. The tool is emscripten **IDBFS** (the IndexedDB-backed filesystem),
     NOT `localStorage` (~5 MB, synchronous, string-only — and not what
     anything here uses). IDBFS keeps the port's existing `std::fs` code
     path intact — D-036's "MEMFS preload, NOT a VFS refactor" decision
     still holds.
  2. The mount is a small `/userdata` directory, **NOT `/RELEASE`**: the 22
     MB read-only `_RELEASE` content stays in MEMFS. A persisted `_RELEASE`
     would be STALE after the next deployment (a returning player would run
     an old `_RELEASE` against a new `oh_web.wasm`) — a real hazard, not a
     theoretical one.
  3. What persists: the base dir's THREE writable paths — `config.json`,
     `Profiles/`, `Replays/` (the full write-census of the base dir; the
     desktop-only `workshopCache.json` / `ohdb.sqlite` are absent on web) —
     each re-pointed at the mount with one `FS.symlink`. The on-disk FORMATS
     are untouched (pinned by parity; persistence re-points the paths, never
     the bytes). First run: the shipped `config.json` seeds `/userdata` — a
     later deploy's shipped config never clobbers the user's own (the C++'s
     user-config-shadows-shipped shape).
  4. Boot ordering: IndexedDB is ASYNCHRONOUS — `main` calls the page's
     `ohWebPersistBoot` glue (mount + the `FS.syncfs` LOAD + the symlinks)
     and boots the composition FROM THE GLUE'S CALLBACK (a `dyn Fn`
     `Closure` + one-shot latch — wasm-bindgen has no `FnOnce` JS closures;
     ownership transfers to the JS GC via `into_js_value`). A load FAILURE
     degrades to the session-only MEMFS (the game still boots — the C++ has
     no durable storage either; this is a platform addition, not a
     transcription).
  5. The way-out: the page's `pagehide` / `visibilitychange` glue calls the
     `save_session` export — the C++ `mainClient` exit-save (the `pSaveAll`
     + `saveConfig` pair, main.cpp:285-291 / :382-388) RE-FIRED at the
     session's real end — and flushes the mount with `FS.syncfs(false)`.
     The re-fire closes the D-029 gap: on web the rAF loop returns, so the
     C++'s exit guards drop at composition (with the boot-time values) and
     an in-session config change would never reach `config.json`. The
     operator tests on ANDROID CHROME, which discards backgrounded tabs —
     `visibilitychange(hidden)` fires before the discard, so a "sync at the
     end of main" design would have lost the data on a phone.
- **Evidence (2026-10-18, at `lane/048`):** `tools/web-persist-probe.mjs`
  (a headless-Chrome CDP driver; the task's acceptance): PHASE 1 — a fresh
  user boots to the C++'s own first-run state (`menu:ETLPNewBoot`), creates
  profile `probe`, toggles `autorestart` in MOpts→GAMEPLAY, dispatches a
  synthetic `pagehide` — the IndexedDB inventory (database `/userdata`,
  store `FILE_DATA`, the emscripten 6.0.12 IDBFS names) holds EXACTLY 4
  entries: the profile (123 B), `config.json` (7952 B,
  `"auto_restart": true`), the `Profiles` + `Replays` dirs — NO `_RELEASE`
  content (the NOT-mounting-`/RELEASE` property as a fact, not a promise).
  PHASE 2 (`Page.reload` — the MEMFS is re-preloaded, only IndexedDB
  survives): the boot lands on `menu:SLPSelectBoot` (NOT `ETLPNewBoot` —
  the profile persisted and the boot skipped name entry; the screenshot
  shows `SELECT LOCAL PROFILE` + `PROBE`) and `config.json` read through
  the engine's own symlinked path still says `"auto_restart": true`.
  14/14 checkpoints, exit 0. Host gate 5/5; the wasm build (emsdk
  container, the root `.cargo/config.toml`'s recipe + `-lidbfs.js` +
  `-sEXPORTED_RUNTIME_METHODS=FS,IDBFS`) and `tools/check-android.sh`
  green.
- **Known consequences (recorded, not papered over):** the probe's input
  must look HUMAN (a tap held across frames — the C++'s menu binds are the
  SSVStart Manager's per-frame trigger polling, so a single-frame tap
  leaves no trace, the C++'s own 025c-documented behavior; and the
  boot-screen-close key must not be a typeable letter — the C++'s
  `playLocally()` clears only `enteredStr`, so a `w` close leaks a `w` into
  the fresh profile name — the C++ leaks it too). Lane 046 (the web IME +
  the operator's fullscreen button) also works on `site/index.html`: this
  lane's page change is ADDITIVE inside the existing module script (the
  `ohWebPersistBoot` glue + the `globalThis.ohWebModule` hook + the way-out
  listeners); if 046's lines land, the merge must keep both.
- **Landed by:** issue 048 (lane 048, branch `lane/048`).
