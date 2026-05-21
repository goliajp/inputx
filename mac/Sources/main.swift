import Carbon
import Cocoa
import InputMethodKit
import InputxKit

// macOS 26's TextInputMenuAgent does not scan `/Library/Input Methods/` or
// `~/Library/Input Methods/` to discover newly-installed IMEs. The only way
// to land a bundle in the system TIS database is for the IME's own binary —
// running inside its own code-signature context — to call
// `TISRegisterInputSource(Bundle.main.bundleURL)`. External callers
// (Swift scripts, `lsregister`, manual `TISRegisterInputSource` invocations
// from another process) return `noErr` but never persist. install.sh runs
// `Inputx install` immediately after copying the bundle for exactly this
// reason; vChewing's pkg postinstall uses the same pattern.
if CommandLine.arguments.count >= 2, CommandLine.arguments[1] == "install" {
    // Mirror vChewing's `IMKHelper.registerInputMethod()` sequence verbatim:
    //   1. Compute our mode IDs from ComponentInputModeDict.tsInputModeListKey
    //   2. TISCreateInputSourceList(matching mode IDs) — triggers system TIS
    //      scan as a side effect (visible as TISFileInterrogator + Keyboard
    //      Layouts duplicate-identifier warnings in NSLog)
    //   3. If empty (first install), call TISRegisterInputSource(bundleURL)
    //   4. Then iterate any returned instances and TISEnableInputSource each
    // Our previous handler skipped step 2 and step 4. That was a guess that
    // bare register would suffice; vChewing's working `install` does more.
    let modeIDs: [String] = {
        guard let comp = Bundle.main.infoDictionary?["ComponentInputModeDict"] as? [String: Any],
              let list = comp["tsInputModeListKey"] as? [String: Any]
        else { return [] }
        return Array(list.keys)
    }()
    let conds: [CFString: Any] = [:]
    let cfDict = conds as CFDictionary
    var instances: [TISInputSource] = []
    if let all = TISCreateInputSourceList(cfDict, true)?.takeRetainedValue() as? [TISInputSource] {
        instances = all.filter { src in
            guard let p = TISGetInputSourceProperty(src, kTISPropertyInputSourceID) else { return false }
            let id = Unmanaged<CFString>.fromOpaque(p).takeUnretainedValue() as String
            return modeIDs.contains(id)
        }
    }
    if instances.isEmpty {
        let s = TISRegisterInputSource(Bundle.main.bundleURL as CFURL)
        NSLog("Inputx install: TISRegisterInputSource OSStatus=\(s) for \(Bundle.main.bundlePath)")
        if s != noErr { exit(1) }
    } else {
        NSLog("Inputx install: \(instances.count) instances already registered, skipping register")
    }
    // Re-query post-register and activate everything
    if let all = TISCreateInputSourceList(cfDict, true)?.takeRetainedValue() as? [TISInputSource] {
        let post = all.filter { src in
            guard let p = TISGetInputSourceProperty(src, kTISPropertyInputSourceID) else { return false }
            let id = Unmanaged<CFString>.fromOpaque(p).takeUnretainedValue() as String
            return modeIDs.contains(id)
        }
        for src in post {
            let e = TISEnableInputSource(src)
            NSLog("Inputx install: TISEnableInputSource OSStatus=\(e)")
        }
        NSLog("Inputx install: post-register query found \(post.count) instances")
    }
    exit(0)
}

// Mach service name — MUST match Info.plist's `InputMethodConnectionName`
// AND the `com.apple.security.temporary-exception.mach-register.global-name`
// entitlement value. Apple convention: `<bundleID>_Connection`.
let kConnectionName = "jp.golia.inputmethod.wubi_Connection"

class AppDelegate: NSObject, NSApplicationDelegate {
    var server: IMKServer?
    var menubar: MenubarSettings?

    func applicationDidFinishLaunching(_ note: Notification) {
        // `inputxSettings.registerDefaults()` already ran inside Globals.swift
        // on first access. Pin process-global rare-CJK toggle to the persisted
        // pref so newly spawned `InputxController` instances inherit the value.
        InputxRareChars.enabled = inputxSettings.showRareChars

        guard let bundleID = Bundle.main.bundleIdentifier else {
            NSLog("Inputx: missing bundle identifier")
            exit(1)
        }
        server = IMKServer(name: kConnectionName, bundleIdentifier: bundleID)
        // Status-bar item lives for the lifetime of the IME process — it's
        // the only user-facing settings surface on macOS (no container app).
        menubar = MenubarSettings()
        NSLog("Inputx: server up, bundle=\(bundleID)")
    }
}

// Standard NSApplication bootstrap: instantiate the delegate first, attach
// it to NSApplication.shared before .run(), then enter the run loop. Matches
// Apple's own IMEs (see /System/Library/Input Methods/*.app — all use
// NSPrincipalClass=NSApplication). A previous incarnation used a custom
// InputxApplication subclass to dodge an IMK delegate-probe race; setting
// the delegate before .run() defuses that race without a subclass.
let delegate = AppDelegate()
let app = NSApplication.shared
app.delegate = delegate
app.run()
