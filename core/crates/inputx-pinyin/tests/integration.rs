//! End-to-end gate test for the v0.1 Phase 1 milestone.
//!
//! From the inputx ROADMAP Phase 1 gate:
//! > `let mut buf = vec![]; session.lookup_into("zhongguo", &mut buf)`
//! > returns `["中国", ...]` from bootstrap dict.

use golia_pinyin::{PinyinEngine, Session};

#[test]
fn gate_zhongguo_via_session() {
    let engine = PinyinEngine::new();
    let mut session = Session::new(&engine);
    for c in "zhongguo".chars() {
        session.input_char(c);
    }
    let mut buf = Vec::new();
    session.lookup_into(&mut buf);
    assert_eq!(
        buf.first().map(String::as_str),
        Some("中国"),
        "expected 中国 first, got {buf:?}"
    );
}

#[test]
fn gate_nihao_phrase() {
    let engine = PinyinEngine::new();
    let mut session = Session::new(&engine);
    for c in "nihao".chars() {
        session.input_char(c);
    }
    let cands = session.candidates();
    assert_eq!(cands.first().map(String::as_str), Some("你好"));
}

#[test]
fn gate_single_char_disambiguation() {
    // `shi` has many candidates; engine should return them all.
    let engine = PinyinEngine::new();
    let mut session = Session::new(&engine);
    for c in "shi".chars() {
        session.input_char(c);
    }
    let cands: Vec<&str> = session.candidates().iter().map(String::as_str).collect();
    assert!(cands.contains(&"是"));
    assert!(cands.contains(&"时"));
    assert!(cands.len() >= 3);
}

#[test]
fn gate_typing_then_backspace_then_commit() {
    let engine = PinyinEngine::new();
    let mut session = Session::new(&engine);
    for c in "zhongguox".chars() {
        session.input_char(c);
    }
    session.backspace(); // drop the trailing 'x'
    let committed = session.commit(0);
    assert_eq!(committed.as_deref(), Some("中国"));
    assert!(session.input().is_empty());
}

// =============================================================================
// item 24 smoke — top-frequent words round-trip through full v0.2 dict.
// Gated to default-features (full pinyin.fst); --features bootstrap_only would
// fail these because the bootstrap dict has different freq distribution.
// =============================================================================

#[cfg(not(feature = "bootstrap_only"))]
#[test]
fn smoke_v02_top_frequent_phrases_rank_early() {
    // v0.2 ranking quality: "in top 5" is the honest bar. The corpus mix
    // (Leipzig wiki+news skews formal-register) sometimes ranks formal
    // synonyms above colloquial defaults — e.g. 落实 (luòshí "implement"
    // / news register) outranks 老师 (lǎoshī "teacher" / spoken) for the
    // pinyin "laoshi" because both share the laoshi reading via 落's
    // secondary lào reading. v0.3 L0 user-learning resolves per-user.
    let engine = PinyinEngine::new();
    let mut session = Session::new(&engine);
    let cases: &[(&str, &str)] = &[
        ("women", "我们"),
        ("nihao", "你好"),
        ("zhongguo", "中国"),
        ("shenme", "什么"),
        ("xianzai", "现在"),
        ("jintian", "今天"),
        ("yinhang", "银行"),
        ("kuaile", "快乐"),
        ("duibuqi", "对不起"),
        ("xuesheng", "学生"),
        ("laoshi", "老师"),
        ("pengyou", "朋友"),
        ("gongzuo", "工作"),
        ("xuexi", "学习"),
        ("shijian", "时间"),
    ];
    let mut failures = Vec::new();
    for (pinyin, expected) in cases {
        session.reset();
        for c in pinyin.chars() {
            session.input_char(c);
        }
        let cands = session.candidates();
        let top5: Vec<&str> = cands.iter().take(5).map(String::as_str).collect();
        if !top5.contains(expected) {
            failures.push(format!(
                "{pinyin}: {expected:?} not in top 5 — got {top5:?}",
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "smoke failures:\n  {}",
        failures.join("\n  ")
    );
}

#[cfg(not(feature = "bootstrap_only"))]
#[test]
fn smoke_v02_top_frequent_single_chars_rank_first() {
    let engine = PinyinEngine::new();
    let mut session = Session::new(&engine);
    let cases: &[(&str, &str)] = &[
        ("wo", "我"),
        ("de", "的"),
        ("le", "了"),
        ("bu", "不"),
        ("zai", "在"),
        ("you", "有"),
    ];
    let mut failures = Vec::new();
    for (pinyin, expected) in cases {
        session.reset();
        for c in pinyin.chars() {
            session.input_char(c);
        }
        let cands = session.candidates();
        let got = cands.first().map(String::as_str);
        if got != Some(*expected) {
            failures.push(format!(
                "{pinyin} → {got:?} (expected {expected:?}; first 5: {:?})",
                cands.iter().take(5).collect::<Vec<_>>(),
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "smoke failures:\n  {}",
        failures.join("\n  ")
    );
}

#[cfg(not(feature = "bootstrap_only"))]
#[test]
fn smoke_v02_spoken_register_present() {
    // SUBTLEX-CH (spoken register) corpus contributed weight 0.6; verify
    // its high-freq tokens rank in the top tier where we'd expect them.
    let engine = PinyinEngine::new();
    let mut session = Session::new(&engine);
    let cases: &[(&str, &[&str])] = &[
        ("zhidao", &["知道"]),
        ("yao", &["要"]),
        ("ta", &["他", "她", "它"]),
    ];
    for (pinyin, expected_in_top) in cases {
        session.reset();
        for c in pinyin.chars() {
            session.input_char(c);
        }
        let cands = session.candidates();
        let top10: Vec<&str> = cands.iter().take(10).map(String::as_str).collect();
        for want in expected_in_top.iter() {
            assert!(
                top10.contains(want),
                "{pinyin}: expected {want:?} in top 10, got {top10:?}",
            );
        }
    }
}
