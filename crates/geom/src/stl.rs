//! Binary and ASCII STL reader.
//!
//! Both forms give the same mesh for the same triangles: coordinates are read as f32 (the
//! format's precision) and widened to f64. Facet normals are ignored. Sizes are checked
//! with overflow-safe arithmetic against the real input length before anything is
//! allocated, so a hostile header cannot make the reader allocate more than the input.

use crate::error::{GeomError, Result};
use crate::mesh::Mesh;
use std::path::Path;

const HEADER: usize = 84;
const RECORD: usize = 50;

/// Read an STL file from disk.
///
/// # Errors
/// Returns [`GeomError::Io`] when the file cannot be read, or the parse error.
pub fn read_stl_file(path: &Path) -> Result<Mesh> {
    let bytes = std::fs::read(path).map_err(|e| GeomError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    parse_stl(&bytes)
}

/// Parse STL bytes (binary or ASCII) into a welded, Morton-ordered [`Mesh`].
///
/// A file is binary when its size is exactly 84 + 50 * (triangle count in the header);
/// otherwise a file that starts with `solid` is ASCII. Anything else is an error.
///
/// # Errors
/// Returns an error for a size mismatch, malformed ASCII, a non-finite coordinate or a
/// file with no triangles. It never panics on any input.
pub fn parse_stl(bytes: &[u8]) -> Result<Mesh> {
    if bytes.len() >= HEADER {
        let declared = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]);
        let expected = HEADER as u64 + RECORD as u64 * u64::from(declared);
        if expected == bytes.len() as u64 {
            return parse_binary(bytes);
        }
        if !starts_with_solid(bytes) {
            return Err(GeomError::StlSizeMismatch {
                declared_triangles: declared,
                expected_bytes: expected,
                actual_bytes: bytes.len(),
            });
        }
    } else if !starts_with_solid(bytes) {
        return Err(GeomError::StlUnrecognized { bytes: bytes.len() });
    }
    parse_ascii(bytes)
}

fn starts_with_solid(bytes: &[u8]) -> bool {
    let t = bytes.trim_ascii_start();
    t.len() >= 5 && t[..5].eq_ignore_ascii_case(b"solid")
}

fn f32_at(b: &[u8], at: usize) -> f64 {
    f64::from(f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]))
}

fn parse_binary(bytes: &[u8]) -> Result<Mesh> {
    let body = &bytes[HEADER..];
    let corners = body.len() / RECORD * 3;
    let mut sx = Vec::with_capacity(corners);
    let mut sy = Vec::with_capacity(corners);
    let mut sz = Vec::with_capacity(corners);
    for rec in body.chunks_exact(RECORD) {
        // 12 bytes of normal are skipped, then three corners of three f32 each.
        for k in 0..3 {
            let at = 12 + 12 * k;
            sx.push(f32_at(rec, at));
            sy.push(f32_at(rec, at + 4));
            sz.push(f32_at(rec, at + 8));
        }
    }
    if sx.is_empty() {
        return Err(GeomError::StlAscii {
            line: 0,
            message: "binary STL holds no triangles".into(),
        });
    }
    Mesh::from_soup(&sx, &sy, &sz)
}

fn ascii_err(line: usize, message: &str) -> GeomError {
    GeomError::StlAscii {
        line,
        message: message.to_owned(),
    }
}

/// Parse `vertex x y z` operands (after the keyword) as finite f32 values.
fn vertex_operands<'a>(line: usize, mut toks: impl Iterator<Item = &'a str>) -> Result<[f64; 3]> {
    let mut v = [0.0_f64; 3];
    for slot in &mut v {
        let t = toks
            .next()
            .ok_or_else(|| ascii_err(line, "vertex needs three numbers"))?;
        let x: f32 = t
            .parse()
            .map_err(|_| ascii_err(line, "vertex coordinate is not a number"))?;
        if !x.is_finite() {
            return Err(ascii_err(line, "vertex coordinate is not finite"));
        }
        *slot = f64::from(x);
    }
    if toks.next().is_some() {
        return Err(ascii_err(line, "vertex has more than three numbers"));
    }
    Ok(v)
}

fn parse_ascii(bytes: &[u8]) -> Result<Mesh> {
    let text = std::str::from_utf8(bytes).map_err(|e| {
        let line = 1 + bytes[..e.valid_up_to()]
            .iter()
            .filter(|&&b| b == b'\n')
            .count();
        ascii_err(line, "not valid UTF-8 text")
    })?;
    let (mut sx, mut sy, mut sz) = (Vec::new(), Vec::new(), Vec::new());
    // Vertices of the open facet; they join the mesh only when `endfacet` validates it.
    let mut open: Option<Vec<[f64; 3]>> = None;
    let mut last_line = 0;
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        last_line = line;
        let mut toks = raw.split_ascii_whitespace();
        let Some(word) = toks.next() else { continue };
        if word.eq_ignore_ascii_case("facet") {
            if open.is_some() {
                return Err(ascii_err(line, "facet starts before the previous endfacet"));
            }
            open = Some(Vec::with_capacity(3));
        } else if word.eq_ignore_ascii_case("vertex") {
            match open.as_mut() {
                Some(vs) if vs.len() < 3 => vs.push(vertex_operands(line, toks)?),
                _ => return Err(ascii_err(line, "vertex outside a three-vertex facet")),
            }
        } else if word.eq_ignore_ascii_case("endfacet") {
            match open.take() {
                Some(vs) if vs.len() == 3 => {
                    for v in vs {
                        sx.push(v[0]);
                        sy.push(v[1]);
                        sz.push(v[2]);
                    }
                }
                _ => {
                    return Err(ascii_err(
                        line,
                        "facet does not have exactly three vertices",
                    ));
                }
            }
        }
    }
    if open.is_some() {
        return Err(ascii_err(last_line, "file ends inside a facet"));
    }
    if sx.is_empty() {
        return Err(ascii_err(last_line, "no facets"));
    }
    Mesh::from_soup(&sx, &sy, &sz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::cube_soup;

    /// Binary STL bytes for a soup (normals zero), with `header` text in the 80-byte header.
    pub(crate) fn binary_stl(header: &str, s: &(Vec<f64>, Vec<f64>, Vec<f64>)) -> Vec<u8> {
        let mut b = vec![0_u8; 80];
        b[..header.len()].copy_from_slice(header.as_bytes());
        let n = (s.0.len() / 3) as u32;
        b.extend_from_slice(&n.to_le_bytes());
        for t in 0..s.0.len() / 3 {
            b.extend_from_slice(&[0_u8; 12]);
            for k in 3 * t..3 * t + 3 {
                for v in [s.0[k], s.1[k], s.2[k]] {
                    b.extend_from_slice(&(v as f32).to_le_bytes());
                }
            }
            b.extend_from_slice(&[0_u8; 2]);
        }
        b
    }

    pub(crate) fn ascii_stl(s: &(Vec<f64>, Vec<f64>, Vec<f64>)) -> String {
        let mut o = String::from("solid cube\n");
        for t in 0..s.0.len() / 3 {
            o.push_str("  facet normal 0 0 0\n    outer loop\n");
            for k in 3 * t..3 * t + 3 {
                o.push_str(&format!(
                    "      vertex {:e} {:e} {:e}\n",
                    s.0[k], s.1[k], s.2[k]
                ));
            }
            o.push_str("    endloop\n  endfacet\n");
        }
        o.push_str("endsolid cube\n");
        o
    }

    #[test]
    fn binary_and_ascii_give_the_same_mesh() {
        let s = cube_soup();
        let bin = parse_stl(&binary_stl("STLB test", &s)).expect("binary");
        let asc = parse_stl(ascii_stl(&s).as_bytes()).expect("ascii");
        assert_eq!(bin, asc);
        assert_eq!((bin.vertex_count(), bin.triangle_count()), (8, 12));
    }

    #[test]
    fn binary_file_whose_header_starts_with_solid_is_still_binary() {
        let s = cube_soup();
        let m = parse_stl(&binary_stl("solid exported by a CAD tool", &s)).expect("binary");
        assert_eq!(m.triangle_count(), 12);
    }

    #[test]
    fn truncated_and_oversized_headers_are_errors_not_allocations() {
        let s = cube_soup();
        let mut b = binary_stl("STLB", &s);
        b.truncate(b.len() - 1);
        assert!(matches!(
            parse_stl(&b),
            Err(GeomError::StlSizeMismatch {
                declared_triangles: 12,
                ..
            })
        ));
        // Header claims 4 billion triangles in an 84-byte file.
        let mut huge = vec![0_u8; 84];
        huge[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
        match parse_stl(&huge) {
            Err(GeomError::StlSizeMismatch {
                declared_triangles,
                expected_bytes,
                actual_bytes,
            }) => {
                assert_eq!(declared_triangles, u32::MAX);
                assert_eq!(expected_bytes, 84 + 50 * u64::from(u32::MAX));
                assert_eq!(actual_bytes, 84);
            }
            other => panic!("expected size mismatch, got {other:?}"),
        }
        assert!(matches!(
            parse_stl(b"abc"),
            Err(GeomError::StlUnrecognized { bytes: 3 })
        ));
        assert!(matches!(
            parse_stl(b""),
            Err(GeomError::StlUnrecognized { bytes: 0 })
        ));
        // An empty binary body (count 0, exactly 84 bytes) holds no triangles.
        assert!(parse_stl(&[0_u8; 84]).is_err());
    }

    #[test]
    fn non_finite_binary_coordinates_are_rejected() {
        let s = cube_soup();
        let mut b = binary_stl("STLB", &s);
        // First corner x of the first record is at 84 + 12.
        b[96..100].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(matches!(parse_stl(&b), Err(GeomError::NonFinite { .. })));
    }

    #[test]
    fn malformed_ascii_is_located() {
        let bad = |t: &str| parse_stl(t.as_bytes());
        assert!(matches!(
            bad("solid x\nendsolid x\n"),
            Err(GeomError::StlAscii { .. })
        ));
        let two = "solid x\nfacet normal 0 0 0\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nendloop\nendfacet\n";
        assert!(matches!(bad(two), Err(GeomError::StlAscii { line: 7, .. })));
        let nan = "solid x\nfacet normal 0 0 0\nvertex nan 0 0\n";
        assert!(matches!(bad(nan), Err(GeomError::StlAscii { line: 3, .. })));
        let inf = "solid x\nfacet normal 0 0 0\nvertex 1e999 0 0\n";
        assert!(matches!(bad(inf), Err(GeomError::StlAscii { line: 3, .. })));
        let open = "solid x\nfacet normal 0 0 0\nvertex 0 0 0\n";
        assert!(matches!(bad(open), Err(GeomError::StlAscii { .. })));
        assert!(matches!(
            parse_stl(b"solid x\n\xff\xfe\n"),
            Err(GeomError::StlAscii { line: 2, .. })
        ));
    }

    #[test]
    fn unclosed_extra_and_stray_ascii_structure_is_rejected() {
        let tri = "vertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\n";
        let bad = |t: &str| parse_stl(t.as_bytes());
        // A second facet starts before the first one is closed.
        let nested =
            format!("solid x\nfacet normal 0 0 0\n{tri}facet normal 0 0 0\n{tri}endfacet\n");
        assert!(matches!(
            bad(&nested),
            Err(GeomError::StlAscii { line: 6, .. })
        ));
        // The first facet is closed and the second never is.
        let tail = format!("solid x\nfacet normal 0 0 0\n{tri}endfacet\nfacet normal 0 0 0\n{tri}");
        assert!(matches!(bad(&tail), Err(GeomError::StlAscii { .. })));
        // Four vertices in one facet.
        let four = format!("solid x\nfacet normal 0 0 0\n{tri}vertex 1 1 0\nendfacet\n");
        assert!(matches!(
            bad(&four),
            Err(GeomError::StlAscii { line: 6, .. })
        ));
        // A vertex between facets, and an endfacet with no facet.
        let stray = format!("solid x\nfacet normal 0 0 0\n{tri}endfacet\nvertex 0 0 0\n");
        assert!(matches!(
            bad(&stray),
            Err(GeomError::StlAscii { line: 7, .. })
        ));
        let orphan = format!("solid x\nfacet normal 0 0 0\n{tri}endfacet\nendfacet\n");
        assert!(matches!(
            bad(&orphan),
            Err(GeomError::StlAscii { line: 7, .. })
        ));
        // A well-formed single facet still parses to one triangle.
        let ok = format!("solid x\nfacet normal 0 0 0\n{tri}endfacet\nendsolid x\n");
        assert_eq!(bad(&ok).expect("one facet").triangle_count(), 1);
    }

    #[test]
    fn missing_file_is_an_io_error_with_the_path() {
        match read_stl_file(Path::new("/nonexistent/plinth/none.stl")) {
            Err(GeomError::Io { path, .. }) => assert!(path.ends_with("none.stl")),
            other => panic!("expected Io error, got {other:?}"),
        }
    }
}
