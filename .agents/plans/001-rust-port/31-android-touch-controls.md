# Issue 31 — touch controls (the tutorial is unpassable on touch; Android first, wasm next) + the dead key-state surface

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

## 2b. THE STRATEGY — the C++ precedent, and where the synthetic focus actually goes

**The C++ already has touch controls, and they cover ONLY rotation.**
`HexagonGame::updateInput_UpdateTouchControls()` (`HGUpdate.cpp:589`, declared
`HexagonGame.hpp:347`, called `:682`) loops `window->getFingerDownPositions()` and sets
`inputImplCCW`/`inputImplCW` by screen half; the port already mirrors it verbatim
(`update.rs:740-762`). So **rotation-on-touch works today, and focus/swap have no touch
path in the C++ either — the tutorial is unpassable on the upstream Android build too.**
The overlay is therefore a sanctioned *deviation* (D-034 class: a new feature), not a
port defect. Record it as a deviation with that rationale; do not present it as
restoring C++ behavior.

**Inject at the input-bit level — the same layer the C++'s own touch controls write.**
The tutorial's gate resolves through:

```
inputFocused            (HGUpdate.cpp:208, from ib[input_bit::focus])
  -> addTid2StateInput(Tid::Focus, inputFocused)   (HexagonGame.cpp:364)
  -> Lua `mFocus`                                  (tutorial/babysteps.lua:107)
  -> step1_trFocus = "yes!"                        (tutorial/babysteps.lua:109)
  -> the step-2 gate                               (tutorial/babysteps.lua:170)
```

and `inputFocused` is also what slows the player (`HGUpdate.cpp:239`, `:251`). So the
focus button must make `input_focused` true and nothing else in the chain changes. Do
**not** make synthesized key events the primary mechanism: `ib` is not where the C++'s
own touch controls write, and a key-event path would have to reach `ib` anyway.

**Call order matters.** `inputFocused` is assigned from `ib` at `:208`, and the touch
controls run later at `:682` — mirror that order so the overlay's write lands last,
exactly as the C++'s rotation touch write overrides keyboard rotation.

**The double-count hazard — subtle, and it would look almost right.** The rotation path
reads *all* `finger_down_positions()`, so a finger held on the focus button would ALSO
count as a rotation finger (left half → ccw). The overlay's consumed pointers must be
excluded from what the rotation path sees, or holding focus spins the player — and
step 2 needs focus and rotation *simultaneously*, so this cannot be dodged.

**Multi-touch needs per-button refcounting.** Track pointer ids; releasing one finger
must clear a button only when no other finger still holds it (step 2 is finger A holding
focus while finger B taps rotate).

**Replay safety is free at this layer**, and this supersedes §2 requirement 3's
"inject only the C++-bound key codes": the replay stores the *derived input bits* per
frame (`update_input_record_current_input_to_last_replay_data`), not key events, so a
touch-generated replay decodes identically on desktop and no new event kind enters the
format. Injecting the bit is both simpler and more faithful than injecting a key.

**Drawing: reuse the game's own key-icon textures.** The C++ already loads
`keyFocus.png`/`keySwap.png`/`keyArrow.png`/`keyReplay.png` and draws them as key hints
when `Config::getShowKeyIcons()` (`HexagonGame.cpp:95-102`, `HGUpdate.cpp:~213`) — on a
phone those hints name keys that do not exist, so the overlay should use the same four
textures and the hint row may be suppressed.

**The `u_isKeyPressed` stub is NOT on this critical path.** The tutorial reads `mFocus`,
an input-bit-derived tutorial variable — not Lua key polling. Fix the stub (§1) for
Lua-facing correctness on every platform, but do not expect the tutorial to depend on
it, and do not let §1 become a prerequisite for §2.

**The port-side seam already exists — use it, do not invent one.** `register_engine_binds`
(`crates/oh-app/src/windowed.rs:1614-1650`) already registers exactly the inputs this
issue needs, through the port's own idiom:

```rust
two_state(Tid::Focus, HexagonGame::<GameApiState>::set_input_focused);  // windowed.rs:1648
two_state(Tid::Swap,  HexagonGame::<GameApiState>::set_input_swap);     // windowed.rs:1649
```

(mirroring `add2StateInput(game, getTrigger(Tid::X), ...)` — HexagonGame.cpp:357-360,
SSVStart `Utils/Input.hpp:209-219`; `trigger_of` reads the **live** config trigger, so a
rebind re-points the bind with no re-registration). The overlay must therefore drive
those same `Tid` two-state inputs — press → the `on` closure, release → the `off`
closure — instead of adding a parallel path. That gets §2b's ordering, rebinding and the
replay-bit record intact for free, and it is the port's own mechanism rather than a new
one.

The window's touch/input data path, for reference: `InputState { fingers, keys, btns }`
(`oh-render/src/input.rs:35-42` — the C++'s three bitsets) plus the sink array
`[Option<Vec2>; FINGER_COUNT]` behind `WindowTouchRenderer::finger_down_positions`
(`windowed.rs:229-247`, fed by the same shared touch slot the 025f menu grid uses).
**Excluding the overlay's consumed pointers from that array is the concrete form of
§2b's double-count hazard.**

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

## 5. Platform-neutral by construction — the wasm build will need this too

Operator directive (2026-10-17): "I will want this for wasm builds as well." So the
overlay must be a **platform-neutral seam** in the D-032 sense, not an Android path:
it consumes a *pointer feed* (`id, x, y, phase`) plus a *view size*, and writes held
button state into the shared input state of §2b. Android feeds it from the existing
shared touch sink (`windowed.rs:203`, `:249-286`); the web target will feed it from
canvas pointer events — **but that feed does not exist yet** (see issue 32).

- **Do not gate the overlay on `#[cfg(target_os = "android")]`.** Gate it on "the
  platform provides a pointer/touch feed" plus an enable flag, so desktop can exercise
  it in tests and the web target inherits it for free when 32 lands.
- **Design for the web's event model now**, so 32 only has to fill a feed: pointer
  events rather than winit, `pointerdown/pointermove/pointerup/pointercancel` tracked by
  `pointerId`, and **pointer capture** so a finger dragged off a button still delivers a
  release (otherwise a button can be left stuck down).
- **The canvas' size is not the config size** — the same trap 025g fixed for the phone
  window (`Config::getWidth()/Height()` vs the real window) applies to a CSS-sized
  canvas with a device pixel ratio. Map pointer coordinates to the overlay in the
  *view's* space, not the config's.
- **Dependency, stated honestly**: 31 delivers the seam + the Android device
  verification. The web half cannot be verified until 32 exists; do not claim browser
  parity for this feature, and leave the seam ready rather than stubbing a web feed.

## 6. Hygiene

`tools/gate.sh` 4/4 before every commit; `cargo fmt --all`; bounded processes
(`timeout`); commit per milestone with **explicit paths**; at most one image read
(§3); device steps via `tools/android-env.sh` + `tools/package-android.sh`
(`CARGO_APK_RELEASE_KEYSTORE`, `RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"`);
**stop-and-report** on any wall.

## 7. Scope guard

The goal is **the operator can play the tutorial to completion on the phone**. A
correct, narrow overlay (focus + swap + rotate, multi-touch, gameplay-only, drawn
readably) that lands §1's plumbing with it is the deliverable. Do not redesign the
input model, do not add rebindable on-screen controls, and do not touch the desktop
input paths beyond making the C++-mirrored key state real.
