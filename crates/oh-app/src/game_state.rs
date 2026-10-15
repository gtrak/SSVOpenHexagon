//! C++ `ssvs::GameEngine`'s game-state switching surface — the minimal
//! `window->setGameState(...)` equivalent (issue 15 step (c), spec §1).
//!
//! C++ oracle (D-014, verified):
//!
//! * `GameWindow::setGameState(GameState&)` (GameWindow.hpp:222-225) just
//!   forwards to `GameEngine::setGameState` (GameEngine.hpp:109-111), which
//!   stores a `GameState*` — the pointer the loop's
//!   `updateFromTimer` / `drawFromTimer` then route through:
//!   `GameEngine::updateFromTimer(mFT)` (GameEngine.hpp:40-50) `assert`s
//!   the state is set and calls the CURRENT state's `update(mFT)`;
//!   `GameEngine::drawFromTimer()` (GameEngine.hpp:52-56, reached via
//!   `TimerBase::runDraw`, `TimerBase.inl:10-13`) calls the current
//!   state's `draw()`.
//! * Two states in practice (spec §1): the C++ `mainClient` owns
//!   `HexagonGame hg` (whose `ssvs::GameState game` member,
//!   `HexagonGame.hpp` "ssvs::GameState game") and `MenuGame mg` (whose
//!   `ssvs::GameState game` member) for the whole scope; the window points
//!   at one or the other (`setGameState(hg.getGame())` main.cpp:446 /
//!   `setGameState(mg->getGame())` main.cpp:460,478,497,505,525,532).
//!
//! The port keeps the states `Rc`-shared and the switching surface
//! single-threaded (the C++ raw pointers are single-threaded too): the C++
//! `fnGoToMenu` member (`HexagonGame.hpp` "std::function<void(const bool)>
//! fnGoToMenu", main.cpp:456-462) is `Send + Sync` (the engine is
//! `Arc<dyn GameApi>`), so the windowed composition's callback only sets a
//! thread-atomic signal that the game state adapter consumes at the start
//! of the next tick (the C++ switch lands in the same inter-tick gap —
//! `oh_app::windowed` documents the one error-path exception); the switch
//! itself goes through [`GameEngine::set_game_state`], which writes the
//! same current-slot atomic.

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use oh_render::graphics::DrawTarget;

/// C++ `window->setGameState(...)`'s target (two states in practice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GameSlot {
    /// The C++ `mg->getGame()` (the MenuGame's `ssvs::GameState`).
    Menu = 0,
    /// The C++ `hg.getGame()` (the HexagonGame's `ssvs::GameState`).
    Game = 1,
}

/// C++ `ssvs::GameState` — a switchable game state (GameState.hpp).
///
/// No `Send`/`Sync` bounds: the states are touched only on the window's
/// thread (the loop's tick/draw, and the menu's in-thread callbacks),
/// exactly like the C++ single-threaded `GameState*`.
pub trait GameState {
    /// C++ `GameState::update(mFT)` (GameState.hpp:39-42) — routed via
    /// `GameEngine::updateFromTimer` (GameEngine.hpp:40-50).
    fn update_from_timer(&self, ft: f32);

    /// C++ `GameState::draw()` (GameState.hpp:44-47) — routed via
    /// `GameEngine::drawFromTimer` (GameEngine.hpp:52-56 →
    /// `TimerBase::runDraw`, TimerBase.inl:10-13).
    fn draw(&self, target: &mut dyn DrawTarget);

    /// C++ `ssvs::GameState::onPostUpdate` delegate (GameState.hpp:60) —
    /// fires once per frame AFTER the timer's update burst, from
    /// `GameEngine::runUpdate` (GameEngine.hpp:86: `gameState->onPostUpdate()`
    /// after `timer->runUpdate()`). Default: the empty delegate (the C++
    /// `ssvu::Delegate` default — `MenuGame` registers nothing; `HexagonGame`
    /// registers `postUpdate`, HexagonGame.cpp:331).
    fn on_post_update(&self) {}
}

/// C++ `ssvs::GameEngine`'s state pointer + `stop` / `isRunning`
/// (GameEngine.hpp:20-124) — the minimal switching surface.
pub struct GameEngine {
    /// The two registered states, indexed by `GameSlot` — set ONCE at
    /// construction (the C++ `mainClient` owns both `ssvs::GameState`s for
    /// the whole scope; `setGameState` only points between them).
    states: [Option<Rc<dyn GameState>>; 2],
    /// C++ `GameState* gameState` (GameEngine.hpp:30) — the current state.
    current: AtomicU8,
    /// C++ `bool running{true}` (GameEngine.hpp:32).
    running: AtomicBool,
}

impl GameEngine {
    /// Register both states (menu + game) before the loop starts.
    #[must_use]
    pub fn new(menu: Rc<dyn GameState>, game: Rc<dyn GameState>) -> Self {
        Self {
            states: [Some(menu), Some(game)],
            current: AtomicU8::new(GameSlot::Menu as u8),
            running: AtomicBool::new(true),
        }
    }

    /// C++ `GameEngine::setGameState(GameState&)` (GameEngine.hpp:109-111;
    /// reached through `window->setGameState`, GameWindow.hpp:222-225).
    ///
    /// The C++ `fnGoToMenu` callback (main.cpp:456-462) fires this from
    /// INSIDE the engine's `goToMenu` — in the ported windowed composition
    /// the stored callback only sets a thread-atomic signal (the callback
    /// is `Send + Sync`; the `Rc`-based menu calls are performed by the
    /// game state adapter, `oh_app::windowed`), which calls this method
    /// with the menu slot — the same store this method writes (same
    /// effect: every subsequent `updateFromTimer` / `drawFromTimer`
    /// routes to the new state).
    pub fn set_game_state(&self, slot: GameSlot) {
        self.current.store(slot as u8, Ordering::SeqCst);
    }

    /// The current state slot (loop/test inspection).
    #[must_use]
    pub fn current(&self) -> GameSlot {
        match self.current.load(Ordering::SeqCst) {
            0 => GameSlot::Menu,
            _ => GameSlot::Game,
        }
    }

    /// C++ `GameEngine::updateFromTimer(mFT)` (GameEngine.hpp:40-50) —
    /// `assert(isValid())` (the port `expect`s: the windowed boot dispatch
    /// always points the window at a state before `window->run()`,
    /// main.cpp:488-537), then the CURRENT state's `update(mFT)`.
    ///
    /// (The C++ also runs `gameState->updateInput(*inputState, mFT)` first
    /// when an `InputState` is attached — GameEngine.hpp:44-48; the port's
    /// states consume input inside their own update: the game through its
    /// joystick/renderer seams, the menu through the (not-yet-ported)
    /// SSVStart input manager.)
    pub fn update_from_timer(&self, ft: f32) {
        let state = self
            .state()
            .expect("game state set (C++ GameEngine::assert(isValid()))");
        state.update_from_timer(ft);
    }

    /// C++ `GameEngine::drawFromTimer()` (GameEngine.hpp:52-56, reached via
    /// `TimerBase::runDraw`, TimerBase.inl:10-13) — the CURRENT state's
    /// `draw()`.
    pub fn draw(&self, target: &mut dyn DrawTarget) {
        let state = self
            .state()
            .expect("game state set (C++ GameEngine::assert(isValid()))");
        state.draw(target);
    }

    /// C++ `GameEngine::runUpdate`'s tail (GameEngine.hpp:80-87:
    /// `gameState->onPostUpdate()` after `timer->runUpdate()`) — the
    /// CURRENT state's `onPostUpdate` delegate, once per frame, AFTER the
    /// whole timer update burst (the C++ `HexagonGame::postUpdate`,
    /// HexagonGame.cpp:331; the menu state's delegate is empty).
    pub fn on_post_update(&self) {
        let state = self
            .state()
            .expect("game state set (C++ GameEngine::assert(isValid()))");
        state.on_post_update();
    }

    /// The current state object (the slot's `Rc` clone — the guard-free
    /// form of the C++ `gameState` pointer dereference; callers must not
    /// hold it across anything that re-enters this engine).
    fn state(&self) -> Option<Rc<dyn GameState>> {
        self.states[self.current() as usize].clone()
    }

    /// C++ `GameEngine::stop()` (GameEngine.hpp:75-78) — called by the
    /// window's SIGINT handler (main.cpp:340-352) and by the window's
    /// `stop()` (GameWindow.hpp:151-155).
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// C++ `GameEngine::isRunning()` (GameEngine.hpp:119-122) — the
    /// `while(gameEngine->isRunning())` loop condition
    /// (GameWindow.hpp:121).
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;

    /// A recording `DrawTarget` (the state's `draw` needs one; no GL).
    struct NullTarget;
    impl DrawTarget for NullTarget {
        fn clear_black(&mut self) {}
        fn set_view(&mut self, _view: oh_render::view::View) {}
        fn draw_vertices(
            &mut self,
            _vertices: &[oh_render::draw::Vertex],
            _states: oh_render::draw::RenderStates,
        ) {
        }
        fn draw_sprite(&mut self, _sprite: &oh_render::texture::Sprite) {}
        fn draw_text(
            &mut self,
            _text: &oh_render::font::Text,
            _states: oh_render::draw::RenderStates,
        ) {
        }
        fn draw_rectangle(
            &mut self,
            _rect: &oh_render::types::RectangleShape,
            _states: oh_render::draw::RenderStates,
        ) {
        }
        fn save_screenshot(&mut self, _path: &str) {}
    }

    /// The fake two-state harness (spec §5): each state records
    /// `update` / `draw` calls with its own name.
    struct RecordingState {
        name: &'static str,
        log: Rc<RefCell<Vec<String>>>,
    }

    impl GameState for RecordingState {
        fn update_from_timer(&self, ft: f32) {
            self.log
                .borrow_mut()
                .push(format!("{}:update({ft})", self.name));
        }
        fn draw(&self, _target: &mut dyn DrawTarget) {
            self.log.borrow_mut().push(format!("{}:draw", self.name));
        }
        fn on_post_update(&self) {
            self.log
                .borrow_mut()
                .push(format!("{}:post_update", self.name));
        }
    }

    fn harness() -> (GameEngine, Rc<RefCell<Vec<String>>>) {
        let log = Rc::new(RefCell::new(Vec::new()));
        let menu = RecordingState {
            name: "menu",
            log: log.clone(),
        };
        let game = RecordingState {
            name: "game",
            log: log.clone(),
        };
        (GameEngine::new(Rc::new(menu), Rc::new(game)), log)
    }

    /// Fresh engine: the current state is the menu (C++ `mainClient` boots
    /// in the menu — main.cpp:476-479 `gotoMenu`).
    #[test]
    fn fresh_engine_starts_in_the_menu_slot() {
        let (engine, _log) = harness();
        assert_eq!(engine.current(), GameSlot::Menu);
        assert!(engine.is_running());
    }

    /// `setGameState` + `updateFromTimer` route the update to the state
    /// that is CURRENT at call time (GameEngine.hpp:40-50: the C++ reads
    /// the `gameState` pointer per call).
    #[test]
    fn update_routes_to_the_current_state() {
        let (engine, log) = harness();
        engine.update_from_timer(0.25);
        engine.set_game_state(GameSlot::Game);
        engine.update_from_timer(0.25);
        engine.set_game_state(GameSlot::Menu);
        engine.update_from_timer(0.25);
        assert_eq!(
            *log.borrow(),
            vec![
                "menu:update(0.25)",
                "game:update(0.25)",
                "menu:update(0.25)"
            ]
        );
    }

    /// `setGameState` + `drawFromTimer` route the draw the same way
    /// (GameEngine.hpp:52-56).
    #[test]
    fn draw_routes_to_the_current_state() {
        let (engine, log) = harness();
        let mut target = NullTarget;
        engine.draw(&mut target);
        engine.set_game_state(GameSlot::Game);
        engine.draw(&mut target);
        assert_eq!(*log.borrow(), vec!["menu:draw", "game:draw"]);
    }

    /// The per-frame order the C++ `runUpdate` → `runDraw` pair gives
    /// (GameWindow.hpp:132-142): update (N× via the timer) THEN draw, and
    /// a state switch mid-frame affects only the LATER calls (the C++
    /// reads the pointer at each call site).
    #[test]
    fn update_then_draw_order_across_a_switch() {
        let (engine, log) = harness();
        let mut target = NullTarget;

        // Frame 1 (menu): two timer ticks + the draw.
        engine.update_from_timer(0.25);
        engine.update_from_timer(0.25);
        engine.draw(&mut target);

        // Mid-frame switch to the game (the C++ `fnHGNewGame` does
        // `newGame` + `setGameState` inside a menu tick — main.cpp:442-449).
        engine.set_game_state(GameSlot::Game);
        // Frame 2 (game): the remaining tick + the draw route to the game.
        engine.update_from_timer(0.25);
        engine.draw(&mut target);

        assert_eq!(
            *log.borrow(),
            vec![
                "menu:update(0.25)",
                "menu:update(0.25)",
                "menu:draw",
                "game:update(0.25)",
                "game:draw",
            ]
        );
    }

    /// A frame's update tail routes to the state current at call time —
    /// the C++ `runUpdate` order (GameEngine.hpp:80-87): the timer burst
    /// (N×`updateFromTimer`) THEN one `onPostUpdate`, each read through the
    /// pointer at its own call site.
    #[test]
    fn on_post_update_routes_to_the_current_state_after_the_burst() {
        let (engine, log) = harness();
        engine.update_from_timer(0.25);
        engine.on_post_update();
        engine.set_game_state(GameSlot::Game);
        engine.update_from_timer(0.25);
        engine.on_post_update();
        assert_eq!(
            *log.borrow(),
            vec![
                "menu:update(0.25)",
                "menu:post_update",
                "game:update(0.25)",
                "game:post_update",
            ]
        );
    }

    /// `stop()` flips the C++ `running` flag (GameEngine.hpp:75-78,119-122)
    /// — the `while(isRunning())` condition the window loop checks.
    #[test]
    fn stop_flips_is_running() {
        let (engine, _log) = harness();
        assert!(engine.is_running());
        engine.stop();
        assert!(!engine.is_running());
    }
}
