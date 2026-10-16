//! The real-SDK backend (feature `steam`): `steamworks` 0.13.1 + ONE
//! runtime-resolved FFI call (the documented gap 1 in
//! `.agents/skills/lib-steamworks.md` §3.1: `RequestCurrentStats` is
//! missing from the wrapper AND the sys bindings — the newer SDK the sys
//! crate bundles deprecated it, and its loader stub no longer exports the
//! flat symbol, so a plain link fails. The repo's vendored SDK still
//! declares it — `public/steam/steam_api_flat.h:432` — so the port looks
//! the symbol up at runtime with a single hand-written `dlsym` binding
//! and calls it on `SteamAPI_SteamUserStats_v013()`; a loader without it
//! yields `false` (the newer SDK's "managed by the client" behavior)).

use std::collections::VecDeque;
use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use std::sync::{Arc, Mutex};

use steamworks::{Callback, CallbackHandle, Client};
use steamworks_sys as sys;

use crate::sdk::{EResult, StatsReceived, SteamSdk};

/// The raw `UserStatsReceived_t` payload (the open `Callback` trait,
/// `steamworks` callback.rs:148-151, lets a backend read the SDK's raw
/// `m_eResult` — the C++ handler logs that raw integer; the wrapper's
/// typed payload only carries non-OK results as `SteamError`).
#[derive(Clone, Copy)]
struct RawStatsReceived(sys::UserStatsReceived_t);

unsafe impl Callback for RawStatsReceived {
    const ID: i32 = sys::UserStatsReceived_t_k_iCallback as i32;

    unsafe fn from_raw(raw: *mut c_void) -> Self {
        // The C++ handler observes only `m_eResult` — copy the whole
        // (Copy) payload out of the SDK's buffer.
        Self(raw.cast::<sys::UserStatsReceived_t>().read_unaligned())
    }
}

/// The `UserStatsStored_t` delivery marker (Steam.cpp:107) — the C++
/// handler ignores the payload (`(void)data`), so the carrier is `()`;
/// only the delivery COUNT is observed.
#[derive(Debug, Clone, Copy)]
struct StatsStored;

unsafe impl Callback for StatsStored {
    const ID: i32 = sys::UserStatsStored_t_k_iCallback as i32;

    unsafe fn from_raw(_raw: *mut c_void) -> Self {
        Self
    }
}

/// The `UserAchievementStored_t` delivery marker (Steam.cpp:108) — same as
/// above (`(void)data`).
#[derive(Debug, Clone, Copy)]
struct AchievementStored;

unsafe impl Callback for AchievementStored {
    const ID: i32 = sys::UserAchievementStored_t_k_iCallback as i32;

    unsafe fn from_raw(_raw: *mut c_void) -> Self {
        Self
    }
}

/// The events delivered by the last `run_callbacks` (the registered
/// handlers push here; the manager drains them via the `take_*` seam
/// methods, exactly as the C++'s `SteamAPI_RunCallbacks` fires the member
/// callbacks synchronously).
#[derive(Debug, Default)]
struct Events {
    stats_received: VecDeque<EResult>,
    stats_stored: usize,
    achievement_stored: usize,
}

// Gap 1: `SteamAPI_ISteamUserStats_RequestCurrentStats` (vendored flat
// header, `public/steam/steam_api_flat.h:432`) — absent from the
// steamworks wrapper AND the sys bindings (skill §3.1): the newer SDK
// the `steamworks-sys` crate bundles has DEPRECATED the call, and its
// loader stub no longer exports the flat symbol — a plain link against
// it fails (`undefined symbol`). So the port looks the symbol up at
// runtime in the already-loaded loader (`dlsym(RTLD_DEFAULT, ...)` —
// the `steamworks` link pulls the loader into the process first):
// the OLD loaders (the repo's vendored SDK — what the C++ links) still
// export it → the real call; a loader without it → `false` (the newer
// SDK's "managed by the Steam client" behavior).
extern "C" {
    #[link_name = "dlsym"]
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

type RequestCurrentStatsFn = extern "C" fn(*mut sys::ISteamUserStats) -> bool;

/// C++ `SteamUserStats()->RequestCurrentStats()` (Steam.cpp:283) —
/// resolved from the already-loaded loader (skill §3.1 gap 1 fallback).
/// Returns `false` when the loader does not export the symbol (the newer
/// SDK's "managed by the Steam client" behavior).
fn request_current_stats() -> bool {
    // RTLD_DEFAULT = NULL (glibc): search the process's loaded libraries —
    // the `steamworks` link has already loaded the official loader stub.
    let name = CString::new("SteamAPI_ISteamUserStats_RequestCurrentStats").expect("no NUL");
    let ptr = unsafe { dlsym(std::ptr::null_mut(), name.as_ptr()) };
    if ptr.is_null() {
        return false;
    }
    // The dlsym result IS the function's address; a `*mut c_void` and an
    // `extern "C" fn` pointer share the representation, so transmute it.
    let f: RequestCurrentStatsFn = unsafe { std::mem::transmute(ptr) };
    let stats = unsafe { sys::SteamAPI_SteamUserStats_v013() };
    f(stats)
}

/// The real-SDK [`SteamSdk`] backend.
///
/// `init` = `steamworks::Client::init()` (the C++'s `SteamAPI_Init`;
/// failure — no Steam client / no appid — is a plain `false`, the runtime
/// half of the graceful degradation). `shutdown` drops the `Client` (whose
/// last `Client` drop runs `SteamAPI_Shutdown`, steamworks lib.rs:571 —
/// the C++'s explicit shutdown call, Steam.cpp:83).
pub struct SteamworksSdk {
    client: Mutex<Option<Client>>,
    events: Arc<Mutex<Events>>,
    /// Kept alive as long as the client (dropping a `CallbackHandle`
    /// unregisters the callback, steamworks callback.rs:157-174).
    handles: Mutex<Vec<CallbackHandle>>,
}

impl std::fmt::Debug for SteamworksSdk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SteamworksSdk")
            .field("initialized", &self.client.lock().unwrap().is_some())
            .finish()
    }
}

impl SteamworksSdk {
    /// Empty backend (uninitialized — `init` performs the `Client::init`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: Mutex::new(None),
            events: Arc::new(Mutex::new(Events::default())),
            handles: Mutex::new(Vec::new()),
        }
    }

    fn client(&self) -> Option<Client> {
        self.client.lock().unwrap().clone()
    }
}

impl Default for SteamworksSdk {
    fn default() -> Self {
        Self::new()
    }
}

impl SteamSdk for SteamworksSdk {
    fn init(&self) -> bool {
        let Ok(client) = Client::init() else {
            return false;
        };
        // The three `STEAM_CALLBACK`s (Steam.cpp:105-109): handlers that
        // only record the delivery into `events` — the manager applies the
        // C++'s handler bodies (log lines + `_got_stats`).
        let events = self.events.clone();
        let events1 = events.clone();
        let h1 = client.register_callback(move |e: RawStatsReceived| {
            events1
                .lock()
                .unwrap()
                .stats_received
                .push_back(EResult(e.0.m_eResult as i32));
        });
        let events2 = events.clone();
        let h2 = client.register_callback(move |_e: StatsStored| {
            events2.lock().unwrap().stats_stored += 1;
        });
        let h3 = client.register_callback(move |_e: AchievementStored| {
            events.lock().unwrap().achievement_stored += 1;
        });
        self.handles.lock().unwrap().extend([h1, h2, h3]);
        *self.client.lock().unwrap() = Some(client);
        true
    }

    fn shutdown(&self) {
        // Drop the `Client` (→ `SteamAPI_Shutdown`, steamworks lib.rs:571).
        *self.client.lock().unwrap() = None;
    }

    fn run_callbacks(&self) {
        // C++ `SteamAPI_RunCallbacks()` (Steam.cpp:301) — the manual
        // dispatch frame + callback loop (steamworks lib.rs:306,160-181);
        // the registered handlers fire synchronously into `events`.
        if let Some(client) = self.client() {
            client.run_callbacks();
        }
    }

    fn take_stats_received(&self) -> Vec<StatsReceived> {
        self.events
            .lock()
            .unwrap()
            .stats_received
            .drain(..)
            .map(|result| StatsReceived { result })
            .collect()
    }

    fn take_stats_stored(&self) -> usize {
        std::mem::take(&mut self.events.lock().unwrap().stats_stored)
    }

    fn take_achievement_stored(&self) -> usize {
        std::mem::take(&mut self.events.lock().unwrap().achievement_stored)
    }

    fn user_steam_id(&self) -> Option<u64> {
        // C++ `get_user_steam_id()` (Steam.cpp:38-52): the flat C API's
        // `BLoggedOn` + `GetSteamID` — the wrapper's `user().logged_on()` /
        // `user().steam_id()` wrap the same two calls.
        let client = self.client()?;
        if !client.user().logged_on() {
            println!("[Steam] Attempted to retrieve Steam ID when not logged in");
            return None;
        }
        Some(client.user().steam_id().raw())
    }

    fn request_current_stats(&self) -> bool {
        // C++ `SteamUserStats()->RequestCurrentStats()` (Steam.cpp:283) —
        // gap 1 fallback (module docs): runtime symbol lookup in the
        // loaded loader.
        if self.client().is_none() {
            return false;
        }
        request_current_stats()
    }

    fn store_stats(&self) -> bool {
        self.client()
            .map(|c| c.user_stats().store_stats().is_ok())
            .unwrap_or(false)
    }

    fn set_achievement(&self, name: &str) -> bool {
        self.client()
            .map(|c| c.user_stats().achievement(name).set().is_ok())
            .unwrap_or(false)
    }

    fn get_achievement(&self, name: &str) -> Option<bool> {
        self.client()?.user_stats().achievement(name).get().ok()
    }

    fn set_stat_float(&self, name: &str, value: f32) -> bool {
        self.client()
            .map(|c| c.user_stats().set_stat_f32(name, value).is_ok())
            .unwrap_or(false)
    }

    fn set_stat_int(&self, name: &str, value: i32) -> bool {
        self.client()
            .map(|c| c.user_stats().set_stat_i32(name, value).is_ok())
            .unwrap_or(false)
    }

    fn get_stat_float(&self, name: &str) -> Option<f32> {
        self.client()?.user_stats().get_stat_f32(name).ok()
    }

    fn get_stat_int(&self, name: &str) -> Option<i32> {
        self.client()?.user_stats().get_stat_i32(name).ok()
    }

    fn set_rich_presence(&self, key: &str, value: &str) -> bool {
        self.client()
            .map(|c| c.friends().set_rich_presence(key, Some(value)))
            .unwrap_or(false)
    }

    fn subscribed_items(&self) -> Vec<u64> {
        // C++ :200-202 — `GetNumSubscribedItems()` + `GetSubscribedItems`
        // (both default `bIncludeLocallyDisabled = false`).
        self.client()
            .map(|c| {
                c.ugc()
                    .subscribed_items(false)
                    .into_iter()
                    .map(|id| id.0)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn item_install_info(&self, id: u64) -> Option<(u64, String, u32)> {
        // C++ :216-218 — `GetItemInstallInfo`. The wrapper's folder buffer
        // is 4096 bytes vs the C++'s 512 (skill §3.3: documented
        // deviation, no observable difference for realistic folders).
        self.client()?
            .ugc()
            .item_install_info(steamworks::PublishedFileId(id))
            .map(|info| (info.size_on_disk, info.folder, info.timestamp))
    }
}
