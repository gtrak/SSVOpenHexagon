//! 29l1 — end-to-end: a hand-edited `config.json` carrying
//! jsoncpp-coercible types must LOAD (the C++ jsoncpp fork coerces them,
//! `jsoncpp.inl:1177-1322`); the pre-fix port hard-failed in
//! `load_config` and the app exited on a config the C++ accepts.

use std::fs;
use std::path::PathBuf;

use oh_config::Config;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oh-config-coerced-{}-{}-{}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace('-', "_"),
        tag
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// A user hand-editing their `config.json` in the types jsoncpp coerces:
/// a boolean written `1`, a float written `true`, an int written as a
/// real, a string written as a number, and a numeric element in a trigger
/// combo array. The C++ loads all of it (`asBool` int arm, `asFloat` bool
/// arm, `asInt` real arm, `asString` number arms, the combo item's
/// `asString` coercion → invalid-name warning → unbound slot,
/// `Global/UtilsJson.cpp:91-124`).
#[test]
fn a_hand_edited_coerced_config_loads() {
    let dir = temp_dir("coerced");
    fs::write(
        dir.join("config.json"),
        r#"{
            "vsync" : 1,
            "music_speed_mult" : true,
            "pixel_multiplier" : 1.5,
            "server_ip" : 123,
            "t_select" : [123]
        }"#,
    )
    .expect("write");

    let mut cfg = Config::default();
    cfg.load_config(&dir, &[] as &[String])
        .expect("the C++ loads this config — jsoncpp coerces every value");

    assert!(cfg.get_vsync(), "`\"vsync\": 1` → true (asBool's int arm)");
    assert_eq!(
        cfg.get_music_speed_mult(),
        1.0_f32,
        "`\"music_speed_mult\": true` → 1.0 (asFloat's bool arm)"
    );
    assert_eq!(
        cfg.get_pixel_multiplier(),
        1,
        "`\"pixel_multiplier\": 1.5` → 1 (asInt's real arm truncates)"
    );
    assert_eq!(
        cfg.get_server_ip(),
        "123",
        "`\"server_ip\": 123` → \"123\" (asString's int arm)"
    );
    // `\"t_select\": [123]` — the item coerces to \"123\", which is not a
    // valid key/button name → the C++ warns and puts an empty bind in its
    // place (UtilsJson.cpp:113-120): the combo is unbound, not an error.
    let trigger = cfg.trigger_copy(oh_config::Tid::Select);
    assert_eq!(trigger.get_combos().len(), 1);
    assert!(
        trigger.get_combos()[0].is_unbound(),
        "the coerced \"123\" is an invalid input name → the unbound slot"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The boundary: a type jsoncpp does NOT coerce (a string where a bool is
/// expected) still fails the sync — the C++ throws out of `loadConfig`
/// (uncaught in `mainClient` → process termination); the port's
/// documented clean-exit deviation surfaces it as `Err` instead.
#[test]
fn a_non_coercible_config_still_fails_the_sync() {
    let dir = temp_dir("non_coercible");
    fs::write(dir.join("config.json"), r#"{"official" : "yes"}"#).expect("write");

    let mut cfg = Config::default();
    let err = cfg
        .load_config(&dir, &[] as &[String])
        .expect_err("jsoncpp asBool has no string arm — the C++ throws");
    assert_eq!(err.message(), "Value is not convertible to bool.");
    let _ = fs::remove_dir_all(&dir);
}
