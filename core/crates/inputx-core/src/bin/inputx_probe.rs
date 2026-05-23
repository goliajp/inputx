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
//!       {"word": "继续", "source": "Pinyin"},
//!       {"word": "曳光弹", "source": "Wubi"},
//!       ...
//!     ]
//!   }

use std::env;
use std::process::ExitCode;

use inputx_core::{AutoCommitPolicy, EngineMode as Mode, Session, Source};

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
        if !b.is_ascii_alphabetic() {
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
