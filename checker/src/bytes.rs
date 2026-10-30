//! Checked little-endian reads. Every read names what it was reading, so a truncated or
//! inconsistent file gives an error that says where, never a panic.

use std::fmt;

/// A file could not be read as the format it claims to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError(pub String);

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ReadError {}

pub type Res<T> = Result<T, ReadError>;

/// Build an error result.
pub fn fail<T>(msg: impl Into<String>) -> Res<T> {
    Err(ReadError(msg.into()))
}

/// `usize` to `u64`. Lengths in memory always fit.
#[must_use]
pub fn wide(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// `buf[off .. off + len]`, or an error naming `what`.
pub fn slice<'a>(buf: &'a [u8], off: u64, len: u64, what: &str) -> Res<&'a [u8]> {
    let Some(end) = off.checked_add(len) else {
        return fail(format!("{what}: offset {off} + length {len} overflows"));
    };
    if end > wide(buf.len()) {
        return fail(format!(
            "{what}: bytes [{off}, {end}) lie past the end of the file ({} bytes)",
            buf.len()
        ));
    }
    let (Ok(a), Ok(b)) = (usize::try_from(off), usize::try_from(end)) else {
        return fail(format!("{what}: offset {off} does not fit in memory"));
    };
    Ok(&buf[a..b])
}

fn array_at<const N: usize>(buf: &[u8], off: u64, what: &str) -> Res<[u8; N]> {
    let s = slice(buf, off, wide(N), what)?;
    let mut a = [0u8; N];
    a.copy_from_slice(s);
    Ok(a)
}

pub fn u32_at(buf: &[u8], off: u64, what: &str) -> Res<u32> {
    Ok(u32::from_le_bytes(array_at::<4>(buf, off, what)?))
}

pub fn u64_at(buf: &[u8], off: u64, what: &str) -> Res<u64> {
    Ok(u64::from_le_bytes(array_at::<8>(buf, off, what)?))
}

pub fn f32_at(buf: &[u8], off: u64, what: &str) -> Res<f32> {
    Ok(f32::from_le_bytes(array_at::<4>(buf, off, what)?))
}

/// Three consecutive f32 values.
pub fn f32x3_at(buf: &[u8], off: u64, what: &str) -> Res<[f32; 3]> {
    Ok([
        f32_at(buf, off, what)?,
        f32_at(buf, off + 4, what)?,
        f32_at(buf, off + 8, what)?,
    ])
}

/// All of `bytes` as little-endian f32 values, in one pass. A trailing partial value is dropped
/// (callers pass whole multiples of four).
#[must_use]
pub fn f32_batch(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// True if every byte is zero.
#[must_use]
pub fn all_zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_little_endian_values() {
        let b = [1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(u32_at(&b, 0, "a"), Ok(1));
        assert_eq!(u64_at(&b, 4, "b"), Ok(2));
        let f = 1.5f32.to_le_bytes();
        assert_eq!(f32_at(&f, 0, "c"), Ok(1.5));
    }

    #[test]
    fn out_of_range_reads_are_errors_that_name_the_field() {
        let b = [0u8; 6];
        let e = u32_at(&b, 4, "model count").unwrap_err();
        assert!(e.0.contains("model count"), "{e}");
        assert!(slice(&b, u64::MAX, 2, "x").is_err());
        assert!(slice(&b, 0, 7, "x").is_err());
        assert_eq!(slice(&b, 6, 0, "x").map(<[u8]>::len), Ok(0));
    }

    #[test]
    fn batch_read_converts_whole_values() {
        let mut b = Vec::new();
        for v in [1.0f32, -2.5, 3.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(f32_batch(&b), vec![1.0, -2.5, 3.0]);
        assert!(all_zero(&[0, 0]) && !all_zero(&[0, 1]));
    }
}
