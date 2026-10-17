//! 29j1 — the engine's joystick pipeline end-to-end with a synthetic
//! `JoystickPoll`: poll → the shared `oh_config::joystick` machine →
//! `HexagonGame::updateInput_UpdateJoystickControls` (the four HGUpdate
//! handlers, `update.rs`) — the same surface the desktop's
//! `oh_app::joystick::RealJoystick` poller implements, so this proves
//! the wiring (seam → machine → handlers) without a physical pad (what
//! it cannot establish: the gilrs device read itself — the report's
//! hardware note).
//!
//! The `HexagonGame` ctor now runs `Config::loadAllJoystickBinds()`
//! (the C++ ctor's `HexagonGame.cpp:448`), so the machine's bind slots
//! are the default `j_*` values by the time the first frame runs.
//!
//! The machine is process-global → ONE `#[test]`, sequential scenarios,
//! the C++ single-threaded game-loop discipline.

mod common;

use std::sync::{Arc, Mutex};

use oh_config::joystick;
use oh_config::{Jdir, Jid, JoystickPoll, TIME_STEP};
use oh_game::seams::{Joystick, NullReplayStore, PcgCosmeticRng};

use common::{capture_go_to_menu, compose_with, release_dir, TestProfiles};

const PACK: &str = "ohvrvanilla_vittorio_romeo_cube_1";
const LEVEL: &str = "ohvrvanilla_vittorio_romeo_cube_1_apeirogon";

/// The `Joystick` seam backed by a synthetic `JoystickPoll` fed to the
/// REAL `oh_config::joystick` machine — exactly what
/// `oh_app::joystick::RealJoystick` does (snapshot the controller,
/// `machine::update(deadzone, &poll)`, answer the seam's queries from
/// the machine).
struct MachineJoystick {
    poll: Arc<Mutex<JoystickPoll>>,
}

impl MachineJoystick {
    fn set_poll(&self, poll: JoystickPoll) {
        *self.poll.lock().expect("poll poisoned") = poll;
    }
}

impl Joystick for MachineJoystick {
    fn update(&self, deadzone: f32) {
        joystick::update(deadzone, &self.poll.lock().expect("poll poisoned"));
    }
    fn pressed_dir(&self, dir: Jdir) -> bool {
        joystick::pressed_dir(dir)
    }
    fn pressed_id(&self, id: Jid) -> bool {
        joystick::pressed(id)
    }
    fn rising_edge_id(&self, id: Jid) -> bool {
        joystick::rising_edge(id)
    }
    fn vendor_id(&self) -> u32 {
        0
    }
}

fn stick_x(x: f32) -> JoystickPoll {
    JoystickPoll {
        x,
        ..JoystickPoll::default()
    }
}

fn buttons(set: &[u32]) -> JoystickPoll {
    let mut p = JoystickPoll::default();
    for &i in set {
        p.buttons[i as usize] = true;
    }
    p
}

#[test]
fn joystick_controls_pipeline_synthetic_polls() {
    let assets = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    let joy = Arc::new(MachineJoystick {
        poll: Arc::new(Mutex::new(JoystickPoll::default())),
    });
    // Windowed: `updateInput_UpdateJoystickControls` is window-gated
    // (the C++ `window == nullptr → return` head).
    let c = compose_with(
        assets,
        release_dir(),
        true,
        Arc::new(TestProfiles),
        Arc::new(NullReplayStore),
        joy.clone(),
        Arc::new(PcgCosmeticRng::new()),
    );
    let seen = capture_go_to_menu(&c.game);

    // Default `j_*` binds (config.rs:499-510), loaded by the ctor's
    // `load_all_joystick_binds`: restart=0, exit=2, force_restart=3,
    // focus=4, swap=5, replay=6, screenshot=7.
    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    c.game.set_must_start(true);
    c.game.update(TIME_STEP, 1.0);
    assert!(
        c.game.get_status().started,
        "the game must have started before the control scenarios"
    );

    // ------------------------------------------------------------------
    // A: the held direction (C++ `setInputImplIfFalse` + the machine's
    //    held-vs-edge split): a stick-right flick arms
    //    `inputImplCW` and it LATCHES (the C++'s `setInputImplIfFalse`
    //    only ever sets — the keyboard release path is the clearer);
    //    adding the left flick both-holds → movement = -lastMovement.
    // ------------------------------------------------------------------
    joy.set_poll(stick_x(100.0));
    c.game.update(TIME_STEP, 1.0);
    assert_eq!(
        c.game.get_input_movement(),
        1,
        "stick right past the deadzone → CW movement"
    );
    joy.set_poll(JoystickPoll::default());
    c.game.update(TIME_STEP, 1.0);
    assert_eq!(
        c.game.get_input_movement(),
        1,
        "the C++ quirk, verbatim: inputImplCW latches until a trigger \
         event clears it (setInputImplIfFalse never unsets)"
    );
    joy.set_poll(stick_x(-100.0));
    c.game.update(TIME_STEP, 1.0);
    assert!(
        c.game.get_input_impl_ccw(),
        "stick left arms inputImplCCW (the machine's Left)"
    );
    assert_eq!(
        c.game.get_input_movement(),
        -1,
        "both held: movement = -lastMovement (the C++ both-pressed arm)"
    );
    joy.set_poll(JoystickPoll::default());

    // ------------------------------------------------------------------
    // B: the held id-buttons (j_swap=5, j_focus=4) arm inputSwap /
    //    inputFocused (C++ `setInputImplIfFalse(inputSwap, ...)`).
    // ------------------------------------------------------------------
    joy.set_poll(buttons(&[5, 4]));
    c.game.update(TIME_STEP, 1.0);
    assert!(
        c.game.get_input_swap(),
        "j_swap (button 5) held → inputSwap"
    );
    assert!(
        c.game.get_input_focused(),
        "j_focus (button 4) held → inputFocused"
    );
    joy.set_poll(JoystickPoll::default());
    c.game.update(TIME_STEP, 1.0);

    // ------------------------------------------------------------------
    // C: death → `j_restart` RISING EDGE (not held): `hasDied &&
    //    risingEdge(Restart)` → `MustRestart`; the same frame's
    //    state-change block consumes it (profile active → restart).
    // ------------------------------------------------------------------
    c.game.perform_player_kill();
    assert!(c.game.get_status().has_died);
    joy.set_poll(buttons(&[0])); // j_restart default
    c.game.update(TIME_STEP, 1.0);
    assert_eq!(
        c.game.get_status().must_state_change,
        oh_game::StateChange::None,
        "MustRestart was consumed in-frame by the state-change block"
    );
    assert!(
        !c.game.get_status().has_died,
        "the game restarted (the level reset)"
    );
    assert!(
        !c.game.get_status().started,
        "newGame resets status (C++ `status = HexagonGameStatus{{}}`, \
         HexagonGame.cpp:664) — start waits on mustStart"
    );
    c.game.set_must_start(true);
    c.game.update(TIME_STEP, 1.0);
    assert!(
        c.game.get_status().started,
        "restarted game starts on demand"
    );

    // ------------------------------------------------------------------
    // D: death → `j_replay` RISING EDGE: `MustReplay` → the state-change
    //    block restarts WITH the last replay (`execute_last_replay`).
    // ------------------------------------------------------------------
    c.game.perform_player_kill();
    assert!(c.game.get_status().has_died);
    joy.set_poll(buttons(&[6])); // j_replay default
    c.game.update(TIME_STEP, 1.0);
    assert_eq!(
        c.game.get_status().must_state_change,
        oh_game::StateChange::None,
        "MustReplay was consumed in-frame"
    );
    assert!(
        c.game.in_replay(),
        "the restart executes the last replay (the C++ replayPlayer.isActive() path)"
    );
    c.game.set_must_start(true);
    c.game.update(TIME_STEP, 1.0);
    assert!(c.game.get_status().started);

    // ------------------------------------------------------------------
    // E: `j_force_restart` RISING EDGE needs NO death (C++'s first
    //    disjunct) → MustRestart → restart.
    // ------------------------------------------------------------------
    joy.set_poll(buttons(&[3])); // j_force_restart default
    c.game.update(TIME_STEP, 1.0);
    assert_eq!(
        c.game.get_status().must_state_change,
        oh_game::StateChange::None,
        "MustRestart (force, no death required) was consumed in-frame"
    );
    assert!(!c.game.get_status().has_died);
    assert!(!c.game.get_status().started, "the restart reset the status");
    c.game.set_must_start(true);
    c.game.update(TIME_STEP, 1.0);
    assert!(c.game.get_status().started);

    // ------------------------------------------------------------------
    // F: `j_exit` RISING EDGE → `goToMenu()` (the C++ default-argument
    //    `goToMenu(true, false)`, HexagonGame.hpp:417) — the app-level
    //    `fn_go_to_menu` hook fires with `mError = false`.
    // ------------------------------------------------------------------
    joy.set_poll(buttons(&[2])); // j_exit default
    c.game.update(TIME_STEP, 1.0);
    assert_eq!(
        *seen.lock().expect("go_to_menu callback poisoned"),
        Some(false),
        "the joystick Exit edge must run goToMenu() (mError = false)"
    );
}
