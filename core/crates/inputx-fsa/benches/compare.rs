//! `inputx-fsa` read-path benchmarks on the real shipped pinyin set
//! (MIN_FREQ=100). Pure regression tracker — no external comparison crate
//! (the fst baseline numbers it once beat are recorded in the zerodep B-polish
//! commit: size -12%, get ~2x, prefix ~5.6x).
//!
//!     cargo bench -p inputx-fsa

use std::collections::BTreeMap;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
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

fn bench(c: &mut Criterion) {
    let g = load_groups();
    let bytes = build_dict(&g);
    eprintln!(
        "\n[size] inputx-fsa Dict = {:.2} MB  ({} codes)\n",
        bytes.len() as f64 / 1_048_576.0,
        g.len()
    );
    let dict = Dict::new(bytes.as_slice()).unwrap();

    let probes: &[&[u8]] = &[b"wo", b"ni", b"de", b"shi", b"zhongguo", b"women", b"qing"];
    c.bench_function("get", |b| {
        b.iter(|| {
            for p in probes {
                black_box(dict.get(p));
            }
        })
    });

    let prefixes: &[&[u8]] = &[b"z", b"zh", b"q", b"w", b"ni"];
    c.bench_function("prefix", |b| {
        b.iter(|| {
            for p in prefixes {
                let mut acc = 0u64;
                dict.prefix_for_each(p, |_, _, v| acc = acc.wrapping_add(v));
                black_box(acc);
            }
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
