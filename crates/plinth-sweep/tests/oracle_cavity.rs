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

mod harness;

use harness::{bands, hex_cell_area_mm2, lamina_reach_mm, sweep, within_band, x_allowed};
use plinth_mesh::make_cavity_box;
use plinth_sweep::Landing;

const OUTER_MM: f64 = 40.0;
const WALL_MM: f64 = 3.0;
const CAVITY_MM: f64 = OUTER_MM - 2.0 * WALL_MM;

#[test]
fn the_cavity_ceiling_is_carried_from_inside_and_the_plate_sees_a_maximum() {
    let plan = sweep(&make_cavity_box(OUTER_MM, WALL_MM).unwrap(), 0.10);
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
    assert_eq!(
        interior_arms, model_landings,
        "every model landing under a closed ceiling is an interior one, whose scar nobody sees"
    );
    assert_eq!(report.landings.interior, interior_arms);
    // The cavity is 34 mm tall, so an interior arm is long. The design records
    // the slenderness and INVENTS NO BUCKLING CONSTANT to gate it; a coupon is
    // the fix. This asserts only that the histogram is populated.
    assert!(
        report.slenderness_max > 10.0,
        "a 34 mm interior arm must show up in the slenderness histogram"
    );

    // ---- counts ------------------------------------------------------------
    within_band(
        model_landings as f64,
        cavity_hex * bands::STRUT_COUNT.0,
        cavity_hex * 3.0,
        "cavity-ceiling arms against the hex covering of the INTERIOR demand",
    );
    assert!(
        (model_landings as f64) < naive_floor_only * 3.0,
        "the cavity field ignored the wall-rooted fringe: {model_landings} arms against a \
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
        interior_demand / x_allowed()
    );
    assert!(
        ceiling_scar > 0.0,
        "the interior arms carry nothing; the ceiling's load is not reaching them"
    );
}
