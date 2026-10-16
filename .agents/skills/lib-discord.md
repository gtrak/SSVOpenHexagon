# lib-discord — Discord Game SDK / rich presence (issue 21)

Source of truth (C++ oracle): `src/SSVOpenHexagon/Core/Discord.cpp` (221 lines) +
`include/SSVOpenHexagon/Core/Discord.hpp`. The C++ links the **official Discord
Game SDK** (`public/discord/*.h` C++ wrapper over `public/discord/ffi.h` C FFI;
the native `discord_game_sdk.so`/`.dll` ship in `_RELEASE`).

Rust port: `crates/oh-discord` — `DiscordManager` (the 1:1 port of
`discord_manager`) over a `DiscordCore` seam, plus the real IPC impl
(`IpcDiscordCore`, `discord-rich-presence` crate) and a `NoDiscordCore` no-SDK
default. The game consumes it through the `oh_discord::Discord` seam
(`oh-game` `HexagonGame` + `oh-app` composition).

## Binding decision — the IPC route (documented difference)

The contract preferred "a binding of the official Game SDK if a sound crate
exists." **It does not.** Verified against the crates.io sparse index
(2026-06):

- `discord-game-sdk` — **does not exist** (the official SDK is C/C++; its
  headers are vendored here, there is no Rust crate wrapping the
  `discord_game_sdk.so`/`.dll` FFI).
- `discord-sdk` 0.1.0 — a 2021 tokio/async rich-presence wrapper; heavy
  (tokio + crossbeam + winreg + app_dirs2), unmaintained, not a Game-SDK
  binding.
- `discord-ipc` 0.0.1-alpha.1 — **yanked**.
- `discord-rich-presence` **1.1.0** (pure Rust, small dep set: `serde`,
  `serde_json`, `uuid`, `thiserror`, `log`, `serde_repr`) — the maintained
  crate for the Discord **IPC** protocol.

So the port takes the **IPC route** via `discord-rich-presence`, and the
difference from the C++ Game-SDK route is documented here (D-014: the
contract's "prefer the official SDK" could not be honored for lack of a sound
crate; the IPC route is the fallback the contract explicitly allows).

### What the Game-SDK route does that the IPC route does not

| C++ Game SDK (`Discord.cpp`) | IPC route (`discord-rich-presence`) |
|---|---|
| `Core::Create(client_id, NoRequireDiscord)` connects to **Discord's RPC servers** over the network (works without the local client running — the `NoRequireDiscord` flag, `ffi.h:76-77`) | `DiscordIpcClient::connect()` connects to the **local Discord client's IPC pipe** (`discord-ipc-0`…`discord-ipc-9` under `$XDG_RUNTIME_DIR`/`$TMPDIR`/… on Unix, `ipc_unix.rs:15-77`); **requires the desktop client running** |
| `RunCallbacks()` (`core.h:20`) — the SDK delivers the `UpdateActivity` result + join/invite events into the callback | `set_activity`/`clear_activity` are **fire-and-forget** (`discord_ipc.rs:184-224`); no result callback, no event pump |
| `SetLogHook(LogLevel::Debug, hook)` (`core.h:21`) — SDK-internal log lines routed into `ssvu::lo("Discord")` | no log hook (the crate logs via the `log` crate); the port's `set_log_hook` is a documented no-op |
| `ActivityManager().RegisterCommand("SSVOpenHexagon.exe")` (`activity_manager.h:11`) — the command Discord runs on a party-join | no equivalent; `register_command` is a documented no-op (returns `true`) |
| `ActivityManager().RegisterSteam(1358090)` (`activity_manager.h:12`) — Steam-linking | no equivalent; `register_steam` is a documented no-op (returns `true`) |
| `UpdateActivity(activity, cb)` — `cb(Result)` fires (during `RunCallbacks`) and the C++ logs `"Fail"` on non-`Ok` | `set_activity(activity)` — no result; the port folds the C++'s async `"Fail"` log into a synchronous `false` return (documented approximation, `manager.rs`) |

**Behavioral consequence in the game:** everything the C++ `discord_manager`
exposes (`run_callbacks` + the three rich-presence setters) is reproduced
1:1 **behind the seam**, so the game's retry/degradation logic, the exact
activity strings, and the call sequence are identical regardless of which
`DiscordCore` backs the manager. What differs is *whether an update actually
reaches Discord* (Game SDK: yes, over the network, even with the client
closed; IPC: only when the local client is running). The degradation model
(Discord absent → game keeps running) is identical on both routes.

## The `discord_manager` API surface (the port target)

`include/SSVOpenHexagon/Core/Discord.hpp:15-37` — `hg::Discord::discord_manager`:

```cpp
discord_manager();                  // inits the core (below)
~discord_manager();
bool run_callbacks();
bool set_rich_presence_in_menu();
bool set_rich_presence_on_replay();
bool set_rich_presence_in_game(const std::string& level_info,
    const std::string& second_info, bool dead = false);
```

Members: `discord::Core* _core; bool _initialized;` (hpp:18-19). Copy/move
deleted (hpp:25-29). `dead` defaults to `false` (hpp:36).

### Every Discord call `Discord.cpp` makes (line citations)

| # | C++ call | `Discord.cpp` lines | Notes |
|---|---|---|---|
| 1 | `discord::Core::Create(725763266110029964, DiscordCreateFlags_NoRequireDiscord, &core)` | 22-23 (in `initialize_discord`, 20-33) | `ClientId` = `int64_t` (`types.h:170`); `DiscordCreateFlags_NoRequireDiscord` = `1` (`ffi.h:76-77`); result != `Result::Ok` → `ssvu::lo("Discord") << "Failed to initialize core\n"` (25-28) + `return false`, else `"Successfully initialized core\n"` (31) |
| 2 | `_core->SetLogHook(discord::LogLevel::Debug, [](level, msg){ lo << (uint32)level << ": " << msg; })` | 42-47 | `LogLevel::Debug` = `4` (`types.h:63-67`); only reached when initialized (the ctor's `if(!_initialized) return;`, 38-41) |
| 3 | `_core->ActivityManager().RegisterCommand("SSVOpenHexagon.exe")` | 49-58 | `Result` != `Ok` → `"Failed to register command\n"` (52) else `"Successfully registered command\n"` (56) |
| 4 | `_core->ActivityManager().RegisterSteam(1358090)` | 59-66 | `Result` != `Ok` → `"Failed to register Steam app\n"` (61) else `"Successfully registered Steam app\n"` (65) |
| 5 | `_core->RunCallbacks()` | 79 (in `run_callbacks`, 72-86) | gated on `_initialized` (74-77 → `return false`); != `Ok` → `"Failed to run callbacks\n"` (81) + `return false`; else `return true` (85) |
| 6 | `set_rich_presence_in_menu`: `activity.SetState("Selecting Level")` (96), `SetDetails("")` (97), `GetTimestamps().SetStart(now_seconds)` (99-101), `ActivityManager().UpdateActivity(activity, cb)` (102-109) | 88-112 | `now_seconds` = `high_resolution_clock::now().time_since_epoch().count() / 1'000'000'000` cast to `int64_t` (99-101); `return true` (111) |
| 7 | `set_rich_presence_on_replay`: `SetState("Watching Replay")` (122), `SetDetails("")` (124), `SetStart(0)` (127), `UpdateActivity(activity, cb)` (128-135) | 114-138 | `SetStart(0)` clears the timestamp; `return true` (137) |
| 8 | `set_rich_presence_in_game(level_info, second_info, dead)`: `SetDetails(level_info.data())` (152), `SetState(second_info.data())` (155), `SetStart(now_seconds)` if `!dead` (161-165) else `SetStart(0)` (171), `UpdateActivity(activity, cb)` (173-180) | 140-183 | the C++ comment (149-154) notes the strings are the 1st/2nd RP line; `dead` comes from the caller default `false` (hpp:36); `return true` (182) |
| — | the `UpdateActivity` callback body (all three setters) | 103-109 / 129-135 / 174-180 | `if(r != discord::Result::Ok) lo << "Fail\n";` (107 / 133 / 178) — fires during a later `RunCallbacks` |

The `#else` (Android, no-SDK) arm (187-219): `_core{nullptr}, _initialized{false}` (191); every method `return false` (197-217). **This is exactly the port's degradation path** — the manager exists but is inert.

### The retry / hang (graceful-degradation) logic — NOT in `Discord.cpp`

It lives in `HexagonGame` (the game state), which is why the port puts it in
`oh-game`, not `oh-discord`:

- `include/SSVOpenHexagon/Core/HexagonGame.hpp:89,91` — `bool discordHung{false};`
  and `std::int8_t discordAttempt{1};` (members, alongside the
  `steamHung`/`steamAttempt` twins).
- `src/SSVOpenHexagon/Core/HexagonGame.cpp:495-506` (inside
  `updateRichPresenceCallbacks()`, 478-507):

```cpp
if(discordManager != nullptr && !discordHung)
{
    if(!discordManager->run_callbacks())
    {
        discordAttempt += 1;
        if(discordAttempt > 20)
        {
            discordHung = true;
            ssvu::lo("Discord") << "Too many failed callbacks. Stopping "
                                   "Discord callbacks.\n";
        }
    }
}
```

`updateRichPresenceCallbacks()` is driven every frame in **both** states:
the in-game `update` (`HGUpdate.cpp:155`) and the menu via
`fnHGUpdateRichPresenceCallbacks` = `hg.updateRichPresenceCallbacks()`
(`main.cpp:451-454`, invoked from `MenuGame::update` at
`MenuGame.cpp:2638-2640`). So the attempt counter starts at `1` and the
manager "hangs" after 20 consecutive `run_callbacks()` failures (the 21st
attempt). A non-initialized manager returns `false` every call, so on a
machine without Discord the loop burns 20 frames and then stops calling —
**the game keeps running either way** (this is what CI proves headless).

### Rich-presence string construction (the byte-exact surface)

The manager only *carries* `level_info`/`second_info`; the strings are built
by the callers (all in `oh-game`, using the engine's `name_format` /
`diff_format` / `time_format` — `hexagon_game.rs`, the C++
`nameFormat`/`diffFormat`/`timeFormat`, `HexagonGame.cpp:251,256,263`):

| site | C++ (citation) | `level_info` / `second_info` / `dead` |
|---|---|---|
| `HexagonGame::start` (not replay) | `HGUpdate.cpp:520-524` (in `start`, 504) | `nameFormat(name) + " [x" + diffFormat(diff) + "]"` / `nameFormat(getPackName())` / `false` (default) |
| `HexagonGame::start` (replay) | `HGUpdate.cpp:537-541` | `set_rich_presence_on_replay()` (no args) |
| `HexagonGame::death_updateRichPresence` | `HexagonGame.cpp:882-886` | `nameFormat(name) + " [x" + diffFormat(diff) + "]"` / `"Survived " + timeFormat(time) + "s"` / `true` |
| `MenuGame::init` | `MenuGame.cpp:796` | `set_rich_presence_in_menu()` (no args) |

Guards: `death_updateRichPresence` requires `window != nullptr` **and**
`!inReplay()` (`HexagonGame.cpp:847-857`); the in-game `start` sites are
inside the C++ `window != nullptr` implicit path (the `start` body runs only
when a level is active); the menu site is `MenuGame::init`.

### The `main.cpp` wiring (consumer sites)

- `main.cpp:264-269` — `std::optional<hg::Discord::discord_manager> discordManager;`
  then `if(!headless) discordManager.emplace();` (created **only when not
  headless**).
- `main.cpp:416` — `HexagonGame` gets
  `(discordManager.has_value() ? &*discordManager : nullptr)` (nullable
  pointer → the port's `Option<Arc<dyn Discord>>`).
- `main.cpp:432-434` — `SSVOH_ASSERT(discordManager.has_value());` then
  `mg.emplace(steamManager, *discordManager, …)` (the menu takes a
  non-null reference).
- `main.cpp:176,217` — the `-server` / headless-composition paths pass
  `nullptr /* discordManager */`.
- `main.cpp:451-454` — `mg->fnHGUpdateRichPresenceCallbacks = [&]{ hg.updateRichPresenceCallbacks(); };`

## The `DiscordCore` seam (the SDK surface)

`crates/oh-discord/src/core.rs` — the raw SDK methods `discord_manager`
calls, mirroring the `oh-workshop-uploader/src/steam.rs` `SteamUgc` pattern
(raw SDK surface; the manager layers the C++ behavior on top):

```rust
pub trait DiscordCore: Send {
    fn init(&mut self) -> bool;               // #1 Core::Create(…, NoRequireDiscord)
    fn set_log_hook(&mut self);               // #2 SetLogHook(Debug, …)
    fn register_command(&mut self, cmd: &str) -> bool;   // #3
    fn register_steam(&mut self, app_id: u32) -> bool;   // #4
    fn run_callbacks(&mut self) -> bool;      // #5 RunCallbacks() == Ok
    fn update_activity(&mut self, a: &Activity) -> bool; // #6/#7/#8 UpdateActivity
}
pub struct Activity { pub details: String, pub state: String, pub start: i64 }
```

`DISCORD_APP_ID: u64 = 725763266110029964` (Discord.cpp:23),
`STEAM_APP_ID: u32 = 1358090` (Discord.cpp:59),
`REGISTER_COMMAND = "SSVOpenHexagon.exe"` (Discord.cpp:49).

## The real IPC impl (`IpcDiscordCore`)

`crates/oh-discord/src/ipc.rs` — `discord-rich-presence` 1.1.0
(`DiscordIpcClient` + the `DiscordIpc` trait):

- `init` = `DiscordIpcClient::new(app_id)` + `.connect()` (pipe find +
  handshake). No local client → `find_pipe`/connect fails → `false`
  (graceful, identical to the C++ `Core::Create` failing).
- `update_activity` = `.set_activity(crate Activity)` (maps
  `details`/`state`/`start`); `start == 0` ⇒ the crate's `Timestamps` with no
  `start` (the C++ `SetStart(0)` "clear" semantics).
- `set_log_hook` / `register_command` / `register_steam` — documented no-ops
  (no IPC equivalent); `register_*` return `true` (the C++'s `Result::Ok`
  path, so the manager's "Successfully registered …" lines still print).
- `run_callbacks` — the IPC route has no callback queue; returns `true` once
  connected (the activity was already delivered by `set_activity`). This keeps
  the game's retry logic from spuriously "hanging" on a healthy IPC link.
  (Documented deviation: the C++ `RunCallbacks` also pumps join/invite events
  the game never installs handlers for.)

## MANUAL on-Discord checks (not run in CI)

CI is headless (no Discord client, no display): it proves the **degradation**
path (init fails / manager inert → game runs, 20-frame hang, no crash). The
positive path (a live Discord client showing "Selecting Level" /
"<Level> [x<diff>]" / "Watching Replay" / "Survived <t>s") needs a machine
with the Discord desktop client running:

1. `cargo run` (windowed) with Discord open → status shows
   "Selecting Level" in the menu.
2. Start a level → "<LevelName> [x<diff>]" (line 1) + pack name (line 2) +
   an elapsed-time bar.
3. Die → line 2 becomes "Survived <t>s" and the time bar clears.
4. Watch a replay → "Watching Replay", no time bar.
5. Close the Discord client mid-game → the 20-frame hang log
   ("Too many failed callbacks…") and the game keeps running.

## Oracle / verification

- `crates/oh-discord/tests/manager.rs` — the fake `DiscordCore` double pins:
  the init success/failure split, the exact `Activity` byte-strings for all
  three setters (menu / replay / in-game dead & alive), the
  `start == 0` clearing, `run_callbacks` delegation, and the manager's
  `initialized` gate (uninitialized → every setter returns `false` / no core
  call).
- `crates/oh-game` retry tests — 20 consecutive failures flip
  `discord_hung`; a success in between resets nothing (the C++ only increments
  on failure and never resets `discordAttempt`).
- Gate: `tools/gate.sh` (fmt/build/clippy `-D warnings`/test), Linux + the
  IPC impl compiles for Windows (`ipc_windows.rs` in the crate).
