# Task 049 — the Android frame cap is SET but NOT ENFORCED (K2), and the record disagrees with itself

Read `.agents/WORKER-HYGIENE.md` first — §11 is directly relevant to this task (the C++ extlib sources
you will need to read are SUBMODULES, empty in your worktree; read them in the main checkout).

## Where this came from (both halves are mine, and they disagree — settle it with code)

Lane 047's config-application audit verdicted `limit_fps` and `max_fps` as **APPLIED** on Android, because
the port reads the values and they reach the pacing logic. The register's **K2** row says the opposite:
the cap is **NOT enforced** on Android, with a measured mechanism:

> winit 0.30's android `poll_events_with_timeout` short-circuits to a zero wait whenever a redraw is
> pending (winit 0.30.13 `src/platform_impl/android/mod.rs:549-554`), and 025i's ANR fix re-requests a
> redraw after EVERY frame, so `pending_redraw` is always true → the deadline never blocks → the loop
> free-runs. Measured on-device 2026-10-16 (shipped `limit_fps=true, max_fps=200`): menu ~500 fps,
> in-game ~360 fps on the light tutorial level, where the C++'s `Window::display` paces every sub-5 ms
> iteration to 200 fps (`sleep(5 ms − elapsed)` after the swap). CPU 76–71% vs the C++'s paced lower burn.

**K2 wins on the evidence** — it is measured, and 047's criterion was "reads the value AND produces the
C++'s observable effect", which fails on Android. So:

1. **Correct the audit's record** (it is a living document): the Android cell for `limit_fps` and
   `max_fps` becomes MISSING with a cross-reference to K2, in the spec's table and in the tracker row.
   Do not soften K2 to match the audit; the audit was the wrong one, and the correction should say so.
2. **Fix K2** if the fix is bounded (see below). A phone free-running 2–3× faster than the C++ is heat
   and battery the operator pays for, so this is a real defect, not bookkeeping.

## Why the fix is not "just use WaitUntil"

025i's per-frame `request_redraw` is what fixed the Android ANR (K3's family), and it is precisely what
makes the winit deadline useless there. So the fix CANNOT be "stop requesting redraws" unless you can
show it does not reintroduce the ANR — that trade is the whole reason this row is still open.

The C++ has the answer, and it is a sleep, not an event-loop feature: SFML's frame limiter lives in
`Window::display()` (`setFramerateLimit`'s `sleep(5 ms − elapsed)` after the swap; the clock restarts
each paced display, so a ≥5 ms iteration sleeps 0). Mirror that on Android at the equivalent point in
the port's frame, honouring the same configuration the desktop path already uses
(`effective_framerate_limit()`, `limit_fps`, `max_fps`, and vsync's precedence), and do NOT change the
desktop pacing (it works) or the wasm path (rAF already paces it).

**Read the C++ you cite** — SFML's `Window.cpp`/`RenderWindow` lives at
`cpp-original/extlibs/SFML/...` and SSVStart at `cpp-original/extlibs/SSVStart/...`, both SUBMODULES:
they are EMPTY in your worktree (hygiene §11) and populated in `/home/gary/dev/SSVOpenHexagon/
cpp-original/extlibs/`. Cite what you actually read, from there, and say in the report that you did.
Also verify winit's `poll_events_with_timeout` short-circuit yourself (the vendored source is in
`~/.cargo/registry`, or the crate's docs source; winit 0.30.13 `src/platform_impl/android/mod.rs`) —
if that citation is wrong, say so, because the whole root cause rests on it.

## Acceptance, and the honest boundary

- A **cited** answer to "is the cap enforced on Android": the winit short-circuit, and the port's call
  path, in code terms. If K2's mechanism turns out to be wrong, that is the deliverable — report it as
  the finding instead.
- If you fix it: the sleep/pacing in the port's Android frame, mirroring the C++'s site and semantics,
  with the configuration precedence respected, plus a test for the pacing ARITHMETIC on the host (the
  device's behaviour cannot be reproduced here).
- **The device measurement is the operator's** — the phone's ADB listener is currently down, so you
  cannot verify fps on hardware. State exactly what to measure when it returns (the same probe K2 used:
  `limit_fps=true, max_fps=200`, menu fps and in-game fps via the port's own log lines, expecting ~200
  and the C++'s lower CPU burn). A precise "pacing implemented and host-tested, device unverified" beats
  a claim, and the ANR non-regression is part of what the device check must confirm.
- Host gate before each commit; `cargo check --target aarch64-linux-android` (§4); no wasm build needed
  unless you touch a shared path. Update K2's register row (CLOSED with the landing hash, or still open
  with the new evidence) and the audit's two cells in the same commit as your change.

## Boundaries

`crates/oh-render/src/window.rs` (the pacing) + the register/audit records. Do not touch the game's
fixed-timestep logic (the cap is presentation pacing, not simulation), do not change desktop behaviour,
do not remove 025i's redraw policy, and append a `## Report`.

## Report

**The cited answer — "is the cap enforced on Android?" — was NO, and the mechanism is verified
end-to-end in the pinned sources (all read directly by this lane):**

1. **winit's short-circuit** (the whole root cause rests on it — verified in the pinned registry
   source, `~/.cargo/registry/src/index.crates.io-*/winit-0.30.13/`, the `Cargo.lock` pin):
   `src/platform_impl/android/mod.rs` — `EventLoop::run` pumps `pump_events(None, …)` in a bare
   loop (`mod.rs:495-508`), which calls `poll_events_with_timeout` (`:543`); there,
   `self.pending_redraw |= self.redraw_flag.get_and_reset()` (`:549`), then
   `timeout = if self.running && (self.pending_redraw || …has_incoming()) { Some(Duration::ZERO) }
   else { …the `ControlFlow::WaitUntil` deadline, `:556-562`… }` (`:551-555`) — i.e. the
   deadline is computed ONLY in the arm that is unreachable while a redraw is pending.
   `request_redraw()` sets `redraw_flag` (`:122`). The spec's `:549-554` citation is correct
   (the short-circuit block is `:551-555`, one line off).
2. **The port's call path**: `RunHandler::about_to_wait` (`crates/oh-render/src/window.rs`)
   runs the frame, sets `ControlFlow::WaitUntil(now + 1/limit)` when
   `effective_framerate_limit() > 0`, and — 025i's ANR fix — calls
   `gl.winit_window().request_redraw()` after EVERY frame. So `pending_redraw` is `true` before
   every poll → `Duration::ZERO` wait → the loop free-runs regardless of the deadline. K2's
   on-device numbers (menu ~500 / in-game ~360 fps vs the C++'s paced 200) follow.

**The fix (bounded — landed `88f353df`):** the C++'s own answer, mirrored at the C++'s own site.
SFML's frame limiter is a SLEEP in `Window::display()` — the pinned SFML `dacceddbd` (the
`GIT_TAG` in `cpp-original/CMakeLists.txt`'s CPM block): `src/SFML/Window/Window.cpp:207-219`
(`m_context->display()` → `sleep(m_frameTimeLimit − m_clock.getElapsedTime()); m_clock.restart();`),
`:175-181` (`setFramerateLimit` → `Time::Zero` for 0), `:230` (the `initialize()` clock start),
`Window.hpp:284` (`m_clock`), and the negative-sleep no-op in `src/SFML/System/Sleep.cpp:41-48`.
The port now has `frame_limiter_sleep(limit, elapsed)` (pure, ungated — `1/limit − elapsed`,
saturated at 0) + `Window::pace_display` (android-only: the `effective_framerate_limit()` gate —
`limit_fps`/`max_fps` precedence, vsync's swap interval independent, exactly as the C++'s
`recreateWindow` applies `:94` `setVerticalSyncEnabled` + `:95` `setFramerateLimit` together),
called in `about_to_wait` AFTER the frame (the swap, `tick_fields`'s `target.present()`) ran.
025i's per-frame `request_redraw` is UNTOUCHED — it is the ANR fix, and it is precisely what
defeats the deadline; "stop requesting redraws" was never on the table (nor "just use
WaitUntil"). The per-iteration sleep ≤ 1/min `max_fps` (5 ms at the shipped 200) is nowhere near
Android's 5 s input-dispatch ANR window, and the C++ does this exact sleep on this exact thread
in its Android build. Desktop pacing is byte-unchanged (`WaitUntil`/`Poll` as before); the wasm
path is byte-unchanged (rAF paces it; the new function is pure std, the `pace_display` call is
android-gated).

**Citations I actually read (hygiene §11):** the SFML sources from the populated oracle-deps copy
`/home/gary/.cache/ohoracle-deps/sfml-fork` (commit `dacceddbd` — the CMakeLists pin; the
`cpp-original/extlibs/SFML` submodule dir is empty in BOTH the lane worktree and the main
checkout, so the main-checkout path in the spec could not be read there); SSVStart's
`GameWindow.hpp` from the main checkout (`cpp-original/extlibs/SSVStart/…`, populated: `:95` the
limit apply, `:117-142` the `run()` loop, `:142` the `display()` call); winit from the pinned
cargo-registry source (above). **One register citation corrected:** the K2 row's SFML pin
`2f11710` (`Window.cpp:372-384` / `:440`) was stale — the actual CPM pin is `dacceddbd` and its
`display()` is at `Window.cpp:207-219` (the limiter logic itself matches the row's description
verbatim; only the pin hash + line numbers were wrong).

**Tests / gates:** host gate 5/5 green (`cargo test` workspace; the new
`frame_limiter_sleep_matches_the_cpp_arithmetic` pins the arithmetic — 0 → 0; 200 fps: 0 ms →
5 ms full frame, 3 ms → 2 ms, 5 ms → 0, 20 ms → 0 (the "≥ frame time sleeps 0" arm), 60 fps →
1/60 s within f32 rounding; `backend` 15 → 16 tests). `cargo check --target
aarch64-linux-android` (whole workspace) green — the android branch is invisible to the host
gate (hygiene §4). The audit doc's two Android cells needed no further change: the
orchestrator's `e96134de` already corrected them to "MISSING — see K2", and this fix is the
repair that row assigned to this lane (the record half was pre-done, per the dispatch).

**What I could NOT establish — the device half is the operator's (the ADB listener is down):**
when the phone returns, re-measure exactly as K2's probe did — shipped `limit_fps=true,
max_fps=200`: (a) MENU fps and IN-GAME fps from the port's own log lines (the
`msDraw`/`msUpdate`-adjacent frame timing the 025j session used — expect ≈200 fps, not ~500/
~360), and the process CPU settling to the C++'s paced lower burn (vs the 76–71 % free-run);
(b) the ANR non-regression: the dropbox `data_app_anr` count stays flat across the session
(025i's fix — the per-frame redraw — is untouched, but "stays flat" is the confirmation, not the
assumption). Status: **pacing implemented and host-tested, device unverified.**

**Register:** K2 row updated in the same commit as the change — CLOSED with the fix hash
(`88f353df`), the device half recorded as the operator's pending probe, the stale SFML pin
citation corrected.
