# inputx-scoring

Probability-native candidate scoring primitive for IME engines.

```text
score(W | i) = log_prior(W) + log_likelihood(i | W)
```

This crate ships only the schema — the `Candidate` struct, the
`MatchType` enum, the `Source` tag, and the additive sort key. It has
no opinion on how producers derive `log_prior` or `log_likelihood`;
that lives in the consuming engine (Inputx wubi / pinyin / nihongo
cement crates, or any third-party IME that wants the same Bayesian
shape).

## Why log-space, why fixed-point

`P(W|i) ∝ P(i|W) · P(W)` is the standard Bayesian decomposition for
candidate ranking. In log space, multiplication becomes addition —
so each candidate's score is the sum of two independent factors, and
producers (dict lookups, n-gram scorers, fuzzy matchers) can contribute
to either term without coordinating a single global formula.

`i32` Q4 fixed-point (`Q4 = 16`) gives ~0.0625 log-unit resolution
over ±67 million log units — far more headroom than any realistic
corpus needs. Integer arithmetic also keeps the path deterministic
across platforms (no `f64` rounding drift between mac / iOS / Linux).

## Schema

```rust
pub const Q4: i32 = 16;

pub enum Source { Wubi = 0, Pinyin = 1, Japanese = 2 }

pub enum MatchType {
    Exact,
    Prefix(u16),                       // proximity_milli
    Fuzzy(u16),                        // edit_cost_milli
    Composed { bigram_links: u8 },     // chain_len - 1
}

pub struct Candidate<'a> {
    pub word: &'a str,
    pub log_prior: i32,
    pub log_likelihood: i32,
    pub match_type: MatchType,
    pub source: Source,
}

pub fn score(c: &Candidate<'_>) -> i32 { c.log_prior + c.log_likelihood }
```

## Usage

```rust
use inputx_scoring::{Candidate, MatchType, Source, Q4, score};

let c = Candidate {
    word: "继续",
    log_prior: Q4 * 10,
    log_likelihood: Q4 * 5,
    match_type: MatchType::Exact,
    source: Source::Pinyin,
};
assert_eq!(score(&c), Q4 * 15);
```

## `no_std`

Disable the `std` feature for `#![no_std]` builds (the schema itself
is `core` only; the `std` feature gates optional formatting helpers).

```toml
inputx-scoring = { version = "1.4", default-features = false }
```

## License

Dual-licensed under MIT or Apache-2.0.
