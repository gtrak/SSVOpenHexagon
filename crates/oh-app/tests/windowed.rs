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
//!
//! Issue 015c-tail: the SIGINT handler's safe port (C++ main.cpp:334-350)
//! — the flag the signal sets is injected by the tests (the spec's
//! "signal closure"); the loop head performs the C++ handler's actions
//! (the three log lines + the deferred `globalWindow.stop()`); and the
//! window icon (C++ main.cpp:312-328) — `Assets/icon.png` decoded via the
//! screenshot's `png` dependency + set on the window (headless: recorded).

mod common;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::Ordering;
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
        Arc::new(oh_steam::SteamManager::new(Box::new(oh_steam::NoSteam))),
        None, // no Discord (headless test; build() creates the real IPC manager when None)
        // issue 025f (D-031) — the soft-input controller: the no-op (no
        // android-activity in the host tests — the desktop's controller).
        Arc::new(oh_app::windowed::NoOpSoftInput),
    )
    .expect("windowed build")
}

/// 29b4 test setup: a data-root SANDBOX for the menu-state tests — the
/// C++'s normal user flow (the boot screens → the profile flow → SMain →
/// the "LEVEL SELECT" item, the pre-29b4 tests' `-p -l` boot's equivalent
/// menu state) writes to the data root's `Profiles/`, so it must never
/// run against the tracked `_RELEASE` (the `headless_replay.rs` sandbox
/// invariant: link-or-copy the read-only subtrees via the shared
/// `common::sandbox_link`, copy `config.json`).
struct TestSandbox {
    dir: PathBuf,
}

impl TestSandbox {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "oh-app-windowed-sandbox-{}-{}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        ));
        std::fs::create_dir_all(&dir).expect("sandbox dir");
        let release = common::release_dir();
        for name in ["Packs", "Assets", "ConfigOverrides"] {
            common::sandbox_link(&release.join(name), &dir.join(name));
        }
        std::fs::copy(release.join("config.json"), dir.join("config.json"))
            .expect("sandbox config copy");
        // A writable `Profiles/` dir: the boot flow's profile create/write
        // lands here (the C++ `loadAssets`'s `Folder Profiles/ does not
        // exist` error + `profile_create`'s `reload_local_profiles` both
        // require the directory to exist — the real `_RELEASE` has none,
        // which is exactly the case this sandbox exists to avoid).
        std::fs::create_dir_all(dir.join("Profiles")).expect("sandbox Profiles dir");
        Self { dir }
    }
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A `WindowedClient` over the SANDBOX data root, with the C++ test's
/// `testProfile` created in it (the boot flow's `playLocally()` lands in
/// `SLPSelectBoot` — the profile-select state — instead of
/// `ETLPNewBoot`). The config matches `windowed_client`'s returning-player
/// shape (override `first_time_playing`-adjacent flags per test).
fn sandboxed_windowed_client(config: Option<Arc<RwLock<Config>>>) -> (WindowedClient, TestSandbox) {
    let sandbox = TestSandbox::new();
    let assets =
        Arc::new(oh_assets::Assets::new(&sandbox.dir, false, false).expect("_RELEASE loads"));
    assets
        .profile_create("testProfile")
        .expect("profile create");
    let config = config.unwrap_or_else(|| {
        Arc::new(RwLock::new({
            let mut c = Config::default();
            c.set_first_time_playing(false);
            c
        }))
    });
    let client = WindowedClient::build(
        Window::headless(1920, 1080),
        config,
        assets,
        &sandbox.dir,
        Arc::new(oh_steam::SteamManager::new(Box::new(oh_steam::NoSteam))),
        None, // no Discord (headless test; build() creates the real IPC manager when None)
        Arc::new(oh_app::windowed::NoOpSoftInput),
    )
    .expect("windowed build");
    (client, sandbox)
}

/// 29b4: drives the C++'s NORMAL user flow from a plain boot to
/// `LevelSelection` (the pre-29b4 tests' `-p -l` boot used to provide this
/// state as a shortcut; 29b4 made the boot START the level — the
/// C++'s `loadCommandLineLevel` tail `playSelectedLevel`, MenuGame.cpp:
/// 1778 — so the menu-state tests now take the manual path: the boot
/// screens' `checkCloseBootScreens` (MenuGame.cpp:360-380 — the port's
/// `advance_boot_screen`) → `playLocally()`'s profile select (the
/// sandbox's `testProfile` → `SLPSelectBoot`, MenuGame.cpp:1787-1799) →
/// the profile item's `exec` (→ SMain) → the "LEVEL SELECT" item (→
/// LevelSelection)). The ctor's `initBootLevel`'s `setIndex(0)` half lands
/// on the FIRST selectable pack's first level (the tutorial pack's
/// `babysteps` — the menu's selectable order is NOT the pack-priority
/// order); the old `-p cube -l apeirogon` boot pinned the canonical test
/// level, so this helper finishes by steering the selection back to
/// `cube`/`apeirogon` (`changePackTo` + `setIndex`, the same drawer
/// mutation the menu's own pack/level binds perform) so every menu-state
/// test runs on the level the game-slot tests do.
fn boot_to_level_selection(client: &mut WindowedClient) {
    client.boot(None, None, None);
    client.menu.advance_boot_screen();
    assert_eq!(client.menu.state(), States::EpilepsyWarning);
    client.menu.advance_boot_screen();
    assert_eq!(client.menu.state(), States::SLPSelectBoot);
    client.menu.ok_action(); // the profile item's `exec` → SMain
    assert_eq!(client.menu.state(), States::SMain);
    client.menu.ok_action(); // "LEVEL SELECT" → LevelSelection
    assert_eq!(client.menu.state(), States::LevelSelection);

    // Steer the selection to the canonical `cube`/`apeirogon` level.
    {
        let assets = client.menu.assets();
        let packs = assets.selectable_pack_ids();
        let cube_id = assets.find_pack_id_by_name("cube").expect("cube pack");
        let cube_idx = packs
            .iter()
            .position(|p| p == &cube_id)
            .expect("cube pack in the selectable order");
        let apeirogon_idx = assets
            .level_ids_for_pack(&cube_id)
            .iter()
            .position(|l| l.ends_with("_apeirogon"))
            .expect("apeirogon level in the cube pack");
        let mut s = client.menu.shared().borrow_mut();
        s.change_pack_to(cube_idx as i32);
        s.set_index(apeirogon_idx as i32);
    }
    assert_eq!(
        client.menu.selected_level_id(),
        "ohvrvanilla_vittorio_romeo_cube_1_apeirogon"
    );
}
/// 29b4: the C++ `-p -l` boot (main.cpp:507-511) — `mg->init(false, "cube",
/// "apeirogon")`'s `loadCommandLineLevel` tail is `playSelectedLevel();`
/// (MenuGame.cpp:1778) → `fnHGNewGame` (main.cpp:442-449):
/// `hg.newGame(...)` + `window->setGameState(hg.getGame())`. The boot
/// STARTS the level — "to immediately run level" (main.cpp:96,:104) —
/// and the window points at the GAME (not the menu).
#[test]
fn boot_with_p_and_l_starts_the_level_immediately() {
    let client = windowed_client();
    client.boot(Some("cube"), Some("apeirogon"), None);

    assert_eq!(
        client.engine.current(),
        GameSlot::Game,
        "the `loadCommandLineLevel` tail's `playSelectedLevel` fired `fnHGNewGame`"
    );
    // The menu state is the tail's pre-play state (the C++ `playSelectedLevel`
    // changes no menu state — the window's game-state switch is the seam's
    // job, main.cpp:446-448): `LevelSelection` with the level selected
    // (MenuGame.cpp:1775 + the `setIndex` half, :1773-1774).
    assert_eq!(client.menu.state(), States::LevelSelection);
    assert_eq!(
        client.menu.selected_level_id(),
        "ohvrvanilla_vittorio_romeo_cube_1_apeirogon"
    );
    // `LevelData.id` is the short level id (the full asset id is
    // `pack_id + "_" + LevelData.id` — the map key); the menu-side
    // assertion above already pins the full id.
    assert_eq!(client.game.get_level_data().id, "apeirogon");
}

/// 29b4: the C++ `loadCommandLineLevel` FAILURE paths (the early returns
/// before the `playSelectedLevel` tail — invalid pack, MenuGame.cpp:
/// 1701-1707; invalid level, :1741-1747) leave the C++ `gameState` null
/// (the first frame's `assert(isValid())` would fire); the documented
/// deviation points the window at the menu instead. No crash either way.
#[test]
fn boot_with_an_invalid_pack_or_level_lands_in_the_menu() {
    // Invalid pack: the C++'s first early return (MenuGame.cpp:1701-1707).
    let client = windowed_client();
    client.boot(Some("no_such_pack"), Some("apeirogon"), None);
    assert_eq!(
        client.engine.current(),
        GameSlot::Menu,
        "the failure path: the port points the window at the menu"
    );

    // Invalid level (the pack exists): the C++'s nested-loop early return
    // (MenuGame.cpp:1741-1747).
    let client = windowed_client();
    client.boot(Some("cube"), Some("no_such_level"), None);
    assert_eq!(client.engine.current(), GameSlot::Menu);
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
        Arc::new(oh_game::seams::NoneSteamSink),
        None, // no Discord (replay fixture)
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
    let (mut client, _sandbox) = sandboxed_windowed_client(None);
    boot_to_level_selection(&mut client);
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

/// 29u1 — the frame loop's menu-state cursor handlers (the C++
/// `MenuGame::onEvent`'s `KeyPressed`/`MouseMoved`, MenuGame.cpp:274-307):
/// a key press hides the cursor (:278), a mouse move re-shows it (:295),
/// and a key release does NOT re-show (the C++ has no release handler).
/// The cursor's observable state is the window's own `mouse_cursor_visible`
/// flag (the per-frame sync, the C++ `MenuGame::setMouseCursorVisible` →
/// `window.setMouseCursorVisible`, MenuGame.cpp:3641-3644).
#[test]
fn cursor_hides_on_keypress_and_shows_on_mouse_move_in_the_menu() {
    let (mut client, _sandbox) = sandboxed_windowed_client(None);
    assert_eq!(client.engine.current(), GameSlot::Menu);
    // The C++ ctor's `setMouseCursorVisible(true)` (MenuGame.cpp:245) — the
    // window + the shared flag start visible.
    assert!(client.window.is_mouse_cursor_visible());

    // C++ :278 — a key press hides the cursor (the menu state's detection).
    set_key(&mut client, Key::A, true);
    tick(&mut client);
    assert!(
        !client.window.is_mouse_cursor_visible(),
        "a key press hides the cursor"
    );

    // A key release does NOT re-show (the C++ has no release cursor handler).
    set_key(&mut client, Key::A, false);
    tick(&mut client);
    assert!(
        !client.window.is_mouse_cursor_visible(),
        "a key release does not re-show the cursor"
    );

    // C++ :295 — a mouse move re-shows the cursor (the raw pixel changed).
    oh_render::window::Window::apply_event(
        &mut client.window,
        oh_render::window::Event::MouseMoved {
            position: oh_render::Vec2::new(100.0, 100.0),
        },
    );
    tick(&mut client);
    assert!(
        client.window.is_mouse_cursor_visible(),
        "a mouse move re-shows the cursor"
    );
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
    // 29b4: the boot's `loadCommandLineLevel` tail (`playSelectedLevel`,
    // MenuGame.cpp:1778) already fired `fnHGNewGame` — the level is
    // running, no manual `ok_action` needed.
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
    client.boot(Some("cube"), Some("apeirogon"), None); // 29b4: the boot starts the level (its `playSelectedLevel` tail)
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
    client.boot(Some("cube"), Some("apeirogon"), None); // 29b4: the boot starts the level (its `playSelectedLevel` tail)
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
        Arc::new(oh_steam::SteamManager::new(Box::new(oh_steam::NoSteam))),
        None, // no Discord (headless test; build() creates the real IPC manager when None)
        // issue 025f (D-031) — the soft-input controller: the no-op (the
        // desktop's controller — no android-activity in the host tests).
        Arc::new(oh_app::windowed::NoOpSoftInput),
    )
    .expect("windowed build");
    client.boot(Some("cube"), Some("apeirogon"), None); // 29b4: the boot starts the level (its `playSelectedLevel` tail)
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
    let (mut client, _sandbox) = sandboxed_windowed_client(None);
    boot_to_level_selection(&mut client);
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

/// The hardcoded F3 (MenuGame.cpp:1012-1014): the menu's `reloadAssets`
/// handler (MenuGame.cpp:3092-3127) runs end-to-end — the `Config::
/// getDebug()` guard (the config's `official = false` + `debug = true`,
/// Config.cpp:1117-1120), the real-assets `reloadAllShaders` +
/// `reloadLevel` (the CUBE pack's `apeirogon` level), the UPPERCASE
/// result dialog, and `setIgnoreAllInputs(2)`.
#[test]
fn scripted_f3_reloads_the_level_assets_in_debug() {
    let config = Arc::new(RwLock::new({
        let mut c = Config::default();
        c.set_first_time_playing(false);
        c.set_official(false);
        c.set_debug(true);
        c
    }));
    let (mut client, _sandbox) = sandboxed_windowed_client(Some(config));
    boot_to_level_selection(&mut client);
    assert_eq!(client.menu.state(), States::LevelSelection);

    set_key(&mut client, Key::F3, true);
    tick(&mut client);
    set_key(&mut client, Key::F3, false);
    tick(&mut client);

    let s = client.menu.shared().borrow();
    let lines = s.dialog.dialog_text();
    // The C++'s `reloadLevel` result, uppercased (the level file + the
    // style + the music data sections — the `nullSoundId` early return
    // ends the output).
    assert_eq!(
        lines.first().map(String::as_str),
        Some("LEVEL DATA APEIROGON.JSON SUCCESSFULLY LOADED"),
        "the uppercase reload result dialog"
    );
    assert!(
        lines
            .iter()
            .any(|l| l == "PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE"),
        "the C++'s close line: {lines:?}"
    );
    assert_eq!(s.ignore_inputs, 2, "the C++'s `setIgnoreAllInputs(2)`");
}

/// The hardcoded F4 (MenuGame.cpp:1016-1018): `reloadAssets(true)` — the
/// PACK's assets re-read (the first result line: "Levels successfully
/// reloaded", uppercased by the handler).
#[test]
fn scripted_f4_reloads_the_pack_assets_in_debug() {
    let config = Arc::new(RwLock::new({
        let mut c = Config::default();
        c.set_first_time_playing(false);
        c.set_official(false);
        c.set_debug(true);
        c
    }));
    let (mut client, _sandbox) = sandboxed_windowed_client(Some(config));
    boot_to_level_selection(&mut client);

    set_key(&mut client, Key::F4, true);
    tick(&mut client);
    set_key(&mut client, Key::F4, false);
    tick(&mut client);

    let s = client.menu.shared().borrow();
    let lines = s.dialog.dialog_text();
    assert_eq!(
        lines.first().map(String::as_str),
        Some("LEVELS SUCCESSFULLY RELOADED"),
        "the uppercase pack-reload result: {lines:?}"
    );
    assert_eq!(s.ignore_inputs, 2);
}

/// The handler's `Config::getDebug()` guard, end-to-end: the default
/// (non-debug) client's F3 press runs the handler, which returns before
/// the reload — no dialog, no input-ignore change.
#[test]
fn scripted_f3_without_debug_mode_is_a_no_op() {
    let (mut client, _sandbox) = sandboxed_windowed_client(None); // `Config::default()` — `debug = false`
    boot_to_level_selection(&mut client);
    assert_eq!(client.menu.state(), States::LevelSelection);
    let ignore_before = client.menu.shared().borrow().ignore_inputs;

    set_key(&mut client, Key::F3, true);
    tick(&mut client);
    set_key(&mut client, Key::F3, false);
    tick(&mut client);

    let s = client.menu.shared().borrow();
    assert!(s.dialog.empty(), "no result dialog (the debug guard)");
    assert_eq!(s.ignore_inputs, ignore_before);
    assert_eq!(client.menu.state(), States::LevelSelection);
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

// ---------------------------------------------------------------------
// SIGINT (C++ main.cpp:334-350 — the safe port, issue 015c-tail)
// ---------------------------------------------------------------------

/// C++ main.cpp:341-348 — the SIGINT handler's actions as the safe port
/// performs them at the loop head: the three C++ log lines (main.cpp:343-348)
/// in the C++'s order around the deferred `globalWindow.stop()`
/// (main.cpp:347), + the window close that ends the port's winit loop. The
/// test injects the log sink (the spec's "signal closure" seam) and asserts
/// the exact C++ lines + the stop.
#[test]
fn sigint_stop_runs_the_cpp_lines_and_stops_the_engine() {
    let mut client = windowed_client();
    client.boot(None, None, None);
    assert!(client.engine.is_running());
    assert!(client.window.is_open());

    let mut lines = Vec::new();
    WindowedClient::sigint_stop(
        2, /* the C++ handler's `int s` (main.cpp:343) — SIGINT */
        &mut client.window,
        &client.engine,
        &mut |line| lines.push(line.to_string()),
    );

    assert_eq!(
        lines,
        vec![
            "[::main] Caught signal '2' with game window open".to_string(),
            "[::main] Stopping game window...".to_string(),
            "[::main] Done stopping game window".to_string(),
        ],
        "the three C++ log lines in the C++ order (main.cpp:343-348)"
    );
    assert!(
        !client.engine.is_running(),
        "the deferred globalWindow.stop() (main.cpp:347 → GameEngine.hpp:75-78)"
    );
    assert!(
        !client.window.is_open(),
        "the winit loop's exit condition (module docs: the loop-exit translation)"
    );
}

/// C++ main.cpp:340-349 — the loop head (GameWindow.hpp:121): the flag —
/// set from a closure (the spec's injected "signal closure"; the SAME
/// atomic store `install_sigint`'s `signal-hook` flag handler performs on a
/// real SIGINT) — ends the run: the frame performs the deferred stop (the
/// log lines print to the test process's stdout) and closes the window, so
/// `Window::run` (headless: exactly one frame) returns with the engine
/// stopped — the C++'s `while(isRunning())` ending after `globalWindow.stop()`.
#[test]
fn sigint_flag_at_the_loop_head_stops_the_run() {
    let client = windowed_client();
    client.boot(None, None, None);
    let engine = client.engine.clone();
    // Set the flag from a closure — the signal-hook handler's equivalent:
    let set_flag = || client.interrupt.store(true, Ordering::SeqCst);
    set_flag();
    client.run(); // headless: one frame; the loop head observes the flag
    assert!(
        !engine.is_running(),
        "the run ended with the engine stopped (the C++'s isRunning() == false)"
    );
}

// ---------------------------------------------------------------------
// Window icon (C++ main.cpp:312-328 — issue 015c-tail)
// ---------------------------------------------------------------------

/// C++ main.cpp:312-328 — `Assets/icon.png` via `sf::Image` + `setIcon`:
/// the port decodes the PNG (the screenshot's `png` dependency — the
/// C++'s stb-based `loadFromFile`) and sets it on the window. Headless
/// (no display): the set is recorded (`icon_is_set`); on a live winit
/// window the RGBA8 pixels are uploaded to the platform window.
#[test]
fn create_window_sets_the_icon_from_release_assets() {
    let window = oh_app::windowed::create_window(&Config::default(), &common::release_dir());
    assert!(
        window.icon_is_set(),
        "_RELEASE/Assets/icon.png decodes + is set (the C++'s resetIcon)"
    );
}

/// The C++'s load-failure path (main.cpp:316-319): no `Assets/icon.png`
/// under the base dir → the icon stays the platform default (the C++ logs
/// "Failed to load icon image" — the port's line goes to the test's
/// stdout).
#[test]
fn create_window_without_an_icon_file_keeps_the_platform_default() {
    let missing = std::env::temp_dir().join(format!(
        "oh-app-no-icon-{}-{}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
    ));
    std::fs::create_dir_all(&missing).expect("sandbox dir");
    let window = oh_app::windowed::create_window(&Config::default(), &missing);
    assert!(
        !window.icon_is_set(),
        "no Assets/icon.png → the C++'s 'Failed to load icon image' path"
    );
    let _ = std::fs::remove_dir_all(&missing);
}

/// The icon decode itself: the REAL `_RELEASE/Assets/icon.png` (256×256
/// RGBA, `file(1)`-verified) decodes through `Image::load_png` to a
/// 256×256 RGBA8 image with non-trivial content; a missing file is a clean
/// `Err` (the C++'s `loadFromFile` false, main.cpp:316).
#[test]
fn icon_png_decode_is_rgba8_and_reports_errors() {
    let icon = oh_render::Image::load_png(&common::release_dir().join("Assets/icon.png"))
        .expect("the shipped icon decodes");
    assert_eq!((icon.get_width(), icon.get_height()), (256, 256));
    assert_eq!(icon.get_pixels().len(), 256 * 256 * 4);
    // Non-trivial content: SOME pixel is opaque (the icon is not blank).
    assert!(icon.get_pixels().chunks_exact(4).any(|px| px[3] != 0));

    let missing = std::env::temp_dir().join("oh-app-no-such-icon.png");
    assert!(oh_render::Image::load_png(&missing).is_err());
}

// ---------------------------------------------------------------------
// 029e3 (the graphics audit's M3): the window's FPS slot — the C++'s
// `window->getFPS()` (GameWindow.hpp:335-338 → the timer's `float fps`,
// TimerBase.hpp:23) is published to the engine's `Renderer::fps()` seam
// (the FPS text in `updateText`) every frame: 0 before the first
// `runFPS` (the C++'s `TimerBase::fps{0}`), the timer's value after.
// The pre-fix seam was a constant `u32` 0 (the live `timer.get_fps`
// was never read).
// ---------------------------------------------------------------------

#[test]
fn the_window_fps_slot_publishes_the_timer_fps() {
    let mut client = windowed_client();

    // Before any frame: the C++'s `TimerBase::fps{0}` initial value
    // (TimerBase.hpp:23) — the FPS text's first frame renders "0".
    assert_eq!(
        client.window_fps(),
        0.0,
        "the C++'s TimerBase::fps = 0 initial value before the first runFPS"
    );

    // One frame — `tick_fields`'s end runs the C++'s
    // `runFrameTime` + `runFPS` (GameWindow.hpp:148), then publishes the
    // timer's value into the shared slot the engine's seam reads.
    client.tick_self();
    assert!(
        client.window_fps() > 0.0,
        "after a frame, the slot holds the live window FPS (the pre-fix constant 0 is gone)"
    );
    assert_eq!(
        client.window_fps(),
        client.timer.get_fps(),
        "the slot IS the C++'s `window->getFPS()` = the timer's fps"
    );
}

/// 029e4 (the graphics audit's M4): the `onRenderStage` frametime's
/// divisor — the composition's `AppDraw.fps` (the C++'s
/// `window->getFPS()`, HGGraphics.cpp:76) — must track the timer's live
/// FPS: 0 before the first frame (the C++'s `TimerBase::fps` initial
/// 0 — the first hook call's frametime is the C++'s `60.f / 0.f` = +inf,
/// pinned by `bridge.rs`'s first-frame test), the timer's value after
/// each frame's `runFPS`. The pre-fix `AppDraw.fps` stayed at its
/// `AppDraw::new` initial 60.0 (never updated) — a constant `1/60`
/// frametime.
#[test]
fn the_app_draw_fps_tracks_the_timer_fps() {
    let mut client = windowed_client();

    assert_eq!(
        client.app_draw().borrow().fps,
        0.0,
        "the C++'s initial 0 before the first runFPS (the pre-fix 60.0 is gone)"
    );

    // One frame — `tick_fields`'s end runs the C++'s `runFPS`
    // (GameWindow.hpp:148) then refreshes the `AppDraw`'s fps from the
    // timer (the next frame's hook call divides 60 by this value).
    client.tick_self();
    let fps = client.app_draw().borrow().fps;
    assert!(
        fps > 0.0,
        "after a frame, AppDraw.fps = the timer's live fps"
    );
    assert_eq!(
        fps,
        client.timer.get_fps(),
        "AppDraw.fps IS the C++'s window->getFPS() (the frametime's divisor)"
    );
}

// ---------------------------------------------------------------------
// Issue 31 — the gameplay touch overlay (spec §2b/§5): the platform-
// neutral pointer feed over the existing shared touch sink drives the
// bound inputs (focus / swap / rotation) + the §2.6 exit affordance,
// gameplay-only, with the §2b double-count exclusion. The desktop's
// enable flag (spec §5) is what the tests flip to exercise the overlay
// in-process (spec §4.1) — the real device run is MANUAL (spec §4.3).
// ---------------------------------------------------------------------

/// A touch finger down at `view` pixels (the winit `TouchPhase::Started`
/// fold — the same event the Android HAL delivers; the window's fold
/// updates the shared touch sink the overlay's feed diffs).
fn touch_down(client: &mut WindowedClient, finger: u32, x: f32, y: f32) {
    Window::apply_event(
        &mut client.window,
        oh_render::window::Event::TouchBegan {
            finger,
            position: oh_render::Vec2::new(x, y),
        },
    );
}

fn touch_up(client: &mut WindowedClient, finger: u32) {
    Window::apply_event(
        &mut client.window,
        oh_render::window::Event::TouchEnded { finger },
    );
}

/// The test window's (1920×1080 — `windowed_client`) view-space button
/// centers (the `touch_overlay::button_centers` layout: focus =
/// `w*0.5 - 40`, the bottom row's `h - PAD - BUTTON_HALF` = 1036; the
/// CCW corner's `PAD + BUTTON_HALF` = 44).
const FOCUS_XY: (f32, f32) = (920.0, 1036.0);
const CCW_XY: (f32, f32) = (44.0, 1036.0);
const EXIT_XY: (f32, f32) = (44.0, 44.0);

/// Boots to the level selection, starts the level (the scripted Enter —
/// the C++ `fnHGNewGame`), and starts it (`must_start`), with the
/// overlay's enable flag flipped (spec §5's desktop test exercise).
fn start_level_with_overlay(client: &mut WindowedClient) {
    boot_to_level_selection(client);
    set_key(client, Key::ENTER, true);
    tick(client);
    set_key(client, Key::ENTER, false);
    assert_eq!(client.engine.current(), GameSlot::Game);
    client
        .touch_overlay
        .write()
        .expect("touch overlay poisoned")
        .set_enabled(true);
    client.game.set_must_start(true);
    // One tick: the `must_start` → `start()` (the overlay's gate reads
    // `status.started` at the NEXT frame's feed — the gate's
    // per-frame refresh, the module docs).
    tick(client);
    assert!(client.game.get_status().started);
}

#[test]
fn the_touch_overlay_focus_button_drives_the_bound_focus_input() {
    let (mut client, _sandbox) = sandboxed_windowed_client(None);
    start_level_with_overlay(&mut client);

    // The FOCUS button (the tutorial's step-2 blocker — the C++'s
    // `inputFocused` the `mFocus` tutorial variable reads).
    touch_down(&mut client, 0, FOCUS_XY.0, FOCUS_XY.1);
    tick(&mut client);
    assert!(
        client.game.get_input_focused(),
        "the held focus button drives the bound focus input (the C++-bound semantics — spec §2.3)"
    );
    assert!(
        !client.game.get_input_impl_ccw(),
        "§2b hazard (a): the focus finger is CONSUMED — it must not also count as a left-half rotation finger (x=920 < the 960 half-width)"
    );
    // Spec §2.3/§2b — the replay safety: the recorded input is the
    // DERIVED bit (the focus bit 3), not a new event kind — a
    // touch-generated replay decodes identically on desktop.
    let data = client.game.last_replay_data();
    assert!(
        data.at(data.size() - 1).focus(),
        "the focus bit records into the replay data (the derived-bit replay safety)"
    );

    // Release: the keyboard's `Type::Always` off-closure (the
    // authoritative clear — no keyboard on the test device) takes the
    // bit back on the next tick.
    touch_up(&mut client, 0);
    tick(&mut client);
    assert!(
        !client.game.get_input_focused(),
        "release clears the bound input"
    );
    let data = client.game.last_replay_data();
    assert!(!data.at(data.size() - 1).focus());
}

/// The step-2 shape (spec §2b): finger A holds focus while finger B
/// rotates — BOTH register simultaneously (the multi-touch refcount),
/// and each release clears only its own button.
#[test]
fn the_touch_overlay_focus_and_rotate_register_simultaneously() {
    let (mut client, _sandbox) = sandboxed_windowed_client(None);
    start_level_with_overlay(&mut client);

    touch_down(&mut client, 0, FOCUS_XY.0, FOCUS_XY.1);
    touch_down(&mut client, 1, CCW_XY.0, CCW_XY.1);
    tick(&mut client);
    assert!(client.game.get_input_focused());
    assert_eq!(
        client.game.get_input_movement(),
        -1,
        "the rotate finger (x=44 < the 960 half) drives the bound CCW input — focus AND rotate at once"
    );

    // The rotate tap lifts (the refcount: the focus finger keeps its
    // button; the rotate button clears) — the C++'s
    // `updateInput_ResolveInputImplToInputMovement` then sees the
    // focus-only state (no movement) exactly as a keyboard focus-only
    // hold would.
    touch_up(&mut client, 1);
    tick(&mut client);
    assert!(
        client.game.get_input_focused(),
        "the focus finger still holds"
    );
    assert_eq!(
        client.game.get_input_movement(),
        0,
        "the lifted rotate finger clears its button (no movement)"
    );

    touch_up(&mut client, 0);
    tick(&mut client);
    assert!(!client.game.get_input_focused());
}

/// The C++'s screen-half rotation path survives the overlay (the §4.4
/// regression: a finger OFF every button is a rotation finger exactly
/// as today — the overlay only consumes the fingers IT captured).
#[test]
fn a_touch_off_the_buttons_still_rotates_via_the_screen_half_path() {
    let (mut client, _sandbox) = sandboxed_windowed_client(None);
    start_level_with_overlay(&mut client);

    // (200, 200): no button there (the exit's rect is 12..76 × 12..76;
    // the bottom row is at y=1004..1068) — the C++'s left-half finger.
    touch_down(&mut client, 0, 200.0, 200.0);
    tick(&mut client);
    assert!(
        client.game.get_input_impl_ccw(),
        "a free left-half finger still rotates CCW (the C++'s rotation path, unmodified)"
    );
    assert!(!client.game.get_input_focused());
    // The overlay consumed NOTHING (the finger hit no button — the
    // rotation path saw it whole).
    assert!(client
        .touch_overlay
        .read()
        .expect("touch overlay poisoned")
        .captured_ids()
        .is_empty());
    touch_up(&mut client, 0);
    tick(&mut client);
    assert!(!client.game.get_input_impl_ccw());
}

/// §2.4 — the overlay must not fire during menus (the gate's
/// menu half: not the game slot → inactive → the events are ignored,
/// no capture, the menu's 9-zone grid's sink is untouched — the
/// exclusion only applies to the engine's rotation path, which the
/// menu never reads).
#[test]
fn the_touch_overlay_is_inactive_during_the_menu() {
    let (mut client, _sandbox) = sandboxed_windowed_client(None);
    boot_to_level_selection(&mut client);
    assert_eq!(client.engine.current(), GameSlot::Menu);
    client
        .touch_overlay
        .write()
        .expect("touch overlay poisoned")
        .set_enabled(true);

    touch_down(&mut client, 0, FOCUS_XY.0, FOCUS_XY.1);
    tick(&mut client);
    assert!(
        !client
            .touch_overlay
            .read()
            .expect("touch overlay poisoned")
            .active(),
        "the overlay does not fire during menus (spec §2.4)"
    );
    assert!(
        client
            .touch_overlay
            .read()
            .expect("touch overlay poisoned")
            .captured_ids()
            .is_empty(),
        "no capture while inactive"
    );

    // The menu's 9-zone grid still sees the finger (the SHARED sink's
    // levels are untouched — §2.4 "must not steal touches from the menu
    // grid": the exclusion is on the engine's rotation path only).
    touch_up(&mut client, 0);
    tick(&mut client);
}

/// §2.6 — the exit affordance: a press on the `MENU` label fires the
/// C++'s notInConsole-wrapped Escape binding ONCE (the menu's
/// `go_to_menu(true, false)` contract — 029d1; no new exit path).
#[test]
fn the_touch_overlay_exit_button_goes_to_the_menu_once_per_press() {
    let (mut client, _sandbox) = sandboxed_windowed_client(None);
    start_level_with_overlay(&mut client);

    touch_down(&mut client, 0, EXIT_XY.0, EXIT_XY.1);
    tick(&mut client);
    // The press edge's fire: `trigger_go_to_menu` = `go_to_menu(true,
    // false)` → the C++'s `fnGoToMenu` callback signals the adapter
    // (main.cpp:456-462) — the C++'s switch lands in the same
    // inter-tick gap, so the state change is the NEXT tick's start
    // (the `GameSlotAdapter::update_from_timer`'s consumption).
    tick(&mut client);
    assert_eq!(
        client.engine.current(),
        GameSlot::Menu,
        "the exit press lands on the menu (the 029d1 contract: the menu's go_to_menu(true, false), no new exit path)"
    );
    // Still held (the finger is down): the held state must not re-fire
    // the go-to-menu on the next ticks (the edge, not the level) — and
    // the state transition's deactivation cleared the capture (the
    // module docs' no-stuck-input semantics).
    assert!(
        client
            .touch_overlay
            .read()
            .expect("touch overlay poisoned")
            .captured_ids()
            .is_empty(),
        "deactivation cleared the capture"
    );
    touch_up(&mut client, 0);
    tick(&mut client);
    assert_eq!(client.engine.current(), GameSlot::Menu);
}

// `u_isKeyPressed` / `u_isMouseButtonPressed` (HGScripting.cpp:89-98 /
// :135-141) must read the window's LIVE input state through the
// `Renderer` seam (the C++ `window->getInputState()[...]`). The pre-fix
// port stubbed both to `false` on every platform (the issue's §1
// defect) — this test fails today against the stub: no key the window
// holds was ever visible to the game.
// ---------------------------------------------------------------------

#[test]
fn the_live_window_key_state_reaches_the_game_lua_surface() {
    use oh_game::GameApi;
    use oh_render::window::Event;

    let mut client = windowed_client();

    assert!(
        !client.game.is_key_pressed(Key::L_SHIFT.0),
        "no key is down before the fold"
    );

    // The event fold (the winit `runEvents`'s per-event half): the
    // `KeyPressed` arm updates the window's `InputState` AND republishes
    // it into the shared slot the engine's renderer seam reads.
    Window::apply_event(
        &mut client.window,
        Event::KeyPressed {
            code: Key::L_SHIFT,
            repeated: false,
        },
    );
    assert!(
        client.game.is_key_pressed(Key::L_SHIFT.0),
        "the window's live LShift state reaches the game's Lua surface (the pre-fix stub returned false)"
    );

    Window::apply_event(
        &mut client.window,
        Event::KeyReleased { code: Key::L_SHIFT },
    );
    assert!(
        !client.game.is_key_pressed(Key::L_SHIFT.0),
        "the release clears it (the C++'s live read, not a sticky flag)"
    );
}

#[test]
fn the_live_window_button_state_reaches_the_game_lua_surface() {
    use oh_game::GameApi;
    use oh_render::window::Event;

    let mut client = windowed_client();

    Window::apply_event(
        &mut client.window,
        Event::MouseButtonPressed {
            button: oh_render::Button::X_BUTTON1,
        },
    );
    assert!(
        client
            .game
            .is_mouse_button_pressed(oh_render::Button::X_BUTTON1.0),
        "the window's live XButton1 state reaches the game's Lua surface"
    );
    Window::apply_event(
        &mut client.window,
        Event::MouseButtonReleased {
            button: oh_render::Button::X_BUTTON1,
        },
    );
    assert!(
        !client
            .game
            .is_mouse_button_pressed(oh_render::Button::X_BUTTON1.0),
        "the release clears it"
    );
}

#[test]
fn out_of_range_lua_key_codes_are_false_not_panic() {
    use oh_game::GameApi;

    let client = windowed_client();

    // The C++'s `std::bitset::operator[]` would throw `out_of_range` on
    // these; the port's defined behavior is `false` (a level script
    // cannot make the host panic — the `is_key_pressed_code` note).
    assert!(!client.game.is_key_pressed(-2));
    assert!(!client.game.is_key_pressed(101));
    assert!(!client.game.is_key_pressed(i32::MAX));
    assert!(!client.game.is_mouse_button_pressed(-2));
    assert!(!client.game.is_mouse_button_pressed(5));
    // In-range codes still read the live state (false — nothing down).
    assert!(!client.game.is_key_pressed(Key::UNKNOWN.0));
    assert!(!client
        .game
        .is_mouse_button_pressed(oh_render::Button::LEFT.0));
}
