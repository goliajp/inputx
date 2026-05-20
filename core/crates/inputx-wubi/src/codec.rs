//! Self-contained Wubi 86 codec — pure algorithm + types, zero external imports.
//!
//! This module is intentionally usable from `build.rs` (via `#[path]`) as
//! well as the runtime crate. **Do not** add `use crate::...` lines here, or
//! `extern crate alloc;`; the only allowed imports are `core::*`.
//!
//! Everything is `#[inline]` and zero-allocation. The encoder writes into a
//! caller-provided `[u8; 4]` buffer and returns the populated length.

use core::fmt;

/// One of the five Wubi stroke categories. Discriminants 1..=5 match the
/// canonical numbering used in seed data files; do not renumber.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Stroke {
    /// 横 — horizontal.
    Heng = 1,
    /// 竖 — vertical.
    Shu = 2,
    /// 撇 — left-falling.
    Pie = 3,
    /// 捺 — right-falling (incl. 点 / dot).
    Na = 4,
    /// 折 — turning.
    Zhe = 5,
}

impl Stroke {
    /// Decode a stroke discriminant from its `u8`. `None` for any value
    /// outside `1..=5` — seed-file parsers should treat that as a row-skip,
    /// not a hard error.
    #[inline]
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Heng),
            2 => Some(Self::Shu),
            3 => Some(Self::Pie),
            4 => Some(Self::Na),
            5 => Some(Self::Zhe),
            _ => None,
        }
    }
}

/// Wubi 字形 (character shape) — three-way classification used by the 末笔
/// 识别码 rule. Discriminants 1..=3 match the seed-file numbering.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// 左右 — left-right structure.
    LeftRight = 1,
    /// 上下 — top-bottom structure.
    TopBottom = 2,
    /// 杂合 — single-component / unsplit.
    Whole = 3,
}

impl Shape {
    /// Decode a shape discriminant from its `u8`. `None` for any value
    /// outside `1..=3`.
    #[inline]
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::LeftRight),
            2 => Some(Self::TopBottom),
            3 => Some(Self::Whole),
            _ => None,
        }
    }
}

/// Wubi 86 末笔识别码: (last stroke, shape) → key letter.
#[inline]
pub const fn shibie_ma(stroke: Stroke, shape: Shape) -> u8 {
    let table: [[u8; 3]; 5] = [
        [b'g', b'f', b'd'],
        [b'h', b'j', b'k'],
        [b't', b'r', b'e'],
        [b'y', b'u', b'i'],
        [b'n', b'b', b'v'],
    ];
    let s = stroke as usize - 1;
    let p = shape as usize - 1;
    table[s][p]
}

/// Region letter — first key of each stroke region.
/// Used by 成字字根 rule.
#[inline]
pub const fn region_letter(stroke: Stroke) -> u8 {
    match stroke {
        Stroke::Heng => b'g',
        Stroke::Shu => b'h',
        Stroke::Pie => b't',
        Stroke::Na => b'y',
        Stroke::Zhe => b'n',
    }
}

/// 25 键名字根 of Wubi 86 — encode as letter × 4.
pub const JIANMING_ZIGEN: &str = "王土大木工目日口田山禾白月人金言立水火之已子女又纟";

/// 5 单笔画 字根 — encode as `letter letter L L`.
pub const DAN_BI_HUA: &[char] = &['一', '丨', '丿', '丶', '乙'];

/// Borrowed character decomposition. Cheap to construct from `&[char]` /
/// `&[Stroke]`, suitable for both stack-only build pipelines and
/// the runtime hot path.
#[derive(Debug, Clone, Copy)]
pub struct DecompRef<'a> {
    /// Ordered 字根 sequence (1..=N items). Encoder consumes positions
    /// `[0, 1, 2, last]`.
    pub zigen: &'a [char],
    /// Stroke sequence for 成字字根 / 末笔识别码 rules. May be longer than
    /// `zigen`; only the first/second/last positions are read.
    pub strokes: &'a [Stroke],
    /// Whole-character shape — drives the 末笔识别码 lookup.
    pub shape: Shape,
}

impl<'a> DecompRef<'a> {
    /// First stroke, or `None` for an empty stroke list.
    #[inline]
    pub fn first_stroke(&self) -> Option<Stroke> {
        self.strokes.first().copied()
    }
    /// Second stroke (index 1), or `None` if fewer than 2 strokes.
    #[inline]
    pub fn second_stroke(&self) -> Option<Stroke> {
        self.strokes.get(1).copied()
    }
    /// Last stroke, or `None` for an empty stroke list. Used by the 识别码
    /// rule for 2- and 3-字根 codes.
    #[inline]
    pub fn last_stroke(&self) -> Option<Stroke> {
        self.strokes.last().copied()
    }
}

/// Why an `encode_with_lookup` call failed. Always recoverable — never
/// panics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// `decomp.zigen` was empty.
    EmptyZigen,
    /// One of the 字根 wasn't in the supplied lookup. Carries the offending
    /// character so the caller can log + continue past the bad row.
    UnknownZigen(char),
    /// A 2- or 3-字根 decomposition needs strokes for the 识别码 / 成字字根
    /// rule but `decomp.strokes` was empty.
    MissingStroke,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::EmptyZigen => f.write_str("empty zigen sequence"),
            EncodeError::UnknownZigen(c) => write!(f, "unknown zigen: {c}"),
            EncodeError::MissingStroke => f.write_str("decomp has no strokes"),
        }
    }
}

#[inline]
fn is_jianming(c: char) -> bool {
    // O(n) on 25-char string; ~50ns. Phase 2 swaps to PHF set for ~5ns.
    JIANMING_ZIGEN.contains(c)
}

#[inline]
fn is_dan_bi_hua(c: char) -> bool {
    let mut i = 0;
    while i < DAN_BI_HUA.len() {
        if DAN_BI_HUA[i] == c {
            return true;
        }
        i += 1;
    }
    false
}

/// Encode a decomposition using the supplied 字根 lookup. Writes the code
/// bytes into `out` and returns the number of bytes written (3 or 4).
///
/// The encoder is the ONLY entry point that runs the rules; all platform
/// glue (PHF runtime tables, HashMap build-time tables) goes through here
/// via the lookup closure.
pub fn encode_with_lookup<F>(
    decomp: &DecompRef,
    lookup: F,
    out: &mut [u8; 4],
) -> Result<usize, EncodeError>
where
    F: Fn(char) -> Option<u8>,
{
    if decomp.zigen.is_empty() {
        return Err(EncodeError::EmptyZigen);
    }

    let n = decomp.zigen.len();

    match n {
        1 => encode_single_zigen(decomp, lookup, out),
        2 => {
            let l1 = lookup(decomp.zigen[0]).ok_or(EncodeError::UnknownZigen(decomp.zigen[0]))?;
            let l2 = lookup(decomp.zigen[1]).ok_or(EncodeError::UnknownZigen(decomp.zigen[1]))?;
            let last = decomp.last_stroke().ok_or(EncodeError::MissingStroke)?;
            let im = shibie_ma(last, decomp.shape);
            out[0] = l1;
            out[1] = l2;
            out[2] = im;
            Ok(3)
        }
        3 => {
            let l1 = lookup(decomp.zigen[0]).ok_or(EncodeError::UnknownZigen(decomp.zigen[0]))?;
            let l2 = lookup(decomp.zigen[1]).ok_or(EncodeError::UnknownZigen(decomp.zigen[1]))?;
            let l3 = lookup(decomp.zigen[2]).ok_or(EncodeError::UnknownZigen(decomp.zigen[2]))?;
            let last = decomp.last_stroke().ok_or(EncodeError::MissingStroke)?;
            let im = shibie_ma(last, decomp.shape);
            out[0] = l1;
            out[1] = l2;
            out[2] = l3;
            out[3] = im;
            Ok(4)
        }
        _ => {
            // 4+ 字根: 1st + 2nd + 3rd + last
            let l1 = lookup(decomp.zigen[0]).ok_or(EncodeError::UnknownZigen(decomp.zigen[0]))?;
            let l2 = lookup(decomp.zigen[1]).ok_or(EncodeError::UnknownZigen(decomp.zigen[1]))?;
            let l3 = lookup(decomp.zigen[2]).ok_or(EncodeError::UnknownZigen(decomp.zigen[2]))?;
            let ll = lookup(decomp.zigen[n - 1])
                .ok_or(EncodeError::UnknownZigen(decomp.zigen[n - 1]))?;
            out[0] = l1;
            out[1] = l2;
            out[2] = l3;
            out[3] = ll;
            Ok(4)
        }
    }
}

#[inline]
fn encode_single_zigen<F>(
    decomp: &DecompRef,
    lookup: F,
    out: &mut [u8; 4],
) -> Result<usize, EncodeError>
where
    F: Fn(char) -> Option<u8>,
{
    let z = decomp.zigen[0];
    let l = lookup(z).ok_or(EncodeError::UnknownZigen(z))?;

    if is_dan_bi_hua(z) {
        out[0] = l;
        out[1] = l;
        out[2] = b'l';
        out[3] = b'l';
        return Ok(4);
    }
    if is_jianming(z) {
        out[0] = l;
        out[1] = l;
        out[2] = l;
        out[3] = l;
        return Ok(4);
    }
    // 成字字根 rules depend on stroke count:
    //   - 1 stroke → handled by 单笔画 above (this branch unreachable here)
    //   - 2 strokes → 3-letter code: letter + first + last
    //   - ≥ 3 strokes → 4-letter code: letter + first + second + last
    let first = decomp.first_stroke().ok_or(EncodeError::MissingStroke)?;
    let last = decomp.last_stroke().ok_or(EncodeError::MissingStroke)?;
    let stroke_count = decomp.strokes.len();
    if stroke_count == 2 {
        out[0] = l;
        out[1] = region_letter(first);
        out[2] = region_letter(last);
        return Ok(3);
    }
    let second = decomp.second_stroke().ok_or(EncodeError::MissingStroke)?;
    out[0] = l;
    out[1] = region_letter(first);
    out[2] = region_letter(second);
    out[3] = region_letter(last);
    Ok(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy(c: char) -> Option<u8> {
        match c {
            '王' => Some(b'g'),
            '土' => Some(b'f'),
            '大' => Some(b'd'),
            '人' => Some(b'w'),
            '一' => Some(b'g'),
            '丨' => Some(b'h'),
            '丿' => Some(b't'),
            '丶' => Some(b'y'),
            '乙' => Some(b'n'),
            _ => None,
        }
    }

    #[test]
    fn shibie_grid() {
        assert_eq!(shibie_ma(Stroke::Heng, Shape::LeftRight), b'g');
        assert_eq!(shibie_ma(Stroke::Heng, Shape::Whole), b'd');
        assert_eq!(shibie_ma(Stroke::Zhe, Shape::Whole), b'v');
    }

    #[test]
    fn region_letters() {
        assert_eq!(region_letter(Stroke::Heng), b'g');
        assert_eq!(region_letter(Stroke::Zhe), b'n');
    }

    #[test]
    fn jianming_letter_x4() {
        let d = DecompRef {
            zigen: &['王'],
            strokes: &[Stroke::Heng],
            shape: Shape::Whole,
        };
        let mut out = [0u8; 4];
        let n = encode_with_lookup(&d, dummy, &mut out).unwrap();
        assert_eq!(&out[..n], b"gggg");
    }

    #[test]
    fn dan_bi_hua_rule() {
        for (c, stroke, expected) in &[
            ('一', Stroke::Heng, b"ggll"),
            ('丨', Stroke::Shu, b"hhll"),
            ('丿', Stroke::Pie, b"ttll"),
            ('丶', Stroke::Na, b"yyll"),
            ('乙', Stroke::Zhe, b"nnll"),
        ] {
            let d = DecompRef {
                zigen: &[*c],
                strokes: &[*stroke],
                shape: Shape::Whole,
            };
            let mut out = [0u8; 4];
            let n = encode_with_lookup(&d, dummy, &mut out).unwrap();
            assert_eq!(&out[..n], *expected, "{c} mismatch");
        }
    }

    #[test]
    fn unknown_zigen_errors_out() {
        let d = DecompRef {
            zigen: &['🦀'],
            strokes: &[Stroke::Heng],
            shape: Shape::Whole,
        };
        let mut out = [0u8; 4];
        assert!(matches!(
            encode_with_lookup(&d, dummy, &mut out),
            Err(EncodeError::UnknownZigen('🦀'))
        ));
    }

    #[test]
    fn empty_zigen_errors_out() {
        let d = DecompRef {
            zigen: &[],
            strokes: &[Stroke::Heng],
            shape: Shape::Whole,
        };
        let mut out = [0u8; 4];
        assert!(matches!(
            encode_with_lookup(&d, dummy, &mut out),
            Err(EncodeError::EmptyZigen)
        ));
    }

    // ---- Coverage fill: helper accessors + error formatting + 2/3-字根
    // branches were exercised only by the wider integration tests, which
    // hides regressions in these specific surfaces behind unrelated test
    // failures. Direct unit tests keep them honest.

    #[test]
    fn stroke_shape_from_u8_round_trip_and_reject() {
        for (n, s) in [
            (1u8, Stroke::Heng),
            (2, Stroke::Shu),
            (3, Stroke::Pie),
            (4, Stroke::Na),
            (5, Stroke::Zhe),
        ] {
            assert_eq!(Stroke::from_u8(n), Some(s));
        }
        assert_eq!(Stroke::from_u8(0), None);
        assert_eq!(Stroke::from_u8(6), None);

        for (n, sh) in [
            (1u8, Shape::LeftRight),
            (2, Shape::TopBottom),
            (3, Shape::Whole),
        ] {
            assert_eq!(Shape::from_u8(n), Some(sh));
        }
        assert_eq!(Shape::from_u8(0), None);
        assert_eq!(Shape::from_u8(4), None);
    }

    #[test]
    fn decomp_ref_stroke_accessors() {
        let with_three = DecompRef {
            zigen: &[],
            strokes: &[Stroke::Heng, Stroke::Shu, Stroke::Pie],
            shape: Shape::Whole,
        };
        assert_eq!(with_three.first_stroke(), Some(Stroke::Heng));
        assert_eq!(with_three.second_stroke(), Some(Stroke::Shu));
        assert_eq!(with_three.last_stroke(), Some(Stroke::Pie));

        let single = DecompRef {
            zigen: &[],
            strokes: &[Stroke::Heng],
            shape: Shape::Whole,
        };
        assert_eq!(single.first_stroke(), Some(Stroke::Heng));
        assert_eq!(single.second_stroke(), None);
        assert_eq!(single.last_stroke(), Some(Stroke::Heng));

        let empty = DecompRef {
            zigen: &[],
            strokes: &[],
            shape: Shape::Whole,
        };
        assert_eq!(empty.first_stroke(), None);
        assert_eq!(empty.second_stroke(), None);
        assert_eq!(empty.last_stroke(), None);
    }

    #[test]
    fn encode_error_display_messages() {
        assert_eq!(
            format!("{}", EncodeError::EmptyZigen),
            "empty zigen sequence"
        );
        assert_eq!(
            format!("{}", EncodeError::UnknownZigen('🦀')),
            "unknown zigen: 🦀"
        );
        assert_eq!(
            format!("{}", EncodeError::MissingStroke),
            "decomp has no strokes"
        );
    }

    #[test]
    fn two_zigen_emits_three_codes_with_shibie() {
        // 2-字根: l1 + l2 + 识别码(last stroke, shape). dummy() only knows
        // single-stroke 字根, so use the 一+一 case → both map to 'g', last
        // stroke is Heng, shape Whole → 识别码 = 'd'. Expected: "ggd".
        let d = DecompRef {
            zigen: &['一', '一'],
            strokes: &[Stroke::Heng, Stroke::Heng],
            shape: Shape::Whole,
        };
        let mut out = [0u8; 4];
        let n = encode_with_lookup(&d, dummy, &mut out).unwrap();
        assert_eq!(&out[..n], b"ggd");
    }

    #[test]
    fn three_zigen_emits_four_codes_with_shibie() {
        // 3-字根: l1 + l2 + l3 + 识别码. Three 一 字根 + Heng + Whole
        // shape → "ggg" + 识别码('d') = "gggd".
        let d = DecompRef {
            zigen: &['一', '一', '一'],
            strokes: &[Stroke::Heng, Stroke::Heng, Stroke::Heng],
            shape: Shape::Whole,
        };
        let mut out = [0u8; 4];
        let n = encode_with_lookup(&d, dummy, &mut out).unwrap();
        assert_eq!(&out[..n], b"gggd");
    }

    #[test]
    fn two_zigen_missing_stroke_errors_out() {
        let d = DecompRef {
            zigen: &['一', '一'],
            strokes: &[],
            shape: Shape::Whole,
        };
        let mut out = [0u8; 4];
        assert!(matches!(
            encode_with_lookup(&d, dummy, &mut out),
            Err(EncodeError::MissingStroke)
        ));
    }
}
