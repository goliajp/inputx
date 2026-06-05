import Carbon
import Cocoa
import InputMethodKit
import InputxKit

// `Inputx install` registers the bundle with the TIS database and enables
// each declared input mode. install.sh runs this immediately after copying
// the .app into place so the IME shows up in the picker AND lands in the
// user's enabled input-source list in one step.
// DIAGNOSTIC 2026-05-26 — direct candidate dump bypassing IMK/LaunchAgent.
// Verifies whether the mac binary's bundled inputx-core gives the same
// candidate order as cli inputx-probe. If yes: IME-layer caching/timing
// bug. If no: the mac binary links a different (stale) inputx-core.
if CommandLine.arguments.count >= 3, CommandLine.arguments[1] == "probe" {
    let buf = CommandLine.arguments[2]
    let sess = InputxSession()
    sess.setEngineMode(.mixed)
    sess.setJapaneseEnabled(true)
    for codepoint in buf.unicodeScalars {
        _ = sess.handleKey(codepoint: codepoint.value, modifiers: InputxModifiers(rawValue: 0))
    }
    let n = min(sess.candidateCount, 5)
    for i in 0..<n {
        let w = sess.candidate(at: i) ?? "?"
        let s = sess.candidateSource(at: i)
        print("#\(i+1) \(w) (\(s))")
    }
    exit(0)
}

if CommandLine.arguments.count >= 2, CommandLine.arguments[1] == "install" {
    let modeIDs: [String] = {
        guard let comp = Bundle.main.infoDictionary?["ComponentInputModeDict"] as? [String: Any],
              let list = comp["tsInputModeListKey"] as? [String: Any]
        else { return [] }
        return Array(list.keys)
    }()
    let all = (TISCreateInputSourceList(nil, true)?.takeRetainedValue() as? [TISInputSource]) ?? []
    func match(_ src: TISInputSource) -> Bool {
        guard let p = TISGetInputSourceProperty(src, kTISPropertyInputSourceID) else { return false }
        let id = Unmanaged<CFString>.fromOpaque(p).takeUnretainedValue() as String
        return modeIDs.contains(id)
    }
    // TISRegisterInputSource policy — single contract:
    //   - 0 rows match expected mode IDs → register (this is the
    //     first-install path; macOS will prompt for TCC consent).
    //   - exactly 1 row per expected mode ID → already registered,
    //     do not re-register (TIS appends; re-register creates
    //     duplicates).
    //   - any other state (duplicates, orphan IDs with our bundle
    //     prefix, etc.) → FAIL. Caller (mac/reinstall.py) detects
    //     this earlier and instructs the user to run `--clean`.
    //
    // Pre-2026-06-02 commits had orphan cleanup + duplicate dedupe
    // here as defensive bandaids for a different bug (missing
    // TISInputSourceID in Info.plist + an "always re-register"
    // policy that compounded the mess). Per project rule
    // (no-defensive-programming): root cause fixed in d6cdc52,
    // bandaids removed here.
    let alreadyRegistered = all.filter(match)
    if alreadyRegistered.count > modeIDs.count {
        NSLog("Inputx install: REFUSING to install — TIS has "
            + "\(alreadyRegistered.count) rows for \(modeIDs.count) "
            + "expected mode IDs (duplicates or orphans). Run "
            + "`mac/reinstall.py --clean` then retry.")
        exit(1)
    }
    if alreadyRegistered.isEmpty {
        let status = TISRegisterInputSource(Bundle.main.bundleURL as CFURL)
        guard status == noErr else {
            NSLog("Inputx install: TISRegisterInputSource failed OSStatus=\(status)")
            exit(1)
        }
    }
    let postRegister = ((TISCreateInputSourceList(nil, true)?.takeRetainedValue() as? [TISInputSource]) ?? [])
        .filter(match)
    for src in postRegister {
        TISEnableInputSource(src)
    }
    NSLog("Inputx install: \(postRegister.count) mode(s) registered + enabled")
    exit(0)
}

// MUST match Info.plist `InputMethodConnectionName` AND the entitlement's
// `com.apple.security.temporary-exception.mach-register.global-name`.
let kConnectionName = "jp.golia.inputmethod.wubi_Connection"

class AppDelegate: NSObject, NSApplicationDelegate {
    var server: IMKServer?

    func applicationDidFinishLaunching(_ note: Notification) {
        // Pin process-global rare-CJK toggle so spawned InputxController
        // instances inherit the persisted pref.
        InputxRareChars.enabled = inputxSettings.showRareChars

        guard let bundleID = Bundle.main.bundleIdentifier else {
            NSLog("Inputx: missing bundle identifier")
            exit(1)
        }
        server = IMKServer(name: kConnectionName, bundleIdentifier: bundleID)
        // Settings entry point: click the active input source in the macOS
        // menu bar (the "Inputx Wubi" item next to the keyboard layout
        // icon). `InputxController.menu()` hosts every toggle / radio /
        // action — see IMEController.swift `// MARK: - System input-source
        // menu integration`.
        //
        // The pre-2026-06-06 NSStatusItem ("五" status item with its own
        // dropdown) was retired here per user request: it duplicated every
        // entry of the IMK menu, cluttered the menu bar, and visually
        // collided with the system input-source indicator. Apple-canonical
        // IME behavior: settings live ONLY inside IMKInputController.menu().
        //
        // We deliberately do NOT auto-open the Settings window from
        // `applicationShouldHandleReopen` / `applicationOpenUntitledFile`
        // because macOS dispatches those events during LaunchServices /
        // IMK activation cycles too, which means every reinstall (dev or
        // system) was popping the window. The IMK-menu entry covers the
        // discoverability need without the side-effect.
    }
}

// Wire the delegate to NSApplication.shared BEFORE .run() so IMK's delegate
// probe during server bring-up never sees a nil delegate.
let delegate = AppDelegate()
let app = NSApplication.shared
app.delegate = delegate
app.run()
