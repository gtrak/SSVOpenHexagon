# Task 025f — Android text entry: the IME (then the touch-keyboard fallback)

Lane 025f. Master green (gate 4/4, 728 tests) at `eb35f92e`. Read D-031
first — it frames this lane: **the C++ has no Android text path to
transcribe** (its text input came from SFML's own Android layer), so this
is a deliberate platform addition, and the acceptance is *on-device
usability*, not code shape.

## The problem (operator-reported, reproduced)

On the phone, the boot flow reaches the name-entry screen
(`ETLPNewBoot` — "type your name") and there is **no keyboard**: NativeActivity
never asks the IME to appear, so a phone without a hardware keyboard cannot
enter a profile name. (Verified: `grep show_soft_input crates/` is empty —
nothing ever requests it.)

## Already done (do NOT redo)

- The character pipeline is DONE and device-verified (lane 025d):
  android-activity's `TextInputState`/`KeyboardInput.text` →
  `Window`'s entered-text sink → the menu's `onEvent(TextEntered)` handler →
  `enteredChars`, gated by the ported `isEnteringText()`
  (`MenuGame.cpp:2862-2878`). Typing worked via `adb shell input keyevent
  32 29 34` (typed "dAF") and Enter created the profile.
- `AndroidApp::show_soft_input(bool)` / `hide_soft_input(bool)` exist
  (android-activity 0.6.1 `lib.rs:1071`, `:1080`) and `ndk 0.9`'s
  `NativeActivity::show_soft_input` is the underlying call.

## Goal

**Step 1 (preferred — try this first):** the system IME appears when the
game is entering text and hides when it leaves:
- Call `show_soft_input(true)` on **entering** a text-entry state and
  `hide_soft_input(false)`/"not always" on leaving. Use the port's EXACT
  transition points, not a guess: `isEnteringText()` is already ported
  (`oh-menu`, `States::is_entering_text`), so the natural site is the same
  per-frame hook that handles the boot/entered-text flow — compute the
  edge (was-entering → is-entering) and call once per transition, not
  per frame.
- Handle the manifest side: cargo-apk 0.10's activity metadata
  (`ndk-build` 0.10 `manifest.rs:93-114`) has **no** `windowSoftInputMode`
  field. Set it via `[package.metadata.android] android_manifest` (a
  hand-written manifest file — cargo-apk supports the override; check
  `apk.rs`'s `android_manifest` handling) with
  `android:windowSoftInputMode="stateVisible|adjustResize"` on the
  activity. Keep everything else in that manifest byte-identical to what
  cargo-apk generated (diff the built APK's manifest before/after with
  `aapt dump xmltree` to prove it).
- Verify on the phone: reaching the name screen makes the keyboard appear
  (`dumpsys input_method | grep -i shown`, or a screenshot showing the
  IME), typing produces glyphs in the field, Enter creates the profile.
- **Known hazard, report it precisely if hit:** NativeActivity's
  `InputConnection` sometimes shows the IME but delivers nothing (no
  committed text). If that happens, capture the evidence
  (`dumpsys input_method`, logcat from the IME, whether `TextEntered`
  fires) and STOP — do not hack around it silently.

**Step 2 (fallback — ONLY if step 1 fails on hardware, and say so
explicitly):** an in-game touch keyboard. Draw it with the existing
GL passes (the 025c `gl_passes` text/rectangle paths are live) and drive
it from the existing touch pipeline (`WindowEvent::Touch` →
`finger_down_positions` / the touch sink, 25b SG5). It needs: a layout of
key rectangles, hit-testing on touch release, mapping to the same
`enteredChars`/`TextEntered` path, and visibility tied to
`isEnteringText()`. Escalate to the orchestrator before building it if
step 1 fails — do not build both unprompted.

## Acceptance

- On the phone (moto g power 2021, adb `192.168.10.138:38721`, wake it
  first): a fresh install (`adb shell pm clear`) reaches the name screen,
  the keyboard is usable, a profile is created by touch-typing alone, and
  the main menu then renders `CURRENT PROFILE: <name>`. Evidence:
  screenshots (histograms fine) + the relevant `dumpsys`/logcat lines.
- The keyboard must NOT appear in non-text states (level select, in-game):
  show the before/after states.
- Host: gate 4/4 (728+ tests), aarch64 workspace check green, fmt/clippy
  clean. Any host-testable logic (the transition edge, the manifest
  content, key-hit math if step 2 happens) gets unit tests.
- Do not break desktop: the IME calls are android-gated.

## Environment / hygiene

- Build: `export CARGO_APK_RELEASE_KEYSTORE=$PWD/tools/android/openhexagon-release.keystore
  CARGO_APK_RELEASE_KEYSTORE_PASSWORD=openhexagon
  RUSTFLAGS='-C link-arg=-Wl,-z,max-page-size=16384'`;
  `. tools/android-env.sh && bash tools/package-android.sh`.
- Install: `adb -s 192.168.10.138:38721 install -r target/release/apk/open-hexagon.apk`.
- **Hygiene** (a lane died at 41.7M tokens): commit every ~10-15 tool
  calls; prefer `dumpsys`/logcat greps and PIL histograms over reading
  images; at most one image read, at the end; stop and report if the
  session grows long.
- No emulator. Never touch non-phone processes.
