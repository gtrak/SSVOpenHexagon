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
//! * **Boot-dispatch null states** — the C++ `-p -l` SUCCESS path
//!   (`main.cpp:507-511`: `mg->init` → `loadCommandLineLevel` whose tail
//!   `playSelectedLevel`, MenuGame.cpp:1778, fires `fnHGNewGame` →
//!   `window->setGameState(hg.getGame())`, main.cpp:442-449) sets the game
//!   state; only the `-p -l` FAILURE path (the `loadCommandLineLevel`
//!   early returns, MenuGame.cpp:1701-1765 — invalid pack / no levels /
//!   invalid level / no profiles) and the replay decompress-failure path
//!   (`main.cpp:488-492`: log + `return`) NEVER call
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
// issue 041: no longer gated off emscripten — the menu's REAL audio sink
// (`MenuAudioSink`) now compiles on the web too (the OpenAL backend behind the
// `AudioDevice` seam), and it calls the `oh_game::seams::Audio` trait methods
// (`play_sound_override` / `stop_music` / `stop_sounds`).
use oh_game::seams::Audio;
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
use oh_render::Vec2;

use crate::bridge::AppDraw;
use crate::compose::{compose_with_config, RealProfiles};
use crate::game_state::{GameEngine, GameSlot, GameState};

/// Issue 014b — the egui console overlay's lazy slot (`None` until the first
/// shown frame — `crate::lua_console_ui`'s module docs: Gates). The type is
/// aliased so the `WindowedClient`'s field + the `tick_fields` param exist on
/// every platform; on emscripten the console is OFF (W2) — `lua_console_ui`
/// is `#[cfg(not(target_os = "emscripten"))]`, so the slot degrades to the
/// unit `()`.
#[cfg(not(target_os = "emscripten"))]
type IlcSlot = Rc<RefCell<Option<crate::lua_console_ui::IlcConsole>>>;
#[cfg(target_os = "emscripten")]
type IlcSlot = ();

/// The menu's `AudioSink` over the REAL `oh_audio::Audio` (issue 025e — the
/// C++ `hg::Audio`, main.cpp:392-401, shared with the engine). The menu's two
/// `AudioSink` calls map onto the C++ menu's audio calls:
///
/// * `play_sound(assetId)` → C++ `MenuGame::playSoundOverride(assetId)`
///   (MenuGame.cpp:1057-1061) — the `!Config::getNoSound()` gate +
///   `audio.playSoundOverride(assetId)`.
/// * `stop_all()` → C++ `audio.stopMusic(); audio.stopSounds();`
///   (MenuGame.cpp:798-799, the `MenuGame::init` teardown).
///
/// issue 041: no longer gated off emscripten — the web's audio now works via
/// the OpenAL backend behind the `oh_render::audio::AudioDevice` seam (rodio /
/// cpal have no emscripten backend, but emscripten's OpenAL does), so the
/// menu's blips play on the web exactly as they do on desktop / Android.
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
///
/// 029e3: the window's FPS slot (the C++ `window->getFPS()` —
/// `GameWindow.hpp:335-338`'s `gameEngine->getFPS()` = the timer's
/// `float fps`, TimerBase.hpp:23: refreshed by `runFPS` at the END of
/// each loop iteration, GameWindow.hpp:148 — so a draw/update pass
/// reads the PREVIOUS iteration's value, 0 on the very first frame). The
/// C++'s window IS the timer owner; the port's `WindowTouchRenderer`
/// stands in for the C++'s `window` on this data path too (the
/// engine's `Renderer` seam's `fps()` — the FPS text in `updateText`).
/// `tick_fields` writes `timer.get_fps()`'s `f32` bits here right after
/// the C++'s `runFPS` position; the engine reads it through the trait
/// on its next tick.
struct WindowTouchRenderer {
    positions: std::sync::Arc<
        std::sync::RwLock<[Option<oh_math::ssvs::Vec2>; oh_render::input::FINGER_COUNT]>,
    >,
    fps_bits: std::sync::Arc<std::sync::atomic::AtomicU32>,
    /// issue 31 §1 — the window's LIVE `InputState` slot (the C++
    /// `window->getInputState()` the `u_isKeyPressed` /
    /// `u_isMouseButtonPressed` reads, HGScripting.cpp:92/:138): the
    /// windowed client installs it on the window's event fold
    /// (`Window::set_input_state_sink` — republished on every input
    /// event), and the engine's `GameApi::is_key_pressed` /
    /// `is_mouse_button_pressed` consult it through the `Renderer` seam
    /// (the C++'s `window` stand-in). The raw-code subscripts are
    /// bounds-checked in `InputState::is_key_pressed_code` /
    /// `is_button_pressed_code` (the C++'s bitset `out_of_range` throw
    /// becomes a defined `false`).
    input_state: std::sync::Arc<std::sync::RwLock<oh_render::input::InputState>>,
    /// issue 31 §2b — the gameplay touch overlay (the consumed-pointer
    /// exclusion — the §2b double-count hazard: a finger held on an
    /// overlay button must NOT also count as a screen-half rotation
    /// finger; the C++'s rotation path reads ALL of
    /// `getFingerDownPositions`, so the overlay's captured pointers are
    /// filtered out of what this seam reports).
    overlay: std::sync::Arc<std::sync::RwLock<crate::touch_overlay::TouchOverlay>>,
    /// issue 31 — the actual-window-size slot (the C++
    /// `window->getRenderWindow().getSize()` the touch controls'
    /// half-split reads, HGUpdate.cpp:598 — the 025g lesson: the
    /// ACTUAL window size, not the config's; `tick_fields` refreshes it
    /// every frame, the `render_size` method's docs).
    size: std::sync::Arc<RenderWindowSize>,
}

impl oh_game::renderer::Renderer for WindowTouchRenderer {
    fn finger_down_positions(&self) -> Vec<oh_math::ssvs::Vec2> {
        // issue 31 §2b hazard (a) — the overlay's captured pointers are
        // EXCLUDED (see the field's docs): the C++'s rotation path sees
        // the remaining fingers only.
        let consumed: std::collections::HashSet<u32> = self
            .overlay
            .read()
            .expect("touch overlay poisoned")
            .captured_ids()
            .into_iter()
            .collect();
        self.positions
            .read()
            .expect("touch sink poisoned")
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| slot.filter(|_| !consumed.contains(&(i as u32))))
            .collect()
    }

    // C++ `window->getFPS()` — the timer's `float fps` (0 before the
    // first `runFPS`), the previous frame's value by the C++'s loop
    // order (GameWindow.hpp:117-150: runDraw runs before runFPS).
    fn fps(&self) -> f32 {
        f32::from_bits(self.fps_bits.load(std::sync::atomic::Ordering::SeqCst))
    }

    // C++ `window->getInputState()[ssvs::KKey(mKey)]` (HGScripting.cpp:92)
    // — the window's live key state, the bounds-checked raw-code read.
    fn is_key_pressed(&self, key_code: i32) -> bool {
        self.input_state
            .read()
            .expect("input state sink poisoned")
            .is_key_pressed_code(key_code)
    }

    // C++ `window->getInputState()[ssvs::MBtn(mKey)]` (HGScripting.cpp:
    // 138) — the window's live button state, the bounds-checked
    // raw-code read.
    fn is_mouse_button_pressed(&self, button_code: i32) -> bool {
        self.input_state
            .read()
            .expect("input state sink poisoned")
            .is_button_pressed_code(button_code)
    }

    // C++ `window->getRenderWindow().getSize()` (HGUpdate.cpp:598 — the
    // touch controls' half-split) — the actual window size's live
    // slot (the 025g lesson: NOT the config's resolution).
    fn render_size(&self) -> oh_math::ssvs::Vec2 {
        let (w, h) = self.size.get();
        oh_math::ssvs::Vec2::new(w, h)
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
    /// 29u1 — the menu's cursor-visibility flag (the C++
    /// `MenuGame::mouseCursorVisible`, MenuGame.hpp:273): the single
    /// source of truth the menu's `play_selected_level` (hide) /
    /// `handle_mouse_wheel` (show) seam writes and the frame loop's
    /// key-press-hide / mouse-move-show (the C++'s other two cursor
    /// handlers, MenuGame.cpp:278 / :295) + the per-frame window sync
    /// read. Created in `WindowedClient::build` (the C++ ctor's
    /// `setMouseCursorVisible(true)`).
    cursor: std::rc::Rc<std::cell::Cell<bool>>,
}

impl MenuWindow for MenuWindowSource {
    fn width(&self) -> f32 {
        self.size.get().0
    }
    fn height(&self) -> f32 {
        self.size.get().1
    }
    fn set_mouse_cursor_visible(&self, visible: bool) {
        self.cursor.set(visible);
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

/// issue 32 §2.4 (emscripten only — the 025i pacing lesson, the
/// `tick_fields`'s `runFPS` clamp's docs): the max frame gap a web frame
/// may bill the timer as — 100 ms in frametime (6 FT, `getSecondsToFT`
/// × 60): the `request_animation_frame` loop stops while the tab is
/// hidden, and a gap past this caps the next frame's `frameTime` + drops
/// the timer's accumulated backlog (no fast-forward on the return; the
/// C++'s `maxLoops` cap alone only bounds the per-frame burst, not the
/// backlog's multi-frame drain).
#[cfg(target_os = "emscripten")]
const WEB_FRAME_GAP_CLAMP_FT: f32 = 6.0;

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
        // BOTH input managers (issue 010b) + the `Config::
        // loadAllJoystickBinds()` tail (MenuGame.cpp:3387) — the slot the
        // windowed `build` fills (29j1: the tail re-feeds the shared
        // `Joystick` machine with the re-bound `j_*` values).
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
        // C++ `Config::setFullscreen(window, value)` (MenuGame.cpp:1370-1373)
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
    fn resolution_width(&self) -> f32 {
        // C++ `Config::getWidth()` (Config.cpp:1102-1106).
        self.config.read().expect("config poisoned").get_width() as f32
    }
    fn resolution_height(&self) -> f32 {
        // C++ `Config::getHeight()` (Config.cpp:1107-1111).
        self.config.read().expect("config poisoned").get_height() as f32
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

// ---------------------------------------------------------------------------
// 029n1 (K6) — the pack-shader provider seam + the shader-bound draw.
// ---------------------------------------------------------------------------

/// The `PackShaderProvider` over the composition's `Arc<Assets>` (K6):
/// the C++'s `assets.getShaderByShaderId(id)` payload (source + the
/// uniform store) as the `oh-render` GL layer's
/// [`oh_render::pack_shaders::PackShaderSnapshot`] — the `oh-assets`
/// → `oh-render` type conversion at the seam (the `oh-render`
/// `PackShaderProvider` trait's implementation; `oh-render` cannot
/// depend on `oh-assets`, the cycle would run through `oh-game`).
pub struct AssetsPackShaderProvider {
    assets: Arc<oh_assets::Assets>,
}

impl AssetsPackShaderProvider {
    #[must_use]
    pub fn new(assets: Arc<oh_assets::Assets>) -> Self {
        Self { assets }
    }
}

impl oh_render::pack_shaders::PackShaderProvider for AssetsPackShaderProvider {
    fn pack_shader_snapshot(
        &self,
        id: usize,
    ) -> Option<oh_render::pack_shaders::PackShaderSnapshot> {
        let snap = self.assets.shader_snapshot(id)?;
        Some(oh_render::pack_shaders::PackShaderSnapshot {
            source: snap.source,
            shader_type: match snap.shader_type {
                oh_assets::ShaderType::Fragment => oh_render::shader::ShaderType::Fragment,
                oh_assets::ShaderType::Vertex => oh_render::shader::ShaderType::Vertex,
                oh_assets::ShaderType::Geometry => oh_render::shader::ShaderType::Geometry,
            },
            uniforms: snap
                .uniforms
                .into_iter()
                .map(|(name, value)| {
                    let uniform = match value {
                        oh_assets::UniformValue::F(x) => oh_render::shader::Uniform::F(x),
                        oh_assets::UniformValue::FVec2(x, y) => {
                            oh_render::shader::Uniform::FVec2(x, y)
                        }
                        oh_assets::UniformValue::FVec3(x, y, z) => {
                            oh_render::shader::Uniform::FVec3(x, y, z)
                        }
                        oh_assets::UniformValue::FVec4(x, y, z, w) => {
                            oh_render::shader::Uniform::FVec4(x, y, z, w)
                        }
                        oh_assets::UniformValue::I(x) => oh_render::shader::Uniform::I(x),
                        oh_assets::UniformValue::IVec2(x, y) => {
                            oh_render::shader::Uniform::IVec2(x, y)
                        }
                        oh_assets::UniformValue::IVec3(x, y, z) => {
                            oh_render::shader::Uniform::IVec3(x, y, z)
                        }
                        oh_assets::UniformValue::IVec4(x, y, z, w) => {
                            oh_render::shader::Uniform::IVec4(x, y, z, w)
                        }
                    };
                    (name, uniform)
                })
                .collect(),
        })
    }
}

/// `DrawTarget` over a live [`GlParts`] context (the windowed loop's real
/// target). `clear_black` is a real `glClear`; `draw_vertices` submits the
/// scene batches through [`GlBatchUploader`] (or — 029n1 K6 — through the
/// bound stage's compiled pack fragment shader, the C++'s
/// `sf::RenderStates{shader}`'s execution half); the sprite / text /
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
    /// 029n1 (K6): the pack-shader source (`None` = no pack shaders can
    /// be resolved — the headless/fallback targets draw everything
    /// through the default program; the C++'s `window == nullptr` / the
    /// headless `Config::getDisableGameRendering` guards leave no draw
    /// path at all, so the absence of a provider there is unobservable).
    provider: Option<Arc<dyn oh_render::pack_shaders::PackShaderProvider>>,
}

impl<'a> GlDrawTarget<'a> {
    /// `parts` must outlive the target (the `WindowedClient`'s window does
    /// — the target is a per-frame scratch built and dropped in `tick`).
    /// The GL objects (the batch uploader, the texture cache, the textured
    /// program, the font atlas, the pack-shader cache) live in the
    /// `GlParts` — the target only borrows them (the SG5 state-hygiene pin:
    /// nothing is re-created per frame).
    #[must_use]
    pub fn new(parts: &'a GlParts) -> Self {
        Self {
            parts,
            view: View::new(
                oh_math::ssvs::Vec2::new(parts.width as f32 / 2.0, parts.height as f32 / 2.0),
                oh_math::ssvs::Vec2::new(parts.width as f32, parts.height as f32),
            ),
            provider: None,
        }
    }

    /// 029n1 (K6): the pack-shader source for the shader-bound
    /// `draw_vertices` (the C++'s `assets` behind the `getRenderStates`
    /// lambda's `getShaderByShaderId`).
    #[must_use]
    pub fn with_provider(
        mut self,
        provider: Arc<dyn oh_render::pack_shaders::PackShaderProvider>,
    ) -> Self {
        self.provider = Some(provider);
        self
    }

    /// 029n1 (K6): the shader-bound `draw_vertices` half — `true` when the
    /// pack program drew (the caller skips the default program), `false`
    /// when the draw falls through to it (the invalid id, the non-fragment
    /// registry entry, the sticky compile failure — each the C++'s
    /// default-render continuation).
    fn draw_vertices_with_pack_shader(
        &mut self,
        vertices: &[Vertex],
        id: usize,
        provider: &Arc<dyn oh_render::pack_shaders::PackShaderProvider>,
    ) -> bool {
        // The C++'s `getShaderByShaderId`'s `nullptr` guard
        // (`HGGraphics.cpp:93-94` — no default-state early return here:
        // the states already carry the bound id, the fallback is the
        // default program, not `RenderStates::Default`'s blend — the C++
        // `RenderStates{shader}`'s blend IS the default Alpha anyway).
        let Some(snapshot) = provider.pack_shader_snapshot(id) else {
            return false;
        };
        // The C++ binds whatever `getShaderByShaderId` hands back; a
        // vertex-only / geometry-only `sf::Shader` on a desktop GL
        // context would leave the fragment stage at the GL default.
        // The port's scene draws are the fragment-shader surface (the
        // levels' `shdr_setActiveFragmentShader` binds `.frag` files —
        // the only binding entry point, `LuaScripting.cpp:1549-1557`),
        // so a non-fragment id draws through the default program (the
        // equivalent visual result, the scope guard's geometry-shader
        // exclusion).
        if snapshot.shader_type != oh_render::shader::ShaderType::Fragment {
            return false;
        }
        // The stored-uniform upload needs the snapshot's values through
        // the (possibly first) compile — keep the owned copy beside the
        // borrow of the compiled program.
        let uniforms = snapshot.uniforms.clone();
        let mut cache = self.parts.pack_shaders();
        match cache.program(&self.parts.gl, id, &snapshot) {
            Some(program) => {
                program.upload_uniforms(&self.parts.gl, &uniforms);
                program.draw(&self.parts.gl, vertices, self.view);
                true
            }
            // The sticky compile failure (logged once by the cache) — the
            // default program draws (the C++'s load-failure continuation).
            None => false,
        }
    }

    /// 029n1 (K6): the shader-bound `draw_text` half — `true` when the
    /// pack program drew the glyph quads (the font atlas on unit 0,
    /// the C++'s font texture — the C++'s `window->draw(text,
    /// getRenderStates(RenderStage::Text))`, `HGGraphics.cpp:283`),
    /// `false` when the draw falls through to the default textured-quad
    /// pass (the invalid id, the non-fragment registry entry, the
    /// sticky compile failure, the empty text / no-atlas case — each
    /// the C++'s default-render continuation).
    fn draw_text_with_pack_shader(
        &mut self,
        text: &Text,
        id: usize,
        provider: &Arc<dyn oh_render::pack_shaders::PackShaderProvider>,
    ) -> bool {
        let Some(snapshot) = provider.pack_shader_snapshot(id) else {
            return false;
        };
        if snapshot.shader_type != oh_render::shader::ShaderType::Fragment {
            return false;
        }
        let uniforms = snapshot.uniforms.clone();
        let mut cache = self.parts.pack_shaders();
        let Some(program) = cache.program(&self.parts.gl, id, &snapshot) else {
            return false;
        };
        // The stored-uniform upload (the C++'s bind-time half — the
        // values the hook / `onUpdate` stored before this draw).
        program.upload_uniforms(&self.parts.gl, &uniforms);
        // The glyph quads + the font atlas on unit 0 (the `GlPasses`
        // owns the atlas — the two `GlParts` slots are independent
        // `RefCell`s, the single-threaded GL's borrow soundness).
        let mut passes = self.parts.passes();
        passes.draw_text_with_pack_program(&self.parts.gl, text, self.view, program)
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
        // correction).
        self.parts.apply_blend(states.blend_mode);
        // 029n1 (K6): the bound per-stage pack fragment shader (the C++'s
        // `sf::RenderStates{assets.getShaderByShaderId(*fragmentShaderId)}`
        // — `HGGraphics.cpp:77`): draw through the compiled pack program
        // (the deferred load-time compile + the stored-uniform upload —
        // `oh_render::pack_shaders`'s module docs for the set-time vs
        // bind-time divergence), else fall through to the default
        // pass-through program (the C++'s load-failure path,
        // `Assets.cpp:889-892`'s "Failed to load shader" + the
        // default-render continuation).
        if let Some(id) = states.shader {
            if let Some(provider) = self.provider.clone() {
                if self.draw_vertices_with_pack_shader(vertices, id, &provider) {
                    return;
                }
            }
        }
        // The default pass-through program (the C++'s no-shader / the
        // load-failure draw — the batch uploads with the default
        // program). The uploader is the `GlParts`-shared one (the SG5
        // audit: the pre-025c per-frame re-creation leaked the GL
        // objects).
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
        // C++ `window->draw(text, states)` — the C++'s
        // `drawText(getRenderStates(RenderStage::Text))`
        // (`HGGraphics.cpp:283`): the font-atlas text pass
        // (issue-025c SG3) with the per-draw-call render states —
        // including 029n1 (K6)'s bound pack fragment shader (the
        // `text_shader.lua` level's `text.frag` — the hue-shifting
        // `font`-sampling text effect).
        self.parts.apply_blend(states.blend_mode);
        if let Some(id) = states.shader {
            if let Some(provider) = self.provider.clone() {
                if self.draw_text_with_pack_shader(text, id, &provider) {
                    return;
                }
            }
        }
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
    steam: Arc<dyn oh_game::seams::SteamSink>,
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
    /// issue 31 §2b — the gameplay touch overlay (the §2b mechanics'
    /// docs: the `touch_overlay` module): this `update_input`'s apply
    /// is the C++'s touch-controls position (HGUpdate.cpp:682 — after
    /// the keyboard binds' per-tick re-apply, before the game's
    /// `update_input`'s resolve + record) and its writes are the
    /// additive set-if-held (the C++'s `setInputImplIfFalse` pattern,
    /// generalized to the focus / swap buttons the C++'s rotation-only
    /// touch controls lack).
    touch_overlay: std::sync::Arc<std::sync::RwLock<crate::touch_overlay::TouchOverlay>>,
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
        // issue 31 §2b — the touch overlay's held inputs land LAST
        // among the input writes (the C++'s touch controls' position —
        // after the keyboard binds' per-tick re-apply; the `Type::
        // Always` off-closures are the authoritative clear, so the
        // additive set-if-held (the C++'s `setInputImplIfFalse`,
        // HGUpdate.cpp:589-597) makes the effective value per tick the
        // OR of keyboard and overlay — "exactly the C++-bound
        // semantics", spec §2.3: no new input kind, the replay's
        // record sees the same derived bits). The console gate mirrors
        // the C++'s `updateInput`'s `imguiLuaConsoleHasInput()` early
        // return (the game's own touch controls are gated the same
        // way). The ACTION buttons' edges (issue 039 F31 — the
        // `Tid::Exit` / `Tid::Replay` / `Tid::Restart` / `Tid::Pause`
        // binds' observables, HexagonGame.cpp:385-441) fire once per
        // physical press under the action gate (the whole game state —
        // the gameplay / action class split of the overlay's two gates,
        // the C++'s `Type::Once` per-frame poll); the console-gating
        // follows the C++'s `notInConsole` wrappers — the `Tid::Pause`
        // bind is the ungated one (HexagonGame.cpp:421-441), its menu's
        // `go_to_menu(true, false)` contract the `Tid::Exit` bind's
        // (029d1).
        {
            let mut o = self.touch_overlay.write().expect("touch overlay poisoned");
            if o.active() && o.enabled() && !self.game.lua_console_has_input() {
                if o.held(crate::touch_overlay::Button::RotateCCW) {
                    self.game.set_input_impl_ccw(true);
                }
                if o.held(crate::touch_overlay::Button::RotateCW) {
                    self.game.set_input_impl_cw(true);
                }
                if o.held(crate::touch_overlay::Button::Focus) {
                    self.game.set_input_focused(true);
                }
                if o.held(crate::touch_overlay::Button::Swap) {
                    self.game.set_input_swap(true);
                }
            }
            // The action edges (F31): taken unconditionally (the
            // pending edges drain every frame — the C++'s `Type::Once`
            // per-frame poll; an edge whose class gate is off at the
            // take is dropped, never deferred), applied under the
            // action gate.
            let edges = o.take_press_edges();
            if o.action_active() && o.enabled() {
                let console = self.game.lua_console_has_input();
                if edges[crate::touch_overlay::Button::Exit.idx()] && !console {
                    self.game.trigger_go_to_menu();
                }
                if edges[crate::touch_overlay::Button::Replay.idx()] && !console {
                    self.game.trigger_replay();
                }
                if edges[crate::touch_overlay::Button::Restart.idx()] && !console {
                    self.game.trigger_restart();
                }
                if edges[crate::touch_overlay::Button::Pause.idx()] {
                    self.game.trigger_pause();
                }
            }
        }
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
    /// issue 029b2: the window's mouse-wheel-event sink (the
    /// `oh_render::window` `wheel_events_sink` docs — the C++ `MenuGame::
    /// onEvent(MouseWheelScrolled)` handler's data path, MenuGame.cpp:
    /// 299-356; the `refresh_input` docs, below).
    wheel_events: std::sync::Arc<std::sync::RwLock<Vec<f32>>>,
    /// The window's entered-text sink (the C++ `runEvents`'s `TextEntered`
    /// dispatch — the `MenuGame::handle_text_entered` docs; the
    /// `refresh_input` docs, below).
    entered_text: std::sync::Arc<std::sync::RwLock<Vec<char>>>,
    /// issue 025f (D-031) — the soft-input (system IME) controller (the
    /// `SoftInput` docs): the `refresh_input`'s transition edge shows it
    /// on entering the text-entry states and hides it on leaving (the C++
    /// `isEnteringText` gate's port — `oh-menu` `States::is_entering_text`).
    soft_input: std::sync::Arc<dyn SoftInput>,
    /// 29i1 (F3/F4's `reloadAssets`): the pending `ignoreAllInputs` flag.
    /// The handler's `setIgnoreAllInputs(2)` fires INSIDE this adapter's
    /// `input_manager` `update` (the F3/F4 bind), so its callback can't
    /// borrow the manager reentrantly (the C++ has no such lock — plain
    /// `game.ignoreAllInputs`, MenuGame.cpp:3389-3402): it stashes the
    /// flag here instead, and the `update_input` applies it right after
    /// the `update` returns. Observationally identical: the manager reads
    /// `is_ignoring_all` only at `update` entry (the C++'s
    /// `Manager.hpp:31-33` `if(isIgnoringAll) return;` half).
    ignore_all_pending: std::rc::Rc<std::cell::Cell<Option<bool>>>,
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
        // `onEvent` handlers (MenuGame.cpp:519-535 `KeyReleased`,
        // :616-629 `MouseButtonReleased`, :673-686
        // `JoystickButtonReleased`): any key / mouse / joypad RELEASE
        // advances the boot screen — gated on `ignoreInputs != 0` (the
        // boot screens hold it, `setIgnoreAllInputs(1)` at
        // MenuGame.cpp:358) + the `LoadingScreen` / `EpilepsyWarning`
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
        // `unicode < 128` filter (MenuGame.cpp:265-272); the
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
            // (MenuGame.cpp:519-728): each release event runs the
            // handler — scenario 1 (a boot screen, the C++
            // `checkCloseBootScreens`), then the shared tail (the dialog
            // delay / box scenarios + the generic `--ignoreInputs`
            // decrement, the port's `MenuGame::handle_release_edge` —
            // issue 025g: the decrement is what unblocks the menu's
            // key binds after `returnToLevelSelection`'s
            // `setIgnoreAllInputs(1)` on `fnGoToMenu`; 29b3: the dialog
            // halves — the keyboard key's identity drives the Key
            // handler's close-key / Escape arms, the mouse / joystick /
            // finger releases carry none). The port's
            // documented once-per-frame boot advance holds (the
            // 025c/025f note above): at most ONE `advance_boot_screen`
            // per frame — two boot releases in one frame (the C++'s
            // per-event double-fire) is the documented edge case the
            // boot flow never exercises.
            let mut boot_advanced = false;
            let mut run_edge = |released: Option<Key>| {
                if self.menu.shared().borrow().ignore_inputs == 0 {
                    return;
                }
                match self.menu.state() {
                    States::LoadingScreen | States::EpilepsyWarning if !boot_advanced => {
                        self.menu.advance_boot_screen();
                        boot_advanced = true;
                    }
                    _ => self.menu.handle_release_edge(released),
                }
            };
            // The C++ `KeyReleased` handler (MenuGame.cpp:519-565) — the
            // keyboard releases carry the key identity (the dialog's
            // close-key / Escape arms consult it, 29b3).
            for key in edges.keys.iter() {
                run_edge(Some(*key));
            }
            // The C++ `MouseButtonReleased` / `JoystickButtonReleased`
            // handlers (MenuGame.cpp:616-645 / :673-702) — no key
            // identity (the dialog halves consult `keyToClose` only).
            for _ in edges.buttons.iter() {
                run_edge(None);
            }
            for _ in edges.fingers.iter() {
                run_edge(None);
            }
        }
        // C++ `MenuGame::onEvent(MouseWheelScrolled)` (MenuGame.cpp:
        // 299-356) — the per-event handler over the window's wheel-event
        // sink (the C++ `runEvents` per-event order — each recorded
        // delta runs the handler: the menu-navigation `wheelProgress`
        // accumulation, the LevelSelection `YScrollTo` scroll + the
        // `a35_eagerformore` end-scroll gag, the focus-held quick pack
        // change).
        {
            let mut events = self.wheel_events.write().expect("wheel sink poisoned");
            for delta in events.drain(..) {
                self.menu.handle_mouse_wheel(delta);
            }
        }
        // C++ `leftRightActionImpl`'s Shift modifier (MenuGame.cpp:1877-
        // 1878) — `isKeyPressed(LShift) || isKeyPressed(RShift)` (29b8c):
        // the C++ reads the keyboard live in the action; the port's
        // equivalent is this per-frame snapshot feed (the menu has no
        // window access in oh-menu — the `set_shift_held` hook).
        self.menu.set_shift_held(
            input.is_key_pressed(oh_config::Key::L_SHIFT)
                || input.is_key_pressed(oh_config::Key::R_SHIFT),
        );
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
        // 29i1 — the F3/F4 `ignoreAllInputs` deferral (the
        // `ignore_all_pending` field's docs): the manager reads
        // `is_ignoring_all` only at `update` entry, so applying the flag
        // after the `update` returns is observationally identical to the
        // C++'s in-fire `game.ignoreAllInputs`.
        if let Some(flag) = self.ignore_all_pending.take() {
            self.input_manager.borrow_mut().ignore_all_inputs(flag);
        }
    }

    // C++ `MenuGame` registers no `onPostUpdate` (only `onUpdate` /
    // `onDraw`, MenuGame.cpp:257-259) — the trait default (empty delegate).
}

/// The inverse of the C++ `static_cast<int>(Tid)` (HexagonGame.cpp:353,
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

    // Hardcoded Escape (HexagonGame.cpp:378-380): `goToMenu()` — the
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
    // consumer (MenuGame.cpp:2852-2854) now runs at the port's `update()`
    // tail (the `quit()` request path the windowed loop consumes — 29b7),
    // so a continuous 20s hold quits the game.
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
    // Editable Tid::Exit (MenuGame.cpp:975-983): the `isEnteringText()`
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
    // Editable Tid::Screenshot (MenuGame.cpp:987-988):
    // `mustTakeScreenshot = true` — the C++ consumer is `MenuGame::draw`
    // (issue 13b, unported).
    mgr.add_input(
        trigger_of(Tid::Screenshot),
        move |_ft| {
            // TODO(010b-menu-screenshot): C++ `mustTakeScreenshot = true`
            // (MenuGame.cpp:988) — the menu's screenshot consumer is the
            // unported `MenuGame::draw` (issue 13b).
        },
        |_| {},
        Type::Once,
        Tid::Screenshot as i32,
        Mode::Overlap,
    );
    // Hardcoded LAlt+Enter (MenuGame.cpp:989-997): the fullscreen toggle
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
        // C++ `.setPriorityUser(-1000)` (MenuGame.cpp:997): an `int` into
        // a `std::size_t` wraps to `usize::MAX - 999`.
        mgr.set_priority_user(idx, (usize::MAX - 999) as u64);
        // C++ `game.ignoreNextInputs()` (MenuGame.cpp:994): after this
        // action, the rest of the frame's binds are skipped
        // (Manager.hpp:40-43).
        mgr.set_ignore_next_on_fire(idx);
    }
    // Hardcoded Backspace (MenuGame.cpp:999-1001).
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
    // Hardcoded F1 (MenuGame.cpp:1003-1005): `addRemoveFavoriteLevel()`
    // (29b1 — the favorites toggle, wired through the 029c1 profile
    // store).
    {
        let m = menu.clone();
        mgr.add_input(
            Trigger::key(Key::F1),
            move |_ft| m.add_remove_favorite_level(),
            |_| {},
            Type::Once,
            -1,
            Mode::Overlap,
        );
    }
    // Hardcoded F2 (MenuGame.cpp:1007-1010): `switchToFromFavoriteLevels()`
    // (29b1 — the favorites view switch).
    {
        let m = menu.clone();
        mgr.add_input(
            Trigger::key(Key::F2),
            move |_ft| m.switch_to_from_favorite_levels(),
            |_| {},
            Type::Once,
            -1,
            Mode::Overlap,
        );
    }
    // Hardcoded F3 (MenuGame.cpp:1011-1013): `reloadAssets(false)` —
    // reload the LEVEL's assets (the handler's guard — LevelSelection +
    // no dialog + debug mode — is inside `MenuGame::reloadAssets`,
    // MenuGame.cpp:3092-3127).
    {
        let m = menu.clone();
        mgr.add_input(
            Trigger::key(Key::F3),
            move |_ft| m.reload_assets(false),
            |_| {},
            Type::Once,
            -1,
            Mode::Overlap,
        );
    }
    // Hardcoded F4 (MenuGame.cpp:1015-1017): `reloadAssets(true)` — reload
    // the PACK's assets.
    {
        let m = menu.clone();
        mgr.add_input(
            Trigger::key(Key::F4),
            move |_ft| m.reload_assets(true),
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
#[cfg(target_os = "emscripten")]
/// The web rAF-loop tick count (lane 026 W5 evidence): how many frames the
/// browser-driven loop has executed — read by `oh-web`'s `status()` probe so
/// a browser check can distinguish "the engine loop is alive" from "the page
/// loaded but the loop died" (the 26c `status()` shape, extended).
static WEB_TICKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(target_os = "emscripten")]
/// The probe's reader (lane 026).
#[must_use]
pub fn web_tick_count() -> u64 {
    WEB_TICKS.load(std::sync::atomic::Ordering::Relaxed)
}

/// issue 32 — the state probe's slot (the `WEB_TICKS` twin — lane 026's
/// `status()` evidence shape, extended): the composition's CURRENT
/// representative state, stored by the rAF loop once per frame — the
/// menu state's `States` discriminant (0..=9) or `10` for the game
/// state (the level running). `oh-web`'s `status()` reads it through
/// [`web_state`], so the browser check confirms each acceptance step
/// (boot → name entry → menu → level start) as the port's own
/// observable (the C++ `States` names — the C++ `MenuGame::States`
/// enum's discriminants, `oh_menu::states::States`).
#[cfg(target_os = "emscripten")]
static WEB_STATE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[cfg(target_os = "emscripten")]
/// The state probe's reader (issue 32): the C++ `States` name (the
/// menu) or `"game"` (the level running).
#[must_use]
pub fn web_state() -> &'static str {
    match WEB_STATE.load(std::sync::atomic::Ordering::Relaxed) {
        0 => "menu:ETLPNewBoot",
        1 => "menu:LoadingScreen",
        2 => "menu:EpilepsyWarning",
        3 => "menu:SLPSelectBoot",
        4 => "menu:SMain",
        5 => "menu:LevelSelection",
        6 => "menu:MOpts",
        7 => "menu:MOnline",
        8 => "menu:SLPSelect",
        9 => "menu:ETLPNew",
        _ => "game",
    }
}

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
    /// 29u1 — the menu's cursor-visibility flag (the C++
    /// `MenuGame::mouseCursorVisible`, MenuGame.hpp:273): the single
    /// source of truth the menu's `play_selected_level` (hide, the seam
    /// `MenuWindowSource` writes) / `handle_mouse_wheel` (show) and the
    /// frame loop's key-press-hide / mouse-move-show (the C++'s other two
    /// cursor handlers, MenuGame.cpp:278 / :295) update; `tick_fields`
    /// syncs it to the window's own flag each frame (the C++ applies it
    /// via `window.setMouseCursorVisible` in `MenuGame::
    /// setMouseCursorVisible`, MenuGame.cpp:3641-3644 — the port keeps the
    /// window's field the observable state). The C++ ctor's
    /// `setMouseCursorVisible(true)` = the `true` start.
    cursor_visible: std::rc::Rc<std::cell::Cell<bool>>,
    /// 29u1 — the cursor's last-frame detection state (the frame loop's
    /// key-press-hide / mouse-move-show diff, held here so the static
    /// `tick_fields` can read/write it through references): the previous
    /// frame's raw mouse pixel (`Option::None` before the first frame)
    /// + the previous frame's key-pressed bitset (`InputState::get_keys`).
    cursor_last_mouse: std::cell::Cell<Option<Vec2>>,
    cursor_last_keys: std::cell::Cell<[bool; oh_render::input::KEY_BITS]>,
    /// C++ `hg` (main.cpp:414-421) — the composition's engine handle.
    pub game: Rc<HexagonGame<GameApiState>>,
    /// The 029n1 (K6) pack-shader source: the shared `HGAssets` the
    /// GL layer's per-stage fragment shaders compile + upload from
    /// (the C++'s single `HGAssets` — `assets.getShaderByShaderId`,
    /// `HGGraphics.cpp:77`; the `tick`'s `GlDrawTarget` takes the
    /// [`AssetsPackShaderProvider`] over this `Arc`).
    assets: Arc<oh_assets::Assets>,
    /// The shared process config (the C++'s `hg::Config` singleton —
    /// `build`'s parameter): the egui console footer's statics
    /// seed/write back into it (HGUpdate.cpp:1549, 1555-1557) and the
    /// `tick` / `run` frame bodies pass it to the console pass.
    pub config: Arc<RwLock<Config>>,
    /// 029e3 — the window's FPS slot (the C++ `window->getFPS()` /
    /// `gameEngine->getFPS()` = the timer's `float fps`, TimerBase.hpp:23)
    /// as raw `f32` bits: `tick_fields` stores `timer.get_fps()`'s bits
    /// right after the C++'s `runFPS` position (GameWindow.hpp:148);
    /// the engine's `WindowTouchRenderer` (the C++'s `window` stand-in
    /// for the `Renderer::fps()` seam — the FPS text in `updateText`)
    /// reads it on the next tick. The C++'s `TimerBase::fps{0}` initial
    /// value = the slot's `0.0f32.to_bits()` start (the first frame's
    /// FPS text renders "0" — the C++'s first-frame value).
    fps_bits: std::sync::Arc<std::sync::atomic::AtomicU32>,
    /// issue 31 — the gameplay touch overlay's state machine (the
    /// module docs' §2b/§5 mechanics): the frame's feed + draw halves
    /// drive / paint it, the game slot's per-tick apply reads its held
    /// state (the bound-input writes), the engine's rotation path reads
    /// its consumed pointers (the double-count hazard). `pub`: the
    /// scripted-touch tests set the enable flag + inspect the state
    /// (spec §4.1's in-process verification — the desktop's enable flag
    /// is what the tests flip to exercise the overlay, spec §5).
    pub touch_overlay: std::sync::Arc<std::sync::RwLock<crate::touch_overlay::TouchOverlay>>,
    /// issue 31 — the overlay's on-screen buttons (the key-icon
    /// textures + the exit label — the `tick_fields`'s draw half).
    touch_overlay_buttons:
        std::rc::Rc<std::cell::RefCell<crate::touch_overlay::TouchOverlayButtons>>,
    /// issue 31 — the shared touch sink the overlay's pointer feed
    /// diffs frame-to-frame (spec §5's "Android feeds it from the
    /// existing shared touch sink" — the SAME sink the menu's 9-zone
    /// grid + the engine's rotation path read; the diff's state lives
    /// in `last_touch_positions`).
    touch_positions: std::sync::Arc<
        std::sync::RwLock<[Option<oh_math::ssvs::Vec2>; oh_render::input::FINGER_COUNT]>,
    >,
    /// issue 31 — the previous frame's touch sink (the pointer feed's
    /// diff state: `None → Some` = a `Down`, `Some → None` = an `Up`;
    /// a move is a no-op — pointer capture keeps the bound pointer on
    /// its button, the `touch_overlay` module docs).
    last_touch_positions:
        std::cell::Cell<[Option<oh_math::ssvs::Vec2>; oh_render::input::FINGER_COUNT]>,
    /// Issue 014b — the egui console overlay's lazy slot (`None` until
    /// the first shown frame — `crate::lua_console_ui`'s module docs:
    /// Gates). Shared by the per-event hook (the C++'s
    /// `Imgui::processEvent` feed, HexagonGame.cpp:335) and the frame
    /// bodies (`tick_fields`'s post-update / draw halves). On emscripten
    /// (W2) the slot is the degenerate `()`.
    ilc: IlcSlot,
    /// C++ `steamManager` (main.cpp:251) — the composition's Steam manager
    /// (issue 020): shared with the engine (the `steam_manager` seam) and
    /// driving the menu's `MenuGame::init`-lifecycle calls
    /// (MenuGame.cpp:793-794 — `oh-menu` is extend-only, so the
    /// composition makes them at the same lifecycle point).
    /// W2: the trait object (the `oh_steam::SteamManager` concrete type is
    /// desktop-only; emscripten passes `NoneSteamSink`).
    pub steam: Arc<dyn oh_game::seams::SteamSink>,
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
        steam: Arc<dyn oh_game::seams::SteamSink>,
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
        // Web boot trace (lane 026 W5) — see `client.rs` (the
        // emscripten-only `[channel]` boot lines through `oh_game::log_sink`).
        #[cfg(target_os = "emscripten")]
        oh_game::log_sink::emit_log("web", "windowed build entry");
        let timer = TimerStatic::new(TIME_STEP, TIME_SLICE, 50.0);

        // ------------------------------------------------------------------
        // C++ main.cpp:413-421 — the engine (`window` = true): the C++
        // passes `&steamManager` first (issue 020 — the composition's
        // shared manager); the profile surface is the SAME shared
        // `Assets` store the menu's `RealMenuAssets` below holds (issue
        // 029c1 — the C++'s single `HGAssets`, main.cpp:381/:434: a run's
        // PB writes are visible in the menu and persist through the
        // `main_client` pSaveAll exit guard), the audio / discord null
        // equivalents are the composition's. The shared `Config` is the C++'s single
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
        // (MenuGame.cpp:265-272) over it). Desktop and Android: the fold
        // records identically (the winit `KeyboardInput` `text` field).
        let entered_text_sink: std::sync::Arc<std::sync::RwLock<Vec<char>>> =
            std::sync::Arc::new(std::sync::RwLock::new(Vec::new()));
        window.set_entered_text_sink(entered_text_sink.clone());
        // issue 029b2: the mouse-wheel-event sink (the C++ `MenuGame::
        // onEvent(MouseWheelScrolled)` handler's data path — MenuGame.
        // cpp:299-356; the window's fold records one delta per
        // `MouseWheelScrolled`; the menu state's `refresh_input` drain
        // runs the C++ handler per recorded delta, the C++ `runEvents`
        // per-event order). Desktop and Android: the fold records
        // identically (the winit `MouseWheel` event).
        let wheel_events_sink: std::sync::Arc<std::sync::RwLock<Vec<f32>>> =
            std::sync::Arc::new(std::sync::RwLock::new(Vec::new()));
        window.set_wheel_events_sink(wheel_events_sink.clone());
        // 029e3 — the window's FPS slot (the C++ `TimerBase::fps{0}`
        // initial value, TimerBase.hpp:23 — `0.0f32.to_bits()`; the
        // client field's docs carry the loop-order semantics).
        let fps_bits = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0.0f32.to_bits()));
        // issue 31 §1 — the window's live `InputState` slot (the C++
        // `window->getInputState()` read surface for
        // `u_isKeyPressed` / `u_isMouseButtonPressed`): installed on the
        // window's event fold (the input-mutating arms republish the
        // window's current state) and read by the engine through the
        // renderer seam below (the seed = the window's state at install,
        // the C++'s ctor-time window state).
        let input_state = std::sync::Arc::new(std::sync::RwLock::new(*window.get_input_state()));
        window.set_input_state_sink(input_state.clone());
        // issue 31 §2b — the gameplay touch overlay's state machine
        // (the module docs): the platform-neutral pointer-feed machine
        // shared by the frame's feed + draw halves (the `tick_fields`
        // halves below), the game slot's per-tick apply (the
        // bound-input writes — the C++'s touch-controls position), and
        // the engine's rotation path (the consumed-pointer exclusion —
        // the §2b double-count hazard). Spec §5's enable flag: the
        // pointer feed exists on the android target (the winit fold's
        // touch events) AND the emscripten target (issue 32 — the
        // canvas's pointer events feed the SAME shared touch sink this
        // frame's feed diffs, spec §5's "31 and 32 meet here"); the
        // desktop keeps it off (§4.4: the desktop build has no overlay
        // and no behavior change — `WindowEvent::Touch` never arrives
        // there either).
        let touch_overlay = std::sync::Arc::new(std::sync::RwLock::new({
            #[cfg(any(target_os = "android", target_os = "emscripten"))]
            {
                let mut overlay = crate::touch_overlay::TouchOverlay::new();
                overlay.set_enabled(true);
                overlay
            }
            #[cfg(not(any(target_os = "android", target_os = "emscripten")))]
            crate::touch_overlay::TouchOverlay::new()
        }));
        // issue 025g — the ACTUAL-window-size slot (the `RenderWindowSize`
        // docs): created here (moved up from its former position below —
        // the `WindowTouchRenderer`'s `render_size` seam half, the
        // touch controls' half-split) and refreshed by `tick_fields`
        // every frame.
        let render_size = std::sync::Arc::new(RenderWindowSize::new(
            window.get_width(),
            window.get_height(),
        ));
        let renderer: Option<std::sync::Arc<dyn oh_game::renderer::Renderer + 'static>> =
            Some(std::sync::Arc::new(WindowTouchRenderer {
                positions: touch_sink.clone(),
                fps_bits: fps_bits.clone(),
                input_state,
                overlay: touch_overlay.clone(),
                size: render_size.clone(),
            }));
        // C++ main.cpp:264-269 — `discordManager.emplace()` (only when NOT
        // headless; the windowed path is always non-headless).
        // W2: the `IpcDiscordCore` is `#[cfg(not(target_os = "emscripten"))]`
        // (W3); the web path uses the no-SDK `NoDiscordCore`.
        let discord = discord.unwrap_or_else(|| {
            #[cfg(not(target_os = "emscripten"))]
            {
                Arc::new(oh_discord::DiscordManager::new(
                    oh_discord::IpcDiscordCore::new(oh_discord::DISCORD_APP_ID),
                )) as Arc<dyn oh_discord::Discord>
            }
            #[cfg(target_os = "emscripten")]
            {
                Arc::new(oh_discord::DiscordManager::new(oh_discord::NoDiscordCore))
                    as Arc<dyn oh_discord::Discord>
            }
        });

        // C++ main.cpp:392-401 — `hg::Audio audio{ soundBufferGetter,
        // musicPathGetter }` (both getters capture `&assets`), then
        // `audio.setSoundVolume(Config::getSoundVolume());
        // audio.setMusicVolume(Config::getMusicVolume());`. The REAL audio is
        // shared with the engine (`compose_with_config` below) and the menu
        // (`MenuAudioSink`, the C++ `mg.emplace(..., audio, ...)` at
        // main.cpp:434). issue 025e: this replaces the previous `None`
        // engine-audio / `NoopAudio` menu-sink (the missed subsystem).
        // issue 041: no longer gated off emscripten — the web's audio works
        // via the OpenAL backend behind the `AudioDevice` seam, so the REAL
        // audio is shared with the engine and the menu on EVERY target
        // (desktop / Android / web), exactly as the C++ `mainClient` does.
        let audio = Arc::new(oh_audio::Audio::new(assets.clone()));
        {
            let c = config.read().expect("config poisoned");
            audio.set_sound_volume(c.get_sound_volume());
            audio.set_music_volume(c.get_music_volume());
        }

        // 29j1 — the C++ `Joystick` global's poller (the `sf::Joystick`
        // controller-0 read): the desktop backend is gilrs
        // (`crate::joystick::RealJoystick` — ONE object shared with the
        // menu's `MenuJoystick` seam below; the engine's `Arc<dyn
        // Joystick>` and the menu's `Rc<dyn MenuJoystick>` are two views
        // of the same `Arc<RealJoystick>`); Android / wasm keep the
        // no-op `NoJoystick` (the C++'s `isConnected(0) == false` state
        // — the Android build's SFML has no gamepad backend in this
        // port, documented in `crate::joystick`'s module docs).
        #[cfg(not(any(target_os = "android", target_family = "wasm")))]
        let joystick_concrete = std::sync::Arc::new(crate::joystick::RealJoystick::new());
        #[cfg(not(any(target_os = "android", target_family = "wasm")))]
        let joystick: std::sync::Arc<dyn oh_game::seams::Joystick + 'static> =
            joystick_concrete.clone() as std::sync::Arc<dyn oh_game::seams::Joystick>;
        #[cfg(any(target_os = "android", target_family = "wasm"))]
        let joystick: std::sync::Arc<dyn oh_game::seams::Joystick + 'static> =
            std::sync::Arc::new(oh_game::seams::NoJoystick);

        let composed = compose_with_config(
            assets.clone(),
            base_dir.to_path_buf(),
            true, /* window */
            // issue 029c1 — the C++'s single `HGAssets` (main.cpp:381/
            // :434): the SAME `Arc<Assets>` the menu's `RealMenuAssets`
            // below wraps, so a run's PB writes reach the menu and the
            // exit-save guard (the `NullProfiles` no-op stand-in is gone
            // from the shipping windowed composition).
            Arc::new(RealProfiles::new(assets.clone())),
            config.clone(),
            steam.clone(),
            Some(discord.clone()),
            renderer,
            Some(audio.clone()),
            // issue 029a1 — the C++ `death_saveReplay` filesystem half
            // (HexagonGame.cpp:1067-1097): the windowed entries chdir to
            // the game-data root before composing (the desktop
            // `dispatch::run`'s main.cpp:653 port; Android's
            // `chdir_to_base_dir`), so rooting on `base_dir` is the C++'
            // CWD-relative `Replays/...` — a death persists
            // `Replays/<levelId>/<diff>x/<name>.z` in the internal data
            // dir on Android, beside `Profiles/` / `config.json` (the
            // folders the C++ creates at main.cpp:275-276).
            Arc::new(crate::replay_store::RealReplayStore::new(
                base_dir.to_path_buf(),
            )),
            joystick.clone(),
        );
        #[cfg(target_os = "emscripten")]
        oh_game::log_sink::emit_log(
            "web",
            "engine + menu composed (the four callback links bound)",
        );

        // 29j1 — the menu's `MenuJoystick` seam over the SAME shared
        // poller (the C++'s one global `Joystick`; desktop: the
        // `RealMenuJoystick` `Arc` view of the same `RealJoystick` the
        // engine's seam wraps; Android / wasm: the no-op
        // `NoMenuJoystick` — so both states observe one controller).
        #[cfg(not(any(target_os = "android", target_family = "wasm")))]
        let menu_joystick: Rc<dyn oh_menu::seams::MenuJoystick> =
            Rc::new(crate::joystick::RealMenuJoystick::new(joystick_concrete));
        #[cfg(any(target_os = "android", target_family = "wasm"))]
        let menu_joystick: Rc<dyn oh_menu::seams::MenuJoystick> =
            Rc::new(oh_menu::seams::NoMenuJoystick);

        // The C++ `initText` fonts (HexagonGame.cpp:289-290:
        // `Assets/OpenSquare-Regular.ttf` / `OpenSquare-Bold.ttf`).
        let assets_dir = base_dir.join("Assets");
        let font = Font::load(&assets_dir.join("OpenSquare-Regular.ttf"))
            .map_err(|e| format!("failed to load Assets/OpenSquare-Regular.ttf: {e}"))?;
        let font_bold = Font::load(&assets_dir.join("OpenSquare-Bold.ttf"))
            .map_err(|e| format!("failed to load Assets/OpenSquare-Bold.ttf: {e}"))?;
        let app_draw = Rc::new(RefCell::new(AppDraw::new(
            &config.read().expect("config poisoned"),
            &assets_dir,
            font.clone(),
            font_bold.clone(),
        )));
        // The menu draw bridge's fonts (issue-013b) — the C++ shares the
        // `initText` fonts with the game (HexagonGame.cpp:289-290). The
        // credits-bar PNGs load from the same `Assets/` folder (29w1).
        let menu_draw = RefCell::new(crate::menu_draw::MenuDraw::new(
            &assets_dir,
            &font,
            &font_bold,
        ));

        // ------------------------------------------------------------------
        // C++ main.cpp:427-434 — the MenuGame (the Discord / HexagonClient
        // arguments are their null equivalents: issues 21/17;
        // `NoopNetwork` / `NoopAudio` / the real `Assets` via
        // `RealMenuAssets` — one `_RELEASE` load shared with the engine,
        // as the C++ shares one `HGAssets`). The C++ ctor's `steamManager`
        // argument's `unlock_achievement` half (29b2 / 29b8: the
        // `a35_eagerformore` wheel-scroll + `a23_login` call sites) runs
        // on the composition's shared `SteamManager` (the same manager
        // the engine's `steam_manager` seam uses — the C++'s one global).
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
        // → `HexagonGame::refreshTrigger`, HexagonGame.cpp:461-465) + the
        // menu-side `game.refreshTrigger` half of the rebind callback
        // (MenuGame.cpp:1233-1242). The ported seam passes (name, id) in
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
        // 29i1 — the F3/F4 `ignoreAllInputs` deferral latch (the
        // `MenuSlotAdapter::ignore_all_pending` field's docs): the handler
        // fires inside the menu manager's `update`, so its callback can't
        // re-borrow the manager; the `update_input` applies the flag after
        // the `update` returns.
        let ignore_all_pending = std::rc::Rc::new(std::cell::Cell::new(None));
        // 29u1 — the menu's cursor-visibility flag (the C++
        // `MenuGame::mouseCursorVisible`): created at `true` (the C++
        // ctor's `setMouseCursorVisible(true)`, MenuGame.cpp:245); the
        // `MenuWindowSource` seam writes it (the menu's `play_selected_level`
        // hide / `handle_mouse_wheel` show) and the frame loop reads it
        // (the key-press-hide / mouse-move-show + the per-frame window sync).
        let cursor_visible = std::rc::Rc::new(std::cell::Cell::new(true));

        let menu_audio: Rc<dyn AudioSink> = {
            // issue 041: the REAL audio sink on every target (desktop /
            // Android / web) — the web's no-op `NoopAudioSink` is gone now
            // that the OpenAL backend behind the `AudioDevice` seam makes the
            // web's menu blips play.
            Rc::new(MenuAudioSink {
                audio: audio.clone(),
                config: config.clone(),
            })
        };
        let menu = Rc::new(MenuGame::new(
            Rc::new(RealMenuAssets::from_arc(assets.clone())),
            Rc::new(ConfigMenuAdapter::new(
                config.clone(),
                refresh_binds_slot.clone(),
            )),
            Rc::new(NoopNetwork),
            menu_audio,
            menu_touch,
            menu_joystick,
            Rc::new(MenuWindowSource {
                size: render_size.clone(),
                cursor: cursor_visible.clone(),
            }),
            new_game_cb,
            trigger_refresh_cb,
            rich_presence_cb,
            {
                let steam = steam.clone();
                Some(Box::new(move |id| {
                    // C++ `steamManager.unlock_achievement(id)` — the
                    // C++'s two menu-side call sites (`"a35_eagerformore"`
                    // @MenuGame.cpp:335; `"a23_login"` @:2591).
                    steam.unlock_achievement(id);
                }))
            },
            {
                // 29i1 (the `ignore_all_pending` deferral — the field's
                // docs): the C++'s `setIgnoreAllInputs` `game.ignoreAll`
                // half (MenuGame.cpp:3389-3402), stashed instead of
                // applied (the reentrancy) and applied by the
                // `update_input` after the `update` the handler fired in.
                let ignore_all_pending = ignore_all_pending.clone();
                Some(Box::new(move |ignore| {
                    ignore_all_pending.set(Some(ignore));
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
        // `Config::loadAllJoystickBinds()` tail (MenuGame.cpp:3387) is now
        // live — 29j1: after a rebind the 12 `j_*` values are re-loaded
        // into the shared `Joystick` machine, exactly the C++ order
        // (keyboard trigger refresh loop, then the joystick tail)).
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
                // C++ `Config::loadAllJoystickBinds()` (MenuGame.cpp:3387 →
                // Config.cpp:1609-1615) — re-feed the shared machine with
                // the (possibly re-bound) `j_*` values.
                config
                    .read()
                    .expect("config poisoned")
                    .load_all_joystick_binds();
            }));
        }

        // ------------------------------------------------------------------
        // The two `GameState` adapters + the `GameEngine` switcher
        // (main.cpp:448/461/479/502: the four `window->setGameState`
        // call sites — the `-p -l` SUCCESS path sets the game state via
        // the `fnHGNewGame` at :448, main.cpp:442-449; only the FAILURE
        // path never does — the port's documented deviation, module
        // docs).
        let return_to_menu = Arc::new(AtomicU8::new(0));
        let menu_adapter: Rc<dyn GameState> = Rc::new(MenuSlotAdapter {
            menu: menu.clone(),
            input_manager: menu_manager.clone(),
            menu_draw,
            released_edges: released_sink.clone(),
            wheel_events: wheel_events_sink.clone(),
            entered_text: entered_text_sink.clone(),
            soft_input,
            ignore_all_pending,
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
            touch_overlay: touch_overlay.clone(),
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
            cursor_visible,
            cursor_last_mouse: std::cell::Cell::new(None),
            cursor_last_keys: std::cell::Cell::new([false; oh_render::input::KEY_BITS]),
            game,
            assets,
            config,
            fps_bits,
            touch_overlay,
            touch_overlay_buttons: Rc::new(RefCell::new(
                crate::touch_overlay::TouchOverlayButtons::new(&assets_dir, font.clone()),
            )),
            touch_positions: touch_sink.clone(),
            last_touch_positions: std::cell::Cell::new([None; oh_render::input::FINGER_COUNT]),
            steam,
            discord: Some(discord),
            engine_slot,
            interrupt: Arc::new(AtomicBool::new(false)),
            _app_draw: app_draw,
            composed,
            // W2: the egui console slot is desktop-only; emscripten uses
            // the degenerate `()`.
            #[cfg(not(target_os = "emscripten"))]
            ilc: Rc::new(RefCell::new(None)),
            #[cfg(target_os = "emscripten")]
            ilc: (),
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

    /// C++ `main.cpp:471-537` — the boot dispatch (module docs: the
    /// null-state deviations — the `-p -l` FAILURE path + the replay
    /// decompress-failure path).
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
                        &replay.pack_id.to_string_lossy(),
                        &replay.level_id.to_string_lossy(),
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
                    // issue 020). The tail `loadCommandLineLevel` →
                    // `playSelectedLevel()` (MenuGame.cpp:1778) fires the
                    // `fnHGNewGame` seam (main.cpp:442-449: `hg.newGame(
                    // ...)` + `window->setGameState(hg.getGame())`), so a
                    // SUCCESSFUL boot lands in the Game slot — the `-p -l`
                    // "immediately run level" behavior, main.cpp:96-97,
                    // :104-105.
                    self.steam.set_rich_presence_in_menu();
                    self.steam.update_hardcoded_achievements();
                    let booted = self.menu.init_command_line_level(false, pack, level);
                    self.menu_rich_presence(); // C++ mg->init's Discord lines (:796)
                    if !booted {
                        // DOCUMENTED DEVIATION (module docs): the C++
                        // FAILURE path (the `loadCommandLineLevel` early
                        // returns — invalid pack / no levels / invalid
                        // level / no profiles, MenuGame.cpp:1701-1765 —
                        // BEFORE the `playSelectedLevel` tail) leaves
                        // `gameState` null (the first frame's
                        // `assert(isValid())` would fire); the port
                        // points the window at the menu (the tail left
                        // it in `LevelSelection`, MenuGame.cpp:1775).
                        self.engine.set_game_state(GameSlot::Menu);
                    }
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
    /// W2: desktop-only (the web loop is a plain `request_animation_frame`
    /// — no signal handler).
    #[cfg(not(target_os = "emscripten"))]
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
    /// `sig` is the C++ handler's `int s` (main.cpp:341-344, SIGINT);
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
    #[cfg(not(target_os = "emscripten"))]
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
            assets,
            config,
            fps_bits,
            touch_overlay,
            touch_overlay_buttons,
            touch_positions,
            last_touch_positions,
            engine_slot: _engine_slot,
            interrupt,
            _app_draw: app_draw,
            ilc,
            render_size,
            cursor_visible,
            cursor_last_mouse,
            cursor_last_keys,
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
                    &assets,
                    &config,
                    &ilc,
                    &render_size,
                    &cursor_visible,
                    &cursor_last_mouse,
                    &cursor_last_keys,
                    &fps_bits,
                    &app_draw,
                    &touch_overlay,
                    &touch_overlay_buttons,
                    &touch_positions,
                    &last_touch_positions,
                )
            },
        );
        // C++ main.cpp:594-595 — the `main_client` caller prints the
        // "Finished" line (the ported `main_client` does, after this
        // returns).
    }

    /// Emscripten (W2): drive the emscripten `Window::run` (the
    /// `request_animation_frame` loop — the winit event loop's web
    /// replacement). The egui console hook is OFF on web (W2) — the
    /// `IlcSlot` is the degenerate `()` and the frame body's console
    /// halves are gated off, so the loop is the plain `on_frame` drive.
    /// Drop order matches the desktop [`Self::run`].
    ///
    /// `move`: the emscripten `Window::run`'s `on_frame` is `'static`
    /// (the rAF loop outlives this function), so the closure owns the
    /// captured pieces (the desktop `run_with_event_hook`'s `on_frame`
    /// is not `'static` and borrows them instead). `composed` (the
    /// heap-pinned `LuaContext` every `SharedLua` carrier points into —
    /// the `SharedLua` contract, compose.rs) is captured into the closure
    /// too: the loop outlives `main` (emscripten's runtime stays alive
    /// after `main` returns), so the `LuaContext` must live with the loop,
    /// not die at this function's end. It is referenced LAST in the body,
    /// so the closure captures (and drops) it AFTER the states'
    /// `Rc<HexagonGame>` carriers (the contract's drop order).
    #[cfg(target_os = "emscripten")]
    pub fn run(self) {
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
            cursor_visible,
            cursor_last_mouse,
            cursor_last_keys,
            fps_bits,
            // 029n1 (K6) + 031: fields the shared `tick_fields` contract gained
            // after lane 026 branched (the pack-shader provider's `Assets`, and
            // the touch overlay's pieces). This `run` compiles ONLY for the
            // emscripten target, so the host gate never saw the drift - the
            // emscripten build is what catches it.
            assets,
            touch_overlay,
            touch_overlay_buttons,
            touch_positions,
            last_touch_positions,
        } = self;
        let mut timer = timer;
        window.run(move |window| {
            #[cfg(target_os = "emscripten")]
            WEB_TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // issue 32 — the state probe's per-frame store (the rAF loop is
            // the composition's single-threaded frame — the store is the
            // current representative state: the menu's `States`
            // discriminant, or `10` for the game state).
            #[cfg(target_os = "emscripten")]
            WEB_STATE.store(
                match engine.current() {
                    GameSlot::Menu => menu.state() as u32,
                    GameSlot::Game => 10,
                },
                std::sync::atomic::Ordering::Relaxed,
            );
            Self::tick_fields(
                window,
                &mut timer,
                &engine,
                &menu,
                &interrupt,
                &game,
                &assets,
                &config,
                &ilc,
                &render_size,
                &cursor_visible,
                &cursor_last_mouse,
                &cursor_last_keys,
                &fps_bits,
                &_app_draw,
                &touch_overlay,
                &touch_overlay_buttons,
                &touch_positions,
                &last_touch_positions,
            );
            let _ = &composed;
        });
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
            &self.assets,
            &self.config,
            &self.ilc,
            &self.render_size,
            &self.cursor_visible,
            &self.cursor_last_mouse,
            &self.cursor_last_keys,
            &self.fps_bits,
            &self._app_draw,
            &self.touch_overlay,
            &self.touch_overlay_buttons,
            &self.touch_positions,
            &self.last_touch_positions,
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
            &self.assets,
            &self.config,
            &self.ilc,
            &self.render_size,
            &self.cursor_visible,
            &self.cursor_last_mouse,
            &self.cursor_last_keys,
            &self.fps_bits,
            &self._app_draw,
            &self.touch_overlay,
            &self.touch_overlay_buttons,
            &self.touch_positions,
            &self.last_touch_positions,
        )
    }

    /// The C++ `window->getFPS()` (GameWindow.hpp:335-338 — the timer's
    /// `float fps`, the LAST frame's `runFPS` result; 0 before the first
    /// frame — the `TimerBase::fps{0}` initial value, TimerBase.hpp:23).
    /// 029e3: the value the engine's `Renderer::fps()` seam (the FPS text
    /// in `updateText`) reads every tick — the shared `fps_bits` slot
    /// `tick_fields` publishes after the C++'s `runFPS` position.
    #[must_use]
    pub fn window_fps(&self) -> f32 {
        f32::from_bits(self.fps_bits.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// 029e4 — the composition's `AppDraw` (the C++'s `HexagonGame`
    /// window-side draw state): `tick_fields` refreshes its `fps`
    /// (the `onRenderStage` frametime's `60.f / fps` divisor — the C++'
    /// `window->getFPS()`, HGGraphics.cpp:76) every frame; tests read it
    /// to pin that refresh (the `_app_draw` field is held for drop order,
    /// not API — this accessor is the test's window onto it).
    #[must_use]
    pub fn app_draw(&self) -> &Rc<RefCell<crate::bridge::AppDraw>> {
        &self._app_draw
    }

    /// The frame body over the client's pieces (the `tick` wrapper is the
    /// `&self` convenience; `run` calls this with the moved-out fields).
    /// `#[allow]`: the C++'s frame body genuinely needs all eleven pieces
    /// (the two states' frame halves + the console's post-update/draw
    /// halves + the phase timers + the window's FPS publication + the
    /// composition's `AppDraw` fps refresh) - a context struct would just
    /// hide the C++'s parameter list (cf. `HexagonGame::new`'s allow). On
    /// emscripten (W2) the console + SIGINT halves are gated off, so the
    /// `interrupt` / `game` / `config` / `ilc` params are unused there.
    #[allow(clippy::too_many_arguments)]
    #[cfg_attr(target_os = "emscripten", allow(unused_variables))]
    fn tick_fields(
        window: &mut Window,
        timer: &mut TimerStatic,
        engine: &Rc<GameEngine>,
        menu: &Rc<MenuGame>,
        interrupt: &AtomicBool,
        game: &Rc<HexagonGame<GameApiState>>,
        // 029n1 (K6) — the pack-shader source (the C++'s `assets` behind
        // the `getRenderStates` lambda): the per-frame `GlDrawTarget`
        // takes the `AssetsPackShaderProvider` over it.
        assets: &Arc<oh_assets::Assets>,
        config: &Arc<RwLock<Config>>,
        ilc: &IlcSlot,
        render_size: &std::sync::Arc<RenderWindowSize>,
        // 29u1 — the menu's cursor-visibility flag (the C++
        // `MenuGame::mouseCursorVisible`): the frame's key-press-hide /
        // mouse-move-show (the C++'s other two cursor handlers,
        // MenuGame.cpp:278 / :295 — menu-state only) write it, and the
        // per-frame sync below applies it to the window's own flag (the C++
        // `MenuGame::setMouseCursorVisible` → `window.setMouseCursorVisible`,
        // MenuGame.cpp:3641-3644).
        cursor_visible: &std::rc::Rc<std::cell::Cell<bool>>,
        cursor_last_mouse: &std::cell::Cell<Option<Vec2>>,
        cursor_last_keys: &std::cell::Cell<[bool; oh_render::input::KEY_BITS]>,
        // 029e3 — the window's FPS slot (the C++ `window->getFPS()`;
        // the `WindowTouchRenderer`'s docs): the frame's `runFPS` result
        // is published here so the NEXT frame's engine tick (the
        // `Renderer::fps()` read in the FPS text) sees the C++'s
        // previous-frame value (GameWindow.hpp:117-150's loop order).
        fps_bits: &std::sync::Arc<std::sync::atomic::AtomicU32>,
        // 029e4 — the composition's `AppDraw` (the C++'s
        // `HexagonGame` window-side draw state): its `fps` (the
        // `onRenderStage` frametime's `60.f / fps` divisor — the C++'
        // `window->getFPS()`, HGGraphics.cpp:76) is refreshed from the
        // timer at the same `runFPS` position below.
        app_draw: &std::rc::Rc<std::cell::RefCell<crate::bridge::AppDraw>>,
        // issue 31 — the gameplay touch overlay (the module docs): the
        // feed half diffs `touch_positions` against `last_touch_positions`
        // into the pointer feed before the update burst; the draw half
        // paints the active buttons (the §2.5 layout) after the game's
        // draw.
        touch_overlay: &std::sync::Arc<std::sync::RwLock<crate::touch_overlay::TouchOverlay>>,
        touch_overlay_buttons: &std::rc::Rc<
            std::cell::RefCell<crate::touch_overlay::TouchOverlayButtons>,
        >,
        touch_positions: &std::sync::Arc<
            std::sync::RwLock<[Option<oh_math::ssvs::Vec2>; oh_render::input::FINGER_COUNT]>,
        >,
        last_touch_positions: &std::cell::Cell<
            [Option<oh_math::ssvs::Vec2>; oh_render::input::FINGER_COUNT],
        >,
    ) {
        // C++ `while(gameEngine->isRunning())` loop head (GameWindow.hpp:
        // 121) — the SIGINT flag (the safe port of the C++ handler, main
        // .cpp:340-349; module docs: the SIGINT bullet): the handler's
        // three log lines (main.cpp:343-348) around the deferred
        // `globalWindow.stop()` (main.cpp:347) + the window close that
        // ends the port's winit loop (the C++'s loop condition is the
        // engine's `isRunning()`; the port's is `window.open`). Observed
        // here, the C++'s `isRunning()` condition reads the handler's
        // `running = false` at the same loop-head position. Desktop only —
        // the web loop is a plain `request_animation_frame` (no signal
        // handler, W2); the `interrupt` flag is never set on emscripten,
        // so this gate is absent there (behavior-identical).
        #[cfg(not(target_os = "emscripten"))]
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
        // docs). The vsync half of that recreation IS live-applicable: the
        // C++'s `recreateWindow` ends with `renderWindow.setVerticalSync
        // Enabled(vsync)` (GameWindow.hpp:94 — the pinned SFML's
        // `eglSwapInterval(display, vsync ? 1 : 0)`), and the options'
        // "vsync" toggle (MenuGame.cpp:1425-1426 → `Config::setVsync`,
        // Config.cpp:685-689) writes the CONFIG here — the C++'s
        // `mustRecreate` re-applies it on the next iteration's recreation.
        // The port applies the same observable change — the live EGL swap
        // interval — without recreating the winit window (the documented
        // recreation gap; the full recreation is lane 029w's scope).
        // The `!=` gate: the EGL request only on an ACTUAL change (the
        // per-frame cost is a single `RwLock` read + a bool compare).
        let cfg_vsync = config.read().expect("config poisoned").get_vsync();
        if window.get_vsync() != cfg_vsync {
            window.set_vsync(cfg_vsync);
        }
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
        // 29u1 — the C++'s menu-state cursor handlers (MenuGame.cpp:274-
        // 307): any `KeyPressed` hides the cursor (:278), a `MouseMoved`
        // that actually moved re-shows it (:295). The port observes these
        // PER FRAME (the menu-state event feed's once-per-frame
        // approximation of the C++'s per-event handlers — the documented
        // 025c/025f note): a key newly pressed this frame (the `input`
        // snapshot vs the previous frame's bitset) hides; a changed raw
        // mouse pixel re-shows (the hide-then-show order makes a same-frame
        // key+move resolve to "show" — the mouse presence is the cursor's
        // intent). Menu-state only — the C++'s handlers are on the menu
        // state's event bus (the game state never re-shows, so a started
        // level's hidden cursor persists, exactly the C++ behavior).
        if engine.current() == GameSlot::Menu {
            let cur_keys = input.get_keys();
            let last_keys = cursor_last_keys.get();
            if cur_keys.iter().zip(last_keys.iter()).any(|(c, l)| *c && !l) {
                cursor_visible.set(false); // C++ :278 — KeyPressed → hide
            }
            cursor_last_keys.set(*cur_keys);
            let mp = window.mouse_pixel();
            // Only a REAL change from the previous frame re-shows (the
            // C++ `actuallyMoved` = the delta != 0): the first frame has
            // no previous pixel (`None`) — not a move, so no show.
            if let Some(last) = cursor_last_mouse.get() {
                if last != mp {
                    cursor_visible.set(true); // C++ :295 — MouseMoved → show
                }
            }
            cursor_last_mouse.set(Some(mp));
        }
        // issue 31 — the gameplay touch overlay's per-frame feed + gate
        // (spec §2b/§5): the pointer feed is the frame-to-frame diff of
        // the shared touch sink's levels (spec §5's "Android feeds it
        // from the existing shared touch sink" — a `None → Some` is a
        // `Down` at the new position, a `Some → None` is an `Up`; a
        // move is a no-op — pointer capture keeps the bound pointer on
        // its button, the `touch_overlay` module docs). The gate is the
        // spec's gameplay-only half (the buttons only while playing —
        // the game slot's `started` and NOT `has_died` (not the menu /
        // boot / results screens) and not a replay playback — spec
        // §2.4/§2.5); deactivation clears every capture (the state
        // transition's semantics, the module docs).
        {
            let (view_w, view_h) = render_size.get();
            let positions = touch_positions.read().expect("touch sink poisoned");
            let last = last_touch_positions.get();
            let mut o = touch_overlay.write().expect("touch overlay poisoned");
            let status = game.get_status();
            o.set_active(
                engine.current() == GameSlot::Game
                    && status.started
                    && !status.has_died
                    && !game.in_replay(),
            );
            // issue 039 (F31) — the action buttons' gate: the whole
            // GAME state (playing + the `has_died` results screen + the
            // replay playback — the C++'s `Tid::Exit` / `Tid::Restart`
            // / `Tid::Replay` / `Tid::Pause` binds' reachability,
            // HexagonGame.cpp:385-441; deliberately WITHOUT the
            // `!game.in_replay()` half — the replay playback's exit /
            // restart / replay is the C++'s `in_replay`'s skip-and-
            // re-fire and must stay reachable on touch).
            o.set_action_active(engine.current() == GameSlot::Game && status.started);
            o.set_view(view_w, view_h);
            for (i, (prev, cur)) in last.iter().zip(positions.iter()).enumerate() {
                let id = i as u32;
                match (prev, cur) {
                    (None, Some(p)) => o.pointer_event(&crate::touch_overlay::PointerEvent {
                        id,
                        position: *p,
                        phase: crate::touch_overlay::Phase::Down,
                    }),
                    (Some(_), None) => o.pointer_event(&crate::touch_overlay::PointerEvent {
                        id,
                        position: oh_math::ssvs::Vec2::new(0.0, 0.0),
                        phase: crate::touch_overlay::Phase::Up,
                    }),
                    _ => {}
                }
            }
            last_touch_positions.set(*positions);
        }
        engine.refresh_input(input);
        timer.run_update(|ft| {
            engine.update_input(input, ft);
            engine.update_from_timer(ft);
        });
        engine.on_post_update();
        // 29u1 — apply the menu's cursor state to the window (the C++
        // `MenuGame::setMouseCursorVisible` → `window.setMouseCursorVisible`,
        // MenuGame.cpp:3641-3644): the frame's key/mouse detection (above,
        // pre-update — the C++'s `runEvents` position) + the menu's
        // `play_selected_level` hide / `handle_mouse_wheel` show (during the
        // update burst, via the `MenuWindowSource` seam) all wrote the
        // shared flag; this applies the frame's final value to the window's
        // own flag (the observable `Window::mouse_cursor_visible`).
        window.set_mouse_cursor_visible(cursor_visible.get());
        // C++ `postUpdate_ImguiLuaConsole`'s egui UI tail (issue 014b /
        // D-022 — `crate::lua_console_ui`'s module docs: the per-frame
        // order + the C++'s one-frame `wantCapture*` lag). The C++ runs
        // it INSIDE `HexagonGame::postUpdate` (HexagonGame.cpp:331), so
        // the game-state gate is the C++'s state gate. OFF on emscripten
        // (W2 — the console is desktop-only).
        #[cfg(not(target_os = "emscripten"))]
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
            // 029n1 (K6): the pack-shader source (the C++'s `assets`
            // behind the `getRenderStates` lambda) — the bound stages'
            // `.frag` programs compile + upload from it at draw time.
            let mut target = GlDrawTarget::new(gl)
                .with_provider(Arc::new(AssetsPackShaderProvider::new(assets.clone())));
            engine.draw(&mut target);
            // issue 31 + issue 039 F31 — the overlay's buttons (the §2.5
            // layout: the GAMEPLAY buttons only while playing — the
            // gate's draw half; the F31 ACTION row (exit / replay /
            // restart / pause) under the action gate — the whole game
            // state, the game-over screen's controls) — after the
            // game's draw, before the console's paint — a dev overlay
            // may cover the controls, the controls must not cover the
            // game). The buttons live in the VIEW's pixel space (the
            // 025g lesson) — the `draw`'s 1:1 view; the ON state is the
            // button's held state (the C++'s `drawKeyIcons`'s opacity
            // pair, the module docs); the per-class visibility is the
            // `draw`'s own (the `active` / `action_active` halves).
            {
                let o = touch_overlay.read().expect("touch overlay poisoned");
                // The draw's gate = the operational gate (spec §5's
                // "a pointer feed exists" + an enable flag) + EITHER
                // class's gate (the F31 half: the game-over screen's
                // action row draws while the gameplay row does not —
                // a disabled overlay (the desktop — §4.4: no overlay,
                // no behavior change) draws nothing even when a gate
                // alone would pass).
                if o.enabled() && (o.active() || o.action_active()) {
                    let (view_w, view_h) = render_size.get();
                    touch_overlay_buttons.borrow_mut().draw(
                        &mut target,
                        &o,
                        view_w,
                        view_h,
                        game.get_color_text(),
                    );
                }
            }
            // C++ `drawImguiLuaConsole` (HGGraphics.cpp:318-336) — the
            // console's GL paint, the C++ `draw()`'s LAST step
            // (HGGraphics.cpp:315), after the game's own draw. OFF on
            // emscripten (W2 — the console is desktop-only).
            #[cfg(not(target_os = "emscripten"))]
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
        // Emscripten (issue 32 §2.4 — the 025i pacing lesson): the rAF
        // loop STOPS while the tab is hidden (the browser pauses it —
        // the Android activity-pause's web twin); without a cap the
        // whole hidden span accumulates as the next frame's `frameTime`
        // and the timer's backlog (`time`) fast-forwards the game on
        // return (a 60 s hidden tab = 3600 FT = 3550 ticks drained over
        // the following frames, the C++ `maxLoops` cap only limiting the
        // per-frame burst). The clamp: a frame gap past 100 ms (6 FT —
        // six normal 16.7 ms rAF frames; comfortably beyond a jank
        // frame, far below any real pause) caps the `frameTime` AND
        // drops the accumulated backlog (`TimerStatic::reset` — the
        // C++ `reset` zeroes `time` + `loops`; normal frames, gap ≤
        // 100 ms, take the C++ path verbatim). Web-only: the desktop /
        // android loops keep the C++'s faithful accumulation (their
        // pause semantics are the platform's, not the tab's).
        #[cfg(target_os = "emscripten")]
        {
            timer.run_frame_time();
            if timer.get_frame_time() > WEB_FRAME_GAP_CLAMP_FT {
                timer.set_frame_time(WEB_FRAME_GAP_CLAMP_FT);
                timer.reset();
            }
            timer.run_fps();
        }
        #[cfg(not(target_os = "emscripten"))]
        {
            timer.run_frame_time();
            timer.run_fps();
        }
        // 029e3 — publish the just-computed window FPS (the C++'
        // `runFPS`'s `fps = getFTToFPS(frameTime)` result, now in
        // `timer.get_fps()`) into the shared slot the engine's
        // `Renderer::fps()` seam reads on its NEXT tick — the C++'
        // `window->getFPS()` is exactly this value by the loop order
        // above (a frame's draw/update sees the previous frame's).
        fps_bits.store(timer.get_fps().to_bits(), Ordering::SeqCst);
        // 029e4 — the SAME value drives the `onRenderStage` frametime
        // (`60.f / fps`, HGGraphics.cpp:76) through the composition's
        // `AppDraw` — the pre-fix `fps` stayed at its `AppDraw::new`
        // initial (60.0, never updated), so the hook's frametime was the
        // constant `1/60` instead of the C++'s live `60.f /
        // window->getFPS()` (the first frame's C++ value is `60.f / 0.f`
        // = +inf, the C++'s `TimerBase::fps{0}`).
        app_draw.borrow_mut().fps = timer.get_fps();

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
#[cfg_attr(target_os = "emscripten", allow(unused_variables))]
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
    // Emscripten (issue 32 §2.3): NOT here — the web window's size IS
    // the real canvas size (the CSS box × the device pixel ratio,
    // `Window::create`'s measurement — the 025g "config size ≠ real
    // size" invariant), and the restore would squish it back to the
    // config's resolution: unlike the winit targets, a web window
    // delivers no creation-time `Resized` that would correct it (the
    // DOM `resize` listener covers browser resizes only). The menu
    // layout + the hit tests would then run in the config space while
    // the pointer positions run in the real one — every hit test off.
    #[cfg(not(target_os = "emscripten"))]
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
    // — the C++'s stb-based `loadFromFile`, always RGBA8). OFF on
    // emscripten (W2 — a browser canvas has no window icon; the tab's
    // favicon is the PAGE's, not the canvas's).
    #[cfg(not(target_os = "emscripten"))]
    {
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
            Rc::new(oh_menu::NoMenuJoystick),
            oh_menu::FakeWindow::new(),
            None,
            None,
            None,
            None, // `unlock_achievement` — the 29b2/29b8 Steam seam (None in the tests)
            None,
            Rc::new(oh_menu::layout::LinearFontHeight),
        ));
        let assets_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../_RELEASE/Assets");
        let f = oh_render::font::Font::load(&assets_dir.join("OpenSquare-Regular.ttf"))
            .expect("OpenSquare-Regular.ttf must load");
        let released_edges = Arc::new(RwLock::new(ReleasedEdges::default()));
        let adapter = MenuSlotAdapter {
            menu: menu.clone(),
            input_manager: Rc::new(RefCell::new(Manager::new())),
            menu_draw: RefCell::new(crate::menu_draw::MenuDraw::new(&assets_dir, &f, &f)),
            released_edges: released_edges.clone(),
            wheel_events: Arc::new(RwLock::new(Vec::new())),
            entered_text: Arc::new(RwLock::new(Vec::new())),
            soft_input: Arc::new(NoOpSoftInput),
            ignore_all_pending: Rc::new(Cell::new(None)),
            was_entering_text: Cell::new(false),
        };

        // The C++ ctor body holds the boot gate (MenuGame.cpp:358).
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
            Rc::new(oh_menu::NoMenuJoystick),
            oh_menu::FakeWindow::new(),
            None,
            None,
            None,
            None, // `unlock_achievement` — the 29b2/29b8 Steam seam (None in the tests)
            None,
            Rc::new(oh_menu::layout::LinearFontHeight),
        ));
        let assets_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../_RELEASE/Assets");
        let f = oh_render::font::Font::load(&assets_dir.join("OpenSquare-Regular.ttf"))
            .expect("OpenSquare-Regular.ttf must load");
        let released_edges = Arc::new(RwLock::new(ReleasedEdges::default()));
        let adapter = MenuSlotAdapter {
            menu: menu.clone(),
            input_manager: Rc::new(RefCell::new(Manager::new())),
            menu_draw: RefCell::new(crate::menu_draw::MenuDraw::new(&assets_dir, &f, &f)),
            released_edges: released_edges.clone(),
            wheel_events: Arc::new(RwLock::new(Vec::new())),
            entered_text: Arc::new(RwLock::new(Vec::new())),
            soft_input: Arc::new(NoOpSoftInput),
            ignore_all_pending: Rc::new(Cell::new(None)),
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
    /// `onEvent(TextEntered)` handler (MenuGame.cpp:265-272 — the
    /// `unicode < 128` filter) → `enteredChars` → `update()`'s
    /// `isEnteringText`-gated accumulation (`process_entered_chars`,
    /// MenuGame.cpp:2862-2878) → `enteredStr`. This is what makes the
    /// fresh-install profile creation (ETLPNewBoot → `ok_action` →
    /// SMain) reachable from device input (the pre-fix hole: the
    /// `TextEntered` fold arm was a no-op, so `enteredStr` could never
    /// fill and `ok_action`'s `create_profile_from_entered` always
    /// returned `ProfileCreateResult::Skipped`).
    #[test]
    fn entered_text_chain_accumulates_the_entered_str() {
        use oh_render::window::Event;

        let menu = Rc::new(MenuGame::new(
            FakeAssets::new(),
            FakeConfig::new(),
            Rc::new(NoopNetwork),
            oh_menu::RecordingAudio::new(),
            Rc::new(NoopTouch),
            Rc::new(oh_menu::NoMenuJoystick),
            oh_menu::FakeWindow::new(),
            None,
            None,
            None,
            None, // `unlock_achievement` — the 29b2/29b8 Steam seam (None in the tests)
            None,
            Rc::new(oh_menu::layout::LinearFontHeight),
        ));
        let assets_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../_RELEASE/Assets");
        let f = oh_render::font::Font::load(&assets_dir.join("OpenSquare-Regular.ttf"))
            .expect("OpenSquare-Regular.ttf must load");
        let entered_text = Arc::new(RwLock::new(Vec::new()));
        let released_edges = Arc::new(RwLock::new(ReleasedEdges::default()));
        let adapter = MenuSlotAdapter {
            menu: menu.clone(),
            input_manager: Rc::new(RefCell::new(Manager::new())),
            menu_draw: RefCell::new(crate::menu_draw::MenuDraw::new(&assets_dir, &f, &f)),
            released_edges: released_edges.clone(),
            wheel_events: Arc::new(RwLock::new(Vec::new())),
            entered_text: entered_text.clone(),
            soft_input: Arc::new(NoOpSoftInput),
            ignore_all_pending: Rc::new(Cell::new(None)),
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
        // once-per-frame drain (the C++ handler, MenuGame.cpp:265-272).
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

        // The handler's `unicode < 128` filter (MenuGame.cpp:268): the
        // non-ASCII char never reaches `enteredChars`.
        Window::apply_event(&mut window, Event::TextEntered { character: 'é' });
        adapter.refresh_input(&InputState::new());
        assert!(menu.shared().borrow().entered_chars.is_empty());

        // The C++ gate (MenuGame.cpp:2860): outside the entering-text
        // states the accumulation is suppressed (the `enteredChars.
        // clear()` at :2878 is UNCONDITIONAL).
        menu.shared().borrow_mut().state = States::SMain;
        Window::apply_event(&mut window, Event::TextEntered { character: 'x' });
        adapter.refresh_input(&InputState::new());
        menu.update(0.01);
        assert_eq!(menu.shared().borrow().entered_str, "DAF");
        assert!(menu.shared().borrow().entered_chars.is_empty());
    }

    /// issue 029b2 (the C++ `MenuGame::onEvent(MouseWheelScrolled)`
    /// handler, MenuGame.cpp:299-356): the full wheel chain — the winit
    /// `WindowEvent::MouseWheel` → `map_window_event` (the SFML sign
    /// convention — the `Event::MouseWheelScrolled` variant's docs) →
    /// the REAL `Window::apply_event` fold (one entry per event, the
    /// pre-fix hole: the arm was `{ .. } => {}`, the wheel worked
    /// NOWHERE) → the shared wheel-event sink → the REAL
    /// `MenuSlotAdapter::refresh_input` drain (per recorded delta, the
    /// C++ `runEvents` per-event order) → the C++ handler (the SMain
    /// `wheelProgress` accumulation — a full line past ±1 fires
    /// `upAction` / `downAction`, MenuGame.cpp:344-354).
    #[test]
    fn wheel_chain_drives_the_menu_navigation() {
        use oh_render::window::map_window_event;
        use winit::event::{DeviceId, MouseScrollDelta, TouchPhase, WindowEvent};

        let assets = {
            let fa = FakeAssets::new();
            fa.add_profile("P1");
            fa
        };
        let menu = Rc::new(MenuGame::new(
            assets,
            FakeConfig::new(),
            Rc::new(NoopNetwork),
            oh_menu::RecordingAudio::new(),
            Rc::new(NoopTouch),
            Rc::new(oh_menu::NoMenuJoystick),
            oh_menu::FakeWindow::new(),
            None,
            None,
            None,
            None, // `unlock_achievement` — the 29b2/29b8 Steam seam (None in the tests)
            None,
            Rc::new(oh_menu::layout::LinearFontHeight),
        ));
        let assets_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../_RELEASE/Assets");
        let f = oh_render::font::Font::load(&assets_dir.join("OpenSquare-Regular.ttf"))
            .expect("OpenSquare-Regular.ttf must load");
        let wheel_events = Arc::new(RwLock::new(Vec::new()));
        let adapter = MenuSlotAdapter {
            menu: menu.clone(),
            input_manager: Rc::new(RefCell::new(Manager::new())),
            menu_draw: RefCell::new(crate::menu_draw::MenuDraw::new(&assets_dir, &f, &f)),
            released_edges: Arc::new(RwLock::new(ReleasedEdges::default())),
            wheel_events: wheel_events.clone(),
            entered_text: Arc::new(RwLock::new(Vec::new())),
            soft_input: Arc::new(NoOpSoftInput),
            ignore_all_pending: Rc::new(Cell::new(None)),
            was_entering_text: Cell::new(false),
        };
        let mut window = Window::headless(800, 480);
        window.set_wheel_events_sink(wheel_events.clone());

        // Boot through the two boot screens to SMain (one profile →
        // SLPSelectBoot → ok).
        adapter.menu.advance_boot_screen();
        adapter.menu.advance_boot_screen();
        adapter.menu.ok_action();
        assert_eq!(adapter.menu.state(), States::SMain);
        assert_eq!(
            adapter.menu.selected_item_name().as_deref(),
            Some("LEVEL SELECT")
        );

        // Wheel-UP lines (the winit `LineDelta(0, +1)` — X11 Button4 /
        // wheel up, the SFML-positive sign) across frames: 0.5 + 0.5 +
        // 0.1 of progress, the action firing on the 0.1 past +1.
        for delta in [0.5, 0.5, 0.1] {
            Window::apply_event(
                &mut window,
                map_window_event(&WindowEvent::MouseWheel {
                    device_id: DeviceId::dummy(),
                    delta: MouseScrollDelta::LineDelta(0.0, delta),
                    phase: TouchPhase::Moved,
                })
                .expect("MouseWheel must map"),
            );
            adapter.refresh_input(&InputState::new());
        }
        assert_eq!(
            adapter.menu.selected_item_name().as_deref(),
            Some("EXIT"),
            "the up-action wrapped from index 0 to the last item"
        );
        assert_eq!(adapter.menu.shared().borrow().wheel_progress, 0.0);

        // Wheel DOWN: the negative line delta (the C++'s `wheelProgress
        // < -1.f` arm → `downAction`): from EXIT (index 4) it wraps to
        // the first item.
        Window::apply_event(
            &mut window,
            map_window_event(&WindowEvent::MouseWheel {
                device_id: DeviceId::dummy(),
                delta: MouseScrollDelta::LineDelta(0.0, -2.0),
                phase: TouchPhase::Moved,
            })
            .expect("MouseWheel must map"),
        );
        adapter.refresh_input(&InputState::new());
        assert_eq!(
            adapter.menu.selected_item_name().as_deref(),
            Some("LEVEL SELECT")
        );
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
            Rc::new(oh_menu::NoMenuJoystick),
            oh_menu::FakeWindow::new(),
            None,
            None,
            None,
            None, // `unlock_achievement` — the 29b2/29b8 Steam seam (None in the tests)
            None,
            Rc::new(oh_menu::layout::LinearFontHeight),
        ));
        let assets_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../_RELEASE/Assets");
        let f = oh_render::font::Font::load(&assets_dir.join("OpenSquare-Regular.ttf"))
            .expect("OpenSquare-Regular.ttf must load");
        let soft_input: Arc<dyn SoftInput> = rec.clone();
        let adapter = MenuSlotAdapter {
            menu: menu.clone(),
            input_manager: Rc::new(RefCell::new(Manager::new())),
            menu_draw: RefCell::new(crate::menu_draw::MenuDraw::new(&assets_dir, &f, &f)),
            released_edges: Arc::new(RwLock::new(ReleasedEdges::default())),
            wheel_events: Arc::new(RwLock::new(Vec::new())),
            entered_text: Arc::new(RwLock::new(Vec::new())),
            soft_input,
            ignore_all_pending: Rc::new(Cell::new(None)),
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
