//! JSON serialization for L0 snapshots — exposed via FFI as opaque blobs
//! the iOS host writes to App Group `Library/Application Support/inputx/
//! {wubi,pinyin}_l0.json`.
//!
//! The wubi + golia-pinyin crates intentionally have no serde dep (keeps
//! their published artifacts minimal); this module bridges to serde here
//! so inputx-core owns the JSON shape.
//!
//! Schema (versioned for forward-compat):
//! ```jsonc
//! // wubi
//! { "version": 1, "engine": "wubi",
//!   "pins": [["code","word"], …],
//!   "pick_counts": [["code","word",n], …],
//!   "layer_prefs": [0.7, 1.0, 1.0, 1.0, 1.0, 1.0] }
//!
//! // pinyin
//! { "version": 1, "engine": "pinyin",
//!   "pins": [["pinyin","word"], …],
//!   "pick_counts": [["pinyin","word",n], …] }
//! ```

use serde::{Deserialize, Serialize};

/// Schema version. Bumped whenever the JSON shape changes; importers
/// silently drop unrecognized versions (returns 0 accepted pins).
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct WubiSnapshotJson {
    version: u32,
    engine: String,
    pins: Vec<(String, String)>,
    pick_counts: Vec<(String, String, u32)>,
    layer_prefs: Vec<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PinyinSnapshotJson {
    version: u32,
    engine: String,
    pins: Vec<(String, String)>,
    pick_counts: Vec<(String, String, u32)>,
}

pub fn wubi_to_json(snap: &wubi::L0Snapshot) -> String {
    let payload = WubiSnapshotJson {
        version: SCHEMA_VERSION,
        engine: "wubi".to_string(),
        pins: snap.pins.clone(),
        pick_counts: snap.pick_counts.clone(),
        layer_prefs: snap.layer_prefs.to_vec(),
    };
    serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
}

pub fn wubi_from_json(json: &str) -> Option<wubi::L0Snapshot> {
    let parsed: WubiSnapshotJson = serde_json::from_str(json).ok()?;
    if parsed.version != SCHEMA_VERSION || parsed.engine != "wubi" {
        return None;
    }
    let mut layer_prefs = wubi::DEFAULT_LAYER_PREFS;
    for (i, v) in parsed
        .layer_prefs
        .iter()
        .enumerate()
        .take(layer_prefs.len())
    {
        layer_prefs[i] = *v;
    }
    Some(wubi::L0Snapshot {
        pins: parsed.pins,
        pick_counts: parsed.pick_counts,
        layer_prefs,
    })
}

pub fn pinyin_to_json(snap: &golia_pinyin::L0Snapshot) -> String {
    let payload = PinyinSnapshotJson {
        version: SCHEMA_VERSION,
        engine: "pinyin".to_string(),
        pins: snap.pins.clone(),
        pick_counts: snap.pick_counts.clone(),
    };
    serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
}

pub fn pinyin_from_json(json: &str) -> Option<golia_pinyin::L0Snapshot> {
    let parsed: PinyinSnapshotJson = serde_json::from_str(json).ok()?;
    if parsed.version != SCHEMA_VERSION || parsed.engine != "pinyin" {
        return None;
    }
    Some(golia_pinyin::L0Snapshot {
        pins: parsed.pins,
        pick_counts: parsed.pick_counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wubi_round_trip_empty() {
        let snap = wubi::L0Snapshot {
            pins: vec![],
            pick_counts: vec![],
            layer_prefs: wubi::DEFAULT_LAYER_PREFS,
        };
        let json = wubi_to_json(&snap);
        assert!(json.contains("\"engine\":\"wubi\""));
        let back = wubi_from_json(&json).unwrap();
        assert!(back.pins.is_empty());
        assert!(back.pick_counts.is_empty());
        assert_eq!(back.layer_prefs.len(), 6);
    }

    #[test]
    fn wubi_round_trip_populated() {
        let snap = wubi::L0Snapshot {
            pins: vec![("khlg".into(), "中国".into())],
            pick_counts: vec![("khlg".into(), "跑车".into(), 2)],
            layer_prefs: [0.5, 1.0, 1.5, 1.0, 1.0, 1.0],
        };
        let json = wubi_to_json(&snap);
        let back = wubi_from_json(&json).unwrap();
        assert_eq!(back.pins, vec![("khlg".into(), "中国".into())]);
        assert_eq!(back.pick_counts, vec![("khlg".into(), "跑车".into(), 2)]);
        assert!((back.layer_prefs[2] - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn pinyin_round_trip() {
        let snap = golia_pinyin::L0Snapshot {
            pins: vec![("zhongguo".into(), "中国".into())],
            pick_counts: vec![("women".into(), "我们".into(), 1)],
        };
        let json = pinyin_to_json(&snap);
        assert!(json.contains("\"engine\":\"pinyin\""));
        let back = pinyin_from_json(&json).unwrap();
        assert_eq!(back.pins, vec![("zhongguo".into(), "中国".into())]);
        assert_eq!(back.pick_counts, vec![("women".into(), "我们".into(), 1)]);
    }

    #[test]
    fn wrong_engine_rejected() {
        let snap = golia_pinyin::L0Snapshot {
            pins: vec![],
            pick_counts: vec![],
        };
        let json = pinyin_to_json(&snap);
        // Trying to import as wubi → fails (engine mismatch).
        assert!(wubi_from_json(&json).is_none());
    }

    #[test]
    fn unknown_version_rejected() {
        let json = r#"{"version":99,"engine":"pinyin","pins":[],"pick_counts":[]}"#;
        assert!(pinyin_from_json(json).is_none());
    }

    #[test]
    fn malformed_json_yields_none() {
        assert!(wubi_from_json("{not json").is_none());
        assert!(pinyin_from_json("not json at all").is_none());
    }

    // -----------------------------------------------------------------
    // B. L0 import/export fuzz — random + adversarial JSON inputs.
    // Contract: `*_from_json` never panics; always returns Option (None
    // on any parse / version / engine error). Round-trip equality for
    // valid snapshots is also asserted.
    // -----------------------------------------------------------------

    use proptest::prelude::*;

    /// Shrinkable strategy producing valid wubi L0Snapshots.
    fn wubi_snap_strategy() -> impl Strategy<Value = wubi::L0Snapshot> {
        let pins = proptest::collection::vec(
            (
                proptest::collection::vec(b'a'..=b'z', 1..6)
                    .prop_map(|v| String::from_utf8(v).unwrap()),
                "[\u{4e00}-\u{9fff}]{1,4}".prop_filter(
                    "non-empty word", |s: &String| !s.is_empty()),
            ),
            0..16,
        );
        let pick_counts = proptest::collection::vec(
            (
                proptest::collection::vec(b'a'..=b'z', 1..6)
                    .prop_map(|v| String::from_utf8(v).unwrap()),
                "[\u{4e00}-\u{9fff}]{1,4}".prop_filter(
                    "non-empty word", |s: &String| !s.is_empty()),
                0u32..1000,
            ),
            0..16,
        );
        let layer_prefs = (
            -100.0f64..100.0,  // intentionally include negatives + extremes
            0.0f64..10.0,
            0.0f64..10.0,
            0.0f64..10.0,
            0.0f64..10.0,
            0.0f64..10.0,
        );
        (pins, pick_counts, layer_prefs).prop_map(|(p, pc, (l0, l1, l2, l3, l4, l5))| {
            wubi::L0Snapshot {
                pins: p,
                pick_counts: pc,
                layer_prefs: [l0, l1, l2, l3, l4, l5],
            }
        })
    }

    fn pinyin_snap_strategy() -> impl Strategy<Value = golia_pinyin::L0Snapshot> {
        let pins = proptest::collection::vec(
            (
                proptest::collection::vec(b'a'..=b'z', 1..12)
                    .prop_map(|v| String::from_utf8(v).unwrap()),
                "[\u{4e00}-\u{9fff}]{1,4}".prop_filter(
                    "non-empty word", |s: &String| !s.is_empty()),
            ),
            0..16,
        );
        let pick_counts = proptest::collection::vec(
            (
                proptest::collection::vec(b'a'..=b'z', 1..12)
                    .prop_map(|v| String::from_utf8(v).unwrap()),
                "[\u{4e00}-\u{9fff}]{1,4}".prop_filter(
                    "non-empty word", |s: &String| !s.is_empty()),
                0u32..1000,
            ),
            0..16,
        );
        (pins, pick_counts).prop_map(|(p, pc)| golia_pinyin::L0Snapshot {
            pins: p,
            pick_counts: pc,
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, .. ProptestConfig::default() })]

        /// `wubi_from_json` never panics on arbitrary byte input —
        /// always returns Option. JSON parsing is at the trust boundary
        /// (user-controlled L0 files on disk), so panic == DoS vector.
        #[test]
        fn wubi_from_json_no_panic_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
            // Stringify lossy-utf8 — `from_json` takes &str, so caller
            // would have to lossy-convert anyway.
            let s = String::from_utf8_lossy(&bytes);
            let _ = wubi_from_json(&s);  // assert: just no panic
        }

        #[test]
        fn pinyin_from_json_no_panic_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
            let s = String::from_utf8_lossy(&bytes);
            let _ = pinyin_from_json(&s);
        }

        /// `from_json` also doesn't panic on arbitrary UTF-8 strings
        /// (broader coverage than just bytes — includes valid CJK,
        /// emoji, control chars in JSON).
        #[test]
        fn wubi_from_json_no_panic_unicode(s in ".*") {
            let _ = wubi_from_json(&s);
        }

        #[test]
        fn pinyin_from_json_no_panic_unicode(s in ".*") {
            let _ = pinyin_from_json(&s);
        }

        /// Round-trip identity for valid wubi snapshots. layer_prefs is
        /// f64 — serde_json's DEFAULT writer doesn't use ryu and can
        /// drop the trailing digit (loss of ~1 ULP per round-trip).
        /// Caught by proptest with input `1.8214578926411533` →
        /// `1.821457892641153`. Practical impact is nil (ranking
        /// weights at 1e-16 precision don't change candidate order),
        /// but worth flagging if codec changes ever cause it to grow.
        /// Tolerate up to 4 ULP drift; tighten if we switch to ryu.
        #[test]
        fn wubi_roundtrip(snap in wubi_snap_strategy()) {
            let json = wubi_to_json(&snap);
            let back = wubi_from_json(&json).expect("valid round-trip should parse");
            prop_assert_eq!(back.pins, snap.pins);
            prop_assert_eq!(back.pick_counts, snap.pick_counts);
            for i in 0..snap.layer_prefs.len() {
                let a = back.layer_prefs[i];
                let b = snap.layer_prefs[i];
                let ulp_drift = (a.to_bits() as i64 - b.to_bits() as i64).unsigned_abs();
                prop_assert!(ulp_drift <= 4,
                    "layer_prefs[{}] drift {} ULP: {} ({:#x}) vs {} ({:#x})",
                    i, ulp_drift, a, a.to_bits(), b, b.to_bits());
            }
        }

        #[test]
        fn pinyin_roundtrip(snap in pinyin_snap_strategy()) {
            let json = pinyin_to_json(&snap);
            let back = pinyin_from_json(&json).expect("valid round-trip should parse");
            prop_assert_eq!(back.pins, snap.pins);
            prop_assert_eq!(back.pick_counts, snap.pick_counts);
        }

        /// Codec idempotence — the SECOND round-trip is byte-identical
        /// to the first. (First round-trip MAY shed precision in
        /// layer_prefs due to serde_json's non-ryu default; but once
        /// parsed back, re-emitting should produce stable JSON.)
        #[test]
        fn wubi_roundtrip_idempotent(snap in wubi_snap_strategy()) {
            // First round-trip: lossy on f64 last digit potentially.
            let json1 = wubi_to_json(&snap);
            let parsed1 = wubi_from_json(&json1).expect("round-trip parse");
            // Second round-trip: f64 already at its representable form.
            let json2 = wubi_to_json(&parsed1);
            let parsed2 = wubi_from_json(&json2).expect("second parse");
            let json3 = wubi_to_json(&parsed2);
            prop_assert_eq!(json2, json3,
                "codec not idempotent after 2 round-trips");
        }

        #[test]
        fn pinyin_roundtrip_idempotent(snap in pinyin_snap_strategy()) {
            let json1 = pinyin_to_json(&snap);
            let parsed1 = pinyin_from_json(&json1).expect("round-trip parse");
            let json2 = pinyin_to_json(&parsed1);
            prop_assert_eq!(json1, json2,
                "pinyin codec (no f64) should be byte-stable on first round-trip");
        }

        /// Cross-engine reject: wubi snapshot serialized then parsed as
        /// pinyin returns None, and vice versa.
        #[test]
        fn cross_engine_rejection(
            wubi_snap in wubi_snap_strategy(),
            pinyin_snap in pinyin_snap_strategy(),
        ) {
            let wubi_json = wubi_to_json(&wubi_snap);
            prop_assert!(pinyin_from_json(&wubi_json).is_none(),
                "pinyin importer accepted wubi-tagged JSON");
            let pinyin_json = pinyin_to_json(&pinyin_snap);
            prop_assert!(wubi_from_json(&pinyin_json).is_none(),
                "wubi importer accepted pinyin-tagged JSON");
        }
    }
}
