---
name: polish
description: Polish IME ranking via data assets (corpus / overlay TSV / EngineWeights TOML / phrases.txt) — never via composite/*.rs edits. Classifies the user's report into add/reorder/demote/delete, picks the right data file, applies minimum change, verifies via inputx-probe + baseline, then ALWAYS adds or updates a test case so the polish becomes a regression-locked invariant. Per user 2026-06-01 "终态" directive.
argument-hint: <free-form description of the polish — e.g. "lixiang 应该是 理想 不是 立项", "出现 蒌 在 mo 排第一不对", "加 不是 这个 wubi 缺", "fuzzy 太前 总是抢"
---

# /polish — Protocol

User reports a ranking imperfection. The skill classifies → applies → verifies → tests → commits. The terminal state is **a test case that pins the new behavior**, not just a fix — so polish actions compound rather than drift.

## Bedrock rule

Polish data lives in TSV files. Polish code does not exist.

> **Read first:** [`RANKING-MODEL-INVARIANTS.md`](RANKING-MODEL-INVARIANTS.md) — the canonical 10-tier × 3-engine model + the strict no-special-list anti-pattern. Every action below assumes that doc as bedrock.

**Never edit `composite/dispatch.rs` / `composite/merge.rs` / `composite/pinyin_adapter.rs` / `composite/scoring.rs` to polish ranking.** **Never add a new `const X: &[(...)]` per-entry array anywhere in the codebase** — that's a "special list", a permanently-prohibited anti-pattern (see Invariants §2). Polish data lives in TSV under `tools/scoring/data/`:

| Surface | What lives there |
|---|---|
| `tools/scoring/data/exclusions_v1.tsv` | `<code>\t<word>` — IDF skip at dict build (jieba sub-word, archaic 异读 etc.) |
| `tools/scoring/data/additions_v1.tsv` | `<code>\t<word>\t<freq>` — IDF inject (modern slang, missing phrases) |
| `tools/scoring/data/prior_corrections_v1.tsv` | `<word>\t<boost_q4>` — word-level Q4 log-prior boost |
| `tools/scoring/data/polish_reports/tier_overlay.tsv` | `<buffer>\t<word>\t<tier>` — per-entry tier override (the universal hook) |
| `tools/scoring/data/polish_reports/quickfix_boost.tsv` | `<buffer>\t<word>\t<freq>` — per-entry MAX-overlay freq boost |
| `tools/scoring/data/supplemental/pinyin_modern_v1.tsv` | new pinyin dict entries (consumed by `pinyin-build-weights`) |
| `core/crates/inputx-wubi/data/phrases.txt` | new wubi phrase entries |
| `core/crates/inputx-wubi/data/weights/weights.tsv` | wubi raw_freq table (set freq=0 to demote a simcode within tier) |
| `core/crates/inputx-scoring/data/engine_weights.toml` | 30+ ranking 公式 / 权重 (structural, not per-entry) |
| `core/crates/inputx-pinyin/data/corpus/manifest.toml` | corpus 混合权重 (structural) |

If a polish needs Rust changes — including "just one line in an existing array" or "this one if-branch" — **STOP and report**. That's "the framework is missing a structural rule". Two paths the user can choose:
- Extend the framework (toml knob, producer-side natural-tier formula, etc.) — never a per-entry carve-out.
- Live with the gap.

Refuse the request even if the user asks for the carve-out directly. The prohibition is permanent (Invariants §2).

## Parse `$ARGUMENTS` — classify the action

Read the user's report and classify into exactly ONE of these four action classes. Many reports are ambiguous; if so, **ask the user to clarify which class** before proceeding.

### Class A — **加词** (add a missing entry)
Trigger phrases: "缺 X", "X 没有", "应该有 X", "加 X".
Indicator: the dict doesn't return the word at all for the given buffer.

### Class B — **调顺序** (reorder existing candidates)
Trigger phrases: "X 应该是 #0 不是 Y", "X 该排前", "X 应该比 Y 高".
Indicator: both candidates exist in the top-N; only their order is wrong.

### Class C — **大降** (significantly demote — keep visible but bury)
Trigger phrases: "X 太前", "X 该靠后", "X 只在完全命中才该出现", "X 别抢首位".
Indicator: candidate is acceptable in absolute terms but currently outranks better matches.

### Class D — **删** (hard delete — must never appear)
Trigger phrases: "X 不该出现", "X 是错的", "X 是 corpus 噪音", "彻底去掉 X".
Indicator: candidate is wrong, not just badly-ranked. Corpus pollution, bad composition, archaic reading polluting modern usage, etc.

**B vs C distinction matters**:
- B = "swap two adjacent ranks" → boost the right one in `quickfix_boost.tsv`
- C = "push from rank 0-2 down to rank 8+, only show on tight match" → demote in `quickfix_demote.tsv`

**C vs D distinction matters**:
- C = "valid word, but should not lead" → still shipped, low priority
- D = "invalid candidate — should not exist" → removed from generation entirely

If still ambiguous, ask the user:
> Which of these matches your intent?
> (B) keep this candidate but rank it lower than X — it's still useful
> (C) significantly bury this candidate — only show it when the match is tight
> (D) delete this candidate entirely — it's wrong, should never appear

## Step 1 — Reproduce + diagnose

Before any data edit, run `inputx-probe` (or the polish-cli `show` wrapper) to capture **current top-10 for the buffer in question**. This is the baseline. Record it in the eventual commit message + test case.

```sh
python3 tools/polish-cli/inputx-polish.py show <buffer>
# or directly:
cd core && cargo run --quiet --release --bin inputx-probe -- <buffer> --mode mixed
```

Identify:
- Buffer (the typed input)
- Mode (Mixed / PinyinOnly — usually Mixed)
- Current top-10
- The target outcome (which word at #0, or which removed)

If the action is **Class A** (add), the word may not be in top-10 — that's expected. Verify the buffer makes sense and the engine routes correctly (e.g. wubi-shaped buffer goes through wubi dispatch).

## Step 2 — Apply the minimum data change

Pick the data asset from the table below. **Touch one file**. Anything wider is suspicious — stop and re-evaluate.

### Class A (加词)

For pinyin:
- If the missing word IS in `core/crates/inputx-pinyin/data/weights/weights.tsv` already (just at low freq → didn't pass the freq cutoff): use **`quickfix_boost.tsv`** with a freq that beats the cutoff (default cutoff = 100 in `pinyin-build-dict`). Boost to current top peer freq + 10% margin.
- If the missing word is NOT in weights.tsv: add to **`tools/scoring/data/supplemental/pinyin_modern_v1.tsv`** with a sensible freq (compare to similar-frequency words in weights.tsv for calibration — typically 30k-80k for top-tier real words, 50k for "I want this to be one of top 3"). MAX overlay semantics, so this both adds the entry AND sets a baseline freq.

For wubi:
- Append to **`core/crates/inputx-wubi/data/phrases.txt`** with the correct wubi-86 code. Use `inputx-polish add-phrase <word> --engine wubi` if the encoder binary is wired; otherwise compute manually per wubi-86 rules (top-2 codes per char for ≤2-char phrases, top-1 codes for ≥3-char) and add the row.

For JP: out of scope today — JP gaps go to a separate process (mozc / nihongo-jukugo TSV).

### Class B (调顺序)

Use **`tools/scoring/data/polish_reports/quickfix_boost.tsv`**.

Format: `<buffer>\t<word>\t<freq>\t# <comment>`. MAX overlay semantics — final freq for that (buffer, word) = `max(base, freq_here)`.

Compute the minimum boost:
- Look up the CURRENT top word's freq in `weights.tsv` → call this `top_base`
- Look up the chosen word's freq → `chosen_base`
- Boost magnitude = `top_base + 10% margin` (e.g. `top_base + max(top_base * 0.1, 1000)`)

The chosen candidate flips to top-1; gap to #2 is the 10% margin. Polish-log style.

Helper: `python3 tools/polish-cli/inputx-polish.py pick <buffer> <chosen>` does this end-to-end (compute + write + rebuild + verify).

### Class C (大降)

Use **`tools/scoring/data/polish_reports/quickfix_demote.tsv`**.

If the file doesn't exist yet, create it (this is the v1.13+ polish-cli's first introduction). Format:

```
# Format: <pinyin>\t<word>\t<demoted_freq>\t# <reason>
# Semantics: REPLACE overlay — `pinyin-build-dict` sets the (pinyin, word)
# freq to this value, regardless of base or other overlays. Use to push
# valid-but-rarely-wanted candidates to the bottom tier without removing
# them entirely. Typical demoted_freq: 100-500 (just above the cutoff so
# the entry survives, but lands at the tail).
mo\t嶙\t200\t# wubi Jianma2 simcode but rare-CJK char; ship visible only when fully matched
```

If `pinyin-build-dict` doesn't yet support this overlay, **STOP and report** — this is a framework extension (small one — add a new overlay alongside polish-log + modern-vocab in `build_dict.rs`, REPLACE instead of MAX). User decides whether to extend the framework or do a one-off via existing tools.

For wubi demote: if the wubi phrase has too-high natural freq, no overlay mechanism exists yet — STOP and report. Workaround: delete the wubi phrase entirely (Class D) and accept the loss.

### Class D (删)

Two sub-cases:

**D.1 — Dict entry exists, want it removed entirely**
- Pinyin: add a row to `tools/scoring/data/exclusions_v1.tsv` (create the file if absent) with `<pinyin>\t<word>`. Wire `pinyin-build-dict` to skip these (similar to the `BAKED_EXCLUSIONS` table already in `idf-from-pinyin-dict.rs` — extend to read from this file).
- Wubi: delete the row from `core/crates/inputx-wubi/data/phrases.txt`.

**D.2 — Dict doesn't have the entry directly; some pipeline (fuzzy / Viterbi composition / initials) is GENERATING it**
- This is harder. The generator (composite/pinyin_adapter.rs Path 5 K-best, Path 1b fuzzy, JP compose_sentence, etc.) is producing a candidate that shouldn't exist.
- **STOP and report**: the framework is generating bad output, not the data. Options: (a) extend the generator's quality gate (e.g. K-best now drops products whose bigram score < threshold), or (b) add the offender to an exclusion list the generator consults. Either path is a framework extension, not a data polish — user decides.

### How to decide between B and C — the 10×/100× heuristic

If the rank change you want is **1–2 slots** (e.g. #2 → #0), use Class B (boost).

If the rank change is **5+ slots OR "should rarely if ever appear"**, use Class C (demote). C is for cases where any boost on the right candidate would be drowned by noise — easier to pull the wrong candidate down.

## Step 3 — Rebuild + verify

```sh
make polish-rebuild     # weights → dict → idf → baseline test
```

`polish-rebuild` exits non-zero if the baseline 24-test fixture fails. **STOP and report**: the polish broke a documented invariant. The user decides whether to update the conflicting baseline test (the new polish is the correct truth) or revert the polish (the conflicting baseline is the correct truth).

Then re-run `inputx-probe` on the polished buffer to confirm the outcome:

```sh
python3 tools/polish-cli/inputx-polish.py show <buffer>
```

For Class A: target word should now be in top-10 at some defensible rank.
For Class B: target word should be at #0.
For Class C: demoted word should be at rank ≥ 8.
For Class D: target word should NOT be in any returned candidates.

## Step 4 — Add / update test case

**This is the non-negotiable step.** Polish without a test is drift — the next polish can silently re-break this one.

Find the right test file:

| Class | Test location | Pattern |
|---|---|---|
| B (reorder) — pinyin-only | `core/crates/inputx-core/src/composite/comprehensive_baseline.rs::pinyin_only_top_common_single_syllable` or `pinyin_only_multi_syllable` | `("<buffer>", "<expected_top>")` |
| B — mixed mode | `core/crates/inputx-core/src/composite/comprehensive_baseline.rs::jianma{1,2,3}_*` | `("<buffer>", "<expected_top>")` |
| A (add) — pinyin | `core/crates/inputx-core/src/composite/comprehensive_baseline.rs::pinyin_only_extended_common_words` | `("<buffer>", "<expected_top>")` |
| C (demote) — | `core/crates/inputx-core/src/composite/comprehensive_baseline.rs::rare_jianma2_chars_yield_to_pinyin_top` or a new equivalent | acceptable-set assertion: word `NOT IN top-3` |
| D (delete) — | `core/crates/inputx-core/src/composite/comprehensive_baseline.rs::no_traditional_in_top5_for_common_pinyin` or a new "blacklist" test | word `NOT IN top-10` |

**Reuse > add**. Many polish actions can extend an existing case (add a row to a fixed-style `&[(buffer, expected)]` table) instead of creating a new test. Only create a new test function if the assertion shape genuinely doesn't fit any existing case.

When updating an existing test:
- Append the new `(buffer, expected)` row at the bottom of the same fixture (keep contextual grouping if there's an obvious cluster).
- Inline-comment the polish reason and the user-report date so future readers can trace back.

When creating a new test:
- Name it descriptively: `polish_<scope>_<buffer>_<expected_outcome>` (e.g. `polish_lixiang_lixiang_leads`).
- Match the file's existing test scaffolding (helper functions, `#[test]` placement).
- Add one assertion line plus a comment block describing the original user report.

Run `cargo test -p inputx-core --lib <new_test_name>` (or the existing test that contains the new row) to confirm it passes.

## Step 5 — Auto-deploy to mac IME (silent reinstall)

After baseline passes and BEFORE committing, ship the change to the
live mac IME so the user sees the polish immediately. Use the single
canonical install entry — `mac/reinstall.py` — which auto-detects
"reinstall" mode (bundle already trusted), takes a backup, runs the
swap, watches 5s for crashes, rolls back on failure.

```sh
mac/reinstall.py
```

What it does in reinstall mode:

1. Snapshot the currently-running bundle to `Inputx.app.bak-<timestamp>`.
2. Build + swap the bundle on disk; re-bootstrap the LaunchAgent.
3. Invalidate macOS 26's IntlDataCache + restart TextInputMenuAgent
   so the picker re-enumerates fresh.
4. Wait 5s for crash window, verify PID is alive AND unchanged
   (no crash + LaunchAgent respawn cycle).
5. On failure: tear down the broken install, restore the backup,
   re-bootstrap the LaunchAgent against the restored bundle, exit
   non-zero.
6. On success: drop the backup, exit zero.

What it explicitly does NOT do (per memory: no-defensive-programming):
- Does NOT call `Inputx install` / TISRegisterInputSource. That's
  owned by macOS Settings UI's "Add Input Source" flow (which also
  grants the TCC trust required for the picker to show us). Calling
  TISRegister from us would create duplicate TIS rows.
- Does NOT touch `AppleEnabledInputSources`. Settings owns it; our
  writes wouldn't reach Settings UI's view anyway.

**Bundle ID + mode ID are stable across reinstalls** — that's what
keeps TCC trust persistent so subsequent reinstalls are silent.
If you ever find yourself wanting to change CFBundleIdentifier or
TISInputSourceID in Info.plist: STOP. That breaks every existing
user's TCC trust and forces them through System Settings UI again.
Memory: [[no-buggy-looking-ids]] covers the one historical case
where this was necessary (the `wubi.wubi.zh` → `wubi.zh` fix);
do not repeat the cost casually.

If `reinstall.py` exits non-zero — the previous Inputx version
is still running (rolled back), but the polish data change is sitting
in the working tree uncommitted. Two paths:

- **The polish change is correct but the build hit a transient issue**
  (sccache desync, cargo lock race, etc.): try again with `mac/reinstall.py`
  once. If still failing, STOP and report — escalate to the user.
- **The polish change broke the build somehow** (e.g. a TOML row that
  build_dict rejects with a panic — extremely rare since polish only
  touches overlays): revert the data change, report to user. The
  polish-rebuild step at Step 3 should have caught this — if it
  didn't, that's a polish-rebuild gap, file it.

NEVER skip `mac/reinstall.py` for "fast iteration". The user has been
burned multiple times by reinstalls that succeeded mid-script but
left the IME in a half-deployed state where text input across the OS
stops working. The 5s health window + backup is the defense; the
polish skill must always go through it.

## Step 6 — Commit

One commit per polish action. Message structure:

```
polish(<class>:<engine>): <one-line summary> [polish][data]

User report 2026-MM-DD: "<verbatim quote>".

Classification: Class <A|B|C|D> — <one sentence>.

Before:
  buffer=<buf> mode=<mode>
  top10 = [<word1>, <word2>, ...]

After:
  buffer=<buf> mode=<mode>
  top10 = [<word1>, <word2>, ...]

Data change:
  - <file path>: <line summary, e.g. "added row: lixiang 理想 49000">

Test:
  - <file>::<test_name>: <line summary, e.g. "added case (lixiang, 理想)">

Rebuild + baseline:
  - make polish-rebuild → baseline 24/24 ✓, lib 288/0 ✓.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
```

Then `git push origin develop`. If the polish breaks something that requires multi-step recovery, **stop and report — do not amend or revert**; the user decides.

## Step 7 — Handle "polish accidentally broke baseline"

If `make polish-rebuild` fails (baseline test regression), three paths:

1. **The polish was wrong**: revert the data change (`git checkout -- <file>`), report to user.
2. **The baseline test was wrong** (pre-v1.10 era assertion, doesn't reflect modern usage): update the baseline test as part of this same polish commit. Write a brief explanation in the commit message of why the old assertion was outdated.
3. **The polish has scope mismatch** (you changed a Class B `quickfix_boost` row, but the right action was Class C demote): revert, re-classify with the user, retry.

## Anti-patterns to refuse

- **Adding a per-entry hardcoded list / array / if-branch anywhere in Rust source**: STRICT permanent prohibition per [RANKING-MODEL-INVARIANTS §2](RANKING-MODEL-INVARIANTS.md#2-strict-no-special-lists-anywhere). Includes `const X: &[(&str, &str)] = &[...]`, `match (buf, w) { ... }`, `if buf == "..." { ... }`, `protect_list = [...]`. Refuse even when the user requests it directly. The data surface (Bedrock table above) is the only per-entry hook.
- **Editing `composite/*.rs` to polish ranking**: per bedrock rule. Refuse and explain.
- **Polish without a test case**: the polish *will* drift. Stop and add the test.
- **Multi-action commits**: each polish action gets its own commit. If the user reports 5 things in one message, do them as 5 commits, sequentially.
- **Boost values picked from thin air**: always derive from `weights.tsv` peer freq (B) or current cutoff (A/C). Magnitude justification goes in the commit message.
- **Mixing class B and C in one TSV**: `quickfix_boost.tsv` is for boosts, `quickfix_demote.tsv` is for demotes. Don't put a low value in boost expecting MIN semantics — boost is MAX.
- **Skipping `make polish-rebuild`**: every polish action MUST run through the rebuild + baseline gate before commit.
- **Skipping `mac/reinstall.py`**: every polish action MUST deploy to the live IME before commit. The script's backup + 5s health window + automatic rollback is the user-protection contract — reinstall failures have historically left the OS unable to accept text input.
- **Changing CFBundleIdentifier or TISInputSourceID**: forbidden during polish. These two IDs are what keep TCC trust persistent across reinstalls; touching them breaks every existing user's silent-reinstall contract and forces them through System Settings UI to re-grant trust. If you genuinely believe one must change, STOP the polish and escalate.

## On framework extensions

When the protocol says **"STOP and report — framework extension needed"**, it means:
- `quickfix_demote.tsv` doesn't exist yet (first Class C invocation)
- `exclusions_v1.tsv` doesn't exist yet (first Class D.1 invocation)
- Class D.2 is requested (composition-layer fix)
- `inputx-polish add-phrase` wubi encoder isn't shipped (first Class A wubi invocation)

In each case, stop and tell the user exactly what extension is needed, what the smallest possible patch is (e.g. "add 20-line overlay block to `pinyin-build-dict.rs`"), and let them decide whether to extend now or work around. Don't silently expand the polish to also extend the framework — that's two changes, two commits, two reviews.
