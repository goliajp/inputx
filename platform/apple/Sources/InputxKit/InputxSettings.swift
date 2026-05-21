import Foundation

/// Persistent user preferences, backed by a host-provided `UserDefaults`
/// instance. macOS uses `.standard` (scoped by the IME's bundle ID); iOS
/// uses the App Group `UserDefaults(suiteName:)` shared between the
/// container app's Settings UI and the keyboard extension.
///
/// All defaults match the iOS app's first-launch behavior so that
/// installing on either platform feels identical.
public final class InputxSettings {
    private let defaults: UserDefaults

    private enum Keys {
        static let engineMode = "engineMode"
        static let autoCommitPolicy = "autoCommitPolicy"
        static let showRareChars = "showRareChars"
        static let useCjkPunct = "useCjkPunct"
        static let useFullWidth = "useFullWidth"
        static let showSourceIndicator = "showSourceIndicator"
        /// JP plugin "enhancement attach" toggle — orthogonal to engineMode.
        /// Mixed / WubiOnly / PinyinOnly + this=true → JP candidates appended.
        /// engineMode=.japaneseOnly forces JP regardless of this flag.
        static let japaneseEnabled = "japaneseEnabled"
    }

    /// Construct over a specific `UserDefaults`. Pass `.standard` for Mac
    /// (where bundle-scoped defaults Just Work) or the App Group suite
    /// instance for iOS.
    public init(defaults: UserDefaults) {
        self.defaults = defaults
    }

    /// Seed default values for any unset keys. Call once at host launch so
    /// subsequent reads return the seeded defaults without each call site
    /// having to repeat the value.
    public func registerDefaults() {
        defaults.register(defaults: [
            Keys.engineMode: Int(InputxEngineMode.mixed.rawValue),
            Keys.autoCommitPolicy: Int(InputxAutoCommitPolicy.onFourCodesIfUnique.rawValue),
            Keys.showRareChars: false,
            Keys.useCjkPunct: true,
            Keys.useFullWidth: false,
            Keys.showSourceIndicator: true,
            Keys.japaneseEnabled: false,
        ])
    }

    public var engineMode: InputxEngineMode {
        get {
            let raw = UInt8(defaults.integer(forKey: Keys.engineMode))
            return InputxEngineMode(rawValue: raw) ?? .mixed
        }
        set {
            defaults.set(Int(newValue.rawValue), forKey: Keys.engineMode)
        }
    }

    public var autoCommitPolicy: InputxAutoCommitPolicy {
        get {
            let raw = UInt32(defaults.integer(forKey: Keys.autoCommitPolicy))
            return InputxAutoCommitPolicy(rawValue: raw) ?? .onFourCodesIfUnique
        }
        set {
            defaults.set(Int(newValue.rawValue), forKey: Keys.autoCommitPolicy)
        }
    }

    public var showRareChars: Bool {
        get { defaults.bool(forKey: Keys.showRareChars) }
        set { defaults.set(newValue, forKey: Keys.showRareChars) }
    }

    public var useCjkPunct: Bool {
        get { defaults.bool(forKey: Keys.useCjkPunct) }
        set { defaults.set(newValue, forKey: Keys.useCjkPunct) }
    }

    public var useFullWidth: Bool {
        get { defaults.bool(forKey: Keys.useFullWidth) }
        set { defaults.set(newValue, forKey: Keys.useFullWidth) }
    }

    public var showSourceIndicator: Bool {
        get { defaults.bool(forKey: Keys.showSourceIndicator) }
        set { defaults.set(newValue, forKey: Keys.showSourceIndicator) }
    }

    public var japaneseEnabled: Bool {
        get { defaults.bool(forKey: Keys.japaneseEnabled) }
        set { defaults.set(newValue, forKey: Keys.japaneseEnabled) }
    }
}
