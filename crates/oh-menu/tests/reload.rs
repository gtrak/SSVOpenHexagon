//! 29i1 / 29b9 — the F3/F4 `reloadAssets` handler (C++
//! `MenuGame::reloadAssets`, MenuGame.cpp:3092-3127): the guard
//! (`state == LevelSelection && dialogBox.empty() && Config::getDebug()`),
//! the `reloadAllShaders` + `reloadPack` / `reloadLevel` calls (F3 = the
//! LEVEL, F4 = the PACK), the `setIndex(lvlDrawer->currentIndex)` reload
//! of the level data, the UPPERCASE result dialog, `select.ogg`, and
//! `setIgnoreAllInputs(2)`.

use std::rc::Rc;

use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::{FakeAssets, FakeConfig, FakeWindow, RecordingAudio};
use oh_menu::{MenuConfig, MenuGame, NoMenuJoystick, NoopNetwork, NoopTouch, States};

fn menu_fixture() -> (MenuGame, Rc<FakeAssets>, Rc<FakeConfig>, Rc<RecordingAudio>) {
    let assets = FakeAssets::new();
    assets.add_pack("pA", &["alpha", "beta"]);
    let config = FakeConfig::new();
    let audio = RecordingAudio::new();
    let menu = MenuGame::new(
        assets.clone(),
        config.clone(),
        Rc::new(NoopNetwork),
        audio.clone(),
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        None, // `new_game`
        None, // `trigger_refresh`
        None, // `rich_presence`
        None, // `unlock_achievement` — the 29b2/29b8 Steam seam (None in the tests)
        None, // `ignore_inputs`
        Rc::new(LinearFontHeight),
    );
    (menu, assets, config, audio)
}

/// SMain → `ok_action` (the "LEVEL SELECT" item) → the level selection.
fn into_level_selection(menu: &MenuGame) {
    menu.shared().borrow_mut().change_state_to(States::SMain);
    menu.ok_action();
    assert_eq!(menu.state(), States::LevelSelection);
}

/// The C++'s exact dialog: the uppercased reload result + the blank line +
/// the close line (`HexagonDialogBox::create` splits on '\n' and drops the
/// empty segment after the LAST '\n').
fn assert_reload_dialog(menu: &MenuGame) {
    let s = menu.shared().borrow();
    assert!(
        !s.dialog.empty(),
        "the result dialog box must be open (the C++'s `showDialogBox`)"
    );
    assert_eq!(
        s.dialog.dialog_text(),
        &[
            "FAKE RELOAD RESULT".to_string(),
            String::new(),
            "PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE".to_string(),
        ]
    );
}

/// Debug off → the whole handler is a no-op (the C++'s
/// `Config::getDebug()` gate, MenuGame.cpp:3096-3099): no reload call, no
/// dialog, no sound, the input-ignore gate untouched.
#[test]
fn f3_f4_guard_debug_off() {
    let (menu, assets, config, audio) = menu_fixture();
    into_level_selection(&menu);
    config.set_debug(false);
    let sounds_before = audio.played.borrow().len();

    let ignore_before = menu.shared().borrow().ignore_inputs;
    menu.reload_assets(false);
    menu.reload_assets(true);

    assert!(
        assets.reload_log().is_empty(),
        "no reload call in non-debug"
    );
    assert_eq!(assets.shaders_reload_count(), 0, "no shader reload");
    assert!(menu.shared().borrow().dialog.empty(), "no dialog");
    assert_eq!(
        audio.played.borrow().len(),
        sounds_before,
        "no sound fired (the guard returns before the select.ogg)"
    );
    assert_eq!(
        menu.shared().borrow().ignore_inputs,
        ignore_before,
        "the C++'s `setIgnoreAllInputs(2)` never runs"
    );
}

/// A dialog box open → the guard rejects (the C++'s
/// `!dialogBox.empty()`, MenuGame.cpp:3095): the reload never fires.
#[test]
fn f3_f4_guard_dialog_open() {
    let (menu, assets, config, audio) = menu_fixture();
    into_level_selection(&menu);
    config.set_debug(true);
    // An open dialog box (any — the C++ only checks `empty()`).
    menu.shared()
        .borrow_mut()
        .show_dialog_box("SOME OTHER MESSAGE\n");
    let sounds_before = audio.played.borrow().len();

    menu.reload_assets(false);

    assert!(assets.reload_log().is_empty());
    assert_eq!(assets.shaders_reload_count(), 0);
    // The original dialog is untouched (the reload's `showDialogBox`
    // never ran).
    assert_eq!(
        menu.shared().borrow().dialog.dialog_text(),
        &["SOME OTHER MESSAGE".to_string()]
    );
    assert_eq!(
        audio.played.borrow().len(),
        sounds_before,
        "no sound fired (the guard returns before the select.ogg)"
    );
}

/// Not in the level selection → the guard rejects (the C++'s
/// `state != States::LevelSelection`, MenuGame.cpp:3094).
#[test]
fn f3_f4_guard_wrong_state() {
    let (menu, assets, config, _audio) = menu_fixture();
    config.set_debug(true);
    // SMain (NOT the level selection).
    menu.shared().borrow_mut().change_state_to(States::SMain);

    menu.reload_assets(true);

    assert!(assets.reload_log().is_empty());
    assert_eq!(assets.shaders_reload_count(), 0);
    assert!(menu.shared().borrow().dialog.empty());
}

/// F3 in the guarded state: `reloadAllShaders` + `reloadLevel` (the
/// `entire_pack == false` half), the `setIndex` reload, the uppercase
/// dialog, `select.ogg`, and `setIgnoreAllInputs(2)`.
#[test]
fn f3_reloads_the_level_and_shows_the_uppercase_dialog() {
    let (menu, assets, config, audio) = menu_fixture();
    into_level_selection(&menu);
    config.set_debug(true);
    let level_id = menu
        .shared()
        .borrow()
        .lvl_slct
        .level_data_ids
        .first()
        .cloned()
        .expect("the boot pack has levels");
    let sounds_before = audio.played.borrow().len();

    menu.reload_assets(false);

    // The seam calls: the shader reload first, then the LEVEL reload
    // (`entire_pack == false` — the C++'s F3, MenuGame.cpp:1012-1014)
    // with the current level's id.
    assert_eq!(assets.shaders_reload_count(), 1);
    assert_eq!(assets.reload_log(), vec![(false, level_id.clone())]);

    // The `setIndex(lvlDrawer->currentIndex)` half: the level data is
    // re-read (the port's `current_level` — the C++'s `levelData` — is
    // still the same level, freshly loaded).
    assert_eq!(
        menu.shared()
            .borrow()
            .current_level
            .as_ref()
            .map(|l| l.id.clone()),
        Some(level_id.clone())
    );

    // The UPPERCASE result dialog + the close line.
    assert_reload_dialog(&menu);

    // `select.ogg` + `setIgnoreAllInputs(2)`.
    assert_eq!(
        *audio.played.borrow().get(sounds_before).unwrap(),
        "select.ogg"
    );
    assert_eq!(menu.shared().borrow().ignore_inputs, 2);
}

/// F4: the `entire_pack == true` half (the C++'s F4,
/// MenuGame.cpp:1016-1018) — the PACK reload.
#[test]
fn f4_reloads_the_entire_pack() {
    let (menu, assets, config, _audio) = menu_fixture();
    into_level_selection(&menu);
    config.set_debug(true);
    let level_id = menu
        .shared()
        .borrow()
        .lvl_slct
        .level_data_ids
        .first()
        .cloned()
        .expect("the boot pack has levels");

    menu.reload_assets(true);

    assert_eq!(assets.shaders_reload_count(), 1);
    assert_eq!(assets.reload_log(), vec![(true, level_id)]);
    assert_reload_dialog(&menu);
    assert_eq!(menu.shared().borrow().ignore_inputs, 2);
}
