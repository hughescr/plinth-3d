//! **O3 — the cavity ceiling.** A ceiling that closes over a hollow demands a
//! support that lands on the cavity FLOOR, from inside.
//!
//! `docs/physics.md` §"supports may seat on model": a cavity ceiling must be
//! supported from the cavity's lower interior, never by punching the exterior
//! wall. And the owner's ruling of 2026-08-31: a scar inside a hidden cavity
//! never needs removing, so an interior landing is not a cost to be avoided.
//!
//! What this oracle exercises: interior landings, landing injection, demand
//! INSIDE the model, and — its sharpest lesion — **max over events at the
//! plate**. The bottom wall's own peel event is 1600 mm2 and the ceiling's is
//! 1156 mm2; the design of record predicted the plate would see 1600, and the
//! body of this test records why that prediction is wrong and what the
//! max-over-events rule actually buys. A sum bug does not read 2756 -- it reads
//! three orders of magnitude more -- and that is what sabotage S7 turns on.

use crate::harness;

use harness::{bands, hex_cell_area_mm2, lamina_reach_mm, sweep_with, within_band};
use plinth_mesh::make_cavity_box;
use plinth_sweep::{Landing, SweepPolicy};

const OUTER_MM: f64 = 40.0;
const WALL_MM: f64 = 3.0;
const CAVITY_MM: f64 = OUTER_MM - 2.0 * WALL_MM;

#[test]
fn the_cavity_ceiling_is_carried_from_inside_and_the_plate_sees_a_maximum() {
    // Graded at the X this fixture was built at (12,
    // `harness::PATH_FIXTURE_X_ALLOWED`): at the ruled 18 no merge on the
    // cavity box is over-grown, the un-merge undoes none, and the anti-vacuity
    // check for the 2026-09-06 fix below fails (measured 2026-09-30). Nothing
    // else here reads X: placement is by reach, and the ceiling scar below is
    // printed against the X the run used.
    let policy = harness::at_path_fixture_x(
        SweepPolicy::from_registry()
            .expect("the registry supplies every number the sweep reads")
            .with_working_pitch_mm(0.10),
    );
    let plan = sweep_with(&make_cavity_box(OUTER_MM, WALL_MM).unwrap(), 0.10, &policy);
    let report = &plan.report;
    println!("{}", report.render_disclosure());

    // ---- hand-derived, before the code was asked ---------------------------
    let ceiling_mm2 = CAVITY_MM * CAVITY_MM;
    let reach = lamina_reach_mm();
    // Eroding a square by a disc gives a square, so the wall-rooted fringe is
    // exactly the ring the erosion removes.
    let interior_demand = (CAVITY_MM - 2.0 * reach).powi(2);
    let wall_rooted_fringe = ceiling_mm2 - interior_demand;
    let box_underside_mm2 = OUTER_MM * OUTER_MM;
    let cavity_hex = (interior_demand / hex_cell_area_mm2()).ceil();
    let plate_hex = (box_underside_mm2 / hex_cell_area_mm2()).ceil();
    let naive_floor_only = (ceiling_mm2 / hex_cell_area_mm2()).ceil();
    println!(
        "hand-derived: ceiling {ceiling_mm2:.0} mm2, wall-rooted fringe {wall_rooted_fringe:.2} \
         mm2, interior demand {interior_demand:.2} mm2 -> {cavity_hex} cavity arms; box \
         underside {box_underside_mm2:.0} mm2 -> {plate_hex} plate arms; the naive floor-only \
         baseline that ignores wall rooting would be {naive_floor_only} cavity arms, a {:.0} % \
         difference, which is why this oracle exists",
        100.0 * (naive_floor_only / cavity_hex - 1.0)
    );

    let interior_arms = plan.struts.iter().filter(|strut| strut.interior).count();
    let model_landings = plan
        .struts
        .iter()
        .filter(|strut| matches!(strut.landing, Landing::Model { .. }))
        .count();
    println!(
        "measured: struts {} (plate {}, model {}, strut {}, interior {}), peak layer {:.1} mm2, \
         plate reaction {:.1} mm2, scar {:.2} mm2, slenderness p50 {:.1} max {:.1}",
        report.struts_total,
        report.landings.plate,
        report.landings.model,
        report.landings.strut,
        report.landings.interior,
        report.peak_layer_area_mm2,
        report.plate_reaction_mm2,
        report.total_scar_mm2,
        report.slenderness_p50,
        report.slenderness_max,
    );

    assert_eq!(report.residual_demand_cells, 0, "no bare islands in the box");
    assert!(
        report.uncarried_load_mm2 <= 0.0,
        "{} mm2 of load went uncarried",
        report.uncarried_load_mm2
    );

    // ---- the ceiling is carried from INSIDE --------------------------------
    assert!(
        model_landings > 0,
        "the cavity ceiling got no arm landing on model; it can only be reached from inside"
    );
    // ---- `interior` is a fact about the LANDING, DERIVED HERE --------------
    //
    // # Re-cut 2026-09-06, as a stronger claim than the count it replaces
    //
    // This assertion used to read `interior_arms == model_landings` and it went
    // red at 37 of 49 when the descent learned to land part-way down another
    // arm's shaft. Two production defects were behind that, both fixed, and
    // both are graded below rather than accommodated:
    //
    // 1. `interior` was evaluated at the arm's SEAT column and seat layer.
    //    Every arm under every overhang has model above its seat — that is
    //    what makes a face down-facing — so the seat form answered "interior"
    //    for exterior scars, and for a leaned arm it answered about a column
    //    the arm's base is not even in. It is now asked at the LANDING, which
    //    is where a scar is actually worn.
    // 2. It was computed before `unmerge_overgrown`, which MOVES ARMS OFF
    //    TRUNKS and therefore changes landings. The 12 arms that read false
    //    were exactly the un-merged ones, flagged from a landing they no
    //    longer had.
    //
    // The expected count is derived from the fixture's own solid, arm by arm,
    // and never pasted: this box is CLOSED, so every point strictly inside its
    // outer footprint and below its top face has cured material standing over
    // it, and every landing therefore wears a scar nobody will see. An arm
    // landing on a shelf with open sky above it would be counted false by the
    // same derivation, which is what makes this a test of the rule rather than
    // of this box.
    let height = harness::layer_height_mm();
    let (min_z, _) = harness::z_span(&make_cavity_box(OUTER_MM, WALL_MM).unwrap());
    let plate_z = min_z - harness::LIFT_MM;
    let half_outer = OUTER_MM / 2.0;
    let mut expected_interior = 0_usize;
    for strut in &plan.struts {
        let Landing::Model { layer } = strut.landing else {
            continue;
        };
        let base = strut.base_cell();
        let x = plan.grid.center_x_mm(plan.grid.col_of(base));
        let y = plan.grid.center_y_mm(plan.grid.row_of(base));
        let z = plate_z + (layer as f64 + 0.5) * height;
        // Material stands over this landing iff it is under the box's own top
        // face and inside the box's footprint.
        let covered = x.abs() < half_outer && y.abs() < half_outer && z < min_z + OUTER_MM;
        if covered {
            expected_interior += 1;
        }
    }
    assert_eq!(
        expected_interior, model_landings,
        "the fixture's own geometry says every model landing in a closed box is covered; if this \
         fails the derivation below is grading the wrong thing"
    );
    assert_eq!(
        interior_arms, expected_interior,
        "`interior` must agree, arm by arm, with the fixture's own answer about what stands over \
         each LANDING"
    );
    assert_eq!(
        report.landings.interior, expected_interior,
        "the published census must agree with the field it describes"
    );
    // Anti-vacuity for defect (2): the un-merge loop must actually have fired
    // on this fixture, or the "recomputed after un-merging" half of the fix is
    // being graded on nothing.
    assert!(
        report.merges_refused_by_radius > 0,
        "no merge was undone on the cavity box, so the interior census was never re-asked after \
         a landing moved; this fixture no longer exercises the 2026-09-06 fix"
    );
    assert_eq!(report.landings.interior, interior_arms);
    // The cavity is 34 mm tall, so an interior arm is long. The design records
    // the slenderness and INVENTS NO BUCKLING CONSTANT to gate it; a coupon is
    // the fix. This asserts only that the histogram is populated.
    assert!(
        report.slenderness_max > 10.0,
        "a 34 mm interior arm must show up in the slenderness histogram"
    );

    // ---- counts, over the TREE and not over the roots alone ----------------
    //
    // # Re-cut 2026-09-06
    //
    // This band used to be graded on `model_landings` alone, which was the
    // whole cavity field while every arm ran straight down. Once an arm may
    // land part-way down a neighbour's shaft the cavity field is a TREE: 49 of
    // its members reach the cavity floor themselves and the rest reach it
    // through one of those 49. Counting only the roots would report the field
    // shrinking by two thirds when nothing left the cavity, so the count
    // resolves each arm to the landing its load ultimately arrives at.
    let root_is_model = |mut index: usize| -> bool {
        // The landing graph is a forest (a merge is always onto an arm that is
        // already placed), so the walk terminates; the bound is belt and
        // braces against a future cycle rather than a live risk.
        for _ in 0..plan.struts.len() {
            match plan.struts[index].landing {
                Landing::Model { .. } => return true,
                Landing::Plate => return false,
                Landing::Strut { id, .. } => index = id as usize,
            }
        }
        false
    };
    let cavity_field = (0..plan.struts.len()).filter(|index| root_is_model(*index)).count();
    println!(
        "cavity field {cavity_field} arms over {model_landings} floor landings ({} of them \
         merged onto another member)",
        cavity_field - model_landings
    );
    assert!(
        cavity_field >= model_landings,
        "every floor landing is its own tree's root, so the tree cannot be smaller than the roots"
    );
    within_band(
        cavity_field as f64,
        cavity_hex * bands::STRUT_COUNT.0,
        cavity_hex * 3.0,
        "cavity-ceiling arms against the hex covering of the INTERIOR demand",
    );
    assert!(
        (cavity_field as f64) < naive_floor_only * 3.0,
        "the cavity field ignored the wall-rooted fringe: {cavity_field} arms against a \
         floor-only baseline of {naive_floor_only}"
    );

    // ---- THE PLATE SEES A MAXIMUM OVER EVENTS, NOT A SUM OVER LAYERS -------
    //
    // # Correction to the design of record's §7.3, reported not absorbed
    //
    // §7.3 predicts `Σ_plate = 1600 mm², NOT 2756`, on the reasoning that the
    // ceiling's 1156 mm² "delivers down the same path at a LOWER PER-CELL
    // DENSITY (0.765 inside the cavity footprint) against 1.000 for the bottom
    // layer's own event, so the maximum is the bottom layer's own event
    // everywhere". **That reasoning assumes the routed load arrives diffuse,
    // and it does not.** An arm concentrates the ceiling's whole share into ONE
    // raster cell of the cavity floor, where the per-cell density is ~1000×
    // the wall's own event and the maximum keeps it. The two events therefore
    // do add at the plate, and the measured reaction is
    // `bottom wall's own event + the routed ceiling + the arms' own cured
    // material`.
    //
    // That is the correct physics, not a defect: an arm's point load has to go
    // somewhere and the wall under it has to carry it. What the max-over-events
    // rule buys is not this — it is that a 900-layer stack does not charge 900
    // events. That is what is asserted here, because it is the discriminator a
    // sum bug actually fails: sabotage S7 replaces the per-cell max with a sum
    // over every layer and the reaction rises by three orders of magnitude, not
    // by 72 %.
    let sum_over_layers = report.peak_layer_area_mm2 * report.layers as f64;
    let arm_material: f64 = plan
        .struts
        .iter()
        .map(|strut| strut.carried_mm2.len() as f64 * plan.grid.cell_area_mm2())
        .sum();
    let routed_upper_bound = box_underside_mm2 + ceiling_mm2 + arm_material;
    println!(
        "plate reaction {:.1} mm2; bottom-wall event {box_underside_mm2:.0} + routed ceiling \
         {ceiling_mm2:.0} + arm material {arm_material:.0} = {routed_upper_bound:.0}; a sum over \
         all {} layers would be {sum_over_layers:.0}",
        report.plate_reaction_mm2,
        report.layers,
    );
    assert!(
        report.plate_reaction_mm2 >= box_underside_mm2 * (1.0 - bands::AREA_IDENTITY),
        "the plate reaction {:.1} mm2 is below the bottom wall's own peel event alone",
        report.plate_reaction_mm2
    );
    assert!(
        report.plate_reaction_mm2 < 0.02 * sum_over_layers,
        "the plate reaction {:.1} mm2 is within 2 % of a sum over every layer \
         ({sum_over_layers:.0} mm2); the events are being added instead of maximised",
        report.plate_reaction_mm2
    );
    within_band(
        report.plate_reaction_mm2,
        box_underside_mm2,
        routed_upper_bound * 1.35,
        "plate reaction between the bottom wall's own event and that event plus the routed \
         ceiling and the arms' own material",
    );
    assert!(
        report.conservation_residual <= 1.0e-9 * report.peak_layer_area_mm2.max(1.0),
        "conservation residual {} is above tolerance",
        report.conservation_residual
    );

    // ---- the scar the ceiling wears ----------------------------------------
    let ceiling_scar: f64 = plan
        .struts
        .iter()
        .filter(|strut| matches!(strut.landing, Landing::Model { .. }))
        .map(|strut| {
            let radius = strut.tip_radius_mm();
            core::f64::consts::PI * radius * radius
        })
        .sum();
    println!(
        "ceiling scar {ceiling_scar:.2} mm2 against a hand-derived {:.2} mm2",
        interior_demand / plan.report.x_allowed
    );
    assert!(
        ceiling_scar > 0.0,
        "the interior arms carry nothing; the ceiling's load is not reaching them"
    );
}
