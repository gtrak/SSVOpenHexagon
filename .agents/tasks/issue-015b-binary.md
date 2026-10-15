# Task spec — issue 015 (step b): the `open-hexagon` binary (args, printLuaDocs, headless replay run)

You are a worker. Implement exactly this. Do not redesign or widen scope.
**No window/winit/GL/event-pump code** — the windowed `mainClient` path and
`mainServer` are steps (c) and issue 18.

## Why this task exists (read first)

Issue 15's contract says "`open-hexagon -headless -p base -l <level>` runs N
ticks and exits 0". **That is wrong** (D-012, verified against the source): the
C++ headless path (`main.cpp:537-581`) IGNORES `-p`/`-l` (those are consumed by
the windowed MenuGame path only, `mg->init(false, *cliLevelPack,
*cliLevelName)` at `:510`), and **requires a compressed
replay file** — with none present it logs
`"Running in headless mode without replay...?"` and `return 1`. With one, it
decompresses, replays to death via
`hg.runReplayUntilDeathAndGetScore(replayFile, 1 /* maxProcessingSeconds */,
1.f /* timescale */)` and prints `"Player died.\nFinal time: <playedTimeSeconds>"`.
Port THAT.

Also note `main.cpp:672-686`: `printLuaDocs` and `server` are checked BEFORE
`mainClient`, and `SSVOH_ASSERT(!printLuaDocs)` `:685` means the client path
can never see them.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`, branch `master`, HEAD `86a747a0`, tree
clean, gate 4/4 (369 tests default + feature-on), `oh-app` lib exists (15a).

Read first:
1. C++ oracle: `src/SSVOpenHexagon/Core/main.cpp` — `ParsedArgs`/`parseArgs`
   `:80-140`, `createFolderIfNonExistant` helper `:65-79` + calls `:275-276`,
   `Config::loadConfig(args)` + `reapplyResolution` `:282-283`, the
   config-save scope guard `:287-292`, the headless branch `:537-581`,
   `getFirstCompressedReplayFilenameFromArgs` `:146-154` — D-014 CORRECTION
   (landed 15b): the scan is a SUBSTRING test (`arg.find(".ohr.z") !=
   npos`), NOT "args not starting with `-`"; `mainPrintLuaDocs` `:167-189`,
   the dispatch `:666-686`.
   D-014 CORRECTION (landed 15b): the spec's "C++ saves config … BEFORE
   ImGui shutdown" (in §3) is reversed — the three `mainClient` scope
   guards fire LIFO at scope exit, so the config-save guard (registered
   first, `:287`) fires LAST (after the `pSaveAll` and ImGui guards).
   For the pieces that exist in the 15b tree (config save only) the
   relative position is the same either way: after the game ran, after
   the `Finished` line.
2. `crates/oh-app/src/` (15a) and `crates/oh-game/tests/common/mod.rs` (the
   two-phase composition — 15b replicates it inside the binary).
3. `crates/oh-game/src/seams.rs` (`ScriptHost`, `NoScriptHost`),
   `crates/oh-lua/src/docs.rs` (`print_docs_to_stdout`),
   `crates/oh-replay` (the compressed-replay reader — find the
   `deserialize_from_file` analogue; the C++ type is
   `compressed_replay_file::deserialize_from_file`).
4. `mapping/oracle-vectors.md` — the oracle compressed-replay fixtures (CZHEX
   bytes) you can drop into a test.

## Scope

### 1. `parse_args` (verbatim port, `main.cpp:80-160`)

`-p <pack>` / `-l <level>` (pairs, later wins per the C++ overwrite
semantics), `-printLuaDocs`, `-headless`, `-server`; everything else goes to
the `args` vector (config overrides + the replay-filename scan). Port the
exact flag strings and the "i+1 skip" behaviour. Unit-test with argv slices.

### 2. `main_print_lua_docs` (port `main.cpp:167-189`)

Headless assets (`oh_assets::Assets::new(root, steam=None, headless=true)`), a
null-window engine (the composition from `tests/common`, moved into the lib or
a `compose` module — 15b may promote the test compose helper into
`oh-app` as `pub`, deduplicating 15a's test copy), `init_lua_and_print_docs()`
(oh-lua `print_docs_to_stdout`), the `\n\n\n\n\n` fences, and the trailing
`"::mainPrintLuaDocs" << "Finished\n"` log line. Exit 0.
**Default-mode behaviour (D-023): 11 headers, zero entries** — do NOT expect
function docs.

### 3. Folders + config lifecycle

`createFolderIfNonExistant("Profiles/")` + `("Replays/")` (find the C++
helper — `std::filesystem::create_directories` + log; port it with the same
log output on creation). `Config::load_config(args)` +
`reapply_resolution()`; the SCOPE-GUARD order matters: the C++ scope
guards fire LIFO at scope exit, so the config-save guard (registered
FIRST, `main.cpp:287`) fires LAST — after the `pSaveAll` guard, after the
ImGui shutdown, after the game ran, and after the `Finished` line (D-014
CORRECTION: this spec's first draft said "BEFORE ImGui shutdown" — the
LIFO order is the reverse; see the note in the Context section). Mirror
the order for the pieces that exist (config save + its two log lines, at
scope end after the game ran).

### 4. Headless client run (port `main.cpp:537-581` exactly)

- No replay file in args → the exact log line + `return 1` (`:542-545`).
- Unreadable/corrupt file → the decompress failure (`:554-558`) prints
  `Could not decompress replay file` (stderr) + `return 1`; the
  `deserialize_from_file` failure path (`:577-581`, a file that name-scans
  but cannot be read) logs `Failed to read compressed replay file in
  headless mode '<name>'` and falls THROUGH to `return 0` (verified against
  the source — the C++ does not return 1 there; D-014 double-check done in
  15b, pinned by the `headless_missing_replay_file…` test).
- Success → `"Player died.\nFinal time: <score>\n"` where the score is
  `runReplayUntilDeathAndGetScore(replay, 1, 1.f).playedTimeSeconds` — check
  whether that engine method exists from the determinism harness (issue 09
  ported `executeGameUntilDeath`-equivalents; expose/reuse — do not duplicate
  simulation logic).
- Steam/Discord/audio/HexagonClient do not exist yet (issues 17/20/21): pass
  the None/no-op equivalents the engine seams already accept; document that
  15c/17/20/21 fill them.
- `mainServer` (issue 18): print `"Server mode is not yet implemented"` +
  `return 1`? NO — check the C++ first (`mainServer`, `main.cpp:197+`): the
  port MUST NOT silently succeed where C++ runs a server. Return a
  non-zero exit with an explicit "not ported yet (issue 18)" message, and
  record that as a documented deviation until issue 18 lands.

### 5. The binary + windowed stub

`[[bin]] name = "open-hexagon"`. Dispatch per `main.cpp:672-686` order
(printLuaDocs → server → client). The windowed client path prints
`"Windowed mode is not yet ported (issue 15 step c)"` to stderr and returns
non-zero — a documented stub replaced in 15c. (C++ would run the game; the
stub must be unmistakable.)

### 6. Tests (cargo-level, headless — no display needed)

- `-printLuaDocs` over a real `_RELEASE`: exit 0, output = the 11 headers
  byte-exact (default mode) + the Finished line; feature-on build: entry
  lines present.
- Headless + a REAL compressed replay fixture (build one in-test with
  `oh-replay`'s writer from a live run — the determinism harness already
  produces replay files; compress via the oh-replay compressor): exit 0 and
  the printed final time equals the harness's own score for that replay.
  D-014 CORRECTION (landed 15b): `tests/determinism.rs` pins NO absolute
  numbers (its module docs: "Self-consistent, NOT C++-verified … NO
  expected numbers" — D-018), and the live run's score is not absolutely
  pinnable (the game RNG is entropy-seeded per `newGame` — C++-faithful,
  `HexagonGame.cpp:306` + `initializeRng()` `:57-65`; observed 1.75417,
  1.725, …). The `1.3875` figure appears only in DECISIONS.md D-019 as a
  scratch discrimination value, not as a fixture. So the established
  cross-check IS the D-018 property `determinism.rs` establishes: the
  binary's printed `Final time:` (the replay run's `playedTimeSeconds`,
  C++ `std::ostream` float format via `format_ostream_f64`) must equal the
  harness's own live-run `played_time_seconds` — pinned by
  `headless_real_compressed_replay_runs_to_death_and_prints_the_harness_score`.
- Headless without replay → exit 1 + exact line; corrupt replay → the right
  exit code per your verified C++ reading.
- Unknown flag handling: `-p` without a following value etc. — port the C++
  loop's behaviour exactly and pin it.

## Process requirements (mandatory — prior lanes timed out)

1. Commit after every green milestone: (a) parse_args + tests; (b) folders +
   config lifecycle; (c) printLuaDocs path + test; (d) headless replay run +
   tests; (e) dispatch stubs + full workspace gate.
2. `cargo test -p oh-app` after each file; fix clippy `-D warnings`
   immediately; never hold a lock guard across a re-acquiring call.
3. Verify every C++ citation against the real source (D-014) — including the
   two the spec asks you to double-check (fall-through, mainServer). Fix the
   spec in your commit if anything is off and say so.
4. Update `.agents/plans/STATUS.md` row 15 in your final commit.

## Out of scope

- Windowing, event pump, MenuGame↔HexagonGame runtime switching, screenshot
  key, signal handler (`std::signal`), vsync/timers — step (c).
- The egui console overlay (14b — step c).
- Server functionality (issue 18), Steam (20), Discord (21), the real
  HexagonClient (17).
- Any change to simulation order, the D-019 fix, or the determinism harness.
