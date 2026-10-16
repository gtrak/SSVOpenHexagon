# Issue 21 — Discord integration

## Objective

Port `Core/Discord.cpp` (221 lines): the `discord_manager` — Discord Game SDK init,
rich presence updates (level/score/time), and graceful degradation when Discord is
absent.

## Key decisions

- Rust binding candidate: `discord-rich-presence` (IPC) or `discord-sdk` (the
  official Game SDK). The C++ uses the **Game SDK**
  (`discord_game_sdk.so`/`discord_game_sdk.dll`); match whichever gives the same
  behavior. Prefer a binding of the official SDK if it exists; otherwise use the
  IPC crate and document the difference.
- Degradation: the C++ has `discordHung`/`discordAttempt` retry logic — reproduce it.
  (D-014, verified: this retry is a `HexagonGame` member, NOT a `discord_manager`
  one — `include/.../HexagonGame.hpp:89,91` and `src/.../HexagonGame.cpp:495-506`
  (in `updateRichPresenceCallbacks`, HexagonGame.cpp:477-507); `Discord.cpp` has
  zero `discordHung`/`discordAttempt` references. So it is ported in `oh-game`
  (driving `Discord::run_callbacks`), not in `oh-discord`.)
- Rich-presence strings must match `death_updateRichPresence` / `updateRichPresenceCallbacks`.

## Files

| File | Change |
|---|---|
| `crates/oh-discord/Cargo.toml`, `src/lib.rs` | new |
| `.agents/skills/lib-discord.md` | new — from the real SDK header + crate source |

## Steps

1. Enumerate the Discord API calls in `Discord.cpp`.
2. Port `discord_manager` 1:1.
3. Port the retry/degradation logic and the presence strings.

## Verification

- With Discord absent: init fails gracefully, game continues (test).
- A trait/fake verifies presence update call sequence and exact strings.
- `tools/gate.sh` green; builds on Linux + Windows.

## Notes / risks

- Discord SDK native libraries must ship in `_RELEASE` as the C++ expects; copy the
  loader behavior (dlopen/LoadLibrary of the bundled `.so`/`.dll`).
