//! H1a drain/vent planning from the immutable H0 cavity field.
//!
//! This module intentionally plans only. It neither Boolean-drills the shell
//! nor declares the result drainable or printable; those claims belong to H1b
//! and the later cavity-support verifier.

use geom_kernel::{NativeSurface, NativeSurfaceConfig, V3};
use plinth_core::{m0_constants, Finite};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CavityEvidence, CavityField, H0ResourceDiagnostics, ShellCandidate};

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
    /// First deduplicated crossing: cavity void into its enclosing inner wall.
    pub inner_crossing_mm: Finite,
    /// Cavity-origin to outside exit distance, before the H1b drill extension.
    pub length_mm: Finite,
    /// Frozen voxel-stepping exit distance recorded as `wall_mm` by Python.
    pub sampled_exit_mm: Finite,
    pub keepout_start_mm: [Finite; 3],
    pub keepout_end_mm: [Finite; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrainPlan {
    holes: Vec<HoleGeometry>,
    unavailable_candidates: Vec<CandidateDiagnostic>,
}

impl DrainPlan {
    #[must_use]
    pub fn holes(&self) -> &[HoleGeometry] {
        &self.holes
    }

    #[must_use]
    pub fn unavailable_candidates(&self) -> &[CandidateDiagnostic] {
        &self.unavailable_candidates
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateDiagnostic {
    pub role: HoleRole,
    pub lobe_id: Option<u32>,
    pub site: usize,
    pub direction: [Finite; 3],
    pub crossings: usize,
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

    #[cfg(test)]
    pub(crate) fn with_test_refilled_field(mut self, excluded: &[bool]) -> Self {
        self.cavity_field
            .refill_excluded(excluded)
            .expect("test exclusion matches field");
        self
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

    #[must_use]
    pub fn into_reason(self) -> DrainPlanningRefusal {
        self.reason
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
    #[error("H1a allocation preflight underestimated a bounded planner buffer")]
    PreflightUnderestimated,
    #[error("native surface construction or batch ray query failed")]
    Kernel,
    #[error("cavity lobe {lobe_id} has no legal outward exit within 45mm")]
    NoExit {
        lobe_id: u32,
        diagnostics: Vec<CandidateDiagnostic>,
    },
    #[error("cavity lobe {lobe_id} exit exceeded the 45mm ceiling")]
    ExitTooLong { lobe_id: u32 },
    #[error("{role:?} candidate has {crossings} deduplicated crossings; exactly two are required")]
    CrossingCount {
        role: HoleRole,
        lobe_id: Option<u32>,
        crossings: usize,
    },
    #[error("{role:?} ray did not cross cavity wall then outer shell")]
    ShellBreach {
        role: HoleRole,
        lobe_id: Option<u32>,
    },
    #[error("non-finite geometry occurred while planning {role:?}")]
    NonFinite { role: HoleRole },
    #[error("no vent site escaped through a horizontal wall within 45mm")]
    NoVentSite {
        diagnostics: Vec<CandidateDiagnostic>,
    },
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct RayPath {
    inner_mm: f64,
    outer_mm: f64,
    sampled_mm: f64,
}

#[derive(Clone, Copy)]
struct PlannerPreflight {
    lobe_capacity: usize,
    high_site_capacity: usize,
    candidate_capacity: usize,
    requested_bytes: usize,
    normalized_direction_bytes: usize,
}

#[derive(Clone, Copy)]
struct PlannerPolicy {
    drain_direction_count: usize,
    vent_direction_count: usize,
    lobe_band_mm: f64,
    lobe_min_ml: f64,
    vent_top_band_mm: f64,
    vent_sample_divisor: usize,
    exit_ceiling_mm: f64,
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
    let mut extension_ladder = policy
        .hollow_extra_drain_policy()
        .diagnostic()
        .extensions_mm();
    let keepout_before_mm = extension_ladder
        .next()
        .ok_or(DrainPlanningRefusal::Constants)?;
    let drain_keepout_after_mm = extension_ladder
        .next()
        .ok_or(DrainPlanningRefusal::Constants)?
        - keepout_before_mm;
    let vent_keepout_after_mm = extension_ladder
        .next()
        .ok_or(DrainPlanningRefusal::Constants)?
        - keepout_before_mm;
    let field = candidate.cavity_field();
    let resources = candidate.cavity().resources();
    let drain_direction_count = policy
        .hollow_drain_directions()
        .diagnostic()
        .directions()
        .len();
    let vent_direction_count = policy
        .hollow_vent_directions()
        .diagnostic()
        .directions()
        .len();
    let vent_top_band_mm = policy.hollow_vent_top_band_mm().diagnostic().get();
    let vent_sample_divisor =
        usize::try_from(policy.hollow_vent_sample_divisor().diagnostic().get())
            .map_err(|_| DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let native = NativeSurface::estimate_index_memory_from_indexed(
        candidate.mesh().verts(),
        candidate.mesh().tris(),
        field.pitch_mm().get(),
    )
    .map_err(|_| DrainPlanningRefusal::Kernel)?;
    let preflight = planner_preflight(
        candidate,
        field,
        resources,
        native.peak_bytes(),
        PlannerPolicy {
            drain_direction_count,
            vent_direction_count,
            lobe_band_mm: policy.hollow_lobe_band_mm().diagnostic().get(),
            lobe_min_ml: policy.hollow_lobe_min_ml().diagnostic().get(),
            vent_top_band_mm,
            vent_sample_divisor,
            exit_ceiling_mm: policy.hollow_exit_ray_max_mm().diagnostic().get(),
        },
    )?;
    debug_assert!(preflight.requested_bytes <= candidate.cavity().drain_planning_cap_bytes());
    debug_assert!(preflight.normalized_direction_bytes > 0);
    let lobes = low_lobes(
        field,
        policy.hollow_lobe_band_mm().diagnostic().get(),
        policy.hollow_lobe_min_ml().diagnostic().get(),
        candidate.cavity().drain_planning_cap_bytes(),
    )?;
    let vent_sites = high_sites(
        field,
        preflight.high_site_capacity,
        vent_top_band_mm,
        vent_sample_divisor,
    )?;

    let mut candidates = Vec::new();
    let drain_directions: Vec<V3> = policy
        .hollow_drain_directions()
        .diagnostic()
        .directions()
        .iter()
        .map(|direction| normalize([direction.x(), direction.y(), direction.z()]))
        .collect::<Result<_, _>>()?;
    let horizontal: Vec<V3> = policy
        .hollow_vent_directions()
        .diagnostic()
        .directions()
        .iter()
        .map(|direction| normalize([direction.x(), direction.y(), direction.z()]))
        .collect::<Result<_, _>>()?;
    let candidate_count = lobes
        .len()
        .checked_mul(drain_directions.len())
        .and_then(|value| value.checked_add(vent_sites.len().checked_mul(horizontal.len())?))
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    if lobes.len() > preflight.lobe_capacity || candidate_count > preflight.candidate_capacity {
        return Err(DrainPlanningRefusal::PreflightUnderestimated);
    }
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
    let ceiling = policy.hollow_exit_ray_max_mm().diagnostic().get();
    let sampled_exits = sampled_exit_distances(field, &candidates, ceiling)?;

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
    // One gathered batch covers every drain and vent candidate. Results are
    // scattered solely by the stable candidate ID below.
    let (offsets, ts, _faces) = surface
        .cast_rays(&origins, &directions, ceiling)
        .map_err(|_| DrainPlanningRefusal::Kernel)?;

    let mut paths = vec![None; candidates.len()];
    let mut unavailable_candidates = Vec::new();
    for candidate in &candidates {
        let begin =
            usize::try_from(offsets[candidate.id]).map_err(|_| DrainPlanningRefusal::Kernel)?;
        let end =
            usize::try_from(offsets[candidate.id + 1]).map_err(|_| DrainPlanningRefusal::Kernel)?;
        paths[candidate.id] = retain_candidate_path(
            candidate,
            exit_length(
                &ts[begin..end],
                ceiling,
                sampled_exits[candidate.id],
                candidate.role,
                candidate.lobe_id,
            ),
            &mut unavailable_candidates,
        )?;
    }

    let mut holes = Vec::with_capacity(lobes.len() + 1);
    for (site, lobe) in lobes.iter().enumerate() {
        let best = best_site(&candidates, &paths, HoleRole::Drain, site).ok_or_else(|| {
            DrainPlanningRefusal::NoExit {
                lobe_id: lobe.id,
                diagnostics: std::mem::take(&mut unavailable_candidates),
            }
        })?;
        holes.push(hole(
            best.0,
            best.1,
            policy.hollow_drain_r_mm().diagnostic().get(),
            keepout_before_mm,
            drain_keepout_after_mm,
        )?);
    }
    let vent = best_role(&candidates, &paths, HoleRole::Vent).ok_or_else(|| {
        DrainPlanningRefusal::NoVentSite {
            diagnostics: std::mem::take(&mut unavailable_candidates),
        }
    })?;
    holes.push(hole(
        vent.0,
        vent.1,
        policy.hollow_vent_r_mm().diagnostic().get(),
        keepout_before_mm,
        vent_keepout_after_mm,
    )?);
    Ok(DrainPlan {
        holes,
        unavailable_candidates,
    })
}

fn retain_candidate_path(
    candidate: &Candidate,
    result: Result<Option<RayPath>, DrainPlanningRefusal>,
    diagnostics: &mut Vec<CandidateDiagnostic>,
) -> Result<Option<RayPath>, DrainPlanningRefusal> {
    match result {
        // Every non-pair crossing count makes only this deterministic
        // direction unavailable. Alternate directions/sites remain eligible,
        // exactly as the frozen stepping loop continued.
        Err(DrainPlanningRefusal::CrossingCount { crossings, .. }) => {
            diagnostics.push(CandidateDiagnostic {
                role: candidate.role,
                lobe_id: candidate.lobe_id,
                site: candidate.site,
                direction: candidate.direction.map(|value| {
                    Finite::new(value).expect("normalized registered direction is finite")
                }),
                crossings,
            });
            Ok(None)
        }
        result => result,
    }
}

fn exit_length(
    hits: &[f64],
    ceiling: f64,
    sampled_mm: Option<f64>,
    role: HoleRole,
    lobe_id: Option<u32>,
) -> Result<Option<RayPath>, DrainPlanningRefusal> {
    if hits.iter().any(|value| !value.is_finite()) {
        return Err(DrainPlanningRefusal::NonFinite { role });
    }
    if hits.len() != 2 {
        return Err(DrainPlanningRefusal::CrossingCount {
            role,
            lobe_id,
            crossings: hits.len(),
        });
    }
    let inner = hits[0];
    let outer = hits[1];
    if inner <= 0.0 || outer <= inner {
        return Err(DrainPlanningRefusal::ShellBreach { role, lobe_id });
    }
    if outer > ceiling {
        return Err(lobe_id.map_or(
            DrainPlanningRefusal::NoVentSite {
                diagnostics: Vec::new(),
            },
            |lobe_id| DrainPlanningRefusal::ExitTooLong { lobe_id },
        ));
    }
    let Some(sampled_mm) = sampled_mm else {
        return Ok(None);
    };
    Ok(Some(RayPath {
        inner_mm: inner,
        outer_mm: outer,
        sampled_mm,
    }))
}

fn sampled_exit_distances(
    field: &CavityField,
    candidates: &[Candidate],
    ceiling_mm: f64,
) -> Result<Vec<Option<f64>>, DrainPlanningRefusal> {
    let pitch = field.pitch_mm().get();
    let sample_count = (ceiling_mm / pitch).floor() as usize + 1;
    let gathered_count = candidates
        .len()
        .checked_mul(sample_count)
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let mut gathered = Vec::with_capacity(gathered_count);
    let origin = field.origin_mm().map(Finite::get);
    let dims = field.dimensions();
    for candidate in candidates {
        for sample in 0..sample_count {
            let distance = sample as f64 * pitch;
            let point = add(candidate.origin, scale(candidate.direction, distance));
            let mut index = [0_usize; 3];
            let mut in_bounds = true;
            for axis in 0..3 {
                let coordinate = ((point[axis] - origin[axis]) / pitch).round_ties_even();
                if coordinate < 0.0 || coordinate >= dims[axis] as f64 {
                    in_bounds = false;
                    break;
                }
                index[axis] = coordinate as usize;
            }
            gathered.push(
                in_bounds.then(|| index[0] + index[1] * dims[0] + index[2] * dims[0] * dims[1]),
            );
        }
    }
    let mut exits = vec![None; candidates.len()];
    for (candidate_id, samples) in gathered.chunks_exact(sample_count).enumerate() {
        exits[candidate_id] = samples.iter().enumerate().find_map(|(sample, flat)| {
            let occupied = flat.is_some_and(|flat| field.source_contains_flat(flat));
            (!occupied).then_some(sample as f64 * pitch)
        });
    }
    Ok(exits)
}

fn best_site<'a>(
    candidates: &'a [Candidate],
    paths: &[Option<RayPath>],
    role: HoleRole,
    site: usize,
) -> Option<(&'a Candidate, RayPath)> {
    let mut best: Option<(&Candidate, RayPath)> = None;
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.role == role && candidate.site == site)
    {
        let Some(path) = paths[candidate.id] else {
            continue;
        };
        if best.is_none_or(|(_, selected)| path.sampled_mm < selected.sampled_mm) {
            // Strictly smaller only: equal values keep the earlier canonical
            // direction/site ordering just like Python's `if r < best`.
            best = Some((candidate, path));
        }
    }
    best
}

fn best_role<'a>(
    candidates: &'a [Candidate],
    paths: &[Option<RayPath>],
    role: HoleRole,
) -> Option<(&'a Candidate, RayPath)> {
    let mut best: Option<(&Candidate, RayPath)> = None;
    for candidate in candidates.iter().filter(|candidate| candidate.role == role) {
        let Some(path) = paths[candidate.id] else {
            continue;
        };
        if best.is_none_or(|(_, selected)| path.sampled_mm < selected.sampled_mm) {
            best = Some((candidate, path));
        }
    }
    best
}

fn hole(
    candidate: &Candidate,
    path: RayPath,
    radius: f64,
    keepout_before_mm: f64,
    keepout_after_mm: f64,
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
        outer_exit_mm: array(point(path.outer_mm))?,
        radius_mm: finite(radius)?,
        inner_crossing_mm: finite(path.inner_mm)?,
        length_mm: finite(path.outer_mm)?,
        sampled_exit_mm: finite(path.sampled_mm)?,
        keepout_start_mm: array(point(-keepout_before_mm))?,
        keepout_end_mm: array(point(path.sampled_mm + keepout_after_mm))?,
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
        .ok_or(DrainPlanningRefusal::NoVentSite {
            diagnostics: Vec::new(),
        })?;
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

fn high_sites(
    field: &CavityField,
    capacity: usize,
    top_band_mm: f64,
    sample_divisor: usize,
) -> Result<Vec<V3>, DrainPlanningRefusal> {
    let high_z = field
        .member_flats()
        .map(|flat| field.world_at_flat(flat)[2])
        .max_by(|left, right| left.partial_cmp(right).unwrap())
        .ok_or(DrainPlanningRefusal::NoVentSite {
            diagnostics: Vec::new(),
        })?;
    let high_count = field
        .member_flats()
        .filter(|flat| field.world_at_flat(*flat)[2] > high_z - top_band_mm)
        .count();
    let step = (high_count / sample_divisor).max(1);
    let mut sites = Vec::with_capacity(capacity);
    for (index, flat) in field
        .member_flats()
        .filter(|flat| field.world_at_flat(*flat)[2] > high_z - top_band_mm)
        .enumerate()
    {
        if index % step == 0 {
            sites.push(field.world_at_flat(flat));
        }
    }
    if sites.len() > capacity {
        return Err(DrainPlanningRefusal::PreflightUnderestimated);
    }
    Ok(sites)
}

fn planner_preflight(
    candidate: &ShellCandidate,
    field: &CavityField,
    resources: H0ResourceDiagnostics,
    native_peak_bytes: usize,
    policy: PlannerPolicy,
) -> Result<PlannerPreflight, DrainPlanningRefusal> {
    let cap = candidate.cavity().drain_planning_cap_bytes();
    let low_z = field
        .member_flats()
        .map(|flat| field.world_at_flat(flat)[2])
        .min_by(|left, right| left.partial_cmp(right).unwrap())
        .ok_or(DrainPlanningRefusal::NoVentSite {
            diagnostics: Vec::new(),
        })?;
    let low_members = field
        .member_flats()
        .filter(|flat| field.world_at_flat(*flat)[2] < low_z + policy.lobe_band_mm)
        .count();
    let minimum_lobe_voxels =
        (policy.lobe_min_ml * 1000.0 / field.pitch_mm().get().powi(3)).ceil() as usize;
    if minimum_lobe_voxels == 0 {
        return Err(DrainPlanningRefusal::ResourceAccountingOverflow);
    }
    let lobe_capacity = low_members / minimum_lobe_voxels;
    let high_z = field
        .member_flats()
        .map(|flat| field.world_at_flat(flat)[2])
        .max_by(|left, right| left.partial_cmp(right).unwrap())
        .ok_or(DrainPlanningRefusal::NoVentSite {
            diagnostics: Vec::new(),
        })?;
    let high_members = field
        .member_flats()
        .filter(|flat| field.world_at_flat(*flat)[2] > high_z - policy.vent_top_band_mm)
        .count();
    let high_step = (high_members / policy.vent_sample_divisor).max(1);
    let high_site_capacity = high_members
        .checked_add(high_step - 1)
        .map(|value| value / high_step)
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let candidate_capacity = lobe_capacity
        .checked_mul(policy.drain_direction_count)
        .and_then(|value| {
            value.checked_add(high_site_capacity.checked_mul(policy.vent_direction_count)?)
        })
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    if candidate_capacity > resources.query_batch_points() {
        return Err(DrainPlanningRefusal::ResourceLimitExceeded {
            requested_bytes: candidate_capacity
                .checked_mul(QUERY_CANDIDATE_BYTES)
                .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?,
            cap_bytes: cap,
        });
    }
    let allocator = |count: usize, size: usize, align: usize| {
        crate::allocator_allocation_bound(count, size, align)
            .map_err(|_| DrainPlanningRefusal::ResourceAccountingOverflow)
    };
    let retained_shell = crate::allocator_allocation_bound_bytes(
        candidate
            .mesh()
            .retained_buffer_bytes()
            .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?,
        std::mem::align_of::<V3>(),
    )
    .map_err(|_| DrainPlanningRefusal::ResourceAccountingOverflow)?;
    // The aggregate check below remains the single source of the planner's
    // cap diagnostic; this helper separately bills both retained allocations.
    let retained_field = retained_field_allocation_peak(field, usize::MAX)?;
    let field_cells = field.flat_count();
    let lobe_workspace = [
        allocator(
            field_cells,
            std::mem::size_of::<i32>(),
            std::mem::align_of::<i32>(),
        )?,
        allocator(
            field_cells,
            std::mem::size_of::<usize>(),
            std::mem::align_of::<usize>(),
        )?,
        allocator(
            field_cells,
            std::mem::size_of::<usize>(),
            std::mem::align_of::<usize>(),
        )?,
        allocator(
            field_cells,
            std::mem::size_of::<V3>(),
            std::mem::align_of::<V3>(),
        )?,
        allocator(
            lobe_capacity,
            std::mem::size_of::<Lobe>(),
            std::mem::align_of::<Lobe>(),
        )?,
    ]
    .into_iter()
    .try_fold(0_usize, |total, value| total.checked_add(value))
    .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let normalized_direction_bytes = allocator(
        policy.drain_direction_count,
        std::mem::size_of::<V3>(),
        std::mem::align_of::<V3>(),
    )?
    .checked_add(allocator(
        policy.vent_direction_count,
        std::mem::size_of::<V3>(),
        std::mem::align_of::<V3>(),
    )?)
    .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let candidate_buffers = [
        allocator(
            candidate_capacity,
            std::mem::size_of::<Candidate>(),
            std::mem::align_of::<Candidate>(),
        )?,
        allocator(
            candidate_capacity,
            std::mem::size_of::<V3>(),
            std::mem::align_of::<V3>(),
        )?,
        allocator(
            candidate_capacity,
            std::mem::size_of::<V3>(),
            std::mem::align_of::<V3>(),
        )?,
        allocator(
            candidate_capacity,
            std::mem::size_of::<Option<RayPath>>(),
            std::mem::align_of::<Option<RayPath>>(),
        )?,
        allocator(
            candidate_capacity,
            std::mem::size_of::<CandidateDiagnostic>(),
            std::mem::align_of::<CandidateDiagnostic>(),
        )?,
        normalized_direction_bytes,
        allocator(
            high_site_capacity,
            std::mem::size_of::<V3>(),
            std::mem::align_of::<V3>(),
        )?,
        allocator(
            lobe_capacity
                .checked_add(1)
                .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?,
            std::mem::size_of::<HoleGeometry>(),
            std::mem::align_of::<HoleGeometry>(),
        )?,
        allocator(
            candidate_capacity
                .checked_add(1)
                .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?,
            std::mem::size_of::<i64>(),
            std::mem::align_of::<i64>(),
        )?,
    ]
    .into_iter()
    .try_fold(0_usize, |total, value| total.checked_add(value))
    .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let sample_count = (policy.exit_ceiling_mm / field.pitch_mm().get()).floor() as usize + 1;
    let gathered_samples = candidate_capacity
        .checked_mul(sample_count)
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let sampled_wall_buffers = allocator(
        gathered_samples,
        std::mem::size_of::<Option<usize>>(),
        std::mem::align_of::<Option<usize>>(),
    )?
    .checked_add(allocator(
        candidate_capacity,
        std::mem::size_of::<Option<f64>>(),
        std::mem::align_of::<Option<f64>>(),
    )?)
    .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    // `cast_rays` retains one `(Vec<f64>, Vec<i64>)` header pair per ray while
    // serially building both flattened arrays. During the parallel gather,
    // each ray can also retain its `(t, face)` collection. Every deduplicated
    // crossing can originate on a distinct face, so all ragged, transient and
    // flattened payloads are charged with allocator/capacity slack.
    let hit_values = candidate_capacity
        .checked_mul(candidate.mesh().tris().len())
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let ray_buffers = [
        allocator(
            candidate_capacity,
            std::mem::size_of::<(Vec<f64>, Vec<i64>)>(),
            std::mem::align_of::<(Vec<f64>, Vec<i64>)>(),
        )?,
        allocator(
            hit_values,
            std::mem::size_of::<(f64, usize)>(),
            std::mem::align_of::<(f64, usize)>(),
        )?,
        allocator(
            hit_values,
            std::mem::size_of::<f64>(),
            std::mem::align_of::<f64>(),
        )?,
        allocator(
            hit_values,
            std::mem::size_of::<i64>(),
            std::mem::align_of::<i64>(),
        )?,
        allocator(
            hit_values,
            std::mem::size_of::<f64>(),
            std::mem::align_of::<f64>(),
        )?,
        allocator(
            hit_values,
            std::mem::size_of::<i64>(),
            std::mem::align_of::<i64>(),
        )?,
    ]
    .into_iter()
    .try_fold(0_usize, |total, value| total.checked_add(value))
    .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let requested = [
        retained_shell,
        retained_field,
        lobe_workspace,
        candidate_buffers,
        sampled_wall_buffers,
        native_peak_bytes,
        ray_buffers,
    ]
    .into_iter()
    .try_fold(0_usize, |total, value| total.checked_add(value))
    .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    if requested > cap {
        return Err(DrainPlanningRefusal::ResourceLimitExceeded {
            requested_bytes: requested,
            cap_bytes: cap,
        });
    }
    Ok(PlannerPreflight {
        lobe_capacity,
        high_site_capacity,
        candidate_capacity,
        requested_bytes: requested,
        normalized_direction_bytes,
    })
}

fn retained_field_allocation_peak(
    field: &CavityField,
    cap: usize,
) -> Result<usize, DrainPlanningRefusal> {
    let allocations = field.retained_allocations();
    let membership = crate::allocator_allocation_bound_bytes(
        allocations.membership_bytes(),
        std::mem::align_of::<u64>(),
    )
    .map_err(|_| DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let source_occupancy = crate::allocator_allocation_bound_bytes(
        allocations.source_occupancy_bytes(),
        std::mem::align_of::<u64>(),
    )
    .map_err(|_| DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let requested = membership
        .checked_add(source_occupancy)
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    if requested > cap {
        return Err(DrainPlanningRefusal::ResourceLimitExceeded {
            requested_bytes: requested,
            cap_bytes: cap,
        });
    }
    Ok(requested)
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

    fn cast_counts(mesh: &plinth_mesh::Mesh, rays: &[(V3, V3)]) -> Vec<usize> {
        let estimate =
            NativeSurface::estimate_index_memory_from_indexed(mesh.verts(), mesh.tris(), 0.5)
                .expect("native estimate");
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 0.5,
                max_grid_bytes: estimate.max_axis_grid_bytes(),
            },
        )
        .expect("native surface");
        let origins: Vec<_> = rays.iter().map(|(origin, _)| *origin).collect();
        let directions: Vec<_> = rays.iter().map(|(_, direction)| *direction).collect();
        let (offsets, _, _) = surface
            .cast_rays(&origins, &directions, 45.0)
            .expect("ray batch");
        offsets
            .windows(2)
            .map(|window| usize::try_from(window[1] - window[0]).expect("nonnegative offsets"))
            .collect()
    }

    fn frozen_candidate() -> ShellCandidate {
        let baseline =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/baseline");
        let json = baseline.join("Sphere.pipeline.json");
        assert!(
            json.is_file(),
            "required frozen baseline JSON is missing: {json:?}"
        );
        let path = baseline.join("Sphere.prepped.oriented-solid.stl");
        assert!(
            path.is_file(),
            "required frozen baseline STL is missing: {path:?}"
        );
        create_shell(load_binary_stl(path, WeldPolicy::None).expect("frozen input"))
            .expect("frozen eligible shell")
    }

    fn frozen_candidate_with_unequal_field_capacities() -> ShellCandidate {
        let candidate = frozen_candidate();
        let (mesh, cavity, field) = candidate.into_parts();
        let dimensions = field.dimensions();
        let origin = field.origin_mm();
        let pitch = field.pitch_mm();
        let member_count = field.member_count();
        let members = field.member_flats().collect::<Vec<_>>();
        let source_members = (0..field.flat_count())
            .filter(|flat| field.source_contains_flat(*flat))
            .collect::<Vec<_>>();
        let field = field.with_test_unequal_retained_capacities();
        assert_eq!(field.dimensions(), dimensions);
        assert_eq!(field.origin_mm(), origin);
        assert_eq!(field.pitch_mm(), pitch);
        assert_eq!(field.member_count(), member_count);
        assert_eq!(field.member_flats().collect::<Vec<_>>(), members);
        assert_eq!(
            (0..field.flat_count())
                .filter(|flat| field.source_contains_flat(*flat))
                .collect::<Vec<_>>(),
            source_members
        );
        let allocations = field.retained_allocations();
        assert_ne!(
            allocations.membership_bytes(),
            allocations.source_occupancy_bytes()
        );
        ShellCandidate {
            mesh,
            cavity,
            cavity_field: field,
        }
    }

    fn frozen_hollow_report() -> serde_json::Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline/Sphere.pipeline.json");
        let bytes = std::fs::read(&path).unwrap_or_else(|error| {
            panic!("required frozen baseline is missing: {path:?}: {error}")
        });
        let root: serde_json::Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("frozen baseline is invalid JSON: {path:?}: {error}"));
        root["stages"]
            .as_array()
            .and_then(|stages| stages.iter().find(|stage| stage["name"] == "hollow"))
            .and_then(|stage| stage.get("report"))
            .cloned()
            .expect("frozen baseline must contain the hollow report")
    }

    #[test]
    fn required_frozen_baseline_artifacts_are_present() {
        let report = frozen_hollow_report();
        assert!(report["drains"].is_array());
        assert!(report["vent"].is_object());
        drop(frozen_candidate());
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
            exit_length(&[0.25, 3.5], 45.0, Some(4.0), HoleRole::Drain, Some(1),)
                .expect("valid pair"),
            Some(RayPath {
                inner_mm: 0.25,
                outer_mm: 3.5,
                sampled_mm: 4.0,
            })
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
        let selected = best_site(
            &candidates,
            &[
                Some(RayPath {
                    inner_mm: 0.25,
                    outer_mm: 3.5,
                    sampled_mm: 4.0,
                }),
                Some(RayPath {
                    inner_mm: 0.25,
                    outer_mm: 3.5,
                    sampled_mm: 4.0,
                }),
            ],
            HoleRole::Drain,
            0,
        )
        .expect("both exact rays exit");
        assert_eq!(selected.0.id, 0, "the frozen direction order wins ties");

        for (hits, crossings) in [
            (&[][..], 0_usize),
            (&[0.25][..], 1),
            (&[0.25, 1.0, 2.0][..], 3),
        ] {
            assert!(matches!(
                exit_length(hits, 45.0, Some(4.0), HoleRole::Drain, Some(1)),
                Err(DrainPlanningRefusal::CrossingCount { crossings: actual, .. }) if actual == crossings
            ));
        }
        assert!(matches!(
            exit_length(&[-0.1, 3.5], 45.0, Some(4.0), HoleRole::Drain, Some(1),),
            Err(DrainPlanningRefusal::ShellBreach { .. })
        ));
        assert!(matches!(
            exit_length(&[0.25, 45.1], 45.0, Some(45.0), HoleRole::Drain, Some(1),),
            Err(DrainPlanningRefusal::ExitTooLong { lobe_id: 1 })
        ));
        assert!(
            exit_length(&[0.25, 45.0], 45.0, Some(45.0), HoleRole::Drain, Some(1),)
                .expect("inclusive guide ceiling")
                .is_some()
        );
    }

    #[test]
    fn native_canonical_rays_pin_zero_one_two_and_three_plus_crossing_refusals() {
        let box_mesh = make_box([10.0, 10.0, 10.0], [0.0, 0.0, 0.0]).expect("box");
        assert_eq!(
            cast_counts(
                &box_mesh,
                &[
                    ([20.0, 0.0, 5.0], [1.0, 0.0, 0.0]),
                    ([0.0, 0.0, 5.0], [1.0, 0.0, 0.0]),
                    ([-20.0, 0.0, 5.0], [1.0, 0.0, 0.0]),
                ],
            ),
            vec![0, 1, 2]
        );
        let shell = frozen_candidate();
        assert_eq!(
            cast_counts(shell.mesh(), &[([-20.0, 0.0, 10.0], [1.0, 0.0, 0.0])],),
            vec![4]
        );
    }

    #[test]
    fn one_and_three_crossings_are_reported_while_alternate_direction_continues() {
        let candidates = [
            Candidate {
                id: 0,
                role: HoleRole::Drain,
                lobe_id: Some(7),
                site: 0,
                origin: [0.0; 3],
                direction: [0.0, 0.0, -1.0],
            },
            Candidate {
                id: 1,
                role: HoleRole::Drain,
                lobe_id: Some(7),
                site: 0,
                origin: [0.0; 3],
                direction: [1.0, 0.0, -1.0],
            },
            Candidate {
                id: 2,
                role: HoleRole::Drain,
                lobe_id: Some(7),
                site: 0,
                origin: [0.0; 3],
                direction: [-1.0, 0.0, -1.0],
            },
        ];
        let mut diagnostics = Vec::new();
        let mut paths = vec![None; candidates.len()];
        for (candidate, hits) in
            candidates
                .iter()
                .zip([&[0.25][..], &[0.25, 2.0][..], &[0.25, 1.0, 2.0][..]])
        {
            paths[candidate.id] = retain_candidate_path(
                candidate,
                exit_length(hits, 45.0, Some(2.5), candidate.role, candidate.lobe_id),
                &mut diagnostics,
            )
            .expect("crossing-count mismatch is candidate-local");
        }
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].crossings, 1);
        assert_eq!(diagnostics[1].crossings, 3);
        assert_eq!(
            best_site(&candidates, &paths, HoleRole::Drain, 0)
                .expect("valid alternate direction remains")
                .0
                .id,
            1
        );
    }

    #[test]
    fn sampled_wall_batch_pins_diagonal_ties_even_and_inclusive_ceiling() {
        let mut selected = Array3::from_elem((5, 5, 5), false);
        selected[(2, 2, 2)] = true;
        let mut occupancy = Array3::from_elem((5, 5, 5), false);
        occupancy[(2, 2, 2)] = true;
        occupancy[(1, 2, 3)] = true;
        let diagonal_field = CavityField::from_selected_mask_and_occupancy(
            &selected,
            &occupancy,
            [0.0, 0.0, 0.0],
            0.5,
        )
        .expect("diagonal field");
        let diagonal = Candidate {
            id: 0,
            role: HoleRole::Drain,
            lobe_id: Some(1),
            site: 0,
            origin: [1.0, 1.0, 1.0],
            direction: normalize([1.0, 0.0, -1.0]).expect("diagonal"),
        };
        assert_eq!(
            sampled_exit_distances(&diagonal_field, &[diagonal], 2.0).expect("diagonal batch"),
            vec![Some(1.5)]
        );

        let mut boundary_selected = Array3::from_elem((1, 1, 91), false);
        boundary_selected[(0, 0, 0)] = true;
        let mut boundary_occupancy = Array3::from_elem((1, 1, 91), false);
        for x in 0..90 {
            boundary_occupancy[(0, 0, x)] = true;
        }
        let boundary_field = CavityField::from_selected_mask_and_occupancy(
            &boundary_selected,
            &boundary_occupancy,
            [0.0, 0.0, 0.0],
            0.5,
        )
        .expect("boundary field");
        let boundary = Candidate {
            id: 0,
            role: HoleRole::Vent,
            lobe_id: None,
            site: 0,
            origin: [0.0, 0.0, 0.0],
            direction: [1.0, 0.0, 0.0],
        };
        assert_eq!(
            sampled_exit_distances(&boundary_field, &[boundary], 45.0)
                .expect("inclusive boundary batch"),
            vec![Some(45.0)]
        );

        let mut tie_selected = Array3::from_elem((1, 1, 3), false);
        tie_selected[(0, 0, 0)] = true;
        let tie_field = CavityField::from_selected_mask_and_occupancy(
            &tie_selected,
            &tie_selected,
            [0.0, 0.0, 0.0],
            0.5,
        )
        .expect("tie field");
        let tie = Candidate {
            origin: [0.25, 0.0, 0.0],
            ..boundary
        };
        assert_eq!(
            sampled_exit_distances(&tie_field, &[tie], 0.5).expect("ties-even batch"),
            vec![Some(0.5)]
        );
    }

    #[test]
    fn frozen_shell_field_has_an_enclosed_drain_and_vent_plan() {
        let baseline = frozen_hollow_report();
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
        let xyz = |value: &serde_json::Value| {
            let values = value.as_array().expect("baseline xyz array");
            [
                Finite::new(values[0].as_f64().expect("x")).expect("finite x"),
                Finite::new(values[1].as_f64().expect("y")).expect("finite y"),
                Finite::new(values[2].as_f64().expect("z")).expect("finite z"),
            ]
        };
        assert_eq!(holes[0].center_mm, xyz(&baseline["drains"][0]["xyz"]));
        assert_eq!(holes[1].center_mm, xyz(&baseline["vent"]["xyz"]));
        assert_eq!(
            holes[0].sampled_exit_mm.get(),
            baseline["drains"][0]["wall_mm"]
                .as_f64()
                .expect("drain wall_mm")
        );
        assert_eq!(
            holes[1].sampled_exit_mm.get(),
            baseline["vent"]["wall_mm"].as_f64().expect("vent wall_mm")
        );
        let keepouts = baseline["cavity_descriptor"]["drain_keepouts"]
            .as_array()
            .expect("baseline keepouts");
        assert_eq!(holes[0].direction, xyz(&keepouts[0]["axis"]));
        assert_eq!(holes[0].keepout_start_mm, xyz(&keepouts[0]["p0"]));
        assert_eq!(holes[0].keepout_end_mm, xyz(&keepouts[0]["p1"]));
        assert_eq!(holes[0].radius_mm.get().to_bits(), 1.5_f64.to_bits());
        assert_eq!(holes[1].direction, xyz(&keepouts[1]["axis"]));
        assert_eq!(holes[1].keepout_start_mm, xyz(&keepouts[1]["p0"]));
        assert_eq!(holes[1].keepout_end_mm, xyz(&keepouts[1]["p1"]));
        assert_eq!(holes[1].radius_mm.get().to_bits(), 1.25_f64.to_bits());
        assert!(holes.iter().all(|hole| {
            hole.inner_crossing_mm.get() > 0.0
                && hole.length_mm.get() > hole.inner_crossing_mm.get()
        }));
    }

    #[test]
    fn no_exit_is_a_typed_refusal_and_candidate_is_returned() {
        let mesh = make_box([30.0, 30.0, 30.0], [0.0, 0.0, 0.0]).expect("box");
        let resource = H0ResourceDiagnostics {
            voxel_count: 1,
            query_batch_points: 1_000,
            grid_coordinate_bytes: 0,
            retained_raw_source_buffer_bytes: 0,
            raster_work_items: 0,
            retained_cavity_membership_bytes: 8,
            retained_source_occupancy_bytes: 8,
            marching_vertex_upper_bound: 0,
            marching_triangle_upper_bound: 0,
            boolean_input_primitive_units: 0,
            boolean_marshaled_input_bytes: 0,
            boolean_output_vertex_limit: 0,
            boolean_output_triangle_limit: 0,
        };
        let candidate = ShellCandidate {
            mesh,
            cavity: CavityEvidence {
                wall_thickness_mm: Finite::new(2.5).expect("finite"),
                frozen_voxel_pitch_mm: Finite::new(0.5).expect("finite"),
                cavity_volume_mm3: Finite::new(0.125).expect("finite"),
                source_volume_mm3: Finite::new(27_000.0).expect("finite"),
                resources: resource,
                drain_planning_cap_bytes: usize::MAX,
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
        assert!(matches!(
            failure.reason(),
            DrainPlanningRefusal::NoExit { lobe_id: 1, diagnostics }
                if !diagnostics.is_empty() && diagnostics.iter().all(|item| item.crossings == 0)
        ));
        assert_eq!(
            failure.into_candidate().cavity_field().member_count(),
            4_000
        );
    }

    #[test]
    fn native_cast_rays_plan_is_bit_identical_across_one_four_and_sixteen_threads() {
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
        cavity.drain_planning_cap_bytes = 1;
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

    #[test]
    fn planner_preflight_has_an_exact_cap_boundary_before_candidate_allocation() {
        let candidate = frozen_candidate();
        let (mesh, mut cavity, field) = candidate.into_parts();
        cavity.drain_planning_cap_bytes = usize::MAX;
        let unlimited = ShellCandidate {
            mesh,
            cavity,
            cavity_field: field,
        };
        let constants = m0_constants().expect("M0 constants");
        let policy = constants.placeholders();
        let native = NativeSurface::estimate_index_memory_from_indexed(
            unlimited.mesh().verts(),
            unlimited.mesh().tris(),
            unlimited.cavity_field().pitch_mm().get(),
        )
        .expect("native preflight");
        let preflight = planner_preflight(
            &unlimited,
            unlimited.cavity_field(),
            unlimited.cavity().resources(),
            native.peak_bytes(),
            PlannerPolicy {
                drain_direction_count: policy
                    .hollow_drain_directions()
                    .diagnostic()
                    .directions()
                    .len(),
                vent_direction_count: policy
                    .hollow_vent_directions()
                    .diagnostic()
                    .directions()
                    .len(),
                lobe_band_mm: policy.hollow_lobe_band_mm().diagnostic().get(),
                lobe_min_ml: policy.hollow_lobe_min_ml().diagnostic().get(),
                vent_top_band_mm: policy.hollow_vent_top_band_mm().diagnostic().get(),
                vent_sample_divisor: usize::try_from(
                    policy.hollow_vent_sample_divisor().diagnostic().get(),
                )
                .expect("sample divisor fits usize"),
                exit_ceiling_mm: policy.hollow_exit_ray_max_mm().diagnostic().get(),
            },
        )
        .expect("unlimited preflight");
        assert_eq!(preflight.normalized_direction_bytes, 480);
        let exact = preflight.requested_bytes;

        let exact_candidate = frozen_candidate();
        let (mesh, mut cavity, field) = exact_candidate.into_parts();
        cavity.drain_planning_cap_bytes = exact;
        plan_drains(ShellCandidate {
            mesh,
            cavity,
            cavity_field: field,
        })
        .expect("exact H1a cap");

        let below = frozen_candidate();
        let (mesh, mut cavity, field) = below.into_parts();
        cavity.drain_planning_cap_bytes = exact - 1;
        let refusal = plan_drains(ShellCandidate {
            mesh,
            cavity,
            cavity_field: field,
        })
        .expect_err("cap minus one must refuse before planner allocations");
        assert!(matches!(
            refusal.reason(),
            DrainPlanningRefusal::ResourceLimitExceeded { requested_bytes, cap_bytes }
                if *requested_bytes == exact && *cap_bytes == exact - 1
        ));
    }

    #[test]
    fn unequal_retained_field_capacities_have_a_real_planner_cap_boundary() {
        let unlimited = frozen_candidate_with_unequal_field_capacities();
        let constants = m0_constants().expect("M0 constants");
        let policy = constants.placeholders();
        let native = NativeSurface::estimate_index_memory_from_indexed(
            unlimited.mesh().verts(),
            unlimited.mesh().tris(),
            unlimited.cavity_field().pitch_mm().get(),
        )
        .expect("native preflight");
        let preflight = planner_preflight(
            &unlimited,
            unlimited.cavity_field(),
            unlimited.cavity().resources(),
            native.peak_bytes(),
            PlannerPolicy {
                drain_direction_count: policy
                    .hollow_drain_directions()
                    .diagnostic()
                    .directions()
                    .len(),
                vent_direction_count: policy
                    .hollow_vent_directions()
                    .diagnostic()
                    .directions()
                    .len(),
                lobe_band_mm: policy.hollow_lobe_band_mm().diagnostic().get(),
                lobe_min_ml: policy.hollow_lobe_min_ml().diagnostic().get(),
                vent_top_band_mm: policy.hollow_vent_top_band_mm().diagnostic().get(),
                vent_sample_divisor: usize::try_from(
                    policy.hollow_vent_sample_divisor().diagnostic().get(),
                )
                .expect("sample divisor fits usize"),
                exit_ceiling_mm: policy.hollow_exit_ray_max_mm().diagnostic().get(),
            },
        )
        .expect("unequal-capacity planner preflight");
        let exact = preflight.requested_bytes;

        let (mesh, mut cavity, field) = unlimited.into_parts();
        cavity.drain_planning_cap_bytes = exact;
        plan_drains(ShellCandidate {
            mesh,
            cavity,
            cavity_field: field,
        })
        .expect("exact unequal-capacity H1a cap");

        let below = frozen_candidate_with_unequal_field_capacities();
        let (mesh, mut cavity, field) = below.into_parts();
        cavity.drain_planning_cap_bytes = exact - 1;
        let refusal = plan_drains(ShellCandidate {
            mesh,
            cavity,
            cavity_field: field,
        })
        .expect_err("cap minus one must refuse in planner preflight");
        assert!(matches!(
            refusal.reason(),
            DrainPlanningRefusal::ResourceLimitExceeded { requested_bytes, cap_bytes }
                if *requested_bytes == exact && *cap_bytes == exact - 1
        ));
    }
}
