#!/usr/bin/env bash
set -euo pipefail

# Inputx — Maestro test driver.
#
# Build (sim, Debug by default) + install Inputx onto the simulator named in
# $SIMULATOR_DEVICE (default "sim-inputx"), then run one or all Maestro
# flows under ios/maestro/flows/.
#
# Usage:
#   ./maestro_test.sh                  # build + run all flows in flows/
#   ./maestro_test.sh 01-smoke         # build + run a single flow (extension optional)
#   ./maestro_test.sh --no-build       # skip build/install, run all flows
#   ./maestro_test.sh --no-build 01-smoke
#
# Env vars:
#   SIMULATOR_DEVICE — sim name, default "sim-inputx"
#   IOS_CONFIG       — Debug | Release (default Debug for sim)

cd "$(dirname "$0")"

# Mirror build_sim.sh's default — keep the two scripts in sync so the same
# sim gets built into AND run against. Override via SIMULATOR_DEVICE.
DEVICE="${SIMULATOR_DEVICE:-sim-inputx}"

MAESTRO="${MAESTRO:-$HOME/.maestro/bin/maestro}"
[ -x "$MAESTRO" ] || { echo "maestro not found at $MAESTRO"; exit 1; }

DO_BUILD=1
FLOW=""
for arg in "$@"; do
    case "$arg" in
        --no-build) DO_BUILD=0 ;;
        -*)         echo "unknown flag: $arg"; exit 1 ;;
        *)          FLOW="$arg" ;;
    esac
done

if [ "$DO_BUILD" = "1" ]; then
    echo "[maestro] build + install (sim)"
    ./build_sim.sh
fi

FLOWS_DIR="maestro/flows"

# Resolve the iOS sim UDID by exact name. simctl's positional search arg
# does substring matching on a single word — multi-word names like "Inputx
# iPhone 17" return an empty list. Easier to list everything and filter
# in Python by exact name.
#
# Without an explicit --udid Maestro auto-picks the first booted device,
# which can be (a) some other booted sim, or (b) a connected Android
# emulator if one's around — both lead to confusing failures.
SIM_UDID=$(xcrun simctl list devices --json 2>/dev/null \
    | /usr/bin/python3 -c '
import json, sys
data = json.load(sys.stdin)
name = sys.argv[1] if len(sys.argv) > 1 else None
for runtime, devs in data["devices"].items():
    for d in devs:
        if d.get("isAvailable") and (name is None or d.get("name") == name):
            print(d["udid"]); sys.exit(0)
sys.exit(1)
' "$DEVICE" || true)

MAESTRO_FLAGS=(--platform ios)
if [ -n "$SIM_UDID" ]; then
    MAESTRO_FLAGS+=(--udid "$SIM_UDID")
    # Belt-and-suspenders — maestro 2.x sometimes routes to the wrong sim
    # despite --udid when multiple sims are booted (the xcodebuild child it
    # spawns picks up Simulator.app's CurrentDeviceUDID default instead).
    # Pin it explicitly so the destination always matches our intent.
    defaults write com.apple.iphonesimulator CurrentDeviceUDID "$SIM_UDID" 2>/dev/null || true
fi

# Kill any stale maestro-driver-ios / xcodebuild-test-without-building
# left from a previous run. On iOS 26 the XCUITest driver can latch onto
# whichever app was frontmost when it last attached — so if the sim's
# previous frontmost app wasn't inputx, every viewHierarchy call returns
# that app's hierarchy and our asserts fail with the wrong-app screenshot.
# Fresh driver process re-attaches to the actual frontmost (inputx after
# reset_app_group_state pre-warms it).
pkill -9 -f maestro-driver-ios 2>/dev/null || true
pkill -9 -f "xcodebuild test-without-building" 2>/dev/null || true
sleep 1

# Reset App Group state back to first-install defaults. Some flows leak
# state (autoCommitPolicy after 83, useCjkPunct after 77, engineMode if a
# Picker flip didn't restore properly, recent emojis after 82, etc.); when
# the next flow assumes default state it silently fails (e.g. CJK comma
# turns into ASCII comma, "Never" policy suppresses auto-commit).
#
# Maestro 2.x's `test FLOWS_DIR` invocation runs all flows in one JVM
# without between-flow hooks, so we drive the reset from the shell here
# and run flows one-at-a-time when sweeping the whole directory.
reset_app_group_state() {
    # Reset is two-part:
    #   (1) The App Group preferences (engineMode / useCjkPunct / etc.) —
    #       these go through iOS's cfprefsd which caches reads in-process.
    #       Direct PlistBuddy edits do NOT invalidate that cache, so we
    #       drive reset via a hidden launch arg: terminate the app, launch
    #       it with `-inputx-reset-defaults`, let AppDelegate write the
    #       defaults through UserDefaults (which DOES update cfprefsd's
    #       cache), then terminate again so maestro launches it fresh.
    #   (2) L0 learning JSON files — plain filesystem state, no cache layer,
    #       just rm them.
    local app_id="jp.golia.inputx"
    local group_id="group.jp.golia.inputx"
    local group_path
    group_path=$(xcrun simctl get_app_container "$SIM_UDID" "$app_id" groups 2>/dev/null \
        | awk -v g="$group_id" '$1==g {print $2}')
    if [ -z "$group_path" ]; then
        return 0  # app not installed yet — nothing to reset
    fi
    # Wipe L0 learning files first so any in-flight L0 import on the next
    # app launch sees no prior state.
    rm -f "$group_path/Library/Application Support/inputx/wubi_l0.json"   2>/dev/null || true
    rm -f "$group_path/Library/Application Support/inputx/pinyin_l0.json" 2>/dev/null || true
    # cfprefsd-respecting defaults reset via the hidden launch arg.
    xcrun simctl terminate "$SIM_UDID" "$app_id" 2>/dev/null || true
    xcrun simctl launch    "$SIM_UDID" "$app_id" -inputx-reset-defaults >/dev/null 2>&1 || true
    sleep 0.5  # let AppDelegate's didFinishLaunching run + synchronize()
    xcrun simctl terminate "$SIM_UDID" "$app_id" 2>/dev/null || true
    # Re-launch (without reset arg) so inputx is the frontmost app when
    # maestro takes over. On iOS 26 maestro's launchApp can't reliably
    # bring a backgrounded app to foreground when SpringBoard's previous
    # frontmost was another third-party app — pre-warming foreground
    # here makes maestro's first screenshot actually catch inputx UI.
    xcrun simctl launch "$SIM_UDID" "$app_id" >/dev/null 2>&1 || true
    sleep 0.3
}

reset_app_group_state

if [ -z "$FLOW" ]; then
    echo "[maestro] running all flows in $FLOWS_DIR (udid=$SIM_UDID)"
    # Run flows one-at-a-time, resetting App Group state between each so
    # flow N+1 never inherits flow N's leaked settings.
    PASS=0; FAIL=0
    FAIL_LIST=()
    for FLOW_FILE in "$FLOWS_DIR"/*.yaml; do
        reset_app_group_state
        echo ""
        echo "[maestro] >>> $(basename "$FLOW_FILE")"
        if "$MAESTRO" "${MAESTRO_FLAGS[@]}" test "$FLOW_FILE"; then
            PASS=$((PASS+1))
        else
            FAIL=$((FAIL+1))
            FAIL_LIST+=("$(basename "$FLOW_FILE")")
        fi
    done
    echo ""
    echo "[maestro] === summary: $PASS passed, $FAIL failed ==="
    if [ "$FAIL" -gt 0 ]; then
        for f in "${FAIL_LIST[@]}"; do
            echo "[maestro]   FAIL: $f"
        done
    fi
    [ "$FAIL" = 0 ]
else
    # Accept either bare name (01-smoke) or full path / full filename.
    if [ -f "$FLOWS_DIR/$FLOW.yaml" ]; then
        TARGET="$FLOWS_DIR/$FLOW.yaml"
    elif [ -f "$FLOWS_DIR/$FLOW" ]; then
        TARGET="$FLOWS_DIR/$FLOW"
    elif [ -f "$FLOW" ]; then
        TARGET="$FLOW"
    else
        echo "flow not found: $FLOW (looked in $FLOWS_DIR)"
        exit 1
    fi
    echo "[maestro] running $TARGET (udid=$SIM_UDID)"
    "$MAESTRO" "${MAESTRO_FLAGS[@]}" test "$TARGET"
fi
