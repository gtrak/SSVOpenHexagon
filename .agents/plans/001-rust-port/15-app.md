# Issue 15 — `oh-app`: boot, game loop, CLI, modes

## Objective

Port `Core/main.cpp`: argument parsing (`-p`, `-l`, `-printLuaDocs`, `-headless`,
`-server`, ...), folder creation, window creation, the fixed-timestep game loop,
event handling, screenshot (F12), signal handling (SIGINT), headless mode, server
mode, and the `MenuGame`↔`HexagonGame`↔`HexagonClient` wiring. Produce the
`open-hexagon` binary.

## Key decisions

- Copy the frame loop exactly: `TIME_STEP = 60/240`, accumulator, timescale,
  vsync/fps limiting. The C++ uses `ssvs::GameWindow` timers (static/dynamic);
  reproduce the same accumulation.
- CLI flags and semantics copied verbatim (including `-headless` behavior for tests).
- The `_RELEASE` directory is the working directory for assets/config/packs.
- Signals: graceful shutdown on interrupt.

## Files

| File | Change |
|---|---|
| `crates/oh-app/Cargo.toml` | new — binary `open-hexagon`; deps `oh-*` |
| `crates/oh-app/src/{main,args,loop,screenshot,signals}.rs` | new |

## Steps

1. Port argument parsing (exact flags, `cliLevelPack`, `cliLevelName`,
   `printLuaDocs`, `headless`, `server`).
2. Port `createFolderIfNonExistant` and boot-time folder setup.
3. Port the game loop (fixed timestep, per-frame update, `MenuGame`/`HexagonGame`
   dispatch, FPS text).
4. Port event pump → input state → trigger refresh.
5. Port screenshot and signal handling.
6. Wire the callbacks (`fnGoToMenu`, `fnHGNewGame`, `fnHGTriggerRefresh`,
   `fnHGUpdateRichPresenceCallbacks`).

## Verification

- `open-hexagon -headless -p base -l <level>` runs N ticks and exits 0; assert the
  same score as a C++ headless run for a fixed seed (or exit cleanly if the C++
  headless path differs — document).
- `open-hexagon -printLuaDocs` writes the expected doc file.
- A scripted PTY check: launch the game, wait for first frame, send quit, assert
  exit 0 (not timeout).
- `tools/gate.sh` green; both Linux and Windows builds compile.

## Notes / risks

- Headless must not require a GPU/window; the renderer seam from issue 09/10 makes
  this possible — verify `-headless` needs no display on Linux (no X/Wayland).
