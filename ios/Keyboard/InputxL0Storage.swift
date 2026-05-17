import Foundation

/// File-system helpers for L0 (per-user learning) snapshot persistence in
/// the App Group container `group.jp.golia.inputx`.
///
/// The Keyboard extension writes JSON snapshots on viewWillDisappear
/// (item 76) and reads them on viewDidLoad (item 77). The InputxApp can
/// also read/write these files via the Settings UI for export / import /
/// reset (item 74).
///
/// Path layout:
///     <AppGroupContainer>/Library/Application Support/inputx/
///         wubi_l0.json
///         pinyin_l0.json
///
/// File contents are versioned JSON produced by `inputx_session_export_l0_json`
/// (inputx-core); see `composite/l0_json.rs` for the schema.
enum InputxL0Storage {
    static let appGroupID = "group.jp.golia.inputx"
    static let wubiEngineRaw: UInt8 = 0
    static let pinyinEngineRaw: UInt8 = 1

    static func containerDir() -> URL? {
        guard let container = FileManager.default
            .containerURL(forSecurityApplicationGroupIdentifier: appGroupID)
        else {
            return nil
        }
        let dir = container
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent("Application Support", isDirectory: true)
            .appendingPathComponent("inputx", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    static func l0FileURL(engineRawValue: UInt8) -> URL? {
        guard let dir = containerDir() else { return nil }
        let name: String
        switch engineRawValue {
        case wubiEngineRaw:   name = "wubi_l0.json"
        case pinyinEngineRaw: name = "pinyin_l0.json"
        default:              return nil
        }
        return dir.appendingPathComponent(name)
    }

    /// Read JSON content for the given engine; `nil` if file missing or unreadable.
    static func readL0Json(engineRawValue: UInt8) -> String? {
        guard let url = l0FileURL(engineRawValue: engineRawValue),
              FileManager.default.fileExists(atPath: url.path) else {
            return nil
        }
        return try? String(contentsOf: url, encoding: .utf8)
    }

    /// Write JSON atomically. Returns success.
    @discardableResult
    static func writeL0Json(_ json: String, engineRawValue: UInt8) -> Bool {
        guard let url = l0FileURL(engineRawValue: engineRawValue) else { return false }
        do {
            try json.write(to: url, atomically: true, encoding: .utf8)
            return true
        } catch {
            print("[InputxL0Storage] write failed: \(error)")
            return false
        }
    }

    /// Delete both engines' L0 files. Returns true if any deletion happened.
    @discardableResult
    static func resetAll() -> Bool {
        var any = false
        for raw in [wubiEngineRaw, pinyinEngineRaw] {
            if let url = l0FileURL(engineRawValue: raw),
               FileManager.default.fileExists(atPath: url.path) {
                try? FileManager.default.removeItem(at: url)
                any = true
            }
        }
        return any
    }
}
