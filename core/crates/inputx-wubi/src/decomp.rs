//! Owned character-decomposition data (`char → Decomp`) and a parser for
//! `data/seed.txt`.
//!
//! The runtime encoder works against the borrowed [`crate::codec::DecompRef`].
//! `Decomp` is the convenient owned counterpart used by tools and tests; it
//! exposes `as_ref()` to feed the codec.

use crate::codec::{DecompRef, Shape, Stroke};

const SEED_TXT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/seed.txt"));

/// Owned counterpart of [`DecompRef`]. Use [`Decomp::as_ref`] to feed the
/// runtime encoder; the encoder itself only ever sees the borrowed form.
#[derive(Debug, Clone)]
pub struct Decomp {
    /// Ordered 字根 sequence (1..=N).
    pub zigen: Vec<char>,
    /// Stroke sequence, used for 成字字根 and 末笔识别码 rules.
    pub strokes: Vec<Stroke>,
    /// 字形 (left-right / top-bottom / whole).
    pub shape: Shape,
}

impl Decomp {
    /// Borrow this `Decomp` as a [`DecompRef`] for the encoder.
    #[inline]
    pub fn as_ref(&self) -> DecompRef<'_> {
        DecompRef {
            zigen: &self.zigen,
            strokes: &self.strokes,
            shape: self.shape,
        }
    }

    /// First stroke or `None` if the stroke list is empty.
    pub fn first_stroke(&self) -> Option<Stroke> {
        self.strokes.first().copied()
    }
    /// Last stroke or `None` if the stroke list is empty.
    pub fn last_stroke(&self) -> Option<Stroke> {
        self.strokes.last().copied()
    }
}

/// Parse a seed-format TSV string into `(char, Decomp)` pairs.
///
/// Format: each non-blank, non-`#` line is `<char>\t<zigen>\t<strokes>\t<shape>`,
/// where `zigen` is whitespace-separated 字根, `strokes` is whitespace-
/// separated digits 1..=5, and `shape` is one of 1..=3. Malformed lines are
/// silently skipped (consistent with the build-time parser in `build.rs`).
pub fn parse_seed(src: &str) -> Vec<(char, Decomp)> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(4, '\t');
        let (Some(ch), Some(zg), Some(strokes_field), Some(shape)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let ch = ch.trim();
        if ch.chars().count() != 1 {
            continue;
        }
        let ch = ch.chars().next().unwrap();
        let zigen: Vec<char> = zg.split_whitespace().flat_map(|s| s.chars()).collect();
        if zigen.is_empty() {
            continue;
        }
        let strokes: Vec<Stroke> = strokes_field
            .split_whitespace()
            .filter_map(|s| s.parse::<u8>().ok().and_then(Stroke::from_u8))
            .collect();
        if strokes.is_empty() {
            continue;
        }
        let Ok(p) = shape.trim().parse::<u8>() else {
            continue;
        };
        let Some(shape) = Shape::from_u8(p) else {
            continue;
        };
        out.push((
            ch,
            Decomp {
                zigen,
                strokes,
                shape,
            },
        ));
    }
    out
}

/// Parse the seed bundled into the binary at compile time.
/// Equivalent to `parse_seed(include_str!("data/seed.txt"))`.
pub fn embedded_seed() -> Vec<(char, Decomp)> {
    parse_seed(SEED_TXT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_seed_entries() {
        let entries = embedded_seed();
        assert!(entries.len() >= 25);
        let (ch, d) = &entries[0];
        assert_eq!(*ch, '王');
        assert_eq!(d.zigen, vec!['王']);
        assert_eq!(d.first_stroke(), Some(Stroke::Heng));
    }
}
