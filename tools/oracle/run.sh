#!/usr/bin/env bash
# Builds and runs the C++ oracle probes; prints the golden values.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HERE="$ROOT/tools/oracle"
OUT="$(mktemp -d)"
CXX="${CXX:-g++}"
FLAGS=(-std=c++20 -O0
  -I"$HERE/shim" -I"$ROOT/include" -I"$ROOT"
  -I"$ROOT/extlibs/vrm_pp/include" -I"$ROOT/extlibs/SSVUtils/include"
  -I"$ROOT/extlibs/SSVStart/include")

echo "=== pcg ==="
"$CXX" "${FLAGS[@]}" -o "$OUT/pcg" "$HERE/pcg_probe.cpp"
"$OUT/pcg"

echo "=== replay ==="
"$CXX" "${FLAGS[@]}" -o "$OUT/replay" "$HERE/replay_probe.cpp" "$HERE/stubs.cpp" \
  "$ROOT/src/SSVOpenHexagon/Core/Replay.cpp" "$ROOT/src/SSVOpenHexagon/Utils/Timestamp.cpp" -lz
"$OUT/replay"

echo "=== concat/buildPackId/validator ==="
"$CXX" "${FLAGS[@]}" -o "$OUT/concat" "$HERE/concat_probe.cpp" "$HERE/stubs.cpp" \
  "$ROOT/src/SSVOpenHexagon/Utils/BuildPackId.cpp" "$ROOT/src/SSVOpenHexagon/Utils/LevelValidator.cpp"
"$OUT/concat"

echo "=== ostream float formatting (hg::Utils::concat) ==="
"$CXX" "${FLAGS[@]}" -o "$OUT/format" "$HERE/format_probe.cpp"
"$OUT/format"

echo "=== timepoint formatting (hg::Utils::formatTimepoint) ==="
"$CXX" "${FLAGS[@]}" -o "$OUT/timestamp" "$HERE/timestamp_probe.cpp"
"$OUT/timestamp"
