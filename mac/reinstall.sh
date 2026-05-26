#!/usr/bin/env bash
# Silent re-install: rebuild → bundle swap → LaunchAgent re-bootstrap →
# reenable. Does NOT call `install.sh` (which runs `Inputx install` →
# TISRegisterInputSource + TISEnableInputSource → macOS treats it as a
# fresh IME registration and re-prompts TCC "允许 'Inputx' 启用..."
# every single time).
#
# Bundle id stays jp.golia.inputmethod.wubi across rebuilds, so TIS
# already knows the bundle and TCC already has the permission grant.
# All we need on re-install:
#   - swap the .app at ~/Library/Input Methods/Inputx.app
#   - replace the LaunchAgent plist + re-bootstrap
#   - re-add the wubi mode to AppleEnabledInputSources if it fell out
#     (via scripts/reenable.sh — defaults-write path, no TCC popup)
#
# What's silent:
#   * No GUI dialogs.
#   * Build / install / bootstrap output captured to a temp log and NOT
#     shown — successful runs only print two `[reinstall] ✓` lines.
#   * On failure the captured log is dumped to stderr for debugging.
#
# What can't be made silent (macOS limitation):
#   * The very FIRST install (before TCC has ever seen this bundle id)
#     must go through `./install.sh` → System Settings → 文本输入 →
#     Add → approve the third-party IME popup. Once TCC has remembered,
#     `reinstall.sh` is silent forever.

set -uo pipefail
cd "$(dirname "$0")"

LOG=$(mktemp -t inputx-reinstall.XXXXXX)
trap 'rm -f "$LOG"' EXIT

fail() {
  echo "[reinstall] ✗ $1" >&2
  echo "--- captured output ---" >&2
  cat "$LOG" >&2
  exit 1
}

APP_NAME="Inputx"
APP_SRC="$(cd .. && pwd)/build/$APP_NAME.app"
APP_DST="$HOME/Library/Input Methods/$APP_NAME.app"
LA_DST="$HOME/Library/LaunchAgents/jp.golia.inputmethod.wubi.plist"

# 1. Build if requested.
if [ "${1:-}" != "--no-build" ]; then
  sccache --stop-server >>"$LOG" 2>&1 || true
  if ! RUSTC_WRAPPER= ./build.sh >>"$LOG" 2>&1; then
    fail "build failed"
  fi
fi

if [ ! -d "$APP_SRC" ]; then
  fail "no build at $APP_SRC — run with build (no --no-build) first"
fi

# 2. Stop the running binary + the LaunchAgent.
launchctl bootout "gui/$UID/jp.golia.inputmethod.wubi" >>"$LOG" 2>&1 || true
pkill -9 -f "Inputx.app/Contents/MacOS/Inputx" >>"$LOG" 2>&1 || true
sleep 1

# 3. Swap bundle in place. NO `Inputx install` (which would re-trigger
#    TCC popup via TISRegisterInputSource / TISEnableInputSource).
mkdir -p "$(dirname "$APP_DST")"
if [ -d "$APP_DST" ]; then
  if [ -w "$APP_DST" ] && [ -w "$APP_DST/Contents" ]; then
    if ! rm -rf "$APP_DST" >>"$LOG" 2>&1; then
      fail "rm old bundle"
    fi
  else
    if ! osascript -e "do shell script \"rm -rf '$APP_DST'\" with administrator privileges" >>"$LOG" 2>&1; then
      fail "rm root-owned old bundle"
    fi
  fi
fi
if ! cp -R "$APP_SRC" "$APP_DST" >>"$LOG" 2>&1; then
  fail "cp new bundle"
fi

# 3a. Reset L0 user-learning state (wubi + pinyin pins / pick_counts).
#     User directive 2026-05-26: "每次更新都要清理掉 l0 pin，不然我不知道
#     你有没有做". L0 pins multiply candidate score by PRIOR_L0_PIN_MULT
#     (= 1000×) which dwarfs corpus / prior_correction work — leaving them
#     in place across reinstalls makes polish-log verification impossible
#     (a previously-pinned candidate stays #1 regardless of any engine
#     change). polish-log.jsonl is preserved (it's a history record, not
#     a ranking-influencing pin store).
L0_DIR="$HOME/Library/Containers/jp.golia.inputmethod.wubi/Data/Library/Application Support/Inputx"
rm -f "$L0_DIR/wubi_l0.json" "$L0_DIR/pinyin_l0.json" 2>>"$LOG" || true

# 3b. Purge the loose build/ .app from LaunchServices and disk. Without
#     this, LS auto-registers the $HOME-resident bundle and may shadow
#     the install at /Library/Input Methods/ — picker silently hides
#     our IME. See feedback_ls_multi_version_pollution.
if [ "${KEEP_BUILD_APP:-0}" != "1" ]; then
  # shellcheck source=./_purge_ls.sh
  source ./_purge_ls.sh
  purge_ls_app "$APP_SRC" >>"$LOG" 2>&1 || true
fi

# 4. Restart TextInputMenuAgent so the menu-bar picker re-reads the new
#    bundle (display name, icon, mode list). TIS itself doesn't need a
#    re-register here — bundle id unchanged.
killall TextInputMenuAgent >>"$LOG" 2>&1 || true

# 5. (Re)install the LaunchAgent plist and bootstrap. Bootstrap is noisy
#    on re-load ("Bootstrap failed: 5: I/O error") when the domain still
#    considers the plist loaded — fully silenced; step 6 is the source
#    of truth for "is it running."
mkdir -p "$(dirname "$LA_DST")"
sed "s|__APP_PATH__|$APP_DST|g" Resources/LaunchAgent.plist.template > "$LA_DST" 2>>"$LOG"
launchctl bootstrap "gui/$UID" "$LA_DST" >>"$LOG" 2>&1 || true
sleep 1

# 6. Verify the binary is running.
if pgrep -f "Inputx.app/Contents/MacOS/Inputx" >/dev/null; then
  PID=$(pgrep -f "Inputx.app/Contents/MacOS/Inputx" | head -1)
  echo "[reinstall] ✓ Inputx running (PID $PID)"
else
  fail "Inputx not started — check 'log show --predicate \"process == \\\"Inputx\\\"\"'"
fi

# 7. Verify input source is enabled. If not, restore via reenable.sh
#    (defaults-write path on AppleEnabledInputSources — no TCC popup).
ENABLED=$(defaults read com.apple.HIToolbox AppleEnabledInputSources 2>/dev/null \
           | grep -c "jp.golia.inputmethod.wubi" || true)
if [ "$ENABLED" -gt 0 ]; then
  echo "[reinstall] ✓ enabled in input source list"
else
  if ! ./scripts/reenable.sh >>"$LOG" 2>&1; then
    fail "input source missing AND reenable failed"
  fi
  echo "[reinstall] ✓ re-added to input source list"
fi
