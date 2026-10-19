#!/usr/bin/env bash
# tools/smoke-windowed.sh — the WINDOWED desktop smoke test.
#
# Why: the desktop build once silently degraded to a one-frame HEADLESS run
# (it printed `[::mainClient] Finished` and exited 0, opening NO window)
# because the desktop-mode winit probe consumed the process's single
# `EventLoop`. The existing packaging smoke (tools/package.sh) only runs
# `-headless`, so it never caught that. This smoke boots the game WINDOWED
# and proves: a real GL context comes up (the `[oh-render] context
# GL_SAMPLES` line), the client does NOT fall back to the one-frame
# headless path (no `[::mainClient] Finished`), and the client is still in
# its frame loop when the timeout fires (exit 124).
#
# Usage:
#   tools/smoke-windowed.sh [path-to-open-hexagon]
# (default: ${CARGO_TARGET_DIR:-target}/debug/open-hexagon — honors
# CARGO_TARGET_DIR, same as tools/gate.sh)
#
# The game writes `Profiles/`, `Replays/` and `config.json` into its CWD on
# every run, so it is NEVER run inside the tracked `_RELEASE`: it runs from
# a throwaway `mktemp -d` sandbox (a pristine copy of `_RELEASE` + the
# binary), removed on exit.
#
# Display: run directly when `DISPLAY` / `WAYLAND_DISPLAY` is set; else
# under `xvfb-run -a -s "-screen 0 1280x720x24"` when `xvfb-run` exists;
# else skip-with-reason (exit 0) — a headless box with no Xvfb CANNOT run
# the windowed column (D-011 style: it is NOT run, not faked, not
# headless). CI installs xvfb + the Mesa software GL driver via
# tools/ci-deps.sh.
#
# `set -u` (NOT `-e`): we must capture the `timeout` exit code. 124 is
# `timeout`'s "the command was still running when it was killed" status —
# the expected healthy end (the client is in its frame loop), as opposed to
# the one-frame headless fallback that exits 0.

set -u
cd "$(dirname "$0")/.."

BIN="${1:-${CARGO_TARGET_DIR:-target}/debug/open-hexagon}"
[ -f "$BIN" ] || { echo "error: binary not found or not a file: $BIN" >&2; exit 1; }
BIN_NAME="$(basename "$BIN")"

TIMEOUT_A=20                                   # phase A: probe child (bounded so a hung display query cannot hang the gate)
TIMEOUT_B="${OH_WINDOWED_SMOKE_TIMEOUT:-20}"   # phase B: shipped-config boot
TIMEOUT_C=20                                   # phase C: auto-resolution boot (cold llvmpipe + the child probe)

# Print a failure message, then dump any named files (raw), then exit 1.
die() {
    echo "smoke-windowed FAIL: $1" >&2
    shift
    local f
    for f in "$@"; do
        if [ -n "${f:-}" ] && [ -f "$f" ]; then
            echo "----- $f (raw) -----" >&2
            cat "$f" >&2
        fi
    done
    exit 1
}

# ---------------------------------------------------------------------
# Display selection (module docs): direct / xvfb-run / skip-with-reason.
# ---------------------------------------------------------------------
if [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; then
    DISPLAY_CMD=()
elif command -v xvfb-run >/dev/null 2>&1; then
    DISPLAY_CMD=(xvfb-run -a -s "-screen 0 1280x720x24")
else
    echo "smoke-windowed SKIP: no display (DISPLAY/WAYLAND_DISPLAY unset) and no xvfb-run — the windowed column was NOT run (not faked, not headless). Install xvfb + libgl1-mesa-dri (tools/ci-deps.sh) or run on a display."
    exit 0
fi

# ---------------------------------------------------------------------
# The sandbox: a throwaway copy of the tracked _RELEASE + the binary.
# ---------------------------------------------------------------------
SANDBOX="$(mktemp -d "${TMPDIR:-/tmp}/oh-windowed-smoke.XXXXXX")"
# Guard: an unchecked mktemp under `set -u` (no -e) that fails would leave
# $SANDBOX empty and make the next `cp -R _RELEASE/. "$SANDBOX/"` target an
# empty path (i.e. copy into the CWD's parent — potentially `/`).
if [ -z "${SANDBOX:-}" ] || [ ! -d "$SANDBOX" ]; then
    echo "error: mktemp -d failed; refusing to run" >&2
    exit 1
fi
trap 'rm -rf "$SANDBOX"' EXIT

cp -R _RELEASE/. "$SANDBOX/"
cp "$BIN" "$SANDBOX/$BIN_NAME"
chmod +x "$SANDBOX/$BIN_NAME"

# Run the sandboxed binary under the display wrapper, stdout -> $1,
# stderr -> $2. $3 = timeout seconds (empty = run without a timeout).
# Remaining args are passed to the binary. The function's exit status is
# the (possibly wrapped) command's exit status (captured by the caller
# via $?).
run_sandbox() {
    local out="$1" err="$2" t="$3"
    shift 3
    (
        cd "$SANDBOX"
        if [ -n "$t" ]; then
            # `timeout` INSIDE the wrapper: it kills the game, and the
            # wrapper (xvfb-run) propagates the 124. With an empty
            # DISPLAY_CMD this is a plain `timeout $t ./bin`.
            "${DISPLAY_CMD[@]+"${DISPLAY_CMD[@]}"}" timeout "$t" "./$BIN_NAME" "$@"
        else
            "${DISPLAY_CMD[@]+"${DISPLAY_CMD[@]}"}" "./$BIN_NAME" "$@"
        fi
    ) >"$out" 2>"$err"
}

# The three windowed-boot assertions (phase B + C share them): a real GL
# context was created, the one-frame headless fallback did NOT happen, and
# the timeout killed the still-running client (exit 124).
check_boot() {
    local label="$1" out="$2" err="$3" rc="$4"
    local fails=()
    if ! grep -q '\[oh-render\] context GL_SAMPLES' "$err"; then
        fails+=("no '[oh-render] context GL_SAMPLES' line in stderr (no real GL context was created)")
    fi
    if grep -q '\[::mainClient\] Finished' "$out" || grep -q '\[::mainClient\] Finished' "$err"; then
        fails+=("the one-frame headless fallback '[::mainClient] Finished' appeared (the window degraded headless)")
    fi
    if [ "$rc" -ne 124 ]; then
        fails+=("expected the timeout kill (exit 124 = 'still running when killed'), got exit $rc")
    fi
    if [ "${#fails[@]}" -ne 0 ]; then
        local f
        for f in "${fails[@]}"; do
            echo "smoke-windowed: $label — $f" >&2
        done
        die "$label: windowed-boot assertions failed" "$out" "$err"
    fi
}

# ---------------------------------------------------------------------
# Phase A — the probe child: `--oh-desktop-mode-probe` must exit 0 and
# print a parseable `OH_DESKTOP_MODE=WxH` to stdout. BOTH dimensions must
# be > 0: the probe's `0x0` is its display-query FAILURE fallback (no
# usable display), so it must NOT pass as a valid mode.
# ---------------------------------------------------------------------
A_OUT="$SANDBOX/.smoke-a.out"
A_ERR="$SANDBOX/.smoke-a.err"
run_sandbox "$A_OUT" "$A_ERR" "$TIMEOUT_A" --oh-desktop-mode-probe
probe_rc=$?
[ "$probe_rc" -eq 0 ] || die "phase A (probe child): exit $probe_rc (expected 0)" "$A_OUT" "$A_ERR"
grep -Eq 'OH_DESKTOP_MODE=[1-9][0-9]*x[1-9][0-9]*' "$A_OUT" \
    || die "phase A (probe child): no parseable 'OH_DESKTOP_MODE=WxH' (W,H > 0; the 0x0 display-query-failure fallback must not pass) on stdout" "$A_OUT" "$A_ERR"
echo "phase A (probe child): OK — $(grep -Eo 'OH_DESKTOP_MODE=[1-9][0-9]*x[1-9][0-9]*' "$A_OUT" | head -n1)"

# ---------------------------------------------------------------------
# Phase B — the shipped config (both auto-resolution flags false) booted
# windowed: a real GL context up + still in the frame loop at the timeout.
# ---------------------------------------------------------------------
B_OUT="$SANDBOX/.smoke-b.out"
B_ERR="$SANDBOX/.smoke-b.err"
run_sandbox "$B_OUT" "$B_ERR" "$TIMEOUT_B"
b_rc=$?
check_boot "phase B (shipped-config windowed boot)" "$B_OUT" "$B_ERR" "$b_rc"
echo "phase B (shipped-config windowed boot): OK — GL context up, no Finished, killed by timeout (exit 124)"

# ---------------------------------------------------------------------
# Phase C — the auto-resolution path: turn both flags on in the sandbox
# config.json and boot again (shorter timeout). Exercises the
# parent -> probe-child -> reapply_resolution -> window path end-to-end.
# ---------------------------------------------------------------------
[ -f "$SANDBOX/config.json" ] || die "phase C (auto-resolution): sandbox config.json missing (the game needs it to exercise auto-resolution)"
sed -i -E \
    -e 's/"windowed_auto_resolution"[[:space:]]*:[[:space:]]*false/"windowed_auto_resolution" : true/' \
    -e 's/"fullscreen_auto_resolution"[[:space:]]*:[[:space:]]*false/"fullscreen_auto_resolution" : true/' \
    "$SANDBOX/config.json"
grep -Eq '"windowed_auto_resolution"[[:space:]]*:[[:space:]]*true' "$SANDBOX/config.json" \
    || die "phase C (auto-resolution): could not set windowed_auto_resolution=true in the sandbox config" "$SANDBOX/config.json"
grep -Eq '"fullscreen_auto_resolution"[[:space:]]*:[[:space:]]*true' "$SANDBOX/config.json" \
    || die "phase C (auto-resolution): could not set fullscreen_auto_resolution=true in the sandbox config" "$SANDBOX/config.json"

C_OUT="$SANDBOX/.smoke-c.out"
C_ERR="$SANDBOX/.smoke-c.err"
run_sandbox "$C_OUT" "$C_ERR" "$TIMEOUT_C"
c_rc=$?
check_boot "phase C (auto-resolution windowed boot)" "$C_OUT" "$C_ERR" "$c_rc"
echo "phase C (auto-resolution windowed boot): OK — GL context up, no Finished, killed by timeout (exit 124)"

echo "PASS: windowed smoke — phase A (probe child) + phase B (shipped-config windowed boot) + phase C (auto-resolution reapply) all green"
