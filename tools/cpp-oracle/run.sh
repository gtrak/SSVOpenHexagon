#!/usr/bin/env bash
# Build (and run) the C++ reference binary in a container — task 030b's "C++ column".
#
#   tools/cpp-oracle/run.sh image             # build the dependency image (docker)
#   tools/cpp-oracle/run.sh init              # git submodule update --init the 5 extlibs the build needs
#   tools/cpp-oracle/run.sh seed              # fetch every CPM pin by SHA into $BUILD/*-src
#   tools/cpp-oracle/run.sh instrument        # apply fingerprint.patch to the tree (the instrumented build)
#   tools/cpp-oracle/run.sh deinstrument      # reverse it (the reference tree back to pristine)
#   tools/cpp-oracle/run.sh configure         # cmake configure  (VERIFIED GREEN 2026-10-17; re-verified at the 038 cpp-original/ source path 2026-10-18)
#   tools/cpp-oracle/run.sh build             # configure + build (LONG: SFML+LuaJIT+the game)
#   tools/cpp-oracle/run.sh shell             # interactive shell in the image
#   tools/cpp-oracle/run.sh run -- <args>     # run the built binary from _RELEASE
#   tools/cpp-oracle/run.sh run [-e KEY=VAL ...] -- ARGS...   (run the built binary)
#
# 038 REORG: the C++ tree (src/, include/, extlibs/, CMakeLists.txt, ...) now
# lives in cpp-original/; the CMake source dir is /src/cpp-original. The repo
# is still mounted at /src, and _RELEASE/ still sits at the repo root (the
# moved CMakeLists.txt reaches it through the committed cpp-original/_RELEASE
# pointer — C++-side build glue, see DECISIONS D-038).
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
#  * WALL #5 (030b, 2026-10-17): a fresh LANE WORKTREE has the `extlibs/*` git
#    submodules UNINITIALIZED (empty dirs). The container mounts the worktree at
#    /src, so `include(vrm_cmake)` (CMakeLists.txt:256, module from
#    extlibs/vrm_cmake/cmake) and every `#include "SSVUtils/..."` / "SSVStart/..." /
#    "SSVMenuSystem/..." die on the empty tree. `init` checks the five pinned
#    submodules out (SSVUtils, SSVStart, SSVMenuSystem, vrm_cmake, vrm_pp —
#    `extlibs/SFML` is listed in .gitmodules but UNUSED: the build uses the CPM
#    vittorioromeo/SFML fork). Run it once per worktree; it only touches the
#    worktree (the submodule commit pointers are the repo's pinned state).
#  * WALL #6 (030b, 2026-10-17): the old `build` one-liner built its job count
#    as `\"$(nproc)\"` INSIDE the container's `sh -lc` string, so cmake saw a
#    literal-quoted `-j"24"` and printed its usage. The job count is now expanded
#    by the HOST shell before docker run.
#  * WALL #7 (030b, 2026-10-17): the pinned SFML fork (dacceddb) predates the
#    GCC 13+ libstdc++ change that stopped transitively including <cstdint>,
#    so its PCH dies on `std::uint32_t has not been declared` under the
#    container's g++ 15. The fix is ENVIRONMENTAL (no source change): force
#    `-include cstdint` through CMAKE_CXX_FLAGS in the cmake invocation below.
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

  init)
    # The extlibs submodules the C++ build actually needs (pinned SHAs are the
    # repo's state — `git submodule update --init` checks exactly those out).
    # 038: the submodules moved with the tree into cpp-original/ (the
    # .gitmodules paths follow; the orphan extlibs/SFML entry has no gitlink).
    ( cd "$REPO" && timeout 900 git submodule update --init \
        cpp-original/extlibs/SSVUtils cpp-original/extlibs/SSVStart \
        cpp-original/extlibs/SSVMenuSystem cpp-original/extlibs/vrm_cmake \
        cpp-original/extlibs/vrm_pp )
    ;;

  instrument)
    # The C++ column's instrumentation lives ONLY as
    # tools/cpp-oracle/fingerprint.patch: master's reference tree stays pristine
    # so that every D-014 `file:line` citation keeps addressing the same text.
    # The patch is exactly equivalent to the (development) in-tree edits that
    # produced the vectors — verified by reverse-applying it and diffing the
    # seven files against master (all byte-identical).
    # The container mounts $REPO READ-ONLY at /src, so the patch cannot be
    # applied from inside: do it here, on the host, before `build`.
    # 038: the tree moved into cpp-original/. `git apply` refuses to follow
    # the root `src`/`include` citation symlinks ("beyond a symbolic link"),
    # so the prefix is explicit here (--directory=cpp-original). The patch
    # itself is 030b's untouched artifact (written against the pre-reorg
    # src/... and include/... paths; its hunk content is unchanged).
    git -C "$REPO" apply --directory=cpp-original \
      "$REPO/tools/cpp-oracle/fingerprint.patch" \
      && echo "instrumented (-fingerprint hook in the tree) — remember: deinstrument"
    ;;

  deinstrument)
    git -C "$REPO" apply -R --directory=cpp-original \
      "$REPO/tools/cpp-oracle/fingerprint.patch" \
      && echo "pristine (the reference tree is back to master)"
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
        # The build job count is expanded HERE (host shell), not inside the
        # container's sh: an escaped `\"$(nproc)\"` inside the sh -lc string
        # reaches cmake as a literal-quoted `-j"24"` and cmake prints its
        # usage (verified 2026-10-17, the 030b build wall #5).
        if [ "$CMD" = configure ]; then
          # shellcheck disable=SC2046
          docker run "${DOCKER_ARGS[@]}" "$IMAGE" sh -lc \
            "cmake -S /src/cpp-original -B /build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_FLAGS='-include cstdint' $(fc_flags)"
        else
          # shellcheck disable=SC2046
          docker run "${DOCKER_ARGS[@]}" "$IMAGE" sh -lc \
            "cmake -S /src/cpp-original -B /build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_FLAGS='-include cstdint' $(fc_flags) && cmake --build /build -j $(nproc)"
        fi
        ;;
      shell)
        docker run -it "${DOCKER_ARGS[@]}" "$IMAGE" bash
        ;;
      run)
        # Optional `-e KEY=VAL` flags before the `--` separator are forwarded
        # to the container as `--env` (debug hooks, e.g. OH030B_FP_DEBUG_PATH).
        EXTRA_ENV=()
        while [ "${1:-}" = "-e" ]; do
          shift
          EXTRA_ENV+=("--env" "${1:?-e needs KEY=VAL}")
          shift
        done
        [ "${1:-}" = "--" ] && shift || true
        # Inner script (run by the container's sh; the binary name is the
        # cmake target SSVOpenHexagon). Kept single-quoted: no apostrophes
        # inside, or the OUTER shell mis-parses (learned the hard way).
        RUN_SCRIPT='
          set -e
          bin=$(find /build -maxdepth 2 -type f -perm -u+x | grep -E "/(SSVOpenHexagon|OpenHexagon)[.a-z]*$" | head -1)
          [ -n "$bin" ] || { echo "no built SSVOpenHexagon binary in /build" >&2; exit 1; }
          # main() sets the CWD to the parent of argv[0]
          # (src/SSVOpenHexagon/Core/main.cpp) and loads `Assets/` / `Packs/`
          # from the CWD, so stage a _RELEASE-shaped dir under the writable
          # /build: the binary symlink (as argv[0]) + symlinks to every
          # /src/_RELEASE entry (read-only sources).
          mkdir -p /build/_RELEASE
          for e in /src/_RELEASE/*; do
            n=$(basename "$e")
            [ "$n" = "SSVOpenHexagon" ] && continue
            ln -sfn "$e" "/build/_RELEASE/$n"
          done
          ln -sfn "$bin" /build/_RELEASE/SSVOpenHexagon
          exec /build/_RELEASE/SSVOpenHexagon "$@"
        '
        docker run "${DOCKER_ARGS[@]}" "${EXTRA_ENV[@]}" -w /build "$IMAGE" sh -lc "$RUN_SCRIPT" sh "$@"
        ;;
    esac
    ;;

  *)
    sed -n '2,18p' "${BASH_SOURCE[0]}"
    exit 1
    ;;
esac
