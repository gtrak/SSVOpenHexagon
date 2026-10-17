//! Issue 20 — the Steam integration: the 1:1 port of `Core/Steam.cpp`'s
//! `steam_manager` (1062 lines, the C++ oracle) over the `steamworks`
//! binding.
//!
//! # Layout
//!
//! * [`SteamSdk`] — the raw Steam SDK seam (one method per C++ SDK call,
//!   with the Steam.cpp citation each).
//! * [`SteamManager`] — the `steam_manager_impl` port 1:1: init + the
//!   graceful-degradation gates (the game runs non-fatally without Steam —
//!   every method short-circuits on the failed `SteamAPI_Init`, exactly as
//!   the C++'s `_initialized` flag does), the callback pump,
//!   achievements/stats (incl. the float-first bugged-API fallbacks), the
//!   exact rich-presence strings, the workshop cache, the hung-callback
//!   counter (the C++'s `steamAttempt`/`steamHung`), and the
//!   encrypted-app-ticket surface (this tree's no-`SecretSteamKey.hpp`
//!   build — see the manager's module docs).
//! * [`NoSteam`] — the no-SDK backend (default build = the C++'s
//!   `SSVOH_ANDROID` no-SDK branch, Steam.cpp:961-1062).
//! * [`steamworks_sdk::SteamworksSdk`] (feature `steam`) — the real SDK
//!   backend.
//!
//! # The online-server seam
//!
//! [`SteamManager`] implements `oh_net::client::SteamManager` — the four
//! polling methods the C++ `HexagonClient` constructor drives for the
//! validated Steam ID (`HexagonClient.cpp:55-89`; the trait,
//! `oh-net/src/client.rs`).
//!
//! # Binding decision
//!
//! `steamworks` 0.13.1, evaluated from its checked-out source — see
//! `.agents/skills/lib-steamworks.md` for the full C++ API enumeration,
//! the C++→crate map, and the documented gaps + fallbacks.
//!
//! # MANUAL (verified only on a machine with the Steam client)
//!
//! With the `steam` feature, the behaviors that need a live client + the
//! app's `steam_appid.txt` (1358090, the repo's `_RELEASE/steam_appid.txt`):
//! the real `SteamAPI_Init` success path, the real callback deliveries
//! (stats received/stored, achievement stored), the real achievement/stat
//! round trip, the real rich-presence updates, the real workshop
//! enumeration, and the no-key ticket request against a live client.
//! The no-Steam paths (init failure → every gate false, the hung counter,
//! the no-key ticket surface) are what this crate's tests pin.

mod manager;
mod no_steam;
mod sdk;
#[cfg(feature = "steam")]
#[doc(hidden)]
mod steamworks_sdk;

pub use manager::SteamManager;
pub use no_steam::NoSteam;
pub use sdk::{EResult, StatsReceived, SteamSdk};
#[cfg(feature = "steam")]
pub use steamworks_sdk::SteamworksSdk;

/// The engine's Steam surface (`oh_game::seams::SteamSink` — the C++
/// `steam_manager*` the `HexagonGame` constructor takes first,
/// main.cpp:415, + the menu's `MenuGame::init`-lifecycle calls,
/// MenuGame.cpp:793-794): the manager IS the C++ object, so the
/// adaptation is a straight delegation (the manager methods' `bool`
/// return values are the C++'s — the sink drops them, exactly as the
/// C++ call sites do).
impl oh_game::seams::SteamSink for SteamManager {
    fn set_rich_presence_in_game(&self, level_name: &str, difficulty_mult: &str, time: &str) {
        let _ = self.set_rich_presence_in_game(level_name, difficulty_mult, time);
    }

    fn update_rich_presence_callbacks(&self) {
        SteamManager::update_rich_presence_callbacks(self);
    }

    fn unlock_achievement(&self, achievement_id: &str) {
        let _ = self.unlock_achievement(achievement_id);
    }

    fn set_rich_presence_in_menu(&self) {
        let _ = self.set_rich_presence_in_menu();
    }

    fn update_hardcoded_achievements(&self) {
        let _ = self.update_hardcoded_achievements();
    }
}

/// The encrypted-app-ticket seam for the online server: the C++
/// `steam_manager` as `HexagonClient` consumes it (`HexagonClient.cpp:55-89`
/// — the C++ holds the manager by reference and polls these four during
/// construction; the port implements the same surface on the shared
/// manager, `oh_net::client::SteamManager` — `oh-net/src/client.rs`).
/// Emscripten (issue 26c sub-goal 5B): OFF — the web vertical slice has no
/// online client (26c §5), and `oh-net` (the trait's home) is gated off
/// emscripten (its C deps do not link there on a Windows host — the
/// `Cargo.toml` note). The `SteamManager`'s own ticket methods (the
/// `manager.rs` surface) stay available; only this `oh-net` trait impl is
/// gone on web.
#[cfg(not(target_os = "emscripten"))]
impl oh_net::client::SteamManager for SteamManager {
    fn got_encrypted_app_ticket_response(&self) -> bool {
        SteamManager::got_encrypted_app_ticket_response(self)
    }

    fn run_callbacks(&mut self) {
        SteamManager::run_callbacks(self);
    }

    fn got_encrypted_app_ticket(&self) -> bool {
        SteamManager::got_encrypted_app_ticket(self)
    }

    fn get_ticket_steam_id(&self) -> Option<u64> {
        SteamManager::get_ticket_steam_id(self)
    }
}
