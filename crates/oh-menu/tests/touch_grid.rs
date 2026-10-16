//! issue 025f — host unit tests for the 9-zone touch grid (the 1:1 port
//! of the C++ `MenuGame::update` block, MenuGame.cpp:2802-2835): the
//! thirds math, the `ok` / `left` / `right` / `up` / `down` dispatch, the
//! single-finger-only rule, and the exact-boundary no-ops.
//!
//! The grid reads the platform's `MenuTouch` seam (the C++'s
//! `window.getFingerDownCount()` / `getFingerDownPositions()` — LOGICAL
//! window coordinates); the windowed / Android path feeds it the
//! window's touch-event fold sink (`oh_app::windowed::MenuTouchSink`,
//! the same data the engine's 25b SG5 `Renderer` seam reads), and these
//! tests feed it the `FingerTouch` fake below.
//!
//! NOTE: the `Shared` layout defaults are NOT what the grid runs
//! against — `refresh_camera` (the boot path's `play_locally`) recomputes
//! `w` / `h` from the config size (the C++ `refreshCamera`,
//! MenuGame.cpp:3131-3168): every test derives its zone coordinates from
//! `MenuGame::window_size()` (the C++ `getWindowWidth()` /
//! `getWindowHeight()` port — the `getFingerDownPositions()` coordinate
//! space).

use std::cell::RefCell;
use std::rc::Rc;

use oh_math::ssvs::Vec2;
use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::MenuTouch;
use oh_menu::{
    FakeAssets, FakeConfig, FakeWindow, MenuAssets, MenuGame, NoopNetwork, RecordingAudio, States,
};

// ---------------------------------------------------------------------------
// The test touch seam: explicit down-finger positions (LOGICAL coords).
// ---------------------------------------------------------------------------

struct FingerTouch {
    down: RefCell<Vec<Vec2>>,
}

impl MenuTouch for FingerTouch {
    fn finger_down_count(&self) -> usize {
        self.down.borrow().len()
    }
    fn finger_down_positions(&self) -> Vec<Vec2> {
        self.down.borrow().clone()
    }
}

fn make_assets(profiles: &[&str]) -> Rc<dyn MenuAssets> {
    let fa = FakeAssets::new();
    fa.add_pack("cube", &["cube1", "cube2", "cube3"]);
    fa.add_pack("orthoplex", &["ortho1"]);
    for p in profiles {
        fa.add_profile(p);
    }
    fa
}

/// The fixture (the `state_machine.rs` `make_menu` shape — two packs,
/// optional profiles) with the `FingerTouch` seam in place of
/// `NoopTouch`.
fn menu_with_touch(profiles: &[&str]) -> (MenuGame, Rc<RecordingAudio>, Rc<FingerTouch>) {
    let audio = RecordingAudio::new();
    let touch = Rc::new(FingerTouch {
        down: RefCell::new(Vec::new()),
    });
    let menu = MenuGame::new(
        make_assets(profiles),
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        audio.clone(),
        touch.clone(),
        FakeWindow::new(),
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    (menu, audio, touch)
}

/// Press ONE finger at `(x, y)` (LOGICAL coords) for the next `update`.
fn tap(touch: &Rc<FingerTouch>, x: f32, y: f32) {
    *touch.down.borrow_mut() = vec![Vec2::new(x, y)];
}

/// Boot through the two boot screens to the main menu (the
/// `state_machine.rs` `boot_to_main` shape, with one profile).
fn boot_to_main(menu: &MenuGame) {
    menu.advance_boot_screen();
    menu.advance_boot_screen();
    assert_eq!(menu.state(), States::SLPSelectBoot);
    menu.ok_action();
    assert_eq!(menu.state(), States::SMain);
}

fn audio_contains(audio: &Rc<RecordingAudio>, id: &str) -> bool {
    audio.played.borrow().iter().any(|s| s == id)
}

// ---------------------------------------------------------------------------
// The zones (one finger down)
// ---------------------------------------------------------------------------

/// The 025f acceptance path in the host: fresh install, the IME
/// `TextEntered` chars arrive (`handle_text_entered` — the C++'s
/// `onEvent` handler), then ONE finger in the center zone (the C++'s
/// `okAction` zone) creates the profile and advances to the main menu.
#[test]
fn center_zone_ok_creates_profile_from_entered_name() {
    let (menu, audio, touch) = menu_with_touch(&[]);
    // Fresh-install boot (no profiles): LoadingScreen →
    // EpilepsyWarning → ETLPNewBoot.
    menu.advance_boot_screen();
    menu.advance_boot_screen();
    assert_eq!(menu.state(), States::ETLPNewBoot);

    for c in "test".chars() {
        menu.handle_text_entered(c);
    }

    // The touch submit: the middle band's center third (the C++'s
    // `okAction` zone, MenuGame.cpp:2825-2829).
    let (w, h) = menu.window_size();
    tap(&touch, w / 2.0, h / 2.0);
    menu.update(0.016);

    // The profile was created and the state advanced (the C++'s
    // `okAction` entering-text arm: `openHexagon.ogg` + `SMain`).
    assert_eq!(menu.state(), States::SMain);
    assert!(audio_contains(&audio, "openHexagon.ogg"));
}

/// The middle band's LEFT third → `leftAction` (the LevelSelection
/// branch: difficulty down — a state-level observable unique to
/// `leftAction`, MenuGame.cpp:2816-2819).
#[test]
fn left_zone_in_level_selection_steps_difficulty_down() {
    let (menu, audio, touch) = menu_with_touch(&["P1"]);
    boot_to_main(&menu);
    menu.ok_action(); // SMain "LEVEL SELECT" → LevelSelection
    assert_eq!(menu.state(), States::LevelSelection);
    let before = menu.diff_mult_idx();

    let (w, h) = menu.window_size();
    tap(&touch, w / 6.0, h / 2.0);
    menu.update(0.016);

    assert_eq!(menu.diff_mult_idx(), before - 1);
    assert!(audio_contains(&audio, "difficultyMultDown.ogg"));
}

/// The middle band's RIGHT third → `rightAction` (difficulty up — the
/// `rightAction`-unique sound, MenuGame.cpp:2820-2823).
#[test]
fn right_zone_in_level_selection_steps_difficulty_up() {
    let (menu, audio, touch) = menu_with_touch(&["P1"]);
    boot_to_main(&menu);
    menu.ok_action();
    assert_eq!(menu.state(), States::LevelSelection);
    let before = menu.diff_mult_idx();

    let (w, h) = menu.window_size();
    tap(&touch, 5.0 * w / 6.0, h / 2.0);
    menu.update(0.016);

    assert_eq!(menu.diff_mult_idx(), before + 1);
    assert!(audio_contains(&audio, "difficultyMultUp.ogg"));
}

/// The TOP band → `upAction` (the SMain menu step: "LEVEL SELECT" →
/// previous item, wrapping to the last main-menu item "EXIT",
/// MenuGame.cpp:2831-2833).
#[test]
fn top_zone_in_smain_moves_selection_up() {
    let (menu, _audio, touch) = menu_with_touch(&["P1"]);
    boot_to_main(&menu);
    assert_eq!(menu.selected_item_name().as_deref(), Some("LEVEL SELECT"));

    let (w, h) = menu.window_size();
    tap(&touch, w / 2.0, h / 6.0);
    menu.update(0.016);

    assert_eq!(menu.selected_item_name().as_deref(), Some("EXIT"));
}

/// The BOTTOM band → `downAction` (the SMain menu step: "LEVEL SELECT"
/// → next item "LOCAL PROFILES", MenuGame.cpp:2834-2836).
#[test]
fn bottom_zone_in_smain_moves_selection_down() {
    let (menu, _audio, touch) = menu_with_touch(&["P1"]);
    boot_to_main(&menu);
    assert_eq!(menu.selected_item_name().as_deref(), Some("LEVEL SELECT"));

    let (w, h) = menu.window_size();
    tap(&touch, w / 2.0, 5.0 * h / 6.0);
    menu.update(0.016);

    assert_eq!(menu.selected_item_name().as_deref(), Some("LOCAL PROFILES"));
}

// ---------------------------------------------------------------------------
// The single-finger-only rule + the exact-boundary no-ops
// ---------------------------------------------------------------------------

/// ZERO fingers down: the grid never runs (the C++'s
/// `getFingerDownCount() == 1` gate, MenuGame.cpp:2802).
#[test]
fn no_fingers_no_action() {
    let (menu, audio, _touch) = menu_with_touch(&["P1"]);
    boot_to_main(&menu);
    let before_name = menu.selected_item_name();
    let before_audio = audio.played.borrow().len();

    menu.update(0.016);

    assert_eq!(menu.selected_item_name(), before_name);
    assert_eq!(audio.played.borrow().len(), before_audio);
}

/// TWO fingers down: the grid never runs (the C++'s
/// `getFingerDownCount() == 1` gate — `== 1`, not `>= 1`).
#[test]
fn two_fingers_no_action() {
    let (menu, audio, touch) = menu_with_touch(&["P1"]);
    boot_to_main(&menu);
    let before_name = menu.selected_item_name();
    let before_audio = audio.played.borrow().len();

    *touch.down.borrow_mut() = {
        let (w, h) = menu.window_size();
        vec![
            Vec2::new(w / 6.0, h / 2.0),       // would be the left zone alone
            Vec2::new(5.0 * w / 6.0, h / 2.0), // would be the right zone alone
        ]
    };
    menu.update(0.016);

    assert_eq!(menu.selected_item_name(), before_name);
    assert_eq!(audio.played.borrow().len(), before_audio);
}

/// The exact third-lines fire NO action (the C++'s strict inequalities,
/// MenuGame.cpp:2810-2836: a position exactly on `hThird` / `hLT` /
/// `wThird` matches no zone):
#[test]
fn exact_third_boundaries_fire_no_action() {
    let (menu, audio, touch) = menu_with_touch(&["P1"]);
    boot_to_main(&menu);
    menu.ok_action(); // → LevelSelection (where any misfire leaves a
                      // sound / difficulty-index trace)
    assert_eq!(menu.state(), States::LevelSelection);
    let before = menu.diff_mult_idx();
    let before_audio = audio.played.borrow().len();
    let (w, h) = menu.window_size();

    // `p.y == hThird`: not `> hThird` (no middle band), not `< hThird`
    // (no up).
    tap(&touch, w / 2.0, h / 3.0);
    menu.update(0.016);
    // `p.y == hLT`: not `< hLT` (no middle band), not `> hLT` (no down).
    tap(&touch, w / 2.0, h - h / 3.0);
    menu.update(0.016);
    // `p.x == wThird` inside the middle band: not `< wThird` (no left),
    // not `> wThird` (no ok), not `> wLT` (no right).
    tap(&touch, w / 3.0, h / 2.0);
    menu.update(0.016);

    assert_eq!(menu.diff_mult_idx(), before);
    assert_eq!(audio.played.borrow().len(), before_audio);
}
