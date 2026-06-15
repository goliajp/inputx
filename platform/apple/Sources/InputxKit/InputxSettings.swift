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
        /// Phase-4 CP-4.4 master switch for the user-bigram learner.
        /// When `true` (default), `pinyin_adapter.commit_index()` bumps
        /// the L0 user_bigram counter so the LM scoring path (CP-4.3)
        /// can shift mass toward the user's actual habits. When `false`,
        /// the host-side commit FFI should skip the bump entry point.
        /// Users who want a stable scoring model — for example testing
        /// or troubleshooting — can flip this off in SettingsWindow.
        static let userLearningEnabled = "userLearningEnabled"
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
            // First-install default: "从不自动" — user has the final say
            // on every commit. The OnFourCodesIfUnique policy was too
            // surprising for users typing free-form Chinese; making
            // Never the floor lets users opt up to auto-commit if they
            // want, rather than fighting accidental commits from day 1.
            Keys.autoCommitPolicy: Int(InputxAutoCommitPolicy.never.rawValue),
            Keys.showRareChars: false,
            Keys.useCjkPunct: true,
            Keys.useFullWidth: false,
            Keys.showSourceIndicator: true,
            // JP plugin on by default — the plugin's whole point is
            // making JP work seamlessly, and zero-cost-when-off-anyway
            // was the original justification for "default false". In
            // practice users expect the IME to "know JP" out of the box.
            Keys.japaneseEnabled: true,
            // Phase-4 CP-4.4: user-bigram learner on by default. The
            // bigram_lm_bonus cold-start guard (CP-4.3) keeps λ_u = 0
            // until 100 bigrams have been observed, so the toggle is
            // safe-on-by-default — early commits are still byte-equal
            // Phase-2 末.
            Keys.userLearningEnabled: true,
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

    /// Phase-4 CP-4.4 master switch for user-bigram learning. When on
    /// (default), commit events feed `pinyin_adapter.bump_user_bigram`;
    /// when off, the host-side commit FFI is responsible for skipping
    /// the bump call so the user model stays frozen at its current
    /// state. The cold-start guard inside `bigram_lm_bonus` keeps
    /// behaviour byte-equal Phase-2 until 100 bigrams accumulate, so
    /// flipping this off mid-stream just halts further learning — it
    /// doesn't erase what's already there.
    public var userLearningEnabled: Bool {
        get { defaults.bool(forKey: Keys.userLearningEnabled) }
        set { defaults.set(newValue, forKey: Keys.userLearningEnabled) }
    }
}
