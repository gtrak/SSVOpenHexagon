# lib-steamworks — `steamworks` 0.13.1 as used by `Core/Steam.cpp` (issue 20)

Source of truth: the checked-out crate source
`~/.cargo/registry/src/index.crates.io-*/{steamworks-0.13.1,steamworks-sys-0.13.0}/`
(recorded below, not from memory/docs.rs) + the vendored SDK headers
`public/steam/` (the same headers the C++ build includes —
`steam_api.h`, `steam_api_flat.h`, `steamencryptedappticket.h`) +
`_RELEASE/steam_appid.txt`. Rust port: `crates/oh-steam` (the `steamworks`
backend behind the crate's `steam` feature; the default build is the
no-SDK `NoSteam` backend).

## 1. The C++ API enumeration (every Steam SDK call in `Core/Steam.cpp`)

`Core/Steam.cpp` (1062 lines) — `steam_manager_impl` + its two free helpers.
Every direct SDK call, with line numbers:

| # | C++ call | line | where |
|---|---|---|---|
| 1 | `SteamAPI_SteamUser_v022()` | Steam.cpp:41 | `get_user_steam_id()` (flat C API — comment at :40: "Using C API here because C++ one doesn't work with MinGW") |
| 2 | `SteamAPI_ISteamUser_BLoggedOn(steam_user)` | Steam.cpp:43 | idem — the not-logged-in gate (logs "Attempted to retrieve Steam ID when not logged in\n", :45-47) |
| 3 | `SteamAPI_ISteamUser_GetSteamID(steam_user)` | Steam.cpp:51 | idem — `CSteamID{...}` |
| 4 | `SteamAPI_Init()` | Steam.cpp:58 | `initialize_steamworks()` |
| 5 | `SteamAPI_Shutdown()` | Steam.cpp:83 | `shutdown_steamworks()` (called by the impl destructor, :268-275, iff `_initialized`) |
| 6 | `SteamAPI_RunCallbacks()` | Steam.cpp:301 | `run_callbacks()` |
| 7 | `STEAM_CALLBACK(..., on_user_stats_received, UserStatsReceived_t)` | Steam.cpp:105-106 | member callback; body :150-158 (logs "Received user stats (rc: <m_eResult>)\n", sets `_got_stats = true` regardless of rc) |
| 8 | `STEAM_CALLBACK(..., on_user_stats_stored, UserStatsStored_t)` | Steam.cpp:107 | body :160-166 (logs "Stored user stats\n"; `(void)data`) |
| 9 | `STEAM_CALLBACK(..., on_user_achievement_stored, UserAchievementStored_t)` | Steam.cpp:108 | body :168-174 (logs "Stored user achievement\n"; `(void)data`) |
| 10 | `CCallResult<steam_manager_impl, EncryptedAppTicketResponse_t>` | Steam.cpp:125-126 | member; `.Set(handle, this, &on_encrypted_app_ticket_response)` at :684-686 — **only in the `#if __has_include("SSVOpenHexagon/Online/SecretSteamKey.hpp")` branch** |
| 11 | `SteamUGC()->GetNumSubscribedItems()` | Steam.cpp:200 | `load_workshop_data()` (default arg `bIncludeLocallyDisabled = false`) |
| 12 | `SteamUGC()->GetSubscribedItems(data, n)` | Steam.cpp:202 | idem |
| 13 | `SteamUGC()->GetItemInstallInfo(id, &itemDiskSize, folderBuf, 512, &lastUpdateTimestamp)` | Steam.cpp:216-218 | idem — `char folderBuf[512]` (:206-207) |
| 14 | `SteamUserStats()->RequestCurrentStats()` | Steam.cpp:283 | `request_stats_and_achievements()` |
| 15 | `SteamUserStats()->StoreStats()` | Steam.cpp:319 | `store_stats()` |
| 16 | `SteamUserStats()->SetAchievement(name.data())` | Steam.cpp:349 | `unlock_achievement()` |
| 17 | `SteamUserStats()->GetAchievement(name.data(), out)` | Steam.cpp:417 | `get_achievement()` |
| 18 | `SteamUserStats()->SetStat(name.data(), as_float)` / `SetStat(name.data(), data)` | Steam.cpp:397-398 | `set_and_store_stat()` — try float first, then int (comment :394-395: "Steam API seems to be bugged, and sometimes needs floats even for integer stats") |
| 19 | `SteamUserStats()->GetStat(name.data(), &as_float)` / `GetStat(name.data(), out)` | Steam.cpp:437,443 | `get_stat()` — try float first (→ `*out = as_float`, float→int narrowing), then int |
| 20 | `SteamFriends()->SetRichPresence("steam_display", "#InMenu")` | Steam.cpp:366 | `set_rich_presence_in_menu()` |
| 21 | `SteamFriends()->SetRichPresence("levelname", ...)` / `("difficultymult", ...)` / `("time", ...)` / `("steam_display", "#InGame")` | Steam.cpp:378-383 | `set_rich_presence_in_game()` — four calls, `&&`-chained (short-circuit) |
| 22 | `SteamUser()->RequestEncryptedAppTicket(&unSecretData, sizeof(unSecretData))` | Steam.cpp:681-682 | `request_encrypted_app_ticket()` — `unSecretData = 123456` (:679, `[[maybe_unused]] static`) — **`#if` branch only** |
| 23 | `SteamUser()->GetEncryptedAppTicket(rgubTicket, 1024, &cubTicket)` | Steam.cpp:752-754 | `on_encrypted_app_ticket_response()` — **`#if` branch only** |
| 24 | `SteamEncryptedAppTicket_BDecryptTicket(ticket, cubTicket, rgubDecrypted, &cubDecrypted, rgubKey, 32)` | Steam.cpp:769-771 | idem — `rgubKey[32]` = `#include "SSVOpenHexagon/Online/SecretSteamKey.hpp"` (:763-766), `k_nSteamEncryptedAppTicketSymmetricKeyLen` = 32 (`public/steam/steamencryptedappticket.h:13`) — **`#if` branch only** |
| 25 | `SteamEncryptedAppTicket_BIsTicketForApp(decrypted, cubDecrypted, SteamUtils()->GetAppID())` | Steam.cpp:776-777 | idem — **`#if` branch only** |
| 26 | `SteamEncryptedAppTicket_GetTicketSteamID(decrypted, cubDecrypted, &steamIDFromTicket)` | Steam.cpp:784-785 | idem — **`#if` branch only** |
| 27 | `SteamEncryptedAppTicket_GetUserVariableData(decrypted, cubDecrypted, &cubData)` | Steam.cpp:810-811 | idem — secret-data check :813-818 (mismatch only logs "Error: failed to retrieve secret data\n"; `_got_ticket = true` still happens, :819-820) — **`#if` branch only** |
| 28 | `EncryptedAppTicketResponse_t` result codes `k_EResultNoConnection` / `k_EResultDuplicateRequest` / `k_EResultLimitExceeded` / `k_EResultOK` | Steam.cpp:712,721,730,738,747 | idem — per-code log lines :713-741, `SSVOH_ASSERT` :747 — **`#if` branch only** |

Not present in `Steam.cpp` (contract checklist): **remote storage** (no
`ISteamRemoteStorage` calls anywhere), **overlay** (no
`ActivateGameOverlay*`; the in-game overlay is automatic when the loader
stub is linked), **rich-presence read**, **leaderboards**, **matchmaking**,
**auth-session tickets**. The only other Steam surface in the repo is the
workshop *tool* (`src/OHWorkshopUploader/main.cpp` — `CreateItem` /
`StartItemUpdate` / `SetItemContent` / `SetItemPreview` / `SubmitItemUpdate`,
port issue 19, seam in `oh-workshop-uploader/src/steam.rs`) — the game's
`steam_manager` never uses those.

### The ticket `#if` branch is INACTIVE in this tree

`SecretSteamKey.hpp` is **absent** from the repo (verified: no file matches
`SecretSteamKey*`; the only references are the two `__has_include` at
Steam.cpp:680,700 + the `#include` at :765). So this tree's build compiles
the `#else` arms:

- `request_encrypted_app_ticket()` (Steam.cpp:688-690): logs
  "Attempted to request encrypted app ticket without secret key\n", returns
  `false`. No SDK call, no call-result registration → the CCallResult
  handler (and its `#else` body, Steam.cpp:835-839: `_got_ticket_response =
  true; _got_ticket = false; _ticket_steam_id = nullopt;`) is **never
  invoked** (it only fires after `.Set(handle, ...)`, which is `#if`-gated).
- Consequences the port pins 1:1: `got_encrypted_app_ticket_response()` stays
  `false` forever, `got_encrypted_app_ticket()` stays `false`,
  `get_ticket_steam_id()` stays `nullopt`. The C++ online login
  (`HexagonClient.cpp:55-89`: `while(!_steamManager.got_encrypted_app_ticket_response())
  { _steamManager.run_callbacks(); }`) spins — equally in C++ and in the
  port; both builds need a key-carrying build for online login.

The `#if`-branch decryption flow (rows 22-28) is transcribed above for
completeness; the port does NOT port it (it cannot be built or tested
without the absent secret key, and no code path in this tree reaches it).
Re-porting it = hand-written FFI over `public/steam/steamencryptedappticket.h`
(rows 16-40: `BDecryptTicket`, `BIsTicketForApp`, `GetTicketSteamID`,
`GetUserVariableData`, `k_nSteamEncryptedAppTicketSymmetricKeyLen = 32`) + the
`#include`d 32-byte key + a key-injection seam.

### The C++ no-SDK build precedent (`SSVOH_ANDROID`)

Steam.cpp:961-1062 (`#else` of `#ifndef SSVOH_ANDROID`) is the C++'s own
"Steam absent" build: every manager method returns `false` / `nullopt` /
no-ops (`is_initialized` → `false`, `for_workshop_pack_folders` → empty,
`get_ticket_steam_id` → `nullopt`). (The C++'s `impl()` there is a
null-deref landmine — `SSVOH_ASSERT(_impl != nullptr)` over `_impl{nullptr}`;
the port's `NoSteam` keeps the observable all-false behavior without the
UB.) This is the precedent for the Rust default (feature `steam` OFF) build:
identical observable behavior, safe.

## 2. `steamworks` 0.13.1 mapping (C++ call → crate API)

`steamworks` 0.13.1 (Thinkofname/Noxime, `SteamManager::init()` →
`Client::init()`, src/lib.rs:249): init via `SteamAPI_InitFlat` +
`SteamAPI_ManualDispatch_Init` (lib.rs:251-276); dropping the last `Client`
runs `SteamAPI_Shutdown()` (lib.rs:571, `Manager::Client` arm) — the C++'s
explicit `SteamAPI_Shutdown()` maps to dropping the owned `Client`.

| C++ (row #) | `steamworks` 0.13.1 | notes (source line) |
|---|---|---|
| 1-3 user id + logged-on | `client.user().logged_on()` (user.rs:24-27, wraps `SteamAPI_ISteamUser_BLoggedOn`), `client.user().steam_id().raw()` (user.rs:14-17 → `SteamAPI_ISteamUser_GetSteamID`; `SteamId::raw() -> u64`, lib.rs:595) | the C++ used `SteamAPI_SteamUser_v022()` (MinGW comment); the crate's `Client::user()` (lib.rs:429-436) fetches the same `ISteamUser` — `SteamAPI_SteamUser_v023()` in sys. Same interface, newer vtable version; the loader returns one `ISteamUser` either way. |
| 4 init | `Client::init()` (lib.rs:249) → `SIResult<Client>` | the C++'s appid mechanism = `steam_appid.txt` in CWD — the crate does the same (lib.rs:239-241 doc: "placing a `steam_appid.txt` with the ID inside in the current working directory"). The repo's `_RELEASE/steam_appid.txt` = **1358090**. |
| 5 shutdown | drop the `Client` (lib.rs:571) | |
| 6 run callbacks | `client.run_callbacks()` (lib.rs:306) → manual-dispatch `SteamAPI_ManualDispatch_RunFrame` + `GetNextCallback` loop (lib.rs:160-181) | registered callbacks fire synchronously inside it; call-result completions are routed to `Callbacks::call_results` (lib.rs:166-174) |
| 7-9 stats callbacks | `client.register_callback(\|UserStatsReceived\| ...)` (lib.rs:336-343; payloads `stat_callback.rs:18-30` `UserStatsReceived { steam_id, game_id, result: Result<(), SteamError> }`, `:46` `UserStatsStored`, `:72` `UserAchievementStored`; dispatch ids `callback.rs:123-130`) | `result` is `Ok(())` iff `m_eResult == k_EResultOK` (lib.rs:72-78 `to_steam_result`) — the C++ handler logs the RAW `m_eResult` integer regardless, so the port must recover the integer (see Gap 1 below: the sys `EResult` enum is reachable; `SteamError` only carries non-OK). |
| 11-13 UGC | `client.ugc().subscribed_items(false)` (ugc.rs:624-638: `GetNumSubscribedItems` + `GetSubscribedItems`, sys linux_bindings.rs:25270,25276); `client.ugc().item_install_info(PublishedFileId(id))` (ugc.rs:664-681 → `SteamAPI_ISteamUGC_GetItemInstallInfo`, sys :25290) → `Option<InstallInfo { folder: String, size_on_disk: u64, timestamp: u32 }>` (ugc.rs:525-529) | the wrapper's folder buffer is **4096** bytes vs the C++'s **512** (Steam.cpp:206-207,216-218) — a path longer than 511 chars truncates in C++ but not in the port; all realistic folders are far shorter (documented deviation). |
| 15 store stats | `client.user_stats().store_stats()` (user_stats.rs:522-529 → `SteamAPI_ISteamUserStats_StoreStats`) | `Result<(), ()>` |
| 16 set achievement | `client.user_stats().achievement(name).set()` (user_stats.rs:633; stats.rs:57-68 → `SteamAPI_ISteamUserStats_SetAchievement`) | |
| 17 get achievement | `.get()` (stats.rs:33-45 → `SteamAPI_ISteamUserStats_GetAchievement`) | |
| 18-19 set/get stats | `set_stat_f32` / `set_stat_i32` (user_stats.rs:614,572), `get_stat_f32` / `get_stat_i32` (user_stats.rs:591,549) | the C++'s float-then-int fallback order is preserved by the MANAGER (two separate calls), not the crate |
| 20-21 rich presence | `client.friends().set_rich_presence(key, Some(value))` (friends.rs:205-215 → `SteamAPI_ISteamFriends_SetRichPresence`; `None`/empty → null pointer = "unset" per its doc) | |
| 25 app id | `client.utils().app_id()` (utils.rs:132 → `GetAppID`) | only needed by the (inactive) `#if` ticket branch |
| 22-28 ticket | — | see Gap 2 |

## 3. Gaps + fallbacks (the binding decision)

Decision: **`steamworks` 0.13.1 is the binding** — it covers every call the
ACTIVE build needs (rows 1-21 above) — with three documented gaps, each with
its fallback:

1. **`RequestCurrentStats` is missing from BOTH the wrapper AND the sys
   bindings.** The wrapper only documents it (user_stats.rs doc comments at
   :294,:520); the sys `ISteamUserStats` flat surface (linux_bindings.rs
   :23208-23404) has `GetStat/SetStat/GetAchievement/SetAchievement/
   StoreStats/...` but NO `RequestCurrentStats` — because the newer SDK the
   sys crate bundles has DEPRECATED it (`steamworks-sys-0.13.0/lib/steam/
   public/steam/isteamuserstats.h:91-95`: "Note: this call is no longer
   required as it is managed by the Steam client ... `// virtual bool
   RequestCurrentStats() = 0;`"). The repo's vendored SDK (what the C++
   builds against) still declares the flat symbol:
   `public/steam/steam_api_flat.h:432` `S_API bool
   SteamAPI_ISteamUserStats_RequestCurrentStats(ISteamUserStats* self);`.
   **Fallback (implemented in `oh-steam`)**: the symbol is looked up at
   RUNTIME with `dlsym(RTLD_DEFAULT, "SteamAPI_ISteamUserStats_
   RequestCurrentStats")` in the already-loaded loader (the `steamworks`
   link loads the stub first), then called with
   `steamworks_sys::SteamAPI_SteamUserStats_v013()`. A plain `extern "C"`
   link fails against the newer stub (`undefined symbol` — verified), so the
   runtime lookup is what keeps the build green while preserving the call:
   an OLD loader (the repo's vendored SDK — what the C++ links) still exports
   it → the real call, 1:1 with the C++; a newer loader without it →
   `false` (the "managed by the client" no-op the deprecation note
   prescribes).
2. **The encrypted-app-ticket surface is absent from the sys bindings**
   (no `SteamEncryptedAppTicket_*` symbols; the bindgen header set is
   `steam_api_flat.h` + `steam_gameserver.h` — `steamencryptedappticket.h`
   is not in it — and the wrapper's `CallbackResult` enum has no
   `EncryptedAppTicketResponse` variant, so that callback (id 154, sys
   `EncryptedAppTicketResponse_t_k_iCallback`, linux_bindings.rs:2473-2478)
   would be DROPPED by `CallbackResult::from_raw`'s catch-all `None`,
   callback.rs:141-143). Only `SteamAPI_ISteamUser_RequestEncryptedAppTicket`
   / `GetEncryptedAppTicket` (sys :21485,:21492) and the payload struct
   (sys :2470) exist. **Fallback**: not needed — rows 22-28 are the inactive
   `#if` branch (section 1). If a key-carrying build is ever required:
   hand-written FFI over `public/steam/steamencryptedappticket.h` + an open
   `Callback` impl (`steamworks`'s `Callback` trait is OPEN —
   callback.rs:148-151 `pub unsafe trait Callback { const ID: i32; unsafe
   fn from_raw(raw: *mut c_void) -> Self; }`, so a custom payload with
   `ID = 154` can be registered via `Client::register_callback` (lib.rs:336)
   and fired by `run_callbacks` — verified from source).
3. **`GetItemInstallInfo` folder buffer 4096 vs 512** (mapping row 13) —
   documented deviation; no fallback needed.

Also verified and NOT a gap: `Client::init()`'s failure (no Steam client
running / no appid) is a normal `Err` — the crate never panics or aborts on
a machine without Steam (lib.rs:260-264: non-OK `ESteamAPIInitResult` →
`Err(SteamAPIInitError)`). That is the runtime half of graceful degradation.

## 4. Link / load model (the "locatable at runtime exactly as the C++ expects")

- C++: links the official loader stub (`_RELEASE/steam_api64.dll` Windows;
  `libsteam_api.so` Linux); the stub finds the real client at runtime and
  `steam_appid.txt` in CWD supplies the appid when not launched through
  Steam. Without the *client*, `SteamAPI_Init()` simply returns `false`.
- `steamworks-sys` 0.13.0 ships the SAME official loader stubs in the crate
  (`steamworks-sys-0.13.0/lib/steam/redistributable_bin/linux64/
  libsteam_api.so` — an ELF loader, verified with `file`); its `build.rs`
  copies it to `OUT_DIR` and emits `cargo:rustc-link-lib=dylib=steam_api`
  (build.rs:46-60). Build-time linking therefore works on any machine
  (no Steam needed to COMPILE).
- RUNTIME: a linked binary needs `libsteam_api.so` findable by the dynamic
  loader (next to the binary / `LD_LIBRARY_PATH` — the C++'s `_RELEASE/
  steam_api64.dll`-next-to-exe layout). A machine without the stub CANNOT
  START a linked binary at all — that is why `oh-steam` gates the whole
  `steamworks` dependency behind the **`steam` cargo feature (default
  OFF)**: default builds (CI, the gate, servers/tools) link NO Steam and
  run the `NoSteam` backend (init fails → every manager method false/
  nullopt, the exact no-Steam C++ behavior). A Steam deployment builds with
  `--features oh-app/steam` (forwarded to `oh-steam/steam`) and ships
  `libsteam_api.so` + `steam_appid.txt` (1358090) next to the binary,
  exactly the C++ `_RELEASE` layout; there, a missing *client* degrades at
  runtime via `Client::init()`'s `Err`.
- The C++'s `SSVOH_ANDROID` no-SDK build (section 1) is the in-source
  precedent for the feature-OFF build.
- **VERIFIED EMPIRICALLY on this machine** (no Steam client; probe crate
  linking steamworks 0.13.1): (1) without the stub on the loader path the
  linked binary CANNOT START (`error while loading shared libraries:
  libsteam_api.so`, exit 127) — hence the feature gate; (2) with the stub
  resolvable (`LD_LIBRARY_PATH` to the crate's `redistributable_bin/
  linux64/libsteam_api.so`) but no client, `Client::init()` returns
  `Err(FailedGeneric("Failed to load module '.../steamclient.so'"))` and the
  process continues — the C++'s `SteamAPI_Init()`-false behavior, byte for
  behavior (the stub even probes `/home/.steam/sdk64/steamclient.so` like
  the C++'s `steam_api64.dll` probes the client registry).

## 5. `oh-steam` seam model (mirrors issue 19's `SteamUgc`)

`oh_steam::SteamSdk` = the raw SDK surface (the C++'s direct SDK calls, rows
1-21 above, minus the manager's own logic). `oh_steam::SteamManager` = the
`steam_manager_impl` port 1:1 (log lines verbatim, `_initialized` /
`_got_stats` / dedup sets / the hung-counter — the C++'s
`steamAttempt{1}`/`steamHung` (HexagonGame.hpp:90-92, the accounting at
HexagonGame.cpp:477-495) move INTO the manager's
`update_rich_presence_callbacks()`: identical observable behavior, since the
counter only ever reacts to THIS manager's `run_callbacks` result, and both
C++ call paths (the engine's per-frame `HGUpdate.cpp:155` and the menu's
`fnHGUpdateRichPresenceCallbacks`, main.cpp:451-454) hit the same HexagonGame
member — the port's single manager object). Backends: `NoSteam` (feature off
+ the init-failed stand-in, all-false per section 1's Android branch) and
`steamworks_sdk::SteamworksSdk` (feature `steam`). The online-server seam is
`impl oh_net::client::SteamManager for oh_steam::SteamManager` (the four
polling methods, oh-net/src/client.rs:461-470).

## 6. Composition wiring (issue 20 (c))

The C++ hands its `steam_manager` to the composition points in
`mainClient`; the port mirrors each:

| C++ call site | Port |
|---|---|
| `main.cpp:251` — `steam_manager steamManager;` (construct + `SteamAPI_Init`) | `oh_app::steam::steam_manager()` at the top of `main_client` (`client.rs`), held to the end of `mainClient` (the C++'s scope — drops last in the windowed teardown, `WindowedClient::run`'s destructuring) |
| `main.cpp:255-258` — iff `is_initialized()`: `request_encrypted_app_ticket()` + `request_stats_and_achievements()` + `run_callbacks()` | the same three calls, `is_initialized`-gated, in `client.rs` (the no-key ticket request logs + false — gap 2) |
| `main.cpp:415` — `HexagonGame{&steamManager, ...}` (first argument) | `HexagonGame::new(steam_manager: Arc<dyn SteamSink>, ...)` — the first parameter (the `oh_steam::SteamManager` implements `oh_game::seams::SteamSink`; the C++ `steamManager == nullptr` equivalent is `NoneSteamSink`) |
| `HGUpdate.cpp:147-149` — the 5 s timer's `set_rich_presence_in_game(nameStr, diffStr, timeStr)` | `update.rs` rich-presence block (timer accounting verbatim, call on the sink) |
| `HGUpdate.cpp:155` / `HexagonGame.cpp:477-495` — `updateRichPresenceCallbacks()` (Steam half + the `steamAttempt`/`steamHung` counter) | `update.rs::update_rich_presence_callbacks` → the sink → the manager (the counter lives in the manager — §5) |
| `HGUpdate.cpp:449-451,479-481,497-499` — `unlock_achievement("a22_swapdeath")` (3 swap-death sites, each `steamManager != nullptr`-gated) | the three `update.rs` sites, on the sink (the gate is the `NoneSteamSink` null-equivalent) |
| `HGScripting.cpp:850` — the Lua `unlockAchievement` | `GameApi::unlock_achievement` → the engine's `steam_manager` seam (`hexagon_game.rs`) |
| `main.cpp:434` — `MenuGame{steamManager, ...}` + `MenuGame.cpp:793-794` — the `init()` `set_rich_presence_in_menu()` + `update_hardcoded_achievements()` | `oh-menu` is extend-only, so the composition makes them at the same lifecycle point: `WindowedClient` (the `fnGoToMenu` consumption in `GameSlotAdapter::update_from_timer` + `boot`'s `gotoMenu` + `init_command_line_level` paths, `windowed.rs`) |
| `main.cpp:451-454` — `mg->fnHGUpdateRichPresenceCallbacks = [&]{ hg.updateRichPresenceCallbacks(); }` | the menu's `RichPresenceCallback` seam wired to `steam.update_rich_presence_callbacks()` (`windowed.rs`) |

Residual C++ call sites (NOT wired — documented in the contract's
"Residual C++ call sites" section): `MenuGame.cpp:335`
(`a35_eagerformore`, a menu-internal deep-scroll event; `oh-menu` is
extend-only), `MenuGame.cpp:2591` (`a23_login`, online-login success —
inert without the online client, issue 17), `HexagonGame.cpp:1063`
(`a24_onlinescore`, online score upload — inert without the online
client, issue 17).
