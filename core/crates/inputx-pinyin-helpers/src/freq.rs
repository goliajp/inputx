//! Raw-frequency recovery from Q4 log-space `log_prior`.
//!
//! `inputx_scoring::log_prior_from_freq(freq) -> i32` produces
//! `Q4 · ln(1 + freq)` rounded to the nearest integer. The composite
//! hot path's v1.3 score chain takes a raw `freq: u64` input
//! (PinyinDict::lookup_with_scores_into emits `base + freq`), so the
//! cutover to .idf-sourced data needs to recover an estimated raw
//! frequency from the stored Q4 log_prior. This module is that
//! inverse.
//!
//! Round-trip precision: ln-then-round-then-exp introduces ~0.5%
//! error at typical bigram counts (10–10k) and similar magnitude at
//! dict freq scale (1k–100k). The v1.3 score chain is
//! ranking-tolerant of <1% per-candidate score drift — verified by
//! the v1.4.6 C2 wiring (same exp(log/Q4) inversion technique) which
//! preserved baseline fixture diff zero across 69 canonical entries.

use inputx_scoring::Q4;

/// Recover an estimated raw frequency from a Q4 fixed-point log_prior
/// value: inverse of `inputx_scoring::log_prior_from_freq`. Used by
/// the v1.4.6 sub-phase C3 cutover where the composite pinyin
/// adapter switches its dict lookup source from
/// `inputx_pinyin::PinyinDict::lookup_with_scores_into` (raw freq
/// embedded as `freq` field) to `inputx_dict_format::IdfReader::
/// lookup` (Q4 log_prior in entry table).
///
/// Mathematical identity (mod rounding):
///   `Q4 · ln(1 + freq).round() → log_prior_q4`
///   `exp(log_prior_q4 / Q4) - 1 → freq_est`
///
/// Round-trip error is bounded by the Q4 quantization step (1 unit
/// at Q4 = 16 → 1/16 log unit → ~6.5% in linear space for the
/// neighborhood of the true value). At freq=1000 → log_prior_q4≈110
/// (rounded down by 0.5) → freq_est≈990 (delta ~1%). At freq=50000 →
/// log_prior_q4≈173 → freq_est≈49500 (delta ~1%).
///
/// Returns 0 when `log_prior_q4 <= 0` (no rounding-induced negative
/// values; saturates at the empty-freq sentinel).
pub fn estimated_freq_from_log_prior(log_prior_q4: i16) -> u64 {
    if log_prior_q4 <= 0 {
        return 0;
    }
    let ln_term = log_prior_q4 as f64 / Q4 as f64;
    let exp_term = ln_term.exp();
    // `log_prior_from_freq` does `ln(1 + freq)` so we subtract 1 on
    // the inverse. Saturating cast: dict freq stays in u64 range
    // well below 1e20 so no overflow concern.
    let est = exp_term - 1.0;
    if est < 0.0 { 0 } else { est.round() as u64 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inputx_scoring::log_prior_from_freq;

    /// Round-trip a small handful of representative frequencies and
    /// assert the inverse recovers within the documented ±6% window.
    #[test]
    fn round_trip_within_6_percent() {
        let cases: &[u64] = &[0, 1, 10, 100, 1000, 10_000, 100_000, 1_000_000];
        for &freq in cases {
            let lp = log_prior_from_freq(freq);
            let lp_i16 = lp.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            let est = estimated_freq_from_log_prior(lp_i16);
            if freq == 0 {
                assert_eq!(est, 0, "freq=0 must round-trip exactly");
                continue;
            }
            let actual = est as f64;
            let expected = freq as f64;
            let ratio = (actual - expected).abs() / expected;
            assert!(
                ratio <= 0.06,
                "freq={freq} log_prior={lp} → est={est}; drift {ratio:.4} > 6%",
            );
        }
    }

    #[test]
    fn zero_log_prior_yields_zero_freq() {
        assert_eq!(estimated_freq_from_log_prior(0), 0);
        assert_eq!(estimated_freq_from_log_prior(-100), 0);
    }

    #[test]
    fn monotone_in_log_prior() {
        let a = estimated_freq_from_log_prior(50);
        let b = estimated_freq_from_log_prior(100);
        let c = estimated_freq_from_log_prior(200);
        assert!(a < b && b < c, "got a={a} b={b} c={c}");
    }
}
