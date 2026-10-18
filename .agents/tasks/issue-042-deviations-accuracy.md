# Task 042 — is the deviations register still TRUE? (the semantic sweep path-existence cannot do)

Read `.agents/WORKER-HYGIENE.md` first. Context: the operator asked *"is the deviations-register
accurate?"*. I checked it mechanically and found this split:

- **Paths are accurate**: of the **260 sweep rows** in `.agents/plans/001-rust-port/deviations.md`
  that carry a path reference, exactly **one** points at a file that no longer exists
  (`crates/oh-android/build.rs`, deleted by D-037 — now annotated). The F-section is maintained
  (29 rows; tonight's F28/F30 present; F29 correctly removed with its subject), and K6's stale
  "the pack shaders never execute" claim plus the misleading "not yet fixed" section header were
  fixed in `82d0d312` with an explicit status block.
- **The DISPOSITIONS are unverified, and that is this task.** The sweep rows record the 28b
  triage's verdict on each site — `no-op` (86), `placeholder` (26), `Noop` (25), `TODO(` (23),
  `NoSteam` (19), `unported` (18), and more — and **many audit-fix landings have happened since
  that triage**, so rows can be marked "placeholder"/"unported"/"TODO(" for code that now works,
  or describe a deviation that no longer exists. Path-existence cannot prove any of that; only
  reading each claim against the code (and against the C++ it cites) can.

## What to produce

For **each row**, a verdict in place, appended to the row (the register is a LIVING document —
unlike `.agents/audits/*.md`, which are immutable by hygiene §7, so do not touch those):

- **STILL TRUE** — the claim holds against the current code (no annotation needed if you are
  confident; a short `(re-verified <date>)` is welcome);
- **NOW FIXED** — the code moved on: name the landing commit that fixed it, so the row reads
  correctly to a future reader (this is the majority case to hunt for);
- **WRONG** — the row was mistaken when written (rare; say so plainly and cite what contradicts
  it, because "code wins when code and audit disagree" is the project's rule);
- **UNVERIFIABLE** — say exactly why (e.g. the C++ citation is dead, or the behaviour needs a
  device).

Deliver a **summary table** in your report: how many rows in each verdict class, and the list of
rows that moved to NOW FIXED / WRONG with their commits.

## How to make 260 rows tractable (do this — do not read them all blind)

**Prioritise by evidence of change, not by order.** For each row, its path(s) are mechanical: ask
whether that path has commits AFTER the triage's date (`git log --oneline <since> -- <path>`), and
work the rows whose files have changed first, because those are where a stale disposition is
possible. Rows whose files have not changed since the triage are strong candidates for
STILL TRUE, and can be spot-checked rather than re-derived — but SAY that is what you did, and
give the counts, so the reader knows the difference between "verified" and "unchanged since".

Also re-verify the F-row *claims* you touch: an F-row describes a deviation the port *intends* to
keep, so the question there is different — does the deviation still exist as described, and is its
justification still the right one? Flag any that a later landing silently made moot (that is
exactly what F29 was before D-037 deleted its subject).

And verify the three closures I just wrote are correct, since they are now load-bearing claims:
**K3/K4 closed by D-037**, **K6 closed by 029n1 (`4c86855d`)**. If any closure overstates what
landed, that is the most important finding in this task.

## Rules

- **The C++ is the reference and now lives in `cpp-original/`**, with root `src`/`include`
  symlinks so a citation of the form `src/SSVOpenHexagon/…` still resolves. Cite the C++ for any
  claim about parity, and note that a citation which does not resolve is not evidence.
- **Do not rewrite the audits** (hygiene §7), do not invent verdicts you did not check, and
  when the code and a row disagree, the code wins and the row is corrected with the citation.
- Bound every process (`timeout`); commit with explicit paths as you go (one commit per batch of
  rows, not one giant commit at the end); at most ONE image read (you need none); `cargo fmt` is
  irrelevant unless you touch Rust.
- If the sweep turns out to be larger than you can honestly finish, **stop and report** with the
  counts you did verify, the method, and the remainder — a precise partial sweep with the
  prioritisation stated beats a claimed complete one.
