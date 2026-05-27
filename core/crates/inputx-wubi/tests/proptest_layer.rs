//! Property tests for the layer pack/unpack codec.

use proptest::prelude::*;

use inputx_wubi::Layer;
use inputx_wubi::layer::{MAX_FREQ_SCORE, pack, unpack};

fn layer_strategy() -> impl Strategy<Value = Layer> {
    prop_oneof![
        Just(Layer::Auto),
        Just(Layer::Phrase),
        Just(Layer::Zigen),
        Just(Layer::Jianma3),
        Just(Layer::Jianma2),
        Just(Layer::Jianma1),
    ]
}

proptest! {
    /// `unpack(pack(l, f)) == (l, f)` for any `f` in the valid domain
    /// `[0, MAX_FREQ_SCORE]`. (Imports the bound from the crate so it can't
    /// drift out of sync with `FREQ_BITS` — a hardcoded copy is exactly what
    /// silently broke after the E1 56→20 change.)
    #[test]
    fn pack_unpack_roundtrip(
        layer in layer_strategy(),
        freq in 0u64..=MAX_FREQ_SCORE,
    ) {
        let p = pack(layer, freq);
        let (l_out, f_out) = unpack(p);
        prop_assert_eq!(l_out, layer);
        prop_assert_eq!(f_out, freq);
    }

    /// Higher-priority layer always packs to a larger u64 — irrespective of
    /// freq. This is the invariant build.rs relies on for its `*w < weight`
    /// merge step.
    #[test]
    fn higher_priority_layer_dominates_freq(
        f1 in 0u64..=MAX_FREQ_SCORE,
        f2 in 0u64..=MAX_FREQ_SCORE,
    ) {
        // Auto + max freq must still be < Phrase + 0 freq, etc.
        prop_assert!(pack(Layer::Phrase, f2) > pack(Layer::Auto, f1));
        prop_assert!(pack(Layer::Zigen, f2) > pack(Layer::Phrase, f1));
        prop_assert!(pack(Layer::Jianma3, f2) > pack(Layer::Zigen, f1));
        prop_assert!(pack(Layer::Jianma2, f2) > pack(Layer::Jianma3, f1));
        prop_assert!(pack(Layer::Jianma1, f2) > pack(Layer::Jianma2, f1));
    }

    /// Within a layer, larger freq packs to a larger u64.
    #[test]
    fn within_layer_freq_orders(
        layer in layer_strategy(),
        a in 0u64..=MAX_FREQ_SCORE,
        b in 0u64..=MAX_FREQ_SCORE,
    ) {
        if a < b {
            prop_assert!(pack(layer, a) < pack(layer, b));
        } else if a > b {
            prop_assert!(pack(layer, a) > pack(layer, b));
        } else {
            prop_assert_eq!(pack(layer, a), pack(layer, b));
        }
    }

    /// Out-of-domain freq saturates to `MAX_FREQ_SCORE` (never wraps): it
    /// stays inside its layer and ranks at the top of it, and the packed value
    /// is monotonic non-decreasing in freq across the whole `u64` range — so a
    /// pathological huge freq can never invert priority.
    #[test]
    fn over_range_freq_saturates_and_stays_monotonic(
        layer in layer_strategy(),
        over in (MAX_FREQ_SCORE + 1)..=u64::MAX,
        any in any::<u64>(),
    ) {
        let (l, f) = unpack(pack(layer, over));
        prop_assert_eq!(l, layer);
        prop_assert_eq!(f, MAX_FREQ_SCORE);
        // monotone non-decreasing: a ≤ b ⟹ pack(a) ≤ pack(b)
        let lo = any.min(over);
        let hi = any.max(over);
        prop_assert!(pack(layer, lo) <= pack(layer, hi));
    }
}
