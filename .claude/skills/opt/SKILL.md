---
name: opt
description: Optimize candidate tables for one or all input engines. Each engine's optimize step is idempotent (running twice in a row produces no diff) and side-effect-free (does NOT auto-commit, does NOT reinstall). Usage `/opt [py|wb|jp]` — empty arg means all three.
argument-hint: py | wb | jp | (empty = all)
---

# /opt — Protocol

A single high-level command that runs each engine's "optimize candidate dict" step on demand. Designed to be safe to invoke at any time, by anyone, without thinking: re-running produces no churn, never commits anything, never deploys anything.

## Parse `$ARGUMENTS`

Strip whitespace. Match against:

| arg          | targets       |
|--------------|---------------|
| `""` (empty) | py + wb + jp  |
| `py`         | pinyin only   |
| `wb`         | wubi only     |
| `jp`         | japanese only |
| anything else | print usage block below, stop |

If multiple letters concatenated (e.g. `pyjp`), reject and print usage — keep parsing strict so future expansion (e.g. `--no-audit`) doesn't collide.

## Execution order

When running multiple engines, run **py → wb → jp**. Each engine is independent; failure in one doesn't abort the others (log and continue).

For each targeted engine, emit a one-line section header: `## opt: <engine>` so the operator can scan results easily.

## py (pinyin) — full refresh

Run the existing `tools/scoring/refresh.sh` from the project root. That script already composes the 4-step refresh (expand / rescore / audit / cutoff+rebuild) and is documented in `.claude/INTERNAL_TOOLS.md`.

```bash
cd <project-root>
./tools/scoring/refresh.sh
```

After it exits, run `git -C <project-root> diff --stat tools/scoring/data/ core/crates/inputx-pinyin/data/`. Echo that stat (or "no changes") to the operator.

Do NOT pass `--reinstall`. /opt's contract is: no deploys, no commits. Operator decides whether to ship.

## wb (wubi) — currently a no-op

Wubi has no polish-log feedback loop or overlay mechanism today — its dict is generated deterministically from corpus + the encoding tables, and the user's "pick rate" doesn't carry signal the same way pinyin's does (most wubi codes resolve uniquely).

Print: `[opt wb] no refresh mechanism exists yet. Edit core/crates/inputx-wubi/data/* directly and rebuild via cargo if you need a change.` Treat as success (no error exit).

Future: when a wubi-polish equivalent of `aggregate_polish_log.py` lands, hook it here.

## jp (japanese) — regen kanji + jukugo

Re-run the two JP generators in order. Both are idempotent — they read source TSVs under `tools/scoring/data/supplemental/` (jp_kanji_v1.tsv, jp_jukugo_v1.tsv, jp_counters_v1.tsv, jp_brands_v1.tsv) and rewrite `core/crates/inputx-nihongo/src/{kanji,jukugo}.rs`. If sources haven't changed, the output is byte-identical.

```bash
cd <project-root>
python3 tools/nihongo/build_kanji_rs.py
python3 tools/nihongo/build_jukugo_rs.py
```

After both, run a sanity grep for punctuation that shouldn't be in the word column (these slip in when polish-log-style sources echo old data — see commit `d863434`):

```bash
grep -cE '[？！?!，。、…〜～♪♥★☆「」『』]' core/crates/inputx-nihongo/src/jukugo.rs
```

Should print `0`. If nonzero, list the offending entries with line numbers — they indicate the source TSV needs a manual cleanup pass.

Run `git -C <project-root> diff --stat core/crates/inputx-nihongo/src/`. Echo the stat.

## Output contract — what /opt prints

For each engine ran, one `## opt: <engine>` section. Within it:
- Brief status line (e.g. `pinyin.fst: 4809045 → 4809045 bytes (no change)`)
- If `git diff --stat` shows changes, list them
- If any sub-step failed, surface the error verbatim with which engine + which step

End with one summary line: `ran: py, wb, jp — N files changed`. If N is 0, the dict is already at target state.

NEVER commit. NEVER deploy. NEVER `git add`. If the operator wants to ship, they read the diff and act themselves.

## Idempotency contract — must hold

Re-running `/opt` (same args) immediately after a successful run must produce zero `git diff` lines. This is a property of:
- pinyin: refresh.sh internals (already verified idempotent)
- wubi: no-op (trivially idempotent)
- jp: generators read source TSVs deterministically

If a sub-step ever stops being idempotent (e.g. timestamp baked into output, RNG seeding from clock), fix the sub-step. Don't make this skill paper over non-determinism.

## Why this exists

The user wanted one high-level entry point for "make the candidate dict better" so they don't have to remember `refresh.sh` vs `build_jukugo_rs.py` vs which dir to cd into. `/opt` is the universal answer:

- "拼音不准了" → `/opt py`
- "日语候选有奇怪东西" → `/opt jp`
- "都顺手优化一下" → `/opt`
- Even if nothing's wrong: safe to run, won't hurt.

## Usage

```
/opt        — optimize all three (py + wb + jp), no commit, no deploy
/opt py     — pinyin only
/opt wb     — wubi only (currently a no-op stub)
/opt jp     — japanese only
```

After /opt: review `git diff` (the skill prints the stat); if you want to ship, commit + run `./mac/reinstall.sh` yourself.
