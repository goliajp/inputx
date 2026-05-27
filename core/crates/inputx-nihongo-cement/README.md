# inputx-nihongo-cement

Japanese-specific consumer-engine cement for Inputx — built on top
of the [`inputx-nihongo`](https://crates.io/crates/inputx-nihongo)
facade plus [`inputx-dict-format`](https://crates.io/crates/inputx-dict-format)
(IDFv1 dict reader).

## Public surface (v1.4.6 sub-phase C populates this)

- `NihongoIdfLookup` — IdfReader-driven jukugo / kanji lookup
  adapter
- chouonpu plumbing helpers

## Architecture note

The stateful `JapaneseAdapter` lives in `inputx-core/composite/` —
it shares cross-engine `mode` + `scoring` + `merge` modules with
wubi/pinyin paths, making it **composite root cement** (PLAN-stones-
extract.md terminology). This crate contains only genuinely
nihongo-specific helpers — anything a third-party JP IME consumer
would copy verbatim.

## License

Dual-licensed under MIT OR Apache-2.0.
