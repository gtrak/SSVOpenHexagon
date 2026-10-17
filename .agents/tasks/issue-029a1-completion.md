# Task 029a1-completion — finish the replay-store lane's evidence trail

The main implementation is ALREADY LANDED on master and orchestrator-verified:
`14167fc6` (RealReplayStore) + `7196f522` (fmt/clippy), gate 4/4, **753 tests**
(750 + the 3 new `replay_store.rs` tests: round-trip, idempotent dir, failure
path). The windowed composition wires `RealReplayStore` rooted at `base_dir`;
headless keeps `NullReplayStore` (disk-silent, matching the C++ `inReplay()`
gate, `HexagonGame.cpp:970`).

The previous worker DIED mid-device-verification (it read four screenshots in a
row and blew up). **This task is only the missing tail. Do NOT rewrite the
implementation; do not refactor `replay_store.rs`.**

Note: it also left a temporary `debuggable = true` in
`crates/oh-android/Cargo.toml` — the orchestrator already reverted it. If you
need it for `run-as`, re-add it, use it, and **revert it in a commit before you
finish**. Say which you did.

## What is missing (exactly these three things)

### 1. On-device proof that the replay file is actually written
Rebuild the APK and install it:
```
cd /home/gary/dev/SSVOpenHexagon
export CARGO_APK_RELEASE_KEYSTORE=$PWD/tools/android/openhexagon-release.keystore
export CARGO_APK_RELEASE_KEYSTORE_PASSWORD=openhexagon
export RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
. tools/android-env.sh && bash tools/package-android.sh
```
Device: `adb -s 192.168.10.138:38721` (wake with `KEYCODE_WAKEUP` first;
`pm clear` for a clean slate). Play a level, die, return to the menu, then
prove the file exists **with TEXT, not screenshots**:

    adb -s 192.168.10.138:38721 shell run-as info.vittorioromeo.openhexagon find files -name '*.z'
    adb -s 192.168.10.138:38721 shell run-as info.vittorioromeo.openhexagon ls -lR files/Replays

(`run-as` needs the debuggable flag — that is what it is for. If the release
build cannot be made debuggable, say so and instead prove it with the debug
build; do not fabricate evidence.)

**HARD RULE: your device session is TEXT-ONLY. At most ONE screenshot read for
the whole task, and only if you are truly stuck. Report `find`/`ls` output as
the evidence.** Prefer `dumpsys`/`logcat` greps (`logcat -d | grep -i replay`),
`pidof`, `find`, `ls -l`. If you find yourself reading three images in a row,
STOP and report — that is the failure mode that killed the last attempt.

If no replay appears, DO NOT hand-wave: report the negative result with the
`logcat` lines (the port logs on the `Replay` channel) and say where you think
it went wrong. A precise negative is worth more than a vague positive.

### 2. The deviations register row (issue 28's rule — a fixed deviation gets a row)
Append to `.agents/plans/001-rust-port/deviations.md` following the EXISTING
format exactly (read the last ~40 lines first; several rows were recently added
for 025j). The row: the `ReplayStore` seam was a no-op (`NullReplayStore`) so
no replay was ever persisted; NOW the windowed composition writes the C++
`Replays/<levelId>/<diff>x/<name>.z` via `create_dir_all` +
`CompressedReplayFile::serialize_to_file` (`HexagonGame.cpp:1067-1097`), and
the headless paths stay disk-silent (matching `HexagonGame.cpp:970`'s
`inReplay()` gate). Give it the id the file's scheme implies.

### 3. A short report in `.agents/tasks/issue-029a1-replay-store.md`
Append a `## Report` section: the citations as implemented, the impl's
placement + why, the host tests, the DEVICE evidence (the literal `find`/`ls`
output), and residual risk. If you changed nothing in the implementation, say
so explicitly ("implementation unchanged; this commit adds evidence + the
register row").

## Hygiene
- Commit every 3-5 tool calls. No worktrees (D-035). No emulator. Never touch
  non-phone processes.
- The issue-29 audit may still be writing `.agents/audits/*.md` in the
  background — do not edit or commit that directory.
- Keep the gate green: `bash tools/gate.sh` (expect 753 tests).
