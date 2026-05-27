//! Criterion benchmarks for the pinyin engine hot paths.
//!
//! Run with `cargo bench -p inputx-pinyin` (release profile auto).
//! Establishes the perf baseline that informs the per-keystroke budget
//! enforced by `inputx-core`'s perfgate unit test.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use inputx_pinyin::{PinyinEngine, char_to_pinyin, segment};

fn bench_lookup(c: &mut Criterion) {
    let eng = PinyinEngine::new();
    let mut group = c.benchmark_group("dict.lookup");
    // Common short word — single-syllable, multi-candidate
    group.bench_function("ni_single_syllable", |b| {
        b.iter(|| black_box(eng.dict().lookup(black_box("ni"))));
    });
    // Common two-syllable phrase
    group.bench_function("zhongguo_phrase", |b| {
        b.iter(|| black_box(eng.dict().lookup(black_box("zhongguo"))));
    });
    // Long input — pushes segmenter to enumerate splits
    group.bench_function("zhongguorenmin_4syll", |b| {
        b.iter(|| black_box(eng.dict().lookup(black_box("zhongguorenmin"))));
    });
    // Miss path — input that doesn't resolve to any word
    group.bench_function("miss_xxx", |b| {
        b.iter(|| black_box(eng.dict().lookup(black_box("xxxxxx"))));
    });
    group.finish();
}

fn bench_lookup_into(c: &mut Criterion) {
    let eng = PinyinEngine::new();
    let mut group = c.benchmark_group("dict.lookup_into");
    // Reused buffer — what the IME hot path actually uses
    group.bench_function("zhongguo_reused_buf", |b| {
        let mut buf = Vec::with_capacity(32);
        b.iter(|| {
            eng.dict()
                .lookup_into(black_box("zhongguo"), black_box(&mut buf));
        });
    });
    group.finish();
}

fn bench_prefix_for_each(c: &mut Criterion) {
    let eng = PinyinEngine::new();
    let mut group = c.benchmark_group("dict.prefix_for_each");

    // Per-keystroke prefix completion at varied prefix lengths. Short
    // prefixes scan the most FST entries — the worst-case path that
    // `inputx-core`'s perfgate guards. The bench here gives finer detail
    // than the unit-test perfgate's min-of-N.
    for prefix in ["z", "zh", "zho", "zhon", "zhong", "zhongguo"] {
        group.bench_function(format!("count_only_{prefix}"), |b| {
            b.iter(|| {
                let mut n = 0u64;
                eng.dict().prefix_for_each(black_box(prefix), |_, _, _| {
                    n += 1;
                });
                black_box(n)
            });
        });
    }
    group.finish();
}

fn bench_prefix_for_each_raw(c: &mut Criterion) {
    let eng = PinyinEngine::new();
    let mut group = c.benchmark_group("dict.prefix_for_each_raw");
    // `_raw` variant skips per-entry utf8 validation. Inputx-core uses
    // this on the keystroke hot path; the speedup vs `prefix_for_each`
    // is the main reason both APIs exist.
    for prefix in ["z", "zh", "zho", "zhong"] {
        group.bench_function(format!("count_only_{prefix}"), |b| {
            b.iter(|| {
                let mut n = 0u64;
                eng.dict().prefix_for_each_raw(black_box(prefix), |_, _, _| {
                    n += 1;
                });
                black_box(n)
            });
        });
    }
    group.finish();
}

fn bench_segment(c: &mut Criterion) {
    let mut group = c.benchmark_group("segmenter.segment");
    // The segmenter's DP enumerates all valid syllable splits. Input
    // length is the dominant cost.
    group.bench_function("zhongguo_2syll", |b| {
        b.iter(|| black_box(segment(black_box("zhongguo"))));
    });
    group.bench_function("zhongguorenmin_4syll", |b| {
        b.iter(|| black_box(segment(black_box("zhongguorenmin"))));
    });
    group.bench_function("ambiguous_xian_xi_an", |b| {
        // Classic ambiguity: `xian` could be `xian` or `xi`+`an`.
        b.iter(|| black_box(segment(black_box("xian"))));
    });
    group.finish();
}

fn bench_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode.char_to_pinyin");
    // Reverse lookup — char → pinyin reading. Hot in features like
    // "show pinyin of selected char" UIs.
    group.bench_function("hit_common", |b| {
        b.iter(|| black_box(char_to_pinyin(black_box('中'))));
    });
    group.bench_function("hit_uncommon", |b| {
        b.iter(|| black_box(char_to_pinyin(black_box('鬱'))));
    });
    group.bench_function("miss_latin", |b| {
        b.iter(|| black_box(char_to_pinyin(black_box('A'))));
    });
    group.finish();
}

fn bench_record_pick(c: &mut Criterion) {
    let mut group = c.benchmark_group("dict.record_pick");
    // Each iter records a pick on a fresh engine to avoid mutating
    // shared global state with N-many counters.
    group.bench_function("zhongguo_to_zhongguo", |b| {
        b.iter_with_setup(PinyinEngine::new, |eng| {
            black_box(
                eng.dict()
                    .record_pick(black_box("zhongguo"), black_box("中国")),
            );
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_lookup,
    bench_lookup_into,
    bench_prefix_for_each,
    bench_prefix_for_each_raw,
    bench_segment,
    bench_encode,
    bench_record_pick,
);
criterion_main!(benches);
