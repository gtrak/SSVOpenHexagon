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
#   * xvfb           — `xvfb-run`, the headless X server the gate's
#                      windowed smoke (tools/smoke-windowed.sh) runs the
#                      desktop build under when no DISPLAY is present.
#                      NOT on the ubuntu-latest image.
#   * libgl1-mesa-dri — the Mesa software GL driver (llvmpipe) Xvfb needs
#                      to actually create a GL context for the windowed
#                      smoke's `[oh-render] context GL_SAMPLES`
#                      assertion. NOT on the ubuntu-latest image.
#   * libegl1 + libegl-mesa0 — the EGL loader + vendor library. The
#                      desktop GL path is EGL (glutin's
#                      DisplayApiPreference::Egl, crates/oh-render/src/gl.rs),
#                      and libgl1-mesa-dri alone provides no libEGL at all:
#                      without it, window creation fails,
#                      GlBackend::try_create returns None, and the windowed
#                      smoke's `[oh-render] context GL_SAMPLES` assertion
#                      can't be reached. NOT on the ubuntu-latest image.
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
command -v xvfb-run >/dev/null || MISSING+=("xvfb")
dpkg -s libgl1-mesa-dri >/dev/null 2>&1 || MISSING+=("libgl1-mesa-dri")
dpkg -s libegl1 >/dev/null 2>&1 || MISSING+=("libegl1")
dpkg -s libegl-mesa0 >/dev/null 2>&1 || MISSING+=("libegl-mesa0")

if [ "${#MISSING[@]}" -eq 0 ]; then
    echo "ci-deps: all prerequisites present (libudev, alsa, mingw-w64, xvfb, mesa-dri, libegl1, libegl-mesa0)"
    exit 0
fi

echo "== apt-get install -y: ${MISSING[*]}"
sudo apt-get install -y "${MISSING[@]}"
echo "ci-deps: prerequisites installed"
