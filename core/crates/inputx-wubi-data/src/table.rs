//! Thin adapter over [`inputx_wubi::WubiDict`] (the embedded FST in the
//! sibling [`inputx-wubi`](https://crates.io/crates/inputx-wubi) crate). The dict
//! instance is process-global via `OnceLock`; L0 (per-user learning) state
//! therefore persists across `Session` instances within one process.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use inputx_wubi::{L0Snapshot, WubiDict};

static DICT: OnceLock<WubiDict> = OnceLock::new();

fn dict() -> &'static WubiDict {
    DICT.get_or_init(WubiDict::embedded)
}

/// Codepoint cutoff for "rare CJK". Anything ≥ this lands in CJK Extension
/// B (`U+20000`) or higher — blocks where most consumer fonts on iOS /
/// Android lack glyphs, so committing those characters into typical apps
/// renders as `?`. Default behavior of [`lookup`] is to filter them out;
/// power users can re-enable via [`set_show_rare`].
const RARE_CODEPOINT_THRESHOLD: u32 = 0x20000;

/// When `false` (default), `lookup` drops candidates containing any rare
/// CJK character. Industry-standard CJK IMEs (Apple, Sogou, Baidu) silently
/// hide these from candidate lists for the same reason — their dictionaries
/// have them but the host UI can't render them, so showing them is worse
/// than not.
static SHOW_RARE: AtomicBool = AtomicBool::new(false);

pub fn set_show_rare(show: bool) {
    SHOW_RARE.store(show, Ordering::Relaxed);
}

pub fn show_rare() -> bool {
    SHOW_RARE.load(Ordering::Relaxed)
}

/// Force-init the embedded `WubiDict` and exercise common lookup paths so
/// the OS faults the FST's `.rodata` pages into RAM and any internal `fst::Map`
/// streamer state is primed. Idempotent — relies on `OnceLock::get_or_init`
/// for the dict, and `WubiDict::lookup` for the page-touch effect. Called
/// from `Session::warmup` so a host can off-load the cold-path cost to a
/// background thread at startup instead of paying it on the user's first
/// keystroke. ~100-300ms on iPhone cold; <1ms idempotent.
pub fn warmup() {
    let d = dict();
    // 13 wubi codes spanning all 5 key zones (横/竖/撇/捺/折) — the FST
    // is laid out alphabetically, so this touches pages across the whole
    // .rodata range, not just one bucket.
    for code in &[
        "g", "h", "j", "k", "l", "m", "a", "s", "d", "f", "p", "q", "wq",
    ] {
        let _ = d.lookup(code);
    }
}

/// `true` iff every character in `word` is below the rare-CJK threshold
/// (`U+20000` — start of CJK Extension B). Pinyin-side composer also calls
/// this so the user-facing rare-char toggle applies uniformly to both
/// engines (item 54).
pub fn is_displayable(word: &str) -> bool {
    word.chars().all(|c| (c as u32) < RARE_CODEPOINT_THRESHOLD)
}

/// Exact lookup for `code`. Returns the candidates ranked by L0/L1, with
/// rare CJK candidates filtered unless `show_rare()` is `true`.
pub fn lookup(code: &str) -> Vec<String> {
    let mut all = dict().lookup(code);
    if !SHOW_RARE.load(Ordering::Relaxed) {
        all.retain(|w| is_displayable(w));
    }
    all
}

/// Scored variant of [`lookup`]. Returns `(word, score)` tuples for
/// the composite cross-engine merge. Rare-CJK filter applied here too.
pub fn lookup_with_scores(code: &str) -> Vec<(String, f64)> {
    let mut all: Vec<(String, f64)> = Vec::new();
    dict().lookup_with_scores_into(code, &mut all);
    if !SHOW_RARE.load(Ordering::Relaxed) {
        all.retain(|(w, _)| is_displayable(w));
    }
    all
}

/// Layer-aware variant: each candidate also carries its origin Layer
/// (Jianma1/2/3, Zigen, Phrase, Auto). Composite dispatch uses the
/// layer tag to make context-aware ranking decisions — e.g. demoting
/// low-confidence Auto / Phrase wubi candidates when the buffer shape
/// suggests pinyin intent, while keeping high-confidence Jianma simcodes
/// untouched (the 伙-rule: wubi simcodes always lead at their code).
pub fn lookup_with_layer(code: &str) -> Vec<(String, f64, inputx_wubi::Layer)> {
    let mut all: Vec<(String, f64, inputx_wubi::Layer)> = Vec::new();
    dict().lookup_with_layer_into(code, &mut all);
    if !SHOW_RARE.load(Ordering::Relaxed) {
        all.retain(|(w, _, _)| is_displayable(w));
    }
    all
}

/// Per-code lookup exposing raw frequency (separate from layer.base ·
/// pref) — used by the v1.4.7 composite hot path for orthodox
/// score decomposition into (log_prior_q4 = Q4·ln(1+freq),
/// log_likelihood_q4 = Q4·ln(layer.base() · pref · demotes)). Rare-CJK
/// filter applied uniformly with `lookup_with_layer`.
///
/// v1.4.7 sub-phase A4 step 2: data source is `wubi_idf_reader()`
/// (cement-owned `IdfReader` over `EMBEDDED_WUBI_IDF`) instead of the
/// facade `WubiDict::lookup_with_freq_layer_into`. The IDF entry
/// carries `raw_freq` losslessly and `Layer` via the engine_tag bits
/// of `EntryFlags`. Output is byte-equivalent to the previous facade
/// fill modulo the rare-CJK retain pass.
pub fn lookup_with_freq_layer(
    code: &str,
) -> Vec<(String, inputx_wubi::Layer, u64)> {
    let reader = crate::wubi_idf_reader();
    let entries = reader.lookup(code.as_bytes());
    let mut all: Vec<(String, inputx_wubi::Layer, u64)> = entries
        .into_iter()
        .map(|e| {
            (
                e.word.to_string(),
                crate::layer_from_idf_tag(e.flags.engine_tag()),
                e.raw_freq as u64,
            )
        })
        .collect();
    if !SHOW_RARE.load(Ordering::Relaxed) {
        all.retain(|(w, _, _)| is_displayable(w));
    }
    all
}

/// Prefix-prediction lookup: `(word, freq, code_len)` for every dict
/// entry whose code strictly extends `prefix` (no exact-code matches).
/// Rare-CJK filter applied uniformly with [`lookup`]. Wired into the
/// composite dispatch so Wubi gets the same prefix-prediction shape as
/// pinyin / JP (e.g. `jj` exact 是 stays at #0, predictions 日/时 follow).
///
/// v1.4.7 sub-phase A4 step 2: streamed through `wubi_idf_reader()
/// .prefix_for_each_entry` instead of `WubiDict::prefix_predictions`.
/// Strictly-extending filter (`code.len() > prefix.len()`) and the
/// `freq_desc → word_asc` sort match the facade output byte-for-byte
/// modulo the rare-CJK retain pass.
pub fn prefix_predictions(prefix: &str) -> Vec<(String, u64, usize)> {
    let reader = crate::wubi_idf_reader();
    let prefix_lower = prefix.to_ascii_lowercase();
    let prefix_len = prefix_lower.len();
    let mut all: Vec<(String, u64, usize)> = Vec::new();
    reader.prefix_for_each_entry(prefix_lower.as_bytes(), |e| {
        if e.code.len() <= prefix_len {
            return;
        }
        all.push((e.word.to_string(), e.raw_freq as u64, e.code.len()));
    });
    // freq desc, word asc — matches the facade ordering.
    all.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    if !SHOW_RARE.load(Ordering::Relaxed) {
        all.retain(|(w, _, _)| is_displayable(w));
    }
    all
}

/// Notify the dictionary that the user committed `word` for `code`. The
/// internal pick counter advances; on threshold the word auto-pins. All
/// learning logic lives in `wubi` — this is just a passthrough so the IME
/// layer doesn't need to know about counters.
pub fn record_pick(code: &str, word: &str) {
    dict().record_pick(code, word);
}

/// User-pinned word for `code`, if any. Cement passthrough so the
/// composite dispatch layer can re-apply the L0 pin × 1000 promotion
/// when wubi candidates come from `lookup_with_freq_layer` (which
/// returns raw per-entry data without pin promotion baked in).
pub fn pinned_word(code: &str) -> Option<String> {
    dict().pinned_word(code)
}

/// Snapshot the current L0 state (pins + pending pick counts + layer
/// prefs) for host-side persistence. Host stores it however it wants
/// (UserDefaults on Apple platforms, IndexedDB in web, etc.) and feeds
/// it back via [`import_l0`] on next launch.
pub fn export_l0() -> L0Snapshot {
    dict().export_l0()
}

/// Restore a previously-exported L0 snapshot. Entries whose `(code, word)`
/// no longer exist in the lexicon (e.g., after a wubi data version bump
/// removed an extension char) are silently dropped. Returns the count of
/// accepted pins.
pub fn import_l0(snap: L0Snapshot) -> usize {
    dict().import_l0(snap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_letter_jianma1_resolves() {
        // 一级简码: 'g' → 一 (canonical 86 standard)
        assert!(lookup("g").contains(&"一".to_string()));
    }

    #[test]
    fn keyname_zigen_full_code() {
        // 键名字根: 王 = gggg
        assert!(lookup("gggg").contains(&"王".to_string()));
    }

    #[test]
    fn unknown_returns_empty() {
        assert!(lookup("xyzz123").is_empty());
        assert!(lookup("").is_empty());
    }
}
