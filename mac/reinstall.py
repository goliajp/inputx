#!/usr/bin/env python3
"""mac/reinstall.py — single entry point for Inputx install / reinstall.

Architecture (2026-06-06): canonical macOS IMK lifecycle. We use
imklaunchagent as the SOLE spawn path — no user-level LaunchAgent.
Reinstall is purely "swap bundle on disk + nudge LS + kill old
process"; the next host-app use triggers imklaunchagent to lazy-spawn
the new bundle. Mach-name registration is exclusive, so only one
Inputx process can ever exist. This eliminates the dual-process race
(LaunchAgent KeepAlive vs imklaunchagent on-demand) that caused
"two Inputx in menubar" + "can switch but can't type in already-open
apps" symptoms on 2026-06-05/06.

The earlier KeepAlive LaunchAgent (docs/macos-ime-recipe-2026.md §9)
was a workaround for "imklaunchagent silently refuses to launch" —
that refusal was caused by missing IMK Info.plist keys
(InputMethodServerDataSourceClass + InputMethodSessionController),
which are now present. Verified 2026-06-06: bootout LaunchAgent +
kill all Inputx procs → imklaunchagent lazy-spawned the binary in
~6 seconds on first host-app use. The workaround is no longer
needed and was actively harming us.

Mode is auto-detected:
  - "first":     bundle absent OR TIS has no row for our mode IDs
                 → drops bundle, then USER must do System Settings
                   → Keyboard → 文本输入 → 编辑 → + → 简体中文 →
                   Inputx 五笔 → 添加 + click Allow. That single UI
                   step is what registers TIS, writes
                   AppleEnabledInputSources, AND grants the macOS
                   third-party-IME TCC trust (which is the gate the
                   menu picker filters on). No programmatic shortcut
                   exists — macOS locks it behind UI.
  - "reinstall": bundle present AND TIS already has a row for our
                 mode ID (Settings registered it on the previous
                 first-install) → silent atomic bundle swap. No
                 TISRegister call (Settings owns that side, calling
                 it would create duplicate TIS rows). No
                 AppleEnabledInputSources write (Settings owns it).
                 TCC trust persists across bundle cdhash changes as
                 long as the bundle ID stays the same.

Any other TIS state (duplicates of the canonical mode ID, orphan
IDs from old bundle layouts like the pre-d6cdc52 `wubi.wubi.zh`)
is treated as STATE CORRUPTION and the script fails loudly.
Use `--clean` to drop the bundle + all TIS rows for our bundle ID
and start over (then a single Settings Add re-registers cleanly).

Safety: backup the current bundle, run, then probe-test the new
binary (run its `probe` CLI subcommand, check it returns Chinese
candidates). If the probe fails or returns wrong output → rollback.
Pass `--no-safe` to skip backup + probe.

Usage:
  mac/reinstall.py                # auto-detect, build, safe install
  mac/reinstall.py --no-build     # skip cargo rebuild
  mac/reinstall.py --no-safe      # no backup, no probe-test
  mac/reinstall.py --clean        # uninstall bundle + clean TIS state

Per project rule (memory: no-defensive-programming): one canonical
path per scenario, fail loud on unexpected state.
"""
from __future__ import annotations

import argparse
import os
import shutil
import signal
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

# ─── Constants ────────────────────────────────────────────────────────

APP_NAME = "Inputx"
BUNDLE_ID = "jp.golia.inputmethod.wubi"
MODE_ID = "jp.golia.inputmethod.wubi.zh"
CONNECTION_NAME = f"{BUNDLE_ID}_Connection"

HOME = Path.home()
MAC_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = MAC_DIR.parent
APP_SRC = PROJECT_ROOT / "build" / f"{APP_NAME}.app"
APP_DST = HOME / "Library" / "Input Methods" / f"{APP_NAME}.app"
# Legacy LaunchAgent path — kept ONLY so first-time-fresh-clones of a
# machine that previously had the LaunchAgent installed get it cleaned
# up by `--clean` / first reinstall. We never write to it anymore.
LA_DST = HOME / "Library" / "LaunchAgents" / f"{BUNDLE_ID}.plist"
L0_DIR = (
    HOME / "Library" / "Containers" / BUNDLE_ID
    / "Data" / "Library" / "Application Support" / APP_NAME
)
LSREGISTER = (
    "/System/Library/Frameworks/CoreServices.framework"
    "/Frameworks/LaunchServices.framework/Support/lsregister"
)
PROCESS_PATTERN = "Inputx.app/Contents/MacOS/Inputx"

# v1.15 hot-reload data-only detection --------------------------------------
#
# `SNAPSHOT_DIR/last-install.sha` records the git HEAD sha of the last
# successful `mac/reinstall.py` run. Every reinstall (fast OR full)
# updates it at the end. Phase D `classify_change_scope()` diffs it
# against current HEAD to decide whether the polish that happened
# since the last install is data-only (→ hot-reload fast path) or
# also touched code (→ full reinstall).
SNAPSHOT_DIR = HOME / "Library" / "Caches" / "inputx-reinstall-snapshots"
LAST_INSTALL_SHA = SNAPSHOT_DIR / "last-install.sha"
# Paths a change may touch and still qualify for the SIGUSR1 fast path.
#
# The admission rule is NOT "this file is data rather than code". It is:
# **the hot-reload swap set actually carries this file's effect into the
# installed bundle.** `do_hot_reload_data()` copies exactly
#
#     pinyin.dict, words.idf, bigrams.ngm, bigrams_inter.ngm
#     polish/{tier_overlay, quickfix_boost, exclusions_v1,
#             prior_corrections_v1, modern_vocab_v1,
#             corpus_garbage_filter_v1}.tsv
#
# into Contents/Resources/data/. Everything else the engines read is
# `include_bytes!` / `include_str!` — it lives INSIDE the binary, and no
# amount of SIGUSR1 will change it without shipping a new binary.
#
# So a prefix belongs here only if it is (a) one of the files above,
# (b) a source that `polish-rebuild` regenerates one of those files
# from, or (c) something that never reaches the bundle at all (tests,
# tooling, CI, docs).
#
# The wubi / nihongo data prefixes were briefly removed from this list
# (2026-08-05): their tables were embedded in the binary, so a wubi or JP
# polish took the fast path and shipped NOTHING — caught by the
# `wyet 信用 > 食用` polish, where hot-reload reported success and the
# installed bundle went on answering 食用. v1.17 fixed the underlying
# gap instead of living with it: those tables now sit behind ArcSwap
# slots, ship in the bundle, and get read back by
# `Session::reload_engine_data`, so the prefixes are admissible again —
# this time because the swap set genuinely carries them.
DATA_ONLY_PREFIXES: tuple[str, ...] = (
    # (a) files the swap set copies verbatim.
    "core/crates/inputx-pinyin-data-core/data/",
    "core/crates/inputx-pinyin-helpers/data/",
    "core/crates/inputx-wubi-data/data/",
    "core/crates/inputx-nihongo-data-kanji/data/",
    "core/crates/inputx-nihongo-data-jukugo/data/",
    # (b) sources `polish-rebuild` regenerates those files from.
    "core/crates/inputx-pinyin/data/",
    "core/crates/inputx-wubi/data/",
    "core/crates/inputx-nihongo/data/",
    "tools/scoring/data/",
    # (c) things that never reach the bundle.
    # Test-only src files the /polish protocol appends baseline cases
    # to. Gated behind `#[cfg(test)]` at composite/mod.rs so they never
    # link into the release `libinputx_core.a` the .app bundle carries.
    "core/crates/inputx-core/src/composite/comprehensive_baseline.rs",
    "core/crates/inputx-core/src/composite/baseline_quality_test.rs",
    # Tooling that DOES NOT ship in the .app bundle — reinstall.py
    # itself runs from the workspace, not from install location, so
    # changes to it don't affect the running Inputx binary and MUST
    # NOT force a bundle swap. Same for build.sh / hot-patch scripts,
    # the workflow YAML, etc. Fixes the self-referential trap where
    # every whitelist expansion previously triggered a full reinstall.
    "mac/reinstall.py",
    "mac/build.sh",
    "mac/hot-patch-assets.sh",
    "mac/scripts/",
    "mac/release.sh",
    "mac/notarize.sh",
    "Makefile",
    ".github/",
    "docs/",
    "README.md",
)

# ─── Output ───────────────────────────────────────────────────────────


def log(msg: str) -> None:
    print(f"[reinstall] {msg}")


def die(msg: str, *, code: int = 1) -> None:
    print(f"[reinstall] ✗ {msg}", file=sys.stderr)
    sys.exit(code)


# ─── Shell primitives ─────────────────────────────────────────────────


def run(cmd: list[str] | str, *, check: bool = True, input_: str | None = None,
        capture: bool = False) -> subprocess.CompletedProcess[str]:
    """Run a shell command. If check=True (default), non-zero exit = die().
    No `|| true` — every command we run is one we believe must succeed."""
    is_str = isinstance(cmd, str)
    result = subprocess.run(
        cmd, shell=is_str, check=False, text=True, input=input_,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )
    if check and result.returncode != 0:
        if capture:
            sys.stderr.write(result.stdout or "")
            sys.stderr.write(result.stderr or "")
        die(f"command failed (exit {result.returncode}): {cmd}")
    return result


def swift_eval(source: str) -> str:
    """Compile + run a Swift snippet that prints to stdout. Returns stdout.
    Used for TIS API queries that have no shell equivalent."""
    r = subprocess.run(
        ["swift", "-"], input=source, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False,
    )
    if r.returncode != 0:
        die(f"swift snippet failed:\n{r.stderr}")
    return r.stdout


def binary_probe_test(bundle: Path, *, timeout_s: float = 60.0) -> tuple[bool, str]:
    """Validate `bundle`'s binary by running its `probe` CLI subcommand
    (defined in mac/Sources/main.swift) and asserting it returns Chinese
    candidates for a known-good buffer.

    This is the post-install health check that replaced the pre-2026-06-06
    "PID alive 5s" check. Under the canonical imklaunchagent lifecycle
    there is no LaunchAgent keeping a PID alive — imklaunchagent
    lazy-spawns on first host-app use. So "PID alive" is no longer a
    valid signal. Probe-test runs the binary directly (which exits
    early via its CLI subcommand, never instantiating IMKServer, so
    it can't race with the lazy spawn), validating:
      - the bundle is loadable (dyld + main entry resolve)
      - the embedded Rust core links + produces correct candidates
      - the Swift IMKit wrapper invokes the core correctly

    Returns (ok, debug_output). `ok=False` triggers rollback.
    """
    exe = bundle / "Contents" / "MacOS" / APP_NAME
    if not exe.is_file():
        return False, f"binary missing: {exe}"
    try:
        r = subprocess.run(
            [str(exe), "probe", "nihao"],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, timeout=timeout_s, check=False,
        )
    except subprocess.TimeoutExpired:
        return False, f"probe timed out after {timeout_s}s"
    out = (r.stdout or "") + (r.stderr or "")
    if r.returncode != 0:
        return False, f"probe exited {r.returncode}: {out!r}"
    # `nihao` should yield 你好 at #1 in any sane build. If the Rust
    # core or its data files are broken, this fails loudly here rather
    # than later when the user can't type.
    if "你好" not in out:
        return False, f"probe ran but no 你好 in output: {out!r}"
    return True, out


# ─── TIS state inspection ─────────────────────────────────────────────


@dataclass(frozen=True)
class TisRow:
    source_id: str
    enabled: bool


def query_tis_rows() -> list[TisRow]:
    """List every TIS row whose ID matches our bundle prefix.
    Includes the bundle-level entry AND each mode entry."""
    out = swift_eval(r"""
import Carbon
let modes = (TISCreateInputSourceList(nil, true)?.takeRetainedValue()
             as? [TISInputSource]) ?? []
for m in modes {
    guard let idP = TISGetInputSourceProperty(m, kTISPropertyInputSourceID)
    else { continue }
    let id = Unmanaged<CFString>.fromOpaque(idP).takeUnretainedValue() as String
    if id.hasPrefix("jp.golia.inputmethod.wubi") {
        let enP = TISGetInputSourceProperty(m, kTISPropertyInputSourceIsEnabled)
        let en = enP.map {
            Unmanaged<CFBoolean>.fromOpaque($0).takeUnretainedValue() == kCFBooleanTrue
        } ?? false
        print("\(id)|\(en ? "Y" : "N")")
    }
}
""")
    rows: list[TisRow] = []
    for line in out.strip().splitlines():
        sid, en = line.split("|", 1)
        rows.append(TisRow(source_id=sid, enabled=(en == "Y")))
    return rows


def _ls_paths_for_bundle_id(bundle_id: str) -> list[str]:
    """List every on-disk path LaunchServices currently associates with
    `bundle_id`. Walks `lsregister -dump` and groups by record.

    Used by `verify_post_conditions` to detect the "two Inputx" class
    of bug — multiple .app bundles registered to the same id will all
    surface in the macOS input-source menubar, regardless of TIS state.
    """
    r = subprocess.run(
        [LSREGISTER, "-dump"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        text=True, check=False,
    )
    if r.returncode != 0:
        return []
    paths: list[str] = []
    current_path: str | None = None
    current_id: str | None = None
    for line in r.stdout.splitlines():
        stripped = line.lstrip()
        if stripped.startswith("path:"):
            raw = stripped.split(":", 1)[1].strip()
            # lsregister -dump appends `(0xXXXX)` record-id markers.
            # Strip so the path round-trips through Path().resolve().
            if raw.endswith(")"):
                paren = raw.rfind(" (0x")
                if paren > 0:
                    raw = raw[:paren]
            current_path = raw
            current_id = None
        elif stripped.startswith("identifier:"):
            current_id = stripped.split(":", 1)[1].strip()
            if current_id == bundle_id and current_path:
                paths.append(current_path)
    return paths


def query_third_party_enabled_count() -> int:
    """Number of `AppleEnabledThirdPartyInputSources` entries pointing
    at our BUNDLE_ID.

    macOS 26 split the per-IME enabled list across two plists. Apple's
    built-in IMEs (SCIM/CharacterPalette/etc) live in
    `com.apple.HIToolbox.plist`'s `AppleEnabledInputSources`. THIRD-
    PARTY IMEs (us) live in `com.apple.inputsources.plist`'s
    `AppleEnabledThirdPartyInputSources`. Confused about this for
    too long — the prior code read `AppleEnabledInputSources` and
    always got 0 for our bundle. Verified 2026-06-06 on a working
    install:

      plutil -p ~/Library/Preferences/com.apple.inputsources.plist
        AppleEnabledThirdPartyInputSources = (
            { Bundle ID = "jp.golia.inputmethod.wubi"; ... },
            { Bundle ID = "jp.golia.inputmethod.wubi"; ... },
        )

    Each enabled mode adds a row; our bundle + zh mode together = 2.
    """
    r = subprocess.run(
        ["defaults", "read", "com.apple.inputsources",
         "AppleEnabledThirdPartyInputSources"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, check=False,
    )
    if r.returncode != 0:
        return 0
    return r.stdout.count(BUNDLE_ID)


def current_selected_source_id() -> str | None:
    """Bundle.mode id of the user's currently selected input source.
    Used by reinstall to capture-then-restore selection across the
    process-kill step.
    """
    out = swift_eval(r"""
import Carbon
if let src = TISCopyCurrentKeyboardInputSource()?.takeRetainedValue() {
    let idP = TISGetInputSourceProperty(src, kTISPropertyInputSourceID)
    let id = idP.map {
        Unmanaged<CFString>.fromOpaque($0).takeUnretainedValue() as String
    } ?? ""
    print(id)
}
""").strip()
    return out or None


def restore_input_source(source_id: str) -> bool:
    """Programmatically re-select the given input source via TIS. Called
    after reinstall's kill-old-process step so the user's IME selection
    survives the bundle swap (kill resets the active mach connection;
    macOS auto-falls back to ABC for any host app that was bound to
    the dead binary; restoring is a no-op for users who weren't on us).
    Returns true on success.

    Side effect: triggers imklaunchagent to lazy-spawn the new bundle
    when the selected source is ours (since IMK looks up the Mach name
    on TISSelectInputSource → finds nothing → spawns).
    """
    out = swift_eval(f"""
import Carbon
let modes = (TISCreateInputSourceList(nil, true)?.takeRetainedValue()
             as? [TISInputSource]) ?? []
for m in modes {{
    guard let idP = TISGetInputSourceProperty(m, kTISPropertyInputSourceID)
    else {{ continue }}
    let id = Unmanaged<CFString>.fromOpaque(idP).takeUnretainedValue() as String
    if id == "{source_id}" {{
        let rc = TISSelectInputSource(m)
        print("rc=\\(rc)")
        exit(0)
    }}
}}
print("not_found")
""").strip()
    return out == "rc=0"


# ─── Steps ────────────────────────────────────────────────────────────


def build_bundle() -> None:
    """Build the Swift app bundle via mac/build.sh."""
    # Stop sccache before build (some Rust crates compile differently
    # under sccache; build.sh expects a clean state). Skip silently
    # when sccache isn't installed — fresh devices won't have it.
    if shutil.which("sccache"):
        subprocess.run(["sccache", "--stop-server"], stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL, check=False)
    env = os.environ.copy()
    env["RUSTC_WRAPPER"] = ""
    log("building bundle (cargo + swiftc)")
    r = subprocess.run(["./build.sh"], cwd=MAC_DIR, env=env, check=False)
    if r.returncode != 0:
        die("build failed; rerun with output visible to debug")
    if not APP_SRC.is_dir():
        die(f"build reported success but {APP_SRC} doesn't exist")


def stop_running_ime() -> None:
    """Kill any running Inputx process so the next host-app use will
    trigger imklaunchagent to lazy-spawn the new bundle, not connect
    to the old in-memory binary still serving the Mach service.

    SIGKILL is intentional: the binary's IMKServer connection holds
    the Mach name; clean shutdown is unnecessary for a reinstall.

    Also force-unloads any legacy user LaunchAgent from before the
    2026-06-06 architecture change — a leftover KeepAlive plist would
    immediately respawn the killed binary and recreate the dual-spawn
    race we're moving away from.
    """
    subprocess.run(
        ["launchctl", "bootout", f"gui/{os.getuid()}/{BUNDLE_ID}"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    )
    if LA_DST.exists():
        log(f"removing legacy LaunchAgent plist {LA_DST.name}")
        LA_DST.unlink()
    subprocess.run(
        ["pkill", "-9", "-f", PROCESS_PATTERN],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    )
    time.sleep(1)


def _renamex_swap(a: Path, b: Path) -> None:
    """Atomically exchange two paths on macOS via renamex_np(RENAME_SWAP).

    Both paths must exist on the same volume (always true here — both
    sit under ~/Library/Input Methods/).

    Why we need atomicity here, not "rm A; mv B A": macOS 26's
    HIToolbox watches `~/Library/Input Methods/*.app` continuously
    and re-evaluates each installed IME's enabled state on directory
    change events. If APP_DST disappears for even ~100ms (which it
    does during shutil.rmtree + shutil.copytree, total ~1-2s for our
    ~80MB bundle), HIToolbox drops us from the enabled-IME set and
    the user has to re-add via System Settings → Keyboard UI to
    recover. Verified on a fresh macOS 26 device 2026-06-05 after two
    back-to-back polish reinstalls silently broke input-source
    switching. The RENAME_SWAP path keeps APP_DST pointing at a valid
    bundle inode at every instant — HIToolbox never sees the gap.
    """
    import ctypes
    import ctypes.util
    libc = ctypes.CDLL(ctypes.util.find_library("c"), use_errno=True)
    libc.renamex_np.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
    libc.renamex_np.restype = ctypes.c_int
    RENAME_SWAP = 0x2
    rc = libc.renamex_np(str(a).encode(), str(b).encode(), RENAME_SWAP)
    if rc != 0:
        errno_val = ctypes.get_errno()
        die(f"renamex_np(RENAME_SWAP, {a}, {b}) failed (errno {errno_val})")


def swap_bundle() -> None:
    """Atomically swap the on-disk bundle.

    Strategy: stage the new bundle next to the install path, then
    `renamex_np(RENAME_SWAP)` swaps the two inodes in one syscall.
    APP_DST always points to a valid bundle; macOS HIToolbox never
    sees the path go missing (which is what cost the user their
    input-source-enabled state on 2026-06-05 — see `_renamex_swap`
    for the diagnosis).

    First-install path (APP_DST doesn't exist yet): no swap target,
    just rename the staged copy into place.
    """
    APP_DST.parent.mkdir(parents=True, exist_ok=True)
    staging = APP_DST.parent / f"{APP_NAME}.app.staging-{int(time.time())}"
    if staging.exists():
        shutil.rmtree(staging)
    shutil.copytree(APP_SRC, staging)

    if APP_DST.exists():
        if not (os.access(APP_DST, os.W_OK) and os.access(APP_DST / "Contents", os.W_OK)):
            # Prior pkg install left a root-owned bundle; remove it
            # first (this DOES leave a brief gap, but pkg-installed
            # users are rare and we can't atomic-swap into a path
            # we don't own).
            log("removing root-owned prior install (admin password)")
            run(["osascript", "-e",
                 f"do shell script \"rm -rf '{APP_DST}'\" with administrator privileges"])
            staging.rename(APP_DST)
        else:
            _renamex_swap(APP_DST, staging)
            # APP_DST now holds the new bundle; staging now holds the old.
            # Drop the OLD bundle's LaunchServices registration BEFORE
            # rmtree — LS caches by (path, cdhash) and an unregister
            # call on a deleted path is a no-op, so the old cdhash entry
            # would otherwise persist until macOS's next periodic LS
            # rescan. That residue is what surfaced as a duplicate
            # nested-container-app entry on 2026-06-05 after Phase I's
            # first deployment (debugged via lsregister -dump showing
            # two `jp.golia.inputx` entries with different cdhash).
            subprocess.run(
                [LSREGISTER, "-u", str(staging)],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
            )
            shutil.rmtree(staging)
    else:
        staging.rename(APP_DST)

    if not APP_DST.is_dir():
        die(f"bundle swap reported success but {APP_DST} missing")


def reset_l0_state() -> None:
    """Wipe L0 user-learning state on every reinstall.

    Per user directive 2026-05-26: pins multiply candidate score by
    PRIOR_L0_PIN_MULT (1000×), which dominates any corpus / polish-log
    work. Pins surviving across reinstalls make verifying polish
    changes impossible.

    polish-log.jsonl is preserved (it's a history record, not a
    ranking-influencing pin store).
    """
    for name in ("wubi_l0.json", "pinyin_l0.json"):
        p = L0_DIR / name
        if p.exists():
            p.unlink()


def purge_build_app_from_launchservices() -> None:
    """Unregister the build/ source bundle from LaunchServices.

    LaunchServices auto-registers any .app it sees; the build/ copy at
    PROJECT_ROOT/build/Inputx.app can shadow the install at $APP_DST
    and silently hide our IME from the picker.
    """
    # `_purge_ls.sh` is a shell helper that does the actual lsregister -u
    # + LSSetDefaultRoleHandler reset. Sourcing it from Python via bash.
    helper = MAC_DIR / "_purge_ls.sh"
    if not helper.exists():
        die(f"missing helper script: {helper}")
    run(["bash", "-c", f". '{helper}' && purge_ls_app '{APP_SRC}'"])


def purge_stray_project_bundles_from_launchservices() -> None:
    """Sweep every `.app` bundle in the project tree EXCEPT APP_SRC and
    APP_DST out of macOS LaunchServices.

    Implementation lives in `mac/scripts/purge-stray-ls.sh` so the same
    sweep is invokable standalone via `make purge-stray-ls` (e.g. after
    an `xcodebuild -destination "iOS Device"` if the user notices
    duplicate Inputx in the menubar before their next reinstall).
    Keeping a single source of truth avoids reinstall.py and the
    shell script drifting apart.

    Why this sweep exists: macOS LaunchServices auto-registers any
    `.app` it encounters under `~/`, INCLUDING iOS-platform bundles.
    The project's `ios/build-{device,sim}/Build/Products/*/InputxApp.app`
    artifacts surface in the input-source picker (which lists by
    display-name match, not platform filter) and produce "two Inputx"
    duplicates — verified 2026-06-05.
    """
    helper = MAC_DIR / "scripts" / "purge-stray-ls.sh"
    if not helper.is_file():
        die(f"missing helper script: {helper}")
    run(["bash", str(helper)])


def sweep_stale_swap_staging() -> None:
    """Remove any `Inputx.app.staging-*` directories that didn't get
    cleaned up by a prior reinstall (e.g. user ctrl-c'd mid-swap).

    These are user-writable and never load as IMEs (no LaunchAgent
    points at them), but they:
      - consume disk
      - can confuse `purge_stray_project_bundles_from_launchservices`
        if they were registered to LS before the rmtree
      - silently grow over time

    Same-directory glob — staging always sits next to APP_DST.
    """
    for p in APP_DST.parent.glob(f"{APP_NAME}.app.staging-*"):
        if p.is_dir():
            log(f"removing stale staging dir: {p.name}")
            subprocess.run(
                [LSREGISTER, "-u", str(p)],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
            )
            shutil.rmtree(p, ignore_errors=True)


def register_install_path_with_launchservices() -> None:
    """Tell LaunchServices the canonical bundle URL is $APP_DST.

    After `purge_build_app_from_launchservices`, LS has zero entries
    for our bundle ID. Without this register call, the picker
    silently hides Inputx even though TIS and AppleEnabledInputSources
    are correct. Diagnosed 2026-05-27 (commit 5c6c00d era).
    """
    run([LSREGISTER, "-f", str(APP_DST)])


def _intl_data_cache_paths() -> tuple[Path, list[Path]]:
    """Resolve $DARWIN_USER_CACHE_DIR and the two cache file paths.

    Factored out so `invalidate_intl_data_cache`,
    `force_intl_data_cache_rebuild`, and `diagnose_silent_fail` all
    read from the same canonical location.
    """
    r = subprocess.run(
        ["getconf", "DARWIN_USER_CACHE_DIR"],
        stdout=subprocess.PIPE, text=True, check=True,
    )
    cache_dir = Path(r.stdout.strip())
    if not cache_dir.exists():
        die(f"DARWIN_USER_CACHE_DIR doesn't exist: {cache_dir}")
    files = [
        cache_dir / "com.apple.IntlDataCache.le",
        cache_dir / "com.apple.IntlDataCache.le.kbdx",
    ]
    return cache_dir, files


def invalidate_intl_data_cache() -> None:
    """Delete macOS 26's TIS enumeration cache.

    The picker reads `$DARWIN_USER_CACHE_DIR/com.apple.IntlDataCache.le*`
    rather than re-querying TIS on every list. After a bundle swap,
    the cache is stale; nothing else invalidates it (killing
    TextInputMenuAgent, lsregister -f, FSEvents, and the private
    TISUpdateIntlFileCache() symbol all leave it untouched).
    Documented in docs/macos-ime-recipe-2026.md.

    Pairs with `force_intl_data_cache_rebuild`: delete invalidates,
    rebuild writes fresh — together they guarantee no daemon ends up
    holding a malformed in-process cache header.
    """
    _, files = _intl_data_cache_paths()
    for p in files:
        if p.exists():
            p.unlink()


def force_intl_data_cache_rebuild() -> None:
    """Trigger HIToolbox to rebuild `IntlDataCache.le[+.kbdx]` on disk.

    `invalidate_intl_data_cache` deletes the cache files but doesn't
    write fresh ones — the rebuild happens lazily on the next process
    that links HIToolbox and queries TIS (TextInputMenuAgent on user
    click, an opening app, etc.).

    Under fast reinstall sequences the cache can linger in a
    "deleted on disk + malformed/zero header still cached in some
    long-lived daemon's address space" state. Symptom observed
    2026-06-26 after the 3rd reinstall in 1h:

        imklaunchagent: (HIToolbox) TISFileInterrogator
            updateSystemInputSources false but old data invalid:
            currentCacheHeaderPtr nonNULL? 0, ->cacheFormatVersion 0,
            ->magicCookie 00000000, inputSourceTableCountSys 0

    imklaunchagent saw the deletion, started a rebuild — and that
    rebuild RACE the second invalidate-and-restart pass in
    `do_reinstall`, leaving the daemon's view stuck on a zero-header
    cache. Subsequent `TISSelectInputSource` calls returned
    OSStatus=0 but never actually flipped the active source
    (nothing to bind against).

    Fix: explicitly enumerate via `TISCreateInputSourceList(nil, true)`
    in a fresh subprocess. The `inIncludeAllInstalled=true` flag is
    documented to force a full filesystem rescan + persistent cache
    write. By the time this function returns, the cache files exist
    on disk with the new bundle's row included.

    Verify post-condition: cache files exist + non-zero size. If
    rebuild silently no-op'd (rare, but possible if HIToolbox decides
    its in-process header is "still valid" against the deleted file),
    log so the warm-cycle step downstream can correlate.
    """
    out = swift_eval(r"""
import Carbon
// `inIncludeAllInstalled = true` forces TISFileInterrogator to
// rescan ~/Library/Input Methods/ + /Library/Input Methods/ +
// system bundles, ignoring its "cache still valid" shortcut.
// The rescan persists to IntlDataCache.le[+.kbdx] as a side effect.
if let arr = TISCreateInputSourceList(nil, true)?.takeRetainedValue()
    as? [TISInputSource]
{
    print("rebuild=\(arr.count)")
} else {
    print("rebuild=nil")
}
""")
    _, files = _intl_data_cache_paths()
    for p in files:
        if not p.exists():
            log(f"⚠ {p.name} missing after rebuild ({out.strip()}) — "
                "warm-cycle will catch any downstream symptom")
        elif p.stat().st_size == 0:
            log(f"⚠ {p.name} is zero-byte after rebuild ({out.strip()}) — "
                "warm-cycle will catch any downstream symptom")
    # Soft-success log (always print, so reinstall transcript records
    # the rebuild moment even when the cache files look right).
    log(f"✓ forced IntlDataCache rebuild ({out.strip()})")


def restart_text_input_menu_agent() -> None:
    """Force the menu-bar picker to re-enumerate fresh.

    Pairs with `invalidate_intl_data_cache`: with the cache gone,
    the next picker rebuild reads TIS directly. Killing the agent
    forces that rebuild to happen now rather than on the next user
    click.
    """
    subprocess.run(
        ["killall", "TextInputMenuAgent"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    )


def ensure_third_party_enabled() -> None:
    """Re-assert our TIS rows in the macOS-26 third-party-IME plist.

    Why this exists (user 2026-07-01: "cannot input chinese ... 总出问题"):
    every reinstall replaces the bundle on disk → cdhash changes →
    macOS sometimes drops our entry from
    `com.apple.inputsources.AppleEnabledThirdPartyInputSources`,
    leaving the bundle valid + signed + LS-registered but NOT enabled
    in any host-app's IMK picker. The pre-2026-07-01 script reported
    success and left the user with a silently-broken IME, recoverable
    only by manually removing+re-adding via System Settings — which
    also triggered .bak duplication accumulation (now fixed: backups
    go to ~/Library/Caches/).

    macOS 26 splits IME state across two plists:
      - `com.apple.HIToolbox.AppleEnabledInputSources` — built-in IMEs
      - `com.apple.inputsources.AppleEnabledThirdPartyInputSources` —
        third-party IMEs (us). This is the canonical source-of-truth
        for our enabled state.

    Write the two rows our bundle expects (one for the bundle keyboard-
    input-method row, one for the input mode), and the matching row
    in HIToolbox.AppleEnabledInputSources. SIP doesn't block `defaults
    import` for these plists — Settings UI just owns the picker UX,
    not the underlying writeability. (Earlier confusion: I assumed
    SIP blocked writes; experimentally it doesn't. Settings UI does
    re-read on next launch, but our writes survive the read.)

    Pairs with `restart_text_input_menu_agent` immediately after, so
    the menu-bar picker re-enumerates and surfaces us without the
    user having to click anything.
    """
    import plistlib
    import tempfile

    def _ensure(domain: str, key: str, want_entries: list[dict]) -> tuple[int, int]:
        r = subprocess.run(
            ["defaults", "export", domain, "-"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=False,
        )
        try:
            plist = plistlib.loads(r.stdout) if r.stdout else {}
        except Exception:
            plist = {}
        existing = plist.get(key, []) or []
        before = len(existing)

        # Skip rows that already cover our wants.
        def _has_row(want: dict) -> bool:
            return any(
                all(e.get(k) == v for k, v in want.items())
                for e in existing
            )

        added = 0
        for want in want_entries:
            if not _has_row(want):
                existing.append(want)
                added += 1
        if added == 0:
            return before, 0

        plist[key] = existing
        with tempfile.NamedTemporaryFile("wb", suffix=".plist", delete=False) as f:
            plistlib.dump(plist, f)
            tmp = f.name
        subprocess.run(["defaults", "import", domain, tmp], check=True)
        return before, added

    # 1) Canonical third-party enabled plist (macOS 26).
    tp_wants = [
        {"Bundle ID": BUNDLE_ID, "InputSourceKind": "Keyboard Input Method"},
        {"Bundle ID": BUNDLE_ID, "Input Mode": MODE_ID,
         "InputSourceKind": "Input Mode"},
    ]
    _, tp_added = _ensure(
        "com.apple.inputsources",
        "AppleEnabledThirdPartyInputSources",
        tp_wants,
    )

    # DON'T write to com.apple.HIToolbox.AppleEnabledInputSources for
    # third-party IMEs. macOS 26 returns the UNION of both plists from
    # the TIS API, so writing the same mode row to both creates a
    # `len(state.mode_rows) > 1` corruption flag in `query_tis_rows()`.
    # com.apple.inputsources is the canonical plist; HIToolbox is for
    # built-in IMEs only.
    if tp_added:
        log(f"✓ re-asserted {tp_added} TIS enabled row(s) "
            f"in com.apple.inputsources")
    else:
        log("✓ TIS enabled rows already present")


def bounce_imklaunchagent() -> None:
    """Restart `com.apple.imklaunchagent` so its in-process bundle /
    connection-name cache is rebuilt from disk.

    Why this is needed (root-cause analysis 2026-06-26):

    imklaunchagent is the user-level LaunchAgent that brokers every
    IME activation request from host apps. It caches each known
    bundle's `(bundleIdentifier → InputMethodConnectionName)` mapping
    on first request, and reuses that mapping for the lifetime of the
    daemon. After a bundle swap, even when:

      - `lsregister -f` refreshed LaunchServices
      - `IntlDataCache.le` was deleted and rebuilt
      - `TextInputMenuAgent` was killed (lazy-respawned on next click)

    imklaunchagent's IN-PROCESS map is untouched. When the freshly-
    spawned new binary tries to publish its `IMKServer(name: …)`
    Mach connection, imklaunchagent looks up the cached name for the
    bundle, finds it doesn't match the launch request, and logs:

        imklaunchagent: (InputMethodKit) [com.apple.inputmethodkit:Server]
            Refusing connection name for bundle:
            unrecognized 'InputMethodConnectionName' value

    The host app's IMK client gets no Mach service to connect to.
    Symptom from the user side: "picker shows Inputx, clicking it
    silently fails / typing produces nothing". Documented in
    docs/macos-ime-recipe-2026.md gate 2 (symptom-fix table line 103);
    the doc's suggested fix ("fresh `lsregister -f` + restart of
    `TextInputMenuAgent`") works most of the time but is insufficient
    under fast reinstall sequences — the in-process map can survive
    both.

    Forensic evidence (2026-06-26 incident, 3 polish reinstalls in 1h):
    `Refusing connection name for bundle` was logged at 17:45:38,
    17:46:46, 19:50:02, 20:34:47 — every time the user / a host app
    tried to activate Inputx, until they manually re-added via
    System Settings (which made Settings UI itself force-rebuild
    imklaunchagent's mapping).

    Fix: kill imklaunchagent's process — `launchd` auto-respawns it.

    Why `kill -9 <pid>` and not `launchctl kickstart -k`: kickstart
    on a system-domain LaunchAgent (the plist lives in
    `/System/Library/LaunchAgents/`) is rejected by SIP with errno
    150 "Operation not permitted while System Integrity Protection
    is engaged", even when the service runs in the user `gui/$UID/`
    domain. SIP gates the launchctl management API, not the
    process-table kill path: SIGKILL through `kill -9` succeeds
    against a non-SIP-protected process address space, and launchd's
    KeepAlive contract for imklaunchagent guarantees a respawn
    within ~1s. We use `pkill -9 -f .../imklaunchagent$` so we
    target only the system imklaunchagent binary (not anything that
    happens to have the string in its argv), then poll for the new
    PID to confirm the respawn.

    Side effect on other IMEs is bounded: existing host-app Mach
    connections to other IMEs survive the kill (they were
    established directly, bypassing imklaunchagent); only new IMK
    launches during the ~0.5–1s gap pause briefly until launchd
    finishes respawning. Acceptable cost during a reinstall.
    """
    # Resolve the imklaunchagent pid via pgrep so we can verify the
    # respawn produced a different one (== launchd kicked in).
    def pid_of() -> int | None:
        r = subprocess.run(
            ["pgrep", "-f", r"/imklaunchagent$"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            text=True, check=False,
        )
        for line in (r.stdout or "").splitlines():
            line = line.strip()
            if line.isdigit():
                return int(line)
        return None

    before = pid_of()
    if before is None:
        # No live imklaunchagent: launchd will lazy-spawn it on next
        # IMK call. No cache to clear because there's no process. Done.
        log("✓ imklaunchagent not currently running (no cached state to clear)")
        return

    subprocess.run(
        ["kill", "-9", str(before)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    )

    # Poll until launchd respawns it (or until our 3s budget runs
    # out — way more than the typical ~0.3s respawn latency).
    deadline = time.monotonic() + 3.0
    after = None
    while time.monotonic() < deadline:
        after = pid_of()
        if after is not None and after != before:
            break
        time.sleep(0.1)

    if after is None or after == before:
        log(f"⚠ imklaunchagent did not respawn within 3s "
            f"(before={before}, after={after}). The warm-cycle step "
            "downstream will catch any resulting selection failure.")
        return
    log(f"✓ bounced imklaunchagent ({before} → {after}, "
        "fresh bundle/connection-name cache)")


def verify_can_switch_to_us(*, restore_to: str | None = None) -> bool:
    """Warm-cycle: programmatically select OUR mode, confirm the
    system actually flipped, then restore the prior selection.

    Why this is needed:

    `TISSelectInputSource` is documented to return `noErr` (=0) on
    success — but observed behavior under stale-state conditions
    (malformed IntlDataCache header, stale imklaunchagent
    bundle-name map) is "returns 0 + silently no-ops". The active
    source stays at whatever it was. No host-app symptom until the
    user notices "I can't switch to Inputx" minutes/hours later,
    long disconnected from the reinstall that caused it.

    The warm-cycle gives `do_reinstall` an end-to-end signal:
      1. Read current selection (the source we'll restore to).
      2. `TISSelectInputSource(our mode)`.
      3. Sleep briefly so the dispatcher commits.
      4. Read current selection again.
      5. If it doesn't start with our `BUNDLE_ID` → silent fail
         detected → caller `die()`s with diagnostics.
      6. Restore the original selection (best effort — the user's
         workflow is more important than perfect symmetry, and a
         failed restore is non-fatal).

    Returns True if the round-trip switched and back; False if the
    select silently failed. The caller is expected to surface the
    False result loudly.

    NOTE: `restore_to` overrides the captured pre-state when the
    caller already knows what selection should be restored (e.g.
    `do_reinstall` captures `pre_selected` at the very start, before
    any disruptive step, so passes that in here).
    """
    pre = restore_to if restore_to is not None else current_selected_source_id()

    # Step 1 — try to select us.
    if not restore_input_source(MODE_ID):
        log(f"✗ warm-cycle: TISSelectInputSource({MODE_ID}) was refused "
            "(TIS returned a non-zero status code)")
        return False

    # Step 2 — give the dispatcher a chance to commit. Empirically
    # 0.5s is enough for the macOS HID + IMK plumbing to settle; we
    # also leave a tighter polling window below as belt-and-braces.
    deadline = time.monotonic() + 1.5
    actual: str | None = None
    while time.monotonic() < deadline:
        actual = current_selected_source_id()
        if (actual or "").startswith(BUNDLE_ID):
            break
        time.sleep(0.1)

    ok = (actual or "").startswith(BUNDLE_ID)
    if not ok:
        log(f"✗ warm-cycle: TISSelectInputSource returned 0 but "
            f"current source is {actual!r} (expected prefix "
            f"{BUNDLE_ID!r}) after 1.5s of polling")

    # Step 3 — restore (best effort, never blocks the result).
    if pre and pre != MODE_ID:
        restore_input_source(pre)

    return ok


def diagnose_silent_fail() -> None:
    """Dump the diagnostic context that explains a warm-cycle failure.

    Called when `verify_can_switch_to_us` returns False. Three pieces
    are useful to a future debugger / to the user staring at a broken
    picker:

      1. IntlDataCache files — present / size / mtime. A zero-byte
         or missing cache after `force_intl_data_cache_rebuild` is a
         very strong signal that the rebuild itself didn't take.

      2. Recent `imklaunchagent: Refusing connection name` log lines
         — these are the smoking gun for the stale-name-cache
         scenario.

      3. The orthodox user-side recovery: re-add via System Settings.
         Settings UI's "Add Input Source" flow internally forces
         imklaunchagent + HIToolbox to do a full re-read, which is
         the only path that reliably unsticks them when the script's
         bounce + rebuild somehow didn't.
    """
    log("─── diagnostics for silent input-source selection failure ───")

    # 1. IntlDataCache file state.
    _, files = _intl_data_cache_paths()
    for p in files:
        if not p.exists():
            log(f"  {p.name}: MISSING")
        else:
            stat = p.stat()
            mtime = time.strftime("%Y-%m-%d %H:%M:%S",
                                  time.localtime(stat.st_mtime))
            log(f"  {p.name}: {stat.st_size} bytes, mtime={mtime}")

    # 2. imklaunchagent refusals over the last 5 minutes — those
    # are the canonical Gate-2 silent-fail symptom.
    log("  imklaunchagent refusals (last 5m):")
    r = subprocess.run(
        ["log", "show", "--last", "5m", "--predicate",
         'eventMessage CONTAINS "Refusing connection name"'],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        text=True, check=False,
    )
    # Filter on BOTH process name and message — `log show` echoes its
    # own `--predicate` argv in a meta log line that would otherwise
    # match a bare "Refusing connection name" substring search.
    refusal_lines = [
        ln for ln in (r.stdout or "").splitlines()
        if "imklaunchagent" in ln
        and "Refusing connection name" in ln
        and "predicate" not in ln
    ]
    if not refusal_lines:
        log("    (none — silent fail is upstream of imklaunchagent, "
            "likely an IntlDataCache or TIS state issue)")
    else:
        for ln in refusal_lines[-5:]:
            log(f"    {ln}")

    # 3. The user-side recovery the script can't perform from CLI.
    log("")
    log("  RECOVERY: open System Settings → Keyboard → 文本输入 → 编辑")
    log("  (or 'Edit…' under Input Sources), remove Inputx via '−',")
    log("  then re-add via '+'. This forces macOS to re-issue a fresh")
    log("  InputMethodConnectionName binding through the same UI flow")
    log("  that originally granted TCC trust, bypassing whatever cache")
    log("  is sticky in the script's own state-cleaning attempt.")


# ─── State classification ────────────────────────────────────────────


@dataclass(frozen=True)
class TisState:
    rows: list[TisRow]

    @property
    def mode_rows(self) -> list[TisRow]:
        return [r for r in self.rows if r.source_id == MODE_ID]

    @property
    def bundle_rows(self) -> list[TisRow]:
        return [r for r in self.rows if r.source_id == BUNDLE_ID]

    @property
    def orphan_rows(self) -> list[TisRow]:
        """Rows whose ID is ours by prefix but is NEITHER the bundle ID
        NOR the canonical mode ID. The legacy `wubi.wubi.zh` lives
        here when an old bundle layout still has TIS rows."""
        return [r for r in self.rows
                if r.source_id != BUNDLE_ID and r.source_id != MODE_ID]


def classify_state() -> tuple[str, TisState]:
    """Return (mode, tis_state) where mode is:
       "first"      — clean state, fresh install OK
       "reinstall"  — bundle already trusted, silent reinstall OK
       "corrupt"    — duplicates / orphans, requires --clean
    """
    rows = query_tis_rows()
    state = TisState(rows=rows)

    if state.orphan_rows:
        return "corrupt", state
    if len(state.mode_rows) > 1:
        return "corrupt", state
    if len(state.bundle_rows) > 1:
        return "corrupt", state

    if not APP_DST.exists() or not state.mode_rows:
        return "first", state
    return "reinstall", state


def describe_corruption(state: TisState) -> str:
    parts: list[str] = []
    if state.orphan_rows:
        ids = ", ".join(sorted(r.source_id for r in state.orphan_rows))
        parts.append(f"orphan TIS rows from older bundle layouts: {ids}")
    if len(state.mode_rows) > 1:
        parts.append(f"{len(state.mode_rows)}× duplicate rows for {MODE_ID}")
    if len(state.bundle_rows) > 1:
        parts.append(f"{len(state.bundle_rows)}× duplicate bundle rows for {BUNDLE_ID}")
    return "; ".join(parts)


# ─── Cleanup (--clean) ────────────────────────────────────────────────


def clean_tis_state() -> None:
    """One-shot migration: drop bundle + every TIS row for our bundle ID.

    Use this when TIS state is corrupted (from old code paths,
    accumulated re-registrations, etc.). After running, the
    subsequent reinstall runs as a fresh first-install.
    """
    log("disabling all TIS rows for our bundle ID")
    swift_eval(r"""
import Carbon
let modes = (TISCreateInputSourceList(nil, true)?.takeRetainedValue()
             as? [TISInputSource]) ?? []
for m in modes {
    guard let idP = TISGetInputSourceProperty(m, kTISPropertyInputSourceID)
    else { continue }
    let id = Unmanaged<CFString>.fromOpaque(idP).takeUnretainedValue() as String
    if id.hasPrefix("jp.golia.inputmethod.wubi") {
        TISDisableInputSource(m)
        print("disabled \(id)")
    }
}
""")
    log("stopping IME process + removing legacy LaunchAgent if any")
    stop_running_ime()
    if APP_DST.exists():
        log(f"removing {APP_DST}")
        shutil.rmtree(APP_DST)
    log("clearing AppleEnabledInputSources entry")
    swift_eval(r"""
import Foundation
guard let defaults = UserDefaults(suiteName: "com.apple.HIToolbox") else { exit(0) }
var enabled = defaults.array(forKey: "AppleEnabledInputSources") as? [[String: Any]] ?? []
let before = enabled.count
enabled.removeAll { ($0["Bundle ID"] as? String) == "jp.golia.inputmethod.wubi" }
defaults.set(enabled, forKey: "AppleEnabledInputSources")
_ = defaults.synchronize()
print("entries: \(before) -> \(enabled.count)")
""")
    log("invalidating IntlDataCache + restarting TextInputMenuAgent")
    invalidate_intl_data_cache()
    restart_text_input_menu_agent()
    log("✓ TIS state cleaned. Run mac/reinstall.py to install fresh.")


# ─── Install / reinstall happy paths ──────────────────────────────────


def do_first_install(with_build: bool) -> None:
    """First install: drop the bundle on disk, hand off to System
    Settings UI for TIS register + TCC trust grant. No LaunchAgent,
    no programmatic process spawn — imklaunchagent will lazy-spawn
    the binary on first host-app use after the user completes the
    Settings Add step.

    Why not TISRegister from here: macOS gates the TCC "Allow Inputx
    to read all input" popup behind Settings UI's Add Input Source
    button — there's no programmatic equivalent. Calling
    TISRegisterInputSource from us would just create a TIS row
    without the trust grant, and the picker would still filter us
    out. Settings UI's Add path covers both TIS register AND TCC
    trust in one user action.
    """
    if with_build:
        build_bundle()
    elif not APP_SRC.is_dir():
        die(f"no build at {APP_SRC} — drop --no-build or run mac/build.sh first")

    sweep_stale_swap_staging()
    swap_bundle()
    reset_l0_state()
    purge_build_app_from_launchservices()
    purge_stray_project_bundles_from_launchservices()
    register_install_path_with_launchservices()
    stop_running_ime()
    invalidate_intl_data_cache()
    restart_text_input_menu_agent()

    log("✓ bundle installed at " + str(APP_DST))
    log("")
    log("NEXT STEP (mandatory, macOS gates this behind UI):")
    log("  System Settings → Keyboard → 文本输入 → Input Sources → 编辑")
    log("  → +  → 简体中文 → Inputx 五笔 → 添加")
    log("  → click \"Allow\" on the 'Inputx wants to read all input' popup.")
    log("")
    log("After that single action, TCC trust is granted and imklaunchagent")
    log("will lazy-spawn Inputx on first host-app use. Every subsequent")
    log("`mac/reinstall.py` runs silently (no popup) until the bundle ID")
    log("changes again.")


def do_reinstall(with_build: bool) -> None:
    """Silent reinstall: atomic bundle swap + LS refresh + warm-cycle
    verify. We kill any running Inputx process during the flow so the
    next host-app use lazy-spawns the new bundle via imklaunchagent.
    If the user was actively typing through Inputx, we capture-then-
    restore the selection so the swap is invisible to them.

    Explicitly NOT done here (would create duplicates / re-trigger TCC):
      - `Inputx install` / TISRegisterInputSource (Settings owns it)
      - AppleEnabledThirdPartyInputSources writes (Settings owns it)
      - LaunchAgent install (retired 2026-06-06 — was a workaround for
        a refusal scenario that no longer exists)

    Cache + daemon hardening (2026-06-26 incident retrospective):

      A reinstall's last-mile failure mode is "bundle on disk is correct
      + TIS row enabled + picker shows our entry + TISSelectInputSource
      returns OSStatus 0 — but the active source never actually changes".
      Two independent caches collude to produce it:

      1. `IntlDataCache.le[+.kbdx]` — the on-disk TIS enumeration cache.
         `invalidate_intl_data_cache` deletes the files but doesn't write
         fresh ones; rebuild waits for the next process to query TIS.
         Under fast reinstall sequences the cache can linger in a
         "deleted on disk + malformed/zero header still cached in a long-
         lived daemon's address space" state, in which subsequent TIS
         API calls succeed nominally but bind against nothing.

      2. `imklaunchagent`'s in-process `(bundleId → InputMethodConnectionName)`
         map. The daemon caches this on first request and reuses it for
         its full lifetime. After a bundle swap, the cached name no
         longer matches the new bundle's Info.plist; when the new binary
         tries to publish `IMKServer(name:)`, imklaunchagent refuses with
         `Refusing connection name for bundle: unrecognized
         'InputMethodConnectionName' value`.

      Forensic evidence collected 2026-06-26 (3 polish reinstalls in 1h):
      after the 3rd reinstall, `Refusing connection name` was logged at
      17:45 / 17:46 / 19:50 / 20:34 — every user attempt to activate
      Inputx — until they manually re-added via System Settings (whose
      Add UI internally forces the same rebuild + bounce we now do).

      Fix in this function, in order:
        - `force_intl_data_cache_rebuild`: explicit TIS enumeration in
          a fresh subprocess persists a fresh cache to disk after the
          deletion, so no daemon ends up holding a malformed header.
        - `bounce_imklaunchagent`: `launchctl kickstart -k` the user-
          level LaunchAgent, clearing its in-process bundle/name map.
        - `verify_can_switch_to_us`: end-to-end warm cycle that
          programmatically selects our mode and confirms the active
          source actually flipped. If it didn't, `diagnose_silent_fail`
          dumps the cache state + recent imklaunchagent refusals + the
          user-side recovery path, then `die()`.

      Net effect: a reinstall either succeeds end-to-end (verified) or
      fails loudly with diagnostics. No silent-broken-IME-after-reinstall
      window.
    """
    if with_build:
        build_bundle()
    elif not APP_SRC.is_dir():
        die(f"no build at {APP_SRC} — drop --no-build or run mac/build.sh first")

    # Capture before any destructive step so we can restore after.
    pre_selected = current_selected_source_id()
    user_was_on_us = pre_selected is not None and pre_selected.startswith(BUNDLE_ID)
    if user_was_on_us:
        log(f"user is currently on {pre_selected} — will restore after swap")

    sweep_stale_swap_staging()
    swap_bundle()
    reset_l0_state()
    purge_build_app_from_launchservices()
    purge_stray_project_bundles_from_launchservices()
    register_install_path_with_launchservices()
    invalidate_intl_data_cache()
    restart_text_input_menu_agent()
    # kill late: keeps the old binary serving host apps through the
    # earlier swap+LS steps, then this triggers imklaunchagent to
    # lazy-spawn fresh on next use with all updated state in place.
    stop_running_ime()
    # Second invalidate + agent restart pass — the picker rebuild after
    # the first pass races the kill; this second pass gives the rebuild
    # a clean view of "no live Inputx, but TIS row enabled".
    invalidate_intl_data_cache()
    restart_text_input_menu_agent()
    # Now that the cache is invalidated for the second time and no
    # live Inputx is racing it, EXPLICITLY rebuild the cache rather
    # than waiting for the next ambient TIS query to do it. This
    # guarantees the cache files exist on disk with the new bundle's
    # row by the time the warm-cycle below tries to bind.
    force_intl_data_cache_rebuild()
    # And bounce imklaunchagent — the on-disk cache being correct is
    # not sufficient; the daemon's IN-PROCESS bundle/connection-name
    # map also has to be fresh, otherwise the next
    # `IMKServer(name:)` publish gets refused.
    bounce_imklaunchagent()

    # Warm-cycle: programmatically prove that selecting us actually
    # works, before reporting success to the user. Always run — even
    # when `user_was_on_us` is False, the diagnostic value of catching
    # a silent fail HERE (during reinstall, with full context) is far
    # higher than catching it later via "user reports can't switch".
    restore_target = pre_selected if user_was_on_us else pre_selected
    if not verify_can_switch_to_us(restore_to=restore_target):
        diagnose_silent_fail()
        die("warm-cycle verification failed — the bundle on disk is "
            "correct and TIS reports our row as enabled and selectable, "
            "but `TISSelectInputSource(our mode)` did not flip the "
            "active source. See diagnostics above; the user-side "
            "recovery path is System Settings → re-add Inputx.")
    if user_was_on_us:
        log(f"✓ restored input source selection → {pre_selected}")
    else:
        log(f"✓ warm-cycle confirmed switching to {MODE_ID} works")
    verify_post_conditions(expect_first_install=False)


def verify_post_conditions(*, expect_first_install: bool) -> None:
    """Each install step has a contract; verify the post-state matches.

    Post-condition signals (after the 2026-06-06 LaunchAgent retirement):
      - TIS row for MODE_ID exists and is enabled (persistent registry)
      - LaunchServices has exactly ONE registration for our BUNDLE_ID
        (no stray iOS containers / atomic-swap residue)
      - Bundle on disk is the path we just wrote

    NOT checked: process PID. Under imklaunchagent lazy-spawn, there
    is no Inputx process right after install — the first host-app use
    triggers the spawn. Checking PID would always fail. Binary health
    is validated by with_safety_net's probe-test (running the binary's
    `probe` CLI subcommand directly).
    """
    rows = query_tis_rows()
    mode_rows = [r for r in rows if r.source_id == MODE_ID]
    if len(mode_rows) != 1:
        die(f"post-condition violation: expected 1 TIS row for {MODE_ID}, "
            f"found {len(mode_rows)}. Aborting before this state corrupts "
            "the picker further.")
    log(f"✓ TIS row for {MODE_ID} (enabled={mode_rows[0].enabled})")

    # LS-singleton post-condition: lsregister -dump must have at most
    # ONE entry for our IME bundle id. Two entries surfaces as
    # "two Inputx" in the macOS input-source menubar — even though TIS
    # is single — because the picker enumerates LS-registered bundles
    # by display name, not by TIS rows. Verified failure mode
    # 2026-06-05 after Phase I deploy: iOS build products + atomic-
    # swap residue together produced 3 LS entries. The sweep + atomic-
    # swap-residue-cleanup above SHOULD prevent it; the post-condition
    # is the trip wire that says "if it happens again, abort instead
    # of silently shipping a broken picker".
    ls_paths = _ls_paths_for_bundle_id(BUNDLE_ID)
    extra = [p for p in ls_paths if Path(p).resolve() != APP_DST.resolve()]
    if extra:
        die("post-condition violation: LaunchServices has stray "
            f"registrations for {BUNDLE_ID} besides the install path:\n"
            + "\n".join(f"  - {p}" for p in extra)
            + "\nRun `lsregister -u <path>` on each, then retry.")
    log(f"✓ LaunchServices singleton for {BUNDLE_ID}")

    if expect_first_install:
        log("✓ first-install complete (bundle on disk + TIS row + LS singleton).")
        log("")
        log("NEXT STEP (user action required — macOS gates this behind UI):")
        log("  System Settings → Keyboard → Input Sources → Add Input")
        log("  Source → Simplified Chinese → Inputx 五笔 → Add → click")
        log("  'Allow' on the 'Inputx wants to read all input' popup.")
        log("")
        log("After that, imklaunchagent will lazy-spawn Inputx on the next")
        log("host-app use, and Ctrl+Space can switch to it.")
        return

    if not mode_rows[0].enabled:
        die(f"post-condition violation: TIS row for {MODE_ID} exists "
            "but is disabled. Re-enable via System Settings → Keyboard "
            "→ Input Sources, or run --clean and reinstall fresh.")

    # AppleEnabledThirdPartyInputSources is the macOS 26 store that
    # actually controls picker visibility for 3rd-party IMEs (Apple's
    # built-in ones use AppleEnabledInputSources in HIToolbox.plist
    # instead). Discovered 2026-06-06 — prior versions of this script
    # incorrectly read the HIToolbox key and always got 0. If our
    # bundle isn't in the 3rd-party store, the user added it via
    # Settings UI in some prior session that got cleaned up (e.g.
    # bundle id changed) and needs to redo the Add step. Without it,
    # TIS thinks we're enabled but the picker won't show us.
    enabled_count = query_third_party_enabled_count()
    if enabled_count == 0:
        die(f"post-condition violation: AppleEnabledThirdPartyInputSources "
            f"has no entry for {BUNDLE_ID}. macOS Settings UI is the only "
            "way to add it (TCC trust gating). Open:\n"
            "  System Settings → Keyboard → Input Sources → Edit → +\n"
            "  → Simplified Chinese → Inputx Wubi → Add\n"
            "then `mac/reinstall.py --no-build` to re-verify.")
    log(f"✓ AppleEnabledThirdPartyInputSources has {enabled_count} entry "
        f"for {BUNDLE_ID}")


# ─── Safety wrapper (backup + probe-test + rollback) ───────────


def with_safety_net(install_fn) -> None:
    """Run `install_fn`, but snapshot + rollback if the new binary
    fails its probe-test.

    Probe-test catches the binary-broken class: link errors, missing
    Rust core data files, segfaults on candidate generation, Swift
    runtime mismatches. The binary's `probe` CLI subcommand runs the
    full input → candidates path WITHOUT instantiating IMKServer, so
    we can verify correctness directly (no race with imklaunchagent
    lazy-spawn, no LaunchAgent involvement).

    The pre-2026-06-06 "PID alive 5s" check is gone — imklaunchagent
    only spawns on first host-app use, so right after install the PID
    is intentionally absent. Replacing the implicit process-liveness
    signal with explicit binary-output validation.
    """
    backup: Path | None = None

    if APP_DST.is_dir():
        # Snapshot to a directory OUTSIDE ~/Library/Input Methods/, because
        # macOS scans Input Methods/ for IMEs and would register .bak-*
        # as additional TIS source rows (user 2026-07-01: 「现在 input
        # sources 里有十几个 inputx，删了再添加也是一次加出来十几个」 — root cause
        # was .bak left in Input Methods/ after rollbacks, multiplying the
        # TIS rows macOS Settings UI exposes).
        snapshot_dir = HOME / "Library" / "Caches" / "inputx-reinstall-snapshots"
        snapshot_dir.mkdir(parents=True, exist_ok=True)
        backup = snapshot_dir / f"{APP_NAME}.app.bak-{int(time.time())}"
        log(f"snapshotting current bundle → {backup}")
        shutil.copytree(APP_DST, backup)

    try:
        install_fn()
    except SystemExit as e:
        if e.code != 0:
            _rollback(backup, "install step failed")
        raise

    log("probing new binary (running `Inputx probe nihao` CLI subcommand)")
    ok, detail = binary_probe_test(APP_DST)
    if not ok:
        _rollback(backup, f"binary probe failed: {detail}")
    log("✓ binary probe returned 你好")

    # Re-assert our enabled TIS rows + bounce menu agent so the macOS-26
    # picker shows us without the user needing to re-add via Settings.
    # (Most reinstalls preserve the enabled state; some don't. Idempotent
    # write covers both cases without surprising the working ones.)
    ensure_third_party_enabled()
    restart_text_input_menu_agent()

    if backup is not None:
        shutil.rmtree(backup)
    log("✓ install complete (imklaunchagent will lazy-spawn on first use)")


def _rollback(backup: Path | None, reason: str) -> None:
    if backup is None:
        die(f"new install failed ({reason}) AND no backup to restore. "
            "Manual recovery required.", code=1)
    log(f"⚠ rolling back: {reason}")
    # No need to bootout LaunchAgent (we don't install one anymore).
    # Kill any running Inputx so the lazy-spawn after rollback picks
    # up the restored bundle instead of the broken in-memory binary.
    subprocess.run(
        ["pkill", "-9", "-f", PROCESS_PATTERN],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    )
    time.sleep(1)
    if APP_DST.exists():
        shutil.rmtree(APP_DST)
    shutil.move(str(backup), str(APP_DST))
    # Refresh LS and picker cache so the restored bundle's cdhash
    # propagates immediately.
    subprocess.run(
        [LSREGISTER, "-f", str(APP_DST)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    )
    invalidate_intl_data_cache()
    restart_text_input_menu_agent()
    # Validate the restored backup itself works — paranoia, but worth
    # confirming we didn't restore something that was already broken.
    ok, detail = binary_probe_test(APP_DST)
    if not ok:
        die(f"backup restored to {APP_DST} but probe ALSO failed: {detail}. "
            "Manual recovery required.", code=2)
    log(f"✓ backup restored to {APP_DST}, probe verifies previous build works")
    die(f"new build was unstable: {reason}", code=2)


# ─── Main ─────────────────────────────────────────────────────────────


def _run_git(args: list[str]) -> str:
    """Small wrapper — `git` from PROJECT_ROOT, stdout stripped."""
    out = subprocess.check_output(["git", "-C", str(PROJECT_ROOT)] + args)
    return out.decode().strip()


def _current_head_sha() -> str:
    return _run_git(["rev-parse", "HEAD"])


def classify_change_scope() -> str:
    """Diff the CURRENT WORKING TREE against the tree that was last
    installed (`LAST_INSTALL_SHA`, written by `_record_install_sha`) and
    return one of:

    - "data-only": every changed file sits under a DATA_ONLY_PREFIXES
      prefix. Safe for the SIGUSR1 fast path.
    - "code":     at least one changed file is outside the data prefixes. The
      running IME binary won't reflect the new code — must do full reinstall.
    - "unknown":  no LAST_INSTALL_SHA yet (first install this machine),
      or the recorded sha is missing from the local repo (rebase / prune).
      Safe fallback = full reinstall.

    One `git diff <marker>` against the working tree, not the old
    `marker..HEAD` + `status --porcelain` pair. The question this
    function answers is "what changed since the bytes I installed?", and
    the marker commit already carries the working tree as installed, so
    a single working-tree diff answers it directly. The two-scan version
    got it wrong in both directions once an install happened dirty (the
    /polish protocol's Step-5-before-Step-6 order makes that the norm):
    a file installed dirty and then committed showed up in `marker..HEAD`
    as new code, and a file installed dirty and left dirty showed up in
    `status --porcelain` as new code. Both forced a needless full
    reinstall, whose real cost is host apps with a live IMK session
    (WeChat) going mute until restarted.

    `git diff` only covers tracked files, so untracked ones are scanned
    separately — a brand-new source file is real code even though no
    diff mentions it.
    """
    if not LAST_INSTALL_SHA.exists():
        return "unknown"
    try:
        last_sha = LAST_INSTALL_SHA.read_text().strip()
    except OSError:
        return "unknown"
    if not last_sha:
        return "unknown"
    # Verify the sha still exists in the local repo.
    try:
        subprocess.check_output(
            ["git", "-C", str(PROJECT_ROOT), "cat-file", "-e", last_sha],
            stderr=subprocess.DEVNULL,
        )
    except subprocess.CalledProcessError:
        return "unknown"

    # Tracked files that differ between the installed tree and the
    # working tree — committed since, uncommitted now, or both.
    changed = subprocess.check_output(
        ["git", "-C", str(PROJECT_ROOT), "diff", "--name-only", last_sha],
    ).decode().splitlines()
    # Untracked files (`git diff` can't see them).
    untracked = subprocess.check_output(
        ["git", "-C", str(PROJECT_ROOT),
         "ls-files", "--others", "--exclude-standard"],
    ).decode().splitlines()
    all_paths = list(dict.fromkeys(changed + untracked))  # dedup, preserve order
    if not all_paths:
        # Nothing changed — treat as data-only (no-op fast path fine).
        return "data-only"
    for path in all_paths:
        if not any(path.startswith(prefix) for prefix in DATA_ONLY_PREFIXES):
            log(f"scope=code — changed outside data prefixes: {path}")
            return "code"
    return "data-only"


def _record_install_sha() -> None:
    """Persist a marker describing what was actually installed, for the
    next `classify_change_scope()` call. Best-effort; failure is non-fatal.

    The marker must describe HEAD **plus the working tree**, not bare
    HEAD. The /polish protocol deploys at Step 5 and commits at Step 6,
    so an install routinely carries uncommitted changes. Recording bare
    HEAD made the very next run re-see those same changes — now
    committed — as brand-new code and take the full-reinstall path for
    something already installed. That cost is not academic: a full
    reinstall kills the IME process, and host apps holding a live IMK
    session (WeChat) go "switches fine, types nothing" until restarted.
    Observed 2026-08-05: the framework commit installed dirty at
    648549c3, got committed as c7b24786, and the next (pure-data)
    polish re-read `inputx-pinyin-v2/src/lib.rs` out of that diff.

    `git stash create` builds a commit object for the current working
    tree WITHOUT touching the tree, the index, or any ref — exactly the
    "what did I just install" snapshot we need. It prints nothing when
    the tree is clean, in which case HEAD already describes the install.

    The commit it makes is dangling, so `git gc` can eventually prune
    it. `classify_change_scope()` already treats a missing sha as
    "unknown" → full reinstall, so the decay path is the old behavior,
    never something worse.
    """
    try:
        SNAPSHOT_DIR.mkdir(parents=True, exist_ok=True)
        stashed = subprocess.check_output(
            ["git", "-C", str(PROJECT_ROOT), "stash", "create"],
        ).decode().strip()
        marker = stashed or _current_head_sha()
        LAST_INSTALL_SHA.write_text(marker + "\n")
    except (OSError, subprocess.CalledProcessError) as e:
        log(f"warn: couldn't record install sha: {e}")


def do_hot_reload_data() -> None:
    """v1.15 hot-reload fast path.

    Regenerate pinyin.dict + words.idf (polish-rebuild), atomically
    swap them into the installed bundle's Contents/Resources/data/,
    codesign --force to keep hardened-runtime happy, then send SIGUSR1
    to the running Inputx process so it re-parses in place. No
    IMKClient churn → user's active preedit / typing survives.

    Fall back to a full reinstall when:
    - the bundle isn't installed yet (`APP_DST.exists() is False`);
    - the running Inputx process can't be located (no PID for USR1);
    - any single-step in the fast path errors — safer to reset than
      leave the bundle half-swapped.
    """
    if not APP_DST.exists():
        die("bundle not installed yet — run `mac/reinstall.py` first")
    data_dir = APP_DST / "Contents" / "Resources" / "data"
    if not data_dir.exists():
        die(f"{data_dir} missing — bundle predates Phase B (v1.15). "
            "Run a full `mac/reinstall.py` to install a Phase-B bundle first.")

    log("(1/4) polish-rebuild — regenerate pinyin.dict + words.idf")
    project_root = PROJECT_ROOT
    subprocess.run(
        ["make", "polish-rebuild"],
        cwd=str(project_root),
        check=True,
    )

    log("(2/4) atomic swap of Contents/Resources/data/")
    sources = {
        "pinyin.dict": project_root
            / "core/crates/inputx-pinyin-data-core/data/pinyin.dict",
        "words.idf": project_root
            / "core/crates/inputx-pinyin-helpers/data/words.idf",
        "bigrams.ngm": project_root
            / "core/crates/inputx-pinyin-helpers/data/bigrams.ngm",
        "bigrams_inter.ngm": project_root
            / "core/crates/inputx-pinyin-helpers/data/bigrams_inter.ngm",
        # v1.17: the wubi + nihongo tables. Until these shipped here, the
        # only way to change them was to replace the binary — which is why
        # a wubi or JP polish forced a full reinstall, and (while the
        # whitelist wrongly admitted them) could take the fast path and
        # deliver nothing at all. Note the rename: wubi and pinyin both
        # call their IDF `words.idf` in-crate, so wubi's lands here as
        # `wubi.idf` to keep the flat data dir unambiguous.
        "wubi.idf": project_root
            / "core/crates/inputx-wubi-data/data/words.idf",
        "wubi86.dict": project_root
            / "core/crates/inputx-wubi-data/data/wubi86.dict",
        "kanji.idf": project_root
            / "core/crates/inputx-nihongo-data-kanji/data/kanji.idf",
        "jukugo.idf": project_root
            / "core/crates/inputx-nihongo-data-jukugo/data/jukugo.idf",
    }
    # v1.16 hot-reload: also swap the 6 polish overlay TSVs into
    # `data/polish/`. v2 engine reads these via ArcSwap so the
    # running Inputx picks up polish additions without a binary
    # rebuild. Pre-v1.16 v2 used compile-time `include_str!` for
    # these TSVs, making SIGUSR1 a no-op for anything v2-flavored.
    polish_sources = {
        "tier_overlay.tsv": project_root
            / "tools/scoring/data/polish/tier_overlay.tsv",
        "quickfix_boost.tsv": project_root
            / "tools/scoring/data/polish/quickfix_boost.tsv",
        "exclusions_v1.tsv": project_root
            / "tools/scoring/data/polish/exclusions_v1.tsv",
        "prior_corrections_v1.tsv": project_root
            / "tools/scoring/data/polish/prior_corrections_v1.tsv",
        "modern_vocab_v1.tsv": project_root
            / "tools/scoring/data/polish/modern_vocab_v1.tsv",
        "corpus_garbage_filter_v1.tsv": project_root
            / "tools/scoring/data/polish/corpus_garbage_filter_v1.tsv",
    }
    for name, src in sources.items():
        if not src.exists():
            die(f"missing source {src} — polish-rebuild output layout changed?")
    for name, src in polish_sources.items():
        if not src.exists():
            die(f"missing polish source {src}")
    staging = data_dir.parent / "data.new"
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True)
    for name, src in sources.items():
        shutil.copy2(src, staging / name)
    polish_stage = staging / "polish"
    polish_stage.mkdir()
    for name, src in polish_sources.items():
        shutil.copy2(src, polish_stage / name)
    # Manifest for reinstall.py's future diff-mode (Phase D).
    with (staging / "manifest.sha256").open("w") as fh:
        for name in sources:
            digest = subprocess.check_output(
                ["shasum", "-a", "256", str(staging / name)]
            ).decode()
            fh.write(digest)
    with (polish_stage / "manifest.sha256").open("w") as fh:
        for name in polish_sources:
            digest = subprocess.check_output(
                ["shasum", "-a", "256", str(polish_stage / name)]
            ).decode()
            fh.write(digest)
    # Atomic swap: old data/ → data.old, staging → data/.
    old = data_dir.parent / "data.old"
    if old.exists():
        shutil.rmtree(old)
    os.rename(data_dir, old)
    os.rename(staging, data_dir)
    shutil.rmtree(old)

    log("(3/4) codesign --force (hardened-runtime bundle integrity)")
    subprocess.run(
        ["codesign", "--force", "--deep", "--options", "runtime",
         "--sign", "-", str(APP_DST)],
        check=False,  # sign-with-ad-hoc may fail on a Developer-signed bundle;
                     # a full re-sign is only needed to satisfy hardened-runtime
                     # inspection, which macOS re-runs on next process launch,
                     # not on the currently-running one. Non-fatal.
    )

    log("(4/4) kill -USR1 Inputx (signal running process to re-parse)")
    try:
        out = subprocess.check_output(["pgrep", "-f", PROCESS_PATTERN])
        pids = [int(p) for p in out.decode().split()]
    except subprocess.CalledProcessError:
        log("Inputx not running — nothing to signal; next launch will read new files")
        return
    if not pids:
        log("Inputx not running — nothing to signal")
        return
    for pid in pids:
        os.kill(pid, signal.SIGUSR1)
        log(f"→ SIGUSR1 sent to pid {pid}")
    log("✓ hot-reload dispatched; watch `log stream --process Inputx` for "
        "'SIGUSR1 → dict reload broadcast'")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__,
                                      formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--no-build", action="store_true",
                        help="skip cargo + swiftc rebuild")
    parser.add_argument("--no-safe", action="store_true",
                        help="skip backup snapshot + binary probe-test")
    parser.add_argument("--clean", action="store_true",
                        help="uninstall bundle + all TIS rows (migration)")
    # v1.15 hot-reload: swap pinyin.dict / words.idf / bigrams*.ngm
    # inside the installed bundle in place, then send SIGUSR1 to the
    # running Inputx process so it re-parses without exit. Keeps every
    # host app's IMKClient Mach-port alive — user's active preedit
    # survives the polish.
    parser.add_argument("--force-hot-reload", action="store_true",
                        help="Manual test: run hot-reload path regardless of "
                             "the auto scope check")
    parser.add_argument("--no-hot-reload", action="store_true",
                        help="Disable the auto data-only branch; always run "
                             "the full reinstall + pkill flow (fallback if a "
                             "hot-reload flow breaks in production)")
    parser.add_argument("--rehearse", action="store_true",
                        help="Print the auto-detected scope + polish-rebuild "
                             "target files then exit — no SIGUSR1, no swap")
    args = parser.parse_args()

    if args.clean:
        clean_tis_state()
        return

    if args.force_hot_reload:
        do_hot_reload_data()
        _record_install_sha()
        return

    scope = classify_change_scope()
    log(f"change scope since last install: {scope}")
    if args.rehearse:
        log(f"--rehearse: scope={scope}; "
            f"would take {'hot-reload' if scope == 'data-only' else 'full reinstall'} "
            f"path. Exiting without action.")
        return
    if scope == "data-only" and APP_DST.exists() and not args.no_hot_reload:
        log("→ data-only fast path (hot-reload; user's typing sessions stay alive)")
        do_hot_reload_data()
        _record_install_sha()
        return

    # From here on the IME process gets replaced, which severs the Mach
    # connection every host app's IMK client is holding. Apps re-bind on
    # their next activation, but ones that keep a long-lived input
    # session — WeChat is the reliable offender — end up able to SWITCH
    # to us and unable to TYPE, with a full app restart as the only
    # recovery. Say so up front instead of letting the user rediscover it.
    log("→ full reinstall (the IME process is replaced)")
    log("  note: an app you were actively typing in may switch but not "
        "type until you fully quit and reopen it (WeChat does this)")

    mode, state = classify_state()
    if mode == "corrupt":
        die(
            "TIS state is corrupted:\n"
            f"  {describe_corruption(state)}\n"
            "Run `mac/reinstall.py --clean` to uninstall + clean TIS, "
            "then `mac/reinstall.py` to fresh-install."
        )

    log(f"detected mode: {mode}")
    with_build = not args.no_build

    if mode == "first":
        run_fn = lambda: do_first_install(with_build)
    else:
        run_fn = lambda: do_reinstall(with_build)

    if args.no_safe:
        run_fn()
    else:
        with_safety_net(run_fn)
    # v1.15 hot-reload: record HEAD as the marker so the next
    # reinstall's classify_change_scope() can tell "data-only since"
    # from "code changed since". Only reached when the full reinstall
    # above returned cleanly.
    _record_install_sha()


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        die("interrupted", code=signal.SIGINT + 128)
