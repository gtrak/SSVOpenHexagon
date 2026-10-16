# Task 028b — deviations triage + the gate-wired checker

Lane 28b of issue 28 (`.agents/plans/001-rust-port/28-deviations-audit.md`).
Depends on 28a's `deviations.md` (the raw inventory). Read both first, plus
`DECISIONS.md` D-033 (the operator's deferred queue).

## Part 1 — triage every row (judgement, verified)

For each row in `deviations.md`, assign:

- **category** (the spec's table; fix any 28a `unknown`s you can resolve),
- **verdict**:
  - `ACCEPTED` — the port is legitimately different. REQUIRES a reason and,
    where the comment asserts something about the C++ (e.g. "dead in C++",
    "unused"), a **re-verified citation** (open the C++ and show it). An
    accepted verdict with an unverified claim is how the 9-zone grid slipped
    through — do not repeat it.
  - `FIX` — the port must be completed; add a one-line scope note and a risk
    rank (user-visible gameplay > user-visible cosmetic > internal >
    determinism/data — determinism/data is ALWAYS escalated to the
    orchestrator, never just fixed).
  - `DECISION` — needs the operator (e.g. a C++ behavior we intentionally
    will not reproduce, or a substitution with compatibility consequences).
    These go in the report as a numbered list, not in the register alone.
- **risk**: one of `uservisible-gameplay`, `uservisible-cosmetic`,
  `determinism`, `data-format`, `internal`, `none`.
- **tracking**: an issue id, a D-number, or `28c-<n>` (a new FIX lane you
  are proposing).

Sort the register by risk (highest first) within each crate, so the
dangerous ones are unmissable, and update the header to say it is triaged
(with the triaging HEAD commit).

## Part 2 — `tools/check-deviations.sh` (the machine guard)

A script that fails when a deviation marker in `crates/` (excluding
`tests/`) has no register row:

- **Sweep**: the same markers as 28a (keep the lists in sync — put the
  marker list in ONE place, e.g. `tools/deviations-markers.txt`, read by
  both the script and documented in the register header).
- **Match**: by `path:line` is too brittle (line numbers drift every
  commit). Match by **(file, a normalized comment snippet)** — e.g. the
  first 40 alphanumeric chars of the marker line, lowercased. Document the
  matching rule in the script header AND in the register's header, and make
  the failure message print the exact snippet + file + line + the suggested
  register line, so fixing it is mechanical.
- **Allowlist**: `tools/deviations-allowlist.txt` for hits that are genuine
  non-deviations (test doubles are already excluded; a C++-faithful no-op
  still needs a register row, so the allowlist should stay tiny — every
  entry needs a one-line reason).
- **Exit codes**: 0 = every hit has a register row or an allowlist entry;
  1 = unregistered markers found (print them); 2 = usage/infra error.
- **Run it**: it must pass on the triaged tree. Prove it fails correctly by
  temporarily adding a fake marker to a scratch file and showing the
  failure (do not commit the fake).

## Part 3 — wire it into the gate

`tools/gate.sh` currently runs 4 stages (fmt, build, clippy, test) and
prints `GATE_EXIT=<rc>` per stage. Add the checker as a **5th stage** in the
same `run_stage` style, e.g.:

```sh
run_stage "tools/check-deviations.sh" ./tools/check-deviations.sh
```

Keep the "never pipe to tail" rule and the honest exit codes. Note in the
script's header that this is issue 28's standing guard: **a lane that drops
a C++ behavior must add its register row in the same commit** (the check
makes that enforceable).

## Constraints

- Do NOT fix deviations in this lane (that is 28c…). You are producing the
  triaged register + the guard + the gate wiring.
- The register is documentation: keep it readable (one row per deviation,
  no duplicated prose; the appendix grouping from 28a stays).
- Hygiene: commit every ~10-15 tool calls; grep/python3 over file reads; no
  image reads. Gate must be green (now 5/5 with your stage).
- Do not touch `crates/oh-menu`/`crates/oh-app` sources (lane 025f owns
  them while it works).

## Report

Counts by category and verdict; the **DECISION list** (numbered, for the
operator); the **FIX list ranked by risk** (these become 28c lanes); the
UNCITED findings from 28a's report with your verification result for each;
the checker's failure proof; and the gate output showing 5/5.
