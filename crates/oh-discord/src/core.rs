//! The `DiscordCore` seam — the raw Discord Game SDK surface the
//! `discord_manager` uses (the `oh-workshop-uploader/src/steam.rs` `SteamUgc`
//! pattern: the raw SDK surface; [`crate::manager::DiscordManager`] layers the
//! C++ `discord_manager` behavior on top).
//!
//! The C++ `discord_manager` (`src/SSVOpenHexagon/Core/Discord.cpp`) drives a
//! `discord::Core` (the official Game SDK) through exactly these calls —
//! each maps to one `DiscordCore` method (the skill doc
//! `.agents/skills/lib-discord.md` carries the full line-cited enumeration):
//!
//! | `DiscordCore` method | C++ call (`Discord.cpp`) |
//! |---|---|
//! | [`DiscordCore::init`] | `discord::Core::Create(725763266110029964, DiscordCreateFlags_NoRequireDiscord, &core)` (22-23) |
//! | [`DiscordCore::set_log_hook`] | `_core->SetLogHook(discord::LogLevel::Debug, …)` (42-47) |
//! | [`DiscordCore::register_command`] | `_core->ActivityManager().RegisterCommand("SSVOpenHexagon.exe")` (49-58) |
//! | [`DiscordCore::register_steam`] | `_core->ActivityManager().RegisterSteam(1358090)` (59-66) |
//! | [`DiscordCore::run_callbacks`] | `_core->RunCallbacks()` (79) |
//! | [`DiscordCore::update_activity`] | `_core->ActivityManager().UpdateActivity(activity, cb)` (102 / 128 / 173) |
//!
//! `Send` so a [`std::sync::Mutex`] over the core makes a
//! [`crate::manager::DiscordManager`] `Send + Sync` (the engine holds it as
//! `Arc<dyn Discord>`).

/// C++ `discord::Activity` (`public/discord/types.h:301-330`) — the
/// rich-presence payload the manager builds. Only the fields
/// `discord_manager` touches are carried: `details` / `state` / the
/// `start` timestamp (the C++ never sets name/type/assets/party/secrets).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Activity {
    /// C++ `activity.SetDetails(...)` — the FIRST rich-presence line.
    pub details: String,
    /// C++ `activity.SetState(...)` — the SECOND rich-presence line.
    pub state: String,
    /// C++ `activity.GetTimestamps().SetStart(...)`
    /// (`public/discord/types.h:240-250`) — unix **seconds**; `0` clears the
    /// timestamp (the C++ `SetStart(0)` "remove any existing timestamp",
    /// `Discord.cpp:127,171`).
    pub start: i64,
}

/// C++ `725763266110029964` (`Discord.cpp:23`) — the Discord Application
/// (client) ID. The C++ passes it as `ClientId` (`int64_t`,
/// `public/discord/types.h:170`); the value is positive so `u64` carries it.
pub const DISCORD_APP_ID: u64 = 725_763_266_110_029_964;

/// C++ `1358090` (`Discord.cpp:59`) — the linked Steam App ID
/// (`RegisterSteam(std::uint32_t)`, `activity_manager.h:12`).
pub const STEAM_APP_ID: u32 = 1_358_090;

/// C++ `"SSVOpenHexagon.exe"` (`Discord.cpp:49`) — the party-join command
/// Discord runs (`RegisterCommand(char const*)`, `activity_manager.h:11`).
pub const REGISTER_COMMAND: &str = "SSVOpenHexagon.exe";

/// The raw Discord SDK surface the [`crate::manager::DiscordManager`] drives.
///
/// The real implementations are [`crate::ipc::IpcDiscordCore`] (the
/// `discord-rich-presence` IPC client — the contract's documented fallback
/// for the absent official-SDK crate) and a test double (see
/// `crates/oh-discord/tests/manager.rs`). [`crate::no_discord::NoDiscordCore`]
/// is the no-SDK default (mirrors `oh-workshop-uploader`'s `NoSteam`):
/// `init` fails, so a manager over it is inert — exactly the C++'s
/// no-Discord degradation (`Discord.cpp:187-219`, the Android arm).
pub trait DiscordCore: Send {
    /// C++ `initialize_discord` (Discord.cpp:20-33): `discord::Core::Create(
    /// DISCORD_APP_ID, DiscordCreateFlags_NoRequireDiscord, &core)` —
    /// `true` iff the result is `discord::Result::Ok` (`types.h:11`). The
    /// "Failed/Successfully initialized core" log is the MANAGER's (below),
    /// not the core's (the C++ logs it in `initialize_discord`, a
    /// `discord_manager`-side function, not in the SDK).
    fn init(&mut self) -> bool;

    /// C++ `_core->SetLogHook(discord::LogLevel::Debug, cb)`
    /// (Discord.cpp:42-47; `core.h:21`) — the SDK-internal log hook. Only
    /// meaningful on the Game-SDK route; the IPC impl is a documented no-op
    /// (the crate logs via the `log` crate instead).
    fn set_log_hook(&mut self);

    /// C++ `_core->ActivityManager().RegisterCommand(REGISTER_COMMAND)`
    /// (Discord.cpp:49-58; `activity_manager.h:11`) — `true` on
    /// `Result::Ok`.
    fn register_command(&mut self, command: &str) -> bool;

    /// C++ `_core->ActivityManager().RegisterSteam(STEAM_APP_ID)`
    /// (Discord.cpp:59-66; `activity_manager.h:12`) — `true` on
    /// `Result::Ok`.
    fn register_steam(&mut self, steam_app_id: u32) -> bool;

    /// C++ `_core->RunCallbacks()` (Discord.cpp:79; `core.h:20`) — `true`
    /// iff the result is `Result::Ok`. On the Game-SDK route this also
    /// delivers the pending `UpdateActivity` result into its callback; the
    /// IPC route has no callback queue (documented deviation,
    /// `crate::ipc`).
    fn run_callbacks(&mut self) -> bool;

    /// C++ `_core->ActivityManager().UpdateActivity(activity, cb)`
    /// (Discord.cpp:102/128/173; `activity_manager.h:13`) — dispatches the
    /// activity. The C++ call is `void` (the result arrives in `cb` during a
    /// later `RunCallbacks`); the port folds that callback's `"Fail"` log
    /// into a synchronous `false` return (the manager logs it).
    fn update_activity(&mut self, activity: &Activity) -> bool;
}
