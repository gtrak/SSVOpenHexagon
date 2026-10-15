//! Issue 15 step (a): the D-020 draw bridge — `HexagonGame::draw()`
//! (`HGGraphics.cpp:52-316`) driven from the engine's state.
//!
//! See the module docs in `src/bridge.rs` for the C++ draw-pass order the
//! bridge implements and the no-lock-guard-across-`draw_frame` discipline.

mod common;

use std::sync::Arc;

use oh_config::{Config, TIME_STEP};
use oh_game::seams::ScriptHost;
use oh_render::draw::RenderStates;

use common::{compose, release_dir, test_fonts, DrawEvent, Recording};

fn load_release() -> Arc<oh_assets::Assets> {
    // C++: `hg::HGAssets assets{nullptr, true /* headless */};`
    Arc::new(oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"))
}

/// The exact pack/level ids the C++ test names (re-derived via
/// `buildPackId` against the shipped `pack.json`s — D-018; the same ids
/// as `oh-game`'s `tests/determinism.rs`).
const PACK: &str = "ohvrvanilla_vittorio_romeo_cube_1";
const LEVEL: &str = "ohvrvanilla_vittorio_romeo_cube_1_apeirogon";

/// A window=false engine → `draw` is a no-op (no clear, no vertices):
/// the C++ `if(window == nullptr || ...) return;` guard
/// (`HGGraphics.cpp:54`) — the engine's `window` ctor flag models
/// `window != nullptr`, so nothing after the guard is observed.
#[test]
fn draw_is_a_no_op_without_a_window() {
    let assets = load_release();
    let c = compose(assets, release_dir(), false);
    c.game.new_game(PACK, LEVEL, true, 1.0, false);

    let (font, font_bold) = test_fonts();
    let mut app = oh_app::bridge::AppDraw::new(&Config::default(), font, font_bold);
    let mut rec = Recording::default();
    oh_app::bridge::draw(&c.game, &mut app, &mut rec);

    assert!(rec.events.is_empty(), "window == nullptr ⇒ no events");
    // The bridge must not have touched the draw state either.
    assert!(app.state().background_camera.is_none());
    assert!(app.state().overlay_camera.is_none());
}

/// Binds the distinct fragment-shader ids the C++ `draw()` would bind when
/// every stage has a shader (the shipped levels define none — the
/// discrimination happens on the STATE the bridge forwards, so the test
/// pins all nine explicitly). Stage `i` maps to shader `100 + i`.
fn bind_all_stages(game: &oh_game::hexagon_game::HexagonGame<oh_lua::api::state::GameApiState>) {
    let mut g = game.state_handle().lock();
    g.status.fragment_shader_ids = [
        Some(100),
        Some(101),
        Some(102),
        Some(103),
        Some(104),
        Some(105),
        Some(106),
        Some(107),
        Some(108),
    ];
}

/// The `alwaysSpinRight` replay stream — the C++ test's policy pinned by
/// `oh-game`'s `tests/determinism.rs`: CW-only bits (bit 1 = right →
/// byte `0x02`). Built directly instead of live-run-captured: the C++
/// `ReplayExecution` flow is `live run → replay file →
/// runReplayUntilDeathAndGetScore(rf)`; the bridge test needs the second
/// phase (the replay path `setLastReplay` + `newGame(executeLastReplay)`,
/// which is what makes the seed — and therefore the whole run —
/// identical across engines). A constructed `rf` pins that seed exactly
/// (`seed 0`) without a wall-clock-budgeted live run (the C++ `1s`
/// processing budget is a safety net, not a behavior pin — the oh-game
/// determinism harness owns the live phase).
#[must_use]
fn always_spin_right_replay() -> oh_replay::ReplayFile {
    let mut data = oh_replay::ReplayData::default();
    // 3000 ticks = 50 s of game time — far past the first wall arrival
    // (walls spawn at the 1600 unit spawn distance; the player sits at
    // ~75 units); the run stops at death regardless.
    for _ in 0..3000 {
        data.record_input(false, true, false, false);
    }
    oh_replay::ReplayFile {
        version: 0,
        player_name: "testProfile".to_string(),
        seed: 0,
        data,
        pack_id: PACK.to_string(),
        level_id: LEVEL.to_string(),
        first_play: true,
        difficulty_mult: 1.0,
        played_score: 0.0,
    }
}

/// The full-frame bridge test (spec: "real composition … run a few ticks
/// like `tests/determinism.rs` does"): TWO fresh WINDOWED engines replay
/// the same `alwaysSpinRight` stream (seed 0) to death — a few ticks per
/// `TIME_STEP`, the level's `.lua` running for real — and draw one frame
/// each. Asserts:
///
/// 1. `clear_black` is the first event (C++ `:83`);
/// 2. the C++ pass order: background (`:108-121`) before the 3D layers
///    (`:261-263`) before the flat wall/cap/pivot/player passes
///    (`:275-278`);
/// 3. the wall batch is non-empty (the player died to a wall);
/// 4. the wall vertex count — and the WHOLE recording — is deterministic
///    across the two identical runs (same seed → same frame);
/// 5. `must_take_screenshot` is consumed (`:305-313`): the frame ends with
///    the `screenshot.png` save and the engine flag is cleared.
#[test]
fn full_frame_passes_cpp_order_and_is_deterministic() {
    let assets = load_release();
    let root = release_dir();
    let rf = always_spin_right_replay();

    // ---- runs A + B (windowed replays): draw one frame each -----------
    let mut recordings = Vec::new();
    let mut wall_vertex_counts = Vec::new();
    for _ in 0..2 {
        let c = compose(assets.clone(), root.clone(), true);
        // C++ `runReplayUntilDeathAndGetScore`'s first two steps:
        // `setLastReplay(rf)` + `newGame(..., executeLastReplay = true)`.
        c.game.set_last_replay(rf.clone());
        c.game.new_game(
            &rf.pack_id,
            &rf.level_id,
            rf.first_play,
            rf.difficulty_mult,
            true,
        );
        // A few ticks like the determinism harness (C++
        // `executeGameUntilDeath`'s loop body: `update` + `post_update`),
        // to death. The replay input (CW every tick) auto-starts the game
        // (the C++ `HGUpdate.cpp:650-658` `mustStart = true` branch).
        for _ in 0..3000 {
            if c.game.get_status().has_died {
                break;
            }
            c.game.update(TIME_STEP, 1.0);
            c.game.post_update();
        }
        assert!(
            c.game.get_status().has_died,
            "the replay run must end at death within the stream"
        );

        // Every stage gets a distinct shader id → the per-stage states in
        // the recording identify the C++ render stage (discrimination).
        bind_all_stages(&c.game);
        // C++ `mustTakeScreenshot` armed before the frame (`:305`).
        c.game.set_must_take_screenshot(true);

        let (font, font_bold) = test_fonts();
        let mut app = oh_app::bridge::AppDraw::new(&Config::default(), font, font_bold);
        app.fps = 60.0; // C++ `window->getFPS()` — the test's window rate
        let mut rec = Recording::default();
        oh_app::bridge::draw(&c.game, &mut app, &mut rec);

        // 5. screenshot consumed: LAST event + engine flag cleared.
        let DrawEvent::Screenshot(path) = rec.events.last().expect("events") else {
            panic!("last event must be the screenshot save");
        };
        assert_eq!(path, "screenshot.png");
        assert!(
            !c.game.must_take_screenshot(),
            "mustTakeScreenshot cleared after the frame"
        );

        // 1. clear first.
        assert_eq!(rec.events.first().expect("events"), &DrawEvent::Clear);

        // 2. pass order (stage ids: 0 background, 1-3 the 3D layers,
        //    4 wall, 5 cap, 6 pivot, 7 player — the C++ `:261-278` order).
        let idx = |stage: usize| -> usize {
            rec.events
                .iter()
                .position(|e| {
                    matches!(
                        e,
                        DrawEvent::Vertices { states, .. }
                            if *states == RenderStates::with_shader(100 + stage)
                    )
                })
                .unwrap_or_else(|| panic!("stage {stage} (shader {})", 100 + stage))
        };
        let i_background = idx(0);
        let i_wall_3d = idx(1);
        let i_pivot_3d = idx(2);
        let i_player_3d = idx(3);
        let i_wall = idx(4);
        let i_cap = idx(5);
        let i_pivot = idx(6);
        let i_player = idx(7);
        assert!(
            i_background < i_wall_3d
                && i_wall_3d < i_pivot_3d
                && i_pivot_3d < i_player_3d
                && i_player_3d < i_wall
                && i_wall < i_cap
                && i_cap < i_pivot
                && i_pivot < i_player,
            "C++ pass order: background < 3D layers < wall < cap < pivot < player"
        );

        // 3. wall batch non-empty; the 3D wall layer is the pre-filled
        //    batch repeated `_3d_depth` times (apeirogon style: 8 —
        //    `HGGraphics.cpp:149-152`'s `numWallQuads * depth`).
        let count = |stage: usize| -> usize {
            let e = &rec.events[idx(stage)];
            let DrawEvent::Vertices { count, .. } = e else {
                panic!()
            };
            *count
        };
        let wall_count = count(4);
        assert!(wall_count > 0, "the wall batch must be non-empty");
        assert_eq!(
            count(1),
            8 * wall_count,
            "3D wall layer = depth × wallQuads"
        );
        // Background = `sides * 3` vertices (apeirogon: `l_setSides(6)`).
        assert_eq!(count(0), c.game.get_level_status().sides as usize * 3);

        wall_vertex_counts.push(wall_count);
        recordings.push(rec);
    }

    // 4. determinism: same seed → same wall count AND the whole
    //    recorded frame (every event, every state).
    assert_eq!(wall_vertex_counts[0], wall_vertex_counts[1]);
    assert_eq!(
        recordings[0].events, recordings[1].events,
        "same seed ⇒ identical recording"
    );
}

/// `onRenderStage` (spec: "with a level whose style/level Lua defines it,
/// assert the callback fires per stage with the C++ frametime argument
/// (60/fps)"): the level's Lua context defines `onRenderStage(rs, ft)`;
/// with every stage shader-bound the hook fires once per rendered stage
/// (`HGGraphics.cpp:74-77` + the `getRenderStates` call sites
/// `:119/:261-263/:275-278/:283`), and `ft == 60 / fps` (fps 120 ⇒ 0.5).
#[test]
fn on_render_stage_fires_per_stage_with_frametime() {
    let assets = load_release();
    let c = compose(assets, release_dir(), true);
    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    c.game.set_must_start(true);
    // A few ticks like the determinism harness (the level's `onUpdate`
    // runs for real; walls spawn).
    for _ in 0..20 {
        c.game.update(TIME_STEP, 1.0);
        c.game.post_update();
    }
    assert!(c.game.get_status().started, "the game auto-started");

    // The level-side hook (C++ `runLuaFunctionIfExists` semantics: the
    // global must exist; the int return is discarded).
    c.host
        .execute_code(
            "rsLog = ''\nfunction onRenderStage(rs, ft)\n\
             rsLog = rsLog .. rs .. ',' .. ft .. ';'\n\nend",
        )
        .expect("onRenderStage definition");

    // Every stage bound → the hook fires for each rendered stage.
    bind_all_stages(&c.game);

    let (font, font_bold) = test_fonts();
    let mut app = oh_app::bridge::AppDraw::new(&Config::default(), font, font_bold);
    app.fps = 120.0; // C++ frametime argument = 60.f / fps = 0.5
    let mut rec = Recording::default();
    oh_app::bridge::draw(&c.game, &mut app, &mut rec);

    let log = c
        .host
        .read_variable_string("rsLog")
        .expect("rsLog readable");
    // 9 stages: 0 (background) … 7 (player) + 8 (Text — `drawText`;
    // level info re-evaluates Text only when shown, which it is not).
    let expected = (0..9).map(|s| format!("{s},0.5;")).collect::<String>();
    assert_eq!(
        log, expected,
        "the hook fired once per stage with the C++ frametime (60/120)"
    );
    assert!(!rec.events.is_empty(), "the frame rendered");
}
