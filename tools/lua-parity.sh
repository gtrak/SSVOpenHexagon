#!/usr/bin/env bash
# The Lua parity gate stage (the issue-30 decision tool — reworked by D-037,
# lane 030d: the port now ships ONE Lua VM, Luau, and the LuaJIT arm of the
# A/B harness is gone with it).
#
# What it does:
#
#   1. EMIT: `oh-parity` differential-simulation fingerprints for N runs
#      (run-0..run-(N-1)) of the SHIPPING VM (Luau — default features;
#      raise the repeat count with `LUA_PARITY_REPEATS_COUNT=K`): the full
#      `_RELEASE/Packs` corpus (level cases + the `base/Scripts`
#      helper-script cases), fixed TIME_STEP dt, seeded deterministic input.
#      Artifacts: `target/lua-parity/luau/run-<k>/cases.json` (under
#      `target/` — never committed).
#   2. REPEATS: every run-k vs run-0 — each run index is a SEPARATE
#      `cargo test` process, so a hit means confirmed per-process
#      hash-randomization/`pairs`-order nondeterminism in the shipping VM.
#   3. C++ COLUMN UNDER THE SHIPPING CONFIGURATION: the 030b fingerprint
#      vectors (`parity/cpp_fingerprints.json`, 6 cases / 3627 frames)
#      re-proven against the committed C++ column by
#      `parity_cpp_fingerprints` — this is the shipping configuration's
#      proof of the C++ parity guarantee (030c: 3627/3627 frames under
#      exactly this configuration). The per-case table lands in the log;
#      on a mismatch it carries the first divergent frame + field.
#   4. VM PROBE PINS: `lua_vm_probes`' `probe_pins` — every probe
#      re-run in-process and compared against its fixed reference value
#      (the measured Luau dialect of this exact build; `%.14g` and the
#      port's number→string are the C++-identical ones — the two paths
#      that can reach game-visible text, 030a §3).
#   5. DIALECT CENSUS: `dialect_census` — the zero-5.1-isms sweep over the
#      shipped pack scripts (self-checks its own discrimination: a census
#      pattern that stopped matching would turn the zero into a lie).
#   6. PRELUDE TESTS: `lua51_compat_prelude` — the shipped 5.1 compat set
#      end to end: the one-line Rust `load` binding (every context), the
#      prelude's `loadstring = load` activation
#      (`loadstring('return 6*7')() == 42`), the per-ism classification,
#      the approximation boundaries, idempotency.
#
# Every comparison here compares something real: the Luau column against
# the committed C++ vectors (3), against its own previous processes (2),
# against fixed probe references (4), and the zero census (5) + the
# prelude contract (6). The old stage 7 (clippy under `backend-luau`, the
# non-default configuration) is DELETED by this rework: once the default
# IS Luau, gate stage 3 (`cargo clippy --workspace`) covers the shipping
# configuration, and there is no non-default Lua configuration left to
# lint. The old A/B fingerprint comparison (Luau artifact vs LuaJIT
# artifact) is gone with the second backend — its job is the committed
# C++ column (3), which is a stronger reference (it is the C++ itself).
#
# Exit code: 0 iff every comparison passed.
#
# Standalone: `tools/lua-parity.sh` (or `LUA_PARITY_REPEATS_COUNT=3
# tools/lua-parity.sh`). As a gate stage it is appended to
# `target/gate.log` by `tools/gate.sh`.
#
# Bound: every `cargo test` invocation is wrapped in `timeout`
# (LUA_PARITY_STAGE_TIMEOUT, default 2400 s).

set -u
cd "$(dirname "$0")/.."

REPEATS="${LUA_PARITY_REPEATS_COUNT:-2}"
TIMEOUT="${LUA_PARITY_STAGE_TIMEOUT:-2400}"

# The lane's per-command build environment (same as every other lane
# command; gate.sh does not set these, so default them here).
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
export CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"
export CARGO_PROFILE_DEV_INCREMENTAL="${CARGO_PROFILE_DEV_INCREMENTAL:-false}"

fail=0
note() { printf '[lua-parity] %s\n' "$*"; }
step_fail() { note "FAILED: $*"; fail=1; }

# ---------------------------------------------------------------------------
# 1. Emit fingerprints, N separate processes of the shipping VM (Luau).
# ---------------------------------------------------------------------------
for k in $(seq 0 $((REPEATS - 1))); do
    note "emit luau run-$k (separate VM process)"
    if ! timeout "$TIMEOUT" env LUA_PARITY_RUN="$k" cargo test -q -p oh-parity \
        --test lua_engine_parity emit_fingerprints -- --exact; then
        step_fail "emit luau run-$k"
    fi
done

# ---------------------------------------------------------------------------
# 2. Per-process determinism: every run-k vs run-0 (the shipping VM).
# ---------------------------------------------------------------------------
repeats_list=""
for k in $(seq 1 $((REPEATS - 1)) 2>/dev/null); do
    repeats_list="${repeats_list:+$repeats_list,}$k"
done
if [ -n "$repeats_list" ]; then
    note "repeat determinism (run-$repeats_list vs run-0)"
    if ! timeout "$TIMEOUT" env LUA_PARITY_REPEATS="$repeats_list" \
        cargo test -q -p oh-parity \
        --test lua_engine_parity repeat_determinism -- --exact; then
        step_fail "repeat determinism (luau)"
    fi
fi

# ---------------------------------------------------------------------------
# 3. The C++ column under the shipping configuration (030b vectors vs
#    the port's ONE VM — the C++ parity guarantee, re-proven per gate).
# ---------------------------------------------------------------------------
note "C++ fingerprint vectors under the shipping configuration (030c proof: 3627/3627)"
if ! timeout "$TIMEOUT" cargo test -q -p oh-parity --test parity_cpp_fingerprints; then
    step_fail "C++ fingerprint vectors (first divergent frame is in the log)"
fi

# ---------------------------------------------------------------------------
# 4. The VM probe pins (fixed reference values for the shipping VM's
#    dialect — including the C++-identical %.14g / number->string paths).
# ---------------------------------------------------------------------------
note "probe pins (the shipping VM's dialect vs its fixed reference)"
if ! timeout "$TIMEOUT" cargo test -q -p oh-parity \
    --test lua_vm_probes probe_pins -- --exact; then
    step_fail "probe pins"
fi

# ---------------------------------------------------------------------------
# 5. The third-party dialect census (zero 5.1-isms in the shipped packs,
#    self-checking its own patterns' discrimination).
# ---------------------------------------------------------------------------
note "dialect census (the zero, re-verified)"
if ! timeout "$TIMEOUT" cargo test -q -p oh-parity --test dialect_census; then
    step_fail "dialect census"
fi

# ---------------------------------------------------------------------------
# 6. The prelude tests (the shipped 5.1 compat set + the one-line Rust
#    load binding, end to end).
# ---------------------------------------------------------------------------
note "prelude tests (the 5.1 compat set under the shipping VM)"
if ! timeout "$TIMEOUT" cargo test -q -p oh-parity --test lua51_compat_prelude; then
    step_fail "prelude tests"
fi

if [ "$fail" -eq 0 ]; then
    note "OK: $REPEATS runs of the shipping VM; C++ column, probe pins, census and prelude all compared (artifacts under $CARGO_TARGET_DIR/lua-parity/)"
else
    note "FAIL: see the lines above (artifacts under $CARGO_TARGET_DIR/lua-parity/)"
fi
exit "$fail"
