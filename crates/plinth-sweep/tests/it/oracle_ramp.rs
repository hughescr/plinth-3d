//! **O4 — the sloped underside.** The oracle the flat three could not be.
//!
//! `oracle_disc` uses `make_recessed_disc` (flat bottom), `oracle_tee` uses
//! `make_overhang_table` (flat slab, flat legs) and `oracle_cavity` uses
//! `make_cavity_box` (axis-aligned). On all three, **every arm is fed by a
//! single birth layer**, so an arm ledger that SUMS its per-layer shares
//! measures the same number as one that takes their MAXIMUM, and a reach test
//! that lets a ramp creep indefinitely never sees a ramp. Two defects hid in
//! exactly that blind spot and were found by running real poses:
//!
//! * the arm ledger summed distinct peel events, printing one arm carrying
//!   2566.58 mm² against a whole-model peak layer event of 682.85 mm² on the
//!   10° Round 100 (`plinth-evidence/slice-sweep-2026-09-02/lane3-orientation/measure/sweep-run.log`:16,21);
//! * the up-sweep called a tilted underside entirely rooted while the
//!   down-sweep lost 595.7 mm² of its load — `residual_demand == 0` beside
//!   `uncarried_load > 0`, a contradiction about one field.
//!
//! This oracle is a recessed disc PINNED 20° off the build axis, which is the
//! verifier's own prescription ("the recessed disc pinned 10-45 degrees off").
//! Its underside is a plane at 20°, so every arm along the slope is fed by a
//! DIFFERENT birth layer and the max-over-events rule is load-bearing. It
//! grades at the same bands `oracle_disc` uses, with the arm material named
//! rather than absorbed.

use crate::harness;

use harness::{bands, sweep, within_band, x_allowed};
use plinth_mesh::{make_recessed_disc, Mesh};

/// Tilt a mesh about the X axis. Test-local, four lines, and named: rotating a
/// fixture is what "pinned 20° off" means, and doing it here keeps the pose out
/// of the orientation search entirely so this oracle grades the sweep and
/// nothing else.
fn tilt_about_x(mesh: &Mesh, degrees: f64) -> Mesh {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let verts = mesh
        .verts()
        .iter()
        .map(|vertex| {
            [
                vertex[0],
                vertex[1] * cos - vertex[2] * sin,
                vertex[1] * sin + vertex[2] * cos,
            ]
        })
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rigid rotation preserves a valid mesh")
}

/// The Round 25 profile, pinned 20° off the build axis.
///
/// Round 25 rather than Round 50 because the pose triples the layer count (a
/// 25 mm disc tilted 20° spans 3.5/cos20° + 25·sin20° ≈ 12.3 mm of Z against
/// 3.5 mm flat), and the physics under test is the slope, not the diameter.
fn tilted_disc() -> Mesh {
    let flat = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("the Round 25 profile");
    tilt_about_x(&flat, 20.0)
}

#[test]
fn a_sloped_underside_charges_each_arm_the_largest_event_and_not_their_sum() {
    let plan = sweep(&tilted_disc(), 0.1);
    let report = &plan.report;
    println!("{}", report.render_disclosure());

    let cell_area = plan.grid.cell_area_mm2();
    // With max over events an arm's cell is a carrier on every layer it spans
    // and collects at least that layer's own single-cell event, so the whole
    // arm population can add at most one cell's event per arm to the plate.
    // Named, quantified, and three orders below the peel it is added to.
    let arm_material_mm2 = report.struts_total as f64 * cell_area;
    let invariant = report.injected_load_mm2 / x_allowed();
    println!(
        "measured: layers {}, pitch {:.4} mm, arms {}, peak layer {:.4} mm2, injected {:.4} mm2 \
         (ratio to peak {:.4}), scar {:.4} mm2 (invariant {:.4}, ratio {:.4}), plate reaction \
         {:.4} mm2 (ratio to injected {:.4}), arm material {:.4} mm2, uncarried {:.4} mm2, \
         residual demand {} cells",
        report.layers,
        report.achieved_pitch_mm,
        report.struts_total,
        report.peak_layer_area_mm2,
        report.injected_load_mm2,
        report.injected_load_mm2 / report.peak_layer_area_mm2,
        report.total_scar_mm2,
        invariant,
        report.total_scar_mm2 / invariant,
        report.plate_reaction_mm2,
        report.plate_reaction_mm2 / report.injected_load_mm2,
        arm_material_mm2,
        report.uncarried_load_mm2,
        report.residual_demand_cells,
    );

    assert!(report.struts_total > 0, "a 20 deg underside needs a support field");

    // ---- what the identity of §C2 becomes on a slope, re-derived -----------
    //
    // §C2 states the plate sees "the largest single layer cross-section", and
    // that derivation assumes what every flat oracle satisfies and this fixture
    // does not: that every arm is fed by ONE birth layer. Walk the sweep's own
    // rule instead. Descending, a cell takes `max(spread_in, own)`; a column
    // that already carried one layer up arrives with at least `own` and adds
    // nothing, so a column injects its own event exactly once, at the top of
    // each contiguous cured run it has. The conservation total is therefore
    // `Σ over columns of (runs × p²)` — the model's XY SILHOUETTE for a convex
    // solid — and on a flat-bottomed model every column's run tops out inside
    // the same layer, which makes the silhouette and the peak cross-section the
    // same number and recovers §C2 verbatim. `injected_load_mm2` is that total,
    // accumulated by the engine at the max step itself.
    //
    // This is the ONE re-derivation this fix makes, and it is confined to the
    // fixtures whose arms are fed by more than one event: `oracle_disc`,
    // `oracle_tee` and `oracle_cavity` are unchanged and still grade against
    // `peak_layer_area_mm2`, which is what keeps the two forms honest about
    // each other.
    assert!(
        report.injected_load_mm2 >= report.peak_layer_area_mm2 * (1.0 - bands::AREA_IDENTITY),
        "the conservation total {:.4} mm2 fell below the peak layer event {:.4} mm2; every cell \
         of the peak layer injects its own event somewhere at or above that layer",
        report.injected_load_mm2,
        report.peak_layer_area_mm2,
    );
    assert!(
        report.injected_load_mm2 > report.peak_layer_area_mm2 * 1.5,
        "the conservation total {:.4} mm2 is within 1.5x of the peak layer {:.4} mm2, so this \
         fixture's arms are NOT fed by distinct events and it is not testing what it exists to \
         test",
        report.injected_load_mm2,
        report.peak_layer_area_mm2,
    );

    // ---- the plate sees what was injected, and no more ---------------------
    //
    // Summing distinct events instead of taking their maximum is unbounded in
    // exactly this quantity: an arm's cell is a carrier on every layer it
    // passes through, so with `+` a 500-layer arm is charged 500 events. S11 is
    // that mutation and this assertion is what turns it red.
    assert!(
        report.plate_reaction_mm2
            <= report.injected_load_mm2 * (1.0 + bands::AREA_IDENTITY) + arm_material_mm2,
        "the plate saw {:.4} mm2 against {:.4} mm2 of injected peel plus {:.4} mm2 of arm \
         material; with max over events nothing may reach the plate that was never injected",
        report.plate_reaction_mm2,
        report.injected_load_mm2,
        arm_material_mm2,
    );

    // ---- the scar invariant, and the precondition it has always had --------
    //
    // `Σ π r² = ℓ / X_ALLOWED` holds only while every arm can actually be given
    // the radius the area law asks for. §6.5 says what happens when it cannot:
    // the arm emits at `r_max` and the engine warns with the realized
    // `X_real = carried/(π r_max²)`. A clamped arm therefore provides LESS scar
    // than the identity wants, by construction, and demanding the identity
    // anyway would be demanding that the clamp not exist. The flat oracles meet
    // the precondition and grade the identity directly; this one states it.
    let required_scar_mm2 = plan
        .struts
        .iter()
        .map(|strut| strut.tip_load_mm2() / x_allowed())
        .sum::<f64>();
    println!(
        "required scar {required_scar_mm2:.4} mm2, provided {:.4} mm2, {} overloaded arms",
        report.total_scar_mm2,
        report.overloaded_struts.len(),
    );
    // Over-providing is never excused by a clamp, so this half is unconditional.
    assert!(
        report.total_scar_mm2 <= required_scar_mm2 * (1.0 + bands::AREA_IDENTITY),
        "the field provides {:.4} mm2 of scar against {required_scar_mm2:.4} mm2 the area law \
         asks for; an arm may be clamped SMALL, never grown past its load",
        report.total_scar_mm2,
    );
    if report.total_scar_mm2 < required_scar_mm2 * (1.0 - bands::AREA_IDENTITY) {
        // Then the shortfall must be NAMED, arm by arm, with a realized X above
        // the limit. Emit and warn, never refuse — but never emit silently.
        assert!(
            !report.overloaded_struts.is_empty(),
            "the field provides {:.4} mm2 of scar against {required_scar_mm2:.4} mm2 required and \
             warned about nothing; a shortfall must be reported on the emitted file",
            report.total_scar_mm2,
        );
        let worst = report
            .overloaded_struts
            .iter()
            .map(|arm| arm.realized_x)
            .fold(0.0_f64, f64::max);
        assert!(
            worst > x_allowed(),
            "the shortfall warning names a realized X of {worst:.4} against X_ALLOWED {:.4}; it \
             must name a section that is actually over the limit",
            x_allowed(),
        );
    } else {
        within_band(
            report.total_scar_mm2,
            invariant * (1.0 - bands::AREA_IDENTITY),
            invariant * (1.0 + bands::AREA_IDENTITY),
            "total scar against the conservation total / X_ALLOWED on a sloped underside",
        );
    }
}

#[test]
fn the_two_sweeps_agree_about_a_sloped_underside() {
    let plan = sweep(&tilted_disc(), 0.1);
    let report = &plan.report;
    println!(
        "arms {}, removals {}, layers {}; residual demand {} cells ({:.4} mm2); uncarried load \
         {:.4} mm2 lowest at layer {:?} worst gap {:.4} mm; sweeps_disagree {}",
        report.struts_total,
        report.counters.strut_removals,
        report.layers,
        report.residual_demand_cells,
        report.residual_demand_mm2,
        report.uncarried_load_mm2,
        report.uncarried_lowest_layer,
        report.uncarried_worst_gap_mm,
        report.sweeps_disagree,
    );

    // The contradiction itself, as one predicate rather than two counters a
    // reader has to compare by hand. Since the reach step cost became one
    // lamina everywhere, the up-sweep's rootedness IMPLIES the down-sweep's
    // routability: `U <= 1` means the chamfer path back to a vertically
    // inherited cell is at most one lamina reach long, and a vertically
    // inherited cell was cured one layer down, which is exactly the carrier the
    // down-sweep looks for.
    assert!(
        !report.sweeps_disagree,
        "the up-sweep called every cell rooted while the down-sweep lost {:.4} mm2 of their \
         load; the two sweeps are using different reach criteria",
        report.uncarried_load_mm2,
    );
    assert_eq!(
        report.residual_demand_cells, 0,
        "a 20 deg underside is not an island; every cell is reachable from below"
    );
    assert!(
        report.uncarried_load_mm2 <= 0.0,
        "{:.4} mm2 of load reached no carrier on a sloped underside",
        report.uncarried_load_mm2,
    );
}
