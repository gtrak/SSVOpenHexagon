//! Issue-015c (c): the `MenuGame` command-line boot surface
//! (`init` / `init_command_line_level` / `load_command_line_level` /
//! `return_to_level_selection`) — the target of the C++ `main.cpp` boot
//! dispatch (`main.cpp:436-462` callbacks + `main.cpp:488-525` dispatch).

use std::rc::Rc;

use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::NewGameCallback;
use oh_menu::{
    FakeAssets, FakeConfig, MenuAssets, MenuConfig, MenuGame, NoopNetwork, NoopTouch,
    RecordingAudio, States,
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
        Rc::new(NoopTouch),
        cb,
        None,
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
        Rc::new(NoopTouch),
        None,
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

/// Issue 025d: the entered-text path (the C++ `TextEntered` handler,
/// MenuGame.cpp:491-498, + `update()`'s `isEnteringText`-gated
/// accumulation, MenuGame.cpp:2862-2878) — the path a fresh install (no
/// profiles → ETLPNewBoot) takes to the main menu: `handle_text_entered`
/// (the handler — the `unicode < 128` filter) fills `enteredChars`,
/// `update()` accumulates into `enteredStr` only in the entering-text
/// states, and `ok_action` (the C++ `okAction`'s ETLPNewBoot branch,
/// MenuGame.cpp:2208-2257) creates the profile and lands in SMain.
#[test]
fn entered_text_fills_the_entered_str_and_ok_action_creates_the_profile() {
    // NO profiles (the fresh-install shape — the C++
    // `getLocalProfilesSize() == 0` branch, MenuGame.cpp:1787).
    let assets: Rc<dyn MenuAssets> = FakeAssets::new();
    let config: Rc<dyn MenuConfig> = FakeConfig::new();
    let audio = RecordingAudio::new();
    let menu = MenuGame::new(
        assets,
        config,
        Rc::new(NoopNetwork),
        audio,
        Rc::new(NoopTouch),
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );

    // The fresh-install boot: LoadingScreen → EpilepsyWarning →
    // `play_locally()` → ETLPNewBoot.
    assert_eq!(menu.state(), States::LoadingScreen);
    menu.advance_boot_screen();
    assert_eq!(menu.state(), States::EpilepsyWarning);
    menu.advance_boot_screen();
    assert_eq!(menu.state(), States::ETLPNewBoot);

    // The C++ `TextEntered` handler (MenuGame.cpp:491-498): ASCII chars
    // land in `enteredChars`; the `unicode < 128` filter drops the
    // non-ASCII (U+00E9).
    menu.handle_text_entered('D');
    menu.handle_text_entered('A');
    menu.handle_text_entered('F');
    menu.handle_text_entered('\u{e9}');
    // The C++ `update()` accumulation (MenuGame.cpp:2862-2878) —
    // alphanumeric → `enteredStr`, then `enteredChars` clears.
    menu.update(0.01);
    assert_eq!(menu.shared().borrow().entered_str, "DAF");
    assert!(menu.shared().borrow().entered_chars.is_empty());

    // The C++ `okAction`'s ETLPNewBoot branch (MenuGame.cpp:2208-2257):
    // the non-empty `enteredStr` creates the profile (sets it current)
    // and lands in SMain (the main menu).
    menu.ok_action();
    assert_eq!(menu.state(), States::SMain);
    assert!(menu.shared().borrow().entered_str.is_empty());

    // The C++ gate (MenuGame.cpp:2862): outside the entering-text
    // states the accumulation is suppressed (the `enteredChars.clear()`
    // at :2878 is unconditional).
    menu.handle_text_entered('x');
    menu.update(0.01);
    assert_eq!(menu.shared().borrow().entered_str, "");
    assert!(menu.shared().borrow().entered_chars.is_empty());
}
