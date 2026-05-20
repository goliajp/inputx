//! Build-time codegen for wubi:
//! - `phf::Map<char, u8>` for 字根 → letter (`zigen.phf.rs`)
//! - `phf::Map<u8, char>` for 一级简码 (`jianma1.phf.rs`)
//! - `wubi86.fst` — pre-built FST containing all encoded dictionary entries
//!
//! The FST is built by running the encoder algorithmically over `seed.txt`
//! (`#[path = "src/codec.rs"]` shares the algorithm with the runtime).

#[path = "src/codec.rs"]
mod codec;
#[path = "src/layer.rs"]
mod layer;

use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use codec::{DecompRef, Shape, Stroke, encode_with_lookup};
use layer::{Layer, pack};

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let zigen_src =
        fs::read_to_string(crate_dir.join("data/zigen86.txt")).expect("data/zigen86.txt missing");
    let jianma1_src =
        fs::read_to_string(crate_dir.join("data/jianma1.txt")).expect("data/jianma1.txt missing");
    let seed_src =
        fs::read_to_string(crate_dir.join("data/seed.txt")).expect("data/seed.txt missing");
    // auto_decomp.txt is optional — large machine-generated companion to
    // seed.txt. Decomps may not be visually canonical but encode correctly.
    let auto_src = fs::read_to_string(crate_dir.join("data/auto_decomp.txt")).unwrap_or_default();
    // jianma_simplified.txt holds 二级 / 三级 简码 (table-defined, not
    // algorithmically derived). Format: `<code>\t<char>`.
    let simplified_src =
        fs::read_to_string(crate_dir.join("data/jianma_simplified.txt")).unwrap_or_default();
    // phrases.txt holds 词组 (multi-char phrases). Format: `<code>\t<phrase>`.
    let phrases_src =
        fs::read_to_string(crate_dir.join("data/phrases.txt")).unwrap_or_default();

    let zigen_map = parse_zigen_map(&zigen_src);
    let jianma1_pairs = parse_jianma1_pairs(&jianma1_src);

    write_zigen_phf(&out_dir, &zigen_map);
    write_jianma1_phf(&out_dir, &jianma1_pairs);
    build_fst(
        &out_dir,
        &crate_dir,
        &zigen_map,
        &jianma1_pairs,
        &seed_src,
        &auto_src,
        &simplified_src,
        &phrases_src,
    );

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data/zigen86.txt");
    println!("cargo:rerun-if-changed=data/jianma1.txt");
    println!("cargo:rerun-if-changed=data/seed.txt");
    println!("cargo:rerun-if-changed=data/auto_decomp.txt");
    println!("cargo:rerun-if-changed=data/jianma_simplified.txt");
    println!("cargo:rerun-if-changed=data/phrases.txt");
    println!("cargo:rerun-if-changed=data/weights/weights.tsv");
    println!("cargo:rerun-if-changed=src/codec.rs");
    println!("cargo:rerun-if-changed=src/layer.rs");
    println!("cargo:rerun-if-env-changed=WUBI_PROMOTE_THRESHOLD");
}

/// Load `data/weights/weights.tsv` into a `(code, word) → freq_score` lookup.
/// If the file is missing, returns an empty map — freq scores default to 0
/// and ranking falls back to LAYER_BASE alone (still correct, just less
/// granular).
fn load_freq_scores(crate_dir: &std::path::Path) -> HashMap<(String, String), u64> {
    let path = crate_dir.join("data/weights/weights.tsv");
    let Ok(src) = fs::read_to_string(&path) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let (Some(code), Some(word), Some(_layer), Some(freq)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Ok(freq) = freq.parse::<u64>() else {
            continue;
        };
        out.insert((code.to_string(), word.to_string()), freq);
    }
    out
}

/// Parse `zigen86.txt` into a `char → key letter` map.
fn parse_zigen_map(src: &str) -> HashMap<char, u8> {
    let mut map = HashMap::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let (Some(letter), Some(zigen)) = (parts.next(), parts.next()) else {
            continue;
        };
        let letter = letter.trim();
        let zigen = zigen.trim();
        if letter.len() != 1 {
            continue;
        }
        let l = letter.as_bytes()[0].to_ascii_lowercase();
        if !l.is_ascii_alphabetic() || l == b'z' {
            continue;
        }
        for c in zigen.chars() {
            map.insert(c, l);
        }
    }
    map
}

/// Parse `jianma1.txt` into a list of (letter, char) pairs (ordered by file).
fn parse_jianma1_pairs(src: &str) -> Vec<(u8, char)> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let (Some(letter), Some(ch)) = (parts.next(), parts.next()) else {
            continue;
        };
        let letter = letter.trim();
        let ch = ch.trim();
        if letter.len() != 1 {
            continue;
        }
        let l = letter.as_bytes()[0].to_ascii_lowercase();
        if !l.is_ascii_alphabetic() || l == b'z' {
            continue;
        }
        if ch.chars().count() == 1 {
            out.push((l, ch.chars().next().unwrap()));
        }
    }
    out
}

fn write_zigen_phf(out_dir: &std::path::Path, map: &HashMap<char, u8>) {
    let mut codegen = phf_codegen::Map::new();
    for (k, v) in map {
        codegen.entry(*k, &format!("{v}u8"));
    }
    let path = out_dir.join("zigen.phf.rs");
    let mut f = fs::File::create(&path).unwrap();
    writeln!(
        f,
        "/// Auto-generated by build.rs from data/zigen86.txt.\n\
         pub static ZIGEN: ::phf::Map<char, u8> = {};",
        codegen.build()
    )
    .unwrap();
}

fn write_jianma1_phf(out_dir: &std::path::Path, pairs: &[(u8, char)]) {
    let mut codegen = phf_codegen::Map::new();
    for (k, v) in pairs {
        codegen.entry(*k, &format!("'{v}'"));
    }
    let path = out_dir.join("jianma1.phf.rs");
    let mut f = fs::File::create(&path).unwrap();
    writeln!(
        f,
        "/// Auto-generated by build.rs from data/jianma1.txt.\n\
         pub static JIANMA1: ::phf::Map<u8, char> = {};",
        codegen.build()
    )
    .unwrap();
}

/// Build the main dictionary FST by running the encoder over `seed.txt` and
/// merging in the 一级简码 entries.
///
/// FST keys are composite: `code\0word`, so all words for a given code can
/// be retrieved via a prefix range scan on `code\0`. FST values pack
/// `(layer, freq_score)` via `layer::pack`. `freq_score` comes from
/// `data/weights/weights.tsv` if present (regenerated by
/// `wubi-build-weights`); 0 otherwise. With freq=0 the ordering is purely
/// by `LAYER_BASE` — still correct, just less granular within a layer.
fn build_fst(
    out_dir: &std::path::Path,
    crate_dir: &std::path::Path,
    zigen_map: &HashMap<char, u8>,
    jianma1: &[(u8, char)],
    seed_src: &str,
    auto_src: &str,
    simplified_src: &str,
    phrases_src: &str,
) {
    let mut entries: BTreeMap<Vec<u8>, u64> = BTreeMap::new();
    let freq_scores = load_freq_scores(crate_dir);
    let freq_for =
        |code: &str, word: &str| freq_scores.get(&(code.to_string(), word.to_string())).copied().unwrap_or(0);

    // 一级简码 entries: code is a single letter.
    for (letter, ch) in jianma1 {
        let mut key = Vec::with_capacity(2 + ch.len_utf8());
        key.push(*letter);
        key.push(0u8);
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        key.extend_from_slice(s.as_bytes());
        let code_str = (*letter as char).to_string();
        let word_str = ch.to_string();
        entries.insert(key, pack(Layer::Jianma1, freq_for(&code_str, &word_str)));
    }

    let lookup = |c: char| -> Option<u8> { zigen_map.get(&c).copied() };
    let mut buf = [0u8; 4];

    // Hand-curated seed (winning weight on conflict).
    let mut seed_chars: HashMap<char, ()> = HashMap::new();
    for (ch, zigen, strokes, shape) in parse_seed_for_build(seed_src) {
        seed_chars.insert(ch, ());
        let decomp_ref = DecompRef {
            zigen: &zigen,
            strokes: &strokes,
            shape,
        };
        if let Ok(n) = encode_with_lookup(&decomp_ref, &lookup, &mut buf) {
            let key = compose_key(&buf[..n], ch);
            let code_str = std::str::from_utf8(&buf[..n]).unwrap_or("").to_string();
            let weight = pack(Layer::Zigen, freq_for(&code_str, &ch.to_string()));
            entries
                .entry(key)
                .and_modify(|w| {
                    if *w < weight {
                        *w = weight;
                    }
                })
                .or_insert(weight);
        } else {
            println!("cargo:warning=seed encode failed for {ch}");
        }
    }

    // Auto-decomposed entries (skip chars already in seed; algorithmically
    // verified to round-trip but pedagogically may pick non-canonical 字根
    // sequences).
    let mut auto_added = 0usize;
    let mut auto_skipped = 0usize;
    for (ch, zigen, strokes, shape) in parse_seed_for_build(auto_src) {
        if seed_chars.contains_key(&ch) {
            auto_skipped += 1;
            continue;
        }
        let decomp_ref = DecompRef {
            zigen: &zigen,
            strokes: &strokes,
            shape,
        };
        if let Ok(n) = encode_with_lookup(&decomp_ref, &lookup, &mut buf) {
            let key = compose_key(&buf[..n], ch);
            let code_str = std::str::from_utf8(&buf[..n]).unwrap_or("").to_string();
            let weight = pack(Layer::Auto, freq_for(&code_str, &ch.to_string()));
            entries.entry(key).or_insert(weight);
            auto_added += 1;
        }
    }

    // 二级 / 三级 简码 — table-defined (not algorithmic). Direct (code, char)
    // entries from `data/jianma_simplified.txt`.
    let mut jianma2_added = 0usize;
    let mut jianma3_added = 0usize;
    for raw in simplified_src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let (Some(code), Some(word)) = (parts.next(), parts.next()) else {
            continue;
        };
        let code = code.trim();
        let word = word.trim();
        if word.chars().count() != 1 {
            continue;
        }
        let ch = word.chars().next().unwrap();
        let key = compose_key(code.as_bytes(), ch);
        let weight = match code.len() {
            2 => {
                jianma2_added += 1;
                pack(Layer::Jianma2, freq_for(code, &ch.to_string()))
            }
            3 => {
                jianma3_added += 1;
                pack(Layer::Jianma3, freq_for(code, &ch.to_string()))
            }
            _ => continue,
        };
        // If a higher-weight entry already exists, keep that one.
        entries
            .entry(key)
            .and_modify(|w| {
                if *w < weight {
                    *w = weight;
                }
            })
            .or_insert(weight);
    }

    // 词组 — multi-character phrases (table-defined, 4-letter codes).
    let mut phrases_added = 0usize;
    for raw in phrases_src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let (Some(code), Some(phrase)) = (parts.next(), parts.next()) else {
            continue;
        };
        let code = code.trim();
        let phrase = phrase.trim();
        if code.len() != 4 || phrase.chars().count() < 2 {
            continue;
        }
        // Compose the key with the phrase as the "word" portion.
        let mut key = Vec::with_capacity(code.len() + 1 + phrase.len());
        key.extend_from_slice(code.as_bytes());
        key.push(0u8);
        key.extend_from_slice(phrase.as_bytes());
        let weight_phrase = pack(Layer::Phrase, freq_for(code, phrase));
        entries
            .entry(key)
            .and_modify(|w| {
                if *w < weight_phrase {
                    *w = weight_phrase;
                }
            })
            .or_insert(weight_phrase);
        phrases_added += 1;
    }

    let fst_path = out_dir.join("wubi86.fst");
    let file = fs::File::create(&fst_path).expect("create fst output");
    let mut builder = fst::MapBuilder::new(file).expect("MapBuilder::new");
    for (key, val) in &entries {
        builder.insert(key, *val).expect("FST insert");
    }
    builder.finish().expect("FST finish");

    println!(
        "cargo:warning=wubi: FST wrote {} entries (jianma1: {}, seed: {}, auto: {}/skip {}, jianma2: {}, jianma3: {}, phrases: {})",
        entries.len(),
        jianma1.len(),
        seed_chars.len(),
        auto_added,
        auto_skipped,
        jianma2_added,
        jianma3_added,
        phrases_added,
    );
}

fn compose_key(code: &[u8], ch: char) -> Vec<u8> {
    let mut key = Vec::with_capacity(code.len() + 1 + ch.len_utf8());
    key.extend_from_slice(code);
    key.push(0u8);
    let mut chbuf = [0u8; 4];
    let s = ch.encode_utf8(&mut chbuf);
    key.extend_from_slice(s.as_bytes());
    key
}

fn parse_seed_for_build(src: &str) -> Vec<(char, Vec<char>, Vec<Stroke>, Shape)> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(4, '\t');
        let (Some(ch), Some(zg), Some(strokes_field), Some(shape)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let ch = ch.trim();
        if ch.chars().count() != 1 {
            continue;
        }
        let ch = ch.chars().next().unwrap();
        let zigen: Vec<char> = zg.split_whitespace().flat_map(|s| s.chars()).collect();
        if zigen.is_empty() {
            continue;
        }
        let strokes: Vec<Stroke> = strokes_field
            .split_whitespace()
            .filter_map(|s| s.parse::<u8>().ok().and_then(Stroke::from_u8))
            .collect();
        if strokes.is_empty() {
            continue;
        }
        let Ok(p) = shape.trim().parse::<u8>() else {
            continue;
        };
        let Some(shape) = Shape::from_u8(p) else {
            continue;
        };
        out.push((ch, zigen, strokes, shape));
    }
    out
}
