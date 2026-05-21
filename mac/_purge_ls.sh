#!/usr/bin/env bash
# Helper for mac/{release,install}.sh and mac/pkg/build_pkg.sh: unregister
# an intermediate .app from Launch Services and delete it from disk.
#
# Why this exists: macOS LaunchServices auto-registers any .app bundle it
# encounters under $HOME (no `open` needed). When build artifacts at
# `build/Inputx.app` or `build/pkg-stage/Inputx.app` linger after the
# release/install pipeline, LS keeps multiple records for `jp.golia.inputmethod.wubi`
# and tends to pick the freshest-mtime one as primary — which is the
# build/staging copy, flagged `launch-disabled` because it's not installed
# under /Library/Input Methods/. Result: System Settings → Keyboard input
# source picker silently hides our IME even though the install at
# /Library/Input Methods/Inputx.app is healthy.
#
# Sealed deliverables (.dmg, .pkg) don't trip LS — only loose .app bundles
# do — so each pipeline can safely purge its intermediate .app once the
# sealed artifact is built. Set KEEP_BUILD_APP=1 to opt out (e.g. for
# post-build poking, code-signing inspection, etc.).

_LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister

purge_ls_app() {
    local p="$1"
    [ -z "$p" ] && return 0
    if [ -d "$p" ]; then
        "$_LSREGISTER" -u "$p" 2>/dev/null || true
        rm -rf "$p"
        echo "[purge-ls] cleaned $p"
    fi
}
