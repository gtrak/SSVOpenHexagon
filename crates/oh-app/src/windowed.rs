//! The C++ `mainClient` windowed path (issue 15 step (c)) — in the C++
//! order:
//!
//! * window creation + config setters + title + timer install
//!   (`main.cpp:297-332`);
//! * engine + MenuGame + the four `fnHG*` / `fnGoToMenu` links
//!   (`main.cpp:413-463`);
//! * the boot dispatch — `gotoMenu` / `-p -l` / compressed-replay
//!   drag&drop (`main.cpp:471-537`);
//! * `window->run()` + `Finished` (`main.cpp:588-595`).
//!
//! # Per-frame order (the C++ `GameWindow::run()` body, GameWindow.hpp:117-150)
//!
//! ```text
//! while(gameEngine->isRunning()) {
//!     [mustRecreate → recreateWindow]     // not ported: created once
//!     renderWindow.setActive(true)        // the GL context is current by construction
//!     this->clear()                       // sf::Color::Transparent (GameWindow.hpp:126,157-160)
//!     gameEngine->refreshTimer()          // GameTimer::refresh no-op (GameTimer.hpp:26-37)
//!     runEvents()                         // poll → inputState + per-event handleEvent
//!     gameEngine->runUpdate()             // refreshInput → timer->runUpdate (N×update) → onPostUpdate
//!     gameEngine->runDraw()               // timer->runDraw → the current state's draw
//!     renderWindow.display()              // the explicit per-frame back-buffer swap (`window.swap_buffers`; winit does NOT swap a glutin `Surface` at event-loop end)
//!     gameEngine->runFPS()                // runFrameTime + runFPS (GameEngine.hpp:96-101)
//! }
//! ```
//!
//! [`frame`] is that body for one frame (the state-switching tests drive it
//! directly with fake states + the REAL [`oh_render::timer::TimerStatic`]);
//! [`WindowedClient::tick`] wraps it with the port's draw target and the
//! loop-exit translation.
//!
//! # Documented gaps (each a C++ line, none silently faked)
//!
//! * **Icon** — C++ `main.cpp:312-328` loads `Assets/icon.png` via
//!   `sf::Image` and sets the window icon (re-applied on window
//!   recreation, `onRecreation`). Ported (issue 015c-tail): the PNG
//!   decode is the screenshot's `png` dependency (`Image::load_png` — the
//!   C++'s stb-based `loadFromFile` always yields RGBA8, which is what the
//!   winit `window::Icon` takes), and `create_window` sets it on the window
//!   (`Window::set_icon` → `GlParts::set_icon` → winit
//!   `set_window_icon`; headless: recorded only). The C++'s
//!   `onRecreation` re-apply (main.cpp:326) has NO port hook: the port's
//!   winit window is created once and never recreated at runtime (module
//!   docs: the `pixel_mult` note — the C++'s `mustRecreate`/`recreateWindow`
//!   deferral, GameWindow.hpp:89-93, has no port equivalent), so the icon
//!   is set exactly once, at creation. The C++'s load-failure log
//!   ("Failed to load icon image", main.cpp:318) is ported too.
//! * **SIGINT** — C++ `main.cpp:340-349` (block `334-350`) installs a
//!   `SIGINT` handler that stops the engine; its own `TODO (P2): UB`
//!   comment (main.cpp:339) marks the non-atomic stop IN the signal
//!   context. Ported safe (issue 015c-tail): `signal-hook`'s
//!   `flag::register` sets the client's `interrupt` `Arc<AtomicBool>` on
//!   SIGINT (the signal context does ONE atomic store); the loop head
//!   (`tick_fields`, the C++'s `while(isRunning())` position,
//!   GameWindow.hpp:121) performs the SAME actions — the handler's three
//!   log lines (main.cpp:343-348) in the C++'s order around the deferred
//!   `globalWindow.stop()` (main.cpp:347) — then closes the window, which
//!   ends the port's winit loop (the C++'s loop condition is the engine's
//!   `isRunning()`; the port's winit loop ends on the window's close,
//!   below). `install_sigint` is called by `main_client` before the boot
//!   dispatch (the C++'s placement, before `window->run()`, main.cpp:588).
//! * **Input management** — the C++ per-frame `refreshInput` +
//!   per-tick `updateInput` run the SSVStart `InputManager`
//!   (GameEngine.hpp:84; GameState.hpp:49-57), which maps the keyboard
//!   binds (`Config::getTrigger(Tid::...)`, HexagonGame.cpp:352-365) onto
//!   the states' actions. The SSVStart input manager is not ported, so the
//!   window's input state is current (winit folds every event into
//!   `Window::input`) but no trigger→action wiring exists yet: the windowed
//!   build runs the timer / update / draw / state-switching loop but
//!   keyboard play is not yet available. The per-event
//!   `gameEngine->handleEvent` (GameWindow.hpp:79 → GameEngine.hpp:58-62)
//!   is a no-op for both states here: the C++ `HexagonGame`'s `onAnyEvent`
//!   is `Imgui::processEvent` (HexagonGame.cpp:335 — the ImGui console,
//!   unported) and `MenuGame` registers none.
//! * **fnHGTriggerRefresh** — C++ `main.cpp:436-440`
//!   (`hg.refreshTrigger(trigger, bindId)`): the `HexagonGame::refreshTrigger`
//!   (SSVStart input manager) is unported with the input management above;
//!   the wired closure documents the no-op.
//! * **fnHGUpdateRichPresenceCallbacks** — C++ `main.cpp:451-454`
//!   (`hg.updateRichPresenceCallbacks()`): the wired closure pumps the
//!   engine's `steam_manager` seam (the C++'s Steam half + the
//!   `steamAttempt`/`steamHung` counter now in the manager — issue 020);
//!   the Discord half is issue 21 (absent).
//! * **MenuGame `draw`** — C++ `MenuGame.cpp:259`
//!   (`game.onDraw += [this] { draw(); }`): wired to
//!   `crate::menu_draw::draw` (the issue-013b bridge — the full C++
//!   `MenuGame::draw` surface, main menu / level selection / profile /
//!   entering-text / load-results / online status). The menu's UPDATE is
//!   wired — its state machine runs on the real timer.
//! * **fnGoToMenu timing** — C++ `main.cpp:456-462` runs
//!   `returnToLevelSelection` + `init(error)` + `setGameState(menu)`
//!   synchronously at the tail of `HexagonGame::goToMenu`. The engine's
//!   callback member must be `Send` (the engine is `Send` — its timeline
//!   actions are), so the wired closure only sets a thread-atomic flag;
//!   the game state's `update_from_timer` consumes it at the START of the
//!   NEXT tick. Between a tick's `goToMenu` and the next tick nothing
//!   routes differently (the switch lands in the same gap the C++ switch
//!   lands in), EXCEPT the rare `goToMenu` from the `onRenderStage`
//!   Lippincott error path (during `draw`, `HGGraphics.cpp:74-77`): there
//!   the port's switch lands at the start of the next frame's first tick
//!   instead of mid-draw — a ≤1-frame difference on an error path no
//!   shipped level takes.
//! * **Boot-dispatch null states** — the C++ `-p -l` path
//!   (`main.cpp:507-511`: `mg->init` only) and the replay decompress-failure
//!   path (`main.cpp:488-492`: log + `return`) NEVER call
//!   `window->setGameState`, leaving the engine's `gameState` null — the
//!   first frame's `assert(isValid())` (GameEngine.hpp:41) then fires. The
//!   port points the window at the MENU state on both paths (documented
//!   deviation: the C++'s null-state crash is not reproduced).
//! * **GL surface** — the [`GlDrawTarget`] clears the real framebuffer and
//!   uploads the scene vertex batches (walls / player) through
//!   [`oh_render::batching::GlBatchUploader`]; the sprite / text / rectangle
//!   passes are the issue-025c GL passes (the `oh_render::gl_passes`
//!   textured-quad pipeline — the pool texture's on-demand RGBA8 upload
//!   (straight alpha, the SFML match) + the textured quad shader for the
//!   sprite's transform/rect and the font-atlas glyphs (the per-glyph SFML
//!   `Text::draw` geometry, `oh_render::glyphs`), the batch uploader's
//!   solid-colour stream for the `RectangleShape` fill + outline ring —
//!   plus the per-draw-call SFML blend application, `GlParts::apply_blend`),
//!   and the screenshot save is the REAL GL readback → PNG (the C++
//!   `GameWindow::saveScreenshot`, GameWindow.hpp:167-174 —
//!   `GlParts::save_screenshot`: `glReadPixels` bottom-up → row flip →
//!   `png`-crate encode; D-004 SFML entry). The per-stage pack-shader
//!   binding (`RenderStates::shader`) stays the documented issue-10 gap:
//!   the internal passes run, the pack's fragment shaders do not execute.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, RwLock};

use oh_config::{Config, Key, Tid, Trigger, TIME_SLICE, TIME_STEP};
use oh_game::hexagon_game::HexagonGame;
use oh_game::seams::{Audio, NullProfiles};
use oh_lua::api::state::GameApiState;
use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::{
    MenuTouch, MenuWindow, NewGameCallback, RichPresenceCallback, TriggerRefreshCallback,
};
use oh_menu::states::States;
use oh_menu::{AudioSink, MenuConfig, MenuGame, NoopNetwork, RealMenuAssets};
use oh_render::batching::GlBatchUploader;
use oh_render::draw::{BlendMode, RenderStates, Vertex};
use oh_render::font::{Font, Text};
use oh_render::graphics::DrawTarget;
use oh_render::input::InputState;
use oh_render::input_manager::{Manager, Mode, Type};
use oh_render::texture::Sprite;
use oh_render::timer::TimerStatic;
use oh_render::types::RectangleShape;
use oh_render::view::View;
use oh_render::window::Window;
use oh_render::GlParts;

use crate::bridge::AppDraw;
use crate::compose::compose_with_config;
use crate::game_state::{GameEngine, GameSlot, GameState};

/// The menu's `AudioSink` over the REAL `oh_audio::Audio` (issue 025e — the
/// C++ `hg::Audio`, main.cpp:392-401, shared with the engine). The menu's two
/// `AudioSink` calls map onto the C++ menu's audio calls:
///
/// * `play_sound(assetId)` → C++ `MenuGame::playSoundOverride(assetId)`
///   (MenuGame.cpp:1057-1061) — the `!Config::getNoSound()` gate +
///   `audio.playSoundOverride(assetId)`.
/// * `stop_all()` → C++ `audio.stopMusic(); audio.stopSounds();`
///   (MenuGame.cpp:798-799, the `MenuGame::init` teardown).
struct MenuAudioSink {
    audio: std::sync::Arc<oh_audio::Audio>,
    config: std::sync::Arc<std::sync::RwLock<Config>>,
}

impl AudioSink for MenuAudioSink {
    fn play_sound(&self, asset_id: &str) {
        // C++ `MenuGame::playSoundOverride` (MenuGame.cpp:1057-1061) gates the
        // menu SFX on `!Config::getNoSound()` (the engine's own one-shots are
        // gated by its `shouldPlaySounds`; the menu's go straight to the
        // audio object, so the gate lives here).
        if self.config.read().expect("config poisoned").get_no_sound() {
            return;
        }
        self.audio.play_sound_override(asset_id);
    }

    fn stop_all(&self) {
        // C++ `audio.stopMusic(); audio.stopSounds();` (MenuGame.cpp:798-799).
        self.audio.stop_music();
        self.audio.stop_sounds();
    }
}

/// issue 25b SG5: the engine's `Renderer` seam over the window's
/// down-finger positions (the C++ `getFingerDownPositions` data path —
/// the C++'s window IS the position source; the port's windowed client
/// owns both sides: the window's touch events update the shared slot
/// array (the C++'s 0..`FINGER_COUNT` order; the window's CURRENT size
/// coordinates — issue 025g: the raw winit coordinates, the C++'s
/// `mapPixelToCoords` target on the phone; the 025b-era physical →
/// config-logical rescale is gone, `oh-render`'s `RunHandler` note),
/// the engine's `updateInput_UpdateTouchControls` reads it through this
/// renderer every update tick). Desktop: `WindowEvent::Touch` never
/// arrives — every slot stays `None`, the engine sees the exact
/// no-renderer behavior (behavior-identical, the 25b constraint).
struct WindowTouchRenderer {
    positions: std::sync::Arc<
        std::sync::RwLock<[Option<oh_math::ssvs::Vec2>; oh_render::input::FINGER_COUNT]>,
    >,
}

impl oh_game::renderer::Renderer for WindowTouchRenderer {
    fn finger_down_positions(&self) -> Vec<oh_math::ssvs::Vec2> {
        self.positions
            .read()
            .expect("touch sink poisoned")
            .iter()
            .filter_map(|slot| *slot)
            .collect()
    }
}

/// issue 025f — the menu's `MenuTouch` seam over the SAME shared touch
/// sink (the C++ `window.getFingerDownCount()` / `getFingerDownPositions()`
/// data the 25b SG5 window fold maintains — the C++'s `mapPixelToCoords`
/// window coordinates; the engine's `WindowTouchRenderer` above reads
/// the identical sink): the platform feed of the 9-zone touch grid
/// (oh-menu `MenuGame:: update`, MenuGame.cpp:2802-2835). The fold passes
/// the raw winit touch coordinates through (issue 025g: the physical
/// drawable pixels ARE the window's current-size space — the C++'s
/// `mapPixelToCoords` target on the phone; the 025b-era physical →
/// config-logical rescale is gone, `oh-render`'s `RunHandler` note), so
/// the positions are in the ACTUAL window space the grid's thirds run
/// in (issue 025g: the `Resized` fold sets the window's size to the
/// native window size — winit delivers it before the first frame — and
/// keeps it current on every later resize, the IME's ADJUST_RESIZE
/// included). Desktop: no `WindowEvent::Touch` arrives — the sink stays
/// empty, the grid never fires (behavior-identical to the C++ desktop;
/// the mouse-zone parity is the operator's later step — the
/// `MouseButtonReleased` exec path must not double-fire, the oh-menu
/// `MenuTouch` seam note).
struct MenuTouchSink {
    positions: std::sync::Arc<
        std::sync::RwLock<[Option<oh_math::ssvs::Vec2>; oh_render::input::FINGER_COUNT]>,
    >,
}

impl MenuTouch for MenuTouchSink {
    fn finger_down_count(&self) -> usize {
        self.positions
            .read()
            .expect("touch sink poisoned")
            .iter()
            .filter(|slot| slot.is_some())
            .count()
    }
    fn finger_down_positions(&self) -> Vec<oh_math::ssvs::Vec2> {
        self.positions
            .read()
            .expect("touch sink poisoned")
            .iter()
            .filter_map(|slot| *slot)
            .collect()
    }
}

/// issue 025g — the shared ACTUAL-window-size slot (the C++
/// `window.getRenderWindow().getSize()` live value — the actual current
/// render-target size: the desktop's requested physical size, the
/// Android native window's screen size; `MenuGame::getWindowWidth()` /
/// `getWindowHeight()`, MenuGame.cpp:3350-3357). `tick_fields` refreshes
/// it every frame from the `Window`'s current size bookkeeping (`width_px`
/// / `height_px` — the `WindowEvent::Resized` fold's `set_size`; the
/// window's creation size before the first resize — the liveness the C++
/// gets from reading the window object at every call site; the
/// config-vs-actual divergence the seam fixes, the oh-menu `MenuWindow`
/// seam docs); the menu's `MenuWindow` seam reads it.
pub struct RenderWindowSize {
    size: std::sync::RwLock<(f32, f32)>,
}

impl RenderWindowSize {
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            size: std::sync::RwLock::new((width as f32, height as f32)),
        }
    }
    /// The per-frame refresh (a no-op when unchanged).
    pub fn update(&self, width: u32, height: u32) {
        let mut s = self.size.write().expect("render-window-size poisoned");
        if s.0 != width as f32 || s.1 != height as f32 {
            *s = (width as f32, height as f32);
        }
    }
    #[must_use]
    pub fn get(&self) -> (f32, f32) {
        *self.size.read().expect("render-window-size poisoned")
    }
}

/// issue 025g — the menu's `MenuWindow` seam over the [`RenderWindowSize`]
/// slot (the C++ ctor's `window` reference's size half —
/// `MenuGame::getWindowWidth()` / `getWindowHeight()` read
/// `window.getRenderWindow().getSize()`, the ACTUAL current render-target
/// size, NOT the config's resolution — the oh-menu `MenuWindow` docs).
struct MenuWindowSource {
    size: std::sync::Arc<RenderWindowSize>,
}

impl MenuWindow for MenuWindowSource {
    fn width(&self) -> f32 {
        self.size.get().0
    }
    fn height(&self) -> f32 {
        self.size.get().1
    }
}

/// `MenuConfig` over the REAL `oh_config::Config` (the C++ menu reads
/// `hg::Config` directly; the port's seam is the same object the engine
/// composition uses — one `Config`, shared by `Arc<RwLock<...>>`).
/// C++ `MenuGame::refreshBinds()`' input-manager half as a fill-in slot
/// (`WindowedClient::build` fills it after the menu + both managers
/// exist; the menu's "RESET CONFIG" / "reset binds" items call through
/// it — menu.rs:145,219).
type RefreshBindsSlot = Rc<RefCell<Option<Box<dyn Fn()>>>>;

pub struct ConfigMenuAdapter {
    config: Arc<RwLock<Config>>,
    /// C++ `MenuGame::refreshBinds()` (MenuGame.cpp:3373-3387) — the
    /// input-manager half, filled by `WindowedClient::build` (the options'
    /// "RESET CONFIG" / "reset binds" items call it, menu.rs:145,219).
    refresh_binds: RefreshBindsSlot,
}

impl ConfigMenuAdapter {
    #[must_use]
    pub fn new(config: Arc<RwLock<Config>>, refresh_binds: RefreshBindsSlot) -> Self {
        Self {
            config,
            refresh_binds,
        }
    }
}

/// C++ `sf::Color::Transparent` (the `GameWindow::run` pre-update clear's
/// default, GameWindow.hpp:126,157).
const COLOR_TRANSPARENT: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

impl MenuConfig for ConfigMenuAdapter {
    // --- layout / sizing ---
    fn size_x(&self) -> f32 {
        self.config.read().expect("config poisoned").get_size_x()
    }
    fn size_y(&self) -> f32 {
        self.config.read().expect("config poisoned").get_size_y()
    }
    fn zoom_factor(&self) -> f32 {
        self.config
            .read()
            .expect("config poisoned")
            .get_zoom_factor()
    }
    fn first_time_playing(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_first_time_playing()
    }
    fn set_first_time_playing(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_first_time_playing(value);
    }
    fn show_login_at_startup(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_show_login_at_startup()
    }
    fn set_show_login_at_startup(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_show_login_at_startup(value);
    }
    fn black_and_white(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_black_and_white()
    }
    fn set_black_and_white(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_black_and_white(value);
    }
    fn official(&self) -> bool {
        self.config.read().expect("config poisoned").get_official()
    }
    fn set_official(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_official(value);
    }
    fn debug(&self) -> bool {
        self.config.read().expect("config poisoned").get_debug()
    }
    fn set_debug(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_debug(value);
    }
    fn joystick_deadzone(&self) -> f32 {
        self.config
            .read()
            .expect("config poisoned")
            .get_joystick_deadzone()
    }
    fn set_joystick_deadzone(&self, value: f32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_joystick_deadzone(value);
    }

    // --- gameplay ---
    fn auto_restart(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_auto_restart()
    }
    fn set_auto_restart(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_auto_restart(value);
    }
    fn rotate_to_start(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_rotate_to_start()
    }
    fn set_rotate_to_start(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_rotate_to_start(value);
    }
    fn invincible(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_invincible()
    }
    fn set_invincible(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_invincible(value);
    }
    fn timescale(&self) -> f32 {
        self.config.read().expect("config poisoned").get_timescale()
    }
    fn set_timescale(&self, value: f32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_timescale(value);
    }
    fn save_last_username(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_save_last_login_username()
    }
    fn set_save_last_username(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_save_last_login_username(value);
    }
    fn last_login_username(&self) -> String {
        self.config
            .read()
            .expect("config poisoned")
            .get_last_login_username()
            .to_string()
    }
    fn set_last_login_username(&self, value: &str) {
        self.config
            .write()
            .expect("config poisoned")
            .set_last_login_username(value);
    }
    fn reset_config_to_defaults(&self) {
        self.config
            .write()
            .expect("config poisoned")
            .reset_config_to_defaults();
    }
    fn reset_binds_to_defaults(&self) {
        self.config
            .write()
            .expect("config poisoned")
            .reset_binds_to_defaults();
    }
    fn refresh_binds(&self) {
        // C++ `MenuGame::refreshBinds()` (MenuGame.cpp:3373-3387) — the
        // per-trigger `game.refreshTrigger` + `fnHGTriggerRefresh` loop over
        // BOTH input managers (issue 010b); the
        // `Config::loadAllJoystickBinds()` tail is the unported joystick
        // (NoJoystick seam) — documented no-op.
        if let Some(f) = self.refresh_binds.borrow().as_ref() {
            f();
        }
    }
    fn fullscreen(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_fullscreen()
    }
    fn set_fullscreen(&self, value: bool) {
        // C++ `Config::setFullscreen(window, value)` (MenuGame.cpp:1374-1379)
        // also recreates the window; the port's winit window is not
        // recreated at runtime (module docs: `set_pixel_mult` note) — the
        // config value is set, the recreation is the documented gap.
        self.config
            .write()
            .expect("config poisoned")
            .set_fullscreen(value);
    }
    fn set_resolution(&self, w: u32, h: u32) {
        // C++ `changeResolutionTo` → `Config::setCurrentResolution`.
        self.config
            .write()
            .expect("config poisoned")
            .set_current_resolution(w, h);
    }
    fn set_resolution_auto(&self) {
        // C++ "automatically set resolution" item (MenuGame.cpp:1321-1323)
        // → the `windowedAutoResolution` / `fullscreenAutoResolution` flags
        // + `reapplyResolution`; the port's `Config` keeps the flags but
        // `set_current_resolution_auto` needs the desktop size, which this
        // seam does not hold — documented no-op.
    }
    fn resolution_modes(&self) -> Vec<(u32, u32)> {
        // C++ builds the RESOLUTION submenus from
        // `sf::VideoMode::getFullscreenModes()` (MenuGame.cpp:1332-1363) —
        // display-dependent runtime data; the port's winit/monitor-mode
        // equivalent is unported, so (as with `FakeConfig`) the list is
        // empty: the "automatically set resolution" / "go windowed" /
        // "go fullscreen" items still work.
        Vec::new()
    }

    // --- graphics / text ---
    fn draw_text_outlines(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_draw_text_outlines()
    }
    fn set_draw_text_outlines(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_draw_text_outlines(value);
    }
    fn text_padding(&self) -> i32 {
        // C++ `Config::getTextPadding()` is a float; the port's seam takes
        // an int (the C++ value is integral in all shipped configs).
        self.config
            .read()
            .expect("config poisoned")
            .get_text_padding() as i32
    }
    fn set_text_padding(&self, value: i32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_text_padding(value as f32);
    }
    fn text_scaling(&self) -> f32 {
        self.config
            .read()
            .expect("config poisoned")
            .get_text_scaling()
    }
    fn set_text_scaling(&self, value: f32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_text_scaling(value);
    }
    fn antialiasing(&self) -> u32 {
        self.config
            .read()
            .expect("config poisoned")
            .get_antialiasing_level()
    }
    fn set_antialiasing(&self, value: u32) {
        // C++ `Config::setAntialiasingLevel(window, n)` (MenuGame.cpp:1452-1464)
        // also recreates the window with the new MSAA level — the runtime
        // recreation is the documented gap; the config value is set.
        self.config
            .write()
            .expect("config poisoned")
            .set_antialiasing_level(value);
    }
    fn darken_uneven_background_chunk(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_darken_uneven_background_chunk()
    }
    fn set_darken_uneven_background_chunk(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_darken_uneven_background_chunk(value);
    }

    // --- HUD toggles ---
    fn show_key_icons(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_show_key_icons()
    }
    fn set_show_key_icons(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_show_key_icons(value);
    }
    fn key_icons_scale(&self) -> f32 {
        self.config
            .read()
            .expect("config poisoned")
            .get_key_icons_scale()
    }
    fn set_key_icons_scale(&self, value: f32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_key_icons_scale(value);
    }
    fn show_level_info(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_show_level_info()
    }
    fn set_show_level_info(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_show_level_info(value);
    }
    fn show_timer(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_show_timer()
    }
    fn set_show_timer(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_show_timer(value);
    }
    fn show_status_text(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_show_status_text()
    }
    fn set_show_status_text(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_show_status_text(value);
    }

    // --- 3D / fx ---
    fn three_d(&self) -> bool {
        self.config.read().expect("config poisoned").get_3d()
    }
    fn set_three_d(&self, value: bool) {
        self.config.write().expect("config poisoned").set_3d(value);
    }
    fn shaders(&self) -> bool {
        self.config.read().expect("config poisoned").get_shaders()
    }
    fn set_shaders(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_shaders(value);
    }
    fn no_pulse(&self) -> bool {
        self.config.read().expect("config poisoned").get_no_pulse()
    }
    fn set_no_pulse(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_no_pulse(value);
    }
    fn no_rotation(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_no_rotation()
    }
    fn set_no_rotation(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_no_rotation(value);
    }
    fn no_background(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_no_background()
    }
    fn set_no_background(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_no_background(value);
    }
    fn flash(&self) -> bool {
        self.config.read().expect("config poisoned").get_flash()
    }
    fn set_flash(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_flash(value);
    }
    fn camera_shake_multiplier(&self) -> f32 {
        self.config
            .read()
            .expect("config poisoned")
            .get_camera_shake_multiplier()
    }
    fn set_camera_shake_multiplier(&self, value: f32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_camera_shake_multiplier(value);
    }
    fn angle_tilt_intensity(&self) -> f32 {
        self.config
            .read()
            .expect("config poisoned")
            .get_angle_tilt_intensity()
    }
    fn set_angle_tilt_intensity(&self, value: f32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_angle_tilt_intensity(value);
    }

    // --- player trail / swap fx ---
    fn show_player_trail(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_show_player_trail()
    }
    fn set_show_player_trail(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_show_player_trail(value);
    }
    fn player_trail_alpha(&self) -> u32 {
        self.config
            .read()
            .expect("config poisoned")
            .get_player_trail_alpha()
    }
    fn set_player_trail_alpha(&self, value: u32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_player_trail_alpha(value);
    }
    fn player_trail_scale(&self) -> f32 {
        self.config
            .read()
            .expect("config poisoned")
            .get_player_trail_scale()
    }
    fn set_player_trail_scale(&self, value: f32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_player_trail_scale(value);
    }
    fn player_trail_decay(&self) -> f32 {
        self.config
            .read()
            .expect("config poisoned")
            .get_player_trail_decay()
    }
    fn set_player_trail_decay(&self, value: f32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_player_trail_decay(value);
    }
    fn player_trail_has_swap_color(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_player_trail_has_swap_color()
    }
    fn set_player_trail_has_swap_color(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_player_trail_has_swap_color(value);
    }
    fn show_swap_particles(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_show_swap_particles()
    }
    fn set_show_swap_particles(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_show_swap_particles(value);
    }
    fn show_swap_blinking_effect(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_show_swap_blinking_effect()
    }
    fn set_show_swap_blinking_effect(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_show_swap_blinking_effect(value);
    }

    // --- audio / misc ---
    fn vsync(&self) -> bool {
        self.config.read().expect("config poisoned").get_vsync()
    }
    fn set_vsync(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_vsync(value);
    }
    fn limit_fps(&self) -> bool {
        self.config.read().expect("config poisoned").get_limit_fps()
    }
    fn set_limit_fps(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_limit_fps(value);
    }
    fn max_fps(&self) -> u32 {
        self.config.read().expect("config poisoned").get_max_fps()
    }
    fn set_max_fps(&self, value: u32) {
        // C++ `Config::setMaxFPS(window, n)` (MenuGame.cpp:1431) also applies
        // the limit to the live window — the runtime re-apply is the
        // documented recreation gap; the config value is set.
        self.config
            .write()
            .expect("config poisoned")
            .set_max_fps(value);
    }
    fn show_fps(&self) -> bool {
        self.config.read().expect("config poisoned").get_show_fps()
    }
    fn set_show_fps(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_show_fps(value);
    }
    fn no_sound(&self) -> bool {
        self.config.read().expect("config poisoned").get_no_sound()
    }
    fn set_no_sound(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_no_sound(value);
    }
    fn no_music(&self) -> bool {
        self.config.read().expect("config poisoned").get_no_music()
    }
    fn set_no_music(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_no_music(value);
    }
    fn sound_volume(&self) -> u32 {
        // C++ `Config::getSoundVolume()` is a float (Config.cpp:309); the
        // port's seam takes a u32 (the shipped value, 100.f, is integral).
        self.config
            .read()
            .expect("config poisoned")
            .get_sound_volume() as u32
    }
    fn set_sound_volume(&self, value: u32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_sound_volume(value as f32);
    }
    fn music_volume(&self) -> u32 {
        self.config
            .read()
            .expect("config poisoned")
            .get_music_volume() as u32
    }
    fn set_music_volume(&self, value: u32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_music_volume(value as f32);
    }
    fn music_speed_dm_sync(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_music_speed_dm_sync()
    }
    fn set_music_speed_dm_sync(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_music_speed_dm_sync(value);
    }
    fn music_speed_multiplier(&self) -> f32 {
        self.config
            .read()
            .expect("config poisoned")
            .get_music_speed_mult()
    }
    fn set_music_speed_multiplier(&self, value: f32) {
        self.config
            .write()
            .expect("config poisoned")
            .set_music_speed_mult(value);
    }
    fn play_swap_ready_sound(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_play_swap_ready_sound()
    }
    fn set_play_swap_ready_sound(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_play_swap_ready_sound(value);
    }
    fn use_lua_file_cache(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_use_lua_file_cache()
    }
    fn set_use_lua_file_cache(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_use_lua_file_cache(value);
    }
    fn clear_lua_file_cache(&self) {
        // C++ "clear lua file cache" advanced item → the per-assets
        // `getLuaFileCache()` map; the composition's host owns the
        // equivalent map (compose.rs) and exposes no clear — documented
        // no-op (the cache is per-process, so a new run clears it).
    }
    fn disable_game_rendering(&self) -> bool {
        self.config
            .read()
            .expect("config poisoned")
            .get_disable_game_rendering()
    }
    fn set_disable_game_rendering(&self, value: bool) {
        self.config
            .write()
            .expect("config poisoned")
            .set_disable_game_rendering(value);
    }
}

/// `DrawTarget` over a live [`GlParts`] context (the windowed loop's real
/// target). `clear_black` is a real `glClear`; `draw_vertices` submits the
/// scene batches through [`GlBatchUploader`]; the sprite / text /
/// rectangle passes are the issue-025c GL passes (the
/// [`oh_render::gl_passes::GlPasses`] textured-quad pipeline for sprite +
/// font-atlas text, the batch uploader's solid-colour stream for
/// rectangles — the module docs' GL-surface bullet), and the screenshot
/// save is the REAL GL readback → PNG (the C++ `GameWindow::
/// saveScreenshot`, GameWindow.hpp:167-174 — `GlParts::save_screenshot`:
/// `glReadPixels` bottom-up → row flip → `png`-crate encode; D-004 SFML
/// entry). `'a` is the window's borrow (the `GlParts` lives inside the
/// `WindowedClient`'s window for the whole loop).
///
/// # State hygiene (the C++ per-draw-call `RenderStates`)
///
/// Every draw call re-applies its blend mode first (SFML applies the
/// render states' blend BEFORE the draw — the C++'s `window->draw(x,
/// states)` per call; the game's stages all use the `Alpha` default,
/// `gl.enable(BLEND)` + `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` — the pre-025c
/// vertex pass left GL blending at the driver default (off), which the
/// C++ never does: the correction is behaviour-neutral for the opaque
/// scene batches and enables the C++'s alpha-blended flash / 3D layers on
/// every platform). The program / texture / VAO / uniform re-application
/// is inside each pass (`oh_render::gl_passes`), per draw call — the C++
/// frame order (the caller's draw sequence) is untouched.
pub struct GlDrawTarget<'a> {
    parts: &'a GlParts,
    view: View,
}

impl<'a> GlDrawTarget<'a> {
    /// `parts` must outlive the target (the `WindowedClient`'s window does
    /// — the target is a per-frame scratch built and dropped in `tick`).
    /// The GL objects (the batch uploader, the texture cache, the textured
    /// program, the font atlas) live in the `GlParts` — the target only
    /// borrows them (the SG5 state-hygiene pin: nothing is re-created per
    /// frame).
    #[must_use]
    pub fn new(parts: &'a GlParts) -> Self {
        Self {
            parts,
            view: View::new(
                oh_math::ssvs::Vec2::new(parts.width as f32 / 2.0, parts.height as f32 / 2.0),
                oh_math::ssvs::Vec2::new(parts.width as f32, parts.height as f32),
            ),
        }
    }
}

impl DrawTarget for GlDrawTarget<'_> {
    fn clear_black(&mut self) {
        // C++ `window->clear(sf::Color::Black)` (HGGraphics.cpp:83).
        self.parts.clear([0.0, 0.0, 0.0, 1.0]);
    }

    fn set_view(&mut self, view: View) {
        self.view = view;
    }

    fn draw_vertices(&mut self, vertices: &[Vertex], states: RenderStates) {
        if vertices.is_empty() {
            return;
        }
        // The C++ per-draw-call render states' blend (the `GlParts::
        // apply_blend` doc: the pre-025c pass left GL blending off — the
        // correction); the per-stage shader binding is the issue-10 GL
        // surface (the batch uploads with the default program).
        self.parts.apply_blend(states.blend_mode);
        // The per-stage `RenderStates` (shaders) are the issue-10 GL
        // surface; the batch uploads with the default program. The
        // uploader is the `GlParts`-shared one (the SG5 audit: the
        // pre-025c per-frame re-creation leaked the GL objects).
        let mut slot = self.parts.batch();
        let failed = slot.failed;
        match slot.uploader.as_mut() {
            Some(u) => u.draw(&self.parts.gl, vertices, self.view),
            None if !failed => match GlBatchUploader::new(&self.parts.gl) {
                Ok(mut u) => {
                    u.draw(&self.parts.gl, vertices, self.view);
                    slot.uploader = Some(u);
                }
                Err(e) => {
                    // Documented: the scene batch is not submitted (the GL
                    // program could not be created on this context).
                    eprintln!("[GlDrawTarget] GL batch uploader unavailable: {e}");
                    slot.failed = true;
                }
            },
            None => {}
        }
    }

    fn draw_sprite(&mut self, sprite: &Sprite) {
        // C++ `window->draw(sprite)` — the default render states (the Alpha
        // blend; the textured-quad pass, issue-025c SG2 — the sprite's
        // position/rotation/scale/origin + texture rect + the pool texture's
        // on-demand upload, `oh_render::gl_passes`).
        self.parts.apply_blend(BlendMode::Alpha);
        let mut passes = self.parts.passes();
        passes.draw_sprite(&self.parts.gl, sprite, self.view);
    }

    fn draw_text(&mut self, text: &Text, states: RenderStates) {
        // C++ `window->draw(text, states)` — the font-atlas text pass
        // (issue-025c SG3): the per-glyph quads (the SFML `Text::draw`
        // geometry, `oh_render::glyphs`'s layout) through the textured quad
        // with the shared atlas texture (the raster cache + the growth
        // re-upload live in the `GlParts`-shared `GlPasses` — NOT the
        // per-frame scratch, the frame order stays the C++'s).
        self.parts.apply_blend(states.blend_mode);
        let mut passes = self.parts.passes();
        passes.draw_text(&self.parts.gl, text, self.view);
    }

    fn draw_rectangle(&mut self, rect: &RectangleShape, states: RenderStates) {
        // C++ `window->draw(rectangleShape, states)` — the solid-colour
        // pass (issue-025c SG4): the shape's fill quad + the outline ring's
        // four quads (`oh_render::gl_passes::rectangle_vertices`), through
        // the batch uploader's vertex-colour program.
        let vertices = oh_render::gl_passes::rectangle_vertices(rect);
        if vertices.is_empty() {
            return;
        }
        self.parts.apply_blend(states.blend_mode);
        let mut slot = self.parts.batch();
        let failed = slot.failed;
        match slot.uploader.as_mut() {
            Some(u) => u.draw(&self.parts.gl, &vertices, self.view),
            None if !failed => match GlBatchUploader::new(&self.parts.gl) {
                Ok(mut u) => {
                    u.draw(&self.parts.gl, &vertices, self.view);
                    slot.uploader = Some(u);
                }
                Err(e) => {
                    eprintln!("[GlDrawTarget] GL batch uploader unavailable: {e}");
                    slot.failed = true;
                }
            },
            None => {}
        }
    }

    fn save_screenshot(&mut self, path: &str) {
        // C++ `window->saveScreenshot("screenshot.png")` (HGGraphics.cpp:
        // 305-313, the `mustTakeScreenshot` gate; `saveScreenshot` itself
        // is GameWindow.hpp:167-174): read the default framebuffer —
        // AFTER the frame's draws (this runs inside `draw_frame`'s step 12,
        // before the swap) — flip bottom-up→top-down, write the PNG.
        // The C++ ignores the `saveToFile` result; mirror that (no return
        // on the `DrawTarget` surface).
        self.parts.save_screenshot(path);
    }

    fn present(&mut self) {
        // C++ `renderWindow.display()` (GameWindow.hpp:145 — the swap after
        // the frame's draws). Issue 25, on-device finding: this was never
        // wired — every GL platform presented nothing (the desktop live
        // check was MANUAL and never run; the emulator caught it).
        self.parts.present();
    }
}

/// A no-op target (headless windows / GL-less fallback): the C++'s
/// `window == nullptr` equivalent — the game bridge's
/// `window_present()` guard is what actually no-ops the game draw; this
/// exists so the loop's `engine.draw(target)` always has a target.
pub struct NullDrawTarget;

impl DrawTarget for NullDrawTarget {
    fn clear_black(&mut self) {}
    fn set_view(&mut self, _view: View) {}
    fn draw_vertices(&mut self, _vertices: &[Vertex], _states: RenderStates) {}
    fn draw_sprite(&mut self, _sprite: &Sprite) {}
    fn draw_text(&mut self, _text: &Text, _states: RenderStates) {}
    fn draw_rectangle(&mut self, _rect: &RectangleShape, _states: RenderStates) {}
    fn save_screenshot(&mut self, _path: &str) {}
}

/// The C++ `hg` (`HexagonGame`) `GameState` (main.cpp:448 `hg.getGame()`):
/// the engine's `update(mFT, 1.f)` / `postUpdate()` / `draw()` (the D-020
/// bridge) — plus the C++ `fnGoToMenu` consumption (module docs: timing).
struct GameSlotAdapter {
    game: Rc<HexagonGame<GameApiState>>,
    menu: Rc<MenuGame>,
    app_draw: Rc<RefCell<AppDraw>>,
    /// The C++ `steamManager` (main.cpp:251) — the `fnGoToMenu`
    /// consumption's `MenuGame::init`-lifecycle Steam calls
    /// (MenuGame.cpp:793-794; `oh-menu` is extend-only, so the
    /// composition makes them here, issue 020).
    steam: Arc<oh_steam::SteamManager>,
    /// The C++ `discordManager` (main.cpp:264-269) — the `fnGoToMenu`
    /// consumption's `MenuGame::init`-lifecycle Discord call
    /// (MenuGame.cpp:796; `oh-menu` is extend-only, issue 021).
    discord: Option<Arc<dyn oh_discord::Discord>>,
    /// The C++ `fnGoToMenu` signal (0 = none, 1 = no-error, 2 = error):
    /// set by the engine's stored callback, consumed at the start of the
    /// next tick (module docs: fnGoToMenu timing).
    return_to_menu: Arc<AtomicU8>,
    /// `window->setGameState(mg->getGame())` (main.cpp:461) — resolved
    /// through the shared state-switch slot (the engine is built from this
    /// adapter, so it cannot be captured directly; the `WindowedClient`
    /// fills the slot before `boot` and keeps the engine alive).
    engine_slot: Rc<RefCell<Option<Rc<GameEngine>>>>,
    /// The C++ `HexagonGame::game` member's `ssvs::Input::Manager`
    /// (GameState.hpp:64-75) — the engine keyboard binds
    /// (HexagonGame.cpp:349-460, registered in `WindowedClient::build`).
    input_manager: Rc<RefCell<Manager>>,
}

impl GameState for GameSlotAdapter {
    fn update_from_timer(&self, ft: f32) {
        // C++ `fnGoToMenu` (main.cpp:456-462) — consumed at the START of
        // the tick (the C++ switch lands in the same inter-tick gap).
        match self.return_to_menu.load(Ordering::SeqCst) {
            0 => {}
            v => {
                self.return_to_menu.store(0, Ordering::SeqCst);
                let error = v == 2;
                // C++ `MenuGame::init`'s Steam lines (MenuGame.cpp:793-794)
                // — `oh-menu` is extend-only, so the composition makes
                // them at the same lifecycle point (issue 020).
                self.steam.set_rich_presence_in_menu();
                self.steam.update_hardcoded_achievements();
                // C++ main.cpp:458-461.
                self.menu.return_to_level_selection();
                self.menu.init(error);
                // C++ mg->init's Discord lines (:796).
                if let Some(d) = &self.discord {
                    d.set_rich_presence_in_menu();
                }
                if let Some(engine) = self.engine_slot.borrow().clone() {
                    engine.set_game_state(GameSlot::Menu);
                }
            }
        }
        // C++ `HexagonGame.cpp:328-330` (`game.onUpdate += [this](mFT)
        // { update(mFT, 1.f); }`).
        self.game.update(ft, 1.0);
    }

    fn on_post_update(&self) {
        // C++ `HexagonGame.cpp:331` (`game.onPostUpdate += [this]
        // { postUpdate(); }`).
        self.game.post_update();
    }

    fn draw(&self, target: &mut dyn DrawTarget) {
        // C++ `HexagonGame.cpp:333` (`game.onDraw += [this] { draw(); }`)
        // — the D-020 bridge (`bridge::draw`).
        crate::bridge::draw(&self.game, &mut self.app_draw.borrow_mut(), target);
    }

    fn refresh_input(&self, input: &InputState) {
        // C++ `GameState::refreshInput` (GameState.hpp:54-57 →
        // `inputManager.refresh`, Manager.hpp:46-58).
        self.input_manager.borrow_mut().refresh(input);
    }

    fn update_input(&self, input: &InputState, ft: f32) {
        // C++ `GameState::updateInput` (GameState.hpp:49-52 →
        // `inputManager.update`, Manager.hpp:32-45).
        self.input_manager.borrow_mut().update(input, ft);
    }
}

/// issue 025f / D-031 — the soft-input (the system IME, the on-screen
/// keyboard) controller: `show` on the menu's ENTER-TEXT entry, `hide`
/// on its leave (the C++ `isEnteringText` gate's transition — the
/// `MenuSlotAdapter::refresh_input` docs: the edge is computed there,
/// once per transition, not per frame).
///
/// The C++'s Android text input came from SFML's own Android layer (the
/// IME wiring lived in SFML, D-031) — the port's deliberate platform
/// addition is this seam: the Android controller (the android-activity
/// `show_soft_input(true)` / `hide_soft_input(false)`, the
/// `oh-app::android` `ImeController`) and the desktop no-op (no IME on
/// the desktop window — the C++ desktop build never asked for one).
pub trait SoftInput: Send + Sync {
    /// Show the system IME (the android-activity `show_soft_input(true)`
    /// — `InputMethodManager.showSoftInput(decorView, SHOW_IMPLICIT)`).
    fn show(&self);
    /// Hide the system IME (the android-activity `hide_soft_input(false)`
    /// — `hideSoftInputFromWindow(token, 0)`: an unconditional hide, not
    /// only the implicitly shown IME).
    fn hide(&self);
}

/// The desktop / headless `SoftInput` (the trait's docs): the no-op —
/// no IME on the desktop window.
pub struct NoOpSoftInput;

impl SoftInput for NoOpSoftInput {
    fn show(&self) {}
    fn hide(&self) {}
}

/// The C++ `mg->getGame()` `GameState` (main.cpp:461/479/505/532): the
/// menu's `update(mFT)` (MenuGame.cpp:257) + `draw()` (MenuGame.cpp:259 —
/// the issue-013b bridge, `crate::menu_draw::draw`).
struct MenuSlotAdapter {
    menu: Rc<MenuGame>,
    /// The C++ `MenuGame::game` member's `ssvs::Input::Manager` — the menu
    /// keyboard binds (MenuGame.cpp:912-1018, registered in
    /// `WindowedClient::build`).
    input_manager: Rc<RefCell<Manager>>,
    /// The bridge's render-side state (the C++'s `MenuGame` draw members —
    /// fonts / sprites / quads; the C++'s `draw` mutates its members, so
    /// the bridge needs a `RefCell`, as the game state's `AppDraw`).
    menu_draw: RefCell<crate::menu_draw::MenuDraw>,
    /// The window's release-edge sink (the `oh_render::window::
    /// ReleasedEdges` docs — the C++ `checkCloseBootScreens`'s
    /// event-driven seam; the `refresh_input` docs, below).
    released_edges: std::sync::Arc<std::sync::RwLock<oh_render::window::ReleasedEdges>>,
    /// The window's entered-text sink (the C++ `runEvents`'s `TextEntered`
    /// dispatch — the `MenuGame::handle_text_entered` docs; the
    /// `refresh_input` docs, below).
    entered_text: std::sync::Arc<std::sync::RwLock<Vec<char>>>,
    /// issue 025f (D-031) — the soft-input (system IME) controller (the
    /// `SoftInput` docs): the `refresh_input`'s transition edge shows it
    /// on entering the text-entry states and hides it on leaving (the C++
    /// `isEnteringText` gate's port — `oh-menu` `States::is_entering_text`).
    soft_input: std::sync::Arc<dyn SoftInput>,
    /// issue 025f (D-031) — the previous frame's `is_entering_text` (the
    /// `was` latch of the transition edge the `refresh_input` tail
    /// computes — the `refresh_input` docs, below).
    was_entering_text: Cell<bool>,
}

impl GameState for MenuSlotAdapter {
    fn update_from_timer(&self, ft: f32) {
        // C++ MenuGame.cpp:257 (`game.onUpdate += [this](mFT) {
        // update(mFT); }`).
        self.menu.update(ft);
    }

    fn draw(&self, target: &mut dyn DrawTarget) {
        // C++ MenuGame.cpp:259 (`game.onDraw += [this] { draw(); }`) —
        // the full C++ `MenuGame::draw` surface (issue-013b).
        crate::menu_draw::draw(&self.menu, &mut self.menu_draw.borrow_mut(), target);
    }

    fn refresh_input(&self, input: &InputState) {
        // C++ `checkCloseBootScreens` (MenuGame.cpp:360-380) via the
        // `onEvent` handlers (MenuGame.cpp:518-537 `KeyReleased`,
        // :617-637 `MouseButtonReleased`, :674-694
        // `JoystickButtonReleased`): any key / mouse / joypad RELEASE
        // advances the boot screen — gated on `ignoreInputs != 0` (the
        // boot screens hold it, `setIgnoreAllInputs(1)` at
        // MenuGame.cpp:357) + the `LoadingScreen` / `EpilepsyWarning`
        // state. The C++ fires it PER RELEASE EVENT during `runEvents`
        // (before the frame's update); the port's equivalent fires here —
        // `refresh_input` is the once-per-frame hook the frame body runs
        // before the update burst — over the window's RELEASE-EDGE SINK
        // (`oh_render::window::ReleasedEdges`): one entry per release
        // event the fold applied. The sink is essential (not just more
        // robust): a fast hardware key tap — press + release inside one
        // frame, the `adb input keyevent` shape — leaves no trace in the
        // per-frame `InputState` snapshot (both bits land between two
        // `get_input_state` reads), so a snapshot diff would miss it
        // (the issue-025c on-device finding: `input keyevent 66` never
        // advanced the LoadingScreen). The Android touch seam: SFML's
        // window layer converts a touch release into a mouse event (the
        // C++'s `MouseButtonReleased`), so a finger release is the same
        // event. One advance per frame: the C++'s per-event double-fire
        // on two releases between two polls is an edge case the game's
        // boot flow never exercises (the first release advances; the
        // boot screen then re-arms its own gate). Beyond the boot
        // screens, every release runs the C++ handlers' shared tail —
        // the `--ignoreInputs` decrement (`handle_release_edge`, issue
        // 025g; the `refresh_input` drain's per-edge loop).
        // (The issue-025c on-device finding: the `MenuGame::
        // advance_boot_screen` port existed but had no app-layer call
        // site — the boot screens were uncloseable on the windowed
        // path.)
        // C++ `runEvents`'s `TextEntered` dispatch (GameWindow.hpp:42-83
        // — the handler fires PER EVENT, before the frame's update;
        // `refresh_input` is the once-per-frame hook the frame body runs
        // before the update burst) over the window's ENTERED-TEXT SINK
        // (the `oh_render::window` `entered_text_sink` docs): one char
        // per `TextEntered` the fold applied (the winit `KeyboardInput`
        // `text` field — the C++'s SFML `TextEntered` analogue). The
        // `MenuGame::handle_text_entered` handler runs the C++'s
        // `unicode < 128` filter (MenuGame.cpp:491-498); the
        // `update()`'s `enteredChars` → `enteredStr` accumulation
        // (`process_entered_chars`) then consumes them per frame.
        {
            let mut chars = self
                .entered_text
                .write()
                .expect("entered-text sink poisoned");
            if !chars.is_empty() {
                for c in chars.drain(..) {
                    self.menu.handle_text_entered(c);
                }
            }
        }
        if let Some(edges) = self
            .released_edges
            .write()
            .expect("released sink poisoned")
            .take()
        {
            // C++ `KeyReleased` / `MouseButtonReleased` /
            // `JoystickButtonReleased` per-release dispatch
            // (MenuGame.cpp:495-700): each release event runs the
            // handler — scenario 1 (a boot screen, the C++
            // `checkCloseBootScreens`), then the shared tail (the dialog
            // delay / box scenarios + the generic `--ignoreInputs`
            // decrement, the port's `MenuGame::handle_release_edge` —
            // issue 025g: the decrement is what unblocks the menu's
            // key binds after `returnToLevelSelection`'s
            // `setIgnoreAllInputs(1)` on `fnGoToMenu`). The port's
            // documented once-per-frame boot advance holds (the
            // 025c/025f note above): at most ONE `advance_boot_screen`
            // per frame — two boot releases in one frame (the C++'s
            // per-event double-fire) is the documented edge case the
            // boot flow never exercises.
            let mut boot_advanced = false;
            let n_edges = edges.keys.len() + edges.buttons.len() + edges.fingers.len();
            for _ in 0..n_edges {
                if self.menu.shared().borrow().ignore_inputs == 0 {
                    continue;
                }
                match self.menu.state() {
                    States::LoadingScreen | States::EpilepsyWarning if !boot_advanced => {
                        self.menu.advance_boot_screen();
                        boot_advanced = true;
                    }
                    _ => self.menu.handle_release_edge(),
                }
            }
        }
        self.input_manager.borrow_mut().refresh(input);
        // issue 025f (D-031) — the soft-input (system IME) transition
        // edge: the menu's `is_entering_text` (the C++ `isEnteringText`,
        // `oh-menu` `States::is_entering_text` — `ETLPNewBoot` /
        // `ETLPNew`) changed since the last frame's `refresh_input`
        // (the boot-screen advance above can flip it WITHIN this frame,
        // so the edge is read AFTER the released-edges drain) → show the
        // IME on the enter edge, hide it on the leave edge — ONCE per
        // transition (the `was` latch), never per frame. The
        // android-activity calls ride the controller (the `SoftInput`
        // docs); the desktop is the no-op (module docs: D-031).
        let entering = self.menu.is_entering_text();
        if entering != self.was_entering_text.get() {
            self.was_entering_text.set(entering);
            if entering {
                self.soft_input.show();
            } else {
                self.soft_input.hide();
            }
        }
    }

    fn update_input(&self, input: &InputState, ft: f32) {
        self.input_manager.borrow_mut().update(input, ft);
    }

    // C++ `MenuGame` registers no `onPostUpdate` (only `onUpdate` /
    // `onDraw`, MenuGame.cpp:257-259) — the trait default (empty delegate).
}

/// The inverse of the C++ `static_cast<int>(Tid)` (HexagonGame.cpp:355,
/// MenuGame.cpp:919) — the 16 keyboard bind ids 0..=15 (`Tid` is
/// `#[repr(i32)]`, `TriggersCount = 16` excluded).
fn tid_from_i32(v: i32) -> Option<Tid> {
    match v {
        0 => Some(Tid::RotateCCW),
        1 => Some(Tid::RotateCW),
        2 => Some(Tid::Focus),
        3 => Some(Tid::Select),
        4 => Some(Tid::Exit),
        5 => Some(Tid::ForceRestart),
        6 => Some(Tid::Restart),
        7 => Some(Tid::Replay),
        8 => Some(Tid::Screenshot),
        9 => Some(Tid::Swap),
        10 => Some(Tid::Up),
        11 => Some(Tid::Down),
        12 => Some(Tid::NextPack),
        13 => Some(Tid::PreviousPack),
        14 => Some(Tid::LuaConsole),
        15 => Some(Tid::Pause),
        _ => None,
    }
}

/// C++ `HexagonGame`'s keyboard bind registrations (HexagonGame.cpp:349-460)
/// — into the GAME state's manager (the C++ registers into its own
/// `ssvs::GameState game` member; the port's `GameSlotAdapter` owns the
/// manager). `trigger_of` reads the LIVE config triggers (`Config::getTrigger`,
/// the C++'s `Config::getTrigger(tid)` at every registration —
/// HexagonGame.cpp:352-356), so the rebind / `refreshBinds` paths
/// re-point the binds without re-registration.
fn register_engine_binds(
    mgr: &mut Manager,
    game: &Rc<HexagonGame<GameApiState>>,
    config: &Arc<RwLock<Config>>,
) {
    let trigger_of = |tid: Tid| config.read().expect("config poisoned").trigger_copy(tid);

    // `add2StateInput(game, getTrigger(Tid::X), inputImplX)`
    // (HexagonGame.cpp:357-360; SSVStart `Utils/Input.hpp:209-219`):
    // `Type::Always` on/off closures over the impl flags.
    let mut two_state = |tid: Tid, set: fn(&HexagonGame<GameApiState>, bool)| {
        let g_on = game.clone();
        let g_off = game.clone();
        mgr.add_input(
            trigger_of(tid),
            move |_ft| set(&g_on, true),
            move |_ft| set(&g_off, false),
            Type::Always,
            tid as i32,
            Mode::Overlap,
        );
    };
    two_state(
        Tid::RotateCCW,
        HexagonGame::<GameApiState>::set_input_impl_ccw,
    );
    two_state(
        Tid::RotateCW,
        HexagonGame::<GameApiState>::set_input_impl_cw,
    );
    two_state(Tid::Focus, HexagonGame::<GameApiState>::set_input_focused);
    two_state(Tid::Swap, HexagonGame::<GameApiState>::set_input_swap);

    // The `notInConsole` wrapper (HexagonGame.cpp:367-375): the action runs
    // only when `imguiLuaConsoleHasInput()` is false.
    let not_in_console = |g: Rc<HexagonGame<GameApiState>>,
                          action: fn(&HexagonGame<GameApiState>)| {
        move |_ft| {
            if !g.lua_console_has_input() {
                action(&g);
            }
        }
    };

    // Hardcoded Escape (HexagonGame.cpp:377-381): `goToMenu()` — the
    // default-argument call `goToMenu(true, false)`
    // (HexagonGame.hpp:417). C++ default `mTriggerID = -1`
    // (GameState.hpp:70).
    {
        let g = game.clone();
        mgr.add_input(
            Trigger::key(Key::ESCAPE),
            not_in_console(g, HexagonGame::<GameApiState>::trigger_go_to_menu),
            |_| {},
            Type::Always,
            -1,
            Mode::Overlap,
        );
    }
    {
        let g = game.clone();
        mgr.add_input(
            trigger_of(Tid::Exit),
            not_in_console(g, HexagonGame::<GameApiState>::trigger_go_to_menu),
            |_| {},
            Type::Always,
            Tid::Exit as i32,
            Mode::Overlap,
        );
    }
    {
        let g = game.clone();
        mgr.add_input(
            trigger_of(Tid::ForceRestart),
            not_in_console(g, HexagonGame::<GameApiState>::trigger_force_restart),
            |_| {},
            Type::Once,
            Tid::ForceRestart as i32,
            Mode::Overlap,
        );
    }
    {
        let g = game.clone();
        mgr.add_input(
            trigger_of(Tid::Restart),
            not_in_console(g, HexagonGame::<GameApiState>::trigger_restart),
            |_| {},
            Type::Once,
            Tid::Restart as i32,
            Mode::Overlap,
        );
    }
    {
        let g = game.clone();
        mgr.add_input(
            trigger_of(Tid::Replay),
            not_in_console(g, HexagonGame::<GameApiState>::trigger_replay),
            |_| {},
            Type::Once,
            Tid::Replay as i32,
            Mode::Overlap,
        );
    }
    {
        let g = game.clone();
        mgr.add_input(
            trigger_of(Tid::Screenshot),
            not_in_console(g, HexagonGame::<GameApiState>::trigger_screenshot),
            |_| {},
            Type::Once,
            Tid::Screenshot as i32,
            Mode::Overlap,
        );
    }
    // Tid::LuaConsole (HexagonGame.cpp:412-419) — NOT console-gated (the
    // debug gate is the body's, `set_ilc_show_console_next`).
    {
        let g = game.clone();
        mgr.add_input(
            trigger_of(Tid::LuaConsole),
            move |_ft| g.trigger_lua_console(),
            |_| {},
            Type::Once,
            Tid::LuaConsole as i32,
            Mode::Overlap,
        );
    }
    // Tid::Pause (HexagonGame.cpp:421-441) — NOT console-gated either.
    {
        let g = game.clone();
        mgr.add_input(
            trigger_of(Tid::Pause),
            move |_ft| g.trigger_pause(),
            |_| {},
            Type::Once,
            Tid::Pause as i32,
            Mode::Overlap,
        );
    }
}

/// C++ `MenuGame::initInput()` (MenuGame.cpp:912-1018) — the menu keyboard
/// binds, into the MENU state's manager (the C++ registers into
/// `MenuGame::game`). Same live-`Config::getTrigger` reading as the engine
/// registration (MenuGame.cpp:919-921).
fn register_menu_binds(mgr: &mut Manager, menu: &Rc<MenuGame>, config: &Arc<RwLock<Config>>) {
    let trigger_of = |tid: Tid| config.read().expect("config poisoned").trigger_copy(tid);

    // C++ `if(!mouseHovering) leftAction()/rightAction()` (MenuGame.cpp:
    // 923-938): the port's mouse-hover surface is issue 13b's menu-draw
    // lane (the `mouseHovering` state is unported — no menu item tracking
    // exists yet); the actions run unguarded.
    {
        let m = menu.clone();
        mgr.add_input(
            trigger_of(Tid::RotateCCW),
            move |_ft| m.left_action(),
            |_| {},
            Type::Once,
            Tid::RotateCCW as i32,
            Mode::Overlap,
        );
    }
    {
        let m = menu.clone();
        mgr.add_input(
            trigger_of(Tid::RotateCW),
            move |_ft| m.right_action(),
            |_| {},
            Type::Once,
            Tid::RotateCW as i32,
            Mode::Overlap,
        );
    }
    // Hardcoded Up/Down (MenuGame.cpp:940-957) + the editable Tid::Up/Down.
    {
        let m = menu.clone();
        mgr.add_input(
            Trigger::key(Key::UP),
            move |_ft| m.up_action(),
            |_| {},
            Type::Once,
            -1,
            Mode::Overlap,
        );
    }
    {
        let m = menu.clone();
        mgr.add_input(
            trigger_of(Tid::Up),
            move |_ft| m.up_action(),
            |_| {},
            Type::Once,
            Tid::Up as i32,
            Mode::Overlap,
        );
    }
    {
        let m = menu.clone();
        mgr.add_input(
            Trigger::key(Key::DOWN),
            move |_ft| m.down_action(),
            |_| {},
            Type::Once,
            -1,
            Mode::Overlap,
        );
    }
    {
        let m = menu.clone();
        mgr.add_input(
            trigger_of(Tid::Down),
            move |_ft| m.down_action(),
            |_| {},
            Type::Once,
            Tid::Down as i32,
            Mode::Overlap,
        );
    }
    {
        let m = menu.clone();
        mgr.add_input(
            trigger_of(Tid::NextPack),
            move |_ft| m.change_pack_action(1),
            |_| {},
            Type::Once,
            Tid::NextPack as i32,
            Mode::Overlap,
        );
    }
    {
        let m = menu.clone();
        mgr.add_input(
            trigger_of(Tid::PreviousPack),
            move |_ft| m.change_pack_action(-1),
            |_| {},
            Type::Once,
            Tid::PreviousPack as i32,
            Mode::Overlap,
        );
    }
    // `add2StateInput(game, getTrigger(Tid::Focus), focusHeld, ...)`
    // (MenuGame.cpp:956-957 + SSVStart `Utils/Input.hpp:209-219`).
    {
        let m_on = menu.clone();
        let m_off = menu.clone();
        mgr.add_input(
            trigger_of(Tid::Focus),
            move |_ft| m_on.set_focus_held(true),
            move |_ft| m_off.set_focus_held(false),
            Type::Always,
            Tid::Focus as i32,
            Mode::Overlap,
        );
    }
    // Hardcoded Enter (MenuGame.cpp:959-961).
    {
        let m = menu.clone();
        mgr.add_input(
            Trigger::key(Key::ENTER),
            move |_ft| m.ok_action(),
            |_| {},
            Type::Once,
            -1,
            Mode::Overlap,
        );
    }
    // Hardcoded Escape `Type::Always` (MenuGame.cpp:963-971): the
    // `exitTimer` accumulation; the `exitTimer > 20 → window.stop()`
    // consumer (MenuGame.cpp:2852) is the unported `MenuGame::draw` lane
    // (issue 13b) — the timer accumulates only.
    {
        let m_on = menu.clone();
        let m_off = menu.clone();
        mgr.add_input(
            Trigger::key(Key::ESCAPE),
            move |ft| m_on.add_exit_timer(ft),
            move |_ft| m_off.reset_exit_timer(),
            Type::Always,
            -1,
            Mode::Overlap,
        );
    }
    // Hardcoded Escape `Type::Once` (MenuGame.cpp:973-975).
    {
        let m = menu.clone();
        mgr.add_input(
            Trigger::key(Key::ESCAPE),
            move |_ft| m.exit_action(),
            |_| {},
            Type::Once,
            -1,
            Mode::Overlap,
        );
    }
    // Editable Tid::Exit (MenuGame.cpp:989-997): the `isEnteringText()`
    // guard + `exitAction()`.
    {
        let m = menu.clone();
        mgr.add_input(
            trigger_of(Tid::Exit),
            move |_ft| {
                if !m.is_entering_text() {
                    m.exit_action();
                }
            },
            |_| {},
            Type::Once,
            Tid::Exit as i32,
            Mode::Overlap,
        );
    }
    // Editable Tid::Screenshot (MenuGame.cpp:999-1001):
    // `mustTakeScreenshot = true` — the C++ consumer is `MenuGame::draw`
    // (issue 13b, unported).
    mgr.add_input(
        trigger_of(Tid::Screenshot),
        move |_ft| {
            // TODO(010b-menu-screenshot): C++ `mustTakeScreenshot = true`
            // (MenuGame.cpp:1000) — the menu's screenshot consumer is the
            // unported `MenuGame::draw` (issue 13b).
        },
        |_| {},
        Type::Once,
        Tid::Screenshot as i32,
        Mode::Overlap,
    );
    // Hardcoded LAlt+Enter (MenuGame.cpp:1003-1013): the fullscreen toggle
    // (`Config::setFullscreen(window, !getFullscreen())` — the port's
    // `MenuConfig::set_fullscreen` seam) + `game.ignoreNextInputs()` +
    // `.setPriorityUser(-1000)`.
    {
        let cfg = menu.config().clone();
        let idx = mgr.add_input(
            Trigger::keys(&[Key::L_ALT, Key::ENTER]),
            move |_ft| cfg.set_fullscreen(!cfg.fullscreen()),
            |_| {},
            Type::Once,
            -1,
            Mode::Overlap,
        );
        // C++ `.setPriorityUser(-1000)` (MenuGame.cpp:1012): an `int` into
        // a `std::size_t` wraps to `usize::MAX - 999`.
        mgr.set_priority_user(idx, (usize::MAX - 999) as u64);
        // C++ `game.ignoreNextInputs()` (MenuGame.cpp:1008): after this
        // action, the rest of the frame's binds are skipped
        // (Manager.hpp:40-43).
        mgr.set_ignore_next_on_fire(idx);
    }
    // Hardcoded Backspace (MenuGame.cpp:1015-1017).
    {
        let m = menu.clone();
        mgr.add_input(
            Trigger::key(Key::BACKSPACE),
            move |_ft| m.erase_action(),
            |_| {},
            Type::Once,
            -1,
            Mode::Overlap,
        );
    }
    // Hardcoded F1 (MenuGame.cpp:1019-1021): `addRemoveFavoriteLevel()` —
    // the favorites-drawer half is the unported 13b detail.
    mgr.add_input(
        Trigger::key(Key::F1),
        move |_ft| {
            // TODO(010b-menu-favorite): C++ `addRemoveFavoriteLevel()`
            // (MenuGame.cpp:1020) — the favorites toggle is the unported
            // issue 13b detail.
        },
        |_| {},
        Type::Once,
        -1,
        Mode::Overlap,
    );
    // Hardcoded F2 (MenuGame.cpp:1023-1025): `switchToFromFavoriteLevels()`.
    mgr.add_input(
        Trigger::key(Key::F2),
        move |_ft| {
            // TODO(010b-menu-favorites-toggle): C++
            // `switchToFromFavoriteLevels()` (MenuGame.cpp:1024) — the
            // favorites view switch is the unported issue 13b detail.
        },
        |_| {},
        Type::Once,
        -1,
        Mode::Overlap,
    );
    // Hardcoded F3/F4 (MenuGame.cpp:1027-1033): `reloadAssets(false/true)`
    // → the `MenuAssets::reload_assets` seam.
    {
        let m = menu.clone();
        mgr.add_input(
            Trigger::key(Key::F3),
            move |_ft| {
                let assets = m.assets().clone();
                let id = m.selected_level_id();
                assets.reload_assets(false, &id);
            },
            |_| {},
            Type::Once,
            -1,
            Mode::Overlap,
        );
    }
    {
        let m = menu.clone();
        mgr.add_input(
            Trigger::key(Key::F4),
            move |_ft| {
                let assets = m.assets().clone();
                let id = m.selected_level_id();
                assets.reload_assets(true, &id);
            },
            |_| {},
            Type::Once,
            -1,
            Mode::Overlap,
        );
    }
}

/// The C++ `mainClient` windowed client: the window + the `GameEngine`
/// state switcher + the 240 Hz timer + both states, in the C++'s
/// single-threaded scope (main.cpp:297-535).
///
/// Drop order is part of the `SharedLua` contract (compose.rs): the
/// states' `Rc<HexagonGame>` clones (held by the adapters, dropped with
/// `engine`) all drop BEFORE `composed` (the last field), which owns the
/// heap-pinned `LuaContext` every host closure points into.
#[allow(dead_code)] // `menu` / `game` are read by the tests + the loop exit
pub struct WindowedClient {
    pub window: Window,
    pub engine: Rc<GameEngine>,
    pub timer: TimerStatic,
    /// C++ `mg` (main.cpp:427-434).
    pub menu: Rc<MenuGame>,
    /// issue 025g — the ACTUAL-window-size slot (`RenderWindowSize`'s
    /// docs — the C++ `window.getRenderWindow().getSize()` live value the
    /// menu's `MenuWindow` seam reads; `tick_fields` refreshes it every
    /// frame from the window's current size).
    render_size: std::sync::Arc<RenderWindowSize>,
    /// C++ `hg` (main.cpp:414-421) — the composition's engine handle.
    pub game: Rc<HexagonGame<GameApiState>>,
    /// The shared process config (the C++'s `hg::Config` singleton —
    /// `build`'s parameter): the egui console footer's statics
    /// seed/write back into it (HGUpdate.cpp:1549, 1555-1557) and the
    /// `tick` / `run` frame bodies pass it to the console pass.
    pub config: Arc<RwLock<Config>>,
    /// Issue 014b — the egui console overlay's lazy slot (`None` until
    /// the first shown frame — `crate::lua_console_ui`'s module docs:
    /// Gates). Shared by the per-event hook (the C++'s
    /// `Imgui::processEvent` feed, HexagonGame.cpp:335) and the frame
    /// bodies (`tick_fields`'s post-update / draw halves).
    ilc: Rc<RefCell<Option<crate::lua_console_ui::IlcConsole>>>,
    /// C++ `steamManager` (main.cpp:251) — the composition's Steam manager
    /// (issue 020): shared with the engine (the `steam_manager` seam) and
    /// driving the menu's `MenuGame::init`-lifecycle calls
    /// (MenuGame.cpp:793-794 — `oh-menu` is extend-only, so the
    /// composition makes them at the same lifecycle point).
    pub steam: Arc<oh_steam::SteamManager>,
    /// C++ `discordManager` (main.cpp:264-269 — created only when NOT
    /// headless; the windowed path is always non-headless, so it is always
    /// `Some` here). The real IPC impl (`discord-rich-presence`, the
    /// contract's documented fallback for the absent official-SDK crate);
    /// it degrades gracefully when no local Discord client is running
    /// (`init` fails -> the manager is inert, the C++'s
    /// `Core::Create`-failure path).
    pub discord: Option<Arc<dyn oh_discord::Discord>>,
    /// The C++ post-construction state-switch slot (main.cpp:442-462):
    /// the `fnHGNewGame` closure and the `GameSlotAdapter`'s `fnGoToMenu`
    /// consumption both resolve `window->setGameState(...)` through it —
    /// it is filled with the engine right after the engine is built (the
    /// C++'s raw `window` pointer exists for the whole scope; the port
    /// fills the slot before any callback can fire — `boot` runs first).
    engine_slot: Rc<RefCell<Option<Rc<GameEngine>>>>,
    /// C++ `globalWindow`'s SIGINT stop (main.cpp:334-350) — the safe
    /// port's `Arc<AtomicBool>`: `install_sigint`'s `signal-hook` closure
    /// stores `true` on SIGINT (the ONLY signal-context work — the C++'s
    /// `TODO (P2): UB` fixed), and the loop head (`tick_fields`, the C++
    /// `while(isRunning())` position) performs the handler's actions
    /// around the deferred `globalWindow.stop()` (module docs: the SIGINT
    /// bullet).
    pub interrupt: Arc<AtomicBool>,
    // Held for drop order (the adapters live in `engine`; the `AppDraw`
    // holds the fonts — both must drop before `composed`).
    _app_draw: Rc<RefCell<AppDraw>>,
    composed: crate::compose::Composed,
}

impl WindowedClient {
    /// C++ `mainClient`'s windowed pieces up to (not including) the boot
    /// dispatch (main.cpp:297-463) — timer + engine + MenuGame + the four
    /// callback links. `window` is created by the caller (the C++ creates
    /// it in `mainClient`, main.cpp:297-332 — see [`create_window`]; tests
    /// pass a headless `Window` so no display / GL / event loop is
    /// touched — the spec's "real windowed run = MANUAL").
    pub fn build(
        mut window: Window,
        config: Arc<RwLock<Config>>,
        assets: Arc<oh_assets::Assets>,
        base_dir: &Path,
        steam: Arc<oh_steam::SteamManager>,
        discord: Option<Arc<dyn oh_discord::Discord>>,
        // issue 025f (D-031) — the soft-input (system IME) controller
        // (the `SoftInput` docs): the Android activity's
        // `show_soft_input` / `hide_soft_input` on the menu's text-entry
        // transitions; the desktop's no-op.
        soft_input: Arc<dyn SoftInput>,
    ) -> Result<Self, String> {
        // ------------------------------------------------------------------
        // C++ main.cpp:330-332 — `window->setTimer<ssvs::TimerStatic>(
        // hg::Config::TIME_STEP, hg::Config::TIME_SLICE);` ("240 ticks per
        // second"). The C++ template's `maxLoops` argument defaults to
        // 50.f (TimerStatic.hpp:24).
        //
        // The C++ SIGINT handler (main.cpp:334-350) is installed over the
        // client AFTER `build` (the C++'s placement — after the window +
        // timer, before the boot dispatch): `WindowedClient::install_sigint`
        // (`main_client` calls it); the flag it sets is created here.
        let timer = TimerStatic::new(TIME_STEP, TIME_SLICE, 50.0);

        // ------------------------------------------------------------------
        // C++ main.cpp:413-421 — the engine (`window` = true): the C++
        // passes `&steamManager` first (issue 020 — the composition's
        // shared manager); the profile editor surface is issue 17
        // (`NullProfiles`), the audio / discord null equivalents are the
        // composition's. The shared `Config` is the C++'s single
        // process-global `hg::Config` (menu writes reach the engine —
        // e.g. the debug toggle the `Tid::Pause` body reads,
        // HexagonGame.cpp:421-441).
        // issue 25b SG5: the engine's `Renderer` seam (the C++
        // `getFingerDownPositions` data path) — the shared touch sink
        // the window's touch events update (the windowed client owns
        // both sides; module note on `WindowTouchRenderer`). Desktop:
        // `WindowEvent::Touch` never arrives — the sink stays empty and
        // the engine sees the no-renderer behavior (behavior-identical).
        let touch_sink: std::sync::Arc<
            std::sync::RwLock<[Option<oh_math::ssvs::Vec2>; oh_render::input::FINGER_COUNT]>,
        > = std::sync::Arc::new(std::sync::RwLock::new(
            [None; oh_render::input::FINGER_COUNT],
        ));
        window.set_touch_position_sink(touch_sink.clone());
        // issue 025f: the menu's `MenuTouch` seam (the 9-zone touch grid,
        // MenuGame.cpp:2802-2835) — the SAME shared sink the engine's
        // `WindowTouchRenderer` below reads: the window's touch-event
        // fold's data (the window's CURRENT size coordinates — issue
        // 025g: the raw winit coordinates, the same space the grid's
        // thirds run in via the `MenuWindow` seam).
        let menu_touch: std::rc::Rc<dyn MenuTouch> = std::rc::Rc::new(MenuTouchSink {
            positions: touch_sink.clone(),
        });
        // issue 025c: the release-edge sink (the C++
        // `checkCloseBootScreens` event-driven seam — the
        // `oh_render::window::ReleasedEdges` docs): the window's event
        // fold appends one entry per key / button / finger RELEASE;
        // the menu state's `refresh_input` drains it (the boot screens
        // advance on any release while `ignore_inputs` holds). Desktop
        // and Android: the fold appends identically (the scripted-
        // input tests' key events flow through it).
        let released_sink: std::sync::Arc<std::sync::RwLock<oh_render::window::ReleasedEdges>> =
            std::sync::Arc::new(std::sync::RwLock::new(Default::default()));
        window.set_released_edges_sink(released_sink.clone());
        // issue 025d: the entered-text sink (the C++ `runEvents`
        // `TextEntered` dispatch's data path — the window's fold records
        // one char per `TextEntered`; the menu state's `refresh_input`
        // drain runs the C++ `MenuGame`'s `onEvent(TextEntered)` handler
        // (MenuGame.cpp:491-498) over it). Desktop and Android: the fold
        // records identically (the winit `KeyboardInput` `text` field).
        let entered_text_sink: std::sync::Arc<std::sync::RwLock<Vec<char>>> =
            std::sync::Arc::new(std::sync::RwLock::new(Vec::new()));
        window.set_entered_text_sink(entered_text_sink.clone());
        let renderer: Option<std::sync::Arc<dyn oh_game::renderer::Renderer + 'static>> =
            Some(std::sync::Arc::new(WindowTouchRenderer {
                positions: touch_sink,
            }));
        // C++ main.cpp:264-269 — `discordManager.emplace()` (only when NOT
        // headless; the windowed path is always non-headless).
        let discord = discord.unwrap_or_else(|| {
            Arc::new(oh_discord::DiscordManager::new(
                oh_discord::IpcDiscordCore::new(oh_discord::DISCORD_APP_ID),
            )) as Arc<dyn oh_discord::Discord>
        });

        // C++ main.cpp:392-401 — `hg::Audio audio{ soundBufferGetter,
        // musicPathGetter }` (both getters capture `&assets`), then
        // `audio.setSoundVolume(Config::getSoundVolume());
        // audio.setMusicVolume(Config::getMusicVolume());`. The REAL audio is
        // shared with the engine (`compose_with_config` below) and the menu
        // (`MenuAudioSink`, the C++ `mg.emplace(..., audio, ...)` at
        // main.cpp:434). issue 025e: this replaces the previous `None`
        // engine-audio / `NoopAudio` menu-sink (the missed subsystem).
        let audio = Arc::new(oh_audio::Audio::new(assets.clone()));
        {
            let c = config.read().expect("config poisoned");
            audio.set_sound_volume(c.get_sound_volume());
            audio.set_music_volume(c.get_music_volume());
        }

        let composed = compose_with_config(
            assets.clone(),
            base_dir.to_path_buf(),
            true, /* window */
            Arc::new(NullProfiles),
            config.clone(),
            steam.clone(),
            Some(discord.clone()),
            renderer,
            Some(audio.clone()),
        );

        // The C++ `initText` fonts (HexagonGame.cpp:289-290:
        // `Assets/OpenSquare-Regular.ttf` / `OpenSquare-Bold.ttf`).
        let assets_dir = base_dir.join("Assets");
        let font = Font::load(&assets_dir.join("OpenSquare-Regular.ttf"))
            .map_err(|e| format!("failed to load Assets/OpenSquare-Regular.ttf: {e}"))?;
        let font_bold = Font::load(&assets_dir.join("OpenSquare-Bold.ttf"))
            .map_err(|e| format!("failed to load Assets/OpenSquare-Bold.ttf: {e}"))?;
        let app_draw = Rc::new(RefCell::new(AppDraw::new(
            &config.read().expect("config poisoned"),
            font.clone(),
            font_bold.clone(),
        )));
        // The menu draw bridge's fonts (issue-013b) — the C++ shares the
        // `initText` fonts with the game (HexagonGame.cpp:289-290).
        let menu_draw = RefCell::new(crate::menu_draw::MenuDraw::new(&font, &font_bold));

        // ------------------------------------------------------------------
        // C++ main.cpp:427-434 — the MenuGame (the Steam / Discord /
        // HexagonClient arguments are their null equivalents: issues
        // 20/21/17; `NoopNetwork` / `NoopAudio` / the real `Assets` via
        // `RealMenuAssets` — one `_RELEASE` load shared with the engine,
        // as the C++ shares one `HGAssets`).
        let game = Rc::new(composed.game.clone());
        let engine_slot: Rc<RefCell<Option<Rc<GameEngine>>>> = Rc::new(RefCell::new(None));

        // ------------------------------------------------------------------
        // C++ `HexagonGame::game` / `MenuGame::game` — the two
        // `ssvs::Input::Manager`s (GameState.hpp:64-75) and the keyboard
        // binds (issue 010b): the engine's (HexagonGame.cpp:349-460, below)
        // and the menu's (MenuGame.cpp:912-1018, registered after the menu
        // object exists). The C++ registers them in the objects'
        // constructors; the port registers them here, where the windowed
        // composition owns both states.
        let game_manager: Rc<RefCell<Manager>> = Rc::new(RefCell::new(Manager::new()));
        let menu_manager: Rc<RefCell<Manager>> = Rc::new(RefCell::new(Manager::new()));
        {
            let mut gm = game_manager.borrow_mut();
            register_engine_binds(&mut gm, &game, &config);
        }

        // C++ main.cpp:442-449 — `fnHGNewGame`: `hg.newGame(...)` +
        // `window->setGameState(hg.getGame())`.
        let new_game_cb: Option<NewGameCallback> = {
            let g = game.clone();
            let slot = engine_slot.clone();
            Some(Box::new(
                move |pack_id, level_id, first_play, diff_mult, execute_last_replay| {
                    g.new_game(
                        pack_id,
                        level_id,
                        first_play,
                        diff_mult,
                        execute_last_replay,
                    );
                    if let Some(e) = slot.borrow().clone() {
                        e.set_game_state(GameSlot::Game);
                    }
                },
            ))
        };

        // C++ main.cpp:436-440 — `fnHGTriggerRefresh` (`hg.refreshTrigger`
        // → `HexagonGame::refreshTrigger`, HexagonGame.cpp:467-470) + the
        // menu-side `game.refreshTrigger` half of the rebind callback
        // (MenuGame.cpp:1233-1240). The ported seam passes (name, id) in
        // place of the trigger value (menu.rs:263) — the value is re-read
        // from the shared `Config` (the same object the menu's
        // `KeyboardBindControl` rebinds, oh-config `rebind_trigger`).
        let trigger_refresh_cb: Option<TriggerRefreshCallback> = {
            let config = config.clone();
            let menu_manager = menu_manager.clone();
            let game_manager = game_manager.clone();
            Some(Box::new(move |_name, bind_id| {
                let Some(tid) = tid_from_i32(bind_id) else {
                    return;
                };
                let trigger = config.read().expect("config poisoned").trigger_copy(tid);
                menu_manager
                    .borrow_mut()
                    .refresh_triggers(&trigger, bind_id);
                game_manager
                    .borrow_mut()
                    .refresh_triggers(&trigger, bind_id);
            }))
        };

        // C++ main.cpp:451-454 — `fnHGUpdateRichPresenceCallbacks`:
        // `hg.updateRichPresenceCallbacks()` (main.cpp:453) — the Steam
        // half is the engine's `steam_manager` seam (the hung counter
        // lives in the manager, issue 020); the Discord half is issue 21
        // (absent).
        let rich_presence_cb: Option<RichPresenceCallback> = {
            let steam = steam.clone();
            Some(Box::new(move || {
                steam.update_rich_presence_callbacks();
            }))
        };

        // C++ `MenuGame::refreshBinds()`' input-manager half (the options'
        // "RESET CONFIG" / "reset binds" items, menu.rs:145,219 →
        // MenuGame.cpp:1175,1213 → :3373-3387) — filled AFTER the menu +
        // managers exist (the slot breaks the menu↔adapter cycle).
        let refresh_binds_slot: RefreshBindsSlot = Rc::new(RefCell::new(None));

        // issue 025g: the ACTUAL-window-size slot (the C++
        // `window.getRenderWindow().getSize()` live value — the menu's
        // `MenuWindow` seam source; `tick_fields` refreshes it every frame
        // from the window's current size — the `RenderWindowSize` docs).
        let render_size = std::sync::Arc::new(RenderWindowSize::new(
            window.get_width(),
            window.get_height(),
        ));

        let menu = Rc::new(MenuGame::new(
            Rc::new(RealMenuAssets::from_arc(assets)),
            Rc::new(ConfigMenuAdapter::new(
                config.clone(),
                refresh_binds_slot.clone(),
            )),
            Rc::new(NoopNetwork),
            Rc::new(MenuAudioSink {
                audio: audio.clone(),
                config: config.clone(),
            }),
            menu_touch,
            Rc::new(MenuWindowSource {
                size: render_size.clone(),
            }),
            new_game_cb,
            trigger_refresh_cb,
            rich_presence_cb,
            {
                let menu_manager = menu_manager.clone();
                Some(Box::new(move |ignore| {
                    // C++ `setIgnoreAllInputs`' `game.ignoreAllInputs` half
                    // (MenuGame.cpp:3389-3402) → the menu state's manager.
                    menu_manager.borrow_mut().ignore_all_inputs(ignore);
                }))
            },
            Rc::new(LinearFontHeight),
        ));

        // C++ `MenuGame::initInput()` (MenuGame.cpp:912-1018) — the menu
        // keyboard binds, into the menu state's manager.
        {
            register_menu_binds(&mut menu_manager.borrow_mut(), &menu, &config);
        }

        // Fill the `refreshBinds` slot (both managers, every trigger —
        // C++ `MenuGame::refreshBinds`, MenuGame.cpp:3373-3387; the
        // `Config::loadAllJoystickBinds()` tail is the unported joystick
        // — documented no-op).
        {
            let config = config.clone();
            let menu_manager = menu_manager.clone();
            let game_manager = game_manager.clone();
            *refresh_binds_slot.borrow_mut() = Some(Box::new(move || {
                for i in 0..=15_i32 {
                    let Some(tid) = tid_from_i32(i) else {
                        continue;
                    };
                    let trigger = config.read().expect("config poisoned").trigger_copy(tid);
                    menu_manager.borrow_mut().refresh_triggers(&trigger, i);
                    game_manager.borrow_mut().refresh_triggers(&trigger, i);
                }
            }));
        }

        // ------------------------------------------------------------------
        // The two `GameState` adapters + the `GameEngine` switcher
        // (main.cpp:448/461/479/502: `window->setGameState` — the
        // `-p -l` path sets NO state in the C++; the port's documented
        // deviation, module docs).
        let return_to_menu = Arc::new(AtomicU8::new(0));
        let menu_adapter: Rc<dyn GameState> = Rc::new(MenuSlotAdapter {
            menu: menu.clone(),
            input_manager: menu_manager.clone(),
            menu_draw,
            released_edges: released_sink.clone(),
            entered_text: entered_text_sink.clone(),
            soft_input,
            // The first menu state is the boot `LoadingScreen` (not a
            // text-entry state — `is_entering_text` is false), so the
            // edge latch starts there.
            was_entering_text: Cell::new(false),
        });
        let game_adapter: Rc<dyn GameState> = Rc::new(GameSlotAdapter {
            game: game.clone(),
            menu: menu.clone(),
            app_draw: app_draw.clone(),
            steam: steam.clone(),
            discord: Some(discord.clone()),
            return_to_menu: return_to_menu.clone(),
            engine_slot: engine_slot.clone(),
            input_manager: game_manager.clone(),
        });
        let engine = Rc::new(GameEngine::new(menu_adapter, game_adapter));
        // The C++'s `window` pointer exists before the callbacks are wired
        // (main.cpp:297 vs :436-462); the port fills the slot the moment
        // the engine exists — before `boot` / `run` / any callback can
        // fire (the callbacks are only invoked by the loop / the engine).
        *engine_slot.borrow_mut() = Some(engine.clone());

        // C++ main.cpp:456-462 — `hg.fnGoToMenu`: the engine's callback
        // must be `Send` (module docs: fnGoToMenu timing), so it only
        // signals; the `GameSlotAdapter` performs the C++'s three calls.
        {
            let flag = return_to_menu.clone();
            game.set_fn_go_to_menu(Arc::new(move |error| {
                flag.store(if error { 2 } else { 1 }, Ordering::SeqCst);
            }));
        }

        Ok(Self {
            window,
            engine: engine.clone(),
            timer,
            menu,
            render_size,
            game,
            config,
            steam,
            discord: Some(discord),
            engine_slot,
            interrupt: Arc::new(AtomicBool::new(false)),
            _app_draw: app_draw,
            composed,
            ilc: Rc::new(RefCell::new(None)),
        })
    }

    /// C++ `MenuGame::init`'s `discordManager.set_rich_presence_in_menu()`
    /// (MenuGame.cpp:796) — the one-shot "Selecting Level" activity. oh-menu
    /// is not edited (issue 21's hard exclusion), so the app layer fires it
    /// at each menu-init site — the C++ `mg->init` calls: boot `gotoMenu`
    /// (main.cpp:480), the `-p -l` boot (main.cpp:510), and the
    /// `fnGoToMenu` consumption (main.cpp:458-461).
    fn menu_rich_presence(&self) {
        if let Some(d) = &self.discord {
            d.set_rich_presence_in_menu();
        }
    }

    /// C++ `main.cpp:471-537` — the boot dispatch (module docs: the two
    /// null-state deviations).
    pub fn boot(
        &self,
        cli_level_pack: Option<&str>,
        cli_level_name: Option<&str>,
        compressed_replay_filename: Option<&str>,
    ) {
        // C++ main.cpp:476-480 — `gotoMenu`.
        let goto_menu = || {
            // C++ `MenuGame::init`'s Steam lines (MenuGame.cpp:793-794) —
            // the composition's lifecycle-point equivalent (issue 020).
            self.steam.set_rich_presence_in_menu();
            self.steam.update_hardcoded_achievements();
            self.menu.init(false); // C++ `mg->init(false /* mError */)`
            self.menu_rich_presence(); // C++ mg->init's Discord lines (:796)
            self.engine.set_game_state(GameSlot::Menu); // `window->setGameState(mg->getGame())`
        };

        // C++ main.cpp:482-503 — `gotoGameCompressedReplay`.
        let goto_game_compressed_replay = |crf: &oh_replay::CompressedReplayFile| {
            match oh_replay::decompress_replay_file(crf) {
                None => {
                    // C++ main.cpp:490-491.
                    eprintln!("Could not decompress replay file");
                    // DOCUMENTED DEVIATION (module docs): the C++ `return`s
                    // here with NO state set (the first frame's
                    // `assert(isValid())` would fire); the port points the
                    // window at the menu instead.
                    goto_menu();
                }
                Some(replay) => {
                    // C++ main.cpp:496-502.
                    self.game.set_last_replay(replay.clone());
                    self.game.new_game(
                        &replay.pack_id,
                        &replay.level_id,
                        replay.first_play,
                        replay.difficulty_mult,
                        true, /* mExecuteLastReplay */
                    );
                    self.engine.set_game_state(GameSlot::Game);
                }
            }
        };

        match compressed_replay_filename {
            None => {
                if let (Some(pack), Some(level)) = (cli_level_pack, cli_level_name) {
                    // C++ main.cpp:509-510 — `mg->init(false, *cliLevelPack,
                    // *cliLevelName)` — its `init(false)` half carries the
                    // `MenuGame::init` Steam lines (MenuGame.cpp:793-794;
                    // the composition's lifecycle-point equivalent,
                    // issue 020).
                    self.steam.set_rich_presence_in_menu();
                    self.steam.update_hardcoded_achievements();
                    self.menu.init_command_line_level(false, pack, level);
                    self.menu_rich_presence(); // C++ mg->init's Discord lines (:796)
                                               // DOCUMENTED DEVIATION (module docs): the C++ sets no
                                               // state here either (null `gameState` → first-frame
                                               // assert); the port points the window at the menu (the
                                               // `loadCommandLineLevel` tail left it in
                                               // `LevelSelection`, MenuGame.cpp:1773).
                    self.engine.set_game_state(GameSlot::Menu);
                } else {
                    // C++ main.cpp:514-515 — "Start game as normal".
                    goto_menu();
                }
            }
            Some(filename) => {
                let mut crf = oh_replay::CompressedReplayFile::default();
                if crf.deserialize_from_file(filename) {
                    // C++ main.cpp:523-524.
                    println!("[Replay] Playing compressed replay file '{filename}'");
                    goto_game_compressed_replay(&crf);
                } else {
                    // C++ main.cpp:530-531.
                    println!("[Replay] Failed to read compressed replay file '{filename}'");
                    // C++ main.cpp:533.
                    goto_menu();
                }
            }
        }
    }

    /// C++ `main.cpp:340-349` — `std::signal(SIGINT, ...)`: the SAFE
    /// install (the C++'s `TODO (P2): UB` comment, main.cpp:339 — the C++
    /// handler ran `globalWindow.stop()`, a non-atomic engine mutation,
    /// INSIDE the signal context; the port keeps the signal context to
    /// ONE atomic store). `signal_hook::flag::register` sets
    /// [`Self::interrupt`] on SIGINT; the loop head (`tick_fields`) then
    /// performs the handler's actions — [`Self::sigint_stop`]. The
    /// ported `main_client` calls this once per process (a second call
    /// replaces the handler);
    /// on registration failure the default SIGINT (terminate) stays in
    /// force — the pre-port behavior (the C++'s `std::signal` would return
    /// `SIG_ERR` likewise).
    pub fn install_sigint(&self) {
        let _ = signal_hook::flag::register(
            signal_hook::consts::signal::SIGINT,
            self.interrupt.clone(),
        );
    }

    /// C++ `main.cpp:341-348` — the SIGINT handler's actions, performed at
    /// the loop head (the C++'s `while(isRunning())` position,
    /// GameWindow.hpp:121 — the C++ ran them in the signal context, the UB
    /// its comment marks): the three log lines (main.cpp:343-348) in the
    /// C++'s order around the deferred `globalWindow.stop()`
    /// (main.cpp:347 → `gameEngine->stop()`, GameEngine.hpp:75-78), then
    /// the window close that ends the port's winit loop (the C++'s loop
    /// condition is the engine's `isRunning()`; the port's winit loop ends
    /// on the window's close — module docs: the loop-exit translation).
    /// `sig` is the C++ handler's `int s` (main.cpp:343-344, SIGINT);
    /// `log` is the injected sink (production: stdout; tests assert the
    /// exact C++ lines).
    pub fn sigint_stop(
        sig: i32,
        window: &mut Window,
        engine: &Rc<GameEngine>,
        log: &mut dyn FnMut(&str),
    ) {
        log(&format!(
            "[::main] Caught signal '{sig}' with game window open"
        ));
        log("[::main] Stopping game window...");
        engine.stop(); // the deferred C++ `globalWindow.stop()` (main.cpp:347)
        log("[::main] Done stopping game window");
        window.close(); // end the winit loop (the port's loop-exit condition)
    }

    /// C++ `main.cpp:588-592` — `window->run()`: drive the winit loop, one
    /// frame per [`Self::tick_fields`]. Consumes the client (the `Window`
    /// is moved into the loop, as the C++'s `GameWindow::run` owns its
    /// loop); drop order is preserved (`steam` — the C++'s first
    /// construction, main.cpp:251 — drops LAST, as the C++'s scope
    /// teardown does; `composed` — the `LuaContext` — drops after the
    /// states' `Rc<HexagonGame>` clones).
    pub fn run(self) {
        // Declaration order = reverse drop order: `steam` first ⇒
        // dropped last (the C++'s teardown — the manager's `Drop` is the
        // `SteamAPI_Shutdown`); `composed` next ⇒ after the engine/menu
        // (the `SharedLua` contract, compose.rs).
        let WindowedClient {
            steam: _steam,
            discord: _discord,
            composed,
            window,
            engine,
            timer,
            menu,
            game,
            config,
            engine_slot: _engine_slot,
            interrupt,
            _app_draw,
            ilc,
            render_size,
        } = self;
        let _ = &composed; // keep it alive until the end of the scope
        let mut timer = timer;
        // Issue 014b — the C++ `runEvents`'s per-event
        // `gameEngine->handleEvent` (GameWindow.hpp:79; the game's
        // `onAnyEvent` = `Imgui::processEvent`, HexagonGame.cpp:335) —
        // the egui console's event feed: it fires AFTER the SFML
        // input-state fold (the C++'s `runEvents` order, GameWindow.hpp:
        // 42-83 — the input state is updated FIRST), and the game input
        // is gated by the engine's `imgui_lua_console_has_input`, NOT by
        // dropping events (the C++ updates its `inputState`
        // unconditionally and lets the gate suppress). The slot stays
        // `None` until the first shown frame (the GL-gated lazy
        // creation), so most frames this is a cheap check.
        let hook_ilc = ilc.clone();
        window.run_with_event_hook(
            Some(Box::new(move |winit_window, event| {
                if let Some(console) = hook_ilc.borrow_mut().as_mut() {
                    console.on_event(winit_window, event);
                }
            })),
            |window| {
                Self::tick_fields(
                    window,
                    &mut timer,
                    &engine,
                    &menu,
                    &interrupt,
                    &game,
                    &config,
                    &ilc,
                    &render_size,
                )
            },
        );
        // C++ main.cpp:594-595 — the `main_client` caller prints the
        // "Finished" line (the ported `main_client` does, after this
        // returns).
    }

    /// One frame — the C++ `GameWindow::run` body (GameWindow.hpp:117-150)
    /// with the port's target + exit translation (module docs).
    pub fn tick(&self, window: &mut Window, timer: &mut TimerStatic) {
        Self::tick_fields(
            window,
            timer,
            &self.engine,
            &self.menu,
            &self.interrupt,
            &self.game,
            &self.config,
            &self.ilc,
            &self.render_size,
        )
    }

    /// A [`Self::tick`] driven by the client's own window + timer — the
    /// windowed input tests' driver (`tick` takes them as arguments; this
    /// borrows them from the client instead — no `&self`/`&mut self`
    /// conflict on the same field).
    pub fn tick_self(&mut self) {
        Self::tick_fields(
            &mut self.window,
            &mut self.timer,
            &self.engine,
            &self.menu,
            &self.interrupt,
            &self.game,
            &self.config,
            &self.ilc,
            &self.render_size,
        )
    }

    /// The frame body over the client's pieces (the `tick` wrapper is the
    /// `&self` convenience; `run` calls this with the moved-out fields).
    /// `#[allow]`: the C++'s frame body genuinely needs all eight pieces
    /// (the two states' frame halves + the console's post-update/draw
    /// halves + the phase timers) — a context struct would just hide the
    /// C++'s parameter list (cf. `HexagonGame::new`'s allow).
    #[allow(clippy::too_many_arguments)]
    fn tick_fields(
        window: &mut Window,
        timer: &mut TimerStatic,
        engine: &Rc<GameEngine>,
        menu: &Rc<MenuGame>,
        interrupt: &AtomicBool,
        game: &Rc<HexagonGame<GameApiState>>,
        config: &Arc<RwLock<Config>>,
        ilc: &Rc<RefCell<Option<crate::lua_console_ui::IlcConsole>>>,
        render_size: &std::sync::Arc<RenderWindowSize>,
    ) {
        // C++ `while(gameEngine->isRunning())` loop head (GameWindow.hpp:
        // 121) — the SIGINT flag (the safe port of the C++ handler, main
        // .cpp:340-349; module docs: the SIGINT bullet): the handler's
        // three log lines (main.cpp:343-348) around the deferred
        // `globalWindow.stop()` (main.cpp:347) + the window close that
        // ends the port's winit loop (the C++'s loop condition is the
        // engine's `isRunning()`; the port's is `window.open`). Observed
        // here, the C++'s `isRunning()` condition reads the handler's
        // `running = false` at the same loop-head position.
        if interrupt.load(Ordering::SeqCst) {
            let mut out = |line: &str| println!("{line}");
            Self::sigint_stop(
                signal_hook::consts::signal::SIGINT,
                window,
                engine,
                &mut out,
            );
            return;
        }
        // issue 025g — the ACTUAL-window-size slot's per-frame refresh
        // (the C++ `window.onRecreation`'s value liveness — the C++ reads
        // `window.getRenderWindow().getSize()` live at every call site;
        // the port's `Window` size bookkeeping is current here: the
        // event fold's `Resized → set_size` half ran during the event
        // dispatch that drove this frame, the winit android
        // `MainEvent::WindowResized` → `Resized(native size)` before the
        // first frame included): the menu's `MenuWindow` seam (the
        // 9-zone grid's thirds + `refresh_camera`'s view math + the
        // `drawOnlineStatus` identity view) reads it.
        render_size.update(window.get_width(), window.get_height());
        // C++ `if(mustRecreate) recreateWindow();` (GameWindow.hpp:123) —
        // the port creates the window once (no runtime recreation; module
        // docs).
        // C++ `renderWindow.setActive(true);` (GameWindow.hpp:125) — the
        // GL context is current by construction (glutin).
        // C++ `this->clear();` (GameWindow.hpp:126; `clear` at :157-160,
        // default `sf::Color::Transparent`) — the per-frame pre-update
        // clear of the default framebuffer.
        if let Some(gl) = window.gl() {
            gl.clear(COLOR_TRANSPARENT);
        }
        // C++ `gameEngine->refreshTimer();` (GameWindow.hpp:128 →
        // GameEngine.hpp:34-37 → GameTimer::refresh, GameTimer.hpp:26-37):
        // the next-impl swap is a no-op — `mainClient` installs the timer
        // exactly once before `run()` (main.cpp:330-332).
        // C++ `runEvents();` (GameWindow.hpp:42-83) — the winit handler
        // already folded the queued window events into `window.input`
        // (Closed → `window.close()`; the C++'s per-event
        // `gameEngine->handleEvent` is the documented no-op, module docs).
        // C++ `gameEngine->runUpdate();` (GameWindow.hpp:133 →
        // GameEngine.hpp:80-86): `refreshInput(mInput)` (ONCE per frame,
        // before the burst) → `timer->runUpdate()` (N×`updateFromTimer`,
        // each = the current state's `updateInput` + `update`,
        // GameEngine.hpp:40-50 / GameState.hpp:49-57) → `onPostUpdate`.
        // The C++ `msUpdate` span starts at this position
        // (GameWindow.hpp:131-137 — the C++ timer wraps `runEvents()` +
        // `runUpdate()`; the port's `runEvents` equivalent — winit's
        // per-event fold — completed during the event dispatch that
        // drove this frame, so its time is not included: documented,
        // the footer's `update ms` is the port's `runUpdate` span).
        let ms_update_start = std::time::Instant::now();
        let input = window.get_input_state();
        engine.refresh_input(input);
        timer.run_update(|ft| {
            engine.update_input(input, ft);
            engine.update_from_timer(ft);
        });
        engine.on_post_update();
        // C++ `postUpdate_ImguiLuaConsole`'s egui UI tail (issue 014b /
        // D-022 — `crate::lua_console_ui`'s module docs: the per-frame
        // order + the C++'s one-frame `wantCapture*` lag). The C++ runs
        // it INSIDE `HexagonGame::postUpdate` (HexagonGame.cpp:331), so
        // the game-state gate is the C++'s state gate.
        if engine.current() == GameSlot::Game {
            crate::lua_console_ui::frame_post_update(window, game, config, ilc);
        }
        // C++ `msUpdate = ...` (GameWindow.hpp:135-137).
        window.set_ms_update(ms_update_start.elapsed().as_secs_f64() as f32 * 1000.0);
        // C++ `gameEngine->runDraw();` + `renderWindow.display();`
        // (GameWindow.hpp:141-143) — the current state's `draw`, then the
        // explicit per-frame back-buffer swap (`window.swap_buffers()`,
        // GameWindow.hpp:143 — winit does NOT swap a glutin `Surface` at
        // event-loop end). The target is a
        // per-frame scratch (the concrete type — no `'static` erasure):
        // the `GlParts` lives in the client's window for the whole loop.
        // The C++ `msDraw` span (GameWindow.hpp:143-146 — the C++ timer
        // wraps `runDraw()` + `display()`; the port's swap is the
        // explicit `window.swap_buffers()` above, measured in this span).
        let ms_draw_start = std::time::Instant::now();
        if let Some(gl) = window.gl() {
            let mut target = GlDrawTarget::new(gl);
            engine.draw(&mut target);
            // C++ `drawImguiLuaConsole` (HGGraphics.cpp:318-336) — the
            // console's GL paint, the C++ `draw()`'s LAST step
            // (HGGraphics.cpp:315), after the game's own draw.
            if engine.current() == GameSlot::Game {
                crate::lua_console_ui::frame_draw(window, game, ilc);
            }
            // C++ `renderWindow.display()` — the swap is INSIDE the C++
            // msDraw span (GameWindow.hpp:141-143: runDraw + display).
            // Issue 25, on-device finding: the swap was never wired in this
            // loop body — every GL platform presented nothing (the desktop
            // live check was MANUAL and never run; the emulator caught it;
            // windows-testing found the same defect independently and called
            // it window.swap_buffers() — same call, inside this msDraw span).
            target.present();
        } else {
            let mut target = NullDrawTarget;
            engine.draw(&mut target);
        }
        // C++ `msDraw = ...` (GameWindow.hpp:144-146).
        window.set_ms_draw(ms_draw_start.elapsed().as_secs_f64() as f32 * 1000.0);
        // C++ `gameEngine->runFPS();` (GameWindow.hpp:148 →
        // GameEngine.hpp:96-101): `runFrameTime` (the next frame's
        // `frameTime`, TimerBase.hpp:40-43) then `runFPS`
        // (TimerBase.hpp:45-48).
        timer.run_frame_time();
        timer.run_fps();

        // Loop-exit translation — the C++ `while(gameEngine->isRunning())`
        // (GameWindow.hpp:121) sees `false` from:
        // (a) the window's `Closed` event → C++ `gameEngine->stop()`
        //     (GameWindow.hpp:53);
        // (b) the menu's EXIT item → C++ `window->stop()` →
        //     `gameEngine->stop()` (GameWindow.hpp:151-155).
        if !window.is_open() || (engine.current() == GameSlot::Menu && menu.quit_requested()) {
            engine.stop();
        }
    }
}

/// C++ `main.cpp:297-332` — the window (the C++ creates it in
/// `mainClient`, BEFORE the assets at :381; the order is behaviorally
/// irrelevant — neither reads the other — so the ported shared-assets load
/// in `main_client` stays where it is). `base_dir` is the C++'s CWD
/// (the binary passes `std::env::current_dir()` — the C++'s CWD-relative
/// paths, incl. `Assets/icon.png` below, resolve against it).
///
/// `Window::create` degrades to a headless window (`gl() == None`) when no
/// display / GPU is available; the windowed loop then runs its frames on
/// the `NullDrawTarget` (no GL). Production passes the result to
/// [`WindowedClient::build`]; tests pass a `Window::headless` instead (the
/// spec's "real windowed run = MANUAL").
#[must_use]
pub fn create_window(config: &Config, base_dir: &Path) -> Window {
    let logical_w = config.get_width() as f32;
    let logical_h = config.get_height() as f32;
    let pixel_mult = config.get_pixel_multiplier() as f32;
    // C++ `recreateWindow` (GameWindow.hpp:89-93): the render window is
    // sized `width * pixelMult × height * pixelMult`; the logical members
    // stay logical. The port applies this once, at creation.
    let physical_w = (logical_w * pixel_mult) as u32;
    let physical_h = (logical_h * pixel_mult) as u32;
    // C++ `makeWindowTitle()` (main.cpp:139-143) with `GAME_VERSION_STR`
    // (Version.hpp:47).
    let title = "Open Hexagon 2.1.7 - by Vittorio Romeo - https://vittorioromeo.info";

    let mut window = Window::create(physical_w.max(1), physical_h.max(1), title);
    // Restore the logical size members (the C++ `setSize(logical)`).
    window.set_size(logical_w as u32, logical_h as u32);

    // C++ main.cpp:305-310 (the setters, in the C++ order).
    window.set_pixel_mult(pixel_mult);
    window.set_fullscreen(config.get_fullscreen());
    window.set_antialiasing_level(config.get_antialiasing_level());
    window.set_vsync(config.get_vsync());
    window.set_fps_limited(config.get_limit_fps());
    window.set_max_fps(config.get_max_fps() as f32);

    // C++ main.cpp:312-328 — `Assets/icon.png` via `sf::Image` +
    // `setIcon` (re-applied on `onRecreation` — no port hook: the port's
    // window is never recreated at runtime, module docs: the Icon bullet).
    // The decode is the screenshot's `png` dependency (`Image::load_png`
    // — the C++'s stb-based `loadFromFile`, always RGBA8).
    let icon_path = base_dir.join("Assets/icon.png");
    match oh_render::Image::load_png(&icon_path) {
        Ok(icon) => {
            window.set_icon(&icon);
        }
        Err(_) => {
            // C++ main.cpp:318 (the C++'s `::main` channel).
            println!("[::main] Failed to load icon image");
        }
    }

    window
}

/// C++ `GameWindow::run`'s per-frame body (GameWindow.hpp:117-150) as a
/// free function over the composition's pieces — the state-switching
/// tests drive THIS with fake states + the real [`TimerStatic`] (the
/// spec's §5 harness); [`WindowedClient::tick`] is the client-level
/// wrapper (target + exit translation) around it.
///
/// The C++ order, exactly:
/// 1. `clear` (the pre-update framebuffer clear — headless: no-op);
/// 2. `runEvents` (the caller's event loop / the test's no-op — the
///    window's `InputState` is current by then);
/// 3. `runUpdate`: `timer->runUpdate` (N× the CURRENT state's
///    `update_from_timer`) then `onPostUpdate` (GameEngine.hpp:80-86);
/// 4. `runDraw`: the CURRENT state's `draw` (GameEngine.hpp:89-94 →
///    TimerBase::runDraw, TimerBase.inl:10-13);
/// 5. `runFPS`: `runFrameTime` + `runFPS` (GameEngine.hpp:96-101).
pub fn frame(
    window: &mut Window,
    engine: &GameEngine,
    timer: &mut TimerStatic,
    target: &mut dyn DrawTarget,
) {
    if let Some(gl) = window.gl() {
        gl.clear(COLOR_TRANSPARENT); // C++ `this->clear()` (GameWindow.hpp:126)
    }
    // C++ `runUpdate` (GameEngine.hpp:80-86): `refreshInput(mInput)` ONCE
    // per frame, then the burst (each tick = `updateInput` + `update`,
    // GameEngine.hpp:40-50 / GameState.hpp:49-57).
    let input = window.get_input_state();
    engine.refresh_input(input);
    timer.run_update(|ft| {
        engine.update_input(input, ft);
        engine.update_from_timer(ft);
    });
    engine.on_post_update();
    // C++ `runDraw` (GameEngine.hpp:89-94).
    engine.draw(target);
    // C++ `renderWindow.display()` (GameWindow.hpp:145 — the swap after
    // runDraw, inside the run loop's iteration; issue 25 on-device finding:
    // the present was never wired — no GL platform showed a frame).
    target.present();
    // C++ `runFPS` (GameEngine.hpp:96-101).
    timer.run_frame_time();
    timer.run_fps();
}

#[cfg(test)]
mod tests {
    use super::*;
    use oh_menu::seams::{FakeAssets, FakeConfig, NoopNetwork, NoopTouch};
    use oh_render::window::ReleasedEdges;
    use std::sync::RwLock;

    /// The issue-025c boot-screen wiring (the `MenuSlotAdapter::
    /// refresh_input` docs): a key-RELEASE event while the boot screen
    /// holds `ignore_inputs` advances LoadingScreen →
    /// EpilepsyWarning → `play_locally()` (ETLPNewBoot — no profiles in
    /// the fake assets), and no advance happens without a release event
    /// (a held key is not a release). The events are driven through the
    /// RELEASE-EDGE SINK — the event-level seam a same-frame
    /// press+release (the `adb input keyevent` shape) reaches even
    /// though the per-frame `InputState` snapshot never shows the key.
    #[test]
    fn release_edge_advances_the_boot_screens() {
        let menu = Rc::new(MenuGame::new(
            FakeAssets::new(),
            FakeConfig::new(),
            Rc::new(NoopNetwork),
            oh_menu::RecordingAudio::new(),
            Rc::new(NoopTouch),
            oh_menu::FakeWindow::new(),
            None,
            None,
            None,
            None,
            Rc::new(oh_menu::layout::LinearFontHeight),
        ));
        let f = oh_render::font::Font::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../_RELEASE/Assets/OpenSquare-Regular.ttf"),
        )
        .expect("OpenSquare-Regular.ttf must load");
        let released_edges = Arc::new(RwLock::new(ReleasedEdges::default()));
        let adapter = MenuSlotAdapter {
            menu: menu.clone(),
            input_manager: Rc::new(RefCell::new(Manager::new())),
            menu_draw: RefCell::new(crate::menu_draw::MenuDraw::new(&f, &f)),
            released_edges: released_edges.clone(),
            entered_text: Arc::new(RwLock::new(Vec::new())),
            soft_input: Arc::new(NoOpSoftInput),
            was_entering_text: Cell::new(false),
        };

        // The C++ ctor body holds the boot gate (MenuGame.cpp:357).
        assert_eq!(adapter.menu.state(), States::LoadingScreen);
        assert_eq!(adapter.menu.shared().borrow().ignore_inputs, 1);

        // Enter HELD (no release event yet) — no advance.
        let mut down = InputState::new();
        down.set_key_pressed(Key::ENTER, true);
        adapter.refresh_input(&down);
        assert_eq!(adapter.menu.state(), States::LoadingScreen);

        // Enter RELEASED (the `KeyReleased` event; the snapshot may have
        // never shown the key pressed — a same-frame tap) —
        // LoadingScreen → EpilepsyWarning; the gate re-arms (the C++
        // `checkCloseBootScreens` `setIgnoreAllInputs(1)`).
        released_edges
            .write()
            .expect("sink poisoned")
            .keys
            .insert(Key::ENTER);
        adapter.refresh_input(&InputState::new());
        assert_eq!(adapter.menu.state(), States::EpilepsyWarning);
        assert_eq!(adapter.menu.shared().borrow().ignore_inputs, 1);

        // A finger RELEASE (the Android touch seam — the SFML window
        // layer's `MouseButtonReleased`) — EpilepsyWarning →
        // `play_locally()` → ETLPNewBoot (the fake assets have no
        // profiles); the gate clears.
        released_edges
            .write()
            .expect("sink poisoned")
            .fingers
            .insert(0);
        adapter.refresh_input(&InputState::new());
        assert_eq!(adapter.menu.state(), States::ETLPNewBoot);
        assert_eq!(adapter.menu.shared().borrow().ignore_inputs, 0);
    }

    /// Issue 025d (the on-device defect + fix).
    ///
    /// The LIVE CHAIN: the real `Window::apply_event` fold (the
    /// `runEvents` per-event side effects) + the real
    /// `MenuSlotAdapter::refresh_input` drain, driven TOGETHER over a
    /// real `Window` (the sink-in-isolation test above never exercised
    /// the fold).
    ///
    /// The keyboard release is the SFML `KeyReleased` the mapping
    /// produces for the phone's Enter (the raw Android winit shape —
    /// physical `Code(Enter)` + logical `Character("\n")` — is pinned to
    /// that SFML event by `oh-render`'s `backend` test
    /// `android_enter_character_form_maps_to_sfml_enter`; the pre-fix
    /// defect was that that shape mapped to `Key::UNKNOWN`,
    /// `map_window_event` returned `None`, and the release never reached
    /// the sink — the phone stayed on the LoadingScreen while the host
    /// tests stayed green).
    ///
    /// The touch release is the raw winit `WindowEvent::Touch`
    /// (`TouchPhase::Ended`) the phone delivered — the full raw-event
    /// chain: `map_window_event` → fold → sink → drain → advance.
    #[test]
    fn device_event_chain_advances_the_boot_screens() {
        use oh_render::window::map_window_event;
        use winit::dpi::PhysicalPosition;
        use winit::event::{DeviceId, Touch, TouchPhase, WindowEvent};

        let menu = Rc::new(MenuGame::new(
            FakeAssets::new(),
            FakeConfig::new(),
            Rc::new(NoopNetwork),
            oh_menu::RecordingAudio::new(),
            Rc::new(NoopTouch),
            oh_menu::FakeWindow::new(),
            None,
            None,
            None,
            None,
            Rc::new(oh_menu::layout::LinearFontHeight),
        ));
        let f = oh_render::font::Font::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../_RELEASE/Assets/OpenSquare-Regular.ttf"),
        )
        .expect("OpenSquare-Regular.ttf must load");
        let released_edges = Arc::new(RwLock::new(ReleasedEdges::default()));
        let adapter = MenuSlotAdapter {
            menu: menu.clone(),
            input_manager: Rc::new(RefCell::new(Manager::new())),
            menu_draw: RefCell::new(crate::menu_draw::MenuDraw::new(&f, &f)),
            released_edges: released_edges.clone(),
            entered_text: Arc::new(RwLock::new(Vec::new())),
            soft_input: Arc::new(NoOpSoftInput),
            was_entering_text: Cell::new(false),
        };
        let mut window = Window::headless(800, 480);
        window.set_released_edges_sink(released_edges.clone());

        assert_eq!(adapter.menu.state(), States::LoadingScreen);
        // The Enter release (the SFML `KeyReleased` the mapping produces
        // for the phone's Enter — see the test docs) through the REAL
        // fold: the sink gets the edge (the pre-fix hole — the event was
        // dropped before the fold, so the sink stayed empty).
        Window::apply_event(
            &mut window,
            oh_render::window::Event::KeyReleased { code: Key::ENTER },
        );
        assert!(!window.get_input_state().is_key_pressed(Key::ENTER));

        // The once-per-frame drain (the C++ `runUpdate` head) —
        // LoadingScreen → EpilepsyWarning; the gate re-arms.
        adapter.refresh_input(&InputState::new());
        assert_eq!(adapter.menu.state(), States::EpilepsyWarning);
        assert_eq!(adapter.menu.shared().borrow().ignore_inputs, 1);

        // The phone's touch release, verbatim from the 025d logcat
        // (`adb shell input tap 720 540` — the `TouchPhase::Ended`;
        // the SFML window layer's `MouseButtonReleased` seam): the FULL
        // raw-event chain — `map_window_event` → fold → sink → drain →
        // `play_locally()` → ETLPNewBoot (the fake assets have no
        // profiles); the gate clears.
        let touch_ended = WindowEvent::Touch(Touch {
            device_id: DeviceId::dummy(),
            phase: TouchPhase::Ended,
            location: PhysicalPosition::new(650.0, 540.0),
            force: None,
            id: 0,
        });
        Window::apply_event(
            &mut window,
            map_window_event(&touch_ended).expect("TouchPhase::Ended must map"),
        );
        adapter.refresh_input(&InputState::new());
        assert_eq!(adapter.menu.state(), States::ETLPNewBoot);
        assert_eq!(adapter.menu.shared().borrow().ignore_inputs, 0);
    }

    /// Issue 025d (the acceptance flow beyond the boot gate — the 1:1 C++
    /// path a fresh install takes to the main menu): the ENTERED-TEXT
    /// chain — the window's `TextEntered` fold (the C++ `runEvents`
    /// dispatch, GameWindow.hpp:42-83 — the character arrives in the
    /// winit `KeyboardInput` `text` field on desktop AND Android, the
    /// issue-025d logcat evidence: `text: Some("\n")` on the Enter
    /// press) → the shared entered-text sink → the REAL
    /// `MenuSlotAdapter::refresh_input` drain → the C++ `MenuGame`
    /// `onEvent(TextEntered)` handler (MenuGame.cpp:491-498 — the
    /// `unicode < 128` filter) → `enteredChars` → `update()`'s
    /// `isEnteringText`-gated accumulation (`process_entered_chars`,
    /// MenuGame.cpp:2862-2878) → `enteredStr`. This is what makes the
    /// fresh-install profile creation (ETLPNewBoot → `ok_action` →
    /// SMain) reachable from device input (the pre-fix hole: the
    /// `TextEntered` fold arm was a no-op, so `enteredStr` could never
    /// fill and `ok_action`'s `create_profile_from_entered` returned
    /// false).
    #[test]
    fn entered_text_chain_accumulates_the_entered_str() {
        use oh_render::window::Event;

        let menu = Rc::new(MenuGame::new(
            FakeAssets::new(),
            FakeConfig::new(),
            Rc::new(NoopNetwork),
            oh_menu::RecordingAudio::new(),
            Rc::new(NoopTouch),
            oh_menu::FakeWindow::new(),
            None,
            None,
            None,
            None,
            Rc::new(oh_menu::layout::LinearFontHeight),
        ));
        let f = oh_render::font::Font::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../_RELEASE/Assets/OpenSquare-Regular.ttf"),
        )
        .expect("OpenSquare-Regular.ttf must load");
        let entered_text = Arc::new(RwLock::new(Vec::new()));
        let released_edges = Arc::new(RwLock::new(ReleasedEdges::default()));
        let adapter = MenuSlotAdapter {
            menu: menu.clone(),
            input_manager: Rc::new(RefCell::new(Manager::new())),
            menu_draw: RefCell::new(crate::menu_draw::MenuDraw::new(&f, &f)),
            released_edges: released_edges.clone(),
            entered_text: entered_text.clone(),
            soft_input: Arc::new(NoOpSoftInput),
            was_entering_text: Cell::new(false),
        };
        let mut window = Window::headless(800, 480);
        window.set_released_edges_sink(released_edges.clone());
        window.set_entered_text_sink(entered_text.clone());

        // The fresh-install boot (the `device_event_chain_advances_the_
        // boot_screens` chain): Enter release → EpilepsyWarning; finger
        // release → `play_locally()` → ETLPNewBoot (the fake assets have
        // no profiles — the C++ `getLocalProfilesSize() == 0` branch).
        Window::apply_event(&mut window, Event::KeyReleased { code: Key::ENTER });
        adapter.refresh_input(&InputState::new());
        assert_eq!(adapter.menu.state(), States::EpilepsyWarning);
        Window::apply_event(&mut window, Event::TouchEnded { finger: 0 });
        adapter.refresh_input(&InputState::new());
        assert_eq!(adapter.menu.state(), States::ETLPNewBoot);

        // The phone's character keys (the winit `KeyboardInput` `text`
        // field — `adb shell input keyevent 32/29/34` = "D" / "A" /
        // "F"): the fold's `TextEntered` recording, then the
        // once-per-frame drain (the C++ handler, MenuGame.cpp:491-498).
        for c in ['D', 'A', 'F'] {
            Window::apply_event(&mut window, Event::TextEntered { character: c });
        }
        adapter.refresh_input(&InputState::new());
        assert_eq!(menu.shared().borrow().entered_chars, vec!['D', 'A', 'F']);
        // The C++ `update()` accumulation (MenuGame.cpp:2862-2878) —
        // alphanumeric → `enteredStr`, then `enteredChars` clears.
        menu.update(0.01);
        assert_eq!(menu.shared().borrow().entered_str, "DAF");
        assert!(menu.shared().borrow().entered_chars.is_empty());

        // The handler's `unicode < 128` filter (MenuGame.cpp:492): the
        // non-ASCII char never reaches `enteredChars`.
        Window::apply_event(&mut window, Event::TextEntered { character: 'é' });
        adapter.refresh_input(&InputState::new());
        assert!(menu.shared().borrow().entered_chars.is_empty());

        // The C++ gate (MenuGame.cpp:2862): outside the entering-text
        // states the accumulation is suppressed (the `enteredChars.
        // clear()` at :2878 is UNCONDITIONAL).
        menu.shared().borrow_mut().state = States::SMain;
        Window::apply_event(&mut window, Event::TextEntered { character: 'x' });
        adapter.refresh_input(&InputState::new());
        menu.update(0.01);
        assert_eq!(menu.shared().borrow().entered_str, "DAF");
        assert!(menu.shared().borrow().entered_chars.is_empty());
    }

    /// issue 025f (D-031): the soft-input (system IME) transition edge —
    /// the `MenuSlotAdapter::refresh_input` docs: the menu's
    /// `is_entering_text` (the C++ `isEnteringText` — the `oh-menu`
    /// `States::is_entering_text`: `ETLPNewBoot` / `ETLPNew`) flips the
    /// controller's `show`/`hide` ONCE per transition (the `was` latch),
    /// never per frame. Driven over the `SoftInput` seam with a
    /// recording controller (no android-activity on the host — the
    /// seam's whole point).
    #[test]
    fn soft_input_edge_shows_on_entering_text_and_hides_on_leaving() {
        // The recording controller (the `SoftInput` seam's test arm —
        // records the calls in order: `true` = `show`, `false` = `hide`).
        #[derive(Default)]
        struct RecordingSoftInput {
            calls: std::sync::Mutex<Vec<bool>>,
        }
        impl SoftInput for RecordingSoftInput {
            fn show(&self) {
                self.calls
                    .lock()
                    .expect("recording mutex poisoned")
                    .push(true);
            }
            fn hide(&self) {
                self.calls
                    .lock()
                    .expect("recording mutex poisoned")
                    .push(false);
            }
        }
        let rec = Arc::new(RecordingSoftInput::default());
        let menu = Rc::new(MenuGame::new(
            FakeAssets::new(),
            FakeConfig::new(),
            Rc::new(NoopNetwork),
            oh_menu::RecordingAudio::new(),
            Rc::new(NoopTouch),
            oh_menu::FakeWindow::new(),
            None,
            None,
            None,
            None,
            Rc::new(oh_menu::layout::LinearFontHeight),
        ));
        let f = oh_render::font::Font::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../_RELEASE/Assets/OpenSquare-Regular.ttf"),
        )
        .expect("OpenSquare-Regular.ttf must load");
        let soft_input: Arc<dyn SoftInput> = rec.clone();
        let adapter = MenuSlotAdapter {
            menu: menu.clone(),
            input_manager: Rc::new(RefCell::new(Manager::new())),
            menu_draw: RefCell::new(crate::menu_draw::MenuDraw::new(&f, &f)),
            released_edges: Arc::new(RwLock::new(ReleasedEdges::default())),
            entered_text: Arc::new(RwLock::new(Vec::new())),
            soft_input,
            was_entering_text: Cell::new(false),
        };

        // The boot state is not a text-entry state — no call (the latch
        // starts at `false`, matching the first frame).
        assert!(!adapter.menu.is_entering_text());
        adapter.refresh_input(&InputState::new());
        assert!(rec
            .calls
            .lock()
            .expect("recording mutex poisoned")
            .is_empty());

        // Enter the entering-text state (the C++ `isEnteringText`'s
        // `ETLPNewBoot` arm — the fresh-install name screen) → ONE
        // `show`.
        menu.shared().borrow_mut().state = States::ETLPNewBoot;
        adapter.refresh_input(&InputState::new());
        assert_eq!(
            rec.calls
                .lock()
                .expect("recording mutex poisoned")
                .as_slice(),
            &[true]
        );
        // Holding the state across frames: no per-frame calls (the `was`
        // latch — the task's "call once per transition, not per frame").
        adapter.refresh_input(&InputState::new());
        adapter.refresh_input(&InputState::new());
        assert_eq!(
            rec.calls
                .lock()
                .expect("recording mutex poisoned")
                .as_slice(),
            &[true]
        );

        // Leave it (→ `SMain`, the profile-created state) → ONE `hide`.
        menu.shared().borrow_mut().state = States::SMain;
        adapter.refresh_input(&InputState::new());
        assert_eq!(
            rec.calls
                .lock()
                .expect("recording mutex poisoned")
                .as_slice(),
            &[true, false]
        );
        // And holding `SMain` stays quiet (no keyboard in the main menu).
        adapter.refresh_input(&InputState::new());
        assert_eq!(
            rec.calls
                .lock()
                .expect("recording mutex poisoned")
                .as_slice(),
            &[true, false]
        );

        // The second entering-text state (`ETLPNew` — the level-pack
        // name dialog) re-arms the `show` on its own edge.
        menu.shared().borrow_mut().state = States::ETLPNew;
        adapter.refresh_input(&InputState::new());
        assert_eq!(
            rec.calls
                .lock()
                .expect("recording mutex poisoned")
                .as_slice(),
            &[true, false, true]
        );
    }
}
