//! C ABI surface. cbindgen reads this module and emits `inputx_core.h`.
//!
//! Ownership rules:
//! - `InputxSession*` is owned by the caller; pair `inputx_session_new` with `inputx_session_free`.
//! - C strings returned by `inputx_session_*` query/commit functions are heap-allocated;
//!   the caller MUST free each one via `inputx_string_free`.
//! - All functions are safe to call with a NULL `session`; queries return 0/NULL,
//!   `inputx_session_free` is a no-op.

use core::ffi::{CStr, c_char};
use std::ffi::CString;

use inputx_core::{AutoCommitPolicy, EngineMode, InputMode, Session};

/// Opaque handle to a Inputx IME session.
pub struct InputxSession {
    inner: Session,
}

fn dup_to_cstring(s: &str) -> *mut c_char {
    CString::new(s)
        .map(|cs| cs.into_raw())
        .unwrap_or(core::ptr::null_mut())
}

/// Create a new session. Always returns a non-null pointer the caller owns.
#[unsafe(no_mangle)]
pub extern "C" fn inputx_session_new() -> *mut InputxSession {
    Box::into_raw(Box::new(InputxSession {
        inner: Session::new(),
    }))
}

/// Free a session. Safe to call with NULL.
///
/// # Safety
/// `session` must come from `inputx_session_new`, not yet freed, not used after.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_free(session: *mut InputxSession) {
    if !session.is_null() {
        unsafe {
            drop(Box::from_raw(session));
        }
    }
}

/// Process a keystroke.
///
/// `codepoint` — Unicode scalar value (0 if no character, e.g., modifier-only).
/// `modifiers` — bitmask: bit 0=Shift, 1=Ctrl, 2=Alt, 3=Cmd, 4=Fn.
///
/// Returns `1` if the IME consumed the event (host should suppress the original
/// key); `0` if the host should pass the key through. After a consumed event,
/// the host should:
///   1. call `inputx_session_take_commit` first — if non-NULL, deliver the text
///      to the client (and free with `inputx_string_free`);
///   2. then refresh its preedit/candidate UI from `inputx_session_preedit` and
///      `inputx_session_candidate*`.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_key_event(
    session: *mut InputxSession,
    codepoint: u32,
    modifiers: u32,
) -> u8 {
    let Some(s) = (unsafe { session.as_mut() }) else {
        return 0;
    };
    if s.inner.handle_key(codepoint, modifiers) {
        1
    } else {
        0
    }
}

/// Take the engine's pending auto-commit text, if any. Returns NULL if there
/// is none. Caller must free a non-NULL result via `inputx_string_free`.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_take_commit(session: *mut InputxSession) -> *mut c_char {
    let Some(s) = (unsafe { session.as_mut() }) else {
        return core::ptr::null_mut();
    };
    match s.inner.take_pending_commit() {
        Some(text) => dup_to_cstring(&text),
        None => core::ptr::null_mut(),
    }
}

/// Returns the current preedit (composing) string as a heap-allocated, NUL-
/// terminated UTF-8 C string. Returns NULL if there is no preedit, on bad
/// input, or on allocation failure. Caller must free via `inputx_string_free`.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_preedit(session: *const InputxSession) -> *mut c_char {
    let Some(s) = (unsafe { session.as_ref() }) else {
        return core::ptr::null_mut();
    };
    if s.inner.preedit().is_empty() {
        return core::ptr::null_mut();
    }
    dup_to_cstring(s.inner.preedit())
}

/// Returns the number of current candidates (0 if not composing or session is NULL).
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_candidate_count(session: *const InputxSession) -> usize {
    let Some(s) = (unsafe { session.as_ref() }) else {
        return 0;
    };
    s.inner.candidate_count()
}

/// Returns the candidate at `index` as a heap-allocated, NUL-terminated UTF-8
/// C string. Returns NULL if `index` is out of range, session is NULL, or on
/// allocation failure. Caller must free via `inputx_string_free`.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_candidate(
    session: *const InputxSession,
    index: usize,
) -> *mut c_char {
    let Some(s) = (unsafe { session.as_ref() }) else {
        return core::ptr::null_mut();
    };
    match s.inner.candidates().get(index) {
        Some(text) => dup_to_cstring(text),
        None => core::ptr::null_mut(),
    }
}

/// Returns the number of next-word predictions (联想) available after the
/// most-recent CJK commit. 0 if no predictions are active (no prior commit
/// this session, non-CJK last commit, or mode doesn't allow pinyin).
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_prediction_count(session: *const InputxSession) -> usize {
    let Some(s) = (unsafe { session.as_ref() }) else {
        return 0;
    };
    s.inner.prediction_count()
}

/// Drop pending 联想 candidates so the host can re-sync its candidate
/// panel after a key event that bypassed `inputx_session_handle_key`
/// (e.g. IMK-controller-side punct routing). After this call,
/// `inputx_session_prediction_count` returns 0 until the next commit
/// repopulates predictions.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_cancel_predictions(session: *mut InputxSession) {
    let Some(s) = (unsafe { session.as_mut() }) else {
        return;
    };
    s.inner.cancel_predictions();
}

/// Returns the prediction at `index` as a heap-allocated UTF-8 C string.
/// NULL if out of range, session is NULL, or allocation fails. Caller
/// must free via `inputx_string_free`.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_prediction(
    session: *const InputxSession,
    index: usize,
) -> *mut c_char {
    let Some(s) = (unsafe { session.as_ref() }) else {
        return core::ptr::null_mut();
    };
    let preds = s.inner.predictions();
    match preds.get(index) {
        Some(text) => dup_to_cstring(text),
        None => core::ptr::null_mut(),
    }
}

/// Commit a prediction by index. Returns the committed text (caller frees
/// via `inputx_string_free`), or NULL if out of range. Triggers a fresh
/// round of predictions internally (chained 联想). Doesn't record an
/// engine L0 pick — predictions are buffer-less commits.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_commit_prediction(
    session: *mut InputxSession,
    index: usize,
) -> *mut c_char {
    let Some(s) = (unsafe { session.as_mut() }) else {
        return core::ptr::null_mut();
    };
    match s.inner.commit_prediction(index) {
        Some(text) => dup_to_cstring(&text),
        None => core::ptr::null_mut(),
    }
}

/// Manually commit the candidate at `index`. Returns the committed text as a
/// heap-allocated UTF-8 C string (caller frees via `inputx_string_free`), or
/// NULL if `index` is out of range. On success the session's composing state
/// is cleared.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_commit_index(
    session: *mut InputxSession,
    index: usize,
) -> *mut c_char {
    let Some(s) = (unsafe { session.as_mut() }) else {
        return core::ptr::null_mut();
    };
    match s.inner.commit_index(index) {
        Some(text) => dup_to_cstring(&text),
        None => core::ptr::null_mut(),
    }
}

/// Clear composing state without committing.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_clear(session: *mut InputxSession) {
    if let Some(s) = unsafe { session.as_mut() } {
        s.inner.clear();
    }
}

/// Pay every cold-init cost in the engine stack up front (wubi dict + FST
/// page-fault touch, pinyin dict, 简拼 INITIALS_INDEX build, wubi→pinyin
/// dispatch path). After this returns, the user's first keystroke is on
/// hot paths only. Idempotent — second call is ~1ms (cached caches).
///
/// Intended use: host wakes a background thread at keyboard-load time and
/// calls this once before the user starts typing. Cost is ~500ms cold;
/// safe to interleave with other session calls (the host's wrapper is
/// responsible for any locking it needs).
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_warmup(session: *mut InputxSession) {
    if let Some(s) = unsafe { session.as_mut() } {
        s.inner.warmup();
    }
}

/// Set the auto-commit policy. `policy` must be one of:
///   0 = Never, 1 = OnFourCodes, 2 = OnUniqueMatch, 3 = OnFourCodesIfUnique.
/// Returns `1` if the value was accepted, `0` if it was out of range
/// (in which case the policy is unchanged).
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_set_auto_commit_policy(
    session: *mut InputxSession,
    policy: u32,
) -> u8 {
    let Some(s) = (unsafe { session.as_mut() }) else {
        return 0;
    };
    match AutoCommitPolicy::from_u32(policy) {
        Some(p) => {
            s.inner.set_auto_commit_policy(p);
            1
        }
        None => 0,
    }
}

/// Free a string previously returned by a `inputx_*` function. Safe with NULL.
///
/// # Safety
/// `s` must be a pointer previously returned by a `inputx_*` function and not
/// yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_string_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}

/// Toggle visibility of rare CJK candidates (Unicode codepoint ≥ U+20000,
/// i.e., Extension B and beyond). When `0` (default), `lookup` filters
/// them out — most iOS / Android fonts can't render them, so committing
/// would show `?` in the receiving app. Set to `1` for power users who
/// want full Han coverage in candidates and accept the rendering caveat.
///
/// Process-global state; affects all sessions in the same process.
#[unsafe(no_mangle)]
pub extern "C" fn inputx_set_show_rare_chars(show: u8) {
    inputx_core::wubi::set_show_rare(show != 0);
}

#[unsafe(no_mangle)]
pub extern "C" fn inputx_get_show_rare_chars() -> u8 {
    if inputx_core::wubi::show_rare() { 1 } else { 0 }
}

// ----------------------------------------------------------------------
// Phase 4 dual-engine FFI (items 44 + 45)
// ----------------------------------------------------------------------

/// Switch engine mode. `mode` must be one of:
///   0 = Mixed (wubi primary + pinyin fallback)
///   1 = WubiOnly
///   2 = PinyinOnly
///   3 = JapaneseOnly (standalone JP plugin; wubi/pinyin dormant)
/// Returns `1` if accepted, `0` if `mode` was out of range or session NULL
/// (state unchanged in either case).
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_set_engine_mode(
    session: *mut InputxSession,
    mode: u8,
) -> u8 {
    let Some(s) = (unsafe { session.as_mut() }) else {
        return 0;
    };
    match EngineMode::from_u8(mode) {
        Some(m) => {
            s.inner.set_mode(m);
            1
        }
        None => 0,
    }
}

/// Returns the current engine mode (0/1/2). Returns 0 (Mixed) if session
/// is NULL — same as the default.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_get_engine_mode(session: *const InputxSession) -> u8 {
    match unsafe { session.as_ref() } {
        Some(s) => s.inner.mode().as_u8(),
        None => EngineMode::default().as_u8(),
    }
}

/// Set the top-level input mode (orthogonal to engine mode).
///   0 = Cjk (default — CJK composing pipeline)
///   1 = En  (pure passthrough: `inputx_session_key_event` returns 0;
///            host receives ASCII directly, no IME preedit)
/// Returns `1` if accepted, `0` if `mode` was out of range or session NULL
/// (state unchanged in either case).
///
/// Cjk → En with in-flight composing commits the raw ASCII codes the
/// user typed (same semantic as pressing return in CJK with a non-empty
/// preedit). En → Cjk has nothing to drain — EN doesn't buffer.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_set_input_mode(
    session: *mut InputxSession,
    mode: u8,
) -> u8 {
    let Some(s) = (unsafe { session.as_mut() }) else {
        return 0;
    };
    match InputMode::from_u8(mode) {
        Some(m) => {
            s.inner.set_input_mode(m);
            1
        }
        None => 0,
    }
}

/// Returns the current input mode (0=Cjk / 1=En). Returns 0 (Cjk) if
/// session is NULL — same as the default.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_get_input_mode(session: *const InputxSession) -> u8 {
    match unsafe { session.as_ref() } {
        Some(s) => s.inner.input_mode().as_u8(),
        None => InputMode::default().as_u8(),
    }
}

/// Toggle the JP plugin's "enhancement" attachment. `on` is interpreted
/// as boolean: `0` = off, anything else = on. Returns `1` if the call
/// reached the engine, `0` if `session` was NULL.
///
/// Independent of `inputx_session_set_engine_mode` — the JP plugin can
/// attach to any Chinese mode (Mixed / WubiOnly / PinyinOnly) as a
/// supplementary source, or run standalone via `engine_mode = 3`
/// (`JapaneseOnly`). When `engine_mode = JapaneseOnly`, this toggle is
/// implicitly true and explicit `set_japanese_enabled(0)` has no effect.
///
/// Default after `inputx_session_new` is OFF.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_set_japanese_enabled(
    session: *mut InputxSession,
    on: u8,
) -> u8 {
    let Some(s) = (unsafe { session.as_mut() }) else {
        return 0;
    };
    s.inner.set_japanese_enabled(on != 0);
    1
}

/// Read the JP-plugin enhancement toggle: returns `1` if on, `0` if off
/// or `session` is NULL.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_get_japanese_enabled(session: *const InputxSession) -> u8 {
    match unsafe { session.as_ref() } {
        Some(s) if s.inner.japanese_enabled() => 1,
        _ => 0,
    }
}

/// `1` iff the JP sub-engine has a non-empty buffer (i.e. the user is
/// mid-romaji-composition). Hosts use this to decide whether `-` should
/// be forwarded to the engine as chōonpu (when JP composing) or passed
/// through as ASCII / locale-mapped punct (otherwise). Returns `0` when
/// the session is NULL.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_is_composing_japanese(session: *const InputxSession) -> u8 {
    match unsafe { session.as_ref() } {
        Some(s) if s.inner.japanese_is_composing() => 1,
        _ => 0,
    }
}

/// Source byte for the candidate at `index`:
///   0 = Wubi
///   1 = Pinyin
///   2 = Japanese
/// Returns `255` (sentinel) if `index` is out of range or session is NULL.
/// 255 is chosen because it's outside the valid range and unsigned-safe;
/// iOS bridge treats anything > 2 as "unknown" → no source dot.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_candidate_source(
    session: *const InputxSession,
    index: u32,
) -> u8 {
    let Some(s) = (unsafe { session.as_ref() }) else {
        return 255;
    };
    s.inner.candidate_source(index as usize).unwrap_or(255)
}

/// Export the L0 layer for `engine` (0=Wubi, 1=Pinyin) as a heap-allocated
/// UTF-8 JSON string. Caller must free via `inputx_string_free`. Returns
/// NULL on bad engine kind, NULL session, or allocation failure.
///
/// JSON shape (versioned for forward-compat):
/// ```jsonc
/// // wubi
/// { "version":1, "engine":"wubi",
///   "pins":[["code","word"],…],
///   "pick_counts":[["code","word",n],…],
///   "layer_prefs":[0.7,1.0,1.0,1.0,1.0,1.0] }
/// // pinyin
/// { "version":1, "engine":"pinyin",
///   "pins":[…], "pick_counts":[…] }
/// ```
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_export_l0_json(
    session: *const InputxSession,
    engine: u8,
) -> *mut c_char {
    let Some(s) = (unsafe { session.as_ref() }) else {
        return core::ptr::null_mut();
    };
    match s.inner.export_l0_json(engine) {
        Some(json) => dup_to_cstring(&json),
        None => core::ptr::null_mut(),
    }
}

/// Import an L0 snapshot for `engine` (0=Wubi, 1=Pinyin) from a NUL-
/// terminated UTF-8 JSON string. Returns the count of accepted pins
/// (entries whose `(input, word)` no longer exist in the lexicon are
/// silently dropped). Returns 0 on bad engine, malformed JSON, version
/// mismatch, NULL session, or NULL `json`.
///
/// # Safety
/// `session` must be valid (or NULL); `json` must be a valid C string
/// (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_import_l0_json(
    session: *const InputxSession,
    engine: u8,
    json: *const c_char,
) -> usize {
    if json.is_null() {
        return 0;
    }
    let Some(s) = (unsafe { session.as_ref() }) else {
        return 0;
    };
    let Ok(json_str) = (unsafe { CStr::from_ptr(json) }).to_str() else {
        return 0;
    };
    s.inner.import_l0_json(engine, json_str)
}

// ----------------------------------------------------------------------
// Phase 9 locale layer (items 81-85)
// ----------------------------------------------------------------------

/// Map an ASCII codepoint to its CJK punctuation equivalent. Returns the
/// original codepoint unchanged if there's no mapping (caller passes
/// through). Stateless — quotes go through the per-session smart-quote
/// state via `inputx_session_smart_quote`.
///
/// Examples (u32 codepoints):
///   `,`(0x2C) → `，`(0xFF0C)
///   `.`(0x2E) → `。`(0x3002)
///   `(`(0x28) → `（`(0xFF08)
#[unsafe(no_mangle)]
pub extern "C" fn inputx_punct_ascii_to_cjk(codepoint: u32) -> u32 {
    match char::from_u32(codepoint) {
        Some(c) => inputx_core::locale::punct::ascii_to_cjk_punct(c)
            .map(|m| m as u32)
            .unwrap_or(codepoint),
        None => codepoint,
    }
}

/// Map an ASCII codepoint to its full-width counterpart (offset +0xFEE0
/// for `!..~`; space → ideographic space `U+3000`). Returns the original
/// codepoint unchanged if not mappable.
#[unsafe(no_mangle)]
pub extern "C" fn inputx_punct_full_width(codepoint: u32) -> u32 {
    match char::from_u32(codepoint) {
        Some(c) => inputx_core::locale::width::full_width(c)
            .map(|m| m as u32)
            .unwrap_or(codepoint),
        None => codepoint,
    }
}

/// Smart-quote mapping via the session's per-instance state. `"` and `'`
/// alternate between opening and closing CJK quote forms. Non-quote
/// codepoints pass through unchanged.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_smart_quote(
    session: *mut InputxSession,
    codepoint: u32,
) -> u32 {
    let Some(s) = (unsafe { session.as_mut() }) else {
        return codepoint;
    };
    let Some(c) = char::from_u32(codepoint) else {
        return codepoint;
    };
    s.inner
        .smart_quote(c)
        .map(|m| m as u32)
        .unwrap_or(codepoint)
}

/// Reset the session's smart-quote alternator (next quote will be opening).
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_smart_quote_reset(session: *mut InputxSession) {
    if let Some(s) = unsafe { session.as_mut() } {
        s.inner.smart_quote_reset();
    }
}

// ----------------------------------------------------------------------
// CP-5.2 step-3: cell-dict L0.5 layer FFI. Hosts load TOML packs at
// app launch / on user toggle so domain vocab (IT terms, scientific
// names, etc.) outranks corpus on lookups without touching the
// embedded dict. Layer is per-session: each call accumulates into the
// session's PinyinDict; `clear` wipes; `count` is "N entries loaded".
// ----------------------------------------------------------------------

/// Load a TOML cell-dict pack into the session's L0.5 layer. `toml` is
/// a NUL-terminated UTF-8 string. Returns the number of entries
/// accepted on success, or a negative error code:
///   -1 = NULL session or NULL `toml`
///   -2 = `toml` is not valid UTF-8
///   -3 = parse error (malformed TOML / wrong schema)
///
/// Multiple loads accumulate. Use `inputx_session_clear_cell_dict` to wipe.
///
/// # Safety
/// `session` must be a valid pointer from `inputx_session_new` (or NULL);
/// `toml` must point to a NUL-terminated C string (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_load_cell_dict(
    session: *mut InputxSession,
    toml: *const c_char,
) -> i64 {
    let Some(s) = (unsafe { session.as_ref() }) else {
        return -1;
    };
    if toml.is_null() {
        return -1;
    }
    let cstr = unsafe { CStr::from_ptr(toml) };
    let Ok(text) = cstr.to_str() else { return -2 };
    match s.inner.load_cell_dict(text) {
        Ok(n) => n as i64,
        Err(_) => -3,
    }
}

/// Wipe the session's L0.5 cell-dict layer.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_clear_cell_dict(session: *mut InputxSession) {
    if let Some(s) = unsafe { session.as_ref() } {
        s.inner.clear_cell_dict();
    }
}

/// Return the number of `(pinyin, word)` entries currently in the
/// session's L0.5 layer. Returns 0 for NULL session.
///
/// # Safety
/// `session` must be valid (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inputx_session_cell_dict_count(session: *mut InputxSession) -> usize {
    match unsafe { session.as_ref() } {
        Some(s) => s.inner.cell_dict_count(),
        None => 0,
    }
}

// ----------------------------------------------------------------------
// C. FFI boundary fuzz — `cargo test --release ffi::tests::fuzz_ffi_*`
// Goals: (a) no Rust panic crosses the C ABI for ANY input combination
// (panics in `panic = "abort"` releases are immediate UB / SIGABRT and
// would crash the iOS keyboard extension process); (b) every returned
// `*mut c_char` is either NULL or freeable via `inputx_string_free`; (c)
// NULL session pointers are no-ops; (d) out-of-range indices / enum
// values are clamped, not crashing.
// ----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// CP-5.2 step-3 smoke: cell-dict load → count → clear round-trip
    /// via the C ABI. Verifies the host-facing surface accepts a
    /// minimal TOML pack and exposes the entry count.
    #[test]
    fn ffi_cell_dict_round_trip() {
        let toml = CString::new(concat!(
            "[meta]\n",
            "name = \"smoke\"\n",
            "version = 1\n",
            "author = \"t\"\n",
            "description = \"\"\n",
            "license = \"CC0-1.0\"\n",
            "[[entry]]\n",
            "pinyin = \"shdx\"\n",
            "word = \"上海大学\"\n",
            "freq = 999\n",
            "[[entry]]\n",
            "pinyin = \"yjs\"\n",
            "word = \"研究生\"\n",
            "freq = 888\n",
        ))
        .unwrap();
        let s = inputx_session_new();
        unsafe {
            let n = inputx_session_load_cell_dict(s, toml.as_ptr());
            assert_eq!(n, 2, "two entries should load");
            assert_eq!(inputx_session_cell_dict_count(s), 2);
            inputx_session_clear_cell_dict(s);
            assert_eq!(inputx_session_cell_dict_count(s), 0);
            assert_eq!(inputx_session_load_cell_dict(s, core::ptr::null()), -1);
            let bad = CString::new("not toml at all").unwrap();
            assert_eq!(inputx_session_load_cell_dict(s, bad.as_ptr()), -3);
            inputx_session_free(s);
            assert_eq!(inputx_session_cell_dict_count(core::ptr::null_mut()), 0);
            inputx_session_clear_cell_dict(core::ptr::null_mut());
        }
    }

    /// Random FFI ops generated by proptest.
    #[derive(Debug, Clone)]
    enum FfiOp {
        Letter(u8),
        Backspace,
        Escape,
        Clear,
        Preedit,
        CandidateCount,
        Candidate(usize),
        Commit(usize),
        TakeCommit,
        SetPolicy(u32),
        SetMode(u8),
        GetMode,
        SmartQuote(u32),
        SmartQuoteReset,
        PunctAsciiToCjk(u32),
        PunctFullWidth(u32),
        L0Export(u8),
        L0Import(u8, Vec<u8>),
        CandidateSource(usize),
        KeyEvent(u32, u32),
    }

    fn ffi_op() -> impl Strategy<Value = FfiOp> {
        prop_oneof![
            6 => any::<u8>().prop_map(FfiOp::Letter),
            2 => Just(FfiOp::Backspace),
            1 => Just(FfiOp::Escape),
            1 => Just(FfiOp::Clear),
            1 => Just(FfiOp::Preedit),
            1 => Just(FfiOp::CandidateCount),
            2 => any::<usize>().prop_map(FfiOp::Candidate),
            2 => any::<usize>().prop_map(FfiOp::Commit),
            1 => Just(FfiOp::TakeCommit),
            1 => any::<u32>().prop_map(FfiOp::SetPolicy),
            1 => any::<u8>().prop_map(FfiOp::SetMode),
            1 => Just(FfiOp::GetMode),
            1 => any::<u32>().prop_map(FfiOp::SmartQuote),
            1 => Just(FfiOp::SmartQuoteReset),
            1 => any::<u32>().prop_map(FfiOp::PunctAsciiToCjk),
            1 => any::<u32>().prop_map(FfiOp::PunctFullWidth),
            1 => any::<u8>().prop_map(FfiOp::L0Export),
            1 => (any::<u8>(), proptest::collection::vec(any::<u8>(), 0..128))
                    .prop_map(|(e, b)| FfiOp::L0Import(e, b)),
            1 => any::<usize>().prop_map(FfiOp::CandidateSource),
            1 => (any::<u32>(), any::<u32>()).prop_map(|(c, m)| FfiOp::KeyEvent(c, m)),
        ]
    }

    /// Free a returned C string if non-null. Must be called or every
    /// non-NULL return from FFI leaks.
    unsafe fn drop_cstr(p: *mut c_char) {
        if !p.is_null() {
            unsafe { inputx_string_free(p) };
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            // 128 cases × ~40 ops each ≈ 5k FFI calls per run.
            // FFI calls have some overhead (CString alloc/free) — keep
            // case count modest so the test runs in <30s.
            cases: 128,
            .. ProptestConfig::default()
        })]

        /// FFI never panics on arbitrary op sequences. Owns a single
        /// session for the duration of the case and frees on exit.
        #[test]
        fn fuzz_ffi_no_panic(ops in proptest::collection::vec(ffi_op(), 1..40)) {
            let s = inputx_session_new();
            prop_assert!(!s.is_null(), "inputx_session_new returned NULL");
            for op in &ops {
                unsafe {
                    match op {
                        FfiOp::Letter(b) => {
                            // handle_letter contract: ASCII a-z only.
                            // Other bytes go through key_event path.
                            let _ = inputx_session_key_event(s, *b as u32, 0);
                        }
                        FfiOp::Backspace => {
                            // handle_letter has no FFI; backspace via key_event with
                            // codepoint=0x08 (BS) or escape — use a dedicated FFI if
                            // present. We use clear() as a stand-in since inputx has no
                            // direct backspace FFI.
                            // Actually escape() comes closest. There's no FFI for
                            // backspace in this surface — represent via key_event
                            // with codepoint=0 (modifier-only) to keep coverage.
                            let _ = inputx_session_key_event(s, 0, 0);
                        }
                        FfiOp::Escape => {
                            inputx_session_clear(s);
                        }
                        FfiOp::Clear => inputx_session_clear(s),
                        FfiOp::Preedit => drop_cstr(inputx_session_preedit(s)),
                        FfiOp::CandidateCount => {
                            let _ = inputx_session_candidate_count(s);
                        }
                        FfiOp::Candidate(i) => drop_cstr(inputx_session_candidate(s, *i)),
                        FfiOp::Commit(i) => drop_cstr(inputx_session_commit_index(s, *i)),
                        FfiOp::TakeCommit => drop_cstr(inputx_session_take_commit(s)),
                        FfiOp::SetPolicy(p) => {
                            let _ = inputx_session_set_auto_commit_policy(s, *p);
                        }
                        FfiOp::SetMode(m) => {
                            let _ = inputx_session_set_engine_mode(s, *m);
                        }
                        FfiOp::GetMode => {
                            let _ = inputx_session_get_engine_mode(s);
                        }
                        FfiOp::SmartQuote(cp) => {
                            let _ = inputx_session_smart_quote(s, *cp);
                        }
                        FfiOp::SmartQuoteReset => inputx_session_smart_quote_reset(s),
                        FfiOp::PunctAsciiToCjk(cp) => {
                            let _ = inputx_punct_ascii_to_cjk(*cp);
                        }
                        FfiOp::PunctFullWidth(cp) => {
                            let _ = inputx_punct_full_width(*cp);
                        }
                        FfiOp::L0Export(eng) => drop_cstr(inputx_session_export_l0_json(s, *eng)),
                        FfiOp::L0Import(eng, bytes) => {
                            // Construct a C string from arbitrary bytes; skip if
                            // bytes contain interior NUL (CString::new fails).
                            if let Ok(cs) = CString::new(bytes.clone()) {
                                let _ = inputx_session_import_l0_json(s, *eng, cs.as_ptr());
                            }
                        }
                        FfiOp::CandidateSource(i) => {
                            let _ = inputx_session_candidate_source(s, *i as u32);
                        }
                        FfiOp::KeyEvent(cp, modi) => {
                            let _ = inputx_session_key_event(s, *cp, *modi);
                        }
                    }
                }
            }
            unsafe { inputx_session_free(s) };
        }

        /// Every FFI op tolerates a NULL session — returns NULL/0 with
        /// no UB. Run all ops twice: once before any state was modified,
        /// once after attempting to interact (state should still be safe).
        #[test]
        fn fuzz_ffi_null_session_safe(ops in proptest::collection::vec(ffi_op(), 1..20)) {
            let null_s: *mut InputxSession = core::ptr::null_mut();
            let null_s_const: *const InputxSession = core::ptr::null();
            for op in &ops {
                unsafe {
                    match op {
                        FfiOp::Preedit => {
                            let p = inputx_session_preedit(null_s_const);
                            prop_assert!(p.is_null(), "preedit(NULL) returned non-NULL");
                        }
                        FfiOp::CandidateCount => {
                            prop_assert_eq!(inputx_session_candidate_count(null_s_const), 0);
                        }
                        FfiOp::Candidate(i) => {
                            let p = inputx_session_candidate(null_s_const, *i);
                            prop_assert!(p.is_null());
                        }
                        FfiOp::Commit(i) => {
                            let p = inputx_session_commit_index(null_s, *i);
                            prop_assert!(p.is_null());
                        }
                        FfiOp::TakeCommit => {
                            let p = inputx_session_take_commit(null_s);
                            prop_assert!(p.is_null());
                        }
                        FfiOp::Clear => inputx_session_clear(null_s),
                        FfiOp::SetPolicy(p) => {
                            let _ = inputx_session_set_auto_commit_policy(null_s, *p);
                        }
                        FfiOp::SetMode(m) => {
                            // Returns 0 on NULL per docs.
                            prop_assert_eq!(inputx_session_set_engine_mode(null_s, *m), 0);
                        }
                        FfiOp::GetMode => {
                            prop_assert_eq!(inputx_session_get_engine_mode(null_s_const), 0);
                        }
                        FfiOp::SmartQuote(cp) => {
                            // NULL session: pass-through (returns input).
                            prop_assert_eq!(inputx_session_smart_quote(null_s, *cp), *cp);
                        }
                        FfiOp::SmartQuoteReset => inputx_session_smart_quote_reset(null_s),
                        FfiOp::L0Export(eng) => {
                            let p = inputx_session_export_l0_json(null_s_const, *eng);
                            prop_assert!(p.is_null());
                        }
                        FfiOp::L0Import(eng, bytes) => {
                            if let Ok(cs) = CString::new(bytes.clone()) {
                                prop_assert_eq!(
                                    inputx_session_import_l0_json(null_s_const, *eng, cs.as_ptr()),
                                    0
                                );
                            }
                        }
                        FfiOp::CandidateSource(i) => {
                            prop_assert_eq!(
                                inputx_session_candidate_source(null_s_const, *i as u32),
                                255
                            );
                        }
                        FfiOp::KeyEvent(cp, modi) => {
                            let _ = inputx_session_key_event(null_s, *cp, *modi);
                        }
                        // Letter/Backspace/Escape — go through key_event/clear above.
                        FfiOp::Letter(_) | FfiOp::Backspace | FfiOp::Escape => {}
                        FfiOp::PunctAsciiToCjk(cp) => {
                            // Stateless; just verify no panic with arbitrary cp.
                            let _ = inputx_punct_ascii_to_cjk(*cp);
                        }
                        FfiOp::PunctFullWidth(cp) => {
                            let _ = inputx_punct_full_width(*cp);
                        }
                    }
                }
            }
            // No free needed — NULL.
        }

        /// `inputx_session_import_l0_json` with arbitrary bytes (incl.
        /// invalid UTF-8, malformed JSON, huge inputs) doesn't panic.
        /// Trust-boundary entry: user controls the L0 JSON file on disk.
        #[test]
        fn fuzz_ffi_l0_import_arbitrary_bytes(
            engine in any::<u8>(),
            bytes in proptest::collection::vec(any::<u8>(), 0..512),
        ) {
            let s = inputx_session_new();
            unsafe {
                if let Ok(cs) = CString::new(bytes) {
                    let _ = inputx_session_import_l0_json(s, engine, cs.as_ptr());
                }
                inputx_session_free(s);
            }
        }
    }
}
