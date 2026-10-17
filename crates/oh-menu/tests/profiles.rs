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

// ===========================================================================
// 29b6 — the created profile's menu item (the audit's row 6)
// ===========================================================================

fn profile_item_names(menu: &MenuGame) -> Vec<String> {
    menu.profile_selection_menu
        .get_category_by_name("profile selection")
        .get_items()
        .iter()
        .map(|item| item.borrow().get_name())
        .collect()
}

/// C++ `okAction`'s profile creation (MenuGame.cpp:2211-2256): the created
/// profile must APPEAR in the "profile selection" menu — the C++ pushes the
/// item (`profiles.create<Single>(enteredStr, …)`), and the audit's point
/// about it: it pushes AT THE SORTED POSITION, not at the end
/// (`profiles.sortByName()`).
#[test]
fn ok_action_create_profile_adds_the_item_at_the_sorted_position() {
    let (menu, assets, audio) = menu_with_profiles(&["c", "a"], Some("a"));
    menu.shared().borrow_mut().change_state_to(States::ETLPNew);
    menu.shared().borrow_mut().entered_str = "b".into();
    menu.ok_action();
    assert_eq!(
        profile_item_names(&menu),
        vec!["a".to_string(), "b".into(), "c".into()],
        "the item landed AT THE SORTED POSITION (the C++ `profiles.sortByName()` re-sort) — NOT appended"
    );
    assert!(
        assets.profile_names().iter().any(|n| n == "b"),
        "the profile was created in the store (the C++ `assets.pCreate`)"
    );
    assert_eq!(
        assets.current_profile_name(),
        Some("b".into()),
        "the C++ `assets.pSetCurrent(enteredStr)`"
    );
    assert_eq!(
        menu.shared().borrow().entered_str,
        "",
        "the C++ `enteredStr = \"\"`"
    );
    assert_eq!(
        menu.state(),
        States::SMain,
        "the C++ `changeStateTo(States::SMain)`"
    );
    // The ETLPNew arm `break`s (C++ :2255) → the okAction tail's select.ogg.
    assert!(
        audio.played.borrow().iter().any(|s| s == "select.ogg"),
        "the ETLPNew arm falls through to the okAction tail"
    );
}

/// The audit's exact question: the C++ `okAction` re-sorts with
/// `profiles.sortByName()` — the comparator is the ITEM name's byte order
/// (the C++ `std::sort`'s default `operator<` on `std::string`), NOT the
/// favorites list's lowercased-name sort and NOT a case-insensitive sort.
/// So "Zebra" sorts BEFORE "apple" ('Z' = 0x5A < 'a' = 0x61).
#[test]
fn ok_action_create_profile_ordering_matches_the_cpp_comparator() {
    let (menu, _assets, _audio) = menu_with_profiles(&["apple"], Some("apple"));
    menu.shared().borrow_mut().change_state_to(States::ETLPNew);
    menu.shared().borrow_mut().entered_str = "Zebra".into();
    menu.ok_action();
    assert_eq!(
        profile_item_names(&menu),
        vec!["Zebra".to_string(), "apple".into()],
        "the C++ `sortByName()` byte-order comparator: uppercase before lowercase (NOT a lowercased or case-insensitive sort)"
    );
}

/// The created item is SELECTABLE: selecting it runs the C++ item's
/// callback (`assets.pSetCurrent(name); changeFavoriteLevelsToProfile();`) —
/// the current profile switches AND the favorites drawer rebuilds from the
/// NEW profile's favorites (the 29b1 rebuild the C++ item callback wires).
#[test]
fn ok_action_created_profile_item_is_selectable_and_sets_the_current_profile() {
    let (menu, assets, _audio) = menu_with_profiles(&["a", "b"], Some("a"));
    // Seed a favorite into "b" (the per-profile store — the 29b1 shape):
    assets.profile_set_current("b");
    assets.add_favorite_level("pA_apple");
    // Create "c" (it becomes current; it has no favorites yet):
    menu.shared().borrow_mut().change_state_to(States::ETLPNew);
    menu.shared().borrow_mut().entered_str = "c".into();
    menu.ok_action();
    // Now sit on "b" and exec it:
    menu.shared()
        .borrow_mut()
        .change_state_to(States::SLPSelect);
    menu.profile_selection_menu.next(); // a → b
    assert_eq!(
        menu.profile_selection_menu.get_item().borrow().get_name(),
        "b",
        "the selection is on the `b` item"
    );
    menu.profile_selection_menu.get_item().borrow_mut().exec();
    assert_eq!(
        assets.current_profile_name(),
        Some("b".into()),
        "the item callback switched the current profile (the C++ `assets.pSetCurrent(name)`)"
    );
    let s = menu.shared().borrow();
    assert_eq!(
        s.fav_slct.level_data_ids,
        vec!["pA_apple".to_string()],
        "the item callback rebuilt the favorites LIST from `b`'s profile (the C++ `changeFavoriteLevelsToProfile()` — the 29b1 rebuild)"
    );
    assert!(
        !s.favorites_active,
        "the C++ item callback rebuilds the list but does NOT switch the drawer (the C++ `changeFavoriteLevelsToProfile` forces the regular drawer only in its EMPTY arm — the F2 switch is separate)"
    );
}

/// The `ETLPNewBoot` creation arm RETURNS from okAction (C++ :2249-2252) —
/// openHexagon.ogg plays, but the okAction tail's select.ogg does NOT.
#[test]
fn ok_action_create_profile_boot_arm_returns_without_the_select_tail() {
    let (menu, assets, audio) = menu_with_profiles(&["a"], Some("a"));
    menu.shared()
        .borrow_mut()
        .change_state_to(States::ETLPNewBoot);
    menu.shared().borrow_mut().entered_str = "b".into();
    menu.ok_action();
    assert_eq!(
        menu.state(),
        States::SMain,
        "the boot arm changed state to SMain"
    );
    assert!(
        audio.played.borrow().iter().any(|s| s == "openHexagon.ogg"),
        "the C++ :2250 `playSoundOverride(\"openHexagon.ogg\")`"
    );
    assert!(
        !audio.played.borrow().iter().any(|s| s == "select.ogg"),
        "the C++ :2251-2252 `return` — the select.ogg tail was skipped"
    );
    assert_eq!(
        assets.current_profile_name(),
        Some("b".into()),
        "the profile was created + set current"
    );
    assert_eq!(profile_item_names(&menu).len(), 2, "the item was added");
}

/// The audit's exact question: is the port's `if entered_str.is_empty()`
/// guard FAITHFUL? Yes — the C++ wraps the whole creation block in
/// `if(!enteredStr.empty())` (C++ :2214), so an empty entered string skips
/// the creation (and the item add) and falls through to the okAction tail.
/// The port must NOT create a profile or add an item, and must still play
/// the tail's select.ogg (the C++ `break` → tail).
#[test]
fn ok_action_empty_entered_str_is_the_cpp_skip_arm() {
    let (menu, assets, audio) = menu_with_profiles(&["a"], Some("a"));
    menu.shared()
        .borrow_mut()
        .change_state_to(States::ETLPNewBoot);
    // entered_str is already empty (the fixture default).
    menu.ok_action();
    assert_eq!(
        assets.profile_names(),
        vec!["a".to_string()],
        "NO profile was created (the C++ `if(!enteredStr.empty())` guard skipped `pCreate`)"
    );
    assert_eq!(
        profile_item_names(&menu),
        vec!["a".to_string()],
        "NO item was added (the guard skipped `profiles.create`)"
    );
    assert_eq!(
        menu.state(),
        States::ETLPNewBoot,
        "the state did NOT advance (the guard skipped `changeStateTo`)"
    );
    assert!(
        audio.played.borrow().iter().any(|s| s == "select.ogg"),
        "the C++ `break` → the okAction tail's select.ogg still plays"
    );
}

/// The duplicate-name guard (C++ :2220-2233): `error.ogg` + dialog box +
/// `setIgnoreAllInputs(2)` + a `return` OUT of okAction — so the okAction
/// tail's select.ogg does NOT play (the C++ `return`, not a `break`). The
/// port previously played select.ogg here (it `break`ed); 29b6 makes it
/// faithful.
#[test]
fn ok_action_duplicate_name_rejects_without_creating_or_playing_select() {
    let (menu, assets, audio) = menu_with_profiles(&["a"], Some("a"));
    menu.shared().borrow_mut().change_state_to(States::ETLPNew);
    menu.shared().borrow_mut().entered_str = "a".into();
    menu.ok_action();
    assert!(
        audio.played.borrow().iter().any(|s| s == "error.ogg"),
        "the C++ :2227 `playSoundOverride(\"error.ogg\")`"
    );
    assert!(
        !menu.shared().borrow().dialog.empty(),
        "the C++ :2228-2230 dialog box is up"
    );
    assert_eq!(
        menu.shared().borrow().ignore_inputs,
        2,
        "the C++ :2231 `setIgnoreAllInputs(2)`"
    );
    assert_eq!(
        assets.profile_names(),
        vec!["a".to_string()],
        "NO second profile was created"
    );
    assert_eq!(
        profile_item_names(&menu),
        vec!["a".to_string()],
        "NO item was added"
    );
    assert_eq!(
        menu.state(),
        States::ETLPNew,
        "the state did NOT advance (the C++ `return`, :2232)"
    );
    assert!(
        !audio.played.borrow().iter().any(|s| s == "select.ogg"),
        "the C++ `return` — the select.ogg tail was NOT played (the 29b6 faithfulness fix)"
    );
}
