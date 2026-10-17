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

---

## Report (all six items — one commit each)

**29b4** (uservisible-gameplay, `8fbe6bf1`) — the `-p <pack> -l <level>` CLI boot now STARTS the level.
- **Fix**: `Shared::load_command_line_level` (oh-menu `menu_game.rs`) gained the C++'s last statement — `play_selected_level()` (MenuGame.cpp:1778) — after the `change_state_to(LevelSelection)` tail; the `init_command_line_level` / `load_command_line_level` pair now returns the C++ `bool` result, and the windowed boot's `-p -l` branch (oh-app `windowed.rs`) captures it and forces `GameSlot::Menu` ONLY on failure (the success path lands in the Game slot via the `new_game` callback, the C++ `fnHGNewGame`, main.cpp:442-449). The in-code + module-docs "null game state → first-frame assert" comment was reworded: that claim is true only of the C++ FAILURE path (invalid pack/level, the early returns at MenuGame.cpp:1701-1765); the success path sets the state at main.cpp:446-448.
- **Citation**: MenuGame.cpp:1778 (the tail `playSelectedLevel()`), :1688-1780 (`loadCommandLineLevel`), :2369-2383 (`playSelectedLevel`); main.cpp:442-449 (`fnHGNewGame`), :96-97/:104-105 ("-p/-l to immediately run level"), :446-448 (the success-path state set).
- **Tests**: oh-menu `boot.rs` +1 (the tail fires the `new_game` seam exactly once with the boot-selected level's args; the menu state is the tail's pre-play `LevelSelection`); oh-app `windowed.rs`: the old "lands in LevelSelection" boot test REPLACED by `boot_with_p_and_l_starts_the_level_immediately` (success → `GameSlot::Game`) + new `boot_with_an_invalid_pack_or_level_lands_in_the_menu` (BOTH failure arms → menu, no crash); the menu-state tests that used `-p -l` as a LevelSelection shortcut now drive the C++'s normal user flow through a data-root sandbox.
- **Not established**: the on-device `-p -l` boot (the phone's ADB listener was down for the batch — no device work, no temporary `debuggable` flag; the host tests pin the seam contract end-to-end over the real `_RELEASE` assets; the device confirmation is PENDING).

**29u1** (internal, `2dbcdd9c`) — `setMouseCursorVisible` is wired end-to-end.
- **Fix**: new `MenuWindow::set_mouse_cursor_visible` seam (the C++ `MenuGame::setMouseCursorVisible` → `window.setMouseCursorVisible`, MenuGame.cpp:3641-3644); the `MenuWindowSource` backs it with a shared `Rc<Cell<bool>>` cursor flag (the C++ `MenuGame::mouseCursorVisible`) created `true` in `build` (the ctor's `setMouseCursorVisible(true)`, :245). `play_selected_level` hides (inside the `new_game` guard, the C++ `if(fnHGNewGame)`, :2373); `handle_mouse_wheel` re-shows (:304-306); the C++'s other two menu-state handlers (KeyPressed → hide :278, MouseMoved → show :295) are observed per-frame in `tick_fields` (menu-state only) with a new `Window::mouse_pixel()` accessor (the `actuallyMoved` delta test, :284-296).
- **Citation**: MenuGame.cpp:2373, :304-306, :278, :295, :284-296, :197, :245, :3641-3644; the cursor logic is entirely menu-state (the game state never re-shows, so a started level's hidden cursor persists).
- **Tests**: oh-menu `cursor.rs` (3 — hide over a recording `FakeWindow`; the unwired-`new_game` guard leaves it visible; wheel re-shows after a hide) + oh-app `windowed.rs` (1 — the real frame loop: a key press hides, a release does NOT re-show, a mouse move re-shows — the window's `mouse_cursor_visible` pinned).
- **Not established**: the actual OS cursor hide/show (the headless window records the flag only — the real `sf::RenderWindow::setMouseCursorVisible` GL/winit call is the live-window half, a manual check; the flag's toggling at the C++'s four call sites is pinned host-side).

**29o1** (internal, `52a470d8`) — the two non-windowed SIGINT handlers (headless exit-1 + the server print).
- **Fix**: the headless "without game window" handler (the C++ `main.cpp:627-634`: log `Caught signal '<s>' without game window open, exiting...` + `exit(1)`) is now installed for the headless path; the server's `Caught signal %d\n` print (HexagonServer.cpp:1389-1390) is added to the flag handler. The handler bodies are async-signal-safe (`write(2)` of a pre-built line) — the C++'s own `TODO (P2): UB` is noted.
- **Structural deviation (recorded in deviations.md)**: the C++ `std::signal` REPLACES the prior handler (one active at a time); `signal-hook` CHAINS (every registered action fires, in registration order), so a "default + replacement" would fire the headless `exit(1)` FIRST and never the windowed/server handler. The port therefore installs PER MODE (`sigint.rs` `install_headless` / `install_server`; `windowed.rs::install_sigint` for windowed) — the active handler per mode is identical to the C++. The headless handler ends in a raw `_exit(1)`; the server handler does a single atomic store (`globalRunning = false`; the C++'s `globalListener.close()` deferred to the run loop's exit).
- **Citation**: main.cpp:627-634 (headless), :340-350 (the windowed replacement); HexagonServer.cpp:1389-1390 (the server print).
- **Tests**: the handler BODY is pinned in-process (oh-app `sigint.rs` +2 — `headless_line_is_the_c_log_line` pins the byte-exact `Caught signal '2' without game window open, exiting...` line; `server_line_is_the_c_printf_line` pins `Caught signal 2\n`).
- **Not established (explicit, per this lane's hard rule)**: the END-TO-END signal delivery (a real SIGINT → handler → the printed line + exit-1 / stop-flag) is a manual check. The interrupted 029misc lane's attempt to hand-verify the exit code by launching the BLOCKING binary (`./target/debug/open-hexagon server`, which never returns) is exactly the racy, non-reproducible check this lane exists to avoid — a negative hand result ("exit=0" because the process finished first) is noise, not evidence. So the end-to-end delivery is recorded as NOT established; only the handler body is pinned.

**29h1** (internal, `f0c6f7cc`) — the replay [de]serialization diagnostics (the SSVOH_TRY/printTryFailure lines).
- **Fix**: the C++ `SSVOH_TRY`/`printTryFailure` (Replay.cpp:25-41) prints `Failed [de]serialization operation '<op>'` to stderr from 24 call sites; the port's `Writer`/`Reader` failed silently. Ported a private `print_try_failure` (the C++ `printTryFailure`, `eprint!` to stderr over the pure `try_failure_line` so the shape is testable) at the same 24 points with the C++'s stringified macro-argument as the `<op>` (`write(_version)`, `read_str(_player_name)`, the nested `write_str`/`read_str` lambda ops, etc.). A nested failure prints at both the inner and enclosing site, exactly as the C++ macro nests. The port-only hostile-input guards (the up-front bounds checks the C++'s `resize` would OOM on) stay silent — they have no `SSVOH_TRY` site.
- **Citation**: Replay.cpp:25-41 (the macro + `printTryFailure`).
- **Tests**: oh-replay +2 (`try_failure_line_matches_the_cpp_shape` pins the byte-exact C++ line across all 24 op names; `too_short_buffer_fails_at_the_first_read_site` pins the first read site).
- **Not established**: nothing — this closes the replay pair at 1:1.

**29a4** (internal, `cd9c5415`) — the death/replay log lines + the M8–M12 surface gaps.
- **Fix**: NO code change — the death/replay log lines were already landed by 029a1 (`14167fc6`). `Attempting to send and save replay...` is in BOTH `death` (HexagonGame.cpp:980) and `save_replay` (HexagonGame.cpp:555), and the `Successfully saved new compressed replay file '...'` success line (HexagonGame.cpp:1093-1094) + the failure line are in `RealReplayStore::save` (oh-app `replay_store.rs`).
- **M8–M12 verdicts (all NOT-NEEDED, recorded in deviations.md — no dead API added for symmetry)**: M8 `refreshTrigger` (HexagonGame.cpp:461-464) — the rebind is wired at the composition (`Manager::refresh_triggers`, input_manager.rs:399); M9 `getGame()` (HexagonGame.cpp:1527) — the composition's `GameEngine::set_game_state(GameSlot::Game)` (game_state.rs, main.cpp:448/502); M10 `getColor(int)` (HexagonGame.cpp:1537) — the only C++ caller is `drawText`'s `getColor(0)` (HGGraphics.cpp:732), inlined in `bridge.rs::draw_frame`; M11 pack accessors (HexagonGame.cpp:1359-1376) — `getPackId`/`getPackName` ported, `getPackData` = the `SharedState.pack_data` clone, `getPackDisambiguator`/`getPackAuthor`/`getPackVersion` have NO C++ call sites (not ported); M12 `getAssets()` (HexagonGame.cpp:1611) — NO C++ call sites (not ported).
- **Citation**: HexagonGame.cpp:980, :555, :1093-1094 (the log lines); HexagonGame.cpp:461-464, :1527, :1537, :1359-1376, :1611 (M8–M12) + their C++ call sites (HGGraphics.cpp:732, main.cpp:448/502).
- **Tests**: gate unchanged (docs-only commit; the log lines were already pinned by 029a1's tests).
- **Not established**: nothing.

**29m1** (data-format edge, `15fecda4`) — the replay string bytes + the bounded file read.
- **Fix (a) — non-UTF-8 strings**: the C++ `player_name`/`pack_id`/`level_id` are `std::string` (raw bytes, NO UTF-8 requirement); a legacy C++ `.ohr` with a non-UTF-8 name must round-trip losslessly. First checked what the C++ does on re-read/re-serialize: the `std::string` holds the raw bytes, `read_str` copies them verbatim (no UTF-8 check), `write_str` writes them back — so the C++ round-trips LOSSLESSLY, which is the requirement. Added `oh_replay::ByteString` (raw `Vec<u8>`: `From<&str/String/Vec<u8>/&[u8]>`, `Deref<[u8]>`, `as_bytes`/`to_str` lossless + validated, `to_string_lossy`/`Display` lossy U+FFFD) and held the three `ReplayFile` fields in it. `Writer::write_str` now takes `&ByteString`; `Reader::read_str` fills `&mut ByteString` with the raw bytes (verbatim, NO UTF-8 check) — so BOTH the file form and the packet form (`serialize_to_packet`/`deserialize_from_packet` route through the file-form `serialize`/`deserialize`) round-trip non-UTF-8 names losslessly. The `&str` callers (`get_level_validator`/`is_valid_pack_id`/`is_valid_level_id`/`get_level_data`/`new_game`) take `.to_string_lossy()`. The ONE recorded deviation: the C++ writes the raw bytes to the display and `create_filename`'s path; the port's `String`-based `Display`/`create_filename` is the lossy UTF-8 form (U+FFFD) for non-UTF-8 names (Rust `String` is UTF-8-only). The wire bytes round-trip losslessly.
- **Fix (b) — the unbounded read**: `ReplayFile::deserialize_from_file` did `std::fs::read` (the WHOLE file) before the 2 MiB cap check, so a hostile multi-GB file OOMed. The C++ reads into the 2 MiB static buffer (Replay.cpp:363-370): a larger file is a short read → failbit → false (fails, does not OOM). Now the port opens the file and reads at most `BUF_SIZE + 1` bytes (`File::take(BUF_SIZE + 1)` → `read_to_end`), failing past the cap WITHOUT reading the whole file. `CompressedReplayFile::deserialize_from_file` is unchanged (the C++ reads the compressed file unbounded; the 2 MiB cap is the DECOMPRESSION destination, not the compressed file).
- **Citation**: Replay.cpp:363-370 (the 2 MiB static-buffer read); Replay.cpp:19-23 (the `std::string` fields).
- **Tests**: oh-replay +5 (`bytestring_display_is_lossy_for_non_utf8`; `replay_file_non_utf8_name_round_trips_losslessly` pins a `[0xFF,'a']` name through the file form byte-for-byte; `replay_file_deserialize_from_file_fails_on_a_file_over_the_cap` + `_cap_boundary` pin the sparse 3 MiB / cap+1 reject and the exactly-cap accept — created in the test's temp dir via `set_len`, never a committed giant fixture; `_missing_path_fails`).
- **Not established**: nothing (the lossy display/filename for non-UTF-8 names is the recorded deviation, not a gap).

**Gate**: green after each item. Lane baseline 908 (the batch file's "900" is pre-harvest; the interrupted lane's 29b4 +2 / 29u1 +4 / 29o1 +2 moved it to 908). This lane: 29h1 +2 → 910, 29a4 +0 (docs) → 910, 29m1 +5 → **915**. `bash tools/gate.sh` 4/4 (fmt-check / build / clippy `-D warnings` / full test suite).
