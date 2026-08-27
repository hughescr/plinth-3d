//! Immutable, exact H0 cavity membership hand-off.
//!
//! This deliberately retains the selected EDT/label component rather than
//! attempting to infer it from the Boolean shell.  The bit representation is
//! compact and its iteration order is the frozen x-fastest grid order.

use plinth_core::Finite;
use scirs2_core::ndarray::Array3;

use crate::HollowError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CavityFieldRetainedAllocations {
    membership_bytes: usize,
    source_occupancy_bytes: usize,
}

impl CavityFieldRetainedAllocations {
    #[must_use]
    pub const fn membership_bytes(self) -> usize {
        self.membership_bytes
    }

    #[must_use]
    pub const fn source_occupancy_bytes(self) -> usize {
        self.source_occupancy_bytes
    }
}

#[derive(Debug, PartialEq)]
pub struct CavityField {
    dims: [usize; 3],
    origin_mm: [Finite; 3],
    pitch_mm: Finite,
    membership: Vec<u64>,
    source_occupancy: Vec<u64>,
    member_count: usize,
}

impl CavityField {
    #[cfg(test)]
    pub(crate) fn with_test_unequal_retained_capacities(mut self) -> Self {
        let minimum_capacity = self
            .membership
            .capacity()
            .checked_add(1)
            .expect("test capacity fits usize");
        let original = self.source_occupancy.clone();
        let mut expanded = Vec::with_capacity(minimum_capacity.max(original.len()));
        expanded.extend_from_slice(&original);
        assert_eq!(expanded, original);
        self.source_occupancy = expanded;
        assert!(self.source_occupancy.capacity() > self.membership.capacity());
        self
    }

    #[cfg(test)]
    pub(crate) fn from_selected_mask(
        mask: &Array3<bool>,
        origin_mm: [f64; 3],
        pitch_mm: f64,
    ) -> Result<Self, HollowError> {
        Self::from_selected_mask_and_occupancy(mask, mask, origin_mm, pitch_mm)
    }

    pub(crate) fn from_selected_mask_and_occupancy(
        mask: &Array3<bool>,
        occupancy: &Array3<bool>,
        origin_mm: [f64; 3],
        pitch_mm: f64,
    ) -> Result<Self, HollowError> {
        let [nz, ny, nx]: [usize; 3] = mask
            .shape()
            .try_into()
            .map_err(|_| HollowError::VoxelStack)?;
        let count = nx
            .checked_mul(ny)
            .and_then(|value| value.checked_mul(nz))
            .ok_or(HollowError::VoxelGridTooLarge)?;
        let words = count
            .checked_add(63)
            .ok_or(HollowError::ResourceAccountingOverflow)?
            / 64;
        let mut membership = vec![0_u64; words];
        if occupancy.shape() != mask.shape() {
            return Err(HollowError::VoxelStack);
        }
        let mut source_occupancy = vec![0_u64; words];
        let mut member_count = 0_usize;
        // Do not rely on ndarray's physical stride iteration here. H0's
        // coordinate contract is explicit: mask indexes are (z,y,x), while
        // field IDs are canonical x-fastest values.
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    if mask[(z, y, x)] {
                        let flat = x + y * nx + z * nx * ny;
                        membership[flat / 64] |= 1_u64 << (flat % 64);
                        member_count = member_count
                            .checked_add(1)
                            .ok_or(HollowError::VoxelGridTooLarge)?;
                    }
                    if occupancy[(z, y, x)] {
                        let flat = x + y * nx + z * nx * ny;
                        source_occupancy[flat / 64] |= 1_u64 << (flat % 64);
                    }
                }
            }
        }
        let finite = |value| Finite::new(value).map_err(|_| HollowError::InvalidArithmetic);
        Ok(Self {
            dims: [nx, ny, nz],
            origin_mm: [
                finite(origin_mm[0])?,
                finite(origin_mm[1])?,
                finite(origin_mm[2])?,
            ],
            pitch_mm: finite(pitch_mm)?,
            membership,
            source_occupancy,
            member_count,
        })
    }

    #[must_use]
    pub const fn dimensions(&self) -> [usize; 3] {
        self.dims
    }

    #[must_use]
    pub const fn origin_mm(&self) -> [Finite; 3] {
        self.origin_mm
    }

    #[must_use]
    pub const fn pitch_mm(&self) -> Finite {
        self.pitch_mm
    }

    #[must_use]
    pub const fn member_count(&self) -> usize {
        self.member_count
    }

    #[must_use]
    pub const fn retained_allocations(&self) -> CavityFieldRetainedAllocations {
        CavityFieldRetainedAllocations {
            membership_bytes: self.membership.capacity() * std::mem::size_of::<u64>(),
            source_occupancy_bytes: self.source_occupancy.capacity() * std::mem::size_of::<u64>(),
        }
    }

    #[must_use]
    pub(crate) fn contains_flat(&self, flat: usize) -> bool {
        self.membership
            .get(flat / 64)
            .is_some_and(|word| word & (1_u64 << (flat % 64)) != 0)
    }

    /// Is this world point in the selected cavity, or in the voxel shell
    /// immediately around it?
    ///
    /// The cavity's own surface is marching-cubes over this membership, so it
    /// sits about half a pitch outside the member voxel centres: a point that
    /// is genuinely in the void can round to a non-member index. Widening the
    /// question to the 26 neighbours removes that half-pitch fuzz. It is a
    /// one-voxel widening, not a tolerance on any gate — the caller uses this
    /// only to tell the void it is draining apart from the open air outside
    /// the model, and those two are always separated by at least the realized
    /// wall, which is several voxels thick.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the \
              integer bounds and maps NaN to 0, and every site in this function is range-checked \
              before the cast or clamped into range after it (read site by site, 2026-08-26); \
              integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    pub(crate) fn contains_point_neighbourhood(&self, point: [f64; 3]) -> bool {
        let pitch = self.pitch_mm.get();
        let base = [0, 1, 2].map(|axis| {
            ((point[axis] - self.origin_mm[axis].get()) / pitch).round_ties_even() as isize
        });
        for dz in -1_isize..=1 {
            for dy in -1_isize..=1 {
                for dx in -1_isize..=1 {
                    let q = [base[0] + dx, base[1] + dy, base[2] + dz];
                    if q.iter().enumerate().any(|(axis, value)| {
                        *value < 0 || *value as usize >= self.dims[axis]
                    }) {
                        continue;
                    }
                    let flat = q[0] as usize
                        + q[1] as usize * self.dims[0]
                        + q[2] as usize * self.dims[0] * self.dims[1];
                    if self.contains_flat(flat) {
                        return true;
                    }
                }
            }
        }
        false
    }

    #[must_use]
    pub(crate) fn source_contains_flat(&self, flat: usize) -> bool {
        self.source_occupancy
            .get(flat / 64)
            .is_some_and(|word| word & (1_u64 << (flat % 64)) != 0)
    }

    #[must_use]
    pub(crate) fn flat_to_index(&self, flat: usize) -> [usize; 3] {
        let nx = self.dims[0];
        let plane = nx * self.dims[1];
        let z = flat / plane;
        let remaining = flat % plane;
        [remaining % nx, remaining / nx, z]
    }

    #[must_use]
    pub(crate) fn world_at_flat(&self, flat: usize) -> [f64; 3] {
        let [x, y, z] = self.flat_to_index(flat);
        let pitch = self.pitch_mm.get();
        [
            self.origin_mm[0].get() + x as f64 * pitch,
            self.origin_mm[1].get() + y as f64 * pitch,
            self.origin_mm[2].get() + z as f64 * pitch,
        ]
    }

    #[must_use]
    pub(crate) fn flat_count(&self) -> usize {
        self.dims[0] * self.dims[1] * self.dims[2]
    }

    pub(crate) fn member_flats(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.flat_count()).filter(|flat| self.contains_flat(*flat))
    }

    /// How many **separate chambers** this one selected resin-flow interior
    /// occupies, counted as the connected components of its own deep core —
    /// the members whose clearance to the cavity's boundary is at least
    /// `min_clearance_mm`.
    ///
    /// A **rotation- and translation-invariant** structural measurement, and
    /// the honest way to ask "did the far chamber get in?" once the
    /// orientation sweep has turned the model and there is no axis left to
    /// ask along. A passage narrower than `min_clearance_mm` contributes no
    /// deep voxels at all, so two chambers joined only through such a passage
    /// count as two, while one chamber plus however much dead space a flood
    /// legitimately claimed inside that passage still counts as one. Pick
    /// `min_clearance_mm` between the throat's own cavity radius and the
    /// smaller chamber's, and the count is exactly the chamber count.
    ///
    /// # Errors
    ///
    /// [`HollowError`] if the distance transform or the labelling refuses the
    /// field's own dimensions.
    pub fn deep_lobe_count(&self, min_clearance_mm: f64) -> Result<usize, HollowError> {
        if !min_clearance_mm.is_finite() || min_clearance_mm <= 0.0 {
            return Err(HollowError::InvalidArithmetic);
        }
        let [nx, ny, nz] = self.dims;
        let mut mask = Array3::from_elem((nz, ny, nx), false);
        for flat in self.member_flats() {
            let [x, y, z] = self.flat_to_index(flat);
            mask[(z, y, x)] = true;
        }
        let clearance = crate::frozen_distance_transform(&mask, self.pitch_mm.get())?;
        let deep = clearance.mapv(|distance_mm| distance_mm >= min_clearance_mm);
        if !deep.iter().any(|value| *value) {
            return Ok(0);
        }
        let (_labels, count) = scirs2_ndimage::morphology3d::label3d(deep.view(), 6)
            .map_err(|_| HollowError::VoxelStack)?;
        Ok(count)
    }

    pub(crate) fn mask_excluding(&self, excluded: &[bool]) -> Result<Array3<bool>, HollowError> {
        if excluded.len() != self.flat_count() {
            return Err(HollowError::VoxelStack);
        }
        let [nx, ny, nz] = self.dims;
        let mut mask = Array3::from_elem((nz, ny, nx), false);
        for flat in self.member_flats().filter(|flat| !excluded[*flat]) {
            let [x, y, z] = self.flat_to_index(flat);
            mask[(z, y, x)] = true;
        }
        Ok(mask)
    }

    pub(crate) fn refill_excluded(&mut self, excluded: &[bool]) -> Result<usize, HollowError> {
        if excluded.len() != self.flat_count() {
            return Err(HollowError::VoxelStack);
        }
        let mut removed = 0_usize;
        for (flat, exclude) in excluded.iter().copied().enumerate() {
            if exclude && self.contains_flat(flat) {
                self.membership[flat / 64] &= !(1_u64 << (flat % 64));
                removed = removed
                    .checked_add(1)
                    .ok_or(HollowError::ResourceAccountingOverflow)?;
            }
        }
        self.member_count = self
            .member_count
            .checked_sub(removed)
            .ok_or(HollowError::ResourceAccountingOverflow)?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn x_fastest_packing_preserves_the_h0_transform() {
        let mut mask = Array3::from_elem((2, 2, 3), false);
        mask[(1, 0, 2)] = true;
        let field = CavityField::from_selected_mask(&mask, [-1.0, 2.0, 3.0], 0.5)
            .expect("finite source transform");
        assert_eq!(field.dimensions(), [3, 2, 2]);
        assert_eq!(field.member_count(), 1);
        assert!(field.contains_flat(8));
        assert_eq!(field.flat_to_index(8), [2, 0, 1]);
        assert_eq!(field.world_at_flat(8), [0.0, 2.0, 3.5]);
        assert_eq!(field.member_flats().collect::<Vec<_>>(), vec![8]);
        assert_eq!(
            field.retained_allocations(),
            CavityFieldRetainedAllocations {
                membership_bytes: 8,
                source_occupancy_bytes: 8,
            }
        );
    }
}
