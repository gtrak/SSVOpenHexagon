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

- **The host gate is not a web/Android gate.** A change to target-gated code
  (`#[cfg(target_os = "emscripten")]`, android, gles paths) is INVISIBLE to
  `cargo build`/`cargo test` on the host: those branches are never compiled, and a
  textual merge can happily produce a tree that does not compile for that target while
  every host stage is green. This bit the 026 landing - git merged 026's cfg'd
  emscripten paths against a master that had moved on (K6's `gl.rs` rework, 031's
  overlay fields), so the web build did not compile AT ALL and the host gate did not
  notice. If your change touches a target-gated path, BUILD THAT TARGET before calling
  it done: `cargo check -p <crate> --target wasm32-unknown-emscripten` (emsdk container,
  the recipe in `tools/`-adjacent notes) or `--target aarch64-linux-android`. Landing
  such a change without it means the next person discovers it, usually in the target the
  operator actually wants.

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

## 8. Parallel lanes run in PERSISTENT WORKTREES (D-035, amended)

Lanes may run 2-3 at a time, each in its own stable branch + worktree — never in
the main checkout. Create/join one with `tools/lane-worktree.sh`:

```
tools/lane-worktree.sh create <lane>     # branch lane/<lane> + ../oh-lanes/<lane>
tools/lane-worktree.sh env <lane>        # prints the exports + cd you must use
```

Rules that make parallelism safe:

- **Your worktree is yours.** Your own files, your own index. (This is why
  `git add -A` is no longer a cross-lane hazard — but still commit explicit paths
  where practical.)
- **Your own target dir**, plus `CARGO_PROFILE_DEV_DEBUG=0` /
  `CARGO_PROFILE_TEST_DEBUG=0` and `CARGO_PROFILE_DEV_INCREMENTAL=false`
  (`env` prints them). Separate worktrees have different absolute source paths, so
  a *shared* target dir would thrash cargo's fingerprints into full rebuilds; and
  the main `target/` is 63 GB of mostly debug info, so three of those would fill
  the disk.
- **Never remove or force-clean a worktree or branch.** Branches are persistent and
  the orchestrator lands them (squash, per D-025). `lane-worktree.sh remove`
  refuses unless the branch is merged — that refusal is deliberate; do not work
  around it. Never `cargo clean` a directory that is not yours.
- **Prefer targeted checks** (`cargo test -p <crate>`, `cargo clippy -p <crate>`)
  while other lanes are running; the full gate is the orchestrator's job at
  quiescent points. A cold first build in a fresh worktree is expected to be slow.
- **If you see a compile error or a failing test in a file you do not own, it is
  not yours** — another lane is mid-edit in its own worktree, and your worktree
  cannot see their uncommitted work anyway. Do not "fix" it; re-run, and if it
  persists in *your* files, report it.
- Report disk pressure rather than deleting anything.

## 9. Diagnosing a dead lane (do this before theorizing about context)

A lane dying with an empty final message is almost always the **compaction path**,
not context exhaustion and not a token budget. The evidence trail, in order:

1. `process-terminal.json` + the `subagent.step.failed` event in the run dir
   (`/tmp/pi-subagents-uid-*/async-subagent-runs/<id>/`). The step record carries
   `tokens.windowPeak` and `exitCode`.
2. The child's `session.jsonl` (`.../run-0/session.jsonl`): count
   `grep -c -i compact` — **zero markers means compaction never succeeded**, which
   is the real signal (cumulative token totals are *not* — they are re-sent context
   per turn, so 20M+ is normal for a long lane).
3. `~/.pi/agent/settings.json`: `compaction.model` names the server that does the
   summarizing. Here it is deliberately `desktop/desktop` — a **remote
   workstation**, chosen so long prefills don't block the local worker model.
   If that box is asleep/busy/hosed, compaction fails, contexts grow unbounded
   (a 231K-token `windowPeak` was observed), and the run exits 1.

So: **check the compaction server's health first** (`curl -s -o /dev/null -w '%{http_code} %{time_total}' <url>/v1/models`;
a port that accepts TCP while HTTP hangs = a wedged server, not a network/DNS
problem). A 2026-10-17 investigation burned ~10 tool calls on KV-exhaustion and
model-name theories before this; the layout (localhost = worker sglang, remote
desktop = compaction) is intentional and should not be "fixed" by pointing
compaction at the local model — that steals prefill from the workers.
