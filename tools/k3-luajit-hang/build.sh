#!/usr/bin/env bash
# issue 029k1 (K3) — build the babysteps hot-loop driver against a chosen
# LuaJIT build.
#
# Usage: ./build.sh <arch> <variant> [qemu]
#   arch:    aarch64 | x86_64
#   variant: vendored — the port's LuaJIT: upstream LuaJIT @ a4f56a4
#                    (2025-01-13, "v2.1 + master", the tree
#                    luajit-src 210.5.12+a4f56a4 bundles; FFI compiled in,
#                    exactly what mlua's `vendored` feature builds)
#            cpp      — the C++'s LuaJIT: vittorioromeo/LuaJIT @ 4dc5ffbf
#                    (2023-04-02, CMakeLists.txt pin; verified to be plain
#                    upstream v2.1 with CMake support — see the task
#                    report), built with -DLUAJIT_DISABLE_FFI=1 exactly
#                    like the CMakeLists.txt CPM pin
#   qemu:    optional marker — builds a SEPARATE source copy with
#            `lj_mcode_sync` no-op'ed (the `dc cvau`/`ic ivau`
#            cache-maintenance instructions are not emulated by
#            qemu-user; under QEMU the generated JIT code is translated
#            on the fly, so the flush is a no-op there by construction).
#            NEVER use a qemu-mode binary on a real device.
#
# Requires: git; for aarch64: the NDK from tools/android-env.sh
# (aarch64-linux-android28-clang). Output: $K3_BUILD/<arch>-<variant>[-qemu]/driver
# (plus the built libluajit.a; the LuaJIT source tree lives in
# $K3_BUILD/src-<variant>[-qemu]).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"

ARCH="${1:?usage: build.sh <arch> <variant> [qemu]}"
VARIANT="${2:?usage: build.sh <arch> <variant> [qemu]}"
QEMU_MODE="${3:-}"

BUILD="${K3_BUILD:-$HERE/build}"
SUFFIX="${QEMU_MODE:+-qemu}"
DEST="$BUILD/$ARCH-$VARIANT$SUFFIX"
SRC="$BUILD/src-$VARIANT$SUFFIX"
mkdir -p "$DEST"

case "$VARIANT" in
  vendored) REPO_URL="https://github.com/LuaJIT/LuaJIT"; PIN="a4f56a459a588ae768801074b46ba0adcfb49eb1" ;;
  cpp)      REPO_URL="https://github.com/vittorioromeo/LuaJIT"; PIN="4dc5ffbf60b16e54274c470fcc022165685c96fe" ;;
  *) echo "unknown variant $VARIANT" >&2; exit 2 ;;
esac

if [ ! -d "$SRC/.git" ]; then
  echo ">> cloning $VARIANT LuaJIT ($PIN)"
  git clone --quiet "$REPO_URL" "$SRC"
fi
if [ "$(git -C "$SRC" rev-parse HEAD)" != "$PIN" ]; then
  echo ">> checking out $PIN"
  git -C "$SRC" checkout --quiet "$PIN"
fi
echo ">> LuaJIT at: $(git -C "$SRC" rev-parse --short HEAD) ($(git -C "$SRC" log -1 --format=%ad --date=short))"

if [ "$QEMU_MODE" = "qemu" ]; then
  if ! grep -q "qemu K3: no cache flush" "$SRC/src/lj_mcode.c"; then
    echo ">> qemu mode: no-op'ing lj_mcode_sync (cache flush unneeded under qemu-user)"
    python3 - "$SRC/src/lj_mcode.c" <<'EOF'
import sys
p = sys.argv[1]
s = open(p).read()
needle = "  __clear_cache(start, end);\n"
assert needle in s, "lj_mcode_sync __clear_cache not found"
s = s.replace(needle, "  (void)start; (void)end; /* qemu K3: no cache flush */\n")
open(p, "w").write(s)
EOF
  fi
fi

# --- build libluajit.a (Makefile, the way luajit-src builds it) ---------
case "$ARCH" in
  aarch64)
    . "$REPO/tools/android-env.sh"
    CC_FLAG="$CC_aarch64_linux_android"
    LD_FLAG="$CC_aarch64_linux_android"
    NDK_BIN="$(dirname "$CC_FLAG")"
    ;;
  x86_64)
    CC_FLAG="cc"
    LD_FLAG="cc"
    NDK_BIN=""
    ;;
  *) echo "unknown arch $ARCH" >&2; exit 2 ;;
esac

EXTRA_CFLAGS=""
if [ "$VARIANT" = "cpp" ]; then
  # The CMakeLists.txt CPM pin builds with LUAJIT_DISABLE_FFI=ON.
  EXTRA_CFLAGS=" -DLUAJIT_DISABLE_FFI=1"
fi

if [ "$ARCH" = "aarch64" ]; then
  # The Makefile's default ar/strip are the host's; with `make -e` the
  # environment wins (the same mechanism luajit-src uses).
  export TARGET_AR="$NDK_BIN/llvm-ar rcus"
  export TARGET_STRIP="$NDK_BIN/llvm-strip"
fi

echo ">> make libluajit.a"
make -C "$SRC/src" -j"$(nproc)" clean >/dev/null 2>&1 || true
# Build the `libluajit.a` target directly (not `all`): the static `luajit`
# binary target needs a working TARGET_STRIP, and we only need the lib.
make -C "$SRC/src" -e -j"$(nproc)" libluajit.a BUILDMODE=static TARGET_SYS=Linux \
  XCFLAGS="-fPIC${EXTRA_CFLAGS}" \
  STATIC_CC="$CC_FLAG" TARGET_LD="$LD_FLAG" \
  HOST_CC="${HOST_CC:-cc}" >/dev/null
[ -f "$SRC/src/libluajit.a" ] || { echo "libluajit.a missing" >&2; exit 1; }

# --- link the driver ------------------------------------------------------
echo ">> linking driver"
if [ "$ARCH" = "aarch64" ]; then
  # NDK bionic: fully static; -lm/-ldl after the archive (the FFI C code
  # references dlopen; the driver never loads C modules, so runtime dlopen
  # is not exercised). -fno-emulated-tls + the 64-aligned TLS probe in
  # driver.c make Bionic's executable-TLS alignment check pass (see
  # driver.c comment / tls64.py).
  $CC_aarch64_linux_android -O2 -static -fno-emulated-tls -ftls-model=local-exec \
    "$HERE/driver.c" -I "$SRC/src" \
    "$SRC/src/libluajit.a" -lm -ldl -o "$DEST/driver"
else
  # Host glibc: dynamic (static glibc is broken for many libm symbols);
  # -lm must come after the LuaJIT archive that references the symbols.
  cc -O2 "$HERE/driver.c" -I "$SRC/src" "$SRC/src/libluajit.a" -lm \
    -o "$DEST/driver"
fi

if [ "$ARCH" = "aarch64" ]; then
  # Bionic (NDK 27) aborts static executables whose executable TLS segment
  # alignment is below 64 (its own __libc_tls_generation_copy is
  # 8-aligned). The patch only raises the ELF alignment metadata — it does
  # not change any code. See README.
  python3 "$HERE/tls64.py" "$DEST/driver"
fi

echo ">> done: $DEST/driver"
if [ "$ARCH" = "aarch64" ]; then
  echo "   run: QEMU_AARCH64=<qemu-aarch64> $DEST/driver <babysteps.lua> [--jit-off]"
fi
