//! The C++ `mainClient`'s Steam manager (main.cpp:251) — issue 020.
//!
//! The C++ constructs `hg::Steam::steam_manager steamManager;` at the
//! top of `mainClient` (before the folders, the config, the assets); the
//! construction includes `SteamAPI_Init` (the port's
//! `oh_steam::SteamManager::new` does the same). If initialized, the
//! C++ then requests the encrypted app ticket, the stats/achievements,
//! and pumps the callbacks once (main.cpp:255-258). The port holds the
//! same object as `Arc<oh_steam::SteamManager>` — shared with the engine
//! (`HexagonGame`'s `steam_manager` seam, main.cpp:415) and with the
//! windowed menu composition (the `MenuGame::init`-lifecycle calls,
//! MenuGame.cpp:793-794).
//!
//! Backend selection: the `steam` cargo feature links the real SDK
//! (`steamworks` 0.13.1 — the binding evaluation in
//! `.agents/skills/lib-steamworks.md`); without the feature (the
//! default — the gate's CI has no Steam at all, and a binary linked
//! against the loader stub cannot even start without the Steam client)
//! the manager runs on the `NoSteam` backend — the C++'s
//! `SSVOH_ANDROID` no-SDK branch (Steam.cpp:961-1062): init fails,
//! every method is a logged no-op, and the game is fully playable
//! (graceful degradation — the C++'s `steamAttempt`/`steamHung`
//! accounting, HexagonGame.hpp:90-92, lives in the manager in the port).

use std::sync::Arc;

use oh_steam::{SteamManager, SteamSdk};

/// The real-SDK backend (feature `steam`).
#[cfg(feature = "steam")]
fn backend() -> Box<dyn SteamSdk> {
    Box::new(oh_steam::SteamworksSdk::new())
}

/// The no-SDK backend (default): the C++'s `SSVOH_ANDROID` branch —
/// `SteamAPI_Init` has no counterpart, so init "fails" and every
/// manager method degrades exactly as the C++'s `_initialized ==
/// false` gates do.
#[cfg(not(feature = "steam"))]
fn backend() -> Box<dyn SteamSdk> {
    Box::new(oh_steam::NoSteam)
}

/// C++ `main.cpp:251` — `hg::Steam::steam_manager steamManager;`
/// (construct + `SteamAPI_Init`).
#[must_use]
pub fn steam_manager() -> Arc<SteamManager> {
    Arc::new(SteamManager::new(backend()))
}

// The C++'s `HGAssets{&steamManager, ...}` seam (`oh_assets::
// WorkshopPacks` for `SteamManager` — the impl's in `oh-steam`,
// `SteamManager`'s orphan-rule home) is used by the compositions'
// `Assets::new_with_workshop` calls (the workshop-pack discovery half —
// the live enumeration when initialized, the `workshopCache.json`
// fallback when not, `Assets::load_all_pack_datas`, Assets.cpp:750-764).
