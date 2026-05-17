import Cocoa
import InputMethodKit

let kConnectionName = "Inputx_Connection"

class AppDelegate: NSObject, NSApplicationDelegate {
    var server: IMKServer?

    func applicationDidFinishLaunching(_ note: Notification) {
        guard let bundleID = Bundle.main.bundleIdentifier else {
            NSLog("Inputx: missing bundle identifier")
            exit(1)
        }
        server = IMKServer(name: kConnectionName, bundleIdentifier: bundleID)
        NSLog("Inputx: server up, bundle=\(bundleID)")
    }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.run()
