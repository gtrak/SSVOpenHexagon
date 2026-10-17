//! 29p1 — the `oh_assets::WorkshopPacks` seam for `SteamManager` (the
//! C++'s `HGAssets{&steamManager, ...}` argument, main.cpp:211/:380):
//! `is_initialized()` + `workshop_pack_folders()` mirror the C++'s
//! `steam_manager::is_initialized` / `for_workshop_pack_folders`
//! surface the C++'s `Assets::loadAllPackDatas` uses (Assets.cpp:750-764).

use std::collections::HashMap;

use oh_assets::WorkshopPacks;
use oh_steam::{SteamManager, SteamSdk};

/// The scripted raw-SDK double (workshop surface only) — the
/// `workshop_cache.rs` fixture's shape.
#[derive(Debug)]
struct Fake {
    subscribed: Vec<u64>,
    install_info: HashMap<u64, (u64, String, u32)>,
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
        self.subscribed.clone()
    }
    fn item_install_info(&self, id: u64) -> Option<(u64, String, u32)> {
        self.install_info
            .get(&id)
            .map(|(s, f, t)| (*s, f.clone(), *t))
    }
}

#[test]
fn workshop_packs_seam_mirrors_the_cpp_manager_surface() {
    // The manager's workshop cache write targets the process CWD (the
    // C++'s relative `"workshopCache.json"` — an isolated CWD keeps the
    // repo tree clean; `workshop_cache.rs`'s isolation note).
    let dir = tempfile::tempdir().unwrap();
    let cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    // (a) The initialized manager: the installed folders (the C++'s
    // `GetItemInstallInfo` paths, the SDK-subscription order).
    let mut install = HashMap::new();
    install.insert(1_u64, (10_u64, "/steam/workshop/f1".to_owned(), 1_u32));
    install.insert(2_u64, (20_u64, "/steam/workshop/f2".to_owned(), 2_u32));
    let m = SteamManager::new(Box::new(Fake {
        subscribed: vec![1, 2],
        install_info: install,
    }));
    assert!(m.is_initialized());
    assert_eq!(
        m.workshop_pack_folders(),
        vec!["/steam/workshop/f1", "/steam/workshop/f2"]
    );
    let _ = std::fs::remove_file("workshopCache.json");
    std::env::set_current_dir(cwd).unwrap();

    // (b) The no-SDK manager (the default backend): uninitialized →
    // the cache-fallback half is the port's live path (the C++'s
    // `!is_initialized` arm, Assets.cpp:755-763) + no live folders.
    let m2 = SteamManager::new(Box::new(oh_steam::NoSteam));
    assert!(!m2.is_initialized());
    assert!(m2.workshop_pack_folders().is_empty());
}
