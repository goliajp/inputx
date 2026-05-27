# Legacy dict-bootstrap pipeline

Pre-IDFv1 pipeline that generated the bundled `pinyin.dict` /
`bigrams.fsa` / `trigrams.dict` data shipped inside the
`inputx-pinyin` facade crate. v1.4.3 introduced IDFv1
(`data/private-dict/v0.0.1/pinyin/words.idf`) as the
forward-compatible dict format; v1.4.7+ sort-key cutover will switch
the composite engine to read .idf directly, at which point this
pipeline becomes purely historical / reproducibility-anchor.

## What's here

- `build_pinyin_modern.py` — modern Chinese word list builder with
  finance-domain expansion
- `build_pinyin_bigrams.py` — bigram count extractor → bigrams.fsa
  shape source
- `build_pinyin_trigrams.py` — trigram count extractor → trigrams.dict
- `build_name_chars.py` — name-character corpus filter (was
  `tools/scoring/03b_pollution/build_name_chars.py`)

## What's NOT here yet

The full `tools/scoring/` directory (Makefile, refresh.sh, README.md,
01_fetch / 02_extract / 03_normalize / 03b_pollution / 06_llm_annotate
/ 07_validate subdirs, and the .py files like audit_pinyin_overlays.py
/ discover_new_words.py / strip_long_entries.py / etc.) still lives
in place at `tools/scoring/` because:

1. The corresponding `core/crates/inputx-pinyin/tools/build_weights.rs`
   and `build_ngrams_fsa.rs` reference `tools/scoring/data/...` paths
   for extracted-corpus intermediate files; moving the whole dir
   would break those tools.
2. v1.4.7+ sort-key cutover will retire `build_weights.rs` /
   `build_ngrams_fsa.rs` themselves; at that point the full
   `tools/scoring/` can move here cleanly.

## Per-facade corpus manifests

`core/crates/inputx-pinyin/data/corpus/manifest.toml` +
`core/crates/inputx-wubi/data/corpus/manifest.toml` stay in their
facade crates — they're referenced by the facade's own
`tools/fetch_corpus.rs` build-time tools, not by runtime.

## Status

This directory satisfies PLAN.md L4 v1.4.6 → v1.4.7 trigger (c)
(`tools/legacy-dict-bootstrap/` exists, legacy build_*.py moved).
The remaining `tools/scoring/` migration is v1.4.7+ scope.
