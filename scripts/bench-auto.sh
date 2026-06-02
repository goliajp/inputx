#!/usr/bin/env bash
# Fully automated perf+mem benchmark for Inputx.
#
# Drives Inputx with a fixed deterministic corpus via CGEventPost
# (see scripts/bench-auto.swift) into a fresh TextEdit document.
# Captures PerfTimer + vmmap + heap snapshots before and after the
# corpus. Outputs structured JSON to stdout (and a labeled file
# under bench/results/ when --save <label> is passed) so multiple
# runs across commits can be compared apples-to-apples.
#
# Pre-conditions:
#   - Inputx is installed + LaunchAgent-managed (mac/reinstall.py)
#   - Inputx is enabled in System Settings → Keyboard → Input Sources
#   - Terminal / IDE running this script has accessibility permission
#     (System Settings → Privacy & Security → Accessibility)
#     First CGEventPost call will prompt if missing.
#
# Usage:
#   scripts/bench-auto.sh                     # one-shot, prints JSON to stdout
#   scripts/bench-auto.sh --save baseline     # saves JSON to bench/results/<HEAD>-baseline.json
#   scripts/bench-auto.sh --runs 3            # 3 back-to-back runs, prints array
#   scripts/bench-auto.sh --runs 3 --save mywin    # 3 runs, saved separately

set -euo pipefail
cd "$(dirname "$0")/.."

RUNS=1
LABEL=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --runs) RUNS="$2"; shift 2 ;;
        --save) LABEL="$2"; shift 2 ;;
        *)      echo "[bench-auto] unknown arg: $1" >&2; exit 2 ;;
    esac
done

# ── Compile the Swift driver (fast — recompiles only on source change)
SWIFT_BIN="/tmp/inputx-bench-auto"
SWIFT_SRC="scripts/bench-auto.swift"
if [[ ! -x "$SWIFT_BIN" ]] || [[ "$SWIFT_SRC" -nt "$SWIFT_BIN" ]]; then
    echo "[bench-auto] compiling driver..." >&2
    swiftc -O "$SWIFT_SRC" -o "$SWIFT_BIN" \
        -framework Carbon -framework Cocoa
fi

# ── Verify Inputx is in good state for measurement
HEAD_SHA=$(git rev-parse --short HEAD)
LOG=/private/tmp/inputx.err.log

verify_inputx_healthy() {
    local pid
    pid=$(pgrep -x Inputx | head -1 || true)
    [[ -z "$pid" ]] && return 1
    # Exactly one Inputx (no duplicates from imklaunchagent ghost-launching)
    local count
    count=$(pgrep -x Inputx | wc -l | tr -d ' ')
    (( count == 1 )) || { echo "[bench-auto]   $count Inputx processes (need exactly 1)" >&2; return 1; }
    # stderr bound to log file (not /dev/null)
    local stderr_target
    stderr_target=$(lsof -p "$pid" 2>/dev/null | awk '$4 == "2u" {print $NF}')
    [[ "$stderr_target" == "/private/tmp/inputx.err.log" ]] || { echo "[bench-auto]   stderr=$stderr_target (need /private/tmp/inputx.err.log)" >&2; return 1; }
    # No Mach service registration failure since last log clear
    local mach_err
    mach_err=$(grep "could not register" "$LOG" 2>/dev/null | wc -l | tr -d ' ')
    (( mach_err == 0 )) || { echo "[bench-auto]   Mach service registration failed ($mach_err errors in log)" >&2; return 1; }
    echo "$pid"
    return 0
}

# Try a clean state up to 3 times: kill everything Inputx + reinstall + verify
PID=""
for attempt in 1 2 3; do
    echo "[bench-auto] attempt $attempt to bring up clean Inputx..." >&2
    # Kill ALL Inputx (including imklaunchagent ghosts)
    pkill -x Inputx 2>/dev/null || true
    sleep 1
    # Force-clear log so old registration errors don't false-positive verify
    : > "$LOG"
    # mac/reinstall.py is the single canonical install entry point —
    # auto-detects first vs reinstall, runs with safety net (backup +
    # 5s health window + rollback). No `|| true` fallbacks; failures
    # surface via non-zero exit and the captured log.
    (cd mac && ./reinstall.py > /tmp/bench-reinstall.log 2>&1) || {
        echo "[bench-auto]   reinstall failed; see /tmp/bench-reinstall.log" >&2
        continue
    }
    sleep 2  # Give Mach service registration a moment
    PID=$(verify_inputx_healthy) && break || {
        echo "[bench-auto]   verify failed, retrying..." >&2
        PID=""
    }
done
if [[ -z "$PID" ]]; then
    echo "[bench-auto] FATAL: couldn't bring up healthy Inputx after 3 attempts" >&2
    exit 1
fi
echo "[bench-auto] healthy Inputx PID=$PID" >&2

# ── Per-run measurement
LOG=/private/tmp/inputx.err.log

run_once() {
    local run_n="$1"
    : > "$LOG"
    # `vmmap -summary` output: "Physical footprint:         15.0M"
    # Extract just the number+unit, convert M→raw MB.
    parse_phys() {
        local prefix="$1"
        vmmap -summary "$PID" 2>/dev/null | \
            grep "^$prefix:" | head -1 | \
            grep -oE "[0-9]+\.?[0-9]*[KMG]" | head -1 | \
            awk '{
                v = $0
                gsub(/[KMG]$/,"",v)
                if (match($0,/K/)) print v/1024
                else if (match($0,/G/)) print v*1024
                else print v
            }'
    }

    local BEFORE_PHYS=$(parse_phys "Physical footprint")

    # Drive the typing
    "$SWIFT_BIN" 2> >(while read line; do echo "  [drv] $line" >&2; done)

    # Snapshot after
    local AFTER_PHYS=$(parse_phys "Physical footprint")
    local AFTER_PEAK=$(vmmap -summary "$PID" 2>/dev/null | grep "Physical footprint (peak)" | grep -oE "[0-9]+\.?[0-9]*[KMG]" | head -1 | awk '{v=$0; gsub(/[KMG]$/,"",v); if (match($0,/K/)) print v/1024; else if (match($0,/G/)) print v*1024; else print v}')
    local AFTER_HEAP_BYTES=$(heap "$PID" 2>/dev/null | grep -E "All zones: [0-9]+ nodes \([0-9]+ bytes\)" | head -1 | grep -oE "\([0-9]+ bytes\)" | grep -oE "[0-9]+")
    local AFTER_NONOBJECT_COUNT=$(heap "$PID" 2>/dev/null | awk '/[ 	]non-object[ 	]/ {print $1; exit}')
    local AFTER_NONOBJECT_BYTES=$(heap "$PID" 2>/dev/null | awk '/[ 	]non-object[ 	]/ {print $2; exit}')

    # Median of PerfTimer p50/p95 across all flushes for the key labels
    median_field() {
        local label="$1"
        local field="$2"  # "p50" or "p95"
        grep "^\[perf\] $label:" "$LOG" | \
            grep -oE "$field=[0-9]+\.[0-9]+" | cut -d= -f2 | \
            sort -n | awk '{a[NR]=$1} END {if (NR==0) {print "null"; exit}; print a[int((NR+1)/2)]}'
    }

    cat <<EOF
{
  "run": $run_n,
  "head": "$HEAD_SHA",
  "phys_before_mb": $BEFORE_PHYS,
  "phys_after_mb": $AFTER_PHYS,
  "phys_peak_mb": $AFTER_PEAK,
  "heap_bytes_after": $AFTER_HEAP_BYTES,
  "non_object_count": $AFTER_NONOBJECT_COUNT,
  "non_object_bytes": $AFTER_NONOBJECT_BYTES,
  "handle_p50_ms": $(median_field "IMEController.handle" "p50"),
  "handle_p95_ms": $(median_field "IMEController.handle" "p95"),
  "refresh_p50_ms": $(median_field "CandidatePanel.refresh" "p50"),
  "refresh_p95_ms": $(median_field "CandidatePanel.refresh" "p95"),
  "rebuildRows_p50_ms": $(median_field "CandidatePanel.rebuildRows" "p50"),
  "rebuildRows_p95_ms": $(median_field "CandidatePanel.rebuildRows" "p95"),
  "imk_dispatch_p50_ms": $(median_field "IMK.dispatch" "p50"),
  "imk_dispatch_p95_ms": $(median_field "IMK.dispatch" "p95")
}
EOF
}

# ── Run N times
{
    if (( RUNS > 1 )); then
        echo "["
        for i in $(seq 1 "$RUNS"); do
            run_once "$i"
            if (( i < RUNS )); then echo ","; fi
        done
        echo "]"
    else
        run_once 1
    fi
} > /tmp/bench-auto-output.json
cat /tmp/bench-auto-output.json
if [[ -n "$LABEL" ]]; then
    mkdir -p bench/results
    OUT="bench/results/${HEAD_SHA}-${LABEL}.json"
    cp /tmp/bench-auto-output.json "$OUT"
    echo "[bench-auto] saved → $OUT" >&2
fi
