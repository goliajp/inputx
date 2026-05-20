import Foundation

/// Persistent user preferences, backed by `UserDefaults.standard` under the
/// IME's bundle identifier. macOS scopes UserDefaults by bundle so the IME
/// extension and any future container/menubar tool share the same store.
///
/// All defaults match the iOS app's first-launch defaults so behavior is
/// consistent across platforms.
enum InputxSettings {
    private static let defaults = UserDefaults.standard

    private enum Keys {
        static let engineMode = "engineMode"
        static let autoCommitPolicy = "autoCommitPolicy"
        static let showRareChars = "showRareChars"
        static let useCjkPunct = "useCjkPunct"
        static let useFullWidth = "useFullWidth"
        static let showSourceIndicator = "showSourceIndicator"
    }

    /// Seed default values for any unset keys. Called once at launch so
    /// subsequent reads can be `defaults.integer/bool/...` without nil-coal.
    /// Matches iOS first-launch defaults.
    static func registerDefaults() {
        defaults.register(defaults: [
            Keys.engineMode: InputxEngineMode.mixed.rawValue,
            Keys.autoCommitPolicy: InputxAutoCommitPolicy.onFourCodesIfUnique.rawValue,
            Keys.showRareChars: false,
            Keys.useCjkPunct: true,
            Keys.useFullWidth: false,
            Keys.showSourceIndicator: true,
        ])
    }

    static var engineMode: InputxEngineMode {
        get {
            let raw = UInt8(defaults.integer(forKey: Keys.engineMode))
            return InputxEngineMode(rawValue: raw) ?? .mixed
        }
        set {
            defaults.set(Int(newValue.rawValue), forKey: Keys.engineMode)
        }
    }

    static var autoCommitPolicy: InputxAutoCommitPolicy {
        get {
            let raw = UInt32(defaults.integer(forKey: Keys.autoCommitPolicy))
            return InputxAutoCommitPolicy(rawValue: raw) ?? .onFourCodesIfUnique
        }
        set {
            defaults.set(Int(newValue.rawValue), forKey: Keys.autoCommitPolicy)
        }
    }

    static var showRareChars: Bool {
        get { defaults.bool(forKey: Keys.showRareChars) }
        set { defaults.set(newValue, forKey: Keys.showRareChars) }
    }

    static var useCjkPunct: Bool {
        get { defaults.bool(forKey: Keys.useCjkPunct) }
        set { defaults.set(newValue, forKey: Keys.useCjkPunct) }
    }

    static var useFullWidth: Bool {
        get { defaults.bool(forKey: Keys.useFullWidth) }
        set { defaults.set(newValue, forKey: Keys.useFullWidth) }
    }

    static var showSourceIndicator: Bool {
        get { defaults.bool(forKey: Keys.showSourceIndicator) }
        set { defaults.set(newValue, forKey: Keys.showSourceIndicator) }
    }
}
