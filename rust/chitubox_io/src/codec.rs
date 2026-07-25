//! Bounds-checked little-endian primitive codec. All integers u32 LE, all
//! floats f32 LE (format invariant). Reads never panic; writes assume the
//! caller sized the buffer (internal use only).

use crate::error::{ChituboxError, Result};

#[inline]
pub(crate) fn read_u32(buf: &[u8], off: usize, context: &'static str) -> Result<u32> {
    let end = off
        .checked_add(4)
        .ok_or(ChituboxError::ArithmeticOverflow { context })?;
    if end > buf.len() {
        return Err(ChituboxError::Truncated {
            need: end,
            have: buf.len(),
            context,
        });
    }
    Ok(u32::from_le_bytes([
        buf[off],
        buf[off + 1],
        buf[off + 2],
        buf[off + 3],
    ]))
}

#[inline]
pub(crate) fn read_f32(buf: &[u8], off: usize, context: &'static str) -> Result<f32> {
    Ok(f32::from_bits(read_u32(buf, off, context)?))
}

#[inline]
pub(crate) fn write_u32(buf: &mut [u8], off: usize, val: u32) {
    buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
}

/// `a + b`, or `ArithmeticOverflow{context}`.
#[inline]
pub(crate) fn add(a: usize, b: usize, context: &'static str) -> Result<usize> {
    a.checked_add(b)
        .ok_or(ChituboxError::ArithmeticOverflow { context })
}

/// `a * b`, or `ArithmeticOverflow{context}`.
#[inline]
pub(crate) fn mul(a: usize, b: usize, context: &'static str) -> Result<usize> {
    a.checked_mul(b)
        .ok_or(ChituboxError::ArithmeticOverflow { context })
}

/// Narrow a layout offset to u32, or `FileTooLarge`.
#[inline]
pub(crate) fn u32_of(v: usize) -> Result<u32> {
    u32::try_from(v).map_err(|_| ChituboxError::FileTooLarge { size: v as u64 })
}
