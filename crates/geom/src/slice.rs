//! One slicing pass over every layer (lane B; design `step11-construction.md` sections 4.1
//! and 5.9).
//!
//! Layer k covers `[origin + k h, origin + (k + 1) h)` with the plate at z = 0. The result
//! per layer is the exact vector polygon set of the slab envelope (bottom slice, top slice
//! and the projection of every triangle clipped to the slab) and, beside it, the mid-plane
//! slice. Areas are f64 sums over the polygons.
//!
//! How the envelope is built. A vertical line through a point of the solid inside the slab
//! meets the solid in intervals whose ends are either on a slab plane or on the surface
//! inside the slab. So the projection of "solid inside the slab" is the union of the slice
//! just inside the bottom plane, the slice just inside the top plane, and the projection of
//! the surface triangles clipped to the slab. Slices are not linked into loops: their
//! directed segments, together with the counter-clockwise outlines of the projected pieces,
//! go into one winding sweep (see `sweep`), and the filled region is "winding above zero".
//!
//! Ties. A vertex exactly on a slice plane counts as lying on the side that makes the slice
//! the limit from inside the slab (bottom plane: the vertex is below; top plane: the vertex
//! is above). Every closed mesh then gives closed slices. A horizontal triangle exactly on
//! a slab plane adds nothing itself; the slices decide it. A horizontal triangle strictly
//! inside the slab is projected.

mod sweep;

use crate::counters::{self, Op, ParReport};
use crate::measure::Measure;
use crate::mesh::Mesh;
use sweep::Seg;

/// The layer grid: where layer 0 starts, the layer height and the layer count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerGrid {
    /// Layer height, mm (0.05 for the owner's profile).
    pub height: f64,
    /// z of the bottom of layer 0, mm.
    pub origin: f64,
    /// Number of layers.
    pub layers: u32,
}

impl LayerGrid {
    /// The grid whose layer 0 starts at the mesh's lowest point and whose layers reach its
    /// highest point. `None` when the height is not positive, the mesh is empty, or the layer
    /// count does not fit.
    #[must_use]
    pub fn covering(mesh: &Mesh, height: f64) -> Option<Self> {
        let b = mesh.bounds();
        let n = ((b.max[2] - b.min[2]) / height).ceil();
        if mesh.triangle_count() == 0
            || height.is_nan()
            || height <= 0.0
            || !n.is_finite()
            || n >= 1.0e7
        {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(Self {
            height,
            origin: b.min[2],
            layers: (n as u32).max(1),
        })
    }

    /// z of the bottom of layer `k`, mm.
    #[must_use]
    pub fn z_bottom(&self, k: u32) -> f64 {
        self.origin + f64::from(k) * self.height
    }

    /// z of the top of layer `k`, mm (the bottom of layer `k + 1`).
    #[must_use]
    pub fn z_top(&self, k: u32) -> f64 {
        self.origin + f64::from(k + 1) * self.height
    }

    /// The layer whose slab holds height `z`, if any.
    #[must_use]
    pub fn layer_of(&self, z: f64) -> Option<u32> {
        let k = ((z - self.origin) / self.height).floor();
        if k >= 0.0 && k < f64::from(self.layers) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Some(k as u32)
        } else {
            None
        }
    }

    fn valid(&self) -> bool {
        self.height.is_finite() && self.height > 0.0 && self.origin.is_finite()
    }
}

/// A set of closed rings in the XY plane. Outer rings run counter-clockwise, holes clockwise.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rings {
    /// The rings.
    pub rings: Vec<Vec<[f64; 2]>>,
    /// Signed area of the whole set, mm² (holes subtract).
    pub area: f64,
}

/// What one layer holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayerSlab {
    /// Layer index, 0 at the grid origin.
    pub layer: u32,
    /// The slab envelope: the peel area convention.
    pub envelope: Rings,
    /// The mid-plane slice, reported beside it.
    pub midplane: Rings,
}

type V2 = [f64; 2];

/// The mesh triangles bucketed by the layers whose closed slab they may touch (CSR form).
struct Buckets {
    start: Vec<usize>,
    tris: Vec<u32>,
}

impl Buckets {
    fn of(mesh: &Mesh, grid: &LayerGrid) -> Self {
        let n = grid.layers as usize;
        let span = |t: usize| -> Option<(usize, usize)> {
            let idx = &mesh.indices()[3 * t..3 * t + 3];
            let z = [0, 1, 2].map(|c| mesh.zs()[idx[c] as usize]);
            let lo = z[0].min(z[1]).min(z[2]);
            let hi = z[0].max(z[1]).max(z[2]);
            let a = ((lo - grid.origin) / grid.height).floor() - 1.0;
            let b = ((hi - grid.origin) / grid.height).floor() + 1.0;
            if b < 0.0 || a >= n as f64 {
                return None;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Some((a.max(0.0) as usize, (b as usize).min(n - 1)))
        };
        let mut counts = vec![0_usize; n + 1];
        for t in 0..mesh.triangle_count() {
            if let Some((a, b)) = span(t) {
                for c in &mut counts[a + 1..=b + 1] {
                    *c += 1;
                }
            }
        }
        for k in 0..n {
            counts[k + 1] += counts[k];
        }
        let mut fill = counts.clone();
        let mut tris = vec![0_u32; counts[n]];
        for t in 0..mesh.triangle_count() {
            if let Some((a, b)) = span(t) {
                for slot in &mut fill[a..=b] {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        tris[*slot] = t as u32;
                    }
                    *slot += 1;
                }
            }
        }
        Self {
            start: counts,
            tris,
        }
    }

    fn layer(&self, k: usize) -> &[u32] {
        &self.tris[self.start[k]..self.start[k + 1]]
    }
}

/// A triangle's corners and vertex ids.
struct Tri {
    id: [u32; 3],
    p: [[f64; 3]; 3],
}

fn tri_of(mesh: &Mesh, t: u32) -> Tri {
    let base = 3 * t as usize;
    let id = [
        mesh.indices()[base],
        mesh.indices()[base + 1],
        mesh.indices()[base + 2],
    ];
    let p = id.map(|i| {
        let i = i as usize;
        [mesh.xs()[i], mesh.ys()[i], mesh.zs()[i]]
    });
    Tri { id, p }
}

/// The point where edge (`i`, `j`) crosses the plane `zp`. Both triangles that share the edge
/// compute it from the same ordered pair, so the two results are identical bits.
fn edge_point(tri: &Tri, a: usize, b: usize, zp: f64) -> V2 {
    let (lo, hi) = if tri.id[a] < tri.id[b] {
        (a, b)
    } else {
        (b, a)
    };
    let (p, q) = (tri.p[lo], tri.p[hi]);
    let t = (zp - p[2]) / (q[2] - p[2]);
    [p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]
}

/// The directed slice segment of one triangle at plane `zp`, with the interior on the left.
/// `tie_above`: a vertex exactly on the plane counts as above.
fn slice_segment(tri: &Tri, zp: f64, tie_above: bool) -> Option<(V2, V2)> {
    let above = tri
        .p
        .map(|c| if tie_above { c[2] >= zp } else { c[2] > zp });
    let n_above = above.iter().filter(|&&a| a).count();
    if n_above == 0 || n_above == 3 {
        return None;
    }
    // The odd vertex: the only one above (n_above 1) or the only one below (n_above 2).
    let odd_is_above = n_above == 1;
    let o = (0..3).find(|&i| above[i] == odd_is_above)?;
    let (p1, p2) = ((o + 1) % 3, (o + 2) % 3);
    let x = edge_point(tri, o, p1, zp);
    let y = edge_point(tri, p2, o, zp);
    Some(if odd_is_above { (x, y) } else { (y, x) })
}

/// The triangle clipped to `z0 <= z <= z1`, as a polygon of (x, y).
fn clip_to_slab(tri: &Tri, z0: f64, z1: f64) -> Vec<V2> {
    let mut poly: Vec<[f64; 3]> = tri.p.to_vec();
    for (plane, keep_above) in [(z0, true), (z1, false)] {
        let inside = |c: &[f64; 3]| {
            if keep_above {
                c[2] >= plane
            } else {
                c[2] <= plane
            }
        };
        let mut next = Vec::with_capacity(poly.len() + 1);
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let (ia, ib) = (inside(&a), inside(&b));
            if ia {
                next.push(a);
            }
            if ia != ib {
                let t = (plane - a[2]) / (b[2] - a[2]);
                next.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), plane]);
            }
        }
        poly = next;
        if poly.len() < 3 {
            return Vec::new();
        }
    }
    poly.iter().map(|c| [c[0], c[1]]).collect()
}

fn push_polygon(poly: &[V2], out: &mut Vec<Seg>) {
    let n = poly.len();
    if n < 3 {
        return;
    }
    let mut a2 = 0.0;
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        a2 += p[0] * q[1] - q[0] * p[1];
    }
    if a2.abs() < 1e-14 {
        return;
    }
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let s = if a2 > 0.0 {
            Seg::directed(p, q)
        } else {
            Seg::directed(q, p)
        };
        out.extend(s);
    }
}

fn is_horizontal(tri: &Tri) -> bool {
    let z = tri.p.map(|c| c[2]);
    (z[0] - z[1]).abs() < 1e-12 && (z[0] - z[2]).abs() < 1e-12
}

/// The sweep input of one layer: the envelope segments and the mid-plane segments.
fn layer_segments(
    mesh: &Mesh,
    tris: &[u32],
    z0: f64,
    z1: f64,
    envelope: &mut Vec<Seg>,
    mid: &mut Vec<Seg>,
) {
    envelope.clear();
    mid.clear();
    let zm = 0.5 * (z0 + z1);
    for &t in tris {
        let tri = tri_of(mesh, t);
        for (zp, tie_above, to_mid) in [(z0, false, false), (z1, true, false), (zm, false, true)] {
            if let Some((p, q)) = slice_segment(&tri, zp, tie_above) {
                let s = Seg::directed(p, q);
                if to_mid {
                    mid.extend(s);
                } else {
                    envelope.extend(s);
                }
            }
        }
        let zlo = tri.p[0][2].min(tri.p[1][2]).min(tri.p[2][2]);
        let zhi = tri.p[0][2].max(tri.p[1][2]).max(tri.p[2][2]);
        if zhi < z0 || zlo > z1 {
            continue;
        }
        if is_horizontal(&tri) {
            if zlo > z0 && zlo < z1 {
                let poly: Vec<V2> = tri.p.iter().map(|c| [c[0], c[1]]).collect();
                push_polygon(&poly, envelope);
            }
        } else {
            push_polygon(&clip_to_slab(&tri, z0, z1), envelope);
        }
    }
}

fn rings_of(segs: &[Seg]) -> Rings {
    if segs.is_empty() {
        return Rings::default();
    }
    sweep::rings(&sweep::strips(segs))
}

/// Slab for layers `first..last` (scratch buffers reused).
fn slabs_in(mesh: &Mesh, grid: &LayerGrid, b: &Buckets, first: u32, last: u32) -> Vec<LayerSlab> {
    let mut env = Vec::new();
    let mut mid = Vec::new();
    (first..last)
        .map(|k| {
            layer_segments(
                mesh,
                b.layer(k as usize),
                grid.z_bottom(k),
                grid.z_top(k),
                &mut env,
                &mut mid,
            );
            LayerSlab {
                layer: k,
                envelope: rings_of(&env),
                midplane: rings_of(&mid),
            }
        })
        .collect()
}

/// Slice `mesh` into every layer of `grid` in one pass, in layer order.
#[must_use]
pub fn slice_layers(mesh: &Mesh, grid: &LayerGrid) -> Measure<Vec<LayerSlab>> {
    slice_layers_par(mesh, grid, 1)
}

/// Diagnostic batch of explicit slab endpoints, sorted by both bottom and top.
/// Uses the production material-envelope kernel and builds triangle bands once.
/// Bounded to 1024 slabs so phase diagnostics cannot create an unbounded work list.
pub(crate) fn slice_slabs_at(mesh: &Mesh, bounds: &[[f64; 2]]) -> Measure<Vec<LayerSlab>> {
    if bounds.len() > 1024
        || bounds
            .iter()
            .any(|z| !z[0].is_finite() || !z[1].is_finite() || z[0] >= z[1])
        || bounds
            .windows(2)
            .any(|w| w[0][0] > w[1][0] || w[0][1] > w[1][1])
    {
        return Measure::Unverified("invalid or over-budget irregular slab batch");
    }
    counters::record(Op::SliceLayers, bounds.len());
    let mut active = vec![Vec::new(); bounds.len()];
    for t in 0..mesh.triangle_count() {
        let triangle = mesh.triangle_corners(t);
        let lo = triangle.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min);
        let hi = triangle
            .iter()
            .map(|v| v[2])
            .fold(f64::NEG_INFINITY, f64::max);
        let first = bounds.partition_point(|z| z[1] < lo);
        let last = bounds.partition_point(|z| z[0] <= hi);
        for band in &mut active[first..last] {
            band.push(t as u32);
        }
    }
    let mut env = Vec::new();
    let mut mid = Vec::new();
    Measure::Known(
        bounds
            .iter()
            .zip(active)
            .enumerate()
            .map(|(k, (&[bottom, top], tris))| {
                layer_segments(mesh, &tris, bottom, top, &mut env, &mut mid);
                LayerSlab {
                    layer: k as u32,
                    envelope: rings_of(&env),
                    midplane: rings_of(&mid),
                }
            })
            .collect(),
    )
}

/// Like [`slice_layers`] on up to `threads` threads, in contiguous layer ranges. The result
/// is identical for every `threads`. The calling thread is one of the workers; counted once,
/// on the calling thread, with one item per layer. The tasks made and the distinct threads
/// that ran them are left for [`counters::take_par`].
#[must_use]
pub fn slice_layers_par(mesh: &Mesh, grid: &LayerGrid, threads: usize) -> Measure<Vec<LayerSlab>> {
    if !grid.valid() {
        return Measure::Unverified("slice_layers: the layer grid is not valid");
    }
    counters::record(Op::SliceLayers, grid.layers as usize);
    if grid.layers == 0 || mesh.triangle_count() == 0 {
        counters::record_par(ParReport {
            tasks: 0,
            threads: 0,
        });
        return Measure::Known(
            (0..grid.layers)
                .map(|k| LayerSlab {
                    layer: k,
                    ..LayerSlab::default()
                })
                .collect(),
        );
    }
    let b = Buckets::of(mesh, grid);
    let (slabs, report) = par_ranges(grid.layers as usize, threads, &|first, last| {
        #[allow(clippy::cast_possible_truncation)]
        slabs_in(mesh, grid, &b, first as u32, last as u32)
    });
    counters::record_par(report);
    Measure::Known(slabs)
}

/// Run `work(first, last)` over contiguous ranges of `0..n` on up to `threads` threads (the
/// calling thread is one of them) and join the results in order. The report says how many
/// tasks ran on how many distinct threads. The result is the same for every `threads`.
pub(crate) fn par_ranges<T: Send>(
    n: usize,
    threads: usize,
    work: &(dyn Fn(usize, usize) -> Vec<T> + Sync),
) -> (Vec<T>, ParReport) {
    if n == 0 {
        return (
            Vec::new(),
            ParReport {
                tasks: 0,
                threads: 0,
            },
        );
    }
    let tasks = threads.clamp(1, n);
    let per = n.div_ceil(tasks);
    let ranges: Vec<(usize, usize)> = (0..tasks)
        .map(|i| ((i * per).min(n), ((i + 1) * per).min(n)))
        .filter(|r| r.0 < r.1)
        .collect();
    let (parts, ids) = std::thread::scope(|s| {
        let handles: Vec<_> = ranges[1..]
            .iter()
            .map(|&(a, z)| s.spawn(move || (work(a, z), std::thread::current().id())))
            .collect();
        let mut parts = vec![work(ranges[0].0, ranges[0].1)];
        let mut ids = vec![std::thread::current().id()];
        for h in handles {
            let (p, id) = h.join().unwrap_or_else(|e| std::panic::resume_unwind(e));
            parts.push(p);
            ids.push(id);
        }
        (parts, ids)
    });
    let mut distinct = Vec::with_capacity(ids.len());
    for id in ids {
        if !distinct.contains(&id) {
            distinct.push(id);
        }
    }
    (
        parts.into_iter().flatten().collect(),
        ParReport {
            tasks: ranges.len(),
            threads: distinct.len(),
        },
    )
}

/// Scalar references: slow, direct versions of the batched pass, for tests.
pub mod reference {
    use super::{LayerGrid, Seg, layer_segments, slice_segment, tri_of};
    use crate::mesh::Mesh;

    /// The area of the cross-section at plane `zp` by the shoelace sum of the directed slice
    /// segments of every triangle: no sweep, no union.
    #[must_use]
    pub fn slice_area(mesh: &Mesh, zp: f64, tie_above: bool) -> f64 {
        let mut a = 0.0;
        for t in 0..mesh.triangle_count() {
            #[allow(clippy::cast_possible_truncation)]
            let tri = tri_of(mesh, t as u32);
            if let Some((p, q)) = slice_segment(&tri, zp, tie_above) {
                a += 0.5 * (p[0] * q[1] - q[0] * p[1]);
            }
        }
        a
    }

    /// The envelope area of layer `k` with all-pairs crossing events and a winding test at the
    /// middle of every strip: no active list, no incremental cutting.
    #[must_use]
    pub fn envelope_area(mesh: &Mesh, grid: &LayerGrid, k: u32) -> f64 {
        let all: Vec<u32> = (0..mesh.triangle_count())
            .map(|t| {
                #[allow(clippy::cast_possible_truncation)]
                {
                    t as u32
                }
            })
            .collect();
        let mut env = Vec::new();
        let mut mid = Vec::new();
        layer_segments(
            mesh,
            &all,
            grid.z_bottom(k),
            grid.z_top(k),
            &mut env,
            &mut mid,
        );
        brute_union_area(&env)
    }

    /// The area where the winding of `segs` is above zero, by cutting at every end point and
    /// every pairwise crossing.
    #[must_use]
    pub(super) fn brute_union_area(segs: &[Seg]) -> f64 {
        let mut ys: Vec<f64> = segs.iter().flat_map(|s| [s.y0, s.y1]).collect();
        for (i, a) in segs.iter().enumerate() {
            for b in &segs[i + 1..] {
                if let Some(y) = crossing_y(a, b) {
                    ys.push(y);
                }
            }
        }
        ys.sort_unstable_by(f64::total_cmp);
        ys.dedup();
        let mut area = 0.0;
        for w in ys.windows(2) {
            let (ya, yb) = (w[0], w[1]);
            let ym = 0.5 * (ya + yb);
            let mut at: Vec<(f64, f64, f64, i32)> = segs
                .iter()
                .filter(|s| s.y0 <= ya && s.y1 >= yb)
                .map(|s| (s.x_at(ym), s.x_at(ya), s.x_at(yb), i32::from(s.d)))
                .collect();
            at.sort_by(|p, q| p.0.total_cmp(&q.0));
            let mut wind = 0;
            let mut left: Option<(f64, f64)> = None;
            for (_, xa, xb, d) in at {
                let before = wind;
                wind += d;
                if before <= 0 && wind > 0 {
                    left = Some((xa, xb));
                } else if before > 0
                    && wind <= 0
                    && let Some((la, lb)) = left.take()
                {
                    area += 0.5 * (yb - ya) * ((xa - la) + (xb - lb));
                }
            }
        }
        area
    }

    fn crossing_y(a: &Seg, b: &Seg) -> Option<f64> {
        let lo = a.y0.max(b.y0);
        let hi = a.y1.min(b.y1);
        if lo >= hi {
            return None;
        }
        let (da, db) = (a.x_at(lo) - b.x_at(lo), a.x_at(hi) - b.x_at(hi));
        if da * db < 0.0 {
            Some(lo + (hi - lo) * da / (da - db))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
