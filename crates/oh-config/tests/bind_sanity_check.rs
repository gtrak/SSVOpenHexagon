//! 29a3 — `Config::keyboard_binds_sanity_check()`'s REPAIR semantics
//! (C++ `Config.cpp:1392-1406` → `resizeTrigger` @1363-1390): the sanity
//! check does not validate — it FIXES malformed trigger shapes:
//!
//! * **agglomerate** — unbound combos are removed (the C++'s
//!   `isUnbound()` erase loop) so the bound ones sit at the front;
//! * **trim** — more than `MAX_BINDS` (4) combos → the tail is popped
//!   (the C++'s `pop_back` — the LAST bind is dropped, not the first);
//! * **pad** — fewer than `MAX_BINDS` → cleared combos fill the tail
//!   (the C++'s `Combo({KKey::Unknown})` = the port's `Combo::cleared()`).
//!
//! The scope is exactly the first 12 triggers (RotateCCW..Down, the
//! C++'s `triggerRotateCCW()..triggerDown()` list) — NextPack /
//! PreviousPack / LuaConsole / Pause are NOT repaired (a malformed
//! `t_next` survives the check verbatim).
//!
//! This file pins the data-layer behavior; that the `HexagonGame` ctor
//! actually RUNS it (with `load_all_joystick_binds`) is pinned by
//! `oh-game/tests/bind_ctor_effects.rs` (29a3's engine-level proof —
//! the C++ ctor's HexagonGame.cpp:345 + :448).

use oh_config::{resize_trigger, Button, Combo, Config, Key, Tid, MAX_BINDS};

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

/// The C++ `resizeTrigger` on one trigger (Config.cpp:1363-1390):
/// 5 combos (3 bound + 2 unbound, scattered) → 4 combos, the bound ones
/// agglomerated at the front, the tail cleared.
#[test]
fn resize_trigger_agglomerates_trims_and_pads() {
    // Scattered: bound / unbound / bound / unbound / bound.
    let mut combos = vec![
        key_combo(Key::A),
        Combo::cleared(),
        key_combo(Key::D),
        Combo::cleared(),
        btn_combo(Button::MIDDLE),
    ];
    resize_trigger_from_combos(&mut combos);
    assert_eq!(combos.len(), MAX_BINDS as usize, "padded to MAX_BINDS");
    assert!(
        !combos[0].is_unbound(),
        "the first bound combo agglomerates to 0"
    );
    assert!(!combos[1].is_unbound());
    assert!(!combos[2].is_unbound());
    assert!(
        combos[3].is_unbound(),
        "the padding slot is a cleared combo"
    );
}

/// All 5 combos bound: the overflow is trimmed from the TAIL (the C++'s
/// `pop_back` — the 5th bind is the one dropped).
#[test]
fn resize_trigger_trims_overflow_from_the_tail() {
    let mut combos = vec![
        key_combo(Key::A),
        key_combo(Key::B),
        key_combo(Key::C),
        key_combo(Key::D),
        key_combo(Key::E),
    ];
    resize_trigger_from_combos(&mut combos);
    assert_eq!(combos.len(), MAX_BINDS as usize, "trimmed to MAX_BINDS");
    assert!(
        combos[0].get_keys()[Key::A.0 as usize + 1],
        "the first bind survives"
    );
    assert!(
        !combos[3].get_keys()[Key::E.0 as usize + 1],
        "the 5th (last) bind is the one dropped"
    );
}

/// One bound combo: padded out to 4 (the C++'s fill loop).
#[test]
fn resize_trigger_pads_short_triggers() {
    let mut combos = vec![key_combo(Key::SPACE)];
    resize_trigger_from_combos(&mut combos);
    assert_eq!(combos.len(), MAX_BINDS as usize);
    assert!(!combos[0].is_unbound());
    for c in &combos[1..] {
        assert!(c.is_unbound(), "the padding slots are cleared");
    }
}

/// `keyboard_binds_sanity_check`'s EXACT scope (Config.cpp:1392-1406):
/// the first 12 triggers are repaired, the other 4 (NextPack /
/// PreviousPack / LuaConsole / Pause) are left verbatim.
#[test]
fn keyboard_binds_sanity_check_repairs_exactly_the_cpp_twelve() {
    let mut cfg = Config::default();

    // Malform EVERY trigger: 5 combos (bound, cleared, bound, cleared,
    // bound) — the same shape for all 16, so the scope is the only
    // variable.
    let malformed = || {
        let c = vec![
            key_combo(Key::A),
            Combo::cleared(),
            key_combo(Key::D),
            Combo::cleared(),
            btn_combo(Button::MIDDLE),
        ];
        // Store it directly (no repair on the way in).
        c
    };
    for tid in [
        Tid::RotateCCW,
        Tid::RotateCW,
        Tid::Focus,
        Tid::Select,
        Tid::Exit,
        Tid::ForceRestart,
        Tid::Restart,
        Tid::Replay,
        Tid::Screenshot,
        Tid::Swap,
        Tid::Up,
        Tid::Down,
        Tid::NextPack,
        Tid::PreviousPack,
        Tid::LuaConsole,
        Tid::Pause,
    ] {
        *cfg.get_trigger(tid).get_combos_mut() = malformed();
    }

    cfg.keyboard_binds_sanity_check();

    // The C++'s 12: repaired (agglomerated + padded to 4).
    for tid in [
        Tid::RotateCCW,
        Tid::RotateCW,
        Tid::Focus,
        Tid::Select,
        Tid::Exit,
        Tid::ForceRestart,
        Tid::Restart,
        Tid::Replay,
        Tid::Screenshot,
        Tid::Swap,
        Tid::Up,
        Tid::Down,
    ] {
        let t = cfg.trigger_copy(tid);
        assert_eq!(
            t.get_combos().len(),
            MAX_BINDS as usize,
            "{tid:?} must be repaired to MAX_BINDS"
        );
        assert!(
            !t.get_combos()[0].is_unbound()
                && !t.get_combos()[1].is_unbound()
                && !t.get_combos()[2].is_unbound()
                && t.get_combos()[3].is_unbound(),
            "{tid:?} must have its 3 binds agglomerated at the front"
        );
    }

    // The other 4: untouched (still the malformed 5-combo shape).
    for tid in [
        Tid::NextPack,
        Tid::PreviousPack,
        Tid::LuaConsole,
        Tid::Pause,
    ] {
        let t = cfg.trigger_copy(tid);
        assert_eq!(
            t.get_combos().len(),
            5,
            "{tid:?} is outside the C++ scope — unchanged"
        );
    }
}

/// The check is a REPAIR, not a validator: once it has normalized a
/// config, running it again changes nothing (idempotent — the second
/// run sees every trigger already agglomerated and padded). The first
/// run on the shipped defaults DOES change the shape (the defaults
/// carry 2-3 combos, the C++'s loaded config carries whatever the file
/// has — both normalize to the 4-combo shape).
#[test]
fn keyboard_binds_sanity_check_is_idempotent_after_the_first_run() {
    let mut cfg = Config::default();
    cfg.keyboard_binds_sanity_check();
    let after_first = shape_map(&cfg);
    cfg.keyboard_binds_sanity_check();
    assert_eq!(
        after_first,
        shape_map(&cfg),
        "a second run must change nothing"
    );
}

type TriggerShape = (Vec<bool>, Vec<i32>); // (unbound flags, bound-key indices)

fn shape_map(cfg: &Config) -> Vec<(Tid, TriggerShape)> {
    [
        Tid::RotateCCW,
        Tid::RotateCW,
        Tid::Focus,
        Tid::Select,
        Tid::Exit,
        Tid::ForceRestart,
        Tid::Restart,
        Tid::Replay,
        Tid::Screenshot,
        Tid::Swap,
        Tid::Up,
        Tid::Down,
        Tid::NextPack,
        Tid::PreviousPack,
        Tid::LuaConsole,
        Tid::Pause,
    ]
    .iter()
    .map(|&tid| {
        let t = cfg.trigger_copy(tid);
        (
            tid,
            (
                t.get_combos().iter().map(|c| c.is_unbound()).collect(),
                t.get_combos()
                    .iter()
                    .filter_map(|c| {
                        c.get_keys()
                            .iter()
                            .copied()
                            .enumerate()
                            .find(|(_, on)| *on)
                            .map(|(i, _)| (i as i32) - 1)
                    })
                    .collect(),
            ),
        )
    })
    .collect()
}

/// A free function over a `Vec<Combo>` (the C++ `resizeTrigger` takes a
/// `Trigger&`; the port's `resize_trigger` takes `&mut Trigger` — this
/// adapter keeps the tests about the COMBOS shape).
fn resize_trigger_from_combos(combos: &mut Vec<Combo>) {
    let mut t = oh_config::Trigger::from_combos(combos.clone());
    resize_trigger(&mut t);
    *combos = t.get_combos().to_vec();
}
