import UIKit
import SwiftUI

// `inputxSharedDefaults` is declared in Keyboard/InputxCore.swift and reused
// here now that InputxApp compiles the Keyboard/ sources too (so it can
// instantiate KeyboardViewController for the in-app debug keyboard).

@main
class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        // Maestro test harness hook: when launched with -inputx-reset-defaults,
        // wipe App Group state back to first-install defaults BEFORE SwiftUI
        // reads any of it. Writing through UserDefaults (rather than direct
        // PlistBuddy edits) is the only way to invalidate cfprefsd's in-process
        // cache so subsequent reads by relaunched keyboard/host see fresh
        // values. Direct plist edits leave cfprefsd serving stale values.
        if CommandLine.arguments.contains("-inputx-reset-defaults") {
            if let defs = inputxSharedDefaults {
                defs.set(3,    forKey: "autoCommitPolicy")
                defs.set(0,    forKey: "engineMode")
                defs.set(true, forKey: "useCjkPunct")
                defs.set(false, forKey: "useFullWidth")
                defs.set(false, forKey: "showRareChars")
                defs.set(true,  forKey: "showSourceIndicator")
                defs.removeObject(forKey: "inputx_recent_emojis")
                defs.synchronize()
            }
        }

        let window = UIWindow(frame: UIScreen.main.bounds)
        // Phase 8 (item 70) — SwiftUI ContentView replaces v0.1
        // UIKit MainViewController.
        window.rootViewController = UIHostingController(rootView: SettingsView())
        window.makeKeyAndVisible()
        self.window = window
        return true
    }
}
