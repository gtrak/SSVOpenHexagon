//! Issue 15 step (c): the WINDOWED wiring — headless-first tests (spec §5):
//! the state-switching loop driven with the REAL `TimerStatic` over fake
//! states, and the `WindowedClient` boot dispatch with a fake (headless)
//! window. The real GL + winit run is MANUAL (spec §5):
//! `Window::create` degrades to a headless window without a display, and
//! these tests inject a `Window::headless` so no display / GL / event loop
//! is touched at all.
//!
//! Issue 010b: the input-manager wiring — scripted `InputState` sequences
//! drive the menu binds (Enter → PLAY), the engine's `Tid::*` triggers
//! (rotation flags, screenshot, force-restart, pause), and the
//! replay no-leak invariant (the live manager sets the flags, but the
//! replay's recorded input drives the movement).

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use oh_app::game_state::{GameEngine, GameSlot, GameState};
use oh_app::windowed::{frame, WindowedClient};
use oh_config::{Config, Key, TIME_STEP};
use oh_menu::States;
use oh_render::graphics::DrawTarget;
use oh_render::timer::TimerStatic;
use oh_render::view::View;
use oh_render::window::Window;

// ---------------------------------------------------------------------
// Fake states (spec §5: "two fake states; assert update/draw order +
// counts") — driven through the REAL [`TimerStatic`] + [`frame`].
// ---------------------------------------------------------------------

/// No-op target (the fake states record their own calls; no GL).
struct NullTarget;
impl DrawTarget for NullTarget {
    fn clear_black(&mut self) {}
    fn set_view(&mut self, _view: View) {}
    fn draw_vertices(&mut self, _v: &[oh_render::draw::Vertex], _s: oh_render::draw::RenderStates) {
    }
    fn draw_sprite(&mut self, _s: &oh_render::texture::Sprite) {}
    fn draw_text(&mut self, _t: &oh_render::font::Text, _s: oh_render::draw::RenderStates) {}
    fn draw_rectangle(
        &mut self,
        _r: &oh_render::types::RectangleShape,
        _s: oh_render::draw::RenderStates,
    ) {
    }
    fn save_screenshot(&mut self, _p: &str) {}
}

/// No-op state (fills the engine's slots before the real fakes are wired
/// in — see the indirection note on [`Indirect`]).
struct Noop;
impl GameState for Noop {
    fn update_from_timer(&self, _ft: f32) {}
    fn draw(&self, _t: &mut dyn DrawTarget) {}
}

/// Delegates to a state installed AFTER the engine exists: the real fake
/// states need an `Rc<GameEngine>` to switch through (the C++ `setGameState`
/// from inside a tick), while the engine needs the states at construction
/// — the `RefCell` break the cycle.
struct Indirect {
    inner: RefCell<Rc<dyn GameState>>,
}
impl GameState for Indirect {
    fn update_from_timer(&self, ft: f32) {
        self.inner.borrow().update_from_timer(ft);
    }
    fn on_post_update(&self) {
        self.inner.borrow().on_post_update();
    }
    fn draw(&self, t: &mut dyn DrawTarget) {
        self.inner.borrow().draw(t);
    }
}

/// Records its `update` / `on_post_update` / `draw` calls under `name`;
/// after `switch_after` OWN updates, fires `engine.set_game_state(to)` —
/// the C++ `fnHGNewGame` shape (main.cpp:442-449): a state switch from
/// INSIDE a timer tick, mid-burst (the C++ reads its `gameState` pointer
/// at every call site, GameEngine.hpp:40-56).
struct Switching {
    name: &'static str,
    log: Rc<RefCell<Vec<String>>>,
    engine: Rc<GameEngine>,
    switch_after: Option<(GameSlot, usize)>,
}
impl GameState for Switching {
    fn update_from_timer(&self, ft: f32) {
        self.log
            .borrow_mut()
            .push(format!("{}:update({ft})", self.name));
        if let Some((to, at)) = self.switch_after {
            let own = self
                .log
                .borrow()
                .iter()
                .filter(|e| e.starts_with(&format!("{}:update", self.name)))
                .count();
            if own == at {
                self.engine.set_game_state(to);
            }
        }
    }
    fn on_post_update(&self) {
        self.log
            .borrow_mut()
            .push(format!("{}:post_update", self.name));
    }
    fn draw(&self, _t: &mut dyn DrawTarget) {
        self.log.borrow_mut().push(format!("{}:draw", self.name));
    }
}

/// Spec §5 (a): state-switching routing with the real timer.
///
/// C++ frame order pinned (GameWindow.hpp:117-150): N×`updateFromTimer`
/// (the timer burst) → `onPostUpdate` (GameEngine.hpp:80-86) → `draw`
/// (GameEngine.hpp:89-94) → `runFPS`. The switch fires on the menu's 2nd
/// tick of a 4-tick burst: the C++'s pointer read per call means the
/// burst CONTINUES on the new state (no frame-boundary flush).
#[test]
fn real_timer_drives_switched_states_in_the_cpp_frame_order() {
    let log = Rc::new(RefCell::new(Vec::new()));

    let menu_slot: Rc<Indirect> = Rc::new(Indirect {
        inner: RefCell::new(Rc::new(Noop)),
    });
    let game_slot: Rc<Indirect> = Rc::new(Indirect {
        inner: RefCell::new(Rc::new(Noop)),
    });
    let engine = Rc::new(GameEngine::new(menu_slot.clone(), game_slot.clone()));

    let menu_state: Rc<dyn GameState> = Rc::new(Switching {
        name: "menu",
        log: log.clone(),
        engine: engine.clone(),
        switch_after: Some((GameSlot::Game, 2)),
    });
    let game_state: Rc<dyn GameState> = Rc::new(Switching {
        name: "game",
        log: log.clone(),
        engine: engine.clone(),
        switch_after: None,
    });
    menu_slot.inner.replace(menu_state as Rc<dyn GameState>);
    game_slot.inner.replace(game_state as Rc<dyn GameState>);

    let mut window = Window::headless(800, 600);
    // The test's own timer (the client's is the C++'s 1/240 s install):
    // step = time_slice = 0.25 ⇒ 1.0 s of frame time = 4 ticks.
    let mut timer = TimerStatic::new(0.25, 0.25, 50.0);
    let mut target = NullTarget;

    // Frame 1: 4 ticks — the 2nd menu tick switches to the game mid-burst.
    timer.set_frame_time(1.0);
    frame(&mut window, &engine, &mut timer, &mut target);

    // Frame 2: 0.5 s = 2 ticks, all game.
    timer.set_frame_time(0.5);
    frame(&mut window, &engine, &mut timer, &mut target);

    assert_eq!(
        *log.borrow(),
        vec![
            "menu:update(0.25)",
            "menu:update(0.25)", // ← set_game_state(Game) here (mid-burst)
            "game:update(0.25)",
            "game:update(0.25)",
            "game:post_update", // after the burst, before the draw
            "game:draw",
            "game:update(0.25)",
            "game:update(0.25)",
            "game:post_update",
            "game:draw",
        ]
    );
    assert_eq!(engine.current(), GameSlot::Game);
}

// ---------------------------------------------------------------------
// Boot dispatch (spec §5 (b)/(c): "-p -l" and replay boot with a fake
// window; assert newGame-reached / replay-loaded).
// ---------------------------------------------------------------------

fn windowed_client() -> WindowedClient {
    let assets = Arc::new(
        oh_assets::Assets::new(common::release_dir(), false, false).expect("_RELEASE loads"),
    );
    // A returning player's config (C++ `first_time_playing = false`):
    // the boot's first-time-tips dialogs (MenuGame.cpp:847-870) are what
    // `setIgnoreAllInputs(1)` on boot; without the 13b key-event lane the
    // ported menu can't close them, so the scripted-input tests boot as
    // a returning player (the C++'s `init(false, pack, level)` for a
    // non-first-time config leaves `ignoreInputs` at 0).
    let config = Arc::new(RwLock::new({
        let mut c = Config::default();
        c.set_first_time_playing(false);
        c
    }));
    WindowedClient::build(
        Window::headless(1920, 1080),
        config,
        assets,
        &common::release_dir(),
    )
    .expect("windowed build")
}
/// Spec §5 (b): the C++ `-p -l` boot (main.cpp:507-511) —
/// `mg->init(false, "cube", "apeirogon")` leaves the menu in
/// `LevelSelection` with the level selected (MenuGame.cpp:1688-1775);
/// the window points at the menu. The PLAY item then fires the C++
/// `fnHGNewGame` (main.cpp:442-449): `hg.newGame(...)` +
/// `window->setGameState(hg.getGame())`.
#[test]
fn boot_with_p_and_l_lands_in_level_selection_and_play_reaches_the_game_slot() {
    let client = windowed_client();
    client.boot(Some("cube"), Some("apeirogon"), None);

    assert_eq!(client.engine.current(), GameSlot::Menu);
    assert_eq!(client.menu.state(), States::LevelSelection);
    assert_eq!(
        client.menu.selected_level_id(),
        "ohvrvanilla_vittorio_romeo_cube_1_apeirogon"
    );

    client.menu.ok_action(); // C++ `playSelectedLevel` → `fnHGNewGame`
    assert_eq!(client.engine.current(), GameSlot::Game);
    // `LevelData.id` is the short level id (the full asset id is
    // `pack_id + "_" + LevelData.id` — the map key); the menu-side
    // assertion above already pins the full id.
    assert_eq!(client.game.get_level_data().id, "apeirogon");
}

/// A live-run replay fixture (the same shape as `headless_replay.rs`'
/// `live_run_to_death`): `alwaysSpinRight` on cube_1/apeirogon to death,
/// compressed.
fn live_compressed_replay_path(assets: &Arc<oh_assets::Assets>) -> String {
    let c = oh_app::compose::compose_with(
        assets.clone(),
        common::release_dir(),
        false, /* window */
        Arc::new(oh_game::seams::NullProfiles),
    );
    c.game.set_always_spin_right(true);
    c.game.new_game(
        "ohvrvanilla_vittorio_romeo_cube_1",
        "ohvrvanilla_vittorio_romeo_cube_1_apeirogon",
        true,
        1.0,
        false,
    );
    let captured = Arc::new(std::sync::Mutex::new(None));
    let cap = captured.clone();
    c.game
        .set_on_death_replay_created(Arc::new(move |rf: &oh_replay::ReplayFile| {
            *cap.lock().expect("replay capture poisoned") = Some(rf.clone());
        }));
    c.game.set_must_start(true);
    c.game
        .execute_game_until_death(1, 1.0)
        .expect("live run exceeded the C++ 1-second processing budget");
    let rf = captured
        .lock()
        .expect("replay capture poisoned")
        .take()
        .expect("on_death_replay_created never fired");
    let crf = oh_replay::compress_replay_file(&rf).expect("compress");
    let path =
        std::env::temp_dir().join(format!("oh-windowed-replay-{}.ohr.z", std::process::id()));
    std::fs::write(&path, &crf.data).expect("write replay fixture");
    path.to_string_lossy().into_owned()
}

/// Spec §5 (c): the compressed-replay boot (main.cpp:518-534) —
/// decompress + `hg.setLastReplay` + `hg.newGame(..., executeLastReplay
/// = true)` + `window->setGameState(hg.getGame())` (main.cpp:496-502).
#[test]
fn boot_with_a_compressed_replay_loads_it_into_the_game_slot() {
    let client = windowed_client();
    let replay = live_compressed_replay_path(&Arc::new(
        oh_assets::Assets::new(common::release_dir(), false, false).expect("_RELEASE loads"),
    ));

    client.boot(None, None, Some(&replay));

    assert_eq!(client.engine.current(), GameSlot::Game);
    assert!(
        client.game.in_replay(),
        "the replay is active (setLastReplay)"
    );
    // Same `LevelData.id` note as the `-p -l` test: the short level id.
    assert_eq!(client.game.get_level_data().id, "apeirogon");
}

/// Spec §5 (d): a readable-but-undecompressable file — the C++
/// `gotoGameCompressedReplay` logs and returns with NO state set (the
/// first frame's `assert(isValid())` would fire); the documented
/// deviation points the window at the menu instead.
#[test]
fn boot_with_an_undecompressable_replay_points_the_window_at_the_menu() {
    let client = windowed_client();
    let path =
        std::env::temp_dir().join(format!("oh-windowed-garbage-{}.ohr.z", std::process::id()));
    std::fs::write(&path, b"this is not a compressed replay").expect("write garbage");

    client.boot(None, None, Some(path.to_str().expect("utf-8 path")));

    assert_eq!(client.engine.current(), GameSlot::Menu);
    // The C++ `gotoMenu` tail (main.cpp:476-480): `mg->init(false)` +
    // the menu state.
    assert!(client.menu.state() != States::LevelSelection);
}

// ---------------------------------------------------------------------
// Issue 010b: the input-manager wiring — scripted `InputState` drives
// the windowed loop through the real per-frame order
// (`refresh_input` → N×(`update_input` + `update_from_timer`),
// GameEngine.hpp:40-50/80-86; the fake window's `InputState` is the
// C++'s `GameWindow::mInput`). The real keyboard stays MANUAL.
// ---------------------------------------------------------------------

/// One input frame: exactly one timer tick (`TIME_STEP` = `TIME_SLICE`
/// = 0.25 FT — one `updateFromTimer` per frame, the C++'s 240 Hz at 60
/// frames per second of frame time).
fn tick(client: &mut WindowedClient) {
    client.timer.set_frame_time(TIME_STEP);
    client.tick_self();
}

fn set_key(client: &mut WindowedClient, key: Key, down: bool) {
    client
        .window
        .get_input_state_mut()
        .set_key_pressed(key, down);
}

/// The menu's hardcoded Enter bind (MenuGame.cpp:959-961, `Type::Once`)
/// drives `okAction` → the C++ `fnHGNewGame` (main.cpp:442-449) → the
/// Game slot; holding Enter one more frame must not re-fire (the menu
/// state is no longer active, and the game's `Tid::Restart` bind
/// (kSpace/kEnter/bMiddle) is `hasDied`-gated — a no-op while alive).
#[test]
fn scripted_enter_key_starts_the_level_from_the_menu() {
    let mut client = windowed_client();
    client.boot(Some("cube"), Some("apeirogon"), None);
    assert_eq!(client.engine.current(), GameSlot::Menu);
    assert_eq!(client.menu.state(), States::LevelSelection);

    set_key(&mut client, Key::ENTER, true);
    tick(&mut client);

    assert_eq!(
        client.engine.current(),
        GameSlot::Game,
        "Enter → okAction → newGame → setGameState(Game)"
    );

    tick(&mut client);
    assert_eq!(client.engine.current(), GameSlot::Game);

    set_key(&mut client, Key::ENTER, false);
}

/// `Tid::RotateCCW` (HexagonGame.cpp:357, `add2StateInput` → `Type::Always`
/// on/off): kA down sets `inputImplCCW`, release clears it; and the first
/// movement key press starts the level (the C++ `mustStart` gate,
/// `updateInput`: `if(!started && (!rotateToStart || inputImplCCW ||
/// inputImplCW)) mustStart = true`).
#[test]
fn scripted_rotation_key_sets_the_impl_flag_and_starts_the_level() {
    let mut client = windowed_client();
    client.boot(Some("cube"), Some("apeirogon"), None);
    client.menu.ok_action();
    assert_eq!(client.engine.current(), GameSlot::Game);
    assert!(!client.game.get_status().started);

    set_key(&mut client, Key::A, true);
    tick(&mut client);
    assert!(client.game.get_input_impl_ccw(), "kA down → inputImplCCW");
    // `mustStart` is set by this tick's `updateInput` (the `!started &&
    // (!rotateToStart || inputImplCCW)` gate) and consumed by `start()` on
    // the NEXT tick's update (update.rs:148-150 vs :639-643).
    tick(&mut client);
    assert!(
        client.game.get_status().started,
        "the movement key press starts the level"
    );

    set_key(&mut client, Key::A, false);
    tick(&mut client);
    assert!(!client.game.get_input_impl_ccw(), "kA released → cleared");
}

/// `Tid::Screenshot` (HexagonGame.cpp:408-410, `Type::Once`, the
/// `notInConsole` gate): kF12 sets `mustTakeScreenshot` — consumed by the
/// windowed `draw`'s screenshot within the same frame (the C++
/// `HGGraphics.cpp:305-312` clears it after the save attempt), so the
/// assertion drives the frame's UPDATE phase only (the loop's order minus
/// the draw, `tick_fields`'s pre-draw half) and checks before the draw.
#[test]
fn scripted_screenshot_key_sets_must_take_screenshot() {
    let mut client = windowed_client();
    client.boot(Some("cube"), Some("apeirogon"), None);
    client.menu.ok_action();
    assert!(!client.game.must_take_screenshot());

    set_key(&mut client, Key::F12, true);
    {
        let input = client.window.get_input_state();
        client.engine.refresh_input(input);
        client.engine.update_input(input, TIME_STEP);
        client.engine.update_from_timer(TIME_STEP);
    }
    assert!(client.game.must_take_screenshot());

    set_key(&mut client, Key::F12, false);
    tick(&mut client);
}

/// `Tid::ForceRestart` (HexagonGame.cpp:385-387, `Type::Once`, NO
/// `hasDied` gate): with no local profile active (the windowed
/// composition's `NullProfiles` — the C++'s
/// `!profiles->anyLocalProfileActive()` branch), `MustRestart` →
/// `goToMenu(false, false)` → the engine back to the Menu slot
/// (main.cpp:456-462's `fnGoToMenu` tail).
#[test]
fn scripted_force_restart_key_returns_to_the_menu() {
    let mut client = windowed_client();
    client.boot(Some("cube"), Some("apeirogon"), None);
    client.menu.ok_action();
    // Start the level first (the state change is consumed only once
    // `status.started`).
    set_key(&mut client, Key::A, true);
    tick(&mut client);
    set_key(&mut client, Key::A, false);
    tick(&mut client);
    assert!(client.game.get_status().started);

    set_key(&mut client, Key::R, true);
    tick(&mut client); // Once bind → mustStateChange = MustRestart → consumed in the same tick's update → goToMenu (no local profile)
    tick(&mut client); // the `fnGoToMenu` flag is consumed at the start of the next game tick
    set_key(&mut client, Key::R, false);

    assert_eq!(client.engine.current(), GameSlot::Menu);
    assert_eq!(client.menu.state(), States::LevelSelection);
}

/// `Tid::Pause` (HexagonGame.cpp:421-441): the debug-gated `debugPause`
/// toggle — `Config::getDebug` is the C++ `getOfficial() ?
/// debug().getDefault() : debug()` (Config.cpp:1117-1120), so the config
/// needs `official = false` + `debug = true`; then kF2 toggles
/// `debugPause` on and off (the `shouldPlayMusic` music half is the audio
/// seam's; `NullAudio` makes it a no-op).
#[test]
fn scripted_pause_key_toggles_debug_pause() {
    let assets = Arc::new(
        oh_assets::Assets::new(common::release_dir(), false, false).expect("_RELEASE loads"),
    );
    let config = Arc::new(RwLock::new({
        let mut c = Config::default();
        c.set_first_time_playing(false);
        c.set_official(false);
        c.set_debug(true);
        c
    }));
    let mut client = WindowedClient::build(
        Window::headless(1920, 1080),
        config,
        assets,
        &common::release_dir(),
    )
    .expect("windowed build");
    client.boot(Some("cube"), Some("apeirogon"), None);
    client.menu.ok_action();
    assert!(!client.game.debug_pause());

    set_key(&mut client, Key::F2, true);
    tick(&mut client);
    assert!(
        client.game.debug_pause(),
        "kF2 (debug config) → debugPause on"
    );

    set_key(&mut client, Key::F2, false);
    tick(&mut client);
    set_key(&mut client, Key::F2, true);
    tick(&mut client);
    assert!(!client.game.debug_pause(), "second kF2 → debugPause off");
    set_key(&mut client, Key::F2, false);
}

/// The menu's `Tid::Down` bind (MenuGame.cpp:940-957, `Type::Once`):
/// kS moves the LevelSelection's selection to the next level.
#[test]
fn scripted_down_key_navigates_the_level_selection() {
    let mut client = windowed_client();
    client.boot(Some("cube"), Some("apeirogon"), None);
    let initial = client.menu.selected_level_id().clone();

    set_key(&mut client, Key::S, true);
    tick(&mut client);
    set_key(&mut client, Key::S, false);

    assert_eq!(client.engine.current(), GameSlot::Menu);
    assert_ne!(
        client.menu.selected_level_id(),
        initial,
        "kS (Tid::Down) moves the selection"
    );
}

/// Replay no-leak (the task's critical invariant): during a replay, the
/// live window's input still feeds the input manager (the C++'s
/// `GameState::updateInput` runs every frame — GameEngine.hpp:40-50), so
/// `inputImplCCW` follows the live kA — but the movement comes from the
/// replay's recorded input (the `mustReplayInput` branch skips
/// `updateInput`, update.rs:160-185), so the level plays the fixture's
/// `alwaysSpinRight` and NOT the live key.
#[test]
fn replay_run_ignores_live_input_for_movement() {
    let mut client = windowed_client();
    let assets = Arc::new(
        oh_assets::Assets::new(common::release_dir(), false, false).expect("_RELEASE loads"),
    );
    let replay = live_compressed_replay_path(&assets);
    client.boot(None, None, Some(&replay));
    assert_eq!(client.engine.current(), GameSlot::Game);
    assert!(client.game.in_replay());

    set_key(&mut client, Key::A, true);
    for _ in 0..8 {
        tick(&mut client);
    }
    assert!(client.game.in_replay());
    assert!(
        client.game.get_input_impl_ccw(),
        "the live manager still sets the flag (the C++ runs the binds in replay too)"
    );
    assert_eq!(
        client.game.get_input_movement(),
        1,
        "the movement stays the replay's (alwaysSpinRight → right), NOT the live kA"
    );
    set_key(&mut client, Key::A, false);
}
