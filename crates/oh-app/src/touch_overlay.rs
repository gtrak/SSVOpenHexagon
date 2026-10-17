//! Issue 31 §2b/§5 — the gameplay touch overlay: a PLATFORM-NEUTRAL
//! state machine that turns a *pointer feed* (`id, x, y, phase` — the
//! spec's §5 shape, the web's pointer event model: `pointerdown /
//! pointermove / pointerup / pointercancel` tracked by `pointerId`, with
//! pointer capture) plus a *view size* into the game's bound inputs
//! (focus / swap / rotation) — the C++'s rotation-only touch controls
//! (HGUpdate.cpp:589) generalized to the focus / swap buttons the C++
//! lacks (the D-034-class sanctioned deviation: a new feature; the
//! tutorial is unpassable on the upstream Android build too,
//! babysteps.lua:167), drawn as half-transparent on-screen buttons in
//! the game's own key-icon aesthetic (HGGraphics.cpp:368-376's
//! offOpacity 90 / onOpacity 255 over `getColorText`).
//!
//! Platform-neutral by construction (spec §5): NO winit, NO
//! `#[cfg(target_os = "android")]` here — the winit composition feeds
//! the pointer events it derives from the existing shared touch sink
//! (the spec's "Android feeds it from the existing shared touch sink"),
//! and the web target (issue 32) will feed it the canvas pointer events
//! directly — the feed is the seam.
//!
//! The §2b mechanics this module implements:
//! * **Per-button refcounting** — a button is held iff ≥1 captured
//!   pointer maps to it; releasing one finger must not clear a button
//!   another finger still holds (the step-2 shape: finger A holds focus
//!   while finger B taps rotate).
//! * **Pointer capture** — a pointer that went down on a button stays
//!   bound to it while it is down (a drag off the button does not
//!   release it), so the release still lands here (a button can never
//!   be left stuck down).
//! * **Consumed-pointer exclusion** — the captured pointer ids are
//!   published for the engine's rotation path to exclude (the §2b
//!   double-count hazard: a finger held on a button must not ALSO count
//!   as a screen-half rotation finger).
//! * **The additive set-if-held apply** (the C++'s
//!   `setInputImplIfFalse` pattern, HGUpdate.cpp:589-597) lives in the
//!   composition (`windowed.rs`'s `GameSlotAdapter::update_input` — the
//!   C++'s touch-controls position, after the keyboard binds' per-tick
//!   re-apply), NOT here: this module only reports held state.

use std::collections::HashMap;

use oh_math::ssvs::Vec2;
use oh_render::font::Text;
use oh_render::texture::Sprite;
use oh_render::{Color, Font};

// ---------------------------------------------------------------------
// The pointer feed (spec §5: `(id, x, y, phase)` + a view size).
// ---------------------------------------------------------------------

/// The feed's phases (the web's pointer event model — spec §5:
/// `pointerdown / pointermove / pointerup / pointercancel`; the
/// `Canceled` fold is the feed SOURCE's job — the winit fold maps
/// `TouchPhase::Canceled` to `Up`, the C++/SFML's "a touch release is a
/// release" (the `TouchEnded` seam note)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// `pointerdown` (winit `TouchPhase::Started`).
    Down,
    /// `pointermove` (winit `TouchPhase::Moved`) — a no-op for the
    /// overlay (pointer capture: a bound pointer keeps its button).
    Moved,
    /// `pointerup` / `pointercancel` (winit `Ended` / `Canceled`).
    Up,
}

/// One pointer feed event, in the VIEW's pixel space (spec §5: the 025g
/// lesson — the view's space, NOT the config's: a 1530×678 phone window
/// with a 1024×768 config; a CSS-sized canvas with a device pixel ratio
/// is the web's same trap).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    /// The `pointerId` (winit's `finger`).
    pub id: u32,
    /// The position (view pixels; unused by `Up`).
    pub position: Vec2,
    pub phase: Phase,
}

// ---------------------------------------------------------------------
// The buttons.
// ---------------------------------------------------------------------

/// The overlay's buttons (spec §2: the bound inputs — focus (the
/// tutorial's step-2 blocker), swap, rotation left/right — + §2.6: the
/// pause/exit affordance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// §2.6 — the way OUT of gameplay (the C++'s Escape → `goToMenu()`;
    /// the port's `trigger_go_to_menu` = the menu's `go_to_menu(true,
    /// false)` contract — 029d1; the C++'s back key does the same on
    /// Android). Edge-triggered (one fire per physical press), not a
    /// held input.
    Exit,
    /// The focus input (the default bind: LShift — the tutorial's
    /// "focused (left shift)" gate, babysteps.lua:167).
    Focus,
    /// The swap input (the config's swap bind — the shipped prose's
    /// "space or middle mouse", the live trigger, not the prose).
    Swap,
    /// The rotation-CCW input (the C++ touch control's left screen
    /// half).
    RotateCCW,
    /// The rotation-CW input (the C++ touch control's right screen half).
    RotateCW,
}

impl Button {
    /// The fixed order (the hit-test priority when the view is too
    /// small for the rectangles to be disjoint + the `held` /
    /// `press_edges` array indices).
    pub const ALL: [Button; 5] = [
        Self::Exit,
        Self::RotateCCW,
        Self::RotateCW,
        Self::Focus,
        Self::Swap,
    ];

    /// The `ALL` index (the `held` / `press_edges` / `consumed` array
    /// positions).
    pub const fn idx(self) -> usize {
        match self {
            Self::Exit => 0,
            Self::RotateCCW => 1,
            Self::RotateCW => 2,
            Self::Focus => 3,
            Self::Swap => 4,
        }
    }
}

// ---------------------------------------------------------------------
// The layout (view pixel space — the 025g lesson).
// ---------------------------------------------------------------------

/// The buttons' half-size: the key-icon textures' native 64×64 (the
/// C++'s `keyIcons` `halfSize = 32`). A touch target wants the full
/// texture size — no C++ scale factor (the C++'s 0.75 hint scale is for
/// keyboard HINTS, not touch targets).
const BUTTON_HALF: f32 = 32.0;
/// The screen-edge padding (a little room beyond the C++'s key-icon
/// `padding = 8 * scaling` — the buttons must not touch the edge).
const PAD: f32 = 12.0;
/// The focus / swap center-to-center gap (the two 64px buttons sit
/// bottom-middle, 80px apart — 16px clear between the rectangles).
const MIDDLE_GAP: f32 = 80.0;

/// The five button centers (view pixel space — the spec §5 "map pointer
/// coordinates to the overlay in the VIEW's space"): two-thumbs — the
/// rotations at the bottom corners (left thumb → CCW, right thumb → CW:
/// the C++'s screen-half rotation's spatial meaning preserved), the
/// focus / swap at the bottom middle (either thumb), the exit at the
/// top-left (the action area stays clear).
#[must_use]
pub fn button_centers(w: f32, h: f32) -> [Vec2; 5] {
    [
        Vec2::new(PAD + BUTTON_HALF, PAD + BUTTON_HALF), // Exit — top-left
        Vec2::new(PAD + BUTTON_HALF, h - PAD - BUTTON_HALF), // RotateCCW — bottom-left
        Vec2::new(w - PAD - BUTTON_HALF, h - PAD - BUTTON_HALF), // RotateCW — bottom-right
        Vec2::new(w * 0.5 - MIDDLE_GAP * 0.5, h - PAD - BUTTON_HALF), // Focus
        Vec2::new(w * 0.5 + MIDDLE_GAP * 0.5, h - PAD - BUTTON_HALF), // Swap
    ]
}

// ---------------------------------------------------------------------
// The state machine.
// ---------------------------------------------------------------------

/// The overlay's state machine (spec §2b/§5): a pointer feed + a view
/// size in; per-button held state (refcounted by pointer id) + press
/// edges out. Pure data (no callbacks, no window) — the composition
/// owns the apply (the bound-input writes) and the drawing.
#[derive(Default)]
pub struct TouchOverlay {
    /// The enable flag (spec §5: gate on "a pointer feed exists" + an
    /// enable flag — the winit composition sets it where the platform
    /// provides the feed; a disabled overlay ignores every event and
    /// reports nothing held).
    enabled: bool,
    /// The gameplay-only gate (spec §2.4/§2.5: the buttons only while
    /// playing — the composition sets it per frame from the game status;
    /// deactivation clears every capture + held flag + edge, so a finger
    /// on a button at the state's end leaves no stuck input for the
    /// next session).
    active: bool,
    view_w: f32,
    view_h: f32,
    /// Pointer capture (spec §5): a `Down` hit on a button binds the
    /// pointer id to that button; the pointer stays bound while it is
    /// down (the web's pointer-capture semantics — a drag off the
    /// button does not release it), so its `Up` still lands here.
    captured: HashMap<u32, Button>,
    /// The per-button held state (the §2b refcount: a button is held iff
    /// ≥1 captured pointer maps to it).
    held: [bool; 5],
    /// The press edges since the last `take_press_edges` (the `Exit`
    /// affordance's fire — once per physical press, the C++'s
    /// notInConsole-wrapped Escape binding's observable).
    press_edges: [bool; 5],
}

impl TouchOverlay {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The enable flag (spec §5's "an enable flag").
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The gameplay-only gate's per-frame refresh (spec §2.4/§2.5).
    /// Deactivation clears every capture + held flag + pending edge —
    /// the C++'s state-transition semantics (the dead game's held touch
    /// must not leak into the next session's inputs).
    pub fn set_active(&mut self, active: bool) {
        if self.active && !active {
            self.captured.clear();
            self.held = [false; 5];
            self.press_edges = [false; 5];
        }
        self.active = active;
    }
    #[must_use]
    pub fn active(&self) -> bool {
        self.active
    }

    /// The view size's per-frame refresh (spec §5: the view's space, not
    /// the config's).
    pub fn set_view(&mut self, w: f32, h: f32) {
        self.view_w = w;
        self.view_h = h;
    }

    /// The feed's per-event half (spec §5): a pointer event in, the
    /// capture / refcount / edge state out. A disabled / inactive
    /// overlay (or a pre-`set_view` zero view) ignores the event — the
    /// spec §2.4 "must not fire during menus" + §5 gate.
    pub fn pointer_event(&mut self, ev: &PointerEvent) {
        if !self.enabled || !self.active || self.view_w <= 0.0 || self.view_h <= 0.0 {
            return;
        }
        match ev.phase {
            Phase::Down => {
                // A pointer already bound (a double `Down` the feed's
                // source should not emit) keeps its button.
                if self.captured.contains_key(&ev.id) {
                    return;
                }
                let Some(btn) = Self::hit_test(ev.position, self.view_w, self.view_h) else {
                    return; // no button under the pointer — not consumed
                };
                self.captured.insert(ev.id, btn);
                if !self.held[btn.idx()] {
                    self.held[btn.idx()] = true;
                    self.press_edges[btn.idx()] = true;
                }
            }
            // Pointer capture (spec §5): a bound pointer keeps its
            // button while it is down — the move is a no-op here.
            Phase::Moved => {}
            Phase::Up => {
                // The §2b refcount: clear the button only when NO other
                // captured pointer still holds it (step 2: finger A on
                // focus while finger B lifts from rotate).
                if let Some(btn) = self.captured.remove(&ev.id) {
                    if !self.captured.values().any(|b| *b == btn) {
                        self.held[btn.idx()] = false;
                    }
                }
            }
        }
    }

    /// The hit-test (the `ALL` order's priority when the view is too
    /// small for the rectangles to be disjoint — deterministic).
    fn hit_test(p: Vec2, w: f32, h: f32) -> Option<Button> {
        let centers = button_centers(w, h);
        Button::ALL
            .iter()
            .find(|b| {
                let c = centers[b.idx()];
                (p.x - c.x).abs() <= BUTTON_HALF && (p.y - c.y).abs() <= BUTTON_HALF
            })
            .copied()
    }

    /// A button's held state (the §2b refcount's result): the
    /// composition's per-tick apply writes the bound input iff this is
    /// true (the additive set-if-held — the composition's
    /// `GameSlotAdapter::update_input`, the C++'s
    /// `setInputImplIfFalse` position).
    #[must_use]
    pub fn held(&self, btn: Button) -> bool {
        self.held[btn.idx()]
    }

    /// The press edges since the last take (the composition consumes
    /// the `Exit` edge — one fire per physical press).
    #[must_use]
    pub fn take_press_edges(&mut self) -> [bool; 5] {
        let edges = self.press_edges;
        self.press_edges = [false; 5];
        edges
    }

    /// The consumed pointer ids (spec §2b's double-count hazard — the
    /// composition's `WindowTouchRenderer::finger_down_positions`
    /// EXCLUDES these from the engine's rotation path: a finger held on
    /// a button must not also count as a screen-half rotation finger).
    #[must_use]
    pub fn captured_ids(&self) -> Vec<u32> {
        self.captured.keys().copied().collect()
    }
}

// ---------------------------------------------------------------------
// The buttons' on-screen drawing.
// ---------------------------------------------------------------------

/// The overlay's on-screen buttons (spec §2.5): the game's own key-icon
/// textures (`keyArrow` / `keyFocus` / `keySwap` — the C++'s
/// `initKeyIcons`, HexagonGame.cpp:100-102; on a phone the C++'s hint
/// row names keys that do not exist, so the overlay reuses the same
/// textures as TOUCH CONTROLS) + the exit's text label (no C++
/// counterpart — the §2.6 affordance is a new feature), half-transparent
/// in the VIEW's pixel space (the C++'s `drawKeyIcons` opacity pair —
/// off 90 / on 255 — over `getColorText`, HGGraphics.cpp:368-376).
///
/// The sprites are PERSISTENT (the GL texture cache's pointer-keyed
/// upload: per-frame texture clones would re-upload every frame and
/// thrash the cache's eviction — the module docs' "stable long-lived
/// textures" note); `draw` re-applies only the per-frame transforms
/// (the positions track the view size) + colors (the held states).
pub struct TouchOverlayButtons {
    rotate_ccw: Sprite,
    rotate_cw: Sprite,
    focus: Sprite,
    swap: Sprite,
    exit: Text,
}

impl TouchOverlayButtons {
    /// The assets' `keyArrow` / `keyFocus` / `keySwap` (the C++'s
    /// `getTextureOrNullTexture`'s 1×1-white fallback when a file is
    /// missing — `bridge::load_texture_or_null`) + the game's regular
    /// font (the `exit` label).
    #[must_use]
    pub fn new(assets_dir: &std::path::Path, font: Font) -> Self {
        let rotate = crate::bridge::load_texture_or_null(assets_dir, "keyArrow.png");
        let focus_tex = crate::bridge::load_texture_or_null(assets_dir, "keyFocus.png");
        let swap_tex = crate::bridge::load_texture_or_null(assets_dir, "keySwap.png");
        Self {
            // The C++'s `keyIconLeft.setRotation(sf::degrees(180))`
            // (HexagonGame.cpp:120): the `keyArrow` texture is the RIGHT
            // arrow — the CCW button is its 180° rotation.
            rotate_ccw: Sprite::new(rotate.clone()).with_rotation(180.0),
            rotate_cw: Sprite::new(rotate),
            focus: Sprite::new(focus_tex),
            swap: Sprite::new(swap_tex),
            // The §2.6 affordance's label: the game's font, the C++'s
            // key-icon off-opacity (it is an action, not a hint).
            exit: Text::new(font, "MENU", 20),
        }
    }

    /// Draw the active buttons on `target` (the caller's frame: after
    /// the game's draw, before the console's paint — a dev overlay may
    /// cover the controls, the controls must not cover the game). The
    /// buttons live in the VIEW's pixel space (the 025g lesson): the
    /// 1:1 view over the whole render target.
    pub fn draw(
        &mut self,
        target: &mut dyn oh_render::graphics::DrawTarget,
        overlay: &TouchOverlay,
        view_w: f32,
        view_h: f32,
        color_text: Color,
    ) {
        // The 1:1 view (the `GlDrawTarget::new`'s initial view — the
        // game's camera view was set by the game's draw; the buttons'
        // space is the window's pixels, the menu's `drawOnlineStatus`
        // identity-view precedent).
        target.set_view(oh_render::view::View::new(
            Vec2::new(view_w * 0.5, view_h * 0.5),
            Vec2::new(view_w, view_h),
        ));

        // The C++'s `drawKeyIcons`'s opacity pair (off 90 / on 255) over
        // `getColorText` — the ON state is the overlay's held state
        // (the button IS the input: a held button glows full).
        let off = Color {
            r: color_text.r,
            g: color_text.g,
            b: color_text.b,
            a: 90,
        };
        let on = Color {
            r: color_text.r,
            g: color_text.g,
            b: color_text.b,
            a: 255,
        };
        let centers = button_centers(view_w, view_h);

        let place = |sprite: &mut Sprite, btn: Button| {
            // The C++'s `setOrigin({halfSize, halfSize})` — center
            // origin, the position = the center.
            sprite.set_origin(Vec2::new(BUTTON_HALF, BUTTON_HALF));
            sprite.set_position(centers[btn.idx()]);
            sprite.set_color(if overlay.held(btn) { on } else { off });
        };
        place(&mut self.rotate_ccw, Button::RotateCCW);
        place(&mut self.rotate_cw, Button::RotateCW);
        place(&mut self.focus, Button::Focus);
        place(&mut self.swap, Button::Swap);
        target.draw_sprite(&self.rotate_ccw);
        target.draw_sprite(&self.rotate_cw);
        target.draw_sprite(&self.focus);
        target.draw_sprite(&self.swap);

        // The exit's label (centered in its 64×64 rect — the `measure`'s
        // width/height, the C++'s center-origin idiom).
        let (tw, th, _) = self.exit.measure();
        self.exit
            .set_position(centers[Button::Exit.idx()] + Vec2::new(-tw * 0.5, -th * 0.5));
        self.exit
            .set_fill_color(if overlay.held(Button::Exit) { on } else { off });
        target.draw_text(&self.exit, oh_render::draw::RenderStates::default());
    }
}

trait SpriteExt {
    fn with_rotation(self, deg: f32) -> Self;
}
impl SpriteExt for Sprite {
    fn with_rotation(mut self, deg: f32) -> Self {
        self.set_rotation(deg);
        self
    }
}

// ---------------------------------------------------------------------
// Tests: the §2b state machine (refcounting, capture, edges, gates,
// hit-test, consumed ids).
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The test view: the spec's phone window (1530×678 — the 025g
    /// "config size is not the window size" example).
    const W: f32 = 1530.0;
    const H: f32 = 678.0;

    fn down(id: u32, x: f32, y: f32) -> PointerEvent {
        PointerEvent {
            id,
            position: Vec2::new(x, y),
            phase: Phase::Down,
        }
    }
    fn moved(id: u32, x: f32, y: f32) -> PointerEvent {
        PointerEvent {
            id,
            position: Vec2::new(x, y),
            phase: Phase::Moved,
        }
    }
    fn up(id: u32) -> PointerEvent {
        PointerEvent {
            id,
            position: Vec2::new(0.0, 0.0),
            phase: Phase::Up,
        }
    }
    fn active_overlay() -> TouchOverlay {
        let mut o = TouchOverlay::new();
        o.set_enabled(true);
        o.set_active(true);
        o.set_view(W, H);
        o
    }
    /// The `Focus` button's center (the layout's bottom-middle-left).
    fn focus_pos() -> Vec2 {
        button_centers(W, H)[Button::Focus.idx()]
    }
    fn ccw_pos() -> Vec2 {
        button_centers(W, H)[Button::RotateCCW.idx()]
    }
    fn cw_pos() -> Vec2 {
        button_centers(W, H)[Button::RotateCW.idx()]
    }
    fn swap_pos() -> Vec2 {
        button_centers(W, H)[Button::Swap.idx()]
    }
    fn exit_pos() -> Vec2 {
        button_centers(W, H)[Button::Exit.idx()]
    }

    #[test]
    fn press_release_drives_the_held_state() {
        let mut o = active_overlay();
        assert!(!o.held(Button::Focus));
        o.pointer_event(&down(1, focus_pos().x, focus_pos().y));
        assert!(o.held(Button::Focus));
        assert_eq!(o.captured_ids(), vec![1]);
        o.pointer_event(&up(1));
        assert!(!o.held(Button::Focus));
        assert!(o.captured_ids().is_empty());
    }

    /// The §2b multi-touch refcount: two fingers on ONE button — the
    /// first release keeps it held, the second clears it.
    #[test]
    fn the_refcount_keeps_the_button_held_while_any_finger_holds_it() {
        let mut o = active_overlay();
        o.pointer_event(&down(1, focus_pos().x, focus_pos().y));
        o.pointer_event(&down(2, focus_pos().x, focus_pos().y));
        assert_eq!(o.captured_ids().len(), 2);
        o.pointer_event(&up(1));
        assert!(
            o.held(Button::Focus),
            "releasing one finger must not clear a button another still holds"
        );
        o.pointer_event(&up(2));
        assert!(!o.held(Button::Focus));
    }

    /// The step-2 shape: finger A holds focus while finger B taps
    /// rotate — BOTH register simultaneously, and each release clears
    /// only its own button.
    #[test]
    fn focus_and_rotate_register_simultaneously() {
        let mut o = active_overlay();
        o.pointer_event(&down(1, focus_pos().x, focus_pos().y));
        o.pointer_event(&down(2, ccw_pos().x, ccw_pos().y));
        assert!(o.held(Button::Focus));
        assert!(o.held(Button::RotateCCW));
        assert!(!o.held(Button::RotateCW));
        assert!(!o.held(Button::Swap));
        o.pointer_event(&up(2)); // the rotate tap lifts
        assert!(o.held(Button::Focus), "the focus finger still holds");
        assert!(!o.held(Button::RotateCCW));
        o.pointer_event(&up(1));
        assert!(!o.held(Button::Focus));
    }

    /// Pointer capture (spec §5): a finger that drags OFF its button
    /// stays bound (the move is a no-op) — the release still lands on
    /// the button (a button can never be left stuck down).
    #[test]
    fn a_drag_off_the_button_stays_captured() {
        let mut o = active_overlay();
        o.pointer_event(&down(1, focus_pos().x, focus_pos().y));
        // Drag far away (past every button, even past the screen).
        o.pointer_event(&moved(1, 0.0, 0.0));
        o.pointer_event(&moved(1, 99999.0, 99999.0));
        assert!(
            o.held(Button::Focus),
            "capture: the bound pointer keeps its button through the drag"
        );
        o.pointer_event(&up(1));
        assert!(!o.held(Button::Focus));
    }

    /// A double `Down` for one pointer keeps the first button (the feed
    /// source's contract is one `Down` per pointer-down; the machine
    /// must not double-capture).
    #[test]
    fn a_repeat_down_keeps_the_first_capture() {
        let mut o = active_overlay();
        o.pointer_event(&down(1, focus_pos().x, focus_pos().y));
        o.pointer_event(&down(1, ccw_pos().x, ccw_pos().y));
        assert!(o.held(Button::Focus));
        assert!(!o.held(Button::RotateCCW));
        o.pointer_event(&up(1));
        assert!(!o.held(Button::Focus));
    }

    /// The `Up` of an unbound pointer (a finger that never hit a button)
    /// is a no-op (nothing captured, nothing released).
    #[test]
    fn an_up_for_an_unbound_pointer_is_a_noop() {
        let mut o = active_overlay();
        o.pointer_event(&down(1, W * 0.5, H * 0.5)); // mid-screen: no button
        assert!(o.captured_ids().is_empty(), "a no-hit down is not consumed");
        o.pointer_event(&up(1));
        assert!(!o.held(Button::Focus));
    }

    #[test]
    fn the_exit_edge_fires_once_per_press() {
        let mut o = active_overlay();
        o.pointer_event(&down(1, exit_pos().x, exit_pos().y));
        let edges = o.take_press_edges();
        assert!(edges[Button::Exit.idx()], "the press edge records");
        // Still held: no second edge (the C++'s Escape binding fires
        // once per press, not per tick).
        assert!(!o.take_press_edges()[Button::Exit.idx()]);
        o.pointer_event(&up(1));
        // A fresh press is a fresh edge.
        o.pointer_event(&down(1, exit_pos().x, exit_pos().y));
        assert!(o.take_press_edges()[Button::Exit.idx()]);
    }

    #[test]
    fn the_level_buttons_have_press_edges_too() {
        let mut o = active_overlay();
        o.pointer_event(&down(1, swap_pos().x, swap_pos().y));
        let edges = o.take_press_edges();
        assert!(edges[Button::Swap.idx()]);
        // A second finger on the same button: NO new edge (the button is
        // already held — the refcount's second increment is not a press).
        o.pointer_event(&down(2, swap_pos().x, swap_pos().y));
        assert!(!o.take_press_edges()[Button::Swap.idx()]);
    }

    /// The gates: disabled / inactive / pre-view overlays ignore every
    /// event (spec §2.4: "must not fire during menus"; §5's enable
    /// flag).
    #[test]
    fn the_gates_ignore_events() {
        // Disabled (the desktop's enable flag — §4.4: no desktop overlay).
        let mut o = TouchOverlay::new();
        o.set_active(true);
        o.set_view(W, H);
        o.pointer_event(&down(1, focus_pos().x, focus_pos().y));
        assert!(!o.held(Button::Focus));

        // Inactive (a menu / boot / results screen — the composition's
        // per-frame gate).
        let mut o = TouchOverlay::new();
        o.set_enabled(true);
        o.set_view(W, H);
        o.pointer_event(&down(1, focus_pos().x, focus_pos().y));
        assert!(!o.held(Button::Focus));

        // Pre-view (the zero view — the composition sets it from the
        // render size every frame).
        let mut o = TouchOverlay::new();
        o.set_enabled(true);
        o.set_active(true);
        o.pointer_event(&down(1, focus_pos().x, focus_pos().y));
        assert!(!o.held(Button::Focus));
    }

    /// Deactivation clears every capture + held flag + edge (the state
    /// transition's semantics — no stuck input leaks into the next
    /// session).
    #[test]
    fn deactivation_clears_every_capture() {
        let mut o = active_overlay();
        o.pointer_event(&down(1, focus_pos().x, focus_pos().y));
        o.pointer_event(&down(2, exit_pos().x, exit_pos().y));
        assert!(o.held(Button::Focus));
        o.set_active(false);
        assert!(!o.held(Button::Focus));
        assert!(!o.held(Button::Exit));
        assert!(o.captured_ids().is_empty());
        assert!(!o.take_press_edges().iter().any(|e| *e));
        // The physically-still-down fingers' `Up`s are no-ops now.
        o.pointer_event(&up(1));
        o.pointer_event(&up(2));
        assert!(!o.held(Button::Focus));
        // Re-activation is clean: a fresh press is required.
        o.set_active(true);
        assert!(!o.held(Button::Focus));
        o.pointer_event(&down(1, focus_pos().x, focus_pos().y));
        assert!(o.held(Button::Focus));
    }

    /// The hit-test: each corner / middle position lands on its button;
    /// the mid-screen (the play area) lands on none.
    #[test]
    fn the_layout_maps_each_position_to_its_button() {
        let mut o = active_overlay();
        o.pointer_event(&down(1, exit_pos().x, exit_pos().y));
        o.pointer_event(&down(2, ccw_pos().x, ccw_pos().y));
        o.pointer_event(&down(3, cw_pos().x, cw_pos().y));
        o.pointer_event(&down(4, focus_pos().x, focus_pos().y));
        o.pointer_event(&down(5, swap_pos().x, swap_pos().y));
        assert!(o.held(Button::Exit));
        assert!(o.held(Button::RotateCCW));
        assert!(o.held(Button::RotateCW));
        assert!(o.held(Button::Focus));
        assert!(o.held(Button::Swap));
        // The play area: no button (a mid-screen finger is a rotation
        // finger for the C++'s screen-half path — NOT consumed).
        o.pointer_event(&down(6, W * 0.5, H * 0.5));
        assert_eq!(
            o.captured_ids().len(),
            5,
            "the mid-screen finger is not consumed"
        );
        assert!(!o.captured_ids().contains(&6));
    }

    /// The view-space mapping (the 025g lesson): the SAME pointer
    /// coordinates land on different buttons for different view sizes —
    /// the overlay tracks the VIEW, not the config.
    #[test]
    fn the_hit_test_tracks_the_view_not_the_config() {
        let mut o = active_overlay();
        // A tiny view: the bottom-middle `Focus` center is at
        // `w * 0.5 - 40` — for `w = 400`, the `CCW` center (44) and the
        // `Focus` center (160) are 116 apart (disjoint); the bottom-LEFT
        // corner is still `CCW`'s.
        o.set_view(400.0, 300.0);
        let ccw_small = button_centers(400.0, 300.0)[Button::RotateCCW.idx()];
        o.pointer_event(&down(1, ccw_small.x, ccw_small.y));
        assert!(o.held(Button::RotateCCW));
    }
}
