//! K7 (deviations register row K7 — "one PCG stream, not two"): a level whose
//! `onRenderStage` draws random numbers must advance the SAME stream the
//! gameplay reads — the C++'s `HexagonGame::rng`
//! (`random_number_generator`, `include/SSVOpenHexagon/Core/
//! RandomNumberGenerator.{hpp,cpp}`), which the C++'s redefined Lua
//! `math.random` is bound to (`src/SSVOpenHexagon/Core/LuaScripting.cpp`:
//! `initRandom(lua, rng)` :81-107 binds `u_rndReal`/`u_rndIntUpper`/
//! `u_rndInt` to `rng.get_real/get_int`; `redefineRandom` :158-176 rebinds
//! `math.random` to `u_rndSwitch`; `HexagonGame::initLua`
//! (`HGScripting.cpp`:1105-1107) passes the game's OWN `rng`, not the
//! SSVUtils global).
//!
//! The property asserted here (the register's acceptance): a draw made from
//! the render hook advances the game stream the gameplay draws read —
//! draw in the hook, then assert the next gameplay draw equals what a
//! single-stream order (one `random_number_generator` from the replay seed)
//! produces. A test that merely re-asserted a split (hook draws on a
//! separate stream) would be worthless: under a split, the hook run and a
//! no-hook control run would be byte-identical in every game-stream metric,
//! and this test's assertions (the hook draws counted in `draws()`, the
//! hook values equal to the single-stream oracle values, the side-selection
//! draw equal to the oracle's, the final states equal to the oracle's) all
//! fail.
//!
//! # The single-stream oracle
//!
//! `set_last_replay` + `new_game(executeLastReplay=true)` is the C++'s
//! `rng = random_number_generator{replayFile._seed}` (`HexagonGame.cpp`:
//! the port's `hexagon_game.rs` `new_game` replay branch) — a deterministic
//! seed. The oracle is a fresh `RandomNumberGenerator::new(seed)` advanced
//! by the EXACT wrapper-call sequence the engine consumes (hook `get_real`
//! calls, the increment's side-selection `get_int`, and the per-frame
//! `rng.advance(fixup(...))` blocks, `HGUpdate.cpp`:449-458 / port
//! `update.rs`:400-411 — read from the engine's post-frame status values,
//! the values the C++ block reads). Every wrapper call is one stream
//! position; `state_position()` (030b's canonical MCG label, affine in the
//! bump count) pins the position, `draws()` the attribution.
//!
//! # Frame choreography (the C++'s per-frame order: `update` → `postUpdate`
//! → `draw` (the `onRenderStage` hook fires inside the draw,
//! `HGGraphics.cpp`:73-74))
//!
//! The synthetic level (`Scripts/seq.lua`) draws ONE `math.random()` per
//! `onRenderStage` call and records it in the global `hookDraws`;
//! `l_setIncTime(1.0)` makes the increment (and its side-selection draw,
//! `HGUpdate.cpp`:299-303 `sideChange(rng.get_int(sidesMin, sidesMax))`,
//! port `update.rs`:338-344) fire every simulated second = 240 frames of
//! `TIME_STEP`. The level spawns no walls (the C++'s `setSides`
//! (`HexagonGame.cpp`:1515-1524) does not spawn either — walls come from
//! the level's `w_addWall` timeline calls, which this level makes none), so
//! the player never dies and the run is frame-bounded, not death-bounded.

mod common;

use std::io::Write;
use std::sync::Arc;

use oh_config::TIME_STEP;
use oh_replay::{ReplayData, ReplayFile};
use oh_rng::{RandomNumberGenerator, Seed};

/// The known replay seed (the C++'s `replayFile._seed` → `mRng`'s seed).
const SEED: Seed = 42;
/// The run's frame budget. The increment fires at frame 266: start at
/// frame 2, the C++'s 0.1 s start pause (`currentPause = 0.1 * 60` FT,
/// `HexagonGameStatus` default) defers the increment clock to frame 26, and
/// 1.0 s `incTime` = 240 `TIME_STEP` frames after that. The budget covers
/// the side-selection draw with margin; the second increment (frame 506)
/// stays outside the run, so exactly ONE gameplay draw occurs.
const FRAMES: usize = 280;

fn write_synthetic_pack(root: &std::path::Path) {
    let pack = root.join("Packs/testport_k7");
    std::fs::create_dir_all(pack.join("Levels")).unwrap();
    std::fs::create_dir_all(pack.join("Styles")).unwrap();
    std::fs::create_dir_all(pack.join("Music")).unwrap();
    std::fs::create_dir_all(pack.join("Scripts")).unwrap();

    fn write_file(path: &std::path::Path, content: &str) {
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    write_file(
        &pack.join("pack.json"),
        r#"{
    "disambiguator": "testport",
    "name": "k7",
    "author": "tester",
    "version": 1
}"#,
    );
    write_file(&pack.join("Styles/basic.json"), r#"{"id": "basic"}"#);
    write_file(&pack.join("Music/none.json"), r#"{"id": "none"}"#);

    // The K7 level: `onRenderStage` draws random numbers (the register's
    // "third-party content" case). One `math.random()` per hook call = one
    // `rng.get_real(0, 1)` on the game stream (the C++'s `redefineRandom`
    // body, `math.random()` with no args → `u_rndSwitch(0, 0, 0)` →
    // `u_rndReal()` → `rng.get_real<float>(0, 1)`).
    write_file(
        &pack.join("Levels/seq.json"),
        r#"{"id": "seq", "styleId": "basic", "musicId": "none", "luaFile": "Scripts/seq.lua"}"#,
    );
    write_file(
        &pack.join("Scripts/seq.lua"),
        r#"-- K7 property level: the render hook draws random numbers.
hookDraws = {}

function onInit()
    -- One increment per simulated second (the C++ `updateIncrement`'s
    -- `incTime` gate, HGUpdate.cpp:720-730) so the side-selection draw
    -- (`rng.get_int(sidesMin, sidesMax)`, HGUpdate.cpp:299-303) fires on a
    -- known cadence; a non-degenerate range makes the draw value-informative.
    l_setIncTime(1.0)
    l_setSidesMin(3)
    l_setSidesMax(5)
end

function onRenderStage(rs, ft)
    hookDraws[#hookDraws + 1] = math.random()
end
"#,
    );

    // The control level: the same level WITHOUT the hook draw (`onRenderStage`
    // present but body-less — the C++'s `runLuaFunctionIfExists` still fires
    // it, it just consumes no RNG).
    write_file(
        &pack.join("Levels/quiet.json"),
        r#"{"id": "quiet", "styleId": "basic", "musicId": "none", "luaFile": "Scripts/quiet.lua"}"#,
    );
    write_file(
        &pack.join("Scripts/quiet.lua"),
        r#"-- The K7 control: no render-hook draws.
function onInit()
    l_setIncTime(1.0)
    l_setSidesMin(3)
    l_setSidesMax(5)
end

function onRenderStage(rs, ft) end
"#,
    );
}

/// The C++'s `replayFile` the run is seeded from (`setLastReplay` → the
/// `newGame(executeLastReplay)` `rng = random_number_generator{_seed}`).
/// The inputs are the determinism-pinned `alwaysSpinRight` stream (0x02 =
/// right; mapping/oracle-vectors.md fixture 2) — 300 bytes cover the frame
/// budget with margin.
fn seeded_replay(level_id: &str) -> ReplayFile {
    let mut data = ReplayData::default();
    for _ in 0..300 {
        data.record_input(false, true, false, false);
    }
    ReplayFile {
        version: 0,
        player_name: "tester".into(),
        seed: SEED,
        data,
        pack_id: "testport_tester_k7_1".into(),
        level_id: level_id.into(),
        first_play: true,
        difficulty_mult: 1.0,
        played_score: 0.0,
    }
}

/// One fully-composed engine over the synthetic pack (the issue-009 step (c)
/// composition — the real `ScriptHost` over Luau, so `math.random` runs the
/// C++'s redefinition through the shared state, exactly as the windowed
/// bridge's `on_render_stage` closure drives it).
fn compose_level(level_id: &str, dir: &std::path::Path) -> common::Composed {
    let assets =
        Arc::new(oh_assets::Assets::new(dir, true, false).expect("synthetic pack loads headless"));
    let c = common::compose(assets, dir.to_path_buf(), false);
    c.game.set_last_replay(seeded_replay(level_id));
    c.game.new_game(
        "testport_tester_k7_1",
        level_id,
        true,
        1.0,
        true, /* executeLastReplay: the C++'s `rng = random_number_generator{_seed}` */
    );
    c
}

/// The C++'s `fixup` (`HGUpdate.cpp`:449 — `x < 0 ? -x : x`, the float →
/// `state_type` narrowing the port's `update.rs`:387-394 documents).
fn fixup(x: f32) -> u64 {
    if x < 0.0_f32 {
        -x as u64
    } else {
        x as u64
    }
}

/// The engine's game-RNG metrics (030b's fingerprint fields: the wrapper
/// draw count + the canonical MCG state label), read from the shared store
/// the 285 lambdas and the update loop both lock.
fn game_rng_metrics(c: &common::Composed) -> (u64, u64) {
    let st = c.game.state_handle().lock();
    (st.rng.draws(), st.rng.state_position())
}

/// The C++'s post-frame `fixup` advance sum (`HGUpdate.cpp`:449-458), read
/// from the engine's post-`update` status values — the exact values the
/// C++ block reads (the block is the LAST consumer of them in the frame).
fn frame_advance(c: &common::Composed) -> u64 {
    let st = c.game.state_handle().lock();
    fixup(st.status.pulse)
        + fixup(st.status.pulse_3d)
        + fixup(st.status.fast_spin)
        + fixup(st.status.flash_effect)
        + fixup(st.level_status.rotation_speed)
}

/// Drives the C++'s per-frame order (`update` → `postUpdate` → the
/// `onRenderStage` hook in the draw) for `FRAMES` frames, mirroring every
/// game-stream consumption on the single-stream oracle in call order.
///
/// Returns `(the side-selection value the oracle produced, the frame it
/// fired on, the oracle)`. `hook_draws` (when the level's `onRenderStage`
/// draws) is advanced on the oracle per hook call; the side draw
/// (`get_int(3, 5)`) lands on the oracle BEFORE its frame's `fixup`
/// advance (the C++'s in-frame order: the side check at `HGUpdate.cpp`:299
/// precedes the fixup block at `:449`); the per-frame `fixup` advances are
/// replayed from the engine's post-frame values.
fn drive_frames(
    c: &common::Composed,
    oracle: &mut RandomNumberGenerator,
    hook_draws: &mut Option<&mut Vec<f32>>,
) -> (u32, usize) {
    let mut side_value: Option<u32> = None;
    let mut side_frame: usize = 0;

    for frame in 1..=FRAMES {
        let sides_before: u32 = c
            .lua()
            .execute_code_typed("return l_getSides()")
            .expect("l_getSides");

        c.game.update(TIME_STEP, 1.0);
        c.game.post_update();

        let sides_after: u32 = c
            .lua()
            .execute_code_typed("return l_getSides()")
            .expect("l_getSides");
        let drew_side = sides_before != sides_after;

        // The started frames (frame 2 on: the replay path's `mustStart`
        // defers `start()` one frame, the C++'s `HGUpdate.cpp`:169-172)
        // consume the frame's fixup advances; the side draw (increment
        // frame) consumed it BEFORE those advances (in-frame order above).
        if frame >= 2 {
            if drew_side {
                // The C++'s `sideChange(rng.get_int(sidesMin, sidesMax))` —
                // the game stream, u32 path (the port's `update.rs`:340-342
                // reads `sides_min`/`sides_max` as u32).
                side_value = Some(oracle.get_int::<u32>(3, 5));
                side_frame = frame;
                assert_eq!(
                    sides_after,
                    side_value.unwrap(),
                    "frame {frame}: the engine's side-selection draw must equal the \
                     single-stream oracle's value at this call position"
                );
            }
            oracle.advance(frame_advance(c));
        }

        // The C++'s draw phase: the per-stage `onRenderStage` hook (the
        // windowed `draw_frame` fires it once per render stage; this test
        // drives it once per frame, AFTER the update, the C++'s frame
        // order). The level's body makes at most one `math.random()` call
        // (one `get_real(0, 1)`) when `hook_draws` is armed.
        c.game.run_on_render_stage(0, 1.0).expect("onRenderStage");
        if let Some(hook_draws) = hook_draws.as_mut() {
            hook_draws.push(oracle.get_real::<f32>(0.0, 1.0));
        }
    }

    (
        side_value.expect("the increment must have fired inside the frame budget"),
        side_frame,
    )
}

#[test]
fn render_hook_draw_advances_the_game_rng_stream() {
    let dir = std::env::temp_dir().join(format!("oh-game-k7-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    write_synthetic_pack(&dir);

    // ------------------------------------------------------------------
    // The HOOK run: the level's `onRenderStage` draws a `math.random()` on
    // every hook call.
    // ------------------------------------------------------------------
    let c = compose_level("testport_tester_k7_1_seq", &dir);

    // `new_game(executeLastReplay)` re-seeded the shared game RNG from the
    // replay seed (the C++'s `rng = random_number_generator{_seed}`) and
    // made no gameplay draws (the level's `onInit` is `l_set*` only).
    let (draws0, state0) = game_rng_metrics(&c);
    assert_eq!(
        draws0, 0,
        "new_game must not have consumed a game-stream draw"
    );
    let mut oracle = RandomNumberGenerator::new(SEED);
    assert_eq!(
        oracle.state_position(),
        state0,
        "the engine's pre-hook game-RNG state must be the single stream's \
         position at the replay seed"
    );

    // The render hook: ONE invocation, the level's `onRenderStage` calls
    // `math.random()` once.
    c.game.run_on_render_stage(0, 1.0).expect("onRenderStage");
    let (draws1, state1) = game_rng_metrics(&c);
    assert_eq!(
        draws1,
        draws0 + 1,
        "the hook's draw must be ATTRIBUTED to the game stream's draw count \
         (a separate cosmetic stream would not increment it)"
    );
    let pre_hook_draw: f32 = c
        .lua()
        .execute_code_typed("return hookDraws[1]")
        .expect("the hook's recorded draw");
    // `get_real::<f32>(0, 1)` is exactly one engine bump (`generate_canonical`
    // `__m = 1`, full width, no rejection — oh-rng's oracle pins it).
    assert_eq!(
        state1,
        state0.wrapping_add(1),
        "one f32 [0,1] draw advances the game stream by exactly one bump"
    );
    assert_eq!(
        pre_hook_draw.to_bits(),
        oracle.get_real::<f32>(0.0, 1.0).to_bits(),
        "the hook's draw must be the SINGLE-STREAM value at the post-newGame \
         position (an independent entropy stream would not match)"
    );

    // The frame loop: the next GAMEPLAY draw (the increment's side
    // selection) must equal what the single-stream order (hook draws
    // interleaved, replay seed) produces.
    let mut hook_draws = Vec::new();
    let mut hook_draws_opt: Option<&mut Vec<f32>> = Some(&mut hook_draws);
    let (_, side_frame) = drive_frames(&c, &mut oracle, &mut hook_draws_opt);

    let (draws_final, state_final) = game_rng_metrics(&c);
    assert_eq!(
        draws_final,
        1 /* pre-hook */ + FRAMES as u64 /* per-frame hooks */ + 1, /* the side draw */
        "every render-hook draw and the side-selection draw must count on the \
         game stream"
    );
    assert_eq!(
        state_final,
        oracle.state_position(),
        "after the whole run the engine's game-RNG state must equal the \
         single-stream oracle's (hook draws and gameplay draws interleaved \
         in the C++'s frame order)"
    );

    // Every recorded hook draw, in order, bit-exactly the single-stream
    // value at its call position.
    let recorded: Vec<f32> = c
        .lua()
        .execute_code_typed("return hookDraws")
        .expect("the hook's recorded draws");
    assert_eq!(
        recorded.len(),
        1 + FRAMES,
        "the level recorded one draw per hook call"
    );
    for (i, (rec, orc)) in recorded
        .iter()
        .zip([vec![pre_hook_draw], hook_draws].concat().iter())
        .enumerate()
    {
        assert_eq!(
            rec.to_bits(),
            orc.to_bits(),
            "hook draw #{i} must be the single-stream value at its call position"
        );
    }

    // ------------------------------------------------------------------
    // The CONTROL run: the same level WITHOUT the hook draw (body-less
    // `onRenderStage`), the same seed, the same frames. Under a single
    // stream this run's game state diverges from the hook run's (the hook
    // draws shifted the stream) and its draw count is lower by exactly the
    // hook draw count. Under the split the two runs would be identical in
    // every game-stream metric — which is what this pair refutes.
    // ------------------------------------------------------------------
    let q = compose_level("testport_tester_k7_1_quiet", &dir);
    let (q_draws0, q_state0) = game_rng_metrics(&q);
    assert_eq!(q_draws0, 0);
    let mut q_oracle = RandomNumberGenerator::new(SEED);
    assert_eq!(q_oracle.state_position(), q_state0);

    let mut q_hook: Option<&mut Vec<f32>> = None;
    let (_, q_side_frame) = drive_frames(&q, &mut q_oracle, &mut q_hook);
    let (q_draws_final, q_state_final) = game_rng_metrics(&q);

    assert_eq!(
        q_side_frame, side_frame,
        "the increment is time-driven: both runs reach the side-selection \
         frame at the same frame index"
    );
    assert_eq!(q_draws_final, 1 /* the side draw only */);
    assert_eq!(
        q_state_final,
        q_oracle.state_position(),
        "the control run's game-RNG state must equal its single-stream \
         oracle's (no hook draws on the stream); its side-selection value \
         was asserted against the engine on the firing frame inside \
         drive_frames"
    );

    // The cross-run property: the hook run's game stream carries the hook
    // draws (its draw count is higher by exactly the hook draw count) and
    // its final state diverges from the control's (the hook draws interleaved
    // into the gameplay stream and shifted it).
    assert_eq!(
        draws_final,
        q_draws_final + (1 + FRAMES) as u64,
        "the hook run's game-stream draw count must exceed the control's by \
         exactly the hook draw count"
    );
    assert_ne!(
        state_final, q_state_final,
        "the hook draws must have SHIFTED the game stream (a split stream \
         would leave the two runs' game states identical)"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
