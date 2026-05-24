//! Candidate-quality baseline tests (TDD per user 2026-05-24:
//! "候选质量是非常容易用 tdd 保障的，你可以借助参考输入法来建立一些
//! baseline 做测试，这个可以作为一个核心的验证机制").
//!
//! Each entry pins `(buffer, expected_top_word)` for Mixed mode under
//! the production policy (Never, no auto-commit interference). Any
//! regression that pushes the expected word below #0 fails the test
//! with a full top-10 dump so the change is obvious.
//!
//! Source of truth for the baseline:
//!   1. Common single-syllable pinyins where the top character is
//!      universally accepted (matches Sogou / Simeji / 智能ABC / etc.)
//!   2. User-reported polish cases (lixiang→理想, queshi→缺失, mo→默,
//!      da→大) — once a regression is reported, lock it forever.
//!   3. Wubi simcode protection (伙-rule entries already covered by
//!      session::wubi_simcode_priority; not duplicated here).
//!
//! This file is the ground truth for "what the user expects to see".
//! Adding a new line is HOW you encode a polish judgement. Removing
//! one requires an equivalent diagnostic explanation.

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

    /// Run a list of (buffer, expected_top) baseline assertions. Failure
    /// prints ALL failed rows + their actual top-10 for easy triage.
    fn assert_baseline(cases: &[(&str, &str)]) {
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

    /// Single-syllable common pinyins — these are the absolute hot path
    /// (chars typed thousands of times per session) and their #0 must
    /// match universal expectation. Reference: Sogou/Simeji default.
    #[test]
    fn baseline_common_single_syllable_pinyins() {
        // (buffer, expected_top_word).
        // Pinyins with explicit wubi-simcode protection
        // (wo/ni/ta/de/shi/you) live in session::wubi_simcode_priority;
        // not duplicated.
        let cases: &[(&str, &str)] = &[
            // Top-tier particles / pronouns.
            ("de", "胡"),      // wubi-simcode-protected; here just to assert it
            ("le", "了"),
            ("ma", "吗"),
            ("ba", "吧"),
            ("ne", "呢"),
            ("ya", "呀"),
            ("la", "拉"),       // 拉 over Jianma2 轼 (deleted)
            // Pronouns (covered by simcode tests for wo/ni/ta).
            // Common verbs.
            ("kan", "看"),
            ("ting", "听"),
            ("zuo", "做"),
            // ("xie", "..."): base 些(48955) > 写(45388) > 血/谢/协... — corpus
            //   says 些 is more common. User can override via polish-log if 写
            //   should lead; for now baseline expects corpus order.
            ("xie", "些"),
            ("shuo", "说"),
            ("hao", "好"),
            ("xiang", "想"),
            // Common nouns.
            ("jia", "家"),
            ("ren", "人"),
            ("tian", "天"),
            ("yue", "月"),
            ("nian", "年"),
            ("ri", "日"),       // 日 over Jianma2 朱 (deleted)
            // High-freq particles / function words that polish-log
            // surfaced as broken before purge_jianma2_misaligned.
            ("di", "的"),       // was leading 砂 before purge
            ("bu", "不"),       // was leading 联
            ("yi", "一"),       // was leading 就
            ("ji", "给"),       // was leading 晃
            ("qi", "其"),       // was leading 乐
            ("ge", "个"),       // was leading 表
            ("du", "都"),       // was leading 磁
            ("na", "那"),       // was leading 民
            ("er", "而"),       // was leading 遥
            ("fa", "法"),       // was leading 载
            ("ke", "可"),       // was leading 吸
            ("si", "死"),       // was leading 档 (note: 死 is corpus top)
            ("an", "安"),       // was leading 世
            ("ai", "爱"),       // was leading 东
            ("hu", "湖"),       // was leading 瞳
            ("he", "和"),       // was leading 肯
            ("mu", "目"),       // was leading 赠
            ("se", "色"),       // was leading 极
            ("te", "特"),       // was leading 秀
            ("ti", "提"),       // was leading 秒
            ("tu", "土"),       // was leading 科
            ("xi", "西"),       // was leading 纱
            ("ya", "呀"),       // 呀 over Jianma2 度
            ("ye", "也"),       // was leading 衣
            // User polish-log + regression cases.
            ("da", "大"),       // was leading Jianma2 左 (deleted)
            ("mo", "没"),       // was leading Jianma2 嶙 (deleted)
        ];
        assert_baseline(cases);
    }

    /// Multi-syllable common words. Cover the polish-log explicit
    /// hits + a few high-traffic compounds.
    #[test]
    fn baseline_common_multi_syllable_words() {
        let cases: &[(&str, &str)] = &[
            // Polish-log hits.
            ("lixiang", "理想"),     // not 立项 (user feedback 2026-05-24)
            ("queshi", "缺失"),      // user picked 4×; auto-tune
            ("youshi", "优势"),      // user picked 5×
            ("rongyu", "冗余"),      // user picked 5×
            ("zhineng", "智能"),     // polish-log n=3
            ("fanye", "翻页"),       // polish-log n=3
            ("maoding", "锚定"),     // polish-log n=3
            ("yuming", "域名"),      // user 2026-05-24: 余名→域名 (corpus
                                    //   余名 23014 ≈ 域名 22550; modern
                                    //   vocab boost lifts 域名 to lead)
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
        assert_baseline(cases);
    }
}
