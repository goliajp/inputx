# inputx-nihongo

Self-contained Japanese input engine for Rust — Hepburn (with kunrei
alternates) romaji → hiragana/katakana, on-yomi kanji lookup, jukugo
(熟語) compound lookup, and `compose_sentence` particle/copula
sentence guesses. Pluggable third-source designed to plug next to
the wubi / pinyin engines in [Inputx](https://github.com/goliajp/inputx),
or to run standalone as a permissively-licensed Japanese IME building
block.

```toml
[dependencies]
inputx-nihongo = "1.4"
```

## What's in the box

- **Romaji table** — Hepburn primary + kunrei alternates + extended
  foreign-syllable subset (`fa`/`fi`/`vu`/`tha`/`kwo`/…) for
  gairaigo. Whole-buffer rendering: every keystroke re-renders the
  kana from scratch, so backspace / mid-buffer edits never leave
  stale state.
- **27,380 jukugo entries** — `JUKUGO_TABLE` covering common
  compounds; `lookup_by_reading` returns `(kanji, freq)` tuples,
  `lookup_by_reading_prefix` powers the prefix-prediction path
  (`shinjuk → 新宿` at proximity 7/8).
- **1,666 kanji readings** (813 source kanji × multi-reading) —
  restricted to kanji whose Unicode codepoint is identical to a
  Simplified-Chinese hanzi, so single-kanji surfaces (`ka → 加, 家,
  …`) never collide with the wubi / pinyin output.
- **`JapaneseEngine`** — stateful per-session driver: `handle_letter`
  / `backspace` / `escape` / `candidates` / `commit_index`, parallel
  in shape to `inputx-wubi::WubiEngine` so the composite layer plugs
  it in next to the existing engines without bespoke wiring.
- **Chōonpu plumbing** — `-` is a typeable JP chōonpu (ー) input
  character when the buffer is non-empty, so long-vowel words like
  コーヒー stay reachable; empty-buffer `-` rejects so a stranded
  hyphen still routes to the host as ASCII punctuation.

## Quick start

```rust
use inputx_nihongo::{JapaneseEngine, KanaKind};

let mut eng = JapaneseEngine::new();
for b in b"shinjuku" {
    eng.handle_letter(*b);
}

// Candidates re-rendered after every keystroke; ordered by
// composite-layer scoring (jukugo > kanji > kana fallback).
for cand in eng.candidates() {
    let tag = match cand.kind {
        KanaKind::Hiragana => "hira",
        KanaKind::Katakana => "kata",
        KanaKind::Kanji => "kanji",
    };
    println!("{tag}: {} (freq={}, composed={})", cand.word, cand.freq, cand.composed);
}
// kanji: 新宿 (freq=N, composed=false)
// hira:  しんじゅく (freq=0, composed=false)
// kata:  シンジュク (freq=0, composed=false)

// Commit one — engine clears its buffer ready for the next word.
let committed = eng.commit_index(0);
assert_eq!(committed.as_deref(), Some("新宿"));
```

Direct table access (no engine state needed):

```rust
use inputx_nihongo::{jukugo, kanji};

// All jukugo whose reading is `shinjuku`.
let jc: Vec<(&str, u32)> = jukugo::lookup_by_reading("shinjuku").collect();

// All kanji whose on-yomi includes `ka`.
let kc: Vec<(char, u32)> = kanji::lookup_by_reading("ka").collect();

// Prefix-prediction: jukugo whose reading STARTS with `shinjuk`.
let pred: Vec<(&str, u32, usize)> =
    jukugo::lookup_by_reading_prefix("shinjuk").collect();
```

## Plugin-style integration

The crate has no dependency on `inputx-wubi` / `inputx-pinyin`. The
consumer (typically `inputx-core::composite`) holds an optional
`JapaneseEngine` and routes keystrokes to it based on the
`enable_japanese` flag. When disabled the engine is never constructed
and JP has zero runtime cost on the hot path.

For the cement / dispatch wiring used by the Inputx IME, see the
companion [`inputx-nihongo-cement`](../inputx-nihongo-cement/) crate.

## Scope

- **In v1.4:** romaji → hiragana/katakana, on-yomi kanji lookup,
  jukugo compound lookup, mechanical `compose_sentence`
  (particle/copula sentence guesses, marked `composed: true` so the
  composite layer can score them below real dictionary entries),
  prefix-prediction for jukugo.
- **Not in v1.4:** okurigana / verb inflection, user-learning (no L0
  pin layer), per-kanji frequency ordering beyond what's baked into
  the tables.

## API stability

The 1.x line follows semver:

- **`JapaneseEngine` / `Candidate` / `KanaKind` public surface** —
  no breaking changes within 1.x. New `Candidate` fields land via
  `#[non_exhaustive]` semantics (already in place).
- **`jukugo::lookup_by_reading` / `jukugo::lookup_by_reading_prefix`
  / `kanji::lookup_by_reading` iterators** — signature stable; the
  underlying `JUKUGO_TABLE` / `KANJI_TABLE` const tables may grow
  (new entries) but never shrink in 1.x.
- **Chōonpu `-` handling** — fixed: `-` is a JP input character
  when the buffer is non-empty, rejected when empty. Stable across
  1.x.

Anything not on `pub` is internal — module layout, helper traits,
romaji-table internals — and may change in any minor release.

## License

Dual-licensed under [MIT](LICENSE-MIT) **OR**
[Apache-2.0](LICENSE-APACHE) © 2026 GOLIA K.K., at your option.

Bundled data (jukugo / kanji tables) is hand-curated from
permissively-licensed and public-domain sources; no GPL or LGPL
content is embedded.
