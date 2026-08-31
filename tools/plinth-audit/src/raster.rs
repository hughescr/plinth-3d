//! The printer-grid rasteriser, in run-length form.
//!
//! `audit-invariants.md` §1.6 pins three things and this module implements
//! exactly them.
//!
//! * **The grid is the printer's own**: `0.014 x 0.019` mm in plane, `0.050`
//!   mm in Z, origin at the centred frame's origin, layer planes at
//!   `z_k = z_plate + (k + 0.5) * layer_height`.
//! * **The representation is run-length, never a pixel array.** A per-pixel
//!   loop over a 16K grid is a defect, not an implementation choice: a 200 mm
//!   model is on the order of 1.5e11 pixel visits and the interval form is on
//!   the order of 1e7 edge visits for the same answer.
//! * **The fill rule is full containment.** A pixel column is cured at layer
//!   `k` only if the solid contains the WHOLE pixel footprint at `z_k`.
//!   Partial pixels are NOT cured. The error direction is toward failing,
//!   which is the safe direction for a gate, and its cost is disclosed in
//!   `report.raster_erosion_sensitivity` rather than hidden.
//!
//! **Budgets.** Every entry point takes a [`Budget`]. Exceeding it returns
//! [`RasterError::BudgetExceeded`], which a check must report as
//! `raster_budget_exceeded` and therefore as a FAILURE. It never silently
//! truncates a raster and never returns a partial one.

use crate::consts as c;
use crate::mesh::Mesh;
use crate::primitives::Primitive;
use crate::vec3::{self as v, Aabb, V3};
use std::collections::BTreeMap;

/// The printer lattice. Built once per file and shared by every check.
#[derive(Clone, Copy, Debug)]
pub struct RasterGrid {
    pub pitch_x_mm: f64,
    pub pitch_y_mm: f64,
    pub layer_height_mm: f64,
    /// The plate in the model-centred frame, `-translate_z`.
    pub z_plate_mm: f64,
}

impl RasterGrid {
    #[must_use]
    pub const fn saturn4(z_plate_mm: f64) -> Self {
        Self {
            pitch_x_mm: c::PIXEL_PITCH_X_MM,
            pitch_y_mm: c::PIXEL_PITCH_Y_MM,
            layer_height_mm: c::LAYER_HEIGHT_DEFAULT_MM,
            z_plate_mm,
        }
    }
    /// Sampling height of layer `k`.
    #[must_use]
    pub fn layer_z_mm(&self, k: i64) -> f64 {
        self.z_plate_mm + (k as f64 + 0.5) * self.layer_height_mm
    }
    /// The lowest layer index whose sampling plane is at or above `z`.
    #[must_use]
    pub fn layer_at_or_above(&self, z: f64) -> i64 {
        (((z - self.z_plate_mm) / self.layer_height_mm) - 0.5).ceil() as i64
    }
    #[must_use]
    pub fn layer_at_or_below(&self, z: f64) -> i64 {
        (((z - self.z_plate_mm) / self.layer_height_mm) - 0.5).floor() as i64
    }
    /// Half-open x span of column `i`.
    #[must_use]
    pub fn column_x_mm(&self, i: i64) -> (f64, f64) {
        (i as f64 * self.pitch_x_mm, (i + 1) as f64 * self.pitch_x_mm)
    }
    /// Half-open y span of row `j`.
    #[must_use]
    pub fn row_y_mm(&self, j: i64) -> (f64, f64) {
        (j as f64 * self.pitch_y_mm, (j + 1) as f64 * self.pitch_y_mm)
    }
    /// The y of row boundary `j`, i.e. the low edge of row `j`.
    #[must_use]
    pub fn row_boundary_y_mm(&self, j: i64) -> f64 {
        j as f64 * self.pitch_y_mm
    }
    #[must_use]
    pub const fn pixel_area_mm2(&self) -> f64 {
        self.pitch_x_mm * self.pitch_y_mm
    }
    #[must_use]
    pub const fn voxel_volume_mm3(&self) -> f64 {
        self.pitch_x_mm * self.pitch_y_mm * self.layer_height_mm
    }
    /// The closed box voxel `(k, row, col)` occupies.
    #[must_use]
    pub fn voxel_box(&self, k: i64, row: i64, col: i64) -> Aabb {
        let (x0, x1) = self.column_x_mm(col);
        let (y0, y1) = self.row_y_mm(row);
        let z = self.layer_z_mm(k);
        Aabb {
            min: [x0, y0, z - self.layer_height_mm * 0.5],
            max: [x1, y1, z + self.layer_height_mm * 0.5],
        }
    }
}

/// A half-open run of columns `[c0, c1)` in one pixel row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub c0: i64,
    pub c1: i64,
}

impl Run {
    #[must_use]
    pub const fn len(&self) -> i64 {
        self.c1 - self.c0
    }
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.c1 <= self.c0
    }
}

/// One layer's cured pixels: sorted rows, each a sorted disjoint run list.
#[derive(Clone, Debug, Default)]
pub struct Layer {
    pub k: i64,
    pub z_mm: f64,
    /// `row -> runs`, ordered by row so the whole layer walks in one pass.
    pub rows: BTreeMap<i64, Vec<Run>>,
}

impl Layer {
    #[must_use]
    pub fn pixel_count(&self) -> u64 {
        self.rows
            .values()
            .flat_map(|rs| rs.iter())
            .map(|r| r.len().max(0) as u64)
            .sum()
    }
    #[must_use]
    pub fn runs_in_row(&self, row: i64) -> &[Run] {
        self.rows.get(&row).map_or(&[], Vec::as_slice)
    }
    #[must_use]
    pub fn contains(&self, row: i64, col: i64) -> bool {
        self.runs_in_row(row)
            .iter()
            .any(|r| col >= r.c0 && col < r.c1)
    }
}

#[derive(Clone, Debug)]
pub enum RasterError {
    /// The work or the output would exceed the budget. A check reporting this
    /// must call itself `raster_budget_exceeded` and FAIL.
    BudgetExceeded { what: String, limit: u64 },
    /// The solid's bounding box is empty or non-finite.
    EmptyOrNonFinite,
    /// A mesh that is not closed has no interior to rasterise.
    MeshNotClosed,
}

impl std::fmt::Display for RasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BudgetExceeded { what, limit } => {
                write!(f, "raster budget exceeded: {what} over {limit}")
            }
            Self::EmptyOrNonFinite => write!(f, "solid has an empty or non-finite bounding box"),
            Self::MeshNotClosed => write!(f, "mesh is not closed; it has no interior"),
        }
    }
}

/// Work and memory limits. Defaults are sized for the corpus's largest model
/// (the treant, 429,120 faces, ~91 mm tall) with headroom.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub max_layers: u64,
    pub max_row_visits: u64,
    pub max_runs: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_layers: 20_000,
            max_row_visits: 400_000_000,
            max_runs: 200_000_000,
        }
    }
}

/// A cured solid on the printer lattice.
#[derive(Clone, Debug)]
pub struct RunRaster {
    pub grid: RasterGrid,
    /// Layers in ascending `k`. Empty layers are omitted.
    pub layers: Vec<Layer>,
    /// Total cured pixels across every layer.
    pub pixel_count: u64,
    /// Total run count, the honest size of the representation.
    pub run_count: u64,
}

impl RunRaster {
    #[must_use]
    pub fn empty(grid: RasterGrid) -> Self {
        Self {
            grid,
            layers: Vec::new(),
            pixel_count: 0,
            run_count: 0,
        }
    }

    /// Cured volume, mm^3. Exact: a count of voxels times the voxel volume.
    #[must_use]
    pub fn volume_mm3(&self) -> f64 {
        self.pixel_count as f64 * self.grid.voxel_volume_mm3()
    }
    /// Cured volume in mL, for `report.resin_split`.
    #[must_use]
    pub fn volume_ml(&self) -> f64 {
        self.volume_mm3() / 1000.0
    }
    /// Cured area of one layer, mm^2.
    #[must_use]
    pub fn layer_area_mm2(&self, k: i64) -> f64 {
        self.layer(k)
            .map_or(0.0, |l| l.pixel_count() as f64 * self.grid.pixel_area_mm2())
    }
    #[must_use]
    pub fn layer(&self, k: i64) -> Option<&Layer> {
        self.layers
            .binary_search_by_key(&k, |l| l.k)
            .ok()
            .map(|i| &self.layers[i])
    }
    #[must_use]
    pub fn layer_range(&self) -> Option<(i64, i64)> {
        Some((self.layers.first()?.k, self.layers.last()?.k))
    }
    #[must_use]
    pub fn contains_voxel(&self, k: i64, row: i64, col: i64) -> bool {
        self.layer(k).is_some_and(|l| l.contains(row, col))
    }
    /// Is the point inside a cured voxel?
    #[must_use]
    pub fn contains_point(&self, p: V3) -> bool {
        let k = self.grid.layer_at_or_below(p[2]);
        // The point may sit in the upper half of layer k or the lower half of
        // k+1; the voxel that owns it is the nearer sampling plane.
        for kk in [k, k + 1] {
            let vz = self.grid.layer_z_mm(kk);
            if (p[2] - vz).abs() <= self.grid.layer_height_mm * 0.5 {
                let row = (p[1] / self.grid.pitch_y_mm).floor() as i64;
                let col = (p[0] / self.grid.pitch_x_mm).floor() as i64;
                return self.contains_voxel(kk, row, col);
            }
        }
        false
    }
    /// Every cured voxel whose box meets `query`, as `(k, row, col, box)`.
    ///
    /// This is the primitive I2, I3 and I6 clip a plane or another solid
    /// against. It walks runs, never pixels outside the query.
    #[must_use]
    pub fn voxels_in_box(&self, query: &Aabb) -> Vec<(i64, i64, i64, Aabb)> {
        let mut out = Vec::new();
        if query.is_empty() {
            return out;
        }
        let g = self.grid;
        let k_lo = g.layer_at_or_below(query.min[2]) - 1;
        let k_hi = g.layer_at_or_above(query.max[2]) + 1;
        let r_lo = (query.min[1] / g.pitch_y_mm).floor() as i64;
        let r_hi = (query.max[1] / g.pitch_y_mm).floor() as i64;
        let c_lo = (query.min[0] / g.pitch_x_mm).floor() as i64;
        let c_hi = (query.max[0] / g.pitch_x_mm).floor() as i64;
        for layer in &self.layers {
            if layer.k < k_lo || layer.k > k_hi {
                continue;
            }
            for (&row, runs) in &layer.rows {
                if row < r_lo || row > r_hi {
                    continue;
                }
                for r in runs {
                    let a = r.c0.max(c_lo);
                    let b = (r.c1 - 1).min(c_hi);
                    for col in a..=b {
                        let bx = g.voxel_box(layer.k, row, col);
                        if bx.intersects(query) {
                            out.push((layer.k, row, col, bx));
                        }
                    }
                }
            }
        }
        out
    }

    /// Boolean intersection of two rasters on the same grid.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Self {
        self.combine(other, RunOp::Intersect)
    }
    /// `self` minus `other`.
    #[must_use]
    pub fn difference(&self, other: &Self) -> Self {
        self.combine(other, RunOp::Difference)
    }
    /// Boolean union of two rasters on the same grid.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        self.combine(other, RunOp::Union)
    }

    fn combine(&self, other: &Self, op: RunOp) -> Self {
        let mut layers: Vec<Layer> = Vec::new();
        let mut ks: Vec<i64> = self.layers.iter().map(|l| l.k).collect();
        ks.extend(other.layers.iter().map(|l| l.k));
        ks.sort_unstable();
        ks.dedup();
        for k in ks {
            let a = self.layer(k);
            let b = other.layer(k);
            let mut rows: BTreeMap<i64, Vec<Run>> = BTreeMap::new();
            let mut all_rows: Vec<i64> = Vec::new();
            if let Some(a) = a {
                all_rows.extend(a.rows.keys().copied());
            }
            if let Some(b) = b {
                all_rows.extend(b.rows.keys().copied());
            }
            all_rows.sort_unstable();
            all_rows.dedup();
            for row in all_rows {
                let ra = a.map_or(&[][..], |l| l.runs_in_row(row));
                let rb = b.map_or(&[][..], |l| l.runs_in_row(row));
                let merged = run_op(ra, rb, op);
                if !merged.is_empty() {
                    rows.insert(row, merged);
                }
            }
            if !rows.is_empty() {
                layers.push(Layer {
                    k,
                    z_mm: self.grid.layer_z_mm(k),
                    rows,
                });
            }
        }
        finish(self.grid, layers)
    }

    /// Rasterise the union of convex analytic primitives.
    ///
    /// Exact under the full-containment rule: a convex slice contains a pixel
    /// rectangle exactly when it contains its four corners, so the chord at
    /// each of the row's two boundary lines decides the whole row.
    pub fn from_primitives(
        prims: &[Primitive],
        grid: RasterGrid,
        budget: &Budget,
    ) -> Result<Self, RasterError> {
        let mut bounds = Aabb::empty();
        for p in prims {
            bounds.union(&p.aabb());
        }
        if bounds.is_empty() || !v::is_finite(bounds.min) || !v::is_finite(bounds.max) {
            return Ok(Self::empty(grid));
        }
        let k_lo = grid.layer_at_or_above(bounds.min[2]);
        let k_hi = grid.layer_at_or_below(bounds.max[2]);
        if k_hi < k_lo {
            return Ok(Self::empty(grid));
        }
        if (k_hi - k_lo + 1) as u64 > budget.max_layers {
            return Err(RasterError::BudgetExceeded {
                what: "layers".into(),
                limit: budget.max_layers,
            });
        }
        let mut layers: Vec<Layer> = Vec::new();
        let mut row_visits: u64 = 0;
        for k in k_lo..=k_hi {
            let z = grid.layer_z_mm(k);
            let mut rows: BTreeMap<i64, Vec<Run>> = BTreeMap::new();
            for p in prims {
                let bb = p.aabb();
                if z < bb.min[2] || z > bb.max[2] {
                    continue;
                }
                let j_lo = (bb.min[1] / grid.pitch_y_mm).floor() as i64;
                let j_hi = (bb.max[1] / grid.pitch_y_mm).floor() as i64;
                row_visits += (j_hi - j_lo + 1).max(0) as u64;
                if row_visits > budget.max_row_visits {
                    return Err(RasterError::BudgetExceeded {
                        what: "row visits".into(),
                        limit: budget.max_row_visits,
                    });
                }
                for j in j_lo..=j_hi {
                    let (y0, y1) = grid.row_y_mm(j);
                    let (Some((lo0, hi0)), Some((lo1, hi1))) = (p.x_chord(y0, z), p.x_chord(y1, z))
                    else {
                        continue;
                    };
                    let lo = lo0.max(lo1);
                    let hi = hi0.min(hi1);
                    if hi <= lo {
                        continue;
                    }
                    let c0 = (lo / grid.pitch_x_mm).ceil() as i64;
                    let c1 = (hi / grid.pitch_x_mm).floor() as i64;
                    if c1 > c0 {
                        insert_run(rows.entry(j).or_default(), Run { c0, c1 });
                    }
                }
            }
            if !rows.is_empty() {
                for runs in rows.values_mut() {
                    normalise(runs);
                }
                layers.push(Layer { k, z_mm: z, rows });
            }
        }
        let out = finish(grid, layers);
        if out.run_count > budget.max_runs {
            return Err(RasterError::BudgetExceeded {
                what: "runs".into(),
                limit: budget.max_runs,
            });
        }
        Ok(out)
    }

    /// Rasterise a closed triangle mesh.
    ///
    /// The mesh must be closed: an open soup has no interior and the parity
    /// scanline would be meaningless (`audit-invariants.md` §1.4).
    ///
    /// Per layer the work is: slice into oriented 2D segments; take a winding
    /// scanline at every pixel-ROW BOUNDARY (one per boundary, not two per
    /// row); a row's fully-covered columns are the intersection of its two
    /// boundary interval sets; then any column an edge passes through inside
    /// that row band is removed, because such an edge enters the pixel between
    /// the two scanlines and full containment then fails.
    pub fn from_mesh(m: &Mesh, grid: RasterGrid, budget: &Budget) -> Result<Self, RasterError> {
        if m.is_empty() {
            return Ok(Self::empty(grid));
        }
        if !m.closure.is_closed() {
            return Err(RasterError::MeshNotClosed);
        }
        let bounds = m.bbox;
        let k_lo = grid.layer_at_or_above(bounds.min[2]);
        let k_hi = grid.layer_at_or_below(bounds.max[2]);
        if k_hi < k_lo {
            return Ok(Self::empty(grid));
        }
        let nlayers = (k_hi - k_lo + 1) as u64;
        if nlayers > budget.max_layers {
            return Err(RasterError::BudgetExceeded {
                what: "layers".into(),
                limit: budget.max_layers,
            });
        }

        // Bucket triangles by the layer range they span, once. Batch-first: a
        // per-layer scan of all 429k triangles would be 7.8e8 visits on the
        // treant; the bucket form is one pass plus the crossings themselves.
        let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); nlayers as usize];
        for ti in 0..m.tri_count() {
            let [a, b, cc] = m.tri(ti);
            let zmin = a[2].min(b[2]).min(cc[2]);
            let zmax = a[2].max(b[2]).max(cc[2]);
            let lo = grid.layer_at_or_above(zmin).max(k_lo);
            let hi = grid.layer_at_or_below(zmax).min(k_hi);
            for k in lo..=hi {
                buckets[(k - k_lo) as usize].push(ti as u32);
            }
        }

        let mut layers: Vec<Layer> = Vec::new();
        let mut row_visits: u64 = 0;
        for k in k_lo..=k_hi {
            let z = grid.layer_z_mm(k);
            let segs = slice_layer(m, &buckets[(k - k_lo) as usize], z);
            if segs.is_empty() {
                continue;
            }
            let mut ymin = f64::INFINITY;
            let mut ymax = f64::NEG_INFINITY;
            for s in &segs {
                ymin = ymin.min(s.p[1]).min(s.q[1]);
                ymax = ymax.max(s.p[1]).max(s.q[1]);
            }
            let j_lo = (ymin / grid.pitch_y_mm).floor() as i64;
            let j_hi = (ymax / grid.pitch_y_mm).floor() as i64;
            let nrows = (j_hi - j_lo + 1).max(0) as usize;
            row_visits += nrows as u64;
            if row_visits > budget.max_row_visits {
                return Err(RasterError::BudgetExceeded {
                    what: "row visits".into(),
                    limit: budget.max_row_visits,
                });
            }

            // Crossings at every row boundary j_lo ..= j_hi + 1.
            let nb = nrows + 1;
            let mut crossings: Vec<Vec<(f64, i32)>> = vec![Vec::new(); nb];
            // Columns an edge passes through, per row band.
            let mut blocked: Vec<Vec<(i64, i64)>> = vec![Vec::new(); nrows];
            for s in &segs {
                let (ylo, yhi, dir) = if s.p[1] <= s.q[1] {
                    (s.p[1], s.q[1], 1i32)
                } else {
                    (s.q[1], s.p[1], -1i32)
                };
                // Boundaries strictly crossed: half-open [ylo, yhi).
                let b0 = (ylo / grid.pitch_y_mm).ceil() as i64;
                let b1 = ((yhi / grid.pitch_y_mm).ceil() as i64) - 1;
                for jb in b0.max(j_lo)..=b1.min(j_hi + 1) {
                    let yb = grid.row_boundary_y_mm(jb);
                    if yb < ylo || yb >= yhi {
                        continue;
                    }
                    let t = (yb - s.p[1]) / (s.q[1] - s.p[1]);
                    let x = s.p[0] + t * (s.q[0] - s.p[0]);
                    crossings[(jb - j_lo) as usize].push((x, dir));
                }
                // Row bands this segment touches at all.
                let r0 = ((ylo / grid.pitch_y_mm).floor() as i64).max(j_lo);
                let r1 = ((yhi / grid.pitch_y_mm).floor() as i64).min(j_hi);
                for j in r0..=r1 {
                    let (by0, by1) = grid.row_y_mm(j);
                    let Some((xa, xb)) = clip_segment_x_extent(s, by0, by1) else {
                        continue;
                    };
                    let ca = (xa / grid.pitch_x_mm).floor() as i64;
                    let cb = (xb / grid.pitch_x_mm).floor() as i64;
                    blocked[(j - j_lo) as usize].push((ca, cb));
                }
            }

            let mut boundary_spans: Vec<Vec<(f64, f64)>> = Vec::with_capacity(nb);
            for cr in &mut crossings {
                cr.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                let mut spans = Vec::new();
                let mut wind = 0i32;
                let mut open_x = 0.0f64;
                for &(x, d) in cr.iter() {
                    let before = wind;
                    wind += d;
                    if before == 0 && wind != 0 {
                        open_x = x;
                    } else if before != 0 && wind == 0 {
                        spans.push((open_x, x));
                    }
                }
                boundary_spans.push(spans);
            }

            let mut rows: BTreeMap<i64, Vec<Run>> = BTreeMap::new();
            for j in j_lo..=j_hi {
                let a = &boundary_spans[(j - j_lo) as usize];
                let b = &boundary_spans[(j - j_lo + 1) as usize];
                if a.is_empty() || b.is_empty() {
                    continue;
                }
                let both = intersect_spans(a, b);
                let mut runs: Vec<Run> = Vec::new();
                for (lo, hi) in both {
                    let c0 = (lo / grid.pitch_x_mm).ceil() as i64;
                    let c1 = (hi / grid.pitch_x_mm).floor() as i64;
                    if c1 > c0 {
                        runs.push(Run { c0, c1 });
                    }
                }
                if runs.is_empty() {
                    continue;
                }
                normalise(&mut runs);
                // Remove every column an edge enters inside this row band.
                for &(ca, cb) in &blocked[(j - j_lo) as usize] {
                    runs = subtract_run(&runs, Run { c0: ca, c1: cb + 1 });
                    if runs.is_empty() {
                        break;
                    }
                }
                if !runs.is_empty() {
                    rows.insert(j, runs);
                }
            }
            if !rows.is_empty() {
                layers.push(Layer { k, z_mm: z, rows });
            }
        }
        let out = finish(grid, layers);
        if out.run_count > budget.max_runs {
            return Err(RasterError::BudgetExceeded {
                what: "runs".into(),
                limit: budget.max_runs,
            });
        }
        Ok(out)
    }

    /// Connected components of one layer's cured pixels under 4-connectivity
    /// on runs. Returns one run list per component.
    ///
    /// This is I7's island primitive and I2's patch-component primitive.
    #[must_use]
    pub fn layer_components(&self, k: i64) -> Vec<Vec<(i64, Run)>> {
        let Some(layer) = self.layer(k) else {
            return Vec::new();
        };
        let mut items: Vec<(i64, Run)> = Vec::new();
        for (&row, runs) in &layer.rows {
            for r in runs {
                items.push((row, *r));
            }
        }
        let n = items.len();
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(p: &mut [usize], mut x: usize) -> usize {
            while p[x] != x {
                p[x] = p[p[x]];
                x = p[x];
            }
            x
        }
        // Same row: adjacent runs never touch after normalise(). Cross-row:
        // runs in row j and j+1 whose column spans overlap.
        for i in 0..n {
            for jx in (i + 1)..n {
                if (items[i].0 - items[jx].0).abs() != 1 {
                    continue;
                }
                let (a, b) = (items[i].1, items[jx].1);
                if a.c0 < b.c1 && b.c0 < a.c1 {
                    let (ra, rb) = (find(&mut parent, i), find(&mut parent, jx));
                    if ra != rb {
                        parent[ra] = rb;
                    }
                }
            }
        }
        let mut groups: BTreeMap<usize, Vec<(i64, Run)>> = BTreeMap::new();
        for i in 0..n {
            let r = find(&mut parent, i);
            groups.entry(r).or_default().push(items[i]);
        }
        groups.into_values().collect()
    }
}

#[derive(Clone, Copy)]
enum RunOp {
    Intersect,
    Union,
    Difference,
}

fn run_op(a: &[Run], b: &[Run], op: RunOp) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    match op {
        RunOp::Union => {
            out.extend_from_slice(a);
            out.extend_from_slice(b);
            normalise(&mut out);
        }
        RunOp::Intersect => {
            let (mut i, mut j) = (0usize, 0usize);
            while i < a.len() && j < b.len() {
                let lo = a[i].c0.max(b[j].c0);
                let hi = a[i].c1.min(b[j].c1);
                if hi > lo {
                    out.push(Run { c0: lo, c1: hi });
                }
                if a[i].c1 < b[j].c1 {
                    i += 1;
                } else {
                    j += 1;
                }
            }
        }
        RunOp::Difference => {
            out.extend_from_slice(a);
            for cut in b {
                out = subtract_run(&out, *cut);
            }
        }
    }
    out
}

fn subtract_run(runs: &[Run], cut: Run) -> Vec<Run> {
    let mut out = Vec::with_capacity(runs.len() + 1);
    for r in runs {
        if cut.c1 <= r.c0 || cut.c0 >= r.c1 {
            out.push(*r);
            continue;
        }
        if r.c0 < cut.c0 {
            out.push(Run {
                c0: r.c0,
                c1: cut.c0,
            });
        }
        if cut.c1 < r.c1 {
            out.push(Run {
                c0: cut.c1,
                c1: r.c1,
            });
        }
    }
    out
}

fn insert_run(runs: &mut Vec<Run>, r: Run) {
    runs.push(r);
}

/// Sort, merge touching or overlapping runs, drop empties.
fn normalise(runs: &mut Vec<Run>) {
    runs.retain(|r| !r.is_empty());
    runs.sort_unstable_by_key(|r| (r.c0, r.c1));
    let mut out: Vec<Run> = Vec::with_capacity(runs.len());
    for r in runs.iter() {
        match out.last_mut() {
            Some(last) if r.c0 <= last.c1 => last.c1 = last.c1.max(r.c1),
            _ => out.push(*r),
        }
    }
    *runs = out;
}

fn intersect_spans(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() && j < b.len() {
        let lo = a[i].0.max(b[j].0);
        let hi = a[i].1.min(b[j].1);
        if hi > lo {
            out.push((lo, hi));
        }
        if a[i].1 < b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// One oriented 2D slice segment. Material lies to the LEFT of `p -> q`.
#[derive(Clone, Copy, Debug)]
struct Seg {
    p: [f64; 2],
    q: [f64; 2],
}

fn clip_segment_x_extent(s: &Seg, y0: f64, y1: f64) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let dy = s.q[1] - s.p[1];
    if dy.abs() < 1e-300 {
        if s.p[1] < y0 || s.p[1] > y1 {
            return None;
        }
    } else {
        let a = (y0 - s.p[1]) / dy;
        let b = (y1 - s.p[1]) / dy;
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        t0 = t0.max(lo);
        t1 = t1.min(hi);
        if t0 > t1 {
            return None;
        }
    }
    let xa = s.p[0] + t0 * (s.q[0] - s.p[0]);
    let xb = s.p[0] + t1 * (s.q[0] - s.p[0]);
    Some((xa.min(xb), xa.max(xb)))
}

/// Slice the candidate triangles at plane `z` into oriented 2D segments.
///
/// Orientation: with outward surface normal `n`, the 2D boundary direction
/// that keeps material on the left is `(-n_y, n_x)`. A triangle whose normal
/// is (nearly) vertical produces a degenerate segment and is dropped -- a
/// horizontal facet contributes no boundary to a horizontal slice.
fn slice_layer(m: &Mesh, cands: &[u32], z: f64) -> Vec<Seg> {
    let mut out = Vec::with_capacity(cands.len());
    for &ti in cands {
        let ti = ti as usize;
        let [a, b, cc] = m.tri(ti);
        let d = [a[2] - z, b[2] - z, cc[2] - z];
        // Half-open classification avoids double-counting a vertex on the
        // plane: treat `d == 0` as strictly above.
        let above = [d[0] >= 0.0, d[1] >= 0.0, d[2] >= 0.0];
        let n_above = usize::from(above[0]) + usize::from(above[1]) + usize::from(above[2]);
        if n_above == 0 || n_above == 3 {
            continue;
        }
        let vs = [a, b, cc];
        let mut pts: Vec<[f64; 2]> = Vec::with_capacity(2);
        for e in 0..3 {
            let (i0, i1) = (e, (e + 1) % 3);
            if above[i0] != above[i1] {
                let t = d[i0] / (d[i0] - d[i1]);
                pts.push([
                    vs[i0][0] + t * (vs[i1][0] - vs[i0][0]),
                    vs[i0][1] + t * (vs[i1][1] - vs[i0][1]),
                ]);
            }
        }
        if pts.len() != 2 {
            continue;
        }
        let n = m.normals[ti];
        let want = [-n[1], n[0]];
        let dir = [pts[1][0] - pts[0][0], pts[1][1] - pts[0][1]];
        if dir[0] * dir[0] + dir[1] * dir[1] <= 0.0 {
            continue;
        }
        let (p, q) = if dir[0].mul_add(want[0], dir[1] * want[1]) >= 0.0 {
            (pts[0], pts[1])
        } else {
            (pts[1], pts[0])
        };
        out.push(Seg { p, q });
    }
    out
}

fn finish(grid: RasterGrid, layers: Vec<Layer>) -> RunRaster {
    let mut pixel_count = 0u64;
    let mut run_count = 0u64;
    for l in &layers {
        for runs in l.rows.values() {
            run_count += runs.len() as u64;
            for r in runs {
                pixel_count += r.len().max(0) as u64;
            }
        }
    }
    RunRaster {
        grid,
        layers,
        pixel_count,
        run_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> RasterGrid {
        RasterGrid::saturn4(0.0)
    }

    #[test]
    fn a_cylinder_rasterises_to_about_its_area_but_never_more() {
        // A 2 mm radius cylinder, 1 mm tall, sitting on the plate.
        let p = Primitive::Frustum {
            a: [0.0, 0.0, 0.0],
            ra: 2.0,
            b: [0.0, 0.0, 1.0],
            rb: 2.0,
        };
        let r = RunRaster::from_primitives(&[p], grid(), &Budget::default()).expect("raster");
        let (k0, k1) = r.layer_range().expect("layers");
        assert_eq!(k1 - k0 + 1, 20, "1 mm at 0.05 mm layers");
        let ideal = std::f64::consts::PI * 4.0;
        let got = r.layer_area_mm2(k0);
        // Full containment erodes about a one-pixel rim; it must NEVER exceed
        // the true area, which is the direction the gate depends on.
        assert!(got <= ideal, "{got} <= {ideal}");
        assert!(got > ideal * 0.97, "{got} vs {ideal}");
    }

    #[test]
    fn a_cube_mesh_and_the_equivalent_box_raster_agree_closely() {
        let s = cube_soup(4.0);
        let m = crate::mesh::Mesh::weld_exact(&s);
        assert!(m.closure.is_closed());
        let r = RunRaster::from_mesh(&m, grid(), &Budget::default()).expect("raster");
        let (k0, _) = r.layer_range().expect("layers");
        let got = r.layer_area_mm2(k0);
        let ideal = 16.0;
        assert!(got <= ideal + 1e-9, "{got} <= {ideal}");
        assert!(got > ideal * 0.98, "{got} vs {ideal}");
        // Volume: 4x4x4 = 64 mm^3, eroded by the rim on every layer.
        let vol = r.volume_mm3();
        assert!(vol <= 64.0 + 1e-9 && vol > 64.0 * 0.97, "{vol}");
    }

    #[test]
    fn an_open_mesh_is_refused_rather_than_rasterised() {
        let mut s = cube_soup(2.0);
        s.tris.pop();
        let m = crate::mesh::Mesh::weld_exact(&s);
        assert!(matches!(
            RunRaster::from_mesh(&m, grid(), &Budget::default()),
            Err(RasterError::MeshNotClosed)
        ));
    }

    #[test]
    fn boolean_ops_behave_on_overlapping_balls() {
        let a = Primitive::Ball {
            centre: [0.0, 0.0, 1.0],
            r: 0.5,
        };
        let b = Primitive::Ball {
            centre: [0.4, 0.0, 1.0],
            r: 0.5,
        };
        let g = grid();
        let bud = Budget::default();
        let ra = RunRaster::from_primitives(&[a], g, &bud).expect("a");
        let rb = RunRaster::from_primitives(&[b], g, &bud).expect("b");
        let inter = ra.intersect(&rb);
        let uni = ra.union(&rb);
        let diff = ra.difference(&rb);
        assert!(inter.pixel_count > 0);
        assert!(uni.pixel_count > ra.pixel_count);
        assert_eq!(diff.pixel_count + inter.pixel_count, ra.pixel_count);
        assert_eq!(
            uni.pixel_count,
            ra.pixel_count + rb.pixel_count - inter.pixel_count
        );
        // The union of the two must equal the two-primitive raster exactly.
        let both = RunRaster::from_primitives(&[a, b], g, &bud).expect("both");
        assert_eq!(both.pixel_count, uni.pixel_count);
    }

    #[test]
    fn two_separated_balls_make_two_layer_components() {
        let g = grid();
        let a = Primitive::Ball {
            centre: [0.0, 0.0, 1.0],
            r: 0.3,
        };
        let b = Primitive::Ball {
            centre: [5.0, 0.0, 1.0],
            r: 0.3,
        };
        let r = RunRaster::from_primitives(&[a, b], g, &Budget::default()).expect("raster");
        let k = g.layer_at_or_below(1.0);
        assert_eq!(r.layer_components(k).len(), 2);
    }

    #[test]
    fn the_budget_refuses_rather_than_truncating() {
        let p = Primitive::Frustum {
            a: [0.0, 0.0, 0.0],
            ra: 2.0,
            b: [0.0, 0.0, 100.0],
            rb: 2.0,
        };
        let bud = Budget {
            max_layers: 10,
            ..Budget::default()
        };
        assert!(matches!(
            RunRaster::from_primitives(&[p], grid(), &bud),
            Err(RasterError::BudgetExceeded { .. })
        ));
    }

    fn cube_soup(side: f32) -> crate::reader::Soup {
        let s = side;
        let q = [
            [0.0, 0.0, 0.0],
            [s, 0.0, 0.0],
            [s, s, 0.0],
            [0.0, s, 0.0],
            [0.0, 0.0, s],
            [s, 0.0, s],
            [s, s, s],
            [0.0, s, s],
        ];
        let faces: [[usize; 3]; 12] = [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        crate::reader::Soup {
            tris: faces.iter().map(|f| [q[f[0]], q[f[1]], q[f[2]]]).collect(),
            source_offset: 0,
        }
    }
}
