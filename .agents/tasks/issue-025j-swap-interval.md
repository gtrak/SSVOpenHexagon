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
