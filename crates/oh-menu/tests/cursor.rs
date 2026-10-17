//! 29u1: the menu's cursor-visibility toggles (the C++ `MenuGame::
//! setMouseCursorVisible`'s menu-state call sites) over the
//! [`FakeWindow`] (which records the `MenuWindow::set_mouse_cursor_visible`
//! the C++'s `window.setMouseCursorVisible(x)` applies,
//! MenuGame.cpp:3641-3644). The app-layer key-press-hide / mouse-move-show
//! (the C++'s other two handlers, MenuGame.cpp:278 / :295) are pinned in
//! `oh-app`'s `windowed` tests (the frame loop's menu-state detection).

use std::rc::Rc;

use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::NewGameCallback;
use oh_menu::{
    FakeAssets, FakeConfig, FakeWindow, MenuAssets, MenuConfig, MenuGame, NoMenuJoystick,
    NoopNetwork, NoopTouch, RecordingAudio,
};

fn menu_with_window(window: Rc<FakeWindow>) -> MenuGame {
    let assets: Rc<dyn MenuAssets> = {
        let fa = FakeAssets::new();
        fa.add_pack("cube", &["cube1", "cube2"]);
        fa.add_profile("P1");
        fa
    };
    let config: Rc<dyn MenuConfig> = FakeConfig::new();
    let cb: Option<NewGameCallback> = Some(Box::new(|_, _, _, _, _| {}));
    MenuGame::new(
        assets,
        config,
        Rc::new(NoopNetwork),
        RecordingAudio::new(),
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        window,
        cb,
        None,
        None,
        None,
        None, // `unlock_achievement` — the 29b2/29b8 Steam seam (None in the tests)
        Rc::new(LinearFontHeight),
    )
}

/// C++ `playSelectedLevel` (MenuGame.cpp:2373): the level start hides the
/// cursor (inside the `if(fnHGNewGame)` guard — a wired `new_game` seam).
#[test]
fn play_selected_level_hides_the_cursor() {
    let window = FakeWindow::new();
    // The C++ ctor's `setMouseCursorVisible(true)` (MenuGame.cpp:245) —
    // the `FakeWindow` starts visible.
    assert!(window.is_mouse_cursor_visible());
    let menu = menu_with_window(window.clone());
    menu.shared().borrow_mut().play_selected_level();
    assert!(
        !window.is_mouse_cursor_visible(),
        "the level start hides the cursor (C++ :2373)"
    );
}

/// The unwired-`new_game` guard (the C++ `if(fnHGNewGame)`): with no
/// `new_game` seam the `play_selected_level` early-returns BEFORE the
/// cursor hide, so the cursor stays visible (the C++ only hides when
/// `fnHGNewGame` is set).
#[test]
fn play_selected_level_without_a_new_game_seam_leaves_the_cursor_visible() {
    let window = FakeWindow::new();
    let assets: Rc<dyn MenuAssets> = {
        let fa = FakeAssets::new();
        fa.add_pack("cube", &["cube1"]);
        fa.add_profile("P1");
        fa
    };
    let menu = MenuGame::new(
        assets,
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        RecordingAudio::new(),
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        window.clone(),
        None, // no `new_game` seam — the C++'s null `fnHGNewGame`
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    menu.shared().borrow_mut().play_selected_level();
    assert!(
        window.is_mouse_cursor_visible(),
        "no `fnHGNewGame` → no hide (the C++'s `if(fnHGNewGame)` guard)"
    );
}

/// C++ `onEvent(MouseWheelScrolled)` (MenuGame.cpp:304-306): a wheel scroll
/// re-shows the cursor (after a level start hid it).
#[test]
fn mouse_wheel_shows_the_cursor() {
    let window = FakeWindow::new();
    let menu = menu_with_window(window.clone());
    menu.shared().borrow_mut().play_selected_level(); // hide (C++ :2373)
    assert!(!window.is_mouse_cursor_visible());
    menu.handle_mouse_wheel(1.0);
    assert!(
        window.is_mouse_cursor_visible(),
        "the wheel re-shows the cursor (C++ :304)"
    );
}
