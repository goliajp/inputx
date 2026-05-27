//! WASM bindings for `wubi`. Exposes a small, JS-friendly API that
//! browser and Node consumers can import via `wasm-pack` output.
//!
//! Build:
//!     wasm-pack build crates/wubi-wasm --target web --release
//!
//! Usage (JS):
//!     import init, { WubiEngine, Layer } from "@goliapkg/wubi";
//!     await init();
//!     const eng = new WubiEngine();
//!     const candidates = eng.lookup("khlg");      // ["中国", "跑车", ...]
//!     eng.recordPick("khlg", "跑车");              // user picked candidate
//!     eng.setLayerPref(Layer.Phrase, 1.5);         // boost phrase layer
//!     const state = eng.exportL0();                // structured object
//!     localStorage.setItem("wubi-l0", JSON.stringify(state));
//!     // later:
//!     eng.importL0(JSON.parse(localStorage.getItem("wubi-l0")));

use wasm_bindgen::prelude::*;

use inputx_wubi::{L0Snapshot, LAYER_COUNT, Layer as CoreLayer, WubiDict};

/// Layer enum mirrored for JS. Use as `Layer.Phrase` etc.
#[wasm_bindgen]
pub enum Layer {
    Auto = 0,
    Phrase = 1,
    Zigen = 2,
    Jianma3 = 3,
    Jianma2 = 4,
    Jianma1 = 5,
}

impl From<Layer> for CoreLayer {
    fn from(l: Layer) -> Self {
        match l {
            Layer::Auto => CoreLayer::Auto,
            Layer::Phrase => CoreLayer::Phrase,
            Layer::Zigen => CoreLayer::Zigen,
            Layer::Jianma3 => CoreLayer::Jianma3,
            Layer::Jianma2 => CoreLayer::Jianma2,
            Layer::Jianma1 => CoreLayer::Jianma1,
        }
    }
}

/// Wubi 86 engine wrapping the embedded FST dictionary plus a per-instance
/// L0 layer (in-memory; persistence is up to the host via `exportL0` /
/// `importL0`).
#[wasm_bindgen]
pub struct WubiEngine {
    dict: WubiDict,
}

#[wasm_bindgen]
impl WubiEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WubiEngine {
        Self {
            dict: WubiDict::embedded(),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn len(&self) -> usize {
        self.dict.len()
    }

    #[wasm_bindgen(getter, js_name = isEmpty)]
    pub fn is_empty(&self) -> bool {
        self.dict.is_empty()
    }

    /// Words exactly matching `code` (lowercase a–z), L0/L1 ranked.
    pub fn lookup(&self, code: &str) -> js_sys::Array {
        let arr = js_sys::Array::new();
        for word in self.dict.lookup(code) {
            arr.push(&JsValue::from_str(&word));
        }
        arr
    }

    /// `{code, word}` pairs whose code starts with `prefix`, weight desc.
    #[wasm_bindgen(js_name = prefix)]
    pub fn prefix_lookup(&self, prefix: &str) -> js_sys::Array {
        let arr = js_sys::Array::new();
        for (code, word) in self.dict.prefix(prefix) {
            let obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&obj, &"code".into(), &JsValue::from_str(&code));
            let _ = js_sys::Reflect::set(&obj, &"word".into(), &JsValue::from_str(&word));
            arr.push(&obj);
        }
        arr
    }

    // -------------------------------------------------------------------
    // L0 mutation — host calls these to drive learning. All counter logic
    // lives inside `wubi`; the host only signals events.
    // -------------------------------------------------------------------

    /// Tell the dictionary that the user just committed `word` for `code`.
    /// Returns `true` if this call caused an auto-promotion to L0.
    #[wasm_bindgen(js_name = recordPick)]
    pub fn record_pick(&self, code: &str, word: &str) -> bool {
        self.dict.record_pick(code, word)
    }

    /// Force-pin a word as L0 default for `code` without going through the
    /// pick counter. Returns `true` if `(code, word)` exists in L1.
    pub fn pin(&self, code: &str, word: &str) -> bool {
        self.dict.pin(code, word)
    }

    /// Drop the L0 pin AND any pending pick counters for `code`.
    pub fn forget(&self, code: &str) -> bool {
        self.dict.forget(code)
    }

    /// Set the multiplier for a layer. Negative / NaN values are clamped to 0.
    #[wasm_bindgen(js_name = setLayerPref)]
    pub fn set_layer_pref(&self, layer: Layer, multiplier: f64) {
        self.dict.set_layer_pref(layer.into(), multiplier);
    }

    #[wasm_bindgen(js_name = layerPref)]
    pub fn layer_pref(&self, layer: Layer) -> f64 {
        self.dict.layer_pref(layer.into())
    }

    #[wasm_bindgen(js_name = l0PinCount, getter)]
    pub fn l0_pin_count(&self) -> usize {
        self.dict.l0_pin_count()
    }

    #[wasm_bindgen(js_name = l0PendingCount, getter)]
    pub fn l0_pending_count(&self) -> usize {
        self.dict.l0_pending_count()
    }

    /// Snapshot the L0 state as a JS object:
    /// ```js
    /// { pins: [[code, word], ...],
    ///   pickCounts: [[code, word, n], ...],
    ///   layerPrefs: [Auto, Phrase, Zigen, Jianma3, Jianma2, Jianma1] }
    /// ```
    /// Caller should `JSON.stringify` this for persistence.
    #[wasm_bindgen(js_name = exportL0)]
    pub fn export_l0(&self) -> JsValue {
        let snap = self.dict.export_l0();
        let obj = js_sys::Object::new();

        let pins = js_sys::Array::new();
        for (c, w) in &snap.pins {
            let pair = js_sys::Array::new();
            pair.push(&JsValue::from_str(c));
            pair.push(&JsValue::from_str(w));
            pins.push(&pair);
        }
        let _ = js_sys::Reflect::set(&obj, &"pins".into(), &pins);

        let counts = js_sys::Array::new();
        for (c, w, n) in &snap.pick_counts {
            let triple = js_sys::Array::new();
            triple.push(&JsValue::from_str(c));
            triple.push(&JsValue::from_str(w));
            triple.push(&JsValue::from_f64(*n as f64));
            counts.push(&triple);
        }
        let _ = js_sys::Reflect::set(&obj, &"pickCounts".into(), &counts);

        let prefs = js_sys::Array::new();
        for p in snap.layer_prefs {
            prefs.push(&JsValue::from_f64(p));
        }
        let _ = js_sys::Reflect::set(&obj, &"layerPrefs".into(), &prefs);

        obj.into()
    }

    /// Replace the L0 state from a JS object with the same shape produced by
    /// `exportL0`. Entries whose `(code, word)` no longer exist in L1 are
    /// silently dropped. Returns the count of accepted pins.
    #[wasm_bindgen(js_name = importL0)]
    pub fn import_l0(&self, state: JsValue) -> usize {
        let Some(obj) = state.dyn_ref::<js_sys::Object>() else {
            return 0;
        };
        let snap = parse_snapshot(obj);
        self.dict.import_l0(snap)
    }
}

impl Default for WubiEngine {
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
                    let c = pair.get(0).as_string()?;
                    let w = pair.get(1).as_string()?;
                    Some((c, w))
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
                    let c = triple.get(0).as_string()?;
                    let w = triple.get(1).as_string()?;
                    let n = triple.get(2).as_f64()? as u32;
                    Some((c, w, n))
                })
                .collect()
        })
        .unwrap_or_default();

    let mut layer_prefs = inputx_wubi::DEFAULT_LAYER_PREFS;
    if let Ok(arr) = js_sys::Reflect::get(obj, &"layerPrefs".into())
        && let Ok(arr) = arr.dyn_into::<js_sys::Array>()
    {
        let len = LAYER_COUNT.min(arr.length() as usize);
        for (i, slot) in layer_prefs.iter_mut().take(len).enumerate() {
            if let Some(v) = arr.get(i as u32).as_f64() {
                *slot = v;
            }
        }
    }

    L0Snapshot {
        pins,
        pick_counts,
        layer_prefs,
    }
}
