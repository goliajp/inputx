//! WASM bindings for `inputx-nihongo`. Exposes a JS-friendly
//! `JapaneseEngine` so browser / Node consumers can drive the
//! Japanese IME engine via `wasm-pack` output.
//!
//! Build:
//!
//!     wasm-pack build crates/inputx-nihongo-wasm --target web --release
//!
//! Usage (JS):
//!
//!     import init, { JapaneseEngine, KanaKind } from "@goliapkg/nihongo";
//!     await init();
//!     const eng = new JapaneseEngine();
//!     for (const c of "shinjuku") eng.handleLetter(c.charCodeAt(0));
//!     console.log(eng.candidates());
//!     // [
//!     //   { word: "新宿", kind: KanaKind.Kanji, freq: N,
//!     //     composed: false, proximityMilli: 1000 },
//!     //   { word: "しんじゅく", kind: KanaKind.Hiragana, freq: 0, ... },
//!     //   { word: "シンジュク", kind: KanaKind.Katakana, freq: 0, ... },
//!     // ]
//!     const committed = eng.commitIndex(0); // "新宿"

use wasm_bindgen::prelude::*;

use inputx_nihongo::{JapaneseEngine as CoreEngine, KanaKind as CoreKanaKind};

/// Kana kind mirrored for JS. Use as `KanaKind.Kanji` etc. Numeric
/// values match the facade enum's discriminants.
#[wasm_bindgen]
#[derive(Copy, Clone)]
pub enum KanaKind {
    Hiragana = 0,
    Katakana = 1,
    Kanji = 2,
}

impl From<CoreKanaKind> for KanaKind {
    fn from(k: CoreKanaKind) -> Self {
        match k {
            CoreKanaKind::Hiragana => KanaKind::Hiragana,
            CoreKanaKind::Katakana => KanaKind::Katakana,
            CoreKanaKind::Kanji => KanaKind::Kanji,
        }
    }
}

/// JP engine wrapping the embedded romaji / jukugo / kanji tables.
/// Cheap to construct (all data is in const tables).
#[wasm_bindgen]
pub struct JapaneseEngine {
    engine: CoreEngine,
}

#[wasm_bindgen]
impl JapaneseEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> JapaneseEngine {
        Self {
            engine: CoreEngine::new(),
        }
    }

    /// Push one ASCII letter byte (`'a'..='z'`, `'A'..='Z'`) into the
    /// buffer and re-render candidates. The `-` byte is also accepted
    /// (chōonpu `ー`) when the buffer is non-empty.
    ///
    /// Returns `true` if the byte was accepted; rejected bytes leave
    /// state unchanged so the host controller can route the byte
    /// elsewhere (locale punct mapping, raw passthrough).
    #[wasm_bindgen(js_name = handleLetter)]
    pub fn handle_letter(&mut self, byte: u8) -> bool {
        self.engine.handle_letter(byte)
    }

    /// Pop one byte off the buffer. Returns `true` if state changed.
    pub fn backspace(&mut self) -> bool {
        self.engine.backspace()
    }

    /// Drop the buffer entirely. Returns `true` if state changed.
    pub fn escape(&mut self) -> bool {
        self.engine.escape()
    }

    /// Current buffer rendered as a string (for the preedit display).
    pub fn preedit(&self) -> String {
        self.engine.preedit().to_string()
    }

    #[wasm_bindgen(js_name = isComposing, getter)]
    pub fn is_composing(&self) -> bool {
        self.engine.is_composing()
    }

    /// Current candidates as a `js_sys::Array` of `Object`s. Each
    /// object carries:
    ///
    ///   - `word: string`
    ///   - `kind: KanaKind` (enum numeric value)
    ///   - `freq: number` (0–100, higher = more common)
    ///   - `composed: boolean` (`true` for mechanical
    ///     `compose_sentence` products — particle/copula glue)
    ///   - `proximityMilli: number` (1000 = exact match, < 1000 =
    ///     prefix-prediction candidate whose reading the buffer is
    ///     only a prefix of, e.g. `shinjuk → 新宿` at 875)
    pub fn candidates(&self) -> js_sys::Array {
        let arr = js_sys::Array::new();
        for c in self.engine.candidates() {
            let obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&obj, &"word".into(), &JsValue::from_str(&c.word));
            let kind: KanaKind = c.kind.into();
            let _ =
                js_sys::Reflect::set(&obj, &"kind".into(), &JsValue::from_f64(kind as u32 as f64));
            let _ = js_sys::Reflect::set(&obj, &"freq".into(), &JsValue::from_f64(c.freq as f64));
            let _ = js_sys::Reflect::set(&obj, &"composed".into(), &JsValue::from_bool(c.composed));
            let _ = js_sys::Reflect::set(
                &obj,
                &"proximityMilli".into(),
                &JsValue::from_f64(c.proximity_milli as f64),
            );
            arr.push(&obj);
        }
        arr
    }

    /// Commit the candidate at `index`. Returns the committed text and
    /// clears the buffer. Returns `null` (via empty `JsValue`) if
    /// `index` is out of range; state is unchanged on out-of-range.
    #[wasm_bindgen(js_name = commitIndex)]
    pub fn commit_index(&mut self, index: usize) -> Option<String> {
        self.engine.commit_index(index)
    }
}

impl Default for JapaneseEngine {
    fn default() -> Self {
        Self::new()
    }
}
