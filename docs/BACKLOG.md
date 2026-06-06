# Inputx Backlog

> Source of truth for "what's next". Top section answers "what should
> the assistant do RIGHT NOW given no further user input"; sections
> below are full prioritized backlog with status and triggers.

---

## NEXT-ACTION (read first on session start)

**Latest shipped tag:** `v1.13.0` (2026-06-06)
**Active branch:** `develop` (always at-or-ahead of `master`; master is dormant)
**Active feature/polish branches:** none
**Commits ahead of origin/develop:** 0 (synced after last push)
**Current cycle:** v1.14 (open scope — see `.claude/PLAN.md`)

**Default next action when user says "继续 autorun":** see
**Autorun protocol** section below — but in short: there is no
substantive autoruning work pending. All BACKLOG #1-priority items
are gated on user-report (reactive polish) or trigger-condition (入库
质量门 not yet met, v1.6 opportunistic). Autorun's correct behavior
right now is to **report current state and ask for direction** rather
than invent work.

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
