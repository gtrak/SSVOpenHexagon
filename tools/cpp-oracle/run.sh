#!/usr/bin/env bash
# Build (and run) the C++ reference binary in a container — task 030b's "C++ column".
#
#   tools/cpp-oracle/run.sh image             # build the dependency image (docker)
#   tools/cpp-oracle/run.sh seed              # fetch every CPM pin by SHA into $BUILD/*-src
#   tools/cpp-oracle/run.sh configure         # cmake configure  (VERIFIED GREEN 2026-10-17)
#   tools/cpp-oracle/run.sh build             # configure + build (LONG: SFML+LuaJIT+the game)
#   tools/cpp-oracle/run.sh shell             # interactive shell in the image
#   tools/cpp-oracle/run.sh run -- <args>     # run the built binary from _RELEASE
#
# WHY THIS EXISTS (learned the hard way):
#  * The C++ fetches SFML/luajit/zlib/imgui/imgui-sfml/libsodium/boostpfr via CPM,
#    but SFML still needs SYSTEM X11/GL headers, which cannot be vendored. This
#    host has no passwordless sudo and is missing most of them -> a container.
#  * TWO PINS ARE UNREACHABLE BY CLONE: the pinned commits of
#    `vittorioromeo/SFML@dacceddb` and `vittorioromeo/imgui-sfml@f35f3533` are no
#    longer on any branch (the fork endpoints 301), so CPM's clone+checkout fails
#    with "Failed to checkout tag". They ARE still fetchable by SHA
#    (`git fetch origin <sha>`), so `seed` does exactly that and the build points
#    CMake at the local trees with FETCHCONTENT_SOURCE_DIR_<NAME>.
#  * `libsodium-cmake` carries `libsodium` as a SUBMODULE -> `git submodule update
#    --init --recursive`, or configure dies on a missing `version.h.in`.
#
# The repo is mounted READ-ONLY at /src; everything build-related lives OUTSIDE the
# checkout (${REPO%/}/../oh-cpp-build) so the reference tree stays pristine.
#
# ALWAYS wrap long steps in `timeout` (lane hygiene: never run an unbounded build).
set -euo pipefail

REPO_DEFAULT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REPO="${REPO:-$REPO_DEFAULT}"
BUILD="${BUILD:-${REPO%/}/../oh-cpp-build}"
IMAGE="oh-cpp-oracle:1"
CMD="${1:-}"; shift || true

# name|repo|sha  (the pins are in CMakeLists.txt; keep this list in sync)
PINS=(
  "sfml|vittorioromeo/SFML|dacceddbd1adee49c7bfd120266e200800550394"
  "zlib|madler/zlib|cacf7f1d4e3d44d871b605da3b647f07d718623f"
  "imgui|ocornut/imgui|04316bd223f98e4c40bd82dc5f2a35c4891e4e5f"
  "imgui-sfml|vittorioromeo/imgui-sfml|f35f353376281c0a891a4d614a241914ddfcbe29"
  "libsodium-cmake|vittorioromeo/libsodium-cmake|fd76500b60fcaa341b6fad24cda88c3504df770d"
  "boostpfr|boostorg/pfr|b0bf18798c7037ca8a91a1cd2ad2e5798d8f6d46"
)

# FETCHCONTENT_SOURCE_DIR_* for every seeded tree
fc_flags() {
  local f=""
  for p in "${PINS[@]}"; do
    IFS='|' read -r name _ _ <<<"$p"
    f+=" -DFETCHCONTENT_SOURCE_DIR_$(echo "$name" | tr '[:lower:]' '[:upper:]')=/build/$name-src"
  done
  echo "$f"
}

case "$CMD" in
  image)
    docker build -t "$IMAGE" "$REPO/tools/cpp-oracle"
    ;;

  seed)
    mkdir -p "$BUILD"
    for p in "${PINS[@]}"; do
      IFS='|' read -r name repo sha <<<"$p"
      d="$BUILD/$name-src"
      [ -d "$d/.git" ] || timeout 900 git clone -q "https://github.com/$repo" "$d"
      ( cd "$d" && timeout 300 git fetch -q origin "$sha" && git checkout -q FETCH_HEAD ) \
        && echo "$name -> $(git -C "$d" log --oneline -1 | cut -c1-58)" \
        || { echo "$name -> FETCH FAILED ($sha)" >&2; exit 1; }
      # deps that carry their own submodules (libsodium-cmake does)
      if [ -f "$d/.gitmodules" ]; then
        ( cd "$d" && timeout 900 git submodule update --init --recursive -q ) \
          && echo "$name: submodules ok"
      fi
    done
    ;;

  configure|build|shell|run)
    mkdir -p "$BUILD"
    DOCKER_ARGS=(--rm -v "$REPO":/src:ro -v "$BUILD":/build)
    case "$CMD" in
      configure|build)
        # shellcheck disable=SC2046
        docker run "${DOCKER_ARGS[@]}" "$IMAGE" sh -lc \
          "cmake -S /src -B /build -G Ninja -DCMAKE_BUILD_TYPE=Release $(fc_flags)$([ "$CMD" = build ] && echo ' && cmake --build /build -j\"$(nproc)\"')"
        ;;
      shell)
        docker run -it "${DOCKER_ARGS[@]}" "$IMAGE" bash
        ;;
      run)
        [ "${1:-}" = "--" ] && shift || true
        docker run "${DOCKER_ARGS[@]}" -w /src/_RELEASE "$IMAGE" sh -lc \
          'bin=$(find /build -maxdepth 3 -type f -name "OpenHexagon*" -perm -u+x | head -1); exec "$bin" "$@"' sh "$@"
        ;;
    esac
    ;;

  *)
    sed -n '2,18p' "${BASH_SOURCE[0]}"
    exit 1
    ;;
esac
