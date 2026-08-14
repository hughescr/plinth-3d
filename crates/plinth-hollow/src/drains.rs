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

    #[cfg(test)]
    pub(crate) fn append_test_diagnostics(&mut self, count: usize) {
        self.unavailable_candidates
            .extend((0..count).map(|site| CandidateDiagnostic {
                role: HoleRole::Vent,
                lobe_id: None,
                site,
                direction: [
                    Finite::new(1.0).expect("finite test direction"),
                    Finite::new(0.0).expect("finite test direction"),
                    Finite::new(0.0).expect("finite test direction"),
                ],
                crossings: 3,
            }));
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
    planner_accounting: PlannerAccounting,
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
    /// What H1a planning charged itself against the drain-planning cap.
    #[must_use]
    pub const fn planner_accounting(&self) -> &PlannerAccounting {
        &self.planner_accounting
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
    /// Which chamber this candidate serves, `0`-based. Drains and the vent of
    /// one chamber are accepted or dropped together — see `plan`.
    chamber: usize,
    site: usize,
    origin: V3,
    direction: V3,
}

#[derive(Clone)]
struct Lobe {
    id: u32,
    chamber: usize,
    voxels: usize,
    first_flat: usize,
    origin: V3,
}

/// The cavity's chambers: the 6-connected regions of the selected field, each
/// with its **own** lowest and highest point.
///
/// **Why this exists.** The planner used to measure one lowest point for the
/// whole cavity and drill in the band above it. That works for one chamber and
/// silently abandons every other: a chamber sitting higher up never enters the
/// band, so it never gets a drain, and its resin never leaves the print. Each
/// chamber is its own vessel — resin in it falls to *its* lowest point, not to
/// the model's — so each one is measured on its own here, once, in a single
/// flood over the field's members.
///
/// **A chamber is not the same thing as a resin-flow interior.** Two interiors
/// joined by a passage narrower than the drain bore cannot be drilled through
/// to each other, but resin can still run downhill between them, so they are
/// one vessel to drain and one chamber here. That is the same partition the
/// drilling stage's gravity map uses, which is what keeps the two stages from
/// disagreeing about what a drain serves.
struct Chambers {
    /// One entry per grid cell: `0` for a non-member, otherwise `1..=count`.
    labels: Vec<u32>,
    count: usize,
    /// Indexed by chamber, `0`-based.
    low_z_mm: Vec<f64>,
    high_z_mm: Vec<f64>,
    low_band_members: Vec<usize>,
    high_band_members: Vec<usize>,
}

impl Chambers {
    /// The chamber a member cell belongs to, `0`-based.
    fn chamber_of(&self, flat: usize) -> Option<usize> {
        match self.labels.get(flat).copied().unwrap_or(0) {
            0 => None,
            label => Some(label as usize - 1),
        }
    }
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

/// What H1a planning charged itself against the retained H0 cap, so a caller
/// can report the requirement beside the ceiling instead of only learning the
/// number when the ceiling refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlannerAccounting {
    requested_bytes: usize,
    ray_query_bytes: usize,
    ray_crossings: usize,
    candidate_count: usize,
}

impl PlannerAccounting {
    /// Everything H1a planning retains, including the measured ray-query
    /// charge. This is the figure compared against the drain-planning cap.
    #[must_use]
    pub const fn requested_bytes(&self) -> usize {
        self.requested_bytes
    }

    /// The single `cast_rays` batch's share of [`Self::requested_bytes`].
    #[must_use]
    pub const fn ray_query_bytes(&self) -> usize {
        self.ray_query_bytes
    }

    /// Surface crossings the census counted across the whole batch, before
    /// arc-location dedup. The ray-query charge scales with this, not with the
    /// shell's triangle count.
    #[must_use]
    pub const fn ray_crossings(&self) -> usize {
        self.ray_crossings
    }

    /// Drain and vent candidate rays in the batch.
    #[must_use]
    pub const fn candidate_count(&self) -> usize {
        self.candidate_count
    }
}

/// The registered policy the preflight sizes against. The two band widths that
/// used to live here moved into [`label_chambers`], which is now the one place
/// that decides what a chamber's low band and top band are; the preflight
/// reads the resulting member counts instead of re-deriving them.
#[derive(Clone, Copy)]
struct PlannerPolicy {
    drain_direction_count: usize,
    vent_direction_count: usize,
    lobe_min_ml: f64,
    vent_sample_divisor: usize,
    exit_ceiling_mm: f64,
}

/// Plans the exact frozen H1a candidates through one canonical batched native
/// ray query. On refusal it returns the untouched move-only candidate.
pub fn plan_drains(candidate: ShellCandidate) -> Result<PlannedShell, DrainPlanningFailure> {
    match plan(&candidate) {
        Ok((drain_plan, planner_accounting)) => {
            let (shell, cavity, cavity_field) = candidate.into_parts();
            Ok(PlannedShell {
                shell,
                cavity,
                cavity_field,
                drain_plan,
                planner_accounting,
            })
        }
        Err(reason) => Err(DrainPlanningFailure {
            candidate: Box::new(candidate),
            reason,
        }),
    }
}

fn plan(
    candidate: &ShellCandidate,
) -> Result<(DrainPlan, PlannerAccounting), DrainPlanningRefusal> {
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
    let lobe_band_mm = policy.hollow_lobe_band_mm().diagnostic().get();
    // Label the chambers first: every capacity below is a sum over them, and
    // the site searches that follow are run once per chamber rather than once
    // for the whole cavity. This is the single pass that serves them all --
    // nothing here re-walks the grid per chamber.
    let chambers = label_chambers(
        field,
        lobe_band_mm,
        vent_top_band_mm,
        candidate.cavity().drain_planning_cap_bytes(),
    )?;
    let preflight = planner_preflight(
        candidate,
        field,
        &chambers,
        resources,
        native.peak_bytes(),
        PlannerPolicy {
            drain_direction_count,
            vent_direction_count,
            lobe_min_ml: policy.hollow_lobe_min_ml().diagnostic().get(),
            vent_sample_divisor,
            exit_ceiling_mm: policy.hollow_exit_ray_max_mm().diagnostic().get(),
        },
    )?;
    debug_assert!(preflight.requested_bytes <= candidate.cavity().drain_planning_cap_bytes());
    debug_assert!(preflight.normalized_direction_bytes > 0);
    let lobes = low_lobes(
        field,
        &chambers,
        lobe_band_mm,
        policy.hollow_lobe_min_ml().diagnostic().get(),
        candidate.cavity().drain_planning_cap_bytes(),
    )?;
    let vent_sites = high_sites(
        field,
        &chambers,
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
                chamber: lobe.chamber,
                site,
                origin: lobe.origin,
                direction,
            });
        }
    }
    for (site, &(chamber, origin)) in vent_sites.iter().enumerate() {
        for &direction in &horizontal {
            let id = candidates.len();
            candidates.push(Candidate {
                id,
                role: HoleRole::Vent,
                lobe_id: None,
                chamber,
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
    // Count before charging before allocating: the census walks each ray's
    // BVH descent without storing a hit, so the batch's demand is a measured
    // crossing count rather than the rays x faces ceiling. A ray through a
    // closed hollow shell crosses the surface a handful of times no matter how
    // finely that surface is tessellated.
    let crossings = surface
        .cast_rays_census(&origins, &directions, ceiling)
        .map_err(|_| DrainPlanningRefusal::Kernel)?;
    if crossings.len() != candidates.len() {
        return Err(DrainPlanningRefusal::PreflightUnderestimated);
    }
    let ray_crossings = crossings
        .iter()
        .try_fold(0_usize, |total, value| total.checked_add(*value))
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let ray_query_bytes = ray_query_allocation_bytes(&crossings)?;
    let requested_bytes = preflight
        .requested_bytes
        .checked_add(ray_query_bytes)
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let cap = candidate.cavity().drain_planning_cap_bytes();
    if requested_bytes > cap {
        return Err(DrainPlanningRefusal::ResourceLimitExceeded {
            requested_bytes,
            cap_bytes: cap,
        });
    }

    // One gathered batch covers every drain and vent candidate. Results are
    // scattered solely by the stable candidate ID below.
    let (offsets, ts, _faces) = surface
        .cast_rays(&origins, &directions, ceiling)
        .map_err(|_| DrainPlanningRefusal::Kernel)?;
    // The census walk and the collecting walk are the same traversal, and
    // dedup only merges neighbours, so the flattened array can never be longer
    // than the counted crossings. Refuse rather than let a drift go unnoticed.
    if ts.len() > ray_crossings {
        return Err(DrainPlanningRefusal::PreflightUnderestimated);
    }

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

    // **One drain per lobe, one vent per chamber, and the pair travels
    // together.**
    //
    // A chamber ships its drains only if it also got its vent. A drain with no
    // air intake does not drain — the resin holds up behind a vacuum — so a
    // hole drilled into an unvented chamber weakens the print and buys
    // nothing, and the honest outcome is to leave that chamber solid. The
    // reverse is dropped for the same reason: a vent with no drain below it is
    // a hole in the model that no resin ever leaves through.
    //
    // **A chamber that cannot take a drain is not a refusal.** It contributes
    // no holes here, so the drilling stage's gravity map finds every one of
    // its voxels trapped, fills it back to solid, and reports the volume it
    // gave up. That is the whole point: never ship a sealed void full of
    // resin, and never throw away the rest of the model because one chamber
    // could not be reached. The model as a whole is still refused when NO
    // chamber produced a drained-and-vented pair, which is the single-chamber
    // behaviour unchanged.
    let mut holes = Vec::with_capacity(lobes.len() + chambers.count);
    let mut first_lobe_without_exit: Option<u32> = None;
    for chamber in 0..chambers.count {
        let mut chamber_drains = Vec::new();
        for (site, lobe) in lobes
            .iter()
            .enumerate()
            .filter(|(_, lobe)| lobe.chamber == chamber)
        {
            match best_site(&candidates, &paths, HoleRole::Drain, site) {
                Some(best) => chamber_drains.push(hole(
                    best.0,
                    best.1,
                    policy.hollow_drain_r_mm().diagnostic().get(),
                    keepout_before_mm,
                    drain_keepout_after_mm,
                )?),
                None => first_lobe_without_exit = first_lobe_without_exit.or(Some(lobe.id)),
            }
        }
        if chamber_drains.is_empty() {
            continue;
        }
        let Some(vent) = best_role(&candidates, &paths, HoleRole::Vent, chamber) else {
            continue;
        };
        holes.append(&mut chamber_drains);
        holes.push(hole(
            vent.0,
            vent.1,
            policy.hollow_vent_r_mm().diagnostic().get(),
            keepout_before_mm,
            vent_keepout_after_mm,
        )?);
    }
    // A hole is only ever pushed above by a chamber that got both a drain and
    // a vent, so an empty list means no chamber did — the whole model is
    // refused, and the refusal names whichever of the two was missing.
    if holes.is_empty() {
        return Err(match first_lobe_without_exit {
            Some(lobe_id) => DrainPlanningRefusal::NoExit {
                lobe_id,
                diagnostics: std::mem::take(&mut unavailable_candidates),
            },
            None => DrainPlanningRefusal::NoVentSite {
                diagnostics: std::mem::take(&mut unavailable_candidates),
            },
        });
    }
    Ok((
        DrainPlan {
            holes,
            unavailable_candidates,
        },
        PlannerAccounting {
            requested_bytes,
            ray_query_bytes,
            ray_crossings,
            candidate_count,
        },
    ))
}

/// Measured charge for the single `cast_rays` batch, from the kernel's
/// allocation-free census. `crossings[i]` is the number of `(t, face)` pairs
/// ray `i` collects before arc-location dedup, which bounds from above every
/// buffer that ray contributes to: its transient hit collection (and that
/// collection's in-place sort scratch), its retained deduplicated `ts`/`fs`
/// pair, and its slice of the two flattened arrays. Each ray's transient
/// collection is really freed before the next ray's is built; charging all of
/// them at once keeps the bound safely above the true peak.
///
/// This replaced a `rays x shell faces` product that assumed every ray crossed
/// every triangle. That scaled with mesh density instead of with the crossings
/// that exist, so a finely tessellated shell was charged thousands of times
/// what it allocates.
fn ray_query_allocation_bytes(crossings: &[usize]) -> Result<usize, DrainPlanningRefusal> {
    let allocator = |count: usize, size: usize, align: usize| {
        crate::allocator_allocation_bound(count, size, align)
            .map_err(|_| DrainPlanningRefusal::ResourceAccountingOverflow)
    };
    // `cast_rays` retains one `(Vec<f64>, Vec<i64>)` header pair per ray while
    // it serially builds both flattened arrays.
    let mut total = allocator(
        crossings.len(),
        std::mem::size_of::<(Vec<f64>, Vec<i64>)>(),
        std::mem::align_of::<(Vec<f64>, Vec<i64>)>(),
    )?;
    let mut flattened = 0_usize;
    for &count in crossings {
        let hits = allocator(
            count,
            std::mem::size_of::<(f64, u32)>(),
            std::mem::align_of::<(f64, u32)>(),
        )?;
        let row = [
            // the collected `(t, face)` pairs, and the merge scratch its
            // stable sort may allocate alongside them
            hits,
            hits,
            // the deduplicated `ts` and `fs` this ray retains
            allocator(
                count,
                std::mem::size_of::<f64>(),
                std::mem::align_of::<f64>(),
            )?,
            allocator(
                count,
                std::mem::size_of::<i64>(),
                std::mem::align_of::<i64>(),
            )?,
        ]
        .into_iter()
        .try_fold(0_usize, |sum, value| sum.checked_add(value))
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
        total = total
            .checked_add(row)
            .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
        flattened = flattened
            .checked_add(count)
            .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    }
    // the two flattened arrays the ragged rows are scanned into
    for bytes in [
        allocator(
            flattened,
            std::mem::size_of::<f64>(),
            std::mem::align_of::<f64>(),
        )?,
        allocator(
            flattened,
            std::mem::size_of::<i64>(),
            std::mem::align_of::<i64>(),
        )?,
    ] {
        total = total
            .checked_add(bytes)
            .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    }
    Ok(total)
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

/// The shortest legal exit of one role **within one chamber**.
///
/// Chamber-scoped because the vent it picks has to open into the chamber it
/// vents. A vent chosen from the whole cavity would let one chamber's air hole
/// stand in for another's, and the second chamber would then be drilled and
/// never drain.
fn best_role<'a>(
    candidates: &'a [Candidate],
    paths: &[Option<RayPath>],
    role: HoleRole,
    chamber: usize,
) -> Option<(&'a Candidate, RayPath)> {
    let mut best: Option<(&Candidate, RayPath)> = None;
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.role == role && candidate.chamber == chamber)
    {
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

/// Label the cavity's chambers and measure each one's own extent.
///
/// One flood over the members labels them; one further pass counts how many
/// members of each chamber sit in that chamber's own low band and in its own
/// top band, which is what sizes the planner's buffers. Two passes over the
/// field, not two per chamber.
fn label_chambers(
    field: &CavityField,
    low_band_mm: f64,
    top_band_mm: f64,
    cap: usize,
) -> Result<Chambers, DrainPlanningRefusal> {
    let cells = field.flat_count();
    // Cell indices are carried as `u32` in the flood stack, four bytes each
    // rather than eight, so this proves they fit before any of them is taken
    // — the same admission `crate::flow` and `crate::well_composed` make for
    // their own worksets.
    u32::try_from(cells).map_err(|_| DrainPlanningRefusal::ResourceAccountingOverflow)?;
    // Count before charging before allocating: the label array and the flood
    // stack are both one `u32` per cell in the worst case, and both are named
    // here before either exists.
    let workspace = cells
        .checked_mul(2 * std::mem::size_of::<u32>())
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    if workspace > cap {
        return Err(DrainPlanningRefusal::ResourceLimitExceeded {
            requested_bytes: workspace,
            cap_bytes: cap,
        });
    }
    let mut labels = vec![0_u32; cells];
    let mut low_z_mm: Vec<f64> = Vec::new();
    let mut high_z_mm: Vec<f64> = Vec::new();
    let mut stack: Vec<u32> = Vec::new();
    let mut count = 0_usize;
    for seed in field.member_flats() {
        if labels[seed] != 0 {
            continue;
        }
        count = count
            .checked_add(1)
            .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
        let label =
            u32::try_from(count).map_err(|_| DrainPlanningRefusal::ResourceAccountingOverflow)?;
        labels[seed] = label;
        stack.clear();
        stack.push(seed as u32);
        let mut lowest = f64::INFINITY;
        let mut highest = f64::NEG_INFINITY;
        while let Some(current) = stack.pop() {
            let current = current as usize;
            let z = field.world_at_flat(current)[2];
            lowest = lowest.min(z);
            highest = highest.max(z);
            for neighbor in neighbors(field, current) {
                if labels[neighbor] == 0 && field.contains_flat(neighbor) {
                    labels[neighbor] = label;
                    stack.push(neighbor as u32);
                }
            }
        }
        if !lowest.is_finite() || !highest.is_finite() {
            return Err(DrainPlanningRefusal::NonFinite {
                role: HoleRole::Drain,
            });
        }
        low_z_mm.push(lowest);
        high_z_mm.push(highest);
    }
    if count == 0 {
        return Err(DrainPlanningRefusal::NoVentSite {
            diagnostics: Vec::new(),
        });
    }
    let mut low_band_members = vec![0_usize; count];
    let mut high_band_members = vec![0_usize; count];
    for flat in field.member_flats() {
        let chamber = labels[flat] as usize - 1;
        let z = field.world_at_flat(flat)[2];
        if z < low_z_mm[chamber] + low_band_mm {
            low_band_members[chamber] += 1;
        }
        if z > high_z_mm[chamber] - top_band_mm {
            high_band_members[chamber] += 1;
        }
    }
    Ok(Chambers {
        labels,
        count,
        low_z_mm,
        high_z_mm,
        low_band_members,
        high_band_members,
    })
}

/// The low lobes of every chamber: the drain sites.
///
/// A lobe is a connected piece of one chamber's **own** low band — the band
/// `band_mm` above that chamber's lowest point, not above the model's. A
/// chamber with two feet has two lobes and gets two drains, which is the
/// behaviour the retired Python engine had and the reason it left 0.27 mL of
/// resin in a model where a single drain left 14 mL.
fn low_lobes(
    field: &CavityField,
    chambers: &Chambers,
    band_mm: f64,
    min_ml: f64,
    cap: usize,
) -> Result<Vec<Lobe>, DrainPlanningRefusal> {
    let in_low_band = |flat: usize| -> Option<usize> {
        let chamber = chambers.chamber_of(flat)?;
        (field.world_at_flat(flat)[2] < chambers.low_z_mm[chamber] + band_mm).then_some(chamber)
    };
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
        let Some(chamber) = in_low_band(flat) else {
            continue;
        };
        if labels[flat] >= 0 {
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
                    && in_low_band(neighbor).is_some()
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
            chamber,
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

/// Air-intake sites in the top band of **each** chamber.
///
/// **Why per chamber.** A chamber with a drain and no air intake does not
/// drain: the resin holds up under its own vacuum, exactly as a sealed can
/// does. Air can be shared between chambers only where they connect above the
/// resin line, and chambers that connect are one chamber here by construction
/// — so "a vent for every chamber" is the same rule as "a shared vent where
/// they connect", stated once. One vent site list per chamber is how the
/// planner gets there, and `plan` refuses to keep a chamber's drains unless
/// that chamber also got its vent.
///
/// Returns `(chamber, site)` pairs, in the field's own cell order, sampled at
/// the registered divisor within each chamber's own band.
fn high_sites(
    field: &CavityField,
    chambers: &Chambers,
    capacity: usize,
    top_band_mm: f64,
    sample_divisor: usize,
) -> Result<Vec<(usize, V3)>, DrainPlanningRefusal> {
    let in_top_band = |flat: usize| -> Option<usize> {
        let chamber = chambers.chamber_of(flat)?;
        (field.world_at_flat(flat)[2] > chambers.high_z_mm[chamber] - top_band_mm)
            .then_some(chamber)
    };
    let steps: Vec<usize> = chambers
        .high_band_members
        .iter()
        .map(|members| (members / sample_divisor).max(1))
        .collect();
    let mut seen = vec![0_usize; chambers.count];
    let mut sites = Vec::with_capacity(capacity);
    for flat in field.member_flats() {
        let Some(chamber) = in_top_band(flat) else {
            continue;
        };
        if seen[chamber].is_multiple_of(steps[chamber]) {
            sites.push((chamber, field.world_at_flat(flat)));
        }
        seen[chamber] += 1;
    }
    if sites.len() > capacity {
        return Err(DrainPlanningRefusal::PreflightUnderestimated);
    }
    Ok(sites)
}

/// Size every planner buffer before any of them exists, from the chambers
/// already labelled.
///
/// **The counts scale with the real work, and the caps do not move.** The
/// planner now runs its site search once per chamber, so the capacities below
/// are sums over chambers rather than one measurement of the whole cavity. A
/// model with many chambers therefore asks for more, and if it asks for more
/// than the drain-planning cap allows the answer is the same refusal it has
/// always been.
fn planner_preflight(
    candidate: &ShellCandidate,
    field: &CavityField,
    chambers: &Chambers,
    resources: H0ResourceDiagnostics,
    native_peak_bytes: usize,
    policy: PlannerPolicy,
) -> Result<PlannerPreflight, DrainPlanningRefusal> {
    let cap = candidate.cavity().drain_planning_cap_bytes();
    let minimum_lobe_voxels =
        (policy.lobe_min_ml * 1000.0 / field.pitch_mm().get().powi(3)).ceil() as usize;
    if minimum_lobe_voxels == 0 {
        return Err(DrainPlanningRefusal::ResourceAccountingOverflow);
    }
    // A chamber's low band can hold at most `members / minimum` lobes that
    // clear the registered minimum lobe volume, and a chamber whose whole low
    // band is under that minimum can still hold none -- but the loop below
    // walks it, so a chamber is never worth zero slots here.
    let lobe_capacity = chambers
        .low_band_members
        .iter()
        .try_fold(0_usize, |total, members| {
            total.checked_add(members / minimum_lobe_voxels)
        })
        .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?;
    let high_site_capacity = chambers
        .high_band_members
        .iter()
        .try_fold(0_usize, |total, members| {
            let step = (members / policy.vent_sample_divisor).max(1);
            total.checked_add(members.div_ceil(step))
        })
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
        // The chamber labelling that sizes everything above: one `u32` per
        // cell for the labels, one more for the flood stack, plus the four
        // per-chamber vectors.
        allocator(
            field_cells
                .checked_mul(2)
                .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?,
            std::mem::size_of::<u32>(),
            std::mem::align_of::<u32>(),
        )?,
        allocator(
            chambers
                .count
                .checked_mul(2)
                .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?,
            std::mem::size_of::<f64>(),
            std::mem::align_of::<f64>(),
        )?,
        allocator(
            chambers
                .count
                .checked_mul(3)
                .ok_or(DrainPlanningRefusal::ResourceAccountingOverflow)?,
            std::mem::size_of::<usize>(),
            std::mem::align_of::<usize>(),
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
            std::mem::size_of::<(usize, V3)>(),
            std::mem::align_of::<(usize, V3)>(),
        )?,
        // One hole per lobe (drains) plus one per chamber (vents).
        allocator(
            lobe_capacity
                .checked_add(chambers.count)
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
    // The `cast_rays` batch itself is NOT charged here. Its demand is a
    // crossing count, not a face count, and no bound on it worth having exists
    // before the query index does; `plan` charges it from the kernel's
    // allocation-free census immediately before the batch allocates. What this
    // preflight owes that step is the census's own result vector, which is one
    // count per ray and is known here.
    let ray_census_counts = allocator(
        candidate_capacity,
        std::mem::size_of::<usize>(),
        std::mem::align_of::<usize>(),
    )?;
    let requested = [
        retained_shell,
        retained_field,
        lobe_workspace,
        candidate_buffers,
        sampled_wall_buffers,
        native_peak_bytes,
        ray_census_counts,
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
    use crate::{create_shell, registered_settings, CavityEvidence, RealizedWall};
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

    /// The chambers of a field, at the registered band widths, with no
    /// memory ceiling in the way. Tests that exercise one helper still have
    /// to go through the same labelling production does.
    fn test_chambers(field: &CavityField, low_band_mm: f64, top_band_mm: f64) -> Chambers {
        label_chambers(field, low_band_mm, top_band_mm, usize::MAX).expect("field fits")
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
        create_shell(
            load_binary_stl(path, WeldPolicy::None).expect("frozen input"),
            registered_settings(),
        )
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
        let lobes = low_lobes(
            &full,
            &test_chambers(&full, 15.0, 15.0),
            15.0,
            0.5,
            usize::MAX,
        )
        .expect("field fits");
        assert_eq!(lobes.len(), 1);
        assert_eq!(lobes[0].id, 1);
        assert_eq!(lobes[0].chamber, 0);
        assert_eq!(lobes[0].voxels, 4_000);

        let mut under = Array3::from_elem((10, 20, 20), true);
        under[(0, 0, 0)] = false;
        let under = field(under);
        assert!(low_lobes(
            &under,
            &test_chambers(&under, 15.0, 15.0),
            15.0,
            0.5,
            usize::MAX
        )
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
                chamber: 0,
                site: 0,
                origin: [0.0; 3],
                direction: [0.0, 0.0, -1.0],
            },
            Candidate {
                id: 1,
                role: HoleRole::Drain,
                lobe_id: Some(1),
                chamber: 0,
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
                chamber: 0,
                site: 0,
                origin: [0.0; 3],
                direction: [0.0, 0.0, -1.0],
            },
            Candidate {
                id: 1,
                role: HoleRole::Drain,
                lobe_id: Some(7),
                chamber: 0,
                site: 0,
                origin: [0.0; 3],
                direction: [1.0, 0.0, -1.0],
            },
            Candidate {
                id: 2,
                role: HoleRole::Drain,
                lobe_id: Some(7),
                chamber: 0,
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
            chamber: 0,
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
            chamber: 0,
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

    /// The Sphere's drain and vent plan, pinned against the re-recorded
    /// frozen baseline.
    ///
    /// **Re-recorded 2026-08-12 by FU-51, gated on FU-61.** FU-61 verified on
    /// the corrected engine that every drain and vent bore fully penetrates —
    /// outer surface to cavity void, at full bore width — under worst-case
    /// placement on the 0.5 mm voxel-stepped cavity surface: measured margin
    /// `>= 4.0 mm` by construction (the registered `HOLLOW_EXTRA_DRAIN_POLICY`
    /// extension ladder), `4.50 mm` measured worst case at full bore width on
    /// this model, 5.2x the `0.5 * sqrt(3) ~= 0.866 mm` voxel body diagonal.
    /// **No new margin constant was needed.** Only with that confirmation in
    /// hand does `corpus/baseline/Sphere.pipeline.json` get the ONE
    /// deliberate, recorded regeneration `development.md` reserves for it —
    /// never a side effect of another change, and not taken while the
    /// bore-penetration guarantee was still unproven.
    ///
    /// **History, for provenance (old -> new).** The originally frozen
    /// Python artifact recorded a drain at `xyz = [0, 0, 0.5]`, `wall_mm =
    /// 1.0`, and a vent at `xyz = [0, 0, 19.5]` (axis `+x`), `wall_mm = 4.0`,
    /// under a **stated 2.5 mm wall** — a drilled exit through a quarter of
    /// the requested wall, because the pre-2026-08-10 cavity field had no
    /// exterior margin (the Sphere's six polar tangent points read as deep
    /// interior). The 2026-08-10 exterior-margin fix moved the drain to
    /// `z = 2.5` and the vent to `z = 16.0`; the 2026-08-12 FU-53
    /// exact-distance cavity selection moved them again. **This re-record
    /// replaces the stale Python-era numbers with what that same,
    /// bore-verified engine produces today:** drain `xyz = [0, 0, 3.5]`,
    /// `wall_mm = 4.0`; vent `xyz = [-1.5, -4.0, 15.0]` (axis `-y`, a
    /// different equal-cost tie-break on this body of revolution than the
    /// frozen `+x`), `wall_mm = 5.0`. The old numbers are recorded here, not
    /// in the baseline file, which now states only what the engine does.
    ///
    /// **Re-recorded a SECOND time, 2026-08-13, by the owner ruling on the
    /// wall target and minimum wall.** Zeroing
    /// `HOLLOW_CAVITY_MARCH_MARGIN_MM` cuts the Sphere's cavity at the
    /// requested 2.5 mm instead of 3.366 mm, so the void reaches further out
    /// and both bores meet it sooner: drain `[0, 0, 3.5]` -> `[0, 0, 3.0]`
    /// with `wall_mm` 4.0 -> 3.5, vent `[-1.5, -4.0, 15.0]` ->
    /// `[-0.5, -5.0, 15.5]` with `wall_mm` 5.0 -> 4.0. **FU-61's
    /// bore-penetration guarantee, which gated the first re-record, is not
    /// weakened by this:** it rests on the registered
    /// `HOLLOW_EXTRA_DRAIN_POLICY` extension ladder, which is untouched, and
    /// a shorter sampled exit is LESS wall to penetrate, not more. No margin
    /// constant was needed for this re-record either.
    ///
    /// The `lobe_ml` field beside these is deliberately left as it was. It
    /// was not re-recorded by FU-51 and nothing in the engine reads it, so
    /// regenerating it here would be exactly the silent side-effect
    /// re-freeze this protocol exists to prevent; it is stale Python-era
    /// data and is recorded as such rather than quietly refreshed.
    #[test]
    fn frozen_shell_field_has_an_enclosed_drain_and_vent_plan() {
        let baseline = frozen_hollow_report();
        let planned = plan_drains(frozen_candidate()).expect("source field must be usable");
        let holes = planned.drain_plan().holes();
        assert_eq!(holes.len(), 2);
        assert_eq!(holes[0].role, HoleRole::Drain);
        assert_eq!(holes[0].lobe_id, Some(1));
        assert_eq!(holes[1].role, HoleRole::Vent);
        let xyz = |value: &serde_json::Value| {
            let values = value.as_array().expect("baseline xyz array");
            [
                Finite::new(values[0].as_f64().expect("x")).expect("finite x"),
                Finite::new(values[1].as_f64().expect("y")).expect("finite y"),
                Finite::new(values[2].as_f64().expect("z")).expect("finite z"),
            ]
        };

        // What the re-recorded baseline states, asserted rather than
        // described, so a future accidental re-freeze cannot drift unnoticed.
        assert_eq!(
            xyz(&baseline["drains"][0]["xyz"]),
            [
                Finite::new(0.0).expect("finite"),
                Finite::new(0.0).expect("finite"),
                Finite::new(3.0).expect("finite"),
            ]
        );
        assert_eq!(
            baseline["drains"][0]["wall_mm"]
                .as_f64()
                .expect("drain wall_mm"),
            3.5
        );
        assert_eq!(
            xyz(&baseline["vent"]["xyz"]),
            [
                Finite::new(-0.5).expect("finite"),
                Finite::new(-5.0).expect("finite"),
                Finite::new(15.5).expect("finite"),
            ]
        );
        assert_eq!(
            baseline["vent"]["wall_mm"].as_f64().expect("vent wall_mm"),
            4.0
        );

        // What the engine produces now, pinned **whole** and required to
        // equal the freshly re-recorded baseline exactly: a deliberate
        // regeneration means the record and the engine agree the moment it
        // lands, so this is an equality (a regression pin), not a divergence
        // check. A future engine change that moves either hole must fail
        // this, not silently redefine the baseline.
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
        // The drain exits straight down and the vent exits along -y (the
        // re-recorded axis; the superseded Python-era record held +x), and
        // both radii are the two registered values, all now read straight
        // off the re-recorded baseline rather than asserted as a divergence.
        assert_eq!(holes[0].direction, xyz(&keepouts[0]["axis"]));
        // 2026-08-13: Craig ruled the drain bore to 2.5 mm (radius 1.25 mm);
        // the vent bore is untouched. See HOLLOW_DRAIN_R_MM's provenance.
        assert_eq!(holes[0].radius_mm.get().to_bits(), 1.25_f64.to_bits());
        assert_eq!(holes[1].direction, xyz(&keepouts[1]["axis"]));
        assert_eq!(holes[1].radius_mm.get().to_bits(), 1.25_f64.to_bits());

        // **The keepout volumes support planning is handed.** The frozen
        // artifact's endpoints are pinned above through its axes; these are the
        // engine's own, and they are pinned twice over. First as values, so a
        // moved keepout cannot pass silently:
        // holes[0]'s keepout moved 2026-08-12 with its centre, FU-53 (the
        // note above holes[0].center_mm), and again on 2026-08-13 with the
        // owner ruling that zeroed the march margin: 7.5 -> 7.0, tracking
        // the centre's own 3.5 -> 3.0 exactly.
        assert_eq!(
            holes[0].keepout_start_mm,
            [
                Finite::new(0.0).expect("finite"),
                Finite::new(0.0).expect("finite"),
                Finite::new(7.0).expect("finite"),
            ]
        );
        assert_eq!(
            holes[0].keepout_end_mm,
            [
                Finite::new(0.0).expect("finite"),
                Finite::new(0.0).expect("finite"),
                Finite::new(-4.5).expect("finite"),
            ]
        );
        // holes[1]'s keepout moved with its centre and its realised exit wall
        // (both noted above), 2026-08-12 by FU-53 and again 2026-08-13 by
        // the owner ruling, tracking the vent centre's own move.
        assert_eq!(
            holes[1].keepout_start_mm,
            [
                Finite::new(-0.5).expect("finite"),
                Finite::new(-1.0).expect("finite"),
                Finite::new(15.5).expect("finite"),
            ]
        );
        assert_eq!(
            holes[1].keepout_end_mm,
            [
                Finite::new(-0.5).expect("finite"),
                Finite::new(-14.0).expect("finite"),
                Finite::new(15.5).expect("finite"),
            ]
        );
        // And then as the relation those values are supposed to satisfy, which
        // is what makes them more than four transcribed triples: a keepout runs
        // from `keepout_before` behind the hole centre to `keepout_after`
        // beyond its **sampled exit**, along the hole's own direction. Both
        // clearances are the registered pair, so this catches a centre, a
        // direction or an exit that moved without its keepout moving with it —
        // the frozen artifact satisfies the same relation with its own numbers.
        let along = |centre: [Finite; 3], direction: [Finite; 3], distance: f64| {
            [
                centre[0].get() + direction[0].get() * distance,
                centre[1].get() + direction[1].get() * distance,
                centre[2].get() + direction[2].get() * distance,
            ]
        };
        let as_f64 = |point: [Finite; 3]| point.map(|value| value.get());
        for (hole, before_mm, after_mm) in [(&holes[0], 4.0, 4.0), (&holes[1], 4.0, 5.0)] {
            assert_eq!(
                as_f64(hole.keepout_start_mm),
                along(hole.center_mm, hole.direction, -before_mm),
                "{:?} keepout start is not {before_mm} mm behind its own centre",
                hole.role
            );
            assert_eq!(
                as_f64(hole.keepout_end_mm),
                along(
                    hole.center_mm,
                    hole.direction,
                    hole.sampled_exit_mm.get() + after_mm
                ),
                "{:?} keepout end is not {after_mm} mm beyond its own sampled exit",
                hole.role
            );
        }
        // The same relation holds of the frozen artifact, which is what lets
        // the two records be compared at all rather than merely both recorded.
        assert_eq!(
            xyz(&keepouts[0]["p1"]).map(|value| value.get()),
            along(
                xyz(&baseline["drains"][0]["xyz"]),
                xyz(&keepouts[0]["axis"]),
                baseline["drains"][0]["wall_mm"]
                    .as_f64()
                    .expect("drain wall_mm")
                    + 4.0
            )
        );

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
            cavity_repair: crate::CavityRepairEvidence {
                critical_cells: 0,
                repaired_voxels: 0,
                cell_visits: 0,
                full_grid_passes: 0,
            },
            cavity_distance_index_bytes: 0,
            cavity_distance_exact_query_points: 0,
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
                realized_wall: RealizedWall::measured(Finite::new(2.5).expect("finite")),
                cavity_components: 1,
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
            registered_settings(),
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
        let chambers = test_chambers(
            unlimited.cavity_field(),
            policy.hollow_lobe_band_mm().diagnostic().get(),
            policy.hollow_vent_top_band_mm().diagnostic().get(),
        );
        let preflight = planner_preflight(
            &unlimited,
            unlimited.cavity_field(),
            &chambers,
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
                lobe_min_ml: policy.hollow_lobe_min_ml().diagnostic().get(),
                vent_sample_divisor: usize::try_from(
                    policy.hollow_vent_sample_divisor().diagnostic().get(),
                )
                .expect("sample divisor fits usize"),
                exit_ceiling_mm: policy.hollow_exit_ray_max_mm().diagnostic().get(),
            },
        )
        .expect("unlimited preflight");
        assert_eq!(preflight.normalized_direction_bytes, 480);
        // The whole charge, including the ray-query batch the census measures
        // once the query index exists.
        let exact = plan(&unlimited).expect("uncapped plan").1.requested_bytes();
        assert!(exact > preflight.requested_bytes);

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

    /// The whole point of the census: the ray-query charge is a function of
    /// the crossings that exist, not of how finely the shell is tessellated.
    /// Restore the retired `candidate_capacity * tris` product and the closed
    /// form below is wrong by three orders of magnitude.
    #[test]
    fn the_ray_query_charge_follows_counted_crossings_not_the_shell_face_count() {
        let candidate = frozen_candidate();
        let faces = candidate.mesh().tris().len();
        let (_, accounting) = plan(&candidate).expect("frozen field must retain an enclosed plan");
        let rays = accounting.candidate_count();
        let crossings = accounting.ray_crossings();
        assert!(rays > 0 && crossings > 0);

        // A ray through a closed hollow shell crosses the surface a handful of
        // times; the retired charge assumed it crossed every triangle.
        assert!(
            crossings * 100 < rays * faces,
            "census counted {crossings} crossings against the {rays} rays x {faces} faces ceiling"
        );

        // Closed form of the honest charge: 128 bytes per counted crossing
        // (the transient hit pair and its sort scratch, the retained ts/fs
        // pair, and both flattened arrays, each doubled for allocator slack),
        // plus per-allocation headers and rounding that depend only on the ray
        // count. Nothing in it mentions a face.
        assert!(
            accounting.ray_query_bytes() >= 128 * crossings + 160 * rays + 48,
            "charge {} fell under the closed-form minimum",
            accounting.ray_query_bytes()
        );
        assert!(
            accounting.ray_query_bytes() <= 128 * crossings + 192 * rays + 80,
            "charge {} rose above the closed-form ceiling",
            accounting.ray_query_bytes()
        );

        // The retired shape billed 96 bytes per (ray, face) pair.
        assert!(
            accounting.ray_query_bytes() * 20 < rays * faces * 96,
            "charge {} is not decisively under the rays x faces product",
            accounting.ray_query_bytes()
        );
    }

    /// The charge must never fall below what the batch really allocates, so
    /// the counted crossings have to bound the deduplicated output they pay
    /// for, ray by ray.
    #[test]
    fn the_ray_census_bounds_every_row_the_batch_actually_returns() {
        let candidate = frozen_candidate();
        let field = candidate.cavity_field();
        let native = NativeSurface::estimate_index_memory_from_indexed(
            candidate.mesh().verts(),
            candidate.mesh().tris(),
            field.pitch_mm().get(),
        )
        .expect("native preflight");
        let surface = NativeSurface::from_indexed(
            candidate.mesh().verts(),
            candidate.mesh().tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: field.pitch_mm().get(),
                max_grid_bytes: native.max_axis_grid_bytes(),
            },
        )
        .expect("native surface");
        let origins: Vec<V3> = field
            .member_flats()
            .step_by(97)
            .map(|flat| field.world_at_flat(flat))
            .collect();
        assert!(origins.len() > 8);
        let directions = vec![[0.0, 0.0, 1.0]; origins.len()];
        let counts = surface
            .cast_rays_census(&origins, &directions, 45.0)
            .expect("census");
        let (offsets, ts, _faces) = surface
            .cast_rays(&origins, &directions, 45.0)
            .expect("batch");
        assert_eq!(counts.len(), origins.len());
        assert!(counts.iter().any(|count| *count > 0));
        for (ray, count) in counts.iter().enumerate() {
            let deduplicated = (offsets[ray + 1] - offsets[ray]) as usize;
            assert!(
                deduplicated <= *count,
                "ray {ray} returned {deduplicated} rows against {count} counted crossings"
            );
        }
        assert!(ts.len() <= counts.iter().sum::<usize>());
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
        let chambers = test_chambers(
            unlimited.cavity_field(),
            policy.hollow_lobe_band_mm().diagnostic().get(),
            policy.hollow_vent_top_band_mm().diagnostic().get(),
        );
        let preflight = planner_preflight(
            &unlimited,
            unlimited.cavity_field(),
            &chambers,
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
                lobe_min_ml: policy.hollow_lobe_min_ml().diagnostic().get(),
                vent_sample_divisor: usize::try_from(
                    policy.hollow_vent_sample_divisor().diagnostic().get(),
                )
                .expect("sample divisor fits usize"),
                exit_ceiling_mm: policy.hollow_exit_ray_max_mm().diagnostic().get(),
            },
        )
        .expect("unequal-capacity planner preflight");
        let exact = plan(&unlimited)
            .expect("uncapped unequal-capacity plan")
            .1
            .requested_bytes();
        assert!(exact > preflight.requested_bytes);

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
