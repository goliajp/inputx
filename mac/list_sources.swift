// Diagnostic: dump the TIS database entries for Inputx.
// Usage: `swift mac/list_sources.swift`
// 0 hits + a fresh install = check `~/Library/Input Methods/Inputx.app`,
// the bundle's code-sign state, and the IntlDataCache files described in
// docs/macos-ime-debugging.md (delete to force TIS rebuild).
import Foundation
import Carbon

let bundleID = "jp.golia.inputmethod.wubi"
let filter = [kTISPropertyBundleID: bundleID] as CFDictionary

guard let list = TISCreateInputSourceList(filter, true)?.takeRetainedValue() as? [TISInputSource] else {
    print("no TIS sources for \(bundleID)")
    exit(0)
}
print("\(list.count) TIS source(s) for \(bundleID):")
for src in list {
    func get(_ key: CFString) -> String {
        guard let p = TISGetInputSourceProperty(src, key) else { return "?" }
        return Unmanaged<CFString>.fromOpaque(p).takeUnretainedValue() as String
    }
    print("  \(get(kTISPropertyInputSourceID))  name=\(get(kTISPropertyLocalizedName))  cat=\(get(kTISPropertyInputSourceCategory))")
}
