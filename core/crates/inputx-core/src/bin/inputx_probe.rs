//! `inputx-probe` — engine introspection CLI for the dev webserver.
//!
//! Feeds a buffer string into a `Session` keystroke-by-keystroke and
//! emits a JSON document describing the resulting candidate list with
//! per-candidate source attribution. The dev web UI uses this to show
//! *why* the engine ranks the way it does for any input.
//!
//! Usage:
//!     cargo run --release --bin inputx-probe -- <buffer> [--mode mixed|wubi|pinyin|japanese] [--jp]
//!
//! Output schema (stdout, JSON):
//!   { "buffer": "jixu",
//!     "mode": "Mixed",
//!     "japaneseEnabled": false,
//!     "preedit": "jixu",
//!     "candidates": [
//!       {"word": "继续", "source": "Pinyin", "score": 444652.0},
//!       {"word": "新宿", "source": "Japanese", "score": 437000.0,
//!        "base": 200000.0, "prior": 264000.0, "likelihood": 0.67},
//!       ...
//!     ]
//!   }
//!
//! `base` / `prior` / `likelihood` are present only when the candidate
//! flowed through `predict_score_with_components` (CP-A JP jukugo prefix,
//! CP-B pinyin Path-3, CP-C wubi prefix-prediction) — v1.3 WU-γ
//! two-axis decomposition. Invariant when present:
//!   `base + prior * likelihood` is the *pre*-bigram_bonus /
//!   pre-promote score; the emitted `score` may exceed this by an
//!   additive bigram_bonus (pinyin) or multiplicative full-match
//!   promote (JP).

use std::env;
use std::process::ExitCode;

use inputx_core::{AutoCommitPolicy, EngineMode as Mode, ScoreComponents, Session, Source};

fn print_help() {
    eprintln!("inputx-probe — engine introspection CLI\n");
    eprintln!("Usage:");
    eprintln!("    inputx-probe <buffer> [--mode mixed|wubi|pinyin|japanese] [--jp]");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("    inputx-probe jixu");
    eprintln!("    inputx-probe chongming --mode pinyin");
    eprintln!("    inputx-probe yama --jp");
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args[1] == "-h" || args[1] == "--help" {
        print_help();
        return ExitCode::from(2);
    }

    let buffer = args[1].clone();
    let mut mode = Mode::Mixed;
    let mut jp_on = false;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("--mode requires a value");
                    return ExitCode::from(2);
                }
                mode = match args[i].as_str() {
                    "mixed" => Mode::Mixed,
                    "wubi" => Mode::WubiOnly,
                    "pinyin" => Mode::PinyinOnly,
                    "japanese" | "jp" => Mode::JapaneseOnly,
                    other => {
                        eprintln!("unknown mode: {other}");
                        return ExitCode::from(2);
                    }
                };
            }
            "--jp" => {
                jp_on = true;
            }
            other => {
                eprintln!("unknown arg: {other}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    let mut sess = Session::new();
    sess.set_auto_commit_policy(AutoCommitPolicy::Never);
    sess.set_mode(mode);
    sess.set_japanese_enabled(jp_on || matches!(mode, Mode::JapaneseOnly));
    for b in buffer.bytes() {
        // Forward ASCII letters AND `-` (chōonpu / long-vowel mark for JP).
        // Other punctuation still skipped — `inputx-probe foo,bar` should
        // probe `foobar`, not `foo,bar`.
        if !b.is_ascii_alphabetic() && b != b'-' {
            continue;
        }
        sess.handle_key(b as u32, 0);
    }

    let cands = sess.candidates();
    let mode_str = format!("{:?}", mode);

    let mut out = String::with_capacity(512 + cands.len() * 32);
    out.push('{');
    push_kv_string(&mut out, "buffer", &buffer);
    out.push(',');
    push_kv_string(&mut out, "mode", &mode_str);
    out.push(',');
    push_kv_bool(&mut out, "japaneseEnabled", sess.japanese_enabled());
    out.push(',');
    push_kv_string(&mut out, "preedit", sess.preedit());
    out.push(',');

    out.push_str("\"candidates\":[");
    for (idx, word) in cands.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let src = sess.candidate_source(idx)
            .and_then(Source::from_u8)
            .map(source_str)
            .unwrap_or("Wubi");
        out.push('{');
        push_kv_string(&mut out, "word", word);
        out.push(',');
        push_kv_string(&mut out, "source", src);
        if let Some(score) = sess.candidate_score(idx) {
            out.push(',');
            push_kv_f64(&mut out, "score", score);
        }
        if let Some(c) = sess.candidate_components(idx) {
            out.push(',');
            push_kv_components(&mut out, c);
        }
        out.push('}');
    }
    out.push(']');
    out.push('}');

    println!("{out}");
    ExitCode::SUCCESS
}

fn source_str(s: Source) -> &'static str {
    match s {
        Source::Wubi => "Wubi",
        Source::Pinyin => "Pinyin",
        Source::Japanese => "Japanese",
    }
}

fn push_kv_string(out: &mut String, key: &str, value: &str) {
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    push_json_string(out, value);
}

fn push_kv_bool(out: &mut String, key: &str, value: bool) {
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    out.push_str(if value { "true" } else { "false" });
}

fn push_kv_f64(out: &mut String, key: &str, value: f64) {
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    // Round to 3 decimals to keep the JSON compact + readable; the
    // exact bit pattern is recoverable by re-running the engine with
    // the same buffer (everything is deterministic).
    out.push_str(&format!("{:.3}", value));
}

fn push_kv_components(out: &mut String, c: ScoreComponents) {
    // v1.3 (base, prior, likelihood) linear-space decomposition. Carries
    // a real `score == base + prior · likelihood` invariant for the
    // predict_score chain (CP-A JP / CP-B pinyin / CP-C wubi prediction).
    // Synthetic three-axis fill paths (exact / composed / fuzzy /
    // fallback) leave these zero — read those candidates' tier from the
    // v1.4.2 fields below.
    push_kv_f64(out, "base", c.base);
    out.push(',');
    push_kv_f64(out, "prior", c.prior);
    out.push(',');
    push_kv_f64(out, "likelihood", c.likelihood);
    out.push(',');
    // v1.4.2 WU-γ inputx-scoring three-axis (Q4 log-space). Populated by
    // every fill point in composite/{dispatch,pinyin_adapter,
    // japanese_adapter}.rs. `log_prior_q4 + log_likelihood_q4` is the
    // log-space additive score that the v1.4.5+ cement layer will use
    // as the sort key; today the legacy f64 `score` is still authoritative.
    push_kv_i32(out, "log_prior_q4", c.log_prior_q4);
    out.push(',');
    push_kv_i32(out, "log_likelihood_q4", c.log_likelihood_q4);
    out.push(',');
    push_kv_match_type(out, c.match_type);
}

fn push_kv_i32(out: &mut String, key: &str, value: i32) {
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    out.push_str(&value.to_string());
}

fn push_kv_match_type(out: &mut String, mt: inputx_scoring::MatchType) {
    out.push_str("\"match_type\":");
    match mt {
        inputx_scoring::MatchType::Exact => out.push_str("\"Exact\""),
        inputx_scoring::MatchType::Prefix(prox) => {
            out.push_str(&format!("{{\"Prefix\":{prox}}}"))
        }
        inputx_scoring::MatchType::Fuzzy(cost) => {
            out.push_str(&format!("{{\"Fuzzy\":{cost}}}"))
        }
        inputx_scoring::MatchType::Composed { bigram_links } => {
            out.push_str(&format!("{{\"Composed\":{{\"bigram_links\":{bigram_links}}}}}"))
        }
        inputx_scoring::MatchType::Initials { typed_len, full_len } => {
            out.push_str(&format!(
                "{{\"Initials\":{{\"typed_len\":{typed_len},\"full_len\":{full_len}}}}}"
            ))
        }
    }
}

fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
