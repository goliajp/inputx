# Inputx Dev Inspector — local web debugger

A localhost web app to introspect Inputx's pinyin/wubi/JP engine state,
search the weights dictionary, view heteronym overrides, and watch the
polish-log telemetry stream. Built for the developer iteration loop:
type input → see what the engine does → adjust data → re-build → repeat.

## Run

```bash
python3 tools/devserver/server.py
# opens on http://localhost:7878
```

First run auto-builds the `inputx-probe` Rust binary (~30s release
build). Subsequent runs are instant.

## Tabs

### 候选探针 (engine probe)

Type a buffer, see the live candidate list with per-candidate source
attribution (Wubi / Pinyin / Japanese). Switch engine mode + JP toggle
to compare. Updates on every keystroke (80ms debounce).

Behind the scenes: subprocesses the `inputx-probe` binary, which feeds
the buffer through a real `Session` instance (same code path the IME
uses at runtime). What you see is the truth, not a model.

### 词典搜索 (weights)

Search the pinyin weights TSV by code prefix. See competing entries +
their frequencies. Exact-code matches surface first, then freq-desc.

### 多音字 (heteronyms)

Browse the canonical-reading overrides in
`core/crates/inputx-pinyin/data/heteronyms_curated.tsv`. The substring
search filters both phrase and pinyin columns. Useful for confirming
a 多音字 fix landed (e.g., 重新 → chongxin) before the runtime tells
you it worked.

### Polish 日志

Reads `~/Library/Containers/jp.golia.inputmethod.wubi/Data/Library/
Application Support/Inputx/polish-log.jsonl` — the mac IME's
non-#0-pick telemetry. Shows:
- aggregate: (buffer, pickedWord) pairs picked ≥ 2 times — strong
  signals that the dict needs a boost
- recent: last 50 raw entries, with candidate top-3 + picked index

## API surface

```
GET  /                  → SPA shell
GET  /api/probe?q=...   → live candidate breakdown (subprocess to inputx-probe)
GET  /api/weights?q=... → search weights.tsv by code prefix
GET  /api/heteronyms    → dump heteronyms_curated.tsv
GET  /api/polish-log    → recent + aggregated polish-log entries
```

All routes return JSON. Server is stdlib http.server — no flask, no
fastapi, no npm. The only dependencies are a Python 3 with no extras
and a Rust toolchain (for the probe binary).

## Future

The current build is **read-only** — viewer/explorer. Editor mode
(edit freq, add heteronyms, trigger rebuild) is open work; see
SCORING.md §1.3 for the larger pipeline-driven workflow that this
serves as the human-facing front-end for.
