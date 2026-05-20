# inputx-pinyin

Self-developed Mandarin Pinyin input method engine for Rust — segmenter,
fuzzy syllables, FST-backed dict, L0 / L1+ ranking, WASM-ready via the
companion [`inputx-pinyin-wasm`](../inputx-pinyin-wasm/) crate.

Powers the **[Inputx](https://github.com/goliajp/inputx) IME** on iOS and
the web — this crate is the standalone, reusable Pinyin engine, also
publishable to crates.io for any downstream that wants a clean,
permissively-licensed Mandarin Pinyin stack.

**License:** MIT OR Apache-2.0 (dual). Built from scratch on
permissively-licensed sources only — see [License](#license) below for
the full attribution chain.

> Read this in [简体中文](README.zh-CN.md) · [日本語](README.ja.md).

## What's in the box

- **414,325 FST entries** (`data/pinyin.fst`, ~9 MB):
  - 44,357 single-character readings from Unihan `kHanyuPinlu` +
    `kMandarin`
  - 369,968 multi-character phrase readings — pypinyin canonical override
    for ~43k phrases, Unihan cartesian product for the long tail
  - 105 hand-curated heteronym entries collapse known-noise readings
    (`银行 → yinhang`, `重新 → chongxin`, `着陆 → zhuolu`, …)
- **Segmenter** — DP all-splits enumeration of a pinyin buffer
- **Fuzzy syllables** — 9 toggleable consonant/vowel-pair tolerances
  (`z⇄zh`, `n⇄l`, `en⇄eng`, …) for non-standard typists
- **L0 user-learning** — 3-pick auto-pin per `(input, word)` pair, with
  JSON-serializable snapshot for cross-session persistence
- **Streaming prefix scan** (`prefix_for_each`) — zero-allocation visitor
  over FST entries matching a prefix, used by Inputx's per-keystroke
  partial-input completion (`zho → 中国`, `zhong → 中国/中华/中央`, …)

## Quick start

```toml
# Cargo.toml
[dependencies]
inputx-pinyin = "1.0"
```

```rust
use golia_pinyin::{PinyinEngine, PinyinDict};

let eng = PinyinEngine::new();
let dict = eng.dict();

// Exact-syllable lookup (FST-backed).
let cands = dict.lookup("zhongguo");
// → ["中国", "中过", ...]

// Streaming prefix scan — visitor sees every entry whose pinyin starts
// with the given string, with no Vec allocation up front. Used for
// partial-input candidate generation in hot IME loops.
dict.prefix_for_each("zho", |pinyin, word, freq| {
    println!("{pinyin} {word} (freq={freq})");
});

// Tell the engine the user picked a word. After 3 picks of the same
// (input, word), it's auto-pinned to L0 for that input.
dict.record_pick("zhongguo", "中国");
```

> The crate name on crates.io is `inputx-pinyin`, but the lib name is
> `golia_pinyin` for ergonomic imports — `use golia_pinyin::...` works
> directly.

## Performance

The companion [Inputx IME](https://github.com/goliajp/inputx) enforces a
**per-keystroke 16 ms / 60 Hz frame budget** on the full
candidate-refresh pipeline (exact lookup + 简拼 initials + prefix scan +
filter), measured in its [`perfgate` unit
test](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-core/src/composite/pinyin_adapter.rs).

Bare engine numbers (Apple Silicon, release):

| Op | Latency |
|---|---|
| `dict.lookup("zhongguo")` | < 100 ns |
| `dict.prefix_for_each("zhong", ...)` | ~ 0.8 ms |
| `dict.prefix_for_each("z", ...)` worst case | ~ 4.5 ms |
| `dict.record_pick(input, word)` | < 1 µs |

## Tools (`--features tools`)

Maintainer-only binaries for regenerating the FST from upstream sources.
Library consumers don't need these:

```sh
cargo run --features tools --release --bin unihan-extract-readings
cargo run --features tools --release --bin compose-phrase-readings
cargo run --features tools --release --bin pinyin-fetch-corpus
cargo run --features tools --release --bin pinyin-build-weights
cargo run --features tools --release --bin pinyin-build-fst
```

## License

Engine code is dual-licensed under [MIT](LICENSE-MIT) **OR**
[Apache-2.0](LICENSE-APACHE) © 2026 GOLIA K.K., at your option.

### Bundled data and its licenses

| Source | License | What it contributes |
|---|---|---|
| **Unihan Database** | [Unicode License v3](LICENSE-UNICODE) | Per-char pinyin readings (`kHanyuPinlu`, `kMandarin`) |
| **jieba** (`fxsjy/jieba`) | [MIT](LICENSE-JIEBA) | ~349k Mandarin phrase entries used to seed phrase readings |
| **pypinyin** (`mozillazg/python-pinyin`) | [MIT](LICENSE-PYPINYIN) | ~47k hand-curated canonical phrase pronunciations |
| **Leipzig Corpora Collection** | CC-BY 4.0 | `zho_wikipedia_2018_1M`, `zho_news_2020_100K` — frequency weights |
| **SUBTLEX-CH-WF** | CC-BY 4.0 | Spoken-register frequency weights from film subtitles |

The published crate only ships the *derived* FST + integer frequency
scores — none of the source corpora text is redistributed.
