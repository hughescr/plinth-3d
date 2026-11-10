//! CAL-2 coupon bodies: stacks of flat-bottomed, flat-topped parts (rectangles and one
//! annulus), each part a whole number of layers tall so every layer is one constant
//! footprint. The loaded layer is the body's first layer. An asymmetric tab and the coupon
//! number are raised on the top face, above every loaded layer, so they add nothing to the
//! area that peels in a body layer.
use plinth_geom::Mesh;
use std::collections::BTreeMap;

pub type Tri = [[f64; 3]; 3];

/// Raised feature height above the top face, mm.
pub const RAISE: f64 = 0.4;
/// Pixel sizes of the raised number font, mm, largest first. A number takes the largest that
/// fits its face. The smallest is the least that stays legible after washing and curing: a
/// digit 1.5 mm tall with strokes 0.3 mm wide and 0.4 mm high.
pub const PIXELS: [f64; 5] = [0.5, 0.45, 0.4, 0.35, 0.3];
/// Corner tab, mm square.
pub const TAB: f64 = 1.2;
/// Clear border between a number and the edge of its face, mm.
pub const EDGE: f64 = 0.2;
/// Facets of a ring's circles.
pub const RING_FACETS: usize = 96;

/// The late-load body: an `n` x `n` grid of cells, each with a pad that grows layer by
/// layer (one stepped solid per head, no internal faces), then one slab over all of them.
/// The pads are separate islands until the slab. `z[j]` is the bottom of pad layer `j`,
/// `z[steps.len()]` the top of the last pad and the bottom of the slab.
#[derive(Debug, Clone, PartialEq)]
pub struct Pads {
    pub n: usize,
    /// Lower corner of the whole footprint, and one cell's width and height.
    pub origin: [f64; 2],
    pub cell: [f64; 2],
    /// Width and height of the pad in each layer, centred in its cell.
    pub steps: Vec<[f64; 2]>,
    pub z: Vec<f64>,
}

/// The horizontal footprint of one part.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Axis-aligned rectangle, lower and upper corner.
    Rect { lo: [f64; 2], hi: [f64; 2] },
    /// Annulus about the origin: `RING_FACETS`-gon outer and inner circles.
    Ring { outer: f64, inner: f64 },
    /// Pads under a slab (the part's `z1` is the slab's top).
    Pads(Pads),
}

/// One prism of the body, between heights `z0` and `z1`.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub shape: Shape,
    pub z0: f64,
    pub z1: f64,
}

/// The top face of a body: where the number is raised.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Face {
    /// Rectangle, lower and upper corner, at height `z`.
    Rect { lo: [f64; 2], hi: [f64; 2], z: f64 },
    /// Annulus about the origin (the ring's top surface), at height `z`.
    Ring { outer: f64, inner: f64, z: f64 },
}

/// The raised number on a top face, and the orientation tab where the face has room for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Features {
    /// Centre of the number, mm; it reads along +x.
    pub centre: [f64; 2],
    /// Pixel of the number's font, mm.
    pub pixel: f64,
    /// Lower corner of the corner tab (front-left of the face), if it has one.
    pub tab: Option<[f64; 2]>,
    /// Height of the top face.
    pub z: f64,
    pub number: u32,
}

/// Width and height of the number's box, mm.
#[must_use]
pub fn number_box(number: u32, pixel: f64) -> [f64; 2] {
    let digits = number.to_string().len();
    [(digits * 4 - 1) as f64 * pixel, 5.0 * pixel]
}

impl Features {
    /// Whether the orientation tab fits a `w` by `h` top face: a square of 5 mm or more, or a
    /// strip at least 3.5 mm wide and four times as long as it is wide.
    #[must_use]
    pub fn tab_fits(w: f64, h: f64) -> bool {
        let (short, long) = (w.min(h), w.max(h));
        short >= 5.0 || (short >= 3.5 && long >= 4.0 * short)
    }

    /// The number, as large as the face allows, or none if even the smallest legible pixel
    /// does not fit.
    #[must_use]
    pub fn on(face: Face, number: u32) -> Option<Self> {
        match face {
            Face::Rect { lo, hi, z } => {
                let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
                let pixel = PIXELS.into_iter().find(|&p| {
                    let b = number_box(number, p);
                    b[0] + 2.0 * EDGE <= w && b[1] + 2.0 * EDGE <= h
                })?;
                Some(Self {
                    centre: [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0],
                    pixel,
                    tab: Self::tab_fits(w, h).then_some(lo),
                    z,
                    number,
                })
            }
            Face::Ring { outer, inner, z } => {
                // At the front of the ring (-y), on its centre line, reading along +x. Every
                // corner of the number's box is inside the outer circle and the box is
                // outside the inner circle, each with the border.
                let mid = (outer + inner) / 2.0;
                let pixel = PIXELS.into_iter().find(|&p| {
                    let [w, h] = number_box(number, p);
                    let corner = (w / 2.0).hypot(mid + h / 2.0);
                    corner + EDGE <= outer && mid - h / 2.0 >= inner + EDGE
                })?;
                Some(Self {
                    centre: [0.0, -mid],
                    pixel,
                    tab: None,
                    z,
                    number,
                })
            }
        }
    }

    /// Number of raised boxes: one per lit pixel and one for the tab.
    #[must_use]
    pub fn boxes(&self) -> usize {
        number_pixels(self.number, self.pixel).len() + usize::from(self.tab.is_some())
    }
}

/// Area of a regular `RING_FACETS`-gon of circumradius `r`.
#[must_use]
pub fn polygon_area(r: f64) -> f64 {
    let n = RING_FACETS as f64;
    0.5 * n * r * r * (std::f64::consts::TAU / n).sin()
}

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
fn number_pixels(number: u32, pixel: f64) -> Vec<[f64; 2]> {
    let digits: Vec<usize> = number
        .to_string()
        .bytes()
        .map(|b| usize::from(b - b'0'))
        .collect();
    let width = (digits.len() * 4 - 1) as f64 * pixel;
    let mut out = Vec::new();
    for (d, &digit) in digits.iter().enumerate() {
        for (row, bits) in FONT[digit].iter().enumerate() {
            for col in 0..3 {
                if bits & (4 >> col) != 0 {
                    out.push([
                        -width / 2.0 + (d * 4 + col) as f64 * pixel,
                        2.5 * pixel - (row + 1) as f64 * pixel,
                    ]);
                }
            }
        }
    }
    out
}

/// Outward-wound triangles of an annular prism about the z axis.
fn ring_prism(out: &mut Vec<Tri>, outer: f64, inner: f64, z0: f64, z1: f64) {
    let at = |r: f64, i: usize, z: f64| {
        let a = std::f64::consts::TAU * (i % RING_FACETS) as f64 / RING_FACETS as f64;
        [r * a.cos(), r * a.sin(), z]
    };
    for i in 0..RING_FACETS {
        let quads = [
            // top and bottom annuli
            [
                at(outer, i, z1),
                at(outer, i + 1, z1),
                at(inner, i + 1, z1),
                at(inner, i, z1),
            ],
            [
                at(outer, i, z0),
                at(inner, i, z0),
                at(inner, i + 1, z0),
                at(outer, i + 1, z0),
            ],
            // outer wall
            [
                at(outer, i, z0),
                at(outer, i + 1, z0),
                at(outer, i + 1, z1),
                at(outer, i, z1),
            ],
            // inner wall (normal toward the axis)
            [
                at(inner, i + 1, z0),
                at(inner, i, z0),
                at(inner, i, z1),
                at(inner, i + 1, z1),
            ],
        ];
        for q in quads {
            out.push([q[0], q[1], q[2]]);
            out.push([q[0], q[2], q[3]]);
        }
    }
}

type Rect = [[f64; 2]; 2];

fn quad(out: &mut Vec<Tri>, q: [[f64; 3]; 4]) {
    out.push([q[0], q[1], q[2]]);
    out.push([q[0], q[2], q[3]]);
}

/// Rectangle `r` (lower, upper corner) at height `z`, facing up or down.
fn flat(out: &mut Vec<Tri>, r: Rect, z: f64, up: bool) {
    let p = |x: usize, y: usize| [r[x][0], r[y][1], z];
    let q = [p(0, 0), p(1, 0), p(1, 1), p(0, 1)];
    quad(out, if up { q } else { [q[3], q[2], q[1], q[0]] });
}

/// Down-facing frame at `z` between the inner rectangle and the outer one, as four
/// trapezoids that share their slanted edges and the inner rectangle's edges exactly.
fn frame_down(out: &mut Vec<Tri>, inner: Rect, outer: Rect, z: f64) {
    let at = |x: f64, y: f64| [x, y, z];
    let [[ix0, iy0], [ix1, iy1]] = inner;
    let [[ox0, oy0], [ox1, oy1]] = outer;
    let up = [
        [at(ox0, oy0), at(ox1, oy0), at(ix1, iy0), at(ix0, iy0)],
        [at(ox1, oy0), at(ox1, oy1), at(ix1, iy1), at(ix1, iy0)],
        [at(ox1, oy1), at(ox0, oy1), at(ix0, iy1), at(ix1, iy1)],
        [at(ox0, oy1), at(ox0, oy0), at(ix0, iy0), at(ix0, iy1)],
    ];
    for q in up {
        quad(out, [q[3], q[2], q[1], q[0]]);
    }
}

/// The four outward walls of rectangle `r` between `za` and `zb`; `sides` selects
/// (x low, x high, y low, y high).
fn walls(out: &mut Vec<Tri>, r: Rect, za: f64, zb: f64, sides: [bool; 4]) {
    let p = |x: usize, y: usize, z: f64| [r[x][0], r[y][1], z];
    if sides[2] {
        quad(out, [p(0, 0, za), p(1, 0, za), p(1, 0, zb), p(0, 0, zb)]);
    }
    if sides[3] {
        quad(out, [p(1, 1, za), p(0, 1, za), p(0, 1, zb), p(1, 1, zb)]);
    }
    if sides[0] {
        quad(out, [p(0, 1, za), p(0, 0, za), p(0, 0, zb), p(0, 1, zb)]);
    }
    if sides[1] {
        quad(out, [p(1, 0, za), p(1, 1, za), p(1, 1, zb), p(1, 0, zb)]);
    }
}

/// A closed surface for the pads and their slab: per cell, one stepped pad, the slab's
/// bottom frame around its last step, the slab's top, and its outer walls on the
/// perimeter. Every edge is shared by exactly two triangles.
fn pads(out: &mut Vec<Tri>, p: &Pads, top: f64) {
    let layers = p.steps.len();
    for i in 0..p.n * p.n {
        let (ci, cj) = (i % p.n, i / p.n);
        let lo = [
            p.origin[0] + ci as f64 * p.cell[0],
            p.origin[1] + cj as f64 * p.cell[1],
        ];
        let c = [lo[0] + p.cell[0] / 2.0, lo[1] + p.cell[1] / 2.0];
        let cell: Rect = [lo, [lo[0] + p.cell[0], lo[1] + p.cell[1]]];
        let pad = |j: usize| -> Rect {
            let h = [p.steps[j][0] / 2.0, p.steps[j][1] / 2.0];
            [[c[0] - h[0], c[1] - h[1]], [c[0] + h[0], c[1] + h[1]]]
        };
        flat(out, pad(0), p.z[0], false);
        for j in 0..layers {
            walls(out, pad(j), p.z[j], p.z[j + 1], [true; 4]);
            if j > 0 {
                frame_down(out, pad(j - 1), pad(j), p.z[j]);
            }
        }
        frame_down(out, pad(layers - 1), cell, p.z[layers]);
        flat(out, cell, top, true);
        let edge = [ci == 0, ci + 1 == p.n, cj == 0, cj + 1 == p.n];
        walls(out, cell, p.z[layers], top, edge);
    }
}

/// Body triangles: every part, then the tab and number raised above `features.z`.
#[must_use]
pub fn body(parts: &[Part], features: Option<&Features>) -> Vec<Tri> {
    let mut out = Vec::new();
    for p in parts {
        match &p.shape {
            Shape::Rect { lo, hi } => cuboid(&mut out, [lo[0], lo[1], p.z0], [hi[0], hi[1], p.z1]),
            Shape::Ring { outer, inner } => ring_prism(&mut out, *outer, *inner, p.z0, p.z1),
            Shape::Pads(pads_shape) => pads(&mut out, pads_shape, p.z1),
        }
    }
    if let Some(f) = features {
        // The tab is at the front-left corner; the number reads along +x on the top.
        if let Some(lo) = f.tab {
            cuboid(
                &mut out,
                [lo[0], lo[1], f.z],
                [lo[0] + TAB, lo[1] + TAB, f.z + RAISE],
            );
        }
        let (c, p) = (f.centre, f.pixel);
        for [x, y] in number_pixels(f.number, p) {
            cuboid(
                &mut out,
                [c[0] + x, c[1] + y, f.z],
                [c[0] + x + p, c[1] + y + p, f.z + RAISE],
            );
        }
    }
    out
}

/// Raised boxes in a written body mesh that carries raised features: the flat faces at its
/// highest point, two triangles each. The body's own top is 0.4 mm lower.
#[must_use]
pub fn raised_boxes(mesh: &[plinth_chitubox::Triangle]) -> usize {
    let top = mesh
        .iter()
        .flatten()
        .map(|p| p[2])
        .fold(f32::NEG_INFINITY, f32::max);
    mesh.iter()
        .filter(|t| t.iter().all(|p| p[2] >= top - 1e-4))
        .count()
        / 2
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

    fn slab(side: f64, z1: f64) -> Part {
        Part {
            shape: Shape::Rect {
                lo: [-side / 2.0; 2],
                hi: [side / 2.0; 2],
            },
            z0: 0.0,
            z1,
        }
    }
    fn features(side: f64, z: f64, number: u32) -> Features {
        Features::on(
            Face::Rect {
                lo: [-side / 2.0; 2],
                hi: [side / 2.0; 2],
                z,
            },
            number,
        )
        .unwrap()
    }

    #[test]
    fn slab_is_outward_wound_with_exact_volume() {
        let t = body(&[slab(8.0, 2.0)], None);
        assert_eq!(t.len(), 12);
        assert!((volume(&t) - 8.0 * 8.0 * 2.0).abs() < 1e-9);
    }

    #[test]
    fn features_sit_above_the_slab_and_inside_its_footprint() {
        let t = body(&[slab(8.0, 2.0)], Some(&features(8.0, 2.0, 47)));
        for v in t.iter().flatten() {
            assert!(v[0].abs() <= 4.0 + 1e-9 && v[1].abs() <= 4.0 + 1e-9);
        }
        // Only the 12 slab triangles reach below the top face.
        let below = t
            .iter()
            .filter(|tri| tri.iter().any(|v| v[2] < 2.0 - 1e-9))
            .count();
        assert_eq!(below, 12 - 2);
        assert!(t.iter().flatten().all(|v| v[2] <= 2.0 + RAISE + 1e-9));
        // "4" lights 9 pixels and "7" lights 7.
        let f = features(8.0, 2.0, 47);
        assert!((f.pixel - 0.5).abs() < 1e-12);
        let digits = number_pixels(47, f.pixel).len();
        assert_eq!(digits, 9 + 7);
        assert_eq!(f.boxes(), digits + 1);
        let raised = volume(&t) - 8.0 * 8.0 * 2.0;
        let want = (digits as f64 * f.pixel * f.pixel + TAB * TAB) * RAISE;
        assert!((raised - want).abs() < 1e-9);
    }

    #[test]
    fn a_small_face_takes_a_smaller_legible_number_or_none() {
        let rect = |w: f64, h: f64| Face::Rect {
            lo: [0.0; 2],
            hi: [w, h],
            z: 1.0,
        };
        // Faces from the matrix: 5.79 mm square control, 3.66 mm strip, 1.62 mm witness.
        assert!((Features::on(rect(5.79, 5.79), 12).unwrap().pixel - 0.5).abs() < 1e-12);
        assert!((Features::on(rect(18.3, 3.66), 12).unwrap().pixel - 0.5).abs() < 1e-12);
        // 3.0 mm wide: the largest pixel whose 2-digit box (7 x 5 pixels) and border fit.
        let small = Features::on(rect(3.0, 3.0), 12).unwrap();
        assert!(small.pixel < 0.5 && small.pixel >= 0.3 && small.tab.is_none());
        assert!(Features::on(rect(2.2, 2.2), 12).is_none());
        for w in [0.82, 1.12, 1.62] {
            assert!(Features::on(rect(w, w), 65).is_none(), "{w}");
        }
        // The largest pixel is chosen: one step larger does not fit.
        let b = number_box(12, small.pixel);
        assert!(b[0] + 2.0 * EDGE <= 3.0);
        let bigger = PIXELS.iter().copied().rfind(|&p| p > small.pixel);
        if let Some(p) = bigger {
            assert!(number_box(12, p)[0] + 2.0 * EDGE > 3.0);
        }
    }

    #[test]
    fn a_ring_carries_its_number_on_the_top_surface_inside_both_circles() {
        for (outer, inner) in [(6.85, 3.85), (8.19, 5.19)] {
            let f = Features::on(
                Face::Ring {
                    outer,
                    inner,
                    z: 2.0,
                },
                23,
            )
            .unwrap();
            assert!(f.pixel >= 0.3 && f.tab.is_none());
            let t = body(
                &[Part {
                    shape: Shape::Ring { outer, inner },
                    z0: 0.0,
                    z1: 2.0,
                }],
                Some(&f),
            );
            // Every raised vertex is over the annulus, with the border to spare.
            let raised: Vec<_> = t
                .iter()
                .flatten()
                .filter(|v| v[2] > 2.0 + 1e-9)
                .map(|v| v[0].hypot(v[1]))
                .collect();
            assert!(!raised.is_empty());
            assert!(raised.iter().all(|&r| r >= inner + 0.1 && r <= outer - 0.1));
            assert_eq!(
                (t.len() - RING_FACETS * 8) / 12,
                f.boxes(),
                "one box per lit pixel"
            );
        }
        // A ring too narrow for the smallest legible digits carries none.
        assert!(
            Features::on(
                Face::Ring {
                    outer: 5.0,
                    inner: 3.4,
                    z: 2.0
                },
                23
            )
            .is_none()
        );
    }

    #[test]
    fn raised_boxes_counts_the_written_tops() {
        let f = features(8.0, 2.0, 47);
        let t = body(&[slab(8.0, 2.0)], Some(&f));
        let written: Vec<plinth_chitubox::Triangle> = t
            .iter()
            .map(|tri| tri.map(|p| p.map(|c| c as f32)))
            .collect();
        assert_eq!(raised_boxes(&written), f.boxes());
    }

    #[test]
    fn ring_is_outward_wound_with_the_polygon_volume() {
        let t = body(
            &[Part {
                shape: Shape::Ring {
                    outer: 7.0,
                    inner: 4.0,
                },
                z0: 0.0,
                z1: 2.0,
            }],
            None,
        );
        assert_eq!(t.len(), RING_FACETS * 8);
        let want = (polygon_area(7.0) - polygon_area(4.0)) * 2.0;
        assert!((volume(&t) - want).abs() < 1e-9, "{} {want}", volume(&t));
        // Welded, every edge is shared by exactly two triangles (a closed surface).
        let m = mesh(&t).unwrap();
        assert_eq!(m.triangle_count(), t.len());
        let mut edges = std::collections::BTreeMap::new();
        for tri in m.indices().chunks(3) {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                *edges.entry((a, b)).or_insert(0) += 1;
            }
        }
        assert!(
            edges
                .iter()
                .all(|(&(a, b), &n)| n == 1 && edges.get(&(b, a)) == Some(&1))
        );
    }

    #[test]
    fn pads_under_a_slab_are_one_closed_surface_with_the_exact_volume() {
        let (n, cell, layers) = (3_usize, [3.0, 2.5], 6_usize);
        let steps: Vec<[f64; 2]> = (0..layers)
            .map(|j| {
                let f = (0.4 + 0.6 * j as f64 / layers as f64).sqrt();
                [cell[0] * f, cell[1] * f]
            })
            .collect();
        let z: Vec<f64> = (0..=layers)
            .map(|j| j as f64 * 0.05 + if j == 0 { 0.0 } else { 0.0005 })
            .collect();
        let top = 0.5 - 0.0005;
        let part = Part {
            shape: Shape::Pads(Pads {
                n,
                origin: [-4.5, -3.75],
                cell,
                steps: steps.clone(),
                z: z.clone(),
            }),
            z0: 0.0,
            z1: top,
        };
        let t = body(&[part], None);
        let pads: f64 = (0..layers)
            .map(|j| steps[j][0] * steps[j][1] * (z[j + 1] - z[j]))
            .sum::<f64>()
            * (n * n) as f64;
        let slab = 9.0 * 7.5 * (top - z[layers]);
        assert!((volume(&t) - pads - slab).abs() < 1e-9, "{}", volume(&t));
        // Every directed edge of the welded surface has its reverse exactly once.
        let m = mesh(&t).unwrap();
        let mut edges = std::collections::BTreeMap::new();
        for tri in m.indices().chunks(3) {
            for k in 0..3 {
                *edges.entry((tri[k], tri[(k + 1) % 3])).or_insert(0) += 1;
            }
        }
        assert!(
            edges
                .iter()
                .all(|(&(a, b), &c)| c == 1 && edges.get(&(b, a)) == Some(&1))
        );
        // Nothing but the pad bottoms is at the lowest height.
        let low = t
            .iter()
            .flatten()
            .map(|v| v[2])
            .fold(f64::INFINITY, f64::min);
        assert!(low.abs() < 1e-12);
    }

    #[test]
    fn tab_fit_rule() {
        assert!(Features::tab_fits(5.0, 5.0));
        assert!(!Features::tab_fits(4.2, 4.2));
        assert!(Features::tab_fits(18.5, 3.7));
        assert!(!Features::tab_fits(18.5, 3.0));
    }

    #[test]
    fn digits_are_asymmetric() {
        // 2 mirrored left-right is not 5: the font is not symmetric.
        assert_ne!(number_pixels(2, 0.5), number_pixels(5, 0.5));
        assert_ne!(number_pixels(1, 0.5).len(), number_pixels(8, 0.5).len());
    }

    #[test]
    fn weld_keeps_every_triangle() {
        let t = body(&[slab(8.0, 2.0)], Some(&features(8.0, 2.0, 5)));
        let m = mesh(&t).unwrap();
        assert_eq!(m.triangle_count(), t.len());
    }
}
