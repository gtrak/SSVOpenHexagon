#!/usr/bin/env bash
# Issue 057 — the emsdk container's build step (the host's rustup/cargo
# mounted in, per the 26c/040 recipe: the emsdk image has `emcc` but no
# cargo/rust). Run it from the host with:
#
#   podman run --rm \
#     -v "$PWD"/oh-lanes/<lane>:/work \
#     -v /home/gary/.rustup:/home/gary/.rustup \
#     -v /home/gary/.cargo:/home/gary/.cargo \
#     -e RUSTUP_HOME=/home/gary/.rustup \
#     -e CARGO_HOME=/home/gary/.cargo \
#     -w /work \
#     docker.io/emscripten/emsdk:latest bash /work/tools/build-wasm-emsdk.sh
#
# (podman-emulated on this host; the emsdk image is the documented build
# env — issue-026 §4.) The subtleties this pins:
#   - `CARGO_TARGET_DIR=/work/target` — the MOUNTED path. A host-side
#     target dir (or a container-local /tmp) loses the artifacts when
#     the container exits; the staging (tools/package-web.sh) reads
#     target/wasm32-unknown-emscripten/release/ RELATIVE to the repo
#     root (the 040 CARGO_TARGET_DIR contract).
#   - `CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions` — the
#     Luau link's mandatory env (the .cargo/config.toml 26c.2 note).
#   - `--release` — a dev build of the wasm TRAPS on every rAF
#     (`RuntimeError: memory access out of bounds`, lane 057's first
#     web repro) — the release profile is the only one that runs.
#   - `PATH=/home/gary/.cargo/bin` — cargo itself (the image has none),
#     and wasm-bindgen (the `-sWASM_BINDGEN` link runs the CLI).
#
# Output: /work/target/wasm32-unknown-emscripten/release/{oh-web.js,
# oh_web.wasm, deps/oh_web.data} — the three artifacts the page
# (crates/oh-web/site/index.html + tools/package-web.sh's staging)
# loads in one directory.
set -euo pipefail
export PATH=/home/gary/.cargo/bin:$PATH
cd /work
export CARGO_TARGET_DIR=/work/target
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_DEV_INCREMENTAL=false
export CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions
timeout 5400 cargo build --release -p oh-web --target wasm32-unknown-emscripten
