# 2026-06-06 wubi 繁体 sweep audit

User report: "拼音[corrected→五笔]结果中有很多繁体的结果，这肯定不是
nihongokanji 打出来的" → "詞要的不是沉，而是不应该有，要打开繁体模式
才能有" → "你系统解决吧".

## What shipped

- `sweep_wubi_trad_v2.py` — the script that actually shipped.
  Reads `core/crates/inputx-wubi/data/auto_decomp.txt`, runs each
  char through `opencc -c t2s.json`, deletes rows whose char is
  Traditional AND whose simplified counterpart is reachable via
  any wubi source (auto_decomp + seed + jianma_simplified).
- `auto_decomp_deleted.tsv` — 3527 (trad_char, simp_peer) pairs
  removed from `auto_decomp.txt`.  These chars no longer enter the
  wubi FSA dict at all.
- `auto_decomp_orphans.tsv` — 570 TRAD chars KEPT.  Orphan means
  the simplified counterpart isn't reachable via any wubi source —
  deleting these would silently kill wubi lookup for those chars
  (mainland users would have no way to type them).  These wait for
  the 繁体-mode toggle feature.

## What didn't ship (kept for traceability)

- `sweep_wubi_trad_v1_library_unused.py` — the first script I
  wrote, which targeted `library.tsv` directly.  Turned out to be
  the wrong layer: library.tsv is a freq overlay on top of the
  encoder-generated dict, not the entry source.  Char-level wubi
  entries come from the encoder running over auto_decomp.txt.
  The script ran (and produced the next two files) but had no
  effect on actual dict contents.
- `library_trad_with_simp_peer.tsv` — 1164 (code, trad_word, simp)
  rows that the v1 script flagged in library.tsv.  These had
  freq-override rows but the underlying entry came from the
  encoder, so deleting from library.tsv didn't remove them.  The
  same 1164 (code, word) pairs ARE appended to
  `tools/scoring/data/polish/corpus_garbage_filter_v1.tsv` as the
  re-admission gate for future corpus-digest runs (which target
  library.tsv).
- `library_trad_orphans.tsv` — 2953 library-level orphans for
  reference.

## Effect

| Buffer | Before | After |
|---|---|---|
| `yngk` | 词 / 詞 / 肇事 / 启事 | 词 / 肇事 / 启事 |
| (any TRAD char in `auto_decomp_deleted.tsv`) | wubi surfaces TRAD | wubi can't surface this TRAD via simp-codes |

## Follow-up

- 繁体模式 toggle (framework feature): when shipped, the 570
  orphan TRAD chars and the 3527 swept ones both become reachable
  only when the toggle is on.  The current sweep is a non-toggle
  fix that defaults the engine to 简体-only behavior.
- If a user reports a specific TRAD char they need access to from
  the orphan list (570 chars), individual D1 reverts are fine —
  but the systemic answer is the toggle.
