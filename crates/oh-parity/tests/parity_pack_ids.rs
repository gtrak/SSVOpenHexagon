//! Issue 23 — pack-id / level-validator parity. Runs the corpus-pinned
//! `buildPackId` / `getLevelValidator` strings (`.agents/plans/001-rust-port/
//! parity/pack_ids.json`, C++-probe-derived: the REAL
//! `src/SSVOpenHexagon/Utils/BuildPackId.cpp` + `LevelValidator.cpp` via
//! `tools/oracle/parity_probe.cpp`) through the Rust `oh-data` ports and
//! compares exactly. The ids are the keys everything else hangs off (level
//! ids, validators, score keys), so a formatting drift here breaks replay
//! validation and online score lookups.
//!
//! A third leg of the same tie: the corpus ids are re-derived from the
//! SHIPPED `_RELEASE/Packs/<folder>/pack.json` files through the Rust
//! comment-tolerant JSON loader (`oh_data::json::parse` — the workshopexample
//! pack.json carries comments) + `oh_data::build_pack_id`, so the C++
//! probe, the Rust ports, and the shipped data all agree.
//!
//! The harness itself is tested: `deliberately_wrong_pin_discriminates`
//! runs the same comparison on a corrupted pin and requires the mismatch.

use std::path::{Path, PathBuf};

use oh_data::{build_pack_id, get_level_validator};
use serde_json::Value;

fn corpus_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.agents/plans/001-rust-port/parity")
        .join(name)
}

fn load(name: &str) -> Value {
    let text = std::fs::read_to_string(corpus_path(name))
        .unwrap_or_else(|e| panic!("read corpus {name}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse corpus {name}: {e}"))
}

/// The repo root (`_RELEASE` lives here).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/oh-parity is two levels deep")
        .to_path_buf()
}

/// Compares one corpus pack entry against the Rust ports. Returns `Ok(())`
/// or the first mismatch (the deliberately-wrong-entry test relies on the
/// `Err` arm).
fn check_pack(entry: &Value) -> Result<(), String> {
    let folder = entry["folder"].as_str().unwrap();
    let disambiguator = entry["disambiguator"].as_str().unwrap();
    let author = entry["author"].as_str().unwrap();
    let name = entry["name"].as_str().unwrap();
    let version = entry["version"].as_i64().unwrap() as i32;

    let rust_id = build_pack_id(disambiguator, author, name, version);
    let pinned_id = entry["pack_id"].as_str().unwrap();
    if rust_id != pinned_id {
        return Err(format!(
            "pack {folder}: build_pack_id = {rust_id}, corpus {pinned_id}"
        ));
    }

    let level_id = entry["validator_level_id"].as_str().unwrap();
    let diff = entry["validator_diff_mult"].as_f64().unwrap() as f32;
    let rust_validator = get_level_validator(level_id, diff);
    let pinned_validator = entry["validator"].as_str().unwrap();
    if rust_validator != pinned_validator {
        return Err(format!(
            "pack {folder}: get_level_validator = {rust_validator}, corpus {pinned_validator}"
        ));
    }
    Ok(())
}

#[test]
fn pack_ids_match_cpp_probe() {
    let doc = load("pack_ids.json");
    let packs = doc["packs"].as_array().expect("corpus `packs` array");
    assert_eq!(
        packs.len(),
        8,
        "the corpus pins all 8 shipped _RELEASE packs"
    );
    for entry in packs {
        check_pack(entry).unwrap_or_else(|e| panic!("pack-id mismatch: {e}"));
    }
}

#[test]
fn corpus_ids_rederive_from_shipped_pack_jsons() {
    // The third leg: corpus id == build_pack_id of the SHIPPED pack.json
    // fields (parsed through the Rust comment-tolerant loader — the
    // workshopexample pack.json is comment-laden, exactly what the C++
    // comment-tolerant JsonCpp-style reader handles).
    let doc = load("pack_ids.json");
    for entry in doc["packs"].as_array().unwrap() {
        let folder = entry["folder"].as_str().unwrap();
        let path = repo_root()
            .join("_RELEASE")
            .join("Packs")
            .join(folder)
            .join("pack.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let root =
            oh_data::json::parse(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let disambiguator =
            oh_data::json::extr_str(&root, "disambiguator", "missing").expect("disambiguator");
        let author = oh_data::json::extr_str(&root, "author", "missing").expect("author");
        let name = oh_data::json::extr_str(&root, "name", "missing").expect("name");
        let version = oh_data::json::extr_int(&root, "version", 0).expect("version") as i32;
        assert_eq!(
            (
                disambiguator.as_str(),
                author.as_str(),
                name.as_str(),
                version
            ),
            (
                entry["disambiguator"].as_str().unwrap(),
                entry["author"].as_str().unwrap(),
                entry["name"].as_str().unwrap(),
                entry["version"].as_i64().unwrap() as i32,
            ),
            "pack {folder}: corpus fields drifted from the shipped pack.json"
        );
        assert_eq!(
            build_pack_id(&disambiguator, &author, &name, version),
            entry["pack_id"].as_str().unwrap(),
            "pack {folder}: shipped pack.json + Rust build_pack_id must yield the corpus id"
        );
    }
}

#[test]
fn deliberately_wrong_pin_discriminates() {
    // The contract's "the harness itself is tested": the SAME comparison
    // logic must FAIL on a deliberately corrupted corpus entry.
    let doc = load("pack_ids.json");
    let mut entry = doc["packs"][2].clone();
    // Corrupt the pinned pack id (flip the last character).
    let id = entry["pack_id"].as_str().unwrap();
    let mut chars: Vec<char> = id.chars().collect();
    let last = chars.pop().expect("non-empty id");
    chars.push(if last == '1' { '2' } else { '1' });
    entry["pack_id"] = Value::String(chars.into_iter().collect());
    let err = check_pack(&entry).expect_err("a corrupted pin must not pass");
    assert!(
        err.contains("build_pack_id"),
        "the mismatch names the surface: {err}"
    );

    // And an intact entry passes the same logic (sanity).
    check_pack(&doc["packs"][2]).expect("uncorrupted entry passes");
}
