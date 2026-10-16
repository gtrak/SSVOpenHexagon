//! The no-SDK backend — the default build (feature `steam` OFF).
//!
//! It is the port of the C++'s own no-SDK build: the `#else` arm of
//! `#ifndef SSVOH_ANDROID` (Steam.cpp:961-1062), where every manager
//! method returns `false` / `nullopt` / no-ops. The C++'s branch stores a
//! `nullptr` impl (its `impl()` would null-deref — release UB); the port
//! keeps the observable all-false behavior without the UB: `init` fails,
//! so the manager's `_initialized` gate short-circuits every method,
//! exactly as a real SDK that failed `SteamAPI_Init()` would.

use crate::sdk::{StatsReceived, SteamSdk};

/// The no-SDK [`SteamSdk`] (see module docs).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSteam;

impl SteamSdk for NoSteam {
    fn init(&self) -> bool {
        false
    }
    fn shutdown(&self) {}
    fn run_callbacks(&self) {}
    fn take_stats_received(&self) -> Vec<StatsReceived> {
        Vec::new()
    }
    fn take_stats_stored(&self) -> usize {
        0
    }
    fn take_achievement_stored(&self) -> usize {
        0
    }
    fn user_steam_id(&self) -> Option<u64> {
        None
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
        Vec::new()
    }
    fn item_install_info(&self, _id: u64) -> Option<(u64, String, u32)> {
        None
    }
}
