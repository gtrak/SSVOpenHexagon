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
- An APK that installs and boots to the first frame on a device/emulator
  (MANUAL on-device check documented; emulator AVD optional).
- Linux gate stays 4/4 green.
