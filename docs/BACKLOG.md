# Inputx Backlog

> Source of truth for "what's next". Top section answers "what should
> the assistant do RIGHT NOW given no further user input"; sections
> below are full prioritized backlog with status and triggers.

---

## NEXT-ACTION (read first on session start)

**Latest shipped tag:** `v1.14.0` (2026-08-11) — tagged on `develop`,
same as v1.5.0 through v1.13.0. `CHANGELOG.md` has a matching `1.14.0`
entry; entries for v1.4.0–v1.13.0 were never written and the file says
so rather than backfilling them.
**Active branch:** `develop` (always at-or-ahead of `master`; master is dormant)
**Active feature/polish branches:** `feature/ios-shipping` only —
**shelved** per `.claude/PLAN.md` §L2 ("iOS shipping currently
SHELVED"), 1 commit ahead / 1557 behind develop. Do NOT resume it
during autorun.
**Commits ahead of origin/develop:** 0 (synced after last push)

**Current cycle:** v1.14 — open. Refreshed 2026-08-11. Headline state:

1. **Pinyin pipeline in minimal-debug mode** — **four** category gates
   in `pinyin_adapter.rs` (`PINYIN_DISABLE_COMPOSE / ASSOCIATION /
   FUZZY / PREDICTION`, lines 77-80) all set `true` (verified
   2026-08-11).  Only literal-syllable lookup + FST prefix completion
   + rare-CJK display filter active.  Per-const behavior reference:
   `docs/pinyin-pipeline-gates.md`.  Project memory
   `[[project-pinyin-minimal-debug-state]]` is the cross-session
   anchor — **do not auto-flip a const without user permission**;
   empty results for paused-family buffers (`pyin`, `zg`, `zongguo`,
   `shehv`, `hhhh`, etc.) are intentional, not a regression.
2. **v2 is the default pinyin engine** (`inputx_pinyin_v2::enabled()`
   returns `true` with no config file — "Phase 7 final").  Its word
   table is `inputx-pinyin-v2/data/words.tsv` ∪
   `tools/scoring/data/polish/modern_vocab_v1.tsv`; it **never reads**
   `inputx-pinyin/data/library.tsv`.  Class A 加词 that only touches
   library.tsv rebuilds cleanly, passes baseline, and is still
   invisible at runtime — land it on `modern_vocab_v1.tsv` too.
   (`inputx-probe --help` still says "default = v1"; that text is
   stale.)  The library-only rows v2 lacks are v1 corpus 淤血 and are
   **not** a backfill target — user 2026-08-10: "这些 v1 的词都是不
   需要的".
3. **Recently landed on develop:** three-engine data hot-reload
   (`e60e6f2c`), 全角英数 mode + ⇧space toggle + HUD toast
   (`23d407fd`), polish A 蒙板 (`09794bb6`).

**Default next action when user says "继续 autorun":** report this
state + ask direction.  Per the work-picking algorithm below, every
BACKLOG #1 item is either reactive-polish (needs user report) or
trigger-gated (not currently met by anything cleanly autorun-able).

---

## Autorun protocol

Triggered by user input matching `继续 autorun` / `autorun continue`
/ `keep going` / similar standing-instruction phrasing.

**On entry — checklist (in order):**

1. `git status` + `git pull --ff-only origin develop` — sync, fail
   loud on conflict
2. `git log --oneline -5` — orient on recent merges
3. Read `docs/BACKLOG.md` "NEXT-ACTION" block above
4. Read `.claude/PLAN.md` for current-cycle hot work
5. Check memory: any `[[feedback-…]]` / `[[project-…]]` entries
   relevant to upcoming work
6. Branch out to do work per the algorithm below

**Work-picking algorithm:**

| Condition | Action |
|---|---|
| Active `feature/*` or `polish/*` branch exists locally + unmerged | Resume that branch (read its commits, ask what next step) |
| BACKLOG has an item marked "ready to start" without trigger | Pick it, branch, execute per git-flow |
| BACKLOG #1 is "reactive polish queue" (status: waiting for user report) | **Stop and report**: "no autonomous work pending; ready for /polish or feature direction" |
| 入库质量门 trigger met (weekly polish-D1 reports > 5 OR new corpus ingest) | Pick it, ~3.5d work, branch `feature/ingest-quality-filter-impl` |
| v1.6 cement→stone trigger met (touching cement crate for other reason) | Surface as suggestion to user mid-work, don't start unilaterally |
| Other long-term L1+L2 design triggered | Surface as suggestion |
| Nothing applicable | Report current state + ask user for direction |

**Stop-and-report conditions (mid-autorun):**

- Test red after fix attempt — escalate (don't keep flailing)
- Destructive action needed (delete files, push --force, tag, etc.)
- Scope expansion past the branch's stated goal — re-spec first
- External gate (network, credentials, user-only OS step like
  System Settings IME add)

**Push policy:** every merged branch pushes to origin without
confirmation, per `.claude/workflows/git-flow.md`.

**Verify policy:** every code/data change runs through:
- polish work → `make polish-rebuild` (baseline gate)
- framework work → `cargo test -p inputx-core --lib --release`
- mac IME work → `mac/reinstall.py` (5-step verify + probe-test)

---

## Priority order — actually-pending items only

(Shipped items archived at bottom under "Recently shipped".)

### 1. Reactive polish queue (ambient — waiting on user reports)

**Status:** continuous; runs per `/polish` skill invocation. Cannot
autorun without user input.

**Workflow:** per polish action a `polish/<class>-<slug>` branch
(probe → classify → minimal data edit → polish-rebuild → test →
reinstall → commit → merge → push).

**Recent history (today):** jianti (B+D1), biji (Phase I-ranked
fix), jiaozhu (4 D1 + 1 C), daizhe (A), jieou + qedi 解耦 (2 A),
WU-π cherry-pick 22 wubi phrases.

---

### 2. 入库质量门 — proactive corpus noise filter

**Status:** design doc complete (`docs/PLAN-ingest-noise-filter.md`),
**awaiting trigger** — currently NOT autoruning.

**Effort:** ~3.5d.

**Triggers to start (autorun-eligible if either met):**
- Weekly polish-D1 reports > 5 (sustained, not single-day spike)
- New external corpus harvest opportunity (jieba upgrade, mozc
  re-import, etc.)

**Sub-phases:** NF1..NF6 (within-doc indexing only, not project-
level identifiers). See doc §5.

---

### 3. v1.6 — cement → stone refactor (long-term, opportunistic)

**Status:** deferred 2026-06-01, opportunistic resume per user
2026-06-06: "v1.6 是长期任务，发现有可以做的就告诉我".

**Effort per slice:** small (~hours each); whole refactor ~3d.

**Triggers to surface to user (don't autorun unilaterally):**
- Touching an `inputx-*-cement` crate for another reason
- Adding a new published crate (would need fresh -data / -helpers
  rather than another cement)
- Free capacity between substantive work

**Detail:** `.claude/PLAN-v1.6.md`.

---

### 4. Long-term L1+L2 designs (no active code, await trigger)

| Doc | Topic | Trigger to consider |
|---|---|---|
| `.claude/PLAN-self-built-fsa.md` | self-built FSA replacing `fst` crate | binary-size or perf regression that traces to `fst` |
| `.claude/PLAN-unified-scoring.md` | unify scoring entry points behind one trait | adding a 4th engine (Korean? Vietnamese?) makes the duplication painful |
| `.claude/PLAN-rule-engine.md` | rule engine abstraction (policy vs data) | reactive polish rule count explodes past what tier_overlay + exclusions can express |
| (no doc yet) | 繁体 mode toggle | user wants access to the 570 orphan TRAD chars currently swept out of wubi (see `docs/wubi-trad-sweep-2026-06-06/`) — needs a session-level flag + dispatch filter, parallel to `set_japanese_enabled` |

None blocking; surface when a real driver emerges.

---

### 5. iOS — SHELVED

**Status:** shelved per user 2026-06-06 ("Q3 先搁置").

When this comes off the shelf, create `feature/ios-shipping` branch
and write a dedicated `docs/PLAN-ios-ship.md` to track WUs.

---

## Workflow

All substantive work follows git-flow. Full convention + cheat-sheet
in **`.claude/workflows/git-flow.md`** (gitignored — operational
context, not project artifact). One-line:

> Branch off `develop` with `<prefix>/<slug>` (feature / polish /
> bugfix / infra / docs / hotfix), commit, merge back with `--no-ff`,
> delete branch, push to origin.

---

## Naming convention (2026-06-06)

- **New work:** descriptive names (e.g. 音节意识细化, 入库质量门,
  cement → stone refactor). NOT single-letter Phase labels.
- **Historical:** Phase B..I (shipped ranking-model phases) preserved
  in git log; do not reuse the letters for new work.
- **Version cycle sub-units:** WU-π/ρ/σ allowed *within* a version
  cycle (meaningful only there), not as standalone work-item names.

---

## Companion docs (deeper specs)

- `.claude/PLAN.md` — current cycle scaffold (v1.14 open scope)
- `.claude/PLAN-roadmap.md` — cross-version L2 table
- `.claude/PLAN-v1.9-archived.md` — v1.9 codename → shipped v1.13.0 history
- `.claude/PLAN-v1.6.md` — cement→stone refactor (deferred)
- `.claude/PLAN-self-built-fsa.md` / `PLAN-unified-scoring.md` /
  `PLAN-rule-engine.md` — long-term L1+L2 designs
- `docs/PLAN-ingest-noise-filter.md` — 入库质量门 (proactive corpus filter)
- `docs/PLAN-syllable-aware-pinyin.md` — 音节意识细化 spec (impl shipped v1.13.0)
- `docs/v1.9.0-vNEXT-audit.md` — WU-π audit + close-out
- `docs/v1.9.1-wu-rho-audit.md` — WU-ρ baseline-diff audit
- `docs/macos-ime-recipe-2026.md` — macOS IMK lifecycle (LaunchAgent retired §)
- `data/v14-baseline-fixtures/v1.9-snapshot.json` — current baseline
  (regenerate with `rebuild-snapshot.py` at each cycle boundary)

---

## Recently shipped (since 2026-06-06 — historical, no action)

### v1.14 (open cycle, 2026-06-06 → ongoing)

Refreshed 2026-06-07.  All items here landed on `develop`; no tag
cut yet.

**Framework / engine:**

- **K-best 3-segment chain gate** (`e41174f`) — user report
  `luyaozhi → 路要职`.  Inter-bigram NGM blob (1.27 MB,
  `bigrams_inter.ngm`, `--min-count 15`) + `combined_bigram_log_
  prob_q4` helper + strict-all rule replacing ceil-half over
  combined intra+inter signal.  Sister bin
  `build-inter-bigrams-ngm` regenerates the blob from
  `bigrams_inter.tsv` on future corpus refresh.
- **Path 1c 5-char syllable-tail check** (`7c973df`) — user
  report `tkinn` returning 10 t-k-initials phrases.  At buffer
  length 5 the consonant-prefix rescue now requires the suffix
  to be a plausible pinyin syllable tail.  4-char buffers
  (`pyin`, `xlab`) untouched.
- **Pinyin minimal-paths debug mode** (`65bc010` +
  `9d88692`) — three `pub(crate) const bool` toggles
  (`PINYIN_DISABLE_COMPOSE / ASSOCIATION / FUZZY`) pause each
  family.  Tests gated to early-return on the same const, so
  flipping back to `false` auto-revives assertions.  Path 0a
  (repeated-letter) reclassified into ASSOCIATION per user
  "重复字母不应该是简拼拼接出来的吗".  Behavior reference:
  `docs/pinyin-pipeline-gates.md`.

**Polish — per-buffer:**

- (fa, 载) tier-5 demote revert (`33485d9`) — wubi 二级简码
  must lead at `fa`.
- jianma → 简码/键码 boost (`2bbf348`); same-day refined to
  D1/D2 cleanup of 捡骂/剑麻 (`7d1297a`); then 简码 tier-1 boost
  vs JP exact-prefix kana (`c24664c`).
- Add 体感 at tigan (pinyin) + wsdg (wubi, `53320cc`).
- Regen-blob fallout commit (`063863a`) — discovered the polish
  workflow gap that's now memorialized in
  [[feedback-polish-bundle-regen-blobs]].

**Polish — systemic sweeps:**

- **wubi 日本新字体 (Shinjitai) sweep** (`17b3764`) — user report
  `mid → 巌` "这是什么字啊".  Follow-up to the 繁体 sweep below: it
  used opencc `t2s`, which leaves Japanese Shinjitai (巌 亀 両 伝 児
  図 団 …) untouched (`t2s(c)==c`).  v3 upgrades the norm function to
  `t2s ∘ jp2t` (Shinjitai→Trad→Simp) + a **GB2312 whitelist** guard
  against opencc over-reach onto real Simplified chars (欠 予 芸 糸
  醋 疏 沪 浜 缶 弁 — all GB2312, protected).  217 chars stripped
  from auto_decomp.txt, 218 overlay rows from library.tsv, all logged
  to corpus_garbage_filter_v1.tsv.  Audit + re-runnable v3 script:
  `docs/wubi-jp-shinjitai-sweep-2026-06-08/`.
- **日本新字体 Japanese-only backfill** (`514a751`) — follow-up to
  the wubi sweep above, per user "你清理的同时，还要保证他们在日语
  输入中能顺利正确打出来".  Orthogonal-table call: the 217 chars are
  noise in zh engines but real Japanese kanji.  Stripped all 217
  single-char noise rows from *pinyin* library.tsv (+217 garbage-
  filter rows); backfilled the 153 not already in nihongo as single
  kanji with on/kun readings from **KANJIDIC2** (EDRDG) — chosen over
  mozc cache (which carries name/place noise like みにく→亜).  Every
  generated romaji code round-trip-verified against the engine's own
  romaji table (316/316, 0 drift).  All 217 now type-able in
  JapaneseOnly (iwa→巌, sai→砕, …), gone from pinyin.  Scripts +
  audit: `docs/wubi-jp-shinjitai-sweep-2026-06-08/` (gen_nihongo_
  readings.py, apply_nihongo_pinyin.py, roundtrip_verified.tsv).
- **wubi 繁体 sweep** (`b63d6dd`) — user report `yngk → 詞 /
  词 / 肇事 / 启事`, then "你系统解决吧".  Strip 3527 TRAD
  chars from `auto_decomp.txt` whose simplified counterpart is
  reachable via any wubi source.  570 orphan TRAD chars KEPT
  (no same-source simp peer; deleting would silently kill wubi
  lookup for those chars).  Full audit at
  `docs/wubi-trad-sweep-2026-06-06/`.  The orphan-rescue path
  is the 繁体 mode toggle backlog item (BACKLOG §4).
- **pinyin er→r typo sweep** (`1ee80d7`) — user report
  `zhonghuarnv → 中华儿女`.  Detection: word at code
  `<X>r<Y>` AND the SAME word at `<X>er<Y>` ⇒ corpus
  encoding dropped `e` from mid-word `er`.  165 such rows
  deleted from library.tsv + logged in
  `corpus_garbage_filter_v1.tsv`.  Audit:
  `docs/pinyin-er-typo-sweep-2026-06-06/`.

**Docs / process:**

- BACKLOG cycle-landing breadcrumbs (`35feaa3 / 3b329a5`),
  superseded by this 2026-06-07 refresh.
- Path Nx → Stage N rename pass, reverted, replaced with prose
  reference doc (`e4de045 → 285fd03 → c532ee6`).  Per user
  "保持全是 const PINYIN_XXX 挺好的，只是文档里写一下具体是干
  什么的，别再用 Path X 这种代称了".
- Memory: [[project-pinyin-minimal-debug-state]] +
  [[feedback-polish-bundle-regen-blobs]] saved during this
  cycle.

Test gates at HEAD: baseline 50/50, lib 317/0, v1.9-snapshot
drift unchanged from the audited WU-ρ list.

### v1.13.0 ship summary

Tag annotated, on develop HEAD. `git tag -l v1.13.0 -n100` for full
release notes. Scope:

- **engine**: 音节意识细化 (Path 1c gate + Path 3b trim-retry),
  Phase I (wubi full-code redundancy), ASCII-fallback ↔ Path 1c
  reconciliation
- **dict**: WU-π cherry-pick 22 wubi phrase gaps; reactive polish
  queue (jianti / biji / jiaozhu / daizhe / 解耦)
- **mac IME**: LaunchAgent retired (single-spawn via
  imklaunchagent), atomic bundle swap, stray-LS sweep, NSStatusItem
  retired (settings consolidated to IMK menu + SettingsWindow)
- **process**: git-flow workflow locked, BACKLOG.md as SoT,
  descriptive naming convention retiring single-letter Phase labels

Test gates at ship: baseline 46/46, lib 312/0, mac/reinstall.py
clean (probe returned 你好), 1000-query fixture 60/69 unchanged top-1
with 9 documented intentional drift cases.

### v1.10 / v1.11 / v1.12 ship history

See `git tag -l vX.Y.Z -n100` per tag. Themes: v1.10 TOML data
source for ranking knobs; v1.11 Polish workflow tooling; v1.12
Telemetry-driven calibrate framework.

---

## How to add to this backlog

1. New work surfaces → identify which section it belongs to.
2. Add bullet under that section with: status / trigger / effort /
   branch name (if known).
3. If it doesn't fit any existing section, add a new section with
   linear priority placement.
4. Re-order if priority shifts; new items go to bottom of section.
5. **When shipped**: move to "Recently shipped" footer, NOT delete.
   Future-you needs the audit trail.

Don't let this file grow into a brain dump. Items that haven't
moved in 4+ weeks either get scheduled or get dropped.
