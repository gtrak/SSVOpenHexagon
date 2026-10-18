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
