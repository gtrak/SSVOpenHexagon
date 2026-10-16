#!/usr/bin/env bash
# Issue 24 — the Rust release packager: builds the release artifacts and
# assembles a distributable directory that matches the C++ `_RELEASE`
# layout EXACTLY, from a PRISTINE copy of the tracked `_RELEASE` (the
# script never writes into the tracked `_RELEASE`).
#
# Usage:
#   bash tools/package.sh                # Linux: release build + stage + smoke test
#   bash tools/package.sh --windows      # Windows cross build (via tools/build-windows.sh)
#   bash tools/package.sh --with-steam   # add the Steam feature + the Steam natives
#   bash tools/package.sh --skip-smoke   # skip the packaged-binary smoke test
#
# ---------------------------------------------------------------------
# The C++ `_RELEASE` layout (D-014, cited from this tree)
# ---------------------------------------------------------------------
# The tracked `_RELEASE/` is the C++ install root the C++ binary runs
# from: the game data (`Assets/`, `Packs/`, `ConfigOverrides/`), the
# game config (`config.json` — the 101-key file `oh-config` pins, issue
# 04), the Steam/Discord natives (`steam_api64.dll`/`libsteam_api.so`,
# `discord_game_sdk.dll`/`libdiscord_game_sdk.so`, the
# `sdkencryptedappticket*` pair, `steam_appid.txt`), the Windows launch
# scripts (`windowed.bat`, `fullscreen.bat`, …), the Linux launcher
# (`run_ssvopenhexagon_linux.sh`), and the runtime-data seeds
# (`scores.json`, `users.json`). The C++ build artifacts that CI drops
# into it (`SSVOpenHexagon`, `SSVOpenHexagonLinux`, `OHWorkshopUploader*`
# — all gitignored, see .gitignore) complete the layout: the BINARY
# ALONGSIDE `Assets/`, `Packs/`, `ConfigOverrides/`, `config.json`.
#
# The C++'s own release-assembly scripts confirm the shipping subset:
# `prepare_release.sh` (Windows) and `prepare_release_linux.sh` (Linux)
# copy `Assets/`, `ConfigOverrides/`, `Packs/`, an empty `Profiles/`,
# the game binary(ies), the platform natives, and `steam_appid.txt` —
# the binary in the same directory as the data. (`Profiles/` and
# `Replays/` are runtime-created by the binary itself — main.cpp:275-276,
# ported in oh-app/src/client.rs — so the package does not pre-create
# them, exactly like the C++ prepared releases… which DO pre-create an
# empty `Profiles/`; the difference is cosmetic: the binary creates the
# folder on first run either way.)
#
# This script assembles the Rust equivalent — the `open-hexagon` binary
# alongside `Assets/`, `Packs/`, `ConfigOverrides/`, `config.json` — so
# a player can drop the Rust binary into an existing C++ install's
# `_RELEASE/` and run it unchanged (the contract's objective,
# .agents/plans/001-rust-port/24-packaging.md).
#
# Deviations from the C++ prepared releases (documented, deliberate):
#   * NO Steam/Discord natives in the DEFAULT build — the default Rust
#     build never links them (oh-app's `steam` feature is default-OFF,
#     the C++ no-SDK build analogue, issue 20; the Rust Discord
#     integration speaks the local-client IPC pipe, issue 21 — no DLL/
#     .SO to bundle at all). `--with-steam` adds the Steam feature AND
#     the loader stub the linked binary cannot start without (the
#     `steamworks-sys` 0.13.0 bundled stub — the repo-tracked
#     `_RELEASE` stub is one SDK generation OLDER than the linked
#     binary, see the staging section below — + `steam_appid.txt` (1358090);
#     `.agents/skills/lib-steamworks.md` §4).
#   * NO Windows `.bat` launchers / Linux launcher script — those wrap
#     the C++ binary's own args; the Rust binary takes the same CLI
#     (see release/README.md).
#   * NO `scores.json`/`users.json` seeds — user/leaderboard DATA the
#     runtime manages (the C++ server DB is `ohdb.sqlite`, issue 17-18;
#     the C++ prepared releases do not ship them either).
#   * The full tracked `Packs/` ships (all 8 packs the oh-assets count
#     pins, issue 05); the C++ prepared releases curate 5 official
#     packs (`prepare_release.sh:10-15`) — a curation choice, not a
#     layout one; the extra packs are additive.
#   * The Rust tree also builds `oh-server-control` and
#     `oh-workshop-uploader` (issues 18-19) — they stay in
#     `target/<...>/release/` and are NOT part of the `_RELEASE`
#     game-install layout.
#
# ---------------------------------------------------------------------
# Staging + the pristine-copy invariant
# ---------------------------------------------------------------------
# The staged directory is `target/package/<platform>/_RELEASE` (under
# the gitignored `target/` — no new untracked roots in the repo). The
# name `_RELEASE` is deliberate: the staged directory IS a drop-in
# `_RELEASE`. Everything is COPIED from the tracked `_RELEASE`; nothing
# is ever written back into it (verified at the end via
# `git status --porcelain`).
#
# ---------------------------------------------------------------------
# The smoke test (the packaged dir MUST be tested)
# ---------------------------------------------------------------------
# The headless_replay pattern (crates/oh-app/tests/headless_replay.rs):
# the packaged binary is run headless in a SANDBOXED copy of the staged
# `_RELEASE` (the binary writes `Profiles/`, `Replays/` and saves
# `config.json` into its CWD on every run — main.cpp:275-291 — so it
# must never run inside the staged dir itself), replaying the committed
# fixture `test/fixtures/cube_1_apeirogon.ohr.z`. That fixture is a
# compressed replay of the harness's `alwaysSpinRight` live run on
# `cube_1`/`apeirogon` (the exact ids the C++ test names — D-018),
# generated once by the issue-24 lane and committed: the replay carries
# its RNG seed (`replays always re-seed from the replay file's seed`,
# oh-game/src/hexagon_game.rs:2554; `rng =
# random_number_generator{activeReplay->replayFile._seed}` :777-783),
# so the replayed score is ABSOLUTELY pinned — the verified value is
# `Final time: 1.725` (stable across runs, release build; the D-018
# cross-check: it equals the harness's own live-run score for that
# replay). The smoke asserts: exit 0, the exact C++ `Player died.` +
# `Final time:` line with the pinned score (main.cpp:568-573), the
# `Finished` line (main.cpp:594), and the config-save guard's output
# (main.cpp:285-291). On a Linux host the Windows build's smoke test is
# skipped-with-reason (no .exe runtime; it is covered by the
# windows-latest CI gate + a manual run).

set -euo pipefail
cd "$(dirname "$0")/.."

REPO_ROOT="$(pwd)"
RELEASE_SRC="${REPO_ROOT}/_RELEASE"
FIXTURE="${REPO_ROOT}/test/fixtures/cube_1_apeirogon.ohr.z"
PINNED_FINAL_TIME="1.725"   # the fixture's replayed score (module docs)

PLATFORM="linux"
WITH_STEAM=0
SKIP_SMOKE=0

for arg in "$@"; do
    case "$arg" in
    --linux) PLATFORM="linux" ;;
    --windows) PLATFORM="windows" ;;
    --with-steam) WITH_STEAM=1 ;;
    --skip-smoke) SKIP_SMOKE=1 ;;
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

# ---------------------------------------------------------------------
# The build
# ---------------------------------------------------------------------
CARGO_FEATURES=()
if [ "$WITH_STEAM" -eq 1 ]; then
    CARGO_FEATURES=(--features oh-app/steam)
fi

if [ "$PLATFORM" = "linux" ]; then
    echo "== cargo build --workspace --release ${CARGO_FEATURES[*]:-}"
    cargo build --workspace --release "${CARGO_FEATURES[@]+"${CARGO_FEATURES[@]}"}"
    BIN="target/release/open-hexagon"
    PLATFORM_DIR="linux-$(rustc -vV | awk '/^host:/{print $2}')"
else
    # The Windows cross build (issue 22's tools/build-windows.sh:
    # the libsodium cross-prebuild + the full cargo cross-compile + link
    # of the .exe's; `--release` = the release profile, added by issue 24
    # for this packager). Its prereqs (the rustup target, mingw-w64, make)
    # are checked inside that script.
    echo "== bash tools/build-windows.sh --build --release ${CARGO_FEATURES[*]:-}"
    if [ "${#CARGO_FEATURES[@]}" -gt 0 ]; then
        # build-windows.sh execs a fixed cargo line; the steam-feature
        # Windows build is a documented manual extension (the default
        # cross path is feature-OFF, like the C++'s shipped build).
        echo "error: --with-steam is not wired into the cross path yet (build-windows.sh's fixed cargo line); use the default (feature-OFF) Windows build" >&2
        exit 1
    fi
    bash tools/build-windows.sh --build --release
    BIN="target/x86_64-pc-windows-gnu/release/open-hexagon.exe"
    PLATFORM_DIR="windows-x86_64-pc-windows-gnu"
fi

[ -f "$BIN" ] || { echo "error: release binary missing: $BIN" >&2; exit 1; }

# ---------------------------------------------------------------------
# The staging (a pristine copy of the tracked _RELEASE + the binary)
# ---------------------------------------------------------------------
STAGE_ROOT="${REPO_ROOT}/target/package"
STAGE="${STAGE_ROOT}/${PLATFORM_DIR}/_RELEASE"

echo "== staging ${STAGE} (from a pristine copy of ${RELEASE_SRC})"
rm -rf "${STAGE_ROOT}/${PLATFORM_DIR}"
mkdir -p "$STAGE"

# The layout's four data entries (copied — never moved, never written
# back): the C++ layout's `Assets/` + `Packs/` + `ConfigOverrides/` +
# `config.json` (prepare_release.sh / prepare_release_linux.sh copy the
# same data set; the tracked _RELEASE/config.json is the binary's
# config root, main.cpp:279-283).
cp -R "${RELEASE_SRC}/Assets" "$STAGE/"
cp -R "${RELEASE_SRC}/Packs" "$STAGE/"
cp -R "${RELEASE_SRC}/ConfigOverrides" "$STAGE/"
cp "${RELEASE_SRC}/config.json" "$STAGE/"

# The binary, alongside them.
cp "$BIN" "$STAGE/"
chmod +x "$STAGE/$(basename "$BIN")"

# --with-steam: the natives the linked binary cannot start without
# (.agents/skills/lib-steamworks.md §4). IMPORTANT — the stub must be the
# one the build actually linked against: `steamworks-sys` 0.13.0 (the
# Cargo.lock pin) bundles the official loader stubs, and its build.rs
# links the binary against the BUNDLED stub — the newer SDK generation.
# The repo-tracked `_RELEASE/libsteam_api.so` / `steam_api64.dll` (the
# OLDER stubs the C++ links, CMakeLists.txt:419-425) stop one vtable
# generation behind (the tracked Linux stub exports
# `SteamAPI_SteamUserStats_v012` but not `..._v013`): verified, a
# steam-linked binary run against the tracked stub aborts at startup
# with `undefined symbol: SteamAPI_SteamUserStats_v013`. So the packager
# stages the bundled stub (found in the cargo registry cache — the same
# idiom tools/build-windows.sh uses for the libsodium-sys source). Only
# `steam_appid.txt` (1358090 — SDK-generation-independent) comes from
# the tracked _RELEASE.
if [ "$WITH_STEAM" -eq 1 ]; then
    STEAM_SYS_DIR="$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/steamworks-sys-0.13.0 2>/dev/null | head -n1)"
    if [ -z "${STEAM_SYS_DIR:-}" ]; then
        # Fetch the crate into the registry cache once (the build above
        # already did; this is the defensive re-check).
        cargo fetch
        STEAM_SYS_DIR="$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/steamworks-sys-0.13.0 2>/dev/null | head -n1)"
    fi
    [ -n "${STEAM_SYS_DIR:-}" ] || { echo "error: steamworks-sys-0.13.0 not found in the cargo registry cache" >&2; exit 1; }
    if [ "$PLATFORM" = "linux" ]; then
        cp "${STEAM_SYS_DIR}/lib/steam/redistributable_bin/linux64/libsteam_api.so" "$STAGE/"
    else
        cp "${STEAM_SYS_DIR}/lib/steam/redistributable_bin/win64/steam_api64.dll" "$STAGE/"
    fi
    cp "${RELEASE_SRC}/steam_appid.txt" "$STAGE/"
    echo "== steam natives staged (steamworks-sys bundled loader stub + steam_appid.txt; the repo-tracked _RELEASE stub is one SDK generation older than the linked binary — see module docs)"
fi
# Discord: NEVER a native in the Rust build (issue 21: the local-client
# IPC pipe via discord-rich-presence — the C++'s discord_game_sdk.*
# natives are for the official Game-SDK C++ link only).

# ---------------------------------------------------------------------
# The smoke test: the packaged binary, headless, in a sandboxed copy
# ---------------------------------------------------------------------
if [ "$SKIP_SMOKE" -eq 1 ]; then
    echo "== smoke test skipped (--skip-smoke)"
else
    [ "$PLATFORM" = "linux" ] || {
        echo "== smoke test skipped-with-reason: no .exe runtime on this Linux host (covered by the windows-latest CI gate + a manual run)"
        SKIP_SMOKE=1
    }
fi
if [ "$SKIP_SMOKE" -eq 0 ]; then
    [ -f "$FIXTURE" ] || { echo "error: fixture missing: $FIXTURE" >&2; exit 1; }
    SANDBOX="$(mktemp -d "${TMPDIR:-/tmp}/oh-package-smoke.XXXXXX")"
    trap 'rm -rf "$SANDBOX"' EXIT
    # A SANDBOXED copy (module docs: the binary writes into its CWD —
    # the staged dir must stay pristine).
    cp -R "$STAGE/." "$SANDBOX/"
    cp "$FIXTURE" "$SANDBOX/cube_1_apeirogon.ohr.z"

    echo "== smoke: ./open-hexagon -headless cube_1_apeirogon.ohr.z (in ${SANDBOX})"
    SMOKE_RC=0
    # The C++ launcher pattern (run_ssvopenhexagon_linux.sh:
    # LD_LIBRARY_PATH=".") — needed iff the package carries the Steam
    # loader stub (the plain NEEDED entry does not search the binary's
    # own directory).
    if [ -f "$SANDBOX/libsteam_api.so" ]; then
        SMOKE_OUT="$(cd "$SANDBOX" && LD_LIBRARY_PATH=. ./open-hexagon -headless cube_1_apeirogon.ohr.z 2>/dev/null)" || SMOKE_RC=$?
    else
        SMOKE_OUT="$(cd "$SANDBOX" && ./open-hexagon -headless cube_1_apeirogon.ohr.z 2>/dev/null)" || SMOKE_RC=$?
    fi
    if [ "$SMOKE_RC" -ne 0 ]; then
        echo "error: smoke test exit ${SMOKE_RC} (expected 0); output:" >&2
        echo "$SMOKE_OUT" >&2
        exit 1
    fi
    grep -q "Player died.$" <<<"$SMOKE_OUT" || {
        echo "error: no 'Player died.' line; output:" >&2; echo "$SMOKE_OUT" >&2; exit 1
    }
    grep -q "Final time: ${PINNED_FINAL_TIME}$" <<<"$SMOKE_OUT" || {
        echo "error: pinned score '${PINNED_FINAL_TIME}' not printed (the replay carries its seed — the score must be exact); output:" >&2
        echo "$SMOKE_OUT" >&2
        exit 1
    }
    grep -q '\[::mainClient\] Finished$' <<<"$SMOKE_OUT" || {
        echo "error: no Finished line; output:" >&2; echo "$SMOKE_OUT" >&2; exit 1
    }
    grep -q '\[::main\] Done saving config$' <<<"$SMOKE_OUT" || {
        echo "error: no config-save guard output; output:" >&2; echo "$SMOKE_OUT" >&2; exit 1
    }
    [ -d "$SANDBOX/Profiles" ] && [ -d "$SANDBOX/Replays" ] || {
        echo "error: runtime folders (Profiles/, Replays/) not created by the packaged binary" >&2; exit 1
    }
    [ -f "$SANDBOX/config.json" ] || {
        echo "error: the config-save guard did not save config.json into the sandbox" >&2; exit 1
    }
    echo "== smoke: PASS (exit 0, 'Final time: ${PINNED_FINAL_TIME}', Finished + config-save guard verified)"
fi

# ---------------------------------------------------------------------
# The layout report + the pristine-copy invariant
# ---------------------------------------------------------------------
echo
echo "== staged layout (${STAGE}):"
(cd "$STAGE" && find . -maxdepth 1 | sort)
echo
echo "== tracked _RELEASE untouched (git status):"
if [ -n "$(git status --porcelain -- _RELEASE)" ]; then
    git status --porcelain -- _RELEASE
    echo "error: the tracked _RELEASE was mutated" >&2
    exit 1
fi
echo "(clean)"
echo
echo "PACKAGED: ${STAGE}"
