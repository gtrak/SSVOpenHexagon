# Task spec — issue 009 (step d): port the benchmark replay test (close the coverage gap that hid D-019)

You are a worker. Implement exactly this. Do not redesign or widen scope.

## Why this task exists (read this first)

Issue 009 step (c) landed the determinism harness against **only**
`test/ReplayExecution.t.cpp`'s level set (2 pairs: `cube`/`apeirogon`,
`experimental`/`autotest0`). `09-game-loop.md` requires porting **both**
`test/ReplayExecution.t.cpp` **and** `test/ReplayExecutionBenchmark.t.cpp`. The
benchmark variant uses **4** pairs (adding `orthoplex`/`arcadia` and
`orthoplex`/`bipolarity`) and 100 iterations — and it is precisely that extra
coverage which exposed a **real replay-determinism bug** in the port (already
diagnosed and fixed on `master`, see `DECISIONS.md` **D-019**; the fix is
`hexagon_game.rs` resetting `main_timeline_runner` / `event_timeline_runner` in
`set_level_data_inner`).

So this task is (1) port the benchmark test properly, and (2) add the regression
guard so the bug cannot come back silently.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`, branch `master`, HEAD `40680846`, tree
clean, `bash tools/gate.sh` 4/4 green (304 tests).

Read first:
1. `.agents/tasks/issue-009c-determinism.md` — the harness this extends.
2. `.agents/plans/001-rust-port/DECISIONS.md` — **D-019** (the bug + the level
   that exposed it), **D-018** (the C++ tests assert self-consistency, not
   scores — do NOT invent numeric pins), D-014 (verify citations), D-012 (source
   beats contract).
3. `crates/oh-game/tests/determinism.rs` + `crates/oh-game/tests/common/mod.rs`
   — the existing harness and composition (reuse them; do not fork them).
4. `test/ReplayExecutionBenchmark.t.cpp` — the file you are porting.
5. `.agents/plans/001-rust-port/09-game-loop.md`.

## What the benchmark test actually is (verified — do not re-derive wrongly)

It is `test/ReplayExecution.t.cpp` with exactly three differences:
1. **4 pairs, not 2.** Its `packs`/`levels` arrays are (in order):
   `cube_1` / `..._apeirogon`; `experimental_1` / `..._autotest0`;
   `orthoplex_1` / `..._arcadia`; `orthoplex_1` / `..._bipolarity`.
   (`i % 4` indexing, so all four are used.)
2. **100 iterations, not 25.**
3. The `std::cerr << score ...` line is commented out, and the windowed
   `#ifndef SSVOH_HEADLESS_TESTS` block is absent.

The assertions are the same three (replay created, playback terminated,
`TEST_ASSERT_EQ(score, replayPlayedTimeSeconds)`) — i.e. the **self-consistency**
property from D-018, with no expected numbers. Label it that way.

## What to build

1. **Port the benchmark test** as its own `#[test]` in
   `crates/oh-game/tests/determinism.rs`, covering the 4-pair set at 100
   iterations × both variants (same engine + fresh engine), asserting the exact
   `==` self-consistency property. Reuse `common::{compose, release_dir,
   run_live_to_death}` and keep the existing `alwaysSpinRight` fixed-policy
   deviation (and its documented justification).
   - **Runtime budget is the risk to solve, not to hide.** The oracle probe run
     of a 200-run sweep took ~6.4 s on this machine, so 100 iterations × 2
     variants × 4 pairs is likely acceptable; MEASURE it and report the number.
     If it is too slow for the gate (a gate that takes minutes is a problem),
     reduce the iteration count **and say so explicitly in your report** with the
     measured time and why. Do not silently cut it.
2. **A regression test pinning D-019 specifically**, so the bug cannot return
   unnoticed and its cause is documented in code: a test that performs a
   *second* `new_game` on the same engine and asserts self-consistency (that is
   the exact shape D-019 describes). Use a level that waits early — D-019 found
   `orthoplex/bipolarity` exposes it; `apeirogon`/`autotest0` do NOT. Name it for
   what it guards (e.g. `engine_reuse_after_second_new_game_is_deterministic`),
   and in its doc comment cite `HexagonGame.cpp:668-669`, `:686-687`, `:672-673`
   and D-019.
3. **Verify the regression test discriminates:** revert the D-019 fix in a
   scratch copy and show the new test reddens. Report the exact mutation and the
   observed failure (this is required evidence — a regression guard that does not
   redden is worse than none).

## Working agreement

- **Write first**, compile early, commit at the first green build. Commit as you
  go — you may be interrupted and uncommitted work is lost.
- Touch **only** `crates/oh-game/tests/**` (and `mapping/oracle-vectors.md` if you
  add a probe, which you should not need). Do not modify `crates/*/src/**`,
  `tools/gate.sh`, or any other crate. If you believe such a change is needed,
  STOP and report instead.
- Do **not** modify the D-019 fix to make a test pass. If the benchmark test
  fails, that is a finding — report it with the exact numbers.
- Never present a self-consistent value as C++-verified (D-018).

## Verification (report ALL of these)

1. `bash tools/gate.sh` — paste every `GATE_EXIT=` line (must be 4/4).
2. The measured wall-clock time of the new benchmark test, and the final
   iteration count with its justification if you reduced it.
3. The discrimination check for the D-019 regression test: the exact mutation
   reverted, and the observed failure output.
4. A clean-checkout run:
   `rm -rf /tmp/v9d && mkdir -p /tmp/v9d && git archive HEAD | tar -x -C /tmp/v9d && cd /tmp/v9d && CARGO_TARGET_DIR=/tmp/v9d/target bash tools/gate.sh`.
5. Any gap, stated plainly.

## Report format

Files + test names · gate outputs (all `GATE_EXIT=` lines) · the benchmark test's
runtime and iteration count · the D-019 regression test and its discrimination
proof · the clean-checkout result · deviations · known gaps.
