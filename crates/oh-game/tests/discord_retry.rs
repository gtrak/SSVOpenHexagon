//! The Discord retry / hang — graceful degradation (issue 21). The
//! `discordAttempt` / `discordHung` logic is a `HexagonGame` member (NOT a
//! `discord_manager` one — `include/.../HexagonGame.hpp:89,91`,
//! `src/.../HexagonGame.cpp:495-506`), so it is pinned here, in `oh-game`,
//! with a fake [`oh_discord::Discord`]:
//!
//! * a `run_callbacks` that ALWAYS fails (Discord absent / hung) → after 20
//!   consecutive failures `discordHung` flips and `run_callbacks` is no
//!   longer called (the C++ "Too many failed callbacks. Stopping Discord
//!   callbacks." path) — the game keeps running (no panic);
//! * a `run_callbacks` that SUCCEEDS → called every frame, never hangs.
//!
//! (The `oh-discord` crate's tests pin the `discord_manager` surface behind
//! the `DiscordCore` seam; this file pins the game-loop retry that drives it.)

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use oh_config::Config;
use oh_discord::Discord;
use oh_game::hexagon_game::HexagonGame;
use oh_game::seams::{NoJoystick, NoScriptHost, NullProfiles, NullReplayStore, PcgCosmeticRng};

/// A `Discord` whose `run_callbacks` always fails (Discord absent / hung) —
/// records how many times the game loop actually called it.
struct FailingDiscord {
    run_callbacks_calls: Arc<AtomicUsize>,
}

impl Discord for FailingDiscord {
    fn run_callbacks(&self) -> bool {
        self.run_callbacks_calls.fetch_add(1, Ordering::SeqCst);
        false
    }
    fn set_rich_presence_in_menu(&self) -> bool {
        true
    }
    fn set_rich_presence_on_replay(&self) -> bool {
        true
    }
    fn set_rich_presence_in_game(
        &self,
        _level_info: &str,
        _second_info: &str,
        _dead: bool,
    ) -> bool {
        true
    }
}

/// A `Discord` whose `run_callbacks` always succeeds (a healthy link).
struct HealthyDiscord {
    run_callbacks_calls: Arc<AtomicUsize>,
}

impl Discord for HealthyDiscord {
    fn run_callbacks(&self) -> bool {
        self.run_callbacks_calls.fetch_add(1, Ordering::SeqCst);
        true
    }
    fn set_rich_presence_in_menu(&self) -> bool {
        true
    }
    fn set_rich_presence_on_replay(&self) -> bool {
        true
    }
    fn set_rich_presence_in_game(
        &self,
        _level_info: &str,
        _second_info: &str,
        _dead: bool,
    ) -> bool {
        true
    }
}

/// A minimal engine (no Lua / audio / window draw) wired to `discord`
/// (`None` = the C++ headless `discordManager == nullptr`) — enough to drive
/// `update_rich_presence_callbacks` (it only touches the `discord` seam + the
/// engine-local retry state, never the shared Lua state).
fn engine_with_discord(discord: Option<Arc<dyn Discord>>) -> HexagonGame {
    let assets = oh_assets::Assets::new(
        Arc::new(oh_vfs::RealFs::new(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../_RELEASE"),
        )),
        true,
        true,
    )
    .expect("_RELEASE tree");
    HexagonGame::new(
        Arc::new(oh_game::seams::NoneSteamSink), // steam (the retry test needs no Steam)
        Arc::new(RwLock::new(Config::default())),
        Arc::new(assets),
        None, // audio
        Arc::new(NullProfiles),
        Arc::new(NullReplayStore),
        Arc::new(NoScriptHost),
        Arc::new(NoJoystick),
        Arc::new(PcgCosmeticRng::new()),
        true, /* window */
        None, // renderer
        discord,
    )
}

#[test]
fn discord_retry_hangs_after_20_consecutive_failures() {
    let calls = Arc::new(AtomicUsize::new(0));
    let game = engine_with_discord(Some(Arc::new(FailingDiscord {
        run_callbacks_calls: calls.clone(),
    })));
    // Drive the per-frame callback well past the hang threshold (the C++ menu
    // drives it every frame; the in-game loop does too).
    for _ in 0..30 {
        game.update_rich_presence_callbacks();
    }
    // C++ (HexagonGame.cpp:497-505): `discordAttempt` starts at 1; each failed
    // `run_callbacks` increments it; on the 20th failure it becomes 21 (>20)
    // and `discordHung` flips. So `run_callbacks` is called exactly 20 times,
    // and the 21st..30th frames are no-ops (the game keeps running).
    assert_eq!(calls.load(Ordering::SeqCst), 20);
}

#[test]
fn discord_retry_healthy_link_never_hangs() {
    let calls = Arc::new(AtomicUsize::new(0));
    let game = engine_with_discord(Some(Arc::new(HealthyDiscord {
        run_callbacks_calls: calls.clone(),
    })));
    for _ in 0..30 {
        game.update_rich_presence_callbacks();
    }
    // The C++ only increments `discordAttempt` on a FAILURE — a healthy link
    // is called every frame and never hangs.
    assert_eq!(calls.load(Ordering::SeqCst), 30);
}

#[test]
fn discord_retry_absent_manager_is_inert() {
    // `discordManager == nullptr` (headless — `main.cpp:264-269`
    // `if(!headless)` is false): `update_rich_presence_callbacks` is a no-op
    // (the C++'s `discordManager != nullptr` gate, HexagonGame.cpp:495 — the
    // `let Some(discord) = ... else { return; }` short-circuits before any
    // core call). Driven 30×: no panic, no work.
    let game = engine_with_discord(None);
    for _ in 0..30 {
        game.update_rich_presence_callbacks();
    }
}
