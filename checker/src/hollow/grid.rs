//! Compact fixed-resolution material raster and directed floods. Triangle/cell work and
//! storage are independently capped; a large input is refused, never coarsened to pass.

use super::{CELL_MM, triangle_cell};
use crate::scene::Scene;
use crate::tri::{Bvh, RayHit, Tri, ray_tri, tri_box};
use crate::v3::{Aabb, V};
use std::collections::VecDeque;
use std::ops::RangeInclusive;

pub(super) const FREE: u8 = 0;
pub(super) const MATERIAL: u8 = 1;
pub(super) const UNKNOWN: u8 = 2;
// Covers the measured cylinder fixture (about 38 million cells with its assembly),
// while bounding all dense fields, component lists and flood queues below 2 GiB.
const MAX_CELLS: usize = 64_000_000;
const MAX_TESTS: usize = 120_000_000;

pub(super) struct Grid {
    pub origin: V,
    pub n: [usize; 3],
    pub model: Vec<u8>,
    pub assembly: Vec<u8>,
    pub labels: Vec<u32>,
    tests: usize,
}

/// Lower envelope of parabolas f[q] + (x-q)^2 (exact integer squared distances).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "squared grid distances capped below u32 maximum"
)]
fn distance_line(f: &[u32], out: &mut [u32], sites: &mut [usize], cuts: &mut [f64]) {
    let mut k = 0;
    sites[0] = 0;
    cuts[0] = f64::NEG_INFINITY;
    cuts[1] = f64::INFINITY;
    for q in 1..f.len() {
        let intersection = |v: usize| {
            (f64::from(f[q]) + (q as f64).powi(2) - f64::from(f[v]) - (v as f64).powi(2))
                / (2.0 * (q - v) as f64)
        };
        let mut s = intersection(sites[k]);
        while s <= cuts[k] && k > 0 {
            k -= 1;
            s = intersection(sites[k]);
        }
        k += 1;
        sites[k] = q;
        cuts[k] = s;
        cuts[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, value) in out.iter_mut().enumerate() {
        while cuts[k + 1] < q as f64 {
            k += 1;
        }
        *value = (f64::from(f[sites[k]]) + (q as f64 - sites[k] as f64).powi(2))
            .min(1_000_000_000.0) as u32;
    }
}

impl std::fmt::Debug for Grid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grid")
            .field("origin", &self.origin)
            .field("n", &self.n)
            .field("tests", &self.tests)
            .finish_non_exhaustive()
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bounded grid indices"
)]
impl Grid {
    pub fn build(scene: &Scene) -> Result<Self, String> {
        let bounds = Aabb::of_points(scene.model.iter().flatten().copied());
        let mut extent = bounds;
        for p in &scene.parts {
            extent.grow_box(&p.bbox);
        }
        let origin = extent
            .lo
            .map(|x| (x / CELL_MM).floor() * CELL_MM - 3.0 * CELL_MM);
        let n = std::array::from_fn(|k| ((extent.hi[k] - origin[k]) / CELL_MM).ceil() as usize + 3);
        let cells = n
            .iter()
            .try_fold(1usize, |a, b| a.checked_mul(*b))
            .ok_or("voxel size overflow")?;
        if cells > MAX_CELLS {
            return Err(format!(
                "hollow grid needs {cells} cells, budget is {MAX_CELLS}"
            ));
        }
        let mut g = Self {
            origin,
            n,
            model: vec![FREE; cells],
            assembly: vec![],
            labels: vec![0; cells],
            tests: 0,
        };
        g.model = g.raster(&scene.model, &scene.model_bvh)?;
        Ok(g)
    }

    pub fn index(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.n[1] + y) * self.n[0] + x
    }
    pub fn xyz(&self, i: usize) -> [usize; 3] {
        [
            i % self.n[0],
            i / self.n[0] % self.n[1],
            i / (self.n[0] * self.n[1]),
        ]
    }
    pub fn point(&self, i: usize) -> V {
        let p = self.xyz(i);
        std::array::from_fn(|k| self.origin[k] + (p[k] as f64 + 0.5) * CELL_MM)
    }
    fn cell_box(&self, i: usize) -> Aabb {
        let p = self.point(i);
        Aabb {
            lo: p.map(|x| x - CELL_MM * 0.5),
            hi: p.map(|x| x + CELL_MM * 0.5),
        }
    }
    fn range(&self, bb: &Aabb) -> [RangeInclusive<usize>; 3] {
        std::array::from_fn(|k| {
            let cell = |x: f64| {
                (((x - self.origin[k]) / CELL_MM).floor().max(0.0) as usize).min(self.n[k] - 1)
            };
            cell(bb.lo[k] - 1e-9)..=cell(bb.hi[k] + 1e-9)
        })
    }
    pub fn covered_box(&self, bb: &Aabb) -> Option<[RangeInclusive<usize>; 3]> {
        let end = std::array::from_fn::<_, 3, _>(|k| self.origin[k] + self.n[k] as f64 * CELL_MM);
        (0..3)
            .all(|k| bb.lo[k] >= self.origin[k] && bb.hi[k] <= end[k])
            .then(|| self.range(bb))
    }

    fn raster(&mut self, tris: &[Tri], bvh: &Bvh) -> Result<Vec<u8>, String> {
        let mut cells = vec![FREE; self.model.len()];
        // Triangle-major batches mark every surface cell, including sub-cell pockets.
        for t in tris {
            let r = self.range(&tri_box(t));
            let work = r.iter().map(|r| r.end() - r.start() + 1).product::<usize>();
            self.tests = self
                .tests
                .checked_add(work)
                .ok_or("hollow raster work overflow")?;
            if self.tests > MAX_TESTS {
                return Err("hollow raster exceeds 120 million triangle/cell tests".to_owned());
            }
            for z in r[2].clone() {
                for y in r[1].clone() {
                    for x in r[0].clone() {
                        let i = self.index(x, y, z);
                        if cells[i] != UNKNOWN && triangle_cell(t, &self.cell_box(i)) {
                            cells[i] = UNKNOWN;
                        }
                    }
                }
            }
        }
        // One BVH ray per XY column, not per voxel. A tiny in-cell offset avoids regular
        // mesh diagonals. Since all cut cells are unknown, the offset cannot move a known
        // cell across a boundary. Odd/ambiguous ray columns stay unknown.
        let mut cand = Vec::new();
        let mut hits = Vec::new();
        for y in 0..self.n[1] {
            for x in 0..self.n[0] {
                let mut o = self.point(self.index(x, y, 0));
                o[0] += 1.234_567e-7;
                o[1] += 3.765_431e-7;
                cand.clear();
                hits.clear();
                bvh.query_ray(o, [0.0, 0.0, 1.0], &mut cand);
                let mut clean = true;
                for &ti in &cand {
                    match ray_tri(o, [0.0, 0.0, 1.0], &tris[ti as usize], 1e-12) {
                        RayHit::Hit(t) => hits.push(o[2] + t),
                        RayHit::Ambiguous => clean = false,
                        RayHit::Miss => {}
                    }
                }
                hits.sort_by(f64::total_cmp);
                clean &= hits.len().is_multiple_of(2);
                let mut crossing = 0;
                for z in 0..self.n[2] {
                    let i = self.index(x, y, z);
                    if cells[i] == UNKNOWN {
                        continue;
                    }
                    if !clean {
                        cells[i] = UNKNOWN;
                        continue;
                    }
                    let zz = self.point(i)[2];
                    while crossing < hits.len() && hits[crossing] < zz {
                        crossing += 1;
                    }
                    if !crossing.is_multiple_of(2) {
                        cells[i] = MATERIAL;
                    }
                }
            }
        }
        Ok(cells)
    }

    pub fn assembly(&mut self, scene: &Scene) -> Result<(), String> {
        self.assembly = self.model.clone();
        for p in &scene.parts {
            if !p.is_valid() {
                return Err(format!(
                    "record {} cannot be measured as a closed native support solid",
                    p.rec
                ));
            }
            let bvh = Bvh::build(&p.tris.iter().map(tri_box).collect::<Vec<_>>());
            let origin = p.bbox.lo.map(|x| (x / CELL_MM).floor() * CELL_MM - CELL_MM);
            let n =
                std::array::from_fn(|k| ((p.bbox.hi[k] - origin[k]) / CELL_MM).ceil() as usize + 2);
            let count = n.iter().product();
            let mut local = Self {
                origin,
                n,
                model: vec![FREE; count],
                assembly: vec![],
                labels: vec![],
                tests: self.tests,
            };
            let part = local.raster(&p.tris, &bvh)?;
            self.tests = local.tests;
            for (i, src) in part.into_iter().enumerate().filter(|(_, c)| *c != FREE) {
                let point = local.point(i);
                let xyz: [usize; 3] = std::array::from_fn(|k| {
                    ((point[k] - self.origin[k]) / CELL_MM).floor().max(0.0) as usize
                });
                if (0..3).any(|k| xyz[k] >= self.n[k]) {
                    continue;
                }
                let j = self.index(xyz[0], xyz[1], xyz[2]);
                let dst = &mut self.assembly[j];
                if src == MATERIAL || *dst == MATERIAL {
                    *dst = MATERIAL;
                } else if src == UNKNOWN {
                    *dst = UNKNOWN;
                }
            }
        }
        // Plate is an actual obstruction, not an external escape seed.
        for i in 0..self.assembly.len() {
            if self.cell_box(i).lo[2] <= scene.plate_z() {
                self.assembly[i] = MATERIAL;
            }
        }
        Ok(())
    }

    fn neighbours(&self, i: usize) -> [Option<usize>; 6] {
        let p = self.xyz(i);
        std::array::from_fn(|j| {
            let k = j / 2;
            let stride = [1, self.n[0], self.n[0] * self.n[1]][k];
            if j.is_multiple_of(2) {
                p[k].checked_sub(1).map(|_| i - stride)
            } else {
                (p[k] + 1 < self.n[k]).then_some(i + stride)
            }
        })
    }

    /// Interior seeds are void enclosed by material on both sides in both X and Y.
    /// This bounded axial detector is NOT an absence proof for arbitrary concave bodies.
    /// All components are retained before any width test: a small pocket is not erased.
    pub fn chambers(&mut self) -> Vec<Vec<usize>> {
        let mut enclosed = vec![0u8; self.model.len()];
        // Count enclosing material pairs independently on all three axes. Requiring two
        // axes locates axial cavities in any facing direction, rather than only Z bores.
        for axis in 0..3 {
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            for a in 0..self.n[u] {
                for b in 0..self.n[v] {
                    let mut xyz = [0; 3];
                    xyz[u] = a;
                    xyz[v] = b;
                    let mut ends = None;
                    for k in 0..self.n[axis] {
                        xyz[axis] = k;
                        if self.model[self.index(xyz[0], xyz[1], xyz[2])] == MATERIAL {
                            ends = Some(ends.map_or((k, k), |(lo, _)| (lo, k)));
                        }
                    }
                    if let Some((lo, hi)) = ends {
                        for k in lo + 1..hi {
                            xyz[axis] = k;
                            let i = self.index(xyz[0], xyz[1], xyz[2]);
                            enclosed[i] += 1;
                        }
                    }
                }
            }
        }
        let mut out = Vec::new();
        let mut queue = VecDeque::new();
        for seed in 0..enclosed.len() {
            if enclosed[seed] < 2 || self.model[seed] != FREE || self.labels[seed] != 0 {
                continue;
            }
            let label = u32::try_from(out.len() + 1).expect("bounded cell count");
            self.labels[seed] = label;
            queue.push_back(seed);
            let mut component = Vec::new();
            while let Some(i) = queue.pop_front() {
                component.push(i);
                for j in self.neighbours(i).into_iter().flatten() {
                    if enclosed[j] >= 2 && self.model[j] == FREE && self.labels[j] == 0 {
                        self.labels[j] = label;
                        queue.push_back(j);
                    }
                }
            }
            out.push(component);
        }
        out
    }

    /// Independent bulk-width search, separate from the un-eroded pocket denominator.
    /// Squared Euclidean distance to occupied cell centres uses three linear-time passes.
    /// In the optimistic reading only definite material centres obstruct; subtracting half
    /// a cell diagonal from the required radius retains any cell that could carry a route.
    /// In the strict reading all cut cells obstruct and adding a full cell diagonal covers
    /// both the obstacle cube and every point of each route cube, not just its centre.
    pub fn bulk_routes(&self, chambers: &[Vec<usize>], diameter: f64, optimistic: bool) -> usize {
        const FAR: u32 = 1_000_000_000;
        let mut distance: Vec<u32> = self
            .assembly
            .iter()
            .map(|&c| {
                if c == MATERIAL || (!optimistic && c == UNKNOWN) {
                    0
                } else {
                    FAR
                }
            })
            .collect();
        let longest = *self.n.iter().max().expect("three dimensions");
        let mut line = vec![FAR; longest];
        let mut result = vec![FAR; longest];
        let mut sites = vec![0usize; longest];
        let mut cuts = vec![0.0; longest + 1];
        for axis in 0..3 {
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            let count = self.n[axis];
            for a in 0..self.n[u] {
                for b in 0..self.n[v] {
                    let mut xyz = [0; 3];
                    xyz[u] = a;
                    xyz[v] = b;
                    for (k, value) in line[..count].iter_mut().enumerate() {
                        xyz[axis] = k;
                        *value = distance[self.index(xyz[0], xyz[1], xyz[2])];
                    }
                    distance_line(&line[..count], &mut result[..count], &mut sites, &mut cuts);
                    for (k, &value) in result[..count].iter().enumerate() {
                        xyz[axis] = k;
                        distance[self.index(xyz[0], xyz[1], xyz[2])] = value;
                    }
                }
            }
        }
        let diagonal = 3.0f64.sqrt();
        let radius = diameter / (2.0 * CELL_MM);
        let threshold = if optimistic {
            (radius - diagonal * 0.5).max(0.0)
        } else {
            radius + diagonal
        };
        let open = |i: usize| f64::from(distance[i]) > threshold.powi(2);
        let down = self.flood(true, open);
        let air = self.flood(false, open);
        chambers
            .iter()
            .filter(|cells| !cells.iter().any(|&i| open(i) && down[i] && air[i]))
            .count()
    }

    /// Reverse directed flood from exterior faces. Side-face seeds let air move around
    /// the plate. Gravity is +Z when hanging; the air search moves towards -Z.
    /// This does not itself certify a distinct mouth or its usable width.
    pub fn directed(&self, downhill: bool, optimistic: bool) -> Vec<bool> {
        self.flood(downhill, |i| {
            self.assembly[i] == FREE || (optimistic && self.assembly[i] == UNKNOWN)
        })
    }

    fn flood(&self, downhill: bool, open: impl Fn(usize) -> bool) -> Vec<bool> {
        let mut reached = vec![false; self.model.len()];
        let mut queue = VecDeque::new();
        let z = if downhill { self.n[2] - 1 } else { 0 };
        for (i, visited) in reached.iter_mut().enumerate() {
            let p = self.xyz(i);
            let boundary = p[2] == z
                || p[0] == 0
                || p[0] + 1 == self.n[0]
                || p[1] == 0
                || p[1] + 1 == self.n[1];
            if boundary && open(i) {
                *visited = true;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            let zi = self.xyz(i)[2];
            for j in self.neighbours(i).into_iter().flatten() {
                let zj = self.xyz(j)[2];
                let direction = if downhill { zj <= zi } else { zj >= zi };
                if direction && !reached[j] && open(j) {
                    reached[j] = true;
                    queue.push_back(j);
                }
            }
        }
        reached
    }
}

#[cfg(test)]
mod tests {
    use super::distance_line;

    #[test]
    fn euclidean_distance_envelope_matches_brute_force_on_all_small_masks() {
        for mask in 0u32..256 {
            let f: Vec<u32> = (0..8)
                .map(|k| {
                    if mask & (1 << k) != 0 {
                        0
                    } else {
                        1_000_000_000
                    }
                })
                .collect();
            let mut out = [0; 8];
            distance_line(&f, &mut out, &mut [0; 8], &mut [0.0; 9]);
            for (x, &got) in out.iter().enumerate() {
                let want = f
                    .iter()
                    .enumerate()
                    .map(|(q, &d)| u64::from(d) + x.abs_diff(q).pow(2) as u64)
                    .min()
                    .expect("nonempty")
                    .min(1_000_000_000);
                assert_eq!(u64::from(got), want, "mask {mask} position {x}");
            }
        }
    }
}
