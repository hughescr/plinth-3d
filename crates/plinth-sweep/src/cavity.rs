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
//! 1. a voxel of a lattice that extends past the model on every side is SOLID
//!    only when every print layer it spans holds a whole cell of the model
//!    there, PROVED from the slice's geometry in both horizontal axes
//!    ([`crate::raster::rasterize_whole_cells`]: the cell's centre inside,
//!    and no boundary edge through the cell's interior); the voxels below
//!    the plate are solid. Whole cells, proved, not any-overlap and not a
//!    row-centre sample, because an opening must never be sealed by rounding
//!    or by sampling (challenger, 2026-10-05: the any-overlap mask rounds a
//!    span outward in X, so a 0.8 mm opening across two cells was closed by
//!    both neighbours; the row-centre carrier mask never saw a slit narrow in
//!    Y that held no row centre);
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
//! behaviour (the gap applies): an opening of any width in either horizontal
//! axis, at any phase to the lattice, or one layer high, leaves the cells it
//! touches open, so it lets the flood in; a wall that holds no 6-connected
//! run of whole cells (under 1.0 mm square to the lattice, more on a slant)
//! does not hold the flood, so a cavity behind a wall that thin exempts
//! nothing (the hollow stage's minimum wall is 2.0 mm); a cavity too narrow
//! to resolve is not enclosed air. One limit remains, stated: a lift under
//! one voxel could not be told from a cup and the flood would call the cup
//! enclosed (the lift is several mm in practice; the gap under the model is
//! exterior air).
//!
//! A member is hidden when EVERY sampled point along its axis is hidden: the
//! foot (wherever it lands: plate, model or another support), a point every
//! half voxel up the shaft, and the seat; and the member's SOLID, not its
//! centreline, is what is asked ([`HiddenCavity::solid_hidden`]): every voxel
//! the disc of the drawn or proposed radius touches must be hidden, so a raise
//! that would carry the solid to an exterior face is read as before, and a
//! solid that merely touches the cavity wall (one voxel deep) is not. A
//! proposed landing, a proposed raise or a proposed tie is asked with its own
//! geometry through [`HiddenCavity::span_hidden`] or
//! [`HiddenCavity::segment_hidden`], before it exists.

use std::collections::VecDeque;

use plinth_support::perception::SliceSolid;

use crate::drain::DrainSpec;
use crate::grid::Grid;
use crate::raster::{rasterize_whole_cells, RasterScratch};
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

        // The model, WHOLLY-CONTAINED cells only, and every layer a voxel
        // spans must contain the cell. This is the CONSERVATIVE side for an
        // opening: the any-overlap mask rounds a span outward in X, so a
        // 0.8 mm gap between two spans, standing across two coarse cells,
        // was sealed by both neighbours (challenger round 2, 2026-10-05); and
        // the row-centre carrier mask samples one y per row, so a slit narrow
        // in Y between two row centres sampled as solid wall (round 3). A
        // cell is solid here only when the slice PROVES it solid all
        // through ([`rasterize_whole_cells`]: its centre inside, and no
        // boundary edge through its interior), so a cell that an opening
        // touches, however narrow and in either axis, is NOT solid; and a
        // voxel that any one of its layers leaves open is not solid either
        // (a slit of one layer height is an opening). The price is a thin
        // wall: one that holds no 6-connected run of whole cells (under
        // 1.0 mm square to the lattice, more on a slant) does not hold the
        // flood, so a thinner cavity wall exempts nothing. The engine's
        // hollow stage never makes one (its minimum wall is 2.0 mm).
        let coarse = crate::grid::lay_out_at(origin_x, origin_y, pitch, nx, ny);
        let mut scratch = RasterScratch::new(&coarse);
        let mut whole: Vec<bool> = Vec::new();
        let plane = ny * nx;
        // Layers that touch each voxel row, and how many of them held each
        // cell wholly.
        let mut layers_in_row = vec![0_u32; nz];
        let mut held = vec![0_u32; nz * plane];
        for (layer, solids) in slices.iter().enumerate() {
            let z_lo = plate_z_mm + layer as f64 * layer_height_mm;
            let lo = ((z_lo - origin_z) / pitch).floor();
            let hi = ((z_lo + layer_height_mm - origin_z) / pitch - f64::EPSILON).floor();
            if !lo.is_finite() || !hi.is_finite() || hi < 0.0 {
                continue;
            }
            let (lo, hi) = (lo.max(0.0) as usize, (hi.max(0.0) as usize).min(nz - 1));
            if lo > hi {
                continue;
            }
            for count in &mut layers_in_row[lo..=hi] {
                *count += 1;
            }
            if solids.is_empty() {
                continue;
            }
            rasterize_whole_cells(&coarse, solids, &mut scratch, &mut whole);
            for z in lo..=hi {
                let row = &mut held[z * plane..(z + 1) * plane];
                for (count, &carried) in row.iter_mut().zip(&whole) {
                    *count += u32::from(carried);
                }
            }
        }
        for z in 0..nz {
            if layers_in_row[z] == 0 {
                continue;
            }
            let row = &held[z * plane..(z + 1) * plane];
            for (cell, &count) in row.iter().enumerate() {
                if count == layers_in_row[z] {
                    solid[z * plane + cell] = true;
                }
            }
        }

        // Each drain's mouth plugged, so the flood cannot run in by the bore.
        // A ball wider than the bore on its centre covers the bore's whole
        // cross-section at the mouth, and the margin of three voxels covers
        // the rounding of the lattice and the cells the bore's edge leaves
        // open (a bore's wall cells are partial, so the open duct is up to a
        // voxel wider on each side).
        let mut plugged = 0_usize;
        for drain in drains {
            if !drain.radius_mm.is_finite() || !drain.outlet_mm.iter().all(|v| v.is_finite()) {
                continue;
            }
            let radius = drain.radius_mm.abs() + 3.0 * pitch;
            let reach = (radius / pitch).ceil() as i64 + 1;
            let centre = [
                ((drain.outlet_mm[0] - origin_x) / pitch).floor() as i64,
                ((drain.outlet_mm[1] - origin_y) / pitch).floor() as i64,
                ((drain.outlet_mm[2] - origin_z) / pitch).floor() as i64,
            ];
            for dz in -reach..=reach {
                for dy in -reach..=reach {
                    for dx in -reach..=reach {
                        // Off the lattice below (negative) or beyond (past
                        // its last voxel) on any axis: not a voxel to plug.
                        let (Ok(x), Ok(y), Ok(z)) = (
                            usize::try_from(centre[0] + dx),
                            usize::try_from(centre[1] + dy),
                            usize::try_from(centre[2] + dz),
                        ) else {
                            continue;
                        };
                        if x >= nx || y >= ny || z >= nz {
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
                            solid[index(z, y, x)] = true;
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
    pub fn cell_hidden(&self, cell: usize, layer: usize) -> bool {
        self.solid_hidden(cell, layer, 0.0)
    }

    /// Whether a SOLID of radius `radius_mm` about the point on `cell`'s axis
    /// at `layer` lies wholly in hidden voxels: every voxel the disc of that
    /// radius (and the sampling margin) touches is hidden.
    ///
    /// **The member's SOLID, not its centreline** (challenger, 2026-10-05):
    /// a centreline in a hidden voxel says nothing about a thickened post
    /// whose raise reaches past a thin wall to the outside. A hidden voxel
    /// has enclosed air within a voxel and no exterior air within a voxel, so
    /// a solid that stays in hidden voxels contacts the cavity wall at most a
    /// voxel deep and never reaches an exterior face.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer index is exact in f64"
    )]
    pub fn solid_hidden(&self, cell: usize, layer: usize, radius_mm: f64) -> bool {
        let Some(lattice) = &self.lattice else {
            return false;
        };
        let x = lattice.grid.center_x_mm(lattice.grid.col_of(cell));
        let y = lattice.grid.center_y_mm(lattice.grid.row_of(cell));
        let z = lattice.plate_z_mm + (layer as f64 + 0.5) * lattice.layer_height_mm;
        lattice.point_hidden(x, y, z, radius_mm)
    }

    /// Whether the axis from layer `k_bottom` to `k_top`, standing in
    /// `cell_at(layer)` at each layer, with a solid of radius `radius_at`
    /// about it, lies wholly in hidden voxels: the foot, a point every half
    /// voxel of height, and the seat. A member that does not exist yet (a
    /// proposed landing, a proposed raise) is asked with the geometry it
    /// would have. The radius at a sample is the widest of the layers up to
    /// the next sample, so a profile that swells between samples is read at
    /// its widest.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive pitch-over-height ratio of a few tens"
    )]
    pub fn span_hidden(
        &self,
        k_bottom: usize,
        k_top: usize,
        cell_at: impl Fn(usize) -> usize,
        radius_at: impl Fn(usize) -> f64,
    ) -> bool {
        let Some(lattice) = &self.lattice else {
            return false;
        };
        if self.census.hidden_voxels == 0 || k_bottom == usize::MAX || k_top < k_bottom {
            return false;
        }
        let step = ((lattice.pitch_mm / lattice.layer_height_mm / 2.0).floor().max(1.0)) as usize;
        let mut layer = k_bottom;
        loop {
            let next = (layer + step).min(k_top);
            let radius = (layer..=next).map(&radius_at).fold(0.0_f64, f64::max);
            if !self.solid_hidden(cell_at(layer), layer, radius) {
                return false;
            }
            if layer >= k_top {
                return true;
            }
            layer = next;
        }
    }

    /// Whether `strut`, as it stands, with a solid of radius `radius_mm`
    /// about its axis, lies wholly inside a hidden cavity.
    #[must_use]
    pub fn member_hidden(&self, strut: &SweepStrut, radius_mm: f64) -> bool {
        self.span_hidden(strut.k_bottom, strut.k_top, |layer| strut.cell_at(layer), |_| radius_mm)
    }

    /// Whether a solid of radius `radius_mm` about the straight axis `from`
    /// to `to` (the sweep's frame: the plate at z = 0) lies wholly in hidden
    /// voxels, sampled every quarter voxel along the axis and at both ends.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a positive length over a positive step, a few hundred at most"
    )]
    pub fn segment_hidden(&self, from: [f64; 3], to: [f64; 3], radius_mm: f64) -> bool {
        let Some(lattice) = &self.lattice else {
            return false;
        };
        if self.census.hidden_voxels == 0
            || !from.iter().chain(&to).all(|v| v.is_finite())
            || !radius_mm.is_finite()
        {
            return false;
        }
        let length = geom_kernel::vec3::dist(from, to);
        let samples = (length / (0.25 * lattice.pitch_mm)).ceil().max(1.0) as usize;
        (0..=samples).all(|at| {
            let t = at as f64 / samples as f64;
            lattice.point_hidden(
                t.mul_add(to[0] - from[0], from[0]),
                t.mul_add(to[1] - from[1], from[1]),
                t.mul_add(to[2] - from[2], from[2]) + lattice.plate_z_mm,
                radius_mm,
            )
        })
    }
}

impl Lattice {
    /// Whether every voxel within `radius_mm` (and the sampling margin, a
    /// quarter voxel: the samples of an axis stand at most that far from any
    /// point between them in the horizontal plane) of the point is hidden.
    /// A voxel outside the lattice is not.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "each quotient is checked against its dimension before the index is formed"
    )]
    fn point_hidden(&self, x: f64, y: f64, z: f64, radius_mm: f64) -> bool {
        if !(x.is_finite() && y.is_finite() && z.is_finite() && radius_mm.is_finite()) {
            return false;
        }
        let reach = radius_mm.max(0.0) + 0.25 * self.pitch_mm;
        let [nz, ny, nx] = self.dims;
        let index_of = |value: f64, origin: f64, limit: usize| {
            let at = ((value - origin) / self.pitch_mm).floor();
            (at >= 0.0 && at < limit as f64).then_some(at as usize)
        };
        let Some(iz) = index_of(z, self.origin_mm[2], nz) else {
            return false;
        };
        let (Some(x_lo), Some(x_hi), Some(y_lo), Some(y_hi)) = (
            index_of(x - reach, self.origin_mm[0], nx),
            index_of(x + reach, self.origin_mm[0], nx),
            index_of(y - reach, self.origin_mm[1], ny),
            index_of(y + reach, self.origin_mm[1], ny),
        ) else {
            return false;
        };
        for iy in y_lo..=y_hi {
            let dy = distance_to_span(y, self.origin_mm[1] + iy as f64 * self.pitch_mm, self.pitch_mm);
            for ix in x_lo..=x_hi {
                let dx = distance_to_span(x, self.origin_mm[0] + ix as f64 * self.pitch_mm, self.pitch_mm);
                if dx.hypot(dy) <= reach && !self.hidden[(iz * ny + iy) * nx + ix] {
                    return false;
                }
            }
        }
        true
    }
}

/// Distance from `value` to the span `[lo, lo + width]`, zero inside it.
fn distance_to_span(value: f64, lo: f64, width: f64) -> f64 {
    (lo - value).max(value - (lo + width)).max(0.0)
}

/// The widest radius a finished arm is drawn at: the widest of its radius
/// profile and its tip. The footprint the cavity map is asked about.
#[must_use]
pub fn arm_radius_mm(arm: &plinth_support::SweepArm) -> f64 {
    arm.radius_profile_mm
        .iter()
        .copied()
        .chain(std::iter::once(arm.tip_radius_mm))
        .filter(|radius| radius.is_finite())
        .fold(0.0_f64, f64::max)
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
            // The two corner blocks: a real shell has no missing corner, and
            // the map holds whole cells only (a wall's end row is eroded
            // where nothing stands beyond it).
            parts.push(([wall, wall, outer - wall], x_wall, lo + wall / 2.0, 0.0));
            parts.push(([wall, wall, outer - wall], x_wall, hi - wall / 2.0, 0.0));
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

    /// The sealed [`cavity_box`] with a slot cut through its +y wall over the
    /// whole height, from `slot_lo` to `slot_hi` in x: a recess open to the
    /// outside by an opening of that width.
    pub(crate) fn cavity_box_slit(slot_lo: f64, slot_hi: f64) -> Mesh {
        let inner = OUTER - 2.0 * WALL;
        let lo = CENTRE - OUTER / 2.0;
        let hi = CENTRE + OUTER / 2.0;
        let y_wall = hi - WALL / 2.0;
        let (x_lo, x_hi) = (lo + WALL, hi - WALL);
        let parts: Vec<([f64; 3], f64, f64, f64)> = vec![
            ([OUTER, OUTER, WALL], CENTRE, CENTRE, OUTER - WALL),
            ([WALL, OUTER, OUTER - WALL], lo + WALL / 2.0, CENTRE, 0.0),
            ([WALL, OUTER, OUTER - WALL], hi - WALL / 2.0, CENTRE, 0.0),
            ([inner, WALL, OUTER - WALL], CENTRE, lo + WALL / 2.0, 0.0),
            // The +y wall in two pieces either side of the slot.
            ([slot_lo - x_lo, WALL, OUTER - WALL], (x_lo + slot_lo) / 2.0, y_wall, 0.0),
            ([x_hi - slot_hi, WALL, OUTER - WALL], (slot_hi + x_hi) / 2.0, y_wall, 0.0),
        ];
        boxes(&parts)
    }

    /// The sealed [`cavity_box`] with a slot cut through its +x wall over the
    /// whole height, from `slot_lo` to `slot_hi` in y: a recess open to the
    /// outside by an opening of that width, narrow in Y (the scanline's
    /// sampled axis). `slot_lo == slot_hi` leaves the wall whole, as two
    /// pieces that touch at that y.
    pub(crate) fn cavity_box_slit_in_y(slot_lo: f64, slot_hi: f64) -> Mesh {
        let inner = OUTER - 2.0 * WALL;
        let lo = CENTRE - OUTER / 2.0;
        let hi = CENTRE + OUTER / 2.0;
        let x_wall = hi - WALL / 2.0;
        let (y_lo, y_hi) = (lo + WALL, hi - WALL);
        let parts: Vec<([f64; 3], f64, f64, f64)> = vec![
            ([OUTER, OUTER, WALL], CENTRE, CENTRE, OUTER - WALL),
            ([WALL, OUTER, OUTER - WALL], lo + WALL / 2.0, CENTRE, 0.0),
            ([inner, WALL, OUTER - WALL], CENTRE, lo + WALL / 2.0, 0.0),
            ([inner, WALL, OUTER - WALL], CENTRE, hi - WALL / 2.0, 0.0),
            // The +x wall's corner blocks, and the wall in two pieces either
            // side of the slot.
            ([WALL, WALL, OUTER - WALL], x_wall, lo + WALL / 2.0, 0.0),
            ([WALL, WALL, OUTER - WALL], x_wall, hi - WALL / 2.0, 0.0),
            ([WALL, slot_lo - y_lo, OUTER - WALL], x_wall, (y_lo + slot_lo) / 2.0, 0.0),
            ([WALL, y_hi - slot_hi, OUTER - WALL], x_wall, (slot_hi + y_hi) / 2.0, 0.0),
        ];
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
        assert!(cavity.span_hidden(floor, ceiling, |_| inside, |_| 0.05));
        assert!(!cavity.span_hidden(
            floor,
            ceiling,
            |at| if at > layer(10.0) { outside } else { inside },
            |_| 0.05
        ));
        assert!(!cavity.span_hidden(floor, ceiling, |_| outside, |_| 0.05));
        assert!(!super::HiddenCavity::none().span_hidden(floor, ceiling, |_| inside, |_| 0.05));
    }

    /// **AN OPENING NARROWER THAN TWO VOXELS IS NOT SEALED BY THE LATTICE**
    /// (challenger, 2026-10-05). The sealed 24 mm box with a 0.8 mm slot cut
    /// through its +y wall over the whole height, standing across two coarse
    /// cells at the production 0.5 mm pitch: the slot spans x = 2.1 to 2.9,
    /// the cells' edges stand at 2.0, 2.5 and 3.0. The any-overlap mask
    /// rounded each neighbouring span outward into both cells and sealed the
    /// slot, so the recess read as enclosed and hid its members. The map now
    /// holds only whole cells, so the slot leaves the flood a way in: nothing
    /// is enclosed, nothing is hidden.
    ///
    /// CONTROLS, same phase and wall, nothing else changed: the box without
    /// the slot is enclosed and hides its middle (the first test above), and a
    /// slot wide enough to hold whole cells (two voxels, 1.2 mm) also leaves
    /// nothing hidden. SABOTAGE RUN (by hand): the raster's any-overlap mask
    /// in place of the whole-cell one seals the 0.8 mm slot and the first
    /// assertion fails.
    #[test]
    fn a_slot_narrower_than_two_voxels_does_not_seal_a_recess() {
        let narrow = cavity_box_slit(2.1, 2.9);
        let (map, grid) = map_of(&narrow, &[]);
        assert_eq!(map.census().enclosed_voxels, 0, "{:?}", map.census());
        assert!(!map.cell_hidden(cell(&grid, CENTRE, CENTRE), layer(10.0)), "the recess is open to the outside");
        let (wide, _) = map_of(&cavity_box_slit(2.0, 3.2), &[]);
        assert_eq!(wide.census().enclosed_voxels, 0, "control: a wide slot opens it too");
        let (sealed, _) = map_of(&cavity_box(false, false), &[]);
        assert!(sealed.census().enclosed_voxels > 0, "control: with no slot the box encloses air");
    }

    /// **AN OPENING BETWEEN TWO ROW CENTRES IS NOT SEALED EITHER**
    /// (challenger round 3, 2026-10-05). The scanline samples one y per row,
    /// so a slit narrow in Y that holds no row centre is invisible to it: a
    /// 0.4 mm slot through the +x wall over the whole height at y = 2.30 to
    /// 2.70, between the row centres 2.25 and 2.75 (row edges 2.0, 2.5, 3.0
    /// at the production 0.5 mm pitch), samples exactly as the sealed wall.
    /// The row-centre carrier mask (whole cell in X, the row and both its
    /// neighbours in Y) therefore sealed it and the open recess read as
    /// enclosed. The map now holds a cell only when the slice proves the
    /// WHOLE cell solid in both axes (no boundary edge crosses the cell's
    /// interior, and its centre is inside), so the slot's two edges open the
    /// two rows they cross: nothing is enclosed, nothing hidden.
    ///
    /// CONTROLS, same phase, wall and construction: the wall left whole as
    /// two pieces touching at y = 2.30 (the same off-grid joint, no opening)
    /// encloses air and hides the middle, so the map does not open a sealed
    /// wall at a joint; and a 0.4 mm slot at the same off-grid phase in X
    /// (x = 2.30 to 2.70 through the +y wall; it holds no COLUMN centre
    /// either) is open too. RED BEFORE THE FIX: the row-centre carrier mask
    /// sealed the Y slot (58,824 enclosed voxels). SABOTAGE RUN (by hand):
    /// `rasterize_whole_cells` with its boundary-edge clearing skipped (the
    /// centre test alone) seals it too (55,728) and the first assertion goes
    /// red.
    #[test]
    fn a_slot_between_two_row_centres_does_not_seal_a_recess() {
        let (slot, grid) = map_of(&cavity_box_slit_in_y(2.30, 2.70), &[]);
        assert_eq!(slot.census().enclosed_voxels, 0, "{:?}", slot.census());
        assert!(!slot.cell_hidden(cell(&grid, CENTRE, CENTRE), layer(10.0)), "the recess is open to the outside");
        let (whole, _) = map_of(&cavity_box_slit_in_y(2.30, 2.30), &[]);
        assert!(whole.census().enclosed_voxels > 0, "control: the wall in two touching pieces seals {:?}", whole.census());
        assert!(whole.cell_hidden(cell(&grid, CENTRE, CENTRE), layer(10.0)), "control: its middle is hidden");
        let (x_slot, _) = map_of(&cavity_box_slit(2.30, 2.70), &[]);
        assert_eq!(x_slot.census().enclosed_voxels, 0, "control: the same slot in X is open {:?}", x_slot.census());
    }

    /// **THE SOLID, NOT THE CENTRELINE** (challenger, 2026-10-05). A sealed
    /// box with 1.5 mm walls (three whole cells on the lattice), its +x wall's inner
    /// face at 8.0 mm: a post 0.475 mm inside the face is hidden as a
    /// centreline and as a 0.1 mm solid; a 0.7 mm solid touches the cavity
    /// wall 0.23 mm deep and stays hidden (contact with the cavity wall is
    /// what the ruling allows); a 1.6 mm solid reaches the wall's outer
    /// voxels and is NOT hidden, and neither is one that goes through the
    /// outer face. SABOTAGE RUN (by hand): the radius ignored in
    /// `point_hidden` makes the last two hidden.
    #[test]
    fn a_solid_is_hidden_only_while_it_stays_in_hidden_voxels() {
        let mesh = cavity_box_sized(false, false, 0.0, 9.0, 1.5);
        let (cavity, grid) = map_of(&mesh, &[]);
        assert!(cavity.census().enclosed_voxels > 0, "{:?}", cavity.census());
        let post = cell(&grid, 7.5, CENTRE);
        let at = layer(2.0);
        assert!(cavity.solid_hidden(post, at, 0.0), "the centreline");
        assert!(cavity.solid_hidden(post, at, 0.1), "a thin solid");
        assert!(cavity.solid_hidden(post, at, 0.7), "touching the cavity wall");
        assert!(!cavity.solid_hidden(post, at, 1.6), "into the wall's outer voxels");
        assert!(!cavity.solid_hidden(post, at, 2.5), "through the outer face");
    }
}
