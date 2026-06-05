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


def binary_probe_test(bundle: Path, *, timeout_s: float = 15.0) -> tuple[bool, str]:
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


def query_enabled_sources_count() -> int:
    r = subprocess.run(
        ["defaults", "read", "com.apple.HIToolbox", "AppleEnabledInputSources"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, check=False,
    )
    if r.returncode != 0:
        return 0
    return r.stdout.count(BUNDLE_ID)


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


def invalidate_intl_data_cache() -> None:
    """Delete macOS 26's TIS enumeration cache.

    The picker reads `$DARWIN_USER_CACHE_DIR/com.apple.IntlDataCache.le*`
    rather than re-querying TIS on every list. After a bundle swap,
    the cache is stale; nothing else invalidates it (killing
    TextInputMenuAgent, lsregister -f, FSEvents, and the private
    TISUpdateIntlFileCache() symbol all leave it untouched).
    Documented in docs/macos-ime-recipe-2026.md.
    """
    r = subprocess.run(
        ["getconf", "DARWIN_USER_CACHE_DIR"],
        stdout=subprocess.PIPE, text=True, check=True,
    )
    cache_dir = Path(r.stdout.strip())
    if not cache_dir.exists():
        die(f"DARWIN_USER_CACHE_DIR doesn't exist: {cache_dir}")
    for stem in ("com.apple.IntlDataCache.le", "com.apple.IntlDataCache.le.kbdx"):
        p = cache_dir / stem
        if p.exists():
            p.unlink()


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
    """Silent reinstall: atomic bundle swap + LS refresh. We kill any
    running Inputx process at the END so the next host-app use lazy-
    spawns the new bundle via imklaunchagent (the canonical and ONLY
    spawn path).

    Explicitly NOT done here (would create duplicates / re-trigger TCC):
      - `Inputx install` / TISRegisterInputSource (Settings owns it)
      - AppleEnabledInputSources writes (Settings owns it)
      - LaunchAgent install (retired 2026-06-06 — was a workaround for
        a refusal scenario that no longer exists)
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
    invalidate_intl_data_cache()
    restart_text_input_menu_agent()
    # kill last: keeps the old binary serving host apps for the brief
    # duration of all the above steps, then this triggers imklaunchagent
    # to lazy-spawn fresh on next use with all updated state in place.
    stop_running_ime()
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

    # TIS `enabled` is the authoritative live signal — when it's true,
    # the picker can select us and host apps can drive our IMKServer.
    # `defaults read AppleEnabledInputSources` is NOT a reliable check
    # on macOS 26: the HIToolbox cfprefsd domain caches aggressively
    # and lags behind the real plist by minutes after Settings UI
    # writes. Verified 2026-06-05 on a fresh-device install where the
    # IME was empirically typing into apps but `defaults read` showed
    # zero entries.
    if not mode_rows[0].enabled:
        die(f"post-condition violation: TIS row for {MODE_ID} exists "
            "but is disabled. Re-enable via System Settings → Keyboard "
            "→ Input Sources, or run --clean and reinstall fresh.")


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
        backup = APP_DST.parent / f"{APP_NAME}.app.bak-{int(time.time())}"
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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__,
                                      formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--no-build", action="store_true",
                        help="skip cargo + swiftc rebuild")
    parser.add_argument("--no-safe", action="store_true",
                        help="skip backup snapshot + binary probe-test")
    parser.add_argument("--clean", action="store_true",
                        help="uninstall bundle + all TIS rows (migration)")
    args = parser.parse_args()

    if args.clean:
        clean_tis_state()
        return

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


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        die("interrupted", code=signal.SIGINT + 128)
