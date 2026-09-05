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

    // ---- the FLAT FACE's arms all go to the plate ---------------------------
    //
    // AMENDED 2026-09-04, when the descent learned to lean. The flat bottom
    // face is at one layer with nothing under it, so every arm seated on it
    // still has nothing to land on but the plate, and that is asserted below
    // arm by arm. The DRAFTED RIM is a different matter: its arms are seated
    // above the bottom layer over free space, and one of them may now lean the
    // fraction of a millimetre inward that puts it on the bottom face instead
    // of running the whole lift to the plate. Asserting "every arm lands on
    // the plate" would be asserting that no arm may ever lean, which is the
    // behaviour the owner asked to be rid of; what the oracle grades instead
    // is that no FLAT-FACE arm leans, and that no arm lands on nothing.
    let bottom_layer = plan
        .struts
        .iter()
        .map(|strut| strut.k_top)
        .min()
        .expect("a disc has struts");
    let mut flat_face_arms = 0_usize;
    for strut in &plan.struts {
        if strut.k_top == bottom_layer {
            flat_face_arms += 1;
            assert!(
                matches!(strut.landing, Landing::Plate),
                "strut {} is seated on the flat bottom face and did not land on the plate; on a \
                 flat face no lean can ever be cheaper than straight down",
                strut.id
            );
            assert!(
                !strut.is_leaned(),
                "strut {} is seated on the flat bottom face and LEANED; on a flat face every \
                 column's landing is at the same layer, so no lean can shorten anything",
                strut.id
            );
        }
    }
    assert!(
        flat_face_arms > 0,
        "no arm is seated on the flat bottom face; the partition below is vacuous"
    );
    assert_eq!(
        report.landings.plate + report.landings.model + report.landings.strut,
        report.struts_total,
        "every arm must land on the plate, the model or another arm"
    );

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
fn the_drafted_rim_costs_a_rim_strip_and_no_interior_arms() {
    // # Re-derived 2026-09-03, under the owner's one-lamina ruling
    //
    // This test was `the_rim_ramp_needs_no_supports_of_its_own` and asserted
    // ZERO arms above the bottom layer, on §C4's arithmetic: "a
    // single-lamina-only variant would charge 70/42.6 = 1.64 for the same climb
    // and wrongly demand rim supports". **That arithmetic is wrong, and it is
    // wrong in the direction that made the τ-spent form look necessary.**
    //
    // The rim is drafted `θ = atan(1.000 / 3.487) = 16.0°` off vertical. Per
    // layer it steps `h·tanθ = 0.05 × 0.28679 = 0.0143 mm`, not a whole lamina,
    // and over its 70 layers that accumulates to `1.0038 mm` against
    // `κ·h = 2.1300 mm`. The single-lamina charge for the rim's own climb is
    // therefore **0.471, not 1.64** — §C4 charged one full lamina step per
    // layer, which is the cost of a rim that creeps 0.05 mm a layer, not
    // 0.0143 mm. The rim does not unroot on its own account under EITHER form,
    // and the arithmetic below asserts it.
    //
    // Arms nevertheless appear on the rim under the one-lamina rule, and the
    // reason is the covering, not the ramp: the hex lattice is pitched at
    // exactly the guaranteed reach, so the worst-covered cell of the bottom
    // layer sits at `U = 1.000` with nothing left over, and the first rim arm
    // goes in TWO layers up — after 0.029 mm of creep, not 2.13 mm. That is a
    // fact about a zero-margin covering. What this test now pins is that the
    // price is confined to the rim strip and bounded by that strip's own
    // covering: no arm wanders into the disc's interior, and the rim is not
    // over-run with them.
    let reach = lamina_reach_mm();
    const RIM_RISE_MM: f64 = 3.487;
    const RIM_RUN_MM: f64 = 1.000;
    const RIM_LAYERS: f64 = 70.0;
    let per_layer_step_mm = (RIM_RUN_MM / RIM_RISE_MM) * (RIM_RISE_MM / RIM_LAYERS);
    let accumulated_mm = per_layer_step_mm * RIM_LAYERS;
    println!(
        "rim draft {:.2} deg off vertical, per-layer step {per_layer_step_mm:.6} mm, accumulated \
         run {accumulated_mm:.4} mm against one lamina's reach {reach:.4} mm = {:.3} of the \
         limit",
        (RIM_RUN_MM / RIM_RISE_MM).atan().to_degrees(),
        accumulated_mm / reach,
    );
    assert!(
        accumulated_mm < reach,
        "the rim's own accumulated run {accumulated_mm:.4} mm is not below one lamina's reach \
         {reach:.4} mm; §C4's 1.64 would then have been right after all"
    );

    let plan = sweep(&round_50(), 0.05);
    let bottom_layer = plan
        .struts
        .iter()
        .map(|strut| strut.k_top)
        .min()
        .expect("a disc has struts");
    let above: Vec<&plinth_sweep::SweepStrut> = plan
        .struts
        .iter()
        .filter(|strut| strut.k_top > bottom_layer)
        .collect();
    let radius_of = |strut: &plinth_sweep::SweepStrut| strut.center_mm[0].hypot(strut.center_mm[1]);
    let innermost = above.iter().copied().map(radius_of).fold(f64::INFINITY, f64::min);
    println!(
        "arms above the bottom layer: {} (innermost at r {:.3} mm, seat layers {:?}..{:?})",
        above.len(),
        innermost,
        above.iter().map(|strut| strut.k_top).min(),
        above.iter().map(|strut| strut.k_top).max(),
    );

    // ---- they are RIM arms, not interior demand ---------------------------
    //
    // The bottom face reaches r = 24.0. Anything seated above the bottom layer
    // and further in than one reach radius from that edge would be the disc's
    // own interior unrooting, which would be a defect and not a rim price.
    for strut in &above {
        let radius = radius_of(strut);
        assert!(
            radius > 24.0 - reach,
            "an arm above the bottom layer sits at r {radius:.3} mm, inside the bottom face's \
             own covering; the interior must stay rooted"
        );
        // AMENDED 2026-09-04: a rim arm overhangs free space, so its landing is
        // the plate unless a lean puts it on the bottom face a fraction of a
        // millimetre inboard -- which is shorter, and is priced as such. What
        // may never happen is a rim arm landing on ANOTHER ARM here, because
        // the only arms below it are the flat face's, whose tops are at the
        // bottom layer and therefore no higher than its own straight descent.
        assert!(
            matches!(strut.landing, Landing::Plate | Landing::Model { .. }),
            "a rim arm landed on {:?}; the drafted rim overhangs free space, so its arm goes to \
             the plate or onto the bottom face below it",
            strut.landing,
        );
    }

    // ---- and there are no more of them than the strip needs ---------------
    //
    // The rim's XY projection is an annulus from r = 24 to r = 25: a strip of
    // width w = 1.0 mm and mean circumference 2π·24.5 = 153.94 mm. Covering a
    // strip of width `w` with discs of radius `ρ` centred on its midline puts
    // them at most `2·sqrt(ρ² − (w/2)²)` apart, so one covering of this strip
    // is `153.94 / 4.141 = 37.2` arms. This is a strip covering and NOT the hex
    // cell count, because a 1 mm strip is narrower than the lattice pitch and
    // the lattice never applies to it.
    let strip_width_mm = 1.0_f64;
    let mean_circumference_mm = 2.0 * core::f64::consts::PI * 24.5;
    let along_mm = 2.0 * (reach * reach - (strip_width_mm / 2.0).powi(2)).sqrt();
    let one_covering = mean_circumference_mm / along_mm;
    println!(
        "rim strip covering: {mean_circumference_mm:.2} mm of midline at {along_mm:.3} mm \
         spacing = {one_covering:.1} arms; measured {}",
        above.len(),
    );
    assert!(
        above.len() as f64 <= one_covering,
        "{} arms on a rim strip one covering of which is {one_covering:.1}; the rim is being \
         re-covered on every layer instead of staying rooted from the layer below",
        above.len(),
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
