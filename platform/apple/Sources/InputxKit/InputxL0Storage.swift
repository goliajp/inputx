import Foundation

/// File-system persistence for per-engine L0 user-learning JSON snapshots.
///
/// The storage location is host-controlled: iOS keyboard extension uses an
/// App Group container shared with its container app; macOS IME uses
/// `~/Library/Application Support/Inputx/`; future hosts may pick whatever
/// makes sense (XDG_DATA_HOME on Linux, %APPDATA% on Windows, etc.).
///
/// File layout under the configured directory:
///     wubi_l0.json
///     pinyin_l0.json
///
/// File contents are versioned JSON produced by `inputx_session_export_l0_json`
/// (see Rust `inputx-core::composite::l0_json` for the schema). Schema
/// mismatches are silently absorbed at import time — the engine returns 0
/// accepted entries and the user keeps their existing L0 state intact.
public final class InputxL0Storage {
    private let dirURL: URL

    /// Initialize with a specific storage directory. The directory is
    /// created on first write; the host need not pre-create it.
    public init(directoryURL: URL) {
        self.dirURL = directoryURL
    }

    // MARK: - Convenience factories -----------------------------------------

    /// Storage rooted at an iOS App Group container. Returns `nil` if the
    /// App Group isn't configured / accessible for this process — most
    /// commonly because the bundle's `com.apple.security.application-groups`
    /// entitlement is missing.
    public static func appGroup(_ groupID: String) -> InputxL0Storage? {
        guard let container = FileManager.default
            .containerURL(forSecurityApplicationGroupIdentifier: groupID)
        else { return nil }
        let dir = container
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent("Application Support", isDirectory: true)
            .appendingPathComponent("inputx", isDirectory: true)
        return InputxL0Storage(directoryURL: dir)
    }

    /// Storage at `~/Library/Application Support/<bundleName>/`. Standard
    /// for macOS apps and IMEs.
    public static func userApplicationSupport(bundleName: String = "Inputx") -> InputxL0Storage {
        let support = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first!
        return InputxL0Storage(
            directoryURL: support.appendingPathComponent(bundleName, isDirectory: true)
        )
    }

    // MARK: - High-level session load / save --------------------------------

    /// Read both engines' L0 into the session. Returns the total accepted
    /// pin count; 0 on first launch (no files yet) or on full corruption.
    ///
    /// v1.5 (user 2026-05-24: "每次更新都 clean 一下用户那个 3 次选择
    /// 就排第一的记录"): if the binary's version marker has changed,
    /// reset L0 first — accumulated pick-counts from a previous binary
    /// can bias scoring incorrectly after dict/scoring changes.
    @discardableResult
    public func load(into session: InputxSession) -> Int {
        resetIfBinaryVersionChanged()
        var total = 0
        for engine in InputxL0Engine.allCases {
            guard let json = readJson(engine: engine) else { continue }
            total += session.importL0Json(engine: engine, json: json)
        }
        return total
    }

    /// If the binary's version marker (CFBundleVersion) differs from
    /// the recorded marker on disk, wipe L0 + record the new version.
    /// One-shot per app launch.
    private func resetIfBinaryVersionChanged() {
        let versionKey = "binary_version"
        let markerURL = dirURL.appendingPathComponent(".l0_version_marker")
        let currentVersion = (Bundle.main.infoDictionary?["CFBundleVersion"] as? String)
            ?? (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String)
            ?? "unknown"
        let storedVersion = (try? String(contentsOf: markerURL, encoding: .utf8)) ?? ""
        if storedVersion == currentVersion {
            return
        }
        // Version mismatch (or first launch with no marker) → reset.
        // First launch case: stored is empty, but we still write the
        // marker so subsequent launches with same version skip reset.
        if !storedVersion.isEmpty {
            reset()
        }
        ensureDirectoryExists()
        _ = versionKey  // reserved
        try? currentVersion.write(to: markerURL, atomically: true, encoding: .utf8)
    }

    /// Write both engines' L0 from the session. Atomic per-file: a crash
    /// mid-flush leaves the previous good copy in place.
    public func save(from session: InputxSession) {
        ensureDirectoryExists()
        for engine in InputxL0Engine.allCases {
            guard let json = session.exportL0Json(engine: engine) else { continue }
            writeJson(json, engine: engine)
        }
    }

    /// Delete both engines' files. For the "reset L0" UI action.
    public func reset() {
        for engine in InputxL0Engine.allCases {
            try? FileManager.default.removeItem(at: fileURL(engine: engine))
        }
    }

    // MARK: - Raw JSON read / write (for export/import UIs) -----------------

    /// Read the raw JSON for one engine, or `nil` if the file doesn't exist.
    public func readJson(engine: InputxL0Engine) -> String? {
        let url = fileURL(engine: engine)
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        return try? String(contentsOf: url, encoding: .utf8)
    }

    /// Write raw JSON for one engine atomically. Returns success.
    @discardableResult
    public func writeJson(_ json: String, engine: InputxL0Engine) -> Bool {
        ensureDirectoryExists()
        do {
            try json.write(to: fileURL(engine: engine),
                           atomically: true,
                           encoding: .utf8)
            return true
        } catch {
            return false
        }
    }

    // MARK: - Internals -----------------------------------------------------

    public var directoryURL: URL { dirURL }

    private func fileURL(engine: InputxL0Engine) -> URL {
        let name: String
        switch engine {
        case .wubi:   name = "wubi_l0.json"
        case .pinyin: name = "pinyin_l0.json"
        }
        return dirURL.appendingPathComponent(name)
    }

    private func ensureDirectoryExists() {
        try? FileManager.default.createDirectory(
            at: dirURL,
            withIntermediateDirectories: true,
            attributes: nil
        )
    }
}
