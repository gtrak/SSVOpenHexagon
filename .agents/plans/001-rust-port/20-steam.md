# Issue 20 — Steam integration

## Objective

Port `Core/Steam.cpp` (1062 lines): the `steam_manager` — Steam init, rich
presence, achievements (`steam_unlockAchievement`), stats, overlay, workshop,
and the encrypted app ticket used by the online server.

## Key decisions

- Rust binding candidate: `steamworks` crate. Read its real source and confirm it
  exposes the calls used (client init, user stats/achievements, friends/rich
  presence, UGC/workshop, app ticket).
- The C++ `steam_manager` degrades gracefully when Steam is absent
  (`steamHung`/`steamAttempt` logic in `HexagonGame`) — reproduce the same
  non-fatal behavior so the game runs on a machine without Steam.
- Steam SDK `.dll`/`.so` must be locatable at runtime exactly as the C++ expects
  (`_RELEASE/steam_api64.dll` etc.); copy the loading approach.

## Files

| File | Change |
|---|---|
| `crates/oh-steam/Cargo.toml`, `src/lib.rs` | new |
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

## Notes / risks

- Cannot fully test without a Steam client/appid; pin the call surface with the
  fake and document the untested paths.
- Keep Steam behind the `oh-steam` crate so non-Steam builds (servers/tools) do not
  link it.
