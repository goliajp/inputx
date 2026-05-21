import Foundation
import Carbon

// Try filtering by our bundle ID
let filterBundleID = "jp.golia.inputmethod.wubi"
let props = [kTISPropertyBundleID: filterBundleID] as CFDictionary
if let list = TISCreateInputSourceList(props, true)?.takeRetainedValue() as? [TISInputSource] {
    print("filtered by bundle id: \(list.count)")
    for src in list {
        func get(_ key: CFString) -> String? {
            guard let p = TISGetInputSourceProperty(src, key) else { return nil }
            return Unmanaged<CFString>.fromOpaque(p).takeUnretainedValue() as String
        }
        let id = get(kTISPropertyInputSourceID) ?? "?"
        let name = get(kTISPropertyLocalizedName) ?? "?"
        let bundleID = get(kTISPropertyBundleID) ?? "?"
        let cat = get(kTISPropertyInputSourceCategory) ?? "?"
        print("  id=\(id) name=\(name) bundle=\(bundleID) cat=\(cat)")
    }
}

// Also dump unique categories, to see what's expected
if let allList = TISCreateInputSourceList(nil, true)?.takeRetainedValue() as? [TISInputSource] {
    var cats = Set<String>()
    for src in allList {
        if let p = TISGetInputSourceProperty(src, kTISPropertyInputSourceCategory) {
            cats.insert(Unmanaged<CFString>.fromOpaque(p).takeUnretainedValue() as String)
        }
    }
    print("\nall categories seen: \(cats)")
}
