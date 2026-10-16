# Task 025b — Android NDK: `android_main` binary, APK packaging, assets, touch

Lane 25b of issue 25 (`.agents/plans/001-rust-port/25-android.md`). Literal
1:1 port discipline (mirror C++ structure, f32/f64, no expression reordering).
25a is LANDED (101f920f): the whole workspace `cargo check`s for
`aarch64-linux-android`; winit carries `android-native-activity` (D-026).

## Environment (set up — do not reinstall)

- `~/Android/Sdk` (platform 34, build-tools 34.0.0, NDK 27.2.12479018), Java 25.
- Source `tools/android-env.sh` in every cargo-for-android shell (build
  scripts need `CC_aarch64_linux_android`).
- `.cargo/config.toml` has the aarch64 linker (NDK per-API clang, API 28).
- `cargo install cargo-apk` if you take the cargo-apk route (network OK).

## Goal

An installable, bootable APK: `cargo apk build` (or xbuild/gradle — pick one,
justify) produces an APK containing `lib<name>.so` (aarch64) + the `_RELEASE`
game content as APK assets, with an `android_main` entry that boots the same
windowed pipeline as desktop (`oh-app/src/windowed.rs`: boot dispatch,
MenuGame, HexagonGame) through winit's android event loop + glutin EGL.

## Sub-goals (commit after EACH green one)

1. **Entry crate + event loop**: new binary crate (suggest
   `crates/oh-android`, or an android-gated bin in `oh-app` — your call,
   justify) with `#[no_mangle] android_main(app: AndroidApp)` (android-activity
   native-activity) → `EventLoop::builder().with_android_app(app)` (winit
   `platform::android::EventLoopBuilderExtAndroid`) → reuse the existing
   windowed pipeline as much as possible. Refactor `windowed.rs` only as
   needed to share code with the desktop path (no copy-paste forks of the
   game loop; factor shared pieces).
2. **GL context on Android**: glutin EGL; request `Api::GLES3` (fallback
   GLES2 assessment) via `Display::new`'s API preference on android. glow
   loads GLES entry points. **Assess** which desktop-GL features the port's
   renderer relies on (VAOs, `#version 330`-era built-ins) and record gaps.
3. **Shader translation**: the pack `.frag`/`.vert` files
   (`_RELEASE/Packs/*/Shaders/*`) have NO `#version` (desktop defaults,
   `gl_FragColor`). Write a small deterministic translation pass (e.g.
   prepend `#version 100` + `precision mediump float;` for fragment shaders,
   keep the rest verbatim) applied only on android (or when the created
   context is GLES) — **unit-test the translation host-side** (pure string
   transform, no GL needed). Do NOT modify `_RELEASE` files.
4. **Assets/storage**: pack `_RELEASE` as APK assets (cargo-apk metadata
   `assets`); on first run extract to the app's internal dir
   (`AndroidApp::internal_data_path`) using `AndroidApp::asset_manager()`,
   then `std::env::set_current_dir` there so oh-assets' relative paths
   resolve. Skip extraction when the content is already present. Config/save
   writes land in the same dir (Android has no CWD).
5. **Touch input**: winit on android emits `WindowEvent::Touch`; map touch
   start/move/end to the engine's existing seam
   (`oh_game::renderer::Renderer::finger_down_positions`,
   `update_input_update_touch_controls` in `oh-game/src/update.rs:717` —
   already ported verbatim). Match the C++ touch-control semantics
   (src/SSVOpenHexagon/Core/HGUpdate.cpp `updateInput_UpdateTouchControls`):
   rotating to touch angle, tap = swap, etc. — read the C++ before writing.
6. **APK build script**: `tools/package-android.sh` (or document
   cargo-apk invocation in the plan file) — release build, ABI arm64-v8a.

## Constraints

- **Commit after every green sub-goal.** Never leave work uncommitted.
- Desktop gate is sacred: `tools/gate.sh` 4/4 + `cargo test --workspace`
  (680) at every landing point. The desktop path must be behavior-identical
  (the shader translation and android entry are cfg-gated).
- Do not modify `_RELEASE/` tracked content. Do not bump dependency versions.
- 16KB-page-size note: Android 15 devices require 16KB ELF alignment — check
  whether the NDK r27c defaults suffice (r27 targets 16KB by default for
  lld); note findings in the report.
- Headless host: **no on-device run is possible here.** Verification =
  build-level (APK assembles, `aapt dump badging` sane, `unzip -l` shows
  `lib/arm64-v8a/*.so` + assets) + host-side unit tests. On-device smoke is
  a documented MANUAL check. If you can spin an emulator cheaply (KVM?
  check `/dev/kvm`), a boot smoke is a bonus, not a requirement — do not
  burn the budget on it.
- LuaJIT 16KB page note: luajit-src 210.5.12 predates some 16KB fixes; if
  the build/assessment shows a problem, document precisely (do not patch
  vendored C silently — report instead).

## Report

Acceptance report: sub-goal-by-sub-goal status, commands, commit hashes,
gate tail, APK contents listing, GLES/shader assessment findings, deviations
from D-026, and the explicit MANUAL-check list for a device host.
