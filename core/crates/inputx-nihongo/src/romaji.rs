//! Romaji → kana conversion. Hepburn primary; kunrei alternates accepted
//! so users can type `si`/`ti`/`tu`/`hu`/`zi` and get the canonical kana.
//!
//! The table is sorted by pattern-length DESC so the greedy parser can
//! iterate in declaration order and take the first match. Adding new
//! entries: insert in the right length bucket; same-length order is
//! significant only when two patterns share a prefix (longer table
//! pattern always wins regardless of order within bucket).

/// One romaji pattern with its hiragana + katakana renderings.
///
/// `kana_h` and `kana_k` are full UTF-8 strings (not single chars) so
/// 拗音 digraphs like `kya` → `きゃ` (two codepoints) fit in the table
/// shape as easily as the basic 五十音.
#[derive(Copy, Clone, Debug)]
struct Entry {
    rom: &'static str,
    kana_h: &'static str,
    kana_k: &'static str,
}

// Convenience helper to keep the const table readable.
const fn e(rom: &'static str, h: &'static str, k: &'static str) -> Entry {
    Entry {
        rom,
        kana_h: h,
        kana_k: k,
    }
}

/// Greedy-match table. **Length-descending order.** First match wins.
#[rustfmt::skip]
const TABLE: &[Entry] = &[
    // ---- 4-letter patterns (kunrei small-tsu variants) -------------
    e("xtsu", "っ", "ッ"),
    e("ltsu", "っ", "ッ"),

    // ---- 3-letter patterns: 拗音 + xtu / ltu --------------------------
    e("kya", "きゃ", "キャ"), e("kyu", "きゅ", "キュ"), e("kyo", "きょ", "キョ"),
    e("gya", "ぎゃ", "ギャ"), e("gyu", "ぎゅ", "ギュ"), e("gyo", "ぎょ", "ギョ"),
    e("sha", "しゃ", "シャ"), e("shu", "しゅ", "シュ"), e("sho", "しょ", "ショ"),
    e("sya", "しゃ", "シャ"), e("syu", "しゅ", "シュ"), e("syo", "しょ", "ショ"),
    e("shi", "し",   "シ"),
    e("cha", "ちゃ", "チャ"), e("chu", "ちゅ", "チュ"), e("cho", "ちょ", "チョ"),
    e("tya", "ちゃ", "チャ"), e("tyu", "ちゅ", "チュ"), e("tyo", "ちょ", "チョ"),
    e("chi", "ち",   "チ"),
    e("tsu", "つ",   "ツ"),
    e("jya", "じゃ", "ジャ"), e("jyu", "じゅ", "ジュ"), e("jyo", "じょ", "ジョ"),
    e("nya", "にゃ", "ニャ"), e("nyu", "にゅ", "ニュ"), e("nyo", "にょ", "ニョ"),
    e("hya", "ひゃ", "ヒャ"), e("hyu", "ひゅ", "ヒュ"), e("hyo", "ひょ", "ヒョ"),
    e("bya", "びゃ", "ビャ"), e("byu", "びゅ", "ビュ"), e("byo", "びょ", "ビョ"),
    e("pya", "ぴゃ", "ピャ"), e("pyu", "ぴゅ", "ピュ"), e("pyo", "ぴょ", "ピョ"),
    e("mya", "みゃ", "ミャ"), e("myu", "みゅ", "ミュ"), e("myo", "みょ", "ミョ"),
    e("rya", "りゃ", "リャ"), e("ryu", "りゅ", "リュ"), e("ryo", "りょ", "リョ"),
    e("xtu", "っ",   "ッ"),
    e("ltu", "っ",   "ッ"),
    e("xya", "ゃ",   "ャ"),  e("xyu", "ゅ",   "ュ"),  e("xyo", "ょ",   "ョ"),

    // ---- 3-letter foreign-loanword extensions (mozc/Google IME standard) ----
    // Without these, romaji like `famiriaare` (ファミリアアレ) renders as
    // `fあみりああれ` — the leading `f` has no `fa` map, falls through as
    // ASCII passthrough, and is_jp_clean rejects the whole candidate.
    // fa-row yoon (foreign): ファ/ヴァ/ etc.
    e("fya", "ふゃ", "フャ"), e("fyu", "ふゅ", "フュ"), e("fyo", "ふょ", "フョ"),
    e("vya", "ゔゃ", "ヴャ"), e("vyu", "ゔゅ", "ヴュ"), e("vyo", "ゔょ", "ヴョ"),
    // ts/ch/sh/j single-yoon foreign borrowings
    e("tsa", "つぁ", "ツァ"), e("tsi", "つぃ", "ツィ"),
    e("tse", "つぇ", "ツェ"), e("tso", "つぉ", "ツォ"),
    e("che", "ちぇ", "チェ"),
    e("she", "しぇ", "シェ"),
    // kw/gw row (クァ/グァ etc. — foreign-borrowed labialized k/g)
    e("kwa", "くぁ", "クァ"), e("kwi", "くぃ", "クィ"),
    e("kwe", "くぇ", "クェ"), e("kwo", "くぉ", "クォ"),
    e("gwa", "ぐぁ", "グァ"), e("gwi", "ぐぃ", "グィ"),
    e("gwe", "ぐぇ", "グェ"), e("gwo", "ぐぉ", "グォ"),
    // w-row foreign extensions (ウァ/ウィ/ウェ/ウォ) — 'wh*' explicit form.
    // Also exposes plain `wi/we` 2-letter below as the more common entry.
    e("wha", "うぁ", "ウァ"), e("whi", "うぃ", "ウィ"),
    e("whe", "うぇ", "ウェ"), e("who", "うぉ", "ウォ"),
    // Explicit ティ/ディ via `th*/dh*` — sidesteps kunrei ti=ち / di=ぢ
    // conflict. `tho/dho` map to て+small-yo / で+small-yo per mozc.
    e("tha", "てぁ", "テァ"), e("thi", "てぃ", "ティ"),
    e("the", "てぇ", "テェ"), e("tho", "てょ", "テョ"),
    e("dha", "でぁ", "デァ"), e("dhi", "でぃ", "ディ"),
    e("dhe", "でぇ", "デェ"), e("dho", "でょ", "デョ"),
    // Explicit トゥ/ドゥ via `twu/dwu` — sidesteps kunrei tu=つ / du=づ.
    e("twu", "とぅ", "トゥ"),
    e("dwu", "どぅ", "ドゥ"),

    // ---- 2-letter patterns: basic 五十音 + 浊音 + 半浊音 + 'nn' ----------
    e("ka", "か", "カ"), e("ki", "き", "キ"), e("ku", "く", "ク"), e("ke", "け", "ケ"), e("ko", "こ", "コ"),
    e("ga", "が", "ガ"), e("gi", "ぎ", "ギ"), e("gu", "ぐ", "グ"), e("ge", "げ", "ゲ"), e("go", "ご", "ゴ"),
    e("sa", "さ", "サ"),                       e("su", "す", "ス"), e("se", "せ", "セ"), e("so", "そ", "ソ"),
    e("si", "し", "シ"),  // kunrei
    e("za", "ざ", "ザ"), e("zi", "じ", "ジ"), e("zu", "ず", "ズ"), e("ze", "ぜ", "ゼ"), e("zo", "ぞ", "ゾ"),
    e("ji", "じ", "ジ"),  // hepburn
    e("ta", "た", "タ"),                       e("tu", "つ", "ツ"), e("te", "て", "テ"), e("to", "と", "ト"),
    e("ti", "ち", "チ"),  // kunrei
    e("da", "だ", "ダ"), e("di", "ぢ", "ヂ"), e("du", "づ", "ヅ"), e("de", "で", "デ"), e("do", "ど", "ド"),
    e("na", "な", "ナ"), e("ni", "に", "ニ"), e("nu", "ぬ", "ヌ"), e("ne", "ね", "ネ"), e("no", "の", "ノ"),
    e("ha", "は", "ハ"), e("hi", "ひ", "ヒ"), e("hu", "ふ", "フ"), e("he", "へ", "ヘ"), e("ho", "ほ", "ホ"),
    e("fu", "ふ", "フ"),  // hepburn
    e("ba", "ば", "バ"), e("bi", "び", "ビ"), e("bu", "ぶ", "ブ"), e("be", "べ", "ベ"), e("bo", "ぼ", "ボ"),
    e("pa", "ぱ", "パ"), e("pi", "ぴ", "ピ"), e("pu", "ぷ", "プ"), e("pe", "ぺ", "ペ"), e("po", "ぽ", "ポ"),
    e("ma", "ま", "マ"), e("mi", "み", "ミ"), e("mu", "む", "ム"), e("me", "め", "メ"), e("mo", "も", "モ"),
    e("ya", "や", "ヤ"),                       e("yu", "ゆ", "ユ"),                       e("yo", "よ", "ヨ"),
    e("ra", "ら", "ラ"), e("ri", "り", "リ"), e("ru", "る", "ル"), e("re", "れ", "レ"), e("ro", "ろ", "ロ"),
    e("wa", "わ", "ワ"), e("wo", "を", "ヲ"),
    e("ja", "じゃ", "ジャ"), e("ju", "じゅ", "ジュ"), e("jo", "じょ", "ジョ"),  // hepburn 2-letter

    // ---- 2-letter foreign-loanword extensions ------------------------
    // fa-row (ファ): native JP has only fu = ふ/フ; foreign borrowings need
    // fa/fi/fe/fo for words like ファミリア (familia) / フィルム (film) /
    // フェルト (felt) / フォーマット (format).
    e("fa", "ふぁ", "ファ"), e("fi", "ふぃ", "フィ"),
    e("fe", "ふぇ", "フェ"), e("fo", "ふぉ", "フォ"),
    // va-row (ヴァ): foreign 'v' — ヴァイオリン (violin), ヴィデオ etc.
    // Hiragana ゔ (U+3094) is rare but standard for paired small-vowels.
    e("va", "ゔぁ", "ヴァ"), e("vi", "ゔぃ", "ヴィ"),
    e("vu", "ゔ",   "ヴ"),  e("ve", "ゔぇ", "ヴェ"),
    e("vo", "ゔぉ", "ヴォ"),
    // w-row foreign 2-letter aliases (ウィ/ウェ): wedding=ウェディング etc.
    e("wi", "うぃ", "ウィ"), e("we", "うぇ", "ウェ"),
    // je: foreign borrowings (ジェット = jet) — covers gap between ja/ju/jo.
    e("je", "じぇ", "ジェ"),
    // Small explicit kana: type ぁ/ぃ/ぅ/ぇ/ぉ directly via `xa-xo` (mozc/
    // Hepburn convention) or `la-lo` (alternative IME convention; some
    // users default to `l-`). Both aliases supported.
    e("xa", "ぁ", "ァ"), e("xi", "ぃ", "ィ"), e("xu", "ぅ", "ゥ"),
    e("xe", "ぇ", "ェ"), e("xo", "ぉ", "ォ"),
    e("la", "ぁ", "ァ"), e("li", "ぃ", "ィ"), e("lu", "ぅ", "ゥ"),
    e("le", "ぇ", "ェ"), e("lo", "ぉ", "ォ"),
    // 'nn' and single 'n' are NOT table entries — they're handled by the
    // moraic-n state machine in `render` so that `annai` → あんない and
    // `nna` → んな work the way mozc/Google JP IME do them. Adding them
    // back to the table would break that (greedy "nn" would consume both
    // n's, so `annai` would mis-parse as a+nn+ai = あんあい).

    // ---- 1-letter patterns ------------------------------------------
    e("a", "あ", "ア"), e("i", "い", "イ"), e("u", "う", "ウ"), e("e", "え", "エ"), e("o", "お", "オ"),
    e("-", "ー", "ー"),  // chōonpu (long-vowel mark)
];

/// Render `s` (a romaji buffer) as hiragana. Unmappable bytes pass
/// through as ASCII so the caller can show partial input to the user
/// while they keep typing. Double-consonant gemination (`kk*` → `っk*`)
/// is recognized; double-vowel does not produce 促音.
pub fn to_hiragana(s: &str) -> String {
    render(s, /* katakana = */ false)
}

/// Render `s` as katakana. Same parser as [`to_hiragana`].
pub fn to_katakana(s: &str) -> String {
    render(s, /* katakana = */ true)
}

fn render(s: &str, katakana: bool) -> String {
    let lower = s.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let sokuon = if katakana { "ッ" } else { "っ" };
    let n_str = if katakana { "ン" } else { "ん" };
    let mut out = String::with_capacity(s.len() * 3);
    let mut i = 0;
    while i < bytes.len() {
        // Gemination: doubled consonant (not `n`, not vowel) emits 促音
        // and consumes only one byte, leaving the doubled consonant to
        // be matched as part of the following syllable on the next iter.
        // `n` doubling is excluded here — it's handled by the moraic-n
        // state machine below (`nn` is the explicit-ん shortcut, not 促音).
        if i + 1 < bytes.len()
            && bytes[i] == bytes[i + 1]
            && bytes[i] != b'n'
            && is_gemini_consonant(bytes[i])
        {
            out.push_str(sokuon);
            i += 1;
            continue;
        }
        // Moraic-n state machine. Drives correct parsing of `annai` →
        // あんない and `nna` → んな, matching mozc / Google JP IME
        // behavior:
        //   `n` at buffer end                       → ん (consume 1)
        //   `nn` at buffer end (no further bytes)   → ん (consume 2,
        //       single-ん shortcut so users typing `nn` to commit ん
        //       don't get んん)
        //   `n` followed by another consonant       → ん (consume 1),
        //       letting the next consonant start a fresh syllable.
        //       Critical for `annai`: a + n[→ん] + n + a + i parses as
        //       a + ん + na + i.
        //   `n` followed by vowel or `y`            → fall through to
        //       the greedy table (which matches `na`, `nya`, etc.).
        if bytes[i] == b'n' {
            if i + 1 >= bytes.len() {
                out.push_str(n_str);
                i += 1;
                continue;
            }
            let next = bytes[i + 1];
            if next == b'n' && i + 2 >= bytes.len() {
                out.push_str(n_str);
                i += 2;
                continue;
            }
            if !matches!(next, b'a' | b'i' | b'u' | b'e' | b'o' | b'y') {
                out.push_str(n_str);
                i += 1;
                continue;
            }
            // else: fall through to table match for `n+vowel` / `n+y+vowel`.
        }
        // Longest-prefix table match.
        let mut matched_len = 0;
        for entry in TABLE.iter() {
            let pat = entry.rom.as_bytes();
            if pat.len() <= bytes.len() - i && &bytes[i..i + pat.len()] == pat {
                out.push_str(if katakana { entry.kana_k } else { entry.kana_h });
                matched_len = pat.len();
                break;
            }
        }
        if matched_len == 0 {
            // Unmappable byte (e.g. user typed a digit / punct, or an
            // incomplete romaji prefix like 'k' / 'sh'). Pass through
            // as ASCII so the candidate stays visible during typing.
            out.push(bytes[i] as char);
            i += 1;
        } else {
            i += matched_len;
        }
    }
    out
}

const fn is_gemini_consonant(b: u8) -> bool {
    matches!(
        b,
        b'k' | b'g'
            | b's'
            | b'z'
            | b'j'
            | b't'
            | b'd'
            | b'c'
            | b'h'
            | b'f'
            | b'b'
            | b'p'
            | b'm'
            | b'r'
            | b'y'
            | b'w'
            | b'v'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_vowels() {
        assert_eq!(to_hiragana("a"), "あ");
        assert_eq!(to_hiragana("aiueo"), "あいうえお");
        assert_eq!(to_katakana("aiueo"), "アイウエオ");
    }

    #[test]
    fn basic_ka_row() {
        assert_eq!(to_hiragana("ka"), "か");
        assert_eq!(to_hiragana("kakikukeko"), "かきくけこ");
        assert_eq!(to_katakana("ka"), "カ");
    }

    #[test]
    fn voiced_and_semi_voiced() {
        assert_eq!(to_hiragana("ga"), "が");
        assert_eq!(to_hiragana("za"), "ざ");
        assert_eq!(to_hiragana("ba"), "ば");
        assert_eq!(to_hiragana("pa"), "ぱ");
    }

    #[test]
    fn hepburn_and_kunrei_aliases() {
        // shi/si, chi/ti, tsu/tu, fu/hu, ji/zi all map to canonical kana.
        assert_eq!(to_hiragana("shi"), "し");
        assert_eq!(to_hiragana("si"), "し");
        assert_eq!(to_hiragana("chi"), "ち");
        assert_eq!(to_hiragana("ti"), "ち");
        assert_eq!(to_hiragana("tsu"), "つ");
        assert_eq!(to_hiragana("tu"), "つ");
        assert_eq!(to_hiragana("fu"), "ふ");
        assert_eq!(to_hiragana("hu"), "ふ");
        assert_eq!(to_hiragana("ji"), "じ");
        assert_eq!(to_hiragana("zi"), "じ");
    }

    #[test]
    fn youon_digraphs() {
        assert_eq!(to_hiragana("kya"), "きゃ");
        assert_eq!(to_hiragana("kyu"), "きゅ");
        assert_eq!(to_hiragana("kyo"), "きょ");
        assert_eq!(to_hiragana("sha"), "しゃ");
        assert_eq!(to_hiragana("cha"), "ちゃ");
        assert_eq!(to_hiragana("ja"), "じゃ");
        assert_eq!(to_hiragana("nyo"), "にょ");
        assert_eq!(to_katakana("rya"), "リャ");
    }

    #[test]
    fn sokuon_gemination() {
        // `kka` → っ + か
        assert_eq!(to_hiragana("kka"), "っか");
        // `ssa` → っ + さ
        assert_eq!(to_hiragana("ssa"), "っさ");
        // explicit small-tsu spellings
        assert_eq!(to_hiragana("xtsu"), "っ");
        assert_eq!(to_hiragana("xtu"), "っ");
        // `nn` is *not* a sokuon — it's ん (the moraic n)
        assert_eq!(to_hiragana("nn"), "ん");
    }

    #[test]
    fn moraic_n_basic() {
        // Single `n` at buffer end commits as ん.
        assert_eq!(to_hiragana("n"), "ん");
        // `n` followed by vowel forms the n-row syllable.
        assert_eq!(to_hiragana("na"), "な");
        // `nn` at buffer end is the explicit-ん shortcut — single ん.
        assert_eq!(to_hiragana("nn"), "ん");
    }

    #[test]
    fn moraic_n_annai_pattern() {
        // The mozc-style golden case: `n` followed by another consonant
        // commits as ん even though it's mid-buffer. Otherwise `annai`
        // parses to あんあい (wrong) instead of あんない.
        assert_eq!(to_hiragana("nna"), "んな");
        assert_eq!(to_hiragana("annai"), "あんない");
        assert_eq!(to_hiragana("ganbaru"), "がんばる");
        // 'wa' deliberately maps to わ — users typing the historical
        // は-particle pronunciation will get こんにちわ from `konnichiwa`,
        // which is the standard romaji-mapping outcome (mozc behavior).
        assert_eq!(to_hiragana("konnichiwa"), "こんにちわ");
    }

    #[test]
    fn chouonpu_long_mark() {
        assert_eq!(to_hiragana("ka-"), "かー");
        assert_eq!(to_katakana("ko-hi-"), "コーヒー");
    }

    #[test]
    fn incomplete_romaji_passes_through() {
        // Lone consonant left in the buffer renders as ASCII so the
        // candidate is "still typing" not "fully consumed garbage".
        assert_eq!(to_hiragana("k"), "k");
        assert_eq!(to_hiragana("sh"), "sh");
    }

    #[test]
    fn multi_syllable_word() {
        assert_eq!(to_hiragana("nihon"), "にほん");
        assert_eq!(to_hiragana("tokyo"), "ときょ");
        assert_eq!(to_hiragana("nippon"), "にっぽん");
        assert_eq!(to_katakana("kohi-"), "コヒー");
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(to_hiragana("KA"), "か");
        assert_eq!(to_hiragana("Nihon"), "にほん");
    }

    #[test]
    fn foreign_fa_row() {
        // 2-letter ファ row — the famiriaare regression. Pre-fix `fa` had
        // no entry, leading `f` fell through as ASCII, is_jp_clean rejected
        // every hiragana/katakana variant containing it.
        assert_eq!(to_katakana("fa"), "ファ");
        assert_eq!(to_katakana("famiriaare"), "ファミリアアレ");
        assert_eq!(to_katakana("fi"), "フィ");
        assert_eq!(to_katakana("fe"), "フェ");
        assert_eq!(to_katakana("fo"), "フォ");
        // hiragana variants render through the same path.
        assert_eq!(to_hiragana("fa"), "ふぁ");
        assert_eq!(to_hiragana("famiriaare"), "ふぁみりああれ");
    }

    #[test]
    fn foreign_va_row() {
        assert_eq!(to_katakana("va"), "ヴァ");
        assert_eq!(to_katakana("vi"), "ヴィ");
        assert_eq!(to_katakana("vu"), "ヴ");
        assert_eq!(to_katakana("ve"), "ヴェ");
        assert_eq!(to_katakana("vo"), "ヴォ");
        // Real foreign-loanword: violin = vaiorin
        assert_eq!(to_katakana("vaiorin"), "ヴァイオリン");
    }

    #[test]
    fn foreign_w_row() {
        // 2-letter `wi/we` (also accepts 3-letter `whi/whe` aliases)
        assert_eq!(to_katakana("wi"), "ウィ");
        assert_eq!(to_katakana("we"), "ウェ");
        assert_eq!(to_katakana("whi"), "ウィ");
        assert_eq!(to_katakana("whe"), "ウェ");
        assert_eq!(to_katakana("wha"), "ウァ");
        assert_eq!(to_katakana("who"), "ウォ");
        // wedding = wedhingu (kunrei `di`=ぢ, so ディ requires explicit dhi)
        assert_eq!(to_katakana("wedhingu"), "ウェディング");
        // weak point: wedingu still renders 'wedi' as ウェ+ぢ → ウェヂ
        // (kunrei mapping) which is expected and matches mozc.
        assert_eq!(to_katakana("wedingu"), "ウェヂング");
    }

    #[test]
    fn foreign_je_che_she() {
        assert_eq!(to_katakana("je"), "ジェ");
        assert_eq!(to_katakana("che"), "チェ");
        assert_eq!(to_katakana("she"), "シェ");
        // jet = jetto
        assert_eq!(to_katakana("jetto"), "ジェット");
        // shake = sheiku
        assert_eq!(to_katakana("sheiku"), "シェイク");
    }

    #[test]
    fn foreign_ts_row() {
        assert_eq!(to_katakana("tsa"), "ツァ");
        assert_eq!(to_katakana("tsi"), "ツィ");
        assert_eq!(to_katakana("tse"), "ツェ");
        assert_eq!(to_katakana("tso"), "ツォ");
    }

    #[test]
    fn foreign_thi_dhi_twu_dwu() {
        // Explicit ティ/ディ/トゥ/ドゥ — sidesteps kunrei conflict.
        assert_eq!(to_katakana("thi"), "ティ");
        assert_eq!(to_katakana("dhi"), "ディ");
        assert_eq!(to_katakana("twu"), "トゥ");
        assert_eq!(to_katakana("dwu"), "ドゥ");
        // party = pa-thi-
        assert_eq!(to_katakana("pa-thi-"), "パーティー");
    }

    #[test]
    fn foreign_kwa_gwa_rows() {
        assert_eq!(to_katakana("kwa"), "クァ");
        assert_eq!(to_katakana("kwi"), "クィ");
        assert_eq!(to_katakana("kwe"), "クェ");
        assert_eq!(to_katakana("kwo"), "クォ");
        assert_eq!(to_katakana("gwa"), "グァ");
    }

    #[test]
    fn foreign_fy_vy_yoon() {
        assert_eq!(to_katakana("fya"), "フャ");
        assert_eq!(to_katakana("fyu"), "フュ");
        assert_eq!(to_katakana("vya"), "ヴャ");
        assert_eq!(to_katakana("vyu"), "ヴュ");
    }

    #[test]
    fn small_kana_explicit() {
        // xa-xo and la-lo aliases — type small ぁぃぅぇぉ directly.
        assert_eq!(to_hiragana("xa"), "ぁ");
        assert_eq!(to_hiragana("xi"), "ぃ");
        assert_eq!(to_hiragana("xu"), "ぅ");
        assert_eq!(to_hiragana("xe"), "ぇ");
        assert_eq!(to_hiragana("xo"), "ぉ");
        assert_eq!(to_hiragana("la"), "ぁ");
        assert_eq!(to_katakana("xa"), "ァ");
        assert_eq!(to_katakana("la"), "ァ");
    }

    #[test]
    fn pre_existing_kunrei_di_du_unchanged() {
        // Sanity guard against the new 'dh*/dw*/d*' entries clobbering
        // the kunrei `di`=ぢ / `du`=づ entries. mozc convention: bare
        // `di` is still kunrei → ぢ; explicit ディ uses `dhi`.
        assert_eq!(to_hiragana("di"), "ぢ");
        assert_eq!(to_hiragana("du"), "づ");
        assert_eq!(to_hiragana("ti"), "ち");
        assert_eq!(to_hiragana("tu"), "つ");
    }
}
