# Issue 20 — Steam integration

## Objective

Port `Core/Steam.cpp` (1062 lines): the `steam_manager` — Steam init, rich
presence, achievements (`steam_unlockAchievement`), stats, overlay, workshop,
and the encrypted app ticket used by the online server.

## Key decisions

- Rust binding candidate: `steamworks` crate. Read its real source and confirm it
  exposes the calls used (client init, user stats/achievements, friends/rich
  presence, UGC/workshop, app ticket).
  - **VERIFIED (issue-20 (a), `.agents/skills/lib-steamworks.md`)**: covers every
    ACTIVE call (rows 1-21 of the skill's enumeration), with documented gaps:
    `RequestCurrentStats` missing from wrapper AND sys bindings AND the
    bundled loader stub (the newer SDK the sys crate bundles deprecated the
    call — a plain link fails with `undefined symbol`); fallback: a
    hand-written `dlsym` binding resolves the flat symbol from the
    already-loaded loader at runtime (the vendored SDK the C++ links still
    exports it — `public/steam/steam_api_flat.h:432` — old loader → the real
    call, newer loader → `false`, the deprecated no-op), and the
    encrypted-app-ticket surface absent from the sys bindings — moot in
    this tree because the ticket's decryption branch is the inactive
    `#if __has_include("SSVOpenHexagon/Online/SecretSteamKey.hpp")` arm (the
    header is absent from the repo; the active `#else` path is ported 1:1).
    The `steamworks` dependency is feature-gated (`oh-steam/steam`,
    `oh-app/steam`, default OFF = the C++'s `SSVOH_ANDROID` no-SDK build,
    Steam.cpp:961-1062) — verified: a linked binary cannot start on a
    machine without the loader stub, so the gate's no-Steam path requires
    the OFF-by-default build.
- The C++ `steam_manager` degrades gracefully when Steam is absent
  (`steamHung`/`steamAttempt` logic in `HexagonGame`) — reproduce the same
  non-fatal behavior so the game runs on a machine without Steam.
- Steam SDK `.dll`/`.so` must be locatable at runtime exactly as the C++ expects
  (`_RELEASE/steam_api64.dll` etc.); copy the loading approach.

## Files

| `crates/oh-steam/Cargo.toml`, `src/lib.rs` | new |
| `crates/oh-steam/src/{sdk,manager,no_steam}.rs` | new — the `SteamSdk` seam + the `steam_manager` port + the no-SDK backend |
| `crates/oh-steam/src/steamworks_sdk.rs` | new — the `steam`-feature real backend (incl. the `dlsym`-resolved `RequestCurrentStats` fallback) |
| `crates/oh-steam/tests/{manager,workshop_cache}.rs` | new — the fake-SDK pinning tests (no Steam required) |
| `crates/oh-game/src/seams.rs` | `SteamSink` trait + `NoneSteamSink` (the C++ `steamManager == nullptr` equivalent) |
| `crates/oh-game/src/hexagon_game.rs` | the C++'s first-constructor-argument `steam_manager` field + `GameApi::unlock_achievement` wired to it (the Lua `unlockAchievement`, HGScripting.cpp:850) |
| `crates/oh-game/src/update.rs` | the C++ call sites: the 5 s rich-presence timer + `set_rich_presence_in_game` (HGUpdate.cpp:147-149), `updateRichPresenceCallbacks`'s Steam half (HexagonGame.cpp:477-495), the three `a22_swapdeath` sites (HGUpdate.cpp:449-451,479-481,497-499) |
| `crates/oh-app/src/steam.rs` | new — the `mainClient` Steam manager (main.cpp:251-258) + feature backend selection |
| `crates/oh-app/src/client.rs` | the manager at the C++'s position (before the folders), the initialized-gated ticket/stats/callbacks calls, shared with the engine + the windowed composition |
| `crates/oh-app/src/windowed.rs` | the engine seam + the `fnHGUpdateRichPresenceCallbacks` closure (main.cpp:451-454) + the `MenuGame::init`-lifecycle menu Steam calls (MenuGame.cpp:793-794) at the composition's `init`/`init_command_line_level`/`fnGoToMenu` points (oh-menu is extend-only) |
| `crates/oh-app/src/compose.rs` | the `steam` parameter through `compose_with(_config)` into `HexagonGame::new` |
| `.agents/skills/lib-steamworks.md` | new — from real source |

## Steps

1. Enumerate every Steam API call in `Steam.cpp` (transcription worker output in
   `mapping/thirdparty-cpp.md`/`networking.md`).
2. Port `steam_manager` methods 1:1 over the chosen binding.
3. Port the graceful-degradation logic and the init retry behavior.
4. Port rich-presence display strings exactly.

## Verification

- With Steam absent: init fails gracefully, game continues, no panic (test).
- A trait-based fake Steam verifies the call sequence for achievements/rich presence.
- `tools/gate.sh` green; builds on Linux + Windows.

## Residual C++ call sites (documented, out of this issue's wiring scope)

- `MenuGame.cpp:335` — `unlock_achievement("a35_eagerformore")`: a
  menu-INTERNAL event (the level-list deep-scroll); `oh-menu` is
  extend-only, so the menu cannot carry the seam.
- `MenuGame.cpp:2591` — `unlock_achievement("a23_login")`: online-login
  success — inert without the online client (issue 17).
- `HexagonGame.cpp:1063` — `unlock_achievement("a24_onlinescore")`:
  online score upload — inert without the online client (issue 17);
  the C++ `HGScripting.cpp:850` Lua `unlockAchievement` IS wired
  (`GameApi::unlock_achievement` → the engine's `steam_manager` seam).

## Notes / risks

- Cannot fully test without a Steam client/appid; pin the call surface with the
  fake and document the untested paths.
- Keep Steam behind the `oh-steam` crate so non-Steam builds (servers/tools) do not
  link it.
