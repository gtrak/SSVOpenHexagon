//! The real [`DiscordCore`] — the `discord-rich-presence` IPC client
//! (issue 21's binding decision, `.agents/skills/lib-discord.md`).
//!
//! The official Discord Game SDK has no sound Rust crate, so this speaks the
//! Discord **IPC** protocol to the LOCAL Discord desktop client instead of the
//! Game SDK's RPC-server route. The differences (documented in the skill
//! doc's table):
//!
//! * `init` connects to the local client's `discord-ipc-N` pipe
//!   (`ipc_unix.rs:33-77` in the crate) — **requires the client running**;
//!   with no client it fails and the manager degrades (the C++'s
//!   `Core::Create`-failure path, identical game behavior).
//! * `set_log_hook` / `register_command` / `register_steam` have no IPC
//!   equivalent — documented no-ops (`register_*` return `true`, the C++'s
//!   `Result::Ok` path, so the manager's "Successfully registered …" lines
//!   still print).
//! * `run_callbacks` — the IPC route has no callback queue (`set_activity`
//!   is fire-and-forget); returns `true` while connected so the game's
//!   retry logic doesn't spuriously "hang" on a healthy link. (Deviation:
//!   the C++ `RunCallbacks` also pumps join/invite events the game never
//!   installs handlers for.)

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};

use crate::core::{Activity, DiscordCore};

/// The `discord-rich-presence` IPC client behind the [`DiscordCore`] seam.
///
/// Construct with the Discord Application ID
/// ([`crate::core::DISCORD_APP_ID`]): `IpcDiscordCore::new(DISCORD_APP_ID)`.
pub struct IpcDiscordCore {
    // C++ `ClientId` (`public/discord/types.h:170`) — the crate's
    // `DiscordIpcClient::new` takes the client id as a string.
    client_id: String,
    // The connected IPC client (the C++ `discord::Core*`); `None` until
    // `init` connects.
    client: Option<DiscordIpcClient>,
}

impl IpcDiscordCore {
    #[must_use]
    pub fn new(client_id: u64) -> Self {
        Self {
            client_id: client_id.to_string(),
            client: None,
        }
    }
}

impl DiscordCore for IpcDiscordCore {
    /// C++ `discord::Core::Create(DISCORD_APP_ID, NoRequireDiscord, &core)`
    /// — the IPC equivalent is `DiscordIpcClient::new(client_id)` +
    /// `connect()` (pipe find + handshake). No local client → `false`
    /// (graceful, the C++'s `Result != Ok` path).
    fn init(&mut self) -> bool {
        let mut client = DiscordIpcClient::new(self.client_id.clone());
        match client.connect() {
            Ok(()) => {
                self.client = Some(client);
                true
            }
            Err(_) => false,
        }
    }

    /// No IPC equivalent (the Game SDK's `SetLogHook`, `core.h:21`); the
    /// crate logs via the `log` crate. Documented no-op.
    fn set_log_hook(&mut self) {}

    /// No IPC equivalent (the Game SDK's `RegisterCommand`,
    /// `activity_manager.h:11` — the party-join command). Documented no-op;
    /// returns `true` (the C++'s `Result::Ok` path).
    fn register_command(&mut self, _command: &str) -> bool {
        true
    }

    /// No IPC equivalent (the Game SDK's `RegisterSteam`,
    /// `activity_manager.h:12` — Steam-linking). Documented no-op; returns
    /// `true`.
    fn register_steam(&mut self, _steam_app_id: u32) -> bool {
        true
    }

    /// The IPC route has no callback queue (`set_activity` is
    /// fire-and-forget); report healthy while connected. Documented
    /// deviation from the Game SDK's `RunCallbacks` event pump.
    fn run_callbacks(&mut self) -> bool {
        self.client.is_some()
    }

    /// C++ `UpdateActivity(activity, cb)` — `set_activity` over the pipe.
    /// `start == 0` maps to NO `start` timestamp (the C++ `SetStart(0)`
    /// "clear" semantics).
    fn update_activity(&mut self, a: &Activity) -> bool {
        let Some(client) = self.client.as_mut() else {
            return false;
        };
        let mut payload = activity::Activity::new();
        payload = payload.state(a.state.clone()).details(a.details.clone());
        if a.start != 0 {
            payload = payload.timestamps(activity::Timestamps::new().start(a.start));
        }
        client.set_activity(payload).is_ok()
    }
}
