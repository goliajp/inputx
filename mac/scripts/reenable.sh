#!/usr/bin/env bash
# reenable.sh — restore Inputx 五笔 to the menu-bar input switcher
# without going through System Settings → 文本输入 → +.
#
# Why this exists: macOS's AppleEnabledInputSources (the user-visible
# keyboard-menu list) sometimes gets cleared by reasons we haven't
# isolated — multiple lsregister runs, container resets, FileVault
# events, sometimes user clicks "-" in System Settings by mistake. The
# IME binary, LaunchAgent, TIS database, and TCC permission grant all
# stay intact; only AppleEnabledInputSources is missing the mode entry.
#
# This script writes that mode entry back via UserDefaults. Works
# silently AFTER the user has approved the third-party IME permission
# popup at least once (TCC remembers the bundle id). First-time installs
# must go through System Settings UI to trigger that popup.
#
# Run: ./mac/scripts/reenable.sh
# (No arguments. Idempotent — does nothing if wubi mode is already in
# the list. Restarts TextInputMenuAgent so the menu bar re-reads.)
#
# See docs/macos-ime-recipe-2026.md → "Don't kill cfprefsd" + the
# Symptom → fix table row for "User's IME stops appearing in their
# keyboard menu" for the wider context.

set -euo pipefail

cat <<'SWIFT' | swift -
import Foundation

let bundleID = "jp.golia.inputmethod.wubi"
let modeID   = "jp.golia.inputmethod.wubi.zh"

guard let defaults = UserDefaults(suiteName: "com.apple.HIToolbox") else {
    print("[reenable] ERR: can't open com.apple.HIToolbox defaults"); exit(1)
}

var enabled = defaults.array(forKey: "AppleEnabledInputSources") as? [[String: Any]] ?? []
let already = enabled.contains {
    ($0["Input Mode"] as? String) == modeID
}

if already {
    print("[reenable] wubi mode already enabled (\(enabled.count) total entries)")
    exit(0)
}

let entry: [String: Any] = [
    "Bundle ID":       bundleID,
    "Input Mode":      modeID,
    "InputSourceKind": "Input Mode",
]
enabled.append(entry)
defaults.set(enabled, forKey: "AppleEnabledInputSources")
_ = defaults.synchronize()

let reread = (UserDefaults(suiteName: "com.apple.HIToolbox")?
    .array(forKey: "AppleEnabledInputSources") as? [[String: Any]] ?? [])
let confirmed = reread.contains { ($0["Input Mode"] as? String) == modeID }
print(confirmed
    ? "[reenable] OK: wubi added to AppleEnabledInputSources (\(reread.count) total)"
    : "[reenable] FAIL: wrote but re-read doesn't show wubi — macOS may have rejected programmatic add. Add via System Settings UI instead.")
SWIFT

# Force the menu-bar input switcher to re-read the list.
killall TextInputMenuAgent 2>/dev/null || true
