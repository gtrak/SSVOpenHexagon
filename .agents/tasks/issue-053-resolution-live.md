# Task 053 — the resolution change must take effect live (the last window-owned setting that waits for a restart)

Read `.agents/WORKER-HYGIENE.md` first (§11 for the pinned C++), then the register's **025g** row and
`.agents/tasks/issue-025g-*.md` (the creation-frozen-surface deviation this lane has to resolve), and the
just-landed **29w** work (`.agents/tasks/issue-29w-window-recreation.md` + its `## Report`), because this
is the same family and the same method.

## The gap, stated by 29w as it landed (verify it, then close it)

The C++'s resolution change is a **live** call, not a recreation: `MenuGame::changeResolutionTo`
(`MenuGame.cpp:1039-1049`) calls the LIVE `getRenderWindow().setSize(…)` and never sets `mustRecreate`.
The port has no equivalent — the window/backbuffer size is frozen at creation (the **025g**
creation-frozen-surface deviation) and `reapply_resolution` is only the auto-resolution CONFIG recompute,
with no window call at all. So a resolution change in the options menu does nothing until a restart.

29w also recorded the same hole on the fullscreen path: the C++'s `setFullscreen` resizes to the config
resolution (`Config.cpp:644-645`), while the port's live fullscreen apply changes the style only. If that
fits the same mechanism, fix both in this lane; if not, say so and scope it.

## What to do

- Read the C++ live-resize path in the pinned source and mirror what it actually does — the window
  `setSize` AND whatever must follow it in the port for the observable to be right: the GL viewport, the
  canvas/backbuffer size, and the menu's layout (which runs in the real size — the 025g "config size !=
  real size" lesson is exactly what this row is about).
- **Reuse the existing resize machinery** rather than inventing a second path: the port already handles a
  window `Resized` event end to end (issue 032's real-size path feeds the canvas backing store and the GL
  viewport). The question is whether a programmatic size change can drive that same path.
- **Per-platform answer required, not assumed**: on the WEB the canvas is CSS-sized, so a "resolution"
  change is not meaningful the way it is on a desktop — and on ANDROID the surface is the screen. If this
  is desktop-only, state that per platform with the reason, as 29w did.
- If the honest answer is that the port's frozen-surface structure makes a live resize a large change,
  REPORT that with evidence and a size rather than half-building it — 29w set that precedent and it is a
  perfectly good outcome.

## Acceptance

Mirror 29w's shape, because it worked: (a) the C++'s live path mirrored and cited; (b) the runtime change
OBSERVABLE without a restart, verified as far as this box allows — preferably an Xvfb probe in the style
of `crates/oh-render/examples/window_recreation_probe.rs`, showing the size actually change on the live
window and the viewport/backbuffer follow; (c) a plain statement of what could not be verified. Update
the 025g row and any row your work closes, and the tracker.

Gates: host gate before each commit, `tools/check-android.sh` (§4 — window/surface code is platform
code), `tools/build-windows.sh --check` if you touch the desktop window path, and the wasm check if you
touch the web path.
