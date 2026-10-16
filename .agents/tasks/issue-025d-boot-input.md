# Task 025d — the boot screen does not advance from real device input

Lane 025d of issue 25 (Android). Master is green (gate 4/4, 718 tests) at
`f74b8aa4`. Literal-port discipline; commit after every green step.

## The bug (fully evidence-ed on real hardware)

On the arm64 phone (moto g power 2021, Android 11, adb
`192.168.10.138:38721`) the game boots, renders the complete boot screen
(hexagon + "PACKS LOADED 8 / LEVELS LOADED 37 / ASSETS LOADED 61 / ERRORS
0 / NO LOAD ERRORS / PRESS ANY KEY OR BUTTON TO CONTINUE" — confirmed by
screenshot), and then **never advances**: it stays in
`States::LoadingScreen` forever.

Orchestrator probes already run (a temporary `state-probe` in
`WindowedClient::run`, since reverted):

- `slot=Menu menu_state=LoadingScreen ignore_inputs=1 open=true` — for
  minutes, unchanged. The loop is live (~60 fps, hexagon rotating).
- `adb shell input keyevent 66` (Enter): no advance.
- `adb shell input tap 720 540`: no advance.

So the gate IS armed (`ignore_inputs=1`, state `LoadingScreen` — exactly
the condition `advance_boot_screen()` requires) and the app is running;
what never happens is the app-side drain seeing a release edge.

## What the lane before you implemented (already on master — do not redo)

`oh-menu::MenuGame::advance_boot_screen()` (the C++
`checkCloseBootScreens` port) existed but had **no app-layer call site**;
the previous lane added:

- `oh-render/src/window.rs`: a `ReleasedEdges` sink (shared-sink pattern),
  `Window::set_released_edges_sink`, and appends in the `apply_event` arms
  for `KeyReleased` / `MouseButtonReleased` / `TouchEnded` (the touch arm
  is documented as the Android seam for the C++'s `MouseButtonReleased`
  because SFML's window layer turns a touch release into a mouse release).
- `oh-app/src/windowed.rs`: `MenuSlotAdapter::refresh_input` drains the
  sink and calls `menu.advance_boot_screen()` when
  `!edges.is_empty() && ignore_inputs > 0 && matches!(state,
  LoadingScreen | EpilepsyWarning)`.
- unit tests: `release_edge_advances_the_boot_screens`,
  `key_release_edge_advances_the_boot_screens`.

**The wiring exists; the events never arrive (or never arrive where the
drain runs).** Find out which, with evidence — do not assume.

## Investigation plan (in order)

1. **Is `refresh_input` even called on the windowed/android path?** Grep
   the real frame body (`WindowedClient::run`'s closure in
   `oh-app/src/windowed.rs`; the `frame()` free function beside it is a
   different, possibly unused, path — check which one the loop body
   calls). If `MenuSlotAdapter::refresh_input` is not in the live path,
   that is the bug: wire it at the C++ position (the once-per-frame
   pre-update hook, i.e. right around `engine.refresh_input(input)`).
2. **Do the events reach `apply_event`?** Add a temporary eprintln in the
   `TouchEnded` / `KeyReleased` arms and in the sink append, rebuild,
   tap/Enter, read `adb logcat -d | grep RustStdout`. Also confirm
   whether winit on android delivers `WindowEvent::Touch` (the SG5 path
   already consumes touch *positions*, so Touch events do arrive — check
   whether `TouchPhase::Ended` maps through `map_window_event`).
3. **Is the sink the same instance?** `set_released_edges_sink` is called
   on the window the loop uses (`windowed.rs:1818-1820`) — verify the
   adapter reads the same `Arc` (clone ordering; a sink set on a different
   `Window` handle would silently swallow everything).
4. **Is the drain timing right?** `refresh_input` runs before
   `engine.refresh_input`; winit's android event dispatch happens on a
   different thread/loop turn from the frame. If the drain runs before the
   events land in the sink, the edge is consumed on the next frame — still
   fine (the sink persists), unless someone clears it.
5. Check the C++ order in `MenuGame.cpp` (`checkCloseBootScreens` +
   `setIgnoreAllInputs`) and `GameWindow.hpp:42-83` (`runEvents`) so the
   fix matches the 1:1 semantics.

## Acceptance

- On the phone: boot screen advances LoadingScreen → EpilepsyWarning →
  SLPSelectBoot (C++ flow) with a tap AND with Enter; then the main menu
  renders (level selection list). Screencap evidence (paths + distinct
  colors) at each state; the `state-probe` trick (temporary, reverted
  before the final commit) is the cheapest way to prove transitions.
- Host: `tools/gate.sh` 4/4 (718+ tests), `cargo check --workspace
  --target aarch64-linux-android` green, `cargo fmt` clean.
- New/extended unit tests for whatever the actual defect was (the existing
  tests pass while the device fails — that gap must close: e.g. a test
  that drives the REAL `Window::apply_event` fold + the REAL adapter
  `refresh_input` together, rather than the sink in isolation).

## Worker-hygiene rules (from the 025c failure postmortem)

The previous lane (e9512005) failed with `500 status code (no body)` — the
local model server died AFTER session compaction at ~41.7M tokens / 325
tool calls. The worker CAN read images (pi model `local` declares
`input: ["text","image"]`; it had successfully read the screenshot). The
failure is context bloat, so:

- **Keep turns small.** Do NOT run 300+ tool calls in one session. Land a
  commit every ~10-15 tool calls, then keep going — the commits are what
  survive.
- **Prefer pixel histograms over reading PNGs.** `python3 -c` with PIL
  (distinct-color counts, top colors, and per-region means) is smaller and
  more decisive than an image attachment; read an image ONLY at the final
  acceptance step, once, with a small context (do it as the last action of
  a fresh short session if needed).
- If you feel the session is long (many rounds), STOP and report progress
  instead of pushing on — a partial report + commits beats a dead session.

## Environment

- Build: `export CARGO_APK_RELEASE_KEYSTORE=$PWD/tools/android/openhexagon-release.keystore
  CARGO_APK_RELEASE_KEYSTORE_PASSWORD=openhexagon
  RUSTFLAGS='-C link-arg=-Wl,-z,max-page-size=16384'` then
  `. tools/android-env.sh && bash tools/package-android.sh` (default
  arm64; exit code 0 expected here, unlike the x86_64 override case).
- Install/run: `adb -s 192.168.10.138:38721 install -r
  target/release/apk/open-hexagon.apk`, then `adb -s ... shell am start -n
  info.vittorioromeo.openhexagon/android.app.NativeActivity`.
- Wake the phone first if the screen is locked: `adb ... shell input
  keyevent KEYCODE_WAKEUP` (a locked screen shows a 1-color screenshot).
- Do NOT use the emulator (torn down; the CUDA/OOM incidents). Do NOT
  touch non-emulator/phone processes on this box. WiFi debugging may drop —
  reconnect with `adb connect 192.168.10.138:38721`.
- Commit after every green step; the previous lanes lost long stretches of
  work to timeouts with nothing committed.
