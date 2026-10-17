# Task 029j-batch — the joystick subsystem + the bind-name/hint surface (29j1, 29a3, 29a2)

Coherent batch: all three need the *same* missing thing — the config's bind
tables actually loaded in production, and a real poller behind the ported
joystick machine. Gate green at **807 tests**. **No worktree** (D-035). One
commit per item.

---

## 1. 29j1 — the joystick is a DOUBLE no-op  [uservisible-gameplay]

The audit (`joystick` pair) and the orchestrator agree on the shape:

- **The state machine IS ported and well-tested** — `crates/oh-config/src/joystick.rs`:
  `JoystickState`, `set_joystick_bind`, `JoystickPoll`, `button_pressed`,
  `update(deadzone, poll)`, `pressed_dir`, `rising_edge_dir`, `pressed(jid)`,
  `rising_edge(jid)`, `ignore_all_presses`. That is the C++ `Joystick` class's
  brain, faithfully translated.
- **Nothing ever fills the poll.** There is no platform poller at all: no crate
  reads a controller, and `gilrs` is not even a dependency (verified). The C++
  gets its input from SFML (`sf::Joystick`).
- **The game call sites don't use the machine anyway.** The engine talks to the
  `Joystick` seam (`crates/oh-game/src/seams.rs:354-363`: `update(deadzone)`,
  `pressed_dir`, `pressed_id`, `rising_edge_id`), whose **only** implementation
  is `NoJoystick` (always false). So even with a poller, the game would ignore it.
- **The 12 config binds are never loaded into the machine in production.** The
  C++ calls `Config::loadAllJoystickBinds()` in the game ctor
  (`HexagonGame.cpp:448`); the port never calls it (grep: no call site in
  `oh-game`/`oh-app` — the function exists in `oh-config` and only tests call it).
- **The `risingEdge` handlers are unported**: the C++ has 11 in
  `MenuGame.cpp` and 4 in `HGUpdate.cpp` (Exit / ForceRestart / Restart /
  Replay…). Verify the exact set yourself — `grep -rn risingEdge src/` — and port
  the ones that drive game/menu actions.

**Do:**
1. Add a real poller behind the seam — a `RealJoystick` in `oh-app` (the
   `RealProfiles` precedent) that updates the `oh-config` machine each frame from
   a desktop backend. `gilrs` is the natural choice, **desktop-only**: cfg-gate it
   to the desktop targets and keep `NoJoystick` on Android/wasm, documenting
   that. Check the backend's system requirements (Linux may pull a udev/libudev
   dependency) and, if it does, say so in the report and CI docs — do not
   silently add a system library requirement to the gate.
2. Wire the shipping desktop composition to it (one controller = index 0; find
   how the C++ treats a single joystick and mirror it, including the deadzone
   source in the config).
3. Load the binds in production (see 29a3 below — do it once, here).
4. Port the `risingEdge` handlers.

**Tests (host, no hardware):** drive the ported machine with synthetic
`JoystickPoll` snapshots and assert the edges/actions fire — including
rising-edge-vs-held semantics and the `ignore_all_presses` gate. The poller
itself should be a thin, untested wrapper over gilrs (say what you could not
test without a physical pad).

## 2. 29a3 — the ctor config side effects are never called (the joystick half)  [internal]

The C++ game ctor calls `Config::keyboardBindsSanityCheck()`
(`HexagonGame.cpp:345`) and `Config::loadAllJoystickBinds()` (`:448`). **Neither
is ever called in the port** (`oh-config` defines both; only tests use them).
`loadAllJoystickBinds` is the one 29j1 needs, so this is mostly the same change —
but make sure BOTH are live, at the same point in the lifecycle the C++ calls them
(the game ctor, i.e. wherever the port builds the engine), and that
`keyboardBindsSanityCheck`'s repair semantics match the C++ (read it: it FIXES
invalid binds, it is not just a validator). Test both.

## 3. 29a2 — the restart/replay input-hint strings are never built (M1)  [uservisible-cosmetic, user-visible]

C++ `newGame` builds `status.restartInput` / `status.replayInput` in a
`if(window != nullptr)` block (`HexagonGame.cpp:758-821`):
`getKeyboardBindNames(Tid::Restart/Replay)` (`:763-764`),
`getJoystickBindName(Jid::Restart/Replay)` (`:781`, `:801`), and assembles
"PRESS <keys> TO RESTART", an "OR JOYSTICK <btn>" line, and the
"PRESS … TO REPLAY" / "NO … BUTTON SET" variants. The port has only a comment
(`hexagon_game.rs:950-953`: *"windowed-only (and Config bind-name plumbing, issues
10/15); headless leaves … empty"*), and `update_text` reads the fields
(`update.rs:1395-1396`), so a windowed player who dies gets **no** restart/replay
hint.

The bind-name plumbing already exists — `oh-config`'s
`get_keyboard_bind_names` (`config.rs:2106`) and `get_joystick_bind_name`
(`:1970`) — it is simply never called from `new_game`. Port the C++ block
verbatim (all the string variants and the exact spacing/newlines), gated on
`window` like the C++, and test the assembled strings (including the
no-bind-set variants) — this is exactly the kind of string that must match the
C++ character-for-character.

---

## Hygiene

- **The phone's ADB is DOWN** (`adb connect 192.168.10.138:38721` → refused; the
  host still pings it). No device work; do not add the temporary `debuggable`
  flag. Anything hardware-dependent (a physical gamepad, on-device behaviour) goes
  in the report as **not tested**, precisely.
- One commit per item, naming the row id. **Stage explicit paths — never
  `git add -A`** (shared checkout + index).
- Keep the gate green after each item (`bash tools/gate.sh`, expect 807 + yours).
  If gilrs adds a build requirement, the gate must still pass — if it can't,
  stop and report rather than leaving the gate red.
- Refresh `deviations.md` rows (issue 28's rule); `.agents/audits/*.md` are
  committed reports — do not rewrite them.
- Append a `## Report`: per item — the fix, the verified citation, the tests, and
  what you could not establish.
