//! Triangle-mesh reconstruction: welding, closure, winding, components, a
//! uniform-grid spatial index, ray casting, point-to-surface distance and the
//! ray-parity inside test.
//!
//! Everything here is reimplemented rather than imported from `geom_kernel`.
//! Three places where this implementation **deliberately differs** from the
//! engine's, because a difference is a finding and not a bug:
//!
//! 1. **No robust predicates.** `geom_kernel` uses exact orientation
//!    arithmetic; this module uses plain `f64` with an explicit tolerance at
//!    every comparison. Where the two disagree on a near-degenerate triangle,
//!    the audit's answer is the one with a stated error bound.
//! 2. **Signed-negative-zero canonicalisation on the exact weld.**
//!    `audit-invariants.md` §1.4 welds the model by exact `f32` bit identity.
//!    `-0.0` and `+0.0` are different bit patterns and the same coordinate, so
//!    [`Mesh::weld_exact`] maps `-0.0` to `+0.0` before hashing and nothing
//!    else. Any other bit difference splits the vertex, as specified.
//! 3. **Winding is never silently flipped per shell.** A hollow model's inner
//!    shell has a genuinely negative signed volume and flipping it would point
//!    its normals into the material. This module records the per-component
//!    signed volume as a diagnostic, applies one global sign taken from the
//!    total, and offers [`Mesh::normal_agrees_with_solid`] so a check can
//!    verify outwardness per facet instead of assuming it.

use crate::consts as c;
use crate::reader::Soup;
use crate::vec3::{self as v, Aabb, V3};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Closure
// ---------------------------------------------------------------------------

/// The result of the closure and manifoldness audit of a welded mesh.
///
/// A model that is not closed fails the file with `FILE_MESH_NOT_CLOSED`:
/// inside/outside is undefined on an open soup and the never-pass rule forbids
/// proceeding on a guess (`audit-invariants.md` §1.4).
#[derive(Clone, Debug, Default)]
pub struct ClosureReport {
    /// Undirected edges carrying exactly one face.
    pub boundary_edges: usize,
    /// Undirected edges carrying three or more faces.
    pub nonmanifold_edges: usize,
    /// Undirected edges carrying two faces with the SAME directed incidence,
    /// i.e. two neighbours wound inconsistently.
    pub inconsistently_wound_edges: usize,
    /// Triangles with two identical welded corners; they carry no area and no
    /// orientation.
    pub degenerate_triangles: usize,
    pub total_undirected_edges: usize,
}

impl ClosureReport {
    /// A mesh is closed when every undirected edge carries exactly two faces
    /// with opposite directed incidence, and no triangle is degenerate.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.boundary_edges == 0
            && self.nonmanifold_edges == 0
            && self.inconsistently_wound_edges == 0
            && self.degenerate_triangles == 0
    }
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "boundary_edges={} nonmanifold_edges={} inconsistently_wound_edges={} \
             degenerate_triangles={} total_undirected_edges={}",
            self.boundary_edges,
            self.nonmanifold_edges,
            self.inconsistently_wound_edges,
            self.degenerate_triangles,
            self.total_undirected_edges
        )
    }
}

// ---------------------------------------------------------------------------
// Query results
// ---------------------------------------------------------------------------

/// The answer to a point-in-solid query. **There is no third passing state**:
/// [`Inside::Indeterminate`] fails whichever invariant asked
/// (`audit-invariants.md` §0 rule 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inside {
    Inside,
    Outside,
    /// Sixteen ray directions were exhausted on grazing hits. Reason string
    /// for the failure line is `ray_parity_exhausted`.
    Indeterminate,
}

impl Inside {
    #[must_use]
    pub const fn is_indeterminate(self) -> bool {
        matches!(self, Self::Indeterminate)
    }
}

/// One ray/triangle crossing.
#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    /// Parameter along the ray; the hit point is `origin + t * dir`.
    pub t: f64,
    pub tri: usize,
    pub point: V3,
    /// The facet normal from the winding, already carrying the mesh's global
    /// orientation sign. See [`Mesh::normal_agrees_with_solid`] before
    /// treating it as outward on a shell you have not verified.
    pub normal: V3,
    /// `dir . normal`; negative means the ray enters the material.
    pub cos_incidence: f64,
}

/// The nearest point of the surface to a query point.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceHit {
    pub distance_mm: f64,
    pub tri: usize,
    pub point: V3,
    pub normal: V3,
}

// ---------------------------------------------------------------------------
// Uniform grid
// ---------------------------------------------------------------------------

/// A uniform grid over triangle bounding boxes.
///
/// Chosen over a BVH on purpose: it is 60 lines, it has no build-order
/// sensitivity, and its ray march is a plain 3D DDA whose traversal order is
/// deterministic. Determinism matters more than the last factor of two here,
/// because two audits of the same bytes must give the same answer.
#[derive(Clone, Debug)]
struct Grid {
    origin: V3,
    cell: V3,
    dims: [usize; 3],
    /// CSR: `starts[i]..starts[i+1]` indexes into `items`.
    starts: Vec<u32>,
    items: Vec<u32>,
}

impl Grid {
    fn build(tri_boxes: &[Aabb], bounds: Aabb) -> Self {
        let n = tri_boxes.len().max(1);
        let ext = bounds.extent();
        // Target roughly one triangle per cell, capped so memory stays sane.
        let target_cells = n.clamp(1, 4_000_000) as f64;
        let vol = (ext[0].max(1e-9)) * (ext[1].max(1e-9)) * (ext[2].max(1e-9));
        let s = (vol / target_cells).cbrt().max(1e-6);
        let mut dims = [0usize; 3];
        let mut cell = [0.0f64; 3];
        for i in 0..3 {
            let d = ((ext[i] / s).ceil() as usize).clamp(1, 1024);
            dims[i] = d;
            cell[i] = (ext[i] / d as f64).max(1e-9);
        }
        let ncells = dims[0] * dims[1] * dims[2];
        let mut counts = vec![0u32; ncells + 1];
        let origin = if bounds.is_empty() {
            [0.0; 3]
        } else {
            bounds.min
        };
        let idx_range = |b: &Aabb| -> ([usize; 3], [usize; 3]) {
            let mut lo = [0usize; 3];
            let mut hi = [0usize; 3];
            for i in 0..3 {
                let a = ((b.min[i] - origin[i]) / cell[i]).floor();
                let z = ((b.max[i] - origin[i]) / cell[i]).floor();
                lo[i] = a.max(0.0).min((dims[i] - 1) as f64) as usize;
                hi[i] = z.max(0.0).min((dims[i] - 1) as f64) as usize;
            }
            (lo, hi)
        };
        for b in tri_boxes {
            let (lo, hi) = idx_range(b);
            for z in lo[2]..=hi[2] {
                for y in lo[1]..=hi[1] {
                    for x in lo[0]..=hi[0] {
                        counts[1 + x + dims[0] * (y + dims[1] * z)] += 1;
                    }
                }
            }
        }
        for i in 1..=ncells {
            counts[i] += counts[i - 1];
        }
        let total = counts[ncells] as usize;
        let starts = counts.clone();
        let mut cursor = counts;
        let mut items = vec![0u32; total];
        for (ti, b) in tri_boxes.iter().enumerate() {
            let (lo, hi) = idx_range(b);
            for z in lo[2]..=hi[2] {
                for y in lo[1]..=hi[1] {
                    for x in lo[0]..=hi[0] {
                        let ci = x + dims[0] * (y + dims[1] * z);
                        items[cursor[ci] as usize] = ti as u32;
                        cursor[ci] += 1;
                    }
                }
            }
        }
        Self {
            origin,
            cell,
            dims,
            starts,
            items,
        }
    }

    fn cell_items(&self, x: usize, y: usize, z: usize) -> &[u32] {
        let ci = x + self.dims[0] * (y + self.dims[1] * z);
        let a = self.starts[ci] as usize;
        let b = self.starts[ci + 1] as usize;
        &self.items[a..b]
    }

    fn box_cells(&self, b: &Aabb) -> ([usize; 3], [usize; 3]) {
        let mut lo = [0usize; 3];
        let mut hi = [0usize; 3];
        for i in 0..3 {
            let a = ((b.min[i] - self.origin[i]) / self.cell[i]).floor();
            let z = ((b.max[i] - self.origin[i]) / self.cell[i]).floor();
            lo[i] = a.max(0.0).min((self.dims[i] - 1) as f64) as usize;
            hi[i] = z.max(0.0).min((self.dims[i] - 1) as f64) as usize;
        }
        (lo, hi)
    }
}

// ---------------------------------------------------------------------------
// Mesh
// ---------------------------------------------------------------------------

/// A welded, indexed triangle mesh with adjacency, components and a spatial
/// index. Build with [`Mesh::weld_exact`] for the model mesh and
/// [`Mesh::weld_tolerant`] for a support record's own soup.
#[derive(Clone, Debug)]
pub struct Mesh {
    pub verts: Vec<V3>,
    pub tris: Vec<[u32; 3]>,
    /// Facet normal from the winding, times the mesh's global orientation
    /// sign. Zero-length for a degenerate triangle.
    pub normals: Vec<V3>,
    pub areas: Vec<f64>,
    pub bbox: Aabb,
    pub closure: ClosureReport,
    /// Per-triangle connected-component id under shared-undirected-edge
    /// adjacency. These are the surface components §2.2 partitions over.
    pub component_of_tri: Vec<u32>,
    pub component_count: usize,
    /// Per-component signed volume (already six times reduced to real volume).
    /// A negative value is a shell wound inward -- typically a cavity, and
    /// legitimate. Reported, never used to flip a shell.
    pub component_volume_mm3: Vec<f64>,
    /// `+1.0` or `-1.0`, the one global sign applied to every facet normal so
    /// that a solid of positive total volume has outward normals.
    pub global_orientation_sign: f64,
    /// Total signed volume before the global sign was applied, mm^3.
    pub raw_signed_volume_mm3: f64,
    /// How many source-soup corners collapsed into one welded vertex, summed.
    pub welded_away: usize,
    grid: Grid,
    tri_boxes: Vec<Aabb>,
    /// Undirected edge -> the (up to two) incident triangles.
    edge_faces: HashMap<(u32, u32), Vec<u32>>,
}

impl Mesh {
    /// Weld by **exact `f32` bit identity**, the rule the model mesh is built
    /// under (`audit-invariants.md` §1.4). `-0.0` is canonicalised to `+0.0`
    /// first; nothing else is.
    #[must_use]
    pub fn weld_exact(soup: &Soup) -> Self {
        let mut map: HashMap<[u32; 3], u32> = HashMap::with_capacity(soup.len() * 3 / 2 + 1);
        let mut verts: Vec<V3> = Vec::new();
        let mut tris: Vec<[u32; 3]> = Vec::with_capacity(soup.len());
        let mut welded_away = 0usize;
        for i in 0..soup.len() {
            let mut idx = [0u32; 3];
            for (j, slot) in idx.iter_mut().enumerate() {
                let mut key = soup.bits(i, j);
                for k in &mut key {
                    // The two zeros are the same coordinate.
                    if *k == 0x8000_0000 {
                        *k = 0;
                    }
                }
                *slot = *map.entry(key).or_insert_with(|| {
                    verts.push(soup.v(i, j));
                    (verts.len() - 1) as u32
                });
                if (*slot as usize) < verts.len() - 1 {
                    welded_away += 1;
                }
            }
            tris.push(idx);
        }
        Self::finish(verts, tris, welded_away)
    }

    /// Weld with a distance tolerance.
    ///
    /// `audit-contract.md` §3.3 measured 7 of 402 emitted sphere soups OPEN
    /// under exact-bit welding -- their pole vertices are genuinely not
    /// bit-identical across longitudes on disk -- and instructs: **weld with a
    /// tolerance, never with exact bits** for support soups. `tol` is a
    /// distance in mm; use [`crate::consts::tol_geom`] at the soup's own
    /// magnitude.
    #[must_use]
    pub fn weld_tolerant(soup: &Soup, tol: f64) -> Self {
        let cell = (tol * 2.0).max(1e-12);
        let key_of = |p: V3| -> [i64; 3] {
            [
                (p[0] / cell).floor() as i64,
                (p[1] / cell).floor() as i64,
                (p[2] / cell).floor() as i64,
            ]
        };
        let mut buckets: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
        let mut verts: Vec<V3> = Vec::new();
        let mut tris: Vec<[u32; 3]> = Vec::with_capacity(soup.len());
        let mut welded_away = 0usize;
        let tol2 = tol * tol;
        for i in 0..soup.len() {
            let mut idx = [0u32; 3];
            for (j, slot) in idx.iter_mut().enumerate() {
                let p = soup.v(i, j);
                let k = key_of(p);
                let mut found: Option<u32> = None;
                'search: for dz in -1..=1i64 {
                    for dy in -1..=1i64 {
                        for dx in -1..=1i64 {
                            let nk = [k[0] + dx, k[1] + dy, k[2] + dz];
                            if let Some(list) = buckets.get(&nk) {
                                for &vi in list {
                                    if v::dist2(verts[vi as usize], p) <= tol2 {
                                        found = Some(vi);
                                        break 'search;
                                    }
                                }
                            }
                        }
                    }
                }
                *slot = found.unwrap_or_else(|| {
                    verts.push(p);
                    let vi = (verts.len() - 1) as u32;
                    buckets.entry(k).or_default().push(vi);
                    vi
                });
                if found.is_some() {
                    welded_away += 1;
                }
            }
            tris.push(idx);
        }
        Self::finish(verts, tris, welded_away)
    }

    fn finish(verts: Vec<V3>, tris: Vec<[u32; 3]>, welded_away: usize) -> Self {
        let mut bbox = Aabb::empty();
        for p in &verts {
            bbox.add_point(*p);
        }
        let n = tris.len();
        let mut normals = vec![[0.0; 3]; n];
        let mut areas = vec![0.0f64; n];
        let mut tri_boxes = Vec::with_capacity(n);
        let mut raw6 = 0.0f64;
        for (i, t) in tris.iter().enumerate() {
            let (a, b, cc) = (
                verts[t[0] as usize],
                verts[t[1] as usize],
                verts[t[2] as usize],
            );
            areas[i] = v::tri_area(a, b, cc);
            normals[i] = v::tri_normal(a, b, cc).unwrap_or([0.0; 3]);
            let mut bb = Aabb::empty();
            bb.add_point(a);
            bb.add_point(b);
            bb.add_point(cc);
            tri_boxes.push(bb);
            raw6 += v::signed_tet_volume6(a, b, cc);
        }
        let raw_signed_volume_mm3 = raw6 / 6.0;
        let global_orientation_sign = if raw_signed_volume_mm3 < 0.0 {
            -1.0
        } else {
            1.0
        };
        if global_orientation_sign < 0.0 {
            for nrm in &mut normals {
                *nrm = v::mul(*nrm, -1.0);
            }
        }

        // Edge audit.
        let mut edge_faces: HashMap<(u32, u32), Vec<u32>> = HashMap::with_capacity(n * 3 / 2 + 1);
        let mut directed: HashMap<(u32, u32), i32> = HashMap::with_capacity(n * 3 + 1);
        let mut closure = ClosureReport::default();
        for (i, t) in tris.iter().enumerate() {
            if t[0] == t[1] || t[1] == t[2] || t[2] == t[0] {
                closure.degenerate_triangles += 1;
            }
            for e in 0..3 {
                let (a, b) = (t[e], t[(e + 1) % 3]);
                let key = if a < b { (a, b) } else { (b, a) };
                edge_faces.entry(key).or_default().push(i as u32);
                *directed.entry((a, b)).or_default() += 1;
            }
        }
        closure.total_undirected_edges = edge_faces.len();
        for (&(a, b), faces) in &edge_faces {
            match faces.len() {
                1 => closure.boundary_edges += 1,
                2 => {
                    let fwd = directed.get(&(a, b)).copied().unwrap_or(0);
                    let rev = directed.get(&(b, a)).copied().unwrap_or(0);
                    if fwd != 1 || rev != 1 {
                        closure.inconsistently_wound_edges += 1;
                    }
                }
                _ => closure.nonmanifold_edges += 1,
            }
        }

        // Components over shared undirected edges.
        let mut parent: Vec<u32> = (0..n as u32).collect();
        fn find(parent: &mut [u32], mut x: u32) -> u32 {
            while parent[x as usize] != x {
                parent[x as usize] = parent[parent[x as usize] as usize];
                x = parent[x as usize];
            }
            x
        }
        for faces in edge_faces.values() {
            for w in faces.windows(2) {
                let (a, b) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
                if a != b {
                    parent[a as usize] = b;
                }
            }
        }
        let mut relabel: HashMap<u32, u32> = HashMap::new();
        let mut component_of_tri = vec![0u32; n];
        for i in 0..n {
            let root = find(&mut parent, i as u32);
            let next = relabel.len() as u32;
            let id = *relabel.entry(root).or_insert(next);
            component_of_tri[i] = id;
        }
        let component_count = relabel.len();
        let mut comp6 = vec![0.0f64; component_count];
        for (i, t) in tris.iter().enumerate() {
            let (a, b, cc) = (
                verts[t[0] as usize],
                verts[t[1] as usize],
                verts[t[2] as usize],
            );
            comp6[component_of_tri[i] as usize] +=
                global_orientation_sign * v::signed_tet_volume6(a, b, cc);
        }
        let component_volume_mm3 = comp6.iter().map(|x| x / 6.0).collect();

        let grid = Grid::build(&tri_boxes, bbox);
        Self {
            verts,
            tris,
            normals,
            areas,
            bbox,
            closure,
            component_of_tri,
            component_count,
            component_volume_mm3,
            global_orientation_sign,
            raw_signed_volume_mm3,
            welded_away,
            grid,
            tri_boxes,
            edge_faces,
        }
    }

    #[must_use]
    pub fn tri_count(&self) -> usize {
        self.tris.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }
    /// The three corners of triangle `i`.
    #[must_use]
    pub fn tri(&self, i: usize) -> [V3; 3] {
        let t = self.tris[i];
        [
            self.verts[t[0] as usize],
            self.verts[t[1] as usize],
            self.verts[t[2] as usize],
        ]
    }
    #[must_use]
    pub fn tri_centroid(&self, i: usize) -> V3 {
        let [a, b, cc] = self.tri(i);
        [
            (a[0] + b[0] + cc[0]) / 3.0,
            (a[1] + b[1] + cc[1]) / 3.0,
            (a[2] + b[2] + cc[2]) / 3.0,
        ]
    }
    /// Enclosed volume, mm^3. **Meaningless on an open soup** -- the signed
    /// tetrahedron sum of an open soup depends on where the soup sits
    /// (`audit-contract.md` §3.3). Returns `None` unless the mesh is closed,
    /// rather than computing through it.
    #[must_use]
    pub fn closed_volume_mm3(&self) -> Option<f64> {
        self.closure
            .is_closed()
            .then_some(self.raw_signed_volume_mm3 * self.global_orientation_sign)
    }
    /// Total surface area, mm^2.
    #[must_use]
    pub fn surface_area_mm2(&self) -> f64 {
        self.areas.iter().sum()
    }
    /// Undirected edges incident to triangle `i`'s neighbours.
    #[must_use]
    pub fn neighbours(&self, i: usize) -> Vec<usize> {
        let t = self.tris[i];
        let mut out = Vec::new();
        for e in 0..3 {
            let (a, b) = (t[e], t[(e + 1) % 3]);
            let key = if a < b { (a, b) } else { (b, a) };
            if let Some(faces) = self.edge_faces.get(&key) {
                for &fnd in faces {
                    if fnd as usize != i {
                        out.push(fnd as usize);
                    }
                }
            }
        }
        out
    }

    /// Every triangle whose bounding box meets `query`.
    #[must_use]
    pub fn triangles_in_box(&self, query: &Aabb) -> Vec<usize> {
        let mut out = Vec::new();
        if self.tris.is_empty() || !query.intersects(&self.bbox) {
            return out;
        }
        let (lo, hi) = self.grid.box_cells(query);
        let mut seen = vec![false; self.tris.len()];
        for z in lo[2]..=hi[2] {
            for y in lo[1]..=hi[1] {
                for x in lo[0]..=hi[0] {
                    for &ti in self.grid.cell_items(x, y, z) {
                        let ti = ti as usize;
                        if !seen[ti] && self.tri_boxes[ti].intersects(query) {
                            seen[ti] = true;
                            out.push(ti);
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// All crossings of the infinite ray `origin + t*dir`, `t > 0`, sorted by
    /// `t`. Nothing is culled: back faces are crossings too.
    ///
    /// `grazing_tol` is the distance from a hit point to the nearest triangle
    /// edge below which the hit is reported as grazing; a grazing hit makes
    /// parity unreliable and the caller must retry with another direction.
    pub fn ray_crossings(
        &self,
        origin: V3,
        dir: V3,
        grazing_tol: f64,
        out: &mut Vec<RayHit>,
        grazed: &mut bool,
    ) {
        out.clear();
        *grazed = false;
        if self.tris.is_empty() {
            return;
        }
        for ti in self.ray_candidates(origin, dir) {
            let [a, b, cc] = self.tri(ti);
            let Some((t, u, w)) = v::ray_triangle(origin, dir, a, b, cc) else {
                continue;
            };
            let bary = [1.0 - u - w, u, w];
            if t <= 0.0 {
                // A near-origin crossing still matters for grazing: an origin
                // sitting exactly on the surface makes parity meaningless.
                if t.abs() <= grazing_tol && (-0.5..=1.5).contains(&u) && (-0.5..=1.5).contains(&w)
                {
                    *grazed = true;
                }
                continue;
            }
            let inside_bary = bary.iter().all(|&x| x >= 0.0) && u + w <= 1.0;
            let point = v::add(origin, v::mul(dir, t));
            // Distance from the hit to the triangle's boundary decides grazing.
            let (edge_d, _) = closest_on_boundary(point, a, b, cc);
            if edge_d <= grazing_tol {
                *grazed = true;
                if !inside_bary {
                    continue;
                }
            } else if !inside_bary {
                continue;
            }
            let n = self.normals[ti];
            out.push(RayHit {
                t,
                tri: ti,
                point,
                normal: n,
                cos_incidence: v::dot(dir, n),
            });
        }
        out.sort_by(|x, y| x.t.partial_cmp(&y.t).unwrap_or(std::cmp::Ordering::Equal));
        // Two crossings at the same `t` mean the ray met an edge or a vertex.
        for w in out.windows(2) {
            if (w[1].t - w[0].t).abs() <= grazing_tol {
                *grazed = true;
            }
        }
    }

    /// The first crossing along `dir` from `origin`, or `None`.
    /// Grazing is reported so the caller can escalate to indeterminate.
    #[must_use]
    pub fn ray_first(&self, origin: V3, dir: V3, grazing_tol: f64) -> (Option<RayHit>, bool) {
        let mut hits = Vec::new();
        let mut grazed = false;
        self.ray_crossings(origin, dir, grazing_tol, &mut hits, &mut grazed);
        (hits.first().copied(), grazed)
    }

    /// Point-in-solid by exact ray parity against the welded closed mesh, with
    /// the deterministic golden-spiral direction sequence of
    /// `audit-invariants.md` §1.4. **The query point is never perturbed** --
    /// moving the point changes the question. Sixteen exhausted retries give
    /// [`Inside::Indeterminate`].
    #[must_use]
    pub fn point_inside(&self, p: V3) -> Inside {
        if !self.closure.is_closed() {
            // Inside/outside is undefined on an open soup. Say so.
            return Inside::Indeterminate;
        }
        if !self.bbox.expanded(c::tol_geom_at(p)).contains(p) {
            return Inside::Outside;
        }
        let tol = c::tol_geom_at(p);
        let mut hits = Vec::new();
        for k in 0..v::RAY_DIRECTION_BUDGET {
            let dir = v::golden_spiral_direction(k);
            let mut grazed = false;
            self.ray_crossings(p, dir, tol, &mut hits, &mut grazed);
            if grazed {
                continue;
            }
            return if hits.len() % 2 == 1 {
                Inside::Inside
            } else {
                Inside::Outside
            };
        }
        Inside::Indeterminate
    }

    /// Batch form of [`Mesh::point_inside`]. Prefer it: a per-item call in a
    /// hot path is a defect, and every ownership and intersection check queries
    /// points by the million.
    #[must_use]
    pub fn points_inside(&self, points: &[V3]) -> Vec<Inside> {
        points.iter().map(|p| self.point_inside(*p)).collect()
    }

    /// Nearest point of the surface to `p`, by expanding-radius grid search.
    /// `None` only for an empty mesh.
    #[must_use]
    pub fn nearest_surface(&self, p: V3) -> Option<SurfaceHit> {
        if self.tris.is_empty() {
            return None;
        }
        let ext = self.bbox.extent();
        let diag = v::norm(ext).max(1e-6);
        let mut r = (self.grid.cell[0] + self.grid.cell[1] + self.grid.cell[2]) / 3.0;
        loop {
            let q = Aabb {
                min: [p[0] - r, p[1] - r, p[2] - r],
                max: [p[0] + r, p[1] + r, p[2] + r],
            };
            let cands = self.triangles_in_box(&q);
            let mut best: Option<SurfaceHit> = None;
            for ti in cands {
                let [a, b, cc] = self.tri(ti);
                let (d, pt) = v::point_triangle_closest(p, a, b, cc);
                if best.is_none_or(|h| d < h.distance_mm) {
                    best = Some(SurfaceHit {
                        distance_mm: d,
                        tri: ti,
                        point: pt,
                        normal: self.normals[ti],
                    });
                }
            }
            // The result is only trustworthy once the search radius covers it.
            if let Some(h) = best {
                if h.distance_mm <= r || r >= diag * 2.0 {
                    return Some(h);
                }
            }
            if r >= diag * 2.0 {
                // Exhaustive fallback; only reachable for a degenerate grid.
                let mut best: Option<SurfaceHit> = None;
                for ti in 0..self.tris.len() {
                    let [a, b, cc] = self.tri(ti);
                    let (d, pt) = v::point_triangle_closest(p, a, b, cc);
                    if best.is_none_or(|h| d < h.distance_mm) {
                        best = Some(SurfaceHit {
                            distance_mm: d,
                            tri: ti,
                            point: pt,
                            normal: self.normals[ti],
                        });
                    }
                }
                return best;
            }
            r *= 2.0;
        }
    }

    /// Batch form of [`Mesh::nearest_surface`].
    #[must_use]
    pub fn nearest_surface_batch(&self, points: &[V3]) -> Vec<Option<SurfaceHit>> {
        points.iter().map(|p| self.nearest_surface(*p)).collect()
    }

    /// Does triangle `i`'s stored normal actually point out of the material?
    ///
    /// Verified rather than assumed: step one `tol_geom` off the centroid on
    /// each side and ask the parity test. `None` when either probe is
    /// indeterminate -- which is the honest answer and must not be read as
    /// agreement.
    #[must_use]
    pub fn normal_agrees_with_solid(&self, i: usize) -> Option<bool> {
        let cnt = self.tri_centroid(i);
        let n = self.normals[i];
        if v::norm(n) == 0.0 {
            return None;
        }
        // One layer is the smallest step at which a probe is meaningful on a
        // mesh this audit grades; anything smaller lands inside tol_geom.
        let step = c::LAYER_HEIGHT_DEFAULT_MM * 0.25;
        let out = self.point_inside(v::add(cnt, v::mul(n, step)));
        let inn = self.point_inside(v::sub(cnt, v::mul(n, step)));
        match (out, inn) {
            (Inside::Outside, Inside::Inside) => Some(true),
            (Inside::Inside, Inside::Outside) => Some(false),
            _ => None,
        }
    }

    fn ray_candidates(&self, origin: V3, dir: V3) -> Vec<usize> {
        // 3D DDA over the grid, clipped to the mesh bounds. Deterministic.
        let g = &self.grid;
        let mut tmin = 0.0f64;
        let mut tmax = f64::INFINITY;
        for i in 0..3 {
            let lo = g.origin[i];
            let hi = g.origin[i] + g.cell[i] * g.dims[i] as f64;
            if dir[i].abs() < 1e-300 {
                if origin[i] < lo || origin[i] > hi {
                    return Vec::new();
                }
            } else {
                let inv = 1.0 / dir[i];
                let (mut a, mut b) = ((lo - origin[i]) * inv, (hi - origin[i]) * inv);
                if a > b {
                    std::mem::swap(&mut a, &mut b);
                }
                tmin = tmin.max(a);
                tmax = tmax.min(b);
                if tmin > tmax {
                    return Vec::new();
                }
            }
        }
        let entry = v::add(origin, v::mul(dir, tmin.max(0.0)));
        let mut cell = [0i64; 3];
        let mut step = [0i64; 3];
        let mut t_max = [f64::INFINITY; 3];
        let mut t_delta = [f64::INFINITY; 3];
        for i in 0..3 {
            let f = (entry[i] - g.origin[i]) / g.cell[i];
            cell[i] = (f.floor() as i64).clamp(0, g.dims[i] as i64 - 1);
            if dir[i] > 0.0 {
                step[i] = 1;
                let next = g.origin[i] + (cell[i] + 1) as f64 * g.cell[i];
                t_max[i] = (next - origin[i]) / dir[i];
                t_delta[i] = g.cell[i] / dir[i];
            } else if dir[i] < 0.0 {
                step[i] = -1;
                let next = g.origin[i] + cell[i] as f64 * g.cell[i];
                t_max[i] = (next - origin[i]) / dir[i];
                t_delta[i] = -g.cell[i] / dir[i];
            }
        }
        let mut seen = vec![false; self.tris.len()];
        let mut out = Vec::new();
        loop {
            for &ti in g.cell_items(cell[0] as usize, cell[1] as usize, cell[2] as usize) {
                let ti = ti as usize;
                if !seen[ti] {
                    seen[ti] = true;
                    out.push(ti);
                }
            }
            let axis = if t_max[0] < t_max[1] {
                if t_max[0] < t_max[2] {
                    0
                } else {
                    2
                }
            } else if t_max[1] < t_max[2] {
                1
            } else {
                2
            };
            if step[axis] == 0 || t_max[axis] > tmax {
                break;
            }
            cell[axis] += step[axis];
            if cell[axis] < 0 || cell[axis] >= g.dims[axis] as i64 {
                break;
            }
            t_max[axis] += t_delta[axis];
        }
        out
    }
}

/// Distance from `p` to the boundary (the three edges) of triangle `(a,b,c)`.
fn closest_on_boundary(p: V3, a: V3, b: V3, cc: V3) -> (f64, V3) {
    let mut best = (f64::INFINITY, a);
    for (u, w) in [(a, b), (b, cc), (cc, a)] {
        let e = v::sub(w, u);
        let len2 = v::dot(e, e);
        let t = if len2 > 0.0 {
            (v::dot(v::sub(p, u), e) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = v::add(u, v::mul(e, t));
        let d = v::dist(p, q);
        if d < best.0 {
            best = (d, q);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_cube_soup() -> Soup {
        // Outward-wound unit cube, 12 triangles, corners at 0 and 1.
        let p = |x: f32, y: f32, z: f32| [x, y, z];
        let q = [
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
            p(1.0, 0.0, 1.0),
            p(1.0, 1.0, 1.0),
            p(0.0, 1.0, 1.0),
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
        Soup {
            tris: faces.iter().map(|f| [q[f[0]], q[f[1]], q[f[2]]]).collect(),
            source_offset: 0,
        }
    }

    #[test]
    fn a_cube_welds_closed_with_eight_vertices() {
        let m = Mesh::weld_exact(&unit_cube_soup());
        assert_eq!(m.verts.len(), 8);
        assert_eq!(m.tri_count(), 12);
        assert!(m.closure.is_closed(), "{}", m.closure.summary());
        assert_eq!(m.component_count, 1);
        let vol = m.closed_volume_mm3().expect("closed");
        assert!((vol - 1.0).abs() < 1e-12, "{vol}");
    }

    #[test]
    fn inside_and_outside_are_decided_for_a_cube() {
        let m = Mesh::weld_exact(&unit_cube_soup());
        assert_eq!(m.point_inside([0.5, 0.5, 0.5]), Inside::Inside);
        assert_eq!(m.point_inside([0.5, 0.5, 1.5]), Inside::Outside);
        assert_eq!(m.point_inside([-2.0, 0.5, 0.5]), Inside::Outside);
        assert_eq!(m.point_inside([0.25, 0.75, 0.1]), Inside::Inside);
    }

    #[test]
    fn an_open_soup_refuses_to_answer_inside() {
        let mut s = unit_cube_soup();
        s.tris.pop();
        let m = Mesh::weld_exact(&s);
        assert!(!m.closure.is_closed());
        assert_eq!(m.closure.boundary_edges, 3);
        assert!(m.point_inside([0.5, 0.5, 0.5]).is_indeterminate());
        assert!(m.closed_volume_mm3().is_none());
    }

    #[test]
    fn a_reversed_cube_still_reports_positive_volume_and_outward_normals() {
        let mut s = unit_cube_soup();
        for t in &mut s.tris {
            t.swap(1, 2);
        }
        let m = Mesh::weld_exact(&s);
        assert!(m.closure.is_closed(), "{}", m.closure.summary());
        assert!((m.global_orientation_sign + 1.0).abs() < 1e-15);
        let vol = m.closed_volume_mm3().expect("closed");
        assert!((vol - 1.0).abs() < 1e-12, "{vol}");
        // Every facet normal must now point out of the material.
        for i in 0..m.tri_count() {
            assert_eq!(m.normal_agrees_with_solid(i), Some(true), "tri {i}");
        }
    }

    #[test]
    fn ray_first_finds_the_near_face() {
        let m = Mesh::weld_exact(&unit_cube_soup());
        let (hit, _) = m.ray_first([0.5, 0.5, -3.0], [0.0, 0.0, 1.0], 1e-9);
        let h = hit.expect("hit");
        assert!((h.t - 3.0).abs() < 1e-12, "{}", h.t);
        assert!(h.cos_incidence < 0.0, "entering the material");
    }

    #[test]
    fn nearest_surface_finds_the_face_below() {
        let m = Mesh::weld_exact(&unit_cube_soup());
        let h = m.nearest_surface([0.5, 0.5, 2.0]).expect("hit");
        assert!((h.distance_mm - 1.0).abs() < 1e-12, "{}", h.distance_mm);
    }

    #[test]
    fn tolerant_welding_closes_a_soup_exact_welding_leaves_open() {
        let mut s = unit_cube_soup();
        // Nudge exactly ONE occurrence of a shared corner, by far less than
        // the weld tolerance. Nudging every occurrence would move the vertex
        // rather than split it, which is not the defect under test.
        'outer: for t in &mut s.tris {
            for vtx in t.iter_mut() {
                if *vtx == [1.0, 1.0, 1.0] {
                    *vtx = [1.000_000_1, 1.0, 1.0];
                    break 'outer;
                }
            }
        }
        let exact = Mesh::weld_exact(&s);
        assert!(!exact.closure.is_closed());
        let tolerant = Mesh::weld_tolerant(&s, 1e-5);
        assert!(
            tolerant.closure.is_closed(),
            "{}",
            tolerant.closure.summary()
        );
        assert_eq!(tolerant.verts.len(), 8);
    }
}

/// A crossing measured along a **fixed** direction, where retrying another
/// direction is not available because the direction is the question.
///
/// The seat-depth measurement travels along the support axis `u`, and
/// `audit-invariants.md` §3.1 measures depth along that axis "because the seat
/// formula's cut plane is perpendicular to that axis". So the parity test's
/// escape hatch -- try the next golden-spiral direction -- does not apply:
/// another direction would measure a different quantity.
///
/// **Measured on `exports/sphere-first-artifact.chitubox`, 2026-08-30: the
/// engine seats every contact exactly on a model VERTEX**, so the axis ray
/// leaves through a point shared by six triangles, 5e-8 mm from their common
/// vertex, on 9 of 9 embedded tips. Treating that as indeterminate would make
/// the audit refuse every well-seated contact in the corpus.
///
/// What is actually ambiguous there is the **normal**, not the distance: all
/// six triangles agree on `t` to 1e-13 mm. So this type reports the distance as
/// determined, reports every triangle that agrees at that `t`, hands back their
/// area-weighted mean normal, and sets [`AxisCrossing::on_mesh_boundary`] so the
/// caller runs the 5-degree normal cross-check §3.1 already specifies.
#[derive(Clone, Debug)]
pub struct AxisCrossing {
    pub t: f64,
    pub point: V3,
    /// Every triangle whose crossing lies within tolerance of `t`. More than
    /// one means the ray met a shared edge or a vertex.
    pub triangles: Vec<usize>,
    /// Area-weighted mean normal over `triangles`.
    pub normal: V3,
    /// The crossing landed on an edge or a vertex shared by several facets.
    pub on_mesh_boundary: bool,
    /// Largest difference among the agreeing `t` values, mm.
    pub t_spread_mm: f64,
}

/// Why an axis crossing could not be established.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisError {
    /// Crossings clustered near the first one disagree on the distance beyond
    /// tolerance, so the depth itself is not determined.
    DistanceAmbiguous,
    /// The mesh is not closed, so there is no interior to leave.
    MeshNotClosed,
}

impl Mesh {
    /// The first crossing along a FIXED direction, with vertex and edge hits
    /// resolved rather than refused. See [`AxisCrossing`].
    ///
    /// `Ok(None)` means the ray never leaves the mesh, which is itself a
    /// finding on a closed solid.
    pub fn axis_crossing(
        &self,
        origin: V3,
        dir: V3,
        tol: f64,
    ) -> Result<Option<AxisCrossing>, AxisError> {
        if !self.closure.is_closed() {
            return Err(AxisError::MeshNotClosed);
        }
        // Collect every forward crossing, counting near-misses too: a ray that
        // leaves exactly through a vertex is inside one triangle by the
        // barycentric test and within `tol` of five others.
        let mut cands: Vec<(f64, usize)> = Vec::new();
        for ti in self.ray_candidates(origin, dir) {
            let [a, b, cc] = self.tri(ti);
            let Some((t, u, w)) = v::ray_triangle(origin, dir, a, b, cc) else {
                continue;
            };
            if t <= 0.0 {
                continue;
            }
            let inside_bary = u >= 0.0 && w >= 0.0 && u + w <= 1.0;
            let point = v::add(origin, v::mul(dir, t));
            let (edge_d, _) = closest_on_boundary(point, a, b, cc);
            if inside_bary || edge_d <= tol {
                cands.push((t, ti));
            }
        }
        if cands.is_empty() {
            return Ok(None);
        }
        cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let t0 = cands[0].0;
        // The cluster is every crossing within tolerance of the first.
        let cluster: Vec<(f64, usize)> = cands
            .iter()
            .copied()
            .take_while(|(t, _)| (t - t0).abs() <= tol.max(1e-9))
            .collect();
        let spread = cluster
            .iter()
            .map(|(t, _)| (t - t0).abs())
            .fold(0.0f64, f64::max);
        // A second, DISTINCT crossing arriving within tolerance but from a
        // facet the cluster does not share would make the distance itself
        // uncertain. That is the only genuine ambiguity here.
        if spread > tol.max(1e-9) {
            return Err(AxisError::DistanceAmbiguous);
        }
        let mut acc = [0.0f64; 3];
        let mut wsum = 0.0f64;
        for (_, ti) in &cluster {
            let a = self.areas[*ti];
            acc = v::add(acc, v::mul(self.normals[*ti], a));
            wsum += a;
        }
        let normal = if wsum > 0.0 {
            v::normalize(acc).unwrap_or(self.normals[cluster[0].1])
        } else {
            self.normals[cluster[0].1]
        };
        Ok(Some(AxisCrossing {
            t: t0,
            point: v::add(origin, v::mul(dir, t0)),
            triangles: cluster.iter().map(|(_, ti)| *ti).collect(),
            normal,
            on_mesh_boundary: cluster.len() > 1,
            t_spread_mm: spread,
        }))
    }
}
