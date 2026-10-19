# Task 059 — triage the tracker's queue, so "what's next?" can be answered from the tracker

Read `.agents/WORKER-HYGIENE.md` first. This is a DOCS-ONLY bookkeeping task, and its value is that the
project's own queue currently lies: I answered "what's next?" by scanning `.agents/plans/STATUS.md` and
found rows marked QUEUED or SPECD for work that has demonstrably landed, sitting next to rows that may be
genuinely open.

## The evidence that prompted it

A scan of the status column turns up, among others: **32** ("the wasm build has NO INPUT of any kind") and
**35** ("wire the engine log sink to ANDROID LOGCAT") marked QUEUED/SPECD, both of which landed long ago
(032 and 039's STEP 0); **030c** and **38** marked SPECD, both landed; **29v1** and **29w1** marked QUEUED,
both landed in the 029-tail batch. Next to them sit rows whose state I could NOT determine by inspection —
**29m** (the menu mouse hover, which I have dispatched separately because a live user report points at it),
**29f** (the favorites UI), **29j** (the gamepad poller and the eleven rising-edge handlers), **29t**
(`GameTimer::refresh`), **29q1** (a citation/comment drift sweep) — any of which may be done, partial, or
genuinely open.

## Mission

For EVERY row in the tracker whose status is QUEUED, SPECD, PENDING or otherwise not a terminal state:
determine the truth and record it, exactly as issue 042 did for the deviations register's dispositions —
the method is the same and it worked.

- Evidence, not impression: `git log` for the landing, the audit reports and task files, the register's
  own rows, and the code itself. A row is LANDED only if you can cite the commit.
- Mark each row LANDED (with the hash), OBSOLETE (with the reason — e.g. superseded by a later decision),
  or STILL OPEN (with what remains and a size). Do not delete rows; the record of what was once queued is
  worth keeping.
- Distinguish "the row is stale" from "the row is partly done": a row whose subject landed but whose
  audit-fix half did not is STILL OPEN and should say so precisely.

## Deliverable

A short section in the report with the counts (N rows examined, M landed, K obsolete, J still open) and,
**the part I actually want**: a clean, evidenced list of what is genuinely still open, so the next
"what's next?" question is answerable without archaeology. Update `.agents/plans/STATUS.md` in place.

BOUNDARIES: docs only — no code, no register-row rewrites (042's ground), no renumbering. Tests are not
required (nothing executes), but commit with explicit paths and bound every command with `timeout`. If a
row's truth cannot be established, say "cannot determine, and here is what I checked" rather than guessing.
