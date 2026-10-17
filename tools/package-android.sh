#!/usr/bin/env bash
# Issue 25b SG6 — the Android APK packaging (release, aarch64-v8a only).
#
# Builds the installable, bootable APK through cargo-apk (0.10):
#   cargo apk build --release
# reads the [package.metadata.android] in crates/oh-android/Cargo.toml
# (the package id, the 28..34 SDK range, the fullscreen landscape
# NativeActivity, the _RELEASE content as APK assets) and produces
# `target/release/apk/open-hexagon.apk` (cargo-apk 0.10's layout)
# containing:
#   lib/arm64-v8a/liboh_android.so   the android_main entry (D-026)
#   assets/Packs/..., assets/config.json, ...   the _RELEASE tree
#                                               (the SG4 extraction source)
#
# Environment (the issue-25 setup, do not reinstall):
#   * tools/android-env.sh — the NDK r27c toolchain on PATH (the
#     aarch64-linux-android28-clang linker, .cargo/config.toml) + the
#     CC/AR env the C builds in the graph (luau0-src, libsodium-sys)
#     need.
#   * cargo-apk 0.10 on PATH (`cargo install cargo-apk` — checked below).
#   * the Android SDK build-tools 34.0.0 (aapt / zipalign / apksigner —
#     cargo-apk 0.10's packaging trio; discovered from $ANDROID_HOME).
#
# Signing: cargo-apk refuses an UNSIGNED release build (the `dev` profile
# gets the auto debug keystore, `release` must name one). This script
# owns a project SELF-DISTRIBUTION keystore at
# tools/android/openhexagon-release.keystore (generated with keytool on
# first use, fixed password below — NOT a store-distribution key: a real
# store release must bring its own keystore + CARGO_APK_RELEASE_KEYSTORE).
#
# Verification is BUILD-LEVEL (headless host — the 25b spec: no device,
# the on-device install/boot/touch run is the documented MANUAL check):
# the APK assembles, `aapt dump badging` is sane, `unzip -l` shows the
# .so + the assets.

set -euo pipefail
cd "$(dirname "$0")/.."

# The NDK toolchain (PATH + the CC/AR env for the C builds).
. tools/android-env.sh

if ! command -v cargo-apk >/dev/null 2>&1; then
    echo "cargo-apk not found on PATH — install it with: cargo install cargo-apk" >&2
    exit 1
fi

# The SDK build-tools cargo-apk 0.10 packages with (aapt/zipalign/
# apksigner — the installed 34.0.0; cargo-apk auto-discovers the newest
# build-tools dir under $ANDROID_HOME, this check just fails early with
# a clear message when the SDK layout is wrong).
if [ ! -d "${ANDROID_HOME}/build-tools/34.0.0" ]; then
    echo "Android build-tools 34.0.0 not found under \${ANDROID_HOME} (${ANDROID_HOME})" >&2
    echo "— cargo-apk needs the aapt/zipalign/apksigner trio (issue-25 setup)." >&2
    exit 1
fi

# ----------------------------------------------------------------------
# The self-distribution release keystore (see the header). keytool ships
# with the JDK the SDK tooling requires (Java 25 here).
KEYSTORE="tools/android/openhexagon-release.keystore"
KEYSTORE_PASS="openhexagon"
if [ ! -f "$KEYSTORE" ]; then
    echo "Generating the self-distribution release keystore: $KEYSTORE"
    mkdir -p tools/android
    keytool -genkeypair -v \
        -keystore "$KEYSTORE" \
        -alias openhexagon \
        -keyalg RSA -keysize 2048 -validity 10000 \
        -storepass "$KEYSTORE_PASS" -keypass "$KEYSTORE_PASS" \
        -dname "CN=Open Hexagon (self-distribution), OU=Rust port, O=SSVOpenHexagon, C=IT" >/dev/null
fi
# ABSOLUTE path: cargo-apk runs apksigner with cwd = target/<profile>/apk/
# (a relative keystore path would 404 against that cwd).
export CARGO_APK_RELEASE_KEYSTORE="$(pwd)/$KEYSTORE"
export CARGO_APK_RELEASE_KEYSTORE_PASSWORD="$KEYSTORE_PASS"

# ----------------------------------------------------------------------
# The staging assets dir (the on-device smoke fix): cargo-apk packs the
# folder named by the metadata `assets` key; we stage a COPY of _RELEASE
# (never mutating the tracked content) + the generated `.filelist` (one
# assets-relative path per line) the extractor reads instead of the
# unreliable AAssetManager_openDir("") root enumeration (module docs in
# oh-app/src/android.rs::extract_content — the emulator caught openDir
# returning an EMPTY root listing, a silent 0-file extraction).
STAGE="target/oh-apk-assets"
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp -a _RELEASE/. "$STAGE/"
( cd "$STAGE" && find . -type f ! -name oh.filelist -printf '%P\n' | LC_ALL=C sort > oh.filelist )
echo "staged $(wc -l < "$STAGE/oh.filelist") asset files + oh.filelist into $STAGE"

# ----------------------------------------------------------------------
# The 16KB-page-size alignment (Android 15's 16KB OEM devices, the 25b
# note): the NDK r27c lld DEFAULT is 4KB ELF LOAD alignment (verified
# empirically: llvm-readelf -l on an NDK-linked .so shows align 0x1000
# on the API-28 AND the API-35 clang targets). -z max-page-size=16384
# makes the .so 16KB-aligned (0x4000 — a superset: it still loads on
# 4KB-page devices; the APK is thereby 16KB-page-ready). It is ALSO in
# .cargo/config.toml (the target-scoped rustflags, for direct
# `cargo ... --target aarch64-linux-android` builds) — but cargo-apk's
# `cargo_ndk` injects its OWN CARGO_ENCODED_RUSTFLAGS (the clang target
# flag + the libgcc workaround) which OVERRIDES the config's, so under
# cargo-apk the flag must ride on RUSTFLAGS (cargo_ndk reads + merges
# that one; -C link-arg hits only final links).
export RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"

# ----------------------------------------------------------------------
# The release APK. cargo-apk parses the CURRENT manifest for the
# [package.metadata.android] table, so point it at the oh-android
# package (the workspace root is a virtual manifest — cargo-apk 0.10
# rejects it); the build stays in this workspace (the target dir, the
# workspace manifest, the _RELEASE asset path all resolve from the
# crate's manifest).
# OH_APK_TARGET (optional): override the ABI (metadata pins
# aarch64-linux-android) — e.g. OH_APK_TARGET=x86_64-linux-android for
# the emulator smoke (the x86_64 CC/CXX/AR env exported separately).
TARGET_ARGS=()
if [ -n "${OH_APK_TARGET:-}" ]; then
    TARGET_ARGS=(--target "$OH_APK_TARGET")
    echo "ABI override: $OH_APK_TARGET"
fi
cargo apk build --release --manifest-path crates/oh-android/Cargo.toml "${TARGET_ARGS[@]}"

# ----------------------------------------------------------------------
# The build-level verification (the 25b spec's acceptance surface).
# cargo-apk 0.10's output: target/<profile>/apk/<apk_name>.apk (the
# find below is layout-tolerant anyway).
APK="$(find target -path "*/release/open-hexagon.apk" -o -path "*/apk/open-hexagon.apk" -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2-)"
if [ -z "${APK:-}" ] || [ ! -f "$APK" ]; then
    echo "open-hexagon.apk not found under target/ (the cargo-apk build output)" >&2
    exit 1
fi
echo
echo "== APK: $APK ($(du -h "$APK" | cut -f1)) =="

echo
echo "== aapt dump badging =="
"${ANDROID_HOME}/build-tools/34.0.0/aapt" dump badging "$APK" | grep -E "^(package|application-label:|launchable-activity|sdkVersion|targetSdkVersion|native-code|uses-feature)" || true

echo
echo "== APK contents (the 25b spec surface: lib/arm64-v8a + assets) =="
unzip -l "$APK" | grep -E "arm64-v8a|assets/Packs/|^.*assets/config.json|AndroidManifest" | head -20
unzip -l "$APK" | tail -3

# The asset tree spot-check: the Packs + config.json must be IN the APK
# (the SG4 extraction's source) — count the asset entries.
ASSET_COUNT="$(unzip -l "$APK" | grep -c "assets/" || true)"
echo
echo "asset entries in the APK: $ASSET_COUNT"
if [ "$ASSET_COUNT" -lt 100 ]; then
    echo "UNEXPECTED: fewer than 100 asset entries (the _RELEASE tree is ~1000+ files)" >&2
    exit 1
fi

# The 16KB-page alignment check (the 25b note): the .so's ELF LOAD
# segments must be 0x4000-aligned (the RUSTFLAGS above; the NDK default
# is 0x1000/4KB, which is NOT 16KB-page-ready).
NDK_BIN="${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin"
rm -rf /tmp/cargo-apk-so-check
unzip -o -j "$APK" "lib/arm64-v8a/liboh_android.so" -d /tmp/cargo-apk-so-check >/dev/null
ALIGN="$(${NDK_BIN}/llvm-readelf -l /tmp/cargo-apk-so-check/liboh_android.so | grep LOAD | awk '{print $NF}' | sort -u)"
echo "ELF LOAD alignment: $ALIGN"
if [ "$ALIGN" != "0x4000" ]; then
    echo "UNEXPECTED: the .so is not 16KB-aligned (expected 0x4000, got $ALIGN)" >&2
    exit 1
fi
rm -rf /tmp/cargo-apk-so-check

# The android_main entry symbol must be exported (the NativeActivity's
# dlsym target).
unzip -p "$APK" "lib/arm64-v8a/liboh_android.so" > /tmp/cargo-apk-so-bin
if ! ${NDK_BIN}/llvm-nm -D /tmp/cargo-apk-so-bin | grep -q " T android_main"; then
    echo "UNEXPECTED: the android_main symbol is not exported from liboh_android.so" >&2
    rm -f /tmp/cargo-apk-so-bin
    exit 1
fi
rm -f /tmp/cargo-apk-so-bin
echo "android_main symbol: exported"

# The signature check (the self-distribution key — the spec's
# "installable": a signed APK).
"${ANDROID_HOME}/build-tools/34.0.0/apksigner" verify "$APK" >/dev/null 2>&1 || {
    echo "UNEXPECTED: apksigner verify failed (the APK is not signed)" >&2
    exit 1
}
echo "signature: verified"

echo
echo "APK OK — $APK"
echo "MANUAL checks (device host, the 25b spec's on-device list):"
echo "  1. adb install -r $APK   (the first install: the SG4 extraction"
echo "     runs — the [::mainClient] extracted N content files line)"
echo "  2. launch — the menu renders (the GLES 3.0 context + the"
echo "     translated internal shaders, SG2/SG3)"
echo "  3. start a game — touch left/right screen half = CCW/CW"
echo "     (the SG5 touch seam), a tap's behavior matches the C++"
echo "  4. quit + relaunch — the content marker skips the re-extraction"
echo "  5. rotate the device — the sensorLandscape activity keeps"
echo "     landscape without a game-state loss (the config_changes)"
