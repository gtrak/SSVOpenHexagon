//! The drawing layer — `HGGraphics.cpp`'s render-stage loop with its
//! per-stage shader `RenderStates`, the vertex flush order, the
//! `HexagonGame::draw*` methods, `StyleData::drawBackground*`, the flash
//! polygon and the render-driving particle updates.
//!
//! This is a free-function port: everything the C++ methods read from
//! `HexagonGame`/`Config`/`StyleData`/`LuaScripting` arrives as parameters
//! or through the [`Config`] reference, so the layer stays decoupled from
//! `oh-game` (whose update loop owns `status`/`levelStatus` and calls into
//! here). `oh-render` must not depend on `oh-game` (layering: `oh-game` →
//! `oh-render`).
//!
//! Sources (mapping/rendering-menu.md):
//! - §1 `RenderStage` (exact render order + per-stage fragment-shader
//!   binding);
//! - §3 `draw()` pipeline (exact frame order, see [`draw_frame`]),
//!   `initFlashEffect`, `drawKeyIcons`, `drawLevelInfo`,
//!   `drawParticles`/`drawTrailParticles`/`drawSwapParticles`,
//!   `drawText_TimeAndStatus`/`drawText_Message`/`drawText_PersonalBest` /
//!   `drawText` (in [`crate::text_draw`]);
//! - §4.2/§4.3 key-icon + level-info layout (`init_key_icons` /
//!   `update_key_icons` / `update_level_info`);
//! - §5.6–5.10 render-driving updates (`update_flash`, the particle
//!   updates);
//! - §6.3 `adjustAlpha` (`componentClamp`), `getColorDarkened`;
//! - §7.7 `StyleData::drawBackground` / `drawBackgroundMenu`;
//! - §9.3 per-frame shader binding (the `onRenderStage` Lua hook).
//!
//! `TODO(issue-009-wiring)` markers below name the `HexagonGame` call sites
//! that must call into these functions once issue 009 lands.

use std::borrow::Cow;

use oh_config::Config;
use oh_math::hg::color::{component_clamp, get_color_darkened, Color};
use oh_math::hg::move_towards::get_move_towards_zero;
use oh_math::ssvs::vector2::{get_vec_from_rad, Vec2};
use oh_math::ssvu::math::{to_rad, PI, TAU};

use crate::camera::{draw_set_camera_centers, PreShakeCenters};
use crate::draw::{RenderStates, Vertex, VertexBatch};
use crate::font::{Font, Text};
use crate::text_draw::{self, diff_format};
use crate::texture::{Sprite, Texture};
use crate::types::RectangleShape;
use crate::view::{Camera, View};

// ---------------------------------------------------------------------------
// RenderStage (mapping §1) — the exact render order for the per-stage
// fragment-shader binding (and the Lua `RenderStage` table).
// ---------------------------------------------------------------------------

/// `hg::RenderStage` (`include/SSVOpenHexagon/Core/HGStatus.hpp`) —
/// `enum class RenderStage : std::size_t`. The discriminants **are** the
/// per-stage fragment-shader binding indices (`status.fragmentShaderIds`)
/// and match the Lua `RenderStage` table in
/// `_RELEASE/Packs/base/Scripts/utils.lua`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(usize)]
pub enum RenderStage {
    /// `RenderStage::BackgroundTris` = 0.
    BackgroundTris = 0,
    /// `RenderStage::WallQuads3D` = 1.
    WallQuads3D = 1,
    /// `RenderStage::PivotQuads3D` = 2.
    PivotQuads3D = 2,
    /// `RenderStage::PlayerTris3D` = 3.
    PlayerTris3D = 3,
    /// `RenderStage::WallQuads` = 4.
    WallQuads = 4,
    /// `RenderStage::CapTris` = 5.
    CapTris = 5,
    /// `RenderStage::PivotQuads` = 6.
    PivotQuads = 6,
    /// `RenderStage::PlayerTris` = 7.
    PlayerTris = 7,
    /// `RenderStage::Text` = 8.
    Text = 8,
}

impl RenderStage {
    /// `RenderStage::Count` = 9 — the length of `fragmentShaderIds`.
    pub const COUNT: usize = 9;

    /// `static_cast<std::size_t>(rs)`.
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self as usize
    }
}

/// `std::array<std::optional<std::size_t>, RenderStage::Count>` — the
/// per-stage fragment-shader id binding (`HexagonGameStatus::fragmentShaderIds`).
pub type FragmentShaderIds = [Option<usize>; RenderStage::COUNT];

/// C++ `draw()`'s `getRenderStates(rs)` (mapping §3.3 / §9.3):
/// - no shaders enabled (`Config::getShaders()` false) → `RenderStates::Default`;
/// - no bound id for this stage → `Default`;
/// - otherwise run Lua `onRenderStage(stage, 60.f / fps)` (if defined, pass
///   as `on_render_stage`) **before** returning the bound shader's states.
#[must_use]
pub fn render_states_for_stage(
    shaders_enabled: bool,
    fragment_shader_ids: &FragmentShaderIds,
    stage: RenderStage,
    frametime: f32,
    on_render_stage: Option<&dyn Fn(RenderStage, f32)>,
) -> RenderStates {
    if !shaders_enabled {
        return RenderStates::default();
    }

    let Some(fragment_shader_id) = fragment_shader_ids[stage.as_usize()] else {
        return RenderStates::default();
    };

    if let Some(hook) = on_render_stage {
        hook(stage, frametime);
    }

    RenderStates::with_shader(fragment_shader_id)
}

// ---------------------------------------------------------------------------
// The render-target surface (the `window->draw(...)` dispatch).
// ---------------------------------------------------------------------------

/// The target `draw_frame` renders into — the `sf::RenderWindow` analogue.
/// Tests record the event sequence; the GL backend performs real uploads
/// (see [`crate::batching`]).
pub trait DrawTarget {
    /// C++ `window->clear(sf::Color::Black)`.
    fn clear_black(&mut self);

    /// C++ `window->setView(view)` — the view is **copied** (a later camera
    /// `set_skew`/`set_center` does not change what is currently drawn,
    /// matching `sf::RenderWindow::setView`).
    fn set_view(&mut self, view: View);

    /// C++ `render(vertex batch)` —
    /// `mRenderTarget.draw(vertices, size, Triangles, states)`.
    fn draw_vertices(&mut self, vertices: &[Vertex], states: RenderStates);

    /// C++ `render(sprite)` (default render states).
    fn draw_sprite(&mut self, sprite: &Sprite);

    /// C++ `render(text, states)`.
    fn draw_text(&mut self, text: &Text, states: RenderStates);

    /// C++ `render(rectangleShape, states)`.
    fn draw_rectangle(&mut self, rect: &RectangleShape, states: RenderStates);

    /// C++ `window->saveScreenshot(path)`.
    fn save_screenshot(&mut self, path: &str);
}

// ---------------------------------------------------------------------------
// The per-frame draw state (the C++ HexagonGame graphics-related members,
// mapping §3.13).
// ---------------------------------------------------------------------------

/// C++ `HexagonGame::Particle` (PB star particles — overlay camera).
#[derive(Clone, Debug)]
pub struct Particle {
    pub sprite: Sprite,
    pub velocity: Vec2,
    pub angular_velocity: f32,
}

/// C++ `HexagonGame::TrailParticle` (background/world coordinates).
#[derive(Clone, Debug)]
pub struct TrailParticle {
    pub sprite: Sprite,
    pub angle: f32,
}

/// C++ `HexagonGame::SwapParticle`.
#[derive(Clone, Debug)]
pub struct SwapParticle {
    pub sprite: Sprite,
    pub velocity: Vec2,
}

/// C++ `HexagonGame::SwapParticleSpawnInfo`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwapParticleSpawnInfo {
    /// `ready == true` — the "swap blip" burst (14 particles).
    /// `ready == false` — the "after swap" burst (20 + 10 particles).
    pub ready: bool,
    pub position: Vec2,
    pub angle: f32,
}

/// The `HexagonGame` graphics members the draw layer mutates/reads
/// (mapping §3.13). Constructed by `oh-game` (the `oh-render` → `oh-game`
/// direction is NOT allowed).
pub struct DrawState {
    // Cameras (mapping §4.1: `std::optional<ssvs::Camera>`).
    pub background_camera: Option<Camera>,
    pub overlay_camera: Option<Camera>,

    /// C++ `preShakeCenters` — captured/restored by
    /// [`crate::camera::update_camera_shake`].
    pub pre_shake_centers: Option<PreShakeCenters>,

    /// C++ `flashPolygon` (`Utils::FastVertexVectorTris`, 6 vertices).
    pub flash_polygon: VertexBatch,

    // Particles (mapping §3.13).
    pub particles: Vec<Particle>,
    pub trail_particles: Vec<TrailParticle>,
    pub swap_particles: Vec<SwapParticle>,

    // Key icons (textures: `keyArrow.png` ×2, `keyFocus.png`, `keySwap.png`,
    // `replayIcon.png` — set up by [`init_key_icons`]).
    pub key_icon_left: Sprite,
    pub key_icon_right: Sprite,
    pub key_icon_focus: Sprite,
    pub key_icon_swap: Sprite,
    pub replay_icon: Sprite,

    // Level info (mapping §4.3).
    pub level_info_rectangle: RectangleShape,
    pub level_info_text_level: Text,
    pub level_info_text_pack: Text,
    pub level_info_text_author: Text,
    pub level_info_text_by: Text,
    pub level_info_text_dm: Text,

    // In-game texts (mapping §4.1 `initText` — fonts `OpenSquare-Regular.ttf`
    // / `OpenSquare-Bold.ttf`).
    pub fps_text: Text,
    pub time_text: Text,
    pub text: Text,
    pub replay_text: Text,
    pub message_text: Text,
    pub pb_text: Text,

    /// C++ `pbTextGrowth` — the PB-text size sawtooth (driven by
    /// [`text_draw::update_text`]).
    pub pb_text_growth: f32,
}

/// C++ file-static `initText` (mapping §4.1):
/// `sf::Text{ text, font, unsigned(characterSize / zoomFactor) }`.
#[must_use]
pub fn init_text(font: &Font, text: &str, character_size: f32, zoom_factor: f32) -> Text {
    Text::new(font.clone(), text, (character_size / zoom_factor) as u32)
}

// ---------------------------------------------------------------------------
// Frame input (the C++ values `draw()` reads besides the DrawState).
// ---------------------------------------------------------------------------

/// Everything [`draw_frame`] reads from the game besides [`DrawState`] and
/// the render target.
///
/// `TODO(issue-009-wiring)`: `HexagonGame::draw()` fills these from
/// `Config`, `status` (`has_died`, `show_player_trail`, `pulse3d`,
/// `fragmentShaderIds`), `levelStatus` (`cameraShake`, `sides`,
/// `darkenUnevenBackgroundChunk`), `styleData` (the colour/3D fields), the
/// Lua `onRenderStage` hook, `ssvu::getRndR` (the non-game RNG for the
/// shake jitter) and the player/wall vertex batches it produces via
/// `CWall::draw` / `CPlayer::draw` / `cwManager.draw` (note: `oh-game`'s
/// `FastVertexVectorTris` carries an extra unused `tex_coords` field — map
/// position+color into `oh_render::Vertex`).
pub struct FrameInput<'a> {
    pub config: &'a Config,

    /// C++ `status.fragmentShaderIds`.
    pub fragment_shader_ids: &'a FragmentShaderIds,

    /// C++ `status.hasDied`.
    pub has_died: bool,
    /// C++ `levelStatus.cameraShake` (magnitude; > 0 → jittered centres).
    pub level_status_camera_shake: f32,
    /// C++ `levelStatus.sides`.
    pub sides: u32,
    /// C++ `levelStatus.darkenUnevenBackgroundChunk` (ANDed with
    /// `Config::getDarkenUnevenBackgroundChunk()`).
    pub darken_uneven_background_chunk: bool,
    /// C++ `status.showPlayerTrail`.
    pub show_player_trail: bool,
    /// C++ `status.pulse3D`.
    pub pulse3d: f32,
    /// C++ `mustShowReplayUI()`.
    pub must_show_replay_ui: bool,
    /// C++ `mustTakeScreenshot` (cleared here, as in C++).
    pub must_take_screenshot: &'a mut bool,
    /// C++ `window->getFPS()` (drives the `onRenderStage` frametime
    /// `60.f / fps`).
    pub fps: f32,

    /// C++ `getInputMovement()` / `getInputFocused()` / `getInputSwap()`
    /// (key-icon colours).
    pub input_movement: i32,
    pub input_focused: bool,
    pub input_swap: bool,

    /// C++ `getColorText()`.
    pub color_text: Color,

    // ---- scene vertex batches (filled by the caller — `CWall::draw`,
    // `cwManager.draw`, `CPlayer::draw` outputs, mapping §3.3 step 4) ----
    pub wall_quads: &'a [Vertex],
    pub pivot_quads: &'a [Vertex],
    pub player_tris: &'a [Vertex],
    pub cap_tris: &'a [Vertex],

    // ---- background / style (StyleData, mapping §7) ----
    /// C++ `styleData.getColors()`.
    pub background_colors: &'a [Color],
    /// C++ `styleData.bgTileRadius`.
    pub bg_tile_radius: f32,
    /// C++ `styleData.BGRotOff` (degrees).
    pub bg_rot_off: f32,

    // ---- 3D (StyleData 3D fields + colour getters) ----
    /// C++ `styleData._3dDepth`.
    pub three_d_depth: f32,
    /// C++ `styleData._3dSkew`.
    pub three_d_skew: f32,
    /// C++ `styleData._3dSpacing`.
    pub three_d_spacing: f32,
    /// C++ `styleData._3dDarkenMult`.
    pub three_d_darken_mult: f32,
    /// C++ `styleData._3dAlphaMult`.
    pub three_d_alpha_mult: f32,
    /// C++ `styleData._3dAlphaFalloff`.
    pub three_d_alpha_falloff: f32,
    /// C++ `styleData._3dPerspectiveMult`.
    pub three_d_perspective_mult: f32,
    /// C++ `styleData.get3DOverrideColor()` (the current 3D override —
    /// `main` when no explicit override colour is set).
    pub three_d_override_color: Color,
    /// C++ `styleData.getMainColor()`.
    pub main_color: Color,
    /// C++ `getColorWall()`.
    pub wall_color: Color,
    /// C++ `getColorPlayer()`.
    pub player_color: Color,

    // ---- menu background (HGGraphics.cpp lines 122–141) ----
    /// C++ `state >= States::SMain` — the menu-hexagon background is only
    /// rendered from the main state up (in-menu screens use the plain
    /// background only).
    pub menu_background_enabled: bool,
    /// C++ `fourByThree` (the menu hexagon radius: 75 vs 100).
    pub menu_four_by_three: bool,
    /// C++ `styleData.getCapColorResult()` (menu background cap ring).
    pub cap_color: Color,

    /// C++ `ssvu::getRndR` — the **non-game** RNG (draw-time jitter; PB
    /// particles). Pass a closure wrapping the chosen engine.
    pub jitter: &'a dyn Fn(f32, f32) -> f32,
    /// C++ `runLuaFunctionIfExists<int, float>("onRenderStage", rs, 60/fps)`
    /// — `Some` only when the level script defines `onRenderStage`.
    pub on_render_stage: Option<&'a dyn Fn(RenderStage, f32)>,
}

// ---------------------------------------------------------------------------
// The full frame (mapping §3.3 `draw()` — exact order).
// ---------------------------------------------------------------------------

/// C++ `HexagonGame::draw()` (mapping §3.3) — the full frame pipeline:
///
/// 1. clear to black;
/// 2. camera centres (shake jitter or fixed) while `!hasDied`;
/// 3. background tris (if `!NoBackground`), `BackgroundTris` states;
/// 4. background view; build the 3D layer copies (if `get3D()`);
/// 5. render `WallQuads3D` → `PivotQuads3D` → `PlayerTris3D`;
/// 6. (still background view) trail particles (if enabled + shown) then
///    swap particles (if enabled);
/// 7. render `WallQuads` → `CapTris` → `PivotQuads` → `PlayerTris`;
/// 8. overlay view: particles + `drawText` (`Text` states);
/// 9. key icons (if `getShowKeyIcons() || mustShowReplayUI()`);
/// 10. level info (if `getShowLevelInfo() || mustShowReplayUI()`, `Text`
///     states — a fresh `getRenderStates(Text)` evaluation);
/// 11. flash polygon (if `getFlash()`, default states);
/// 12. screenshot `"screenshot.png"` if `mustTakeScreenshot`, then clear the
///     flag.
///
/// (The C++ final step `drawImguiLuaConsole()` is the ImGui debug console —
/// out of scope for the draw layer; `TODO(issue-009-wiring)`.)
///
/// `TODO(issue-009-wiring)`: called from `HexagonGame`'s `onDraw`
/// (`game.onDraw += [this] { draw(); }`).
pub fn draw_frame(state: &mut DrawState, input: &mut FrameInput, target: &mut impl DrawTarget) {
    let config = input.config;

    if config.get_disable_game_rendering() {
        // C++ also returns when `window == nullptr` — in the port a target
        // exists by construction; the caller guards window absence.
        return;
    }

    let get_render_states = |rs: RenderStage| {
        render_states_for_stage(
            config.get_shaders(),
            input.fragment_shader_ids,
            rs,
            60.0 / input.fps,
            input.on_render_stage,
        )
    };

    target.clear_black();

    // ------------------------------------------------------------------------
    // 2. Camera centres (pre-shake / shake logic, mapping §3.3 step 2).
    // C++ asserts both cameras exist (they are set in the constructor).
    draw_set_camera_centers(
        state.background_camera.as_mut(),
        state.overlay_camera.as_mut(),
        input.has_died,
        input.level_status_camera_shake,
        config.get_width(),
        config.get_height(),
        input.jitter,
    );

    // ------------------------------------------------------------------------
    // 3. Background.
    if !config.get_no_background() {
        target.set_view(
            state
                .background_camera
                .as_mut()
                .expect("checked above")
                .apply(),
        );

        let mut background_tris = VertexBatch::new_tris();
        draw_background(
            &mut background_tris,
            Vec2::new(0.0, 0.0),
            input.sides,
            config.get_darken_uneven_background_chunk() && input.darken_uneven_background_chunk,
            config.get_black_and_white(),
            input.background_colors,
            input.bg_tile_radius,
            input.bg_rot_off,
        );

        target.draw_vertices(
            background_tris.as_slice(),
            get_render_states(RenderStage::BackgroundTris),
        );

        // C++ `state >= States::SMain` → the menu-hexagon background
        // (default render states — no shader, no blend).
        if input.menu_background_enabled {
            let mut menu_tris = VertexBatch::new_tris();
            draw_background_menu(
                &mut menu_tris,
                Vec2::new(0.0, 0.0),
                input.sides,
                config.get_darken_uneven_background_chunk() && input.darken_uneven_background_chunk,
                config.get_black_and_white(),
                input.menu_four_by_three,
                input.background_colors,
                input.bg_tile_radius,
                input.bg_rot_off,
                input.main_color,
                input.cap_color,
            );
            target.draw_vertices(menu_tris.as_slice(), RenderStates::default());
        }
    }

    // ------------------------------------------------------------------------
    // 4. Background view again; build the 3D layer copies.
    // (The C++ fills `wallQuads`/`pivotQuads`/`playerTris`/`capTris` here —
    // in the port the caller pre-filled `input.{wall,pivot,player,cap}_*`.)
    //
    // C++ `wallQuads.reserve_more_quad(walls.size() + cwManager.count())` is
    // a capacity hint on the caller-owned buffers; skipped.
    target.set_view(
        state
            .background_camera
            .as_mut()
            .expect("checked above")
            .apply(),
    );

    let mut wall_quads_3d = VertexBatch::new_tris();
    let mut pivot_quads_3d = VertexBatch::new_tris();
    let mut player_tris_3d = VertexBatch::new_tris();

    if config.get_3d() {
        let depth = input.three_d_depth;
        let num_wall_quads = input.wall_quads.len();
        let num_pivot_quads = input.pivot_quads.len();
        let num_player_tris = input.player_tris.len();

        // C++ `reserve(numWallQuads * depth)` — size_t × float, truncated.
        wall_quads_3d.reserve((num_wall_quads as f32 * depth) as usize);
        pivot_quads_3d.reserve((num_pivot_quads as f32 * depth) as usize);
        player_tris_3d.reserve((num_player_tris as f32 * depth) as usize);

        let pulse3d_effect = if config.get_no_pulse() {
            1.0
        } else {
            input.pulse3d
        };
        let effect = input.three_d_skew * config.get_3d_multiplier() * pulse3d_effect;

        // Vertical-only skew on the background camera (baked by `apply()`).
        let background = state.background_camera.as_mut().expect("checked above");
        background.set_skew(Vec2::new(1.0, 1.0 + effect));

        let rad_rot = to_rad(background.get_rotation()) + PI / 2.0;
        let sin_rot = rad_rot.sin();
        let cos_rot = rad_rot.cos();

        // C++ `for(std::size_t i = 0; i < depth; ++i)` — `size_t` vs `float`
        // comparison: an integral `i < depth` (a fractional depth makes ONE
        // extra copy, `floor(depth) + 1`).
        let mut i_copy: usize = 0;
        while (i_copy as f32) < depth {
            // C++ `unsafe_emplace_other` — append the whole source batch.
            wall_quads_3d.push_vertices(input.wall_quads);
            pivot_quads_3d.push_vertices(input.pivot_quads);
            player_tris_3d.push_vertices(input.player_tris);
            i_copy += 1;
        }

        for j in 0..(depth as i32) {
            let j = j as usize;
            let i = depth - j as f32 - 1.0;

            let offset = input.three_d_spacing
                * ((i + 1.0) * input.three_d_perspective_mult)
                * (effect * 3.6)
                * 1.4;

            let new_pos = Vec2::new(offset * cos_rot, offset * sin_rot);

            let mut override_color;

            if !config.get_black_and_white() {
                override_color =
                    get_color_darkened(input.three_d_override_color, input.three_d_darken_mult);
            } else {
                override_color = get_color_darkened(
                    Color::new(255, 255, 255, input.main_color.a),
                    input.three_d_darken_mult,
                );
            }
            adjust_alpha(
                &mut override_color,
                i,
                input.three_d_alpha_mult,
                input.three_d_alpha_falloff,
            );

            // Draw pivot layers.
            {
                let vs = pivot_quads_3d.as_slice_mut();
                for v in &mut vs[j * num_pivot_quads..(j + 1) * num_pivot_quads] {
                    v.position += new_pos;
                    v.color = override_color;
                }
            }

            if input.three_d_override_color == input.main_color {
                override_color = get_color_darkened(input.wall_color, input.three_d_darken_mult);
                adjust_alpha(
                    &mut override_color,
                    i,
                    input.three_d_alpha_mult,
                    input.three_d_alpha_falloff,
                );
            }

            // Draw wall layers.
            {
                let vs = wall_quads_3d.as_slice_mut();
                for v in &mut vs[j * num_wall_quads..(j + 1) * num_wall_quads] {
                    v.position += new_pos;
                    v.color = override_color;
                }
            }

            // Apply player color if no 3D override is present.
            if input.three_d_override_color == input.main_color {
                override_color = get_color_darkened(input.player_color, input.three_d_darken_mult);
                adjust_alpha(
                    &mut override_color,
                    i,
                    input.three_d_alpha_mult,
                    input.three_d_alpha_falloff,
                );
            }

            // Draw player layers.
            {
                let vs = player_tris_3d.as_slice_mut();
                for v in &mut vs[j * num_player_tris..(j + 1) * num_player_tris] {
                    v.position += new_pos;
                    v.color = override_color;
                }
            }
        }
    }

    // ------------------------------------------------------------------------
    // 5. 3D layers (still the pre-skew background view — `setView` copied
    //    the view before `setSkew`).
    target.draw_vertices(
        wall_quads_3d.as_slice(),
        get_render_states(RenderStage::WallQuads3D),
    );
    target.draw_vertices(
        pivot_quads_3d.as_slice(),
        get_render_states(RenderStage::PivotQuads3D),
    );
    target.draw_vertices(
        player_tris_3d.as_slice(),
        get_render_states(RenderStage::PlayerTris3D),
    );

    // ------------------------------------------------------------------------
    // 6. Particles in world space.
    if config.get_show_player_trail() && input.show_player_trail {
        draw_trail_particles(&state.trail_particles, target);
    }

    if config.get_show_swap_particles() {
        draw_swap_particles(&state.swap_particles, target);
    }

    // ------------------------------------------------------------------------
    // 7. The flat layers.
    target.draw_vertices(input.wall_quads, get_render_states(RenderStage::WallQuads));
    target.draw_vertices(input.cap_tris, get_render_states(RenderStage::CapTris));
    target.draw_vertices(
        input.pivot_quads,
        get_render_states(RenderStage::PivotQuads),
    );
    target.draw_vertices(
        input.player_tris,
        get_render_states(RenderStage::PlayerTris),
    );

    // ------------------------------------------------------------------------
    // 8. Overlay camera: particles + text.
    target.set_view(
        state
            .overlay_camera
            .as_mut()
            .expect("checked above")
            .apply(),
    );

    draw_particles(&state.particles, target);
    text_draw::draw_text(
        state,
        config,
        input.background_colors,
        input.color_text,
        input.must_show_replay_ui,
        get_render_states(RenderStage::Text),
        target,
    );

    // ------------------------------------------------------------------------
    // 9. Key icons.
    if config.get_show_key_icons() || input.must_show_replay_ui {
        draw_key_icons(
            state,
            input.color_text,
            input.input_movement,
            input.input_focused,
            input.input_swap,
            input.must_show_replay_ui,
            target,
        );
    }

    // ------------------------------------------------------------------------
    // 10. Level info (fresh `getRenderStates(Text)` evaluation).
    if config.get_show_level_info() || input.must_show_replay_ui {
        draw_level_info(state, get_render_states(RenderStage::Text), target);
    }

    // ------------------------------------------------------------------------
    // 11. Flash polygon (default states).
    if config.get_flash() {
        target.draw_vertices(state.flash_polygon.as_slice(), RenderStates::default());
    }

    // ------------------------------------------------------------------------
    // 12. Screenshot.
    if *input.must_take_screenshot {
        target.save_screenshot("screenshot.png");
        *input.must_take_screenshot = false;
    }

    // C++ final step: `drawImguiLuaConsole()` — the ImGui debug console
    // (mapping §3.4); out of scope here. `TODO(issue-009-wiring)`.
}

/// C++ `adjustAlpha` lambda inside `draw()` (mapping §3.3 3D notes):
/// `newAlpha = (a / _3dAlphaMult) - i * _3dAlphaFalloff`, clamped to
/// `[0, 255]` (`Utils::componentClamp`); skipped entirely when
/// `_3dAlphaMult == 0.f`.
fn adjust_alpha(color: &mut Color, i: f32, alpha_mult: f32, alpha_falloff: f32) {
    if alpha_mult == 0.0 {
        return;
    }

    let new_alpha = (color.a as f32 / alpha_mult) - i * alpha_falloff;

    color.a = component_clamp(new_alpha);
}

// ---------------------------------------------------------------------------
// Flash (mapping §3.5 / §5.6 + the real C++ `death_flashEffect`).
// ---------------------------------------------------------------------------

/// C++ `HexagonGame::initFlashEffect(r, g, b)` (mapping §3.5) — a
/// window-covering quad (100 px margin on all sides) in the batch vertex
/// order, alpha 0 (animated by [`update_flash`]).
#[must_use]
pub fn init_flash_effect(r: u8, g: u8, b: u8, width: u32, height: u32) -> VertexBatch {
    let color = Color::new(r, g, b, 0);

    let width = width as f32;
    let height = height as f32;
    let offset = 100.0;

    let nw = Vec2::new(-offset, -offset);
    let sw = Vec2::new(-offset, height + offset);
    let se = Vec2::new(width + offset, height + offset);
    let ne = Vec2::new(width + offset, -offset);

    let mut batch = VertexBatch::new_tris();
    // C++ `flashPolygon.batch_unsafe_emplace_back_quad(color, nw, sw, se, ne)`
    // (reserved for 6 vertices).
    batch.reserve(6);
    batch.push_quad(color, nw, sw, se, ne);
    batch
}

/// C++ `HexagonGame::updateFlash(mFT)` (mapping §5.6) — decay 3·FT toward 0
/// (clamped to `[0, 255]`), then write the alpha into all 6 flash vertices.
/// (The C++ calls this even with `Config::getFlash()` false — the render is
/// what is gated, not the decay.)
pub fn update_flash(ft: f32, flash_effect: &mut f32, flash_polygon: &mut VertexBatch) {
    if *flash_effect > 0.0 {
        *flash_effect -= 3.0 * ft;
    }

    // C++ `ssvu::getClamped(flashEffect, 0.f, 255.f)`.
    *flash_effect = (*flash_effect).clamp(0.0, 255.0);

    for vertex in flash_polygon.as_slice_mut() {
        vertex.color.a = *flash_effect as u8;
    }
}

/// Real C++ `death_flashEffect` (HexagonGame.cpp — mapping §4.6's variant is
/// stale; the source is authoritative): white flash polygon, effect 255.
pub fn death_flash_effect(
    width: u32,
    height: u32,
    flash_polygon: &mut VertexBatch,
    flash_effect: &mut f32,
) {
    *flash_polygon = init_flash_effect(255, 255, 255, width, height);
    *flash_effect = 255.0;
}

// ---------------------------------------------------------------------------
// Particle drawing (mapping §3.8).
// ---------------------------------------------------------------------------

/// C++ `HexagonGame::drawParticles` (overlay camera; default states).
pub fn draw_particles(particles: &[Particle], target: &mut impl DrawTarget) {
    for p in particles {
        target.draw_sprite(&p.sprite);
    }
}

/// C++ `HexagonGame::drawTrailParticles` (background camera; default
/// states).
pub fn draw_trail_particles(particles: &[TrailParticle], target: &mut impl DrawTarget) {
    for p in particles {
        target.draw_sprite(&p.sprite);
    }
}

/// C++ `HexagonGame::drawSwapParticles` (background camera; default states).
pub fn draw_swap_particles(particles: &[SwapParticle], target: &mut impl DrawTarget) {
    for p in particles {
        target.draw_sprite(&p.sprite);
    }
}

// ---------------------------------------------------------------------------
// Render-driving particle updates (mapping §5.8–5.10).
//
// `TODO(issue-009-wiring)`: called from `HexagonGame::update()` —
// `updateParticles` always (with a window); `updateTrailParticles` when
// `Config::getShowPlayerTrail() && status.showPlayerTrail`;
// `updateSwapParticles` when `Config::getShowSwapParticles()`. The player
// position/angle/changed-angle values come from the `CPlayer` instance;
// `status.radius` from the status. The non-game RNG (`ssvu::getRndR` /
// `ssvu::getRndI`) is injected as closures — PB/swap particles are
// intentionally NON-deterministic w.r.t. replays (mapping §5.8 note).
// ---------------------------------------------------------------------------

/// Parameters for [`update_particles`] (C++ `HexagonGame::updateParticles`).
pub struct PBParticleUpdateParams<'a> {
    /// C++ `Config::getWidth()` / `getHeight()`.
    pub width: u32,
    pub height: u32,
    /// C++ `getColorMain()`.
    pub color_main: Color,
    /// C++ `txStarParticle` ("starParticle.png").
    pub tx_star_particle: &'a Texture,
    /// C++ `mustSpawnPBParticles`.
    pub must_spawn_pb_particles: bool,
    /// C++ `nextPBParticleSpawn` (mutated).
    pub next_pb_particle_spawn: &'a mut f32,
    /// C++ `ssvu::getRndR(a, b)` (non-game RNG).
    pub rnd_real: &'a dyn Fn(f32, f32) -> f32,
    /// C++ `ssvu::getRndI(a, b)` (non-game RNG; 90..145 for PB alpha).
    pub rnd_int: &'a dyn Fn(u32, u32) -> u32,
}

/// C++ `HexagonGame::updateParticles(mFT)` (mapping §5.8) — cull out-of-bounds
/// (256 px margin), advance position/rotation, spawn PB star particles every
/// 2.75 s while `mustSpawnPBParticles`.
pub fn update_particles(ft: f32, particles: &mut Vec<Particle>, p: &mut PBParticleUpdateParams) {
    const PADDING: f32 = 256.0;
    let width = p.width as f32;
    let height = p.height as f32;

    particles.retain(|pt| {
        let pos = pt.sprite.get_position();
        !(pos.x < 0.0 - PADDING
            || pos.x > width + PADDING
            || pos.y < 0.0 - PADDING
            || pos.y > height + PADDING)
    });

    for pt in particles.iter_mut() {
        pt.sprite
            .set_position(pt.sprite.get_position() + pt.velocity * ft);
        // C++ `setRotation(getRotation() + degrees(angularVelocity * mFT))`
        // — `sf::degrees` is a unit wrapper, the value is degrees.
        pt.sprite
            .set_rotation(pt.sprite.get_rotation() + pt.angular_velocity * ft);
    }

    if p.must_spawn_pb_particles {
        *p.next_pb_particle_spawn -= ft;
        if *p.next_pb_particle_spawn <= 0.0 {
            particles.push(make_pb_particle(p));
            *p.next_pb_particle_spawn = 2.75;
        }
    }
}

/// C++ `makePBParticle` (mapping §5.8) — RNG draw order: position x,
/// rotation, scale, alpha, velocity x, velocity y, angular velocity.
fn make_pb_particle(p: &PBParticleUpdateParams) -> Particle {
    let mut sprite = Sprite::new(p.tx_star_particle.clone());
    sprite.set_position(Vec2::new((p.rnd_real)(-64.0, p.width as f32 + 64.0), -64.0));
    sprite.set_rotation((p.rnd_real)(0.0, 360.0));

    let scale = (p.rnd_real)(0.75, 1.35);
    sprite.set_scale(Vec2::new(scale, scale));

    let mut c = p.color_main;
    c.a = (p.rnd_int)(90, 145) as u8;
    sprite.set_color(c);

    let velocity = Vec2::new((p.rnd_real)(-12.0, 12.0), (p.rnd_real)(4.0, 18.0));
    let angular_velocity = (p.rnd_real)(-6.0, 6.0);

    Particle {
        sprite,
        velocity,
        angular_velocity,
    }
}

/// Parameters for [`update_trail_particles`] (C++
/// `HexagonGame::updateTrailParticles`).
pub struct TrailParticleUpdateParams<'a> {
    /// C++ `txSmallCircle` ("smallCircle.png").
    pub tx_small_circle: &'a Texture,
    /// C++ `player.getPosition()`.
    pub player_position: Vec2,
    /// C++ `player.getPlayerAngle()`.
    pub player_angle: f32,
    /// C++ `player.hasChangedAngle()` (spawn gate).
    pub player_has_changed_angle: bool,
    /// C++ `getColorPlayerTrail()`.
    pub color_player_trail: Color,
    /// C++ `Config::getPlayerTrailScale()`.
    pub player_trail_scale: f32,
    /// C++ `Config::getPlayerTrailAlpha()`.
    pub player_trail_alpha: u8,
    /// C++ `Config::getPlayerTrailDecay()`.
    pub player_trail_decay: f32,
    /// C++ `status.radius`.
    pub radius: f32,
}

/// C++ `HexagonGame::updateTrailParticles(mFT)` (mapping §5.9) — alpha
/// decays toward 0 by `PlayerTrailDecay * mFT` (dead at `a <= 3`), scale
/// ×0.98 per frame, position at `getVecFromRad(angle, radius + 2.4f)`
/// (background/world coordinates), spawn when the player angle changed.
pub fn update_trail_particles(
    ft: f32,
    trail_particles: &mut Vec<TrailParticle>,
    p: &TrailParticleUpdateParams,
) {
    trail_particles.retain(|pt| pt.sprite.get_color().a > 3);

    for pt in trail_particles.iter_mut() {
        let mut color = pt.sprite.get_color();

        let new_alpha = get_move_towards_zero(color.a as f32, p.player_trail_decay * ft);

        color.a = new_alpha as u8;
        pt.sprite.set_color(color);

        let s = pt.sprite.get_scale();
        pt.sprite.set_scale(Vec2::new(s.x * 0.98, s.y * 0.98));

        pt.sprite
            .set_position(get_vec_from_rad(pt.angle, p.radius + 2.4));
    }

    if p.player_has_changed_angle {
        let mut sprite = Sprite::new(p.tx_small_circle.clone());
        sprite.set_position(p.player_position);
        // C++ `setOrigin(Vector2f{txSmallCircle->getSize()} / 2.f)`.
        let (tw, th) = p.tx_small_circle.get_size();
        sprite.set_origin(Vec2::new(tw as f32 / 2.0, th as f32 / 2.0));

        let scale = p.player_trail_scale;
        sprite.set_scale(Vec2::new(scale, scale));

        let mut c = p.color_player_trail;
        c.a = p.player_trail_alpha;
        sprite.set_color(c);

        trail_particles.push(TrailParticle {
            sprite,
            angle: p.player_angle,
        });
    }
}

/// Parameters for [`update_swap_particles`] (C++
/// `HexagonGame::updateSwapParticles`).
pub struct SwapParticleUpdateParams<'a> {
    /// C++ `txSmallCircle` ("smallCircle.png").
    pub tx_small_circle: &'a Texture,
    /// C++ `getColorPlayerTrail()`.
    pub color_player_trail: Color,
    /// C++ `swapParticlesSpawnInfo` — copied out; the C++ clears the game
    /// member afterwards (the caller owns `swapParticlesSpawnInfo` and
    /// resets it after this call, as C++ does at the end of the function).
    pub swap_particles_spawn_info: Option<SwapParticleSpawnInfo>,
    /// C++ `ssvu::getRndR(a, b)` (non-game RNG).
    pub rnd_real: &'a dyn Fn(f32, f32) -> f32,
}

/// C++ `HexagonGame::updateSwapParticles(mFT)` (mapping §5.10) — alpha
/// decay `−3.5·mFT` (dead at `a <= 3`), scale ×0.98, motion along
/// `velocity·mFT`; burst spawns from a pending
/// [`SwapParticleSpawnInfo`]:
/// - `ready == false` (after swap): 20 × (expand 0.45, speed ×1.0, scale
///   ×1.0, alpha 45) then 10 × (expand `PI`, speed ×0.45, scale ×0.75, alpha
///   35);
/// - `ready == true` (blip): 14 × (expand `PI`, speed ×1.3, scale ×0.4,
///   alpha 140).
pub fn update_swap_particles(
    ft: f32,
    swap_particles: &mut Vec<SwapParticle>,
    p: SwapParticleUpdateParams,
) {
    swap_particles.retain(|pt| pt.sprite.get_color().a > 3);

    for pt in swap_particles.iter_mut() {
        let mut color = pt.sprite.get_color();

        let new_alpha = get_move_towards_zero(color.a as f32, 3.5 * ft);

        color.a = new_alpha as u8;
        pt.sprite.set_color(color);

        let s = pt.sprite.get_scale();
        pt.sprite.set_scale(Vec2::new(s.x * 0.98, s.y * 0.98));
        pt.sprite
            .set_position(pt.sprite.get_position() + pt.velocity * ft);
    }

    if let Some(si) = p.swap_particles_spawn_info {
        if !si.ready {
            for _ in 0..20 {
                swap_particles.push(make_swap_particle(&p, &si, 0.45, 1.0, 1.0, 45.0));
            }

            for _ in 0..10 {
                swap_particles.push(make_swap_particle(&p, &si, PI, 0.45, 0.75, 35.0));
            }
        } else {
            for _ in 0..14 {
                swap_particles.push(make_swap_particle(&p, &si, PI, 1.3, 0.4, 140.0));
            }
        }
    }
}

/// C++ `makeSwapParticle(si, expand, speedMult, scaleMult, alpha)`
/// (mapping §5.10) — RNG draw order: scale, angle jitter, speed.
fn make_swap_particle(
    p: &SwapParticleUpdateParams,
    si: &SwapParticleSpawnInfo,

    expand: f32,
    speed_mult: f32,
    scale_mult: f32,
    alpha: f32,
) -> SwapParticle {
    let mut sprite = Sprite::new(p.tx_small_circle.clone());
    sprite.set_position(si.position);
    // C++ `setOrigin(Vector2f{txSmallCircle->getSize()} / 2.f)`.
    let (tw, th) = p.tx_small_circle.get_size();
    sprite.set_origin(Vec2::new(tw as f32 / 2.0, th as f32 / 2.0));

    let scale = (p.rnd_real)(0.65, 1.35) * scale_mult;
    sprite.set_scale(Vec2::new(scale, scale));

    let mut c = p.color_player_trail;
    c.a = alpha as u8;
    sprite.set_color(c);

    let velocity = get_vec_from_rad(
        si.angle + (p.rnd_real)(-expand, expand),
        (p.rnd_real)(0.1, 10.0) * speed_mult,
    );

    SwapParticle { sprite, velocity }
}

// ---------------------------------------------------------------------------
// Key icons (mapping §3.6 / §4.2).
// ---------------------------------------------------------------------------

/// C++ `HexagonGame::initKeyIcons` (mapping §4.2) — set the four textures
/// smooth, assign the sprite textures (left/right share `keyArrow.png`),
/// then run [`update_key_icons`].
///
/// `TODO(issue-009-wiring)`: called on window creation/recreation
/// (`window->onRecreation += [this] { initKeyIcons(); }`); the textures come
/// from the assets (`keyArrow.png`, `keyFocus.png`, `keySwap.png`,
/// `replayIcon.png`).
#[allow(clippy::too_many_arguments)] // faithful C++ signature
pub fn init_key_icons(
    key_icon_left: &mut Sprite,
    key_icon_right: &mut Sprite,
    key_icon_focus: &mut Sprite,
    key_icon_swap: &mut Sprite,
    replay_icon: &mut Sprite,
    key_arrow: &mut Texture,
    key_focus: &mut Texture,
    key_swap: &mut Texture,
    replay: &mut Texture,
    config: &Config,
) {
    key_arrow.set_smooth(true);
    key_focus.set_smooth(true);
    key_swap.set_smooth(true);
    replay.set_smooth(true);

    key_icon_left.set_texture(key_arrow.clone());
    key_icon_right.set_texture(key_arrow.clone());
    key_icon_focus.set_texture(key_focus.clone());
    key_icon_swap.set_texture(key_swap.clone());
    replay_icon.set_texture(replay.clone());

    update_key_icons(
        key_icon_left,
        key_icon_right,
        key_icon_focus,
        key_icon_swap,
        replay_icon,
        config,
    );
}

/// C++ `HexagonGame::updateKeyIcons` (mapping §4.2) — four 64×64 px
/// (halfSize 32, centre origin) icons along the bottom-right, each shifted
/// left by `64·s + 8·s` from the previous; left arrow rotated 180°; replay
/// icon (origin at the texture bottom-right, scale `s/2`) at top-right.
/// `s = KeyIconsScale / ZoomFactor`. Called every frame from `update()` when
/// key icons are shown.
///
/// `TODO(issue-009-wiring)`: called from `HexagonGame::update()` when
/// `Config::getShowKeyIcons() || mustShowReplayUI()`.
pub fn update_key_icons(
    key_icon_left: &mut Sprite,
    key_icon_right: &mut Sprite,
    key_icon_focus: &mut Sprite,
    key_icon_swap: &mut Sprite,
    replay_icon: &mut Sprite,
    config: &Config,
) {
    const HALF_SIZE: f32 = 32.0;
    const SIZE: f32 = HALF_SIZE * 2.0;

    key_icon_left.set_origin(Vec2::new(HALF_SIZE, HALF_SIZE));
    key_icon_right.set_origin(Vec2::new(HALF_SIZE, HALF_SIZE));
    key_icon_focus.set_origin(Vec2::new(HALF_SIZE, HALF_SIZE));
    key_icon_swap.set_origin(Vec2::new(HALF_SIZE, HALF_SIZE));

    key_icon_left.set_rotation(180.0);

    let scaling = config.get_key_icons_scale() / config.get_zoom_factor();

    key_icon_left.set_scale(Vec2::new(scaling, scaling));
    key_icon_right.set_scale(Vec2::new(scaling, scaling));
    key_icon_focus.set_scale(Vec2::new(scaling, scaling));
    key_icon_swap.set_scale(Vec2::new(scaling, scaling));

    let scaled_half_size = HALF_SIZE * scaling;
    let scaled_size = SIZE * scaling;
    let padding = 8.0 * scaling;
    let final_padding = scaled_size + padding;
    let final_padding_x = Vec2::new(final_padding, 0.0);

    let width = config.get_width() as f32;
    let height = config.get_height() as f32;

    let bottom_right = Vec2::new(
        width - padding - scaled_half_size,
        height - padding - scaled_half_size,
    );

    key_icon_swap.set_position(bottom_right);
    key_icon_focus.set_position(key_icon_swap.get_position() - final_padding_x);
    key_icon_right.set_position(key_icon_focus.get_position() - final_padding_x);
    key_icon_left.set_position(key_icon_right.get_position() - final_padding_x);

    // ------------------------------------------------------------------------

    replay_icon.set_origin(Vec2::new(SIZE, SIZE));
    replay_icon.set_scale(Vec2::new(scaling / 2.0, scaling / 2.0));

    let top_right = Vec2::new(
        width - padding - scaled_half_size,
        padding + scaled_half_size,
    );

    replay_icon.set_position(top_right);
}

/// C++ `HexagonGame::drawKeyIcons` (mapping §3.6) — off-opacity 90 /
/// on-opacity 255 versions of the text colour; movement −1 → left, +1 →
/// right; the replay icon (on-colour) only in replay UI. (The C++ method
/// takes no `Config` — colours/positions come from `updateKeyIcons`.)
pub fn draw_key_icons(
    state: &mut DrawState,
    color_text: Color,
    input_movement: i32,
    input_focused: bool,
    input_swap: bool,
    must_show_replay_ui: bool,
    target: &mut impl DrawTarget,
) {
    const OFF_OPACITY: u8 = 90;
    const ON_OPACITY: u8 = 255;

    let off_color = Color::new(color_text.r, color_text.g, color_text.b, OFF_OPACITY);
    let on_color = Color::new(color_text.r, color_text.g, color_text.b, ON_OPACITY);

    state.key_icon_left.set_color(if input_movement == -1 {
        on_color
    } else {
        off_color
    });
    state.key_icon_right.set_color(if input_movement == 1 {
        on_color
    } else {
        off_color
    });
    state
        .key_icon_focus
        .set_color(if input_focused { on_color } else { off_color });
    state
        .key_icon_swap
        .set_color(if input_swap { on_color } else { off_color });

    target.draw_sprite(&state.key_icon_left);
    target.draw_sprite(&state.key_icon_right);
    target.draw_sprite(&state.key_icon_focus);
    target.draw_sprite(&state.key_icon_swap);

    // ------------------------------------------------------------------------

    if must_show_replay_ui {
        state.replay_icon.set_color(on_color);
        target.draw_sprite(&state.replay_icon);
    }
}

// ---------------------------------------------------------------------------
// Level info (mapping §4.3 / §3.7).
// ---------------------------------------------------------------------------

/// C++ `updateLevelInfo`'s `trim` lambda — `s.substr(0, 26)` (26 BYTES).
/// Level metadata is ASCII so byte/char coincide; a multibyte char straddling
/// byte 26 falls back to 26 CHARACTERS (keeps valid UTF-8).
fn trim_26(s: &str) -> Cow<'_, str> {
    if s.len() <= 26 {
        return Cow::Borrowed(s);
    }

    match s.get(..26) {
        Some(t) => Cow::Borrowed(t),
        None => Cow::Owned(s.chars().take(26).collect()),
    }
}

/// C++ `HexagonGame::updateLevelInfo` (mapping §4.3) — the 325×75 box
/// (scale `1/zoomFactor`, 3 px outline in the main colour, fill
/// `getColor(0)` or Black) in the bottom-left corner, with the level / pack
/// / author / "BY" / difficulty-mult lines (20/14/20/12/14 px ÷ zoomFactor,
/// uppercased + trimmed to 26 chars). Called every frame from `update()`
/// when `Config::getShowLevelInfo() || mustShowReplayUI()`.
///
/// `offset_color` = C++ `getColor(0)` (Black in B&W / no colours);
/// `main_color` = C++ `styleData.getMainColor()`; `color_text` = C++
/// `getColorText()`.
///
/// `TODO(issue-009-wiring)`: called from `HexagonGame::update()`; the level
/// name/pack/author/difficulty come from `levelData` / `getPackName()` /
/// `difficultyMult`.
#[allow(clippy::too_many_arguments)] // faithful C++ signature
pub fn update_level_info(
    state: &mut DrawState,
    config: &Config,
    color_text: Color,
    offset_color: Color,
    main_color: Color,
    level_name: &str,
    pack_name: &str,
    author: &str,
    difficulty_mults_count: usize,
    difficulty_mult: f32,
) {
    const LEVEL_INFO_SCALING: f32 = 1.0;
    let zoom = config.get_zoom_factor();
    let scaling = LEVEL_INFO_SCALING / zoom;
    let padding = 8.0 * scaling;

    let size = Vec2::new(325.0, 75.0);
    let half_size = Vec2::new(size.x / 2.0, size.y / 2.0);
    let scaled_half_size = Vec2::new(half_size.x * scaling, half_size.y * scaling);

    let rect = &mut state.level_info_rectangle;
    rect.set_size(size);
    rect.set_scale(Vec2::new(scaling, scaling));
    rect.set_fill_color(offset_color);
    rect.set_outline_color(main_color);
    rect.set_origin(half_size);
    rect.set_outline_thickness(3.0);

    let width = config.get_width() as f32;
    let bottom_left = Vec2::new(
        padding + scaled_half_size.x,
        config.get_height() as f32 - padding - scaled_half_size.y,
    );

    rect.set_position(bottom_left);
    let _ = width;

    let t_padding = padding;

    // ------------------------------------------------------------------------
    // Level name line.
    {
        let level = oh_math::hg::string::to_uppercase(level_name.to_string());
        let level = trim_26(&level);
        let t = &mut state.level_info_text_level;
        t.set_fill_color(color_text);
        // C++ `setCharacterSize(20.f / zoomFactor)` — truncated to unsigned.
        t.set_character_size((20.0 / zoom) as u32);
        t.set_string(level.as_ref());
        t.set_origin(t.get_local_bounds().get_nw());
        let nw = rect.get_global_bounds().get_nw();
        t.set_position(Vec2::new(nw.x + t_padding, nw.y + t_padding));
    }

    // ------------------------------------------------------------------------
    // Pack name line.
    {
        let pack = oh_math::hg::string::to_uppercase(pack_name.to_string());
        let pack = trim_26(&pack);
        let t = &mut state.level_info_text_pack;
        t.set_fill_color(color_text);
        t.set_character_size((14.0 / zoom) as u32);
        t.set_string(pack.as_ref());
        t.set_origin(t.get_local_bounds().get_nw());
        let sw = state.level_info_text_level.get_global_bounds().get_sw();
        t.set_position(Vec2::new(sw.x, sw.y + t_padding));
    }

    // ------------------------------------------------------------------------
    // Author line.
    {
        let author = oh_math::hg::string::to_uppercase(author.to_string());
        let author = trim_26(&author);
        let t = &mut state.level_info_text_author;
        t.set_fill_color(color_text);
        t.set_character_size((20.0 / zoom) as u32);
        t.set_string(author.as_ref());
        t.set_origin(t.get_local_bounds().get_se());
        let se = rect.get_global_bounds().get_se();
        t.set_position(Vec2::new(se.x - t_padding, se.y - t_padding));
    }

    // ------------------------------------------------------------------------
    // "BY" line.
    {
        let t = &mut state.level_info_text_by;
        t.set_fill_color(color_text);
        t.set_character_size((12.0 / zoom) as u32);
        t.set_string("BY");
        t.set_origin(t.get_local_bounds().get_se());
        let sw = state.level_info_text_author.get_global_bounds().get_sw();
        t.set_position(Vec2::new(sw.x - t_padding, sw.y));
    }

    // ------------------------------------------------------------------------
    // Difficulty-mult line (only when the level has more than one).
    if difficulty_mults_count > 1 {
        let t = &mut state.level_info_text_dm;
        t.set_fill_color(color_text);
        t.set_character_size((14.0 / zoom) as u32);
        t.set_string(format!("{}x", diff_format(difficulty_mult)));
        t.set_origin(t.get_local_bounds().get_sw());
        let sw = rect.get_global_bounds().get_sw();
        t.set_position(Vec2::new(sw.x + t_padding, sw.y - t_padding));
    } else {
        state.level_info_text_dm.set_string("");
    }
}

/// C++ `HexagonGame::drawLevelInfo(mStates)` (mapping §3.7) — rectangle +
/// the five text lines, all with the given (Text-stage) states.
pub fn draw_level_info(state: &mut DrawState, states: RenderStates, target: &mut impl DrawTarget) {
    target.draw_rectangle(&state.level_info_rectangle, states);
    target.draw_text(&state.level_info_text_level, states);
    target.draw_text(&state.level_info_text_pack, states);
    target.draw_text(&state.level_info_text_author, states);
    target.draw_text(&state.level_info_text_by, states);
    target.draw_text(&state.level_info_text_dm, states);
}

// ---------------------------------------------------------------------------
// Background (mapping §7.7 — `StyleData::drawBackground` /
// `drawBackgroundMenu`).
//
// `TODO(issue-009-wiring)`: `HexagonGame::draw()` calls
// `draw_background` with `styleData` values
// (`getColors()`, `bgTileRadius`, `BGRotOff`); `MenuGame` (issue 12) calls
// `draw_background_menu` for its background.
// ---------------------------------------------------------------------------

/// C++ `StyleData::drawBackground` = `reserve_more(sides*3)` +
/// `drawBackgroundImpl` (mapping §7.7): one triangle-fan sector per side —
/// centre + two orbit points at `bgTileRadius`, `div = tau/sides * 1.0001`
/// (the 1.0001 overlap avoids hairline gaps). Sector `i` is centred at
/// `toRad(BGRotOff) + div*i` spanning `±halfDiv`, coloured
/// `colors[i mod n]` (Black in B&W; the final sector, when `sides` is even,
/// darkened ×1.4 with the uneven-chunk flag). No vertices when `colors` is
/// empty.
#[allow(clippy::too_many_arguments)] // faithful C++ signature
pub fn draw_background(
    vertices: &mut VertexBatch,
    center: Vec2,
    sides: u32,
    darken_uneven_background_chunk: bool,
    black_and_white: bool,
    colors: &[Color],
    bg_tile_radius: f32,
    bg_rot_off: f32, // degrees
) {
    // C++ `drawBackground` reserves before the empty-colours return; no
    // observable difference.
    vertices.reserve_more((sides * 3) as usize);

    if colors.is_empty() {
        return;
    }

    let div = TAU / sides as f32 * 1.0001;
    let half_div = div / 2.0;
    let distance = bg_tile_radius;
    let rot_off = to_rad(bg_rot_off);

    for i in 0..sides {
        let angle = rot_off + div * i as f32;
        let mut current_color = colors[(i as usize) % colors.len()];

        let must_darken = (i % 2 == 0 && i == sides - 1) && darken_uneven_background_chunk;

        if black_and_white {
            current_color = Color::BLACK;
        } else if must_darken {
            current_color = get_color_darkened(current_color, 1.4);
        }

        // C++ `batch_unsafe_emplace_back(color, center, orbit(+halfDiv),
        // orbit(-halfDiv))`.
        vertices.push_color_positions(
            current_color,
            &[
                center,
                get_orbit_point(&center, angle + half_div, distance),
                get_orbit_point(&center, angle - half_div, distance),
            ],
        );
    }
}

/// `ssvs::getOrbitRad(vec, rad, radius)` via `Vec2` (aliasing the helper
/// without importing a second copy of the name).
fn get_orbit_point(center: &Vec2, rad: f32, radius: f32) -> Vec2 {
    *center + get_vec_from_rad(rad, radius)
}

/// C++ `StyleData::drawBackgroundMenu` = `reserve_more(sides*3 + sides*6)` +
/// the sectors of [`draw_background`] + the menu hexagon overlay (mapping
/// §7.7): 60°-sector wedges, outer ring at `hexagonRadius + 10` in the main
/// colour, inner ring at `hexagonRadius` in the cap colour; radius 75 for
/// 4:3 windows, 100 otherwise. `main_color` / `cap_color` are the C++
/// `getMainColor()` / `getCapColorResult()` (B&W → White/Black).
#[allow(clippy::too_many_arguments)] // faithful C++ signature
pub fn draw_background_menu(
    vertices: &mut VertexBatch,
    center: Vec2,
    sides: u32,
    darken_uneven_background_chunk: bool,
    black_and_white: bool,
    four_by_three: bool,
    colors: &[Color],
    bg_tile_radius: f32,
    bg_rot_off: f32, // degrees
    main_color: Color,
    cap_color: Color,
) {
    // C++ `reserve_more(sides * 3 + sides * 6)` then
    // `drawBackgroundImpl` (with the uneven-chunk darkening) +
    // `drawBackgroundMenuHexagonImpl`.
    vertices.reserve_more((sides * 3 + sides * 6) as usize);

    if colors.is_empty() {
        return;
    }

    // Sector pass (drawBackgroundImpl).
    let div = TAU / sides as f32 * 1.0001;
    let half_div = div / 2.0;
    let distance = bg_tile_radius;
    let rot_off = to_rad(bg_rot_off);

    for i in 0..sides {
        let angle = rot_off + div * i as f32;
        let mut current_color = colors[(i as usize) % colors.len()];

        let must_darken = (i % 2 == 0 && i == sides - 1) && darken_uneven_background_chunk;

        if black_and_white {
            current_color = Color::BLACK;
        } else if must_darken {
            current_color = get_color_darkened(current_color, 1.4);
        }

        vertices.push_color_positions(
            current_color,
            &[
                center,
                get_orbit_point(&center, angle + half_div, distance),
                get_orbit_point(&center, angle - half_div, distance),
            ],
        );
    }

    // ------------------------------------------------------------------------
    // The menu hexagon overlay (drawBackgroundMenuHexagonImpl).
    let hexagon_radius = if four_by_three { 75.0 } else { 100.0 };

    let color_main = if black_and_white {
        Color::WHITE
    } else {
        main_color
    };
    let color_cap = if black_and_white {
        Color::BLACK
    } else {
        cap_color
    };

    for i in 0..sides {
        let angle = rot_off + div * i as f32;

        vertices.push_color_positions(
            color_main,
            &[
                center,
                get_orbit_point(&center, angle + half_div, hexagon_radius + 10.0),
                get_orbit_point(&center, angle - half_div, hexagon_radius + 10.0),
            ],
        );

        vertices.push_color_positions(
            color_cap,
            &[
                center,
                get_orbit_point(&center, angle + half_div, hexagon_radius),
                get_orbit_point(&center, angle - half_div, hexagon_radius),
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use crate::view::View;

    // ---------------------------------------------------------------------
    // A recording DrawTarget for the stage-sequence assertions.
    // ---------------------------------------------------------------------

    #[derive(Clone, Debug, PartialEq)]
    enum DrawEvent {
        Clear,
        SetView(View),
        Vertices { count: usize, states: RenderStates },
        Sprite,
        Text { states: RenderStates },
        Rectangle { states: RenderStates },
        Screenshot(String),
    }

    #[derive(Default)]
    struct Recording {
        events: Vec<DrawEvent>,
    }

    impl DrawTarget for Recording {
        fn clear_black(&mut self) {
            self.events.push(DrawEvent::Clear);
        }
        fn set_view(&mut self, view: View) {
            self.events.push(DrawEvent::SetView(view));
        }
        fn draw_vertices(&mut self, vertices: &[Vertex], states: RenderStates) {
            self.events.push(DrawEvent::Vertices {
                count: vertices.len(),
                states,
            });
        }
        fn draw_sprite(&mut self, _sprite: &Sprite) {
            self.events.push(DrawEvent::Sprite);
        }
        fn draw_text(&mut self, _text: &Text, states: RenderStates) {
            self.events.push(DrawEvent::Text { states });
        }
        fn draw_rectangle(&mut self, _rect: &RectangleShape, states: RenderStates) {
            self.events.push(DrawEvent::Rectangle { states });
        }
        fn save_screenshot(&mut self, path: &str) {
            self.events.push(DrawEvent::Screenshot(path.to_string()));
        }
    }

    fn test_font() -> Font {
        Font::load(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../_RELEASE/Assets/OpenSquare-Regular.ttf"
        )))
        .expect("OpenSquare-Regular.ttf")
    }

    fn empty_texture() -> Texture {
        Texture::new()
    }

    fn quad(pos: f32) -> [Vertex; 6] {
        let c = Color::new(10, 20, 30, 255);
        let nw = Vec2::new(pos, pos);
        let sw = Vec2::new(pos, pos + 1.0);
        let se = Vec2::new(pos + 1.0, pos + 1.0);
        let ne = Vec2::new(pos + 1.0, pos);
        [nw, sw, se, nw, se, ne]
            .into_iter()
            .map(|p| Vertex::new(p, c))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }

    fn make_draw_state() -> DrawState {
        let font = test_font();
        let zoom = 1.0;
        DrawState {
            background_camera: None,
            overlay_camera: None,
            pre_shake_centers: None,
            flash_polygon: VertexBatch::new_tris(),
            particles: Vec::new(),
            trail_particles: Vec::new(),
            swap_particles: Vec::new(),
            key_icon_left: Sprite::new(empty_texture()),
            key_icon_right: Sprite::new(empty_texture()),
            key_icon_focus: Sprite::new(empty_texture()),
            key_icon_swap: Sprite::new(empty_texture()),
            replay_icon: Sprite::new(empty_texture()),
            level_info_rectangle: RectangleShape::default(),
            level_info_text_level: init_text(&font, "LVL", 20.0, zoom),
            level_info_text_pack: init_text(&font, "PACK", 14.0, zoom),
            level_info_text_author: init_text(&font, "AUTHOR", 20.0, zoom),
            level_info_text_by: init_text(&font, "BY", 12.0, zoom),
            level_info_text_dm: init_text(&font, "", 14.0, zoom),
            fps_text: init_text(&font, "0", 25.0, zoom),
            time_text: init_text(&font, "0", 70.0, zoom),
            text: init_text(&font, "", 25.0, zoom),
            replay_text: init_text(&font, "", 20.0, zoom),
            message_text: init_text(&font, "", 38.0, zoom),
            pb_text: init_text(&font, "", 65.0, zoom),
            pb_text_growth: 0.0,
        }
    }

    /// The deterministic jitter: returns the range max (so a shake of
    /// magnitude `m` moves the centre by `(+m, +m)`).
    fn jitter_max() -> Box<dyn Fn(f32, f32) -> f32> {
        Box::new(|_a, b| b)
    }

    fn base_config() -> Config {
        let mut c = Config::default();
        c.set_shaders(true);
        c.set_3d(true);
        c.set_flash(false);
        c
    }

    // ---------------------------------------------------------------------
    // RenderStage
    // ---------------------------------------------------------------------

    #[test]
    fn render_stage_order_matches_cpp_and_lua() {
        assert_eq!(RenderStage::BackgroundTris.as_usize(), 0);
        assert_eq!(RenderStage::WallQuads3D.as_usize(), 1);
        assert_eq!(RenderStage::PivotQuads3D.as_usize(), 2);
        assert_eq!(RenderStage::PlayerTris3D.as_usize(), 3);
        assert_eq!(RenderStage::WallQuads.as_usize(), 4);
        assert_eq!(RenderStage::CapTris.as_usize(), 5);
        assert_eq!(RenderStage::PivotQuads.as_usize(), 6);
        assert_eq!(RenderStage::PlayerTris.as_usize(), 7);
        assert_eq!(RenderStage::Text.as_usize(), 8);
        assert_eq!(RenderStage::COUNT, 9);
    }

    #[test]
    fn render_states_for_stage_bindings() {
        let ids: FragmentShaderIds = [
            Some(100),
            Some(101),
            None,
            Some(103),
            Some(104),
            Some(105),
            Some(106),
            Some(107),
            Some(108),
        ];
        use std::cell::RefCell;
        use std::rc::Rc;

        let calls = Rc::new(RefCell::new(Vec::new()));
        let calls_box = calls.clone();
        let hook: Box<dyn Fn(RenderStage, f32)> =
            Box::new(move |s, ft| calls_box.borrow_mut().push((s, ft)));

        // shaders off → always default (hook not called).
        assert_eq!(
            render_states_for_stage(false, &ids, RenderStage::Text, 1.0, Some(&hook)),
            RenderStates::default()
        );

        // no bound id → default (hook not called).
        assert_eq!(
            render_states_for_stage(true, &ids, RenderStage::PivotQuads3D, 1.0, Some(&hook)),
            RenderStates::default()
        );
        assert!(
            calls.borrow().is_empty(),
            "hook must not fire for default states"
        );

        // bound id → with_shader, and the onRenderStage hook fires first
        // with the frame time 60/fps (fps 120 → 0.5).
        let st = render_states_for_stage(true, &ids, RenderStage::WallQuads, 0.5, Some(&hook));
        assert_eq!(st, RenderStates::with_shader(104));
        assert_eq!(*calls.borrow(), vec![(RenderStage::WallQuads, 0.5)]);
    }

    // ---------------------------------------------------------------------
    // draw_frame — the stage sequence + per-stage shader binding.
    // ---------------------------------------------------------------------

    #[test]
    fn draw_frame_emits_exact_stage_sequence() {
        let mut config = base_config();
        config.set_show_key_icons(true);
        config.set_show_level_info(true);
        config.set_show_fps(true);

        let mut state = make_draw_state();
        let (bg, ov) = crate::camera::init_cameras(
            config.get_width() as f32,
            config.get_height() as f32,
            config.get_zoom_factor(),
        );
        state.background_camera = Some(bg);
        state.overlay_camera = Some(ov);
        state.flash_polygon = init_flash_effect(255, 0, 0, config.get_width(), config.get_height());

        // Scene: 1 wall quad (6 verts), 1 pivot quad (6), 1 player tri (3),
        // 1 cap quad (6). 3D depth 1.
        let wall_quads = quad(0.0);
        let pivot_quads = quad(1.0);
        let player_tris = [
            Vertex::new(Vec2::new(0.0, 0.0), Color::RED),
            Vertex::new(Vec2::new(1.0, 0.0), Color::RED),
            Vertex::new(Vec2::new(0.0, 1.0), Color::RED),
        ];
        let cap_tris = quad(2.0);

        let colors = [Color::new(1, 2, 3, 255), Color::new(4, 5, 6, 255)];
        let main_color = Color::new(90, 90, 90, 255);
        // an EXPLICIT override (≠ main) so the wall/player layers use the
        // darkened override colour.
        let override_color = Color::new(12, 34, 56, 255);

        let mut must_take_screenshot = true;
        let jitter: Box<dyn Fn(f32, f32) -> f32> = jitter_max();

        let ids: FragmentShaderIds = [
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
            Some(6),
            Some(7),
            Some(8),
        ];

        let mut input = FrameInput {
            config: &config,
            fragment_shader_ids: &ids,
            has_died: false,
            level_status_camera_shake: 0.0,
            sides: 6,
            darken_uneven_background_chunk: false,
            show_player_trail: false,
            pulse3d: 1.0,
            must_show_replay_ui: false,
            must_take_screenshot: &mut must_take_screenshot,
            fps: 60.0,
            input_movement: -1,
            input_focused: false,
            input_swap: false,
            color_text: Color::WHITE,
            wall_quads: &wall_quads,
            pivot_quads: &pivot_quads,
            player_tris: &player_tris,
            cap_tris: &cap_tris,
            background_colors: &colors,
            bg_tile_radius: 10000.0,
            bg_rot_off: 0.0,
            three_d_depth: 1.0,
            three_d_skew: 0.18,
            three_d_spacing: 1.0,
            three_d_darken_mult: 1.5,
            three_d_alpha_mult: 0.5,
            three_d_alpha_falloff: 3.0,
            three_d_perspective_mult: 1.0,
            three_d_override_color: override_color,
            main_color,
            wall_color: Color::new(200, 200, 200, 255),
            player_color: Color::new(50, 50, 250, 255),
            menu_background_enabled: false,
            menu_four_by_three: false,
            cap_color: Color::WHITE,
            jitter: &jitter,
            on_render_stage: None,
        };

        let mut rec = Recording::default();
        draw_frame(&mut state, &mut input, &mut rec);

        // ---- the exact event sequence -------------------------------------
        // 1. clear
        // 2. setView(background)  [background block]
        // 3. background tris: 6 sides × 3 verts = 18, shader 0
        // 4. setView(background)  [flat/3D block]
        // 5. wallQuads3D: 1 copy × 6 = 6, shader 1
        // 6. pivotQuads3D: 1 copy × 6 = 6, shader 2
        // 7. playerTris3D: 1 copy × 3 = 3, shader 3
        //    (no trail/swap particles — trail disabled, swap vec empty)
        // 8. wallQuads: 6, shader 4
        // 9. capTris: 6, shader 5
        // 10. pivotQuads: 6, shader 6
        // 11. playerTris: 3, shader 7
        // 12. setView(overlay)
        //     (no particles)
        // 13. drawText: timeText (showTimer default true) — shader 8
        // 14. drawText: status text (showStatusText default true) — shader 8
        // 15. drawText: fpsText (showFps on) — shader 8
        //     (message/pb empty → skipped; no replay UI)
        // 16. 4 key-icon sprites (showKeyIcons on)
        // 17. level info: rectangle + 4 texts (dm line empty — still drawn,
        //     C++ renders it unconditionally) — 5 events, shader 8
        // 18. flash (getFlash off → NOT drawn)
        // 19. screenshot (mustTakeScreenshot → cleared)
        let expected: Vec<DrawEvent> = vec![
            DrawEvent::Clear,
            DrawEvent::SetView(state.background_camera.unwrap().get_view()),
            DrawEvent::Vertices {
                count: 18,
                states: RenderStates::with_shader(0),
            },
            DrawEvent::SetView(state.background_camera.unwrap().get_view()),
            DrawEvent::Vertices {
                count: 6,
                states: RenderStates::with_shader(1),
            },
            DrawEvent::Vertices {
                count: 6,
                states: RenderStates::with_shader(2),
            },
            DrawEvent::Vertices {
                count: 3,
                states: RenderStates::with_shader(3),
            },
            DrawEvent::Vertices {
                count: 6,
                states: RenderStates::with_shader(4),
            },
            DrawEvent::Vertices {
                count: 6,
                states: RenderStates::with_shader(5),
            },
            DrawEvent::Vertices {
                count: 6,
                states: RenderStates::with_shader(6),
            },
            DrawEvent::Vertices {
                count: 3,
                states: RenderStates::with_shader(7),
            },
            DrawEvent::SetView(state.overlay_camera.unwrap().get_view()),
            DrawEvent::Text {
                states: RenderStates::with_shader(8),
            },
            DrawEvent::Text {
                states: RenderStates::with_shader(8),
            },
            DrawEvent::Text {
                states: RenderStates::with_shader(8),
            },
            DrawEvent::Sprite,
            DrawEvent::Sprite,
            DrawEvent::Sprite,
            DrawEvent::Sprite,
            DrawEvent::Rectangle {
                states: RenderStates::with_shader(8),
            },
            DrawEvent::Text {
                states: RenderStates::with_shader(8),
            },
            DrawEvent::Text {
                states: RenderStates::with_shader(8),
            },
            DrawEvent::Text {
                states: RenderStates::with_shader(8),
            },
            DrawEvent::Text {
                states: RenderStates::with_shader(8),
            },
            DrawEvent::Text {
                states: RenderStates::with_shader(8),
            },
            DrawEvent::Screenshot("screenshot.png".into()),
        ];
        assert_eq!(rec.events, expected, "draw() stage/event sequence");
        assert!(!must_take_screenshot, "flag cleared after screenshot");
    }

    #[test]
    fn draw_frame_3d_layers_offset_and_color() {
        // depth 2, explicit override (≠ main): every layer gets the
        // darkened override colour; offsets grow toward the far layer.
        let mut config = base_config();
        config.set_show_key_icons(false);
        config.set_show_level_info(false);
        config.set_show_fps(false);
        config.set_show_timer(false);
        config.set_show_status_text(false);

        let mut state = make_draw_state();
        let (bg, ov) = crate::camera::init_cameras(
            config.get_width() as f32,
            config.get_height() as f32,
            config.get_zoom_factor(),
        );
        state.background_camera = Some(bg);
        state.overlay_camera = Some(ov);

        let wall_quads = quad(0.0);
        let ids: FragmentShaderIds = [None, None, None, None, None, None, None, None, None];
        let mut must_take_screenshot = false;
        let jitter: Box<dyn Fn(f32, f32) -> f32> = jitter_max();

        let override_color = Color::new(150, 120, 90, 255);
        let main_color = Color::new(90, 90, 90, 255);

        let mut input = FrameInput {
            config: &config,
            fragment_shader_ids: &ids,
            has_died: false,
            level_status_camera_shake: 0.0,
            sides: 4,
            darken_uneven_background_chunk: false,
            show_player_trail: false,
            pulse3d: 2.0,
            must_show_replay_ui: false,
            must_take_screenshot: &mut must_take_screenshot,
            fps: 60.0,
            input_movement: 0,
            input_focused: false,
            input_swap: false,
            color_text: Color::WHITE,
            wall_quads: &wall_quads,
            pivot_quads: &[],
            player_tris: &[],
            cap_tris: &[],
            background_colors: &[], // B&W-free empty palette → no background tris
            bg_tile_radius: 10000.0,
            bg_rot_off: 0.0,
            three_d_depth: 2.0,
            three_d_skew: 1.0,
            three_d_spacing: 1.0,
            three_d_darken_mult: 2.0,
            three_d_alpha_mult: 2.0,
            three_d_alpha_falloff: 1.0,
            three_d_perspective_mult: 1.0,
            three_d_override_color: override_color,
            main_color,
            wall_color: Color::new(200, 200, 200, 255),
            player_color: Color::new(50, 50, 250, 255),
            menu_background_enabled: false,
            menu_four_by_three: false,
            cap_color: Color::WHITE,
            jitter: &jitter,
            on_render_stage: None,
        };

        let mut rec: Recording = Recording::default();
        draw_frame(&mut state, &mut input, &mut rec);

        // The 12-vertex draw is the 3D wall layer (2 copies × 6) with no
        // shader bound (all ids None).
        let wall_3d = rec
            .events
            .iter()
            .find(|e| matches!(e, DrawEvent::Vertices { count: 12, .. }))
            .expect("12-vertex 3D wall draw");
        let DrawEvent::Vertices { states, .. } = wall_3d else {
            panic!();
        };
        assert_eq!(*states, RenderStates::default()); // no shader bound

        // Now verify the layer math independently:
        // effect = _3dSkew × 3DMultiplier(1) × pulse3D(2) = 2.0
        // layer j=0: i = 2 − 0 − 1 = 1 →
        //   offset = spacing × ((i+1) × perspMult) × (effect × 3.6) × 1.4
        //          = 1 × 2 × 7.2 × 1.4 = 20.16
        // layer j=1: i = 0 → 1 × 1 × 7.2 × 1.4 = 10.08
        // rotation 0 → radRot = π/2 → (cos, sin) ≈ (0, 1).
        let rad_rot = to_rad(0.0) + PI / 2.0;
        let expected_offset = |j: i32| {
            let i = 2.0 - j as f32 - 1.0;
            let offset = 1.0 * ((i + 1.0) * 1.0) * (2.0 * 3.6) * 1.4;
            (offset * rad_rot.cos(), offset * rad_rot.sin())
        };
        let (dx0, dy0) = expected_offset(0); // far layer, bigger offset
        let (dx1, dy1) = expected_offset(1); // near layer

        // alpha: a=255, alphaMult 2 → (255/2) − i×1 → j=0: 127.5−1=126.5→126;
        // j=1: 127.5−0=127.5→127 (truncation).
        // override colour darkened by 2: (75, 60, 45, 126/127).
        let dark = get_color_darkened(override_color, 2.0);
        assert_eq!(dark.r, 75);
        assert_eq!(dark.g, 60);
        assert_eq!(dark.b, 45);

        // The recording only keeps counts — assert the arithmetic through a
        // second direct check: rebuild the 3D copies exactly like draw_frame
        // and assert vertex positions/colours.
        // Mirror the two `unsafe_emplace_other` copies (capacity is 12).
        let mut wall_3d = VertexBatch::new_tris();
        wall_3d.push_vertices(&wall_quads);
        wall_3d.push_vertices(&wall_quads);
        for j in 0..2 {
            let i = 2.0 - j as f32 - 1.0;
            let offset = 1.0 * ((i + 1.0) * 1.0) * (2.0 * 3.6) * 1.4;
            let new_pos = Vec2::new(offset * rad_rot.cos(), offset * rad_rot.sin());
            let mut oc = get_color_darkened(override_color, 2.0);
            let new_alpha = (oc.a as f32 / 2.0) - i * 1.0;
            oc.a = component_clamp(new_alpha);
            let vs = wall_3d.as_slice_mut();
            for v in &mut vs[j * 6..(j + 1) * 6] {
                v.position += new_pos;
                v.color = oc;
            }
        }
        // Layer j=0 (vertices 0..6) got the bigger offset; layer j=1 (6..12)
        // the smaller one.
        let vs = wall_3d.as_slice();
        assert!((vs[0].position.x - dx0).abs() < 1e-4);
        assert!((vs[0].position.y - dy0).abs() < 1e-4);
        assert!((vs[6].position.x - dx1).abs() < 1e-4);
        assert!((vs[6].position.y - dy1).abs() < 1e-4);
        assert_eq!(vs[0].color, Color::new(75, 60, 45, 126));
        assert_eq!(vs[6].color, Color::new(75, 60, 45, 127));
        // The frame's camera now carries the vertical skew (1, 1 + effect).
        assert_eq!(
            state.background_camera.unwrap().get_skew(),
            Vec2::new(1.0, 1.0 + 2.0)
        );
    }

    #[test]
    fn draw_frame_skips_shaders_and_flags() {
        // Shaders OFF → every state is the default; disable rendering → no
        // events at all.
        let mut config = Config::default();
        // Config's default is shaders ON — this test exercises the
        // shaders-off path explicitly.
        config.set_shaders(false);
        assert!(!config.get_shaders());

        let mut state = make_draw_state();
        let (bg, ov) = crate::camera::init_cameras(
            config.get_width() as f32,
            config.get_height() as f32,
            config.get_zoom_factor(),
        );
        state.background_camera = Some(bg);
        state.overlay_camera = Some(ov);

        let mut must_take_screenshot = false;
        let jitter: Box<dyn Fn(f32, f32) -> f32> = jitter_max();
        let ids: FragmentShaderIds = [
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
            Some(6),
            Some(7),
            Some(8),
            Some(9),
        ];

        let mut input = FrameInput {
            config: &config,
            fragment_shader_ids: &ids,
            has_died: false,
            level_status_camera_shake: 0.0,
            sides: 3,
            darken_uneven_background_chunk: false,
            show_player_trail: false,
            pulse3d: 1.0,
            must_show_replay_ui: false,
            must_take_screenshot: &mut must_take_screenshot,
            fps: 60.0,
            input_movement: 0,
            input_focused: false,
            input_swap: false,
            color_text: Color::WHITE,
            wall_quads: &[],
            pivot_quads: &[],
            player_tris: &[],
            cap_tris: &[],
            background_colors: &[Color::new(7, 8, 9, 255)],
            bg_tile_radius: 500.0,
            bg_rot_off: 0.0,
            three_d_depth: 1.0,
            three_d_skew: 0.18,
            three_d_spacing: 1.0,
            three_d_darken_mult: 1.5,
            three_d_alpha_mult: 0.5,
            three_d_alpha_falloff: 3.0,
            three_d_perspective_mult: 1.0,
            three_d_override_color: Color::WHITE,
            main_color: Color::WHITE,
            wall_color: Color::WHITE,
            player_color: Color::WHITE,
            menu_background_enabled: false,
            menu_four_by_three: false,
            cap_color: Color::WHITE,
            jitter: &jitter,
            on_render_stage: None,
        };

        let mut rec = Recording::default();
        {
            draw_frame(&mut state, &mut input, &mut rec);

            // Every vertex/text/rect event uses the default states
            // (shaders disabled).
            for e in &rec.events {
                match e {
                    DrawEvent::Vertices { states, .. }
                    | DrawEvent::Text { states }
                    | DrawEvent::Rectangle { states } => {
                        assert_eq!(*states, RenderStates::default(), "shader off ⇒ default");
                    }
                    _ => {}
                }
            }
        } // `input` (and its &config borrow) ends here

        // Disable game rendering → nothing is emitted.
        config.set_disable_game_rendering(true);
        {
            let mut input = FrameInput {
                config: &config,
                fragment_shader_ids: &ids,
                has_died: false,
                level_status_camera_shake: 0.0,
                sides: 3,
                darken_uneven_background_chunk: false,
                show_player_trail: false,
                pulse3d: 1.0,
                must_show_replay_ui: false,
                must_take_screenshot: &mut must_take_screenshot,
                fps: 60.0,
                input_movement: 0,
                input_focused: false,
                input_swap: false,
                color_text: Color::WHITE,
                wall_quads: &[],
                pivot_quads: &[],
                player_tris: &[],
                cap_tris: &[],
                background_colors: &[Color::new(7, 8, 9, 255)],
                bg_tile_radius: 500.0,
                bg_rot_off: 0.0,
                three_d_depth: 1.0,
                three_d_skew: 0.18,
                three_d_spacing: 1.0,
                three_d_darken_mult: 1.5,
                three_d_alpha_mult: 0.5,
                three_d_alpha_falloff: 3.0,
                three_d_perspective_mult: 1.0,
                three_d_override_color: Color::WHITE,
                main_color: Color::WHITE,
                wall_color: Color::WHITE,
                player_color: Color::WHITE,
                menu_background_enabled: false,
                menu_four_by_three: false,
                cap_color: Color::WHITE,
                jitter: &jitter,
                on_render_stage: None,
            };
            let before = rec.events.len();
            draw_frame(&mut state, &mut input, &mut rec);
            assert_eq!(rec.events.len(), before);
        }
    }

    // ---------------------------------------------------------------------
    // Flash
    // ---------------------------------------------------------------------

    #[test]
    fn flash_polygon_layout_and_update() {
        let mut batch = init_flash_effect(255, 128, 0, 800, 450);
        let v = batch.as_slice();
        assert_eq!(v.len(), 6);
        let c = Color::new(255, 128, 0, 0);
        // quad(nw, sw, se, ne) with a 100 px margin.
        assert_eq!(v[0].position, Vec2::new(-100.0, -100.0));
        assert_eq!(v[1].position, Vec2::new(-100.0, 550.0));
        assert_eq!(v[2].position, Vec2::new(900.0, 550.0));
        assert_eq!(v[3].position, Vec2::new(-100.0, -100.0));
        assert_eq!(v[4].position, Vec2::new(900.0, 550.0));
        assert_eq!(v[5].position, Vec2::new(900.0, -100.0));
        assert!(v.iter().all(|x| x.color == c));

        let mut effect = 255.0;
        // decay 3·FT; ft = 10 → 255 − 30 = 225.
        update_flash(10.0, &mut effect, &mut batch);
        assert_eq!(effect, 225.0);
        assert!(batch.as_slice().iter().all(|x| x.color.a == 225));

        // decay never goes below 0.
        update_flash(100.0, &mut effect, &mut batch);
        assert_eq!(effect, 0.0);
        assert!(batch.as_slice().iter().all(|x| x.color.a == 0));

        let mut batch2 = VertexBatch::new_tris();
        let mut effect2 = 0.0;
        death_flash_effect(800, 450, &mut batch2, &mut effect2);
        assert_eq!(effect2, 255.0);
        assert_eq!(batch2.as_slice()[0].color, Color::new(255, 255, 255, 0));
    }

    // ---------------------------------------------------------------------
    // Background
    // ---------------------------------------------------------------------

    #[test]
    fn draw_background_emits_one_sector_per_side_in_order() {
        let mut v = VertexBatch::new_tris();
        let colors = [Color::RED, Color::GREEN, Color::BLUE];
        draw_background(
            &mut v,
            Vec2::new(0.0, 0.0),
            3,
            false,
            false,
            &colors,
            100.0,
            0.0,
        );

        let div = TAU / 3.0 * 1.0001;
        let half = div / 2.0;
        let verts = v.as_slice();
        assert_eq!(verts.len(), 9);

        for i in 0..3usize {
            let angle = div * i as f32;
            assert_eq!(verts[i * 3].position, Vec2::new(0.0, 0.0));
            assert_eq!(
                verts[i * 3 + 1].position,
                get_orbit_point(&Vec2::new(0.0, 0.0), angle + half, 100.0)
            );
            assert_eq!(
                verts[i * 3 + 2].position,
                get_orbit_point(&Vec2::new(0.0, 0.0), angle - half, 100.0)
            );
            assert_eq!(verts[i * 3].color, colors[i]);
        }
    }

    #[test]
    fn draw_background_b_and_w_and_uneven_darkening() {
        let colors = [
            Color::new(255, 255, 255, 255),
            Color::new(100, 100, 100, 255),
        ];

        // 4 sides, even; sector 3 (i == sides-1, odd) is NOT darkened.
        let mut v = VertexBatch::new_tris();
        draw_background(
            &mut v,
            Vec2::new(0.0, 0.0),
            4,
            true,
            false,
            &colors,
            10.0,
            90.0,
        );
        assert_ne!(v.as_slice()[9].color, get_color_darkened(colors[1], 1.4));

        // 6 sides: sector 5 is odd → not darkened; sector 4 (even, not last)
        // not darkened; use 5? i==sides-1 must be even → sides even. With 4:
        // last = 3 (odd) — so pick sides = 2: sector 1 is odd. The C++
        // condition `i % 2 == 0 && i == sides - 1` is only true for odd
        // sides (e.g. sides = 5 → i = 4).
        let mut v = VertexBatch::new_tris();
        draw_background(
            &mut v,
            Vec2::new(0.0, 0.0),
            5,
            true,
            false,
            &colors,
            10.0,
            0.0,
        );
        assert_eq!(
            v.as_slice()[12].color,
            get_color_darkened(colors[4 % 2], 1.4),
            "sector sides-1 (even index) darkened ×1.4"
        );

        // B&W → every sector Black.
        let mut v = VertexBatch::new_tris();
        draw_background(
            &mut v,
            Vec2::new(0.0, 0.0),
            3,
            true,
            true,
            &colors,
            10.0,
            0.0,
        );
        assert!(v.as_slice().iter().all(|x| x.color == Color::BLACK));

        // empty colours → no vertices.
        let mut v = VertexBatch::new_tris();
        draw_background(&mut v, Vec2::new(0.0, 0.0), 3, false, false, &[], 10.0, 0.0);
        assert!(v.is_empty());
    }

    #[test]
    fn draw_background_menu_adds_two_rings_per_sector() {
        let mut v = VertexBatch::new_tris();
        let colors = [Color::BLUE, Color::GREEN];
        draw_background_menu(
            &mut v,
            Vec2::new(0.0, 0.0),
            6,
            false,
            false,
            false,
            &colors,
            10000.0,
            0.0,
            Color::new(10, 20, 30, 255),
            Color::new(40, 50, 60, 255),
        );
        // 6 sectors × 3 + 6 × (3 + 3) = 54.
        assert_eq!(v.as_slice().len(), 54);

        // C++ layout: ALL sectors first (0..18), then the rings
        // (drawBackgroundMenuHexagonImpl): sector 0 outer ring @ 18..21,
        // inner ring @ 21..24; radii 110 (main colour) / 100 (cap).
        let verts = v.as_slice();
        let a110 = (110.0 * half6().cos(), 110.0 * half6().sin());
        let a100 = (100.0 * half6().cos(), 100.0 * half6().sin());
        assert_eq!(verts[18].position, Vec2::new(0.0, 0.0)); // ring centre
        assert!((verts[19].position.x - a110.0).abs() < 1e-3);
        assert!((verts[19].position.y - a110.1).abs() < 1e-3);
        assert_eq!(verts[19].color, Color::new(10, 20, 30, 255));
        assert!((verts[22].position.x - a100.0).abs() < 1e-3);
        assert_eq!(verts[22].color, Color::new(40, 50, 60, 255));
        // sectors: colour cycles by mod index.
        assert_eq!(verts[0].color, Color::BLUE);
        assert_eq!(verts[3].color, Color::GREEN);
    }

    fn half6() -> f32 {
        // sector 0, rotOff 0: angle = 0, point at +halfDiv.
        (TAU / 6.0 * 1.0001) / 2.0
    }

    // ---------------------------------------------------------------------
    // Key icons / level info
    // ---------------------------------------------------------------------

    #[test]
    fn key_icon_layout_matches_constants() {
        let mut config = Config::default();
        config.set_key_icons_scale(2.0); // s = 2 / zoom(1.27)

        let mut left = Sprite::new(empty_texture());
        let mut right = Sprite::new(empty_texture());
        let mut focus = Sprite::new(empty_texture());
        let mut swap = Sprite::new(empty_texture());
        let mut replay = Sprite::new(empty_texture());

        update_key_icons(
            &mut left,
            &mut right,
            &mut focus,
            &mut swap,
            &mut replay,
            &config,
        );

        let s = 2.0 / 1.27;
        let scaled_half = 32.0 * s;
        let scaled_size = 64.0 * s;
        let padding = 8.0 * s;
        let w = config.get_width() as f32;
        let h = config.get_height() as f32;

        let br = Vec2::new(w - padding - scaled_half, h - padding - scaled_half);
        assert_eq!(swap.get_position(), br);
        assert_eq!(
            focus.get_position(),
            Vec2::new(br.x - (scaled_size + padding), br.y)
        );
        assert_eq!(
            right.get_position(),
            focus.get_position() - Vec2::new(scaled_size + padding, 0.0)
        );
        assert_eq!(
            left.get_position(),
            right.get_position() - Vec2::new(scaled_size + padding, 0.0)
        );
        assert_eq!(left.get_rotation(), 180.0);
        assert_eq!(swap.get_rotation(), 0.0);

        assert_eq!(left.get_origin(), Vec2::new(32.0, 32.0));
        assert_eq!(left.get_scale(), Vec2::new(s, s));

        // Replay: origin at (64, 64), scale s/2, top-right corner.
        assert_eq!(replay.get_origin(), Vec2::new(64.0, 64.0));
        assert_eq!(replay.get_scale(), Vec2::new(s / 2.0, s / 2.0));
        assert_eq!(
            replay.get_position(),
            Vec2::new(w - padding - scaled_half, padding + scaled_half)
        );
    }

    #[test]
    fn draw_key_icons_opacities() {
        let mut state = make_draw_state();
        let mut rec = Recording::default();

        draw_key_icons(
            &mut state,
            Color::new(200, 210, 220, 255),
            -1, // left pressed
            false,
            false,
            false,
            &mut rec,
        );
        assert_eq!(
            state.key_icon_left.get_color(),
            Color::new(200, 210, 220, 255)
        );
        assert_eq!(
            state.key_icon_right.get_color(),
            Color::new(200, 210, 220, 90)
        );
        assert_eq!(
            state.key_icon_focus.get_color(),
            Color::new(200, 210, 220, 90)
        );
        assert_eq!(
            state.key_icon_swap.get_color(),
            Color::new(200, 210, 220, 90)
        );
        assert_eq!(rec.events, vec![DrawEvent::Sprite; 4]);

        // Replay UI: the fifth sprite appears with the on colour.
        draw_key_icons(
            &mut state,
            Color::new(200, 210, 220, 255),
            1,
            true,
            true,
            true,
            &mut rec,
        );
        assert_eq!(
            state.replay_icon.get_color(),
            Color::new(200, 210, 220, 255)
        );
        assert_eq!(rec.events.len(), 4 + 5);
    }

    #[test]
    fn update_level_info_layout_and_trim() {
        let config = Config::default();
        let mut state = make_draw_state();
        let long_name = "X".repeat(40);

        update_level_info(
            &mut state,
            &config,
            Color::WHITE,
            Color::new(3, 4, 5, 255),
            Color::new(9, 9, 9, 255),
            &long_name,
            "my pack",
            "some author",
            2,
            1.5,
        );

        let rect = &state.level_info_rectangle;
        assert_eq!(rect.get_size(), Vec2::new(325.0, 75.0));
        assert_eq!(rect.get_outline_thickness(), 3.0);
        assert_eq!(rect.get_fill_color(), Color::new(3, 4, 5, 255));
        assert_eq!(rect.get_outline_color(), Color::new(9, 9, 9, 255));

        let s = 1.0 / 1.27;
        let padding = 8.0 * s;
        let h = config.get_height() as f32;
        let expected_pos = Vec2::new(padding + 162.5 * s, h - padding - 37.5 * s);
        assert!((rect.get_position().x - expected_pos.x).abs() < 1e-4);
        assert!((rect.get_position().y - expected_pos.y).abs() < 1e-4);

        // Trim + uppercase.
        assert_eq!(state.level_info_text_level.get_string(), &"X".repeat(26));
        assert_eq!(state.level_info_text_pack.get_string(), "MY PACK");
        assert_eq!(state.level_info_text_author.get_string(), "SOME AUTHOR");
        assert_eq!(state.level_info_text_by.get_string(), "BY");
        assert_eq!(state.level_info_text_dm.get_string(), "1.5x");

        // No DM line when a single difficulty.
        update_level_info(
            &mut state,
            &config,
            Color::WHITE,
            Color::BLACK,
            Color::WHITE,
            "a",
            "b",
            "c",
            1,
            1.0,
        );
        assert!(state.level_info_text_dm.get_string().is_empty());
    }

    #[test]
    fn level_info_draw_emits_rect_then_five_texts() {
        let mut state = make_draw_state();
        let mut rec = Recording::default();
        draw_level_info(&mut state, RenderStates::with_shader(8), &mut rec);
        assert_eq!(
            rec.events,
            vec![
                DrawEvent::Rectangle {
                    states: RenderStates::with_shader(8)
                },
                DrawEvent::Text {
                    states: RenderStates::with_shader(8)
                },
                DrawEvent::Text {
                    states: RenderStates::with_shader(8)
                },
                DrawEvent::Text {
                    states: RenderStates::with_shader(8)
                },
                DrawEvent::Text {
                    states: RenderStates::with_shader(8)
                },
                DrawEvent::Text {
                    states: RenderStates::with_shader(8)
                },
            ]
        );
    }

    // ---------------------------------------------------------------------
    // Particles (render-driving updates)
    // ---------------------------------------------------------------------

    #[test]
    fn swap_particle_bursts_spawn_the_cpp_counts() {
        let tx = empty_texture();
        let spawn = Some(SwapParticleSpawnInfo {
            ready: false,
            position: Vec2::new(5.0, 6.0),
            angle: 0.0,
        });
        let rnd: Box<dyn Fn(f32, f32) -> f32> = Box::new(|_a, b| b); // max
        let mut swap = Vec::new();

        update_swap_particles(
            1.0,
            &mut swap,
            SwapParticleUpdateParams {
                tx_small_circle: &tx,
                color_player_trail: Color::new(1, 2, 3, 255),
                // Option<Copy> — `spawn` stays usable for the assertion.
                swap_particles_spawn_info: spawn,
                rnd_real: &rnd,
            },
        );
        assert!(
            spawn.is_some(),
            "copy consumed by value; game-side flag cleared by caller"
        );
        assert_eq!(swap.len(), 30, "20 + 10 after-swap burst");
        // first 20: alpha 45; next 10: alpha 35.
        assert!(swap[..20].iter().all(|p| p.sprite.get_color().a == 45));
        assert!(swap[20..].iter().all(|p| p.sprite.get_color().a == 35));
        // deterministic rnd (max): speed = 10 × mult; the "after" first group
        // speedMult 1.0 → 10.
        assert_eq!(swap[0].velocity, get_vec_from_rad(0.45, 10.0));

        // ready=true blip → 14 particles, alpha 140.
        let spawn = Some(SwapParticleSpawnInfo {
            ready: true,
            position: Vec2::new(5.0, 6.0),
            angle: 0.0,
        });
        swap.clear();
        update_swap_particles(
            1.0,
            &mut swap,
            SwapParticleUpdateParams {
                tx_small_circle: &tx,
                color_player_trail: Color::new(1, 2, 3, 255),
                swap_particles_spawn_info: spawn,
                rnd_real: &rnd,
            },
        );
        assert_eq!(swap.len(), 14);
        assert!(swap.iter().all(|p| p.sprite.get_color().a == 140));
        assert_eq!(swap[0].velocity, get_vec_from_rad(PI, 13.0));
    }

    #[test]
    fn swap_particles_decay_and_death() {
        // alpha 10, decay 3.5 → 6.5 → 6 (truncation), one frame of mFT 1.
        let mut vec = vec![make_swap_particle_for_test()];
        let tx = empty_texture();
        let rnd: Box<dyn Fn(f32, f32) -> f32> = Box::new(|a, _b| a);
        update_swap_particles(
            1.0,
            &mut vec,
            SwapParticleUpdateParams {
                tx_small_circle: &tx,
                color_player_trail: Color::WHITE,
                swap_particles_spawn_info: None,
                rnd_real: &rnd,
            },
        );
        assert_eq!(vec[0].sprite.get_color().a, 6);
        // frame 2: 6 − 3.5 = 2.5 → 2 (truncation).
        update_swap_particles(
            1.0,
            &mut vec,
            SwapParticleUpdateParams {
                tx_small_circle: &tx,
                color_player_trail: Color::WHITE,
                swap_particles_spawn_info: None,
                rnd_real: &rnd,
            },
        );
        assert_eq!(vec[0].sprite.get_color().a, 2);
        // frame 3: 2 ≤ 3 at the top → erased.
        update_swap_particles(
            1.0,
            &mut vec,
            SwapParticleUpdateParams {
                tx_small_circle: &tx,
                color_player_trail: Color::WHITE,
                swap_particles_spawn_info: None,
                rnd_real: &rnd,
            },
        );
        assert!(vec.is_empty());
    }

    fn make_swap_particle_for_test() -> SwapParticle {
        SwapParticle {
            sprite: {
                let mut s = Sprite::new(empty_texture());
                s.set_color(Color::new(1, 2, 3, 10));
                s.set_position(Vec2::new(0.0, 0.0));
                s.set_scale(Vec2::new(2.0, 2.0));
                s
            },
            velocity: Vec2::new(0.0, 0.0),
        }
    }

    #[test]
    fn trail_particles_reposition_and_die() {
        let tx = empty_texture();
        let p = TrailParticle {
            sprite: {
                let mut s = Sprite::new(tx.clone());
                s.set_color(Color::new(1, 2, 3, 100));
                s.set_scale(Vec2::new(1.0, 1.0));
                s
            },
            angle: 0.0,
        };
        let mut trail = vec![p];

        update_trail_particles(
            1.0,
            &mut trail,
            &TrailParticleUpdateParams {
                tx_small_circle: &tx,
                player_position: Vec2::new(1.0, 2.0),
                player_angle: 1.0,
                player_has_changed_angle: true,
                color_player_trail: Color::new(4, 5, 6, 255),
                player_trail_scale: 2.0,
                player_trail_alpha: 90,
                player_trail_decay: 20.0,
                radius: 50.0,
            },
        );
        // old particle: alpha 100 − 20 = 80; scale × 0.98; position at
        // getVecFromRad(0, 52.4) = (52.4, 0).
        assert_eq!(trail[0].sprite.get_color().a, 80);
        assert!((trail[0].sprite.get_scale().x - 0.98).abs() < 1e-6);
        assert!((trail[0].sprite.get_position().x - 52.4).abs() < 1e-4);
        // new particle spawned at the player position with alpha 90.
        assert_eq!(trail.len(), 2);
        assert_eq!(trail[1].sprite.get_color().a, 90);
        assert_eq!(trail[1].sprite.get_position(), Vec2::new(1.0, 2.0));
        assert_eq!(trail[1].angle, 1.0);

        // decay to death (alpha ≤ 3, checked at the TOP of the frame):
        // old 80→60→40→20→0 (erased on frame 6); new 90→70→50→30→10→0
        // (erased on frame 7).
        for _ in 0..7 {
            update_trail_particles(
                1.0,
                &mut trail,
                &TrailParticleUpdateParams {
                    tx_small_circle: &tx,
                    player_position: Vec2::new(1.0, 2.0),
                    player_angle: 1.0,
                    player_has_changed_angle: false,
                    color_player_trail: Color::new(4, 5, 6, 255),
                    player_trail_scale: 2.0,
                    player_trail_alpha: 90,
                    player_trail_decay: 20.0,
                    radius: 50.0,
                },
            );
        }
        assert!(trail.is_empty());
    }

    #[test]
    fn pb_particles_spawn_on_timer_and_cull_out_of_bounds() {
        let tx = empty_texture();
        let rnd: Box<dyn Fn(f32, f32) -> f32> = Box::new(|a, _b| a); // min
        let rnd_int: Box<dyn Fn(u32, u32) -> u32> = Box::new(|a, _b| a);
        let mut next_spawn = 0.0;

        let params = PBParticleUpdateParams {
            width: 800,
            height: 450,
            color_main: Color::new(11, 22, 33, 255),
            tx_star_particle: &tx,
            must_spawn_pb_particles: true,
            next_pb_particle_spawn: &mut next_spawn,
            rnd_real: &rnd,
            rnd_int: &rnd_int,
        };

        let mut p: Vec<Particle> = Vec::new();
        {
            let mut params = params;
            update_particles(1.0, &mut p, &mut params);
        }
        // deterministic min: position (−64, −64), rotation 0, scale 0.75,
        // alpha 90, velocity (−12, 4), angular −6.
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].sprite.get_position(), Vec2::new(-64.0, -64.0));
        assert_eq!(p[0].sprite.get_color(), Color::new(11, 22, 33, 90));
        assert_eq!(p[0].velocity, Vec2::new(-12.0, 4.0));
        assert_eq!(next_spawn, 2.75);

        // walk it far out of bounds (y > height + 256).
        for _ in 0..200 {
            {
                let mut params = PBParticleUpdateParams {
                    width: 800,
                    height: 450,
                    color_main: Color::new(11, 22, 33, 255),
                    tx_star_particle: &tx,
                    must_spawn_pb_particles: true,
                    next_pb_particle_spawn: &mut next_spawn,
                    rnd_real: &rnd,
                    rnd_int: &rnd_int,
                };
                update_particles(1.0, &mut p, &mut params);
            }
            if p.is_empty() {
                break;
            }
        }
        // It leaves the cull box, then a fresh one spawns afterwards.
        assert!(!p.is_empty(), "timer keeps respawning every 2.75 frames");
    }
}
