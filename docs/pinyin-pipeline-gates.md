# Pinyin pipeline gates

Four `pub(crate) const bool` toggles at the top of
`core/crates/inputx-core/src/composite/pinyin_adapter.rs` pause whole
families of pinyin candidate-generation behavior so the engine can be
polished category-by-category instead of all-at-once.

Initial state (2026-06-06, `_PREDICTION` added 2026-06-11): all
`true` — every speculative or mechanical generator is paused; only
literal pinyin spelling + mid-typing prefix prediction survives.
Flip a const to `false` to re-enable the entire family.

Tests that pin disabled-family behavior carry an early-return on the
same const, so flipping it back to `false` auto-revives the
assertions without anyone needing to remember to un-`#[ignore]`.

## `PINYIN_DISABLE_COMPOSE`

When `true`, the engine never *assembles* a candidate by stitching
multiple dict entries together.  Disables:

- **Long-buffer Viterbi sentence assembly** — for buffers ≥ 8 bytes,
  segment the typed pinyin into a sequence of dict-matched phrases
  and emit the concatenation.  Example with the gate `false`:
  `yongbuliao → 用不了` (assembled from `用`, `不`, `了` — no single
  dict entry at the buffer code).
- **K-best short-buffer composition** — when every other generator
  produced an empty list for a short multi-syllable buffer, fall
  back to a Viterbi K-best assembly from single-char dict entries.
  Example with the gate `false`: `kaopu → 靠谱`,
  `taikexi → 太可惜` (when those words aren't dict entries).
- **Mechanical fallback composition** — last-resort character-by-
  character segment for buffers that don't compose meaningfully.
  Example with the gate `false`: `akashi → 阿卡是` (sits at a low
  speculative tier so JP / wubi can win, but is at least visible).

With the gate `true`, the same buffers return empty from pinyin
(other engines unaffected).

## `PINYIN_DISABLE_ASSOCIATION`

When `true`, the engine doesn't surface typing-shortcut candidates
that aren't real pinyin spellings.  Disables:

- **Repeated-letter interjection** — 3+ copies of the same letter
  map to the matching reduplicated Chinese interjection.  Examples
  with the gate `false`:
  `hhhh → 哈哈哈哈`, `aaaa → 啊啊啊啊`, `mmm → 嗯嗯嗯`,
  `www → 呜呜呜`, `eee → 诶诶诶`, `ooo → 哦哦哦`.
- **简拼 (first-letter abbreviation)** — vowel-free buffer is
  reverse-looked-up as token initials.  Examples with the gate
  `false`: `zg → 中国`, `wsm → 为什么`, `hhh → 哈哈哈`.

With the gate `true`, both behaviors are off.  `hhhh` in Mixed mode
falls back to whatever wubi has at the `h*` codes (e.g. `目` at
`hhhh` wubi simcode).

## `PINYIN_DISABLE_FUZZY`

When `true`, the engine doesn't try to interpret what the user
*might have meant* — only what they actually typed.  Disables:

- **Southern-dialect initial swaps** — common Sogou-style fuzzy
  initial expansion (`z↔zh`, `c↔ch`, `s↔sh`, `n↔l`, `f↔h`,
  `r↔l`, `in↔ing`, `en↔eng`, `an↔ang`).  Example with the gate
  `false`: `zongguo → 中国` (treated as `zhongguo` after `z→zh`
  swap, scored at a discount).
- **2-consonant-prefix typo rescue** — when the buffer is a 4–5
  char `[consonant][consonant][vowel-tail]` shape that doesn't
  parse as pinyin, reverse-look-up the 2-consonant prefix as
  initials.  Example with the gate `false`: `pyin → 拼音` (the
  user dropped the `i` between `p` and `y`).
- **Syllable-aware trim-retry** — last-resort recovery for
  4–5 char buffers where typing slipped on the trailing char.
  Drops trailing chars until the remainder is a valid prefix,
  then surfaces that shorter prefix's completions.  Example with
  the gate `false`: `shehv → 社会 / 奢华 / 设好 / ...` (`v` is
  treated as a stray keystroke; the panel matches `sheh`).

With the gate `true`, all three of those buffers return empty.

## `PINYIN_DISABLE_PREDICTION`

When `true`, the panel never shows post-commit next-word
predictions (the Sogou-style 联想 panel).  Unlike the other three
gates, the gate point lives in
`core/crates/inputx-core/src/composite/engine.rs`
(`CompositeEngine::refresh_predictions`) — the const itself stays in
`pinyin_adapter.rs` with the rest of the family.  Disables:

- **Post-commit next-word predictions** — after committing two
  consecutive CJK words (strict-trigram context, v1.4 policy), the
  panel stays visible and offers predicted continuations without
  any typing.  Example with the gate `false`: commit 我们 then
  一起, panel offers trigram continuations of (我们, 一起, *).
- **Chained prediction commits** — picking a prediction re-seeds
  the context and fires the next round (capped at
  `PREDICTION_CHAIN_LIMIT` = 2 consecutive picks).

With the gate `true`, `predicted_candidates()` is always empty, so
the host hides the panel after every commit.  Note the distinction
from `PINYIN_DISABLE_ASSOCIATION`: that gate covers **in-buffer
typing shortcuts** (简拼, repeated-letter); this one covers the
**after-commit** prediction surface.  Both are colloquially "联想".

## What's always on (not gated)

Three families have no toggle because the engine would be useless
without them:

- **Exact-syllable lookup** — type literal pinyin syllables,
  get the dict matches at that exact code.  This is the
  "正确拼写" path: `nihao → 你好 / 泥壕`, `shijian → 时间`.
- **FST prefix completion** — mid-typing word prediction.
  This is the "预测性输入" path: `zho → 中国 / 中华 / 重 / ...`,
  `lianxia → 联想`.
- **Rare-CJK display filter** — orthogonal to candidate
  generation; just hides unicode-rare CJK chars from the visible
  ranking so they don't crowd the panel.

## How to re-enable a family

1. Edit `core/crates/inputx-core/src/composite/pinyin_adapter.rs`
   and flip the matching const from `true` to `false`.
2. Run `make polish-rebuild` and `cargo test -p inputx-core --lib
   --release`.  Tests pinned to that family's behavior switch from
   "early-return" mode to actual assertions; if anything's broken,
   it surfaces now.
3. Probe a couple of the example buffers above with
   `core/target/release/inputx-probe <buf> --mode mixed`.

## Why these three, why this initial state

User directive 2026-06-06: "我们可以先暂停所有的拼音里 拼接字、
联想以及错别字模糊吗？只保留正确拼写和预测性的输入，我们一个个细节
来做好".

The three categories map 1:1 to "拼接字 (compose) / 联想
(association / shortcuts) / 错别字模糊 (fuzzy / typo recovery)".
Initial state pauses all three together; the polish work that
follows re-enables one family at a time so each detail can be
validated in isolation rather than fighting noise from the other
two.

`PINYIN_DISABLE_PREDICTION` joined 2026-06-11 ("我们把联想也先用
flag 关闭吧") — the post-commit prediction panel is the remaining
"联想" surface the 2026-06-06 sweep didn't cover, paused under the
same polish-one-at-a-time regime.
