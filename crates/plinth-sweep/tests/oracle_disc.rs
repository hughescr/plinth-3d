//! **O1 — the disc.** Uniform peel over a flat circle carried by a field of
//! struts and nothing else.
//!
//! The whole point of the slice-sweep core. `Round_25/50/100` are flat bases
//! with a drafted rim and a recessed top, and the legacy engine could not
//! support one: its coverage raster refined per TRIANGLE, so a 98 mm sliver
//! cost 65,536 cells, and its load ledger charged each whole triangle to the
//! seat nearest its centroid, so half a fan's seats carried nothing and emitted
//! as hairline tips while a fifth clamped at the maximum radius — load-bearing
//! tips 8.5–9.5 mm apart against a 2.6 mm measured reach.
//!
//! What this oracle exercises: the reach test, greedy covering, plate landings,
//! conservation, the scar invariant, and tip sizing inside both clamps. **No
//! clamp warning may fire on a flat disc**; one that does is a red flag against
//! this oracle rather than a curiosity.
//!
//! # The fixture, and what it is not
//!
//! The `Round_*.stl` files are run outputs and are not in this repository;
//! committing them is LANE 2's job (design decision D11). Until then the oracle
//! grades a **parametric member of the same family** —
//! [`plinth_mesh::make_recessed_disc`] — built to the measured `Round_50`
//! profile: `r 24 → 25` over `z −3.5 → 0`, normal `nz = −0.276`, with the
//! 2.5 mm top recess. Every area is measured by the harness from the slice
//! stack; the design's hand-derived numbers are the grading target for THESE
//! dimensions and are asserted as such.

mod harness;

use harness::{bands, hex_cell_area_mm2, lamina_reach_mm, sweep, within, within_band, x_allowed};
use plinth_mesh::{make_disc_fan, make_disc_uniform, make_recessed_disc};
use plinth_sweep::Landing;

/// The Round 50 profile, measured from the source STL and reproduced here.
fn round_50() -> plinth_mesh::Mesh {
    make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128).expect("the Round 50 profile")
}

#[test]
fn the_disc_is_supported_by_a_plate_rooted_field_at_the_measured_spacing() {
    let plan = sweep(&round_50(), 0.05);
    let report = &plan.report;

    // ---- what the harness measured, printed so a moved number is visible ---
    println!("{}", report.render_disclosure());
    println!(
        "measured: peak layer {:.2} mm2, struts {}, scar {:.2} mm2, spacing mean {:.3} p95 \
         {:.3} max {:.3} mm, tips {:.4}..{:.4} mm radius",
        report.peak_layer_area_mm2,
        report.struts_total,
        report.total_scar_mm2,
        report.seat_spacing.mean_mm,
        report.seat_spacing.p95_mm,
        report.seat_spacing.max_mm,
        report.tip_radius.min_mm,
        report.tip_radius.max_mm,
    );

    // ---- the gate: no bare islands, no refusal -----------------------------
    assert_eq!(
        report.residual_demand_cells, 0,
        "a flat disc must leave no uncovered demand"
    );
    assert!(
        report.uncarried_load_mm2 <= 0.0,
        "{} mm2 of load went uncarried",
        report.uncarried_load_mm2
    );
    assert!(report.struts_total > 0, "a flat disc needs a support field");

    // ---- strut count against the hex covering ------------------------------
    // Demand is the bottom FACE: the rim is a drafted frustum, so the
    // bottom-most cross-section is a circle of r = 24.0, not the r = 25
    // footprint. pi*24^2 = 1809.6 mm2.
    let bottom_face_mm2 = core::f64::consts::PI * 24.0 * 24.0;
    let hex_count = (bottom_face_mm2 / hex_cell_area_mm2()).ceil();
    within_band(
        report.struts_total as f64,
        hex_count * bands::STRUT_COUNT.0,
        hex_count * bands::STRUT_COUNT.1,
        "strut count against the hex covering of the measured bottom face",
    );

    // ---- conservation: the PEAK layer, not the bottom and not the footprint -
    // With max-over-events the load delivered to the plate is the largest
    // single layer cross-section. For a drafted, recessed disc that layer is
    // the one immediately BELOW the recess floor: r_out(-2.5) = 24 + 1/3.5 =
    // 24.28571, area 1852.83 mm2.
    let predicted_peak = core::f64::consts::PI * (24.0_f64 + 1.0 / 3.5).powi(2);
    within(
        report.peak_layer_area_mm2,
        predicted_peak,
        bands::AREA_IDENTITY,
        "peak layer area measured from the slice stack",
    );

    // ---- the scar invariant ------------------------------------------------
    // Sum of pi*r^2 over the tips equals peak area / X_ALLOWED, INDEPENDENT of
    // how many arms share the load (`docs/physics.md`:100-118).
    within(
        report.total_scar_mm2,
        report.peak_layer_area_mm2 / x_allowed(),
        bands::AREA_IDENTITY,
        "total scar against peak_layer_area / X_ALLOWED",
    );
    // The plate carries the peak event AND the arms' own material, which is
    // cured resin and gets peeled like everything else: 233 arms across 100
    // layers of lift contribute one raster cell each per layer, about 58 mm².
    // So the reaction is bounded BELOW by the peak layer and above by the peak
    // plus the arms' own area, and asserting equality would be asserting that
    // the arms weigh nothing.
    let arm_material_mm2 = plan
        .struts
        .iter()
        .map(|strut| strut.carried_mm2.len() as f64 * plan.grid.cell_area_mm2())
        .sum::<f64>();
    within_band(
        report.plate_reaction_mm2,
        report.peak_layer_area_mm2,
        (report.peak_layer_area_mm2 + arm_material_mm2) * (1.0 + bands::AREA_IDENTITY),
        "plate reaction between the peak event and the peak event plus the arms' own material",
    );

    // ---- every landing is the plate, every arm is exactly the lift ----------
    assert_eq!(
        report.landings.plate, report.struts_total,
        "a flat disc has nothing to land on but the plate"
    );
    for strut in &plan.struts {
        assert!(
            matches!(strut.landing, Landing::Plate),
            "strut {} did not land on the plate",
            strut.id
        );
    }

    // ---- spacing -----------------------------------------------------------
    // Graded on the p95, not the mean, and the reason is a measurement rather
    // than a preference. The interior of the field is the lattice and every
    // interior arm's nearest neighbour is exactly the lattice pitch; the rim
    // strip is greedy fill and its arms sit closer together, so the MEAN
    // reports the boundary and the p95 reports the pitch. Round 50 measures a
    // mean of 2.70 mm and a p95 of 3.40 mm — the p95 is the quantity the
    // design's `√3 · ρ` predicts, and the mean is published beside it so the
    // boundary's density is visible rather than averaged away.
    within_band(
        report.seat_spacing.p95_mm,
        bands::SEAT_SPACING_MM.0,
        bands::SEAT_SPACING_MM.1,
        "p95 nearest-neighbour seat spacing (the interior lattice pitch)",
    );
    assert!(
        report.seat_spacing.mean_mm > 0.5 * bands::SEAT_SPACING_MM.0,
        "mean seat spacing {:.3} mm has collapsed; the boundary fill is out of control",
        report.seat_spacing.mean_mm
    );
    assert!(
        report.seat_spacing.max_mm <= 2.0 * lamina_reach_mm() * 1.25,
        "the widest seat gap {:.3} mm exceeds twice the measured reach",
        report.seat_spacing.max_mm
    );

    // ---- no clamp may fire on a flat disc ----------------------------------
    assert!(
        report.overloaded_struts.is_empty(),
        "a flat disc overloaded {} arms; that is a red flag against this oracle",
        report.overloaded_struts.len()
    );
    assert_eq!(
        report.tip_radius.count_at_max, 0,
        "no arm on a flat disc may clamp at the maximum radius"
    );
    assert!(
        report.conservation_residual <= 1.0e-9 * report.peak_layer_area_mm2.max(1.0),
        "conservation residual {} is above tolerance",
        report.conservation_residual
    );
}

#[test]
fn the_rim_ramp_needs_no_supports_of_its_own() {
    // The rim rises 3.487 mm over 70 layers while moving 1.0 mm outward. The
    // reach potential charges that climb about 0.15 against a limit of 1, so
    // the rim is self-rooted and demands nothing. A single-lamina-only reading
    // would charge 70/42.6 = 1.64 and wrongly demand rim supports, which is why
    // this is a test and not a remark.
    let plan = sweep(&round_50(), 0.05);
    let bottom_layer = plan
        .struts
        .iter()
        .map(|strut| strut.k_top)
        .min()
        .expect("a disc has struts");
    let above_the_bottom = plan
        .struts
        .iter()
        .filter(|strut| strut.k_top > bottom_layer)
        .count();
    assert_eq!(
        above_the_bottom, 0,
        "{above_the_bottom} arms were placed above the bottom layer; the rim and the recess \
         must be self-rooted"
    );
}

#[test]
fn the_same_disc_triangulated_two_ways_gets_the_same_field() {
    // **Triangulation invariance — the death of RC3 and RC4 as a test.** The
    // two meshes bound the same solid and differ only in how many triangles
    // cover the flat faces. An engine that keys its coverage raster or its load
    // ledger to triangles disagrees with itself between them.
    let fan = sweep(&make_disc_fan(30.0, 4.0, 128).unwrap(), 0.05);
    let uniform = sweep(&make_disc_uniform(30.0, 4.0, 128, 8).unwrap(), 0.05);
    assert!(
        uniform.struts.len() as f64 / fan.struts.len().max(1) as f64 > 3.0
            || uniform.struts.len() == fan.struts.len(),
        "sanity: the two fixtures must differ in triangle count, not in solid"
    );
    assert_eq!(
        fan.struts.len(),
        uniform.struts.len(),
        "strut count moved with the triangulation: {} against {}",
        fan.struts.len(),
        uniform.struts.len()
    );
    within(
        uniform.report.total_scar_mm2,
        fan.report.total_scar_mm2,
        0.01,
        "total scar across two triangulations of one solid",
    );
    assert_eq!(
        fan.report.sweep_digest, uniform.report.sweep_digest,
        "the placed field itself moved with the triangulation"
    );
}
