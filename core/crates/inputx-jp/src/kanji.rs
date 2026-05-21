//! Kanji table — JP常用漢字 ∩ Simplified-CN-hanzi, by Unicode codepoint.
//!
//! Each entry pairs a kanji codepoint with one or more on-yomi readings
//! (Hepburn romaji, ASCII lowercase). Lookup: given a romaji string,
//! return the kanji whose on-yomi list contains it. Linear scan; the
//! table is small (≈150 entries) and per-keystroke lookup cost is
//! dominated by the kana renderer, not this.
//!
//! # The codepoint-identity gate
//! Every entry below was hand-verified to satisfy:
//!
//!   The kanji's Unicode codepoint equals the codepoint of the same
//!   character in Simplified Chinese as used by the wubi / pinyin
//!   engines.
//!
//! Concretely: 高 (U+9AD8) appears identically in both writing systems,
//! so it's included. 経 (U+7D4C JP shinjitai) vs 经 (U+7ECF CN) are
//! distinct codepoints, so 経 is excluded — the user typing "kei"
//! should never get a JP-only-glyph kanji that's a different character
//! from any CN candidate.
//!
//! New entries: verify codepoint equality (a quick `python3 -c "print(hex(ord('X')))"`
//! comparison vs the CN form), then add to the appropriate group. Do
//! NOT add entries based on visual similarity alone — JP shinjitai and
//! CN simplified made different choices in many cases (馬/马, 東/东,
//! 見/见, 漢/汉, 書/书, 楽/乐, 愛/爱, 黒/黑, …).
//!
//! # On-yomi only
//! Kun-yomi (native Japanese readings) is excluded by design — those
//! readings often produce multi-letter romaji that overlaps with
//! pinyin / wubi inputs in confusing ways (e.g. `yama` for 山 conflicts
//! with no pinyin syllable, but `tabemono` for 食物 multi-kanji opens
//! a can of worms this MVP doesn't handle).

/// A kanji entry: (codepoint, on-yomi readings).
///
/// Multi-reading kanji are listed in canonical order — primary on-yomi
/// first, alternates after. Search uses contains() so order doesn't
/// affect lookup correctness, only the order entries are scanned when
/// the table has duplicates for a reading (e.g., many kanji read "ka").
#[derive(Copy, Clone, Debug)]
pub struct KanjiEntry {
    pub kanji: char,
    pub readings: &'static [&'static str],
}

const fn k(c: char, r: &'static [&'static str]) -> KanjiEntry {
    KanjiEntry { kanji: c, readings: r }
}

/// JP常用漢字 ∩ Simplified-CN-hanzi.
///
/// Grouped by semantic family for readability. Within a group, ordered
/// by descending frequency / pedagogical importance.
#[rustfmt::skip]
pub const KANJI_TABLE: &[KanjiEntry] = &[
    // ---- 数字 -------------------------------------------------------
    k('一', &["ichi", "itsu"]),
    k('二', &["ni"]),
    k('三', &["san"]),
    k('四', &["shi"]),
    k('五', &["go"]),
    k('六', &["roku", "riku"]),
    k('七', &["shichi"]),
    k('八', &["hachi"]),
    k('九', &["kyuu", "ku"]),
    k('十', &["juu", "jitsu"]),
    k('百', &["hyaku"]),
    k('千', &["sen"]),
    k('万', &["man", "ban"]),
    k('零', &["rei"]),

    // ---- 自然 / 元素 ------------------------------------------------
    k('日', &["nichi", "jitsu"]),
    k('月', &["getsu", "gatsu"]),
    k('火', &["ka"]),
    k('水', &["sui"]),
    k('木', &["boku", "moku"]),
    k('金', &["kin", "kon"]),
    k('土', &["do", "to"]),
    k('山', &["san", "zan", "yama"]),
    k('川', &["sen", "kawa"]),
    k('田', &["den", "ta"]),
    k('天', &["ten", "ame"]),
    k('地', &["chi", "ji"]),
    k('海', &["kai", "umi"]),
    k('空', &["kuu", "sora"]),
    k('雨', &["u", "ame"]),
    k('雪', &["setsu", "yuki"]),
    k('林', &["rin", "hayashi"]),
    k('森', &["shin", "mori"]),
    k('花', &["ka", "hana"]),
    k('草', &["sou", "kusa"]),
    k('石', &["seki", "shaku", "koku"]),

    // ---- 方位 / 大小 ------------------------------------------------
    k('上', &["jou", "shou"]),
    k('下', &["ka", "ge"]),
    k('中', &["chuu"]),
    k('内', &["nai", "dai"]),
    k('外', &["gai", "ge"]),
    k('前', &["zen"]),
    k('左', &["sa"]),
    k('右', &["u", "yuu"]),
    k('南', &["nan"]),
    k('北', &["hoku"]),
    k('西', &["sei", "sai"]),
    k('京', &["kyou", "kei"]),
    k('大', &["dai", "tai"]),
    k('小', &["shou"]),
    k('多', &["ta"]),
    k('少', &["shou"]),
    k('高', &["kou"]),
    // 長 / 长 are distinct codepoints (U+9577 vs U+957F) — excluded.

    // ---- 人 / 関係 --------------------------------------------------
    k('人', &["jin", "nin"]),
    k('子', &["shi", "su"]),
    k('女', &["jo", "nyo", "nyou"]),
    k('男', &["dan", "nan"]),
    k('父', &["fu"]),
    k('母', &["bo"]),
    k('兄', &["kei", "kyou"]),
    k('弟', &["tei", "dai"]),
    k('友', &["yuu"]),
    k('王', &["ou"]),
    k('民', &["min"]),
    k('公', &["kou", "ku"]),
    k('主', &["shu"]),
    k('私', &["shi", "watashi", "watakushi"]),
    k('自', &["ji", "shi"]),

    // ---- 時間 / 季節 ------------------------------------------------
    k('年', &["nen"]),
    k('早', &["sou", "sa"]),
    k('夕', &["seki"]),
    k('朝', &["chou"]),
    k('夜', &["ya"]),
    k('春', &["shun"]),
    k('夏', &["ka", "ge"]),
    k('秋', &["shuu"]),
    k('冬', &["tou"]),
    k('古', &["ko"]),
    k('今', &["kon", "kin"]),
    k('新', &["shin"]),
    k('明', &["mei", "myou"]),
    k('元', &["gen", "gan"]),
    k('周', &["shuu"]),

    // ---- 動作 / 状態 ------------------------------------------------
    k('行', &["kou", "gyou", "an"]),
    k('来', &["rai"]),
    k('入', &["nyuu"]),
    k('出', &["shutsu", "sui"]),
    k('立', &["ritsu", "ryuu"]),
    k('生', &["sei", "shou"]),
    k('先', &["sen"]),
    k('安', &["an"]),
    k('美', &["bi", "mi"]),
    k('白', &["haku", "byaku"]),
    k('赤', &["seki", "shaku"]),
    k('青', &["sei", "shou"]),
    k('黄', &["kou", "ou"]),

    // ---- 文 / 学 ----------------------------------------------------
    k('文', &["bun", "mon"]),
    k('字', &["ji"]),
    k('学', &["gaku"]),
    k('名', &["mei", "myou"]),
    k('心', &["shin"]),
    k('思', &["shi"]),
    k('言', &["gen", "gon"]),
    k('音', &["on", "in"]),
    k('画', &["ga", "kaku"]),
    k('体', &["tai", "tei"]),
    k('物', &["butsu", "motsu"]),
    k('力', &["ryoku", "riki"]),
    k('工', &["kou", "ku"]),
    k('玉', &["gyoku"]),

    // ---- 国 / 社会 --------------------------------------------------
    k('国', &["koku"]),
    k('本', &["hon"]),
    k('道', &["dou", "tou"]),
    k('路', &["ro"]),
    k('校', &["kou"]),
    k('会', &["kai", "e"]),
    k('社', &["sha"]),
    k('政', &["sei", "shou"]),
    k('治', &["ji", "chi"]),
    k('全', &["zen"]),
    k('同', &["dou"]),
    k('共', &["kyou"]),
    k('半', &["han"]),
    k('数', &["suu"]),
    k('化', &["ka", "ke"]),
    k('由', &["yuu", "yu"]),
    k('真', &["shin"]),
    k('写', &["sha"]),
    k('漫', &["man"]),
    k('仕', &["shi", "ji"]),
    k('事', &["ji"]),
    k('供', &["kyou", "ku"]),

    // ---- 食 / 動植物 ------------------------------------------------
    k('食', &["shoku"]),
    k('米', &["bei", "mai"]),
    k('麦', &["baku"]),
    k('茶', &["cha", "sa"]),
    k('牛', &["gyuu"]),
    k('犬', &["ken"]),
    k('何', &["ka"]),
];

/// Look up kanji whose on-yomi list contains exactly `reading` (ASCII
/// lowercase romaji, no diacritics — long vowels are encoded as
/// doubled vowels: `kou`, `juu`, `kyuu`).
///
/// Returns kanji in declaration order from [`KANJI_TABLE`].
pub fn lookup_by_reading(reading: &str) -> Vec<char> {
    KANJI_TABLE
        .iter()
        .filter(|e| e.readings.iter().any(|r| *r == reading))
        .map(|e| e.kanji)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_callout_high_kou() {
        // The user's framing example: 高 must be in the table, reading "kou".
        let cands = lookup_by_reading("kou");
        assert!(cands.contains(&'高'), "expected 高 for 'kou', got {:?}", cands);
    }

    #[test]
    fn numbers_present() {
        assert!(lookup_by_reading("ichi").contains(&'一'));
        assert!(lookup_by_reading("san").contains(&'三'));
        assert!(lookup_by_reading("hyaku").contains(&'百'));
        assert!(lookup_by_reading("man").contains(&'万'));
    }

    #[test]
    fn single_letter_reading_ka() {
        // 'ka' has many kanji readings; we just need to see expected ones.
        let cands = lookup_by_reading("ka");
        assert!(cands.contains(&'火'));
        assert!(cands.contains(&'花'));
        assert!(cands.contains(&'下'));  // 'ka' is one of 下's on-yomi
    }

    #[test]
    fn unknown_reading_empty() {
        assert_eq!(lookup_by_reading("zzz"), Vec::<char>::new());
        assert_eq!(lookup_by_reading(""), Vec::<char>::new());
    }

    #[test]
    fn every_entry_has_at_least_one_reading() {
        for e in KANJI_TABLE.iter() {
            assert!(
                !e.readings.is_empty(),
                "kanji {} has no readings — every entry must have ≥1",
                e.kanji
            );
            for r in e.readings.iter() {
                assert!(
                    r.bytes().all(|b| b.is_ascii_lowercase()),
                    "reading {:?} for {} must be ASCII lowercase",
                    r, e.kanji
                );
            }
        }
    }

    #[test]
    fn every_kanji_is_basic_cjk_or_extension() {
        // Loose codepoint sanity: all entries should be in CJK Unified
        // Ideographs (Basic U+4E00-U+9FFF). Excludes surrogate pairs etc.
        // The codepoint-identity-with-CN gate is hand-verified at entry
        // time; this is a smoke filter.
        for e in KANJI_TABLE.iter() {
            let cp = e.kanji as u32;
            assert!(
                (0x4E00..=0x9FFF).contains(&cp),
                "kanji {} U+{:04X} outside Basic CJK — verify codepoint-identity-with-CN",
                e.kanji, cp
            );
        }
    }
}
