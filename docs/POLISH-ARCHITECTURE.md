# Polish architecture — design target for v1.10 → v1.12 (pre-v2.0)

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

### Hack point 1 — `dispatch.rs` `const`-tier scoring magic

```text
core/crates/inputx-core/src/composite/dispatch.rs
  const RARE_CHAR_DEMOTE: f64 = 0.001    (was 0.3 pre-v1.9.0)
  const CHAR_PROMINENT_FLOOR: u64 = 20_000
  let auto_demote = match pinyin_len { 1 => 0.01, 2 => 0.05,
                                       3 => 0.10, _ => 0.20 };
  let phrase_mult = if pinyin_intent {
      if full_code { LIKELIHOOD_WUBI_FULL_CODE_PROMOTE } else { 0.5 };
  };
  let single_promote = if full_code && is_single && raw_freq > max_phrase_freq
                       { 100.0 } else { 1.0 };
```

Every one of these is a polish knob that disguises as code. Touching
them requires editing `dispatch.rs` — the v1.9.0 corpus-merge
regression chain ended up doing exactly this (RARE_CHAR_DEMOTE 0.3 →
0.001 in commit dfc46c5, since reverted) because there was no clean
"调评分" surface for the relevant axis.

**Target**: every numeric constant in dispatch.rs that influences
ranking moves to `EngineWeights`. Code only computes; configuration
lives in one struct.

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

### v1.10 — Promote dispatch.rs hardcodes to EngineWeights

**Goal**: every numeric constant in `dispatch.rs` that influences
ranking lives in `EngineWeights`. After this cycle a polish that
adjusts ranking on any axis touches `inputx_default()` exclusively
— no dispatch.rs edits.

| WU | Move | Knob name |
|---|---|---|
| α | `RARE_CHAR_DEMOTE` | `wubi_rare_char_demote_q4` (default = ln(0.001)·16 ≈ -110) |
| β | `CHAR_PROMINENT_FLOOR` | `wubi_char_prominent_floor_freq` (default 20_000) |
| γ | `auto_demote` (per-len table) | `wubi_auto_layer_demote_q4: [i32; 4]` (per pinyin_len 1..4) |
| δ | `phrase_mult` (speculative 0.5 / full_code promote) | `wubi_phrase_speculative_demote_q4` + reuse `LIKELIHOOD_WUBI_FULL_CODE_PROMOTE` |
| ε | `single_promote` 100.0 (single-char Jianma1 hint) | `wubi_single_char_full_code_promote_q4` |

Estimated: **2-3 days**. Mostly mechanical (add field, set default,
replace `const` with `weights.foo`, verify baseline byte-identical).

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
