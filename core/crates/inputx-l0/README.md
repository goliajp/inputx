# inputx-l0

Per-user pin / boost store for IME engines. **L0v1** binary format —
deterministic, atomic file writes, hard-reset friendly (old / unknown
files mount as empty without surfacing errors to the user).

## Why "L0"

"L0" sits *on top of* the corpus-derived dict `log_prior` and adds a
Q4 boost (`inputx_scoring::Q4 = 16`, so 1 log-unit per 16 integer
steps). A pinned word becomes effectively unbeatable at the user's
typical input — `weight=100` → `+100 ÷ 16 = 6.25` log units →
`e^6.25 ≈ 518×` linear-space ranking multiplier.

## Hard reset

`L0Store::open` returns `Err(BadMagic)` for unknown file formats
(including any pre-v1.4 JSON pin files). For "ignore old data, start
fresh" semantics — the path the Inputx reinstall flow uses —
`L0Store::open_or_empty` mounts as empty on any failure.

This is intentional per Inputx PLAN.md D3: "user 接受个性化 lost
(可重建)".

## Binary format

```
+---------------------------+
| Header (12 bytes)         |
|   magic[4]   "L0v1"       |
|   reserved u32  (= 0)     |
|   entry_count u32         |
+---------------------------+
| Entries (varlen each)     |
|   code_len u8             |
|   code_bytes (UTF-8)      |
|   word_len u8             |
|   word_bytes (UTF-8)      |
|   boost i16 (Q4 LE)       |
+---------------------------+
```

Each entry is `(code, word) → boost`. Code/word length capped at 255
bytes each (UTF-8); the asserts panic at write time if exceeded — IME
codes and words don't approach this limit (longest pinyin code is ~30
bytes, longest realistic word ~20 bytes).

## API

```rust
use inputx_l0::L0Store;

let mut store = L0Store::open_or_empty("pins.l0");
store.pin("jixu", "继续", 100);       // weight 0..=255
let boost = store.boost("jixu", "继续"); // Some(i16) Q4
store.forget("jixu", "继续");
store.reset();                         // drop everything
store.save("pins.l0")?;                // atomic write
```

`L0Store::iter()` returns `(code, word, boost)` in deterministic
`(code asc, word asc)` order. Two saves of the same in-memory state
produce byte-identical files.

## `no_std`

```toml
inputx-l0 = { version = "1.4", default-features = false }
```

Disables the std-only `open` / `save` helpers. `from_bytes` /
`to_bytes` / `pin` / `boost` / `iter` / `reset` stay available
(`alloc` required).

## License

MIT OR Apache-2.0.
