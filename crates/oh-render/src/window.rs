//! `sf::RenderWindow` / `sf::Event` — the window + event-loop surface.
//!
//! The platform backend is `winit` 0.30 (window, events, input) with the real
//! GL context from `gl.rs` (glutin + glow). The SFML `Event` set, the winit→
//! SFML key/button mapping, the window state (vsync, framerate, cursor, view,
//! size), `mapPixelToCoords`, screenshots, and the joystick surface live here
//! and are headless-testable; only the live-context bootstrap (`create`) and
//! the real event loop (`run`) need a display.
//!
//! **Event-loop model note.** winit 0.30 removed SFML-style non-blocking
//! `pollEvent`; instead a single `EventLoop::run` delivers events and the game
//! frame runs on `MainEventsCleared`. So the SFML `while(pollEvent)` loop
//! (`GameWindow::runEvents`) maps onto `Window::run`, which drives the winit
//! loop, folds `WindowEvent`s into the SFML `Event` set (via `map_window_event`
//! + `apply_event`), and invokes the per-frame handler once. `Window::headless`
//!   (no display) still runs exactly one frame so the headless game path works.

#[cfg(not(target_os = "emscripten"))]
use std::time::{Duration, Instant};

#[cfg(target_os = "emscripten")]
use wasm_bindgen::closure::Closure;
#[cfg(target_os = "emscripten")]
use wasm_bindgen::JsCast;
#[cfg(not(target_os = "emscripten"))]
use winit::application::ApplicationHandler;
#[cfg(not(target_os = "emscripten"))]
use winit::event::WindowEvent;
#[cfg(target_os = "android")]
use winit::event_loop::EventLoop;
#[cfg(not(target_os = "emscripten"))]
use winit::event_loop::{ActiveEventLoop, ControlFlow};
#[cfg(not(target_os = "emscripten"))]
use winit::keyboard::{Key as WKey, KeyCode, KeyLocation, NamedKey, PhysicalKey};
#[cfg(not(target_os = "emscripten"))]
use winit::window::WindowId;

use crate::gl::{GlBackend, GlParts};
use crate::input::InputState;
use crate::view::View;
use crate::Button;
use crate::Key;
use crate::Vec2;

/// `sf::Event` — the event set the game's `runEvents` switches on
/// (mapping §3, SSVStart `GameWindow.hpp`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// `sf::Event::Closed`.
    Closed,
    /// `sf::Event::Resized`.
    Resized { width: u32, height: u32 },
    /// `sf::Event::GainedFocus`.
    GainedFocus,
    /// `sf::Event::LostFocus`.
    LostFocus,
    /// `sf::Event::KeyPressed`.
    KeyPressed { code: Key, repeated: bool },
    /// `sf::Event::KeyReleased`.
    KeyReleased { code: Key },
    /// `sf::Event::MouseButtonPressed`.
    MouseButtonPressed { button: Button },
    /// `sf::Event::MouseButtonReleased`.
    MouseButtonReleased { button: Button },
    /// `sf::Event::MouseMoved`.
    MouseMoved { position: Vec2 },
    /// `sf::Event::MouseWheelScrolled` (vertical, in "lines").
    ///
    /// Sign convention (verified against the SFML fork the C++ builds
    /// against, `include/SFML/Window/Event.hpp` — "Wheel offset (positive
    /// is up/left, negative is down/right)"): `delta` is POSITIVE when the
    /// wheel moves UP (away from the user). The winit source agrees —
    /// winit 0.30.13's X11 backend maps Button4 (wheel up) to
    /// `LineDelta(0, +1)` / Button5 to `LineDelta(0, -1)`
    /// (`src/platform_impl/linux/x11/event_processor.rs:1083-1091`), and
    /// Windows' `WM_MOUSEWHEEL` is positive on wheel-up — so the `dy` the
    /// `map_window_event` fold produces needs NO sign flip.
    MouseWheelScrolled { delta: f32 },
    /// `sf::Event::TextEntered`.
    TextEntered { character: char },
    /// `sf::Event::TouchBegan`.
    TouchBegan { finger: u32, position: Vec2 },
    /// The touch moved (the winit `TouchPhase::Moved` — the port's
    /// addition over the SFML `Event` set: the SFML touch positions are
    /// polled, the port's are event-updated; issue 25b SG5's
    /// `finger_down_positions` seam).
    TouchMoved { finger: u32, position: Vec2 },
    /// `sf::Event::TouchEnded`.
    TouchEnded { finger: u32 },
}

/// `sf::Joystick::Axis` (the axes the game polls).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoystickAxis {
    X,
    Y,
    Z,
    RotateX,
    RotateY,
    RotateZ,
    PovX,
    PovY,
    AbsX,
    AbsY,
}

// ---------------------------------------------------------------------------
// winit -> SFML mapping (pure + headless-testable)
// ---------------------------------------------------------------------------

/// Map a winit mouse button to the exact SFML `Mouse::Button` integer.
#[cfg(not(target_os = "emscripten"))]
#[must_use]
pub fn winit_button_to_sfml(button: &winit::event::MouseButton) -> Option<Button> {
    match button {
        winit::event::MouseButton::Left => Some(Button::LEFT),
        winit::event::MouseButton::Right => Some(Button::RIGHT),
        winit::event::MouseButton::Middle => Some(Button::MIDDLE),
        winit::event::MouseButton::Other(1) => Some(Button::X_BUTTON1),
        winit::event::MouseButton::Other(2) => Some(Button::X_BUTTON2),
        _ => None,
    }
}

/// Map a winit logical + physical key to the exact SFML `Keyboard::Key`
/// integer (oracle-vectors.md). `Unknown` when unmappable.
#[cfg(not(target_os = "emscripten"))]
#[must_use]
pub fn winit_key_to_sfml(logical: &WKey, physical: &PhysicalKey, location: KeyLocation) -> Key {
    // Numpad is delivered logically as `Character`, so disambiguate via the
    // physical key first.
    if let PhysicalKey::Code(code) = physical {
        match code {
            KeyCode::Numpad0 => return Key::NUMPAD0,
            KeyCode::Numpad1 => return Key::NUMPAD1,
            KeyCode::Numpad2 => return Key::NUMPAD2,
            KeyCode::Numpad3 => return Key::NUMPAD3,
            KeyCode::Numpad4 => return Key::NUMPAD4,
            KeyCode::Numpad5 => return Key::NUMPAD5,
            KeyCode::Numpad6 => return Key::NUMPAD6,
            KeyCode::Numpad7 => return Key::NUMPAD7,
            KeyCode::Numpad8 => return Key::NUMPAD8,
            KeyCode::Numpad9 => return Key::NUMPAD9,
            KeyCode::NumpadAdd => return Key::ADD,
            KeyCode::NumpadSubtract => return Key::SUBTRACT,
            KeyCode::NumpadMultiply => return Key::MULTIPLY,
            KeyCode::NumpadDivide => return Key::DIVIDE,
            KeyCode::NumpadEnter => return Key::ENTER,
            KeyCode::NumpadDecimal | KeyCode::NumpadComma => return Key::PERIOD,
            // The main Enter key (winit's `KeyCode::Enter` — "not on the
            // Numpad", winit android `keycodes.rs`): on Android the logical
            // half of the same event is the device-character-map's
            // `Character("\n")` (the issue-025d on-device finding: `adb
            // shell input keyevent 66` arrived as physical
            // `Code(Enter)` + logical `Character("\n")`, and without this
            // arm the mapping fell to `Key::UNKNOWN` — the event was
            // dropped and the boot screens never saw the release).
            KeyCode::Enter => return Key::ENTER,
            _ => {}
        }
    }

    match logical {
        WKey::Character(s) => {
            if let Some(c) = s.chars().next() {
                match c {
                    'a'..='z' => return Key(Key::A.0 + (c as i32 - 'a' as i32)),
                    'A'..='Z' => return Key(Key::A.0 + (c as i32 - 'A' as i32)),
                    '0'..='9' => return Key(Key::NUM0.0 + (c as i32 - '0' as i32)),
                    // The Enter key's character form (the Android device
                    // character map emits '\n' for KEYCODE_ENTER — winit
                    // android `to_logical`, the issue-025d on-device
                    // finding): the physical `Code(Enter)` arm above is the
                    // same key; this arm covers the character-form delivery
                    // with a non-`Code` physical half.
                    '\n' => return Key::ENTER,
                    ',' => return Key::COMMA,
                    '.' => return Key::PERIOD,
                    ';' => return Key::SEMICOLON,
                    '\'' => return Key::APOSTROPHE,
                    '/' => return Key::SLASH,
                    '\\' => return Key::BACKSLASH,
                    '`' => return Key::GRAVE,
                    '=' => return Key::EQUAL,
                    '-' => return Key::HYPHEN,
                    '[' => return Key::L_BRACKET,
                    ']' => return Key::R_BRACKET,
                    _ => {}
                }
            }
        }
        WKey::Named(nk) => match nk {
            NamedKey::Space => return Key::SPACE,
            NamedKey::Enter => return Key::ENTER,
            NamedKey::Backspace => return Key::BACKSPACE,
            NamedKey::Tab => return Key::TAB,
            NamedKey::PageUp => return Key::PAGE_UP,
            NamedKey::PageDown => return Key::PAGE_DOWN,
            NamedKey::End => return Key::END,
            NamedKey::Home => return Key::HOME,
            NamedKey::Insert => return Key::INSERT,
            NamedKey::Delete => return Key::DELETE,
            NamedKey::ArrowLeft => return Key::LEFT,
            NamedKey::ArrowRight => return Key::RIGHT,
            NamedKey::ArrowUp => return Key::UP,
            NamedKey::ArrowDown => return Key::DOWN,
            NamedKey::Escape => return Key::ESCAPE,
            NamedKey::Pause => return Key::PAUSE,
            NamedKey::ContextMenu => return Key::MENU,
            NamedKey::F1 => return Key::F1,
            NamedKey::F2 => return Key::F2,
            NamedKey::F3 => return Key::F3,
            NamedKey::F4 => return Key::F4,
            NamedKey::F5 => return Key::F5,
            NamedKey::F6 => return Key::F6,
            NamedKey::F7 => return Key::F7,
            NamedKey::F8 => return Key::F8,
            NamedKey::F9 => return Key::F9,
            NamedKey::F10 => return Key::F10,
            NamedKey::F11 => return Key::F11,
            NamedKey::F12 => return Key::F12,
            NamedKey::F13 => return Key::F13,
            NamedKey::F14 => return Key::F14,
            NamedKey::F15 => return Key::F15,
            NamedKey::Control => {
                return if location == KeyLocation::Right {
                    Key::R_CONTROL
                } else {
                    Key::L_CONTROL
                }
            }
            NamedKey::Shift => {
                return if location == KeyLocation::Right {
                    Key::R_SHIFT
                } else {
                    Key::L_SHIFT
                }
            }
            NamedKey::Alt => {
                return if location == KeyLocation::Right {
                    Key::R_ALT
                } else {
                    Key::L_ALT
                }
            }
            NamedKey::Meta => {
                return if location == KeyLocation::Right {
                    Key::R_SYSTEM
                } else {
                    Key::L_SYSTEM
                }
            }
            _ => {}
        },
        _ => {}
    }

    Key::UNKNOWN
}

/// Map a winit `WindowEvent` to the SFML `Event` (or `None` when it has no SFML
/// analogue / the key is unmappable). Pure + headless-testable.
#[cfg(not(target_os = "emscripten"))]
#[must_use]
pub fn map_window_event(event: &WindowEvent) -> Option<Event> {
    match event {
        WindowEvent::CloseRequested => Some(Event::Closed),
        WindowEvent::Focused(true) => Some(Event::GainedFocus),
        WindowEvent::Focused(false) => Some(Event::LostFocus),
        WindowEvent::Resized(size) => Some(Event::Resized {
            width: size.width,
            height: size.height,
        }),
        WindowEvent::KeyboardInput { event: ke, .. } => {
            let code = winit_key_to_sfml(&ke.logical_key, &ke.physical_key, ke.location);
            if code == Key::UNKNOWN {
                return None;
            }
            Some(match ke.state {
                winit::event::ElementState::Pressed => Event::KeyPressed {
                    code,
                    repeated: ke.repeat,
                },
                winit::event::ElementState::Released => Event::KeyReleased { code },
            })
        }
        WindowEvent::MouseInput { button, state, .. } => {
            let button = winit_button_to_sfml(button)?;
            Some(match state {
                winit::event::ElementState::Pressed => Event::MouseButtonPressed { button },
                winit::event::ElementState::Released => Event::MouseButtonReleased { button },
            })
        }
        WindowEvent::CursorMoved { position, .. } => Some(Event::MouseMoved {
            position: Vec2::new(position.x as f32, position.y as f32),
        }),
        WindowEvent::MouseWheel { delta, .. } => {
            let dy = match delta {
                winit::event::MouseScrollDelta::LineDelta(_, y) => *y,
                winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 10.0,
            };
            Some(Event::MouseWheelScrolled { delta: dy })
        }
        WindowEvent::Touch(touch) => Some(match touch.phase {
            winit::event::TouchPhase::Started => Event::TouchBegan {
                finger: touch.id as u32,
                position: Vec2::new(touch.location.x as f32, touch.location.y as f32),
            },
            winit::event::TouchPhase::Moved => Event::TouchMoved {
                finger: touch.id as u32,
                position: Vec2::new(touch.location.x as f32, touch.location.y as f32),
            },
            // `Canceled` folds to the `Ended` arm (spec 31 §5's
            // `pointercancel` — a canceled touch is a release): the C++
            // SFML Android activity delivers `MotionEvent.ACTION_CANCEL`
            // as a `TouchEnded` (the SFML window layer's touch-seam
            // note, the `TouchEnded` arm's docs). The pre-fix `_ =>
            // return None` discard left a PHANTOM down finger in the
            // window's input state / the shared touch sink / the engine's
            // rotation path — a stuck rotation (and, for issue 31's
            // overlay, a stuck button) until the next real touch.
            winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                Event::TouchEnded {
                    finger: touch.id as u32,
                }
            }
        }),
        _ => None,
    }
}

/// Map a DOM `KeyboardEvent` `code` / `key` to the exact SFML
/// `Keyboard::Key` integer — the web twin of [`winit_key_to_sfml`]: the SAME
/// SFML integers, with the DOM `KeyboardEvent.code` in place of the winit
/// `KeyCode` / `NamedKey` (`"KeyA"` → `KeyCode::KeyA`, `"Numpad0"` →
/// `KeyCode::Numpad0`, `"ShiftLeft"` → `NamedKey::Shift`+`Left` location, …)
/// and the DOM `key` in place of the winit `Character` logical key (the
/// punctuation arm). `UNKNOWN` when unmappable.
///
/// Ungated (issue 32): the mapping is PURE (DOM strings in, the SFML
/// `Key` out — no `winit` / `web-sys` types), so the host gate's in-process
/// tests pin the pinned SFML integers (oracle-vectors.md) exactly as the
/// emscripten keydown listener consumes them — replays and menu binds
/// cannot diverge by target.
#[must_use]
pub fn web_key_to_sfml(code: &str, key: &str) -> Key {
    // The `Key<A-Z>` / `Digit<D>` / `Numpad<D>` families (the winit
    // `Character` + numpad arms).
    if let Some(c) = code.strip_prefix("Key") {
        if let Some(ch) = c.chars().next() {
            if ch.is_ascii_uppercase() && c.len() == 1 {
                return Key(Key::A.0 + (ch as i32 - 'A' as i32));
            }
        }
    }
    if let Some(d) = code.strip_prefix("Digit") {
        if let Some(ch) = d.chars().next() {
            if ch.is_ascii_digit() && d.len() == 1 {
                return Key(Key::NUM0.0 + (ch as i32 - '0' as i32));
            }
        }
    }
    if let Some(d) = code.strip_prefix("Numpad") {
        if let Some(ch) = d.chars().next() {
            if ch.is_ascii_digit() && d.len() == 1 {
                return Key(Key::NUMPAD0.0 + (ch as i32 - '0' as i32));
            }
        }
    }
    match code {
        "NumpadAdd" => return Key::ADD,
        "NumpadSubtract" => return Key::SUBTRACT,
        "NumpadMultiply" => return Key::MULTIPLY,
        "NumpadDivide" => return Key::DIVIDE,
        "NumpadEnter" => return Key::ENTER,
        "NumpadDecimal" => return Key::PERIOD,
        "Space" => return Key::SPACE,
        "Enter" => return Key::ENTER,
        "Backspace" => return Key::BACKSPACE,
        "Tab" => return Key::TAB,
        "PageUp" => return Key::PAGE_UP,
        "PageDown" => return Key::PAGE_DOWN,
        "End" => return Key::END,
        "Home" => return Key::HOME,
        "Insert" => return Key::INSERT,
        "Delete" => return Key::DELETE,
        "ArrowLeft" => return Key::LEFT,
        "ArrowRight" => return Key::RIGHT,
        "ArrowUp" => return Key::UP,
        "ArrowDown" => return Key::DOWN,
        "Escape" => return Key::ESCAPE,
        "Pause" => return Key::PAUSE,
        "ContextMenu" => return Key::MENU,
        "ShiftLeft" => return Key::L_SHIFT,
        "ShiftRight" => return Key::R_SHIFT,
        "ControlLeft" => return Key::L_CONTROL,
        "ControlRight" => return Key::R_CONTROL,
        "AltLeft" => return Key::L_ALT,
        "AltRight" => return Key::R_ALT,
        "MetaLeft" => return Key::L_SYSTEM,
        "MetaRight" => return Key::R_SYSTEM,
        _ => {}
    }
    // The `F<N>` keys (F1..F15 — the winit `NamedKey::F*` arm).
    if let Some(n) = code.strip_prefix("F") {
        if let Ok(n) = n.parse::<i32>() {
            if (1..=15).contains(&n) {
                return Key(Key::F1.0 + (n - 1));
            }
        }
    }
    // The punctuation (the DOM `key` — the winit `Character` arm).
    match key {
        "," => return Key::COMMA,
        "." => return Key::PERIOD,
        ";" => return Key::SEMICOLON,
        "'" => return Key::APOSTROPHE,
        "/" => return Key::SLASH,
        "\\" => return Key::BACKSLASH,
        "`" => return Key::GRAVE,
        "=" => return Key::EQUAL,
        "-" => return Key::HYPHEN,
        "[" => return Key::L_BRACKET,
        "]" => return Key::R_BRACKET,
        _ => {}
    }
    Key::UNKNOWN
}

/// Map a DOM `PointerEvent` / `MouseEvent` `button` index to the exact SFML
/// `Mouse::Button` integer — the web twin of [`winit_button_to_sfml`] (the
/// SAME SFML integers: the DOM `button` 0/1/2 are left/middle/right, 3/4 the
/// X buttons — the winit `Other(1)` / `Other(2)` arms).
///
/// Ungated (issue 32): pure (DOM `i16` in, the SFML `Button` out) — the host
/// gate's in-process tests pin it beside [`web_key_to_sfml`].
#[must_use]
pub fn web_mouse_button_to_sfml(button: i16) -> Option<Button> {
    match button {
        0 => Some(Button::LEFT),
        1 => Some(Button::MIDDLE),
        2 => Some(Button::RIGHT),
        3 => Some(Button::X_BUTTON1),
        4 => Some(Button::X_BUTTON2),
        _ => None,
    }
}

/// issue 32 §2.2 — the `keydown`'s `preventDefault` policy (pure +
/// host-testable — the emscripten keydown listener applies it).
///
/// `true` = the key the game consumes (`preventDefault` stops the page
/// from ALSO scrolling / zooming / moving focus on it: `Space` and the
/// arrow keys scroll, `Tab` moves focus). `false` for everything else:
///
/// * the unmappable keys (no SFML `Key` — the page's business);
/// * the `ctrl` / `meta` combos (the browser-level shortcuts — reload,
///   new tab, copy / paste, zoom: they must NOT be swallowed);
/// * the IME composition keys (`is_composing` — a composition must run to
///   its commit, the `key` values there are `"Process"` / `"Composing"`,
///   not the typed character);
/// * the browser-reserved function keys (`F5` reload, `F11` fullscreen,
///   `F12` devtools — the DOM `code`s; the C++ consumes its own F-keys
///   (the menu's F3/F4, 29b9), but the browser's three are the page's
///   level, not the game's).
#[must_use]
pub fn web_keydown_prevent_default(
    code: &str,
    key: &str,
    ctrl_key: bool,
    meta_key: bool,
    is_composing: bool,
) -> bool {
    if ctrl_key || meta_key || is_composing {
        return false;
    }
    match code {
        "F5" | "F11" | "F12" => return false,
        _ => {}
    }
    web_key_to_sfml(code, key) != Key::UNKNOWN
}

// ---------------------------------------------------------------------------
// issue 32 — the web pointer feed (the DOM pointer events → SFML `Event`
// mapping)
// ---------------------------------------------------------------------------

/// issue 32 §2.1 — the web POINTER FEED: the DOM `pointerdown` /
/// `pointermove` / `pointerup` / `pointercancel` → SFML `Event` mapping
/// (the pointer twin of [`web_key_to_sfml`]), fed by the emscripten
/// listeners one synthetic DOM-shaped event at a time:
///
/// * **the pointer-type dispatch** — the DOM `pointerType` `"mouse"` →
///   the SFML mouse fold (`MouseMoved` + `MouseButtonPressed` /
///   `MouseButtonReleased`, the DOM `button` through
///   [`web_mouse_button_to_sfml`]); `"touch"` / `"pen"` → the SFML touch
///   fold (`TouchBegan` / `TouchMoved` / `TouchEnded`) with a STABLE
///   0..`FINGER_COUNT` finger slot per live pointer: the DOM `pointerId`
///   (unbounded, monotonic across the page's pointer sessions) mapped
///   onto the `InputState`'s 16 finger slots — a slot is allocated on
///   `down` (lowest free), freed on `up` / `cancel`; a 17th concurrent
///   touch is ignored (the slots are full — beyond any hardware);
/// * **the release-half bookkeeping** (the stuck-down / stuck-release
///   guards, spec §2.1's "a stuck-down input is the classic browser
///   input bug"): a `pointerup` / `pointercancel` emits a release ONLY
///   for a pointer / button that went down here (the `setPointerCapture`
///   on the listener's `pointerdown` is what guarantees the `up` lands
///   even for a drag off the element — this feed makes the release
///   observable exactly once: a press that started off-canvas (no
///   `down` seen) produces no release, and a double `up` produces one).
///
/// Pure + host-testable (no `web-sys`): the emscripten listeners are the
/// thin wrapper that hands it the DOM event's fields and pushes the
/// resulting `Event`s into the shared queue the rAF loop drains.
pub struct WebPointerFeed {
    /// The live touch / pen pointers' finger slots (`pointerId` →
    /// 0..`FINGER_COUNT` slot).
    touch_slots: std::collections::HashMap<u32, u32>,
    /// The live mouse pointers' pressed-button masks (`pointerId` → the
    /// DOM `button` bitmask, `1 << button` per pressed 0..=4 button).
    mouse_buttons: std::collections::HashMap<u32, u16>,
}

impl Default for WebPointerFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl WebPointerFeed {
    #[must_use]
    pub fn new() -> Self {
        Self {
            touch_slots: std::collections::HashMap::new(),
            mouse_buttons: std::collections::HashMap::new(),
        }
    }

    /// `pointerdown`. `position` is the pointer in the WINDOW's current
    /// size space (the listener's `canvas_position` — the CSS box × the
    /// device pixel ratio; the 025g invariant the touch sink + the menu
    /// grid's thirds + the engine's half-split all run in).
    pub fn pointer_down(
        &mut self,
        pointer_type: &str,
        pointer_id: u32,
        button: i16,
        position: Vec2,
    ) -> Vec<Event> {
        if pointer_type == "mouse" {
            let mut out = vec![Event::MouseMoved { position }];
            if let Some(btn) = web_mouse_button_to_sfml(button) {
                *self.mouse_buttons.entry(pointer_id).or_insert(0) |= 1u16 << button;
                out.push(Event::MouseButtonPressed { button: btn });
            }
            out
        } else {
            // "touch" / "pen" (everything non-mouse is a touch pointer —
            // the SFML touch events carry pens too).
            let slot = match self.touch_slots.get(&pointer_id) {
                Some(s) => *s,
                None => match Self::lowest_free_slot(&self.touch_slots) {
                    Some(s) => {
                        self.touch_slots.insert(pointer_id, s);
                        s
                    }
                    None => return Vec::new(), // all 16 slots live: ignored
                },
            };
            vec![Event::TouchBegan {
                finger: slot,
                position,
            }]
        }
    }

    /// `pointermove`. A mouse pointer moves the `MouseMoved` fold (hover
    /// included — the desktop winit path delivers `CursorMoved` the same
    /// way); a touch / pen pointer moves its `TouchMoved` fold ONLY while
    /// it is down (a pen hover — no `down` seen — is not a touch).
    pub fn pointer_move(
        &mut self,
        pointer_type: &str,
        pointer_id: u32,
        position: Vec2,
    ) -> Vec<Event> {
        if pointer_type == "mouse" {
            vec![Event::MouseMoved { position }]
        } else {
            self.touch_slots
                .get(&pointer_id)
                .map(|&slot| {
                    vec![Event::TouchMoved {
                        finger: slot,
                        position,
                    }]
                })
                .unwrap_or_default()
        }
    }

    /// `pointerup` (the release half — the release-once guard's body).
    pub fn pointer_up(&mut self, pointer_type: &str, pointer_id: u32, button: i16) -> Vec<Event> {
        if pointer_type == "mouse" {
            let Some(mask) = self.mouse_buttons.get_mut(&pointer_id) else {
                return Vec::new(); // no `down` seen: not our press
            };
            let bit = if (0..16).contains(&button) {
                1u16 << button
            } else {
                0
            };
            if *mask & bit == 0 {
                return Vec::new(); // this button never went down here
            }
            *mask &= !bit;
            if *mask == 0 {
                self.mouse_buttons.remove(&pointer_id);
            }
            match web_mouse_button_to_sfml(button) {
                Some(btn) => vec![Event::MouseButtonReleased { button: btn }],
                None => Vec::new(),
            }
        } else {
            self.touch_slots
                .remove(&pointer_id)
                .map(|slot| vec![Event::TouchEnded { finger: slot }])
                .unwrap_or_default()
        }
    }

    /// `pointercancel` — the browser's "this pointer is going away" (the
    /// system took it, the touch hit a `touch-action` no-go, the pointer
    /// left the page): the release half, ALL buttons at once (a mouse
    /// `cancel` releases its whole mask; a touch `cancel` ends its
    /// finger — the winit fold's `TouchPhase::Canceled` → `Up` twin, the
    /// C++/SFML's "a touch release is a release").
    pub fn pointer_cancel(&mut self, pointer_type: &str, pointer_id: u32) -> Vec<Event> {
        if pointer_type == "mouse" {
            let Some(mask) = self.mouse_buttons.remove(&pointer_id) else {
                return Vec::new();
            };
            (0..5)
                .filter(|b| mask & (1u16 << *b) != 0)
                .filter_map(|b| web_mouse_button_to_sfml(b as i16))
                .map(|btn| Event::MouseButtonReleased { button: btn })
                .collect()
        } else {
            self.pointer_up("touch", pointer_id, 0)
        }
    }

    /// The lowest free finger slot (0..`FINGER_COUNT`; `None` when all
    /// 16 are live).
    fn lowest_free_slot(slots: &std::collections::HashMap<u32, u32>) -> Option<u32> {
        (0..crate::input::FINGER_COUNT as u32).find(|s| !slots.values().any(|v| *v == *s))
    }
}

/// The per-window-event hook of [`Window::run_with_event_hook`] (issue
/// 014b) — the egui console's `egui-winit` event feed, seeing every raw
/// winit `WindowEvent` on a real window (the C++ `runEvents`'s per-event
/// `gameEngine->handleEvent`, GameWindow.hpp:79, called AFTER the SFML
/// input-state fold). Desktop/android only (the egui console + its
/// `egui-winit` feed have no web path — the rAF `Runner` in [`Window::run`]).
#[cfg(not(target_os = "emscripten"))]
type EventHook = dyn FnMut(&winit::window::Window, &winit::event::WindowEvent);

// ---------------------------------------------------------------------------
// Release edges (issue 025c — the C++ `checkCloseBootScreens` seam)
// ---------------------------------------------------------------------------

/// The RELEASE events since the last drain — one entry per
/// `KeyReleased` / `MouseButtonReleased` / `TouchEnded` the event fold
/// applied (the issue-025c on-device finding: the C++'s boot-screen
/// handlers are `onEvent`-driven — MenuGame.cpp ctor :518 `KeyReleased`,
/// :617 `MouseButtonReleased`, :674 `JoystickButtonReleased` — while a
/// fast hardware key tap (press + release inside one frame, the `adb`
/// `input keyevent` shape) leaves NO trace in the per-frame
/// `InputState` snapshot (both bits land between two `get_input_state`
/// reads, so a snapshot diff misses the release; a ~100 ms touch tap
/// straddles the frame boundary and is caught). The window's event fold
/// appends to the shared sink; the menu state's `refresh_input`
/// (the once-per-frame pre-update hook) drains it.
#[derive(Debug, Default)]
pub struct ReleasedEdges {
    pub keys: std::collections::HashSet<Key>,
    pub buttons: std::collections::HashSet<Button>,
    pub fingers: std::collections::HashSet<u32>,
}

impl ReleasedEdges {
    /// No release events recorded since the last drain.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.buttons.is_empty() && self.fingers.is_empty()
    }

    /// Take the recorded edges out (the `refresh_input` drain).
    pub fn take(&mut self) -> Option<ReleasedEdges> {
        if self.is_empty() {
            None
        } else {
            std::mem::take(self).into()
        }
    }
}

// ---------------------------------------------------------------------------
// Window
// ---------------------------------------------------------------------------

/// `ssvs::GameWindow` / `sf::RenderWindow` — the window + event surface.
pub struct Window {
    title: String,
    width_px: u32,
    height_px: u32,
    /// C++ `GameWindow::pixelMult` (GameWindow.hpp:37, default 1.f) — the
    /// system pixels-per-logical-pixel multiplier `mainClient` sets from
    /// `Config::getPixelMultiplier()` (main.cpp:305). The C++ applies it at
    /// window (re)creation (`recreateWindow`, GameWindow.hpp:93: the render
    /// window is sized `width * pixelMult × height * pixelMult` while the
    /// logical `width`/`height` members stay logical); the port applies it
    /// once, at `create` (the caller passes the PHYSICAL size), and records
    /// the value here for `get_pixel_mult` (the winit window is not
    /// recreated at runtime, so a later `set_pixel_mult` records but does
    /// not resize — the C++'s `mustRecreate` deferral has no port
    /// equivalent).
    pixel_mult: f32,
    fullscreen: bool,
    antialiasing_level: u32,
    vsync: bool,
    framerate_limit: f32,
    fps_limited: bool,
    mouse_cursor_visible: bool,
    focus: bool,
    open: bool,
    view: View,
    input: InputState,
    mouse_pixel: Vec2,
    icon_set: bool,
    // C++ `GameWindow::msUpdate` / `msDraw` (GameWindow.hpp:36; getters at
    // :270-276) — the per-frame update/draw phase durations in ms that the
    // C++'s `GameWindow::run` records (GameWindow.hpp:131-146); the issue
    // 014b egui console footer displays them (`window->getMsUpdate()` /
    // `getMsDraw()`, HGUpdate.cpp:1544-1546). `0.0` until the first
    // `WindowedClient` frame records them (the C++ members are likewise
    // default-initialized).
    ms_update: f32,
    ms_draw: f32,
    // Real-GL backend (both `None` in headless mode):
    #[cfg(not(target_os = "emscripten"))]
    event_loop: Option<winit::event_loop::EventLoop<()>>,
    gl: Option<GlParts>,
    // Emscripten (W1): the DOM → SFML event queue the
    // `request_animation_frame` loop drains per frame (the SFML
    // `pollEvent` batch — `apply_event` per event, the winit path's same
    // fold; `None` = headless / no canvas) + the attached listener
    // closures (they MUST outlive the registration — a dropped
    // `Closure` dangles the registered JS callback).
    #[cfg(target_os = "emscripten")]
    event_queue: Option<std::sync::Arc<std::cell::RefCell<Vec<Event>>>>,
    #[cfg(target_os = "emscripten")]
    #[allow(dead_code)] // never read — the closures must outlive the registration
    dom_listeners: Vec<Closure<dyn FnMut(web_sys::Event)>>,
    // Android (issue 25b SG2): the `ensure_android_gl` once-only flag —
    // a GL creation failure (no GLES context) degrades to the headless
    // frames permanently (the C++'s no-GL `window == nullptr` equivalent),
    // so the re-`resumed`s after activity pauses don't retry (nor log).
    // Desktop: always `false` (the desktop bootstrap is pre-`run`).
    #[cfg(target_os = "android")]
    ensure_android_gl_failed: bool,
    // issue 25b SG5 (space clarified by issue 025g): the down-finger
    // positions (the window's CURRENT size coordinates — the winit touch
    // positions pass through UNSCALED, `RunHandler::window_event`: the
    // physical drawable pixels ARE the window's current-size space, the
    // C++'s `mapPixelToCoords` target on the phone),
    // finger-id → position; the C++ `getFingerDownPositions`'s data
    // (SSVStart GameWindow.hpp:299 — the C++ polls SFML, the port
    // event-updates on TouchBegan/TouchMoved/TouchEnded). Always empty
    // on the desktop path (no `WindowEvent::Touch` there).
    finger_down_positions: std::collections::HashMap<u32, Vec2>,
    // issue 25b SG5: the SHARED sink the engine's `Renderer` seam reads
    // (the windowed client installs it — the `Window` itself is not
    // shareable: its winit `EventLoop` field is not `Send`). One slot per
    // finger (the C++'s 0..`FINGER_COUNT` iteration order), `None` = the
    // finger is up. The field `None` = no sink (headless / pre-install).
    touch_position_sink:
        Option<std::sync::Arc<std::sync::RwLock<[Option<Vec2>; crate::input::FINGER_COUNT]>>>,
    // issue 31 §1: the SHARED live input-state sink the engine's `Renderer`
    // seam reads (`Renderer::is_key_pressed` / `is_mouse_button_pressed` —
    // the C++ `u_isKeyPressed` / `u_isMouseButtonPressed`'s
    // `window->getInputState()[...]` reads, HGScripting.cpp:92/:138). The
    // windowed client installs it; every input-mutating `apply_event` arm
    // republishes the window's current `InputState` into it (the liveness
    // the C++ gets from reading the window's input state at call time).
    // `None` = not installed (headless / pre-install frames).
    input_state_sink: Option<std::sync::Arc<std::sync::RwLock<InputState>>>,
    // issue 025c: the SHARED release-edge sink the menu state's
    // `refresh_input` drains (the `ReleasedEdges` docs — the C++
    // `checkCloseBootScreens`'s event-driven seam; `None` = not
    // installed, the pre-install frames).
    released_edges_sink: Option<std::sync::Arc<std::sync::RwLock<ReleasedEdges>>>,
    // issue 025d: the SHARED entered-text sink the menu state's
    // `refresh_input` drains (the C++ `runEvents`'s per-event
    // `TextEntered` dispatch — the `MenuGame`'s `onEvent(TextEntered)`
    // handler, MenuGame.cpp:491-498, appends to `enteredChars`; the
    // window's fold records one char per `TextEntered`, the menu's
    // drain runs the handler's filter; `None` = not installed).
    entered_text_sink: Option<std::sync::Arc<std::sync::RwLock<Vec<char>>>>,
    // issue 029b2: the SHARED mouse-wheel-event sink the menu state's
    // `refresh_input` drains (the C++ `MenuGame::onEvent(
    // MouseWheelScrolled)` handler's data path, MenuGame.cpp:299-356;
    // the entered-text-sink pattern: one entry per wheel event the fold
    // applied, drained per frame in the C++'s per-event order;
    // `None` = not installed).
    wheel_events_sink: Option<std::sync::Arc<std::sync::RwLock<Vec<f32>>>>,
}

impl Window {
    /// A headless window (no display / GL context). Every SFML surface is
    /// usable; `gl()` is `None` and `run` executes exactly one frame.
    #[must_use]
    pub fn headless(width: u32, height: u32) -> Self {
        Self::new_inner(width, height, "open-hexagon")
    }

    /// Create a real window + GL context. Degrades to a headless window
    /// (`gl() == None`) when no display / GPU is available.
    #[cfg(not(target_os = "emscripten"))]
    #[must_use]
    pub fn create(width: u32, height: u32, title: &str) -> Self {
        match GlBackend::try_create(width, height, title) {
            Some(backend) => {
                let base = Self::new_inner(width, height, title);
                Self {
                    event_loop: Some(backend.event_loop),
                    gl: Some(backend.parts),
                    ..base
                }
            }
            None => Self::new_inner(width, height, title),
        }
    }

    /// Emscripten (W1): the window over the page's
    /// `<canvas id="c">` (no winit — the canvas IS the window; the
    /// `title` is the page's `<title>`, a no-op). Degrades to a headless
    /// window (`gl() == None`) when the canvas is missing or WebGL2 is
    /// unavailable (the desktop no-display / no-GPU arm). The DOM event
    /// listeners are attached here (the queue the rAF loop drains —
    /// `attach_web_listeners`); a GL failure still keeps the loop alive
    /// (null frames, the android degradation's web twin).
    ///
    /// issue 32 §2.3 — the window's size is the canvas's REAL size (the
    /// CSS box × the device pixel ratio, `web_canvas_real_size`), NOT the
    /// requested (config) size: the 025g lesson (config size ≠ real size
    /// — on the phone the native window is screen-sized, and here the
    /// canvas is the page's box; the menu layout + every hit test run in
    /// the real size). A not-yet-laid-out canvas (zero rect) falls back
    /// to the requested size.
    #[cfg(target_os = "emscripten")]
    #[must_use]
    pub fn create(width: u32, height: u32, title: &str) -> Self {
        let base = Self::new_inner(width, height, title);
        // The canvas (the W1 selector — the page MUST provide it).
        let canvas: web_sys::HtmlCanvasElement = match web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("c"))
            .map(|el| el.unchecked_into())
        {
            Some(canvas) => canvas,
            None => return base,
        };
        // The REAL size (the CSS box × the DPR — `web_canvas_real_size`):
        // the window's size bookkeeping + the GL canvas both start in the
        // real size (a zero rect — not laid out yet — keeps the requested
        // size; the `resize` listener catches the layout when it lands).
        let (real_w, real_h) = {
            let rect = canvas.get_bounding_client_rect();
            if rect.width() > 0.0 && rect.height() > 0.0 {
                web_canvas_real_size(&canvas)
            } else {
                (width.max(1), height.max(1))
            }
        };
        // The DOM → SFML event queue + listeners (the winit event loop's web
        // replacement).
        let queue: std::sync::Arc<std::cell::RefCell<Vec<Event>>> =
            std::sync::Arc::new(std::cell::RefCell::new(Vec::new()));
        let dom_listeners = attach_web_listeners(&canvas, &queue);
        // The WebGL2 context (`gl.rs` — the emscripten C API + the glow
        // loader; it sets the canvas's backing store to the size it is
        // given — the REAL one); a failure degrades to the headless
        // frames (the loop still runs on the canvas).
        let gl = GlBackend::try_create(real_w, real_h, title).map(|backend| backend.parts);
        Self {
            gl,
            event_queue: Some(queue),
            dom_listeners,
            // The real-size bookkeeping (the requested-size `base` above
            // is only the canvas-missing / not-laid-out fallback).
            width_px: real_w,
            height_px: real_h,
            view: View::new(
                Vec2::new(real_w as f32 / 2.0, real_h as f32 / 2.0),
                Vec2::new(real_w as f32, real_h as f32),
            ),
            ..base
        }
    }

    /// Android (issue 25b): the window over the event loop the
    /// `android_main` entry built (the `AndroidApp`-bound winit loop —
    /// `EventLoop::new()` panics on Android, so the loop arrives pre-built;
    /// the caller passes the platform window's LOGICAL size — the C++'s
    /// logical `width`/`height` members — while the platform window itself
    /// is screen-sized).
    ///
    /// The activity's `ANativeWindow` only exists between `InitWindow` and
    /// `TerminateWindow`, so the winit window + the glutin EGL context
    /// materialize on the first `resumed` ([`Self::ensure_android_gl`]);
    /// until then `gl()` is `None` and the loop frames run on the
    /// `NullDrawTarget` — the headless degradation, exactly the
    /// [`Self::create`] no-display arm.
    #[cfg(target_os = "android")]
    #[must_use]
    pub fn create_android(event_loop: EventLoop<()>, width: u32, height: u32, title: &str) -> Self {
        let base = Self::new_inner(width, height, title);
        Self {
            event_loop: Some(event_loop),
            ..base
        }
    }

    /// Android (issue 25b SG2): materialize the platform window + the
    /// glutin EGL context (the GLES 3.0 request — `gl.rs`'s
    /// `create_android_parts` module docs carry the full assessment) on
    /// the first `resumed` (the `ANativeWindow` exists from `InitWindow` —
    /// winit's android `create_window` wraps it; the `resumed` IS the
    /// `InitWindow` dispatch, `winit::platform_impl::android`). A creation
    /// failure degrades once to the headless frames
    /// (`ensure_android_gl_failed` — the re-`resumed`s after activity
    /// pauses are no-ops, the C++'s no-GL `window == nullptr` equivalent);
    /// desktop never calls this (the desktop bootstrap is pre-`run`).
    #[cfg(target_os = "android")]
    fn ensure_android_gl(&mut self, el: &ActiveEventLoop) {
        if self.gl.is_some() || self.ensure_android_gl_failed {
            return;
        }
        match crate::gl::create_android_parts(el) {
            Some(parts) => {
                // C++ `recreateWindow` (GameWindow.hpp:89-95): the render
                // window is created and IMMEDIATELY
                // `renderWindow.setVerticalSyncEnabled(vsync)` is applied —
                // the port's GL materialization here is that creation, so
                // the stored config value (the `create_android_window`
                // `window.set_vsync(config.get_vsync())`, the C++ main.cpp:
                // 308) goes onto the fresh EGL surface now. The explicit
                // request is load-bearing: the EGL driver's default swap
                // interval is 1 (glutin 0.32's `SwapInterval` docs) —
                // without it the shipped `vsync=false` would run at the
                // driver's vsync pacing instead of the C++'s explicit
                // `eglSwapInterval(display, 0)`.
                parts.set_swap_interval(self.vsync);
                self.gl = Some(parts);
            }
            None => {
                self.ensure_android_gl_failed = true;
                eprintln!(
                    "[Window::ensure_android_gl] Android GL context unavailable (headless frames)"
                );
            }
        }
    }

    fn new_inner(width: u32, height: u32, title: &str) -> Self {
        Self {
            title: title.to_string(),
            width_px: width,
            height_px: height,
            pixel_mult: 1.0,
            fullscreen: false,
            antialiasing_level: 3,
            vsync: false,
            framerate_limit: 0.0,
            fps_limited: false,
            mouse_cursor_visible: true,
            focus: true,
            open: true,
            view: View::new(
                Vec2::new(width as f32 / 2.0, height as f32 / 2.0),
                Vec2::new(width as f32, height as f32),
            ),
            input: InputState::new(),
            mouse_pixel: Vec2::new(0.0, 0.0),
            icon_set: false,
            ms_update: 0.0,
            ms_draw: 0.0,
            #[cfg(not(target_os = "emscripten"))]
            event_loop: None,
            gl: None,
            #[cfg(target_os = "emscripten")]
            event_queue: None,
            #[cfg(target_os = "emscripten")]
            dom_listeners: Vec::new(),
            #[cfg(target_os = "android")]
            ensure_android_gl_failed: false,
            finger_down_positions: std::collections::HashMap::new(),
            touch_position_sink: None,
            input_state_sink: None,
            released_edges_sink: None,
            entered_text_sink: None,
            wheel_events_sink: None,
        }
    }

    /// issue 25b SG5: install the shared down-finger-position sink (the
    /// engine's `Renderer::finger_down_positions` seam source — the
    /// windowed client calls this once before the loop; the touch events
    /// update BOTH the window's own `finger_down_positions` map and this
    /// sink; headless windows never get one).
    pub fn set_touch_position_sink(
        &mut self,
        sink: std::sync::Arc<std::sync::RwLock<[Option<Vec2>; crate::input::FINGER_COUNT]>>,
    ) {
        self.touch_position_sink = Some(sink);
    }

    /// The sink's write half (the touch-event updates — `None` = no-op).
    fn with_touch_sink(&self, f: impl FnOnce(&mut [Option<Vec2>; crate::input::FINGER_COUNT])) {
        if let Some(sink) = &self.touch_position_sink {
            f(&mut sink.write().expect("touch sink poisoned"));
        }
    }

    /// The release-edge sink's write half (the release-event appends —
    /// `None` = no-op; the `apply_event` release arms call it).
    fn with_released_sink(&self, f: impl FnOnce(&mut ReleasedEdges)) {
        if let Some(sink) = &self.released_edges_sink {
            f(&mut sink.write().expect("released sink poisoned"));
        }
    }

    /// issue 31 §1: install the shared live input-state sink (the engine's
    /// `Renderer::is_key_pressed` / `is_mouse_button_pressed` seam source —
    /// the windowed client calls this once before the loop; every
    /// input-mutating event fold republishes the window's `InputState`;
    /// headless windows never get one).
    pub fn set_input_state_sink(&mut self, sink: std::sync::Arc<std::sync::RwLock<InputState>>) {
        self.input_state_sink = Some(sink);
    }

    /// The input-state sink's write half (the input-event republish —
    /// `None` = no-op).
    fn publish_input_state(&self) {
        if let Some(sink) = &self.input_state_sink {
            *sink.write().expect("input state sink poisoned") = self.input;
        }
    }

    /// issue 025c: install the shared release-edge sink (the windowed
    /// client installs it once before the loop — the `Window` itself is
    /// not shareable: its winit `EventLoop` field is not `Send`, as the
    /// touch sink above).
    pub fn set_released_edges_sink(
        &mut self,
        sink: std::sync::Arc<std::sync::RwLock<ReleasedEdges>>,
    ) {
        self.released_edges_sink = Some(sink);
    }

    /// issue 025d: install the shared entered-text sink (the windowed
    /// client installs it once before the loop — the C++ `runEvents`
    /// `TextEntered` dispatch's data path; the `Window` itself is not
    /// shareable, as the sinks above).
    pub fn set_entered_text_sink(&mut self, sink: std::sync::Arc<std::sync::RwLock<Vec<char>>>) {
        self.entered_text_sink = Some(sink);
    }

    /// issue 029b2: install the shared mouse-wheel-event sink (the windowed
    /// client installs it once before the loop — the C++ `MenuGame`
    /// `MouseWheelScrolled` `onEvent` handler's data path; the `Window`
    /// itself is not shareable, as the sinks above).
    pub fn set_wheel_events_sink(&mut self, sink: std::sync::Arc<std::sync::RwLock<Vec<f32>>>) {
        self.wheel_events_sink = Some(sink);
    }

    /// The wheel-event sink's write half (the `MouseWheelScrolled` appends
    /// — `None` = no-op; the `apply_event` wheel arm calls it).
    fn with_wheel_sink(&self, f: impl FnOnce(&mut Vec<f32>)) {
        if let Some(sink) = &self.wheel_events_sink {
            f(&mut sink.write().expect("wheel sink poisoned"));
        }
    }

    /// The entered-text sink's write half (the `TextEntered` appends —
    /// `None` = no-op; the `apply_event` `TextEntered` arm calls it).
    fn with_entered_text_sink(&self, f: impl FnOnce(&mut Vec<char>)) {
        if let Some(sink) = &self.entered_text_sink {
            f(&mut sink.write().expect("entered-text sink poisoned"));
        }
    }

    /// C++ `getFingerDownPositions()` (SSVStart GameWindow.hpp:299-309):
    /// for each finger whose `InputState` bit is set, its current position
    /// in the window's CURRENT size coordinates (issue 025g — the raw
    /// winit touch coordinates, the C++'s `mapPixelToCoords` target on
    /// the phone) — `(0,0)` for a down finger with no recorded position
    /// (the SFML `Touch::getPosition` unknown-position value). The engine's
    /// `updateInput_UpdateTouchControls` half-width comparison runs
    /// against these (oh-game `update_input_update_touch_controls`, the
    /// verbatim C++ port reading the `Renderer` seam).
    #[must_use]
    pub fn finger_down_positions(&self) -> Vec<Vec2> {
        (0..crate::input::FINGER_COUNT)
            .filter(|i| self.input.is_finger(*i as u32))
            .map(|i| {
                self.finger_down_positions
                    .get(&(i as u32))
                    .copied()
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Whether a real GL context is attached.
    #[must_use]
    pub fn gl(&self) -> Option<&GlParts> {
        self.gl.as_ref()
    }

    /// `isOpen()`.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// `close()`.
    pub fn close(&mut self) {
        self.open = false;
    }

    // ---- size / style -----------------------------------------------------
    /// `getSize()`.
    #[must_use]
    pub fn get_size(&self) -> (u32, u32) {
        (self.width_px, self.height_px)
    }

    /// `getWidth()`.
    #[must_use]
    pub fn get_width(&self) -> u32 {
        self.width_px
    }

    /// `getHeight()`.
    #[must_use]
    pub fn get_height(&self) -> u32 {
        self.height_px
    }

    /// `setSize(w, h)`.
    pub fn set_size(&mut self, width: u32, height: u32) {
        self.width_px = width;
        self.height_px = height;
    }

    /// C++ `setPixelMult` (GameWindow.hpp:197-201) — see the `pixel_mult`
    /// field docs: the port applies the multiplier at creation; this records
    /// the value the C++ `mainClient` sets (main.cpp:305).
    pub fn set_pixel_mult(&mut self, pixel_mult: f32) {
        self.pixel_mult = pixel_mult;
    }

    /// C++ `GameWindow::pixelMult` (GameWindow.hpp:37).
    #[must_use]
    pub fn get_pixel_mult(&self) -> f32 {
        self.pixel_mult
    }

    /// `setTitle`.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// `getTitle()`.
    #[must_use]
    pub fn get_title(&self) -> &str {
        &self.title
    }

    /// `setFullscreen`.
    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
    }

    #[must_use]
    pub fn get_fullscreen(&self) -> bool {
        self.fullscreen
    }

    /// `setAntialiasingLevel`.
    pub fn set_antialiasing_level(&mut self, level: u32) {
        self.antialiasing_level = level;
    }

    #[must_use]
    pub fn get_antialiasing_level(&self) -> u32 {
        self.antialiasing_level
    }

    // ---- vsync / framerate ------------------------------------------------
    /// `setVerticalSyncEnabled` (`ssvs::setVsync`).
    pub fn set_vsync(&mut self, enabled: bool) {
        self.vsync = enabled;
        // C++ `GameWindow::setVsync` (GameWindow.hpp:192-195) stores the
        // value and marks `mustRecreate`; the next iteration's
        // `recreateWindow` re-applies `renderWindow.setVerticalSyncEnabled(
        // vsync)` (GameWindow.hpp:94) — the pinned SFML's
        // `eglSwapInterval(display, vsync ? 1 : 0)` (see
        // `GlParts::set_swap_interval`). The port applies the same
        // OBSERVABLE change — the live EGL swap interval — directly, on the
        // live context, instead of recreating the winit window (the
        // documented recreation gap, module docs; the full recreation is
        // lane 029w's scope).
        if let Some(gl) = self.gl.as_ref() {
            gl.set_swap_interval(enabled);
        }
    }

    #[must_use]
    pub fn get_vsync(&self) -> bool {
        self.vsync
    }

    /// `setFramerateLimit` — `ssvs::GameWindow` only applies it when
    /// `fpsLimited` is true (0 otherwise).
    pub fn set_framerate_limit(&mut self, max_fps: f32) {
        self.framerate_limit = max_fps;
        self.fps_limited = max_fps != 0.0;
    }

    /// `setMaxFPS`.
    pub fn set_max_fps(&mut self, max_fps: f32) {
        self.framerate_limit = max_fps;
    }

    /// `setFPSLimited`.
    pub fn set_fps_limited(&mut self, limited: bool) {
        self.fps_limited = limited;
    }

    /// The framerate limit actually in force (`ssvs::GameWindow` applies
    /// `maxFPS` only when fps-limited, else `0`).
    #[must_use]
    pub fn effective_framerate_limit(&self) -> f32 {
        if self.fps_limited {
            self.framerate_limit
        } else {
            0.0
        }
    }

    // ---- cursor / icon / focus -------------------------------------------
    /// `setMouseCursorVisible`.
    pub fn set_mouse_cursor_visible(&mut self, visible: bool) {
        self.mouse_cursor_visible = visible;
    }

    #[must_use]
    pub fn is_mouse_cursor_visible(&self) -> bool {
        self.mouse_cursor_visible
    }

    /// `setIcon` (C++ main.cpp:322-323: `renderWindow.setIcon(size,
    /// icon.getPixelsPtr())`): record that an icon was set; on a live winit
    /// window the RGBA8 pixels are uploaded to the platform window (the
    /// C++'s `sf::RenderWindow::setIcon` upload) via `GlParts::set_icon`.
    /// Headless (no GL): the set is recorded only (the `icon_is_set`
    /// surface stays honest in both modes).
    #[cfg(not(target_os = "emscripten"))]
    pub fn set_icon(&mut self, image: &crate::Image) {
        self.icon_set = true;
        if let Some(gl) = self.gl.as_ref() {
            let icon = winit::window::Icon::from_rgba(
                image.get_pixels().to_vec(),
                image.get_width(),
                image.get_height(),
            )
            .expect("RGBA8 buffer matches the image size (Image invariant)");
            gl.set_icon(icon);
        }
    }

    /// Emscripten (W1): a browser canvas has no window icon (the tab's
    /// favicon is the PAGE's, not the canvas's) — record the set only
    /// (the `icon_is_set` surface stays honest; the documented web no-op).
    #[cfg(target_os = "emscripten")]
    pub fn set_icon(&mut self, image: &crate::Image) {
        let _ = image;
        self.icon_set = true;
    }

    #[must_use]
    pub fn icon_is_set(&self) -> bool {
        self.icon_set
    }

    /// `hasFocus()`.
    #[must_use]
    pub fn has_focus(&self) -> bool {
        self.focus
    }

    // ---- view / camera ----------------------------------------------------
    /// `setView`.
    pub fn set_view(&mut self, view: View) {
        self.view = view;
    }

    /// `getView()`.
    #[must_use]
    pub fn get_view(&self) -> View {
        self.view
    }

    /// `getDefaultView()` — the 1:1 pixel view.
    #[must_use]
    pub fn get_default_view(&self) -> View {
        View::new(
            Vec2::new(self.width_px as f32 / 2.0, self.height_px as f32 / 2.0),
            Vec2::new(self.width_px as f32, self.height_px as f32),
        )
    }

    /// `mapPixelToCoords(pixel)` (in the current view).
    #[must_use]
    pub fn map_pixel_to_coords(&self, pixel: Vec2) -> Vec2 {
        self.view.map_pixel_to_coords(
            pixel,
            Vec2::new(self.width_px as f32, self.height_px as f32),
        )
    }

    /// `mapPixelToCoords(pixel, view)` with an explicit view.
    #[must_use]
    pub fn map_pixel_to_coords_in_view(&self, pixel: Vec2, view: View) -> Vec2 {
        view.map_pixel_to_coords(
            pixel,
            Vec2::new(self.width_px as f32, self.height_px as f32),
        )
    }

    /// `getMousePosition()` — the mouse in the current view's coordinates.
    #[must_use]
    pub fn get_mouse_position(&self) -> Vec2 {
        self.map_pixel_to_coords(self.mouse_pixel)
    }

    /// The mouse's RAW window-pixel position (the C++ `MouseMoved` event's
    /// `mEvent.mouseMovePosition` — the 29u1 cursor re-show's "actually
    /// moved" source, MenuGame.cpp:284-296). Not view-mapped: the cursor's
    /// moved-test compares the raw pixel across frames (a camera transform
    /// would make a static-camera pixel diff unstable).
    #[must_use]
    pub fn mouse_pixel(&self) -> Vec2 {
        self.mouse_pixel
    }

    // ---- input ------------------------------------------------------------
    /// `ssvs::GameWindow::getInputState()`.
    #[must_use]
    pub fn get_input_state(&self) -> &InputState {
        &self.input
    }

    /// `ssvs::GameWindow::getInputState()` (mutable — test / headless use).
    pub fn get_input_state_mut(&mut self) -> &mut InputState {
        &mut self.input
    }

    // ---- joystick ---------------------------------------------------------
    /// `sf::Joystick::isConnected(id)` — always `false` headless.
    #[must_use]
    pub fn is_joystick_connected(&self, _id: u16) -> bool {
        false
    }

    /// `sf::Joystick::getAxisPosition(id, axis)` — `0` headless.
    #[must_use]
    pub fn get_joystick_axis(&self, _id: u16, _axis: JoystickAxis) -> i32 {
        0
    }

    /// `sf::Joystick::isButtonPressed(id, button)` — always `false` headless.
    #[must_use]
    pub fn is_joystick_button_pressed(&self, _id: u16, _button: u16) -> bool {
        false
    }

    // ---- FPS / screenshot -------------------------------------------------
    /// `ssvs::GameWindow::getFPS()` — `60` until a real loop measures it.
    #[must_use]
    pub fn get_fps(&self) -> f32 {
        60.0
    }

    /// `ssvs::GameWindow::saveScreenshot(path)` (GameWindow.hpp:167-174) —
    /// writes the current framebuffer to a PNG. The GL readback runs
    /// AFTER the frame's draws (the C++ calls it from `draw()`,
    /// HGGraphics.cpp:305-313, before the swap), rows bottom-up;
    /// [`crate::gl::flip_rows`] puts them top-down before the PNG encode
    /// (the C++'s `copyToImage` row reversal). Returns `true` on success;
    /// the C++ ignores the result (the `DrawTarget` consumer mirrors that).
    /// Headless (no GL) returns `false` — the C++'s `window != nullptr`
    /// guard (HGGraphics.cpp:307) is the port's `NullDrawTarget` no-op.
    pub fn save_screenshot(&self, path: &str) -> bool {
        self.gl().is_some_and(|g| g.save_screenshot(path))
    }

    /// C++ `renderWindow.display()` (GameWindow.hpp:143) — the per-frame
    /// back-buffer swap, delegated to [`GlParts::present`] when a real GL
    /// context is attached. Headless (`gl() == None`) is a no-op (the C++'s
    /// `window == nullptr` equivalent). (Naming: the windows-testing branch
    /// called this `swap_buffers`; master's on-device fix landed it as
    /// `present` — one implementation, this is the `Window`-level
    /// convenience delegate.)
    pub fn swap_buffers(&self) {
        if let Some(gl) = self.gl() {
            gl.present();
        }
    }

    // ---- per-frame phase durations (issue 014b) ---------------------------
    /// C++ `GameWindow::msUpdate` (GameWindow.hpp:36) — the update phase
    /// duration in ms for the last frame.
    #[must_use]
    pub fn get_ms_update(&self) -> f32 {
        self.ms_update
    }

    /// C++ `GameWindow::msUpdate = ...` (GameWindow.hpp:135-137) — the
    /// windowed loop records the phase's elapsed milliseconds.
    pub fn set_ms_update(&mut self, ms: f32) {
        self.ms_update = ms;
    }

    /// C++ `GameWindow::msDraw` (GameWindow.hpp:36) — the draw phase
    /// duration in ms for the last frame.
    #[must_use]
    pub fn get_ms_draw(&self) -> f32 {
        self.ms_draw
    }

    /// C++ `GameWindow::msDraw = ...` (GameWindow.hpp:144-146).
    pub fn set_ms_draw(&mut self, ms: f32) {
        self.ms_draw = ms;
    }

    // ---- the game loop ----------------------------------------------------
    /// Drive the winit event loop (the SFML `while(isOpen){ runEvents; update;
    /// draw; }` model, via `run_app`). `on_frame` is invoked once per
    /// `about_to_wait` (i.e. once per idle frame). In headless mode it runs
    /// exactly one frame and stops.
    #[cfg(not(target_os = "emscripten"))]
    pub fn run(self, mut on_frame: impl FnMut(&mut Window)) {
        self.run_with_event_hook(None, &mut on_frame);
    }

    /// Emscripten (W1): drive the `request_animation_frame` loop (the winit
    /// event loop's web replacement; the browser paces the frames at the
    /// display refresh). Each rAF tick drains the DOM event queue
    /// (`apply_event` per event — the SFML `runEvents` batch, the winit
    /// path's same fold), invokes `on_frame`, and re-requests a frame while
    /// `is_open`. In headless mode (no canvas) it runs exactly one frame and
    /// stops.
    ///
    /// Unlike the desktop's blocking `EventLoop::run_app`, this RETURNS after
    /// scheduling the first frame (the browser owns the loop — the winit-web
    /// `spawn_app` shape); the frames then run until `Event::Closed` (the
    /// `cancel_animation_frame` + self-drop in the final tick).
    #[cfg(target_os = "emscripten")]
    pub fn run(mut self, mut on_frame: impl FnMut(&mut Window) + 'static) {
        let Some(queue) = self.event_queue.take() else {
            on_frame(&mut self);
            self.open = false;
            return;
        };
        let web_window =
            web_sys::window().expect("web window (the queue is only installed by create)");
        struct Runner<F> {
            window: Window,
            on_frame: F,
            queue: std::sync::Arc<std::cell::RefCell<Vec<Event>>>,
            // The self-owning closure (it MUST outlive every scheduled frame —
            // a dropped `Closure` dangles the registered rAF callback): the
            // final tick (`Event::Closed`) takes it out, dropping the `Rc`
            // with it (the winit `el.exit()` analogue).
            closure: Option<Closure<dyn FnMut(f64)>>,
            raf_id: i32,
        }
        let runner: std::rc::Rc<std::cell::RefCell<Runner<_>>> =
            std::rc::Rc::new(std::cell::RefCell::new(Runner {
                window: self,
                on_frame,
                queue,
                closure: None,
                raf_id: 0,
            }));
        let closure = {
            let runner = runner.clone();
            // A clone for the callback (the original stays for the initial
            // `request_animation_frame` below).
            let ww = web_window.clone();
            // `own_assert_unwind_safe` — the runner captures `RefCell`s (not
            // `UnwindSafe`), and a frame callback must never unwind into JS
            // anyway (the web build ships `panic = "abort"`).
            Closure::own_assert_unwind_safe(move |_timestamp: f64| {
                let mut r = runner.borrow_mut();
                // A plain `&mut Runner` (not the `RefMut` smart pointer) so
                // the disjoint field borrows below are place projections.
                let r = &mut *r;
                // Drain the DOM event batch into a local (the SFML
                // `runEvents` batch — `apply_event` per event, the winit
                // path's same fold).
                let events: Vec<Event> = r.queue.borrow_mut().drain(..).collect();
                for event in events {
                    Window::apply_event(&mut r.window, event);
                }
                // The per-frame handler (disjoint field borrows).
                {
                    let on_frame = &mut r.on_frame;
                    let window = &mut r.window;
                    on_frame(window);
                }
                if r.window.open {
                    let id = ww
                        .request_animation_frame(
                            r.closure
                                .as_ref()
                                .unwrap()
                                .as_ref()
                                .unchecked_ref::<js_sys::Function>(),
                        )
                        .expect("request_animation_frame");
                    r.raf_id = id;
                } else {
                    // `Event::Closed`: stop the loop — cancel the pending
                    // frame (if any) and drop the self-owning closure.
                    let _ = ww.cancel_animation_frame(r.raf_id);
                    r.closure.take();
                }
            })
        };
        runner.borrow_mut().closure = Some(closure);
        let id = web_window
            .request_animation_frame(
                runner
                    .borrow()
                    .closure
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unchecked_ref::<js_sys::Function>(),
            )
            .expect("request_animation_frame");
        runner.borrow_mut().raf_id = id;
    }

    /// The [`Self::run`] variant with a per-window-event hook (issue 014b —
    /// the egui console's `egui-winit` event feed, the C++
    /// `game.onAnyEvent += Imgui::processEvent` (HexagonGame.cpp:335) that
    /// the C++ `runEvents` dispatches AFTER the input-state update, per
    /// event, GameWindow.hpp:42-83). The hook sees every raw winit
    /// `WindowEvent` (real windows only — headless runs one frame without
    /// an event loop and never fires it) with the GL backend's winit window
    /// (the `egui-winit` state's window; `None` hook = plain [`Self::run`]).
    /// Desktop/android only (the egui console's `egui-winit` feed has no web
    /// path — the rAF `Runner` in [`Self::run`]).
    #[cfg(not(target_os = "emscripten"))]
    pub fn run_with_event_hook(
        mut self,
        on_event: Option<Box<EventHook>>,
        mut on_frame: impl FnMut(&mut Window),
    ) {
        let event_loop = match self.event_loop.take() {
            Some(el) => el,
            None => {
                on_frame(&mut self);
                self.open = false;
                return;
            }
        };
        let mut handler = RunHandler {
            window: &mut self,
            on_event,
            on_frame: &mut on_frame,
        };
        let _ = event_loop.run_app(&mut handler);
    }

    /// The SFML `GameWindow::runEvents` side effects for one event.
    /// `pub`: the windowed composition's tests drive the REAL fold (the
    /// `map_window_event` → sink → `MenuSlotAdapter::refresh_input`
    /// chain, issue 025d) with a real `Window`.
    pub fn apply_event(window: &mut Window, event: Event) {
        match event {
            Event::Closed => window.close(),
            Event::GainedFocus => window.focus = true,
            Event::LostFocus => {
                window.input.reset();
                window.publish_input_state();
                window.focus = false;
            }
            Event::KeyPressed { code, .. } => {
                window.input.set_key_pressed(code, true);
                window.publish_input_state();
            }
            Event::KeyReleased { code } => {
                window.input.set_key_pressed(code, false);
                window.publish_input_state();
                window.with_released_sink(|edges| {
                    edges.keys.insert(code);
                });
            }
            Event::MouseButtonPressed { button } => {
                window.input.set_button_pressed(button, true);
                window.publish_input_state();
            }
            Event::MouseButtonReleased { button } => {
                window.input.set_button_pressed(button, false);
                window.publish_input_state();
                window.with_released_sink(|edges| {
                    edges.buttons.insert(button);
                });
            }
            Event::MouseMoved { position } => window.mouse_pixel = position,
            Event::Resized { width, height } => {
                // Emscripten (issue 32 §2.3): the window resize IS the
                // canvas resize — the C++ `recreateWindow`'s render-window
                // half, applied to the canvas (the backing store + the GL
                // viewport + the `GlParts` size bookkeeping move together,
                // the 025g "config size ≠ real size" invariant: the menu
                // layout + the hit tests run in the size the canvas is
                // ACTUALLY drawn into). The desktop port's
                // creation-frozen GL surface stays the documented
                // deviation (the winit arm below — the size bookkeeping
                // only, the 025g `Resized` fold).
                #[cfg(target_os = "emscripten")]
                if let Some(gl) = window.gl.as_mut() {
                    gl.resize(width, height);
                }
                window.set_size(width, height);
            }
            Event::TouchBegan { finger, position } => {
                window.input.set_finger(finger, true);
                window.finger_down_positions.insert(finger, position);
                window.with_touch_sink(|slots| {
                    if (finger as usize) < slots.len() {
                        slots[finger as usize] = Some(position);
                    }
                });
            }
            Event::TouchMoved { finger, position } => {
                window.finger_down_positions.insert(finger, position);
                window.with_touch_sink(|slots| {
                    if (finger as usize) < slots.len() {
                        slots[finger as usize] = Some(position);
                    }
                });
            }
            Event::TouchEnded { finger } => {
                window.input.set_finger(finger, false);
                window.finger_down_positions.remove(&finger);
                window.with_touch_sink(|slots| {
                    if (finger as usize) < slots.len() {
                        slots[finger as usize] = None;
                    }
                });
                // The Android touch seam (the SFML window layer turns a
                // touch release into the C++'s `MouseButtonReleased`)
                // — the boot-screen handler sees it the same way.
                window.with_released_sink(|edges| {
                    edges.fingers.insert(finger);
                });
            }
            // C++ `MenuGame::onEvent(MouseWheelScrolled)` (MenuGame.cpp:
            // 299-356) — the handler runs PER EVENT (the C++ `runEvents`
            // drains the pending events between polls; the port's
            // per-frame `refresh_input` drain calls the handler per
            // recorded delta, the C++'s per-event order — one entry per
            // event, like the entered-text sink). Sign: the SFML `delta`
            // and the winit `dy` agree (the variant's docs) — no flip.
            Event::MouseWheelScrolled { delta } => {
                window.with_wheel_sink(|events| events.push(delta));
            }
            // C++ `runEvents` (GameWindow.hpp:42-83): the `TextEntered`
            // event has NO input-state fold half — it is dispatched
            // straight to the state's `onEvent` handler (the C++
            // `MenuGame` handler, MenuGame.cpp:491-498, appends to
            // `enteredChars`). The port's fold records the char in the
            // shared sink; the menu state's `refresh_input` drain runs
            // the handler (its `unicode < 128` filter included — the
            // handler's position, 1:1).
            Event::TextEntered { character } => {
                window.with_entered_text_sink(|chars| {
                    chars.push(character);
                });
            }
        }
    }
}

/// `ApplicationHandler` that folds winit window events into SFML `Event`s and
/// drives the per-frame callback on `about_to_wait` (the winit 0.30 frame-pace
/// point — `MainEventsCleared` was removed in winit 0.29). `on_event` is the
/// issue-014b raw-event hook (C++ `runEvents`'s per-event
/// `gameEngine->handleEvent`, GameWindow.hpp:79 — called AFTER the SFML
/// input-state fold, the C++'s order). Desktop/android only (the web path has
/// no winit loop — the rAF `Runner` in [`Window::run`]).
#[cfg(not(target_os = "emscripten"))]
struct RunHandler<'a, F> {
    window: &'a mut Window,
    on_event: Option<Box<EventHook>>,
    on_frame: &'a mut F,
}

#[cfg(not(target_os = "emscripten"))]
impl<F: FnMut(&mut Window)> ApplicationHandler<()> for RunHandler<'_, F> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        // The C++ loop model is an UNCAPPED spin: SFML's `poll()` is
        // non-blocking, so `while (isOpen) { poll; update; draw; }`
        // (GameWindow.hpp:117-150) iterates as fast as it can. winit
        // 0.30's default is `ControlFlow::Wait` — `about_to_wait` fires
        // once per EVENT WAKEUP, so without `Poll` the loop only ticks
        // while events stream in (the on-device smoke caught it: ~20
        // frames through the android lifecycle burst, then silence;
        // desktop masked it — the scripted tests inject an event per
        // step). `Poll` = winit returns from the wait immediately, the
        // faithful `while (isOpen)` spin (issue 25, on-device finding).
        el.set_control_flow(winit::event_loop::ControlFlow::Poll);
        // Android (issue 25b SG2): the first `resumed` IS the
        // `InitWindow` dispatch — the `ANativeWindow` exists from here;
        // materialize the platform window + the GLES context (once; the
        // `ensure_android_gl_failed` guard covers the re-`resumed`s).
        // Desktop: the statement is android-gated (no-op).
        #[cfg(target_os = "android")]
        self.window.ensure_android_gl(el);
    }

    fn window_event(&mut self, _el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(e) = map_window_event(&event) {
            // Android (issue 25b SG5; the space clarified by issue 025g):
            // winit delivers the touch position in PHYSICAL pixels of the
            // window's drawable — which IS the window's current size space
            // (the `width_px` / `height_px` the `Resized` fold keeps
            // current; the C++'s `mapPixelToCoords` maps to the window's
            // CURRENT size — the same space, so the raw position is the
            // coordinate the C++'s `getFingerDownPositions()` returns on
            // the phone). The 025g menu grid's thirds run on that same
            // current size (the `MenuWindow` seam — `oh_app::windowed`
            // `tick_fields`), and the engine's 25b SG5 half-width
            // comparison runs on it too (its documented contract —
            // `Config::getWidth()/2` threshold, oh-game). The 025b-era
            // physical → config-logical rescale (window size ÷ GL-surface
            // size) is REMOVED: after a post-GL window resize (the IME's
            // ADJUST_RESIZE shrinks the native window; the `GlParts`
            // bookkeeping is creation-frozen) the scale's denominator
            // went stale and squashed the positions out of the grid's
            // thirds space (a window-center tap landed in the wrong band).
            // Desktop: `WindowEvent::Touch` never arrives (the arm is
            // dead there) — behavior-identical.
            Window::apply_event(self.window, e);
        }
        // SFML dispatches a `TextEntered` event ALONGSIDE the
        // `KeyPressed` for a character key (the C++ `runEvents` polls
        // them as two events — the `MenuGame`'s `onEvent(TextEntered)`
        // handler, MenuGame.cpp:265-272, appends to
        // `enteredChars`). winit carries the character in the same
        // `KeyboardInput`'s `text` field (the Android device keymap —
        // the issue-025d on-device evidence: `adb input keyevent 66`
        // arrived with `text: Some("\n")`), so the second SFML event is
        // restored here (the `map_window_event` 1:1 single mapping
        // stays). The `Pressed` gate: SFML's `TextEntered` fires on the
        // press half only (the release's `text` is `None` on the
        // device). The Backspace half (29b3): SFML also emits
        // `TextEntered` with unicode 8 for Backspace (the C++'s second
        // `TextEntered` handler's `pop_back` arm — the dialog input box
        // erase, MenuGame.cpp:509-516 — fires on it; the Android
        // keymap's `AKEYCODE_DEL` → `\u{8}` text covers the same
        // event), but winit carries no `text` for the Backspace key, so
        // the second SFML event is restored from the physical key
        // (the `else` — a keymap that DID deliver the char through
        // `text` must not double-pop).
        if let WindowEvent::KeyboardInput { event: ke, .. } = &event {
            if ke.state == winit::event::ElementState::Pressed {
                if let Some(c) = ke.text.as_deref().and_then(|s| s.chars().next()) {
                    Window::apply_event(self.window, Event::TextEntered { character: c });
                } else if ke.physical_key == PhysicalKey::Code(KeyCode::Backspace)
                    || ke.logical_key == WKey::Named(NamedKey::Backspace)
                {
                    Window::apply_event(self.window, Event::TextEntered { character: '\u{8}' });
                }
            }
        }
        // C++ `runEvents` (GameWindow.hpp:42-83): the input state is
        // updated FIRST, then `gameEngine->handleEvent(event)` (the game's
        // `onAnyEvent` = `Imgui::processEvent`, HexagonGame.cpp:335 — the
        // ImGui console's event feed; the port's egui equivalent). The
        // hook is GL-gated: headless (`gl == None`) has no console (the
        // C++'s `window == nullptr` early-return, HGUpdate.cpp:1290-1293).
        if let (Some(hook), Some(gl)) = (self.on_event.as_deref_mut(), &self.window.gl) {
            hook(gl.winit_window(), &event);
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.window.open {
            (self.on_frame)(self.window);
        }
        if !self.window.open {
            el.exit();
            return;
        }
        // Pace the next frame: the C++ SFML `setFramerateLimit` sleep (the
        // shipped `_RELEASE/config.json` has `limit_fps=true`, `max_fps=200`,
        // `vsync=false`). `WaitUntil` when a framerate limit is in force,
        // `Poll` (continuous, unthrottled) otherwise — mirroring the C++
        // continuous `GameWindow::run` loop.
        let limit = self.window.effective_framerate_limit();
        if limit > 0.0 {
            el.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_secs_f64(1.0 / f64::from(limit)),
            ));
        } else {
            el.set_control_flow(ControlFlow::Poll);
        }
        // winit-android (issue 025i): re-request a redraw after EVERY frame.
        // The C++ `GameWindow::run` (GameWindow.hpp:117-150) draws
        // UNCONDITIONALLY each iteration (`while (isRunning) { update; draw;
        // display; }` — there is no "only redraw if a redraw was requested"
        // gate; the SFML `setFramerateLimit` sleep is the only throttle). On
        // desktop winit the `WaitUntil`/vsync already runs a frame per loop
        // iteration, so this is a no-op there. On Android, however, winit's
        // event loop (`poll_events_with_timeout`) SKIPS the frame AND the
        // input read on a `PollEvent::Wake` that carries no pending redraw
        // request (the early-return that `pending_redraw` gates); the native
        // window's compositor wakes the app looper every frame once a level
        // is actively compositing, so the loop then spins with no frames and
        // no input consumed — the 103%-CPU ANR (hypothesis 1: frame pacing /
        // the event loop). Re-requesting the redraw each frame keeps the
        // C++'s unconditional per-iteration draw (and the per-iteration input
        // read, `GameWindow::runEvents`) alive on Android. The C++ has no
        // redraw-request concept (it always draws); this request only keeps
        // the winit-android loop faithful to that — a documented deviation
        // (register row, issue 28's rule).
        #[cfg(target_os = "android")]
        if let Some(gl) = self.window.gl.as_ref() {
            gl.winit_window().request_redraw();
        }
    }
}

// ---------------------------------------------------------------------------
// Emscripten (W1) — the DOM event listeners (the winit event loop's web
// replacement)
// ---------------------------------------------------------------------------

/// Register one DOM listener; keep the `Closure` in `listeners` ONLY when the
/// registration succeeded (a dropped `Closure` dangles the JS callback).
/// `own_assert_unwind_safe` — the handlers capture the `RefCell` queue (not
/// `UnwindSafe`), and a DOM event handler must never unwind into JS anyway
/// (the web build ships `panic = "abort"`).
#[cfg(target_os = "emscripten")]
fn add_dom_listener(
    target: &web_sys::EventTarget,
    ty: &str,
    listeners: &mut Vec<Closure<dyn FnMut(web_sys::Event)>>,
    handler: impl FnMut(web_sys::Event) + 'static,
) {
    let cb = Closure::own_assert_unwind_safe(handler);
    if target
        .add_event_listener_with_callback(ty, cb.as_ref().unchecked_ref::<js_sys::Function>())
        .is_ok()
    {
        listeners.push(cb);
    }
}

/// The page's device pixel ratio (the DOM `window.devicePixelRatio` — the
/// CSS-px → physical-px scale; `1.0` when the window is unavailable or the
/// value is degenerate). issue 32 §2.3: the REAL window size is the canvas
/// CSS box × this (the 025g lesson — a wrong DPR mapping silently breaks
/// every hit test).
#[cfg(target_os = "emscripten")]
fn web_device_pixel_ratio() -> f64 {
    web_sys::window()
        .map(|w| w.device_pixel_ratio())
        .filter(|d| *d > 0.0)
        .unwrap_or(1.0)
}

/// The canvas's REAL size in PHYSICAL pixels (issue 32 §2.3): the CSS box
/// (the bounding rect — the layout box, not the backing-store attributes)
/// × the device pixel ratio. The `Window`'s size bookkeeping + the GL
/// canvas + the pointer positions all live in this space (the winit path's
/// "physical drawable pixels ARE the window's current size space" — the
/// 025g invariant).
#[cfg(target_os = "emscripten")]
#[must_use]
fn web_canvas_real_size(canvas: &web_sys::HtmlCanvasElement) -> (u32, u32) {
    let dpr = web_device_pixel_ratio();
    let rect = canvas.get_bounding_client_rect();
    (
        (rect.width() * dpr).max(1.0) as u32,
        (rect.height() * dpr).max(1.0) as u32,
    )
}

/// The DOM client position in the WINDOW's current size space (the SFML
/// window-relative mouse/touch position — the C++ `mapPixelToCoords`'s
/// pixel half; the canvas's bounding rect is the window origin). The CSS
/// box coordinates (client − rect origin) × the device pixel ratio — the
/// same physical space `web_canvas_real_size` reports (spec §2.3/§2.5: a
/// position in a different space than the window size rotates / hit-tests
/// wrongly rather than crashing). The web-sys stable API exposes the
/// client coordinates as `f32` (the DOM doubles, truncated); the rect is
/// `f64`.
#[cfg(target_os = "emscripten")]
fn canvas_position(canvas: &web_sys::HtmlCanvasElement, client_x: f64, client_y: f64) -> Vec2 {
    let dpr = web_device_pixel_ratio();
    let rect = canvas.get_bounding_client_rect();
    Vec2::new(
        ((client_x - rect.left()) * dpr) as f32,
        ((client_y - rect.top()) * dpr) as f32,
    )
}

/// Attach the DOM → SFML event listeners (the SFML `runEvents` event
/// source; issue 32): keyboard + focus/blur/resize on the `window` (the
/// page focus, not the canvas focus) + `visibilitychange` on the
/// `document`; the POINTER events (`pointerdown` / `pointermove` /
/// `pointerup` / `pointercancel`) + `wheel` on the canvas (positions in
/// the window's current size space — `canvas_position`). The pointer
/// events carry BOTH the mouse (the `pointerType` `"mouse"` → the SFML
/// mouse-button fold) and the touch / pen (the `"touch"` / `"pen"` → the
/// SFML touch fold — the legacy `MouseEvent` / `TouchEvent` listeners are
/// gone: the pointer model is their unified replacement, and it is what
/// the issue-31 overlay's pointer feed expects). Each mapped event is
/// pushed to the shared queue the rAF loop drains per frame. Returns the
/// closures — the caller MUST keep them alive for the window's life.
#[cfg(target_os = "emscripten")]
fn attach_web_listeners(
    canvas: &web_sys::HtmlCanvasElement,
    queue: &std::sync::Arc<std::cell::RefCell<Vec<Event>>>,
) -> Vec<Closure<dyn FnMut(web_sys::Event)>> {
    let window = web_sys::window().expect("web window (attach is web-only)");
    let document = window
        .document()
        .expect("web document (attach is web-only)");
    let mut listeners = Vec::new();

    // ---- Keyboard (the DOM `code`/`key` → SFML `Key`, `web_key_to_sfml`) ----

    // `keydown` — the `KeyPressed` + the alongside `TextEntered` (the
    // C++ `runEvents` polls them as two events — the `MenuGame`'s
    // `onEvent(TextEntered)` handler, MenuGame.cpp:491-498, appends to
    // `enteredChars`; master's `entered_text_sink` (issue 025d) is fed
    // here on web). The guards keep the character to a single printable
    // char: no modifier combo (the ctrl/alt/meta `key` values are
    // non-printable control chars), a single-char `key` (this excludes
    // Enter — the `key` "Enter" is a word, not a char — and every
    // functional key whose `key` is a named word), and no repeat (the
    // C++ `TextEntered` fires on the press half only — the winit fold's
    // `Pressed` gate, the held-key repeat would double-type). The
    // character comes from `key`, deliberately INDEPENDENT of the SFML
    // key-code map (text input must not depend on `code` being mappable
    // — the synthetic/IME paths leave it empty). The Backspace half
    // (29b3, the desktop fold's twin): SFML emits a `TextEntered` with
    // unicode 8 for Backspace (the C++'s dialog-input `pop_back` arm —
    // the erase, MenuGame.cpp:509-516); the DOM carries no `key` char
    // for it, so it is restored from the physical `code`.
    let q = queue.clone();
    add_dom_listener(&window, "keydown", &mut listeners, move |e| {
        let Some(ke) = e.dyn_ref::<web_sys::KeyboardEvent>() else {
            return;
        };
        let prevent = web_keydown_prevent_default(
            ke.code().as_str(),
            ke.key().as_str(),
            ke.ctrl_key(),
            ke.meta_key(),
            ke.is_composing(),
        );
        if prevent {
            e.prevent_default(); // the page must not also scroll / zoom / move focus
        }
        if !ke.ctrl_key() && !ke.alt_key() && !ke.meta_key() && !ke.repeat() {
            let key = ke.key();
            if key.chars().count() == 1 {
                if let Some(c) = key.chars().next() {
                    q.borrow_mut().push(Event::TextEntered { character: c });
                }
            } else if ke.code() == "Backspace" {
                q.borrow_mut()
                    .push(Event::TextEntered { character: '\u{8}' });
            }
        }
        let code = web_key_to_sfml(ke.code().as_str(), ke.key().as_str());
        if code == Key::UNKNOWN {
            return;
        }
        q.borrow_mut().push(Event::KeyPressed {
            code,
            repeated: ke.repeat(),
        });
    });
    // `keyup` — the `KeyReleased` (no `preventDefault`: the browser
    // shortcuts ride the press half, and a swallowed release would leave
    // the OS key state inconsistent).
    let q = queue.clone();
    add_dom_listener(&window, "keyup", &mut listeners, move |e| {
        let Some(ke) = e.dyn_ref::<web_sys::KeyboardEvent>() else {
            return;
        };
        let code = web_key_to_sfml(ke.code().as_str(), ke.key().as_str());
        if code == Key::UNKNOWN {
            return;
        }
        q.borrow_mut().push(Event::KeyReleased { code });
    });

    // ---- Pointer (the issue-31 pointer feed's DOM source, spec §2.1) ----

    // The pointer-state machine (the `WebPointerFeed`'s docs: the type
    // dispatch + the release-once guard) — shared across the four
    // pointer listeners (a `pointermove` must see the `pointerdown`'s
    // slot / button record).
    let feed = std::rc::Rc::new(std::cell::RefCell::new(WebPointerFeed::new()));
    let c = canvas.clone();
    let q = queue.clone();
    let f = feed.clone();
    add_dom_listener(&canvas, "pointerdown", &mut listeners, move |e| {
        let Some(pe) = e.dyn_ref::<web_sys::PointerEvent>() else {
            return;
        };
        // The stuck-down guard (spec §2.1): capture the pointer so a drag
        // OFF the canvas still delivers its `pointerup` / `pointercancel`
        // here. `let _ =` — a capture of a pointer the browser does not
        // track (an untrusted / synthetic event) throws
        // `NotSupportedError`; the event still folds (the capture is the
        // DELIVERY guarantee, the fold is the mapping).
        let _ = c.set_pointer_capture(pe.pointer_id());
        e.prevent_default(); // no text-selection / drag default on a game press
        let position = canvas_position(&c, pe.client_x() as f64, pe.client_y() as f64);
        let events = f.borrow_mut().pointer_down(
            pe.pointer_type().as_str(),
            pe.pointer_id() as u32,
            pe.button(),
            position,
        );
        q.borrow_mut().extend(events);
    });
    let c = canvas.clone();
    let q = queue.clone();
    let f = feed.clone();
    add_dom_listener(&canvas, "pointermove", &mut listeners, move |e| {
        let Some(pe) = e.dyn_ref::<web_sys::PointerEvent>() else {
            return;
        };
        let position = canvas_position(&c, pe.client_x() as f64, pe.client_y() as f64);
        let events = f.borrow_mut().pointer_move(
            pe.pointer_type().as_str(),
            pe.pointer_id() as u32,
            position,
        );
        q.borrow_mut().extend(events);
    });
    let q = queue.clone();
    let f = feed.clone();
    add_dom_listener(&canvas, "pointerup", &mut listeners, move |e| {
        let Some(pe) = e.dyn_ref::<web_sys::PointerEvent>() else {
            return;
        };
        let events = f.borrow_mut().pointer_up(
            pe.pointer_type().as_str(),
            pe.pointer_id() as u32,
            pe.button(),
        );
        q.borrow_mut().extend(events);
    });
    let q = queue.clone();
    let f = feed.clone();
    add_dom_listener(&canvas, "pointercancel", &mut listeners, move |e| {
        let Some(pe) = e.dyn_ref::<web_sys::PointerEvent>() else {
            return;
        };
        let events = f
            .borrow_mut()
            .pointer_cancel(pe.pointer_type().as_str(), pe.pointer_id() as u32);
        q.borrow_mut().extend(events);
    });

    // ---- Wheel (the 29b2 wheel sink's data path) ----
    // The DOM `deltaY` is positive for scroll DOWN — the winit
    // `MouseScrollDelta` sign is inverted: positive = up, so negate; the
    // winit mapping: line deltas pass through, pixel deltas /10 — the DOM
    // `deltaMode` constants: 0 = pixels, 1 = lines, 2 = pages (web-sys
    // exposes the raw `u32`; pages are treated as the pixel arm). The
    // listener is on the CANVAS (a wheel over the game is the game's —
    // the page must not also scroll / zoom: `preventDefault` always).
    let q = queue.clone();
    add_dom_listener(&canvas, "wheel", &mut listeners, move |e| {
        let Some(we) = e.dyn_ref::<web_sys::WheelEvent>() else {
            return;
        };
        e.prevent_default();
        let delta = if we.delta_mode() == 1 {
            -we.delta_y() as f32
        } else {
            -we.delta_y() as f32 / 10.0
        };
        q.borrow_mut().push(Event::MouseWheelScrolled { delta });
    });

    // ---- Focus / blur / visibility (the SFML `GainedFocus` /
    // `LostFocus` — the fold's `LostFocus` arm releases EVERY held input,
    // the C++'s `reset()` on focus loss; spec §2.4) ----

    let q = queue.clone();
    add_dom_listener(&window, "focus", &mut listeners, move |_e| {
        q.borrow_mut().push(Event::GainedFocus);
    });
    // The `blur` (the window lost focus) AND the `visibilitychange` →
    // hidden (the tab was backgrounded — the `blur` alone misses the
    // browser's own tab-switch / backgrounding, where the page keeps
    // "focus" in its own DOM sense but no key can reach it; a key held
    // when the tab hides must not stay stuck on return, spec §2.4).
    let q = queue.clone();
    add_dom_listener(&window, "blur", &mut listeners, move |_e| {
        q.borrow_mut().push(Event::LostFocus);
    });
    let q = queue.clone();
    add_dom_listener(&document, "visibilitychange", &mut listeners, move |_e| {
        let hidden = web_sys::window()
            .and_then(|w| w.document())
            .is_some_and(|d| d.visibility_state() == web_sys::VisibilityState::Hidden);
        if hidden {
            q.borrow_mut().push(Event::LostFocus);
        }
    });

    // ---- Resize (the SFML `Resized` with the REAL size — the CSS box ×
    // the device pixel ratio, `web_canvas_real_size`; spec §2.3). The
    // fold's `Resized` arm does the window bookkeeping; on emscripten it
    // ALSO resizes the GL canvas (the `GlParts::resize` — the C++
    // `recreateWindow`'s render-window half: the desktop's creation-frozen
    // GL surface stays the documented deviation, the web canvas resizes
    // cheaply, so the web window tracks the real size 1:1). ----
    let c = canvas.clone();
    let q = queue.clone();
    add_dom_listener(&window, "resize", &mut listeners, move |_e| {
        let (width, height) = web_canvas_real_size(&c);
        q.borrow_mut().push(Event::Resized { width, height });
    });

    listeners
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C++ `GameWindow::msUpdate` / `msDraw` (GameWindow.hpp:36,270-276):
    /// the members are default-initialized (`0`) and the windowed loop
    /// records each frame's phase durations; the issue-014b console footer
    /// reads them back through the getters.
    #[test]
    fn ms_phase_durations_default_zero_and_round_trip() {
        let mut w = Window::headless(640, 480);
        assert_eq!(w.get_ms_update(), 0.0);
        assert_eq!(w.get_ms_draw(), 0.0);
        w.set_ms_update(3.25);
        w.set_ms_draw(1.75);
        assert_eq!(w.get_ms_update(), 3.25);
        assert_eq!(w.get_ms_draw(), 1.75);
    }

    /// The event-hook loop variant is headless-safe (the C++
    /// `window == nullptr` early-return, HGUpdate.cpp:1290-1293): without a
    /// real event loop the frame callback runs exactly once and the hook —
    /// the egui console's `Imgui::processEvent` analogue (the C++
    /// `runEvents` per-event dispatch, GameWindow.hpp:79) — is never fired.
    /// Desktop/android only (`run_with_event_hook` is the winit path's).
    #[cfg(not(target_os = "emscripten"))]
    #[test]
    fn run_with_event_hook_headless_runs_one_frame_and_never_fires_the_hook() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;
        let frames = Arc::new(AtomicU32::new(0));
        let hook_fired = Arc::new(AtomicU32::new(0));
        let (frames_c, hook_c) = (frames.clone(), hook_fired.clone());
        let w = Window::headless(640, 480);
        w.run_with_event_hook(
            Some(Box::new(move |_win, _event| {
                hook_c.store(1, Ordering::SeqCst)
            })),
            move |_win| frames_c.store(1, Ordering::SeqCst),
        );
        assert_eq!(frames.load(Ordering::SeqCst), 1);
        assert_eq!(hook_fired.load(Ordering::SeqCst), 0);
    }

    // issue 25b SG5: the touch → engine seam (the C++
    // `getFingerDownPositions` data path; the positions arrive in the
    // window's CURRENT size coordinates — the raw winit coordinates,
    // issue 025g: the 025b-era physical→config-logical rescale lived in
    // `RunHandler::window_event` and went stale after a post-GL window
    // resize; the fold now passes the position through).
    #[test]
    fn touch_events_fold_into_input_state_and_the_sink() {
        use crate::input::FINGER_COUNT;
        use std::sync::{Arc, RwLock};

        let mut w = Window::headless(800, 600);
        let sink: Arc<RwLock<[Option<Vec2>; FINGER_COUNT]>> =
            Arc::new(RwLock::new([None; FINGER_COUNT]));
        w.set_touch_position_sink(sink.clone());

        // Finger 2 down at (100, 300) logical (the C++'s left half of an
        // 800-wide window — the `updateInput_UpdateTouchControls` CCW arm).
        Window::apply_event(
            &mut w,
            Event::TouchBegan {
                finger: 2,
                position: Vec2::new(100.0, 300.0),
            },
        );
        assert!(w.input.is_finger(2));
        assert_eq!(w.finger_down_positions(), vec![Vec2::new(100.0, 300.0)]);
        assert_eq!(
            sink.read().expect("poisoned")[2],
            Some(Vec2::new(100.0, 300.0))
        );
        assert!(sink.read().expect("poisoned")[0].is_none());

        // The move updates both (the down position tracks the finger).
        Window::apply_event(
            &mut w,
            Event::TouchMoved {
                finger: 2,
                position: Vec2::new(500.0, 300.0),
            },
        );
        assert_eq!(w.finger_down_positions(), vec![Vec2::new(500.0, 300.0)]);
        assert_eq!(
            sink.read().expect("poisoned")[2],
            Some(Vec2::new(500.0, 300.0))
        );

        // A second finger: the C++'s 0..FINGER_COUNT order (ascending id).
        Window::apply_event(
            &mut w,
            Event::TouchBegan {
                finger: 1,
                position: Vec2::new(700.0, 300.0),
            },
        );
        assert_eq!(
            w.finger_down_positions(),
            vec![Vec2::new(700.0, 300.0), Vec2::new(500.0, 300.0)]
        );

        // Up: the bit clears, both storages drop the position.
        Window::apply_event(&mut w, Event::TouchEnded { finger: 2 });
        assert!(!w.input.is_finger(2));
        assert!(w.input.is_finger(1));
        assert_eq!(w.finger_down_positions(), vec![Vec2::new(700.0, 300.0)]);
        assert!(sink.read().expect("poisoned")[2].is_none());
    }

    /// issue 025d: the `TextEntered` fold arm (the C++ `runEvents`
    /// `TextEntered` dispatch — GameWindow.hpp:42-83's no-input-state
    /// half: the char goes straight to the state's `onEvent` handler,
    /// which the port routes through the shared entered-text sink; the
    /// handler's `unicode < 128` filter is the menu drain's, 1:1).
    #[test]
    fn text_entered_fold_appends_to_the_entered_text_sink() {
        use std::sync::{Arc, RwLock};

        let mut w = Window::headless(800, 480);
        let sink: Arc<RwLock<Vec<char>>> = Arc::new(RwLock::new(Vec::new()));
        w.set_entered_text_sink(sink.clone());

        Window::apply_event(&mut w, Event::TextEntered { character: 'D' });
        Window::apply_event(&mut w, Event::TextEntered { character: 'A' });
        // The fold records the char as-is (the `unicode < 128` filter is
        // the C++ handler's — MenuGame.cpp:492 — the menu drain's, not
        // the fold's).
        Window::apply_event(&mut w, Event::TextEntered { character: '中' });
        assert_eq!(*sink.read().expect("poisoned"), vec!['D', 'A', '中']);

        // No sink installed (headless / pre-install): the fold arm is a
        // no-op (the `with_entered_text_sink` `None` arm) — no panic.
        let mut bare = Window::headless(800, 480);
        Window::apply_event(&mut bare, Event::TextEntered { character: 'x' });
    }

    /// issue 029b2: the `MouseWheelScrolled` fold arm (the C++
    /// `MenuGame::onEvent(MouseWheelScrolled)` handler's data path —
    /// MenuGame.cpp:299-356). The delta's SIGN is the SFML `delta`
    /// convention (positive = wheel up, the variant's docs: the winit
    /// X11 Button4 → `LineDelta(0, +1)` / Button5 → `LineDelta(0, -1)`
    /// mapping + the SFML fork's `Event.hpp` comment) — the fold records
    /// it as-is (no sign flip; a flip would invert the C++'s wheel
    /// navigation and level-list scroll).
    #[test]
    fn wheel_fold_appends_the_delta_to_the_wheel_events_sink() {
        use std::sync::{Arc, RwLock};

        let mut w = Window::headless(800, 480);
        let sink: Arc<RwLock<Vec<f32>>> = Arc::new(RwLock::new(Vec::new()));
        w.set_wheel_events_sink(sink.clone());

        // One entry per event, in arrival order (the C++ `runEvents`
        // per-event order the menu drain preserves).
        Window::apply_event(&mut w, Event::MouseWheelScrolled { delta: 1.0 });
        Window::apply_event(&mut w, Event::MouseWheelScrolled { delta: -2.0 });
        Window::apply_event(&mut w, Event::MouseWheelScrolled { delta: 0.5 });
        assert_eq!(*sink.read().expect("poisoned"), vec![1.0, -2.0, 0.5]);

        // No sink installed (headless / pre-install): the fold arm is a
        // no-op (the `with_wheel_sink` `None` arm) — no panic.
        let mut bare = Window::headless(800, 480);
        Window::apply_event(&mut bare, Event::MouseWheelScrolled { delta: 1.0 });
    }

    /// issue 029b2: the winit `MouseWheel` → SFML `MouseWheelScrolled`
    /// mapping's sign (the wheel-inversion trap, checked from the winit
    /// source rather than assumed — the `Event::MouseWheelScrolled`
    /// variant's docs): line deltas pass through as-is, pixel deltas are
    /// scaled by `1/10` (the fold's established `pos.y / 10.0`), and a
    /// wheel-UP event (X11 Button4 → `LineDelta(0, +1)`, winit 0.30.13
    /// `event_processor.rs:1083-1091`) stays POSITIVE — the SFML fork's
    /// `delta` "positive is up" convention.
    /// Desktop/android only (`map_window_event` is the winit path's).
    #[cfg(not(target_os = "emscripten"))]
    #[test]
    fn map_window_event_mouse_wheel_keeps_the_sfml_sign() {
        use winit::dpi::PhysicalPosition;
        use winit::event::DeviceId;

        let wheel = |delta: winit::event::MouseScrollDelta| WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta,
            phase: winit::event::TouchPhase::Moved,
        };
        match map_window_event(&wheel(winit::event::MouseScrollDelta::LineDelta(0.0, 1.0))).unwrap()
        {
            Event::MouseWheelScrolled { delta } => assert_eq!(delta, 1.0),
            other => panic!("expected MouseWheelScrolled, got {other:?}"),
        }
        match map_window_event(&wheel(winit::event::MouseScrollDelta::LineDelta(0.0, -1.0)))
            .unwrap()
        {
            Event::MouseWheelScrolled { delta } => assert_eq!(delta, -1.0),
            other => panic!("expected MouseWheelScrolled, got {other:?}"),
        }
        // Pixel deltas (touchpads): `pos.y / 10.0`, sign preserved.
        match map_window_event(&wheel(winit::event::MouseScrollDelta::PixelDelta(
            PhysicalPosition::new(0.0, -120.0),
        )))
        .unwrap()
        {
            Event::MouseWheelScrolled { delta } => assert_eq!(delta, -12.0),
            other => panic!("expected MouseWheelScrolled, got {other:?}"),
        }
    }

    /// Desktop/android only (`map_window_event` is the winit path's).
    #[cfg(not(target_os = "emscripten"))]
    #[test]
    fn map_window_event_touch_phases_carry_the_position() {
        use winit::dpi::PhysicalPosition;
        use winit::event::DeviceId;

        let touch = |phase: winit::event::TouchPhase| {
            WindowEvent::Touch(winit::event::Touch {
                device_id: DeviceId::dummy(),
                phase,
                location: PhysicalPosition::new(40.0, 90.0),
                force: None,
                id: 3,
            })
        };
        match map_window_event(&touch(winit::event::TouchPhase::Started)).unwrap() {
            Event::TouchBegan { finger, position } => {
                assert_eq!(finger, 3);
                assert_eq!(position, Vec2::new(40.0, 90.0));
            }
            other => panic!("expected TouchBegan, got {other:?}"),
        }
        match map_window_event(&touch(winit::event::TouchPhase::Moved)).unwrap() {
            Event::TouchMoved { finger, position } => {
                assert_eq!(finger, 3);
                assert_eq!(position, Vec2::new(40.0, 90.0));
            }
            other => panic!("expected TouchMoved, got {other:?}"),
        }
        match map_window_event(&touch(winit::event::TouchPhase::Ended)).unwrap() {
            Event::TouchEnded { finger } => assert_eq!(finger, 3),
            other => panic!("expected TouchEnded, got {other:?}"),
        }
        // spec 31 §5's `pointercancel`: a CANCELED touch folds to the
        // `Ended` arm (the C++ SFML Android activity's
        // `ACTION_CANCEL` → `TouchEnded`; the pre-fix discard left a
        // phantom down finger — the module's fold note).
        match map_window_event(&touch(winit::event::TouchPhase::Cancelled)).unwrap() {
            Event::TouchEnded { finger } => assert_eq!(finger, 3),
            other => panic!("expected TouchEnded (the cancel fold), got {other:?}"),
        }
    }

    // ------------------------------------------------------------------
    // issue 32 — the web input mapping (the in-process half of the spec's
    // §3: the synthetic DOM-shaped event → the right sink entry / key
    // code, pinned against the SFML integers of oracle-vectors.md so
    // replays and menu binds cannot diverge by target).
    // ------------------------------------------------------------------

    /// issue 32 §2.2 — the DOM `code`/`key` → the pinned SFML `Key`
    /// integers (oracle-vectors.md's `enum Key`): the families (letters,
    /// digits, numpad), the function keys, the modifiers, the
    /// punctuation-by-`key` arm, and the unmappable fall-through.
    #[test]
    fn web_key_to_sfml_pins_the_sfml_integers() {
        // Letters / digits / numpad (A=0..Z=25, Num0=26..Num9=35,
        // Numpad0=75..Numpad9=84).
        assert_eq!(web_key_to_sfml("KeyA", "a"), Key::A);
        assert_eq!(web_key_to_sfml("KeyZ", "z"), Key::Z);
        assert_eq!(web_key_to_sfml("KeyM", "m"), Key::M);
        assert_eq!(web_key_to_sfml("Digit0", "0"), Key::NUM0);
        assert_eq!(web_key_to_sfml("Digit9", "9"), Key::NUM9);
        assert_eq!(web_key_to_sfml("Numpad0", "0"), Key::NUMPAD0);
        assert_eq!(web_key_to_sfml("Numpad5", "5"), Key::NUMPAD5);
        assert_eq!(web_key_to_sfml("Numpad9", "9"), Key::NUMPAD9);
        // The numpad's operators + enter + decimal (Add=67..Divide=70,
        // NumpadDecimal → Period=50).
        assert_eq!(web_key_to_sfml("NumpadAdd", "+"), Key::ADD);
        assert_eq!(web_key_to_sfml("NumpadSubtract", "-"), Key::SUBTRACT);
        assert_eq!(web_key_to_sfml("NumpadMultiply", "*"), Key::MULTIPLY);
        assert_eq!(web_key_to_sfml("NumpadDivide", "/"), Key::DIVIDE);
        assert_eq!(web_key_to_sfml("NumpadEnter", "Enter"), Key::ENTER);
        assert_eq!(web_key_to_sfml("NumpadDecimal", "."), Key::PERIOD);
        // The functional keys (Space=57, Enter=58, Backspace=59, Tab=60,
        // the arrows Left=71..Down=74, Escape=36, Pause=100, the
        // context Menu=45).
        assert_eq!(web_key_to_sfml("Space", " "), Key::SPACE);
        assert_eq!(web_key_to_sfml("Enter", "Enter"), Key::ENTER);
        assert_eq!(web_key_to_sfml("Backspace", "Backspace"), Key::BACKSPACE);
        assert_eq!(web_key_to_sfml("Tab", "Tab"), Key::TAB);
        assert_eq!(web_key_to_sfml("ArrowLeft", "ArrowLeft"), Key::LEFT);
        assert_eq!(web_key_to_sfml("ArrowRight", "ArrowRight"), Key::RIGHT);
        assert_eq!(web_key_to_sfml("ArrowUp", "ArrowUp"), Key::UP);
        assert_eq!(web_key_to_sfml("ArrowDown", "ArrowDown"), Key::DOWN);
        assert_eq!(web_key_to_sfml("Escape", "Escape"), Key::ESCAPE);
        assert_eq!(web_key_to_sfml("Pause", "Pause"), Key::PAUSE);
        assert_eq!(web_key_to_sfml("ContextMenu", "ContextMenu"), Key::MENU);
        // The modifiers (LControl=37, LShift=38, LAlt=39, LSystem=40,
        // RControl=41, RShift=42, RAlt=43, RSystem=44).
        assert_eq!(web_key_to_sfml("ShiftLeft", "Shift"), Key::L_SHIFT);
        assert_eq!(web_key_to_sfml("ShiftRight", "Shift"), Key::R_SHIFT);
        assert_eq!(web_key_to_sfml("ControlLeft", "Control"), Key::L_CONTROL);
        assert_eq!(web_key_to_sfml("ControlRight", "Control"), Key::R_CONTROL);
        assert_eq!(web_key_to_sfml("AltLeft", "Alt"), Key::L_ALT);
        assert_eq!(web_key_to_sfml("AltRight", "Alt"), Key::R_ALT);
        assert_eq!(web_key_to_sfml("MetaLeft", "Meta"), Key::L_SYSTEM);
        assert_eq!(web_key_to_sfml("MetaRight", "Meta"), Key::R_SYSTEM);
        // The F-keys (F1=85..F15=99 — the 29b9 F3/F4 menu binds included).
        assert_eq!(web_key_to_sfml("F1", "F1"), Key::F1);
        assert_eq!(web_key_to_sfml("F3", "F3"), Key::F3);
        assert_eq!(web_key_to_sfml("F4", "F4"), Key::F4);
        assert_eq!(web_key_to_sfml("F15", "F15"), Key::F15);
        // The punctuation-by-`key` arm (the winit `Character` twin — the
        // LAYOUT'S character, the C++'s keysym-based parity).
        assert_eq!(web_key_to_sfml("Comma", ","), Key::COMMA);
        assert_eq!(web_key_to_sfml("Period", "."), Key::PERIOD);
        assert_eq!(web_key_to_sfml("Semicolon", ";"), Key::SEMICOLON);
        assert_eq!(web_key_to_sfml("Quote", "'"), Key::APOSTROPHE);
        assert_eq!(web_key_to_sfml("Slash", "/"), Key::SLASH);
        assert_eq!(web_key_to_sfml("Backslash", "\\"), Key::BACKSLASH);
        assert_eq!(web_key_to_sfml("Backquote", "`"), Key::GRAVE);
        assert_eq!(web_key_to_sfml("Equal", "="), Key::EQUAL);
        assert_eq!(web_key_to_sfml("Minus", "-"), Key::HYPHEN);
        assert_eq!(web_key_to_sfml("BracketLeft", "["), Key::L_BRACKET);
        assert_eq!(web_key_to_sfml("BracketRight", "]"), Key::R_BRACKET);
        // The unmappable: the SFML enum has no lock / dead / media keys,
        // and a garbage `code` maps to nothing.
        assert_eq!(web_key_to_sfml("CapsLock", "CapsLock"), Key::UNKNOWN);
        assert_eq!(web_key_to_sfml("NumLock", "NumLock"), Key::UNKNOWN);
        assert_eq!(web_key_to_sfml("AudioPlay", "AudioPlay"), Key::UNKNOWN);
        assert_eq!(web_key_to_sfml("", ""), Key::UNKNOWN);
        // A `code` that is NOT one of the families (the `KeyA` prefix
        // with a two-letter tail is not a letter — no false positive).
        assert_eq!(web_key_to_sfml("KeyAZ", "a"), Key::UNKNOWN);
        assert_eq!(web_key_to_sfml("Digit12", "1"), Key::UNKNOWN);
        assert_eq!(web_key_to_sfml("F", "f"), Key::UNKNOWN); // bare "F"
    }

    /// issue 32 §2.2 — the `keydown`'s `preventDefault` policy: the keys
    /// the game consumes are swallowed (the page must not ALSO scroll on
    /// Space / the arrows, zoom on `=`, or move focus on Tab); the
    /// browser-level shortcuts are NOT (the ctrl / meta combos, the
    /// browser-reserved F5 / F11 / F12, the IME composition, and the
    /// unmappable keys).
    #[test]
    fn web_keydown_prevent_default_policy() {
        // Consumed: the scrollers + the menu / game keys.
        assert!(web_keydown_prevent_default(
            "Space", " ", false, false, false
        ));
        assert!(web_keydown_prevent_default(
            "ArrowLeft",
            "ArrowLeft",
            false,
            false,
            false
        ));
        assert!(web_keydown_prevent_default(
            "ArrowDown",
            "ArrowDown",
            false,
            false,
            false
        ));
        assert!(web_keydown_prevent_default(
            "Tab", "Tab", false, false, false
        ));
        assert!(web_keydown_prevent_default(
            "Enter", "Enter", false, false, false
        ));
        assert!(web_keydown_prevent_default(
            "KeyA", "a", false, false, false
        ));
        assert!(web_keydown_prevent_default(
            "Backspace",
            "Backspace",
            false,
            false,
            false
        ));
        assert!(web_keydown_prevent_default("F3", "F3", false, false, false)); // the 29b9 bind
        assert!(web_keydown_prevent_default("F4", "F4", false, false, false));
        // The browser-level shortcuts stay the browser's:
        assert!(!web_keydown_prevent_default(
            "KeyR", "r", true, false, false
        )); // Ctrl+R reload
        assert!(!web_keydown_prevent_default(
            "KeyC", "c", false, true, false
        )); // Cmd+C copy
        assert!(!web_keydown_prevent_default(
            "F5", "F5", false, false, false
        )); // reload
        assert!(!web_keydown_prevent_default(
            "F11", "F11", false, false, false
        )); // fullscreen
        assert!(!web_keydown_prevent_default(
            "F12", "F12", false, false, false
        )); // devtools
            // The IME composition runs to its commit:
        assert!(!web_keydown_prevent_default(
            "KeyA", "Process", false, false, true
        ));
        // The unmappable keys are the page's business:
        assert!(!web_keydown_prevent_default(
            "CapsLock", "CapsLock", false, false, false
        ));
        assert!(!web_keydown_prevent_default("", "", false, false, false));
    }

    /// issue 32 §2.1 — the DOM `button` → the pinned SFML `Mouse::Button`
    /// integers (oracle-vectors.md's `enum Button`: Left=0..XButton2=4).
    #[test]
    fn web_mouse_button_to_sfml_pins_the_button_integers() {
        assert_eq!(web_mouse_button_to_sfml(0), Some(Button::LEFT));
        assert_eq!(web_mouse_button_to_sfml(1), Some(Button::MIDDLE));
        assert_eq!(web_mouse_button_to_sfml(2), Some(Button::RIGHT));
        assert_eq!(web_mouse_button_to_sfml(3), Some(Button::X_BUTTON1));
        assert_eq!(web_mouse_button_to_sfml(4), Some(Button::X_BUTTON2));
        // The DOM's 5..= buttons have no SFML twin (the winit fold's
        // `_ => None` twin).
        assert_eq!(web_mouse_button_to_sfml(5), None);
        assert_eq!(web_mouse_button_to_sfml(9), None);
        assert_eq!(web_mouse_button_to_sfml(-1), None);
    }

    /// issue 32 §2.1 — the mouse pointer feed: the `pointerdown`'s
    /// `MouseMoved` + `MouseButtonPressed` (the position lands in the
    /// `mouse_pixel` space the windowed path's `CursorMoved` uses), the
    /// hover `pointermove`'s `MouseMoved`, and the release-once guard
    /// (a `pointerup` with no seen `pointerdown` produces NO release —
    /// the off-canvas press's stuck-release guard; a double `pointerup`
    /// produces one release).
    #[test]
    fn web_pointer_feed_mouse() {
        let mut f = WebPointerFeed::new();
        let p = Vec2::new(10.0, 20.0);
        // A left-button down: the move + the press (the winit path's
        // `CursorMoved` → `MouseInput` order).
        match f.pointer_down("mouse", 1, 0, p).as_slice() {
            [Event::MouseMoved { position }, Event::MouseButtonPressed { button }]
                if *position == p && *button == Button::LEFT => {}
            other => panic!("mouse down: {other:?}"),
        }
        // A second button on the same pointer (the right, held while the
        // left is down — the DOM `button` carries each press).
        match f.pointer_down("mouse", 1, 2, p).as_slice() {
            [Event::MouseMoved { position }, Event::MouseButtonPressed { button }]
                if *position == p && *button == Button::RIGHT => {}
            other => panic!("second-button down: {other:?}"),
        }
        // The hover move.
        match f.pointer_move("mouse", 1, Vec2::new(30.0, 40.0)).as_slice() {
            [Event::MouseMoved { position }] if *position == Vec2::new(30.0, 40.0) => {}
            other => panic!("hover move: {other:?}"),
        }
        // The right's release (the left is STILL down — the mask's bit).
        match f.pointer_up("mouse", 1, 2).as_slice() {
            [Event::MouseButtonReleased { button }] if *button == Button::RIGHT => {}
            other => panic!("right release: {other:?}"),
        }
        // The left's release + the double-release guard (one release per
        // press, ever).
        match f.pointer_up("mouse", 1, 0).as_slice() {
            [Event::MouseButtonReleased { button }] if *button == Button::LEFT => {}
            other => panic!("left release: {other:?}"),
        }
        assert!(
            f.pointer_up("mouse", 1, 0).is_empty(),
            "the double release is a no-op (release-once)"
        );
        // An off-canvas press (no `down` seen — the capture never fired)
        // must not produce a release.
        assert!(
            f.pointer_up("mouse", 42, 0).is_empty(),
            "no `down` seen: no release (the off-canvas stuck-release guard)"
        );
        // An unmappable button (`9`) is tracked by no one.
        assert!(f
            .pointer_down("mouse", 7, 9, p)
            .into_iter()
            .all(|e| !matches!(e, Event::MouseButtonPressed { .. })));
        assert!(f.pointer_up("mouse", 7, 9).is_empty());
    }

    /// issue 32 §2.1 — the touch pointer feed: the `pointerId` →
    /// 0..`FINGER_COUNT` finger-slot allocation (the `InputState`'s
    /// 16-slot space — a 17th concurrent touch is ignored), the
    /// move-while-down, the up / cancel's slot free, and the
    /// no-down-no-event guards.
    #[test]
    fn web_pointer_feed_touch() {
        let mut f = WebPointerFeed::new();
        // Two touches: the slots 0 + 1 (the lowest free, ascending).
        match f
            .pointer_down("touch", 5, 0, Vec2::new(1.0, 2.0))
            .as_slice()
        {
            [Event::TouchBegan { finger, position }]
                if *finger == 0 && *position == Vec2::new(1.0, 2.0) => {}
            other => panic!("touch 0 down: {other:?}"),
        }
        match f
            .pointer_down("touch", 9, 0, Vec2::new(3.0, 4.0))
            .as_slice()
        {
            [Event::TouchBegan { finger, position }]
                if *finger == 1 && *position == Vec2::new(3.0, 4.0) => {}
            other => panic!("touch 1 down: {other:?}"),
        }
        // The move tracks its own slot.
        match f.pointer_move("touch", 9, Vec2::new(6.0, 7.0)).as_slice() {
            [Event::TouchMoved { finger, position }]
                if *finger == 1 && *position == Vec2::new(6.0, 7.0) => {}
            other => panic!("touch move: {other:?}"),
        }
        // A move with NO down seen (a pen hover — the `pointerdown`
        // never fired) is not a touch.
        assert!(
            f.pointer_move("touch", 77, Vec2::new(0.0, 0.0)).is_empty(),
            "no down, no touch move"
        );
        // The up / cancel end the finger (the slot frees).
        match f.pointer_up("touch", 5, 0).as_slice() {
            [Event::TouchEnded { finger }] if *finger == 0 => {}
            other => panic!("touch up: {other:?}"),
        }
        match f.pointer_cancel("touch", 9).as_slice() {
            [Event::TouchEnded { finger }] if *finger == 1 => {}
            other => panic!("touch cancel: {other:?}"),
        }
        // The freed slots: the next pointers take 0 + 1 again.
        match f
            .pointer_down("touch", 5, 0, Vec2::new(0.0, 0.0))
            .as_slice()
        {
            [Event::TouchBegan { finger, .. }] if *finger == 0 => {}
            other => panic!("slot re-use: {other:?}"),
        }
        // The `pointercancel` of an unbound pointer is a no-op (the
        // release-half bookkeeping: nothing went down here).
        assert!(f.pointer_cancel("touch", 99).is_empty());
        f.pointer_cancel("touch", 5);
        // All 16 slots live: the 17th touch is ignored (the
        // `FINGER_COUNT` cap — beyond any hardware).
        for i in 0..16 {
            match f
                .pointer_down("pen", 100 + i, 0, Vec2::new(0.0, 0.0))
                .as_slice()
            {
                [Event::TouchBegan { finger, .. }] if *finger == i => {}
                other => panic!("slot {i} down: {other:?}"),
            }
        }
        assert!(
            f.pointer_down("touch", 999, 0, Vec2::new(0.0, 0.0))
                .is_empty(),
            "the 17th concurrent touch has no slot (ignored)"
        );
        // The `pen` dispatches like a touch (the SFML touch events carry
        // pens too — the non-mouse arm).
        for i in 0..16 {
            f.pointer_cancel("pen", 100 + i);
        }
        match f
            .pointer_down("pen", 1234, 0, Vec2::new(9.0, 9.0))
            .as_slice()
        {
            [Event::TouchBegan { finger, .. }] if *finger == 0 => {}
            other => panic!("pen down: {other:?}"),
        }
    }

    /// issue 32 §2.4 — the release-on-blur behavior: the web `blur` /
    /// `visibilitychange`-hidden listeners push `Event::LostFocus`, and
    /// the fold's `LostFocus` arm is the C++'s `reset()` on focus loss —
    /// every held input (keys / buttons / fingers) is released, the
    /// shared input-state sink republished cleared. A key held when the
    /// tab loses focus must not stay stuck.
    #[test]
    fn lost_focus_releases_every_held_input() {
        use std::sync::{Arc, RwLock};

        let mut w = Window::headless(800, 600);
        let sink: Arc<RwLock<InputState>> = Arc::new(RwLock::new(InputState::new()));
        w.set_input_state_sink(sink.clone());

        // Hold a key + a button + a finger.
        Window::apply_event(
            &mut w,
            Event::KeyPressed {
                code: Key::A,
                repeated: false,
            },
        );
        Window::apply_event(
            &mut w,
            Event::MouseButtonPressed {
                button: Button::LEFT,
            },
        );
        Window::apply_event(
            &mut w,
            Event::TouchBegan {
                finger: 2,
                position: Vec2::new(100.0, 100.0),
            },
        );
        assert!(w.get_input_state().is_key_pressed(Key::A));
        assert!(w.get_input_state().is_button_pressed(Button::LEFT));
        assert!(w.get_input_state().is_finger(2));
        assert!(sink.read().expect("poisoned").is_key_pressed(Key::A));

        // The blur / the tab's hidden (the listeners' shared event).
        Window::apply_event(&mut w, Event::LostFocus);
        assert!(!w.get_input_state().is_key_pressed(Key::A));
        assert!(!w.get_input_state().is_button_pressed(Button::LEFT));
        assert!(!w.get_input_state().is_finger(2));
        assert_eq!(
            *sink.read().expect("poisoned"),
            InputState::new(),
            "the shared sink republishes the cleared state"
        );
        assert!(!w.has_focus());

        // The re-focus does NOT re-press (the C++ `GainedFocus` only
        // restores the flag — the inputs stay up until re-pressed).
        Window::apply_event(&mut w, Event::GainedFocus);
        assert!(w.has_focus());
        assert!(!w.get_input_state().is_key_pressed(Key::A));
    }

    /// issue 32 §2.3 — the web resize fold: the `Resized` event carries
    /// the REAL canvas size (the CSS box × the DPR, the listener's
    /// `web_canvas_real_size`) and the fold's `set_size` half moves the
    /// window's size bookkeeping (the menu layout + the `mapPixelToCoords`
    /// half-width space) with it — on host (no GL, no canvas) the fold
    /// is the bookkeeping only (the emscripten arm's `GlParts::resize`
    /// is the target-gated half, verified by the web build).
    #[test]
    fn resized_fold_moves_the_size_bookkeeping() {
        let mut w = Window::headless(800, 600);
        assert_eq!((w.get_width(), w.get_height()), (800, 600));
        // A pointer in the current size space + the view-mapped read.
        Window::apply_event(
            &mut w,
            Event::MouseMoved {
                position: Vec2::new(100.0, 50.0),
            },
        );
        assert_eq!(w.mouse_pixel(), Vec2::new(100.0, 50.0));
        Window::apply_event(
            &mut w,
            Event::Resized {
                width: 1600,
                height: 900,
            },
        );
        assert_eq!((w.get_width(), w.get_height()), (1600, 900));
        // The default view tracks the new size (the 1:1 pixel view over
        // the REAL window — the `mapPixelToCoords` target moved with it).
        let v = w.get_default_view();
        assert_eq!(
            (v.get_center(), v.get_size()),
            (Vec2::new(800.0, 450.0), Vec2::new(1600.0, 900.0))
        );
    }
}
