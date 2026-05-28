# inputx-nihongo-cement

> **⚠ DEPRECATED at 1.4.1 (2026-05).** The v1.5 cycle's D11
> taxonomy correction reclassified "cement" as **application source
> code, not a published crate**. This crate ships pure data +
> reader helpers (no application glue, no per-session state) — by
> the new taxonomy that's data stones, mis-named.
>
> **Migration path (v1.6 backlog)**:
>
> - `EMBEDDED_NIHONGO_JUKUGO_IDF` + `nihongo_jukugo_idf_reader` →
>   future `inputx-nihongo-data-jukugo` stone.
> - `EMBEDDED_NIHONGO_KANJI_IDF` + `nihongo_kanji_idf_reader` →
>   future `inputx-nihongo-data-kanji` stone.
> - v1.5.1 WU-κ moved the Japanese state machine + candidate
>   generation OUT of `inputx_nihongo::JapaneseEngine` and INTO the
>   Inputx monorepo's `inputx-core/src/japanese/`. Application
>   consumers needing a stateful JP engine should copy
>   [`inputx-core/src/japanese/`](https://github.com/goliajp/inputx/tree/develop/core/crates/inputx-core/src/japanese)
>   per the cement-as-application-source taxonomy.
>
> v1.4.1 is just a deprecation marker on the existing 1.4.0 code —
> no API or content change; safe to upgrade.

Japanese-specific data + lookup helpers for the [Inputx](https://github.com/goliajp/inputx)
IME.

```toml
[dependencies]
inputx-nihongo-cement = "1.4"
```

## What's in the box

- **`EMBEDDED_NIHONGO_JUKUGO_IDF`** (v1.4.7 sub-phase A4 step 3) —
  process-embedded IDFv1 jukugo dict blob (~1.1 MB, 27,380
  entries). Byte-equivalent to the facade's `JUKUGO_TABLE` const
  table; the `idf-from-nihongo-jukugo` snapshot binary sources
  both from the same data.
- **`EMBEDDED_NIHONGO_KANJI_IDF`** (v1.4.7 sub-phase A4 step 3) —
  process-embedded IDFv1 kanji dict blob (~40 KB, 1,666 `(reading,
  kanji)` pairs — multi-reading expansion of 813 source kanji).
- **`nihongo_jukugo_idf_reader()` / `nihongo_kanji_idf_reader()`**
  — process-global `IdfReader` `OnceLock`s; the 1 MB jukugo parse
  + sha256 verify amortizes once across the process, subsequent
  `lookup(reading)` calls are O(|reading|) FST walks with zero per-
  query allocation.

## Quick start

```rust
use inputx_nihongo_cement::{
    nihongo_jukugo_idf_reader, nihongo_kanji_idf_reader,
};

let jr = nihongo_jukugo_idf_reader();
for entry in jr.lookup(b"shinjuku") {
    println!(
        "{} log_prior_q4={} raw_freq={}",
        entry.word, entry.log_prior, entry.raw_freq,
    );
}
// 新宿 log_prior_q4=N raw_freq=N

// Prefix prediction — streaming visit; cement applies its own
// ranking before truncation.
jr.prefix_for_each_entry(b"shinjuk", |e| {
    // e.word = "新宿", e.code = "shinjuku", e.raw_freq = N
});

let kr = nihongo_kanji_idf_reader();
for entry in kr.lookup(b"nichi") {
    println!("{} log_prior_q4={}", entry.word, entry.log_prior);
}
// 日 log_prior_q4=N (and other nichi-readings)
```

## Architecture note

The stateful `JapaneseAdapter` (composite engine state machine —
buffer, romaji → kana rendering, candidate compose) lives in
`inputx-core/composite/`. It shares cross-engine `mode` + `scoring`
+ `merge` modules with wubi / pinyin paths, so by
[`PLAN-stones-extract.md` "Cement catalog"](https://github.com/goliajp/inputx/blob/develop/.claude/PLAN-stones-extract.md)
terminology that's **composite root cement** (lives in `inputx-core`,
not here).

### v1.4.7 scope note

The Inputx composite hot path currently still calls
`inputx_nihongo::JapaneseEngine::candidates()` — the facade engine's
candidate generation bundles `jukugo::lookup_by_reading` +
`kanji::lookup_by_reading` + `compose_sentence` + kana fallback +
chōonpu plumbing (~500 LOC) with the per-session state machine in
a way pinyin / wubi cement do not. Cutting that to the IDF readers
here is a v1.4.8 facade refactor item — the readers are shipped
now so the refactor can plug straight in without further
infrastructure churn. Runtime behavior is identical regardless,
because the IDF blobs and the facade const tables are byte-
equivalent.

## API stability

The 1.x line follows semver:

- **`nihongo_jukugo_idf_reader()` / `nihongo_kanji_idf_reader()`
  signatures** — stable across 1.x.
- **`EMBEDDED_NIHONGO_JUKUGO_IDF` / `EMBEDDED_NIHONGO_KANJI_IDF`
  blob versions** — IDFv1 on the current 1.4 line. Reader stays
  backward-compatible across IDFv1 sub-versions; the embedded blob
  rebuilds with each release as the underlying `JUKUGO_TABLE` /
  `KANJI_TABLE` content updates.

## License

Dual-licensed under MIT OR Apache-2.0.
