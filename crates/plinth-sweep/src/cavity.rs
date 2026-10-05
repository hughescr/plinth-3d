//! Which members stand inside a HIDDEN CAVITY.
//!
//! Craig ruled on 2026-08-31 that a support hidden in a sealed (interior)
//! cavity is never removed or cleaned, and on 2026-10-05 that the 0.3 mm
//! final clearance gap therefore does not apply to such a member. The ruling
//! is about the CAVITY, so the test has to be about the cavity: a member is
//! hidden when the air around it is enclosed.
//!
//! **Why not the scar flag** ([`crate::SweepStrut::interior`], "model stands
//! above the landing in the same column"). That flag asks whether anyone will
//! see a SCAR; its own documentation says a re-entrant exterior pocket also
//! tests true, which was harmless for a scar bonus and is not harmless for a
//! clearance exemption (challenger, 2026-10-05): an open-sided recess with a
//! floor and an overhang is visible and removable, and the flag called it
//! hidden. The flag also says nothing for a member that lands on another
//! support, so a branch standing wholly inside a cavity on an interior post
//! read as exterior. This module answers the cavity question itself, for a
//! member of ANY landing, from the model's slices.
//!
//! # How
//!
//! One batch over the whole model, at the registered hollowing pitch (the
//! pitch the drain-escape check and the drill stage decide their own flood
//! at):
//!
//! 1. every print layer is OR-ed (any-overlap, as the escape check does) into
//!    a voxel lattice that extends past the model on every side; the voxels
//!    below the plate are solid;
//! 2. each drain's mouth is PLUGGED with a solid ball a little wider than the
//!    bore. A drain is a hole the model was drilled with so resin can leave;
//!    the cavity behind it is still the cavity (a support in it still cannot
//!    be reached), so the flood must not run in through the bore;
//! 3. the empty space is flooded from the lattice's boundary (6-connected).
//!    Empty voxels the flood never reaches are ENCLOSED air; the rest is
//!    exterior air, pockets and recesses included, because they open to the
//!    outside somewhere;
//! 4. a voxel is HIDDEN when enclosed air lies within one voxel of it and no
//!    exterior air does. A member's base stands ON a cavity floor, in or at
//!    the surface of solid, so the neighbourhood is what makes a foot on the
//!    floor count; an exterior neighbour anywhere near it (a thin wall, a lip
//!    of the opening) withholds the exemption.
//!
//! **The direction of every error is toward NOT hidden**, which is today's
//! behaviour (the gap applies): a wall thinner than a voxel is any-overlap
//! solid so it does not leak; a cavity too narrow to resolve is not enclosed
//! air and exempts nothing; a lift under one voxel could not be told from a
//! cup and the flood would call the cup enclosed (the lift is several mm in
//! practice; the gap under the model is exterior air).
//!
//! A member is hidden when EVERY sampled point along its axis is hidden: the
//! foot (wherever it lands: plate, model or another support), a point every
//! lattice voxel up the shaft, and the seat. A proposed landing or a proposed
//! tie is asked with its own geometry through [`HiddenCavity::span_hidden`],
//! before it exists.

use std::collections::VecDeque;

use plinth_support::perception::SliceSolid;

use crate::drain::DrainSpec;
use crate::grid::Grid;
use crate::raster::{rasterize_into, LayerMasks, RasterScratch};
use crate::SweepStrut;

/// What one build measured (the path-hit witnesses of the cavity map).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CavityCensus {
    /// Whether a lattice was built at all (a sweep with slices).
    pub built: bool,
    pub voxel_pitch_mm: f64,
    pub voxels: usize,
    pub solid_voxels: usize,
    /// Empty voxels the flood from outside never reached.
    pub enclosed_voxels: usize,
    /// Voxels a member may stand in under the exemption.
    pub hidden_voxels: usize,
    /// Drain mouths plugged for the flood.
    pub drains_plugged: usize,
}

#[derive(Debug)]
struct Lattice {
    /// The sweep's working grid, to turn a member's cell into a position.
    grid: Grid,
    origin_mm: [f64; 3],
    pitch_mm: f64,
    /// `[nz, ny, nx]`.
    dims: [usize; 3],
    plate_z_mm: f64,
    layer_height_mm: f64,
    hidden: Vec<bool>,
}

/// The cavity map. Empty ([`HiddenCavity::none`]) exempts nothing.
#[derive(Debug, Default)]
pub struct HiddenCavity {
    lattice: Option<Lattice>,
    census: CavityCensus,
}

impl HiddenCavity {
    /// No cavity anywhere: every question answers `false`.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn census(&self) -> CavityCensus {
        self.census
    }

    /// Whether any voxel is hidden at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.census.hidden_voxels == 0
    }

    /// Build the map from the model's slices (layer `l` is the slab
    /// `[plate_z + l h, plate_z + (l + 1) h)` in the mesh's frame, the frame
    /// the drains' mouths are in).
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "voxel counts and indices bounded by the coarse lattice this function lays out \
                  itself; every quotient is clamped into its own dimension before use"
    )]
    pub fn build(
        grid: &Grid,
        slices: &[Vec<SliceSolid>],
        drains: &[DrainSpec],
        plate_z_mm: f64,
        layer_height_mm: f64,
        voxel_pitch_mm: f64,
        pad_mm: f64,
    ) -> Self {
        if slices.is_empty() || voxel_pitch_mm <= 0.0 || layer_height_mm <= 0.0 {
            return Self::none();
        }
        let pitch = voxel_pitch_mm;
        let pad = pad_mm + pitch;
        let origin_x = grid.center_x_mm(0) - 0.5 * grid.pitch_mm() - pad;
        let origin_y = grid.center_y_mm(0) - 0.5 * grid.pitch_mm() - pad;
        let origin_z = plate_z_mm - pitch;
        let span_x = grid.pitch_mm() * grid.cols() as f64 + 2.0 * pad;
        let span_y = grid.pitch_mm() * grid.rows() as f64 + 2.0 * pad;
        let span_z = (slices.len() as f64) * layer_height_mm + pitch + pad;
        let nx = (span_x / pitch).ceil().max(1.0) as usize;
        let ny = (span_y / pitch).ceil().max(1.0) as usize;
        let nz = (span_z / pitch).ceil().max(1.0) as usize;
        let index = |z: usize, y: usize, x: usize| (z * ny + y) * nx + x;
        let mut solid = vec![false; nz * ny * nx];

        // The plate: every voxel whose centre is below the plate plane.
        for z in 0..nz {
            if origin_z + (z as f64 + 0.5) * pitch >= plate_z_mm {
                break;
            }
            for cell in 0..ny * nx {
                solid[z * ny * nx + cell] = true;
            }
        }

        // The model, any-overlap, every layer into every voxel row it touches.
        let coarse = crate::grid::lay_out_at(origin_x, origin_y, pitch, nx, ny);
        let mut scratch = RasterScratch::new(&coarse);
        let mut masks = LayerMasks::default();
        for (layer, solids) in slices.iter().enumerate() {
            let z_lo = plate_z_mm + layer as f64 * layer_height_mm;
            let lo = ((z_lo - origin_z) / pitch).floor();
            let hi = ((z_lo + layer_height_mm - origin_z) / pitch - f64::EPSILON).floor();
            if !lo.is_finite() || !hi.is_finite() || hi < 0.0 {
                continue;
            }
            let (lo, hi) = (lo.max(0.0) as usize, (hi.max(0.0) as usize).min(nz - 1));
            if lo > hi || solids.is_empty() {
                continue;
            }
            rasterize_into(&coarse, solids, &mut scratch, &mut masks);
            for z in lo..=hi {
                for cell in 0..ny * nx {
                    if masks.cover[cell] {
                        solid[z * ny * nx + cell] = true;
                    }
                }
            }
        }

        // Each drain's mouth plugged, so the flood cannot run in by the bore.
        // A ball wider than the bore on its centre covers the bore's whole
        // cross-section at the mouth, and the margin of two voxels covers the
        // rounding of the lattice.
        let mut plugged = 0_usize;
        for drain in drains {
            if !drain.radius_mm.is_finite() || !drain.outlet_mm.iter().all(|v| v.is_finite()) {
                continue;
            }
            let radius = drain.radius_mm.abs() + 2.0 * pitch;
            let reach = (radius / pitch).ceil() as i64 + 1;
            let centre = [
                ((drain.outlet_mm[0] - origin_x) / pitch).floor() as i64,
                ((drain.outlet_mm[1] - origin_y) / pitch).floor() as i64,
                ((drain.outlet_mm[2] - origin_z) / pitch).floor() as i64,
            ];
            for dz in -reach..=reach {
                for dy in -reach..=reach {
                    for dx in -reach..=reach {
                        let (x, y, z) = (centre[0] + dx, centre[1] + dy, centre[2] + dz);
                        if x < 0 || y < 0 || z < 0 || x >= nx as i64 || y >= ny as i64 || z >= nz as i64 {
                            continue;
                        }
                        let at = [
                            origin_x + (x as f64 + 0.5) * pitch,
                            origin_y + (y as f64 + 0.5) * pitch,
                            origin_z + (z as f64 + 0.5) * pitch,
                        ];
                        let d = [
                            at[0] - drain.outlet_mm[0],
                            at[1] - drain.outlet_mm[1],
                            at[2] - drain.outlet_mm[2],
                        ];
                        if d[0].mul_add(d[0], d[1].mul_add(d[1], d[2] * d[2])) <= radius * radius {
                            solid[index(z as usize, y as usize, x as usize)] = true;
                        }
                    }
                }
            }
            plugged += 1;
        }

        // The flood from the lattice's boundary over empty voxels.
        let mut exterior = vec![false; solid.len()];
        let mut queue: VecDeque<usize> = VecDeque::new();
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let boundary =
                        z == 0 || y == 0 || x == 0 || z + 1 == nz || y + 1 == ny || x + 1 == nx;
                    let at = index(z, y, x);
                    if boundary && !solid[at] && !exterior[at] {
                        exterior[at] = true;
                        queue.push_back(at);
                    }
                }
            }
        }
        while let Some(at) = queue.pop_front() {
            let (z, rest) = (at / (ny * nx), at % (ny * nx));
            let (y, x) = (rest / nx, rest % nx);
            let mut visit = |nz_: usize, ny_: usize, nx_: usize| {
                let next = index(nz_, ny_, nx_);
                if !solid[next] && !exterior[next] {
                    exterior[next] = true;
                    queue.push_back(next);
                }
            };
            if z > 0 {
                visit(z - 1, y, x);
            }
            if z + 1 < nz {
                visit(z + 1, y, x);
            }
            if y > 0 {
                visit(z, y - 1, x);
            }
            if y + 1 < ny {
                visit(z, y + 1, x);
            }
            if x > 0 {
                visit(z, y, x - 1);
            }
            if x + 1 < nx {
                visit(z, y, x + 1);
            }
        }
        let enclosed: Vec<bool> = solid
            .iter()
            .zip(&exterior)
            .map(|(&is_solid, &is_exterior)| !is_solid && !is_exterior)
            .collect();
        let enclosed_voxels = enclosed.iter().filter(|v| **v).count();

        // Hidden: enclosed air within one voxel, exterior air within none.
        let mut hidden = vec![false; solid.len()];
        if enclosed_voxels > 0 {
            for z in 0..nz {
                for y in 0..ny {
                    for x in 0..nx {
                        let mut near_enclosed = false;
                        let mut near_exterior = false;
                        'scan: for z2 in z.saturating_sub(1)..=(z + 1).min(nz - 1) {
                            for y2 in y.saturating_sub(1)..=(y + 1).min(ny - 1) {
                                for x2 in x.saturating_sub(1)..=(x + 1).min(nx - 1) {
                                    let at = index(z2, y2, x2);
                                    near_enclosed |= enclosed[at];
                                    if exterior[at] {
                                        near_exterior = true;
                                        break 'scan;
                                    }
                                }
                            }
                        }
                        hidden[index(z, y, x)] = near_enclosed && !near_exterior;
                    }
                }
            }
        }
        let hidden_voxels = hidden.iter().filter(|v| **v).count();
        let census = CavityCensus {
            built: true,
            voxel_pitch_mm: pitch,
            voxels: solid.len(),
            solid_voxels: solid.iter().filter(|v| **v).count(),
            enclosed_voxels,
            hidden_voxels,
            drains_plugged: plugged,
        };
        Self {
            lattice: Some(Lattice {
                grid: *grid,
                origin_mm: [origin_x, origin_y, origin_z],
                pitch_mm: pitch,
                dims: [nz, ny, nx],
                plate_z_mm,
                layer_height_mm,
                hidden,
            }),
            census,
        }
    }

    /// Whether the point on `cell`'s axis at `layer` is hidden.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "a layer index is exact in f64; each quotient is checked against its dimension \
                  before the index is formed"
    )]
    pub fn cell_hidden(&self, cell: usize, layer: usize) -> bool {
        let Some(lattice) = &self.lattice else {
            return false;
        };
        let (row, col) = (lattice.grid.row_of(cell), lattice.grid.col_of(cell));
        let x = lattice.grid.center_x_mm(col);
        let y = lattice.grid.center_y_mm(row);
        let z = lattice.plate_z_mm + (layer as f64 + 0.5) * lattice.layer_height_mm;
        let at = |value: f64, origin: f64| ((value - origin) / lattice.pitch_mm).floor();
        let (fx, fy, fz) = (
            at(x, lattice.origin_mm[0]),
            at(y, lattice.origin_mm[1]),
            at(z, lattice.origin_mm[2]),
        );
        let [nz, ny, nx] = lattice.dims;
        if fx < 0.0 || fy < 0.0 || fz < 0.0 {
            return false;
        }
        let (ix, iy, iz) = (fx as usize, fy as usize, fz as usize);
        if ix >= nx || iy >= ny || iz >= nz {
            return false;
        }
        lattice.hidden[(iz * ny + iy) * nx + ix]
    }

    /// Whether an axis from layer `k_bottom` to `k_top`, standing in
    /// `cell_at(layer)` at each layer, lies wholly in hidden voxels: the foot,
    /// one point per lattice voxel of height, and the seat. A member that
    /// does not exist yet (a proposed landing, a proposed tie) is asked with
    /// the geometry it would have.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive pitch-over-height ratio of a few tens"
    )]
    pub fn span_hidden(&self, k_bottom: usize, k_top: usize, cell_at: impl Fn(usize) -> usize) -> bool {
        let Some(lattice) = &self.lattice else {
            return false;
        };
        if self.census.hidden_voxels == 0 || k_bottom == usize::MAX || k_top < k_bottom {
            return false;
        }
        let stride = ((lattice.pitch_mm / lattice.layer_height_mm).floor().max(1.0)) as usize;
        let mut layer = k_bottom;
        loop {
            if !self.cell_hidden(cell_at(layer), layer) {
                return false;
            }
            if layer >= k_top {
                return true;
            }
            layer = (layer + stride).min(k_top);
        }
    }

    /// Whether `strut`, as it stands, lies wholly inside a hidden cavity.
    #[must_use]
    pub fn member_hidden(&self, strut: &SweepStrut) -> bool {
        self.span_hidden(strut.k_bottom, strut.k_top, |layer| strut.cell_at(layer))
    }
}

/// Fixtures the tests of the rules that read the cavity map share.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::HiddenCavity;
    use crate::drain::DrainSpec;
    use crate::grid::Grid;
    use plinth_mesh::{make_box, Mesh};
    use plinth_support::perception::{slice_stack, SliceStackLimits};

    /// Registered layer height of these fixtures.
    pub(crate) const H: f64 = 0.05;
    pub(crate) const PITCH: f64 = 0.5;

    /// One mesh from closed boxes that touch but never overlap: `(size,
    /// centre x, centre y, bottom z)`.
    pub(crate) fn boxes(parts: &[([f64; 3], f64, f64, f64)]) -> Mesh {
        let mut verts: Vec<[f64; 3]> = Vec::new();
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for &(size, x, y, z) in parts {
            let part = make_box(size, [x, y, z]).expect("a positive box");
            let offset = u32::try_from(verts.len()).expect("a small fixture");
            verts.extend_from_slice(part.verts());
            tris.extend(part.tris().iter().map(|t| [t[0] + offset, t[1] + offset, t[2] + offset]));
        }
        Mesh::new(verts, tris).expect("disjoint closed boxes make a valid mesh")
    }

    /// A cavity box, as touching slabs: outer `OUTER` mm square about
    /// `(5, 5)`, walls `WALL` mm, standing on the plate at z = 0, the
    /// ceiling at z = `OUTER`. `open_side` leaves the +x wall out (an
    /// open-sided pocket: a back wall, side walls and a ceiling, open to
    /// the outside on one face); `bore` cuts a square hole `BORE` mm wide
    /// through the +x wall at mid height (a drilled drain).
    pub(crate) const OUTER: f64 = 24.0;
    pub(crate) const WALL: f64 = 3.0;
    pub(crate) const BORE: f64 = 2.0;
    pub(crate) const CENTRE: f64 = 5.0;

    pub(crate) fn cavity_box(open_side: bool, bore: bool) -> Mesh {
        cavity_box_at(open_side, bore, 0.0)
    }

    /// [`cavity_box`] standing on a plate at `z0` instead of z = 0.
    pub(crate) fn cavity_box_at(open_side: bool, bore: bool, z0: f64) -> Mesh {
        cavity_box_sized(open_side, bore, z0, OUTER, WALL)
    }

    /// [`cavity_box_at`] with its own outer size and wall.
    pub(crate) fn cavity_box_sized(open_side: bool, bore: bool, z0: f64, outer: f64, wall: f64) -> Mesh {
        let inner = outer - 2.0 * wall;
        let lo = CENTRE - outer / 2.0;
        let hi = CENTRE + outer / 2.0;
        let mut parts: Vec<([f64; 3], f64, f64, f64)> = vec![
            // Ceiling.
            ([outer, outer, wall], CENTRE, CENTRE, outer - wall),
            // -x wall, and the two y walls between the ceiling and the plate.
            ([wall, outer, outer - wall], lo + wall / 2.0, CENTRE, 0.0),
            ([inner, wall, outer - wall], CENTRE, lo + wall / 2.0, 0.0),
            ([inner, wall, outer - wall], CENTRE, hi - wall / 2.0, 0.0),
        ];
        let x_wall = hi - wall / 2.0;
        if !open_side {
            if bore {
                let mid = (outer - wall) / 2.0;
                let (below, above) = (mid - BORE / 2.0, outer - wall - (mid + BORE / 2.0));
                let beside = (inner - BORE) / 2.0;
                parts.push(([wall, inner, below], x_wall, CENTRE, 0.0));
                parts.push(([wall, inner, above], x_wall, CENTRE, mid + BORE / 2.0));
                parts.push(([wall, beside, BORE], x_wall, CENTRE - (BORE + beside) / 2.0, mid - BORE / 2.0));
                parts.push(([wall, beside, BORE], x_wall, CENTRE + (BORE + beside) / 2.0, mid - BORE / 2.0));
            } else {
                parts.push(([wall, inner, outer - wall], x_wall, CENTRE, 0.0));
            }
        }
        for part in &mut parts {
            part.3 += z0;
        }
        boxes(&parts)
    }

    /// The map of `mesh` on a grid over its footprint, the plate at z = 0,
    /// with `drains` plugged.
    pub(crate) fn map_of(mesh: &Mesh, drains: &[DrainSpec]) -> (HiddenCavity, Grid) {
        map_of_at(mesh, drains, 0.0)
    }

    /// [`map_of`] with the plate at `plate_z`.
    pub(crate) fn map_of_at(mesh: &Mesh, drains: &[DrainSpec], plate_z: f64) -> (HiddenCavity, Grid) {
        let grid = crate::grid::lay_out_at(-14.0, -14.0, 0.1, 360, 360);
        (map_on(mesh, drains, plate_z, &grid), grid)
    }

    /// The map of `mesh` on `grid`, which must cover the mesh's footprint
    /// (the sweep's grid always does), the plate at `plate_z`.
    pub(crate) fn map_on(mesh: &Mesh, drains: &[DrainSpec], plate_z: f64, grid: &Grid) -> HiddenCavity {
        let top = mesh.verts().iter().map(|v| v[2]).fold(f64::MIN, f64::max);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a small positive layer count"
        )]
        let layers = ((top - plate_z) / H).ceil() as usize;
        #[expect(clippy::cast_precision_loss, reason = "a small layer index")]
        let planes: Vec<f64> = (0..layers).map(|layer| plate_z + (layer as f64 + 0.5) * H).collect();
        let slices = slice_stack(mesh, &planes, SliceStackLimits::production()).expect("the fixture slices");
        HiddenCavity::build(grid, &slices, drains, plate_z, H, PITCH, 1.0)
    }

    /// The cell at `(x, y)` mm of `grid`.
    pub(crate) fn cell(grid: &Grid, x: f64, y: f64) -> usize {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a coordinate inside the grid"
        )]
        grid.index(grid.row_coord(y).floor() as usize, grid.col_coord(x).floor() as usize)
    }

    /// The layer whose slab holds height `z` mm.
    pub(crate) fn layer(z: f64) -> usize {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a small positive layer index"
        )]
        let layer = (z / H).floor() as usize;
        layer
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use crate::drain::DrainSpec;

    /// **A SEALED CAVITY IS HIDDEN; THE WALL AND THE OUTSIDE ARE NOT.** A
    /// 24 mm box with 3 mm walls: a point in the middle of its 18 mm cavity,
    /// and one at the cavity's floor beside the plate, are hidden; a point in
    /// the wall's mass, and one outside the box, are not.
    #[test]
    fn a_sealed_cavity_is_hidden_and_its_wall_and_the_outside_are_not() {
        let (cavity, grid) = map_of(&cavity_box(false, false), &[]);
        let census = cavity.census();
        assert!(census.enclosed_voxels > 0 && census.hidden_voxels > 0, "{census:?}");
        let inside = cell(&grid, CENTRE, CENTRE);
        assert!(cavity.cell_hidden(inside, layer(10.0)), "the middle of the cavity");
        assert!(cavity.cell_hidden(inside, layer(0.2)), "the cavity floor (the plate)");
        assert!(!cavity.cell_hidden(cell(&grid, CENTRE - OUTER / 2.0 + 1.5, CENTRE), layer(10.0)), "the wall's mass");
        assert!(!cavity.cell_hidden(cell(&grid, CENTRE + OUTER / 2.0 + 2.0, CENTRE), layer(10.0)), "outside");
    }

    /// **AN OPEN-SIDED POCKET IS NOT A CAVITY** (challenger 2026-10-05: the
    /// scar flag, "model stands above the landing in the same column", called
    /// a recess with a floor and an overhang hidden). The same box with its +x
    /// wall left out has a ceiling over the same floor and the same column
    /// of model material over every point inside it, and is open to the
    /// outside on one face: nothing in it is hidden. (The scar flag's own
    /// answer for such a column is the anti-vacuity of the thickening test in
    /// `slenderness`, which asks both.)
    #[test]
    fn an_open_sided_pocket_is_not_hidden_though_model_stands_over_it() {
        let (pocket, grid) = map_of(&cavity_box(true, false), &[]);
        assert_eq!(pocket.census().enclosed_voxels, 0, "{:?}", pocket.census());
        for z in [0.2, 5.0, 10.0, 20.0] {
            assert!(
                !pocket.cell_hidden(cell(&grid, CENTRE, CENTRE), layer(z)),
                "pocket air at {z} mm is exterior air"
            );
        }
    }

    /// **A DRILLED DRAIN DOES NOT OPEN THE CAVITY.** The box with a 2 mm
    /// bore through its +x wall: flooded with no plug the bore lets the
    /// outside in and nothing is hidden; with the drain's mouth plugged (a
    /// `DrainSpec` at the outer face of the wall) the cavity is enclosed
    /// again.
    #[test]
    fn a_drilled_bore_is_plugged_so_the_cavity_stays_hidden() {
        let mesh = cavity_box(false, true);
        let (open, grid) = map_of(&mesh, &[]);
        assert_eq!(open.census().hidden_voxels, 0, "no plug: the bore floods the cavity {:?}", open.census());
        let mouth = DrainSpec {
            outlet_mm: [CENTRE + OUTER / 2.0, CENTRE, (OUTER - WALL) / 2.0],
            outward_axis: [1.0, 0.0, 0.0],
            radius_mm: BORE / 2.0,
        };
        let (plugged, _) = map_of(&mesh, &[mouth]);
        assert_eq!(plugged.census().drains_plugged, 1);
        assert!(plugged.cell_hidden(cell(&grid, CENTRE, CENTRE), layer(10.0)), "{:?}", plugged.census());
    }

    /// A member is hidden only when its WHOLE axis is: a branch standing
    /// from the cavity's floor to a seat under the ceiling is; the same
    /// member with a seat past the wall, and a member whose foot is outside,
    /// are not. The empty map exempts nothing.
    #[test]
    fn a_member_is_hidden_only_when_its_whole_axis_is() {
        let (cavity, grid) = map_of(&cavity_box(false, false), &[]);
        let inside = cell(&grid, CENTRE, CENTRE);
        let outside = cell(&grid, CENTRE + OUTER / 2.0 + 2.0, CENTRE);
        let (floor, ceiling) = (layer(0.2), layer(OUTER - WALL - 1.0));
        assert!(cavity.span_hidden(floor, ceiling, |_| inside));
        assert!(!cavity.span_hidden(floor, ceiling, |at| if at > layer(10.0) { outside } else { inside }));
        assert!(!cavity.span_hidden(floor, ceiling, |_| outside));
        assert!(!super::HiddenCavity::none().span_hidden(floor, ceiling, |_| inside));
    }
}
