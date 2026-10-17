//! Issue 029m row 29d1: the two `goToMenu()` call sites transcribed from
//! `HGUpdate.cpp` must pass the C++ DEFAULT arguments —
//! `goToMenu(bool mSendScores = true, bool mError = false)`
//! (`include/SSVOpenHexagon/Core/HexagonGame.hpp:417`) — i.e.
//! `go_to_menu(true, false)`:
//!
//! * the death/restart-prompt → menu path
//!   (`src/SSVOpenHexagon/Core/HGUpdate.cpp:399`, ported at
//!   `crates/oh-game/src/update.rs` — the `must_state_change` &&
//!   `!anyLocalProfileActive()` branch), and
//! * the joystick `Exit` rising edge
//!   (`src/SSVOpenHexagon/Core/HGUpdate.cpp:576`, ported in
//!   `update_input_update_joystick_controls`).
//!
//! The default `mSendScores = true` makes `goToMenu` run
//! `if(mSendScores && !mError) saveReplay();`
//! (`src/SSVOpenHexagon/Core/HexagonGame.cpp:1263-1265`), which submits the
//! local best and sends/saves the replay. Both call sites originally passed
//! `(false, false)`, dropping that — the port's own `trigger_go_to_menu`
//! (`hexagon_game.rs`) already passed `(true, false)`.
//!
//! # What the tests observe
//!
//! * **Joystick `Exit` (player alive, profile active)** — `save_replay()`
//!   passes its gate (`should_save_score() && !has_died`,
//!   `HexagonGame.cpp:547`) and reaches `replay_store.save(...)`: the
//!   counting store sees exactly ONE save after the menu return. This is the
//!   directly observable proof that the call site passes `mSendScores = true`
//!   (with the old `(false, false)` the store would see zero).
//! * **Death → restart prompt → menu (no local profile)** — the only state
//!   that reaches the `update.rs` `must_state_change` branch (it requires
//!   `!any_local_profile_active()`) is ALSO one where the C++
//!   `saveReplay()` gate is a no-op (`!status.hasDied` is false after death,
//!   AND `shouldSaveScore()` rejects with no local profile —
//!   `HexagonGame.cpp:547, 1179-1185`), so the C++ does NOT double-save the
//!   replay there (the replay was already saved by the death path,
//!   `HexagonGame.cpp:980-981`). The test asserts that parity: the store
//!   keeps exactly the death save, and the app-level `fn_go_to_menu` callback
//!   fired with `mError == false` (the C++ default-argument path, not the
//!   error path).
//! * **`mError` path** — the Lippincott handler calls `go_to_menu(false, true)`
//!   (`hexagon_game.rs` `host_error`); the score must NOT be sent there.

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use oh_config::{Jdir, Jid};
use oh_game::seams::{Joystick, NoJoystick, PcgCosmeticRng, Profiles, ReplayStore};
use oh_replay::CompressedReplayFile;

use common::{capture_go_to_menu, compose_with, release_dir, TestProfiles};

const PACK: &str = "ohvrvanilla_vittorio_romeo_cube_1";
const LEVEL: &str = "ohvrvanilla_vittorio_romeo_cube_1_apeirogon";

/// Counts `ReplayStore::save` calls (the C++ `crf.serialize_to_file`
/// half of `death_sendAndSaveReplay` — reached only via
/// `death()`'s own save and `save_replay()`, which the `go_to_menu`
/// `mSendScores && !mError` branch invokes).
#[derive(Debug, Default)]
struct CountingReplayStore {
    saves: Mutex<usize>,
}

impl ReplayStore for CountingReplayStore {
    fn save(&self, _: &str, _: &str, _: &CompressedReplayFile) -> bool {
        *self.saves.lock().expect("saves poisoned") += 1;
        true
    }
}

impl CountingReplayStore {
    #[must_use]
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn saves(&self) -> usize {
        *self.saves.lock().expect("saves poisoned")
    }
}

/// C++ no-local-profile state: `assets.anyLocalProfileActive() == false`
/// (the C++ `no_profile` fallback name, `HexagonGame.cpp:995-997`).
#[derive(Debug)]
struct NoLocalProfiles;

impl Profiles for NoLocalProfiles {
    fn any_local_profile_active(&self) -> bool {
        false
    }
    fn current_local_profile_name(&self) -> String {
        "no_profile".to_string()
    }
    fn get_local_score(&self, _level_id: &str) -> f32 {
        0.0
    }
    fn set_local_score(&self, _level_id: &str, _score: f32) {}
}

/// The C++ `Joystick::risingEdge(Jid::Exit)` edge, armed by the test
/// (`flag = true`) between frames; all other queries false, exactly like
/// `NoJoystick`.
#[derive(Debug)]
struct ExitRisingJoystick {
    flag: Arc<AtomicBool>,
}

impl ExitRisingJoystick {
    fn new() -> (Arc<Self>, Arc<AtomicBool>) {
        let flag = Arc::new(AtomicBool::new(false));
        (Arc::new(Self { flag: flag.clone() }), flag)
    }
}

impl Joystick for ExitRisingJoystick {
    fn update(&self, _: f32) {}
    fn pressed_dir(&self, _: Jdir) -> bool {
        false
    }
    fn pressed_id(&self, _: Jid) -> bool {
        false
    }
    fn rising_edge_id(&self, id: Jid) -> bool {
        matches!(id, Jid::Exit) && self.flag.load(Ordering::SeqCst)
    }
    fn vendor_id(&self) -> u32 {
        0
    }
}

#[test]
fn joystick_exit_go_to_menu_sends_the_score() {
    let assets = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    let store = CountingReplayStore::new();
    // Windowed: the C++ `goToMenu` head-gate (`window == nullptr → log +
    // return`, HexagonGame.cpp:1241-1247) makes the score branch a no-op
    // headless, and `updateInput_UpdateJoystickControls` is window-gated.
    let (joy, exit_flag) = ExitRisingJoystick::new();
    let c = compose_with(
        assets,
        release_dir(),
        true,
        Arc::new(TestProfiles),
        store.clone(),
        joy,
        Arc::new(PcgCosmeticRng::new()),
    );
    let seen = capture_go_to_menu(&c.game);

    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    c.game.set_must_start(true);
    c.game.update(oh_config::TIME_STEP, 1.0);
    assert!(
        c.game.get_status().started,
        "the game must have started before the Exit edge"
    );
    assert_eq!(
        store.saves(),
        0,
        "no replay save before the menu return (firstPlay = true)"
    );

    exit_flag.store(true, Ordering::SeqCst);
    c.game.update(oh_config::TIME_STEP, 1.0);

    // C++ `goToMenu()` default args (HexagonGame.hpp:417) →
    // `saveReplay()` (HexagonGame.cpp:1263-1265) → the gate
    // `shouldSaveScore() && !hasDied` passes (profile active, default
    // official config, not unscored, not in a replay, player alive) →
    // one store save.
    assert_eq!(
        store.saves(),
        1,
        "joystick-Exit go_to_menu must run save_replay (mSendScores = true)"
    );
    assert_eq!(
        *seen.lock().expect("poisoned"),
        Some(false),
        "the app-level callback must see mError = false"
    );
}

#[test]
fn death_restart_prompt_go_to_menu_matches_the_cpp() {
    let assets = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    let store = CountingReplayStore::new();
    // Windowed: the C++ `goToMenu` head-gate (`window == nullptr → log +
    // return`, HexagonGame.cpp:1241-1247) skips the score branch headless.
    let c = compose_with(
        assets,
        release_dir(),
        true,
        Arc::new(NoLocalProfiles),
        store.clone(),
        Arc::new(NoJoystick),
        Arc::new(PcgCosmeticRng::new()),
    );
    let seen = capture_go_to_menu(&c.game);

    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    c.game.set_must_start(true);
    c.game.update(oh_config::TIME_STEP, 1.0);

    c.game.perform_player_kill();
    assert_eq!(
        store.saves(),
        1,
        "the death path saves the replay once (C++ HexagonGame.cpp:980-981)"
    );

    // Decay `deathInputIgnore` (C++ HGUpdate.cpp:225-228; 10.f initial,
    // minus mFT per update) so the C++ `Tid::Restart` bind's
    // `deathInputIgnore <= 0.f && hasDied` gate (HexagonGame.cpp:389-397)
    // passes, then press it.
    for _ in 0..45 {
        c.game.update(oh_config::TIME_STEP, 1.0);
    }
    c.game.trigger_restart();
    assert_eq!(
        c.game.get_status().must_state_change,
        oh_game::StateChange::MustRestart,
        "the restart prompt must be armed"
    );

    c.game.update(oh_config::TIME_STEP, 1.0);

    // C++ `goToMenu()` (HGUpdate.cpp:399) → `saveReplay()` — but the gate
    // `shouldSaveScore() && !hasDied` (HexagonGame.cpp:547) is a NO-OP in
    // exactly this state: `hasDied` is true, and with no local profile
    // `shouldSaveScore()` rejects (HexagonGame.cpp:1179-1185). The C++
    // therefore does not save the replay a second time; the port must
    // match (and the menu return must not take the error path).
    assert_eq!(
        store.saves(),
        1,
        "the death save only — the C++ saveReplay() gate is a no-op after \
         death / without a local profile (no double save)"
    );
    assert_eq!(
        *seen.lock().expect("poisoned"),
        Some(false),
        "the app-level callback must see mError = false (the C++ \
         default-argument path, not the Lippincott error path)"
    );
}

#[test]
fn m_error_path_does_not_send_the_score() {
    let assets = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    let store = CountingReplayStore::new();
    // Windowed: the C++ `goToMenu` head-gate (`window == nullptr → log +
    // return`, HexagonGame.cpp:1241-1247) skips the score branch headless.
    let c = compose_with(
        assets,
        release_dir(),
        true,
        Arc::new(TestProfiles),
        store.clone(),
        Arc::new(NoJoystick),
        Arc::new(PcgCosmeticRng::new()),
    );
    let seen = capture_go_to_menu(&c.game);

    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    c.game.set_must_start(true);
    c.game.update(oh_config::TIME_STEP, 1.0);

    // The Lippincott handler's exact call (`hexagon_game.rs`
    // `host_error` → `go_to_menu(false, true)`; C++
    // `luaExceptionLippincottHandler` → `goToMenu(false, true)`): the
    // `mError` path must NOT send/save the score.
    c.game.go_to_menu(false, true);

    assert_eq!(store.saves(), 0, "the mError path must not run save_replay");
    assert_eq!(*seen.lock().expect("poisoned"), Some(true));
}
