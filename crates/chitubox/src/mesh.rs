//! Triangle soup: 9 little-endian f32 per triangle, 36 bytes, no normals, no count
//! (spec section 5). Whole slices convert in one pass.

use crate::bytes::err;
use crate::error::{ParseError, ParseErrorKind, WriteError};

/// Bytes per triangle in the file.
pub const TRIANGLE_BYTES: u64 = 36;

/// One triangle: three vertices, xyz each. Counter-clockwise seen from outside.
pub type Triangle = [[f32; 3]; 3];

/// Decode a mesh from `bytes` (found at absolute offset `at`). Fails unless the
/// length is a multiple of 36 and every coordinate is finite.
pub(crate) fn decode(
    bytes: &[u8],
    at: u64,
    what: &'static str,
) -> Result<Vec<Triangle>, ParseError> {
    if !bytes.len().is_multiple_of(36) {
        return Err(err(
            at,
            ParseErrorKind::Invalid {
                what,
                value: bytes.len() as u64,
            },
        ));
    }
    let mut out: Vec<Triangle> = Vec::with_capacity(bytes.len() / 36);
    let mut finite = true;
    for chunk in bytes.chunks_exact(36) {
        let mut flat = [0f32; 9];
        for (dst, src) in flat.iter_mut().zip(chunk.chunks_exact(4)) {
            *dst = f32::from_le_bytes([src[0], src[1], src[2], src[3]]);
            finite &= dst.is_finite();
        }
        out.push([
            [flat[0], flat[1], flat[2]],
            [flat[3], flat[4], flat[5]],
            [flat[6], flat[7], flat[8]],
        ]);
    }
    if finite {
        Ok(out)
    } else {
        Err(err(at, ParseErrorKind::NonFinite { what }))
    }
}

/// True when every coordinate of every triangle is finite.
pub(crate) fn all_finite(tris: &[Triangle]) -> bool {
    tris.iter().flatten().flatten().all(|v| v.is_finite())
}

/// Append the file form of `tris` to `out`.
pub(crate) fn encode_into(tris: &[Triangle], out: &mut Vec<u8>) {
    out.reserve(tris.len() * 36);
    for v in tris.iter().flatten().flatten() {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// Size in bytes of `tris` in the file.
pub(crate) fn byte_len(tris: &[Triangle], what: &'static str) -> Result<u64, WriteError> {
    (tris.len() as u64)
        .checked_mul(TRIANGLE_BYTES)
        .ok_or(WriteError::TooLarge { what })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_round_trips_and_rejects_bad_length() {
        let tris: Vec<Triangle> = vec![
            [[0.0, 1.0, 2.0], [3.0, 4.0, 5.0], [6.0, 7.0, -0.0]],
            [[1.5; 3], [2.5; 3], [3.5; 3]],
        ];
        let mut bytes = Vec::new();
        encode_into(&tris, &mut bytes);
        assert_eq!(bytes.len(), 72);
        let back = decode(&bytes, 0, "mesh").unwrap();
        assert_eq!(back, tris);
        assert!(decode(&bytes[..71], 0, "mesh").is_err());
    }

    #[test]
    fn nonfinite_mesh_is_rejected() {
        let mut bytes = Vec::new();
        encode_into(&[[[f32::INFINITY, 0.0, 0.0]; 3]], &mut bytes);
        assert!(matches!(
            decode(&bytes, 0, "mesh").unwrap_err().kind,
            ParseErrorKind::NonFinite { .. }
        ));
    }
}
