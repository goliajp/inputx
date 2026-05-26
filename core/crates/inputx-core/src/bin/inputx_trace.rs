//! `inputx-trace` — debug introspection of the rule engine.
//!
//! v3.0.1 (2026-05-24): scaffolding-only — the rule registry is empty,
//! so right now this binary just dumps the context that WOULD be fed
//! to rules. Each v3.0.x checkpoint adds actual rule firings as
//! migration proceeds.
//!
//! Usage:
//!
//!     cargo run --bin inputx-trace -- mixed xlab
//!     cargo run --bin inputx-trace -- pinyin lixiang
//!     cargo run --bin inputx-trace -- wubi tjvs
//!
//! Output is text (one rule per line: `[elapsed] name effect`); a
//! future `--json` flag will emit machine-readable trace for tooling.

use inputx_core::rules::{Context, ContextFlags};
use inputx_core::EngineMode;
use std::env;

fn parse_mode(s: &str) -> Option<EngineMode> {
    match s.to_ascii_lowercase().as_str() {
        "mixed" => Some(EngineMode::Mixed),
        "wubi" | "wubionly" => Some(EngineMode::WubiOnly),
        "pinyin" | "pinyinonly" => Some(EngineMode::PinyinOnly),
        "jp" | "japanese" | "japaneseonly" => Some(EngineMode::JapaneseOnly),
        _ => None,
    }
}

fn derive_flags(buffer: &str) -> ContextFlags {
    let lower = buffer.to_ascii_lowercase();
    ContextFlags {
        has_vowel: lower.chars().any(|c| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'v')),
        has_non_speculative_pinyin: false, // populated by engines at v3.0.2+
        pinyin_intent: false,
        starts_with_z: lower.starts_with('z'),
        buffer_len: buffer.len(),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: inputx-trace <mode> <buffer>");
        eprintln!("  mode: mixed | wubi | pinyin | jp");
        eprintln!("  example: inputx-trace mixed xlab");
        std::process::exit(2);
    }
    let mode = match parse_mode(&args[0]) {
        Some(m) => m,
        None => {
            eprintln!("error: unknown mode {:?}", args[0]);
            std::process::exit(2);
        }
    };
    let buffer = args[1].clone();
    let ctx = Context {
        mode,
        buffer: buffer.clone(),
        prev_committed: None,
        second_prev_committed: None,
        flags: derive_flags(&buffer),
    };
    println!("=== inputx-trace v3.0.1 (scaffolding only) ===");
    println!("context:");
    println!("  mode:      {:?}", ctx.mode);
    println!("  buffer:    {:?}", ctx.buffer);
    println!("  flags:     {:?}", ctx.flags);
    println!();
    println!("candidate-rule engine: 0 rules registered (v3.0.2 migration pending)");
    println!("prediction-rule engine: 0 rules registered (v3.0.3 migration pending)");
    println!("commit-rule engine: 0 rules registered (v3.0.4 migration pending)");
    println!();
    println!("(At v3.0.5 this binary will dump full firing traces; for now");
    println!("it's a context-inspection tool that proves the rules module");
    println!("is wired in and ready for migration.)");
}
