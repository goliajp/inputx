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
