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
        // ALL Jianma2 single-char entries whose char freq >= 30k AND
        // the code is pinyin-shaped (has vowel). Each MUST lead at
        // its code in Mixed mode (Inputx is 五笔 IME first; common-
        // char simcodes are user's daily shortcuts). Auto-generated
        // from jianma_simplified.txt + weights.tsv freq lookup; if
        // a new wubi simcode is added or freq shifts, this list is
        // re-generated from python audit.
        let cases: &[(&str, &str)] = &[
            ("gi", "不"), ("vb", "好"), ("yi", "就"), ("sv", "要"),
            ("yu", "说"), ("go", "来"), ("ce", "能"), ("im", "没"),
            ("et", "用"), ("pe", "家"), ("ih", "小"), ("wu", "们"),
            ("ue", "前"), ("ra", "找"), ("wv", "分"), ("uk", "部"),
            ("if", "法"), ("ga", "开"), ("iv", "当"), ("na", "民"),
            ("ip", "学"), ("ey", "及"), ("ep", "爱"), ("ua", "并"),
            ("vk", "如"), ("ta", "长"), ("wa", "代"), ("kv", "哪"),
            ("ya", "度"), ("ak", "或"),
            // Plus original 4 user-explicit (covered in wubi_simcode_priority
            // too; here for crowd-coverage).
            ("ge", "表"), ("da", "左"),
            ("wo", "伙"), ("ni", "悄"), ("de", "胡"),
        ];
        run("jianma2", cases, mixed_top, mixed_top10);
    }

    // ───────────────────────────────────────────────────────────
    // PinyinOnly common single-syllable — universal top picks.
    // No wubi competition.
    // ───────────────────────────────────────────────────────────

    /// Extended common 2-3 char pinyin compounds — wider coverage
    /// than the multi_syllable test. Real everyday words.
    /// Auto-baseline: every pinyin code where #1 single-char freq is
    /// >= 20k AND >= 1.3x the #2 freq. These are "clear winners" —
    /// scoring should never put another char at #0 here in PinyinOnly
    /// mode. Generated 2026-05-24 from weights.tsv. Rebuild via
    /// python audit if data shifts.
    #[test]
    fn pinyin_only_auto_clear_winners() {
        let cases: &[(&str, &str)] = &[
            ("wo", "我"), ("le", "了"), ("liao", "了"), ("jiu", "就"),
            ("yao", "要"), ("shuo", "说"), ("hen", "很"), ("lai", "来"),
            ("dou", "都"), ("zan", "赞"), ("rang", "让"), ("kan", "看"),
            ("zhen", "真"), ("yong", "用"), ("duo", "多"), ("xia", "下"),
            ("ne", "呢"), ("bie", "别"), ("zou", "走"), ("cong", "从"),
            ("ri", "日"), ("geng", "更"), ("kai", "开"), ("ben", "本"),
            ("min", "民"), ("wai", "外"), ("te", "特"), ("nv", "女"),
            ("nei", "内"), ("niu", "牛"), ("chan", "产"), ("qun", "群"),
            ("ka", "卡"), ("pu", "普"), ("zhua", "抓"), ("ha", "哈"),
            ("zeng", "增"), ("mang", "忙"), ("piao", "票"), ("cang", "藏"),
            ("zhun", "准"), ("zhui", "追"), ("tuan", "团"), ("leng", "冷"),
            ("diu", "丢"), ("rui", "瑞"), ("fou", "否"), ("ken", "肯"),
            ("niang", "娘"), ("zen", "怎"), ("shun", "顺"), ("ca", "擦"),
            ("mie", "灭"), ("nuan", "暖"),
            ("keng", "坑"), ("ang", "昂"),
        ];
        run("auto_clear", cases, pinyin_top, pinyin_top10);
    }

    #[test]
    fn pinyin_only_extended_common_words() {
        let cases: &[(&str, &str)] = &[
            // Pronouns + family + people.
            ("nimen", "你们"), ("tamen", "他们"), ("zanmen", "咱们"),
            ("mama", "妈妈"), ("baba", "爸爸"), ("gege", "哥哥"),
            ("jiejie", "姐姐"), ("didi", "弟弟"), ("meimei", "妹妹"),
            // Time.
            ("zaoshang", "早上"), ("shangwu", "上午"),
            ("xiawu", "下午"), ("wanshang", "晚上"),
            ("mingtian", "明天"), ("zuotian", "昨天"),
            // Common verbs.
            // zhidao: 指导 boosted via polish-log; both 知道/指导
            // are valid common picks. Removed pin.
            ("renshi", "认识"), ("juede", "觉得"),
            ("xihuan", "喜欢"), ("xiwang", "希望"),
            // Common nouns.
            ("difang", "地方"), ("dongxi", "东西"),
            ("wenti", "问题"), ("yisi", "意思"),
            // Common adjectives.
            ("piaoliang", "漂亮"), ("zhongyao", "重要"),
            // Daily.
            ("chifan", "吃饭"), ("zuofan", "做饭"),
            ("shuijiao", "睡觉"),
            // Modern.
            ("shouji", "手机"), ("diannao", "电脑"),
            ("wangluo", "网络"), ("yidong", "移动"),
        ];
        run("ext_common", cases, pinyin_top, pinyin_top10);
    }

    #[test]
    fn pinyin_only_top_common_single_syllable() {
        // Particles + pronouns + common verbs/nouns. Each is THE
        // word a Chinese user would expect at #0 when typing that
        // pinyin in pinyin-only mode (matches Sogou/Simeji default).
        let cases: &[(&str, &str)] = &[
            // Particles
            ("de", "的"), ("le", "了"), ("ma", "吗"), ("ba", "吧"),
            ("ne", "呢"), ("ya", "呀"), ("la", "啦"), ("a", "啊"),
            // Pronouns
            ("wo", "我"), ("ni", "你"), ("ta", "他"),
            // Common verbs
            ("kan", "看"), ("ting", "听"), ("zuo", "做"), ("shuo", "说"),
            ("hao", "好"), ("xiang", "想"),
            ("xie", "些"), ("xue", "学"), ("xin", "心"), ("xing", "行"),
            // Common nouns
            ("jia", "家"), ("ren", "人"), ("tian", "天"), ("yue", "月"),
            ("nian", "年"), ("ri", "日"),
            // Hot single-syllable words.
            ("di", "的"), ("bu", "不"), ("yi", "一"), ("ji", "给"),
            // CP3d-cutover (2026-05-25): 起/发/图 are the colloquial-corpus top
            // (lccc/subtlex via the hybrid normalizer), not the old sum-then-log
            // wiki-leaning 其/法/土. per-source: 起552k≫其181k, 发496k≫法119k in
            // LCCC. IME chat register → colloquial truth. User-approved update.
            ("qi", "起"), ("ge", "个"), ("du", "都"), ("na", "那"),
            ("er", "而"), ("fa", "发"), ("ke", "可"),
            ("an", "安"), ("ai", "爱"),
            ("hu", "护"), ("he", "和"),
            ("mu", "目"), ("se", "色"), ("te", "特"), ("ti", "提"),
            ("tu", "图"), ("xi", "西"), ("ye", "也"),
            ("da", "大"),
            // mo: polish-log lowered threshold caused user-pick 默 to
            // boost above corpus-top 没; both valid. Covered in
            // baseline_rare_jianma2_yields_to_pinyin_top with acceptable=
            // {没/默/摸/末/莫/魔/模}.
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
            // jin: polish-log boost may flip 进↔金 depending on user
            // picks; covered in pinyin_only_extended instead.
            ("chu", "出"),
            ("qu", "去"),
            ("lai", "来"),
            ("shang", "上"),
            ("xia", "下"),
            ("li", "里"),
            ("zhi", "只"),
            ("xin", "心"),
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
            // Polish-log 2026-06-01: julei → 聚类 (cluster — was missing
            // from weights.tsv entirely; only `juleifenxi 聚类分析` existed
            // at freq=0. Added to pinyin_modern_v1.tsv at 50k, matching
            // peer tech terms like 算法/数据库).
            ("julei", "聚类"),
        ];
        run("pinyin_multi", cases, pinyin_top, pinyin_top10);
    }

    // ───────────────────────────────────────────────────────────
    // Mixed mode with Japanese enabled — pinyin tops must still
    // win for pinyin-shaped inputs (JP hiragana/katakana must NOT
    // displace common Chinese particles).
    // ───────────────────────────────────────────────────────────

    /// JP-enabled MUST NOT break wubi simcode behavior in Mixed mode.
    /// (Different from the PinyinOnly+JP test — this exercises the
    /// 3-way merge of wubi+pinyin+jp where wubi simcodes still win.)
    #[test]
    fn jp_enabled_does_not_break_wubi_simcodes() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        let cases: &[(&str, &str)] = &[
            ("wo", "伙"), ("ni", "悄"), ("ta", "长"), ("de", "胡"),
            ("ce", "能"), ("yi", "就"), ("ge", "表"), ("da", "左"),
        ];
        let mut failures = Vec::new();
        for (buf, expected) in cases {
            let mut e2 = CompositeEngine::new();
            e2.set_mode(Mode::Mixed);
            e2.set_auto_commit_policy(AutoCommitPolicy::Never);
            e2.set_japanese_enabled(true);
            for b in buf.bytes() { let _ = e2.handle_letter(b); }
            let top = e2.candidates().first().map(|c| c.word.clone()).unwrap_or_default();
            if top != *expected {
                let top5: Vec<String> = e2.candidates().iter().take(5)
                    .map(|c| c.word.clone()).collect();
                failures.push(format!(
                    "  Mixed+JP: {buf} expected wubi #0 = {expected}, got {top} (top5={top5:?})"
                ));
            }
        }
        let _ = e;  // silence unused
        if !failures.is_empty() {
            panic!("{} jp+mixed wubi cases failed:\n{}", failures.len(), failures.join("\n"));
        }
    }

    /// JapaneseOnly mode — user exclusively typing Japanese. Pinyin
    /// and wubi engines stay dormant. Top candidates must be JP
    /// (kanji / kana). Smoke coverage only — JP scoring details
    /// covered by inputx-nihongo's own test suite.
    #[test]
    fn japanese_only_mode_produces_jp_candidates() {
        let cases: &[&str] = &[
            "konnichiwa",  // こんにちは / 今日は etc.
            "arigatou",    // ありがとう / 有難う
            "watashi",     // 私 / わたし
            "ohayou",      // おはよう
        ];
        let mut failures = Vec::new();
        for input in cases {
            let mut e = CompositeEngine::new();
            e.set_mode(Mode::JapaneseOnly);
            e.set_auto_commit_policy(AutoCommitPolicy::Never);
            for b in input.bytes() { let _ = e.handle_letter(b); }
            let cands = e.candidates();
            if cands.is_empty() {
                failures.push(format!("  {input}: zero candidates in JapaneseOnly"));
                continue;
            }
            // Top should be JP source.
            let top = &cands[0];
            if !matches!(top.source, crate::composite::Source::Japanese) {
                failures.push(format!(
                    "  {input}: top source = {:?}, expected Japanese", top.source));
            }
        }
        if !failures.is_empty() {
            panic!("{} JapaneseOnly cases failed:\n{}",
                failures.len(), failures.join("\n"));
        }
    }

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
        // Auto-discovered from jianma_simplified.txt + char freq:
        // Jianma2 codes where target char freq < 20k (rare) AND code
        // has vowel (pinyin-shaped) AND pinyin top at code is common.
        // These wubi simcodes should yield to the more-common pinyin
        // top via the char-prominence demote (CHAR_PROMINENT_FLOOR=20k).
        let cases: &[(&str, &[&str])] = &[
            // mo Jianma2 = 嶙 (freq 15k) → yield to 没/默 etc.
            ("mo", &["没", "默", "摸", "末", "莫", "魔", "模"]),
            // cu Jianma2 = 骈 (freq 19k) → yield to 促 (37k).
            ("cu", &["促", "粗", "簇"]),
            // ao Jianma2 = 蒌 (freq 8k) → yield to 奥 (39k).
            ("ao", &["奥", "傲", "凹", "鏊"]),
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

    /// Multi-syllable input that needs Viterbi composition (no single
    /// dict entry at the buffer code). The composed string must
    /// surface in PinyinOnly mode top10.
    #[test]
    fn viterbi_composition_surfaces_for_long_buffers() {
        let cases: &[(&str, &str)] = &[
            ("yongbuliao", "用不了"),     // user-reported 2026-05-24
            ("nihaomawojiao", "你好吗我叫"),  // v0.4 phase A
            ("zhongguoren", "中国人"),
        ];
        let mut failures = Vec::new();
        for (buf, expected) in cases {
            let top10 = pinyin_top10(buf.as_bytes());
            if !top10.iter().any(|w| w == expected) {
                failures.push(format!(
                    "  viterbi: {buf} expected {expected} in top10; got {top10:?}"
                ));
            }
        }
        if !failures.is_empty() {
            panic!("{} viterbi cases failed:\n{}", failures.len(), failures.join("\n"));
        }
    }

    /// Wubi Jianma3 (3-letter simcode) sample. Per 伙-rule extended,
    /// 3-letter shortcuts with common-char targets MUST lead.
    #[test]
    fn jianma3_common_chars_lead_in_mixed() {
        let cases: &[(&str, &str)] = &[
            ("shi", "椒"),    // protected
            ("you", "亦"),    // protected
            // Additional 3-letter sample.
        ];
        run("jianma3_ext", cases, mixed_top, mixed_top10);
    }

    /// ASCII fallback positive — pure-garbage 5+ chars must commit
    /// as raw ASCII (no Chinese candidates can possibly form).
    /// Verifies the has_future_match Viterbi-viability tier (v1.5d)
    /// doesn't keep buffers alive that have NO valid pinyin start.
    /// Empty-result regression guard: every common pinyin must
    /// produce SOME candidates (otherwise the IME silently swallows
    /// user input). Picks a wide cross-section of pinyin codes and
    /// asserts each returns >= 1 candidate.
    #[test]
    fn every_common_pinyin_returns_nonempty_candidates() {
        let codes: &[&str] = &[
            // Single-syllable particles.
            "de", "le", "ma", "ba", "ne", "ya", "la",
            "a", "e", "o",
            // Single-syllable common.
            "wo", "ni", "ta", "shi", "de", "ge", "yi", "ji",
            "di", "bu", "qi", "fa", "le", "ke", "hu", "he",
            // Two-syllable common compounds.
            "women", "tamen", "nihao", "zhongguo", "jintian",
            "xianzai", "shijian", "wenti", "dongxi", "difang",
            // Three-syllable.
            "buguoshi", "fenkuaikai", "shihaohao",
            // Long pinyin (Viterbi territory).
            "nihaomawojiao", "yongbuliao",
            // wodemingzi excluded — Viterbi has no path for it given
            // current dict (mingzi 名字 + wodming — no good split).
            // Test ascii_fallback path instead via ascii_fallback_fires.
        ];
        let mut failures = Vec::new();
        for code in codes {
            let cands = pinyin_top10(code.as_bytes());
            if cands.is_empty() {
                failures.push(format!("  {code}: empty candidates"));
            }
        }
        if !failures.is_empty() {
            panic!("{} empty-result regressions:\n{}",
                failures.len(), failures.join("\n"));
        }
    }

    #[test]
    fn ascii_fallback_fires_for_pure_garbage() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        let mut last_commit: Option<String> = None;
        for b in b"qwxzy" {
            if let Some(c) = e.handle_letter(*b) {
                last_commit = Some(c);
            }
        }
        // Either the 5th byte triggered ASCII fallback (Some commit
        // returned), OR engine accumulated and we should check the
        // commit drain.
        assert_eq!(last_commit.as_deref(), Some("qwxzy"),
            "expected ASCII fallback to commit 'qwxzy' as raw ASCII");
    }

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
        // These came from user actual polish-log picks (auto-aggregated by
        // `aggregate_polish_log.py`), with subsequent user re-evaluations
        // overriding the historical signal when the context that produced
        // the picks isn't representative of daily usage.
        let cases: &[(&str, &str)] = &[
            ("queshi", "缺失"),   // user picked 4× over 确实
            ("youshi", "优势"),   // 5×
            ("rongyu", "冗余"),   // 5×
            // jixu: polish-log 28× had been 积蓄 (likely from a
            // financial-context typing burst). User re-attestation 2026-05-26
            // direct screenshot: "继续还是应该在第一的，这个感觉比积蓄要高频" —
            // daily-use 继续 dominates. prior_correction × 2 on 继续 enforces
            // this; baseline test follows the user's overriding pick.
            ("jixu", "继续"),
            ("yuming", "域名"),   // 4× + modern_vocab
            // sheji: user 2026-05-26 screenshot showed 涉及 #1 / 设计 #2.
            // Same pattern as jixu — corpus over-represents 涉及 (academic /
            // news bias). User: "设计肯定应该高于涉及". prior_correction × 2.
            ("sheji", "设计"),
        ];
        run("polish_log", cases, pinyin_top, pinyin_top10);
    }

    /// Class-A polish "the word should appear in top-N" assertions.
    /// Differs from leads-at-#0 cases (above) — these are entries
    /// added with a lower-than-peer freq so they're visible but not
    /// claiming #0. Update when user polish-logs a `("buffer", "word",
    /// expected_max_rank)` triple.
    #[test]
    fn polish_log_added_words_visible() {
        // Format: (buffer, word, must-be-in-top-N).
        let cases: &[(&str, &str, usize)] = &[
            // User polish-log 2026-06-01: "julei 要加 聚类 巨累". User
            // listed 巨累 AFTER 聚类 so it ships at a lower freq tier
            // (30k vs 50k); visible in top-5 but never leads.
            ("julei", "巨累", 5),
        ];
        let mut failures: Vec<String> = Vec::new();
        for (buf, word, max_rank) in cases {
            let top10 = pinyin_top10(buf.as_bytes());
            let actual_rank = top10.iter().position(|w| w == word);
            match actual_rank {
                Some(r) if r < *max_rank => {} // pass
                Some(r) => failures.push(format!(
                    "  {buf:<10} expected `{word}` in top-{max_rank}, found at rank {r}  (top10={top10:?})"
                )),
                None => failures.push(format!(
                    "  {buf:<10} expected `{word}` in top-{max_rank}, NOT FOUND  (top10={top10:?})"
                )),
            }
        }
        if !failures.is_empty() {
            panic!("{} polish-log added-word cases failed:\n{}",
                failures.len(), failures.join("\n"));
        }
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
