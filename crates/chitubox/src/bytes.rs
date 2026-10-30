//! Bounds-checked little-endian cursor used by the reader. Every size is checked
//! before it is used; no read can panic or allocate from an unchecked count.

use crate::error::{ParseError, ParseErrorKind};

/// A cursor over the whole file or over a slice of it. `base` is the absolute
/// file offset of `data[0]`, so errors always report absolute offsets.
#[derive(Debug)]
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    base: u64,
    pos: usize,
}

/// Convert a file size or offset to `usize`, failing instead of truncating.
pub(crate) fn usize_of(v: u64, what: &'static str, at: u64) -> Result<usize, ParseError> {
    usize::try_from(v).map_err(|_| err(at, ParseErrorKind::Overflow { what }))
}

pub(crate) fn err(offset: u64, kind: ParseErrorKind) -> ParseError {
    ParseError { offset, kind }
}

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            base: 0,
            pos: 0,
        }
    }

    /// Absolute offset of the next unread byte.
    pub(crate) fn pos(&self) -> u64 {
        self.base.saturating_add(self.pos as u64)
    }

    pub(crate) fn remaining(&self) -> u64 {
        (self.data.len() - self.pos) as u64
    }

    /// Fail with `Mismatch` unless the cursor is exactly at `found`.
    pub(crate) fn expect_pos(&self, what: &'static str, found: u64) -> Result<(), ParseError> {
        let expected = self.pos();
        if expected == found {
            Ok(())
        } else {
            Err(err(
                expected,
                ParseErrorKind::Mismatch {
                    what,
                    expected,
                    found,
                },
            ))
        }
    }

    pub(crate) fn take(&mut self, what: &'static str, n: u64) -> Result<&'a [u8], ParseError> {
        if n > self.remaining() {
            return Err(err(self.pos(), ParseErrorKind::Truncated { what, need: n }));
        }
        let n = usize_of(n, what, self.pos())?;
        let end = self.pos + n;
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    /// Take `n` bytes and return a reader over them.
    pub(crate) fn sub(&mut self, what: &'static str, n: u64) -> Result<Reader<'a>, ParseError> {
        let base = self.pos();
        let data = self.take(what, n)?;
        Ok(Reader { data, base, pos: 0 })
    }

    /// Look at `n` bytes at absolute offset `off` without moving the cursor.
    pub(crate) fn peek_at(
        &self,
        what: &'static str,
        off: u64,
        n: u64,
    ) -> Result<Reader<'a>, ParseError> {
        let truncated = || err(off, ParseErrorKind::Truncated { what, need: n });
        let start = off.checked_sub(self.base).ok_or_else(truncated)?;
        let end = start.checked_add(n).ok_or_else(truncated)?;
        if end > self.data.len() as u64 {
            return Err(truncated());
        }
        let (s, e) = (usize_of(start, what, off)?, usize_of(end, what, off)?);
        Ok(Reader {
            data: &self.data[s..e],
            base: off,
            pos: 0,
        })
    }

    pub(crate) fn expect_zero(&mut self, what: &'static str, n: u64) -> Result<(), ParseError> {
        let at = self.pos();
        let bytes = self.take(what, n)?;
        if bytes.iter().any(|&b| b != 0) {
            return Err(err(at, ParseErrorKind::NonZero { what }));
        }
        Ok(())
    }

    fn array<const N: usize>(&mut self, what: &'static str) -> Result<[u8; N], ParseError> {
        let bytes = self.take(what, N as u64)?;
        let mut out = [0u8; N];
        out.copy_from_slice(bytes);
        Ok(out)
    }

    pub(crate) fn u32(&mut self, what: &'static str) -> Result<u32, ParseError> {
        Ok(u32::from_le_bytes(self.array(what)?))
    }

    pub(crate) fn u64(&mut self, what: &'static str) -> Result<u64, ParseError> {
        Ok(u64::from_le_bytes(self.array(what)?))
    }

    /// A float that must be finite.
    pub(crate) fn f32(&mut self, what: &'static str) -> Result<f32, ParseError> {
        let at = self.pos();
        let v = f32::from_le_bytes(self.array(what)?);
        if v.is_finite() {
            Ok(v)
        } else {
            Err(err(at, ParseErrorKind::NonFinite { what }))
        }
    }

    pub(crate) fn f32x3(&mut self, what: &'static str) -> Result<[f32; 3], ParseError> {
        Ok([self.f32(what)?, self.f32(what)?, self.f32(what)?])
    }

    /// Fail unless every byte has been read.
    pub(crate) fn finish(&self) -> Result<(), ParseError> {
        let rest = self.remaining();
        if rest == 0 {
            Ok(())
        } else {
            Err(err(
                self.pos(),
                ParseErrorKind::TrailingBytes { count: rest },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_past_end_is_truncated() {
        let mut r = Reader::new(&[1, 2, 3]);
        assert!(matches!(
            r.take("x", 4).unwrap_err().kind,
            ParseErrorKind::Truncated { .. }
        ));
        assert_eq!(r.take("x", 3).unwrap(), &[1, 2, 3]);
        assert!(r.finish().is_ok());
    }

    #[test]
    fn huge_take_does_not_panic() {
        let mut r = Reader::new(&[0; 4]);
        assert!(r.take("x", u64::MAX).is_err());
        assert!(r.peek_at("x", u64::MAX, u64::MAX).is_err());
        assert!(r.peek_at("x", 2, u64::MAX).is_err());
    }

    #[test]
    fn nan_is_rejected() {
        let nan = f32::NAN.to_le_bytes();
        let mut r = Reader::new(&nan);
        assert!(matches!(
            r.f32("v").unwrap_err().kind,
            ParseErrorKind::NonFinite { .. }
        ));
    }

    #[test]
    fn sub_reader_reports_absolute_offsets() {
        let data = [0u8, 0, 0, 0, 9, 9, 9, 9];
        let mut r = Reader::new(&data);
        r.take("skip", 4).unwrap();
        let mut s = r.sub("rec", 4).unwrap();
        assert_eq!(s.pos(), 4);
        assert_eq!(s.u32("v").unwrap(), 0x0909_0909);
    }
}
