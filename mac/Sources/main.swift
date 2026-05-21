import Carbon
import Cocoa
import InputMethodKit
import InputxKit

// `Inputx install` registers the bundle with the TIS database and enables
// each declared input mode. install.sh runs this immediately after copying
// the .app into place so the IME shows up in the picker AND lands in the
// user's enabled input-source list in one step.
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
    let alreadyRegistered = all.filter(match)
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
        // Menubar status item is the only user-facing settings surface —
        // there's no container app on macOS.
        menubar = MenubarSettings()
    }
}

// Wire the delegate to NSApplication.shared BEFORE .run() so IMK's delegate
// probe during server bring-up never sees a nil delegate.
let delegate = AppDelegate()
let app = NSApplication.shared
app.delegate = delegate
app.run()
