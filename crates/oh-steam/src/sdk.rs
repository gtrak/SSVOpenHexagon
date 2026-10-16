//! The raw Steam SDK surface — the seam every [`crate::SteamManager`] logic
//! runs over (mirrors issue 19's `SteamUgc` pattern: the test double is
//! just the raw SDK surface, the manager carries the C++'s own logic —
//! log lines, state flags, dedup sets, the retry counter).
//!
//! Each method is exactly one C++ SDK call `steam_manager_impl` makes
//! (`.agents/skills/lib-steamworks.md` §1, rows 1-21), with the C++ line
//! citation in the doc. The manager's callback *bodies* (the C++
//! `STEAM_CALLBACK` handlers, Steam.cpp:150-174) are NOT part of this
//! seam: `run_callbacks` delivers the raw events, the manager applies them
//! (log lines + `_got_stats`), exactly as the C++'s `SteamAPI_RunCallbacks`
//! fires the registered handlers synchronously.

/// The SDK's `EResult` carried as its integer value (the C++ handlers log
/// the raw enum value; the name table, if ever needed, is issue 19's
/// `oh_workshop_uploader::steam::result_to_string` — the C++'s own table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EResult(pub i32);

/// One `UserStatsReceived_t` delivery (C++ `STEAM_CALLBACK(...,
/// on_user_stats_received, UserStatsReceived_t)`, Steam.cpp:105-106 → the
/// handler at :150-158 observes only `m_eResult`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsReceived {
    /// C++ `data->m_eResult`.
    pub result: EResult,
}

/// The raw Steam SDK surface everything `steam_manager_impl` calls.
///
/// Implementors: [`crate::NoSteam`] (the no-SDK default — the C++'s
/// `SSVOH_ANDROID` `#else` build, Steam.cpp:961-1062: `init` fails and
/// everything is false/nullopt/no-op) and, behind the `steam` feature,
/// [`crate::steamworks_sdk::SteamworksSdk`] (the real SDK).
pub trait SteamSdk: Send + Sync {
    /// C++ `SteamAPI_Init()` (Steam.cpp:58) — `true` iff the SDK
    /// initialized (the C++'s `initialize_steamworks` returns it verbatim).
    fn init(&self) -> bool;

    /// C++ `SteamAPI_Shutdown()` (Steam.cpp:83).
    fn shutdown(&self);

    /// C++ `SteamAPI_RunCallbacks()` (Steam.cpp:301). After this, the SDK's
    /// callback queue is drained; the delivered events are picked up with
    /// the `take_*` methods.
    fn run_callbacks(&self);

    /// The `UserStatsReceived_t` events delivered by the last
    /// `run_callbacks` (in SDK delivery order).
    fn take_stats_received(&self) -> Vec<StatsReceived>;

    /// The `UserStatsStored_t` event count delivered by the last
    /// `run_callbacks` (Steam.cpp:107; the C++ handler ignores the payload:
    /// `(void)data`).
    fn take_stats_stored(&self) -> usize;

    /// The `UserAchievementStored_t` event count (Steam.cpp:108;
    /// `(void)data`).
    fn take_achievement_stored(&self) -> usize;

    /// C++ `get_user_steam_id()` (Steam.cpp:38-52):
    /// `SteamAPI_SteamUser_v022()` + `SteamAPI_ISteamUser_BLoggedOn` +
    /// `SteamAPI_ISteamUser_GetSteamID`. `None` = not logged in (the
    /// SDK-side "Attempted to retrieve Steam ID when not logged in" log
    /// line lives here — the C++ helper logs it, Steam.cpp:45-47).
    fn user_steam_id(&self) -> Option<u64>;

    /// C++ `SteamUserStats()->RequestCurrentStats()` (Steam.cpp:283).
    fn request_current_stats(&self) -> bool;

    /// C++ `SteamUserStats()->StoreStats()` (Steam.cpp:319).
    fn store_stats(&self) -> bool;

    /// C++ `SteamUserStats()->SetAchievement(name.data())` (Steam.cpp:349).
    fn set_achievement(&self, name: &str) -> bool;

    /// C++ `SteamUserStats()->GetAchievement(name.data(), out)`
    /// (Steam.cpp:417) — `Some(unlocked)` on SDK success, `None` on
    /// failure (the C++ `bool` return + out-param, flattened).
    fn get_achievement(&self, name: &str) -> Option<bool>;

    /// C++ `SteamUserStats()->SetStat(name.data(), as_float)`
    /// (Steam.cpp:397 — the "try with float" half of the bugged-API
    /// fallback, Steam.cpp:394-395).
    fn set_stat_float(&self, name: &str, value: f32) -> bool;

    /// C++ `SteamUserStats()->SetStat(name.data, data)` (Steam.cpp:398 —
    /// the "try with integer" half).
    fn set_stat_int(&self, name: &str, value: i32) -> bool;

    /// C++ `SteamUserStats()->GetStat(name.data(), &as_float)`
    /// (Steam.cpp:437).
    fn get_stat_float(&self, name: &str) -> Option<f32>;

    /// C++ `SteamUserStats()->GetStat(name.data(), out)` (Steam.cpp:443).
    fn get_stat_int(&self, name: &str) -> Option<i32>;

    /// C++ `SteamFriends()->SetRichPresence(key, value)` (Steam.cpp:366,
    /// 378-383).
    fn set_rich_presence(&self, key: &str, value: &str) -> bool;

    /// C++ `SteamUGC()->GetNumSubscribedItems()` +
    /// `SteamUGC()->GetSubscribedItems(data, n)` (Steam.cpp:200-202; both
    /// with the default `bIncludeLocallyDisabled = false`), in SDK order.
    fn subscribed_items(&self) -> Vec<u64>;

    /// C++ `SteamUGC()->GetItemInstallInfo(id, &itemDiskSize, folderBuf,
    /// 512, &lastUpdateTimestamp)` (Steam.cpp:216-218) —
    /// `Some((itemDiskSize, folder, lastUpdateTimestamp))` when the item
    /// is installed, `None` otherwise.
    fn item_install_info(&self, id: u64) -> Option<(u64, String, u32)>;
}
