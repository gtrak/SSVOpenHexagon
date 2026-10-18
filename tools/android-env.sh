#!/usr/bin/env bash
# Issue 25 — Android NDK build environment (aarch64-linux-android, API 28).
# Source me: `. tools/android-env.sh`. Requires $ANDROID_HOME (or the default
# ~/Android/Sdk) with ndk/27.2.12479018 installed (issue 25 setup).
set -euo pipefail
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.2.12479018"
TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
export PATH="$TOOLCHAIN:$PATH"

# The NDK pin must survive the runner-image pre-sets: the GitHub
# ubuntu-latest image writes ANDROID_SDK_ROOT, ANDROID_HOME, ANDROID_NDK,
# ANDROID_NDK_HOME AND ANDROID_NDK_ROOT to /etc/environment, all pointing
# at the image's own (newer) NDK (runner-images install-android-sdk.sh:
# "ANDROID_NDK, ANDROID_NDK_HOME, and ANDROID_NDK_ROOT variables should
# be set as many customer builds depend on them", actions/runner-
# images#5879). ndk-build — cargo-apk 0.10's NDK discovery (Ndk::
# from_env) — resolves ANDROID_NDK_ROOT BEFORE ANDROID_NDK_HOME, so on a
# runner the image NDK beats this file's pin (issue 045, run
# 39103410548: the release android job resolved 27.3.13750724 instead of
# the pinned 27.2.12479018). Unset the higher-priority names so the
# pinned ANDROID_NDK_HOME above is the one that wins. (No-op on a dev
# box, where none of the three is pre-set.)
unset ANDROID_NDK_ROOT ANDROID_NDK_PATH NDK_HOME
API=28
TGT="${ANDROID_ABI:-aarch64-linux-android}"
export CC_${TGT//-/_}="$TOOLCHAIN/${TGT}${API}-clang"
export CXX_${TGT//-/_}="$TOOLCHAIN/${TGT}${API}-clang++"
export AR_${TGT//-/_}="$TOOLCHAIN/llvm-ar"
export RANLIB_${TGT//-/_}="$TOOLCHAIN/llvm-ranlib"
echo "android env ready: $TGT API $API"
