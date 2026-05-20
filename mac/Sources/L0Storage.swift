import Foundation

/// Reads / writes the per-engine L0 user-learning JSON files in
/// `~/Library/Application Support/Inputx/`.
///
/// Storage is best-effort: missing files, schema mismatches, or filesystem
/// errors are silently absorbed so the IME still launches on a corrupted
/// L0 store. The user keeps whatever the engine could recover.
enum InputxL0Storage {
    private static let dirURL: URL = {
        let support = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first!
        return support.appendingPathComponent("Inputx", isDirectory: true)
    }()

    private static func fileURL(for engine: InputxL0Engine) -> URL {
        let name: String
        switch engine {
        case .wubi:   name = "wubi_l0.json"
        case .pinyin: name = "pinyin_l0.json"
        }
        return dirURL.appendingPathComponent(name)
    }

    /// Read both engines' L0 into the session. Called once at IMK activate.
    /// Returns total accepted pins (wubi + pinyin); 0 on first launch.
    @discardableResult
    static func loadInto(_ session: InputxSession) -> Int {
        var total = 0
        for engine in [InputxL0Engine.wubi, .pinyin] {
            let url = fileURL(for: engine)
            guard let data = try? Data(contentsOf: url),
                  let json = String(data: data, encoding: .utf8)
            else { continue }
            total += session.importL0Json(engine: engine, json: json)
        }
        return total
    }

    /// Write both engines' L0 from the session. Called on deactivate /
    /// periodically — JSON write is atomic so a crash mid-flush doesn't
    /// leave a half-written file.
    static func saveFrom(_ session: InputxSession) {
        try? FileManager.default.createDirectory(
            at: dirURL,
            withIntermediateDirectories: true,
            attributes: nil
        )
        for engine in [InputxL0Engine.wubi, .pinyin] {
            guard let json = session.exportL0Json(engine: engine),
                  let data = json.data(using: .utf8)
            else { continue }
            let url = fileURL(for: engine)
            try? data.write(to: url, options: [.atomic])
        }
    }

    /// Delete both files — for a "reset L0" menu action.
    static func reset() {
        for engine in [InputxL0Engine.wubi, .pinyin] {
            try? FileManager.default.removeItem(at: fileURL(for: engine))
        }
    }
}
