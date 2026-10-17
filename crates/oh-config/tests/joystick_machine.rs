//! 29j1 — the `oh_config::joystick` state machine driven with synthetic
//! `JoystickPoll` snapshots: the rising-edge-vs-held semantics the C++
//! `Joystick::update` / `risingEdge` / `ignoreAllPresses` give the menu
//! and game handlers, the dpad-vs-stick OR, the SFML deadzone, and the
//! bind-index seam (`Config::load_all_joystick_binds` →
//! `set_joystick_bind` → `poll.button_pressed`).
//!
//! The machine is a process-global (`JOYSTICK_STATE`), so these
//! scenarios run in ONE `#[test]` function, sequentially — the same
//! discipline the C++'s single-threaded game loop upholds (two parallel
//! pollers would race the was-pressed latches the C++ never guards).
//!
//! What this CANNOT establish: a physical controller through the real
//! backend (`oh_app::joystick::RealJoystick`'s gilrs device read,
//! button-mapping-database entries, Linux hidraw/udev access) — no pad
//! in CI. The poll→seam contract tested here is exactly the surface the
//! backend implements (the SFML-scale `JoystickPoll`).

use oh_config::joystick::{self, Jdir, Jid, JoystickPoll};
use oh_config::Config;

#[test]
fn machine_dynamics_synthetic_polls() {
    // The C++ `HexagonGame` ctor's `Config::loadAllJoystickBinds()`
    // (HexagonGame.cpp:448) — the default config arms the machine's
    // 12 bind slots: j_restart=0, j_select=1, j_exit=2,
    // j_force_restart=3, j_focus=4, j_swap=5, j_replay=6,
    // j_screenshot=7, j_next=11, j_previous=10, ... (config.rs:499-510).
    let cfg = Config::default();
    cfg.load_all_joystick_binds();
    let deadzone = cfg.get_joystick_deadzone(); // 5.0 (the SFML scale)

    let empty = JoystickPoll::default();
    joystick::update(deadzone, &empty); // settle: everything unpressed

    // ------------------------------------------------------------------
    // Rising edge vs held (C++ `risingEdge(Jid::Restart)`, Jid button 0).
    // ------------------------------------------------------------------
    let mut restart = JoystickPoll::default();
    restart.buttons[0] = true;
    joystick::update(deadzone, &restart);
    assert!(joystick::pressed(Jid::Restart), "press frame: pressed");
    assert!(
        joystick::rising_edge(Jid::Restart),
        "press frame: rising edge"
    );
    joystick::update(deadzone, &restart);
    assert!(joystick::pressed(Jid::Restart), "held: still pressed");
    assert!(
        !joystick::rising_edge(Jid::Restart),
        "held: NO rising edge (the C++ was-pressed latch)"
    );
    joystick::update(deadzone, &empty);
    assert!(!joystick::pressed(Jid::Restart), "release frame: unpressed");
    assert!(
        !joystick::rising_edge(Jid::Restart),
        "release frame: no edge"
    );
    joystick::update(deadzone, &restart);
    assert!(
        joystick::rising_edge(Jid::Restart),
        "re-press after release: a NEW rising edge"
    );
    joystick::update(deadzone, &empty);

    // ------------------------------------------------------------------
    // Directions: the dpad POV and the stick OR into one state (C++
    // `doDirection`); left = -povX / -x, right = +povX / +x; the deadzone
    // kills sub-threshold values; the SFML Y axis is DOWN-positive, so
    // +100 is Down and POV +127 is Up (Joystick.cpp:30-36 mapping).
    // ------------------------------------------------------------------
    let dpad_left = JoystickPoll {
        pov_x: -127.0,
        ..JoystickPoll::default()
    };
    joystick::update(deadzone, &dpad_left);
    assert!(joystick::pressed_dir(Jdir::Left), "dpad left");
    assert!(joystick::rising_edge_dir(Jdir::Left), "dpad left: edge");
    let stick_left = JoystickPoll {
        x: -100.0,
        ..JoystickPoll::default()
    };
    joystick::update(deadzone, &stick_left);
    assert!(
        joystick::pressed_dir(Jdir::Left),
        "stick left keeps Left (OR)"
    );
    assert!(
        !joystick::rising_edge_dir(Jdir::Left),
        "the source swap is not a new edge"
    );
    let sub_deadzone = JoystickPoll {
        x: 3.0,
        ..JoystickPoll::default()
    };
    joystick::update(deadzone, &sub_deadzone);
    assert!(
        !joystick::pressed_dir(Jdir::Left),
        "within the deadzone: released"
    );
    assert!(!joystick::pressed_dir(Jdir::Right), "still no Right");
    let stick_right = JoystickPoll {
        x: 100.0,
        ..JoystickPoll::default()
    };
    joystick::update(deadzone, &stick_right);
    assert!(
        joystick::pressed_dir(Jdir::Right),
        "stick right past deadzone"
    );
    joystick::update(deadzone, &empty);
    let stick_down = JoystickPoll {
        y: 100.0,
        ..JoystickPoll::default()
    };
    joystick::update(deadzone, &stick_down);
    assert!(
        joystick::pressed_dir(Jdir::Down),
        "SFML Y down-positive: +100 is Down"
    );
    assert!(!joystick::pressed_dir(Jdir::Up));
    let dpad_up = JoystickPoll {
        pov_y: 127.0,
        ..JoystickPoll::default()
    };
    joystick::update(deadzone, &dpad_up);
    assert!(
        joystick::pressed_dir(Jdir::Up),
        "POV up (+127) — the C++'s up = +povY"
    );
    assert!(
        !joystick::pressed_dir(Jdir::Down),
        "the dpad release dropped Down"
    );
    joystick::update(deadzone, &empty);

    // ------------------------------------------------------------------
    // `ignoreAllPresses` (C++ `Joystick::ignoreAllPresses`): the
    // was-pressed latch + the `update` early-return. A press made DURING
    // the ignore window is not registered (the state freezes at the
    // pre-ignore values); after re-enable its first update fires the
    // rising edge (the was-flags were reset to false, the C++'s "if all
    // xxxWasPressed values are true all new presses return false"), and
    // a button already held at ignore-time does NOT double-fire (its
    // frozen `pressed` was true, so the next `do_button` re-latches the
    // was-flag true).
    // ------------------------------------------------------------------
    let swap = JoystickPoll {
        buttons: [false, false, false, false, false, true, false, false],
        ..JoystickPoll::default()
    }; // j_swap default = button 5
    joystick::update(deadzone, &empty); // settle: swap unpressed
    joystick::ignore_all_presses(true);
    joystick::update(deadzone, &swap); // the press lands in the ignore window
    assert!(
        !joystick::pressed(Jid::Swap),
        "a press made while ignoring is not registered (state frozen)"
    );
    assert!(!joystick::rising_edge(Jid::Swap));
    joystick::ignore_all_presses(false);
    joystick::update(deadzone, &swap); // still held: the re-enable's first update
    assert!(
        joystick::pressed(Jid::Swap),
        "re-enabled: the held press is now registered"
    );
    assert!(
        joystick::rising_edge(Jid::Swap),
        "the window-missed press re-fires its edge after re-enable"
    );
    joystick::update(deadzone, &swap);
    assert!(
        !joystick::rising_edge(Jid::Swap),
        "held past the re-enable: no further edge"
    );
    joystick::update(deadzone, &empty);
    assert!(!joystick::pressed(Jid::Swap));

    // ------------------------------------------------------------------
    // Out-of-range bind indices read as unpressed (SFML's
    // `isButtonPressed` 8-button limit): the default j_next=11 /
    // j_previous=10 can never fire even with every SFML button held;
    // button 7 (j_screenshot default) does.
    // ------------------------------------------------------------------
    let all_buttons = JoystickPoll {
        buttons: [true; 8],
        ..JoystickPoll::default()
    };
    joystick::update(deadzone, &empty);
    joystick::update(deadzone, &all_buttons);
    assert!(
        !joystick::pressed(Jid::NextPack),
        "bind 11 is out of SFML range"
    );
    assert!(
        !joystick::pressed(Jid::PreviousPack),
        "bind 10 is out of SFML range"
    );
    assert!(
        joystick::pressed(Jid::Screenshot),
        "bind 7 fires (button 7 held)"
    );
    assert!(
        joystick::pressed(Jid::Select),
        "bind 1 fires (button 1 held)"
    );
}
