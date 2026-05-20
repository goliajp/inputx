//! High-level encoder API over [`crate::codec`]: zero-alloc `encode_into`
//! plus an ergonomic `encode` that returns a stack [`EncodedCode`].
//!
//! The runtime lookup goes through [`crate::zigen::lookup`]. Build-time
//! callers and tests should use [`crate::codec::encode_with_lookup`]
//! directly with their own lookup closure.

use core::fmt;

use crate::codec::{EncodeError, encode_with_lookup};
use crate::decomp::Decomp;
use crate::zigen;

/// Stack-allocated Wubi code (3 or 4 ASCII letters).
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct EncodedCode {
    bytes: [u8; 4],
    len: u8,
}

impl EncodedCode {
    /// Borrow the populated prefix (3 or 4 bytes). Trailing bytes of the
    /// internal `[u8; 4]` are uninitialized and MUST NOT be read.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
    /// Borrow as `&str`. Always valid UTF-8 (encoder only emits ASCII).
    #[inline]
    pub fn as_str(&self) -> &str {
        // SAFETY: encoder writes only ASCII (a–z, plus 'l' for 单笔画).
        unsafe { core::str::from_utf8_unchecked(self.as_bytes()) }
    }
    /// Number of populated code bytes — always 3 or 4 for a successfully-
    /// encoded character.
    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }
    /// `true` iff `len() == 0`. Only happens for default-constructed values
    /// the encoder hasn't filled — successful encodes never return empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl fmt::Debug for EncodedCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EncodedCode({:?})", self.as_str())
    }
}

impl fmt::Display for EncodedCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Zero-allocation encode. Writes the code into `out` and returns the
/// number of bytes written (3 or 4).
#[inline]
pub fn encode_into(decomp: &Decomp, out: &mut [u8; 4]) -> Result<usize, EncodeError> {
    encode_with_lookup(&decomp.as_ref(), zigen::lookup, out)
}

/// Convenience: encode into a stack-allocated [`EncodedCode`].
#[inline]
pub fn encode(decomp: &Decomp) -> Result<EncodedCode, EncodeError> {
    let mut out = [0u8; 4];
    let n = encode_into(decomp, &mut out)?;
    Ok(EncodedCode {
        bytes: out,
        len: n as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{Shape, Stroke};
    use crate::decomp::embedded_seed;

    #[test]
    fn encodes_jianming_as_letter_x4() {
        let mut count = 0;
        for (ch, decomp) in embedded_seed() {
            if !crate::codec::JIANMING_ZIGEN.contains(ch) {
                continue;
            }
            let code = encode(&decomp).expect("encode failed");
            assert_eq!(code.len(), 4, "{ch}");
            let bytes = code.as_bytes();
            assert!(
                bytes.iter().all(|b| *b == bytes[0]),
                "{ch} → {} should be letter ×4",
                code
            );
            count += 1;
        }
        assert_eq!(count, 25);
    }

    #[test]
    fn encodes_dan_bi_hua() {
        let cases: &[(char, &str, Stroke)] = &[
            ('一', "ggll", Stroke::Heng),
            ('丨', "hhll", Stroke::Shu),
            ('丿', "ttll", Stroke::Pie),
            ('丶', "yyll", Stroke::Na),
            ('乙', "nnll", Stroke::Zhe),
        ];
        for (ch, expected, stroke) in cases {
            let d = Decomp {
                zigen: vec![*ch],
                strokes: vec![*stroke],
                shape: Shape::Whole,
            };
            let code = encode(&d).unwrap();
            assert_eq!(code.as_str(), *expected, "{ch} mismatch");
        }
    }

    #[test]
    fn encodes_specific_jianming() {
        let seed: std::collections::HashMap<char, _> = embedded_seed().into_iter().collect();
        for (ch, expected) in &[
            ('王', "gggg"),
            ('土', "ffff"),
            ('大', "dddd"),
            ('禾', "tttt"),
            ('纟', "xxxx"),
        ] {
            let d = seed.get(ch).unwrap();
            let code = encode(d).unwrap();
            assert_eq!(code.as_str(), *expected);
        }
    }

    #[test]
    fn encoder_does_not_allocate_on_zero_alloc_path() {
        // Sanity: encode_into uses a stack buffer only.
        let d = Decomp {
            zigen: vec!['王'],
            strokes: vec![Stroke::Heng],
            shape: Shape::Whole,
        };
        let mut buf = [0u8; 4];
        let n = encode_into(&d, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"gggg");
    }
}
