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

    func commit(at index: Int) -> String? {
        guard let cstr = inputx_session_commit_index(handle, UInt(index)) else { return nil }
        defer { inputx_string_free(cstr) }
        return String(cString: cstr)
    }

    func clear() {
        inputx_session_clear(handle)
    }

    @discardableResult
    func setAutoCommitPolicy(_ policy: InputxAutoCommitPolicy) -> Bool {
        return inputx_session_set_auto_commit_policy(handle, policy.rawValue) != 0
    }
}
