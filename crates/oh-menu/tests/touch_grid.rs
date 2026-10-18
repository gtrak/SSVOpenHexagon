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
//! issue 054 (F35) — the CADENCE is no longer the C++'s per-frame
//! dispatch: a dispatch arms the `touch_delay` debounce (the C++'s dead
//! `touchDelay` mechanism completed — 50 ms) and the gate holds the next
//! dispatch until it decays. The zone-coordinate tests below model the
//! human tap (the `settle` helper decays the boot path's arms before the
//! tap lands); the F35 tests at the bottom pin the cadence itself.
//!
//! **The `ft` unit here is FT, not seconds** (the C++'s `mFT`'s unit —
//! `getSecondsToFT`: seconds × 60, TimerBase.hpp:42; the runtime's
//! 240 Hz fixed tick passes `TIME_STEP` = 0.25 FT per call): the
//! zone tests pass `update(1.0)` (one 60 fps frame's FT — the value is
//! irrelevant to their assertions), and the F35 cadence section (below)
//! passes `update(1.5)` — 25 ms of wall clock per call, so 50 ms = 2
//! calls EXACTLY in f32 (the arithmetic pinned, not wobbled).
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
    FakeAssets, FakeConfig, FakeWindow, MenuAssets, MenuGame, NoMenuJoystick, NoopNetwork,
    RecordingAudio, States,
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
        Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        None,
        None,
        None,
        None,
        None, // `unlock_achievement` — the 29b2/29b8 Steam seam (None in the tests)
        Rc::new(LinearFontHeight),
    );
    (menu, audio, touch)
}

/// Press ONE finger at `(x, y)` (LOGICAL coords) for the next `update`.
fn tap(touch: &Rc<FingerTouch>, x: f32, y: f32) {
    *touch.down.borrow_mut() = vec![Vec2::new(x, y)];
}

/// F35 — settle the `touch_delay` debounce before a fresh tap: the
/// boot / `ok_action` path arms the 50 ms debounce (the C++'s
/// per-action arms — now read in ms), and a human tap lands after it
/// has decayed. Advance empty frames (no finger down) until the gate
/// opens (4 × 16.67 ms frames covers the 50 ms with room).
fn settle(menu: &MenuGame) {
    for _ in 0..4 {
        if menu.touch_delay() <= 0.0 {
            return;
        }
        menu.update(1.0);
    }
}

/// The SMain step's `beep.ogg` count (the per-dispatch instrument for
/// the F35 cadence tests: every `up_action` / `down_action` SMain step
/// plays it — the C++'s `playSoundOverride("beep.ogg")`).
fn beep_count(audio: &Rc<RecordingAudio>) -> usize {
    audio
        .played
        .borrow()
        .iter()
        .filter(|s| s.as_str() == "beep.ogg")
        .count()
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
    menu.update(1.0);

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
    settle(&menu); // F35: the ok_action's arm decays before the tap
    let before = menu.diff_mult_idx();

    let (w, h) = menu.window_size();
    tap(&touch, w / 6.0, h / 2.0);
    menu.update(1.0);

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
    settle(&menu); // F35: the ok_action's arm decays before the tap
    let before = menu.diff_mult_idx();

    let (w, h) = menu.window_size();
    tap(&touch, 5.0 * w / 6.0, h / 2.0);
    menu.update(1.0);

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
    settle(&menu); // F35: the boot path's arm decays before the tap

    let (w, h) = menu.window_size();
    tap(&touch, w / 2.0, h / 6.0);
    menu.update(1.0);

    assert_eq!(menu.selected_item_name().as_deref(), Some("EXIT"));
}

/// The BOTTOM band → `downAction` (the SMain menu step: "LEVEL SELECT"
/// → next item "LOCAL PROFILES", MenuGame.cpp:2834-2836).
#[test]
fn bottom_zone_in_smain_moves_selection_down() {
    let (menu, _audio, touch) = menu_with_touch(&["P1"]);
    boot_to_main(&menu);
    assert_eq!(menu.selected_item_name().as_deref(), Some("LEVEL SELECT"));
    settle(&menu); // F35: the boot path's arm decays before the tap

    let (w, h) = menu.window_size();
    tap(&touch, w / 2.0, 5.0 * h / 6.0);
    menu.update(1.0);

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

    menu.update(1.0);

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
    menu.update(1.0);

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
    menu.update(1.0);
    // `p.y == hLT`: not `< hLT` (no middle band), not `> hLT` (no down).
    tap(&touch, w / 2.0, h - h / 3.0);
    menu.update(1.0);
    // `p.x == wThird` inside the middle band: not `< wThird` (no left),
    // not `> wThird` (no ok), not `> wLT` (no right).
    tap(&touch, w / 3.0, h / 2.0);
    menu.update(1.0);

    assert_eq!(menu.diff_mult_idx(), before);
    assert_eq!(audio.played.borrow().len(), before_audio);
}

// ---------------------------------------------------------------------------
// F35 (issue 054) — the cadence: a press acts ONCE, a held finger
// auto-repeats at the deliberate 50 ms (the C++'s per-frame dispatch —
// 039's 18 dispatches for one 400 ms held press — is the parity this
// departs from; the operator's "the repeated touch events are a non
// starter").
//
// The frame time here is `1.5` FT per call (the C++'s `mFT`'s unit:
// 60 FT per second — 1.5 FT = 25 ms of wall clock, and 50 ms = 2 calls
// EXACTLY in f32: the decay's `ft * 1000.0 / 60.0` = 25.0 ms per call
// divides the 50 ms debounce with no rounding residue — the arithmetic
// is pinned, not wobbled).
// ---------------------------------------------------------------------------

/// F35 — the arithmetic: a held finger's zone re-fires at most once per
/// 50 ms, not per frame. At 25 ms calls, the 50 ms debounce expires
/// every second call: 12 calls of a held finger in the SMain `down`
/// zone (300 ms) fire exactly 6 dispatches (calls 1, 3, 5, 7, 9, 11 —
/// the first immediate, the edge half), where the C++'s per-frame grid
/// would fire all 12. The instrument is the SMain step's `beep.ogg`
/// (one per dispatch).
#[test]
fn a_held_finger_dispatches_at_the_50ms_cadence_not_per_frame() {
    let (menu, audio, touch) = menu_with_touch(&["P1"]);
    boot_to_main(&menu);
    settle(&menu);
    assert_eq!(beep_count(&audio), 0);

    let (w, h) = menu.window_size();
    tap(&touch, w / 2.0, 5.0 * h / 6.0); // the down zone, held

    for _ in 0..12 {
        menu.update(1.5);
    }

    assert_eq!(
        beep_count(&audio),
        6,
        "300 ms held = 6 × 50 ms, not 12 calls"
    );
}

/// F35 — the edge: a press acts ONCE. A single-call tap dispatches
/// exactly one action; the release dispatches nothing; a fresh press
/// (after the debounce has decayed) dispatches exactly one more.
#[test]
fn a_press_dispatches_once() {
    let (menu, audio, touch) = menu_with_touch(&["P1"]);
    boot_to_main(&menu);
    settle(&menu);
    assert_eq!(beep_count(&audio), 0);

    let (w, h) = menu.window_size();
    tap(&touch, w / 2.0, 5.0 * h / 6.0);
    menu.update(1.5);
    assert_eq!(beep_count(&audio), 1, "the press's one dispatch");

    // The release: nothing to dispatch (no finger down).
    *touch.down.borrow_mut() = Vec::new();
    menu.update(1.5);
    assert_eq!(beep_count(&audio), 1);

    // A fresh press (the debounce long decayed) is a fresh dispatch.
    settle(&menu);
    tap(&touch, w / 2.0, 5.0 * h / 6.0);
    menu.update(1.5);
    assert_eq!(beep_count(&audio), 2);
}

/// F35 — the operator's "wonky" case: a finger jittering between two
/// zones frame-to-frame (the boundary-microdrift that made the input
/// feel "really wonky") re-dispatches at most once per 50 ms — the
/// C++'s per-frame grid would have alternated the two zones at the
/// frame rate. 12 calls of zone-oscillation (down / up, 300 ms) =
/// exactly 6 dispatches (one per 50 ms), not 12.
#[test]
fn a_jittering_finger_cannot_race_the_zones() {
    let (menu, audio, touch) = menu_with_touch(&["P1"]);
    boot_to_main(&menu);
    settle(&menu);
    assert_eq!(beep_count(&audio), 0);

    let (w, h) = menu.window_size();
    let down = Vec2::new(w / 2.0, 5.0 * h / 6.0);
    let up = Vec2::new(w / 2.0, h / 6.0);
    for i in 0..12 {
        *touch.down.borrow_mut() = vec![if i % 2 == 0 { down } else { up }];
        menu.update(1.5);
    }

    assert_eq!(
        beep_count(&audio),
        6,
        "one dispatch per 50 ms, not per call"
    );
}

/// F35 — the debounce decays at the C++'s decay position (the
/// `update()` head, MenuGame.cpp:2787-2790) — independent of the grid's
/// own `finger_down_count() == 1` gate: a held press arms 50 ms; the
/// two-finger calls (the grid's skip) keep the decay running; when the
/// second finger lifts, the single-finger zone re-fires exactly when
/// the decay completes (not sooner, and not per-frame).
#[test]
fn the_debounce_decays_while_the_grid_is_skipped() {
    let (menu, audio, touch) = menu_with_touch(&["P1"]);
    boot_to_main(&menu);
    settle(&menu);
    assert_eq!(beep_count(&audio), 0);

    let (w, h) = menu.window_size();
    let down = Vec2::new(w / 2.0, 5.0 * h / 6.0);
    let other = Vec2::new(w / 6.0, h / 2.0); // a second finger (left zone)

    // The held press: fire #1 (immediate), arm 50 ms.
    *touch.down.borrow_mut() = vec![down];
    menu.update(1.5);
    assert_eq!(beep_count(&audio), 1);

    // Two fingers for ONE call (the grid's skip — the decay keeps
    // running: 50 → 25 ms; the gate is still closed — the field's
    // own value proves it).
    *touch.down.borrow_mut() = vec![down, other];
    menu.update(1.5);
    assert_eq!(
        beep_count(&audio),
        1,
        "the two-finger call dispatches nothing"
    );
    assert_eq!(
        menu.touch_delay(),
        25.0,
        "the decay ran during the skip (50 → 25 ms)",
    );

    // The second finger lifts: the next call decays 25 → 0 — the gate
    // opens exactly at the decay's completion: fire #2.
    *touch.down.borrow_mut() = vec![down];
    menu.update(1.5);
    assert_eq!(
        beep_count(&audio),
        2,
        "the decay completed: the auto-repeat"
    );
    assert_eq!(
        menu.touch_delay(),
        50.0,
        "the dispatch re-armed the 50 ms debounce",
    );
}
