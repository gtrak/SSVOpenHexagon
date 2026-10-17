//! Issue 029m row 29s1: the `scoreOverride` `readVariable` error path.
//!
//! The C++ reads the override DIRECTLY — `lua.readVariable<float>(
//! levelStatus.scoreOverride)` (HGUpdate.cpp:296-299; the timer-text
//! analogue in `updateText`, HGGraphics.cpp:542-544) — NOT through
//! `runLuaFunctionIfExists`, so a missing/`nil` variable throws
//! `VariableDoesntExistException` OUT of `update()`. The SSVStart timer
//! chain has no try/catch (`GameEngine::updateFromTimer` →
//! `GameState::update`), so the C++ terminates the process on a broken
//! level.
//!
//! The port keeps its SAFER documented deviation (deviations.md K5 —
//! matching the C++ would mean a deliberate crash on a level-authoring
//! bug): `host_error` (the Lippincott print + `go_to_menu(false, true)`
//! unless debug) + SKIP the custom-score update for this frame; the frame
//! continues. This test pins that behavior.

mod common;

use std::sync::Arc;

use oh_config::TIME_STEP;
use oh_game::seams::ScriptHost;

use common::{capture_go_to_menu, compose, release_dir};

const PACK: &str = "ohvrvanilla_vittorio_romeo_cube_1";
const LEVEL: &str = "ohvrvanilla_vittorio_romeo_cube_1_apeirogon";

#[test]
fn score_override_read_error_skips_and_menus_instead_of_crashing() {
    let assets = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    // Windowed: the C++ `goToMenu` head-gate (`window == nullptr → log +
    // return`, HexagonGame.cpp:1241-1247) skips the `fnGoToMenu` callback
    // headless, and the observable of this test is exactly that callback.
    let c = compose(assets, release_dir(), true);
    let seen = capture_go_to_menu(&c.game);

    c.game.new_game(PACK, LEVEL, true, 1.0, false);

    // Arm the broken-override state the C++ way: `l_overrideScore` sets
    // `scoreOverride`/`scoreOverridden`, then its spliced validation
    // snippet errors on a missing (non-number) variable — caught AT THE
    // `l_overrideScore` site (HGScripting.cpp:625-645: print +
    // `goToMenu(false, true)` unless debug).
    c.host
        .execute_code("l_overrideScore('nonexistent_score_var')")
        .expect("the level API call itself must not fail");
    assert_eq!(
        *seen.lock().expect("poisoned"),
        Some(true),
        "the l_overrideScore validation error takes the mError menu path"
    );

    // Now run the frame: C++ `lua.readVariable<float>` would throw out of
    // `update()` uncaught (process termination). The port must instead:
    // print + `go_to_menu(false, true)` + skip `updateCustomScore`, and
    // keep running.
    c.game.set_must_start(true);
    c.game.update(TIME_STEP, 1.0);

    assert!(
        !c.game.get_status().has_died,
        "the engine must survive the broken read"
    );
    assert_eq!(
        c.game.get_status().get_custom_score(),
        0.0_f32,
        "the broken read must not update the custom score"
    );
    assert_eq!(
        *seen.lock().expect("poisoned"),
        Some(true),
        "the update()-side read error takes the mError menu path (host_error \
         -> go_to_menu(false, true), debug off)"
    );
}

#[test]
fn l_override_score_validation_passes_for_a_number_variable() {
    // The C++-correct spliced validation snippet (HGScripting.cpp:630-634)
    // must ACCEPT a numeric global — no error, no menu — and the value
    // must then drive the custom score through the `update()` read.
    let assets = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    let c = compose(assets, release_dir(), true);
    let seen = capture_go_to_menu(&c.game);

    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    c.host
        .execute_code("myScore = 42.5")
        .expect("the seed assignment must run");
    c.host
        .execute_code("l_overrideScore('myScore')")
        .expect("the level API call itself must not fail");

    assert_eq!(
        *seen.lock().expect("poisoned"),
        None,
        "a numeric override must not take the l_overrideScore error path"
    );

    c.game.set_must_start(true);
    c.game.update(TIME_STEP, 1.0);

    assert_eq!(
        c.game.get_status().get_custom_score(),
        42.5_f32,
        "the numeric override must drive the custom score via the \
         update() readVariable"
    );
    assert_eq!(
        *seen.lock().expect("poisoned"),
        None,
        "no menu return on a healthy override"
    );
}
