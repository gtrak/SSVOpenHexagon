#!/usr/bin/env bash
# Recovery for the tinyllb KV-admission deadlock.
#
# SYMPTOM: every request to the local model endpoint hangs forever
# (0 bytes, no tokens) while `sglang:num_running_reqs` is 0. Workers appear to
# "think" for 20+ minutes and die having made no tool calls.
#
# CAUSE (observed 3x on 2026-10-15): tinyllb's kv_bin_pack admission gate holds
# every request because `projected` (current_kv + ema_kv_per_flow) exceeds
# `max_threshold` (0.90). When no flows are active the EMA does not decay, so
# once it reaches >= 0.90 the gate never admits again — a permanent deadlock.
# Log signature:
#   tinyllb::scheduler: kv bin-pack: holding — projected over threshold
#   current_kv=0.0 ema_kv_per_flow=0.9059... projected=0.9059... admit=false
#
# FIX: restart tinyllb (resets the EMA). This script only restarts a service
# that is provably wedged; it does not change any configuration.
set -u

ENDPOINT="${1:-http://gary-agents:1234}"
PROBE_TIMEOUT=12

if curl -sS -m "$PROBE_TIMEOUT" -o /dev/null \
     -H 'Content-Type: application/json' \
     -d '{"model":"local","messages":[{"role":"user","content":"hi"}],"max_tokens":1}' \
     "$ENDPOINT/v1/chat/completions" 2>/dev/null; then
    echo "healthy: endpoint answered within ${PROBE_TIMEOUT}s"
    exit 0
fi

echo "WEDGED: no response in ${PROBE_TIMEOUT}s — restarting tinyllb"
systemctl --user restart tinyllb.service
sleep 5
systemctl --user is-active tinyllb.service
