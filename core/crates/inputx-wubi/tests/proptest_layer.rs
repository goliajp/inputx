//! Property tests for the layer pack/unpack codec.

use proptest::prelude::*;

use wubi::Layer;
use wubi::layer::{pack, unpack};

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

const FREQ_MASK: u64 = 0x00FF_FFFF_FFFF_FFFF;

proptest! {
    /// `unpack(pack(l, f)) == (l, f)` for any valid `f` (≤ 56 bits).
    #[test]
    fn pack_unpack_roundtrip(
        layer in layer_strategy(),
        freq in 0u64..=FREQ_MASK,
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
        f1 in 0u64..=FREQ_MASK,
        f2 in 0u64..=FREQ_MASK,
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
        a in 0u64..=FREQ_MASK,
        b in 0u64..=FREQ_MASK,
    ) {
        if a < b {
            prop_assert!(pack(layer, a) < pack(layer, b));
        } else if a > b {
            prop_assert!(pack(layer, a) > pack(layer, b));
        } else {
            prop_assert_eq!(pack(layer, a), pack(layer, b));
        }
    }
}
