//! The `steam_manager` port — 1:1 over a [`crate::sdk::SteamSdk`] seam.
//!
//! The C++ `steam_manager` (include/SSVOpenHexagon/Core/Steam.hpp) is a
//! pimpl: the public class is pure delegation to `steam_manager_impl`
//! (Steam.cpp:830-1001). The port is a single struct — the impl's logic —
//! parameterized on the raw SDK seam (issue 19's `SteamUgc` pattern).
//!
//! # Graceful degradation (the game MUST run non-fatally without Steam)
//!
//! The C++'s degradation is the `_initialized` flag: when
//! `SteamAPI_Init()` fails (Steam.cpp:58-79: "Failed to initialize Steam
//! API"), EVERY method short-circuits to `false`/`nullopt`/no-op — no
//! panic, no error propagation. The port keeps that 1:1: `new` runs
//! `initialize_steamworks` (Steam.cpp:54-80) and records `_initialized`;
//! every public method gates on it.
//!
//! # The hung-callback counter (C++ `steamHung` / `steamAttempt`)
//!
//! The C++ keeps `steamAttempt{1}` / `steamHung{false}` on `HexagonGame`
//! (HexagonGame.hpp:90-92), driven by `HexagonGame::updateRichPresenceCallbacks`
//! (HexagonGame.cpp:477-495): every frame (the engine's update loop,
//! HGUpdate.cpp:155, AND the menu's `fnHGUpdateRichPresenceCallbacks`,
//! main.cpp:451-454) while `steamManager != nullptr && !steamHung`, a
//! failed `steamManager->run_callbacks()` bumps `steamAttempt`; above 20
//! it sets `steamHung = true` and logs "Too many failed callbacks.
//! Stopping Steam callbacks." — after which `run_callbacks` is never
//! called again. On a machine without Steam, `run_callbacks` fails
//! (uninitialized), so the C++ game logs that line once ~20-40 frames in
//! and stops pumping. The port moves the counter INTO the manager
//! (`update_rich_presence_callbacks`, below): behavior-identical, because
//! the counter only ever reacts to THIS manager's `run_callbacks` result
//! and both C++ call paths hit the same HexagonGame member — the port's
//! single manager object stands in for it.
//!
//! # The encrypted app ticket (the online-server seam)
//!
//! `request_encrypted_app_ticket` / `got_encrypted_app_ticket_response` /
//! `got_encrypted_app_ticket` / `get_ticket_steam_id` are the four methods
//! the C++ `HexagonClient` constructor polls (HexagonClient.cpp:55-89;
//! the Rust seam is `oh_net::client::SteamManager`, implemented on this
//! type in `lib.rs`). This tree compiles the C++'s `#else` (no
//! `SecretSteamKey.hpp` — the header is absent from the repo; see
//! `.agents/skills/lib-steamworks.md` §1): the request logs "Attempted to
//! request encrypted app ticket without secret key" and returns `false`,
//! the response flag is never set, the ticket stays `None`. Pinned 1:1.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::sdk::SteamSdk;

/// C++ `steam_manager_impl`'s state (the members at Steam.cpp:91-126) +
/// the C++ `HexagonGame`'s hung counter (module docs).
#[derive(Debug)]
struct ManagerState {
    /// C++ `_initialized` (Steam.cpp:91).
    initialized: bool,
    /// C++ `_got_stats` (Steam.cpp:92) — set by the
    /// `UserStatsReceived_t` callback (Steam.cpp:150-158), reset by a
    /// failed `RequestCurrentStats` (Steam.cpp:284-286).
    got_stats: bool,
    /// C++ `_got_ticket_response` (Steam.cpp:93).
    got_ticket_response: bool,
    /// C++ `_got_ticket` (Steam.cpp:94).
    got_ticket: bool,
    /// C++ `_ticket_steam_id` (Steam.cpp:95) — `CSteamID::ConvertToUint64`.
    ticket_steam_id: Option<u64>,
    /// C++ `_unlocked_achievements` (Steam.cpp:97).
    unlocked_achievements: HashSet<String>,
    /// C++ `_workshop_pack_folders` (Steam.cpp:98). The C++ is a
    /// `std::unordered_set<std::string>` (iteration order unspecified);
    /// the port keeps insertion order (deterministic; every C++ order is
    /// equally "correct" for an unordered_set).
    workshop_pack_folders: Vec<String>,
    /// C++ `HexagonGame::steamAttempt` (HexagonGame.hpp:92:
    /// `std::int8_t steamAttempt{1}`).
    steam_attempt: i8,
    /// C++ `HexagonGame::steamHung` (HexagonGame.hpp:90).
    steam_hung: bool,
}

/// The C++ `steam_manager` / `steam_manager_impl` port (module docs).
///
/// `Send + Sync` (the C++ object is single-threaded main-thread state; the
/// composition shares it as `Arc<SteamManager>` and the engine stores
/// `Arc<dyn SteamSink>` — interior `Mutex` keeps the sharing sound).
pub struct SteamManager {
    sdk: Box<dyn SteamSdk>,
    state: Mutex<ManagerState>,
}

// The C++'s four hardcoded-achievement packs (Steam.cpp:460-630):
// (the stat name, the member achievement names in source order).
const PACKS: [(&str, &[&str]); 4] = [
    // "Cube Master" (Steam.cpp:462-503).
    (
        "s0_packprogress_cube",
        &[
            "a0_babysteps",
            "a1_pointless",
            "a2_flattering",
            "a3_seconddim",
            "a4_apeirogon",
            "a5_commando",
            "a6_euclidian",
            "a7_pi",
            "a8_lab",
            "a9_ratio",
        ],
    ),
    // "Hypercube Master" (Steam.cpp:505-546).
    (
        "s1_packprogress_hypercube",
        &[
            "a11_evotutorial",
            "a12_disco",
            "a13_acceleradiant",
            "a14_gforce",
            "a15_incongruence",
            "a16_slither",
            "a17_polyhedrug",
            "a18_reppaws",
            "a19_centrifugalforce",
            "a20_massacre",
        ],
    ),
    // "Cube God" (Steam.cpp:548-589).
    (
        "s2_packprogress_cubegod",
        &[
            "a25_pointless_hard",
            "a26_flattering_hard",
            "a27_seconddim_hard",
            "a28_apeirogon_hard",
            "a29_commando_hard",
            "a30_euclidian_hard",
            "a31_pi_hard",
            "a32_lab_hard",
            "a33_ratio_hard",
        ],
    ),
    // "Hypercube Master" (the C++'s comment says "Hypercube Master" here
    // too — a copy-paste in the source, Steam.cpp:591; the stat is
    // s3_packprogress_hypercubegod, so it is the "Hypercube God" pack).
    (
        "s3_packprogress_hypercubegod",
        &[
            "a38_disco_hard",
            "a39_acceleradiant_hard",
            "a40_gforce_hard",
            "a41_incongruence_hard",
            "a42_slither_hard",
            "a43_polyhedrug_hard",
            "a44_reppaws_hard",
            "a45_centrifugalforce_hard",
            "a46_massacre_hard",
        ],
    ),
];

impl SteamManager {
    /// C++ `steam_manager()` (Steam.cpp:949-951) + the impl constructor
    /// (Steam.cpp:251-266): `_initialized{initialize_steamworks()}`, then
    /// `load_workshop_data()` iff initialized.
    pub fn new(sdk: Box<dyn SteamSdk>) -> Self {
        let this = Self {
            sdk,
            state: Mutex::new(ManagerState {
                initialized: false,
                got_stats: false,
                got_ticket_response: false,
                got_ticket: false,
                ticket_steam_id: None,
                unlocked_achievements: HashSet::new(),
                workshop_pack_folders: Vec::new(),
                steam_attempt: 1, // C++ `steamAttempt{1}` (HexagonGame.hpp:92)
                steam_hung: false,
            }),
        };
        let initialized = this.initialize_steamworks();
        if initialized {
            this.load_workshop_data();
        }
        this
    }

    /// C++ `initialize_steamworks()` (Steam.cpp:54-80).
    fn initialize_steamworks(&self) -> bool {
        println!("[Steam] Initializing Steam API");
        if self.sdk.init() {
            println!("[Steam] Steam API successfully initialized");
            match self.sdk.user_steam_id() {
                Some(id) => println!("[Steam] User Steam ID: '{id}'"),
                None => println!("[Steam] Could not retrieve user Steam ID"),
            }
            self.state.lock().unwrap().initialized = true;
            return true;
        }
        println!("[Steam] Failed to initialize Steam API");
        false
    }

    /// C++ `load_workshop_data()` (Steam.cpp:196-244).
    fn load_workshop_data(&self) {
        // C++ :200-202 — `GetNumSubscribedItems` + `GetSubscribedItems`.
        let items = self.sdk.subscribed_items();
        let mut st = self.state.lock().unwrap();

        // C++ `ssvuj::Obj cacheArray` (:209) — each installed item is
        // archived at `_workshop_pack_folders.size()` BEFORE the emplace
        // (:231-232): `ssvuj::arch` is JsonCpp `operator[]` (SSVUtilsJson/
        // Utils/Main.hpp:133-136) — APPEND when the index equals the size,
        // OVERWRITE when it is below (a duplicate folder keeps the set
        // deduped but the index no longer tracks the array size). The
        // port mirrors that exactly (verified against a C++-semantics
        // simulation over all small folder sequences).
        let mut cache: Vec<String> = Vec::new();
        for id in items {
            println!("[Steam] Workshop subscribed item id: {id}");

            // C++ :214-236 — `folderBuf[512]` + `GetItemInstallInfo`.
            let Some((item_disk_size, folder, _last_update_timestamp)) =
                self.sdk.item_install_info(id)
            else {
                continue;
            };
            println!(
                "[Steam] Workshop id {id} is installed, with size {item_disk_size} at folder {folder}"
            );

            let idx = st.workshop_pack_folders.len();
            if !st.workshop_pack_folders.contains(&folder) {
                st.workshop_pack_folders.push(folder.clone());
            }
            if cache.len() <= idx {
                cache.push(folder.clone());
            } else {
                cache[idx] = folder.clone();
            }
        }

        // C++ :238-244 — write the cache only when non-empty.
        if !cache.is_empty() {
            println!("[Steam] Updating workshop cache");
            // C++ `ssvuj::arch(cacheObj, "cachedPacks", cacheArray);
            // ssvuj::writeToFile(cacheObj, "workshopCache.json")` (:240-243)
            // — the `ssvuj::Writer` (JsonCpp built-in) is compact:
            // `{"cachedPacks":["<folder1>",...]}`.
            let obj = serde_json::json!({ "cachedPacks": cache });
            // The C++'s `std::ofstream` failure is ignored (no error path,
            // Steam.cpp:243); same here.
            let _ = std::fs::write("workshopCache.json", obj.to_string());
        }
    }

    /// C++ `steam_manager::is_initialized()` (Steam.cpp:953-955).
    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.state.lock().unwrap().initialized
    }

    /// C++ `request_stats_and_achievements()` (Steam.cpp:277-292).
    pub fn request_stats_and_achievements(&self) -> bool {
        {
            let st = self.state.lock().unwrap();
            if !st.initialized {
                println!("[Steam] Attempted to request stats when uninitialized");
                return false;
            }
        }
        if !self.sdk.request_current_stats() {
            println!("[Steam] Failed to get stats and achievements");
            self.state.lock().unwrap().got_stats = false;
            return false;
        }
        println!("[Steam] Successfully requested stats and achievements");
        true
    }

    /// C++ `run_callbacks()` (Steam.cpp:294-303) + the callback-handler
    /// bodies (Steam.cpp:150-174) applied as the SDK delivers the events.
    pub fn run_callbacks(&self) -> bool {
        if !self.state.lock().unwrap().initialized {
            return false;
        }
        self.sdk.run_callbacks();
        let mut st = self.state.lock().unwrap();
        // C++ `on_user_stats_received` (Steam.cpp:150-158): logs the raw
        // `m_eResult` and sets `_got_stats = true` (regardless of rc).
        for ev in self.sdk.take_stats_received() {
            println!("[Steam] Received user stats (rc: {})", ev.result.0);
            st.got_stats = true;
        }
        // C++ `on_user_stats_stored` (Steam.cpp:160-166).
        for _ in 0..self.sdk.take_stats_stored() {
            println!("[Steam] Stored user stats");
        }
        // C++ `on_user_achievement_stored` (Steam.cpp:168-174).
        for _ in 0..self.sdk.take_achievement_stored() {
            println!("[Steam] Stored user achievement");
        }
        true
    }

    /// C++ `store_stats()` (Steam.cpp:305-326).
    pub fn store_stats(&self) -> bool {
        {
            let st = self.state.lock().unwrap();
            if !st.initialized {
                println!("[Steam] Attempted to store stat when uninitialized");
                return false;
            }
            if !st.got_stats {
                println!("[Steam] Attempted to store stat without stats");
                return false;
            }
        }
        if !self.sdk.store_stats() {
            println!("[Steam] Failed to store stats");
            return false;
        }
        true
    }

    /// C++ `unlock_achievement(name)` (Steam.cpp:328-358).
    pub fn unlock_achievement(&self, name: &str) -> bool {
        {
            let st = self.state.lock().unwrap();
            if !st.initialized {
                println!("[Steam] Attempted to unlock achievement when uninitialized");
                return false;
            }
            if !st.got_stats {
                println!("[Steam] Attempted to unlock achievement without stats");
                return false;
            }
            if st.unlocked_achievements.contains(name) {
                return false;
            }
        }
        if !self.sdk.set_achievement(name) {
            println!("[Steam] Failed to unlock achievement {name}");
            return false;
        }
        self.state
            .lock()
            .unwrap()
            .unlocked_achievements
            .insert(name.to_owned());
        self.store_stats()
    }

    /// C++ `set_rich_presence_in_menu()` (Steam.cpp:362-367):
    /// `SetRichPresence("steam_display", "#InMenu")` — the string is the
    /// game's localization key (the `#` prefix = Steam's localized-string
    /// form, resolved against the store page's "InMenu" string).
    pub fn set_rich_presence_in_menu(&self) -> bool {
        if !self.state.lock().unwrap().initialized {
            return false;
        }
        self.sdk.set_rich_presence("steam_display", "#InMenu")
    }

    /// C++ `set_rich_presence_in_game(level_name_format,
    /// difficulty_mult_format, time_format)` (Steam.cpp:369-384): four
    /// `SetRichPresence` calls, `&&`-chained (short-circuit — a failing
    /// key stops the chain), keys `levelname` / `difficultymult` / `time`
    /// / `steam_display` with the `"#InGame"` string.
    pub fn set_rich_presence_in_game(
        &self,
        level_name_format: &str,
        difficulty_mult_format: &str,
        time_format: &str,
    ) -> bool {
        if !self.state.lock().unwrap().initialized {
            return false;
        }
        self.sdk.set_rich_presence("levelname", level_name_format)
            && self
                .sdk
                .set_rich_presence("difficultymult", difficulty_mult_format)
            && self.sdk.set_rich_presence("time", time_format)
            && self.sdk.set_rich_presence("steam_display", "#InGame")
    }

    /// C++ `set_and_store_stat(name, data)` (Steam.cpp:386-409): try the
    /// float overload first, then the int (Steam.cpp:394-398's bugged-API
    /// comment), then `store_stats`.
    pub fn set_and_store_stat(&self, name: &str, data: i32) -> bool {
        if !self.state.lock().unwrap().initialized {
            return false;
        }
        // C++ `const float as_float = data;` (Steam.cpp:396).
        let as_float = data as f32;
        if !self.sdk.set_stat_float(name, as_float) && !self.sdk.set_stat_int(name, data) {
            println!("[Steam] Error setting stat '{name}' to '{as_float}'");
            return false;
        }
        self.store_stats()
    }

    /// C++ `get_achievement(out, name)` (Steam.cpp:411-425) — the C++'s
    /// `bool` return + `bool* out` flattened to `Option<bool>`
    /// (`None` = the C++'s `false`).
    #[must_use]
    pub fn get_achievement(&self, name: &str) -> Option<bool> {
        {
            let st = self.state.lock().unwrap();
            if !st.initialized || !st.got_stats {
                return None;
            }
        }
        match self.sdk.get_achievement(name) {
            Some(unlocked) => Some(unlocked),
            None => {
                println!("[Steam] Error getting achievement {name}");
                None
            }
        }
    }

    /// C++ `is_achievement_unlocked(name)` (Steam.cpp:450-461).
    #[must_use]
    pub fn is_achievement_unlocked(&self, name: &str) -> Option<bool> {
        self.get_achievement(name)
    }

    /// C++ `get_stat(out, name)` (Steam.cpp:427-449) — float first
    /// (`*out = as_float`, the C++'s float→int narrowing), then int.
    #[must_use]
    pub fn get_stat(&self, name: &str) -> Option<i32> {
        {
            let st = self.state.lock().unwrap();
            if !st.initialized || !st.got_stats {
                return None;
            }
        }
        if let Some(as_float) = self.sdk.get_stat_float(name) {
            // C++ `*out = as_float;` (Steam.cpp:439) — in-range float→int
            // truncation (the C++ is UB out-of-range; Rust saturates).
            return Some(as_float as i32);
        }
        if let Some(v) = self.sdk.get_stat_int(name) {
            return Some(v);
        }
        println!("[Steam] Error getting stat {name}");
        None
    }

    /// C++ `update_hardcoded_achievement_<pack>()` (Steam.cpp:462-589,
    /// the four bodies sharing one shape).
    fn update_hardcoded_achievement_pack(&self, stat: &str, achievements: &[&str]) -> bool {
        let st = self.state.lock().unwrap();
        if !st.initialized || !st.got_stats {
            return false;
        }
        drop(st);
        // C++ `get_stat(&stat, "<stat name>")` (e.g. Steam.cpp:470-474).
        let Some(stat_value) = self.get_stat(stat) else {
            return false;
        };
        // C++ `acc` — the sum over `is_achievement_unlocked(name)
        // .value_or(false) ? 1 : 0` (e.g. Steam.cpp:476-486).
        let acc: i32 = achievements
            .iter()
            .map(|name| i32::from(self.is_achievement_unlocked(name).unwrap_or(false)))
            .sum();
        // C++ `if(acc > stat) { if(!set_and_store_stat(...)) return false; }`
        if acc > stat_value && !self.set_and_store_stat(stat, acc) {
            return false;
        }
        true
    }

    /// C++ `update_hardcoded_achievements()` (Steam.cpp:591-610) —
    /// "Intentionally not short-circuiting via boolean operators here":
    /// all four packs are evaluated; success iff zero failures.
    pub fn update_hardcoded_achievements(&self) -> bool {
        let failures: i32 = PACKS
            .iter()
            .map(|(stat, achievements)| {
                i32::from(!self.update_hardcoded_achievement_pack(stat, achievements))
            })
            .sum();
        failures == 0
    }

    /// C++ `for_workshop_pack_folders(f)` (Steam.cpp:612-624): invokes
    /// `f` for each folder iff initialized (the C++ iterates the
    /// `unordered_set`; the port iterates in insertion order — module
    /// state docs).
    pub fn for_workshop_pack_folders(&self, mut f: impl FnMut(&str)) {
        if !self.state.lock().unwrap().initialized {
            return;
        }
        for folder in self.state.lock().unwrap().workshop_pack_folders.clone() {
            f(&folder);
        }
    }

    /// C++ `request_encrypted_app_ticket()` — THIS TREE'S BUILD
    /// (the `#else` arm, Steam.cpp:688-690: the `SecretSteamKey.hpp`
    /// header is absent from the repo, so the `#if` arm with
    /// `SteamUser()->RequestEncryptedAppTicket` + the CCallResult
    /// registration, Steam.cpp:680-686, is compiled out). The
    /// uninitialized half (Steam.cpp:676-679) is active in both arms.
    pub fn request_encrypted_app_ticket(&self) -> bool {
        if !self.state.lock().unwrap().initialized {
            println!("[Steam] Attempted to request encrypted app ticket when uninitialized");
            return false;
        }
        println!("[Steam] Attempted to request encrypted app ticket without secret key");
        false
    }

    /// C++ `got_encrypted_app_ticket_response()` (Steam.cpp:841-844).
    /// In this tree's build the flag is never set (no request is ever
    /// made — see `request_encrypted_app_ticket`), so this is always
    /// `false`.
    #[must_use]
    pub fn got_encrypted_app_ticket_response(&self) -> bool {
        self.state.lock().unwrap().got_ticket_response
    }

    /// C++ `got_encrypted_app_ticket()` (Steam.cpp:846-849).
    #[must_use]
    pub fn got_encrypted_app_ticket(&self) -> bool {
        self.state.lock().unwrap().got_ticket
    }

    /// C++ `get_ticket_steam_id()` (Steam.cpp:851-855):
    /// `_ticket_steam_id->ConvertToUint64()` (the C++ dereferences the
    /// `nullopt` optional — UB; the port returns it as `Option`, same
    /// value on every path the C++ caller reaches).
    #[must_use]
    pub fn get_ticket_steam_id(&self) -> Option<u64> {
        self.state.lock().unwrap().ticket_steam_id
    }

    /// The C++ `HexagonGame::updateRichPresenceCallbacks()` STEAM half
    /// (HexagonGame.cpp:477-495 — the `steamManager != nullptr &&
    /// !steamHung` gate + the `steamAttempt`/`steamHung` accounting; the
    /// counter lives here, module docs). The caller applies the C++'s
    /// `steamManager != nullptr` gate (the engine's `Option`); the Discord
    /// half (HexagonGame.cpp:497-510) is issue 21.
    pub fn update_rich_presence_callbacks(&self) {
        {
            let st = self.state.lock().unwrap();
            if st.steam_hung {
                return;
            }
        }
        // C++ `if(!steamManager->run_callbacks())` (HexagonGame.cpp:482-490).
        if !self.run_callbacks() {
            let mut st = self.state.lock().unwrap();
            st.steam_attempt += 1;
            if st.steam_attempt > 20 {
                st.steam_hung = true;
                println!("[Steam] Too many failed callbacks. Stopping Steam callbacks.");
            }
        }
    }

    /// C++ `HexagonGame::steamAttempt` / `steamHung` (HexagonGame.hpp:90-92)
    /// exposed for observability — the C++ keeps them private engine state
    /// (no getter); the port needs the value observable to pin the
    /// latching behavior without a live SDK.
    #[must_use]
    pub fn hung_state(&self) -> (bool, i8) {
        let st = self.state.lock().unwrap();
        (st.steam_hung, st.steam_attempt)
    }
}

/// The C++'s `HGAssets{&steamManager, ...}` argument (main.cpp:211, :380)
/// — the Assets' `mSteamManager` pointer's port seam
/// (`oh_assets::WorkshopPacks`): the workshop-pack discovery half
/// (`Assets::load_all_pack_datas`'s workshop branch, Assets.cpp:750-764)
/// — the live enumeration when the manager initialized, the
/// `workshopCache.json` fallback when it did not. (The impl lives in
/// `oh-steam` — the orphan-rule home: `SteamManager` is this crate's
/// type, `WorkshopPacks`'s is `oh-assets`'s.)
impl oh_assets::WorkshopPacks for SteamManager {
    fn is_initialized(&self) -> bool {
        self.is_initialized()
    }
    fn workshop_pack_folders(&self) -> Vec<String> {
        // C++ `for_workshop_pack_folders(f)` — the port's `Vec` mirror of
        // the C++'s per-folder callback (the load-order-preserving storage
        // above; the C++'s `unordered_set` order is unspecified).
        let mut folders = Vec::new();
        self.for_workshop_pack_folders(|f| folders.push(f.to_string()));
        folders
    }
}

impl Drop for SteamManager {
    /// C++ `~steam_manager_impl()` (Steam.cpp:268-275):
    /// `if(_initialized) shutdown_steamworks()` (Steam.cpp:81-86: the log
    /// lines around `SteamAPI_Shutdown()`).
    fn drop(&mut self) {
        if self.state.lock().unwrap().initialized {
            println!("[Steam] Shutting down Steam API");
            self.sdk.shutdown();
            println!("[Steam] Shut down Steam API");
        }
    }
}
