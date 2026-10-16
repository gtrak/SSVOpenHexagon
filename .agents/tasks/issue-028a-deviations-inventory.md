# Task 028a — the deviations inventory (mechanical)

Lane 28a of issue 28 (`.agents/plans/001-rust-port/28-deviations-audit.md` —
read it first; it has the rationale, the categories, and the standing rules).
Master green (gate 4/4, 729 tests) at `3a9da524`.

## Why

The operator's goal is a **1:1 Rust port as the starting base**. A
"documented, assumed harmless" deviation (the menu 9-zone touch grid,
dropped by the 25b lane as "out of scope") turned out to be
**load-bearing** — without it a phone cannot submit a name. The fix is a
deviations REGISTER so every divergence is auditable, plus a machine check
so none can live only in a code comment.

## Your job: produce the raw inventory, no judgement

1. **Sweep** `crates/` (exclude `tests/` and `target/`) for these markers
   (case-insensitive): `not ported`, `unported`, `not implemented`,
   `documented no-op`, `no-op`, `placeholder`, `TODO(`, `FIXME`,
   `divergenc`, `approximat`, `simplif`, `out of scope`, `stub`,
   `the port's analogue`, `no-op sink`, `Noop`, `NoAudio`/`NoSteam`/
   `NoDiscord`/`NoJoystick` (the deliberate seams).
2. **For each hit**, emit a register row with:
   - `path:line` (exact, as of your HEAD),
   - the verbatim comment text (trim to the essential clause if it is a
     multi-line block, but keep it recognisable),
   - the enclosing item (function/struct/module) — so a reader can find it,
   - the C++ counterpart the comment names, IF it names one (write
     `C++: <file:line or symbol>` — and **verify the citation exists** with
     a grep before you write it; if it does not exist, write
     `C++: UNCITED` — that is itself a finding),
   - one of the categories from `28-deviations-audit.md`'s table
     (platform seam / dead C++ / not-yet-ported / draw-path remainder /
     load-bearing / determinism-risk / test-double-or-legit-no-op / unknown),
     with `unknown` as the honest default when unsure.
3. **Write it to `.agents/plans/001-rust-port/deviations.md`** as a table,
   grouped by crate, rows in the sweep's reproducible order. Header must
   state: the sweep command, the marker list, the HEAD commit, and that
   VERDICTS COME LATER (28b).
4. **Do NOT fix anything. Do NOT triage.** Judgement is 28b's job, and a
   half-triaged table is worse than a clean inventory.

## Constraints

- Exclude `tests/` (test doubles and `no-op` test seams are not deviations)
  — but DO include doc-comment module headers, which is where the
  important deviations are stated.
- Do not invent rows: every row must come from a real marker hit. If a
  marker produces >150 hits in one crate (likely for `no-op`), group them
  as one row with the count + a representative sample + the exact list in
  an appendix section, rather than 150 near-identical rows.
- Commit every ~10-15 tool calls (hygiene — a previous lane died at 41.7M
  tokens of context). This lane is grep-heavy: prefer `grep -rn | head` and
  `python3` aggregation over reading whole files, and do NOT read images.
- Host gate must stay green (you are not changing code, so it should be
  untouched — run `tools/gate.sh` once at the end to prove it).
- Files to create/modify: `deviations.md` only (plus your report). Do not
  touch other lanes' files (025f is editing `oh-menu`/`oh-app`).

## Report

Row counts per category, the UNCITED list (comments that name a C++ site
that does not exist — these are the highest-value findings), the grouped
crate summary, and anything that looks load-bearing (a dropped behavior
that a user would notice) flagged explicitly for 28b's triage. Keep the
report short; the table is the deliverable.
