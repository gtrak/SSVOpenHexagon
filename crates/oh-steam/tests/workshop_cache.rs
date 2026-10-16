//! The `workshopCache.json` write — C++ `load_workshop_data`'s cache half
//! (Steam.cpp:238-244). Isolated in its own test binary: the C++ writes
//! to the process CWD (relative `"workshopCache.json"`), and
//! `std::env::set_current_dir` is process-global, so these scenarios run
//! sequentially inside ONE test function.

use std::collections::HashMap;

use oh_steam::{SteamManager, SteamSdk};

/// The scripted raw-SDK double (workshop surface only).
#[derive(Debug)]
struct Fake {
    subscribed: Vec<u64>,
    install_info: HashMap<u64, (u64, String, u32)>,
    subscribed_calls: std::sync::Mutex<usize>,
}

impl SteamSdk for Fake {
    fn init(&self) -> bool {
        true
    }
    fn shutdown(&self) {}
    fn run_callbacks(&self) {}
    fn take_stats_received(&self) -> Vec<oh_steam::StatsReceived> {
        Vec::new()
    }
    fn take_stats_stored(&self) -> usize {
        0
    }
    fn take_achievement_stored(&self) -> usize {
        0
    }
    fn user_steam_id(&self) -> Option<u64> {
        Some(1)
    }
    fn request_current_stats(&self) -> bool {
        false
    }
    fn store_stats(&self) -> bool {
        false
    }
    fn set_achievement(&self, _name: &str) -> bool {
        false
    }
    fn get_achievement(&self, _name: &str) -> Option<bool> {
        None
    }
    fn set_stat_float(&self, _name: &str, _value: f32) -> bool {
        false
    }
    fn set_stat_int(&self, _name: &str, _value: i32) -> bool {
        false
    }
    fn get_stat_float(&self, _name: &str) -> Option<f32> {
        None
    }
    fn get_stat_int(&self, _name: &str) -> Option<i32> {
        None
    }
    fn set_rich_presence(&self, _key: &str, _value: &str) -> bool {
        false
    }
    fn subscribed_items(&self) -> Vec<u64> {
        *self.subscribed_calls.lock().unwrap() += 1;
        self.subscribed.clone()
    }
    fn item_install_info(&self, id: u64) -> Option<(u64, String, u32)> {
        self.install_info
            .get(&id)
            .map(|(s, f, t)| (*s, f.clone(), *t))
    }
}

#[test]
fn workshop_cache_write_is_the_cpp_json_shape() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    // Scenario 1 — the normal case (Steam.cpp:200-244): three subscribed
    // items, two installed → the cache is the SDK-subscription order of
    // the INSTALLED folders, compact `{"cachedPacks":[...]}` (the ssvuj
    // `Writer` is JsonCpp's built-in compact writer, SSVUtilsJson/Utils/
    // Io.hpp:56-62).
    let mut install = HashMap::new();
    install.insert(1_u64, (10_u64, "dir/f1".to_owned(), 1_u32));
    install.insert(3_u64, (30_u64, "dir/f3".to_owned(), 3_u32));
    let fake = Fake {
        subscribed: vec![1, 2, 3],
        install_info: install,
        subscribed_calls: std::sync::Mutex::new(0),
    };
    let m = SteamManager::new(Box::new(fake));
    assert!(m.is_initialized());
    let mut folders = Vec::new();
    m.for_workshop_pack_folders(|f| folders.push(f.to_owned()));
    assert_eq!(folders, vec!["dir/f1".to_owned(), "dir/f3".to_owned()]);
    assert_eq!(
        std::fs::read_to_string("workshopCache.json").unwrap(),
        "{\"cachedPacks\":[\"dir/f1\",\"dir/f3\"]}"
    );
    drop(m);

    // Scenario 2 — nothing installed: no cache file (Steam.cpp:239: the
    // write is gated on the non-empty set).
    let _ = std::fs::remove_file("workshopCache.json");
    let empty = Fake {
        subscribed: vec![9],
        install_info: HashMap::new(),
        subscribed_calls: std::sync::Mutex::new(0),
    };
    let m2 = SteamManager::new(Box::new(empty));
    assert!(m2.for_workshop_pack_folders_collect().is_empty());
    assert!(!std::path::Path::new("workshopCache.json").exists());
    drop(m2);

    // Scenario 3 — a duplicate folder (two items installed in one
    // folder): the C++'s `arch` at the set-size index keeps the set
    // deduped while the array records each installation (verified
    // against the C++ semantics in the manager's docs).
    let mut install3 = HashMap::new();
    install3.insert(1_u64, (1_u64, "same".to_owned(), 1_u32));
    install3.insert(2_u64, (2_u64, "same".to_owned(), 2_u32));
    install3.insert(3_u64, (3_u64, "other".to_owned(), 3_u32));
    let fake3 = Fake {
        subscribed: vec![1, 2, 3],
        install_info: install3,
        subscribed_calls: std::sync::Mutex::new(0),
    };
    let m3 = SteamManager::new(Box::new(fake3));
    // C++: item 3's `arch` index is the set size (1 — "same" already
    // deduped), which OVERWRITES the array slot, not appends (the
    // verified C++-semantics simulation, manager docs).
    assert_eq!(
        std::fs::read_to_string("workshopCache.json").unwrap(),
        "{\"cachedPacks\":[\"same\",\"other\"]}"
    );
    drop(m3);

    std::env::set_current_dir(cwd).unwrap();
}

// A `for_workshop_pack_folders` collect helper for the assertions (the
// C++'s visitor, as a Vec).
trait CollectFolders {
    fn for_workshop_pack_folders_collect(&self) -> Vec<String>;
}
impl CollectFolders for SteamManager {
    fn for_workshop_pack_folders_collect(&self) -> Vec<String> {
        let mut v = Vec::new();
        self.for_workshop_pack_folders(|f| v.push(f.to_owned()));
        v
    }
}
