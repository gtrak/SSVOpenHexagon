//! 29i1 — the F3/F4 debug asset reload (C++ `Assets.cpp:1159-1443`
//! `reloadAllShaders` / `reloadPack` / `reloadLevel`): the re-read-from-
//! disk-in-place semantics — a pack reload re-reads that pack's level /
//! style / music / sound definitions (insert-or-replace; a deleted file
//! is NOT removed), a level reload re-reads ONE level's files by name
//! (the exact result strings + the early-return arms), and the reload
//! leaves the load-results state untouched.

use std::fs;
use std::path::{Path, PathBuf};

use oh_assets::Assets;

const PACK: &str = "d_a_p_1"; // build_pack_id("d", "a", "p", 1)

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oh-assets-reload-{}-{}-{tag}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// The fixture pack: `Packs/p` with `pack.json` + `Levels/` + `Styles/`.
/// `levels` is a file-name → JSON-content map; the pack.json shape is the
/// `build_pack_id` triple (disambiguator "d", author "a", name "p",
/// version 1 → pack id `d_a_p_1`).
fn write_fixture(root: &Path, levels: &[(&str, &str)]) {
    let pack = root.join("Packs").join("p");
    fs::create_dir_all(pack.join("Levels")).expect("levels dir");
    fs::create_dir_all(pack.join("Styles")).expect("styles dir");
    fs::write(
        pack.join("pack.json"),
        r#"{"disambiguator":"d","name":"p","author":"a","version":1}"#,
    )
    .expect("pack.json");
    for (name, body) in levels {
        fs::write(pack.join("Levels").join(name), body).expect("level file");
    }
    fs::write(
        pack.join("Styles").join("s1.json"),
        r#"{"id":"s1","colors":[{"value":[255,0,0,255]},{"value":[0,255,0,255]}]}"#,
    )
    .expect("style file");
}

fn load(root: &Path) -> Assets {
    Assets::new(root, true, false).expect("headless load")
}

/// The task's acceptance test: write a level file, load, modify it on disk,
/// reload, observe the new content.
#[test]
fn reload_level_replaces_the_modified_file_on_disk() {
    let root = temp_dir("level-replace");
    write_fixture(
        &root,
        &[(
            "alpha.json",
            r#"{"id":"alpha","name":"Alpha v1","styleId":"s1"}"#,
        )],
    );
    let a = load(&root);
    assert_eq!(
        a.get_level_data(&format!("{PACK}_alpha"))
            .expect("boot load")
            .name,
        "Alpha v1"
    );

    // Modify the level on disk (a new name + a new style) + the style file.
    fs::write(
        root.join("Packs/p/Levels/alpha.json"),
        r#"{"id":"alpha","name":"Alpha v2","styleId":"s2"}"#,
    )
    .expect("modify level");
    fs::write(
        root.join("Packs/p/Styles/s2.json"),
        r#"{"id":"s2","colors":[{"value":[0,0,255,255]},{"value":[255,255,0,255]}]}"#,
    )
    .expect("new style");

    let out = a.reload_level(PACK, "Packs/p/", "alpha");

    // The C++'s exact result lines (the `MenuGame::reloadAssets` dialog
    // source — lower case here; the menu uppercases it).
    assert!(
        out.contains("level data alpha.json successfully loaded\n"),
        "{out:?}"
    );
    assert!(
        out.contains("style data s2.json successfully loaded\n"),
        "{out:?}"
    );

    // The NEW content is live in the store (the C++'s in-place replace).
    let lvl = a
        .get_level_data(&format!("{PACK}_alpha"))
        .expect("the level still exists");
    assert_eq!(lvl.name, "Alpha v2");
    assert_eq!(lvl.style_id, "s2");
    assert!(a.get_style_data(PACK, "s2").is_some(), "the new style data");
    // The OLD style is still there (the C++ never erases on reload).
    assert!(a.get_style_data(PACK, "s1").is_some());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn reload_pack_rereads_all_levels_adds_new_and_keeps_deleted() {
    let root = temp_dir("pack-reread");
    write_fixture(
        &root,
        &[
            (
                "alpha.json",
                r#"{"id":"alpha","name":"Alpha v1","styleId":"s1"}"#,
            ),
            ("beta.json", r#"{"id":"beta","name":"Beta","styleId":"s1"}"#),
        ],
    );
    let a = load(&root);
    let ids = a.get_level_ids_by_pack(PACK).expect("pack levels");
    assert_eq!(ids, vec![format!("{PACK}_alpha"), format!("{PACK}_beta")]);

    // Modify alpha on disk; add gamma; DELETE beta's file.
    fs::write(
        root.join("Packs/p/Levels/alpha.json"),
        r#"{"id":"alpha","name":"Alpha v2","styleId":"s1"}"#,
    )
    .expect("modify alpha");
    fs::write(
        root.join("Packs/p/Levels/gamma.json"),
        r#"{"id":"gamma","name":"Gamma","styleId":"s1"}"#,
    )
    .expect("add gamma");
    fs::remove_file(root.join("Packs/p/Levels/beta.json")).expect("delete beta");

    let out = a.reload_pack(PACK, "Packs/p/");

    // The C++'s five sections, verbatim — the missing Music/ + Sounds/
    // folders are the "invalid … folder path" arms (the fixture has only
    // Levels/ + Styles/).
    assert_eq!(
        out,
        "Levels successfully reloaded\n\
         Styles successfully reloaded\n\
         invalid music data folder path\n\
         invalid music folder path\n\
         invalid custom sound folder path\n"
    );

    // alpha replaced in place (the name moved), gamma APPENDED to the
    // id list (the C++'s `levelDataIdsByPack[mPackId].emplace_back` on a
    // new key), beta STALE (the C++ never erases on reload — the deleted
    // file's entry stays in `levelDatas` + the list).
    assert_eq!(
        a.get_level_data(&format!("{PACK}_alpha")).unwrap().name,
        "Alpha v2"
    );
    assert!(
        a.get_level_data(&format!("{PACK}_gamma")).is_some(),
        "the new level file was re-read"
    );
    assert!(
        a.get_level_data(&format!("{PACK}_beta")).is_some(),
        "the deleted file's entry stays (the C++ never erases on reload)"
    );
    let ids = a.get_level_ids_by_pack(PACK).expect("pack levels");
    assert_eq!(
        ids,
        vec![
            format!("{PACK}_alpha"),
            format!("{PACK}_beta"),
            format!("{PACK}_gamma"),
        ]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn reload_pack_missing_levels_folder_aborts_everything() {
    let root = temp_dir("missing-levels");
    // A pack with NO Levels/ folder (only Styles/).
    let pack = root.join("Packs").join("p");
    fs::create_dir_all(pack.join("Styles")).expect("styles dir");
    fs::write(
        pack.join("pack.json"),
        r#"{"disambiguator":"d","name":"p","author":"a","version":1}"#,
    )
    .expect("pack.json");
    let a = load(&root);
    let out = a.reload_pack(PACK, "Packs/p/");
    // The C++ returns EXACTLY the single line (the whole operation is
    // cancelled — no other section output).
    assert_eq!(out, "invalid level folder path\n");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn reload_level_missing_file_arms_are_the_exact_cpp_strings() {
    let root = temp_dir("missing-arms");
    write_fixture(
        &root,
        &[(
            "alpha.json",
            r#"{"id":"alpha","name":"Alpha","styleId":"s1"}"#,
        )],
    );
    let a = load(&root);

    // (a) No matching level data file.
    assert_eq!(
        a.reload_level(PACK, "Packs/p/", "ghost"),
        "no matching level data file found\n"
    );

    // (b) The style file is missing (the level loads, the style arm
    // fires; no Music/ in the fixture → the music data + file arms; the
    // `nullSoundId` early return ends the output).
    fs::write(
        root.join("Packs/p/Levels/alpha.json"),
        r#"{"id":"alpha","name":"Alpha","styleId":"missing"}"#,
    )
    .expect("repoint style");
    assert_eq!(
        a.reload_level(PACK, "Packs/p/", "alpha"),
        "level data alpha.json successfully loaded\n\
         no matching style file found\n\
         invalid music data folder path\n\
         invalid music folder path\n"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn reload_level_sound_and_music_file_arms() {
    let root = temp_dir("sound-music");
    let pack = root.join("Packs").join("p");
    fs::create_dir_all(pack.join("Levels")).expect("levels");
    fs::create_dir_all(pack.join("Sounds")).expect("sounds");
    fs::create_dir_all(pack.join("Music")).expect("music");
    fs::write(
        pack.join("pack.json"),
        r#"{"disambiguator":"d","name":"p","author":"a","version":1}"#,
    )
    .expect("pack.json");

    // A level with a sound + a music (the C++'s non-null arms).
    fs::write(
        pack.join("Levels").join("alpha.json"),
        r#"{"id":"alpha","name":"Alpha","styleId":"nullStyleId","soundId":"boom","musicId":"m1"}"#,
    )
    .expect("level");
    fs::write(pack.join("Music").join("m1.ogg"), "not-a-real-ogg").expect("music");
    fs::write(
        pack.join("Music").join("m1.json"),
        r#"{"id":"m1","name":"M1"}"#,
    )
    .expect("music data");
    let a = load(&root);

    // (a) The sound file is missing (Sounds/ exists, `boom.ogg` does not).
    let out = a.reload_level(PACK, "Packs/p/", "alpha");
    assert!(
        out.contains("no matching custom sound file found\n"),
        "{out:?}"
    );
    // The music data + music file arms both succeeded (emplace).
    assert!(
        out.contains("music data m1.json successfully loaded\n"),
        "{out:?}"
    );
    assert_eq!(
        a.get_music_path(&format!("{PACK}_m1")).as_deref(),
        Some("Packs/p/Music/m1.ogg")
    );

    // (b) The sound file appears → "new custom sound file … loaded" + the
    // path registered in the storage (first load wins).
    fs::write(pack.join("Sounds").join("boom.ogg"), "not-a-real-ogg").expect("sound");
    let out = a.reload_level(PACK, "Packs/p/", "alpha");
    assert!(
        out.contains("new custom sound file boom.ogg successfully loaded\n"),
        "{out:?}"
    );
    assert!(
        a.get_sound_path(&format!("{PACK}_boom")).is_some(),
        "the sound path is registered (the C++'s `assetStorage->loadSoundBuffer`)"
    );

    // (c) The second reload short-circuits (the C++'s `hasSoundBuffer`):
    // "… is already loaded" — no new-load line.
    let out = a.reload_level(PACK, "Packs/p/", "alpha");
    assert!(
        out.contains("custom sound file boom.ogg is already loaded\n"),
        "{out:?}"
    );
    assert!(!out.contains("new custom sound file"), "{out:?}");

    // (d) `nullSoundId` + `nullMusicId` + `nullStyleId` → the sound
    // section is skipped entirely (the C++'s early `return output`); the
    // missing-Styles arm + the missing-`nullMusicId.json` arm still fire.
    fs::write(
        root.join("Packs/p/Levels/alpha.json"),
        r#"{"id":"alpha","name":"Alpha","styleId":"nullStyleId","soundId":"nullSoundId","musicId":"nullMusicId"}"#,
    )
    .expect("null arms");
    let out = a.reload_level(PACK, "Packs/p/", "alpha");
    assert_eq!(
        out,
        "level data alpha.json successfully loaded\n\
         invalid style folder path\n\
         no matching music data file found\n"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn reload_pack_parse_failure_loads_the_cpp_nullid_default() {
    let root = temp_dir("parse-fail");
    // A VALID alpha + a TRUNCATED bad.json (a parse failure — the C++
    // `ssvuj::getFromFile` logs + yields a Null object → the default-
    // valued `LevelData`, id = "nullId").
    write_fixture(
        &root,
        &[
            (
                "alpha.json",
                r#"{"id":"alpha","name":"Alpha","styleId":"s1"}"#,
            ),
            ("bad.json", r#"{"id":"x","#),
        ],
    );
    let a = load(&root);
    let out = a.reload_pack(PACK, "Packs/p/");
    assert!(out.contains("Levels successfully reloaded\n"), "{out:?}");
    // The parse-failed file became the C++'s default-valued level (the
    // `nullId` key — the same behavior as the boot-time load, pinned by
    // `release_tree.rs`'s truncated-JSON test).
    assert!(
        a.get_level_data(&format!("{PACK}_nullId")).is_some(),
        "the C++'s Null-object → default `LevelData` (id = nullId)"
    );
    assert_eq!(
        a.get_level_data(&format!("{PACK}_nullId")).unwrap().name,
        "nullName"
    );
    let ids = a.get_level_ids_by_pack(PACK).expect("ids");
    assert!(ids.contains(&format!("{PACK}_nullId")));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn reload_all_shaders_leaves_the_registry_untouched() {
    let root = temp_dir("shaders");
    // Non-headless: `Assets/` + `assets.json` are required (the C++'s
    // (1) step); the fixture's empty assets.json loads nothing.
    fs::create_dir_all(root.join("Assets")).expect("assets dir");
    fs::write(root.join("Assets").join("assets.json"), "{}").expect("assets.json");
    let pack = root.join("Packs").join("p");
    fs::create_dir_all(pack.join("Levels")).expect("levels");
    fs::create_dir_all(pack.join("Shaders")).expect("shaders");
    fs::write(
        pack.join("pack.json"),
        r#"{"disambiguator":"d","name":"p","author":"a","version":1}"#,
    )
    .expect("pack.json");
    fs::write(
        pack.join("Levels").join("alpha.json"),
        r#"{"id":"alpha","name":"Alpha","styleId":"nullStyleId"}"#,
    )
    .expect("level");
    fs::write(pack.join("Shaders").join("a.frag"), "// frag").expect("frag");
    fs::write(pack.join("Shaders").join("b.vert"), "// vert").expect("vert");

    let a = Assets::new(&root, false, false).expect("non-headless load");
    let frag_id = a
        .get_shader_id_by_path("Packs/p/Shaders/a.frag")
        .expect("the frag is registered");
    let vert_id = a
        .get_shader_id_by_path("Packs/p/Shaders/b.vert")
        .expect("the vert is registered");

    // The C++'s `loadFromFile` re-read stands in as the file-existence
    // re-check: the registry (ids / lookups) is untouched by the reload.
    a.reload_all_shaders();
    assert_eq!(
        a.get_shader_id_by_path("Packs/p/Shaders/a.frag"),
        Some(frag_id)
    );
    assert_eq!(
        a.get_shader_id_by_path("Packs/p/Shaders/b.vert"),
        Some(vert_id)
    );
    assert_eq!(a.get_shader(PACK, "a.frag").map(|s| s.id), Some(frag_id));

    // A deleted source fails the re-read (the C++ logs + `continue` —
    // the registry entry remains; only the log is observable, so the
    // assertion is the registry's invariance).
    fs::remove_file(pack.join("Shaders").join("a.frag")).expect("delete frag");
    a.reload_all_shaders();
    assert_eq!(
        a.get_shader_id_by_path("Packs/p/Shaders/a.frag"),
        Some(frag_id)
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn reload_pack_replaces_style_and_music_data_in_place() {
    let root = temp_dir("style-music-data");
    let pack = root.join("Packs").join("p");
    fs::create_dir_all(pack.join("Levels")).expect("levels");
    fs::create_dir_all(pack.join("Styles")).expect("styles");
    fs::create_dir_all(pack.join("Music")).expect("music");
    fs::write(
        pack.join("pack.json"),
        r#"{"disambiguator":"d","name":"p","author":"a","version":1}"#,
    )
    .expect("pack.json");
    fs::write(
        pack.join("Levels").join("alpha.json"),
        r#"{"id":"alpha","name":"Alpha","styleId":"s1","musicId":"m1"}"#,
    )
    .expect("level");
    fs::write(
        pack.join("Styles").join("s1.json"),
        r#"{"id":"s1","colors":[{"value":[1,2,3,4]},{"value":[5,6,7,8]}]}"#,
    )
    .expect("style");
    fs::write(
        pack.join("Music").join("m1.json"),
        r#"{"id":"m1","name":"M1 v1"}"#,
    )
    .expect("music data");
    fs::write(pack.join("Music").join("m1.ogg"), "ogg").expect("music file");
    let a = load(&root);
    assert_eq!(
        a.get_music_data(PACK, "m1").expect("boot music").name,
        "M1 v1"
    );

    // Modify both data files on disk.
    fs::write(
        root.join("Packs/p/Styles/s1.json"),
        r#"{"id":"s1","colors":[{"value":[9,9,9,9]},{"value":[8,8,8,8]}]}"#,
    )
    .expect("modify style");
    fs::write(
        root.join("Packs/p/Music/m1.json"),
        r#"{"id":"m1","name":"M1 v2"}"#,
    )
    .expect("modify music");

    let out = a.reload_pack(PACK, "Packs/p/");
    assert!(out.contains("Styles successfully reloaded\n"), "{out:?}");
    assert!(
        out.contains("Music data successfully reloaded\n"),
        "{out:?}"
    );
    assert!(
        out.contains("Music files successfully reloaded\n"),
        "{out:?}"
    );

    // Replaced in place (the C++'s `styleDataMap[key] =` /
    // `musicDataMap[key] =`).
    assert_eq!(a.get_music_data(PACK, "m1").unwrap().name, "M1 v2");
    let style = a.get_style_data(PACK, "s1").expect("the style");
    let mut style = style;
    style.compute_colors();
    assert_eq!(
        style.get_colors()[0],
        oh_math::hg::color::Color::new(9, 9, 9, 9)
    );
    // The music `.ogg` is registered in `musicPathMap` (the headless boot
    // skips the `.ogg` map — the C++'s `!headless` gate; the reload has
    // no such gate, as in the C++).
    assert_eq!(
        a.get_music_path(&format!("{PACK}_m1")).as_deref(),
        Some("Packs/p/Music/m1.ogg")
    );
    let _ = fs::remove_dir_all(&root);
}
