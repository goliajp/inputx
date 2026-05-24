# inputx-fsa

A compact, **zero-dependency** ordered map from byte keys to `u64`, backed by a
**minimal acyclic finite-state automaton** (MA-FSA / DAWG) plus a value table
addressed by lexicographic rank.

It's a clean-room implementation written for the [Inputx](https://github.com/goliajp/inputx)
IME's dictionaries — a focused stand-in for the read + build path of the
excellent [`fst`](https://crates.io/crates/fst) crate, with **no dependencies**
and an IME-tuned two-level `Dict` on top.

```toml
[dependencies]
inputx-fsa = "1"
```

## What you get

- **`Fsa`** — an immutable `&[u8] → u64` map. `get`, prefix/range scan
  (eager `prefix()`, zero-alloc `prefix_for_each()`, or a lazy
  `Iterator` via `iter()` / `range()`).
- **`Dict`** — a one-code-to-many index: each byte *code* maps to an ordered
  list of `(item, value)` pairs. Keeps the unique item bytes *out* of the
  automaton, so it's markedly smaller than flattening `code\0item → value`
  into one FSA — the structure an IME wants (a pinyin/wubi code → its ranked
  candidate words).
- **Build once, read forever** — `Builder` / `DictBuilder` serialize to a
  `Vec<u8>` you embed (`include_bytes!`) or `mmap`; the reader works over any
  `AsRef<[u8]>` with no decompression step.

## Properties

- **Zero dependencies.** That is the entire point.
- **`#![no_std]`** with `--no-default-features` (alloc only) — reader *and*
  builder work; the default `std` feature just uses a faster build register.
- **`#![forbid(unsafe_code)]`**, and the reader never panics on a corrupt or
  untrusted buffer — out-of-range reads degrade to empty/None.
- **Small + fast.** Measured against `fst` on the Inputx shipped pinyin set
  (216k entries): the two-level `Dict` is **~12% smaller**, exact `get` is
  **~2× faster**, and prefix scan **~5.6× faster** (a tiny code automaton +
  contiguous item blob beats walking a `code\0item` graph per entry).

## Example

```rust
use inputx_fsa::{Builder, Fsa};

let mut b = Builder::new();
b.insert(b"apple", 1);
b.insert(b"apply", 2);
b.insert(b"banana", 3);
let bytes = b.finish();              // serialize (embed / mmap this)

let fsa = Fsa::new(bytes).unwrap();  // read over any AsRef<[u8]>
assert_eq!(fsa.get(b"apple"), Some(1));
assert_eq!(fsa.get(b"grape"), None);

// lazy, composable prefix scan
let app: Vec<_> = fsa.range(b"app").collect();
assert_eq!(app, vec![(b"apple".to_vec(), 1), (b"apply".to_vec(), 2)]);
```

Two-level `Dict` (a code → many ranked items):

```rust
use inputx_fsa::{DictBuilder, Dict};

let mut b = DictBuilder::new();
b.insert(b"wo", "\u{6211}".as_bytes(), 100); // 我
b.insert(b"wo", "\u{63e1}".as_bytes(), 40);  // 握
b.insert(b"women", "\u{6211}\u{4eec}".as_bytes(), 90); // 我们
let dict = Dict::new(b.finish()).unwrap();

// items come back value-descending (top candidate first)
let mut got = String::new();
dict.get_for_each(b"wo", |item, _v| got.push_str(std::str::from_utf8(item).unwrap()));
assert_eq!(got, "\u{6211}\u{63e1}"); // 我握
```

## License

MIT OR Apache-2.0.
