//! ASCII → CJK punctuation mapping.
//!
//! Apple, Sogou, Baidu IMEs all auto-convert ASCII punct to CJK forms when
//! the user is in Chinese input mode. Inputx mirrors that:
//!
//! ```text
//!   , → ，    . → 。    ! → ！    ? → ？
//!   ; → ；    : → ：    ( → （    ) → ）
//!   < → 《    > → 》    [ → 【    ] → 】
//!   \ → 、
//! ```
//!
//! Smart quotes (`"` `'`) need state to alternate open/close — kept in
//! [`SmartQuoteState`] for sessions that want full smart-quote behavior.
//! Stateless callers (most of v1) can use [`ascii_to_cjk_open_quote`] /
//! [`ascii_to_cjk_close_quote`] directly via the long-press variant
//! popup.

/// Map a single ASCII punctuation char to its CJK equivalent. Returns
/// `None` for chars without a CJK mapping (caller falls back to inserting
/// the original char unchanged).
pub fn ascii_to_cjk_punct(c: char) -> Option<char> {
    Some(match c {
        ',' => '，',
        '.' => '。',
        '!' => '！',
        '?' => '？',
        ';' => '；',
        ':' => '：',
        '(' => '（',
        ')' => '）',
        '<' => '《',
        '>' => '》',
        '[' => '【',
        ']' => '】',
        '\\' => '、',
        // Quotes go through SmartQuoteState; passthrough here.
        _ => return None,
    })
}

pub fn ascii_to_cjk_open_quote(c: char) -> Option<char> {
    Some(match c {
        '"' => '\u{201C}',  // LEFT DOUBLE QUOTATION MARK
        '\'' => '\u{2018}', // LEFT SINGLE QUOTATION MARK
        _ => return None,
    })
}

pub fn ascii_to_cjk_close_quote(c: char) -> Option<char> {
    Some(match c {
        '"' => '\u{201D}',  // RIGHT DOUBLE QUOTATION MARK
        '\'' => '\u{2019}', // RIGHT SINGLE QUOTATION MARK
        _ => return None,
    })
}

/// Decide the CJK curly form of an ASCII quote (`"` / `'`) from the
/// character immediately preceding the caret in the document.
///
/// This is the **stateless** smart-quote path: instead of an in-memory
/// open/close toggle (which resets on every IME switch, mouse click, or
/// mid-text edit), the direction is derived from document ground truth,
/// exactly like Apple's own smart quotes and the Typographizer library.
/// A host that can read the caret's preceding character (macOS IMK
/// `attributedSubstring(from:)`) should prefer this; hosts that cannot
/// read context fall back to [`SmartQuoteState`].
///
/// `prev` is the grapheme immediately before the caret, or `None` at the
/// very start of the document. Returns the curly glyph, or `None` if
/// `ascii_quote` is not `"` or `'` (caller inserts it unchanged).
///
/// Rule: emit a **closing** quote when the preceding character is
/// "inside-quotation" context — an alphanumeric (Latin / digit / CJK /
/// kana …), a closing bracket, or *our own* opening curly quote (so a
/// bare `"` right after `“` closes rather than doubling). Everything else
/// — start of document, whitespace, opening brackets, the *other* quote
/// type's opener, a closing curly quote, and all punctuation — emits an
/// **opening** quote.
///
/// Known single-char limitation: a sentence-ender inside a quotation
/// (`“…好。` then `"`) reads as opening, since one preceding char cannot
/// distinguish "period that closes an inner clause" from "period that
/// ends the previous sentence". Punctuation classification lives in
/// [`is_closing_quote_context`] and is unit-tested, so it is cheap to
/// tune if real usage demands it.
pub fn quote_for_preceding(prev: Option<char>, ascii_quote: char) -> Option<char> {
    let (open, close, own_opener) = match ascii_quote {
        '"' => ('\u{201C}', '\u{201D}', '\u{201C}'), // “ ” — own opener “
        '\'' => ('\u{2018}', '\u{2019}', '\u{2018}'), // ‘ ’ — own opener ‘
        _ => return None,
    };
    let closing = prev.is_some_and(|p| is_closing_quote_context(p, own_opener));
    Some(if closing { close } else { open })
}

/// Whether `prev` (the char before the caret) puts us *inside* a
/// quotation, so the next quote of the given type should close.
/// `own_opener` is the opening curly glyph of the quote type being typed
/// (`“` for double, `‘` for single) — matching it means "right after our
/// own opener", which closes to avoid `““`.
fn is_closing_quote_context(prev: char, own_opener: char) -> bool {
    if prev == own_opener {
        return true;
    }
    // Alphanumeric covers Latin letters, digits, CJK ideographs, kana,
    // Hangul, etc. — all "word content" that a closing quote follows.
    if prev.is_alphanumeric() {
        return true;
    }
    // Closing brackets: the quoted span sits after them (e.g. `(注)”`).
    matches!(
        prev,
        ')' | ']'
            | '}'
            | '\u{FF09}' // ）
            | '\u{3011}' // 】
            | '\u{300B}' // 》
            | '\u{300D}' // 」
            | '\u{300F}' // 』
            | '\u{3009}' // 〉
            | '\u{3015}' // 〕
            | '\u{FF3D}' // ］
            | '\u{FF5D}' // ｝
    )
}

/// Per-session smart-quote state. Alternates between open and close
/// glyphs each time the user types `"` or `'`. The default state assumes
/// the next quote is opening (no preceding quote in the document).
///
/// This is the **fallback** path for hosts that cannot read the caret's
/// document context; the stateless [`quote_for_preceding`] is preferred
/// when context is available.
#[derive(Debug, Clone, Copy, Default)]
pub struct SmartQuoteState {
    next_double_is_open: bool,
    next_single_is_open: bool,
}

impl SmartQuoteState {
    pub fn new() -> Self {
        Self {
            next_double_is_open: true,
            next_single_is_open: true,
        }
    }

    /// Map an ASCII quote to its smart-CJK form, advancing state.
    /// Non-quote chars pass through untouched (`None`).
    pub fn map(&mut self, c: char) -> Option<char> {
        match c {
            '"' => {
                let mapped = if self.next_double_is_open {
                    ascii_to_cjk_open_quote(c)
                } else {
                    ascii_to_cjk_close_quote(c)
                };
                self.next_double_is_open = !self.next_double_is_open;
                mapped
            }
            '\'' => {
                let mapped = if self.next_single_is_open {
                    ascii_to_cjk_open_quote(c)
                } else {
                    ascii_to_cjk_close_quote(c)
                };
                self.next_single_is_open = !self.next_single_is_open;
                mapped
            }
            _ => None,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Sync the toggle to a direction just decided by the stateless
    /// context path, so a subsequent quote typed in a context-less host
    /// continues from the correct side. `mapped` is the curly glyph that
    /// was emitted for ASCII quote `c`.
    pub fn sync_after(&mut self, c: char, mapped: char) {
        // Emitted an opener → the next same-type quote should close, and
        // vice-versa.
        let emitted_open = matches!(mapped, '\u{201C}' | '\u{2018}');
        match c {
            '"' => self.next_double_is_open = !emitted_open,
            '\'' => self.next_single_is_open = !emitted_open,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comma_period_etc() {
        assert_eq!(ascii_to_cjk_punct(','), Some('，'));
        assert_eq!(ascii_to_cjk_punct('.'), Some('。'));
        assert_eq!(ascii_to_cjk_punct('!'), Some('！'));
        assert_eq!(ascii_to_cjk_punct('?'), Some('？'));
        assert_eq!(ascii_to_cjk_punct(';'), Some('；'));
        assert_eq!(ascii_to_cjk_punct(':'), Some('：'));
    }

    #[test]
    fn brackets_paired() {
        assert_eq!(ascii_to_cjk_punct('('), Some('（'));
        assert_eq!(ascii_to_cjk_punct(')'), Some('）'));
        assert_eq!(ascii_to_cjk_punct('<'), Some('《'));
        assert_eq!(ascii_to_cjk_punct('>'), Some('》'));
        assert_eq!(ascii_to_cjk_punct('['), Some('【'));
        assert_eq!(ascii_to_cjk_punct(']'), Some('】'));
    }

    #[test]
    fn backslash_to_cjk_pause() {
        assert_eq!(ascii_to_cjk_punct('\\'), Some('、'));
    }

    #[test]
    fn unmapped_returns_none() {
        assert_eq!(ascii_to_cjk_punct('a'), None);
        assert_eq!(ascii_to_cjk_punct('1'), None);
        assert_eq!(ascii_to_cjk_punct('@'), None);
    }

    #[test]
    fn quotes_not_mapped_by_punct_fn() {
        // Quotes go through SmartQuoteState, not the static fn.
        assert_eq!(ascii_to_cjk_punct('"'), None);
        assert_eq!(ascii_to_cjk_punct('\''), None);
    }

    #[test]
    fn smart_quote_alternates_double() {
        let mut s = SmartQuoteState::new();
        assert_eq!(s.map('"'), Some('\u{201C}')); // open
        assert_eq!(s.map('"'), Some('\u{201D}')); // close
        assert_eq!(s.map('"'), Some('\u{201C}')); // open again
    }

    #[test]
    fn smart_quote_alternates_single_independently() {
        let mut s = SmartQuoteState::new();
        assert_eq!(s.map('\''), Some('\u{2018}')); // open
        assert_eq!(s.map('"'), Some('\u{201C}')); // open (independent)
        assert_eq!(s.map('\''), Some('\u{2019}')); // close
    }

    #[test]
    fn smart_quote_passthrough_for_non_quotes() {
        let mut s = SmartQuoteState::new();
        assert_eq!(s.map('a'), None);
        assert_eq!(s.map(','), None);
    }

    #[test]
    fn reset_returns_to_initial() {
        let mut s = SmartQuoteState::new();
        s.map('"');
        s.reset();
        assert_eq!(s.map('"'), Some('\u{201C}')); // back to open
    }

    // ---- stateless context-based path -----------------------------------

    #[test]
    fn ctx_start_of_document_opens() {
        assert_eq!(quote_for_preceding(None, '"'), Some('\u{201C}'));
        assert_eq!(quote_for_preceding(None, '\''), Some('\u{2018}'));
    }

    #[test]
    fn ctx_after_letter_or_cjk_closes() {
        // The core of the user's IME-switch case: caret follows content,
        // so the quote closes regardless of any prior toggle state.
        assert_eq!(quote_for_preceding(Some('好'), '"'), Some('\u{201D}'));
        assert_eq!(quote_for_preceding(Some('a'), '"'), Some('\u{201D}'));
        assert_eq!(quote_for_preceding(Some('7'), '"'), Some('\u{201D}'));
        assert_eq!(quote_for_preceding(Some('ん'), '\''), Some('\u{2019}'));
    }

    #[test]
    fn ctx_after_whitespace_opens() {
        assert_eq!(quote_for_preceding(Some(' '), '"'), Some('\u{201C}'));
        assert_eq!(quote_for_preceding(Some('\n'), '"'), Some('\u{201C}'));
        assert_eq!(quote_for_preceding(Some('\u{3000}'), '"'), Some('\u{201C}')); // 全角空格
    }

    #[test]
    fn ctx_after_clause_punctuation_opens() {
        // 他说：“…”  /  他说，“…”
        assert_eq!(quote_for_preceding(Some('：'), '"'), Some('\u{201C}'));
        assert_eq!(quote_for_preceding(Some('，'), '"'), Some('\u{201C}'));
        assert_eq!(quote_for_preceding(Some('。'), '"'), Some('\u{201C}'));
    }

    #[test]
    fn ctx_after_own_opener_closes_to_avoid_doubling() {
        // Typed “ then (switch away/back, nothing between) another " →
        // must close, never “”. This is the exact reset bug, fixed.
        assert_eq!(quote_for_preceding(Some('\u{201C}'), '"'), Some('\u{201D}'));
        assert_eq!(
            quote_for_preceding(Some('\u{2018}'), '\''),
            Some('\u{2019}')
        );
    }

    #[test]
    fn ctx_after_other_type_opener_opens_for_nesting() {
        // “‘…’”  — a single quote right after a double opener nests (opens).
        assert_eq!(
            quote_for_preceding(Some('\u{201C}'), '\''),
            Some('\u{2018}')
        );
        assert_eq!(quote_for_preceding(Some('\u{2018}'), '"'), Some('\u{201C}'));
    }

    #[test]
    fn ctx_after_closing_curly_opens_new_quote() {
        // …” "  — previous quotation finished, next is a fresh opener.
        assert_eq!(quote_for_preceding(Some('\u{201D}'), '"'), Some('\u{201C}'));
        assert_eq!(
            quote_for_preceding(Some('\u{2019}'), '\''),
            Some('\u{2018}')
        );
    }

    #[test]
    fn ctx_after_closing_bracket_closes() {
        assert_eq!(quote_for_preceding(Some(')'), '"'), Some('\u{201D}'));
        assert_eq!(quote_for_preceding(Some('）'), '"'), Some('\u{201D}'));
        assert_eq!(quote_for_preceding(Some('】'), '"'), Some('\u{201D}'));
    }

    #[test]
    fn ctx_after_opening_bracket_opens() {
        assert_eq!(quote_for_preceding(Some('('), '"'), Some('\u{201C}'));
        assert_eq!(quote_for_preceding(Some('（'), '"'), Some('\u{201C}'));
        assert_eq!(quote_for_preceding(Some('【'), '"'), Some('\u{201C}'));
    }

    #[test]
    fn ctx_non_quote_passes_through() {
        assert_eq!(quote_for_preceding(Some('a'), 'x'), None);
        assert_eq!(quote_for_preceding(None, ','), None);
    }
}
