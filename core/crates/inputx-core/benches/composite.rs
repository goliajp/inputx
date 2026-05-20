//! Criterion benchmarks for the composite dual-engine hot path.
//!
//! Run with `cargo bench -p inputx-core` (release profile auto).
//!
//! The unit-test `perfgate_refresh_candidates_under_budget` enforces a
//! hard upper bound on the per-keystroke cost; criterion here gives
//! finer-grained numbers for tuning and regression triage. They are
//! complementary, not redundant: the perfgate runs every `cargo test`
//! and shouts on regression; criterion runs on demand and tells you
//! *how much* slower (or faster) a change is.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use inputx_core::Session;

/// Build a warmed-up session — pages in the FST `.rodata` and builds the
/// process-global 简拼 initials index so the first measured keystroke
/// isn't paying cold-start cost.
fn warm_session() -> Session {
    let mut s = Session::new();
    s.warmup();
    s
}

fn bench_last_keystroke(c: &mut Criterion) {
    let _ = warm_session(); // populate global lazy state once
    let mut group = c.benchmark_group("composite.last_keystroke");

    // Worst case first — short prefixes scan the most FST entries.
    let probes: &[(&str, &str)] = &[
        ("single_z", "z"),
        ("single_h", "h"),
        ("partial_zh", "zh"),
        ("partial_zho", "zho"),
        ("partial_zhon", "zhon"),
        ("syllable_zhong", "zhong"),
        ("phrase_zhongguo", "zhongguo"),
        ("phrase_women", "women"),
        ("phrase_beijing", "beijing"),
        ("collision_shang", "shang"),
        ("initials_hh", "hh"),
        ("initials_hhh", "hhh"),
    ];

    for (label, input) in probes {
        let bytes = input.as_bytes();
        let (head, last) = bytes.split_at(bytes.len() - 1);
        let last = last[0];
        group.bench_function(format!("{label}_{input}"), |b| {
            b.iter_with_setup(
                || {
                    let mut s = Session::new();
                    for &c in head {
                        s.handle_key(c as u32, 0);
                    }
                    s
                },
                |mut s| {
                    s.handle_key(black_box(last) as u32, 0);
                    black_box(s.candidate_count());
                },
            );
        });
    }
    group.finish();
}

fn bench_full_word_typing(c: &mut Criterion) {
    let _ = warm_session();
    let mut group = c.benchmark_group("composite.full_word_typing");

    // End-to-end: every keystroke for a representative input, including
    // commit. This is what the user actually experiences as "type a word".
    let words: &[(&str, &str, usize)] = &[
        ("zhongguo", "中国", 0),
        ("women", "我们", 0),
        ("beijing", "北京", 0),
        ("haha", "哈哈", 0),
    ];

    for (input, expected, index) in words {
        group.bench_function(format!("type_{input}"), |b| {
            b.iter_with_setup(Session::new, |mut s| {
                for c in input.bytes() {
                    s.handle_key(black_box(c) as u32, 0);
                }
                let committed = s.commit_index(black_box(*index));
                debug_assert_eq!(committed.as_deref(), Some(*expected));
                black_box(committed);
            });
        });
    }
    group.finish();
}

fn bench_session_construction(c: &mut Criterion) {
    let _ = warm_session();
    let mut group = c.benchmark_group("composite.lifecycle");

    // Session creation cost — relevant per keyboard-extension instance.
    // After global lazy state is warmed (which we do once before the
    // benches start), this should be sub-µs.
    group.bench_function("Session::new", |b| {
        b.iter(|| black_box(Session::new()));
    });

    // Warmup cost — only paid once per process; here for visibility.
    // Iter-with-setup so each measurement starts from a fresh session.
    group.bench_function("Session::warmup_after_global_init", |b| {
        b.iter_with_setup(Session::new, |mut s| {
            s.warmup();
            black_box(s);
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_last_keystroke,
    bench_full_word_typing,
    bench_session_construction,
);
criterion_main!(benches);
