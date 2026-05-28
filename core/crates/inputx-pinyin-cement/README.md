# inputx-pinyin-cement

> **⚠ DEPRECATED at 1.4.1 (2026-05).** The v1.5 cycle's D11
> taxonomy correction reclassified "cement" as **application source
> code, not a published crate**. This crate actually ships pure
> data + stateless helpers (no application glue, no per-session
> state) — by the new taxonomy that's stones, just mis-named.
>
> **Migration path (v1.6 backlog)**:
>
> - `EMBEDDED_PINYIN_IDF` + `EMBEDDED_BIGRAMS_NGM` → future
>   `inputx-pinyin-data-words` + reuse of existing
>   `inputx-pinyin-data-bigrams` data stones.
> - `bigram_boost_from_ngm` / `legacy_bigram_boost_from_ngm` /
>   `estimated_freq_from_log_prior` / `pinyin_idf_reader` → future
>   `inputx-pinyin-helpers` stone.
>
> v1.4.1 is just a deprecation marker on the existing 1.4.0 code —
> no API or content change; safe to upgrade.

Pinyin-specific data + lookup helpers for the [Inputx](https://github.com/goliajp/inputx)
IME.

```toml
[dependencies]
inputx-pinyin-cement = "1.4"
```

Sits on top of the [`inputx-pinyin`](https://crates.io/crates/inputx-pinyin)
facade plus [`inputx-dict-format`](https://crates.io/crates/inputx-dict-format)
(IDFv1 dict reader) and [`inputx-ngram`](https://crates.io/crates/inputx-ngram)
(NGMv1 bigram). Use this if you want a ready-to-drive Pinyin IME
engine backed by the IDFv1 / NGMv1 file formats; use the facade alone
if you want only the lower-level lookup primitives + your own runtime
glue.

## What's in the box

- **`EMBEDDED_PINYIN_IDF`** (v1.4.6 sub-phase C) — process-embedded
  IDFv1 pinyin dict blob, 237,354 entries with FST code index for
  O(|code|) lookups. v1.4.7 sub-phase A5: prior-correction Q4
  boosts are baked into `log_prior_q4` at snapshot build time, so
  cement-side reads need no separate correction lambda.
- **`pinyin_idf_reader()`** — process-global `IdfReader` `OnceLock`;
  9 MB parse + sha256 verify amortizes once across the process,
  subsequent `lookup(code)` calls are O(|code|) FST walks with zero
  per-query allocation.
- **`EMBEDDED_BIGRAMS_NGM`** (v1.4.6 sub-phase C2) — embedded
  NGMv1 bigram blob (~600 KB, ~64k triplets). Same Q4 log-prob
  data source as the facade's bundled `bigrams.fsa`, just packaged
  for `inputx_ngram::NgramTable::from_bytes` consumption.
- **`bigram_boost_from_ngm(table, prev, next) -> i16`** — Q4 log-
  space bigram bonus, NGMv1-backed. Add directly to a candidate's
  `log_prior_q4`.
- **`legacy_bigram_boost_from_ngm(table, prev, next) -> f64`** —
  v1.3-calibration bridge (BIGRAM_BOOST_MAX × ln(1+count) /
  ln(1+REF), capped at 50k); used during the legacy-f64 sort
  transition window where both the Q4 and f64 sort keys had to
  carry consistent bigram signal.
- **`estimated_freq_from_log_prior(i16) -> u64`** — inverse of
  `inputx_scoring::log_prior_from_freq`, recovers an estimated raw
  freq from the Q4 log_prior. Round-trip drift ≤ 1% at typical
  bigram counts. Largely superseded by the v1.4.7 A4 step 1
  schema bump (entries now carry `raw_freq: u32` losslessly) but
  kept exported for callers reading legacy IDFv1.4.6 blobs.

## Quick start

```rust
use inputx_pinyin_cement::{pinyin_idf_reader, bigram_boost_from_ngm, EMBEDDED_BIGRAMS_NGM};
use inputx_ngram::NgramTable;

// Read pinyin entries — exact match, FST-indexed.
let reader = pinyin_idf_reader();
for entry in reader.lookup(b"jixu") {
    println!(
        "{} log_prior_q4={} raw_freq={}",
        entry.word, entry.log_prior, entry.raw_freq,
    );
}
// 继续 log_prior_q4=N (baked +17 correction) raw_freq=74652

// Streaming prefix scan — cement business rules apply per entry.
reader.prefix_for_each_entry(b"zhong", |e| {
    // … rank, filter, build top-k
});

// Bigram boost in Q4 log-space; add to log_prior_q4 of the next word.
let ngm = NgramTable::from_bytes(EMBEDDED_BIGRAMS_NGM)
    .expect("EMBEDDED_BIGRAMS_NGM is a valid NGMv1 blob");
let boost: i16 = bigram_boost_from_ngm(&ngm, Some("今天"), "是");
```

## Architecture note

The stateful `PinyinAdapter` (state machine driven by composite
engine keystrokes — buffer, fuzzy variants, prefix prediction, L0
pin) lives in `inputx-core/composite/`. It shares cross-engine
modules (rules engine, mode dispatcher, scoring constants) with
wubi / nihongo paths, so by [`PLAN-stones-extract.md` "Cement
catalog"](https://github.com/goliajp/inputx/blob/develop/.claude/PLAN-stones-extract.md)
terminology that's **composite root cement** (lives in `inputx-core`,
not here). `inputx-pinyin-cement` only contains genuinely pinyin-
specific helpers — anything a third-party Pinyin IME consumer would
copy verbatim if they re-implemented the wiring.

## API stability

The 1.x line follows semver:

- **`pinyin_idf_reader()` / `bigram_boost_from_ngm` /
  `legacy_bigram_boost_from_ngm` / `estimated_freq_from_log_prior`
  signatures** — stable across 1.x.
- **`EMBEDDED_PINYIN_IDF` / `EMBEDDED_BIGRAMS_NGM` blob versions**
  — IDFv1.4.7 + NGMv1 on the current 1.4 line. Reader stays
  backward-compatible across IDFv1 / NGMv1 sub-versions; the
  embedded blob rebuilds with each release as the underlying
  facade's `PinyinDict` corpus updates.
- **L0 JSON helpers** — JSON shape stable, with the Inputx "old
  files load as empty" hard-reset semantics retained.

## License

Dual-licensed under MIT OR Apache-2.0.
