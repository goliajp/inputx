//! User-curated prior correction table — score multipliers for words whose
//! corpus-derived freq diverges from real-world usage frequency.
//!
//! Why a separate mechanism (not dict edits):
//!   `weights.tsv` is the deterministic output of `build_weights.rs` from a
//!   fixed corpus mix (zho_news + subtlex + zho_wikipedia per
//!   `data/corpus/manifest.toml`). Hand-editing rows there is exactly the
//!   ad-hoc post-process pattern PLAN-dict-pipeline §CP1-step2 flagged as
//!   "the four py scripts not in any orchestrator" — they break the
//!   identity-aligned-to-build_weights contract.
//!
//!   This module is the **scoring-layer** correction: a `(word →
//!   multiplier)` table consulted at merge time. It does NOT mutate dict
//!   data, can be inspected / removed / overridden per release, and will be
//!   subsumed by v1.4's prior-axis re-architecture (LIKELIHOOD_TABLE +
//!   PRIOR_* fields per candidate) once that lands. The contract here is
//!   intentionally narrow: only **multiplicative score adjustments**, only
//!   driven by user polish-log evidence, no per-context logic.
//!
//! Why a separate table from `blacklist.rs`:
//!   - blacklist is binary (drop the word entirely; "片你" isn't a word).
//!   - prior_correction is continuous (the word IS valid, its frequency
//!     estimate just diverges from usage; "积蓄" is a real word that the
//!     news corpus over-represents).
//!
//! Adding a row:
//!   - MUST cite the user polish-log case in the same commit (buffer +
//!     observed top-N + user comment).
//!   - MUST add a regression test in `dispatch.rs` or `session.rs` tests
//!     pinning the expected ranking after the correction applies.
//!   - PREFER small multipliers (0.3-3.0); anything outside that window
//!     suggests the corpus is fundamentally wrong about this word and the
//!     dict-pipeline T0 work should address it instead.

/// `(word, log_prior_boost_q4)` — additive Q4 log-prior boost applied at
/// the merge chokepoint. Calibrated so each canon polish-log case beats
/// its competitor under the v1.4.7 score_q4 sort key (`log_prior_q4 +
/// log_likelihood_q4` Bayesian). Q4=16, so boost = ln(effective_multiplier)
/// · 16 in log space; +11 ≈ ×2.0 linear, +17 ≈ ×2.9 linear, +24 ≈ ×4.5.
///
/// Why Q4 boosts not legacy ×multipliers (v1.4.7 A3 cutover, 2026-05-27):
/// pre-A3 sort key was legacy f64 score `s = base + freq·mult` — applying
/// `s *= correction` boosted whole-score, including the `base` floor. The
/// score_q4 sort key is `log_prior_q4 = Q4·ln(freq)` — no `base` floor in
/// log space — so a legacy ×2 multiplier ≈ +11 Q4 was too small to
/// overcome freq differences that the linear `base` floor previously
/// dampened (e.g. 继续 freq 75k vs 积蓄 freq 166k: linear `base+freq` gave
/// 继续 a near-equal floor that ×2 multiplier could clear; pure log
/// `Q4·ln(freq)` gave 积蓄 a +12 Q4 lead that +11 boost couldn't reverse).
/// Recalibrated values below pin canon polish-log intent directly in
/// log-prior space — each boost = exact Q4 amount needed to beat the
/// canon competitor + small safety margin.
///
/// Keep entries sorted alphabetically by Chinese (for human review), one
/// per polish-log case.
const PRIOR_CORRECTIONS: &[(&str, i32)] = &[
    // 2026-05-26 user polish-log (jixu → 继续 should lead, screenshot
    // showed 积蓄 #1 / 继续 #2). Probe confirmed corpus puts 积蓄 freq at
    // 166k vs 继续 75k — newswire/financial sources inflate 积蓄. Real
    // daily-use frequency 继续 >> 积蓄 (user-attested: "继续还是应该在
    // 第一的，这个感觉比积蓄要高频"). v1.4.7 A3 calibration: 继续 raw
    // log_prior_q4 ≈ 积蓄 raw - 13 Q4 (ln(166k/75k)·Q4 ≈ 12.7); boost +17
    // (≈ ×2.9 linear) puts 继续 +4 Q4 above 积蓄 with safety margin.
    // dict-pipeline CP3+ (corpus reweighting / pollution filter) should
    // eventually neutralize this so the boost goes to 0 / row removed.
    ("继续", 17),
    // 2026-05-26 user polish-log (sheji → 设计 should lead, screenshot
    // showed 涉及 #1 / 设计 #2). Same corpus skew pattern as 继续: news /
    // academic-paper sources over-represent 涉及 vs daily 设计 usage. User
    // attestation: "设计肯定应该高于涉及". Boost +11 (≈ ×2 linear) gives
    // 设计 +9 Q4 lead over 涉及 — comfortable canon margin.
    ("设计", 11),
    // 2026-05-26 baseline-fix after deterministic pinyin.dict rebuild
    // (build_pinyin_modern.py finance expansion exposed PLAN-dict-pipeline
    // "三套混杂" — modern_v1 overlay had `立项` = 50000 which the previous
    // manual-mix .dict didn't reflect). Restore lixiang→理想 #0 ranking.
    // Boost +11 (≈ ×2 linear) puts 理想 +6 Q4 above 立项.
    ("理想", 11),
    // jiazai weights.tsv has 加在 20123 > 加载 18923 but baseline test
    // pins 加载 outranking 加在 (common-use signal: file loading is much
    // more frequent than the "加 in" particle compound). Boost +7
    // (≈ ×1.55 linear) puts 加载 +5 Q4 above 加在.
    ("加载", 7),
    // 2026-05-26 user polish-log (juti → 具体 should lead, screenshot
    // showed 暗送秋波 #1 / 具体 #2). 暗送秋波 is a valid wubi 4-code
    // Phrase entry that takes the LIKELIHOOD_WUBI_FULL_CODE_PROMOTE × 1.1
    // boost (designed for aiyi→东京 same-tier wubi-first wins); but here
    // 具体 corpus freq 36k vs phrase ~12k still loses by ~16k after the
    // promote. User: "具体的常用分应该太高了". Boost +7 (≈ ×1.55 linear)
    // puts 具体 +21 Q4 above 暗送秋波 — decisive canon margin.
    ("具体", 7),
    // 2026-05-27 user polish-log (tongyi → 统一 should lead, screenshot
    // showed 同意 #1 / 同一 #2 / 统一 #3 / 同义 #4 / 通译 #5 / 通义 #6 /
    // 通易 #7). User: "统一应该大于同一，在第二或第一顺位". v1.4.7 A3:
    // 统一 raw log_prior_q4 ≈ 同意 raw - 10 Q4; boost +13 (≈ ×2.25 linear)
    // puts 统一 +3 Q4 above 同意 (#1) with 统一 > 同一 satisfied too.
    ("统一", 13),
];

/// Lookup the user-curated Q4 log-prior boost for `word`. Returns 0 (no
/// correction) when the word isn't on the list. O(N) linear scan — the
/// table is intentionally tiny (corpus-divergence cases are rare; each one
/// is a documented event). If this list exceeds ~50 entries the dict-
/// pipeline calibration probably needs a deeper fix.
pub fn correction_for(word: &str) -> i32 {
    PRIOR_CORRECTIONS
        .iter()
        .find(|(w, _)| *w == word)
        .map(|(_, b)| *b)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_corrections_apply() {
        // v1.4.7 A3 calibration: Q4 log-additive boosts (i32, not f64
        // multipliers). +11 ≈ ×2 linear, +17 ≈ ×2.9 linear, +24 ≈ ×4.5.
        assert_eq!(correction_for("继续"), 17);
        assert_eq!(correction_for("设计"), 11);
        assert_eq!(correction_for("理想"), 11);
        assert_eq!(correction_for("加载"), 7);
        assert_eq!(correction_for("具体"), 7);
        assert_eq!(correction_for("统一"), 13);
        // Untouched words pass through at 0 (no boost).
        assert_eq!(correction_for("积蓄"), 0);
        assert_eq!(correction_for("涉及"), 0);
        assert_eq!(correction_for("立项"), 0);
        assert_eq!(correction_for("加在"), 0);
        assert_eq!(correction_for("暗送秋波"), 0);
        assert_eq!(correction_for("新宿"), 0);
        assert_eq!(correction_for(""), 0);
        // Q4 boost sanity window — ±20 Q4 (≈ ×0.29 to ×3.5 linear).
        // Outside this range, dict-pipeline calibration should handle
        // the case instead. Defensive bound, not enforced at runtime.
        for (_, b) in PRIOR_CORRECTIONS {
            assert!(b.abs() <= 20,
                "prior_correction Q4 boost {b} out of [-20, 20] sanity window; \
                 corpus calibration should handle this case instead");
        }
    }
}
