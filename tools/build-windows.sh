#!/usr/bin/env bash
# Issue 22 — the x86_64-pc-windows-gnu cross-build helper (Linux/macOS host).
#
#   bash tools/build-windows.sh            # cargo check --workspace --target x86_64-pc-windows-gnu
#   bash tools/build-windows.sh --all-targets   # also check the test/bench targets
#   bash tools/build-windows.sh --build    # FULL cross-build (compiles + links the .exe's)
#   bash tools/build-windows.sh --build --release  # the RELEASE profile (issue 24 packaging)
#   bash tools/build-windows.sh --force-sodium  # re-run the libsodium prebuild even if cached
#
# Prereqs (both on the GitHub ubuntu-latest runner and any dev box with
# mingw-w64 + make installed):
#   * the rustup target:     rustup target add x86_64-pc-windows-gnu
#   * the MinGW cross gcc:   x86_64-w64-mingw32-gcc (Debian/Ubuntu: apt install mingw-w64)
#   * make (the host make drives the libsodium prebuild + the LuaJIT build)
#
# Why the libsodium prebuild: libsodium-sys 0.2.7 (the D-004 pin, sodiumoxide
# 0.2.7) hardcodes '--host=<the rust target triple>' into the vendored
# libsodium 1.0.18 autoconf configure, whose config.sub rejects
# 'x86_64-pc-windows-gnu' ("Kernel `windows' not known to work with OS
# `gnu'"). The canonical MinGW triple 'x86_64-w64-mingw32' IS accepted by the
# same configure, so we prebuild the static libsodium for it (the official
# sodiumoxide cross-compiling path — point SODIUM_LIB_DIR at the prebuilt lib
# and libsodium-sys links instead of building from source). The result is
# cached under target/ (gitignored; 'cargo clean' / --force-sodium rebuilds).
#
# The other C deps cross-compile in place: Luau (mlua 0.10.5 `luau` feature
# via luau0-src — D-037: the port's single Lua VM, statically built from
# C++ source through the same mingw cross toolchain) plus rusqlite
# (bundled) + flate2 (system-zlib host build) go through cc as usual.
# NOTE: the MSVC target (x86_64-pc-windows-msvc) is NOT cross-buildable from
# Linux (the C++/C deps' msvc path needs the native MSVC toolchain) — it is
# covered by the windows-latest CI job instead.
#
# Linking: 'cargo check' never links, so the check path needs nothing beyond
# the above. '--build' links with the target's default linker
# (x86_64-w64-mingw32-gcc — no linker override needed as long as mingw-w64
# is installed); .cargo/config.toml adds -ladvapi32 for the cross-built
# static libsodium (SystemFunction036). The default build never links the
# Steam/Discord SDKs (oh-steam's 'steam' feature is default-OFF; there is no
# oh-discord crate in the workspace yet — issue 21).

set -u
cd "$(dirname "$0")/.."

TARGET="x86_64-pc-windows-gnu"
MODE="check"
EXTRA_TARGETS=()
FORCE_SODIUM=0
RELEASE=0

for arg in "$@"; do
    case "$arg" in
    --check) MODE="check" ;;
    --build) MODE="build" ;;
    --all-targets) EXTRA_TARGETS=(--all-targets) ;;
    --release) RELEASE=1 ;;
    --force-sodium) FORCE_SODIUM=1 ;;
    -h | --help)
        grep '^#' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *)
        echo "unknown flag: $arg (see --help)" >&2
        exit 2
        ;;
    esac
done

CROSS_GCC="x86_64-w64-mingw32-gcc"
command -v "$CROSS_GCC" >/dev/null || {
    echo "error: $CROSS_GCC not found (Debian/Ubuntu: sudo apt install mingw-w64)" >&2
    exit 1
}
command -v make >/dev/null || {
    echo "error: make not found (needed by the libsodium prebuild + the LuaJIT build)" >&2
    exit 1
}

# ---------------------------------------------------------------------
# rustup target
# ---------------------------------------------------------------------
if ! rustup target list --installed | grep -qx "$TARGET"; then
    echo "== installing rustup target $TARGET"
    rustup target add "$TARGET"
fi

# ---------------------------------------------------------------------
# The libsodium prebuild (cached under target/)
# ---------------------------------------------------------------------
SODIUM_DIR="$(pwd)/target/${TARGET}/cross-libsodium"
SODIUM_PREFIX="${SODIUM_DIR}/prefix"
SODIUM_LIB="${SODIUM_PREFIX}/lib/libsodium.a"

need_sodium=1
if [ -f "$SODIUM_LIB" ]; then
    need_sodium=0
fi
if [ "$FORCE_SODIUM" -eq 1 ]; then
    rm -rf "$SODIUM_DIR"
    need_sodium=1
fi

if [ "$need_sodium" -eq 1 ]; then
    # The libsodium 1.0.18 source ships inside the pinned libsodium-sys
    # crate (cargo fetches it as a build dependency of oh-net's sodiumoxide).
    crate_src=$(
        ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/libsodium-sys-0.2.7 2>/dev/null \
            | head -n1
    )
    if [ -z "${crate_src:-}" ]; then
        # Fetch it (the registry cache layout needs the crate downloaded once).
        cargo fetch
        crate_src=$(
            ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/libsodium-sys-0.2.7 2>/dev/null \
                | head -n1
        )
    fi
    if [ -z "${crate_src:-}" ] || [ ! -d "${crate_src}/libsodium" ]; then
        echo "error: cannot find the vendored libsodium source (libsodium-sys-0.2.7/libsodium)" >&2
        exit 1
    fi

    echo "== prebuilding libsodium 1.0.18 for x86_64-w64-mingw32 (-> ${SODIUM_LIB})"
    mkdir -p "$SODIUM_DIR"
    rm -rf "${SODIUM_DIR}/src"
    cp -r "${crate_src}/libsodium" "${SODIUM_DIR}/src"
    (
        cd "${SODIUM_DIR}/src"
        # The canonical MinGW triple — the one libsodium 1.0.18's config.sub
        # accepts (it rejects the rust triple x86_64-pc-windows-gnu).
        ./configure \
            --host=x86_64-w64-mingw32 \
            --prefix="$SODIUM_PREFIX" \
            --libdir="$SODIUM_PREFIX/lib" \
            --enable-shared=no \
            CC="$CROSS_GCC"
        # Cross build: 'all', not 'check' (the C tests cannot run on the host).
        make -j"$(nproc)" all
        make install
    )
fi
[ -f "$SODIUM_LIB" ] || { echo "error: libsodium prebuild did not produce $SODIUM_LIB" >&2; exit 1; }
echo "SODIUM_LIB_DIR=$SODIUM_PREFIX/lib (cached: ${SODIUM_LIB})"
export SODIUM_LIB_DIR="$SODIUM_PREFIX/lib"

# ---------------------------------------------------------------------
# The cargo step
# ---------------------------------------------------------------------
case "$MODE" in
check)
    echo "== cargo check --workspace ${EXTRA_TARGETS[*]:-} --target $TARGET"
    exec cargo check --workspace "${EXTRA_TARGETS[@]+"${EXTRA_TARGETS[@]}"}" --target "$TARGET"
    ;;
build)
    if [ "$RELEASE" -eq 1 ]; then
        echo "== cargo build --workspace --release --target $TARGET (full cross-compile + link, release profile)"
        exec cargo build --workspace --release --target "$TARGET"
    else
        echo "== cargo build --workspace --target $TARGET (full cross-compile + link)"
        exec cargo build --workspace --target "$TARGET"
    fi
    ;;
esac
