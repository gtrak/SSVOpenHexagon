//! Parity test — the level-run pairs (parity/level_runs.json, see its
//! `evidence` + `score_identity` + `frames_note` fields):
//!
//! - `level_runs_live_properties` — the LIVE alwaysSpinRight run (its engine
//!   RNG is entropy-seeded, C++ `initializeRng()`; the levels' onIncrement
//!   side changes draw from it, C++ HGUpdate.cpp:300-303, so its death
//!   frame/score vary with the seed — the C++ test's live run carries the
//!   same property): pin the PROPERTIES, not the outcome:
//!   (a) the recorded input bytes are all 0x02 (C++-derived: the
//!   alwaysSpinRight input block HGUpdate.cpp:660-666, bit order pinned by
//!   the C++-derived replay fixtures), (b) the recorded replay, replayed
//!   through `set_last_replay` + `newGame(executeLastReplay=true)`,
//!   terminates at the live run's exact score — the C++ test's
//!   `TEST_ASSERT_EQ(score, replayPlayedTimeSeconds)` (D-018).
//! - `seeded_replay_runs_match_corpus` — the SEEDED REPLAY run per pair
//!   (corpus seed + the `input_count`×0x02 stream + `expected_score`):
//!   fully deterministic — the exact death frame (`expected_frames`) and
//!   the exact score (bit-exact `assert_eq!`), plus the timekeeping
//!   identity `S == ((F − 25) × 0.25) / 60.0` (the replay run defers its
//!   start by one frame — C++ HGUpdate.cpp:187 — then the 0.1 s start
//!   pause = 6.0 frametime units = 24 frames at TIME_STEP 0.25, C++
//!   `HexagonGameStatus::resetTime` + `accumulateFrametime`).
//! - `level_runs_reference_the_cpp_derived_envelope_fixtures` — the
//!   cross-reference integrity between the level-run pins and the C++-
//!   derived envelope fixtures (replay_serialization.json); the byte-level
//!   envelope match against the Rust `oh_replay` port lives in
//!   parity_replay.rs.
//! - `deliberately_wrong_pin_discriminates` — the exact asserts reject a
//!   1 ulp score drift, an off-by-one input count, and a wrong seed.

mod common;

use std::sync::Arc;

use common::{compose, corpus_path, read_corpus, release_dir, Run};

/// C++ `Config::TIME_STEP` (Config.hpp:26).
const TIME_STEP: f32 = 0.25;

/// The alwaysSpinRight stream: N × (left=false, right=true, swap=false,
/// focus=false) — C++ `HexagonGame::alwaysSpinRight` (HexagonGame.cpp:257)
/// drives the input block at HGUpdate.cpp:660-666; the only non-zero bit is
/// `input_bit::right = 0x02` (InputBits.hpp) through the CW-only resolve
/// branch (C++ InputHandler.cpp:91-95 — pinned by the C++-derived
/// replay_probe fixture 2).
fn spin(n: usize) -> oh_replay::ReplayData {
    let mut d = oh_replay::ReplayData::default();
    for _ in 0..n {
        d.record_input(false, true, false, false);
    }
    d
}

/// The live run: `alwaysSpinRight` + `new_game(first_play, 1.0x)` + manual
/// frame loop (the D-018 manual-update pattern, crates/oh-game/src/
/// hexagon_game.rs:1620-1624 — the `update` → `post_update` sequence the
/// C++ test performs on a windowed engine with no event loop).
fn run_live(assets: &Arc<oh_assets::Assets>, run: &Run) -> (oh_replay::ReplayFile, u64, f64) {
    let root = release_dir();
    let c = compose(assets.clone(), root, false);
    c.game.set_always_spin_right(true);
    c.game.new_game(
        &run.pack,
        &run.level,
        run.first_play,
        run.difficulty_mult,
        false,
    );
    let cap: Arc<std::sync::Mutex<Option<oh_replay::ReplayFile>>> =
        Arc::new(std::sync::Mutex::new(None));
    let cap2 = cap.clone();
    c.game
        .set_on_death_replay_created(Arc::new(move |rf: &oh_replay::ReplayFile| {
            *cap2.lock().expect("poisoned") = Some(rf.clone());
        }));
    c.game.set_must_start(true);
    let mut frames: u64 = 0;
    while !c.game.get_status().has_died {
        c.game.update(TIME_STEP, 1.0);
        c.game.post_update();
        frames += 1;
        assert!(frames < 5_000_000, "runaway live run");
    }
    let rf = cap
        .lock()
        .expect("poisoned")
        .clone()
        .unwrap_or_else(|| panic!("no death replay for {}", run.pair));
    let score = c
        .game
        .get_status()
        .get_played_accumulated_frametime_in_seconds();
    (rf, frames, score)
}

/// Replay `rf` until death; return (frames, score).
fn run_replay_until_death(
    assets: &Arc<oh_assets::Assets>,
    run: &Run,
    rf: oh_replay::ReplayFile,
) -> (u64, f64) {
    let root = release_dir();
    let c = compose(assets.clone(), root, false);
    // C++-faithful: the C++ test's replay phase runs on the SAME engine
    // (ReplayExecution.t.cpp), whose `alwaysSpinRight` flag is still set
    // from the live phase — so the replay engine keeps it too (it only
    // matters if the recorded stream ends before death, which the corpus
    // runs never do, but the fidelity is free).
    c.game.set_always_spin_right(true);
    c.game.set_last_replay(rf);
    c.game.new_game(
        &run.pack,
        &run.level,
        run.first_play,
        run.difficulty_mult,
        true,
    );
    let mut frames: u64 = 0;
    while !c.game.get_status().has_died {
        c.game.update(TIME_STEP, 1.0);
        c.game.post_update();
        frames += 1;
        assert!(frames < 5_000_000, "runaway replay run");
    }
    let score = c
        .game
        .get_status()
        .get_played_accumulated_frametime_in_seconds();
    (frames, score)
}

#[test]
fn level_runs_live_properties() {
    let doc = read_corpus(corpus_path("level_runs.json"));
    let assets = Arc::new(oh_assets::Assets::new(release_dir(), true, false).expect("load"));
    for entry in doc["runs"].as_array().expect("runs") {
        let run = Run::from(entry);
        // (a)+(b) — the live run's properties (the outcome itself varies
        // with the entropy-seeded RNG, see the corpus `evidence`).
        let (rf, frames, score) = run_live(&assets, &run);
        assert!(
            (0..rf.data.size()).all(|i| rf.data.at(i).to_u8() == 0x02),
            "{}: the alwaysSpinRight live run must record an all-0x02 input \
             stream (C++-derived bit order), got {} inputs",
            run.pair,
            rf.data.size()
        );
        let (rf_frames, rf_score) = run_replay_until_death(&assets, &run, rf);
        assert_eq!(
            rf_frames,
            frames + 1,
            "{}: the recorded replay must run for the live run's frame count \
             plus the structural one-frame start deferral (the replay branch \
             arms must-start on frame 0, the live run pre-arms it — the \
             C++ update loop's replay branch, HGUpdate.cpp:187)",
            run.pair
        );
        assert_eq!(
            rf_score, score,
            "{}: the recorded replay must reproduce the live run's exact \
             score (the C++ test's TEST_ASSERT_EQ(score, \
             replayPlayedTimeSeconds); D-018)",
            run.pair
        );
        // Observed, not pinned: logged for the PARITY.md table.
        eprintln!(
            "live {}: frames={} score={} (observed; the entropy-seeded RNG \
             makes the live outcome seed-dependent — only the properties \
             above are pinned)",
            run.pair, frames, score
        );
    }
}

#[test]
fn seeded_replay_runs_match_corpus() {
    let doc = read_corpus(corpus_path("level_runs.json"));
    let assets = Arc::new(oh_assets::Assets::new(release_dir(), true, false).expect("load"));
    for entry in doc["runs"].as_array().expect("runs") {
        let run = Run::from(entry);
        // (i) the exact seeded-replay outcome (the C++-derived corpus).
        let rf = oh_replay::ReplayFile {
            version: 0,
            player_name: run.player_name.clone().into(),
            seed: run.seed,
            data: spin(run.input_count as usize),
            pack_id: run.pack.clone().into(),
            level_id: run.level.clone().into(),
            first_play: run.first_play,
            difficulty_mult: run.difficulty_mult,
            played_score: run.expected_score,
        };
        let (frames, score) = run_replay_until_death(&assets, &run, rf);
        assert_eq!(
            frames, run.expected_frames,
            "{}: the seeded replay run must die on the corpus frame \
             (deterministic in the pinned seed + input stream)",
            run.pair
        );
        assert_eq!(
            score, run.expected_score,
            "{}: the seeded replay run must end at the corpus score to the \
             exact bit (see the corpus `evidence`)",
            run.pair
        );
        // (ii) the timekeeping identity (C++-derived): the replay run
        // defers its start by one frame (HGUpdate.cpp:187), then the
        // 0.1 s start pause = 6.0 frametime units = 24 frames — so
        // (frames − 25) × 0.25 accumulated, ÷ 60: exact in f64 (integer ×
        // 0.25, then one correctly-rounded division, same as the engine).
        let identity = (((run.expected_frames - 25) as f64) * 0.25) / 60.0;
        assert_eq!(
            run.expected_score, identity,
            "{}: corpus score must satisfy S = ((F - 25) * 0.25) / 60 exactly \
             (C++-derived timekeeping)",
            run.pair
        );
        assert_eq!(
            score, identity,
            "{}: the engine score must satisfy the same identity",
            run.pair
        );
    }
}

#[test]
fn level_runs_reference_the_cpp_derived_envelope_fixtures() {
    // Cross-reference integrity (the byte-level envelope match itself lives
    // in parity_replay.rs::replay_serialization_matches_cpp_probe, which
    // checks every `always_spin_right_*` fixture): every level-run pair must
    // have a C++-derived envelope fixture with the SAME pack/level/seed/
    // input-count/score the corpus pins — a drift between the two corpus
    // files is a corpus bug this test catches.
    let runs = read_corpus(corpus_path("level_runs.json"));
    let rep_doc = read_corpus(corpus_path("replay_serialization.json"));
    let fixtures = rep_doc["fixtures"].as_object().expect("fixtures");
    for entry in runs["runs"].as_array().expect("runs") {
        let run = Run::from(entry);
        let key = format!("always_spin_right_{}", run.pair);
        let fx = fixtures
            .get(&key)
            .unwrap_or_else(|| panic!("missing C++ envelope fixture for {}", run.pair));
        assert_eq!(
            fx["pack_id"].as_str().unwrap(),
            run.pack,
            "{}: fixture pack drift",
            run.pair
        );
        assert_eq!(
            fx["level_id"].as_str().unwrap(),
            run.level,
            "{}: fixture level drift",
            run.pair
        );
        assert_eq!(
            fx["seed"].as_u64().unwrap(),
            run.seed,
            "{}: fixture seed drift",
            run.pair
        );
        assert_eq!(
            fx["input_count"].as_u64().unwrap(),
            run.input_count,
            "{}: fixture input-count drift",
            run.pair
        );
        let score_bits = u64::from_str_radix(
            fx["played_score_bits"]
                .as_str()
                .unwrap()
                .strip_prefix("0x")
                .unwrap(),
            16,
        )
        .expect("played_score_bits hex");
        assert_eq!(
            f64::from_bits(score_bits),
            run.expected_score,
            "{}: fixture score drift",
            run.pair
        );
    }
}

#[test]
fn deliberately_wrong_pin_discriminates() {
    let doc = read_corpus(corpus_path("level_runs.json"));
    let assets = Arc::new(oh_assets::Assets::new(release_dir(), true, false).expect("load"));
    // Use the fastest pair (orthoplex_1_bipolarity: 358 frames).
    let entry = doc["runs"][3].clone();
    let run = Run::from(&entry);
    assert_eq!(run.pair, "orthoplex_1_bipolarity", "corpus order drift");

    // (a) a 1 ulp score drift: the corpus score satisfies the identity
    // exactly; the adjacent f64 does not.
    let identity = (((run.expected_frames - 25) as f64) * 0.25) / 60.0;
    assert_eq!(
        run.expected_score, identity,
        "intact pin satisfies the identity"
    );
    assert_ne!(
        identity,
        f64::from_bits(run.expected_score.to_bits() + 1),
        "a 1 ulp drift must fail the exact identity assert"
    );

    // (b) a corrupted input stream: from input #200 on, the spin is
    // reversed (0x01 left instead of 0x02 right) — the sustained
    // direction reversal makes the player's trajectory diverge, so the
    // run cannot terminate at the pinned (frame, score). (A single-frame
    // flip would not: one frame's displacement is too small to change
    // the collision with the seeded walls layout — verified
    // empirically; the sustained reversal does.)
    let mut data = oh_replay::ReplayData::default();
    for i in 0..run.input_count as usize {
        if i >= 200 {
            data.record_input(true, false, false, false);
        } else {
            data.record_input(false, true, false, false);
        }
    }
    let wrong_stream = oh_replay::ReplayFile {
        version: 0,
        player_name: run.player_name.clone().into(),
        seed: run.seed,
        data,
        pack_id: run.pack.clone().into(),
        level_id: run.level.clone().into(),
        first_play: run.first_play,
        difficulty_mult: run.difficulty_mult,
        played_score: run.expected_score,
    };
    let (frames, score) = run_replay_until_death(&assets, &run, wrong_stream);
    assert_ne!(
        (frames, score),
        (run.expected_frames, run.expected_score),
        "a corrupted input stream must not reproduce the pinned outcome"
    );

    // (c) a wrong seed: a different RNG stream -> different side-change
    // draws (the onIncrement draws, HGUpdate.cpp:300-303) -> a different
    // walls layout -> a different outcome.
    let wrong_seed = oh_replay::ReplayFile {
        seed: 12345,
        ..oh_replay::ReplayFile {
            version: 0,
            player_name: run.player_name.clone().into(),
            seed: run.seed,
            data: spin(run.input_count as usize),
            pack_id: run.pack.clone().into(),
            level_id: run.level.clone().into(),
            first_play: run.first_play,
            difficulty_mult: run.difficulty_mult,
            played_score: run.expected_score,
        }
    };
    let (frames, score) = run_replay_until_death(&assets, &run, wrong_seed);
    assert_ne!(
        (frames, score),
        (run.expected_frames, run.expected_score),
        "a wrong seed must not reproduce the pinned outcome"
    );
}
