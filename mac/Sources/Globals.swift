import Foundation
import InputxKit

// Module-level singletons consumed by the IMK controller + menubar. Keeps
// the host-specific config (UserDefaults instance, on-disk L0 path) in one
// place; both the menubar UI and any spawned IMK client instance read from
// these globals.
//
// macOS choices:
//   - UserDefaults.standard is bundle-scoped, so the IME's prefs are
//     automatically isolated from other apps.
//   - L0 lives under `~/Library/Application Support/Inputx/`, the
//     conventional macOS spot for per-user app data.

let inputxSettings: InputxSettings = {
    let s = InputxSettings(defaults: .standard)
    s.registerDefaults()
    return s
}()

let inputxL0Storage = InputxL0Storage.userApplicationSupport(bundleName: "Inputx")
