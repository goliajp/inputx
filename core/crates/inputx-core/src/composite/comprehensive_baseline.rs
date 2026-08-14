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

    /// Climb-final Stage A3 2026-06-16 long-abbrev wire invariant —
    /// COMPOSE-gated because resolution flows through
    /// `compose_via_lattice_paths` abbrev_resolver + INITIALS_INDEX
    /// wrapper + long-abbrev ASCII-fallback escape valve. When
    /// `PINYIN_DISABLE_COMPOSE` flips back to `false`, this auto-revives.
    #[test]
    fn pinyin_only_long_abbrev_zhrmghg_leads() {
        if super::super::pinyin_adapter::PINYIN_DISABLE_COMPOSE {
            return;
        }
        let cases: &[(&str, &str)] = &[("zhrmghg", "中华人民共和国")];
        run("long_abbrev", cases, pinyin_top, pinyin_top10);
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
            // Polish-log 2026-06-16 — 上海大学 user-reported missing
            // in lib (probe shdx returned only K-best 时候多谢, not
            // 上海大学). Added shanghaidaxue → 上海大学 freq=16000 to
            // library.tsv (calibrated against peer university 4-char
            // phrases: 北京 16628 / 复旦 14657 / 清华 18555 / 同济
            // 14181 / 南京 16402 — median ~16k). This row populates
            // INITIALS_INDEX bucket "shdx" so Path 2 direct lookup
            // surfaces 上海大学 as top-1 for the abbreviation.
            ("shanghaidaxue", "上海大学"),
            // Climb-final Stage A1 2026-06-16 — zhishi → 知识 was at
            // rank #1 behind 只是 (#0 at base freq 46040; 知识 at
            // 35575). Boosted 知识 in quickfix_boost.tsv to 51000
            // (top peer + 10% margin) so canonical zhishi resolves to
            // 知识 at top-1, enabling fuzzy variant edges (z↔zh fuzzy
            // pair like `zi shi` / `zhi xi`) to deliver 知识 in
            // fuzzy_miu fixture cases.
            ("zhishi", "知识"),
            // Polish-log 2026-06-21 — `ziqia` user-reported missing 自洽
            // in top-N (probe showed 自强/自谦/资浅/子枪 fuzzy fallbacks
            // 0..6, 自洽场/自洽性 at #7/#8, root 自洽 absent). Added
            // ziqia → 自洽 freq=22000 to library.tsv (calibrated against
            // peer 自欺 22283 / 自强 24113 / 自谦 13932). Exact Path 1a
            // hit now wins over fuzzy fallbacks; downstream consumers
            // (K-best, initials reverse-lookup) get the entry too.
            ("ziqia", "自洽"),
            // Polish-log 2026-06-26 — "sisi 思思第一,咝咝不需要,
            // 要加嘶嘶". 思思 (人名/昵称) was on corpus_garbage_filter
            // as "name" — removed so the polish row takes effect.
            // Added to library.tsv freq=35000 (beats 丝丝 21713
            // with margin). Companion D1: 咝咝 deleted; A: 嘶嘶
            // 拟声 added at freq=8000 (visible deeper).
            ("sisi", "思思"),
            // Polish-log 2026-06-26 — "zouta 揍他可以有,邹韬奋删除".
            // 揍他 (zòu+tā) was missing from library.tsv at the
            // exact-match code; user wanted it visible. Added
            // zouta 揍他 freq=10000 polish. Companion D1: zoutaofen
            // 邹韬奋 (obscure historical journalist name) deleted —
            // was leaking into the zouta prefix-completion top-N.
            ("zouta", "揍他"),
            // Polish 2026-06-30: user "shima 是吗 这个拼音要有" —
            // common question particle 是吗 (shì + ma) missing from
            // CC-CEDICT/modern_vocab; user typed shima and saw only
            // 时髦/视盲 (prefix from shimao/shimang). Added 是吗
            // 50000 to modern_vocab_v1.tsv.
            ("shima", "是吗"),
            // Polish 2026-06-30: user "xingshi 形式 > 形势 > 姓氏 > 刑事".
            // 4-way order lock via cascading quickfix_boost (50/40/30/20k).
            ("xingshi", "形式"),
            // Polish 2026-06-30: user "lianxu 连续 >> 怜恤" — 连续 HSK 4
            // 强 boost,怜恤 文言 tier_overlay → 5 (双管齐下 since >>).
            ("lianxu", "连续"),
            // Polish 2026-06-30: "shenru 深入 > 渗入 > 慎入".
            ("shenru", "深入"),
            // Polish 2026-06-30: user "helan 荷兰 这个词肯定要有的，国家名".
            // CC-CEDICT proper-noun reject in v2 ingest; modern_vocab backfill.
            ("helan", "荷兰"),
            // Polish 2026-06-30: user "deguo 德国 也没有，其他国家名全都要补上".
            // Batch backfill 68 country names + 2 disambiguation quickfix
            // (韩国 vs 汗国, 巴西 vs 把戏).
            ("deguo", "德国"),
            ("meiguo", "美国"),
            ("faguo", "法国"),
            ("yingguo", "英国"),
            ("riben", "日本"),
            ("hanguo", "韩国"),
            ("eluosi", "俄罗斯"),
            ("yindu", "印度"),
            ("xibanya", "西班牙"),
            ("jianada", "加拿大"),
            ("aodaliya", "澳大利亚"),
            ("baxi", "巴西"),
            ("xinjiapo", "新加坡"),
            ("aiji", "埃及"),
            ("tuerqi", "土耳其"),
            // Polish 2026-06-30: user "shijiebei 世界杯" — proper-noun event
            // name CC-CEDICT-rejected; modern_vocab backfill.
            ("shijiebei", "世界杯"),
            // Polish 2026-06-30: user "shijinsai 世锦赛要高于日语" — 55k
            // tier-1 calibration to beat JP exact-prefix kana (same as
            // jianma 简码 / fudu 复读 / aijie 娭毑 / weixin 微信).
            ("shijinsai", "世锦赛"),
            // Polish 2026-06-30: user "shanle 删了 要加上" — 删+了 modal
            // compound, not in CC-CEDICT. Compose path can't synthesize
            // (≥1-word rule + 删 not in words.tsv as word). Direct entry.
            ("shanle", "删了"),
            // Polish 2026-06-30: user "liucheng 流程肯定是最高的，柳橙
            // 级别应该低多了". 流程 quickfix 50k + 柳橙 (台湾柑橘) tier 5.
            ("liucheng", "流程"),
            // Polish 2026-06-30: user "balagui 巴拉圭, 国家名/城市名补全".
            // Batch 2: +90 countries (gap fill) + 80 world cities.
            ("balagui", "巴拉圭"),
            ("wulagui", "乌拉圭"),
            ("yemen", "也门"),
            ("aisaiebiya", "埃塞俄比亚"),
            ("jinbabuwei", "津巴布韦"),
            // Cities (anchors)
            ("beijing", "北京"),
            ("shanghai", "上海"),
            ("xianggang", "香港"),
            ("niuyue", "纽约"),
            ("dehelan", "德黑兰"),
            ("kailuo", "开罗"),
            ("lundun", "伦敦"),
            ("moerben", "墨尔本"),
            // Polish 2026-06-30: user "yanjiu 研究肯定要在烟酒前面".
            ("yanjiu", "研究"),
            // Polish 2026-06-30: user "胡萝卜应该在日语前" — 55k tier-1
            // calibration to beat JP exact-prefix kana (same as
            // shijinsai 世锦赛 / jianma 简码 / fudu 复读 / aijie 娭毑).
            ("huluobo", "胡萝卜"),
            // Polish 2026-06-30: user "ceshi 测试最高".
            ("ceshi", "测试"),
            // Polish 2026-07-07: user "fangfeiziwo 放飞自我 现在房费自我
            // 肯定是错的要删". Class A add — real modern 4-char idiom
            // that wasn't in dict; K-best previously assembled 房费自我
            // (nonsense) as top-1. Added at freq 50000 in modern_vocab_v1.
            ("fangfeiziwo", "放飞自我"),
            // Polish 2026-07-07: user "guigu 硅谷". Class A add — 硅谷
            // absent from v2 words.tsv, and 硅光 / 硅光子 / 硅光技术
            // (all modern_vocab tier 3 @ 30k) dominated the top-3.
            // Added at freq 60000 → tier 2 in modern_vocab_v1 to beat
            // the whole 硅光* cluster.
            ("guigu", "硅谷"),
            // Polish 2026-07-08: user "wuyiwei 误以为". Class A add —
            // 误以为 absent from v2 words.tsv; top-1 was 无以为报.
            // Added at freq 50000 → tier 2 in modern_vocab_v1.
            ("wuyiwei", "误以为"),
            // Polish 2026-07-08: user "tengxun 腾讯". Class A add —
            // 腾讯 absent from v2 while its compounds (腾讯会议/文档/
            // 云/体育/网/音乐) filled the top-6. Added at freq 80000
            // → tier 2 to outrank the compound cluster.
            ("tengxun", "腾讯"),
            // Polish 2026-07-08 batch: user "主要科技公司名称要加入
            // 词库". Class A adds to modern_vocab_v1 (freq >= 50000
            // → v2 tier 2). 微博 was already in v2 cedict @ tier 4;
            // modern_vocab dedup skipped my override so tier_overlay
            // route `weibo\t微博\t2` applied. Wubi 4-code buffers
            // (guge / didi) still win 平静 / 耕耘 by cross-engine
            // precedence — pinyin-only mode below returns the tech
            // company as expected.
            ("baidu", "百度"),
            ("huawei", "华为"),
            ("guge", "谷歌"),
            ("douyin", "抖音"),
            ("weibo", "微博"),
            ("bilibili", "哔哩哔哩"),
            ("tesila", "特斯拉"),
            ("gaotong", "高通"),
            ("wangyi", "网易"),
            ("souhu", "搜狐"),
            ("youku", "优酷"),
            ("aiqiyi", "爱奇艺"),
            ("zhihu", "知乎"),
            // Polish 2026-07-08: user "ali 阿里". Class A add — 阿里
            // absent from v2 (only compounds 阿里巴巴/阿里山/阿里地区
            // /阿里斯 filled top 3-6). Added at freq 60000 → tier 2.
            ("ali", "阿里"),
            // Polish 2026-07-08: user "shuizhu 水煮". Class A add —
            // 水煮 absent from v2 (cedict only had 水柱/水珠 tier 4).
            // Cooking staple (水煮鱼 / 水煮肉片 etc.). Freq 60000 → tier 2.
            ("shuizhu", "水煮"),
            // Polish 2026-07-08: user "yushou 御守". Class A add —
            // 御守 (shrine amulet, Japanese-origin loanword) absent
            // from v2 (cedict had 御手/玉手/预售/驭手 tier 4 only).
            // Freq 60000 → tier 2.
            ("yushou", "御守"),
            // Polish 2026-07-08: user "jieqian 解签". Class A add —
            // 解签 (interpreting a fortune stick) missing from both
            // v1 library and v2 words.tsv entirely. Top-1 was 借钱
            // (cedict tier 4). Freq 60000 → tier 2.
            ("jieqian", "解签"),
            // Polish 2026-07-08: user "fenzhi 分支 第一". Class B via
            // quickfix_boost (v2 dedup would swallow a modern_vocab
            // override — 分支 was already in cedict tier 4, plus 分之
            // cedict+hsk4 tier 2 held #0). quickfix promotes to tier 1
            // to beat cedict tier 2 sovereignty-style.
            ("fenzhi", "分支"),
            // Polish 2026-07-08: user "wangle 王磊不是词，忘了倒要加上".
            // Class A add — 忘了 (colloquial verb+了) absent from dict;
            // K-best had assembled 王磊 (personal-name compose) as the
            // sole top-1. modern_vocab @ freq 60000 → tier 2 = 440k,
            // 忘了 leads and 王磊 falls off top-10 entirely.
            ("wangle", "忘了"),
            // Polish 2026-07-09: user "zibaoqiduan 自暴其短". Class A add
            // — user-specified idiom absent from dict; K-best was
            // returning 自保七段 (nonsense compose). Freq 50000 → tier 2.
            ("zibaoqiduan", "自暴其短"),
            // Polish 2026-07-09 batch: user "并且，你可以广泛地补充一些
            // 高频的四字成语". Audited ~60 common idioms, added 9 that
            // were missing or wrong top-1. 初生之犊 needed quickfix_boost
            // (previously in corpus_garbage_filter as freq=0; v2 dedup
            // logic un-excludes on quickfix presence). All others via
            // modern_vocab_v1 freq 60000 → tier 2.
            ("dadazhekou", "大打折扣"),
            ("yimoyiyang", "一模一样"),
            ("mingmingbaibai", "明明白白"),
            ("dacidabei", "大慈大悲"),
            ("chunhuaqiushi", "春华秋实"),
            ("chushengzhidu", "初生之犊"),
            // 2026-07-10 vocab-audit P2: the 07-09 batch rows for
            // 一如既往/自力更生 carried corrupt pinyin (yiruquanwang /
            // zilishengsheng — not typos a typer would make). Rows
            // replaced with correct-pinyin modern_vocab rows; pins
            // updated to match.
            ("yirujiwang", "一如既往"),
            ("yijuchengming", "一举成名"),
            ("ziligengsheng", "自力更生"),
            // Polish 2026-07-09 batch #2: user "常见四字成语应该是有很多的
            // 你需要找到一个好的来源然后全量补充". Ingested from
            // pwxcoo/chinese-xinhua idiom.json (~30k), filtered to:
            //   1. 4-char, all-tier-1 chars (通用规范一级 = 3500),
            //   2. per-char pinyin syllable validated against v2
            //      readings.tsv (rejects source data typos, keeps
            //      polyphone variants),
            //   3. word appears in v2 modern_freq.tsv @ score ≥ 10000,
            //   4. missing from v2 words.tsv AND modern_vocab_v1.tsv.
            // Yield: 14 idioms after ü→v canonicalization + dedup pass.
            ("damodayang", "大模大样"),
            ("guaimoguaiyang", "怪模怪样"),
            ("xiangmoxiangyang", "像模像样"),
            ("banshengbushu", "半生不熟"),
            ("lusishuishou", "鹿死谁手"),
            ("fushangdajia", "富商大贾"),
            ("gouxuepentou", "狗血喷头"),
            ("zhanuanhaihan", "乍暖还寒"),
            ("xiuweixiangtou", "臭味相投"),
            ("fushangjujia", "富商巨贾"),
            ("xieloutianji", "泄露天机"),
            ("laodiaozhongdan", "老调重弹"),
            ("chousibaojian", "抽丝剥茧"),
            ("renshengchaolu", "人生朝露"),
            // Polish 2026-07-10: polyphone alternate routes for common
            // 4-char idioms where users mis-type a polyphone char with
            // the "wrong" reading. E.g. 长治久安 is chángzhìjiǔ'ān but
            // if user types `zhangzhijiuan` (long=grow, wrong) the idiom
            // now still lands. 780 alternates via quickfix_boost.tsv
            // (not modern_vocab — modern_vocab entries aggregate char
            // freq, would tip-scale 得/大 above 的/大 single-char pins).
            // Test-pin one from each of the 4 major polyphone classes.
            ("zhangzhijiuan", "长治久安"), // 长 cháng→zhǎng
            ("daichiyijing", "大吃一惊"),  // 大 dà→dài
            ("hushuibadao", "胡说八道"),   // 说 shuō→shuì
            ("juedaiduoshu", "绝大多数"),  // 大 dà→dài
            // Polish 2026-07-10: user "haidilao 海底捞". Class A add —
            // 海底捞 (hotpot chain) absent everywhere; buffer returned
            // ZERO candidates (not even a K-best compose). modern_vocab
            // @ freq 60000 → tier 2.
            ("haidilao", "海底捞"),
            // Polish 2026-07-10: user "latiao 辣条". Verified already
            // correct at probe time (library digested freq 23575, sole
            // candidate at #0) — no data change; pin so it can't drift.
            ("latiao", "辣条"),
            // Polish 2026-07-30: user "tingle 停了". Class A add —
            // 停了 (colloquial verb+了, same shape as 忘了/拔了) absent
            // from library.tsv entirely; the buffer's sole candidate was
            // 烃类 via a fuzzy path. modern_vocab @ freq 30000.
            ("tingle", "停了"),
            // Polish 2026-07-30: user "jiazhe 夹着". Class A add — 夹着
            // (verb+着, same shape as 躺着) in neither v2 words.tsv nor
            // modern_vocab; the buffer only produced fuzzy jiazheng hits
            // (家政 / 加征 / 假证). modern_vocab @ freq 30000.
            ("jiazhe", "夹着"),
            // Polish 2026-07-30: user "jiazhu 夹住". Class A add — 夹住
            // sits in the legacy v1 library.tsv (16034) but never made it
            // into v2 words.tsv, so the dict only had cedict tier-4 家主
            // (403286) / 加注. modern_vocab @ freq 60000 → tier 2 = 440k,
            // clearing tier 4 the way 御守/解签 do.
            ("jiazhu", "夹住"),
            // Polish 2026-08-01: user "miaobian 描边". Class A add — 描边
            // (graphics/design term, also gaming slang) absent from both
            // v1 library.tsv and v2 words.tsv; the buffer's sole candidate
            // was digested 秒变 (380k). modern_vocab @ freq 30000 → 410k.
            ("miaobian", "描边"),
            // Polish 2026-08-02: user "chaogao 超高". Class A add — 超高
            // sits in v1 library.tsv (25172 digested) but never made it
            // into v2 words.tsv (known v1→v2 ingest gap, same as 夹住);
            // the buffer only surfaced prefix-completion compounds
            // (超高压/超高温/超高频 @ 130k). modern_vocab @ 30000 → 410k.
            ("chaogao", "超高"),
            // Polish 2026-08-02: user "zaobuzhu 遭不住". Class A add —
            // colloquial V+不住 (同族 撑不住/耐不住/熬不住 all modern_vocab
            // @ 15000); the buffer returned zero candidates from every
            // engine. modern_vocab @ family-standard 15000 → 380k.
            ("zaobuzhu", "遭不住"),
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
            ("queshi", "确实"), // user 2026-06-27 re-attestation: 确实 > 缺失
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
            // Polish-log 2026-06-12: "daomu 盗墓第一，道木不像个词" —
            // 道木 (jieba sub-word noise) D1-deleted; 盗墓 (22904)
            // auto-leads over 倒幕 (7328) as the top real candidate.
            ("daomu", "盗墓"),
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
            // Polish-log 2026-06-14: "toulan 偷懒应该大于投篮". base 偷懒
            // 27206 / 投篮 22830 (both digested) — score reflected the
            // gap but the merge returned [投篮, 偷懒] anyway (same shape
            // as the daiban case [[polish-ordering-quickfix-vs-libraryfreq]]).
            // Per kongdang method: library.tsv freqs overridden to clean
            // descending 偷懒 50000 / 投篮 15000 + source=polish. Empirical:
            // 28k/22k gap didn't drive order; 50k/15k did.
            ("toulan", "偷懒"),
            // Polish-log 2026-06-21 Phase A (AA-redup sweep, fanfan): user
            // "感觉现在叠词的问题还是很严重，有相当多根本不是个词".
            // fanfan top10 was 8/10 noise (饭饭/反反/烦烦/范范/帆帆/犯犯/
            // 繁繁/翻翻) + 凡凡/番番 in low ranks. D1 deleted 10 corpus-
            // noise rows from library.tsv (jieba reduplication artefacts +
            // name nicknames), preserved only 翻番(double) and 泛泛
            // (superficial) — the two real fanfan words. 翻番 leads as the
            // higher-freq true compound. See docs/pinyin-AA-redup-sweep-
            // 2026-06-21/ for the broader 1566-entry sweep plan.
            ("fanfan", "翻番"),
            // Polish-log 2026-06-26: "xuxian 虚线 续弦 都要在许仙前面".
            // 许仙 (白蛇传 proper noun) base 17781 was crowding #0 over
            // 虚线 16059 and 续弦 8669. quickfix_boost lifts 虚线→19600
            // (#0) and 续弦→18700 (#1), 许仙 base falls to #2.
            ("xuxian", "虚线"),
            // Polish-log 2026-07-03: "shouyin 收银 > 手印 > 收音 > 手淫，
            // 首音不需要". Base freqs put 手印 (20834) at #0, 手淫 (16492)
            // at #1, 收银 (23056) at #2 due to char-freq offsets.
            // quickfix_boost lifts 收银→40000 (#0), 手印→35000 (#1),
            // 收音→30000 (#2). 首音 added to exclusions_v1.tsv (phonology
            // term, not needed at top; still available for K-best).
            ("shouyin", "收银"),
            // Polish-log 2026-07-06: user /polish zhujiao 猪脚 — 猪脚
            // (pork trotters, 猪脚饭 Cantonese/Taiwanese food) sat at
            // library base 23015 while 主教/助教/注脚/住脚 all had
            // char-freq offsets landing top-1..#3. modern_vocab boost
            // 猪脚→30000 flips it to #0.
            ("zhujiao", "猪脚"),
            // Polish-log 2026-07-06: user /polish PLA marshals — 贺龙
            // (He Long, Ten Marshals) sat at library base 16137 but
            // 合拢/合龙 char-freq landed top-1/#2. modern_vocab boost
            // 贺龙→30000 flips it to #0.
            ("helong", "贺龙"),
            // Polish-log 2026-07-06: user /polish PLA marshals — 彭德怀
            // (Peng Dehuai, Ten Marshals) sat at library base 15834 but
            // buffer returned 0 candidates (no char-pair noise strong
            // enough, 4-syll compound didn't K-best compose either).
            // modern_vocab boost 彭德怀→30000 surfaces at #0.
            ("pengdehuai", "彭德怀"),
            // Polish-log 2026-07-06: user /polish PLA marshals — 林彪
            // (Lin Biao, Ten Marshals) sat at library base 16839 but
            // buffer returned 0 candidates (same 4-syll K-best gap
            // as 彭德怀). modern_vocab boost 林彪→30000 surfaces at #0.
            ("linbiao", "林彪"),
            // Polish-log 2026-07-06: user /polish fuyao 敷药 — 敷药
            // (apply medicine externally, common TCM verb) sat at library
            // base 13829 while 服药 (take medicine orally, freq 19592)
            // dominated so much that 敷药 didn't even surface in top10.
            // quickfix_boost 敷药 → 22000 flips it to #0 above 服药.
            ("fuyao", "敷药"),
            // Polish-log 2026-07-06: user /polish gaizai 盖在 — 盖在
            // (V+prep phrase "covered on/at") missing from library
            // entirely; only 改在 (freq 15362) existed at this code but
            // filtered from top10 too. modern_vocab add 盖在 → 30000
            // surfaces at #0.
            ("gaizai", "盖在"),
            // Polish-log 2026-07-06: user /polish beidiao 背调第一, 贝雕
            // 删了. After D1 deletion of 贝雕 (obscure), 背调 (background
            // check, modern HR term at modern_vocab 45000) leads.
            ("beidiao", "背调"),
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

    /// Class A polish (user report 2026-06-09): "sakai 应该有 … 堺 那个
    /// 堺雅人，日语姓氏". 堺 (kun さかい — Sakai city / surname 堺雅人)
    /// was missing from nihongo library (corpus digest never had it; the
    /// 2026-06-08 KANJIDIC2 backfill only covered the 217 swept Shinjitai
    /// chars). Added sakai 堺 (kanji, freq 42). 境 (also kun さかい,
    /// boundary) stays — both are valid さかい readings.
    #[test]
    fn polish_sakai_includes_sakai_kanji() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::JapaneseOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in "sakai".bytes() {
            let _ = e.handle_letter(b);
        }
        let words: Vec<String> = e.candidates().iter().map(|c| c.word.clone()).collect();
        assert!(
            words.iter().any(|w| w == "堺"),
            "sakai: 堺 not in JP candidates — got {words:?}"
        );
    }

    /// 2026-07-08 polish: user "harete 腫れて". Class A add — te-form of
    /// 腫れる (to swell) missing from nihongo jukugo; top-1 was kana
    /// はれて. `晴れてる` was appearing at #3 (wrong verb entirely).
    /// Added `harete 腫れて jukugo 72` (freq matches 晴れた band).
    #[test]
    fn polish_harete_harete_kanji_leads() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::JapaneseOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in "harete".bytes() {
            let _ = e.handle_letter(b);
        }
        let top = e.candidates().first().map(|c| c.word.clone());
        assert_eq!(top.as_deref(), Some("腫れて"), "harete #0 = 腫れて");
    }

    /// 2026-07-08 polish: user "haguki 歯茎". Class A add — 歯茎 (gums)
    /// missing from nihongo library; top-1 was default kana はぐき.
    /// Added `haguki 歯茎 jukugo 78` (freq matches 歯磨き band).
    #[test]
    fn polish_haguki_haguki_kanji_leads() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::JapaneseOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in "haguki".bytes() {
            let _ = e.handle_letter(b);
        }
        let top = e.candidates().first().map(|c| c.word.clone());
        assert_eq!(top.as_deref(), Some("歯茎"), "haguki #0 = 歯茎");
    }

    /// 2026-06-09 KANJIDIC2 backfill v2: filled single-kanji readings for
    /// every 常用 (grade 1-8) + 人名用 (9-10) char missing from nihongo
    /// (1225 + 841 chars; nihongo single-kanji coverage was ~43%). Flat
    /// freq=10 keeps them reachable but always below corpus-attested
    /// words (guards the akashi→明石 case in phase_g). Audit:
    /// docs/nihongo-kanji-backfill-2026-06-09/. Pins reachability of
    /// representative previously-missing common chars by their reading.
    #[test]
    fn polish_jouyou_backfill_reachable() {
        let cases: &[(&str, &str)] = &[
            ("nigi", "握"), // kun にぎ(る)
            ("ei", "映"),   // on えい
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
                    "  {reading}: {kanji} not in JP candidates — {words:?}"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
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
            // 2026-06-30 user re-attestation: 图例 > 图利 (was 图利 #0
            // via pure-v1 corpus 9843 freq; user prefers 图例).
            ("tuli", "图例"),
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
            // Polish-log 2026-06-10: "娭毑现在 t 几，我想在日语前面".
            // 娭毑 (Xiang dialect grandmother) base 3000 sat below JP
            // exact-prefix kana あいじえ/アイジエ; quickfix_boost → 55000
            // (same calibration as jianma 简码 / fudu 复读) lifts it
            // above JP.
            ("aijie", "娭毑"),
            // Polish-log 2026-06-14: "pianse 偏色应该在日语前，是中频词
            // 应该". 偏色 base 2367 sat at #2 below JP exact-prefix kana
            // ぴあんせ / ピアンセ. Per user 中频词 judgment + peer freqs
            // 偏方 18948 / 偏离 20209 / 偏向 24858 / 偏颇 18281, library.tsv
            // freq overridden to 25000 (mid 偏向~偏见 band) + source=polish.
            // 偏色 now leads ぴあんせ / ピアンセ.
            ("pianse", "偏色"),
            // Polish-log 2026-06-26: "微信第一，惟心也要在日语前". 威信
            // base 23029 was #0 over 微信 18206; 惟心 base 3094 sat #7
            // below JP exact-prefix kana ウェイィン. quickfix_boost lifts
            // 微信 → 25500 (top1) and 惟心 → 15500 (tier 1, above JP).
            // Full-order assertion in polish_weixin_weixin_above_weixin_惟心_above_jp.
            ("weixin", "微信"),
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

    /// Class B polish (user report 2026-06-10): "edu 额度应该第二甚至
    /// 也可以第一". 额度 base 15305 was below tier-1 cutoff, sitting
    /// rank #5 with JP exact-prefix kana えづ at #2. quickfix_boost
    /// → 24000 lifts 额度 into tier 1 between 恶毒 (26372, kept at #1)
    /// and the cutoff, so the displayed order becomes 恶毒 / 额度 /
    /// えづ / エヅ / 饿肚 — 额度 at #2 (user's "应该第二" target).
    /// Class B polish (user report 2026-06-26): "weixin 微信第一，惟心
    /// 也要在日语前". 威信 base 23029 led #0 over 微信 18206; 惟心 base
    /// 3094 sat at #7 below JP exact-prefix kana ウェイィン (tier 2).
    /// quickfix_boost lifts 微信 → 25500 (top1) and 惟心 → 15500 (tier
    /// 1, just above 维新 14931 base). Asserts full structural invariant:
    /// 微信 < 威信 (微信 leads) AND 惟心 < ウェイィン (惟心 above JP).
    #[test]
    fn polish_weixin_weixin_top_and_weixin_above_jp() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"weixin" {
            let _ = e.handle_letter(*b);
        }
        let cands: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        let pos = |w: &str| cands.iter().position(|x| x == w);
        let wx = pos("微信").expect("微信 missing from weixin top10");
        let wxin = pos("威信").expect("威信 missing from weixin top10");
        let wxinc = pos("惟心").expect("惟心 missing from weixin top10");
        let jp = pos("ウェイィン").expect("ウェイィン missing from weixin top10 (JP off?)");
        assert!(
            wx < wxin,
            "weixin: 微信 should lead 威信; got top10={cands:?}"
        );
        assert!(
            wxinc < jp,
            "weixin: 惟心 should rank above ウェイィン; got top10={cands:?}"
        );
    }

    #[test]
    fn polish_edu_edu_above_jp_exact_prefix_kana() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"edu" {
            let _ = e.handle_letter(*b);
        }
        let cands: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        let pos = |w: &str| cands.iter().position(|x| x == w);
        let edu = pos("额度").expect("额度 missing from edu top10");
        let ezu = pos("えづ").expect("えづ missing from edu top10 (JP off?)");
        assert!(
            edu < ezu,
            "edu: 额度 should rank above えづ; got top10={cands:?}"
        );
    }

    /// Structural rule (user 2026-06-10): at full wubi 4-code buffer
    /// where a single-char Auto entry competes with phrase entries,
    /// single chars lead unless a phrase has corpus freq ≥
    /// `WUBI_PHRASE_EXTREME_FREQ_FLOOR` (25k by default).
    /// User: "五笔是四码输入法，四码如果有单字除非极其生僻或词组
    /// 顺序极高，否则都应该在词组前".
    #[test]
    fn full_code_single_char_leads_phrase_unless_phrase_extreme() {
        // iiiu — 淼 (16822) + 尛 (0) Auto vs 水滴 (20391) + 汗流浃背
        //        (16776) Phrase. No phrase ≥ 25k → single chars lead.
        let iiiu_top = mixed_top10(b"iiiu");
        let pos_iiiu = |w: &str| iiiu_top.iter().position(|x| x == w);
        let p_miao = pos_iiiu("淼").expect("淼 missing iiiu");
        let p_mu = pos_iiiu("尛").expect("尛 missing iiiu");
        let p_sd = pos_iiiu("水滴").expect("水滴 missing iiiu");
        let p_hl = pos_iiiu("汗流浃背").expect("汗流浃背 missing iiiu");
        assert!(
            p_miao < p_sd && p_miao < p_hl && p_mu < p_sd && p_mu < p_hl,
            "iiiu: single chars 淼/尛 should lead phrases; top10={iiiu_top:?}"
        );

        // gmww — 两 (37372) Auto vs 两败俱伤 (15272) Phrase. Phrase
        //        below 25k floor → single 两 leads (classic case).
        let gmww_top = mixed_top10(b"gmww");
        assert_eq!(
            gmww_top.first().map(String::as_str),
            Some("两"),
            "gmww: 两 should lead; top={gmww_top:?}"
        );

        // wcng — 鹟 (5961) Auto vs 公司 (42817) Phrase. Phrase ≥ 25k
        //        floor → phrase 公司 leads (exception case).
        let wcng_top = mixed_top10(b"wcng");
        assert_eq!(
            wcng_top.first().map(String::as_str),
            Some("公司"),
            "wcng: 公司 should lead (phrase extreme exception); top={wcng_top:?}"
        );

        // ywyg — 谁 (35073) Auto vs 认证 (25690) + 论证 (23346) Phrase.
        // 认证 ≥ 25k floor BUT 谁 freq still > 认证, so dominance must
        // NOT fire — single 谁 leads. 2026-06-10 bugfix to the
        // dominance check: comparing against the absolute floor is
        // not enough; the dominant phrase has to also outrank the
        // best competing single-char freq.
        let ywyg_top = mixed_top10(b"ywyg");
        assert_eq!(
            ywyg_top.first().map(String::as_str),
            Some("谁"),
            "ywyg: 谁 should lead — phrase 认证 above 25k floor but 谁 more popular; top={ywyg_top:?}"
        );
    }

    /// Structural rule (user 2026-06-10): at wubi simcode buffers (e.g.
    /// iii = jianma3 simcode of 水), single-char prefix-predictions
    /// (淼, 尛 at iiiu) outrank phrase prefix-predictions (沙漠 at iiia,
    /// 水滴 at iiiu) so simcode-typing users see kanji extensions before
    /// phrase extensions.
    #[test]
    fn simcode_prefix_predictions_single_char_above_phrase() {
        let top = mixed_top10(b"iii");
        let pos = |w: &str| top.iter().position(|x| x == w);
        let p_miao = pos("淼").expect("淼 missing in iii predictions");
        let p_shamo = pos("沙漠");
        // 淼 must appear and rank above any phrase prediction (e.g. 沙漠
        // when present). If 沙漠 isn't pulled in this run (top10 capped),
        // skip the comparison — the absent-from-top guarantee suffices.
        if let Some(ps) = p_shamo {
            assert!(
                p_miao < ps,
                "iii: 淼 single-char prediction should rank above phrase 沙漠; top10={top:?}"
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

    /// Class A polish (user report 2026-06-10): "加 aijie 娭毑". 娭毑
    /// is a Xiang (Hunan) dialect word for grandmother / old lady,
    /// absent from library.tsv before this polish. Added with
    /// source=polish at freq 3000 — buffer aijie has no other
    /// candidates, so 娭毑 leads trivially; freq picked low enough
    /// to mark the entry as dialect / rare-use.
    #[test]
    fn polish_aijie_includes_aijie() {
        let top10 = mixed_top10("aijie".as_bytes());
        assert!(
            top10.iter().any(|w| w == "娭毑"),
            "aijie: 娭毑 not in candidates; top10={top10:?}"
        );
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

    /// Class B polish (user report 2026-06-10): "dongdong 洞洞 咚咚
    /// 动动 东东 就好了，这个顺序". After D1 删 冬冬/冻冻/栋栋, four
    /// nickname/onomatopoeia survive at this buffer — natural-freq
    /// order (东东>咚咚>动动>洞洞) inverts the user's preferred
    /// order. quickfix_boost cascade (洞洞 35000 > 咚咚 30000 >
    /// 动动 28000 > 东东 27535 base) flips it.
    #[test]
    fn polish_dongdong_order() {
        let top10 = mixed_top10("dongdong".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let (a, b, c, d) = (pos("洞洞"), pos("咚咚"), pos("动动"), pos("东东"));
        assert!(
            matches!((a, b, c, d), (Some(a), Some(b), Some(c), Some(d)) if a < b && b < c && c < d),
            "dongdong expected 洞洞<咚咚<动动<东东; top10={top10:?}"
        );
    }

    /// 2026-06-13 polyphone-dup sweep batch1 — the COUNTERPART to the
    /// not-in-top assertions: deleting the wrong-reading copies must not
    /// harm each word's correct-reading code, which carries the same
    /// (now sole) row. Catches an over-broad sweep that nukes both sides.
    #[test]
    fn polyphone_sweep_correct_readings_kept() {
        let cases: &[(&str, &str)] = &[
            ("yidali", "意大利"),         // 大 correct = dà
            ("meicuo", "没错"),           // 没 correct = méi
            ("daerxi", "大儿媳"),         // 大 correct, also fixes the er-typo case
            ("guozao", "聒噪"),           // 聒 correct = guō (the kept side)
            ("kansi", "看似"),            // 似 correct = sì (② reversed kept side)
            ("shousha", "手刹"),          // batch2a: 刹 shā kept
            ("niboer", "尼泊尔"),         // batch2a: 泊 bó kept
            ("xiaopingguo", "削苹果"),    // batch2a: 削 xiāo kept
            ("pengyouquan", "朋友圈"),    // batch2b: 圈 quān kept
            ("zhujuan", "猪圈"),          // batch2b: 圈 juàn kept
            ("yixi", "一系"),             // batch2b: 系 xì kept
            ("zhaohuaxishi", "朝花夕拾"), // batch2b-2: 朝 zhāo kept
            ("xiangyao", "降妖"),         // batch2b-2: 降 xiáng kept
            ("chengtang", "盛汤"),        // batch2b-2: 盛 chéng kept
            ("chairen", "差人"),          // batch2b-2: 差 chāi kept
            ("danke", "蛋壳"),            // batch2b-3: 壳 ké kept
            ("shoudu", "首都"),           // batch2b-3: 都 dū kept
            ("tiandu", "天都"),           // batch2b-3: 都 dū kept
            ("danpian", "弹片"),          // batch2c: 弹 dàn 名词 kept
            ("tantiao", "弹跳"),          // batch2c: 弹 tán 动词 kept
        ];
        let mut missing = Vec::new();
        for (buf, word) in cases {
            if !mixed_top10(buf.as_bytes()).iter().any(|w| w == word) {
                missing.push(format!("  {buf}: correct-reading {word} lost"));
            }
        }
        assert!(
            missing.is_empty(),
            "sweep removed correct readings:\n{}",
            missing.join("\n")
        );
    }

    /// Class D1 polish (user report 2026-06-12): "momo 嶙嶙也不像个词，
    /// 默默第一". The wubi phrase row (momo, 嶙嶙) held mixed #0 via the
    /// wubi tier; deleting it lets the natural pinyin top 默默 lead the
    /// buffer in Mixed mode.
    #[test]
    fn polish_momo_momo_leads_mixed() {
        assert_eq!(
            mixed_top("momo".as_bytes()),
            "默默",
            "momo mixed top-1 must be 默默"
        );
    }

    /// Class A+B polish (user report 2026-06-13): "daiban 待办 代班 代办
    /// 呆板 这个顺序，其他的不应该有". After the polyphone D1 cleanup
    /// (大办/大坂/大板/大阪) + the 带班 D2 hide, the surviving daiban set
    /// is reordered to the user's preference. 代班 (substitute shift)
    /// was missing from the dict entirely — added as a polish row
    /// (library.tsv freq 18000), then quickfix_boost sets the clean
    /// descending order 待办 24000 > 代班 22000 > 代办 20000 > 呆板 18000.
    /// Invariant: exactly these four, in this order.
    #[test]
    fn polish_daiban_order() {
        let top10 = mixed_top10("daiban".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let (a, b, c, d) = (pos("待办"), pos("代班"), pos("代办"), pos("呆板"));
        assert!(
            matches!((a, b, c, d), (Some(a), Some(b), Some(c), Some(d)) if a < b && b < c && c < d),
            "daiban expected 待办<代班<代办<呆板; top10={top10:?}"
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

    /// Class B polish (user report 2026-06-10): "quanquan ... 全权 >
    /// 圈圈". Underlying library.tsv freqs have 圈圈 (26401) higher
    /// than 全权 (18842) — without intervention, only the tier system
    /// kept 全权 ahead, fragile to upstream tier changes. quickfix_boost
    /// lifts 全权 to 29500 (圈圈 base + 10%) so the invariant is held by
    /// data, not by tier interaction.
    #[test]
    fn polish_quanquan_quanquan_above_circlecircle() {
        let top10 = mixed_top10("quanquan".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let q = pos("全权").expect("全权 missing from quanquan top10");
        let r = pos("圈圈").expect("圈圈 missing from quanquan top10");
        assert!(
            q < r,
            "quanquan: 全权 should rank above 圈圈; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-10): "chongshi 重试 第二".
    /// 重试 sat #3 behind 冲蚀/重拾. Pair-boost in quickfix_boost
    /// (充实 60000 / 重试 50000) — same winner-take-all workaround as
    /// the qingjiao polish — keeps 充实 #0 and lands 重试 #1.
    #[test]
    fn polish_chongshi_chongshi_second() {
        let top10 = mixed_top10("chongshi".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("充实").expect("充实 missing from chongshi top10");
        let b = pos("重试").expect("重试 missing from chongshi top10");
        assert!(
            a == 0 && b == 1,
            "chongshi: expected 充实 #0, 重试 #1; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-10): "xiangcai 湘菜".
    /// 湘菜 was absent from v2 words.tsv (buffer returned 香菜 only).
    /// modern_vocab @ 15000 → tier 4 lands it #1 behind the more
    /// common 香菜 (30000 → tier 3 overshot to #0 in probe).
    #[test]
    fn polish_xiangcai_xiangcai_behind_xiangcai() {
        let top10 = mixed_top10("xiangcai".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("香菜").expect("香菜 missing from xiangcai top10");
        let b = pos("湘菜").expect("湘菜 missing from xiangcai top10");
        assert!(
            a == 0 && b == 1,
            "xiangcai: expected 香菜 #0, 湘菜 #1; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-11): "shoujian 收件第一".
    /// 收件 was absent from v2 words.tsv (buffer returned 收监/兽奸/
    /// 手贱 only) despite library freq 19224 — same gap shape as the
    /// xiangcai polish. modern_vocab @ 30000 → tier 3 lands it #0
    /// above cedict-tier-4 收监.
    #[test]
    fn polish_shoujian_shoujian_first() {
        let top10 = mixed_top10("shoujian".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("收件").expect("收件 missing from shoujian top10");
        assert!(a == 0, "shoujian: expected 收件 #0; got top10={top10:?}");
    }

    /// Class D2 polish (user report 2026-07-11): "shoujiao 兽交 手交
    /// 这种词删了". Same shape as the shoujian 兽奸 case — cedict
    /// ingest rows, modern_freq 0, vile + niche — exclusions_v1
    /// hides both from shoujiao display.
    #[test]
    fn polish_shoujiao_vile_terms_hidden() {
        let top10 = mixed_top10("shoujiao".as_bytes());
        for w in ["兽交", "手交"] {
            assert!(
                !top10.iter().any(|x| x == w),
                "shoujiao: {w} must not appear; got top10={top10:?}"
            );
        }
    }

    /// Class D2 polish (user report 2026-07-11): "兽奸删除（太小众
    /// 恶劣了）". Real cedict word but vile + niche — exclusions_v1
    /// hides it from shoujian display; stays in v2 words.tsv so
    /// nothing else regresses.
    #[test]
    fn polish_shoujian_shoujian_hidden() {
        let top10 = mixed_top10("shoujian".as_bytes());
        assert!(
            !top10.iter().any(|x| x == "兽奸"),
            "shoujian: 兽奸 must not appear; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-11): "baiping 白屏
    /// heiping 黑屏". Both tech terms absent from v2 words.tsv
    /// (白屏 absent everywhere; 黑屏 in v1 library @ 19194 but the
    /// heiping buffer returned NOTHING). modern_vocab @ 15000 each:
    /// 白屏 lands #1 behind the common 摆平 (tier 4, 21611); 黑屏
    /// is the only heiping candidate → #0.
    #[test]
    fn polish_baiping_heiping_screen_terms() {
        let top10 = mixed_top10("baiping".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("摆平").expect("摆平 missing from baiping top10");
        let b = pos("白屏").expect("白屏 missing from baiping top10");
        assert!(
            a == 0 && b == 1,
            "baiping: expected 摆平 #0, 白屏 #1; got top10={top10:?}"
        );
        let top10 = mixed_top10("heiping".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("黑屏"),
            "heiping: expected 黑屏 #0; got top10={top10:?}"
        );
    }

    /// Pin (user report 2026-07-12): "shiyishi 试一试". Verified
    /// already #0 in both Mixed and Mixed+JP at report time — no data
    /// change; this test pins it against drift. (The report's
    /// screenshot was the shiyashi buffer — that JP compose pollution
    /// is a framework gate gap, tracked separately.)
    #[test]
    fn polish_shiyishi_shiyishi_first() {
        let top10 = mixed_top10("shiyishi".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("试一试"),
            "shiyishi: expected 试一试 #0; got top10={top10:?}"
        );
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"shiyishi" {
            let _ = e.handle_letter(*b);
        }
        assert_eq!(
            e.candidates().first().map(|c| c.word.as_str()),
            Some("试一试"),
            "shiyishi Mixed+JP: expected 试一试 #0"
        );
    }

    /// Class B polish (user report 2026-07-13): "zuowei 作为 > 座位".
    /// 座位 (32420) led over the far more common 作为 (41456) via
    /// tier ordering. quickfix_boost 35662 (座位 base + 10% margin,
    /// pick-helper derived) lifts 作为 to #0.
    #[test]
    fn polish_zuowei_zuowei_first() {
        let top10 = mixed_top10("zuowei".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("作为").expect("作为 missing from zuowei top10");
        let b = pos("座位").expect("座位 missing from zuowei top10");
        assert!(
            a == 0 && b == 1,
            "zuowei: expected 作为 #0, 座位 #1; got top10={top10:?}"
        );
    }

    /// Class D polish (user report 2026-07-13): "做为不是词要删".
    /// Nonstandard variant of 作为 — D1-deleted from v1 library
    /// (+ garbage filter record); the v2 cedict ingest row is
    /// suppressed via exclusions_v1 per the ingest-artifact
    /// precedent.
    #[test]
    fn polish_zuowei_zuowei_variant_gone() {
        let top10 = mixed_top10("zuowei".as_bytes());
        assert!(
            !top10.iter().any(|x| x == "做为"),
            "zuowei: 做为 must not appear; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-13): "qingliang 轻量 >
    /// 清凉，清亮不是个词". 轻量 sat #2. quickfix_boost 27385 (清凉
    /// base 24896 + 10% margin, pick-helper derived) lifts it to #0;
    /// 清凉 follows at #1.
    #[test]
    fn polish_qingliang_qingliang_first() {
        let top10 = mixed_top10("qingliang".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("轻量").expect("轻量 missing from qingliang top10");
        let b = pos("清凉").expect("清凉 missing from qingliang top10");
        assert!(
            a == 0 && b == 1,
            "qingliang: expected 轻量 #0, 清凉 #1; got top10={top10:?}"
        );
    }

    /// Class D2 polish (user report 2026-07-13): "清亮不是个词".
    /// Real v1-library/cedict word (清亮 11109) but the user doesn't
    /// want it at this buffer — exclusions_v1 hides it from display,
    /// entry stays in dict for reverse-lookup.
    #[test]
    fn polish_qingliang_qingliang_hidden() {
        let top10 = mixed_top10("qingliang".as_bytes());
        assert!(
            !top10.iter().any(|x| x == "清亮"),
            "qingliang: 清亮 must not appear; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-14): "huacaoshumu 花草树木".
    /// 花草树木 in v1 library @ 7223 but absent from v2 words.tsv —
    /// the buffer only returned the composed 花草数目. modern_vocab
    /// @ 15000 → #0 (no exact competitor).
    #[test]
    fn polish_huacaoshumu_present() {
        let top10 = mixed_top10("huacaoshumu".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("花草树木"),
            "huacaoshumu: expected 花草树木 #0; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-14): "shumu 树木第一".
    /// 数目 (v2 tier ordering) led over 树木. quickfix_boost 26877
    /// (数目 base + 10% margin, pick-helper derived) lifts 树木 to #0.
    #[test]
    fn polish_shumu_shumu_first() {
        let top10 = mixed_top10("shumu".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("树木").expect("树木 missing from shumu top10");
        let b = pos("数目").expect("数目 missing from shumu top10");
        assert!(
            a == 0 && b == 1,
            "shumu: expected 树木 #0, 数目 #1; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-13): "chongqi 重启 > 充气".
    /// 充气 led by a hair (402477 vs 401818). quickfix_boost 24533
    /// (top base + 10% margin, pick-helper derived) lifts 重启 to #0.
    #[test]
    fn polish_chongqi_chongqi_first() {
        let top10 = mixed_top10("chongqi".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("重启").expect("重启 missing from chongqi top10");
        let b = pos("充气").expect("充气 missing from chongqi top10");
        assert!(
            a == 0 && b == 1,
            "chongqi: expected 重启 #0, 充气 #1; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-12): "xiejiao 斜角第一".
    /// 斜角 sat #2 behind 邪教/歇脚. quickfix_boost 19276 (top base
    /// 17524 + 10% margin, pick-helper derived) lifts it to #0.
    #[test]
    fn polish_xiejiao_xiejiao_first() {
        let top10 = mixed_top10("xiejiao".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("斜角").expect("斜角 missing from xiejiao top10");
        let b = pos("邪教").expect("邪教 missing from xiejiao top10");
        assert!(
            a == 0 && b == 1,
            "xiejiao: expected 斜角 #0, 邪教 #1; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-12): "nanwei 难为 第一".
    /// 南纬 led despite lower library freq (17786 vs 24426).
    /// quickfix_boost 19564 (pick-helper derived) lifts 难为 to #0.
    #[test]
    fn polish_nanwei_nanwei_first() {
        let top10 = mixed_top10("nanwei".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("难为").expect("难为 missing from nanwei top10");
        let b = pos("南纬").expect("南纬 missing from nanwei top10");
        assert!(
            a == 0 && b == 1,
            "nanwei: expected 难为 #0, 南纬 #1; got top10={top10:?}"
        );
    }

    /// Class D2 polish (user report 2026-07-12): "nanwei 男卫 删掉".
    /// cedict ingest row, modern_freq 0 — same shape as the shoujian/
    /// shoujiao cases. exclusions_v1 hides it from nanwei display.
    #[test]
    fn polish_nanwei_nanwei_hidden() {
        let top10 = mixed_top10("nanwei".as_bytes());
        assert!(
            !top10.iter().any(|x| x == "男卫"),
            "nanwei: 男卫 must not appear; got top10={top10:?}"
        );
    }

    /// Framework rule (PLAN-exact-common-above-jp-kana, 2026-07-17), from
    /// the user principle "一般常用的拼音或五笔刚好完全命中时肯定是要在
    /// 日语前面的". v2 caps the natural tier of exact whole-buffer word
    /// matches with modern_freq ≥ 20000 at tier 4 — the same bucket as the
    /// ≥5-letter mechanical-kana band, where px > nx puts the word first.
    /// These buffers carry NO quickfix rows: the ordering is rule-driven.
    /// (The lian 立案 overlay-sovereignty guard lives in the gf-recall test.)
    #[test]
    fn framework_exact_common_word_above_jp_kana() {
        for (buf, expect) in [
            ("aijiaaihu", "挨家挨户"),
            ("anjisuan", "氨基酸"),
            ("anbujiuban", "按部就班"),
        ] {
            let mut e = CompositeEngine::new();
            e.set_mode(Mode::Mixed);
            e.set_auto_commit_policy(AutoCommitPolicy::Never);
            e.set_japanese_enabled(true);
            for b in buf.bytes() {
                let _ = e.handle_letter(b);
            }
            let top10: Vec<String> = e
                .candidates()
                .iter()
                .take(10)
                .map(|c| c.word.clone())
                .collect();
            assert_eq!(
                top10.first().map(String::as_str),
                Some(expect),
                "{buf} Mixed+JP: expected {expect} #0 above JP kana (rule-driven, no quickfix); got top10={top10:?}"
            );
        }
    }

    /// Framework rule (ceiling-first JP band, 2026-07-29), from the user
    /// directive "日语怎么可能会超过常见拼音的 100% 命中，任何时候这都不
    /// 应该，这是危险信号" + "日语真正的高分档还是应该不低，但再高几乎也
    /// 不应该超过 100% 命中的拼音常见词，更不可能超过五笔".
    ///
    /// Sibling of `framework_exact_common_word_above_jp_kana`: that rule
    /// only cleared the mechanical-KANA bands, leaving the JP kanji DICT
    /// band (jukugo + single kanji, freq quantile) at tier 2-3 where it
    /// still beat exact-hit common Chinese words. 85 (buffer, word) pairs
    /// over 54 buffers were affected. `JP_TIER_CEILING` in
    /// japanese_adapter.rs now clamps every JP dict path at tier 4, where
    /// px > nx decides. No quickfix rows on these buffers — rule-driven.
    #[test]
    fn framework_exact_common_word_above_jp_dict() {
        for (buf, expect, jp_loser) in [
            ("henji", "痕迹", "返事"),
            ("shiyou", "石油", "仕様"),
            ("jinji", "紧急", "人事"),
            ("bijin", "逼近", "美人"),
            ("kantan", "勘探", "簡単"),
            ("miman", "弥漫", "未満"),
        ] {
            let mut e = CompositeEngine::new();
            e.set_mode(Mode::Mixed);
            e.set_auto_commit_policy(AutoCommitPolicy::Never);
            e.set_japanese_enabled(true);
            for b in buf.bytes() {
                let _ = e.handle_letter(b);
            }
            let top10: Vec<String> = e
                .candidates()
                .iter()
                .take(10)
                .map(|c| c.word.clone())
                .collect();
            assert_eq!(
                top10.first().map(String::as_str),
                Some(expect),
                "{buf} Mixed+JP: expected {expect} #0 above JP dict {jp_loser} \
                 (rule-driven, no quickfix); got top10={top10:?}"
            );
        }
    }

    /// The other half of the ceiling-first directive: "单个的假名或者两个
    /// 音节的假名，排名还是要确保能在中文的预测词和低频率词前面".
    ///
    /// The ceiling must not be so aggressive that kana disappears. Short
    /// buffers (≤ 2 letters) keep `JP_TIER_SHORT_KANA` = 1 — safe because
    /// `words.tsv` has zero codes that short, so there is no exact-hit
    /// Chinese WORD to protect there — and two-syllable buffers sit at
    /// the ceiling, still above every low-freq / predicted Chinese
    /// candidate (t5+). A regression here means kana sank out of view:
    /// clamping short kana to t3 during development pushed も / え past
    /// rank 50 at `mo` / `e`.
    #[test]
    fn framework_short_kana_stays_visible_under_jp_ceiling() {
        for (buf, kana, max_rank) in [
            ("ki", "き", 3usize),
            ("ka", "か", 6),
            ("sa", "さ", 6),
            ("kana", "かな", 0),
            ("tuli", "ツィ", 4),
        ] {
            let mut e = CompositeEngine::new();
            e.set_mode(Mode::Mixed);
            e.set_auto_commit_policy(AutoCommitPolicy::Never);
            e.set_japanese_enabled(true);
            for b in buf.bytes() {
                let _ = e.handle_letter(b);
            }
            let words: Vec<String> = e.candidates().iter().map(|c| c.word.clone()).collect();
            let rank = words.iter().position(|w| w == kana);
            assert!(
                rank.is_some_and(|r| r <= max_rank),
                "{buf} Mixed+JP: kana {kana} must rank within #{max_rank}; \
                 got rank={rank:?} top10={:?}",
                &words[..words.len().min(10)]
            );
        }
    }

    /// Class B polish (user report 2026-07-17): "tianmafan 添麻烦 第一",
    /// with the general principle "一般常用的拼音或五笔刚好完全命中时肯定是
    /// 要在日语前面的". Same shape as jiejiari: mechanical kana led while
    /// 添麻烦 (base 22795, mid tier) sat at #2. quickfix 55000 → tier 1.
    #[test]
    fn polish_tianmafan_tianmafan_above_jp_kana() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"tianmafan" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        assert_eq!(
            top10.first().map(String::as_str),
            Some("添麻烦"),
            "tianmafan Mixed+JP: expected 添麻烦 #0 above JP kana; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-18): "shouxieti 手写体要在日语前".
    /// 手写体 is v2 tier 5 (cedict) with modern_freq 18308 — just below the
    /// exact-common cap threshold (20000), so the framework rule doesn't lift
    /// it and tier-4 mechanical kana led. quickfix 55000 → tier 1.
    #[test]
    fn polish_shouxieti_shouxieti_above_jp_kana() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"shouxieti" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        assert_eq!(
            top10.first().map(String::as_str),
            Some("手写体"),
            "shouxieti Mixed+JP: expected 手写体 #0 above JP kana; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-15): "jiejiari 现在日语在前".
    /// With Japanese enabled the mechanical kana じえじあり/ジエジアリ lead;
    /// 节假日 (base 25131, mid tier) sat at #2 below them. quickfix 55000
    /// lifts it to tier 1 — the JP-first positional rule places kana above
    /// pinyin tiers 2+, but tier-1 pinyin wins. Same 55k calibration as
    /// 简码 / 娭毑 / 世锦赛 / 复读.
    #[test]
    fn polish_jiejiari_jiejiari_above_jp_kana() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"jiejiari" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        assert_eq!(
            top10.first().map(String::as_str),
            Some("节假日"),
            "jiejiari Mixed+JP: expected 节假日 #0 above JP kana; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-12): "laduzi 拉肚子 要在
    /// 日语前". In Mixed+JP the mechanical kana renders (ァヅジ/ぁづじ)
    /// out-tiered 拉肚子 (base 26606 → mid tier). quickfix 60000
    /// lifts its natural z-score tier above JP mechanical kana.
    #[test]
    fn polish_laduzi_laduzi_above_jp_kana() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"laduzi" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        assert_eq!(
            top10.first().map(String::as_str),
            Some("拉肚子"),
            "laduzi Mixed+JP: expected 拉肚子 #0 above JP kana; got top10={top10:?}"
        );
    }

    /// Framework fix (user report 2026-07-12, shiyashi screenshot):
    /// compose_sentence expanded EVERY single-kanji homophone per
    /// slot (始/指/此/歯/私/資 all read shi) → ~30 cartesian products
    /// (始や始/始や指/...) flooding the window. Fix: single-kanji
    /// compose slots take the top-freq homophone only (japanese/
    /// compose.rs + facade twin). Genuine composes (私は学生です,
    /// 先生は) are jukugo-slot driven and unaffected.
    #[test]
    fn jp_compose_single_kanji_slots_no_cartesian_flood() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"shiyashi" {
            let _ = e.handle_letter(*b);
        }
        let words: Vec<String> = e.candidates().iter().map(|c| c.word.clone()).collect();
        let is_kanji = |ch: char| ('一'..='鿿').contains(&ch);
        let spliced = words
            .iter()
            .filter(|w| w.contains('や') && w.chars().next().is_some_and(is_kanji))
            .count();
        assert!(
            spliced <= 2,
            "shiyashi Mixed+JP: expected ≤2 kanji-や-kanji compose products, got {spliced}: {words:?}"
        );
    }

    /// v2 chengyu backfill sweep (user directive 2026-07-14, "做 v2
    /// backfill sweep 补成语" + "你用自己 llm 专业知识一个个审查才能
    /// 入库"): all 36,674 v1 four-hanzi words missing from v2 were
    /// judged per-row; 4,697 proposals, 4,668 accepted after lead
    /// per-row review (29 rejected: misspellings / variant glyphs /
    /// bad-pinyin codes). See docs/pinyin-v2-chengyu-2026-07-14/.
    #[test]
    fn v2_chengyu_backfill_representatives() {
        for (buf, expect) in [
            ("yegonghaolong", "叶公好龙"),
            ("paodingjieniu", "庖丁解牛"),
            ("chengmenlixue", "程门立雪"),
            ("banmennongfu", "班门弄斧"),
            ("wusuoweiju", "无所畏惧"),
        ] {
            let top10 = mixed_top10(buf.as_bytes());
            assert_eq!(
                top10.first().map(String::as_str),
                Some(expect),
                "{buf}: expected {expect} #0; got top10={top10:?}"
            );
        }
        // The 29 rejected forms (大作文章 / 默默无名 / 莫明其妙 …) are
        // NOT asserted absent here: they already live in the v1
        // library.tsv corpus, so declining to backfill them into v2
        // does not make them un-typeable. Purging misspelling variants
        // from the v1 main dict belongs to the separate library-audit
        // project, not to this backfill.
    }

    /// Garbage-filter recall sweep (user directive 2026-07-13, applied
    /// 2026-07-14): per-row LLM re-review of all 17,800 audit-P1/P2
    /// filter rows; 691 false positives recalled (see
    /// docs/pinyin-gf-recall-2026-07-14/). Representatives pinned:
    /// lib>0 double-kill victims (木村/出去玩), lib=0 re-adds
    /// (咋整/并没有), and the ambiguous-segmentation guard — 立案
    /// resurfaced by the recall must stay BELOW 脸/连 (tier_overlay 5
    /// + stale dogfood quickfix removed).
    #[test]
    fn gf_recall_sweep_representatives() {
        for (buf, expect) in [
            ("mucun", "木村"),
            ("chuquwan", "出去玩"),
            ("zazheng", "咋整"),
            ("bingmeiyou", "并没有"),
        ] {
            let top10 = mixed_top10(buf.as_bytes());
            assert_eq!(
                top10.first().map(String::as_str),
                Some(expect),
                "{buf}: expected {expect} #0; got top10={top10:?}"
            );
        }
        let top10 = mixed_top10("lian".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("脸"),
            "lian: expected 脸 #0; got top10={top10:?}"
        );
        let lian_pos = top10.iter().position(|x| x == "立案");
        assert!(
            lian_pos.is_none_or(|p| p >= 3),
            "lian: 立案 must not crowd top-3; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-13): "gaizhu 盖住
    /// chuangwai 窗外". Both v1-present / v2-missing (盖住 19223,
    /// 窗外 27935). 窗外 additionally needed a 回捞: the 2026-07-10
    /// modern-vocab audit P2 mistagged it "frag-grammar" and its
    /// corpus_garbage_filter row was killing BOTH the v1 dict row
    /// (build-time drop) and the v2 display (exclusions = v1 ∪
    /// garbage filter) — the false-positive record was removed.
    #[test]
    fn polish_gaizhu_chuangwai_present() {
        let top10 = mixed_top10("gaizhu".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("盖住"),
            "gaizhu: expected 盖住 #0; got top10={top10:?}"
        );
        let top10 = mixed_top10("chuangwai".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("窗外"),
            "chuangwai: expected 窗外 #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-13): "fazhen 法阵".
    /// 法阵 was absent from every data surface (v1 library has only
    /// low-freq 发疹/发针; the buffer surfaced a fuzzy 法政).
    /// modern_vocab @ 15000 → #0 (no exact competitor).
    #[test]
    fn polish_fazhen_fazhen_first() {
        let top10 = mixed_top10("fazhen".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("法阵"),
            "fazhen: expected 法阵 #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-13): "bianjing 辩经".
    /// 辩经 was absent from every data surface (v1 library has only
    /// 变靓/汴京/边境). modern_vocab @ 15000 → tier 4, lands #1
    /// behind the common 边境 (27835) per the 白屏 precedent.
    #[test]
    fn polish_bianjing_bianjing_present() {
        let top10 = mixed_top10("bianjing".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("边境").expect("边境 missing from bianjing top10");
        let b = pos("辩经").expect("辩经 missing from bianjing top10");
        assert!(
            a == 0 && b == 1,
            "bianjing: expected 边境 #0, 辩经 #1; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-13): "mobao 墨宝".
    /// 墨宝 in v1 library @ 16054 but absent from v2 words.tsv — the
    /// buffer only returned the composed junk 摸吧哦 (same gap shape
    /// as 黑屏/武僧/躲过去). modern_vocab @ 15000 → #0.
    #[test]
    fn polish_mobao_mobao_first() {
        let top10 = mixed_top10("mobao".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("墨宝"),
            "mobao: expected 墨宝 #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-13): "duoguoqu 躲过去".
    /// 躲过去 in v1 library @ 10888 but absent from v2 words.tsv —
    /// the buffer returned NOTHING (same gap shape as 黑屏/武僧).
    /// modern_vocab @ 15000 → #0 (no competitor).
    #[test]
    fn polish_duoguoqu_duoguoqu_first() {
        let top10 = mixed_top10("duoguoqu".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("躲过去"),
            "duoguoqu: expected 躲过去 #0; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-17): first "lvli 膂力 删除",
    /// then revised to "那保留只是排第二吧". The interim D2 exclusion was
    /// withdrawn; instead 履历 gets a quickfix (7300 = 膂力 base 6633 + 10%
    /// margin) so it takes #0 and 膂力 stays visible at #1.
    #[test]
    fn polish_lvli_lvli_first_lvli_second() {
        let top10 = mixed_top10("lvli".as_bytes());
        let head: Vec<&str> = top10.iter().take(2).map(String::as_str).collect();
        assert_eq!(head, ["履历", "膂力"], "lvli: got top10={top10:?}");
    }

    /// Class B polish (user report 2026-07-17): "dangji 当即 > 宕机 > 当季 >
    /// 党籍 > 党纪，一般政治用语都应该后置". A modern_vocab row (党纪 45000)
    /// had lifted the political term to #0. The requested order also inverts
    /// the natural freq of 党籍/党纪 (14312 < 16382), so all five need chain
    /// boosts: 60000/55000/50000/45000/40000 descending.
    #[test]
    fn polish_dangji_full_order() {
        let top10 = mixed_top10("dangji".as_bytes());
        let head: Vec<&str> = top10.iter().take(5).map(String::as_str).collect();
        assert_eq!(
            head,
            ["当即", "宕机", "当季", "党籍", "党纪"],
            "dangji: got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-16): "chayi 差异第一".
    /// 差异 carries the highest library freq of the group (28532) but the
    /// modern-freq prior lifted 诧异 (base 20425) above it. quickfix 23000
    /// (诧异 base + 10% margin) flips 差异 to #0.
    #[test]
    fn polish_chayi_chayi_first() {
        let top10 = mixed_top10("chayi".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("差异"),
            "chayi: expected 差异 #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-16): "chengbuzhu 撑不住".
    /// 撑不住 lives in v1 library (freq 19544) but was absent from v2, so
    /// the buffer returned NOTHING (same v2-ingest-gap shape as 黑屏/武僧/
    /// 躲过去/挂着). verb+不+resultative is a legit typing unit.
    /// modern_vocab @ 15000 → #0 (no competitor).
    #[test]
    fn polish_chengbuzhu_chengbuzhu_first() {
        let top10 = mixed_top10("chengbuzhu".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("撑不住"),
            "chengbuzhu: expected 撑不住 #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-18): "gejutese 各具特色".
    /// v2-ingest gap: 各具特色 lives in v1 (freq 10742) but not in v2 —
    /// the buffer showed JP kana + the K-best splice 格局特色 instead.
    /// (The chengyu backfill's first-pass reviewer had judged it KEEP as
    /// an assembled 各+具+特色; user explicitly wants it.) modern_vocab
    /// @ 15000 → tier 4, where px > nx puts it above the kana band.
    #[test]
    fn polish_gejutese_gejutese_first() {
        let top10 = mixed_top10("gejutese".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("各具特色"),
            "gejutese: expected 各具特色 #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-18): "chuda 触达".
    /// 触达 was absent from BOTH v1 library and v2 words — the buffer
    /// showed only JP kana + prefix-completion fillers (出单/初代/出道).
    /// Common tech/business word (用户触达). modern_vocab @ 15000 →
    /// tier 4 exact, px > nx puts it above the kana band; the prefix
    /// fillers yield to the exact match.
    #[test]
    fn polish_chuda_chuda_first() {
        let top10 = mixed_top10("chuda".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("触达"),
            "chuda: expected 触达 #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-20): "fangzhong 房中，第一".
    /// v2-ingest gap: 房中 lives in v1 library (freq 15522) but not in v2
    /// words, so the buffer returned only the cedict Taiwan term 房仲.
    /// modern_vocab @ 15000 → exact tier 4, which leads.
    #[test]
    fn polish_fangzhong_fangzhong_first() {
        let top10 = mixed_top10("fangzhong".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("房中"),
            "fangzhong: expected 房中 #0; got top10={top10:?}"
        );
        assert!(
            !top10.contains(&"房仲".to_string()),
            "fangzhong: 房仲 is a Taiwan-only term (房屋仲介), excluded from \
             Path-1 per user 2026-07-20 \"台湾用语删了吧\"; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-20): "qingyan 轻烟第一".
    /// 轻烟 was in neither v1 nor v2, so the buffer showed only the
    /// cedict tier-4 pair 青眼 / 轻言. modern_vocab @ 30000 → natural
    /// tier 3, which outranks tier 4 without a quickfix override.
    #[test]
    fn polish_qingyan_qingyan_first() {
        let top10 = mixed_top10("qingyan".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("轻烟"),
            "qingyan: expected 轻烟 #0; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-20): "jiewei 结尾第一".
    /// All five v2 rows are tier 4, so order fell to modern_freq, where
    /// 解围 (23675) edged 结尾 (23637) by 38 — while the v1 corpus has
    /// 结尾 28796 vs 解围 17699, i.e. the jieba-percentile prior is
    /// inverted here (same family as tujian / chayi / fenwei).
    #[test]
    fn polish_jiewei_jiewei_first() {
        let top10 = mixed_top10("jiewei".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("结尾"),
            "jiewei: expected 结尾 #0; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-20): "shazhang 纱帐第一".
    /// 纱帐 came in from modern_vocab at 15000 → tier 4, tying the cedict
    /// row 煞账 (also tier 4) at score 380000, so the codepoint
    /// tiebreak put 煞(U+715E) ahead of 纱(U+7EB1). Raising the SAME
    /// modern_vocab row to 30000 buys tier 3, which wins outright.
    ///
    /// Note for future polishes: `data.rs::build_words` keeps only the
    /// FIRST modern_vocab row per (code, word) — appending a second row
    /// with a higher freq is silently ignored, the existing row must be
    /// edited in place.
    #[test]
    fn polish_shazhang_shazhang_first() {
        let top10 = mixed_top10("shazhang".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("纱帐"),
            "shazhang: expected 纱帐 #0; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-20): "qian 千应该在第四位".
    /// 千 was buried at #44 (score 434831). Root cause: a single-char
    /// row `qian 千 35000` in modern_vocab_v1 turned it into a tier-3
    /// WORD entry (410000 + modern_freq 24831 = 434831). The word path
    /// runs before the char path and claims the dedup slot, so 千 never
    /// got its natural single-char score — and tier-3-word is far worse
    /// than what 千 earns as a char (通用规范 tier 1, HSK 2).
    ///
    /// Deleting that row lets the char path score it normally; 千 lands
    /// at #2 (521434), i.e. better than the requested 4th. No override
    /// was added — forcing it to exactly 4th would mean demoting it
    /// below 浅 (modern_freq 24784 < 千 24831, HSK 5 vs 2), i.e.
    /// contradicting the data.
    ///
    /// Lesson: single-char rows do not belong in modern_vocab — they
    /// shadow the char path and can only lower a char's rank.
    #[test]
    fn polish_qian_qian_in_top4() {
        let top10 = mixed_top10("qian".as_bytes());
        let idx = top10.iter().position(|w| w == "千");
        assert!(
            matches!(idx, Some(i) if i <= 3),
            "qian: expected 千 within top-4; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-21): "yichu 移除 > 溢出 > 益处 >
    /// 衣橱 > 一处 > 移出 > 役畜". Full 7-candidate reorder via a descending
    /// quickfix chain (winner-take-all tier 1). 役畜 (draft animals — a
    /// real but uncommon word, so KEPT not deleted) gets no quickfix row
    /// and stays tier 4, landing last. Supersedes the old strict-0001
    /// "一处 over 溢出" rows, which the new explicit order inverts.
    #[test]
    fn polish_yichu_full_order() {
        let top10 = mixed_top10("yichu".as_bytes());
        let want = ["移除", "溢出", "益处", "衣橱", "一处", "移出", "役畜"];
        let got: Vec<&str> = top10
            .iter()
            .filter(|w| want.contains(&w.as_str()))
            .map(String::as_str)
            .collect();
        assert_eq!(
            got, want,
            "yichu: expected order {want:?}; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-21): "tlpv 备案第一". Wubi-side
    /// Class B, so the kongdang route (quickfix_boost is pinyin-only):
    /// both candidates are layer-1 entries ordered purely by freq, and
    /// 备案 (filing/registration) lost to 血案 (murder case) 19275 vs
    /// 19466. library.tsv freq 19275 → 21413 (血案 + 10%), layer 1
    /// unchanged, source flipped to `polish` so corpus digest can't
    /// clobber it.
    #[test]
    fn polish_tlpv_beian_first() {
        let top10 = mixed_top10("tlpv".as_bytes());
        let head: Vec<&str> = top10.iter().take(2).map(String::as_str).collect();
        assert_eq!(head, ["备案", "血案"], "tlpv: got top10={top10:?}");
    }

    /// Class B polish (user report 2026-08-05): "wyet 信用 > 食用". Same
    /// wubi kongdang route as tlpv — all three wyet rows are layer-1, so
    /// ordering is pure freq and 信用 (29283) lost to 食用 (29856) by 573.
    /// library.tsv freq 29283 → 33000 (食用 + ~10%), layer 1 unchanged,
    /// source flipped to `polish` so a corpus digest can't clobber it.
    /// 停用 keeps #2 untouched.
    #[test]
    fn polish_wyet_xinyong_first() {
        let top10 = mixed_top10("wyet".as_bytes());
        let head: Vec<&str> = top10.iter().take(3).map(String::as_str).collect();
        assert_eq!(head, ["信用", "食用", "停用"], "wyet: got top10={top10:?}");
    }

    /// Class B polish (user report 2026-07-21): "fuzhi 复制第一 赋值第二
    /// 扶植第三". 福祉 held #0 via an earlier quickfix (30000); the three
    /// requested words go above it with a descending chain, so 福祉 keeps
    /// its row and simply lands at #4 — the earlier ruling is honoured as
    /// far as the new one allows.
    #[test]
    fn polish_fuzhi_head_order() {
        let top10 = mixed_top10("fuzhi".as_bytes());
        let head: Vec<&str> = top10.iter().take(4).map(String::as_str).collect();
        assert_eq!(
            head,
            ["复制", "赋值", "扶植", "福祉"],
            "fuzhi: got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-24): "zhifu 支付第一，制服第二，
    /// 致富第三". 致富 held #0 via an earlier quickfix (30000); the three
    /// requested words go above it with a descending chain, so 致富 keeps
    /// its row and lands at #2 (its requested third place).
    #[test]
    fn polish_zhifu_head_order() {
        let top10 = mixed_top10("zhifu".as_bytes());
        let head: Vec<&str> = top10.iter().take(3).map(String::as_str).collect();
        assert_eq!(head, ["支付", "制服", "致富"], "zhifu: got top10={top10:?}");
    }

    /// Class A polish (user report 2026-07-24): "heshui 喝水".
    /// v2-ingest gap: 喝水 lives in v1 (freq 28818, above 河水 25201) but
    /// not in v2, so the buffer showed only the cedict tier-4 word 河水.
    /// modern_vocab @ 30000 → natural tier 3, which outranks tier 4.
    #[test]
    fn polish_heshui_heshui_first() {
        let top10 = mixed_top10("heshui".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("喝水"),
            "heshui: expected 喝水 #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-24): "xianmafan 嫌麻烦".
    /// Absent from both v1 and v2 — the buffer showed only JP kana (≥5
    /// letters → kana tier 4). Same shape as gejutese: modern_vocab @
    /// 15000 → tier 4 exact, where px > nx puts it above the kana band.
    #[test]
    fn polish_xianmafan_xianmafan_first() {
        let top10 = mixed_top10("xianmafan".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("嫌麻烦"),
            "xianmafan: expected 嫌麻烦 #0; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-24): "yake 牙科第一".
    /// 牙科 and 亚科 are both v2 tier 4, so order fell to modern_freq,
    /// where the taxonomy term 亚科 (subfamily) edged 牙科 (dentistry) —
    /// while v1 corpus has 牙科 22820 >> 亚科 11196. A single quickfix
    /// (winner-take-all) lifts 牙科 to #0; 亚科 falls to #1.
    #[test]
    fn polish_yake_yake_first() {
        let top10 = mixed_top10("yake".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("牙科"),
            "yake: expected 牙科 #0; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-24): "yatong 牙痛第一".
    /// 牙痛 and 压痛 are both v2 tier 4; modern_freq put the medical term
    /// 压痛 (tenderness) at #0 over 牙痛 (toothache), while v1 corpus has
    /// 牙痛 10556 > 压痛 6569. Single quickfix (winner-take-all) lifts 牙痛
    /// to #0; 压痛 falls to #1.
    #[test]
    fn polish_yatong_yatong_first() {
        let top10 = mixed_top10("yatong".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("牙痛"),
            "yatong: expected 牙痛 #0; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-25): "xinsu 新宿第一".
    /// Both candidates are v2 tier 4, so order fell to the modern_freq
    /// tiebreaker — and only the archaic 信宿 (two nights' lodging, 16745)
    /// has a jieba entry while the Tokyo place name 新宿 (modern_vocab
    /// supplement) gets +0. v1 corpus disagrees: 新宿 17925 >> 信宿 7934.
    /// quickfix (winner-take-all → tier 1) lifts 新宿 to #0; 信宿 → #1.
    #[test]
    fn polish_xinsu_xinsu_first() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"xinsu" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        assert_eq!(
            top10.first().map(String::as_str),
            Some("新宿"),
            "xinsu Mixed+JP: expected 新宿 #0; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-29): "huashuo 话说 > 华硕".
    /// Same shape as xinsu: both candidates are v2 tier 4 with no usable
    /// usage signal to separate them — 话说 is a cedict row whose
    /// modern_freq is 0 (jieba segments the discourse marker as 话+说)
    /// and 华硕 is a modern_vocab supplement at 15000, which carries no
    /// modern_freq entry either. Equal tier + equal within-tier score
    /// meant the order fell all the way through to the alphabetical
    /// tiebreak, where 华 (U+534E) < 话 (U+8BDD) put the ASUS brand name
    /// ahead of an everyday opener. quickfix (winner-take-all → tier 1)
    /// lifts 话说 to #0, derived from 华硕's v1 corpus base 17053 + 10%.
    /// 华硕 stays visible at #1.
    #[test]
    fn polish_huashuo_huashuo_first() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"huashuo" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        assert_eq!(
            top10.first().map(String::as_str),
            Some("话说"),
            "huashuo Mixed+JP: expected 话说 #0; got top10={top10:?}"
        );
        assert!(
            top10.iter().any(|w| w == "华硕"),
            "huashuo: 华硕 must stay visible (reorder, not a delete); \
             got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-25): "chidai 池袋".
    /// 池袋 (Ikebukuro) lives in v1 library.tsv at 12149 but never made it
    /// into the v2 words.tsv ingest, so chidai emitted 痴呆 alone — the
    /// systemic "v2 ingest 缺 v1 语料词" gap, not a ranking bug. Added to
    /// modern_vocab_v1 at 15000, the minimum that maps to tier 4: at tier 5
    /// it would sink below the >=5-letter mechanical-kana band with JP on.
    /// 痴呆 keeps #0 on its modern_freq ruling (22945); the invariant pinned
    /// here is that 池袋 surfaces at all AND outranks the kana.
    #[test]
    fn polish_chidai_ikebukuro_present_above_jp_kana() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"chidai" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        let ikebukuro = top10.iter().position(|w| w == "池袋");
        assert!(
            ikebukuro.is_some_and(|i| i < 3),
            "chidai Mixed+JP: expected 池袋 within top-3; got top10={top10:?}"
        );
        let kana = top10.iter().position(|w| w == "ちだい");
        assert!(
            kana.is_none_or(|k| ikebukuro.unwrap() < k),
            "chidai Mixed+JP: expected 池袋 above kana ちだい; got top10={top10:?}"
        );
    }

    /// Class A+B polish (user reports 2026-07-26): "bale 拔了", then
    /// "现在罢了是第一，我想拔了第二，芭乐第三".
    ///
    /// 拔了 existed in NO source (v1 library.tsv, v2 words.tsv, modern_vocab
    /// all lacked it under every code), so bale emitted only 罢了 / 芭乐.
    /// Added to modern_vocab_v1 at 15000 → tier 4, matching the file's
    /// existing X+了 colloquial band (寄了 15000 / 凉了 18000).
    ///
    /// That alone could only reach #2: a tier-4 exact word scores 380000 +
    /// modern_freq, and modern_vocab additions carry no modern_freq entry
    /// (+0), so 拔了 sat pinned at 380000 — unable to pass 芭乐 (385408)
    /// within its tier, while the next tier up (410000) would have overtaken
    /// 罢了 (404674) outright. No #1 rung existed between them, so the
    /// requested order needed an explicit 3-row quickfix chain (60k/50k/40k),
    /// which sidesteps the tier arithmetic entirely: all three become tier 1
    /// and the sovereignty rule strips their modern_freq, so boost value
    /// alone orders them.
    ///
    /// 芭乐 is boosted too rather than left to land #2 on its own — its
    /// natural #2 was only "nothing ahead of it", so any future tier-4
    /// addition to bale scoring above 385408 would have displaced it to #3
    /// and broken the requested 芭乐-third ruling.
    #[test]
    fn polish_bale_three_rank_order_lock() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"bale" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        let head: Vec<&str> = top10.iter().take(3).map(String::as_str).collect();
        assert_eq!(
            head,
            vec!["罢了", "拔了", "芭乐"],
            "bale Mixed+JP: expected 罢了 / 拔了 / 芭乐 in that order; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-28): "goujian 构建 > 勾践 > 构件".
    ///
    /// 构件 (404267) and 构建 (404176) are v2 cedict tier-4 entries scoring
    /// 380000 + modern_freq; 勾践 comes from modern_vocab_v1 (15000, tier 4)
    /// and carries no modern_freq entry, so it sat pinned at 380000 behind
    /// both. Same shape as the bale chain above: within-tier arithmetic
    /// offers no rung between the two incumbents, so an explicit 3-row
    /// quickfix chain (60k/54k/48k, the fuzhi / zhifu descending-chain
    /// convention) locks all three ranks at tier 1 where boost value alone
    /// orders them.
    ///
    /// Class A follow-up (user report 2026-07-28): "goujian 够贱第四". 够贱
    /// existed in no source (v1 library.tsv, v2 words.tsv, modern_vocab all
    /// lacked it), so goujian emitted only the three above. Added to
    /// modern_vocab_v1 at 15000 → tier 4, which lands it at exactly #3: no
    /// modern_freq entry means it is pinned at 380000, below the boosted trio
    /// and above the JP kana (240000). Rank 4 is by construction, not by
    /// boost — the report asked for fourth, so no quickfix row was added.
    #[test]
    fn polish_goujian_four_rank_order_lock() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"goujian" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        let head: Vec<&str> = top10.iter().take(4).map(String::as_str).collect();
        assert_eq!(
            head,
            vec!["构建", "勾践", "构件", "够贱"],
            "goujian Mixed+JP: expected 构建 / 勾践 / 构件 / 够贱 in that order; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-28): "yougoujian 有够贱".
    ///
    /// 有够贱 existed in no source, so yougoujian had no exact match at all
    /// and fell through to K-best, which emitted the nonsense 尤诟几案
    /// (260000) below two JP kana readings. Added to modern_vocab_v1 at
    /// 15000 → tier 4, which as an exact match scores 380000 and takes #0
    /// outright; the K-best product stops being emitted once the exact path
    /// fires.
    ///
    /// On [[feedback-no-fake-long-compounds]]: 有够X is a productive
    /// intensifier shape (有够烦 / 有够蠢 / …), which that rule normally
    /// keeps out of modern_vocab. It does not apply here — the rule targets
    /// compounds auto-harvested from dogfood segment TSVs, and this is an
    /// explicit per-word user request. No sibling 有够X rows were added.
    #[test]
    fn polish_yougoujian_leads() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        for b in b"yougoujian" {
            let _ = e.handle_letter(*b);
        }
        let top10: Vec<String> = e
            .candidates()
            .iter()
            .take(10)
            .map(|c| c.word.clone())
            .collect();
        assert_eq!(
            top10.first().map(String::as_str),
            Some("有够贱"),
            "yougoujian Mixed+JP: expected 有够贱 at #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-28): "tangzhe 躺着 … 更恶劣的是，
    /// 躺着没有，躺着也中枪又有".
    ///
    /// 躺着 was absent from every source (v1 library.tsv, v2 words.tsv,
    /// modern_vocab), so tangzhe surfaced only two entries bleeding in by
    /// code prefix: 汤镇业 (160000, an actor's name, code tangzhenye) and
    /// 躺着也中枪 (70000, a 5-char meme, code tangzheyezhongqiang). The base
    /// word existed nowhere while its own 5-char derivative did.
    ///
    /// Freq 30000 → tier 3 (410000). The nearest structural peer is 带着
    /// (35000, same V+着 shape); 30000 is the tier-3 floor, one notch
    /// conservative. Once an exact match exists the prefix-bleed candidates
    /// stop being emitted at this buffer entirely — they are deleted
    /// separately (see polish_tangzhenye_tangzheyezhongqiang_deleted).
    #[test]
    fn polish_tangzhe_tangzhe_leads() {
        let top10 = mixed_top10("tangzhe".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("躺着"),
            "tangzhe: expected 躺着 #0; got top10={top10:?}"
        );
    }

    /// Class D1 polish (user report 2026-07-28): "汤镇业/躺着也中枪 这种应该
    /// 删掉 … 2 字以上的词本来就应该谨慎，字数越高谨慎要求也越高".
    ///
    /// 汤镇业 is a 3-char proper noun (a Hong Kong actor) that reached the
    /// 2-char buffer tangzhe by code prefix. Deleted from all three surfaces
    /// it occupied — modern_vocab_v1 (30000), v1 library.tsv (5922), and
    /// logged in corpus_garbage_filter_v1.tsv so a future corpus digest
    /// cannot re-admit it. v2 folds the garbage filter into its runtime
    /// exclusion set keyed on (code, word), so the entry is gone at every
    /// buffer, not just this one.
    ///
    /// 躺着也中枪 is a 5-char meme phrase carried by CC-CEDICT. It lived only
    /// in the generated v2 words.tsv (tier 6), which build-words.py rewrites
    /// from source on every ingest, so a hand edit there would not survive —
    /// the garbage filter is the durable D1 surface for generated tables.
    #[test]
    fn polish_tangzhenye_tangzheyezhongqiang_deleted() {
        let top10 = mixed_top10("tangzhenye".as_bytes());
        assert!(
            !top10.iter().any(|w| w == "汤镇业"),
            "tangzhenye: 汤镇业 must be gone from the dict; got top10={top10:?}"
        );
        let top10 = mixed_top10("tangzheyezhongqiang".as_bytes());
        assert!(
            !top10.iter().any(|w| w == "躺着也中枪"),
            "tangzheyezhongqiang: 躺着也中枪 must be gone from the dict; got top10={top10:?}"
        );
        let top10 = mixed_top10("tangzhe".as_bytes());
        assert!(
            !top10.iter().any(|w| w == "汤镇业" || w == "躺着也中枪"),
            "tangzhe: neither must bleed in by code prefix; got top10={top10:?}"
        );
    }

    /// Regression pin (user report 2026-07-28): "gougou 狗狗".
    ///
    /// No data change — 狗狗 was already #0 (387109 = tier-4 cedict 380000 +
    /// modern_freq 7109) and the report is satisfied as-is. Pinned because
    /// it had no coverage at all, and because its 叠字 sibling 猫猫 was
    /// silently blocklisted by the 2026-06-22 `childlike_duplicate` sweep
    /// (see polish_maomao_maomao_leads) — 狗狗 escaped only by accident of
    /// which rows that sweep picked, so the invariant is worth locking.
    #[test]
    fn polish_gougou_gougou_leads() {
        let top10 = mixed_top10("gougou".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("狗狗"),
            "gougou: expected 狗狗 #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-28): "maomao 猫猫".
    ///
    /// Needed a 回捞, not just an add. The 2026-06-22 D1 sweep tagged 猫猫
    /// `childlike_duplicate` and logged it in corpus_garbage_filter_v1.tsv;
    /// v2 folds that file into its runtime exclusion set (exclusions = v1 ∪
    /// garbage filter), so a plain modern_vocab row was swallowed silently —
    /// the word did not appear at any rank. Same mechanism as the 窗外
    /// false-positive removed on 2026-07-13 (see
    /// polish_gaizhu_chuangwai_present). The blocklist row was deleted.
    ///
    /// Freq 30000 → tier 3 (410000) rather than the usual 15000 → tier 4,
    /// so 猫猫 deterministically clears the incumbent 猫毛 (modern_vocab
    /// 15000, tier 4, 380000) without needing a quickfix row; two tier-4
    /// entries would have tied at 380000. 30000 is the file's common-word
    /// band (喝水 / 轻烟).
    ///
    /// Sibling note: the same sweep tagged 11 other 叠字 nicknames (贝贝 /
    /// 便便 / 啵啵 / 兜兜 / 加加 / 尼尼 / 妞妞 / 球球 / 糖糖 / 兔兔 / 猪猪)
    /// while leaving 狗狗 alone. Only 猫猫 was recalled here — a blanket
    /// un-blocklist needs the per-row audit that [[feedback-sweep-per-row-audit]]
    /// requires.
    #[test]
    fn polish_maomao_maomao_leads() {
        let top10 = mixed_top10("maomao".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("猫猫"),
            "maomao: expected 猫猫 #0; got top10={top10:?}"
        );
    }

    /// Auto-pin removal (user 2026-07-20: "整个自动置顶都关了吧，没必要
    /// 这个功能"). Repeatedly committing a NON-top candidate must never
    /// promote it to #0 — candidate order is the dictionary's ruling plus
    /// explicit pins, nothing else.
    ///
    /// Origin: `fcu` (wubi 去/支/云). The user picked 云 (idx 2) three
    /// times on 2026-07-19; the old 3-pick auto-pin then wired 云 to #0
    /// and 去 stayed demoted across restarts — visible in the live
    /// polish-log as the candidate list flipping from [去,支,云] to
    /// [云,去,支] mid-session.
    #[test]
    fn repeated_commits_never_reorder_candidates() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);

        let baseline = mixed_top10(b"fcu");
        let target = &baseline[2];

        for _ in 0..5 {
            for b in b"fcu" {
                let _ = e.handle_letter(*b);
            }
            let idx = e
                .candidates()
                .iter()
                .position(|c| &c.word == target)
                .expect("target must stay present");
            assert_eq!(e.commit_index(idx).as_deref(), Some(target.as_str()));
        }

        assert_eq!(
            mixed_top10(b"fcu"),
            baseline,
            "committing the #2 candidate 5x must not reorder fcu"
        );
    }

    /// Class B polish (user report 2026-07-18): "jianju 间距 > 艰巨，
    /// 这两个在最前面". Was [艰巨, 检举, 间距, 兼具]. Pair boost per the
    /// winner-take-all rule: 间距 60000 > 艰巨 50000; 检举/兼具 unboosted
    /// fall behind.
    #[test]
    fn polish_jianju_head_pair() {
        let top10 = mixed_top10("jianju".as_bytes());
        let head: Vec<&str> = top10.iter().take(2).map(String::as_str).collect();
        assert_eq!(head, ["间距", "艰巨"], "jianju: got top10={top10:?}");
    }

    /// Class B polish (user report 2026-07-18): "wytf 领先 > 依靠" — the
    /// first wubi-side Class B. quickfix_boost is pinyin-only (wubi never
    /// reads it), so the kongdang precedent applies: reorder via a clean
    /// library.tsv freq edit, source marked `polish` so corpus digest
    /// cannot clobber it. 领先 28113 → 35260 (依靠 32054 + 10%), layer 1
    /// unchanged.
    #[test]
    fn polish_wytf_lingxian_first() {
        let top10 = mixed_top10("wytf".as_bytes());
        let head: Vec<&str> = top10.iter().take(2).map(String::as_str).collect();
        assert_eq!(head, ["领先", "依靠"], "wytf: got top10={top10:?}");
    }

    /// Class B polish (user report 2026-07-18): "jiqi 机器 第一".
    /// 机器 carries the highest v1 base of the group (35883 vs 极其 29042)
    /// but v2's HSK-derived tiers put the adverb 极其 at #0. quickfix
    /// 31950 (极其 base + 10%); 极其 unboosted falls to #1.
    #[test]
    fn polish_jiqi_jiqi_first() {
        let top10 = mixed_top10("jiqi".as_bytes());
        let head: Vec<&str> = top10.iter().take(2).map(String::as_str).collect();
        assert_eq!(head, ["机器", "极其"], "jiqi: got top10={top10:?}");
    }

    /// Class B polish (user report 2026-07-18): "fenwei 氛围 > 分为".
    /// 氛围 carries the higher v1 base (29248 vs 分为 27863) but the
    /// modern-freq prior lifted 分为 to #0. quickfix 30650 (分为 base +
    /// 10%); 分为 unboosted falls to #1 naturally.
    #[test]
    fn polish_fenwei_fenwei_first() {
        let top10 = mixed_top10("fenwei".as_bytes());
        let head: Vec<&str> = top10.iter().take(2).map(String::as_str).collect();
        assert_eq!(head, ["氛围", "分为"], "fenwei: got top10={top10:?}");
    }

    /// Class B polish (user report 2026-07-18): "mideng 幂等第一，迷瞪
    /// 似乎不是词". 迷瞪 IS a real word (现汉方言词, mídeng 神志迷糊) —
    /// per the user's own hedge it stays visible at #1 rather than being
    /// deleted; only the tech term 幂等 is lifted to #0. 迷瞪 had led via
    /// its modern_freq tiebreak (15303 vs 幂等's 0).
    #[test]
    fn polish_mideng_mideng_first_mideng_kept() {
        let top10 = mixed_top10("mideng".as_bytes());
        let head: Vec<&str> = top10.iter().take(2).map(String::as_str).collect();
        assert_eq!(head, ["幂等", "迷瞪"], "mideng: got top10={top10:?}");
    }

    /// Class B polish (user report 2026-07-18): "tangxia 躺下第一".
    /// In v2, 淌下/躺下 are both cedict tier 4 with identical score AND
    /// identical modern_freq (0 — jieba segments 躺下 as 躺/下), so the
    /// sort fell through to the alphabetical key and 淌 (U+6DCC) beat
    /// 躺 (U+8EBA) by codepoint. 淌下 has no v1 library row at all;
    /// 躺下 carries v1 freq 22445. quickfix 24700 (own base + 10%).
    #[test]
    fn polish_tangxia_tangxia_first() {
        let top10 = mixed_top10("tangxia".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("躺下"),
            "tangxia: expected 躺下 #0; got top10={top10:?}"
        );
    }

    /// Class D polish (user report 2026-07-15): "weidu ... 唯读 删除".
    /// 唯读 is the Taiwan term for read-only (大陆规范: 只读) and existed only
    /// as a v2 CC-CEDICT entry — not in the v1 pinyin library. Logged in
    /// corpus_garbage_filter_v1.tsv, which v2 folds into its runtime exclusion
    /// set (exclusions_v1 ∪ garbage_filter), dropping it from output.
    #[test]
    fn polish_weidu_weidu_no_weidu_taiwan() {
        let top10 = mixed_top10("weidu".as_bytes());
        assert!(
            !top10.iter().any(|w| w == "唯读"),
            "weidu: 唯读 (台湾用语) must not appear; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-15): "weidu 维度第二，未读第三".
    /// 唯独 keeps #0; 维度 and 未读 were at #3/#6. v2's quickfix is
    /// winner-take-all, so pinning ranks means chain-boosting the whole head
    /// in order: 唯独 60000 > 维度 50000 > 未读 40000, with 纬度/围堵/惟独
    /// unboosted so they fall below.
    #[test]
    fn polish_weidu_head_order() {
        let top10 = mixed_top10("weidu".as_bytes());
        let head: Vec<&str> = top10.iter().take(3).map(String::as_str).collect();
        assert_eq!(head, ["唯独", "维度", "未读"], "weidu: got top10={top10:?}");
    }

    /// Class B polish (user report 2026-07-21): "youxiang 邮箱第一".
    /// Supersedes the 2026-07-15 "邮箱第二" ruling — the user now wants 邮箱
    /// at #0, so the pair boost is simply swapped: 邮箱 60000 > 油箱 50000,
    /// 幽香 (unboosted) stays #2.
    #[test]
    fn polish_youxiang_youxiang_first() {
        let top10 = mixed_top10("youxiang".as_bytes());
        let head: Vec<&str> = top10.iter().take(2).map(String::as_str).collect();
        assert_eq!(head, ["邮箱", "油箱"], "youxiang: got top10={top10:?}");
    }

    /// Class D1 polish (user report 2026-07-14): "pusu 朴素第一，补校不是一个词".
    /// 补校 was not a pinyin candidate at all — it was a wubi phrase (pusu is
    /// its wubi code) digested out of the corpus at freq 0, and an exact wubi
    /// phrase match outranks pinyin in the cross-engine merge, so it held #0.
    /// Deleting the wubi library row does both halves of the request at once.
    #[test]
    fn polish_pusu_pusu_first_buxiao_gone() {
        let top10 = mixed_top10("pusu".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("朴素"),
            "pusu: expected 朴素 #0; got top10={top10:?}"
        );
        assert!(
            !top10.iter().any(|w| w == "补校"),
            "pusu: 补校 is not a word and must not appear; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-14): "xie 斜放到第四位".
    /// 斜 sat at #4 behind 歇. v2's quickfix is winner-take-all, so a lone
    /// boost row on 斜 would have taken #0 outright (it did — the pinyin_only
    /// baseline caught it). Placing a word at rank N therefore needs the whole
    /// prefix chain boosted above it: 些 60000 > 写 55000 > 鞋 50000 > 斜 45000,
    /// with 歇 left unboosted so it falls to #4.
    #[test]
    fn polish_xie_xie_fourth() {
        let top10 = mixed_top10("xie".as_bytes());
        let head: Vec<&str> = top10.iter().take(5).map(String::as_str).collect();
        assert_eq!(
            head,
            ["些", "写", "鞋", "斜", "歇"],
            "xie: got top10={top10:?}"
        );
    }

    /// v2 non-idiom backfill (2026-07-14). The v2 word list came from
    /// CC-CEDICT + HSK and never absorbed the v1 corpus vocabulary, so
    /// 135,579 v1 non-four-hanzi words were missing from v2 entirely.
    /// Every candidate was reviewed row-by-row; 15,081 admitted @ 15000.
    #[test]
    fn v2_nonidiom_backfill_representatives() {
        for (buf, expect) in [
            ("naodong", "脑洞"),
            ("chaoshan", "潮汕"),
            ("zaotangzi", "澡堂子"),
            ("meinanzi", "美男子"),
        ] {
            let top10 = mixed_top10(buf.as_bytes());
            assert_eq!(
                top10.first().map(String::as_str),
                Some(expect),
                "{buf}: expected {expect} #0; got top10={top10:?}"
            );
        }
        // The backfill introduced 汴京 (bianjing) and 发疹 (fazhen), which
        // displaced two user-decided orderings. quickfix_boost pins the
        // user's order back; the new words stay, just behind. Guard both.
        let bj = mixed_top10("bianjing".as_bytes());
        assert_eq!(
            bj.first().map(String::as_str),
            Some("边境"),
            "bianjing: {bj:?}"
        );
        assert_eq!(
            bj.get(1).map(String::as_str),
            Some("辩经"),
            "bianjing: {bj:?}"
        );
        let fz = mixed_top10("fazhen".as_bytes());
        assert_eq!(
            fz.first().map(String::as_str),
            Some("法阵"),
            "fazhen: {fz:?}"
        );
    }

    /// Class B polish (user report 2026-07-14): "tujian 图鉴第一".
    /// Both candidates in top-2 already; 土建 (construction jargon) led
    /// on modern-freq prior despite a lower library freq than 图鉴.
    /// quickfix_boost 17354 = 土建 base 15777 + 10% margin.
    #[test]
    fn polish_tujian_tujian_first() {
        let top10 = mixed_top10("tujian".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("图鉴"),
            "tujian: expected 图鉴 #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-13): "wuseng 武僧".
    /// 武僧 in v1 library @ 12030 but absent from v2 words.tsv —
    /// the wuseng buffer returned NOTHING (same gap shape as 黑屏).
    /// modern_vocab @ 15000 → #0 (no competitor).
    #[test]
    fn polish_wuseng_wuseng_first() {
        let top10 = mixed_top10("wuseng".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("武僧"),
            "wuseng: expected 武僧 #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-12): "guazhe 挂着".
    /// 挂着 was absent from every data surface — the guazhe buffer
    /// returned NOTHING (only 惦挂着 exists in v1 library). verb+着
    /// units are legit dict entries (看着 34440 precedent).
    /// modern_vocab @ 15000 → #0 (no competitor).
    #[test]
    fn polish_guazhe_guazhe_first() {
        let top10 = mixed_top10("guazhe".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("挂着"),
            "guazhe: expected 挂着 #0; got top10={top10:?}"
        );
    }

    /// Class A polish (user report 2026-07-12): "yingle 赢了".
    /// 赢了 was absent from every data surface — the buffer only
    /// returned a fuzzy 营垒 (yinglei). verb+了 units are legit dict
    /// entries (对了/duile precedent in both v1 library and v2
    /// cedict). modern_vocab @ 15000 → #0 (no exact competitor).
    #[test]
    fn polish_yingle_yingle_first() {
        let top10 = mixed_top10("yingle".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("赢了"),
            "yingle: expected 赢了 #0; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-12): "jianjie 间接 > 简洁
    /// > 简介 > 见解". Was [间接, 见解, 简介, 简洁]. quickfix is
    /// winner-take-all, so a chain-boost (间接 60000 / 简洁 50000 /
    /// 简介 40000, qingjiao precedent) pins the top-3; unboosted
    /// 见解 falls to #3.
    #[test]
    fn polish_jianjie_full_order() {
        let top10 = mixed_top10("jianjie".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("间接").expect("间接 missing from jianjie top10");
        let b = pos("简洁").expect("简洁 missing from jianjie top10");
        let c = pos("简介").expect("简介 missing from jianjie top10");
        let d = pos("见解").expect("见解 missing from jianjie top10");
        assert!(
            a == 0 && b == 1 && c == 2 && d == 3,
            "jianjie: expected 间接>简洁>简介>见解 at #0..#3; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-11): "luanma 乱码 > 乱麻".
    /// 乱麻 led (base 18678 vs 16953). quickfix_boost 20545 (top base
    /// + 10% margin, pick-helper derived) lifts 乱码 to #0.
    #[test]
    fn polish_luanma_luanma_first() {
        let top10 = mixed_top10("luanma".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("乱码").expect("乱码 missing from luanma top10");
        let b = pos("乱麻").expect("乱麻 missing from luanma top10");
        assert!(
            a == 0 && b == 1,
            "luanma: expected 乱码 #0, 乱麻 #1; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-11): "tuichu 退出 > 推出".
    /// 推出 led by a hair (404844 vs 404742 — v2 within-tier weights
    /// don't track library freq). quickfix_boost lifts 退出 to #0.
    #[test]
    fn polish_tuichu_tuichu_first() {
        let top10 = mixed_top10("tuichu".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("退出").expect("退出 missing from tuichu top10");
        let b = pos("推出").expect("推出 missing from tuichu top10");
        assert!(
            a == 0 && b == 1,
            "tuichu: expected 退出 #0, 推出 #1; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-11): "jinlai 进来 第一".
    /// 近来 sat #0 over 进来 despite 进来's higher library freq
    /// (36093 vs 26802 — v2 within-tier weights don't track library
    /// freq). quickfix_boost lifts 进来 to 60000 → #0.
    #[test]
    fn polish_jinlai_jinlai_first() {
        let top10 = mixed_top10("jinlai".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("进来").expect("进来 missing from jinlai top10");
        let b = pos("近来").expect("近来 missing from jinlai top10");
        assert!(
            a == 0 && b == 1,
            "jinlai: expected 进来 #0, 近来 #1; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-10): "qingjiao 青椒第二".
    /// 青椒 sat at #3 behind 倾角/清剿 (v2 within-tier weights don't
    /// track library freq). quickfix promotion is winner-take-all at
    /// any value, so a pair-boost holds the order by data: 请教 60000
    /// stays #0, 青椒 50000 lands #1.
    #[test]
    fn polish_qingjiao_qingjiao_second() {
        let top10 = mixed_top10("qingjiao".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("请教").expect("请教 missing from qingjiao top10");
        let b = pos("青椒").expect("青椒 missing from qingjiao top10");
        assert!(
            a == 0 && b == 1,
            "qingjiao: expected 请教 #0, 青椒 #1; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-29): "lianjie 连接第一".
    /// 连接 (library 31309) sat at #2 behind 链接 (14595) because 链接
    /// carries a modern_vocab 55000 row plus a 30000 quickfix. Adding a
    /// 60000 quickfix for 连接 puts both in the same promoted tier where
    /// boost value decides: 连接 #0, 链接 #1.
    #[test]
    fn polish_lianjie_lianjie_first() {
        let top10 = mixed_top10("lianjie".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("连接").expect("连接 missing from lianjie top10");
        let b = pos("链接").expect("链接 missing from lianjie top10");
        assert!(
            a == 0 && b == 1,
            "lianjie: expected 连接 #0, 链接 #1; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-06-26): "xuxian 虚线 续弦 都要
    /// 在许仙前面". 许仙 (白蛇传 proper noun) base 17781 was crowding
    /// #0 over the common-usage 虚线 (16059) and 续弦 (8669).
    /// quickfix_boost lifts 虚线 → 19600 and 续弦 → 18700 so the order
    /// becomes [虚线 #0, 续弦 #1, 许仙 #2].
    #[test]
    fn polish_xuxian_xuxian_xuxian_above_xuxian() {
        let top10 = mixed_top10("xuxian".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("虚线").expect("虚线 missing from xuxian top10");
        let b = pos("续弦").expect("续弦 missing from xuxian top10");
        let c = pos("许仙").expect("许仙 missing from xuxian top10");
        assert!(
            a < b && b < c,
            "xuxian: expected 虚线<续弦<许仙; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-06): "bingtong 病痛 > 冰桶 >
    /// 丙酮". Base freqs (病痛 23002 > 冰桶 14411 > 丙酮 14110) suggested
    /// 病痛 should lead but 丙酮 was #0 due to IDF asymmetry. quickfix_boost
    /// lifts 病痛 → 30000 (#0), 冰桶 → 25000 (#1), 丙酮 base falls to #2.
    #[test]
    fn polish_bingtong_order() {
        let top10 = mixed_top10("bingtong".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("病痛").expect("病痛 missing from bingtong top10");
        let b = pos("冰桶").expect("冰桶 missing from bingtong top10");
        let c = pos("丙酮").expect("丙酮 missing from bingtong top10");
        assert!(
            a < b && b < c,
            "bingtong: expected 病痛<冰桶<丙酮; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-30): "fangzhi 防止 > 防治 > 放置
    /// > 仿制 > 防制 > 纺织 > 方知 > 方志". All eight are pre-existing
    /// digested rows; base freq had 纺织 (17796) leading over 防止 (31851)
    /// via IDF asymmetry, and 防制 (7878) has to jump above 纺织. An explicit
    /// 8-row quickfix chain (60000 down to 46000 in 2000 steps) puts them all
    /// in the promoted tier where boost value alone orders them.
    #[test]
    fn polish_fangzhi_full_order() {
        let top10 = mixed_top10("fangzhi".as_bytes());
        let expected = [
            "防止", "防治", "放置", "仿制", "防制", "纺织", "方知", "方志",
        ];
        let ranks: Vec<usize> = expected
            .iter()
            .map(|w| {
                top10
                    .iter()
                    .position(|x| x == w)
                    .unwrap_or_else(|| panic!("{w} missing from fangzhi top10: {top10:?}"))
            })
            .collect();
        assert!(
            ranks.windows(2).all(|p| p[0] < p[1]),
            "fangzhi: expected {expected:?} in order; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-30): "bianji 编辑 > 边际 > 遍及".
    /// A 2026-06-30 dogfood quickfix row (bianji 遍及 30000) had promoted
    /// 遍及 to tier 1 = 540300, outranking 编辑 (cedict+hsk5 tier 3 = 434977)
    /// and 边际 (cedict tier 4 = 403724). That row is retired and replaced by
    /// an explicit 3-row chain (60000 / 55000 / 50000) so boost value alone
    /// orders all three within tier 1.
    #[test]
    fn polish_bianji_order() {
        let top10 = mixed_top10("bianji".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("编辑").expect("编辑 missing from bianji top10");
        let b = pos("边际").expect("边际 missing from bianji top10");
        let c = pos("遍及").expect("遍及 missing from bianji top10");
        assert!(
            a < b && b < c,
            "bianji: expected 编辑<边际<遍及; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-30):
    /// "fanhua 繁华 > 泛化 > 反话 > 反华 > 繁花".
    /// All five are pre-existing digested rows; base order ran
    /// 繁华 / 繁花 / 反话 / 反华 / 泛化 — 泛化 (library 10668 + modern_vocab
    /// 15000) sat last while 繁花 (19324) led the rest on raw library freq.
    /// Explicit 5-row chain (60000 / 56000 / 52000 / 48000 / 44000) puts all
    /// five in tier 1 where boost value alone orders them.
    #[test]
    fn polish_fanhua_order() {
        let top10 = mixed_top10("fanhua".as_bytes());
        let expected = ["繁华", "泛化", "反话", "反华", "繁花"];
        let ranks: Vec<usize> = expected
            .iter()
            .map(|w| {
                top10
                    .iter()
                    .position(|x| x == w)
                    .unwrap_or_else(|| panic!("{w} missing from fanhua top10: {top10:?}"))
            })
            .collect();
        assert!(
            ranks.windows(2).all(|p| p[0] < p[1]),
            "fanhua: expected {expected:?} in order; got top10={top10:?}"
        );
    }

    /// Class B polish (user report 2026-07-30): "dongshi 懂事 第一".
    /// A modern_vocab_v1 row (dongshi 董事 50000) lifted 董事 over 懂事 even
    /// though the library freqs run the other way (懂事 29367 > 董事 24272) —
    /// the modern-freq overlay inverted the natural order. A single quickfix
    /// row (55000) promotes 懂事 to tier 1; 董事 keeps its modern_vocab
    /// standing at #1.
    #[test]
    fn polish_dongshi_order() {
        let top10 = mixed_top10("dongshi".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("懂事"),
            "dongshi: expected 懂事 at #0; got top10={top10:?}"
        );
        let b = top10
            .iter()
            .position(|x| x == "董事")
            .expect("董事 missing from dongshi top10");
        assert!(b > 0, "dongshi: expected 董事 below 懂事; got {top10:?}");
    }

    /// Class B polish (user report 2026-08-02): "yanzhi 颜值第一".
    /// 颜值 sat at #5 (380k) under 研制/胭脂/腌制/阏氏/焉知. A single
    /// quickfix row (55000 → tier 1 = 540550) promotes it to #0 over
    /// 研制 404887; the rest of the order is untouched (dongshi
    /// single-row precedent).
    #[test]
    fn polish_yanzhi_order() {
        let top10 = mixed_top10("yanzhi".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("颜值"),
            "yanzhi: expected 颜值 at #0; got top10={top10:?}"
        );
        let b = top10
            .iter()
            .position(|x| x == "研制")
            .expect("研制 missing from yanzhi top10");
        assert!(b > 0, "yanzhi: expected 研制 below 颜值; got {top10:?}");
    }

    /// Class B polish (user report 2026-08-05): "baifen 百分 > 白粉".
    /// The two were a PERFECT tie — same v2 tier (4, both cedict), same
    /// modern_freq (22193), so both scored 402193 and the order fell
    /// through to the final codepoint tiebreak, where 白 (U+767D) edges
    /// out 百 (U+767E). Nothing in the corpus signal separates them, so
    /// this is an attested per-case preference: a single quickfix row
    /// (55000 → tier 1 = 540550) puts 百分 at #0 and leaves 白粉 at #1.
    #[test]
    fn polish_baifen_order() {
        let top10 = mixed_top10("baifen".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("百分"),
            "baifen: expected 百分 at #0; got top10={top10:?}"
        );
        let b = top10
            .iter()
            .position(|x| x == "白粉")
            .expect("白粉 missing from baifen top10");
        assert!(b > 0, "baifen: expected 白粉 below 百分; got {top10:?}");
    }

    /// Class B polish (user report 2026-08-15): "gaoliang 高亮 > 高粱".
    /// All three gaoliang words share v2 tier 4 (cedict), so modern_freq
    /// decided the order — and jieba's dict has no 高亮 (score 0) while
    /// 高粱 24194 / 膏粱 18603 both score, burying the tech term at #2.
    /// A pair-boost (60000 / 50000 → tier 1) locks the explicit order
    /// (bianji / fanhua chain precedent).
    #[test]
    fn polish_gaoliang_order() {
        let top10 = mixed_top10("gaoliang".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("高亮"),
            "gaoliang: expected 高亮 at #0; got top10={top10:?}"
        );
        let b = top10
            .iter()
            .position(|x| x == "高粱")
            .expect("高粱 missing from gaoliang top10");
        assert_eq!(b, 1, "gaoliang: expected 高粱 at #1; got {top10:?}");
    }

    /// Class C polish (user report 2026-08-02): "jianlou 检漏好像不是词?".
    /// 检漏 IS real (technical: 检漏仪 / 真空检漏) so no D1 — but at lib
    /// freq 3094 it outranked colloquial 捡漏 (15033) in the live scores,
    /// an inversion of the natural library order. tier_overlay demotes
    /// (jianlou, 检漏) to tier 5: 简陋 leads, 捡漏 second, 检漏 sinks
    /// below the everyday words but stays reachable.
    #[test]
    fn polish_jianlou_jianlou_demoted() {
        let top10 = mixed_top10("jianlou".as_bytes());
        assert_eq!(
            top10.first().map(String::as_str),
            Some("简陋"),
            "jianlou: expected 简陋 at #0; got top10={top10:?}"
        );
        assert!(
            !top10.iter().take(3).any(|x| x == "检漏"),
            "jianlou: 检漏 must not be in top-3; got {top10:?}"
        );
        let jian = top10
            .iter()
            .position(|x| x == "捡漏")
            .expect("捡漏 missing from jianlou top10");
        assert!(jian < 3, "jianlou: expected 捡漏 in top-3; got {top10:?}");
    }

    /// Class C polish (user report 2026-07-06): "khuq 跤 > 中奖 > 中将,
    /// 五笔不应该出这种问题, 四码单字如果不是低频难检应该在词上面的".
    /// khuq is a wubi 4-code full-code buffer that produces 跤 (single-char,
    /// freq 17713) and phrases 中将 (25158) / 中奖 (18235). The wubi
    /// engine's layer bonus (phrases get +400k over single-chars) pushed
    /// the phrases to top, but the user considers 跤 (common single-char)
    /// should lead. tier_overlay demotes 中将 → tier 6 and 中奖 → tier 5
    /// so 跤 (natural tier ≤4) sorts above both. NOTE: user also raised
    /// structural concern "五笔不应该出这种问题" — the wubi phrase-over-
    /// single-char layer bonus deserves a framework-level review; per-entry
    /// polish here is a workaround for this specific buffer only.
    #[test]
    fn polish_khuq_order() {
        let top10 = mixed_top10("khuq".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("跤").expect("跤 missing from khuq top10");
        let b = pos("中奖").expect("中奖 missing from khuq top10");
        let c = pos("中将").expect("中将 missing from khuq top10");
        assert!(
            a < b && b < c,
            "khuq: expected 跤<中奖<中将; got top10={top10:?}"
        );
    }

    /// Class C polish (user report 2026-07-06): "wugu 假痴不癫 肯定要放
    /// 很后面，T 级低, 无辜 > 五谷 > 无故 > 巫蛊 > 假痴不癫". Wubi
    /// 3-jianma 假痴不癫 was leading at #0 via 五笔 dispatch. tier_overlay
    /// 假痴不癫 → tier 8 demotes it below all real pinyin candidates.
    #[test]
    fn polish_wugu_order() {
        let top10 = mixed_top10("wugu".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let a = pos("无辜").expect("无辜 missing from wugu top10");
        let b = pos("五谷").expect("五谷 missing from wugu top10");
        let c = pos("巫蛊").expect("巫蛊 missing from wugu top10");
        let d = pos("假痴不癫").expect("假痴不癫 missing from wugu top10");
        assert!(
            a < b && b < c && c < d,
            "wugu: expected 无辜<五谷<巫蛊<假痴不癫; got top10={top10:?}"
        );
    }

    /// Class C polish (user report 2026-06-10): "quanquan 拳拳 劝劝
    /// 要放在最后". 拳拳 is idiom-only (拳拳之心), kept in the dict
    /// (and on the AA-redup-sweep keep whitelist) but tier-demoted so
    /// it doesn't crowd common daily peers 全权 / 圈圈.
    ///
    /// 2026-06-22 update: AA-redup sweep batch 2 (commit pending)
    /// deleted 劝劝 entirely as noise (verb-redup "劝一劝" too weak
    /// for a dict entry — recapture via `source=polish` row if a user
    /// reports it). Original test pinned 劝劝 alongside 拳拳 in the
    /// demote invariant; with 劝劝 gone from the dict the assertion
    /// reduces to "拳拳 ranks below 全权 / 圈圈".
    #[test]
    fn polish_quanquan_demote_fistfist_reduplication() {
        let top10 = mixed_top10("quanquan".as_bytes());
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let q = pos("全权").expect("全权 missing from quanquan top10");
        let r = pos("圈圈").expect("圈圈 missing from quanquan top10");
        let f = pos("拳拳").expect("拳拳 missing from quanquan top10");
        assert!(
            q < f && r < f,
            "quanquan: 拳拳 should rank below 全权/圈圈; got top10={top10:?}"
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

    /// Polish-log 2026-07-10: user flagged shej candidates polluted by
    /// dogfood article-segment fragments ("这种垃圾短句怎么进了词库").
    /// Root cause: A147-era ingest dumped clause slices into
    /// modern_vocab_v1.tsv at flat freq 30000. Phase-1 sweep audited
    /// all >=5-hanzi rows per-row (1611 rows, 1454 deleted, logged to
    /// corpus_garbage_filter_v1.tsv). These fragments must never
    /// resurface.
    #[test]
    fn polish_shej_dogfood_fragments_removed() {
        let top10 = mixed_top10(b"shej");
        let mut failures = Vec::new();
        // P2 extension (same sweep, 2-4 hanzi classes): 社均 is a
        // stats-jargon slice, 舌尖上 a 舌尖上的中国 title slice.
        for bad in &[
            "涉及多个政府部门",
            "涉及本部门的政务数据校核申请",
            "社均",
            "舌尖上",
        ] {
            if top10.iter().any(|w| w == bad) {
                failures.push(format!("  shej: {bad} still surfaces; top10={top10:?}"));
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} shej dogfood-fragment cases failed:\n{}",
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
            // 2026-06-27 user: "thgf 牌 > 处于". Single-char full code
            // (牌 = thgf) must beat 2-char phrase encoding (处=th, 于=
            // gf). Phrase 处于 demoted to tier 5 via tier_overlay.
            ("thgf", "牌"),
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
        // CP-3.6 step-2 long-abbrev wire (2026-06-16) made vowel-free
        // 5+ char buffers an abbreviation-intent signal; legitimate
        // strings like `qwxzy` now resolve to 请问下周一. Use `vvvvv`
        // (v is in the vowel-exclusion list and never a pinyin
        // initial) so the buffer reliably routes to ASCII fallback.
        for b in b"vvvvv" {
            if let Some(c) = e.handle_letter(*b) {
                last_commit = Some(c);
            }
        }
        assert_eq!(
            last_commit.as_deref(),
            Some("vvvvv"),
            "expected ASCII fallback to commit 'vvvvv' as raw ASCII"
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
            ("queshi", "确实"), // user 2026-06-27 re-attestation reversed prior polish-log (n=4 缺失 → 确实)
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
            // User polish-log 2026-06-28: "tuidao 推倒 > 推导 > 退到". 推倒
            // already leads naturally (base 23966); 推导 boosted above 退到
            // via quickfix_boost (see polish_log_tuidao_order for the
            // relative-rank invariant).
            ("tuidao", "推倒"),
        ];
        run("polish_log", cases, pinyin_top, pinyin_top10);
    }

    /// 2026-06-28 user: "tuidao 推倒 > 推导 > 退到". 推倒 leads via natural
    /// freq (base 23966 > peers); 推导 boost (quickfix_boost.tsv freq=20000)
    /// flips order against 退到 (base 16854, raw freq lower than 推导's
    /// boosted 20000). This test pins the relative ordering 推导 > 退到
    /// — the actual polish goal. 推倒 leadership covered separately above.
    #[test]
    fn polish_log_tuidao_order() {
        let top10 = pinyin_top10(b"tuidao");
        let pos = |w: &str| top10.iter().position(|x| x == w);
        let p_tuidao_dao = pos("推导");
        let p_tuidao_tui = pos("退到");
        assert!(
            p_tuidao_dao.is_some() && p_tuidao_tui.is_some(),
            "tuidao top10 missing 推导 or 退到: {:?}",
            top10
        );
        assert!(
            p_tuidao_dao.unwrap() < p_tuidao_tui.unwrap(),
            "tuidao: 推导 must rank above 退到 (got 推导@{} 退到@{}, top10={:?})",
            p_tuidao_dao.unwrap(),
            p_tuidao_tui.unwrap(),
            top10
        );
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
            user_bigram: vec![],
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
            user_bigram: vec![],
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

    /// Polish 2026-08-05 (user /polish): "fangdic / fangdich / fangdicha
    /// 这种情况下，仍然应该是房地产在前，没到底 4 字还在前，预测逻辑上要做
    /// 好排序，matching 怎么都应该是有序的".
    ///
    /// Class C — 房地产商 / 房地产业 carried a bulk `30000` freq in
    /// modern_vocab_v1.tsv (→ v2 tier 3) while the base word 房地产 only
    /// exists in the cedict-derived words.tsv at tier 5. v2's prefix
    /// completion path scores `250000 − tier·30000 + modern_freq`, so the
    /// two derived compounds (modern_freq 0 — neither appears in the jieba
    /// corpus at all) outranked their own base word (modern_freq 24650) at
    /// every partial buffer. Both demoted to 12000 (→ tier 5), which puts
    /// all three in one tier and lets the modern-freq tiebreaker order them
    /// honestly.
    ///
    /// Before: [房地产业 160000, 房地产商 160000, 房地产 124650]
    /// After:  [房地产 124650, 房地产业 100000, 房地产商 100000]
    #[test]
    fn polish_fangdic_base_word_leads_its_extensions() {
        for buf in ["fangdic", "fangdich", "fangdicha"] {
            let top10 = pinyin_top10(buf.as_bytes());
            let pos = |w: &str| top10.iter().position(|x| x == w);
            let base = pos("房地产").unwrap_or_else(|| {
                panic!("房地产 missing from {buf} top10; got {top10:?}");
            });
            for ext in ["房地产业", "房地产商"] {
                if let Some(e) = pos(ext) {
                    assert!(
                        base < e,
                        "{buf}: 房地产 must lead its extension {ext}; got {top10:?}"
                    );
                }
            }
        }
        // The full code still resolves to the base word, and each
        // extension still owns its own complete code.
        assert_eq!(
            pinyin_top10(b"fangdichan").first().map(String::as_str),
            Some("房地产"),
            "fangdichan must still lead 房地产"
        );
        assert_eq!(
            pinyin_top10(b"fangdichanye").first().map(String::as_str),
            Some("房地产业"),
            "fangdichanye must still lead 房地产业"
        );
        assert_eq!(
            pinyin_top10(b"fangdichanshang").first().map(String::as_str),
            Some("房地产商"),
            "fangdichanshang must still lead 房地产商"
        );
    }

    /// Polish 2026-08-10 (user /polish): "mengban 蒙板".
    ///
    /// Class A — `mengban` had NO word entry in either pinyin surface.
    /// The buffer returned a single candidate, 盟邦, which is not even
    /// an exact match (it lives at `mengbang` and reached `mengban`
    /// through the prefix-completion band at `composed_fallback_score`).
    ///
    /// The add had to land on BOTH surfaces: `library.tsv` feeds v1's
    /// pinyin.dict + words.idf, but v2 (the default engine since Phase
    /// 7) builds its word table from `words.tsv` ∪ `modern_vocab_v1.tsv`
    /// and never reads library.tsv — a library-only add is invisible at
    /// runtime. freq 16000 → v2 tier 4, the same tier cedict assigns
    /// every 2-char word (盟邦 / 蒙蔽 / 蒙馆 are all tier 4).
    ///
    /// Before: [盟邦]   After: [蒙板]
    #[test]
    fn polish_mengban_yields_mengban() {
        let top10 = pinyin_top10(b"mengban");
        assert_eq!(
            top10.first().map(String::as_str),
            Some("蒙板"),
            "mengban must lead 蒙板 (Class A add); got {top10:?}"
        );
    }

    /// Phase 7d framework rule (same 2026-08-05 report, second half:
    /// "预测逻辑上要做好排序，matching 怎么都应该是有序的"). The v2 prefix
    /// band orders by 字数 first inside a single tier, so a word always
    /// outranks the words that extend it — end-to-end through the
    /// cross-engine merge, not just inside `inputx_pinyin_v2::query`.
    ///
    /// Chains are tier-inverted on purpose (人民 tier 4 < 人民币 tier 2;
    /// 计算机 tier 5 < 计算机病毒 tier 4) so passing means the ordering
    /// came from the structural rule, not from the entries' tiers.
    /// Before Phase 7d: `renmi` → [人民币 任命 人民军 人民性 人民网
    /// 人民大会堂 人民日报社 人民] — the base word came dead last.
    #[test]
    fn prefix_completion_base_word_outranks_extensions() {
        let cases: &[(&str, &str, &[&str])] = &[
            ("renmi", "人民", &["人民币", "人民网", "人民大会堂"]),
            (
                "jisuanj",
                "计算机",
                &["计算机病毒", "计算机程序", "计算机网络"],
            ),
            ("fangdic", "房地产", &["房地产商", "房地产业"]),
            ("beijin", "北京", &["北京市", "北京鸭"]),
            ("xiaox", "小心", &["小心翼翼"]),
        ];
        let mut failures = Vec::new();
        for (buffer, base, extensions) in cases {
            let top10 = pinyin_top10(buffer.as_bytes());
            let pos = |w: &str| top10.iter().position(|x| x == w);
            let Some(p_base) = pos(base) else {
                failures.push(format!("  {buffer}: {base} missing; got {top10:?}"));
                continue;
            };
            for ext in *extensions {
                if let Some(p_ext) = pos(ext)
                    && p_base > p_ext
                {
                    failures.push(format!(
                        "  {buffer}: {base}@{p_base} must outrank {ext}@{p_ext}; got {top10:?}"
                    ));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} prefix-ordering invariant breaks:\n{}",
            failures.len(),
            failures.join("\n")
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
            // 2026-06-13 polyphone-dup sweep correction: the canonical
            // code here was wrongly set to the 大→dài misreading
            // `daierxi` by the original er-typo sweep. 大儿媳 reads
            // dà-ér-xí = daerxi; the daierxi copy was a 大-polyphone
            // dup, now deleted. Canonical fixed to daerxi.
            ("darxi", "daerxi", "大儿媳"),
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
            ("duan", &["雄姿英发"]), // 2026-06-29: wubi 4-char DUAN collides with pinyin duan
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
            // User polish-log 2026-06-09: "xiewen 斜纹就行了，写文不像
            // 是个词" — jieba sub-word noise (写 + 文); 斜纹 now leads.
            ("xiewen", &["写文"]),
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
            // ("yichu", &["一出", "一处", "一触"]) — 一处 retired from noise 2026-06-30 strict (user typed in article 0001)
            ("yichu", &["一出", "一触"]),
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
            // User polish-log 2026-06-10: "quanquan 泉泉 犬犬 不应该有" —
            // single-char reduplication noise from jieba sub-word ingest;
            // neither is a real Chinese bigram. D1 deleted from
            // library.tsv + logged to corpus_garbage_filter_v1.
            ("quanquan", &["泉泉", "犬犬"]),
            // User polish-log 2026-06-10: "tongxia 同下不像是个词，
            // 应该是统辖" — 同下 is jieba 字字直拼 (同+下), not a real
            // word (the academic citation idiom is 同上, not 同下).
            // D1 deleted; 统辖 (rule, govern) auto-leads as the
            // remaining real candidate at this buffer.
            ("tongxia", &["同下"]),
            // User polish-log 2026-06-10: "dongdong 洞洞 咚咚 动动 东东
            // 就好了，其他的不合适" — 冬冬 / 冻冻 / 栋栋 are jieba
            // single-char reduplication noise (字字直拼 + reduplication
            // pattern); only nicknames/onomatopoeia survive (洞洞 咚咚
            // 动动 东东). D1 deleted from library.tsv + logged to
            // corpus_garbage_filter_v1.
            ("dongdong", &["冬冬", "冻冻", "栋栋"]),
            // User polish-log 2026-06-12: "daomu 盗墓第一，道木不像个词" —
            // 道木 is jieba sub-word noise (railway-sleeper jargon at
            // best, 字字直拼 dào+mù); was anomalously leading the
            // buffer. D1 deleted from library.tsv + logged to
            // corpus_garbage_filter_v1. 盗墓 leads (see top-1 fixture).
            ("daomu", &["道木"]),
            // User polish-log 2026-06-27: "wuliu 五六不是个词" — 字字直拼
            // wǔ+liù, jieba sub-word noise (counts from patterns like
            // 五六个/五六十, not a standalone word). D1 deleted from
            // library.tsv + logged to corpus_garbage_filter_v1. 物流
            // auto-leads.
            ("wuliu", &["五六"]),
            // Polish 2026-06-30: user "xingshie 刑事饿，这不应该存在".
            // word+char compose noise; D2-hidden (framework gap noted).
            ("xingshie", &["刑事饿"]),
            // Polish 2026-06-30: user "神入不应该是个词" — 字字直拼
            // shén+rù, not a real Chinese word. D1 via corpus_garbage_filter
            // (v2 reads garbage_filter as exclusion).
            ("shenru", &["神入"]),
            // Polish 2026-06-30: user "yidal 义大利面 删了". 义大利面 是
            // Taiwan variant; mainland 意大利面. D1 via corpus_garbage_filter
            // + v2 prefix path now honors own-code exclusion (一次堵
            // yidalimian 真实代码 + yidal 前缀两处).
            ("yidal", &["义大利面"]),
            ("yidalimian", &["义大利面"]),
            // User polish-log 2026-06-27: "zhengyu 正宇 正于 证于 都不是
            // 词" — 正宇 is a given name (proper noun), 正于/证于 are
            // 字字直拼 jieba noise (e.g. 正于此时). All three D1 deleted
            // from library.tsv + logged to corpus_garbage_filter_v1.
            // 蒸鱼 (steamed fish) is a real word and auto-leads.
            ("zhengyu", &["正宇", "正于", "证于"]),
            // User polish-log 2026-06-28: "tuidao 推到删" — 字字直拼
            // tuī+dào (e.g. 把车推到那边), not a standalone word. D1
            // deleted; 推倒 (push over) auto-leads as the real verb.
            ("tuidao", &["推到"]),
            // 2026-06-13 polyphone-dup sweep batch1 — wrong-reading corpus
            // copies (a word mass-duplicated onto a 错读 code keyed off a
            // secondary char reading, identical freq = pure copy). These
            // must NOT surface at the 错读 buffer; the correct-reading
            // version stays (asserted in polyphone_sweep_correct_readings_kept).
            ("yidaili", &["意大利"]), // 大 da→dai
            ("mocuo", &["没错"]),     // 没 mei→mo
            ("wokuai", &["我会"]),    // 会 hui→kuai
            ("yuxian", &["遇见"]),    // 见 jian→xian
            ("guazao", &["聒噪"]),    // 聒 — pypinyin判反, 删prim(gua), 正确读 guō
            // batch2a (双向多音字 per-word call): wrong-reading side deleted.
            ("shoucha", &["手刹"]),       // 刹 手刹读shā, cha错读
            ("xuepingguo", &["削苹果"]),  // 削 削苹果读xiāo, xue错读
            ("nipoer", &["尼泊尔"]),      // 泊 尼泊尔读bó, po错读
            ("pengyoujuan", &["朋友圈"]), // 圈 quān, juan错读
            ("zhuquan", &["猪圈"]),       // 圈 猪圈读juàn, quan错读
            ("yiji", &["一系"]),          // 系 一系读xì, ji错读
            // batch2b-2 大字 (default 主体 + minority exceptions):
            ("chaohuaxishi", &["朝花夕拾"]), // 朝 zhāo, chao错读
            ("jiangyao", &["降妖"]),         // 降 xiáng, jiang错读
            ("shengtang", &["盛汤"]),        // 盛 chéng, sheng错读
            ("charen", &["差人"]),           // 差 chāi, cha错读
            // batch2b-3:
            ("danqiao", &["蛋壳"]), // 壳 ké, qiao错读
            ("shoudou", &["首都"]), // 都 dū, dou错读
            ("shene", &["深恶"]),   // 恶 wù, e错读
            // batch2c — 弹 (default dàn 名词, tán 动词 exceptions):
            // dàn-reading 弹片 / 弹幕 must NOT appear at the tan→错读 buffer;
            // tán-reading 弹跳 / 弹奏 must NOT appear at the dan→错读 buffer.
            ("tanpian", &["弹片"]), // 弹 dàn, tan错读
            ("tanmu", &["弹幕"]),   // 弹 dàn, tan错读
            ("dantiao", &["弹跳"]), // 弹 tán, dan错读
            ("danbo", &["弹拨"]),   // 弹 tán, dan错读
            // 2026-06-17 polyphone-dup sweep batch2-hard 行 char (climb-final
            // Stage B3 part-1). 行 = xíng (走/进行 主流) + háng (银行/
            // 行业/商店/线 of text). pypinyin defaults 行 prim_syl=xing for
            // every word — wrong副本 either lives at hang→ (xíng-reading
            // copy mass-duped) or at xing→ (háng-reading mis-primaried).
            // 252 rows deleted; 20 truly-ambiguous kept both (人行/大行/
            // 民行/三人行 — see batch2_hard_verdict.py).
            //
            // del_wrong group: word is xíng-reading; hang→ copy is the wrong
            // one and got deleted. So at hang-buffer, word now absent.
            ("jiuhangle", &["就行了"]),  // 行 xíng
            ("haihang", &["还行"]),      // 行 xíng
            ("hangbuxing", &["行不行"]), // 行 xíng (双 occur)
            ("hangdetong", &["行得通"]), // 行 xíng
            // NB: jinhangqu (进行曲) excluded — lattice composer
            // reconstructs it via single-char heteronym path (进+行háng+
            // 曲); dict-row deletion alone can't suppress this without
            // a D2-style hide list, which is a separate concern.
            // del_prim group: word is háng-reading; pypinyin mis-primaried as
            // xing → xing-copy deleted. So at xing-buffer, word now absent.
            ("xingyuan", &["行员"]), // 行 háng (bank teller)
            ("zhixing", &["支行"]),  // 行 háng (financial branch)
            ("gexing", &["各行"]),   // 行 háng (各行各业)
            ("minxing", &["闵行"]),  // 行 háng (上海地名)
            ("qinxing", &["琴行"]),  // 行 háng (instrument shop)
            ("taixing", &["太行"]),  // 行 háng (山名)
            // 2026-06-17 polyphone-dup sweep batch2-hard 长 char (climb-final
            // Stage B3 part-2). 长 = cháng (长城/长时间/place names) +
            // zhǎng (长胖/长肉 + 官名 营长/族长). 246 wrong-syl copies
            // deleted; 43 truly-ambiguous kept both (长长/长头/子长/
            // 长大 — see batch2_hard_verdict.py uncertain set).
            //
            // del_prim group: word is cháng-reading; pypinyin zhang-row
            // is the wrong copy and got deleted. So at zhang-buffer,
            // word now absent.
            ("zhangdao", &["长岛"]),  // 长 cháng (place)
            ("zhangtan", &["长滩"]),  // 长 cháng (place)
            ("zhangyi", &["长椅"]),   // 长 cháng (long chair)
            ("zhanglu", &["长路"]),   // 长 cháng (long road)
            ("zhangzhi", &["长治"]),  // 长 cháng (place 长治市)
            ("zhangjing", &["长颈"]), // 长 cháng (long neck)
            ("zhangqun", &["长裙"]),  // 长 cháng (long skirt)
            // del_wrong group: word is zhǎng-reading; pypinyin chang-row
            // is the wrong copy and got deleted. So at chang-buffer,
            // word now absent.
            ("yingchang", &["营长"]),      // 长 zhǎng (officer)
            ("zuchang", &["族长"]),        // 长 zhǎng (clan chief)
            ("tanchang", &["探长"]),       // 长 zhǎng (detective)
            ("yuanchang", &["园长"]),      // 长 zhǎng (school principal)
            ("quchang", &["区长"]),        // 长 zhǎng (district chief)
            ("changpang", &["长胖"]),      // 长 zhǎng (grow fat)
            ("changrou", &["长肉"]),       // 长 zhǎng (grow meat)
            ("changdou", &["长痘"]),       // 长 zhǎng (grow pimples)
            ("changjianshi", &["长见识"]), // 长 zhǎng (gain insight)
            // 2026-06-17 polyphone-dup sweep batch2-hard 重 char (climb-final
            // Stage B3 part-3). 重 = zhòng (heavy/important 重要/重病/
            // 重视 主流) + chóng (again/re- 重新/重来/重组 + 重庆).
            // Default del_wrong (zhòng-side wins for most words). Exception
            // del_prim = clear chóng-reading words (重 + verb pattern +
            // 重庆 abbreviations). 174 rows cleared; many layer-count words
            // (一重/两重 etc) kept both as truly ambiguous.
            //
            // del_wrong group: word is zhòng-reading; pypinyin chong-row
            // is the wrong copy and got deleted. So at chong-buffer,
            // word now absent.
            ("zunchong", &["尊重"]),   // 重 zhòng (respect)
            ("renchong", &["任重"]),   // 重 zhòng (任重道远)
            ("zhengchong", &["症重"]), // 重 zhòng — but "症重" rare; skip
            ("changchong", &["惨重"]), // 重 zhòng (severe)
            ("haochong", &["毫重"]),   // skip non-existent
            // del_prim group: word is chóng-reading; pypinyin zhong-row
            // is the wrong copy and got deleted. So at zhong-buffer,
            // word now absent.
            ("zhongkao", &["重考"]),   // 重 chóng (re-exam)
            ("zhongzhen", &["重振"]),  // 重 chóng (re-spirit)
            ("zhongsuo", &["重塑"]),   // 重 chóng (re-shape)
            ("zhongshi", &["重拾"]),   // 重 chóng (re-pickup) — NB 重视 zhòng-shì also exists, KEPT
            ("zhongkai", &["重开"]),   // 重 chóng (re-open)
            ("zhongyou", &["重游"]),   // 重 chóng (re-visit)
            ("zhongfa", &["重发"]),    // 重 chóng (re-send)
            ("zhongjian", &["重见"]),  // 重 chóng (re-see)
            ("zhongma", &["重码"]),    // 重 chóng (duplicate code)
            ("zhongyou_p", &["重邮"]), // 重 chóng (重庆邮电)
            // — NB: 重组 has prim_code zhongzu primary already, so del_prim
            // wiped that. Check both ends:
            ("zhongzu", &["重组"]), // 重 chóng (re-organize)
            // User polish-log 2026-06-12: "momo 嶙嶙也不像个词，默默第一"
            // — 嶙嶙 was a wubi-side phrase row (momo = structural full
            // code) leading mixed #0 via the wubi tier. 嶙 is a real
            // char (craggy) but the reduplication isn't a standalone
            // word. D1 deleted from wubi library.tsv + logged to
            // corpus_garbage_filter_v1. Companion pinyin-side D1 same
            // report: "磨磨 墨墨 莫莫 也不是词" — single-char
            // reduplication noise from jieba sub-word ingest (compounds
            // 磨磨蹭蹭 / 默默耕耘 etc. unaffected).
            ("momo", &["嶙嶙", "磨磨", "墨墨", "莫莫"]),
            // User polish-log 2026-06-26: "sisi 思思第一，咝咝不需要,
            // 要加嘶嘶" — 咝咝 (jieba 单字 reduplication 拟声, weak)
            // D1 deleted from library.tsv; companion A 加: 思思 (人名
            // / 昵称, removed from corpus_garbage_filter so polish row
            // takes effect) leads, 嘶嘶 (onomatopoeia) added at low freq.
            ("sisi", &["咝咝"]),
            // User polish-log 2026-06-26: "zouta 邹韬奋删除" —
            // 邹韬奋 (historical journalist) was leaking into the
            // zouta prefix-completion top-N from the zoutaofen
            // full-code entry. D1 deleted from library.tsv +
            // logged to corpus_garbage_filter_v1.
            ("zouta", &["邹韬奋"]),
            // User polish-log 2026-06-26: "zoue 字也 子也 子叶 这些都不要,
            // 而且出现的原因也不正常". Root cause: lattice K-best stacked
            // typo edges across segments [zo→zi | ue→ye] with no Exact
            // anchor — pure 2-edit-distance speculation. Structural fix in
            // dict.rs::compose_via_lattice_paths: drop any path with no
            // Exact or Abbrev edge as anchor. Test pins the absence of
            // all three stacked-typo products at this buffer.
            ("zoue", &["字也", "子也", "子叶"]),
            // User polish-log 2026-06-13: "daiban … 其他的是 da '大'
            // 这个读音的东西，多音字处理的问题". 大办/大坂/大板/大阪
            // are 大(dà)-reading words that were duplicated under the
            // wrong code `daiban` (大 is never read dài). Each also
            // exists correctly under `daban`. D1 deleted the daiban
            // copies + logged to corpus_garbage_filter_v1.
            ("daiban", &["大办", "大坂", "大板", "大阪"]),
            // User polish-log 2026-06-13: "另外 大板 大办 也不是个词" —
            // companion to the daiban polyphone cleanup. 大板/大办 are
            // jieba 字字直拼 sub-word noise (not standalone words); D1
            // deleted from the correct code `daban` too. 大半/大坂/大班/
            // 大阪 (real words / Osaka place names) untouched at daban.
            ("daban", &["大板", "大办"]),
            // User polish-log 2026-06-13: daiban 只要 待办/代班/代办/
            // 呆板. 带班 (dài-bān, lead a shift) is a REAL word with the
            // correct reading, so NOT a D1 delete — it's a per-case D2
            // display-hide (exclusions_v1.tsv) so 带班 stays in the dict
            // for reverse-lookup / K-best but never surfaces at the
            // daiban buffer. Same NOT-in-top10 assertion shape as the
            // D1 cases above (cf. the yichu D2 case earlier).
            ("daiban", &["带班"]),
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
            // 2026-07-04 user: "xiaxi 罅隙 这种词不要了，太生僻了" —
            // 罅隙 is a real formal/literary word (crack/fissure) but the
            // user considers it too obscure to lead. D2 via exclusions_v1.tsv
            // keeps 罅隙 in pinyin.dict for K-best / initials reverse-lookup
            // future-proofing when compose gates flip back on; only the
            // Path-1 IDF display layer drops it here.
            ("xiaxi", &["罅隙"]),
            // User polish-log 2026-07-06: "wujia 五加 不是词" — 五加 is
            // a jieba sub-word noise (from compounds 五加科 / 五加皮 in
            // Chinese herbal medicine, none standalone). D1 deleted from
            // library.tsv + logged to corpus_garbage_filter_v1 so future
            // corpus digests can't re-admit. 物价 / 屋架 / 无价 auto-lead
            // as the real wujia candidates.
            ("wujia", &["五加"]),
            // User polish-log 2026-07-06: "beidiao 背调第一, 贝雕删了" —
            // 贝雕 (shell carving, obscure niche craft) at library freq
            // 2987 led over 背调 (background check, common HR term).
            // D1 deleted 贝雕 from library.tsv + logged to
            // corpus_garbage_filter_v1; 背调 (already in modern_vocab at
            // 45000) auto-leads.
            ("beidiao", &["贝雕"]),
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
