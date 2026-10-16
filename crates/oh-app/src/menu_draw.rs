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
//! condition in `refresh_if_stale` (issue 025g: the layout INPUTS — the
//! ACTUAL window size, the C++ `getWindowWidth()` / `getWindowHeight()`,
//! plus the config size / zoom the `backgroundCamera` view runs on —
//! changed since the last draw) and then runs the C++ `:5758-5765`
//! block: `refresh_camera()` (on the actual window size — the
//! config-vs-actual divergence the C++'s `window.onRecreation` half
//! reacts to), `adjust_levels_offset()`, `adjust_menu_offset(true)`,
//! `resetNamesScrolls()` (plus the `updateLevelSelectionDrawingParameters`
//! resets, MenuGame.cpp: 4296-4300). The first draw always refreshes (the
//! C++ constructor ran `refreshCamera()` already; the port's
//! `MenuGame::new` ran `refresh_camera()` too — the stale-detection just
//! re-applies the same idempotent setup once the cameras/texts exist).
//!
//! **`levelStatus.sides` is the default `6` and
//! `darkenUnevenBackgroundChunk` is `false`** in the background pass
//! (MenuGame.cpp:5780-5783): the menu's `LevelStatus` is the default-
//! constructed one (LevelStatus.hpp:66) and the menu Lua
//! (`onLoad`/`onUpdate` level-status hooks) is unwired (menu Lua is the
//! documented 13a gap).
//!
//! **`Config::getSizeX()/getSizeY()`** (the `backgroundCamera` view's
//! size in `refresh_if_stale`) map to the `MenuConfig` seam's
//! `size_x()`/`size_y()` (the C++'s `Config::getSizeX()` / `getSizeY()`,
//! MenuGame.cpp:3138-3140 — the derived square `max(width, height) * 1.3`).
//! The C++'s
//! window-size consumers are DIFFERENT (`getWindowWidth()` /
//! `getWindowHeight()` — `window.getRenderWindow().getSize()`, the
//! ACTUAL current window: the overlay-camera size, the online-status
//! identity view, the 9-zone grid's thirds — issue 025g: the port used
//! to read the config size for all of them, the config-vs-actual
//! divergence the device probe caught; they now run on the `MenuWindow`
//! seam's `menu.window_size()`.)
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
//! the full C++ layout either way, and issue-025c's GL sprite pass
//! renders them (the 1×1 placeholder texture uploads as-is — the
//! placeholder *size* still shifts the `getGlobalBottom`-relative
//! transforms (the "CURRENT PROFILE" line, the credits bars) by the
//! real-texture-size delta until the texture table lands).

use std::rc::Rc;

use oh_math::hg::color::Color;
use oh_math::hg::concat::format_ostream_f64;
use oh_math::hg::string::uppercasify;
use oh_math::hg::timestamp::{format_timepoint, to_timepoint};
use oh_math::ssvs::vector2::{get_orbit_rad, Vec2};
use oh_menu::dialog_box::{DBoxDraw, DialogQuad, HexagonDialogBox};
use oh_menu::layout::{self, DIFFICULTY_BUMP_EFFECT_MAX};
use oh_menu::menu_game::{LevelDrawer, MenuGame};
use oh_menu::seams::{
    ConnectionState, LevelInfo, LoadResults, MenuAssets, NetworkClient, ScoreEntry,
};
use oh_menu::states::{PackChange, States};
use oh_menu::{Category, Menu, Shared};
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

    // ---- C++ `MenuFont` members (MenuGame.cpp:199-224) ------------------
    // The fixed-constructor-size fonts; the aspect-dependent ones are
    // re-sized on refresh from `oh_menu::layout::char_sizes`.
    txt_version: Text,
    /// The boot states' "PRESS ANY KEY" line (the (b) LoadingScreen arm;
    /// the (c) EpilepsyWarning arm uses it too).
    txt_prof: Text,
    txt_menu_big: Text,
    /// C++ `txtMenuSmall` (`openSquare` — the options/online submenu
    /// labels; aspect-dependent char size, set on refresh: 16
    /// four-by-three / 24 otherwise, MenuGame.cpp:3209/:3224).
    txt_menu_small: Text,
    /// C++ `txtMenuTiny` (`openSquare` — the "[OFFICIAL MODE ENABLED]"
    /// badge; 9 four-by-three / 14 otherwise, MenuGame.cpp:
    /// 3208/:3223).
    txt_menu_tiny: Text,
    /// C++ `txtLoadSmall` (`openSquareBold`, `loadSmall` char size, the
    /// loading-screen labels / error lines).
    txt_load_small: Text,
    /// C++ `txtLoadBig` (`openSquare`, `loadBig` char size, the
    /// loading-screen counters).
    txt_load_big: Text,
    /// C++ `txtRandomTip` (`openSquare`, `randomTip` char size, the
    /// loading-screen tip lines).
    txt_random_tip: Text,
    /// C++ `txtSelectionBig` (`openSquareBold`, `selectionBig` char size,
    /// the level names + the "ONLINE LEADERBOARD" heading).
    txt_selection_big: Text,
    /// C++ `txtSelectionMedium` (`openSquareBold`, fixed char size 19 — the
    /// pack labels, the difficulty / pack / music / leaderboard lines).
    txt_selection_medium: Text,
    /// C++ `txtSelectionSmall` (`openSquare`, `selectionSmall` char size
    /// 14 — the C++ constructor leaves the SFML default 30, but
    /// `refreshCamera` sets 14 for BOTH aspects, MenuGame.cpp:3229/:3237 —
    /// the port's refresh mirrors that on the first draw, as the C++'s
    /// constructor ends with `refreshCamera()`). The level author lines,
    /// the difficulty / pack / music detail lines, the loading labels.
    txt_selection_small: Text,
    /// C++ `txtSelectionScore` (`openSquare`, fixed char size 28 — its
    /// *height* feeds the leaderboard line spacing; the C++ never renders
    /// the text itself, so the `Text` is constructed but untouched).
    #[allow(dead_code)] // C++ parity field (ctor :224); never rendered
    txt_selection_score: Text,
    /// C++ `txtSelectionRanked` (`openSquareBold`, `selectionRanked` char
    /// size — the "RANKED" badge).
    txt_selection_ranked: Text,
    txt_online_status: Text,
    /// C++ `HexagonDialogBox::txtDialog` (created at char size 0; the
    /// bridge applies `dialog.char_size()` before the text pass).
    txt_dialog: Text,

    // ---- (c) the profile / entering-text fonts (MenuGame.cpp:196-207) ----
    /// C++ `txtProfile` (`openSquare`, fixed char size 32 — the boot-time
    /// name width probes; the rows render at `profCharSize` 30 / 42).
    txt_profile: Text,
    /// C++ `txtInstructionsBig` (`openSquare`, fixed char size 46 — the
    /// boot states' instructions).
    txt_instructions_big: Text,
    /// C++ `txtInstructionsMedium` (`openSquare` — `refreshCamera` never
    /// sets it; it keeps the SFML `sf::Text` default char size 30, the
    /// D-014 note in `oh_menu::layout::char_sizes`).
    txt_instructions_medium: Text,
    /// C++ `txtInstructionsSmall` (`openSquare`, fixed char size 20 — the
    /// profile-deletion instructions).
    txt_instructions_small: Text,
    /// C++ `txtEnteringText` (`openSquare`, fixed char size 54 — the
    /// entered-name text).
    txt_entering_text: Text,

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
    /// C++ `epilepsyWarning` (`epilepsyWarning.png`; the EpilepsyWarning
    /// arm draws it — the transform is set on refresh, :3157-3160).
    epilepsy_warning: Sprite,
    /// C++ `rsOnlineStatus` (default size `{128, 32}`).
    rs_online_status: RectangleShape,

    // ---- C++ `MenuFont::height` caches (updateHeight, :3236-3244) -------
    /// The `txtMenuBig` glyph height at the current char size (C++
    /// `txtMenuBig.height` — the main-menu quad math).
    h_menu_big: f32,
    /// The `txtMenuSmall` glyph height at the current char size (C++
    /// `txtMenuSmall.height` — the options/online submenu quad math).
    h_menu_small: f32,
    #[allow(dead_code)]
    // C++ parity probe (`updateHeight` :3238); no ported draw reads `txtMenuTiny.height`
    h_menu_tiny: f32,
    /// The (b) fonts' heights (the same probe — each font's `fontHeight`
    /// at its current char size, C++ `updateHeight` loop :3236-3244).
    h_load_small: f32,
    h_load_big: f32,
    h_random_tip: f32,
    h_selection_big: f32,
    h_selection_medium: f32,
    h_selection_small: f32,
    h_selection_score: f32,
    h_selection_ranked: f32,
    /// The `txtProf` glyph height at its fixed char size 18 (the boot
    /// states' "PRESS ANY KEY" line, MenuGame.cpp:5832/:5839).
    h_prof: f32,
    /// The (c) font heights (the same probe — each at its fixed char size
    /// — `txtInstructionsBig` 46, `txtInstructionsMedium` 30, the SFML
    /// default, `txtInstructionsSmall` 20, `txtEnteringText` 54).
    h_instructions_big: f32,
    h_instructions_medium: f32,
    h_instructions_small: f32,
    h_entering_text: f32,
    /// The `txtProfile` font heights at `profCharSize` (30) / 42 (C++
    /// `Utils::getFontHeight(txtProfile.font, ...)`, :3891-3893, :4016-
    /// :4018) — the profile-row line math.
    h_prof_char: f32,
    h_prof_selected: f32,

    // ---- C++ `updateLevelSelectionDrawingParameters` (:4298-4312) --------
    // Computed on refresh (the C++ `refreshCamera` calls it right after the
    // `updateHeight` loop, :3246-3249) from the (b) font heights.
    /// C++ `textToQuadBorder` (`txtSelectionMedium.height * frameSizeMulti`).
    text_to_quad_border: f32,
    /// C++ `slctFrameSize` (`textToQuadBorder * 0.3`).
    slct_frame_size: f32,
    /// C++ `packLabelHeight`.
    pack_label_height: f32,
    /// C++ `levelLabelHeight`.
    level_label_height: f32,

    // ---- C++ `namesScroll` (MenuGame.hpp:409) — draw-mutated ------------
    /// C++ `namesScroll[Label::ScrollsSize]` — the level-selection left
    /// panel's per-label marquee scroll (LevelName, PackName, PackAuthor,
    /// MusicName, MusicAuthor, MusicAlbum).
    /// (The (b) level-selection left panel reads it; (a) only resets it.)
    #[allow(dead_code)] // lands with the 13b(b) level selection
    names_scroll: [f32; 6],

    // ---- C++ `levelDescription` (MenuGame.hpp:408) — draw-cached --------
    /// The `formatLevelDescription` output (MenuGame.cpp:4678-4775), keyed
    /// by `(level id, w)` — the C++ recomputes on `setIndex` (:2950) and on
    /// `refreshCamera` (:3252-3255, `w`-dependent via
    /// `getMaximumTextWidth` :4673-4675); the bridge recomputes lazily when
    /// the key changes (a level change or a resize).
    desc_key: Option<(String, f32)>,
    desc_lines: Vec<String>,

    // ---- stale-refresh tracking (C++ `mustRefresh`, :5758) --------------
    /// `(w, h, four_by_three, char_sizes)` at the last refresh.
    last_refresh: Option<(f32, f32, f32, f32, f32)>,

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
    font_bold: Font,
}

impl MenuDraw {
    /// Build the render state for `MenuGame::draw`. `font` is the C++
    /// `openSquare` (`OpenSquare-Regular.ttf`, MenuGame.cpp:162); the bold
    /// variant is the C++ `openSquareBold` (the bold-only fonts
    /// `txtLoadSmall` / `txtSelectionBig` / `txtSelectionMedium` /
    /// `txtSelectionRanked`, :206-:224).
    #[must_use]
    pub fn new(font: &Font, font_bold: &Font) -> Self {
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
            // `txtVersion` 40, `txtProf` 18, `txtOnlineStatus` 24, the (b)
            // fixed sizes `txtSelectionMedium` 19 / `txtSelectionScore` 28;
            // `txtSelectionSmall` keeps the SFML default (30).
            txt_version: Text::new(font.clone(), String::new(), 40),
            txt_prof: Text::new(font.clone(), String::new(), 18),
            txt_menu_big: Text::new(font.clone(), String::new(), char_sizes.menu_big as u32),
            // `txtMenuSmall` / `txtMenuTiny` keep the SFML default 30 in the
            // C++ constructor (:212-213) — `refreshCamera` (the first draw's
            // refresh) replaces it with the aspect size, as for `txtMenuBig`.
            txt_menu_small: Text::new(font.clone(), String::new(), char_sizes.menu_small as u32),
            txt_menu_tiny: Text::new(font.clone(), String::new(), char_sizes.menu_tiny as u32),
            txt_load_small: Text::new(
                font_bold.clone(),
                String::new(),
                char_sizes.load_small as u32,
            ),
            txt_load_big: Text::new(font.clone(), String::new(), char_sizes.load_big as u32),
            txt_random_tip: Text::new(font.clone(), String::new(), char_sizes.random_tip as u32),
            txt_selection_big: Text::new(
                font_bold.clone(),
                String::new(),
                char_sizes.selection_big as u32,
            ),
            txt_selection_medium: Text::new(font_bold.clone(), String::new(), 19),
            txt_selection_small: Text::new(font.clone(), String::new(), 30),
            txt_selection_score: Text::new(font.clone(), String::new(), 28),
            txt_selection_ranked: Text::new(
                font_bold.clone(),
                String::new(),
                char_sizes.selection_ranked as u32,
            ),
            txt_online_status: Text::new(font.clone(), String::new(), 24),
            txt_dialog: Text::new(font.clone(), String::new(), 0),
            txt_profile: Text::new(font.clone(), String::new(), 32),
            txt_instructions_big: Text::new(font.clone(), String::new(), 46),
            txt_instructions_medium: Text::new(font.clone(), String::new(), 30),
            txt_instructions_small: Text::new(font.clone(), String::new(), 20),
            txt_entering_text: Text::new(font.clone(), String::new(), 54),
            title_bar: Sprite::new(placeholder.clone()),
            credits_bar1: Sprite::new(placeholder.clone()),
            credits_bar2: Sprite::new(placeholder.clone()),
            s_online: Sprite::new(placeholder.clone()),
            epilepsy_warning: Sprite::new(placeholder),
            rs_online_status: rs,
            h_menu_big: 0.0, // probed on the first refresh
            h_menu_small: 0.0,
            h_menu_tiny: 0.0,
            h_load_small: 0.0,
            h_load_big: 0.0,
            h_random_tip: 0.0,
            h_selection_big: 0.0,
            h_selection_medium: 0.0,
            h_selection_small: 0.0,
            h_selection_score: 0.0,
            h_selection_ranked: 0.0,
            h_prof: 0.0,
            h_instructions_big: 0.0,
            h_instructions_medium: 0.0,
            h_instructions_small: 0.0,
            h_entering_text: 0.0,
            h_prof_char: 0.0,
            h_prof_selected: 0.0,
            text_to_quad_border: 0.0,
            slct_frame_size: 0.0,
            pack_label_height: 0.0,
            level_label_height: 0.0,
            names_scroll: [0.0; 6],
            desc_key: None,
            desc_lines: Vec::new(),
            last_refresh: None,
            last_state: States::LoadingScreen,
            last_pack_sig: 0,
            last_level_index: 0,
            font: font.clone(),
            font_bold: font_bold.clone(),
        }
    }

    /// The C++ `mustRefresh` block (MenuGame.cpp:5758-5765) — re-apply the
    /// camera / char-size / offset setup when the layout inputs changed
    /// since the last draw (or on the very first draw).
    ///
    /// issue 025g — the staleness inputs are the `refresh_camera` inputs:
    /// the ACTUAL window size (`menu.window_size()` — the C++
    /// `getWindowWidth()` / `getWindowHeight()`, MenuGame.cpp:3131-3136 —
    /// the window resize / recreation is what the C++'s `onRecreation`
    /// half of `mustRefresh` reacts to) AND the config size / zoom (the
    /// `backgroundCamera` view's input, MenuGame.cpp:3138-3140 — the port
    /// does not recreate the window on a config resolution change, so a
    /// config-only change must fire the refresh too; the C++ fires it via
    /// the window recreation).
    fn refresh_if_stale(&mut self, menu: &MenuGame) {
        let shared = menu.shared();
        let (win_w, win_h) = menu.window_size();
        let (size_x, size_y, zoom) = {
            let s = shared.borrow();
            let c = s.config();
            (c.size_x(), c.size_y(), c.zoom_factor())
        };
        let stale = match self.last_refresh {
            Some((lw, lh, lsx, lsy, lzoom)) => {
                lw != win_w || lh != win_h || lsx != size_x || lsy != size_y || lzoom != zoom
            }
            None => true,
        };
        if !stale {
            return;
        }
        self.last_refresh = Some((win_w, win_h, size_x, size_y, zoom));

        // C++ :5761 `refreshCamera()` (the port's `refresh_camera` also
        // resets the five menus' category offsets, MenuGame.cpp:3189-3204)
        // — issue 025g: it runs on the ACTUAL window size (the C++'s
        // `getWindowWidth()` / `getWindowHeight()`), so the derived values
        // are re-read AFTER the call (pre-call they are stale when the
        // window size changed).
        menu.refresh_camera();
        let (w, h, fbt, sizes) = {
            let s = shared.borrow();
            (s.w, s.h, s.four_by_three, s.char_sizes)
        };

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
        // `overlayCamera.setView(sf::View{sf::FloatRect({0, 0}, {w, h})})` —
        // the SFML `View(FloatRect)` ctor centres the view at the rect's
        // centre, so the overlay view is centre `(w/2, h/2)`, size `(w, h)`:
        // a top-left-origin mapping of the menu's `[0, w] × [0, h]` layout.
        // (The constructor's config-based view is replaced by the time the
        // constructor finishes, since it ends with `refreshCamera()`.)
        let cam_size = Vec2::new(size_x * zoom, size_y * zoom);
        self.background_camera
            .set_view(View::new(Vec2::new(0.0, 0.0), cam_size));
        self.background_camera
            .set_skew(Vec2::new(1.0, if fbt { 0.8 } else { 0.6 }));
        self.overlay_camera
            .set_view(View::new(Vec2::new(w / 2.0, h / 2.0), Vec2::new(w, h)));

        // C++ `refreshCamera`'s char-size half (:3207-3244): set the
        // aspect-dependent sizes on the fonts + `updateHeight` the cached
        // heights. (a) caches `txtMenuBig.height`; (b) adds the
        // loading-screen / level-selection fonts.
        self.txt_menu_big.set_character_size(sizes.menu_big as u32);
        self.h_menu_big = font_height(&self.font, sizes.menu_big as u32);
        // C++ :3208-3209 / :3223-3224 — the aspect-dependent submenu sizes;
        // the `updateHeight` loop (:3238) probes both.
        self.txt_menu_small
            .set_character_size(sizes.menu_small as u32);
        self.h_menu_small = font_height(&self.font, sizes.menu_small as u32);
        self.txt_menu_tiny
            .set_character_size(sizes.menu_tiny as u32);
        self.h_menu_tiny = font_height(&self.font, sizes.menu_tiny as u32);
        self.txt_load_small
            .set_character_size(sizes.load_small as u32);
        self.h_load_small = font_height(&self.font_bold, sizes.load_small as u32);
        self.txt_load_big.set_character_size(sizes.load_big as u32);
        self.h_load_big = font_height(&self.font, sizes.load_big as u32);
        self.txt_random_tip
            .set_character_size(sizes.random_tip as u32);
        self.h_random_tip = font_height(&self.font, sizes.random_tip as u32);
        self.txt_selection_big
            .set_character_size(sizes.selection_big as u32);
        self.h_selection_big = font_height(&self.font_bold, sizes.selection_big as u32);
        // C++ :3229/:3237 — 14 for BOTH aspects (the constructor's SFML
        // default 30 is replaced by the time the C++ ctor finishes, since
        // it ends with `refreshCamera()`).
        self.txt_selection_small
            .set_character_size(sizes.selection_small as u32);
        self.h_selection_small = font_height(&self.font, sizes.selection_small as u32);
        // `txtSelectionMedium` (19) / `txtSelectionScore` (28) are fixed
        // constructor sizes — `refreshCamera` only re-runs `updateHeight`
        // on them, never `setCharacterSize`.
        self.h_selection_medium = font_height(&self.font_bold, 19);
        self.h_selection_score = font_height(&self.font, 28);
        self.h_prof = font_height(&self.font, 18);
        // (c) the profile / entering-text fonts — all fixed constructor
        // sizes (only `refreshCamera`'s `updateHeight` re-probes them):
        self.txt_profile.set_character_size(sizes.profile as u32);
        self.txt_instructions_big
            .set_character_size(sizes.instructions_big as u32);
        self.h_instructions_big = font_height(&self.font, sizes.instructions_big as u32);
        // `instructions_medium` keeps the SFML default 30 (the D-014 note
        // in `layout::char_sizes`) — still probed.
        self.txt_instructions_medium
            .set_character_size(sizes.instructions_medium as u32);
        self.h_instructions_medium = font_height(&self.font, sizes.instructions_medium as u32);
        self.txt_instructions_small
            .set_character_size(sizes.instructions_small as u32);
        self.h_instructions_small = font_height(&self.font, sizes.instructions_small as u32);
        self.txt_entering_text
            .set_character_size(sizes.entering_text as u32);
        self.h_entering_text = font_height(&self.font, sizes.entering_text as u32);
        // The profile-row line math (C++ `getFontHeight(txtProfile.font,
        // profCharSize)` / `profSelectedCharSize`, :3891-3893, :4016-4018).
        self.h_prof_char = font_height(&self.font, 30);
        self.h_prof_selected = font_height(&self.font, 42);
        self.txt_selection_ranked
            .set_character_size(sizes.selection_ranked as u32);
        self.h_selection_ranked = font_height(&self.font_bold, sizes.selection_ranked as u32);

        // C++ `refreshCamera` → `updateLevelSelectionDrawingParameters`
        // (:3246-3249, :4298-4312) — after the `updateHeight` loop:
        self.text_to_quad_border = self.h_selection_medium * layout::FRAME_SIZE_MULTI;
        self.slct_frame_size = self.text_to_quad_border * 0.3;
        self.pack_label_height =
            self.h_selection_medium + 2.0 * self.text_to_quad_border + self.slct_frame_size;
        self.level_label_height = self.h_selection_big
            + self.h_selection_small
            + self.text_to_quad_border
            + 2.0 * self.text_to_quad_border
            - self.slct_frame_size;
        // C++ `refreshCamera` → `formatLevelDescription` (:3252-3255) —
        // invalidate the cached lines (the left panel recomputes lazily).
        self.desc_key = None;

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
///
/// issue 025h — SFML's `Text::getGlobalBounds()` (the C++ `updateHeight`
/// probe, MenuGame.cpp:3236-3244) is the INK union of the glyph outline
/// bounds (each glyph's `bounds`, baseline-relative) — NOT the line
/// metrics: for openSquare the probe's ink union is ≈0.64× the char size
/// while `ascent − descent` is exactly 1.0×. The port's stale line-height
/// probe (`Text::measure()`'s `h = ascent - descent`) ran the C++ layout
/// math (e.g. `numbersHeight = bottomHeight − 2·txtLoadBig.height`,
/// MenuGame.cpp:4212) on the 1.0× height and pulled the load-results
/// values up onto their labels — the phone's "PACKS 8OADED" overlap
/// 025g's wide-aspect char sizes (loadBig 70) exposed.
fn font_height(font: &Font, char_size: u32) -> f32 {
    // The C++ probe string (FontHeight.cpp:13).
    const PROBE: &str = "ABCDEFGHILMNOPQRSTUVZ:";
    let px = char_size as f32;
    let mut top = f32::INFINITY;
    let mut bottom = f32::NEG_INFINITY;
    for ch in PROBE.chars() {
        let m = font.glyph_metrics(ch, px);
        // fontdue's `bounds.ymin` is the bottom-edge offset in typography
        // coordinates (y up, baseline 0); the top edge sits at
        // `ymin + height` (the glyphs.rs pin, :418-421).
        top = top.min(m.bounds.ymin);
        bottom = bottom.max(m.bounds.ymin + m.bounds.height);
    }
    bottom - top
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
/// profile / load states; (d) the `windowed.rs` wiring; last, the
/// 13b-deferred `MOpts` / `MOnline` options/online submenu draws
/// (`drawSubmenusSmall` :3606, `drawOptionsSubmenus` :3784).
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
    let dialog_box_text_color = s.dialog_box_text_color;
    let entering_text_offset = s.entering_text_offset;
    let level_details_offset = s.level_details_offset;
    let dialog = s.dialog.clone();
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
            // C++ :5830-5833 — `drawLoadResults()` + the "PRESS ANY KEY"
            // line; the C++ then `return`s (the screenshot / dialog passes
            // are skipped — the gate at the tail of `draw` mirrors that).
            draw_load_results(app, &s, target);
            render_text(
                target,
                &mut app.txt_prof,
                "PRESS ANY KEY OR BUTTON TO CONTINUE",
                Vec2::new(app.h_prof, h - app.h_prof * 2.7 + 5.0),
            );
        }
        States::EpilepsyWarning => {
            // C++ :5836-5840 — the `epilepsyWarning` sprite (its transform
            // was set on refresh, :3157-3160) + the "PRESS ANY KEY" line;
            // the C++ then `return`s (the tail gate mirrors that).
            target.draw_sprite(&app.epilepsy_warning);
            render_text(
                target,
                &mut app.txt_prof,
                "PRESS ANY KEY OR BUTTON TO CONTINUE",
                Vec2::new(app.h_prof, h - app.h_prof * 2.7 + 5.0),
            );
        }
        States::ETLPNewBoot => {
            // C++ :5842-5845 — `drawEnteringTextBoot()` + `drawGraphics()`.
            {
                let s = menu.shared().borrow();
                draw_entering_text_boot(app, &s, target);
            }
            draw_graphics(app, target);
        }
        States::SLPSelectBoot => {
            // C++ :5847-5850 — `drawProfileSelectionBoot()` +
            // `drawGraphics()`.
            {
                let s = menu.shared().borrow();
                draw_profile_selection_boot(app, menu, &s, target);
            }
            draw_graphics(app, target);
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
                // C++ :5861-5863 — `drawProfileSelection(profileIndent,
                // true)`.
                let shared = menu.shared().borrow();
                draw_profile_selection(app, menu, &shared, profile_indent, true, target);
            }
            if entering_text_offset != 0.0 {
                // C++ :5866-5868 — `drawEnteringText(profileIndent, true)`
                // (the C++'s `enteringTextOffset` member is eased in
                // place — the `RefMut` lives for the call).
                let mut shared = menu.shared().borrow_mut();
                draw_entering_text(app, &mut shared, profile_indent, true, target);
            }
            if level_details_offset != 0.0 {
                // C++ :5871-5874 — `drawLevelSelectionRightSide(*lvlDrawer,
                // true)` + `drawLevelSelectionLeftSide(*lvlDrawer, true)`
                // (the folding level-selection panels behind the main
                // menu; `*lvlDrawer` is the active drawer).
                {
                    let mut shared = menu.shared().borrow_mut();
                    let reads = LevelSelReads::take(&shared, fps_mult);
                    let assets = shared.assets().clone();
                    let network = shared.network().clone();
                    let favorites_active = shared.favorites_active;
                    if favorites_active {
                        draw_level_selection_right_side(
                            app,
                            &mut shared.fav_slct,
                            &reads,
                            &assets,
                            &network,
                            true,
                            target,
                        );
                    } else {
                        draw_level_selection_right_side(
                            app,
                            &mut shared.lvl_slct,
                            &reads,
                            &assets,
                            &network,
                            true,
                            target,
                        );
                    }
                    // C++ :5871 second half — `drawLevelSelectionLeftSide`
                    // (`*lvlDrawer` = the active drawer; the helper picks
                    // it from `shared` via `favorites_active`).
                    draw_level_selection_left_side(
                        app,
                        &mut shared,
                        &reads,
                        &assets,
                        &network,
                        true,
                        target,
                    );
                }
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
            // C++ :5881-5886 — fold the main menu (offset-gated).
            if menu.main_menu.get_category().get_offset() != 0.0 {
                draw_main_menu(
                    menu,
                    app,
                    &menu.main_menu.get_category_by_name("main"),
                    w - indent_big,
                    true,
                    menu_quad_color,
                    menu_text_color,
                    fps_mult,
                    target,
                );
            }
            // C++ :5888-5890 — the options menu (right).
            draw_main_menu(
                menu,
                app,
                &menu.options_menu.get_category_by_name("options"),
                w - indent_big,
                false,
                menu_quad_color,
                menu_text_color,
                fps_mult,
                target,
            );
            // C++ :5892-5893 — the options submenus (left).
            draw_submenus_small(
                menu,
                app,
                &menu.options_menu,
                indent_small,
                menu_quad_color,
                menu_text_color,
                fps_mult,
                target,
            );
            // C++ :5894-5895 — `drawGraphics()` + `drawOnlineStatus()`.
            draw_graphics(app, target);
            draw_online_status(menu, app, target);
        }
        States::MOnline => {
            // C++ :5899-5904 — fold the main menu (offset-gated).
            if menu.main_menu.get_category().get_offset() != 0.0 {
                draw_main_menu(
                    menu,
                    app,
                    &menu.main_menu.get_category_by_name("main"),
                    w - indent_big,
                    true,
                    menu_quad_color,
                    menu_text_color,
                    fps_mult,
                    target,
                );
            }
            // C++ :5906-5908 — the online menu (right; its only category
            // is named `"options"`, C++ :1596).
            draw_main_menu(
                menu,
                app,
                &menu.online_menu.get_category_by_name("options"),
                w - indent_big,
                false,
                menu_quad_color,
                menu_text_color,
                fps_mult,
                target,
            );
            // C++ :5910-5911 — the online submenus (left; the `"options"`
            // category is skipped by `drawSubmenusSmall`, so the left
            // column is empty — the C++'s behaviour for the single-category
            // online menu).
            draw_submenus_small(
                menu,
                app,
                &menu.online_menu,
                indent_small,
                menu_quad_color,
                menu_text_color,
                fps_mult,
                target,
            );
            // C++ :5912-5913 — `drawGraphics()` + `drawOnlineStatus()`.
            draw_graphics(app, target);
            draw_online_status(menu, app, target);
        }
        States::ETLPNew => {
            // C++ :5912-5916 — `drawMainMenu(mainMenu.
            // getCategoryByName("local profiles"), w - indentBig, false)`
            // + `drawEnteringText(profileIndent, false)` +
            // `drawGraphics()`.
            draw_main_menu(
                menu,
                app,
                &menu.main_menu.get_category_by_name("local profiles"),
                w - indent_big,
                false,
                menu_quad_color,
                menu_text_color,
                fps_mult,
                target,
            );
            {
                let mut shared = menu.shared().borrow_mut();
                draw_entering_text(app, &mut shared, profile_indent, false, target);
            }
            draw_graphics(app, target);
        }
        States::SLPSelect => {
            // C++ :5919-5923 — the `local profiles` `drawMainMenu` +
            // `drawProfileSelection(profileIndent, false)` +
            // `drawGraphics()`.
            draw_main_menu(
                menu,
                app,
                &menu.main_menu.get_category_by_name("local profiles"),
                w - indent_big,
                false,
                menu_quad_color,
                menu_text_color,
                fps_mult,
                target,
            );
            {
                let shared = menu.shared().borrow();
                draw_profile_selection(app, menu, &shared, profile_indent, false, target);
            }
            draw_graphics(app, target);
        }
        States::LevelSelection => {
            // C++ :5926-5949 — fold the main menu (offset-gated), the
            // favorite / regular drawer `drawLevelSelectionRightSide` pair
            // (F2-gated), `drawLevelSelectionLeftSide(*lvlDrawer, false)`,
            // `drawOnlineStatus()`.
            if menu.main_menu.get_category().get_offset() != 0.0 {
                draw_main_menu(
                    menu,
                    app,
                    &menu.main_menu.get_category_by_name("main"),
                    w - indent_big,
                    true,
                    menu_quad_color,
                    menu_text_color,
                    fps_mult,
                    target,
                );
            }

            // C++ :5929-5944 — the active drawer (non-reverting) + the
            // other drawer while it is still folding (reverting).
            {
                let mut shared = menu.shared().borrow_mut();
                let reads = LevelSelReads::take(&shared, fps_mult);
                let assets = shared.assets().clone();
                let network = shared.network().clone();
                if shared.favorites_active {
                    draw_level_selection_right_side(
                        app,
                        &mut shared.fav_slct,
                        &reads,
                        &assets,
                        &network,
                        false,
                        target,
                    );
                    if shared.lvl_slct.x_offset != 0.0 {
                        draw_level_selection_right_side(
                            app,
                            &mut shared.lvl_slct,
                            &reads,
                            &assets,
                            &network,
                            true,
                            target,
                        );
                    }
                } else {
                    draw_level_selection_right_side(
                        app,
                        &mut shared.lvl_slct,
                        &reads,
                        &assets,
                        &network,
                        false,
                        target,
                    );
                    if shared.fav_slct.x_offset != 0.0 {
                        draw_level_selection_right_side(
                            app,
                            &mut shared.fav_slct,
                            &reads,
                            &assets,
                            &network,
                            true,
                            target,
                        );
                    }
                }

                // C++ :5946 — `drawLevelSelectionLeftSide(*lvlDrawer, false)`
                // (`*lvlDrawer` = the active drawer; the helper picks it
                // from `shared` via `favorites_active`).
                draw_level_selection_left_side(
                    app,
                    &mut shared,
                    &reads,
                    &assets,
                    &network,
                    false,
                    target,
                );
            }

            // C++ :5948.
            draw_online_status(menu, app, target);
        }
    }

    // C++ :5974-5978 — `mustTakeScreenshot` (module docs: always false in
    // the port).

    // C++ :5980-5984 — the dialog box pass. The C++ `return`s out of the
    // `LoadingScreen` / `EpilepsyWarning` arms (:5833, :5840) — the
    // screenshot / dialog passes are skipped for those states.
    let backdrop = style_bg
        .as_ref()
        .and_then(|bg| bg.colors.first().copied())
        .unwrap_or(Color::BLACK);
    if !dialog.empty() && !matches!(state, States::LoadingScreen | States::EpilepsyWarning) {
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
    dialog_box_text_color: Color,
    entering_text_offset: f32,
    level_details_offset: f32,
    dialog: HexagonDialogBox,
    profile_name: String,
    missing_dependencies: Vec<String>,
    black_and_white: bool,
    // ---- (b) loading-screen reads (C++ `loadInfo` / `randomTip` /
    // `hexagonRotation` / `scrollbarOffset`) ----
    load: LoadResults,
    random_tip: [String; 2],
    hexagon_rotation: f32,
    scrollbar_offset: i32,
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
            dialog_box_text_color: s.dialog_box_text_color,
            entering_text_offset: s.entering_text_offset,
            level_details_offset: s.level_details_offset,
            dialog: s.dialog.clone(),
            profile_name: s.assets().current_profile_name().unwrap_or_default(),
            missing_dependencies: s.assets().pack_ids_with_missing_dependencies(),
            black_and_white: s.config().black_and_white(),
            load: s.assets().load_results(),
            random_tip: s.random_tip.clone(),
            hexagon_rotation: s.hexagon_rotation,
            scrollbar_offset: s.scrollbar_offset,
        }
    }
}

// ---------------------------------------------------------------------------
// `drawScrollbar` (MenuGame.cpp:3568-3585)
// ---------------------------------------------------------------------------

/// C++ `MenuGame::drawScrollbar(totalHeight, size, notches, x, y, color)`
/// (MenuGame.cpp:3568-3585) — the notch-math pinned by the (b) tests:
/// `notchHeight = totalHeight / size; barHeight = totalHeight - notches *
/// notchHeight; startHeight = y + notchHeight * scrollbarOffset` (the
/// C++'s member `scrollbarOffset`).
#[allow(clippy::too_many_arguments)] // the C++ members threaded through the bridge
fn draw_scrollbar(
    app: &mut MenuDraw,
    total_height: f32,
    size: i32,
    notches: i32,
    x: f32,
    y: f32,
    scrollbar_offset: i32,
    color: Color,
    target: &mut dyn DrawTarget,
) {
    let notch_height = total_height / size as f32;
    let bar_height = total_height - notches as f32 * notch_height;
    let start_height = y + notch_height * scrollbar_offset as f32;
    app.menu_quads.clear();
    app.menu_quads.reserve_quad(1);
    create_quad(
        &mut app.menu_quads,
        color,
        x,
        x + app.text_to_quad_border,
        start_height,
        start_height + bar_height,
    );
    target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());
}

// ---------------------------------------------------------------------------
// `scrollName` / `scrollNameRightBorder` (MenuGame.cpp:4320-4378)
// ---------------------------------------------------------------------------

/// C++ `MenuGame::scrollName(text, scroller)` (MenuGame.cpp:4320-4332):
/// FPS-consistent marquee — `scroller += getFPSMult(); text += "  "`, then
/// rotate left by `getMod(int(scroller / 100), text.length())` chars
/// (`ssvu::toInt` = truncation; the scroller is always ≥ 0 here, so the
/// `as i32` cast matches).
fn scroll_name(text: &mut String, scroller: &mut f32, fps_mult: f32) {
    *scroller += fps_mult;
    text.push_str("  ");
    let shift = layout::mod_range((*scroller / 100.0) as i32, 0, text.len() as i32) as usize;
    // The C++ advances `shift` BYTES (`std::next` on a `std::string`);
    // the port floors to a char boundary so a multi-byte shift can never
    // panic (ASCII names: identical).
    let mut shift = shift;
    while shift > 0 && !text.is_char_boundary(shift) {
        shift -= 1;
    }
    if shift > 0 {
        let rotated = text[shift..].to_string() + &text[..shift];
        *text = rotated;
    }
}

/// C++ `MenuGame::scrollNameRightBorder(text, key, font, scroller, border)`
/// (MenuGame.cpp:4334-4358): the labelled marquee — measure `key`,
/// uppercasify, and if `key + text` fits the border keep it; else scroll
/// the (keyless, uppercased) text, truncate it to the border, then
/// re-prepend the key.
fn scroll_name_right_border(
    text: &mut String,
    key: &str,
    txt: &mut Text,
    scroller: &mut f32,
    border: f32,
    fps_mult: f32,
) {
    txt.set_string(key);
    let key_width = txt.get_local_bounds().get_width();
    uppercasify(text);
    txt.set_string(text.as_str());
    let border = border - key_width;
    if txt.get_local_bounds().get_width() <= border {
        *text = format!("{key}{text}");
        return;
    }
    scroll_name(text, scroller, fps_mult);
    txt.set_string(text.as_str());
    while txt.get_local_bounds().get_width() > border && text.len() > 1 {
        text.pop();
        txt.set_string(text.as_str());
    }
    *text = format!("{key}{text}");
}

/// C++ `MenuGame::scrollNameRightBorder(text, font, scroller, border)`
/// (the keyless overload, MenuGame.cpp:4360-4378).
fn scroll_name_right_border_no_key(
    text: &mut String,
    txt: &mut Text,
    scroller: &mut f32,
    border: f32,
    fps_mult: f32,
) {
    uppercasify(text);
    txt.set_string(text.as_str());
    if txt.get_local_bounds().get_width() <= border {
        return;
    }
    scroll_name(text, scroller, fps_mult);
    txt.set_string(text.as_str());
    while txt.get_local_bounds().get_width() > border && text.len() > 1 {
        text.pop();
        txt.set_string(text.as_str());
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
// The options/online submenu draws (C++ `drawSubmenusSmall` :3606,
// `drawOptionsSubmenus` :3784)
// ---------------------------------------------------------------------------

/// C++ `MenuGame::drawSubmenusSmall(subMenus, indent)`
/// (MenuGame.cpp:3606-3630) — the left-hand options/online submenu column.
/// Walks `subMenus`' categories: the `"options"` category is skipped
/// ("Already drawn before this loop" — the right column drew it via
/// `drawMainMenu`), and each remaining category draws when it is the
/// current one (C++ `optionsMenu.getCategory()` — read through the
/// `options_menu` member exactly as the C++ reads its `optionsMenu`
/// member) or is mid-fold (`getOffset() != 0`).
#[allow(clippy::too_many_arguments)] // the C++ members threaded through the bridge
fn draw_submenus_small(
    menu: &MenuGame,
    app: &mut MenuDraw,
    sub_menu: &Menu,
    indent: f32,
    menu_quad_color: Color,
    menu_text_color: Color,
    fps_mult: f32,
    target: &mut dyn DrawTarget,
) {
    // C++ `optionsMenu.getCategory().getName()` (the port reads the same
    // member — `menu.options_menu` — even in the MOnline arm, where
    // `sub_menu` is the online menu).
    let current_name = menu.options_menu.get_category().get_name().to_string();
    // C++ `menuHalfHeight` — set by the `drawMainMenu` call the MOpts /
    // MOnline arms run right before this helper (the port stores it in
    // `Shared`, as `draw_main_menu` does).
    let menu_half_height = menu.shared().borrow().menu_half_height;
    for c in sub_menu.get_categories() {
        // C++ :3613-3617 — "Already drawn before this loop".
        if c.get_name() == "options" {
            continue;
        }
        let currently_selected = current_name == c.get_name();
        let has_offset = c.get_offset() != 0.0;
        // C++ :3622-3626 — "this submenu has been fully folded".
        if !currently_selected && !has_offset {
            continue;
        }
        draw_options_submenus(
            app,
            &c,
            indent,
            !currently_selected && has_offset,
            menu_half_height,
            menu_quad_color,
            menu_text_color,
            fps_mult,
            target,
        );
    }
}

/// C++ `MenuGame::drawOptionsSubmenus(mSubMenu, baseIndent, revertOffset)`
/// (MenuGame.cpp:3784-3844) — one left submenu panel: the two backing
/// quads (text-coloured frame + quad-coloured body) and the uppercased
/// item names (the selected one `"> "`-prefixed, disabled ones greyed and
/// badged with "[OFFICIAL MODE ENABLED]").
#[allow(clippy::too_many_arguments)] // the C++ members threaded through the bridge
fn draw_options_submenus(
    app: &mut MenuDraw,
    m_sub_menu: &Category,
    base_indent: f32,
    revert_offset: bool,
    menu_half_height: f32,
    menu_quad_color: Color,
    menu_text_color: Color,
    fps_mult: f32,
    target: &mut dyn DrawTarget,
) {
    let items = m_sub_menu.get_items();
    let size = items.len();

    // C++ "Calculate quads coordinates" (:3790-3797).
    let h_menu_small = app.h_menu_small;
    let mut quad_border = h_menu_small * layout::FRAME_SIZE_MULTI;
    let double_border = quad_border * 2.0;
    let interline = 2.1 * h_menu_small;
    let total_height = interline * (size as f32 - 1.0) + 2.0 * double_border + h_menu_small;
    // C++ `std::max(menuHalfHeight - totalHeight / 2.f, ssvs::getGlobalBottom
    // (creditsBar2) + 10.f)` (the placeholder texture shifts the value by
    // the real-texture-size delta — the module docs note).
    let credits_bottom = app.credits_bar2.get_global_bounds().get_bottom();
    let quad_height = (menu_half_height - total_height / 2.0).max(credits_bottom + 10.0);

    // C++ "Offset" (:3799-3802) — the 4-arg `calcMenuOffset` with
    // `speedUp = true` (the header default is `false`, :307-308).
    let mut offset = m_sub_menu.get_offset();
    let panel_offset =
        layout::calc_menu_offset(&mut offset, base_indent, revert_offset, true, fps_mult);
    *m_sub_menu.get_offset_mut() = offset;
    let indent = base_indent - quad_border - panel_offset;

    // C++ "Draw the quads that surround the text" (:3804-3812).
    app.menu_quads.clear();
    app.menu_quads.reserve_quad(2);
    create_quad(
        &mut app.menu_quads,
        menu_text_color,
        0.0,
        indent + double_border,
        quad_height,
        quad_height + total_height,
    );
    create_quad(
        &mut app.menu_quads,
        menu_quad_color,
        0.0,
        indent + quad_border,
        quad_height + quad_border,
        quad_height + total_height - quad_border,
    );
    target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());

    // C++ "Draw the text on top of the quads" (:3814-3843).
    quad_border = quad_border * 1.5 - panel_offset;
    let mut txt_height = quad_height - h_menu_small * layout::FONT_HEIGHT_OFFSET + double_border;
    for (i, item) in items.iter().enumerate() {
        let item_name = {
            let b = item.borrow();
            let mut name = b.get_name();
            // C++ `Utils::uppercasify(itemName)` (:3824).
            uppercasify(&mut name);
            if i as i32 == m_sub_menu.get_idx() {
                name = format!("> {name}");
            }
            name
        };
        let enabled = item.borrow().is_enabled();
        let c = if enabled {
            menu_text_color
        } else {
            Color::new(150, 150, 150, 255)
        };
        render_text_color(
            target,
            &mut app.txt_menu_small,
            &item_name,
            Vec2::new(quad_border, txt_height),
            c,
        );

        // C++ :3834-3840 — the disabled-item badge, positioned from the
        // just-rendered item text's global bounds (`getGlobalRight/
        // getGlobalTop(txtMenuSmall.font) + 6.f / - 2.f`).
        if !enabled {
            let bounds = app.txt_menu_small.get_global_bounds();
            render_text_color(
                target,
                &mut app.txt_menu_tiny,
                "[OFFICIAL MODE ENABLED]",
                Vec2::new(bounds.get_right() + 6.0, bounds.get_top() - 2.0),
                Color::new(150, 150, 150, 255),
            );
        }

        txt_height += interline;
    }
}

// ---------------------------------------------------------------------------
// The level-selection draw surface (C++ `drawLevelSelectionRightSide`
// :4957, `drawLevelSelectionLeftSide` :5306, `drawLoadResults` :4170)
// ---------------------------------------------------------------------------

/// The `Shared` values the level-selection draws read, snapshotted while
/// the caller holds the drawer field borrow (the C++ reads `MenuGame`
/// members directly; the port cannot hold a `Ref`/`RefMut` across a
/// `&mut LevelDrawer` field borrow, so every other read is a snapshot).
struct LevelSelReads {
    w: f32,
    h: f32,
    fps_mult: f32,
    focus_held: bool,
    pack_change_state: PackChange,
    pack_change_offset: f32,
    diff_mult_idx: i32,
    favorites_active: bool,
    is_level_favorite: bool,
    difficulty_bump_effect: f32,
    menu_quad_color: Color,
    menu_text_color: Color,
    menu_selection_color: Color,
}

impl LevelSelReads {
    fn take(shared: &Shared, fps_mult: f32) -> Self {
        Self {
            w: shared.w,
            h: shared.h,
            fps_mult,
            focus_held: shared.focus_held,
            pack_change_state: shared.pack_change_state,
            pack_change_offset: shared.pack_change_offset,
            diff_mult_idx: shared.diff_mult_idx,
            favorites_active: shared.favorites_active,
            is_level_favorite: shared.is_level_favorite,
            difficulty_bump_effect: shared.difficulty_bump_effect,
            menu_quad_color: shared.menu_quad_color,
            menu_text_color: shared.menu_text_color,
            menu_selection_color: shared.menu_selection_color,
        }
    }
}

/// C++ `MenuGame::drawLoadResults()` (MenuGame.cpp:4170-4296) — the boot
/// load-results screen: the rotating hexagon, the three vertical
/// separators, the PACKS / LEVELS / ASSETS counters, the two random tip
/// lines, and the error block (with the red scrollbar past
/// `maxErrorsOnScreen`).
fn draw_load_results(app: &mut MenuDraw, s: &SharedSnapshot, target: &mut dyn DrawTarget) {
    let w = s.w;
    let h = s.h;

    // C++ :4172-4194 — the hexagon (six orbit-cornered quads, white).
    let div = oh_math::ssvu::math::TAU / 6.0 * 0.5;
    let hexagon_radius = 100.0;
    let center_pos = Vec2::new(w / 2.0, h / 5.0);

    app.menu_quads.clear();
    app.menu_quads.reserve_quad(6);
    for i in 0..6 {
        let s_angle = div * 2.0 * (i as f32 + s.hexagon_rotation);
        let nw = get_orbit_rad(&center_pos, s_angle - div, hexagon_radius);
        let ne = get_orbit_rad(&center_pos, s_angle + div, hexagon_radius);
        let se = get_orbit_rad(&center_pos, s_angle + div, hexagon_radius + 10.0);
        let sw = get_orbit_rad(&center_pos, s_angle - div, hexagon_radius + 10.0);
        // C++ `batch_unsafe_emplace_back_quad(White, nw, sw, se, ne)` —
        // four orbit points, not axis-aligned, so the batch primitive
        // directly (the scalar `createQuad` only builds axis-aligned quads).
        app.menu_quads.push_quad(Color::WHITE, nw, sw, se, ne);
    }

    // C++ :4196-4205 — the vertical separators (x = w/2, w/4 offsets).
    app.menu_quads.reserve_more_quad(3);
    let x_offset = w / 4.0;
    let mut top_height = h / 2.0 - h / 15.0;
    let bottom_height = h / 2.0 + h / 15.0;
    for i in -1..2 {
        create_quad(
            &mut app.menu_quads,
            Color::WHITE,
            w / 2.0 + i as f32 * x_offset - 5.0,
            w / 2.0 + i as f32 * x_offset + 5.0,
            top_height,
            bottom_height,
        );
    }
    target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());

    // C++ :4207-4242 — the counters (white; the C++ sets the fills here).
    top_height += 5.0 - app.h_load_small;
    let numbers_height = bottom_height - app.h_load_big * 2.0;
    app.txt_load_small.set_fill_color(Color::WHITE);
    app.txt_load_big.set_fill_color(Color::WHITE);

    // 1
    let text_offset = w - 3.0 * x_offset;
    render_text_centered(
        target,
        &mut app.txt_load_small,
        "PACKS LOADED",
        Vec2::new(text_offset / 2.0, top_height),
    );
    render_text_centered(
        target,
        &mut app.txt_load_big,
        &s.load.packs.to_string(),
        Vec2::new(text_offset / 2.0, numbers_height),
    );
    // 2
    let text_offset = w - x_offset;
    render_text_centered(
        target,
        &mut app.txt_load_small,
        "LEVELS LOADED",
        Vec2::new(text_offset / 2.0, top_height),
    );
    render_text_centered(
        target,
        &mut app.txt_load_big,
        &s.load.levels.to_string(),
        Vec2::new(text_offset / 2.0, numbers_height),
    );
    // 3
    let text_offset = w + x_offset;
    render_text_centered(
        target,
        &mut app.txt_load_small,
        "ASSETS LOADED",
        Vec2::new(text_offset / 2.0, top_height),
    );
    render_text_centered(
        target,
        &mut app.txt_load_big,
        &s.load.assets.to_string(),
        Vec2::new(text_offset / 2.0, numbers_height),
    );

    // C++ :4244-4253 — the random tip (two lines).
    let tip_interline = app.h_random_tip * 1.5;
    let mut height = h - tip_interline * 2.0;
    for i in [1, 0] {
        // C++ `for(int i{1}; i >= 0; --i)`.
        render_text_centered(
            target,
            &mut app.txt_random_tip,
            &s.random_tip[i],
            Vec2::new(w / 2.0, height - tip_interline),
        );
        height -= tip_interline;
    }

    // C++ :4255-4296 — the error block (red).
    let size = s.load.error_messages.len() as i32;
    let text_offset = w + 3.0 * x_offset;
    render_text_centered_color(
        target,
        &mut app.txt_load_small,
        "ERRORS",
        Vec2::new(text_offset / 2.0, top_height),
        Color::RED,
    );
    render_text_centered_color(
        target,
        &mut app.txt_load_big,
        &size.to_string(),
        Vec2::new(text_offset / 2.0, numbers_height),
        Color::RED,
    );

    // No error messages.
    if size == 0 {
        let bottom = bottom_height + app.h_load_small * 1.75;
        render_text_centered_color(
            target,
            &mut app.txt_load_small,
            "NO LOAD ERRORS",
            Vec2::new(w / 2.0, bottom),
            Color::WHITE,
        );
        return;
    }

    // Print errors from last to first.
    let mut bottom_height = bottom_height + app.h_load_small / 2.0;
    let txt_spacing = app.h_load_small * 2.0;

    // But first handle the scrollbar.
    let (scrollbar_notches, drawn_size) =
        layout::scrollbar_notches(size, layout::MAX_ERRORS_ON_SCREEN);
    if scrollbar_notches != 0 {
        draw_scrollbar(
            app,
            txt_spacing * drawn_size as f32,
            size,
            scrollbar_notches,
            w - 4.0 - app.text_to_quad_border,
            bottom_height + txt_spacing / 4.0,
            s.scrollbar_offset,
            Color::RED,
            target,
        );
    }

    // C++ `for(int i{drawnSize - 1 + scrollbarOffset}; i > -1 +
    // scrollbarOffset; --i)`.
    for i in (s.scrollbar_offset..s.scrollbar_offset + drawn_size).rev() {
        let Some(msg) = s.load.error_messages.get(i as usize) else {
            continue;
        };
        render_text_centered(
            target,
            &mut app.txt_load_small,
            msg,
            Vec2::new(w / 2.0, bottom_height),
        );
        bottom_height += txt_spacing;
    }
}

/// C++ `MenuGame::drawLevelSelectionRightSide(drawer, revertOffset)`
/// (MenuGame.cpp:4957-5304) — the right-hand level list + the pack-label
/// column. `drawer` is the caller's `&mut` field borrow of the active or
/// the folding drawer (the C++'s `LevelDrawer&`); all other C++ member
/// reads come from `reads` / the seam handles.
#[allow(clippy::too_many_arguments)] // the C++ members threaded through the bridge
fn draw_level_selection_right_side(
    app: &mut MenuDraw,
    drawer: &mut LevelDrawer,
    reads: &LevelSelReads,
    assets: &Rc<dyn MenuAssets>,
    network: &Rc<dyn NetworkClient>,
    revert_offset: bool,
    target: &mut dyn DrawTarget,
) {
    let w = reads.w;

    // C++ :4960-4965 — the indent constants + the drawer X-offset easing.
    let outer_frame = app.text_to_quad_border + app.slct_frame_size;
    let pack_label_offset = w * 0.33 - outer_frame;
    let quads_indent = w - pack_label_offset;
    let txt_indent = w - pack_label_offset / 2.0;
    let level_indent = quads_indent + outer_frame;
    let mut x_offset = drawer.x_offset;
    let panel_offset = layout::calc_menu_offset(
        &mut x_offset,
        w - quads_indent,
        revert_offset,
        false,
        reads.fps_mult,
    );
    drawer.x_offset = x_offset;

    // C++ :4967-4977 — the pack / level counts (the favorites drawer sees
    // one pack; focus-held draws a single "..." row).
    let infos = assets.selectable_pack_ids();
    let packs_size = if drawer.is_favorites {
        1
    } else {
        infos.len() as i32
    };
    let levels_size = if drawer.is_favorites {
        drawer.level_data_ids.len() as i32
    } else if reads.focus_held {
        1
    } else {
        drawer.level_data_ids.len() as i32
    };

    let mut prev_level_indent = 0.0;

    // C++ :4986-4991 — the top instruction lines (two centered strings).
    let top_left = Vec2::new(w / 2.0, 2.0);
    let instruction = if reads.favorites_active {
        "PRESS F2 TO SHOW ALL LEVELS"
    } else {
        "PRESS F2 TO SHOW FAVORITE LEVELS"
    };
    render_text_centered(target, &mut app.txt_selection_small, instruction, top_left);
    render_text_centered(
        target,
        &mut app.txt_selection_small,
        "\nHOLD FOCUS TO JUMP BETWEEN PACKS",
        top_left,
    );

    // C++ :4993-4996 — the alpha level colour + the medium-font fill.
    let alpha_text_color = Color {
        a: 150,
        ..reads.menu_text_color
    };
    app.txt_selection_medium
        .set_fill_color(reads.menu_text_color);

    // C++ `float height{0}` (:4988) — the C++'s initial 0 is immediately
    // overwritten (:4998); initialize directly.
    let mut height = app.pack_label_height
        * (if reads.favorites_active {
            1
        } else {
            drawer.pack_idx + 1
        } as f32)
        + app.slct_frame_size
        - reads.pack_change_offset
        + drawer.y_offset;

    // C++ :5000-5100 — the level list (quads first, then the labels).
    for i in 0..levels_size {
        app.menu_quads.clear();
        app.menu_quads.reserve_quad(3);

        // If the list is folding give all level labels the same
        // alignment; otherwise ease the per-level X offsets.
        if reads.pack_change_state != PackChange::Rest {
            if let Some(o) = drawer.lvl_offsets.get_mut(i as usize) {
                *o = 0.0;
            }
        } else if let Some(o) = drawer.lvl_offsets.get_mut(i as usize) {
            layout::calc_menu_item_offset(o, i == drawer.current_index, reads.fps_mult);
        }
        // A missing entry reads 0 (the C++'s `lvlOffsets` is sized to the
        // max pack level count, ctor step 10; the port's coarse resize,
        // menu_game.rs `init_boot_level`, can leave it short).
        let off = drawer.lvl_offsets.get(i as usize).copied().unwrap_or(0.0);
        let mut indent = quads_indent + panel_offset;
        if !reads.focus_held {
            indent -= off;
        }

        // Top frame.
        if i > 0 {
            let prev_off = drawer
                .lvl_offsets
                .get((i - 1) as usize)
                .copied()
                .unwrap_or(0.0);
            if prev_off > off {
                create_quad(
                    &mut app.menu_quads,
                    reads.menu_quad_color,
                    prev_level_indent,
                    w,
                    height,
                    height + app.slct_frame_size,
                );
            } else {
                create_quad(
                    &mut app.menu_quads,
                    reads.menu_quad_color,
                    indent,
                    w,
                    height,
                    height + app.slct_frame_size,
                );
            }
        } else {
            create_quad(
                &mut app.menu_quads,
                reads.menu_quad_color,
                indent,
                w,
                height,
                height + app.slct_frame_size,
            );
        }

        // Side frame.
        create_quad(
            &mut app.menu_quads,
            reads.menu_quad_color,
            indent,
            indent + app.slct_frame_size,
            height + app.slct_frame_size,
            height + app.level_label_height,
        );

        // Body (module docs: the no-hover C++ path — `mouseOverlap`
        // false, `mouseOverlapColor(false, c)` the identity).
        let c = if i == drawer.current_index {
            reads.menu_selection_color
        } else {
            alpha_text_color
        };
        create_quad(
            &mut app.menu_quads,
            c,
            indent + app.slct_frame_size,
            w,
            height + app.slct_frame_size,
            height + app.level_label_height,
        );

        // C++ :5040-5050 — the click edge (module docs: no mouse in the
        // port; `mustPlay` / `mustChangeIndexTo` never fire).
        target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());
        prev_level_indent = indent;

        // Level data (the C++ null-checks the pointer + `continue`s).
        let Some(level_id) = drawer.level_data_ids.get(i as usize).cloned() else {
            continue;
        };
        let Some(level) = assets.level(&level_id) else {
            continue;
        };

        // Level name.
        let mut indent = level_indent + panel_offset;
        if !reads.focus_held {
            indent -= off;
        }
        height += app.text_to_quad_border;

        let mut temp_string = if reads.focus_held {
            String::from("...")
        } else {
            level.name.clone()
        };
        uppercasify(&mut temp_string);

        // C++ `mouseOverlapColor(mouseOverlap, menuQuadColor)` = identity
        // on the no-mouse path.
        let current_diff_mult = level.get_nth_diff_mult(reads.diff_mult_idx);
        let level_validator = level.get_validator(current_diff_mult);

        render_text_color(
            target,
            &mut app.txt_selection_big,
            &temp_string,
            Vec2::new(
                indent,
                height - app.h_selection_big * layout::FONT_HEIGHT_OFFSET,
            ),
            reads.menu_quad_color,
        );

        // The RANKED badge (online-supported, scored levels only).
        if !level.unscored && network.is_level_supported_by_server(level_validator) {
            let padding = 5.0;
            let width = 50.0;

            app.menu_quads.clear();
            app.menu_quads.reserve_quad(1);
            create_quad(
                &mut app.menu_quads,
                reads.menu_quad_color,
                w - width - padding,
                w,
                height - app.text_to_quad_border,
                height - app.text_to_quad_border + app.h_selection_ranked + padding + 1.0,
            );
            target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());

            render_text_color(
                target,
                &mut app.txt_selection_ranked,
                "RANKED",
                Vec2::new(
                    w - width,
                    height - app.h_selection_ranked * layout::FONT_HEIGHT_OFFSET - 3.0,
                ),
                c,
            );
        }

        // Author.
        height += app.h_selection_big + app.text_to_quad_border;

        let mut temp_string = if reads.focus_held {
            String::from("...")
        } else {
            level.author.clone()
        };
        uppercasify(&mut temp_string);
        render_text_color(
            target,
            &mut app.txt_selection_small,
            &temp_string,
            Vec2::new(
                indent,
                height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
            ),
            reads.menu_quad_color,
        );

        height += app.h_selection_small + app.text_to_quad_border - app.slct_frame_size;
    }

    // Bottom frame for the last element.
    app.menu_quads.clear();
    app.menu_quads.reserve_quad(1);
    create_quad(
        &mut app.menu_quads,
        reads.menu_quad_color,
        prev_level_indent,
        w,
        height,
        height + app.slct_frame_size,
    );
    target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());

    height += app.slct_frame_size;
    let mut i = layout::mod_range(drawer.pack_idx + 1, 0, packs_size);
    if i == 0 {
        height = drawer.y_offset;
    }

    // C++ :5108-5304 — the pack labels (the do-while over the packs,
    // starting AFTER the current pack; the current pack's label carries
    // the downward arrow, the others the right-pointing one).
    let arrow_width = app.pack_label_height / 2.0 - app.text_to_quad_border;
    let start_i = i;
    loop {
        app.menu_quads.clear();
        app.menu_quads.reserve_quad(2);

        let mut temp_x = quads_indent - outer_frame + panel_offset;

        create_quad(
            &mut app.menu_quads,
            reads.menu_text_color,
            temp_x - app.slct_frame_size,
            w,
            height,
            height + app.pack_label_height + app.slct_frame_size,
        );

        // Body (module docs: no hover).
        create_quad(
            &mut app.menu_quads,
            reads.menu_quad_color,
            temp_x,
            w,
            height + app.slct_frame_size,
            height + app.pack_label_height,
        );

        // C++ :5131-5145 — the label-row quads render (the click edge is
        // the no-mouse path; `mustChangePackIndexTo` never fires).
        target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());

        // Name & >.
        let pack_name = if drawer.is_favorites {
            "FAVORITES".to_string()
        } else {
            let pack_id = infos.get(i as usize).cloned().unwrap_or_default();
            let mut name = assets.pack_data_info(&pack_id).name;
            uppercasify(&mut name);
            name
        };

        app.txt_selection_medium.set_string(&pack_name);
        temp_x = (txt_indent - app.txt_selection_medium.get_global_bounds().get_width() / 2.0)
            .max(quads_indent + arrow_width + 2.0 * app.slct_frame_size + outer_frame)
            + panel_offset;

        app.txt_selection_medium.set_position(Vec2::new(
            temp_x,
            height + outer_frame - app.h_selection_medium * layout::FONT_HEIGHT_OFFSET,
        ));
        // C++ :5157-5162 — save / set / render / restore the fill
        // (`mouseOverlapColor(false, menuTextColor)` = `menuTextColor`).
        {
            let old = app.txt_selection_medium.get_fill_color();
            app.txt_selection_medium
                .set_fill_color(reads.menu_text_color);
            target.draw_text(&app.txt_selection_medium, RenderStates::default());
            app.txt_selection_medium.set_fill_color(old);
        }

        app.menu_quads.clear();
        app.menu_quads.reserve_quad(2);

        if i == drawer.pack_idx {
            // Draw > pointing downward (the current pack; the C++ comment
            // notes the coordinates align the arrow with the middle of
            // the regular > arrows).
            height += (app.pack_label_height - app.text_to_quad_border - app.slct_frame_size) / 2.0;
            let mut temp = quads_indent + arrow_width / 2.0 + app.slct_frame_size + panel_offset;

            let top_left = Vec2::new(temp - arrow_width, height);
            let bottom_left = Vec2::new(temp - arrow_width, height + 2.0 * app.slct_frame_size);
            height += arrow_width;
            let top_right = Vec2::new(temp, height);
            let bottom_right = Vec2::new(temp, height + 2.0 * app.slct_frame_size);
            app.menu_quads.push_quad(
                reads.menu_text_color,
                top_left,
                bottom_left,
                bottom_right,
                top_right,
            );

            let top_left = Vec2::new(temp, height);
            let bottom_left = Vec2::new(temp, height + 2.0 * app.slct_frame_size);
            height -= arrow_width;
            temp += arrow_width;
            let top_right = Vec2::new(temp, height);
            let bottom_right = Vec2::new(temp, height + 2.0 * app.slct_frame_size);
            app.menu_quads.push_quad(
                reads.menu_text_color,
                top_left,
                bottom_left,
                bottom_right,
                top_right,
            );

            target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());
        } else {
            height += app.slct_frame_size / 2.0;
            let mut temp = quads_indent + panel_offset;

            let top_left = Vec2::new(temp, height + app.text_to_quad_border);
            let top_right = Vec2::new(
                temp + 2.0 * app.slct_frame_size,
                height + app.text_to_quad_border,
            );
            height += app.pack_label_height / 2.0;
            temp += app.pack_label_height / 2.0 - app.text_to_quad_border;
            let bottom_left = Vec2::new(temp, height);
            let bottom_right = Vec2::new(temp + 2.0 * app.slct_frame_size, height);
            app.menu_quads.push_quad(
                reads.menu_text_color,
                top_left,
                bottom_left,
                bottom_right,
                top_right,
            );

            let top_left = Vec2::new(temp, height);
            let top_right = Vec2::new(temp + 2.0 * app.slct_frame_size, height);
            height += app.pack_label_height / 2.0;
            temp = quads_indent + panel_offset;
            let bottom_left = Vec2::new(temp, height - app.text_to_quad_border);
            let bottom_right = Vec2::new(
                temp + 2.0 * app.slct_frame_size,
                height - app.text_to_quad_border,
            );
            app.menu_quads.push_quad(
                reads.menu_text_color,
                top_left,
                bottom_left,
                bottom_right,
                top_right,
            );

            target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());
            height -= app.slct_frame_size / 2.0;
        }

        i = layout::mod_range(i + 1, 0, packs_size);
        if i == 0 {
            height = drawer.y_offset;
        }
        if i == start_i {
            break;
        }
    }
}

/// C++ `MenuGame::drawLevelSelectionLeftSide(drawer, revertOffset)`
/// (MenuGame.cpp:5306-5748) — the left-hand level-details panel: the
/// backdrop, the scrolling level name, the level description, the
/// difficulty / pack / music blocks, the favorite button, and the
/// leaderboard block (the offline fallbacks — the port's
/// `NetworkClient` reports `Disconnected`, so the C++'s offline paths are
/// the ones rendered).
///
/// `shared` is the caller's `RefMut` (the C++'s `*lvlDrawer` is read
/// through it — the active drawer, read-only here — and the C++
/// `levelDetailsOffset` member is eased in place).
#[allow(clippy::too_many_arguments)] // the C++ members threaded through the bridge
fn draw_level_selection_left_side(
    app: &mut MenuDraw,
    shared: &mut Shared,
    reads: &LevelSelReads,
    assets: &Rc<dyn MenuAssets>,
    network: &Rc<dyn NetworkClient>,
    revert_offset: bool,
    target: &mut dyn DrawTarget,
) {
    let w = reads.w;
    let h = reads.h;

    // C++ `*lvlDrawer` — the active drawer (the port's
    // `favorites_active` picks it, as the C++ `lvlDrawer` pointer does).
    let (drawer_ids, current_index) = if reads.favorites_active {
        (
            &shared.fav_slct.level_data_ids,
            shared.fav_slct.current_index,
        )
    } else {
        (
            &shared.lvl_slct.level_data_ids,
            shared.lvl_slct.current_index,
        )
    };

    // C++ :5308-5327 — the current level + pack (the C++ guards on
    // `currentPack == nullptr`).
    let Some(level_id) = drawer_ids.get(current_index as usize) else {
        return;
    };
    let Some(level) = assets.level(level_id) else {
        return;
    };
    let current_pack = assets.pack_data_info(&level.pack_id);

    let line_thickness = 2.0;

    let max_panel_offset = w * 0.33;
    let panel_offset = layout::calc_menu_offset(
        &mut shared.level_details_offset,
        max_panel_offset,
        revert_offset,
        false,
        reads.fps_mult,
    );
    let small_interline = app.h_selection_small * 1.5;
    let medium_interline = app.h_selection_small / 2.0;
    let text_x_pos = app.text_to_quad_border - panel_offset;
    // C++ `getMaximumTextWidth()` (:4673-4675) — `w * 0.33 - 2 * textToQuadBorder`.
    let text_right_border = w * 0.33 - 2.0 * app.text_to_quad_border;

    let width = max_panel_offset - panel_offset;
    let mut height = app.text_to_quad_border;

    // C++ :5333-5341 — the backdrop (the alpha-150 text-colour fill) + the
    // right border line.
    app.menu_quads.clear();
    app.menu_quads.reserve_quad(2);
    let alpha_text_color = Color {
        a: 150,
        ..reads.menu_text_color
    };
    create_quad(&mut app.menu_quads, alpha_text_color, 0.0, width, 0.0, h);
    create_quad(
        &mut app.menu_quads,
        reads.menu_quad_color,
        width,
        width + line_thickness,
        0.0,
        h,
    );
    target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());
    app.menu_quads.clear();

    // C++ :5343-5348 — the level name (the keyless right-border marquee,
    // `namesScroll[Label::LevelName]`).
    let mut temp_string = level.name.clone();
    scroll_name_right_border_no_key(
        &mut temp_string,
        &mut app.txt_selection_big,
        &mut app.names_scroll[0],
        text_right_border,
        reads.fps_mult,
    );
    render_text(
        target,
        &mut app.txt_selection_big,
        &temp_string,
        Vec2::new(
            text_x_pos,
            height - app.h_selection_big * layout::FONT_HEIGHT_OFFSET,
        ),
    );

    // C++ :5350-5369 — the level description (up to `descLines` = 5 lines;
    // the C++ `formatLevelDescription` runs on `setIndex` / `refreshCamera`
    // — the bridge recomputes lazily on the `(level id, w)` key).
    height += app.h_selection_big + app.text_to_quad_border - app.h_selection_small * 0.7;

    let desc_key = (level.id.clone(), w);
    if app.desc_key.as_ref() != Some(&desc_key) {
        app.desc_key = Some(desc_key.clone());
        format_level_description(app, &level, w);
    }
    let desc_lines_len = app.desc_lines.len();
    for i in 0..desc_lines_len {
        let line = app.desc_lines[i].clone();
        render_text(
            target,
            &mut app.txt_selection_small,
            &line,
            Vec2::new(text_x_pos, height),
        );
        // C++ `i == descLines - 1 ? txtSelectionSmall.height : smallInterline`.
        height += if i == 4 {
            app.h_selection_small
        } else {
            small_interline
        };
    }
    if desc_lines_len != 5 {
        height += small_interline * (4usize.saturating_sub(desc_lines_len)) as f32
            + app.h_selection_small;
    }
    height += app.text_to_quad_border + app.h_selection_small * 0.7;

    // C++ :5371-5407 — the difficulty block (top line, the bump-scaled
    // value, bottom line).
    app.menu_quads.reserve_more_quad(2);
    height += line_thickness;
    create_quad(
        &mut app.menu_quads,
        reads.menu_quad_color,
        0.0,
        width,
        height - line_thickness,
        height,
    );

    app.txt_selection_small
        .set_fill_color(reads.menu_quad_color);
    app.txt_selection_medium
        .set_fill_color(reads.menu_quad_color);

    height += app.text_to_quad_border;
    let difficulty_height = height - app.h_selection_medium * layout::FONT_HEIGHT_OFFSET;

    render_text_color(
        target,
        &mut app.txt_selection_medium,
        "DIFFICULTY: ",
        Vec2::new(text_x_pos, difficulty_height),
        reads.menu_quad_color,
    );

    let difficulty_str = if level.difficulty_mults.len() > 1 {
        let current_diff_mult = level.get_nth_diff_mult(reads.diff_mult_idx);
        // C++ `"< " + ssvu::toStr(mult) + " >"` — the `toStr` of a `float`
        // streams with the default 6-significant-digit format.
        format!("< {} >", format_ostream_f64(current_diff_mult as f64))
    } else {
        "NONE".to_string()
    };

    let difficulty_bump_factor =
        1.0 + (reads.difficulty_bump_effect / DIFFICULTY_BUMP_EFFECT_MAX) * 0.25;
    app.txt_selection_medium
        .set_scale(Vec2::new(difficulty_bump_factor, difficulty_bump_factor));
    // C++ :5397-5400 — the x is `textXPos + getGlobalBounds().width` of the
    // "DIFFICULTY: " string STILL ON the medium font, measured WITH the
    // bump scale applied (the string the `renderText` above left there).
    let label_width = app.txt_selection_medium.get_global_bounds().get_width();
    render_text(
        target,
        &mut app.txt_selection_medium,
        &difficulty_str,
        Vec2::new(text_x_pos + label_width, difficulty_height),
    );
    app.txt_selection_medium.set_scale(Vec2::new(1.0, 1.0));

    // Bottom line.
    height += app.h_selection_medium + app.text_to_quad_border + line_thickness;
    create_quad(
        &mut app.menu_quads,
        reads.menu_quad_color,
        0.0,
        width,
        height,
        height - line_thickness,
    );

    // C++ :5409-5448 — the pack info block.
    height += app.text_to_quad_border;
    render_text(
        target,
        &mut app.txt_selection_medium,
        "PACK",
        Vec2::new(
            app.text_to_quad_border - panel_offset,
            height - app.h_selection_medium * layout::FONT_HEIGHT_OFFSET,
        ),
    );

    height += app.h_selection_medium + medium_interline;
    let mut temp_string = current_pack.name.clone();
    scroll_name_right_border(
        &mut temp_string,
        "NAME: ",
        &mut app.txt_selection_small,
        &mut app.names_scroll[1],
        text_right_border,
        reads.fps_mult,
    );
    render_text(
        target,
        &mut app.txt_selection_small,
        &temp_string,
        Vec2::new(
            text_x_pos,
            height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
        ),
    );

    // Pack author.
    height += app.h_selection_small + medium_interline;
    let mut temp_string = current_pack.author.clone();
    scroll_name_right_border(
        &mut temp_string,
        "AUTHOR: ",
        &mut app.txt_selection_small,
        &mut app.names_scroll[2],
        text_right_border,
        reads.fps_mult,
    );
    render_text(
        target,
        &mut app.txt_selection_small,
        &temp_string,
        Vec2::new(
            text_x_pos,
            height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
        ),
    );

    // Version.
    height += app.h_selection_small + medium_interline;
    let mut temp_string = format!("VERSION: {}", current_pack.version);
    uppercasify(&mut temp_string);
    render_text(
        target,
        &mut app.txt_selection_small,
        &temp_string,
        Vec2::new(
            text_x_pos,
            height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
        ),
    );

    // Bottom line.
    app.menu_quads.reserve_more_quad(1);
    height += app.h_selection_small + app.h_selection_medium / 2.0 + line_thickness;
    create_quad(
        &mut app.menu_quads,
        reads.menu_quad_color,
        0.0,
        width,
        height,
        height - line_thickness,
    );

    // C++ :5450-5486 — the music info block (missing music data is the C++
    // default-constructed `MusicData` — the seam's empty strings).
    height += app.text_to_quad_border;
    render_text(
        target,
        &mut app.txt_selection_medium,
        "MUSIC",
        Vec2::new(
            app.text_to_quad_border - panel_offset,
            height - app.h_selection_medium * layout::FONT_HEIGHT_OFFSET,
        ),
    );

    height += app.h_selection_medium + medium_interline;
    let mut temp_string = level.music_name.clone();
    scroll_name_right_border(
        &mut temp_string,
        "NAME: ",
        &mut app.txt_selection_small,
        &mut app.names_scroll[3],
        text_right_border,
        reads.fps_mult,
    );
    render_text(
        target,
        &mut app.txt_selection_small,
        &temp_string,
        Vec2::new(
            text_x_pos,
            height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
        ),
    );

    // Track author.
    height += app.h_selection_small + medium_interline;
    let mut temp_string = level.music_author.clone();
    scroll_name_right_border(
        &mut temp_string,
        "AUTHOR: ",
        &mut app.txt_selection_small,
        &mut app.names_scroll[4],
        text_right_border,
        reads.fps_mult,
    );
    render_text(
        target,
        &mut app.txt_selection_small,
        &temp_string,
        Vec2::new(
            text_x_pos,
            height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
        ),
    );

    // Album name.
    height += app.h_selection_small + medium_interline;
    let mut temp_string = if level.music_album.is_empty() {
        "NONE".to_string()
    } else {
        level.music_album.clone()
    };
    scroll_name_right_border(
        &mut temp_string,
        "ALBUM: ",
        &mut app.txt_selection_small,
        &mut app.names_scroll[5],
        text_right_border,
        reads.fps_mult,
    );
    render_text(
        target,
        &mut app.txt_selection_small,
        &temp_string,
        Vec2::new(
            text_x_pos,
            height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
        ),
    );

    height += app.h_selection_small + app.text_to_quad_border;

    // C++ :5488-5517 — the favorite "button" (the frame quads + the body +
    // the centred label; the click edge is the no-mouse path, so
    // `mustFavorite` never fires).
    app.menu_quads.reserve_more_quad(10);
    let favorite_button_bottom = height + 3.0 * app.h_selection_medium;

    create_quad(
        &mut app.menu_quads,
        reads.menu_quad_color,
        line_thickness - panel_offset,
        width,
        height,
        height + line_thickness,
    );
    create_quad(
        &mut app.menu_quads,
        reads.menu_quad_color,
        -panel_offset,
        line_thickness - panel_offset,
        height,
        favorite_button_bottom,
    );
    create_quad(
        &mut app.menu_quads,
        reads.menu_quad_color,
        width,
        width,
        height,
        favorite_button_bottom,
    );
    create_quad(
        &mut app.menu_quads,
        reads.menu_quad_color,
        line_thickness - panel_offset,
        width,
        favorite_button_bottom - line_thickness,
        favorite_button_bottom,
    );
    create_quad(
        &mut app.menu_quads,
        reads.menu_selection_color,
        line_thickness - panel_offset,
        width,
        height + line_thickness,
        favorite_button_bottom - line_thickness,
    );
    target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());
    app.menu_quads.clear();

    render_text_centered_offset_color(
        target,
        &mut app.txt_selection_medium,
        if reads.is_level_favorite {
            "[F1] UNFAVORITE"
        } else {
            "[F1]    FAVORITE"
        },
        Vec2::new(
            max_panel_offset / 2.0,
            height + app.h_selection_medium * (1.0 - layout::FONT_HEIGHT_OFFSET),
        ),
        -panel_offset,
        reads.menu_quad_color,
    );

    height = favorite_button_bottom + app.text_to_quad_border;

    // C++ :5519-5542 — the personal best.
    render_text(
        target,
        &mut app.txt_selection_medium,
        "LOCAL PERSONAL BEST",
        Vec2::new(
            app.text_to_quad_border - panel_offset,
            height - app.h_selection_medium * layout::FONT_HEIGHT_OFFSET,
        ),
    );
    height += app.h_selection_medium + app.text_to_quad_border;

    let current_diff_mult = level.get_nth_diff_mult(reads.diff_mult_idx);

    if level.unscored {
        render_text(
            target,
            &mut app.txt_selection_small,
            "N/A",
            Vec2::new(
                app.text_to_quad_border - panel_offset,
                height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
            ),
        );
    } else {
        // C++ `getValidatorWithoutPackId` + the current profile's score.
        let local_level_validator = level.get_validator_without_pack_id(current_diff_mult);
        let score = assets.local_profile_score(local_level_validator);
        let score_str = format!("{}s", format_ostream_f64(score as f64));
        render_text(
            target,
            &mut app.txt_selection_small,
            &score_str,
            Vec2::new(
                app.text_to_quad_border - panel_offset,
                height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
            ),
        );
    }

    // Line.
    height += app.h_selection_small + app.text_to_quad_border + line_thickness;
    app.menu_quads.reserve_quad(1);
    create_quad(
        &mut app.menu_quads,
        reads.menu_quad_color,
        0.0,
        width,
        height,
        height - line_thickness,
    );

    // "LEADERBOARD".
    height += app.text_to_quad_border;
    render_text_centered_offset(
        target,
        &mut app.txt_selection_big,
        "ONLINE LEADERBOARD",
        Vec2::new(
            max_panel_offset / 2.0,
            height - app.h_selection_big * layout::FONT_HEIGHT_OFFSET * 0.8,
        ),
        -panel_offset,
    );

    // Line.
    height += app.h_selection_score + app.h_selection_big / 2.0 + 3.0;
    app.menu_quads.reserve_more_quad(1);
    create_quad(
        &mut app.menu_quads,
        reads.menu_quad_color,
        0.0,
        width,
        height,
        height + line_thickness,
    );
    height += line_thickness;

    height += app.h_selection_small;

    let level_validator = level.get_validator(current_diff_mult);

    // C++ :5558-5565 — the score-request kick (the C++
    // `leaderboardCache->shouldRequestScores` 6-second TTL predicate is
    // the `NetworkClient` seam's `should_request_scores`).
    if !level.unscored
        && network.state() == ConnectionState::LoggedIn_Ready
        && network.is_level_supported_by_server(level_validator)
        && network.should_request_scores(level_validator)
    {
        network.request_scores(level_validator);
    }
    let got_score_info = network.has_score_information(level_validator);

    let tx = app.text_to_quad_border - panel_offset;
    if level.unscored {
        render_text(
            target,
            &mut app.txt_selection_small,
            "LEADERBOARD DISABLED FOR THIS LEVEL",
            Vec2::new(
                tx,
                height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
            ),
        );
    } else if network.state() != ConnectionState::LoggedIn_Ready {
        render_text(
            target,
            &mut app.txt_selection_small,
            "PLEASE LOG IN TO LOAD LEADERBOARD",
            Vec2::new(
                tx,
                height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
            ),
        );
    } else if !network.is_level_supported_by_server(level_validator) {
        render_text(
            target,
            &mut app.txt_selection_small,
            "THIS LEVEL IS NOT SUPPORTED BY THE SERVER",
            Vec2::new(
                tx,
                height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
            ),
        );
    } else if !got_score_info {
        render_text(
            target,
            &mut app.txt_selection_small,
            "...",
            Vec2::new(
                tx,
                height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
            ),
        );
    } else {
        let scores = network.scores(level_validator);
        if !scores.is_empty() {
            for ps in &scores {
                // C++ `drawEntry(index, ...)` — the position arg is the
                // loop index (the C++ prints `#(i + 1)`).
                draw_leaderboard_entry(app, target, ps, &mut height, tx);
            }
            height -= app.h_selection_medium + app.h_selection_small;
        } else {
            render_text(
                target,
                &mut app.txt_selection_medium,
                "NO SCORES FOUND",
                Vec2::new(
                    tx,
                    height - app.h_selection_medium * layout::FONT_HEIGHT_OFFSET,
                ),
            );
        }

        // Line.
        height += app.h_selection_score + app.h_selection_big / 2.0;
        app.menu_quads.reserve_more_quad(1);
        create_quad(
            &mut app.menu_quads,
            reads.menu_quad_color,
            0.0,
            width,
            height,
            height + line_thickness,
        );
        height += line_thickness;

        height += app.h_selection_small;

        render_text(
            target,
            &mut app.txt_selection_small,
            "YOUR POSITION",
            Vec2::new(
                tx,
                height - app.h_selection_small * layout::FONT_HEIGHT_OFFSET,
            ),
        );
        height += app.h_selection_small * 2.0 + 5.0;

        if got_score_info {
            match network.own_score(level_validator) {
                None => {
                    render_text(
                        target,
                        &mut app.txt_selection_medium,
                        "NO OWN SCORE SET",
                        Vec2::new(
                            tx,
                            height - app.h_selection_medium * layout::FONT_HEIGHT_OFFSET,
                        ),
                    );
                }
                Some(own) => {
                    // C++ `drawEntry(ownScore->position, ...)` — verbatim,
                    // including the `#(position + 1)` print (the C++ passes
                    // the stored rank as the `i` arg; kept as shipped).
                    draw_leaderboard_entry(app, target, &own, &mut height, tx);
                }
            }
        }
    }

    // C++ :5747 — the final `render(menuQuads)` (the remaining line quads).
    target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());
}

/// C++ `drawEntry` lambda inside `drawLevelSelectionLeftSide` (MenuGame.cpp:
/// 5583-5621) — one leaderboard row: the UTC timestamp line + the
/// `#pos` / `score` / `player` line, then the inter-row advance. `pos`
/// is the C++ lambda's `i` arg (the loop index for top scores, the stored
/// rank for the own-score entry) and prints as `#(i + 1)` — verbatim.
fn draw_leaderboard_entry(
    app: &mut MenuDraw,
    target: &mut dyn DrawTarget,
    entry: &ScoreEntry,
    height: &mut f32,
    tx: f32,
) {
    let score = entry.score_value as f32;

    // C++ `Utils::toTimepoint` + `formatTimepoint(..., "%Y-%m-%d %H:%M:%S")`
    // (Utils/Timestamp.cpp:24-44 — UTC).
    let tp = to_timepoint(entry.score_timestamp as u64);
    let timestamp_str = format_timepoint(tp, "%Y-%m-%d %H:%M:%S");

    let pos_str = format!("#{}", entry.position + 1);
    // C++ `ssvu::toStr(score) + 's'` — the `float`'s default 6-significant-
    // digit stream format.
    let score_str = format!("{}s", format_ostream_f64(score as f64));

    // C++ player-name clamp (> 19 → 16 + "...") — the C++ is byte-based
    // (`std::string::size` / `resize(16)`); the boundary floor keeps the
    // `&str` slice valid on multi-byte names (ASCII names: identical).
    let player_str = if entry.user_name.len() > 19 {
        let mut end = 16;
        while !entry.user_name.is_char_boundary(end) {
            end -= 1;
        }
        let mut s = entry.user_name[..end].to_string();
        s.push_str("...");
        s
    } else {
        entry.user_name.clone()
    };

    let ty =
        *height - app.h_selection_medium * layout::FONT_HEIGHT_OFFSET + app.h_selection_small - 9.0;
    let y_spacing = 11.0;

    render_text(
        target,
        &mut app.txt_selection_small,
        &timestamp_str,
        Vec2::new(tx, ty),
    );
    render_text(
        target,
        &mut app.txt_selection_medium,
        &pos_str,
        Vec2::new(tx, ty + y_spacing),
    );
    render_text(
        target,
        &mut app.txt_selection_medium,
        &score_str,
        Vec2::new(tx + 58.0, ty + y_spacing),
    );
    render_text(
        target,
        &mut app.txt_selection_medium,
        &player_str,
        Vec2::new(tx + 185.0, ty + y_spacing),
    );

    *height += app.h_selection_medium + app.h_selection_small + app.h_selection_small + 10.0;
}

/// C++ `MenuGame::formatLevelDescription()` (MenuGame.cpp:4678-4775) —
/// the uppercase word wrap of the current level's description into at most
/// `descLines` (5) lines, measured against `getMaximumTextWidth()` with the
/// `txtSelectionSmall` font. The C++ runs this in `setIndex` (:2950) and
/// `refreshCamera` (:3252-3255); the bridge runs it lazily in
/// `draw_level_selection_left_side` when the `(level id, w)` key changes
/// (the description is only ever read there — same visible result, no
/// menu-side font access needed).
fn format_level_description(app: &mut MenuDraw, level: &LevelInfo, w: f32) {
    // C++ `getMaximumTextWidth()` (:4673-4675).
    let max_width = w * 0.33 - 2.0 * app.text_to_quad_border;
    app.desc_lines.clear();

    let mut desc = level.description.clone();
    if desc.is_empty() {
        return;
    }
    uppercasify(&mut desc);

    // C++ :4691-4706 — split into words; newline words keep the newline,
    // spaces are skipped. (Byte indices: the split points are ASCII bytes,
    // so every slice is a valid UTF-8 boundary — the C++ is byte-oriented
    // too.)
    let mut words: Vec<String> = Vec::new();
    let mut with_newline = desc.clone();
    with_newline.push('\n');
    let mut j = 0usize;
    for (i, b) in with_newline.as_bytes().iter().enumerate() {
        match b {
            b'\n' => {
                words.push(with_newline[j..=i].to_string());
                j = i + 1;
            }
            b' ' => {
                words.push(with_newline[j..i].to_string());
                j = i + 1;
            }
            _ => {}
        }
    }

    // C++ :4708-4769 — group words into lines while they fit `maxWidth`
    // (the width probe is the `txtSelectionSmall` font, as the C++ sets
    // the string on that font).
    let desc_lines = 5; // C++ `inline constexpr int descLines{5}` (:4602)
    let mut candidate = String::new();
    for word in &words {
        if app.desc_lines.len() >= desc_lines {
            break;
        }
        let candidate_empty = candidate.is_empty();
        let temp = if candidate_empty {
            word.clone()
        } else {
            format!(" {word}")
        };
        // C++ :4735-4741 — the width probe string is `candidate + temp`
        // (the full line candidate) on the `txtSelectionSmall` font.
        let probe = if candidate_empty {
            temp.clone()
        } else {
            format!("{candidate}{temp}")
        };
        app.txt_selection_small.set_string(&probe);

        // If the last character is a newline...
        if !temp.is_empty() && temp.ends_with('\n') {
            if app.txt_selection_small.get_local_bounds().get_width() < max_width {
                candidate += &temp;
                app.desc_lines.push(candidate.clone());
            } else {
                // ...otherwise add "candidate" to the vector and add the
                // new word on its own.
                app.desc_lines.push(candidate.clone());
                if app.desc_lines.len() < desc_lines {
                    app.desc_lines.push(word.clone());
                }
            }
            candidate.clear();
            continue;
        }

        // If there is no newline check if the line fits...
        if app.txt_selection_small.get_local_bounds().get_width() < max_width {
            candidate += &temp;
            continue;
        }

        // ...if it doesn't add to the vector "candidate" and then set it
        // to be just the overflowing word, to be checked upon in the next
        // cycles.
        app.desc_lines.push(candidate.clone());
        candidate = word.clone();
    }

    // Add whatever is left if it fits.
    if app.desc_lines.len() < desc_lines {
        app.desc_lines.push(candidate);
    }
}

// ---------------------------------------------------------------------------
// The profile / entering-text draw surface (C++ `drawProfileSelection`
// :3880, `drawProfileSelectionBoot` :4007, `drawEnteringText` :4091,
// `drawEnteringTextBoot` :4147, `formatSurvivalTime` :3847)
// ---------------------------------------------------------------------------

/// C++ `MenuGame::formatSurvivalTime(data)` (MenuGame.cpp:3847-3878) —
/// the "Total survival time …" line. `total` is the C++'s summed
/// `getScores()` seconds (the port seam's `profile_total_survival_time`).
fn format_survival_time(total: i32) -> String {
    // C++ `stream << "Total survival time "`.
    let mut out = String::from("Total survival time ");
    if total < 60 {
        // C++ `setfill('0') << setw(2) << time`.
        out.push_str(&format!("{total:02}"));
    } else if total < 3600 {
        // C++ `setw(2) << time / 60 << ":" << setw(2) << time % 60`.
        out.push_str(&format!("{:02}:{:02}", total / 60, total % 60));
    } else {
        // C++ `time / 3600 << ":"` (unpadded) + `time %= 3600` + the two
        // padded parts.
        out.push_str(&format!(
            "{}:{:02}:{:02}",
            total / 3600,
            (total % 3600) / 60,
            total % 60
        ));
    }
    out
}

/// C++ `MenuGame::drawProfileSelection(xOffset, revertOffset)`
/// (MenuGame.cpp:3880-4005) — the profile list panel: the surrounding
/// quads, the optional scrollbar, the centered name rows (`profCharSize`
/// 30, the selected one 42) with their 15-pt-smaller survival-time
/// lines, and the deletion instructions below the box. `shared` is the
/// caller's `Ref` (the C++ reads its members directly; the eased
/// offset is the menu's `profileSelectionMenu` category member).
fn draw_profile_selection(
    app: &mut MenuDraw,
    menu: &MenuGame,
    shared: &Shared,
    x_offset: f32,
    revert_offset: bool,
    target: &mut dyn DrawTarget,
) {
    let w = shared.w;
    let h = shared.h;
    let menu_text_color = shared.menu_text_color;
    let menu_quad_color = shared.menu_quad_color;

    // C++ :3881-3887.
    let category = menu.profile_selection_menu.get_category();
    let items = category.get_items();
    let real_size = items.len() as i32;
    let (scrollbar_notches, drawn_size) =
        layout::scrollbar_notches(real_size, layout::MAX_PROFILES_ON_SCREEN);

    // C++ :3890-3893 — `getFontHeight(txtProfile.font, 30 / 42)`.
    let font_height = app.h_prof_char;
    let selected_font_height = app.h_prof_selected;

    // C++ :3896-3906 — the name width probes (`profMinWidth` 400 floor;
    // the C++ probes on `txtProfile.font` at its CURRENT char size — the
    // size the previous frame's survival lines left it at; the port's
    // persistent `txt_profile` mirrors that).
    let mut text_width: f32 = 400.0;
    for item in &items {
        let mut name = item.borrow().get_name();
        uppercasify(&mut name);
        app.txt_profile.set_string(&name);
        text_width = text_width.max(app.txt_profile.get_global_bounds().get_width());
    }

    // C++ :3908-3915 — the vertical constants + the scrollbar space.
    let interline = 4.0 * font_height;
    let double_border = 20.0; // C++ `profFrameSize * 2.f` (10)
    let total_height =
        (interline * (drawn_size as f32 - 1.0) + double_border * 2.0 + font_height * 3.0)
            .max(360.0); // C++ `profMinHeight`
    let scrollbar_interspace = 3.0; // C++ `scrollbarInterspace`
    text_width += double_border + scrollbar_interspace * 2.0;

    // C++ :3917-3918 — keep the box in bounds.
    let mut indent = ((w + x_offset) * 0.5 - text_width) / 2.0 + 10.0;
    indent = indent.max(double_border);

    // C++ :3920-3930 — the deletion-instructions width probe + indent.
    app.txt_instructions_small.set_string(
        "Press backspace to delete the selected profile\nYou cannot delete the profile currently in use",
    );
    let instructions_width = app.txt_instructions_small.get_global_bounds().get_width();
    let result_indent = indent + (text_width - instructions_width) / 2.0;
    if result_indent < 0.0 {
        indent += -result_indent + 10.0;
    }

    // C++ :3932-3935 — the vertical coordinates.
    let title_bounds = app.title_bar.get_global_bounds();
    let title_bar_bottom = title_bounds.top + title_bounds.get_height();
    let mut quad_height = ((h - total_height) / 2.0).max(title_bar_bottom + 60.0);
    let mut txt_height =
        quad_height - font_height * layout::FONT_HEIGHT_OFFSET + double_border + 5.0; // C++ `profFrameSize * 0.5`

    // C++ :3937-3941 — the submenu global offset.
    let mut offset = category.get_offset();
    let panel_offset = layout::calc_menu_offset(
        &mut offset,
        h - quad_height,
        revert_offset,
        false,
        layout::fps_mult(app.fps),
    );
    *category.get_offset_mut() = offset;
    txt_height += panel_offset;
    quad_height += panel_offset;

    // C++ :3943-3954 — the surrounding quads.
    app.menu_quads.clear();
    app.menu_quads.reserve_quad(2);
    create_quad(
        &mut app.menu_quads,
        menu_text_color,
        indent - double_border,
        indent + double_border + text_width,
        quad_height,
        quad_height + total_height,
    );
    create_quad(
        &mut app.menu_quads,
        menu_quad_color,
        indent - 10.0,
        indent + 10.0 + text_width,
        quad_height + 10.0,
        quad_height + total_height - 10.0,
    );
    target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());

    // C++ :3956-3963 — the scrollbar (if needed).
    if scrollbar_notches != 0 {
        draw_scrollbar(
            app,
            total_height - 2.0 * (10.0 + scrollbar_interspace),
            real_size,
            scrollbar_notches,
            indent + text_width - scrollbar_interspace,
            quad_height + 10.0 + scrollbar_interspace,
            shared.scrollbar_offset,
            menu_text_color,
            target,
        );
    }

    // C++ :3965-3989 — the profile rows.
    txt_height += 5.0; // C++ `profFrameSize / 2`
    for i in shared.scrollbar_offset..shared.scrollbar_offset + drawn_size {
        let Some(item) = items.get(i as usize) else {
            continue;
        };
        let name = item.borrow().get_name();
        let selected = i == category.get_idx();

        let mut y_pos = txt_height - (if selected { font_height * 0.75 } else { 0.0 });
        let mut upper = name.clone();
        uppercasify(&mut upper);
        // C++ :3982-3984 — `renderTextCentered(str, font, size, pos,
        // color)` (:3322-3331): sets the char size (30 / 42), renders
        // with `menuTextColor`, restores the fill — NOT the char size
        // (the survival line below reads the current size).
        app.txt_profile
            .set_character_size(if selected { 42 } else { 30 } as u32);
        render_text_centered_color(
            target,
            &mut app.txt_profile,
            &upper,
            Vec2::new(indent + text_width / 2.0, y_pos),
            menu_text_color,
        );

        // C++ :3986-3994 — the survival-time line (the C++'s `data !=
        // nullptr` guard is the seam's `Option`).
        if let Some(total) = shared.assets().profile_total_survival_time(&name) {
            y_pos += (if selected {
                selected_font_height
            } else {
                font_height
            }) * 1.75;
            // C++ `setCharacterSize(getCharacterSize() - 15)` — the name
            // render left 42 / 30 → 27 / 15.
            app.txt_profile
                .set_character_size((if selected { 42 } else { 30 }) - 15);
            render_text_centered(
                target,
                &mut app.txt_profile,
                &format_survival_time(total),
                Vec2::new(indent + text_width / 2.0, y_pos),
            );
        }

        txt_height += interline;
    }

    // C++ :3996-3999 — the deletion instructions below the box (the C++
    // renders the font directly — its string was set by the probe above).
    app.txt_instructions_small.set_position(Vec2::new(
        indent + (text_width - instructions_width) / 2.0,
        quad_height + total_height,
    ));
    target.draw_text(&app.txt_instructions_small, RenderStates::default());
}

/// C++ `MenuGame::drawProfileSelectionBoot()` (MenuGame.cpp:4007-4089) —
/// the boot profile list: the two instructions, the optional scrollbar
/// (the "Total survival time 0000:00" width probe), and the centered
/// name / survival rows. No offsets are eased (the C++ has none).
fn draw_profile_selection_boot(
    app: &mut MenuDraw,
    menu: &MenuGame,
    shared: &Shared,
    target: &mut dyn DrawTarget,
) {
    let w = shared.w;
    let h = shared.h;

    // C++ :4008-4014.
    let category = menu.profile_selection_menu.get_category();
    let items = category.get_items();
    let real_size = items.len() as i32;
    let (scrollbar_notches, drawn_size) =
        layout::scrollbar_notches(real_size, layout::MAX_PROFILES_ON_SCREEN);

    // C++ :4016-4023.
    let font_height = app.h_prof_char;
    let selected_font_height = app.h_prof_selected;
    let interline = 4.0 * font_height;
    let total_height = interline * drawn_size as f32;
    let mut height = (h - total_height) / 2.0 - selected_font_height * 1.5;

    // C++ :4025-4038 — the instructions (clear of the title bar).
    let instructions_height = 1.5 * app.h_instructions_big;
    let title_bounds = app.title_bar.get_global_bounds();
    let title_bar_bottom = title_bounds.top + title_bounds.get_height();
    height = (height - 2.0 * instructions_height).max(title_bar_bottom + 40.0);
    for s in ["SELECT LOCAL PROFILE", "PRESS ESC TO CREATE A NEW PROFILE"] {
        render_text_centered(
            target,
            &mut app.txt_instructions_big,
            s,
            Vec2::new(w / 2.0, height),
        );
        height += instructions_height;
    }
    height += selected_font_height;

    // C++ :4040-4057 — the scrollbar (if needed; the C++'s `White`).
    if scrollbar_notches != 0 {
        let mut width: f32 = 0.0;
        for item in &items {
            let mut name = item.borrow().get_name();
            uppercasify(&mut name);
            app.txt_profile.set_string(&name);
            width = width.max(app.txt_profile.get_global_bounds().get_width());
        }
        app.txt_profile.set_string("Total survival time 0000:00");
        width = width.max(app.txt_profile.get_global_bounds().get_width());
        width += 10.0;

        draw_scrollbar(
            app,
            total_height,
            real_size,
            scrollbar_notches,
            (w + width) / 2.0,
            height,
            shared.scrollbar_offset,
            Color::WHITE,
            target,
        );
    }

    // C++ :4059-4087 — the name / score rows (centered on w/2).
    for i in shared.scrollbar_offset..shared.scrollbar_offset + drawn_size {
        let Some(item) = items.get(i as usize) else {
            continue;
        };
        let name = item.borrow().get_name();
        let selected = i == category.get_idx();

        let mut y_pos = height - (if selected { font_height * 0.75 } else { 0.0 });
        let mut upper = name.clone();
        uppercasify(&mut upper);
        // C++ :4073-4074 — the 4-arg overload (:3314-3318): sets the size
        // and renders with the font's CURRENT fill (the constructor's
        // white — nothing else sets it).
        app.txt_profile
            .set_character_size(if selected { 42 } else { 30 } as u32);
        render_text_centered(
            target,
            &mut app.txt_profile,
            &upper,
            Vec2::new(w / 2.0, y_pos),
        );

        if let Some(total) = shared.assets().profile_total_survival_time(&name) {
            y_pos += (if selected {
                selected_font_height
            } else {
                font_height
            }) * 1.75;
            app.txt_profile
                .set_character_size((if selected { 42 } else { 30 }) - 15);
            render_text_centered(
                target,
                &mut app.txt_profile,
                &format_survival_time(total),
                Vec2::new(w / 2.0, y_pos),
            );
        }

        height += interline;
    }
}

/// C++ `MenuGame::drawEnteringText(xOffset, revertOffset)`
/// (MenuGame.cpp:4091-4145) — the entered-name box: the surrounding
/// quads, the centered name, and the three instructions above the box.
/// `shared` is the caller's `RefMut` — the C++'s `enteringTextOffset`
/// member is eased in place.
fn draw_entering_text(
    app: &mut MenuDraw,
    shared: &mut Shared,
    x_offset: f32,
    revert_offset: bool,
    target: &mut dyn DrawTarget,
) {
    let w = shared.w;
    let h = shared.h;
    let menu_text_color = shared.menu_text_color;
    let menu_quad_color = shared.menu_quad_color;

    // C++ :4093-4098 — the entered text + the box width.
    let mut entered = shared.entered_str.clone();
    uppercasify(&mut entered);
    app.txt_entering_text.set_string(&entered);
    let text_width = 200.0_f32.max(app.txt_entering_text.get_global_bounds().get_width());

    // C++ :4100-4105 — the coordinates (`enteringTextMinWidth` 200 is the
    // floor in the `max` above; `profFrameSize` 10).
    let double_frame = 20.0;
    let indent = ((w + x_offset) * 0.5 - text_width) / 2.0 + 10.0;
    let txt_bottom = app.h_entering_text * 0.45;
    let total_height = app.h_entering_text + txt_bottom + double_frame * 2.0;
    let mut quad_height = shared.menu_half_height - total_height / 2.0;
    let mut txt_height =
        quad_height - app.h_entering_text * layout::FONT_HEIGHT_OFFSET + double_frame;

    // C++ :4108-4112 — the offset (the C++'s `enteringTextOffset` member).
    let panel_offset = layout::calc_menu_offset(
        &mut shared.entering_text_offset,
        h - quad_height,
        revert_offset,
        false,
        layout::fps_mult(app.fps),
    );
    txt_height += panel_offset;
    quad_height += panel_offset;

    // C++ :4114-4121 — the surrounding quads.
    app.menu_quads.clear();
    app.menu_quads.reserve_quad(2);
    create_quad(
        &mut app.menu_quads,
        menu_text_color,
        indent - double_frame,
        indent + double_frame + text_width,
        quad_height,
        quad_height + total_height,
    );
    create_quad(
        &mut app.menu_quads,
        menu_quad_color,
        indent - 10.0,
        indent + 10.0 + text_width,
        quad_height + 10.0,
        quad_height + total_height - 10.0,
    );
    target.draw_vertices(app.menu_quads.as_slice(), RenderStates::default());

    // C++ :4124-4126 — the entered text (the 5-arg centered-offset —
    // save / set / render / restore the fill).
    render_text_centered_offset_color(
        target,
        &mut app.txt_entering_text,
        &entered,
        Vec2::new(text_width / 2.0, txt_height + 5.0),
        indent,
        menu_text_color,
    );

    // C++ :4128-4144 — the instructions above the quads (the 4-arg
    // centered-offset — the fill was just set to `menuQuadColor` and
    // persists across the loop, as in the C++).
    let instructions_height = 1.5 * app.h_instructions_medium;
    txt_height -= 10.0 + instructions_height * 3.0;
    app.txt_instructions_medium.set_fill_color(menu_quad_color);
    for s in ["INSERT TEXT", "PRESS ENTER WHEN DONE", "PRESS ESC TO ABORT"] {
        render_text_centered_offset(
            target,
            &mut app.txt_instructions_medium,
            s,
            Vec2::new(text_width / 2.0, txt_height),
            indent,
        );
        txt_height += instructions_height;
    }
}

/// C++ `MenuGame::drawEnteringTextBoot()` (MenuGame.cpp:4147-4168) — the
/// boot profile-creation screen: the two instructions + the entered name.
fn draw_entering_text_boot(app: &mut MenuDraw, shared: &Shared, target: &mut dyn DrawTarget) {
    let w = shared.w;
    let h = shared.h;

    // C++ :4149.
    let mut height = (h - app.h_entering_text) / 2.0;

    // C++ :4151-4161 — the instructions.
    let instructions_height = 1.5 * app.h_instructions_big;
    height -= instructions_height * 2.0;
    for s in ["PROFILE CREATION", "PLEASE TYPE A NAME AND PRESS ENTER"] {
        render_text_centered(
            target,
            &mut app.txt_instructions_big,
            s,
            Vec2::new(w / 2.0, height),
        );
        height += instructions_height;
    }

    // C++ :4163-4167 — the entered name (the 4-arg centered — sets White,
    // restores the fill).
    height += instructions_height / 2.0;
    let mut entered = shared.entered_str.clone();
    uppercasify(&mut entered);
    render_text_centered_color(
        target,
        &mut app.txt_entering_text,
        &entered,
        Vec2::new(w / 2.0, height),
        Color::WHITE,
    );
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
    // view. issue 025g: `getWindowWidth()` / `getWindowHeight()` are the
    // ACTUAL window size (the C++'s `window.getRenderWindow().getSize()`,
    // MenuGame.cpp:3350-3357 — the `MenuWindow` seam), NOT the config
    // size the port used to read (the config-vs-actual divergence, the
    // 025g device finding).
    let (win_w, win_h) = menu.window_size();
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
    use oh_menu::seams::{FakeAssets, FakeConfig, LoadResults, MenuConfig, NoopNetwork};
    use oh_render::draw::{RenderStates, Vertex};
    use std::rc::Rc;

    /// A `DrawTarget` that records the call stream (type + a short
    /// fingerprint per call) so the tests assert the C++ draw-call ORDER.
    #[derive(Default)]
    struct Recording {
        calls: Vec<String>,
        /// issue 025h — the drawn texts' (string, position) pairs, so the
        /// load-results position pin can read the actual coordinates.
        texts: Vec<(String, Vec2)>,
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
        fn draw_text(&mut self, t: &Text, _s: RenderStates) {
            self.calls.push("text".into());
            self.texts.push((t.get_string().into(), t.get_position()));
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
            Rc::new(oh_menu::NoopTouch),
            oh_menu::FakeWindow::new(),
            None,
            None,
            None,
            None, // `ignore_inputs` — the 010b seam; not exercised by these draw tests
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

    /// Builds a `MenuGame` over `assets` + `config` (the (b) tests register
    /// packs / levels on the `FakeAssets` before handing it in).
    fn menu_with_config(
        assets: Rc<FakeAssets>,
        config: Rc<FakeConfig>,
    ) -> (Rc<MenuGame>, MenuDraw) {
        let network = Rc::new(NoopNetwork);
        let audio = oh_menu::RecordingAudio::new();
        let menu = Rc::new(MenuGame::new(
            assets,
            config,
            network,
            audio,
            Rc::new(oh_menu::NoopTouch),
            oh_menu::FakeWindow::new(),
            None,
            None,
            None,
            None, // `ignore_inputs` — the 010b seam
            Rc::new(oh_menu::layout::LinearFontHeight),
        ));
        let f = font();
        let app = MenuDraw::new(&f, &f);
        (menu, app)
    }

    /// The default-`FakeConfig` variant of [`menu_with_config`].
    fn menu_with(assets: Rc<FakeAssets>) -> (Rc<MenuGame>, MenuDraw) {
        menu_with_config(assets, FakeConfig::new())
    }

    #[test]
    fn loading_screen_draws_load_results_not_background_or_graphics() {
        let (menu, mut app) = menu_and_app();
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::LoadingScreen;
        }
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        let calls = &rec.calls;
        // C++ order: clear → background view (the `mainOrAbove` gate keeps
        // the hexagon background off) → overlay view → `drawLoadResults`
        // (the hexagon + 3 separators as ONE 54-vertex batch; 6 counter
        // texts; 2 random tips; "ERRORS" + count + "NO LOAD ERRORS") →
        // "PRESS ANY KEY..." → the C++ `return` (no screenshot / dialog
        // passes, no `drawGraphics`, no online status).
        assert_eq!(calls[0], "clear");
        assert!(calls[1].starts_with("view("));
        assert!(calls[2].starts_with("view("));
        assert_eq!(calls[3], "verts(54)", "6 hexagon quads + 3 separators");
        let text_count = calls.iter().filter(|c| *c == "text").count();
        assert_eq!(
            text_count, 12,
            "6 counters + 2 tips + 3 error lines + press-any-key"
        );
        assert_eq!(calls.len(), 3 + 1 + text_count, "no other passes");
        assert!(!calls.iter().any(|c| c == "sprite"), "no graphics");
    }

    #[test]
    fn load_results_with_errors_draws_scrollbar_and_clamped_lines() {
        let assets = FakeAssets::new();
        assets.set_load(LoadResults {
            packs: 3,
            levels: 42,
            assets: 7,
            error_messages: (1..=9).map(|i| format!("error {i}")).collect(),
        });
        let (menu, mut app) = menu_with(assets);
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::LoadingScreen;
        }
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        let calls = &rec.calls;
        // C++ :4255-4296 — nine errors > `maxErrorsOnScreen` (7): the
        // scrollbar draws (1 quad = 6 verts) and exactly 7 error lines
        // render (in reverse: "error 9" … "error 3").
        assert_eq!(calls[3], "verts(54)");
        for k in 0..10 {
            assert_eq!(calls[4 + k], "text", "counters + tips + error labels");
        }
        assert_eq!(calls[14], "verts(6)", "the red scrollbar");
        for k in 0..7 {
            assert_eq!(calls[15 + k], "text", "the 7 clamped error lines");
        }
        assert_eq!(calls[22], "text", "press-any-key");
        assert_eq!(calls.len(), 23);
    }

    #[test]
    fn level_selection_draws_both_panels_in_cpp_order() {
        let assets = FakeAssets::new();
        assets.add_pack("pack1", &["A", "B", "C"]);
        let (menu, mut app) = menu_with(assets.clone());
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::LevelSelection;
            s.lvl_slct.level_data_ids = vec!["pack1_A".into(), "pack1_B".into(), "pack1_C".into()];
            s.current_level = Some(assets.level("pack1_A").unwrap());
        }
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        let calls: Vec<String> = rec.calls.clone();
        // C++ :5926-5949 order (the fake has one pack, three levels, no
        // favorites, an offline `NoopNetwork` → no RANKED badges, the
        // "PLEASE LOG IN" leaderboard fallback, no main-menu fold):
        //   clear → bg view → bg verts → overlay view
        //   right: 2 instruction texts; per level one 18-vert batch
        //     (top + side + body) THEN its name + author texts (the C++
        //     renders the quads and reads the level data per level);
        //     the 6-vert bottom frame; the 12-vert pack-label batch +
        //     label text + the 12-vert down arrow (the current pack's)
        //   left: 12-vert backdrop; the level name; the "DIFFICULTY: "
        //     label + its value; the pack block (4); the music block (4);
        //     the 48-vert batch (the difficulty / pack lines created but
        //     only rendered together with the favorite button — the C++
        //     "Also renders all previous quads"); the favorite label;
        //     the personal best label + score; "ONLINE LEADERBOARD";
        //     "PLEASE LOG IN…"; the final 12-vert batch (the two
        //     separator lines, rendered at the C++'s trailing
        //     `render(menuQuads)`)
        //   online status: window view + sprite + rect + text
        assert_eq!(calls[0], "clear");
        assert!(calls[1].starts_with("view("));
        assert!(calls[2].starts_with("verts("), "background");
        assert!(calls[3].starts_with("view("));
        assert_eq!(&calls[4..6], &["text", "text"], "F2 instructions");
        assert_eq!(
            &calls[6..12],
            &["verts(18)", "text", "text", "verts(18)", "text", "text"],
            "level rows 1-2 (quads, then name + author)"
        );
        assert_eq!(
            &calls[12..15],
            &["verts(18)", "text", "text"],
            "level row 3"
        );
        assert_eq!(calls[15], "verts(6)", "bottom frame");
        assert_eq!(calls[16], "verts(12)", "pack-label frame + body");
        assert_eq!(calls[17], "text", "pack label");
        assert_eq!(calls[18], "verts(12)", "the down arrow (current pack)");
        assert_eq!(calls[19], "verts(12)", "left backdrop + border");
        assert_eq!(calls[20], "text", "level name");
        assert_eq!(
            &calls[21..23],
            &["text", "text"],
            "difficulty label + value"
        );
        assert_eq!(
            &calls[23..27],
            &["text", "text", "text", "text"],
            "pack block"
        );
        assert_eq!(
            &calls[27..31],
            &["text", "text", "text", "text"],
            "music block"
        );
        assert_eq!(
            calls[31], "verts(48)",
            "difficulty + pack lines (3 quads) + favorite button (5)"
        );
        assert_eq!(calls[32], "text", "favorite label");
        assert_eq!(&calls[33..35], &["text", "text"], "personal best + score");
        assert_eq!(calls[35], "text", "ONLINE LEADERBOARD");
        assert_eq!(calls[36], "text", "PLEASE LOG IN…");
        assert_eq!(
            calls[37], "verts(12)",
            "the two left-panel lines (trailing render)"
        );
        assert!(calls[38].starts_with("view("), "online status view");
        assert_eq!(&calls[39..42], &["sprite", "rect", "text"], "online badge");
        assert_eq!(calls.len(), 42);
    }

    #[test]
    fn smain_folds_level_selection_panels_when_offset_nonzero() {
        // C++ :5871-5874 — while `levelDetailsOffset != 0` the SMain arm
        // draws the folding right + left panels (reverting) BEFORE the
        // main menu. The first draw runs the C++ `mustRefresh` block (the
        // port's `refresh_if_stale`), which zeroes the offsets — so set
        // them AFTER a first draw, as the C++'s navigation does.
        let assets = FakeAssets::new();
        assets.add_pack("pack1", &["A"]);
        let (menu, mut app) = menu_with(assets.clone());
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::SMain;
            s.lvl_slct.level_data_ids = vec!["pack1_A".into()];
            s.current_level = Some(assets.level("pack1_A").unwrap());
        }
        draw(&menu, &mut app, &mut Recording::default());
        {
            menu.shared().borrow_mut().level_details_offset = 300.0;
        }
        let item_count = menu.main_menu.get_category().get_items().len();
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        let calls: Vec<String> = rec.calls.clone();

        // The fold: one 18-vert level row + its name/author, the 6-vert
        // bottom frame, the 12-vert pack label + text + arrow, then the
        // left panel (12-vert backdrop … the trailing 12-vert batch).
        let level_row = calls
            .iter()
            .position(|c| c == "verts(18)")
            .expect("the folding level row is drawn");
        assert_eq!(
            &calls[level_row..level_row + 11],
            &[
                "verts(18)",
                "text",
                "text",
                "verts(6)",
                "verts(12)",
                "text",
                "verts(12)",
                "verts(12)",
                "text",
                "text",
                "text",
            ],
            "right-side fold + the left backdrop + level name + difficulty"
        );

        // The main-menu quads come AFTER the whole left panel: the fold's
        // left panel ends with its 12-vert trailing batch, then the main
        // menu, then `drawGraphics` (the first sprite after it — the
        // online-status badge sprites come later). So the main-menu batch
        // is the LAST verts batch before the first sprite.
        let graphics = calls
            .iter()
            .position(|c| c == "sprite")
            .expect("drawGraphics runs after the main menu");
        let main_menu = calls[..graphics]
            .iter()
            .rposition(|c| c.starts_with("verts("))
            .expect("the main-menu quad batch");
        assert!(
            main_menu > level_row + 10,
            "the main menu is drawn after the folded panels (row at {level_row}, main menu at {main_menu})"
        );
        let main_quads: usize = calls[main_menu]
            .trim_start_matches("verts(")
            .trim_end_matches(')')
            .parse()
            .expect("vertex count");
        assert_eq!(main_quads / 6, item_count, "one trapezoid quad per item");
    }

    #[test]
    fn level_selection_draws_favorites_drawer_when_active() {
        let assets = FakeAssets::new();
        assets.add_pack("pack1", &["A", "B"]);
        let (menu, mut app) = menu_with(assets.clone());
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::LevelSelection;
            s.favorites_active = true;
            s.fav_slct.level_data_ids = vec!["pack1_A".into()];
            s.lvl_slct.level_data_ids = vec!["pack1_A".into(), "pack1_B".into()];
            s.current_level = Some(assets.level("pack1_A").unwrap());
        }
        // The first draw runs the refresh (which zeroes the offsets, as
        // the C++ `refreshCamera` → `updateLevelSelectionDrawingParameters`);
        // the regular drawer's X-offset is set afterwards, as the C++'s
        // `switchToFromFavoriteLevels` leaves it non-zero while folding.
        draw(&menu, &mut app, &mut Recording::default());
        {
            menu.shared().borrow_mut().lvl_slct.x_offset = 100.0;
        }
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        let calls: Vec<String> = rec.calls.clone();
        // The active (favorites) drawer: one level row + the 6-vert bottom
        // frame + ONE pack label (the "FAVORITES" one — `packsSize == 1`)
        // with its down arrow; then the regular drawer still folds
        // (`XOffset != 0`): 2 level rows + its bottom frame + its pack
        // label + arrow. The left panel is the favorites drawer's.
        let rows: usize = calls.iter().filter(|c| **c == "verts(18)").count();
        assert_eq!(rows, 3, "1 favorites row + 2 folding regular rows");
        let labels: usize = calls.iter().filter(|c| **c == "verts(12)").count();
        // favorites label + favorites arrow + regular label + regular
        // arrow + left backdrop + the two left lines.
        assert_eq!(labels, 6);
    }

    #[test]
    fn format_survival_time_pins_the_cpp_three_cases() {
        // C++ :3847-3878 — "Total survival time " + the padded pieces.
        assert_eq!(format_survival_time(7), "Total survival time 07");
        assert_eq!(format_survival_time(59), "Total survival time 59");
        assert_eq!(format_survival_time(425), "Total survival time 07:05");
        assert_eq!(format_survival_time(3600), "Total survival time 1:00:00");
        assert_eq!(format_survival_time(3661), "Total survival time 1:01:01");
    }

    #[test]
    fn entering_text_boot_draws_instructions_then_name_then_graphics() {
        let assets = FakeAssets::new();
        let (menu, mut app) = menu_with(assets);
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::ETLPNewBoot;
            s.entered_str = "Gary".into();
        }
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        let calls: Vec<String> = rec.calls.clone();
        // C++ :5842-5845 — `drawEnteringTextBoot()` (2 instructions + the
        // entered name) + `drawGraphics()` (3 bars + version); no
        // background below SMain, no online status, no dialog.
        assert_eq!(calls[0], "clear");
        assert!(calls[1].starts_with("view("));
        assert!(
            calls[2].starts_with("view("),
            "no background verts below SMain"
        );
        assert_eq!(
            &calls[3..6],
            &["text", "text", "text"],
            "instructions + name"
        );
        assert_eq!(
            &calls[6..10],
            &["sprite", "sprite", "sprite", "text"],
            "drawGraphics"
        );
        assert_eq!(calls.len(), 10);
    }

    #[test]
    fn profile_selection_boot_draws_instructions_rows_and_graphics() {
        let assets = FakeAssets::new();
        assets.profile_create("Alpha");
        assets.profile_create("Beta");
        let (menu, mut app) = menu_with(assets);
        {
            menu.shared().borrow_mut().state = States::SLPSelectBoot;
        }
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        let calls: Vec<String> = rec.calls.clone();
        // C++ :5847-5850 — `drawProfileSelectionBoot()`: the 2
        // instructions, then per profile the name + the survival line
        // (the fake reports the profiles as existing, 0 s total — no
        // scrollbar for 2 <= maxProfilesOnScreen 6); + `drawGraphics()`.
        assert_eq!(calls[0], "clear");
        assert!(calls[1].starts_with("view("));
        assert!(
            calls[2].starts_with("view("),
            "no background verts below SMain"
        );
        assert_eq!(&calls[3..5], &["text", "text"], "the 2 boot instructions");
        assert_eq!(
            &calls[5..9],
            &["text", "text", "text", "text"],
            "name + survival, x2"
        );
        assert_eq!(
            &calls[9..13],
            &["sprite", "sprite", "sprite", "text"],
            "drawGraphics"
        );
        assert_eq!(calls.len(), 13);
    }

    #[test]
    fn etlpnew_draws_local_profiles_menu_and_entering_box() {
        let assets = FakeAssets::new();
        let (menu, mut app) = menu_with(assets);
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::ETLPNew;
            s.entered_str = "G".into();
        }
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        let calls: Vec<String> = rec.calls.clone();
        let item_count = menu
            .main_menu
            .get_category_by_name("local profiles")
            .get_items()
            .len();
        // C++ :5912-5916 — the `local profiles` main menu (3 items),
        // `drawEnteringText` (the 2-quad box + the name + 3
        // instructions), `drawGraphics()`.
        assert_eq!(calls[0], "clear");
        assert!(calls[1].starts_with("view("));
        assert_eq!(calls[2], "verts(0)", "background (no style)");
        assert!(calls[3].starts_with("view("));
        assert_eq!(calls[4], "text", "the CURRENT PROFILE line");
        let quads: usize = calls[5]
            .trim_start_matches("verts(")
            .trim_end_matches(')')
            .parse()
            .expect("vertex count");
        assert_eq!(quads / 6, item_count, "the local-profiles trapezoids");
        for k in 0..item_count {
            assert_eq!(calls[6 + k], "text", "the item names");
        }
        let base = 6 + item_count;
        assert_eq!(calls[base], "verts(12)", "the entering box (2 quads)");
        assert_eq!(calls[base + 1], "text", "the entered name");
        assert_eq!(
            &calls[base + 2..base + 5],
            &["text", "text", "text"],
            "the 3 instructions"
        );
        assert_eq!(
            &calls[base + 5..base + 9],
            &["sprite", "sprite", "sprite", "text"],
            "drawGraphics"
        );
        assert_eq!(calls.len(), base + 9);
    }

    #[test]
    fn sselect_draws_local_profiles_menu_and_profile_box() {
        let assets = FakeAssets::new();
        assets.profile_create("Alpha");
        assets.profile_create("Beta");
        let (menu, mut app) = menu_with(assets);
        {
            menu.shared().borrow_mut().state = States::SLPSelect;
        }
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        let calls: Vec<String> = rec.calls.clone();
        let item_count = menu
            .main_menu
            .get_category_by_name("local profiles")
            .get_items()
            .len();
        // C++ :5919-5923 — the `local profiles` main menu, then
        // `drawProfileSelection` (the 2-quad box — no scrollbar for 2
        // profiles — the name + survival rows, the deletion
        // instructions), then `drawGraphics()`.
        assert_eq!(calls[0], "clear");
        assert!(calls[1].starts_with("view("));
        assert_eq!(calls[2], "verts(0)");
        assert!(calls[3].starts_with("view("));
        assert_eq!(calls[4], "text", "the CURRENT PROFILE line");
        let quads: usize = calls[5]
            .trim_start_matches("verts(")
            .trim_end_matches(')')
            .parse()
            .expect("vertex count");
        assert_eq!(quads / 6, item_count);
        let base = 6 + item_count;
        assert_eq!(calls[base], "verts(12)", "the profile box (2 quads)");
        assert_eq!(
            &calls[base + 1..base + 5],
            &["text", "text", "text", "text"],
            "name + survival, x2"
        );
        assert_eq!(calls[base + 5], "text", "the deletion instructions");
        assert_eq!(
            &calls[base + 6..base + 10],
            &["sprite", "sprite", "sprite", "text"],
            "drawGraphics"
        );
        assert_eq!(calls.len(), base + 10);
    }

    #[test]
    fn mopts_draws_fold_options_column_and_submenus_in_cpp_order() {
        // C++ :5880-5896 — the MOpts arm: fold the main menu (offset-gated),
        // the `options` right column, the left submenu column, graphics,
        // the online status. `official = true` disables the three
        // official-gated gameplay toggles (the `[OFFICIAL MODE ENABLED]`
        // badges, C++ :3834-3840).
        let assets = FakeAssets::new();
        let config = FakeConfig::new();
        config.set_official(true);
        let (menu, mut app) = menu_with_config(assets, config);
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::MOpts;
        }
        // The first draw runs the C++ `mustRefresh` block (the port's
        // `refresh_if_stale`), which zeroes the category offsets — as the
        // C++'s navigation does, set them afterwards.
        draw(&menu, &mut app, &mut Recording::default());
        *menu.main_menu.get_category().get_offset_mut() = 400.0; // folding
        let gameplay = menu.options_menu.get_category_by_name("gameplay");
        menu.options_menu.set_category(&gameplay); // navigate into the submenu
        menu.update(0.0); // the per-frame `when` predicates (official gate)
        *gameplay.get_offset_mut() = 540.0; // fully open (`indentSmall`)

        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        let calls: Vec<String> = rec.calls.clone();
        assert_eq!(calls[0], "clear");
        assert!(calls[1].starts_with("view("));
        assert_eq!(calls[2], "verts(0)", "background (no style)");
        assert!(calls[3].starts_with("view("));
        assert_eq!(calls[4], "text", "the CURRENT PROFILE line");
        // C++ :5882-5886 — the main-menu fold (5 items).
        assert_eq!(calls[5], "verts(30)");
        assert_eq!(&calls[6..11], &["text"; 5], "the 5 main items");
        // C++ :5888-5890 — the options right column (7 items).
        assert_eq!(calls[11], "verts(42)");
        assert_eq!(&calls[12..19], &["text"; 7], "the 7 options items");
        // C++ :5892-5893 — the left submenu: only the current `gameplay`
        // category draws (the `"options"` category is skipped; the rest
        // are folded): 2 quads, 9 item names, 3 disabled-item badges.
        assert_eq!(calls[19], "verts(12)", "the 2 submenu quads");
        assert_eq!(
            &calls[20..32],
            &["text"; 12],
            "9 items + 3 [OFFICIAL MODE ENABLED] badges"
        );
        // C++ :5894-5895 — `drawGraphics` + `drawOnlineStatus`.
        assert_eq!(&calls[32..36], &["sprite", "sprite", "sprite", "text"]);
        assert!(calls[36].starts_with("view("));
        assert_eq!(&calls[37..40], &["sprite", "rect", "text"]);
        assert_eq!(calls.len(), 40);
    }

    #[test]
    fn monline_draws_fold_and_online_column_with_empty_left_panel() {
        // C++ :5898-5914 — the MOnline arm: the online menu's ONLY category
        // is named `"options"` (C++ :1596), so `drawSubmenusSmall` skips it
        // ("Already drawn before this loop") and the left panel stays
        // empty — the C++'s behaviour for the single-category online menu.
        let (menu, mut app) = menu_and_app();
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::MOnline;
        }
        draw(&menu, &mut app, &mut Recording::default());
        *menu.main_menu.get_category().get_offset_mut() = 400.0; // folding
        menu.update(0.0); // the `when` predicates (offline: CONNECT only)

        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        let calls: Vec<String> = rec.calls.clone();
        assert_eq!(calls[0], "clear");
        assert!(calls[1].starts_with("view("));
        assert_eq!(calls[2], "verts(0)", "background (no style)");
        assert!(calls[3].starts_with("view("));
        assert_eq!(calls[4], "text", "the CURRENT PROFILE line");
        // C++ :5899-5904 — the main-menu fold (5 items).
        assert_eq!(calls[5], "verts(30)");
        assert_eq!(&calls[6..11], &["text"; 5], "the 5 main items");
        // C++ :5906-5908 — the online right column (6 items).
        assert_eq!(calls[11], "verts(36)");
        assert_eq!(&calls[12..18], &["text"; 6], "the 6 online items");
        // C++ :5910-5911 — no left-panel calls follow (`drawSubmenusSmall`
        // skipped the only category).
        assert_eq!(&calls[18..22], &["sprite", "sprite", "sprite", "text"]);
        assert!(calls[22].starts_with("view("));
        assert_eq!(&calls[23..26], &["sprite", "rect", "text"]);
        assert_eq!(calls.len(), 26);
    }

    #[test]
    fn mopts_draw_is_deterministic_for_the_same_state() {
        // The deferred arms' full-recording determinism check (the
        // established pattern): two draws of the same MOpts state match
        // call-for-call, including the offset-eased panels.
        let assets = FakeAssets::new();
        let (menu, mut app) = menu_with(assets);
        {
            let mut s = menu.shared().borrow_mut();
            s.state = States::MOpts;
        }
        draw(&menu, &mut app, &mut Recording::default());
        *menu.main_menu.get_category().get_offset_mut() = 400.0;
        menu.update(0.0);

        let mut a = Recording::default();
        draw(&menu, &mut app, &mut a);
        let mut b = Recording::default();
        draw(&menu, &mut app, &mut b);
        assert_eq!(a.calls, b.calls, "two draws of the same state match");
    }

    #[test]
    fn epilepsy_warning_draws_the_sprite_and_press_any_key() {
        let assets = FakeAssets::new();
        let (menu, mut app) = menu_with(assets);
        {
            menu.shared().borrow_mut().state = States::EpilepsyWarning;
        }
        let mut rec = Recording::default();
        draw(&menu, &mut app, &mut rec);
        let calls: Vec<String> = rec.calls.clone();
        // C++ :5836-5840 — the sprite + the line, then the C++ `return`
        // (no dialog / graphics / online status).
        assert_eq!(calls[0], "clear");
        assert!(calls[1].starts_with("view("));
        assert!(
            calls[2].starts_with("view("),
            "no background verts below SMain"
        );
        assert_eq!(calls[3], "sprite", "epilepsyWarning");
        assert_eq!(calls[4], "text", "PRESS ANY KEY…");
        assert_eq!(calls.len(), 5);
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

    /// issue 025h — the load-results counter positions run the C++
    /// `drawLoadResults` formula (MenuGame.cpp:4207-4242) on the INK font
    /// heights (the C++ `updateHeight` probe: `getGlobalBounds().height`
    /// of "ABCDEFGHILMNOPQRSTUVZ:"), so the value sits BELOW its label at
    /// any window aspect:
    ///
    /// * the phone's actual 1530×678 (025g's wide space: w ≈ 1733.1,
    ///   h = 768, the non-fourByThree loadBig 70 / loadSmall 24 sizes),
    /// * the desktop's 1024×768 (the fourByThree 56/16 sizes), and
    /// * the square-ish 768×768 (maps to the same 1024×768 space).
    ///
    /// Pre-025h the port's probe returned the line height (exactly the
    /// char size for openSquare — `ascent − descent` = 1.0×) instead of
    /// the ink union (≈0.64×): `numbersHeight = bottomHeight −
    /// 2·hLoadBig` then landed the value ~18px ABOVE its label on the
    /// phone — the "PACKS 8OADED" / "ER0RS" overlap the device screenshot
    /// caught.
    #[test]
    fn load_results_counters_value_below_label_at_two_window_sizes() {
        let f = font();
        for (win_w, win_h) in [(1530.0, 678.0), (1024.0, 768.0), (768.0, 768.0)] {
            let window = oh_menu::FakeWindow::new();
            window.set_size(win_w, win_h);
            let menu = Rc::new(MenuGame::new(
                FakeAssets::new(),
                FakeConfig::new(),
                Rc::new(NoopNetwork),
                oh_menu::RecordingAudio::new(),
                Rc::new(oh_menu::NoopTouch),
                window,
                None,
                None,
                None,
                None,
                Rc::new(oh_menu::layout::LinearFontHeight),
            ));
            {
                let mut s = menu.shared().borrow_mut();
                s.state = States::LoadingScreen;
            }
            let mut app = MenuDraw::new(&f, &f);
            let mut rec = Recording::default();
            draw(&menu, &mut app, &mut rec);

            // The C++ formula (MenuGame.cpp:4207-4242), on the
            // `refresh_view` space + the ink probe heights (`font_height`
            // = the C++ `updateHeight`'s `getFontHeight`).
            let (w, h, fbt, _) = layout::refresh_view(win_w, win_h);
            let sizes = layout::char_sizes(fbt);
            let h_ls = font_height(&f, sizes.load_small as u32);
            let h_lb = font_height(&f, sizes.load_big as u32);
            let label_y = h / 2.0 - h / 15.0 + 5.0 - h_ls;
            let value_y = h / 2.0 + h / 15.0 - h_lb * 2.0;

            // The regression itself: the value BELOW its label (clearing
            // the label's ink bottom), not overlapping it.
            assert!(
                value_y >= label_y + h_ls,
                "{win_w}×{win_h}: value y {value_y:.2} must clear the label's ink bottom {label_y:.2}+{h_ls:.2}"
            );

            // `render_text_centered` stores center − width/2; the width at
            // the same char size gives the center back.
            let width_of = |s: &str, size: u32| -> f32 {
                Text::new(f.clone(), s.to_string(), size).measure().0
            };

            // The three counter labels: the C++ `textOffset` centers
            // ((w−3·xOffset)/2, (w−xOffset)/2, (w+xOffset)/2, xOffset =
            // w/4) at the C++ `topHeight`.
            let labels = [
                ("PACKS LOADED", (w - 3.0 * (w / 4.0)) / 2.0),
                ("LEVELS LOADED", (w - (w / 4.0)) / 2.0),
                ("ASSETS LOADED", (w + (w / 4.0)) / 2.0),
            ];
            for (label, cx) in labels {
                let (_, pos) = rec
                    .texts
                    .iter()
                    .find(|(t, _)| t == label)
                    .unwrap_or_else(|| panic!("label {label:?} not drawn at {win_w}×{win_h}"));
                let cx_rec = pos.x + width_of(label, sizes.load_small as u32) / 2.0;
                let ly = pos.y;
                assert!(
                    (cx_rec - cx).abs() < 0.5,
                    "{label}: center x {cx_rec:.2} != C++ {cx:.2} at {win_w}×{win_h}"
                );
                assert!(
                    (ly - label_y).abs() < 0.5,
                    "{label}: y {ly:.2} != C++ topHeight {label_y:.2} at {win_w}×{win_h}"
                );
            }

            // The four "0" counters (packs/levels/assets/errors, all zero
            // on the default `LoadResults`): one per `textOffset` column
            // at the C++ `numbersHeight`.
            let zeroes: Vec<_> = rec.texts.iter().filter(|(t, _)| t == "0").collect();
            assert_eq!(zeroes.len(), 4, "the four 0-counters at {win_w}×{win_h}");
            let cols = [
                (w - 3.0 * (w / 4.0)) / 2.0,
                (w - (w / 4.0)) / 2.0,
                (w + (w / 4.0)) / 2.0,
                (w + 3.0 * (w / 4.0)) / 2.0,
            ];
            for (i, (_, pos)) in zeroes.iter().enumerate() {
                let cx_rec = pos.x + width_of("0", sizes.load_big as u32) / 2.0;
                let col_x = cols[i];
                let cy = pos.y;
                assert!(
                    (cx_rec - col_x).abs() < 0.5,
                    "counter {i}: center x {cx_rec:.2} != C++ {col_x:.2} at {win_w}×{win_h}"
                );
                assert!(
                    (cy - value_y).abs() < 0.5,
                    "counter {i}: y {cy:.2} != C++ numbersHeight {value_y:.2} at {win_w}×{win_h}"
                );
            }

            // The "NO LOAD ERRORS" line: the C++ `bottomHeight +
            // 1.75·txtLoadSmall.height`.
            let (_, none_pos) = rec
                .texts
                .iter()
                .find(|(t, _)| t == "NO LOAD ERRORS")
                .unwrap_or_else(|| panic!("NO LOAD ERRORS not drawn at {win_w}×{win_h}"));
            let none_y = h / 2.0 + h / 15.0 + h_ls * 1.75;
            assert!(
                (none_pos.y - none_y).abs() < 0.5,
                "NO LOAD ERRORS: y {} vs C++ {} at {win_w}×{win_h}",
                none_pos.y,
                none_y
            );
        }
    }
}
