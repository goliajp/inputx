import Cocoa
import InputMethodKit
import InputxKit

// Mach service name — MUST match Info.plist's `InputMethodConnectionName`
// AND the `com.apple.security.temporary-exception.mach-register.global-name`
// entitlement value. Apple convention: `<bundleID>_Connection`.
let kConnectionName = "jp.golia.inputx_Connection"

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

// Force-load the InputxApplication subclass — Info.plist's NSPrincipalClass
// (`Inputx.InputxApplication`) points at it, and Cocoa's bootstrap creates
// the instance from that string. We don't manually `NSApplication.shared` /
// `.run()` here: the subclass wires the delegate in its `init()` and then
// `NSApplicationMain` runs the loop.
_ = NSApplicationMain(CommandLine.argc, CommandLine.unsafeArgv)
