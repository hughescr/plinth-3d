//! **A lander joins a support arm far enough BELOW its head that the section
//! carrying the sum clears the seat** (head-join, 2026-09-29).
//!
//! # The defect
//!
//! A member that lands on a support arm's HEAD (at the arm's own seat layer,
//! or one under it) hands the arm its load right at the contact. Since the
//! contact-sizing rule (2026-09-29, `down_sweep::run`) that load is booked one
//! slot under the contact, so the ledger's contact is sized from the arm's own
//! peel. The GEOMETRY did not follow. The drawn shaft is sum-sized from one
//! slot (0.05 mm) under the seat plane, and on an arm that leans the ring of
//! that section is tilted: on the Round 25 ring stiffener (arms 39, 41 and 44,
//! about 53° from vertical) its upper edge rises about 0.52 mm and crosses the
//! seat plane. The seat trim cuts it at the plane, so the cut section lies on
//! the model face. The file's contact was the own-sized ball joined with a
//! trimmed sum-sized section, and on arms 39 and 44 that shaft was wider than
//! the ball (`lanes/contact-sizing.md` §5). The lander's own end sat at the
//! parent's seat point, inside the ball, and its solid lay on the face.
//!
//! # The rule
//!
//! A lander may join the load path under a support arm's head only where
//!
//! 1. the section that carries the SUM stands at least the registered final
//!    clearance (`FINAL_CLEARANCE_GAP_MM`, 0.3 mm, the owner's field ruling of
//!    2026-08-22) from the seat plane of the member it joins, on the support
//!    side — so above the join the head arm carries its own peel only, and
//!    everything of it in its seat zone is sized from its own load; and
//! 2. the lander's own solid, re-landed there, keeps the two-tier clearance
//!    from the model (the same ruling): the placement margin (`STANDOFF_MM`,
//!    0.6 mm) where any such join exists, else the 0.3 mm hard limit. A join
//!    that only moved the lander's contact with the model somewhere else would
//!    not be a fix.
//!
//! The joins tried are the head arm's own shaft under the booked slot, then
//! the member it lands on, and so on down its landing chain (support arms
//! only), highest first. The first that passes both tests at the better tier
//! wins. Measured on the Round 25 stiffener: no slot of arm 39 itself lets
//! lander 69 clear the ring's lower corner (its straight segment clips it by
//! 0.07 to 0.50 mm at every slot), and arm 39's own trunk, arm 4, does at and
//! below layer 57.
//!
//! **Test 1 is measured on the member as the file draws it.** The drawn shaft
//! starts at the tip ball's centre and runs straight to the base
//! (`plinth_support::lower_sweep_arms`). The ball centre and the seat plane
//! come from [`plinth_support::tip_contact_discs`], which is the lowering's own
//! derivation. The sum-sized span starts at the top of the slot the load is
//! booked into, or at the ball centre where that is lower. Its ring is normal
//! to the shaft axis `m`, so its closest point to the plane is
//! `d(centre) − R·√(1 − (n·m)²)` for a plane of outward normal `n`, with `R`
//! the DRAWN radius there (junction raises included).
//!
//! **Test 2 is the kernel's clearance predicate**
//! ([`geom_kernel::NativeSurface::segment_clearance_classified`], the one the
//! legacy engine's brace and rewire stages use): the lander's drawn segment
//! from its seat to the join, at its widest drawn radius, against the model
//! surface. The part of the segment inside the lander's own contact (its seat
//! to the far side of its tip ball) is not tested: that is where it is meant
//! to touch. A row the predicate could not decide ([`UpperBound`] or
//! [`Unmeasured`]) does not pass. Members that rest on a moved lander are
//! re-landed on its new axis and must be no nearer the model than they were,
//! up to the same tier.
//!
//! [`UpperBound`]: geom_kernel::SegmentClearanceClassified::UpperBound
//! [`Unmeasured`]: geom_kernel::SegmentClearanceClassified::Unmeasured
//!
//! A vertical arm under a flat face is untouched when its ball already puts the
//! sum-sized span deep enough (Round 25 arm 42: 0.468 mm, over 0.3 mm).
//!
//! **Only HEAD joins** — a lander at the arm's seat layer or its top alive
//! layer, the two landings whose load reaches the contact slot, which is the
//! contact-sizing rule's own definition. The seat plane is the model face near
//! the contact; a join well down a shaft is measured against the same
//! infinite plane only where that plane is still a fair stand-in for the
//! model, and under a steep face it is not (Round 25 arm 6: its face is 75°
//! from horizontal, and the plane passes 1.25 mm below the model's lowest
//! point where its lander joins). Such joins are not this rule's.
//!
//! # What moves, and what it is held to
//!
//! The lander is re-landed along the straight segment from its own seat, with
//! the axis refusals a remedy-1 merge uses (`slenderness::segment_path`: model
//! material, another member, a drain's escape cylinder) and the lean law, as
//! well as test 2. Members that rest on the lander are re-landed on its new
//! axis at their own landing layer, transitively, under the same tests.
//!
//! **Emit and warn.** A join that cannot move — no join passes, a cross
//! (brace) tied to a member that would have to move, a lander that is not a
//! support arm, or no model surface to measure against — is kept as it is and
//! counted by its reason in [`HeadJoinOutcome`]. The EMITTED field (after the
//! footprint rule, which may slide a seat and so re-tilt a shaft) is surveyed
//! again, and what still reaches a seat zone is published.
//!
//! No new physical constant: the two clearances are the registered ruling,
//! and every other number is the down-sweep's, the lowering's or the grid's.
//! The surface index's parity-grid pitch and byte cap are the registered
//! `DECIMATION_PROBE_GRID_PITCH_MM` / `_MAX_GRID_BYTES` (the only registered
//! pair for a distance-probe surface index); the segment clearance is a BVH
//! walk and does not depend on that pitch.
//!
//! Batch-first: one survey per pass (the drawn profiles, the seat faces of
//! every trunk with a head lander in one closest-face query, and every
//! contact disc the pass needs in one call), the surface index built once per
//! run and only when a join is in a seat zone, and one clearance batch per
//! lander over all of its candidate joins.

use std::cell::OnceCell;

use geom_kernel::{NativeSurface, NativeSurfaceConfig, SegmentClearanceClassified};
use plinth_core::LayerPlan;
use plinth_mesh::Mesh;
use plinth_support::SweepArm;

use crate::slenderness::{segment_path, SlendernessContext};
use crate::up_sweep::ColumnHistory;
use crate::{Landing, StrutKind, SweepStrut};

/// What the head-join rule measured and did on one run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeadJoinOutcome {
    /// Whether the rule may move joins. `false` only under the sabotage knob;
    /// the emitted-field census still runs and publishes.
    pub enabled: bool,
    /// The clearance the sum-sized section must keep from the seat plane, and
    /// the hard limit for a moved member's solid, in mm (`FINAL_CLEARANCE_GAP_MM`).
    pub clearance_mm: f64,
    /// The placement margin a moved member's solid is searched at first, in mm
    /// (`STANDOFF_MM`).
    pub standoff_mm: f64,
    /// Head joins (a member landed at a support arm's seat layer or its top
    /// alive layer) examined by the first survey. The path-hit witness: zero
    /// means no member landed on a support arm's head at all.
    pub joins_examined: usize,
    /// Head joins whose sum-sized section reached the seat zone when the rule
    /// first ran, before it moved anything.
    pub joins_in_seat_zone_before: usize,
    /// Landers moved, over every pass.
    pub landers_moved: usize,
    /// Of those, the ones that went down the head arm's landing chain, onto a
    /// member below it, because no slot of the head arm itself passed.
    pub landers_moved_down_the_chain: usize,
    /// Of those, the ones placed at the 0.3 mm hard limit because no join
    /// passed the 0.6 mm placement margin.
    pub landers_at_hard_limit: usize,
    /// The largest single drop, in layers, from the old join to the new one.
    pub max_drop_layers: usize,
    /// The smallest clearance a moved lander's solid keeps from the model, in
    /// mm (`INFINITY` when nothing moved).
    pub min_lander_clearance_mm: f64,
    /// The load the moved landers hand the path below, in mm², summed.
    pub load_moved_mm2: f64,
    /// Members resting on a moved lander that were re-landed on its new axis.
    pub dependants_relanded: usize,
    /// Candidate joins measured by the clearance predicate (the path-hit
    /// witness for test 2).
    pub clearance_rows: usize,
    /// Rule passes that moved something (each followed by a down-sweep).
    pub passes: usize,
    /// Joins kept in the seat zone at the last pass, by reason (warnings, and
    /// states rather than events: the last pass describes the field).
    pub kept_no_clear_join: usize,
    pub kept_carrying_a_tie: usize,
    pub kept_not_an_arm: usize,
    pub kept_surface_unavailable: usize,
    /// Head joins whose seat geometry could not be measured (no seat plane or
    /// a degenerate shaft), so the rule could not grade them.
    pub joins_unmeasured: usize,
    /// **The emitted field**: head joins whose sum-sized section still reaches
    /// the seat zone, as `(lander id, trunk id)`. A warning; empty when the
    /// rule held everywhere.
    pub in_seat_zone_after: Vec<(u32, u32)>,
}

/// Everything the rule reads besides the strut set.
pub(crate) struct HeadJoinContext<'a> {
    pub slender: &'a SlendernessContext<'a>,
    pub mesh: &'a Mesh,
    pub layer_plan: &'a LayerPlan,
    pub plate_z_mm: f64,
    /// `FINAL_CLEARANCE_GAP_MM`: test 1, and the hard limit of test 2.
    pub clearance_mm: f64,
    /// `STANDOFF_MM`: test 2's placement margin.
    pub standoff_mm: f64,
    pub surface_pitch_mm: f64,
    pub surface_max_bytes: u64,
    /// The model surface, built on first need and kept for the run; `None`
    /// inside when it could not be built.
    pub surface: OnceCell<Option<NativeSurface>>,
}

impl HeadJoinContext<'_> {
    fn surface(&self) -> Option<&NativeSurface> {
        self.surface
            .get_or_init(|| {
                let verts = self.mesh.verts();
                let tris = self.mesh.tris();
                let estimate = NativeSurface::estimate_index_memory_from_indexed(
                    verts,
                    tris,
                    self.surface_pitch_mm,
                )
                .ok()?;
                let cap = usize::try_from(self.surface_max_bytes).unwrap_or(usize::MAX);
                if estimate.peak_bytes() > cap {
                    return None;
                }
                NativeSurface::from_indexed(
                    verts,
                    tris,
                    NativeSurfaceConfig {
                        grid_pitch_mm: self.surface_pitch_mm,
                        max_grid_bytes: estimate.max_axis_grid_bytes(),
                    },
                )
                .ok()
            })
            .as_ref()
    }
}

/// One member's measured tip: the drawn shaft from the ball centre `centre`
/// to the base `base`, the seat plane (`plane_point`, outward `normal`), the
/// ring spread `√(1 − (n·m)²)` of a section normal to that shaft, and how far
/// its own contact reaches down its axis from the drawn seat.
#[derive(Clone, Copy)]
struct Head {
    centre: [f64; 3],
    base: [f64; 3],
    plane_point: [f64; 3],
    normal: [f64; 3],
    spread: f64,
    seat: [f64; 3],
    contact_reach_mm: f64,
}

/// A head join, as the survey found it.
#[derive(Clone, Copy)]
struct Join {
    child: usize,
    parent: usize,
    layer: usize,
    /// The slot of the parent the lander's load is booked into.
    booked: usize,
}

impl Head {
    /// The clearance, in mm, between the seat plane and the nearest point of
    /// the sum-sized section of radius `radius` whose top is the top of layer
    /// `slot` (or the ball centre where that is lower). Negative is on the
    /// model side of the plane.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer index bounded by the plan's own layer count is exact in f64"
    )]
    fn clearance_at(&self, plate_z_mm: f64, layer_height_mm: f64, slot: usize, radius: f64) -> f64 {
        let slot_top_z = plate_z_mm + (slot + 1) as f64 * layer_height_mm;
        let rise = self.centre[2] - self.base[2];
        let top_z = slot_top_z.min(self.centre[2]);
        let t = ((self.centre[2] - top_z) / rise).clamp(0.0, 1.0);
        let at: [f64; 3] = std::array::from_fn(|axis| {
            self.centre[axis] + t * (self.base[axis] - self.centre[axis])
        });
        let distance =
            geom_kernel::vec3::dot(geom_kernel::vec3::sub(at, self.plane_point), self.normal);
        distance - radius * self.spread
    }
}

/// The contact slot's booking: a lander at the arm's seat layer or top alive
/// layer is booked one slot under the contact where there is one
/// (`down_sweep::run`), else into the contact itself.
fn booked_slot(parent: &SweepStrut) -> usize {
    let contact = parent.k_top.saturating_sub(1);
    if contact > parent.k_bottom {
        contact - 1
    } else {
        contact
    }
}

/// Every head join of `struts`: a member landed on a support arm at the arm's
/// seat layer or its top alive layer.
fn head_joins(struts: &[SweepStrut]) -> Vec<Join> {
    let mut joins = Vec::new();
    for (child, strut) in struts.iter().enumerate() {
        let Landing::Strut { id, layer } = strut.landing else {
            continue;
        };
        let parent = id as usize;
        let Some(host) = struts.get(parent) else {
            continue;
        };
        if parent == child
            || host.id != id
            || host.kind != StrutKind::Arm
            || host.k_bottom == usize::MAX
            || host.k_top <= host.k_bottom
            || layer + 1 < host.k_top
        {
            continue;
        }
        joins.push(Join {
            child,
            parent,
            layer,
            booked: booked_slot(host),
        });
    }
    joins
}

/// Indices, once each, in index order.
fn sorted_unique(mut indices: Vec<usize>) -> Vec<usize> {
    indices.sort_unstable();
    indices.dedup();
    indices
}

/// Each listed member's measured tip from its lowering input, by ONE batched
/// contact-disc call, scattered to strut index.
fn heads_of(
    count: usize,
    members: &[usize],
    arms: &[SweepArm],
    layer_plan: &LayerPlan,
) -> Vec<Option<Head>> {
    let mut heads = vec![None; count];
    let Ok(discs) = plinth_support::tip_contact_discs(arms, layer_plan) else {
        return heads;
    };
    for ((&index, arm), disc) in members.iter().zip(arms).zip(&discs) {
        let Some(disc) = disc else {
            continue;
        };
        let centre = disc.ball_centre_mm;
        let base = arm.base_mm;
        if centre[2] - base[2] <= 0.0 {
            continue;
        }
        let Some(axis) = geom_kernel::vec3::normalize(geom_kernel::vec3::sub(base, centre)) else {
            continue;
        };
        let along = geom_kernel::vec3::dot(disc.outward_normal, axis);
        heads[index] = Some(Head {
            centre,
            base,
            plane_point: disc.centre_mm,
            normal: disc.outward_normal,
            spread: (1.0 - along * along).max(0.0).sqrt(),
            seat: arm.seat_mm,
            contact_reach_mm: geom_kernel::vec3::norm(geom_kernel::vec3::sub(centre, arm.seat_mm))
                + disc.ball_radius_mm,
        });
    }
    heads
}

/// Measure the tips of `members` on the LIVE field: each one's seat face now
/// (a seat does not move before the footprint rule), its arm built unshifted
/// with its drawn profile, and one contact-disc batch.
fn measure(
    context: &HeadJoinContext<'_>,
    struts: &[SweepStrut],
    profiles: &[Vec<f64>],
    members: &[usize],
) -> Vec<Option<Head>> {
    let grid = context.slender.grid;
    let height = context.slender.layer_height_mm;
    let mut tips: Vec<SweepStrut> = members.iter().map(|&index| struts[index].clone()).collect();
    let faces =
        crate::tip_landing::tip_seat_faces(context.mesh, grid, &tips, context.plate_z_mm, height);
    let arms: Vec<SweepArm> = tips
        .iter_mut()
        .zip(&faces)
        .zip(members)
        .map(|((tip, face), &index)| {
            tip.seat_normal = face.map_or([0.0; 3], |(normal, _)| normal);
            let (seat, base) = crate::adapter::drawn_ends(grid, context.plate_z_mm, height, tip);
            crate::adapter::sweep_arm(
                tip,
                seat,
                base,
                face.map(|(_, point)| point),
                profiles[index].clone(),
            )
        })
        .collect();
    heads_of(struts.len(), members, &arms, context.layer_plan)
}

/// What one survey found.
struct Survey {
    examined: usize,
    unmeasured: usize,
    in_zone: Vec<Join>,
    heads: Vec<Option<Head>>,
    profiles: Vec<Vec<f64>>,
}

/// Grade `joins` on the given drawn profiles (indexed by strut) and heads.
fn grade(
    struts: &[SweepStrut],
    joins: Vec<Join>,
    heads: Vec<Option<Head>>,
    profiles: Vec<Vec<f64>>,
    plate_z_mm: f64,
    layer_height_mm: f64,
    clearance_mm: f64,
) -> Survey {
    let mut out = Survey {
        examined: joins.len(),
        unmeasured: 0,
        in_zone: Vec::new(),
        heads,
        profiles,
    };
    for join in joins {
        let host = &struts[join.parent];
        let Some(head) = out.heads.get(join.parent).copied().flatten() else {
            out.unmeasured += 1;
            continue;
        };
        let Some(&radius) =
            out.profiles[join.parent].get(join.booked.saturating_sub(host.k_bottom))
        else {
            out.unmeasured += 1;
            continue;
        };
        // Only a section WIDER than the arm's own contact can make the contact
        // more than own-load sized; a lander whose load does not widen the
        // shaft leaves nothing in the seat zone but the arm's own section.
        let own = host.tip_radius_mm();
        if radius <= own * (1.0 + 1.0e-9) {
            continue;
        }
        let clearance = head.clearance_at(plate_z_mm, layer_height_mm, join.booked, radius);
        if clearance < clearance_mm - 1.0e-9 {
            out.in_zone.push(join);
        }
    }
    out
}

/// Survey the LIVE field inside the sweep.
fn survey(context: &HeadJoinContext<'_>, struts: &[SweepStrut]) -> Survey {
    let height = context.slender.layer_height_mm;
    let joins = head_joins(struts);
    if joins.is_empty() {
        return grade(
            struts,
            joins,
            Vec::new(),
            Vec::new(),
            context.plate_z_mm,
            height,
            0.0,
        );
    }
    let (profiles, _) = crate::adapter::drawn_profiles(struts);
    let parents = sorted_unique(joins.iter().map(|join| join.parent).collect());
    let heads = measure(context, struts, &profiles, &parents);
    grade(
        struts,
        joins,
        heads,
        profiles,
        context.plate_z_mm,
        height,
        context.clearance_mm,
    )
}

/// **The emitted field's census**: how many head joins it has, and every one
/// whose sum-sized section still reaches the seat zone, as `(lander id, trunk
/// id)`, measured on the arms exactly as they go to the lowering
/// (`adapter::arms_of`, footprint moves included).
#[must_use]
pub fn census(
    struts: &[SweepStrut],
    arms: &[SweepArm],
    layer_plan: &LayerPlan,
    plate_z_mm: f64,
    layer_height_mm: f64,
    clearance_mm: f64,
) -> (usize, Vec<(u32, u32)>) {
    let joins = head_joins(struts);
    let examined = joins.len();
    if joins.is_empty() || arms.len() != struts.len() {
        return (examined, Vec::new());
    }
    let parents = sorted_unique(joins.iter().map(|join| join.parent).collect());
    let trunk_arms: Vec<SweepArm> = parents.iter().map(|&index| arms[index].clone()).collect();
    let heads = heads_of(struts.len(), &parents, &trunk_arms, layer_plan);
    let profiles = arms
        .iter()
        .map(|arm| arm.radius_profile_mm.clone())
        .collect();
    let listed = grade(
        struts,
        joins,
        heads,
        profiles,
        plate_z_mm,
        layer_height_mm,
        clearance_mm,
    )
    .in_zone
    .iter()
    .map(|join| (struts[join.child].id, struts[join.parent].id))
    .collect();
    (examined, listed)
}

/// Why a join could not be moved.
enum Refused {
    NoClearJoin,
    CarryingTie,
}

/// One tentative move: `(strut index, landing, k_bottom, path)`.
type Move = (usize, Landing, usize, Vec<u32>);

/// The point a member's drawn base sits at when it lands in `cell` at `layer`
/// (`adapter::drawn_ends`: the cell's centre, at the layer's bottom face).
#[expect(
    clippy::cast_precision_loss,
    reason = "a layer index bounded by the plan's own layer count is exact in f64"
)]
fn base_point(context: &HeadJoinContext<'_>, cell: usize, layer: usize) -> [f64; 3] {
    let grid = context.slender.grid;
    [
        grid.center_x_mm(grid.col_of(cell)),
        grid.center_y_mm(grid.row_of(cell)),
        context.plate_z_mm + layer as f64 * context.slender.layer_height_mm,
    ]
}

/// How far down a member's axis `axis` (unit, seat to base) its own seat zone
/// reaches: its tip ball, or, where further, the point where the member's
/// solid first stands `tier` mm clear of its OWN seat plane. The kernel's
/// clearance treats the solid as a capsule (the axis's distance to the
/// surface less the radius), so that point is where the axis stands
/// `tier + r` from the plane: `(tier + r) / (n·m)`. The zone test 1 grants a
/// head arm's own section, granted to the member's own seat. `None` when the
/// axis does not leave the seat plane at all.
fn own_seat_zone_mm(head: &Head, axis: [f64; 3], radius: f64, tier: f64) -> Option<f64> {
    let along = geom_kernel::vec3::dot(head.normal, axis);
    if along <= 0.0 {
        return None;
    }
    Some(head.contact_reach_mm.max((tier + radius) / along))
}

/// Test 2 over a batch of segments of one member, from its drawn seat to each
/// of `bases`, at its widest drawn radius, with its own seat zone untested
/// ([`own_seat_zone_mm`]): the clearance in mm for each, `None` where the
/// predicate did not decide or nothing is left to test.
fn solid_clearances(
    surface: &NativeSurface,
    head: &Head,
    radius: f64,
    bases: &[[f64; 3]],
    tier: f64,
) -> Vec<Option<f64>> {
    let n = bases.len();
    let seats = vec![head.seat; n];
    let radii = vec![radius; n];
    let zero = vec![0.0; n];
    let insets: Vec<f64> = bases
        .iter()
        .map(|&base| {
            geom_kernel::vec3::normalize(geom_kernel::vec3::sub(base, head.seat))
                .and_then(|axis| own_seat_zone_mm(head, axis, radius, tier))
                .unwrap_or(f64::INFINITY)
        })
        .collect();
    surface
        .segment_clearance_classified(&seats, bases, &radii, &radii, 0.0, &insets, &zero)
        .map_or_else(
            |_| vec![None; n],
            |rows| {
                rows.into_iter()
                    .map(SegmentClearanceClassified::clearance_mm)
                    .collect()
            },
        )
}

/// The segment `strut` would take from its own seat down to `(base_cell,
/// k_bottom)`, under the lean law and `segment_path`'s refusals, or `None`.
fn relanded_path(
    context: &HeadJoinContext<'_>,
    index: &ColumnHistory,
    strut: &SweepStrut,
    base_cell: usize,
    k_bottom: usize,
    host_id: u32,
) -> Option<Vec<u32>> {
    let slender = context.slender;
    let layers = strut.k_top.checked_sub(k_bottom)?;
    if layers == 0 {
        return None;
    }
    let lateral_mm = crate::cell_distance_mm(slender.grid, strut.cell, base_cell);
    if lateral_mm > 0.0 && slender.lean.descent_cost_mm(layers, lateral_mm).is_none() {
        return None;
    }
    segment_path(
        slender,
        index,
        strut.cell,
        strut.k_top,
        base_cell,
        k_bottom,
        Some(host_id),
    )
}

/// The cell a member stands in at `layer` once it has `path` from `k_bottom`.
fn cell_on(strut: &SweepStrut, path: &[u32], k_bottom: usize, layer: usize) -> Option<usize> {
    if layer == strut.k_top {
        return Some(strut.cell);
    }
    let slot = layer.checked_sub(k_bottom)?;
    path.get(slot).map(|cell| *cell as usize)
}

/// What the move planner reads besides the join itself.
struct Planner<'a, 'c> {
    context: &'a HeadJoinContext<'c>,
    index: &'a ColumnHistory,
    struts: &'a [SweepStrut],
    dependants: &'a [Vec<usize>],
    tied: &'a [bool],
    heads: &'a [Option<Head>],
    profiles: &'a [Vec<f64>],
    surface: &'a NativeSurface,
}

impl Planner<'_, '_> {
    fn widest(&self, member: usize) -> f64 {
        self.profiles[member]
            .iter()
            .copied()
            .fold(self.struts[member].tip_radius_mm(), f64::max)
    }

    /// Plan the lander's move onto `host` at `layer`, and every member resting
    /// on it re-landed on its new axis, transitively, each no nearer the model
    /// than it was, up to `tier` mm.
    fn plan(
        &self,
        child: usize,
        host: usize,
        layer: usize,
        tier: f64,
    ) -> Result<Vec<Move>, Refused> {
        let struts = self.struts;
        let base_cell = struts[host].cell_at(layer);
        let path = relanded_path(
            self.context,
            self.index,
            &struts[child],
            base_cell,
            layer,
            struts[host].id,
        )
        .ok_or(Refused::NoClearJoin)?;
        let mut moves: Vec<Move> = vec![(
            child,
            Landing::Strut {
                id: struts[host].id,
                layer,
            },
            layer,
            path,
        )];
        let mut visited = vec![false; struts.len()];
        visited[child] = true;
        let mut cursor = 0;
        while cursor < moves.len() {
            let moved = moves[cursor].0;
            let moved_bottom = moves[cursor].2;
            let moved_path = moves[cursor].3.clone();
            cursor += 1;
            if self.tied[moved] {
                return Err(Refused::CarryingTie);
            }
            for &dependant in &self.dependants[moved] {
                let strut = &struts[dependant];
                // A cross (or a mast) resting on a member that must move was
                // placed for the geometry it has; it is not re-planned here.
                if strut.kind != StrutKind::Arm {
                    return Err(Refused::CarryingTie);
                }
                if visited[dependant] {
                    continue;
                }
                let Landing::Strut { layer: at, .. } = strut.landing else {
                    continue;
                };
                if cell_on(&struts[moved], &moved_path, moved_bottom, at) == Some(strut.base_cell())
                {
                    continue;
                }
                // THE DEPENDANT FOLLOWS THE MEMBER IT RESTS ON: re-landed on its
                // new axis at its own landing layer, or, where that breaks the
                // lean law or a refusal, lower down the same member, highest
                // first — each no nearer the model than it was, up to the tier.
                let mut options: Vec<(usize, Vec<u32>, [f64; 3])> = Vec::new();
                for layer in (moved_bottom..=at).rev() {
                    let Some(cell) = cell_on(&struts[moved], &moved_path, moved_bottom, layer)
                    else {
                        continue;
                    };
                    if let Some(path) = relanded_path(
                        self.context,
                        self.index,
                        strut,
                        cell,
                        layer,
                        struts[moved].id,
                    ) {
                        options.push((layer, path, base_point(self.context, cell, layer)));
                    }
                }
                let head = self.heads[dependant].ok_or(Refused::NoClearJoin)?;
                let mut bases = vec![base_point(self.context, strut.base_cell(), strut.k_bottom)];
                bases.extend(options.iter().map(|option| option.2));
                let found =
                    solid_clearances(self.surface, &head, self.widest(dependant), &bases, tier);
                let required = found[0].map_or(tier, |before| tier.min(before)) - 1.0e-9;
                let chosen = options
                    .into_iter()
                    .zip(found.into_iter().skip(1))
                    .find(|(_, after)| after.is_some_and(|after| after >= required));
                let Some(((layer, dependant_path, _), _)) = chosen else {
                    return Err(Refused::NoClearJoin);
                };
                visited[dependant] = true;
                moves.push((
                    dependant,
                    Landing::Strut {
                        id: struts[moved].id,
                        layer,
                    },
                    layer,
                    dependant_path,
                ));
            }
        }
        Ok(moves)
    }
}

/// The joins a lander may be moved to, highest first: the head arm's own
/// shaft under the booked slot, then each member below it on its landing
/// chain (support arms only) from the layer the one above lands at.
fn candidate_joins(struts: &[SweepStrut], join: &Join) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut host = join.parent;
    let mut top = join.booked;
    let mut hops = 0;
    loop {
        let strut = &struts[host];
        let lowest = strut.k_bottom;
        if top > lowest {
            let highest = if host == join.parent { top - 1 } else { top };
            for layer in (lowest..=highest).rev() {
                out.push((host, layer));
            }
        } else if host != join.parent && top == lowest {
            out.push((host, lowest));
        }
        let Landing::Strut { id, layer } = strut.landing else {
            break;
        };
        let next = id as usize;
        hops += 1;
        if hops > struts.len()
            || struts.get(next).is_none_or(|next_strut| {
                next_strut.id != id || next_strut.kind != StrutKind::Arm || next == join.child
            })
        {
            break;
        }
        host = next;
        top = layer;
    }
    out
}

/// One pass of the rule over `struts`: survey, then move every head join whose
/// sum-sized section reaches the seat zone to the highest admissible join.
/// Returns how many landers it moved; the caller re-sizes the field with the
/// down-sweep whenever that is not zero, and runs it again, to a fixed point.
#[expect(
    clippy::too_many_lines,
    reason = "one pass: survey, index, measure, then per join the candidate batch, the tiers \
              and the move; split further it would pass the same dozen locals around"
)]
pub(crate) fn apply(
    context: &HeadJoinContext<'_>,
    struts: &mut [SweepStrut],
    outcome: &mut HeadJoinOutcome,
    first: bool,
) -> usize {
    let found = survey(context, struts);
    if first {
        outcome.joins_examined = found.examined;
        outcome.joins_in_seat_zone_before = found.in_zone.len();
    }
    outcome.joins_unmeasured = found.unmeasured;
    outcome.kept_no_clear_join = 0;
    outcome.kept_carrying_a_tie = 0;
    outcome.kept_not_an_arm = 0;
    outcome.kept_surface_unavailable = 0;
    if found.in_zone.is_empty() {
        return 0;
    }
    let Some(surface) = context.surface() else {
        outcome.kept_surface_unavailable = found.in_zone.len();
        return 0;
    };
    let height = context.slender.layer_height_mm;
    let n = struts.len();
    let mut dependants: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut tied = vec![false; n];
    for (index, strut) in struts.iter().enumerate() {
        if let Landing::Strut { id, .. } = strut.landing {
            if let Some(list) = dependants.get_mut(id as usize) {
                list.push(index);
            }
        }
        if let Some(host) = strut.seat_on_strut {
            if let Some(flag) = tied.get_mut(host as usize) {
                *flag = true;
            }
        }
    }
    // Every member that may move this pass — the landers in the zone and what
    // rests on them — is left OUT of the obstacle index, so none blocks its
    // own new path; each accepted move is stamped back in as it lands.
    let mut may_move = vec![false; n];
    let mut stack: Vec<usize> = found.in_zone.iter().map(|join| join.child).collect();
    while let Some(index) = stack.pop() {
        if std::mem::replace(&mut may_move[index], true) {
            continue;
        }
        stack.extend(dependants[index].iter().copied());
    }
    let mut obstacles = ColumnHistory::new(context.slender.grid);
    for (position, strut) in struts.iter().enumerate() {
        if may_move[position] || strut.k_bottom == usize::MAX || strut.k_top < strut.k_bottom {
            continue;
        }
        obstacles.stamp_shaft(
            strut.id,
            strut.k_bottom,
            strut.k_top,
            strut.cell,
            &strut.path,
        );
    }
    // Every member whose tip this pass needs, in ONE measurement: the hosts on
    // every candidate chain (test 1), and the landers and what rests on them
    // (test 2's contact reach).
    let candidates: Vec<Vec<(usize, usize)>> = found
        .in_zone
        .iter()
        .map(|join| candidate_joins(struts, join))
        .collect();
    let mut wanted: Vec<usize> = candidates.iter().flatten().map(|&(host, _)| host).collect();
    wanted.extend((0..n).filter(|&index| may_move[index]));
    let wanted = sorted_unique(wanted);
    let mut heads = measure(context, struts, &found.profiles, &wanted);
    for (index, head) in found.heads.iter().enumerate() {
        if head.is_some() && heads[index].is_none() {
            heads[index] = *head;
        }
    }

    let mut moved_count = 0;
    let mut moved_already = vec![false; n];
    for (join, chain) in found.in_zone.iter().zip(&candidates) {
        if moved_already[join.child] {
            continue;
        }
        if struts[join.child].kind != StrutKind::Arm {
            outcome.kept_not_an_arm += 1;
            continue;
        }
        // TEST 1 and the lean law, per candidate, from arithmetic alone.
        let lander = &struts[join.child];
        let sum_clears = |host: usize, layer: usize| -> bool {
            let Some(head) = heads[host] else {
                // A host below the head arm whose tip could not be measured:
                // its seat plane is unknown, and the join is not at its head.
                return host != join.parent;
            };
            let Some(&radius) = found.profiles[host].get(layer - struts[host].k_bottom) else {
                return false;
            };
            head.clearance_at(context.plate_z_mm, height, layer, radius)
                >= context.clearance_mm - 1.0e-9
        };
        let admissible: Vec<(usize, usize)> = chain
            .iter()
            .copied()
            .filter(|&(host, layer)| layer < lander.k_top && sum_clears(host, layer))
            .collect();
        // TEST 2, one batch over every candidate the arithmetic admitted.
        let Some(lander_head) = heads[join.child] else {
            outcome.kept_no_clear_join += 1;
            continue;
        };
        let bases: Vec<[f64; 3]> = admissible
            .iter()
            .map(|&(host, layer)| base_point(context, struts[host].cell_at(layer), layer))
            .collect();
        let widest = found.profiles[join.child]
            .iter()
            .copied()
            .fold(lander.tip_radius_mm(), f64::max);
        let tiers = [(context.standoff_mm, false), (context.clearance_mm, true)];
        let clearances: Vec<Vec<Option<f64>>> = tiers
            .iter()
            .map(|&(tier, _)| solid_clearances(surface, &lander_head, widest, &bases, tier))
            .collect();
        outcome.clearance_rows += bases.len() * tiers.len();
        let planner = Planner {
            context,
            index: &obstacles,
            struts,
            dependants: &dependants,
            tied: &tied,
            heads: &heads,
            profiles: &found.profiles,
            surface,
        };
        let mut refusal = Refused::NoClearJoin;
        let mut planned: Option<(usize, usize, f64, bool, Vec<Move>)> = None;
        'tiers: for (&(tier, at_hard_limit), measured) in tiers.iter().zip(&clearances) {
            for (&(host, layer), clearance) in admissible.iter().zip(measured) {
                let Some(clearance) = *clearance else {
                    continue;
                };
                if clearance < tier - 1.0e-9 {
                    continue;
                }
                match planner.plan(join.child, host, layer, tier) {
                    Ok(moves) => {
                        planned = Some((host, layer, clearance, at_hard_limit, moves));
                        break 'tiers;
                    }
                    Err(reason) => refusal = reason,
                }
            }
        }
        let Some((host, layer, clearance, at_hard_limit, moves)) = planned else {
            match refusal {
                Refused::NoClearJoin => outcome.kept_no_clear_join += 1,
                Refused::CarryingTie => outcome.kept_carrying_a_tie += 1,
            }
            continue;
        };
        outcome.max_drop_layers = outcome
            .max_drop_layers
            .max(join.layer.saturating_sub(layer));
        outcome.load_moved_mm2 += struts[join.child]
            .carried_mm2
            .first()
            .copied()
            .unwrap_or(0.0);
        outcome.dependants_relanded += moves.len() - 1;
        outcome.landers_moved_down_the_chain += usize::from(host != join.parent);
        outcome.landers_at_hard_limit += usize::from(at_hard_limit);
        outcome.min_lander_clearance_mm = outcome.min_lander_clearance_mm.min(clearance);
        for (member, landing, k_bottom, path) in moves {
            let strut = &mut struts[member];
            strut.landing = landing;
            strut.k_bottom = k_bottom;
            strut.path = path;
            // `carried_mm2` and `radius_mm` are now stale; the caller re-runs
            // the down-sweep, which re-sizes both from `ledger_len`.
            obstacles.stamp_shaft(
                strut.id,
                strut.k_bottom,
                strut.k_top,
                strut.cell,
                &strut.path,
            );
            moved_already[member] = true;
        }
        moved_count += 1;
    }
    outcome.landers_moved += moved_count;
    if moved_count > 0 {
        outcome.passes += 1;
    }
    moved_count
}

#[cfg(test)]
mod tests {
    use super::Head;

    /// A shaft leaning from a ball centre at the origin toward `(3, 0, −4)`
    /// (cos θ = 0.8 from vertical) under a flat face through `z = 0.2` with
    /// outward normal `−z`, one layer 0.05 mm high on a plate at `z = −10`.
    fn leaned() -> Head {
        Head {
            centre: [0.0, 0.0, 0.0],
            base: [3.0, 0.0, -4.0],
            plane_point: [0.0, 0.0, 0.2],
            normal: [0.0, 0.0, -1.0],
            spread: 0.6,
            seat: [0.0, 0.0, 0.2],
            contact_reach_mm: 0.0,
        }
    }

    /// Hand-derived: the section whose top is the top of layer 179 sits at
    /// `z = −10 + 180·0.05 = −1`, 1.2 mm below the plane; a 0.5 mm ring tilted
    /// by `sin θ = 0.6` rises 0.3 mm of that back, leaving 0.9 mm. Sabotage:
    /// dropping the ring's rise (`radius · spread`) reads 1.2 and goes red.
    #[test]
    fn a_tilted_ring_rises_toward_the_seat_by_its_radius_times_the_lean() {
        let clearance = leaned().clearance_at(-10.0, 0.05, 179, 0.5);
        assert!((clearance - 0.9).abs() < 1.0e-9, "{clearance}");
    }

    /// Above the ball centre the section cannot start: a slot whose top is
    /// above the centre is measured AT the centre (0.2 mm below the plane).
    #[test]
    fn a_section_never_starts_above_the_ball_centre() {
        let clearance = leaned().clearance_at(-10.0, 0.05, 250, 0.5);
        assert!((clearance - (0.2 - 0.3)).abs() < 1.0e-9, "{clearance}");
    }
}
