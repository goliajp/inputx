//! Top-level input mode — CJK vs EN. Orthogonal to `EngineMode`
//! (which picks wubi / pinyin / mixed *within* CJK mode).
//!
//! Default = `Cjk`. User toggles via shift-single-click on macOS or the
//! on-screen "中/EN" key on iOS. EN mode runs a pure ASCII preedit
//! pipeline; the composite engine is dormant.

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum InputMode {
    /// Composite CJK engine flow (wubi + pinyin per `EngineMode`).
    #[default]
    Cjk = 0,
    /// English preedit mode. Letters/digits/punct accumulate in the
    /// session's `en_preedit`; `return` commits without sending \n;
    /// `space` commits with a trailing ASCII space. Engine dormant.
    En = 1,
}

impl InputMode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Cjk),
            1 => Some(Self::En),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Cjk => Self::En,
            Self::En => Self::Cjk,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_cjk() {
        assert_eq!(InputMode::default(), InputMode::Cjk);
    }

    #[test]
    fn round_trip_u8() {
        for m in [InputMode::Cjk, InputMode::En] {
            assert_eq!(InputMode::from_u8(m.as_u8()), Some(m));
        }
        assert_eq!(InputMode::from_u8(99), None);
    }

    #[test]
    fn toggle_round_trip() {
        assert_eq!(InputMode::Cjk.toggle(), InputMode::En);
        assert_eq!(InputMode::En.toggle(), InputMode::Cjk);
        assert_eq!(InputMode::Cjk.toggle().toggle(), InputMode::Cjk);
    }
}
