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

## Report

**The gap verified, then closed.** The C++'s resolution change is a LIVE call:
`MenuGame::changeResolutionTo` (MenuGame.cpp:1039-1053) — value guard :1041-1044 (a
resize to the already-configured resolution rewrites NOTHING, the auto-resolution
flag included), `setCurrentResolution` :1046, the LIVE `renderWindow.setSize(w, h)`
:1047 (never a `mustRecreate` mark), the `refreshCamera` / `adjustLevelsOffset` /
`adjustMenuOffset(true)` / `resetNamesScrolls` tail :1049-1052. The menu's
`onEvent(Resized)` (MenuGame.cpp:261-263) runs the SAME function with the event's
size — the config follows a user/WM resize in the MENU state only (the game state
has no Resized handler). 29w's recorded twin holds: `Config::setFullscreen`
(Config.cpp:639-649) carries the same live `renderWindow.setSize(res)`
(Config.cpp:644-645). All cites re-verified in the pinned source this lane.

**Not a big change** — the spec's escape hatch was available but not needed: the
port's `Resized`-event machinery already reached the GL viewport + the `GlParts`
size bookkeeping (issue 032's web arm, the `Event::Resized` fold's
`gl.resize`), and the menu layout/camera already re-derive on a size change
through the `refresh_if_stale` stale-detection (the 025g F1 seam — its tuple
carries the window size). The C++'s `changeResolutionTo` tail is therefore
already covered by the per-frame `refresh_if_stale`; what was missing was the
live window resize itself. The change: (1) `Window::set_inner_size` (the C++'s
live `RenderWindow::setSize`) — winit 0.30's `request_inner_size` (the `&self`
form) + the size bookkeeping's move with the request (SFML's `m_size`
immediate-update semantics); (2) the desktop arm of the ONE resize path —
`GlParts::resize` (the GLX/EGL backbuffer resize via `GlSurface::resize` +
`gl.viewport` + the size bookkeeping, the issue-32 web arm's same shape —
NO second path, NO context recreation); (3) the `tick_fields`'s desktop
MENU-state sync at the C++'s loop-head position: config-follows-window (the
folded `Resized` edge → `set_current_resolution` — the fold's triage suppresses
the port's own `set_inner_size` / fullscreen-transition deliveries, the C++'s
guard never fires on its own `setSize`'s delivery) + window-follows-config
(fired when the config's resolution CHANGED since the last frame — the C++'s
setter-driven `changeResolutionTo` trigger — NOT a config-vs-window diff, so a
`pixel_multiplier` boot's ×pm window is not pulled down to the logical config;
the `resolution_sync` primes to the config's boot resolution); the else-drain
discards the game-state's edges (the C++'s no-handler half — a menu RETURN
must not fire a stale follow); (4) the fullscreen twin — the mode-switched
`get_width()`/`get_height()` (Config.cpp:1102-1111's switch) reaching the same
`set_inner_size` (the style half was 047's; the resize half is this lane's).

**Per-platform answer.** DESKTOP: the live resize above (the C++'s
`changeResolutionTo` / `setFullscreen` window halves). ANDROID: inert by
design — the activity window's surface is the screen's (the platform owns it;
the `Resized` fold's bookkeeping covers its size changes); the C++ has no
android build. WEB: inert by design — the canvas is the page's CSS box (the
browser owns its size; the issue-32 real-size path feeds the backing store +
viewport); the C++ has no web build. The options menu's `resolution_modes()`
is the empty vector (the C++'s menu lists no per-resolution items either —
its `create<i::Single>` list is the same), so the probe drives the
`set_inner_size` / config path directly; the menu's "automatically set
resolution" item (`set_resolution_auto`) remains the documented 29r1
desktop-mode no-op (out of scope, unchanged).

**Acceptance.** (a) the C++'s live path mirrored + cited (above). (b) the
runtime change observable WITHOUT a restart: the Xvfb probe
`crates/oh-render/examples/resolution_live_probe.rs` — PASSED: the live
window moved 800x600 → 640x480 (winit's `inner_size` — the OS window's real
size, not the port's bookkeeping), the viewport is [0 0 640 480], the
backbuffer's bookkeeping follows on the `Resized` delivery, and the resized
backbuffer draws (a read-back-verified clear at the new centre,
read-before-swap order). Plus the headless integration test `oh-app`
`options_menu_resolution_change_moves_the_window_and_the_config_follows_an_os_resize`:
the setter-driven window-follow (one frame after the menu write — the 29w
sync's timing), the boot's no-shrink priming, the config-follow on an OS
`Resized` edge (with the C++'s value guard's flag half: a same-size edge does
not clear `windowed_auto_resolution`), the no-ping-pong (the window stays
where the resize put it), and the game-state's no-config-follow +
bookkeeping-follow. (c) NOT verifiable on this box: a human-eye view of the
resized menu (the probe's viewport/backbuffer/read-back observables stand in
for it); the GLX backbuffer's PHYSICAL resize (GLX exposes no backbuffer-size
query — the observable is the window's real inner size + the viewport + a
usable context; the EGL/ANGLE backbuffer halves are the same
`GlSurface::resize` call); the android/web no-ops (structural — the C++ has
no android/web build).

**Gates.** Host gate green at each commit (fmt / build / clippy `-D warnings`
/ 1033 tests / lua-parity); `tools/check-android.sh` green (the android-gated
unused-parameter fix); `tools/build-windows.sh --check` green;
`cargo check -p oh-web --target wasm32-unknown-emscripten` green (the emsdk
podman — the shared window.rs/gl.rs paths).

**Commits.** `9a8ff4b5` — the live window resize (the mechanism:
`Window::set_inner_size` + the desktop `GlParts::resize` arm + the
`Resized`-fold triage + the `tick_fields`'s 053 sync + the adapter comment
cleanup). Follow-up commit — the acceptance (probe + headless test) + the
register/tracker updates (the 025g F1 row's 053 note, the 29w row's residual
(c) closure, the new 053 row).
