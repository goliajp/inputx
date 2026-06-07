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

/// Per-session smart-quote state. Alternates between open and close
/// glyphs each time the user types `"` or `'`. The default state assumes
/// the next quote is opening (no preceding quote in the document).
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
}
