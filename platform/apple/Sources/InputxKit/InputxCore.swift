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

    /// `true` iff the JP sub-engine specifically has a non-empty buffer.
    /// Used by the IME controller to route `-` (chōonpu) into the engine
    /// only when JP is mid-composition; outside JP composing, `-` falls
    /// to locale punctuation.
    public var isComposingJapanese: Bool {
        inputx_session_is_composing_japanese(handle) != 0
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

    /// Drop pending 联想 candidates. The host calls this from keyDown
    /// paths that bypass `handleKey` (Path B in IMEController — ASCII
    /// punct routed through locale-mapping before reaching the engine).
    /// After this call, `predictionCount` returns 0 until the next CJK
    /// commit repopulates predictions, so a subsequent
    /// `showPredictionsOrHide` will hide the panel.
    public func cancelPredictions() {
        inputx_session_cancel_predictions(handle)
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

    // MARK: - Segment mode (拼音手动分段)
    //
    // ← stop points (anchors, descending prefix lengths that have
    // candidates) + first-segment candidates + partial commit. Pinyin-only
    // by construction — wubi never participates in segment mode.

    /// The ← stop points: descending prefix lengths where `buffer[0..k]`
    /// has candidates. Empty when not pinyin-composing. The first element
    /// (largest) is where the first ← lands.
    public func segmentAnchors() -> [Int] {
        let n = Int(inputx_session_segment_anchor_count(handle))
        return (0..<n).map { Int(inputx_session_segment_anchor(handle, UInt($0))) }
    }

    /// Number of candidates for the first segment `buffer[0..k]`.
    public func segmentCandidateCount(prefixLen k: Int) -> Int {
        Int(inputx_session_segment_candidate_count(handle, UInt(k)))
    }

    /// Candidate at `index` for the first segment `buffer[0..k]`.
    public func segmentCandidate(prefixLen k: Int, at index: Int) -> String? {
        guard let cstr = inputx_session_segment_candidate(handle, UInt(k), UInt(index))
        else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    /// Commit the first segment `buffer[0..k]`'s candidate at `index`; the
    /// remainder is kept and re-composed. Returns committed text or `nil`.
    public func commitSegment(prefixLen k: Int, at index: Int) -> String? {
        guard let cstr = inputx_session_commit_segment(handle, UInt(k), UInt(index))
        else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    // MARK: - v1.15 hot-reload ----------------------------------------------

    /// Reload this session's pinyin dict from `path` (typically the
    /// running bundle's `Contents/Resources/data/`, atomically
    /// replaced by `reinstall.py`'s data-only fast path). Preserves
    /// L0 pins / cell-dict / LM; leaves any in-flight preedit alone.
    /// Returns `true` on success; on failure the session's dict is
    /// left in its prior state.
    @discardableResult
    public func reloadPinyinData(from path: String) -> Bool {
        return path.withCString { inputx_reload_pinyin_data(handle, $0) == 0 }
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

    /// Context-based smart quote. Decides the curly form of `codepoint`
    /// (`"` / `'`) from the document text before the caret rather than an
    /// in-memory toggle, so it survives IME switches, mouse clicks, and
    /// mid-text edits (and handles Chinese `他说“…”`, no space before the
    /// opener). Nesting-aware: counts unclosed quotes of this type on the
    /// current line. `contextBefore` is the text up to the caret (a
    /// bounded window is fine; empty = opens). When the host cannot read
    /// context at all, call `smartQuote(_:)` (the toggle fallback)
    /// instead. Non-quote codepoints pass through unchanged.
    public func smartQuoteCtx(_ codepoint: UInt32, contextBefore: String) -> UInt32 {
        return contextBefore.withCString { ctx in
            inputx_session_smart_quote_ctx(handle, codepoint, ctx)
        }
    }

    public func smartQuoteReset() {
        inputx_session_smart_quote_reset(handle)
    }

    // MARK: - Cell-dict L0.5 (CP-5.2 step-3) --------------------------------

    /// Result of `loadCellDict`. Distinguishes "successfully loaded N
    /// entries" from the three negative error paths the FFI reports.
    public enum CellDictLoadResult: Equatable {
        case ok(Int)
        case nullInput
        case nonUtf8
        case parseError
    }

    /// Load a TOML cell-dict pack into the session's L0.5 layer.
    /// Multiple calls accumulate; use `clearCellDict` to wipe.
    @discardableResult
    public func loadCellDict(toml: String) -> CellDictLoadResult {
        let raw = toml.withCString { ptr -> Int64 in
            inputx_session_load_cell_dict(handle, ptr)
        }
        switch raw {
        case let n where n >= 0: return .ok(Int(n))
        case -1: return .nullInput
        case -2: return .nonUtf8
        case -3: return .parseError
        default: return .parseError
        }
    }

    /// Wipe the session's L0.5 cell-dict layer.
    public func clearCellDict() {
        inputx_session_clear_cell_dict(handle)
    }

    /// Number of `(pinyin, word)` entries currently in the L0.5 layer.
    public var cellDictCount: Int {
        Int(inputx_session_cell_dict_count(handle))
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

// MARK: - v1.15 hot-reload bootstrap + signal-driven refresh ---------------

/// Process-global entry points for the hot-reload flow. `setPinyinDataDirectory`
/// is called once at app startup (before `IMKServer` is built) so
/// [`inputx_session_new`] loads polish data from disk instead of the
/// embedded blobs. `reload` is called per-session from the SIGUSR1
/// DispatchSource after `reinstall.py` swaps the on-disk files.
public enum InputxPinyinData {
    /// Point the Rust core at the bundle's pinyin-data directory.
    /// Returns `true` on success; a failure here is fatal at startup —
    /// downstream sessions would fall back to embedded, silently
    /// masking the polish flow — so callers typically `preconditionFailure`
    /// on `false`.
    @discardableResult
    public static func setDirectory(_ path: String) -> Bool {
        return path.withCString { inputx_set_pinyin_data_dir($0) == 0 }
    }

}
