# tools/scoring/data — directory layout

Post-2026-06-03 治理 architecture. See `.claude/PLAN-corpus-治理.md`
for the full design rationale.

## Layout

```
tools/scoring/data/
├── digest_log.toml             ← append-only event history
│                                  ("what happened, when")
├── source_registry.toml        ← mutable per-source current state
│                                  ("what version is the latest we know")
│                                  drives automated feeding workflows
│
├── polish/                     ← polish-mutable data (the 5 main surfaces)
│   ├── exclusions_v1.tsv         (D2 Path-1 display filter)
│   ├── corpus_garbage_filter_v1.tsv  (D1 future-proof record)
│   ├── prior_corrections_v1.tsv      (word-level Q4 log-prior boost)
│   ├── tier_overlay.tsv              (Class C per-entry tier override)
│   └── quickfix_boost.tsv            (Class B per-entry MAX freq overlay)
│
├── reports/                    ← polish-log analysis (read-only, not polish data)
│   ├── repeat_misses.tsv
│   └── near_miss_bigger_rank.tsv
│
├── supplemental/               ← legacy pre-治理 imports;
│   └── *.tsv                     scheduled for retirement / re-ingestion
│
└── baseline/ cache/ extracted/ filtered/ merged/  ← gitignored research artifacts
```

## Library files (NOT in this dir — engine-local)

The dict source of truth for each engine lives with the engine crate,
not under `tools/scoring/data/`:

```
core/crates/inputx-pinyin/data/library.tsv     ← pinyin dict 库
core/crates/inputx-wubi/data/library.tsv       ← wubi dict 库
core/crates/inputx-nihongo/data/library.tsv    ← nihongo dict 库
```

Library schema: `<code>\t<word>\t[engine-specific cols]\t<freq>\t<source>`
where `source ∈ {digested, polish}`.

## Polish protocol

See `.claude/skills/polish/SKILL.md` (Bedrock section) and
`.claude/RANKING-MODEL-INVARIANTS.md` (§3 data surface table) for the
canonical 4-class polish protocol (A 加词 / B 调顺序 / C 大降 / D
删 — with D split into D1 删错条 vs D2 隐藏正条).
