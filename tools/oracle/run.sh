#!/usr/bin/env bash
# C++ oracle probes
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
# The net probe links the real libsodium + boost.pfr (the CMake CPM pins);
# on this machine: cached 1.0.18 headers, the system 1.0.18 runtime, and the
# CPM-pinned pfr checkout.
SODIUM_DEV="${SODIUM_DEV:-/home/gary/.cache/ohoracle-deps/sodium-dev}"
SODIUM_LIB="${SODIUM_LIB:-/usr/lib/x86_64-linux-gnu/libsodium.so.23.3.0}"
PFR="${PFR:-/tmp/pfr-b0bf18798c7037ca8a91a1cd2ad2e5798d8f6d46}"

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

echo "=== gameplay geometry (CWall/CPlayer/line-circle) ==="
"$CXX" "${FLAGS[@]}" -include "$HERE/pch.hpp" -o "$OUT/gameplay" \
  "$HERE/gameplay_probe.cpp" "$HERE/stubs.cpp" \
  "$ROOT/src/SSVOpenHexagon/Components/CWall.cpp" \
  "$ROOT/src/SSVOpenHexagon/Components/CPlayer.cpp" \
  "$ROOT/src/SSVOpenHexagon/Utils/Geometry.cpp" \
  "$ROOT/src/SSVOpenHexagon/Utils/Ticker.cpp"
"$OUT/gameplay"

echo "=== net (Online/Shared + Sodium wire vectors) ==="
# `-include cstdint`: Online/Sodium.hpp uses std::uint64_t without including
# <cstdint> (the CMake build gets it transitively); the probe force-includes it
# instead of editing the real source.
"$CXX" "${FLAGS[@]}" -include cstdint \
  -I"$SODIUM_DEV/usr/include" -I"$PFR/include" \
  -o "$OUT/net" "$HERE/net_probe.cpp" "$HERE/stubs.cpp" \
  "$ROOT/src/SSVOpenHexagon/Online/Shared.cpp" \
  "$ROOT/src/SSVOpenHexagon/Online/Sodium.cpp" \
  "$ROOT/src/SSVOpenHexagon/Core/Replay.cpp" \
  "$ROOT/src/SSVOpenHexagon/Utils/Timestamp.cpp" \
  -lz "$SODIUM_LIB"
"$OUT/net"
