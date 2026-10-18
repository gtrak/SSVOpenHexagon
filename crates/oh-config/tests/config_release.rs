//! Integration tests against the REAL `_RELEASE/config.json` and
//! `_RELEASE/ConfigOverrides/*.json` (task spec verification §2–§5;
//! mapping §6/§9).

use std::path::{Path, PathBuf};

use oh_config::{
    bind_to_human_readable_name, clear_trigger_bind, default_server_level_whitelist,
    rebind_trigger, resize_trigger, Button, Config, Jid, JoystickBindControl, Key,
    KeyboardBindControl, Tid, TICKS_PER_SECOND, TIME_STEP,
};
use oh_data::json::{self, Obj};

fn release_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../_RELEASE")
}

fn load_release(override_ids: &[&str]) -> Config {
    let mut cfg = Config::default();
    cfg.load_config(&release_dir(), override_ids)
        .expect("loading the shipped _RELEASE config must not fail");
    cfg
}

/// A unique temp dir for file round-trips (tests run in threads in one
/// process; the gate has no `tempfile` dependency to add).
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oh-config-test-{}-{}-{}",
        tag,
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn timing_constants_are_exact() {
    assert_eq!(TICKS_PER_SECOND, 240.0_f32);
    // C++ `60.f / 240.f` == 0.25f exactly.
    assert_eq!(TIME_STEP, 0.25_f32);
    assert_eq!(TIME_STEP.to_bits(), 0.25_f32.to_bits());
}

#[test]
fn shipped_config_loads_with_exact_values() {
    let cfg = load_release(&[]);

    // bools
    assert!(cfg.get_3d()); // "3D_enabled" : true
    assert!(cfg.get_official());
    assert!(!cfg.get_fullscreen());
    assert!(!cfg.get_vsync());
    assert!(cfg.get_limit_fps());
    assert!(!cfg.get_first_time_playing()); // shipped: false
    assert!(cfg.get_show_fps()); // shipped: true
    assert!(cfg.get_show_level_info()); // shipped: true
    assert!(!cfg.get_no_sound() && !cfg.get_no_music());
    assert!(cfg.get_show_status_text() && cfg.get_show_timer());

    // ints / uints
    assert_eq!(cfg.get_max_fps(), 200);
    assert_eq!(cfg.get_antialiasing_level(), 4);
    assert_eq!(cfg.get_pixel_multiplier(), 1);
    assert_eq!(cfg.get_windowed_width(), 1736);
    assert_eq!(cfg.get_windowed_height(), 901);
    assert_eq!(cfg.get_fullscreen_width(), 1920);
    assert_eq!(cfg.get_fullscreen_height(), 1080);
    assert_eq!(cfg.get_3d_max_depth(), 100);
    assert_eq!(cfg.get_server_port(), 50505);
    assert_eq!(cfg.get_server_control_port(), 50506);

    // floats — the file holds f32-precise doubles; official-locked getters
    // return the defaults
    assert_eq!(cfg.get_sound_volume(), 100.0_f32);
    assert_eq!(cfg.get_player_speed(), 9.45_f32); // official -> default
    assert_eq!(cfg.get_player_focus_speed(), 4.625_f32);
    assert_eq!(cfg.get_player_size(), 7.3_f32);
    assert_eq!(cfg.get_zoom_factor(), 0.8523862361907959_f64 as f32);
    assert_eq!(cfg.get_key_icons_scale(), 0.75_f32);
    assert_eq!(cfg.get_joystick_deadzone(), 5.0_f32);
    assert_eq!(cfg.get_3d_multiplier(), 1.0_f32);
    assert_eq!(cfg.get_player_trail_scale(), 0.8999999761581421_f64 as f32);

    // strings
    assert_eq!(cfg.get_server_ip(), "139.162.199.162");
    assert_eq!(cfg.get_last_login_username(), "vee");

    // the shipped whitelist is the 82-entry (truncated) file value
    let whitelist = cfg.get_server_level_whitelist();
    assert_eq!(whitelist.len(), 82);
    assert_eq!(
        whitelist[0],
        "ohvrvanilla_vittorio_romeo_cube_1_apeirogon_m_0.35"
    );
    assert_eq!(
        whitelist[81],
        "vanitylevels_Vipre_Vanity_10006_troglodyte_m_1.002"
    );

    // joystick bind values (Jid order)
    assert_eq!(cfg.joystick_bind(Jid::Select), 1);
    assert_eq!(cfg.joystick_bind(Jid::Exit), 2);
    assert_eq!(cfg.joystick_bind(Jid::Focus), 4);
    assert_eq!(cfg.joystick_bind(Jid::Swap), 5);
    assert_eq!(cfg.joystick_bind(Jid::ForceRestart), 3);
    assert_eq!(cfg.joystick_bind(Jid::Restart), 0);
    assert_eq!(cfg.joystick_bind(Jid::Replay), 6);
    assert_eq!(cfg.joystick_bind(Jid::Screenshot), 7);
    assert_eq!(cfg.joystick_bind(Jid::NextPack), 11);
    assert_eq!(cfg.joystick_bind(Jid::PreviousPack), 10);
    assert_eq!(cfg.joystick_bind(Jid::AddToFavorites), 8);
    assert_eq!(cfg.joystick_bind(Jid::FavoritesMenu), 9);

    // keyboard binds: t_select[0] = kSpace ("t_main[0][0]" style)
    let select = cfg.trigger_ref(Tid::Select);
    assert!(!select.get_combos()[0].is_unbound());
    assert!(select.get_combos()[0].get_keys()[Key::SPACE.0 as usize + 1]);
    assert!(select.get_combos()[1].get_btns()[Button::MIDDLE.0 as usize + 1]);
    assert!(select.get_combos()[2].is_unbound());
    // the shipped file stores t_restart as [kSpace, kEnter, bMiddle, ""]
    let restart = cfg.trigger_ref(Tid::Restart);
    assert_eq!(restart.get_combos().len(), 4);
    assert!(restart.get_combos()[0].get_keys()[Key::SPACE.0 as usize + 1]);
    assert!(restart.get_combos()[1].get_keys()[Key::ENTER.0 as usize + 1]);
    assert!(restart.get_combos()[2].get_btns()[Button::MIDDLE.0 as usize + 1]);
    assert!(restart.get_combos()[3].is_unbound());
    // t_lua_console is stored with a single (unpadded) combo in the file
    assert_eq!(cfg.trigger_ref(Tid::LuaConsole).get_combos().len(), 1);

    // bind display names
    assert_eq!(cfg.get_keyboard_bind_names(Tid::Select), "SPACE, MMB");
    assert_eq!(cfg.get_keyboard_bind_names(Tid::RotateCCW), "A, LEFT, LMB");
    assert_eq!(cfg.get_keyboard_bind_names(Tid::ForceRestart), "UP, R");
    assert_eq!(cfg.get_keyboard_bind_names(Tid::Replay), "Y");
    assert_eq!(cfg.get_keyboard_bind_names(Tid::LuaConsole), "F1");
}

#[test]
fn default_config_values() {
    // no config.json in base_dir -> null root -> all compile-time defaults
    let dir = temp_dir("defaults");
    let mut cfg = Config::default();
    cfg.load_config(&dir, &[] as &[&str]).unwrap();

    assert!(cfg.get_official());
    assert!(!cfg.get_no_pulse());
    assert!(!cfg.get_debug());
    assert!(cfg.get_3d());
    assert_eq!(cfg.get_max_fps(), 200);
    assert_eq!(cfg.get_player_speed(), 9.45_f32);
    assert_eq!(cfg.get_player_focus_speed(), 4.625_f32);
    assert_eq!(cfg.get_player_size(), 7.3_f32);
    assert_eq!(cfg.get_zoom_factor(), 1.27_f32);
    assert_eq!(cfg.get_server_ip(), "139.162.199.162");
    assert_eq!(cfg.get_server_port(), 50505);
    assert_eq!(cfg.get_server_control_port(), 50506);
    assert_eq!(
        cfg.get_server_level_whitelist().len(),
        default_server_level_whitelist().len()
    );
    assert_eq!(default_server_level_whitelist().len(), 174);
    assert_eq!(cfg.get_size_x(), 1500.0_f32);
    assert_eq!(cfg.get_size_y(), 1500.0_f32);
    assert_eq!(cfg.get_spawn_distance(), 1600.0_f32);
    assert!(cfg.is_eligible_for_score());

    // default binds (one input per combo, exactly as Config.cpp)
    let cfg2 = cfg.clone();
    assert_eq!(cfg2.joystick_bind(Jid::Swap), 5);
    assert_eq!(cfg2.joystick_bind(Jid::NextPack), 11);
    let select = cfg2.trigger_ref(Tid::Select);
    assert_eq!(select.get_combos().len(), 2);
    assert!(select.get_combos()[0].get_keys()[Key::SPACE.0 as usize + 1]);
    assert!(select.get_combos()[1].get_btns()[Button::MIDDLE.0 as usize + 1]);
    assert_eq!(cfg2.get_keyboard_bind_names(Tid::Select), "SPACE, MMB");
    assert_eq!(cfg2.get_keyboard_bind_names(Tid::RotateCW), "D, RIGHT, RMB");
    assert_eq!(
        cfg2.get_keyboard_bind_names(Tid::Focus),
        "LSHIFT, BXBUTTON1"
    );
    assert_eq!(cfg2.get_keyboard_bind_names(Tid::Exit), "T, BXBUTTON2");
    assert_eq!(
        cfg2.get_keyboard_bind_names(Tid::Restart),
        "SPACE, ENTER, MMB"
    );
}

#[test]
fn overrides_change_exactly_the_keys_they_name() {
    let overrides_dir = release_dir().join("ConfigOverrides");
    let mut names: Vec<String> = std::fs::read_dir(&overrides_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".json"))
        .collect();
    names.sort();
    assert_eq!(
        names.iter().map(String::as_str).collect::<Vec<_>>(),
        [
            "debug.json",
            "fraps.json",
            "fullscreen.json",
            "highfps.json",
            "localserver.json",
            "no3D.json",
            "noaudio.json",
            "nocosmeticscript.json",
            "remoteserver.json",
            "windowed.json"
        ]
    );

    let mut base = load_release(&[]);
    let base_map: Obj = json::parse(&base.save_to_string()).unwrap();

    for file in &names {
        let name = file.strip_suffix(".json").unwrap();
        let override_map: Obj =
            json::parse(&std::fs::read_to_string(overrides_dir.join(file)).unwrap()).unwrap();
        assert!(override_map.is_object());

        let mut ovr = load_release(&[name]);
        let ovr_map: Obj = json::parse(&ovr.save_to_string()).unwrap();

        // every key the override names carries exactly its value
        if let Obj::Object(expected) = &override_map {
            for (k, v) in expected {
                assert_eq!(ovr_map.get(k), Some(v), "override key `{k}` in {file}");
            }
        }
        // nothing else changed: every base key not named keeps its value,
        // and no foreign key appears
        for (k, v) in base_map.as_object().unwrap() {
            if !override_map.get(k).is_some() {
                assert_eq!(
                    ovr_map.get(k),
                    Some(v),
                    "key `{k}` must be untouched by {file}"
                );
            }
        }
        for k in ovr_map.as_object().unwrap().keys() {
            assert!(
                override_map.get(k).is_some() || base_map.get(k).is_some(),
                "foreign key `{k}` appeared via {file}"
            );
        }

        // and the values are actually reflected in the struct
        if let Obj::Object(expected) = &override_map {
            for (k, v) in expected {
                match k.as_str() {
                    // official-locked values: the getters return the default
                    // while official is on, so verify via the JSON only
                    "debug" | "invincible" => assert!(matches!(v, Obj::Bool(_))),
                    "fullscreen" => {
                        assert_eq!(ovr.get_fullscreen(), v.as_bool().unwrap())
                    }
                    "fullscreen_auto_resolution" => {
                        assert_eq!(ovr.get_fullscreen_auto_resolution(), v.as_bool().unwrap())
                    }
                    "fullscreen_width" => {
                        assert_eq!(ovr.get_fullscreen_width(), v.as_u64().unwrap() as u32)
                    }
                    "fullscreen_height" => {
                        assert_eq!(ovr.get_fullscreen_height(), v.as_u64().unwrap() as u32)
                    }
                    "vsync" => assert_eq!(ovr.get_vsync(), v.as_bool().unwrap()),
                    "3D_enabled" => assert_eq!(ovr.get_3d(), v.as_bool().unwrap()),
                    "draw_text_outlines" => {
                        assert_eq!(ovr.get_draw_text_outlines(), v.as_bool().unwrap())
                    }
                    "no_music" => assert_eq!(ovr.get_no_music(), v.as_bool().unwrap()),
                    "no_sound" => assert_eq!(ovr.get_no_sound(), v.as_bool().unwrap()),
                    "server_ip" => assert_eq!(ovr.get_server_ip(), v.as_str().unwrap()),
                    "server_port" => assert_eq!(ovr.get_server_port(), v.as_u64().unwrap() as u16),
                    "show_messages" => {
                        assert_eq!(ovr.get_show_messages(), v.as_bool().unwrap())
                    }
                    // nocosmeticscript.json's change_styles/change_music are
                    // UNKNOWN config keys: shallow-merged into root (re-saved)
                    // but not part of any value
                    "change_styles" | "change_music" => {
                        assert!(matches!(v, Obj::Bool(_)));
                    }
                    other => panic!("unhandled override key {other}"),
                }
            }
        }
    }
}

#[test]
fn save_reload_round_trip_is_stable() {
    let mut cfg = load_release(&[]);
    cfg.keyboard_binds_sanity_check();
    let s1 = cfg.save_to_string();

    // save through the file API too
    let dir = temp_dir("roundtrip");
    cfg.save_config(&dir.join("config.json")).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("config.json")).unwrap(),
        s1
    );

    // reload from the written file and re-save: byte-identical
    let mut cfg2 = Config::default();
    cfg2.load_config(&dir, &[] as &[&str])
        .expect("reloading the saved config must not fail");
    let s2 = cfg2.save_to_string();
    assert_eq!(s2, s1, "second save must be byte-identical");

    // the re-parsed struct carries the same values
    let shipped: Obj =
        json::parse(&std::fs::read_to_string(release_dir().join("config.json")).unwrap()).unwrap();
    let saved: Obj = json::parse(&s1).unwrap();
    let shipped_keys: Vec<&str> = shipped
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let saved_keys: Vec<&str> = saved
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(shipped_keys, saved_keys, "top-level key sets must match");
    assert_eq!(saved_keys.len(), 101);

    for (k, v) in shipped.as_object().unwrap() {
        assert_eq!(
            saved.get(k),
            Some(v),
            "value of `{k}` must survive the round trip"
        );
    }

    assert_eq!(cfg2.get_keyboard_bind_names(Tid::Select), "SPACE, MMB");
    assert_eq!(cfg2.get_max_fps(), 200);
    assert_eq!(cfg2.get_last_login_username(), "vee");
}

#[test]
fn bind_sanity_checks_on_real_config() {
    let mut cfg = load_release(&[]);
    cfg.keyboard_binds_sanity_check();

    // the first 12 triggers are padded to exactly 4 combos
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
        assert_eq!(
            cfg.trigger_ref(tid).get_combos().len(),
            4,
            "{tid:?} must have 4 combos after the sanity check"
        );
    }
    // NextPack/PreviousPack/LuaConsole/Pause are NOT resized (1 combo each
    // in the shipped file)
    for tid in [
        Tid::NextPack,
        Tid::PreviousPack,
        Tid::LuaConsole,
        Tid::Pause,
    ] {
        assert_eq!(cfg.trigger_ref(tid).get_combos().len(), 1, "{tid:?}");
    }
    cfg.joystick_binds_sanity_check(); // documented no-op (no C++ definition)
}

#[test]
fn resize_rebind_clear_semantics() {
    let mut trig = oh_config::Trigger::from_combos(vec![
        oh_config::Combo::cleared(),
        {
            let mut c = oh_config::Combo::new();
            c.add_key(Key::W);
            c
        },
        oh_config::Combo::cleared(),
    ]);

    // agglomerate: drop unbound slots, keep the W combo, pad to 4
    resize_trigger(&mut trig);
    let combos = trig.get_combos();
    assert_eq!(combos.len(), 4);
    assert!(!combos[0].is_unbound());
    assert!(combos[0].get_keys()[Key::W.0 as usize + 1]);
    for c in &combos[1..] {
        assert!(c.is_unbound());
    }

    // more than maxBinds -> pop from the back
    for _ in 0..4 {
        let mut c = oh_config::Combo::new();
        c.add_key(Key::A);
        trig.get_combos_mut().push(c);
    }
    resize_trigger(&mut trig);
    assert_eq!(trig.get_combos().len(), 4);

    // rebind: key > -1 binds a key at the slot; index >= 4 wraps to 0 +
    // clears slot 0
    clear_trigger_bind(&mut trig, 0);
    assert!(trig.get_combos()[0].is_unbound());
    rebind_trigger(&mut trig, Key::SPACE.0, Button::LEFT.0, 0);
    assert!(trig.get_combos()[0].get_keys()[Key::SPACE.0 as usize + 1]);
    rebind_trigger(&mut trig, -1, Button::MIDDLE.0, 4);
    assert!(trig.get_combos()[0].get_btns()[Button::MIDDLE.0 as usize + 1]);
    assert!(!trig.get_combos()[0].get_keys()[Key::SPACE.0 as usize + 1]);
}

#[test]
fn joystick_bind_names_and_sanity() {
    let cfg = load_release(&[]);

    // MS vendor (0x045E): column 0, XBOX 360 layout (A0 B1 X2 Y3 LB4 RB5...)
    assert_eq!(cfg.get_joystick_bind_name(Jid::Select, 0x045E), "B"); // j_select = 1
    assert_eq!(cfg.get_joystick_bind_name(Jid::Exit, 0x045E), "X"); // j_exit = 2
    assert_eq!(cfg.get_joystick_bind_name(Jid::Focus, 0x045E), "LB"); // j_focus = 4
                                                                      // Sony vendor (0x54C): column 1 (A0 B1 X2 Y3 L1 R1...)
    assert_eq!(cfg.get_joystick_bind_name(Jid::Select, 0x54C), "CROSS");
    assert_eq!(cfg.get_joystick_bind_name(Jid::Swap, 0x54C), "R1"); // j_swap = 5
                                                                    // other/absent vendor: plain toStr
    assert_eq!(cfg.get_joystick_bind_name(Jid::Select, 0), "1");
    assert_eq!(cfg.get_joystick_bind_name(Jid::NextPack, 0), "11");

    // 33 is the unbound sentinel; >= 12 with a known vendor -> ""
    let mut cfg2 = cfg.clone();
    cfg2.set_joystick_bind_value(Jid::Restart, 33);
    assert_eq!(cfg2.get_joystick_bind_name(Jid::Restart, 0x045E), "");
    cfg2.set_joystick_bind_value(Jid::Focus, 12);
    assert_eq!(cfg2.get_joystick_bind_name(Jid::Focus, 0x045E), "");
    assert_eq!(cfg2.get_joystick_bind_name(Jid::Focus, 0x1234), "12");
}

#[test]
fn eligibility_reasons() {
    let mut cfg = Config::default();
    assert!(cfg.is_eligible_for_score());
    assert_eq!(cfg.get_uneligibility_reason(), "");

    cfg.set_official(false);
    assert!(!cfg.is_eligible_for_score());
    assert_eq!(cfg.get_uneligibility_reason(), "official mode off");

    // the C++ getters are official-locked, so with official on every later
    // branch sees the defaults and passes (the port mirrors that)
    cfg.set_official(true);
    cfg.set_debug(true);
    cfg.set_no_pulse(true);
    assert!(cfg.is_eligible_for_score());
    assert!(!cfg.get_debug());
    assert!(!cfg.get_no_pulse());
}

#[test]
fn resolution_and_sizing_math() {
    let mut cfg = Config::default();
    // windowed 800x600 defaults: size = max(800, 600) * 1.3
    cfg.recalculate_sizes();
    assert_eq!(cfg.get_size_x(), 800.0_f32 * 1.3_f32);
    assert_eq!(cfg.get_size_y(), 800.0_f32 * 1.3_f32);
    assert_eq!(
        cfg.get_zoom_factor(),
        (1024.0_f32 / 800.0_f32).max(768.0_f32 / 600.0_f32)
    );

    cfg.set_fullscreen(true);
    assert_eq!(cfg.get_width(), 1920);
    assert_eq!(cfg.get_height(), 1080);
    assert_eq!(cfg.get_size_x(), 1920.0_f32 * 1.3_f32);

    cfg.set_current_resolution(1280, 720);
    assert!(!cfg.get_fullscreen_auto_resolution());
    assert_eq!(cfg.get_fullscreen_width(), 1280);
    assert_eq!(cfg.get_fullscreen_height(), 720);

    cfg.set_current_resolution_auto(2560, 1440);
    assert!(cfg.get_fullscreen_auto_resolution());
    assert_eq!(cfg.get_fullscreen_width(), 2560);
    assert_eq!(cfg.get_fullscreen_height(), 1440);

    // auto resolution on with official-locked autoZoomFactor -> zoom factor
    // still updates (default true)
    cfg.reapply_resolution(2560, 1440);
    assert_eq!(
        cfg.get_zoom_factor(),
        (1024.0_f32 / 2560.0_f32).max(768.0_f32 / 1440.0_f32)
    );

    // manual antialiasing setter only writes on change
    assert_eq!(cfg.get_antialiasing_level(), 4);
    cfg.set_antialiasing_level(4);
    cfg.set_antialiasing_level(8);
    assert_eq!(cfg.get_antialiasing_level(), 8);

    // vsync / limit fps / max fps setters
    cfg.set_vsync(true);
    cfg.set_limit_fps(false);
    cfg.set_max_fps(144);
    assert!(cfg.get_vsync() && !cfg.get_limit_fps());
    assert_eq!(cfg.get_max_fps(), 144);
}

#[test]
fn bind_control_keyboard() {
    let mut cfg = load_release(&[]);
    cfg.keyboard_binds_sanity_check();

    let mut ctl = KeyboardBindControl::new("Select", &mut cfg, Tid::Select, Key::UNKNOWN);
    assert_eq!(ctl.get_name(&cfg), "Select: SPACE, MMB");

    // binding an already-assigned key is a no-op (but stops waiting)
    assert!(ctl.new_keyboard_bind_key(&mut cfg, Key::SPACE, |_, _| {}));
    assert_eq!(cfg.get_keyboard_bind_names(Tid::Select), "SPACE, MMB");
    // a new key lands in the next free slot (after the active combos)
    assert!(ctl.new_keyboard_bind_key(&mut cfg, Key::Q, |_, _| {}));
    assert_eq!(cfg.get_keyboard_bind_names(Tid::Select), "SPACE, MMB, Q");
    // an already-assigned button is a no-op
    assert!(ctl.new_keyboard_bind_button(&mut cfg, Button::MIDDLE, |_, _| {}));
    // erase removes the LAST active slot
    assert!(ctl.erase(&mut cfg, |_, _| {}));
    assert_eq!(cfg.get_keyboard_bind_names(Tid::Select), "SPACE, MMB");
    assert!(ctl.erase(&mut cfg, |_, _| {}));
    assert_eq!(cfg.get_keyboard_bind_names(Tid::Select), "SPACE");
    assert!(ctl.erase(&mut cfg, |_, _| {}));
    assert_eq!(cfg.get_keyboard_bind_names(Tid::Select), "");
    assert!(!ctl.erase(&mut cfg, |_, _| {}));

    // waiting flag toggles and the trailing "_"
    ctl.exec();
    assert!(ctl.is_waiting_for_bind());
    assert!(ctl.get_name(&cfg).ends_with('_'));
}

#[test]
fn bind_control_keyboard_hardcoded_key_sanitized() {
    // HexagonGame passes e.g. Key::ESC? no — the hardcoded set is J K L R Y
    // Escape LAlt Enter Backspace Up Down F1..F4: force a hardcoded key into
    // t_force_restart (Up) manually and check the ctor sanitizes it.
    let mut cfg = Config::default();
    {
        let trig = cfg.get_trigger(Tid::ForceRestart);
        clear_trigger_bind(trig, 0);
        clear_trigger_bind(trig, 1);
        let mut c = oh_config::Combo::new();
        c.add_key(Key::UP);
        let mut c2 = oh_config::Combo::new();
        c2.add_key(Key::R);
        *trig.get_combos_mut() = vec![c, c2];
    }
    let _ctl = KeyboardBindControl::new("FR", &mut cfg, Tid::ForceRestart, Key::UP);
    // the combo holding kUp must have been cleared
    let combos = cfg.trigger_ref(Tid::ForceRestart).get_combos();
    assert!(combos[0].is_unbound());
    assert!(!combos[1].is_unbound());

    // binding the hardcoded key is rejected
    let mut ctl2 = KeyboardBindControl::new("FR", &mut cfg, Tid::ForceRestart, Key::UP);
    assert!(!ctl2.new_keyboard_bind_key(&mut cfg, Key::UP, |_, _| {}));
    assert!(!ctl2.is_waiting_for_bind());
}

#[test]
fn bind_control_joystick() {
    let mut cfg = load_release(&[]);
    let mut ctl = JoystickBindControl::new("Swap", Jid::Swap);
    assert_eq!(ctl.get_name(&cfg, 0), "Swap: 5");

    ctl.exec();
    assert!(ctl.is_waiting_for_bind());
    // same button -> no change
    ctl.new_joystick_bind(&mut cfg, 5, |_, _| {});
    assert_eq!(cfg.joystick_bind(Jid::Swap), 5);
    assert!(!ctl.is_waiting_for_bind());
    // different button -> applied + callback args
    let called = std::cell::Cell::new(0u32);
    ctl.new_joystick_bind(&mut cfg, 6, |joy, id| {
        called.set(called.get() + 1);
        assert_eq!(joy, 6);
        assert_eq!(id, Jid::Swap as i32);
    });
    assert_eq!(called.get(), 1);
    assert_eq!(cfg.joystick_bind(Jid::Swap), 6);
    // erase -> 33 sentinel; second erase is a no-op
    assert!(ctl.erase(&mut cfg, |_, _| {}));
    assert_eq!(cfg.joystick_bind(Jid::Swap), 33);
    assert!(!ctl.erase(&mut cfg, |_, _| {}));
}

#[test]
fn joystick_state_machine() {
    // feed binds from the shipped config (j_restart = 0, j_select = 1, ...)
    let cfg = load_release(&[]);
    cfg.load_all_joystick_binds();

    // nothing pressed -> no presses, no edges
    oh_config::joystick::update(5.0, &oh_config::JoystickPoll::default());
    assert!(!oh_config::joystick::pressed(Jid::Restart));

    // press SFML button 0 (j_restart) on controller 0
    let mut poll = oh_config::JoystickPoll::default();
    poll.buttons[0] = true;
    oh_config::joystick::update(5.0, &poll);
    assert!(oh_config::joystick::pressed(Jid::Restart));
    assert!(oh_config::joystick::rising_edge(Jid::Restart));
    // still held -> pressed but no rising edge
    oh_config::joystick::update(5.0, &poll);
    assert!(oh_config::joystick::pressed(Jid::Restart));
    assert!(!oh_config::joystick::rising_edge(Jid::Restart));
    // release
    poll.buttons[0] = false;
    oh_config::joystick::update(5.0, &poll);
    assert!(!oh_config::joystick::pressed(Jid::Restart));

    // ignoreAllPresses: update() is a complete no-op
    oh_config::joystick::ignore_all_presses(true);
    poll.buttons[0] = true;
    oh_config::joystick::update(5.0, &poll);
    assert!(!oh_config::joystick::pressed(Jid::Restart));
    oh_config::joystick::ignore_all_presses(false);
    oh_config::joystick::update(5.0, &poll);
    assert!(oh_config::joystick::pressed(Jid::Restart));

    // directions: left stick X past the deadzone -> Left, inverted stick Y
    poll.buttons[0] = false;
    poll.x = -20.0;
    poll.y = -20.0;
    oh_config::joystick::update(5.0, &poll);
    assert!(oh_config::joystick::pressed_dir(oh_config::Jdir::Left));
    assert!(oh_config::joystick::pressed_dir(oh_config::Jdir::Up));
    assert!(!oh_config::joystick::pressed_dir(oh_config::Jdir::Right));
    // reset for the other tests
    poll.x = 0.0;
    poll.y = 0.0;
    oh_config::joystick::update(5.0, &poll);
}

#[test]
fn bind_name_human_readable() {
    assert_eq!(bind_to_human_readable_name("kSpace"), "Space");
    assert_eq!(bind_to_human_readable_name("bLeft"), "LMB");
    assert_eq!(bind_to_human_readable_name("bRight"), "RMB");
    assert_eq!(bind_to_human_readable_name("bMiddle"), "MMB");
    assert_eq!(bind_to_human_readable_name("bXButton1"), "bXButton1");
}
