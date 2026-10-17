#!/usr/bin/env bash
# Lane 030a: the Lua-backend parity gate stage (the "unify everything on
# Luau?" decision tool).
#
# What it does (each backend in its own `cargo test` PROCESSES — the
# multi-process repeat is what settles the per-process hash-randomization /
# `pairs`-order question empirically):
#
#   1. EMIT: `oh-parity` differential-simulation fingerprints for N runs
#      (run-0..run-(N-1)) per backend (default N=2; raise with
#      `LUA_PARITY_REPEATS_COUNT=K`): the full `_RELEASE/Packs` corpus
#      (level cases + the `base/Scripts` helper-script cases), fixed
#      TIME_STEP dt, seeded deterministic input. Artifacts:
#      `target/lua-parity/<backend>/run-<k>/cases.json` (under `target/` —
#      never committed).
#   2. PROBE: the explicit VM probes (number spellings, `%.14g`, the
#      port's number->string path, `error()` shape, dotted-path semantics,
#      post-sandbox surface, `pairs` order, `mlua::Integer` width):
#      `target/lua-parity/<backend>/run-0/probes.json`.
#   3. COMPARE A/B: `luajit/run-0` vs `luau/run-0` — the comparator fails
#      with the case + FIRST DIVERGENT FRAME (or "identical").
#   4. COMPARE REPEATS: every run-k vs run-0 within each backend
#      (per-process determinism; a hit means confirmed
#      hash-randomization/pairs-order nondeterminism).
#   5. COMPARE PROBES: the per-probe diff table (printed to the log).
#   6. C++ COLUMN UNDER LUAU (lane 030c): the 030b fingerprint vectors
#      re-proven under the Luau backend (the third column of the
#      C++ ↔ LuaJIT ↔ Luau proof; the LuaJIT column is gate stage 4's
#      default-backend `cargo test --workspace` over the same test file).
#
# Exit code: 0 iff every comparison passed. The probe table is
# informational (backends ARE expected to differ at the dialect level);
# the A/B fingerprint comparison is the gate: the shipped corpus must
# simulate byte-identically, or the report says why not.
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

# `run_test <backend|luajit> <test> [env VAR=val ...]` — one cargo-test
# process (one VM process).
run_test() {
    local flags=()
    if [ "$1" = "luau" ]; then
        flags+=(--no-default-features --features backend-luau)
    fi
    shift
    local test="$1"; shift
    timeout "$TIMEOUT" env "$@" cargo test -q -p oh-parity \
        ${flags[@]+"${flags[@]}"} \
        --test lua_engine_parity "$test" -- --exact
}

# ---------------------------------------------------------------------------
# 1+2. Emit fingerprints + probes, N separate processes per backend.
# ---------------------------------------------------------------------------
for backend in luajit luau; do
    for k in $(seq 0 $((REPEATS - 1))); do
        note "emit $backend run-$k (separate VM process)"
        if ! run_test "$backend" emit_fingerprints LUA_PARITY_RUN="$k"; then
            step_fail "emit $backend run-$k"
        fi
    done
    note "emit $backend probes (run-0, separate VM process)"
    if [ "$backend" = luau ]; then
        timeout "$TIMEOUT" cargo test -q -p oh-parity --no-default-features \
            --features backend-luau --test lua_vm_probes emit_probes -- --exact \
            || step_fail "emit luau probes"
    else
        timeout "$TIMEOUT" cargo test -q -p oh-parity \
            --test lua_vm_probes emit_probes -- --exact \
            || step_fail "emit luajit probes"
    fi
done

# ---------------------------------------------------------------------------
# 3. A/B comparison (luajit/run-0 vs luau/run-0) + 4. per-backend repeats.
# ---------------------------------------------------------------------------
if ! timeout "$TIMEOUT" env LUA_PARITY_COMPARE=1 cargo test -q -p oh-parity \
    --test lua_engine_parity compare_backends -- --exact; then
    step_fail "A/B comparison (first divergent frame is in the log)"
fi

repeats_list=""
for k in $(seq 1 $((REPEATS - 1)) 2>/dev/null); do
    repeats_list="${repeats_list:+$repeats_list,}$k"
done
for backend in luajit luau; do
    [ -z "$repeats_list" ] && continue
    if [ "$backend" = luau ]; then
        timeout "$TIMEOUT" env LUA_PARITY_REPEATS="$repeats_list" \
            cargo test -q -p oh-parity --no-default-features --features backend-luau \
            --test lua_engine_parity repeat_determinism -- --exact \
            || step_fail "repeat determinism ($backend)"
    else
        timeout "$TIMEOUT" env LUA_PARITY_REPEATS="$repeats_list" \
            cargo test -q -p oh-parity \
            --test lua_engine_parity repeat_determinism -- --exact \
            || step_fail "repeat determinism ($backend)"
    fi
done

# ---------------------------------------------------------------------------
# 5. The probe diff table (informational).
# ---------------------------------------------------------------------------
timeout "$TIMEOUT" env LUA_PARITY_PROBES=1 cargo test -q -p oh-parity \
    --test lua_vm_probes compare_probes -- --exact \
    || step_fail "probe diff table"

# ---------------------------------------------------------------------------
# 6. The 030c third column: the 030b C++ fingerprint vectors re-proven
#    under Luau (parity_cpp_fingerprints runs backend-agnostically; the
#    default-backend LuaJIT column is gate stage 4). On a mismatch the log
#    carries the per-case table + the first divergent frame + field.
# ---------------------------------------------------------------------------
note "cpp-fingerprint vectors under Luau (030c third column)"
if ! timeout "$TIMEOUT" cargo test -q -p oh-parity --no-default-features \
    --features backend-luau --test parity_cpp_fingerprints; then
    step_fail "cpp-fingerprint vectors under Luau (first divergent frame is in the log)"
fi

if [ "$fail" -eq 0 ]; then
    note "OK: $REPEATS runs per backend; A/B + repeats + probes all compared (artifacts under $CARGO_TARGET_DIR/lua-parity/)"
else
    note "FAIL: see the lines above (artifacts under $CARGO_TARGET_DIR/lua-parity/)"
fi
exit "$fail"
