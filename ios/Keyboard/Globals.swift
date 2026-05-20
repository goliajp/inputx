import Foundation
import InputxKit

// App Group container shared between the InputxApp container app and the
// InputxKeyboard extension. Both processes consume the same L0 file store
// through `inputxL0Storage`, so user-learning written by one is read by
// the other.
//
// File compiles into both targets (App + Keyboard) since project.yml
// includes `path: Keyboard` in both. The module-level `let` evaluates per
// process at first access.

let inputxAppGroupID = "group.jp.golia.inputx"

/// L0 user-learning storage, rooted in the App Group container.
/// Falls back to a process-local Documents-dir path if the App Group
/// entitlement isn't present (defensive — should never happen in a
/// correctly signed build).
let inputxL0Storage: InputxL0Storage = {
    if let storage = InputxL0Storage.appGroup(inputxAppGroupID) {
        return storage
    }
    NSLog("[Inputx] App Group container missing — falling back to local Documents")
    let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!
    return InputxL0Storage(directoryURL: docs.appendingPathComponent("inputx", isDirectory: true))
}()

/// App Group `UserDefaults` instance — shared by SwiftUI `@AppStorage`
/// (container app) and direct reads (keyboard extension's
/// `KeyboardViewController`). Same keys as the iOS Settings UI uses.
let inputxSharedDefaults: UserDefaults =
    UserDefaults(suiteName: inputxAppGroupID) ?? .standard
