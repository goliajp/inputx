//! Candidate-quality baseline tests (TDD per user 2026-05-24:
//! "候选质量是非常容易用 tdd 保障的，你可以借助参考输入法来建立一些
//! baseline 做测试，这个可以作为一个核心的验证机制").
//!
//! Each entry pins `(buffer, expected_top_word)` for Mixed mode under
//! the production policy (Never, no auto-commit interference). Any
//! regression that pushes the expected word below #0 fails the test
//! with a full top-10 dump so the change is obvious.
//!
//! IMPORTANT — what we DON'T test here:
//!   Single-letter and 2-letter buffers that overlap a wubi 简码
//!   (Jianma1/Jianma2/Jianma3) are intentionally NOT asserted at
//!   the pinyin-top level. Per the 伙-rule (session::wubi_simcode_
//!   priority), wubi simcodes with common-word targets MUST lead
//!   their codes in Mixed mode — that's a core Inputx promise (五笔
//!   IME first). The user has explicitly reinforced this 2026-05-24:
//!   "五笔二级简码的常用词，应该评分是非常高的".
//!
//!   So `di → 砂`, `da → 左`, `ge → 表`, etc. are CORRECT (those are
//!   wubi simcodes). If a SPECIFIC Jianma2 entry surfaces a truly
//!   rare char (e.g. `mo → 嶙`), the fix is data — delete that single
//!   entry from jianma_simplified.txt — never a blanket rule.
//!
//!   What this file pins: MULTI-syllable pinyin codes (no wubi Jianma
//!   collision possible — Jianma is 1-3 letters), polish-log hits,
//!   and JP-enabled pinyin-top vs JP-top ordering.

#[cfg(test)]
#[cfg(not(feature = "bootstrap_only"))]
mod tests {
    use crate::composite::engine::CompositeEngine;
    use crate::composite::Mode;
    use crate::wubi::AutoCommitPolicy;

    fn mixed_top(buffer: &[u8]) -> Vec<String> {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in buffer { let _ = e.handle_letter(*b); }
        e.candidates().iter().take(10).map(|c| c.word.clone()).collect()
    }

    fn pinyin_top(buffer: &[u8]) -> Vec<String> {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in buffer { let _ = e.handle_letter(*b); }
        e.candidates().iter().take(10).map(|c| c.word.clone()).collect()
    }

    fn assert_baseline_mixed(cases: &[(&str, &str)]) {
        let mut failures: Vec<String> = Vec::new();
        for (buf, expected) in cases {
            let top10 = mixed_top(buf.as_bytes());
            let actual = top10.first().cloned().unwrap_or_default();
            if actual != *expected {
                failures.push(format!(
                    "  {buf:<10} expected #0 = {expected}, got {actual}  (top10={top10:?})"
                ));
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} baseline-quality cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    fn assert_baseline_pinyin_only(cases: &[(&str, &str)]) {
        let mut failures: Vec<String> = Vec::new();
        for (buf, expected) in cases {
            let top10 = pinyin_top(buf.as_bytes());
            let actual = top10.first().cloned().unwrap_or_default();
            if actual != *expected {
                failures.push(format!(
                    "  (PinyinOnly) {buf:<10} expected #0 = {expected}, got {actual}  (top10={top10:?})"
                ));
            }
        }
        if !failures.is_empty() {
            panic!(
                "{} PinyinOnly baseline cases failed:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }

    /// Multi-syllable pinyins — no wubi Jianma overlap (Jianma is 1-3
    /// letters; multi-syllable codes are 4+). These are pure pinyin
    /// assertions safe in Mixed mode.
    #[test]
    fn baseline_multi_syllable_pinyins() {
        let cases: &[(&str, &str)] = &[
            // Polish-log hits.
            ("lixiang", "理想"),     // not 立项 (user 2026-05-24)
            ("queshi", "缺失"),      // user picked 4×
            ("youshi", "优势"),      // user picked 5×
            ("rongyu", "冗余"),      // user picked 5×
            ("zhineng", "智能"),     // polish-log n=3
            ("fanye", "翻页"),       // polish-log n=3
            ("maoding", "锚定"),     // polish-log n=3
            ("yuming", "域名"),      // user 2026-05-24 (modern_vocab boost)
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
        ];
        assert_baseline_mixed(cases);
    }

    /// PinyinOnly mode: no wubi competition, so pinyin top must lead
    /// at every common single-syllable code. This is the user's
    /// "pure pinyin intent" experience — locks in scoring polish.
    #[test]
    fn baseline_pinyin_only_single_syllable() {
        let cases: &[(&str, &str)] = &[
            ("de", "的"),
            ("le", "了"),
            ("ma", "吗"),
            ("ba", "吧"),
            ("ne", "呢"),
            ("ya", "呀"),
            ("la", "拉"),
            ("kan", "看"),
            ("ting", "听"),
            ("zuo", "做"),
            ("shuo", "说"),
            ("hao", "好"),
            ("xiang", "想"),
            ("jia", "家"),
            ("ren", "人"),
            ("tian", "天"),
            ("yue", "月"),
            ("nian", "年"),
            ("ri", "日"),
            ("di", "的"),
            ("bu", "不"),
            ("yi", "一"),
            ("ge", "个"),       // PinyinOnly: 个 leads (no wubi 表)
            ("da", "大"),       // PinyinOnly: 大 leads (no wubi 左)
            ("mo", "没"),
        ];
        assert_baseline_pinyin_only(cases);
    }
}
