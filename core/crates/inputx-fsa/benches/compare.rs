//! Self-contained read-path benchmarks for `inputx-fsa` (no external data).
//! A synthetic dictionary of ~20k prefix-sharing codes, each with a few
//! ranked items — the shape an IME dict has.
//!
//!     cargo bench -p inputx-fsa

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use inputx_fsa::{Dict, DictBuilder};

fn build() -> Vec<u8> {
    let mut b = DictBuilder::new();
    for i in 0..20_000u32 {
        let code = format!("code{i:05}");
        for j in 0..5u32 {
            b.insert(code.as_bytes(), format!("word{i}_{j}").as_bytes(), u64::from(i * 10 + j));
        }
    }
    b.finish()
}

fn bench(c: &mut Criterion) {
    let bytes = build();
    eprintln!("\n[size] Dict = {:.2} MB (20k codes × 5 items)\n", bytes.len() as f64 / 1_048_576.0);
    let dict = Dict::new(bytes.as_slice()).unwrap();

    // exact get on a spread of codes (hit) + a miss
    let probes: Vec<String> = (0..1000).map(|i| format!("code{:05}", (i * 19) % 20_000)).collect();
    c.bench_function("get", |bch| {
        bch.iter(|| {
            for p in &probes {
                black_box(dict.get(p.as_bytes()));
            }
        })
    });

    // prefix scans of varying breadth
    let prefixes: &[&[u8]] = &[b"code0", b"code00", b"code1", b"code"];
    c.bench_function("prefix_for_each", |bch| {
        bch.iter(|| {
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
