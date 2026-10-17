# Task 025j — the missing swap interval (vsync) + the residual per-frame CPU

Lane 025j. Master green (gate 4/4, 749 tests) at `aa13960d` + the 025i ANR
fix. **No worktree** (D-035): main checkout; commit every 3-5 tool calls.

## Starting evidence (from lane 025i — do not re-derive)

025i fixed the **ANR** (a bare `PollEvent::Wake` early-returning winit's
`poll_events_with_timeout`, skipping `single_iteration` — frame and input
read — until Android's 5 s input-dispatch timeout; fixed by re-requesting a
redraw per frame, C++-faithful to `GameWindow::run`'s unconditional
per-iteration draw).

What REMAINS: the app pins one core (~102%, `android_main`) during play.
025i's measurements on the Adreno 610 phone:

| phase | cost |
|---|---|
| `gl.clear` | ≈ 10.9 ms (suspected back-pressure spin) |
| `msDraw` (draw + present submit) | ≈ 6.4 ms |
| `msUpdate` | ≈ 2.4 ms |
| loop period | ≈ 21 ms ≈ 46 fps (so it IS pacing; not an O(1) busy-spin) |

## The hypothesis to verify (orchestrator, from the sources)

**The port never requests an EGL swap interval.** `grep -rn
"swap_interval\|SwapInterval\|EGL_SWAP" crates/` is EMPTY. The C++ applies
`renderWindow.setVerticalSyncEnabled(vsync)` (`extlibs/SSVStart/.../GameWindow.hpp:94`)
and SFML's Android/EGL backend sets the corresponding surface attribute /
`eglSwapInterval`; our `Window::set_vsync` (`crates/oh-render/src/window.rs:720`)
only STORES a bool, and `GlParts::present` (`gl.rs`) swaps with no interval
bound — so the swap returns without blocking on the GPU, the next frame's
`gl.clear` waits on the still-busy pipeline (the 10.9 ms spin), and the loop
spins CPU. Shipped config: `vsync=false, limit_fps=true, max_fps=200`.

So this lane is a **fidelity fix**, not a perf hack: make the port's
vsync/framerate behaviour match the C++/SFML semantics.

## What to do

1. **Read the C++/SFML side** for both levers:
   - `GameWindow.hpp:93-95` (`setVerticalSyncEnabled(vsync)` at window
     creation) and `:215-220` (re-applied on recreate) — where `vsync`
     comes from (`Config::getVsync`).
   - The SFML fork's Android/EGL context code for the swap interval
     (search the fork at the pinned commit; if it uses
     `eglSwapInterval(display, 1)` or an `EGL_SWAP_INTERVAL` surface
     attribute, mirror that exactly and cite it).
   - `setFramerateLimit(fpsLimited ? maxFPS : 0)` (`:95`, `:219`) — the
     port already paces with `WaitUntil` (`window.rs:1249`); confirm it is
     applied on Android and that the limit value matches
     (`effective_framerate_limit`).
2. **Implement the swap interval** through the glutin/EGL path. Find the
   supported API in the pinned crates (`glutin 0.32`: `SurfaceAttributes`
   has a `swap_interval` builder for the EGL backend; `GlSurface::set_swap_interval`
   may exist) — prefer the glutin API over raw EGL FFI; if glutin exposes
   nothing usable on Android, an `eglSwapInterval` call via the existing
   `ndk`/`egl` bindings is acceptable WITH a register note.
   Wire it so `Config::get_vsync()` (and the options-menu toggle, which
   already flows to `Window::set_vsync`) actually changes the interval at
   runtime, matching SFML's `setVerticalSyncEnabled` (which recreates/applies
   the context attribute — check what SFML does and mirror the observable
   behaviour).
3. **Verify the CPU effect on the device**: measure the same phases
   (`gl.clear` / `msDraw` / `msUpdate` / loop period) and the process CPU
   during play, before vs after. Report numbers. Expected: with vsync ON
   the swap blocks (~16.7 ms at 60 Hz) and the core stops being pinned;
   with vsync OFF + `max_fps=200` the C++ still burns CPU by design
   (`fpsLimited` + a 200 cap), so **report what the C++ config actually
   implies** rather than forcing a low CPU number — the acceptance is
   "matches the C++ semantics", not "uses little CPU".
   - ALSO check the shipped `_RELEASE/config.json` vs the C++ default: if
     the C++ ships `vsync=false`, say so and show that the port now does
     the same thing the C++ would (and that toggling vsync in the options
     menu changes the behaviour live).
4. **Do not regress the ANR fix** (025i's per-frame redraw request) or the
   025c texture-cache/eviction work. No new ANR in `dumpsys dropbox` after
   a few minutes of play.
5. **Report**: the C++/SFML citations, the API you used, the before/after
   numbers (CPU + phases, with the config values), the register row (issue
   28's rule — this is a fixed deviation), and residual risk. Keep the host
   gate green (749 tests).

## Hygiene

- Commit every 3-5 tool calls. No worktrees. No emulator.
- Never touch non-phone processes.
- Histograms/`dumpsys` greps over image reads; at most ONE image read.
- Revert any probe before the final commit and say so.

## Report (completed 2026-10-16, phone 192.168.10.138:38721 — POCOPhone 1600×720, Adreno 610, Android 11, winit 0.30 / glutin 0.32.3 / GLES3)

**API used**: glutin 0.32 `GlSurface::set_swap_interval` (`crates/oh-render/src/gl.rs` `GlParts::set_swap_interval`) — EGL backend → `eglSwapInterval(display, 1|0)` (glutin 0.32.3 `src/api/egl/surface.rs:369-378`): the SAME display-level call the pinned SFML fork makes (`EglContext.cpp:197-200`, commit `2f11710`). `Window::set_vsync` (window.rs) applies it live on the live context (SFML's observable behavior — the C++'s full `recreateWindow` re-creation is the documented port gap, lane 029w); `ensure_android_gl` applies the stored value at GL materialization (EGL driver default is interval 1, so the shipped `vsync=false` needed the explicit request — the C++ `recreateWindow` ordering); `tick_fields` (windowed.rs) re-applies the config value on the live window each frame (the C++ `mustRecreate` re-apply slot) → the options-menu toggle changes behavior live. New host GL test: `backend.rs set_vsync_round_trips_and_applies_on_a_live_context`.

**Before/after** (shipped `_RELEASE/config.json`: vsync=false, limit_fps=true, max_fps=200; toggled live through the port's own options menu; CPU = whole process over 8 s, % of one core):

| phase | vsync OFF (shipped) | vsync ON (toggled) |
|---|---|---|
| in-game loop period | 2.4–2.9 ms (~360 fps, light tutorial level) | 16.6–17.7 ms (60 fps, vblank-locked) |
| in-game gl.clear | 0.4–0.5 ms | 5–11 ms (GPU drain at frame start — the 025i ~10.9 ms back-pressure signature) |
| in-game msUpdate / msDraw | 0.12–0.16 / 1.7–3.4 ms | 0.35–1.28 / 4.9–8.1 ms |
| in-game process CPU | 76% | 47% |
| menu period | ~2 ms (~500 fps) | 16.6–16.8 ms (60 fps) |
| menu process CPU | 71% | 45% |

Before the fix the menu vsync toggle was a no-op (period stayed ~2 ms). After: OFF→ON flips the loop from free-run to vblank-locked (2.5 ms → 16.7 ms) and ON→OFF back (16.7 ms → 2.3 ms), in both the menu and in-game. The OFF numbers are the C++'s explicit `eglSwapInterval(0)` semantics (uncapped by design); the ON numbers match the SFML/EGL blocking-swap behavior (025i's in-game 10.9 ms gl.clear + 21 ms period re-derive: that measurement ran under the driver-default non-blocking swap — the gl.clear cost is GPU pipeline back-pressure, not vsync blocking).

**Register**: `.agents/plans/001-rust-port/deviations.md` — K1 promoted to **F4** (fix + citations + these numbers); **K2** filed (NEW deviation diagnosed, not fixed): the port's 200 fps cap (`WaitUntil` pacing) is nullified on Android — winit 0.30's android `poll_events_with_timeout` (mod.rs:549-554) zeroes the wait whenever a redraw is pending, and 025i's ANR fix re-requests a redraw every frame → the menu/light levels free-run at ~2–3× the C++'s 200 fps cap (C++: SFML `Window::display` `sleep(5 ms − elapsed)`, Window.cpp:372-384, verified). Fixing it touches the 025i interaction — needs its own lane + device re-verification.

**ANR / 025i / 025c**: no regressions. `dumpsys dropbox` — last ANR entry 18:36 (pre-fix builds); ZERO ANRs across the ~35-min 025j session (19:46–20:20, both vsync states, menu + in-game). Texture cache untouched (no texture-path changes in this lane).

**Probes**: all temporary probes reverted in commit `dc02da8f` (verified: both touched files byte-identical to the pre-probe commit `5a34cffb`).

**Host gate**: fmt clean; 750/750 tests (749 baseline + 1 new); clippy clean; `cargo check --target aarch64-linux-android` clean.

**Residual risk**: K2 above (the pacing deviation — pre-existing, filed, unfixed). The C++'s full window re-creation on vsync change is NOT reproduced (the SFML-observable live apply is — documented, lane 029w owns the re-creation gap). Desktop/WL backends: `set_swap_interval` maps to the platform equivalent (GLX swap interval / WGL SwapInterval) — host-tested with a live GL context, not visually verified on a desktop display.
