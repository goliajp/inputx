//! Engine mode for the composite (wubi + pinyin) router.
//!
//! Default = `Mixed` — wubi primary, pinyin fallback (万能/搜狗五笔 style).
//! User toggles via the App settings; `Mode::from_u8` is called from FFI
//! when the keyboard reads the App-Group `engineMode` value.

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    /// Both Chinese engines participate. Wubi runs first; pinyin
    /// contributes fallback / supplementary candidates. `z` prefix
    /// forces pinyin-only for that input (z is not a valid wubi 字根
    /// letter). If `enable_japanese` is on, JP candidates append after.
    #[default]
    Mixed = 0,
    /// Wubi only. Pinyin engine is dormant. JP may still append if
    /// `enable_japanese` is on.
    WubiOnly = 1,
    /// Pinyin only. Wubi engine is dormant. JP may still append if
    /// `enable_japanese` is on.
    PinyinOnly = 2,
    /// Japanese plugin runs standalone — wubi and pinyin are dormant
    /// regardless of `enable_japanese` (which is implicitly true here).
    /// Reach this mode when the user wants pure JP input with no
    /// Chinese-candidate interference.
    JapaneseOnly = 3,
}

impl Mode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Mixed),
            1 => Some(Self::WubiOnly),
            2 => Some(Self::PinyinOnly),
            3 => Some(Self::JapaneseOnly),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn allows_wubi(self) -> bool {
        matches!(self, Self::Mixed | Self::WubiOnly)
    }

    pub fn allows_pinyin(self) -> bool {
        matches!(self, Self::Mixed | Self::PinyinOnly)
    }

    /// `true` iff the JP plugin should participate regardless of the
    /// independent `enable_japanese` toggle. Today only `JapaneseOnly`
    /// forces this; other modes consult the toggle.
    pub fn forces_japanese(self) -> bool {
        matches!(self, Self::JapaneseOnly)
    }

    /// `true` iff JP is the *only* engine in this mode — used by the
    /// composite layer to short-circuit Chinese-engine work entirely.
    pub fn is_japanese_only(self) -> bool {
        matches!(self, Self::JapaneseOnly)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_mixed() {
        assert_eq!(Mode::default(), Mode::Mixed);
    }

    #[test]
    fn round_trip_u8() {
        for m in [Mode::Mixed, Mode::WubiOnly, Mode::PinyinOnly, Mode::JapaneseOnly] {
            assert_eq!(Mode::from_u8(m.as_u8()), Some(m));
        }
        assert_eq!(Mode::from_u8(99), None);
    }

    #[test]
    fn allows_predicates() {
        assert!(Mode::Mixed.allows_wubi());
        assert!(Mode::Mixed.allows_pinyin());
        assert!(Mode::WubiOnly.allows_wubi());
        assert!(!Mode::WubiOnly.allows_pinyin());
        assert!(!Mode::PinyinOnly.allows_wubi());
        assert!(Mode::PinyinOnly.allows_pinyin());
        assert!(!Mode::JapaneseOnly.allows_wubi());
        assert!(!Mode::JapaneseOnly.allows_pinyin());
    }

    #[test]
    fn japanese_only_forces_jp() {
        assert!(Mode::JapaneseOnly.forces_japanese());
        assert!(!Mode::Mixed.forces_japanese());
        assert!(Mode::JapaneseOnly.is_japanese_only());
        assert!(!Mode::Mixed.is_japanese_only());
    }
}
