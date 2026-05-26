# Inputx Changelog

## 1.2.0 — 2026-05-26

**Polish + 首版正式发布。** 词库 pipeline 收口到唯一真相源，四维工程指标 baseline 入库，应用 UI 与日语扩展收口，mac dmg 通过 notarize + staple 上线，iOS 保留 self-use sideload 能力。本版本所有 ranking 修复按 `P(W|i) = P(i|W) · P(W)` 概率框架诠释（指导思想见 `.claude/PLAN-probabilistic-model.md`）。

### Scoring / Ranking

Probability-framed tunes; each entry tweaks `P(i|W)` (likelihood) or `P(W)` (prior) for a specific case.

- `aiyi → 东京`: full-code wubi exact beats same-tier pinyin (`LIKELIHOOD_WUBI_FULL_CODE` × 1.1)
- `shinjuku → 新宿`: JP full-match jukugo beats forced Chinese composition (`LIKELIHOOD_JP_FULL_MATCH` × 1.3)
- `shinjuk → 新宿`: JP jukugo prefix prediction (CP-A of `PLAN-prefix-prediction`), `predict_score = proximity^K · freq`
- `jixu → 曳光弹` regression fix: full-code wubi promote 1.2 → 1.1 (rare wubi coincidence no longer outscores high-prior pinyin)
- `jieji / jieshou / jie` JP `時へ時` pollution: `composed` flag blocks Viterbi from polluting cross-engine
- `是嗯据库` etc junk Viterbi compositions: per-char quality gate now drops at generation, not sinks; pollution blacklist added
- `yongzhong`: exact dict phrase beats forced composition; suppressed wubi dropped
- Pure-kana "jukugo" (`えっ` / `ありがとう`) drops to single-kanji tier
- Pinyin prediction trigram threshold 50 → 15 (`lianxiang → 联想` fires reliably)
- `kaopu → 靠谱`: Path 5 composition outranks mechanical JP kana in Mixed+JP
- `woyao → 我要`: particle-kana-led kanji junk rejected
- ASCII fallback: long invalid input commits as ASCII (matches Sogou wubi behavior)

### Japanese plugin

- **Default ON** (was default OFF in 1.1.x). UI footer updated; users can disable in Settings if not needed.
- Mozc `dictionary_oss` (BSD-3) import: jukugo 6.7k → 27.4k (+21.7k entries, jukugo.rs 976 KB).
- `東京都`-style place names composed via jukugo + category-suffix kanji (no enumeration explosion).

### Dict pipeline cutover (T0)

- CP1–CP5 (`PLAN-dict-pipeline`): rebuild weights from corpus per `SCORING.md`, identity-aligned to bare `build_weights` output, then evolved.
- Hybrid normalizer (per-source-log-count → global ln+minmax) replaces the 1.1.x ad-hoc chain.
- CP3d pollution filter (long words / traditional / particle fragments / proper-noun double-hit) replaces `strip_*/purge_*.py`.
- LCCC colloquial source + corpus new-word discovery (CP3c) adds 18.5k high-quality words (给力/网红/吐槽/榨干/…). Shipped dict at `core/crates/inputx-pinyin/data/{pinyin.dict,bigrams.fsa,bigrams_intra.fsa,trigrams.dict}`.
- gate1 regression infra (CP3a) + engine-level negatives/known-fail sets.

### Engineering baseline (CP2 of `PLAN-v1.2`)

- Perfgate `< 0.3 ms` on input paths (well under iOS keyboard 2% frame budget).
- Binary / dict size + iOS keyboard extension memory headroom values written to `ios/AppStore-checklist.md` (real-device Instruments still pending — non-blocking).
- `strip = "symbols"` confirmed on release profile; `lto = "off"` retained (iOS linker constraint).

### Engine / robustness

- Mixed-mode backspace: when wubi is frozen at 4 chars and pinyin is longer, re-derive wubi from pinyin (fixes `pinyinggg → 王/珏` desync).
- Pinyin last-resort Viterbi fallback for short non-lexeme buffers (zero-candidate inputs).
- `inputx-fsa` prep'd for crates.io publish (zero-dep, BSD-3 / MIT / Apache-2.0).
- `wubi/layer`: saturate freq on pack + single-source `MAX_FREQ_SCORE`.

### iOS app

- SettingsView footer reflects JP default-on with disable hint.
- L0 learning storage path, autocommit policy, CJK punct cycle, engine-mode picker — all UI-bound, Maestro flows in `ios/maestro/flows/` cover the surface.

### Invariants kept

- 0-dep / offline (keystroke path has no LLM / cloud).
- Wubi position stability (exact full-code / partial-code `P(i|W)` high and definite → always wins #1; prefix prediction additive only).
- WeChat / Notes etc host-app crash class bugs: none open.

---

## 1.1.1 — earlier

See git tags `v1.0.0` / `v1.0.1` / `v1.1.0` / `v1.1.1` and corresponding history.
