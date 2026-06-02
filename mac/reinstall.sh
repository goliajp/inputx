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

# 3c. Re-register the install path to LaunchServices. purge_ls_app at 3b
#     unregisters the build/.app, leaving LS with ZERO entries for
#     jp.golia.inputmethod.wubi. Without an explicit register here, the
#     IME picker (Ctrl+Space / System Settings) silently hides Inputx —
#     even though the binary, LaunchAgent, AppleEnabledInputSources, and
#     TCC grant are all intact. macOS auto-scan of ~/Library/Input Methods/
#     isn't guaranteed to fire post-purge, so register explicitly.
#     Diagnosed 2026-05-27: reinstall completed cleanly but picker empty.
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
"$LSREGISTER" -f "$APP_DST" >>"$LOG" 2>&1 || true

# 4. (Re)install the LaunchAgent plist and bootstrap. Bootstrap is noisy
#    on re-load ("Bootstrap failed: 5: I/O error") when the domain still
#    considers the plist loaded — fully silenced; step 6 is the source
#    of truth for "is it running."
mkdir -p "$(dirname "$LA_DST")"
sed "s|__APP_PATH__|$APP_DST|g" Resources/LaunchAgent.plist.template > "$LA_DST" 2>>"$LOG"
launchctl bootstrap "gui/$UID" "$LA_DST" >>"$LOG" 2>&1 || true
sleep 1

# 5. Refresh the Ctrl+Space picker UI by replicating the user's "Settings
#    → 文本输入 → Edit → 删除 → 添加" round-trip *purely at the UserDefaults
#    layer*. The disable+enable round-trip via TIS API DOES fire the picker
#    refresh notification but ALSO retriggers the "允许开发者获取敏感信息"
#    TCC popup on every reinstall — user-reported 2026-05-27, retracted
#    from the earlier 9b0957a approach.
#
#    Instead we mutate AppleEnabledInputSources directly: remove our mode
#    entry, write+synchronize, re-append the entry, write+synchronize.
#    cfprefsd fires its own distributed notification on the second
#    synchronize (the list shape changed), which the picker UI receives
#    and uses to invalidate its filter cache. TCC stays silent because no
#    TIS API is called — UserDefaults writes are not gated by the
#    third-party-IME consent flow (only TISRegister/TISEnable are).
#
#    Empirically: this matches what Settings UI does internally on Edit →
#    delete + re-add (which the user discovered restores the picker).
swift - <<'SWIFT' >>"$LOG" 2>&1 || true
import Foundation

let modeID = "jp.golia.inputmethod.wubi.zh"
let bundleID = "jp.golia.inputmethod.wubi"

guard let defaults = UserDefaults(suiteName: "com.apple.HIToolbox") else { exit(0) }
var enabled = defaults.array(forKey: "AppleEnabledInputSources") as? [[String: Any]] ?? []

// Phase 1 — strip our entry. Force-write even if absent so cfprefsd
// re-pushes the array shape (no-op writes are coalesced; this guarantees
// at least one change event).
enabled.removeAll { ($0["Input Mode"] as? String) == modeID }
defaults.set(enabled, forKey: "AppleEnabledInputSources")
_ = defaults.synchronize()

// Phase 2 — re-append. Same shape Settings UI writes.
enabled.append([
    "Bundle ID":       bundleID as Any,
    "Input Mode":      modeID as Any,
    "InputSourceKind": "Input Mode" as Any,
])
defaults.set(enabled, forKey: "AppleEnabledInputSources")
_ = defaults.synchronize()
SWIFT

# 5b. Invalidate IntlDataCache — the macOS 26 cache that holds the
#     TIS enumeration result. Surviving this cache is what lets a
#     reinstalled bundle silently vanish from the keyboard menu
#     picker even though every other gate (TIS db, defaults,
#     codesign, LaunchAgent) checks out. Documented in
#     docs/macos-ime-recipe-2026.md. Deletion is the only reliable
#     invalidation — neither `killall TextInputMenuAgent`,
#     `lsregister -f`, FSEvents, nor `TISUpdateIntlFileCache()`
#     actually clear it.
#
#     Belongs in reinstall.sh (not just reinstall-safe.sh) so every
#     caller — reinstall-safe, bench-auto, manual runs — picks up
#     the invalidation. Otherwise bench-auto's 3-run loop racks up
#     stale-cache regressions every session.
CACHE_DIR=$(getconf DARWIN_USER_CACHE_DIR 2>/dev/null || echo "")
if [ -n "$CACHE_DIR" ]; then
  rm -f "${CACHE_DIR}"com.apple.IntlDataCache.le* >>"$LOG" 2>&1 || true
fi

# 5c. Restart TextInputMenuAgent so picker respawns AFTER the TIS
#     state transition above + the IntlDataCache invalidation.
#     Guarantees the picker process inits fresh.
killall TextInputMenuAgent >>"$LOG" 2>&1 || true

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
