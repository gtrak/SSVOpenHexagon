//! Issue-015c (c): the `MenuGame` command-line boot surface
//! (`init` / `init_command_line_level` / `load_command_line_level` /
//! `return_to_level_selection`) — the target of the C++ `main.cpp` boot
//! dispatch (`main.cpp:436-462` callbacks + `main.cpp:488-525` dispatch).

use std::rc::Rc;

use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::NewGameCallback;
use oh_menu::{
    FakeAssets, FakeConfig, MenuAssets, MenuConfig, MenuGame, NoopNetwork, RecordingAudio, States,
};

fn make_menu() -> (MenuGame, Rc<RecordingAudio>) {
    let assets: Rc<dyn MenuAssets> = {
        let fa = FakeAssets::new();
        fa.add_pack("cube", &["cube1", "cube2", "cube3"]);
        fa.add_pack("orthoplex", &["ortho1"]);
        fa.add_profile("P1");
        fa
    };
    let config: Rc<dyn MenuConfig> = FakeConfig::new();
    let audio = RecordingAudio::new();
    let cb: Option<NewGameCallback> = None;
    let menu = MenuGame::new(
        assets,
        config,
        Rc::new(NoopNetwork),
        audio.clone(),
        cb,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    (menu, audio)
}

/// Drive the boot screens to the first profile-selection state (the same
/// starting point `load_command_line_level` assumes in the C++ boot path).
fn boot_to_profile_select(menu: &MenuGame) {
    assert_eq!(menu.state(), States::LoadingScreen);
    menu.advance_boot_screen();
    assert_eq!(menu.state(), States::EpilepsyWarning);
    menu.advance_boot_screen();
    assert_eq!(menu.state(), States::SLPSelectBoot);
}

#[test]
fn init_plays_select_or_error_sound() {
    let (menu, audio) = make_menu();
    boot_to_profile_select(&menu);
    menu.init(false);
    menu.init(true);
    let played = audio.played.borrow();
    assert!(played.contains(&"select.ogg".to_string()));
    assert!(played.contains(&"error.ogg".to_string()));
}

#[test]
fn load_command_line_level_selects_and_lands_in_level_selection() {
    let (menu, _audio) = make_menu();
    boot_to_profile_select(&menu);

    assert!(menu.load_command_line_level("cube", "cube2"));

    // C++ `loadCommandLineLevel` tail (MenuGame.cpp:1756-1773): the boot ends
    // in `LevelSelection` with the requested level selected.
    assert_eq!(menu.state(), States::LevelSelection);
    assert_eq!(menu.selected_level_id(), "cube_cube2");
    assert_eq!(menu.selected_level_index(), 1);
}

#[test]
fn load_command_line_level_rejects_unknown_pack_and_level() {
    let (menu, _audio) = make_menu();
    boot_to_profile_select(&menu);

    assert!(!menu.load_command_line_level("nonexistent", "cube1"));
    assert_eq!(menu.state(), States::SLPSelectBoot); // unchanged

    assert!(!menu.load_command_line_level("cube", "nonexistent"));
    assert_eq!(menu.state(), States::SLPSelectBoot); // unchanged
}

#[test]
fn load_command_line_level_matches_pack_name_not_id() {
    // C++ `loadCommandLineLevel` scans `getPackDatas()` for a matching pack
    // NAME (MenuGame.cpp:1695-1701) — the `-p` argument is the name, not the
    // id.
    let assets: Rc<dyn MenuAssets> = {
        let fa = FakeAssets::new();
        fa.add_pack("cube", &["cube1"]);
        fa.set_pack_name("cube", "the cube pack");
        fa.add_profile("P1");
        fa
    };
    let config: Rc<dyn MenuConfig> = FakeConfig::new();
    let audio = RecordingAudio::new();
    let menu = MenuGame::new(
        assets,
        config,
        Rc::new(NoopNetwork),
        audio,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    menu.advance_boot_screen();
    menu.advance_boot_screen();

    assert!(menu.load_command_line_level("the cube pack", "cube1"));
    assert_eq!(menu.selected_level_id(), "cube_cube1");
    // The id itself is NOT a valid `-p` value.
    assert!(!menu.load_command_line_level("cube", "cube1"));
}

#[test]
fn return_to_level_selection_resets_drawer_and_ignores_one_input() {
    let (menu, _audio) = make_menu();
    boot_to_profile_select(&menu);
    assert!(menu.load_command_line_level("cube", "cube2"));
    assert_eq!(menu.state(), States::LevelSelection);

    // C++ `returnToLevelSelection` (MenuGame.cpp:3365-3370): the level
    // drawer's X offset is forced to 0 and one input is ignored (so the
    // "back" press that triggered it does not also navigate the menu).
    menu.return_to_level_selection();
    assert_eq!(menu.lvl_slct_x_offset(), 0.0);
}
