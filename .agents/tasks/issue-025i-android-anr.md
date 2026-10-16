# Task 025i — the Android ANR (the game plays, then hangs under sustained input)

Lane 025i. Master green (gate 4/4, 749 tests) at `229e4cd3`. **No worktree**
(D-035): main checkout; commit every 3-5 tool calls.

## The symptom (device-verified; both the milestone and the defect)

**The game PLAYS on the phone.** Reproduced by the orchestrator from a
fresh install, TOUCH-ONLY: the IME appears, a name is typed, a center-band
tap creates the profile, the main menu appears, and a level starts — the
screenshot shows the live tutorial level ("BABY STEPS" by Vittorio Romeo,
running clock 8.795, "TUTORIAL MODE", the player hexagon with trail,
correct rendering).

**Then it hangs.** Three separate ANR records in `dumpsys dropbox`:

```
Subject: Input dispatching timed out (... is not responding. Waited 5002-5006ms for KeyEvent/MotionEvent)
CPU usage ... with 99% awake:
  103% <pid>/info.vittorioromeo.openhexagon: 101% user + 1.5% kernel
```

So: the app saturates one core (~103%) and stops servicing the input queue
until Android ANRs it. Reproducible across runs (16:50, 17:38, 18:12).

## Where to look (hypotheses, in order — verify, do not guess)

1. **Frame pacing / the event loop.** `HandleableEventLoop`'s
   `about_to_wait` (`crates/oh-render/src/window.rs:1129-1141`) sets
   `ControlFlow::WaitUntil(now + 1/limit)` when
   `effective_framerate_limit() > 0`, else `ControlFlow::Poll`. The shipped
   config is `limit_fps=true, max_fps=200` — but CHECK what
   `effective_framerate_limit()` actually returns on Android (`:732-736`),
   and whether `WaitUntil` is being honoured there. Note the app pins ONE
   core, which looks like a spin, not a paced loop: confirm whether the
   loop is running uncapped (Poll) because the limit resolves to 0, or
   because `WaitUntil` is being overridden by continuous redraw requests
   (a `request_redraw` per frame from the winit android backend would
   defeat WaitUntil → busy loop at O(1 frame) cost, a classic winit-android
   interaction).
2. **The lifecycle.** Android pauses the activity (screen off / background)
   and the app must stop rendering; the `BadSurface` present path was
   already made non-fatal (f74b8aa4), but the LOOP may keep spinning frames
   while paused, burning CPU and starving the input dispatcher. Check
   whether `WindowEvent::Suspended`/`resumed` (or winit's android
   equivalents) stop the frame callback.
3. **A per-frame leak building pressure** — e.g. the texture/atlas cache
   growing, or per-frame allocations (the 025c texture-cache eviction work
   capped one such case) causing GC/allocation stalls. Look at the
   frame-time numbers the port already tracks (`msUpdate`/`msDraw`) if you
   can surface them, and at memory in `dumpsys meminfo`.
4. **Input handling cost** — the 025d/025f event folds and the 9-zone grid
   run per event/frame; confirm they are O(1) and not, say, re-running a
   profile save per event.

## What to do

1. **Reproduce with evidence**: fresh install, play with touch, and capture
   the ANR window (logcat + `dumpsys dropbox` + the CPU line) plus whatever
   the app itself logs. The orchestrator's repro is above; make it
   repeatable and note the steps.
2. **Diagnose precisely** (which hypothesis holds, with file:line and
   evidence) — a targeted temporary probe is fine, but **revert it before
   your final commit** and say so.
3. **Fix it 1:1 with the C++ where a C++ rule exists** (SFML's
   `setFramerateLimit` sleep semantics; the C++ `GameWindow::run` loop;
   the SFML android pause/resume behaviour). Where the C++ has no
   equivalent (winit-specific), say so explicitly and justify the fix in
   the commit message — this is a platform-behaviour decision, and the
   operator wants deviations recorded (issue 28's rule: add the register
   row in the same commit).
4. **Acceptance on the device**: play for at least a few minutes of real
   touch input — rotate, die, restart, return to the menu, re-enter a level
   — with NO ANR (verify with `dumpsys dropbox` showing no new ANR entry
   for our package after the run) and the app's CPU not pinned (check
   `top`/`dumpsys cpuinfo` during play; a paced loop should sit well below
   100%). Report the numbers.
5. **Report** (short): the diagnosis with evidence, the fix + its
   justification (or the C++ citation), the before/after CPU + ANR
   evidence, the register row if a deviation was taken, and residual risk.
   Keep the host gate green (`tools/gate.sh`, 749 tests).

## Hygiene

- Commit every 3-5 tool calls (three lanes have lost work to timeouts).
- No worktrees. No emulator. Never touch non-phone processes.
- Prefer histograms/`dumpsys` greps over image reads; at most ONE image
  read, at the very end.
