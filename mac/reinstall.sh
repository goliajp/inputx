#!/usr/bin/env bash
# Silent re-install + LaunchAgent re-bootstrap + (if needed) re-enable
# the input source. Use after rebuild, OR after the user accidentally
# clicks something in System Settings that removes Inputx from the
# active input list.
#
# What's silent:
#   * No GUI dialogs.
#   * Build / install / bootstrap output is captured to a temp log and
#     NOT shown — successful runs only print two `[reinstall] ✓` lines.
#   * On failure the captured log is dumped to stderr so it's debuggable.
#   * If the bundle is already enabled, no re-enable step (idempotent).
#
# What can't be made silent (macOS limitation):
#   * The very first install (or a true Remove→Add) requires the user
#     to confirm the addition through System Settings → 文本输入. Once
#     done, TCC remembers and subsequent re-adds via this script bypass
#     the prompt (see `scripts/reenable.sh` for the mechanism).

set -uo pipefail
cd "$(dirname "$0")"

# Capture build + install + bootstrap chatter here; only dump on failure.
LOG=$(mktemp -t inputx-reinstall.XXXXXX)
trap 'rm -f "$LOG"' EXIT

# Surface the captured log when a step fails, so the user has something
# to grep instead of "command failed silently."
fail() {
  echo "[reinstall] ✗ $1" >&2
  echo "--- captured output ---" >&2
  cat "$LOG" >&2
  exit 1
}

# 1. Build if requested (skip when --no-build given).
if [ "${1:-}" != "--no-build" ]; then
  sccache --stop-server >/dev/null 2>&1 || true
  if ! RUSTC_WRAPPER= ./build.sh >>"$LOG" 2>&1; then
    fail "build failed"
  fi
fi

# 2. Stop any running instance + the LaunchAgent.
launchctl bootout "gui/$UID/jp.golia.inputmethod.wubi" >>"$LOG" 2>&1 || true
pkill -9 -f "Inputx.app/Contents/MacOS/Inputx" >>"$LOG" 2>&1 || true
sleep 1

# 3. Copy the built bundle into ~/Library/Input Methods/.
if ! ./install.sh >>"$LOG" 2>&1; then
  fail "install failed"
fi

# 4. Bootstrap the LaunchAgent — re-starts the binary cleanly.
#    Always silent: `bootstrap` is noisy on re-load ("Bootstrap failed:
#    5: Input/output error" when the plist is still considered loaded by
#    the domain). Step 5 below is the source of truth for "did it
#    actually start"; we don't propagate bootstrap's exit code.
launchctl bootstrap "gui/$UID" "$HOME/Library/LaunchAgents/jp.golia.inputmethod.wubi.plist" \
  >>"$LOG" 2>&1 || true
sleep 1

# 5. Verify it's running.
if pgrep -f "Inputx.app/Contents/MacOS/Inputx" >/dev/null; then
  PID=$(pgrep -f "Inputx.app/Contents/MacOS/Inputx" | head -1)
  echo "[reinstall] ✓ Inputx running (PID $PID)"
else
  fail "Inputx not started — also check 'log show --predicate \"process == \\\"Inputx\\\"\"'"
fi

# 6. Verify input source is enabled. If not, programmatic re-add
#    (works because TCC remembers prior approval).
ENABLED=$(defaults read com.apple.HIToolbox AppleEnabledInputSources 2>/dev/null \
           | grep -c "jp.golia.inputmethod.wubi" || true)
if [ "$ENABLED" -gt 0 ]; then
  echo "[reinstall] ✓ enabled in input source list"
else
  if ! ./scripts/reenable.sh >>"$LOG" 2>&1; then
    fail "input source missing AND reenable.sh failed"
  fi
  echo "[reinstall] ✓ re-added to input source list"
fi
