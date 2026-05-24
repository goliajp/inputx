//! `inputx-fsa` vs `fst` on the real shipped pinyin set (MIN_FREQ=100).
//! Measures the two operations the IME actually performs — "all words for a
//! code" (exact) and "all entries under a code prefix" — plus reports the
//! on-disk size of each index.
//!
//!     cargo bench -p inputx-fsa

use std::collections::BTreeMap;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fst::{IntoStreamer, Streamer};
use inputx_fsa::{Dict, DictBuilder};

fn load_groups() -> BTreeMap<String, Vec<(String, u64)>> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../inputx-pinyin/data/weights/weights.tsv"
    );
    let text = std::fs::read_to_string(path).expect("weights.tsv (run from workspace)");
    let mut g: BTreeMap<String, Vec<(String, u64)>> = BTreeMap::new();
    for line in text.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut it = line.split('\t');
        let (Some(py), Some(w), Some(f)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        let f: u64 = f.parse().unwrap_or(0);
        if f < 100 {
            continue;
        }
        g.entry(py.to_string()).or_default().push((w.to_string(), f));
    }
    g
}

fn build_dict(g: &BTreeMap<String, Vec<(String, u64)>>) -> Vec<u8> {
    let mut b = DictBuilder::new();
    for (c, ws) in g {
        for (w, f) in ws {
            b.insert(c.as_bytes(), w.as_bytes(), *f);
        }
    }
    b.finish()
}

fn build_fst(g: &BTreeMap<String, Vec<(String, u64)>>) -> Vec<u8> {
    let mut entries: Vec<(Vec<u8>, u64)> = Vec::new();
    for (c, ws) in g {
        for (w, f) in ws {
            let mut k = c.as_bytes().to_vec();
            k.push(0);
            k.extend_from_slice(w.as_bytes());
            entries.push((k, *f));
        }
    }
    entries.sort();
    let mut b = fst::MapBuilder::memory();
    for (k, v) in &entries {
        b.insert(k, *v).unwrap();
    }
    b.into_inner().unwrap()
}

fn fst_words(map: &fst::Map<Vec<u8>>, code: &[u8]) -> Vec<(Vec<u8>, u64)> {
    let mut lo = code.to_vec();
    lo.push(0);
    let mut hi = code.to_vec();
    hi.push(1);
    let mut out = Vec::new();
    let mut s = map.range().ge(lo.as_slice()).lt(hi.as_slice()).into_stream();
    while let Some((k, v)) = s.next() {
        out.push((k[code.len() + 1..].to_vec(), v));
    }
    out
}

fn fst_prefix_sum(map: &fst::Map<Vec<u8>>, prefix: &[u8]) -> u64 {
    let lo = prefix.to_vec();
    let mut hi = prefix.to_vec();
    *hi.last_mut().unwrap() += 1;
    let mut acc = 0u64;
    let mut s = map.range().ge(lo.as_slice()).lt(hi.as_slice()).into_stream();
    while let Some((_, v)) = s.next() {
        acc = acc.wrapping_add(v);
    }
    acc
}

fn bench(c: &mut Criterion) {
    let g = load_groups();
    let dict_bytes = build_dict(&g);
    let fst_bytes = build_fst(&g);
    eprintln!(
        "\n[size] inputx-fsa Dict = {:.2} MB   fst = {:.2} MB   ({} codes)\n",
        dict_bytes.len() as f64 / 1_048_576.0,
        fst_bytes.len() as f64 / 1_048_576.0,
        g.len()
    );
    let dict = Dict::new(dict_bytes.as_slice()).unwrap();
    let fmap = fst::Map::new(fst_bytes).unwrap();

    let probes: &[&[u8]] = &[b"wo", b"ni", b"de", b"shi", b"zhongguo", b"women", b"qing"];
    c.bench_function("get/inputx-fsa", |b| {
        b.iter(|| {
            for p in probes {
                black_box(dict.get(p));
            }
        })
    });
    c.bench_function("get/fst", |b| {
        b.iter(|| {
            for p in probes {
                black_box(fst_words(&fmap, p));
            }
        })
    });

    let prefixes: &[&[u8]] = &[b"z", b"zh", b"q", b"w", b"ni"];
    c.bench_function("prefix/inputx-fsa", |b| {
        b.iter(|| {
            for p in prefixes {
                let mut acc = 0u64;
                dict.prefix_for_each(p, |_, _, v| acc = acc.wrapping_add(v));
                black_box(acc);
            }
        })
    });
    c.bench_function("prefix/fst", |b| {
        b.iter(|| {
            for p in prefixes {
                black_box(fst_prefix_sum(&fmap, p));
            }
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
