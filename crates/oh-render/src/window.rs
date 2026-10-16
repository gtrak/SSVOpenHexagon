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

use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
#[cfg(target_os = "android")]
use winit::event_loop::EventLoop;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key as WKey, KeyCode, KeyLocation, NamedKey, PhysicalKey};
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
            winit::event::TouchPhase::Ended => Event::TouchEnded {
                finger: touch.id as u32,
            },
            _ => return None,
        }),
        _ => None,
    }
}

/// The per-window-event hook of [`Window::run_with_event_hook`] (issue
/// 014b) — the egui console's `egui-winit` event feed, seeing every raw
/// winit `WindowEvent` on a real window (the C++ `runEvents`'s per-event
/// `gameEngine->handleEvent`, GameWindow.hpp:79, called AFTER the SFML
/// input-state fold).
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
    event_loop: Option<winit::event_loop::EventLoop<()>>,
    gl: Option<GlParts>,
    // Android (issue 25b SG2): the `ensure_android_gl` once-only flag —
    // a GL creation failure (no GLES context) degrades to the headless
    // frames permanently (the C++'s no-GL `window == nullptr` equivalent),
    // so the re-`resumed`s after activity pauses don't retry (nor log).
    // Desktop: always `false` (the desktop bootstrap is pre-`run`).
    #[cfg(target_os = "android")]
    ensure_android_gl_failed: bool,
    // issue 25b SG5: the down-finger positions (LOGICAL coordinates — the
    // winit touch positions are scaled physical→logical in
    // `RunHandler::window_event`, the C++'s `mapPixelToCoords`),
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
            event_loop: None,
            gl: None,
            #[cfg(target_os = "android")]
            ensure_android_gl_failed: false,
            finger_down_positions: std::collections::HashMap::new(),
            touch_position_sink: None,
            released_edges_sink: None,
            entered_text_sink: None,
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

    /// The entered-text sink's write half (the `TextEntered` appends —
    /// `None` = no-op; the `apply_event` `TextEntered` arm calls it).
    fn with_entered_text_sink(&self, f: impl FnOnce(&mut Vec<char>)) {
        if let Some(sink) = &self.entered_text_sink {
            f(&mut sink.write().expect("entered-text sink poisoned"));
        }
    }

    /// C++ `getFingerDownPositions()` (SSVStart GameWindow.hpp:299-309):
    /// for each finger whose `InputState` bit is set, its current LOGICAL
    /// position — `(0,0)` for a down finger with no recorded position
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
    pub fn run(self, mut on_frame: impl FnMut(&mut Window)) {
        self.run_with_event_hook(None, &mut on_frame);
    }

    /// The [`Self::run`] variant with a per-window-event hook (issue 014b —
    /// the egui console's `egui-winit` event feed, the C++
    /// `game.onAnyEvent += Imgui::processEvent` (HexagonGame.cpp:335) that
    /// the C++ `runEvents` dispatches AFTER the input-state update, per
    /// event, GameWindow.hpp:42-83). The hook sees every raw winit
    /// `WindowEvent` (real windows only — headless runs one frame without
    /// an event loop and never fires it) with the GL backend's winit window
    /// (the `egui-winit` state's window; `None` hook = plain [`Self::run`]).
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
                window.focus = false;
            }
            Event::KeyPressed { code, .. } => window.input.set_key_pressed(code, true),
            Event::KeyReleased { code } => {
                window.input.set_key_pressed(code, false);
                window.with_released_sink(|edges| {
                    edges.keys.insert(code);
                });
            }
            Event::MouseButtonPressed { button } => window.input.set_button_pressed(button, true),
            Event::MouseButtonReleased { button } => {
                window.input.set_button_pressed(button, false);
                window.with_released_sink(|edges| {
                    edges.buttons.insert(button);
                });
            }
            Event::MouseMoved { position } => window.mouse_pixel = position,
            Event::Resized { width, height } => window.set_size(width, height),
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
            Event::MouseWheelScrolled { .. } => {}
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
/// input-state fold, the C++'s order).
struct RunHandler<'a, F> {
    window: &'a mut Window,
    on_event: Option<Box<EventHook>>,
    on_frame: &'a mut F,
}

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
        if let Some(mut e) = map_window_event(&event) {
            // Android (issue 25b SG5): winit delivers the touch position
            // in PHYSICAL pixels (the activity window's drawable); the
            // engine's seam (the C++ `getFingerDownPositions` →
            // `mapPixelToCoords`) wants the LOGICAL coordinates — scale
            // each axis by the window's logical/physical ratio (the
            // screen-sized platform window vs the config logical size;
            // per-axis, the SFML `mapPixelToCoords`). The physical size
            // is the GL surface's (`GlParts`'s `width`/`height` — the
            // native window's screen size on android, the requested
            // physical size on desktop; winit 0.30 has no window-by-id
            // lookup, the surface IS the drawable). Before the first
            // `resumed` (or on a GL-creation failure) the surface is
            // unknown: the position stays raw (the documented headless
            // degradation — no GL, the frames are null-target anyway).
            // Desktop: `WindowEvent::Touch` never arrives (the arm is
            // dead there) — behavior-identical.
            if matches!(e, Event::TouchBegan { .. } | Event::TouchMoved { .. }) {
                if let Some(gl) = self.window.gl.as_ref() {
                    let sx = self.window.get_width() as f32 / (gl.width as f32).max(1.0);
                    let sy = self.window.get_height() as f32 / (gl.height as f32).max(1.0);
                    e = match e {
                        Event::TouchBegan { finger, position } => Event::TouchBegan {
                            finger,
                            position: Vec2::new(position.x * sx, position.y * sy),
                        },
                        Event::TouchMoved { finger, position } => Event::TouchMoved {
                            finger,
                            position: Vec2::new(position.x * sx, position.y * sy),
                        },
                        other => other,
                    };
                }
            }
            Window::apply_event(self.window, e);
        }
        // SFML dispatches a `TextEntered` event ALONGSIDE the
        // `KeyPressed` for a character key (the C++ `runEvents` polls
        // them as two events — the `MenuGame`'s `onEvent(TextEntered)`
        // handler, MenuGame.cpp:491-498, appends to `enteredChars`).
        // winit carries the character in the same `KeyboardInput`'s
        // `text` field (the Android device keymap — the issue-025d
        // on-device evidence: `adb input keyevent 66` arrived with
        // `text: Some("\n")`), so the second SFML event is restored
        // here (the `map_window_event` 1:1 single mapping stays). The
        // `Pressed` gate: SFML's `TextEntered` fires on the press half
        // only (the release's `text` is `None` on the device).
        if let WindowEvent::KeyboardInput { event: ke, .. } = &event {
            if ke.state == winit::event::ElementState::Pressed {
                if let Some(c) = ke.text.as_deref().and_then(|s| s.chars().next()) {
                    Window::apply_event(self.window, Event::TextEntered { character: c });
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
    }
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
    // `getFingerDownPositions` data path; the positions arrive LOGICAL —
    // the `RunHandler::window_event`'s physical→logical scale is the
    // `mapPixelToCoords` equivalent, exercised over the already-scaled
    // `Event`s here).
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
    }
}
