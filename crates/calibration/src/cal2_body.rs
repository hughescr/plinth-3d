//! CAL-2 coupon bodies: a flat-bottomed, flat-topped square slab. The loaded layer is the
//! slab's first layer. An asymmetric tab and the coupon number are raised on the top face,
//! above the loaded layer, so they add nothing to the body area that peels first.
use plinth_geom::Mesh;
use std::collections::BTreeMap;

pub type Tri = [[f64; 3]; 3];

/// Raised feature height above the top face, mm.
pub const RAISE: f64 = 0.4;
/// Pixel of the raised number font, mm.
pub const PIXEL: f64 = 0.5;
/// Corner tab, mm square.
pub const TAB: f64 = 1.2;
/// Bodies narrower than this carry no tab or number.
pub const MIN_FEATURE_SIDE: f64 = 5.0;

/// 3 x 5 digits, top row first, bit 2 = left column.
const FONT: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 1, 1],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
];

/// Twelve outward-wound triangles for an axis-aligned box.
fn cuboid(out: &mut Vec<Tri>, lo: [f64; 3], hi: [f64; 3]) {
    let p =
        |x: usize, y: usize, z: usize| [[lo[0], hi[0]][x], [lo[1], hi[1]][y], [lo[2], hi[2]][z]];
    let quads = [
        [p(0, 0, 0), p(0, 1, 0), p(1, 1, 0), p(1, 0, 0)],
        [p(0, 0, 1), p(1, 0, 1), p(1, 1, 1), p(0, 1, 1)],
        [p(0, 0, 0), p(1, 0, 0), p(1, 0, 1), p(0, 0, 1)],
        [p(1, 1, 0), p(0, 1, 0), p(0, 1, 1), p(1, 1, 1)],
        [p(0, 1, 0), p(0, 0, 0), p(0, 0, 1), p(0, 1, 1)],
        [p(1, 0, 0), p(1, 1, 0), p(1, 1, 1), p(1, 0, 1)],
    ];
    for q in quads {
        out.push([q[0], q[1], q[2]]);
        out.push([q[0], q[2], q[3]]);
    }
}

/// Number pixels as (x, y) lower-left corners relative to the number's centre.
fn number_pixels(number: u32) -> Vec<[f64; 2]> {
    let digits: Vec<usize> = number
        .to_string()
        .bytes()
        .map(|b| usize::from(b - b'0'))
        .collect();
    let width = (digits.len() * 4 - 1) as f64 * PIXEL;
    let mut out = Vec::new();
    for (d, &digit) in digits.iter().enumerate() {
        for (row, bits) in FONT[digit].iter().enumerate() {
            for col in 0..3 {
                if bits & (4 >> col) != 0 {
                    out.push([
                        -width / 2.0 + (d * 4 + col) as f64 * PIXEL,
                        2.5 * PIXEL - (row + 1) as f64 * PIXEL,
                    ]);
                }
            }
        }
    }
    out
}

/// Body triangles: x, y in [-side/2, side/2], z in [0, thickness], features above.
#[must_use]
pub fn body(side: f64, thickness: f64, number: Option<u32>) -> Vec<Tri> {
    let h = side / 2.0;
    let mut out = Vec::new();
    cuboid(&mut out, [-h, -h, 0.0], [h, h, thickness]);
    if let Some(number) = number.filter(|_| side >= MIN_FEATURE_SIDE) {
        // The tab is at the front-left corner; the number reads along +x on the top.
        cuboid(
            &mut out,
            [-h, -h, thickness],
            [-h + TAB, -h + TAB, thickness + RAISE],
        );
        for [x, y] in number_pixels(number) {
            cuboid(
                &mut out,
                [x, y, thickness],
                [x + PIXEL, y + PIXEL, thickness + RAISE],
            );
        }
    }
    out
}

/// Weld equal corners so the mesh is an indexed surface.
///
/// # Errors
/// Mesh construction refused by the geometry crate.
pub fn mesh(tris: &[Tri]) -> crate::Result<Mesh> {
    let mut index = BTreeMap::new();
    let (mut xs, mut ys, mut zs, mut tri) = (vec![], vec![], vec![], vec![]);
    for v in tris.iter().flatten() {
        let key = v.map(|c| (c * 1e6).round() as i64);
        let next = u32::try_from(xs.len())?;
        let at = *index.entry(key).or_insert_with(|| {
            xs.push(v[0]);
            ys.push(v[1]);
            zs.push(v[2]);
            next
        });
        tri.push(at);
    }
    Ok(Mesh::new(xs, ys, zs, tri).map_err(|e| format!("body mesh: {e:?}"))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(tris: &[Tri]) -> f64 {
        tris.iter()
            .map(|t| {
                let [a, b, c] = *t;
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0
            })
            .sum()
    }

    #[test]
    fn slab_is_outward_wound_with_exact_volume() {
        let t = body(8.0, 3.0, None);
        assert_eq!(t.len(), 12);
        assert!((volume(&t) - 8.0 * 8.0 * 3.0).abs() < 1e-9);
    }

    #[test]
    fn features_sit_above_the_slab_and_inside_its_footprint() {
        let t = body(8.0, 3.0, Some(47));
        for v in t.iter().flatten() {
            assert!(v[0].abs() <= 4.0 + 1e-9 && v[1].abs() <= 4.0 + 1e-9);
        }
        // Only the 12 slab triangles reach below the top face.
        let below = t
            .iter()
            .filter(|tri| tri.iter().any(|v| v[2] < 3.0 - 1e-9))
            .count();
        assert_eq!(below, 12 - 2);
        assert!(t.iter().flatten().all(|v| v[2] <= 3.0 + RAISE + 1e-9));
        // "4" lights 9 pixels and "7" lights 7.
        let digits = number_pixels(47).len();
        assert_eq!(digits, 9 + 7);
        let raised = volume(&t) - 8.0 * 8.0 * 3.0;
        let want = (digits as f64 * PIXEL * PIXEL + TAB * TAB) * RAISE;
        assert!((raised - want).abs() < 1e-9);
    }

    #[test]
    fn tiny_bodies_carry_no_features_and_digits_are_asymmetric() {
        assert_eq!(body(1.5, 3.0, Some(3)).len(), 12);
        // 2 mirrored left-right is not 5: the font is not symmetric.
        assert_ne!(number_pixels(2), number_pixels(5));
        assert_ne!(number_pixels(1).len(), number_pixels(8).len());
    }

    #[test]
    fn weld_keeps_every_triangle() {
        let t = body(8.0, 3.0, Some(5));
        let m = mesh(&t).unwrap();
        assert_eq!(m.triangle_count(), t.len());
    }
}
