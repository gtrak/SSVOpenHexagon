//! 29b2 — the `MouseWheelScrolled` handler (C++ `MenuGame.cpp:299-356`):
//! per-menu index navigation over `wheelProgress`, the LevelSelection
//! `YScrollTo += delta * 48.f` accumulation with the two-sided clamp (the
//! lower bound firing `a35_eagerformore` + `error.ogg` and resetting to 0),
//! and the focus-held quick pack change. Synthetic deltas, both clamp
//! ends, the per-event order (a frame's several wheel events each pass
//! the clamp — the C++ `runEvents` semantics the app layer's per-delta
//! drain preserves).

use std::cell::RefCell;
use std::rc::Rc;

use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::UnlockAchievementCallback;
use oh_menu::{
    FakeAssets, FakeConfig, FakeWindow, MenuAssets, MenuConfig, MenuGame, NoMenuJoystick,
    NoopNetwork, NoopTouch, RecordingAudio, States,
};

type UnlockLog = Rc<RefCell<Vec<String>>>;

fn make_menu() -> (MenuGame, UnlockLog, Rc<RecordingAudio>) {
    let assets: Rc<dyn MenuAssets> = {
        let fa = FakeAssets::new();
        fa.add_pack("cube", &["cube1", "cube2", "cube3"]);
        fa.add_pack("orthoplex", &["ortho1"]);
        fa.add_profile("P1");
        fa
    };
    let config: Rc<dyn MenuConfig> = FakeConfig::new();
    let audio = RecordingAudio::new();
    let unlocks: UnlockLog = Rc::new(RefCell::new(Vec::new()));
    let rec = unlocks.clone();
    let cb: Option<UnlockAchievementCallback> =
        Some(Box::new(move |id| rec.borrow_mut().push(id.to_string())));
    let menu = MenuGame::new(
        assets,
        config,
        Rc::new(NoopNetwork),
        audio.clone(),
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        None,
        None,
        None,
        cb,
        None,
        Rc::new(LinearFontHeight),
    );
    (menu, unlocks, audio)
}

/// Boot the menu through the two boot screens to the main menu (one
/// existing profile → SLPSelectBoot → ok → SMain).
fn boot_to_main(menu: &MenuGame) {
    assert_eq!(menu.state(), States::LoadingScreen);
    menu.advance_boot_screen();
    assert_eq!(menu.state(), States::EpilepsyWarning);
    menu.advance_boot_screen();
    assert_eq!(menu.state(), States::SLPSelectBoot);
    menu.ok_action();
    assert_eq!(menu.state(), States::SMain);
}

fn wheel_progress(menu: &MenuGame) -> f32 {
    menu.shared().borrow().wheel_progress
}

#[test]
fn wheel_menu_nav_up_accrued_line_fires_up_action() {
    let (menu, _unlocks, _audio) = make_menu();
    boot_to_main(&menu);
    assert_eq!(menu.selected_item_name().as_deref(), Some("LEVEL SELECT"));

    // C++ :344-354 — `wheelProgress += delta`; the action fires only past
    // +1 (a full wheel "line").
    menu.handle_mouse_wheel(0.5);
    assert_eq!(wheel_progress(&menu), 0.5);
    assert_eq!(menu.selected_item_name().as_deref(), Some("LEVEL SELECT"));

    menu.handle_mouse_wheel(0.5); // exactly 1.0 — NOT `> 1.f`
    assert_eq!(wheel_progress(&menu), 1.0);
    assert_eq!(menu.selected_item_name().as_deref(), Some("LEVEL SELECT"));

    menu.handle_mouse_wheel(0.1); // 1.1 > 1 → upAction + reset
    assert_eq!(wheel_progress(&menu), 0.0);
    // `upAction` from index 0 wraps to the last item.
    assert_eq!(menu.selected_item_name().as_deref(), Some("EXIT"));
}

#[test]
fn wheel_menu_nav_down_accrued_line_fires_down_action() {
    let (menu, _unlocks, _audio) = make_menu();
    boot_to_main(&menu);

    menu.handle_mouse_wheel(-0.5);
    assert_eq!(wheel_progress(&menu), -0.5);
    assert_eq!(menu.selected_item_name().as_deref(), Some("LEVEL SELECT"));

    menu.handle_mouse_wheel(-0.5); // exactly -1.0 — NOT `< -1.f`
    assert_eq!(wheel_progress(&menu), -1.0);
    assert_eq!(menu.selected_item_name().as_deref(), Some("LEVEL SELECT"));

    menu.handle_mouse_wheel(-0.1); // -1.1 < -1 → downAction + reset
    assert_eq!(wheel_progress(&menu), 0.0);
    assert_eq!(menu.selected_item_name().as_deref(), Some("LOCAL PROFILES"));
}

#[test]
fn wheel_level_list_scrolls_the_target_y() {
    let (menu, _unlocks, _audio) = make_menu();
    boot_to_main(&menu);
    menu.ok_action(); // LEVEL SELECT → LevelSelection (cube, index 0)
    assert_eq!(menu.state(), States::LevelSelection);

    // C++ :324-326 — `YScrollTo += delta * 48.f` (focus not held).
    assert_eq!(
        menu.shared().borrow().lvl_slct.y_scroll_to,
        0.0,
        "fresh selection starts at the top"
    );
    menu.handle_mouse_wheel(-1.0); // wheel down → scroll the list down
    assert_eq!(menu.shared().borrow().lvl_slct.y_scroll_to, -48.0);
    menu.handle_mouse_wheel(-1.5); // accumulation: -48 - 72
    assert_eq!(menu.shared().borrow().lvl_slct.y_scroll_to, -120.0);
    menu.handle_mouse_wheel(1.0); // back up: -120 + 48
    assert_eq!(menu.shared().borrow().lvl_slct.y_scroll_to, -72.0);
}

#[test]
fn wheel_level_list_clamps_the_top() {
    let (menu, _unlocks, _audio) = make_menu();
    boot_to_main(&menu);
    menu.ok_action();
    // C++ :328-331 — `YScrollTo > 0 → YScrollTo = 0` (the list cannot
    // scroll past its top).
    {
        let mut s = menu.shared().borrow_mut();
        s.lvl_slct.y_scroll_to = -20.0;
    }
    menu.handle_mouse_wheel(1.0); // -20 + 48 = +28 → 0
    assert_eq!(menu.shared().borrow().lvl_slct.y_scroll_to, 0.0);
}

#[test]
fn wheel_level_list_end_scroll_unlocks_eager_for_more_and_resets() {
    let (menu, unlocks, audio) = make_menu();
    boot_to_main(&menu);
    menu.ok_action();
    // C++ :332-339 — `YScrollTo < -4000`: the "Why..." branch — the
    // `a35_eagerformore` achievement + `error.ogg`, then `YScrollTo = 0`
    // (the jump back to the list's top, NOT a clamp to -4000).
    {
        let mut s = menu.shared().borrow_mut();
        s.lvl_slct.y_scroll_to = -3990.0;
    }
    menu.handle_mouse_wheel(-1.0); // -3990 - 48 = -4038 < -4000
    assert_eq!(
        menu.shared().borrow().lvl_slct.y_scroll_to,
        0.0,
        "the end-scroll resets to the top"
    );
    assert_eq!(*unlocks.borrow(), vec!["a35_eagerformore".to_string()]);
    assert!(
        audio.played.borrow().iter().any(|s| s == "error.ogg"),
        "the C++ plays error.ogg on the end-scroll; got {:?}",
        audio.played.borrow()
    );
}

#[test]
fn wheel_level_list_no_achievement_at_the_exact_bound() {
    let (menu, unlocks, _audio) = make_menu();
    boot_to_main(&menu);
    menu.ok_action();
    // C++ :332 — the test is `< -4000` (strict): landing exactly on
    // -4000 keeps the value, no achievement.
    {
        let mut s = menu.shared().borrow_mut();
        s.lvl_slct.y_scroll_to = -3952.0;
    }
    menu.handle_mouse_wheel(-1.0); // -3952 - 48 = exactly -4000
    assert_eq!(menu.shared().borrow().lvl_slct.y_scroll_to, -4000.0);
    assert!(unlocks.borrow().is_empty());
}

#[test]
fn wheel_level_list_clamps_per_event_like_the_cpp_run_events() {
    let (menu, unlocks, _audio) = make_menu();
    boot_to_main(&menu);
    menu.ok_action();
    // Two wheel events in one frame (the C++ `runEvents` runs the handler
    // per event between polls — the port's `refresh_input` drain calls
    // `handle_mouse_wheel` per recorded delta): the FIRST event that
    // crosses -4000 fires the gag and resets to 0; the SECOND starts from
    // the reset value (a summed per-frame delta would not cross the bound
    // at all).
    {
        let mut s = menu.shared().borrow_mut();
        s.lvl_slct.y_scroll_to = -3990.0;
    }
    menu.handle_mouse_wheel(-1.0); // -4038 → gag + reset to 0
    menu.handle_mouse_wheel(-1.0); // -48 (NOT -8076: the second event
                                   // runs on the reset value, the C++'s per-event order)
    assert_eq!(menu.shared().borrow().lvl_slct.y_scroll_to, -48.0);
    assert_eq!(*unlocks.borrow(), vec!["a35_eagerformore".to_string()]);
}

#[test]
fn wheel_focus_held_does_the_quick_pack_change() {
    let (menu, _unlocks, audio) = make_menu();
    boot_to_main(&menu);
    menu.ok_action();
    assert_eq!(menu.state(), States::LevelSelection);
    assert_eq!(menu.current_pack_id(), "cube");

    // C++ :319-321 — `if(focusHeld) changePackQuick(delta > 0 ? -1 : 1)`:
    // wheel UP (positive) jumps to the PREVIOUS pack (the C++'s
    // `upAction` focus-held arm is `changePackQuick(-1)`).
    menu.set_focus_held(true);
    menu.handle_mouse_wheel(1.0);
    assert_eq!(
        menu.current_pack_id(),
        "orthoplex",
        "wheel up with focus → previous pack (the 2-pack wrap)"
    );
    assert!(
        audio.played.borrow().iter().any(|s| s == "beep.ogg"),
        "changePackQuick plays beep.ogg"
    );

    // Wheel down (negative) → next pack (back to cube).
    menu.handle_mouse_wheel(-1.0);
    assert_eq!(menu.current_pack_id(), "cube");

    // The C++'s ternary checks `delta > 0`: a zero delta takes the -1 arm
    // (`changePackQuick(1)`).
    menu.handle_mouse_wheel(0.0);
    assert_eq!(
        menu.current_pack_id(),
        "orthoplex",
        "delta == 0 takes the C++'s `: 1` arm"
    );
}
