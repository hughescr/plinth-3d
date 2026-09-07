//! **O2 — the T.** A neck must transmit the peel of the larger layers beyond
//! it, and the reach spreading is what carries the load inward to it.
//!
//! `make_overhang_table` is the corpus fixture whose own doc comment already
//! names it the reach-spreading witness: four 4 × 4 × 20 mm legs at (±10, ±10)
//! under a 30 × 30 × 4 mm slab. The slab's underside is 900 mm² of down-facing
//! area, of which only the part within one lamina's reach of a leg is
//! self-rooted; the rest needs arms. The legs themselves are the neck, and the
//! load that walks inward along the slab concentrates into them.
//!
//! What this oracle exercises: reach spreading, conservation through a
//! bottleneck, the `τ` dependence of reach, and the model-section warning.

mod harness;

use harness::{bands, hex_cell_area_mm2, lamina_reach_mm, sweep, within, within_band, x_allowed};
use plinth_mesh::{make_overhang_table, make_overhang_table_thin};
use plinth_sweep::Landing;

const SLAB_UNDERSIDE_MM2: f64 = 900.0;
const LEG_WIDTH_MM: f64 = 4.0;
const LEG_CENTRES_MM: [[f64; 2]; 4] = [[-10.0, -10.0], [-10.0, 10.0], [10.0, -10.0], [10.0, 10.0]];

/// The self-rooted fringe around one leg: the square dilated by a disc of the
/// reach radius, with ROUNDED corners.
///
/// `w² + 4·w·ρ + π·ρ²`. Design conflict C3 corrects the oracle spec's grown
/// square here: the reachable set from a square root region is a Minkowski sum
/// with a disc, so the corners are quarter-discs and not squares.
fn leg_fringe_mm2() -> f64 {
    let reach = lamina_reach_mm();
    LEG_WIDTH_MM * LEG_WIDTH_MM
        + 4.0 * LEG_WIDTH_MM * reach
        + core::f64::consts::PI * reach * reach
}

#[test]
fn the_slab_load_walks_inward_to_the_legs_and_conserves_through_them() {
    let plan = sweep(&make_overhang_table().unwrap(), 0.05);
    let report = &plan.report;
    println!("{}", report.render_disclosure());
    println!(
        "measured: peak layer {:.2} mm2, struts {} (plate {}, model {}, strut {}), scar {:.2} \
         mm2, plate reaction {:.2} mm2, spacing mean {:.3} p95 {:.3} mm",
        report.peak_layer_area_mm2,
        report.struts_total,
        report.landings.plate,
        report.landings.model,
        report.landings.strut,
        report.total_scar_mm2,
        report.plate_reaction_mm2,
        report.seat_spacing.mean_mm,
        report.seat_spacing.p95_mm,
    );

    assert_eq!(report.residual_demand_cells, 0, "no bare islands on the T");
    assert!(
        report.uncarried_load_mm2 <= 0.0,
        "{} mm2 of load went uncarried",
        report.uncarried_load_mm2
    );

    // ---- the fringe is covered like everything else, not clustered ---------
    //
    // # Re-derived 2026-09-03, under the owner's one-lamina ruling
    //
    // This assertion was `inside_fringe == 0`: the slab's underside within one
    // reach radius of a leg is carried by the leg, so no arm belongs there.
    // That holds while `reach::weight` spends the cured thickness `τ`, because
    // then the run from the arms under a leg out to the leg's own edge — about
    // 2 mm through a 4 mm-square, 20 mm-tall solid post — costs almost nothing
    // and the leg's cells arrive at the slab with `U ≈ 0`, leaving a full
    // lamina of budget for the fringe.
    //
    // Under the ruling that run is priced at the one-lamina rate like every
    // other lateral run. The leg's own bottom layer is 4 × 4 mm and is covered
    // by two arms, so its worst-covered cell already sits at `U` close to 1;
    // the climb up the leg is free but gives nothing back, so at the slab there
    // is no budget left and the fringe is demand like the rest of the
    // underside. That is the price of the ruling, in the OVER-support
    // direction, and it is measured rather than argued: 12 arms.
    //
    // What must still be true — and is the thing this assertion existed to
    // catch — is that the fringe is covered at the SAME density as everywhere
    // else. An engine that had lost its reach criterion would pile arms up
    // against the legs. The fringe tested here is the leg square dilated by
    // `0.75 ρ`, an area of `4·w·d + π·d²` per leg; one hex covering of the four
    // is `4 · 33.58 / 11.79 = 11.4` arms, and the harness's own upper band for
    // a boundary-dominated region is 1.7 of that.
    let reach = lamina_reach_mm();
    let slab_bottom_layer = plan
        .struts
        .iter()
        .map(|strut| strut.k_top)
        .max()
        .expect("the T has arms");
    let inside_fringe = plan
        .struts
        .iter()
        .filter(|strut| strut.k_top == slab_bottom_layer)
        .filter(|strut| {
            LEG_CENTRES_MM.iter().any(|leg| {
                let dx = (strut.center_mm[0] - leg[0]).abs() - LEG_WIDTH_MM / 2.0;
                let dy = (strut.center_mm[1] - leg[1]).abs() - LEG_WIDTH_MM / 2.0;
                dx.max(0.0).hypot(dy.max(0.0)) < 0.75 * reach
            })
        })
        .count();
    let fringe_test_band = 0.75 * reach;
    let fringe_test_area_mm2 = 4.0
        * (4.0 * LEG_WIDTH_MM * fringe_test_band
            + core::f64::consts::PI * fringe_test_band * fringe_test_band);
    let one_covering = fringe_test_area_mm2 / hex_cell_area_mm2();
    println!(
        "leg fringe band {fringe_test_band:.4} mm, tested area {fringe_test_area_mm2:.2} mm2, \
         one hex covering {one_covering:.1} arms, measured {inside_fringe}"
    );
    assert!(
        inside_fringe as f64 <= one_covering * bands::STRUT_COUNT.1,
        "{inside_fringe} arms sit inside the legs' fringes against {:.1} the lattice would put \
         there; the sweep is clustering arms at the legs instead of covering them",
        one_covering * bands::STRUT_COUNT.1,
    );

    // ---- strut count ------------------------------------------------------
    // Slab demand is the underside minus the four fringes; the legs' own
    // undersides sit on the plate and take a hex minimum of two arms each.
    let slab_demand = SLAB_UNDERSIDE_MM2 - 4.0 * leg_fringe_mm2();
    let leg_underside = 4.0 * LEG_WIDTH_MM * LEG_WIDTH_MM;
    let hex_count = (slab_demand / hex_cell_area_mm2()).ceil() + 8.0;
    println!(
        "hand-derived: leg fringe {:.3} mm2 each, slab demand {:.2} mm2, leg underside {:.1} \
         mm2, hex prediction {hex_count}",
        leg_fringe_mm2(),
        slab_demand,
        leg_underside,
    );
    // REPORTED against a loose sanity bound, not graded on the disc oracle's
    // tight band, and the reason is measured rather than preferred. The T is a
    // 30 x 30 slab on four 4 mm legs: its perimeter-to-area ratio is four times
    // the disc's, and a 4 mm leg is SMALLER than the square a single reach ball
    // covers is wide (2*rho/sqrt(2) = 3.01 mm), so both the slab rim and the legs
    // are dominated by boundary fill rather than by the lattice. Measured 140
    // arms (117 slab, 23 leg) against a hex prediction of 63 -- a ratio of 2.2,
    // where the disc measures 1.51. The disc grades the covering density; this
    // oracle grades the load path, and the count is published so a future
    // improvement in boundary fill is visible as a fall in it.
    println!(
        "measured/hand-derived strut ratio: {:.2}",
        report.struts_total as f64 / hex_count
    );
    within_band(
        report.struts_total as f64,
        hex_count * bands::STRUT_COUNT.0,
        hex_count * 3.0,
        "strut count against the hex covering of the slab demand plus the legs",
    );

    // ---- conservation through the neck -------------------------------------
    // The governing peel event is the slab: 900 mm². Every path to the plate
    // runs either down an arm or down a leg, and the totals must close.
    within(
        report.peak_layer_area_mm2,
        SLAB_UNDERSIDE_MM2,
        bands::AREA_IDENTITY,
        "peak layer area measured from the slice stack",
    );
    within(
        report.total_scar_mm2,
        report.peak_layer_area_mm2 / x_allowed(),
        bands::AREA_IDENTITY,
        "total scar against peak_layer_area / X_ALLOWED",
    );
    assert!(
        report.conservation_residual <= 1.0e-9 * report.peak_layer_area_mm2.max(1.0),
        "conservation residual {} is above tolerance",
        report.conservation_residual
    );

    // ---- the legs are model, and the engine does not resize them -----------
    // X_model = fringe / leg section = 64.334 / 16 = 4.02, comfortably under
    // X_ALLOWED = 12, so the stock fixture must fire NO model-section warning.
    // The oracle spec implied it would; it does not, and that is why the thin
    // variant below exists.
    let leg_x_model = leg_fringe_mm2() / (LEG_WIDTH_MM * LEG_WIDTH_MM);
    assert!(
        leg_x_model < x_allowed(),
        "hand arithmetic: a 4 mm leg carries X_model = {leg_x_model:.2}"
    );
    assert!(
        report.model_section_overload.is_empty(),
        "the stock table fired {} model-section warnings; its legs carry X_model = {leg_x_model:.2}",
        report.model_section_overload.len()
    );

    // ---- WHERE THE SLAB'S ARMS MAY LAND ------------------------------------
    //
    // # Re-cut 2026-09-06, as a stronger claim than the one it replaces
    //
    // This read "every arm lands on the plate; nothing on this fixture is
    // above anything else", and it went red the moment a descent could land
    // part-way down a neighbour's shaft: 98 of the slab's arms now merge into
    // trunks. Nothing about the fixture changed — what was false was the
    // inference from "no MODEL is above anything else" to "no ARM is either".
    //
    // The claim that survives, and it is the sharper one, is that the slab's
    // flat underside offers a lean NOTHING BUT ANOTHER ARM to land on. A model
    // landing under this slab would be a landing on a surface that is not
    // there, and a lean that reported a plate landing would be a lean that
    // went nowhere. Both are asserted arm by arm, and the census is required
    // to partition.
    let mut leaned = 0_usize;
    for strut in &plan.struts {
        assert!(
            !matches!(strut.landing, Landing::Model { .. }),
            "arm {} landed on MODEL at {:?}; there is nothing under this slab but its four legs, \
             the air between them and the plate",
            strut.id,
            strut.landing
        );
        assert!(
            strut.is_leaned() == matches!(strut.landing, Landing::Strut { .. }),
            "arm {} leaned {} and landed on {:?}; leaving your own column and reaching another \
             arm's shaft are the same event on a flat underside",
            strut.id,
            strut.is_leaned(),
            strut.landing
        );
        if strut.is_leaned() {
            leaned += 1;
        }
    }
    assert!(
        leaned > 0,
        "no arm under the slab leaned, so the claims above were graded on nothing and the \
         invariance below is vacuous"
    );
    assert_eq!(
        report.landings.plate + report.landings.model + report.landings.strut,
        report.struts_total,
        "every arm must land on the plate, the model or another arm"
    );

    // ---- WHAT MERGING MOVES, AND WHAT IT CANNOT MOVE -----------------------
    //
    // The four legs and the plate together hold the slab's peel whatever the
    // arms above them do, and the area law sizes every root from the load it
    // carries. So `Σ π r_root²` over the plate-landing arms is an invariant of
    // the LANDING RULE — merging halves the number of roots and doubles their
    // section — and that is asserted here against the same fixture run with
    // the rule switched off. It is a prediction about a quantity nobody tuned.
    // **BOTH SIDES RUN WITH THE SLENDERNESS LANE OFF** (2026-09-06). Switching
    // shaft landings off leaves more members over their own slenderness limit,
    // so the lane answers with more masts, and a mast is a new plate contact
    // carrying its own root area. This claim is about the LANDING RULE, so the
    // second variable is held rather than absorbed.
    let merging = harness::sweep_with(
        &make_overhang_table().unwrap(),
        0.05,
        &plinth_sweep::SweepPolicy::from_registry()
            .expect("the registry supplies every number the sweep reads")
            .with_working_pitch_mm(0.05)
            .without_slenderness(),
    );
    let straight = harness::sweep_with(
        &make_overhang_table().unwrap(),
        0.05,
        &plinth_sweep::SweepPolicy::from_registry()
            .expect("the registry supplies every number the sweep reads")
            .with_working_pitch_mm(0.05)
            .without_slenderness()
            .without_shaft_landings(),
    );
    println!(
        "shaft landings ON: {} plate contacts, {:.6} mm2 of plate contact area; OFF: {} and \
         {:.6} mm2",
        merging.report.landings.plate,
        merging.report.plate_contact_area_mm2,
        straight.report.landings.plate,
        straight.report.plate_contact_area_mm2,
    );
    assert_eq!(
        straight.report.landings.strut, 0,
        "the knob must actually put the engine back on the straight descent"
    );
    assert!(
        merging.report.landings.plate < straight.report.landings.plate,
        "merging must take arms OFF the plate: {} contacts with the rule on against {} with it \
         off",
        merging.report.landings.plate,
        straight.report.landings.plate,
    );
    let area_gap =
        (merging.report.plate_contact_area_mm2 - straight.report.plate_contact_area_mm2).abs();
    assert!(
        area_gap <= 1.0e-4 * straight.report.plate_contact_area_mm2.max(1.0),
        "plate contact AREA moved by {area_gap:.6} mm2 between the two landing rules ({:.6} on, \
         {:.6} off); the plate holds the same load either way, so the area it holds it over is \
         not the rule's to change",
        merging.report.plate_contact_area_mm2,
        straight.report.plate_contact_area_mm2,
    );
}

#[test]
fn a_leg_too_thin_for_its_own_fringe_is_warned_about_and_still_emitted() {
    // The model-section witness. **The engine supports models; it does not
    // redesign them**, so a neck that cannot carry what walks into it is a
    // warning on a file that is still emitted, and the test's real content is
    // that the file IS still emitted.
    //
    // # The hand derivation is a BOUND, not a prediction, and that is a
    // # correction to what this test used to assume
    //
    // `w² + 4wρ + πρ²` is the whole Minkowski fringe around the leg, and the
    // leg only receives the part of it that routes to the leg RATHER THAN to a
    // nearby arm — load goes to its nearest root, and the slab is covered in
    // arms. So the fringe is an upper bound on what the leg can be charged,
    // never an equality. At `w = 1.0 mm` the bound is 23.77 and the engine
    // measured 11.81 against an `X_ALLOWED` of 12: the test passed on 1.6 % of
    // margin, on the side of the limit that a change in arm layout moves. That
    // is a fixture calibrated to a coincidence, so the leg is thinned until the
    // conclusion is robust, and both sides of the bound are asserted instead of
    // one.
    let width = 0.5;
    let reach = lamina_reach_mm();
    let fringe_bound =
        width * width + 4.0 * width * reach + core::f64::consts::PI * reach * reach;
    let bound_x = fringe_bound / (width * width);
    println!("hand-derived: a {width} mm leg can be charged at most X_model = {bound_x:.2}");
    assert!(
        bound_x > x_allowed(),
        "the thin variant must be able to exceed the limit by hand before the engine is asked"
    );

    let plan = sweep(&make_overhang_table_thin(width).unwrap(), 0.05);
    println!(
        "measured: struts {}, model-section warnings {}, worst X_model seen {:.4}, residual \
         demand {} cells",
        plan.report.struts_total,
        plan.report.model_section_overload.len(),
        plan.report.model_section_max_x,
        plan.report.residual_demand_cells,
    );
    for overload in plan.report.model_section_overload.iter().take(4) {
        println!(
            "  layer {} component {:.2} mm2 carrying {:.2} mm2, X = {:.2}",
            overload.layer, overload.component_area_mm2, overload.carried_mm2, overload.realized_x
        );
    }
    // EMIT AND WARN, NEVER REFUSE. The plan exists, it has arms, and it names
    // the overloaded section.
    assert!(plan.report.struts_total > 0, "the thin table still gets a field");
    assert!(
        !plan.report.model_section_overload.is_empty(),
        "a leg that can be charged up to X_model = {bound_x:.2} was not warned about; the engine \
         measured a worst section of {:.4}",
        plan.report.model_section_max_x,
    );
    let worst = plan
        .report
        .model_section_overload
        .iter()
        .map(|overload| overload.realized_x)
        .fold(0.0_f64, f64::max);
    assert!(
        worst > x_allowed(),
        "the warning must name a realized X above the limit, not merely fire"
    );
    // And the other side of the bound: no section may be charged more than the
    // uncontested fringe, because load that is not in the fringe has a nearer
    // root and cannot arrive here at all. Without this the test would pass just
    // as happily if the spread invented load.
    assert!(
        plan.report.model_section_max_x <= bound_x,
        "a model section realized X = {:.4} against an uncontested-fringe bound of {bound_x:.2}; \
         load arrived that had a nearer root",
        plan.report.model_section_max_x,
    );
}
