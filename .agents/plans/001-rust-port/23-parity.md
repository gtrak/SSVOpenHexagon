# Issue 23 — Parity verification harness + corpus

## Objective

Build the end-to-end evidence that the port is 1:1: a harness that runs the same
level/seed/input corpus through the Rust build (and the C++ build where available)
and compares outcomes, plus a growing corpus of pinned expectations.

## Key decisions

- The strongest evidence is **C++-derived expectations**: replay files, scores,
  RNG streams, and serialized bytes produced by the real C++ code.
- Where the C++ build is available (it is on this Linux box for the pure-logic
  parts), generate expectations by compiling the specific `.cpp` with shims (as
  done for `oracle-vectors.md`). For full-game runs, the C++ needs SFML/LuaJIT/ImGui
  — effort is justified only if the full C++ build succeeds; otherwise pin the
  pure-logic layer and document the gap.
- The corpus lives in-repo under `.agents/plans/001-rust-port/parity/` (vectors) and
  is consumed by a `cargo test -p oh-parity` crate or a `tools/parity.sh` script.

## Files

| File | Change |
|---|---|
| `crates/oh-parity/` or `tools/parity.sh` | new — harness |
| `.agents/plans/001-rust-port/parity/*.json`/`*.bin` | new — corpus |
| `.agents/plans/001-rust-port/PARITY.md` | new — results table |

## Steps

1. Define corpus entries: `{level, pack, seed, difficulty_mult, input_vector,
   expected_score, expected_frames, expected_rng_draws}`.
2. Generate expectations from the C++ build (pure-logic probes for RNG/replay; full
   runs if the C++ build succeeds).
3. Run the Rust build against each entry; record pass/fail in `PARITY.md`.
4. Extend the corpus to cover all base/cube levels and edge seeds.
5. Make the harness a gate: a regression in parity fails.

## Verification

- `PARITY.md` records, per corpus entry, the C++ value and the Rust value and the
  verdict. Every entry is reproducible from a committed script + committed corpus.
- The harness itself is tested (a deliberately wrong entry fails).

## Notes / risks

- Do not commit expectations that cannot be regenerated from a committed script —
  an unreproducible number is a claim, not evidence (plan-process rule).
