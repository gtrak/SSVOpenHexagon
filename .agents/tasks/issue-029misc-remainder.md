# Task 029misc-remainder — items 4–6 of the 029misc batch + the batch report

The 029misc lane was **interrupted** (44M tokens, 327 turns) while trying to
hand-verify SIGINT exit codes by launching blocking processes — it ran
`./target/debug/open-hexagon server`, which never returns, and wedged for 4+
minutes in that call. Its first three items were harvested and are committed:

- `8fbe6bf1` **29b4** — `-p/-l` boot now starts the level
- `2dbcdd9c` **29u1** — `setMouseCursorVisible` through the play path
- `52a470d8` **29o1** — the two non-windowed SIGINT handlers (harvested +
  formatted by the orchestrator; the gate was 3/4 because of an unformatted
  `format!`)

Master is green: gate 4/4, **908 tests**.

## Hard rule for this lane (it is why the lane exists)

**Never hand-verify a signal/exit-code behaviour by launching a process that
blocks.** If you must run the binary, wrap it in an explicit bound
(`timeout 10 ./target/debug/open-hexagon …`) *and* put the whole thing in one
bounded bash call. The design that makes this unnecessary is already in
`sigint.rs`: `headless_line(sig)` is a pure function of the signal number, so
test the *body* in-process and say plainly that the end-to-end signal delivery is
a hand-check. A negative result from a racing hand-check ("exit=0" three times,
because the process finished first) is not evidence — it is noise. If a manual
check cannot be made reliable in one bounded attempt, stop and report it as
not-established.

## The three remaining items

These are items 4–6 of `.agents/tasks/issue-029misc-batch.md`. **Read that file**
— it has the citations. Summary:

1. **29h1** [internal] — the replay [de]serialization diagnostics are silent. C++
   `SSVOH_TRY`/`printTryFailure` prints `Failed [de]serialization operation '…'`
   from 15 call sites (`Replay.cpp:25-41`); the port's `Writer`/`Reader` fail
   silently. Port it at the same points with the same message shape. This is the
   replay pair's **only** missing behaviour — it closes that pair at 1:1.

2. **29a4** [internal] — the death/replay log lines + surface gaps. The
   `"Attempting to send and save replay..."` line belongs in `death`
   (`HexagonGame.cpp:980`), not only inside `save_replay`
   (`hexagon_game.rs:1654`); the success line
   `"Successfully saved new compressed replay file …"` (`:1093-1094`) is absent.
   Then the M8–M12 surface gaps (`refreshTrigger` `:461`, `getGame()` `:1527`,
   `getColor(int)` `:1537`, the pack accessors `:1359-1376`, `getAssets()`
   `:1611`) — **check each against its C++ call sites** and either add it or
   record precisely why the port's structure makes it unnecessary. Do **not** add
   dead API for symmetry's sake.

3. **29m1** [data-format edge] — two things, one commit:
   (a) **non-UTF-8 replay strings**: the C++ stores
   `player_name`/`pack_id`/`level_id` as `std::string` bytes; the port requires
   UTF-8 and fails deserialization (`crates/oh-replay/src/lib.rs:19-23` documents
   it as accepted), so a legacy C++ `.ohr` with such a string is unreadable — a
   real wire-compat break. **First check what the C++ does when it re-reads and
   re-serializes such a name** — that determines the requirement — then implement
   the faithful fix, or the minimal correct thing, and record precisely what
   remains.
   (b) **the unbounded read**: `ReplayFile::deserialize_from_file` does
   `std::fs::read` before the 2 MiB cap check (`lib.rs:740-751`) where the C++
   reads into a bounded 2 MiB static buffer (`Replay.cpp:363-370`), so a
   hostile/corrupt multi-GB file OOMs the port. Bound the read (read cap+1 and
   fail past it) so it matches the C++'s "fails, does not OOM". Test with a
   sparse/oversized file created in the test's temp dir — **do not commit a giant
   fixture**.

## Also required

- **Append the `## Report` to `.agents/tasks/issue-029misc-batch.md`** covering
  **all six items** — for 29b4/29u1/29o1 read the three commits above
  (`8fbe6bf1`, `2dbcdd9c`, `52a470d8`) and summarise what they did, so the batch
  has one complete report. Be explicit about what the 29o1 hand-check could not
  establish.
- **The deviations rows** for anything that is a deviation (issue 28's rule) —
  including 29o1's signal-hook-vs-`std::signal` structural deviation, if the
  harvested commit did not already add it.

## Hygiene

- **The phone's ADB is DOWN** (`adb connect 192.168.10.138:38721` → refused; the
  host still pings it). No device work; no temporary `debuggable` flag.
- One commit per item, naming the row id. **Stage explicit paths — never
  `git add -A`** (shared checkout + index).
- Keep the gate green after each item (`bash tools/gate.sh`, expect 908 + yours).
  Run `cargo fmt --all` before you commit — the interrupted lane's only gate
  failure was an unformatted `format!`.
- `.agents/audits/*.md` are committed reports — do not rewrite them.
- If the work overruns your context, STOP after a coherent item, commit what is
  green, and report what remains.
