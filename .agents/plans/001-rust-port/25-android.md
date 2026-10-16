# Issue 25 — Android NDK port (`aarch64-linux-android`)

## Objective

Run the ported game on Android (ARM64) via the NDK: workspace builds for
`aarch64-linux-android`, an `android_main` binary, APK packaging, touch input,
and the Android storage model.

## Environment (set up 2026-10, orchestrator)

- SDK `~/Android/Sdk` (platform 34, build-tools 34.0.0, NDK **27.2.12479018**),
  Java 25, `cargo-ndk` installed (its `-- cargo` delegation mis-runs here — use
  the manual config instead).
- `tools/android-env.sh` — sources `CC/AR/CXX_aarch64_linux_android` (the NDK
  per-API clang, API 28) for the cc-rs crates; `.cargo/config.toml` has the
  `[target.aarch64-linux-android]` linker.
- **Verified green already**: `cargo check -p oh-game/-p oh-lua --target
  aarch64-linux-android` (LuaJIT 210.5.12 cross-builds via luajit-src).
- **oh-render blocker**: winit 0.30 pulls `android-activity` 0.6.1 which
  requires the `native-activity` OR `game-activity` feature (E0583 +
  explicit error today).
- **GLES risk**: the pack shaders (`Packs/*/Shaders/*.frag`) have NO
  `#version` (desktop-GL defaults, `gl_FragColor`) — GLES 3.0 needs
  translation (precision qualifiers + `out` var). Assess in the render lane.

## Key decisions (D-026 family)

- **aarch64-only first** (armv7 LuaJIT lacks GC64 and is a compatibility
  swamp; x86_64-android is emulator-only). Document if armv7 is wanted later.
- **`native-activity`** over `game-activity` (smaller surface; no Java game
  framework needed) — revisit only if GameActivity features are required.
- **Storage model**: Android has no CWD — ship `_RELEASE` inside the APK's
  assets and extract to the app's internal dir on first run; `oh-assets`'s
  relative paths then resolve (chdir or root-remap at the `oh-app` layer).
- **Touch input**: the engine already has finger bitmaps + the
  `finger_down_positions` Renderer seam (the C++ touch controls,
  `updateInput_UpdateTouchControls`) — map android-activity pointer events to
  the existing `InputState` fingers.
- **Steam/Discord/egui console**: stay feature-off / absent on Android (the
  default build already never links them; the egui overlay is desktop-only —
  document).
- **Audio**: check whether any device sink exists at all (desktop included);
  if not, an AAudio sink is a documented follow-up, not a blocker for this
  issue's build/run scope.

## Files

| File | Change |
|---|---|
| `.cargo/config.toml`, `tools/android-env.sh` | done (above) |
| `crates/oh-render` features, `crates/oh-android/` or `oh-app` android bin | new |
| `tools/package-android.sh`, `app/` gradle or `cargo-apk` metadata | new |
| `.agents/plans/001-rust-port/25-android.md` | this file |

## Sub-lanes

| Lane | Scope |
|---|---|
| 25a | workspace `cargo check --target aarch64-linux-android` green (the android-activity feature choice, remaining unix-only fixes; tests stay host-only) |
| 25b | the android binary: `android_main` + winit/EGL event loop + GLES assessment (shaders) |
| 25c | APK packaging (cargo-apk/xbuild/gradle) + assets extraction + touch input |
| 25d | (follow-ups) armv7, AAudio, on-device instrumentation |

## Verification

- `cargo check --workspace --target aarch64-linux-android` green.
- **Real hardware (2026-10-16, the sanctioned on-device target)**: moto g
  power (2021), Android 11, arm64-v8a, WiFi debugging — adb
  `192.168.10.138:38721` (mDNS-discovered; paired once). The emulator is
  TORN DOWN: its gfxstream/SwiftShader backend ballooned to 28.8 GB host
  RSS (OOM postmortem) and a lane's raw `-gpu auto` launches made CUDA
  allocations that disturbed other GPU workloads — software emulation is
  no longer used while real hardware is available (the bounded
  `tools/infra/emulator-start.sh` stays for emulator-less contingency).
- Real-device findings fixed: the LuaJIT `__clear_cache` dlopen failure
  (cshim 23d68ee3); extraction 222/222 verified on device; the full C++
  boot order runs (Profiles/Replays created, NoSteam/NoDiscord continue).
- Linux gate stays 4/4 green.

## 025f — Android text entry: the system IME (D-031 route 1)

The C++ has no Android text-entry path to transcribe (D-031: the IME
wiring lived in SFML's own Android layer) — the lane is a deliberate
platform addition, and its acceptance is on-device usability:

- **The edge wiring** (`oh-app`): `MenuSlotAdapter::refresh_input` (the
  per-frame boot-flow hook) computes the `was → is` edge of the ported
  `States::is_entering_text` (`ETLPNewBoot` / `ETLPNew`) and calls the
  `windowed::SoftInput` controller ONCE per transition:
  `AndroidApp::show_soft_input(true)` on entering the text-entry states,
  `hide_soft_input(false)` (an unconditional hide — not only the
  implicitly shown IME) on leaving. The Android controller is
  `oh-app::android::ImeController` (the cloned `AndroidApp`; the winit
  loop thread IS the `android_main` thread); the desktop is the
  `NoOpSoftInput` (the C++ desktop build never asked for a soft input).
  The character pipeline is the 025d chain, untouched: the
  `KeyboardInput.text` field → the entered-text sink → `TextEntered` →
  `enteredChars`. Unit-tested over a recording controller (show-once on
  the enter edge, hide-once on the leave edge, quiet across held frames,
  re-arm on the second entering-text state).
- **cargo-apk 0.10 has no `windowSoftInputMode` (the 025f manifest
  finding)**: `[package.metadata.android]` is flattened
  (`#[serde(flatten)]`) into the `ndk-build` 0.10.0 `AndroidManifest`
  STRUCT (cargo-apk-0.10.0 `src/manifest.rs:48,100`), which quick_xml
  serializes and aapt compiles — there is NO hand-written / raw-XML
  manifest-override path, and the `Activity` struct
  (`ndk-build-0.10.0/src/manifest.rs:93-114`) has no
  `windowSoftInputMode` field (unknown TOML keys are silently dropped).
  So the activity ships WITHOUT `android:windowSoftInputMode`: the IME's
  show/hide is driven purely by the runtime `show_soft_input` /
  `hide_soft_input` calls (the explicit
  `InputMethodManager.showSoftInput(decorView, SHOW_IMPLICIT)` /
  `hideSoftInputFromWindow(token, 0)`). `stateVisible` is deliberately
  ABSENT (it would show the keyboard on the boot screens — against the
  acceptance). If a future device needs `adjustResize` (IME occluding
  the name field), the mechanism is a repo-local patched cargo-apk
  (orchestrator-authorized as a separate infra step, 025f decision) —
  the "everything else unchanged" proof is the baseline below.

### Baseline manifest (the 025f APK's `aapt dump xmltree`)

Command: `${ANDROID_HOME}/build-tools/34.0.0/aapt dump xmltree
target/release/apk/open-hexagon.apk AndroidManifest.xml`:

```text
N: android=http://schemas.android.com/apk/res/android
  E: manifest (line=1)
    A: android:versionCode(0x0101021b)=(type 0x10)0x1000100
    A: android:versionName(0x0101021c)="0.1.0" (Raw: "0.1.0")
    A: android:compileSdkVersion(0x01010572)=(type 0x10)0x22
    A: android:compileSdkVersionCodename(0x01010573)="14" (Raw: "14")
    A: package="info.vittorioromeo.openhexagon" (Raw: "info.vittorioromeo.openhexagon")
    A: platformBuildVersionCode=(type 0x10)0x22 (Raw: "34")
    A: platformBuildVersionName=(type 0x10)0xe (Raw: "14")
    E: uses-sdk (line=1)
      A: android:minSdkVersion(0x0101020c)=(type 0x10)0x1c
      A: android:targetSdkVersion(0x01010270)=(type 0x10)0x22
    E: application (line=1)
      A: android:theme(0x01010000)=@0x0103012a
      A: android:label(0x01010001)="Open Hexagon" (Raw: "Open Hexagon")
      A: android:hasCode(0x0101000c)=(type 0x12)0x0
      A: android:debuggable(0x0101000f)=(type 0x12)0x0
      E: activity (line=1)
        A: android:name(0x01010003)="android.app.NativeActivity" (Raw: "android.app.NativeActivity")
        A: android:exported(0x01010010)=(type 0x12)0xffffffff
        A: android:screenOrientation(0x0101001e)=(type 0x10)0x6
        A: android:configChanges(0x0101001f)=(type 0x11)0xfb0
        E: meta-data (line=1)
          A: android:name(0x01010003)="android.app.lib_name" (Raw: "android.app.lib_name")
          A: android:value(0x01010024)="oh_android" (Raw: "oh_android")
        E: intent-filter (line=1)
          E: action (line=1)
            A: android:name(0x01010003)="android.intent.action.MAIN" (Raw: "android.intent.action.MAIN")
          E: category (line=1)
            A: android:name(0x01010003)="android.intent.category.LAUNCHER" (Raw: "android.intent.category.LAUNCHER")
```

## issue 025f (D-031) — IME text entry + the 9-zone touch grid

**Problem (on-device):** the name-entry screen (`ETLPNewBoot`) was not
submittable by touch alone. The port showed/hid the system IME on the
text-entry state edge (step 1), and IME character taps delivered into
`enteredStr` (Gboard key taps → per-char `beep.ogg`), but Gboard's
bottom-right key acts as the IME **Done** action (NativeActivity exposes
no `InputConnection`/`EditorInfo` control — `inputType=0x0`,
`imeOptions=0x0`): it hides the IME and delivers NO key event to the
game, so `okAction()` never ran.

**Step 1 (committed `72678a67`):** `SoftInput` seam (`oh-app`
`windowed.rs`, `NoOpSoftInput` host default; `ImeController` in
`oh-app/src/android.rs` over `show_soft_input`/`hide_soft_input`), wired
in `MenuSlotAdapter::refresh_input` once per `is_entering_text`
was→is transition (show entering `ETLPNewBoot`/`ETLPNew`, hide on
leaving — unconditional). Unit test:
`soft_input_edge_shows_on_entering_text_and_hides_on_leaving`.

**Manifest finding (supervisor decision: runtime-only):** cargo-apk 0.10
`android_manifest` is a TOML-flattened struct — no
`windowSoftInputMode` field, no raw-XML override, unknown keys silently
dropped. The activity has NO `windowSoftInputMode`; the window manager
resolves `ADJUST_RESIZE` by default (baseline xmltree above). `stateVisible`
was rejected (keyboard would show on the non-text boot screens).
Repo-local cargo-apk fork only if the device proves the attribute
genuinely required.

**Step 2 (committed `8aeaacf9`):** the C++ 9-zone touch grid
(`MenuGame::update`, MenuGame.cpp:2802-2835 — documented as omitted in
25b; now proven load-bearing) ported 1:1 into the menu update path:
with exactly ONE finger down, the screen thirds dispatch — middle band
x-thirds → `left_action`/`ok_action`/`right_action`, top/bottom bands →
`up_action`/`down_action`; un-gated by state; each action sets
`touchDelay = 50` (the C++ debounce). PLATFORM-NEUTRAL (operator's
constraint): the grid + third-math + dispatch live in `oh-menu`
(`menu_game.rs` `update`, after the delay decrements, before
`menu.update()`), reading the new `MenuTouch` seam
(`oh-menu/src/seams.rs`: `finger_down_count` / `finger_down_positions`,
the C++ `window.getFingerDownCount()` / `getFingerDownPositions()`
analogues; `window_size()` ports `getWindowWidth/Height`). The windowed
client feeds it `MenuTouchSink` over the SAME shared touch sink the 25b
SG5 engine `Renderer` seam reads (the window's touch-event fold, LOGICAL
coordinates); headless/desktop feed `NoopTouch` (never fires). DESKTOP
MOUSE deliberately NOT wired to the zones in this lane (the
`MouseButtonReleased` exec path would double-fire; operator accepted the
parity as a later step). Host tests: `crates/oh-menu/tests/touch_grid.rs`
(8 tests: center-ok profile creation from entered name — the acceptance
path, left/right/up/down zone dispatch, zero-finger / two-finger
no-ops, exact-boundary no-ops).

**Open:** on-device acceptance re-run (type by IME + submit by tapping
the screen center → profile created; keyboard absent in non-text
states).
