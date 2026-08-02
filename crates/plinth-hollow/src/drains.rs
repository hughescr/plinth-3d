//! H1a drain/vent planning from the immutable H0 cavity field.
//!
//! This module intentionally plans only. It neither Boolean-drills the shell
//! nor declares the result drainable or printable; those claims belong to H1b
//! and the later cavity-support verifier.

use geom_kernel::{NativeSurface, NativeSurfaceConfig, V3};
use plinth_core::{m0_constants, Finite};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CavityEvidence, CavityField, HollowResourceEstimate, ShellCandidate};

const VENT_TOP_BAND_MM: f64 = 2.0;
const VENT_SAMPLE_DIVISOR: usize = 40;
const KEEPOUT_BEFORE_MM: f64 = 4.0;
const KEEPOUT_AFTER_MM: f64 = 4.0;
const QUERY_CANDIDATE_BYTES: usize = 2 * std::mem::size_of::<V3>()
    + std::mem::size_of::<u64>()
    + std::mem::size_of::<f64>()
    + std::mem::size_of::<i64>();

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoleRole {
    Drain,
    Vent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoleGeometry {
    pub role: HoleRole,
    pub lobe_id: Option<u32>,
    pub center_mm: [Finite; 3],
    pub direction: [Finite; 3],
    pub outer_exit_mm: [Finite; 3],
    pub radius_mm: Finite,
    /// Cavity-origin to outside exit distance, before the H1b drill extension.
    pub length_mm: Finite,
    pub keepout_start_mm: [Finite; 3],
    pub keepout_end_mm: [Finite; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrainPlan {
    holes: Vec<HoleGeometry>,
}

impl DrainPlan {
    #[must_use]
    pub fn holes(&self) -> &[HoleGeometry] {
        &self.holes
    }
}

/// Sole move-forward authority after H1a. There is still exactly one mesh and
/// exactly one H0 cavity field; the plan is an in-memory typed attachment.
#[derive(Debug)]
pub struct PlannedShell {
    shell: plinth_mesh::Mesh,
    cavity: CavityEvidence,
    cavity_field: CavityField,
    drain_plan: DrainPlan,
}

impl PlannedShell {
    #[must_use]
    pub const fn shell(&self) -> &plinth_mesh::Mesh {
        &self.shell
    }
    #[must_use]
    pub const fn cavity(&self) -> &CavityEvidence {
        &self.cavity
    }
    #[must_use]
    pub const fn cavity_field(&self) -> &CavityField {
        &self.cavity_field
    }
    #[must_use]
    pub const fn drain_plan(&self) -> &DrainPlan {
        &self.drain_plan
    }
    #[must_use]
    pub fn into_parts(self) -> (plinth_mesh::Mesh, CavityEvidence, CavityField, DrainPlan) {
        (self.shell, self.cavity, self.cavity_field, self.drain_plan)
    }
}

#[derive(Debug)]
pub struct DrainPlanningFailure {
    candidate: Box<ShellCandidate>,
    reason: DrainPlanningRefusal,
}

impl DrainPlanningFailure {
    #[must_use]
    pub const fn reason(&self) -> &DrainPlanningRefusal {
        &self.reason
    }
    #[must_use]
    pub fn into_candidate(self) -> ShellCandidate {
        *self.candidate
    }
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum DrainPlanningRefusal {
    #[error("M0 constants publication failed")]
    Constants,
    #[error("H1a planning memory {requested_bytes} exceeds retained H0 cap {cap_bytes}")]
    ResourceLimitExceeded {
        requested_bytes: usize,
        cap_bytes: usize,
    },
    #[error("checked H1a planning resource accounting overflowed")]
    ResourceAccountingOverflow,
    #[error("native surface construction or batch ray query failed")]
    Kernel,
    #[error("cavity lobe {lobe_id} has no legal outward exit within 45mm")]
    NoExit { lobe_id: u32 },
    #[error("cavity lobe {lobe_id} exit exceeded the 45mm ceiling")]
    ExitTooLong { lobe_id: u32 },
    #[error("{role:?} ray did not cross cavity wall then outer shell")]
    ShellBreach {
        role: HoleRole,
        lobe_id: Option<u32>,
    },
    #[error("non-finite geometry occurred while planning {role:?}")]
    NonFinite { role: HoleRole },
    #[error("no vent site escaped through a horizontal wall within 45mm")]
    NoVentSite,
}

#[derive(Clone, Copy)]
struct Candidate {
    id: usize,
    role: HoleRole,
    lobe_id: Option<u32>,
    site: usize,
    origin: V3,
    direction: V3,
}

#[derive(Clone)]
struct Lobe {
    id: u32,
    voxels: usize,
    first_flat: usize,
    origin: V3,
}

/// Plans the exact frozen H1a candidates through one canonical batched native
/// ray query. On refusal it returns the untouched move-only candidate.
pub fn plan_drains(candidate: ShellCandidate) -> Result<PlannedShell, DrainPlanningFailure> {
    match plan(&candidate) {
        Ok(drain_plan) => {
            let (shell, cavity, cavity_field) = candidate.into_parts();
            Ok(PlannedShell {
                shell,
                cavity,
                cavity_field,
                drain_plan,
            })
        }
        Err(reason) => Err(DrainPlanningFailure {
            candidate: Box::new(candidate),
            reason,
        }),
    }
}

fn plan(candidate: &ShellCandidate) -> Result<DrainPlan, DrainPlanningRefusal> {
    let constants = m0_constants().map_err(|_| DrainPlanningRefusal::Constants)?;
    let policy = constants.placeholders();
    let field = candidate.cavity_field();
    let resources = candidate.cavity().resources();
    let lobes = low_lobes(
        field,
        policy.hollow_lobe_band_mm().diagnostic().get(),
        policy.hollow_lobe_min_ml().diagnostic().get(),
        candidate.cavity().resource_cap_bytes(),
    )?;
    let vent_sites = high_sites(field)?;

    let mut candidates = Vec::new();
    let drain_directions: Vec<V3> = policy
        .hollow_drain_directions()
        .diagnostic()
        .directions()
        .iter()
        .map(|direction| normalize([direction.x(), direction.y(), direction.z()]))
        .collect::<Result<_, _>>()?;
    let horizontal = [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
    ];
    let candidate_count = lobes
        .len()
        .checked_mul(drain_directions.len())
        .and_then(|value| value.checked_add(vent_sites.len().checked_mul(horizontal.len())?))
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let native = NativeSurface::estimate_index_memory_from_indexed(
        candidate.mesh().verts(),
        candidate.mesh().tris(),
        field.pitch_mm().get(),
    )
    .map_err(|_| DrainPlanningRefusal::Kernel)?;
    ensure_query_bound(
        candidate_count,
        candidate.mesh().tris().len(),
        resources,
        candidate.cavity().resource_cap_bytes(),
        native.peak_bytes(),
    )?;
    candidates.reserve_exact(candidate_count);
    for (site, lobe) in lobes.iter().enumerate() {
        for &direction in &drain_directions {
            let id = candidates.len();
            candidates.push(Candidate {
                id,
                role: HoleRole::Drain,
                lobe_id: Some(lobe.id),
                site,
                origin: lobe.origin,
                direction,
            });
        }
    }
    for (site, &origin) in vent_sites.iter().enumerate() {
        for &direction in &horizontal {
            let id = candidates.len();
            candidates.push(Candidate {
                id,
                role: HoleRole::Vent,
                lobe_id: None,
                site,
                origin,
                direction,
            });
        }
    }
    debug_assert_eq!(candidates.len(), candidate_count);

    let surface = NativeSurface::from_indexed(
        candidate.mesh().verts(),
        candidate.mesh().tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: field.pitch_mm().get(),
            max_grid_bytes: native.max_axis_grid_bytes(),
        },
    )
    .map_err(|_| DrainPlanningRefusal::Kernel)?;
    let origins: Vec<V3> = candidates
        .iter()
        .map(|candidate| candidate.origin)
        .collect();
    let directions: Vec<V3> = candidates
        .iter()
        .map(|candidate| candidate.direction)
        .collect();
    let ceiling = policy.hollow_exit_ray_max_mm().diagnostic().get();
    // One gathered batch covers every drain and vent candidate. Results are
    // scattered solely by the stable candidate ID below.
    let (offsets, ts, _faces) = surface
        .cast_rays(&origins, &directions, ceiling)
        .map_err(|_| DrainPlanningRefusal::Kernel)?;

    let mut lengths = vec![None; candidates.len()];
    for candidate in &candidates {
        let begin =
            usize::try_from(offsets[candidate.id]).map_err(|_| DrainPlanningRefusal::Kernel)?;
        let end =
            usize::try_from(offsets[candidate.id + 1]).map_err(|_| DrainPlanningRefusal::Kernel)?;
        lengths[candidate.id] =
            exit_length(&ts[begin..end], ceiling, candidate.role, candidate.lobe_id)?;
    }

    let mut holes = Vec::with_capacity(lobes.len() + 1);
    for (site, lobe) in lobes.iter().enumerate() {
        let best = best_site(&candidates, &lengths, HoleRole::Drain, site)
            .ok_or(DrainPlanningRefusal::NoExit { lobe_id: lobe.id })?;
        holes.push(hole(
            best.0,
            best.1,
            policy.hollow_drain_r_mm().diagnostic().get(),
        )?);
    }
    let vent =
        best_role(&candidates, &lengths, HoleRole::Vent).ok_or(DrainPlanningRefusal::NoVentSite)?;
    holes.push(hole(
        vent.0,
        vent.1,
        policy.hollow_vent_r_mm().diagnostic().get(),
    )?);
    Ok(DrainPlan { holes })
}

fn exit_length(
    hits: &[f64],
    ceiling: f64,
    role: HoleRole,
    lobe_id: Option<u32>,
) -> Result<Option<f64>, DrainPlanningRefusal> {
    if hits.iter().any(|value| !value.is_finite()) {
        return Err(DrainPlanningRefusal::NonFinite { role });
    }
    let Some(&inner) = hits.first() else {
        return Ok(None);
    };
    let Some(&outer) = hits.get(1) else {
        return Ok(None);
    };
    if inner < 0.0 || outer <= inner {
        return Err(DrainPlanningRefusal::ShellBreach { role, lobe_id });
    }
    if outer > ceiling {
        return Err(lobe_id.map_or(DrainPlanningRefusal::NoVentSite, |lobe_id| {
            DrainPlanningRefusal::ExitTooLong { lobe_id }
        }));
    }
    Ok(Some(outer))
}

fn best_site<'a>(
    candidates: &'a [Candidate],
    lengths: &[Option<f64>],
    role: HoleRole,
    site: usize,
) -> Option<(&'a Candidate, f64)> {
    let mut best = None;
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.role == role && candidate.site == site)
    {
        let Some(length) = lengths[candidate.id] else {
            continue;
        };
        if best.is_none_or(|(_, selected)| length < selected) {
            // Strictly smaller only: equal values keep the earlier canonical
            // direction/site ordering just like Python's `if r < best`.
            best = Some((candidate, length));
        }
    }
    best
}

fn best_role<'a>(
    candidates: &'a [Candidate],
    lengths: &[Option<f64>],
    role: HoleRole,
) -> Option<(&'a Candidate, f64)> {
    let mut best = None;
    for candidate in candidates.iter().filter(|candidate| candidate.role == role) {
        let Some(length) = lengths[candidate.id] else {
            continue;
        };
        if best.is_none_or(|(_, selected)| length < selected) {
            best = Some((candidate, length));
        }
    }
    best
}

fn hole(
    candidate: &Candidate,
    length: f64,
    radius: f64,
) -> Result<HoleGeometry, DrainPlanningRefusal> {
    let point = |distance: f64| add(candidate.origin, scale(candidate.direction, distance));
    let finite = |value: f64| {
        Finite::new(value).map_err(|_| DrainPlanningRefusal::NonFinite {
            role: candidate.role,
        })
    };
    let array = |value: V3| -> Result<[Finite; 3], DrainPlanningRefusal> {
        Ok([finite(value[0])?, finite(value[1])?, finite(value[2])?])
    };
    Ok(HoleGeometry {
        role: candidate.role,
        lobe_id: candidate.lobe_id,
        center_mm: array(candidate.origin)?,
        direction: array(candidate.direction)?,
        outer_exit_mm: array(point(length))?,
        radius_mm: finite(radius)?,
        length_mm: finite(length)?,
        keepout_start_mm: array(point(-KEEPOUT_BEFORE_MM))?,
        keepout_end_mm: array(point(length + KEEPOUT_AFTER_MM))?,
    })
}

fn low_lobes(
    field: &CavityField,
    band_mm: f64,
    min_ml: f64,
    cap: usize,
) -> Result<Vec<Lobe>, DrainPlanningRefusal> {
    let low_z = field
        .member_flats()
        .map(|flat| field.world_at_flat(flat)[2])
        .min_by(|left, right| left.partial_cmp(right).unwrap())
        .ok_or(DrainPlanningRefusal::NoVentSite)?;
    let label_bytes = field
        .flat_count()
        .checked_mul(std::mem::size_of::<i32>())
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let workspace = label_bytes
        .checked_add(
            field
                .flat_count()
                .checked_mul(2 * std::mem::size_of::<usize>())
                .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?,
        )
        .and_then(|value| {
            field
                .flat_count()
                .checked_mul(std::mem::size_of::<V3>())
                .and_then(|points| value.checked_add(points))
        })
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    if workspace > cap {
        return Err(DrainPlanningRefusal::ResourceLimitExceeded {
            requested_bytes: workspace,
            cap_bytes: cap,
        });
    }
    let mut labels = vec![-1_i32; field.flat_count()];
    let mut lobes = Vec::new();
    let mut next_component_id = 1_u32;
    for flat in field.member_flats() {
        if labels[flat] >= 0 || field.world_at_flat(flat)[2] >= low_z + band_mm {
            continue;
        }
        let id = next_component_id;
        next_component_id = next_component_id
            .checked_add(1)
            .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
        let mut stack = vec![flat];
        labels[flat] = id as i32;
        let mut members = Vec::new();
        while let Some(current) = stack.pop() {
            members.push(current);
            for neighbor in neighbors(field, current) {
                if labels[neighbor] < 0
                    && field.contains_flat(neighbor)
                    && field.world_at_flat(neighbor)[2] < low_z + band_mm
                {
                    labels[neighbor] = id as i32;
                    stack.push(neighbor);
                }
            }
        }
        let volume_ml = members.len() as f64 * field.pitch_mm().get().powi(3) / 1000.0;
        if volume_ml < min_ml {
            continue;
        }
        let lobe_low = members
            .iter()
            .map(|member| field.world_at_flat(*member)[2])
            .min_by(|left, right| left.partial_cmp(right).unwrap())
            .ok_or(DrainPlanningRefusal::NonFinite {
                role: HoleRole::Drain,
            })?;
        let low_points: Vec<V3> = members
            .iter()
            .map(|member| field.world_at_flat(*member))
            .filter(|point| point[2] < lobe_low + 1.0)
            .collect();
        let mut origin = [0.0; 3];
        for point in &low_points {
            origin[0] += point[0];
            origin[1] += point[1];
            origin[2] += point[2];
        }
        let count = low_points.len() as f64;
        origin[0] /= count;
        origin[1] /= count;
        origin[2] = lobe_low;
        lobes.push(Lobe {
            id,
            voxels: members.len(),
            first_flat: flat,
            origin,
        });
    }
    // Frozen Python ranks by descending voxel volume. First flat is the stable
    // implementation tie-break needed for a thread-independent H1 plan.
    lobes.sort_by(|left, right| {
        right
            .voxels
            .cmp(&left.voxels)
            .then(left.first_flat.cmp(&right.first_flat))
    });
    Ok(lobes)
}

fn high_sites(field: &CavityField) -> Result<Vec<V3>, DrainPlanningRefusal> {
    let high_z = field
        .member_flats()
        .map(|flat| field.world_at_flat(flat)[2])
        .max_by(|left, right| left.partial_cmp(right).unwrap())
        .ok_or(DrainPlanningRefusal::NoVentSite)?;
    let high_count = field
        .member_flats()
        .filter(|flat| field.world_at_flat(*flat)[2] > high_z - VENT_TOP_BAND_MM)
        .count();
    let step = (high_count / VENT_SAMPLE_DIVISOR).max(1);
    Ok(field
        .member_flats()
        .filter(|flat| field.world_at_flat(*flat)[2] > high_z - VENT_TOP_BAND_MM)
        .enumerate()
        .filter(|(index, _)| index % step == 0)
        .map(|(_, flat)| field.world_at_flat(flat))
        .collect())
}

fn ensure_query_bound(
    candidates: usize,
    faces: usize,
    resources: HollowResourceEstimate,
    cap: usize,
    native_peak_bytes: usize,
) -> Result<(), DrainPlanningRefusal> {
    if candidates > resources.query_batch_points() {
        return Err(DrainPlanningRefusal::ResourceLimitExceeded {
            requested_bytes: candidates
                .checked_mul(QUERY_CANDIDATE_BYTES)
                .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?,
            cap_bytes: cap,
        });
    }
    let worst_hits = candidates
        .checked_mul(faces)
        .and_then(|value| {
            value.checked_mul(std::mem::size_of::<f64>() + std::mem::size_of::<i64>())
        })
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let requested = candidates
        .checked_mul(QUERY_CANDIDATE_BYTES)
        .and_then(|value| value.checked_add(worst_hits))
        .and_then(|value| value.checked_add(native_peak_bytes))
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    if requested > cap {
        return Err(DrainPlanningRefusal::ResourceLimitExceeded {
            requested_bytes: requested,
            cap_bytes: cap,
        });
    }
    Ok(())
}

fn neighbors(field: &CavityField, flat: usize) -> impl Iterator<Item = usize> {
    let [x, y, z] = field.flat_to_index(flat);
    let [nx, ny, nz] = field.dimensions();
    let plane = nx * ny;
    let mut values = [None; 6];
    values[0] = if x > 0 { Some(flat - 1) } else { None };
    values[1] = if x + 1 < nx { Some(flat + 1) } else { None };
    values[2] = if y > 0 { Some(flat - nx) } else { None };
    values[3] = if y + 1 < ny { Some(flat + nx) } else { None };
    values[4] = if z > 0 { Some(flat - plane) } else { None };
    values[5] = if z + 1 < nz { Some(flat + plane) } else { None };
    values.into_iter().flatten()
}

fn normalize(value: V3) -> Result<V3, DrainPlanningRefusal> {
    let length = value[0].hypot(value[1]).hypot(value[2]);
    if !length.is_finite() || length == 0.0 {
        return Err(DrainPlanningRefusal::NonFinite {
            role: HoleRole::Drain,
        });
    }
    Ok([value[0] / length, value[1] / length, value[2] / length])
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(value: V3, amount: f64) -> V3 {
    [value[0] * amount, value[1] * amount, value[2] * amount]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_shell, CavityEvidence};
    use plinth_core::Finite;
    use plinth_mesh::{load_binary_stl, make_box, WeldPolicy};
    use scirs2_core::ndarray::Array3;
    use std::sync::{Mutex, OnceLock};

    fn field(mask: Array3<bool>) -> CavityField {
        CavityField::from_selected_mask(&mask, [0.0, 0.0, 0.0], 0.5).expect("finite field")
    }

    fn frozen_candidate() -> ShellCandidate {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline/Sphere.prepped.oriented-solid.stl");
        create_shell(load_binary_stl(path, WeldPolicy::None).expect("frozen input"))
            .expect("frozen eligible shell")
    }

    #[test]
    fn lobe_threshold_is_inclusive_and_keeps_x_fastest_lobe_identity() {
        // 4,000 cells * 0.5^3 / 1000 = exactly 0.5 mL: frozen Python keeps
        // this because it refuses only values strictly below the threshold.
        let full = field(Array3::from_elem((10, 20, 20), true));
        let lobes = low_lobes(&full, 15.0, 0.5, usize::MAX).expect("field fits");
        assert_eq!(lobes.len(), 1);
        assert_eq!(lobes[0].id, 1);
        assert_eq!(lobes[0].voxels, 4_000);

        let mut under = Array3::from_elem((10, 20, 20), true);
        under[(0, 0, 0)] = false;
        assert!(low_lobes(&field(under), 15.0, 0.5, usize::MAX)
            .expect("field fits")
            .is_empty());
    }

    #[test]
    fn ray_pair_uses_outer_exit_and_ordered_tie_keeps_first_direction() {
        assert_eq!(
            exit_length(&[0.25, 3.5], 45.0, HoleRole::Drain, Some(1)).expect("valid pair"),
            Some(3.5)
        );
        let candidates = [
            Candidate {
                id: 0,
                role: HoleRole::Drain,
                lobe_id: Some(1),
                site: 0,
                origin: [0.0; 3],
                direction: [0.0, 0.0, -1.0],
            },
            Candidate {
                id: 1,
                role: HoleRole::Drain,
                lobe_id: Some(1),
                site: 0,
                origin: [0.0; 3],
                direction: [0.5, 0.0, -1.0],
            },
        ];
        let selected = best_site(&candidates, &[Some(3.5), Some(3.5)], HoleRole::Drain, 0)
            .expect("both exact rays exit");
        assert_eq!(selected.0.id, 0, "the frozen direction order wins ties");
    }

    #[test]
    fn frozen_shell_field_has_an_enclosed_drain_and_vent_plan() {
        let planned = plan_drains(frozen_candidate()).expect("source field must be usable");
        let holes = planned.drain_plan().holes();
        assert_eq!(holes.len(), 2);
        assert_eq!(holes[0].role, HoleRole::Drain);
        assert_eq!(holes[0].lobe_id, Some(1));
        assert_eq!(
            holes[0].center_mm,
            [
                Finite::new(0.0).expect("finite"),
                Finite::new(0.0).expect("finite"),
                Finite::new(0.5).expect("finite"),
            ]
        );
        assert_eq!(holes[1].role, HoleRole::Vent);
        assert_eq!(
            holes[1].center_mm,
            [
                Finite::new(0.0).expect("finite"),
                Finite::new(0.0).expect("finite"),
                Finite::new(19.5).expect("finite"),
            ]
        );
        assert!(holes.iter().all(|hole| hole.length_mm.get() > 0.0));
    }

    #[test]
    fn no_exit_is_a_typed_refusal_and_candidate_is_returned() {
        let mesh = make_box([30.0, 30.0, 30.0], [0.0, 0.0, 0.0]).expect("box");
        let resource = HollowResourceEstimate {
            voxel_count: 1,
            query_batch_points: 1_000,
            grid_coordinate_bytes: 0,
            occupancy_bytes: 0,
            edt_peak_bytes: 0,
            label_peak_bytes: 0,
            surface_peak_bytes: 0,
            rasterization_peak_bytes: 0,
            retained_cavity_field_bytes: 8,
            output_upper_bound_bytes: 0,
            peak_bytes: 0,
        };
        let candidate = ShellCandidate {
            mesh,
            cavity: CavityEvidence {
                wall_thickness_mm: Finite::new(2.5).expect("finite"),
                frozen_voxel_pitch_mm: Finite::new(0.5).expect("finite"),
                cavity_volume_mm3: Finite::new(0.125).expect("finite"),
                source_volume_mm3: Finite::new(27_000.0).expect("finite"),
                resources: resource,
                resource_cap_bytes: usize::MAX,
            },
            cavity_field: field(Array3::from_elem((10, 20, 20), true)),
        };
        // Move the retained voxel well away from the shell: every canonical
        // ray misses, so the failure is the planner's typed no-exit result.
        let candidate = ShellCandidate {
            cavity_field: CavityField::from_selected_mask(
                &Array3::from_elem((10, 20, 20), true),
                [1_000.0, 1_000.0, 1_000.0],
                0.5,
            )
            .expect("field"),
            ..candidate
        };
        let failure =
            plan_drains(candidate).expect_err("no ray can leave from an invented remote cavity");
        assert_eq!(
            failure.reason(),
            &DrainPlanningRefusal::NoExit { lobe_id: 1 }
        );
        assert_eq!(failure.into_candidate().cavity_field().member_count(), 4_000);
    }

    #[test]
    fn planning_is_bit_identical_at_one_four_and_sixteen_threads() {
        static ENV: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("thread env lock");
        let previous = std::env::var_os("PLINTH_THREADS");
        let mut plans = Vec::new();
        for threads in ["1", "4", "16"] {
            std::env::set_var("PLINTH_THREADS", threads);
            plans.push(
                plan_drains(frozen_candidate())
                    .expect("frozen field must retain an enclosed plan")
                    .into_parts()
                    .3,
            );
        }
        match previous {
            Some(value) => std::env::set_var("PLINTH_THREADS", value),
            None => std::env::remove_var("PLINTH_THREADS"),
        };
        assert_eq!(plans[0], plans[1]);
        assert_eq!(plans[0], plans[2]);
    }

    #[test]
    fn retained_h0_resource_cap_bounds_planning_before_query_allocation() {
        let candidate = create_shell(
            load_binary_stl(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../corpus/baseline/Sphere.prepped.oriented-solid.stl"),
                WeldPolicy::None,
            )
            .expect("frozen input"),
        )
        .expect("shell");
        let (mesh, mut cavity, field) = candidate.into_parts();
        cavity.resource_cap_bytes = 1;
        let failure = plan_drains(ShellCandidate {
            mesh,
            cavity,
            cavity_field: field,
        })
        .expect_err("label allocation must be refused");
        assert!(matches!(
            failure.reason(),
            DrainPlanningRefusal::ResourceLimitExceeded { .. }
        ));
    }
}
