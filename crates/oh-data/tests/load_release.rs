//! Integration test: parse the REAL `_RELEASE` files and assert exact
//! field values (mapping §9; task spec verification §2).

use std::path::{Path, PathBuf};

use oh_data::json::{self, read_file_with_errors, Obj};
use oh_data::{build_pack_id, load_music_from_json, pack_data_from_json, LevelData, StyleData};

fn release_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../_RELEASE")
        .to_path_buf()
}

fn load_release_json(file: &std::path::Path) -> Obj {
    let (obj, err) = read_file_with_errors(file);
    assert!(
        err.is_empty(),
        "parse failure for {}: {err}",
        file.display()
    );
    obj
}

#[test]
fn all_pack_jsons_parse_with_exact_values() {
    let packs = release_dir().join("Packs");
    let mut count = 0;
    let expected = [
        // (folder, disambiguator, name, author, version, priority, deps)
        (
            "artwork",
            "thing",
            "Artwork",
            "Synth Morxemplum",
            1,
            -99999.02,
            1,
        ),
        (
            "base",
            "ohvrvanilla",
            "base",
            "vittorio romeo",
            1,
            -99999.04,
            0,
        ),
        (
            "cube",
            "ohvrvanilla",
            "cube",
            "vittorio romeo",
            1,
            -99999.02,
            1,
        ),
        (
            "experimental",
            "ohvrvanilla",
            "experimental",
            "vittorio romeo",
            1,
            -2.0,
            1,
        ),
        (
            "hypercube",
            "ohvrvanilla",
            "hypercube",
            "vittorio romeo",
            1,
            -99999.01,
            1,
        ),
        (
            "orthoplex",
            "ohvrvanilla",
            "orthoplex",
            "vittorio romeo",
            1,
            -99999.0,
            1,
        ),
        (
            "tutorial",
            "ohvrvanilla",
            "tutorial",
            "vittorio romeo",
            1,
            -99999.03,
            1,
        ),
        (
            "workshopexample",
            "ohvrworkshopexample",
            "example",
            "vittorio romeo",
            1,
            0.0,
            1,
        ),
    ];

    for (folder, disambiguator, name, author, version, priority, deps) in expected {
        let path = packs.join(folder).join("pack.json");
        let obj = load_release_json(&path);
        let pack_id = build_pack_id(disambiguator, author, name, version);
        let pd = pack_data_from_json(&obj, &format!("Packs/{folder}/"), &pack_id).unwrap();
        assert_eq!(pd.disambiguator, disambiguator);
        assert_eq!(pd.name, name);
        assert_eq!(pd.author, author);
        assert_eq!(pd.version, version);
        assert_eq!(pd.priority, priority);
        assert_eq!(pd.dependencies.len(), deps);
        assert_eq!(pd.folder_path, format!("Packs/{folder}/"));
        assert_eq!(pd.id, pack_id);
        count += 1;
    }
    assert_eq!(count, 8);

    // spot-check dependency content (base dep of cube)
    let obj = load_release_json(&packs.join("cube/pack.json"));
    let pd = pack_data_from_json(&obj, "Packs/cube/", "ohvrvanilla_vittorio_romeo_cube_1").unwrap();
    assert_eq!(pd.dependencies[0].disambiguator, "ohvrvanilla");
    assert_eq!(pd.dependencies[0].name, "base");
    assert_eq!(pd.dependencies[0].author, "vittorio romeo");
    assert_eq!(pd.dependencies[0].min_version, 1);
    // base pack.json has NO "dependencies" key → empty vector.
    let base = load_release_json(&packs.join("base/pack.json"));
    let pb =
        pack_data_from_json(&base, "Packs/base/", "ohvrvanilla_vittorio_romeo_base_1").unwrap();
    assert!(pb.dependencies.is_empty());
    assert_eq!(pb.description, "no description"); // default, key absent
}

#[test]
fn cube_apeirogon_level_has_exact_values() {
    let obj = load_release_json(&release_dir().join("Packs/cube/Levels/apeirogon.json"));
    let ld =
        LevelData::from_json(&obj, "Packs/cube/", "ohvrvanilla_vittorio_romeo_cube_1").unwrap();
    assert_eq!(ld.id, "apeirogon");
    assert_eq!(ld.name, "apeirogon");
    assert_eq!(ld.description, "hardest level of difficulty");
    assert_eq!(ld.author, "vittorio romeo");
    assert_eq!(ld.menu_priority, 40);
    assert!(ld.selectable);
    assert_eq!(ld.music_id, "milkyWays");
    assert_eq!(ld.sound_id, "nullSoundId"); // default (key absent)
    assert_eq!(ld.style_id, "apeirogon");
    assert_eq!(
        ld.lua_script_path,
        "Packs/cube/Scripts/Levels/apeirogon.lua"
    );
    // 1.0 is appended and the vector sorted:
    assert_eq!(ld.difficulty_mults, vec![0.35f32, 1.0f32, 1.6f32]);
    assert!(!ld.unscored);
    assert_eq!(
        ld.get_validator(0.35),
        Some("ohvrvanilla_vittorio_romeo_cube_1_apeirogon_m_0.35")
    );
    assert_eq!(
        ld.get_validator(1.6),
        Some("ohvrvanilla_vittorio_romeo_cube_1_apeirogon_m_1.6")
    );
    assert_eq!(
        ld.get_validator_without_pack_id(0.35),
        Some("apeirogon_m_0.35")
    );
    // matches the shipped config.json whitelist (checked below).
}

#[test]
fn cube_pi_level_values() {
    let obj = load_release_json(&release_dir().join("Packs/cube/Levels/pi.json"));
    let ld =
        LevelData::from_json(&obj, "Packs/cube/", "ohvrvanilla_vittorio_romeo_cube_1").unwrap();
    assert_eq!(ld.id, "pi");
    assert_eq!(
        ld.name,
        "3.141592653589793238462643383279502884197169399375105820974944592307816406286"
    );
    assert_eq!(ld.description, "do you like pi?");
    assert_eq!(ld.menu_priority, 70);
    assert_eq!(ld.style_id, "pi");
    assert_eq!(ld.music_id, "callMeKatla");
    assert_eq!(ld.difficulty_mults, vec![0.5f32, 1.0f32, 1.8f32]);
}

#[test]
fn validator_ids_match_shipped_config_whitelist() {
    let obj = load_release_json(&release_dir().join("config.json"));
    let whitelist =
        json::extr_vec_str(json::get_missing_as_null(&obj, "server_level_whitelist")).unwrap();
    assert!(!whitelist.is_empty());

    // Every non-unscored BUNDLED-CUBE validator id must appear in the
    // shipped config.json whitelist — this pins the iostream
    // 6-significant-digit float formatting (§7.1): `1.0` → "1", 0.5 →
    // "0.5", 1.8 → "1.8", 0.35 → "0.35", 1.6 → "1.6".
    for file in std::fs::read_dir(release_dir().join("Packs/cube/Levels")).unwrap() {
        let path = file.unwrap().path();
        if !path.extension().is_some_and(|e| e == "json") {
            continue;
        }
        let obj = load_release_json(&path);
        let ld =
            LevelData::from_json(&obj, "Packs/cube/", "ohvrvanilla_vittorio_romeo_cube_1").unwrap();
        if ld.unscored {
            continue;
        }
        for dm in &ld.difficulty_mults {
            let validator = ld.get_validator(*dm).expect("validator for known mult");
            assert!(
                whitelist.iter().any(|w| w == validator),
                "validator {validator} missing from server_level_whitelist"
            );
        }
    }
}

#[test]
fn every_release_level_style_and_music_parses() {
    let packs_dir = release_dir().join("Packs");
    let mut levels = 0usize;
    let mut styles = 0usize;
    let mut music = 0usize;

    for pack in std::fs::read_dir(&packs_dir).unwrap() {
        let pack = pack.unwrap().path();
        if !pack.is_dir() {
            continue;
        }
        // real pack id (mapping §7.2):
        let pack_obj = load_release_json(&pack.join("pack.json"));
        let real_id = build_pack_id(
            &json::extr_str(&pack_obj, "disambiguator", "no disambiguator").unwrap(),
            &json::extr_str(&pack_obj, "author", "unknown author").unwrap(),
            &json::extr_str(&pack_obj, "name", "unknown name").unwrap(),
            json::extr_int(&pack_obj, "version", 0).unwrap() as i32,
        );
        let full_pack_path = format!("Packs/{}/", pack.file_name().unwrap().to_string_lossy());

        if let Ok(dir) = std::fs::read_dir(pack.join("Levels")) {
            for entry in dir {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|e| e == "json") {
                    let obj = load_release_json(&path);
                    let ld = LevelData::from_json(&obj, &full_pack_path, &real_id).unwrap();
                    assert!(!ld.difficulty_mults.is_empty());
                    levels += 1;
                }
            }
        }
        if let Ok(dir) = std::fs::read_dir(pack.join("Styles")) {
            for entry in dir {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|e| e == "json") {
                    let obj = load_release_json(&path);
                    let _sd = StyleData::from_json(&obj).unwrap();
                    styles += 1;
                }
            }
        }
        if let Ok(dir) = std::fs::read_dir(pack.join("Music")) {
            for entry in dir {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|e| e == "json") {
                    let obj = load_release_json(&path);
                    let md = load_music_from_json(&obj).unwrap();
                    assert!(!md.id.is_empty());
                    music += 1;
                }
            }
        }
    }

    // Counts cross-checked against `find _RELEASE/Packs -name '*.json'`:
    // Levels 37, Styles 33, Music 24.
    assert_eq!(levels, 37);
    assert_eq!(styles, 33);
    assert_eq!(music, 24);
}

#[test]
fn cube_callme_katla_music_segments_are_exact() {
    let obj = load_release_json(&release_dir().join("Packs/cube/Music/callMeKatla.json"));
    let md = load_music_from_json(&obj).unwrap();
    assert_eq!(md.id, "callMeKatla");
    assert_eq!(md.file_name, "callMeKatla.ogg");
    assert_eq!(md.name, "Call me Katla, Baby");
    assert_eq!(md.album, "");
    assert_eq!(md.author, "Dunderpatrullen");
    let segs = md.segments().to_vec();
    assert_eq!(
        segs,
        vec![
            oh_data::Segment::new(23.411, 0.0),
            oh_data::Segment::new(72.866, 0.0),
            oh_data::Segment::new(139.775, 0.0),
        ]
    );
    assert!(md.first_play);
}

#[test]
fn assets_json_has_exact_lists() {
    let obj = load_release_json(&release_dir().join("Assets/assets.json"));
    let fonts = json::extr_vec_str(json::get_missing_as_null(&obj, "fonts")).unwrap();
    let textures = json::extr_vec_str(json::get_missing_as_null(&obj, "textures")).unwrap();
    let sounds = json::extr_vec_str(json::get_missing_as_null(&obj, "soundBuffers")).unwrap();
    assert_eq!(fonts, vec!["OpenSquare-Regular.ttf", "OpenSquare-Bold.ttf"]);
    assert_eq!(textures.len(), 17);
    assert_eq!(textures[0], "bottomBar.png");
    assert_eq!(textures[16], "titleBar.png");
    assert_eq!(sounds.len(), 16);
    assert_eq!(sounds[0], "beep.ogg");
    assert_eq!(sounds[15], "swapBlip.ogg");
}

#[test]
fn shipped_config_json_parses_and_holds_f32_as_f64() {
    let obj = load_release_json(&release_dir().join("config.json"));
    assert!(json::is_obj(&obj));
    // "player_speed" : 9.449999809265137 — the f32 9.45f archived as double
    // (`%#.16g`). Assert the EXACT double the file holds.
    let speed = json::as_float(json::get_missing_as_null(&obj, "player_speed")).unwrap();
    assert_eq!(speed, 9.449999809265137f64);
    let official = json::as_bool(json::get_missing_as_null(&obj, "official")).unwrap();
    assert!(official);
    let max_depth = json::as_int(json::get_missing_as_null(&obj, "3D_max_depth")).unwrap();
    assert_eq!(max_depth, 100);
}

#[test]
fn comment_full_style_file_parses_with_values() {
    // Packs/artwork/Styles/pointless.json is full of `//` comments.
    let obj = load_release_json(&release_dir().join("Packs/artwork/Styles/pointless.json"));
    let sd = StyleData::from_json(&obj).unwrap();
    assert_eq!(sd.id, "pointless");
    assert_eq!(sd.hue_increment, 0.7f32);
    assert_eq!(sd.pulse_max, 1.0f32);
    assert_eq!(sd.pulse_increment, 0.0125f32);
    assert_eq!(sd._3d_depth, 8.0f32);
    assert_eq!(sd._3d_skew, 0.15f32);
    assert_eq!(sd._3d_darken_mult, 2.0f32);
    assert_eq!(sd.get_color_datas().len(), 2);
    assert_eq!(sd.get_color_datas()[0].dynamic_darkness, 2.7f32);
    assert_eq!(
        sd.get_color_datas()[1].color,
        oh_math::hg::color::Color::new(32, 32, 32, 255)
    );
    match sd.get_cap_color() {
        oh_data::CapColor::ColorData(cd) => {
            assert!(!cd.main);
            assert_eq!(cd.color, oh_math::hg::color::Color::new(0, 0, 0, 255));
        }
        other => panic!("expected non-legacy cap color, got {other:?}"),
    }
    // text_color pulse has a NEGATIVE component ([0, 198, -66, 0]) — int, not
    // u8.
    assert_eq!(sd.get_text_color_data().pulse.b, -66);
}
