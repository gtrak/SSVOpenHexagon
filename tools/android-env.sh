#!/usr/bin/env bash
# Issue 25 — Android NDK build environment (aarch64-linux-android, API 28).
# Source me: `. tools/android-env.sh`. Requires $ANDROID_HOME (or the default
# ~/Android/Sdk) with ndk/27.2.12479018 installed (issue 25 setup).
set -euo pipefail
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.2.12479018"
TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
export PATH="$TOOLCHAIN:$PATH"
API=28
TGT=aarch64-linux-android
export CC_${TGT//-/_}="$TOOLCHAIN/${TGT}${API}-clang"
export CXX_${TGT//-/_}="$TOOLCHAIN/${TGT}${API}-clang++"
export AR_${TGT//-/_}="$TOOLCHAIN/llvm-ar"
export RANLIB_${TGT//-/_}="$TOOLCHAIN/llvm-ranlib"
echo "android env ready: $TGT API $API"
