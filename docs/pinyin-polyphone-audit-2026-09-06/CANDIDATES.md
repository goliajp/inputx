# Polyphone mis-coding audit — candidate list (2026-09-06)

Triggered by the `zhongzai 重载` report: 重载's everyday sense is chóngzài,
yet a zhòngzài row existed at `zhongzai` and outranked 重灾.
This is a read-only survey of how widespread that shape is.

## Method

Source: `core/crates/inputx-pinyin-v2/data/words.tsv` (88,135 rows — the v2
surface that actually decides ranking; v1 `library.tsv` is not consulted by
the default engine).

1. Group rows by word. 365 words have ≥2 rows.
2. Split those: **294 rows are exact duplicates** (identical code + word +
   reading_path + tier + source) — an ingest artifact, not a polyphone issue,
   tracked separately in §4. That leaves **75 words / 151 (word, code) pairs**
   where the same word sits under genuinely different codes.
3. Probe all 151 codes (`inputx-probe --mode mixed --jp`) to get real ranks.
   133 of 151 land in top-3 — rank alone does not separate signal from noise.
4. Per-row review of all 151. The filter that matters is **not** "is this
   reading rare" but:

   > Is this reading wrong-or-unusable for this word, AND does the row
   > outrank a word the user would actually be typing at this code?

   A rare-but-real reading that sits alone at its code (top-5 is just itself
   plus Japanese romaji fallbacks) harms nobody — the user never types that
   code. Those are dismissed, which is most of them.

## §1 — Class A: wrong reading, and it buries a common word

These are the exact `zhongzai` shape. Recommend D2 (hide from Path-1, keep
the dict row for K-best), same as `af0d6062`.

| code | word | reading in row | why wrong | buried by it |
|---|---|---|---|---|
| `chongdian` | 重点 | [重\|chóng][点\|diǎn] | 重点 is zhòngdiǎn; chóngdiǎn is not a word | **充电** (403736 vs 464956) |
| `baochang` | 保长 | [保\|bǎo][长\|cháng] | the office is bǎozhǎng; bǎocháng is not a word | 饱尝 / 报偿 / 包场 (401105 vs 402925) |
| `shidiao` | 失调 | [失\|shī][调\|diào] | 失调 is shītiáo (loss of balance); shīdiào is not a word | 石雕 / 失掉 / 时调 (403799 vs 404199) |

## §2 — Class B: reading_path is wrong, but the row sits alone at its code

**Re-reviewed 2026-09-06 after §1 shipped. Most of these do not survive a
second look — recorded here rather than silently dropped, because the first
pass would have justified seven edits and at least three of them are wrong.**

Two failure modes in the original pass:

- **A wrong `reading_path` label is not a wrong code.** For 发卡 / 公差 /
  憎恶(zengwu) the code is exactly right; only the annotation inside the row
  mislabels a tone or reading. That field does not drive ranking, so editing
  it changes nothing a user can observe.
- **"That reading doesn't exist" was asserted too fast.** 行头 hángtóu is the
  head of a 行会 (attested, 《水浒》); 调门 tiáomén is a real mechanical term
  (调节门); 石 as a unit of weight genuinely reads dàn, so 英石 yīngdàn has a
  basis even if yīngshí is the modern form. These were listed as "should not
  exist" on my reading alone.

| code | word | first pass said | after re-review |
|---|---|---|---|
| `zenge` | 憎恶 | zēngè not a word | **holds** — 憎恶 is zēngwù (厌恶 sense); zēngè has no basis |
| `zengwu` | 憎恶 | label [恶\|wū] should be wù | label-only, code correct — no ranking effect |
| `faqia` | 发卡 | label [发\|fā] should be fà | label-only, code correct — no ranking effect |
| `gongcha` | 公差 | label [差\|chà] should be chā | label-only, code correct — no ranking effect |
| `hangtou` | 行头 | hángtóu should not exist | **withdrawn** — hángtóu = head of a 行会, attested |
| `tiaomen` | 调门 | tiáomén should not exist | **withdrawn** — 调节门, real mechanical term |
| `yingdan` | 英石 | yīngdàn should not exist | **weakened** — 石 as a unit does read dàn |

**Recommendation: change nothing here.** Every row in this table sits alone
at its code (top-5 is itself plus Japanese romaji), so nothing is buried and
the fix has zero upside. The one row that survives review (`zenge`) would, if
hidden, only turn a working buffer into an empty one. D1 error cost is
asymmetric — a wrong delete silently breaks reverse-lookup somewhere unrelated
— and this table is exactly the shape where that cost gets paid for nothing.

## §3 — Class C: rare reading parked at #1 behind a common word

`qianshou 纤手` (behind 歉收) · `qumu 取模` (behind 曲目) ·
`tiaosheng 调升` (behind 跳绳) · `lingchang 灵长` (behind 领唱) ·
`zhansheng 颤声` (behind 战胜) · `tantan 啴啴` (behind 谈谈) ·
`yuyu 喁喁` (behind 说说) · `zhengzheng 丁丁` (behind 整整) ·
`chanchan 啴啴` (behind 潺潺)

**Recommendation: change nothing.** In every one of these the word the user
is actually typing already holds #0. Hiding the #1 row buys one slot in a
list nobody scrolls, at the same D1/D2 risk as §2.

## §4 — Separate finding: 294 exact-duplicate rows

290 keys in `words.tsv` appear twice with every field identical
(e.g. lines 4654/4655 `anxian 安闲`, 4803/4804 `baibai 拜拜`,
6632/6633 `bingfa 并发`). Unrelated to polyphony — an ingest artifact.
Not investigated here whether the duplicate affects scoring; flagged only.

## Dismissed

- `dele → 鬟` looked anomalous (a huán character at a `dele` code) but 鬟 is a
  **Wubi** candidate, not pinyin. Normal cross-engine tier behavior, not a bug.
- The remaining ~120 (word, code) pairs are legitimate dual readings
  (大夫 dàfū/dàifū, 便宜 biànyí/piányi, 口角 kǒujiǎo/kǒujué, 同行 tóngháng/tóngxíng,
  外传 wàichuán/wàizhuàn, 总长 zǒngcháng/zǒngzhǎng, …) or rare-but-real readings
  sitting alone at their code (出圈 chūjuàn, 咱家 zájiā, 频数 pínshuò, …).

## Status (2026-09-06)

- §1 — all three shipped: `a64533dd` chongdian · `4984e28b` baochang ·
  `9e69f1e1` shidiao. Each is a D2 row in `exclusions_v1.tsv` plus a
  buffer-scoped regression test.
- §2 — re-reviewed and **not acted on**; three of the seven claims were
  withdrawn on second look. See the table above.
- §3 — **not acted on**; the user's word already leads at every one.
- §4 — 294 duplicate rows, still open, unrelated to polyphony.
