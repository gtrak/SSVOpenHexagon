#!/usr/bin/env bash
# tools/infra/emulator-start.sh — the BOUNDED headless Android emulator
# launcher (issue 25, the 2026-10-16 OOM postmortem).
#
# What happened: the emulator (qemu, guest `-memory 3072`) ballooned to
# 28.8 GB anon RSS — the gfxstream/SwiftShader software-GL backend leaks
# host-side buffer memory under the game's per-frame re-uploads — filled
# all 16 G of swap (the box froze), and the kernel OOM-killed it
# (dmesg 2026-10-16 12:58:54, the ONLY oom-kill in the window).
#
# The bound: run qemu inside a user-scope cgroup with MemoryMax — a leak
# is contained to the emulator (it gets killed alone) instead of starving
# the whole box (rustc + lanes + the sglang server live here too).
#
# Usage: tools/infra/emulator-start.sh [avd]   (default: oh25)
#   idempotent: kills any prior emulator first; waits for boot.
set -euo pipefail

AVD="${1:-oh25}"
MEM="${OH_EMU_MEM:-3072}"
# The leak bound: guest RAM + SwiftShader working set + headroom; the
# balloon dies here instead of triggering the global OOM killer.
MAX_MEM="${OH_EMU_MAX_MEM:-8G}"
MAX_SWAP="${OH_EMU_MAX_SWAP:-2G}"

ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
ADB="$ANDROID_HOME/platform-tools/adb"

# Idempotent: one emulator, no orphans.
pkill -f "qemu-system.*-avd $AVD" 2>/dev/null && sleep 3 || true

# sg kvm: the group membership applies to new sessions only (usermod -aG
# without re-login); systemd-run --user --scope keeps the cgroup bound.
nohup sg kvm -c "systemd-run --user --scope -p MemoryMax=$MAX_MEM -p MemorySwapMax=$MAX_SWAP $ANDROID_HOME/emulator/emulator -avd $AVD -no-window -no-audio -no-boot-anim -gpu swiftshader_indirect -memory $MEM -no-snapshot > /tmp/emulator-oh.log 2>&1" >/dev/null 2>&1 &

echo "waiting for the emulator to boot (bounded at MemoryMax=$MAX_MEM)..."
for i in $(seq 1 60); do
    sleep 5
    BOOT="$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')"
    if [ "$BOOT" = "1" ]; then
        echo "booted: $($ADB devices | grep emulator | head -1)"
        exit 0
    fi
done
echo "ERROR: emulator did not boot in 300s — /tmp/emulator-oh.log" >&2
exit 1
