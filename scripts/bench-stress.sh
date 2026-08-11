#!/usr/bin/env bash
# Stress-test typing latency: spawn CPU pressure, ask the operator to
# type for a fixed window, then dump PerfTimer numbers captured during
# the pressure interval. Used to verify Inputx doesn't degrade when
# other apps are competing for CPU (user 2026-05-31: "完全有可能设备
# 的性能压力一上来,输入法先卡住了").
#
# Compares against `scripts/bench-typing.sh` (no-stress baseline). A
# good IME should keep p50 within ~2× of the uncontended baseline and
# never blow past one frame (16 ms) at p95.
#
# Usage:
#   scripts/bench-stress.sh           # 8 CPU stressors × 30 s
#   scripts/bench-stress.sh 4 60      # 4 stressors × 60 s

set -euo pipefail
cd "$(dirname "$0")/.."

NUM_STRESSORS="${1:-8}"
DURATION_SEC="${2:-30}"

PID=$(pgrep -x Inputx | head -1)
if [[ -z "$PID" ]]; then
    echo "[stress] ✗ no Inputx process — launch via ./mac/reinstall.sh first" >&2
    exit 1
fi

LOGFILE="/private/tmp/inputx.err.log"
: > "$LOGFILE"

# Spawn N background CPU hogs (`yes` pegs a core).
echo "[stress] spawning $NUM_STRESSORS CPU stressors..."
STRESSOR_PIDS=()
for _ in $(seq 1 "$NUM_STRESSORS"); do
    yes > /dev/null &
    STRESSOR_PIDS+=("$!")
done

cleanup() {
    echo "[stress] killing stressors..."
    for pid in "${STRESSOR_PIDS[@]}"; do
        kill -9 "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo
cat <<EOF
[stress] $NUM_STRESSORS CPU stressors will run for ${DURATION_SEC} s.
[stress] During that window, type continuously in a Chinese text field
[stress] (e.g., Notes / TextEdit / Safari URL bar) with Inputx active.
[stress] Aim for ≥ 100 keystrokes to fill the PerfTimer windows.
[stress] When the timer ends, stop typing and we'll dump the numbers.
EOF

# Visible 1-Hz countdown so the operator knows how long they have left.
for i in $(seq "$DURATION_SEC" -1 1); do
    printf "\r[stress] %2d s remaining ... " "$i"
    sleep 1
done
printf "\r[stress] stress window ended.       \n"

cleanup
trap - EXIT INT TERM

echo
echo "[stress] === latency captured during stress ==="
PERF_LINES=$(grep "^\[perf\]" "$LOGFILE" | grep -v widthFitHit || true)
if [[ -z "$PERF_LINES" ]]; then
    echo "[stress] ✗ no [perf] lines — did you actually type?" >&2
    exit 1
fi
echo "$PERF_LINES"

echo
echo "[stress] === post-stress process snapshot ==="
ps -p "$PID" -o pid,rss,%cpu,etime,command 2>&1
echo
echo "[stress] vmmap phys_footprint:"
vmmap -summary "$PID" 2>/dev/null | grep -E "Physical footprint" | head -2
