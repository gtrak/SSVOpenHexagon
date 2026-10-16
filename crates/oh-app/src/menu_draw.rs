//! issue 013b — the `MenuGame::draw()` render bridge (the menu's draw
//! surface).
//!
//! C++ `MenuGame::draw()` (`src/SSVOpenHexagon/Core/MenuGame.cpp:5751-5986`)
//! dispatches on `state` to the helper renders (`:3568-:6161`): the
//! hexagon-menu background (`styleData.drawBackgroundMenu`), the "CURRENT
//! PROFILE:" line, the state-specific menu panels, `drawGraphics` /
//! `drawOnlineStatus`, and the `HexagonDialogBox` pass.
//!
//! 13a ported the *update* half as `oh_menu::MenuGame` with `draw` left an
//! explicit no-op; this module ports the *draw* half. It lives in `oh-app`
//! (D-020: the composition crate is the only crate that may depend on both
//! the engine side — `oh-menu` — and the render side — `oh-render`):
//! `MenuDraw` owns every C++ `MenuGame` render member (the cameras, the
//! vertex batches, the `MenuFont` texts + cached glyph heights, the
//! sprites, the `HexagonDialogBox` text, and the draw-mutated layout state
//! the `oh_menu::Shared` holder does not carry), and `draw` executes the
//! C++ dispatch against any [`DrawTarget`] (the GL target in the windowed
//! loop, a recording target in the tests).
//!
//! ## C++-fidelity notes (read before diffing against the C++)
//!
//! **Mouse state is always the no-hover C++ path.** The C++ `draw`
//! recomputes `mousePressed` / `mouseHovering` from
//! `sf::Mouse::isButtonPressed` and runs `overlayMouseOverlap*` against
//! the overlay camera (MenuGame.cpp:5753-5756). The port has no mouse
//! input state yet (the SSVStart input manager is the documented 15c gap;
//! `oh_render::input::InputState` exists but is not wired to the menu), so
//! every `mouseOverlap*` is `false`, `mouseOverlapColor(false, c)` is the
//! identity, and the click-edge actions (`mustUseMenuItem` /
//! `mustChangeIndexTo` / `mustChangePackIndexTo` / `mustFavorite` /
//! `mustPlay`) never fire. The rendered quads/positions are the exact C++
//! no-hover layout.
//!
//! **`mustRefresh` is size-driven, not event-driven.** The C++ sets
//! `mustRefresh` on window-resize events; the port detects the same
//! condition in `refresh_if_stale` (the shared layout values — `w` / `h`
//! / aspect / char sizes — changed since the last draw) and then runs the
//! C++ `:5758-5765` block: `refresh_camera()`, `adjust_levels_offset()`,
//! `adjust_menu_offset(true)`, `resetNamesScrolls()` (plus the
//! `updateLevelSelectionDrawingParameters` resets, MenuGame.cpp:
//! 4296-4300). The first draw always refreshes (the C++ constructor ran
//! `refreshCamera()` already; the port's `MenuGame::new` ran
//! `refresh_camera()` too — the stale-detection just re-applies the same
//! idempotent setup once the cameras/texts exist).
//!
//! **`levelStatus.sides` is the default `6` and
//! `darkenUnevenBackgroundChunk` is `false`** in the background pass
//! (MenuGame.cpp:5780-5783): the menu's `LevelStatus` is the default-
//! constructed one (LevelStatus.hpp:66) and the menu Lua
//! (`onLoad`/`onUpdate` level-status hooks) is unwired (menu Lua is the
//! documented 13a gap).
//!
//! **`Config::getWidth()/getHeight()`** (the overlay-camera size, the
//! dialog `center`/`centerUpperHalf` borders, and the online-status
//! window height) map to the `MenuConfig` seam's `size_x()`/`size_y()` —
//! in the C++ these report the current resolution, which is the same
//! steady-state value the window size (`getSizeX/getSizeY`) reports; the
//! port's `Config` keeps them in lockstep (`create_window` maps the
//! resolution to the logical window size 1:1, `windowed.rs`).
//!
//! **`window.getFPS()`** is threaded through as `MenuDraw::fps` (the
//! windowed loop refreshes it every frame, C++ `main.cpp`'s
//! `window.run` loop); `getFPSMult()` is `200.f / fps`
//! (MenuGame.cpp:5985-5989, ported as `oh_menu::layout::fps_mult`).
//!
//! **`mustTakeScreenshot`** (MenuGame.cpp:5974-5978) is always `false`:
//! the screenshot key binding lives in the unwired SSVStart input
//! manager.
//!
//! **`txtDialog` char size**: the C++ `HexagonDialogBox` text is created
//! at char size 0 and `create()` leaves it at the dialog's `charSize`
//! (HexagonDialogBox.cpp:44-47); the bridge applies
//! `dialog.char_size()` before the text pass.
//!
//! **The `namesScroll` / `levelDescription` state is bridge-owned.** The
//! C++ keeps them as `MenuGame` members mutated by the draw; the port's
//! `oh_menu` update side carries documented no-ops
//! (`reset_level_names_scrolls`, menu_game.rs) for exactly this reason.
//! The bridge resets the scrolls on the C++ trigger set: leaving
//! `LevelSelection` (MenuGame.cpp:2349, :2475, :1774), the active
//! drawer's pack changing (`changePack`/`changePackQuick`, :2121, :2141),
//! the level index changing (`leftRightAction`, :1967, :2059 —
//! `resetLevelNamesScrolls`), and the refresh path (:5765). The level
//! description is recomputed when the current level changes (the C++
//! `setIndex`/`refreshCamera` call `formatLevelDescription`, :2950,
//! :3253).
//!
//! **The sprite textures are 1×1 placeholders.** The C++ textures
//! (`titleBar.png`, `creditsBar1.png`, `creditsBar2.png`,
//! `onlineIcon*.png`, `epilepsyWarning.png`) are not loaded by the port
//! yet (the `oh-assets` texture table is the 15c gap); the bridge emits
//! the full C++ layout either way, and issue 10's GL target renders
//! sprites as documented no-ops until the texture table lands. The
//! placeholder *size* (1 px) only shifts the `getGlobalBottom`-relative
//! transforms (the "CURRENT PROFILE" line, the credits bars) by the
//! real-texture-size delta.

use oh_math::hg::color::Color;
use oh_math::ssvs::vector2::Vec2;
use oh_menu::dialog_box::{DBoxDraw, DialogQuad, HexagonDialogBox};
use oh_menu::layout::{self, CharSizes};
use oh_menu::menu_game::{LevelDrawer, MenuGame};
use oh_menu::seams::ConnectionState;
use oh_menu::states::States;
use oh_menu::{Category, Menu};
use oh_render::draw::RenderStates;
use oh_render::font::{Font, Text};
use oh_render::graphics::{draw_background_menu, DrawTarget};
use oh_render::texture::{Sprite, Texture};
use oh_render::types::RectangleShape;
use oh_render::view::{Camera, View};
use oh_render::VertexBatch;

/// C++ file-level `fontHeightDifferential` (HexagonDialogBox.cpp:158).
const FONT_HEIGHT_DIFFERENTIAL: f32 = 0.9;

/// The C++ `MenuGame` render state that `draw` owns (everything except the
/// `oh_menu::Shared` / menu-system state, which `draw` reads/mutates
/// through short-lived borrows).
pub struct MenuDraw {
    // ---- C++ `window->getFPS()` (getFPSMult divisor) --------------------
    /// C++ `window->getFPS()` — the windowed loop refreshes this every
    /// frame before `draw` (default 60 = the C++'s 60 FPS baseline).
    pub fps: f32,

    // ---- C++ camera members (MenuGame.cpp:185-193) ----------------------
    /// C++ `backgroundCamera` (centre `(0,0)`; the view size is set on
    /// refresh from `Config::getSizeX/Y * getZoomFactor`, the skew from the
    /// aspect).
    background_camera: Camera,
    /// C++ `overlayCamera` (the view size is set on refresh).
    overlay_camera: Camera,

    // ---- C++ vertex-batch members (MenuGame.cpp:231-232) ----------------
    /// C++ `menuBackgroundTris`.
    menu_background_tris: VertexBatch,
    /// C++ `menuQuads`.
    menu_quads: VertexBatch,

    // ---- C++ `MenuFont` members (MenuGame.cpp:203-224) ------------------
    // The fixed-constructor-size fonts; the aspect-dependent ones are
    // re-sized on refresh from `oh_menu::layout::char_sizes`.
    txt_version: Text,
    /// (13b-c: the "PRESS ANY KEY" lines on the boot states.)
    #[allow(dead_code)] // lands with the 13b(c) boot states
    txt_prof: Text,
    txt_menu_big: Text,
    txt_selection_small: Text,
    txt_online_status: Text,
    /// C++ `HexagonDialogBox::txtDialog` (created at char size 0; the
    /// bridge applies `dialog.char_size()` before the text pass).
    txt_dialog: Text,

    // ---- C++ sprite / shape members (MenuGame.cpp:175-181) --------------
    /// C++ `titleBar` (`titleBar.png`).
    title_bar: Sprite,
    /// C++ `creditsBar1` (`creditsBar1.png`).
    credits_bar1: Sprite,
    /// C++ `creditsBar2` (`creditsBar2.png`).
    credits_bar2: Sprite,
    /// C++ `sOnline` (starts on `onlineIconFail.png`; the online-status
    /// pass re-points it to `onlineIcon.png` when connected — in the port
    /// both are the 1×1 placeholder, see the module docs).
    s_online: Sprite,
    /// C++ `epilepsyWarning` (`epilepsyWarning.png`; the (c) state draws
    /// it — the transform is set on refresh, :3157-3160).
    #[allow(dead_code)] // lands with the 13b(c) EpilepsyWarning state
    epilepsy_warning: Sprite,
    /// C++ `rsOnlineStatus` (default size `{128, 32}`).
    rs_online_status: RectangleShape,

    // ---- C++ `MenuFont::height` cache (updateHeight, :3236-3244) --------
    /// The `txtMenuBig` glyph height at the current char size (C++
    /// `txtMenuBig.height` — the main-menu quad math).
    h_menu_big: f32,

    // ---- C++ `namesScroll` (MenuGame.hpp:409) — draw-mutated ------------
    /// C++ `namesScroll[Label::ScrollsSize]` — the level-selection left
    /// panel's per-label marquee scroll (LevelName, PackName, PackAuthor,
    /// MusicName, MusicAuthor, MusicAlbum).
    /// (The (b) level-selection left panel reads it; (a) only resets it.)
    #[allow(dead_code)] // lands with the 13b(b) level selection
    names_scroll: [f32; 6],

    // ---- C++ `levelDescription` (MenuGame.hpp:408) — draw-cached --------
    /// The `formatLevelDescription` output, keyed by level id (see the
    /// module docs: the bridge recomputes when the current level changes).
    /// (The (b) level-selection left panel reads it; (a) only caches it.)
    #[allow(dead_code)] // lands with the 13b(b) level selection
    desc_key: Option<String>,
    #[allow(dead_code)] // lands with the 13b(b) level selection
    desc_lines: Vec<String>,

    // ---- stale-refresh tracking (C++ `mustRefresh`, :5758) --------------
    /// `(w, h, four_by_three, char_sizes)` at the last refresh.
    last_refresh: Option<(f32, f32, bool, CharSizes)>,

    // ---- names-scroll reset triggers (see the module docs) --------------
    last_state: States,
    /// The active drawer's `(pack_idx, level count, is_favorites)` at the
    /// last draw (the C++ `changePack` / `changePackQuick` trigger).
    last_pack_sig: u64,
    /// The active drawer's current index (the C++ `leftRightAction`
    /// `resetLevelNamesScrolls` trigger).
    last_level_index: i32,

    // ---- the fonts (kept for re-probing heights on refresh) -------------
    font: Font,
}

impl MenuDraw {
    /// Build the render state for `MenuGame::draw`. `font` is the C++
    /// `openSquare` (`OpenSquare-Regular.ttf`, MenuGame.cpp:162); the bold
    /// variant is the C++ `openSquareBold` (the bold-only fonts —
    /// `txtLoadSmall`, `txtSelectionBig`, `txtSelectionMedium`,
    /// `txtSelectionRanked` — land with the b/c states, which is why the
    /// constructor takes it now but only the regular font is used yet).
    #[must_use]
    pub fn new(font: &Font, font_bold: &Font) -> Self {
        let _ = font_bold; // (b/c states: the bold-only MenuFonts)
        let placeholder = Texture::create(1, 1);
        let char_sizes = layout::char_sizes(false); // refined on refresh
        let mut rs = RectangleShape::default();
        rs.set_size(Vec2::new(128.0, 32.0)); // C++ `rsOnlineStatus{128, 32}`
        Self {
            fps: 60.0,
            background_camera: Camera::from_view(View::new(
                Vec2::new(0.0, 0.0),
                Vec2::new(800.0, 450.0), // sized on the first refresh
            )),
            overlay_camera: Camera::from_view(View::new(
                Vec2::new(0.0, 0.0),
                Vec2::new(800.0, 450.0), // sized on the first refresh
            )),
            menu_background_tris: VertexBatch::new_tris(),
            // C++ `menuQuads` — a `FastVertexVectorTris` fed by
            // `batch_unsafe_emplace_back_quad` (6 triangle vertices per
            // quad), so the port uses the triangles batch too.
            menu_quads: VertexBatch::new_tris(),
            // C++ constructor char sizes (MenuGame.cpp:199-224):
            // `txtVersion` 40, `txtProf` 18, `txtOnlineStatus` 24;
            // `txtSelectionSmall` keeps the SFML default (30).
            txt_version: Text::new(font.clone(), String::new(), 40),
            txt_prof: Text::new(font.clone(), String::new(), 18),
            txt_menu_big: Text::new(font.clone(), String::new(), char_sizes.menu_big as u32),
            txt_selection_small: Text::new(font.clone(), String::new(), 30),
            txt_online_status: Text::new(font.clone(), String::new(), 24),
            txt_dialog: Text::new(font.clone(), String::new(), 0),
            title_bar: Sprite::new(placeholder.clone()),
            credits_bar1: Sprite::new(placeholder.clone()),
            credits_bar2: Sprite::new(placeholder.clone()),
            s_online: Sprite::new(placeholder.clone()),
            epilepsy_warning: Sprite::new(placeholder),
            rs_online_status: rs,
            h_menu_big: 0.0, // probed on the first refresh
            names_scroll: [0.0; 6],
            desc_key: None,
            desc_lines: Vec::new(),
            last_refresh: None,
            last_state: States::LoadingScreen,
            last_pack_sig: 0,
            last_level_index: 0,
            font: font.clone(),
        }
    }

    /// The C++ `mustRefresh` block (MenuGame.cpp:5758-5765) — re-apply the
    /// camera / char-size / offset setup when the layout inputs changed
    /// since the last draw (or on the very first draw).
    fn refresh_if_stale(&mut self, menu: &MenuGame) {
        let shared = menu.shared();
        let (w, h, fbt, sizes) = {
            let s = shared.borrow();
            (s.w, s.h, s.four_by_three, s.char_sizes)
        };
        let stale = match self.last_refresh {
            Some((lw, lh, lfbt, lsizes)) => lw != w || lh != h || lfbt != fbt || lsizes != sizes,
            None => true,
        };
        if !stale {
            return;
        }
        self.last_refresh = Some((w, h, fbt, sizes));

        // C++ :5761 `refreshCamera()` (the port's `refresh_camera` also
        // resets the five menus' category offsets, MenuGame.cpp:3189-3204).
        menu.refresh_camera();

        // C++ :5762-5764 — `adjustLevelsOffset()` + `adjustMenuOffset(true)`
        // + `resetNamesScrolls()`; the C++ `refreshCamera` additionally ran
        // `updateLevelSelectionDrawingParameters` (:3246-3249), which
        // zeroes `levelDetailsOffset` / `lvlSlct.XOffset` / `favSlct.XOffset`
        // (:4296-4300).
        menu.adjust_menu_offset(true);
        {
            let mut s = shared.borrow_mut();
            s.adjust_levels_offset();
            s.level_details_offset = 0.0;
            s.lvl_slct.x_offset = 0.0;
            s.fav_slct.x_offset = 0.0;
        }
        self.names_scroll = [0.0; 6];

        // C++ `refreshCamera`'s camera half (:3141-3159):
        // `backgroundCamera` keeps its constructor view ({0,0} ×
        // {sizeX * zoom, sizeY * zoom}) and gains the aspect skew;
        // `overlayCamera.setView({{0, 0}, {w, h}})` — the virtual menu
        // space (the constructor's config-based view is replaced by the
        // time the constructor finishes, since it ends with
        // `refreshCamera()`).
        let (size_x, size_y, zoom) = {
            let s = shared.borrow();
            let c = s.config();
            (c.size_x(), c.size_y(), c.zoom_factor())
        };
        let cam_size = Vec2::new(size_x * zoom, size_y * zoom);
        self.background_camera
            .set_view(View::new(Vec2::new(0.0, 0.0), cam_size));
        self.background_camera
            .set_skew(Vec2::new(1.0, if fbt { 0.8 } else { 0.6 }));
        self.overlay_camera
            .set_view(View::new(Vec2::new(0.0, 0.0), Vec2::new(w, h)));

        // C++ `refreshCamera`'s char-size half (:3207-3244): set the
        // aspect-dependent sizes on the fonts + `updateHeight` the cached
        // heights. (a) caches `txtMenuBig.height` only; the b/c states add
        // their own.
        self.txt_menu_big.set_character_size(sizes.menu_big as u32);
        self.h_menu_big = font_height(&self.font, sizes.menu_big as u32);

        // C++ `refreshCamera`'s sprite / version-text transforms
        // (:3144-3159) — verbatim:
        // `titleBar.setOrigin(0); setScale({0.5, 0.5});
        // setPosition({20, 20});`
        self.title_bar.set_origin(Vec2::new(0.0, 0.0));
        self.title_bar.set_scale(Vec2::new(0.5, 0.5));
        self.title_bar.set_position(Vec2::new(20.0, 20.0));
        // `txtVersion.font.setString(GAME_VERSION_STR); setOrigin({
        // getLocalRight(txtVersion.font), 0}); setPosition({
        // getGlobalRight(titleBar) - 15, getGlobalTop(titleBar) + 15});`
        self.txt_version.set_string("2.1.7");
        self.txt_version.set_origin(Vec2::new(
            self.txt_version.get_local_bounds().get_width(),
            0.0,
        ));
        let title = self.title_bar.get_global_bounds();
        self.txt_version.set_position(Vec2::new(
            title.left + title.get_width() - 15.0,
            title.top + 15.0,
        ));
        // `creditsBar1.setOrigin({getLocalWidth(creditsBar1), 0});
        // setScale({0.373, 0.373}); setPosition({w - 20, 20});`
        self.credits_bar1.set_origin(Vec2::new(
            self.credits_bar1.get_local_bounds().get_width(),
            0.0,
        ));
        self.credits_bar1.set_scale(Vec2::new(0.373, 0.373));
        self.credits_bar1.set_position(Vec2::new(w - 20.0, 20.0));
        // `creditsBar2.setOrigin({getLocalWidth(creditsBar2), 0});
        // setScale({0.373, 0.373}); setPosition({w - 20, 17 +
        // getGlobalBottom(creditsBar1)});`
        self.credits_bar2.set_origin(Vec2::new(
            self.credits_bar2.get_local_bounds().get_width(),
            0.0,
        ));
        self.credits_bar2.set_scale(Vec2::new(0.373, 0.373));
        let credits1_bottom = self.credits_bar1.get_global_bounds();
        self.credits_bar2.set_position(Vec2::new(
            w - 20.0,
            17.0 + credits1_bottom.top + credits1_bottom.get_height(),
        ));
        // `scaleFactor = w / 1024; epilepsyWarning.setOrigin(
        // getLocalCenter); setPosition({1024 / (2 / scaleFactor), 768 / 2 -
        // 50}); setScale({0.36, 0.36});` (the (c) state draws it).
        let scale_factor = w / 1024.0;
        self.epilepsy_warning.set_origin(Vec2::new(
            self.epilepsy_warning.get_local_bounds().get_width() / 2.0,
            self.epilepsy_warning.get_local_bounds().get_height() / 2.0,
        ));
        self.epilepsy_warning
            .set_position(Vec2::new(1024.0 / (2.0 / scale_factor), 768.0 / 2.0 - 50.0));
        self.epilepsy_warning.set_scale(Vec2::new(0.36, 0.36));
    }

    /// C++ `backgroundCamera.apply()` (the skew is baked in).
    fn background_view(&mut self) -> View {
        self.background_camera.apply()
    }

    /// C++ `overlayCamera.apply()`.
    fn overlay_view(&mut self) -> View {
        self.overlay_camera.apply()
    }
}

/// C++ `Utils::getFontHeight(font, charSize)` (FontHeight.cpp:11-20): the
/// global-bounds height of the probe string at the given char size.
fn font_height(font: &Font, char_size: u32) -> f32 {
    let probe = Text::new(font.clone(), "ABCDEFGHILMNOPQRSTUVZ:", char_size);
    let (_, h, _) = probe.measure();
    h
}

// ---------------------------------------------------------------------------
// The C++ `render` / `renderText*` helpers (the `ssvs::` SFML helpers in
// MenuGame.cpp) — thin wrappers so the state code below reads like the C++.
// Every menu draw uses the default `RenderStates` (the menu stages apply no
// per-stage shader — `RenderStates::default()` is the C++ `window.draw`
// default).
// ---------------------------------------------------------------------------

/// C++ `render(drawable)` — default render states.
fn render_sprite(target: &mut dyn DrawTarget, sprite: &Sprite) {
    target.draw_sprite(sprite);
}

/// C++ `render(shape)` — default render states.
fn render_rectangle(target: &mut dyn DrawTarget, rect: &RectangleShape) {
    target.draw_rectangle(rect, RenderStates::default());
}

/// C++ `renderText(str, font, pos)` — `font.setString(str);
/// font.setPosition(pos); render(font);`
fn render_text(target: &mut dyn DrawTarget, txt: &mut Text, s: &str, pos: Vec2) {
    txt.set_string(s);
    txt.set_position(pos);
    target.draw_text(txt, RenderStates::default());
}

/// C++ `renderText(str, font, pos, color)` — the C++ saves/restores the
/// fill color around the draw.
fn render_text_color(
    target: &mut dyn DrawTarget,
    txt: &mut Text,
    s: &str,
    pos: Vec2,
    color: Color,
) {
    let old = txt.get_fill_color();
    txt.set_fill_color(color);
    render_text(target, txt, s, pos);
    txt.set_fill_color(old);
}

/// C++ `renderTextCentered(str, font, pos)` — `pos.x -
/// getGlobalWidth(font) / 2` after setting the string.
#[allow(dead_code)] // lands with the 13b(b)/(c) states
fn render_text_centered(target: &mut dyn DrawTarget, txt: &mut Text, s: &str, pos: Vec2) {
    txt.set_string(s);
    let x = pos.x - txt.get_global_bounds().get_width() / 2.0;
    txt.set_position(Vec2::new(x, pos.y));
    target.draw_text(txt, RenderStates::default());
}

/// C++ `renderTextCentered(str, font, pos, color)`.
#[allow(dead_code)] // lands with the 13b(b)/(c) states
fn render_text_centered_color(
    target: &mut dyn DrawTarget,
    txt: &mut Text,
    s: &str,
    pos: Vec2,
    color: Color,
) {
    let old = txt.get_fill_color();
    txt.set_fill_color(color);
    render_text_centered(target, txt, s, pos);
    txt.set_fill_color(old);
}

/// C++ `renderTextCenteredOffset(str, font, pos, xOffset)` — `xOffset +
/// pos.x - getGlobalWidth(font) / 2`.
#[allow(dead_code)] // lands with the 13b(b) level selection
fn render_text_centered_offset(
    target: &mut dyn DrawTarget,
    txt: &mut Text,
    s: &str,
    pos: Vec2,
    x_offset: f32,
) {
    txt.set_string(s);
    let x = x_offset + pos.x - txt.get_global_bounds().get_width() / 2.0;
    txt.set_position(Vec2::new(x, pos.y));
    target.draw_text(txt, RenderStates::default());
}

/// C++ `renderTextCenteredOffset(str, font, pos, xOffset, color)`.
#[allow(dead_code)] // lands with the 13b(b) level selection
fn render_text_centered_offset_color(
    target: &mut dyn DrawTarget,
    txt: &mut Text,
    s: &str,
    pos: Vec2,
    x_offset: f32,
    color: Color,
) {
    let old = txt.get_fill_color();
    txt.set_fill_color(color);
    render_text_centered_offset(target, txt, s, pos, x_offset);
    txt.set_fill_color(old);
}

/// C++ `createQuad(color, x1, x2, y1, y2)` (MenuGame.cpp:3520-3525).
fn create_quad(b: &mut VertexBatch, color: Color, x1: f32, x2: f32, y1: f32, y2: f32) {
    b.push_quad(
        color,
        Vec2::new(x1, y1), // nw
        Vec2::new(x1, y2), // sw
        Vec2::new(x2, y2), // se
        Vec2::new(x2, y1), // ne
    );
}

/// C++ `createQuadTrapezoid(color, x1, x2, x3, y1, y2, left)`
/// (MenuGame.cpp:3533-3553).
#[allow(clippy::too_many_arguments)] // the C++ signature + the port's batch
fn create_quad_trapezoid(
    b: &mut VertexBatch,
    color: Color,
    x1: f32,
    x2: f32,
    x3: f32,
    y1: f32,
    y2: f32,
    left: bool,
) {
    let (nw, ne, se, sw) = if left {
        (
            Vec2::new(x1, y1),
            Vec2::new(x2, y1),
            Vec2::new(x3, y2),
            Vec2::new(x1, y2),
        )
    } else {
        (
            Vec2::new(x1, y1),
            Vec2::new(x2, y1),
            Vec2::new(x2, y2),
            Vec2::new(x3, y2),
        )
    };
    b.push_quad(color, nw, sw, se, ne);
}

// ---------------------------------------------------------------------------
// `MenuGame::draw` (MenuGame.cpp:5751-5986)
// ---------------------------------------------------------------------------

/// C++ `MenuGame::draw()` — the full menu draw dispatch.
///
/// See the module docs for the fidelity notes (mouse, `mustRefresh`,
/// `levelStatus`, screenshots). The state arms land per milestone: (a)
/// `SMain` + the always-on passes; (b) `LevelSelection`; (c) the boot /
/// profile / options / online / load states.
pub fn draw(menu: &MenuGame, app: &mut MenuDraw, target: &mut dyn DrawTarget) {
    // C++ :5753-5756 — the mouse state (module docs: always the no-hover
    // path in the port).

    // C++ :5758-5765 — the `mustRefresh` block (size-driven in the port).
    app.refresh_if_stale(menu);

    // C++ :5767 `styleData.computeColors()` — the current level's style
    // (the C++ `styleData` member is set by `setIndex`; the seam runs
    // `computeColors` too). `_level_id` is the (b)/(c) states' key (the
    // level-selection panels read the level's data through it).
    let (_level_id, style_bg) = {
        let s = menu.shared().borrow();
        let id = s
            .current_level
            .as_ref()
            .map(|l| l.id.clone())
            .unwrap_or_default();
        let bg = s.assets().style_background(&id);
        (id, bg)
    };

    // C++ :5768 `window.clear(sf::Color{0, 0, 0, 255})`.
    target.clear_black();

    // C++ :5770 `window.setView(backgroundCamera.apply())`.
    target.set_view(app.background_view());

    // The C++ reads its members directly; the port snapshots the shared
    // state in ONE short borrow (the draw helpers then need their own
    // `borrow_mut`s — e.g. `draw_main_menu` writing `menuHalfHeight` / the
    // item offsets — which would deadlock against a held `Ref`).
    let s = SharedSnapshot::take(menu);
    let state = s.state;
    let w = s.w;
    let h = s.h;
    let four_by_three = s.four_by_three;
    let menu_text_color = s.menu_text_color;
    let menu_quad_color = s.menu_quad_color;
    let menu_selection_color = s.menu_selection_color;
    let dialog_box_text_color = s.dialog_box_text_color;
    let entering_text_offset = s.entering_text_offset;
    let level_details_offset = s.level_details_offset;
    let dialog = s.dialog;
    let main_or_above = (state as i32) >= (States::SMain as i32);

    // C++ :5775-5787 — the hexagon-menu background (module docs:
    // `levelStatus.sides` = 6 default, `darken` = false).
    if main_or_above {
        app.menu_background_tris.clear();
        if let Some(bg) = &style_bg {
            draw_background_menu(
                &mut app.menu_background_tris,
                Vec2::new(0.0, 0.0),
                6,
                false,
                s.black_and_white,
                four_by_three,
                &bg.colors,
                bg.bg_tile_radius,
                bg.bg_rot_off,
                bg.main_color,
                bg.cap_color.unwrap_or(Color::BLACK),
            );
        }
        target.draw_vertices(app.menu_background_tris.as_slice(), RenderStates::default());
    }

    // C++ :5789 `window.setView(overlayCamera.apply())`.
    target.set_view(app.overlay_view());

    // C++ :5791-5817 — the "CURRENT PROFILE:" line.
    if main_or_above && state != States::LevelSelection {
        let mut str_buf = String::from("CURRENT PROFILE: ");
        str_buf.push_str(&s.profile_name);

        let missing = &s.missing_dependencies;
        if !missing.is_empty() {
            str_buf.push_str("\n\nWARNING - PACKS WITH MISSING DEPENDENCIES:");
            for p in missing {
                str_buf.push_str("\n    ");
                str_buf.push_str(p);
            }
            str_buf.push_str("\nFORGOT TO DOWNLOAD THEM FROM THE STEAM WORKSHOP?");
        }

        // C++ `{20.f, getGlobalBottom(titleBar) + 8}` (the placeholder
        // texture shifts the value by the real-texture-size delta — the
        // module docs note).
        let title_bounds = app.title_bar.get_global_bounds();
        let title_bottom = title_bounds.top + title_bounds.get_height();
        render_text(
            target,
            &mut app.txt_selection_small,
            &str_buf,
            Vec2::new(20.0, title_bottom + 8.0),
        );
    }

    // C++ :5819-5826 — the indents.
    let (indent_big, indent_small, profile_indent) = if four_by_three {
        (280.0, 410.0, -75.0)
    } else {
        (400.0, 540.0, -100.0)
    };

    // C++ :5828 — the state dispatch.
    let fps_mult = layout::fps_mult(app.fps);
    check_names_scroll_resets(menu, app, state);
    match state {
        States::LoadingScreen => {
            // TODO(issue-13b-load-results): C++ :5830-5833 —
            // `drawLoadResults()` + the "PRESS ANY KEY" line.
        }
        States::EpilepsyWarning => {
            // TODO(issue-13b-epilepsy): C++ :5836-5840 — render the
            // `epilepsyWarning` sprite + the "PRESS ANY KEY" line.
        }
        States::ETLPNewBoot => {
            // TODO(issue-13b-entering-text-boot): C++ :5842-5845 —
            // `drawEnteringTextBoot()` + `drawGraphics()`.
        }
        States::SLPSelectBoot => {
            // TODO(issue-13b-profile-selection-boot): C++ :5847-5850 —
            // `drawProfileSelectionBoot()` + `drawGraphics()`.
        }
        States::SMain => {
            // C++ :5853-5866 — fold the previous menus.
            if menu.options_menu.get_category().get_offset() != 0.0 {
                draw_main_menu(
                    menu,
                    app,
                    &menu.options_menu.get_category_by_name("options"),
                    w - indent_big,
                    true,
                    menu_quad_color,
                    menu_text_color,
                    fps_mult,
                    target,
                );
            }
            if menu.profile_selection_menu.get_category().get_offset() != 0.0 {
                // TODO(issue-13b-profile-selection): C++ :5861-5863 —
                // `drawProfileSelection(profileIndent, true)`.
            }
            if entering_text_offset != 0.0 {
                // TODO(issue-13b-entering-text): C++ :5866-5868 —
                // `drawEnteringText(profileIndent, true)`.
            }
            if level_details_offset != 0.0 {
                // TODO(issue-13b-level-selection): C++ :5871-5874 —
                // `drawLevelSelectionRightSide(*lvlDrawer, true)` +
                // `drawLevelSelectionLeftSide(*lvlDrawer, true)`.
            }

            // C++ :5877-5879 — the main menu (right) + graphics + online.
            draw_main_submenus(
                menu,
                app,
                &menu.main_menu,
                indent_big,
                w,
                h,
                menu_quad_color,
                menu_text_color,
                fps_mult,
                target,
            );
            draw_graphics(app, target);
            draw_online_status(menu, app, target);
        }
        States::MOpts => {
            // TODO(issue-13b-options): C++ :5882-5894 — fold the main menu
            // (offset-gated), `drawMainMenu(optionsMenu.
            // getCategoryByName("options"), w - indentBig, false)`,
            // `drawSubmenusSmall(optionsMenu.getCategories(), indentSmall)`,
            // `drawGraphics()`, `drawOnlineStatus()`.
            let _ = (
                w,
                h,
                menu_quad_color,
                menu_text_color,
                menu_selection_color,
                indent_big,
                indent_small,
                profile_indent,
                fps_mult,
            );
        }
        States::MOnline => {
            // TODO(issue-13b-online): C++ :5897-5909 — the `onlineMenu`
            // analogue of the MOpts arm.
            let _ = (
                w,
                h,
                menu_quad_color,
                menu_text_color,
                menu_selection_color,
                indent_big,
                indent_small,
                profile_indent,
                fps_mult,
            );
        }
        States::ETLPNew => {
            // TODO(issue-13b-entering-text): C++ :5912-5916 —
            // `drawMainMenu(mainMenu.getCategoryByName("local profiles"),
            // w - indentBig, false)` + `drawEnteringText(profileIndent,
            // false)` + `drawGraphics()`.
            let _ = (
                w,
                h,
                menu_quad_color,
                menu_text_color,
                menu_selection_color,
                indent_big,
                indent_small,
                profile_indent,
                fps_mult,
            );
        }
        States::SLPSelect => {
            // TODO(issue-13b-profile-selection): C++ :5919-5923 — the
            // `local profiles` `drawMainMenu` + `drawProfileSelection` +
            // `drawGraphics()`.
            let _ = (
                w,
                h,
                menu_quad_color,
                menu_text_color,
                menu_selection_color,
                indent_big,
                indent_small,
                profile_indent,
                fps_mult,
            );
        }
        States::LevelSelection => {
            // TODO(issue-13b-level-selection): C++ :5926-5949 — the
            // main-menu fold (offset-gated), the favorite / regular drawer
            // `drawLevelSelectionRightSide` pair (F2-gated),
            // `drawLevelSelectionLeftSide(*lvlDrawer, false)`,
            // `drawOnlineStatus()`.
            let _ = (
                w,
                h,
                menu_quad_color,
                menu_text_color,
                menu_selection_color,
                indent_big,
                indent_small,
                profile_indent,
                fps_mult,
            );
        }
    }

    // C++ :5974-5978 — `mustTakeScreenshot` (module docs: always false in
    // the port).

    // C++ :5980-5984 — the dialog box pass.
    let backdrop = style_bg
        .as_ref()
        .and_then(|bg| bg.colors.first().copied())
        .unwrap_or(Color::BLACK);
    if !dialog.empty() {
        // C++ `window.setView(overlayCamera.apply())` (re-applied).
        target.set_view(app.overlay_view());
        draw_dialog_box(app, &dialog, dialog_box_text_color, backdrop, w, h, target);
    }

    // C++ :5986-5988 — the `lastMouseClick` edge (module docs: no mouse).
}

/// The one-shot shared-state snapshot `draw` reads before the state
/// dispatch (the C++'s direct member reads; see the note in `draw` on why
/// the port must not hold a `Ref` across the helpers).
struct SharedSnapshot {
    state: States,
    w: f32,
    h: f32,
    four_by_three: bool,
    menu_text_color: Color,
    menu_quad_color: Color,
    menu_selection_color: Color,
    dialog_box_text_color: Color,
    entering_text_offset: f32,
    level_details_offset: f32,
    dialog: HexagonDialogBox,
    profile_name: String,
    missing_dependencies: Vec<String>,
    black_and_white: bool,
}

impl SharedSnapshot {
    fn take(menu: &MenuGame) -> Self {
        let s = menu.shared().borrow();
        Self {
            state: s.state,
            w: s.w,
            h: s.h,
            four_by_three: s.four_by_three,
            menu_text_color: s.menu_text_color,
            menu_quad_color: s.menu_quad_color,
            menu_selection_color: s.menu_selection_color,
            dialog_box_text_color: s.dialog_box_text_color,
            entering_text_offset: s.entering_text_offset,
            level_details_offset: s.level_details_offset,
            dialog: s.dialog.clone(),
            profile_name: s.assets().current_profile_name().unwrap_or_default(),
            missing_dependencies: s.assets().pack_ids_with_missing_dependencies(),
            black_and_white: s.config().black_and_white(),
        }
    }
}

// ---------------------------------------------------------------------------
// `drawMainSubmenus` (MenuGame.cpp:3586-3604)
// ---------------------------------------------------------------------------

/// C++ `MenuGame::drawMainSubmenus(subMenus, indent)` — draw every
/// `mainMenu` category that is the current one or still carrying an
/// (un-folded) offset.
#[allow(clippy::too_many_arguments)] // the C++ members threaded through the bridge
fn draw_main_submenus(
    menu: &MenuGame,
    app: &mut MenuDraw,
    main_menu: &Menu,
    indent: f32,
    w: f32,
    h: f32,
    menu_quad_color: Color,
    menu_text_color: Color,
    fps_mult: f32,
    target: &mut dyn DrawTarget,
) {
    let categories = main_menu.get_categories();
    let current = menu.main_menu.get_category();
    let current_name = current.get_name().to_string();
    for c in &categories {
        let currently_selected = current_name == c.get_name();
        let has_offset = c.get_offset() != 0.0;
        // C++ `if(!currentlySelected && !hasOffset) continue;`
        if !currently_selected && !has_offset {
            continue;
        }
        draw_main_menu(
            menu,
            app,
            c,
            w - indent,
            !currently_selected && has_offset,
            menu_quad_color,
            menu_text_color,
            fps_mult,
            target,
        );
    }
    let _ = h; // (the C++ reads `h` inside `drawMainMenu`)
}

// ---------------------------------------------------------------------------
// `drawMainMenu` (MenuGame.cpp:3700-3782)
// ---------------------------------------------------------------------------

/// C++ `MenuGame::drawMainMenu(mSubMenu, baseIndent, revertOffset)` — the
/// right-hand menu column (the main / options / online panels and the
/// "local profiles" column).
#[allow(clippy::too_many_arguments)] // the C++ members threaded through the bridge
fn draw_main_menu(
    menu: &MenuGame,
    app: &mut MenuDraw,
    m_sub_menu: &Category,
    base_indent: f32,
    revert_offset: bool,
    menu_quad_color: Color,
    menu_text_color: Color,
    fps_mult: f32,
    target: &mut dyn DrawTarget,
) {
    let items = m_sub_menu.get_items();
    let size = items.len();
    let (w, h) = {
        let s = menu.shared().borrow();
        (s.w, s.h)
    };

    // C++ "Global menu offset" —
    // `calcMenuOffset(mSubMenu.getOffset(), -(baseIndent - w),
    // revertOffset)` (speedUp = false, the 3-arg overload).
    let mut offset = m_sub_menu.get_offset();
    let panel_offset = layout::calc_menu_offset(
        &mut offset,
        -(base_indent - w),
        revert_offset,
        false,
        fps_mult,
    );
    *m_sub_menu.get_offset_mut() = offset;
    let base_indent = base_indent + panel_offset;

    // C++ "Calculate quads coordinates" (font height = the cached
    // `txtMenuBig.height`).
    let h_menu_big = app.h_menu_big;
    let interline = 3.0 * h_menu_big;
    let quad_border = h_menu_big * layout::FRAME_SIZE_MULTI;
    let double_border = 2.0 * quad_border;
    let total_height = interline * (size as f32 - 1.0) + double_border + h_menu_big;
    let mut quad_height = (h - total_height) / 2.0 + interline - quad_border;
    let mut txt_height = quad_height - h_menu_big * layout::FONT_HEIGHT_OFFSET + quad_border;

    // C++ "Store info needed to draw the submenus" —
    // `menuHalfHeight = quadHeight + totalHeight / 2.f`.
    {
        let mut s = menu.shared().borrow_mut();
        s.menu_half_height = quad_height + total_height / 2.0;
    }

    // C++ "Draw the quads that surround the text" (module docs: the
    // `mouseOverlaps` vector + the `mustUseMenuItem` edge are the no-hover
    // no-click C++ paths in the port).
    app.menu_quads.clear();
    app.menu_quads.reserve_quad(size);
    for (i, item) in items.iter().enumerate() {
        // C++ `calcMenuItemOffset(items[i]->getOffset(),
        // i == mSubMenu.getIdx())`.
        {
            let mut item = item.borrow_mut();
            layout::calc_menu_item_offset(
                item.base_mut().offset_mut(),
                i as i32 == m_sub_menu.get_idx(),
                fps_mult,
            );
        }
        let off = item.borrow().offset();
        let indent = base_indent - off;

        // C++ `bodyMins` / `bodyMaxs` (the hover region — unused in the
        // no-mouse port, kept here so the C++ layout is traceable):
        // `{indent - h * 2.5, quadHeight}` .. `{w, quadHeight +
        // doubleBorder + h}`.
        let _ = indent;

        // C++ `c = !items[i]->isEnabled() ? {110, 110, 110} : menuQuadColor;`
        let enabled = item.borrow().is_enabled();
        let c = if enabled {
            menu_quad_color
        } else {
            Color::new(110, 110, 110, 255)
        };

        // C++ `createQuadTrapezoid(mouseOverlapColor(false, c),
        // indent - h * 2.5, w, indent - h / 2, quadHeight, quadHeight +
        // doubleBorder + h, false)`.
        create_quad_trapezoid(
            &mut app.menu_quads,
            c,
            indent - h_menu_big * 2.5,
            w,
            indent - h_menu_big / 2.0,
            quad_height,
            quad_height + double_border + h_menu_big,
            false,
        );

        quad_height += interline;
    }
    target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());

    // C++ "Draw the text on top of the quads".
    for item in &items {
        let off = item.borrow().offset();
        let indent = base_indent - off;
        let name = item.borrow().get_name();
        let enabled = item.borrow().is_enabled();
        let c = if enabled {
            menu_text_color
        } else {
            Color::new(150, 150, 150, 255)
        };
        render_text_color(
            target,
            &mut app.txt_menu_big,
            &name,
            Vec2::new(indent, txt_height),
            c,
        );
        txt_height += interline;
    }
}

// ---------------------------------------------------------------------------
// `drawGraphics` (MenuGame.cpp:5991-5997)
// ---------------------------------------------------------------------------

/// C++ `MenuGame::drawGraphics()` (MenuGame.cpp:5991-5997) — the
/// title/credits bars + the version text (its string/origin/position were
/// set by `refresh_if_stale`, C++ `refreshCamera` :3148-3150).
fn draw_graphics(app: &mut MenuDraw, target: &mut dyn DrawTarget) {
    // C++ `render(titleBar); render(creditsBar1); render(creditsBar2);`
    render_sprite(target, &app.title_bar);
    render_sprite(target, &app.credits_bar1);
    render_sprite(target, &app.credits_bar2);
    // C++ `render(txtVersion.font)` (default render states).
    target.draw_text(&app.txt_version, RenderStates::default());
}

// ---------------------------------------------------------------------------
// `drawOnlineStatus` (MenuGame.cpp:5999-6095)
// ---------------------------------------------------------------------------

/// C++ `MenuGame::drawOnlineStatus()` — the bottom-right connection badge.
fn draw_online_status(menu: &MenuGame, app: &mut MenuDraw, target: &mut dyn DrawTarget) {
    let net = menu.shared().borrow().network().clone();

    // C++ `window.getRenderWindow().setView({{0, 0},
    // {getWindowWidth(), getWindowHeight()}})` — the identity (window)
    // view; in the port the window size is the config size (module docs).
    let (win_w, win_h) = {
        let s = menu.shared().borrow();
        let c = s.config();
        (c.size_x(), c.size_y())
    };
    target.set_view(View::new(Vec2::new(0.0, 0.0), Vec2::new(win_w, win_h)));

    // C++ `onlineStatusScaling = 1.5f; scaling = 1.5f /
    // Config::getZoomFactor(); padding = 3.f * onlineStatusScaling;`
    let online_status_scaling = 1.5;
    let zoom = menu.shared().borrow().config().zoom_factor();
    let scaling = online_status_scaling / zoom;
    let padding = 3.0 * online_status_scaling;

    // C++ `txtOnlineStatus.font.setCharacterSize(10 * scaling);
    // setFillColor(White);`
    app.txt_online_status
        .set_character_size((10.0 * scaling) as u32);
    app.txt_online_status.set_fill_color(Color::WHITE);

    // C++ `hexagonClient.getState()` → `(stateGood, stateString)`.
    let (state_good, state_string): (bool, String) = match net.state() {
        ConnectionState::Disconnected => (false, "DISCONNECTED".into()),
        ConnectionState::Connecting => (false, "CONNECTING".into()),
        ConnectionState::ConnectionError => (false, "CONNECTION ERROR".into()),
        ConnectionState::Connected => {
            if net.has_rt_keys() {
                (true, "CONNECTED, PLEASE LOG IN".into())
            } else {
                (false, "CONNECTED, BUT KEY EXCHANGE FAILED".into())
            }
        }
        ConnectionState::LoggedIn | ConnectionState::LoggedIn_Ready => (
            true,
            format!(
                "LOGGED IN AS {}",
                net.login_name().unwrap_or_else(|| "UNKNOWN".into())
            ),
        ),
    };

    // C++ `txtHeight = getGlobalHeight(txtOnlineStatus.font)` (measured on
    // the `ABC,:ç@'` probe string) → `spriteScale`.
    app.txt_online_status.set_string("ABC,:ç@'");
    let txt_height = app.txt_online_status.get_global_bounds().get_height();
    let sprite_scale = (txt_height + 2.0 * padding) / 64.0;

    // C++ `txtOnlineStatus.font.setString(stateString);` (+ the texture
    // swap — both textures are placeholders in the port, module docs; the
    // `stateGood` branch picks `onlineIcon` vs `onlineIconFail`).
    let _ = state_good;
    app.txt_online_status.set_string(&state_string);
    app.s_online
        .set_scale(Vec2::new(sprite_scale, sprite_scale));
    app.s_online.set_origin(Vec2::new(0.0, 0.0)); // C++ `getLocalSW` = (0, 0)
    app.s_online
        .set_position(Vec2::new(padding, win_h - padding));

    // C++ `rsOnlineStatus.setSize({getGlobalWidth(txtOnlineStatus) +
    // 4 * padding, txtHeight + 2 * padding}); setFillColor(Black);
    // setOrigin(getLocalSW(...)); setPosition({getGlobalRight(sOnline) +
    // padding, sOnline.getPosition().y});`
    let text_width = app.txt_online_status.get_global_bounds().get_width();
    app.rs_online_status.set_size(Vec2::new(
        text_width + 4.0 * padding,
        txt_height + 2.0 * padding,
    ));
    app.rs_online_status.set_fill_color(Color::BLACK);
    app.rs_online_status.set_origin(Vec2::new(0.0, 0.0));
    let online_right = app.s_online.get_position().x + 64.0 * sprite_scale; // C++ `getGlobalRight(sOnline)`
    app.rs_online_status
        .set_position(Vec2::new(online_right + padding, win_h - padding));

    // C++ `txtOnlineStatus.setOrigin(getLocalCenterW(...));
    // setPosition({getGlobalLeft(rsOnlineStatus) + 2 * padding,
    // getGlobalCenter(rsOnlineStatus).y});`
    let local_center_w = app.txt_online_status.get_local_bounds().get_width() / 2.0;
    app.txt_online_status
        .set_origin(Vec2::new(local_center_w, 0.0));
    app.txt_online_status.set_position(Vec2::new(
        online_right + 2.0 * padding,
        win_h - padding + (txt_height + 2.0 * padding) / 2.0,
    ));

    // C++ `render(sOnline); render(rsOnlineStatus); render(txtOnlineStatus);`
    render_sprite(target, &app.s_online);
    render_rectangle(target, &app.rs_online_status);
    target.draw_text(&app.txt_online_status, RenderStates::default());

    // The C++ `LoggedIn` / `LoggedIn_Ready` branch also persists the
    // login name (`Config::setLastLoginUsername`, MenuGame.cpp:6035-6045)
    // when `Config::getSaveLastUsername()` — the port mirrors the side
    // effect.
    let state = net.state();
    if (state == ConnectionState::LoggedIn || state == ConnectionState::LoggedIn_Ready)
        && menu.shared().borrow().config().save_last_username()
    {
        if let Some(name) = net.login_name() {
            menu.shared()
                .borrow()
                .config()
                .set_last_login_username(&name);
        }
    }
}

// ---------------------------------------------------------------------------
// The `HexagonDialogBox` pass (C++ `HexagonDialogBox::draw` + `drawText`,
// HexagonDialogBox.cpp:100-230)
// ---------------------------------------------------------------------------

/// C++ `dialogBox.draw(dialogBoxTextColor, styleData.getColor(0))` — the
/// frame quads (the port's `HexagonDialogBox::draw` coarse output) + the
/// text lines (`drawText`, HexagonDialogBox.cpp:124-156).
fn draw_dialog_box(
    app: &mut MenuDraw,
    dialog: &HexagonDialogBox,
    txt_color: Color,
    backdrop: Color,
    config_width: f32,
    config_height: f32,
    target: &mut dyn DrawTarget,
) {
    // C++ frame quads (`drawTopLeft` / `drawCenter` /
    // `drawCenterUpperHalf` → `drawBox` → `window.draw`); `Config::
    // getWidth()/getHeight()` map to the config size (module docs).
    let mut quads: Vec<DialogQuad> = Vec::new();
    dialog.draw(txt_color, backdrop, config_width, config_height, &mut quads);
    app.menu_quads.clear();
    for q in &quads {
        create_quad(&mut app.menu_quads, q.color, q.x1, q.x2, q.y1, q.y2);
    }
    target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());

    // C++ `drawText(txtColor, xOffset, yOffset)` per mode (HexagonDialogBox
    // .cpp:163, :209, :228).
    app.txt_dialog.set_character_size(dialog.char_size());
    app.txt_dialog.set_fill_color(txt_color);
    let interline = dialog.line_height() * 1.5;
    let (x_offset, y_offset) = match dialog.draw_mode() {
        DBoxDraw::TopLeft => (
            dialog.x_pos() + 2.0 * dialog.double_frame_size() + dialog.dialog_width() / 2.0,
            dialog.y_pos() - dialog.line_height() * FONT_HEIGHT_DIFFERENTIAL
                + 2.0 * dialog.double_frame_size(),
        ),
        DBoxDraw::Center => {
            let fmax = (1024.0 / config_width).max(768.0 / config_height);
            let cw = config_width * fmax;
            let ch = config_height * fmax / 2.0 + dialog.y_pos();
            let half = dialog.total_height() / 2.0;
            (
                cw / 2.0,
                ch - half - dialog.line_height() * FONT_HEIGHT_DIFFERENTIAL
                    + 2.0 * dialog.double_frame_size(),
            )
        }
        DBoxDraw::CenterUpperHalf => {
            let fmax = (1024.0 / config_width).max(768.0 / config_height);
            let cw = config_width * fmax;
            let ch = config_height * fmax / 2.0 + dialog.y_pos();
            (
                cw / 2.0,
                ch - dialog.total_height() - dialog.line_height() * FONT_HEIGHT_DIFFERENTIAL
                    + 2.0 * dialog.double_frame_size(),
            )
        }
    };
    let mut height_offset = 0.0;
    for str in dialog.dialog_text() {
        // C++: skip empty lines (but still advance `heightOffset`).
        if !str.is_empty() {
            app.txt_dialog.set_string(str);
            let gw = app.txt_dialog.get_global_bounds().get_width();
            app.txt_dialog.set_position(Vec2::new(
                x_offset - gw / 2.0,
                y_offset + height_offset + 5.0,
            ));
            target.draw_text(&app.txt_dialog, RenderStates::default());
        }
        height_offset += interline;
    }
    if dialog.input_box() {
        height_offset += interline;
        app.txt_dialog.set_string(if dialog.input_box_password() {
            "*".repeat(dialog.input().len())
        } else {
            dialog.input().to_string()
        });
        let gw = app.txt_dialog.get_global_bounds().get_width();
        app.txt_dialog.set_position(Vec2::new(
            x_offset - gw / 2.0,
            y_offset + height_offset + 5.0,
        ));
        target.draw_text(&app.txt_dialog, RenderStates::default());
    }
}

// ---------------------------------------------------------------------------
// The `namesScroll` reset triggers (module docs)
// ---------------------------------------------------------------------------

/// Run the C++ `namesScroll` reset triggers for this frame (the (b)
/// level-selection left panel consumes `names_scroll`; until then the
/// trigger bookkeeping runs so the scrolls are correct when it lands).
fn check_names_scroll_resets(menu: &MenuGame, app: &mut MenuDraw, state: States) {
    let (leaving_level_selection, pack_changed, level_changed) = {
        let s = menu.shared().borrow();
        let active: &LevelDrawer = if s.favorites_active {
            &s.fav_slct
        } else {
            &s.lvl_slct
        };
        (
            app.last_state == States::LevelSelection && state != States::LevelSelection,
            pack_signature(active) != app.last_pack_sig,
            active.current_index != app.last_level_index,
        )
    };
    // Leaving `LevelSelection` → full reset (C++ :2349, :2475, :1774).
    if leaving_level_selection || pack_changed {
        app.names_scroll = [0.0; 6];
    }
    // The level index changed (`leftRightAction`, C++ :1967, :2059) →
    // `resetLevelNamesScrolls` (LevelName + the music entries).
    if level_changed {
        app.names_scroll[0] = 0.0;
        for i in 3..6 {
            app.names_scroll[i] = 0.0;
        }
    }
    let s = menu.shared().borrow();
    let active: &LevelDrawer = if s.favorites_active {
        &s.fav_slct
    } else {
        &s.lvl_slct
    };
    app.last_state = state;
    app.last_pack_sig = pack_signature(active);
    app.last_level_index = active.current_index;
}

/// A cheap `(pack_idx, level count, is_favorites)` signature — the C++
/// resets the scrolls on the pack *change*, which any of these three
/// captures.
fn pack_signature(d: &LevelDrawer) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325; // FNV-1a
    for v in [
        d.pack_idx as u64,
        d.level_data_ids.len() as u64,
        d.is_favorites as u64,
    ] {
        h ^= v;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oh_menu::seams::{FakeAssets, FakeConfig, NoopNetwork};
    use oh_render::draw::{RenderStates, Vertex};
    use std::rc::Rc;

    /// A `DrawTarget` that records the call stream (type + a short
    /// fingerprint per call) so the tests assert the C++ draw-call ORDER.
    #[derive(Default)]
    struct Recording {
        calls: Vec<String>,
    }

    impl DrawTarget for Recording {
        fn clear_black(&mut self) {
            self.calls.push("clear".into());
        }
        fn set_view(&mut self, v: View) {
            self.calls.push(format!(
                "view({:.0},{:.0})",
                v.get_center().x,
                v.get_size().x
            ));
        }
        fn draw_vertices(&mut self, v: &[Vertex], _s: RenderStates) {
            self.calls.push(format!("verts({})", v.len()));
        }
        fn draw_sprite(&mut self, _s: &Sprite) {
            self.calls.push("sprite".into());
        }
        fn draw_text(&mut self, _t: &Text, _s: RenderStates) {
            self.calls.push("text".into());
        }
        fn draw_rectangle(&mut self, _r: &RectangleShape, _s: RenderStates) {
            self.calls.push("rect".into());
        }
        fn save_screenshot(&mut self, _p: &str) {
            self.calls.push("screenshot".into());
        }
    }

    /// The repo-shipped font (the C++ `openSquare`); the bold variant
    /// doubles as a stand-in until the b/c states use it.
    fn font() -> Font {
        let p = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../_RELEASE/Assets/OpenSquare-Regular.ttf"
        ));
        Font::load(p).expect("OpenSquare-Regular.ttf must load")
    }

    fn menu_and_app() -> (Rc<MenuGame>, MenuDraw) {
        let assets = FakeAssets::new();
        let config = FakeConfig::new();
        let network = Rc::new(NoopNetwork);
        let audio = oh_menu::RecordingAudio::new();
        let menu = Rc::new(MenuGame::new(
            assets,
            config,
            network,
            audio,
            None,
            None,
            None,
            Rc::new(oh_menu::layout::LinearFontHeight),
        ));
        let f = font();
        let app = MenuDraw::new(&f, &f);
        (menu, app)
    }

    #[test]
    fn smain_draws_background_profile_graphics_online_in_order() {
        let (menu, mut app) = menu_and_app();
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::SMain;
        }
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);

        let calls = &rec.calls;
        // C++ order: clear → background view → background verts → overlay
        // view → "CURRENT PROFILE" text → the main-menu quads (one verts
        // call per drawn category) + its texts → drawGraphics (3 sprites +
        // version text) → drawOnlineStatus (window view + sprite + rect +
        // text).
        assert_eq!(calls[0], "clear");
        assert!(calls[1].starts_with("view("), "background view set second");
        assert!(
            calls.get(2).is_some_and(|c| c.starts_with("verts(")),
            "the menu-hexagon background verts are third: {calls:?}"
        );
        assert!(calls[3].starts_with("view("), "overlay view fourth");
        let profile_text = calls
            .iter()
            .position(|c| c == "text")
            .expect("the CURRENT PROFILE line is drawn");
        let main_menu_verts = calls
            .iter()
            .position(|c| c.starts_with("verts(") && c != &calls[2])
            .expect("the main-menu quads are drawn");
        assert!(
            profile_text < main_menu_verts,
            "the profile line precedes the main menu: {calls:?}"
        );
        let graphics = calls
            .iter()
            .position(|c| c == "sprite")
            .expect("drawGraphics' title bar");
        assert_eq!(calls[graphics + 1], "sprite"); // creditsBar1
        assert_eq!(calls[graphics + 2], "sprite"); // creditsBar2
        assert_eq!(calls[graphics + 3], "text"); // txtVersion

        // drawOnlineStatus: identity view, sprite, rect, status text.
        let online_view = calls
            .iter()
            .rposition(|c| c.starts_with("view("))
            .expect("the online-status identity view");
        assert_eq!(calls[online_view + 1], "sprite");
        assert_eq!(calls[online_view + 2], "rect");
        assert_eq!(calls[online_view + 3], "text");
        assert_eq!(calls[calls.len() - 1], "text"); // the status text is last
    }

    #[test]
    fn smain_main_menu_quads_and_texts_are_doubled() {
        // The main menu's current category ("main") draws one quad trapezoid
        // per item + one text per item (C++ `drawMainMenu`'s two loops);
        // the "local profiles" category is folded (offset 0, not current) →
        // skipped by `drawMainSubmenus`.
        let (menu, mut app) = menu_and_app();
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::SMain;
        }
        let item_count = menu.main_menu.get_category().get_items().len();
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);

        let main_menu_verts = rec
            .calls
            .iter()
            .position(|c| c.starts_with("verts(") && c != &rec.calls[2])
            .expect("the main-menu quad batch");
        let quads: usize = rec.calls[main_menu_verts]
            .trim_start_matches("verts(")
            .trim_end_matches(')')
            .parse()
            .expect("vertex count");
        assert_eq!(quads / 6, item_count, "one trapezoid quad per item");
        let first_item_text = main_menu_verts + 1;
        for k in 0..item_count {
            assert_eq!(
                rec.calls[first_item_text + k],
                "text",
                "each item name is drawn in order"
            );
        }
    }

    #[test]
    fn loading_screen_skips_background_and_graphics() {
        let (menu, mut app) = menu_and_app();
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::LoadingScreen;
        }
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        // The C++ `mainOrAbove` gate: no background verts, no profile
        // line, no graphics (the LoadingScreen arm is the (c) TODO marker).
        assert!(!rec.calls.iter().any(|c| c.starts_with("verts(")));
        assert!(!rec.calls.iter().any(|c| c == "sprite"));
        assert!(!rec.calls.iter().any(|c| c == "text"));
        assert_eq!(rec.calls[0], "clear");
    }

    #[test]
    fn fps_mult_is_the_cpp_200_rate() {
        // C++ `getFPSMult` = `200.f / window.getFPS()` (MenuGame.cpp:
        // 5985-5989) — 60 FPS → 200/60.
        assert!((layout::fps_mult(60.0) - 200.0 / 60.0).abs() < 1e-6);
    }

    #[test]
    fn draw_is_deterministic_for_the_same_state() {
        let (menu, mut app) = menu_and_app();
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::SMain;
        }
        let mut a = Recording::default();
        draw(&menu, &mut app, &mut a);
        let mut b = Recording::default();
        draw(&menu, &mut app, &mut b);
        assert_eq!(a.calls, b.calls, "two draws of the same state match");
    }
}
