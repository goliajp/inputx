import Foundation

/// Modifier bitmask matching the Rust core convention.
/// bit 0=Shift, 1=Ctrl, 2=Alt, 3=Cmd, 4=Fn.
struct InputxModifiers: OptionSet {
    let rawValue: UInt32

    static let shift = InputxModifiers(rawValue: 1 << 0)
    static let ctrl  = InputxModifiers(rawValue: 1 << 1)
    static let alt   = InputxModifiers(rawValue: 1 << 2)
    static let cmd   = InputxModifiers(rawValue: 1 << 3)
    static let fn    = InputxModifiers(rawValue: 1 << 4)
}

enum InputxAutoCommitPolicy: UInt32 {
    case never = 0
    case onFourCodes = 1
    case onUniqueMatch = 2
    case onFourCodesIfUnique = 3
}

enum InputxEngineMode: UInt8 {
    case mixed = 0
    case wubiOnly = 1
    case pinyinOnly = 2
}

enum InputxL0Engine: UInt8 {
    case wubi = 0
    case pinyin = 1
}

enum InputxCandidateSource: UInt8 {
    case wubi = 0
    case pinyin = 1
    case unknown = 255
}

/// Type-safe Swift wrapper around the inputx-core C FFI.
/// One instance per IMK client. Not thread-safe; assume IMKit serializes
/// calls on the main thread (it does).
final class InputxSession {
    private let handle: OpaquePointer

    init() {
        guard let h = inputx_session_new() else {
            fatalError("inputx_session_new returned NULL")
        }
        self.handle = h
    }

    deinit {
        inputx_session_free(handle)
    }

    /// Returns true if the IME consumed the keystroke (host should suppress it).
    /// Pair every consumed event with a `takeCommit()` drain before reading
    /// preedit / candidates.
    func handleKey(codepoint: UInt32, modifiers: InputxModifiers) -> Bool {
        return inputx_session_key_event(handle, codepoint, modifiers.rawValue) != 0
    }

    /// Drain pending committed text, if any. Handles the C string lifetime.
    func takeCommit() -> String? {
        guard let cstr = inputx_session_take_commit(handle) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    var preedit: String? {
        guard let cstr = inputx_session_preedit(handle) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    var candidateCount: Int {
        return Int(inputx_session_candidate_count(handle))
    }

    func candidate(at index: Int) -> String? {
        guard let cstr = inputx_session_candidate(handle, UInt(index)) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    /// Engine source for the candidate at `index` (Wubi / Pinyin / unknown).
    /// Drives the W/P indicator in the candidate panel.
    func candidateSource(at index: Int) -> InputxCandidateSource {
        let raw = inputx_session_candidate_source(handle, UInt32(index))
        return InputxCandidateSource(rawValue: raw) ?? .unknown
    }

    /// Commit candidate at `index` and reset composition. Returns committed text.
    func commit(at index: Int) -> String? {
        guard let cstr = inputx_session_commit_index(handle, UInt(index)) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    /// Drop composition without commit (escape).
    func clear() {
        inputx_session_clear(handle)
    }

    /// Pay every cold-path cost up front: page in FST `.rodata` and build the
    /// process-global 简拼 initials index so the first measured keystroke
    /// isn't paying ~1-2s cold-start. Call once at IMK activate.
    func warmup() {
        inputx_session_warmup(handle)
    }

    // MARK: - Settings -------------------------------------------------------

    @discardableResult
    func setAutoCommitPolicy(_ policy: InputxAutoCommitPolicy) -> Bool {
        return inputx_session_set_auto_commit_policy(handle, policy.rawValue) != 0
    }

    @discardableResult
    func setEngineMode(_ mode: InputxEngineMode) -> Bool {
        return inputx_session_set_engine_mode(handle, mode.rawValue) != 0
    }

    var engineMode: InputxEngineMode {
        let raw = inputx_session_get_engine_mode(handle)
        return InputxEngineMode(rawValue: raw) ?? .mixed
    }

    // MARK: - L0 persistence -------------------------------------------------

    /// Serialize one engine's L0 (pins + pending counters) to JSON.
    func exportL0Json(engine: InputxL0Engine) -> String? {
        guard let cstr = inputx_session_export_l0_json(handle, engine.rawValue) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    /// Import one engine's L0 from JSON. Returns count of accepted entries.
    /// Returns 0 on schema mismatch, wrong engine, or malformed input.
    @discardableResult
    func importL0Json(engine: InputxL0Engine, json: String) -> Int {
        return json.withCString { ptr in
            Int(inputx_session_import_l0_json(handle, engine.rawValue, ptr))
        }
    }

    // MARK: - Smart quote (per-session state) -------------------------------

    /// Smart-quote next codepoint via per-session state: `"` and `'` alternate
    /// between opening / closing CJK forms. Non-quote codepoints pass through.
    func smartQuote(_ codepoint: UInt32) -> UInt32 {
        return inputx_session_smart_quote(handle, codepoint)
    }

    func smartQuoteReset() {
        inputx_session_smart_quote_reset(handle)
    }
}

// MARK: - Process-global locale helpers (stateless) --------------------------

enum InputxLocale {
    /// ASCII punctuation → CJK equivalent. Returns the same codepoint when
    /// no mapping exists (so callers can use as a passthrough).
    static func asciiToCjk(_ codepoint: UInt32) -> UInt32 {
        return inputx_punct_ascii_to_cjk(codepoint)
    }

    /// ASCII → full-width (e.g. 'A' → '\u{FF21}', ' ' → '\u{3000}'). Returns
    /// the same codepoint when no mapping exists.
    static func fullWidth(_ codepoint: UInt32) -> UInt32 {
        return inputx_punct_full_width(codepoint)
    }
}

// MARK: - Rare-CJK process-global toggle ------------------------------------

enum InputxRareChars {
    static var enabled: Bool {
        get { inputx_get_show_rare_chars() != 0 }
        set { inputx_set_show_rare_chars(newValue ? 1 : 0) }
    }
}
