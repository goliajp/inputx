import Cocoa
import InputMethodKit

let kConnectionName = "Inputx_Connection"

class AppDelegate: NSObject, NSApplicationDelegate {
    var server: IMKServer?
    var menubar: MenubarSettings?

    func applicationDidFinishLaunching(_ note: Notification) {
        InputxSettings.registerDefaults()
        // Pin process-global rare-CJK toggle to the persisted pref so newly
        // spawned `InputxController` instances inherit the same value.
        InputxRareChars.enabled = InputxSettings.showRareChars

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

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.run()
