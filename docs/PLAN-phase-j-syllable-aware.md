# Phase J — Pinyin syllable-aware engine refinement

> **Status (2026-06-06):** specification approved by user, awaiting
> implementation under `feature/phase-j-impl` branch.
>
> **User directive:** "Q4 做的，只是我要彻底完整的方案" — this doc
> is the full plan; implementation must not deviate without
> updating it first.
>
> **Predecessor:** ranking-model Phase I (`928d5c7`, wubi full-code
> redundancy gate). Next ranking-model phase is **Phase J**.
>
> **Related but separate:** corpus filter NF1..NF6 (`docs/PLAN-ingest-noise-filter.md`)
> — uses NF prefix to avoid name collision with this Phase J.

---

## 1. Why this phase exists

### 1.1 The reported pain

User report 2026-06-05 (`shehv` polish thread): typing `shehv` /
`shehb` / `shehz` produces a top-10 dominated by 2-syllable Chinese
words whose pinyin initials are `s+h` (时候 / 生活 / 说话 / 社会 / …).

The user's complaint, restated:
- `she` is a clean first syllable
- `hv` / `hb` / `hz` is junk continuation
- The IME should treat this as "user committed `she`, then typed
  junk" — surface continuations of `she` (社会 / 奢华 / 设好 / …)
- The IME currently treats it as "missing-vowel typo from the start"
  — surface 时候 / 生活 / 说话 (which have nothing to do with `she`)

### 1.2 The architectural source

The misfire is **Path 1c** (initials-fallback typo rescue),
`core/crates/inputx-core/src/composite/pinyin_adapter.rs` ~lines
1186-1229. Path 1c gate today:

```rust
buffer.len() ∈ [4, 5]          // Phase H cap
&& !prefix_exists(buffer)       // not a clean pinyin dict prefix
&& consonant_prefix.len() == 2  // 2 consonants before first vowel
&& suffix_len >= 2              // ≥2 chars after
```

It looks ONLY at the consonant-letter shape of the buffer, not at
the syllable structure. For `shehv`:
- consonant_prefix = `sh` (chars-up-to-first-vowel; stops at `e`)
- suffix_len = 3 (`ehv`)
- All conditions pass → Path 1c fires → looks up `sh` in 2-letter
  initials index → returns 时候/生活/... 50 candidates

The mechanism was designed for typos like `pyin → 拼音` (real typo:
user dropped the `i`, buffer has NO clean syllable prefix). It
over-applies to `shehv`-class buffers where a clean syllable prefix
DOES exist.

### 1.3 What "Apple-canonical" pinyin engines do here

Both macOS native pinyin and Sogou pinyin apply syllable-aware
segmentation:

1. Parse buffer left-to-right, find the longest valid syllable prefix
2. If the remainder also parses as continuation → standard
   multi-syllable expansion
3. If the remainder is malformed → treat as "user mid-typing the
   second syllable", offer prefix-completions of `<first syl> +
   <leading consonant of remainder>*`
4. If the buffer has NO valid syllable prefix at all → initials-
   fallback typo rescue (= our Path 1c semantics)

Our engine is missing #3 (syllable-resegment + partial completion)
and our Path 1c (#4) overshoots into territory where #3 should apply.

Phase J closes both gaps.

---

## 2. Current state — file:line audit

| Layer | File | Symbol | Role |
|---|---|---|---|
| Syllable validation | `core/crates/inputx-pinyin/src/syllable.rs:106` | `is_valid(s) -> bool` | exact-match against 403-syllable inventory |
| Syllable segmentation | `core/crates/inputx-pinyin/src/segmenter.rs:46` | `segment(s) -> Vec<Segmentation>` | DP full-coverage segmentation; returns `[]` if any suffix can't segment |
| Re-export | `core/crates/inputx-pinyin/src/lib.rs:53` | both above | public API |
| Pinyin dict prefix test | `core/crates/inputx-pinyin/src/dict.rs:284` | `prefix_exists(p) -> bool` | raw-byte prefix match against FST dict keys |
| Pinyin dict prefix lookup | `core/crates/inputx-pinyin/src/dict.rs:294` | `prefix(p) -> Vec<(String, String)>` | raw-byte prefix lookup |
| Path 1c gate (shared helper) | `core/crates/inputx-core/src/composite/pinyin_adapter.rs` | `path1c_consonant_prefix() -> Option<String>` | computed via `chars().take_while(|c| !is_vowel(c))` |
| Path 1c fire site | `core/crates/inputx-core/src/composite/pinyin_adapter.rs` ~1186-1229 | `find_candidates()` Path 1c block | reads `path1c_consonant_prefix()` |
| Path 3 prefix-completion | `core/crates/inputx-core/src/composite/pinyin_adapter.rs` ~1337-1376 | `find_candidates()` Path 3 block | `push_prefix_top_k(buffer, ...)` |
| `is_pure_garbage` ASCII fallback gate | `core/crates/inputx-core/src/composite/engine.rs` ~426 | `is_pure_garbage() -> bool` | early-true unless: pinyin engine has future_match OR `path1c_would_fire` OR JP is enabled |
| `has_future_match` | `core/crates/inputx-core/src/composite/pinyin_adapter.rs:900` | `has_future_match() -> bool` | trims trailing 1..4 chars, checks suffix-could-start-syllable + prefix_exists |

**Gaps for Phase J:**

1. No `longest_valid_syllable_prefix(s) -> Option<&str>` API. `segment()`
   requires full coverage and returns `[]` for partial-match cases.
2. Path 1c's gate ignores syllable structure entirely.
3. No "trim-retry" pattern in Path 3: if `prefix_exists(buffer)` is
   false, Path 3 just yields nothing — no fallback that drops a
   trailing char and retries with the shorter buffer.

---

## 3. The behavioral gap, by concrete example

| Buffer | Current top-N | What user expects | Why current is wrong |
|---|---|---|---|
| `shehv` | 时候 / 生活 / 说话 / 社会 / 上海 / 上好 / … | 社会 / 奢华 / 设好 / 射核 / … (same as `sheh`) | Path 1c hijacks; `she` is clean syllable, not a missing-vowel typo |
| `shehb` | same | same | same |
| `shehz` | same | same | same |
| `xianv` | (today: empty / ASCII fallback after [v-wipe](docs/PLAN-…) fix) | 现在 / 先生 / 显然 / xian-h* completions | similar shape — `xian` clean syl, `v` junk |
| `pyin` | 拼音 / … | 拼音 / … | already correct — Path 1c is designed for this, no clean syl prefix exists |
| `pnyin` | 拼音 / … | 拼音 / … | already correct — same reason |
| `hello` | (today: ASCII fallback at len 5) | ASCII fallback (English word) | already correct — must NOT regress this case |
| `qwxzy` | ASCII fallback | ASCII fallback | already correct — no syl prefix, no consonant shape, must NOT regress |

**The discrimination needed**: a buffer has a "clean syllable
commitment" iff a valid pinyin syllable of length **≥ 3** prefixes
it. Threshold of 3 chosen because:

- 2-letter syllables (`he`, `ma`, `na`, …) overlap with English
  high-frequency word starts → false positives like `hello → he+llo`
- 3-letter syllables (`she`, `xia`, `dao`, `tang`, …) are
  unambiguously Chinese-shape — only matches Chinese intent
- Most non-trivial pinyin words start with a 3+ char syllable in
  practice; 2-char ones are mostly particles, well-handled today

---

## 4. Design space — three options weighed

### Option A — minimal: tighten Path 1c gate only

Add `longest_valid_syllable_prefix(buffer).chars().count() >= 3` to
Path 1c's "do NOT fire" conditions. No new positive mechanism.

**Effect on shehv:** Path 1c blocked → 0 candidates → wiped by
ASCII fallback (because `is_pure_garbage` returns true). Worse
than current.

**Verdict:** rejected. Solves the misfire but leaves a hole.

### Option B — chosen: Option A + Path 3 trim-retry rescue

Tighten Path 1c gate (Option A) AND add a sibling mechanism: when
the buffer has a clean syllable prefix (≥3 chars) but `prefix_exists`
fails, drop trailing chars one at a time and run Path 3 on the
trimmed buffer (the first trim that satisfies `prefix_exists`).

**Effect on shehv:** Path 1c blocked → trim-retry: `shehv` → `sheh`
→ `prefix_exists("sheh")` = true → Path 3 surfaces 社会 / 奢华 /
设好 / … (= same as `sheh`). 

**Effect on hello:** Path 1c blocked anyway (consonant_prefix `h`,
len 1, gate already rejects). Trim-retry blocked (longest syl
prefix `he` is only len 2). → 0 cands path → ASCII fallback. ✓

**Effect on pyin:** longest syl prefix none → trim-retry blocked.
Path 1c still fires (consonant_prefix `py` len 2, suffix `in` len 2,
no clean syl). → 拼音 rescue. ✓

**Effect on qwxzy:** no syl prefix, no consonant shape. → ASCII
fallback. ✓

**Effect on xianv:** longest syl prefix `xian` (len 4). trim-retry:
`xianv` → `xian` → `prefix_exists` true → Path 3 surfaces 现在 /
先生 / … xian-completions. ✓

**Verdict:** chosen. Solves the user pain, preserves all currently-
correct behavior.

### Option C — maximal: full multi-syllable Viterbi resegmentation

Replace `path1c_consonant_prefix` + `has_future_match` + Path 3
with a unified Viterbi over the buffer that maximizes
`P(syllable sequence | buffer)`, surfacing candidates from any
prefix segmentation.

**Effect:** strictly more powerful, handles 3+ syllable partial
input gracefully.

**Verdict:** rejected for THIS phase. Scope explosion (multi-day
refactor + risk to Phase H bigram-density gate). Phase J ships
Option B; Option C lives as a future PLAN if Option B proves
insufficient.

---

## 5. Chosen design — Option B detail

### 5.1 New helper: `longest_valid_syllable_prefix`

Location: `core/crates/inputx-pinyin/src/syllable.rs` (new public
function alongside `is_valid` / `count`).

```rust
/// Greedy: scans buffer left-to-right, returns the longest prefix
/// that is_valid() recognizes as a complete syllable. Returns None
/// if no prefix of length 1..min(6, buffer.len()) is a valid syllable.
///
/// Used by composite/pinyin_adapter.rs's Path 1c gate (Phase J) to
/// detect "user committed to a clean syllable" — gating against
/// initials-fallback typo rescue.
///
/// Stops at length 6 because the longest Mandarin syllable is
/// `zhuang/chuang/shuang` (6 letters) — searching further is wasted
/// work.
pub fn longest_valid_syllable_prefix(s: &str) -> Option<&str> {
    let cap = s.len().min(6);
    let mut best: Option<&str> = None;
    for end in 1..=cap {
        // s.is_char_boundary(end) holds because syllable letters are
        // ASCII; defensive check would be needed if we ever accepted
        // non-ASCII pinyin input.
        let candidate = &s[..end];
        if is_valid(candidate) {
            best = Some(candidate);
        }
    }
    best
}
```

Re-exported from `core/crates/inputx-pinyin/src/lib.rs:53` alongside
`is_valid`.

### 5.2 Tighten Path 1c gate

Location: `core/crates/inputx-core/src/composite/pinyin_adapter.rs`,
the `path1c_consonant_prefix()` method (added by `a5c2ee7`).

Add a new condition: skip if longest valid syllable prefix is ≥ 3 chars.

```rust
pub(crate) fn path1c_consonant_prefix(&self) -> Option<String> {
    if self.has_non_speculative_candidate {
        return None;
    }
    if !(4..=5).contains(&self.buffer.len()) {
        return None;
    }
    if self.engine.dict().prefix_exists(&self.buffer) {
        return None;
    }
    // Phase J: if the buffer starts with a clean ≥3-char syllable,
    // the user committed to that syllable and the trailing chars
    // are mid-typing junk, not a missing-vowel typo. Trim-retry
    // (Path 3b below) handles the candidate generation in that
    // case; Path 1c stays out.
    if inputx_pinyin::longest_valid_syllable_prefix(&self.buffer)
        .map_or(false, |s| s.chars().count() >= 3)
    {
        return None;
    }
    let consonant_prefix: String = self.buffer.chars()
        .take_while(|c| !matches!(*c, 'a' | 'e' | 'i' | 'o' | 'u' | 'v'))
        .collect();
    let suffix_len = self.buffer.len() - consonant_prefix.len();
    if consonant_prefix.len() == 2 && suffix_len >= 2 {
        Some(consonant_prefix)
    } else {
        None
    }
}
```

### 5.3 Add Path 3b — syllable-aware trim-retry

Location: same file, immediately after Path 3 (after the existing
`push_prefix_top_k(&self.buffer, …)` call). The new block fires
only when:

- Path 3 yielded no candidates (so `self.candidates.is_empty()` or
  more precisely `self.candidates.len() == pre_count`)
- Buffer has a clean ≥3-char syllable prefix

```rust
// Path 3b (Phase J): syllable-aware trim-retry. When the buffer
// has a clean ≥3-char syllable prefix but doesn't match any FST
// prefix as-is, the trailing chars are likely mid-typing of a
// 2nd syllable that hasn't completed yet. Drop trailing chars
// one at a time until prefix_exists succeeds, then surface the
// shorter prefix's completions. The user sees the same panel as
// if they hadn't typed the trailing chars yet.
//
// `shehv` → trim `v` → `sheh` (prefix_exists ✓) → Path 3 on `sheh`
//   surfaces 社会 / 奢华 / 设好 / 射核 / … (sheh* completions).
// `xianv` → trim `v` → `xian` → 现在 / 先生 / … xian* completions.
// Bounded at 4 trims (max useful — beyond that we'd lose the
// first syllable entirely).
if allow_prefix_completion
    && self.candidates.len() == cap_before_path3
    && inputx_pinyin::longest_valid_syllable_prefix(&self.buffer)
        .map_or(false, |s| s.chars().count() >= 3)
{
    let buf_bytes = self.buffer.len();
    'trim_loop: for trim in 1..=4.min(buf_bytes - 1) {
        let shorter = &self.buffer[..buf_bytes - trim];
        if self.engine.dict().prefix_exists(shorter) {
            let want = cap - self.candidates.len();
            if want > 0 {
                push_prefix_top_k(
                    shorter,
                    want,
                    &mut seen,
                    &mut self.candidates,
                    &mut self.prefix_scored,
                    &mut self.prefix_components,
                );
            }
            break 'trim_loop;
        }
    }
}
```

Note `cap_before_path3` is a new local variable captured immediately
before the Path 3 block to detect "Path 3 produced nothing".

### 5.4 No change needed to `is_pure_garbage`

Today `is_pure_garbage` (engine.rs ~426) returns true unless any of:
- JP plugin active → false
- `has_future_match` → false
- `path1c_would_fire` → false

With Phase J's trim-retry adding a new candidate-generation path,
`is_pure_garbage`'s natural escape (`pinyin engine produces some
candidates` is implicit via the subsequent `if preedit.len() >= 5
&& is_pure_garbage()` only triggering ASCII fallback when garbage
genuinely has nothing). We need to ensure `is_pure_garbage` ALSO
checks the syllable-prefix condition so that buffers like `shehv`
(where Path 1c is now blocked but trim-retry will succeed) don't
wipe before Path 3b can run.

Add to `is_pure_garbage`:

```rust
// Phase J: if the buffer has a clean ≥3-char syllable prefix, the
// trim-retry path (pinyin_adapter Path 3b) will produce candidates;
// not garbage.
if inputx_pinyin::longest_valid_syllable_prefix(&self.pinyin.buffer())
    .map_or(false, |s| s.chars().count() >= 3)
{
    return false;
}
```

(`pinyin.buffer()` accessor exists; if not, add a pass-through.)

---

## 6. Edge cases + non-goals

### 6.1 Edge cases handled

| Buffer | Path 1c? | Trim-retry? | ASCII fallback? | Verdict |
|---|---|---|---|---|
| `shehv` | blocked (she ≥3) | fires → sheh | no | ✓ user-pain fixed |
| `shehb` | blocked | fires → sheh | no | ✓ |
| `shehz` | blocked | fires → sheh | no | ✓ |
| `xianv` | blocked (xian ≥3) | fires → xian | no | ✓ |
| `xianzhang` | n/a (prefix_exists true) | n/a | no | ✓ unchanged |
| `pyin` | fires (no syl prefix ≥3) | n/a | no | ✓ Path 1c rescues |
| `pnyin` | fires | n/a | no | ✓ |
| `hello` | blocked (consonant `h` len 1) | blocked (`he` only len 2) | yes | ✓ |
| `qwxzy` | blocked | blocked | yes | ✓ |
| `nihao` | n/a (prefix_exists true) | n/a | no | ✓ unchanged |
| `daizhe` | n/a (prefix_exists true after polish 794de7e) | n/a | no | ✓ unchanged |

### 6.2 Explicit non-goals

- **No multi-syllable Viterbi**. Buffers like `nihaomawojiao`
  (multi-syllable composition probe) keep going through the
  existing Composed-Viterbi (Phase F) path. Phase J only adds
  one-trim-retry, not arbitrary segmentation.

- **No change to wubi side**. Wubi has no syllable concept; its
  4-letter exhaustion path is governed by Phase I (full-code
  redundancy) and not relevant here.

- **No change to JP side**. JP adapter's romaji parsing is
  independent; `is_pure_garbage` already early-returns false when
  JP is enabled, so trim-retry doesn't interact.

- **No retroactive corpus rerun**. Phase J is engine-side; existing
  dict / IDF byte-for-byte unchanged.

### 6.3 Regressions to watch

- Any baseline test that asserted `shehv`-class behavior. Currently
  none documented (the user's polish of `shehv` was driven by the
  v-wipe bug, not a tier-pinned outcome).
- Any test with a 5-char buffer that has a ≥3-char clean syllable
  prefix and currently passes via Path 1c initials surfacing — these
  would now go through trim-retry. Should produce equivalent or
  better candidates, but the FIRST cand may shift. Audit baseline
  diff.

---

## 7. Implementation plan — file-by-file

| File | Change | Loc |
|---|---|---|
| `core/crates/inputx-pinyin/src/syllable.rs` | add `pub fn longest_valid_syllable_prefix` | new function |
| `core/crates/inputx-pinyin/src/lib.rs:53` | re-export the new function | re-export line |
| `core/crates/inputx-pinyin/src/syllable.rs` (tests) | add unit tests for `longest_valid_syllable_prefix` covering: empty, len-1 valid, len-1 invalid, mixed, ASCII-only, edge "hello"/"shehv"/"xianv"/"pyin"/"qwxzy" | new tests |
| `core/crates/inputx-core/src/composite/pinyin_adapter.rs` | tighten `path1c_consonant_prefix()` gate per §5.2 | within method |
| `core/crates/inputx-core/src/composite/pinyin_adapter.rs` | add Path 3b trim-retry block per §5.3 | after existing Path 3 |
| `core/crates/inputx-core/src/composite/engine.rs` | add syllable-prefix non-garbage signal per §5.4 | within `is_pure_garbage` |
| `core/crates/inputx-core/src/composite/comprehensive_baseline.rs` | add baseline test pinning shehv → sheh-like candidates | new test fn |
| `core/crates/inputx-core/src/composite/comprehensive_baseline.rs` | extend existing ASCII fallback test to assert `hello` STILL hits ASCII fallback (regression guard) | extend existing |

Total: 5 files modified, 1 new public API in `inputx-pinyin`, 1 new
test function.

---

## 8. Test plan

### 8.1 Unit tests (in `inputx-pinyin/src/syllable.rs`)

```rust
#[test]
fn longest_valid_syllable_prefix_basics() {
    assert_eq!(longest_valid_syllable_prefix(""), None);
    assert_eq!(longest_valid_syllable_prefix("a"), Some("a"));
    assert_eq!(longest_valid_syllable_prefix("she"), Some("she"));
    assert_eq!(longest_valid_syllable_prefix("shehv"), Some("she"));
    assert_eq!(longest_valid_syllable_prefix("xian"), Some("xian"));
    assert_eq!(longest_valid_syllable_prefix("xianv"), Some("xian"));
    assert_eq!(longest_valid_syllable_prefix("hello"), Some("he"));
    assert_eq!(longest_valid_syllable_prefix("pyin"), None);
    assert_eq!(longest_valid_syllable_prefix("qwxzy"), None);
    // Maximal Mandarin syllable: chuang / shuang / zhuang (6 letters)
    assert_eq!(longest_valid_syllable_prefix("shuang"), Some("shuang"));
    assert_eq!(longest_valid_syllable_prefix("shuangxx"), Some("shuang"));
}
```

### 8.2 Integration / baseline tests (in `comprehensive_baseline.rs`)

```rust
#[test]
fn phase_j_syllable_aware_trim_retry() {
    // Buffers where a clean ≥3-char syllable prefix + invalid trailing
    // char should route to Path 3b trim-retry, producing the same
    // candidate set as if the trailing char hadn't been typed.
    let cases: &[(&str, &str, &str)] = &[
        // (buffer, equivalent_trimmed, expected_top_candidate_word)
        ("shehv", "sheh", "社会"),
        ("shehb", "sheh", "社会"),
        ("shehz", "sheh", "社会"),
        ("xianv", "xian", "现在"),
    ];
    for (buf, trim, expected_top) in cases {
        let top10 = mixed_top10(buf.as_bytes());
        assert!(!top10.is_empty(),
            "Phase J: {buf} should produce candidates via trim-retry");
        let trim_top10 = mixed_top10(trim.as_bytes());
        assert_eq!(top10.first(), trim_top10.first(),
            "Phase J: {buf} top-1 ({top10:?}) should equal {trim} top-1 ({trim_top10:?})");
        if let Some(t) = top10.first() {
            assert_eq!(t, expected_top,
                "Phase J: {buf} top-1 should be {expected_top}, got {t}");
        }
    }
}

#[test]
fn phase_j_path1c_still_rescues_real_typos() {
    // Buffers with no clean ≥3-char syllable prefix should still
    // get Path 1c initials-fallback rescue.
    for buf in &["pyin", "pnyin", "zhgo"] {
        let top10 = mixed_top10(buf.as_bytes());
        assert!(!top10.is_empty(),
            "Phase J regression: {buf} lost Path 1c rescue");
    }
}

#[test]
fn phase_j_english_still_hits_ascii_fallback() {
    // ASCII fallback must NOT regress for genuine English / pure
    // garbage 5+ char buffers.
    // (Reuse existing ascii_fallback_yields_to_path1c_initials_rescue
    // plus a positive ascii-fallback assertion.)
    // ... see existing ascii_fallback_* tests in engine.rs
}
```

### 8.3 Probe sanity checks (post-deploy)

```sh
python3 tools/polish-cli/inputx-polish.py show shehv  # → 社会 ... (same as sheh)
python3 tools/polish-cli/inputx-polish.py show sheh   # → 社会 ... (unchanged)
python3 tools/polish-cli/inputx-polish.py show pyin   # → 拼音 (Path 1c unchanged)
python3 tools/polish-cli/inputx-polish.py show hello  # → empty/ASCII fallback path
```

---

## 9. Rollout

1. Branch `feature/phase-j-impl` off `develop`.
2. Land §7 changes (5 files), commits scoped per file or per
   logical group as the implementation reveals.
3. `cargo test -p inputx-pinyin --lib` — new helper passes.
4. `cargo test -p inputx-core --lib` — Phase J baseline tests pass,
   existing 311+ baseline tests still pass.
5. `make polish-rebuild` — full chain green.
6. `python3 mac/reinstall.py` — deploys to live IME, post-conditions
   pass (probe returns 你好, LS singleton, TIS row enabled,
   AppleEnabledThirdPartyInputSources singleton).
7. User-acceptance: probe outputs match §8.3, type a few `shehv`-
   class buffers in live apps.
8. `git switch develop && git merge --no-ff feature/phase-j-impl &&
   git push origin develop`. Branch cleanup.

---

## 10. Estimate

- §5.1 helper + unit tests: ~30 min
- §5.2 Path 1c gate tighten: ~10 min
- §5.3 Path 3b trim-retry: ~30 min
- §5.4 is_pure_garbage update: ~10 min
- §8 tests authoring + run: ~30 min
- Reinstall + probe sanity: ~10 min
- Doc + commit + merge: ~20 min

**Total: ~2.5 hours.**

---

## 11. Future extensions Phase J explicitly defers

| Future phase | Trigger to consider |
|---|---|
| **Phase K** — multi-syllable Viterbi resegmentation (full Sogou semantics) | Phase J's trim-retry proves insufficient for ≥6-char buffers with multi-syllable partial input |
| **Per-buffer fuzzy initials** | User reports of "I typed wrong consonant by 1 key" that current Path 1b / 1c don't catch |
| **Phrase suggestion from clean syllable prefix** | `she-` should also suggest 设计 / 设备 / 社会 / 涉外 phrase-level alongside character continuations |

These live in future PLAN docs if/when triggered, NOT in this Phase
J spec.

---

## 12. Status tracking

| Step | Status | Branch / commit |
|---|---|---|
| Design doc (this file) | ✅ landed | `feature/phase-j-spec` → develop |
| Implementation | ⏳ pending | `feature/phase-j-impl` (to be created) |
| Tests | ⏳ pending | same branch |
| Live deploy + verify | ⏳ pending | same branch |
| Merge to develop | ⏳ pending | — |
