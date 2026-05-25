# Candidate Scoring System — Design Doc v2

This is the **operational** spec for Inputx's candidate ranking
quality. The runtime side is one piece; the bigger piece is the
**static-DB build pipeline** — how raw corpora become the weights
that ship inside `pinyin.dict` / `wubi86.dict` (self-built `inputx-fsa`
two-level `Dict`, not the `fst` crate — 0-dep migration). We treat that pipeline
as a serious software project with the four dimensions the user
called out: 来源 / 处理 / 更新 / 退出.

The "the DB is static, quality requirements are very high" framing
means we accept a heavy *offline* compute / curation cost in
exchange for fast deterministic runtime — no LLM in the keystroke
path, no per-user cloud calls, no telemetry-driven mutation of the
shipped dict. Quality moves through versioned data drops.

The runtime ranking model is the simple counterpart: every
candidate carries a `score`, the merge sorts by `score`, wubi 简码
gets a hard floor (Inputx-五笔 brand). Everything else is the
numbers from the build pipeline doing the work.

---

## 0. Sogou as the reference — what they actually do

Sogou Pinyin is the de-facto quality bar for Chinese IMEs. From
public engineering blog posts, papers, and reverse-engineering
write-ups, the techniques worth borrowing:

### 0.1 Data scale advantage

Sogou's search-engine parent gives them the web. They harvest:
- Page titles, anchor text, query logs → modern-vocabulary freq.
- 微博 stream → 网络新词 detection (a new word appears in 1M tweets
  in a day → trending → auto-promote).
- News archives → formal-register baseline.

**We can't crawl at that scale.** Practical substitutes:
- Wikipedia (zh, ja) full dumps — modern formal Chinese + JP.
- Common Crawl filtered Chinese subset (license-aware).
- Open news archives (清华新闻语料, 人民日报历年).
- jieba's bundled dict (already in inputx-pinyin).
- Leipzig Corpora Collection (already in inputx-pinyin).
- Wiktionary phrase lists.

Not the same scale, but ~70% coverage of common vocabulary, which
is the bulk of the typing flow.

### 0.2 Cell library (细胞词库) model

Sogou ships **per-domain dicts** users opt into — 医学 / 法律 /
游戏 / 编程. Each domain has its own freq table, layered over the
base. The IME blends by detected context.

For Inputx v2+, **per-user opt-in domain dicts** is the right pattern.
Bundle a few core domain libs in-binary:
- 计算机 / 互联网 (programming terminology)
- 二次元 / ACG (specific phrases)
- 学术 (academic vocabulary)

Each is a small TSV layered on top of base weights.

### 0.3 N-gram language model + maxent

Sogou's ranking uses a **maxent / log-linear** model combining
- unigram freq
- bigram context (prev word)
- trigram for longer sentences
- character n-gram for OOV
- positional priors (sentence-initial vs mid)

Their ranking score for "given input → produce output sequence":
```
score(W | input) = Σ λᵢ × featureᵢ(W, input, context)
```
where features include `log P(wᵢ | wᵢ₋₁)`, length bonus, character-
class bonus, etc.

**For us (v0.2-v0.4)** — start with unigram, layer in bigram via the
build pipeline, add positional priors at runtime. Maxent / log-linear
is a v0.5+ target requiring labeled training data we don't have yet.

### 0.4 User-behavior feedback

Sogou logs (with user consent) which candidate the user picks at
each input. Aggregated across all users, this becomes the dominant
freq signal — orders of magnitude richer than corpus freq alone.

**Our analog**: PolishLog telemetry (already shipping on macOS).
Local-only, user opts in to "submit" by sending the jsonl to us.
No cloud collection without consent. v0.3 target: a Settings →
"contribute polish data" button that uploads a sanitized hash of
the jsonl when user explicitly clicks.

### 0.5 Fuzzy / smart correction

Sogou auto-corrects: `shanhgai → 上海`, `nih → 你好`. Their fuzzy
table is huge and tuned per common typo pattern.

**Our analog** — inputx-pinyin already has a `FuzzyConfig` for the
canonical pairs (z/zh, c/ch, …). Coverage is fine for v0.2; the
typo-correction layer is v0.4+.

### 0.6 Cloud candidates

Sogou's "云候选" runs a server-side n-gram lookup for queries the
local dict can't answer. We **don't do this** — Inputx is privacy-
preserving and offline. The static DB has to be good enough.

---

## 1. The four dimensions

### 1.1 来源 — Data sources

Every weight in our final `weights.tsv` traces back to one of these.
Each source is versioned (date + checksum) so reproducibility is
guaranteed.

| Source | Type | License | Use |
|---|---|---|---|
| **Wikipedia zh dump** | full-text | CC-BY-SA | unigram + bigram freq, common vocab |
| **Wikipedia ja dump** | full-text | CC-BY-SA | JP kanji + jukugo freq |
| **jieba dict** | (word, freq) pairs | MIT | phrase segmentation baseline |
| **Leipzig zh corpus** | sentence-tokenized | CC-BY | modern-Chinese unigram |
| **Unihan database** | char-level metadata | Unicode | readings (on-yomi / pinyin), variants |
| **现代汉语常用字表** | char list | public | which chars are "common" baseline |
| **常用漢字表 (JP)** | char list | public | which kanji are in the JP base set |
| **OpenCC** | TC↔SC variant maps | Apache | TC/JP-shinjitai ↔ SC bridging |
| **KANJIDIC2** | kanji readings + glosses | EDRDG-PD | JP on/kun readings |
| **Custom — Inputx user picks** | per-user telemetry | local-only | PolishLog jsonl, opt-in upload |
| **Custom — LLM annotations** | (code, expected #1) tuples | curated | resolve ambiguous ranking |

What we *deliberately don't use*:
- Search-engine query logs (don't have access, privacy concerns).
- Cloud-fetched modern-trending words (offline IME principle).
- Proprietary dicts (Sogou's, Baidu's, etc.).

### 1.2 处理 — Processing pipeline

Each source is independently extracted, normalized, then merged.
Pipeline is in `tools/scoring/`:

```
tools/scoring/
  ├── 01_fetch/           # download corpora (versioned URLs + checksums)
  ├── 02_extract/         # corpus → (word, freq) tables per source
  ├── 03_normalize/       # cross-source freq normalization (log-rank)
  ├── 04_layer_assign/    # assign LAYER (Jianma1/2/3/Zigen/Phrase/Auto)
  ├── 05_merge/           # combine sources into unified weights.tsv
  ├── 06_llm_annotate/    # batch LLM rerank for ambiguous codes
  ├── 07_validate/        # run weights against test corpus + polish-log
  ├── 08_pack/            # weights.tsv → .dict/.fsa artifacts (inputx-fsa)
  └── README.md
```

Each step is a separate script. Resumable / cacheable. Output of
step N is the input of step N+1.

#### 03_normalize — cross-source freq

Each source has its own freq scale (Wikipedia in absolute counts,
Leipzig in normalized per-million, jieba in arbitrary integers).
We normalize each to log-rank within source, then weighted-average:

```python
score[word] = Σ_src α[src] × log_rank(word, src)
```

α weights chosen by validation against polish-log + LLM annotation
set. Default α favors Wikipedia (most diverse, modern register).

**Implementation note (CP3b, 2026-05-25):** the shipped pipeline uses
**per-source log-COUNT**, not literal log-rank:

```python
score[word] = Σ_src α[src] × ln(1 + count(word, src)) / max_ln[src]
```

with α = corpus `manifest.weight`. Rationale, decided on gate1/coverage data:
literal rank-based log-rank discards count magnitude, and on our current
*homogeneous* occurrence-count sources (subtlex/news/wiki) that collapses the
long tail — most words' freq_score trend to 0 and get cut by build_dict's
MIN_FREQ (dict 22k vs 219k entries). log-count keeps the per-source scale-free
property — the actual point of §03, which pays off for *heterogeneous* sources
(absolute count vs per-million vs arbitrary ints) — while preserving magnitude,
so coverage stays full AND gate1 improves (kaopu→靠谱 to #1). The literal rank
form stays available as `03_normalize --mode per-source-log-rank` for when
heterogeneous sources are added. `--mode sum-then-log` reproduces the CP2
byte-identical build_weights baseline (counting-chain regression guard).

#### 04_layer_assign — Wubi layer floors

Wubi has a strict structural hierarchy: Jianma1 > Jianma2 > Jianma3
> Zigen > Phrase > Auto. This is the only HARD RULE in scoring —
all other priorities are quantitative. The 04 step takes the
canonical 86-standard simcodes list (`data/jianma*.txt`) and forces
those entries into the Jianma layer with floor scores that no other
source can beat.

This is also where we *filter pollution*: when wubi phrase data
contains an entry whose 4-letter code is a high-freq pinyin word
(e.g., wubi `jixu 曳光弹` colliding with pinyin `jixu 继续`), we
demote the wubi phrase to Auto layer or remove it entirely. The
collision detection is automatic — compare wubi-Phrase codes
against the top-N pinyin readings.

#### 06_llm_annotate — see §3

#### 07_validate — quality gates (see §4 退出机制)

### 1.3 更新 — Update / versioning / distribution

Static DB → versioned data drops, not OTA.

**Schema**:
```
weights.tsv:
  # version: 2026.05.22
  # source-versions:
  #   wikipedia-zh: 20260501
  #   jieba: 0.42
  #   unihan: 15.1
  #   ...
  # built-at: 2026-05-22T08:00:00Z
  # llm-annotations: 142
  <code>\t<word>\t<layer>\t<unified_score>
```

**Distribution paths**:
1. **In-binary** (default) — `weights.tsv` baked into the `.dict` shipped
   with the Inputx.app bundle. Updated via app upgrade.
2. **Side-loadable** (future, v0.4+) — user drops a newer
   `weights.tsv` into `~/Library/Application Support/Inputx/`
   override directory. IME reads override at load. Lets us push
   data improvements without a full app release.
3. **Per-user override** — L0 pins always layer on top.

**Update cadence**: monthly for first 6 months, then quarterly. Each
release ships full pipeline run logs + diff against previous version
(which codes' top changed).

### 1.4 退出 — QA gates + rollback

A weights drop ships only if it passes ALL of:

#### Gate 1 — Regression corpus
- 70+ test rows in `tests/input_corpus.tsv` (curated golden examples).
- Each row: `<code>\t<expected_top>\t<source>\t<notes>`.
- Cases from polish-log + manual review. Expanded over time.
- 100% pass required.

#### Gate 2 — Stability budget
- Run new weights against last release's polish-log batch.
- For inputs where user picked #0 (right answer per previous):
  - Required: new weights also pick #0 for ≥95% of those inputs.
- Prevents regressions: a new corpus that introduces 5% rank flips
  on previously-right cases is rejected.

#### Gate 3 — Coverage delta
- Count of (code, word) pairs in new vs old.
- Tolerance: +∞ growth OK (new vocab welcome), -1% maximum shrinkage
  (no silent coverage loss).

#### Gate 4 — LLM judge eval
- Sample 200 random (code, top-3) tuples from new weights.
- LLM judges "is top-1 the most likely user intent given code?"
  on a 0-3 scale.
- Mean ≥ 2.3 required (calibrated against current baseline).

#### Rollback
- Each release is a single TSV file. Reverting = restore previous
  TSV from the versioned artifact store.
- In-app "Reset to factory dict" option clears overrides + L0.

---

## 2. Runtime side — unified score

Once the static DB is built, the runtime is straightforward:

```rust
struct ScoredCandidate {
    word: String,
    source: Source,    // Wubi / Pinyin / Japanese
    score: f64,        // unified, comparable across sources
}
```

Each engine produces `Vec<ScoredCandidate>` per-keystroke. Merge:

```rust
let all: Vec<ScoredCandidate> = wubi_cands.into_iter()
    .chain(pinyin_cands)
    .chain(jp_cands)
    .collect();
all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
```

### Score formula

```
score(c) = engine_mult[c.source]
        × layer_floor(c.source, c.layer)     // Wubi 简码 = 1e6 floor
        × normalized_freq(c)                  // [1, 2] from build-time
        × length_bias(c)                      // 0.9–1.05
        × (1 + l0_boost(c))                   // [1, 1.5] from user pins
```

Default constants (tunable):
```
engine_mult = { Wubi: 1.0, Pinyin: 1.0, Japanese: 0.85 }
layer_floor (Wubi) = { Jianma1: 1e6, Jianma2: 1e5, Jianma3: 1e4,
                       Zigen: 1000, Phrase: 100, Auto: 50 }
layer_floor (Pinyin) = derived from phrase length + corpus freq
layer_floor (Japanese) = { Jukugo: 1000, SingleKanji: 800, Kana: 100 }
length_bias = 1.0 for 2-3 char, 0.95 for 4 char, 0.9 for 5+
l0_boost = 0.5 × tanh(pick_count / 5)  // capped, soft promotion
```

All weights live in the engine-internal score, which is built into
the index value (already the case for wubi/pinyin via packed u64).
JP synthesizes at runtime from KanaKind.

### Wubi 简码 hard rule — the only structural override

Because we are "Inputx 五笔", Wubi Jianma1/2/3 candidates are
**guaranteed top** by virtue of `layer_floor` reaching 1e6 — no
Pinyin Phrase or JP Kanji score can mathematically beat them. This
is the brand promise encoded in score.

### Cross-engine L0 pin

Already implemented (commit `bd21f1b`): the pinned word for the
current buffer overrides natural score → position 0. Survives
unified-score merge.

---

## 3. LLM integration — offline batch annotation

LLM is **not** in the runtime hot path. It runs at build time, in
the `06_llm_annotate` pipeline step.

### When the LLM gets called

Build pipeline emits a list of **ambiguous codes** — codes where
the top-2 candidates have scores within 5% of each other. These
are the "judgment calls" the corpus alone can't settle.

Example for `jixu` (one of the ones the user flagged):
```
jixu: 继续 (score 44k), 急需 (28k), 积蓄 (26k), 亟需 (18k)
```
继续 is clear winner; not ambiguous. Skip.

For something tighter:
```
yiwei: 以为 (score 25k), 一位 (24k), 一味 (23k)
```
3-way tie. Send to LLM.

### What we send

```
prompt:
  You are calibrating a Chinese pinyin IME's candidate ranking.
  For the input "yiwei", which of these is most likely what a
  user intended?
  
  Options:
  A. 以为 (think / believe)
  B. 一位 (one [classifier])
  C. 一味 (single-mindedly)
  
  Consider modern Chinese usage frequency, register, and
  context-free typing patterns.
  
  Output JSON: {"top": "A", "confidence": 0.0-1.0, "reason": "..."}
```

### What we get back

```
{"top": "A", "confidence": 0.75, "reason": "..."}
```

Stored as an override:
```
tools/scoring/llm_overrides.tsv:
  yiwei \t 以为 \t 0.75 \t llm-claude-opus-4-7-2026-05-22 \t 以为 most common
```

### Pipeline integration

Step `06_llm_annotate`:
1. Read step-5 output (unified weights).
2. Find ambiguous codes (top-2 score gap < 5%).
3. Batch call Claude API (parallel, ~100 codes/min with caching).
4. Parse responses → `llm_overrides.tsv`.
5. Apply overrides as score boost: +50% to the LLM-preferred top.

### Reproducibility

- LLM model + prompt version pinned per pipeline run.
- All API responses cached (so re-running build is deterministic).
- Human review pass before promotion (`07_validate` shows diffs).

### Cost / scale

Estimate: 10,000 ambiguous codes × ~$0.001 per Claude call (with
prompt caching) ≈ $10 per full rebuild. Affordable.

### What LLM is *not* for

- Runtime decisions (latency, privacy, reproducibility).
- Long-form generation (this is classification, not generation).
- Replacing the corpus pipeline (LLM tunes the EDGE cases the
  corpus can't resolve — it's not the primary data source).

---

## 4. Phasing

### Phase 0 — done ✅
- Layered hard-rule merge (wubi → JP kanji → pinyin → JP kana).
- L0 pin cross-engine promotion.
- PolishLog telemetry.
- This design doc.

### Phase 1 — runtime unified score (next 1-2 days)
- Add `score: f64` to `Candidate` struct.
- Each adapter produces scored candidates.
- merge.rs sorts by score.
- Wubi simcode floor enforced via layer_floor constants.
- Tests for known cases (jixu/yama/nihon/wcng).

### Phase 2 — tools/scoring/ pipeline scaffold (1-2 days)
- Python scripts for steps 01–08.
- Initially: just re-derive existing weights, validate identity.
- Pipeline runs end-to-end with sample corpora.

### Phase 3 — first real rebuild (1 week)
- Run pipeline with full corpora.
- Diff against shipped weights.
- Manual review of top-100 changes.
- Ship v2026.05.x with rebuilt weights.

### Phase 4 — LLM annotation (1 week)
- Implement `06_llm_annotate` step.
- Run on top-5k ambiguous codes.
- Manual review of LLM outputs (sample 100).
- Ship v2026.06.x with LLM-tuned weights.

### Phase 5 — telemetry-driven tuning (ongoing)
- Aggregate PolishLog (opt-in upload).
- Each release ships a "fixes from your reports" section in
  release notes.

### Phase 6 — cell libraries (v0.4+)
- Bundle 3-4 domain dicts (computing, ACG, academic).
- Settings → "Enable cell libraries" toggle.

### Phase 7 — bigram (v0.5+)
- Train bigram on Wikipedia.
- Runtime: context-aware ranking using prev-committed word.

---

## 5. Polish-log corpus as ground truth

The user-collected polish-log entries become **the** test corpus
over time. Each non-#0 pick is a signal: "the IME's #1 was wrong
for this input, the user picked X instead".

Aggregation: codes with the most repeat picks (say, 5+ users all
pick the same non-#0 word for the same code) graduate into
`tests/input_corpus.tsv` as required-pass cases.

This closes the loop: real-world misses → curated corpus → fix in
next pipeline run → regression test pinned.

---

## Decisions explicitly deferred to v0.5+

These were considered and intentionally *not* in scope for v0.2-v0.4:

- **N-gram bigram/trigram modeling.** Adds latency + memory + data
  pipeline complexity. Unigram + corpus coverage gets us 80% of
  the way there; bigram is the last 20% and not worth it yet.
- **Auto-discovery of new compound words from user typing.** Risk
  of muscle-memory mistakes becoming permanent dict entries.
- **Cloud sync of L0 pins.** Privacy / offline-IME principle.
- **Predictive sentence-level autocomplete.** Different IME genre.
- **Speech input.** Out of scope.

---

## See also

- `tests/input_corpus.tsv` — regression test cases (lives in repo).
- `mac/Sources/PolishLog.swift` — user-facing telemetry impl.
- `tools/scoring/README.md` — pipeline operator's guide.
- `core/crates/inputx-wubi/src/layer.rs` — wubi layer base values.
- `core/crates/inputx-core/src/composite/merge.rs` — runtime merge.
