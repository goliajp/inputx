//! Build-time codegen for inputx-pinyin:
//! - `bootstrap.fst` — tiny FST built from `data/bootstrap.tsv` (~125
//!   hand-curated entries, MIT-clean). Available behind the
//!   `bootstrap_only` feature for fast tests / minimal builds.
//!
//! The full pinyin.dict (~4 MB, ~237k entries) is shipped via the
//! sibling `inputx-pinyin-data-core` crate. Maintainers regenerate it
//! via `cargo run --features tools --release --bin pinyin-build-dict`
//! after `data/library.tsv` changes.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    build_bootstrap(&crate_dir, &out_dir);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data/bootstrap.tsv");
}

fn build_bootstrap(crate_dir: &std::path::Path, out_dir: &std::path::Path) {
    let tsv_path = crate_dir.join("data/bootstrap.tsv");
    let tsv =
        fs::read_to_string(&tsv_path).unwrap_or_else(|_| panic!("{} missing", tsv_path.display()));

    let mut builder = inputx_fsa::DictBuilder::new();
    for (lineno, raw) in tsv.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let pinyin = parts.next().unwrap_or("").trim();
        let word = parts.next().unwrap_or("").trim();
        if pinyin.is_empty() || word.is_empty() {
            panic!(
                "bootstrap.tsv:{}: bad row (need pinyin\\tword): {raw:?}",
                lineno + 1
            );
        }
        builder.insert(pinyin.to_ascii_lowercase().as_bytes(), word.as_bytes(), 1);
    }
    let dict_path = out_dir.join("bootstrap.dict");
    fs::write(&dict_path, builder.finish()).expect("write bootstrap.dict");
}
