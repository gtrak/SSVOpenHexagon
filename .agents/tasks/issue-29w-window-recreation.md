# Task 29w — window (re)creation: runtime setting changes must take effect, not wait for a restart

Read `.agents/WORKER-HYGIENE.md` first (§11 for the pinned C++) — and note this row is already queued in the
tracker (`STATUS.md` row **29w**, from the 029 audit series), whose citations are the authority:
`GameWindow.hpp:123,179-190` (`mustRecreate` → `recreateWindow`).

## The gap, and how two lanes just made it visible

The C++ recreates the window when a setting that the window owns changes: `mustRecreate` is set and
`recreateWindow()` re-applies the window-owned settings (fullscreen, the antialiasing level, the size). The
port has no equivalent, which leaves real behaviour on the table:

- **047** applied the `fullscreen` config at BOOT only, and recorded that a runtime change is inert.
- **050** plumbed `antialiasing_level` into the three contexts, and recorded the same: "runtime level
  changes stay inert (the C++'s `mustRecreate` → `recreateWindow` has no port equivalent — the level only
  reaches the context from config at boot)". `reapply_resolution` already exists for the resolution half,
  so the size case may be done — CHECK it rather than assuming, and say what you found.

So a user who toggles fullscreen or the antialiasing level in the port's settings menu sees nothing happen
until a restart, where the C++ applies it immediately. That is the deliverable.

## What to do

- Find the C++'s trigger and re-application sites in the pinned source (`GameWindow.hpp:123,179-190` plus
  whatever sets `mustRecreate` — the Config setters 047 cited, e.g. `Config.cpp`'s `setAntialiasingLevel`
  /`setFullscreen` paths) and mirror the SPLIT between "the setting changed" and "the window is rebuilt".
- Web and Android have their own shapes and must not be broken: on the web a fullscreen change is a
  browser-API matter (lane 046 added that button) rather than a window recreation, and on Android there is
  no window style to recreate — so if recreation is desktop-only, state that and gate it accordingly, with
  the reason per platform.
- Recreation is genuinely risky (a live GL context is destroyed and rebuilt; the port has passes, VAOs,
  shader programs and the ImGui/egui state to re-establish). Reuse the existing creation path rather than
  writing a second one, and if the honest answer is that the port's structure does not support a clean
  recreation yet, REPORT that with the evidence and a size — do not half-build it (§3).

## Acceptance

A test that does not need a display would be ideal but may not exist; what is required is (a) the C++'s
trigger/re-apply split mirrored and cited, (b) the runtime change observable — the setting applied without
a restart — verified as far as this box allows (a windowed run under Xvfb, or a headless assertion that the
recreation request is set and consumed), and (c) a plain statement of what could not be verified without a
display/device. Host gate before each commit, `cargo check --target aarch64-linux-android` (§4), explicit
paths, `timeout`, and a `## Report`. Update the 29w tracker row and any register row your work closes.
