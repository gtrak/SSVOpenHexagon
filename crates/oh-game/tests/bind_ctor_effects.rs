//! 29a3 — proof that `HexagonGame::new` runs the C++ ctor's two config
//! side effects at the ctor point (the C++ `HexagonGame.cpp:345` +
//! `:448`):
//!
//! * `Config::keyboardBindsSanityCheck()` — the trigger REPAIR (the
//!   first 12 triggers agglomerated / trimmed / padded to `MAX_BINDS`;
//!   the data-layer semantics are pinned by
//!   `oh-config/tests/bind_sanity_check.rs`);
//! * `Config::loadAllJoystickBinds()` — the 12 `j_*` values feed the
//!   shared `oh_config::joystick` machine (29j1's poller then reads
//!   those slots each frame).
//!
//! The test builds a deliberately malformed config (a 5-combo
//! `t_restart` with scattered unbound slots + custom `j_restart` /
//! `j_replay` values that are NOT the defaults) and asserts both side
//! effects are visible on the very same `Arc<RwLock<Config>>` the
//! composition shares with the menu — exactly the C++'s one global
//! `Config` the ctor mutates.
//!
//! ONE `#[test]`: the `oh_config::joystick` machine is process-global;
//! the second scenario's synthetic poll would race a parallel test.

use std::sync::{Arc, RwLock};

use oh_config::joystick;
use oh_config::{Button, Combo, Config, Jid, JoystickPoll, Key, Tid, MAX_BINDS};
use oh_game::hexagon_game::HexagonGame;
use oh_game::lua_state::SharedState;
use oh_game::seams::{
    NoJoystick, NoScriptHost, NoneSteamSink, NullProfiles, NullReplayStore, PcgCosmeticRng,
};

fn release_dir() -> std::path::PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let root = std::path::Path::new(&manifest)
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root");
    let rel = root.join("_RELEASE");
    assert!(rel.is_dir(), "_RELEASE not found at {rel:?}");
    rel
}

fn key_combo(k: Key) -> Combo {
    let mut c = Combo::new();
    c.add_key(k);
    c
}

fn btn_combo(b: Button) -> Combo {
    let mut c = Combo::new();
    c.add_btn(b);
    c
}

#[test]
fn game_ctor_runs_the_cpp_keyboard_sanity_and_joystick_bind_load() {
    // The malformed side: `t_restart` gets 5 combos (out of the
    // MAX_BINDS=4 range) with unbound slots scattered between the bound
    // ones — the shape the C++'s `resizeTrigger` repair targets (REPLACE
    // the default's combos — extending would agglomerate the defaults in
    // too and blur the repair's observable).
    let mut cfg = Config::default();
    {
        let t = cfg.get_trigger(Tid::Restart);
        *t.get_combos_mut() = vec![
            key_combo(Key::A),
            Combo::cleared(),
            key_combo(Key::D),
            Combo::cleared(),
            btn_combo(Button::MIDDLE),
        ];
    }

    // The custom-bind side: j_restart / j_replay moved OFF their
    // defaults (0 / 6) onto buttons 2 / 3. Button 2 is the C++ DEFAULT
    // j_exit slot — so a machine that still carried the defaults would
    // read button 2 as `Jid::Exit`, not `Jid::Restart`: the assertion
    // below discriminates "the custom config was loaded" from "the
    // defaults were".
    cfg.set_joystick_bind_value(Jid::Restart, 2);
    cfg.set_joystick_bind_value(Jid::Replay, 3);

    let config = Arc::new(RwLock::new(cfg));

    // The ctor under test (the C++'s `HexagonGame` ctor — the
    // `keyboardBindsSanityCheck()` @HexagonGame.cpp:345 + the
    // `loadAllJoystickBinds()` @:448 run on the shared config, before
    // anything else reads a bind).
    let _game: HexagonGame<SharedState> = HexagonGame::new(
        Arc::new(NoneSteamSink),
        config.clone(),
        Arc::new(
            oh_assets::Assets::new(release_dir(), true, true).expect("_RELEASE loads headless"),
        ),
        None, // C++ `audio == nullptr`
        Arc::new(NullProfiles),
        Arc::new(NullReplayStore),
        Arc::new(NoScriptHost),
        Arc::new(NoJoystick),
        Arc::new(PcgCosmeticRng::new()),
        true, /* window: the C++ shipping composition; the ctor side
              effects themselves are NOT window-gated (only the
              per-frame joystick handlers are) */
        None,
        None,
    );

    // ------------------------------------------------------------------
    // Side effect 1: `keyboardBindsSanityCheck()` REPAIRED `t_restart`
    // on the shared config (the same `RwLock<Config>` the menu's
    // `get_keyboard_bind_names` / key-icon reads go through — the C++'s
    // one global).
    // ------------------------------------------------------------------
    {
        let cfg = config.read().expect("config poisoned");
        let restart = cfg.trigger_copy(Tid::Restart);
        let combos = restart.get_combos();
        assert_eq!(
            combos.len(),
            MAX_BINDS as usize,
            "the 5th combo was trimmed (the C++ resizeTrigger pop_back)"
        );
        assert!(
            !combos[0].is_unbound() && !combos[1].is_unbound() && !combos[2].is_unbound(),
            "the 3 bound combos agglomerated to the front"
        );
        assert!(combos[3].is_unbound(), "the tail is the cleared padding");
    }

    // ------------------------------------------------------------------
    // Side effect 2: `loadAllJoystickBinds()` fed the machine the
    // CUSTOM `j_*` values (not the defaults). Every SFML button 0-7 is
    // already a default `j_*` slot, so the custom values deliberately
    // COLLIDE with other defaults (the C++ has no uniqueness constraint
    // — two `Jid`s may share a button); the discriminator is the slot
    // the custom value LEFT behind: button 0 is the DEFAULT j_restart
    // slot — a machine still carrying the defaults would read button 0
    // as `Jid::Restart`, where the custom config (j_restart = 2) leaves
    // it unbound.
    // ------------------------------------------------------------------
    let deadzone = config
        .read()
        .expect("config poisoned")
        .get_joystick_deadzone();
    let mut poll = JoystickPoll::default();
    poll.buttons[0] = true; // the DEFAULT j_restart slot
    joystick::update(deadzone, &poll);
    assert!(
        !joystick::pressed(Jid::Restart),
        "button 0 must NOT read as j_restart (the custom config moved it to slot 2 — the defaults were not loaded)"
    );
    poll.buttons[0] = false;
    poll.buttons[2] = true; // the custom j_restart
    joystick::update(deadzone, &poll);
    assert!(
        joystick::pressed(Jid::Restart),
        "button 2 must read as j_restart (the custom value 2)"
    );
    poll.buttons[2] = false;
    poll.buttons[3] = true; // the custom j_replay
    joystick::update(deadzone, &poll);
    assert!(
        joystick::pressed(Jid::Replay),
        "button 3 must read as j_replay (the custom value 3)"
    );
    poll.buttons[3] = false;
    joystick::update(deadzone, &poll);
}
