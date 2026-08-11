import Foundation

/// CP-5.2 step-3 — descriptor for a bundled cell-dict TOML pack the
/// host can present in Settings UI. The bundled assets live in the
/// host bundle's `cell-dicts/` resource directory (one TOML per pack);
/// `InputxCellDictRegistry.bundled(in:)` scans the directory and
/// returns one descriptor per file.
///
/// Why a registry + descriptor split (vs hard-coding pack IDs in
/// Settings UI):
/// - Adding a new pack = drop a TOML into the resource directory + ship
///   a build. No Swift / SettingsWindow edits.
/// - The descriptor reads the TOML's `[meta]` table once at app launch
///   so the UI can render the human name, version, and description
///   without re-parsing on every settings toggle.
public struct InputxCellDictPack: Hashable, Identifiable {
    /// Stable ID = TOML filename without extension (e.g. `it_terms`).
    /// Persisted in `InputxSettings.enabledCellDictPackIds` so renaming
    /// the resource file would mean migrating user prefs — keep IDs
    /// stable once shipped.
    public let id: String
    public let displayName: String
    public let version: Int
    public let description: String
    public let url: URL

    public init(id: String, displayName: String, version: Int, description: String, url: URL) {
        self.id = id
        self.displayName = displayName
        self.version = version
        self.description = description
        self.url = url
    }
}

public enum InputxCellDictRegistry {
    /// Scan `bundle`'s `cell-dicts/` resource directory and return one
    /// descriptor per `*.toml` file. Returns empty when the directory
    /// isn't present (CI / unit-test bundles often omit it). Packs are
    /// sorted by display name so the Settings UI ordering is stable.
    public static func bundled(in bundle: Bundle = .main) -> [InputxCellDictPack] {
        guard let root = bundle.url(forResource: "cell-dicts", withExtension: nil),
              let entries = try? FileManager.default.contentsOfDirectory(
                at: root,
                includingPropertiesForKeys: nil,
                options: [.skipsHiddenFiles]
              )
        else { return [] }
        return entries
            .filter { $0.pathExtension.lowercased() == "toml" }
            .compactMap(parse)
            .sorted { $0.displayName < $1.displayName }
    }

    /// Apply the user's enabled-pack selection to a live `InputxSession`
    /// session: wipe the L0.5 layer, then re-load every enabled pack.
    /// Called at app launch (once) and whenever the user toggles a
    /// pack in SettingsWindow.
    ///
    /// Returns `(loaded, failed)` — IDs of packs that loaded vs IDs
    /// whose TOML was rejected. UI can surface the failed list as a
    /// "this pack didn't load" badge without aborting the whole apply.
    @discardableResult
    public static func apply(
        enabledIds: Set<String>,
        from packs: [InputxCellDictPack],
        to core: InputxSession
    ) -> (loaded: [String], failed: [String]) {
        core.clearCellDict()
        var loaded: [String] = []
        var failed: [String] = []
        for pack in packs where enabledIds.contains(pack.id) {
            guard let text = try? String(contentsOf: pack.url, encoding: .utf8) else {
                failed.append(pack.id)
                continue
            }
            if case .ok = core.loadCellDict(toml: text) {
                loaded.append(pack.id)
            } else {
                failed.append(pack.id)
            }
        }
        return (loaded, failed)
    }

    /// Read just the TOML's `[meta]` table — name / version / description —
    /// without depending on a full TOML parser. The schema is fixed
    /// (matches `core/crates/inputx-pinyin/src/cell_dict.rs`), so a
    /// hand-rolled line scanner is enough and keeps InputxKit free of
    /// extra deps.
    private static func parse(_ url: URL) -> InputxCellDictPack? {
        let id = url.deletingPathExtension().lastPathComponent
        guard let raw = try? String(contentsOf: url, encoding: .utf8) else { return nil }

        var inMeta = false
        var name: String?
        var version = 1
        var description = ""
        for line in raw.split(whereSeparator: { $0.isNewline }) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("#") || trimmed.isEmpty { continue }
            if trimmed.hasPrefix("[") {
                inMeta = (trimmed == "[meta]")
                if !inMeta && name != nil { break }
                continue
            }
            guard inMeta else { continue }
            guard let eq = trimmed.firstIndex(of: "=") else { continue }
            let key = trimmed[..<eq].trimmingCharacters(in: .whitespaces)
            let value = trimmed[trimmed.index(after: eq)...]
                .trimmingCharacters(in: .whitespaces)
            switch key {
            case "name": name = stripQuotes(value)
            case "version": version = Int(value) ?? 1
            case "description": description = stripQuotes(value)
            default: continue
            }
        }
        return InputxCellDictPack(
            id: id,
            displayName: name ?? id,
            version: version,
            description: description,
            url: url
        )
    }

    private static func stripQuotes(_ s: String) -> String {
        guard s.count >= 2, s.hasPrefix("\""), s.hasSuffix("\"") else { return s }
        return String(s.dropFirst().dropLast())
    }
}
