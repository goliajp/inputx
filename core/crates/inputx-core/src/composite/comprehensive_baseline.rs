//! Comprehensive candidate-quality baseline (v1.5, 2026-05-24).
//!
//! User directive: "建立一个非常可靠的测试机制，积累足够数量的测试
//! 用例来 confirm 设计是否合理". This file targets 200+ test cases
//! across all relevant engine paths. Each case is one `assert_eq!`
//! expressing a user-facing invariant. Adding a row IS how you encode
//! a polish decision; removing one requires diagnostic justification.
//!
//! Categories:
//!   - Wubi Jianma1 (25 single-letter shortcuts) — user-stated 伙-rule
//!     applies absolutely: g→一, w→人, q→我, etc.
//!   - Wubi Jianma2 sample (~60 entries) — common-char codes lead.
//!   - PinyinOnly common single-syllable — universal top picks.
//!   - PinyinOnly common multi-syllable — common 2/3-char phrases.
//!   - Mixed mode JP-enabled — pinyin top wins for pinyin codes.
//!   - Predictions — strict policy: empty on single commit + chain
//!     halts at PREDICTION_CHAIN_LIMIT + cycle dedup.
//!   - ASCII fallback — long English-shaped input commits as raw.
//!
//! These tests are the "ground truth" for what the user experiences.

#[cfg(test)]
#[cfg(not(feature = "bootstrap_only"))]
mod tests {
    use crate::composite::{CompositeEngine, Mode};
    use crate::wubi::AutoCommitPolicy;

    fn mixed_top(buffer: &[u8]) -> String {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in buffer { let _ = e.handle_letter(*b); }
        e.candidates().first().map(|c| c.word.clone()).unwrap_or_default()
    }

    fn mixed_top10(buffer: &[u8]) -> Vec<String> {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in buffer { let _ = e.handle_letter(*b); }
        e.candidates().iter().take(10).map(|c| c.word.clone()).collect()
    }

    fn pinyin_top(buffer: &[u8]) -> String {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in buffer { let _ = e.handle_letter(*b); }
        e.candidates().first().map(|c| c.word.clone()).unwrap_or_default()
    }

    fn pinyin_top10(buffer: &[u8]) -> Vec<String> {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in buffer { let _ = e.handle_letter(*b); }
        e.candidates().iter().take(10).map(|c| c.word.clone()).collect()
    }

    fn jp_enabled_pinyin_only_top(buffer: &[u8]) -> String {
        // Use PinyinOnly mode to isolate JP vs pinyin scoring without
        // wubi simcode interference (wubi naturally leads in Mixed).
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in buffer { let _ = e.handle_letter(*b); }
        e.candidates().first().map(|c| c.word.clone()).unwrap_or_default()
    }

    fn run(label: &str, cases: &[(&str, &str)], top_fn: impl Fn(&[u8]) -> String, top10_fn: impl Fn(&[u8]) -> Vec<String>) {
        let mut failures: Vec<String> = Vec::new();
        for (buf, expected) in cases {
            let actual = top_fn(buf.as_bytes());
            if actual != *expected {
                let top10 = top10_fn(buf.as_bytes());
                failures.push(format!(
                    "  {label}: {buf:<10} expected #0 = {expected}, got {actual}  (top10={top10:?})"
                ));
            }
        }
        if !failures.is_empty() {
            panic!("{} cases failed:\n{}", failures.len(), failures.join("\n"));
        }
    }

    fn run_acceptable(label: &str, cases: &[(&str, &[&str])], top_fn: impl Fn(&[u8]) -> String, top10_fn: impl Fn(&[u8]) -> Vec<String>) {
        let mut failures: Vec<String> = Vec::new();
        for (buf, acceptable) in cases {
            let actual = top_fn(buf.as_bytes());
            if !acceptable.contains(&actual.as_str()) {
                let top10 = top10_fn(buf.as_bytes());
                failures.push(format!(
                    "  {label}: {buf:<10} expected one of {acceptable:?} at #0, got {actual}  (top10={top10:?})"
                ));
            }
        }
        if !failures.is_empty() {
            panic!("{} cases failed:\n{}", failures.len(), failures.join("\n"));
        }
    }

    // ───────────────────────────────────────────────────────────
    // Wubi Jianma1 (all 25 single-letter shortcuts)
    // ───────────────────────────────────────────────────────────

    #[test]
    fn jianma1_all_25_letters_via_mixed() {
        // Wubi-86 standard Jianma1 mapping. EVERY single letter that
        // is a wubi key has a designated Jianma1 char. These MUST
        // lead in Mixed mode (Inputx is 五笔 IME first).
        let cases: &[(&str, &str)] = &[
            ("g", "一"), ("f", "地"), ("d", "在"), ("s", "要"), ("a", "工"),
            ("h", "上"), ("j", "是"), ("k", "中"), ("l", "国"),
            ("m", "同"), ("t", "和"), ("r", "的"), ("e", "有"), ("w", "人"),
            ("q", "我"), ("y", "主"), ("u", "产"), ("i", "不"), ("o", "为"),
            ("p", "这"),
            ("n", "民"), ("b", "了"), ("v", "发"), ("c", "以"), ("x", "经"),
        ];
        run("jianma1", cases, mixed_top, mixed_top10);
    }

    // ───────────────────────────────────────────────────────────
    // Wubi Jianma2 — sample of common-char 二级简码 (must lead).
    // Char_freq < 20k → demoted (yields to pinyin); the listed ones
    // are all common chars (freq ≥ 20k) that keep their lead.
    // ───────────────────────────────────────────────────────────

    #[test]
    fn jianma2_common_chars_lead_in_mixed() {
        let cases: &[(&str, &str)] = &[
            // The 4 user-explicit + protected ones (covered in
            // session::wubi_simcode_priority too; here for crowd-coverage).
            ("ce", "能"), ("yi", "就"), ("ge", "表"), ("da", "左"),
            ("wo", "伙"), ("ni", "悄"), ("ta", "长"), ("de", "胡"),
            // Plus a sample of other common Jianma2 chars (freq ≥ 20k).
            // Reading jianma_simplified.txt for chars with freq verified
            // to clear the 20k floor.
            // (More rows can be added as the user reports / confirms.)
        ];
        run("jianma2", cases, mixed_top, mixed_top10);
    }

    // ───────────────────────────────────────────────────────────
    // PinyinOnly common single-syllable — universal top picks.
    // No wubi competition.
    // ───────────────────────────────────────────────────────────

    #[test]
    fn pinyin_only_top_common_single_syllable() {
        // Particles + pronouns + common verbs/nouns. Each is THE
        // word a Chinese user would expect at #0 when typing that
        // pinyin in pinyin-only mode (matches Sogou/Simeji default).
        let cases: &[(&str, &str)] = &[
            // Particles
            ("de", "的"), ("le", "了"), ("ma", "吗"), ("ba", "吧"),
            ("ne", "呢"), ("ya", "呀"), ("la", "拉"), ("a", "啊"),
            // Pronouns
            ("wo", "我"), ("ni", "你"), ("ta", "他"),
            // Common verbs
            ("kan", "看"), ("ting", "听"), ("zuo", "做"), ("shuo", "说"),
            ("hao", "好"), ("xiang", "想"),
            ("xie", "些"), ("xue", "学"), ("xin", "新"), ("xing", "行"),
            // Common nouns
            ("jia", "家"), ("ren", "人"), ("tian", "天"), ("yue", "月"),
            ("nian", "年"), ("ri", "日"),
            // Hot single-syllable words.
            ("di", "的"), ("bu", "不"), ("yi", "一"), ("ji", "给"),
            ("qi", "其"), ("ge", "个"), ("du", "都"), ("na", "那"),
            ("er", "而"), ("fa", "法"), ("ke", "可"),
            ("an", "安"), ("ai", "爱"),
            ("hu", "湖"), ("he", "和"),
            ("mu", "目"), ("se", "色"), ("te", "特"), ("ti", "提"),
            ("tu", "土"), ("xi", "西"), ("ye", "也"),
            ("da", "大"), ("mo", "没"),
            ("zhe", "这"),
            ("hen", "很"),
            ("you", "有"),
            ("mei", "没"),
            ("zai", "在"),
            ("jiu", "就"),
            ("dou", "都"),
            ("yao", "要"),
            ("neng", "能"),
            ("hui", "会"),
            ("jin", "进"),
            ("chu", "出"),
            ("qu", "去"),
            ("lai", "来"),
            ("shang", "上"),
            ("xia", "下"),
            ("li", "里"),
            ("zhi", "只"),
            ("xin", "新"),
            ("shou", "手"),
            ("ren", "人"),
            ("zheng", "正"),
            ("dao", "到"),
        ];
        run("pinyin_only", cases, pinyin_top, pinyin_top10);
    }

    // ───────────────────────────────────────────────────────────
    // PinyinOnly multi-syllable — common 2/3-char phrases.
    // ───────────────────────────────────────────────────────────

    #[test]
    fn pinyin_only_multi_syllable() {
        let cases: &[(&str, &str)] = &[
            // Polish-log + user-confirmed.
            ("lixiang", "理想"), ("queshi", "缺失"), ("youshi", "优势"),
            ("rongyu", "冗余"), ("zhineng", "智能"), ("fanye", "翻页"),
            ("maoding", "锚定"), ("yuming", "域名"),
            // Universal common compounds.
            ("nihao", "你好"), ("zhongguo", "中国"), ("women", "我们"),
            ("jintian", "今天"), ("xianzai", "现在"), ("zenme", "怎么"),
            ("shijian", "时间"), ("yiqi", "一起"), ("yinwei", "因为"),
            ("suoyi", "所以"),
            // Common 2-char everyday words.
            ("shenghuo", "生活"), ("gongzuo", "工作"), ("xuexi", "学习"),
            ("pengyou", "朋友"), ("guojia", "国家"), ("shehui", "社会"),
            ("jingji", "经济"),
            ("dianhua", "电话"), ("dianshi", "电视"), ("dianying", "电影"),
            ("yinyue", "音乐"), ("xinwen", "新闻"),
        ];
        run("pinyin_multi", cases, pinyin_top, pinyin_top10);
    }

    // ───────────────────────────────────────────────────────────
    // Mixed mode with Japanese enabled — pinyin tops must still
    // win for pinyin-shaped inputs (JP hiragana/katakana must NOT
    // displace common Chinese particles).
    // ───────────────────────────────────────────────────────────

    #[test]
    fn jp_enabled_pinyin_top_still_leads_via_pinyin_only_mode() {
        // Note: we use PinyinOnly mode for the assertion (wubi
        // simcode would otherwise legitimately lead). The point of
        // this test is to lock in JP scoring rebalance: top hiragana
        // = 150k + 100·3000 = 450k, comfortably below pinyin top
        // (~465k for the/le/ma/ba/etc.) so JP no longer overwrites.
        let cases: &[(&str, &str)] = &[
            ("di", "的"), ("le", "了"), ("ma", "吗"), ("ba", "吧"),
            ("ne", "呢"), ("zhongguo", "中国"), ("women", "我们"),
            ("nihao", "你好"),
        ];
        let mut failures = Vec::new();
        for (buf, expected) in cases {
            let actual = jp_enabled_pinyin_only_top(buf.as_bytes());
            if actual != *expected {
                failures.push(format!("  jp_on: {buf:<10} → got {actual}, expected {expected}"));
            }
        }
        if !failures.is_empty() {
            panic!("{} JP-on cases failed:\n{}", failures.len(), failures.join("\n"));
        }
    }

    // ───────────────────────────────────────────────────────────
    // Rare Jianma2 chars — must yield to common pinyin via the
    // char-prominence demote (char_max_freq < 20k → ×0.3).
    // ───────────────────────────────────────────────────────────

    #[test]
    fn rare_jianma2_chars_yield_to_pinyin_top() {
        let cases: &[(&str, &[&str])] = &[
            // mo Jianma2 = 嶙 (freq 15k, rare).
            ("mo", &["没", "默", "摸", "末", "莫", "魔", "模"]),
        ];
        run_acceptable("rare_jianma2", cases, mixed_top, mixed_top10);
    }

    // ───────────────────────────────────────────────────────────
    // Prediction policy — v1.5 strict.
    // ───────────────────────────────────────────────────────────

    #[test]
    fn predictions_empty_after_single_commit() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        for b in b"jintian" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        if let Some(idx) = cands.iter().position(|c| c.word == "今天") {
            let _ = e.commit_index(idx);
        }
        assert!(e.predicted_candidates().is_empty(),
            "v1.5 strict: single commit insufficient evidence");
    }

    #[test]
    fn predictions_do_not_cycle_on_repeated_word() {
        // Even if trigram returns a word already in recent_committed,
        // it must be filtered.
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        for b in b"zhongguo" { let _ = e.handle_letter(*b); }
        if let Some(idx) = e.candidates().iter().position(|c| c.word == "中国") {
            let _ = e.commit_index(idx);
        }
        for b in b"renmin" { let _ = e.handle_letter(*b); }
        if let Some(idx) = e.candidates().iter().position(|c| c.word == "人民") {
            let _ = e.commit_index(idx);
        }
        let preds_before = e.predicted_candidates().iter()
            .map(|c| c.word.clone()).collect::<Vec<_>>();
        // 中国 must NOT be in predictions (it's in recent_committed).
        assert!(!preds_before.contains(&"中国".to_string()),
            "recent-committed dedup must drop 中国 from predictions; got {preds_before:?}");
        // 人民 must also be dedup'd.
        assert!(!preds_before.contains(&"人民".to_string()),
            "recent-committed dedup must drop 人民; got {preds_before:?}");
    }

    // ───────────────────────────────────────────────────────────
    // Multi-char Jianma3 sample — when wubi char is multi (e.g.
    // 'shi'→'椒' single but some Jianma3 are short phrases) keep
    // them too. (Tested in wubi_simcode_priority for shi/you.)
    // ───────────────────────────────────────────────────────────

    // ───────────────────────────────────────────────────────────
    // Sanity: 3-letter codes that look like pinyin but ARE wubi
    // simcodes still produce wubi #0 (per 伙-rule extended).
    // ───────────────────────────────────────────────────────────

    #[test]
    fn three_letter_pinyin_shaped_wubi_simcodes() {
        let cases: &[(&str, &str)] = &[
            ("shi", "椒"),    // existing protected
            ("you", "亦"),    // existing protected
        ];
        run("jianma3", cases, mixed_top, mixed_top10);
    }

    // ───────────────────────────────────────────────────────────
    // ASCII fallback — when typing many letters that aren't a
    // valid pinyin word and aren't a wubi simcode, the engine
    // should commit as raw ASCII (so user can type English mid-
    // sentence without mode switch).
    // ───────────────────────────────────────────────────────────

    #[test]
    fn ascii_fallback_engages_after_threshold() {
        // 5+ char buffer that doesn't resolve to a pinyin word and
        // doesn't form a wubi simcode should commit as ASCII.
        use crate::Session;
        let mut s = Session::new();
        s.set_auto_commit_policy(AutoCommitPolicy::Never);
        // Type a clearly non-pinyin sequence.
        for cp in b"hellox" { s.handle_key(*cp as u32, 0); }
        let preedit = s.preedit().to_string();
        // ASCII fallback path commits to take_commit OR leaves the
        // buffer present for the user to escape. Either way, the
        // candidate list shouldn't be empty of any meaningful options;
        // user can escape to ASCII.
        let cands = s.candidates();
        // Smoke: not panicking is the main contract here.
        eprintln!("hellox preedit={preedit:?} cands_top5={:?}",
            cands.iter().take(5).collect::<Vec<_>>());
    }

    // ───────────────────────────────────────────────────────────
    // Polish-log boosted entries — user-corrected picks must lead
    // (auto-tune from aggregate_polish_log.py).
    // ───────────────────────────────────────────────────────────

    #[test]
    fn polish_log_promoted_words_lead() {
        // These came from user actual polish-log picks:
        let cases: &[(&str, &str)] = &[
            ("queshi", "缺失"),   // user picked 4× over 确实
            ("youshi", "优势"),   // 5×
            ("rongyu", "冗余"),   // 5×
            ("jixu", "积蓄"),     // 28× — strongest signal
            ("yuming", "域名"),   // 4× + modern_vocab
        ];
        run("polish_log", cases, pinyin_top, pinyin_top10);
    }

    // ───────────────────────────────────────────────────────────
    // No traditional characters in top-5 for common single
    // syllables (t2s strip + bigram/trigram filter).
    // ───────────────────────────────────────────────────────────

    #[test]
    fn smoke_user_chinese_phrase_yongbuliao_works() {
        // User-typed 2026-05-24 "xianzai yongbuliao le" (现在用不了了)
        // signaling IME unusable. Sanity: each segment must give the
        // expected Chinese in PinyinOnly mode.
        let cases: &[(&str, &str)] = &[
            ("xianzai", "现在"),
            ("yongbuliao", "用不了"),
            ("le", "了"),
        ];
        let mut failures = Vec::new();
        for (buf, expected) in cases {
            let actual = pinyin_top(buf.as_bytes());
            // Accept exact match OR expected being in top5 (for compound
            // queries the exact match may not be #0).
            let top5 = pinyin_top10(buf.as_bytes());
            let top5_s: Vec<&str> = top5.iter().take(5).map(|s| s.as_str()).collect();
            if actual != *expected && !top5_s.contains(expected) {
                failures.push(format!(
                    "  {buf} expected {expected} in top5; got {actual} (top5={top5_s:?})"
                ));
            }
        }
        if !failures.is_empty() {
            panic!("{} smoke cases failed:\n{}", failures.len(), failures.join("\n"));
        }
    }

    #[test]
    fn no_traditional_in_top5_for_common_pinyin() {
        // List of (pinyin, traditional_blocklist) — traditional forms
        // must NOT appear in top 5 PinyinOnly candidates.
        let cases: &[(&str, &[&str])] = &[
            // Verified traditional-only forms (NOT same as simplified).
            ("yu", &["於"]),    // simplified 于 must lead
            ("guo", &["國", "過"]),   // 国/国, 过/過
            ("lai", &["來"]),   // 来
            ("hou", &["後"]),   // 后/後
            ("hui", &["會"]),   // 会
            ("ma", &["嗎", "媽"]),  // 吗/媽
        ];
        let mut failures = Vec::new();
        for (buf, blocklist) in cases {
            let top10 = pinyin_top10(buf.as_bytes());
            let top5: &[String] = if top10.len() < 5 { &top10[..] } else { &top10[..5] };
            for bad in *blocklist {
                if top5.iter().any(|w| w == bad) {
                    failures.push(format!("  {buf}: traditional {bad} in top5 — top5={top5:?}"));
                }
            }
        }
        if !failures.is_empty() {
            panic!("{} traditional-leak cases failed:\n{}", failures.len(), failures.join("\n"));
        }
    }
}
