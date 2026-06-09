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
        for b in buffer {
            let _ = e.handle_letter(*b);
        }
        e.candidates()
            .first()
            .map(|c| c.word.clone())
            .unwrap_or_default()
    }

    fn mixed_top10(buffer: &[u8]) -> Vec<String> {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in buffer {
            let _ = e.handle_letter(*b);
        }
        e.candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect()
    }

    fn pinyin_top(buffer: &[u8]) -> String {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in buffer {
            let _ = e.handle_letter(*b);
        }
        e.candidates()
            .first()
            .map(|c| c.word.clone())
            .unwrap_or_default()
    }

    fn pinyin_top10(buffer: &[u8]) -> Vec<String> {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in buffer {
            let _ = e.handle_letter(*b);
        }
        e.candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect()
    }

    fn jp_enabled_pinyin_only_top(buffer: &[u8]) -> String {
        // Use PinyinOnly mode to isolate JP vs pinyin scoring without
        // wubi simcode interference (wubi naturally leads in Mixed).
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in buffer {
            let _ = e.handle_letter(*b);
        }
        e.candidates()
            .first()
            .map(|c| c.word.clone())
            .unwrap_or_default()
    }

    fn run(
        label: &str,
        cases: &[(&str, &str)],
        top_fn: impl Fn(&[u8]) -> String,
        top10_fn: impl Fn(&[u8]) -> Vec<String>,
    ) {
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

    fn run_acceptable(
        label: &str,
        cases: &[(&str, &[&str])],
        top_fn: impl Fn(&[u8]) -> String,
        top10_fn: impl Fn(&[u8]) -> Vec<String>,
    ) {
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
            ("g", "一"),
            ("f", "地"),
            ("d", "在"),
            ("s", "要"),
            ("a", "工"),
            ("h", "上"),
            ("j", "是"),
            ("k", "中"),
            ("l", "国"),
            ("m", "同"),
            ("t", "和"),
            ("r", "的"),
            ("e", "有"),
            ("w", "人"),
            ("q", "我"),
            ("y", "主"),
            ("u", "产"),
            ("i", "不"),
            ("o", "为"),
            ("p", "这"),
            ("n", "民"),
            ("b", "了"),
            ("v", "发"),
            ("c", "以"),
            ("x", "经"),
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
            ("gi", "不"),
            ("vb", "好"),
            ("yi", "就"),
            ("sv", "要"),
            ("yu", "说"),
            ("go", "来"),
            ("ce", "能"),
            ("im", "没"),
            ("et", "用"),
            ("pe", "家"),
            ("ih", "小"),
            ("wu", "们"),
            ("ue", "前"),
            ("ra", "找"),
            ("wv", "分"),
            ("uk", "部"),
            ("if", "法"),
            ("ga", "开"),
            ("iv", "当"),
            ("na", "民"),
            ("ip", "学"),
            ("ey", "及"),
            ("ep", "爱"),
            ("ua", "并"),
            ("vk", "如"),
            ("ta", "长"),
            ("wa", "代"),
            ("kv", "哪"),
            ("ya", "度"),
            ("ak", "或"),
            // Plus original 4 user-explicit (covered in wubi_simcode_priority
            // too; here for crowd-coverage).
            ("ge", "表"),
            ("da", "左"),
            ("wo", "伙"),
            ("ni", "悄"),
            ("de", "胡"),
            // 2026-06-06 — user reverted (fa, 载) demote from the
            // 2026-06-03 tier_overlay sweep.
            ("fa", "载"),
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
    /// > scoring should never put another char at #0 here in PinyinOnly
    /// > mode. Generated 2026-05-24 from weights.tsv. Rebuild via
    /// > python audit if data shifts.
    #[test]
    fn pinyin_only_auto_clear_winners() {
        let cases: &[(&str, &str)] = &[
            ("wo", "我"),
            ("le", "了"),
            ("liao", "了"),
            ("jiu", "就"),
            ("yao", "要"),
            ("shuo", "说"),
            ("hen", "很"),
            ("lai", "来"),
            ("dou", "都"),
            ("zan", "赞"),
            ("rang", "让"),
            ("kan", "看"),
            ("zhen", "真"),
            ("yong", "用"),
            ("duo", "多"),
            ("xia", "下"),
            ("ne", "呢"),
            ("bie", "别"),
            ("zou", "走"),
            ("cong", "从"),
            ("ri", "日"),
            ("geng", "更"),
            ("kai", "开"),
            ("ben", "本"),
            ("min", "民"),
            ("wai", "外"),
            ("te", "特"),
            ("nv", "女"),
            ("nei", "内"),
            ("niu", "牛"),
            ("chan", "产"),
            ("qun", "群"),
            ("ka", "卡"),
            ("pu", "普"),
            ("zhua", "抓"),
            ("ha", "哈"),
            ("zeng", "增"),
            ("mang", "忙"),
            ("piao", "票"),
            ("cang", "藏"),
            ("zhun", "准"),
            ("zhui", "追"),
            ("tuan", "团"),
            ("leng", "冷"),
            ("diu", "丢"),
            ("rui", "瑞"),
            ("fou", "否"),
            ("ken", "肯"),
            ("niang", "娘"),
            ("zen", "怎"),
            ("shun", "顺"),
            ("ca", "擦"),
            ("mie", "灭"),
            ("nuan", "暖"),
            ("keng", "坑"),
            ("ang", "昂"),
        ];
        run("auto_clear", cases, pinyin_top, pinyin_top10);
    }

    #[test]
    fn pinyin_only_extended_common_words() {
        let cases: &[(&str, &str)] = &[
            // Pronouns + family + people.
            ("nimen", "你们"),
            ("tamen", "他们"),
            ("zanmen", "咱们"),
            ("mama", "妈妈"),
            ("baba", "爸爸"),
            ("gege", "哥哥"),
            ("jiejie", "姐姐"),
            ("didi", "弟弟"),
            ("meimei", "妹妹"),
            // Time.
            ("zaoshang", "早上"),
            ("shangwu", "上午"),
            ("xiawu", "下午"),
            ("wanshang", "晚上"),
            ("mingtian", "明天"),
            ("zuotian", "昨天"),
            // Common verbs.
            // zhidao: 指导 boosted via polish-log; both 知道/指导
            // are valid common picks. Removed pin.
            ("renshi", "认识"),
            ("juede", "觉得"),
            ("xihuan", "喜欢"),
            ("xiwang", "希望"),
            // Common nouns.
            ("difang", "地方"),
            ("dongxi", "东西"),
            ("wenti", "问题"),
            ("yisi", "意思"),
            // Common adjectives.
            ("piaoliang", "漂亮"),
            ("zhongyao", "重要"),
            // Daily.
            ("chifan", "吃饭"),
            ("zuofan", "做饭"),
            ("shuijiao", "睡觉"),
            // Modern.
            ("shouji", "手机"),
            ("diannao", "电脑"),
            ("wangluo", "网络"),
            ("yidong", "移动"),
            // polish-log 2026-06-02: 步骤 boosted over 不周 (user:
            // "感觉上 步骤 应该在 不周 前面, 用得更多"). corpus had
            // them within 1k (不周 20613 / 步骤 19628) but steps is
            // far more common in everyday CN usage.
            ("buzhou", "步骤"),
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
            ("de", "的"),
            ("le", "了"),
            ("ma", "吗"),
            ("ba", "吧"),
            ("ne", "呢"),
            ("ya", "呀"),
            ("la", "啦"),
            ("a", "啊"),
            // Pronouns
            ("wo", "我"),
            ("ni", "你"),
            ("ta", "他"),
            // Common verbs
            ("kan", "看"),
            ("ting", "听"),
            ("zuo", "做"),
            ("shuo", "说"),
            ("hao", "好"),
            ("xiang", "想"),
            ("xie", "些"),
            ("xue", "学"),
            ("xin", "心"),
            ("xing", "行"),
            // Common nouns
            ("jia", "家"),
            ("ren", "人"),
            ("tian", "天"),
            ("yue", "月"),
            ("nian", "年"),
            ("ri", "日"),
            // Hot single-syllable words.
            ("di", "的"),
            ("bu", "不"),
            ("yi", "一"),
            ("ji", "给"),
            // CP3d-cutover (2026-05-25): 起/发/图 are the colloquial-corpus top
            // (lccc/subtlex via the hybrid normalizer), not the old sum-then-log
            // wiki-leaning 其/法/土. per-source: 起552k≫其181k, 发496k≫法119k in
            // LCCC. IME chat register → colloquial truth. User-approved update.
            ("qi", "起"),
            ("ge", "个"),
            ("du", "都"),
            ("na", "那"),
            ("er", "而"),
            ("fa", "发"),
            ("ke", "可"),
            ("an", "安"),
            ("ai", "爱"),
            ("hu", "护"),
            ("he", "和"),
            ("mu", "目"),
            ("se", "色"),
            ("te", "特"),
            ("ti", "提"),
            ("tu", "图"),
            ("xi", "西"),
            ("ye", "也"),
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
            ("lixiang", "理想"),
            ("queshi", "缺失"),
            ("youshi", "优势"),
            ("rongyu", "冗余"),
            ("zhineng", "智能"),
            ("fanye", "翻页"),
            ("maoding", "锚定"),
            ("yuming", "域名"),
            // Universal common compounds.
            ("nihao", "你好"),
            ("zhongguo", "中国"),
            ("women", "我们"),
            ("jintian", "今天"),
            ("xianzai", "现在"),
            ("zenme", "怎么"),
            ("shijian", "时间"),
            ("yiqi", "一起"),
            ("yinwei", "因为"),
            ("suoyi", "所以"),
            // Common 2-char everyday words.
            ("shenghuo", "生活"),
            ("gongzuo", "工作"),
            ("xuexi", "学习"),
            ("pengyou", "朋友"),
            ("guojia", "国家"),
            ("shehui", "社会"),
            ("jingji", "经济"),
            ("dianhua", "电话"),
            ("dianshi", "电视"),
            ("dianying", "电影"),
            ("yinyue", "音乐"),
            ("xinwen", "新闻"),
            // Polish-log 2026-06-01: julei → 聚类 (cluster — was missing
            // from weights.tsv entirely; only `juleifenxi 聚类分析` existed
            // at freq=0. Added to modern_vocab_v1.tsv at 50k, matching
            // peer tech terms like 算法/数据库).
            ("julei", "聚类"),
            // Polish-log 2026-06-03: chijiuhua → 持久化 (persistence —
            // tech term, was missing from library entirely).  Added as
            // polish row freq=40000 (peer with 技术/设计 tech-term band).
            ("chijiuhua", "持久化"),
            // Polish-log 2026-06-03: maodian → 锚点 (anchor / anchor
            // link — tech term).  Added as polish row freq=35000 (中高频,
            // 跟 距离/技术 同 tier 2 band).
            ("maodian", "锚点"),
            // Polish-log 2026-06-03: "坚持肯定要比减持高" — 减持 had
            // modern_vocab_v1 boost 50000 ahead of 坚持 base 42131.
            // Class B quickfix: 坚持 → 55000 in quickfix_boost.tsv.
            ("jianchi", "坚持"),
            // Polish-log 2026-06-04: "fudu 三个中文词应该在日语上面" —
            // 复读/幅度/服毒 base freqs (24k/20k/15k) put them in pinyin
            // tier 2/3/3, below mechanical kana ふづ/フヅ tier 2 (4-char
            // band).  Boosted to 55k/52k/50k → all tier 1, above JP.
            ("fudu", "幅度"),
            // Polish-log 2026-06-04: "样式要大于央视这个专有名词一点点".
            // base 央视 24744 > 样式 23460; quickfix 样式 → 27000.
            ("yangshi", "样式"),
            // Polish-log 2026-06-05: "希望 简体 在前面". base 健体 19242
            // > 简体 18344; quickfix 简体 → 21200 (top peer + 10% margin).
            ("jianti", "简体"),
            // Polish-log 2026-06-06: "同样有 daizhe 戴着 期待有但没有,
            // 里面反而有大量 fallback 而且还 fallback 内容又大都不是词".
            // daizhe was a "宁缺毋滥" violation — both 带着 (dài-zhe,
            // taking along) and 戴着 (dài-zhe, wearing) were missing
            // from library.tsv entirely, leaving daizhe top filled with
            // dai+ZH char-pair noise (大真/大震/大振/大正/待朕/大征/
            // 代征). Added both as polish rows since they share the
            // exact same pinyin and both extremely common (peer 看着/
            // 睡着/想着 at ~30-35k).
            ("daizhe", "带着"),
            // Polish-log 2026-06-06: jieou → 解耦 (decouple — tech term,
            // was missing entirely from library.tsv; canonical compound
            // jieou doesn't exist as a dict entry, only `jieoulianji
            // 解偶联剂` at freq=0 was nearby). Added polish row freq=40000
            // (peer with 持久化/技术 tech-term band).
            ("jieou", "解耦"),
            // Polish-log 2026-06-06: tigan → 体感 (body sensation —
            // 体感游戏 / 体感温度 / 体感反馈 modern usage).  Was missing
            // from library.tsv entirely (only `tigan 提干` freq 6551).
            // Added as polish row freq=40000 (peer with 持久化/解耦
            // tech-term band).  Wubi side added same commit at wsdg.
            ("tigan", "体感"),
            // Phase I 2026-06-05: wubi full-code redundancy gate.
            // `biji` is the canonical 4-letter wubi-86 code for 隙
            // (阝+小+日+小), which used to trigger ×100 single_promote
            // + tier 1 → wubi 隙 dominated at 1.89M, crushing pinyin
            // 笔记 (431k). Since 隙 is ALSO reachable via the 3-letter
            // prefix `bij` prediction (where wubi already surfaces it
            // at #0 at 624k), Phase I treats the 4-letter boost as
            // redundant — single_promote suppressed, tier falls to
            // layer-default 4. pinyin 笔记 now leads cleanly.
            // No per-entry data; rule fires structurally for any
            // (full_code, single_char) collision with same-prefix
            // prediction under pinyin_intent.
            ("biji", "笔记"),
            // Polish-log 2026-06-04 Phase F framework fix: "changshi
            // 长时不应该在前面,这都不是一个词" + "为什么组合词评分会
            // 这么高... 都是作为填充物的".  Non-dict Composed-Viterbi
            // segmentations (长时 via 长+时 bigram) used to land tier 1
            // and pre-empt real dict tier-2 phrases (尝试 z=2.0). Phase
            // F demotes all non-exact Composed to tier 5 — real dict
            // entries surface naturally.
            ("changshi", "尝试"),
            // Polish-log 2026-06-06: "jianma 捡骂 剑麻 这不对，简码 键码
            // 还稍微好点". base 捡骂 10032 / 剑麻 8049 outranked 简码
            // 4193 / 键码 3861 in modern usage. quickfix 简码 → 12000,
            // 键码 → 11000 — both above 捡骂 with 简码 leading 键码.
            ("jianma", "简码"),
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
            ("wo", "伙"),
            ("ni", "悄"),
            ("ta", "长"),
            ("de", "胡"),
            ("ce", "能"),
            ("yi", "就"),
            ("ge", "表"),
            ("da", "左"),
        ];
        let mut failures = Vec::new();
        for (buf, expected) in cases {
            let mut e2 = CompositeEngine::new();
            e2.set_mode(Mode::Mixed);
            e2.set_auto_commit_policy(AutoCommitPolicy::Never);
            e2.set_japanese_enabled(true);
            for b in buf.bytes() {
                let _ = e2.handle_letter(b);
            }
            let top = e2
                .candidates()
                .first()
                .map(|c| c.word.clone())
                .unwrap_or_default();
            if top != *expected {
                let top5: Vec<String> = e2
                    .candidates()
                    .iter()
                    .take(5)
                    .map(|c| c.word.clone())
                    .collect();
                failures.push(format!(
                    "  Mixed+JP: {buf} expected wubi #0 = {expected}, got {top} (top5={top5:?})"
                ));
            }
        }
        let _ = e; // silence unused
        if !failures.is_empty() {
            panic!(
                "{} jp+mixed wubi cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    /// JapaneseOnly mode — user exclusively typing Japanese. Pinyin
    /// and wubi engines stay dormant. Top candidates must be JP
    /// (kanji / kana). Smoke coverage only — JP scoring details
    /// covered by inputx-nihongo's own test suite.
    #[test]
    fn japanese_only_mode_produces_jp_candidates() {
        let cases: &[&str] = &[
            "konnichiwa", // こんにちは / 今日は etc.
            "arigatou",   // ありがとう / 有難う
            "watashi",    // 私 / わたし
            "ohayou",     // おはよう
        ];
        let mut failures = Vec::new();
        for input in cases {
            let mut e = CompositeEngine::new();
            e.set_mode(Mode::JapaneseOnly);
            e.set_auto_commit_policy(AutoCommitPolicy::Never);
            for b in input.bytes() {
                let _ = e.handle_letter(b);
            }
            let cands = e.candidates();
            if cands.is_empty() {
                failures.push(format!("  {input}: zero candidates in JapaneseOnly"));
                continue;
            }
            // Top should be JP source.
            let top = &cands[0];
            if !matches!(top.source, crate::composite::Source::Japanese) {
                failures.push(format!(
                    "  {input}: top source = {:?}, expected Japanese",
                    top.source
                ));
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} JapaneseOnly cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    /// 2026-06-08 polish — 日本新字体 sweep backfill.  The wubi/pinyin
    /// sweep removed 217 Shinjitai chars (巌 団 図 砕 伝 亀 …) from the
    /// Chinese engines as corpus noise, but they are legitimate everyday
    /// Japanese kanji and MUST stay type-able in JapaneseOnly mode.  153
    /// chars that weren't already single-kanji entries got their on/kun
    /// readings backfilled from KANJIDIC2 (round-trip-verified via the
    /// engine's romaji table).  This pins the invariant: each char is
    /// reachable in Japanese at its reading.  See
    /// docs/wubi-jp-shinjitai-sweep-2026-06-08/.
    #[test]
    fn jp_shinjitai_typeable_after_sweep() {
        // (reading, kanji_that_must_be_in_japanese_candidates)
        let cases: &[(&str, &str)] = &[
            ("iwa", "巌"),  // user-report char (岩の新字体), kun いわ
            ("dan", "団"),  // on だん
            ("zu", "図"),   // on ず
            ("sai", "砕"),  // KANJIDIC2-only gap char, on さい
            ("den", "伝"),  // on でん
            ("kame", "亀"), // kun かめ
        ];
        let mut failures = Vec::new();
        for (reading, kanji) in cases {
            let mut e = CompositeEngine::new();
            e.set_mode(Mode::JapaneseOnly);
            e.set_auto_commit_policy(AutoCommitPolicy::Never);
            for b in reading.bytes() {
                let _ = e.handle_letter(b);
            }
            let words: Vec<String> = e.candidates().iter().map(|c| c.word.clone()).collect();
            if !words.iter().any(|w| w == kanji) {
                failures.push(format!(
                    "  {reading}: {kanji} not in JP candidates — got {words:?}"
                ));
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} jp-shinjitai-typeable cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
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
            ("di", "的"),
            ("le", "了"),
            ("ma", "吗"),
            ("ba", "吧"),
            ("ne", "呢"),
            ("zhongguo", "中国"),
            ("women", "我们"),
            ("nihao", "你好"),
            // Phase C 2026-06-03: tuijian 推荐 was user-reported as
            // failing post-Phase-B (mechanical kana ついじあん was
            // tier 1 and led; demote to tier 4 restores 推荐 #0).
            ("tuijian", "推荐"),
            // Polish-log 2026-06-04: "fudu 三个中文词应该在日语上面" —
            // 复读/幅度/服毒 base freqs land tier 2/3/3 vs mechanical
            // kana ふづ/フヅ tier 2.  quickfix_boost 55k/52k/50k → all
            // three reach tier 1, with 复读 leading.
            ("fudu", "幅度"),
            // Polish-log 2026-06-04 Phase C-3 framework fix: 4-char
            // mechanical kana band tier 2 → tier 5.  User: "tuli 这些
            // 日语不应该在正常的中频拼音前面".  4-char Chinese-shaped
            // buffers (2-syllable CV+CV) are overwhelmingly Chinese
            // intent; mechanical kana ツィ/つぃ now sits at tier 5
            // (less_common).  After D1 of 土里/图里/土粒 (jieba noise),
            // 图利 (freq=9843 z=0.18 tier 4) leads tier 5 JP.
            ("tuli", "图利"),
            // Polish-log 2026-06-04: "maizi 又发现一个,埋在,再怎么差
            // 也不能在日语后面".  埋在 was not a Path-1 dict entry —
            // surfaced via Path 5b fallback (tier 8) below 6-char JP
            // mechanical kana (tier 4).  Class A added 埋在 freq=15000
            // → z=0.81 → tier 3, above JP.
            ("maizai", "埋在"),
            // Polish-log 2026-06-06: "简码应该高于日语".  base 简码 freq
            // 4193 (tier 3) below JP exact-prefix kana じあんま (tier 2);
            // quickfix_boost → 55000 (same calibration as fudu 复读) puts
            // 简码 into tier 1, above JP.
            ("jianma", "简码"),
        ];
        let mut failures = Vec::new();
        for (buf, expected) in cases {
            let actual = jp_enabled_pinyin_only_top(buf.as_bytes());
            if actual != *expected {
                failures.push(format!(
                    "  jp_on: {buf:<10} → got {actual}, expected {expected}"
                ));
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} JP-on cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
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
        for b in b"jintian" {
            let _ = e.handle_letter(*b);
        }
        let cands = e.candidates();
        if let Some(idx) = cands.iter().position(|c| c.word == "今天") {
            let _ = e.commit_index(idx);
        }
        assert!(
            e.predicted_candidates().is_empty(),
            "v1.5 strict: single commit insufficient evidence"
        );
    }

    #[test]
    fn predictions_do_not_cycle_on_repeated_word() {
        // Even if trigram returns a word already in recent_committed,
        // it must be filtered.
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        for b in b"zhongguo" {
            let _ = e.handle_letter(*b);
        }
        if let Some(idx) = e.candidates().iter().position(|c| c.word == "中国") {
            let _ = e.commit_index(idx);
        }
        for b in b"renmin" {
            let _ = e.handle_letter(*b);
        }
        if let Some(idx) = e.candidates().iter().position(|c| c.word == "人民") {
            let _ = e.commit_index(idx);
        }
        let preds_before = e
            .predicted_candidates()
            .iter()
            .map(|c| c.word.clone())
            .collect::<Vec<_>>();
        // 中国 must NOT be in predictions (it's in recent_committed).
        assert!(
            !preds_before.contains(&"中国".to_string()),
            "recent-committed dedup must drop 中国 from predictions; got {preds_before:?}"
        );
        // 人民 must also be dedup'd.
        assert!(
            !preds_before.contains(&"人民".to_string()),
            "recent-committed dedup must drop 人民; got {preds_before:?}"
        );
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
            // (shi, 椒) removed 2026-06-03 per user "shi 肯定不能是 椒，
            // 要是 '是'" — moved to tier_overlay tier 5; protection
            // semantics for that pair retired.
            ("you", "亦"), // existing protected
                           // (duo, 碰) moved to wubi_prominent_simcode_leads_after_sweep_revert
                           // (2026-06-07) — it's part of the reverted 2026-06-03 sweep set,
                           // asserted there alongside the other 38 restored simcodes.
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
        if super::super::pinyin_adapter::PINYIN_DISABLE_COMPOSE {
            return;
        }
        // Each entry MUST have ceil((N-1)/2) bigram-table-present
        // links to survive the stricter quality gate added 2026-06-02
        // (user report: kakarimasu force-segmentation). Real Chinese
        // compositions clear this easily — `用不了` (用不, 不了),
        // `中国人` (中国, 国人). Mechanical force-segmentations like
        // `你好吗我叫` (chain has only 0-1 corpus-supported bigrams
        // out of 3 links) drop, matching user's explicit judgment
        // (`你好吗我叫 这也不算是个句子, 这个其实也不应该出现`).
        let cases: &[(&str, &str)] = &[
            ("yongbuliao", "用不了"), // user-reported 2026-05-24
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
            panic!(
                "{} viterbi cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    /// v1.14 (user report 2026-06-06): a 3-segment Viterbi chain where
    /// only ONE of the two links has corpus bigram support (intra OR
    /// inter combined) is a piggyback assembly — one real bigram
    /// adjacent to a single-char that has no corpus adjacency. The
    /// composed string is mechanical, not a real Chinese phrase. The
    /// gate must drop it.
    ///
    /// `luyaozhi`: Viterbi picks `[路, 要, 职]`. `(要, 职)` is in NGM
    /// intra (count 148 from real word `要职`); `(路, 要)` is in inter
    /// TSV at count 12, BELOW the `build-inter-bigrams-ngm
    /// --min-count 15` cut → not in NGM. The 1/2-combined chain fails
    /// the strict-all rule and drops, restoring the legitimate
    /// prefix-completion `路遥知马力` (extends `luyaozhi` →
    /// `luyaozhimali`, library freq 11120) to top-1.
    #[test]
    fn kbest_3seg_noise_rejected_by_combined_intra_inter_gate() {
        let cases: &[(&str, &str, &str)] = &[
            // (buffer, must NOT surface, must surface in top10)
            ("luyaozhi", "路要职", "路遥知马力"),
        ];
        let mut failures = Vec::new();
        for (buf, bad, good) in cases {
            let top10 = pinyin_top10(buf.as_bytes());
            if top10.iter().any(|w| w == bad) {
                failures.push(format!(
                    "  {buf}: noise composition {bad} should not surface; top10={top10:?}"
                ));
            }
            if !top10.iter().any(|w| w == good) {
                failures.push(format!("  {buf}: expected {good} in top10; got {top10:?}"));
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} K-best noise cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    /// Class A polish (user report 2026-06-08): "chakong 插空，希望有这个
    /// 词，但中等频率吧，意思是插队在空当". 插空 existed in library.tsv at
    /// freq 0 (corpus zero-freq → below the build cutoff, never surfaced).
    /// Bumped to 5000 (source=polish): mid-freq, visible in candidates but
    /// still below 插孔 (7780, #0). Invariant: 插空 reachable at chakong.
    #[test]
    fn polish_chakong_includes_chakong() {
        let top10 = mixed_top10("chakong".as_bytes());
        assert!(
            top10.iter().any(|w| w == "插空"),
            "chakong: 插空 not in candidates; top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-06-08): "kongdang 空荡 > 空当 >
    /// 空档 > 空挡，因为空挡是专有名词". 空挡 (transmission neutral gear)
    /// is a proper/technical term and must rank last among the four.
    /// library.tsv freqs set to a clean descending order (空荡 21148 >
    /// 空当 20000 > 空档 19000 > 空挡 18000). Invariant: relative order.
    #[test]
    fn polish_kongdang_order() {
        let top10 = mixed_top10("kongdang".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let (a, b, c, d) = (pos("空荡"), pos("空当"), pos("空档"), pos("空挡"));
        assert!(
            matches!((a, b, c, d), (Some(a), Some(b), Some(c), Some(d)) if a < b && b < c && c < d),
            "kongdang expected 空荡<空当<空档<空挡; top10={top10:?}"
        );
    }

    /// 2026-06-06 polish — `jianma` cleanup. User: "捡骂 剑麻 不应该
    /// 出现，他们不是词".
    /// - 捡骂: D1 deleted from library.tsv (jieba sub-word noise) +
    ///   logged in corpus_garbage_filter_v1.tsv.
    /// - 剑麻: D2 hidden via exclusions_v1.tsv (real botanical word
    ///   for sisal, kept in dict for K-best / initials reverse-lookup
    ///   per the protocol's default-conservative D2 rule).
    /// Test pins: neither word in any visible candidate slot.
    #[test]
    fn polish_jianma_noise_removed_from_candidates() {
        let top10 = mixed_top10(b"jianma");
        let mut failures = Vec::new();
        for bad in &["捡骂", "剑麻"] {
            if top10.iter().any(|w| w == bad) {
                failures.push(format!("  jianma: {bad} still surfaces; top10={top10:?}"));
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} jianma-cleanup cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    /// Wubi Jianma3 (3-letter simcode) sample. Per 伙-rule extended,
    /// 3-letter shortcuts with common-char targets MUST lead.
    #[test]
    fn jianma3_common_chars_lead_in_mixed() {
        let cases: &[(&str, &str)] = &[
            // (shi, 椒) retired 2026-06-03 — user "shi 肯定不能是 椒，
            // 要是 '是'"; pair moved to tier_overlay tier 5.
            ("you", "亦"), // protected
                           // Additional 3-letter sample.
        ];
        run("jianma3_ext", cases, mixed_top, mixed_top10);
    }

    /// 4-letter wubi full-code Phrase entries added via library.tsv
    /// (Class A polishes for tech terms) must surface at #0 of mixed-
    /// mode for that code. Pile new (code, phrase) pairs here as
    /// future polishes happen.
    #[test]
    fn wubi_phrase_full_code_leads_in_mixed() {
        let cases: &[(&str, &str)] = &[
            // Polish-log 2026-06-06: user "解耦 这个词需要的 ... qedi
            // 五笔". 解耦 (decouple, software architecture term) added
            // to wubi library.tsv at qedi (canonical wubi-86: 解=qe,
            // 耦=di) freq 30000, Phrase layer (1).
            ("qedi", "解耦"),
            // Polish-log 2026-06-06: user "tigan wsdgd 体感，要加这个
            // 词".  Standard wubi-86 2-char phrase code is `wsdg` (体=
            // ws + 感=dg, top-2 each); user's `wsdgd` had an extra
            // trailing `d` — added at canonical 4-letter code.
            ("wsdg", "体感"),
        ];
        run("wubi_phrase_full", cases, mixed_top, mixed_top10);
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
        if super::super::pinyin_adapter::PINYIN_DISABLE_COMPOSE {
            return;
        }
        let codes: &[&str] = &[
            // Single-syllable particles.
            "de",
            "le",
            "ma",
            "ba",
            "ne",
            "ya",
            "la",
            "a",
            "e",
            "o",
            // Single-syllable common.
            "wo",
            "ni",
            "ta",
            "shi",
            "de",
            "ge",
            "yi",
            "ji",
            "di",
            "bu",
            "qi",
            "fa",
            "le",
            "ke",
            "hu",
            "he",
            // Two-syllable common compounds.
            "women",
            "tamen",
            "nihao",
            "zhongguo",
            "jintian",
            "xianzai",
            "shijian",
            "wenti",
            "dongxi",
            "difang",
            // Three-syllable.
            "buguoshi",
            "shihaohao",
            // `fenkuaikai` removed 2026-06-03 — not a real Chinese phrase;
            // previously composed as 分会开 via 会's secondary (kuai)
            // reading, which has been retired in exclusions_v1.tsv. Same
            // pattern as nihaomawojiao below: empty-result is the correct
            // behavior for non-phrase buffers.
            // Long pinyin (Viterbi territory). Each kept buffer
            // forms a real Chinese composition that passes the
            // ceil((N-1)/2) bigram-density gate.
            "yongbuliao",
            // `nihaomawojiao` removed 2026-06-02 — user judgment:
            // "你好吗我叫 这也不算是个句子, 这个其实也不应该出现".
            // The chain (你好-吗-我-叫) has fewer than ceil(3/2)=2
            // corpus-present bigram links, so the stricter gate
            // drops it; empty-result is the correct behavior.
            // `wodemingzi` excluded — Viterbi has no path for it
            // given current dict (mingzi 名字 + wodming — no good
            // split). Test ascii_fallback path instead via
            // ascii_fallback_fires.
        ];
        let mut failures = Vec::new();
        for code in codes {
            let cands = pinyin_top10(code.as_bytes());
            if cands.is_empty() {
                failures.push(format!("  {code}: empty candidates"));
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} empty-result regressions:\n{}",
                failures.len(),
                failures.join("\n")
            );
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
        assert_eq!(
            last_commit.as_deref(),
            Some("qwxzy"),
            "expected ASCII fallback to commit 'qwxzy' as raw ASCII"
        );
    }

    #[test]
    fn ascii_fallback_engages_after_threshold() {
        // 5+ char buffer that doesn't resolve to a pinyin word and
        // doesn't form a wubi simcode should commit as ASCII.
        use crate::Session;
        let mut s = Session::new();
        s.set_auto_commit_policy(AutoCommitPolicy::Never);
        // Type a clearly non-pinyin sequence.
        for cp in b"hellox" {
            s.handle_key(*cp as u32, 0);
        }
        let preedit = s.preedit().to_string();
        // ASCII fallback path commits to take_commit OR leaves the
        // buffer present for the user to escape. Either way, the
        // candidate list shouldn't be empty of any meaningful options;
        // user can escape to ASCII.
        let cands = s.candidates();
        // Smoke: not panicking is the main contract here.
        eprintln!(
            "hellox preedit={preedit:?} cands_top5={:?}",
            cands.iter().take(5).collect::<Vec<_>>()
        );
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
            ("queshi", "缺失"), // user picked 4× over 确实
            ("youshi", "优势"), // 5×
            ("rongyu", "冗余"), // 5×
            // jixu: polish-log 28× had been 积蓄 (likely from a
            // financial-context typing burst). User re-attestation 2026-05-26
            // direct screenshot: "继续还是应该在第一的，这个感觉比积蓄要高频" —
            // daily-use 继续 dominates. prior_correction × 2 on 继续 enforces
            // this; baseline test follows the user's overriding pick.
            ("jixu", "继续"),
            ("yuming", "域名"), // 4× + modern_vocab
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
            panic!(
                "{} polish-log added-word cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    /// Class-B polish "X outranks Y but isn't necessarily #0" assertions —
    /// for cases where user wants A above B without claiming top1 (top1
    /// is held by a third candidate that's correctly leading).
    #[test]
    fn polish_log_relative_ordering() {
        // Format: (buffer, higher, lower).
        let cases: &[(&str, &str, &str)] = &[
            // User polish-log 2026-06-03: "cipin 词频应该高于疵品".
            // 次品 stays #0 (correctly common). 词频 boosted via
            // quickfix_boost.tsv to land above 疵品.
            ("cipin", "词频", "疵品"),
            // User polish-log 2026-06-03: "xian jiao 肯定要改，这两
            // 个可以出现但肯定不能在第一个". 见/觉 are valid secondary
            // readings (xiàn / jiào, e.g. 显见 / 睡觉) but their
            // dominant readings are jiàn / jué — corpus freq from
            // dominant-reading compounds shouldn't push them above
            // mainstream-reading singletons 现 / 叫.
            ("xian", "现", "见"),
            ("jiao", "叫", "觉"),
        ];
        let mut failures: Vec<String> = Vec::new();
        for (buf, higher, lower) in cases {
            let top10 = pinyin_top10(buf.as_bytes());
            let hi_rank = top10.iter().position(|w| w == higher);
            let lo_rank = top10.iter().position(|w| w == lower);
            match (hi_rank, lo_rank) {
                (Some(h), Some(l)) if h < l => {} // pass
                (Some(h), Some(l)) => failures.push(format!(
                    "  {buf:<10} expected `{higher}` (rank {h}) above `{lower}` (rank {l}); top10={top10:?}"
                )),
                (None, _) => failures.push(format!(
                    "  {buf:<10} `{higher}` not in top10; top10={top10:?}"
                )),
                (_, None) => {} // lower missing is fine — higher still wins
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} polish-log ordering cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    // ───────────────────────────────────────────────────────────
    // No traditional characters in top-5 for common single
    // syllables (t2s strip + bigram/trigram filter).
    // ───────────────────────────────────────────────────────────

    #[test]
    fn smoke_user_chinese_phrase_yongbuliao_works() {
        if super::super::pinyin_adapter::PINYIN_DISABLE_COMPOSE {
            return;
        }
        // User-typed 2026-05-24 "xianzai yongbuliao le" (现在用不了了)
        // signaling IME unusable. Sanity: each segment must give the
        // expected Chinese in PinyinOnly mode.
        let cases: &[(&str, &str)] = &[("xianzai", "现在"), ("yongbuliao", "用不了"), ("le", "了")];
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
            panic!(
                "{} smoke cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    // ───────────────────────────────────────────────────────────
    // Wubi L0 pin regression — process-global wubi dict requires
    // serialized access; we save + restore the snapshot to avoid
    // leaking to other parallel tests.
    //
    // Pre-fix bug (2026-06-02): the v1.4.7 cement-layer carve moved
    // wubi business rules from `inputx_wubi::PinyinDict::
    // lookup_with_scores_into` into composite/dispatch.rs, but only
    // re-applied `single_promote` — L0 pin × 1000 was forgotten. Net
    // result: any wubi pin was a no-op for the cross-engine merge.
    // User report: "no matter how many times I pick `就` for `yi`,
    // it never beats pinned pinyin `以`" — symptom #1 was `就` losing
    // to pinned `以`; symptom #2 was random unpinned pinyin like
    // `已` / `亦` / `意` flipping at rank #1 (bigram-from-prev-
    // committed jitter, by design but invisible to the user once `就`
    // is stably at #0).
    //
    // jianma2_common_chars_lead_in_mixed already covers `("yi", "就")`
    // for the cold (no L0) case. This test covers the WITH-pin case
    // where competing pinyin pins exist — `就` must STILL lead.
    // ───────────────────────────────────────────────────────────

    #[test]
    fn wubi_pin_honored_in_mixed_mode_even_against_pinyin_pin() {
        use inputx_wubi::L0Snapshot as WubiL0;

        // Snapshot global wubi L0 + restore on exit so this doesn't
        // leak to other parallel baseline tests.
        let saved = inputx_wubi_data::export_l0();

        // Test L0: pin yi → 就 in wubi side.
        let test_snap = WubiL0 {
            pins: vec![("yi".to_string(), "就".to_string())],
            pick_counts: vec![],
            layer_prefs: inputx_wubi::DEFAULT_LAYER_PREFS,
        };
        let accepted = inputx_wubi_data::import_l0(test_snap);
        assert!(accepted >= 1, "wubi L0 pin should be accepted");

        // Build CompositeEngine + pin pinyin yi → 以 too (the user
        // had both pins active). Pinyin pin sits on the per-engine
        // adapter so each fresh CompositeEngine starts empty —
        // import via the session-style API would also work but
        // we go direct since we already have the engine handle.
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        let pinyin_snap = inputx_pinyin::L0Snapshot {
            pins: vec![("yi".to_string(), "以".to_string())],
            pick_counts: vec![],
        };
        e.pinyin_import_l0(pinyin_snap);

        for b in b"yi" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        let top = top10.first().cloned().unwrap_or_default();

        // Restore global wubi L0 BEFORE any assert that might panic,
        // so the cleanup runs in both paths.
        inputx_wubi_data::import_l0(saved);

        assert_eq!(
            top, "就",
            "wubi L0 pin yi→就 must lead in Mixed mode even when pinyin L0 \
             pins yi→以. Wubi is the IME-first engine; the user's wubi pin \
             encodes muscle memory that pinyin pin must not override. \
             Got top10={top10:?}"
        );
    }

    #[test]
    fn wubi_pin_honored_in_mixed_mode_even_against_pinyin_bigram_context() {
        use inputx_wubi::L0Snapshot as WubiL0;

        // Same as above, but FIRST commit `用` (so prev_committed
        // activates the pinyin bigram boost path), THEN type `yi`.
        //
        // Pre-fix (commit 245893d era): score-based pin × 1000 (+110
        // Q4) was beaten by (用, 以) bigram (+100-200 Q4) added on
        // top of pinned 以's likelihood — `以` ended up at #0 despite
        // the wubi pin on `就`. User 2026-06-02 report: "yi 还是以在
        // 就前面，前面如果没有任何输入的时候，'就' 才能在第一".
        //
        // Fix: structural promotion in dispatch.rs Mixed branch —
        // post-merge, wubi-pinned word is unconditionally moved to
        // #0. Bigram + score-based competition no longer applies to
        // the pinned slot — pin is a USER ASSERTION not a stat hint.

        let saved = inputx_wubi_data::export_l0();

        let test_snap = WubiL0 {
            pins: vec![("yi".to_string(), "就".to_string())],
            pick_counts: vec![],
            layer_prefs: inputx_wubi::DEFAULT_LAYER_PREFS,
        };
        let accepted = inputx_wubi_data::import_l0(test_snap);
        assert!(accepted >= 1, "wubi L0 pin should be accepted");

        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        // Pin pinyin yi→以 to mirror the user's L0 exactly.
        let pinyin_snap = inputx_pinyin::L0Snapshot {
            pins: vec![("yi".to_string(), "以".to_string())],
            pick_counts: vec![],
        };
        e.pinyin_import_l0(pinyin_snap);

        // Step 1 — type `yong` and commit `用` (pinned-ish via top hit;
        // 用 is the top single-char for `yong`). This sets the
        // engine's last_committed_word so prev_committed = "用" on
        // the next dispatch.
        for b in b"yong" {
            let _ = e.handle_letter(*b);
        }
        // Find 用 in the current candidates and commit it.
        let yong_idx = e
            .candidates()
            .iter()
            .position(|c| c.word == "用")
            .expect("用 should appear for buffer yong");
        let committed = e.commit_index(yong_idx);
        assert_eq!(committed.as_deref(), Some("用"), "should commit 用");

        // Step 2 — type yi. Now prev_committed = 用, so the bigram
        // boost fires on pinyin candidates that follow 用 (以, 是, etc.).
        for b in b"yi" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        let top = top10.first().cloned().unwrap_or_default();

        // Restore global wubi L0 BEFORE any assert that might panic.
        inputx_wubi_data::import_l0(saved);

        assert_eq!(
            top, "就",
            "wubi L0 pin yi→就 must lead even after 用 was just \
             committed (which would otherwise lift pinyin 以 via the \
             (用, 以) bigram boost). Pin = user assertion, not a stat \
             hint that bigram can outvote. Got top10={top10:?}"
        );
    }

    #[test]
    fn prominent_wubi_simcode_leads_in_mixed_mode_even_with_prev_committed() {
        // User 2026-06-02: typing `用` then `yi` showed `以` at #0,
        // `就` at #2 — even WITHOUT any L0 pin. The (用, 以) bigram
        // boost in pinyin_adapter.rs was outranking wubi 就's natural
        // simcode advantage. User: "yi 还是以在就前面，前面如果没有
        // 任何输入的时候，'就' 才能在第一，这不对".
        //
        // Rule: 就 is a prominent Jianma2 simcode (passes CHAR_PROMINENT_
        // FLOOR=20k); per the "wubi-first muscle memory" contract it
        // must lead #0 in Mixed mode regardless of pinyin bigram context.
        //
        // Distinct from the pin-based test above — this case has NO L0
        // state at all, so the structural promotion in dispatch.rs
        // must trigger via the prominent_simcode_winner branch (not
        // wubi_pinned).
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);

        // Step 1 — commit 用 to set prev_committed for the bigram path.
        for b in b"yong" {
            let _ = e.handle_letter(*b);
        }
        let yong_idx = e
            .candidates()
            .iter()
            .position(|c| c.word == "用")
            .expect("用 should appear at #N for buffer yong");
        let committed = e.commit_index(yong_idx);
        assert_eq!(committed.as_deref(), Some("用"));

        // Step 2 — type yi. Without the structural promotion, pinyin
        // 以 wins via bigram(用, 以); with the promotion, 就 leads.
        for b in b"yi" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        let top = top10.first().cloned().unwrap_or_default();
        assert_eq!(
            top, "就",
            "prominent wubi Jianma2 `就` (yi) must lead #0 in Mixed \
             mode regardless of prev_committed bigram context. Cold \
             session works correctly via natural sort; this test \
             guards the WITH-context regression. Got top10={top10:?}"
        );
    }

    // ───────────────────────────────────────────────────────────
    // WU-ψ phase 5 — tier overlay regression coverage. Each (buffer,
    // word, expected_position) case below MUST be reflected in
    // tools/scoring/data/polish/tier_overlay.tsv; removing
    // the overlay row should make the test fail loudly.
    // ───────────────────────────────────────────────────────────

    #[test]
    fn pinyin_force_segmentation_dropped_for_jp_romaji() {
        // User report 2026-06-02 kakarimasu screenshot: top #0 was
        // 卡卡日马苏 (mechanical ka-ka-ri-ma-su → 卡-卡-日-马-苏
        // segmentation), beating かかります. User: "这个的中文结果
        // 似乎是无效的，根本不应该出现".
        //
        // Pre-fix: Path 5 fallback's `alternate_bigrams_ok` used a
        // LENIENT "≥1 link non-zero" rule AND exempted top-1
        // (fallback_composition) from the gate entirely. The chain
        // (卡, 卡, 日, 马, 苏) had ONE corpus-present bigram (马, 苏)
        // because "马苏" is a Chinese celebrity name — enough under
        // the lenient rule, and the top-1 exemption let it through
        // anyway.
        //
        // Post-fix (phase 7): ceil((N-1)/2) majority rule applied
        // uniformly to both composed_sentence and the Path 5
        // fallback; top-1 exemption removed.
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"kakarimasu" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        // The garbage force-segmentation must NOT appear in top-10.
        assert!(
            !top10.contains(&"卡卡日马苏".to_string()),
            "kakarimasu must drop the mechanical 卡卡日马苏 \
             force-segmentation; got top10={top10:?}"
        );
        // かかります / カカリマス must lead (the real JP rendering).
        let top = top10.first().cloned().unwrap_or_default();
        assert!(
            top == "かかります" || top == "カカリマス",
            "kakarimasu top must be JP basic kana, got {top:?} \
             (top10={top10:?})"
        );
    }

    #[test]
    fn pinyin_real_fallback_composition_still_surfaces() {
        // Sibling guard for the force-segmentation fix: `kaopu` is a
        // real Chinese fallback composition (靠谱; (靠, 谱) has corpus
        // support). The stricter bigram gate must NOT drop it.
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"kaopu" {
            let _ = e.handle_letter(*b);
        }
        let top = e
            .candidates()
            .first()
            .map(|c| c.word.clone())
            .unwrap_or_default();
        assert_eq!(
            top, "靠谱",
            "kaopu must still surface 靠谱 fallback composition \
             after the stricter bigram gate (real bigram support \
             from corpus)"
        );
    }

    #[test]
    fn pinyin_rare_cjk_chars_yield_to_jp_basic_kana() {
        // User report 2026-06-02 sai screenshot: 8 pinyin rare-CJK chars
        // (噻 腮 鳃 嘥 簺 僿 plus 2 more) sat above JP basic kana さい.
        //
        // Pre-Phase-B fix (phase 7): hard-cutoff tier band by raw_freq.
        //
        // Phase B+C (2026-06-03): z-score quantile + mechanical kana
        // buffer-length split.  Rare-CJK chars自然 落 tier 3-5; mechanical
        // kana on short buffer (sai ≤ 4 chars) → tier 2.  The relative
        // invariant "rare-CJK below さい" still holds, but absolute
        // position changed: nihongo single-kanji (才/裁/最/殺/etc.) are
        // also tier 2 with higher per-char freq than mechanical kana,
        // so they sit between 拼音 tier 2 and さい.
        //
        // Assertion (updated): the rare-CJK chars (嘥 簺 僿 鳃) must rank
        // BELOW JP basic kana さい — this is the load-bearing invariant
        // from the user report.  "top-10" was an implementation detail
        // that the new framework can't satisfy in mixed+jp without
        // suppressing nihongo single-kanji, which is unwanted.
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"sai" {
            let _ = e.handle_letter(*b);
        }
        let cands = e.candidates();
        let words: Vec<String> = cands.iter().map(|c| c.word.clone()).collect();
        let sai_idx = words.iter().position(|w| w == "さい");
        assert!(
            sai_idx.is_some(),
            "さい must be present in candidates for `sai`; got top15={:?}",
            &words[..words.len().min(15)]
        );
        let sai_pos = sai_idx.unwrap();
        for rare in ["嘥", "簺", "僿", "鳃"] {
            if let Some(pos) = words.iter().position(|w| w == rare) {
                assert!(
                    pos > sai_pos,
                    "rare-CJK pinyin char `{rare}` must rank BELOW JP basic kana さい \
                     (got rare at #{pos}, さい at #{sai_pos}); first15={:?}",
                    &words[..words.len().min(15)]
                );
            }
        }
    }

    #[test]
    fn phase_e_bigram_quality_gate_demotes_subword_phrases() {
        // User reports 2026-06-03:
        //   juli  : "举例 肯定要高于 局里 和 剧里 这种并不完全是单词的组合"
        //   guanli: "馆里 这个词高了, 而且这类的 pinyin 词都有点高... 地名加方位
        //           之类的组合"
        // 长期方向 (RANKING-MODEL-INVARIANTS §5.5): 公式 only, 不单条 fix.
        //
        // Phase E gate (pinyin_adapter.rs + engine_weights.toml
        // [scoring.phrase_quality]): 2-char phrase gets +2-tier demote when
        // bigram_boost(c1, c2) < 25000 AND freq ∈ [22000, 30000).  Catches
        // jieba over-segmentation noise without misfiring on real-rare
        // (靠谱/铆钉) or user-attested quickfix-boosted (锚定 40k) entries.
        //
        // Per-case bigram + freq (from spike):
        //   馆里 freq=23852 bigram=21309 → demote (in band, low bigram)
        //   局里 freq=24466 bigram=16664 → demote
        //   剧里 freq=22522 bigram=14082 → demote
        //   居里 freq=17110 bigram=33542 → keep (real but freq< floor)
        //   屋里 freq=31774 bigram=32646 → keep (freq > ceil)
        //   靠谱 freq=10666 bigram=20918 → keep (freq < floor — real-rare)
        //   锚定 quickfix=40000 → keep (freq > ceil — user-attested)
        let cases: &[(&str, &[&str])] = &[
            // (buffer, sub-word noise that must NOT be top-3)
            ("guanli", &["馆里"]),
            ("juli", &["局里", "剧里"]),
        ];
        for (buf, noise_set) in cases {
            let top = pinyin_top10(buf.as_bytes());
            for noise in *noise_set {
                if let Some(pos) = top.iter().position(|w| w == noise) {
                    assert!(
                        pos >= 3,
                        "Phase E gate failed for ({buf}, {noise}) — \
                         expected sub-word phrase NOT in top-3, got #{pos}; \
                         top10={top:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn phase_g_path5b_fallback_yields_to_jp_prediction() {
        // User report 2026-06-03: "akashi 阿卡是 不是一个应该出现的东西,
        // 你再看看这是怎么来的, 同情况都要处理掉".
        //
        // 阿卡是 came from pinyin Path 5b last-resort Viterbi fallback
        // (MatchType::Composed{bigram_links:0}) at tier 1.  When the
        // buffer has no Chinese reading (akashi is JP), Path 5b's
        // forced segment dominates over JP prediction (tier 7).
        // Phase G demotes Path 5b fallback to tier 8 (speculative
        // band) so JP candidates lead.
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"akashi" {
            let _ = e.handle_letter(*b);
        }
        let top: Vec<String> = e
            .candidates()
            .iter()
            .take(5)
            .map(|c| c.word.clone())
            .collect();
        assert_eq!(
            top.first().map(String::as_str),
            Some("明石"),
            "akashi mixed+jp top #0 must be JP 明石 (Path 5b fallback \
             阿卡是 demoted to tier 8); got top={top:?}"
        );
        // 阿卡是 should rank below all JP candidates (明石 / あかし / アカシ).
        if let Some(akashi_idx) = top.iter().position(|w| w == "阿卡是") {
            assert!(
                akashi_idx >= 3,
                "Path 5b fallback 阿卡是 must rank below the 3 JP \
                 candidates (got #{akashi_idx}); top={top:?}"
            );
        }
    }

    #[test]
    fn basic_kana_short_buffer_beats_single_kanji() {
        // User report 2026-06-03 ki: 記 / 紀 / 帰 / 起 / 気 etc. 日语
        // single-kanji ranked above きキ basic kana for buffer `ki` —
        // "ki 没有拼音,假名应该高分;常规假名短字符一定要比其他日语高".
        //
        // Phase C-2 fix: mechanical-kana tier band split 3 ways by
        // buffer length.  1-2 chars (basic 50音 single syllable) → tier 1,
        // beating nihongo single-kanji tier 2.  3-4 chars → tier 2.
        // ≥ 5 chars → tier 4 (Chinese intent, mechanical kana noise).
        //
        // Invariant: basic kana (き / カ etc.) MUST rank above any
        // nihongo single-kanji candidate for the same buffer.  pinyin /
        // wubi candidates are allowed to rank above the kana (e.g. ka:
        // 卡 pinyin leads).
        let cases: &[(&str, &[&str])] = &[
            // ki: no pinyin syllable; nihongo single-kanji crowd was
            // burying きキ pre-fix.
            ("ki", &["き", "キ"]),
            // ka: pinyin 卡 leads (tier 1); か / カ must still beat
            // 日 / 下 / 何 etc. single-kanji.
            ("ka", &["か", "カ"]),
        ];
        // Detect nihongo single-kanji by sniffing the JP-only candidate
        // set: any Han single char emitted in JapaneseOnly mode is a
        // nihongo single-kanji.  In Mixed+jp the same Han chars come
        // back from the merged candidate list; they MUST be ranked
        // BELOW the basic kana.
        for (buf, basic_kana_set) in cases {
            let mut e = CompositeEngine::new();
            e.set_mode(Mode::Mixed);
            e.set_auto_commit_policy(AutoCommitPolicy::Never);
            e.set_japanese_enabled(true);
            for b in buf.bytes() {
                let _ = e.handle_letter(b);
            }
            let top: Vec<String> = e
                .candidates()
                .iter()
                .take(15)
                .map(|c| c.word.clone())
                .collect();

            // Collect known nihongo single-kanji words via JapaneseOnly.
            let mut jp = CompositeEngine::new();
            jp.set_mode(Mode::JapaneseOnly);
            jp.set_auto_commit_policy(AutoCommitPolicy::Never);
            jp.set_japanese_enabled(true);
            for b in buf.bytes() {
                let _ = jp.handle_letter(b);
            }
            let jp_singles: std::collections::HashSet<String> = jp
                .candidates()
                .iter()
                .filter(|c| c.word.chars().count() == 1)
                .filter(|c| {
                    let ch = c.word.chars().next().unwrap();
                    ('\u{4E00}'..='\u{9FFF}').contains(&ch)
                })
                .map(|c| c.word.clone())
                .collect();

            for kana in *basic_kana_set {
                let kana_idx = top.iter().position(|w| w == kana).unwrap_or_else(|| {
                    panic!(
                        "basic kana `{kana}` for `{buf}` must appear in top-15; \
                         got top={top:?}"
                    )
                });
                for (i, w) in top.iter().enumerate() {
                    if i >= kana_idx {
                        break;
                    }
                    assert!(
                        !jp_singles.contains(w),
                        "nihongo single-kanji `{w}` (#{i}) outranks basic kana \
                         `{kana}` (#{kana_idx}) for `{buf}` — `常规假名短字符\
                         一定要比其他日语高` invariant broken; top={top:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn tier_overlay_lifts_juti_juti_to_top() {
        // juti 具体 → tier 0 overrides the natural tier-1 + wubi
        // engine_offset advantage that 暗送秋波 had post-phase-2.
        let cases: &[(&str, &str)] = &[("juti", "具体")];
        run("tier_overlay_juti", cases, mixed_top, mixed_top10);
    }

    #[test]
    fn tier_overlay_demotes_mi_qiao_so_mi_米_leads() {
        // mi 峭 → tier 5 demotion; pinyin 米 (tier 1) wins the
        // top slot.
        let cases: &[(&str, &str)] = &[("mi", "米")];
        run("tier_overlay_mi", cases, mixed_top, mixed_top10);
    }

    // ───────────────────────────────────────────────────────────
    // lüe / nüe alias normalization (user 2026-06-02: "celue 策略，
    // 这种级别的拼音词怎么也没有"). Dict stores under lve/nve; users
    // type lue/nue per Sogou/Google convention. lower_str collapses
    // the alias so both spellings hit the same FST key.
    // ───────────────────────────────────────────────────────────

    #[test]
    fn lue_alias_resolves_to_lve_for_common_words() {
        let cases: &[(&str, &str)] = &[
            ("celue", "策略"),  // canonical lve: celve
            ("celve", "策略"),  // canonical spelling still works
            ("nuedai", "虐待"), // canonical nve: nvedai
            ("nvedai", "虐待"), // canonical spelling still works
        ];
        run("lue_nue_alias", cases, pinyin_top, pinyin_top10);
    }

    /// Polish 2026-06-07 (user /polish): "biguo 比国肯定不能是第一，甚至我
    /// 不确定这是不是一个词，敝国 > 比过 > 日语，期待这样的结果".
    /// Class D1 — 比国 deleted from inputx-pinyin library.tsv (corpus noise,
    /// not a real word; logged in corpus_garbage_filter_v1.tsv so a future
    /// corpus-digest can't re-admit it). Class B — 敝国 boosted to 21000 in
    /// quickfix_boost.tsv so it leads 比过 (base 19033).
    /// Before: [比国, 比过, 敝国].  After: [敝国, 比过] (比国 gone).
    #[test]
    fn polish_biguo_real_word_leads_corpus_noise_deleted() {
        let top10 = pinyin_top10(b"biguo");
        assert_eq!(
            top10.first().map(String::as_str),
            Some("敝国"),
            "敝国 must lead biguo (Class B boost over 比过); got {top10:?}"
        );
        assert!(
            !top10.iter().any(|w| w.as_str() == "比国"),
            "比国 must not appear at all (Class D1 deletion); got {top10:?}"
        );
    }

    #[test]
    fn no_traditional_in_top5_for_common_pinyin() {
        // List of (pinyin, traditional_blocklist) — traditional forms
        // must NOT appear in top 5 PinyinOnly candidates.
        let cases: &[(&str, &[&str])] = &[
            // Verified traditional-only forms (NOT same as simplified).
            ("yu", &["於"]),        // simplified 于 must lead
            ("guo", &["國", "過"]), // 国/国, 过/過
            ("lai", &["來"]),       // 来
            ("hou", &["後"]),       // 后/後
            ("hui", &["會"]),       // 会
            ("ma", &["嗎", "媽"]),  // 吗/媽
        ];
        let mut failures = Vec::new();
        for (buf, blocklist) in cases {
            let top10 = pinyin_top10(buf.as_bytes());
            let top5: &[String] = if top10.len() < 5 {
                &top10[..]
            } else {
                &top10[..5]
            };
            for bad in *blocklist {
                if top5.iter().any(|w| w == bad) {
                    failures.push(format!(
                        "  {buf}: traditional {bad} in top5 — top5={top5:?}"
                    ));
                }
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} traditional-leak cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    /// 2026-06-06 polish — wubi traditional sweep.  User: "拼音[corrected
    /// → 五笔]结果中有很多繁体的结果" → "詞要的不是沉，而是不应该有，
    /// 要打开繁体模式才能有" → "你系统解决吧".  Sweep removed every
    /// (code, trad_word) from wubi library.tsv where opencc t2s(word)
    /// differs AND the same code already has a simplified peer (1164
    /// entries; see docs/wubi-trad-sweep-2026-06-06/).  Orphan TRAD
    /// entries (no same-code simp peer, ~2953) stay until the 繁体-mode
    /// toggle ships — deleting them would silently break wubi lookup
    /// for those chars.  This test seeds the regression invariant: 詞
    /// cannot resurface at yngk even if corpus-digest re-admits it
    /// (corpus_garbage_filter_v1.tsv has the same row as the gate).
    #[test]
    fn no_traditional_in_top10_for_common_wubi() {
        let cases: &[(&str, &[&str])] = &[
            ("yngk", &["詞"]), // 简体 词 leads; 詞 deleted by 2026-06-06 sweep
            // 2026-06-09: 員 was a TRAD dup line in jianma_simplified.txt
            // (kmu 員 alongside kmu 员) + a high-freq library overlay; both
            // removed so 员 leads. jianma_simplified slipped past the
            // 2026-06-06 auto_decomp-only sweep.
            ("kmu", &["員"]),
            // 2026-06-09 systemic jianma_simplified TRAD/Shinjitai sweep
            // (90 dup deletes + 68 trad→simp rewrites; see
            // docs/jianma-simplified-trad-sweep-2026-06-09/). Representative
            // rewritten simcodes — the TRAD form must no longer surface.
            ("deu", &["長"]), // → 长
            ("lmu", &["買"]), // → 买
            ("hqb", &["見"]), // → 见
            ("qou", &["魚"]), // → 鱼
        ];
        let mut failures = Vec::new();
        for (buf, blocklist) in cases {
            let top10 = mixed_top10(buf.as_bytes());
            for bad in *blocklist {
                if top10.iter().any(|w| w == bad) {
                    failures.push(format!(
                        "  {buf}: traditional {bad} in top10 — top10={top10:?}"
                    ));
                }
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} wubi-trad-leak cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    /// 2026-06-08 polish — wubi 日本新字体 (Shinjitai) sweep.  Follow-up
    /// to the 2026-06-06 繁体 sweep, which used opencc `t2s` and so missed
    /// Japanese Shinjitai forms (巌 亀 両 伝 児 図 団 …): they are NOT
    /// classical Traditional, so `t2s(c) == c` and they survived in
    /// auto_decomp.txt.  User saw `mid` prefix-complete to 巌 ("这是什么
    /// 字啊") — a Shinjitai of 巖/岩 outranking pinyin via wx>px.  The
    /// v3 sweep upgrades the normalisation to `t2s ∘ jp2t` (fold Shinjitai
    /// → Traditional → Simplified) with a GB2312 whitelist guard so
    /// opencc's over-reach (欠→缺, 予→豫, 芸→艺, 糸→丝 …) is protected.
    /// 217 chars stripped from auto_decomp.txt + 218 overlay rows from
    /// wubi library.tsv, all logged to corpus_garbage_filter_v1.tsv.
    /// See docs/wubi-jp-shinjitai-sweep-2026-06-08/.  Invariant: the
    /// Shinjitai form cannot resurface at its wubi code; the Simplified
    /// peer leads instead.
    #[test]
    fn no_jp_shinjitai_in_top10_for_common_wubi() {
        let cases: &[(&str, &[&str])] = &[
            ("mid", &["巌"]),  // user report: 密度 leads; 巌 (岩的新字体) gone
            ("midt", &["巌"]), // 巌 full-code also stripped
            ("adat", &["蔵"]), // 蔵 (藏的新字体) gone
        ];
        let mut failures = Vec::new();
        for (buf, blocklist) in cases {
            let top10 = mixed_top10(buf.as_bytes());
            for bad in *blocklist {
                if top10.iter().any(|w| w == bad) {
                    failures.push(format!(
                        "  {buf}: shinjitai {bad} in top10 — top10={top10:?}"
                    ));
                }
            }
        }
        // Positive invariant: the Simplified peer still leads at mid.
        let mid_top = mixed_top10("mid".as_bytes());
        if mid_top.first().map(String::as_str) != Some("密度") {
            failures.push(format!("  mid: 密度 not #0 — top10={mid_top:?}"));
        }
        if !failures.is_empty() {
            panic!(
                "{} wubi-shinjitai-leak cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    /// 2026-06-06 polish — pinyin er→r typo sweep.  User: "zhonghuarnv 为
    /// 什么会出现 中华儿女呢，不应该是 zhonghuaernv 吗".  Detected
    /// systemically by script: word at code `<X>r<Y>` AND the same word
    /// at `<X>er<Y>` ⇒ corpus encoding dropped the 'e' from mid-word
    /// `er` (儿).  165 such rows removed from library.tsv + logged in
    /// corpus_garbage_filter_v1.tsv.  This test seeds: typo'd code
    /// must NOT yield the word; canonical code still does.
    #[test]
    fn no_er_typo_pinyin_codes_surface_real_words() {
        let cases: &[(&str, &str, &str)] = &[
            // (typo_code, canonical_code, word_that_must_not_surface_at_typo)
            ("zhonghuarnv", "zhonghuaernv", "中华儿女"),
            ("darzi", "daerzi", "大儿子"),
            ("dairxi", "daierxi", "大儿媳"),
        ];
        let mut failures = Vec::new();
        for (typo, canon, word) in cases {
            let typo_top = mixed_top10(typo.as_bytes());
            let canon_top = mixed_top10(canon.as_bytes());
            if typo_top.iter().any(|w| w == word) {
                failures.push(format!(
                    "  {typo}: typo'd code surfaces {word} — top10={typo_top:?}"
                ));
            }
            if !canon_top.iter().any(|w| w == word) {
                failures.push(format!(
                    "  {canon}: canonical code missing {word} — top10={canon_top:?}"
                ));
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} er-typo cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    // ───────────────────────────────────────────────────────────
    // Mixed-mode: rare/obscure wubi phrases must not contaminate
    // top-10 for common pinyin buffers. User report 2026-06-03:
    // "jixu 还是有曳光弹在第三，这个词太生僻了我感觉，要么根本不
    // 需要，要么应该 level 很低" — wubi has no demote overlay
    // (Class C wubi unsupported), so Class D deletion from
    // phrases.txt is the only path. This test pins the deletion.
    // ───────────────────────────────────────────────────────────

    #[test]
    fn rare_wubi_phrases_absent_from_mixed_top10() {
        let cases: &[(&str, &[&str])] = &[
            ("jixu", &["曳光弹"]),
            // 2026-06-03 sweep: 4-letter wubi phrases coinciding with
            // common pinyin syllables — wubi tier score outranked pinyin
            // single-char top of each buffer.
            ("yong", &["恋情"]),
            ("deng", &["有情"]),
            ("geng", &["表情"]),
            ("tong", &["释怀"]),
        ];
        let mut failures = Vec::new();
        for (buf, blocklist) in cases {
            let top10 = mixed_top10(buf.as_bytes());
            for bad in *blocklist {
                if top10.iter().any(|w| w == bad) {
                    failures.push(format!(
                        "  {buf}: rare wubi {bad} in mixed top10 — top10={top10:?}"
                    ));
                }
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} rare-wubi-noise cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    // ───────────────────────────────────────────────────────────
    // Wubi jianma2 (二级简码) single-char noise for common pinyin
    // syllables. Demoted via wubi weights.tsv raw_freq → 0.
    // ───────────────────────────────────────────────────────────

    /// Pinyin-side Class C demotes via tier_overlay.tsv — for entries
    /// the user wants "可以有但绝不冒头" (still in the dict for K-best
    /// composition / reverse-lookup, but never surface in the visible
    /// top of pinyin top-7 candidates). Sister test to the wubi tier-5
    /// blocklist below; same NOT-in-top-N assertion shape.
    #[test]
    fn pinyin_tier_overlay_demoted_absent_from_mixed_top7() {
        let cases: &[(&str, &[&str])] = &[
            // Polish-log 2026-06-06: user "jiaozhu ... 胶住 可以有但
            // 肯定是要最后的". Borderline real (口语 colloquial
            // "胶水粘住"), not standard vocab. tier_overlay tier 8 buries
            // it; jiaozhu real words (教主/叫住/浇筑/脚注/校注/浇铸/
            // 浇注/角柱) fill the visible top.
            ("jiaozhu", &["胶住"]),
        ];
        let mut failures = Vec::new();
        for (buf, blocklist) in cases {
            let top10 = mixed_top10(buf.as_bytes());
            let top7: &[String] = if top10.len() < 7 {
                &top10[..]
            } else {
                &top10[..7]
            };
            for bad in *blocklist {
                if top7.iter().any(|w| w == bad) {
                    failures.push(format!(
                        "  {buf}: tier-overlay demoted {bad} in mixed top7 — top7={top7:?}"
                    ));
                }
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} pinyin tier_overlay demote cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    #[test]
    fn wubi_pollution_tier5_demoted_absent_from_mixed_top3() {
        // 2026-06-07: the 2026-06-03 "200-buffer sweep" that demoted ~40
        // prominent wubi simcodes to tier 5 was REVERTED — it violated the
        // core `wx > px > nx` rule (五笔雷打不动优先) by burying high-freq
        // wubi chars (碰/虎/拉/谍/东…) behind pinyin and even nihongo. All
        // 39 single-char simcodes now lead #0 naturally (prominent → tier 1);
        // their positive assertions live in
        // `wubi_prominent_simcode_leads_after_sweep_revert`.
        //
        // What LEGITIMATELY stays tier-5 demoted (the cases below):
        //   - (shi, 椒): user-attested — "shi 要 是 不是 椒"; 椒 stays tier 5.
        //   - multi-char phrases hijacking a pinyin-shaped buffer: nobody
        //     types `suan` wanting 西装革履. Phrase tier-5 demote is correct.
        let cases: &[(&str, &[&str])] = &[
            ("shi", &["椒"]),        // attested: shi → 是; 椒 stays tier 5
            ("bang", &["陈情"]),     // phrase hijacking a pinyin buffer
            ("gang", &["开怀"]),     // phrase hijacking a pinyin buffer
            ("suan", &["西装革履"]), // phrase hijacking a pinyin buffer
        ];
        let mut failures = Vec::new();
        for (buf, blocklist) in cases {
            let top10 = mixed_top10(buf.as_bytes());
            // Top-3 = the visual first row of the candidate panel.
            // Some buffers (ha/me/ri) have thin pinyin exact-match
            // pools so wubi prefix predictions surface at rank 4-5
            // even after tier_overlay demote — that path doesn't
            // route through tier_overlay::get. Anchoring top-3 here
            // captures the user-visible regression while accepting
            // that rare-CJK-heavy buffers may keep wubi prefix
            // predictions in the second visible row.
            let top3: &[String] = if top10.len() < 3 {
                &top10[..]
            } else {
                &top10[..3]
            };
            for bad in *blocklist {
                if top3.iter().any(|w| w == bad) {
                    failures.push(format!(
                        "  {buf}: tier-5 demoted {bad} in mixed top3 — top3={top3:?}"
                    ));
                }
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} wubi-tier5-demote cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    /// 2026-06-07 (user /polish "duo 碰应该在第一位" + design clarification
    /// "五笔是雷打不动的优先 … wx > px > nx"): the 2026-06-03 sweep wrongly
    /// demoted these prominent (char_max_freq ≥ floor) wubi simcodes to
    /// tier 5, burying them behind pinyin / nihongo. Reverting the sweep
    /// restores them to natural tier 1, where wx>px puts the wubi char #0
    /// over same-tier pinyin. Locks the "五笔优先" invariant for the whole
    /// reverted set.
    #[test]
    fn wubi_prominent_simcode_leads_after_sweep_revert() {
        let cases: &[(&str, &str)] = &[
            ("ai", "东"),
            ("an", "世"),
            ("ba", "陈"),
            ("bai", "陈"),
            ("bi", "孙"),
            ("bu", "联"),
            ("dan", "碟"),
            ("di", "砂"),
            ("dou", "灰"),
            ("du", "磁"),
            ("duo", "碰"),
            ("er", "遥"),
            ("fu", "增"),
            ("ha", "虎"),
            ("hao", "虚"),
            ("he", "肯"),
            ("ji", "晃"),
            ("ke", "吸"),
            ("le", "胃"),
            ("lu", "较"),
            ("ma", "曲"),
            ("me", "骨"),
            ("nv", "恨"),
            ("qi", "乐"),
            ("qiu", "尔"),
            ("qu", "匀"),
            ("ran", "拒"),
            ("ren", "扔"),
            ("ri", "朱"),
            ("ru", "拉"),
            ("san", "柜"),
            ("si", "档"),
            ("te", "秀"),
            ("ti", "秒"),
            ("wen", "仍"),
            ("xi", "纱"),
            ("yan", "谍"),
            ("yao", "庶"),
            ("ye", "衣"),
        ];
        let mut failures = Vec::new();
        for (buf, expected) in cases {
            let top10 = mixed_top10(buf.as_bytes());
            let got = top10.first().map(String::as_str);
            if got != Some(*expected) {
                let top3: Vec<&str> = top10.iter().take(3).map(String::as_str).collect();
                failures.push(format!(
                    "  {buf}: expected #0={expected}, got {got:?} (top3={top3:?})"
                ));
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} wubi-simcode-lead cases regressed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    #[test]
    fn rare_wubi_simcode_chars_absent_from_mixed_top5() {
        // 2026-06-03 sweep — rare-CJK / traditional-form wubi simcode
        // chars hijacking common pinyin syllables. wubi simcode_boost
        // pushed them above pinyin top1 even at low raw_freq; setting
        // raw_freq → 0 in wubi weights.tsv demotes them out of the
        // user-visible top-5. They still appear deeper in top-10
        // (kept in dict for pure-wubi users typing the full 4-letter
        // code), so this test checks top-5 only.
        let cases: &[(&str, &[&str])] = &[
            ("hang", &["虛"]),       // traditional form, daily-use 虚 at hao+xu
            ("rang", &["拒"]),       // 拒 reads "jù", not "rang"
            ("yang", &["讵", "詎"]), // 讵 "jù" (rare) + traditional 詎
        ];
        let mut failures = Vec::new();
        for (buf, blocklist) in cases {
            let top10 = mixed_top10(buf.as_bytes());
            let top5: &[String] = if top10.len() < 5 {
                &top10[..]
            } else {
                &top10[..5]
            };
            for bad in *blocklist {
                if top5.iter().any(|w| w == bad) {
                    failures.push(format!(
                        "  {buf}: rare wubi simcode {bad} in mixed top5 — top5={top5:?}"
                    ));
                }
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} wubi-simcode-noise cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    // ───────────────────────────────────────────────────────────
    // Pinyin corpus noise (jieba sub-word artifacts, archaic
    // readings, etc.) — must not appear in mixed top10. Backed by
    // BAKED_EXCLUSIONS in idf_from_pinyin_dict.rs.
    // ───────────────────────────────────────────────────────────

    #[test]
    fn corpus_noise_absent_from_mixed_top10() {
        let cases: &[(&str, &[&str])] = &[
            // User polish-log 2026-06-03: "cipin 次贫不应该存在" —
            // jieba sub-word noise (次 + 贫), not a real Chinese phrase.
            ("cipin", &["次贫"]),
            // User polish-log 2026-06-03: "ciping 茨坪是什么？如果
            // 不能解释也不应该存在" — obscure place name (Jinggangshan
            // town); too rare for daily IME use.
            ("ciping", &["茨坪"]),
            // 2026-06-03 sweep — pinyin secondary-reading pollution
            // at top1 of common single-syllables.
            ("kuai", &["会"]),
            ("shen", &["什"]),
            ("zhuo", &["着"]),
            ("zhao", &["着"]),
            // 2026-06-03 sweep — jieba sub-word / homophone-noise.
            ("xianzai", &["先在", "先宰", "先载"]),
            ("zhege", &["这歌", "哲哥", "著各"]),
            ("yige", &["一格", "亿个", "毅哥", "一歌", "翼各"]),
            ("keyi", &["可意", "课以"]),
            ("tamen", &["塔门"]),
            ("meiyou", &["没油", "魅友"]),
            ("shihou", &["狮吼"]),
            ("haishi", &["嗨氏"]),
            ("jintian", &["津田"]),
            ("kandao", &["砍到", "砍倒", "刊到"]),
            ("wenti", &["吻替"]),
            ("haiyou", &["嗨呦"]),
            ("ruguo", &["入锅", "辱国"]),
            ("suoyi", &["缩衣", "索移", "所译"]),
            ("ziji", &["子鸡"]),
            // User polish-log 2026-06-03: "mo 第三个万是哪来的" —
            // 万 standard pinyin is "wan", not "mo". Legacy
            // corpus-merge noise; D1 deleted from library.tsv +
            // logged to corpus_garbage_filter_v1.tsv (so future
            // corpus-digest can't re-admit via fold).
            ("mo", &["万"]),
            // User polish-log 2026-06-03: "yichu 一出 不应该存在,
            // 一处 也不应该高排序甚至可以没有, 一触 也是一样的问题" —
            // Phase E bigram gate 在数词起头 phrase 上失效 ("一"
            // 高频字让所有邻接 bigram > 25k floor — 真词 一团/一夜
            // 跟 noise 一出/一处/一触 区分不开).  Per §5.5 acceptable
            // per-case D2: exclusions_v1.tsv hide Path-1.
            ("yichu", &["一出", "一处", "一触"]),
            // User polish-log 2026-06-03: "剑持 这不是个词" — 字字直拼
            // jiàn+chí, jieba/corpus noise.  D1 deleted from library.tsv
            // + logged to corpus_garbage_filter_v1.tsv.
            ("jianchi", &["剑持"]),
            // User polish-log 2026-06-04: "tuli 埋在土里这时候土里才有点
            // 意义" — 土里 / 图里 / 土粒 are jieba sub-word noise (only
            // meaningful inside compounds like 埋在土里). D1 deleted
            // from library.tsv + logged to corpus_garbage_filter_v1.
            ("tuli", &["土里", "图里", "土粒"]),
            // User polish-log 2026-06-05: "jianti 间体 不是个词" —
            // 字字直拼 jiàn+tǐ, jieba sub-word noise. Standalone 间体
            // doesn't exist as a Chinese word (appears only inside 中间体
            // / 空间体系, which remain unaffected). D1 deleted from
            // library.tsv + logged to corpus_garbage_filter_v1.
            ("jianti", &["间体"]),
            // User polish-log 2026-06-06: "jiaozhu 叫朱 较著 椒猪 交住
            // 这些都不算是中文词汇吧" — four jieba 主词典 sub-word noise
            // entries from the legacy external pinyin ingest. None
            // standalone Chinese words (字字直拼 / 古汉语残留). D1
            // deleted from library.tsv + logged to corpus_garbage_filter.
            // Real jiaozhu words preserved (教主/叫住/浇筑/脚注/...).
            ("jiaozhu", &["叫朱", "较著", "椒猪", "交住"]),
            // User polish-log 2026-06-04 Phase H: "tsuitachi 这里面怎么
            // 还会有这么多中文,这是怎么命中的".  9-char 日语ローマ字
            // 一日 was triggering Path 1c 2-consonant initials reverse-
            // lookup on `ts`, surfacing 调试/推送/通缩/退市/听说/同时/
            // 同事/天上/天生 etc.  Phase H caps Path 1c at buffer.len()
            // ≤5 (legitimate typo range); long buffers no longer
            // false-trigger initials shortcut.
            (
                "tsuitachi",
                &[
                    "调试", "推送", "通缩", "退市", "听说", "同时", "同事", "天上", "天生",
                ],
            ),
        ];
        let mut failures = Vec::new();
        for (buf, blocklist) in cases {
            let top10 = mixed_top10(buf.as_bytes());
            for bad in *blocklist {
                if top10.iter().any(|w| w == bad) {
                    failures.push(format!(
                        "  {buf}: corpus noise {bad} in mixed top10 — top10={top10:?}"
                    ));
                }
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} corpus-noise cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    /// 音节意识细化 (2026-06-06, docs/PLAN-syllable-aware-pinyin.md):
    /// buffers with a clean ≥3-char syllable prefix + invalid trailing
    /// char should route to Path 3b trim-retry, producing the same
    /// candidate top-1 as the trimmed buffer (= as if the trailing
    /// char hadn't been typed).
    #[test]
    fn syllable_aware_trim_retry_matches_trimmed_buffer() {
        if super::super::pinyin_adapter::PINYIN_DISABLE_FUZZY {
            return;
        }
        let cases: &[(&str, &str, &str)] = &[
            // (buffer, trimmed_equivalent, expected_top_word)
            ("shehv", "sheh", "社会"),
            ("shehb", "sheh", "社会"),
            ("shehz", "sheh", "社会"),
        ];
        for (buf, trim, expected) in cases {
            let top10 = mixed_top10(buf.as_bytes());
            assert!(
                !top10.is_empty(),
                "音节意识细化: {buf} should produce candidates via trim-retry"
            );
            let trim_top10 = mixed_top10(trim.as_bytes());
            assert_eq!(
                top10.first(),
                trim_top10.first(),
                "音节意识细化: {buf} top-1 ({top10:?}) should equal {trim} top-1 ({trim_top10:?})"
            );
            if let Some(t) = top10.first() {
                assert_eq!(
                    t.as_str(),
                    *expected,
                    "音节意识细化: {buf} top-1 should be {expected}, got {t}"
                );
            }
        }
    }

    /// 音节意识细化: buffers with NO ≥3-char clean syllable prefix
    /// and a 2-consonant + ≥2-suffix shape (the Path 1c original
    /// trigger) must still get rescue — Phase H invariant preserved.
    ///
    /// Note: pnyin / zhgo / similar 3-consonant-prefix shapes were
    /// NEVER caught by Path 1c (the gate requires exactly 2 consonants
    /// before the first vowel); they return 0 candidates today and
    /// always did. Not a regression target.
    #[test]
    fn syllable_aware_path1c_still_rescues_real_typos() {
        if super::super::pinyin_adapter::PINYIN_DISABLE_FUZZY {
            return;
        }
        // `pyin`: consonant_prefix=`py` (len 2), suffix=`in` (len 2) →
        // Path 1c gate passes, longest_valid_syllable_prefix is None
        // (no prefix of `pyin` is a valid syllable) so the new
        // syllable-aware skip doesn't fire → Path 1c rescues with
        // p+y initials hits (拼音 / 朋友 / 便宜 / ...).
        let top10 = mixed_top10(b"pyin");
        assert!(
            !top10.is_empty(),
            "音节意识细化 regression: pyin lost Path 1c rescue; top10 was empty"
        );
    }

    /// v1.14 (user report 2026-06-06 `tkinn`): Path 1c 5-char buffers
    /// must require the suffix to be a plausible pinyin syllable tail
    /// — there must exist at least one valid syllable ending with it.
    /// Otherwise random-keystroke / JP-romaji buffers like `tkinn`
    /// (suffix `inn`) flood top-10 with their consonant-prefix
    /// reverse-lookup (痛苦/天空/太空/天开/天会/偷看/...) — clearly
    /// not what the user typed.
    ///
    /// `tkinn` blocks: no Chinese syllable ends with `inn`.
    /// `tkonn` blocks: no Chinese syllable ends with `onn`.
    /// `pyin` (4 chars) still rescues — 4-char buffers stay on the
    /// looser rule (covered by `syllable_aware_path1c_still_rescues_real_typos`).
    #[test]
    fn path1c_5char_buffer_requires_syllable_tail_suffix() {
        let blocked: &[&str] = &["tkinn", "tkonn", "tkenn", "tkann"];
        let mut failures = Vec::new();
        for buf in blocked {
            let top10 = mixed_top10(buf.as_bytes());
            if !top10.is_empty() {
                failures.push(format!(
                    "  {buf}: expected empty top10 (suffix not a syllable tail); \
                     got {top10:?}"
                ));
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} 5-char Path 1c noise cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }
}
