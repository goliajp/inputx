# Dogfood Polish STRICT mode — 1000 articles to 100% PASS

立 2026-06-30 reopen #4. **User directive**:每篇 100% 输入正确,1000 篇一篇不能少。

## 区别于前面 batched dogfood

| 维度 | 旧 dogfood(73.2% 收) | strict mode(本) |
|---|---|---|
| 粒度 | jieba auto-segmentation, batch aggregate | LLM per-segment thinking, per-fire |
| 通过标准 | aggregate 73% PASS 即可 wrap | 每文每段 100% PASS |
| Polish 触发 | batch-mode end-of-pass | per-segment realtime |
| Corpus | 272 articles partial | **1000 articles 一篇不少** |
| Polish 决策 | 自动 batch + threshold | LLM 判 add/delete/reorder one-by-one |

## State files

- `articles_status.tsv` — per-article state(`id status segments_done segments_total last_seg_idx note`)
- `articles/<NNNN>.md` — per-article segmentation 决策 + 输入 log
- `logs/polish_log.tsv` — append-only polish action log(date / buffer / action / before / after)
- `logs/iter_log.tsv` — iter-by-iter summary(article_id, segments_processed, polish_count, time)

## Workflow per /loop fire

1. Read articles_status.tsv,find first article with status != DONE
2. If article is TODO:
   - Read article text
   - Use LLM to decide segmentation(thoughtful — like real user typing)
   - Write segments to articles/<NNNN>.md
   - Mark IN_PROGRESS,seg_total=N
3. If article is IN_PROGRESS:
   - Resume from last_seg_idx
4. For each segment (resumeable):
   - probe v2 for buffer
   - read top-10
   - judge:expected word at #0?
     - YES → mark PASS (auto), continue
     - NO  → polish(decide: quickfix / modern_vocab / delete entry / etc), rebuild v2, re-probe, verify, then mark PASS
   - log to articles/<NNNN>.md
   - update last_seg_idx
5. When all N segments PASS, mark DONE
6. Commit + exit

## Constraints

- baseline 357/0 必须 throughout(每次 polish 后 quick smoke check)
- mac/reinstall.py 每 N articles 一次(N=10 maybe — too often is slow)
- Polish 累计 → 仍写入现有 `tools/scoring/data/polish/` 文件
- 全部 polish 行带 `# strict-<article_id>-<seg_idx>` 标签便于 traceback

## Corpus prep

Current: 272 articles in `scratchpad/corpus/articles/`. Need 1000.

Strategy:
- Slow fetcher 已 cancel 1h+ ago at 272(其实不太活跃 — 看下进程是不是死了)
- Restart with target 1000, 5s sleep(实际限速被 zhwiki 429 拖 → 1h 80+ articles)
- Allow ~10 hours background fetch to reach 1000

If fetch can't reach 1000, **STOP and ask** user — they said 一篇不能少。

## Status legend(per-article)

- `TODO` — not yet segmented
- `IN_PROGRESS` — segmenting OR processing segments
- `DONE` — all segments PASS
- `BLOCKED` — needs user input

## /loop iter checklist

```
1. Read articles_status.tsv
2. Pick article: first IN_PROGRESS or first TODO
3. If TODO: segment, write articles/<id>.md, mark IN_PROGRESS
4. Open articles/<id>.md, find next unprocessed segment
5. probe v2 → judge → polish if needed → re-verify
6. log + update last_seg_idx in state
7. If all PASS this article: mark DONE
8. commit + push + exit
```

Each fire processes some segments(maybe 5-15 depending on polish frequency).Long articles = multiple fires.

## Seed polish(applied as user's example demands)

iter 0 已 apply:
- `fenci 分词` quickfix 30000 (was 粉刺 #0)
- `neihe 内核` quickfix 30000 (was 内河 #0)
- `lunci 轮次` quickfix 30000 (was empty,corpus_garbage_filter 误删)

## Last action

- 2026-06-30 strict mode iter 0 — seed polish 3 applied,baseline 357/0 ✓,structure 建好。Next: corpus expand + article 0001 start。
