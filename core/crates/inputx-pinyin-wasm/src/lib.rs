//! WASM bindings for `inputx-pinyin`. Exposes a small, JS-friendly API
//! that browser and Node consumers can import via `wasm-pack` output.
//!
//! Build:
//!     wasm-pack build crates/inputx-pinyin-wasm --target web --release
//!
//! Default build bakes the bootstrap dict (1.7 KB) — the full 15 MB
//! `pinyin.fst` is too large for typical wasm bundles. Item 33 of the
//! workspace ROADMAP is the proper streaming-load story; until then,
//! pass `--no-default-features` (which propagates to the lib crate) to
//! bake the full dict.
//!
//! Usage (JS):
//! ```js
//! import init, { PinyinEngine } from "@goliapkg/pinyin";
//! await init();
//! const eng = new PinyinEngine();
//! const candidates = eng.lookup("zhongguo");      // ["中国", ...]
//! eng.recordPick("zhongguo", "中国");              // user picked candidate
//! const state = eng.exportL0();                    // structured object
//! localStorage.setItem("pinyin-l0", JSON.stringify(state));
//! // later:
//! eng.importL0(JSON.parse(localStorage.getItem("pinyin-l0")));
//! ```

use wasm_bindgen::prelude::*;

use inputx_pinyin::{L0Snapshot, PinyinDict, char_to_pinyin};

/// Pinyin engine wrapping the embedded FST dictionary plus a per-instance
/// L0 layer (in-memory; persistence is up to the host via `exportL0` /
/// `importL0`).
#[wasm_bindgen]
pub struct PinyinEngine {
    dict: PinyinDict,
}

#[wasm_bindgen]
impl PinyinEngine {
    /// Construct from the embedded FST. Cheap (validates the FST header
    /// and initializes empty L0).
    #[wasm_bindgen(constructor)]
    pub fn new() -> PinyinEngine {
        Self {
            dict: PinyinDict::embedded(),
        }
    }

    /// Total number of `(pinyin, word)` entries in the embedded FST.
    #[wasm_bindgen(getter)]
    pub fn len(&self) -> usize {
        self.dict.len()
    }

    /// `true` iff the embedded FST has zero entries.
    #[wasm_bindgen(getter, js_name = isEmpty)]
    pub fn is_empty(&self) -> bool {
        self.dict.is_empty()
    }

    /// Words exactly matching `pinyin` (lowercase a–z), L0/freq ranked.
    pub fn lookup(&self, pinyin: &str) -> js_sys::Array {
        let arr = js_sys::Array::new();
        for word in self.dict.lookup(pinyin) {
            arr.push(&JsValue::from_str(&word));
        }
        arr
    }

    /// `{pinyin, word}` pairs whose pinyin starts with `prefix`.
    #[wasm_bindgen(js_name = prefix)]
    pub fn prefix_lookup(&self, prefix: &str) -> js_sys::Array {
        let arr = js_sys::Array::new();
        for (pinyin, word) in self.dict.prefix(prefix) {
            let obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&obj, &"pinyin".into(), &JsValue::from_str(&pinyin));
            let _ = js_sys::Reflect::set(&obj, &"word".into(), &JsValue::from_str(&word));
            arr.push(&obj);
        }
        arr
    }

    /// Pinyin readings for a single Han character (`""` if unknown).
    pub fn encode(&self, ch: &str) -> js_sys::Array {
        let arr = js_sys::Array::new();
        let Some(c) = ch.chars().next() else {
            return arr;
        };
        for r in char_to_pinyin(c) {
            arr.push(&JsValue::from_str(&r));
        }
        arr
    }

    // -------------------------------------------------------------------
    // L0 mutation — host calls these to drive learning. All counter logic
    // lives inside `inputx-pinyin`; the host only signals events.
    // -------------------------------------------------------------------

    /// Tell the dictionary that the user just committed `word` for
    /// `pinyin`. Returns `true` if this call caused an auto-promotion to
    /// L0.
    #[wasm_bindgen(js_name = recordPick)]
    pub fn record_pick(&self, pinyin: &str, word: &str) -> bool {
        self.dict.record_pick(pinyin, word)
    }

    /// Force-pin a word as L0 default for `pinyin` without going through
    /// the pick counter. Returns `true` if `(pinyin, word)` exists in L1.
    pub fn pin(&self, pinyin: &str, word: &str) -> bool {
        self.dict.pin(pinyin, word)
    }

    /// Drop the L0 pin AND any pending pick counters for `pinyin`.
    pub fn forget(&self, pinyin: &str) -> bool {
        self.dict.forget(pinyin)
    }

    /// Number of L0 pinned pinyins.
    #[wasm_bindgen(js_name = l0PinCount, getter)]
    pub fn l0_pin_count(&self) -> usize {
        self.dict.l0_pin_count()
    }

    /// Number of distinct (pinyin, word) pairs with pending pick counters.
    #[wasm_bindgen(js_name = l0PendingCount, getter)]
    pub fn l0_pending_count(&self) -> usize {
        self.dict.l0_pending_count()
    }

    /// Snapshot the L0 state as a JS object:
    /// ```js
    /// { pins: [[pinyin, word], ...],
    ///   pickCounts: [[pinyin, word, n], ...] }
    /// ```
    /// Caller should `JSON.stringify` this for persistence.
    #[wasm_bindgen(js_name = exportL0)]
    pub fn export_l0(&self) -> JsValue {
        let snap = self.dict.export_l0();
        let obj = js_sys::Object::new();

        let pins = js_sys::Array::new();
        for (p, w) in &snap.pins {
            let pair = js_sys::Array::new();
            pair.push(&JsValue::from_str(p));
            pair.push(&JsValue::from_str(w));
            pins.push(&pair);
        }
        let _ = js_sys::Reflect::set(&obj, &"pins".into(), &pins);

        let counts = js_sys::Array::new();
        for (p, w, n) in &snap.pick_counts {
            let triple = js_sys::Array::new();
            triple.push(&JsValue::from_str(p));
            triple.push(&JsValue::from_str(w));
            triple.push(&JsValue::from_f64(*n as f64));
            counts.push(&triple);
        }
        let _ = js_sys::Reflect::set(&obj, &"pickCounts".into(), &counts);

        obj.into()
    }

    /// Replace the L0 state from a JS object with the same shape produced
    /// by `exportL0`. Entries whose `(pinyin, word)` no longer exist in
    /// L1 are silently dropped. Returns the count of accepted pins.
    #[wasm_bindgen(js_name = importL0)]
    pub fn import_l0(&self, state: JsValue) -> usize {
        let Some(obj) = state.dyn_ref::<js_sys::Object>() else {
            return 0;
        };
        let snap = parse_snapshot(obj);
        self.dict.import_l0(snap)
    }
}

impl Default for PinyinEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_snapshot(obj: &js_sys::Object) -> L0Snapshot {
    let pins = js_sys::Reflect::get(obj, &"pins".into())
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Array>().ok())
        .map(|arr| {
            arr.iter()
                .filter_map(|pair| {
                    let pair: js_sys::Array = pair.dyn_into().ok()?;
                    let p = pair.get(0).as_string()?;
                    let w = pair.get(1).as_string()?;
                    Some((p, w))
                })
                .collect()
        })
        .unwrap_or_default();

    let pick_counts = js_sys::Reflect::get(obj, &"pickCounts".into())
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Array>().ok())
        .map(|arr| {
            arr.iter()
                .filter_map(|triple| {
                    let triple: js_sys::Array = triple.dyn_into().ok()?;
                    let p = triple.get(0).as_string()?;
                    let w = triple.get(1).as_string()?;
                    let n = triple.get(2).as_f64()? as u32;
                    Some((p, w, n))
                })
                .collect()
        })
        .unwrap_or_default();

    L0Snapshot {
        pins,
        pick_counts,
        ..Default::default()
    }
}
