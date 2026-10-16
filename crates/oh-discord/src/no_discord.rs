//! The no-SDK default [`DiscordCore`] — the mirror of
//! `oh-workshop-uploader`'s `NoSteam`: `init` fails, so a
//! [`crate::manager::DiscordManager`] over it is inert (every method returns
//! `false` / makes no core call) — exactly the C++'s no-Discord degradation
//! (`Discord.cpp:187-219`, the `#else`/Android arm: `_core{nullptr},
//! _initialized{false}`, every method `return false`).
//!
//! The `unimplemented!` arms are unreachable in a well-formed manager: the
//! manager's ctor only calls them when `init` succeeded, and `init` always
//! returns `false` here — so they fire only on a misuse (calling the seam
//! methods directly on an uninitialized core). They make any accidental use
//! loud, as the `NoSteam` pattern intends.

use crate::core::{Activity, DiscordCore};

#[derive(Debug, Clone, Copy, Default)]
pub struct NoDiscordCore;

impl DiscordCore for NoDiscordCore {
    /// No Discord SDK / client is available without the real wiring — the
    /// C++'s `Core::Create` failure path (`Discord.cpp:25-28`).
    fn init(&mut self) -> bool {
        false
    }

    fn set_log_hook(&mut self) {
        unimplemented!("NoDiscordCore: never initialized")
    }

    fn register_command(&mut self, _command: &str) -> bool {
        unimplemented!("NoDiscordCore: never initialized")
    }

    fn register_steam(&mut self, _steam_app_id: u32) -> bool {
        unimplemented!("NoDiscordCore: never initialized")
    }

    fn run_callbacks(&mut self) -> bool {
        unimplemented!("NoDiscordCore: never initialized")
    }

    fn update_activity(&mut self, _activity: &Activity) -> bool {
        unimplemented!("NoDiscordCore: never initialized")
    }
}
