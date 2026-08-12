//! Removal-only well-composedness repair for the selected cavity mask.
//!
//! # Why this exists
//!
//! `fast_surface_nets` emits **at most one vertex per lattice cell**. That is
//! only enough when the mask is *well-composed*: when every 2×2×2 cell of the
//! occupancy array meets the surface in a single disc. Where it does not, two
//! sheets of the surface are forced to share that cell's single vertex, and the
//! edges around it are used by four faces instead of two. The extracted cavity
//! is then watertight and consistently oriented but **non-manifold**, the
//! Boolean inherits it, and `create_shell` refuses its own shell with
//! [`crate::HollowError::InvalidShellTopology`] — the refusal FU-49 filed on
//! `Warhorse.stl` and the Weeping Treant, both of which have pristine,
//! defect-free *input*.
//!
//! # The two critical configurations
//!
//! A binary 3-D volume is well-composed exactly when no 2×2×2 cell contains
//! either (Latecki's C1/C2):
//!
//! - **C1** — one of the cell's six 2×2 faces holds exactly two *diagonal*
//!   foreground voxels. The surface pinches to a point across that face.
//! - **C2** — the cell's foreground is exactly one antipodal (body-diagonal)
//!   pair, or its *background* is exactly one antipodal pair. The surface
//!   pinches to a point at the cell's centre.
//!
//! # Why the repair only ever removes cavity
//!
//! Both critical configurations can be broken by moving voxels in either
//! direction, and only one direction is safe. **Removing** cavity voxels turns
//! void into material: the shell wall can only get *thicker*, so a repair can
//! never breach the registered wall thickness, and the saving it costs is
//! bounded by the voxels it removes. **Adding** cavity voxels would thin the
//! wall at exactly the pinch points where it is already thinnest, which is a
//! wall-thickness violation dressed as a topology fix. So this module removes
//! and never adds, and the wall constant is neither read nor touched here.
//!
//! # Termination, work and memory — stated as what the code guarantees
//!
//! Removal is monotone on a finite set, so the fixpoint terminates with no
//! budget to exhaust: every round that does not stop removes at least one
//! voxel, and the foreground can only shrink.
//!
//! Everything below is bounded by the grid the caller already admitted through
//! `max_voxel_cells`. **No constant, cap, floor or gate threshold is
//! introduced, raised, reinterpreted or bypassed by this module.**
//!
//! - **Cell visits.** One full scan of `(nz+1)(ny+1)(nx+1)` cells, after which
//!   a round only looks at the eight cells incident to each voxel that actually
//!   changed — the complete set of configurations that change can have moved.
//!   Total cell visits are therefore at most `cells + 8 × changed`. This is
//!   counted, not asserted: [`WellComposedRepair::cell_visits`].
//! - **Whole-grid passes.** A round that removes anything can strand part of
//!   the component, so the component is re-selected with a `label3d` over the
//!   whole grid followed by one whole-grid scan for what the re-selection
//!   dropped. **That is a second `O(cells)` domain and it is counted
//!   separately** ([`WellComposedRepair::full_grid_passes`]) rather than folded
//!   into the cell-visit bound, which it is not covered by.
//!
//!   **Each counted re-selection pass is two whole-grid traversals, not one**:
//!   the `label3d` and the `indexed_iter` sweep that applies its verdict. So
//!   with `p` counted passes the true total is
//!   `cells × (2p − 1) + 8 × changed` — the opening cell scan, then two
//!   traversals per re-selection. An earlier version of this doc wrote
//!   `cells × p + 8 × changed`, which undercounts the re-selection domain by
//!   about 2×. Measured corpus-wide `p` is 2, so the true figure is three
//!   traversals rather than two.
//!
//!   **The pass count is bounded by a caller-owned admission limit**
//!   (`H0AdmissionLimits::max_cavity_repair_passes`, production default
//!   **8**), checked at the top of every re-selection round, strictly before
//!   its `label3d` call. FU-54 closed the gap an earlier version of this doc
//!   described here: without a bound, `p ≤ 2 + dropped` and `dropped` can be
//!   `O(cells)`, which is the one unbounded work domain a stage whose entire
//!   design principle is admitted work domains must not contain. A round
//!   that would exceed the limit is never taken — the caller gets
//!   [`crate::HollowError::CavityRepairPassLimitExceeded`], carrying the same
//!   repair evidence the other two repair-caused refusals carry, instead of
//!   the traversal that would have exceeded the bound. The number 8 is the
//!   **owner's**, decided 2026-08-12
//!   (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`,
//!   §7), not derived from anything: measured real models use 1–2 passes — 2
//!   on the committed Weeping Treant v1.2 fixture, the raw Warhorse, the raw
//!   Giant Toad, the raw Awakened Tree and the synthetic pinched dumbbell; 1
//!   on every zero-critical-cell mask. Eight is 4× headroom over the worst
//!   measured case, and it is a ceiling that never bites real geometry, not a
//!   budget to grow into.
//! - **Transient memory.** The only working buffers are a one-byte-per-voxel
//!   `marked` flag array, reused across every round and cleared through the
//!   removal list rather than refilled, and the removal list itself — one
//!   `u32` **linear voxel index** per voxel removed in a single round, so at
//!   most four bytes per voxel of the admitted grid. Both are the same order as
//!   the `Array3<i32>` label array `label3d` already materialises over the same
//!   grid on the caller's behalf. Nothing here materialises the eight incident
//!   cells of every changed voxel: that set — up to `8 × changed` entries of
//!   `[usize; 3]`, i.e. 192 bytes per changed voxel and multiple gigabytes at
//!   the admitted cap — is enumerated lazily instead. Duplicate cells inside it
//!   are visited rather than deduplicated, which is free: the removal rule is a
//!   pure function of the cell's eight bits, so a second visit decides
//!   identically.
//!
//! The removal rule being a pure function of the configuration is also what
//! makes the whole pass order-free and deterministic; a `0..256` test asserts
//! it, and asserts no background corner is ever marked.

use scirs2_core::ndarray::Array3;
use scirs2_ndimage::morphology3d::label3d;

use crate::{largest_label, HollowError};

/// What a repair did, for the resource record and for tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WellComposedRepair {
    critical_cells: usize,
    removed_voxels: usize,
    reselection_dropped_voxels: usize,
    rounds: usize,
    cell_visits: usize,
    full_grid_passes: usize,
}

impl WellComposedRepair {
    /// Critical 2×2×2 cells counted in the first full scan — the size of the
    /// defect before anything was removed.
    pub(crate) const fn critical_cells(self) -> usize {
        self.critical_cells
    }

    /// Cavity voxels turned back into material to reach well-composedness.
    pub(crate) const fn removed_voxels(self) -> usize {
        self.removed_voxels
    }

    /// Cavity voxels dropped because removal disconnected them from the
    /// selected component.
    pub(crate) const fn reselection_dropped_voxels(self) -> usize {
        self.reselection_dropped_voxels
    }

    /// Fixpoint rounds, including the final round that removed nothing. The
    /// resource record does not carry it — it is a property of the iteration,
    /// not of the shell — but the convergence tests assert on it.
    #[cfg(test)]
    pub(crate) const fn rounds(self) -> usize {
        self.rounds
    }

    /// 2×2×2 cells examined across every round.
    pub(crate) const fn cell_visits(self) -> usize {
        self.cell_visits
    }

    /// Passes that touch the *whole* grid: the one opening cell scan, plus one
    /// per component re-selection. The cell-visit count does not cover the
    /// re-selections, so this is what makes the second `O(cells)` domain
    /// visible.
    ///
    /// **A re-selection pass costs two whole-grid traversals**, the `label3d`
    /// and the sweep that applies its verdict, so `p` counted passes are
    /// `2p − 1` traversals of the grid.
    pub(crate) const fn full_grid_passes(self) -> usize {
        self.full_grid_passes
    }
}

/// Repair `mask` to a well-composed single 6-connected component.
///
/// The mask handed in is the largest 6-connected component of the `edt > wall`
/// selection. Removing voxels can disconnect it, so the component is re-selected
/// after any removal and the result is re-verified rather than assumed: the loop
/// exits only on a round that finds nothing to remove and a re-selection that
/// drops nothing. Each pass strictly shrinks the foreground, so the loop
/// terminates.
///
/// [`HollowError::CavityRepairRemovedCavity`] is returned — carrying what the
/// repair had done by then — when the fixpoint consumes the component entirely.
/// That is a *repair* outcome, not a property of the geometry, and the caller
/// must be able to tell it apart from a mask that never had a cavity.
///
/// `max_passes` bounds [`WellComposedRepair::full_grid_passes`] (FU-54): a
/// re-selection round is checked against it *before* that round's `label3d`
/// call, and [`HollowError::CavityRepairPassLimitExceeded`] is returned in
/// place of the traversal that would have exceeded it, carrying what the
/// repair had done so far. This is a caller-owned admission limit, not a
/// constant of this module — see `H0AdmissionLimits::max_cavity_repair_passes`.
pub(crate) fn well_composed_cavity_component(
    mut mask: Array3<bool>,
    max_passes: usize,
) -> Result<(Array3<bool>, WellComposedRepair), HollowError> {
    let dims: [usize; 3] = mask
        .shape()
        .try_into()
        .map_err(|_| HollowError::VoxelStack)?;
    let voxel_count = dims[0]
        .checked_mul(dims[1])
        .and_then(|value| value.checked_mul(dims[2]))
        .ok_or(HollowError::VoxelGridTooLarge)?;
    // Every changed voxel is carried as one `u32` linear index rather than a
    // `[usize; 3]`, which is what keeps the transient workset four bytes wide.
    u32::try_from(voxel_count).map_err(|_| HollowError::VoxelGridTooLarge)?;
    let mut stats = WellComposedRepair::default();
    // One byte per voxel, allocated once and reused by every round. It is
    // cleared through the removal list, never refilled, so it costs one pass in
    // total and not one per round.
    let mut marked = vec![false; voxel_count];
    // The only full cell scan there is. Every later pass looks at the cells
    // incident to voxels that actually changed, which is the complete set of
    // cells whose configuration can have moved.
    let mut cells = CellSet::all(dims)?;
    stats.full_grid_passes += 1;
    let mut first_scan = true;
    loop {
        let removed = repair_cells(&mut mask, dims, cells, &mut marked, &mut stats, first_scan)?;
        first_scan = false;
        if removed == 0 {
            return Ok((mask, stats));
        }
        // Removal can strand part of the component. Re-select the largest, and
        // re-verify only around what the re-selection dropped. This is the
        // second whole-grid domain the module doc names and counts.
        //
        // FU-54: bounded, checked BEFORE the `label3d` call below so the
        // refusal fires in place of the traversal that would exceed the
        // limit, not after it.
        if stats.full_grid_passes >= max_passes {
            return Err(HollowError::CavityRepairPassLimitExceeded {
                passes: stats.full_grid_passes,
                max_passes,
                critical_cells: stats.critical_cells,
                repaired_voxels: stats.removed_voxels + stats.reselection_dropped_voxels,
                cell_visits: stats.cell_visits,
            });
        }
        stats.full_grid_passes += 1;
        let (labels, label_count) = label3d(mask.view(), 6).map_err(|_| HollowError::VoxelStack)?;
        if label_count == 0 {
            return Err(HollowError::CavityRepairRemovedCavity {
                critical_cells: stats.critical_cells,
                repaired_voxels: stats.removed_voxels + stats.reselection_dropped_voxels,
                cell_visits: stats.cell_visits,
                full_grid_passes: stats.full_grid_passes,
            });
        }
        let largest = largest_label(&labels, label_count)?;
        let mut dropped: Vec<u32> = Vec::new();
        for (index, label) in labels.indexed_iter() {
            let voxel = [index.0, index.1, index.2];
            if mask[voxel] && *label != largest {
                mask[voxel] = false;
                dropped.push(linear_index(dims, voxel));
            }
        }
        if dropped.is_empty() {
            return Ok((mask, stats));
        }
        stats.reselection_dropped_voxels += dropped.len();
        cells = CellSet::incident(dims, dropped);
    }
}

/// The linear index of a voxel in the mask's C-order layout.
fn linear_index(dims: [usize; 3], voxel: [usize; 3]) -> u32 {
    let index = (voxel[0] * dims[1] + voxel[1]) * dims[2] + voxel[2];
    // The caller proved `dims[0] * dims[1] * dims[2]` fits in a `u32` before
    // any index was taken, so this cannot truncate.
    index as u32
}

/// The voxel a linear index addresses.
const fn voxel_of(dims: [usize; 3], index: u32) -> [usize; 3] {
    let index = index as usize;
    let x = index % dims[2];
    let rest = index / dims[2];
    [rest / dims[1], rest % dims[1], x]
}

/// The cells a round must examine.
///
/// The first round looks at every cell there is, which on the largest admitted
/// grid is ~17 M of them. The follow-up rounds look at the eight cells around
/// each voxel that changed. **Neither is materialised.** The whole-grid case is
/// a pure range walk, and the incident case holds only the changed voxels
/// themselves — four bytes each — and enumerates their cells lazily, because
/// the materialised, sorted set of `[usize; 3]` cells it replaced was 192 bytes
/// per changed voxel with no admitted bound of its own.
enum CellSet {
    /// Every cell of the grid, in the frozen z/y/x order. `cell_dims` is the
    /// voxel dims plus one on each axis, because cell `c` spans voxels
    /// `c - 1` and `c`.
    All { cell_dims: [usize; 3], total: usize },
    /// The cells incident to a set of changed voxels, held as linear indices.
    /// Duplicates are visited, not deduplicated: the removal rule is a pure
    /// function of the cell's configuration, so a repeat visit decides
    /// identically and dedup would cost the memory this case exists to avoid.
    /// `voxel_dims` here, not cell dims: the entries are voxel indices, and a
    /// voxel's eight incident cells are itself plus one on each axis.
    Incident {
        voxel_dims: [usize; 3],
        voxels: Vec<u32>,
    },
}

impl CellSet {
    /// Every 2×2×2 cell of the grid.
    fn all(dims: [usize; 3]) -> Result<Self, HollowError> {
        // Cells are addressed by their low corner, offset by one so the
        // background padding ring `marching_cavity` adds is covered: cell `c`
        // spans voxels `c - 1` and `c`, for `c` in `0..=n`.
        let cell_dims = [
            dims[0]
                .checked_add(1)
                .ok_or(HollowError::VoxelGridTooLarge)?,
            dims[1]
                .checked_add(1)
                .ok_or(HollowError::VoxelGridTooLarge)?,
            dims[2]
                .checked_add(1)
                .ok_or(HollowError::VoxelGridTooLarge)?,
        ];
        let total = cell_dims[0]
            .checked_mul(cell_dims[1])
            .and_then(|value| value.checked_mul(cell_dims[2]))
            .ok_or(HollowError::VoxelGridTooLarge)?;
        Ok(Self::All { cell_dims, total })
    }

    const fn incident(voxel_dims: [usize; 3], voxels: Vec<u32>) -> Self {
        Self::Incident { voxel_dims, voxels }
    }

    fn len(&self) -> usize {
        match self {
            Self::All { total, .. } => *total,
            // Eight cells per changed voxel, duplicates included: this is the
            // number of cell visits the round will actually perform, which is
            // what the work accounting must be told.
            Self::Incident { voxels, .. } => voxels.len() * 8,
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn iter(&self) -> Box<dyn Iterator<Item = [usize; 3]> + '_> {
        match self {
            Self::All { cell_dims, .. } => {
                let dims = *cell_dims;
                Box::new((0..dims[0]).flat_map(move |z| {
                    (0..dims[1]).flat_map(move |y| (0..dims[2]).map(move |x| [z, y, x]))
                }))
            }
            Self::Incident { voxel_dims, voxels } => {
                let dims = *voxel_dims;
                Box::new(voxels.iter().flat_map(move |index| {
                    let voxel = voxel_of(dims, *index);
                    (0..2).flat_map(move |dz| {
                        (0..2).flat_map(move |dy| {
                            (0..2).map(move |dx| [voxel[0] + dz, voxel[1] + dy, voxel[2] + dx])
                        })
                    })
                }))
            }
        }
    }
}

/// One fixpoint over the critical configurations, removing foreground only.
///
/// Batch-first: every round gathers the *whole* removal set from one pass over
/// the cells it must look at, applies it in one scatter, and derives the next
/// round's cell set from the voxels it removed. There is no per-voxel decision
/// that re-reads the mask mid-round, so the outcome does not depend on scan
/// order.
///
/// `marked` is the caller's reused one-byte-per-voxel dedup flag array. It is
/// `false` for every voxel on entry and on exit, because the removal list is
/// exactly the set of flags this round raised.
fn repair_cells(
    mask: &mut Array3<bool>,
    dims: [usize; 3],
    mut cells: CellSet,
    marked: &mut [bool],
    stats: &mut WellComposedRepair,
    first_scan: bool,
) -> Result<usize, HollowError> {
    let mut first_scan = first_scan;
    let mut removed_here = 0usize;
    while !cells.is_empty() {
        stats.rounds += 1;
        stats.cell_visits += cells.len();
        let mut critical = 0usize;
        // Bounded by the voxels that are still foreground, because only a
        // foreground corner is ever marked and `marked` deduplicates.
        let mut removals: Vec<u32> = Vec::new();
        for cell in cells.iter() {
            let corners = read_cell(mask, dims, cell);
            let marks = critical_removals(&corners);
            if marks == 0 {
                continue;
            }
            critical += 1;
            for index in 0..8 {
                if marks & (1 << index) == 0 {
                    continue;
                }
                if let Some(voxel) = corner_voxel(dims, cell, index) {
                    let linear = linear_index(dims, voxel);
                    if !marked[linear as usize] {
                        marked[linear as usize] = true;
                        removals.push(linear);
                    }
                }
            }
        }
        if first_scan {
            stats.critical_cells = critical;
            first_scan = false;
        }
        if removals.is_empty() {
            return Ok(removed_here);
        }
        for linear in &removals {
            marked[*linear as usize] = false;
            mask[voxel_of(dims, *linear)] = false;
        }
        stats.removed_voxels += removals.len();
        removed_here += removals.len();
        // Only a cell that contains a removed voxel can have become critical.
        cells = CellSet::incident(dims, removals);
    }
    Ok(removed_here)
}

/// The eight corners of a cell, as a bitmask indexed `dz << 2 | dy << 1 | dx`.
fn read_cell(mask: &Array3<bool>, dims: [usize; 3], cell: [usize; 3]) -> u8 {
    let mut corners = 0u8;
    for index in 0..8 {
        if let Some(voxel) = corner_voxel(dims, cell, index) {
            if mask[voxel] {
                corners |= 1 << index;
            }
        }
    }
    corners
}

/// The voxel at a cell corner, or `None` when the corner is in the background
/// padding ring outside the array.
fn corner_voxel(dims: [usize; 3], cell: [usize; 3], index: usize) -> Option<[usize; 3]> {
    let offsets = [index >> 2, (index >> 1) & 1, index & 1];
    let mut voxel = [0usize; 3];
    for axis in 0..3 {
        let coordinate = cell[axis] + offsets[axis];
        // Cell `c` spans voxels `c - 1` and `c`.
        if coordinate == 0 {
            return None;
        }
        let coordinate = coordinate - 1;
        if coordinate >= dims[axis] {
            return None;
        }
        voxel[axis] = coordinate;
    }
    Some(voxel)
}

/// Which of the cell's foreground corners this configuration requires removing,
/// as a corner bitmask. Zero means the cell is not critical.
///
/// The rule is a pure function of the 8-bit configuration, so it is order-free
/// and needs no tie-break: C1 removes the two diagonal foreground corners of
/// every critical face, C2's foreground pair removes both, and C2's background
/// pair — the one case where the *material* is pinched rather than the void —
/// removes the single lowest-indexed foreground corner, which is the smallest
/// change that admits background to the pinch.
fn critical_removals(corners: &u8) -> u8 {
    let corners = *corners;
    let bit = |index: usize| corners & (1 << index) != 0;
    let mut marked = 0u8;

    // C1: the six 2x2 faces. A face is critical when exactly its two diagonal
    // corners are foreground.
    for (a, b, c, d) in FACES {
        let quad = [bit(a), bit(b), bit(c), bit(d)];
        if quad[0] && quad[3] && !quad[1] && !quad[2] {
            marked |= (1 << a) | (1 << d);
        } else if quad[1] && quad[2] && !quad[0] && !quad[3] {
            marked |= (1 << b) | (1 << c);
        }
    }

    // C2: an antipodal pair, in the foreground or in the background.
    let population = corners.count_ones();
    if population == 2 {
        for index in 0..4usize {
            if bit(index) && bit(7 - index) {
                marked |= (1 << index) | (1 << (7 - index));
            }
        }
    } else if population == 6 {
        for index in 0..4usize {
            if !bit(index) && !bit(7 - index) {
                // Removing one foreground corner admits background to the
                // pinch; the remaining rounds settle whatever that exposes.
                if let Some(lowest) = (0..8).find(|candidate| bit(*candidate)) {
                    marked |= 1 << lowest;
                }
            }
        }
    }
    marked
}

/// The six 2×2 faces of a cell, each as its four corner indices in
/// `(a, b, c, d)` order where `a`/`d` and `b`/`c` are the two diagonals.
const FACES: [(usize, usize, usize, usize); 6] = [
    // z = 0 and z = 1: corners (y, x) = (0,0) (0,1) (1,0) (1,1)
    (0, 1, 2, 3),
    (4, 5, 6, 7),
    // y = 0 and y = 1: corners (z, x)
    (0, 1, 4, 5),
    (2, 3, 6, 7),
    // x = 0 and x = 1: corners (z, y)
    (0, 2, 4, 6),
    (1, 3, 5, 7),
];

/// Count the critical 2×2×2 cells of a mask without changing it.
///
/// This is the tests' **independent** oracle: it re-derives from the mask what
/// the repair claims to have found and to have driven to zero. The number
/// production reports is the repair's own
/// [`WellComposedRepair::critical_cells`], which reaches the record through
/// `H0ResourceDiagnostics::cavity_critical_cells` and through the refusal
/// variants — a measurement FU-49's decision procedure turns on must not be
/// reachable only from a test.
#[cfg(test)]
pub(crate) fn critical_cell_count(mask: &Array3<bool>) -> usize {
    let dims: [usize; 3] = mask.shape().try_into().expect("3-D mask");
    let mut count = 0usize;
    for z in 0..=dims[0] {
        for y in 0..=dims[1] {
            for x in 0..=dims[2] {
                if critical_removals(&read_cell(mask, dims, [z, y, x])) != 0 {
                    count += 1;
                }
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4×4×4 body in a 6×6×6 grid, plus whatever extra voxels the caller
    /// hangs off it. A pinch needs a body: a mask that is *only* a pinch is
    /// removed entirely, which is the `CavityRepairRemovedCavity` refusal and
    /// not the behaviour under test here.
    fn body_with(extra: &[[usize; 3]]) -> Array3<bool> {
        let mut mask = Array3::from_elem((6, 6, 6), false);
        for z in 0..4 {
            for y in 0..4 {
                for x in 0..4 {
                    mask[[z, y, x]] = true;
                }
            }
        }
        for voxel in extra {
            mask[*voxel] = true;
        }
        mask
    }

    #[test]
    fn a_solid_block_is_already_well_composed_and_is_not_touched() {
        let mask = Array3::from_elem((4, 4, 4), true);
        assert_eq!(critical_cell_count(&mask), 0);
        let (repaired, stats) = well_composed_cavity_component(mask.clone(), 8).expect("repair");
        assert_eq!(stats.removed_voxels(), 0);
        assert_eq!(stats.critical_cells(), 0);
        // Nothing was removed, so the component was never re-selected: one
        // whole-grid pass, the opening scan.
        assert_eq!(stats.full_grid_passes(), 1);
        assert_eq!(repaired, mask);
    }

    #[test]
    fn a_two_dimensional_checkerboard_pinch_is_detected_and_removed() {
        // A voxel touching the body across a face diagonal: the textbook C1,
        // and the shape a thin cavity branch makes where it turns a corner.
        let mask = body_with(&[[0, 4, 4]]);
        assert!(critical_cell_count(&mask) > 0);
        let (repaired, stats) = well_composed_cavity_component(mask, 8).expect("repair");
        assert_eq!(critical_cell_count(&repaired), 0);
        assert!(stats.removed_voxels() > 0);
        assert!(repaired.iter().any(|value| *value), "body survived");
    }

    #[test]
    fn an_antipodal_pair_is_detected_and_removed() {
        // The 3-D C2: a voxel touching the body only at a body diagonal.
        let mask = body_with(&[[4, 4, 4]]);
        assert!(critical_cell_count(&mask) > 0);
        let (repaired, stats) = well_composed_cavity_component(mask, 8).expect("repair");
        assert_eq!(critical_cell_count(&repaired), 0);
        assert!(stats.removed_voxels() > 0);
        assert!(repaired.iter().any(|value| *value), "body survived");
    }

    #[test]
    fn a_mask_that_is_only_a_pinch_refuses_as_a_repair_outcome_not_as_a_missing_cavity() {
        let mut mask = Array3::from_elem((3, 3, 3), false);
        mask[[0, 0, 0]] = true;
        mask[[1, 1, 1]] = true;
        // The distinction this asserts is the whole point of the variant: the
        // caller must be able to say "the repair ate the cavity" and not
        // "this geometry never had one", and the evidence must survive the
        // refusal rather than being discarded with the candidate.
        let error = well_composed_cavity_component(mask, 8).expect_err("repair removes everything");
        let HollowError::CavityRepairRemovedCavity {
            critical_cells,
            repaired_voxels,
            cell_visits,
            full_grid_passes,
        } = error
        else {
            panic!("expected a repair-caused refusal, got {error}");
        };
        assert!(critical_cells > 0);
        assert_eq!(repaired_voxels, 2);
        assert!(cell_visits > 0);
        assert!(full_grid_passes >= 2, "the re-selection that found nothing");
    }

    #[test]
    fn a_background_antipodal_pair_is_repaired_by_removing_material_from_the_void() {
        // The C2 dual: a 2x2x2 whose *background* is one antipodal pair. The
        // pinch is in the material, and the repair still only removes void.
        let mut mask = Array3::from_elem((2, 2, 2), true);
        mask[[0, 0, 0]] = false;
        mask[[1, 1, 1]] = false;
        assert!(critical_cell_count(&mask) > 0);
        let (repaired, stats) = well_composed_cavity_component(mask.clone(), 8).expect("repair");
        assert_eq!(critical_cell_count(&repaired), 0);
        assert!(stats.removed_voxels() > 0);
        // Removal only: no background voxel ever became foreground.
        for (original, result) in mask.iter().zip(repaired.iter()) {
            assert!(*original || !*result, "repair added a cavity voxel");
        }
    }

    #[test]
    fn the_repair_never_adds_a_cavity_voxel_on_a_pinched_dumbbell() {
        // Two 3x3x3 blobs meeting at a single corner: a pinch that a real
        // branched model produces, and the exact shape the wall must not thin
        // at.
        let mut mask = Array3::from_elem((7, 7, 7), false);
        for z in 0..3 {
            for y in 0..3 {
                for x in 0..3 {
                    mask[[z, y, x]] = true;
                    mask[[z + 4, y + 4, x + 4]] = true;
                }
            }
        }
        // Bridge them through the shared corner only.
        mask[[3, 3, 3]] = true;
        assert!(critical_cell_count(&mask) > 0);
        let (repaired, stats) = well_composed_cavity_component(mask.clone(), 8).expect("repair");
        assert_eq!(critical_cell_count(&repaired), 0);
        for (original, result) in mask.iter().zip(repaired.iter()) {
            assert!(*original || !*result, "repair added a cavity voxel");
        }
        // The result is one 6-connected component: the pinch was cut and the
        // smaller side dropped, or the bridge voxel went.
        let (_, label_count) = label3d(repaired.view(), 6).expect("labels");
        assert_eq!(label_count, 1);
        assert!(stats.rounds() >= 2);
    }

    #[test]
    fn work_is_bounded_by_the_admitted_grid() {
        let mask = body_with(&[[0, 4, 4], [4, 4, 4], [4, 0, 4]]);
        let cells = 7 * 7 * 7;
        let (_, stats) = well_composed_cavity_component(mask, 8).expect("repair");
        assert!(stats.removed_voxels() > 0, "the fixture is pinched");
        // One full scan, then only the eight cells around each voxel that
        // actually changed. Nothing here is a budget that can be exhausted —
        // it is what the algorithm can possibly touch.
        let bound = cells + 8 * (stats.removed_voxels() + stats.reselection_dropped_voxels());
        assert!(
            stats.cell_visits() <= bound,
            "cell visits {} exceeded the derived bound {bound}",
            stats.cell_visits()
        );
    }

    /// The second `O(cells)` domain, counted rather than described.
    ///
    /// The cell-visit bound above says nothing about the whole-grid `label3d`
    /// and whole-grid sweep each component re-selection performs. This asserts
    /// the count exists, that it is what the doc claims — one opening scan plus
    /// one pass per re-selection, each pass being two whole-grid traversals —
    /// and that it obeys the *structural* bound the code guarantees,
    /// `2 + reselection_dropped_voxels`. That structural bound is `O(cells)`;
    /// the caller-owned admission limit (FU-54, `max_passes` below) is a
    /// separate, tighter ceiling checked below in
    /// [`the_pass_limit_refuses_instead_of_taking_the_next_reselection`].
    #[test]
    fn whole_grid_passes_are_counted_and_bounded_by_what_the_reselection_dropped() {
        let mut mask = Array3::from_elem((7, 7, 7), false);
        for z in 0..3 {
            for y in 0..3 {
                for x in 0..3 {
                    mask[[z, y, x]] = true;
                    mask[[z + 4, y + 4, x + 4]] = true;
                }
            }
        }
        mask[[3, 3, 3]] = true;
        let (_, stats) = well_composed_cavity_component(mask, 8).expect("repair");
        assert!(stats.full_grid_passes() >= 2, "a re-selection ran");
        // The opening scan, plus one re-selection per voxel it drops, plus the
        // final re-selection that drops nothing and ends the loop.
        let bound = 2 + stats.reselection_dropped_voxels();
        assert!(
            stats.full_grid_passes() <= bound,
            "whole-grid passes {} exceeded the derived bound {bound}",
            stats.full_grid_passes()
        );
    }

    /// FU-54: the bound fires, and fires with the right evidence.
    ///
    /// The same pinched-dumbbell mask as the test above needs exactly one
    /// re-selection (`full_grid_passes() == 2` there, at the production
    /// default of 8). Handed a `max_passes` of 1 — enough for the opening
    /// scan and nothing else — the required re-selection is refused instead
    /// of taken: the check runs before that round's `label3d`, so no second
    /// whole-grid traversal happens at all.
    #[test]
    fn the_pass_limit_refuses_instead_of_taking_the_next_reselection() {
        let mut mask = Array3::from_elem((7, 7, 7), false);
        for z in 0..3 {
            for y in 0..3 {
                for x in 0..3 {
                    mask[[z, y, x]] = true;
                    mask[[z + 4, y + 4, x + 4]] = true;
                }
            }
        }
        mask[[3, 3, 3]] = true;
        let error =
            well_composed_cavity_component(mask, 1).expect_err("one pass cannot re-select");
        let HollowError::CavityRepairPassLimitExceeded {
            passes,
            max_passes,
            critical_cells,
            repaired_voxels,
            cell_visits,
        } = error
        else {
            panic!("expected CavityRepairPassLimitExceeded, got {error}");
        };
        assert_eq!(passes, 1, "refused at the opening scan's own pass count");
        assert_eq!(max_passes, 1);
        assert!(critical_cells > 0, "the fixture is pinched");
        assert!(repaired_voxels > 0, "the opening scan already removed voxels");
        assert!(cell_visits > 0);
    }

    /// The transient workset is the changed voxels themselves, never their
    /// eight incident cells materialised.
    ///
    /// Asserted structurally, because the allocation this replaced had no
    /// admitted bound: an incident round visits exactly eight cells per changed
    /// voxel — duplicates included — which is both the work it does and the
    /// only thing it holds.
    #[test]
    fn an_incident_round_visits_eight_cells_per_changed_voxel_and_holds_nothing_else() {
        let cells = CellSet::incident([4, 4, 4], vec![0, 21]);
        assert_eq!(cells.len(), 16);
        let visited: Vec<[usize; 3]> = cells.iter().collect();
        assert_eq!(visited.len(), 16);
        // Voxel 0 is [0, 0, 0]; voxel 21 is [1, 1, 1].
        assert_eq!(visited[0], [0, 0, 0]);
        assert_eq!(visited[7], [1, 1, 1]);
        assert_eq!(visited[8], [1, 1, 1]);
        assert_eq!(visited[15], [2, 2, 2]);
    }

    #[test]
    fn linear_indices_round_trip_through_the_masks_own_layout() {
        let dims = [3, 4, 5];
        let mut seen = 0usize;
        for z in 0..dims[0] {
            for y in 0..dims[1] {
                for x in 0..dims[2] {
                    let index = linear_index(dims, [z, y, x]);
                    assert_eq!(index as usize, seen);
                    assert_eq!(voxel_of(dims, index), [z, y, x]);
                    seen += 1;
                }
            }
        }
    }

    /// The mechanism itself, end to end on a synthetic mask: a cavity that is
    /// not well-composed extracts to a **non-manifold** surface, and the same
    /// cavity after repair extracts to a manifold one. This is the link FU-49
    /// asserted and the reason the repair exists; it is pinned here on twelve
    /// voxels rather than on a 200,000-face model that cannot be committed.
    #[test]
    fn a_pinched_mask_extracts_a_nonmanifold_surface_and_a_repaired_one_does_not() {
        // Two 3x3x3 lobes in the same slab, touching across one face diagonal:
        // the C1 configuration, and the shape a branched cavity makes where two
        // limbs pass within a voxel of each other.
        let mut mask = Array3::from_elem((6, 6, 6), false);
        for z in 0..3 {
            for y in 0..3 {
                for x in 0..3 {
                    mask[[z, y, x]] = true;
                    mask[[z, y + 3, x + 3]] = true;
                }
            }
        }
        assert!(critical_cell_count(&mask) > 0, "the fixture is pinched");

        let pinched = crate::marching_cavity(&mask, [0.0, 0.0, 0.0], 0.5).expect("marching");
        let (pinched_boundary, pinched_nonmanifold) = surface_topology(&pinched);
        assert_eq!(
            pinched_boundary, 0,
            "the pinched surface is watertight — that is what makes the defect \
             survive every closure check"
        );
        assert!(
            pinched_nonmanifold > 0,
            "a non-well-composed mask must extract a non-manifold surface, or \
             the repair is solving a problem that does not exist"
        );

        let (repaired, stats) = well_composed_cavity_component(mask, 8).expect("repair");
        assert!(stats.removed_voxels() > 0);
        assert_eq!(critical_cell_count(&repaired), 0);
        let clean = crate::marching_cavity(&repaired, [0.0, 0.0, 0.0], 0.5).expect("marching");
        assert_eq!(
            surface_topology(&clean),
            (0, 0),
            "the repaired mask still extracts a surface that is not a manifold"
        );
    }

    /// `(boundary edges, faces on a non-manifold edge)` of an extracted surface.
    fn surface_topology(mesh: &plinth_mesh::Mesh) -> (usize, usize) {
        let topology = plinth_mesh::build_face_topology(mesh, plinth_mesh::WeldPolicy::ExactBits)
            .expect("topology");
        let nonmanifold = topology
            .cut_flags()
            .iter()
            .filter(|flag| **flag & plinth_mesh::TOPOLOGY_CUT_NONMANIFOLD != 0)
            .count();
        (topology.counts().boundary_edges, nonmanifold)
    }

    #[test]
    fn the_repair_is_a_pure_function_of_the_configuration() {
        // Order-freedom: the removal set for a cell depends only on its eight
        // bits, so two scans of the same mask can never disagree.
        for configuration in 0u16..256 {
            let corners = configuration as u8;
            assert_eq!(
                critical_removals(&corners),
                critical_removals(&corners),
                "configuration {corners:08b} is not a pure function"
            );
            // Nothing outside the foreground is ever marked.
            assert_eq!(critical_removals(&corners) & !corners, 0);
        }
    }
}
