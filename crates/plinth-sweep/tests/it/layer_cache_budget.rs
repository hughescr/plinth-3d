//! **The layer cache's byte budget changes nothing a plan says** (perf lane
//! D1 review, 2026-10-15).
//!
//! The sweep's per-plan layer cache ([`plinth_sweep::layer_cache`]) draws its
//! bytes from one process-wide pool. A layer that does not fit is computed on
//! demand by the consumer instead. These tests run whole plans on the oracle
//! fixtures with the pool at its default, at ZERO (every layer streamed) and
//! at a few MiB (some layers kept), and require the same plan, field by field
//! and bit by bit.
//!
//! The pool is process-wide, so a test that lowers it also lowers it for the
//! tests running beside it. That is harmless by the property under test (a
//! plan is the same at every budget) and the limit is put back before the
//! test returns.

use crate::harness;

use harness::sweep_with_uncached;
use plinth_mesh::{make_cavity_box, make_overhang_table, Mesh};
use plinth_sweep::layer_cache::{streamed_fills_total, BytePool};
use plinth_sweep::{SweepPlan, SweepPolicy};

/// Everything a plan says, as text with every float at its exact bits.
///
/// `Debug` prints a float's shortest round-trip form, which determines its
/// bits, so equal text is equal bits.
fn fingerprint(plan: &SweepPlan) -> String {
    format!(
        "grid {:?} layers {} height {:?} plate {:?}\nstruts {:?}\nreport {}\ntaper {:?}",
        plan.grid,
        plan.layers,
        plan.layer_height_mm,
        plan.plate_z_mm,
        plan.struts,
        plan.report.render_disclosure(),
        plan.load_step_taper,
    )
}

/// Run `mesh` at the process pool's default, at zero and at `partial` bytes;
/// return the three fingerprints and the layers streamed by the last two.
fn at_three_budgets(mesh: &Mesh, policy: &SweepPolicy, partial: u64) -> ([String; 3], [u64; 2]) {
    let pool = BytePool::process();
    let default = fingerprint(&sweep_with_uncached(mesh, policy));

    let before = pool.set_limit(0);
    let streamed_before = streamed_fills_total();
    let zero = fingerprint(&sweep_with_uncached(mesh, policy));
    let streamed_zero = streamed_fills_total() - streamed_before;

    pool.set_limit(partial);
    let streamed_before = streamed_fills_total();
    let some = fingerprint(&sweep_with_uncached(mesh, policy));
    let streamed_some = streamed_fills_total() - streamed_before;
    pool.set_limit(before);
    ([default, zero, some], [streamed_zero, streamed_some])
}

fn assert_same(label: &str, prints: &[String; 3], streamed: [u64; 2]) {
    assert!(
        streamed[0] > 0,
        "{label}: a zero budget streamed no layer, so the comparison proved nothing"
    );
    assert!(
        streamed[1] > 0,
        "{label}: the partial budget streamed no layer, so the comparison proved nothing"
    );
    assert!(prints[0].len() > 1000, "{label}: the fingerprint is too thin to mean anything");
    assert_eq!(prints[0], prints[1], "{label}: a zero budget changed the plan");
    assert_eq!(prints[0], prints[2], "{label}: a partial budget changed the plan");
}

#[test]
fn the_tee_plan_is_the_same_at_every_budget() {
    let policy = SweepPolicy::from_registry()
        .expect("the registry supplies every number the sweep reads")
        .with_working_pitch_mm(0.10)
        .without_clearance_reroute();
    let (prints, streamed) = at_three_budgets(&make_overhang_table().unwrap(), &policy, 3 << 20);
    assert_same("tee", &prints, streamed);
}

#[test]
#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: about 80 s; scripts/gate-full.sh runs it")]
fn the_cavity_box_plan_is_the_same_at_every_budget() {
    let policy = harness::at_path_fixture_x(
        SweepPolicy::from_registry()
            .expect("the registry supplies every number the sweep reads")
            .with_working_pitch_mm(0.10),
    );
    let (prints, streamed) = at_three_budgets(&make_cavity_box(40.0, 3.0).unwrap(), &policy, 3 << 20);
    assert_same("cavity box", &prints, streamed);
}
