//! Integration test: scan the REAL `_RELEASE` tree and assert pack/level
//! counts, exact `LevelData`, discovery order, the asset registry, and the
//! `LoadInfo` error-accumulation contract (task spec verification §1–5).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use oh_assets::pack_scan;
use oh_assets::Assets;
use oh_vfs::RealFs;

fn release_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../_RELEASE")
        .to_path_buf()
}

fn cube_id() -> &'static str {
    "ohvrvanilla_vittorio_romeo_cube_1"
}

/// Primary load: `headless = true` skips the SFML-backed sub-loaders
/// (assets.json, shaders, custom sounds, music `.ogg`) that the render/audio
/// backend owns (issue 10), so this asserts the pure data-side pipeline.
fn load_headless() -> Assets {
    Assets::new(Arc::new(RealFs::new(release_dir())), true, false).expect("_RELEASE loads headless")
}

#[test]
fn real_tree_pack_count_is_eight() {
    let a = load_headless();
    // All 8 shipped packs have a valid pack.json → `loadInfo.packs` == 8.
    assert_eq!(a.get_load_results().packs, 8);
    // Every shipped pack id is present.
    for id in [
        "ohvrvanilla_vittorio_romeo_base_1",
        "ohvrvanilla_vittorio_romeo_cube_1",
        "ohvrvanilla_vittorio_romeo_hypercube_1",
        "ohvrvanilla_vittorio_romeo_orthoplex_1",
        "ohvrvanilla_vittorio_romeo_tutorial_1",
        "ohvrvanilla_vittorio_romeo_experimental_1",
        "thing_Synth_Morxemplum_Artwork_1",
        "ohvrworkshopexample_vittorio_romeo_example_1",
    ] {
        assert!(a.is_valid_pack_id(id), "missing pack id {id}");
    }
}

#[test]
fn real_tree_headless_load_info_counts() {
    let a = load_headless();
    let li = a.get_load_results();
    // music `.json` (24) + style `.json` (33); no `.ogg`/shaders in headless.
    assert_eq!(li.assets, 57);
    assert_eq!(li.levels, 37);
    // The shipped tree is well-formed: no load errors.
    assert!(li.error_messages.is_empty(), "{:?}", li.error_messages);
}

#[test]
fn real_tree_cube_level_count_is_nine() {
    let a = load_headless();
    let ids = a
        .get_level_ids_by_pack(cube_id())
        .expect("cube pack has levels");
    assert_eq!(ids.len(), 9);
    for id in [
        "apeirogon",
        "commando",
        "euclideanpc",
        "flatteringshape",
        "goldenratio",
        "labyrinth",
        "pi",
        "pointless",
        "seconddimension",
    ] {
        assert!(
            ids.iter().any(|x| x == &format!("{}_{}", cube_id(), id)),
            "missing {id}"
        );
    }
}

#[test]
fn real_tree_apeirogon_level_data_is_exact() {
    let a = load_headless();
    let ld = a
        .get_level_data(&format!("{}_apeirogon", cube_id()))
        .expect("apeirogon level data");

    assert_eq!(ld.pack_path, "Packs/cube/");
    assert_eq!(ld.pack_id, cube_id());
    assert_eq!(ld.id, "apeirogon");
    assert_eq!(ld.name, "apeirogon");
    assert_eq!(ld.description, "hardest level of difficulty");
    assert_eq!(ld.author, "vittorio romeo");
    assert_eq!(ld.menu_priority, 40);
    assert!(ld.selectable);
    assert_eq!(ld.music_id, "milkyWays");
    assert_eq!(ld.sound_id, "nullSoundId");
    assert_eq!(ld.style_id, "apeirogon");
    assert_eq!(
        ld.lua_script_path,
        "Packs/cube/Scripts/Levels/apeirogon.lua"
    );
    // difficultyMults [1.6, 0.35] + 1.0, sorted.
    assert_eq!(ld.difficulty_mults, vec![0.35f32, 1.0f32, 1.6f32]);
    assert!(!ld.unscored);

    // Validators (mapping §7.1 float formatting: 0.35, 1, 1.6).
    assert_eq!(
        ld.get_validator(0.35),
        Some("ohvrvanilla_vittorio_romeo_cube_1_apeirogon_m_0.35")
    );
    assert_eq!(
        ld.get_validator(1.0),
        Some("ohvrvanilla_vittorio_romeo_cube_1_apeirogon_m_1")
    );
    assert_eq!(
        ld.get_validator(1.6),
        Some("ohvrvanilla_vittorio_romeo_cube_1_apeirogon_m_1.6")
    );
    assert_eq!(
        ld.get_validator_without_pack_id(1.6),
        Some("apeirogon_m_1.6")
    );
}

#[test]
fn discovery_order_is_byte_sorted_full_path() {
    let vfs = oh_vfs::RealFs::new(release_dir());
    // `scanSingleFolderName("Packs/")`: byte-sorted, trailing slash.
    assert_eq!(
        pack_scan::scan_single_folder_name(&vfs, "Packs/"),
        vec![
            "Packs/artwork/",
            "Packs/base/",
            "Packs/cube/",
            "Packs/experimental/",
            "Packs/hypercube/",
            "Packs/orthoplex/",
            "Packs/tutorial/",
            "Packs/workshopexample/",
        ]
    );
    // `scanSingleByExt("Packs/cube/Levels/", ".json")`: byte-sorted, no slash.
    assert_eq!(
        pack_scan::scan_single_by_ext(&vfs, "Packs/cube/Levels/", ".json"),
        vec![
            "Packs/cube/Levels/apeirogon.json",
            "Packs/cube/Levels/commando.json",
            "Packs/cube/Levels/euclideanpc.json",
            "Packs/cube/Levels/flatteringshape.json",
            "Packs/cube/Levels/goldenratio.json",
            "Packs/cube/Levels/labyrinth.json",
            "Packs/cube/Levels/pi.json",
            "Packs/cube/Levels/pointless.json",
            "Packs/cube/Levels/seconddimension.json",
        ]
    );
}

#[test]
fn pack_infos_are_priority_ordered() {
    let a = load_headless();
    let infos = a.get_selectable_pack_infos();
    // base has no Levels/, so it is not selectable; the other 7 are.
    assert_eq!(infos.len(), 7);
    // Non-decreasing priority (C++ `std::sort` ascending).
    let mut prev = f32::NEG_INFINITY;
    for info in infos {
        let p = a.get_pack_data(&info.id).unwrap().priority;
        assert!(p >= prev, "priority not non-decreasing at {p}");
        prev = p;
    }
    // base (min priority) is NOT selectable (no levels); the selectable list
    // therefore starts at the next-most-negative.
    assert!(!infos
        .iter()
        .any(|i| i.id == "ohvrvanilla_vittorio_romeo_base_1"));
    // workshopexample (priority 0) is the last selectable pack.
    assert!(infos
        .iter()
        .any(|i| i.id == "ohvrworkshopexample_vittorio_romeo_example_1"));
}

#[test]
fn headless_false_loads_assets_json_and_shaders() {
    let a = Assets::new(Arc::new(RealFs::new(release_dir())), false, false)
        .expect("_RELEASE loads full");
    let li = a.get_load_results();
    // 4 shaders + 24 music json + 33 style json.
    assert_eq!(li.assets, 61);
    assert_eq!(li.levels, 37);

    // assets.json populated the storage (all listed files exist in _RELEASE).
    // Note the ids are the EXACT strings from assets.json (they keep their
    // file extensions: "icon.png", "beep.ogg" — see lib-assets.md).
    assert_eq!(a.get_texture("icon.png"), Some(true));
    assert_eq!(a.get_font("OpenSquare-Regular.ttf"), Some(true));
    assert_eq!(a.get_sound_buffer("beep.ogg"), Some(true));

    // Shader registry (id allocation + lookups).
    assert!(a
        .get_shader_id_by_path("Packs/artwork/Shaders/gradient.frag")
        .is_some());
    assert!(a
        .get_shader_id_by_path("Packs/experimental/Shaders/text.frag")
        .is_some());
    // 4 shaders total (2 frag in artwork, 2 frag in experimental).
    let mut count = 0;
    while a.is_valid_shader_id(count) {
        count += 1;
    }
    assert_eq!(count, 4);
    // A by-name lookup resolves and round-trips through the id.
    let id = a
        .get_shader_id("thing_Synth_Morxemplum_Artwork_1", "gradient.frag")
        .unwrap();
    assert_eq!(
        a.get_shader_by_shader_id(id).unwrap().path,
        "Packs/artwork/Shaders/gradient.frag"
    );
}

#[test]
fn missing_asset_is_a_null_fallback() {
    let a = load_headless();
    // headless did not load assets.json: a bare texture id is a null fallback
    // (the game would still run — it never dereferences a missing asset).
    assert_eq!(
        a.get_texture_or_null("does-not-exist"),
        oh_assets::AssetRef::Null
    );
    assert_eq!(
        a.get_font_or_null("does-not-exist"),
        oh_assets::AssetRef::Null
    );
    // A known level that is present resolves.
    assert!(a.get_level_data(&format!("{}_pi", cube_id())).is_some());
}

// ---------------------------------------------------------------------------
// Synthetic bad packs (LoadInfo error accumulation, task spec verification §4)
// ---------------------------------------------------------------------------

fn tmp_root(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("oh-assets-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

#[test]
fn type_mismatch_level_is_a_fatal_exception_with_exact_message() {
    let root = tmp_root("badlevel");
    std::fs::create_dir_all(root.join("Packs/good/Levels")).unwrap();
    std::fs::write(
        root.join("Packs/good/pack.json"),
        r#"{"disambiguator":"d","name":"good","author":"a","version":1}"#,
    )
    .unwrap();
    // loads fine (byte-sorted first)
    std::fs::write(
        root.join("Packs/good/Levels/a.json"),
        r#"{"id":"a","menuPriority":1}"#,
    )
    .unwrap();
    // a type mismatch: `menuPriority` is a string → C++ `getExtr<int>` throws
    std::fs::write(
        root.join("Packs/good/Levels/b.json"),
        r#"{"id":"b","menuPriority":"nope"}"#,
    )
    .unwrap();

    let res = Assets::new(Arc::new(RealFs::new(&root)), true, false);
    let _ = std::fs::remove_dir_all(&root);

    let fatal = res.expect_err("a type mismatch in a level file is fatal");
    let li = &fatal.load_info;
    // packs discovered (pack.json was valid) before the fatal level load.
    assert_eq!(li.packs, 1);
    // only `a.json` was counted before the abort.
    assert_eq!(li.levels, 1);
    // Exact fatal message (JsonError type-mismatch text is fixed in oh-data).
    assert!(
        li.error_messages.iter().any(|m| m
            == "FATAL ERROR, Exception during asset loading: Value is not convertible to int.\n"),
        "got {:?}",
        li.error_messages
    );
    assert!(
        li.error_messages
            .iter()
            .any(|m| m.starts_with("Error loading pack info 'd_a_good_1")),
        "got {:?}",
        li.error_messages
    );
}

#[test]
fn unparseable_level_accumulates_a_formatted_error_and_null_id_level() {
    let root = tmp_root("badjson");
    std::fs::create_dir_all(root.join("Packs/p/Levels")).unwrap();
    std::fs::write(
        root.join("Packs/p/pack.json"),
        r#"{"disambiguator":"d","name":"p","author":"a","version":1}"#,
    )
    .unwrap();
    // Truncated JSON → parse failure: a formatted error is recorded, and the
    // level still loads with all defaults (id = "nullId").
    std::fs::write(root.join("Packs/p/Levels/bad.json"), r#"{"id":"x","#).unwrap();

    let a =
        Assets::new(Arc::new(RealFs::new(&root)), true, false).expect("parse error is non-fatal");
    let _ = std::fs::remove_dir_all(&root);

    let li = a.get_load_results();
    assert_eq!(li.packs, 1);
    assert_eq!(li.levels, 1);
    assert_eq!(li.error_messages.len(), 1);
    assert!(
        li.error_messages[0].ends_with("in file bad.json"),
        "got {:?}",
        li.error_messages
    );
    // The level loaded under its default null id.
    assert!(a.is_valid_level_id("d_a_p_1_nullId"));
}
