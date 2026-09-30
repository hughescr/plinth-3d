//! What a sweep run must say about itself.
//!
//! **Every skip in this engine is disclosed here.** Skipping is legitimate;
//! hiding a skip is not. The rendered block names, in terms, what the engine
//! does (branching, bracing, member clearance of braces and masts) and what it
//! does not — joint spheres, raft, clearance of an arm's own shaft, the repair
//! loop — and states that their absence is not a claim that they are
//! unnecessary.
//!
//! **A shortfall is a WARNING on an emitted file, never a gate** (owner ruling
//! 2026-09-01). The output is diagnostic evidence about the engine, and a
//! refusal destroys that evidence.

use std::fmt::Write as _;

use crate::peel::{PeelFieldMode, PeelFieldStats};

/// One arm carrying more than the area law allows at its maximum radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverloadedStrut {
    pub strut_id: u32,
    pub layer: usize,
    pub carried_mm2: f64,
    pub realized_x: f64,
}

/// One connected component of MODEL material carrying more than `X_ALLOWED`.
///
/// The engine supports models; it does not redesign them. This is a warning
/// about the model's own section, not about a support.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelSectionOverload {
    pub layer: usize,
    pub component_area_mm2: f64,
    pub carried_mm2: f64,
    pub realized_x: f64,
}

/// Struts by where they land.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LandingCounts {
    pub plate: usize,
    pub model: usize,
    pub strut: usize,
    /// Landings inside a cavity, whose scar nobody will ever see (owner ruling
    /// 2026-08-31, "hidden cavity supports never need removing").
    pub interior: usize,
}

/// Summary statistics over a set of seats.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpacingStats {
    pub mean_mm: f64,
    pub p95_mm: f64,
    pub max_mm: f64,
}

/// Summary statistics over the emitted tip radii.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TipRadiusStats {
    pub min_mm: f64,
    pub p50_mm: f64,
    pub max_mm: f64,
    pub count_at_min: usize,
    pub count_at_max: usize,
    /// Arms above the owner's 0.400 mm-radius growth ceiling. **Reported, not
    /// clamped**: that ceiling bounds growth-for-attachment only, and a
    /// load-sized tip above it is legitimate
    /// (`crates/plinth-core/src/constants/evidence.rs`, the
    /// `TIP_RADIUS_MAX_GROWN_SCAR_MM` note).
    pub count_above_owner_growth_ceiling: usize,
}

/// The provenance block. Printed on every run so `κ`'s parents, and the one
/// unexplained disagreement in the physics behind it, travel with the file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReachProvenance {
    pub kappa_used: f64,
    pub kappa_first_loss: f64,
    pub kappa_audit_gate: f64,
    pub l_survived_max_mm: f64,
    pub layer_height_mm: f64,
    /// The cantilever-bending prediction from the registered
    /// `TENSILE_GREEN_MPA` / `PEEL_PRESSURE_PA` pair, both Placeholders. It
    /// disagrees with the plates by about 7.5×. **The measurement governs**;
    /// the disagreement is a standing question against a future green-strength
    /// coupon and is disclosed rather than reconciled.
    pub beam_model_kappa: f64,
    /// The registered spacing the sweep does NOT read — `κ` sets spacing here
    /// — printed side by side so the change is visible.
    pub target_spacing_mm_registered_but_unused: f64,
}

/// Everything one sweep says about itself.
#[derive(Clone, Debug, PartialEq)]
pub struct SweepRunReport {
    pub achieved_pitch_mm: f64,
    pub coarsening_steps: u32,
    pub layers: usize,
    pub cells_per_layer: usize,
    pub reach: ReachProvenance,
    /// `X_ALLOWED`, the design load ratio this run sized every member with
    /// (18 since Craig's ruling of 2026-09-30). Carried so every sentence the
    /// run prints about the area law states the X it actually used, never a
    /// typed number.
    pub x_allowed: f64,
    /// `TIP_RADIUS_MAX_LEGACY_MM`, the largest radius the law may ask for; the
    /// per-tip load ceiling is `x_allowed · π · tip_radius_max_mm²`.
    pub tip_radius_max_mm: f64,
    /// Whether the field this run placed would pass an audit whose span gate
    /// is written at `kappa_audit_gate`. Placement and gating are different
    /// jobs; this field exists so the difference is never decided by accident
    /// (owner ruling request OR-2).
    pub will_pass_audit_span_check: bool,
    pub max_tau_at_binding_reach_mm: f64,
    pub u_from_vertical_max: f64,
    pub tau_vertical_over_tau_used_max: f64,
    pub chamfer_not_converged: u64,
    pub struts_total: usize,
    pub landings: LandingCounts,
    pub seat_spacing: SpacingStats,
    pub tip_radius: TipRadiusStats,
    pub slenderness_p50: f64,
    pub slenderness_max: f64,
    /// The conservation total: every mm2 of peel this run injected into the
    /// load raster. `plate_reaction_mm2` and `total_scar_mm2 * X_ALLOWED` are
    /// graded against THIS, not against the peak layer: the two coincide on a
    /// flat-bottomed model, where every arm is fed by one birth layer, and
    /// part company on a sloped underside, where they must.
    pub injected_load_mm2: f64,
    pub peak_layer_area_mm2: f64,
    /// The largest X_model any model section realized, overloaded or not, so
    /// that "no warnings fired" can be told apart from "the check never ran".
    pub model_section_max_x: f64,
    pub total_scar_mm2: f64,
    pub plate_reaction_mm2: f64,
    /// Every cell that entered the demand set, summed over layers, as an area.
    ///
    /// **The measured support demand**, and the quantity the orientation
    /// objective's projected-down-facing-area proxy exists to estimate. It is
    /// NOT `total_scar_mm2`: by §C2 the scar is `peak_layer_area / X_ALLOWED`,
    /// an identity in the peak cross-section, so scoring a pose by its scar is
    /// the retired `w_peak * peak_xsec_cm2` term under a new name — and with
    /// the sign that made every disc tilt.
    pub demand_area_mm2: f64,
    pub residual_demand_cells: usize,
    pub residual_demand_mm2: f64,
    pub uncarried_load_mm2: f64,
    /// Where the loss was, so the number can be chased instead of believed.
    pub uncarried_lowest_layer: Option<usize>,
    /// How far the nearest carrier was at the worst losing cell, against the
    /// one-lamina reach the down-sweep routes within.
    pub uncarried_worst_gap_mm: f64,
    /// The two sweeps contradicting each other about one field.
    ///
    /// `residual_demand_cells == 0` is the up-sweep saying every model cell has
    /// a rooted carrier within reach. `uncarried_load_mm2 > 0` is the
    /// down-sweep saying load from some of those same cells reached no carrier
    /// at all. Both cannot be true, so reporting them as two independent
    /// counters lets a reader take a clean `residual demand 0` for a covered
    /// model when thousands of mm² of load are on the drive-way. Since the
    /// step cost became one lamina everywhere
    /// (`crate::reach::recompute_potential`) the up-sweep's predicate is
    /// strictly stronger than the down-sweep's, so this must be false; it is
    /// reported rather than asserted because "emit and warn, never refuse"
    /// applies to a defect discovered mid-run just as much as to a shortfall.
    pub sweeps_disagree: bool,
    /// Layers whose load-routing field hit its sweep bound without reaching a
    /// fixed point.
    ///
    /// **Computed since the routing field existed and, until now, read by
    /// nothing.** It is not the same error as `chamfer_not_converged`, and the
    /// difference is the direction: an unconverged CHAMFER reads high and
    /// over-supports, while an unconverged ROUTE leaves a load's distance to
    /// its carrier too high, so the load falls into `uncarried_load_mm2` and
    /// the field is UNDER-supported. The Cylinder's 379 non-convergences are
    /// the harmless kind; this counter is the one that would have named the
    /// other kind, and it was invisible.
    pub route_not_converged: u64,
    /// Arms the DOWN-sweep added because the registered area law asked for the
    /// section (`crate::down_sweep::insert_for_load`). Reach placed the rest.
    pub arms_inserted_by_load: usize,
    /// Layers where the bounded round search hit `max_rounds` while the area
    /// law still wanted more carriers — an ARTIFICIAL limit, not a physical
    /// one. Error direction: UNDER-supported; the layer might still have had
    /// room, `max_rounds` was just not raised far enough to find it. A
    /// warning on an emitted file; the arms themselves are named in
    /// `overloaded_struts`.
    pub insertion_rounds_exhausted: u64,
    /// Layers where a round ran and found NOWHERE left to place another
    /// carrier while the area law still wanted more — a physical exhaustion
    /// of the geometry, not a bound. Error direction: genuinely
    /// over-the-limit; more rounds would not have changed this. A warning on
    /// an emitted file; the arms themselves are named in `overloaded_struts`.
    pub insertion_geometry_exhausted: u64,
    /// Times the area-law trigger was evaluated (per per-event pass; per layer
    /// under the max-field sabotage). A path-hit counter:
    /// `arms_inserted_by_load == 0` must not be readable as "no arm needed one"
    /// when it in fact means "the step never ran".
    pub insertion_rounds: u64,
    pub unrooted_landings: usize,
    pub struts_at_minimum_radius: usize,
    pub overloaded_struts: Vec<OverloadedStrut>,
    pub model_section_overload: Vec<ModelSectionOverload>,
    pub conservation_residual: f64,
    pub peel_field_mode: PeelFieldMode,
    pub peel_field_stats: PeelFieldStats,
    pub sweep_digest: String,
    /// Working cells some drain escape cylinder covers. Zero on a model with
    /// no drains, and then the keep-out was not consulted at all.
    pub drain_keep_out_cells: usize,
    /// Placement candidates the keep-out refused. A path-hit witness: without
    /// it, "every drain is connected" cannot be told apart from "the keep-out
    /// never bound on anything".
    pub drain_keep_out_refusals: u64,
    /// Demand cells left unseated because every seat that would have rooted
    /// them stood inside a drain escape cylinder. A WARNING, and it is already
    /// included in `residual_demand_cells`.
    pub drain_seat_refusals: usize,
    /// What the escape flood found, per drain, on the FINISHED field.
    pub escape: crate::drain::EscapeReport,
    /// Arms whose descent LEANED instead of dropping straight down, because
    /// leaning was strictly cheaper under the arm's own material cost with the
    /// leaned part priced by the registered overhang weight.
    pub arms_leaned: usize,
    /// Of those, the ones that leaned onto ANOTHER ARM and merged into it. The
    /// merged trunk carries the summed load under the area law.
    pub arms_merged: usize,
    /// Cheaper leans that were found and then refused because the path they
    /// needed ran through model material, another arm, or a drain cylinder.
    /// The witness that the path check bites rather than waving everything
    /// through.
    pub lean_paths_blocked: usize,
    /// Merges undone because the parent trunk they loaded would have had to
    /// exceed the registered maximum tip radius to carry the sum.
    pub merges_refused_by_radius: usize,
    /// Merges the radius ceiling would have undone and did NOT, because
    /// another member rests on the arm that would have moved: a trunk that
    /// carries branches may not move without them. A WARNING quantity — the
    /// trunk it loads sits at the registered maximum radius.
    pub merges_kept_carrying: usize,
    /// Merged arms put back ONLY because the trunk they rest on was put back:
    /// a trunk moves with its branches or not at all. Counted inside
    /// `merges_refused_by_radius` too.
    pub merges_moved_with_trunk: usize,
    /// Landings the un-merge pass found whose parent does not stand at the
    /// landing layer at all. Counted, never skipped in silence.
    pub unmerge_stale_landings: usize,
    /// **Every member rests on something**: the landing-validity check run
    /// after every pass that edits landings, the finished field's own check,
    /// and the load gap `injected - plate reaction - uncarried`. A WARNING on
    /// an emitted file, never a refusal.
    pub landing_check: crate::landing_check::LandingLedger,
    /// The largest sideways move one layer of descent may make, in mm --
    /// `h * cot(OVERHANG_ANGLE_DEG - OVERHANG_WEIGHT_TRANSITION_DEG/2)`. Zero
    /// when the lean is disabled, which is the sabotage state.
    pub lean_max_step_mm: f64,
    /// Junctions in the FINISHED field whose landing layer is the parent arm's
    /// own top — the only junction this engine could make before 2026-09-05.
    pub junctions_at_parent_top: usize,
    /// Junctions in the FINISHED field whose landing layer is strictly inside
    /// the parent's shaft. The parent is split in effect: the area law grows
    /// its section from the junction DOWNWARD and leaves it untouched above.
    pub junctions_mid_shaft: usize,
    /// Junctions the DESCENT chose mid-shaft, before un-merging and removal had
    /// their say. The path-hit witness for the shaft-landing rule itself: it
    /// can be non-zero where the finished census is zero, and that difference
    /// is a fact about the later passes, not about the descent.
    pub descent_junctions_mid_shaft: usize,
    /// `(arm, column run)` intervals the column history recorded — the measured
    /// size of the shaft index, published so its cost is a number rather than
    /// a claim.
    pub shaft_spans_recorded: usize,
    /// The slenderness lane, published in full so the operator can see what
    /// the constraint did rather than infer it (owner's ruling of 2026-09-05).
    pub slenderness_lane: crate::slenderness::SlendernessOutcome,
    /// The congestion lane, published in full for the same reason: the operator
    /// sees what the footprint measured and what the remedies did, rather than
    /// inferring it from an arm count.
    pub congestion_lane: crate::congestion::CongestionOutcome,
    /// The footprint rule (fix/tip-trim, 2026-09-28): every tip's contact
    /// disc graded against its seat face, the seats it moved onto the face,
    /// and the knife edges it could only warn about.
    pub seat_footprint: crate::footprint::FootprintOutcome,
    /// The head-join rule (2026-09-29): member-on-arm joins graded against the
    /// trunk's seat zone, the landers moved down their trunk, the joins kept
    /// by reason, and the finished field's census.
    pub head_join: crate::head_join::HeadJoinOutcome,
    /// Per-event sizing (rule a′, 2026-09-29) on the finished field: events
    /// routed, the per-event conservation residual, the path-hit counts, and
    /// the heaviest member under this rule beside the max-field ledger it
    /// replaced.
    pub per_event: crate::per_event::PerEventReport,
    /// The rigid-body reading of the load share (2026-09-29): a WARNING-only
    /// diagnostic that never sizes; the arms whose rigid share passes `ℓ_max`.
    pub rigid_share: crate::rigid_share::RigidShareReport,
    /// The lip-seat rule (2026-09-29): every seated arm's approach to its seat
    /// face against the angle at which the seat trim keeps its section, the
    /// arms re-landed to meet it, and the emitted field's census.
    pub lip_seat: crate::lip_seat::LipSeatOutcome,
    /// **The resin the area law SIZED**, in mm³: `π r²` integrated along every
    /// arm's own axis, layer by layer, with a leaned layer measured along its
    /// slant rather than its rise.
    ///
    /// This is the LEDGER figure, not the file's. The resin the file actually
    /// holds is measured from the emitted records
    /// (`SweepEmission::drawn_volume_mm3`) and is published first, with this
    /// one beside it labelled as the sizing figure (2026-09-28: before then
    /// this number was printed as "support volume" while the file drew 34-51 %
    /// less on the taper spans).
    pub support_volume_mm3: f64,
    /// `Σ π r_root²` over the arms that land on the PLATE — the area the field
    /// asks the plate to hold, and the thing a merge removes when it takes an
    /// arm off the plate and onto a trunk.
    pub plate_contact_area_mm2: f64,
    /// `(layer, column)` pairs where a LEANED arm offered the routing field the
    /// column its own axis occupies one layer up. The path-hit witness for the
    /// tip-column root rule; zero on a field where nothing leaned.
    pub tip_column_roots_offered: u64,
    /// Load, in mm², whose nearest root was one of those tip columns — an UPPER
    /// bound on what the seat-blind accounting lost, since some of it would
    /// have reached a further carrier instead. Zero means the rule changed no
    /// routing on this model, which is what tells an inert rule apart from an
    /// absent one.
    /// Load deposited on a non-carrier column (a wrong routing root); part of
    /// `uncarried_load_mm2`, named so it can never hide inside it.
    pub misrouted_load_mm2: f64,
    pub tip_column_root_load_mm2: f64,
    /// Load, in mm², that landers handed to a support arm's head and that the
    /// down-sweep booked into the shaft BELOW the contact rather than into the
    /// contact slot (2026-09-29). The contact is sized from the arm's own peel;
    /// this load is carried by the shaft. Counted in
    /// [`PathHits::contact_merges_moved_below`].
    pub contact_merge_load_moved_mm2: f64,
    /// Path-hit counters. A green oracle must never be able to mean "the path
    /// never ran" (the witness-must-exercise-the-path lesson).
    pub counters: PathHits,
}

/// Counts of the paths a sabotage drill would break, so a green suite can be
/// distinguished from an unexercised one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PathHits {
    pub reach_tests: u64,
    pub vertical_inheritance_hits: u64,
    /// Cells a wall-creep step was priced for by the surface's overhang weight
    /// (`reach::creep_step`). Zero means the rule never ran.
    pub creep_cells_weighted: u64,
    /// Of those, the cells the weighted step actually rooted from a carrier
    /// one layer down.
    pub creep_steps_taken: u64,
    pub multi_carrier_split_hits: u64,
    pub strut_share_reads: u64,
    pub landing_injections: u64,
    pub max_combine_hits: u64,
    pub rasteriser_entries: u64,
    pub coarsening_loop_hits: u64,
    pub kappa_placement_reads: u64,
    pub kappa_audit_gate_reads: u64,
    pub lift_assertions: u64,
    pub strut_removals: u64,
    /// Arms the seat-only rule would have deleted and the span test kept,
    /// because a layer of their carried span would have been left with no
    /// carrier in reach. The witness that the check BITES in production.
    pub strut_removals_refused_by_span: u64,
    /// Removal passes undone whole because the down-sweep read MORE uncarried
    /// load afterwards than before. Non-zero is a warning about the field, not
    /// an error: the field kept is the one that carried more.
    pub removal_passes_rolled_back: u64,
    /// Candidate landings the descent's lean search priced. Zero means the
    /// search never ran, which a merge count of zero on its own cannot tell
    /// you.
    pub lean_candidates_priced: u64,
    /// Down-sweep reads where a lander's load reached a support arm's contact
    /// slot and was booked one slot lower, into the shaft (2026-09-29). Zero
    /// means no arm landed on another arm's head, which a green
    /// `contact_sizing` test must not be allowed to mean.
    pub contact_merges_moved_below: u64,
    /// The same, at an arm with no slot below its contact: the load stays in
    /// the one slot (there is no shaft to take it) and is counted here.
    pub contact_merges_kept_at_single_slot: u64,
    /// Straight sightlines the reach field priced and found clear, and those
    /// that crossed empty space so the path bent instead
    /// (`reach::any_angle_relax`, 2026-09-30). Zero clear means the reach is
    /// still a sum of raster steps; zero blocked on a model with holes means
    /// the geodesic was never tested.
    pub sightlines_clear: u64,
    pub sightlines_blocked: u64,
    /// The same two counts for the down-sweep's routing field.
    pub route_sightlines_clear: u64,
    pub route_sightlines_blocked: u64,
    /// Islands of demand measured with more than one covering, islands where a
    /// covering other than the global lattice won, and the seats that saved
    /// against the global lattice's own measured count on those islands
    /// (`up_sweep::choose_plan`, 2026-09-30).
    pub islands_compared: u64,
    pub islands_realigned: u64,
    pub island_seats_saved: u64,
    /// Every seat the up-sweep placed, and the hex covering lower bound of the
    /// demand it placed them on: per island, `max(1, ⌈area / ((3√3/2)·R²)⌉)`
    /// at the measured reach `R`. Their ratio is the covering efficiency.
    pub island_seats: u64,
    pub island_hex_bound: u64,
}

impl SweepRunReport {
    /// The disclosure block, verbatim, with this run's values filled in.
    #[must_use]
    pub fn render_disclosure(&self) -> String {
        let mut out = String::new();
        let pass = if self.will_pass_audit_span_check {
            "will"
        } else {
            "will NOT"
        };
        writeln!(
            out,
            "PLACEMENT: slice-sweep v1 (experimental). Read this before judging the file."
        )
        .expect("writing to a String cannot fail");
        writeln!(
            out,
            "This support field was placed by the slice-sweep core, not the legacy engine. It \
             supports the model by a measured reach criterion and sizes every arm by the \
             registered area law pi*r^2 >= carried_area / X_ALLOWED."
        )
        .expect("writing to a String cannot fail");
        writeln!(
            out,
            "Reach used for placement: kappa = {:.7} (LAMINA_REACH_RATIO, Derived from \
             L_SURVIVED_MAX_MM = {:.10} mm and LAYER_HEIGHT_DEFAULT_MM = {:.3} mm, plates 1 and 4 \
             pooled). The audit gates span at kappa = {:.7}; this field {pass} pass that check -- \
             placement and gating are different jobs and this difference is disclosed, not \
             resolved.",
            self.reach.kappa_used,
            self.reach.l_survived_max_mm,
            self.reach.layer_height_mm,
            self.reach.kappa_audit_gate,
        )
        .expect("writing to a String cannot fail");
        writeln!(
            out,
            "The measured kappa and the cantilever-bending prediction from the registered \
             TENSILE_GREEN_MPA / PEEL_PRESSURE_PA pair ({:.3}) disagree by {:.1}x. Both of those \
             inputs are Placeholders. THE MEASUREMENT GOVERNS; the disagreement is a standing \
             question against a future green-strength coupon and is not reconciled here.",
            self.reach.beam_model_kappa,
            self.reach.kappa_used / self.reach.beam_model_kappa,
        )
        .expect("writing to a String cannot fail");
        writeln!(
            out,
            "What this engine DOES: arms lean and merge onto other shafts (tops and mid-shaft \
             landings); a lander on a support arm joins below the arm's seat zone; long members \
             are tied by |X| cross-braces, thickened roots and plate masts; every brace's and \
             mast's drawn solid clears the model by the final gap and every member it is not \
             joined to; plate roots are kept clear of one another; drains keep their escape \
             path. What it does NOT do, and whose absence is not a claim that it is unnecessary: \
             no joint spheres at arm-onto-arm junctions, no radius-aware clearance of an ARM's \
             own shaft against the model or another arm (only its centreline cell is tested, so \
             an arm passing close to a wall is NOT detected), and no repair loop. The peel field \
             is uniform: the literature's area exponent and the machine's front-to-back tilt \
             factor are computed and reported below but decide nothing."
        )
        .expect("writing to a String cannot fail");
        writeln!(
            out,
            "Warnings on this file: residual demand {} cells ({:.4} mm2); uncarried load {:.4} \
             mm2; {} overloaded arms; {} arms above the owner's 0.400 mm-radius growth ceiling; \
             {} model sections carrying more than X_ALLOWED; {} arms surviving at the minimum tip \
             radius; {} coarsening steps to an achieved pitch of {:.4} mm; {} chamfer \
             non-convergences (they read HIGH and OVER-support); {} routing non-convergences \
             (they lose load and UNDER-support); {} layers where insertion hit the round BOUND \
             with the area law still wanting more (UNDER-support risk -- raising max_rounds \
             might still find room); {} layers where insertion found NO GEOMETRY left to place a \
             carrier (genuinely over the limit; more rounds would not help); {} unrooted \
             landings; slenderness p50 {:.1}:1, max {:.1}:1; reach decisions binding at cured \
             thickness up to {:.4} mm.",
            self.residual_demand_cells,
            self.residual_demand_mm2,
            self.uncarried_load_mm2,
            self.overloaded_struts.len(),
            self.tip_radius.count_above_owner_growth_ceiling,
            self.model_section_overload.len(),
            self.struts_at_minimum_radius,
            self.coarsening_steps,
            self.achieved_pitch_mm,
            self.chamfer_not_converged,
            self.route_not_converged,
            self.insertion_rounds_exhausted,
            self.insertion_geometry_exhausted,
            self.unrooted_landings,
            self.slenderness_p50,
            self.slenderness_max,
            self.max_tau_at_binding_reach_mm,
        )
        .expect("writing to a String cannot fail");
        writeln!(
            out,
            "Arms are placed by TWO criteria, both measured. Reach put in {} of them (cured \
             material must never span more than one lamina's reach to a carrier); the area law \
             put in the other {} (a tip carries at most X_ALLOWED * pi * r_max^2 = {:.4} mm2 \
             at X_ALLOWED {} and r_max {:.3} mm, and where the routed load exceeded that the \
             down-sweep added the carriers the law asks for and routed again, {} rounds in \
             total).",
            self.struts_total.saturating_sub(self.arms_inserted_by_load),
            self.arms_inserted_by_load,
            self.x_allowed * core::f64::consts::PI * self.tip_radius_max_mm * self.tip_radius_max_mm,
            self.x_allowed,
            self.tip_radius_max_mm,
            self.insertion_rounds,
        )
        .expect("writing to a String cannot fail");
        if self.escape.verdicts.is_empty() {
            writeln!(
                out,
                "Drains: this model has none the placement was told about, so no escape check \
                 ran. That is NOT a claim that no drain exists -- it is the absence of one."
            )
            .expect("writing to a String cannot fail");
        } else {
            writeln!(
                out,
                "Drains: {} checked, {} ENCLOSED. The check floods the empty space around the \
                 FINISHED field -- model, arms and plate all counted as solid -- at a {:.3} mm \
                 voxel pitch (HOLLOW_PITCH_MM, the pitch the drill stage decides the same \
                 question at) over {} voxels, {} of them solid, and asks whether a ball of the \
                 bore's own radius can travel from each outlet to the outside of the build. \
                 Keep-out: {} working cells, {} placement candidates refused, {} demand cells \
                 left unseated because every seat that would have rooted them stood in a drain \
                 cylinder (already counted in residual demand above).",
                self.escape.verdicts.len(),
                self.escape.enclosed(),
                self.escape.voxel_pitch_mm,
                self.escape.voxels,
                self.escape.solid_voxels,
                self.drain_keep_out_cells,
                self.drain_keep_out_refusals,
                self.drain_seat_refusals,
            )
            .expect("writing to a String cannot fail");
            for verdict in &self.escape.verdicts {
                if verdict.connected {
                    writeln!(
                        out,
                        "  drain at ({:.3}, {:.3}, {:.3}) mm, bore radius {:.3} mm: CONNECTED \
                         (widest free radius found near the outlet {:.3} mm against the {:.3} mm \
                         this bore needs; {} voxels probed).",
                        verdict.outlet_mm[0],
                        verdict.outlet_mm[1],
                        verdict.outlet_mm[2],
                        verdict.radius_mm,
                        verdict.best_clearance_mm,
                        verdict.required_clearance_mm,
                        verdict.probed_voxels,
                    )
                    .expect("writing to a String cannot fail");
                } else {
                    writeln!(
                        out,
                        "  WARNING drain at ({:.3}, {:.3}, {:.3}) mm, bore radius {:.3} mm: \
                         ENCLOSED. No path at least {:.3} mm in free radius runs from this \
                         outlet to the outside of the build; the widest free radius anywhere \
                         near the mouth is {:.3} mm ({} voxels probed). The resin in this cavity \
                         has nowhere to go: the supports around the outlet have made a second \
                         hollow space, which would itself need a drain. THE FILE IS STILL \
                         EMITTED -- this is a warning about the field, not a verdict on the \
                         model.",
                        verdict.outlet_mm[0],
                        verdict.outlet_mm[1],
                        verdict.outlet_mm[2],
                        verdict.radius_mm,
                        verdict.required_clearance_mm,
                        verdict.best_clearance_mm,
                        verdict.probed_voxels,
                    )
                    .expect("writing to a String cannot fail");
                }
            }
        }
        writeln!(
            out,
            "Conservation: injected peel {:.4} mm2 / plate reaction {:.4} mm2 / ratio {:.4}. \
             Every mm2 the down-sweep ever put into the load raster must arrive at the plate; \
             this is the check, on a model whose load may reach the plate through a model or arm \
             landing where the peak-layer identity does not apply.",
            self.injected_load_mm2,
            self.plate_reaction_mm2,
            if self.injected_load_mm2 > 0.0 {
                self.plate_reaction_mm2 / self.injected_load_mm2
            } else {
                f64::NAN
            },
        )
        .expect("writing to a String cannot fail");
        out.push_str(&self.render_landing_check());
        if self.sweeps_disagree {
            writeln!(
                out,
                "DEFECT IN THE ENGINE, NOT A STATEMENT ABOUT THIS MODEL: the two sweeps \
                 contradict each other. The up-sweep reports residual demand 0 cells -- every \
                 model cell has a rooted carrier within reach -- while the down-sweep reports \
                 {:.4} mm2 of load that reached no carrier. Both cannot be true of one field. \
                 The DOWN-sweep is the one to believe: it routes load one lamina at a time, \
                 which is the reach kappa was measured at, so the up-sweep has called something \
                 rooted by a path load cannot travel. Treat the arm sizes on this file as \
                 unverified. Lowest losing layer {:?}; worst gap to a carrier {:.4} mm against a \
                 one-lamina reach of {:.4} mm.",
                self.uncarried_load_mm2,
                self.uncarried_lowest_layer,
                self.uncarried_worst_gap_mm,
                self.reach.kappa_used * self.reach.layer_height_mm,
            )
            .expect("writing to a String cannot fail");
        }
        writeln!(
            out,
            "A shortfall above is a WARNING on an emitted file, never a gate. This output is \
             diagnostic evidence about the engine."
        )
        .expect("writing to a String cannot fail");
        out
    }

    /// The landing-validity line: every member rests on something, or the
    /// file says which do not. One line, with the per-pass trail beside it so
    /// a pass that broke a chain is named even when a later pass mended it.
    #[must_use]
    pub fn render_landing_check(&self) -> String {
        let ledger = &self.landing_check;
        let finished = &ledger.finished;
        let mut trail = String::new();
        for (index, pass) in ledger.passes.iter().enumerate() {
            if index > 0 {
                trail.push_str("; ");
            }
            write!(
                trail,
                "{} {} unrooted / {} invalid",
                pass.pass, pass.unrooted, pass.invalid_landings
            )
            .expect("writing to a String cannot fail");
        }
        let verdict = if finished.clean() {
            "every member rests on the plate, the model, or a member that stands there"
        } else {
            "WARNING: members whose lower end rests on NOTHING are in this file (a warning on an \
             emitted file, never a refusal)"
        };
        let mut out = String::new();
        writeln!(
            out,
            "Landings: {verdict}. Finished field: {} member-on-member landings checked; {} \
             unrooted members {:?}; {} landed on a parent that is not there at the landing \
             layer, {} on a parent that stands in another cell there; {} brace tops tied to a \
             member that has moved; load gap (injected - plate reaction - uncarried) {:.4} mm2, \
             of which {:.4} mm2 is peel carried under a larger event in the same arm ({} reads; \
             not lost) and {:.4} mm2 went nowhere the down-sweep can name; \
             {} over-grown merges kept because a branch they carry has nowhere to go; {} \
             branches put back with their trunk; {} stale landings met \
             by the un-merge pass; slenderness merges refused because the arm carries someone \
             {}, shafts refused because their member no longer stands there {}. After each \
             pass: {trail}.",
            finished.strut_landings,
            finished.unrooted.len(),
            finished.unrooted,
            finished.parent_absent.len(),
            finished.parent_elsewhere.len(),
            finished.brace_tops_detached.len(),
            ledger.load_gap_mm2,
            ledger.absorbed_mm2,
            ledger.absorbed_reads,
            ledger.unexplained_gap_mm2(),
            self.merges_kept_carrying,
            self.merges_moved_with_trunk,
            self.unmerge_stale_landings,
            self.slenderness_lane.merges_refused_carrying,
            self.slenderness_lane.merges_refused_stale_shaft,
        )
        .expect("writing to a String cannot fail");
        out
    }
}
