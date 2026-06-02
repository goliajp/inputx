#!/usr/bin/env bash
# Safe Inputx reinstall — backup → reinstall → verify-stable → rollback-on-fail.
#
# Wraps `reinstall.sh` with extra survival checks for the polish-cli
# auto-update path. The plain `reinstall.sh` only verifies "process
# exists at the moment script ends" — but a freshly-started Inputx
# can crash 2s later (e.g. due to a build that compiled clean but
# crashes on IMK connection), LaunchAgent then either respawns it
# into a crash loop (process exists, can't accept input) or gives up
# (no process, IME silently dead). Both make every text-input app
# in the OS unusable until the user notices and manually fixes.
#
# Survival protocol:
#   1. Snapshot the currently-installed bundle to
#      ~/Library/Input Methods/Inputx.app.bak BEFORE touching anything.
#   2. Run reinstall.sh.
#   3. Wait 5s and verify the process is STILL alive and the PID
#      hasn't shifted (no crash + respawn cycle).
#   4. On verify fail: tear down the broken install, restore the
#      backup, exit non-zero.
#   5. On verify pass: drop the backup, exit zero.
#
# Usage:
#   mac/reinstall-safe.sh                 # full build + safe reinstall
#   mac/reinstall-safe.sh --no-build      # skip cargo rebuild
#
# Designed to be called from `inputx-polish` after a successful
# `make polish-rebuild` so that polish actions ship to the live IME
# without manual intervention, AND without ever bricking the user's
# typing.

set -uo pipefail

cd "$(dirname "$0")"
LOG=$(mktemp -t inputx-reinstall-safe.XXXXXX)
trap 'rm -f "$LOG"' EXIT

APP_NAME="Inputx"
APP_DST="$HOME/Library/Input Methods/$APP_NAME.app"
APP_BAK="$HOME/Library/Input Methods/$APP_NAME.app.bak-$(date +%s)"
PROCESS_PATTERN="Inputx.app/Contents/MacOS/Inputx"

# Verify health window (seconds): wait this long after reinstall.sh
# returns, then re-check process is alive AND PID hasn't shifted.
# 5s catches the typical IMK-connection-failure crash window (Inputx
# binary inits dyld + main, then crashes on IMKServer init if
# entitlements / connection-name mismatch); LaunchAgent respawns
# within ~1s on crash, so 5s gives 2-3 respawn cycles to manifest as
# "PID changed".
HEALTH_WINDOW=5

log() { echo "[safe] $1"; }
warn() { echo "[safe] ⚠️  $1" >&2; }
fail() {
  echo "[safe] ✗ $1" >&2
  echo "--- captured output ---" >&2
  cat "$LOG" >&2
  exit 1
}

# Step 1: snapshot the live bundle.
if [ ! -d "$APP_DST" ]; then
  log "no existing bundle at $APP_DST — first install path, no backup needed"
  BACKUP_TAKEN=0
else
  log "snapshotting current bundle → $APP_BAK"
  if ! cp -R "$APP_DST" "$APP_BAK" 2>>"$LOG"; then
    fail "could not snapshot current bundle — refusing to reinstall blind"
  fi
  BACKUP_TAKEN=1
fi

# Capture the pre-reinstall PID (if any) so we can detect crash-respawn
# cycles. Empty = no Inputx running pre-reinstall (e.g. first install
# or process was already dead).
PRE_PID=$(pgrep -f "$PROCESS_PATTERN" 2>/dev/null | head -1 || true)
log "pre-reinstall PID: ${PRE_PID:-none}"

# Step 2: standard reinstall.
log "running reinstall.sh $*"
if ! ./reinstall.sh "$@" 2>&1 | tee -a "$LOG"; then
  warn "reinstall.sh failed — restoring backup"
  if [ "$BACKUP_TAKEN" -eq 1 ]; then
    rm -rf "$APP_DST" 2>>"$LOG" || true
    mv "$APP_BAK" "$APP_DST" 2>>"$LOG" || true
    log "restored $APP_DST from backup"
  fi
  fail "reinstall failed; backup restored if available"
fi

# Step 3: verify-stable. Capture the post-reinstall PID, wait, re-check.
POST_PID=$(pgrep -f "$PROCESS_PATTERN" 2>/dev/null | head -1 || true)
if [ -z "$POST_PID" ]; then
  warn "reinstall.sh reported success but no Inputx process after it"
  ROLLBACK=1
else
  log "post-reinstall PID: $POST_PID — waiting ${HEALTH_WINDOW}s for crash window"
  sleep "$HEALTH_WINDOW"
  STILL_PID=$(pgrep -f "$PROCESS_PATTERN" 2>/dev/null | head -1 || true)
  if [ -z "$STILL_PID" ]; then
    warn "Inputx process disappeared during health window — likely crashed and LaunchAgent gave up"
    ROLLBACK=1
  elif [ "$STILL_PID" != "$POST_PID" ]; then
    warn "Inputx PID shifted from $POST_PID → $STILL_PID — crash + respawn detected (LaunchAgent's restart loop)"
    ROLLBACK=1
  else
    log "✓ Inputx stable: PID $POST_PID alive ${HEALTH_WINDOW}s post-install"
    ROLLBACK=0
  fi
fi

# Step 4: rollback if unhealthy.
if [ "${ROLLBACK:-0}" -eq 1 ]; then
  if [ "$BACKUP_TAKEN" -eq 1 ]; then
    log "tearing down broken install"
    launchctl bootout "gui/$UID/jp.golia.inputmethod.wubi" >>"$LOG" 2>&1 || true
    pkill -9 -f "$PROCESS_PATTERN" >>"$LOG" 2>&1 || true
    sleep 1
    rm -rf "$APP_DST" 2>>"$LOG" || true
    log "restoring $APP_DST from $APP_BAK"
    mv "$APP_BAK" "$APP_DST" 2>>"$LOG" || fail "could not restore backup — manual recovery needed"
    # Re-bootstrap the LaunchAgent against the restored bundle. The
    # plist already points at $APP_DST (same path), so a bootstrap of
    # the same plist re-loads the restored binary.
    launchctl bootstrap "gui/$UID" "$HOME/Library/LaunchAgents/jp.golia.inputmethod.wubi.plist" >>"$LOG" 2>&1 || true
    sleep 2
    if pgrep -f "$PROCESS_PATTERN" >/dev/null; then
      log "✓ backup restored, previous Inputx version running"
    else
      warn "backup restored but process did not start — IME may be unavailable until user intervenes"
    fi
    fail "new build was unstable — rolled back to pre-reinstall state"
  else
    fail "new install unstable AND no backup to restore — manual fix needed"
  fi
fi

# Step 5a: post-window menu refresh + switchability verification.
#
# reinstall.sh now (commit pending) does the IntlDataCache delete +
# menu-agent kill inline. Re-do BOTH after the health window as
# defense-in-depth: between reinstall.sh's inline kill and our
# 5s sleep, TextInputMenuAgent will have re-spawned. Killing it
# again post-window guarantees the visible picker boots against
# the fully-settled TIS state.
CACHE_DIR=$(getconf DARWIN_USER_CACHE_DIR 2>/dev/null || echo "")
if [ -n "$CACHE_DIR" ]; then
  rm -f "${CACHE_DIR}"com.apple.IntlDataCache.le* >>"$LOG" 2>&1 || true
fi
killall TextInputMenuAgent >>"$LOG" 2>&1 || true
ENABLED_COUNT=$(defaults read com.apple.HIToolbox AppleEnabledInputSources 2>/dev/null \
                | grep -c "jp.golia.inputmethod.wubi" || true)
if [ "$ENABLED_COUNT" -eq 0 ]; then
  warn "Inputx not in AppleEnabledInputSources after reinstall — running reenable.sh"
  if ! ./scripts/reenable.sh >>"$LOG" 2>&1; then
    fail "switchability recovery failed — IME not in menu bar"
  fi
  log "✓ reenable.sh restored Inputx to the input source list"
else
  log "✓ Inputx switchable from menu bar (${ENABLED_COUNT} mode entry)"
fi

# Step 5b: drop the backup, succeed.
if [ "$BACKUP_TAKEN" -eq 1 ]; then
  log "removing backup $APP_BAK"
  rm -rf "$APP_BAK" 2>>"$LOG" || warn "could not delete backup — manual cleanup needed"
fi

log "✓ reinstall-safe complete"
