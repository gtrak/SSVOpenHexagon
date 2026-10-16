//! Discrimination test (issue-026a acceptance #4): build a small in-memory
//! `_RELEASE`-shaped tree and drive BOTH `oh_assets::Assets` and
//! `oh_config::Config` end-to-end through a `MemoryVfs` — **with zero
//! `std::fs` access in this test body** — proving the VFS routing works
//! off-disk.
//!
//! `MemoryVfs` is purely in-memory, so a stray disk dependency (a temp file,
//! a `_RELEASE/` read) would fail this test: the acceptance criterion is that
//! the counts/paths asserted here are produced entirely from the in-memory
//! maps. This test deliberately references no `std::fs` at all (see the
//! acceptance grep).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use oh_assets::pack_scan;
use oh_vfs::MemoryVfs;

/// The pack id `build_pack_id("mem", "tester", "mempack", 1)` produces
/// (spaces -> underscores, joined with `_`, then the int version).
const PACK_ID: &str = "mem_tester_mempack_1";

/// Build a minimal in-memory `_RELEASE`-shaped tree:
///
/// ```text
/// config.json                          -> {"max_fps": 100}
/// ConfigOverrides/
///   testoverride.json                 -> {"max_fps": 999}
/// Packs/
///   mempack/
///     pack.json
///     Levels/
///       alpha.json
///       beta.json
/// ```
///
/// The pack/level JSON shapes mirror `_RELEASE/Packs/cube/` exactly
/// (the real `pack.json` and `Levels/apeirogon.json` field sets); the
/// config/override shapes are the minimal `config.json` +
/// `ConfigOverrides/<id>.json` objects `Config::load_config` shallow-merges.
fn memory_tree() -> (BTreeMap<String, Vec<u8>>, BTreeSet<String>) {
    let mut files = BTreeMap::new();

    files.insert("config.json".to_string(), b"{\"max_fps\":100}".to_vec());
    files.insert(
        "ConfigOverrides/testoverride.json".to_string(),
        b"{\"max_fps\":999}".to_vec(),
    );

    // Mirror the real `_RELEASE/Packs/cube/pack.json` field set.
    files.insert(
        "Packs/mempack/pack.json".to_string(),
        br#"{
            "disambiguator": "mem",
            "name": "mempack",
            "author": "tester",
            "version": 1,
            "priority": -99999.02
        }"#
        .to_vec(),
    );

    // Mirror the real `_RELEASE/Packs/cube/Levels/apeirogon.json` field set.
    files.insert(
        "Packs/mempack/Levels/alpha.json".to_string(),
        br#"{
            "id": "alpha",
            "name": "alpha",
            "description": "alpha level",
            "author": "tester",
            "menuPriority": 1,
            "selectable": true,
            "styleId": "alpha",
            "musicId": "m",
            "luaFile": "Scripts/Levels/alpha.lua",
            "difficultyMults": [1.0]
        }"#
        .to_vec(),
    );
    files.insert(
        "Packs/mempack/Levels/beta.json".to_string(),
        br#"{
            "id": "beta",
            "name": "beta",
            "description": "beta level",
            "author": "tester",
            "menuPriority": 2,
            "selectable": true,
            "styleId": "beta",
            "musicId": "m",
            "luaFile": "Scripts/Levels/beta.lua",
            "difficultyMults": [1.0]
        }"#
        .to_vec(),
    );

    // Directory keys (no trailing slash — the MemoryVfs key convention).
    let mut dirs = BTreeSet::new();
    dirs.insert("Packs".to_string());
    dirs.insert("Packs/mempack".to_string());
    dirs.insert("Packs/mempack/Levels".to_string());
    dirs.insert("ConfigOverrides".to_string());

    (files, dirs)
}

#[test]
fn memory_vfs_drives_assets_and_config_off_disk() {
    let (files, dirs) = memory_tree();
    let vfs = MemoryVfs::new(files, dirs);

    // ---- oh-assets: the full discovery/loading pipeline, off-disk ----
    // `headless = true` skips the SFML-backed sub-loaders (assets.json,
    // shaders, custom sounds, music `.ogg`) that the render/audio backend
    // owns (issue 10), so this asserts the pure data-side pipeline — the
    // same reduced invariants the `RealFs` `_RELEASE` tests assert.
    let assets = oh_assets::Assets::new(Arc::new(vfs.clone()), true, false)
        .expect("in-memory _RELEASE-shaped tree loads headless");

    let li = assets.get_load_results();
    assert_eq!(li.packs, 1, "exactly one pack discovered");
    assert_eq!(li.levels, 2, "both levels discovered");
    assert!(li.error_messages.is_empty(), "{:?}", li.error_messages);

    // The pack id was built (`buildPackId`) and is valid.
    assert!(assets.is_valid_pack_id(PACK_ID));

    // The level count for the pack (both level ids present).
    let ids = assets
        .get_level_ids_by_pack(PACK_ID)
        .expect("mempack pack has levels");
    assert_eq!(ids.len(), 2);
    for id in ["alpha", "beta"] {
        assert!(
            ids.iter().any(|x| x == &format!("{PACK_ID}_{id}")),
            "missing {id}"
        );
    }

    // The byte-sorted discovery order for the pack's `Levels/*.json`
    // (the C++ `std::sort` on the normalized full logical path — the
    // discovery-order rule in `.agents/skills/lib-assets.md`).
    assert_eq!(
        pack_scan::scan_single_by_ext(&vfs, "Packs/mempack/Levels/", ".json"),
        vec![
            "Packs/mempack/Levels/alpha.json",
            "Packs/mempack/Levels/beta.json",
        ]
    );

    // ---- oh-config: `load_config`, off-disk ----
    // The override's distinctive key is applied: config.json sets
    // `max_fps` = 100, `testoverride.json` overrides it to 999.
    let mut cfg = oh_config::Config::default();
    cfg.load_config(&vfs, &["testoverride"])
        .expect("in-memory config + override loads");
    assert_eq!(cfg.get_max_fps(), 999, "override key applied");

    // A non-existent override id is ignored: `max_fps` stays the
    // `config.json` value (100), not the override's 999.
    let mut cfg2 = oh_config::Config::default();
    cfg2.load_config(&vfs, &["doesnotexist"])
        .expect("in-memory config loads with an unknown override id");
    assert_eq!(cfg2.get_max_fps(), 100, "unknown override id ignored");
}
