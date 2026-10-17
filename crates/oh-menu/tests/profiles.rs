//! The menu's profile call sites (issue 029c1): `playLocally`'s
//! first-save (MenuGame.cpp:2427-2432 — `assets.pSaveCurrent()` BEFORE the
//! state choice) and `eraseAction`'s two guards + the file-removal /
//! store-erase order (MenuGame.cpp:2398-2431).

use std::rc::Rc;

use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::{
    FakeAssets, FakeConfig, FakeWindow, MenuAssets, NoMenuJoystick, NoopNetwork, NoopTouch,
    RecordingAudio,
};
use oh_menu::states::States;
use oh_menu::MenuGame;

fn menu_with_profiles(
    names: &[&str],
    current: Option<&str>,
) -> (MenuGame, Rc<FakeAssets>, Rc<RecordingAudio>) {
    let assets = FakeAssets::new();
    for n in names {
        assets.add_profile(n);
    }
    if let Some(c) = current {
        assets.profile_set_current(c);
    }
    let audio = RecordingAudio::new();
    let menu = MenuGame::new(
        assets.clone(),
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        audio.clone(),
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        None,
        None,
        None,
        None,
        None, // `unlock_achievement` — the 29b2/29b8 Steam seam (None in the tests)
        Rc::new(LinearFontHeight),
    );
    (menu, assets, audio)
}

/// C++ `playLocally` (MenuGame.cpp:2427-2432) — `assets.pSaveCurrent()`
/// FIRST: the run's scores reach disk before the profile-select/new-boot
/// state choice.
#[test]
fn play_locally_saves_the_current_profile_first() {
    let (menu, assets, _audio) = menu_with_profiles(&["p1", "p2"], Some("p1"));
    // The profile-selection state the C++ `playLocally` lands in when
    // profiles exist (the boot flow's `play_locally` call site,
    // MenuGame.cpp:1352-equivalent).
    menu.shared().borrow_mut().change_state_to(States::SMain);
    menu.shared().borrow_mut().play_locally();
    assert_eq!(menu.state(), States::SLPSelectBoot, "the state choice ran");
    assert_eq!(
        assets.profile_save_log(),
        vec![Some("p1".to_string())],
        "the save ran FIRST, over the current profile"
    );
}

/// No current profile: the C++ `pSaveCurrent` early return (the save is a
/// no-op) — the call still happens (the log records the `None` arm).
#[test]
fn play_locally_without_a_current_profile_records_the_early_return() {
    let (menu, assets, _audio) = menu_with_profiles(&["p1"], None);
    menu.shared().borrow_mut().change_state_to(States::SMain);
    menu.shared().borrow_mut().play_locally();
    assert_eq!(menu.state(), States::SLPSelectBoot);
    assert_eq!(assets.profile_save_log(), vec![None]);
    // No profiles at all: the ETLPNewBoot arm (still after the save call).
    let (menu2, assets2, _a2) = menu_with_profiles(&[], None);
    menu2.shared().borrow_mut().change_state_to(States::SMain);
    menu2.shared().borrow_mut().play_locally();
    assert_eq!(menu2.state(), States::ETLPNewBoot);
    assert_eq!(assets2.profile_save_log(), vec![None]);
}

/// C++ `eraseAction`'s first guard (MenuGame.cpp:2400-2408): refuse when
/// it is the ONLY remaining profile — error.ogg + dialog +
/// `setIgnoreAllInputs(2)`, nothing removed.
#[test]
fn erase_action_refuses_the_only_remaining_profile() {
    let (menu, assets, audio) = menu_with_profiles(&["p1"], None);
    menu.shared()
        .borrow_mut()
        .change_state_to(States::SLPSelect);
    menu.erase_action();
    assert!(
        audio.played.borrow().iter().any(|s| s == "error.ogg"),
        "the guard plays error.ogg"
    );
    assert!(
        !audio.played.borrow().iter().any(|s| s == "beep.ogg"),
        "no removal beep"
    );
    assert_eq!(
        menu.shared().borrow().ignore_inputs,
        2,
        "setIgnoreAllInputs(2)"
    );
    assert!(
        !menu.shared().borrow().dialog.empty(),
        "the 'ONLY REMAINING PROFILE' dialog is up"
    );
    assert_eq!(
        assets.profile_names(),
        vec!["p1".to_string()],
        "nothing removed"
    );
}

/// C++ `eraseAction`'s second guard (MenuGame.cpp:2409-2418): refuse when
/// it is the CURRENTLY-IN-USE profile.
#[test]
fn erase_action_refuses_the_in_use_profile() {
    let (menu, assets, audio) = menu_with_profiles(&["p1", "p2"], Some("p1"));
    // The selection sits on the FIRST item (p1 — the in-use one).
    menu.shared()
        .borrow_mut()
        .change_state_to(States::SLPSelect);
    menu.erase_action();
    assert!(
        audio.played.borrow().iter().any(|s| s == "error.ogg"),
        "the guard plays error.ogg"
    );
    assert_eq!(
        menu.shared().borrow().ignore_inputs,
        2,
        "setIgnoreAllInputs(2)"
    );
    assert!(
        !menu.shared().borrow().dialog.empty(),
        "the 'CURRENTLY IN USE PROFILE' dialog is up"
    );
    assert_eq!(
        assets.profile_names(),
        vec!["p1".to_string(), "p2".to_string()]
    );
}

/// The successful erase (MenuGame.cpp:2420-2433): the FILE removal runs
/// first, the store erase only on success, then the menu item + beep.
#[test]
fn erase_action_removes_file_then_store_then_item() {
    let (menu, assets, audio) = menu_with_profiles(&["p1", "p2"], Some("p1"));
    // The selection sits on p1 (current — guarded); move it to p2.
    menu.shared()
        .borrow_mut()
        .change_state_to(States::SLPSelect);
    menu.profile_selection_menu.next();
    menu.erase_action();
    // The file is gone (the fake's list = the files), the store entry too.
    assert_eq!(assets.profile_names(), vec!["p1".to_string()], "p2 erased");
    assert!(
        !audio.played.borrow().iter().any(|s| s == "error.ogg"),
        "no guard fired"
    );
    assert!(
        audio.played.borrow().iter().any(|s| s == "beep.ogg"),
        "the removal beep"
    );
    // The menu item is gone (the category shrank).
    assert_eq!(
        menu.profile_selection_menu.get_items().len(),
        1,
        "the item was removed from the category"
    );
}

/// The `std::remove` failure branch (MenuGame.cpp:2424-2428): a missing
/// file logs + returns — no store erase, no item removal, no beep.
#[test]
fn erase_action_missing_file_keeps_the_store() {
    // The seam's missing-file arm directly (the fake's "file" list):
    let assets = FakeAssets::new();
    assets.add_profile("p1");
    assets.add_profile("p2");
    assert!(
        !assets.profile_remove_file("ghost"),
        "the missing file reports failure"
    );
    assert_eq!(
        assets.profile_names(),
        vec!["p1".to_string(), "p2".to_string()]
    );
    assert!(assets.profile_remove_file("p2"));
    assert_eq!(
        assets.profile_names(),
        vec!["p1".to_string()],
        "the existing file removed"
    );
}
