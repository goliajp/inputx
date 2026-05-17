//! ASCII ↔ full-width letter/digit/punct mapping.
//!
//! Full-width forms live in `U+FF01..=U+FF5E` (corresponding to ASCII
//! `!..=~` minus space). Conversion is a fixed offset of `+0xFEE0`.
//!
//! Example: `'a' (U+0061)` ↔ `'ａ' (U+FF41)`.
//!
//! ASCII space `' '` (U+0020) maps to ideographic space `'　'` (U+3000) —
//! special-cased outside the FF01..FF5E block.

const FW_OFFSET: u32 = 0xFEE0;
const ASCII_SPACE: char = ' ';
const IDEOGRAPHIC_SPACE: char = '\u{3000}';

/// ASCII char → full-width counterpart. Returns `None` if `c` has no
/// full-width equivalent (e.g., control chars, non-ASCII).
pub fn full_width(c: char) -> Option<char> {
    let code = c as u32;
    if c == ASCII_SPACE {
        return Some(IDEOGRAPHIC_SPACE);
    }
    if (0x21..=0x7E).contains(&code) {
        return char::from_u32(code + FW_OFFSET);
    }
    None
}

/// Full-width char → ASCII counterpart. Returns `None` if `c` is not a
/// full-width form. Kept as the inverse of `full_width` (round-trip tests
/// depend on it) even though the iOS keyboard path only needs the forward
/// direction today.
#[allow(dead_code)]
pub fn half_width(c: char) -> Option<char> {
    let code = c as u32;
    if c == IDEOGRAPHIC_SPACE {
        return Some(ASCII_SPACE);
    }
    if (0xFF01..=0xFF5E).contains(&code) {
        return char::from_u32(code - FW_OFFSET);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_round_trip() {
        for c in 'a'..='z' {
            let fw = full_width(c).unwrap();
            assert_eq!(half_width(fw), Some(c), "{c} round-trip via {fw}");
        }
        for c in 'A'..='Z' {
            let fw = full_width(c).unwrap();
            assert_eq!(half_width(fw), Some(c));
        }
    }

    #[test]
    fn digits_round_trip() {
        for c in '0'..='9' {
            let fw = full_width(c).unwrap();
            assert_eq!(half_width(fw), Some(c));
        }
    }

    #[test]
    fn known_examples() {
        assert_eq!(full_width('a'), Some('\u{FF41}'));
        assert_eq!(full_width('A'), Some('\u{FF21}'));
        assert_eq!(full_width('5'), Some('\u{FF15}'));
        assert_eq!(full_width('!'), Some('\u{FF01}'));
        assert_eq!(full_width('~'), Some('\u{FF5E}'));
    }

    #[test]
    fn space_special_case() {
        assert_eq!(full_width(' '), Some('\u{3000}'));
        assert_eq!(half_width('\u{3000}'), Some(' '));
    }

    #[test]
    fn non_ascii_returns_none() {
        assert_eq!(full_width('中'), None);
        assert_eq!(full_width('\n'), None);
        assert_eq!(half_width('a'), None);
        assert_eq!(half_width('中'), None);
    }
}
