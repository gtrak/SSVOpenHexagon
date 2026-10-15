#!/usr/bin/env bash
# Recovery for the tinyllb KV-admission deadlock.
#
# SYMPTOM: every request to the local model endpoint hangs forever (0 bytes, no
# tokens) while `sglang:num_running_reqs` is 0. Workers appear to "think" for
# 20+ minutes and die having made no tool calls.
#
# CAUSE (observed 2026-10-15): tinyllb's kv_bin_pack admission gate holds every
# request because `projected` (current_kv + ema_kv_per_flow) exceeds
# `max_threshold` (0.90). When no flows are active the EMA does not decay, so
# once it reaches >= 0.90 the gate never admits again — permanent deadlock.
#
# IMPORTANT — do not misuse this script:
#   * Restarting tinyllb ABORTS every in-flight request. Do NOT run it while a
#     subagent is working: you will kill that agent mid-request (this happened).
#   * A slow-but-healthy backend is NOT wedged. Healthy latency has been observed
#     at 35s; the probe timeout below is deliberately long for that reason.
#   * The book check is the LOG SIGNATURE, not the probe alone.
#
# Usage:
#   tools/infra/restart_lb_if_wedged.sh                 # report only (no restart)
#   tools/infra/restart_lb_if_wedged.sh --restart       # restart if wedged
set -u

ENDPOINT="${1:-http://gary-agents:1234}"
[ "${1:-}" = "--restart" ] && ENDPOINT="${2:-http://gary-agents:1234}"
DO_RESTART=0
[ "${1:-}" = "--restart" ] && DO_RESTART=1

PROBE_TIMEOUT="${PROBE_TIMEOUT:-90}"

# 1) Is there any in-flight agent work? If so, never restart.
if pgrep -f 'pi-subagents' >/dev/null 2>&1 || pgrep -x -c node >/dev/null 2>&1; then
    IN_FLIGHT=1
else
    IN_FLIGHT=0
fi

# 2) Probe latency.
if curl -sS -m "$PROBE_TIMEOUT" -o /dev/null \
     -H 'Content-Type: application/json' \
     -d '{"model":"local","messages":[{"role":"user","content":"hi"}],"max_tokens":1}' \
     "$ENDPOINT/v1/chat/completions" 2>/dev/null; then
    echo "healthy: endpoint answered within ${PROBE_TIMEOUT}s"
    exit 0
fi

# 3) Probe failed. Confirm the DEADLOCK SIGNATURE in the log before acting.
SIG="$(journalctl --user -u tinyllb.service --since '2 min ago' --no-pager 2>/dev/null \
        | grep -oE 'current_kv=[0-9.]+ ema_kv_per_flow=[0-9.]+ projected=[0-9.]+ max_threshold=[0-9.]+ admit=false' \
        | tail -1)"
KV0="$(echo "$SIG" | grep -oE 'current_kv=0\.0' || true)"

echo "probe failed after ${PROBE_TIMEOUT}s"
if [ -z "$SIG" ] || [ -z "$KV0" ]; then
    echo "NOT the known deadlock (no 'admit=false with current_kv=0.0' signature)."
    echo "Last admission line: ${SIG:-<none>}"
    echo "Treat as slow/other cause. NOT restarting."
    exit 2
fi
echo "DEADLOCK SIGNATURE: $SIG"

if [ "$DO_RESTART" -ne 1 ]; then
    echo "Report-only mode. Re-run with --restart to recover."
    exit 3
fi

if [ "$IN_FLIGHT" -ne 0 ]; then
    echo "REFUSING to restart: agent work appears in flight (a restart aborts it)."
    exit 4
fi

echo "restarting tinyllb.service"
systemctl --user restart tinyllb.service
sleep 5
systemctl --user is-active tinyllb.service
