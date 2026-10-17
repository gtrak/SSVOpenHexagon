# Worker hygiene — the rules learned from lanes that burned out

Referenced from every lane spec. These come from real failures, not caution:
two lanes have died at 40M+ tokens, and several gate reds and one lost C++
reference file came from process mistakes rather than bad code.

## 1. Never handle a blocking process carelessly

A lane was interrupted at **44M tokens / 327 turns** for this. It tried to verify
SIGINT exit codes by launching the binary:

```
attempt 1: exit=0   (expected 1 = our handler, 130 = default)
attempt 2: exit=0
attempt 3: exit=0
```

Those results are **noise**: the process finished before the signal landed. It
then escalated to `./target/open-hexagon server` — which never returns — and
wedged in one `bash` call for 4+ minutes until the orchestrator killed it.

- If you run a binary, **bound it**: `timeout 10 ./target/debug/open-hexagon …`,
  in **one** bounded bash call.
- Prefer testing the **body** in-process. `oh-app/src/sigint.rs` exposes
  `headless_line(sig)` as a pure function *precisely* so the handler's effect is
  testable without delivering a signal. Design for this when you write
  signal/exit/timeout/event-loop code.
- A racing negative is not evidence. If a manual check cannot be made reliable in
  **one** bounded attempt, **stop and report it as not-established**.

## 2. At most ONE image read per lane

The earlier 41.7M-token death was a screenshot spiral. Prefer, in order:
`logcat -d | grep`, `dumpsys`, `pidof`, `find`, `ls -lR`, `cat`, and PIL
*analysis* (dimensions, non-black pixel counts, colour histograms) over reading
the image itself. If you notice you are reading images in a row, **stop and
report**.

## 3. Commit cadence and staging

- Commit every 3-5 tool calls, and after each completed item.
- **Stage explicit paths. Never `git add -A`.** A lane and the orchestrator share
  one checkout *and one index*. An orchestrator `git add -A` once swept a lane's
  in-progress edits and committed a deletion of `build/copyArtifacts.bash` (an
  upstream C++ reference file) — the C++ tree is this repo's citation reference
  and must stay pristine.
- Run **`cargo fmt --all`** before committing. An unformatted `format!` left a
  lane's work at gate 3/4.

## 4. Gate before every commit

`bash tools/gate.sh` — four stages (fmt / build / clippy `-D warnings` / test).
Each spec states the expected baseline test count; your work should raise it, not
lower it. If the gate cannot be green, **stop and report** rather than leaving a
red tree for the next lane.

## 5. Stop-and-report beats grinding

Long sessions degrade and spiral. If you are deep in a task and not converging,
commit what is green, say precisely what is done, what is left, and what you could
not establish — a clean handoff is a good outcome. **Never leave the tree
mid-change.**

## 6. Device work

- When the phone is available: `adb -s 192.168.10.138:38721` (wake with
  `KEYCODE_WAKEUP`), and evidence is **text** — `run-as … find/ls/cat`, `logcat`
  greps, `/proc/<pid>/stat` sampling.
- If ADB is unreachable (it goes down: the host still pings the device but the
  listener refuses), **do not** work around it and do not spend time retrying.
  Record the on-device confirmation as **PENDING** and move on.
- `debuggable = true` may be added temporarily for `run-as`, but it **must** be
  reverted in its own commit before you finish, and you must say so.
- Never touch non-phone processes. No emulator.

## 7. Reporting

- Report the fix, the citation you actually verified, the tests, and — explicitly
  — **what you could not establish**. A precise negative is worth more than a
  vague positive.
- Do not rewrite `.agents/audits/*.md`: those are committed audit reports, and
  the audit is a hypothesis to be checked against code, not a source of truth.
  When code and audit disagree, **code wins** — say so, and correct the record.
