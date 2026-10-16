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

## D-028 — wasm/web: a bounded portability experiment (issue 26)

Stance (user, post-investigation): this is a **bounded portability
experiment**, NOT an approval of the whole port. The user approves **26a
(`oh-vfs`)** + a **narrow Phase 0 feasibility spike** and requires evidence
before committing to the rest. The real risk is **behavioral differences**,
not whether Rust compiles to wasm. Phase 1+ is gated on Phase 0 evidence; the
three decisions below are tracked **separately** (D-028a/b/c) so a stop can
isolate what to retain. The strongest reason to pursue: the VFS boundary +
platform separation may be useful beyond wasm. Track the **native regression
burden** of the VFS refactor explicitly. `26-wasm-web.md` is the record; the
decisive milestone is **running a representative game session in a browser**,
NOT `cargo check --target wasm32-unknown-unknown`.

### D-028a — Web Lua runtime (Luau): verified YES on `wasm32-unknown-emscripten`

- **Decision:** Luau (mlua `luau`) on web only; desktop stays LuaJIT-locked.
  Acceptable ONLY if script behavior is part of the compatibility contract
  **where explicitly tested**, and the `oh-parity` corpus can establish that
  boundary.
- **Contract split:** distinguish **API parity / semantic parity / runtime
  parity** (do not conflate them).
- **Lane 26b is a real gate** with three outcomes: **PASS** (corpus + targeted
  adversarial tests establish acceptable compatibility) / **CONDITIONAL PASS**
  (differences identified, bounded, explicitly documented) / **FAIL** (scripts
  rely on unreproducible behavior).
- **Do NOT assume** mlua's Luau backend builds/executes on
  `wasm32-unknown-unknown` — a minimal **compile-and-execute** test is
  required (Phase 0).
- **Phase 0 gate evidence (2026-10-16, P0.2):** the compile-and-execute probe
  was run on both installed wasm targets. **Luau does NOT build on
  `wasm32-unknown-unknown` at all** — `mlua-sys` (0.6.8) → `luau0-src
  0.12.3+luau663` needs a C++ standard library that target has no way to supply
  (`cc-rs: failed to find tool "clang++"`). `vendored` is a **no-op for Luau**
  (`mlua-sys/build/main_inner.rs` routes `any(feature="luau", feature="vendored")`
  to `find_vendored.rs`; `vendored = ["lua-src","luajit-src"]` does not gate
  `luau0-src`). On `wasm32-wasip1` with a host clang 22.1.8 it still fails at
  `fatal error: 'iterator' file not found` (no wasm C++ stdlib). The native
  control (`cargo run`) PASSES, so the script/backend is fine — **the blocker is
  purely a missing wasm C++ toolchain, not a Lua/Luau compatibility question.**
  The viable path is **`wasm32-wasip1` + wasi-sdk** (clang++ + wasi-libc +
  libc++); the alternative is **emscripten/emsdk** for `wasm32-unknown-unknown`.
   wasi-sdk / emsdk were not reachable in this environment (env limitation, not a
   code limitation).
- **26b gate evidence (2026-10-16): PASS on `wasm32-unknown-emscripten`.**
  emsdk 6.0.12 was provisioned and the compile-and-execute probe now **PASSES**
  on the **emscripten** target (superseding the "toolchain-gated / needs
  wasi-sdk" wording above — the viable target is emscripten, not
  `wasm32-wasip1`). A throwaway spike crate (record the *recipe*, not the temp
  path) with `mlua = { version = "0.10", features = ["luau", "vendored"] }` + a
  `main.rs` running a Luau script (`lua.load(<source string>)`, in-memory chunk
  — the path we need, since `.lua` bytes come from the vfs): `cargo build
  --target wasm32-unknown-emscripten` (clean ≈30 s) then
  `node target/wasm32-unknown-emscripten/debug/luau-emsdk-spike.js` printed
  `PASS: luau spike ok; ...` and exited **0** (Debug `.wasm` ≈ 15,732 KB; a
  failing assert prints `FAIL: ...` and exits 1). Versions: mlua 0.10.5,
  mlua-sys 0.6.8, luau0-src 0.12.3+luau663, rust 1.97.1, target
  `wasm32-unknown-emscripten`; emsdk 6.0.12 (emcc 6.0.12, clang 24, node
  24.19.0, python 3.13.3). **Mandatory build recipe** (every env var required):
  `EMSDK`/`EM_CONFIG`; `CC_wasm32_unknown_emscripten=emcc.exe`,
  `CXX_wasm32_unknown_emscripten=em++.exe`,
  `AR_wasm32_unknown_emscripten=llvm-ar.exe`;
  `CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions` (the key one); `PATH`
  must contain a dir with `emcc.bat`/`em++.bat` **name shims** (rustc's link
  step + the `cc` archiver look for the `.bat` names; emsdk 6.0.12 ships only
  `.exe`) plus `<emsdk>\upstream\emscripten`, `<emsdk>\upstream\bin`,
  `<emsdk>\node\...`; crate `.cargo/config.toml`
  `[target.wasm32-unknown-emscripten] rustflags = ["-C","link-arg=-sDEFAULT_TO_CXX"]`;
  and `vendored` is REQUIRED (mlua-sys `find_normal.rs` panics without it).
  **Why `-fwasm-exceptions` is mandatory:** rustc hard-links the emscripten
  target with `WASM_EXCEPTIONS` (an internal emcc setting, not CLI-overridable
  — `-sWASM_EXCEPTIONS=0` is rejected), while `luau0-src` compiles Luau C++ with
  a bare `-fexceptions` which in clang 24 selects **legacy JS exception
  handling** → undefined `__resumeException`/`llvm_eh_typeid_for` at link;
  compiling all C++ with `-fwasm-exceptions` aligns it with the link (the rule
  for ALL C++ in this target). **Staleness trap:** `mlua-sys` declares no
  `rerun-if-env-changed` for the compiler env vars → after changing
  `CXXFLAGS_wasm32_unknown_emscripten` run `cargo clean -p mlua-sys` once.
  **Caveats (record all):** (1) Luau **string interpolation** `` `{x}` `` is
  **INERT** (`LANG_STRING_INTERPO` not enabled in the vendored Luau —
  `"hello {name}!"` stayed literal); immaterial for the current corpus — a
  repo-wide scan of the 42 shipped `.lua` files found NO interpolation
  (backticks only inside comments). (2) `lua.load(<source string>)` (in-memory)
  WORKS (the path we need); `load_from_file`/file ops are assumed broken (mlua's
  own `// TODO: figure out why emscripten fails on file operations`). (3)
  JIT/codegen is excluded (`luau0-src` panics on emscripten if codegen is
  enabled) — OH does not use it. (4) This proves Luau runs under **node**;
  running the same artifact **in a real browser** is still unproven (a 26c
  item).
- **Status:** decided — **verified YES** on `wasm32-unknown-emscripten` (26b
  gate PASS, 2026-10-16, independently verified by the supervisor), with the
  mandatory recipe + the interpolation / `load_from_file` caveats above. The
  earlier "toolchain-gated, needs wasi-sdk/emscripten" wording is superseded.
  Lane 26b is complete; the remaining open item is **browser execution** (26c).
  Skill: `.agents/skills/lib-mlua.md` §WASM; `.agents/skills/lib-web-stack.md`.

### D-028b — Web replay compression (miniz_oxide): yes, with a clearer replay contract

- **Decision:** establish which contract the replay system actually promises
  before weakening anything.
- **Verified finding:** replays are exchanged as an **encrypted compressed
  blob** — `ClientToServer::CTSPCompressedReplay` → `send_encrypted`
  (`crates/oh-net/src/client.rs`); the server calls `decompress_replay_file`
  (`crates/oh-net/src/server.rs`). There is **no hash/signature/checksum over
  the compressed payload** in `oh-replay`/`oh-net` (the `levelValidator` is a
  separate field — used only for the supported-level gate + verbose log).
- **Contract:** therefore the promise is **"same replay semantics —
  decompression recovers the same logical replay"**, NOT byte-identity.
  Byte-identity is asserted only by the desktop oracle pins in `oh-parity`.
- **Do not weaken replay compatibility until a concrete incompatibility is
  demonstrated** — test the substitution (miniz_oxide compress ↔ zlib
  decompress and vice-versa) rather than assuming best-effort.
- **Phase 0 evidence (2026-10-16, P0.1): PASS.** The substitution was tested,
  not assumed. On the fixture `test/fixtures/cube_1_apeirogon.ohr.z` (a
  569-byte serialized replay): (a) the repo's zlib stream (101 B) →
  `miniz_oxide::inflate::decompress_to_vec_zlib` (miniz_oxide 0.9.1) → the exact
  569 bytes; (b) `miniz_oxide::deflate::compress_to_vec_zlib(.., 9)` (112 B) →
  the repo's `decompress_replay_file` → `ReplayFile ==` original, field-for-field.
  The compressed bytes differ (101 vs 112) — expected, since the contract is
  **lossless logical recovery, not byte-identity**; byte-identity remains a
  desktop-oracle property (pinned only by the C++ `compress2` oracle). miniz_oxide
  is therefore a safe web substitution for interchange. Skill:
  `.agents/skills/lib-flate2.md`.
- **Status:** decided (P0.1 PASS).

### D-028c — v1 scope: prototype yes; silent shipping release only if justified

- **Decision:** v1 = no networking / Steam / Discord; audio may be silent
  **only if justified**.
- **Framing:** treat v1 as a **feasibility prototype**, not a product
  commitment. Prove rendering + the game loop first (Phase 2 vertical slice).
- **Status:** decided.

## D-029 — Web target = `wasm32-unknown-emscripten` for the whole app; `winit` dropped on web

- **Decision:** the web build targets **`wasm32-unknown-emscripten`** for the
  WHOLE app — Lua + engine + `web-sys` + `glow` in ONE module — so the Lua VM
  and the engine share a module with direct calls (no cross-module bridge).
  **`winit` is dropped on the web path** (the event loop is a plain
  `request_animation_frame`). This supersedes the 26 plan's original
  "Build/ship shape", which assumed `wasm32-unknown-unknown` + winit + no
  emscripten.
- **Evidence (hard, 26c.1, 2026-10-16, supervisor-verified live in Chromium via
  `agent-browser`):**
  - A `wasm32-unknown-emscripten` module boots in a real browser, creates a
    WebGL2 context, constructs a working `glow` context, runs a
    `request_animation_frame` loop, and exposes a callable `#[wasm_bindgen]`
    export (console: raw-webgl2 clear OK, glow clear OK → `status()=glow-ok`,
    "render loop started"; the 640×480 canvas renders orange).
  - `winit 0.30.13` cannot build for emscripten: `build.rs:11` gates
    `web_platform = { all(target_family = "wasm", target_os = "unknown") }`;
    emscripten is `target_os="emscripten"` →
    `compile_error!("The platform you're compiling for is not supported by
    winit")`. winit-web is therefore **`wasm32-unknown-unknown`-only**.
  - `wasm-bindgen` NOW supports `wasm32-unknown-emscripten` (current docs; the
    old sunset-domain "no plans" page is STALE) — it must be a **`bin` crate**
    (a `cdylib` links as an Emscripten *side module* with no JS loader →
    undefined `__wbindgen_*`) plus **`-Clink-arg=-sWASM_BINDGEN`**; `emcc`
    then runs the `wasm-bindgen` CLI itself over the linked wasm. Requires
    Emscripten ≥ 6.0.10.
  - `glow 0.18` on emscripten compiles its **`native`** module (not
    `web_sys`), so `Context::from_webgl2_context` is unavailable; the working
    route is `glow::Context::from_loader_function_cstr(|n: &CStr|
    emscripten_webgl_get_proc_address(n.as_ptr()))` after creating + making
    current an emscripten WebGL2 context.
  - `mlua`-Luau requires emscripten (D-028a / 26b PASS) — so emscripten is the
    only target that hosts both the Lua VM and the render stack in one module.
- **Recipe (verbatim):** a **binary** crate (`src/main.rs`, no `[lib]`) with
  `rust-toolchain.toml` pinning `channel = "1.97.1"`; `.cargo/config.toml`
  `[build] target = "wasm32-unknown-emscripten"`,
  `[target.wasm32-unknown-emscripten] linker = "emcc.exe"` + rustflags
  `-Cpanic=abort`, `-Cllvm-args=-enable-emscripten-cxx-exceptions=0`,
  `-Crelocation-model=static`, `-Clink-arg=-sWASM_BINDGEN`,
  `-Clink-arg=-Wno-experimental`, `-Clink-arg=-sMODULARIZE`,
  `-Clink-arg=-sEXPORT_ES6`, `-Clink-arg=-sMAX_WEBGL_VERSION=2`,
  `-Clink-arg=-sMIN_WEBGL_VERSION=2`. Deps: `wasm-bindgen 0.2.129`,
  `js-sys 0.3.106`, `web-sys 0.3.106` (Window/Document/Element/HtmlElement/
  HtmlCanvasElement/WebGl2RenderingContext/console/Performance), `glow 0.18`.
  emsdk env (every build): `EMSDK`/`EM_CONFIG`;
  `CC/CXX/AR_wasm32_unknown_emscripten` = `emcc.exe`/`em++.exe`/`llvm-ar.exe`;
  `PATH` includes emsdk's `upstream\emscripten`, `upstream\bin`,
  `~/.cargo/bin` (for the `wasm-bindgen` CLI). emsdk 6.0.12 / node 24.19.0.
- **Gotchas (recorded):** (1) `-sMAX_WEBGL_VERSION=2` (and
  `-sMIN_WEBGL_VERSION=2`) are REQUIRED — without them emscripten logs
  `WebGL 2 requested but only WebGL 1 is supported` and
  `emscripten_webgl_create_context` returns 0. (2)
  `emscripten_webgl_create_context` takes a **CSS selector**, not a bare
  element id (`"#c"` works, `"c"` returns 0). The WebGL2 context must be
  created via the emscripten C API (`emscripten_webgl_init_context_attributes`
  with `major_version=2` → `emscripten_webgl_create_context` →
  `emscripten_webgl_make_context_current`) BEFORE
  `emscripten_webgl_get_proc_address` resolves.
- **Open risk (verify in 26c.3, do NOT assume):** the wasm-bindgen recipe's
  `-Cllvm-args=-enable-emscripten-cxx-exceptions=0` (needed for Rust
  `panic=abort`) may CONFLICT with Luau's C++ exceptions
  (`CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions`, mandatory per 26b)
  when both are linked into ONE module.
- **Implication (not a blocker):** Emscripten provides a libc + in-memory FS,
  so `std::fs`/`std::time`/`std::env` actually WORK on this target (content
  could be preloaded into MEMFS); the fetch→`MemoryVfs` remains the intended
  content path, but `RealFs` is not automatically excluded on emscripten.
- **26c.2 confirmation (2026-10-16, supervisor-verified live in Chromium via
  `agent-browser`):** the **single-module premise is verified** — Luau (mlua,
  vendored C++) + `wasm-bindgen`/`web-sys` + `glow` all link AND run in ONE
  `wasm32-unknown-emscripten` module (console: raw-webgl2 clear OK, glow clear
  OK → `glow-ok`, Luau `acc=9 d=42 q=3` → `luau-ok`; canvas renders orange; debug
  `.js` ≈ 317 KB, `.wasm` ≈ 19.5 MB). Two load-bearing facts the `oh-web`
  implementation must not rediscover: (1) **`-Clink-arg=-sDEFAULT_TO_CXX` is
  REQUIRED to link** — emcc's linker driver does NOT auto-link the C++ exception
  runtime (libcxxabi) for the mixed Rust+C++ module, so Luau's objects (built
  with `-fwasm-exceptions`) leave `__gxx_wasm_personality_v0`, `__cxa_throw`,
  `__cxa_begin_catch`, `std::terminate`, `typeinfo for std::exception`,
  `operator delete`… undefined (linking with `-fwasm-exceptions` is NOT needed —
  emscripten 6.0.x's prebuilt libcxxabi already provides those). (2) **the
  `__wasm_call_ctors` init-order requirement:** with `-sWASM_BINDGEN` the
  wasm-bindgen glue registers Rust `main` via `addOnInit(initBindgen)`, and
  emscripten's `initRuntime()` runs the onInit callbacks **BEFORE**
  `wasmExports['__wasm_call_ctors']()`, so Rust `main` (and Luau's `Lua::new()`)
  runs **before C++ static initializers** → Luau's `kSizeClassConfig`
  (`lmem.cpp:192`) is zero-initialized → `blockSize == 0` → an integer
  divide-by-zero trap in `newclasspage` (`lmem.cpp:285`) via `lua_newstate` — a
  SILENT wasm trap. **FIX:** call `__wasm_call_ctors()` (declared
  `extern "C"`) as the FIRST statement of `main()`. (The
  `-Cllvm-args=-enable-emscripten-cxx-exceptions=0` flag is NOT the culprit — it
  only affects Rust codegen and coexists with Luau's `-fwasm-exceptions`.) Minor
  spike artifact: `main` ran twice (two context handles); ensure a single init
  when structuring `oh-web`.
- **Landed by:** issue 26 (26c.1 spike); recorded in
  `.agents/plans/001-rust-port/26-wasm-web.md` §"26c.1 — emscripten web boot:
  PASS" and `.agents/skills/lib-web-stack.md` §"26c.1 — Emscripten target
  (verified)".
