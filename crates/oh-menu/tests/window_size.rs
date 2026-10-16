//! issue 025g — the menu's layout space follows the REAL window size.
//!
//! The C++ `MenuGame::refreshCamera()` (MenuGame.cpp:3129-3168) derives
//! `w`/`h` / `fourByThree` / the skew from `getWindowWidth()` /
//! `getWindowHeight()` (MenuGame.cpp:3350-3357 —
//! `window.getRenderWindow().getSize()`, the ACTUAL current render-target
//! size), and the 9-zone touch grid's thirds run on the same values
//! (MenuGame.cpp:2804-2807). The port used to feed `refresh_view` the
//! CONFIG size (`MenuConfig::size_x()` / `size_y()`) — invisible on the
//! desktop (window == config resolution × pixelMult; the math is
//! scale-invariant under a uniform factor) but wrong on the phone, where
//! the window is 1530×678 (the 1600×720 screen minus system bars) while
//! the config resolution is 1024×768-ish: the probe read the port's
//! menu space as `window=(1024, 1024)` while the touch coordinates live
//! in the 1530×678 space, so a center tap landed in the "right" band
//! instead of the center `ok` band (the 025g device finding).
//!
//! These tests pin the seam-wired behavior: `window_size()` reports the
//! `MenuWindow` seam (the actual window), `refresh_camera` runs
//! `layout::refresh_view` on it, and a window resize recomputes the
//! layout space (the C++ `window.onRecreation`'s `refreshCamera()` half).

use std::rc::Rc;

use oh_menu::layout::{refresh_view, LinearFontHeight};
use oh_menu::{
    FakeAssets, FakeConfig, FakeWindow, MenuGame, NoopNetwork, NoopTouch, RecordingAudio, States,
};

/// The phone fixture: the config resolution (1024×768-ish — the probe's
/// config space) vs the ACTUAL window 1530×678 (the 1600×720 screen
/// minus system bars).
fn phone_menu() -> (MenuGame, Rc<FakeWindow>) {
    let window = FakeWindow::new();
    window.set_size(1530.0, 678.0);
    let menu = MenuGame::new(
        FakeAssets::new(),
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        RecordingAudio::new(),
        Rc::new(NoopTouch),
        window.clone(),
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    (menu, window)
}

/// `window_size()` is the C++ `getWindowWidth()` / `getWindowHeight()`
/// port: the ACTUAL window size from the seam, not the config size and
/// not the derived `Shared::w`/`h` layout space.
#[test]
fn window_size_reports_the_real_window() {
    let (menu, window) = phone_menu();
    assert_eq!(menu.window_size(), (1530.0, 678.0));
    assert_eq!(window.size(), (1530.0, 678.0));
}

/// The constructor's `refresh_camera` (the C++ ctor tail) ran
/// `refresh_view` on the ACTUAL window — the config (800×450 default)
/// did not drive the layout space.
#[test]
fn refresh_camera_follows_the_real_window_not_the_config() {
    let (menu, _window) = phone_menu();
    let s = menu.shared().borrow();
    let (w, h, fbt, skew) = refresh_view(1530.0, 678.0);
    assert!((s.w - w).abs() < 1e-3);
    assert!((s.h - h).abs() < 1e-3);
    assert_eq!(s.four_by_three, fbt);
    assert_eq!(s.bg_skew_y, skew);
    // NOT the config-derived space the pre-025g port computed (the
    // config's 800×450 default would give a different `w`).
    let (config_w, _, _, _) = refresh_view(800.0, 450.0);
    assert!((s.w - config_w).abs() > 100.0);
}

/// The desktop case: window == config resolution — the computed `w`/`h`
/// are exactly the values the pre-025g config-fed path produced (the
/// desktop must not change).
#[test]
fn desktop_matching_window_unchanged() {
    let window = FakeWindow::new(); // 800×450 == FakeConfig's default
    let menu = MenuGame::new(
        FakeAssets::new(),
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        RecordingAudio::new(),
        Rc::new(NoopTouch),
        window.clone(),
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    let (w, h, fbt, skew) = refresh_view(800.0, 450.0);
    assert_eq!(menu.window_size(), (800.0, 450.0));
    let s = menu.shared().borrow();
    assert!((s.w - w).abs() < 1e-3);
    assert!((s.h - h).abs() < 1e-3);
    assert_eq!(s.four_by_three, fbt);
    assert_eq!(s.bg_skew_y, skew);
}

/// A window resize (the C++ `window.onRecreation`'s size half — the
/// `refreshCamera()` re-run) recomputes the layout space from the NEW
/// actual size; `refresh_camera` is the re-entry point.
#[test]
fn resize_recomputes_the_layout_space() {
    let (menu, window) = phone_menu();
    window.set_size(800.0, 600.0); // 4:3 — the config default would say 4:3-ish too
    menu.refresh_camera();
    let (w, h, fbt, skew) = refresh_view(800.0, 600.0);
    let s = menu.shared().borrow();
    assert!((s.w - w).abs() < 1e-3);
    assert!((s.h - h).abs() < 1e-3);
    assert_eq!(s.four_by_three, fbt);
    assert_eq!(s.bg_skew_y, skew);
    assert_eq!(menu.window_size(), (800.0, 600.0));
}

/// The 025g device finding, host-side: the phone's center tap — physical
/// screen center (800, 360) mapped to window coordinates (765, 339) —
/// must dispatch to the center `ok` zone. Pre-025g, the grid's thirds
/// ran on the config-derived space (1024×1024 for the phone's config:
/// wThird = 341.3, wLT = 682.7 → x = 765 fell in the RIGHT band — the
/// mis-dispatch the probe caught); with the actual-window space
/// (1530×678: the x-thirds (510, 1020), the y-middle (226, 452)) the
/// same tap is the `ok` band, and the entering-text state's `ok_action`
/// creates the profile.
#[test]
fn center_tap_in_the_real_window_space_lands_in_ok_zone() {
    let touch_down = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let menu = MenuGame::new(
        FakeAssets::new(),
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        RecordingAudio::new(),
        Rc::new(PhoneTouch {
            down: touch_down.clone(),
        }),
        {
            let window = FakeWindow::new();
            window.set_size(1530.0, 678.0);
            window
        },
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    // Fresh-install boot: LoadingScreen → EpilepsyWarning → ETLPNewBoot.
    menu.advance_boot_screen();
    menu.advance_boot_screen();
    assert_eq!(menu.state(), States::ETLPNewBoot);
    for c in "test".chars() {
        menu.handle_text_entered(c);
    }
    // The window-center tap (the phone's (800, 360) physical screen
    // center, in the 1530×678 window coordinates).
    *touch_down.borrow_mut() = vec![oh_math::ssvs::Vec2::new(765.0, 339.0)];
    menu.update(0.016);
    // The profile was created and the state advanced to the main menu
    // (the C++ `okAction` entering-text arm, MenuGame.cpp:2825-2829).
    assert_eq!(menu.state(), States::SMain);
}

/// The `PhoneTouch` test double (the touch_grid.rs `FingerTouch` shape,
/// local so this file stays self-contained).
struct PhoneTouch {
    down: std::rc::Rc<std::cell::RefCell<Vec<oh_math::ssvs::Vec2>>>,
}

impl oh_menu::seams::MenuTouch for PhoneTouch {
    fn finger_down_count(&self) -> usize {
        self.down.borrow().len()
    }
    fn finger_down_positions(&self) -> Vec<oh_math::ssvs::Vec2> {
        self.down.borrow().clone()
    }
}
