//! 29b8 — the four menu cosmetic/input gaps:
//! (a) `randomTip` populated at construction (C++ MenuGame.cpp:752-766, the
//! `ssvu::getRndI` draw on the thread-local `pcg32_fast`);
//! (b) the credits-bar's 3-texture cycle (C++ MenuGame.cpp:2848-2850 —
//! `getByModIdx(creditsIds, toInt(currentCreditsId / 100))`, the
//! `currentCreditsId += mFT` per-frame accumulation);
//! (c) the Shift modifier in `leftRightActionImpl` (C++ MenuGame.cpp:
//! 1860-1866 — LShift/RShift → the item's `increase(diff)` runs twice);
//! (d) the `a23_login` Steam unlock on `ELoginSuccess` (C++ MenuGame.cpp:
//! 2590-2593).

use std::cell::RefCell;
use std::rc::Rc;

use oh_menu::seams::{
    ConnectionState, FakeAssets, FakeConfig, FakeWindow, MenuAssets, NetEvent, NetworkClient,
    UnlockAchievementCallback,
};
use oh_menu::{MenuConfig, MenuGame, NoMenuJoystick, NoopNetwork, NoopTouch, States};

/// The `ELoginSuccess` drain test's (29b8d) network: queues the C++
/// `HexagonClient::Event`s that `MenuGame::update` polls
/// (`net.poll_event()` → `handle_net_event`).
#[derive(Default)]
struct QueueNetwork {
    events: std::sync::Mutex<Vec<NetEvent>>,
}

impl NetworkClient for QueueNetwork {
    fn state(&self) -> ConnectionState {
        ConnectionState::LoggedIn
    }
    fn has_rt_keys(&self) -> bool {
        false
    }
    fn connect(&self) {}
    fn disconnect(&self) {}
    fn try_login(&self, _: &str, _: &str) {}
    fn try_register(&self, _: &str, _: &str) {}
    fn try_logout(&self) {}
    fn try_delete_account(&self, _: &str) {}
    fn poll_event(&self) -> Option<NetEvent> {
        self.events.lock().expect("queue poisoned").pop()
    }
    fn update(&self) {}
}

type UnlockLog = Rc<RefCell<Vec<String>>>;

fn make_menu(unlocks: Option<UnlockLog>) -> (MenuGame, Option<Rc<QueueNetwork>>, Rc<FakeConfig>) {
    let fa = {
        let fa = FakeAssets::new();
        fa.add_pack("cube", &["c1", "c2", "c3"]);
        fa.add_profile("P1");
        fa
    };
    let assets: Rc<dyn MenuAssets> = fa;
    // The queueing network only when the unlock log is requested (the
    // 29b8d test) — the other tests run on the `NoopNetwork` (the C++'s
    // offline client state).
    let queue: Option<Rc<QueueNetwork>> =
        unlocks.is_some().then(|| Rc::new(QueueNetwork::default()));
    let network: Rc<dyn NetworkClient> = queue
        .clone()
        .map(|q| q as Rc<dyn NetworkClient>)
        .unwrap_or_else(|| Rc::new(NoopNetwork) as Rc<dyn NetworkClient>);

    let cfg = FakeConfig::new();

    let cb: Option<UnlockAchievementCallback> = unlocks.map(|log| -> UnlockAchievementCallback {
        Box::new(move |id: &str| log.borrow_mut().push(id.to_string()))
    });
    let menu = MenuGame::new(
        assets,
        cfg.clone(),
        network,
        oh_menu::RecordingAudio::new(),
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        None,
        None,
        None,
        cb,
        None,
        Rc::new(oh_menu::layout::LinearFontHeight),
    );
    // Boot through the two boot screens to the main menu.
    menu.advance_boot_screen();
    menu.advance_boot_screen();
    menu.ok_action();
    assert_eq!(menu.state(), States::SMain);
    (menu, queue, cfg)
}

#[test]
fn random_tip_is_populated_from_the_cpp_four_pairs() {
    // C++ ctor (MenuGame.cpp:752-766): `randomTip = tips[getRndI(0, 4)]`
    // — one of the four verbatim pairs, BOTH lines set (pre-fix the port
    // left `["", ""]` — the loading screen's tip rendered nothing).
    let (menu, _, _) = make_menu(None);
    let tip = menu.shared().borrow().random_tip.clone();
    assert!(!tip[0].is_empty() && !tip[1].is_empty());
    assert!(
        matches!(
            (tip[0].as_str(), tip[1].as_str()),
            (
                "HOLDING FOCUS WHILE CHANGING PACK",
                "SKIPS THE SWITCH ANIMATION"
            ) | ("REMEMBER TO TAKE BREAKS", "OPEN HEXAGON IS AN INTENSE GAME")
                | ("EXPERIMENT USING SWAP", "IT MAY SAVE YOUR LIFE")
                | (
                    "IF A LEVEL IS TOO CHALLENGING",
                    "PRACTICE IT AT A LOWER DIFFICULTY"
                )
        ),
        "got {:?} / {:?}",
        tip[0],
        tip[1]
    );
    // The draw is live (the thread-local engine advances — a second
    // construction draws again, the C++'s live-seed global semantics).
    let (menu2, _, _) = make_menu(None);
    assert!(!menu2.shared().borrow().random_tip[0].is_empty());
}

#[test]
fn credits_bar2_id_cycles_through_the_three_textures() {
    // C++ (MenuGame.cpp:2848-2850): `getByModIdx(creditsIds,
    // toInt(currentCreditsId / 100))` — the id changes every 100
    // accumulated seconds, wrapping 0 → 1 → 2 → 0 (the `getMod` wrap,
    // Math.hpp:292).
    let (menu, _, _) = make_menu(None);
    for (credits, expected) in [
        (0.0, 0),
        (99.999, 0),
        (100.0, 1),
        (199.999, 1),
        (200.0, 2),
        (299.999, 2),
        (300.0, 0),     // the 3-way wrap
        (1000.0, 1),    // 10 % 3
        (2000.0, 2),    // 20 % 3
        (3000.0, 0),    // 30 % 3
        (123456.78, 1), // 1234 % 3 (the toInt truncation)
    ] {
        let id = {
            let mut s = menu.shared().borrow_mut();
            s.current_credits_id = credits;
            s.credits_bar2_id()
        };
        assert_eq!(id, expected, "credits {credits} → {id} (want {expected})");
    }
    // The three ids are the C++'s three texture names (hpp:135-137).
    assert_eq!(
        oh_menu::menu_game::CREDITS_BAR_IDS,
        ["creditsBar2.png", "creditsBar2b.png", "creditsBar2c.png"]
    );
}

#[test]
fn update_accumulates_the_credits_id_and_cycles_the_frame() {
    // C++ `currentCreditsId += mFT` (MenuGame.cpp:2848) — the per-frame
    // accumulation at the `update()` tail.
    let (menu, _, _) = make_menu(None);
    assert_eq!(menu.shared().borrow().current_credits_id, 0.0);
    assert_eq!(menu.shared().borrow().credits_bar2_id(), 0);
    menu.update(100.0);
    assert_eq!(menu.shared().borrow().current_credits_id, 100.0);
    assert_eq!(menu.shared().borrow().credits_bar2_id(), 1);
    menu.update(100.0);
    assert_eq!(menu.shared().borrow().credits_bar2_id(), 2);
    menu.update(100.0);
    assert_eq!(
        menu.shared().borrow().credits_bar2_id(),
        0,
        "the 300-second wrap back to the first frame"
    );
}

#[test]
fn shift_modifiers_step_two_slider_values() {
    // C++ `leftRightActionImpl` (MenuGame.cpp:1857-1866): the modifier
    // (`LShift || RShift || focusHeld || wasFocusHeld`) runs the ITEM's
    // `increase(diff)` loop TWICE instead of once — NOT a menu-item move
    // (the C++ gates on `item.canIncrease()`, which only the
    // slider/toggle items set) — the "text padding" slider (0..64, step
    // 1) confirms the step count: right → +1, Shift+right → +2.
    let (menu, _, cfg) = make_menu(None);
    // SMain: LEVEL SELECT → OPTIONS (3 steps down) → MOpts.
    for _ in 0..3 {
        menu.down_action();
    }
    menu.ok_action();
    assert_eq!(menu.state(), States::MOpts);
    // MOpts: GAMEPLAY → GRAPHICS (3 steps down) → the graphics menu.
    for _ in 0..3 {
        menu.down_action();
    }
    menu.ok_action();
    // graphics: visual fxs → … → "text padding" (4 steps down).
    for _ in 0..4 {
        menu.down_action();
    }
    assert_eq!(
        menu.selected_item_name().as_deref(),
        Some("text padding: 0")
    );
    let base = cfg.text_padding();
    // No modifier: one step (+1).
    menu.right_action();
    assert_eq!(cfg.text_padding(), base + 1);
    // Shift held (the app-layer `set_shift_held` feed — the C++'s live
    // `isKeyPressed(LShift/RShift)` read): two steps (+2).
    menu.set_shift_held(true);
    menu.right_action();
    assert_eq!(cfg.text_padding(), base + 3);
    // The left twin: −2.
    menu.left_action();
    assert_eq!(cfg.text_padding(), base + 1);
}

#[test]
fn login_success_unlocks_a23_login_and_failure_does_not() {
    // C++ (MenuGame.cpp:2590-2593): the `ELoginSuccess` handler shows the
    // "LOGIN SUCCESS" dialog AND `steamManager.unlock_achievement(
    // "a23_login")` — the `ELoginFailure` handler does NOT.
    let log: UnlockLog = Rc::new(RefCell::new(Vec::new()));
    let (menu, queue, _) = make_menu(Some(log.clone()));
    let net = queue.expect("the queueing network");

    // A failure first: the dialog, no unlock.
    net.events
        .lock()
        .expect("queue poisoned")
        .push(NetEvent::LoginFailure("bad password".into()));
    menu.update(0.01);
    assert!(log.borrow().is_empty(), "ELoginFailure must not unlock");
    assert!(!menu.shared().borrow().dialog.empty());

    // The success: the unlock (after the dialog, the C++'s lambda order).
    menu.shared().borrow_mut().dialog.clear();
    net.events
        .lock()
        .expect("queue poisoned")
        .push(NetEvent::LoginSuccess);
    menu.update(0.01);
    assert_eq!(*log.borrow(), vec!["a23_login".to_string()]);
}
