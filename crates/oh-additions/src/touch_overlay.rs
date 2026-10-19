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
//! * **The three gates** (spec §2.4/§2.5 + issue 039 F31 + issue 054
//!   F36): the gameplay buttons answer to `active` (playing only), the
//!   action buttons to `action_active` (the whole game state) — except
//!   `Exit`, which additionally answers to `menu_active` (the menu's
//!   back-relevant states: the C++'s `Tid::Exit` bind's menu-side
//!   reachability, the `exit_action` body's touch counterpart). A
//!   button's deactivation clears its capture / held flag / edge only
//!   while its own gate is off (the gate-scoped `clear_now_off` — the
//!   feed order (`set_active` → `set_action_active` →
//!   `set_menu_active`) makes the `Exit`'s OR gate survive the
//!   menu→game hand-off and clear with the action class at the
//!   game→menu one — a capture cannot be held across the latter, whose
//!   switch is the `Exit` press's own effect).

use std::collections::HashMap;

use oh_math::ssvs::Vec2;
use oh_render::font::Text;
use oh_render::texture::Sprite;
use oh_render::{load_texture_or_null, Color, Font};

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
/// pause/exit affordance; issue 054 F36: the `Exit`'s menu-state half).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// §2.6 — the way OUT of gameplay (the C++'s Escape → `goToMenu()`;
    /// the port's `trigger_go_to_menu` = the menu's `go_to_menu(true,
    /// false)` contract — 029d1; the C++'s back key does the same on
    /// Android). Edge-triggered (one fire per physical press), not a
    /// held input.
    ///
    /// issue 054 (F36) — the menu-state half of the same affordance: in
    /// the menu slot's back-relevant states (`menu_exit_relevant` — the
    /// `menu_active` gate), the button drives the menu's `exit_action()`
    /// — the C++'s `Tid::Exit` bind's body (MenuGame.cpp:2457-2502),
    /// with the bind's own `isEnteringText()` guard (MenuGame.cpp:
    /// 975-983) — the touch counterpart of the `T` key / `Escape` /
    /// joystick-Exit bind's menu reachability (the C++'s shipped grid
    /// maps only left / right / ok / up / down — the menu's "back" had
    /// no touch path at all, the operator's "i never see the menu
    /// button", the F36 register row). One button, two slots: the game
    /// state's `Tid::Exit` (the `trigger_go_to_menu`'s contract) and the
    /// menu state's `exit_action`.
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
    /// issue 039 (F31) — the C++ `Tid::Replay` body (HexagonGame.cpp:
    /// 399-407 — `MustReplay`, the `death_input_ignore` + `has_died`
    /// gate / the `in_replay` half): the game-over screen's "PRESS Y TO
    /// REPLAY" affordance. Edge-triggered (the C++'s `Type::Once`).
    Replay,
    /// issue 039 (F31) — the C++ `Tid::Restart` body (HexagonGame.cpp:
    /// 389-397 — `MustRestart`, the `death_input_ignore <= 0 && has_died`
    /// gate): the game-over screen's "PRESS SPACE ... TO RESTART"
    /// affordance. Edge-triggered.
    Restart,
    /// issue 039 (F31) — the C++ `Tid::Pause` body (HexagonGame.cpp:
    /// 421-441 — the DEBUG-gated `debugPause` toggle: a no-op in the
    /// release build, exactly as the keyboard's `F2` is; the button
    /// carries the bind's reachability, not a behaviour the keyboard
    /// lacks). Edge-triggered, and — like the C++'s `Tid::Pause` bind —
    /// NOT console-gated (its sibling `Tid::Restart` / `Tid::Replay`
    /// binds ARE `notInConsole`-wrapped, HexagonGame.cpp:385-407).
    Pause,
}

impl Button {
    /// The fixed order (the hit-test priority when the view is too
    /// small for the rectangles to be disjoint + the `held` /
    /// `press_edges` array indices).
    pub const ALL: [Button; 8] = [
        Self::Exit,
        Self::RotateCCW,
        Self::RotateCW,
        Self::Focus,
        Self::Swap,
        Self::Replay,
        Self::Restart,
        Self::Pause,
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
            Self::Replay => 5,
            Self::Restart => 6,
            Self::Pause => 7,
        }
    }

    /// issue 039 (F31) — the button's CLASS: the `action` buttons
    /// (`Exit` / `Replay` / `Restart` / `Pause`) answer to the
    /// `action_active` gate (the whole game state — playing, game-over
    /// and replay playback: the keyboard binds' reachability, the C++
    /// HexagonGame.cpp:385-441), the `gameplay` buttons (`RotateCCW`
    /// / `RotateCW` / `Focus` / `Swap`) to the `active` gate (playing
    /// only — the spec §2.4/§2.5 half).
    pub const fn is_action(self) -> bool {
        matches!(
            self,
            Self::Exit | Self::Replay | Self::Restart | Self::Pause
        )
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
/// bottom-middle, 80px apart — 16px clear between the rectangles). The
/// F31 action trio reuses the gap at the TOP (the `Replay` / `Restart`
/// pair top-middle, `Pause` top-right): the layout's two rows — the
/// gameplay row at the bottom (the two thumbs), the action row at the
/// top.
const MIDDLE_GAP: f32 = 80.0;

/// The eight button centers (view pixel space — the spec §5 "map
/// pointer coordinates to the overlay in the VIEW's space"): the
/// action row on top (the F31 trio + the exit at the top-left), the
/// gameplay row at the bottom (two-thumbs — the rotations at the bottom
/// corners (left thumb → CCW, right thumb → CW: the C++'s screen-half
/// rotation's spatial meaning preserved), the focus / swap at the bottom
/// middle (either thumb), the exit at the top-left (the action area
/// stays clear)).
#[must_use]
pub fn button_centers(w: f32, h: f32) -> [Vec2; 8] {
    [
        Vec2::new(PAD + BUTTON_HALF, PAD + BUTTON_HALF), // Exit — top-left
        Vec2::new(PAD + BUTTON_HALF, h - PAD - BUTTON_HALF), // RotateCCW — bottom-left
        Vec2::new(w - PAD - BUTTON_HALF, h - PAD - BUTTON_HALF), // RotateCW — bottom-right
        Vec2::new(w * 0.5 - MIDDLE_GAP * 0.5, h - PAD - BUTTON_HALF), // Focus
        Vec2::new(w * 0.5 + MIDDLE_GAP * 0.5, h - PAD - BUTTON_HALF), // Swap
        // issue 039 (F31) — the action row (top): the `Replay` /
        // `Restart` pair top-middle (the focus / swap x's), `Pause`
        // top-right (the `RotateCW` mirror).
        Vec2::new(w * 0.5 - MIDDLE_GAP * 0.5, PAD + BUTTON_HALF), // Replay
        Vec2::new(w * 0.5 + MIDDLE_GAP * 0.5, PAD + BUTTON_HALF), // Restart
        Vec2::new(w - PAD - BUTTON_HALF, PAD + BUTTON_HALF),      // Pause
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
    /// The gameplay-only gate (spec §2.4/§2.5: the gameplay buttons —
    /// the rotations / focus / swap — only while playing: the
    /// composition sets it per frame from the game status; deactivation
    /// clears every GAMEPLAY capture + held flag + edge, so a finger on
    /// a button at the state's end leaves no stuck input for the next
    /// session).
    active: bool,
    /// issue 039 (F31) — the action-button gate (the `Exit` / `Replay`
    /// / `Restart` / `Pause` buttons): the whole GAME state — playing,
    /// game-over (the `has_died` results screen) and replay playback:
    /// the C++'s `Tid::Exit` / `Tid::Restart` / `Tid::Replay` /
    /// `Tid::Pause` keyboard binds stay live in every one of those
    /// states (HexagonGame.cpp:385-441), so the touch affordance does
    /// too. Deactivation clears the class's captures / held flags /
    /// edges where the button's gate is now off (the gate-scoped
    /// `clear_now_off` — the `active` half's clears never touch this
    /// class, and vice versa — the two gates' clears are class-scoped,
    /// the `clear_now_off` half).
    action_active: bool,
    /// issue 054 (F36) — the menu-state's half of the `Exit` (MENU)
    /// button's gate: the C++'s `Tid::Exit` bind's menu-side reachability
    /// (the `exitAction` body, MenuGame.cpp:2457-2502, with the bind's
    /// `isEnteringText()` guard, MenuGame.cpp:975-983) — the composition
    /// feeds it from the `GameSlot::Menu`'s `menu_exit_relevant` states
    /// (the states where `exit_action` is a real "back": level /
    /// options / online / profile selection + its boot arm; NOT `SMain`
    /// — the "main" category's no-op — NOT the entering-text guard, NOT
    /// the boot screens' beep-only no-ops: a dead button is not an
    /// affordance). `Button::Exit` answers to `action_active ||
    /// menu_active` (the game's whole state OR the menu's back states —
    /// one button, two slots); the other action buttons (`Replay` /
    /// `Restart` / `Pause`) answer to `action_active` alone (they have
    /// no menu-side body). Deactivation clears the `Exit` capture / held
    /// flag / edge only where `action_active` is off too (the gate-
    /// scoped `clear_now_off`'s menu half — the game→menu transition
    /// keeps a finger on the MENU button captured while the menu gate
    /// takes over, the transition's semantics).
    menu_active: bool,
    view_w: f32,
    view_h: f32,
    /// Pointer capture (spec §5): a `Down` hit on a button binds the
    /// pointer id to that button; the pointer stays bound while it is
    /// down (the web's pointer-capture semantics — a drag off the
    /// button does not release it), so its `Up` still lands here.
    captured: HashMap<u32, Button>,
    /// The per-button held state (the §2b refcount: a button is held iff
    /// ≥1 captured pointer maps to it).
    held: [bool; 8],
    /// The press edges since the last `take_press_edges` (the action
    /// buttons' fires — once per physical press, the C++'s `Type::Once`
    /// binds' observable).
    press_edges: [bool; 8],
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
    /// Deactivation clears the GAMEPLAY captures + held flags + pending
    /// edges (the gate-scoped `clear_now_off` — the action / menu
    /// buttons' state is their gates', untouched here) — the C++'s
    /// state-transition semantics (the dead game's held touch must not
    /// leak into the next session's inputs).
    pub fn set_active(&mut self, active: bool) {
        let was = self.active;
        self.active = active;
        // The flag moves FIRST: the gate-scoped clear reads the NEW
        // values (a gate whose deactivation triggered the clear must
        // read as off in it — the class-scoped `clear_class`'s
        // unconditional drop is the gate check's post-flip form).
        if was && !active {
            self.clear_now_off();
        }
    }
    #[must_use]
    pub fn active(&self) -> bool {
        self.active
    }

    /// issue 039 (F31) — the action-button gate's per-frame refresh (the
    /// whole game state — the composition's `GameSlot::Game && started`
    /// feed). Deactivation clears the class's captures / held flags /
    /// edges where the button's gate is now off (the gate-scoped
    /// `clear_now_off` — the gameplay buttons' state is the `active`
    /// gate's, untouched here; the `Exit`'s capture survives into the
    /// menu-state half when `menu_active` is on, the gate's own OR).
    pub fn set_action_active(&mut self, active: bool) {
        let was = self.action_active;
        self.action_active = active;
        // The flag moves FIRST (the `set_active`'s note): the clear reads
        // the new `action_active` (off) with the other gates' current
        // values — the `Exit`'s capture survives the hand-off while
        // `menu_active` holds it (the gate's own OR), the rest of the
        // action class clears.
        if was && !active {
            self.clear_now_off();
        }
    }
    #[must_use]
    pub fn action_active(&self) -> bool {
        self.action_active
    }

    /// issue 054 (F36) — the menu-state's gate's per-frame refresh (the
    /// composition's `GameSlot::Menu && menu_exit_relevant` feed — the
    /// `Exit` button's menu-side reachability, the `Tid::Exit` bind's
    /// touch counterpart; the field's docs). Deactivation clears the
    /// `Exit`'s capture / held flag / edge only where `action_active` is
    /// off too (the gate-scoped `clear_now_off`'s menu half — leaving
    /// the game state's `action_active` half untouched).
    pub fn set_menu_active(&mut self, active: bool) {
        let was = self.menu_active;
        self.menu_active = active;
        // The flag moves FIRST (the `set_active`'s note): the clear reads
        // the new `menu_active` (off) with `action_active`'s current
        // value — the `Exit`'s capture survives into the game state
        // while the action gate holds it (the gate's own OR).
        if was && !active {
            self.clear_now_off();
        }
    }
    #[must_use]
    pub fn menu_active(&self) -> bool {
        self.menu_active
    }

    /// The per-button gate (the class split + the F36 menu half): the
    /// gameplay buttons answer to `active`, the action buttons to
    /// `action_active` — except `Button::Exit`, which additionally
    /// answers to `menu_active` (the menu state's `Tid::Exit`
    /// affordance, F36 — one button, two slots).
    fn gate_on(&self, btn: Button) -> bool {
        match btn {
            Button::Exit => self.action_active || self.menu_active,
            b if b.is_action() => self.action_active,
            _ => self.active,
        }
    }

    /// The gate-scoped deactivation clear (the F31 class-scoped clear's
    /// generalization — F36's `Exit` answers to two gates, so "the
    /// class's clear" is "every button whose gate is now off"): drop
    /// the button's captures, clear its held flag where no other
    /// captured pointer still holds it (the §2b refcount), and drop its
    /// pending edge. A gate's deactivation only clears its own class's
    /// buttons (a button belongs to exactly ONE class, and a class is
    /// off only when every gate of its buttons is off — the `Exit`
    /// survives the action→menu hand-off while `menu_active` holds it).
    fn clear_now_off(&mut self) {
        for btn in Button::ALL {
            if self.gate_on(btn) {
                continue;
            }
            self.captured.retain(|_, b| *b != btn);
            if !self.captured.values().any(|b| *b == btn) {
                self.held[btn.idx()] = false;
            }
            self.press_edges[btn.idx()] = false;
        }
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
        if !self.enabled || self.view_w <= 0.0 || self.view_h <= 0.0 {
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
                // issue 039 (F31) + issue 054 (F36) — the per-button
                // gate: a gameplay button only while the `active` gate
                // is on (spec §2.4/§2.5 — "must not fire during menus"),
                // an action button only while its gate is on (`Exit`: the
                // whole game state OR the menu's back states, F36's
                // `menu_active` half; the rest: the whole game state —
                // the C++'s keyboard binds' reachability, HexagonGame.
                // cpp:385-441; the menu's `Tid::Exit` reachability,
                // MenuGame.cpp:2457-2502 + :975-983). A `Down` on a
                // button whose gate is off is NOT consumed.
                if !self.gate_on(btn) {
                    return;
                }
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
    /// the action buttons' edges — one fire per physical press, the
    /// C++'s `Type::Once` binds' observable).
    #[must_use]
    pub fn take_press_edges(&mut self) -> [bool; 8] {
        let edges = self.press_edges;
        self.press_edges = [false; 8];
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
    /// issue 039 (F31) — the action trio's labels (no C++ key-icon
    /// texture exists for replay / restart / pause — the `Exit` label's
    /// text affordance, the game's font; 16 pt: the 64 px box fits
    /// `PAUSE` / `REPLAY`, and `RESTART` overflows it by a couple of
    /// px (the label is centred on the button, the overflow reads)).
    replay: Text,
    restart: Text,
    pause: Text,
}

impl TouchOverlayButtons {
    /// The assets' `keyArrow` / `keyFocus` / `keySwap` (the C++'s
    /// `getTextureOrNullTexture`'s 1×1-white fallback when a file is
    /// missing — `bridge::load_texture_or_null`) + the game's regular
    /// font (the `exit` label).
    #[must_use]
    pub fn new(assets_dir: &std::path::Path, font: Font) -> Self {
        let rotate = load_texture_or_null(assets_dir, "keyArrow.png");
        let focus_tex = load_texture_or_null(assets_dir, "keyFocus.png");
        let swap_tex = load_texture_or_null(assets_dir, "keySwap.png");
        Self {
            // The C++'s `keyIconLeft.setRotation(sf::degrees(180))`
            // (HexagonGame.cpp:120): the `keyArrow` texture is the RIGHT
            // arrow — the CCW button is its 180° rotation.
            rotate_ccw: Sprite::new(rotate.clone()).with_rotation(180.0),
            rotate_cw: Sprite::new(rotate),
            focus: Sprite::new(focus_tex),
            swap: Sprite::new(swap_tex),
            // issue 039 (F31) — the action trio (the `Tid::Replay` /
            // `Tid::Restart` / `Tid::Pause` affordances' labels; the
            // button IS the input, so the label is the bind's NAME —
            // the end-of-level hint's "PRESS REPLAY TO REPLAY" reads
            // the same vocabulary, the Android hint half's F31 line).
            // `Font` is not `Copy`: the label texts take ownership, so
            // the shared handle is cloned for the trio and moved into
            // the last label (`exit` — the literal's order is free).
            replay: Text::new(font.clone(), "REPLAY", 16),
            restart: Text::new(font.clone(), "RESTART", 16),
            pause: Text::new(font.clone(), "PAUSE", 16),
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

        // issue 039 (F31) + issue 054 (F36) — the per-gate visibility:
        // the gameplay row (the rotations / focus / swap) only under the
        // `active` gate (playing only — the spec §2.4/§2.5 half), the
        // action row under the action gates — the `Exit` (MENU) label
        // under EITHER action gate (F36's `menu_active` half: the menu's
        // back states, the `Tid::Exit` bind's touch counterpart), the
        // replay / restart / pause trio under the game-state
        // `action_active` alone (they have no menu-side body — the
        // game-over screen's controls stay drawn over the
        // still-rendered game screen, the C++'s `has_died` state's "the
        // game screen is the results screen's background" half).
        let gameplay = overlay.active();
        let action = overlay.action_active();
        let exit = action || overlay.menu_active();

        let place = |sprite: &mut Sprite, btn: Button| {
            // The C++'s `setOrigin({halfSize, halfSize})` — center
            // origin, the position = the center.
            sprite.set_origin(Vec2::new(BUTTON_HALF, BUTTON_HALF));
            sprite.set_position(centers[btn.idx()]);
            sprite.set_color(if overlay.held(btn) { on } else { off });
        };
        let place_label = |label: &mut Text, btn: Button| {
            // The `Exit` label's centering (the `measure`'s width /
            // height, the C++'s center-origin idiom).
            let (tw, th, _) = label.measure();
            label.set_position(centers[btn.idx()] + Vec2::new(-tw * 0.5, -th * 0.5));
            label.set_fill_color(if overlay.held(btn) { on } else { off });
        };

        if gameplay {
            place(&mut self.rotate_ccw, Button::RotateCCW);
            place(&mut self.rotate_cw, Button::RotateCW);
            place(&mut self.focus, Button::Focus);
            place(&mut self.swap, Button::Swap);
            target.draw_sprite(&self.rotate_ccw);
            target.draw_sprite(&self.rotate_cw);
            target.draw_sprite(&self.focus);
            target.draw_sprite(&self.swap);
        }

        if exit {
            place_label(&mut self.exit, Button::Exit);
            target.draw_text(&self.exit, oh_render::draw::RenderStates::default());
        }
        if action {
            // issue 039 (F31) — the action trio's labels (the `Tid`
            // vocabulary the end-of-level hint's Android half reads,
            // F31's "PRESS REPLAY TO REPLAY" line).
            place_label(&mut self.replay, Button::Replay);
            target.draw_text(&self.replay, oh_render::draw::RenderStates::default());
            place_label(&mut self.restart, Button::Restart);
            target.draw_text(&self.restart, oh_render::draw::RenderStates::default());
            place_label(&mut self.pause, Button::Pause);
            target.draw_text(&self.pause, oh_render::draw::RenderStates::default());
        }
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
        o.set_action_active(true);
        o.set_view(W, H);
        o
    }
    /// The issue 039 (F31) game-over shape: the gameplay gate off
    /// (the `has_died` state — the rotations / focus / swap are gone)
    /// while the action gate stays on (the `Exit` / `Replay` /
    /// `Restart` / `Pause` buttons are the screen's only inputs).
    fn game_over_overlay() -> TouchOverlay {
        let mut o = TouchOverlay::new();
        o.set_enabled(true);
        o.set_active(false);
        o.set_action_active(true);
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
    fn replay_pos() -> Vec2 {
        button_centers(W, H)[Button::Replay.idx()]
    }
    fn restart_pos() -> Vec2 {
        button_centers(W, H)[Button::Restart.idx()]
    }
    fn pause_pos() -> Vec2 {
        button_centers(W, H)[Button::Pause.idx()]
    }
    fn btn_name(b: Button) -> &'static str {
        match b {
            Button::Exit => "Exit",
            Button::Focus => "Focus",
            Button::Swap => "Swap",
            Button::RotateCCW => "RotateCCW",
            Button::RotateCW => "RotateCW",
            Button::Replay => "Replay",
            Button::Restart => "Restart",
            Button::Pause => "Pause",
        }
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

    /// The class-scoped deactivation clears (F31): the gameplay gate's
    /// off clears the gameplay captures + held flags + edges, leaving
    /// the ACTION class's state untouched (and vice versa) — the two
    /// gates' clears are class-scoped (`clear_class`), the C++'s
    /// state-transition semantics (the dead game's held touch must not
    /// leak into the next session's inputs, and the game-over screen's
    /// held action finger must not leak when the game ends either).
    #[test]
    fn the_deactivation_clears_are_class_scoped() {
        // The death transition: the gameplay gate drops, the action
        // gate stays. A finger on `Focus` (gameplay) is cleared; a
        // finger on `Exit` (action) is NOT.
        let mut o = active_overlay();
        o.pointer_event(&down(1, focus_pos().x, focus_pos().y));
        o.pointer_event(&down(2, exit_pos().x, exit_pos().y));
        assert!(o.held(Button::Focus));
        assert!(o.held(Button::Exit));
        o.set_active(false);
        assert!(
            !o.held(Button::Focus),
            "the gameplay finger is cleared at death"
        );
        assert!(
            o.held(Button::Exit),
            "the action finger survives the death transition"
        );
        assert_eq!(o.captured_ids(), vec![2]);
        // The physically-still-down focus finger's `Up` is a no-op now
        // (its capture was cleared with the class).
        o.pointer_event(&up(1));
        assert!(!o.held(Button::Focus));
        // The end-of-game transition: the action gate drops too (the
        // game slot left — the menu). The exit finger is cleared; a
        // re-activation of either gate is clean (a fresh press is
        // required, no stale capture fires).
        o.set_action_active(false);
        assert!(!o.held(Button::Exit));
        assert!(o.captured_ids().is_empty());
        assert!(!o.take_press_edges().iter().any(|e| *e));
        o.pointer_event(&up(2));
        o.set_active(true);
        o.set_action_active(true);
        assert!(!o.held(Button::Focus));
        assert!(!o.held(Button::Exit));
        o.pointer_event(&down(1, focus_pos().x, focus_pos().y));
        assert!(o.held(Button::Focus));
    }

    /// The F31 action trio's press edges: each button fires its edge
    /// once per physical press (the C++'s `Type::Once` binds' observable
    /// — `trigger_replay` / `trigger_restart` / `trigger_pause`), and
    /// the third finger on the same button adds no edge (the refcount's
    /// second increment is not a press).
    #[test]
    fn the_action_trio_fires_one_edge_per_press() {
        let mut o = game_over_overlay();
        for (finger, pos, btn) in [
            (1u32, replay_pos(), Button::Replay),
            (2u32, restart_pos(), Button::Restart),
            (3u32, pause_pos(), Button::Pause),
        ] {
            o.pointer_event(&down(finger, pos.x, pos.y));
            let edges = o.take_press_edges();
            assert!(edges[btn.idx()], "the {} press edge", btn_name(btn));
            assert!(!o.take_press_edges()[btn.idx()], "no edge while held");
            // A second finger on the same button: no new edge.
            o.pointer_event(&down(finger + 10, pos.x, pos.y));
            assert!(!o.take_press_edges()[btn.idx()]);
            o.pointer_event(&up(finger + 10));
            o.pointer_event(&up(finger));
            assert!(!o.held(btn));
        }
        // A fresh press after the release is a fresh edge.
        o.pointer_event(&down(1, replay_pos().x, replay_pos().y));
        assert!(o.take_press_edges()[Button::Replay.idx()]);
        o.pointer_event(&up(1));
    }

    /// issue 054 (F36) — the menu-state shape: with `menu_active` on and
    /// the game-state gates off, the `Exit` (MENU) button captures / holds
    /// / fires while the rest of the action row (the `Replay` / `Restart`
    /// / `Pause` — no menu-side body) and the gameplay row do not — a
    /// stray finger there must not capture, must not hold, must not fire.
    #[test]
    fn the_menu_state_shape_consumes_only_the_exit_button() {
        let mut o = TouchOverlay::new();
        o.set_enabled(true);
        o.set_menu_active(true);
        o.set_view(W, H);
        // The MENU button answers to its gate.
        o.pointer_event(&down(1, exit_pos().x, exit_pos().y));
        assert!(o.held(Button::Exit));
        assert!(o.take_press_edges()[Button::Exit.idx()]);
        // The action trio has no menu-side body — not consumed.
        o.pointer_event(&down(2, replay_pos().x, replay_pos().y));
        o.pointer_event(&down(3, restart_pos().x, restart_pos().y));
        o.pointer_event(&down(4, pause_pos().x, pause_pos().y));
        assert!(!o.held(Button::Replay));
        assert!(!o.held(Button::Restart));
        assert!(!o.held(Button::Pause));
        assert_eq!(o.captured_ids(), vec![1]);
        // The gameplay row is off in the menu — not consumed.
        o.pointer_event(&down(5, focus_pos().x, focus_pos().y));
        assert!(!o.held(Button::Focus));
        assert_eq!(o.captured_ids(), vec![1]);
        o.pointer_event(&up(1));
        assert!(!o.held(Button::Exit));
    }

    /// issue 054 (F36) — the state hand-offs: the composition's feed
    /// order is `set_active` → `set_action_active` → `set_menu_active`,
    /// and the gate-scoped clear reads the post-flip values with the
    /// other gates' current values. game→menu: the action gate drops
    /// FIRST — the action class's clear (F31's semantics: the game's
    /// end clears the action captures) includes the `Exit`; the menu
    /// gate then takes over fresh (the next MENU press — the operator's
    /// release is the transition's physical half: the game→menu switch
    /// is the `Exit` press's own effect, so a finger cannot be held on
    /// the button across it — re-captures). menu→game: the action gate
    /// rises BEFORE the menu gate drops (the feed order) — the `Exit`
    /// gate's OR stays on, the capture survives (a harmless by-product:
    /// the level-start's ok-tap finger is the middle zone's, not the
    /// MENU button's). The other action buttons clear on the game's end
    /// (their single gate drops) and do not re-capture in the menu (no
    /// menu-side body).
    #[test]
    fn the_exit_gate_across_the_state_hand_offs() {
        // game (playing) → menu: the action gate drops first — the
        // action class clears (the `Exit` capture included, F31's
        // semantics), the menu gate then takes over fresh.
        let mut o = active_overlay();
        o.pointer_event(&down(1, exit_pos().x, exit_pos().y));
        o.pointer_event(&down(2, replay_pos().x, replay_pos().y));
        assert!(o.held(Button::Exit));
        assert!(o.held(Button::Replay));
        o.set_active(false);
        o.set_action_active(false);
        assert!(
            !o.held(Button::Exit),
            "the game's end clears the action class (the menu gate rises after)"
        );
        assert!(
            !o.held(Button::Replay),
            "the action trio clears at the game's end (no menu-side body)"
        );
        assert!(o.captured_ids().is_empty());
        o.set_menu_active(true);
        // The physically-still-down fingers' `Up`s are no-ops now (their
        // captures were cleared with the gates).
        o.pointer_event(&up(1));
        o.pointer_event(&up(2));
        // The next press (the operator's lift + re-press — the
        // transition's physical half) re-captures cleanly under the menu
        // gate.
        o.pointer_event(&down(1, exit_pos().x, exit_pos().y));
        assert!(o.held(Button::Exit));
        assert!(o.take_press_edges()[Button::Exit.idx()]);
        // menu → game: the action gate rises before the menu gate drops
        // (the feed order) — the gate's OR keeps the capture.
        o.set_action_active(true);
        o.set_menu_active(false);
        assert!(
            o.held(Button::Exit),
            "the action gate's rise keeps the OR on: the capture survives"
        );
        o.pointer_event(&up(1));
        assert!(!o.held(Button::Exit));
    }

    /// issue 054 (F36) — the menu gate's own deactivation: leaving a
    /// back-relevant menu state for one that is not (`SMain`'s
    /// "main"-category no-op, the entering-text guard, the boot
    /// screens) drops the `Exit` capture / held flag / edge (no stale
    /// MENU press lingers to fire a dead `exit_action`), while the
    /// action gate's state is untouched.
    #[test]
    fn the_menu_gate_deactivation_clears_the_exit_capture() {
        let mut o = TouchOverlay::new();
        o.set_enabled(true);
        o.set_menu_active(true);
        o.set_view(W, H);
        o.pointer_event(&down(1, exit_pos().x, exit_pos().y));
        assert!(o.held(Button::Exit));
        assert!(o.take_press_edges()[Button::Exit.idx()]);
        o.set_menu_active(false);
        assert!(
            !o.held(Button::Exit),
            "the back state left: the capture is gone"
        );
        assert!(o.captured_ids().is_empty());
        assert!(!o.take_press_edges().iter().any(|e| *e));
        // The physically-still-down finger's `Up` is a no-op.
        o.pointer_event(&up(1));
        // A re-activation is clean (a fresh press is required).
        o.set_menu_active(true);
        assert!(!o.held(Button::Exit));
        o.pointer_event(&down(1, exit_pos().x, exit_pos().y));
        assert!(o.held(Button::Exit));
    }

    /// The game-over shape (F31): with the gameplay gate off, a
    /// `Down` on a gameplay button is NOT consumed (the `Focus` / `Swap`
    /// buttons are gone at death — a stray finger there must not
    /// capture, must not hold, must not fire) while the action buttons
    /// answer to their own gate.
    #[test]
    fn the_game_over_shape_consumes_only_the_action_buttons() {
        let mut o = game_over_overlay();
        assert!(!o.held(Button::Focus));
        assert!(
            o.captured_ids().is_empty(),
            "a gameplay button does not capture at death"
        );
        o.pointer_event(&down(2, ccw_pos().x, ccw_pos().y));
        assert!(!o.held(Button::RotateCCW));
        // The action buttons DO capture (the game-over screen's inputs).
        o.pointer_event(&down(3, replay_pos().x, replay_pos().y));
        assert!(o.held(Button::Replay));
        assert!(o.take_press_edges()[Button::Replay.idx()]);
        o.pointer_event(&up(3));
        assert!(!o.held(Button::Replay));
    }

    /// The layout's F31 half: the trio's positions — `Replay` /
    /// `Restart` top-middle (the focus / swap x's, the top row), `Pause`
    /// top-right (the `RotateCW` mirror); the `button_centers`'s 8
    /// entries are all distinct.
    #[test]
    fn the_action_row_layout_sits_above_the_gameplay_row() {
        let centers = button_centers(W, H);
        assert_eq!(centers.len(), 8);
        let replay = centers[Button::Replay.idx()];
        let restart = centers[Button::Restart.idx()];
        let pause = centers[Button::Pause.idx()];
        let focus = centers[Button::Focus.idx()];
        let swap = centers[Button::Swap.idx()];
        let cw = centers[Button::RotateCW.idx()];
        // The top row: same x's as the bottom-middle pair, at the top
        // edge (the `PAD + BUTTON_HALF` center, the `Exit`'s y).
        assert_eq!(replay.x, focus.x);
        assert_eq!(restart.x, swap.x);
        assert_eq!(replay.y, centers[Button::Exit.idx()].y);
        assert_eq!(restart.y, replay.y);
        // `Pause` mirrors `RotateCW` (the top-right corner).
        assert_eq!(pause.x, cw.x);
        assert_eq!(pause.y, centers[Button::Exit.idx()].y);
        // The rows are disjoint (the top row clears the bottom row's
        // top edge by far more than the button's half-size).
        assert!(replay.y + BUTTON_HALF < focus.y - BUTTON_HALF);
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
