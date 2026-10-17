//! 29j1 — the menu's joystick controls driven with synthetic
//! `JoystickPoll` snapshots: poll → the shared `oh_config::joystick`
//! machine → `MenuGame::update`'s joystick block (the C++
//! `MenuGame.cpp:2643-2784` port) — the same surface the desktop's
//! `oh_app::joystick::RealJoystick` poller implements, so this proves
//! the wiring (seam → machine → the 8 portable handlers + the
//! `focusHeld` state) without a physical pad (what it cannot establish:
//! the gilrs device read itself — the report's hardware note).
//!
//! The machine's bind slots come from the SAME path the engine's ctor
//! uses (the C++ `HexagonGame.cpp:448` `loadAllJoystickBinds`); the
//! default `j_next`/`j_previous` (11/10) are out of the SFML 8-button
//! range, so the pack scenario re-binds them to free buttons 7/6
//! (`Joystick::setJoystickBind` — the rebind path's machine half).
//!
//! The machine is process-global → ONE `#[test]`, sequential scenarios,
//! the C++ single-threaded game-loop discipline.

use std::cell::RefCell;
use std::rc::Rc;

use oh_config::joystick;
use oh_config::{Jdir, Jid, JoystickPoll};
use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::MenuJoystick;
use oh_menu::{
    FakeAssets, FakeConfig, FakeWindow, MenuAssets, MenuGame, NoopNetwork, NoopTouch,
    RecordingAudio, States,
};

/// The `MenuJoystick` seam backed by a synthetic `JoystickPoll` fed to
/// the REAL `oh_config::joystick` machine — exactly what
/// `oh_app::joystick::RealMenuJoystick` does (snapshot the controller,
/// `machine::update(deadzone, &poll)`, answer the seam's queries from
/// the machine).
struct MachineMenuJoystick {
    poll: Rc<RefCell<JoystickPoll>>,
}

impl MachineMenuJoystick {
    fn set_poll(&self, poll: JoystickPoll) {
        *self.poll.borrow_mut() = poll;
    }
}

impl MenuJoystick for MachineMenuJoystick {
    fn update(&self, deadzone: f32) {
        joystick::update(deadzone, &self.poll.borrow());
    }
    fn pressed_focus(&self) -> bool {
        joystick::pressed(Jid::Focus)
    }
    fn rising_edge(&self, jid: Jid) -> bool {
        joystick::rising_edge(jid)
    }
    fn rising_edge_dir(&self, jdir: Jdir) -> bool {
        joystick::rising_edge_dir(jdir)
    }
    fn ignore_all_presses(&self, ignore: bool) {
        joystick::ignore_all_presses(ignore)
    }
}

fn menu_with_joystick(joy: Rc<dyn MenuJoystick>) -> (MenuGame, Rc<RecordingAudio>) {
    // Two selectable packs + one local profile: the SLPSelectBoot boot
    // (the `boot.rs` fixture pattern) so `ok` lands in SMain, and the
    // pack-change scenario has both a next and a previous pack.
    let assets: Rc<dyn MenuAssets> = {
        let fa = FakeAssets::new();
        fa.add_pack("cube", &["cube1", "cube2", "cube3"]);
        fa.add_pack("orthoplex", &["ortho1"]);
        fa.add_profile("P1");
        fa
    };
    let audio = RecordingAudio::new();
    let menu = MenuGame::new(
        assets,
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        audio.clone(),
        Rc::new(NoopTouch),
        joy,
        FakeWindow::new(),
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    (menu, audio)
}

#[test]
fn menu_joystick_controls_synthetic_polls() {
    // The C++ `HexagonGame` ctor's `loadAllJoystickBinds()`
    // (HexagonGame.cpp:448) — default `j_*` values: select=1, exit=2,
    // focus=4, swap=5, restart=0, replay=6, screenshot=7, next=11,
    // previous=10 (config.rs:499-510).
    let cfg = oh_config::Config::default();
    cfg.load_all_joystick_binds();
    // j_next / j_previous default to 11/10 — out of the SFML 8-button
    // range (never pressable); the rebind path's machine half.
    joystick::set_joystick_bind(7, Jid::NextPack as i32);
    joystick::set_joystick_bind(6, Jid::PreviousPack as i32);

    let joy = Rc::new(MachineMenuJoystick {
        poll: Rc::new(RefCell::new(JoystickPoll::default())),
    });
    let (menu, _audio) = menu_with_joystick(joy.clone());

    menu.advance_boot_screen();
    menu.advance_boot_screen();
    assert_eq!(menu.state(), States::SLPSelectBoot);
    menu.ok_action();
    assert_eq!(menu.state(), States::SMain);
    assert_eq!(menu.selected_item_name().as_deref(), Some("LEVEL SELECT"));
    menu.update(0.016); // one frame, empty poll: the machine settles

    // ------------------------------------------------------------------
    // Stick Y rising edge, SMain: SFML Y is DOWN-positive — -100 is UP
    // (wrap to the last item "EXIT"); +100 is DOWN (wrap back to the
    // first item "LEVEL SELECT").
    // ------------------------------------------------------------------
    joy.set_poll(JoystickPoll {
        y: -100.0,
        ..JoystickPoll::default()
    });
    menu.update(0.016);
    assert_eq!(
        menu.selected_item_name().as_deref(),
        Some("EXIT"),
        "stick up: the SMain selection wraps to the last item"
    );
    joy.set_poll(JoystickPoll::default());
    menu.update(0.016); // release (the machine's was-latch)
    joy.set_poll(JoystickPoll {
        y: 100.0,
        ..JoystickPoll::default()
    });
    menu.update(0.016);
    assert_eq!(
        menu.selected_item_name().as_deref(),
        Some("LEVEL SELECT"),
        "stick down: wraps back to the first item"
    );
    joy.set_poll(JoystickPoll::default());
    menu.update(0.016);

    // ------------------------------------------------------------------
    // Dpad (POV) rising edges in LevelSelection: left = difficulty
    // down, right = difficulty up (the C++'s -povX / +povX mapping).
    // ------------------------------------------------------------------
    menu.ok_action(); // "LEVEL SELECT" → LevelSelection
    assert_eq!(menu.state(), States::LevelSelection);
    let diff_before = menu.diff_mult_idx();

    joy.set_poll(JoystickPoll {
        pov_x: -127.0,
        ..JoystickPoll::default()
    });
    menu.update(0.016);
    assert_eq!(
        menu.diff_mult_idx(),
        diff_before - 1,
        "dpad left: difficulty down"
    );
    joy.set_poll(JoystickPoll::default());
    menu.update(0.016);
    joy.set_poll(JoystickPoll {
        pov_x: 127.0,
        ..JoystickPoll::default()
    });
    menu.update(0.016);
    assert_eq!(
        menu.diff_mult_idx(),
        diff_before,
        "dpad right: difficulty back up"
    );
    joy.set_poll(JoystickPoll::default());
    menu.update(0.016);

    // ------------------------------------------------------------------
    // Held-vs-edge on the stick (the machine's rising-edge split): the
    // difficulty steps ONCE on the edge frame; holding the stick
    // through the next frame steps nothing.
    // ------------------------------------------------------------------
    let before_held = menu.diff_mult_idx();
    joy.set_poll(JoystickPoll {
        x: 100.0,
        ..JoystickPoll::default()
    });
    menu.update(0.016);
    assert_eq!(
        menu.diff_mult_idx(),
        before_held + 1,
        "the edge frame steps the difficulty"
    );
    menu.update(0.016);
    assert_eq!(
        menu.diff_mult_idx(),
        before_held + 1,
        "held: no second step (the handler is risingEdge, not pressed)"
    );
    joy.set_poll(JoystickPoll::default());
    menu.update(0.016);

    // ------------------------------------------------------------------
    // `focusHeld` (the C++ MenuGame.cpp:2645-2679): moving the level
    // selection away from 0, then HOLDING j_focus (button 4) returns it
    // to 0 on the held-rising frame (`set_index(0)`); holding longer
    // does not re-fire.
    // ------------------------------------------------------------------
    joy.set_poll(JoystickPoll {
        y: 100.0,
        ..JoystickPoll::default()
    });
    menu.update(0.016);
    let moved = menu.selected_level_index();
    assert_ne!(moved, 0, "the down step moved the level selection off 0");
    joy.set_poll(JoystickPoll::default());
    menu.update(0.016);

    let mut focus = JoystickPoll::default();
    focus.buttons[4] = true; // j_focus default
    joy.set_poll(focus);
    menu.update(0.016);
    assert_eq!(
        menu.selected_level_index(),
        0,
        "focusHeld's first held frame returns the selection to level 0"
    );
    menu.update(0.016);
    assert_eq!(
        menu.selected_level_index(),
        0,
        "still held: no second set_index (was_focus_held bookkeeping)"
    );
    joy.set_poll(JoystickPoll::default());
    menu.update(0.016);

    // ------------------------------------------------------------------
    // j_next / j_previous rising edges: the pack change (the re-bound
    // buttons 7/6). The edge frame starts the C++ pack FOLD animation
    // (`changePackAction` → `PackChange::Folding`); the pack itself
    // switches at the Folding phase's end (`change_pack()`), and the
    // Stretching phase runs back to Rest — `step_pack_change` per frame
    // (the C++'s animated pack change; BASE_SCROLL_SPEED 30.0 vs the
    // 30×level-count list height).
    // ------------------------------------------------------------------
    assert_eq!(menu.selectable_pack_count(), 2);
    let pack_before = menu.current_pack_id();
    let mut next = JoystickPoll::default();
    next.buttons[7] = true;
    joy.set_poll(next);
    menu.update(0.016); // the j_next rising-edge frame (Folding starts)
    joy.set_poll(JoystickPoll::default());
    for _ in 0..32 {
        menu.update(1.0); // let the fold run
        if menu.current_pack_id() != pack_before {
            break;
        }
    }
    assert_ne!(
        menu.current_pack_id(),
        pack_before,
        "the j_next rising edge changed the pack (at the fold's end)"
    );
    for _ in 0..16 {
        menu.update(1.0); // the Stretching phase back to Rest
    }

    let mut previous = JoystickPoll::default();
    previous.buttons[6] = true;
    joy.set_poll(previous);
    menu.update(0.016);
    joy.set_poll(JoystickPoll::default());
    for _ in 0..32 {
        menu.update(1.0);
        if menu.current_pack_id() == pack_before {
            break;
        }
    }
    assert_eq!(
        menu.current_pack_id(),
        pack_before,
        "the j_previous rising edge returned to the original pack"
    );
    for _ in 0..16 {
        menu.update(1.0);
    }

    // ------------------------------------------------------------------
    // j_exit rising edge in LevelSelection: back to SMain (the C++
    // `exitAction`'s LevelSelection branch, MenuGame.cpp:1816-1860).
    // ------------------------------------------------------------------
    let mut exit = JoystickPoll::default();
    exit.buttons[2] = true; // j_exit default
    joy.set_poll(exit);
    menu.update(0.016);
    assert_eq!(
        menu.state(),
        States::SMain,
        "the Exit rising edge in LevelSelection returns to the main menu"
    );
    joy.set_poll(JoystickPoll::default());
    menu.update(0.016);

    // ------------------------------------------------------------------
    // j_select rising edge: selects the highlighted item — the SMain
    // "EXIT" item's select requests the window stop (the C++
    // `window->stop()` half — the ported `quit_requested` flag).
    // ------------------------------------------------------------------
    joy.set_poll(JoystickPoll {
        y: -100.0,
        ..JoystickPoll::default()
    });
    menu.update(0.016); // stick up: wraps to the last item "EXIT"
    joy.set_poll(JoystickPoll::default());
    menu.update(0.016);
    assert_eq!(menu.selected_item_name().as_deref(), Some("EXIT"));
    let mut select = JoystickPoll::default();
    select.buttons[1] = true; // j_select default
    joy.set_poll(select);
    menu.update(0.016);
    assert!(
        menu.quit_requested(),
        "the Select rising edge on the Exit item must request the window stop"
    );
}
