//! Canonical Mandarin syllable inventory.
//!
//! 403 valid Hanyu Pinyin syllables (no tone marks, lowercase). Source:
//! Wikipedia "Pinyin table" + ISO 7098. Used by the segmenter to validate
//! splits and reject impossible inputs.
//!
//! Sources cite anywhere from ~400 to ~420 depending on whether marginal
//! forms (e.g. `zhei` informal for 这, `lo` particle, `kei`, `rua`) are
//! included. v0.1 ships the strict standard 403 — adding marginal forms is
//! a v0.2 follow-up once we see them in real corpus data.
//!
//! The j/q/x/y series writes ü as `u` (canonical: `jue`, `que`, `xue`, `yue`).
//! For n/l with ü, both the canonical Unicode form and the QWERTY-input form
//! are accepted: `nü` is typed as `nv`, `lü` as `lv` etc. v0.1 only ships the
//! `v` form (real input behavior); the segmenter / engine never see `ü` from
//! the keyboard.

/// All valid Mandarin syllables in Hanyu Pinyin orthography (no tones,
/// lowercase, ü → `v` for n/l). Plain slice + linear membership (zero-dep,
/// replaces the former `phf::Set`). 403 entries, probed a few dozen times
/// per keystroke — negligible vs the perfgate budget.
pub static VALID_SYLLABLES: &[&str] = &[
    // null-initial vowel-only
    "a", "ai", "an", "ang", "ao",
    "e", "ei", "en", "eng", "er",
    "o", "ou",
    // y/w semi-vowel series
    "yi", "ya", "ye", "yao", "you", "yan", "yin", "yang", "ying", "yong",
    "yu", "yue", "yuan", "yun",
    "wu", "wa", "wo", "wai", "wei", "wan", "wen", "wang", "weng",
    // b
    "ba", "bo", "bai", "bei", "bao", "ban", "ben", "bang", "beng",
    "bi", "bie", "biao", "bian", "bin", "bing",
    "bu",
    // p
    "pa", "po", "pai", "pei", "pao", "pou", "pan", "pen", "pang", "peng",
    "pi", "pie", "piao", "pian", "pin", "ping",
    "pu",
    // m
    "ma", "mo", "me", "mai", "mei", "mao", "mou", "man", "men", "mang", "meng",
    "mi", "mie", "miao", "miu", "mian", "min", "ming",
    "mu",
    // f
    "fa", "fo", "fei", "fou", "fan", "fen", "fang", "feng",
    "fu",
    // d
    "da", "de", "dai", "dei", "dao", "dou", "dan", "den", "dang", "deng", "dong",
    "di", "die", "diao", "diu", "dian", "ding",
    "du", "duo", "dui", "duan", "dun",
    // t
    "ta", "te", "tai", "tao", "tou", "tan", "tang", "teng", "tong",
    "ti", "tie", "tiao", "tian", "ting",
    "tu", "tuo", "tui", "tuan", "tun",
    // n
    "na", "ne", "nai", "nei", "nao", "nou", "nan", "nen", "nang", "neng", "nong",
    "ni", "nie", "niao", "niu", "nian", "nin", "niang", "ning",
    "nu", "nue", "nuo", "nuan",
    "nv", "nve",
    // l
    "la", "le", "lai", "lei", "lao", "lou", "lan", "lang", "leng", "long",
    "li", "lia", "lie", "liao", "liu", "lian", "lin", "liang", "ling",
    "lu", "lue", "luo", "luan", "lun",
    "lv", "lve",
    // g
    "ga", "ge", "gai", "gei", "gao", "gou", "gan", "gen", "gang", "geng", "gong",
    "gu", "gua", "guo", "guai", "gui", "guan", "gun", "guang",
    // k
    "ka", "ke", "kai", "kao", "kou", "kan", "ken", "kang", "keng", "kong",
    "ku", "kua", "kuo", "kuai", "kui", "kuan", "kun", "kuang",
    // h
    "ha", "he", "hai", "hei", "hao", "hou", "han", "hen", "hang", "heng", "hong",
    "hu", "hua", "huo", "huai", "hui", "huan", "hun", "huang",
    // j
    "ji", "jia", "jie", "jiao", "jiu", "jian", "jin", "jiang", "jing", "jiong",
    "ju", "jue", "juan", "jun",
    // q
    "qi", "qia", "qie", "qiao", "qiu", "qian", "qin", "qiang", "qing", "qiong",
    "qu", "que", "quan", "qun",
    // x
    "xi", "xia", "xie", "xiao", "xiu", "xian", "xin", "xiang", "xing", "xiong",
    "xu", "xue", "xuan", "xun",
    // zh
    "zha", "zhe", "zhi", "zhai", "zhao", "zhou", "zhan", "zhen", "zhang", "zheng", "zhong",
    "zhu", "zhua", "zhuo", "zhuai", "zhui", "zhuan", "zhun", "zhuang",
    // ch
    "cha", "che", "chi", "chai", "chao", "chou", "chan", "chen", "chang", "cheng", "chong",
    "chu", "chuo", "chuai", "chui", "chuan", "chun", "chuang",
    // sh
    "sha", "she", "shi", "shai", "shei", "shao", "shou", "shan", "shen", "shang", "sheng",
    "shu", "shua", "shuo", "shuai", "shui", "shuan", "shun", "shuang",
    // r
    "re", "ri", "rao", "rou", "ran", "ren", "rang", "reng", "rong",
    "ru", "ruo", "rui", "ruan", "run",
    // z
    "za", "ze", "zi", "zai", "zei", "zao", "zou", "zan", "zen", "zang", "zeng", "zong",
    "zu", "zuo", "zui", "zuan", "zun",
    // c
    "ca", "ce", "ci", "cai", "cao", "cou", "can", "cen", "cang", "ceng", "cong",
    "cu", "cuo", "cui", "cuan", "cun",
    // s
    "sa", "se", "si", "sai", "sao", "sou", "san", "sen", "sang", "seng", "song",
    "su", "suo", "sui", "suan", "sun",
];

/// `true` iff `s` is a valid Pinyin syllable. Case-insensitive on ASCII.
pub fn is_valid(s: &str) -> bool {
    if s.is_ascii() && s.bytes().all(|b| b.is_ascii_lowercase()) {
        VALID_SYLLABLES.contains(&s)
    } else {
        VALID_SYLLABLES.contains(&s.to_ascii_lowercase().as_str())
    }
}

/// Total number of syllables in the inventory.
pub fn count() -> usize {
    VALID_SYLLABLES.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_is_405() {
        // 405 = 403 strict standard inventory + 2 alias forms (lue, nue)
        // for `lüe`/`nüe` so users typing `celue`/`nuedai` get the same
        // candidates as `celve`/`nvedai`. The dict still stores under
        // `lve`/`nve` only; `lower_str` normalizes `lue→lve`, `nue→nve`
        // at lookup so the two spellings collapse to one storage key.
        // Marginal forms (zhei, lo, kei, rua) tracked for v0.2 once
        // corpus data shows real-world usage.
        assert_eq!(count(), 405, "expected 405 syllables (403 canonical + lue/nue aliases)");
    }

    #[test]
    fn common_syllables_recognized() {
        for s in [
            "wo",
            "ni",
            "ta",
            "shi",
            "zhong",
            "guo",
            "zhongguo".strip_suffix("guo").unwrap(),
            "hao",
            "xie",
            "jin",
            "ming",
            "tian",
        ] {
            assert!(is_valid(s), "{s:?} should be valid");
        }
    }

    #[test]
    fn nonsense_rejected() {
        for s in ["xx", "qz", "rqx", "hmm", "vroom"] {
            assert!(!is_valid(s), "{s:?} should be rejected");
        }
    }

    #[test]
    fn case_insensitive() {
        assert!(is_valid("WO"));
        assert!(is_valid("Zhong"));
    }

    #[test]
    fn v_form_for_nlu() {
        assert!(is_valid("nv"));
        assert!(is_valid("lv"));
        assert!(is_valid("nve"));
        assert!(is_valid("lve"));
    }

    #[test]
    fn lue_nue_alias_form_recognized() {
        // User-friendly aliases for `lüe` / `nüe` — most modern IMEs
        // (Sogou, Google Pinyin) accept both `lue/lve` and `nue/nve`.
        // Dict lookup normalizes back to lve/nve, but segmenter MUST
        // accept the lue/nue spelling here or buffers like `celue`
        // can't be split into `ce + lue`.
        assert!(is_valid("lue"));
        assert!(is_valid("nue"));
    }

    #[test]
    fn jqx_use_u_not_v() {
        // ü-after-j/q/x/y is canonically written `u`.
        assert!(is_valid("ju"));
        assert!(is_valid("qu"));
        assert!(is_valid("xu"));
        assert!(is_valid("yu"));
    }
}
