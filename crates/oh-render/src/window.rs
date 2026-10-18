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

use std::time::Duration;
#[cfg(not(target_os = "emscripten"))]
use std::time::Instant;

#[cfg(target_os = "emscripten")]
use wasm_bindgen::closure::Closure;
#[cfg(target_os = "emscripten")]
use wasm_bindgen::JsCast;
#[cfg(not(target_os = "emscripten"))]
use winit::application::ApplicationHandler;
#[cfg(not(target_os = "emscripten"))]
use winit::event::WindowEvent;
// 053 — the `set_inner_size`'s request (the android winit window's
// surface is the screen's — the setter is desktop-only, its docs).
#[cfg(not(any(target_os = "emscripten", target_os = "android")))]
use winit::dpi::PhysicalSize;
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

// ---------------------------------------------------------------------------
// issue 046 (F32) — the web's IME (the hidden-but-focusable `#ime` input)
// ---------------------------------------------------------------------------

/// issue 046 (F32) — the IME text ROUTING machine: the hidden input's DOM
/// events (`compositionstart` / `compositionend` / `input`) → the `char`s
/// that become the game's `Event::TextEntered`s (the existing entered-text
/// sink — the issue-025d chain — fed, never a second pipeline).
///
/// The four cases the port's `TextEntered` path must see (the task's
/// "the four cases that decide this"):
///
/// * **printable text** — the `input` event's `data` (the phone's soft
///   keyboard AND the desktop typing into the focused element);
/// * **Backspace** — NOT a keypress: the `input` event with
///   `inputType: deleteContentBackward` (and the `deleteContent*`
///   siblings) routed as ONE unicode 8 — the port's `TextEntered(8)`
///   `pop_back` arm (the C++'s second `TextEntered` handler, the dialog
///   erase, MenuGame.cpp:509-516);
/// * **Enter** — produces NO `input` event at all; it is NOT this
///   machine's (the game's `KeyPressed(ENTER)` — the C++'s `okAction`
///   bind, MenuGame.cpp:960 — CREATES the profile, the 025d/025f device
///   flow's "Enter created the profile");
/// * **composition** — `compositionstart` / `update` / `end` (a phone
///   keyboard, an autocomplete, any CJK method): the INTERMEDIATE `input`
///   events (the `insertCompositionText` inputType, fired DURING the
///   composition) must not feed — the `compositionend`'s `data` is the
///   commit and is fed EXACTLY ONCE (the double-commit guard below covers
///   the browser that ALSO fires a final `input` carrying the same data
///   after the `compositionend` — feeding both would double every
///   character; a CANCELLED composition — the empty-`data` `end` — feeds
///   nothing).
///
/// Pure + host-testable (no `web-sys`): the emscripten listeners (the
/// `attach_web_ime_listeners` below) are the thin wrapper that hands it
/// the DOM event's fields and pushes its chars as `TextEntered`s into the
/// window's DOM→SFML event queue.
pub struct WebImeFeed {
    /// A composition is in flight (`compositionstart` seen, the
    /// `compositionend` not yet): the `input` events are the intermediate
    /// ones — suppressed (the `compositionend` is the commit's source).
    composing: bool,
    /// The last `compositionend`'s `data` (the single-shot double-commit
    /// guard: a subsequent `input` carrying the SAME text is the same
    /// commit the `end` already fed — a different event, the same text,
    /// fed twice = doubled characters).
    last_committed: Option<String>,
}

impl Default for WebImeFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl WebImeFeed {
    #[must_use]
    pub fn new() -> Self {
        Self {
            composing: false,
            last_committed: None,
        }
    }

    /// `compositionstart` — the composition's first `input` event is the
    /// intermediate one (suppressed from here to the `compositionend`).
    pub fn composition_start(&mut self) {
        self.composing = true;
    }

    /// `compositionend` — the COMMIT: feed the `data`'s chars (a CJK
    /// method / an autocomplete / a phone keyboard that composes); the
    /// empty `data` (a CANCELLED composition — the Esc) feeds nothing.
    #[must_use]
    pub fn composition_end(&mut self, data: &str) -> Vec<char> {
        self.composing = false;
        if data.is_empty() {
            self.last_committed = None;
            return Vec::new();
        }
        self.last_committed = Some(data.to_string());
        data.chars().collect()
    }

    /// `input` — the SOLE text path while the element is focused (the
    /// keydown listener's `web_keydown_ime_policy` gate): the printable
    /// `data` (the insertions) + the Backspace (the `deleteContent*`
    /// inputTypes — ONE unicode 8 per event, the port's `pop_back` arm's
    /// granularity: one deletion event, one pop).
    #[must_use]
    pub fn input(&mut self, input_type: &str, data: &str) -> Vec<char> {
        // The composition's territory (the flag: a composition in flight;
        // the inputType: the `insertCompositionText` final-commit event a
        // browser fires AFTER the `compositionend` cleared the flag —
        // that commit's data is the `end`'s, fed above, exactly once).
        if self.composing || input_type == "insertCompositionText" {
            return Vec::new();
        }
        // Backspace (the `deleteContentBackward` the task names + the
        // `deleteContent*` siblings — a selected-range delete is ONE event
        // too: one pop, the port's `pop_back` arm's granularity — under
        // feeding is the safe side of the mismatch).
        if input_type.starts_with("delete") {
            self.last_committed = None;
            return vec!['\u{8}'];
        }
        // An insertion (the `insertText` / `insertReplacementText` /
        // `insertFromPaste` / `insertFromDrop` — and the `data`-less
        // browser variants): the `data` is the text.
        if data.is_empty() {
            return Vec::new();
        }
        // The double-commit guard (the docs): the same text the
        // `compositionend` JUST fed is the same commit — not a new one.
        if Some(data) == self.last_committed.as_deref() {
            self.last_committed = None;
            return Vec::new();
        }
        self.last_committed = None;
        data.chars().collect()
    }
}

/// issue 046 (F32) — the `keydown`'s modifier / state fields (the
/// `web_keydown_ime_policy`'s input bundle — the clippy 7-argument
/// bound's: the `code` / `key` + these five + the IME gate):
/// the `KeyboardEvent`'s `ctrlKey` / `altKey` / `metaKey` / `repeat` /
/// `isComposing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebKeydownState {
    /// `ctrlKey` (the browser-level shortcuts — reload, new tab, copy /
    /// paste, zoom: they must NOT be swallowed).
    pub ctrl: bool,
    /// `altKey` (the non-printable control-char `key` values' half).
    pub alt: bool,
    /// `metaKey` (the browser-level shortcuts' other modifier).
    pub meta: bool,
    /// `repeat` (the held-key repeat — the C++ `TextEntered` fires on
    /// the press half only; the held-key repeat would double-type).
    pub repeat: bool,
    /// `isComposing` (the IME composition keys — the `key` values
    /// `"Process"` / `"Composing"`; a composition must run to its
    /// commit, the issue-32 `preventDefault` policy's gate).
    pub is_composing: bool,
}

impl WebKeydownState {
    #[must_use]
    pub fn new(ctrl: bool, alt: bool, meta: bool, repeat: bool, is_composing: bool) -> Self {
        Self {
            ctrl,
            alt,
            meta,
            repeat,
            is_composing,
        }
    }
}

/// issue 046 (F32) — the `keydown`'s text policy (pure + host-testable —
/// the emscripten keydown listener applies it): `(prevent_default, text)`.
///
/// `text` = the char this keydown should push as the alongside
/// `TextEntered` (the issue-32 keydown→`TextEntered` half — the C++
/// `runEvents` polls SFML's `TextEntered` ALONGSIDE the `KeyPressed`;
/// the winit fold's twin's paired-event suppression, the
/// `RunHandler::window_event`'s `KeyboardInput` docs) — or `None`:
///
/// * the key produces no text (the functional keys — the `key` "Enter" /
///   "Escape" / "…" are WORDS, not chars — the modifier combos, the
///   repeats: the issue-32 guards, byte-identical);
/// * **the IME input is focused** (`ime_active` = `document.activeElement`
///   is the `#ime` element): the DOUBLE-TYPING gate. With the hidden input
///   focused the window STILL receives the `keydown` (it bubbles) — so the
///   same character would arrive twice: once through the keydown's
///   `TextEntered` and once through the browser's own insertion into the
///   focused input (the `input` event the feed rides on). The keydown
///   must push NO text (printable AND the Backspace's unicode-8 restore —
///   the browser's deletion fires the `deleteContentBackward` `input`
///   event instead) while the IME is focused: exactly ONE path per
///   keypress. The game's keys (the Enter that CREATES the profile — the
///   C++'s `okAction` bind's `KeyPressed(ENTER)`, the Escape, the
///   navigation) still flow as `KeyPressed` / `KeyReleased` — the
///   listener's mapping halves are UNCHANGED (this policy covers the
///   `TextEntered` + `preventDefault` halves only).
///
/// `prevent_default` = the issue-32 `web_keydown_prevent_default` policy —
/// EXCEPT `false` while the IME input is focused: the browser's own text
/// editing (the char insertion / the backspace the `input` events' feed
/// is) must RUN; a swallowed keydown inserts nothing and fires no
/// `input` event (no text at all — the 041 class of a headless-looking
/// path that is actually dead). The C++'s desktop behaviour is
/// UNCHANGED: `#ime` exists only on the emscripten window, and when it
/// is not focused (every non-text-entry state — the `soft_input.hide()`
/// blur on the transition edge) `ime_active` is `false` and both halves
/// behave exactly as issue 32.
#[must_use]
pub fn web_keydown_ime_policy(
    code: &str,
    key: &str,
    state: &WebKeydownState,
    ime_active: bool,
) -> (bool, Option<char>) {
    let prevent = if ime_active {
        false
    } else {
        web_keydown_prevent_default(code, key, state.ctrl, state.meta, state.is_composing)
    };
    // The text half (the docs): the IME gate AND the issue-32 guards
    // (the modifier combos / the repeat) close it — the `None` block;
    // the single-char / Backspace arms below are the byte-identical
    // issue-32 halves (reached only when none of the gates holds).
    let text = if ime_active || state.ctrl || state.alt || state.meta || state.repeat {
        None
    } else if key.chars().count() == 1 {
        key.chars().next()
    } else if code == "Backspace" {
        Some('\u{8}')
    } else {
        None
    };
    (prevent, text)
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
// issue 049 (K2) — the C++ SFML frame limiter's arithmetic (pure +
// host-testable)
// ---------------------------------------------------------------------------

/// issue 049 (K2) — the C++ SFML `Window::display()`'s frame-limiter
/// ARITHMETIC, extracted pure so the host gate pins it (the device pacing
/// itself is the operator's on-device confirmation — the device cannot be
/// reproduced here).
///
/// The pinned SFML (`dacceddbd`, the `GIT_TAG` in `cpp-original/
/// CMakeLists.txt`'s CPM block) paces inside the display —
/// `src/SFML/Window/Window.cpp:207-219`:
///
/// ```cpp
/// void Window::display()
/// {
///     // Display the backbuffer on screen
///     if (setActive())
///         m_context->display();
///
///     // Limit the framerate if needed
///     if (m_frameTimeLimit != Time::Zero)
///     {
///         sleep(m_frameTimeLimit - m_clock.getElapsedTime());
///         m_clock.restart();
///     }
/// }
/// ```
///
/// i.e. sleep `1/limit − elapsed` AFTER the swap, the frame clock
/// restarted each paced display (`m_clock`, `Window.hpp:284`, restarted
/// at `initialize()`, `Window.cpp:230`). SFML's `sleep` (the fork's
/// `src/SFML/System/Sleep.cpp:41-48`) is a NO-OP for a negative duration
/// (`if (duration >= Time::Zero) sleepImpl(duration)`), so an iteration
/// that already ran at least the frame time (≥ 5 ms at the shipped
/// `max_fps=200`) sleeps 0 — `saturating_sub` is that clamp. A `0` / non-
/// finite limit has no `m_frameTimeLimit` (SFML's `setFramerateLimit`
/// stores `Time::Zero` for `limit == 0`, `Window.cpp:175-181`).
#[must_use]
pub fn frame_limiter_sleep(limit_fps: f32, elapsed: Duration) -> Duration {
    // The C++'s `m_frameTimeLimit != Time::Zero` arm: `setFramerateLimit`
    // takes an `unsigned int` (`Window.cpp:175`), so `0` — and anything
    // non-positive / `NaN` — stores `Time::Zero` (no limit, no sleep).
    if limit_fps.is_nan() || limit_fps <= 0.0 {
        return Duration::ZERO;
    }
    Duration::from_secs_f32(1.0 / limit_fps).saturating_sub(elapsed)
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
    /// the value here for `get_pixel_mult` (29w note: the C++'s
    /// `setPixelMult` marks `mustRecreate` — GameWindow.hpp:197-200 — but
    /// the port's `set_pixel_mult` is the BOOT setter only (main.cpp:305's
    /// call, the `create_window` / `create_android_window`'s), so it stays
    /// the plain store: the C++'s boot-time mark is covered by its
    /// initial `mustRecreate{true}` (the first-iteration creation), which
    /// the port's pre-loop `create` + the flag's `false` start replace —
    /// marking from the boot setter would rebuild the context on the
    /// first frame for no change).
    pixel_mult: f32,
    fullscreen: bool,
    /// 053 — the `MenuGame::onEvent(Resized)`'s `changeResolutionTo`
    /// (MenuGame.cpp:261-263) edge: a folded `Resized` that is NOT the
    /// port's own `set_inner_size` / `set_fullscreen` request's delivery
    /// (the triage, the `Event::Resized` fold's comment). The
    /// `WindowedClient` frame body (oh-app `tick_fields`, desktop + menu
    /// state) consumes it through [`Self::take_resized_edge`]: the C++'
    /// handler's `Config::setCurrentResolution(event.size)` half — the
    /// config follows a user/WM resize in the menu (the C++'s guard,
    /// MenuGame.cpp:1041-1044, is the consumer's value check). `None`
    /// until the first user/WM resize folds (the game state's drain,
    /// the C++'s no-Resized-handler-in-HexagonGame half, keeps it
    /// consumed-and-discarded out of the menu).
    resized_edge: Option<(u32, u32)>,
    /// 053 — the `set_fullscreen` style change is pending its own
    /// `Resized` delivery (winit's fullscreen enter/exit moves the
    /// window to the monitor size / back to the pre-fullscreen size —
    /// the C++'s recreation instead lands directly at the target size,
    /// GameWindow.hpp:88-91, so no such intermediate size exists there):
    /// the `Event::Resized` fold's triage suppresses the config-follow
    /// for that delivery (the C++'s `changeResolutionTo` guard never
    /// fires on it — a spurious config-follow would write the monitor /
    /// restore size into the resolution config). Cleared by the next
    /// folded `Resized` (a boot-time `fullscreen = true` config's two
    /// deliveries — the creation size + the monitor size — are the
    /// fullscreen-gate's cover: the second one's `window.fullscreen`
    /// state is already the C++'s `getFullscreen()` value, the fold's
    /// triage comment). Desktop-only (the android/wasm `set_fullscreen`
    /// arms are the inert stores — no winit window to transition).
    fullscreen_transition: bool,
    /// C++ `GameWindow::mustRecreate` (GameWindow.hpp:39) — 29w: the
    /// "the setting changed" half of the C++'s trigger/re-apply split.
    /// The window-owned setters that the C++ marks (`setFullscreen` /
    /// `setSize` / `setAntialiasingLevel` / `setVsync` / `setPixelMult`,
    /// GameWindow.hpp:176-200) set it; the frame loop's
    /// `if(mustRecreate) recreateWindow()` (GameWindow.hpp:123) consumes
    /// it through [`Self::maybe_recreate_window`]. The port marks it from
    /// [`Self::set_antialiasing_level`] ONLY (its docs: the only window-
    /// owned setting whose re-apply the port cannot do without a context
    /// rebuild — `set_fullscreen` / `set_vsync` apply their observable
    /// half live, 047 / 25j; `set_size` / `set_pixel_mult` are the
    /// port's boot / event-fold bookkeeping, not the C++'s runtime mark
    /// paths). Starts `false` — the C++'s initial `true` covers its
    /// first-iteration window CREATION (GameWindow.hpp:39: the window
    /// does not exist until the first `run()` iteration's
    /// `recreateWindow`); the port's creation is `Window::create`'s,
    /// pre-loop, so the flag starts consumed.
    must_recreate: bool,
    /// C++ `GameWindow::onRecreation` (GameWindow.hpp:104) — the
    /// recreation's consumer delegate: `recreate_window` fires it AFTER
    /// the rebuild + the `inputState.reset` + the flag clear (the C++'s
    /// order, GameWindow.hpp:96-100). The port's one surviving consumer
    /// (the egui console's stale GL state — the `WindowedClient::run`
    /// hook) is the C++'s three consumers' (main.cpp:326's icon /
    /// MenuGame.cpp:730's menu re-layout / HexagonGame.cpp:339's key
    /// icons) only GL-state half: the icon + the menu re-layout are the
    /// surviving winit window's / the per-frame layout's state, the key
    /// icons' re-upload is the `GlPasses` cache's lazy job (the hook's
    /// docs, oh-app `windowed`). `None` = headless / pre-install / the
    /// android+wasm no-op consumptions.
    on_recreation: Option<Box<dyn FnMut()>>,
    /// C++ `GameWindow::antialiasingLevel` (GameWindow.hpp:38, default 3 —
    /// the `new_inner`'s 3) — the MSAA sample count the C++'s
    /// `recreateWindow` passes into `sf::ContextSettings` (GameWindow.hpp:
    /// 91, `main.cpp:307` stores the value). The port applies it at GL
    /// context creation (the desktop/wasm `create` parameter, the android
    /// first-`resumed` materialization's `create_android_parts`) AND — 29w
    /// — at the RUNTIME recreation: [`Self::set_antialiasing_level`] is
    /// the C++ setter's store + `mustRecreate` half (GameWindow.hpp:
    /// 187-190, under the `Config::setAntialiasingLevel`'s change guard,
    /// Config.cpp:703-709); the frame loop's
    /// [`Self::maybe_recreate_window`] (the C++'s
    /// `if(mustRecreate) recreateWindow()`, GameWindow.hpp:123) rebuilds
    /// the desktop context on the new level (`GlParts::recreate` — the
    /// C++'s `recreateWindow`, GameWindow.hpp:85-101), so a settings-menu
    /// level change reaches the context WITHOUT a restart (050's recorded
    /// gap — the level used to reach the context from config at boot
    /// only). Android / wasm: the store half only — the consumption is the
    /// documented no-op (the `recreate_window`'s per-platform reasons):
    /// the level stays for the next boot / activity resume.
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
    // issue 046 (F32) — the hidden-but-focusable IME input element
    // (`#ime`) (the `web_ime_input_get_or_create` docs): the web
    // `SoftInput` controller's (`oh-app` `WebIme` — the 025f
    // `ImeController`'s web twin) focus / blur target on the menu's
    // `is_entering_text` transition edge (the `windowed::
    // MenuSlotAdapter::refresh_input` docs — once per transition, never
    // per frame). `None` = the headless fallback / no DOM (the
    // controller's no-op arm; the desktop / Android windows never have
    // one — their `SoftInput` arms are the no-op / the android-activity).
    #[cfg(target_os = "emscripten")]
    ime_input: Option<web_sys::HtmlInputElement>,
    // issue 046 (F33) — the fullscreen button's (`#fs`) click sink slot
    // (the `set_fullscreen_click_sink` docs): the `oh-app`'s callback
    // (the config's `fullscreen` toggle + the browser Fullscreen API) —
    // the `#fs`'s `click` listener (the `Window::create`'s emscripten
    // half) calls it when set. `None` = the headless fallback / no `#fs`
    // element (no listener attached; the setter no-ops).
    #[cfg(target_os = "emscripten")]
    fullscreen_click_slot:
        Option<std::sync::Arc<std::sync::RwLock<Option<std::sync::Arc<dyn Fn() + Send + Sync>>>>>,
    // Android (issue 25b SG2): the `ensure_android_gl` once-only flag —
    // a GL creation failure (no GLES context) degrades to the headless
    // frames permanently (the C++'s no-GL `window == nullptr` equivalent),
    // so the re-`resumed`s after activity pauses don't retry (nor log).
    // Desktop: always `false` (the desktop bootstrap is pre-`run`).
    #[cfg(target_os = "android")]
    ensure_android_gl_failed: bool,
    // issue 049 (K2): the C++ SFML `Window::display()`'s frame-limiter
    // clock (the fork's `Window.hpp:284` `m_clock` — `Clock::restart()` at
    // `initialize()`, `Window.cpp:230`, and after every paced display,
    // `Window.cpp:217`). Android only: on Android the winit `WaitUntil`
    // deadline never blocks (the K2 short-circuit — the
    // `RunHandler::about_to_wait` note), so the pacing must live in the
    // frame (the C++'s own site); the desktop's `WaitUntil` blocks the
    // loop itself and the web's rAF is the display pacer. `None` = not yet
    // started (the C++ clock starts at window creation — the first paced
    // display is milliseconds after it, so its sleep is the full frame
    // time there too).
    #[cfg(target_os = "android")]
    framerate_clock: Option<Instant>,
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
    /// `antialiasing_level` defaults to `Config::default()`'s level (the
    /// `new_inner`'s comment — zero-divergence first tick, 29w).
    #[must_use]
    pub fn headless(width: u32, height: u32) -> Self {
        Self::new_inner(width, height, "open-hexagon")
    }

    /// Create a real window + GL context. `antialiasing_level` is the
    /// config's `antialiasing_level` (the C++'s `main.cpp:307` value → the
    /// C++ `recreateWindow`'s `ContextSettings::antialiasing` sample count,
    /// GameWindow.hpp:91) — applied to the context being created (050; 0 =
    /// no MSAA request). Degrades to a headless window (`gl() == None`)
    /// when no display / GPU is available (the level is still recorded).
    #[cfg(not(target_os = "emscripten"))]
    #[must_use]
    pub fn create(width: u32, height: u32, title: &str, antialiasing_level: u32) -> Self {
        match GlBackend::try_create(width, height, title, antialiasing_level) {
            Some(backend) => {
                let mut base = Self::new_inner(width, height, title);
                base.antialiasing_level = antialiasing_level;
                Self {
                    event_loop: Some(backend.event_loop),
                    gl: Some(backend.parts),
                    ..base
                }
            }
            None => {
                let mut base = Self::new_inner(width, height, title);
                base.antialiasing_level = antialiasing_level;
                base
            }
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
    pub fn create(width: u32, height: u32, title: &str, antialiasing_level: u32) -> Self {
        let mut base = Self::new_inner(width, height, title);
        // The 050 level (the C++ main.cpp:307 value): recorded up front so
        // the canvas-missing headless fallback carries it too (the store
        // half — the GL half applies it below, when the context is made).
        base.antialiasing_level = antialiasing_level;
        // The canvas (the W1 selector — the page MUST provide it).
        let canvas: web_sys::HtmlCanvasElement = match web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("c"))
            .map(|el| el.unchecked_into())
        {
            Some(canvas) => canvas,
            None => return base,
        };
        // issue 046 (F32) — the hidden-but-focusable IME input (`#ime`)
        // (the `web_ime_input_get_or_create` docs): the phone's soft
        // keyboard's focus target (a mobile browser raises it ONLY for a
        // focused editable element) + its `input` / `compositionend`
        // feed (the `attach_web_ime_listeners` below). `None` = the no-DOM
        // edge (the web `SoftInput` controller's no-op arm — the
        // `oh-app` `WebIme`'s `None` arm).
        let ime: Option<web_sys::HtmlInputElement> = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| web_ime_input_get_or_create(&d));
        // issue 046 (F33) — the fullscreen button (`#fs`) (the
        // `web_fs_button_get_or_create` docs): the config's `fullscreen`
        // value's web applier (the `oh-app`'s click sink — the browser
        // Fullscreen API). `None` = the no-DOM edge (the setter no-ops).
        let fs_button: Option<web_sys::Element> = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| web_fs_button_get_or_create(&d));
        let fullscreen_click_slot: Option<
            std::sync::Arc<std::sync::RwLock<Option<std::sync::Arc<dyn Fn() + Send + Sync>>>>,
        > = fs_button
            .as_ref()
            .map(|_| std::sync::Arc::new(std::sync::RwLock::new(None)));
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
        let dom_listeners = {
            let mut listeners = attach_web_listeners(&canvas, &queue, ime.as_ref());
            // issue 046 (F32) — the IME input's feed listeners (the
            // `compositionstart` / `compositionend` / `input` →
            // `TextEntered` halves — the `attach_web_ime_listeners` docs).
            if let Some(ime_el) = ime.as_ref() {
                listeners.extend(attach_web_ime_listeners(ime_el, &queue));
            }
            // issue 046 (F33) — the `#fs`'s click (the slot's callback —
            // the `set_fullscreen_click_sink`'s `oh-app` half: the
            // config's `fullscreen` toggle + the browser Fullscreen API).
            if let (Some(fs_btn), Some(slot)) = (fs_button.as_ref(), fullscreen_click_slot.as_ref())
            {
                let s = slot.clone();
                add_dom_listener(&fs_btn, "click", &mut listeners, move |_e| {
                    if let Ok(guard) = s.read() {
                        if let Some(cb) = guard.as_ref() {
                            cb();
                        }
                    }
                });
            }
            listeners
        };
        // The WebGL2 context (`gl.rs` — the emscripten C API + the glow
        // loader; it sets the canvas's backing store to the size it is
        // given — the REAL one); a failure degrades to the headless
        // frames (the loop still runs on the canvas). The 050 level maps
        // to the context's `antialias` boolean there (the browser picks
        // the sample count — `gl.rs` states + measures it).
        let gl = GlBackend::try_create(real_w, real_h, title, antialiasing_level)
            .map(|backend| backend.parts);
        Self {
            gl,
            event_queue: Some(queue),
            dom_listeners,
            // issue 046 (F32) — the IME input element (the `Window`
            // field's docs): the web `SoftInput` controller's focus / blur
            // target (the `web_ime_input` accessor below).
            ime_input: ime,
            // issue 046 (F33) — the `#fs`'s click sink slot (the field's
            // docs).
            fullscreen_click_slot,
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
    /// desktop never calls this (the desktop bootstrap is pre-`run`). The
    /// 050 level: the window's stored `antialiasing_level` (the C++
    /// main.cpp:307 value, the `create_android_window`'s
    /// `set_antialiasing_level`) goes into the materialized context — the
    /// C++'s own shape (the level is read at window creation, the
    /// `GameWindow::recreateWindow`'s `ContextSettings`, GameWindow.hpp:
    /// 91; this materialization IS the port's creation).
    #[cfg(target_os = "android")]
    fn ensure_android_gl(&mut self, el: &ActiveEventLoop) {
        if self.gl.is_some() || self.ensure_android_gl_failed {
            return;
        }
        match crate::gl::create_android_parts(el, self.antialiasing_level) {
            Some(parts) => {
                // C++ `recreateWindow` (GameWindow.hpp:89-95): the render
                // window is created and IMMEDIATELY
                // `renderWindow.setVerticalSyncEnabled(vsync)` is applied —
                // the port's GL materialization here is that creation, so
                // the stored config values (the `create_android_window`
                // `window.set_vsync(config.get_vsync())` — the C++ main.cpp:
                // 308 — and the `set_antialiasing_level` — the C++
                // main.cpp:307, passed into `create_android_parts` above as
                // the context's MSAA sample count, 050) go onto the fresh
                // EGL surface now. The explicit vsync request is
                // load-bearing: the EGL driver's default swap
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
            resized_edge: None,
            fullscreen_transition: false,
            must_recreate: false,
            on_recreation: None,
            // 29w — `Config::default()`'s level (oh-config's `Config::
            // default`'s `antialiasing_level` default — 4), NOT the C++'
            // `antialiasingLevel{3}` member default (GameWindow.hpp:38): that
            // C++ value is DEAD state — the boot setters (main.cpp:304-310,
            // `setAntialiasingLevel(mConfig.get().getAntialiasingLevel())`
            // at :306-307) always run before the first `recreateWindow()`
            // (the C++'s initial `mustRecreate{true}`, GameWindow.hpp:39,
            // consumed at :123-124 — the context is always created with the
            // CONFIG's value), so the 3 never materializes. This inner is
            // reached with the literal only by `headless` (the test
            // harness's stand-in for "the window created from the config" —
            // the production `create` / `create_window` pass the config's
            // level explicitly, recorded even on the no-GPU degrade, and
            // `android`/`wasm`'s constructors take it as a parameter): a
            // divergent default (3 vs the config's 4) would mark a spurious
            // recreation request on the FIRST tick of every headless client
            // (the `set_antialiasing_level`'s mark-on-change + the
            // `must_recreate` consumption), whose `inputState.reset()` half
            // (GameWindow.hpp:96 — the port's `reset_input_state`) would
            // wipe the harness's pre-tick input. The C++'s own
            // first-iteration recreation is the window's CREATION (empty
            // input state — the reset wipes nothing), so the port's
            // zero-divergence first tick is the faithful shape.
            antialiasing_level: 4,
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
            #[cfg(target_os = "emscripten")]
            ime_input: None,
            #[cfg(target_os = "emscripten")]
            fullscreen_click_slot: None,
            #[cfg(target_os = "android")]
            ensure_android_gl_failed: false,
            #[cfg(target_os = "android")]
            framerate_clock: None,
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

    /// `setSize(w, h)` — the size BOOKKEEPING (the `width_px` /
    /// `height_px` the `Resized` fold + the boot setters move: the C++
    /// `GameWindow::setSize`'s logical members, GameWindow.hpp:181-185 —
    /// the port's boot / event-fold bookkeeping, the `must_recreate`
    /// field's docs). The C++'s LIVE `RenderWindow::setSize` (the runtime
    /// size change) is [`Self::set_inner_size`].
    pub fn set_size(&mut self, width: u32, height: u32) {
        self.width_px = width;
        self.height_px = height;
    }

    /// C++ `MenuGame::changeResolutionTo`'s `window.getRenderWindow()
    /// .setSize(sf::Vector2u{mWidth, mHeight})` (MenuGame.cpp:1047) +
    /// `Config::setFullscreen`'s `mWindow.getRenderWindow().setSize(res)`
    /// (Config.cpp:644) — 053: the port's LIVE window resize (the 025g
    /// creation-frozen-surface deviation's closure). The C++'s call is
    /// the LIVE SFML `RenderWindow::setSize` — never a `mustRecreate`
    /// mark (the C++'s `changeResolutionTo`'s body, MenuGame.cpp:1039-
    /// 1053, sets none; its `GameWindow::setSize` mark, GameWindow.hpp:
    /// 181-185, is a DIFFERENT setter the C++'s live path never calls):
    /// the port requests the winit window's inner size (the same live
    /// call) and moves the size bookkeeping with it (the SFML
    /// `Window::setSize`'s immediate `m_size` update — the C++'s
    /// `changeResolutionTo`'s `refreshCamera()` reads `getWindowWidth()`'
    /// new value in the SAME call, MenuGame.cpp:1049 → :3131-3136).
    ///
    /// The GL surface + the viewport + the `GlParts` bookkeeping follow
    /// on the `Resized` event the request delivers (the `Event::Resized`
    /// fold's desktop `gl.resize` arm — the ONE resize path, the issue
    /// 32 web arm's same shape; the `GlParts::resize`'s docs). Until
    /// that delivery the one-frame draw runs the new view math on the
    /// old surface (the C++'s own X11 transient — its window resize is
    /// async too). The C++'s `changeResolutionTo` tail (`refreshCamera`,
    /// then `adjustLevelsOffset` / `adjustMenuOffset(true)` /
    /// `resetNamesScrolls`, MenuGame.cpp:1049-1052) is the port's
    /// per-frame `refresh_if_stale` (oh-app `menu_draw` — its stale-
    /// detection tuple carries the window size: the folded size fires
    /// it, no separate call needed).
    ///
    /// A no-op at the current size (the `WindowedClient` sync's per-frame
    /// re-request guard). Desktop only: the android activity window's
    /// surface is the screen's (the platform owns it — the `Resized`
    /// fold's bookkeeping covers its size changes) and the web canvas is
    /// the page's CSS box (the browser owns it — the DOM `resize`
    /// listener's `Resized` fold resizes the canvas 1:1, issue 32): on
    /// both the "resolution" setting has no window to drive (the C++
    /// has no android/web build to match either).
    pub fn set_inner_size(&mut self, width: u32, height: u32) {
        if width == self.width_px && height == self.height_px {
            return;
        }
        self.width_px = width;
        self.height_px = height;
        #[cfg(not(any(target_os = "emscripten", target_os = "android")))]
        if let Some(gl) = self.gl.as_ref() {
            // winit 0.30's `request_inner_size` (its `&self` form of the
            // resize request): `None` = the request went to the display
            // system (the actual size arrives with the `Resized` fold's
            // `gl.resize`); the `Some(current)` rejection (a
            // non-resizable window) = the request was ignored — the
            // bookkeeping above is then the requested size (the C++'s
            // SFML `setSize`'s own `m_size`-updates-while-the-OS-window
            // lags shape).
            let _ = gl
                .winit_window()
                .request_inner_size(PhysicalSize::new(width, height));
        }
    }

    /// 053 — the `MenuGame::onEvent(Resized)`'s `changeResolutionTo` edge
    /// (the `resized_edge` field's docs): the `WindowedClient` frame
    /// body (oh-app `tick_fields`, desktop + menu state) consumes it —
    /// the C++'s handler's `Config::setCurrentResolution(event.size)`
    /// half (the config follows a user/WM resize in the menu). `None`
    /// when no user/WM resize folded since the last take (the port's own
    /// `set_inner_size` / `set_fullscreen` deliveries are the fold's
    /// triage's `own` half — never edges, the `Event::Resized` arm's
    /// comment).
    #[must_use]
    pub fn take_resized_edge(&mut self) -> Option<(u32, u32)> {
        std::mem::take(&mut self.resized_edge)
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

    /// `setFullscreen` — and (desktop, issue 047) the C++ `GameWindow::
    /// setFullscreen`'s OBSERVABLE half. The C++ picks the window style at
    /// render-window creation (`GameWindow.hpp:90`, `fullscreen ?
    /// sf::Style::Fullscreen : sf::Style::Default`), driven by the flag
    /// `mainClient` sets from the config (`main.cpp:306`,
    /// `window->setFullscreen(hg::Config::getFullscreen())`); a
    /// `fullscreen = true` config therefore STARTS the C++ window
    /// fullscreen. The port creates the winit window in [`Self::create`]
    /// (windowed) and only recorded the flag here — so a `fullscreen =
    /// true` config was inert (the flag was read into the bookkeeping but
    /// the window never went fullscreen; the issue 047 finding). Apply the
    /// style to the live winit window when one exists: `Borderless(None)`
    /// = borderless fullscreen on the current monitor, the
    /// `sf::Style::Fullscreen` equivalent (not exclusive). A pre-`run`
    /// request is safe: winit stores a not-yet-visible window's request as
    /// `desired_fullscreen` and applies it on the first visibility notify
    /// (winit 0.30 X11 `set_fullscreen_inner` / `visibility_notify`).
    /// Callers: `oh-app`'s `create_window` (the creation-time application,
    /// the config value right after `Window::create`) + the `WindowedClient`
    /// per-frame config sync — the C++ `Config::setFullscreen(window, …)`'
    /// observable half, one frame after the menu's "go windowed" /
    /// "go fullscreen" item (or LAlt+Enter) wrote the config (29w: 047's
    /// live-apply shape, NOT a `mustRecreate` mark — the port's winit
    /// window changes style IN PLACE; the C++'s mark (GameWindow.hpp:
    /// 176-179) exists because SFML can only change the style by
    /// recreating the render window (GameWindow.hpp:90), and the port's
    /// `recreate_window` rebuilds the GL CONTEXT on the SAME window — it
    /// would not change the style at all).
    ///
    /// Android / wasm: INERT-BY-DESIGN — the platform window (the
    /// activity's immersive window / the page's `<canvas>`) has no
    /// window-style equivalent, and the C++ has no android/wasm build
    /// (SFML desktop-only), so there is no C++ path inert either. The web's
    /// fullscreen affordance is the page's browser-API button (lane 046),
    /// not the config's `fullscreen` value.
    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        if fullscreen == self.fullscreen {
            return;
        }
        self.fullscreen = fullscreen;
        #[cfg(not(any(target_os = "android", target_os = "emscripten")))]
        if let Some(gl) = self.gl.as_ref() {
            // 053 — the transition's own delivery's suppression (the
            // `fullscreen_transition` field's docs): winit's fullscreen
            // enter/exit moves the window to the monitor size / back to
            // the pre-fullscreen size — the C++'s `setFullscreen`'s
            // `renderWindow.setSize(res)` (Config.cpp:644) + recreation
            // (GameWindow.hpp:88-91) land directly at the target size
            // instead, so the C++'s `onEvent(Resized)` config-follow
            // never fires on an intermediate size; the fold's triage
            // suppresses this one's delivery.
            self.fullscreen_transition = true;
            gl.winit_window().set_fullscreen(if fullscreen {
                Some(winit::window::Fullscreen::Borderless(None))
            } else {
                None
            });
        }
    }

    #[must_use]
    pub fn get_fullscreen(&self) -> bool {
        self.fullscreen
    }

    /// `setAntialiasingLevel` (GameWindow.hpp:187-190: store +
    /// `mustRecreate` — the next `recreateWindow` re-reads the member into
    /// the `ContextSettings`, GameWindow.hpp:91) — 29w: the port mirrors
    /// the split. The STORE half: the level member. The MARK half:
    /// `must_recreate = true` — but ONLY on an ACTUAL change, mirroring
    /// the C++'s `Config::setAntialiasingLevel` guard (Config.cpp:703-709:
    /// `if(mValue != antialiasingLevel())` — the window setter is called
    /// only on a change; the guard also keeps the port's boot setter
    /// (`create_window`'s `set_antialiasing_level(config level)` — the C++
    /// main.cpp:307 call, after `Window::create` already stored the same
    /// value) from marking: the C++'s boot-time mark is covered by its
    /// initial `mustRecreate{true}` (GameWindow.hpp:39 — the first-
    /// iteration creation the port does pre-loop in `create`). The RE-APPLY
    /// half: the frame loop's [`Self::maybe_recreate_window`] (the C++'
    /// `if(mustRecreate) recreateWindow()`, GameWindow.hpp:123) rebuilds
    /// the desktop context on the new level (`GlParts::recreate` — the
    /// C++'s `recreateWindow`, GameWindow.hpp:85-101), so the level now
    /// reaches the context at RUNTIME, not just at boot (050's recorded
    /// gap). Android / wasm: the store + mark halves run (the C++'s shape
    /// — the setter is platform-blind); the consumption is the documented
    /// no-op (the `recreate_window`'s per-platform reasons — the level
    /// stays for the next boot / activity resume).
    pub fn set_antialiasing_level(&mut self, level: u32) {
        if level == self.antialiasing_level {
            return;
        }
        self.antialiasing_level = level;
        // The C++ `GameWindow::setAntialiasingLevel`'s mark (GameWindow.hpp:
        // 189-190) — the frame loop's `recreate_window` is the re-apply.
        self.must_recreate = true;
    }

    #[must_use]
    pub fn get_antialiasing_level(&self) -> u32 {
        self.antialiasing_level
    }

    // ---- recreation (29w — the C++ `mustRecreate` → `recreateWindow`) ----
    /// C++ `GameWindow::onRecreation` (GameWindow.hpp:104) — install the
    /// recreation's consumer hook: [`Self::recreate_window`] fires it
    /// AFTER the rebuild + the input-state reset + the flag clear (the
    /// C++'s order, GameWindow.hpp:96-100: `inputState.reset();
    /// mustRecreate = false; onRecreation();`). The port's one surviving
    /// consumer — the egui console's stale GL state, the `WindowedClient::
    /// run` hook (oh-app `windowed`) — is the C++'s three consumers'
    /// (main.cpp:326's icon re-apply / MenuGame.cpp:730's menu re-layout /
    /// HexagonGame.cpp:339's key-icons re-init) only GL-state half: the
    /// icon lives on the SURVIVING winit window (the port's recreation is
    /// a new context on the same window, not a new OS window), the menu
    /// re-layout runs per-frame from the live window size, the key
    /// icons' re-upload is the `GlPasses` texture cache's lazy job after
    /// the recreation's cache reset (the `recreate_window`'s docs).
    /// Re-installs (a second call replaces the hook — the C++ delegate
    /// supports many consumers, the port's single-hook surface covers its
    /// single consumer).
    pub fn set_on_recreation(&mut self, hook: Box<dyn FnMut()>) {
        self.on_recreation = Some(hook);
    }

    /// C++ `if(mustRecreate) recreateWindow();` (GameWindow.hpp:123) — the
    /// frame loop's consumption point (the C++'s loop head; the port's
    /// `WindowedClient` frame body calls it at the same position, oh-app
    /// `windowed`'s `tick_fields`). No-op when no window-owned setting has
    /// marked the flag since the last consumption. Returns whether the
    /// request was consumed (the frame body's `inputState.reset()` half —
    /// the C++'s `recreateWindow`'s `inputState.reset()`, GameWindow.hpp:
    /// 96 — runs at the port's event-model position, its docs).
    #[must_use]
    pub fn maybe_recreate_window(&mut self) -> bool {
        if self.must_recreate {
            self.recreate_window();
            true
        } else {
            false
        }
    }

    /// C++ `GameWindow::recreateWindow`'s `inputState.reset()` (GameWindow.hpp:
    /// 96) — the recreation's input-state half, as its own operation: the
    /// port's frame body applies it AFTER this frame's update burst
    /// consumed the folded events (the port's event model folds the window
    /// events BEFORE the frame body — winit's `about_to_wait`-driven frame,
    /// the winit-fold note, oh-app `windowed` — so the C++'s pre-`runEvents`
    /// position (GameWindow.hpp:123 → :42) is the post-update position here:
    /// the keys held from BEFORE the recreation are gone for the next frame,
    /// exactly the C++'s fresh-window input state; this frame's update
    /// already ran on them — the one-frame input delta, the
    /// `recreate_window`'s docs). The republish reaches the SHARED sink the
    /// engine's `Renderer` seam reads (the C++'s live
    /// `window->getInputState()` liveness — a stale sink would keep the
    /// pre-recreation key state alive in the engine's `is_key_pressed`
    /// reads).
    pub fn reset_input_state(&mut self) {
        self.input.reset();
        self.publish_input_state();
    }

    /// C++ `GameWindow::recreateWindow()` (GameWindow.hpp:85-101) — the
    /// port's mirror, per platform:
    ///
    /// * **desktop + live GL** — the real rebuild: `GlParts::recreate`
    ///   destroys the live context + surface and builds the new ones on
    ///   the SAME winit window + display (the C++'s
    ///   `renderWindow.close()` + `renderWindow.create(VideoMode, title,
    ///   style, ContextSettings{…, antialiasingLevel, …})`, GameWindow.hpp:
    ///   86-91 — the winit window's title + style are unchanged, the
    ///   surface is re-created at the current size) on the STORED
    ///   `antialiasing_level` (the C++'s `ContextSettings` re-read,
    ///   GameWindow.hpp:91 — 050's gap closing: a settings-menu level
    ///   change now reaches the context without a restart). The C++'
    ///   post-create re-applies, in its order: `setSize(width*pixelMult,
    ///   height*pixelMult)` (GameWindow.hpp:93 — the port's surface is
    ///   creation-sized, the 025g note: no size change),
    ///   `setVerticalSyncEnabled(vsync)` (GameWindow.hpp:94 — the
    ///   `set_swap_interval` on the fresh surface, load-bearing the same
    ///   way `ensure_android_gl`'s materialization note says: the new
    ///   surface starts at the driver default), the
    ///   `setFramerateLimit(fpsLimited ? maxFPS : 0)` (GameWindow.hpp:95
    ///   — no port re-apply: the limit is the loop's pacing state, the
    ///   `effective_framerate_limit` fields, which survive the
    ///   recreation unchanged).
    /// * **desktop headless** (no GL) / **android** / **emscripten** — the
    ///   consumption is the no-op + the hooks: android has no window-
    ///   style to recreate (the activity window is the platform's; the
    ///   C++ has no android build to match), the web's GL context is a
    ///   browser-API matter (050's `antialias` boolean-mapping deviation
    ///   stands; lane 046 owns the web's fullscreen shape) — on both the
    ///   stored level stays for the next boot / activity resume. Headless
    ///   has no GL at all. (In all three the `mustRecreate` flag is
    ///   still CONSUMED — the C++'s `recreateWindow` clears it
    ///   unconditionally, GameWindow.hpp:99.)
    ///
    /// After the platform half, the C++'s tail in its order:
    /// `mustRecreate = false` (GameWindow.hpp:99), `onRecreation()`
    /// (GameWindow.hpp:100 — the [`Self::set_on_recreation`] hook: the
    /// egui console's stale-GL-state clear). The C++'s `inputState.reset()`
    /// half (GameWindow.hpp:96 — the recreation clears every held key /
    /// button / finger: the new window starts with a clean input state)
    /// is the [`Self::reset_input_state`] operation the frame body applies
    /// at the port's event-model position (the C++'s reset lands before
    /// the frame's `runEvents`; the port's events are folded BEFORE the
    /// frame body — a top-of-frame reset would drop THIS frame's own key
    /// events, the winit-fold note, oh-app `windowed`: the post-update
    /// position keeps the C++'s observable — held keys gone for the next
    /// frame — without eating the current frame's input).
    fn recreate_window(&mut self) {
        // C++ `recreateWindow` (GameWindow.hpp:85-101) — the platform
        // half (the desktop GL rebuild; the android/wasm/headless
        // documented no-ops, above).
        #[cfg(not(any(target_os = "emscripten", target_os = "android")))]
        if let Some(gl) = self.gl.as_mut() {
            if gl.recreate(self.antialiasing_level) {
                // C++ GameWindow.hpp:94 — the recreation's
                // `setVerticalSyncEnabled(vsync)` re-apply (the fresh
                // surface's swap interval, the `ensure_android_gl`
                // materialization's load-bearing note, desktop twin).
                gl.set_swap_interval(self.vsync);
            }
        }
        // C++ GameWindow.hpp:99 — `mustRecreate = false`.
        // (The C++'s `inputState.reset()` half, GameWindow.hpp:96, is the
        // `reset_input_state` operation — the frame body's post-update
        // application, the docs above.)
        self.must_recreate = false;
        // C++ GameWindow.hpp:100 — `onRecreation()`.
        if let Some(hook) = self.on_recreation.as_mut() {
            hook();
        }
    }

    /// C++ `GameWindow::mustRecreate` (GameWindow.hpp:39) — the flag's
    /// read surface (29w): the C++'s `run` loop head (GameWindow.hpp:123)
    /// is the only reader; the port's tests + the Xvfb recreation probe
    /// read it through this (the "the recreation request is set and
    /// consumed" acceptance, the task's headless half). `false` at rest
    /// (a setter marks it, the frame loop's consumption clears it).
    #[must_use]
    pub fn must_recreate(&self) -> bool {
        self.must_recreate
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
        // OBSERVABLE change — the live EGL swap interval — directly, on
        // the live context, INSTEAD of waiting for the recreation (29w:
        // the C++'s mark exists because SFML re-applies the interval only
        // through `recreateWindow`; the port's swap-interval call is the
        // identical observable, so the setter stays unmarked — the
        // `set_fullscreen`'s live-apply note is the same shape). The value
        // IS re-applied by `recreate_window` on the fresh surface when a
        // recreation does happen (the C++'s order, GameWindow.hpp:94).
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

    /// issue 049 (K2): the C++ SFML `Window::display()`'s frame limiter, at
    /// the port's equivalent point (the frame — the update, the draw, and
    /// the swap — has just run: SFML paces AFTER the swap,
    /// `Window.cpp:207-219`). No-op when no limit is in force (the C++
    /// `m_frameTimeLimit == Time::Zero` arm: `fpsLimited` false or
    /// `maxFPS == 0` — `effective_framerate_limit` is the shared
    /// configuration the desktop path paces with, `limit_fps` / `max_fps`
    /// / `vsync` precedence unchanged: the vsync swap interval applies
    /// independently, as in the C++'s `recreateWindow` — `GameWindow.hpp:94`
    /// `setVerticalSyncEnabled` + `:95` `setFramerateLimit` together). The
    /// 025i per-frame `request_redraw` is UNTOUCHED (it is the ANR fix —
    /// and precisely what makes the winit deadline useless here; the
    /// sleep, not the loop, is the C++'s throttle).
    #[cfg(target_os = "android")]
    fn pace_display(&mut self) {
        let limit = self.effective_framerate_limit();
        if limit <= 0.0 {
            return;
        }
        let now = Instant::now();
        // The clock's start (the SFML `m_clock` — `None` = the C++ clock
        // started at window creation; the first sleep is the full frame
        // time, as the C++'s first post-creation display).
        let elapsed = now - self.framerate_clock.unwrap_or(now);
        std::thread::sleep(frame_limiter_sleep(limit, elapsed));
        // The SFML `m_clock.restart()` — after EVERY paced display
        // (the C++ restarts whether or not the sleep clamped to 0).
        self.framerate_clock = Some(Instant::now());
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

    /// issue 046 (F32) — the page's hidden IME input element (the `#ime`
    /// field's docs): the web `SoftInput` controller's focus / blur
    /// target (the `oh-app` `WebIme`'s `SoftInput::show` / `hide` ride
    /// the menu's `is_entering_text` transition edge). `None` = the
    /// headless fallback (no canvas / no DOM — the controller no-ops).
    #[cfg(target_os = "emscripten")]
    #[must_use]
    pub fn web_ime_input(&self) -> Option<web_sys::HtmlInputElement> {
        self.ime_input.clone()
    }

    /// issue 046 (F33) — install the fullscreen button's (`#fs`) click
    /// callback (the `fullscreen_click_slot` field's docs): the `oh-app`
    ///'s `WindowedClient::build`'s emscripten half — the config's
    /// `fullscreen` toggle (the SAME shared `Arc<RwLock<Config>>` the
    /// LAlt+Enter bind's `MenuConfig` seam writes through — so the
    /// button and the menu's own toggle cannot disagree) + the browser
    /// Fullscreen API's apply (the `documentElement`'s
    /// `requestFullscreen` / the `document`'s `exitFullscreen` — the
    /// `documentElement` so the button stays visible IN the fullscreen;
    /// the Promise's rejection routes to the page's `[oh-web-fs]`
    /// console line — the task's honest limit: a `requestFullscreen`
    /// needs a transient user activation; a CDP probe MAY grant it
    /// (an immediate click after a CDP key input) or not (the operator's
    /// real tap always does on Android Chrome)). No-op when the `#fs`
    /// was absent at creation (the headless fallback — no button, no
    /// click).
    #[cfg(target_os = "emscripten")]
    pub fn set_fullscreen_click_sink(&mut self, cb: std::sync::Arc<dyn Fn() + Send + Sync>) {
        if let Some(slot) = &self.fullscreen_click_slot {
            *slot.write().expect("fullscreen click slot poisoned") = Some(cb);
        }
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
                // issue 046 (F32) — the fast-press visibility (the
                // `BatchPresses`'s docs): the batch's press edges + the
                // state's save (the restoration's `saved` half — the
                // `InputState`'s `Copy`).
                let presses = BatchPresses::from_events(&events);
                let saved_input = r.window.input;
                for event in events {
                    Window::apply_event(&mut r.window, event);
                }
                // The OR-in (the docs): the frame's update sees the
                // press edges down (the C++'s poll-inside-the-press
                // visibility) — the shared sink re-published with them.
                if !presses.is_empty() {
                    presses.apply(&mut r.window.input);
                    r.window.publish_input_state();
                }
                // The per-frame handler (disjoint field borrows).
                {
                    let on_frame = &mut r.on_frame;
                    let window = &mut r.window;
                    on_frame(window);
                }
                // The restoration (the docs): the TRUE post-drain state
                // is the window's state after the frame (the next
                // frame's update sees the key up — the `Type::Once`
                // latch's re-arm, the C++'s `refresh`'s `!isDown`).
                if !presses.is_empty() {
                    r.window.input = saved_input;
                    r.window.publish_input_state();
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
            #[cfg(target_os = "android")]
            frame_log: (None, 0.0, 0),
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
                // ACTUALLY drawn into).
                #[cfg(target_os = "emscripten")]
                if let Some(gl) = window.gl.as_mut() {
                    gl.resize(width, height);
                }
                // Desktop (053): the `changeResolutionTo`'s
                // `renderWindow.setSize` (MenuGame.cpp:1047) + the
                // `setFullscreen`'s `renderWindow.setSize(res)`
                // (Config.cpp:644) delivery — the 025g /
                // creation-frozen-surface deviation is CLOSED: the GL
                // surface + the viewport + the `GlParts` size
                // bookkeeping follow the folded size (the web arm's same
                // shape, `GlParts::resize`'s docs). Android: still the
                // documented bookkeeping-only fold (the activity window's
                // surface is the screen's; the C++ has no android build).
                #[cfg(not(any(target_os = "emscripten", target_os = "android")))]
                if let Some(gl) = window.gl.as_mut() {
                    gl.resize(width, height);
                }
                window.set_size(width, height);
                // 053 — the `MenuGame::onEvent(Resized)` (MenuGame.cpp:
                // 261-263)'s delivery triage: the C++'s handler runs
                // `changeResolutionTo(event.size)` whose GUARD (MenuGame.
                // cpp:1041-1044) early-returns when the config is ALREADY
                // the event's size — the port's own `set_inner_size`
                // delivery (the config was written by its caller first,
                // the oh-app `tick_fields` sync's order) is that
                // early-return, consumed by the consumer's value check;
                // the `set_fullscreen` style transition's own delivery
                // (the monitor size / the pre-fullscreen restore — the
                // C++'s recreation never produces them) is the
                // `fullscreen_transition` suppression; a fullscreen-state
                // delivery is the winit borderless model's own move (the
                // C++'s non-exclusive fullscreen's window size IS its
                // resolution — the port's monitor-sized borderless is the
                // documented 047 shape). Only a user/WM resize in the
                // windowed state is a config-follow edge.
                let own = window.fullscreen_transition || window.fullscreen;
                window.fullscreen_transition = false;
                if !own {
                    window.resized_edge = Some((width, height));
                }
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

/// issue 046 (F32) — a drained event batch's press edges (pure +
/// host-testable — the emscripten `Window::run` rAF drain's fast-press
/// visibility, its docs):
/// the keys / buttons / fingers that went DOWN anywhere in the batch
/// (a `KeyPressed` — repeat or not, a `MouseButtonPressed`, a
/// `TouchBegan`), in batch order.
///
/// Why the batch needs them: the C++'s `poll()` loop is an UNCAPPED
/// spin (the `RunHandler`'s docs — the `GameWindow.hpp:117-150`'s
/// `while (isOpen) { poll; update; draw; }` iterates as fast as it can):
/// a press is therefore visible to the `runUpdate`'s input-state read
/// for as long as the key is down between polls, however brief. The
/// web's rAF drain collapses every event of the frame interval into
/// ONE drain + ONE `apply_event` fold + ONE update — a down+up pair
/// INSIDE the interval (a machine-speed pair — AND the phone soft
/// keyboard's own synthesized down+up for a quick tap) leaves the
/// post-drain state with the key UP: the `Type::Once`'s
/// state-snapshot binds (the Enter → `okAction`'s profile creation —
/// the 046 acceptance's "the profile was created") never see the
/// press. The drain's caller's fix: this batch's press edges are OR'd
/// into the window's input state for the frame's update ONLY (the
/// update sees the key down, exactly as a C++ poll inside the press's
/// window would), then the TRUE post-drain state is restored (no
/// persistent pollution: the next frame's update sees the key up —
/// the `Type::Once` latch's re-arm, the C++'s `refresh`'s `!isDown`
/// re-arm). The winit path needs no twin: its `ControlFlow::Poll`
/// `about_to_wait` fires per event wakeup (each OS-delivered event
/// batch gets its own frame) — the desktop's human press durations
/// (≥ 50ms) span the uncapped spin's polls anyway (the 025g
/// established loop model).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchPresses {
    /// The batch's `KeyPressed`'s codes (the `Key`'s; a repeat is the
    /// key already down — the OR-in a no-op).
    keys: Vec<crate::Key>,
    /// The batch's `MouseButtonPressed`'s buttons.
    btns: Vec<crate::Button>,
    /// The batch's `TouchBegan`'s finger ids (the 0..`FINGER_COUNT`;
    /// an out-of-range id is the `apply`'s `set_finger`'s no-op — the
    /// fold's own guard).
    fingers: Vec<u32>,
}

impl BatchPresses {
    /// The batch's press edges (the docs): a `LostFocus` in the batch
    /// CLEARS the earlier edges (the fold's `reset()` — the C++'s
    /// event order: a press before the blur's `reset()` is gone).
    #[must_use]
    pub fn from_events(events: &[Event]) -> Self {
        let mut out = Self::default();
        for e in events {
            match e {
                Event::LostFocus => out = Self::default(),
                Event::KeyPressed { code, .. } => out.keys.push(*code),
                Event::MouseButtonPressed { button } => out.btns.push(*button),
                Event::TouchBegan { finger, .. } => out.fingers.push(*finger),
                _ => {}
            }
        }
        out
    }

    /// OR the press edges into `state` (each edge's `pressed = true`) —
    /// idempotent (a duplicated edge within the batch is harmless).
    pub fn apply(&self, state: &mut InputState) {
        for k in &self.keys {
            state.set_key_pressed(*k, true);
        }
        for b in &self.btns {
            state.set_button_pressed(*b, true);
        }
        for f in &self.fingers {
            state.set_finger(*f, true);
        }
    }

    /// No press edges in the batch (the drain's save / restore halves
    /// skip — no state touched, no re-publish).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.btns.is_empty() && self.fingers.is_empty()
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
    /// issue 051 (the K2 device half) — the android loop-rate diagnostic
    /// that implements the K2 row's standing probe ("menu fps and in-game
    /// fps from the port's own log lines"): `(the last frame's
    /// `Instant`, the accumulated period in seconds, the frame count)
    /// since the last log line. `SurfaceFlinger --latency` cannot see it
    /// (the Adreno display composites at its own 60 Hz regardless of the
    /// app's submission rate), so the app's own loop period is the fps
    /// instrument (the 039 STEP 0 sink-class device diagnostic).
    #[cfg(target_os = "android")]
    frame_log: (Option<Instant>, f64, u64),
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
        // issue 049 (K2): the C++ SFML frame limiter, at the C++'s own
        // site — AFTER the frame (the swap) has run. ANDROID ONLY: the
        // `WaitUntil` deadline below is never honoured on Android —
        // winit 0.30.13's android `poll_events_with_timeout`
        // (`src/platform_impl/android/mod.rs:549-555`: `pending_redraw` —
        // set by 025i's per-frame `request_redraw` below, and `true`
        // before the next poll — short-circuits the wait to
        // `Duration::ZERO`) — so the deadline never blocks and the loop
        // free-ran (the K2 on-device numbers: menu ~500 fps, in-game
        // ~360 fps against the C++'s paced 200). The C++ paces with a
        // sleep in `Window::display()` after the swap (SFML pinned
        // `dacceddbd`, `Window.cpp:207-219`), so the port paces at the
        // equivalent point; the 025i redraw request stays (the ANR fix —
        // every iteration still draws + reads input, and now also spends
        // its sub-frame time the way the C++'s main thread does; the
        // per-iteration sleep is ≤ 1/min `max_fps` — 5 ms at the shipped
        // 200 — nowhere near Android's 5 s input-dispatch ANR window).
        // Desktop keeps its winit `WaitUntil` (it blocks there — no
        // redraw-pending short-circuit in the desktop backends); the web
        // is paced by rAF.
        #[cfg(target_os = "android")]
        {
            self.window.pace_display();
            self.log_frame_period();
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
// issue 051 (the K2 device half) — the android loop-rate diagnostic (the
// standing probe's "the port's own log lines"): `SurfaceFlinger --latency`
// cannot measure the app's loop rate (the display composites at its own
// 60 Hz whatever the app submits), so the loop period is logged from the
// app itself, every 500 frames (the 039 STEP 0 sink-class device
// diagnostic).
#[cfg(target_os = "android")]
impl<F> RunHandler<'_, F> {
    /// The once-per-frame loop-period accumulator (called after the frame
    /// — the swap — has run, the `pace_display` point): every 500 frames
    /// it logs the average loop period + the fps + the limit in force, so
    /// a device session reads the menu / in-game fps straight out of
    /// logcat (the K2 row's standing probe — `~200` under the shipped
    /// `limit_fps=true, max_fps=200`, `~500/360` free-run when the cap is
    /// not enforced).
    fn log_frame_period(&mut self) {
        let now = Instant::now();
        let (last, acc, count) = &mut self.frame_log;
        if let Some(prev) = *last {
            *acc += (now - prev).as_secs_f64();
            *count += 1;
            if *count >= 500 {
                let avg_s = *acc / *count as f64;
                eprintln!(
                    "[oh-render] loop period {:.2} ms over {} frames (~{} fps; limit in force: {:.0})",
                    avg_s * 1000.0,
                    *count,
                    (1.0 / avg_s).round() as u32,
                    self.window.effective_framerate_limit(),
                );
                *acc = 0.0;
                *count = 0;
            }
        }
        *last = Some(now);
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
    // issue 046 (F32) — the hidden-but-focusable IME input (`#ime`) when
    // the page has a DOM (the keydown listener's `web_keydown_ime_policy`
    // double-typing gate: `None` = the element is absent, the gate is
    // inactive — the issue-32 behaviour byte-identical).
    ime: Option<&web_sys::HtmlInputElement>,
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
    // for it, so it is restored from the physical `code`. issue 046
    // (F32): BOTH the `TextEntered` half AND the `preventDefault` are
    // gated on the IME input's focus (the `web_keydown_ime_policy` docs —
    // the DOUBLE-TYPING gate: while `#ime` is focused its `input` event
    // is the sole text path, and a swallowed keydown would insert
    // nothing into the focused input and fire no `input` event at all);
    // the `KeyPressed` / `KeyReleased` mapping halves (the Enter that
    // CREATES the profile — the C++'s `okAction` bind — the Escape, the
    // navigation) are UNCHANGED below.
    let q = queue.clone();
    let ime_el = ime.map(|el| el.clone());
    add_dom_listener(&window, "keydown", &mut listeners, move |e| {
        let Some(ke) = e.dyn_ref::<web_sys::KeyboardEvent>() else {
            return;
        };
        // issue 046 (F32) — the double-typing gate's DOM half: the
        // `#ime` element is the focused element (the `keydown` bubbles
        // from it to the window — without the gate the same character
        // would arrive twice: the keydown's `TextEntered` + the browser's
        // own insertion's `input` event the feed rides on).
        let ime_active = ime_el.as_ref().is_some_and(web_ime_is_active);
        let (prevent, text) = web_keydown_ime_policy(
            ke.code().as_str(),
            ke.key().as_str(),
            &WebKeydownState::new(
                ke.ctrl_key(),
                ke.alt_key(),
                ke.meta_key(),
                ke.repeat(),
                ke.is_composing(),
            ),
            ime_active,
        );
        if prevent {
            e.prevent_default(); // the page must not also scroll / zoom / move focus
        }
        if let Some(c) = text {
            q.borrow_mut().push(Event::TextEntered { character: c });
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

/// issue 046 (F32) — the hidden-but-focusable IME input element (`#ime`)
/// — the phone's soft keyboard's target (a mobile browser raises the
/// on-screen keyboard ONLY for a focused editable element; the canvas'
/// `keydown` listener never gets one — the task's diagnosis):
///
/// * **hidden but FOCUSABLE** — off-screen (`position: fixed; left:
///   -10000px`) + `opacity: 0`, NEVER `display: none` (browsers refuse to
///   focus those);
/// * the attribute contract (F32): `type="text"`, `inputmode="text"`,
///   `autocapitalize="off"`, `autocomplete="off"`, `spellcheck="false"`,
///   `enterkeyhint="done"` (the soft keyboard's commit key — the Enter
///   that CREATES the profile, the C++'s `okAction` bind, the 025d/025f
///   device flow) + a **16 px font** (a smaller font makes iOS zoom the
///   page on focus — the operator's target is Android Chrome, where the
///   floor is harmless);
/// * **get-or-create** (the committed shell, `crates/oh-web/site/
///   index.html`, declares `#ime` — the element created by Rust is the
///   fallback for a page that predates it, the `#c` canvas's "the page
///   MUST provide it" contract mirrored with a Rust-side create):
///   the attribute + style contract is re-applied either way (the Rust
///   side is the single source of truth — the page's copy is the
///   discoverability twin).
///
/// Returns `None` only when the page has no `document` / `body` at all
/// (the canvas is present here, so this is the no-DOM edge — the web
/// `SoftInput` controller's no-op arm).
#[cfg(target_os = "emscripten")]
fn web_ime_input_get_or_create(document: &web_sys::Document) -> Option<web_sys::HtmlInputElement> {
    let existing = document
        .get_element_by_id("ime")
        .map(|el| el.unchecked_into::<web_sys::HtmlInputElement>());
    let el = match existing {
        Some(el) => el,
        None => {
            let el = document.create_element("input").ok()?;
            let body = document.body()?;
            body.append_child(&el).ok()?;
            el.unchecked_into::<web_sys::HtmlInputElement>()
        }
    };
    // The attribute contract (the docs) — re-applied on the page-declared
    // element too (the committed shell carries the same values; a stale /
    // custom page is brought back to the contract). The typed setters
    // (the `type` / `inputmode` / `autocomplete` / `spellcheck` web-sys
    // exposes) + `set_attribute` for the two web-sys 0.3.106 has no typed
    // setter for (`autocapitalize` / `enterkeyhint` — the same DOM
    // attributes, the generic attribute route) + the `style` attribute
    // (web-sys exposes the `style` declaration read-only — the attribute
    // is the write route; the SAME string the committed shell's inline
    // style carries).
    el.set_type("text");
    el.set_input_mode("text");
    el.set_autocomplete("off");
    el.set_spellcheck(false);
    el.set_attribute("autocapitalize", "off").ok()?;
    el.set_attribute("enterkeyhint", "done").ok()?;
    // The style contract (the docs): off-screen (focusable — never
    // `display: none`), invisible (`opacity: 0`), the 16 px floor (the
    // iOS no-zoom; the Android target is unaffected — the floor is the
    // web's minimum-tappable-font convention, not an Android behaviour).
    el.set_attribute(
        "style",
        "position: fixed; left: -10000px; top: 0; opacity: 0; font-size: 16px;",
    )
    .ok()?;
    Some(el)
}

/// issue 046 (F33) — the fullscreen button's (`#fs`) get-or-create (the
/// `Window` field's twin's `#ime` half — the `web_ime_input_get_or_create`
/// docs): the config's `fullscreen` value's web applier's button (the
/// `oh-app`'s click sink's target — the `set_fullscreen_click_sink` docs).
///
/// The committed shell declares it (the page's `<button id="fs">` + its
/// stylesheet's contract: the bottom-right corner, the 44px touch
/// target, the translucent / subtle look, `touch-action: manipulation`,
/// `pointer-events: auto` on itself only, `display: none` by default —
/// the shell's `ohstatechange` gate shows it in the menu states and
/// HIDES it — not merely makes it transparent — in the game state,
/// where the gameplay touch overlay's RotateCW owns that corner: a
/// hidden element is the only variant that cannot swallow a touch). A
/// found element is left AS-IS (its styling is the shell stylesheet's —
/// the committed shell is the supported page; unlike the `#ime`'s
/// text contract, a stale `#fs` cannot corrupt the input, only look
/// different). The absent-element fallback CREATES it: the full inline
/// style contract (the committed shell's values) + the `⛶` glyph + the
/// `aria-label` — WITHOUT a `display` override (the fallback page lacks
/// the shell's gate JS — the button is visible and working, just
/// ungated: the committed shell is the supported page).
#[cfg(target_os = "emscripten")]
fn web_fs_button_get_or_create(document: &web_sys::Document) -> Option<web_sys::Element> {
    if let Some(existing) = document.get_element_by_id("fs") {
        return Some(existing);
    }
    let el = document.create_element("button").ok()?;
    el.set_attribute("id", "fs").ok()?;
    el.set_attribute("aria-label", "Toggle fullscreen").ok()?;
    // The style contract (the committed shell's `#fs` rule — the
    // fallback page's inline twin; the `display` is deliberately ABSENT
    // — the shell's gate JS owns it, the fallback's button works).
    el.set_attribute(
        "style",
        "position: fixed; right: 12px; bottom: 12px; width: 44px; height: 44px; padding: 0; border: 1px solid rgba(255, 255, 255, 0.25); border-radius: 10px; background: rgba(255, 255, 255, 0.10); color: #ddd; opacity: 0.55; font-size: 20px; line-height: 1; cursor: pointer; touch-action: manipulation; pointer-events: auto; z-index: 10;",
    )
    .ok()?;
    el.set_text_content(Some("⛶"));
    document.body()?.append_child(&el).ok()?;
    Some(el)
}

/// issue 046 (F32) — the keydown gate's DOM half: the `#ime` element is
/// the page's focused element (`document.activeElement`). The `is_same_node`
/// comparison (NOT `==`): the web-sys stable API's object identity — the
/// DOM's node identity, not a structural compare (the DOM's `isSameNode`
/// takes a NULLABLE node — web-sys's `Option<&Node>` signature).
#[cfg(target_os = "emscripten")]
fn web_ime_is_active(ime: &web_sys::HtmlInputElement) -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.active_element())
        .is_some_and(|active| web_sys::Node::is_same_node(&active, Some(ime)))
}

/// issue 046 (F32) — the IME input's DOM → `Event::TextEntered` listeners
/// (the `WebImeFeed` docs — the four cases): `compositionstart` (the
/// composing flag), `compositionend` (the commit's `data` — fed exactly
/// once), `input` (the printable `data` + the `deleteContentBackward`
/// Backspace — the `WebImeFeed`'s routing decides what reaches the
/// queue). Pushed into the SAME DOM→SFML event queue the keydown
/// listener feeds (the `Window::run` rAF drain → the `TextEntered` fold
/// → the windowed client's `entered_text_sink` → the menu's
/// `handle_text_entered` — the EXISTING entered-text sink fed, not a
/// second pipeline). Returns the closures — the caller MUST keep them
/// alive for the window's life (the `dom_listeners` field's contract).
#[cfg(target_os = "emscripten")]
fn attach_web_ime_listeners(
    ime: &web_sys::HtmlInputElement,
    queue: &std::sync::Arc<std::cell::RefCell<Vec<Event>>>,
) -> Vec<Closure<dyn FnMut(web_sys::Event)>> {
    // The routing machine (the `WebImeFeed` docs): the pure half the host
    // tests pin; the listeners are the thin wrapper (the `WebPointerFeed`
    // pattern).
    let feed = std::rc::Rc::new(std::cell::RefCell::new(WebImeFeed::new()));
    let mut listeners = Vec::new();

    // `compositionstart` — the composing flag (the DURING-composition
    // `input` events are the intermediate ones — suppressed; the
    // `compositionend`'s `data` is the commit).
    let f = feed.clone();
    add_dom_listener(ime, "compositionstart", &mut listeners, move |_| {
        f.borrow_mut().composition_start();
    });
    // `compositionend` — the COMMIT (a phone keyboard, an autocomplete,
    // any CJK method): feed the `data` (empty = a CANCELLED composition —
    // nothing fed). The `data` is an owned `String` (the web-sys 0.3.106
    // codegen's `Option<String>` — no borrow to manage).
    let f = feed.clone();
    let q = queue.clone();
    add_dom_listener(ime, "compositionend", &mut listeners, move |e| {
        let data = e
            .dyn_ref::<web_sys::CompositionEvent>()
            .and_then(|c| c.data())
            .unwrap_or_default();
        for c in f.borrow_mut().composition_end(&data) {
            q.borrow_mut().push(Event::TextEntered { character: c });
        }
    });
    // `input` — the SOLE text path while this element is focused (the
    // keydown listener's `web_keydown_ime_policy` gate): the printable
    // text (the `data`) + the Backspace (the `inputType`
    // `deleteContentBackward` — NOT a keypress — the port's
    // `TextEntered(8)`'s `pop_back` arm). Both fields are owned strings
    // (the web-sys 0.3.106 codegen: the `input_type()`'s `String`, the
    // `data()`'s `Option<String>` — no borrow out of the event).
    let f = feed.clone();
    let q = queue.clone();
    add_dom_listener(ime, "input", &mut listeners, move |e| {
        let (input_type, data) = match e.dyn_ref::<web_sys::InputEvent>() {
            Some(i) => {
                let t = i.input_type();
                let d = i.data().unwrap_or_default();
                (t, d)
            }
            None => (String::new(), String::new()),
        };
        for c in f.borrow_mut().input(&input_type, &data) {
            q.borrow_mut().push(Event::TextEntered { character: c });
        }
    });

    listeners
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 050 — the `antialiasing_level` store half (the C++
    /// `GameWindow::antialiasingLevel` member, GameWindow.hpp:38 default 3 —
    /// the `new_inner`'s 3 — and the `setAntialiasingLevel` store +
    /// `mustRecreate`, GameWindow.hpp:187-190; the live-context half is
    /// `backend.rs`'s `antialiasing_level_*` tests — the 050 acceptance is
    /// the measured `GL_SAMPLES`):
    #[test]
    fn antialiasing_level_stores_and_round_trips() {
        let mut w = Window::headless(640, 480);
        assert_eq!(
            w.get_antialiasing_level(),
            4,
            "the config-parity default (the `new_inner`'s comment, 29w)"
        );
        w.set_antialiasing_level(8);
        assert_eq!(w.get_antialiasing_level(), 8);
        w.set_antialiasing_level(0);
        assert_eq!(w.get_antialiasing_level(), 0);
    }

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

    // ------------------------------------------------------------------
    // issue 046 (F32) — the rAF drain's fast-press visibility
    // (`BatchPresses`). The pure half the emscripten `Window::run`
    // drain's save / OR-in / restore applies — the C++'s
    // poll-inside-the-press visibility at the rAF cadence (the
    // `BatchPresses`'s docs).
    // ------------------------------------------------------------------

    /// issue 046 (F32) — the batch's press edges (the docs): the mixed
    /// batch's downs (the `KeyPressed` — a RELEASED-IN-BATCH press still
    /// counts: the press happened IN the interval, the C++'s
    /// poll-inside-the-press visibility; the `MouseButtonPressed`; the
    /// `TouchBegan`), the non-press events ignored (the releases, the
    /// moves, the wheel, the resize, the focus — the state's UP half is
    /// the fold's job, not the batch's).
    #[test]
    fn batch_presses_collects_the_batch_press_edges() {
        let p = Vec2::new(1.0, 2.0);
        let events = vec![
            Event::KeyPressed {
                code: Key::A,
                repeated: false,
            },
            Event::KeyReleased { code: Key::A },
            Event::KeyPressed {
                code: Key::B,
                repeated: false,
            }, // still held (no release in the batch)
            Event::MouseButtonPressed {
                button: Button::LEFT,
            },
            Event::MouseButtonReleased {
                button: Button::LEFT,
            },
            Event::TouchBegan {
                finger: 3,
                position: p,
            },
            Event::TouchEnded { finger: 3 },
            Event::MouseMoved { position: p },
            Event::MouseWheelScrolled { delta: 1.0 },
            Event::Resized {
                width: 100,
                height: 100,
            },
        ];
        let presses = BatchPresses::from_events(&events);
        let mut state = InputState::new();
        presses.apply(&mut state);
        assert!(
            state.is_key_pressed(Key::A),
            "the released-in-batch press is the interval's press (the C++'s poll-inside-the-press)"
        );
        assert!(state.is_key_pressed(Key::B));
        assert!(state.is_button_pressed(Button::LEFT));
        assert!(state.is_finger(3));
        assert!(!state.is_key_pressed(Key::C));
        assert!(!state.is_button_pressed(Button::RIGHT));
        assert!(!state.is_finger(0));
    }

    /// issue 046 (F32) — the `LostFocus`'s batch half (the docs): the
    /// fold's `reset()` clears the state — the batch's earlier press
    /// edges clear WITH it (a press before the blur's `reset()` is
    /// gone, the C++'s event order); a press AFTER the blur survives.
    #[test]
    fn batch_presses_lost_focus_clears_earlier_presses() {
        let events = vec![
            Event::KeyPressed {
                code: Key::A,
                repeated: false,
            },
            Event::LostFocus,
            Event::KeyPressed {
                code: Key::B,
                repeated: false,
            },
        ];
        let mut state = InputState::new();
        // The fold's own halves (for the test's realism): the press's
        // down, the blur's `reset()` (the fold's `LostFocus` arm), the
        // second press's down.
        state.set_key_pressed(Key::A, true); // the `KeyPressed`'s down
        state.reset(); // the `LostFocus`'s `reset()`
        state.set_key_pressed(Key::B, true); // the second press's down
        BatchPresses::from_events(&events).apply(&mut state);
        assert!(
            !state.is_key_pressed(Key::A),
            "the pre-blur press is the `reset()`'s gone half"
        );
        assert!(state.is_key_pressed(Key::B));
    }

    /// issue 046 (F32) — the `repeated`'s keydown (the held-key's
    /// auto-repeat — a `KeyPressed` with `repeated: true`): the key is
    /// DOWN (the repeat's premise) — the edge is collected (the OR-in
    /// the no-op on an already-down state — the batch never lies about
    /// the key's level).
    #[test]
    fn batch_presses_repeated_keydown_is_a_press_edge() {
        let events = vec![Event::KeyPressed {
            code: Key::D,
            repeated: true,
        }];
        let mut state = InputState::new();
        BatchPresses::from_events(&events).apply(&mut state);
        assert!(state.is_key_pressed(Key::D));
    }

    /// issue 046 (F32) — a release-only batch (no downs): empty (the
    /// drain's save / restore halves skip — the state's UP half is the
    /// fold's, nothing to restore).
    #[test]
    fn batch_presses_release_only_batch_is_empty() {
        let events = vec![
            Event::KeyReleased { code: Key::A },
            Event::MouseButtonReleased {
                button: Button::LEFT,
            },
            Event::TouchEnded { finger: 0 },
        ];
        assert!(BatchPresses::from_events(&events).is_empty());
    }

    /// issue 046 (F32) — the save / OR-in / restore's full frame cycle
    /// (the drain's orchestration's semantics, pinned host-side — the
    /// emscripten `Closure`'s body itself is the target-gated half):
    /// the batch's [down+up A] + [held B] over the frame — the frame's
    /// update sees A + B down; the post-frame state is the TRUE
    /// post-drain state (A up, B down — the latch's re-arm's input).
    #[test]
    fn batch_presses_save_or_restore_frame_cycle() {
        let events = vec![
            Event::KeyPressed {
                code: Key::A,
                repeated: false,
            },
            Event::KeyReleased { code: Key::A },
            Event::KeyPressed {
                code: Key::B,
                repeated: false,
            },
        ];
        let presses = BatchPresses::from_events(&events);
        let mut state = InputState::new();
        // B held from before the interval (the previous frame's press).
        state.set_key_pressed(Key::B, true);
        // The fold (the drain's `apply_event` halves, over a bare state):
        // A down, A up, B down (B already down).
        state.set_key_pressed(Key::A, true);
        state.set_key_pressed(Key::A, false);
        state.set_key_pressed(Key::B, true);
        // The save (the drain's `saved_input`) + the OR-in.
        let saved = state;
        presses.apply(&mut state);
        // The frame's update's view (the `on_frame`'s read):
        assert!(
            state.is_key_pressed(Key::A),
            "the fast press is visible to the frame's update"
        );
        assert!(state.is_key_pressed(Key::B));
        // The restoration (the drain's post-`on_frame` half).
        state = saved;
        assert!(
            !state.is_key_pressed(Key::A),
            "the next frame sees the true (released) state"
        );
        assert!(state.is_key_pressed(Key::B));
    }

    // ------------------------------------------------------------------
    // issue 046 (F32) — the IME text routing (`WebImeFeed`) + the keydown
    // double-typing gate (`web_keydown_ime_policy`). The pure halves the
    // emscripten listeners apply — the four cases the task names, pinned
    // host-side (the DOM halves ride the web build + the CDP probe).
    // ------------------------------------------------------------------

    /// issue 046 (F32) — printable text via the `input` event (the
    /// `insertText` inputType — the phone's soft keyboard AND the desktop
    /// typing into the focused element): the `data`'s chars, in order, one
    /// `TextEntered` each (the menu drain's C++ `isAlphanumeric` / `
    /// isPunctuation` filter is the game's half, not the feed's — the
    /// feed delivers the browser's text as-is, exactly one path per
    /// event).
    #[test]
    fn web_ime_feed_inserts_printable_data_in_order() {
        let mut feed = WebImeFeed::new();
        assert_eq!(feed.input("insertText", "0"), vec!['0']);
        assert_eq!(feed.input("insertText", "46"), vec!['4', '6']);
        // The paste / replacement / drop insertions feed their `data` too
        // (the `insertFromPaste` / `insertReplacementText` / `insertFrom
        // Drop` inputTypes — the non-`delete` non-composition arm).
        assert_eq!(feed.input("insertFromPaste", "ab"), vec!['a', 'b']);
        assert_eq!(feed.input("insertReplacementText", "Z"), vec!['Z']);
    }

    /// issue 046 (F32) — the Backspace case: a deletion arrives as an
    /// `input` event with `inputType: deleteContentBackward` (NOT as a
    /// keypress) and is routed as EXACTLY ONE unicode 8 — the port's
    /// `TextEntered(8)`'s `pop_back` arm (the C++'s second `TextEntered`
    /// handler's erase, MenuGame.cpp:509-516). The `deleteContent`
    /// sibling (a selected-range delete) is ONE event too: one pop (the
    /// under-feeding side of the mismatch — the safe one).
    #[test]
    fn web_ime_feed_backspace_is_exactly_one_unicode_8() {
        let mut feed = WebImeFeed::new();
        assert_eq!(feed.input("deleteContentBackward", ""), vec!['\u{8}']);
        assert_eq!(feed.input("deleteContentBackward", ""), vec!['\u{8}']);
        assert_eq!(feed.input("deleteContent", ""), vec!['\u{8}']);
        // A backspace between insertions does not corrupt the next
        // insertion's feed (the guard state is reset by the deletion).
        assert_eq!(feed.input("insertText", "a"), vec!['a']);
        assert_eq!(feed.input("deleteContentBackward", ""), vec!['\u{8}']);
        assert_eq!(feed.input("insertText", "b"), vec!['b']);
    }

    /// issue 046 (F32) — the composition case (a phone keyboard, an
    /// autocomplete or any CJK method): the INTERMEDIATE `input` events
    /// (the `insertCompositionText` inputType, fired DURING the
    /// composition) feed NOTHING — the `compositionend`'s `data` is the
    /// commit, fed EXACTLY ONCE, and a browser that ALSO fires a final
    /// `input` carrying the same `data` AFTER the `compositionend` (the
    /// double-commit ordering) is suppressed by the guard (feeding both
    /// would double every character — the task's "do not let it corrupt
    /// or double").
    #[test]
    fn web_ime_feed_composition_commits_exactly_once() {
        let mut feed = WebImeFeed::new();
        feed.composition_start();
        // The intermediate updates (the `compositionupdate`'s data is NOT
        // fed — the listener attaches to `compositionstart` /
        // `compositionend` only; the `input` events that run DURING the
        // composition are the `insertCompositionText` ones):
        assert_eq!(
            feed.input("insertCompositionText", "ni"),
            Vec::<char>::new()
        );
        assert_eq!(feed.input("insertCompositionText", "n"), Vec::<char>::new());
        // The commit — the `compositionend`'s `data`:
        assert_eq!(feed.composition_end("nihao"), vec!['n', 'i', 'h', 'a', 'o']);
        // The double-commit guard: the SAME data a final `input` event
        // carries after the `end` (the browser that fires both) is the
        // same commit — suppressed:
        assert_eq!(
            feed.input("insertText", "nihao"),
            Vec::<char>::new(),
            "the double-commit must not feed the characters a second time"
        );
        // …while the SAME text typed AFTER the guard is consumed is a NEW
        // insertion (the guard is single-shot):
        assert_eq!(
            feed.input("insertText", "nihao"),
            vec!['n', 'i', 'h', 'a', 'o']
        );
    }

    /// issue 046 (F32) — the CANCELLED composition (the Esc): the
    /// empty-`data` `compositionend` feeds nothing and resets the state —
    /// the following plain insertion is unaffected.
    #[test]
    fn web_ime_feed_cancelled_composition_feeds_nothing() {
        let mut feed = WebImeFeed::new();
        feed.composition_start();
        assert_eq!(
            feed.input("insertCompositionText", "xyz"),
            Vec::<char>::new()
        );
        assert_eq!(
            feed.composition_end(""),
            Vec::<char>::new(),
            "a cancelled composition (the empty data) feeds nothing"
        );
        assert_eq!(feed.input("insertText", "a"), vec!['a']);
    }

    /// issue 046 (F32) — the `data`-less insertions (the browser variant
    /// that fires an `input` event with no `data`) feed nothing (no
    /// guessing: the value-diff fallback would risk doubling what the
    /// `data`-carrying events already delivered).
    #[test]
    fn web_ime_feed_empty_data_feeds_nothing() {
        let mut feed = WebImeFeed::new();
        assert_eq!(feed.input("insertText", ""), Vec::<char>::new());
        assert_eq!(feed.input("", ""), Vec::<char>::new());
    }

    /// issue 046 (F32) — the `policy` shorthand for the tests (the
    /// `WebKeydownState`'s five fields in the listener's order — the
    /// clippy 7-argument bound's bundle): `code`, `key`, `ctrl`, `alt`,
    /// `meta`, `repeat`, `is_composing`, `ime_active`.
    #[allow(clippy::too_many_arguments)] // the test shorthand's 8 fields (the production fn's bundle)
    fn policy(
        code: &str,
        key: &str,
        ctrl: bool,
        alt: bool,
        meta: bool,
        repeat: bool,
        is_composing: bool,
        ime_active: bool,
    ) -> (bool, Option<char>) {
        super::web_keydown_ime_policy(
            code,
            key,
            &super::WebKeydownState::new(ctrl, alt, meta, repeat, is_composing),
            ime_active,
        )
    }

    /// issue 046 (F32) — the DOUBLE-TYPING gate's desktop arm unchanged
    /// (`ime_active = false` — every non-text-entry state, where `#ime`
    /// is blurred): the printable char pushes its `TextEntered` AND
    /// `preventDefault`s (the issue-32 behaviour, byte-identical — the
    /// page must not also scroll / zoom; the character must reach the
    /// game exactly once through THIS path, the keydown's, the C++
    /// `runEvents`'s alongside-`TextEntered` the winit fold's twin
    /// restores, window.rs:1777-1790's paired-event suppression intact).
    #[test]
    fn web_keydown_policy_desktop_path_unchanged_when_ime_blurred() {
        // The printable char: text + prevent (the `KeyA` / "a" pair).
        let (prevent, text) = policy("KeyA", "a", false, false, false, false, false, false);
        assert!(
            prevent,
            "the desktop page-consumed key still preventDefaults"
        );
        assert_eq!(
            text,
            Some('a'),
            "the desktop text path still pushes the char"
        );
        // The Backspace's unicode-8 restore (the 29b3 half, unchanged).
        let (_, text) = policy(
            "Backspace",
            "Backspace",
            false,
            false,
            false,
            false,
            false,
            false,
        );
        assert_eq!(
            text,
            Some('\u{8}'),
            "the desktop Backspace still restores the pop"
        );
        // The repeat (the held-key double-type guard, unchanged).
        let (_, text) = policy("KeyA", "a", false, false, false, true, false, false);
        assert_eq!(text, None, "a repeated key still pushes no text");
        // The modifier combos (unchanged).
        let (_, text) = policy("KeyA", "a", true, false, false, false, false, false);
        assert_eq!(text, None);
        // The Enter (a WORD, not a char — no text; the game's `okAction`
        // bind's `KeyPressed(ENTER)` is the listener's mapping half,
        // outside this policy, unchanged).
        let (_, text) = policy("Enter", "Enter", false, false, false, false, false, false);
        assert_eq!(text, None);
        // The browser-reserved keys still do NOT preventDefault (the
        // F5 / F11 / F12 policy, unchanged).
        let (prevent, _) = policy("F5", "F5", false, false, false, false, false, false);
        assert!(!prevent);
    }

    /// issue 046 (F32) — the DOUBLE-TYPING gate's IME arm (`ime_active =
    /// true` — the menu's text-entry state, `#ime` focused): the keydown
    /// must push NO `TextEntered` (printable AND the Backspace's
    /// unicode-8 restore — the browser's own insertion / deletion fires
    /// the `input` events the feed rides on; both halves feeding would
    /// double-type every character) AND must NOT `preventDefault` (a
    /// swallowed keydown inserts nothing into the focused input and fires
    /// no `input` event — no text at all). One keypress, exactly one
    /// character (through the `input` path — this policy's job is to keep
    /// the keydown path OUT of it).
    #[test]
    fn web_keydown_policy_ime_active_suppresses_text_and_prevent() {
        // The printable char: no text, no prevent.
        let (prevent, text) = policy("KeyA", "a", false, false, false, false, false, true);
        assert!(
            !prevent,
            "a prevented keydown would insert nothing into the focused input"
        );
        assert_eq!(
            text, None,
            "the keydown's char would double-type the input event's"
        );
        // The Backspace (the soft keyboard's deletion key — the `input`
        // event's `deleteContentBackward` is the pop's sole source).
        let (prevent, text) = policy(
            "Backspace",
            "Backspace",
            false,
            false,
            false,
            false,
            false,
            true,
        );
        assert!(!prevent);
        assert_eq!(
            text, None,
            "the keydown's unicode-8 would double-pop the input event's"
        );
        // The Enter (the soft keyboard's commit key — the `okAction`
        // bind's `KeyPressed(ENTER)` the mapping half still pushes, OUTSIDE
        // this policy): no text (it never had any), no prevent.
        let (prevent, text) = policy("Enter", "Enter", false, false, false, false, false, true);
        assert!(!prevent);
        assert_eq!(text, None);
        // The composition keys (the `is_composing` — the `Process` /
        // `Composing` `key` values): no text, no prevent (the composition
        // runs to its commit — the `compositionend`'s feed).
        let (prevent, text) = policy("Dead", "Process", false, false, false, false, true, true);
        assert!(!prevent);
        assert_eq!(text, None);
    }
    // 29w — the C++ `mustRecreate` → `recreateWindow` split, headless
    // (the display half is the Xvfb recreation probe — the `GlParts::
    // recreate`'s live-context rebuild; these pin the trigger/consume
    // contract the frame loop drives): the setter's store + mark half
    // (GameWindow.hpp:187-190) and the loop-head's consumption (the C++'s
    // `if(mustRecreate) recreateWindow();`, GameWindow.hpp:123 — the
    // port's `maybe_recreate_window`).
    #[test]
    fn antialiasing_level_marks_must_recreate_only_on_an_actual_change() {
        let mut w = Window::headless(640, 480);
        // The `new_inner`'s config-parity default (the `Config::default()`
        // level — the `new_inner`'s comment: the C++'s `antialiasingLevel{3}`
        // member default, GameWindow.hpp:38, is dead state) + the flag's
        // consumed start (the C++'s initial `mustRecreate{true}` covers
        // its first-iteration creation; the port creates pre-loop).
        assert_eq!(w.get_antialiasing_level(), 4);
        assert!(!w.must_recreate());
        // Same value: the C++ `Config::setAntialiasingLevel`'s change
        // guard (Config.cpp:703-709) — the window setter runs only on a
        // change; the port's boot setter (after `create` stored the same
        // value) must not mark.
        w.set_antialiasing_level(4);
        assert!(!w.must_recreate());
        // A change: store + mark (GameWindow.hpp:189-190).
        w.set_antialiasing_level(8);
        assert_eq!(w.get_antialiasing_level(), 8);
        assert!(w.must_recreate());
    }

    #[test]
    fn recreate_consumes_the_request_and_fires_the_hook_once() {
        let mut w = Window::headless(640, 480);
        let fired = std::rc::Rc::new(std::cell::Cell::new(0));
        let fired_hook = fired.clone();
        w.set_on_recreation(Box::new(move || {
            fired_hook.set(fired_hook.get() + 1);
        }));
        // No request: the loop head's check is a no-op (the C++'s
        // `if(mustRecreate)` false arm, GameWindow.hpp:123).
        assert!(!w.maybe_recreate_window());
        assert_eq!(fired.get(), 0);
        assert!(!w.must_recreate());
        // The request (the C++ setter's mark — a CHANGE from the
        // config-parity default 4, GameWindow.hpp:189-190) + the held
        // input the recreation's `inputState.reset()` (GameWindow.hpp:96)
        // clears — the frame body's post-update application (the
        // `maybe_recreate_window`'s bool return's consumer).
        w.set_antialiasing_level(8);
        Window::apply_event(
            &mut w,
            Event::KeyPressed {
                code: Key::A,
                repeated: false,
            },
        );
        assert!(w.get_input_state().is_key_pressed(Key::A));
        let recreated = w.maybe_recreate_window();
        assert!(recreated);
        // Consumed + the hook fired (the C++'s `onRecreation()`,
        // GameWindow.hpp:100); the reset half is the frame body's call.
        assert!(!w.must_recreate());
        assert_eq!(fired.get(), 1);
        w.reset_input_state();
        assert!(!w.get_input_state().is_key_pressed(Key::A));
        // The level stays stored for the next boot (the android/wasm
        // consumption shape — headless here: no GL to rebuild).
        assert_eq!(w.get_antialiasing_level(), 8);
        // A second frame with no new change: no re-fire.
        assert!(!w.maybe_recreate_window());
        assert_eq!(fired.get(), 1);
    }

    #[test]
    fn recreate_resets_the_shared_input_sink_too() {
        // The engine's `Renderer` seam reads the SHARED sink (the C++'s
        // live `window->getInputState()` reads — the recreation's reset
        // must reach it, not just the window's own state).
        let mut w = Window::headless(640, 480);
        let sink: std::sync::Arc<std::sync::RwLock<InputState>> =
            std::sync::Arc::new(std::sync::RwLock::new(*w.get_input_state()));
        w.set_input_state_sink(sink.clone());
        Window::apply_event(
            &mut w,
            Event::KeyPressed {
                code: Key::A,
                repeated: false,
            },
        );
        assert!(sink.read().expect("poisoned").is_key_pressed(Key::A));
        w.set_antialiasing_level(8);
        assert!(w.maybe_recreate_window());
        w.reset_input_state();
        assert!(!sink.read().expect("poisoned").is_key_pressed(Key::A));
    }
}
