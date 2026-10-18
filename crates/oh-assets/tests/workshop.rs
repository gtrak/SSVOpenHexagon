//! 29p1 — the workshop-pack discovery (C++ `Assets.cpp:716-764`'s
//! `steamManager != nullptr` half + `Assets.cpp:662-710`'s
//! `loadWorkshopPackDatasFromCache`): the C++'s precedence, mirrored —
//! the cache is read ONLY when a steam manager exists AND is
//! UNINITIALIZED (the Steam-down case the cache exists for); the live
//! enumeration fully REPLACES the cache when Steam is up; the local
//! `Packs/` scan runs first, and `packDatas` is FIRST-WINS (a cached
//! pack whose `pack.json` matches an already-loaded pack keeps the
//! first entry).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use oh_assets::{Assets, WorkshopPacks};

/// The fake steam manager (the C++'s `steam_manager*`'s `WorkshopPacks`
/// surface): `initialized` + the installed folders.
#[derive(Clone)]
struct FakeWorkshop {
    initialized: bool,
    folders: Vec<String>,
}

impl WorkshopPacks for FakeWorkshop {
    fn is_initialized(&self) -> bool {
        self.initialized
    }
    fn workshop_pack_folders(&self) -> Vec<String> {
        self.folders.clone()
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oh-assets-workshop-{}-{}-{tag}",
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

/// A minimal pack at `root/<rel_dir>` (a `pack.json` with the
/// `build_pack_id` triple: disambiguator "w", author "a", name `<name>`,
/// version 1 → pack id `w_a_<name>_1`).
fn write_pack(root: &Path, rel_dir: &str, name: &str) {
    let dir = root.join(rel_dir);
    fs::create_dir_all(&dir).expect("pack dir");
    fs::write(
        dir.join("pack.json"),
        format!(r#"{{"disambiguator":"w","name":"{name}","author":"a","version":1}}"#),
    )
    .expect("pack.json");
}

/// The local `Packs/local` pack (the `Packs/` scan's always-first entry).
fn local_pack(root: &Path) {
    write_pack(root, "Packs/local", "local");
}

fn write_cache(root: &Path, cached_packs: &str) {
    fs::write(
        root.join("workshopCache.json"),
        format!(r#"{{"cachedPacks":{cached_packs}}}"#),
    )
    .expect("cache file");
}

fn pack_ids(a: &Assets) -> Vec<String> {
    let mut ids: Vec<String> = a.get_pack_datas().keys().map(|k| k.to_string()).collect();
    ids.sort();
    ids
}

const LOCAL_ID: &str = "w_a_local_1";

/// Steam UP: the live enumeration loads the manager's folders and the
/// cache is NOT consulted — the cache's bogus entry (a real pack that
/// must NOT appear) is ignored.
#[test]
fn live_enumeration_replaces_the_cache() {
    let root = temp_dir("live-replaces-cache");
    local_pack(&root);
    write_pack(&root, "Workshop/live", "live");
    // The cache points at a REAL pack — if it were consulted, "stale"
    // would load too (the live half ignores the cache entirely).
    write_pack(&root, "Workshop/stale", "stale");
    write_cache(&root, r#"["Workshop/stale"]"#);

    let a = Assets::new_with_workshop(
        &root,
        true,
        false,
        Some(Arc::new(FakeWorkshop {
            initialized: true,
            folders: vec!["Workshop/live".to_string()],
        })),
    )
    .expect("load");

    assert_eq!(
        pack_ids(&a),
        vec!["w_a_live_1", LOCAL_ID],
        "local + the live folder; the cache's entry is not consulted"
    );
    assert_eq!(a.get_pack_datas().len(), 2);
}

/// Steam DOWN: the `workshopCache.json` fallback loads the cached
/// folders — and the manager's (unused) live folders are not read
/// (the bogus `Workshop/wrong` entry must leave no error line). The
/// first-wins half: the cache's duplicate of the local pack's `pack.json`
/// (the same pack id) keeps the LOCAL entry (the `Packs/` scan ran
/// first; the C++'s `packDatas.emplace` is first-wins).
#[test]
fn cache_fallback_when_steam_is_down() {
    let root = temp_dir("cache-fallback");
    local_pack(&root);
    write_pack(&root, "Workshop/one", "one");
    write_pack(&root, "Workshop/two", "two");
    write_cache(&root, r#"["Workshop/one", "Workshop/two", "Packs/local"]"#);

    let a = Assets::new_with_workshop(
        &root,
        true,
        false,
        Some(Arc::new(FakeWorkshop {
            initialized: false,
            folders: vec!["Workshop/wrong".to_string()],
        })),
    )
    .expect("load");

    assert_eq!(
        pack_ids(&a),
        vec![LOCAL_ID, "w_a_one_1", "w_a_two_1"],
        "local + the two cached folders; the duplicate keeps the local \
         entry (first-wins); the manager's live folder is unused"
    );
    // First-wins proof: the local entry's folder path (the cached
    // `Packs/local` duplicate would have the same path — the
    // discriminator is the count, above) + no error line for the
    // manager's unused `Workshop/wrong` folder.
    assert_eq!(a.get_pack_datas().len(), 3);
    assert!(
        a.get_load_results()
            .error_messages
            .iter()
            .all(|m| !m.contains("Workshop/wrong")),
        "the uninitialized manager's folders are never loaded"
    );
}

/// The C++'s `steamManager == nullptr` state (`Assets::new`'s default):
/// the cache file EXISTS on disk but is never read — no steam manager,
/// no workshop packs.
#[test]
fn no_steam_manager_never_reads_the_cache() {
    let root = temp_dir("no-manager");
    local_pack(&root);
    write_pack(&root, "Workshop/cached", "cached");
    write_cache(&root, r#"["Workshop/cached"]"#);

    let a = Assets::new(&root, true, false).expect("load");

    assert_eq!(pack_ids(&a), vec![LOCAL_ID]);
}

/// The C++'s four `return false` arms of `loadWorkshopPackDatasFromCache`
/// (Assets.cpp:671-708): each degrades to the local-only load (the
/// manager exists, is uninitialized, the cache is unusable).
#[test]
fn the_cache_false_arms_degrade_to_local_only() {
    // (file, the C++ arm it exercises) — `None` = no file at all.
    let arms: &[(&str, Option<&str>)] = &[
        (
            "no-file", None, // "Workshop cache file does not exist…"
        ),
        (
            "missing-key",
            Some(r#"{"other":[]}"#), // "[ERROR]: Cannot locate cache
                                     // array in workshop cache file"
        ),
        (
            "null-array",
            Some(r#"{"cachedPacks":null}"#), // "Cache array is null…"
        ),
        (
            "string-not-array",
            Some(r#"{"cachedPacks":"workshopCache.json"}"#), // the same
                                                             // null arm (the C++'s `type() != arrayValue` half)
        ),
        (
            "empty-array",
            Some(r#"{"cachedPacks":[]}"#), // "Cache array is empty…"
        ),
    ];
    for (i, (label, file)) in arms.iter().enumerate() {
        let root = temp_dir(&format!("false-arm-{i}-{label}"));
        local_pack(&root);
        write_pack(&root, "Workshop/cached", "cached");
        if let Some(body) = file {
            fs::write(root.join("workshopCache.json"), body).expect("cache");
        }

        let a = Assets::new_with_workshop(
            &root,
            true,
            false,
            Some(Arc::new(FakeWorkshop {
                initialized: false,
                folders: Vec::new(),
            })),
        )
        .expect("load");
        assert_eq!(
            pack_ids(&a),
            vec![LOCAL_ID],
            "arm {label}: the unusable cache degrades to local only"
        );
    }
}

/// A cached folder WITHOUT a `pack.json` (the C++'s
/// `tryLoadPackFromPath` failure arm, Assets.cpp:722-734): non-fatal —
/// the `Error loading pack data` line lands in the load results.
#[test]
fn a_cached_path_without_pack_json_is_an_error_line() {
    let root = temp_dir("ghost-cache");
    local_pack(&root);
    write_cache(&root, r#"["Workshop/ghost"]"#);
    // `Workshop/ghost/` exists but has no `pack.json`.
    fs::create_dir_all(root.join("Workshop/ghost")).expect("ghost dir");

    let a = Assets::new_with_workshop(
        &root,
        true,
        false,
        Some(Arc::new(FakeWorkshop {
            initialized: false,
            folders: Vec::new(),
        })),
    )
    .expect("load");

    assert_eq!(pack_ids(&a), vec![LOCAL_ID]);
    assert_eq!(
        a.get_load_results().error_messages,
        vec!["Error loading pack data 'Workshop/ghost/\n".to_string()]
    );
}

/// The C++'s uncaught `getExtr<std::vector<std::string>>` type error
/// (a non-string-convertible array element — an object; the JsonCpp
/// fork's `asString` throws `"Value is not convertible to string."`
/// uncaught in the C++ → the port's fatal-error class, the `Assets`
/// ctor's `Err` mapping).
#[test]
fn a_non_string_cache_element_is_fatal() {
    let root = temp_dir("bad-cache-element");
    local_pack(&root);
    write_cache(&root, r#"[{"x":1}]"#);

    let err = Assets::new_with_workshop(
        &root,
        true,
        false,
        Some(Arc::new(FakeWorkshop {
            initialized: false,
            folders: Vec::new(),
        })),
    )
    .expect_err("the C++'s uncaught type error → the port's fatal");
    assert!(
        err.reason.contains("Exception during asset loading"),
        "the fatal mapping's prefix: {}",
        err.reason
    );
}
