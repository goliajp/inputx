//! Write the compiled wubi86 table out to a shippable file.
//!
//!     cargo run --release --bin wubi-emit-dict -- <out-path>
//!
//! `build.rs` compiles `data/library.tsv` + the structural tables into
//! `$OUT_DIR/wubi86.dict` and `dict.rs` pulls it in with `include_bytes!`.
//! That makes the table reachable only by replacing the binary — which is
//! why a wubi polish used to force a full reinstall, killing the IME
//! process and leaving any app with a live input session unable to type
//! until it was restarted.
//!
//! This bin re-emits `DICT_BYTES` verbatim so `reinstall.py` has a file to
//! swap into the running bundle and `Session::reload_engine_data` has one
//! to read back. Emitting the compiled constant rather than re-running the
//! builder is deliberate: the shipped bytes are then the SAME bytes the
//! binary was built with, by construction, with no chance of the two
//! drifting apart.
//!
//! Run from `make polish-rebuild`; the output is committed alongside
//! `pinyin.dict` / `words.idf`, which work the same way.

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let out = args.next().map(PathBuf::from).unwrap_or_else(|| {
        eprintln!("usage: wubi-emit-dict <out-path>");
        std::process::exit(2);
    });
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| {
            eprintln!("create {}: {e}", parent.display());
            std::process::exit(1);
        });
    }
    std::fs::write(&out, inputx_wubi::DICT_BYTES).unwrap_or_else(|e| {
        eprintln!("write {}: {e}", out.display());
        std::process::exit(1);
    });
    println!(
        "wrote {} ({} bytes)",
        out.display(),
        inputx_wubi::DICT_BYTES.len()
    );
}
