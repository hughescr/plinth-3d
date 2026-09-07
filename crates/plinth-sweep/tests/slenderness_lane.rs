//! **Long thin verticals with nothing holding them sideways** — the owner's
//! ruling of 2026-09-05, graded on real fields rather than on fixtures built to
//! agree with it.
//!
//! `slenderness::tests` says what the LAW computes. This says what the finished
//! plan is: that the remedies fire in the order the plan of record sets, that
//! each rung of the ladder is what is doing the work (by deleting the rung above
//! it and watching the next one take over), that an `|X|` really is two crossing
//! diagonals inside the overhang law with no horizontal span, that a mast is
//! within its own brace reach and inside its own slenderness limit, and that a
//! flat disc — whose arms are nowhere near the limit — is byte-identical with
//! the constraint on and off.

mod harness;

use harness::{sweep_with, LIFT_MM};
use plinth_core::m0_constants;
use plinth_mesh::{make_brace_pair, make_lonely_peak, make_recessed_disc, make_tall_stilt_plate};
use plinth_sweep::{
    unbraced_runs, Landing, SlendernessLaw, SweepPlan, SweepPolicy, SweepStrut, UnbracedRun,
};

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every number the sweep reads")
}

/// The law as the production policy builds it, read from the registry here
/// rather than from the crate under test, so a change to the ratio cannot agree
/// with itself.
fn law() -> SlendernessLaw {
    let constants = m0_constants().expect("the registry constructs");
    let placeholders = constants.placeholders();
    SlendernessLaw::new(
        placeholders.slenderness_limit_ratio().diagnostic().get(),
        placeholders.tip_radius_default_mm().diagnostic().get(),
        placeholders.tip_radius_max_legacy_mm().diagnostic().get(),
        placeholders.overhang_angle_deg().diagnostic().get()
            - 0.5
                * placeholders
                    .overhang_weight_transition_deg()
                    .diagnostic()
                    .get(),
    )
}

fn runs_of(plan: &SweepPlan) -> Vec<UnbracedRun> {
    unbraced_runs(&plan.grid, &plan.struts, plan.layer_height_mm, &law())
}

fn braces(plan: &SweepPlan) -> Vec<&SweepStrut> {
    plan.struts
        .iter()
        .filter(|strut| strut.kind == plinth_sweep::StrutKind::Brace)
        .collect()
}

fn masts(plan: &SweepPlan) -> Vec<&SweepStrut> {
    plan.struts
        .iter()
        .filter(|strut| strut.kind == plinth_sweep::StrutKind::Mast)
        .collect()
}

/// The tallest, thinnest thing in the fixture set: a 30 x 30 x 4 mm plate on
/// four 2 mm legs, 30 mm up.
fn stilt_plate() -> plinth_mesh::Mesh {
    make_tall_stilt_plate().expect("the tall stilt plate fixture")
}

/// The lane does REAL work on the broad case, and the field it leaves is not
/// worse than the one it found.
///
/// The claim is deliberately not "every run is under its limit". It is not, and
/// pretending otherwise would be the lying metric this project keeps catching:
/// the remedies are chosen against the radii the down-sweep had already
/// computed, and re-running the down-sweep afterwards RESIZES the arms the
/// remedies moved — a shorter arm carries less and is sized thinner, so its own
/// limit falls under it. What IS claimed, and measured:
///
/// * the ladder fires (merges + braces + masts > 0);
/// * the number of over-limit runs falls, strictly;
/// * whatever is left is NAMED as a warning, one per run, and no arm was
///   dropped or shortened to make the number look better.
#[test]
fn the_ladder_fires_on_the_stilt_plate_and_leaves_fewer_over_limit_runs_than_it_found() {
    let mesh = stilt_plate();
    let with = sweep_with(&mesh, 0.0, &policy());
    let without = sweep_with(&mesh, 0.0, &policy().without_slenderness());

    let lane = &with.report.slenderness_lane;
    let applied = lane.merges_forced + lane.braces_added + lane.masts_dropped;
    assert!(
        applied > 0,
        "the lane applied no remedy at all on the stilt plate: {lane:?}"
    );

    let before = runs_of(&without)
        .iter()
        .filter(|run| run.over_limit())
        .count();
    let after = runs_of(&with).iter().filter(|run| run.over_limit()).count();
    assert!(
        before > 0,
        "the stilt plate has no over-limit run to remedy, so this test grades nothing"
    );
    assert!(
        after < before,
        "the lane left {after} over-limit runs against the {before} it found"
    );
    assert_eq!(
        after,
        lane.warnings.len(),
        "every run left over the limit must be NAMED as a warning, not summarised away"
    );
    assert!(
        with.struts.len() >= without.struts.len(),
        "the lane must never leave a field with fewer members: {} against {}",
        with.struts.len(),
        without.struts.len()
    );
}

/// **THE SABOTAGE LADDER.** Each rung is proved by deleting the rung above it
/// and watching the next one take the work over. A test that only ran the
/// production policy would be green if merging alone did everything.
#[test]
fn deleting_each_rung_hands_the_work_to_the_next_one_down() {
    let mesh = stilt_plate();
    let full = sweep_with(&mesh, 0.0, &policy());
    let no_merge = sweep_with(&mesh, 0.0, &policy().without_slenderness_merge());
    let no_brace = sweep_with(&mesh, 0.0, &policy().without_slenderness_brace());
    let no_mast = sweep_with(&mesh, 0.0, &policy().without_slenderness_mast());

    assert_eq!(
        no_merge.report.slenderness_lane.merges_forced, 0,
        "the merge knob must actually delete remedy 1"
    );
    assert!(
        no_merge.report.slenderness_lane.braces_added
            + no_merge.report.slenderness_lane.masts_dropped
            > 0,
        "with merging deleted the same runs must be tied by braces or masts instead: {:?}",
        no_merge.report.slenderness_lane
    );

    assert_eq!(
        no_brace.report.slenderness_lane.braces_added,
        no_brace.report.slenderness_lane.masts_dropped,
        "with bracing deleted the only crosses left are the ones a mast brings with it"
    );
    assert!(
        no_brace.report.slenderness_lane.masts_dropped > 0,
        "with bracing deleted the work must fall to masts: {:?}",
        no_brace.report.slenderness_lane
    );

    assert_eq!(no_mast.report.slenderness_lane.merges_forced, 0);
    assert_eq!(no_mast.report.slenderness_lane.braces_added, 0);
    assert_eq!(no_mast.report.slenderness_lane.masts_dropped, 0);
    assert!(
        !no_mast.report.slenderness_lane.warnings.is_empty(),
        "with every remedy deleted the runs must come out as NAMED warnings"
    );
    // **NEVER FEWER ARMS.** Emit and warn: a run the lane cannot tie is a
    // warning on an emitted file, and the arm stays exactly where it was.
    let arms = |plan: &SweepPlan| {
        plan.struts
            .iter()
            .filter(|strut| strut.kind == plinth_sweep::StrutKind::Arm)
            .count()
    };
    let baseline = arms(&sweep_with(&mesh, 0.0, &policy().without_slenderness()));
    assert_eq!(
        arms(&no_mast),
        baseline,
        "warning-only mode moved an arm; it must move nothing at all"
    );
    assert!(
        arms(&full) >= baseline - full.report.slenderness_lane.merges_forced,
        "the lane dropped arms rather than tying them"
    );
}

/// **THE `|X|` IS TWO CROSSING DIAGONALS, AND NOTHING IN IT IS HORIZONTAL.**
///
/// Every brace in a field: both ends on a member, a strictly positive rise, and
/// an angle no shallower than the saturated overhang angle the descent itself
/// obeys. The last of those is what the owner asked for in as many words —
/// "ANGLED, as an X, so there is `|X|`" and no unsupported horizontal span.
#[test]
fn every_diagonal_rises_and_none_is_shallower_than_the_overhang_law_admits() {
    let mesh = make_brace_pair().expect("the brace-pair fixture");
    let plan = sweep_with(&mesh, 0.0, &policy());
    let braces = braces(&plan);
    assert!(
        !braces.is_empty(),
        "the brace-pair fixture produced no diagonal at all: {:?}",
        plan.report.slenderness_lane
    );
    let constants = m0_constants().expect("the registry constructs");
    let placeholders = constants.placeholders();
    let saturated_deg = placeholders.overhang_angle_deg().diagnostic().get()
        - 0.5
            * placeholders
                .overhang_weight_transition_deg()
                .diagnostic()
                .get();
    let steepest_step = plan.layer_height_mm / saturated_deg.to_radians().tan();
    for brace in &braces {
        assert!(
            brace.k_top > brace.k_bottom,
            "a diagonal with no rise is a horizontal span, which the ruling forbids"
        );
        assert!(
            matches!(brace.landing, Landing::Strut { .. }),
            "a diagonal must land on a member, not on the plate or the model"
        );
        assert!(
            brace.seat_on_strut.is_some(),
            "a diagonal must record the member its top seats on"
        );
        let layers = brace.k_top - brace.k_bottom;
        let lateral_mm = {
            let grid = &plan.grid;
            let seat = brace.cell;
            let base = brace.base_cell();
            let dx = grid.center_x_mm(grid.col_of(seat)) - grid.center_x_mm(grid.col_of(base));
            let dy = grid.center_y_mm(grid.row_of(seat)) - grid.center_y_mm(grid.row_of(base));
            dx.hypot(dy)
        };
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer count bounded by the plan's own layer count is exact in f64"
        )]
        let admitted_mm = layers as f64 * steepest_step;
        assert!(
            lateral_mm <= admitted_mm + 1.0e-9,
            "a diagonal moved {lateral_mm} mm sideways over {layers} layers, which the overhang \
             law caps at {admitted_mm} mm"
        );
    }
    // The pair fixture has exactly two clusters, so an `|X|` between them is an
    // even number of diagonals: every cross is a member and its twin.
    assert!(
        braces.len().is_multiple_of(2),
        "an |X| is TWO diagonals; {} is not a whole number of crosses",
        braces.len()
    );
    assert_eq!(
        braces.len(),
        2 * plan.report.slenderness_lane.braces_added,
        "the published cross count and the emitted diagonals must be the same thing"
    );
}

/// **A MAST IS INSIDE ITS OWN BRACE REACH AND INSIDE ITS OWN LIMIT.**
///
/// The lonely peak has nothing within 20 mm of it, so remedy 2 has no partner
/// and remedy 3 has to fire. Every mast: plate-rooted, tied by a cross, within
/// the reach the law publishes, and its own run up to its first tie inside its
/// own `L_max`.
#[test]
fn every_mast_is_plate_rooted_braced_and_inside_its_own_slenderness_limit() {
    let mesh = make_lonely_peak().expect("the lonely-peak fixture");
    // The peak's own arms have neighbours: remedy 2 can tie them to each other.
    // Deleting bracing is what makes this fixture grade the MAST path rather
    // than whichever rung happens to fire first.
    let plan = sweep_with(&mesh, 0.0, &policy().without_slenderness_brace());
    let masts = masts(&plan);
    assert!(
        !masts.is_empty(),
        "the lonely peak dropped no mast: {:?}",
        plan.report.slenderness_lane
    );
    let law = law();
    for mast in &masts {
        assert_eq!(mast.landing, Landing::Plate, "a mast is plate-rooted");
        assert_eq!(mast.k_bottom, 0, "a mast runs from the plate");
        let tied: Vec<&SweepStrut> = plan
            .struts
            .iter()
            .filter(|strut| {
                strut.seat_on_strut == Some(mast.id)
                    || matches!(strut.landing, Landing::Strut { id, .. } if id == mast.id)
            })
            .collect();
        assert!(
            !tied.is_empty(),
            "mast {} stands with nothing tied to it, which is pure waste",
            mast.id
        );
        for diagonal in &tied {
            let grid = &plan.grid;
            let seat = diagonal.cell;
            let base = diagonal.base_cell();
            let dx = grid.center_x_mm(grid.col_of(seat)) - grid.center_x_mm(grid.col_of(base));
            let dy = grid.center_y_mm(grid.row_of(seat)) - grid.center_y_mm(grid.row_of(base));
            let reach_mm = law.thickest_brace_reach_mm();
            assert!(
                dx.hypot(dy) <= reach_mm + 1.0e-9,
                "a mast was tied across {} mm, beyond the {reach_mm} mm the law reaches",
                dx.hypot(dy)
            );
        }
    }
    // Every mast's own runs, measured by the same function that measures every
    // other member's.
    for run in runs_of(&plan) {
        let strut = &plan.struts[run.strut_id as usize];
        if strut.kind != plinth_sweep::StrutKind::Mast {
            continue;
        }
        assert!(
            !run.over_limit(),
            "mast {} runs {} mm unbraced against its own limit of {} mm",
            run.strut_id,
            run.length_mm,
            run.limit_mm
        );
    }
}

/// **WHAT THE CONSTRAINT DOES TO A FLAT DISC — MEASURED, NOT ASSUMED.**
///
/// The plan of record predicted the Round bases would be untouched: "their arms
/// are ~5 mm at >= 0.2 mm radius, under any plausible S". **That prediction does
/// not survive measurement and this test records the correction rather than
/// hiding it.** A disc's field is not uniform: the area law gives most of its
/// arms a comfortable radius, and gives the ones carrying almost nothing the
/// SMALLEST PRINTABLE TIP, 0.050 mm. At `S = 50` that tip's limit is 2.5 mm, and
/// those arms run five millimetres and more. Round 25 measures 12 such runs and
/// Round 50 measures 23, so the constraint bites on the discs and the emitted
/// files move. Whether S = 50 is right for them is the owner's call on a sample;
/// what this test holds is that the movement is ATTRIBUTABLE and BOUNDED.
///
/// * the lane measured the discs at all (non-vacuity: a published longest run);
/// * the ARM field is unchanged in count — every member the lane added is a
///   brace or a mast, and no arm was dropped;
/// * the only arms whose PLACEMENT moved are the ones the lane merged, and
///   there are exactly `merges_forced` of them;
/// * no disc comes out with more over-limit runs than it went in with.
#[test]
fn a_flat_discs_arm_field_moves_only_where_the_lane_merged_it() {
    for (name, inner, outer, rim, floor_thickness) in [
        ("Round 25", 11.5, 12.5, 3.5, 2.5),
        ("Round 50", 24.0, 25.0, 3.5, 2.5),
    ] {
        let mesh = make_recessed_disc(inner, outer, rim, floor_thickness, 10.0, 96)
            .unwrap_or_else(|error| panic!("the {name} profile builds: {error}"));
        let with = sweep_with(&mesh, 0.0, &policy());
        let without = sweep_with(&mesh, 0.0, &policy().without_slenderness());
        let lane = &with.report.slenderness_lane;

        assert!(
            lane.longest_run_mm > 0.0,
            "{name}: the lane published no run at all, so this test grades nothing"
        );
        let arms = |plan: &SweepPlan| {
            plan.struts
                .iter()
                .filter(|strut| strut.kind == plinth_sweep::StrutKind::Arm)
                .count()
        };
        assert_eq!(
            arms(&with),
            arms(&without),
            "{name}: the lane changed the ARM count; it may only add braces and masts"
        );
        assert_eq!(
            with.struts.len() - without.struts.len(),
            2 * lane.braces_added + lane.masts_dropped,
            "{name}: the members added and the remedies published disagree"
        );

        let moved = with
            .struts
            .iter()
            .zip(&without.struts)
            .filter(|(one, two)| {
                one.cell != two.cell
                    || one.k_top != two.k_top
                    || one.k_bottom != two.k_bottom
                    || one.landing != two.landing
                    || one.path != two.path
            })
            .count();
        assert_eq!(
            moved, lane.merges_forced,
            "{name}: {moved} arms moved against {} merges published",
            lane.merges_forced
        );

        let before = runs_of(&without)
            .iter()
            .filter(|run| run.over_limit())
            .count();
        let after = runs_of(&with).iter().filter(|run| run.over_limit()).count();
        assert!(
            after <= before,
            "{name}: the lane left {after} over-limit runs against the {before} it found"
        );
        assert_eq!(
            after,
            lane.warnings.len(),
            "{name}: every run left over the limit must be NAMED"
        );
    }
}

/// **FORCE STILL BALANCES ACROSS EVERY BRACE JUNCTION.**
///
/// A diagonal's bottom is a `Landing::Strut`, exactly like a merge, so the
/// ledger must carry its load into the member it lands on: the parent's carried
/// profile at the junction layer must be at least what the child hands it, and
/// the profile must never decrease downward. The same claim
/// `shaft_landing.rs` makes about merges, made about the members this lane adds.
#[test]
fn force_crosses_every_brace_junction_and_no_profile_decreases_downward() {
    let mesh = stilt_plate();
    let plan = sweep_with(&mesh, 0.0, &policy());
    let mut junctions = 0_usize;
    for child in &plan.struts {
        for pair in child.carried_mm2.windows(2) {
            assert!(
                pair[0] >= pair[1] - 1.0e-9,
                "member {}'s carried profile rises upward: {} then {}",
                child.id,
                pair[0],
                pair[1]
            );
        }
        if child.kind != plinth_sweep::StrutKind::Brace {
            continue;
        }
        let Landing::Strut { id, layer } = child.landing else {
            panic!("a diagonal must land on a member");
        };
        let parent = &plan.struts[id as usize];
        let Some(slot) = layer.checked_sub(parent.k_bottom) else {
            continue;
        };
        let Some(parent_here) = parent.carried_mm2.get(slot) else {
            continue;
        };
        let handed = child.carried_mm2.first().copied().unwrap_or(0.0);
        assert!(
            *parent_here >= handed - 1.0e-9,
            "member {} handed {} mm2 to member {} at layer {}, which reads {}",
            child.id,
            handed,
            id,
            layer,
            parent_here,
        );
        junctions += 1;
    }
    assert!(
        junctions > 0,
        "no brace junction was exercised, so this test grades nothing"
    );
}

/// The lift the harness applies is the registered one, asserted here so a
/// change to it is visible in this file rather than silently changing what
/// every fixture above measures.
#[test]
fn the_fixtures_are_measured_at_the_registered_lift() {
    assert!((LIFT_MM - 5.0).abs() < f64::EPSILON);
}
