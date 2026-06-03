# Ranking model invariants

> **Status:** load-bearing. This file is the contract every polish,
> framework change, and review answers to. User-stated permanent
> directive 2026-06-03. Linked from [polish skill bedrock](skills/polish/SKILL.md#bedrock-rule).

---

## 1. The canonical model — 10-tier × 3-engine matrix

There is exactly ONE priority dimension at runtime: the **(tier, engine)
matrix**. Every candidate gets assigned a `(tier_id ∈ 0..=9, engine_id
∈ {wubi=2, pinyin=1, nihongo=0})` pair at producer time, plus a
within-tier likelihood score. The merge does strict lexicographic
sort by:

1. **`tier_id`** — smaller = higher priority. 0 = absolute (pin /
   structural top), 1 = top, …, 9 = longtail. Tier boundaries are
   inviolable: a tier-N candidate never loses to a tier-(N+1)
   candidate regardless of within-axis values.

2. **`engine_id`** — wubi > pinyin > nihongo. Within a tier the
   engine offset is large enough (`engine_gap_q4 > within_tier_max_q4`,
   currently 110 > 100) that NO within-tier likelihood variance can
   flip engine order. "Inputx 五笔-first" lives here, not in
   per-entry code.

3. **within-tier axes** (`log_prior_q4 + log_likelihood_q4 +
   length_weight`, clamped to `[0, within_tier_max_q4]`) — orders
   candidates *within the same (tier, engine) cell*.

Tier names (engine_weights.toml `tier_names`):
```
0  absolute      pin / structural top (user assertion only)
1  top           wubi prominent simcode / pinyin top single char / JP basic kana
2  phrase        full-buffer phrase match
3  common        mid-freq character / common kanji
4  standard      standard dict entries
5  less_common   lower-freq / rare-CJK simcode
6  rare          rare characters
7  specialty     prefix predictions
8  predict       speculative / longer-tail prediction
9  longtail      fuzzy / initials / last-resort
```

**Tier 0 is reserved for explicit user assertions.** Anything else
that wants "top priority" lives at tier 1 and rides the engine offset.

## 2. STRICT: no special lists, anywhere

**Anti-pattern (forbidden):** per-entry hardcoded arrays / if-branches
/ carve-out lookups in Rust source that bypass the matrix model.

Example shapes that are NOT allowed in any commit:

```rust
const BAKED_EXCLUSIONS: &[(&str, &str)] = &[(...), (...)];          // ✗
const PRIOR_CORRECTIONS: &[(&str, i32)] = &[(...), (...)];          // ✗
if (buffer, word) == ("yi", "就") { score *= 1000.0; }              // ✗
let protect_list = &["就", "民", "长", ...];                         // ✗
fn is_protected(buf: &str, w: &str) -> bool { matches!(... ) }      // ✗
```

This rule is permanent. It applies regardless of:
- Whether I proposed the pattern
- Whether the user explicitly requests it
- Whether "just this one entry" feels harmless
- Whether the deadline is tight

If a polish need can't be expressed within the matrix model + the
data surface (Section 3), the conclusion is **the framework needs a
new structural rule** — never a per-entry carve-out. Open the
question with the user before touching `composite/*.rs`.

### What "special list" actually means

A list is "special" when it encodes **per-(buffer, word) data** that
overrides the natural ranking. The distinguishing test:

| Pattern | Special? | Why |
|---|---|---|
| `match layer { Jianma2 => 1, Auto => 4, ... }` | ✗ no | Layer→tier mapping is a STRUCTURAL rule of the wubi engine model. |
| `if pinyin_dict.char_max_freq(c) >= 20_000 { tier 1 } else { tier 5 }` | ✗ no | Threshold rule applied uniformly by a freq signal. |
| `const NOISE_BUFFERS: &[&str] = &["di", "le", ...]` | ✓ yes | Per-entry list with no structural basis. → exclusions_v1.tsv |
| `'z'` carve-out for wubi standalone | ✗ no | Encodes a language property of wubi 86 itself. |
| `l/n + ü` pinyin morphology aliasing | ✗ no | Encodes a property of pinyin spelling rules. |
| `("yi", "就", boost)` row in Rust | ✓ yes | Per-(buffer, word) data. → tier_overlay.tsv / quickfix_boost.tsv |

Line: if it's a `(buffer, word)` tuple or a `(word, value)` tuple
that exists because polish-log evidence demanded it, it's data.
If it encodes a structural property of the input system itself
(spelling, layer model, IME mode), it's framework.

## 3. The data surface — where per-entry polish lives

Six TSV files, all under `tools/scoring/data/`, all read at build
time via `include_str!` so cargo tracks them as build deps. The
`make polish-rebuild` chain picks up changes automatically; no
manual recompile.

| File | Shape | Effect |
|---|---|---|
| `exclusions_v1.tsv` | `<code>\t<word>[\t# why]` | Dict skips this pair at IDF build. Use to drop jieba sub-word artifacts, archaic 异读 pollution, etc. |
| `additions_v1.tsv` | `<code>\t<word>\t<freq>[\t# why]` | Dict injects this pair at IDF build. Use for modern slang / new words absent from upstream jieba. |
| `prior_corrections_v1.tsv` | `<word>\t<boost_q4>[\t# why]` | Word-level Q4 log-prior boost applied to every (code, word) entry. Use when corpus systematically underweights a word across all its readings. |
| `polish_reports/tier_overlay.tsv` | `<buffer>\t<word>\t<tier>[\t# why]` | Override the producer's natural tier for this (buffer, word). Use to demote `(di, 砂)` to tier 5 or promote a polish-log validated `(juti, 具体)` to tier 0. |
| `polish_reports/quickfix_boost.tsv` | `<buffer>\t<word>\t<freq>[\t# why]` | MAX overlay on raw_freq for (buffer, word). Use to bump a candidate above a peer within the same tier without changing the tier. |
| `supplemental/pinyin_modern_v1.tsv` | corpus-style additions consumed by `pinyin-build-weights` | Modern vocab the upstream corpus misses. Affects weights regeneration, not just IDF. |
| `core/crates/inputx-wubi/data/phrases.txt` | `<code>\t<phrase>` | Wubi phrase table — the wubi-side analog of `additions_v1.tsv`. Build-embedded into `WubiDict::embedded()`. |
| `core/crates/inputx-wubi/data/weights/weights.tsv` | `<code>\t<word>\t<layer>\t<raw_freq>` | Wubi dict freq table. Demote a rare-CJK simcode by setting raw_freq to 0 (the `(hang, 虛)` etc. polish pattern). |

There is no "rare seventh overlay" file for emergencies. If you find
yourself wanting to create one, the framework is missing a structural
knob and the fix lives in the engine_weights toml or the producer's
natural-tier logic — not in a new per-entry list.

## 4. How a polish maps to the matrix — worked examples

### Example A — "yi top1 应该是 就，不是 以"
User's mental model: 就 is wubi muscle memory.

Mechanism: `就` is a jianma2 with code "yi"; producer assigns tier 1
(prominent simcode). `以` is a pinyin top-freq single-char; producer
also assigns tier 1. Both at tier 1 → engine offset decides → wubi
就 wins. ✓ Matrix handles it. No data needed.

If 以's bigram boost (from a prev_committed context) could flip the
within-tier order, that's the framework being wrong about
`engine_gap_q4` — fix the toml constant, not add `("yi", "就")` to a
list. (This is the 2026-06-03 fix: `engine_gap_q4: 30 → 110`.)

### Example B — "shi 肯定不能是 椒，要是 是"
Mechanism: 椒 is a jianma3 with code "shi". Producer's natural rule:
`char_max_freq(椒) = 33730 >= CHAR_PROMINENT_FLOOR (20000)` → tier 1.
But user typing "shi" wants pinyin 是 (top freq single-char, also
tier 1) — and 椒 wins by engine offset.

Data fix: `tier_overlay.tsv` row `shi\t椒\t5` overrides 椒's
natural tier to 5. Now `是` (tier 1 pinyin) beats `椒` (tier 5 wubi)
via strict tier ordering. ✓

Note we do NOT add 椒 to a `const SHI_DEMOTE_LIST = ...` array.
The override is data, version-controlled with the repo.

### Example C — "jile 极了 不该出现，寄了 应该能拼出来"
Mechanism: 极了 leaks from compound-bleed in jieba corpus (好极了 →
extracted as 好+极了). 寄了 is internet slang absent from upstream.

Data fix:
- `exclusions_v1.tsv` row `jile\t极了` → IDF skip
- `additions_v1.tsv` row `jile\t寄了\t500` → IDF inject

Both data. No Rust code edited. ✓

### Example D — "rang top1 应该是 让，不是 拒"
Mechanism: 拒 is wubi jianma2 at code "rang", `char_max_freq(拒) ≈
30k > floor` → tier 1, beats pinyin 让 via engine offset.

Data fix: edit `core/crates/inputx-wubi/data/weights/weights.tsv`
row `rang\t拒\t0\t0` (set raw_freq to 0). The wubi build picks up
the change; 拒 sinks within tier 1 (since its likelihood collapses)
and 让 wins via the pinyin-side natural rule.

Alternative if multiple readings of 拒 are at issue: `tier_overlay.tsv`
row `rang\t拒\t5`. Tier 5 wubi loses to tier 1 pinyin 让. ✓

## 5. Tests vs runtime rules

Tests in `comprehensive_baseline.rs`, `session::wubi_simcode_priority`,
and the cement-level scoring tests are **anti-rot regressions**, not
runtime policy. They pin observed behavior so framework changes that
silently regress polish-validated cases fail loudly at CI.

A test case `("yi", "就")` asserting top1 is NOT a "protect list" —
it's a snapshot of "the matrix model + the current data files
produce 就 at #0 for yi, and we don't want that to silently break".
The natural behavior is the truth; the test observes it.

When a test fails after a framework change, the question is:
- Is the test still describing user-validated behavior? → fix the
  framework regression.
- Has the user explicitly retired this case? → update the test
  (don't add a carve-out to make it pass).

Tests are exempt from the no-special-list rule because they don't
participate in runtime sort.

## 6. When you genuinely think the rule is wrong

It isn't. But if a situation arises where the matrix + data surface
*truly* can't express what the user needs, the correct response is:

1. STOP. Don't write code yet.
2. Articulate the gap: what structural rule is missing.
3. Propose the framework extension (new tier-naming rule, new
   producer-side natural-tier formula, new TOML knob).
4. Get explicit user sign-off.
5. Extend the framework.
6. Continue.

Never proceed via per-entry carve-out as a workaround. The cost of
the carve-out compounds; the framework gap is one fix away from
permanent.
