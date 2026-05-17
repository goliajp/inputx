import Foundation

/// App Group's UserDefaults — shared between the main InputxApp and this
/// keyboard extension via the `group.jp.golia.inputx` entitlement. Used
/// for runtime preferences like `showRareChars`.
let inputxSharedDefaults = UserDefaults(suiteName: "group.jp.golia.inputx")

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

/// Dual-engine mode (Phase 4 of the iOS commercial-grade roadmap).
/// Default = .mixed (wubi primary + pinyin fallback, 万能/搜狗五笔 style).
enum InputxEngineMode: UInt8 {
    case mixed = 0       // both engines participate, wubi-first merge
    case wubiOnly = 1
    case pinyinOnly = 2
}

/// Source attribution for a candidate. Returned alongside each candidate
/// for the W/P indicator dot in `CandidateBar`.
enum InputxCandidateSource: UInt8 {
    case wubi = 0
    case pinyin = 1

    static func decode(_ raw: UInt8) -> InputxCandidateSource? {
        switch raw {
        case 0: return .wubi
        case 1: return .pinyin
        default: return nil   // 255 sentinel = "unknown / out of range"
        }
    }
}

/// Type-safe Swift wrapper around the inputx-core C FFI.
/// Thread-safe — every public method serializes through an internal NSLock
/// so a background warm-up dispatch can safely interleave with main-thread
/// keystroke handling. Lock acquisition is ~50ns; a keystroke makes ~45
/// session calls so per-stroke lock cost is ~2µs, well under frame budget.
final class InputxSession {
    private let handle: OpaquePointer
    private let lock = NSLock()

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
        lock.lock()
        defer { lock.unlock() }
        return inputx_session_key_event(handle, codepoint, modifiers.rawValue) != 0
    }

    /// Drain pending committed text, if any. Handles the C string lifetime.
    func takeCommit() -> String? {
        lock.lock()
        defer { lock.unlock() }
        guard let cstr = inputx_session_take_commit(handle) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    var preedit: String? {
        lock.lock()
        defer { lock.unlock() }
        guard let cstr = inputx_session_preedit(handle) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    /// `true` iff the engine has any composing input. Inlines the FFI call
    /// rather than reading `preedit` so the lock doesn't recurse on itself
    /// (NSLock is not reentrant).
    var isComposing: Bool {
        lock.lock()
        defer { lock.unlock() }
        guard let cstr = inputx_session_preedit(handle) else { return false }
        defer { inputx_string_free(cstr) }
        return cstr.pointee != 0
    }

    var candidateCount: Int {
        lock.lock()
        defer { lock.unlock() }
        return Int(inputx_session_candidate_count(handle))
    }

    func candidate(at index: Int) -> String? {
        lock.lock()
        defer { lock.unlock() }
        guard let cstr = inputx_session_candidate(handle, UInt(index)) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    func commit(at index: Int) -> String? {
        lock.lock()
        defer { lock.unlock() }
        guard let cstr = inputx_session_commit_index(handle, UInt(index)) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    func clear() {
        lock.lock()
        defer { lock.unlock() }
        inputx_session_clear(handle)
    }

    /// Pay every cold-init cost in the Rust engine stack up front. After
    /// this returns, the user's first keystroke is on hot paths only.
    /// ~500ms cold, ~1ms idempotent. Safe to call from a background queue;
    /// the internal lock serializes with any main-thread session access.
    func warmup() {
        lock.lock()
        defer { lock.unlock() }
        inputx_session_warmup(handle)
    }

    @discardableResult
    func setAutoCommitPolicy(_ policy: InputxAutoCommitPolicy) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return inputx_session_set_auto_commit_policy(handle, policy.rawValue) != 0
    }

    // MARK: - Phase 4 dual-engine surface

    /// Switch engine mode. Returns true on accept, false on out-of-range
    /// (state unchanged in either case).
    @discardableResult
    func setMode(_ mode: InputxEngineMode) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return inputx_session_set_engine_mode(handle, mode.rawValue) != 0
    }

    var mode: InputxEngineMode {
        lock.lock()
        defer { lock.unlock() }
        let raw = inputx_session_get_engine_mode(handle)
        return InputxEngineMode(rawValue: raw) ?? .mixed
    }

    /// Source byte for the candidate at `index`. `nil` if out of range or
    /// the index has no source attribution (255 sentinel from FFI).
    func candidateSource(at index: Int) -> InputxCandidateSource? {
        lock.lock()
        defer { lock.unlock() }
        let raw = inputx_session_candidate_source(handle, UInt32(index))
        return InputxCandidateSource.decode(raw)
    }

    /// Export the L0 layer for `engine` as JSON for App-Group persistence
    /// (item 76 settings). Caller stores the string however; pair with
    /// `importL0Json` on next launch.
    func exportL0Json(engine: InputxCandidateSource) -> String? {
        lock.lock()
        defer { lock.unlock() }
        guard let cstr = inputx_session_export_l0_json(handle, engine.rawValue) else {
            return nil
        }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    /// Import an L0 snapshot for `engine` from JSON. Returns count of
    /// accepted pins (entries whose `(input, word)` no longer exist in the
    /// lexicon are silently dropped).
    @discardableResult
    func importL0Json(engine: InputxCandidateSource, json: String) -> Int {
        lock.lock()
        defer { lock.unlock() }
        return json.withCString { ptr in
            Int(inputx_session_import_l0_json(handle, engine.rawValue, ptr))
        }
    }
}

/// Process-global runtime settings forwarded into the Rust core.
enum InputxSettings {
    /// Toggle visibility of rare CJK candidates (codepoint ≥ U+20000,
    /// i.e., Extension B and beyond). Off by default — most iOS fonts can't
    /// render them so committing shows `?` in the receiving app.
    /// Persisted in App Group UserDefaults so the main app can write the
    /// preference and the keyboard extension picks it up on startup.
    static func setShowRareChars(_ show: Bool) {
        inputx_set_show_rare_chars(show ? 1 : 0)
    }
}

// MARK: - Phase 9 locale (items 81-85)

/// Stateless ASCII → CJK punct mapping. Returns the original codepoint
/// unchanged if there's no mapping.
enum InputxLocale {
    static func cjkPunct(_ codepoint: UInt32) -> UInt32 {
        return inputx_punct_ascii_to_cjk(codepoint)
    }
    static func fullWidth(_ codepoint: UInt32) -> UInt32 {
        return inputx_punct_full_width(codepoint)
    }
}

extension InputxSession {
    /// Per-session smart-quote map. Returns the (possibly-mapped) codepoint
    /// to insert. `"` and `'` alternate open/close; other chars passthrough.
    func smartQuote(codepoint: UInt32) -> UInt32 {
        lock.lock()
        defer { lock.unlock() }
        return inputx_session_smart_quote(handle, codepoint)
    }

    func resetSmartQuote() {
        lock.lock()
        defer { lock.unlock() }
        inputx_session_smart_quote_reset(handle)
    }
}
