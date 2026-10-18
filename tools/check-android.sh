#!/usr/bin/env bash
# Issue 037 — the fast Android cross-check (the per-push signal; the slow,
# signing-dependent release APK lives in tools/package-android.sh and is
# run by the RELEASE workflow, not on every push).
#
#   bash tools/check-android.sh
#
# Runs `cargo check -p oh-app --target aarch64-linux-android` — the
# engine's android-gated paths (#[cfg(target_os = "android")] in oh-app
# and friends) compile-checked against the pinned NDK, WITHOUT a full
# build (no link, no APK, no signing). This is the "host gate is not a
# web gate" sibling for android: a target-gated regression is invisible
# to the host gate, so the per-push CI carries this check.
#
# Prereqs (same as tools/package-android.sh minus cargo-apk):
#   * tools/android-env.sh — the NDK r27c (27.2.12479018) toolchain on
#     PATH + the CC/AR env the C builds in the graph need.
#   * the rustup target aarch64-linux-android.
#
# Exit code: cargo check's (0 = the android-gated code compiles).

set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="aarch64-linux-android"

# The NDK toolchain (PATH + the CC/AR env for the C builds).
. tools/android-env.sh

if ! rustup target list --installed | grep -qx "$TARGET"; then
    echo "== installing rustup target $TARGET"
    rustup target add "$TARGET"
fi

echo "== cargo check -p oh-app --target $TARGET"
exec cargo check -p oh-app --target "$TARGET"
