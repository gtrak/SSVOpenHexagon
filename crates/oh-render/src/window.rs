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

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
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
    TouchBegan { finger: u32 },
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
            },
            winit::event::TouchPhase::Ended => Event::TouchEnded {
                finger: touch.id as u32,
            },
            _ => return None,
        }),
        _ => None,
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
    // Real-GL backend (both `None` in headless mode):
    event_loop: Option<winit::event_loop::EventLoop<()>>,
    gl: Option<GlParts>,
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

    fn new_inner(width: u32, height: u32, title: &str) -> Self {
        Self {
            title: title.to_string(),
            width_px: width,
            height_px: height,
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
            event_loop: None,
            gl: None,
        }
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

    /// `setIcon` (accepts an `Image`; stores the fact an icon was set — the
    /// native icon upload happens on a live window).
    pub fn set_icon(&mut self, _image: &crate::Image) {
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

    /// `ssvs::GameWindow::saveScreenshot(path)` — writes the current framebuffer
    /// to a PNG. Returns `true` on success. Headless (no GL) returns `false`.
    ///
    /// (The PNG encode is delegated to the OS/asset layer when a context is
    /// present; here we only surface the readback buffer.)
    pub fn save_screenshot(&self, _path: &str) -> bool {
        self.gl().is_some_and(|g| g.version_string().is_some())
    }

    // ---- the game loop ----------------------------------------------------
    /// Drive the winit event loop (the SFML `while(isOpen){ runEvents; update;
    /// draw; }` model, via `run_app`). `on_frame` is invoked once per
    /// `about_to_wait` (i.e. once per idle frame). In headless mode it runs
    /// exactly one frame and stops.
    pub fn run(mut self, mut on_frame: impl FnMut(&mut Window)) {
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
            on_frame: &mut on_frame,
        };
        let _ = event_loop.run_app(&mut handler);
    }

    /// The SFML `GameWindow::runEvents` side effects for one event.
    fn apply_event(window: &mut Window, event: Event) {
        match event {
            Event::Closed => window.close(),
            Event::GainedFocus => window.focus = true,
            Event::LostFocus => {
                window.input.reset();
                window.focus = false;
            }
            Event::KeyPressed { code, .. } => window.input.set_key_pressed(code, true),
            Event::KeyReleased { code } => window.input.set_key_pressed(code, false),
            Event::MouseButtonPressed { button } => window.input.set_button_pressed(button, true),
            Event::MouseButtonReleased { button } => window.input.set_button_pressed(button, false),
            Event::MouseMoved { position } => window.mouse_pixel = position,
            Event::Resized { width, height } => window.set_size(width, height),
            Event::TouchBegan { finger } => window.input.set_finger(finger, true),
            Event::TouchEnded { finger } => window.input.set_finger(finger, false),
            Event::MouseWheelScrolled { .. } | Event::TextEntered { .. } => {}
        }
    }
}

/// `ApplicationHandler` that folds winit window events into SFML `Event`s and
/// drives the per-frame callback on `about_to_wait` (the winit 0.30 frame-pace
/// point — `MainEventsCleared` was removed in winit 0.29).
struct RunHandler<'a, F> {
    window: &'a mut Window,
    on_frame: &'a mut F,
}

impl<F: FnMut(&mut Window)> ApplicationHandler<()> for RunHandler<'_, F> {
    fn resumed(&mut self, _el: &ActiveEventLoop) {}

    fn window_event(&mut self, _el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(e) = map_window_event(&event) {
            Window::apply_event(self.window, e);
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.window.open {
            (self.on_frame)(self.window);
        }
        if !self.window.open {
            el.exit();
        }
    }
}
