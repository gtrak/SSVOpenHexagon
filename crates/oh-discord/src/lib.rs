//! `oh-discord` — the Discord integration port (issue 21).
//!
//! The 1:1 port of the C++ `discord_manager`
//! (`src/SSVOpenHexagon/Core/Discord.cpp` + `include/.../Discord.hpp`):
//! Discord init, the three rich-presence setters (byte-exact `details` /
//! `state` / `start`), and graceful degradation when Discord is absent.
//!
//! # Binding (the contract's decision, `.agents/skills/lib-discord.md`)
//!
//! The C++ links the official Discord **Game SDK** (`discord_game_sdk.so` /
//! `.dll` in `_RELEASE`). No sound Rust crate wraps that SDK (verified
//! against the crates.io index), so the port takes the contract's documented
//! fallback: the **IPC** route via the `discord-rich-presence` crate
//! ([`IpcDiscordCore`]). The Game-SDK-vs-IPC difference is documented in the
//! skill doc. Either way the `discord_manager` behavior — the retry/degradation
//! contract the game relies on — is identical because it sits on this seam.
//!
//! # The seam
//!
//! * [`DiscordCore`] — the raw SDK surface (`init` / `set_log_hook` /
//!   `register_command` / `register_steam` / `run_callbacks` /
//!   `update_activity`); the real impl is [`IpcDiscordCore`], the no-SDK
//!   default is [`NoDiscordCore`], and tests use a fake.
//! * [`DiscordManager`] — the C++ `discord_manager` over a `DiscordCore`.
//! * [`Discord`] — the trait the *game* consumes (`run_callbacks` + the three
//!   setters); the engine holds `Option<Arc<dyn Discord>>` (the C++'s
//!   nullable `discord_manager*`, `main.cpp:264-269,416`).
//!
//! # Degradation (the game MUST run without Discord)
//!
//! * Manager level: `init` fails → `_initialized == false` → every setter /
//!   `run_callbacks` returns `false` with no core call (the C++'s Android
//!   arm, `Discord.cpp:187-219`).
//! * Game-loop level: the `discordAttempt` / `discordHung` retry (NOT a
//!   `discord_manager` member — `HexagonGame.hpp:89,91`,
//!   `HexagonGame.cpp:495-506`) lives in `oh-game`; it stops calling
//!   `run_callbacks` after 20 consecutive failures, so the game keeps
//!   running either way. CI (headless, no Discord) proves this path.
//!
//! # MANUAL on-Discord checks (NOT run in CI)
//!
//! CI is headless (no Discord client, no display): it exercises the
//! degradation path only. The positive path (a live Discord client showing
//! "Selecting Level" / "<Level> [x<diff>]" / "Watching Replay" /
//! "Survived <t>s") needs a machine with the Discord desktop client running —
//! see `.agents/skills/lib-discord.md` ("MANUAL on-Discord checks").

mod core;
mod ipc;
mod manager;
mod no_discord;

pub use core::{Activity, DiscordCore, DISCORD_APP_ID, REGISTER_COMMAND, STEAM_APP_ID};
pub use ipc::IpcDiscordCore;
pub use manager::{Discord, DiscordManager};
pub use no_discord::NoDiscordCore;
