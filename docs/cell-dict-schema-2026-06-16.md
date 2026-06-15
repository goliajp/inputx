# Cell-dict pack schema · CP-5.2 step-1

Phase 5 climb plan CP-5.2 introduces **cell-dicts** — small TOML files
shipping domain vocabulary that the user can install to bias the
candidate list toward their work. An IT person installs an IT pack and
`weifuwu` always leads with 微服务; a finance person installs a finance
pack and `peizhi` leads with 配置 instead of the higher-frequency 陪侄;
and so on.

This document spells the v1 file format. Loaded packs sit between L0
(user pins) and L1 (the embedded corpus dict) — an **L0.5 layer**, in
the dict's existing two-tier ranking model.

## File format (TOML v1)

```toml
[meta]
name = "IT 术语"
version = 1                  # optional · pack schema version
author = "Inputx team"       # optional · free text
description = "Common IT / cloud / DevOps vocabulary"  # optional
license = "CC0-1.0"          # optional · SPDX identifier

[[entry]]
pinyin = "weifuwu"
word = "微服务"
freq = 500000                # optional · default 0

[[entry]]
pinyin = "yunjisuan"
word = "云计算"
freq = 500000
```

### `[meta]` block

| field         | type   | required | description                                   |
| ------------- | ------ | -------- | --------------------------------------------- |
| `name`        | string | ✓        | Display name (shown in Settings).             |
| `version`     | int    |          | Pack schema version. Reserved for migration.  |
| `author`      | string |          | Free-text author / maintainer identifier.     |
| `description` | string |          | One-line summary.                             |
| `license`     | string |          | SPDX identifier (`"CC0-1.0"`, `"MIT"`, …).    |

### `[[entry]]` array of tables

| field    | type   | required | description                                                              |
| -------- | ------ | -------- | ------------------------------------------------------------------------ |
| `pinyin` | string | ✓        | Lowercase pinyin lookup key (`"yunjisuan"`). `lue`/`nue` aliases accepted. |
| `word`   | string | ✓        | The Chinese (or Latin / mixed) candidate string.                          |
| `freq`   | int    |          | Frequency on the L1 `freq_score` scale. Default 0.                        |

## Freq scale guidance

`freq` is on the **same numeric scale** as the embedded dict's
`freq_score` field (corpus-derived, 0..1_000_000). Picking a value:

| value range          | effect                                                         |
| -------------------- | -------------------------------------------------------------- |
| `0` (default)        | Surfaces in lookup but won't outrank any non-trivial L1 entry. |
| `100_000`–`500_000`  | Comfortable "lead the list" range for a curated pack.          |
| `≥ 1_000_000`        | Dominates almost everything; reserve for must-win cases.       |

Loaded entries dedupe with L1 by `word`: when the same word appears in
both the pack and L1, the higher `freq` wins.

## Runtime API

```rust
use inputx_pinyin::PinyinEngine;

let engine = PinyinEngine::new();           // default Inputx engine
let toml = std::fs::read_to_string("docs/cell-dicts/it_terms.toml")?;
let n = engine.dict().load_cell_dict(&toml)?;
println!("loaded {n} entries");

// engine.dict().clear_cell_dict();         // wipe everything
// engine.dict().cell_dict_count();         // current entry total
```

`load_cell_dict` is **additive** — call it once per pack the user has
installed. Failure to parse leaves the existing L0.5 state untouched.

## Production wiring

Step-1 (this commit) ships the schema + loader + a minimum IT pack
under `docs/cell-dicts/it_terms.toml`. The remaining pieces land in
later step:

- **step-2** · Full IT (~500 entries) + finance pack (~300 entries).
- **step-3** · SettingsWindow toggle in the mac IME bundle, "install
  pack from disk" UI.
- **step-4** · Domain-MIU subset that validates "领域专词排序明显提升"
  per the climb-plan acceptance gate.

## Default behavior (byte-equal Phase 5 末)

A `PinyinDict` constructed via `embedded()` has an empty L0.5 — every
lookup path falls through the byte-equal pre-CP-5.2 code branch
(`cell_dict_hits` returns `Vec::new()`, the merge path is skipped).
Hosts that never call `load_cell_dict` see no behavior change.
