# inputx-pinyin-cement

Pinyin-specific consumer-engine cement for Inputx.

Sits on top of the [`inputx-pinyin`](https://crates.io/crates/inputx-pinyin)
facade plus [`inputx-dict-format`](https://crates.io/crates/inputx-dict-format)
(IDFv1 dict reader) and [`inputx-ngram`](https://crates.io/crates/inputx-ngram)
(NGMv1 bigram). Use this if you want a ready-to-drive Pinyin IME engine
backed by the IDFv1 / NGMv1 file formats; use the facade alone if you
want only the lower-level lookup primitives + your own runtime glue.

## Public surface

- `bigram_boost_from_ngm(table, prev, next) -> i16` — Q4 log-space
  bigram bonus, NGMv1-backed
- `legacy_bigram_boost_from_ngm(table, prev, next) -> f64` —
  v1.3-calibration bridge (BIGRAM_BOOST_MAX × ln(1+count) /
  ln(1+REF), capped at 50k)
- `PinyinIdfLookup` (v1.4.6 sub-phase C work) — IdfReader-driven
  lookup adapter
- L0 JSON helpers (carved from inputx-core/composite/l0_json.rs)

## Architecture note

The stateful `PinyinAdapter` (state machine driven by composite engine
keystrokes) lives in `inputx-core/composite/` — it depends on
cross-engine modules (rules engine, mode dispatcher, scoring constants)
that span wubi/pinyin/nihongo. By [`PLAN-stones-extract.md`
"Cement catalog"](../../.claude/PLAN-stones-extract.md) terminology, that's
**composite root cement** (lives in inputx-core), not per-language
cement. `inputx-pinyin-cement` only contains genuinely pinyin-specific
helpers — anything a third-party Pinyin IME consumer would copy
verbatim if they re-implemented the wiring.

## License

Dual-licensed under MIT OR Apache-2.0.
