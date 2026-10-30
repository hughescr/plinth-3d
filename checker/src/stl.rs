//! The checker's own STL reader (binary and ASCII). It shares nothing with the generator's.

use crate::bytes::{Res, f32_batch, fail, wide};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StlFormat {
    Binary,
    Ascii,
}

impl StlFormat {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Ascii => "ascii",
        }
    }
}

/// A triangle soup: nine f32 per triangle (three vertices, x y z each).
#[derive(Debug, Clone, PartialEq)]
pub struct Stl {
    pub format: StlFormat,
    pub coords: Vec<f32>,
}

impl Stl {
    #[must_use]
    pub fn triangles(&self) -> u64 {
        wide(self.coords.len() / 9)
    }

    /// (min, max) corners, or None for an empty mesh or one with a non-finite coordinate.
    #[must_use]
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        if self.coords.is_empty() || self.coords.iter().any(|c| !c.is_finite()) {
            return None;
        }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in self.coords.chunks_exact(3) {
            for ((l, h), c) in lo.iter_mut().zip(hi.iter_mut()).zip(v) {
                *l = l.min(*c);
                *h = h.max(*c);
            }
        }
        Some((lo, hi))
    }

    /// Signed volume in mm3 (positive for counter-clockwise triangles seen from outside).
    #[must_use]
    pub fn signed_volume(&self) -> f64 {
        self.coords
            .chunks_exact(9)
            .map(|t| {
                let p = |i: usize| [f64::from(t[i]), f64::from(t[i + 1]), f64::from(t[i + 2])];
                let (a, b, c) = (p(0), p(3), p(6));
                let cross = [
                    b[1] * c[2] - b[2] * c[1],
                    b[2] * c[0] - b[0] * c[2],
                    b[0] * c[1] - b[1] * c[0],
                ];
                (a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]) / 6.0
            })
            .sum()
    }
}

/// Read an STL file. Binary is recognised by its size (84 + 50 per triangle); anything else must
/// be ASCII (`solid` ... `vertex x y z` ...).
pub fn parse(buf: &[u8]) -> Res<Stl> {
    if buf.len() >= 84 {
        let n = u64::from(u32::from_le_bytes([buf[80], buf[81], buf[82], buf[83]]));
        if 84 + 50 * n == wide(buf.len()) {
            return Ok(parse_binary(&buf[84..]));
        }
    }
    if buf.starts_with(b"solid") {
        return parse_ascii(buf);
    }
    fail("not an STL file: the size does not fit a binary STL and it does not start with 'solid'")
}

fn parse_binary(body: &[u8]) -> Stl {
    let mut coords = Vec::with_capacity(body.len() / 50 * 9);
    for tri in body.chunks_exact(50) {
        coords.extend(f32_batch(&tri[12..48]));
    }
    Stl {
        format: StlFormat::Binary,
        coords,
    }
}

fn parse_ascii(buf: &[u8]) -> Res<Stl> {
    let Ok(text) = std::str::from_utf8(buf) else {
        return fail("ASCII STL is not valid UTF-8");
    };
    let mut coords = Vec::new();
    let mut words = text.split_ascii_whitespace();
    while let Some(w) = words.next() {
        if w != "vertex" {
            continue;
        }
        for axis in ["x", "y", "z"] {
            let Some(tok) = words.next() else {
                return fail(format!("ASCII STL ends inside a vertex (missing {axis})"));
            };
            let Ok(v) = tok.parse::<f32>() else {
                return fail(format!("ASCII STL has a bad number '{tok}'"));
            };
            coords.push(v);
        }
    }
    if coords.is_empty() || !coords.len().is_multiple_of(9) {
        return fail(format!(
            "ASCII STL has {} vertices, not a positive multiple of 3",
            coords.len() / 3
        ));
    }
    Ok(Stl {
        format: StlFormat::Ascii,
        coords,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binary(tris: &[[f32; 9]]) -> Vec<u8> {
        let mut b = vec![0u8; 80];
        b.extend_from_slice(&u32::try_from(tris.len()).expect("small").to_le_bytes());
        for t in tris {
            b.extend_from_slice(&[0u8; 12]);
            for c in t {
                b.extend_from_slice(&c.to_le_bytes());
            }
            b.extend_from_slice(&[0u8; 2]);
        }
        b
    }

    #[test]
    fn binary_and_ascii_agree_on_the_same_triangle() {
        let t = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let b = parse(&binary(&[t])).expect("binary");
        let a = parse(
            b"solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid x\n",
        )
        .expect("ascii");
        assert_eq!(b.format, StlFormat::Binary);
        assert_eq!(a.format, StlFormat::Ascii);
        assert_eq!(a.coords, b.coords);
        assert_eq!(b.triangles(), 1);
    }

    #[test]
    fn tetrahedron_has_positive_volume_and_exact_bounds() {
        let (o, x, y, z) = (
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        );
        let tri = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| {
            let mut t = [0.0; 9];
            t[..3].copy_from_slice(&a);
            t[3..6].copy_from_slice(&b);
            t[6..].copy_from_slice(&c);
            t
        };
        let m = parse(&binary(&[
            tri(o, y, x),
            tri(o, x, z),
            tri(o, z, y),
            tri(x, y, z),
        ]))
        .expect("parses");
        assert!(
            (m.signed_volume() - 1.0 / 6.0).abs() < 1e-12,
            "{}",
            m.signed_volume()
        );
        assert_eq!(m.bounds(), Some(([0.0; 3], [1.0; 3])));
    }

    #[test]
    fn garbage_and_truncated_files_are_errors() {
        assert!(parse(b"hello").is_err());
        let mut b = binary(&[[0.0; 9]]);
        b.pop();
        assert!(parse(&b).is_err());
        assert!(parse(b"solid x\nvertex 1 2\n").is_err());
        assert!(parse(b"solid x\nvertex 1 2 zz\n").is_err());
    }
}
