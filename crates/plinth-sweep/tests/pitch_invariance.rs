//! **P1 — pitch-halving invariance.**
//!
//! `SWEEP_WORKING_PITCH_MM` is a Placeholder and a discretization that DOES
//! move outputs, so the doctrine's "a constant may prune, never decide" is not
//! satisfied by argument alone. This is the test that keeps it from silently
//! deciding one: run the same model at two pitches and require the field to
//! agree on the quantities that are physics rather than raster.

mod harness;

use harness::{bands, sweep, within};
use plinth_mesh::make_recessed_disc;

#[test]
fn halving_the_working_pitch_does_not_move_the_physics() {
    // A 25 mm member of the same drafted, recessed family the disc oracle
    // grades, so two pitches fit comfortably in one test's time budget.
    let disc = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).unwrap();
    let coarse = sweep(&disc, 0.10);
    let fine = sweep(&disc, 0.05);

    println!(
        "coarse (0.10 mm): struts {}, peak {:.2} mm2, scar {:.3} mm2, spacing p95 {:.3} mm",
        coarse.report.struts_total,
        coarse.report.peak_layer_area_mm2,
        coarse.report.total_scar_mm2,
        coarse.report.seat_spacing.p95_mm,
    );
    println!(
        "fine   (0.05 mm): struts {}, peak {:.2} mm2, scar {:.3} mm2, spacing p95 {:.3} mm",
        fine.report.struts_total,
        fine.report.peak_layer_area_mm2,
        fine.report.total_scar_mm2,
        fine.report.seat_spacing.p95_mm,
    );

    // The measured cross-section is geometry, not raster: halving the pitch may
    // resolve the drafted rim better but must not move the area.
    within(
        fine.report.peak_layer_area_mm2,
        coarse.report.peak_layer_area_mm2,
        0.02,
        "peak layer area across a pitch halving",
    );
    // The scar is the physics: `Σ π r² = A_peak / X_ALLOWED`, and it must agree
    // across pitches to the same band the oracles grade it at.
    within(
        fine.report.total_scar_mm2,
        coarse.report.total_scar_mm2,
        bands::AREA_IDENTITY,
        "total scar across a pitch halving",
    );
    // The lattice pitch is set by the registered reach, not by the raster.
    within(
        fine.report.seat_spacing.p95_mm,
        coarse.report.seat_spacing.p95_mm,
        0.10,
        "p95 seat spacing across a pitch halving",
    );
    // And the count agrees inside the same band the oracles use, in both
    // directions.
    let ratio = fine.report.struts_total as f64 / coarse.report.struts_total.max(1) as f64;
    println!("strut-count ratio fine/coarse: {ratio:.3}");
    assert!(
        ratio > 1.0 / bands::STRUT_COUNT.1 && ratio < bands::STRUT_COUNT.1,
        "strut count moved by {ratio:.3}× across a pitch halving; the raster is deciding the field"
    );

    assert!((coarse.report.achieved_pitch_mm - 0.10).abs() < 1.0e-12);
    assert!((fine.report.achieved_pitch_mm - 0.05).abs() < 1.0e-12);
    assert_eq!(coarse.report.residual_demand_cells, 0);
    assert_eq!(fine.report.residual_demand_cells, 0);
}
