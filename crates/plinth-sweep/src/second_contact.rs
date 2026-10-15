//! **The acceptance contract for editing a finished first plan** (SC-0,
//! `SC-DESIGN.md` §4 and §6, with astra round 2 findings 1-3).
//!
//! Nothing in production calls this module yet: it is the contract the edit
//! transaction (SC-3) will be held to, written first and tested alone, so that
//! "keep the edit" can never mean "nobody measured what it did". Everything
//! here is READ-ONLY on a finished field: it never alters the geometry the first
//! plan (P1) emits.
//!
//! # What a finished field is compared on
//!
//! * **Canonical warning keys** ([`Key`], [`Warning`]): every warning class the
//!   run report carries, keyed by WHO is warned about (member ids, not
//!   counts), with a severity where the warning has a size. An equal count can
//!   hide a new defect on another member, so a count is never compared where a
//!   key can be. Lists are sorted by key and merged by worst severity: the same
//!   field gives the same list whatever order the checks ran in, at any thread
//!   count.
//! * **Authoritative FINAL predicates** ([`final_checks`]): the checks that
//!   depend on geometry the finishing stage changes AFTER they were first taken
//!   (landings, unbraced runs with their radius-dependent limit, brace size,
//!   the construction shake, escape) are recomputed on the finished field, so a
//!   late head resize or footprint slide cannot hide behind a counter taken
//!   before it (astra round 2 finding 3). The history counters in the report
//!   are kept for the log and are never compared where a final predicate exists.
//! * **The member-collision census** ([`crate::member_clearance::member_collisions`]),
//!   **the overgrown-merge capacity predicate** ([`overgrown_merges`], which
//!   covers inserted arms the repair pass skips), **the joint certificate row by
//!   row** ([`crate::heads::joint_certificate_rows`]) and **the ground a shrunk
//!   contact keeps** ([`crate::footprint::contacts_keep_ground`]).
//! * **A coverage guard**: [`classify_report`] destructures the run report and
//!   every nested outcome that can carry a warning WITHOUT `..`, so a field added
//!   to any of them fails to compile until it is classed here as per-member,
//!   global, or neutral.
//!
//! # The verdict
//!
//! [`compare`] sets a finished edit against the finished first plan: global
//! regressions first (never blamed; they discard the edit), then each warning key
//! that is new or worse. A regression on an OLD member is traced to a NEW arm
//! only by a structural link ([`Provenance`]); one with no link discards the edit
//! (every doubt resolves to P1). Warnings on new arms withdraw those arms.
//!
//! # Withdrawal
//!
//! [`withdrawal_closure`] and [`remap_for`] are the pure parts of dropping new
//! arms: the closure of everything that rests on them, the id remap
//! (`drop_members`'s own), and the remap of provenance and refused keys.

use std::collections::{BTreeMap, BTreeSet};

use crate::adapter::LoadStepTaper;
use crate::drain::{DrainSpec, EscapeReport};
use crate::grid::Grid;
use crate::heads::{JointCertRow, JointCertState};
use crate::landing_check::{self, LandingCheck, LandingDefect};
use crate::member_clearance::MemberCollision;
use crate::report::SweepRunReport;
use crate::slenderness::SlendernessLaw;
use crate::stage_shake::StageShake;
use crate::{Landing, SweepPlan, SweepPolicy, SweepStrut};

// ------------------------------------------------------------------ keys

/// The class of one warning. The derived order is the sort order of a keyed
/// list and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Class {
    /// A tip whose real contact is short of its plan. `a` = tip.
    ContactShort,
    /// A ball wider than the feature it sits on. `a` = tip.
    ContactOverFeature,
    /// A member with an unbraced run over its limit. `a` = member.
    Unbraced,
    /// A member's drawn solid within the final gap of the model. `a` = member.
    ModelClearance,
    /// A tip buried in another member. `a` = tip, `b` = host.
    TipInside,
    /// Two members' drawn solids overlap. `a` < `b`.
    Collision,
    /// A straight tip that still grazes the model. `a` = member.
    GrazeKept,
    /// A head downgraded after sizing. `a` = member.
    HeadDowngraded,
    /// A head kept short of its clearance. `a` = member.
    HeadClearance,
    /// A link under the bending law. `a` = member.
    LinkOverstress,
    /// A certified head whose drawn chords exceed capacity. `a` = member.
    JointOverCapacity,
    /// A certified head with post slots under their own need. `a` = member.
    JointUnderNeed,
    /// A head that is not certified (old law, or never sized). `a` = member,
    /// `b` = 0 never sized, 1 shared, 2 no seat, 3 infeasible.
    JointUncertified,
    /// A landing defect. `a` = member, `b` = [`LandingDefect`] as a number.
    Landing,
    /// A model foot short of its load's need. `a` = member.
    FootShort,
    /// A seat that is a true knife edge. `a` = member.
    KnifeEdge,
    /// A footprint move refused. `a` = member.
    MoveRefused,
    /// A disc off its face after the final normal. `a` = member.
    OffFace,
    /// A seat trim that bites the shaft. `a` = member.
    LipBite,
    /// A join in a seat zone. `a` = lander, `b` = trunk.
    HeadJoinZone,
    /// A stranded-seat member lapping a root on the finished field. `a` = member.
    Stranded,
    /// An arm over the area law at layer `b`. `a` = member.
    Overload,
    /// An arm merged into a trunk at the radius ceiling. `a` = child, `b` = parent.
    OvergrownMerge,
    /// An old tip that lost the ground its placed contact held. `a` = tip.
    GroundLost,
    /// A printing top that shakes over a pixel. `a` = member.
    ShakenTop,
}

impl Class {
    /// Whether `b` names a member (`a` always does).
    fn second_is_member(self) -> bool {
        matches!(
            self,
            Self::TipInside | Self::Collision | Self::HeadJoinZone | Self::OvergrownMerge
        )
    }
}

/// The identity of one warning: class and members, nothing about its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Key {
    pub class: Class,
    pub a: u32,
    pub b: u32,
}

impl Key {
    fn of(class: Class, a: u32) -> Self {
        Self { class, a, b: 0 }
    }

    fn pair(class: Class, a: u32, b: u32) -> Self {
        Self { class, a, b }
    }

    /// The member ids this warning names.
    pub(crate) fn members(&self) -> Vec<u32> {
        if self.class.second_is_member() {
            vec![self.a, self.b]
        } else {
            vec![self.a]
        }
    }
}

/// One warning and its size. Where a warning has no size, `severity` is 0.
/// Larger is worse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Warning {
    pub key: Key,
    pub severity: f64,
}

/// The tolerance on "worse": a severity must rise by more than this, relative
/// to the larger of itself and 1, to count. Float noise, not physics.
const WORSE_BY: f64 = 1.0e-9;

fn worse(first: f64, now: f64) -> bool {
    now > first + WORSE_BY * first.abs().max(1.0)
}

/// Sort by key and merge equal keys by their worst severity: the canonical list.
fn canonical(mut warnings: Vec<Warning>) -> Vec<Warning> {
    warnings.sort_by(|x, y| x.key.cmp(&y.key).then(x.severity.total_cmp(&y.severity)));
    let mut out: Vec<Warning> = Vec::with_capacity(warnings.len());
    for warning in warnings {
        match out.last_mut() {
            Some(last) if last.key == warning.key => last.severity = last.severity.max(warning.severity),
            _ => out.push(warning),
        }
    }
    out
}

// --------------------------------------------------------------- globals

/// The field-wide quantities a regression on which discards the edit outright
/// (`SC-DESIGN.md` §4, "Global"). Counts may not rise; loads may not rise
/// beyond float noise.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Globals {
    pub uncarried_load_mm2: f64,
    pub misrouted_load_mm2: f64,
    /// `injected - plate reaction - uncarried - absorbed`, absolute.
    pub unexplained_gap_mm2: f64,
    pub conservation_residual: f64,
    pub model_section_overload: usize,
    pub model_section_worst_x: f64,
    pub residual_demand_cells: usize,
    pub route_not_converged: u64,
    pub sweeps_disagree: bool,
    pub unrooted_landings: usize,
    pub insertion_rounds_exhausted: u64,
    pub insertion_geometry_exhausted: u64,
    pub drain_seat_refusals: usize,
    pub merges_kept_carrying: usize,
    pub still_congested: bool,
    pub mast_roots_overlapping: usize,
    pub braces_thicker_than_a_post: usize,
    // The FINAL landing check, by class.
    pub landings_invalid: usize,
    pub landings_unrooted: usize,
    pub brace_tops_detached: usize,
    pub heads_not_intact: usize,
    pub joins_above_joint: usize,
    // The FINAL shake and escape.
    pub shake_stages_over_pixel: usize,
    pub shake_unsolved_stages: usize,
    pub shake_worst_px: f64,
    pub escape_blocked_drains: usize,
}

/// One global quantity that rose.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GlobalRegression {
    pub field: &'static str,
    pub first: f64,
    pub edited: f64,
}

impl Globals {
    #[expect(
        clippy::cast_precision_loss,
        reason = "counts of a field's members, far below 2^52"
    )]
    fn rows(&self) -> Vec<(&'static str, f64, bool)> {
        let flag = |value: bool| f64::from(u8::from(value));
        vec![
            ("uncarried_load_mm2", self.uncarried_load_mm2, true),
            ("misrouted_load_mm2", self.misrouted_load_mm2, true),
            ("unexplained_gap_mm2", self.unexplained_gap_mm2, true),
            ("conservation_residual", self.conservation_residual, true),
            ("model_section_overload", self.model_section_overload as f64, false),
            ("model_section_worst_x", self.model_section_worst_x, true),
            ("residual_demand_cells", self.residual_demand_cells as f64, false),
            ("route_not_converged", self.route_not_converged as f64, false),
            ("sweeps_disagree", flag(self.sweeps_disagree), false),
            ("unrooted_landings", self.unrooted_landings as f64, false),
            ("insertion_rounds_exhausted", self.insertion_rounds_exhausted as f64, false),
            ("insertion_geometry_exhausted", self.insertion_geometry_exhausted as f64, false),
            ("drain_seat_refusals", self.drain_seat_refusals as f64, false),
            ("merges_kept_carrying", self.merges_kept_carrying as f64, false),
            ("still_congested", flag(self.still_congested), false),
            ("mast_roots_overlapping", self.mast_roots_overlapping as f64, false),
            ("braces_thicker_than_a_post", self.braces_thicker_than_a_post as f64, false),
            ("landings_invalid", self.landings_invalid as f64, false),
            ("landings_unrooted", self.landings_unrooted as f64, false),
            ("brace_tops_detached", self.brace_tops_detached as f64, false),
            ("heads_not_intact", self.heads_not_intact as f64, false),
            ("joins_above_joint", self.joins_above_joint as f64, false),
            ("shake_stages_over_pixel", self.shake_stages_over_pixel as f64, false),
            ("shake_unsolved_stages", self.shake_unsolved_stages as f64, false),
            ("shake_worst_px", self.shake_worst_px, true),
            ("escape_blocked_drains", self.escape_blocked_drains as f64, false),
        ]
    }
}

// -------------------------------------------------------- final predicates

/// What the final predicates read that the finished field does not carry: the
/// laws and the layer height, and, to run the escape flood, the slices and the
/// drains.
pub(crate) struct FinalEnv<'a> {
    pub grid: &'a Grid,
    pub plate_z_mm: f64,
    pub layer_height_mm: f64,
    pub law: SlendernessLaw,
    pub taper: LoadStepTaper,
    /// The brace peel pressure of the shake check, or `None` to skip it.
    pub shake_pressure_kpa: Option<f64>,
    pub escape: Option<EscapeInputs<'a>>,
}

/// What the escape flood reads.
pub(crate) struct EscapeInputs<'a> {
    pub slices: &'a [Vec<plinth_support::perception::SliceSolid>],
    pub drains: &'a [DrainSpec],
    pub voxel_pitch_mm: f64,
    pub tip_radius_max_mm: f64,
}

impl<'a> FinalEnv<'a> {
    /// The environment of a finished plan under the policy it was planned with:
    /// the slenderness law and the shake pressure built exactly as the sweep
    /// builds them (`run_sweep_inner`).
    pub(crate) fn of_plan(
        plan: &'a SweepPlan,
        policy: &SweepPolicy,
        escape: Option<EscapeInputs<'a>>,
    ) -> Self {
        let law = if policy.slenderness {
            SlendernessLaw::new(
                policy.slenderness_ratio,
                policy.tip_radius_min_mm,
                policy.tip_radius_max_mm,
                policy.overhang_centre_deg - 0.5 * policy.overhang_transition_deg,
            )
            .with_foot_sizing(policy.x_allowed)
            .with_owner_foot_rule()
        } else {
            SlendernessLaw::disabled()
        };
        Self {
            grid: &plan.grid,
            plate_z_mm: plan.plate_z_mm,
            layer_height_mm: plan.layer_height_mm,
            law,
            taper: plan.load_step_taper,
            shake_pressure_kpa: Some(policy.stage_shake_brace_peel_kpa),
            escape,
        }
    }
}

/// One unbraced run over its limit on the finished field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FinalRun {
    pub member: u32,
    pub length_mm: f64,
    pub limit_mm: f64,
}

/// **The geometry-dependent checks, recomputed on the finished field after its
/// last mutation** (astra round 2 finding 3). Not the history counters the run
/// report holds: those were taken before the footprint slides, the final head
/// resize and the feet, and `publish_runs` keeps the radius-dependent limit it
/// measured with. Read-only.
#[derive(Clone, Debug)]
pub(crate) struct FinalChecks {
    pub landings: LandingCheck,
    /// The runs over their limit, on the radii the file draws, lengths scaled to
    /// the drawn chord, worst per member first by id.
    pub unbraced: Vec<FinalRun>,
    pub braces_thicker_than_a_post: usize,
    pub shake: Option<StageShake>,
    pub escape: Option<EscapeReport>,
}

/// Recompute the final predicates on `struts`. `O(struts)` for the landings, one
/// pass for the runs and the brace census; the shake and the escape flood are the
/// two expensive ones (the shake is the sweep's own diagnostic; the flood is
/// voxel-scale), and run only when the environment asks for them.
pub(crate) fn final_checks(env: &FinalEnv<'_>, struts: &[SweepStrut]) -> FinalChecks {
    let landings = landing_check::check_landings(struts, env.layer_height_mm);
    let runs = crate::slenderness::unbraced_runs_drawn(
        env.grid,
        struts,
        env.layer_height_mm,
        &env.law,
        env.taper,
    );
    let scale = crate::footprint::chord_scales(env.grid, env.plate_z_mm, env.layer_height_mm, struts);
    let mut unbraced: Vec<FinalRun> = runs
        .iter()
        .filter_map(|run| {
            let factor = scale.get(run.strut_id as usize).copied().unwrap_or(1.0);
            let length_mm = run.length_mm * factor;
            (length_mm > run.limit_mm).then_some(FinalRun {
                member: run.strut_id,
                length_mm,
                limit_mm: run.limit_mm,
            })
        })
        .collect();
    unbraced.sort_by(|x, y| x.member.cmp(&y.member).then(x.length_mm.total_cmp(&y.length_mm)));
    let profiles = crate::adapter::drawn_profiles(struts, env.taper).0;
    let braces_thicker_than_a_post = crate::slenderness::braces_thicker_than_a_post(struts, &profiles);
    let shake = env.shake_pressure_kpa.map(|pressure| {
        crate::stage_shake::check(
            env.grid,
            struts,
            &profiles,
            env.layer_height_mm,
            &env.law,
            pressure,
        )
    });
    let escape = env.escape.as_ref().map(|inputs| {
        crate::drain::check_escape(
            env.grid,
            inputs.slices,
            struts,
            inputs.drains,
            env.plate_z_mm,
            env.layer_height_mm,
            inputs.voxel_pitch_mm,
            inputs.tip_radius_max_mm,
        )
    });
    FinalChecks {
        landings,
        unbraced,
        braces_thicker_than_a_post,
        shake,
        escape,
    }
}

// ------------------------------------------------------- overgrown merges

/// An arm merged into a trunk the area law cannot grow further.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OvergrownMerge {
    pub child: u32,
    pub parent: u32,
    /// The parent's drawn-ledger radius at the landing layer, mm.
    pub parent_radius_mm: f64,
}

/// **The capacity predicate for overgrown merges** (G9; astra round 2 finding
/// 2): every ARM that lands on a member whose radius at the landing layer is at
/// the registered maximum tip radius, read-only, sorted by `(child, parent)`.
///
/// This is `unmerge_overgrown`'s radius test and nothing else. That pass is a
/// REPAIR and filters by what it can put back: it skips `inserted_by_load` arms
/// (their descent is grown by the down-sweep, with no recorded column to return
/// to) and arms without a `straight_fallback` (an inserted arm has `None`), so
/// it cannot say whether an inserted arm overloads its trunk. This predicate
/// answers that capacity question, separately; the repair's filters stay where
/// they are.
#[must_use]
pub(crate) fn overgrown_merges(struts: &[SweepStrut], tip_radius_max_mm: f64) -> Vec<OvergrownMerge> {
    let mut found: Vec<OvergrownMerge> = struts
        .iter()
        .filter(|strut| crate::slenderness::live(strut) && strut.kind == crate::StrutKind::Arm)
        .filter_map(|strut| {
            let Landing::Strut { id, layer } = strut.landing else {
                return None;
            };
            let parent = struts.get(id as usize)?;
            if parent.k_bottom == usize::MAX || layer < parent.k_bottom || layer > parent.k_top {
                return None;
            }
            let radius = *parent.radius_mm.get(layer - parent.k_bottom)?;
            (radius >= tip_radius_max_mm).then_some(OvergrownMerge {
                child: strut.id,
                parent: parent.id,
                parent_radius_mm: radius,
            })
        })
        .collect();
    found.sort_by_key(|merge| (merge.child, merge.parent));
    found
}

// ----------------------------------------------------------------- census

/// Everything measured on a finished field beyond the run report.
#[derive(Clone, Debug)]
pub(crate) struct Census {
    pub collisions: Vec<MemberCollision>,
    pub overgrown: Vec<OvergrownMerge>,
    pub joint_rows: Vec<JointCertRow>,
    pub finals: FinalChecks,
    /// Old tips that lost the ground their placed contact held: measured on the
    /// EDITED field against the first plan ([`ground_lost`]); empty for the
    /// first plan itself.
    pub ground_lost: Vec<u32>,
}

impl Census {
    /// Measure the census of a finished plan.
    pub(crate) fn measure(
        plan: &SweepPlan,
        layer_plan: &plinth_core::LayerPlan,
        env: &FinalEnv<'_>,
        tip_radius_max_mm: f64,
    ) -> Self {
        let arms = crate::adapter::arms_of(plan);
        Self {
            collisions: crate::member_clearance::member_collisions(
                &plan.struts,
                &arms,
                layer_plan,
                plan.plate_z_mm,
                plan.grid.pitch_mm(),
            ),
            overgrown: overgrown_merges(&plan.struts, tip_radius_max_mm),
            joint_rows: crate::heads::joint_certificate_rows(&plan.struts),
            finals: final_checks(env, &plan.struts),
            ground_lost: Vec::new(),
        }
    }
}

/// **The old tips that lost the ground their placed contact held**
/// (`SC-DESIGN.md` §4): for every tip whose UNGROWN contact ball changed between
/// the finished first plan and the edit, the cells the first ball held must
/// still join ground ([`crate::footprint::contacts_keep_ground`], the footprint
/// rule's own test, in one batch). Returns the lost tips (old ids, sorted), and,
/// for each, the new arms (ids from `n_c`) whose ball stands within the window of
/// the lost tip: the structural link a ground loss is blamed on.
#[must_use]
pub(crate) fn ground_lost(
    grid: &Grid,
    slices: &[Vec<plinth_support::perception::SliceSolid>],
    plate_z_mm: f64,
    height_mm: f64,
    n_c: u32,
    old: &[(u32, crate::footprint::ContactBall)],
    edited: &[(u32, crate::footprint::ContactBall)],
) -> Vec<(u32, Vec<u32>)> {
    let edited_of: BTreeMap<u32, crate::footprint::ContactBall> = edited.iter().copied().collect();
    let pairs: Vec<(u32, crate::footprint::ContactBall, crate::footprint::ContactBall)> = old
        .iter()
        .filter_map(|&(id, before)| edited_of.get(&id).map(|&after| (id, before, after)))
        .collect();
    let keep = crate::footprint::contacts_keep_ground(
        grid,
        slices,
        plate_z_mm,
        height_mm,
        &pairs.iter().map(|&(_, before, after)| (before, after)).collect::<Vec<_>>(),
    );
    let pitch = grid.pitch_mm();
    let mut lost: Vec<(u32, Vec<u32>)> = pairs
        .iter()
        .zip(&keep)
        .filter(|(_, kept)| !**kept)
        .map(|(&(id, before, after), _)| {
            let travel = (after.centre_mm[0] - before.centre_mm[0])
                .hypot(after.centre_mm[1] - before.centre_mm[1]);
            let reach = before.radius_mm + after.radius_mm + travel + 4.0 * pitch;
            let mut near: Vec<u32> = edited
                .iter()
                .filter(|(other, ball)| {
                    *other >= n_c
                        && (ball.centre_mm[0] - before.centre_mm[0])
                            .hypot(ball.centre_mm[1] - before.centre_mm[1])
                            <= reach + ball.radius_mm
                })
                .map(|(other, _)| *other)
                .collect();
            near.sort_unstable();
            (id, near)
        })
        .collect();
    lost.sort_by_key(|(id, _)| *id);
    lost
}

// -------------------------------------------------------------- finished

/// One finished field, as the comparison reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Finished {
    pub warnings: Vec<Warning>,
    pub globals: Globals,
}

impl Finished {
    /// Class the report and the census of a finished field.
    pub(crate) fn of(report: &SweepRunReport, struts: &[SweepStrut], census: &Census) -> Self {
        let mut warnings: Vec<Warning> = Vec::new();
        let mut globals = Globals::default();
        classify_report(report, struts, &mut warnings, &mut globals);
        classify_census(report, census, &mut warnings, &mut globals);
        Self::from_parts(warnings, globals)
    }

    /// A finished field from its parts, canonicalised.
    pub(crate) fn from_parts(warnings: Vec<Warning>, globals: Globals) -> Self {
        Self {
            warnings: canonical(warnings),
            globals,
        }
    }
}

fn id_of(struts: &[SweepStrut], index: usize) -> u32 {
    struts.get(index).map_or_else(|| u32::try_from(index).unwrap_or(u32::MAX), |strut| strut.id)
}

/// Class every field of the run report. **Every struct here is destructured
/// without `..`**: a field added to the report, or to a nested outcome that can
/// carry a warning, fails to compile until it is classed below as a per-member
/// warning, a global, a replaced-by-final-predicate history counter, or neutral.
///
/// *Replaced*: the landing check, the unbraced-run warnings, the brace census,
/// the shake and the escape verdicts are history counters taken before the
/// finishing stage's last mutation; [`classify_census`] reads their final
/// predicates instead.
#[rustfmt::skip]
#[expect(
    clippy::too_many_lines,
    reason = "one exhaustive destructure per report struct is the coverage guard"
)]
fn classify_report(
    report: &SweepRunReport,
    struts: &[SweepStrut],
    warnings: &mut Vec<Warning>,
    globals: &mut Globals,
) {
    let SweepRunReport {
        achieved_pitch_mm: _, coarsening_steps: _, layers: _, cells_per_layer: _, reach: _,
        x_allowed: _, tip_radius_max_mm: _, will_pass_audit_span_check: _,
        max_tau_at_binding_reach_mm: _, u_from_vertical_max: _, tau_vertical_over_tau_used_max: _,
        chamfer_not_converged: _, struts_total: _, landings: _, seat_spacing: _, tip_radius: _,
        slenderness_p50: _, slenderness_max: _, injected_load_mm2: _, peak_layer_area_mm2: _,
        model_section_max_x: _, total_scar_mm2: _, plate_reaction_mm2: _, demand_area_mm2: _,
        residual_demand_cells, residual_demand_mm2: _, uncarried_load_mm2,
        uncarried_lowest_layer: _, uncarried_worst_gap_mm: _, sweeps_disagree, route_not_converged,
        arms_inserted_by_load: _, insertion_rounds_exhausted, insertion_geometry_exhausted,
        insertion_rounds: _, unrooted_landings, struts_at_minimum_radius: _, overloaded_struts,
        model_section_overload, conservation_residual, peel_field_mode: _, peel_field_stats: _,
        sweep_digest: _, removal_coverage: _, drain_keep_out_cells: _, drain_keep_out_refusals: _,
        drain_seat_refusals,
        // Replaced by the final escape flood.
        escape: _,
        arms_leaned: _, arms_merged: _, lean_paths_blocked: _, merges_refused_by_radius: _,
        merges_kept_carrying, merges_moved_with_trunk: _, unmerge_stale_landings: _,
        landing_check, lean_max_step_mm: _, junctions_at_parent_top: _, junctions_mid_shaft: _,
        descent_junctions_mid_shaft: _, shaft_spans_recorded: _, slenderness_lane,
        congestion_lane, stranded_branch, seat_footprint, seat_footprint_normal, seat_contact,
        // The edit's own outcome line; it is the transaction's, not a field's warning.
        seat_second_contact: _,
        model_feet, head_join,
        // Per-event sizing's conservation residual is the same quantity as
        // `conservation_residual` on this field's own ledger; its path counters are neutral.
        per_event: _,
        // The rigid-body reading never sizes and is diagnostic only (its own docs).
        rigid_share: _,
        lip_seat,
        // The re-route lane's counters; its census runs through `member_model_clearance`.
        clearance_reroute: _,
        member_model_clearance, model_surface_built: _, hidden_cavity: _, support_volume_mm3: _,
        plate_contact_area_mm2: _, tip_column_roots_offered: _, misrouted_load_mm2,
        tip_column_root_load_mm2: _, contact_merge_load_moved_mm2: _, unrooted_routing: _,
        counters: _, heads,
    } = report;

    globals.uncarried_load_mm2 = *uncarried_load_mm2;
    globals.misrouted_load_mm2 = *misrouted_load_mm2;
    globals.conservation_residual = conservation_residual.abs();
    globals.residual_demand_cells = *residual_demand_cells;
    globals.route_not_converged = *route_not_converged;
    globals.sweeps_disagree = *sweeps_disagree;
    globals.unrooted_landings = *unrooted_landings;
    globals.insertion_rounds_exhausted = *insertion_rounds_exhausted;
    globals.insertion_geometry_exhausted = *insertion_geometry_exhausted;
    globals.drain_seat_refusals = *drain_seat_refusals;
    globals.merges_kept_carrying = *merges_kept_carrying;
    globals.model_section_overload = model_section_overload.len();
    globals.model_section_worst_x = model_section_overload
        .iter()
        .map(|overload| overload.realized_x)
        .fold(0.0, f64::max);
    for overloaded in overloaded_struts {
        warnings.push(Warning {
            key: Key::pair(Class::Overload, overloaded.strut_id, u32::try_from(overloaded.layer).unwrap_or(u32::MAX)),
            severity: overloaded.realized_x,
        });
    }

    let crate::landing_check::LandingLedger {
        passes: _,
        finished: finished_landings,
        load_gap_mm2,
        absorbed_mm2,
        absorbed_reads: _,
    } = landing_check;
    globals.unexplained_gap_mm2 = (load_gap_mm2 - absorbed_mm2).abs();
    let LandingCheck {
        strut_landings: _, parent_absent: _, parent_elsewhere: _, ground_short: _,
        brace_tops_detached: _, unrooted: _, heads_not_intact: _, joins_above_joint: _,
        model_feet: _,
        // The only landing field the final recomputation cannot produce: the feet
        // are measured by `model_feet::measure` after the last mutation.
        model_feet_partial,
    } = finished_landings;
    for &(id, tenths) in model_feet_partial {
        warnings.push(Warning {
            key: Key::pair(Class::Landing, id, LandingDefect::FootPartial as u32),
            severity: f64::from(1000_u32.saturating_sub(tenths)) / 10.0,
        });
    }

    let crate::slenderness::SlendernessOutcome {
        ratio: _, thin_brace_reach_mm: _, thick_brace_reach_mm: _, arms_over_limit_before: _,
        runs_over_limit_before: _, merges_forced: _, braces_added: _, masts_dropped: _,
        // Replaced by the final unbraced runs.
        warnings: _,
        refusal_notes: _, last_moved_by: _, longest_run_mm: _, longest_run_limit_mm: _,
        brace_bands_refused: _, mast_cells_refused: _, mast_roots_overlapping_after_sizing,
        merges_undone_by_radius: _, merges_put_back_ids: _, descent_merges_put_back: _,
        crosses_first_plane: _, crosses_further_plane: _, crosses_to_a_later_partner: _,
        cross_bands_planned: _, crosses_refused_overlapping: _, crosses_pruned: _,
        members_thickened: _, thicken_refused_radius: _, thicken_refused_disc: _,
        thicken_refused_no_plan: _, thicken_refused_model: _, foot_runs_read_through_neck: _,
        thicken_refused_neck: _, foot_enlarge_tried: _, feet_enlarged: _, foot_enlarge_refused: _,
        thicken_bay_local: _, carry_down_kept_frame: _, posts_raised_at_a_joint: _,
        thicken_slots_flagged: _, thicken_slots_measured_clear: _, thicken_slots_inside_already: _,
        thicken_slots_undecided: _, thicken_members_hidden: _, thicken_over_existing_lap: _,
        thicken_admitted_where_the_crowding_disc_laps: _, cross_roots_grown: _,
        cross_roots_lapping: _, trunks_raised_under_thickening: _, mast_cells_refused_by_panel: _,
        masts_extended: _, mast_ties_planned_above_the_first_tie: _,
        mast_ties_raised_for_the_piece_above: _, brace_bands_refused_by_model: _,
        brace_bands_refused_by_member: _, brace_bands_moved_above_flare: _,
        brace_bands_dropped_in_flare: _, masts_raised_above_flare: _,
        mast_cells_refused_by_model: _, mast_cells_refused_by_member: _,
        member_refusals_beyond_the_closest_axes: _, member_admissions_the_closest_axes_refused: _,
        mast_cells_refused_thicker_than_member: _, masts_moved_to_a_thinner_cell: _,
        crosses_at_an_existing_tie_layer: _, tie_passes: _, fixed_point: _,
        retie_passes_after_reroute: _, retie_remedies_after_reroute: _, merges_refused_carrying: _,
        headed_kept: _, merges_refused_stale_shaft: _, crosses_kept_in_their_own_plane: _,
        landing_ties_held_late: _, brace_band_refusals: _, brace_bands_deepened_for_their_diagonals: _,
        brace_bands_moved_up_off_the_plate: _, crosses_refused_thicken_cheaper: _,
        thickened_where_cheaper_than_bracing: _, crosses_shared_by_both_posts: _,
        window_lowest_braced_ratio: _, window_highest_thickened_ratio: _,
        crosses_refused_past_pair_ceiling: _, members_thickened_into_the_window: _,
        // Replaced by the final brace census.
        braces_thicker_than_a_post: _,
        frame_band_sets: _, frame_bands_built: _, frame_bands_beyond_fixed_point: _,
        frame_most_bands_in_a_set: _, frame_highest_ratio_braced: _, frame_pairs_past_ceiling: _,
        frame_sets_refused_incomplete: _, frame_pairs_without_room: _,
        frame_bands_moved_from_their_layer: _, frame_sets_respaced: _, frame_sets_pruned: _,
        // Replaced by the final shake.
        stage_shake: _,
        model_tie_runs_tried: _, model_tie_sets: _, model_ties_built: _, model_tie_ids: _,
        model_ties_thickened_for_the_frame: _, model_tie_sets_beyond_fixed_point: _,
        model_tie_worst_sway_px: _, model_tie_refused_no_surface: _,
        model_tie_refused_no_candidate: _, model_tie_refused_frame: _, model_tie_candidates: _,
        model_tie_refused_off_shaft: _, model_tie_refused_thicker_than_post: _,
        model_tie_refused_path_drain: _, model_tie_refused_path_model: _,
        model_tie_refused_path_member: _, model_tie_refused_solid_model: _,
        model_tie_candidates_hidden: _, model_tie_refused_trim_bite: _,
        model_tie_tips_sized_for_the_trim: _, model_ties_moved_from_their_layer: _,
        model_tie_refused_disc_off_face: _, model_tie_refused_solid_member: _,
    } = slenderness_lane;
    globals.mast_roots_overlapping = *mast_roots_overlapping_after_sizing;

    let crate::congestion::CongestionOutcome {
        enabled: _, before: _, at_lane_exit: _, after: _, rounds_used: _, arms_leaned: _,
        arms_inserted: _, shelf_layer: _, shelf_cover_cells: _,
        section_moved_outside_silhouette_mm2: _, flank_load_still_at_plate_mm2: _,
        flank_arms_moved_off_plate: _, flank_roots_overlapping_after_sizing: _,
        flank_radius_mean_placed_mm: _, flank_radius_area_law_mm: _, flank_radius_used_mm: _,
        flank_stacked_area_mm2: _, flank_arms_wanted: _, flank_rounds_not_relieving: _,
        outward_cells_refused: _, headed_kept: _, flank_seats_refused: _, inserted_ids: _,
        flank_relief_evaluated: _, flank_relief_with: _, flank_relief_without: _,
        flank_arms_withdrawn: _, still_congested,
    } = congestion_lane;
    globals.still_congested = *still_congested;

    let crate::stranded_branch::StrandedBranchOutcome {
        enabled: _, stranded_members: _, lapping_before: _, lapping_ids_before: _,
        branches_made: _, branches_kept: _, branches: _, put_back_overloaded: _,
        refused_carrying: _, headed_kept: _, resting_moved_with_branch: _, refused_no_landing: _,
        landings_priced: _, paths_blocked: _, paths_blocked_drain: _, paths_blocked_model: _,
        paths_blocked_member: _, refused_raft: _, rounds: _, decisions: _, lapping_after: _,
        lapping_ids_after: _, lapping_by_lane: _, lapping_on_finished_field: _,
        lapping_ids_on_finished_field,
    } = stranded_branch;
    for &id in lapping_ids_on_finished_field {
        warnings.push(Warning { key: Key::of(Class::Stranded, id), severity: 0.0 });
    }

    let crate::footprint::FootprintOutcome {
        tips_graded: _, tips_on_face: _, tips_moved_onto_face: _, max_move_mm: _,
        members_following_a_moved_host: _, max_follow_mm: _, knife_edges, moves_refused,
        headed_kept: _, moves_refused_lean: _, moves_refused_slenderness: _,
        moves_refused_drain: _, members_checked: _, moves_refused_off_seat: _,
        moves_refused_off_ground: _, tips_graded_for_section: _, tips_moved_for_section: _,
        moves_refused_section: _, moved_for_section: _, section_moves_taken_back: _,
        moves_taken_back_limits: _, settle_rechecks: _, disc_samples: _, search_samples: _,
        verify_samples: _, tips_off_face_at_rim_only: _, layers_rasterised: _, rounds: _,
    } = seat_footprint;
    for &(index, disc_on_face) in knife_edges {
        warnings.push(Warning {
            key: Key::of(Class::KnifeEdge, id_of(struts, index)),
            severity: 1.0 - disc_on_face,
        });
    }
    for &(index, disc_on_face) in moves_refused {
        warnings.push(Warning {
            key: Key::of(Class::MoveRefused, id_of(struts, index)),
            severity: 1.0 - disc_on_face,
        });
    }

    let crate::footprint_normal::FootprintNormalOutcome {
        enabled: _, tips: _, pairs: _, pairs_from_surface: _, fallback_no_ball: _,
        fallback_no_footprint: _, moved: _, moved_over_1_deg: _, moved_over_5_deg: _,
        moved_over_15_deg: _, max_deg: _, mean_deg: _, fixed_point_residual_max_deg: _,
        max_faces_in_footprint: _, slid_remeasured: _, slid_max_change_deg: _, final_changed: _,
        final_max_change_deg: _, final_regraded: _, final_off_face,
    } = seat_footprint_normal;
    for &(index, value) in final_off_face {
        warnings.push(Warning { key: Key::of(Class::OffFace, id_of(struts, index)), severity: value });
    }

    let crate::contact_growth::ContactOutcome {
        enabled: _, tips_measured: _, planned_mm2: _, profile_disc_mm2: _, real_ungrown_mm2: _,
        real_drawn_mm2: _, short_ungrown: _, grown: _, max_growth_ratio: _, rounds: _,
        limit_ball_radius_mm: _, carried_bound: _, stopped_by_overhang: _, ratio_bound: _,
        feature_bound: _, ungrown_over_feature: _, ungrown_over_ratio: _, over_feature, short,
        per_strut: _,
    } = seat_contact;
    for tip in short {
        warnings.push(Warning {
            key: Key::of(Class::ContactShort, id_of(struts, tip.strut)),
            severity: (tip.planned_mm2 - tip.real_mm2).max(0.0),
        });
    }
    for tip in over_feature {
        warnings.push(Warning {
            key: Key::of(Class::ContactOverFeature, id_of(struts, tip.strut)),
            severity: (tip.disc_width_mm - tip.chord_mm).max(0.0),
        });
    }

    let crate::model_feet::ModelFeetOutcome {
        enabled: _, feet: _, faces_measured: _, contacts_measured: _, need_mm2: _,
        planned_mm2: _, real_mm2: _, flat_end_mm2: _,
        // Reported through the landing check's `model_feet_partial`.
        partial: _,
        short_of_need, descent_candidates_refused: _, descent_straight_replaced: _,
        descent_straight_kept: _, relanding_candidates_refused: _,
    } = model_feet;
    for foot in short_of_need {
        warnings.push(Warning {
            key: Key::of(Class::FootShort, foot.id),
            severity: (foot.need_mm2 - foot.real_mm2).max(0.0),
        });
    }

    let crate::head_join::HeadJoinOutcome {
        enabled: _, clearance_mm: _, standoff_mm: _, joins_examined: _,
        joins_in_seat_zone_before: _, joins_in_head_span: _, joins_off_the_model: _,
        candidates_off_the_model: _, model_rows: _, interior_rows: _, sections_near_by_interior: _,
        census_near_by_interior: _, joins_held_by_load_cone: _, census_load_cone_layers: _,
        census_held_by_load_cone: _, landers_moved: _, landers_moved_down_the_chain: _,
        landers_at_hard_limit: _, max_drop_layers: _, min_lander_clearance_mm: _,
        load_moved_mm2: _, dependants_relanded: _, clearance_rows: _, candidates_below_plate: _,
        passes: _, kept_no_clear_join: _, kept_carrying_a_tie: _, kept_not_an_arm: _,
        headed_kept: _, kept_surface_unavailable: _, joins_unmeasured: _, in_seat_zone_after,
    } = head_join;
    for &(lander, trunk) in in_seat_zone_after {
        warnings.push(Warning { key: Key::pair(Class::HeadJoinZone, lander, trunk), severity: 0.0 });
    }

    let crate::lip_seat::LipSeatOutcome {
        enabled: _, arms_graded: _, arms_over_limit_before: _, max_bite_before_mm: _,
        max_approach_before_deg: _, clearance_violators_first: _, clearance_violators_last: _,
        kept_locked_by_tie: _, kept_locked_by_non_arm_below: _,
        kept_behind_a_follower_also_sent: _, kept_for_a_frame: _, arms_relanded: _,
        relanded_on_model: _, relanded_ids: _, children_moved_with_host: _, candidates_priced: _,
        candidates_refused_foot: _, foot_misfits_first: _, foot_misfits_last: _,
        candidates_refused_path: _, candidates_refused_plate: _, candidates_refused_measured: _,
        candidates_refused_clearance: _, candidates_refused_slender: _, kept_for_slenderness: _,
        kept_for_slenderness_ids: _, unheld_pending: _, unheld: _, slender_taken_kept_not_clear: _,
        candidates_hidden: _, candidates_refused_roots: _, clearance_unmeasured: _,
        candidates_verified: _, moves_put_back_host_load: _, moves_put_back_roots: _,
        inserted_arms_withdrawn: _, inserted_on_a_put_back_move: _, blame_spared_already_carried: _,
        blame_held_behind_longer: _, sizing_checks: _, new_overloads_unattributed: _,
        new_root_pairs_unattributed: _, moves_refused_across_passes: _, reruns: _,
        runs_withdrawn: _, kept_carrying: _, kept_no_candidate: _, passes: _, arms_graded_after: _,
        over_limit_after, headed_kept: _, headed_bites: _,
    } = lip_seat;
    for bite in over_limit_after {
        warnings.push(Warning { key: Key::of(Class::LipBite, bite.arm), severity: bite.bite_mm });
    }

    let crate::report::MemberModelClearance {
        ran: _, gap_mm, members_measured: _, members_unmeasured: _, members_hidden: _, rows: _,
        rows_in_own_zones: _, rows_tapered: _, smallest_clearance_mm: _, within_gap, tips_inside,
    } = member_model_clearance;
    for near in within_gap {
        warnings.push(Warning {
            key: Key::of(Class::ModelClearance, near.member),
            severity: (gap_mm - near.clearance_mm).max(0.0),
        });
    }
    for inside in tips_inside {
        warnings.push(Warning {
            key: Key::pair(Class::TipInside, inside.tip_member, inside.host_member),
            severity: inside.depth_mm,
        });
    }

    let crate::heads::HeadsOutcome {
        built: _, refusals: _, skips: _, skipped_seats: _, placement_residual_deg_max: _,
        placement_residual_deg_median: _, final_normal_residual_deg_max: _,
        final_normal_residual_deg_median: _, joints_grown: _, max_standoff_deficit_mm: _,
        downgraded_after_sizing, kept_short_of_clearance, resized_after_moves: _,
        graze_converted: _, graze_asked: _, graze_asked_late: _, graze_converted_late: _,
        graze_reasons: _, graze_kept, link_overstress,
        // Read row by row from the finished struts, `heads::joint_certificate_rows`.
        joint_certificate: _,
    } = heads;
    for &id in downgraded_after_sizing {
        warnings.push(Warning { key: Key::of(Class::HeadDowngraded, id), severity: 0.0 });
    }
    for kept in kept_short_of_clearance {
        warnings.push(Warning { key: Key::of(Class::HeadClearance, kept.member), severity: -kept.standoff_mm });
    }
    for kept in graze_kept {
        warnings.push(Warning { key: Key::of(Class::GrazeKept, kept.id), severity: 0.0 });
    }
    for link in link_overstress {
        warnings.push(Warning { key: Key::of(Class::LinkOverstress, link.member), severity: link.stress_ratio });
    }
}

/// Class the census and the final predicates of a finished field.
fn classify_census(
    report: &SweepRunReport,
    census: &Census,
    warnings: &mut Vec<Warning>,
    globals: &mut Globals,
) {
    for collision in &census.collisions {
        warnings.push(Warning {
            key: Key::pair(Class::Collision, collision.low, collision.high),
            severity: collision.depth_mm,
        });
    }
    for merge in &census.overgrown {
        warnings.push(Warning {
            key: Key::pair(Class::OvergrownMerge, merge.child, merge.parent),
            severity: merge.parent_radius_mm,
        });
    }
    for row in &census.joint_rows {
        match row.state {
            JointCertState::Certified { drawn_max, slots_under_need, worst_need_over_drawn } => {
                if drawn_max > 1.0 + 1.0e-9 {
                    warnings.push(Warning { key: Key::of(Class::JointOverCapacity, row.id), severity: drawn_max });
                }
                if slots_under_need > 0 {
                    warnings.push(Warning {
                        key: Key::of(Class::JointUnderNeed, row.id),
                        severity: worst_need_over_drawn.unwrap_or(1.0),
                    });
                }
            }
            JointCertState::OldLaw(reason) => warnings.push(Warning {
                key: Key::pair(
                    Class::JointUncertified,
                    row.id,
                    match reason {
                        crate::heads::OldLawReason::Shared => 1,
                        crate::heads::OldLawReason::NoSeat => 2,
                        crate::heads::OldLawReason::Infeasible => 3,
                    },
                ),
                severity: 0.0,
            }),
            JointCertState::NotSized => {
                warnings.push(Warning { key: Key::pair(Class::JointUncertified, row.id, 0), severity: 0.0 });
            }
        }
    }
    for &tip in &census.ground_lost {
        warnings.push(Warning { key: Key::of(Class::GroundLost, tip), severity: 0.0 });
    }
    let finals = &census.finals;
    for (defect, id, _) in finals.landings.keyed() {
        warnings.push(Warning {
            key: Key::pair(Class::Landing, id, defect as u32),
            severity: 0.0,
        });
    }
    globals.landings_invalid = finals.landings.invalid_landings();
    globals.landings_unrooted = finals.landings.unrooted.len();
    globals.brace_tops_detached = finals.landings.brace_tops_detached.len();
    globals.heads_not_intact = finals.landings.heads_not_intact.len();
    globals.joins_above_joint = finals.landings.joins_above_joint.len();
    for run in &finals.unbraced {
        warnings.push(Warning {
            key: Key::of(Class::Unbraced, run.member),
            severity: run.length_mm / run.limit_mm.max(f64::MIN_POSITIVE),
        });
    }
    globals.braces_thicker_than_a_post = finals.braces_thicker_than_a_post;
    if let Some(shake) = &finals.shake {
        globals.shake_stages_over_pixel = shake.stages_over_pixel;
        globals.shake_unsolved_stages = shake.unsolved_stages;
        globals.shake_worst_px = shake.worst_px;
        for top in &shake.tops {
            warnings.push(Warning { key: Key::of(Class::ShakenTop, top.member), severity: top.px });
        }
    }
    // The escape verdict on the field: the final flood where it ran, the
    // report's own where it did not.
    let escape = finals.escape.as_ref().unwrap_or(&report.escape);
    globals.escape_blocked_drains = escape.verdicts.iter().filter(|verdict| !verdict.connected).count();
}

// ------------------------------------------------------------ comparison

/// Where each new arm came from, and what the edit's ids are.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Provenance {
    /// The count of members at the checkpoint: ids below it are old (the first
    /// plan's), ids from it are the edit's new arms.
    pub n_c: u32,
    /// Each new arm's trigger host (the member whose layer asked for it).
    pub trigger_host: BTreeMap<u32, u32>,
    /// For each old tip that lost ground, the new arms near it
    /// ([`ground_lost`]).
    pub near_new: BTreeMap<u32, Vec<u32>>,
}

impl Provenance {
    /// The provenance after members are dropped: every key and value mapped
    /// through `remap` (`u32::MAX` = dropped). A new arm that was dropped loses
    /// its row; a trigger host that was dropped leaves `u32::MAX` behind, which
    /// names no member (no blame goes through it).
    #[must_use]
    pub(crate) fn remapped(&self, remap: &[u32]) -> Self {
        let to = |id: u32| remap.get(id as usize).copied().unwrap_or(u32::MAX);
        let kept_before = u32::try_from(
            remap.iter().take(self.n_c as usize).filter(|fresh| **fresh != u32::MAX).count(),
        )
        .unwrap_or(u32::MAX);
        Self {
            n_c: kept_before,
            trigger_host: self
                .trigger_host
                .iter()
                .filter(|(arm, _)| to(**arm) != u32::MAX)
                .map(|(arm, host)| (to(*arm), to(*host)))
                .collect(),
            near_new: self
                .near_new
                .iter()
                .filter(|(tip, _)| to(**tip) != u32::MAX)
                .map(|(tip, arms)| {
                    let mut kept: Vec<u32> = arms.iter().map(|arm| to(*arm)).filter(|arm| *arm != u32::MAX).collect();
                    kept.sort_unstable();
                    (to(*tip), kept)
                })
                .collect(),
        }
    }
}

/// One warning of the edit that is new, or worse than the first plan's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Regression {
    pub key: Key,
    /// The first plan's severity, `None` for a new key.
    pub first: Option<f64>,
    pub edited: f64,
}

/// The result of setting a finished edit against the finished first plan.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Verdict {
    /// Global quantities that rose. Never blamed: any entry discards the edit.
    pub global: Vec<GlobalRegression>,
    /// Warnings that name only NEW arms: those arms are withdrawn.
    pub new_arm_warnings: Vec<Warning>,
    /// Regressions on OLD members, with the new arms a structural link blames.
    pub traced: Vec<(Regression, Vec<u32>)>,
    /// Regressions on old members with no structural link to any new arm:
    /// any entry discards the edit.
    pub untraced: Vec<Regression>,
    /// The new arms to withdraw: the named ones, and the blamed ones.
    pub withdraw: BTreeSet<u32>,
}

impl Verdict {
    /// Nothing regressed and nothing is withdrawn.
    #[must_use]
    pub(crate) fn clean(&self) -> bool {
        self.global.is_empty()
            && self.new_arm_warnings.is_empty()
            && self.traced.is_empty()
            && self.untraced.is_empty()
    }

    /// The edit cannot be repaired by withdrawing arms: a global regression, or
    /// an old member got worse for a reason no new arm explains.
    #[must_use]
    pub(crate) fn discard(&self) -> bool {
        !self.global.is_empty() || !self.untraced.is_empty()
    }
}

/// The structural parents of every member (its landing member and the member its
/// top seats on), by id, from the EDITED field.
fn parents_of(struts: &[SweepStrut]) -> Vec<Vec<u32>> {
    struts
        .iter()
        .map(|strut| {
            let mut parents = Vec::new();
            if let Landing::Strut { id, .. } = strut.landing {
                parents.push(id);
            }
            if let Some(seat) = strut.seat_on_strut {
                parents.push(seat);
            }
            parents
        })
        .collect()
}

/// Whether `target` stands on `arm`'s chain of landings and seats.
fn on_chain(arm: u32, target: u32, parents: &[Vec<u32>]) -> bool {
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut stack = vec![arm];
    while let Some(member) = stack.pop() {
        for &parent in parents.get(member as usize).map_or(&[][..], Vec::as_slice) {
            if parent == target {
                return true;
            }
            if seen.insert(parent) {
                stack.push(parent);
            }
        }
    }
    false
}

/// **Set the finished edit against the finished first plan** (`SC-DESIGN.md`
/// §4). Global quantities first; then every warning key of the edit that is
/// absent from the first plan or whose severity rose.
///
/// * a key naming only new arms: the arm is withdrawn;
/// * a key naming an old member and a new one: blamed on the new one;
/// * a key naming only old members: blamed on a new arm `a` only by a
///   structural link — the member is `a`'s trigger host, stands on `a`'s chain
///   of landings and seats, or (ground lost) has `a`'s ball in its window;
///   with no link it is untraced and the edit is discarded.
///
/// Deterministic: warnings are sorted by key, blame lists by id.
pub(crate) fn compare(
    first: &Finished,
    edited: &Finished,
    edited_struts: &[SweepStrut],
    provenance: &Provenance,
) -> Verdict {
    let mut verdict = Verdict::default();
    for ((name, was, load), (_, now, _)) in first.globals.rows().into_iter().zip(edited.globals.rows()) {
        let rose = if load { worse(was, now) } else { now > was };
        if rose {
            verdict.global.push(GlobalRegression { field: name, first: was, edited: now });
        }
    }
    let before: BTreeMap<Key, f64> = first.warnings.iter().map(|w| (w.key, w.severity)).collect();
    let parents = parents_of(edited_struts);
    let new_arms: Vec<u32> = (provenance.n_c..u32::try_from(edited_struts.len()).unwrap_or(u32::MAX)).collect();
    for warning in &edited.warnings {
        let was = before.get(&warning.key).copied();
        let regressed = match was {
            None => true,
            Some(first_severity) => worse(first_severity, warning.severity),
        };
        if !regressed {
            continue;
        }
        let regression = Regression { key: warning.key, first: was, edited: warning.severity };
        let members = warning.key.members();
        let (new_named, old_named): (Vec<u32>, Vec<u32>) =
            members.iter().partition(|&&member| member >= provenance.n_c);
        if old_named.is_empty() {
            verdict.new_arm_warnings.push(*warning);
            verdict.withdraw.extend(new_named);
            continue;
        }
        let mut blamed: BTreeSet<u32> = new_named.iter().copied().collect();
        if blamed.is_empty() {
            for &old in &old_named {
                for &arm in &new_arms {
                    let linked = provenance.trigger_host.get(&arm) == Some(&old)
                        || on_chain(arm, old, &parents)
                        || provenance.near_new.get(&old).is_some_and(|near| near.contains(&arm));
                    if linked {
                        blamed.insert(arm);
                    }
                }
            }
        }
        if blamed.is_empty() {
            verdict.untraced.push(regression);
        } else {
            verdict.withdraw.extend(blamed.iter().copied());
            verdict.traced.push((regression, blamed.into_iter().collect()));
        }
    }
    verdict
}

// ------------------------------------------------------------- withdrawal

/// The members to withdraw: the seeds and everything that rests on them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Closure {
    pub ids: BTreeSet<u32>,
    /// Old members (ids below the checkpoint) the closure reached. An old member
    /// cannot be withdrawn: any entry discards the edit.
    pub old_reached: Vec<u32>,
}

/// **The dependency closure of a withdrawal**: the seeds, every member landed on
/// or seated on one, transitively. Dropping exactly this set leaves no survivor
/// resting on a dropped member, so `drop_members` never lets a lander fall to the
/// plate (astra round 2 finding 4).
#[must_use]
pub(crate) fn withdrawal_closure(struts: &[SweepStrut], seeds: &BTreeSet<u32>, n_c: u32) -> Closure {
    let mut dependants: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for strut in struts {
        if let Landing::Strut { id, .. } = strut.landing {
            dependants.entry(id).or_default().push(strut.id);
        }
        if let Some(parent) = strut.seat_on_strut {
            dependants.entry(parent).or_default().push(strut.id);
        }
    }
    let mut ids: BTreeSet<u32> = seeds.clone();
    let mut stack: Vec<u32> = seeds.iter().copied().collect();
    while let Some(member) = stack.pop() {
        for &dependant in dependants.get(&member).map_or(&[][..], Vec::as_slice) {
            if ids.insert(dependant) {
                stack.push(dependant);
            }
        }
    }
    let old_reached = ids.iter().copied().filter(|id| *id < n_c).collect();
    Closure { ids, old_reached }
}

/// The old-to-new id map of dropping `withdrawn` from `count` members, exactly
/// as `drop_members` returns it (`u32::MAX` for a dropped member), without
/// touching any field.
#[must_use]
pub(crate) fn remap_for(count: usize, withdrawn: &BTreeSet<u32>) -> Vec<u32> {
    let mut next = 0_u32;
    (0..count)
        .map(|index| {
            if withdrawn.contains(&u32::try_from(index).unwrap_or(u32::MAX)) {
                u32::MAX
            } else {
                let fresh = next;
                next += 1;
                fresh
            }
        })
        .collect()
}

/// The survivors that rest on a withdrawn member: each would fall to the plate
/// in `drop_members`. Empty for any withdrawal that is a closure.
#[must_use]
pub(crate) fn orphaned_by(struts: &[SweepStrut], withdrawn: &BTreeSet<u32>) -> Vec<u32> {
    struts
        .iter()
        .filter(|strut| !withdrawn.contains(&strut.id))
        .filter(|strut| {
            matches!(strut.landing, Landing::Strut { id, .. } if withdrawn.contains(&id))
                || strut.seat_on_strut.is_some_and(|parent| withdrawn.contains(&parent))
        })
        .map(|strut| strut.id)
        .collect()
}

/// A refused-host key list through a remap (`drop_members`'s own map): the keys
/// of dropped members go, the rest are renumbered.
#[must_use]
pub(crate) fn remap_refused(refused: &[(u32, usize)], remap: &[u32]) -> Vec<(u32, usize)> {
    let mut out: Vec<(u32, usize)> = refused
        .iter()
        .filter_map(|&(host, layer)| {
            let fresh = remap.get(host as usize).copied().filter(|fresh| *fresh != u32::MAX)?;
            Some((fresh, layer))
        })
        .collect();
    out.sort_unstable();
    out
}

// ------------------------------------------------------- baseline census

/// What the acceptance contract finds on ONE finished plan, as plain numbers
/// for a log line: how much of the defect the first plan itself already carries
/// (`SC-DESIGN.md` §5: the baseline the edit is held to). Read-only.
#[derive(Clone, Debug, Default)]
pub struct CensusSummary {
    pub members: usize,
    /// Pairs of members whose drawn solids overlap beyond a planned joint.
    pub collisions: usize,
    /// The deepest of them: `(low, high, depth mm, pieces)`, deepest first.
    pub deepest_collisions: Vec<(u32, u32, f64, String)>,
    /// Collisions by the pair of piece kinds.
    pub collisions_by_pieces: BTreeMap<String, usize>,
    /// Arms merged into a trunk at the radius ceiling.
    pub overgrown_merges: usize,
    /// The inserted ones among them.
    pub overgrown_inserted: usize,
    /// Warnings by class, over the whole keyed list.
    pub warnings_by_class: BTreeMap<String, usize>,
    /// Unbraced runs over their limit: the report's history and the final.
    pub unbraced_history: usize,
    pub unbraced_final: usize,
    /// Members whose final unbraced warning is not in the history, and the
    /// reverse: what a late mutation moved.
    pub unbraced_final_only: usize,
    pub unbraced_history_only: usize,
    pub braces_thicker_history: usize,
    pub braces_thicker_final: usize,
    pub landings_invalid_final: usize,
    pub landings_unrooted_final: usize,
    pub landings_invalid_history: usize,
    pub landings_unrooted_history: usize,
    pub shake_over_pixel_history: usize,
    pub shake_over_pixel_final: Option<usize>,
    /// Seconds: the collision census, and all the final predicates.
    pub collision_seconds: f64,
    pub final_seconds: f64,
}

impl std::fmt::Display for CensusSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "members {}", self.members)?;
        writeln!(
            f,
            "G3 collisions {} (by pieces {:?}); overgrown merges {} ({} inserted)",
            self.collisions, self.collisions_by_pieces, self.overgrown_merges, self.overgrown_inserted
        )?;
        for (low, high, depth, pieces) in &self.deepest_collisions {
            writeln!(f, "  collision {low}-{high}: {depth:.4} mm ({pieces})")?;
        }
        writeln!(
            f,
            "unbraced runs over limit: history {} / final {} (final only {}, history only {}); \
             braces thicker than a post: history {} / final {}",
            self.unbraced_history,
            self.unbraced_final,
            self.unbraced_final_only,
            self.unbraced_history_only,
            self.braces_thicker_history,
            self.braces_thicker_final
        )?;
        writeln!(
            f,
            "landings: invalid history {} / final {}; unrooted history {} / final {}; \
             shake stages over a pixel: history {} / final {:?}",
            self.landings_invalid_history,
            self.landings_invalid_final,
            self.landings_unrooted_history,
            self.landings_unrooted_final,
            self.shake_over_pixel_history,
            self.shake_over_pixel_final
        )?;
        writeln!(f, "warnings by class {:?}", self.warnings_by_class)?;
        write!(
            f,
            "seconds: collision census {:.3}, final predicates {:.3}",
            self.collision_seconds, self.final_seconds
        )
    }
}

/// **The baseline census of a finished plan**: the member-collision census, the
/// overgrown-merge capacity predicate, the joint certificate row by row and the
/// final predicates, set beside the report's history counters. Not called by
/// production; a diagnostic for the log and for SC-3's acceptance. The escape
/// flood is not run (it needs the slices and the drains): its history verdict
/// stands. `with_shake` runs the construction-stage shake as a final predicate.
#[must_use]
pub fn baseline_census(
    plan: &SweepPlan,
    layer_plan: &plinth_core::LayerPlan,
    policy: &SweepPolicy,
    with_shake: bool,
) -> CensusSummary {
    let mut env = FinalEnv::of_plan(plan, policy, None);
    if !with_shake {
        env.shake_pressure_kpa = None;
    }
    let arms = crate::adapter::arms_of(plan);
    let started = std::time::Instant::now();
    let collisions = crate::member_clearance::member_collisions(
        &plan.struts,
        &arms,
        layer_plan,
        plan.plate_z_mm,
        plan.grid.pitch_mm(),
    );
    let collision_seconds = started.elapsed().as_secs_f64();
    let started = std::time::Instant::now();
    let finals = final_checks(&env, &plan.struts);
    let final_seconds = started.elapsed().as_secs_f64();
    let census = Census {
        collisions,
        overgrown: overgrown_merges(&plan.struts, policy.tip_radius_max_mm),
        joint_rows: crate::heads::joint_certificate_rows(&plan.struts),
        finals,
        ground_lost: Vec::new(),
    };
    let finished = Finished::of(&plan.report, &plan.struts, &census);
    let history: BTreeSet<u32> = plan
        .report
        .slenderness_lane
        .warnings
        .iter()
        .map(|warning| warning.strut_id)
        .collect();
    let last: BTreeSet<u32> = census.finals.unbraced.iter().map(|run| run.member).collect();
    let mut by_pieces: BTreeMap<String, usize> = BTreeMap::new();
    for collision in &census.collisions {
        *by_pieces
            .entry(format!("{:?}-{:?}", collision.low_piece, collision.high_piece))
            .or_default() += 1;
    }
    let mut deepest: Vec<&MemberCollision> = census.collisions.iter().collect();
    deepest.sort_by(|x, y| y.depth_mm.total_cmp(&x.depth_mm).then((x.low, x.high).cmp(&(y.low, y.high))));
    let mut classes: BTreeMap<String, usize> = BTreeMap::new();
    for warning in &finished.warnings {
        *classes.entry(format!("{:?}", warning.key.class)).or_default() += 1;
    }
    let ledger = &plan.report.landing_check.finished;
    CensusSummary {
        members: plan.struts.len(),
        collisions: census.collisions.len(),
        deepest_collisions: deepest
            .iter()
            .take(8)
            .map(|c| (c.low, c.high, c.depth_mm, format!("{:?}-{:?}", c.low_piece, c.high_piece)))
            .collect(),
        collisions_by_pieces: by_pieces,
        overgrown_merges: census.overgrown.len(),
        overgrown_inserted: census
            .overgrown
            .iter()
            .filter(|merge| plan.struts.get(merge.child as usize).is_some_and(|s| s.inserted_by_load))
            .count(),
        warnings_by_class: classes,
        unbraced_history: plan.report.slenderness_lane.warnings.len(),
        unbraced_final: census.finals.unbraced.len(),
        unbraced_final_only: last.difference(&history).count(),
        unbraced_history_only: history.difference(&last).count(),
        braces_thicker_history: plan.report.slenderness_lane.braces_thicker_than_a_post,
        braces_thicker_final: census.finals.braces_thicker_than_a_post,
        landings_invalid_final: census.finals.landings.invalid_landings(),
        landings_unrooted_final: census.finals.landings.unrooted.len(),
        landings_invalid_history: ledger.invalid_landings(),
        landings_unrooted_history: ledger.unrooted.len(),
        shake_over_pixel_history: plan.report.slenderness_lane.stage_shake.stages_over_pixel,
        shake_over_pixel_final: census.finals.shake.as_ref().map(|shake| shake.stages_over_pixel),
        collision_seconds,
        final_seconds,
    }
}

#[cfg(test)]
mod tests;
