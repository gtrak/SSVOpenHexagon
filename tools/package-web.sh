#!/usr/bin/env bash
# Issue 037 — the web (wasm) release packager: builds oh-web for
# wasm32-unknown-emscripten and assembles the INLINE serve directory
# (the page + its artifacts in the SAME directory, so the game is
# playable straight from a static root with no external CDN).
#
#   bash tools/package-web.sh
#
# The recipe (issue 26c W5, D-029 — this script IS the single source of
# truth for it; CI and local runs both call this script, so they cannot
# drift):
#   * `CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions` — MANDATORY
#     for the Luau link (lane 026 re-verified; the root
#     .cargo/config.toml's [target.wasm32-unknown-emscripten] table
#     carries the rest: MODULARIZE/EXPORT_ES6, the WebGL2 minimums,
#     --preload-file=_RELEASE@/RELEASE, the memory flags).
#   * `cargo build --release -p oh-web --target wasm32-unknown-emscripten`
#     from the REPO ROOT (the --preload-file path `_RELEASE` resolves
#     relative to it).
#   * the build artifacts land at (issue 040: honour CARGO_TARGET_DIR the
#     way tools/gate.sh does for LOG_DIR — every lane and the emsdk
#     container run with it set, a hardcoded target/ broke staging):
#       ${CARGO_TARGET_DIR:-target}/wasm32-unknown-emscripten/release/oh-web.js   (the loader)
#       ${CARGO_TARGET_DIR:-target}/wasm32-unknown-emscripten/release/oh_web.wasm
#       ${CARGO_TARGET_DIR:-target}/wasm32-unknown-emscripten/release/deps/oh_web.data
#     (the .data — the preloaded _RELEASE tree — stays in deps/: the
#     lane-036 serve recipe cites it from there; cargo only hoists the
#     primary outputs to release/).
#
# Prereqs (checked below; the GitHub runner gets them from the
# workflow: the `emscripten/emsdk` toolchain via emscripten-core/
# setup-emsdk, the rustup target, and the wasm-bindgen CLI — emcc runs
# the CLI itself over the linked wasm via the -sWASM_BINDGEN flag, so
# it must be on the build PATH at the pinned version):
#   * emcc on PATH
#   * the rustup target wasm32-unknown-emscripten
#   * the wasm-bindgen CLI, version-matched to Cargo.lock
#
# Output (under the gitignored target/):
#   target/package/web/_WEB/            the serve directory
#   target/package/web/open-hexagon-web.zip   files at the zip root
#     (the zip-root layout is the gh-pages deploy contract: unpacking it
#     at the branch root serves index.html + oh-web.js + oh_web.wasm +
#     oh_web.data side by side)

set -euo pipefail
cd "$(dirname "$0")/.."
REPO_ROOT="$(pwd)"

# Issue 040 — where cargo put the build outputs. Honoured exactly the way
# tools/gate.sh honours it for LOG_DIR: ${CARGO_TARGET_DIR:-target}. Lanes
# and the emsdk container always set it (often outside the repo), so the
# staging cp's must follow it — a hardcoded target/ failed with
# `cp: cannot stat 'target/wasm32-unknown-emscripten/release/oh-web.js'`.
# (A relative value resolves against the repo root, the same cwd cargo
# builds from — the same semantics gate.sh relies on.)
OUT="${CARGO_TARGET_DIR:-target}"

TARGET="wasm32-unknown-emscripten"

# ---------------------------------------------------------------------
# The prereqs
# ---------------------------------------------------------------------
command -v emcc >/dev/null || {
    echo "error: emcc not found on PATH (the emsdk toolchain — the emscripten/emsdk container, or emscripten-core/setup-emsdk on the runner)" >&2
    exit 1
}
command -v wasm-bindgen >/dev/null || {
    echo "error: wasm-bindgen CLI not found on PATH (the -sWASM_BINDGEN link flag makes emcc run it over the linked wasm)" >&2
    echo "— install it at the lockfile version: cargo install wasm-bindgen-cli --version $(grep -A1 '^name = \"wasm-bindgen\"$' Cargo.lock | grep '^version' | cut -d'\"' -f2)" >&2
    exit 1
}

LOCKED_WBG="$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | grep '^version' | cut -d'"' -f2)"
CLI_WBG="$(wasm-bindgen --version | awk '{print $2}')"
if [ "$LOCKED_WBG" != "$CLI_WBG" ]; then
    echo "error: wasm-bindgen CLI ${CLI_WBG} does not match the lockfile's ${LOCKED_WBG} (the CLI and the library must be version-matched)" >&2
    exit 1
fi

if ! rustup target list --installed | grep -qx "$TARGET"; then
    echo "== installing rustup target $TARGET"
    rustup target add "$TARGET"
fi

# ---------------------------------------------------------------------
# The build (the root .cargo/config.toml carries the rest of the recipe)
# ---------------------------------------------------------------------
export CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions
echo "== cargo build --release -p oh-web --target $TARGET (CXXFLAGS=-fwasm-exceptions)"
cargo build --release -p oh-web --target "$TARGET"

# ---------------------------------------------------------------------
# The serve directory (the page + the artifacts inline, same directory)
# ---------------------------------------------------------------------
# The staging lives at the repo-root target/ REGARDLESS of CARGO_TARGET_DIR
# (issue 040): it is this script's own output, not cargo's, and a fixed path
# keeps the CI upload path (ci.yml's pages job) and the report's contract
# independent of wherever the build output was pointed.
STAGE_ROOT="${REPO_ROOT}/target/package/web"
STAGE="${STAGE_ROOT}/_WEB"
ZIP="${STAGE_ROOT}/open-hexagon-web.zip"

echo "== staging ${STAGE} (from the tracked page + the build artifacts)"
rm -rf "$STAGE_ROOT"
mkdir -p "$STAGE"

cp crates/oh-web/site/index.html "$STAGE/"
cp "${OUT}/${TARGET}/release/oh-web.js" "$STAGE/"
cp "${OUT}/${TARGET}/release/oh_web.wasm" "$STAGE/"
# The .data stays in deps/ (module docs) — the preloaded _RELEASE tree.
cp "${OUT}/${TARGET}/release/deps/oh_web.data" "$STAGE/"

for f in index.html oh-web.js oh_web.wasm oh_web.data; do
    [ -s "${STAGE}/${f}" ] || { echo "error: serve-dir file missing or empty: ${STAGE}/${f}" >&2; exit 1; }
done

# ---------------------------------------------------------------------
# The zip (files at the ZIP ROOT — the gh-pages inline-deploy layout)
# ---------------------------------------------------------------------
if command -v zip >/dev/null; then
    (cd "$STAGE" && zip -qr "$ZIP" .)
else
    echo "== zip not found; using python3 (same layout: files at the zip root)"
    python3 -c "import shutil; shutil.make_archive('${STAGE_ROOT}/open-hexagon-web', 'zip', root_dir='${STAGE}')"
fi
[ -s "$ZIP" ] || { echo "error: the zip was not produced: $ZIP" >&2; exit 1; }

# ---------------------------------------------------------------------
# The report
# ---------------------------------------------------------------------
echo
echo "== staged layout (${STAGE}):"
(cd "$STAGE" && ls -l)
echo
echo "== zip: ${ZIP} ($(du -h "$ZIP" | cut -f1))"
echo
echo "WEB PACKAGED: ${STAGE} (zip: ${ZIP})"
