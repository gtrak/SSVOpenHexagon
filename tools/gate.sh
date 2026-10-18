#!/usr/bin/env bash
# Gate harness for the Rust port (established by issue 01, reused by every
# later issue). Runs, in order:
#   1. cargo fmt --all -- --check
#   2. cargo build --workspace
#   3. cargo clippy --workspace --all-targets -- -D warnings
#   4. cargo test --workspace
#   5. tools/lua-parity.sh (the Lua parity stage — D-037: the port ships
#      ONE Lua VM, Luau; the stage compares the shipping column against
#      the committed C++ vectors, the probe pins, the dialect census and
#      the prelude tests, over N separate VM processes; ~6-8 min)
#
# Stage output goes to a log under target/ (gitignored); GATE_EXIT=<rc> is
# printed per stage, and a FAILING stage's tail (last 200 lines) is printed
# to the terminal too — the raw log dies with the runner, so without the
# tail a red CI run showed only GATE_EXIT=101 and the compiler error was
# invisible (issue 044: 8 red runs taught nobody anything). Exits non-zero
# if any stage fails.
# Never pipe the gate output to tail (keeps the exit codes honest).

set -u
cd "$(dirname "$0")/.."

LOG_DIR="${CARGO_TARGET_DIR:-target}"
mkdir -p "$LOG_DIR"
LOG="$LOG_DIR/gate.log"
: > "$LOG"

overall=0

run_stage() {
    local name="$1"
    shift
    printf '== %s\n' "$name"
    "$@" >>"$LOG" 2>&1
    local rc=$?
    printf 'GATE_EXIT=%s\n' "$rc"
    if [ "$rc" -ne 0 ]; then
        overall=1
        # The stage output went to the log, not the terminal — print its tail
        # here so the failing stage's actual error is visible in CI and
        # locally (issue 044, step 1: make the failure visible). The log is
        # appended stage-by-stage, so at a failure the log tail IS the
        # failing stage's output.
        printf -- '--- failing stage: %s (last 200 lines of %s) ---\n' "$name" "$LOG"
        tail -n 200 "$LOG"
    fi
}

run_stage "cargo fmt --all -- --check" cargo fmt --all -- --check
run_stage "cargo build --workspace" cargo build --workspace
run_stage "cargo clippy --workspace --all-targets -- -D warnings" \
    cargo clippy --workspace --all-targets -- -D warnings
run_stage "cargo test --workspace" cargo test --workspace
run_stage "tools/lua-parity.sh" tools/lua-parity.sh

printf 'log: %s\n' "$LOG"
exit "$overall"
