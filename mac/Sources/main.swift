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
    // TISRegisterInputSource policy:
    //  - 0 rows matching expected mode IDs → fresh install, register.
    //  - exactly 1 row per expected mode ID → already correct, do nothing.
    //  - 2+ rows for an expected mode ID → previous-policy duplicates;
    //    disable extras + re-register.
    //
    // PLUS: orphan cleanup. macOS TIS may carry rows for our bundle
    // ID under STALE mode IDs from earlier bundle versions (notably
    // the `jp.golia.inputmethod.wubi.wubi.zh` doubled-segment ID that
    // showed up before the per-mode `TISInputSourceID` was added —
    // 2026-06-02 fix). Any row whose ID is OURS by bundle prefix but
    // is NOT in the current expected `modeIDs` set is an orphan from
    // a prior bundle layout; disable it so it can't conflict with the
    // clean re-registration. The bundle's top-level `TISInputSourceID`
    // is also expected and skipped (it's not a selectable mode).
    let bundleID = Bundle.main.bundleIdentifier ?? ""
    let topLevelID = Bundle.main.infoDictionary?["TISInputSourceID"] as? String ?? ""
    let isOurs: (String) -> Bool = { id in
        id == bundleID || id == topLevelID || id.hasPrefix(bundleID + ".")
    }
    func srcID(_ src: TISInputSource) -> String? {
        guard let p = TISGetInputSourceProperty(src, kTISPropertyInputSourceID) else { return nil }
        return Unmanaged<CFString>.fromOpaque(p).takeUnretainedValue() as String
    }
    var orphansDisabled = 0
    for src in all {
        guard let id = srcID(src) else { continue }
        if isOurs(id) && id != topLevelID && !modeIDs.contains(id) {
            _ = TISDisableInputSource(src)
            orphansDisabled += 1
        }
    }
    if orphansDisabled > 0 {
        NSLog("Inputx install: disabled \(orphansDisabled) orphan TIS row(s) from prior bundle layouts")
    }

    let alreadyRegistered = all.filter(match)
    var duplicatesByID: [String: Int] = [:]
    for src in alreadyRegistered {
        guard let id = srcID(src) else { continue }
        duplicatesByID[id, default: 0] += 1
    }
    let hasDuplicates = duplicatesByID.values.contains { $0 > 1 }
    if hasDuplicates {
        NSLog("Inputx install: detected duplicate TIS rows, disabling + re-registering")
        for src in alreadyRegistered {
            _ = TISDisableInputSource(src)
        }
    }
    if alreadyRegistered.isEmpty || hasDuplicates {
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
    NSLog("Inputx install: \(postRegister.count) mode(s) registered + enabled "
        + "(had \(alreadyRegistered.count) pre-existing, duplicates=\(hasDuplicates), "
        + "orphans cleaned=\(orphansDisabled))")
    exit(0)
}

// MUST match Info.plist `InputMethodConnectionName` AND the entitlement's
// `com.apple.security.temporary-exception.mach-register.global-name`.
let kConnectionName = "jp.golia.inputmethod.wubi_Connection"

class AppDelegate: NSObject, NSApplicationDelegate {
    var server: IMKServer?
    var menubar: MenubarSettings?

    func applicationDidFinishLaunching(_ note: Notification) {
        // Pin process-global rare-CJK toggle so spawned InputxController
        // instances inherit the persisted pref.
        InputxRareChars.enabled = inputxSettings.showRareChars

        guard let bundleID = Bundle.main.bundleIdentifier else {
            NSLog("Inputx: missing bundle identifier")
            exit(1)
        }
        server = IMKServer(name: kConnectionName, bundleIdentifier: bundleID)
        // Settings entry points (in order of discoverability):
        //   1. Click the active input source in the macOS menu bar (the
        //      one labelled "入 Inputx 五笔") — IMKInputController.menu()
        //      override on `InputxController` injects "Inputx 设置…" as
        //      the first item there.
        //   2. NSStatusItem in the menu bar (`MenubarSettings`) — visible
        //      when the user doesn't have menu-bar auto-hide on.
        //
        // We deliberately do NOT auto-open the Settings window from
        // `applicationShouldHandleReopen` / `applicationOpenUntitledFile`
        // because macOS dispatches those events during LaunchServices /
        // IMK activation cycles too, which means every `launchctl bootout
        // + bootstrap` (every dev reinstall, every system reboot) was
        // popping the window. The IMK-menu entry covers the discoverability
        // need without the side-effect.
        menubar = MenubarSettings()
    }
}

// Wire the delegate to NSApplication.shared BEFORE .run() so IMK's delegate
// probe during server bring-up never sees a nil delegate.
let delegate = AppDelegate()
let app = NSApplication.shared
app.delegate = delegate
app.run()
