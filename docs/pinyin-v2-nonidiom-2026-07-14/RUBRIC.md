# v2 non-idiom backfill — per-row review rubric

You are reviewing candidate words for admission into the **inputx pinyin v2**
dictionary. Every candidate already exists in the v1 corpus library
(`library.tsv`, digested from real Chinese corpora) but is MISSING from v2's
word list (v2 was built from CC-CEDICT + HSK, so it never absorbed corpus
vocabulary). Your job: decide, **one row at a time**, whether the row is a real
word worth typing.

Input row format: `code<TAB>word<TAB>freq`
(`code` = concatenated pinyin syllables, no tones; `freq` = v1 corpus freq.)

## Verdict per row — RECALL or KEEP

**RECALL** (admit into v2) — the row is a genuine lexical unit a Chinese typist
would want as one candidate:
- common nouns / verbs / adjectives / adverbs (图鉴, 收件, 轻量, 躲过去)
- established compounds and set phrases (拉肚子, 花草树木, 一步到位)
- widely-known proper nouns: countries, provinces, major cities, famous
  people/brands/works (北京大学, 星巴克, 鲁迅)
- common colloquial / internet words in real use (咋整, 给力, 内卷)
- technical/domain terms in general circulation (光合作用, 中间件, 显卡)
- reduplications and固定叠词 that are real words (干干净净, 马马虎虎)

**KEEP** (do NOT admit) — anything below. When unsure, **KEEP**:
宁缺毋滥 — a missing word costs one polish report; a garbage word pollutes
every buffer it shares a code with, forever.
- **Segmentation noise**: jieba sub-word fragments, arbitrary slices that are
  not words (的时候, 了一个, 和其他, 在这个)
- **Ad-hoc phrases**: 疑问代词/副词/判断词 + 谓语/名词 glued together
  (如何消除, 这些隐患, 我们应该, 可以看到). A typist types these in pieces.
  Burden of proof is on "would someone really type this as ONE unit".
- **Character-by-character literal pinyin** of a non-word string.
- **Wrong pinyin code**: the code does not match the word's standard reading
  (polyphone mismatch, e.g. 佛 as `fu` instead of `fo`, 阿 as `bua`). Verify
  every syllable against the word's actual reading. Any mismatch → KEEP.
- **Misspellings / wrong characters** (再接再励, 默默无名, 做为) and
  **variant/rare glyphs** (锺情, 缥渺) — the correct form usually also exists.
- **Classical Chinese / archaic fragments** not used in modern writing.
- **Obscure personal names, minor place names, rare surnames+given-name combos.**
- **Assembled long compounds** (5+ chars): verb phrases, slogan slices,
  headline fragments. Only RECALL 5+ char rows that are genuine fixed units
  (成语式, 谚语, established terms like 中华人民共和国, 社会主义核心价值观).
- **Dirty / offensive corpus junk** with no legitimate typing use.

## Hard rules
- Judge **every row independently**. No thresholds, no batching, no "high freq
  so probably fine". A freq-40000 row can still be segmentation noise; a
  freq-150 row can still be a perfectly good word.
- Do NOT skip rows. Do NOT sample. Coverage must be 100%.
- Do NOT edit any repository file.

## Output
Write ONLY the RECALL rows to your assigned output file, one per line:
`code<TAB>word<TAB>freq<TAB>短理由(中文,≤12字)`
No header, no KEEP rows, no commentary in the file.
Then reply with exactly: `<chunk-id> reviewed=<N_rows_read> recalled=<N>`.
