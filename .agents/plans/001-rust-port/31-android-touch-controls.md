# Issue 31 — Android touch controls (the tutorial is unpassable on touch) + the dead key-state surface

Read `.agents/WORKER-HYGIENE.md` first. Operator report (2026-10-17, real device):
**"I can't get through the android tutorial because it wants me to pass a 'shift'
key input, and it hangs non-responsive. We need a touch button overlay I can
press."**

**Confirmed, and the operator is exactly right about the key:**

```
_RELEASE/Packs/tutorial/Scripts/Levels/babysteps.lua:154  "by default, use left shift"
_RELEASE/Packs/tutorial/Scripts/Levels/babysteps.lua:167  l_addTracked("step1_trFocus", "focused (left shift)")
_RELEASE/Packs/tutorial/Scripts/Levels/babysteps.lua:174  step25 and step0_trRotateLeft == "yes!" and
                                                          step0_trRotateRight == "yes!" and step1_trFocus == "yes!"
```

Step 2 cannot be completed without focusing with **Left Shift**, and on a
touch-only device there is no Left Shift — so the level never advances. The same
wall exists for **swap** (`_RELEASE/Packs/hypercube/Scripts/Levels/evotutorial.lua:49`
`l_setSwapEnabled(true)`, `:78` "Press space or middle mouse button to swap") and for
focus generally (`hypercube/Scripts/Levels/slither.lua:96` "remember, you can focus
with lshift!"). A tap cannot substitute: SFML/winit turn a touch release into a
**left** mouse event (`crates/oh-app/src/windowed.rs:1437-1438`), and the game's
focus/swap are keyboard/XButton inputs.

## 1. The second, independent defect this exposes — verify and fix it

While tracing the above, the input surface turned out to be dead in the port:

```rust
// crates/oh-game/src/hexagon_game.rs:2699-2706
fn is_key_pressed(&self, _key_code: i32) -> bool {
    // C++ `window->getInputState()[...]` — no window in this port.
    false
}
fn is_mouse_button_pressed(&self, _button_code: i32) -> bool { false }
```

The C++ does not stub these:

```
HGScripting.cpp:89-98    u_isKeyPressed        → window->getInputState()[ssvs::KKey(mKey)]
HGScripting.cpp:135      u_isMouseButtonPressed→ window->getInputState()[ssvs::MBtn(mKey)]
Config.cpp:270           the default bind table row (LShift for focus; XButton1 for swap)
                        — DERIVE the exact defaults from this region, do not guess
MenuGame.cpp:1877        the same idea read directly: isKeyPressed(sf::Keyboard::LShift)
```

So `u_isKeyPressed`/`u_isMouseButtonPressed` return `false` on **every platform**
today, and the game has no path from a real key state into gameplay. That is a D-014
citation deviation, and it is the same plumbing this issue needs: a real
`InputState` already exists (`crates/oh-render/src/input.rs:78`,
`InputState::is_key_pressed`) but is not reachable from the game/Lua host.

**Fix this as part of this issue** — an overlay that synthesizes keys into a dead
input path would appear to work in tests and do nothing on the device.

## 2. Requirements

1. **Plumb the real input state into the game/Lua host** so
   `is_key_pressed`/`is_mouse_button_pressed` consult the live `InputState`,
   mirroring the C++ `window->getInputState()[...]`. Headless keeps a defined
   behavior (the C++ has a window; the port must document what headless returns and
   why, and it must stay deterministic).
2. **The touch overlay** (the operator's ask): press-and-hold on-screen buttons
   during **gameplay only** that synthesize the *bound* inputs:
   - **focus** (Left Shift by default) — the tutorial blocker;
   - **swap** (the swap bind — from the config table; note the shipped tutorial
     prose says "space or middle mouse", so read the actual bind rather than the
     prose);
   - rotation left/right for the tutorial's step-0 gates (`step0_trRotateLeft` /
     `step0_trRotateRight` require rotating **while focusing** — so both buttons
     must be usable simultaneously with focus: **the overlay must support
     multi-touch, and a held focus + a tap must both register.** A single-touch
     toggle will not pass step 2).
3. **Exactly the C++-bound semantics.** The overlay must inject the same key codes
   the config binds, so replays recorded with synthetic input decode identically
   (the replay stores input events, not "touch"): no new input *kind* may leak into
   the game, or replay compatibility breaks. Verify against the codec.
4. **Do not collide with the menu's existing touch grid.** 025f's 9-zone
   `MenuTouch` seam (`windowed.rs:249-286`, the shared touch sink) owns menu
   touches; the overlay is a gameplay-layer concern and must not fire during
   menus/menu transitions, and must not steal touches from the menu grid.
5. **Layout/visibility**: buttons only while playing (not on menus, not on the
   boot/loading or load-results screens), sized for the real window (the 025g
   lesson: the config size is not the window size — a 1530x678 phone window with a
   1024x768 config), and drawn so they do not hide the play area's essential
   elements. Half-transparent, matching the game's own aesthetic is fine — this is
   a sanctioned deviation (D-034: input/window deviations are fixes), so
   **document it as a deviation with its rationale** in `deviations.md`.
6. **Pause/exit affordance**: an overlay that appears in gameplay should give a way
   out (the C++ has Escape/back). Check what the port's Android back handling does
   today and note it — do not silently add a new quit path that bypasses the
   menu's `go_to_menu(true, false)` contract (029d1).

## 3. The "hangs non-responsive" half — get evidence, do not assume

The operator reports the app hanging. Two very different causes are possible and
must be distinguished **before** any fix is claimed:

- **not a hang at all**: the tutorial simply cannot advance (waiting forever for
  `step1_trFocus`), so it looks frozen while the engine is perfectly healthy;
- **a real ANR**: grab the evidence (`adb logcat` around the event, and the
  dropbox ANR traces if present) and check it against **K3** (the known arm64
  `lj_asm_trace` ANR from 025i/029k1) — if it is K3, say so and cross-reference it
  rather than opening a duplicate.

Evidence is text (logcat/`dumpsys`) plus at most **one** image read (a screenshot of
the overlay in play, showing the buttons over a live level). If the device is
unreachable (ADB listener down — check first), report **PENDING** rather than
inferring.

## 4. Verification

1. **In-process/headless (primary)**: the overlay's state machine — press → the
   bound key code is down in the input state; release → up; two simultaneous holds
   (focus + rotate) both register; the menu grid path is untouched when the overlay
   is inactive; replay determinism (a recorded replay still decodes to the same
   events).
2. **The Lua surface**: `u_isKeyPressed`/`u_isMouseButtonPressed` return the live
   state after §1 (this is a test that fails today: the stub returns false).
3. **The real acceptance test (device)**: start `tutorial` → `babysteps` on the
   phone, hold the focus button while rotating, and show `step1_trFocus` reaching
   `"yes!"` and the tutorial advancing past step 2. That is the operator's actual
   requirement.
4. **Regression**: the Android 9-zone menu grid still works (025f/025g), and
   desktop is unaffected (the overlay is Android/gameplay-gated; verify the
   desktop build has no overlay and no behavior change).

## 5. Hygiene

`tools/gate.sh` 4/4 before every commit; `cargo fmt --all`; bounded processes
(`timeout`); commit per milestone with **explicit paths**; at most one image read
(§3); device steps via `tools/android-env.sh` + `tools/package-android.sh`
(`CARGO_APK_RELEASE_KEYSTORE`, `RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"`);
**stop-and-report** on any wall.

## 6. Scope guard

The goal is **the operator can play the tutorial to completion on the phone**. A
correct, narrow overlay (focus + swap + rotate, multi-touch, gameplay-only, drawn
readably) that lands §1's plumbing with it is the deliverable. Do not redesign the
input model, do not add rebindable on-screen controls, and do not touch the desktop
input paths beyond making the C++-mirrored key state real.
