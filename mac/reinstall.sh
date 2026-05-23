#!/usr/bin/env bash
# Silent re-install + LaunchAgent re-bootstrap + (if needed) re-enable
# the input source. Use after rebuild, OR after the user accidentally
# clicks something in System Settings that removes Inputx from the
# active input list.
#
# What's silent:
#   * No GUI dialogs.
#   * Only essential one-line confirmations to stdout.
#   * If the bundle is already enabled, no re-enable step (idempotent).
#
# What can't be made silent (macOS limitation):
#   * The very first install (or a true Remove→Add) requires the user
#     to confirm the addition through System Settings → 文本输入. Once
#     done, TCC remembers and subsequent re-adds via this script bypass
#     the prompt (see `scripts/reenable.sh` for the mechanism).

set -uo pipefail
cd "$(dirname "$0")"

# 1. Build if requested (skip when --no-build given).
if [ "${1:-}" != "--no-build" ]; then
  sccache --stop-server >/dev/null 2>&1 || true
  RUSTC_WRAPPER= ./build.sh 2>&1 | grep '\[build\]' | tail -3
fi

# 2. Stop any running instance + the LaunchAgent.
launchctl bootout "gui/$UID/jp.golia.inputmethod.wubi" >/dev/null 2>&1 || true
pkill -9 -f "Inputx.app/Contents/MacOS/Inputx" >/dev/null 2>&1 || true
sleep 1

# 3. Copy the built bundle into ~/Library/Input Methods/.
./install.sh 2>&1 | grep '\[install\]'

# 4. Bootstrap the LaunchAgent — re-starts the binary cleanly.
#    Fully silent: `bootstrap` is noisy on re-load ("Bootstrap failed: 5:
#    Input/output error" when the plist is still considered loaded by the
#    domain). Step 5 below is the source of truth for "did it actually
#    start", so swallow all bootstrap output here; the user only sees
#    the green/red verification line.
launchctl bootstrap "gui/$UID" "$HOME/Library/LaunchAgents/jp.golia.inputmethod.wubi.plist" \
  >/dev/null 2>&1 || true
sleep 1

# 5. Verify it's running.
if pgrep -f "Inputx.app/Contents/MacOS/Inputx" >/dev/null; then
  PID=$(pgrep -f "Inputx.app/Contents/MacOS/Inputx" | head -1)
  echo "[reinstall] ✓ Inputx running (PID $PID)"
else
  echo "[reinstall] ✗ Inputx not started — check 'log show --predicate \"process == \\\"Inputx\\\"\"'"
  exit 1
fi

# 6. Verify input source is enabled. If not, programmatic re-add
#    (works because TCC remembers prior approval).
ENABLED=$(defaults read com.apple.HIToolbox AppleEnabledInputSources 2>/dev/null \
           | grep -c "jp.golia.inputmethod.wubi" || true)
if [ "$ENABLED" -gt 0 ]; then
  echo "[reinstall] ✓ enabled in input source list"
else
  echo "[reinstall] input source missing — running reenable.sh"
  ./scripts/reenable.sh 2>&1 | tail -5
fi
