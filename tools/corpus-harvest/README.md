# Inputx corpus harvest

Pluggable harvesters that pull fresh public-language corpora and emit
normalized `(word, count, source, fetched_at)` TSVs. Output feeds
into `tools/dict-pipeline/` (see [[PLAN-v1.7]] WU-ρ) which produces
the next-generation private-dict snapshot.

## What's here

- `manifest.yaml` — source registry (URLs / licenses / refresh
  cadence / harvester script name). Each source row is one
  configurable harvest target.
- `LICENSES.md` — attribution + share-alike obligations per source.
- `harvest_zh_wikipedia.py` — Wikipedia 中文 dump → word counts.
- `output/<source>/<YYYY-MM-DD>.tsv` — harvested word counts (gitignored).

## Running a harvest

```sh
# Reproducible: same date → byte-identical output TSV
python3 tools/corpus-harvest/harvest_zh_wikipedia.py --date 20260501

# Without --date: uses manifest.yaml sample_date for dev/test
python3 tools/corpus-harvest/harvest_zh_wikipedia.py

# Dry run: parse 100 articles only, useful for pipeline smoke-test
python3 tools/corpus-harvest/harvest_zh_wikipedia.py --dry-run
```

Output goes to `output/zh-wikipedia/<date>.tsv`.

## Why this exists

Inputx's bundled dict (`data/private-dict/v0.0.1/*`) is a snapshot
from a one-time bootstrap (`tools/legacy-dict-bootstrap/`). New
internet words / brand names / 网络梗 don't appear until we re-train.
v1.7 builds the **persistent pipeline** so re-training is a single
command, not a one-off forensic exercise.

The harvest layer (this dir) only collects raw word counts. The
build layer (`tools/dict-pipeline/`, v1.7.1) takes these TSVs +
existing dict + diff-reports the changes that would land in vNEXT.
No dict actually ships in v1.7 — that's v1.8 work.

## Adding a new source

See LICENSES.md §"Adding a new source". TL;DR: license-check first,
then add a manifest row + license entry + harvester script in one
commit.

## Reproducibility contract

- Same source URL + script version → same output bytes (modulo
  fetched_at column).
- Harvester scripts deterministic: no random seeds left unset, no
  network jitter, no parallel race (single-threaded extract).
- Sources pinned by date in URL template — `latest` aliases used
  only as fallback when a specific dated dump 404s.

## Dependency policy

stdlib-only by default. Where third-party libs are absolutely needed
for usable output (e.g. jieba for Chinese word segmentation), the
harvester checks at startup and prints an install hint
(`pip install jieba`) rather than vendoring or auto-installing.
