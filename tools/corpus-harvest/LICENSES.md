# Corpus source licenses

This directory's `output/*` data derives from third-party sources. Each
source license is recorded here. Any derivative dict shipped from
Inputx that incorporates this data must respect the obligations
below.

## License rules (per [[PLAN-v1.7]] D19)

Only sources under MIT / Apache 2.0 / CC0 / CC BY-SA / public-domain
licenses are eligible. Sources requiring per-row attribution
(non-aggregate-attribution licenses) are out of scope — Inputx ships
dict data as numeric word frequencies, not per-row attributable
content.

## Active sources

### zh-wikipedia

- **License**: [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/)
- **Attribution**: required at the *dataset* level. Inputx's derivative
  dict must include a notice that wikipedia data was used. See
  `core/crates/inputx-pinyin/data/corpus/manifest.toml` for the existing
  attribution surface; new dict ships add a `zh-wikipedia` source row
  there.
- **Share-alike**: derivative dicts published from this corpus are
  themselves CC BY-SA. Inputx's private-dict snapshots
  (`data/private-dict/*`) carry license metadata that downstream
  consumers can read.
- **What we use**: page text → word-segmented counts. No per-page
  metadata, no edit history, no user info.

## Excluded sources (license-incompatible)

The following common Chinese corpora are explicitly **not** eligible:

- **Common Crawl 中文** (CC-MAIN-*) — the crawled pages individually
  carry the source site's original license; treating the aggregate as
  CC0 is misleading. We don't ship these without per-source vetting.
- **微博 / 知乎 / Reddit 中文** scraped corpora — TOS-restricted; even
  if academic-research-permitted, dict redistribution likely violates.
- **THUOCL / SCEL files** without explicit license — common in
  Chinese IME communities but most lack a clear redistributable
  license. Skip until we find license-clear successors.

## Adding a new source

1. Verify license. Prefer CC BY-SA (Wikipedia-style) or public-domain.
2. Add a row to this file with the same fields as zh-wikipedia above.
3. Add an entry to `manifest.yaml`.
4. Implement the harvester script `harvest_<source>.py`.
5. Commit all three in one change.
