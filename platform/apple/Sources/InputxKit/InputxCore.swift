import Foundation
import InputxCoreC

/// Modifier bitmask matching the Rust core convention.
/// bit 0=Shift, 1=Ctrl, 2=Alt, 3=Cmd, 4=Fn.
public struct InputxModifiers: OptionSet, Sendable {
    public let rawValue: UInt32
    public init(rawValue: UInt32) { self.rawValue = rawValue }

    public static let shift = InputxModifiers(rawValue: 1 << 0)
    public static let ctrl  = InputxModifiers(rawValue: 1 << 1)
    public static let alt   = InputxModifiers(rawValue: 1 << 2)
    public static let cmd   = InputxModifiers(rawValue: 1 << 3)
    public static let fn    = InputxModifiers(rawValue: 1 << 4)
}

public enum InputxAutoCommitPolicy: UInt32, Sendable, CaseIterable {
    case never = 0
    case onFourCodes = 1
    case onUniqueMatch = 2
    case onFourCodesIfUnique = 3
}

public enum InputxEngineMode: UInt8, Sendable, CaseIterable {
    case mixed = 0
    case wubiOnly = 1
    case pinyinOnly = 2
    /// Japanese plugin runs standalone — wubi / pinyin dormant regardless
    /// of the `japaneseEnabled` toggle (which is implicitly true here).
    /// Reach this mode when the user wants pure Japanese input without
    /// Chinese-candidate interference.
    case japaneseOnly = 3
}

/// Top-level input mode — CJK vs EN. Orthogonal to `InputxEngineMode`.
/// On macOS: toggled by single-click shift. On iOS: toggled by the
/// on-screen "中/EN" key. EN is pure passthrough — `handleKey` returns
/// false so the host receives ASCII directly with no IME preedit.
public enum InputxInputMode: UInt8, Sendable, CaseIterable {
    case cjk = 0
    case en = 1

    public func toggled() -> InputxInputMode {
        self == .cjk ? .en : .cjk
    }
}

public enum InputxL0Engine: UInt8, Sendable, CaseIterable {
    case wubi = 0
    case pinyin = 1
}

public enum InputxCandidateSource: UInt8, Sendable {
    case wubi = 0
    case pinyin = 1
    case japanese = 2
    case unknown = 255
}

/// Type-safe Swift wrapper around the inputx-core C FFI.
///
/// One instance per IME client. Not thread-safe — assume the host (IMKit on
/// Mac, UIInputViewController on iOS) serializes calls on the main thread,
/// which both do.
public final class InputxSession {
    private let handle: OpaquePointer

    public init() {
        guard let h = inputx_session_new() else {
            fatalError("inputx_session_new returned NULL")
        }
        self.handle = h
    }

    deinit {
        inputx_session_free(handle)
    }

    /// Returns `true` if the IME consumed the keystroke (host should
    /// suppress the original key). Pair every consumed event with a
    /// `takeCommit()` drain before reading preedit / candidates.
    public func handleKey(codepoint: UInt32, modifiers: InputxModifiers) -> Bool {
        return inputx_session_key_event(handle, codepoint, modifiers.rawValue) != 0
    }

    /// Drain pending committed text, if any. The C string lifetime is
    /// owned by the Rust core; this wrapper frees it after copying.
    public func takeCommit() -> String? {
        guard let cstr = inputx_session_take_commit(handle) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    public var preedit: String? {
        guard let cstr = inputx_session_preedit(handle) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    /// `true` iff the engine is mid-composition (preedit is non-empty).
    public var isComposing: Bool {
        guard let pre = preedit else { return false }
        return !pre.isEmpty
    }

    public var candidateCount: Int {
        return Int(inputx_session_candidate_count(handle))
    }

    public func candidate(at index: Int) -> String? {
        guard let cstr = inputx_session_candidate(handle, UInt(index)) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    /// Engine source for the candidate at `index` (Wubi / Pinyin / unknown).
    /// Drives the W/P indicator in the candidate UI.
    public func candidateSource(at index: Int) -> InputxCandidateSource {
        let raw = inputx_session_candidate_source(handle, UInt32(index))
        return InputxCandidateSource(rawValue: raw) ?? .unknown
    }

    /// Commit the candidate at `index` and reset composition. Returns
    /// committed text or `nil` if the index was out of range.
    public func commit(at index: Int) -> String? {
        guard let cstr = inputx_session_commit_index(handle, UInt(index)) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    // MARK: - Predictions (联想)

    /// Number of next-word predictions available after the most-recent
    /// CJK commit. 0 if no predictions are active (cold session, last
    /// commit was non-CJK, mode forbids pinyin predictions, etc.).
    public var predictionCount: Int {
        return Int(inputx_session_prediction_count(handle))
    }

    /// Predicted next-word at `index`, or `nil` for out-of-range.
    public func prediction(at index: Int) -> String? {
        guard let cstr = inputx_session_prediction(handle, UInt(index)) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    /// Commit a prediction by index. Triggers a fresh round of
    /// predictions internally (chained 联想). Returns the committed
    /// text or `nil` for out-of-range index.
    public func commitPrediction(at index: Int) -> String? {
        guard let cstr = inputx_session_commit_prediction(handle, UInt(index)) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    /// Drop the composition without committing (Escape).
    public func clear() {
        inputx_session_clear(handle)
    }

    /// Pay every cold-path cost up front so the first measured keystroke
    /// isn't paying ~1-2 s init for the FST mmap + 简拼 initials index
    /// build. Idempotent — call at IMK activate.
    public func warmup() {
        inputx_session_warmup(handle)
    }

    // MARK: - Settings -------------------------------------------------------

    @discardableResult
    public func setAutoCommitPolicy(_ policy: InputxAutoCommitPolicy) -> Bool {
        return inputx_session_set_auto_commit_policy(handle, policy.rawValue) != 0
    }

    @discardableResult
    public func setEngineMode(_ mode: InputxEngineMode) -> Bool {
        return inputx_session_set_engine_mode(handle, mode.rawValue) != 0
    }

    public var engineMode: InputxEngineMode {
        let raw = inputx_session_get_engine_mode(handle)
        return InputxEngineMode(rawValue: raw) ?? .mixed
    }

    /// Japanese plugin "enhancement" toggle. Independent of `engineMode`:
    /// - In `.mixed` / `.wubiOnly` / `.pinyinOnly`: when on, JP candidates
    ///   are appended after the Chinese candidates.
    /// - In `.japaneseOnly`: implicitly true (mode forces JP).
    /// Default OFF.
    public var japaneseEnabled: Bool {
        get { inputx_session_get_japanese_enabled(handle) != 0 }
        set { _ = inputx_session_set_japanese_enabled(handle, newValue ? 1 : 0) }
    }

    @discardableResult
    public func setJapaneseEnabled(_ on: Bool) -> Bool {
        return inputx_session_set_japanese_enabled(handle, on ? 1 : 0) != 0
    }

    /// Switch the top-level input mode. `.cjk → .en` while composing
    /// commits the in-flight wubi/pinyin codes as **raw ASCII** (user
    /// signaled "not CJK after all") — same semantic as pressing return
    /// with a non-empty preedit. `.en → .cjk` is a pure state flip.
    @discardableResult
    public func setInputMode(_ mode: InputxInputMode) -> Bool {
        return inputx_session_set_input_mode(handle, mode.rawValue) != 0
    }

    public var inputMode: InputxInputMode {
        let raw = inputx_session_get_input_mode(handle)
        return InputxInputMode(rawValue: raw) ?? .cjk
    }

    /// Convenience: flip CJK ↔ EN. Returns the resulting mode.
    @discardableResult
    public func toggleInputMode() -> InputxInputMode {
        let next = inputMode.toggled()
        _ = setInputMode(next)
        return next
    }

    // MARK: - L0 persistence -------------------------------------------------

    /// Serialize one engine's L0 (pins + pending counters) as JSON.
    public func exportL0Json(engine: InputxL0Engine) -> String? {
        guard let cstr = inputx_session_export_l0_json(handle, engine.rawValue) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    /// Import one engine's L0 from JSON. Returns count of accepted entries
    /// (0 on schema mismatch, wrong engine, or malformed input).
    @discardableResult
    public func importL0Json(engine: InputxL0Engine, json: String) -> Int {
        return json.withCString { ptr in
            Int(inputx_session_import_l0_json(handle, engine.rawValue, ptr))
        }
    }

    // MARK: - Smart quote (per-session state) -------------------------------

    /// Smart-quote next codepoint. `"` and `'` alternate between opening
    /// and closing CJK forms across the session lifetime. Non-quote
    /// codepoints pass through unchanged.
    public func smartQuote(_ codepoint: UInt32) -> UInt32 {
        return inputx_session_smart_quote(handle, codepoint)
    }

    public func smartQuoteReset() {
        inputx_session_smart_quote_reset(handle)
    }
}

// MARK: - Process-global locale helpers (stateless) --------------------------

public enum InputxLocale {
    /// ASCII punctuation → CJK equivalent. Returns the same codepoint when
    /// no mapping exists (so callers can use as a passthrough).
    public static func asciiToCjk(_ codepoint: UInt32) -> UInt32 {
        return inputx_punct_ascii_to_cjk(codepoint)
    }

    /// ASCII → full-width (e.g. 'A' → '\u{FF21}', ' ' → '\u{3000}').
    /// Returns the same codepoint when no mapping exists.
    public static func fullWidth(_ codepoint: UInt32) -> UInt32 {
        return inputx_punct_full_width(codepoint)
    }
}

// MARK: - Rare-CJK process-global toggle ------------------------------------

public enum InputxRareChars {
    public static var enabled: Bool {
        get { inputx_get_show_rare_chars() != 0 }
        set { inputx_set_show_rare_chars(newValue ? 1 : 0) }
    }
}
