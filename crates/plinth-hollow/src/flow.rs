//! Resin-flow cavity connectivity.
//!
//! Per the owner ruling of 2026-08-12
//! (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`,
//! §4, FU-47): two interior voxels are part of the same drainable cavity only
//! if resin can actually flow between them — a passage whose free radius
//! admits the registered drain bore. A zero-width point never connects two
//! interiors, and neither does a passage narrower than the bore: each side is
//! its own cavity.
//!
//! **The threshold is read from the registry, never restated.**
//! [`registered_throat_radius_mm`] pulls `HOLLOW_DRAIN_R_MM` — the same
//! constant [`crate::drains`] drills to — exactly as `drains.rs` already does.
//!
//! **The algorithm, batch-first.**
//!
//! 1. [`crate::frozen_distance_transform`] gives every cavity voxel its free
//!    radius: the mm distance to the nearest non-cavity voxel. This is the
//!    *existing* padded-border EDT, reused rather than reimplemented — it
//!    pads with background, which is exactly the boundary condition a cavity
//!    mask needs too.
//! 2. The **core** is the set of cavity voxels whose free radius admits the
//!    registered bore, measured against the *realized* cavity wall rather
//!    than against the background voxel centre the EDT samples:
//!    [`core_clearance_threshold_mm`]. That half-voxel correction is a
//!    tightening and moves no registered constant — see its own doc.
//! 3. `label3d` over the core gives one label per resin-flow-connected
//!    interior. Two interiors joined only by a sub-bore passage get different
//!    labels here, which is the ruling's separation rule.
//! 4. Every other cavity voxel is offered to a core label by a
//!    **multi-source, wave-synchronous breadth-first flood** seeded by every
//!    core voxel at once, and the flood may only run **downhill in
//!    clearance**: a step from `u` to `v` is admitted only when
//!    `clearance[v] <= clearance[u]`. A voxel is claimed in the first wave
//!    that reaches it, and a voxel reached by more than one label in the same
//!    wave takes the **minimum** label — so the result does not depend on
//!    scan order, the house determinism standard (`repair.rs`'s module doc).
//!    Each zone is 6-connected by construction, since a voxel only ever
//!    inherits a label from a 6-neighbour one wave earlier.
//!
//!    **Why downhill.** Sub-bore dead space exists in every real cavity — the
//!    corners of a box, the taper where a wall meets a wall — and it is not a
//!    separate interior: resin in it runs *out* into the bore-admitting space
//!    it hangs off. What the ruling forbids is the opposite motion, climbing
//!    back *up* through a throat into a second interior that nowhere admits
//!    the bore. An unbounded flood cannot tell the two apart and annexes the
//!    second one, certifying as drainable a pocket whose only exit is
//!    narrower than the drain the pipeline drills. The downhill rule admits
//!    the first and refuses the second with one comparison.
//! 5. **Stranding.** A pocket sitting behind a throat is wider than the
//!    throat, so step 4 leaves its interior unlabelled — but the skin of that
//!    pocket, whose own clearance is at or below the throat's, would still be
//!    claimed through the neck. So any claimed voxel with an unlabelled
//!    cavity neighbour that is **strictly wider** than it is unlabelled too,
//!    to fixpoint: it lies on the stranded side of the same throat. The
//!    strictness is what stops the cascade at the throat — leaving a throat
//!    is uphill, and a flat wall region is not strictly wider — and a core
//!    voxel can never be reached by it (a strictly wider unlabelled
//!    neighbour of a core voxel would itself be core, and core voxels are
//!    labelled from the seed).
//! 6. **Re-flood.** Unlabelling in step 5 can strand a further region that
//!    was only reachable *through* what step 5 removed, so the downhill flood
//!    is run once more over the survivors. It can only remove, never add.
//! 7. A cavity voxel still unlabelled (`0`) is not selectable: it cannot be
//!    drained, so leaving it uncut leaves solid material, which is always
//!    safe.
//!
//! **Known residual.** Steps 5 and 6 are run once each, not iterated as a
//! pair, because a pair iterated to fixpoint has no `O(cells)` bound. A
//! region stranded *by step 6* can therefore still keep a skin at or below
//! its own throat's clearance. That is a smaller version of the same defect,
//! always in the safe direction relative to today's unbounded flood (fewer
//! voxels cut, never more), and it is called out here rather than left to be
//! discovered.
//!
//! Everything here is `O(cells)`: [`flow_labels`] makes **ten whole-grid
//! traversals** — the EDT, the core build, its `label3d`, the seed scan, the
//! flood, the unlabelled sweep that seeds stranding, the stranding fixpoint,
//! the survivor-capture-and-reset pass, the re-flood, and the unreached
//! sweep — and [`select_largest_flow_component`] adds the `largest_label`
//! scan and the verdict `mapv`, **twelve in all**. No call in the flood is
//! per-voxel-scalar in the sense D-37 forbids — the frontier is carried as a
//! batch (`Vec<u32>` of linear indices, mirroring `well_composed.rs`'s workset
//! convention) and every voxel is finalized at most twice (once per flood).
//!
//! **Transient memory, stated rather than inferred.** Over the admitted grid:
//! the `f64` clearance field the EDT already returns (8 B/voxel, now held for
//! the whole call because the downhill rule reads it), the `bool` core mask
//! and the `bool` survivor mask (1 B each), the `i32` label array and the
//! `i32` tie-break overlay (4 B each), and the `i32` core labels `label3d`
//! returns, dropped as soon as the seed frontier is built. Peak is ~18
//! B/voxel — 300 MB at `DEFAULT_MAX_VOXEL_CELLS`, the same order as the EDT
//! the caller already paid for. Frontiers are `u32` linear indices and hold
//! at most one entry per voxel.

use scirs2_core::ndarray::Array3;
use scirs2_ndimage::morphology3d::label3d;

use plinth_core::m0_constants;

use crate::{frozen_distance_transform, largest_label, HollowError};

/// Evidence from one flow-connectivity pass, for the resource record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FlowStats {
    /// Resin-flow-connected interiors found (the core's own component count).
    pub(crate) flow_components: usize,
    /// Cavity voxels whose free radius admits the registered bore outright.
    pub(crate) core_voxels: usize,
    /// Cavity voxels the flood could not reach from any core: a 6-connected
    /// region that nowhere admits the bore. Left unselected in every zone.
    pub(crate) unreached_voxels: usize,
}

/// The registered minimum passage radius: `HOLLOW_DRAIN_R_MM`, read fresh
/// rather than restated. Same call `drains.rs` makes for the drilled bore
/// itself.
pub(crate) fn registered_throat_radius_mm() -> Result<f64, HollowError> {
    Ok(m0_constants()
        .map_err(HollowError::Constants)?
        .placeholders()
        .hollow_drain_r_mm()
        .diagnostic()
        .get())
}

/// The clearance a cavity voxel must read for the registered bore to fit
/// beside the *realized* cavity wall.
///
/// [`crate::frozen_distance_transform`] measures centre to centre: from a
/// cavity voxel's centre to the centre of the nearest non-cavity voxel. The
/// wall the pipeline actually cuts is not there — `surface_nets` places the
/// cavity surface halfway between a cavity sample and the solid sample beside
/// it, half a pitch *inside* that background centre. So the free radius a
/// voxel really has is `clearance − pitch/2`, and the bore fits only when
/// `clearance >= throat_r_mm + pitch/2`.
///
/// **This moves no registered constant and is a tightening, not a
/// loosening.** `HOLLOW_DRAIN_R_MM` is untouched; what changes is the
/// discretization bias in how it is compared, and it changes in the direction
/// that refuses passages the raw comparison would have admitted. Without the
/// correction, a five-sample-wide passage at the 0.5 mm pitch reads exactly
/// 1.5 mm of clearance and passes for the 3.0 mm bore while its realized
/// aperture is 2.5 mm. Quantization means the correction can also refuse a
/// passage whose realized aperture is exactly 3.0 mm; that error is on the
/// safe side, which is the side this rule is required to err on.
pub(crate) fn core_clearance_threshold_mm(throat_r_mm: f64, pitch_mm: f64) -> f64 {
    throat_r_mm + 0.5 * pitch_mm
}

/// Label every voxel of `mask` by which resin-flow-connected interior it
/// belongs to. `0` means unreached (not selectable). Labels `1..=flow_components`
/// are the core's own component ids from `label3d`, so they are stable and
/// meaningful on their own — no relabelling pass follows the flood.
pub(crate) fn flow_labels(
    mask: &Array3<bool>,
    pitch_mm: f64,
    throat_r_mm: f64,
) -> Result<(Array3<i32>, FlowStats), HollowError> {
    let dims: [usize; 3] = mask.shape().try_into().map_err(|_| HollowError::VoxelStack)?;
    let voxel_count = dims[0]
        .checked_mul(dims[1])
        .and_then(|value| value.checked_mul(dims[2]))
        .ok_or(HollowError::VoxelGridTooLarge)?;
    u32::try_from(voxel_count).map_err(|_| HollowError::VoxelGridTooLarge)?;

    let clearance = frozen_distance_transform(mask, pitch_mm)?;
    let core_threshold_mm = core_clearance_threshold_mm(throat_r_mm, pitch_mm);
    let core = Array3::from_shape_fn(clearance.raw_dim(), |index| {
        mask[index] && clearance[index] >= core_threshold_mm
    });
    let (core_labels, flow_components) =
        label3d(core.view(), 6).map_err(|_| HollowError::VoxelStack)?;

    let mut labels = Array3::<i32>::zeros((dims[0], dims[1], dims[2]));
    let mut stats = FlowStats {
        flow_components,
        ..FlowStats::default()
    };
    if flow_components == 0 {
        stats.unreached_voxels = mask.iter().filter(|value| **value).count();
        return Ok((labels, stats));
    }

    // Seed the flood: every core voxel starts already labelled and already
    // in the initial frontier. One pass over the whole grid, not one call per
    // core voxel.
    let mut frontier: Vec<u32> = Vec::new();
    for (index, &label) in core_labels.indexed_iter() {
        if label > 0 {
            let voxel = [index.0, index.1, index.2];
            labels[voxel] = label;
            frontier.push(linear_index(dims, voxel));
        }
    }
    stats.core_voxels = frontier.len();
    drop(core_labels);

    // One overlay slot per voxel, reused across every wave and both floods,
    // and cleared only for the entries a wave actually touches -- never
    // reallocated and never swept in full more than the one time it is
    // created.
    let mut overlay = vec![0_i32; voxel_count];
    downhill_flood(mask, &clearance, &mut labels, &mut overlay, frontier, dims);

    // Everything the flood could not claim is on the stranded side of some
    // throat. Push that verdict back over the skin the flood did claim
    // through the same throat.
    let stranded_seed: Vec<u32> = mask
        .indexed_iter()
        .filter_map(|(index, occupied)| {
            let voxel = [index.0, index.1, index.2];
            (*occupied && labels[voxel] == 0).then(|| linear_index(dims, voxel))
        })
        .collect();
    unclaim_stranded(mask, &clearance, &mut labels, dims, stranded_seed);

    // Re-flood the survivors: stranding can cut the only downhill route to a
    // region that is otherwise fine, and a zone that no longer reaches its
    // core is not drainable through it either. One pass captures what
    // survived and clears every non-core label so the flood can re-derive it.
    let mut admitted = Array3::<bool>::default((dims[0], dims[1], dims[2]));
    let mut frontier: Vec<u32> = Vec::with_capacity(stats.core_voxels);
    for (index, is_core) in core.indexed_iter() {
        let voxel = [index.0, index.1, index.2];
        admitted[voxel] = labels[voxel] != 0;
        if *is_core {
            frontier.push(linear_index(dims, voxel));
        } else {
            labels[voxel] = 0;
        }
    }
    drop(core);
    downhill_flood(&admitted, &clearance, &mut labels, &mut overlay, frontier, dims);

    stats.unreached_voxels = mask
        .iter()
        .zip(labels.iter())
        .filter(|(occupied, label)| **occupied && **label == 0)
        .count();
    Ok((labels, stats))
}

/// Multi-source wave-synchronous flood from an already-labelled frontier,
/// admitted only where `admitted` is set and only **downhill in clearance**.
///
/// Ties inside one wave take the minimum label, so the result does not depend
/// on scan order. `overlay` is the caller's reusable one-slot-per-voxel
/// scratch and is left all-zero on return.
fn downhill_flood(
    admitted: &Array3<bool>,
    clearance: &Array3<f64>,
    labels: &mut Array3<i32>,
    overlay: &mut [i32],
    mut frontier: Vec<u32>,
    dims: [usize; 3],
) {
    let mut touched: Vec<u32> = Vec::new();
    while !frontier.is_empty() {
        for &linear in &frontier {
            let voxel = voxel_of(dims, linear);
            let label = labels[voxel];
            let here = clearance[voxel];
            for neighbor in six_neighbors(dims, voxel) {
                if !admitted[neighbor] || labels[neighbor] != 0 || clearance[neighbor] > here {
                    continue;
                }
                let neighbor_linear = linear_index(dims, neighbor) as usize;
                let slot = &mut overlay[neighbor_linear];
                if *slot == 0 {
                    *slot = label;
                    touched.push(neighbor_linear as u32);
                } else if label < *slot {
                    *slot = label;
                }
            }
        }
        let mut next_frontier = Vec::with_capacity(touched.len());
        for &linear in &touched {
            let voxel = voxel_of(dims, linear);
            labels[voxel] = overlay[linear as usize];
            overlay[linear as usize] = 0;
            next_frontier.push(linear);
        }
        touched.clear();
        frontier = next_frontier;
    }
}

/// Spread the unlabelled verdict downhill: a labelled voxel with an
/// unlabelled cavity neighbour that is **strictly wider** than it is on the
/// stranded side of the same throat, so it is unlabelled too, to fixpoint.
///
/// Strictness is what bounds this. The relation only ever runs from a wider
/// voxel to a narrower one, so it stops the moment it would have to leave a
/// throat (uphill) and never crosses a flat wall region; and each voxel is
/// unlabelled at most once, which is what makes the fixpoint `O(cells)`.
fn unclaim_stranded(
    mask: &Array3<bool>,
    clearance: &Array3<f64>,
    labels: &mut Array3<i32>,
    dims: [usize; 3],
    mut frontier: Vec<u32>,
) {
    while !frontier.is_empty() {
        let mut next_frontier: Vec<u32> = Vec::new();
        for &linear in &frontier {
            let voxel = voxel_of(dims, linear);
            let here = clearance[voxel];
            for neighbor in six_neighbors(dims, voxel) {
                // A strictly wider unlabelled neighbour of a core voxel would
                // itself be core, and core voxels are labelled from the seed,
                // so this never reaches the core.
                if !mask[neighbor] || labels[neighbor] == 0 || clearance[neighbor] >= here {
                    continue;
                }
                labels[neighbor] = 0;
                next_frontier.push(linear_index(dims, neighbor));
            }
        }
        frontier = next_frontier;
    }
}

/// The largest resin-flow-connected interior, or `None` when nothing in
/// `mask` admits the registered bore at all.
pub(crate) fn select_largest_flow_component(
    mask: &Array3<bool>,
    pitch_mm: f64,
    throat_r_mm: f64,
) -> Result<Option<(Array3<bool>, FlowStats)>, HollowError> {
    let (labels, stats) = flow_labels(mask, pitch_mm, throat_r_mm)?;
    if stats.flow_components == 0 {
        return Ok(None);
    }
    let largest = largest_label(&labels, stats.flow_components)?;
    let selected = labels.mapv(|label| label == largest);
    Ok(Some((selected, stats)))
}

fn linear_index(dims: [usize; 3], voxel: [usize; 3]) -> u32 {
    let index = (voxel[0] * dims[1] + voxel[1]) * dims[2] + voxel[2];
    // The voxel count was already proven to fit `u32` before any index was
    // taken (`flow_labels`'s admission line), so this cannot truncate.
    index as u32
}

const fn voxel_of(dims: [usize; 3], index: u32) -> [usize; 3] {
    let index = index as usize;
    let x = index % dims[2];
    let rest = index / dims[2];
    [rest / dims[1], rest % dims[1], x]
}

/// The up-to-six 6-connected neighbours of `voxel` that lie inside `dims`.
fn six_neighbors(dims: [usize; 3], voxel: [usize; 3]) -> impl Iterator<Item = [usize; 3]> {
    let [z, y, x] = voxel;
    let [nz, ny, nx] = dims;
    [
        (z > 0).then(|| [z - 1, y, x]),
        (z + 1 < nz).then(|| [z + 1, y, x]),
        (y > 0).then(|| [z, y - 1, x]),
        (y + 1 < ny).then(|| [z, y + 1, x]),
        (x > 0).then(|| [z, y, x - 1]),
        (x + 1 < nx).then(|| [z, y, x + 1]),
    ]
    .into_iter()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `[z][y][x]` axis-aligned box of `mask = true`, `dims` cells to a
    /// side, centred so callers can carve a neck out of it.
    fn full_block(dims: [usize; 3]) -> Array3<bool> {
        Array3::from_elem((dims[0], dims[1], dims[2]), true)
    }

    /// A block wide enough that its own centre clears the registered bore at
    /// the given pitch: `radius_voxels` voxel steps from every face.
    fn wide_block(radius_voxels: usize) -> Array3<bool> {
        full_block([radius_voxels * 2 + 3; 3])
    }

    /// **Test 1 — one solid blob wider than the bore selects everything.**
    /// Proves the rule does not erode the cavity: the whole mask is one core,
    /// one flow component, every voxel selected.
    #[test]
    fn a_blob_wider_than_the_bore_selects_every_voxel() {
        let pitch = 0.5;
        let throat = 1.5; // 3 voxel steps at 0.5 mm.
        let mask = wide_block(4);
        let (selected, stats) =
            select_largest_flow_component(&mask, pitch, throat).expect("flow ok").expect("some");
        assert_eq!(stats.flow_components, 1);
        assert_eq!(stats.unreached_voxels, 0);
        assert_eq!(selected, mask, "not one voxel of the cavity was eroded");
    }

    /// Two blobs joined by a neck whose free radius is `neck_half_width`
    /// voxel steps everywhere along its length.
    fn dumbbell(neck_half_width: usize) -> Array3<bool> {
        let lobe = 6_usize; // half-width of each lobe, well above any throat under test
        let neck_len = 5_usize;
        let ny = lobe * 2 + 1;
        let nz = ny;
        let nx = lobe * 2 + neck_len + lobe * 2;
        let mut mask = Array3::from_elem((nz, ny, nx), false);
        let center = (lobe, lobe);
        for z in 0..nz {
            for y in 0..ny {
                let in_left_lobe = (z as isize - lobe as isize).unsigned_abs() <= lobe
                    && (y as isize - lobe as isize).unsigned_abs() <= lobe;
                if in_left_lobe {
                    for x in 0..(lobe * 2 + 1) {
                        mask[[z, y, x]] = true;
                    }
                }
            }
        }
        let right_start = lobe * 2 + neck_len;
        for z in 0..nz {
            for y in 0..ny {
                let in_right_lobe = (z as isize - lobe as isize).unsigned_abs() <= lobe
                    && (y as isize - lobe as isize).unsigned_abs() <= lobe;
                if in_right_lobe {
                    for x in right_start..nx {
                        mask[[z, y, x]] = true;
                    }
                }
            }
        }
        for z in 0..nz {
            for y in 0..ny {
                let dz = (z as isize - center.0 as isize).unsigned_abs();
                let dy = (y as isize - center.1 as isize).unsigned_abs();
                if dz <= neck_half_width && dy <= neck_half_width {
                    for x in (lobe * 2)..right_start {
                        mask[[z, y, x]] = true;
                    }
                }
            }
        }
        mask
    }

    /// **Test 2 — a neck below the registered throat splits the dumbbell into
    /// two flow components**, each 6-connected.
    #[test]
    fn a_sub_throat_neck_splits_the_dumbbell_into_two_components() {
        let pitch = 0.5;
        let throat = 1.5;
        // neck_half_width=1 at pitch 0.5 -> neck cross-section is 3 voxels
        // wide (1.0 mm), and the EDT free radius at its centre is well under
        // 1.5 mm -- narrower than the registered throat.
        let mask = dumbbell(1);
        let (labels, stats) = flow_labels(&mask, pitch, throat).expect("flow ok");
        assert_eq!(stats.flow_components, 2, "{stats:?}");
        // Every labelled voxel's label matches at least one 6-neighbour's
        // label (or it is alone), which is what "each zone is 6-connected by
        // construction" means operationally.
        for (index, &label) in labels.indexed_iter() {
            if label == 0 {
                continue;
            }
            let voxel = [index.0, index.1, index.2];
            let has_matching_neighbor = six_neighbors(
                labels.shape().try_into().expect("3 dims"),
                voxel,
            )
            .any(|neighbor| labels[neighbor] == label);
            let component_size = labels.iter().filter(|&&l| l == label).count();
            assert!(
                has_matching_neighbor || component_size == 1,
                "voxel {voxel:?} label {label} is isolated inside a larger component"
            );
        }
    }

    /// **Test 3 — the same pair joined by a neck at or above the registered
    /// throat is one component.** Straddles the threshold with test 2.
    #[test]
    fn a_neck_at_the_throat_keeps_the_dumbbell_as_one_component() {
        let pitch = 0.5;
        let throat = 1.5;
        // neck_half_width=3 at pitch 0.5 -> neck cross-section is 7 voxels
        // wide (3.5 mm), comfortably admitting the 1.5 mm throat radius.
        let mask = dumbbell(3);
        let (_, stats) = flow_labels(&mask, pitch, throat).expect("flow ok");
        assert_eq!(stats.flow_components, 1, "{stats:?}");
    }

    /// **Test 4 — two cubes touching at a single lattice-diagonal corner
    /// voxel** (the zero-width point case the ruling names) are two
    /// components. Each cube is large enough to have its own core (so the
    /// split is provably the point-touch, not a starved core), and they share
    /// no face or edge -- only the one corner where `(6,6,6)` and `(7,7,7)`
    /// meet diagonally, which 6-connectivity never bridges.
    #[test]
    fn a_diagonal_point_touch_is_two_components() {
        let pitch = 0.5;
        let throat = 1.5;
        let mut mask = Array3::from_elem((14, 14, 14), false);
        for z in 0..7 {
            for y in 0..7 {
                for x in 0..7 {
                    mask[[z, y, x]] = true;
                }
            }
        }
        for z in 7..14 {
            for y in 7..14 {
                for x in 7..14 {
                    mask[[z, y, x]] = true;
                }
            }
        }
        let (_, stats) = flow_labels(&mask, pitch, throat).expect("flow ok");
        assert_eq!(stats.flow_components, 2, "{stats:?}");
    }

    /// **Test 5 — a mask everywhere thinner than the bore has no core at
    /// all.**
    #[test]
    fn a_mask_thinner_than_the_bore_everywhere_has_no_core() {
        let pitch = 0.5;
        let throat = 1.5;
        // A 1-voxel-thick sheet: free radius is at most half a voxel step
        // anywhere in it, far under a 1.5 mm throat.
        let mask = Array3::from_elem((1, 20, 20), true);
        let selected = select_largest_flow_component(&mask, pitch, throat).expect("flow ok");
        assert!(selected.is_none(), "{selected:?}");
    }

    /// **Test 6 — determinism.** A mirrored/transposed copy of the same
    /// shape gets the corresponding labelling, and running twice is bit
    /// identical.
    #[test]
    fn flow_labelling_is_deterministic_and_transpose_invariant() {
        let pitch = 0.5;
        let throat = 1.5;
        let mask = dumbbell(1);
        let (first, stats_first) = flow_labels(&mask, pitch, throat).expect("flow ok");
        let (second, stats_second) = flow_labels(&mask, pitch, throat).expect("flow ok");
        assert_eq!(first, second);
        assert_eq!(stats_first, stats_second);

        // Mirror along x: the mask is symmetric front-to-back by
        // construction (two equal lobes), so the mirrored mask is identical
        // and the component count must be too.
        let dims = mask.raw_dim();
        let mut mirrored = Array3::from_elem(dims, false);
        let nx = dims[2];
        for ((z, y, x), value) in mask.indexed_iter() {
            mirrored[[z, y, nx - 1 - x]] = *value;
        }
        let (_, stats_mirrored) = flow_labels(&mirrored, pitch, throat).expect("flow ok");
        assert_eq!(stats_mirrored.flow_components, stats_first.flow_components);
    }

    /// **Test 7a — the throat comparison carries the half-voxel surface
    /// offset, and carries it exactly.**
    ///
    /// `dumbbell(2)` at the 0.5 mm pitch is a five-sample-wide neck: its
    /// centre reads exactly 1.5 mm of centre-to-centre clearance, while the
    /// surface `marching_cavity` actually cuts sits half a voxel inside the
    /// background centres on each side — a realized aperture of 2.5 mm, under
    /// the 3.0 mm bore. So the registered throat must *split* this dumbbell.
    /// A comparison written against the raw EDT reads 1.5 >= 1.5 and joins it.
    #[test]
    fn a_five_sample_neck_is_below_the_registered_throat_once_the_surface_offset_is_taken() {
        let pitch = 0.5;
        let throat = 1.5;
        let mask = dumbbell(2);
        let (_, stats) = flow_labels(&mask, pitch, throat).expect("flow ok");
        assert_eq!(stats.flow_components, 2, "{stats:?}");
    }

    /// **Test 7b — the threshold is `>=`, pinned at exact equality.**
    ///
    /// The same five-sample neck, judged against a throat chosen so that the
    /// corrected threshold lands exactly on the clearance the neck reads:
    /// `1.25 + 0.5 × 0.5 = 1.5`, and the neck's centre EDT is 1.5. Every
    /// value here is exact in binary floating point, so this is the boundary
    /// itself and not a neighbourhood of it. Mutating `>=` to `>`, or moving
    /// the correction by any amount in either direction, splits the dumbbell
    /// and fails.
    #[test]
    fn the_core_test_admits_clearance_exactly_equal_to_the_corrected_threshold() {
        let pitch = 0.5;
        let throat = 1.25;
        assert_eq!(core_clearance_threshold_mm(throat, pitch), 1.5);
        let mask = dumbbell(2);
        let (_, stats) = flow_labels(&mask, pitch, throat).expect("flow ok");
        assert_eq!(stats.flow_components, 1, "{stats:?}");
    }

    /// **Test 8 — a coreless pocket behind a sub-throat neck is not annexed,
    /// and ordinary dead space still is.**
    ///
    /// The case the ruling exists for: a cored chamber, a neck too narrow for
    /// the bore, and beyond it a pocket that is *wider than the neck* but
    /// nowhere wide enough for the bore itself. Resin in that pocket can only
    /// leave through a passage the doctrine says resin cannot flow through,
    /// and no 3.0 mm drain fits inside it, so it must not be labelled,
    /// selected, or cut — it stays solid.
    ///
    /// The second half of the test is the one that keeps the rule honest:
    /// **every voxel of the chamber is still claimed**, corners included,
    /// even though a chamber corner's own clearance is half a voxel. Sub-bore
    /// dead space that drains *outward* into bore-admitting space is not a
    /// separate interior and must not be eroded.
    #[test]
    fn a_coreless_pocket_behind_a_sub_throat_neck_is_left_unlabelled() {
        let pitch = 0.5;
        let throat = 1.5;
        let (nz, ny) = (15_usize, 15_usize);
        let chamber_nx = 15_usize;
        let neck_nx = 3_usize;
        let pocket_nx = 5_usize;
        let nx = chamber_nx + neck_nx + pocket_nx;
        let mut mask = Array3::from_elem((nz, ny, nx), false);
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..chamber_nx {
                    mask[[z, y, x]] = true;
                }
            }
        }
        // A 3-sample neck: 1.0 mm of clearance on its axis, far under the
        // registered throat however it is measured.
        for z in 6..9 {
            for y in 6..9 {
                for x in chamber_nx..(chamber_nx + neck_nx) {
                    mask[[z, y, x]] = true;
                }
            }
        }
        // A 5-sample pocket: 1.5 mm on its axis -- wider than the neck, still
        // short of the corrected 1.75 mm core threshold, so it has no core of
        // its own and can never receive a drain.
        for z in 5..10 {
            for y in 5..10 {
                for x in (chamber_nx + neck_nx)..nx {
                    mask[[z, y, x]] = true;
                }
            }
        }

        let (labels, stats) = flow_labels(&mask, pitch, throat).expect("flow ok");
        assert_eq!(stats.flow_components, 1, "{stats:?}");
        let pocket_voxels = 5 * 5 * pocket_nx;
        for z in 0..nz {
            for y in 0..ny {
                for x in (chamber_nx + neck_nx)..nx {
                    if mask[[z, y, x]] {
                        assert_eq!(
                            labels[[z, y, x]],
                            0,
                            "pocket voxel [{z},{y},{x}] was annexed through a sub-throat neck: {stats:?}"
                        );
                    }
                }
            }
        }
        assert!(
            stats.unreached_voxels >= pocket_voxels,
            "the whole pocket must be unreached, not merely its interior: {stats:?}"
        );
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..chamber_nx {
                    assert_eq!(
                        labels[[z, y, x]],
                        1,
                        "chamber voxel [{z},{y},{x}] was eroded; sub-bore dead space that \
                         drains outward is not a separate interior"
                    );
                }
            }
        }
    }

    /// **Test 7 — the threshold comes from the registry.** A future edit
    /// that restates the constant instead of reading it fails this.
    #[test]
    fn the_registered_throat_matches_the_publication() {
        let expected = plinth_core::m0_constants()
            .expect("constants publish")
            .placeholders()
            .hollow_drain_r_mm()
            .diagnostic()
            .get();
        assert_eq!(registered_throat_radius_mm().expect("registered"), expected);
    }
}
