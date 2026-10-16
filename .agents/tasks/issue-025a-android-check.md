# Task 025a — Android NDK: workspace `cargo check` green for `aarch64-linux-android`

You are implementing **lane 25a** of issue 25 (Android NDK port) of plan 001
(`.agents/plans/001-rust-port/25-android.md`). This is a **literal 1:1 port**
project: mirror C++ structure, `f32`/`f64`, no expression reordering.

## Environment (already set up — do NOT reinstall)

- SDK `~/Android/Sdk`, NDK `~/Android/Sdk/ndk/27.2.12479018` (r27c), API 28.
- `.cargo/config.toml` already has `[target.aarch64-linux-android]`
  `linker = "aarch64-linux-android28-clang"`.
- `tools/android-env.sh` (commit c653ab89) exports
  `CC/AR/CXX_aarch64_linux_android` for the cc-rs crates. **Source it in every
  shell that runs cargo for the android target** (build scripts need it:
  mlua-sys/luajit-src will fail without `CC_aarch64_linux_android`).
- `rustup target add aarch64-linux-android` is done.
- Already verified green: `cargo check -p oh-game` and `-p oh-lua` for the
  android target (LuaJIT 210.5.12 cross-builds fine).

## Goal

`bash -c '. tools/android-env.sh && cargo check --workspace --target
aarch64-linux-android'` exits 0, **without regressing the desktop build**
(`cargo test --workspace` green, `tools/gate.sh` 4/4).

## Known blockers (from orchestrator scouting)

1. **oh-render / android-activity feature**: winit 0.30 pulls
   `android-activity` 0.6.1, which errors
   `Either "game-activity" or "native-activity" must be enabled as features`.
   winit exposes the feature as `winit/android-native-activity` (decision
   D-026: native-activity). Enable it the least-invasive way: a
   target-appropriate feature passthrough in `crates/oh-render/Cargo.toml`
   (e.g. an `android` feature or enabling the winit feature unconditionally —
   measure which is cleaner; winit's android code is cfg'd out on non-android
   targets, so an unconditional enable should be harmless for desktop —
   **verify** with a desktop rebuild + gate).
2. **Remaining unix-only APIs**: scout `#[cfg(unix)]`/`#[cfg(target_os)]`
   blocks and std usage that differs on android (e.g. signal-hook, paths).
   Fix with cfg'd code paths, never by deleting behavior.
3. `oh-app`, `oh-server`, tools crates must all check. If a binary crate
   genuinely cannot compile for android (e.g. a desktop-only tool), scope it
   with `#[cfg(not(target_os = "android"))]` on the `[[bin]]`/
   crate-level gating and document it in the report — do not break the
   desktop build.

## Constraints

- **Commit after every green milestone** (e.g. "oh-render checks", "oh-app
  checks", "workspace green"). Never leave work uncommitted — a lost lane
  loses everything.
- Do not touch `_RELEASE/`, do not bump dependency versions (only feature
  enables / cfg fixes). Any `Cargo.lock` churn must be explained.
- The desktop gate is sacred: run `tools/gate.sh` before your final commit
  and paste its tail in the report.
- Tests stay host-only: do NOT try to run `cargo test --target
  aarch64-linux-android` (no device). Check-level only.

## Report

Acceptance report with: per-crate check results (the full workspace list),
the exact commands, commits, gate output tail, and any deviations from
D-026 (with reasons). Flag anything you had to scope out.
