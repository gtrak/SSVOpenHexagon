//! 29b7 — the hold-Exit-to-quit (C++ `MenuGame.cpp:2852-2854`): the
//! `Tid::Escape` `Type::Always` bind's on-half accumulates `exitTimer`
//! (every frame while held, unless in MOpts — MenuGame.cpp:965-971), the
//! off-half resets it to 0, and the `update()` tail's `if(exitTimer > 20)
//! window.stop()` (the port's `quit()` — the `quit_requested` request path
//! the windowed loop consumes) fires only on a CONTINUOUS 20s hold,
//! strictly past the threshold.

use oh_menu::seams::{FakeAssets, FakeConfig, FakeWindow, MenuAssets};
use oh_menu::{MenuGame, NoMenuJoystick, NoopNetwork, NoopTouch, States};

fn make_menu() -> MenuGame {
    let fa = {
        let fa = FakeAssets::new();
        fa.add_pack("cube", &["c1", "c2", "c3"]);
        fa.add_profile("P1");
        fa
    };
    let assets: std::rc::Rc<dyn MenuAssets> = fa;
    let menu = MenuGame::new(
        assets,
        FakeConfig::new(),
        std::rc::Rc::new(NoopNetwork),
        oh_menu::RecordingAudio::new(),
        std::rc::Rc::new(NoopTouch),
        std::rc::Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        None,
        None,
        None,
        None,
        None,
        std::rc::Rc::new(oh_menu::layout::LinearFontHeight),
    );
    // Boot through the two boot screens to the main menu.
    menu.advance_boot_screen();
    menu.advance_boot_screen();
    menu.ok_action();
    assert_eq!(menu.state(), States::SMain);
    menu
}

/// One "held frame": the bind's on-half accumulates `ft`, then the
/// `update()` tail runs the threshold check (the C++'s per-frame order —
/// `runEvents` feeds the bind, `update` checks the timer).
fn held_frame(menu: &MenuGame, ft: f32) {
    menu.add_exit_timer(ft);
    menu.update(ft);
}

#[test]
fn hold_exit_for_20s_quits_strictly_past_the_threshold() {
    let menu = make_menu();
    assert!(!menu.quit_requested());

    // C++ `if(exitTimer > 20)` — STRICTLY greater: exactly 20.0 of
    // accumulated hold does NOT quit.
    for i in 0..20 {
        held_frame(&menu, 1.0);
        assert_eq!(menu.shared().borrow().exit_timer, (i + 1) as f32);
        assert!(!menu.quit_requested(), "frame {}: not yet past 20", i + 1);
    }
    assert!(
        !menu.quit_requested(),
        "exactly 20.0 is NOT > 20 (after the 20th held frame)"
    );
    // The 21st held frame crosses the threshold.
    held_frame(&menu, 1.0);
    assert_eq!(menu.shared().borrow().exit_timer, 21.0);
    assert!(
        menu.quit_requested(),
        "past 20 the C++'s window.stop() fires (the port's quit request)"
    );
}

#[test]
fn releasing_exit_resets_the_timer_so_short_holds_never_quit() {
    // C++ off-half (MenuGame.cpp:971): `exitTimer = 0` — a 19s hold,
    // release, then another 19s hold must NOT quit (the timer does not
    // leak across releases).
    let menu = make_menu();
    for _ in 0..19 {
        held_frame(&menu, 1.0);
    }
    assert_eq!(menu.shared().borrow().exit_timer, 19.0);
    assert!(!menu.quit_requested());
    menu.reset_exit_timer();
    assert_eq!(menu.shared().borrow().exit_timer, 0.0);

    for _ in 0..19 {
        held_frame(&menu, 1.0);
    }
    assert_eq!(menu.shared().borrow().exit_timer, 19.0);
    assert!(
        !menu.quit_requested(),
        "two 19s holds with a release between never reach 20"
    );
}

#[test]
fn the_mopts_state_does_not_accumulate_the_exit_timer() {
    // C++ on-half gate (MenuGame.cpp:965-967): `if(state != States::MOpts)`
    // — holding Exit inside the options menu must not count toward the
    // quit (the C++'s "you can't hold-quit out of the options menu"
    // semantics; the once `exitAction` bind still returns from it).
    let menu = make_menu();
    // MOpts (the OPTIONS item).
    {
        let mut s = menu.shared().borrow_mut();
        s.state = States::MOpts;
    }
    for _ in 0..25 {
        menu.add_exit_timer(1.0);
        menu.update(1.0);
    }
    assert_eq!(menu.shared().borrow().exit_timer, 0.0);
    assert!(!menu.quit_requested());
}
