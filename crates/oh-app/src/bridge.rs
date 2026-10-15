//! The D-020 draw bridge: `HexagonGame::draw()` (`HGGraphics.cpp:52-316`)
//! driven from the engine's state.
//!
//! The C++ `draw()` body is `oh_render::graphics::draw_frame` (issue 11) —
//! this module does ONLY the composition the C++ method gets from its own
//! members: the `window == nullptr` guard, the update-side
//! `updateKeyIcons` / `updateLevelInfo` calls (C++ runs them in
//! `update()` — `HGUpdate.cpp:213-224`; `oh-game` may not call
//! `oh-render`, so the composition owns them, D-013/D-020), the scene-batch
//! fills (`HGGraphics.cpp:127-146`, via
//! [`oh_game::hexagon_game::HexagonGame::draw_scene_batches`]), and the
//! per-frame `FrameInput` / `DrawState` assembly.
//!
//! # C++ draw-pass order (the `HGGraphics.cpp` lines this bridge implements)
//!
//! 1. `:54` `if(window == nullptr || Config::getDisableGameRendering())
//!    return;` — the engine's `window_present()` flag IS the C++
//!    `window != nullptr` (the `HexagonGame` ctor argument,
//!    `hexagon_game.rs`); the second conjunct is `draw_frame`'s own guard.
//! 2. `:59-78` the `getRenderStates` lambda — per-stage fragment-shader
//!    binding + the `onRenderStage(rs, 60.f / window->getFPS())` Lua
//!    dispatch (`:74-77`) — wired into `draw_frame` via
//!    `FrameInput.{fragment_shader_ids, on_render_stage, fps}`.
//! 3. `:83` `window->clear(sf::Color::Black)` — first event of
//!    `draw_frame`.
//! 4. `:85-106` camera centres (shake jitter / fixed) — `draw_frame` step 2
//!    via `FrameInput.{has_died, level_status_camera_shake}` + `jitter`
//!    (the non-game RNG, `:88-96`).
//! 5. `:108-121` background pass (view + `styleData.drawBackground` +
//!    `BackgroundTris` states) — `draw_frame` step 3.
//! 6. `:127-146` the scene-batch fills (walls → custom walls → the player
//!    when `status.started`) — run BEFORE `draw_frame`, here, via
//!    `draw_scene_batches`.
//! 7. `:149-263` the 3D layer copies + `WallQuads3D`/`PivotQuads3D`/
//!    `PlayerTris3D` renders — INSIDE `draw_frame` (it takes the pre-filled
//!    flat batches and builds the 3D copies itself; NOT duplicated here).
//! 8. `:265-278` trail/swap particles + the flat
//!    `WallQuads`/`CapTris`/`PivotQuads`/`PlayerTris` renders —
//!    `draw_frame` steps 6–7.
//! 9. `:280-283` overlay view + particles + `drawText` — `draw_frame` step
//!    8 (the text strings come from the engine's `updateText` values).
//! 10. `:285-297` key icons + level info — `draw_frame` steps 9–10 (their
//!     positions/sizes were set by the update-side calls in step 2 above).
//! 11. `:299-302` the flash polygon — `draw_frame` step 11 (the polygon is
//!     the engine's `flashPolygon`).
//! 12. `:305-313` the screenshot + `mustTakeScreenshot = false` —
//!     `draw_frame` step 12 (the flag is read from / written back to the
//!     engine around the call).
//!
//! (The C++ final step, `drawImguiLuaConsole()` at `:315`, is the ImGui
//! debug console — issue 14 step (b) territory, out of scope here.)
//!
//! # No lock guard is held across `draw_frame`
//!
//! `draw_frame` re-enters the engine (the `onRenderStage` hook →
//! `HexagonGame::run_on_render_stage` → the `ScriptHost`; the Lippincott
//! error path → `go_to_menu`). Every engine value the frame needs is
//! therefore SNAPSHOTTED into locals before the call (status/levelStatus
//! clones, style-data clone, config clone, the batch copies, the camera
//! conversions), and the snapshot locals — never a `st()` / `est()` /
//! `config_ref()` guard — are what `FrameInput` borrows.

use oh_config::Config;
use oh_game::hexagon_game::HexagonGame;
use oh_game::lua_state::SharedStore;
use oh_math::hg::color::Color;
use oh_render::camera::PreShakeCenters as RenderPreShakeCenters;
use oh_render::draw::{Vertex, VertexBatch};
use oh_render::font::{Font, Text};
use oh_render::graphics::{self, DrawState, DrawTarget, FragmentShaderIds, FrameInput};
use oh_render::texture::{Sprite, Texture};
use oh_render::types::RectangleShape;

use crate::camera::to_render_camera;

/// One `oh-game` scene vertex → one `oh-render` vertex (position + color;
/// the `oh-game` `FastVertexVectorTris` vertex carries an extra unused
/// `tex_coords` field — the game never sets it, per `vertex_batch.rs`).
fn vertex_from_engine(v: &oh_game::vertex_batch::Vertex) -> Vertex {
    Vertex::new(v.position, v.color)
}

/// Refill `dst` with the engine batch's vertices (the caller's per-frame
/// scratch; the C++ clears its member batches at `HGGraphics.cpp:127-132`).
fn copy_batch(src: &oh_game::FastVertexVectorTris, dst: &mut Vec<Vertex>) {
    dst.clear();
    dst.extend(src.as_slice().iter().map(vertex_from_engine));
}

/// C++ `HexagonGame::Particle/TrailParticle/SwapParticle` sprite fields
/// (`SpriteState`) → the render `Sprite` (position / rotation / scale /
/// color / origin — the texture is the composition's placeholder; the C++
/// constructor wires `txStarParticle` / `txSmallCircle` from the assets,
/// `HexagonGame.cpp:324-325` — step (c) replaces the placeholders with the
/// registry textures).
fn sprite_from_state(texture: &Texture, s: &oh_game::hexagon_game::SpriteState) -> Sprite {
    let mut sprite = Sprite::new(texture.clone());
    sprite.set_position(s.position);
    sprite.set_rotation(s.rotation_deg);
    sprite.set_scale(s.scale);
    sprite.set_color(s.color);
    sprite.set_origin(s.origin);
    sprite
}

/// The per-frame draw state the composition owns (D-020): the
/// `oh_render::graphics::DrawState` (cameras + pre-shake centres + the
/// particle/key-icon/level-info/text/flash members) + the frame's mutable
/// scratch (the four scene vertex batches, converted from the engine's
/// `FastVertexVectorTris`).
///
/// The cameras are `None` until the first `draw` on a windowed engine
/// (the C++ `backgroundCamera` / `overlayCamera` optionals are emplaced in
/// the constructor's `window != nullptr` branch,
/// `HexagonGame.cpp:318-331` — the engine mirrors that).
pub struct AppDraw {
    state: DrawState,
    /// C++ `window->getFPS()` — the per-frame window-reported rate that
    /// drives the `onRenderStage` frametime (`60.f / fps`,
    /// `HGGraphics.cpp:76`). Step (c) updates it from the window each
    /// frame; the headless tests set it explicitly.
    pub fps: f32,
    // Per-frame scratch (the C++ member batches `wallQuads` /
    // `pivotQuads` / `playerTris` / `capTris`).
    wall_quads: Vec<Vertex>,
    pivot_quads: Vec<Vertex>,
    player_tris: Vec<Vertex>,
    cap_tris: Vec<Vertex>,
    // The C++ `txStarParticle` / `txSmallCircle` (window-ctor branch,
    // `HexagonGame.cpp:324-325`) — placeholders in step (a); step (c)
    // loads the registry textures.
    tx_star_particle: Texture,
    tx_small_circle: Texture,
}

impl AppDraw {
    /// Composition-level construction — the C++ constructor's text/icon
    /// setup (`HexagonGame.cpp:276-331`): the game texts (`initText`,
    /// `:276-281`, character sizes divided by the zoom factor; `pbText` /
    /// `timeText` use the BOLD font, `:298` / `:308`), the level-info texts
    /// (`sf::Text{"", font}` — default character size 0, the sizes are set
    /// per frame by `updateLevelInfo`), and `initKeyIcons`
    /// (`HexagonGame.cpp:87-107` — set the sprite textures + the initial
    /// `updateKeyIcons`).
    #[must_use]
    pub fn new(config: &Config, font: Font, font_bold: Font) -> Self {
        let zoom = config.get_zoom_factor();

        let mut state = DrawState {
            // Emplaced per frame from the engine (windowed engines only).
            background_camera: None,
            overlay_camera: None,
            pre_shake_centers: None,
            // Filled per frame from the engine's `flashPolygon` /
            // particles (below `draw`).
            flash_polygon: VertexBatch::new_tris(),
            particles: Vec::new(),
            trail_particles: Vec::new(),
            swap_particles: Vec::new(),
            key_icon_left: Sprite::new(Texture::new()),
            key_icon_right: Sprite::new(Texture::new()),
            key_icon_focus: Sprite::new(Texture::new()),
            key_icon_swap: Sprite::new(Texture::new()),
            replay_icon: Sprite::new(Texture::new()),
            level_info_rectangle: RectangleShape::default(),
            // C++ `levelInfoTextLevel/Pack/Author/By/DM{"", font}`
            // (`HexagonGame.cpp:299-304`) — character size 0 until
            // `updateLevelInfo` sizes them.
            level_info_text_level: Text::new(font.clone(), "", 0),
            level_info_text_pack: Text::new(font.clone(), "", 0),
            level_info_text_author: Text::new(font.clone(), "", 0),
            level_info_text_by: Text::new(font.clone(), "", 0),
            level_info_text_dm: Text::new(font.clone(), "", 0),
            // C++ `fpsText{initText("0", font, 25.f)}` etc.
            // (`HexagonGame.cpp:307-310`).
            fps_text: graphics::init_text(&font, "0", 25.0, zoom),
            time_text: graphics::init_text(&font_bold, "0", 70.0, zoom),
            text: graphics::init_text(&font, "", 25.0, zoom),
            replay_text: graphics::init_text(&font, "", 20.0, zoom),
            message_text: graphics::init_text(&font, "", 38.0, zoom),
            pb_text: graphics::init_text(&font_bold, "", 65.0, zoom),
            pb_text_growth: 0.0,
        };

        // C++ `initKeyIcons` (`HexagonGame.cpp:87-107`) — the placeholder
        // textures stand in for the assets registry's
        // `keyArrow/keyFocus/keySwap/replayIcon.png` (step (c) wires the
        // registry ones; the layout/rotation/scale behaviour under test
        // is texture-independent).
        let mut key_arrow = Texture::new();
        let mut key_focus = Texture::new();
        let mut key_swap = Texture::new();
        let mut replay = Texture::new();
        graphics::init_key_icons(
            &mut state.key_icon_left,
            &mut state.key_icon_right,
            &mut state.key_icon_focus,
            &mut state.key_icon_swap,
            &mut state.replay_icon,
            &mut key_arrow,
            &mut key_focus,
            &mut key_swap,
            &mut replay,
            config,
        );

        Self {
            state,
            fps: 60.0,
            wall_quads: Vec::new(),
            pivot_quads: Vec::new(),
            player_tris: Vec::new(),
            cap_tris: Vec::new(),
            tx_star_particle: Texture::new(),
            tx_small_circle: Texture::new(),
        }
    }

    /// The owned draw state (camera + pre-shake inspection for tests;
    /// step (c) may extend it).
    #[must_use]
    pub fn state(&self) -> &DrawState {
        &self.state
    }
}

/// C++ `HexagonGame::draw()` (`HGGraphics.cpp:52-316`) — the bridge:
/// snapshot the engine's draw-observable state, assemble
/// `FrameInput`/`DrawState`, call [`oh_render::graphics::draw_frame`].
///
/// Generic over the engine's shared store `S` (the spec's
/// `HexagonGame<GameApiState>` is the composition's concrete store; the
/// lib stays agnostic so pure-`SharedState` engines draw too — and so
/// `oh-app`'s lib does not depend on `oh-lua`).
pub fn draw<S: SharedStore>(
    game: &HexagonGame<S>,
    app: &mut AppDraw,
    target: &mut impl DrawTarget,
) {
    // C++ `HGGraphics.cpp:54` — first conjunct. The engine's `window` ctor
    // flag IS the C++ `window != nullptr` (no window → no cameras → the
    // C++ early-returns before anything is observed). The second conjunct
    // (`Config::getDisableGameRendering()`) is `draw_frame`'s own guard.
    if !game.window_present() {
        return;
    }

    // ------------------------------------------------------------------
    // Update-side icon/level-info layout (C++ `HGUpdate.cpp:213-224`).
    // `oh-game` cannot call `oh-render` (D-013/D-020), so the composition
    // drives these each frame with the same C++ gates. Both C++ methods
    // also carry their own `window == nullptr` return — already satisfied
    // by the guard above.
    let must_show_replay_ui = game.must_show_replay_ui();
    if game.config_ref().get_show_key_icons() || must_show_replay_ui {
        graphics::update_key_icons(
            &mut app.state.key_icon_left,
            &mut app.state.key_icon_right,
            &mut app.state.key_icon_focus,
            &mut app.state.key_icon_swap,
            &mut app.state.replay_icon,
            &game.config_ref(),
        );
    }
    if game.config_ref().get_show_level_info() || must_show_replay_ui {
        // C++ `updateLevelInfo` (`HexagonGame.cpp:159-186`): offset color =
        // `Black` in B&W / empty colours, else `styleData.getColor(0)`;
        // outline = `styleData.getMainColor()` (the RAW style colour — NOT
        // the B&W-adjusted `getColorMain()`).
        let style = game.get_style_data();
        let offset_color =
            if game.config_ref().get_black_and_white() || style.get_colors().is_empty() {
                Color::BLACK
            } else {
                style.get_color(0).copied().unwrap_or(Color::BLACK)
            };
        let level = game.get_level_data();
        graphics::update_level_info(
            &mut app.state,
            &game.config_ref(),
            game.get_color_text(),
            offset_color,
            *style.get_main_color(),
            &level.name,
            &game.get_pack_name(),
            &level.author,
            level.difficulty_mults.len(),
            game.difficulty_mult(),
        );
    }

    // ------------------------------------------------------------------
    // Cameras + pre-shake centres (mapping §4.1 / the C++ optional
    // members). `None` on a windowed engine is impossible (the ctor
    // emplaces both) — but the C++ asserts, the port mirrors: `draw_frame`
    // `expect`s them.
    app.state.background_camera = game.background_camera().map(|c| to_render_camera(&c));
    app.state.overlay_camera = game.overlay_camera().map(|c| to_render_camera(&c));
    app.state.pre_shake_centers = game.pre_shake_centers().map(|p| RenderPreShakeCenters {
        background: p.background,
        overlay: p.overlay,
    });

    // ------------------------------------------------------------------
    // Flash polygon (the C++ `flashPolygon`; alpha animated by the engine's
    // `updateFlash` — the bridge only copies vertices).
    {
        let flash = game.flash_polygon();
        let batch = &mut app.state.flash_polygon;
        batch.clear();
        batch.reserve(flash.size());
        for v in flash.as_slice() {
            batch.push_pos_color(v.position, v.color);
        }
    }

    // ------------------------------------------------------------------
    // Particles (the C++ `particles` / `trailParticles` /
    // `swapParticles`; the update loop maintains them — the bridge copies
    // state, textures from the composition's `txStarParticle` /
    // `txSmallCircle`).
    app.state.particles = game
        .particles()
        .into_iter()
        .map(|p| graphics::Particle {
            sprite: sprite_from_state(&app.tx_star_particle, &p.sprite),
            velocity: p.velocity,
            angular_velocity: p.angular_velocity,
        })
        .collect();
    app.state.trail_particles = game
        .trail_particles()
        .into_iter()
        .map(|p| graphics::TrailParticle {
            sprite: sprite_from_state(&app.tx_small_circle, &p.sprite),
            angle: p.angle,
        })
        .collect();
    app.state.swap_particles = game
        .swap_particles()
        .into_iter()
        .map(|p| graphics::SwapParticle {
            sprite: sprite_from_state(&app.tx_small_circle, &p.sprite),
            velocity: p.velocity,
        })
        .collect();

    // ------------------------------------------------------------------
    // Texts (the C++ `updateText` values — the engine owns the strings;
    // the `Text` objects live here).
    app.state.fps_text.set_string(game.fps_text());
    app.state.time_text.set_string(game.time_text());
    app.state.text.set_string(game.status_text());
    app.state.replay_text.set_string(game.replay_text());
    app.state.message_text.set_string(game.message_text());
    app.state.pb_text.set_string(game.pb_text());
    app.state.pb_text_growth = game.pb_text_growth();

    // ------------------------------------------------------------------
    // The scene batches — the C++ `HGGraphics.cpp:127-146` fill
    // orchestration (walls → custom walls → the player when
    // `status.started`), run BEFORE `draw_frame`'s 3D/flat passes.
    let (wall_quads, pivot_quads, player_tris, cap_tris) = game.draw_scene_batches();
    copy_batch(&wall_quads, &mut app.wall_quads);
    copy_batch(&pivot_quads, &mut app.pivot_quads);
    copy_batch(&player_tris, &mut app.player_tris);
    copy_batch(&cap_tris, &mut app.cap_tris);

    // ------------------------------------------------------------------
    // FrameInput — everything `draw_frame` reads besides the DrawState
    // (SNAPSHOTS only — no engine lock guard crosses `draw_frame`; the
    // onRenderStage hook and its Lippincott error path re-enter the
    // engine).
    let config = game.config_ref().clone(); // C++ static `Config`
    let status = game.get_status();
    let level_status = game.get_level_status();
    let style = game.get_style_data();
    let mut must_take_screenshot = game.must_take_screenshot();
    // C++ `std::array<std::optional<std::size_t>>` ← the engine's
    // `[Option<u64>; 9]` (u64 → usize is exact on this port's targets).
    let fragment_shader_ids: FragmentShaderIds = status
        .fragment_shader_ids
        .map(|id| id.map(|id| id as usize));

    let jitter: Box<dyn Fn(f32, f32) -> f32> = Box::new(|a, b| game.get_rnd_r(a, b));
    // C++ `runLuaFunctionIfExists<int, float>("onRenderStage", rs,
    // 60.f / window->getFPS())` (`HGGraphics.cpp:74-77`); the frametime is
    // computed by `draw_frame`'s `getRenderStates` closure (`60.0 /
    // input.fps`) and passed to the hook.
    let on_render_stage: Box<dyn Fn(oh_render::graphics::RenderStage, f32)> = Box::new(
        |stage, frametime| {
            if let Err(msg) = game.run_on_render_stage(stage.as_usize() as i32, frametime) {
                // C++ `luaExceptionLippincottHandler`
                // (`HGScripting.cpp:1155-1183`): print + `goToMenu(false,
                // true)` unless debug.
                println!(
                    "[runLuaFunctionIfExists] Runtime error on \"onRenderStage\" with level \"{}\": \n{}\n",
                    game.get_level_data().name, msg
                );
                if !game.config_ref().get_debug() {
                    game.go_to_menu(false, true);
                }
            }
        },
    );

    let mut input = FrameInput {
        config: &config,
        fragment_shader_ids: &fragment_shader_ids,
        has_died: status.has_died,
        level_status_camera_shake: level_status.camera_shake,
        sides: level_status.sides,
        darken_uneven_background_chunk: level_status.darken_uneven_background_chunk,
        show_player_trail: status.show_player_trail,
        pulse3d: status.pulse_3d,
        must_show_replay_ui,
        must_take_screenshot: &mut must_take_screenshot,
        fps: app.fps,
        input_movement: game.get_input_movement(),
        input_focused: game.get_input_focused(),
        input_swap: game.get_input_swap(),
        color_text: game.get_color_text(),
        wall_quads: &app.wall_quads,
        pivot_quads: &app.pivot_quads,
        player_tris: &app.player_tris,
        cap_tris: &app.cap_tris,
        background_colors: style.get_colors(),
        bg_tile_radius: style.bg_tile_radius,
        bg_rot_off: style.bg_rot_off,
        three_d_depth: style._3d_depth,
        three_d_skew: style._3d_skew,
        three_d_spacing: style._3d_spacing,
        three_d_darken_mult: style._3d_darken_mult,
        three_d_alpha_mult: style._3d_alpha_mult,
        three_d_alpha_falloff: style._3d_alpha_falloff,
        three_d_perspective_mult: style._3d_perspective_mult,
        three_d_override_color: *style.get_3d_override_color(),
        main_color: *style.get_main_color(),
        wall_color: game.get_color_wall(),
        player_color: game.get_color_player(),
        // C++ `HexagonGame::draw()` never renders the menu-hexagon
        // background — that is `MenuGame`'s (`MenuGame.cpp:5780`,
        // `drawBackgroundMenu`); the `FrameInput` field exists for the
        // menu composition (issue 12/15c), so the game bridge passes
        // `false` (and `menu_four_by_three` / `cap_color` are inert).
        menu_background_enabled: false,
        menu_four_by_three: false,
        cap_color: style.get_cap_color_result().unwrap_or(Color::BLACK),
        jitter: &jitter,
        on_render_stage: Some(&on_render_stage),
    };

    graphics::draw_frame(&mut app.state, &mut input, target);

    // C++ `mustTakeScreenshot = false` (`HGGraphics.cpp:312`) — `draw_frame`
    // cleared the local only when it saved the screenshot; write the
    // (possibly cleared) value back to the engine.
    game.set_must_take_screenshot(must_take_screenshot);
}
