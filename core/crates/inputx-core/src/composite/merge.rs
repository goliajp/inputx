//! Candidate merge — combine wubi + pinyin + (optionally) Japanese
//! candidate lists with source attribution and dedup.

/// Engine that produced a candidate. Surfaced to the iOS UI for the
/// W/P/J indicator dot; FFI returns this as `u8`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Wubi = 0,
    Pinyin = 1,
    Japanese = 2,
}

impl Source {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Wubi),
            1 => Some(Self::Pinyin),
            2 => Some(Self::Japanese),
            _ => None,
        }
    }
}

/// One candidate with its source engine + unified score. The score is
/// produced by the engine's `*_with_scores` API and is comparable
/// across sources after the engine has applied its `engine_mult` /
/// `layer_floor` calibration. The composite merge sorts by score
/// desc; ties keep the first-seen source.
///
/// Equality intentionally ignores `score` so legacy tests that
/// compare `Candidate { word, source }` literals still match — score
/// is a sort key, not part of identity.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub word: String,
    pub source: Source,
    pub score: f64,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.word == other.word && self.source == other.source
    }
}
impl Eq for Candidate {}

/// Maximum candidates retained in the merged list. iOS candidate bar
/// paginates via swipe (item 55), but capping prevents pathological
/// inputs from blowing memory.
pub const MAX_PER_INPUT: usize = 50;

/// Slots reserved at the *end* of the merged list for low-conviction
/// JP kana candidates (always-available fallback regardless of whether
/// the buffer is a known JP word). 4 is enough for hiragana + katakana
/// + maybe small/voiced variants without crowding out pinyin bulk.
pub const JP_KANA_RESERVE: usize = 4;

/// Merge wubi + JP kanji + pinyin + JP kana into a single candidate list.
///
/// # Current ordering rule (Mixed mode, JP-enabled)
///
///   1. **Wubi** — the product is "Inputx 五笔"; wubi outputs lead.
///      Within wubi, jianma1 / jianma2 layer_base dominates per the
///      wubi dict's existing sort, so 一级 / 二级简码 always top.
///   2. **JP kanji** (jukugo compounds + on/kun single-kanji matches) —
///      full-buffer matches only, sits above pinyin bulk so common JP
///      words like `nihon` → 日本 don't get drowned in pinyin fuzz.
///   3. **Pinyin** — Chinese fuzzy / phonetic matches.
///   4. **JP kana** — hiragana + katakana renderings of whatever romaji
///      the user typed. Mechanically derivable from any buffer,
///      low-conviction; reserved last so pinyin bulk stays visible.
///
/// # Design note: future "unified score" v0.2 ↓ direction
///
/// The user's principle is **"公允的同数值化比较"** — a single normalized
/// score across all engines, sort by that, with wubi getting a
/// brand-loyalty boost (since "Inputx 五笔"). Today's hard layered
/// merge (wubi → JP kanji → pinyin → JP kana) is a *placeholder* for
/// that:
///
///   - Each engine produces (word, freq) with engine-specific scales
///     (wubi 0-50k, pinyin similar, JP currently has none).
///   - To unify: normalize freq to [0, 1] per engine (percentile or
///     log-rank), apply per-engine multiplier (wubi 1.2, pinyin 1.0,
///     JP 0.9, etc.), sort by `score = normalized * multiplier`.
///   - Wubi 一级 / 二级简码 floors stay enforced via layer_base in
///     the wubi dict (already the case) — they'd land on top of the
///     unified score by virtue of having highest absolute freq + the
///     wubi multiplier.
///
/// Not implemented yet because: (a) the JP plugin has no real freq
/// data (hand-curated tables, all entries weighted equally), (b)
/// cross-engine normalization is a calibration project that wants
/// the polish-log corpus to validate against. See PolishLog telemetry
/// on mac/iOS for the data-collection side.
///
/// Duplicates by `word` keep the first-seen source attribution.
///
/// Whichever vec the caller hands in empty (e.g. JP toggle off → both
/// jp_kanji and jp_kana empty) is a no-op for that group.
/// Unified-score merge. Takes scored candidates from each engine, sorts
/// by score desc, dedupes by word (first-seen source attribution).
///
/// The score is set by the engine adapter — see
/// `WubiEngine::candidates_with_scores`, `PinyinAdapter::candidates_with_scores`,
/// `JapaneseAdapter::candidates_with_scores`. The engine encodes its
/// own multipliers (wubi simcode floor via `layer.base`, L0 pin via
/// score boost, JP per-kind synthetic scores). Cross-engine ordering
/// is then *just sorting numbers* — no hard rule lives here.
///
/// Source attribution preserved on dedup: if `wubi` and `pinyin` both
/// produce 我们, the higher-scored one wins position; if scores tie,
/// the first one encountered (wubi by iteration order) wins both
/// position and `Source::Wubi` tag.
/// Strict TC-only character set, derived from OpenCC's t2s mapping —
/// every CJK char in [U+4E00, U+9FA0) whose TC form differs from its
/// SC form (3549 entries). Built from the `data/tc_chars_demote.txt`
/// file via `include_str!`.
///
/// Score penalty is multiplicative × 1e-3 so TC candidates still
/// surface at the tail when no SC equivalent exists, but never beat
/// their SC siblings at the same input.
///
/// IMPORTANT: contains ONLY chars whose simplified form differs from
/// the traditional form. Shared chars (公 / 司 / 学 / 时 — same glyph
/// in both registers) are excluded by OpenCC's t2s definition (it
/// only emits entries where input ≠ output).
/// OpenCC t2s-derived (3549 chars). Single newline-separated string;
/// the demote checker iterates char-by-char (line breaks ignored
/// because the file is one-char-per-line).
const TC_DEMOTE_FULL: &str = include_str!("../../data/tc_chars_demote.txt");

/// Legacy hand-curated list — kept for reviewability + as backup if
/// the OpenCC file is ever stripped. The full check uses TC_DEMOTE_FULL.
#[allow(dead_code)]
const TC_DEMOTE_CHARS: &str = concat!(
    "頁",  // 页
    "國",  // 国
    "經",  // 经
    "學",  // 学
    "體",  // 体
    "後",  // 后
    "個",  // 个
    "樣",  // 样
    "變",  // 变
    "風",  // 风
    "種",  // 种
    "點",  // 点
    "達",  // 达
    "過",  // 过
    "還",  // 还
    "進",  // 进
    "這",  // 这
    "麼",  // 么
    "開",  // 开
    "關",  // 关
    "問",  // 问
    "題",  // 题
    "們",  // 们
    "發",  // 发
    "說",  // 说
    "讓",  // 让
    "給",  // 给
    "話",  // 话
    "寫",  // 写
    "聽",  // 听
    "當",  // 当
    "際",  // 际
    "樂",  // 乐
    "業",  // 业
    "師",  // 师
    "參",  // 参
    "與",  // 与
    "資",  // 资
    "產",  // 产
    "務",  // 务
    "員",  // 员
    "應",  // 应
    "該",  // 该
    "總",  // 总
    "統",  // 统
    "舉",  // 举
    "辦",  // 办
    "會",  // 会
    "議",  // 议
    "圖",  // 图
    "書",  // 书
    "畫",  // 画
    "媽",  // 妈
    "親",  // 亲
    "愛",  // 爱
    "聲",  // 声
    "響",  // 响
    "繪",  // 绘
    "認",  // 认
    "識",  // 识
    "記",  // 记
    "憶",  // 忆
    "夢",  // 梦
    "覺",  // 觉
    "鐵",  // 铁
    "車",  // 车
    "場",  // 场
    "馬",  // 马
    "電",  // 电
    "腦",  // 脑
    "軟",  // 软
    "網",  // 网
    "絡",  // 络
    "線",  // 线
    "灣",  // 湾
    "島",  // 岛
    "嶼",  // 屿
    "鄉",  // 乡
    "莊",  // 庄
    "頭",  // 头
    "淚",  // 泪
    "錢",  // 钱
    "價",  // 价
    "買",  // 买
    "賣",  // 卖
    "質",  // 质
    "傳",  // 传
    "節",  // 节
    "氣",  // 气
    "養",  // 养
    "緒",  // 绪
    "醫",  // 医
    "療",  // 疗
    "藥",  // 药
    "處",  // 处
    "劑",  // 剂
    "戶",  // 户
    "裡",  // 里
    "裏",  // 里
    "內",  // 内
    "飯",  // 饭
    "館",  // 馆
    "飲",  // 饮
    "鋪",  // 铺
    "舖",  // 铺
    "營",  // 营
    "歡",  // 欢
    "臨",  // 临
    "鎮",  // 镇
    "縣",  // 县
    "結",  // 结
    "構",  // 构
    "協",  // 协
    "權",  // 权
    "藝",  // 艺
    "術",  // 术
    "劇",  // 剧
    "戲",  // 戏
    "詞",  // 词
    "詩",  // 诗
    "廳",  // 厅
    "緊",  // 紧
    "張",  // 张
    "壓",  // 压
    "釋",  // 释
    "鬆",  // 松
    "寢",  // 寝
    "導",  // 导
    "輔",  // 辅
    "練",  // 练
    "習",  // 习
    "慣",  // 惯
    "貨",  // 货
    "幣",  // 币
    "銀",  // 银
    "儲",  // 储
    "黃",  // 黄
    "鈔",  // 钞
    "賬",  // 账
    "碼",  // 码
    "編",  // 编
    "輯",  // 辑
    "華",  // 华
    "麗",  // 丽
    "從",  // 从
    "標",  // 标
    "準",  // 准
    "確",  // 确
    "實",  // 实
    "見",  // 见
    "東",  // 东
    "區",  // 区
    "兒",  // 儿
    "兩",  // 两
    "幾",  // 几
    "報",  // 报
    "紙",  // 纸
    "選",  // 选
    "擇",  // 择
    "顯",  // 显
    "對",  // 对
    "錯",  // 错
    "覽",  // 览
    "視",  // 视
    "覺",  // 觉
    "聞",  // 闻
    "聲",  // 声
    "驚",  // 惊
    "嚇",  // 吓
    "懼",  // 惧
);

/// HashSet-backed TC check. Built once on first use from the 3549-char
/// OpenCC-derived list. O(1) per char vs `&str::contains` which is O(N).
fn tc_chars() -> &'static std::collections::HashSet<char> {
    use std::sync::OnceLock;
    static SET: OnceLock<std::collections::HashSet<char>> = OnceLock::new();
    SET.get_or_init(|| {
        TC_DEMOTE_FULL
            .chars()
            .filter(|&c| !c.is_whitespace())
            .collect()
    })
}

fn contains_demote_tc(word: &str) -> bool {
    let set = tc_chars();
    word.chars().any(|c| set.contains(&c))
}

pub fn merge(
    wubi: Vec<(String, f64)>,
    pinyin: Vec<(String, f64)>,
    jp_kanji: Vec<(String, f64)>,
    jp_kana: Vec<(String, f64)>,
) -> Vec<Candidate> {
    let total_hint = wubi.len() + pinyin.len() + jp_kanji.len() + jp_kana.len();
    let mut all: Vec<Candidate> = Vec::with_capacity(total_hint);
    use crate::composite::scoring::TC_DEMOTE_MULTIPLIER;
    let demote = |w: &str, s: f64| -> f64 {
        if contains_demote_tc(w) { s * TC_DEMOTE_MULTIPLIER } else { s }
    };
    for (w, s) in wubi {
        let s = demote(&w, s);
        all.push(Candidate { word: w, source: Source::Wubi, score: s });
    }
    for (w, s) in pinyin {
        let s = demote(&w, s);
        all.push(Candidate { word: w, source: Source::Pinyin, score: s });
    }
    for (w, s) in jp_kanji {
        // JP candidates are explicitly JP — TC demote doesn't apply
        // (whether a JP kanji happens to share form with TC is fine).
        all.push(Candidate { word: w, source: Source::Japanese, score: s });
    }
    for (w, s) in jp_kana {
        all.push(Candidate { word: w, source: Source::Japanese, score: s });
    }
    // Stable sort by score desc — ties keep input order (wubi first).
    all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    // Dedupe by word — first-seen wins (higher score after sort).
    let mut seen = std::collections::HashSet::with_capacity(total_hint.min(MAX_PER_INPUT));
    let mut out: Vec<Candidate> = Vec::with_capacity(total_hint.min(MAX_PER_INPUT));
    for c in all {
        if out.len() >= MAX_PER_INPUT { break; }
        if seen.insert(c.word.clone()) {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(word: &str, score: f64) -> (String, f64) {
        (word.into(), score)
    }

    #[test]
    fn empty_inputs_yield_empty() {
        assert!(merge(vec![], vec![], vec![], vec![]).is_empty());
    }

    #[test]
    fn wubi_simcode_score_dominates() {
        // 有 wubi Jianma1 score ≈ 1.05M; 会 (JP "e" on-yomi) 200k.
        // Hard-rule simcode floor is enforced by score, not by layered
        // concat — the number wins.
        let m = merge(
            vec![s("有", 1_045_000.0)],
            vec![],
            vec![s("会", 200_000.0)],
            vec![],
        );
        assert_eq!(m[0].word, "有");
        assert_eq!(m[0].source, Source::Wubi);
    }

    #[test]
    fn pinyin_top_phrase_beats_wubi_phrase_via_score() {
        // jixu collision: wubi 曳光弹 Phrase 400k+7269 = 407k, pinyin
        // 继续 Phrase-base 400k+44652 = 445k. Pinyin wins on score.
        let m = merge(
            vec![s("曳光弹", 407_269.0)],
            vec![s("继续", 444_652.0)],
            vec![],
            vec![],
        );
        assert_eq!(m[0].word, "继续");
        assert_eq!(m[0].source, Source::Pinyin);
    }

    #[test]
    fn pin_score_multiplier_dominates() {
        // L0 pin × 1000 lifts the pinned word above any wubi simcode.
        let m = merge(
            vec![s("有", 1_045_000.0)],
            vec![s("继续", 444_652.0 * 1000.0)],
            vec![],
            vec![],
        );
        assert_eq!(m[0].word, "继续");
    }

    #[test]
    fn jp_jukugo_above_wubi_auto_below_pinyin_top() {
        // JP jukugo synthetic = 300k. Wubi Auto top ≈ 107k. Pinyin
        // top ≈ 445k. JP fits between.
        let m = merge(
            vec![s("两", 107_000.0)],
            vec![s("中国", 445_000.0)],
            vec![s("日本", 300_000.0)],
            vec![s("にほん", 100_000.0)],
        );
        assert_eq!(m[0].word, "中国");
        assert_eq!(m[1].word, "日本");
        assert_eq!(m[2].word, "两");
        assert_eq!(m[3].word, "にほん");
    }

    #[test]
    fn dedup_keeps_highest_score_source() {
        // 你 produced by both wubi (500k) and pinyin (440k) — wubi
        // wins via higher score; pinyin entry dropped.
        let m = merge(
            vec![s("你", 500_000.0)],
            vec![s("你", 440_000.0)],
            vec![],
            vec![],
        );
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].source, Source::Wubi);
    }

    #[test]
    fn cap_at_max_per_input() {
        let many: Vec<(String, f64)> = (0..MAX_PER_INPUT * 2)
            .map(|i| (i.to_string(), 100.0))
            .collect();
        let m = merge(many.clone(), many.clone(), many.clone(), many);
        assert_eq!(m.len(), MAX_PER_INPUT);
    }

    #[test]
    fn source_round_trip_u8() {
        assert_eq!(Source::Wubi.as_u8(), 0);
        assert_eq!(Source::Pinyin.as_u8(), 1);
        assert_eq!(Source::Japanese.as_u8(), 2);
        assert_eq!(Source::from_u8(0), Some(Source::Wubi));
        assert_eq!(Source::from_u8(1), Some(Source::Pinyin));
        assert_eq!(Source::from_u8(2), Some(Source::Japanese));
        assert_eq!(Source::from_u8(99), None);
    }
}
