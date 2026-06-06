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

### 1. ✅ 音节意识细化 — SHIPPED in v1.13.0

Spec: `docs/PLAN-syllable-aware-pinyin.md`. Implementation landed
2026-06-06 (commits 3ad6239 + merge a392abc). Tag v1.13.0 ships it.

Behaviors confirmed: shehv/shehb/shehz → top-1 = 社会 (matches sheh
trim-retry), pyin still gets Path 1c rescue, hello/qwxzy still
ASCII-fallback. Spec → implementation 1:1 with two intentional
adjustments (Path 3b position after Path 5 to preserve compose
path; buf.len() ∈ [4,5] gate to avoid polluting JP-shaped long
buffers).

---

### 2. v1.9 cycle — ✅ COMPLETED in v1.13.0 ship

All sub-versions landed 2026-06-06:

| WU | Status | Closing artifact |
|---|---|---|
| **WU-π** dict-pipeline audit + 22-phrase cherry-pick | ✅ | `docs/v1.9.0-vNEXT-audit.md` close-out section |
| **WU-ρ** baseline fixture diff vs vNEXT | ✅ | `docs/v1.9.1-wu-rho-audit.md`; snapshot regenerated as `v1.9-snapshot.json` |
| **WU-σ** promote + tag | ✅ | tag `v1.13.0` (per actual git-tag chronology — see §"Naming reconciliation" below) |
| **WU-τ** v1.6 resume | ⏳ deferred per opportunistic policy (§5) |

**Naming reconciliation:** the PLAN.md "v1.9 cycle" codename predates
this session by 5 days. Actual git tag chronology jumped v1.8 → v1.10
→ v1.11 → v1.12 (v1.9.0 was skipped at tag-time). Tagging today's
ship as `v1.9.0` would have caused version-sort confusion; tagged as
`v1.13.0` to continue the real series. `.claude/PLAN.md` should be
archived/rewritten in the next planning session to match the v1.13+
reality.

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

## Cycle telemetry (snapshot 2026-06-06 end-of-day, post-v1.13.0 ship)

| Metric | Value |
|---|---|
| Latest shipped tag | **v1.13.0** (2026-06-06, on develop per project convention) |
| Commits ahead of origin/develop | 0 (synced) |
| Active feature/polish branches | none |
| 音节意识细化 status | ✅ shipped in v1.13.0 |
| WU-π (cherry-pick) status | ✅ shipped in v1.13.0 (22 wubi gaps) |
| WU-ρ (baseline diff) status | ✅ shipped in v1.13.0 (audit doc + v1.9-snapshot.json) |
| WU-σ (promote + tag) status | ✅ shipped — tag v1.13.0 takes the place of "v1.9.0 ship" per actual git tag chronology |
| Reactive polish reports today | 8 (4 D1 + 1 C + 3 A) |
| Reinstall arch incidents today | 4 (all root-caused + permanent fix landed) |
| Naming convention | descriptive names for new work; historical Phase B..I labels preserved in git log only |

## v1.13.0 ship summary (2026-06-06)

Tagged `v1.13.0` on `develop` HEAD (commit 9e57e76, the WU-ρ merge).
Release notes live in the annotated tag message: `git tag -l v1.13.0 -n100`.

Scope highlights:
- engine: 音节意识细化 (Path 1c gate + Path 3b trim-retry), Phase I (wubi
  full-code redundancy), ASCII fallback ↔ Path 1c reconciliation
- dict: WU-π cherry-pick 22 wubi phrase gaps; reactive polish queue
  (jianti / biji / jiaozhu / daizhe / 解耦)
- mac IME: LaunchAgent retired (single-spawn architecture via
  imklaunchagent), atomic bundle swap, stray-LS sweep, NSStatusItem
  retired (settings consolidated to IMK menu + SettingsWindow)
- process: git-flow workflow locked, BACKLOG.md as SoT, descriptive
  naming convention

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
