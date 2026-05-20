import Cocoa

/// Custom `NSApplication` subclass — referenced by `Info.plist`'s
/// `NSPrincipalClass` (`Inputx.InputxApplication`).
///
/// **Required for IMKit**, not optional. The standard `NSApplication` +
/// "set delegate at top-level after .shared" pattern leaves a window
/// where InputMethodKit instantiates the app and probes its delegate
/// before the top-level code has wired one up — which results in the
/// IMK server failing to register and the input-source picker silently
/// dropping the bundle from its enumeration.
///
/// Mirrors the canonical pattern from Apple's IMKit examples + the
/// `ensan-hcl/macOS_IMKitSample_2021` reference project: instantiate the
/// `AppDelegate` inside `init()` so it's already attached the first time
/// any IMK code touches `NSApp.delegate`.
@objc(InputxApplication)
final class InputxApplication: NSApplication {
    private let appDelegate = AppDelegate()

    override init() {
        super.init()
        self.delegate = appDelegate
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not implemented for InputxApplication")
    }
}
