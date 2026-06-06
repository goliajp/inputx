# Inputx Backlog

> Source of truth for "what's next" — linearized list, ordered by user
> priority. Updated whenever an item lands or the priority shifts.
>
> **Naming convention (2026-06-06):** new work uses descriptive names
> (音节意识细化 / 入库质量门 / cement→stone refactor). Single-letter
> labels like "Phase A..I" are reserved for already-shipped historical
> phases (git log) — never used for new items. Version-cycle sub-units
> like `WU-π/ρ/σ` keep their Greek letters because they're scoped to
> a specific version (v1.9) and meaningful only there.
>
> **Companion docs (deeper specs live there, this file links out):**
> - `.claude/PLAN.md` — v1.9 cycle detail
> - `.claude/PLAN-roadmap.md` — cross-version roadmap (L2 version table)
> - `docs/PLAN-syllable-aware-pinyin.md` — 音节意识细化 full spec
> - `docs/PLAN-ingest-noise-filter.md` — 入库质量门 (proactive corpus filter)
> - `.claude/PLAN-v1.6.md` — cement → stone refactor (deferred)
> - `.claude/PLAN-self-built-fsa.md`, `.claude/PLAN-unified-scoring.md`,
>   `.claude/PLAN-rule-engine.md` — long-term L1+L2 designs

---

## Workflow

All substantive work follows git-flow. Full convention + cheat-sheet
lives in **`.claude/workflows/git-flow.md`** (gitignored — operational
context, not project artifact). One-line summary:

> Branch off `develop` with `<prefix>/<slug>` (feature / polish /
> bugfix / infra / docs / hotfix), commit, merge back with `--no-ff`,
> delete branch, push to origin. No user confirmation needed.

---

## Priority order (top = do next)

### 1. 音节意识细化 — pinyin syllable-aware engine refinement

**Status:** spec landed (`docs/PLAN-syllable-aware-pinyin.md`),
implementation pending on `feature/syllable-aware-impl` branch.

**Why #1:** user-visible quality lift on the current dict, design
already complete, ~2.5h implementation. Closes the `shehv` / `shehb`
/ `xianv` class where the initials-fallback typo rescue misfires
for buffers that have a clean ≥3-char syllable prefix — those
should fall through to a trim-retry prefix-completion (= matching
`sheh` behavior), not surface sh+h 2-syllable noise (时候/生活/…).

**Effort:** ~2.5h per spec §10 estimate.

**Action items:** entire §7 file-by-file change plan in the spec.
Implementation must track the spec 1:1; deviations update the
spec first.

---

### 2. v1.9 cycle — ship new dict to release

**Status:** PLAN-marked "hot" since 2026-06-01, **all sub-versions
未启动**. Recent weeks went into polish + reinstall architecture +
ranking model Phase H/I — none of those touched the v1.9 dict
pipeline scope. v1.9 stays the current named cycle.

**Sub-versions:**

| WU | Branch | Content | Effort |
|---|---|---|---|
| **WU-π** v1.9.0 | `feature/v1.9-wu-pi-pipeline-rerun` | full pipeline rerun → vNEXT + audit | 2 d |
| **WU-ρ** v1.9.1 | `feature/v1.9-wu-rho-baseline-diff` | baseline fixture diff vs vNEXT + drift gate | 2 d |
| **WU-σ** v1.9.2 | `feature/v1.9-wu-sigma-promote-ship` | promote vNEXT, version bump, release tag | 1 d |
| **WU-τ** v1.9.3 (optional) | `feature/v1.9-wu-tau-v16-resume` | resume v1.6 if capacity allows | 3 d |

**Detail:** `.claude/PLAN.md` (single source of truth for v1.9).

---

### 3. Reactive polish queue (ongoing, ambient)

**Status:** continuous — runs whenever user reports a (buffer, word)
ranking imperfection via the `/polish` skill.

**Workflow:** per polish action a `polish/<class>-<slug>` branch,
following the `polish` skill protocol (probe → classify → minimal
data edit → polish-rebuild → test → reinstall → commit → merge to
develop → push).

**No fixed effort** — driven by user reports. Recent history (this
week): jianti (B+D1), biji (Phase I), jiaozhu (4 D1 + 1 C),
daizhe (A), jieou + qedi 解耦 (2 A).

---

### 4. 入库质量门 — proactive corpus noise filter

**Status:** design doc landed (`docs/PLAN-ingest-noise-filter.md`),
**awaiting architecture review** before scheduling.

**Why later than 音节意识细化:** 音节意识细化 is ~2.5h with immediate
user-visible payoff. 入库质量门 is 3.5 days of infrastructure work
— the payoff is "fewer future polish reports", which only matters
if polish reports actually become a sustained burden. Right now they
average a few a week — manageable via the reactive queue. 入库质量门
takes over once that frequency starts hurting.

**Trigger to start:** 1 of:
- Weekly polish-D1 reports exceed ~5
- New external corpus comes online (jieba upgrade, mozc, ...)
  triggering re-ingestion → opportunity to bake the filter into
  the new ingest path

**Sub-phases:** the design doc uses internal labels NF1..NF6
(lexicon selection → absorption pipeline integration) purely as
within-doc indexing — they're not promoted to project-level
identifiers. See doc §5 for the breakdown.

---

### 5. v1.6 — cement → stone refactor (long-term, opportunistic)

**Status:** deferred 2026-06-01 by user, **resume opportunistically**.
User directive 2026-06-06: "v1.6 是长期任务，发现有可以做的就告诉我".

**Why opportunistic, not scheduled:** engineering housekeeping, no
user-visible behavior change. Best done in slices that piggy-back
on natural ingress points (a new crate added → migrate it cleanly;
a cement crate touched for a real reason → also do the rename).

**Triggers to surface to user:**
- Touching a `inputx-*-cement` crate for another reason
- Adding a new published crate (would need fresh -data / -helpers
  rather than another cement)
- v1.9 WU-τ window if v1.9 finishes ahead of cycle

**Detail:** `.claude/PLAN-v1.6.md`.

---

### 6. Long-term L1+L2 designs (no active code, await trigger)

These have design docs but no active engineering. None are blocking
anything; surface when a real driver emerges.

| Doc | Topic | Trigger to consider |
|---|---|---|
| `.claude/PLAN-self-built-fsa.md` | self-built FSA replacing `fst` crate | binary-size or perf regression that traces to `fst` |
| `.claude/PLAN-unified-scoring.md` | unify scoring entry points behind one trait | adding a 4th engine (Korean? Vietnamese?) makes the duplication painful |
| `.claude/PLAN-rule-engine.md` | rule engine abstraction (policy vs data) | reactive polish rule count explodes past what tier_overlay + exclusions can express |

---

### 7. iOS — SHELVED

**Status:** **shelved** per user 2026-06-06 ("Q3 先搁置").

Engineering state per `README.zh-CN.md`: iOS v1 feature-complete
(Rust engine, dual-engine router, L0 persistence, locale, iOS
keyboard + Settings UI all implemented + tested), remaining work
was real-device validation + perf profiling + TestFlight + App
Store submission.

When this comes off the shelf, create `feature/ios-shipping`
branch and write a dedicated PLAN-ios-ship.md to track WUs.

---

## Cycle telemetry (snapshot 2026-06-06)

| Metric | Value |
|---|---|
| Commits ahead of origin/develop | 0 (just synced) |
| Active feature/polish branches | (will be tracked here as they open) |
| v1.9 sub-versions done / total | 0 / 4 |
| 音节意识细化 status | design doc landed, impl pending |
| Reactive polish reports this week | ~8 (4 D1 + 1 C + 3 A) |
| Reinstall arch incidents this week | 4 (all root-caused + permanent fix landed) |

---

## How to add to this backlog

1. New work surfaces → identify which section (1-7) it belongs to.
2. Add bullet under that section with: status / why-this-priority /
   effort / branch name (if known).
3. If it doesn't fit any existing section, add a new section with
   linear priority placement.
4. Re-order sections if priority shifts; bottom of each section is
   newer / less-blocking items.

Don't let this file grow into a brain dump. Items that haven't
moved in 4+ weeks either get scheduled or get dropped.
