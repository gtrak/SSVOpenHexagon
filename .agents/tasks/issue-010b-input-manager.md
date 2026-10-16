# Task spec — issue 010b: the SSVStart input manager (keyboard/mouse play — playability blocker)

You are a worker. Implement exactly this. Do not redesign or widen scope.

## Why this exists

The 15c windowed wiring landed the real loop but **keyboard/mouse play is
unavailable**: the C++ input path is SSVStart's input system
(`extlibs/SSVStart/include/SSVStart/Input/` — `Enums.hpp`, `InputState.hpp`,
`Input.hpp`, `Bind.hpp/.inl`, `Combo.hpp/.inl`, `Trigger.hpp`,
`Manager.hpp`; ~470 lines) which the port does not have. The game's controls
come from `oh-config` binds (`Config::getBinds()` — landed, issue 04) which
are ssvs `Input::Trigger`s over `Combo`s of keys/buttons. Without this, no
human can play.

## What exists (verified — reuse)

- `crates/oh-render/src/input.rs` — `InputState` (real-time key/button/finger
  bitmaps), winit→SFML `Event` mapping, already fed by `Window::run`'s
  `RunHandler`. The C++ `InputState.hpp` is the same concept — diff your port
  against the real header and extend if fields are missing.
- `crates/oh-config` — the binds (landed issue 04; check the `Bind`/`Trigger`
  shape it exposes vs `extlibs/SSVStart/.../Trigger.hpp`/`Combo.hpp`).
- The engine's trigger ids (`Tid::*`, `HexagonGame.cpp:403-424` — Screenshot,
  LuaConsole, Pause, Restart/FullRestart, Replay, Exit...) and `updateInput`
  (issue 09); the MenuGame triggers (issue 13); `open-hexagon`'s windowed
  loop (15c, `crates/oh-app/src/windowed.rs`).

## Scope

1. Port the SSVStart input system into `oh-render` (or a spot mirroring the
   codebase's seam structure — same crate as `input.rs`): `Combo` (key+button
   set, `==`/subset semantics from `Combo.hpp/.inl`), `Trigger` (a
   `std::vector<Combo>` — Trigger.hpp:16-51; NO type/callbacks on it),
   `Bind` (the trigger + `Type` — `Always`/`Once`, NOT `Once`/`Continuous`
   (Enums.hpp:12-16) — + `Mode` + the on/off (fire/tick) callbacks + the
   release latch; `Bind::update`/`refresh` semantics from `Bind.hpp/.inl`),
   and the `Manager` (per-frame refresh + dispatch — the per-frame order is
   `GameEngine::runUpdate` (GameEngine.hpp:80-86: `refreshInput` ONCE per
   frame, before the timer burst) + `GameEngine::updateFromTimer`
   (GameEngine.hpp:40-50: each tick's `updateInput` BEFORE the state's
   `update`), via `GameState::updateInput`/`refreshInput`
   (GameState.hpp:49-57); `GameWindow` has no `processInput`). Unit tests
   over a synthetic `InputState` for: combo matching, Always-vs-Once,
   key-release transitions, multi-combo binds.
2. Wire the binds: read the live `Config::getTrigger(tid)` at each
   registration (the C++'s `HexagonGame`/`MenuGame` `addTidInput`/menu
   `addInput` sites — HexagonGame.cpp:349-460, MenuGame.cpp:912-1033) into
   the trigger set, so rebind / `refreshBinds` re-point the binds without
   re-registration. The windowed loop feeds the real `InputState`; headless
   keeps the replay-input path (`must_replay_input`) EXACTLY as landed —
   input from the live window MUST NOT leak into headless/replay runs.
3. Connect the engine's `Tid::*` triggers (issue 09's `updateInput`) and the
   MenuGame's actions so that pressing the real keys drives
   restart/pause/screenshot/replay/menu navigation. `Tid::Screenshot` sets
   `must_take_screenshot` (already consumed by `draw_frame`); `Tid::LuaConsole`
   calls the landed `set_ilc_show_console_next` (debug-gated); `Tid::Pause`/
   Restart/FullRestart per the C++ bodies (cite).
4. Tests: headless-first — trigger semantics unit tests; a composed test
   driving the windowed loop with a scripted `InputState` sequence (the fake
   window pattern from `tests/windowed.rs`) asserting e.g. Restart triggers
   `newGame`, Pause sets `debugPause`/pauses ticking per the C++ body, and
   the menu PLAY action starts a level. The real keyboard check stays a
   documented MANUAL step (no display in CI).
5. Final: full `bash tools/gate.sh` (4 stages, print `GATE_EXIT=<rc>`, never
   pipe to tail), `cargo test --workspace`,
   `cargo test --workspace --features oh-game/produce-lua-metadata`, and the
   `.agents/plans/STATUS.md` (row 10 note + the 15c row's
   "input manager unported" caveat cleared) in your final commit.

## Process rules (mandatory)

- Budget: 90 minutes. Commit after EVERY green milestone — suggest: (a)
  Combo/Bind + tests; (b) Trigger + Manager + tests; (c) binds wiring +
  engine/menu connection; (d) loop integration + tests + gate. `cargo test
  -p <crate>` after each file; clippy (`cargo clippy --workspace
  --all-targets -- -D warnings`) fixed immediately per commit.
- Verify every C++ citation against the real source (D-014); fix the spec in
  your commit if wrong and say so.
- If the full menu-trigger surface will not fit, land the ENGINE triggers
  (play: restart/pause/replay/screenshot) first — menu navigation actions
  second — and leave explicit `TODO(010b-<name>)` markers otherwise.

## Out of scope

- Joystick/gamepad (the C++ has joystick support; port it only if trivially
  reachable via the existing `oh-config` joystick data — otherwise a
  documented TODO), touch/finger beyond the existing `InputState` fields,
  the egui console overlay (14b), screenshot pixel readout (15c tail),
  `crates/oh-net/**` / `crates/oh-menu` draw (parallel lanes in flight).
