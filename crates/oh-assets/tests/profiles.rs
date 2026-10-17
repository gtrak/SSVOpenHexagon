//! The local-profile write/state surface (issue 029c1): the C++
//! `pCreate` / `pSetCurrent` / `pRemove` / `pSaveCurrent` / `pSaveAll` /
//! `getLocalScore` / `setLocalScore` (Assets.cpp:1001-1058, :1446-1566) —
//! the on-disk shape, the reload round-trips, and the C++'s guard
//! semantics.
//!
//! `Assets::new` needs the (possibly empty) `Packs/` folder to be
//! non-fatal (the `Assets/` + `assets.json` steps are skipped headless),
//! so every fixture carries one.

use std::fs;
use std::path::{Path, PathBuf};

use oh_assets::Assets;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oh-assets-profiles-{}-{}-{tag}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace('-', "_")
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Headless load over a fixture dir that always carries (possibly empty)
/// `Packs/` + `Profiles/` folders.
fn load(dir: &Path) -> Assets {
    fs::create_dir_all(dir.join("Packs")).expect("packs dir");
    fs::create_dir_all(dir.join("Profiles")).expect("profiles dir");
    Assets::new(dir, true, false).expect("headless load")
}

/// A fresh `Assets` (the C++ `currentProfilePtr{nullptr}` initial state —
/// loading profiles NEVER sets the current pointer; only `pSetCurrent`
/// does).
#[test]
fn fresh_assets_have_no_active_profile() {
    let dir = temp_dir("fresh");
    fs::create_dir_all(dir.join("Profiles")).expect("profiles dir");
    fs::write(
        dir.join("Profiles").join("a.json"),
        "{\n\t\"name\" : \"a\"\n}\n",
    )
    .expect("seed");
    let a = load(&dir);
    assert_eq!(a.get_local_profiles_size(), 1, "the file loads");
    assert!(!a.any_local_profile_active(), "current stays nullptr");
    assert!(a.current_local_profile_name().is_none());
    assert_eq!(a.get_local_score("any_level"), 0.0);
    assert!(!a.is_level_favorite("any_level"));
    // The C++ load bug (LoadFromJson.cpp:40-44 — `isObj("version")` tests
    // the literal, always false): every loaded profile's version is -1.0.0.
    let p = a.get_profile_data("a").expect("profile");
    assert_eq!(p.get_version(), oh_data::GameVersion::new(-1, 0, 0));
    let _ = fs::remove_dir_all(&dir);
}

/// C++ `pCreate` (Assets.cpp:1545-1561) — the on-disk compatibility
/// artifact: exactly the C++'s three keys (NO `version`; `scores` /
/// `favorites` = the C++'s `ssvuj::Obj{}` empty objects → `{}`), the fork
/// writer's byte-sorted members, then the store clear + reload.
#[test]
fn p_create_writes_the_cpp_json_shape_and_reloads() {
    let dir = temp_dir("create_shape");
    let a = load(&dir);
    a.profile_create("foo").expect("create");

    // The exact bytes the C++ `writeToFile` would produce for
    // `{name, scores: {}, favorites: {}}` (the top-level members
    // byte-sorted — the fork's `std::map` storage; the fork's
    // `StyledStreamWriter` comma after every member but the last — the
    // shipped `config.json`'s shape).
    let written = fs::read_to_string(dir.join("Profiles").join("foo.json")).expect("file");
    assert_eq!(
        written,
        "\n{\n\t\"favorites\" : {},\n\t\"name\" : \"foo\",\n\t\"scores\" : {}\n}\n"
    );

    // The store: cleared + reloaded from the file (the C++'s
    // `profileDataMap.clear(); loadAllLocalProfiles();`).
    assert_eq!(a.get_local_profiles_size(), 1);
    assert_eq!(a.get_profile_names(), vec!["foo".to_string()]);
    let p = a.get_profile_data("foo").expect("reloaded");
    assert!(
        p.get_scores().is_empty(),
        "empty `scores` object extracts empty"
    );
    assert!(
        p.get_favorite_level_ids().is_empty(),
        "empty `favorites` object extracts empty"
    );
    assert_eq!(p.get_version(), oh_data::GameVersion::new(-1, 0, 0));

    // A second `Assets` over the same dir parses the port-created file
    // back (on-disk compatibility: the C++ re-reads these files).
    let b = load(&dir);
    assert_eq!(
        b.get_local_profiles_size(),
        1,
        "the port's own file reloads"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// C++ `pCreate` in a dir WITHOUT `Profiles/`: the C++ `std::ofstream`
/// silently fails, the `loadAllLocalProfiles()` returns false, and the C++
/// `std::terminate()`s (Assets.cpp:1555-1560) — the port's clean `Err`
/// (the file write failed too, so nothing is on disk).
#[test]
fn p_create_without_the_profiles_dir_errors_cleanly() {
    let dir = temp_dir("create_missing_dir");
    fs::create_dir_all(dir.join("Packs")).expect("packs dir");
    let a = Assets::new(&dir, true, false).expect("load (no Profiles/ is not fatal)");
    assert_eq!(a.get_local_profiles_size(), 0);
    let err = a.profile_create("foo").expect_err("the C++ terminate arm");
    assert_eq!(err, "Error loading local profiles");
    assert!(
        !dir.join("Profiles").join("foo.json").exists(),
        "the write failed too (no Profiles/ dir)"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// set score → `pSaveCurrent` → reload: the score reaches the file (the
/// C++'s `saveCurrentLocalProfile` four-key shape, `version` = the
/// CURRENT `GAME_VERSION` 2.1.7 — the compile-time constant, not the
/// loaded profile's -1.0.0) and comes back through
/// `load_all_local_profiles`.
#[test]
fn set_score_save_current_and_reload_round_trips() {
    let dir = temp_dir("score_round_trip");
    let a = load(&dir);
    a.profile_create("p1").expect("create");
    a.profile_set_current("p1");
    a.set_local_score("lvl_a", 12.5);
    a.set_local_score("lvl_b", 0.5);
    a.profile_save_current();

    let written = fs::read_to_string(dir.join("Profiles").join("p1.json")).expect("file");
    assert!(written.contains("\"lvl_a\" : 12.50"), "{written}");
    assert!(written.contains("\"lvl_b\" : 0.50"), "{written}");
    // `version` = GAME_VERSION (byte-sorted members: major, micro, minor).
    assert!(
        written.contains("\"major\" : 2")
            && written.contains("\"micro\" : 7")
            && written.contains("\"minor\" : 1"),
        "{written}"
    );
    assert!(written.contains("\"name\" : \"p1\""), "{written}");

    let b = load(&dir);
    b.profile_set_current("p1");
    assert_eq!(b.get_local_score("lvl_a"), 12.5);
    assert_eq!(b.get_local_score("lvl_b"), 0.5);
    assert_eq!(b.get_local_score("never_played"), 0.0);
    let _ = fs::remove_dir_all(&dir);
}

/// favorites → save → reload round-trip (the C++'s set → array → set).
///
/// The fixture carries a minimal pack (the `addLocalProfile`
/// invalid-favorite filter, Assets.cpp:839-846, keeps only valid level
/// ids — `levelDatas` must hold the favorites; the level asset id is
/// `<packId>_<levelId>` = `<disamb>_<author>_<name>_<version>_<id>`).
#[test]
fn favorites_round_trip_through_save_and_reload() {
    let dir = temp_dir("favorites_round_trip");
    let pack = dir.join("Packs").join("favtest");
    fs::create_dir_all(pack.join("Levels")).expect("pack dir");
    fs::write(
        pack.join("pack.json"),
        r#"{
    "disambiguator": "ohvrfav",
    "name": "favtest",
    "author": "tester",
    "version": 1
}
"#,
    )
    .expect("pack.json");
    fs::write(pack.join("Levels").join("lvla.json"), r#"{ "id": "lvla" }"#).expect("level a");
    fs::write(pack.join("Levels").join("lvlb.json"), r#"{ "id": "lvlb" }"#).expect("level b");

    let lvl_a = "ohvrfav_tester_favtest_1_lvla";
    let lvl_b = "ohvrfav_tester_favtest_1_lvlb";
    fs::create_dir_all(dir.join("Profiles")).expect("profiles dir");
    fs::write(
        dir.join("Profiles").join("p1.json"),
        format!("\n{{\n\t\"favorites\" : [ \"{lvl_a}\", \"{lvl_b}\" ],\n\t\"name\" : \"p1\"\n}}\n"),
    )
    .expect("seed");
    let a = Assets::new(&dir, true, false).expect("load");
    let p = a.get_profile_data("p1").expect("p1");
    assert!(
        p.get_favorite_level_ids().contains(lvl_a) && p.get_favorite_level_ids().contains(lvl_b),
        "the valid level ids survive the filter"
    );

    a.profile_set_current("p1");
    a.profile_save_current();
    // The set → array (the BTreeSet's byte-sorted order — the C++'
    // unordered_set order is unspecified; the writer emits what the
    // store handed it).
    let written = fs::read_to_string(dir.join("Profiles").join("p1.json")).expect("file");
    assert!(
        written.contains(&format!("\"favorites\" : [ \"{lvl_a}\", \"{lvl_b}\" ]")),
        "{written}"
    );

    let b = Assets::new(&dir, true, false).expect("reload");
    b.profile_set_current("p1");
    assert!(
        b.is_level_favorite(lvl_a),
        "the saved array reloads as the set"
    );
    assert!(b.is_level_favorite(lvl_b));
    assert!(!b.is_level_favorite("ohvrfav_tester_favtest_1_lvlc"));
    let _ = fs::remove_dir_all(&dir);
}

/// C++ `saveCurrentLocalProfile`'s no-current-profile early return
/// (Assets.cpp:1002-1005): nothing is written.
#[test]
fn save_current_without_a_current_profile_writes_nothing() {
    let dir = temp_dir("save_no_current");
    fs::create_dir_all(dir.join("Profiles")).expect("profiles dir");
    let seed = "{\n\t\"name\" : \"p1\"\n}\n";
    fs::write(dir.join("Profiles").join("p1.json"), seed).expect("seed");
    let a = load(&dir);
    a.profile_save_current();
    let after = fs::read_to_string(dir.join("Profiles").join("p1.json")).expect("file");
    assert_eq!(after, seed, "the early return left the file untouched");
    let _ = fs::remove_dir_all(&dir);
}

/// C++ `pSaveAll` (Assets.cpp:1027-1058): rewrites EVERY profile's file
/// (with no current profile at all — the save is map-driven, not
/// current-driven).
#[test]
fn save_all_rewrites_every_profile_file() {
    let dir = temp_dir("save_all");
    fs::create_dir_all(dir.join("Profiles")).expect("profiles dir");
    fs::write(
        dir.join("Profiles").join("a.json"),
        "{\n\t\"name\" : \"a\"\n}\n",
    )
    .expect("seed a");
    fs::write(
        dir.join("Profiles").join("b.json"),
        "{\n\t\"name\" : \"b\"\n}\n",
    )
    .expect("seed b");
    let a = load(&dir);
    assert!(!a.any_local_profile_active());
    a.profile_save_all();
    for name in ["a", "b"] {
        let written = fs::read_to_string(dir.join("Profiles").join(format!("{name}.json")))
            .expect("rewritten");
        assert!(
            written.contains(&format!("\"name\" : \"{name}\""))
                && written.contains("\"major\" : 2"),
            "{name} rewritten to the four-key shape: {written}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

/// C++ `pRemove` (Assets.cpp:1563-1566) — the map erase ONLY: the file is
/// the MENU's job (MenuGame.cpp:2419-2431).
#[test]
fn profile_remove_erases_only_the_store() {
    let dir = temp_dir("remove_map_only");
    fs::create_dir_all(dir.join("Profiles")).expect("profiles dir");
    fs::write(
        dir.join("Profiles").join("p1.json"),
        "{\n\t\"name\" : \"p1\"\n}\n",
    )
    .expect("seed");
    let a = load(&dir);
    a.profile_remove("p1");
    assert!(
        a.get_profile_data("p1").is_none(),
        "the store entry is gone"
    );
    assert!(
        dir.join("Profiles").join("p1.json").exists(),
        "the file remains (the menu's `std::remove` deletes it)"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// C++ `pSetCurrent` (Assets.cpp:1537-1543) for a name that is not in the
/// store: the C++ asserts (release: UB); the port no-ops (documented
/// deviation) — the current stays what it was.
#[test]
fn profile_set_current_of_a_missing_name_is_a_noop() {
    let dir = temp_dir("set_current_missing");
    fs::create_dir_all(dir.join("Profiles")).expect("profiles dir");
    fs::write(
        dir.join("Profiles").join("p1.json"),
        "{\n\t\"name\" : \"p1\"\n}\n",
    )
    .expect("seed");
    let a = load(&dir);
    a.profile_set_current("ghost");
    assert!(!a.any_local_profile_active(), "still no current profile");
    a.profile_set_current("p1");
    assert!(a.any_local_profile_active());
    assert_eq!(a.current_local_profile_name().as_deref(), Some("p1"));
    a.profile_set_current("ghost");
    assert_eq!(
        a.current_local_profile_name().as_deref(),
        Some("p1"),
        "the unknown name did not displace the current"
    );
    let _ = fs::remove_dir_all(&dir);
}
