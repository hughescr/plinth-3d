//! Run-length rasters of the layer slabs on the printer pixel grid (lane B; design
//! `step11-construction.md` section 4.2).
//!
//! The raster gives connectivity, island sizes and cups. Areas always come from the vector
//! polygons, never from pixel counts.
//!
//! A pixel is filled when its centre lies inside the slab envelope (nonzero winding of the
//! rings). Pixel `(i, j)` covers `[ox + i px, ox + (i + 1) px) x [oy + j py, oy + (j + 1) py)`.
//! The grid origin is a multiple of the pitch, with a margin of at least one empty pixel on
//! every side, so a stack of the same model is the same at any thread count.

use crate::counters::{self, Op};
use crate::measure::Measure;
use crate::slice::{LayerSlab, Rings, par_ranges};

/// The printer pixel pitch in x and y, mm (ChituBox convention, P1).
pub const PIXEL_PITCH_MM: [f64; 2] = [0.014, 0.019];

/// One run of filled pixels in one row: columns `x0..x1` of row `y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    /// Row.
    pub y: u32,
    /// First filled column.
    pub x0: u32,
    /// One past the last filled column.
    pub x1: u32,
}

/// One layer's filled pixels, runs sorted by row then column.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayerRaster {
    /// Layer index.
    pub layer: u32,
    /// The runs.
    pub runs: Vec<Run>,
}

impl LayerRaster {
    /// Number of filled pixels.
    #[must_use]
    pub fn pixels(&self) -> u64 {
        self.runs.iter().map(|r| u64::from(r.x1 - r.x0)).sum()
    }

    /// True when pixel `(x, y)` is filled.
    #[must_use]
    pub fn filled(&self, x: u32, y: u32) -> bool {
        self.run_index(x, y).is_some()
    }

    /// The index of the run that holds pixel `(x, y)`, if it is filled.
    #[must_use]
    pub fn run_index(&self, x: u32, y: u32) -> Option<usize> {
        let at = self.runs.partition_point(|r| (r.y, r.x1) <= (y, x));
        self.runs
            .get(at)
            .is_some_and(|r| r.y == y && r.x0 <= x && x < r.x1)
            .then_some(at)
    }
}

/// All layers on one pixel grid.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterStack {
    /// Pixel pitch in x and y, mm.
    pub pitch: [f64; 2],
    /// World xy of pixel (0, 0)'s lower corner, mm.
    pub origin: [f64; 2],
    /// Grid width and height in pixels.
    pub size: [u32; 2],
    /// One raster per layer, in layer order.
    pub layers: Vec<LayerRaster>,
}

impl RasterStack {
    /// World xy of the centre of pixel `(x, y)`, mm.
    #[must_use]
    pub fn centre(&self, x: f64, y: f64) -> [f64; 2] {
        [
            self.origin[0] + (x + 0.5) * self.pitch[0],
            self.origin[1] + (y + 0.5) * self.pitch[1],
        ]
    }

    /// The pixel that holds world point `(x, y)`, or `None` outside the grid.
    #[must_use]
    pub fn pixel_of(&self, x: f64, y: f64) -> Option<[u32; 2]> {
        let fx = ((x - self.origin[0]) / self.pitch[0]).floor();
        let fy = ((y - self.origin[1]) / self.pitch[1]).floor();
        if fx >= 0.0 && fy >= 0.0 && fx < f64::from(self.size[0]) && fy < f64::from(self.size[1]) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Some([fx as u32, fy as u32])
        } else {
            None
        }
    }
}

/// The grid that holds every envelope with a margin, or `None` when the slabs are empty.
fn grid_of(slabs: &[LayerSlab], pitch: [f64; 2]) -> Option<([f64; 2], [u32; 2])> {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in slabs.iter().flat_map(|s| &s.envelope.rings).flatten() {
        for c in 0..2 {
            lo[c] = lo[c].min(p[c]);
            hi[c] = hi[c].max(p[c]);
        }
    }
    if !(lo[0].is_finite() && hi[0].is_finite() && lo[1].is_finite() && hi[1].is_finite()) {
        return None;
    }
    let origin = [
        (lo[0] / pitch[0]).floor() * pitch[0] - pitch[0],
        (lo[1] / pitch[1]).floor() * pitch[1] - pitch[1],
    ];
    let cols = ((hi[0] - origin[0]) / pitch[0]).ceil() + 2.0;
    let rows = ((hi[1] - origin[1]) / pitch[1]).ceil() + 2.0;
    if cols >= f64::from(u32::MAX) || rows >= f64::from(u32::MAX) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some((origin, [cols as u32, rows as u32]))
}

/// Pixel centre index range `[a, b)` whose centres `(i + 0.5) p + o` lie in `[lo, hi)`.
fn centre_range(lo: f64, hi: f64, o: f64, p: f64, limit: u32) -> (u32, u32) {
    let cap = f64::from(limit);
    let a = ((lo - o) / p - 0.5).ceil().clamp(0.0, cap);
    let b = ((hi - o) / p - 0.5).ceil().clamp(0.0, cap);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (a as u32, b as u32)
}

/// The filled runs of one layer.
fn raster_layer(rings: &Rings, origin: [f64; 2], pitch: [f64; 2], size: [u32; 2]) -> Vec<Run> {
    // One crossing per (ring edge, pixel row whose centre line it spans): row, x, winding.
    let mut cross: Vec<(u32, f64, i8)> = Vec::new();
    for ring in &rings.rings {
        let n = ring.len();
        for i in 0..n {
            let (p, q) = (ring[i], ring[(i + 1) % n]);
            if p[1].total_cmp(&q[1]).is_eq() {
                continue;
            }
            let (lo, hi, d) = if p[1] < q[1] {
                (p, q, -1_i8)
            } else {
                (q, p, 1_i8)
            };
            let (r0, r1) = centre_range(lo[1], hi[1], origin[1], pitch[1], size[1]);
            for row in r0..r1 {
                let y = origin[1] + (f64::from(row) + 0.5) * pitch[1];
                let x = lo[0] + (hi[0] - lo[0]) * ((y - lo[1]) / (hi[1] - lo[1]));
                cross.push((row, x, d));
            }
        }
    }
    cross.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut runs: Vec<Run> = Vec::new();
    let mut i = 0;
    while i < cross.len() {
        let row = cross[i].0;
        let mut w = 0_i32;
        let mut start = 0.0;
        while i < cross.len() && cross[i].0 == row {
            let before = w;
            w += i32::from(cross[i].2);
            if before <= 0 && w > 0 {
                start = cross[i].1;
            } else if before > 0 && w <= 0 {
                let (a, b) = centre_range(start, cross[i].1, origin[0], pitch[0], size[0]);
                if a < b {
                    match runs.last_mut() {
                        Some(last) if last.y == row && last.x1 == a => last.x1 = b,
                        _ => runs.push(Run {
                            y: row,
                            x0: a,
                            x1: b,
                        }),
                    }
                }
            }
            i += 1;
        }
    }
    runs
}

/// Rasterise every slab envelope on one grid with the given pixel pitch.
#[must_use]
pub fn rasterise(slabs: &[LayerSlab], pitch: [f64; 2]) -> Measure<RasterStack> {
    rasterise_par(slabs, pitch, 1)
}

/// Like [`rasterise`] on up to `threads` threads in contiguous layer ranges. The result is the
/// same for every `threads`; counted once on the calling thread, one item per layer; the
/// tasks and threads are left for [`counters::take_par`].
#[must_use]
pub fn rasterise_par(slabs: &[LayerSlab], pitch: [f64; 2], threads: usize) -> Measure<RasterStack> {
    if !(pitch[0].is_finite() && pitch[1].is_finite() && pitch[0] > 0.0 && pitch[1] > 0.0) {
        return Measure::Unverified("rasterise: the pixel pitch is not valid");
    }
    counters::record(Op::Rasterise, slabs.len());
    let Some((origin, size)) = grid_of(slabs, pitch) else {
        counters::record_par(counters::ParReport {
            tasks: 0,
            threads: 0,
        });
        return Measure::Known(RasterStack {
            pitch,
            origin: [0.0, 0.0],
            size: [0, 0],
            layers: slabs
                .iter()
                .map(|s| LayerRaster {
                    layer: s.layer,
                    runs: Vec::new(),
                })
                .collect(),
        });
    };
    let (layers, report) = par_ranges(slabs.len(), threads, &|a, b| {
        slabs[a..b]
            .iter()
            .map(|s| LayerRaster {
                layer: s.layer,
                runs: raster_layer(&s.envelope, origin, pitch, size),
            })
            .collect()
    });
    counters::record_par(report);
    Measure::Known(RasterStack {
        pitch,
        origin,
        size,
        layers,
    })
}

/// Scalar reference: test every pixel centre against every ring edge.
pub mod reference {
    use super::{LayerRaster, RasterStack};
    use crate::slice::Rings;

    /// The nonzero winding number of `rings` at point `(x, y)` by a ray to +x.
    #[must_use]
    pub fn winding(rings: &Rings, x: f64, y: f64) -> i32 {
        let mut w = 0;
        for ring in &rings.rings {
            let n = ring.len();
            for i in 0..n {
                let (p, q) = (ring[i], ring[(i + 1) % n]);
                let up = p[1] <= y && q[1] > y;
                let down = q[1] <= y && p[1] > y;
                if !(up || down) {
                    continue;
                }
                let xc = p[0] + (q[0] - p[0]) * ((y - p[1]) / (q[1] - p[1]));
                if xc > x {
                    // An edge to the right of the point: upward adds, downward subtracts.
                    w += if up { 1 } else { -1 };
                }
            }
        }
        w
    }

    /// Fill a dense `size` bitmap (row-major) from `rings` pixel by pixel.
    #[must_use]
    pub fn dense(rings: &Rings, stack: &RasterStack) -> Vec<bool> {
        let [w, h] = stack.size;
        let mut out = vec![false; (w as usize) * (h as usize)];
        for j in 0..h {
            for i in 0..w {
                let c = stack.centre(f64::from(i), f64::from(j));
                out[(j as usize) * (w as usize) + i as usize] = winding(rings, c[0], c[1]) > 0;
            }
        }
        out
    }

    /// The runs of a dense bitmap, for comparing against [`LayerRaster`].
    #[must_use]
    pub fn runs_of(dense: &[bool], size: [u32; 2], layer: u32) -> LayerRaster {
        let mut runs = Vec::new();
        for j in 0..size[1] {
            let mut i = 0;
            while i < size[0] {
                if dense[(j as usize) * (size[0] as usize) + i as usize] {
                    let a = i;
                    while i < size[0] && dense[(j as usize) * (size[0] as usize) + i as usize] {
                        i += 1;
                    }
                    runs.push(super::Run { y: j, x0: a, x1: i });
                } else {
                    i += 1;
                }
            }
        }
        LayerRaster { layer, runs }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slice::LayerSlab;
    use crate::testutil::slot_threads;

    fn slab_of(rings: Vec<Vec<[f64; 2]>>) -> LayerSlab {
        let mut r = Rings::default();
        for ring in rings {
            let n = ring.len();
            let a: f64 = (0..n)
                .map(|i| {
                    let (p, q) = (ring[i], ring[(i + 1) % n]);
                    0.5 * (p[0] * q[1] - q[0] * p[1])
                })
                .sum();
            r.area += a;
            r.rings.push(ring);
        }
        LayerSlab {
            layer: 0,
            envelope: r,
            midplane: Rings::default(),
        }
    }

    fn square(x: f64, y: f64, s: f64) -> Vec<[f64; 2]> {
        vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s]]
    }

    fn triangle() -> Vec<[f64; 2]> {
        vec![[0.13, 0.11], [1.37, 0.4], [0.5, 1.21]]
    }

    fn stack_of(slabs: &[LayerSlab]) -> RasterStack {
        match rasterise(slabs, PIXEL_PITCH_MM) {
            Measure::Known(s) => s,
            Measure::Unverified(w) => panic!("{w}"),
        }
    }

    #[test]
    fn a_square_fills_the_expected_pixels_and_leaves_a_margin() {
        let s = slab_of(vec![square(1.0, 2.0, 0.28)]);
        let st = stack_of(&[s]);
        let l = &st.layers[0];
        // 0.28 / 0.014 = 20 columns, 0.28 / 0.019 = 14.7 rows (14 or 15).
        assert!((14..=15).contains(&l.runs.len()), "{}", l.runs.len());
        assert!(l.runs.iter().all(|r| r.x1 - r.x0 == 20));
        assert!(l.runs.iter().all(|r| r.x0 >= 1 && r.y >= 1));
        assert!(
            l.runs
                .iter()
                .all(|r| r.x1 + 1 < st.size[0] && r.y + 1 < st.size[1])
        );
        assert!(l.filled(l.runs[0].x0, l.runs[0].y));
        assert!(!l.filled(l.runs[0].x1, l.runs[0].y));
    }

    #[test]
    fn a_hole_ring_leaves_unfilled_pixels() {
        let outer = square(0.0, 0.0, 0.6);
        let mut hole = square(0.2, 0.2, 0.2);
        hole.reverse();
        let st = stack_of(&[slab_of(vec![outer, hole])]);
        let filled = st.layers[0].pixels();
        let full = stack_of(&[slab_of(vec![square(0.0, 0.0, 0.6)])]).layers[0].pixels();
        assert!(filled < full);
        let c = st.pixel_of(0.3, 0.3).unwrap();
        assert!(!st.layers[0].filled(c[0], c[1]));
        let c = st.pixel_of(0.1, 0.1).unwrap();
        assert!(st.layers[0].filled(c[0], c[1]));
    }

    #[test]
    fn the_run_raster_equals_the_pixel_by_pixel_reference() {
        let slabs = [
            slab_of(vec![triangle()]),
            slab_of(vec![square(0.2, 0.2, 0.9), {
                let mut h = square(0.5, 0.5, 0.3);
                h.reverse();
                h
            }]),
        ];
        let st = stack_of(&slabs);
        for (k, s) in slabs.iter().enumerate() {
            let want = reference::runs_of(
                &reference::dense(&s.envelope, &st),
                st.size,
                st.layers[k].layer,
            );
            assert_eq!(st.layers[k], want, "layer {k}");
        }
    }

    #[test]
    fn threads_raster_is_the_same_at_any_thread_count() {
        let slabs: Vec<LayerSlab> = (0..6)
            .map(|k| {
                let mut s = slab_of(vec![square(0.1 * f64::from(k), 0.0, 0.5)]);
                s.layer = k;
                s
            })
            .collect();
        let one = rasterise_par(&slabs, PIXEL_PITCH_MM, 1);
        let limit = slot_threads();
        let _ = counters::take_par();
        let many = rasterise_par(&slabs, PIXEL_PITCH_MM, limit);
        let r = counters::take_par().expect("report");
        assert!(r.tasks <= limit && r.threads <= limit, "{r:?}");
        assert_eq!(one, many);
    }

    #[test]
    fn empty_slabs_and_a_bad_pitch_are_handled() {
        let st = stack_of(&[LayerSlab::default()]);
        assert_eq!(st.size, [0, 0]);
        assert!(st.layers[0].runs.is_empty());
        assert!(rasterise(&[], [0.0, 0.019]).is_unverified());
        counters::reset();
        let _ = stack_of(&[LayerSlab::default(), LayerSlab::default()]);
        let c = counters::snapshot(Op::Rasterise);
        assert_eq!((c.calls, c.items), (1, 2));
    }
}
