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

/// CP-5.2 step-3 — bundled cell-dict pack registry, scanned once at
/// process launch. Reads `Resources/cell-dicts/*.toml` from the IME
/// bundle (where `mac/Resources/cell-dicts/` lands at build time). The
/// SettingsWindow UI iterates over `all` to render one toggle per pack;
/// IMEController re-reads `inputxSettings.enabledCellDictPackIds` on
/// every `applySettingsToSession()` and uses the cache to load only
/// the enabled subset.
///
/// Kept here (not inside InputxKit) because pack-source-of-truth is a
/// host-bundle concern — iOS will scan the App Group container instead
/// of the bundle when its container app ships in a later phase.
final class InputxCellDictPacksCache {
    static let shared = InputxCellDictPacksCache()
    let all: [InputxCellDictPack]
    private init() {
        self.all = InputxCellDictRegistry.bundled(in: .main)
    }
}
