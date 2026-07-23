//! Binary STL reader + identity-centering, and centering for raw triangle
//! input. Per CHITUBOX-FORMAT.md §4: 80-byte header, u32 count, 50 B/tri
//! (12 normal + 36 verts + 2 attr); ChituBox drops normal+attr → 36 B/tri
//! and subtracts the bbox center from every vertex (bit-exact).

use crate::codec::{mul, read_f32, read_u32};
use crate::consts::*;
use crate::error::{ChituboxError, Result};

/// Decode a binary STL into a flat 9-f32/tri vertex stream (normals/attr
/// dropped), then identity-center on the bbox center.
pub(crate) fn parse_binary_stl(bytes: &[u8]) -> Result<Vec<f32>> {
    if bytes.len() >= 5 && bytes[..5].eq_ignore_ascii_case(b"solid") {
        // Heuristic: an ASCII STL starts with "solid" and is not a valid
        // binary payload. Confirm by checking the declared count fits.
        if !looks_like_binary(bytes) {
            return Err(ChituboxError::AsciiStlUnsupported);
        }
    }
    if bytes.len() < STL_HEADER_LEN + 4 {
        return Err(ChituboxError::StlParse {
            detail: format!("file too small: {} bytes", bytes.len()),
        });
    }
    let count = read_u32(bytes, STL_HEADER_LEN, "stl count")? as usize;
    let need = STL_HEADER_LEN + 4 + mul(count, STL_BYTES_PER_TRI, "stl body")?;
    if bytes.len() != need {
        return Err(ChituboxError::StlParse {
            detail: format!(
                "size mismatch: header declares {count} tris (expect {need} bytes), have {}",
                bytes.len()
            ),
        });
    }
    let mut xyz = Vec::with_capacity(count * FLOATS_PER_TRI);
    let mut base = STL_HEADER_LEN + 4;
    for _ in 0..count {
        // skip 12-byte normal; read 9 vertex floats; skip 2-byte attr
        let v0 = base + 12;
        for k in 0..FLOATS_PER_TRI {
            xyz.push(read_f32(bytes, v0 + k * 4, "stl vertex")?);
        }
        base += STL_BYTES_PER_TRI;
    }
    Ok(center(xyz))
}

fn looks_like_binary(bytes: &[u8]) -> bool {
    if bytes.len() < STL_HEADER_LEN + 4 {
        return false;
    }
    let count = u32::from_le_bytes([
        bytes[STL_HEADER_LEN],
        bytes[STL_HEADER_LEN + 1],
        bytes[STL_HEADER_LEN + 2],
        bytes[STL_HEADER_LEN + 3],
    ]) as usize;
    STL_HEADER_LEN
        .checked_add(4)
        .and_then(|h| count.checked_mul(STL_BYTES_PER_TRI).and_then(|b| h.checked_add(b)))
        == Some(bytes.len())
}

/// Center an already-decoded 9-f32/tri stream on its bbox center, in place.
pub(crate) fn center(mut xyz: Vec<f32>) -> Vec<f32> {
    let (min, max) = bbox(&xyz);
    let c = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    for chunk in xyz.chunks_exact_mut(3) {
        chunk[0] -= c[0];
        chunk[1] -= c[1];
        chunk[2] -= c[2];
    }
    xyz
}

/// Bounding box of a flat 3-f32/vertex stream. Returns zeros for an empty
/// stream.
pub(crate) fn bbox(xyz: &[f32]) -> ([f32; 3], [f32; 3]) {
    if xyz.len() < 3 {
        return ([0.0; 3], [0.0; 3]);
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for v in xyz.chunks_exact(3) {
        for a in 0..3 {
            if v[a] < min[a] {
                min[a] = v[a];
            }
            if v[a] > max[a] {
                max[a] = v[a];
            }
        }
    }
    (min, max)
}

/// All finite?
pub(crate) fn all_finite(xyz: &[f32]) -> bool {
    xyz.iter().all(|f| f.is_finite())
}
