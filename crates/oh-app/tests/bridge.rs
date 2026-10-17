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
    let mut app = oh_app::bridge::AppDraw::new(
        &Config::default(),
        &release_dir().join("Assets"),
        font,
        font_bold,
    );
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
        let mut app = oh_app::bridge::AppDraw::new(
            &Config::default(),
            &release_dir().join("Assets"),
            font,
            font_bold,
        );
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
    let mut app = oh_app::bridge::AppDraw::new(
        &Config::default(),
        &release_dir().join("Assets"),
        font,
        font_bold,
    );
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

// ---------------------------------------------------------------------
// 029e2 (the graphics audit's M2): the per-frame character-size / origin
// half of the C++ `updateText` (HGGraphics.cpp:545-571, :593) must reach
// the render state every frame — the pre-fix port never called
// `text_draw::update_text`, so every `Text` kept its constructor size
// forever (status 19 = 25/1.27, message 29 = 38/1.27 left-anchored at
// (0, 0), PB a static 51 = 65/1.27, replay 15 = 20/1.27) and the
// engine's `pb_text_growth` was copied but unconsumed.
// ---------------------------------------------------------------------

/// The bridge applies the C++ per-frame sizes/origins (the `updateText`
/// size half) from the same engine values the C++ reads in one call.
#[test]
fn the_bridge_applies_the_per_frame_text_sizes_and_origins() {
    let assets = load_release();
    let c = compose(assets, release_dir(), true);
    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    c.game.set_must_start(true);
    // A live message (the C++ `e_messageAdd` → `messageText.setString`):
    // the origin assertion needs a non-empty message string.
    c.game.add_message("hello", 10.0, true);
    for _ in 0..20 {
        c.game.update(TIME_STEP, 1.0);
        c.game.post_update();
    }
    assert!(c.game.get_status().started, "the game auto-started");

    let (font, font_bold) = test_fonts();
    let mut app = oh_app::bridge::AppDraw::new(
        &Config::default(),
        &release_dir().join("Assets"),
        font,
        font_bold,
    );
    let mut rec = Recording::default();
    oh_app::bridge::draw(&c.game, &mut app, &mut rec);

    let st = app.state();
    // The C++ values at the default zoom 1.27 / text scaling 1.0
    // (HGGraphics.cpp:545-550, `(size / zoomFactor * textScaling)`
    // truncated): time 70 → 55, status 20 → 15 (the ctor kept 19 =
    // 25/1.27), message 32 → 25 (the ctor kept 29 = 38/1.27),
    // pb 64 → 50 + sin(growth) × 10 (the ctor kept a static 51 =
    // 65/1.27), replay 16 → 12 (the ctor kept 15 = 20/1.27).
    assert_eq!(st.time_text.get_character_size(), 55);
    assert_eq!(
        st.text.get_character_size(),
        15,
        "status 20/1.27 per frame, not the ctor's 19"
    );
    assert_eq!(
        st.text.get_origin(),
        oh_render::Vec2::new(0.0, 0.0),
        "the status-text origin (HGGraphics.cpp:557)"
    );
    assert_eq!(
        st.message_text.get_character_size(),
        25,
        "message 32/1.27 per frame, not the ctor's 29"
    );
    assert!(
        st.message_text.get_origin().x > 0.0,
        "the message text is centred (the C++'s getGlobalWidth/2 origin, :567 — the ctor left it at (0, 0))"
    );
    // PB: 50 + sin(20 × 0.25 × 0.08 = 0.4 rad) × 10 = 50 + 3.89 =
    // 53.89 → 53 — consuming the engine's sawtooth (the pre-fix port
    // copied `pb_text_growth` into the state but never read it).
    assert_eq!(
        st.pb_text.get_character_size(),
        53,
        "64/1.27 → 50 + sin(0.4) × 10, not the ctor's static 51"
    );
    // showFPS off (Config::default) + no replay UI → the fps / replay
    // texts keep their ctor sizes (the C++'s `getShowFPS()` /
    // `mustShowReplayUI()` gates, :560-564 / :574-598).
    assert_eq!(st.fps_text.get_character_size(), 19);
    assert_eq!(st.replay_text.get_character_size(), 15);
}

// ---------------------------------------------------------------------
// 029e1 (the graphics audit's M1): the HUD icon + particle textures must
// be the real `Assets/*.png` decodes — the pre-fix port drew all five
// key/replay icons and every PB/trail/swap particle from
// `Texture::new()`'s 1×1 white pixel (a `sf::Texture` default), where
// the C++ assigns the asset registry's textures: `initKeyIcons`'s
// `getTextureOrNullTexture` loop + `setSmooth(true)`
// (`HexagonGame.cpp:87-107`, called from the ctor `:453` and
// `onRecreation` `:339`) and the ctor window branch's
// `txStarParticle` / `txSmallCircle` (`:324-325`).
// ---------------------------------------------------------------------

/// The six textures are the REAL `Assets/` PNGs (their on-disk
/// dimensions, not the 1×1 placeholder), with the C++'s `setSmooth(true)`
/// on the four icon textures only (`HexagonGame.cpp:96-100` — the
/// particle textures are never smoothed, `:324-325`).
#[test]
fn app_draw_loads_the_real_hud_and_particle_textures() {
    let (font, font_bold) = test_fonts();
    let app = oh_app::bridge::AppDraw::new(
        &Config::default(),
        &release_dir().join("Assets"),
        font,
        font_bold,
    );
    let st = app.state();
    // The four key icons: `keyArrow.png` (64×64) on BOTH left and right
    // (`HexagonGame.cpp:102-103`), `keyFocus.png` / `keySwap.png` (64×64),
    // `replayIcon.png` (128×128) — the C++'s `setSmooth(true)`
    // (`HexagonGame.cpp:96-100`).
    for (name, icon) in [
        ("key_icon_left", st.key_icon_left.get_texture()),
        ("key_icon_right", st.key_icon_right.get_texture()),
        ("key_icon_focus", st.key_icon_focus.get_texture()),
        ("key_icon_swap", st.key_icon_swap.get_texture()),
    ] {
        assert_eq!(
            icon.get_size(),
            (64, 64),
            "{name} must be the 64×64 keyArrow/keyFocus/keySwap PNG"
        );
        assert!(icon.is_smooth(), "{name} carries the C++ setSmooth(true)");
    }
    let replay = st.replay_icon.get_texture();
    assert_eq!(replay.get_size(), (128, 128), "replayIcon.png is 128×128");
    assert!(replay.is_smooth());

    // The particle textures (the ctor window branch, `:324-325`) — NOT
    // smoothed (the C++ smooths only the four icon textures).
    assert_eq!(
        app.tx_star_particle().get_size(),
        (64, 64),
        "starParticle.png is 64×64"
    );
    assert!(!app.tx_star_particle().is_smooth());
    assert_eq!(
        app.tx_small_circle().get_size(),
        (16, 16),
        "smallCircle.png is 16×16"
    );
    assert!(!app.tx_small_circle().is_smooth());
}

/// A missing `Assets/` folder degrades to the C++'s `nullTexture`
/// (`Assets.cpp:568-575` — the `static sf::Texture` default = 1×1 white)
/// on every texture, exactly as the C++'s
/// `getTextureOrNullTexture` does for an absent asset — the game still
/// runs, drawing the placeholder squares (the pre-fix look, now only on
/// a broken install).
#[test]
fn app_draw_missing_assets_fall_back_to_the_cpp_null_textures() {
    let (font, font_bold) = test_fonts();
    let app = oh_app::bridge::AppDraw::new(
        &Config::default(),
        std::path::Path::new("/nonexistent-oh-029e1/assets"),
        font,
        font_bold,
    );
    let st = app.state();
    for icon in [
        st.key_icon_left.get_texture(),
        st.key_icon_right.get_texture(),
        st.key_icon_focus.get_texture(),
        st.key_icon_swap.get_texture(),
        st.replay_icon.get_texture(),
    ] {
        assert_eq!(icon.get_size(), (1, 1), "the nullTexture is 1×1");
    }
    assert_eq!(app.tx_star_particle().get_size(), (1, 1));
    assert_eq!(app.tx_small_circle().get_size(), (1, 1));
}
