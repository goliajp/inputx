//! criterion benches for the hot paths.
//!
//!     cargo bench -p wubi
//!
//! Targets (see PLAN.md):
//!   zigen_lookup    < 50ns   (PHF probe)
//!   jianma_lookup   < 50ns
//!   dict_exact      < 250ns  (FST traversal)
//!   dict_prefix     < 1µs    (first 5 hits)
//!   encode_*        < 500ns

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use wubi::{
    Decomp, Shape, Stroke, WubiDict, embedded_seed, encode_into, lookup_jianma1, lookup_zigen,
};

fn bench_zigen_lookup(c: &mut Criterion) {
    c.bench_function("zigen_lookup_hit", |b| {
        b.iter(|| black_box(lookup_zigen(black_box('王'))));
    });
    c.bench_function("zigen_lookup_miss", |b| {
        b.iter(|| black_box(lookup_zigen(black_box('🦀'))));
    });
}

fn bench_jianma_lookup(c: &mut Criterion) {
    c.bench_function("jianma1_lookup_hit", |b| {
        b.iter(|| black_box(lookup_jianma1(black_box(b'g'))));
    });
}

fn bench_dict(c: &mut Criterion) {
    let dict = WubiDict::embedded();
    c.bench_function("dict_lookup_g", |b| {
        b.iter(|| black_box(dict.lookup(black_box("g"))));
    });
    c.bench_function("dict_lookup_gggg", |b| {
        b.iter(|| black_box(dict.lookup(black_box("gggg"))));
    });
    c.bench_function("dict_lookup_miss_zzzz", |b| {
        b.iter(|| black_box(dict.lookup(black_box("zzzz"))));
    });
    c.bench_function("dict_prefix_g", |b| {
        b.iter(|| black_box(dict.prefix(black_box("g"))));
    });

    // The hot-loop variant: caller-owned buffer reused across calls. This is
    // the path the IME uses on every keystroke; should beat `lookup` by the
    // cost of one heap alloc/free per call.
    c.bench_function("dict_lookup_into_gggg_reused", |b| {
        let mut buf = Vec::with_capacity(8);
        b.iter(|| {
            dict.lookup_into(black_box("gggg"), &mut buf);
            black_box(&buf);
        });
    });

    // L0 mutation hot path — one IME keystroke per call.
    c.bench_function("dict_record_pick", |b| {
        b.iter(|| black_box(dict.record_pick(black_box("gggg"), black_box("王"))));
    });

    // L0 snapshot — runs at app shutdown / suspension; not strictly hot but
    // needs to be cheap enough to be invokable without UI jank.
    c.bench_function("dict_export_l0", |b| {
        b.iter(|| black_box(dict.export_l0()));
    });
}

fn bench_encode(c: &mut Criterion) {
    let seed: std::collections::HashMap<char, Decomp> = embedded_seed().into_iter().collect();
    let wang = seed.get(&'王').cloned().unwrap();
    let yi = seed.get(&'一').cloned().unwrap();
    let synthetic_2 = Decomp {
        zigen: vec!['人', '王'],
        strokes: vec![Stroke::Pie, Stroke::Heng],
        shape: Shape::TopBottom,
    };

    c.bench_function("encode_jianming_wang", |b| {
        let mut buf = [0u8; 4];
        b.iter(|| {
            let _ = encode_into(black_box(&wang), &mut buf);
            black_box(&buf);
        });
    });
    c.bench_function("encode_dan_bi_hua_yi", |b| {
        let mut buf = [0u8; 4];
        b.iter(|| {
            let _ = encode_into(black_box(&yi), &mut buf);
            black_box(&buf);
        });
    });
    c.bench_function("encode_2_zigen_synthetic", |b| {
        let mut buf = [0u8; 4];
        b.iter(|| {
            let _ = encode_into(black_box(&synthetic_2), &mut buf);
            black_box(&buf);
        });
    });
}

criterion_group!(
    benches,
    bench_zigen_lookup,
    bench_jianma_lookup,
    bench_dict,
    bench_encode
);
criterion_main!(benches);
