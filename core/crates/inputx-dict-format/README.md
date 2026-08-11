# inputx-dict-format

IDFv1 binary dict format for IME engines — mmap zero-copy reader,
deterministic writer, probability-native (Q4 log priors), shared layout
across pinyin / wubi / Japanese / future Korean and Vietnamese.

## Why

IME dict files are read on every keystroke and rebuilt rarely. The
hot-path constraint is mmap-friendly zero-copy decoding; the rebuild
constraint is deterministic output (so two builds from the same corpus
produce byte-identical files, verifiable by sha256).

IDFv1 sets one binary layout for all of those engines so the runtime
reader code, dual-path verification harness, and OTA delivery (post-v2)
stay a single implementation. Per-engine semantics ride in the
`engine_kind` byte and the `match_type` byte per entry.

## Layout (96-byte header + sections)

```
+---------+---------------+--------------+---------------+----------------+
| Header  | String pool   | Entry table  | FST code idx  | FST word idx   |
| 64 + 32 | varlen, pad8  | N × 16 B     | varlen        | varlen         |
+---------+---------------+--------------+---------------+----------------+
                                         | Bigram block (optional, v2+)   |
                                         | Embedding block (optional, v2+)|
                                         +--------------------------------+
```

- **Header (64 B)** — magic `b"IDFv"`, `format_version`, `engine_kind`,
  flag word, section offsets and sizes, embedding metadata.
- **sha256 trailer (32 B)** — payload hash covers everything from byte
  96 onward; reader rejects on mismatch.
- **String pool** — deduplicated UTF-8, null-terminated, padded to
  8-byte alignment. Entries refer to byte offsets (u24).
- **Entry table** — fixed 16 B per entry: `word_offset` (u24),
  `code_offset` (u24), `log_prior` (i16 Q4), `match_type` (u8 →
  `inputx_scoring::MatchType` variant), `flags` (u8), `raw_freq`
  (u32 — pre-quantization corpus freq, lossless tiebreaker for entries
  that land in the same Q4 `log_prior` bucket; v1.4.7 schema bump
  repurposed the previously-unused `bigram_offset` slot), 2 B reserved.
- **EntryFlags** — `BLACKLIST` (bit 0), `CURATED_OVERRIDE` (bit 1),
  `USER_ADDED` (bit 2), plus bits 5-7 `ENGINE_TAG_MASK` for an engine-
  specific 3-bit payload (used by `EngineKind::Wubi` to carry the
  `Layer` enum index; zero for other engines).
- **FST code / word indexes** — `inputx_fsa::Fsa` blobs (code →
  entry_index, word → entry_index). v1.4.3 ships with empty indexes;
  reader falls back to a linear scan over the entry table. v1.4.6+
  fills them.

## API

Reader (no_std + alloc clean, std for mmap):

```rust
use inputx_dict_format::{IdfReader, EngineKind};

let r = IdfReader::open("data/private-dict/v0.0.1/pinyin/words.idf")?;
assert_eq!(r.engine_kind(), EngineKind::Pinyin);
for entry in r.lookup(b"jixu") {
    println!("{} log_prior={}", entry.word, entry.log_prior);
}
```

Writer (std):

```rust
use inputx_dict_format::{IdfBuilder, EngineKind, EntryFlags};
use inputx_scoring::{MatchType, log_prior_from_freq};

let mut b = IdfBuilder::new(EngineKind::Pinyin);
b.add_entry(
    "jixu",
    "继续",
    i16::try_from(log_prior_from_freq(44_652)).unwrap_or(i16::MAX),
    MatchType::Exact,
    EntryFlags::default(),
);
let sha = b.build("words.idf".as_ref())?;
println!("built words.idf with sha256 {:x?}", sha);
```

## Determinism

`IdfBuilder::build` is deterministic given the input entry set:

1. Entries are sorted by `(code, word, log_prior)`.
2. Exact `(code, word)` duplicates are deduped.
3. String pool entries are sorted unique UTF-8.
4. Section bytes are written in a fixed order.

Two builds from the same input produce byte-identical files and the
same payload sha256. This is the verification gate at every snapshot
rebuild (PLAN-dict-format-IDFv1.md §"Build determinism").

## License

Dual-licensed under MIT or Apache-2.0.
