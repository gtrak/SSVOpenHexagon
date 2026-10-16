//! The `steam_manager` port pinned against a scripted [`SteamSdk`] fake —
//! the issue-19 `SteamUgc` double pattern: the fake IS the raw SDK surface,
//! the manager carries the C++'s own logic (gates, log lines, dedup, the
//! float-first fallbacks, the hung counter). Every test asserts the C++'s
//! OBSERVABLE behavior (return values + the exact SDK call sequence), so
//! a mutation of the manager logic reddens it. The on-Steam behaviors
//! (real init success, real callback deliveries, real achievement/stat
//! round trips) are MANUAL — no Steam client in CI.
//!
//! The fake is `Clone` (clones share the call record + event queue via
//! `Arc`), so a test can hand a clone to the manager and observe the same
//! object.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use oh_steam::EResult;
#[cfg(feature = "steam")]
use oh_steam::SteamworksSdk;
use oh_steam::{NoSteam, StatsReceived, SteamManager, SteamSdk};

/// The SDK configuration (fixed per test).
#[derive(Debug, Clone, Default)]
struct Cfg {
    init_ok: bool,
    logged_in: bool,
    user_id: u64,
    request_current_stats_ok: bool,
    store_ok: bool,
    set_achievement_ok: bool,
    achievements: HashMap<String, bool>,
    get_achievement_ok: bool,
    stats_float: HashMap<String, f32>,
    stats_int: HashMap<String, i32>,
    set_stat_float_ok: bool,
    set_stat_int_ok: bool,
    rp_ok: bool,
    subscribed: Vec<u64>,
    install_info: HashMap<u64, (u64, String, u32)>,
}

/// The shared mutable half: the queued callback deliveries + the call
/// record (every `Fake` clone observes the same one).
#[derive(Debug, Default)]
struct Shared {
    stats_received: VecDeque<EResult>,
    stats_stored: usize,
    achievement_stored: usize,
    record: Record,
}

/// The scripted raw-SDK double (records every call in C++'s observable
/// order).
#[derive(Debug, Clone)]
struct Fake {
    cfg: Cfg,
    shared: Arc<Mutex<Shared>>,
}

#[derive(Debug, Clone, Default)]
struct Record {
    inits: usize,
    shutdowns: usize,
    run_callbacks: usize,
    user_steam_id: usize,
    request_current_stats: usize,
    store_stats: usize,
    set_achievement: Vec<String>,
    get_achievement: Vec<String>,
    set_stat_float: Vec<(String, f32)>,
    set_stat_int: Vec<(String, i32)>,
    get_stat_float: Vec<String>,
    get_stat_int: Vec<String>,
    rich_presence: Vec<(String, String)>,
    subscribed_items: usize,
    install_info: Vec<u64>,
}

impl Fake {
    fn new() -> Self {
        Self {
            cfg: Cfg::default(),
            shared: Arc::new(Mutex::new(Shared::default())),
        }
    }
    fn initialized(mut self) -> Self {
        self.cfg.init_ok = true;
        self.cfg.logged_in = true;
        self.cfg.user_id = 777;
        self
    }
    /// Queues a successful `UserStatsReceived` delivery (the C++'s
    /// `UserStatsReceived_t` callback fired with `m_eResult = k_EResultOK`).
    fn with_stats(self) -> Self {
        self.shared
            .lock()
            .unwrap()
            .stats_received
            .push_back(EResult(1));
        self
    }
    fn record(&self) -> Record {
        self.shared.lock().unwrap().record.clone()
    }
}

impl SteamSdk for Fake {
    fn init(&self) -> bool {
        self.shared.lock().unwrap().record.inits += 1;
        self.cfg.init_ok
    }
    fn shutdown(&self) {
        self.shared.lock().unwrap().record.shutdowns += 1;
    }
    fn run_callbacks(&self) {
        self.shared.lock().unwrap().record.run_callbacks += 1;
    }
    fn take_stats_received(&self) -> Vec<StatsReceived> {
        std::mem::take(&mut self.shared.lock().unwrap().stats_received)
            .into_iter()
            .map(|result| StatsReceived { result })
            .collect()
    }
    fn take_stats_stored(&self) -> usize {
        std::mem::take(&mut self.shared.lock().unwrap().stats_stored)
    }
    fn take_achievement_stored(&self) -> usize {
        std::mem::take(&mut self.shared.lock().unwrap().achievement_stored)
    }
    fn user_steam_id(&self) -> Option<u64> {
        self.shared.lock().unwrap().record.user_steam_id += 1;
        if self.cfg.logged_in {
            Some(self.cfg.user_id)
        } else {
            None
        }
    }
    fn request_current_stats(&self) -> bool {
        self.shared.lock().unwrap().record.request_current_stats += 1;
        self.cfg.request_current_stats_ok
    }
    fn store_stats(&self) -> bool {
        self.shared.lock().unwrap().record.store_stats += 1;
        self.cfg.store_ok
    }
    fn set_achievement(&self, name: &str) -> bool {
        self.shared
            .lock()
            .unwrap()
            .record
            .set_achievement
            .push(name.to_owned());
        self.cfg.set_achievement_ok
    }
    fn get_achievement(&self, name: &str) -> Option<bool> {
        self.shared
            .lock()
            .unwrap()
            .record
            .get_achievement
            .push(name.to_owned());
        if !self.cfg.get_achievement_ok {
            return None;
        }
        Some(self.cfg.achievements.get(name).copied().unwrap_or(false))
    }
    fn set_stat_float(&self, name: &str, value: f32) -> bool {
        self.shared
            .lock()
            .unwrap()
            .record
            .set_stat_float
            .push((name.to_owned(), value));
        self.cfg.set_stat_float_ok
    }
    fn set_stat_int(&self, name: &str, value: i32) -> bool {
        self.shared
            .lock()
            .unwrap()
            .record
            .set_stat_int
            .push((name.to_owned(), value));
        self.cfg.set_stat_int_ok
    }
    fn get_stat_float(&self, name: &str) -> Option<f32> {
        self.shared
            .lock()
            .unwrap()
            .record
            .get_stat_float
            .push(name.to_owned());
        self.cfg.stats_float.get(name).copied()
    }
    fn get_stat_int(&self, name: &str) -> Option<i32> {
        self.shared
            .lock()
            .unwrap()
            .record
            .get_stat_int
            .push(name.to_owned());
        self.cfg.stats_int.get(name).copied()
    }
    fn set_rich_presence(&self, key: &str, value: &str) -> bool {
        self.shared
            .lock()
            .unwrap()
            .record
            .rich_presence
            .push((key.to_owned(), value.to_owned()));
        self.cfg.rp_ok
    }
    fn subscribed_items(&self) -> Vec<u64> {
        self.shared.lock().unwrap().record.subscribed_items += 1;
        self.cfg.subscribed.clone()
    }
    fn item_install_info(&self, id: u64) -> Option<(u64, String, u32)> {
        self.shared.lock().unwrap().record.install_info.push(id);
        self.cfg
            .install_info
            .get(&id)
            .map(|(s, f, t)| (*s, f.clone(), *t))
    }
}

// ---------------------------------------------------------------------------
// Graceful degradation: no Steam, no panic, every method non-fatal
// ---------------------------------------------------------------------------

#[test]
fn no_steam_backend_is_the_cpp_android_branch() {
    // The C++'s `SSVOH_ANDROID` build (Steam.cpp:961-1062): every method
    // false / nullopt / no-op.
    let fake = Fake::new(); // init_ok = false
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(!m.is_initialized());
    assert!(!m.request_stats_and_achievements());
    assert!(!m.run_callbacks());
    assert!(!m.store_stats());
    assert!(!m.unlock_achievement("a0_babysteps"));
    assert!(!m.set_rich_presence_in_menu());
    assert!(!m.set_rich_presence_in_game("lvl", "2", "10.0"));
    assert!(!m.set_and_store_stat("s0_packprogress_cube", 1));
    assert_eq!(m.get_achievement("a0_babysteps"), None);
    assert_eq!(m.get_stat("s0_packprogress_cube"), None);
    assert!(!m.update_hardcoded_achievements());
    assert!(!m.request_encrypted_app_ticket());
    assert!(!m.got_encrypted_app_ticket_response());
    assert!(!m.got_encrypted_app_ticket());
    assert_eq!(m.get_ticket_steam_id(), None);
    let mut folders = Vec::new();
    m.for_workshop_pack_folders(|f| folders.push(f.to_owned()));
    assert!(folders.is_empty());
    // The C++'s destructor skips `SteamAPI_Shutdown` when uninitialized
    // (Steam.cpp:268-275).
    drop(m);
    let record = fake.record();
    assert_eq!(record.shutdowns, 0);
    assert_eq!(record.request_current_stats, 0);
    assert_eq!(record.rich_presence.len(), 0);
}

#[test]
fn no_steam_direct_is_non_fatal() {
    let m = SteamManager::new(Box::new(NoSteam));
    assert!(!m.is_initialized());
    assert!(!m.update_hardcoded_achievements());
    m.update_rich_presence_callbacks(); // no panic (the call is the assertion)
    assert!(!m.request_encrypted_app_ticket());
}

#[test]
fn init_failure_stays_non_fatal_and_never_shuts_down() {
    let fake = Fake::new(); // init fails
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(!m.is_initialized());
    // Every gate path returns false without touching the SDK further.
    assert!(!m.request_stats_and_achievements());
    assert!(!m.unlock_achievement("a0_babysteps"));
    drop(m);
    let record = fake.record();
    assert_eq!(record.inits, 1);
    assert_eq!(
        record.shutdowns, 0,
        "the C++'s destructor gates on _initialized"
    );
    assert_eq!(record.user_steam_id, 0, "the C++ logs 'Could not retrieve user Steam ID' without an SDK call — the helper is only reached on init success");
}

// ---------------------------------------------------------------------------
// Init success
// ---------------------------------------------------------------------------

#[test]
fn init_success_sets_flag_and_shuts_down_on_drop() {
    let fake = Fake::new().initialized();
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.is_initialized());
    assert_eq!(fake.record().user_steam_id, 1);
    drop(m);
    assert_eq!(fake.record().shutdowns, 1);
}

// ---------------------------------------------------------------------------
// Stats / achievements
// ---------------------------------------------------------------------------

#[test]
fn request_stats_flow() {
    let mut fake = Fake::new().initialized();
    fake.cfg.request_current_stats_ok = true;
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.request_stats_and_achievements());
    assert_eq!(fake.record().request_current_stats, 1);
}

#[test]
fn request_stats_failure_resets_got_stats() {
    // First manager: the request succeeds and the stats-received event
    // lands on a pump — `_got_stats` is true.
    let mut fake = Fake::new().initialized().with_stats();
    fake.cfg.request_current_stats_ok = true;
    fake.cfg.store_ok = true;
    fake.cfg.set_achievement_ok = true;
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.request_stats_and_achievements());
    assert!(m.run_callbacks()); // drains the stats-received event
    assert!(m.unlock_achievement("a0_babysteps")); // works: stats are in

    // Second manager (same SDK, fresh manager state): the request now
    // FAILS — the C++ resets `_got_stats` (Steam.cpp:284-286), so
    // achievement reads are gated off again.
    fake.cfg.request_current_stats_ok = false;
    let m2 = SteamManager::new(Box::new(fake.clone()));
    assert!(!m2.request_stats_and_achievements());
    assert_eq!(m2.get_achievement("a0_babysteps"), None, "_got_stats reset");
    assert!(!m2.unlock_achievement("a9_ratio"));
}

#[test]
fn stats_received_event_sets_got_stats_even_on_error_rc() {
    // C++ `on_user_stats_received`: `_got_stats = true` regardless of the
    // `m_eResult` it logs (Steam.cpp:150-158).
    let fake = {
        let mut f = Fake::new().initialized();
        f.shared
            .lock()
            .unwrap()
            .stats_received
            .push_back(EResult(2)); // k_EResultFail
        f.cfg.store_ok = true;
        f
    };
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.run_callbacks());
    assert!(m.store_stats());
    assert_eq!(fake.record().store_stats, 1);
}

#[test]
fn unlock_achievement_full_flow_and_dedup() {
    let fake = {
        let mut f = Fake::new().initialized().with_stats();
        f.cfg.set_achievement_ok = true;
        f.cfg.store_ok = true;
        f
    };
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.run_callbacks());
    assert!(m.unlock_achievement("a0_babysteps"));
    // The C++: `SetAchievement` then `store_stats` (Steam.cpp:349-356).
    assert_eq!(
        fake.record().set_achievement,
        vec!["a0_babysteps".to_owned()]
    );
    assert_eq!(fake.record().store_stats, 1);
    // Second unlock of the same name: the dedup set short-circuits BEFORE
    // the SDK (Steam.cpp:345-347) — no new SDK calls.
    assert!(!m.unlock_achievement("a0_babysteps"));
    assert_eq!(fake.record().set_achievement.len(), 1);
    assert_eq!(fake.record().store_stats, 1);
}

#[test]
fn unlock_achievement_requires_stats_and_propagates_sdk_failure() {
    // Without the stats-received event: false, no SDK call (Steam.cpp:337-340).
    let fake = {
        let mut f = Fake::new().initialized();
        f.cfg.set_achievement_ok = true;
        f
    };
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(!m.unlock_achievement("a0_babysteps"));
    assert!(fake.record().set_achievement.is_empty());

    // With stats, SDK failure: false AND the name is NOT added to the
    // dedup set (Steam.cpp:349-352: the insert happens after the
    // SetAchievement success).
    let fake2 = {
        let mut f = Fake::new().initialized().with_stats();
        f.cfg.set_achievement_ok = false;
        f
    };
    let m2 = SteamManager::new(Box::new(fake2.clone()));
    assert!(m2.run_callbacks());
    assert!(!m2.unlock_achievement("a0_babysteps"));
    assert_eq!(
        fake2.record().set_achievement,
        vec!["a0_babysteps".to_owned()]
    );
    assert!(m2.get_achievement("a0_babysteps").is_none());
    // A later retry hits the SDK again (no dedup entry was made).
    assert!(!m2.unlock_achievement("a0_babysteps"));
    assert_eq!(fake2.record().set_achievement.len(), 2);
}

#[test]
fn get_achievement_requires_stats_and_gates_on_sdk() {
    let fake = {
        let mut f = Fake::new().initialized().with_stats();
        f.cfg.get_achievement_ok = true;
        f.cfg.achievements.insert("a1_pointless".to_owned(), true);
        f
    };
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.run_callbacks()); // the stats-received event lands
    assert_eq!(m.get_achievement("a1_pointless"), Some(true));
    assert_eq!(m.get_achievement("a0_babysteps"), Some(false));
    assert_eq!(m.is_achievement_unlocked("a1_pointless"), Some(true));

    // No stats → None without an SDK call (Steam.cpp:413-416).
    let m2 = SteamManager::new(Box::new(Fake::new().initialized()));
    assert_eq!(m2.get_achievement("a1_pointless"), None);
}

// ---------------------------------------------------------------------------
// Rich presence (the strings are pinned EXACT — the game's localization keys)
// ---------------------------------------------------------------------------

#[test]
fn rich_presence_menu_exact_strings() {
    let fake = {
        let mut f = Fake::new().initialized();
        f.cfg.rp_ok = true;
        f
    };
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.set_rich_presence_in_menu());
    assert_eq!(
        fake.record().rich_presence,
        vec![("steam_display".to_owned(), "#InMenu".to_owned())]
    );
}

#[test]
fn rich_presence_game_exact_strings_and_order() {
    let fake = {
        let mut f = Fake::new().initialized();
        f.cfg.rp_ok = true;
        f
    };
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.set_rich_presence_in_game("Apeirogon", "2", "10.5"));
    assert_eq!(
        fake.record().rich_presence,
        vec![
            ("levelname".to_owned(), "Apeirogon".to_owned()),
            ("difficultymult".to_owned(), "2".to_owned()),
            ("time".to_owned(), "10.5".to_owned()),
            ("steam_display".to_owned(), "#InGame".to_owned()),
        ]
    );
}

#[test]
fn rich_presence_game_short_circuits_on_failure() {
    // The C++'s `&&` chain (Steam.cpp:378-383): a failing key stops it.
    let fake = Fake::new().initialized(); // rp_ok = false
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(!m.set_rich_presence_in_game("L", "1", "0.0"));
    assert_eq!(
        fake.record().rich_presence,
        vec![("levelname".to_owned(), "L".to_owned())],
        "only the first (failing) call was made"
    );
}

// ---------------------------------------------------------------------------
// Stats (the float-first bugged-API fallbacks, Steam.cpp:394-405, 434-447)
// ---------------------------------------------------------------------------

#[test]
fn set_stat_tries_float_then_int_then_stores() {
    let fake = {
        let mut f = Fake::new().initialized().with_stats();
        f.cfg.set_stat_float_ok = false;
        f.cfg.set_stat_int_ok = true;
        f.cfg.store_ok = true;
        f
    };
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.run_callbacks());
    assert!(m.set_and_store_stat("s0_packprogress_cube", 7));
    assert_eq!(
        fake.record().set_stat_float,
        vec![("s0_packprogress_cube".to_owned(), 7.0_f32)]
    );
    assert_eq!(
        fake.record().set_stat_int,
        vec![("s0_packprogress_cube".to_owned(), 7)]
    );
    assert_eq!(fake.record().store_stats, 1);
}

#[test]
fn set_stat_float_alone_succeeds_without_int() {
    let fake = {
        let mut f = Fake::new().initialized().with_stats();
        f.cfg.set_stat_float_ok = true;
        f.cfg.store_ok = true;
        f
    };
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.run_callbacks());
    assert!(m.set_and_store_stat("s", 3));
    assert!(fake.record().set_stat_int.is_empty());
}

#[test]
fn set_stat_both_fail_returns_false_without_store() {
    let fake = Fake::new().initialized().with_stats();
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.run_callbacks());
    assert!(!m.set_and_store_stat("s", 3));
    assert_eq!(fake.record().store_stats, 0);
}

#[test]
fn get_stat_prefers_float_and_truncates_like_the_cpp() {
    let fake = {
        let mut f = Fake::new().initialized().with_stats();
        f.cfg.stats_float.insert("s".to_owned(), 3.9_f32);
        f.cfg.stats_int.insert("s".to_owned(), 7);
        f
    };
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.run_callbacks());
    // C++: the float is read FIRST and narrowed (`*out = as_float`,
    // Steam.cpp:437-440) — 3.9 → 3, NOT the int's 7.
    assert_eq!(m.get_stat("s"), Some(3));
    assert_eq!(fake.record().get_stat_float, vec!["s".to_owned()]);
    assert!(fake.record().get_stat_int.is_empty());
}

#[test]
fn get_stat_falls_back_to_int() {
    let fake = {
        let mut f = Fake::new().initialized().with_stats();
        f.cfg.stats_int.insert("s".to_owned(), 7);
        f
    };
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.run_callbacks());
    assert_eq!(m.get_stat("s"), Some(7));
    assert_eq!(fake.record().get_stat_float, vec!["s".to_owned()]);
    assert_eq!(fake.record().get_stat_int, vec!["s".to_owned()]);
}

#[test]
fn get_stat_requires_stats_and_gates_both() {
    let m = SteamManager::new(Box::new(Fake::new().initialized()));
    assert_eq!(m.get_stat("s"), None, "no stats → no SDK calls");
}

// ---------------------------------------------------------------------------
// Hardcoded achievements (Steam.cpp:460-610)
// ---------------------------------------------------------------------------

#[test]
fn update_hardcoded_achievements_cubes_only_where_acc_exceeds_stat() {
    let mut fake = Fake::new().initialized().with_stats();
    // "Cube Master": 3 of the 10 member achievements unlocked, the stat is
    // 1 → the pack updates to 3. Every other pack: 0 unlocked, stat 0 →
    // `acc > stat` is false → no SDK writes.
    for name in ["a0_babysteps", "a1_pointless", "a2_flattering"] {
        fake.cfg.achievements.insert(name.to_owned(), true);
    }
    fake.cfg.get_achievement_ok = true;
    for stat in [
        "s0_packprogress_cube",
        "s1_packprogress_hypercube",
        "s2_packprogress_cubegod",
        "s3_packprogress_hypercubegod",
    ] {
        fake.cfg.stats_float.insert(stat.to_owned(), 1.0_f32);
    }
    fake.cfg.set_stat_float_ok = true;
    fake.cfg.store_ok = true;

    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.run_callbacks());
    assert!(m.update_hardcoded_achievements());

    let record = fake.record();
    // The four packs' `get_stat` (float-first) ... (4 reads)
    assert_eq!(record.get_stat_float.len(), 4);
    // ... and exactly ONE stat write (the cube pack, 3 > 1):
    assert_eq!(
        record.set_stat_float,
        vec![("s0_packprogress_cube".to_owned(), 3.0_f32)]
    );
    assert_eq!(record.store_stats, 1);
    // Every pack polled its member achievements (10 + 10 + 9 + 9 = 38):
    assert_eq!(record.get_achievement.len(), 38);
}

#[test]
fn update_hardcoded_achievements_all_four_packs_share_one_shape() {
    // Every pack's MEMBER SET is polled (10 + 10 + 9 + 9 = 38), even when
    // no pack exceeds its stat — the C++ "intentionally not
    // short-circuiting" (Steam.cpp:591-610).
    let mut fake = Fake::new().initialized().with_stats();
    fake.cfg.get_achievement_ok = true;
    fake.cfg
        .stats_float
        .insert("s0_packprogress_cube".to_owned(), 99.0_f32);
    fake.cfg
        .stats_float
        .insert("s1_packprogress_hypercube".to_owned(), 99.0_f32);
    fake.cfg
        .stats_float
        .insert("s2_packprogress_cubegod".to_owned(), 99.0_f32);
    fake.cfg
        .stats_float
        .insert("s3_packprogress_hypercubegod".to_owned(), 99.0_f32);
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(m.run_callbacks());
    assert!(m.update_hardcoded_achievements());
    let record = fake.record();
    assert_eq!(record.get_achievement.len(), 38);
    assert!(record.set_stat_float.is_empty());
}

#[test]
fn update_hardcoded_achievements_requires_stats() {
    let fake = Fake::new().initialized();
    let m = SteamManager::new(Box::new(fake.clone()));
    assert!(!m.update_hardcoded_achievements());
    let record = fake.record();
    assert!(record.get_stat_float.is_empty());
    assert!(record.get_achievement.is_empty());
}

// ---------------------------------------------------------------------------
// The encrypted-app-ticket surface (this tree's no-key build)
// ---------------------------------------------------------------------------

#[test]
fn ticket_no_key_build_pins_the_else_branch() {
    // Steam.cpp:688-690 (the active arm — the `SecretSteamKey.hpp` header
    // is absent from this repo): request → false, the response flag is
    // never set (no CCallResult is registered), the ticket stays None.
    let m = SteamManager::new(Box::new(Fake::new().initialized()));
    assert!(!m.request_encrypted_app_ticket());
    assert!(!m.got_encrypted_app_ticket_response());
    assert!(!m.got_encrypted_app_ticket());
    assert_eq!(m.get_ticket_steam_id(), None);

    // The uninitialized gate (Steam.cpp:676-679):
    let m2 = SteamManager::new(Box::new(Fake::new()));
    assert!(!m2.request_encrypted_app_ticket());
}

// ---------------------------------------------------------------------------
// The hung-callback counter (C++ steamAttempt/steamHung, HexagonGame.cpp:477-495)
// ---------------------------------------------------------------------------

#[test]
fn hung_counter_stops_pumping_after_twenty_failures() {
    // The C++: `steamAttempt{1}` (HexagonGame.hpp:92); each failed
    // `run_callbacks` bumps it; above 20 → `steamHung` + the log line, and
    // `run_callbacks` is never called again. On a no-Steam machine the
    // manager's `run_callbacks` fails (uninitialized — and, 1:1 with the
    // C++'s Steam.cpp:295-298, it returns false WITHOUT pumping the SDK),
    // so the counter latches after exactly 20 updates, and the SDK is
    // never pumped at all.
    let fake = Fake::new(); // init fails → run_callbacks always false
    let m = Arc::new(SteamManager::new(Box::new(fake.clone())));
    for _ in 0..19 {
        m.update_rich_presence_callbacks();
    }
    // attempt = 1 + 19 = 20: not yet latched (the C++ latches ABOVE 20).
    assert_eq!(m.hung_state(), (false, 20));
    m.update_rich_presence_callbacks();
    // attempt = 21 > 20: latched, the log line fired.
    assert_eq!(m.hung_state(), (true, 21));
    for _ in 0..10 {
        m.update_rich_presence_callbacks();
    }
    // Latched state stays; the SDK was never pumped (uninitialized).
    assert_eq!(m.hung_state(), (true, 21));
    assert_eq!(fake.record().run_callbacks, 0);
}

#[test]
fn hung_counter_does_not_latch_on_success() {
    let fake = Fake::new().initialized();
    let m = Arc::new(SteamManager::new(Box::new(fake.clone())));
    for _ in 0..30 {
        m.update_rich_presence_callbacks();
    }
    assert_eq!(fake.record().run_callbacks, 30);
}

// ---------------------------------------------------------------------------
// The online-server seam (oh_net::client::SteamManager)
// ---------------------------------------------------------------------------

#[test]
fn the_online_server_seam_is_the_same_manager() {
    let mut m = SteamManager::new(Box::new(NoSteam));
    let t: &mut dyn oh_net::client::SteamManager = &mut m;
    assert!(!t.got_encrypted_app_ticket_response());
    t.run_callbacks(); // no panic, no-op on the no-Steam manager
    assert!(!t.got_encrypted_app_ticket());
    assert_eq!(t.get_ticket_steam_id(), None);
}

#[test]
#[cfg(feature = "steam")]
fn steamworks_backend_is_a_valid_sdk() {
    // The real backend COMPILES into the seam and degrades on a machine
    // without the Steam client (init fails → the manager's gates).
    let m = SteamManager::new(Box::new(SteamworksSdk::new()));
    assert!(!m.is_initialized());
    assert!(!m.update_hardcoded_achievements());
}

#[test]
fn manager_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SteamManager>();
}
