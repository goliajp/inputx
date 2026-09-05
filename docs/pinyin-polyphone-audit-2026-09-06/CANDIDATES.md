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

No user-visible ranking damage today (nothing to bury), so this is data
hygiene, not a polish emergency. Worth fixing if a reading-path correctness
pass ever runs; each would otherwise silently justify a wrong code.

| code | word | row says | should be |
|---|---|---|---|
| `zenge` | 憎恶 | [憎\|zēng][恶\|è] | 憎恶 is zēngwù — `zenge` should not exist |
| `zengwu` | 憎恶 | [恶\|wū] | code is right, tone/reading label wrong (wù) |
| `yingdan` | 英石 | [石\|dàn] | 英石 (stone, the unit) is yīngshí — `yingdan` should not exist |
| `faqia` | 发卡 | [发\|fā][卡\|qiǎ] | the hair clip is fàqiǎ — code right, 发 label wrong |
| `gongcha` | 公差 | [差\|chà] | mechanical tolerance is gōngchā — code right, label wrong |
| `hangtou` | 行头 | [行\|háng][头\|tóu] | stage costume is xíngtou — `hangtou` should not exist |
| `tiaomen` | 调门 | [调\|tiáo][门\|mén] | 调门 is diàomén — `tiaomen` should not exist |

## §3 — Class C: rare reading parked at #1 behind a common word

Visible but not blocking — the user's word already wins #0. Low value to
touch; listed so a future pass doesn't rediscover them.

`qianshou 纤手` (behind 歉收) · `qumu 取模` (behind 曲目) ·
`tiaosheng 调升` (behind 跳绳) · `lingchang 灵长` (behind 领唱) ·
`zhansheng 颤声` (behind 战胜) · `tantan 啴啴` (behind 谈谈) ·
`yuyu 喁喁` (behind 说说) · `zhengzheng 丁丁` (behind 整整) ·
`chanchan 啴啴` (behind 潺潺)

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
