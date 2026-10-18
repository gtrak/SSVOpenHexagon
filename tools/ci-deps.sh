#!/usr/bin/env bash
# Issue 045 — the CI apt prerequisites, the single source of truth.
#
#   bash tools/ci-deps.sh
#
# Both workflows (.github/workflows/ci.yml + .github/workflows/release.yml)
# used to inline their apt prerequisites as per-job steps. The duplication
# is how the 044 ALSA step (libasound2-dev — the rodio -> cpal -> alsa-sys
# build script needs `alsa.pc`) landed in ci.yml's gate job but NOT in
# release.yml's: the release matrix's gate job died of the exact
# build-script failure 044 had just fixed in CI (run 39097932008), and the
# build matrix below it (linux / windows / android / wasm) never ran. This
# script is now the ONE place the ubuntu-runner apt surface lives: a new
# prerequisite is added HERE and reaches every job that calls it.
#
# The set (each guarded, so a runner that already has it pays only the
# check):
#   * libudev-dev    — the desktop controller backend (gilrs ->
#                      libudev-sys, a Linux-only dep of gilrs-core); the
#                      host builds (the gate jobs + the release linux job)
#                      need the `libudev` pc file. Preinstalled on the
#                      ubuntu-latest image (defensive).
#   * libasound2-dev — the audio backend (rodio -> cpal -> alsa-sys, Linux
#                      only) links the system ALSA library; alsa-sys's
#                      build script needs `alsa.pc`. NOT on the ubuntu-
#                      latest image (044: the gate's `cargo build
#                      --workspace` failed exactly here, run
#                      39088375527: "Package 'alsa' was not found in the
#                      pkg-config search path").
#   * mingw-w64      — the x86_64-pc-windows-gnu cross toolchain (the
#                      x86_64-w64-mingw32-gcc tools/build-windows.sh needs
#                      for the Windows release build + the cross-check).
#                      Preinstalled on the ubuntu-latest image (defensive).
#
# Non-Linux runner: no-op (exit 0), so the script is safe to call from a
# matrix job unconditionally.
#
# Exit: 0 when every prerequisite is present (already or installed now).

set -euo pipefail

if [ "$(uname -s)" != "Linux" ]; then
    echo "ci-deps: not a Linux runner — nothing to install"
    exit 0
fi

MISSING=()
pkg-config --exists libudev || MISSING+=("libudev-dev")
pkg-config --exists alsa || MISSING+=("libasound2-dev")
command -v x86_64-w64-mingw32-gcc >/dev/null || MISSING+=("mingw-w64")

if [ "${#MISSING[@]}" -eq 0 ]; then
    echo "ci-deps: all prerequisites present (libudev, alsa, mingw-w64)"
    exit 0
fi

echo "== apt-get install -y: ${MISSING[*]}"
sudo apt-get install -y "${MISSING[@]}"
echo "ci-deps: prerequisites installed"
