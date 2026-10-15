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
