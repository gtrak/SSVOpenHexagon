#!/usr/bin/env bash
# Issue 23 — the SSVOpenHexagon Rust-port PARITY HARNESS driver.
#
# Primary harness: the `crates/oh-parity` test crate (run via
# `cargo test -p oh-parity`, part of the `cargo test --workspace` gate).
# This script is the CORPUS REGENERATION + VERIFICATION script (the
# contract's "committed script" for the C++-derived corpus):
#
#   1. compiles the REAL C++ sources (Core/RandomNumberGenerator.cpp,
#      Core/Replay.cpp, Utils/BuildPackId.cpp, Utils/LevelValidator.cpp)
#      against the repo's minimal SFML shims (tools/oracle/parity_probe.cpp
#      — the established oracle-probe pattern),
#   2. runs it, splits the output on the `=== corpus: <file> ===` markers,
#      and byte-compares the three C++-derived corpus files
#      (.agents/plans/001-rust-port/parity/{rng_streams,
#      replay_serialization,pack_ids}.json) against the committed corpus —
#      a mismatch means the C++ ground truth drifted (or the corpus was
#      hand-edited),
#   3. sanity-parses the rust-derived level_runs.json (the seeded-replay
#      pins + the C++-derived timekeeping identity),
#   4. runs the parity crate's tests (the Rust build vs the corpus).
#
# Exit 0 = corpus stable + parity green. Never pipe this to tail.
# 038: the C++ tree lives in cpp-original/ (the probes use the canonical
# paths; the root src/include symlinks are for the citation corpus only).
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HERE="$ROOT/tools/oracle"
CORPUS="$ROOT/.agents/plans/001-rust-port/parity"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
CXX="${CXX:-g++}"
FLAGS=(-std=c++20 -O0
  -I"$HERE/shim" -I"$ROOT/cpp-original/include" -I"$ROOT/cpp-original"
  -I"$ROOT/cpp-original/extlibs/vrm_pp/include"
  -I"$ROOT/cpp-original/extlibs/SSVUtils/include"
  -I"$ROOT/cpp-original/extlibs/SSVStart/include")

echo "=== build the parity probe (real C++ sources + shims) ==="
if ! "$CXX" "${FLAGS[@]}" -o "$OUT/parity" "$HERE/parity_probe.cpp" \
  "$HERE/stubs.cpp" \
  "$ROOT/cpp-original/src/SSVOpenHexagon/Core/RandomNumberGenerator.cpp" \
  "$ROOT/cpp-original/src/SSVOpenHexagon/Core/Replay.cpp" \
  "$ROOT/cpp-original/src/SSVOpenHexagon/Utils/BuildPackId.cpp" \
  "$ROOT/cpp-original/src/SSVOpenHexagon/Utils/LevelValidator.cpp" \
  "$ROOT/cpp-original/src/SSVOpenHexagon/Utils/Timestamp.cpp" -lz; then
    echo "PROBE BUILD FAILED (need g++/c++ and libz; see the probe header)"
    exit 1
fi
echo "probe build: ok"

echo "=== run the probe; split on the corpus markers ==="
if ! "$OUT/parity" >"$OUT/probe.out"; then
    echo "PROBE RUN FAILED"
    exit 1
fi

current=""
while IFS= read -r line; do
    if [[ "$line" == "=== corpus: "*" ===" ]]; then
        current="${line#=== corpus: }"
        current="${current% ===}"
        : >"$OUT/$current"
        continue
    fi
    if [[ -n "$current" ]]; then
        printf '%s\n' "$line" >>"$OUT/$current"
    fi
done <"$OUT/probe.out"

failed=0
for f in rng_streams.json replay_serialization.json pack_ids.json; do
    if [[ ! -s "$OUT/$f" ]]; then
        echo "MISSING generated corpus file: $f"
        failed=1
        continue
    fi
    if cmp -s "$OUT/$f" "$CORPUS/$f"; then
        echo "corpus stable (C++-derived, byte-identical): $f"
    else
        echo "CORPUS DRIFT: $f differs from the probe output"
        diff -u "$CORPUS/$f" "$OUT/$f" | head -40
        failed=1
    fi
done

echo "=== sanity-parse the rust-derived level_runs.json ==="
if command -v python3 >/dev/null 2>&1; then
    python3 - "$CORPUS/level_runs.json" <<'PY'
import json, sys
doc = json.load(open(sys.argv[1]))
runs = doc["runs"]
assert len(runs) == 4, f"expected 4 unique level-run entries, got {len(runs)}"
for r in runs:
    # The corpus pins the SEEDED REPLAY run: expected_frames = input_count
    # = the death frame (one input consumed per started frame), and the
    # C++ timekeeping identity (Config.hpp:26 TIME_STEP = 60/240 = 0.25;
    # the replay run defers its start by one frame — HGUpdate.cpp:187 —
    # then resetTime's currentPause = 0.1*60 = 6.0 units consumes 24
    # frames): S == ((F - 25) * 0.25) / 60.0, computed exactly in f64
    # (integer * 0.25, then one correctly-rounded division — the same op
    # sequence the engine accumulates through).
    F = r["expected_frames"]
    S = r["expected_score"]
    assert r["input_count"] == F, r
    assert ((F - 25) * 0.25) / 60.0 == S, r
print("level_runs.json: parses + the timekeeping identity holds")
PY
else
    echo "python3 unavailable: skipped the level_runs.json parse check"
fi

echo "=== run the parity crate (Rust build vs the corpus) ==="
(cd "$ROOT" && cargo test -p oh-parity) || failed=1

if [[ "$failed" -eq 0 ]]; then
    echo "PARITY_OK: corpus stable, parity tests green"
fi
exit "$failed"
