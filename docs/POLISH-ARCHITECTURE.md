# Polish architecture — design target for v1.10 → v1.12 (pre-v2.0)

> **Implementation status 2026-06-01**:
> - v1.10 ✅ shipped (tag `v1.10.0`, commit `ecae7cb`) — single-TOML for 30+ ranking 公式
> - v1.11 ✅ shipped (tag `v1.11.0`, commit `9fd3bbf`) — Makefile + polish-cli + dict-sync-trap fix
> - v1.12 ✅ framework shipped (commit pending) — telemetry-driven calibrate framework; real-data calibration runs require polish-log accumulation
>
> The v2.0-前 终态 per user 2026-06-01 directive is now achieved: every
> ranking polish action is either (a) a TOML / TSV diff, (b) a
> polish-cli invocation, or (c) a calibrate-tool batch — none require
> Rust source edits.

> **User directive 2026-06-01**: "我希望之后就可以安心人工 polish，是真正的
> 会调整语料/评分而不是 hack patch 了".
>
> v1.7 + v1.8 + v1.9 brought the framework: 9 EngineWeights knobs +
> 5-corpus build_weights stack + overlay TSVs (quickfix_boost.tsv +
> pinyin_modern_v1.tsv). But the polish surface is still half-paved:
> several scoring-critical constants live as hardcoded `const` blocks
> in `composite/dispatch.rs` and `composite/merge.rs`, and the polish-
> log → overlay loop is still manual. This doc inventories what's
> still hack-patchable and lays out the path to a corpus/weight-only
> polish workflow.

---

## What "正统 polish" means

A polish action is **clean** when it touches one of:

1. **`corpus/manifest.toml`** — adjust a corpus's mixing `weight`,
   add/remove a source, swap a license-compatible alternative.
2. **`weights/weights.tsv`** — the regenerated output of
   `pinyin-build-weights` / `wubi-build-weights`. Never edit by
   hand; edit upstream and rebuild.
3. **Overlay TSVs** — `tools/scoring/data/polish_reports/quickfix_boost.tsv`,
   `tools/scoring/data/supplemental/pinyin_modern_v1.tsv`,
   `core/crates/inputx-wubi/data/phrases.txt`. These are *data*; the
   build picks them up via MAX overlay semantics.
4. **`EngineWeights::inputx_default()`** — the 9 calibration knobs
   in `core/crates/inputx-scoring/src/lib.rs`. Hand-tuning is fine
   *as long as a value change here is the entire fix* — no
   accompanying special-case code in dispatch/merge.

A polish action is **hack** when it touches one of:

- **`composite/dispatch.rs`** — adding a `const`, an if-branch, a new
  per-buffer rule.
- **`composite/merge.rs`** — adding a per-source override, special-
  case demote, runtime blacklist.
- **`composite/scoring.rs`** — adding a per-engine multiplier or
  proximity-decay table (this *is* policy, but it's structured —
  belongs in EngineWeights or `derive_log_likelihood` extensions).

The bug fixed at `309de56` (lookup_lixiang) is the canonical clean
case: a stale TSV row that nobody noticed, removed without any code
change. This doc's goal is **make every polish look like that**.

---

## Current hack surface (the polish gap)

### Hack point 1 — **~30 ranking-magic constants scattered across `composite/*.rs`**

The polish-relevant numeric constants today live in **four Rust
source files** (`dispatch.rs` + `pinyin_adapter.rs` + `scoring.rs` +
`merge.rs`). Each one was hand-picked by trial-and-error and bound
into the Rust source — touching any of them requires `cargo build`
and a Rust edit. Per the user 2026-06-01 directive these are "公式"
= data, not algorithm.

Full inventory (auditable via `grep -nE "const [A-Z_]+: f64|i32|u64" core/crates/inputx-core/src/composite/*.rs`):

```text
composite/dispatch.rs                   (per-buffer + per-layer policy)
  CHAR_PROMINENT_FLOOR        20_000     freq threshold below = rare char
  RARE_CHAR_DEMOTE            0.001      linear discount on rare Jianma2/3
  auto_demote                 [0.01/0.05/0.10/0.20 by pinyin_len 1-4]
  phrase_mult                 0.5 (speculative) | FULL_CODE_PROMOTE
  single_promote              100.0 (full_code single-char Jianma1 hint)

composite/pinyin_adapter.rs             (per-path tier floors)
  PINYIN_PHRASE_BASE          400_000    Path-1 exact base
  L0_PIN_MULTIPLIER           1000       user-pinned boost
  NON_EXACT_FLOOR             1000       degenerate-tier base
  COMPOSED_SCORE              500_000    long-buffer Viterbi sentence
  COMPOSED_FALLBACK_SCORE     250_000    short-buffer Path-5 last-resort
  FUZZY_BASE                  350_000    pre-discount fuzzy floor
  FUZZY_DISCOUNT              0.3        multiplier on FUZZY_BASE
  COMPOSED_QUALITY_FLOOR      -15_000    Viterbi log-quality cutoff

composite/scoring.rs                    (per-engine + per-match-shape policy)
  LIKELIHOOD_JP_JUKUGO_BASE        200_000     jukugo dict tier
  LIKELIHOOD_JP_SINGLE_KANJI_BASE  100_000     single-kanji dict tier
  LIKELIHOOD_JP_HIRAGANA_BASE      150_000     kana fallback tier
  LIKELIHOOD_JP_KATAKANA_BASE      110_000     kana fallback tier
  LIKELIHOOD_JP_COMPOSED_BASE      130_000     JP Viterbi compose tier
  LIKELIHOOD_JP_COMPOSED_KANJI_BASE 280_000    JP compose pure-kanji bonus
  LIKELIHOOD_JP_FULL_MATCH_PROMOTE 1.3         exact-buffer JP boost
  LIKELIHOOD_PINYIN_PREDICT_BASE   180_000     CP-B prefix-prediction tier
  LIKELIHOOD_WUBI_PREDICT_BASE     50_000      CP-C wubi-prediction tier
  LIKELIHOOD_WUBI_FULL_CODE_PROMOTE 1.1        4-letter-buffer wubi boost
  LIKELIHOOD_WUBI_SINGLE_CHAR_PROMOTE_MULT 100 single-char Jianma1 boost
  LIKELIHOOD_TC_DEMOTE_MULT        1e-3        traditional-char demote
  LIKELIHOOD_ENGINE_MULT_{WUBI,PINYIN,JP} 1.0  legacy per-engine knobs
  LIKELIHOOD_PREDICT_PROXIMITY_K   3.0         prefix decay exponent K
  PRIOR_FREQ_MULT_JP               3000        JP freq-to-prior conversion
  PRIOR_FREQ_MULT_{PINYIN,WUBI}    1.0         zh freq-to-prior conversion

composite/merge.rs                      (cross-engine merge policy)
  TC_DEMOTE_FULL              3549-char OpenCC marker set (data file)
  contains_demote_tc(w)       hand-rolled membership test
                              → multiplier = LIKELIHOOD_TC_DEMOTE_MULT
```

Add to these the **already-in-EngineWeights 9 knobs** (engine_boost /
simcode_boost / bootstrap_floor / char_boost / word_len_bonus /
fuzzy_floor / initials_base / viterbi_decay / + the v1.7 anchors)
and you have **~30 numeric polish-axis values**. v1.10's job is to
make them a **single data asset**, not Rust source.

The v1.9.0 corpus-merge regression chain (RARE_CHAR_DEMOTE 0.3 →
0.001 in commit dfc46c5, since reverted) proved the cost of this
arrangement concretely — there was no clean "调评分" surface so the
fix had to be a `dispatch.rs` edit.

**Target**: a **single TOML file** —
`core/crates/inputx-scoring/data/engine_weights.toml` — holds **every**
ranking-magic numeric constant from the four source files above.
Rust code only reads from this TOML (build-time via `build.rs` →
generates `inputx_default()`); users polish by editing the TOML and
running `make polish-rebuild`. No `cargo build` needed for a polish
action.

The TOML structure mirrors the per-file decomposition above (sections
for `dispatch` / `pinyin_path` / `scoring_bases` / `merge`) so users
can find a constant by scope. Every entry is one row with a default,
a comment explaining what it influences, and a permissible range.

### Hack point 2 — `merge.rs` per-source overrides

```text
core/crates/inputx-core/src/composite/merge.rs
  const TC_DEMOTE_FULL: &str = include_str!("...tc_chars_demote.txt");
  fn contains_demote_tc(word) → bool (3549-char OpenCC set)
  let demote = if contains_demote_tc(w) { LIKELIHOOD_TC_DEMOTE_MULT }
```

TC demote *is* corpus-shaped (the 3549-char marker set is data),
but it's wired through hand-rolled code paths. If we ever want to
demote on a different orthographic axis (e.g. archaic Han variants,
JP shinjitai showing up in zh-only mode), we re-touch merge.rs.

**Target**: `merge.rs` only consumes a generic per-source weight
adjustment, never hardcodes a class. The TC table moves to data;
the demote magnitude moves to EngineWeights.

### Hack point 3 — Manual overlay TSV editing

```text
tools/scoring/data/polish_reports/quickfix_boost.tsv      (36 entries)
tools/scoring/data/supplemental/pinyin_modern_v1.tsv      (512 entries)
core/crates/inputx-wubi/data/phrases.txt                  (61210 entries)
```

These ARE data. But adding a row today means:
- Open the editor
- Manually compute the wubi-86 phrase code (for phrases.txt)
- Pick a freq boost magnitude by guess (50000? 100000?)
- Run `pinyin-build-dict` / `wubi-build-weights` / `idf-from-*`
- Run baseline tests
- Sync `data-core/data/pinyin.dict` (a sync step easy to forget —
  caused the v1.9.0 WU-π.c re-test confusion)

**Target**: a single CLI (`inputx-polish-log` or similar) that takes
"user picked 理想 at lixiang, not 立项" and:
- Detects the existing `weights.tsv` ordering
- Generates the minimum-magnitude boost to flip the ranking
- Writes it to the appropriate overlay TSV
- Triggers rebuild + verify
- Reports success/failure

### Hack point 4 — Cherry-pick from corpus audit

After a corpus diff audit (the v1.9.0 WU-π.b pipeline), the audit
log identifies 20+ candidate entries to add to wubi `phrases.txt`.
Today this requires manual wubi-86 code computation + dict-presence
verification per entry — slow, error-prone, no automation.

**Target**: `cargo run --bin cherry-pick-audit -- --diff
zh-opensubtitles-vs-wubi/new_entries.tsv --top 50` writes 50 verified
candidate rows to `phrases.txt` with correct codes, ready to commit.

### Hack point 5 — EngineWeights calibration is hand-tuned

```text
core/crates/inputx-scoring/src/lib.rs::EngineWeights::inputx_default()
  engine_boost_q4: [8, 0, -100]    // wubi / pinyin / japanese
  simcode_boost_q4: 0
  bootstrap_floor_q4: 0
  char_boost_q4: 0
  word_len_bonus_q4: 0
  fuzzy_likelihood_floor_q4: 205
  initials_likelihood_base_q4: 221
  viterbi_link_decay_q4: -6
```

Each value was picked by running baseline tests at various candidate
values and picking the one that passes (see v1.7.4 megachange commit
message + v1.8.0/1/2 commit messages). Manual binary search by Claude
under user supervision. Works for the 9 knobs we have today, won't
scale to 20+ knobs and won't reflect real user data.

**Target**: telemetry-driven calibration. polish-log (user picks at
each session) → fit EngineWeights values that minimize "user
re-picked a non-#0 candidate" frequency. Closed-loop, no manual
binary search.

---

## Design target — what the polish workflow looks like at v2.0

```
┌─────────────────────────────────────────────────────────┐
│ User types in IME → picks candidate                     │
│                                                          │
│  ↓ polish-log (already exists, drives quickfix_boost)   │
│                                                          │
│ tools/scoring/aggregate_polish_log.py                   │
│   ──→ overlay TSV deltas                                │
│   ──→ corpus weight tweaks (rare)                       │
│   ──→ EngineWeights knob proposals (sometimes)          │
│                                                          │
│ ↓ rebuild via `make polish-rebuild` (single command)    │
│                                                          │
│ ↓ baseline regression test (auto-run)                   │
│                                                          │
│ ↓ if pass → commit + push (or queue for user review)    │
│ ↓ if fail → bisect + report the offending overlay row   │
└─────────────────────────────────────────────────────────┘
```

Everything between user-pick and shipped-dict is automated. User
intervention only when:
- A polish-log proposal fails baseline regression (needs human
  judgment to reconcile signal-vs-design conflict).
- A new knob category is needed (signal that none of the 9 existing
  axes captures the polish target — design work).

---

## Path to the target — sub-cycle plan

### v1.10 — Collect all ranking "公式" into a single TOML data file

**Goal**: every numeric constant in `composite/{dispatch,
pinyin_adapter, scoring, merge}.rs` that affects ranking lives in
**one TOML file** loaded at build time. Rust code only consumes;
configuration lives in data. After this cycle a polish that adjusts
"公式" touches `engine_weights.toml` exclusively — no Rust edits.

#### Target file layout

`core/crates/inputx-scoring/data/engine_weights.toml`:

```toml
# Inputx ranking formula — single source of truth. Edit, rebuild
# (`make polish-rebuild`), run baseline. Never touch Rust to adjust
# these.

[engine_weights]
# v1.7-v1.8 calibration knobs (existing in inputx_default()).
engine_boost_q4 = [8, 0, -100]
simcode_boost_q4 = 0
bootstrap_floor_q4 = 0
char_boost_q4 = 0
word_len_bonus_q4 = 0
fuzzy_likelihood_floor_q4 = 205
initials_likelihood_base_q4 = 221
viterbi_link_decay_q4 = -6

[dispatch.wubi]
char_prominent_floor_freq = 20_000
rare_char_demote = 0.001
auto_layer_demote = [0.01, 0.05, 0.10, 0.20]   # per pinyin_len 1..4
phrase_speculative_demote = 0.5
full_code_single_char_promote = 100.0

[pinyin_path]
phrase_base = 400_000
l0_pin_multiplier = 1000
non_exact_floor = 1000
composed_score = 500_000
composed_fallback_score = 250_000
fuzzy_base = 350_000
fuzzy_discount = 0.3
composed_quality_floor = -15_000

[scoring.jp]
jukugo_base = 200_000
single_kanji_base = 100_000
hiragana_base = 150_000
katakana_base = 110_000
composed_base = 130_000
composed_kanji_base = 280_000
full_match_promote = 1.3
prior_freq_mult = 3000

[scoring.match_shape]
predict_proximity_k = 3.0           # prefix-completion decay exponent
predict_base_pinyin = 180_000
predict_base_wubi = 50_000

[scoring.cross_engine]
wubi_full_code_promote = 1.1
wubi_single_char_promote_mult = 100.0
tc_demote_mult = 1e-3
# Per-engine legacy multipliers; usually 1.0
engine_mult_wubi = 1.0
engine_mult_pinyin = 1.0
engine_mult_jp = 1.0
```

#### Wire path

1. **`inputx-scoring` build.rs**: read `data/engine_weights.toml`
   at build time, emit `engine_weights_generated.rs` with
   `pub const fn inputx_default() -> EngineWeights` + a parallel
   `pub mod scoring_consts { pub const ... }` module of the per-
   formula values. `cargo:rerun-if-changed` watches the TOML.
2. **`inputx-scoring` `src/lib.rs`**: `include!` the generated
   constants, expose them as `pub use`. EngineWeights schema gains
   ~20 new fields (the ranking-magic constants previously in
   `composite/*.rs`).
3. **`composite/{dispatch,pinyin_adapter,scoring,merge}.rs`**:
   replace every `const FOO: f64 = 0.3;` with
   `inputx_scoring::scoring_consts::FOO` (or
   `weights.dispatch_wubi_rare_char_demote` for EngineWeights-routed
   knobs). Baseline must stay byte-identical to commit `309de56`.
4. **Polish workflow** post-v1.10: editing `engine_weights.toml`
   + `make polish-rebuild` is sufficient for any single-value
   ranking-formula change. No `git diff` shows up in `composite/*.rs`.

#### WU plan

| WU | Scope | Risk |
|---|---|---|
| α | scaffold: build.rs + engine_weights.toml + generated module | low (no behavior change yet) |
| β | move `composite/scoring.rs` consts to TOML | medium (consumers in 3 adapter files) |
| γ | move `composite/pinyin_adapter.rs` per-path floors | medium |
| δ | move `composite/dispatch.rs` per-layer policy | medium (RARE_CHAR_DEMOTE etc.) |
| ε | move `composite/merge.rs` TC demote magnitude | low |
| ζ | verify byte-identical baseline + lib (gate before commit) | gate |

Estimated: **3-4 days** (was 2-3 — bigger scope than the original
dispatch-only plan). All WU steps preserve baseline by construction
since defaults match current consts exactly.

#### Post-v1.10 polish workflow examples (concrete)

To verify the design intent matches the user 2026-06-01 directive
("调语料 / 评分 / 公式 …这些纯数据资产"), here are the 4 polish-
action classes and what each looks like post-v1.10:

**A. "Fix this ranking" (single buffer mis-rank)**
```
# Mac-IME-用户 reports: `lixiang` should give 理想 not 立项
$ inputx-polish pick lixiang 理想 --reason "user-report-2026-06-15"
[polish] current ranking: 立项 #0, 理想 #1
[polish] writing quickfix_boost.tsv:
           lixiang  理想  +12000   (minimum boost to flip)
[polish] running make polish-rebuild ... ok
[polish] running baseline tests ... ok (24/24, 288/0)
[polish] committed: "polish(pinyin): lixiang → 理想 (user 2026-06-15)"
```
Nothing in Rust source changes. The fix is 1 TSV row.

**B. "Fix a formula value" (calibration)**
```
# Audit shows wubi 简码 was outranking pinyin top for some short
# buffers. Bump simcode_boost slightly.

# Open engine_weights.toml, change one line:
-  simcode_boost_q4 = 0
+  simcode_boost_q4 = 4         # +0.25 nat = ~×1.3 linear

$ make polish-rebuild
[polish] regen .idf ... ok
[polish] running baseline tests ... ok (24/24, 288/0)
$ git commit -am "polish(weights): simcode_boost +4 for short-buffer wubi"
```
Nothing in Rust changes. Polish is a TOML diff + a baseline pass.

**C. "Fix a corpus mix"**
```
# Modern social-media slang is missing. Add chat-style corpus.
# Edit core/crates/inputx-pinyin/data/corpus/manifest.toml:

+  [corpus.zho_weibo_v2026]
+  url = "..."
+  sha256 = "..."
+  weight = 2.0   # half-weight of colloquial stack — minor signal
+  format = "frequency_list"
+  license = "MIT"

$ cargo run --bin pinyin-fetch-corpus
$ make polish-rebuild
[polish] running baseline tests ... ok (24/24, 288/0)
```

**D. "Fix a missing word" (dict gap)**
```
# Wubi dict missing 不是. Audit log identified this.
$ inputx-polish add-phrase 不是 --engine wubi
[polish] wubi-86 encoder: 不是 → code 'gijg'
[polish] adding to phrases.txt
[polish] running make polish-rebuild ... ok
[polish] running baseline tests ... ok (24/24, 288/0)
[polish] committed: "polish(wubi): add phrase 不是 → gijg"
```

**What user NEVER does post-v1.10**:
- Edit `composite/dispatch.rs` / `merge.rs` / `scoring.rs`
- Manually compute a boost magnitude
- Manually compute a wubi-86 phrase code
- Sync `pinyin-data-core/data/pinyin.dict` separately

If a polish needs Rust changes, that's a signal the framework is
missing a knob — file an architecture bug, don't write the hack.

### v1.11 — polish-log → overlay automation

**Goal**: `inputx-polish-cli pick <buffer> <chosen> [reason]` writes
the appropriate overlay row, rebuilds, runs baseline, commits if
green. Replace the current manual editor workflow.

WU plan:
1. **WU-α**: factor out the build-dict + idf-regen + data-core-sync
   chain into a single `make polish-rebuild` target.
2. **WU-β**: write `tools/polish-cli/` that:
   - Reads user pick from CLI or polish-log JSON
   - Detects current ranking (via `inputx-probe`)
   - Computes minimum boost magnitude to flip ranking
   - Writes the row to the right overlay TSV (quickfix_boost.tsv
     for adjustments, modern_v1.tsv for new high-freq compounds,
     phrases.txt for new wubi phrases via the new auto-encoder)
   - Runs `make polish-rebuild`
   - Runs baseline tests
   - Reports + commits
3. **WU-γ**: wubi-86 phrase encoder reusable from
   `inputx-wubi/tools/import_phrases.rs` exposed as a library
   function so `polish-cli` can call it directly.
4. **WU-δ**: re-sync mechanism for `inputx-pinyin-data-core/data/pinyin.dict`
   — either auto-sync at every dict-rebuild, or remove the dual-file
   setup entirely (probably the latter — `inputx-pinyin-data-core`
   should `include_bytes!` the sibling `inputx-pinyin/data/pinyin.dict`
   directly).

Estimated: **3-5 days**. polish-cli is a non-trivial program.

### v1.12 — telemetry-driven EngineWeights calibration

**Goal**: from the user polish-log accumulated over weeks of use, fit
the 9 EngineWeights knobs (+ any v1.10 additions, so likely ~14 knobs)
to minimize "user re-picked non-#0" frequency. Replace hand-tune with
data-tune.

WU plan:
1. **WU-α**: define the loss function. Each polish-log entry is
   `(buffer, top_candidate_at_pick_time, chosen_candidate, dwell_ms)`.
   Loss = Σ (1 if chosen ≠ top else 0) — or richer if we want to
   weight by dwell time.
2. **WU-β**: gradient descent (or random search at first — the search
   space is small, ~14 dims, all bounded) over EngineWeights values.
   Run against the polish-log corpus + the baseline-test fixture as
   a regularizer ("don't drift baseline by more than N").
3. **WU-γ**: ship the calibration as a recurring offline batch
   (weekly?) rather than continuously. Produces a candidate
   `EngineWeights::inputx_default()` value set; user reviews diff
   before committing.

Estimated: **5-8 days**. Requires the polish-log to have accumulated
enough data (probably 4+ weeks of user IME usage). Algorithm choice
+ regularization tuning is the substantive work.

### v1.13+ (or v2.0) — embedding/transformer LM

Out of scope for the v1.10-v1.12 polish-workflow track. The current
n-gram + EngineWeights stack should sustain through this whole track;
LM upgrade is a separate "phase 4" architectural shift (per
`PLAN-probabilistic-model.md`).

---

## What stays the same

- The 5-corpus mix (`zho_subtlex_ch_wf` + `zho_news_2020_100k` +
  `zho_wikipedia_2018_1m` + `zho_lccc_base` + the future
  `zho_opensubtitles_v2018` once corpus-merge calibration is solved).
- Build pipeline (`pinyin-build-weights` → `pinyin-build-dict` →
  `idf-from-pinyin-dict` → ship).
- 9 (or 14 post-v1.10) EngineWeights knobs as the calibration axis.
- Per-source overlay TSVs (quickfix_boost, modern_v1, phrases.txt)
  as the targeted-fix surface.
- `inputx-probe` as the ranking-diagnostic tool — needed for the
  polish-cli's "before/after" check.

## What goes away

- `RARE_CHAR_DEMOTE` / `CHAR_PROMINENT_FLOOR` / `auto_demote` /
  `phrase_mult` consts in `dispatch.rs`. Replaced by EngineWeights
  fields (v1.10).
- Manual TSV editing for polish-log-driven fixes. Replaced by
  `polish-cli` (v1.11).
- Hand-binary-search calibration of EngineWeights values. Replaced
  by polish-log-driven optimizer (v1.12).
- The `inputx-pinyin/data/pinyin.dict` ↔
  `inputx-pinyin-data-core/data/pinyin.dict` dual-file sync trap.
  Replaced by single source of truth (v1.11).
