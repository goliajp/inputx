# inputx-ngram

N-gram log-probability lookup table for IME engines — bigram /
trigram / extensible to 4-gram. Q4 fixed-point `log_prob`, mmap
zero-copy file load, deterministic writer.

## Binary format (NGMv1)

```
+---------+---------------+--------------------+--------------------+
| Header  | String pool   | Triplet table      | FST ctx index      |
| 64 + 32 | varlen, pad8  | N × 8 B            | varlen (optional)  |
+---------+---------------+--------------------+--------------------+
```

- **Header (64 B + 32 B sha trailer)** — magic `b"NGMv"`,
  `format_version` (1), `max_n` (2/3/4), `entry_count`, section
  offsets and sizes, `sha256_of_payload`.
- **String pool** — deduplicated UTF-8, null-terminated, u24 offsets.
- **Triplet table** — 8 bytes per entry: `ctx_offset` (u24) +
  `next_offset` (u24) + `log_prob` (i16, Q4 fixed-point: 1 log unit =
  16 integer steps).
- **FST ctx index** — `inputx_fsa::Fsa` over ctx_bytes →
  first_triplet_index. Empty in v1.4.4 (reader linear scan fallback);
  cement layer populates in v1.4.6+.

For trigram (`max_n=3`), context words are joined by `\u{1F}` (Unit
Separator) and stored as a single string-pool blob.

## Usage

```rust
use inputx_ngram::{NgramTable, NgramBuilder};

// Reader (zero-copy mmap):
let table = NgramTable::open("bigrams.ngm")?;
let lp = table.log_prob(&["今天"], "是");           // Option<i16> in Q4
let top = table.top_k(&["今天"], 10);                 // Vec<(String, i16)>

// Writer (snapshot tooling, std-only):
let mut b = NgramBuilder::new(2);                     // bigram
b.add(&["今天"], "是", 250);
b.add(&["今天"], "的", 200);
let sha = b.build("bigrams.ngm".as_ref())?;
```

`NgramBuilder` is deterministic: same input → byte-identical output
+ identical payload sha256. Snapshot regenerators (e.g. v1.4.4's
`idf-from-pinyin-bigrams`) rely on this for verification.

## `no_std`

```toml
inputx-ngram = { version = "1.4", default-features = false }
```

Disables the std-only mmap loader and `NgramBuilder`. The schema
types + reader (over a caller-supplied byte slice) stay available.

## License

MIT OR Apache-2.0.
