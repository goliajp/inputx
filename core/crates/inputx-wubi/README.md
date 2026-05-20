# inputx-wubi

Self-developed Wubi 86 (五笔字型) encoder + dictionary for Rust.
PHF + FST backed, with a built-in **L0 / L1+ ranking model** and per-user
auto-learning. WASM-ready via the companion
[`inputx-wubi-wasm`](../inputx-wubi-wasm/) crate.

Powers the **[Inputx](https://github.com/goliajp/inputx) IME** on iOS and
the web — this crate is the standalone, reusable Wubi engine, also
publishable to crates.io for any downstream that wants a clean,
permissively-licensed Wubi stack.

**License:** MIT OR Apache-2.0 (dual). The dictionary data derives from
the public Wubi 86 standard (王永民, 1986); no GPL or LGPL data is
embedded.

> Read this in [简体中文](README.zh-CN.md) · [日本語](README.ja.md).

## What's in the box

- **135,822 FST entries**:
  - 25 一级简码, 616 二级简码, 5,173 三级简码
  - 53 hand-curated 字根 + 70,317 algorithmically-decomposed CJK chars
  - 61,205 phrases (词组)
- Wubi 86 encoder, all four canonical rules
- **L0 / L1+ ranking** — immutable layered lexicon (Auto < Phrase < Zigen
  < Jianma3 < Jianma2 < Jianma1 by base weight) plus a mutable per-user
  override layer with a 3-pick auto-promotion rule
- Layer prefs — host-tunable multipliers per layer
- Reproducible weight pipeline (`wubi-build-weights`) with CI byte-diff
  verify

## Quick start

```toml
# Cargo.toml
[dependencies]
inputx-wubi = "1.0"
```

```rust
use wubi::WubiDict;

let dict = WubiDict::embedded();

// Lookup is L0 + layer + freq ranked.
let candidates = dict.lookup("khlg");
// → ["中国", "跑车", "跨国", "䟧", ...]

// Hot loop (IME use case): reuse the buffer.
let mut buf = Vec::new();
dict.lookup_into("ipbf", &mut buf);

// Tell the dict the user picked a candidate. After 3 picks of the same
// (code, word), it's auto-pinned to L0. Pin/forget/layer-prefs APIs are
// also exposed for explicit host control.
dict.record_pick("khlg", "跑车");
```

> The crate name on crates.io is `inputx-wubi`, but the lib name is
> `wubi` for ergonomic imports — `use wubi::...` works directly.

## Performance (Apple Silicon, release)

| Op                            | Latency    |
|-------------------------------|------------|
| 字根 / 一级简码 PHF lookup    | ~10 ns     |
| `dict.lookup` (1–6 cand)      | 270–620 ns |
| `dict.lookup` miss            | ~145 ns    |
| `dict.prefix` (~5K cand)      | ~1.3 ms    |
| Encoder                       | 8–15 ns    |

## Tools (`--features tools`)

- `wubi-fetch-corpus` — download corpora declared in
  `data/corpus/manifest.toml`, SHA-verified, cached locally
- `wubi-build-weights` — scan corpora, derive `data/weights/weights.tsv`
  + `data/weights/provenance.toml`. `verify` mode for CI byte-diff.

```sh
cargo run --features tools --release --bin wubi-build-weights
cargo run --features tools --release --bin wubi-build-weights -- verify
```

## License

Dual-licensed under [MIT](LICENSE-MIT) **OR** [Apache-2.0](LICENSE-APACHE)
© 2026 GOLIA K.K., at your option.
