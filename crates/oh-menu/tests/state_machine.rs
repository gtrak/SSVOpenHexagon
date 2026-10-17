//! §11 state-machine test: drive a scripted input sequence through `MenuGame`
//! and assert the state transitions *and* the currently-selected item,
//! mirroring the C++ `MenuGame` behavior.

use std::cell::RefCell;
use std::rc::Rc;

use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::{ConnectionState, NetEvent, NetworkClient, NewGameCallback};
use oh_menu::{
    DialogInputState, FakeAssets, FakeConfig, FakeWindow, MenuAssets, MenuConfig, MenuGame,
    NoMenuJoystick, NoopNetwork, NoopTouch, RecordingAudio, States,
};

/// Recorded `fnHGNewGame(pack, level)` call log.
type NewGameLog = Rc<RefCell<Vec<(String, String)>>>;

fn make_menu() -> (MenuGame, NewGameLog, Rc<RecordingAudio>) {
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
        audio.clone(),
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        cb,
        None,
        None,
        None,
        None, // `unlock_achievement` — the 29b2/29b8 Steam seam (None in the tests)
        Rc::new(LinearFontHeight),
    );
    (menu, recorded, audio)
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
    let (menu, recorded, _audio) = make_menu();
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
    let (menu, _recorded, _audio) = make_menu();
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
    let (menu, _recorded, _audio) = make_menu();
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

// ---------------------------------------------------------------------------
// issue 025g — the C++ release-tail `--ignoreInputs` decrement
// (MenuGame.cpp:576-601 / :644-666 / :694-700)
// ---------------------------------------------------------------------------

/// The C++ `fnGoToMenu` (main.cpp:456-462) runs `returnToLevelSelection()`
/// — `setIgnoreAllInputs(1)` — on the game → menu drop; the C++'s first
/// release after the drop takes `ignoreInputs` 1 → 0 (the shared tail of
/// `KeyReleased` / `MouseButtonReleased` / `JoystickButtonReleased`),
/// unblocking the menu's key binds. The port's on-device 025g finding:
/// without the tail the menu stayed input-dead after the round trip.
#[test]
fn release_edge_decrements_ignore_inputs_after_return_to_level_selection() {
    let (menu, _recorded, _audio) = make_menu();
    boot_to_main(&menu);

    // LEVEL SELECT (the C++ SMain's first item) → LevelSelection:
    menu.ok_action();
    assert_eq!(menu.state(), States::LevelSelection);

    // The `fnGoToMenu` half (the C++'s game → menu drop):
    menu.return_to_level_selection();
    assert_eq!(menu.shared().borrow().ignore_inputs, 1);

    // The first release decrements 1 → 0 (the C++ `--ignoreInputs` tail;
    // the keyboard key identity is irrelevant to the generic tail — any
    // key works, 29b3):
    menu.handle_release_edge(Some(oh_config::Key::A));
    assert_eq!(menu.shared().borrow().ignore_inputs, 0);

    // A second release at 0 is the no-op head (`if(ignoreInputs == 0)
    // return;`): no state drift.
    menu.handle_release_edge(Some(oh_config::Key::A));
    assert_eq!(menu.shared().borrow().ignore_inputs, 0);
    assert_eq!(menu.state(), States::LevelSelection);
}

/// Multi-press holds (the C++'s `setIgnoreAllInputs(2)` dialog-error
/// shape): each release decrements once; the binds stay blocked until
/// the counter reaches 0.
#[test]
fn release_edge_holds_ignore_inputs_above_one_until_each_release() {
    let (menu, _recorded, _audio) = make_menu();
    boot_to_main(&menu);
    menu.shared().borrow_mut().set_ignore_all_inputs(2);

    menu.handle_release_edge(Some(oh_config::Key::A));
    assert_eq!(menu.shared().borrow().ignore_inputs, 1);
    menu.handle_release_edge(Some(oh_config::Key::A));
    assert_eq!(menu.shared().borrow().ignore_inputs, 0);
}

/// Scenario 1 (the boot screens — the caller's `advance_boot_screen`
/// owns the release; the tail must not double-decrement) + scenario 2
/// (a dialog delay holds the decrement).
#[test]
fn release_edge_noop_on_boot_screens_and_during_dialog_delay() {
    let (menu, _recorded, _audio) = make_menu();

    // LoadingScreen (ignore held at 1): the tail's scenario-1 `return`
    // — the boot advance (the caller's `advance_boot_screen`) owns it.
    assert_eq!(menu.state(), States::LoadingScreen);
    menu.handle_release_edge(Some(oh_config::Key::A));
    assert_eq!(
        menu.shared().borrow().ignore_inputs,
        1,
        "the boot screen is untouched by the tail"
    );

    boot_to_main(&menu);
    menu.shared().borrow_mut().dialog_box_delay = 64.0;
    menu.shared().borrow_mut().set_ignore_all_inputs(1);
    menu.handle_release_edge(Some(oh_config::Key::A));
    assert_eq!(
        menu.shared().borrow().ignore_inputs,
        1,
        "the dialog delay holds the decrement (scenario 2)"
    );
}

/// Scenario 3: an open dialog box with an unknown close key closes on
/// the release that TAKES THE COUNTER TO 0 — the C++
/// `--ignoreInputs; checkCloseDialogBox();` (the counter gate, 29b3:
/// `setIgnoreAllInputs(2)` dialogs need TWO releases — the C++ comment,
/// MenuGame.cpp:3119-3121: "First key release is the one of the key
/// press that made the dialog box pop up"). The first release here
/// decrements 2 → 1 (the box stays open); the second decrements 1 → 0
/// and runs the `closeBox` (select sound + `clearDialogBox` +
/// `setIgnoreAllInputs(0)`).
#[test]
fn release_edge_closes_an_open_dialog_box() {
    let (menu, _recorded, audio) = make_menu();
    boot_to_main(&menu);

    menu.shared()
        .borrow_mut()
        .show_dialog_box("PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n");
    menu.shared().borrow_mut().set_ignore_all_inputs(2);

    // Release 1: the `--ignoreInputs` 2 → 1; `checkCloseDialogBox`'s
    // counter gate holds (the box is still open).
    menu.handle_release_edge(Some(oh_config::Key::A));
    assert_eq!(menu.shared().borrow().ignore_inputs, 1);
    assert!(
        !menu.shared().borrow().dialog.empty(),
        "release 1 does not close"
    );

    // Release 2: 1 → 0; the `closeBox` fires.
    menu.handle_release_edge(Some(oh_config::Key::A));
    assert!(
        menu.shared().borrow().dialog.empty(),
        "the box cleared (the C++ closeBox)"
    );
    assert_eq!(menu.shared().borrow().ignore_inputs, 0);
    assert!(
        audio.played.borrow().iter().any(|s| s == "select.ogg"),
        "the closeBox select sound"
    );
}

// ---------------------------------------------------------------------------
// 29b3 — the dialog input state machine (MenuGame.cpp:384-489 + :491-517
// + :552-561): the character echo, the `checkCloseDialogBox` sequence
// machine, and the Escape cancel. The network calls stay behind the
// `NetworkClient` seam (a recording fake — no server).
// ---------------------------------------------------------------------------

/// A `NetworkClient` that records the account calls (29b3: the sequence
/// machine's `try_register` / `try_login` / `try_delete_account`
/// assertions without a server).
#[derive(Default)]
struct RecordingNetwork {
    register: RefCell<Vec<(String, String)>>,
    login: RefCell<Vec<(String, String)>>,
    delete_account: RefCell<Vec<String>>,
}

impl RecordingNetwork {
    #[must_use]
    fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }
}

impl NetworkClient for RecordingNetwork {
    fn state(&self) -> ConnectionState {
        ConnectionState::Connected
    }
    fn has_rt_keys(&self) -> bool {
        true
    }
    fn connect(&self) {}
    fn disconnect(&self) {}
    fn try_login(&self, username: &str, password: &str) {
        self.login
            .borrow_mut()
            .push((username.to_string(), password.to_string()));
    }
    fn try_register(&self, username: &str, password: &str) {
        self.register
            .borrow_mut()
            .push((username.to_string(), password.to_string()));
    }
    fn try_logout(&self) {}
    fn try_delete_account(&self, password: &str) {
        self.delete_account.borrow_mut().push(password.to_string());
    }
    fn poll_event(&self) -> Option<NetEvent> {
        None
    }
    fn update(&self) {}
}

fn make_menu_recording_net() -> (MenuGame, Rc<RecordingNetwork>, Rc<RecordingAudio>) {
    let assets: Rc<dyn MenuAssets> = {
        let fa = FakeAssets::new();
        fa.add_pack("cube", &["cube1", "cube2", "cube3"]);
        fa.add_pack("orthoplex", &["ortho1"]);
        fa.add_profile("P1");
        fa
    };
    let config: Rc<dyn MenuConfig> = FakeConfig::new();
    let audio = RecordingAudio::new();
    let net = RecordingNetwork::new();
    let menu = MenuGame::new(
        assets,
        config,
        net.clone(),
        audio.clone(),
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        None,
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    (menu, net, audio)
}

/// The C++ `REGISTER` item's entry half (MenuGame.cpp:1619-1630):
/// `dialogInputState = Registration_EnteringUsername` + the nice dialog +
/// `ignoreInputsAfterMenuExec()` (the port's `must_use_menu_item` is never
/// set — the documented no-hover half — so the counter is 2, the C++
/// keyboard-navigation shape).
fn open_register_username_dialog(menu: &MenuGame) {
    let mut s = menu.shared().borrow_mut();
    s.dialog_input_state = DialogInputState::Registration_EnteringUsername;
    s.show_input_dialog_box_nice("REGISTRATION", "USERNAME", "");
    s.set_ignore_all_inputs(2);
}

/// Type a string into the open dialog (one `handle_text_entered` per
/// char — the app layer's per-char `TextEntered` fold).
fn type_chars(menu: &MenuGame, text: &str) {
    for c in text.chars() {
        menu.handle_text_entered(c);
    }
}

/// Asserts the open dialog's lines are the C++'s exact nice-dialog
/// string (MenuGame.cpp:6119-6145): `title + "\n\nPLEASE INSERT " +
/// inputType + "\n\nCONFIRM WITH [ENTER]\nCANCEL WITH [ESCAPE]\n"`
/// (the `\n`-terminated split yields the trailing empty line — D1).
/// Asserts the open dialog's lines are the C++'s exact nice-dialog
/// string (MenuGame.cpp:6119-6145): `title + "\n\nPLEASE INSERT " +
/// inputType + "\n\nCONFIRM WITH [ENTER]\nCANCEL WITH [ESCAPE]\n"`
/// (split-invariant: the `\n`-joined lines, minus the message's
/// trailing `\n`).
fn assert_nice_dialog(menu: &MenuGame, title: &str, input_type: &str) {
    let joined: String = menu
        .shared()
        .borrow()
        .dialog
        .dialog_text()
        .to_vec()
        .join("\n");
    let expected = format!(
        "{title}\n\nPLEASE INSERT {input_type}\n\nCONFIRM WITH [ENTER]\nCANCEL WITH [ESCAPE]"
    );
    assert_eq!(
        joined.trim_end_matches('\n'),
        expected,
        "the exact nice-dialog string"
    );
}

#[test]
fn dialog_echo_appends_printable_chars_with_the_32_cap() {
    let (menu, _net, audio) = make_menu_recording_net();
    boot_to_main(&menu);
    open_register_username_dialog(&menu);

    type_chars(&menu, "abc");
    assert_eq!(menu.shared().borrow().dialog.input(), "abc");
    assert_eq!(
        audio
            .played
            .borrow()
            .iter()
            .filter(|s| s.as_str() == "beep.ogg")
            .count(),
        3,
        "a beep per appended char"
    );

    // The 32-char cap (C++ :501-508 — `input.size() < 32`): the 30th
    // char lands at 32, the 31st onward is ignored (no beep, no
    // append).
    let filler: String = "a".repeat(30);
    type_chars(&menu, &filler);
    assert_eq!(menu.shared().borrow().dialog.input().len(), 32);
    assert_eq!(
        audio
            .played
            .borrow()
            .iter()
            .filter(|s| s.as_str() == "beep.ogg")
            .count(),
        32,
        "3 + 29 capped appends (the 30th 'a' is over the cap)"
    );
    type_chars(&menu, "xy");
    assert_eq!(menu.shared().borrow().dialog.input().len(), 32);
    assert_eq!(
        audio
            .played
            .borrow()
            .iter()
            .filter(|s| s.as_str() == "beep.ogg")
            .count(),
        32,
        "over-cap chars are ignored (no beep)"
    );

    // The non-printable unicode is NOT echoed (the C++'s `32..126`
    // range; the Backspace half is the separate arm below):
    menu.handle_text_entered('\u{1}');
    assert_eq!(menu.shared().borrow().dialog.input().len(), 32);
    menu.handle_text_entered('\u{7f}');
    assert_eq!(menu.shared().borrow().dialog.input().len(), 32);

    // The 1st `TextEntered` handler is INDEPENDENT (the C++ fires both
    // per event): the profile-name feed carries every `unicode < 128`
    // char — the 3 echo-able chars + 30 filler 'a's + the 2 over-cap
    // chars + the 2 control chars (all < 128; the 32-cap applies to the
    // DIALOG echo only, never to `enteredChars`).
    assert_eq!(menu.shared().borrow().entered_chars.len(), 37);
}

#[test]
fn dialog_echo_backspace_pops_with_beep() {
    let (menu, _net, audio) = make_menu_recording_net();
    boot_to_main(&menu);
    open_register_username_dialog(&menu);

    type_chars(&menu, "ab");
    assert_eq!(menu.shared().borrow().dialog.input(), "ab");

    // Backspace (unicode 8 — the C++'s second `TextEntered` handler's
    // `pop_back` arm, :509-516):
    menu.handle_text_entered('\u{8}');
    assert_eq!(menu.shared().borrow().dialog.input(), "a");
    assert_eq!(
        audio
            .played
            .borrow()
            .iter()
            .filter(|s| s.as_str() == "beep.ogg")
            .count(),
        3,
        "2 appends + 1 pop beep"
    );

    // The pop is gated on a NON-EMPTY input (the C++'s
    // `if(!input.empty())`): the second backspace still pops the
    // remaining "a" (a beep), the third is on empty input (no beep).
    menu.handle_text_entered('\u{8}');
    assert_eq!(menu.shared().borrow().dialog.input(), "");
    assert_eq!(
        audio
            .played
            .borrow()
            .iter()
            .filter(|s| s.as_str() == "beep.ogg")
            .count(),
        4,
        "3 appends/pops + 1 for popping the last char"
    );
    menu.handle_text_entered('\u{8}');
    assert_eq!(menu.shared().borrow().dialog.input(), "");
    assert_eq!(
        audio
            .played
            .borrow()
            .iter()
            .filter(|s| s.as_str() == "beep.ogg")
            .count(),
        4,
        "the empty-input backspace adds no beep"
    );

    // The echo is gated on the INPUT BOX (a plain dialog's input field is
    // unreachable — the C++'s `!dialogBox.isInputBox()` early return):
    menu.shared()
        .borrow_mut()
        .show_dialog_box("A PLAIN DIALOG\n");
    type_chars(&menu, "zz");
    assert_eq!(menu.shared().borrow().dialog.input(), "");
}

#[test]
fn registration_sequence_username_to_password_is_masked_and_captures() {
    let (menu, _net, _audio) = make_menu_recording_net();
    boot_to_main(&menu);
    open_register_username_dialog(&menu);

    // The entry dialog is the C++'s exact nice string, UNMASKED (the
    // username is typed in the clear):
    assert_nice_dialog(&menu, "REGISTRATION", "USERNAME");
    assert!(!menu.shared().borrow().dialog.input_box_password());

    type_chars(&menu, "user");

    // Release 1 (the C++ keyboard-navigation shape — `ignoreInputs`
    // 2 → 1, the counter gate holds; the box is untouched):
    menu.handle_release_edge(Some(oh_config::Key::ENTER));
    assert_eq!(menu.shared().borrow().ignore_inputs, 1);
    assert_eq!(
        menu.shared().borrow().dialog_input_state,
        DialogInputState::Registration_EnteringUsername,
        "the first release is absorbed by the counter gate"
    );

    // Release 2 (1 → 0): the `RegUsername` arm — capture + the MASKED
    // re-dialog (the C++ :419-427):
    menu.handle_release_edge(Some(oh_config::Key::ENTER));
    let s = menu.shared().borrow();
    assert_eq!(s.registration_username, "user");
    assert_eq!(
        s.dialog_input_state,
        DialogInputState::Registration_EnteringPassword
    );
    assert_nice_dialog(&menu, "REGISTRATION", "PASSWORD");
    assert!(s.dialog.is_input_box());
    assert!(
        s.dialog.input_box_password(),
        "the re-dialog is masked (setInputBoxPassword(true))"
    );
    assert_eq!(s.dialog.input(), "", "the re-dialog's input is empty");
    assert_eq!(s.ignore_inputs, 1, "setIgnoreAllInputs(1)");
}

#[test]
fn registration_happy_path_calls_try_register_with_the_typed_values() {
    let (menu, net, _audio) = make_menu_recording_net();
    boot_to_main(&menu);
    open_register_username_dialog(&menu);

    type_chars(&menu, "user");
    menu.handle_release_edge(Some(oh_config::Key::ENTER)); // absorbed
    menu.handle_release_edge(Some(oh_config::Key::ENTER)); // → password

    type_chars(&menu, "pass");
    menu.handle_release_edge(Some(oh_config::Key::ENTER)); // → confirm
    let s = menu.shared().borrow();
    assert_eq!(s.registration_password, "pass");
    assert_eq!(
        s.dialog_input_state,
        DialogInputState::Registration_EnteringPasswordConfirm
    );
    assert_nice_dialog(&menu, "REGISTRATION", "CONFIRM PASSWORD");
    assert!(s.dialog.input_box_password());
    drop(s);

    type_chars(&menu, "pass");
    menu.handle_release_edge(Some(oh_config::Key::ENTER));

    let s = menu.shared().borrow();
    assert_eq!(s.registration_password_confirm, "pass");
    assert_eq!(s.dialog_input_state, DialogInputState::Nothing);
    assert!(s.dialog.empty(), "the closeBox cleared the box");
    assert_eq!(s.ignore_inputs, 0);
    assert_eq!(
        net.register.borrow().as_slice(),
        [("user".to_string(), "pass".to_string())],
        "try_register fired exactly once with the typed values"
    );
}

#[test]
fn registration_mismatch_shows_the_error_dialog_and_does_not_register() {
    let (menu, net, audio) = make_menu_recording_net();
    boot_to_main(&menu);
    open_register_username_dialog(&menu);

    type_chars(&menu, "user");
    menu.handle_release_edge(Some(oh_config::Key::ENTER));
    menu.handle_release_edge(Some(oh_config::Key::ENTER));
    type_chars(&menu, "pass");
    menu.handle_release_edge(Some(oh_config::Key::ENTER));
    type_chars(&menu, "diff"); // the mismatched confirm
    menu.handle_release_edge(Some(oh_config::Key::ENTER));

    let s = menu.shared().borrow();
    assert_eq!(
        net.register.borrow().len(),
        0,
        "the mismatched confirm must NOT call register"
    );
    assert_eq!(
        s.dialog_input_state,
        DialogInputState::Nothing,
        "endInputSequence reset the state"
    );
    assert!(!s.dialog.empty(), "the error dialog is open");
    assert!(!s.dialog.is_input_box(), "the error dialog is a plain box");
    let content: Vec<&str> = s
        .dialog
        .dialog_text()
        .iter()
        .map(String::as_str)
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(
        content,
        [
            "PASSWORD MISMATCH",
            "PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE"
        ],
        "the exact 'PASSWORD MISMATCH' string"
    );
    assert_eq!(s.ignore_inputs, 1, "setIgnoreAllInputs(1)");
    drop(s);
    assert!(
        audio.played.borrow().iter().any(|s| s == "error.ogg"),
        "the mismatch plays error.ogg"
    );

    // The error dialog closes on the next release (the counter 1 → 0 +
    // the `Nothing` arm's `closeBox`):
    menu.handle_release_edge(Some(oh_config::Key::A));
    assert!(menu.shared().borrow().dialog.empty());
    assert_eq!(menu.shared().borrow().ignore_inputs, 0);
}

#[test]
fn login_sequence_calls_try_login_with_the_typed_values() {
    let (menu, net, _audio) = make_menu_recording_net();
    boot_to_main(&menu);

    // The C++ `LOGIN` item's entry (MenuGame.cpp:1607-1617 / :6147-6161):
    // `openLoginDialogBoxAndStartLoginProcess` + `ignoreInputsAfterMenuExec`.
    menu.shared()
        .borrow_mut()
        .open_login_dialog_box_and_start_login_process();
    menu.shared().borrow_mut().set_ignore_all_inputs(2);
    assert_eq!(
        menu.shared().borrow().dialog_input_state,
        DialogInputState::Login_EnteringUsername
    );

    type_chars(&menu, "alice");
    menu.handle_release_edge(Some(oh_config::Key::ENTER)); // absorbed
    menu.handle_release_edge(Some(oh_config::Key::ENTER)); // → password
    let s = menu.shared().borrow();
    assert_eq!(s.login_username, "alice");
    assert_eq!(
        s.dialog_input_state,
        DialogInputState::Login_EnteringPassword
    );
    assert_nice_dialog(&menu, "LOGIN", "PASSWORD");
    assert!(
        s.dialog.input_box_password(),
        "the password field is masked"
    );
    drop(s);

    type_chars(&menu, "secret");
    menu.handle_release_edge(Some(oh_config::Key::ENTER));

    let s = menu.shared().borrow();
    assert_eq!(s.login_password, "secret");
    assert_eq!(s.dialog_input_state, DialogInputState::Nothing);
    assert!(s.dialog.empty());
    assert_eq!(
        net.login.borrow().as_slice(),
        [("alice".to_string(), "secret".to_string())],
        "try_login fired exactly once with the typed values"
    );
}

#[test]
fn delete_account_sequence_calls_try_delete_account_with_the_typed_password() {
    let (menu, net, _audio) = make_menu_recording_net();
    boot_to_main(&menu);

    // The C++ `DELETE ACCOUNT` item's entry (MenuGame.cpp:1641-1655):
    // `DeleteAccount_EnteringPassword` + the nice dialog + the warning +
    // `setInputBoxPassword(true)` + `ignoreInputsAfterMenuExec`.
    let mut s = menu.shared().borrow_mut();
    s.dialog_input_state = DialogInputState::DeleteAccount_EnteringPassword;
    s.show_input_dialog_box_nice(
        "DELETE ACCOUNT",
        "PASSWORD",
        "WARNING: THIS WILL DELETE ALL YOUR SCORES",
    );
    s.dialog.set_input_box_password(true);
    s.set_ignore_all_inputs(2);
    drop(s);

    type_chars(&menu, "pw");
    menu.handle_release_edge(Some(oh_config::Key::ENTER)); // absorbed
    menu.handle_release_edge(Some(oh_config::Key::ENTER));

    let s = menu.shared().borrow();
    assert_eq!(s.delete_account_password, "pw");
    assert_eq!(s.dialog_input_state, DialogInputState::Nothing);
    assert!(s.dialog.empty());
    assert_eq!(net.delete_account.borrow().as_slice(), ["pw".to_string()]);
}

#[test]
fn escape_cancels_a_started_sequence_midway() {
    let (menu, net, audio) = make_menu_recording_net();
    boot_to_main(&menu);
    open_register_username_dialog(&menu);

    type_chars(&menu, "us");

    // The C++ :552-561 — while the input box is open, Escape:
    // `setIgnoreAllInputs(0)` + `dialogInputState = Nothing` + select.ogg
    // + `clearDialogBox`.
    menu.handle_release_edge(Some(oh_config::Key::ESCAPE));
    let s = menu.shared().borrow();
    assert_eq!(s.dialog_input_state, DialogInputState::Nothing);
    assert!(s.dialog.empty());
    assert_eq!(s.ignore_inputs, 0);
    assert_eq!(s.registration_username, "", "nothing was captured");
    drop(s);
    assert!(
        audio.played.borrow().iter().any(|s| s == "select.ogg"),
        "the cancel plays select.ogg"
    );

    // The sequence is DEAD: the subsequent Enters (any key) are no-ops
    // (the head's `ignoreInputs == 0` return) — no capture, no network:
    type_chars(&menu, "er");
    menu.handle_release_edge(Some(oh_config::Key::ENTER));
    menu.handle_release_edge(Some(oh_config::Key::ENTER));
    let s = menu.shared().borrow();
    assert_eq!(s.dialog_input_state, DialogInputState::Nothing);
    assert!(s.dialog.empty());
    assert_eq!(s.ignore_inputs, 0);
    assert!(s.dialog.input().is_empty());
    drop(s);
    assert_eq!(net.register.borrow().len(), 0);
}

#[test]
fn pointer_releases_do_not_advance_the_input_sequence() {
    let (menu, _net, _audio) = make_menu_recording_net();
    boot_to_main(&menu);
    open_register_username_dialog(&menu);
    type_chars(&menu, "us");

    // The C++ mouse / joystick dialog halves (:636-643 / :693-700):
    // `keyToClose != Unknown → return` — the input box (Enter to
    // close) ignores pointer releases entirely (no decrement, no
    // advance):
    menu.handle_release_edge(None);
    menu.handle_release_edge(None);
    let s = menu.shared().borrow();
    assert_eq!(
        s.ignore_inputs, 2,
        "the pointer releases decremented nothing"
    );
    assert_eq!(
        s.dialog_input_state,
        DialogInputState::Registration_EnteringUsername
    );
    assert!(!s.dialog.empty());
    drop(s);

    // A PLAIN (Unknown-key) box still closes on a pointer release (the
    // C++'s `--ignoreInputs; checkCloseDialogBox();`). Model the C++
    // invariant that a plain box is only ever shown with the sequence in
    // `Nothing` (reset the state the registration sequence left in):
    let mut s = menu.shared().borrow_mut();
    s.dialog_input_state = DialogInputState::Nothing;
    s.show_dialog_box("A PLAIN DIALOG\n");
    s.set_ignore_all_inputs(2);
    drop(s);
    menu.handle_release_edge(None); // 2 → 1 (the gate holds)
    assert!(!menu.shared().borrow().dialog.empty());
    menu.handle_release_edge(None); // 1 → 0 (the closeBox)
    assert!(menu.shared().borrow().dialog.empty());
    assert_eq!(menu.shared().borrow().ignore_inputs, 0);
}

#[test]
fn a_non_close_key_release_does_not_decrement_an_input_box() {
    let (menu, _net, _audio) = make_menu_recording_net();
    boot_to_main(&menu);
    open_register_username_dialog(&menu);

    // The C++ Key handler's `--ignoreInputs` fires only on the close key
    // (or the Unknown close key): a random key's release on the input
    // box decrements nothing (the box stays, the sequence holds):
    menu.handle_release_edge(Some(oh_config::Key::L));
    let s = menu.shared().borrow();
    assert_eq!(s.ignore_inputs, 2, "a non-close key does not decrement");
    assert_eq!(
        s.dialog_input_state,
        DialogInputState::Registration_EnteringUsername
    );
    assert!(!s.dialog.empty());
}
