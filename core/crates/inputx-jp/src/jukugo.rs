//! Compound-kanji (熟語 / jukugo) lookup. Whole-buffer match: typing
//! `nihon` → 日本; `gakkou` → 学校; `chuugoku` → 中国.
//!
//! Every member kanji of every entry is hand-verified to be codepoint-
//! identical between JP shinjitai and Simplified-CN — same rule as
//! [`crate::kanji::KANJI_TABLE`]. This means we deliberately *don't*
//! ship common JP compounds where a member kanji isn't shape-identical
//! with the CN simplified form, e.g.:
//!
//!   - 漢字 (漢 U+6F22 ≠ 汉 U+6C49)
//!   - 経済 (経 U+7D4C ≠ 经 U+7ECF)
//!   - 電車 (電 U+96FB ≠ 电 U+7535)
//!   - 東京 (東 U+6771 ≠ 东 U+4E1C)
//!   - 個人 (個 U+500B ≠ 个 U+4E2A)
//!   - 時間 (時 U+6642 ≠ 时 U+65F6; 間 U+9593 ≠ 间 U+95F4)
//!   - 後 (後 U+5F8C ≠ 后 U+540E)
//!   - 義務 (義 U+7FA9 ≠ 义 U+4E49)
//!   - 楽園 (楽 U+697D ≠ 乐 U+4E50)
//!   - 愛 (愛 U+611B ≠ 爱 U+7231)
//!
//! The user's framing was "全 pick 出与中文完全相同的", so half-matches
//! are excluded. Adding new entries: verify each char's codepoint
//! against the CN simplified form first (a quick
//! `python3 -c "print(hex(ord('X')))"` comparison vs the CN counterpart).
//!
//! Long vowels written as doubled vowels in romaji: `nihon` (not `nihoN`),
//! `kou` (not `kō`), `juu` (not `jū`). Hepburn primary.
//!
//! Lookup is whole-buffer exact-match (not prefix, not segmentation).
//! The kana renderer separately produces hiragana / katakana of the
//! same buffer; the user picks whichever via the candidate panel.

pub struct JukugoEntry {
    pub reading: &'static str,
    pub kanji: &'static str,
}

const fn j(reading: &'static str, kanji: &'static str) -> JukugoEntry {
    JukugoEntry { reading, kanji }
}

/// Grouped by semantic family. All entries hand-verified codepoint-
/// identical between JP shinjitai and Simplified-CN. See module doc.
#[rustfmt::skip]
pub const JUKUGO_TABLE: &[JukugoEntry] = &[
    // ---- 国 / 地名 -------------------------------------------------
    j("nihon",       "日本"),
    j("nippon",      "日本"),
    j("chuugoku",    "中国"),
    j("kyouto",      "京都"),
    j("hokkaidou",   "北海道"),

    // ---- 教育 -------------------------------------------------------
    j("gakkou",      "学校"),
    j("gakusei",     "学生"),
    j("sensei",      "先生"),
    j("daigaku",     "大学"),
    j("chuugaku",    "中学"),
    j("shougakkou",  "小学校"),
    j("gakusha",     "学者"),
    j("gakubu",      "学部"),
    j("bungaku",     "文学"),
    j("bunka",       "文化"),

    // ---- 家族 / 人 --------------------------------------------------
    j("kazoku",      "家族"),
    j("yuujin",      "友人"),
    j("jinkou",      "人口"),
    j("shujin",      "主人"),
    j("honnin",      "本人"),
    j("daijin",      "大臣"),
    j("bijin",       "美人"),
    j("hakujin",     "白人"),
    j("seinen",      "青年"),
    j("shounen",     "少年"),
    j("roujin",      "老人"),

    // ---- 時間 / 季節 ------------------------------------------------
    j("nichiyou",    "日曜"),
    j("getsuyou",    "月曜"),
    j("kayou",       "火曜"),
    j("suiyou",      "水曜"),
    j("mokuyou",     "木曜"),
    j("kinyou",      "金曜"),
    j("doyou",       "土曜"),
    j("kongetsu",    "今月"),
    j("raigetsu",    "来月"),
    j("rainen",      "来年"),
    j("shinnen",     "新年"),
    j("ichigatsu",   "一月"),
    j("nigatsu",     "二月"),
    j("sangatsu",    "三月"),
    j("ichinichi",   "一日"),
    j("ichinen",     "一年"),

    // ---- 数 / 量 ----------------------------------------------------
    j("ichibu",      "一部"),
    j("zenbu",       "全部"),
    j("hanbun",      "半分"),
    j("daihan",      "大半"),
    j("daibubun",    "大部分"),
    j("zentai",      "全体"),
    j("zenkoku",     "全国"),

    // ---- 国 -------------------------------------------------------
    j("kokunai",     "国内"),
    j("kokumin",     "国民"),

    // ---- 場所 / 自然 ------------------------------------------------
    j("chuushin",    "中心"),
    j("kazan",       "火山"),
    j("usui",        "雨水"),
    j("sansui",      "山水"),
    j("shinrin",     "森林"),
    j("kaisui",      "海水"),

    // ---- 状態 / 評価 ------------------------------------------------
    j("anshin",      "安心"),
    j("anzen",       "安全"),
    j("daiji",       "大事"),
    j("daihon",      "大本"),
    j("meibutsu",    "名物"),
    j("meian",       "名案"),

    // ---- 自由 / 公的 ------------------------------------------------
    j("jiyuu",       "自由"),
    j("jiritsu",     "自立"),
    j("jishin",      "自身"),
    j("kouhei",      "公平"),
    j("kousei",      "公正"),

    // ---- 古今 -------------------------------------------------------
    j("kodai",       "古代"),
    j("shinkyuu",    "新旧"),

    // ---- 道路 -------------------------------------------------------
    j("douro",       "道路"),
];

/// Returns all kanji compounds whose reading matches `s` exactly.
pub fn lookup_by_reading(s: &str) -> Vec<&'static str> {
    JUKUGO_TABLE
        .iter()
        .filter(|e| e.reading == s)
        .map(|e| e.kanji)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nihon_yields_riben() {
        let cands = lookup_by_reading("nihon");
        assert!(cands.iter().any(|&w| w == "日本"),
            "expected 日本 for nihon, got {:?}", cands);
    }

    #[test]
    fn gakkou_yields_xuexiao() {
        let cands = lookup_by_reading("gakkou");
        assert!(cands.iter().any(|&w| w == "学校"));
    }

    #[test]
    fn chuugoku_yields_zhongguo() {
        let cands = lookup_by_reading("chuugoku");
        assert!(cands.iter().any(|&w| w == "中国"));
    }

    #[test]
    fn unknown_empty() {
        assert!(lookup_by_reading("zzzz").is_empty());
        assert!(lookup_by_reading("").is_empty());
    }

    #[test]
    fn every_kanji_char_in_basic_cjk_block() {
        for e in JUKUGO_TABLE.iter() {
            for ch in e.kanji.chars() {
                let cp = ch as u32;
                assert!(
                    (0x4E00..=0x9FFF).contains(&cp),
                    "{} (entry for '{}') has char U+{:04X} outside Basic CJK",
                    e.kanji, e.reading, cp
                );
            }
        }
    }

    #[test]
    fn every_reading_is_ascii_lowercase() {
        for e in JUKUGO_TABLE.iter() {
            for b in e.reading.bytes() {
                assert!(
                    b.is_ascii_lowercase(),
                    "reading {:?} for {} has non-ASCII-lowercase byte {:#x}",
                    e.reading, e.kanji, b
                );
            }
        }
    }
}
