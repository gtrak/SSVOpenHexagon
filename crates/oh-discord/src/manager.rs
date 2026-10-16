//! The `DiscordManager` — the 1:1 port of the C++ `discord_manager`
//! (`src/SSVOpenHexagon/Core/Discord.cpp` + `include/.../Discord.hpp`),
//! layered over a [`crate::core::DiscordCore`] seam (the `steam_helper` over
//! `SteamUgc` pattern from `oh-workshop-uploader/src/steam.rs`).
//!
//! # What the manager owns (the C++ `discord_manager` behavior)
//!
//! * the `_initialized` flag (hpp:19) — set by the ctor's
//!   `initialize_discord` (Discord.cpp:20-33), gates every method;
//! * the ctor's post-init side effects (Discord.cpp:35-68): the log hook,
//!   `RegisterCommand("SSVOpenHexagon.exe")`, `RegisterSteam(1358090)`;
//! * the four public methods (hpp:31-36): `run_callbacks`,
//!   `set_rich_presence_in_menu`, `set_rich_presence_on_replay`,
//!   `set_rich_presence_in_game` — each gated on `_initialized` and building
//!   the exact `discord::Activity` the C++ builds (byte-exact `details` /
//!   `state` / `start`).
//!
//! # What the manager does NOT own
//!
//! The `discordAttempt` / `discordHung` retry logic is a `HexagonGame`
//! member (not a `discord_manager` one) — `include/.../HexagonGame.hpp:89,91`
//! and `src/.../HexagonGame.cpp:495-506`. It lives in `oh-game`, driving
//! [`Discord::run_callbacks`] (see `.agents/skills/lib-discord.md`).

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::{Activity, DiscordCore, REGISTER_COMMAND, STEAM_APP_ID};

/// The `Discord` seam the game consumes — the `discord_manager`'s public
/// surface (`include/.../Discord.hpp:31-36`). `Send + Sync + 'static` because
/// the engine holds it as `Arc<dyn Discord>` (the C++'s raw
/// `discord_manager*`, `HexagonGame.hpp:88`).
///
/// Every method mirrors a `discord_manager` method's `bool` return: `false`
/// when the manager is not initialized (the C++ `_initialized` gate), else
/// `true` (the C++ returns `true` after dispatching — the `UpdateActivity`
/// result is delivered separately and does not affect the return).
pub trait Discord: Send + Sync + 'static {
    /// C++ `discord_manager::run_callbacks()` (Discord.cpp:72-86).
    fn run_callbacks(&self) -> bool;
    /// C++ `discord_manager::set_rich_presence_in_menu()`
    /// (Discord.cpp:88-112).
    fn set_rich_presence_in_menu(&self) -> bool;
    /// C++ `discord_manager::set_rich_presence_on_replay()`
    /// (Discord.cpp:114-138).
    fn set_rich_presence_on_replay(&self) -> bool;
    /// C++ `discord_manager::set_rich_presence_in_game(level_info,
    /// second_info, dead)` (Discord.cpp:140-183).
    fn set_rich_presence_in_game(&self, level_info: &str, second_info: &str, dead: bool) -> bool;
}

/// C++ `discord_manager` (Discord.cpp:35-183) over a [`DiscordCore`] seam.
///
/// `Mutex<C>` (not `C` by value) because the engine holds the manager as a
/// shared `Arc<dyn Discord>` and calls its `&self` methods from the update
/// loop; the C++'s `discord::Core*` is likewise a single shared object whose
/// methods are invoked serially (the game loop is single-threaded).
pub struct DiscordManager<C: DiscordCore> {
    core: Mutex<C>,
    // C++ `bool _initialized` (Discord.hpp:19).
    initialized: bool,
}

impl<C: DiscordCore> DiscordManager<C> {
    /// C++ `discord_manager()` ctor (Discord.cpp:35-68) — the
    /// `initialize_discord` initializer + the post-init log hook / command /
    /// Steam registration, in the C++'s exact log order:
    ///
    /// 1. `"Successfully initialized core"` / `"Failed to initialize core"`
    ///    (from `initialize_discord`, Discord.cpp:31 / 26-27);
    /// 2. (initialized only) the log hook (no log of its own);
    /// 3. (initialized only) `"Successfully registered command"` /
    ///    `"Failed to register command"` (Discord.cpp:56 / 52);
    /// 4. (initialized only) `"Successfully registered Steam app"` /
    ///    `"Failed to register Steam app"` (Discord.cpp:65 / 61).
    pub fn new(core: C) -> Self {
        let core = Mutex::new(core);
        let initialized = {
            let mut c = core.lock().expect("discord core poisoned");
            let ok = c.init();
            // C++ `initialize_discord` (Discord.cpp:25-32).
            if ok {
                log_line("Successfully initialized core");
            } else {
                log_line("Failed to initialize core");
            }
            ok
        };
        if initialized {
            let mut c = core.lock().expect("discord core poisoned");
            // C++ `SetLogHook` (Discord.cpp:42-47).
            c.set_log_hook();
            // C++ `RegisterCommand` (Discord.cpp:49-58).
            if c.register_command(REGISTER_COMMAND) {
                log_line("Successfully registered command");
            } else {
                log_line("Failed to register command");
            }
            // C++ `RegisterSteam` (Discord.cpp:59-66).
            if c.register_steam(STEAM_APP_ID) {
                log_line("Successfully registered Steam app");
            } else {
                log_line("Failed to register Steam app");
            }
        }
        Self { core, initialized }
    }

    /// C++ `bool _initialized` (Discord.hpp:19) — the degradation gate.
    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// C++ `discord_manager::run_callbacks()` (Discord.cpp:72-86): gated on
    /// `_initialized` (74-77); a non-`Ok` `RunCallbacks` logs
    /// `"Failed to run callbacks"` (81) and returns `false`.
    pub fn run_callbacks(&self) -> bool {
        if !self.initialized {
            return false;
        }
        let mut c = self.core.lock().expect("discord core poisoned");
        if !c.run_callbacks() {
            log_line("Failed to run callbacks");
            return false;
        }
        true
    }

    /// C++ `discord_manager::set_rich_presence_in_menu()`
    /// (Discord.cpp:88-112): `state = "Selecting Level"`, `details = ""`,
    /// `start = now (unix seconds)`.
    pub fn set_rich_presence_in_menu(&self) -> bool {
        if !self.initialized {
            return false;
        }
        let activity = Activity {
            state: "Selecting Level".to_string(),
            details: String::new(),
            start: now_unix_seconds(),
        };
        self.push_activity(&activity);
        true
    }

    /// C++ `discord_manager::set_rich_presence_on_replay()`
    /// (Discord.cpp:114-138): `state = "Watching Replay"`, `details = ""`,
    /// `start = 0` (clears the timestamp).
    pub fn set_rich_presence_on_replay(&self) -> bool {
        if !self.initialized {
            return false;
        }
        let activity = Activity {
            state: "Watching Replay".to_string(),
            details: String::new(),
            start: 0,
        };
        self.push_activity(&activity);
        true
    }

    /// C++ `discord_manager::set_rich_presence_in_game(level_info,
    /// second_info, dead)` (Discord.cpp:140-183): `details = level_info`
    /// (first line), `state = second_info` (second line), `start = now`
    /// when `!dead` else `0` (clears the timestamp on death).
    pub fn set_rich_presence_in_game(
        &self,
        level_info: &str,
        second_info: &str,
        dead: bool,
    ) -> bool {
        if !self.initialized {
            return false;
        }
        let activity = Activity {
            details: level_info.to_string(),
            state: second_info.to_string(),
            start: if dead { 0 } else { now_unix_seconds() },
        };
        self.push_activity(&activity);
        true
    }

    /// The C++ `UpdateActivity(activity, cb)` dispatch shared by the three
    /// setters (Discord.cpp:102 / 128 / 173). The C++ call is `void` and the
    /// method returns `true` regardless of the (async) result; the `"Fail"`
    /// log (Discord.cpp:107 / 133 / 178) is folded here as a synchronous
    /// side effect of a `false` from the seam.
    fn push_activity(&self, activity: &Activity) {
        let mut c = self.core.lock().expect("discord core poisoned");
        if !c.update_activity(activity) {
            log_line("Fail");
        }
    }
}

impl<C: DiscordCore + 'static> Discord for DiscordManager<C> {
    fn run_callbacks(&self) -> bool {
        DiscordManager::run_callbacks(self)
    }
    fn set_rich_presence_in_menu(&self) -> bool {
        DiscordManager::set_rich_presence_in_menu(self)
    }
    fn set_rich_presence_on_replay(&self) -> bool {
        DiscordManager::set_rich_presence_on_replay(self)
    }
    fn set_rich_presence_in_game(&self, level_info: &str, second_info: &str, dead: bool) -> bool {
        DiscordManager::set_rich_presence_in_game(self, level_info, second_info, dead)
    }
}

/// The `ssvu::lo("Discord")` sink — the C++ prints to stdout; the port does
/// the same, channel-prefixed (the `oh-game` `log` convention,
/// `hexagon_game.rs:2377-2381`).
fn log_line(message: &str) {
    println!("[Discord] {message}");
}

/// C++ `static_cast<std::int64_t>(std::chrono::high_resolution_clock::now()
/// .time_since_epoch().count() / 1'000'000'000)` (Discord.cpp:99-101,
/// 161-165) — the current unix time in whole seconds.
#[must_use]
fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
