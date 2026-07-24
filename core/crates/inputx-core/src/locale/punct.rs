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
/// document text preceding the caret — the **stateless** smart-quote path.
///
/// Instead of an in-memory open/close toggle (which resets on every IME
/// switch, mouse click, or mid-text edit), direction is derived from
/// document ground truth: are we currently *inside* an unclosed quotation
/// of this type? If so, close; otherwise open.
///
/// This is nesting-aware rather than the single-preceding-char heuristic
/// Apple/Typographizer use, because the latter assumes a space precedes an
/// opening quote — true for English (`he said "hi"`) but false for Chinese
/// (`他说“你好”`, no space), where "preceding char is a letter" would wrongly
/// force a closing quote at the *start* of a quotation. Counting open/close
/// depth avoids that: `他说“` → depth 1 → next `"` closes; `他说` (no quote)
/// → depth 0 → next `"` opens.
///
/// `ctx_before` is the document text up to the caret (a bounded window is
/// fine — see the line-scoping below). Returns the curly glyph, or `None`
/// if `ascii_quote` is not `"` / `'` (caller inserts it unchanged).
///
/// Scope: only the current line (text after the last `\n` in `ctx_before`)
/// is counted. Quotations are opened and closed within one line/paragraph
/// in normal typing, and line-scoping stops a stray unclosed quote on an
/// earlier line from forcing every later quote to close.
pub fn quote_direction(ctx_before: &str, ascii_quote: char) -> Option<char> {
    let (open, close) = match ascii_quote {
        '"' => ('\u{201C}', '\u{201D}'),  // “ ”
        '\'' => ('\u{2018}', '\u{2019}'), // ‘ ’
        _ => return None,
    };
    let line = ctx_before.rsplit('\n').next().unwrap_or(ctx_before);
    let mut depth: i32 = 0;
    for ch in line.chars() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
        }
    }
    // depth > 0 → an opener of this type is still unclosed → we're inside.
    Some(if depth > 0 { close } else { open })
}

/// Per-session smart-quote state. Alternates between open and close
/// glyphs each time the user types `"` or `'`. The default state assumes
/// the next quote is opening (no preceding quote in the document).
///
/// This is the **fallback** path for hosts that cannot read the caret's
/// document context; the stateless [`quote_direction`] is preferred
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

    // ---- stateless context-based path (nesting-aware) -------------------

    const OPEN_D: char = '“';
    const CLOSE_D: char = '”';
    const OPEN_S: char = '‘';
    const CLOSE_S: char = '’';

    #[test]
    fn ctx_empty_opens() {
        assert_eq!(quote_direction("", '"'), Some(OPEN_D));
        assert_eq!(quote_direction("", '\''), Some(OPEN_S));
    }

    #[test]
    fn ctx_open_quote_mid_cjk_text() {
        // The dominant Chinese pattern: 他说"你好" — no space before the
        // opening quote. Depth 0 → open. (The single-char heuristic got
        // this WRONG, forcing a closing quote after 说.)
        assert_eq!(quote_direction("他说", '"'), Some(OPEN_D));
        assert_eq!(quote_direction("他对我说", '"'), Some(OPEN_D));
    }

    #[test]
    fn ctx_close_quote_when_inside() {
        // 他说“你好 + " → inside (depth 1) → close.
        assert_eq!(quote_direction("他说“你好", '"'), Some(CLOSE_D));
        // The user's IME-switch case: “ then text typed via another IME.
        assert_eq!(
            quote_direction("“然后切去别的输入法打字", '"'),
            Some(CLOSE_D)
        );
    }

    #[test]
    fn ctx_balanced_pair_opens_next() {
        // 他说“你好” + " → a fresh quotation opens.
        assert_eq!(quote_direction("他说“你好”", '"'), Some(OPEN_D));
    }

    #[test]
    fn ctx_close_after_inner_punctuation() {
        // The cases the single-char rule got wrong: punctuation inside a
        // quotation still closes because depth > 0.
        assert_eq!(quote_direction("“今天很好。", '"'), Some(CLOSE_D)); // 句号
        assert_eq!(quote_direction("“好，", '"'), Some(CLOSE_D)); // 逗号
        assert_eq!(quote_direction("“真的吗？", '"'), Some(CLOSE_D)); // 问号
    }

    #[test]
    fn ctx_open_new_sentence_quote_after_period() {
        // 他走了。 + " → outside any quote → open (not close). Same
        // preceding-char class as the case above, resolved by depth
        // rather than the ambiguous single char.
        assert_eq!(quote_direction("他走了。", '"'), Some(OPEN_D));
    }

    #[test]
    fn ctx_single_and_double_counted_independently() {
        // “他说‘你好’ — double still open, inner single closed.
        assert_eq!(quote_direction("“他说‘你好’", '"'), Some(CLOSE_D));
        assert_eq!(quote_direction("“他说‘你好’", '\''), Some(OPEN_S));
        assert_eq!(quote_direction("“他说", '\''), Some(OPEN_S)); // inner opens inside outer
        assert_eq!(quote_direction("“他说‘你好", '\''), Some(CLOSE_S)); // inner still open → close
    }

    #[test]
    fn ctx_line_scoped_ignores_earlier_lines() {
        // An unclosed quote on a previous line does not force a close on
        // the current line.
        assert_eq!(quote_direction("“上一行没闭合\n他说", '"'), Some(OPEN_D));
        // …but an unclosed quote on the CURRENT line does.
        assert_eq!(quote_direction("上一行\n他说“你好", '"'), Some(CLOSE_D));
    }

    #[test]
    fn ctx_non_quote_passes_through() {
        assert_eq!(quote_direction("他说", 'x'), None);
        assert_eq!(quote_direction("", ','), None);
    }
}
