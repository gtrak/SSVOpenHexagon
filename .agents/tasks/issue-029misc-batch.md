# Task 029misc-batch — CLI boot, cursor, SIGINT, replay diagnostics, death logs, replay string bytes (29b4, 29u1, 29o1, 29h1, 29a4, 29m1)

Six small, mechanically-verifiable findings. Gate green at **900 tests**.
**No worktree** (D-035). One commit per item.

Evidence: `.agents/audits/main.md` (M1, M2, M3), `replay.md` (the diagnostics gap
+ the string/read items), `game.md` (M6/M7 + the surface gaps),
`update.md`. Citations below were re-derived.

---

## 1. 29b4 — `-p <pack> -l <level>` does not start the level  [uservisible-gameplay]

C++ chain: `mg->init(false, pack, name)` (`Core/main.cpp:507-510`) →
`MenuGame::loadCommandLineLevel` (`MenuGame.cpp:1688-1780`), whose **last
statement is `playSelectedLevel();`** (`:1778`) → `playSelectedLevel`
(`:2369-2383`) → the `fnHGNewGame` lambda (`main.cpp:442-449`:
`hg.newGame(...)` + `window->setGameState(hg.getGame())`). So the CLI flag starts
the level immediately, as the C++ itself documents ("to immediately run level").

The port's `load_command_line_level` (`oh-menu/src/menu_game.rs:1302-1395`) ends
at `change_state_to(States::LevelSelection)` — no `play_selected_level()` — and
`windowed.rs:2435` then **forces** the menu slot, so `-p -l` lands on the level
list instead of playing. Also note `windowed.rs:2431-2434`'s in-code
justification is **wrong**: it claims the C++ leaves a null game state and
crashes, which is true only of the C++'s *failure* path (invalid pack/level, early
returns at `:1701-1765`); the success path sets the state at `main.cpp:446-448`.
Fix that comment too.

**Do:** call the play path at the end of the load, and stop forcing the menu slot
when the CLI level booted successfully. Test both the success path (the level
starts) and the failure path (invalid pack/level → the menu, no crash).

## 2. 29u1 — `setMouseCursorVisible(false)` is missing from the play path  [internal]

C++ `playSelectedLevel` (`MenuGame.cpp:2373`) hides the cursor. The port's
`play_selected_level` (`menu_game.rs:686-701`) omits it, and
`Window::set_mouse_cursor_visible` (`oh-render/src/window.rs:768-770`) is a
flag-only store **never called from any menu/game path**. Wire it (both at
`play_selected_level` and wherever the game shows the cursor again — check the C++
for the counterpart). Test the flag toggles at the right points.

## 3. 29o1 — the missing SIGINT handlers  [internal]

C++ `main` installs the "without game window" handler **before any dispatch**
(`main.cpp:627-634`: log `Caught signal '<s>' without game window open,
exiting...` + `std::exit(1)`); it is only *replaced* in windowed mode
(`:340-350`). So a headless Ctrl-C logs and exits **1**. The port installs its
handler only in the windowed branch (`client.rs:159`), so headless Ctrl-C dies
silently with status 130. Also the server: the C++ `HexagonServer` ctor installs
a handler that prints `Caught signal %d\n` (`HexagonServer.cpp:1389-1390`); the
port's flag handler (`dispatch.rs:201-204`) prints nothing.

**Do:** install the headless handler, and add the server's print line. Test what
is testable without sending real signals (the handler's body/state), and say
plainly what can only be checked by hand.

## 4. 29h1 — the replay [de]serialization diagnostics are silent  [internal]

C++ `SSVOH_TRY` / `printTryFailure` prints `Failed [de]serialization operation
'…'` to stderr from 15 call sites (`Replay.cpp:25-41`). The port's `Writer`/
`Reader` fail silently (grep: no such print anywhere in `crates/oh-replay`). Port
the diagnostic at the same points, with the same message shape. This is the
replay pair's **only** missing behaviour, so getting it right closes that pair.

## 5. 29a4 — the death/replay log lines + the surface gaps  [internal]

- `HexagonGame.cpp:980`: the `ssvu::lo("Replay") << "Attempting to send and save
  replay...\n"` line in `death` — the port has it only inside `save_replay`
  (`hexagon_game.rs:1654`), not in `death`.
- `HexagonGame.cpp:1093-1094`: the success line `"Successfully saved new
  compressed replay file …"` — absent in the port (which logs only the failure).
- Surface gaps (M8–M12): `refreshTrigger` (`:461`), `getGame()` (`:1527`),
  `getColor(int)` (`:1537`), the pack accessors (`:1359-1376`), `getAssets()`
  (`:1611`). These are API-surface/relocated items with no C++ callers in some
  cases — **check each against its C++ call sites** and either add it or record
  precisely why the port's structure makes it unnecessary (the audit already notes
  `getGame` is the composition adapter's job). Do not add dead API for symmetry's
  sake.

## 6. 29m1 — the replay string bytes + the unbounded file read  [data-format edge]

Two related items in one commit:

- **Non-UTF-8 strings**: the C++ stores `player_name`/`pack_id`/`level_id` as
  `std::string` **bytes**; the port requires UTF-8 and fails to deserialize
  otherwise (`crates/oh-replay/src/lib.rs:19-23` documents this as an accepted
  deviation). Consequence: a legacy C++ `.ohr` with such a string is unreadable —
  a genuine wire-compat break. Decide the faithful fix (preserve the bytes, e.g. a
  byte-string representation, or lossily map in a way that round-trips) and
  **check what the C++ actually does when it re-reads and re-serializes such a
  name** — the answer determines the requirement. If a faithful fix is an
  API-wide change, implement the minimal correct thing and record precisely what
  remains.
- **The unbounded read**: `ReplayFile::deserialize_from_file` does
  `std::fs::read` (whole file) **before** the 2 MiB cap check
  (`lib.rs:740-751`), where the C++ reads into a bounded 2 MiB static buffer
  (`Replay.cpp:363-370`). A hostile/corrupt multi-GB file OOMs the port. Fix:
  bound the read (read at most the cap + 1 and fail past it) so behaviour matches
  the C++'s "fails, does not OOM". Test with a sparse/oversized file (do not
  commit a giant fixture — create it in the test's temp dir).

---

## Hygiene

- **The phone's ADB is DOWN** (`adb connect 192.168.10.138:38721` → refused; the
  host still pings it). No device work; do not add the temporary `debuggable`
  flag. Record device-visual items as **PENDING**.
- One commit per item, naming the row id. **Stage explicit paths — never
  `git add -A`** (shared checkout + index).
- Keep the gate green after each item (`bash tools/gate.sh`, expect 900 + yours).
- Refresh `deviations.md` rows (issue 28's rule); `.agents/audits/*.md` are
  committed reports — do not rewrite them.
- If the work overruns your context, STOP after a coherent item, commit what is
  green, and report what remains — never leave the tree mid-change.
- Append a `## Report`: per item — the fix, the verified citation, the tests, and
  what you could not establish.
