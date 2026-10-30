//! The preview image and its run-length codec (spec section 3).
//!
//! Pixels are u16 words: red bits 15..11, green bits 10..6, blue bits 4..0, bit 5
//! never part of a colour. In the file a word with bit 5 clear is one pixel; a word
//! with bit 5 set is followed by `0x3000 | (run - 1)` and stands for a run of 2 to
//! 4095 identical pixels. Runs are greedy.

use crate::bytes::err;
use crate::error::{ParseError, ParseErrorKind, WriteError};

/// Bit 5 of a word: the repeat flag. Never part of a colour.
pub const REPEAT_FLAG: u16 = 0x20;
/// Longest run one run word pair can hold.
pub const MAX_RUN: usize = 4095;
/// The near-black background word ChituBox uses.
pub const BACKGROUND: u16 = 0x0841;
/// Most pixels the reader decodes and the writer emits (64 MB of pixels). Samples reach
/// 7.1 million.
pub const MAX_PIXELS: u64 = 1 << 25;

/// A decoded preview image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height` pixel words, row by row, none with [`REPEAT_FLAG`] set.
    pub pixels: Vec<u16>,
}

impl Preview {
    /// A preview of one colour. The pixel limit [`MAX_PIXELS`] is checked before anything
    /// is allocated; a larger size is [`WriteError::BadPreview`].
    ///
    /// # Errors
    /// `width * height` is above [`MAX_PIXELS`].
    pub fn solid(width: u32, height: u32, pixel: u16) -> Result<Self, WriteError> {
        let n = u64::from(width) * u64::from(height);
        if n > MAX_PIXELS {
            return Err(WriteError::BadPreview);
        }
        let n = usize::try_from(n).map_err(|_| WriteError::BadPreview)?;
        Ok(Self {
            width,
            height,
            pixels: vec![pixel & !REPEAT_FLAG; n],
        })
    }

    /// Check the size limit, the pixel count and the flag bits. The limit is the one the
    /// reader enforces, so everything the writer emits can be read back.
    pub(crate) fn validate(&self) -> Result<(), WriteError> {
        let want = u64::from(self.width) * u64::from(self.height);
        if want > MAX_PIXELS {
            return Err(WriteError::BadPreview);
        }
        let flagged = self.pixels.iter().any(|&p| p & REPEAT_FLAG != 0);
        if want == self.pixels.len() as u64 && !flagged {
            Ok(())
        } else {
            Err(WriteError::BadPreview)
        }
    }

    /// The file form of the pixel data: greedy runs, longest 4095.
    pub(crate) fn encode(&self) -> Result<Vec<u8>, WriteError> {
        self.validate()?;
        Ok(encode_pixels(&self.pixels))
    }
}

/// Pack 8-bit RGB into a preview pixel word (5 bits per channel, bit 5 clear).
#[must_use]
pub fn pixel_from_rgb8(r: u8, g: u8, b: u8) -> u16 {
    (u16::from(r >> 3) << 11) | (u16::from(g >> 3) << 6) | u16::from(b >> 3)
}

/// Unpack a preview pixel word to 8-bit RGB (low bits filled by replication).
#[must_use]
pub fn pixel_to_rgb8(p: u16) -> [u8; 3] {
    let expand = |v: u16| -> u8 {
        let v = u8::try_from(v & 31).unwrap_or(0);
        (v << 3) | (v >> 2)
    };
    [expand(p >> 11), expand(p >> 6), expand(p)]
}

fn encode_pixels(pixels: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() / 2 + 16);
    for run in pixels.chunk_by(|a, b| a == b) {
        let colour = run[0];
        for part in run.chunks(MAX_RUN) {
            if part.len() == 1 {
                out.extend_from_slice(&colour.to_le_bytes());
            } else {
                out.extend_from_slice(&(colour | REPEAT_FLAG).to_le_bytes());
                // part.len() is 2..=4095 here, so len - 1 fits in 12 bits.
                let count = u16::try_from(part.len() - 1).unwrap_or(0);
                out.extend_from_slice(&(0x3000 | count).to_le_bytes());
            }
        }
    }
    out
}

/// Decode `data` (at absolute offset `at`) into exactly `width * height` pixels.
/// The data must be the canonical encoding of those pixels.
pub(crate) fn decode(width: u32, height: u32, data: &[u8], at: u64) -> Result<Preview, ParseError> {
    let total = u64::from(width) * u64::from(height);
    let invalid = |value| {
        err(
            at,
            ParseErrorKind::Invalid {
                what: "preview",
                value,
            },
        )
    };
    // Most pixels `data` can hold: 4095 per 4 bytes, plus one per leftover word.
    let len = data.len() as u64;
    let capacity = (len / 4) * (MAX_RUN as u64) + (len % 4) / 2;
    if !len.is_multiple_of(2) || total > MAX_PIXELS || total > capacity {
        return Err(invalid(total));
    }
    let total_len = usize::try_from(total).map_err(|_| invalid(total))?;
    let mut pixels: Vec<u16> = Vec::with_capacity(total_len);
    let mut words = data
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]));
    while let Some(w) = words.next() {
        let (colour, run) = if w & REPEAT_FLAG == 0 {
            (w, 1)
        } else {
            let second = words.next().ok_or_else(|| invalid(total))?;
            let run = usize::from(second & 0x0FFF) + 1;
            if second & 0xF000 != 0x3000 || run < 2 {
                return Err(invalid(u64::from(second)));
            }
            (w & !REPEAT_FLAG, run)
        };
        if run > total_len - pixels.len() {
            return Err(invalid(total));
        }
        pixels.resize(pixels.len() + run, colour);
    }
    if pixels.len() != total_len {
        return Err(invalid(total));
    }
    if encode_pixels(&pixels) != data {
        return Err(err(at, ParseErrorKind::NonCanonicalPreview));
    }
    Ok(Preview {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(p: &Preview) -> Preview {
        let bytes = p.encode().unwrap();
        decode(p.width, p.height, &bytes, 452).unwrap()
    }

    #[test]
    fn long_run_splits_at_4095() {
        // 4096 identical pixels: one run of 4095 then a single word (spec section 3).
        let p = Preview::solid(4096, 1, 0x1234 & !REPEAT_FLAG).unwrap();
        let bytes = p.encode().unwrap();
        assert_eq!(bytes.len(), 6);
        assert_eq!(&bytes[2..4], &(0x3000u16 | 4094).to_le_bytes());
        assert_eq!(round_trip(&p), p);
    }

    #[test]
    fn mixed_runs_round_trip() {
        let mut px = vec![BACKGROUND; 10];
        px.push(0x7c00);
        px.extend([0x03c0, 0x03c0]);
        px.push(BACKGROUND);
        let p = Preview {
            width: 14,
            height: 1,
            pixels: px,
        };
        assert_eq!(round_trip(&p), p);
    }

    #[test]
    fn non_greedy_encoding_is_rejected() {
        // Two single words of the same colour: decodes to 2 pixels but is not canonical.
        let w = 0x1000u16.to_le_bytes();
        let data = [w[0], w[1], w[0], w[1]];
        assert!(matches!(
            decode(2, 1, &data, 0).unwrap_err().kind,
            ParseErrorKind::NonCanonicalPreview
        ));
    }

    #[test]
    fn bad_dimensions_and_runs_are_errors() {
        assert!(decode(u32::MAX, u32::MAX, &[], 0).is_err());
        assert!(decode(2, 1, &[0x20, 0], 0).is_err()); // flag word with no run word
        let over = [0x20, 0, 0xff, 0x3f]; // run of 4096
        assert!(decode(2, 1, &over, 0).is_err());
        assert!(decode(3, 1, &[0x20, 0, 0x01, 0x30], 0).is_err()); // run 2 for 3 pixels
        assert!(decode(2, 1, &[0x20, 0, 0x01, 0x20], 0).is_err()); // bad run marker
    }

    #[test]
    fn encode_rejects_wrong_pixel_count_and_flag_bit() {
        let mut p = Preview::solid(2, 2, 5).unwrap();
        p.pixels.pop();
        assert_eq!(p.encode(), Err(WriteError::BadPreview));
        let q = Preview {
            width: 1,
            height: 1,
            pixels: vec![REPEAT_FLAG],
        };
        assert_eq!(q.encode(), Err(WriteError::BadPreview));
    }

    #[test]
    fn pixel_limit_is_a_construction_rule_for_writer_and_reader() {
        let max = u32::try_from(MAX_PIXELS).unwrap();
        // One over the limit: refused before any allocation, also at hostile sizes.
        assert_eq!(Preview::solid(max + 1, 1, 0), Err(WriteError::BadPreview));
        assert_eq!(
            Preview::solid(u32::MAX, u32::MAX, 0),
            Err(WriteError::BadPreview)
        );
        // Public fields bypass `solid`; the writer check still refuses.
        for (width, height, len) in [
            (u32::MAX, u32::MAX, 0),
            (max + 1, 1, 0),
            (max + 1, 1, (max + 1) as usize),
        ] {
            let p = Preview {
                width,
                height,
                pixels: vec![0; len],
            };
            assert_eq!(p.encode(), Err(WriteError::BadPreview));
        }
        // Exactly at the limit: written, and read back by the same limit.
        let p = Preview::solid(max, 1, BACKGROUND).unwrap();
        assert_eq!(round_trip(&p), p);
    }

    #[test]
    fn rgb_helpers_keep_bit5_clear() {
        for v in [0u8, 7, 8, 100, 255] {
            assert_eq!(pixel_from_rgb8(v, v, v) & REPEAT_FLAG, 0);
        }
        assert_eq!(pixel_to_rgb8(pixel_from_rgb8(255, 0, 255)), [255, 0, 255]);
    }
}
