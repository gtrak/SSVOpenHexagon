# Task spec — issue 015 (step c): the windowed game loop (timer, game-state switching, screenshot, SIGINT)

You are a worker. Implement exactly this. Do not redesign or widen scope.
**No egui** (that is the 14b lane, dispatched separately). You complete the
windowed path so `open-hexagon` (no args) actually runs the game.

## What already exists (verified — do not re-derive or duplicate)

- `oh_render::Window` (`crates/oh-render/src/window.rs`) — the full winit 0.30
  backend: `create`, config setters, `InputState`, SFML `Event` mapping,
  `save_screenshot` (STUB with `_path` — see scope 6), `get_fps`, and
  `run(on_frame)` driving `about_to_wait` (the winit 0.30 frame point).
  It has **NO timer and NO game-state concept** — that is this task.
- `oh_app::bridge::draw` (15a) and the `open-hexagon` binary with headless +
  printLuaDocs + stubs (15b, `crates/oh-app/src/{client,dispatch,args}.rs`).
- The engine update loop (issue 09, `HexagonGame::update`), the MenuGame port
  (issue 13 — check what update/draw/input surface it exposes), and the
  trigger-input seam `Tid::*` values referenced in `HexagonGame.cpp:403-424`.

## C++ oracle (read these first; cite line numbers for everything you port)

1. `extlibs/SSVStart/include/SSVStart/GameSystem/Timers/TimerStatic.hpp` — the
   EXACT accumulation: `time += frameTime; while(time >= timeSlice && loops <
   maxLoops) { updateFromTimer(step); time -= timeSlice; ++loops; }` with
   `maxLoops` default 50 (verify in the header) and `reset()` zeroing both.
2. `extlibs/SSVStart/include/SSVStart/GameSystem/GameWindow.hpp` +
   `GameWindow.cpp` (if present) — `runEvents` ORDER: event polling, input
   processing (`Input::refresh*`/trigger dispatch), timer `runUpdate`, draw.
   Port that order verbatim; the port's `Window::run` callback replaces
   `pollEvents` (events already folded in by `RunHandler`).
3. `extlibs/SSVStart/include/SSVStart/GameSystem/GameEngine.hpp` —
   `setGameState`/`updateFromTimer(FT)`/`draw`/state has-bools.
4. `include/SSVOpenHexagon/Global/Config.hpp:25-27` — `TICKS_PER_SECOND =
   240.f`, `TIME_STEP = TIME_SLICE = 60/240`.
5. `src/SSVOpenHexagon/Core/main.cpp:297-350` — window setup order (title
   `:303`, size `:304`, pixelMult `:305`, fullscreen `:306`,
   antialiasing `:307`, vsync `:308`, FPS-limited `:309`, maxFPS `:310`,
   icon `:312-328`, `setTimer<TimerStatic>(TIME_STEP, TIME_SLICE)`
   `:331-332`, the SIGINT handler `:335-350` calling `window->stop()`
   `:347`).
6. `src/SSVOpenHexagon/Core/main.cpp:413-463` — the engine (`:413-421`),
   the MenuGame construction (`:426-434`) and the
   MenuGame↔HexagonGame callbacks: `fnHGTriggerRefresh` `:436-440`,
   `fnHGNewGame` (calls `hg.newGame(...)` `:446` then
   `window->setGameState(hg.getGame())` `:448`) `:442-449`,
   `fnHGUpdateRichPresenceCallbacks` `:451-454`, `fnGoToMenu`
   `:456-462` (menu `returnToLevelSelection()` `:458` + `init(error)`
   `:459` + `setGameState(mg->getGame())` `:461`), the boot dispatch
   `:472-535` (`gotoMenu` `:476-480`, `-p/-l` via
   `mg->init(false, *cliLevelPack, *cliLevelName)` `:507-511`, the
   drag&drop compressed-replay path `gotoGameCompressedReplay`
   `:482-503` — note: the C++ `return`s on decompress-failure `:491` with
   NO state set, and the `-p -l` path sets no state either (null
   `gameState` → first-frame `assert(isValid())`); the port points the
   window at the menu on both paths — documented deviation), and
   `window->run()` `:588-592`.

## Scope

### 1. Timer + game-state machinery (oh-render or oh-app — your call, justify)

Port `TimerStatic` semantics EXACTLY (accumulation, maxLoops 50, reset) and a
minimal game-state switching surface equivalent to
`window->setGameState(...)` — the C++ `GameEngine::updateFromTimer` calls the
CURRENT state's update with the FT. Keep it small: two states in practice
(menu, game). Use the real monotonic clock for `frameTime`; do NOT invent a
fixed frame time.

### 2. Wire the engine into the loop

Per frame, in the C++ `runEvents` order: (a) input state is current (the
`RunHandler` already applied events — verify nothing else is needed), (b)
timer accumulation → N × current-state `update(FT)` (the engine's `update`
path from issue 09 with the WINDOW input seam: the real window's
`InputState`/joystick seam replaces the test fakes), (c) draw — the game
state's `draw()` via the 15a bridge (`oh_app::bridge::draw`), the menu
state's draw via its own path (check what issue 13/11 provide for MenuGame
drawing — `oh_render::graphics::draw_background_menu` exists; if MenuGame's
full draw is not ported, wire what exists and leave an explicit, named
`TODO(issue-13-menu-draw)` marker — do NOT fake it).

### 3. The windowed `main_client` path (replaces the 15b stub)

`main.cpp:244-596` windowed flow: window creation + config setters + icon
(`Assets/icon.png`), timer install, SIGINT (`std::signal` analogue — a
simple atomic-flag handler calling the loop's stop; note the C++ TODO(P2) UB
comment), ImGui init/shutdown = NOT in this lane (14b owns the console;
the C++ `Imgui::initialize` line gets a `TODO(14b)` marker), assets, audio
(no-op seam — issue 20/21 land Steam/Discord; pass the None equivalents the
seams already accept; audio: `oh-render/src/audio.rs` exists — check its
surface and wire it if it is complete, else document), HexagonClient (issue
17 — null equivalent), MenuGame + the four callbacks, boot dispatch
(gotoMenu / `-p -l` / compressed-replay drag&drop — the replay path exists
from 15b), `window->run()`, exit 0.

### 4. Screenshot

`mustTakeScreenshot` is already set/consumed through `draw_frame` (15a). Make
`Window::save_screenshot` real: GL `glReadPixels` of the current framebuffer
(bottom-up row flip like SFML) → PNG to the path. Verify against
`extlibs`/SFML's `RenderTexture`/`saveToFile` or the fork's
`GameWindow::saveScreenshot`; cite what you read. A headless test can assert
the flip + format only if you can render offscreen — otherwise a
skip-with-reason test (D-011 precedent) plus a manual-verify note.

### 5. Tests (headless-first)

- Timer unit tests: accumulation slices, maxLoops cap, reset — values from
  the C++ header defaults.
- State switching: a fake two-state harness proving
  `setGameState` + `updateFromTimer` route update/draw to the right state in
  the right order.
- The `-p -l` boot dispatch and the compressed-replay boot path with a FAKE
  window (the engine+menu composed, no real display — the same pattern as
  `tests/determinism.rs`): assert `newGame` was reached / the replay was
  loaded.
- The real windowed run is a MANUAL check (no display in CI): document the
  exact command in the tracker row (`cargo run` + what to look for), and
  gate the on-screen behaviour behind skip-with-reason.

## Process requirements (mandatory — 4 of 5 prior lanes timed out at 30 min)

1. Commit after every green milestone: (a) timer + tests; (b) state machinery
   + tests; (c) windowed main_client wiring; (d) screenshot + SIGINT; (e)
   full gate. `cargo test -p <crate>` after each file.
2. Fix clippy `-D warnings` immediately per milestone. Never hold a lock
   guard across a re-acquiring call.
3. Verify every C++ citation against the real source (D-014); fix the spec in
   your commit if wrong and say so.
4. Update `.agents/plans/STATUS.md` row 15 in your final commit.

## Out of scope

- egui / the Lua console overlay (14b lane).
- Real Steam/Discord/audio/net (17/18/20/21).
- Simulation changes (the D-019 fix and determinism harness are untouchable);
  headless behaviour must remain byte-identical.
- Windows-specific packaging (issue 22/24).
