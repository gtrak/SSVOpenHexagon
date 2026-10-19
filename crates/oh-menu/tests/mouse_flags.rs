//! issue 058 (row 29m) — the C++ `MenuGame::update`'s mouse-driven
//! action block (MenuGame.cpp:2681-2733) consumption: the PREVIOUS
//! frame's draw's click edges (`mustUseMenuItem` / `mustFavorite` /
//! `mustPlay` / `mustChangeIndexTo` / `mustChangePackIndexTo` — the
//! C++'s "TODO (P2): cleanup mouse control" pair) fire their actions
//! in `update`, at most ONE level-selection chain arm per frame (the
//! C++'s `else if` chain), then clear.

use std::cell::RefCell;
use std::rc::Rc;

use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::{FakeAssets, FakeConfig, FakeWindow, NewGameCallback, RecordingAudio};
use oh_menu::{MenuAssets, MenuGame, NoMenuJoystick, NoopNetwork, NoopTouch, States};

/// Recorded `fnHGNewGame(pack, level)` call log (the
/// `state_machine.rs` test's same alias).
type NewGameLog = Rc<RefCell<Vec<(String, String)>>>;

fn make_menu() -> (MenuGame, Rc<FakeAssets>, NewGameLog, Rc<RecordingAudio>) {
    let assets = FakeAssets::new();
    assets.add_pack("cube", &["cube1", "cube2", "cube3"]);
    assets.add_pack("orthoplex", &["ortho1"]);
    assets.add_profile("P1");
    assets.profile_set_current("P1");
    let audio = RecordingAudio::new();
    let recorded = Rc::new(RefCell::new(Vec::<(String, String)>::new()));
    let rec = recorded.clone();
    let cb: Option<NewGameCallback> = Some(Box::new(move |pack, level, _fp, _dm, _er| {
        rec.borrow_mut().push((pack.to_string(), level.to_string()));
    }));
    let menu = MenuGame::new(
        assets.clone(),
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        audio.clone(),
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        cb,
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    (menu, assets, recorded, audio)
}

fn to_level_selection(menu: &MenuGame) {
    menu.shared()
        .borrow_mut()
        .change_state_to(States::LevelSelection);
    assert_eq!(menu.state(), States::LevelSelection);
}

#[test]
fn mouse_hovering_defaults_false_and_is_the_left_right_gate_flag() {
    let (menu, _assets, _rec, _audio) = make_menu();
    assert!(!menu.mouse_hovering());
    menu.shared().borrow_mut().mouse_hovering = true;
    assert!(menu.mouse_hovering());
}

#[test]
fn update_executes_the_must_use_menu_item_click() {
    let (menu, _assets, _rec, audio) = make_menu();
    menu.shared().borrow_mut().change_state_to(States::SMain);
    // The "main" category's item 0 ("LEVEL SELECT" — its exec moves to
    // LevelSelection).
    menu.shared().borrow_mut().must_use_menu_item = Some(0);
    menu.update(1.0);
    assert_eq!(menu.state(), States::LevelSelection);
    // The C++'s `playSoundOverride("beep.ogg")` (MenuGame.cpp:2694) —
    // the RecordingAudio's sound log.
    assert!(
        audio.played.borrow().iter().any(|s| s.contains("beep")),
        "the click's beep.ogg play: {:?}",
        audio.played.borrow()
    );
    // Consumed.
    assert_eq!(menu.shared().borrow().must_use_menu_item, None);
}

#[test]
fn update_skips_the_must_use_menu_item_out_of_range() {
    let (menu, _assets, _rec, _audio) = make_menu();
    menu.shared().borrow_mut().change_state_to(States::SMain);
    // Out of range (the C++'s `items.size() > *mustUseMenuItem` guard):
    // no exec, the flag clears.
    menu.shared().borrow_mut().must_use_menu_item = Some(99);
    menu.update(1.0);
    assert_eq!(menu.state(), States::SMain);
    assert_eq!(menu.shared().borrow().must_use_menu_item, None);
}

#[test]
fn update_consumes_must_change_index_to() {
    let (menu, _assets, _rec, _audio) = make_menu();
    to_level_selection(&menu);
    assert_eq!(menu.shared().borrow().lvl_slct.current_index, 0);
    menu.shared().borrow_mut().must_change_index_to = Some(2);
    menu.update(1.0);
    assert_eq!(menu.shared().borrow().lvl_slct.current_index, 2);
    assert_eq!(menu.shared().borrow().must_change_index_to, None);
}

#[test]
fn update_consumes_must_change_index_to_no_action_when_already_current() {
    let (menu, _assets, _rec, audio) = make_menu();
    to_level_selection(&menu);
    menu.shared().borrow_mut().must_change_index_to = Some(0);
    menu.update(1.0);
    // The C++'s `currentIndex != *mustChangeIndexTo` guard — no beep.
    assert!(
        !audio.played.borrow().iter().any(|s| s.contains("beep")),
        "no beep for the no-op index change: {:?}",
        audio.played.borrow()
    );
    assert_eq!(menu.shared().borrow().must_change_index_to, None);
}

#[test]
fn update_consumes_must_change_pack_index_to() {
    let (menu, _assets, _rec, _audio) = make_menu();
    to_level_selection(&menu);
    assert_eq!(menu.shared().borrow().lvl_slct.pack_idx, 0);
    menu.shared().borrow_mut().must_change_pack_index_to = Some(1);
    menu.update(1.0);
    assert_eq!(menu.shared().borrow().lvl_slct.pack_idx, 1);
    // The C++'s `getLevelIDsFromPack(pack)` refresh (MenuGame.cpp:2729).
    assert_eq!(
        menu.shared().borrow().lvl_slct.level_data_ids,
        vec!["orthoplex_ortho1".to_string()]
    );
    assert_eq!(menu.shared().borrow().must_change_pack_index_to, None);
}

#[test]
fn update_consumes_must_favorite_toggles_the_favorite() {
    let (menu, assets, _rec, _audio) = make_menu();
    to_level_selection(&menu);
    let level_id = {
        let s = menu.shared().borrow();
        s.lvl_slct
            .level_data_ids
            .get(s.lvl_slct.current_index as usize)
            .cloned()
            .expect("a current level")
    };
    menu.shared().borrow_mut().must_favorite = true;
    menu.update(1.0);
    assert!(assets.is_level_favorite(&level_id), "the favorite added");
    assert!(
        menu.shared().borrow().is_level_favorite,
        "the current level's favorite bit"
    );
    // Second click: un-favorite (the C++'s `addRemoveFavoriteLevel`
    // toggle, MenuGame.cpp:3601-3605).
    menu.shared().borrow_mut().must_favorite = true;
    menu.update(1.0);
    assert!(!assets.is_level_favorite(&level_id));
    assert!(!menu.shared().borrow().must_favorite);
}

#[test]
fn update_consumes_must_play_the_double_click() {
    let (menu, _assets, recorded, _audio) = make_menu();
    // The C++'s `mustPlay` consumption sits inside the
    // `state == States::LevelSelection && packChangeState == Rest`
    // guard (MenuGame.cpp:2709-2712) — `playSelectedLevel`'s only
    // inner guard is the `fnHGNewGame` presence (the port's
    // `new_game` seam, MenuGame.cpp:2370-2372).
    to_level_selection(&menu);
    menu.shared().borrow_mut().must_play = true;
    menu.update(1.0);
    assert_eq!(recorded.borrow().len(), 1);
    assert_eq!(
        recorded.borrow()[0],
        ("cube".to_string(), "cube_cube1".to_string())
    );
    assert!(!menu.shared().borrow().must_play);
}

#[test]
fn the_level_selection_click_chain_fires_at_most_one_arm_per_frame() {
    // The C++'s `else if` chain (MenuGame.cpp:2702-2733): all four
    // flags set at once — the favorite wins, the rest stay pending.
    let (menu, _assets, _rec, _audio) = make_menu();
    to_level_selection(&menu);
    {
        let mut s = menu.shared().borrow_mut();
        s.must_favorite = true;
        s.must_play = true;
        s.must_change_index_to = Some(2);
        s.must_change_pack_index_to = Some(1);
    }
    menu.update(1.0);
    {
        let s = menu.shared().borrow();
        assert!(!s.must_favorite);
        assert!(s.must_play, "mustPlay stays pending");
        assert_eq!(s.must_change_index_to, Some(2));
        assert_eq!(s.must_change_pack_index_to, Some(1));
    }
    menu.update(1.0);
    {
        let s = menu.shared().borrow();
        assert!(!s.must_play, "mustPlay consumed by the second frame's play");
        assert_eq!(s.must_change_index_to, Some(2), "still pending");
        assert_eq!(s.must_change_pack_index_to, Some(1));
    }
}
