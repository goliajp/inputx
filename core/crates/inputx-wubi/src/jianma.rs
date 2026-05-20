//! 一级简码 lookup, backed by a compile-time `phf::Map<u8, char>` generated
//! by `build.rs` from `data/jianma1.txt`.
//!
//! 二级 / 三级简码 layers land in Phase 9.

include!(concat!(env!("OUT_DIR"), "/jianma1.phf.rs"));

/// Look up the 一级简码 character for a key letter (lowercase ASCII).
#[inline]
pub fn lookup_jianma1(letter: u8) -> Option<char> {
    JIANMA1.get(&letter.to_ascii_lowercase()).copied()
}

/// Iterate all (letter, char) pairs.
pub fn iter_jianma1() -> impl Iterator<Item = (u8, char)> + 'static {
    JIANMA1.entries().map(|(k, v)| (*k, *v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_25() {
        assert_eq!(JIANMA1.len(), 25);
        assert_eq!(lookup_jianma1(b'g'), Some('一'));
        assert_eq!(lookup_jianma1(b'q'), Some('我'));
        assert_eq!(lookup_jianma1(b'z'), None);
    }
}
