# inputx-phonetic-edit

Weighted phonetic edit-distance: Wagner-Fischer DP extended with
arbitrary-length substitution pairs from a user-supplied cost table.

Plain Levenshtein treats every substitution as cost 1. That's wrong for
pinyin: a southern-Mandarin typo (`zongguo` for `zhongguo`) is a single
low-confidence swap, not a one-cost edit on par with `xongguo`. This
crate generalizes Wagner-Fischer with a **cost table** of
`(from, to, cost)` substitutions of any length, then ships a Mandarin
default table covering the nine canonical fuzzy pairs.

```toml
[dependencies]
inputx-phonetic-edit = "1"
```

## Example

```rust
use inputx_phonetic_edit::{edit_distance, MANDARIN_DEFAULT, EditCostTable};

// Empty table → plain Levenshtein
assert_eq!(edit_distance("kitten", "sitting", &EditCostTable::EMPTY), 3.0);

// Southern fuzzy: zh↔z swap costs 0.3, not 1.0
let d = edit_distance("zhongguo", "zongguo", &MANDARIN_DEFAULT);
assert!((d - 0.3).abs() < 1e-9);

// Composite: zh↔z (0.3) AND in↔ing (0.2) combine to 0.5
let d = edit_distance("zhin", "zing", &MANDARIN_DEFAULT);
assert!((d - 0.5).abs() < 1e-9);
```

## Mandarin default table

| Pair      | Cost | Class                  |
|-----------|------|------------------------|
| zh ↔ z    | 0.3  | retroflex initial      |
| sh ↔ s    | 0.3  | retroflex initial      |
| ch ↔ c    | 0.3  | retroflex initial      |
| f ↔ h     | 0.3  | labial / glottal       |
| r ↔ l     | 0.3  | initial confusion      |
| n ↔ l     | 0.2  | initial nasal/lateral  |
| in ↔ ing  | 0.2  | nasal final            |
| en ↔ eng  | 0.2  | nasal final            |
| an ↔ ang  | 0.2  | nasal final            |

Same nine pairs that `inputx-pinyin`'s `FuzzyConfig` toggles — by design.

## Properties

- **Zero dependencies.**
- **`#![no_std]`** with `--no-default-features` (alloc only).
- **`#![forbid(unsafe_code)]`**.
- **Deterministic** for the same `(a, b, table)` input.
- Distance is `f64`; substitution costs are user-supplied
  (`0.0..=1.0` by convention but no upper bound is enforced).

## API stability

The 1.x line follows semver:

- **`EditCostTable` / `edit_distance` public surface** — no breaking
  changes within 1.x. New cost-table helpers may be added as minor bumps.
- **`MANDARIN_DEFAULT` pair set** — fixed. Adding or removing pairs would
  silently break callers that compare against `inputx-pinyin`'s
  `FuzzyConfig`; any future re-tuning lands as a separately-named table
  (`MANDARIN_v2` etc.) instead of mutating the const.
- **`#![no_std]` invariant** — building with `--no-default-features`
  continues to compile for `alloc`-only targets across the entire 1.x line.

## License

MIT OR Apache-2.0.
