//! Layer taxonomy for dictionary entries.
//!
//! Every `(code, word)` belongs to exactly one [`Layer`] determined at build
//! time. The layer feeds two purposes:
//!
//! 1. **Coarse ordering** — [`LAYER_BASE`] gives each layer a numeric base
//!    weight so 一级简码 always outranks any 二级简码, etc., regardless of
//!    per-entry frequency.
//! 2. **User-tunable preference** — `WubiDict::set_layer_pref` lets the host
//!    multiply a layer's contribution at lookup time without touching data.
//!
//! Index values pack `(layer << FREQ_BITS) | freq_score` so the runtime can
//! read both in one stream pass. `freq_score` is the corpus-derived frequency
//! from `data/library.tsv` (capped at [`MAX_FREQ_SCORE`]; real data
//! tops out around 50k), normalized within layer.

/// Discriminants are **ascending priority**: `Auto = 0` is lowest, `Jianma1`
/// is highest. This makes the packed `(layer << FREQ_BITS) | freq` compare
/// correctly with raw `u64` ordering — higher u64 = higher priority — so the
/// build-time merge step can keep the larger value on collision without
/// special casing.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    /// Auto-decomposed CJK extension character — algorithmically valid but
    /// may pick a non-canonical 字根 sequence.
    Auto = 0,
    /// Multi-character phrase (词组).
    Phrase = 1,
    /// Hand-curated 字根/seed entry — pedagogically canonical decomposition.
    Zigen = 2,
    /// 三级简码 — three-letter shortcut.
    Jianma3 = 3,
    /// 二级简码 — two-letter shortcut.
    Jianma2 = 4,
    /// 一级简码 — single-letter shortcut (e.g., `g → 一`).
    Jianma1 = 5,
}

/// Total number of layers. Acts as the array length for `LAYER_BASE`,
/// `DEFAULT_LAYER_PREFS`, and any per-layer table the host might keep.
pub const LAYER_COUNT: usize = 6;

/// Per-layer base weight, **indexed by `Layer as usize`** (ascending). Values
/// are spaced so that any in-layer frequency score (capped well below the
/// gap) cannot reorder layers, but a sufficient `layer_pref` multiplier can.
pub const LAYER_BASE: [u64; LAYER_COUNT] = [
    100_000,   // [0] Auto
    400_000,   // [1] Phrase
    500_000,   // [2] Zigen
    600_000,   // [3] Jianma3
    800_000,   // [4] Jianma2
    1_000_000, // [5] Jianma1
];

/// Default `layer_prefs`, indexed by `Layer as usize`. `Auto` is dampened to
/// 0.7 so extension characters don't pollute the top of common 4-letter
/// codes; everything else is 1.0.
#[allow(dead_code)] // runtime-only; build.rs doesn't use prefs
pub const DEFAULT_LAYER_PREFS: [f64; LAYER_COUNT] = [
    0.7, // [0] Auto
    1.0, // [1] Phrase
    1.0, // [2] Zigen
    1.0, // [3] Jianma3
    1.0, // [4] Jianma2
    1.0, // [5] Jianma1
];

#[allow(dead_code)] // some methods are runtime-only; build.rs sees them as unused
impl Layer {
    /// Decode from the discriminant byte. `None` for any value outside
    /// `0..=5` — used by [`unpack`] to recover the layer from a packed FST
    /// value, falling back to [`Layer::Auto`] on corruption.
    pub const fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Auto),
            1 => Some(Self::Phrase),
            2 => Some(Self::Zigen),
            3 => Some(Self::Jianma3),
            4 => Some(Self::Jianma2),
            5 => Some(Self::Jianma1),
            _ => None,
        }
    }

    /// The discriminant byte (0..=5).
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Convert to a `usize` index suitable for `LAYER_BASE` /
    /// `DEFAULT_LAYER_PREFS` array access.
    pub const fn as_index(self) -> usize {
        self as usize
    }

    /// Layer base weight. Equivalent to `LAYER_BASE[self.as_index()]`.
    pub const fn base(self) -> u64 {
        LAYER_BASE[self as usize]
    }
}

/// Bits reserved for `freq_score` in the packed value. The corpus pipeline
/// caps freq at `max_freq_score` (65535 = 16 bits); 20 gives headroom.
/// Layer sits ABOVE freq so a larger packed u64 still means higher priority
/// (layer desc, then freq desc) — the invariant the build-time merge and the
/// inputx-fsa Dict's value-desc item order both rely on. Keeping the packed
/// value small (≤ ~2^23 vs the old ~2^58) is what lets the LEB128 value
/// encoding shrink from ~9 bytes to ~4 (zerodep E1).
const FREQ_BITS: u32 = 20;
const FREQ_MASK: u64 = (1 << FREQ_BITS) - 1;

/// Largest `freq_score` the packed value can represent (`2^FREQ_BITS - 1`).
/// The **single source of truth** for the freq domain — tests and any caller
/// that needs to clamp/validate import this rather than hardcoding a copy
/// (a stale copy in the proptest survived the E1 `FREQ_BITS` 56→20 change and
/// silently broke the invariants until proptest caught it).
pub const MAX_FREQ_SCORE: u64 = FREQ_MASK;

/// Pack `(layer, freq_score)` into a single u64 index value. `freq_score` is
/// **saturated** to [`MAX_FREQ_SCORE`] if it exceeds the field, never wrapped:
/// for an order-by-value structure a wraparound would invert priority (a very
/// high freq would pack to a tiny value and rank last), whereas clamping keeps
/// "higher freq → higher-or-equal priority". Real freqs (≤ ~50k) are far
/// inside the field, so this only matters as a defensive guarantee.
#[allow(dead_code)] // used by build.rs and at runtime; build_weights.rs doesn't pack
pub const fn pack(layer: Layer, freq_score: u64) -> u64 {
    let freq = if freq_score > FREQ_MASK { FREQ_MASK } else { freq_score };
    ((layer as u64) << FREQ_BITS) | freq
}

/// Reverse of [`pack`]. Unknown layer bytes fall back to [`Layer::Auto`]
/// (lowest priority) — preferable to panicking on a corrupt FST.
#[allow(dead_code)] // runtime-only; build.rs only uses pack
pub const fn unpack(packed: u64) -> (Layer, u64) {
    let layer_byte = (packed >> FREQ_BITS) as u8;
    let freq = packed & FREQ_MASK;
    let layer = match Layer::from_u8(layer_byte) {
        Some(l) => l,
        None => Layer::Auto,
    };
    (layer, freq)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        for &l in &[
            Layer::Jianma1,
            Layer::Jianma2,
            Layer::Jianma3,
            Layer::Zigen,
            Layer::Phrase,
            Layer::Auto,
        ] {
            for &f in &[0u64, 1, 1234, FREQ_MASK] {
                let p = pack(l, f);
                let (lu, fu) = unpack(p);
                assert_eq!(lu, l);
                assert_eq!(fu, f);
            }
        }
    }

    #[test]
    fn freq_overflow_is_saturated() {
        // Over-range freq clamps to MAX_FREQ_SCORE (not wrapped to 0) and the
        // layer is untouched — so an out-of-range freq still ranks at the top
        // of its layer rather than the bottom.
        for over in [FREQ_MASK + 1, u64::MAX, 1 << 40] {
            let p = pack(Layer::Phrase, over);
            let (l, f) = unpack(p);
            assert_eq!(l, Layer::Phrase);
            assert_eq!(f, FREQ_MASK);
        }
        // Saturated value must equal packing the exact max, and never spill
        // into the layer bits.
        assert_eq!(pack(Layer::Phrase, u64::MAX), pack(Layer::Phrase, FREQ_MASK));
    }

    #[test]
    fn layer_base_strict_ascending() {
        for w in LAYER_BASE.windows(2) {
            assert!(w[0] < w[1], "LAYER_BASE must be strictly ascending (Auto = lowest priority)");
        }
    }

    #[test]
    fn packed_u64_orders_by_priority() {
        // Higher-priority layer must produce a larger u64 even with zero
        // freq, so the build-time merge step (`if *w < weight`) keeps the
        // winner.
        let auto = pack(Layer::Auto, FREQ_MASK);
        let phrase = pack(Layer::Phrase, 0);
        assert!(phrase > auto, "Phrase + 0 freq must beat Auto + max freq");
        let jm1 = pack(Layer::Jianma1, 0);
        let jm2 = pack(Layer::Jianma2, FREQ_MASK);
        assert!(jm1 > jm2);
    }
}
