//! Issue 009 step (c) + step (d): the ported `test/ReplayExecution.t.cpp`
//! (2 pairs, 25 iterations — D-018) and
//! `test/ReplayExecutionBenchmark.t.cpp` (4 pairs, 100 iterations — the set
//! that exposed D-019, pinned by
//! `engine_reuse_after_second_new_game_is_deterministic`), plus a regression
//! guard for the second-`newGame` determinism.
//!
//! # What this test is (and is not)
//!
//! **Self-consistent, NOT C++-verified.** The C++ file contains exactly three
//! assertions (lines 81, 104, 109) and NO expected numbers: a replay was
//! created, playback terminated, and
//! `TEST_ASSERT_EQ(score, replayPlayedTimeSeconds)` — the live run's
//! `playedTimeSeconds` equals the replay run's. The inputs come from the
//! engine's own test policy (the C++ flips `getRndBool()` per iteration to
//! pick `executeRandomInputs` / `alwaysSpinRight`), so there is no external
//! input vector either. This test asserts the same property with an exact
//! `==` (no epsilon, as the C++ does) and labels it accordingly.
//!
//! # Deviations from the C++ test (documented)
//!
//! * The C++ `getRndBool()` coin flip is replaced by a FIXED policy: the main
//!   25-iteration loop (mirroring the C++ headless loop, both `doTest(i,
//!   false)` + `doTest(i, true)` per iteration) uses `alwaysSpinRight` —
//!   deterministic, and it removes the only timing risk (random inputs can
//!   survive longer). `executeRandomInputs` is still exercised by its own
//!   test (`replay_execution_random_inputs_policy`); the property is
//!   policy-independent.
//! * The C++ windowed loop (`#ifndef SSVOH_HEADLESS_TESTS`) has no Rust
//!   counterpart (no window — the composition is headless, `window = false`).
//! * The C++ `assets.addLocalProfile` / `pSetCurrent("testProfile")`:
//!   oh-assets defers the profile *mutation* API, so the engine's `Profiles`
//!   seam reports the same current profile (`TestProfiles` in `common`).
//!
//! # C++-derived pins in this file (the C++-derived evidence for the issue)
//!
//! * **Pack/level ids** — re-derived from `buildPackId` against the shipped
//!   `pack.json`s (D-018 correction): the exact ids the C++ test names.
//! * **Replay input bytes are all `0x02`** (the `alwaysSpinRight` stream):
//!   C++ `HGUpdate.cpp:660-666` sets `inputImplCW = true` (others false) →
//!   the CW-only branch of `updateInput_ResolveInputImplToInputMovement`
//!   yields `inputMovement = 1` → `right` → `record_input(false, true,
//!   false, false)`; the bit order (bit 1 = right → byte `0x02`) is pinned
//!   by the oracle replay probe (mapping/oracle-vectors.md). Any
//!   replay-affecting desync in the input pipeline or the recording breaks
//!   this pin.
//! * **`rf.played_score == the live run's accumulated frametime (f64)`** —
//!   C++ `getReplayScore` (`HexagonGame.cpp:50-56`): neither level script
//!   calls `l_overrideScore`, so `customScore == 0` →
//!   `getPlayedAccumulatedFrametime()` (a `double`, no conversion).
//! * **Level `onInit` state** (`run_lua_file_cached_path_executes_real_level_script`):
//!   the exact `l_setSides` / `l_setRotationSpeed` values each shipped level
//!   script writes in its `onInit` — C++-source-derived (the shipped `.lua`).
//! * **Replay envelope + byte-exact serialization** of the `alwaysSpinRight`
//!   stream — the `tools/oracle/replay_probe.cpp` fixture 2
//!   (mapping/oracle-vectors.md, "Fixture 2").

mod common;

use std::sync::Arc;

use oh_config::Config;
use oh_lua::api::state::SharedLua;
use oh_lua::LuaContext;
use oh_replay::{compress_replay_file, ReplayData, ReplayFile};

use common::{compose, release_dir, run_live_to_death, LuaScriptHost};

/// The exact pack/level ids the C++ test names (re-derived via `buildPackId`
/// against the shipped `pack.json`s — D-018; level id = `packId + "_" +
/// levelId`, `MenuGame.cpp:4840`).
const PACKS: [&str; 2] = [
    "ohvrvanilla_vittorio_romeo_cube_1",
    "ohvrvanilla_vittorio_romeo_experimental_1",
];
const LEVELS: [&str; 2] = [
    "ohvrvanilla_vittorio_romeo_cube_1_apeirogon",
    "ohvrvanilla_vittorio_romeo_experimental_1_autotest0",
];

fn load_release() -> Arc<oh_assets::Assets> {
    // C++: `hg::HGAssets assets{nullptr, true /* headless */};`
    Arc::new(oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"))
}

/// C++-derived pins on the live run's replay file + its status (module docs
/// carry each derivation).
fn assert_replay_pins(rf: &ReplayFile, status: &oh_game::HexagonGameStatus) {
    // C++ test source: `ProfileData{GAME_VERSION, "testProfile"}` +
    // `pSetCurrent` → `death_createReplayFile` records the profile name.
    assert_eq!(
        rf.player_name.to_str(),
        Some("testProfile"),
        "C++ pSetCurrent(\"testProfile\")"
    );
    // C++ test: `newGame(..., true /* firstPlay */, 1.f /* diffMult */, ...)`.
    assert!(rf.first_play, "C++ test passes firstPlay = true");
    assert_eq!(
        rf.difficulty_mult, 1.0_f32,
        "C++ test passes diffMult = 1.f"
    );
    assert!(rf.data.size() > 0, "the replay recorded no inputs");

    // C++ `alwaysSpinRight` (HGUpdate.cpp:660-666) + the CW-only resolve
    // branch → `movement = 1` (right) → `record_input(false, true, false,
    // false)` → bit 1 → byte 0x02 (oracle bit-order note).
    for k in 0..rf.data.size() {
        assert_eq!(
            rf.data.at(k).to_u8(),
            0x02,
            "input tick {k}: alwaysSpinRight must record (l=false, r=true, s=false, f=false) → byte 0x02"
        );
    }

    // C++ `getReplayScore` (HexagonGame.cpp:50-56): no level script calls
    // `l_overrideScore` → `customScore == 0` → `getPlayedAccumulatedFrametime()`
    // (a double; the f64 accumulated frametime, exact).
    assert_eq!(
        status.get_custom_score(),
        0.0_f32,
        "level scripts never override the score"
    );
    assert_eq!(
        rf.played_score,
        status.get_played_accumulated_frametime(),
        "C++ getReplayScore: customScore 0 → getPlayedAccumulatedFrametime()"
    );
}

/// One C++ `doTest(i, differentHG)` iteration with the `alwaysSpinRight`
/// policy (the C++ `getRndBool()` coin flip is replaced by this fixed
/// choice — see the module docs).
fn do_test(assets: &Arc<oh_assets::Assets>, i: usize, different_hg: bool) {
    let root = release_dir();
    let c = compose(assets.clone(), root.clone(), false);
    let pack = PACKS[i % PACKS.len()];
    let level = LEVELS[i % LEVELS.len()];

    // C++: `hg.alwaysSpinRight = true;` (the fixed-policy stand-in for the
    // C++ `getRndBool()` choice — module docs).
    c.game.set_always_spin_right(true);
    // C++: `hg.newGame(packs[i], levels[i], true, 1.f, false);`
    c.game.new_game(pack, level, true, 1.0, false);
    let (score, rf) = run_live_to_death(&c.game);
    assert!(score > 0.0, "live run died with zero played time");

    assert_replay_pins(&rf, &c.game.get_status());

    // C++: `TEST_ASSERT(score2.has_value())` + `TEST_ASSERT_EQ(score,
    // replayPlayedTimeSeconds)` — the same engine, or a fresh one.
    let score2 = if different_hg {
        let c2 = compose(assets.clone(), root, false);
        c2.game
            .run_replay_until_death_and_get_score(&rf, 1, 1.0)
            .expect("replay playback did not terminate")
    } else {
        c.game
            .run_replay_until_death_and_get_score(&rf, 1, 1.0)
            .expect("replay playback did not terminate")
    };
    // C++ `std::cerr << score << " == " << replayPlayedTimeSeconds`.
    eprintln!("{score} == {}", score2.played_time_seconds);
    assert_eq!(
        score, score2.played_time_seconds,
        "D-018 self-consistency: the live run must replay to the identical played time (exact ==, as the C++)"
    );
}

/// The C++ headless loop: `for(i = 0; i < 25; ++i) { doTest(i, false,
/// nullptr); doTest(i, true, nullptr); }` (the windowed half is
/// `#ifndef SSVOH_HEADLESS_TESTS` — no Rust equivalent; no window).
#[test]
fn replay_execution_headless_loop() {
    let assets = load_release();
    for i in 0..25 {
        do_test(&assets, i, false);
        do_test(&assets, i, true);
    }
}

/// The C++ `getRndBool()` may also pick `executeRandomInputs`
/// (`HGUpdate.cpp:667-674`, a non-deterministic static `mt19937`; the port
/// draws from a per-engine `pcg32_fast` — the streams are not comparable to
/// the C++, documented in `update.rs`). The self-consistency property is
/// policy-independent: the replay records whatever the policy produced.
#[test]
fn replay_execution_random_inputs_policy() {
    let assets = load_release();
    for i in 0..3 {
        let root = release_dir();
        let c = compose(assets.clone(), root.clone(), false);
        c.game.set_execute_random_inputs(true);
        c.game.new_game(
            PACKS[i % PACKS.len()],
            LEVELS[i % LEVELS.len()],
            true,
            1.0,
            false,
        );
        let (score, rf) = run_live_to_death(&c.game);
        assert!(score > 0.0, "live run died with zero played time");

        // Fresh engine (the stronger variant — a different `HexagonGame`).
        let c2 = compose(assets.clone(), root, false);
        let score2 = c2
            .game
            .run_replay_until_death_and_get_score(&rf, 1, 1.0)
            .expect("replay playback did not terminate");
        eprintln!("(random inputs) {score} == {}", score2.played_time_seconds);
        assert_eq!(
            score, score2.played_time_seconds,
            "random-input self-consistency"
        );
    }
}

// ---------------------------------------------------------------------------
// Issue 009 step (d): the ported `test/ReplayExecutionBenchmark.t.cpp` — the
// 4-pair level set at 100 iterations. It is the 2-pair test with exactly
// three differences (verified against the C++ file): 4 pairs (adding
// `orthoplex`/`arcadia` and `orthoplex`/`bipolarity`), 100 iterations, and
// the commented-out `std::cerr` line + no windowed block. The assertions are
// the same three self-consistency checks (D-018) — NO expected numbers. This
// coverage is what exposed D-019 (the stale timeline runners across
// `newGame`), pinned by
// `engine_reuse_after_second_new_game_is_deterministic` below.
// ---------------------------------------------------------------------------

/// The C++ benchmark's `packs` array, verbatim (`i % 4` indexing — all four
/// are used; `orthoplex` appears twice, once per level).
const BENCH_PACKS: [&str; 4] = [
    "ohvrvanilla_vittorio_romeo_cube_1",
    "ohvrvanilla_vittorio_romeo_experimental_1",
    "ohvrvanilla_vittorio_romeo_orthoplex_1",
    "ohvrvanilla_vittorio_romeo_orthoplex_1",
];
/// The C++ benchmark's `levels` array, verbatim (re-derived via `buildPackId`
/// against the shipped `pack.json`s — D-018 correction; `arcadia` / `bipolarity`
/// ship in `_RELEASE/Packs/orthoplex/Levels/`).
const BENCH_LEVELS: [&str; 4] = [
    "ohvrvanilla_vittorio_romeo_cube_1_apeirogon",
    "ohvrvanilla_vittorio_romeo_experimental_1_autotest0",
    "ohvrvanilla_vittorio_romeo_orthoplex_1_arcadia",
    "ohvrvanilla_vittorio_romeo_orthoplex_1_bipolarity",
];

/// One C++ benchmark `doTest(i, differentHG)` iteration (identical to
/// `do_test` above, but indexed over the 4-pair set; the fixed
/// `alwaysSpinRight` policy applies — same documented deviation as the
/// 2-pair loop).
fn bench_do_test(assets: &Arc<oh_assets::Assets>, i: usize, different_hg: bool) {
    let root = release_dir();
    let c = compose(assets.clone(), root.clone(), false);
    c.game.set_always_spin_right(true);
    // C++: `hg.newGame(packs[i % packs.size()], levels[i % levels.size()],
    // true /* firstPlay */, 1.f /* diffMult */, false);`
    c.game.new_game(
        BENCH_PACKS[i % BENCH_PACKS.len()],
        BENCH_LEVELS[i % BENCH_LEVELS.len()],
        true,
        1.0,
        false,
    );
    let (score, rf) = run_live_to_death(&c.game);
    assert!(score > 0.0, "live run died with zero played time");

    // C++: `TEST_ASSERT(score2.has_value())` + `TEST_ASSERT_EQ(score,
    // replayPlayedTimeSeconds)` — the same engine, or a fresh one.
    let score2 = if different_hg {
        let c2 = compose(assets.clone(), root, false);
        c2.game
            .run_replay_until_death_and_get_score(&rf, 1, 1.0)
            .expect("replay playback did not terminate")
    } else {
        c.game
            .run_replay_until_death_and_get_score(&rf, 1, 1.0)
            .expect("replay playback did not terminate")
    };
    // The C++ `std::cerr << score << " == " << replayPlayedTimeSeconds` line
    // is commented out in the benchmark file — not ported.
    assert_eq!(
        score, score2.played_time_seconds,
        "D-018 self-consistency (benchmark 4-pair set): the live run must replay to the identical played time (exact ==, as the C++)"
    );
}

/// The C++ benchmark headless loop: `for(int i = 0; i < 100; ++i) {
/// doTest(i, false, nullptr); doTest(i, true, nullptr); }` — 100 iterations
/// × both variants × the 4-pair set (200 doTest calls; the windowed half is
/// absent in the C++ benchmark file — no Rust equivalent). **Self-consistent,
/// NOT C++-verified** (D-018: no expected numbers anywhere in the C++ file).
#[test]
fn replay_execution_benchmark_headless_loop() {
    let assets = load_release();
    for i in 0..100 {
        bench_do_test(&assets, i, false);
        bench_do_test(&assets, i, true);
    }
}

/// D-019 regression guard: a SECOND `new_game` on the SAME engine must stay
/// deterministic.
///
/// C++ `setLevelData` resets each timeline **and its runner**:
/// `eventTimeline.clear(); eventTimelineRunner = {};` (`HexagonGame.cpp:668-669`),
/// `messageTimeline... + messageTimelineRunner = {};` (`:672-673`),
/// `timeline.clear(); timelineRunner = {};` (`:686-687`). The port
/// originally cleared the three timelines but kept `main_timeline_runner` /
/// `event_timeline_runner` (stale `current_idx` / `wait_start_tp`), so the
/// first `t_wait` / `e_wait` of the NEXT game measured against the previous
/// game's clock: the spawn schedule shifted and the death time moved (D-019).
/// Levels that wait early and heavily on the timelines expose it —
/// `orthoplex/bipolarity` does; `apeirogon` / `autotest0` happen not to (which
/// is why the 2-pair harness passed). `bipolarity.lua`'s `onInit` ends with
/// `t_wait(12 / getMusicDMSyncFactor())` and its `onStep` schedules
/// `e_wait(...)`s.
///
/// The assertion shape D-019's discrimination isolated: a live run that
/// follows a previous `newGame` on the same engine must still replay to the
/// identical played time (exact `==`). **Self-consistent, NOT
/// C++-verified** (D-018).
#[test]
fn engine_reuse_after_second_new_game_is_deterministic() {
    let assets = load_release();
    let root = release_dir();
    let c = compose(assets.clone(), root.clone(), false);
    c.game.set_always_spin_right(true);

    // Game 1: a normal `newGame` on a fresh engine.
    c.game.new_game(
        "ohvrvanilla_vittorio_romeo_orthoplex_1",
        "ohvrvanilla_vittorio_romeo_orthoplex_1_bipolarity",
        true,
        1.0,
        false,
    );
    let (score1, _) = run_live_to_death(&c.game);
    assert!(score1 > 0.0, "game 1 died with zero played time");

    // Game 2: the SAME level via a second `newGame` on the SAME engine — the
    // restart path in the real game too (D-019). With the D-019 bug this
    // live run's spawn schedule shifts (its first wait measured against
    // game 1's runner clock) and it no longer replays to the identical
    // played time.
    c.game.new_game(
        "ohvrvanilla_vittorio_romeo_orthoplex_1",
        "ohvrvanilla_vittorio_romeo_orthoplex_1_bipolarity",
        true,
        1.0,
        false,
    );
    let (score2, rf2) = run_live_to_death(&c.game);
    assert!(score2 > 0.0, "game 2 died with zero played time");

    // Replay game 2's recording on a FRESH engine (D-019's discrimination:
    // the replay itself is deterministic — the live run is what must match
    // it after a previous `newGame`).
    let c2 = compose(assets.clone(), root, false);
    let score3 = c2
        .game
        .run_replay_until_death_and_get_score(&rf2, 1, 1.0)
        .expect("replay playback did not terminate");
    assert_eq!(
        score2, score3.played_time_seconds,
        "D-019: the second newGame on the same engine must replay to the identical played time (exact ==)"
    );
}

// ---------------------------------------------------------------------------
// The C++-derived serialization pin (tools/oracle/replay_probe.cpp fixture 2,
// mapping/oracle-vectors.md "Fixture 2 — the alwaysSpinRight input stream")
// ---------------------------------------------------------------------------

const STREAM_HEX: &str = "000000000b0000007465737450726f66696c65b168de3a000000000c00000000000000020202020202020202020202210000006f68767276616e696c6c615f766974746f72696f5f726f6d656f5f637562655f312b0000006f68767276616e696c6c615f766974746f72696f5f726f6d656f5f637562655f315f61706569726f676f6e010000803f0000000000808640";
const STREAM_CZ_HEX: &str = "78da63606060e006e292d4e29280a2fcb4cc9cd48d19f7ac80220c3c0c10c084041481fcfc8cb2a2b2c4bccc9c9cc4f8b2cc9292fca2ccfcf8a2fcdcd4fcf8e4d2a4d478436d6214c52716a46616e5a7e7e731323034d8836d6a687300002a07288f";

fn hex_of(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(format!("{b:02x}").as_str());
    }
    s
}

/// Builds the probe's fixture-2 replay file field-for-field.
fn stream_fixture() -> ReplayFile {
    let mut data = ReplayData::default();
    for _ in 0..12 {
        // The `alwaysSpinRight` tick: (left, right, swap, focus) =
        // (false, true, false, false).
        data.record_input(false, true, false, false);
    }
    ReplayFile {
        version: 0,
        player_name: "testProfile".into(),
        seed: 987654321,
        data,
        pack_id: "ohvrvanilla_vittorio_romeo_cube_1".into(),
        level_id: "ohvrvanilla_vittorio_romeo_cube_1_apeirogon".into(),
        first_play: true,
        difficulty_mult: 1.0,
        played_score: 720.0,
    }
}

#[test]
fn replay_stream_serialization_matches_cpp_probe() {
    let rf = stream_fixture();
    let mut buf = [0u8; 4096];
    let sr = rf.serialize(&mut buf);
    assert!(sr.is_success(), "serialize failed");
    assert_eq!(
        hex_of(&buf[..sr.written_bytes()]),
        STREAM_HEX,
        "C++ probe fixture 2 (tools/oracle/replay_probe.cpp): the alwaysSpinRight stream must serialize byte-exactly"
    );

    let crf = compress_replay_file(&rf).expect("compression failed");
    assert_eq!(
        hex_of(&crf.data),
        STREAM_CZ_HEX,
        "C++ probe fixture 2: the zlib level-9 bytes must match (flate2 on system zlib)"
    );
}

// ---------------------------------------------------------------------------
// The C++-derived level-script and `run_lua_file` pins
// ---------------------------------------------------------------------------

/// C++ `Utils::runLuaFile` open failure: `runtime_error("Fatal Lua error\n
/// Could not open file: <name>\n")`. Exercised on a NON-headless host
/// (headless takes the cached path — `HexagonGame::runLuaFile`,
/// `HGScripting.cpp:1130-1141`).
#[test]
fn run_lua_file_open_failure_message() {
    let lua = LuaContext::new();
    let host = LuaScriptHost::new(
        &lua,
        false, /* windowed → the uncached path (use_lua_file_cache defaults false) */
        release_dir(),
        Arc::new(std::sync::RwLock::new(Config::default())),
    );
    let shared = SharedLua::new(&lua);
    let err = host
        .execute_file(&shared, "Packs/does-not-exist.lua")
        .expect_err("missing file must fail");
    assert_eq!(
        err,
        "Fatal Lua error\nCould not open file: Packs/does-not-exist.lua\n"
    );
}

/// The headless `newGame` took the C++ CACHED path (`runLuaFileCached` —
/// `HexagonGame::runLuaFile`: headless ⇒ cache) and executed the REAL shipped
/// level scripts: their `onInit` values land in the shared level status.
/// C++-source-derived expectations (the shipped `.lua` `onInit` bodies):
/// apeirogon (`cube`): `l_setSides(6)` + `l_setRotationSpeed(0.3)`;
/// autotest0 (`experimental`): `l_setSides(12)` + `l_setRotationSpeed(5.0)`
/// (both at `diffMult = 1.0`, i.e. the `u_getDifficultyMult() > 1.5`-false
/// branches).
#[test]
fn run_lua_file_cached_path_executes_real_level_scripts() {
    let assets = load_release();
    for (pack, level, sides, rotation_speed) in [
        (PACKS[0], LEVELS[0], 6_u32, 0.3_f32),
        (PACKS[1], LEVELS[1], 12_u32, 5.0_f32),
    ] {
        let c = compose(assets.clone(), release_dir(), false);
        c.game.new_game(pack, level, true, 1.0, false);
        assert_eq!(
            c.game.get_sides(),
            sides,
            "{level}: the level script's onInit l_setSides must land in the shared status"
        );
        assert_eq!(
            c.game.get_rotation_speed(),
            rotation_speed,
            "{level}: the level script's onInit l_setRotationSpeed must land in the shared status"
        );
    }
}
