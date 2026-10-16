//! The `discord_manager` port, pinned against a fake `DiscordCore` (the
//! `oh-workshop-uploader` test-double pattern). These prove the C++ behavior
//! the game relies on WITHOUT a Discord client: the init success/failure
//! split, the exact rich-presence strings + timestamps, the `run_callbacks`
//! delegation, and the manager-level degradation (uninitialized → every
//! method inert).
//!
//! The game-loop retry (`discordAttempt` / `discordHung`) is tested in
//! `oh-game` (it is a `HexagonGame` member, not a `discord_manager` one).

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use oh_discord::{Activity, Discord, DiscordCore, DiscordManager, DISCORD_APP_ID, STEAM_APP_ID};

/// The fake core's recorded state (shared with the test via an `Arc`).
#[derive(Debug, Default)]
struct FakeState {
    init_result: bool,
    set_log_hook_calls: u32,
    register_command: Vec<String>,
    register_steam: Vec<u32>,
    run_callbacks_result: bool,
    run_callbacks_calls: u32,
    update_activity_result: bool,
    activities: Vec<Activity>,
}

#[derive(Clone)]
struct FakeCore {
    inner: Arc<Mutex<FakeState>>,
}

impl FakeCore {
    fn new(state: FakeState) -> (Self, Arc<Mutex<FakeState>>) {
        let inner = Arc::new(Mutex::new(state));
        (
            Self {
                inner: inner.clone(),
            },
            inner,
        )
    }
}

impl DiscordCore for FakeCore {
    fn init(&mut self) -> bool {
        self.inner.lock().expect("fake poisoned").init_result
    }
    fn set_log_hook(&mut self) {
        self.inner.lock().expect("fake poisoned").set_log_hook_calls += 1;
    }
    fn register_command(&mut self, command: &str) -> bool {
        self.inner
            .lock()
            .expect("fake poisoned")
            .register_command
            .push(command.to_string());
        true
    }
    fn register_steam(&mut self, steam_app_id: u32) -> bool {
        self.inner
            .lock()
            .expect("fake poisoned")
            .register_steam
            .push(steam_app_id);
        true
    }
    fn run_callbacks(&mut self) -> bool {
        let mut s = self.inner.lock().expect("fake poisoned");
        s.run_callbacks_calls += 1;
        s.run_callbacks_result
    }
    fn update_activity(&mut self, activity: &Activity) -> bool {
        let mut s = self.inner.lock().expect("fake poisoned");
        s.activities.push(activity.clone());
        s.update_activity_result
    }
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn initialized_manager() -> (DiscordManager<FakeCore>, Arc<Mutex<FakeState>>) {
    let (core, state) = FakeCore::new(FakeState {
        init_result: true,
        run_callbacks_result: true,
        update_activity_result: true,
        ..Default::default()
    });
    (DiscordManager::new(core), state)
}

#[test]
fn init_success_registers_command_and_steam_in_cxx_order() {
    let (manager, state) = initialized_manager();
    assert!(manager.is_initialized());
    let s = state.lock().expect("fake poisoned");
    // C++ ctor (Discord.cpp:42-66): log hook, then command, then Steam.
    assert_eq!(s.set_log_hook_calls, 1);
    assert_eq!(s.register_command, vec!["SSVOpenHexagon.exe".to_string()]);
    assert_eq!(s.register_steam, vec![STEAM_APP_ID]);
}

#[test]
fn init_failure_is_inert() {
    let (core, state) = FakeCore::new(FakeState {
        init_result: false,
        ..Default::default()
    });
    let manager = DiscordManager::new(core);
    // C++ `_initialized == false` (Discord.cpp:74-77, 90-93, 116-119,
    // 143-146) — every method returns `false` with no core call.
    assert!(!manager.is_initialized());
    assert!(!manager.run_callbacks());
    assert!(!manager.set_rich_presence_in_menu());
    assert!(!manager.set_rich_presence_on_replay());
    assert!(!manager.set_rich_presence_in_game("lvl", "pack", false));
    assert!(!manager.set_rich_presence_in_game("lvl", "pack", true));
    let s = state.lock().expect("fake poisoned");
    assert_eq!(s.set_log_hook_calls, 0);
    assert!(s.register_command.is_empty());
    assert!(s.register_steam.is_empty());
    assert_eq!(s.run_callbacks_calls, 0);
    assert!(s.activities.is_empty());
}

#[test]
fn set_rich_presence_in_menu_strings() {
    let (manager, state) = initialized_manager();
    let before = now_seconds();
    assert!(manager.set_rich_presence_in_menu());
    let after = now_seconds();
    let s = state.lock().expect("fake poisoned");
    assert_eq!(s.activities.len(), 1);
    let a = &s.activities[0];
    // C++ Discord.cpp:96-101: state "Selecting Level", details "", start now.
    assert_eq!(a.state, "Selecting Level");
    assert_eq!(a.details, "");
    assert!(
        (before..=after).contains(&a.start),
        "start {a:?} outside {before}..={after}"
    );
}

#[test]
fn set_rich_presence_on_replay_clears_timestamp() {
    let (manager, state) = initialized_manager();
    assert!(manager.set_rich_presence_on_replay());
    let s = state.lock().expect("fake poisoned");
    assert_eq!(s.activities.len(), 1);
    let a = &s.activities[0];
    // C++ Discord.cpp:122-127: state "Watching Replay", details "", start 0.
    assert_eq!(a.state, "Watching Replay");
    assert_eq!(a.details, "");
    assert_eq!(a.start, 0);
}

#[test]
fn set_rich_presence_in_game_alive_sets_start() {
    let (manager, state) = initialized_manager();
    let before = now_seconds();
    assert!(manager.set_rich_presence_in_game("Level [x2]", "Pack Name", false));
    let after = now_seconds();
    let s = state.lock().expect("fake poisoned");
    assert_eq!(s.activities.len(), 1);
    let a = &s.activities[0];
    // C++ Discord.cpp:152-165: details=level_info (1st line),
    // state=second_info (2nd line), start=now when !dead.
    assert_eq!(a.details, "Level [x2]");
    assert_eq!(a.state, "Pack Name");
    assert!((before..=after).contains(&a.start));
}

#[test]
fn set_rich_presence_in_game_dead_clears_start() {
    let (manager, state) = initialized_manager();
    assert!(manager.set_rich_presence_in_game("Level [x1]", "Survived 12.345s", true));
    let s = state.lock().expect("fake poisoned");
    assert_eq!(s.activities.len(), 1);
    let a = &s.activities[0];
    // C++ Discord.cpp:171: dead -> SetStart(0) (clears the timestamp).
    assert_eq!(a.details, "Level [x1]");
    assert_eq!(a.state, "Survived 12.345s");
    assert_eq!(a.start, 0);
}

#[test]
fn run_callbacks_delegates_to_core() {
    // A healthy core (init ok, run_callbacks ok) -> true, one core call.
    let (manager, state) = initialized_manager();
    assert!(manager.run_callbacks());
    assert_eq!(state.lock().expect("fake poisoned").run_callbacks_calls, 1);

    // A failing core -> false (the C++'s "Failed to run callbacks" path).
    let (core, state2) = FakeCore::new(FakeState {
        init_result: true,
        run_callbacks_result: false,
        ..Default::default()
    });
    let manager2 = DiscordManager::new(core);
    assert!(!manager2.run_callbacks());
    assert_eq!(state2.lock().expect("fake poisoned").run_callbacks_calls, 1);
}

#[test]
fn setter_returns_true_even_when_update_fails() {
    // The C++ `set_rich_presence_*` returns `true` after the init gate
    // regardless of the (async) UpdateActivity result (Discord.cpp:111, 137,
    // 182) — the "Fail" log is a separate side effect, not the return.
    let (core, state) = FakeCore::new(FakeState {
        init_result: true,
        update_activity_result: false,
        ..Default::default()
    });
    let manager = DiscordManager::new(core);
    assert!(manager.set_rich_presence_in_menu());
    assert!(manager.set_rich_presence_on_replay());
    assert!(manager.set_rich_presence_in_game("a", "b", false));
    // The activity was still dispatched to the core (the "Fail" logged).
    assert_eq!(state.lock().expect("fake poisoned").activities.len(), 3);
}

#[test]
fn manager_boxes_as_dyn_discord_and_is_send_sync() {
    // The engine holds `Option<Arc<dyn Discord>>` (the C++'s nullable
    // `discord_manager*`, main.cpp:416) — verify the trait object works.
    let (manager, _state) = initialized_manager();
    let boxed: Arc<dyn Discord> = Arc::new(manager);
    assert!(boxed.run_callbacks());
    assert!(boxed.set_rich_presence_in_menu());
    // A `fn` taking `Arc<dyn Discord>` forces the `Send + Sync` bound.
    fn assert_send_sync(_: &Arc<dyn Discord>) {}
    assert_send_sync(&boxed);
}

#[test]
fn discord_app_id_matches_cxx() {
    // Discord.cpp:23 — the Application ID the C++ `Core::Create` uses.
    assert_eq!(DISCORD_APP_ID, 725_763_266_110_029_964);
    // Discord.cpp:59 — the Steam App ID.
    assert_eq!(STEAM_APP_ID, 1_358_090);
}
