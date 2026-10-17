//! 29a2 — the `newGame` restart/replay input-hint strings (the C++
//! `HexagonGame.cpp:758-821` `if(window != nullptr)` block, ported
//! VERBATIM): `getKeyboardBindNames(Tid::Restart/Replay)` trimmed to the
//! FIRST key (the `find(',')` + `erase` half), + `getJoystickBindName`
//! (the vendor-aware button name — MS `0x045E` / Sony `0x54C` / the
//! plain-number fallback) assembled into the "PRESS … TO RESTART" /
//! "OR JOYSTICK <btn>" / "PRESS JOYSTICK <btn> TO …" / "NO … BUTTON
//! SET" forms, each with the C++'s trailing `\n`. `update_text` already
//! reads the fields (update.rs:1415-1418, under the `has_died` gate), so
//! a windowed player who died now gets the C++'s hint; these strings
//! must match the C++ CHARACTER-FOR-CHARACTER.
//!
//! ONE `#[test]`: the scenarios share one windowed composition (each
//! `new_game` re-derives the hints from the CURRENT config + the seam's
//! vendor id — the C++'s live reads), plus one headless composition for
//! the `window != nullptr` gate.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

use oh_config::{Combo, Config, Jdir, Jid, Tid};
use oh_game::seams::{Joystick, NullReplayStore, PcgCosmeticRng};

mod common;

use common::{compose_with_config, release_dir, TestProfiles};

const PACK: &str = "ohvrvanilla_vittorio_romeo_cube_1";
const LEVEL: &str = "ohvrvanilla_vittorio_romeo_cube_1_apeirogon";

/// The `Joystick` seam with a configurable `vendor_id()` (the C++
/// `getJoystickBindName`'s `isConnected(0) ? vendorId : 0` read — the
/// seam's answer); every other query false (the machine is not involved
/// in the hint block).
#[derive(Clone)]
struct VendorJoystick {
    vendor: Arc<AtomicU32>,
}

impl Joystick for VendorJoystick {
    fn update(&self, _: f32) {}
    fn pressed_dir(&self, _: Jdir) -> bool {
        false
    }
    fn pressed_id(&self, _: Jid) -> bool {
        false
    }
    fn rising_edge_id(&self, _: Jid) -> bool {
        false
    }
    fn vendor_id(&self) -> u32 {
        self.vendor.load(Ordering::Relaxed)
    }
}

#[test]
fn new_game_builds_the_cpp_restart_replay_hints_verbatim() {
    let assets = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    let vendor = Arc::new(AtomicU32::new(0));
    let joystick: Arc<dyn Joystick> = Arc::new(VendorJoystick {
        vendor: vendor.clone(),
    });

    // Defaults: t_restart = [SPACE, ENTER, MMB], t_replay = [Y],
    // j_restart = 0, j_replay = 6 (config.rs:499-510).
    let config = Arc::new(RwLock::new(Config::default()));
    let c = compose_with_config(
        assets,
        release_dir(),
        true, /* window — the C++'s `window != nullptr` gate */
        config.clone(),
        None,
        Arc::new(TestProfiles),
        Arc::new(NullReplayStore),
        joystick,
        Arc::new(PcgCosmeticRng::new()),
    );

    // ------------------------------------------------------------------
    // A: vendor 0 (the C++'s `isConnected(0) == false` read —
    //    `getJoystickBindName`'s `ssvu::toStr(value)` plain-number
    //    fallback, Config.cpp:1603).
    // ------------------------------------------------------------------
    vendor.store(0, Ordering::Relaxed);
    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    let status = c.game.get_status();
    assert_eq!(
        status.restart_input, "PRESS SPACE OR JOYSTICK 0 TO RESTART\n",
        "t_restart 'SPACE, ENTER, MMB' trimmed to the first key + the \
         plain-number j_restart name (value 0, vendor 0)"
    );
    assert_eq!(
        status.replay_input, "PRESS Y OR JOYSTICK 6 TO REPLAY\n",
        "t_replay 'Y' (no comma to trim) + the plain-number j_replay name (6)"
    );

    // ------------------------------------------------------------------
    // B: the Microsoft vendor (0x045E) — the XBOX 360 button names
    //    (Config.cpp:1565-1572 column 0: 0 = "A", 6 = "BACK").
    // ------------------------------------------------------------------
    vendor.store(0x045E, Ordering::Relaxed);
    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    let status = c.game.get_status();
    assert_eq!(
        status.restart_input,
        "PRESS SPACE OR JOYSTICK A TO RESTART\n"
    );
    assert_eq!(status.replay_input, "PRESS Y OR JOYSTICK BACK TO REPLAY\n");

    // ------------------------------------------------------------------
    // C: the Sony vendor (0x54C) — column 1 (0 = "SQUARE", 6 = "L2").
    // ------------------------------------------------------------------
    vendor.store(0x54C, Ordering::Relaxed);
    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    let status = c.game.get_status();
    assert_eq!(
        status.restart_input,
        "PRESS SPACE OR JOYSTICK SQUARE TO RESTART\n"
    );
    assert_eq!(status.replay_input, "PRESS Y OR JOYSTICK L2 TO REPLAY\n");

    // ------------------------------------------------------------------
    // D: NO keyboard restart bind (the "PRESS JOYSTICK <btn>" variant) +
    //    the replay side still bound.
    // ------------------------------------------------------------------
    {
        let mut cfg = config.write().expect("config poisoned");
        *cfg.get_trigger(Tid::Restart).get_combos_mut() = vec![Combo::cleared()];
    }
    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    let status = c.game.get_status();
    assert_eq!(
        status.restart_input, "PRESS JOYSTICK SQUARE TO RESTART\n",
        "the empty keyboard name's variant (Sony vendor still 0x54C)"
    );
    assert_eq!(
        status.replay_input, "PRESS Y OR JOYSTICK L2 TO REPLAY\n",
        "the replay side is untouched"
    );

    // ------------------------------------------------------------------
    // E: NOTHING set on either side (the "NO … BUTTON SET" variants —
    //    `getJoystickBindName` returns "" for the value-33 unbound
    //    marker, Config.cpp:1574-1576).
    // ------------------------------------------------------------------
    {
        let mut cfg = config.write().expect("config poisoned");
        *cfg.get_trigger(Tid::Replay).get_combos_mut() = vec![Combo::cleared()];
        cfg.set_joystick_bind_value(Jid::Restart, 33);
        cfg.set_joystick_bind_value(Jid::Replay, 33);
    }
    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    let status = c.game.get_status();
    assert_eq!(status.restart_input, "NO RESTART BUTTON SET\n");
    assert_eq!(status.replay_input, "NO REPLAY BUTTON SET\n");

    // ------------------------------------------------------------------
    // F: the C++ `value >= 12` guard (MS/Sony vendors): a joystick bind
    //    at 12+ reads as UNBOUND (the "" name) → the "NO … BUTTON SET"
    //    variant even with the keyboard side bound… no — the keyboard
    //    side wins: "PRESS SPACE TO RESTART" (no joystick half).
    // ------------------------------------------------------------------
    {
        let mut cfg = config.write().expect("config poisoned");
        cfg.set_joystick_bind_value(Jid::Restart, 12);
        cfg.set_joystick_bind_value(Jid::Replay, 33);
        *cfg.get_trigger(Tid::Restart).get_combos_mut() = vec![{
            let mut c = Combo::new();
            c.add_key(oh_config::Key::SPACE);
            c
        }];
    }
    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    let status = c.game.get_status();
    assert_eq!(
        status.restart_input, "PRESS SPACE TO RESTART\n",
        "the vendor's value>=12 guard empties the joystick name — the \
         keyboard-only form (no ' OR JOYSTICK' half)"
    );
    assert_eq!(
        status.replay_input, "NO REPLAY BUTTON SET\n",
        "j_replay = 33 → the unbound marker's empty name"
    );

    // ------------------------------------------------------------------
    // G: HEADLESS (the C++ `window != nullptr` gate — a second
    //    composition, `window = false`): both strings stay EMPTY even
    //    with everything bound (the C++ headless tests' state).
    // ------------------------------------------------------------------
    vendor.store(0x045E, Ordering::Relaxed);
    {
        let mut cfg = config.write().expect("config poisoned");
        cfg.set_joystick_bind_value(Jid::Restart, 0);
        cfg.set_joystick_bind_value(Jid::Replay, 6);
        *cfg.get_trigger(Tid::Replay).get_combos_mut() = vec![{
            let mut c = Combo::new();
            c.add_key(oh_config::Key::Y);
            c
        }];
    }
    let assets2 = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    let vendor2 = Arc::new(AtomicU32::new(0x54C));
    let c_headless = compose_with_config(
        assets2,
        release_dir(),
        false, /* headless — the C++'s `window == nullptr` */
        config.clone(),
        None,
        Arc::new(TestProfiles),
        Arc::new(NullReplayStore),
        Arc::new(VendorJoystick { vendor: vendor2 }),
        Arc::new(PcgCosmeticRng::new()),
    );
    c_headless.game.new_game(PACK, LEVEL, true, 1.0, false);
    let status = c_headless.game.get_status();
    assert_eq!(
        status.restart_input, "",
        "headless: the C++'s `window == nullptr` gate leaves the hint empty"
    );
    assert_eq!(status.replay_input, "");
}
