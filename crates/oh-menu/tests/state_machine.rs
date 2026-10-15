//! §11 state-machine test: drive a scripted input sequence through `MenuGame`
//! and assert the state transitions *and* the currently-selected item,
//! mirroring the C++ `MenuGame` behavior.

use std::cell::RefCell;
use std::rc::Rc;

use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::NewGameCallback;
use oh_menu::{
    FakeAssets, FakeConfig, MenuAssets, MenuConfig, MenuGame, NoopNetwork, RecordingAudio, States,
};

/// Recorded `fnHGNewGame(pack, level)` call log.
type NewGameLog = Rc<RefCell<Vec<(String, String)>>>;

fn make_menu() -> (MenuGame, NewGameLog) {
    let assets: Rc<dyn MenuAssets> = {
        let fa = FakeAssets::new();
        fa.add_pack("cube", &["cube1", "cube2", "cube3"]);
        fa.add_pack("orthoplex", &["ortho1"]);
        fa.add_profile("P1");
        fa
    };
    let config: Rc<dyn MenuConfig> = FakeConfig::new();
    let audio = RecordingAudio::new();
    let recorded = Rc::new(RefCell::new(Vec::<(String, String)>::new()));
    let rec = recorded.clone();
    let cb: Option<NewGameCallback> = Some(Box::new(move |pack, level, _fp, _dm, _er| {
        rec.borrow_mut().push((pack.to_string(), level.to_string()));
    }));
    let menu = MenuGame::new(
        assets,
        config,
        Rc::new(NoopNetwork),
        audio,
        cb,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    (menu, recorded)
}

/// Boot the menu through the two boot screens to the main menu (with one
/// existing profile → SLPSelectBoot → ok → SMain).
fn boot_to_main(menu: &MenuGame) {
    assert_eq!(menu.state(), States::LoadingScreen);
    menu.advance_boot_screen();
    assert_eq!(menu.state(), States::EpilepsyWarning);
    menu.advance_boot_screen();
    assert_eq!(menu.state(), States::SLPSelectBoot);
    // Profile selection lists the single profile.
    assert_eq!(menu.selected_item_name().as_deref(), Some("P1"));
    menu.ok_action();
    assert_eq!(menu.state(), States::SMain);
}

#[test]
fn boot_flow_and_level_selection() {
    let (menu, recorded) = make_menu();
    boot_to_main(&menu);

    // Main menu: root "main" category, "LEVEL SELECT" selected.
    assert_eq!(menu.selected_category().as_deref(), Some("main"));
    assert_eq!(menu.selected_item_name().as_deref(), Some("LEVEL SELECT"));

    // LEVEL SELECT → LevelSelection (first selection: pack 0, level 0).
    menu.ok_action();
    assert_eq!(menu.state(), States::LevelSelection);
    assert_eq!(menu.selected_level_index(), 0);
    assert_eq!(menu.selected_level_id(), "cube_cube1");
    // `setLevelData` sets `diffMultIdx` to the index of the first 1.0x
    // multiplier (C++ `MenuGame.cpp:3054-3056`: a loop scanning
    // `difficultyMults` for `== 1.f`), NOT unconditionally 0. The fixture's
    // mults are `[0.5, 1.0, 1.5, 2.0]`, so the initial index is 1.
    assert_eq!(menu.diff_mult_idx(), 1);

    // Right = difficulty up (LevelSelection branch, C++ `MenuGame.cpp:1857-1863`
    //: a raw `++diffMultIdx` with no clamping or wrap).
    menu.right_action();
    assert_eq!(menu.diff_mult_idx(), 2);

    // Down: single level-list move to index 1 (two packs, not at boundary).
    menu.down_action();
    assert_eq!(menu.selected_level_index(), 1);
    assert_eq!(menu.selected_level_id(), "cube_cube2");

    // Ok = play the selected level → fnHGNewGame seam fires.
    menu.ok_action();
    assert_eq!(
        *recorded.borrow(),
        vec![("cube".to_string(), "cube_cube2".to_string())]
    );
}

#[test]
fn main_menu_navigation_to_options_submenus() {
    let (menu, _recorded) = make_menu();
    boot_to_main(&menu);

    // LEVEL SELECT → LOCAL PROFILES → ONLINE → OPTIONS (three downs).
    menu.down_action();
    assert_eq!(menu.selected_item_name().as_deref(), Some("LOCAL PROFILES"));
    menu.down_action();
    assert_eq!(menu.selected_item_name().as_deref(), Some("ONLINE"));
    menu.down_action();
    assert_eq!(menu.selected_item_name().as_deref(), Some("OPTIONS"));

    // Ok on OPTIONS → MOpts, "options" category, first item "GAMEPLAY".
    menu.ok_action();
    assert_eq!(menu.state(), States::MOpts);
    assert_eq!(menu.selected_category().as_deref(), Some("options"));
    assert_eq!(menu.selected_item_name().as_deref(), Some("GAMEPLAY"));

    // Down → CONTROLS; ok → goto "controls" category, first item "keyboard".
    menu.down_action();
    assert_eq!(menu.selected_item_name().as_deref(), Some("CONTROLS"));
    menu.ok_action();
    assert_eq!(menu.selected_category().as_deref(), Some("controls"));
    assert_eq!(menu.selected_item_name().as_deref(), Some("keyboard"));

    // ok → goto "keyboard" category, first bind "rotate ccw".
    menu.ok_action();
    assert_eq!(menu.selected_category().as_deref(), Some("keyboard"));
    assert_eq!(menu.selected_item_name().as_deref(), Some("rotate ccw"));

    // Esc in a subcategory (can go back) → back to "controls".
    menu.exit_action();
    assert_eq!(menu.selected_category().as_deref(), Some("controls"));
    assert_eq!(menu.state(), States::MOpts);
}

#[test]
fn online_menu_visibility_with_disconnected_client() {
    let (menu, _recorded) = make_menu();
    boot_to_main(&menu);

    // Down to ONLINE, ok → MOnline.
    menu.down_action(); // LOCAL PROFILES
    menu.down_action(); // ONLINE
    assert_eq!(menu.selected_item_name().as_deref(), Some("ONLINE"));
    menu.ok_action();
    assert_eq!(menu.state(), States::MOnline);

    // Apply the visibility predicates (the C++ `operator|` enable-when).
    menu.update(1.0 / 60.0);

    // The online menu is the "options"-named category of `online_menu`.
    let items = menu.online_menu.get_category().get_items();
    // items: 0 CONNECT, 1 LOGIN, 2 REGISTER, 3 LOGOUT, 4 DISCONNECT, 5 DELETE
    let enabled: Vec<bool> = items.iter().map(|i| i.borrow().is_enabled()).collect();
    assert_eq!(
        enabled,
        vec![true, false, false, false, false, false],
        "CONNECT enabled while disconnected; all login/register items gated off"
    );
}
