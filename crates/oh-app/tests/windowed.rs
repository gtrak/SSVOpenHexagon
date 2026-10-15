//! Issue 15 step (c): the WINDOWED wiring — headless-first tests (spec §5):
//! the state-switching loop driven with the REAL `TimerStatic` over fake
//! states, and the `WindowedClient` boot dispatch with a fake (headless)
//! window. The real GL + winit run is MANUAL (spec §5):
//! `Window::create` degrades to a headless window without a display, and
//! these tests inject a `Window::headless` so no display / GL / event loop
//! is touched at all.

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use oh_app::game_state::{GameEngine, GameSlot, GameState};
use oh_app::windowed::{frame, WindowedClient};
use oh_config::Config;
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
    let config = Arc::new(RwLock::new(Config::default()));
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
