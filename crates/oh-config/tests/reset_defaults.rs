//! 29f1 — C++ `resetConfigToDefaults()` (Config.cpp:538-544) resets ALL 101
//! `LinkedValue`s — its `resetAllFromDefault()` iterates the full
//! `X_LINKEDVALUES` macro whose last line is `X_LINKEDVALUES_BINDS`
//! (Config.cpp:294-373, :290-291, :455-460) — and ends with
//! `reapplyResolution()` (Config.cpp:521-535). After the reset both
//! auto-resolution flags are back to their NSDMI default `false`
//! (Config.cpp:321-322), so the trailing reapply reduces to its
//! `recalculateSizes()` tail (Config.cpp:625-639).

use oh_config::{Combo, Config, Jid, Key, Tid};

/// The 16 trigger binds (`X_LINKEDVALUES_BINDS_TRIGGERS`,
/// Config.cpp:264-288).
const TRIGGER_IDS: [Tid; 16] = [
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
];

/// The 12 joystick binds (`X_LINKEDVALUES_BINDS_JOYSTICK`,
/// Config.cpp:250-262).
const JOYSTICK_IDS: [Jid; 12] = [
    Jid::Select,
    Jid::Exit,
    Jid::Focus,
    Jid::Swap,
    Jid::ForceRestart,
    Jid::Restart,
    Jid::Replay,
    Jid::Screenshot,
    Jid::NextPack,
    Jid::PreviousPack,
    Jid::AddToFavorites,
    Jid::FavoritesMenu,
];

/// A "RESET CONFIG" (the C++ `Config::resetConfigToDefaults()`, the menu's
/// options item) must put EVERY bind back to its default — a custom
/// keyboard bind and a custom joystick bind included — AND recompute
/// `sizeX`/`sizeY`/`zoomFactor` (the C++'s trailing
/// `reapplyResolution()` → `recalculateSizes()`).
#[test]
fn reset_config_to_defaults_resets_binds_and_recomputes_sizes() {
    let mut cfg = Config::default();

    // Custom non-bind values.
    cfg.set_official(false);
    cfg.set_current_resolution(1280, 720);
    cfg.set_sound_volume(55.0);
    cfg.set_timescale(0.5);
    cfg.set_last_login_username("vittorio");

    // Custom binds: one keyboard trigger, one joystick bind.
    {
        let mut combo = Combo::new();
        combo.add_key(Key::F1);
        *cfg.get_trigger(Tid::Select).get_combos_mut() = vec![combo];
    }
    cfg.set_joystick_bind_value(Jid::Select, 7);

    cfg.reset_config_to_defaults();

    // Non-bind values back to their NSDMI defaults (Config.cpp:294-372).
    assert!(cfg.get_official(), "official default");
    assert_eq!(cfg.get_windowed_width(), 800, "windowed_width default");
    assert_eq!(cfg.get_windowed_height(), 600, "windowed_height default");
    assert_eq!(cfg.get_sound_volume(), 100.0, "sound_volume default");
    assert_eq!(cfg.get_timescale(), 1.0, "timescale default");
    assert_eq!(
        cfg.get_last_login_username(),
        "",
        "last_login_username default"
    );

    // Every keyboard bind back to its default (the pre-fix reset left the
    // 28 binds untouched — a custom `t_select` would survive).
    let fresh = Config::default();
    for tid in TRIGGER_IDS {
        assert_eq!(
            cfg.trigger_copy(tid),
            fresh.trigger_copy(tid),
            "{tid:?} back to its default bind"
        );
    }
    // Every joystick bind back to its default.
    for jid in JOYSTICK_IDS {
        assert_eq!(
            cfg.joystick_bind(jid),
            fresh.joystick_bind(jid),
            "{jid:?} back to its default button"
        );
    }

    // The trailing `reapplyResolution()`'s `recalculateSizes()`
    // (Config.cpp:625-639): the defaults' windowed 800×600 →
    // sizeX = sizeY = max(800, 600) * 1.3; zoomFactor =
    // max(1024/800, 768/600) (autoZoomFactor's default is true,
    // Config.cpp:314).
    assert_eq!(cfg.get_size_x(), 800.0_f32 * 1.3_f32, "sizeX recomputed");
    assert_eq!(cfg.get_size_y(), 800.0_f32 * 1.3_f32, "sizeY recomputed");
    assert_eq!(
        cfg.get_zoom_factor(),
        1024.0_f32 / 800.0_f32,
        "zoomFactor recomputed (1.28)"
    );
}

/// The C++ `resetBindsToDefaults()` (Config.cpp:546-550) scope stays
/// binds-only: the 28 binds reset, the other 73 `LinkedValue`s untouched.
#[test]
fn reset_binds_to_defaults_keeps_the_non_bind_values() {
    let mut cfg = Config::default();
    cfg.set_official(false);
    cfg.set_timescale(0.5);
    {
        let mut combo = Combo::new();
        combo.add_key(Key::F9);
        *cfg.get_trigger(Tid::Exit).get_combos_mut() = vec![combo];
    }
    cfg.set_joystick_bind_value(Jid::Restart, 9);

    cfg.reset_binds_to_defaults();

    let fresh = Config::default();
    for tid in TRIGGER_IDS {
        assert_eq!(
            cfg.trigger_copy(tid),
            fresh.trigger_copy(tid),
            "{tid:?} back to its default bind"
        );
    }
    for jid in JOYSTICK_IDS {
        assert_eq!(cfg.joystick_bind(jid), fresh.joystick_bind(jid));
    }
    // The binds-only scope: the non-binds keep their custom values.
    assert!(!cfg.get_official(), "official untouched");
    assert_eq!(cfg.get_timescale(), 0.5, "timescale untouched");
}
