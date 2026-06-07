//! JSON serialization for L0 snapshots — exposed via FFI as opaque blobs
//! the iOS host writes to App Group `Library/Application Support/inputx/
//! {wubi,pinyin}_l0.json`.
//!
//! Self-contained, **zero external deps**: a hand-rolled compact JSON
//! writer + a small panic-free recursive-descent parser live below. This
//! replaces the former serde/serde_json dependency (Stage A of the
//! zero-dep engine milestone, see `.claude/PLAN-self-built-fsa.md`). The
//! inputx-wubi + inputx-pinyin crates already carry no serde; inputx-core now
//! matches them.
//!
//! f64 fidelity note: the writer uses std's `{}` Display for floats, which
//! emits the *shortest string that round-trips exactly* (Grisu + fallback).
//! That is strictly better than serde_json's default non-ryu writer, which
//! used to shed ~1 ULP on `layer_prefs` per round-trip. Drift is now 0.
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

/// Schema version. Bumped whenever the JSON shape changes; importers
/// silently drop unrecognized versions (returns 0 accepted pins).
const SCHEMA_VERSION: u32 = 1;

// ─── Writer ────────────────────────────────────────────────────────────

/// Append `s` as a JSON string literal (RFC 8259 escaping). CJK passes
/// through as raw UTF-8 (valid in JSON); only `"`, `\`, and control chars
/// are escaped.
fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str("\\u");
                for shift in [12, 8, 4, 0] {
                    let nib = ((c as u32) >> shift) & 0xF;
                    out.push(char::from_digit(nib, 16).unwrap_or('0'));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Format an f64 as a JSON number with shortest exact round-trip. Non-finite
/// values (never produced by the engine for layer_prefs) degrade to `0`.
fn push_json_f64(out: &mut String, v: f64) {
    if v.is_finite() {
        // std `{}` is shortest-round-trip; integers render without a `.0`
        // (e.g. `1`, `-100`) which is valid JSON and parses back exactly.
        out.push_str(&v.to_string());
    } else {
        out.push('0');
    }
}

fn push_pairs(out: &mut String, pairs: &[(String, String)]) {
    out.push('[');
    for (i, (a, b)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        push_json_str(out, a);
        out.push(',');
        push_json_str(out, b);
        out.push(']');
    }
    out.push(']');
}

fn push_triples(out: &mut String, triples: &[(String, String, u32)]) {
    out.push('[');
    for (i, (a, b, n)) in triples.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        push_json_str(out, a);
        out.push(',');
        push_json_str(out, b);
        out.push(',');
        out.push_str(&n.to_string());
        out.push(']');
    }
    out.push(']');
}

pub fn wubi_to_json(snap: &inputx_wubi::L0Snapshot) -> String {
    let mut s = String::with_capacity(64 + snap.pins.len() * 16 + snap.pick_counts.len() * 18);
    s.push_str("{\"version\":");
    s.push_str(&SCHEMA_VERSION.to_string());
    s.push_str(",\"engine\":\"wubi\",\"pins\":");
    push_pairs(&mut s, &snap.pins);
    s.push_str(",\"pick_counts\":");
    push_triples(&mut s, &snap.pick_counts);
    s.push_str(",\"layer_prefs\":[");
    for (i, v) in snap.layer_prefs.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        push_json_f64(&mut s, *v);
    }
    s.push_str("]}");
    s
}

pub fn wubi_from_json(json: &str) -> Option<inputx_wubi::L0Snapshot> {
    let v = mini_json::parse(json)?;
    if v.get("version")?.as_u32()? != SCHEMA_VERSION {
        return None;
    }
    if v.get("engine")?.as_str()? != "wubi" {
        return None;
    }
    let pins = parse_pairs(v.get("pins")?)?;
    let pick_counts = parse_triples(v.get("pick_counts")?)?;
    let mut layer_prefs = inputx_wubi::DEFAULT_LAYER_PREFS;
    if let Some(lp) = v.get("layer_prefs").and_then(mini_json::Json::as_arr) {
        for (i, item) in lp.iter().enumerate().take(layer_prefs.len()) {
            if let Some(f) = item.as_f64() {
                layer_prefs[i] = f;
            }
        }
    }
    Some(inputx_wubi::L0Snapshot {
        pins,
        pick_counts,
        layer_prefs,
    })
}

pub fn pinyin_to_json(snap: &inputx_pinyin::L0Snapshot) -> String {
    let mut s = String::with_capacity(48 + snap.pins.len() * 16 + snap.pick_counts.len() * 18);
    s.push_str("{\"version\":");
    s.push_str(&SCHEMA_VERSION.to_string());
    s.push_str(",\"engine\":\"pinyin\",\"pins\":");
    push_pairs(&mut s, &snap.pins);
    s.push_str(",\"pick_counts\":");
    push_triples(&mut s, &snap.pick_counts);
    s.push('}');
    s
}

pub fn pinyin_from_json(json: &str) -> Option<inputx_pinyin::L0Snapshot> {
    let v = mini_json::parse(json)?;
    if v.get("version")?.as_u32()? != SCHEMA_VERSION {
        return None;
    }
    if v.get("engine")?.as_str()? != "pinyin" {
        return None;
    }
    let pins = parse_pairs(v.get("pins")?)?;
    let pick_counts = parse_triples(v.get("pick_counts")?)?;
    Some(inputx_pinyin::L0Snapshot { pins, pick_counts })
}

fn parse_pairs(v: &mini_json::Json) -> Option<Vec<(String, String)>> {
    let arr = v.as_arr()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let t = item.as_arr()?;
        let a = t.first()?.as_str()?.to_string();
        let b = t.get(1)?.as_str()?.to_string();
        out.push((a, b));
    }
    Some(out)
}

fn parse_triples(v: &mini_json::Json) -> Option<Vec<(String, String, u32)>> {
    let arr = v.as_arr()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let t = item.as_arr()?;
        let a = t.first()?.as_str()?.to_string();
        let b = t.get(1)?.as_str()?.to_string();
        let n = t.get(2)?.as_u32()?;
        out.push((a, b, n));
    }
    Some(out)
}

// ─── Minimal panic-free JSON parser ──────────────────────────────────────
//
// Recursive-descent over the input bytes. Every advance goes through
// `get(..)?` so out-of-bounds returns `None` instead of panicking — the FFI
// import path is a trust boundary (user-controlled L0 files), so a panic
// would be a DoS vector. A depth cap bounds recursion on adversarial deeply
// nested input. Only the subset of JSON the schema needs is consumed, but
// the grammar is complete (objects, arrays, strings w/ escapes + surrogate
// pairs, numbers, true/false/null).
mod mini_json {
    /// Max nesting depth. Our schema nests 3 deep (obj → array → array);
    /// 32 is generous headroom while bounding stack use on hostile input.
    const MAX_DEPTH: usize = 32;

    // The Bool/Num/... fields model the complete JSON grammar even though the
    // schema consumer only reads a subset — keep the full value model.
    #[allow(dead_code)]
    #[derive(Debug)]
    pub enum Json {
        Null,
        Bool(bool),
        Num(f64),
        Str(String),
        Arr(Vec<Json>),
        Obj(Vec<(String, Json)>),
    }

    impl Json {
        pub fn get(&self, key: &str) -> Option<&Json> {
            match self {
                Json::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                _ => None,
            }
        }
        pub fn as_str(&self) -> Option<&str> {
            match self {
                Json::Str(s) => Some(s),
                _ => None,
            }
        }
        pub fn as_arr(&self) -> Option<&[Json]> {
            match self {
                Json::Arr(a) => Some(a),
                _ => None,
            }
        }
        pub fn as_f64(&self) -> Option<f64> {
            match self {
                Json::Num(n) => Some(*n),
                _ => None,
            }
        }
        /// Integer-valued number → u32. Our integers (version, counts) are
        /// well within f64-exact range, so the cast is lossless.
        pub fn as_u32(&self) -> Option<u32> {
            match self {
                Json::Num(n) if n.is_finite() && *n >= 0.0 => Some(*n as u32),
                _ => None,
            }
        }
    }

    pub fn parse(s: &str) -> Option<Json> {
        let mut p = Parser {
            b: s.as_bytes(),
            i: 0,
        };
        p.skip_ws();
        let v = p.value(0)?;
        p.skip_ws();
        if p.i == p.b.len() { Some(v) } else { None }
    }

    struct Parser<'a> {
        b: &'a [u8],
        i: usize,
    }

    impl Parser<'_> {
        fn peek(&self) -> Option<u8> {
            self.b.get(self.i).copied()
        }

        fn skip_ws(&mut self) {
            while let Some(c) = self.peek() {
                if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
                    self.i += 1;
                } else {
                    break;
                }
            }
        }

        fn value(&mut self, depth: usize) -> Option<Json> {
            if depth > MAX_DEPTH {
                return None;
            }
            self.skip_ws();
            match self.peek()? {
                b'{' => self.object(depth),
                b'[' => self.array(depth),
                b'"' => self.string().map(Json::Str),
                b't' => self.lit(b"true", Json::Bool(true)),
                b'f' => self.lit(b"false", Json::Bool(false)),
                b'n' => self.lit(b"null", Json::Null),
                b'-' | b'0'..=b'9' => self.number(),
                _ => None,
            }
        }

        fn lit(&mut self, kw: &[u8], v: Json) -> Option<Json> {
            if self.b.get(self.i..self.i + kw.len()) == Some(kw) {
                self.i += kw.len();
                Some(v)
            } else {
                None
            }
        }

        fn number(&mut self) -> Option<Json> {
            let start = self.i;
            if self.peek() == Some(b'-') {
                self.i += 1;
            }
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-') {
                    self.i += 1;
                } else {
                    break;
                }
            }
            let tok = core::str::from_utf8(self.b.get(start..self.i)?).ok()?;
            tok.parse::<f64>().ok().map(Json::Num)
        }

        fn array(&mut self, depth: usize) -> Option<Json> {
            self.i += 1; // consume '['
            let mut out = Vec::new();
            self.skip_ws();
            if self.peek() == Some(b']') {
                self.i += 1;
                return Some(Json::Arr(out));
            }
            loop {
                out.push(self.value(depth + 1)?);
                self.skip_ws();
                match self.peek()? {
                    b',' => self.i += 1,
                    b']' => {
                        self.i += 1;
                        return Some(Json::Arr(out));
                    }
                    _ => return None,
                }
            }
        }

        fn object(&mut self, depth: usize) -> Option<Json> {
            self.i += 1; // consume '{'
            let mut out = Vec::new();
            self.skip_ws();
            if self.peek() == Some(b'}') {
                self.i += 1;
                return Some(Json::Obj(out));
            }
            loop {
                self.skip_ws();
                if self.peek()? != b'"' {
                    return None;
                }
                let key = self.string()?;
                self.skip_ws();
                if self.peek()? != b':' {
                    return None;
                }
                self.i += 1;
                let val = self.value(depth + 1)?;
                out.push((key, val));
                self.skip_ws();
                match self.peek()? {
                    b',' => self.i += 1,
                    b'}' => {
                        self.i += 1;
                        return Some(Json::Obj(out));
                    }
                    _ => return None,
                }
            }
        }

        /// Parse a JSON string starting at the opening quote. Collects raw
        /// bytes (input is valid UTF-8) + decoded escapes, then validates as
        /// UTF-8. Returns `None` on any malformed escape or unterminated
        /// string — never panics.
        fn string(&mut self) -> Option<String> {
            self.i += 1; // consume opening quote
            let mut out: Vec<u8> = Vec::new();
            loop {
                let c = *self.b.get(self.i)?;
                self.i += 1;
                match c {
                    b'"' => return String::from_utf8(out).ok(),
                    b'\\' => {
                        let e = *self.b.get(self.i)?;
                        self.i += 1;
                        match e {
                            b'"' => out.push(b'"'),
                            b'\\' => out.push(b'\\'),
                            b'/' => out.push(b'/'),
                            b'n' => out.push(b'\n'),
                            b't' => out.push(b'\t'),
                            b'r' => out.push(b'\r'),
                            b'b' => out.push(0x08),
                            b'f' => out.push(0x0c),
                            b'u' => {
                                let cp = self.hex4()?;
                                let ch = if (0xD800..=0xDBFF).contains(&cp) {
                                    // high surrogate → expect a `\uXXXX` low surrogate
                                    if *self.b.get(self.i)? != b'\\' {
                                        return None;
                                    }
                                    self.i += 1;
                                    if *self.b.get(self.i)? != b'u' {
                                        return None;
                                    }
                                    self.i += 1;
                                    let lo = self.hex4()?;
                                    if !(0xDC00..=0xDFFF).contains(&lo) {
                                        return None;
                                    }
                                    let c = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                    char::from_u32(c)?
                                } else if (0xDC00..=0xDFFF).contains(&cp) {
                                    return None; // lone low surrogate
                                } else {
                                    char::from_u32(cp)?
                                };
                                let mut buf = [0u8; 4];
                                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                            }
                            _ => return None,
                        }
                    }
                    other => out.push(other),
                }
            }
        }

        fn hex4(&mut self) -> Option<u32> {
            let mut v = 0u32;
            for _ in 0..4 {
                let c = *self.b.get(self.i)?;
                self.i += 1;
                let d = match c {
                    b'0'..=b'9' => (c - b'0') as u32,
                    b'a'..=b'f' => (c - b'a' + 10) as u32,
                    b'A'..=b'F' => (c - b'A' + 10) as u32,
                    _ => return None,
                };
                v = v * 16 + d;
            }
            Some(v)
        }
    }

    #[cfg(test)]
    mod parser_tests {
        use super::*;

        #[test]
        fn parses_nested_and_escapes() {
            let v = parse(r#"{"a":[["x","中\n国"],[1,2.5,-3]],"b":true,"c":null}"#).unwrap();
            assert_eq!(v.get("a").unwrap().as_arr().unwrap().len(), 2);
            let first = &v.get("a").unwrap().as_arr().unwrap()[0];
            assert_eq!(first.as_arr().unwrap()[1].as_str(), Some("中\n国"));
        }

        #[test]
        fn surrogate_pair() {
            // U+1F600 😀 as a surrogate pair.
            let v = parse(r#""😀""#).unwrap();
            assert_eq!(v.as_str(), Some("😀"));
        }

        #[test]
        fn rejects_trailing_garbage_and_unterminated() {
            assert!(parse(r#"{}x"#).is_none());
            assert!(parse(r#"{"a":"#).is_none());
            assert!(parse(r#""unterminated"#).is_none());
            assert!(parse(r#"[1,2,]"#).is_none());
        }

        #[test]
        fn deep_nesting_is_bounded_not_panicking() {
            let deep = "[".repeat(1000);
            assert!(parse(&deep).is_none()); // depth cap → None, no stack overflow
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wubi_round_trip_empty() {
        let snap = inputx_wubi::L0Snapshot {
            pins: vec![],
            pick_counts: vec![],
            layer_prefs: inputx_wubi::DEFAULT_LAYER_PREFS,
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
        let snap = inputx_wubi::L0Snapshot {
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
        let snap = inputx_pinyin::L0Snapshot {
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
        let snap = inputx_pinyin::L0Snapshot {
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

    #[test]
    fn whitespace_tolerant() {
        // A pretty-printed variant must still parse (robustness vs. any
        // host that re-formats the blob before writing it back).
        let json = "{ \"version\" : 1 , \"engine\" : \"pinyin\" ,\n  \"pins\" : [ ] ,\n  \"pick_counts\" : [ [ \"women\" , \"我们\" , 3 ] ] }";
        let back = pinyin_from_json(json).unwrap();
        assert_eq!(back.pick_counts, vec![("women".into(), "我们".into(), 3)]);
    }

    // -----------------------------------------------------------------
    // B. L0 import/export fuzz — random + adversarial JSON inputs.
    // Contract: `*_from_json` never panics; always returns Option (None
    // on any parse / version / engine error). Round-trip equality for
    // valid snapshots is also asserted.
    // -----------------------------------------------------------------

    use proptest::prelude::*;

    /// Shrinkable strategy producing valid wubi L0Snapshots.
    fn wubi_snap_strategy() -> impl Strategy<Value = inputx_wubi::L0Snapshot> {
        let pins = proptest::collection::vec(
            (
                proptest::collection::vec(b'a'..=b'z', 1..6)
                    .prop_map(|v| String::from_utf8(v).unwrap()),
                "[\u{4e00}-\u{9fff}]{1,4}"
                    .prop_filter("non-empty word", |s: &String| !s.is_empty()),
            ),
            0..16,
        );
        let pick_counts = proptest::collection::vec(
            (
                proptest::collection::vec(b'a'..=b'z', 1..6)
                    .prop_map(|v| String::from_utf8(v).unwrap()),
                "[\u{4e00}-\u{9fff}]{1,4}"
                    .prop_filter("non-empty word", |s: &String| !s.is_empty()),
                0u32..1000,
            ),
            0..16,
        );
        let layer_prefs = (
            -100.0f64..100.0, // intentionally include negatives + extremes
            0.0f64..10.0,
            0.0f64..10.0,
            0.0f64..10.0,
            0.0f64..10.0,
            0.0f64..10.0,
        );
        (pins, pick_counts, layer_prefs).prop_map(|(p, pc, (l0, l1, l2, l3, l4, l5))| {
            inputx_wubi::L0Snapshot {
                pins: p,
                pick_counts: pc,
                layer_prefs: [l0, l1, l2, l3, l4, l5],
            }
        })
    }

    fn pinyin_snap_strategy() -> impl Strategy<Value = inputx_pinyin::L0Snapshot> {
        let pins = proptest::collection::vec(
            (
                proptest::collection::vec(b'a'..=b'z', 1..12)
                    .prop_map(|v| String::from_utf8(v).unwrap()),
                "[\u{4e00}-\u{9fff}]{1,4}"
                    .prop_filter("non-empty word", |s: &String| !s.is_empty()),
            ),
            0..16,
        );
        let pick_counts = proptest::collection::vec(
            (
                proptest::collection::vec(b'a'..=b'z', 1..12)
                    .prop_map(|v| String::from_utf8(v).unwrap()),
                "[\u{4e00}-\u{9fff}]{1,4}"
                    .prop_filter("non-empty word", |s: &String| !s.is_empty()),
                0u32..1000,
            ),
            0..16,
        );
        (pins, pick_counts).prop_map(|(p, pc)| inputx_pinyin::L0Snapshot {
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

        /// Round-trip identity for valid wubi snapshots. The hand-rolled
        /// writer uses std `{}` (shortest exact round-trip) for f64, so
        /// `layer_prefs` round-trips with ZERO ULP drift — strictly better
        /// than the former serde_json non-ryu writer (which shed ~1 ULP).
        /// Tolerance kept at 4 ULP as a safety margin.
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

        /// Codec idempotence — re-emitting a parsed snapshot is byte-stable.
        /// With shortest-round-trip f64 the FIRST emit is already stable.
        #[test]
        fn wubi_roundtrip_idempotent(snap in wubi_snap_strategy()) {
            let json1 = wubi_to_json(&snap);
            let parsed1 = wubi_from_json(&json1).expect("round-trip parse");
            let json2 = wubi_to_json(&parsed1);
            prop_assert_eq!(json1, json2, "codec not idempotent");
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
