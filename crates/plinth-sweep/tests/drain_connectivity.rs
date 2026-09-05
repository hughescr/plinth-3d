//! **The drain must still drain once the supports are on it.**
//!
//! The owner opened the emitted Sphere in ChituBox on 2026-09-03 and found the
//! hollow model's drain outlet "completely covered by a clutter of thick
//! overlapping supports at the bottom". Placement had never been told the bore
//! exists.
//!
//! Three assertions, and the middle one is the load-bearing one:
//!
//! 1. A drain that opens into free air reads CONNECTED. Without this the whole
//!    check could be a constant `false`.
//! 2. **Sabotage.** Delete the keep-out — the one thing this lane added to
//!    placement — and the same flat-underside fixture's drain reads ENCLOSED.
//!    This is what proves the keep-out is what holds the mouth open, rather
//!    than the fixture never having been in danger.
//! 3. With the keep-out in force, the mouth is measurably open — and where the
//!    resin still cannot cross the arm field, that shortfall is a published
//!    WARNING on an emitted file rather than a silence.
//!
//! The gap-width invariant (a flood must not squeeze through a passage
//! narrower than the bore diameter) is asserted one level down, on the field
//! itself, in `plinth_hollow::passage`'s own tests, because that is where the
//! threshold lives.

mod harness;

use plinth_sweep::{DrainSpec, SweepPolicy};

/// The fixture: a table whose whole underside is flat and stands WELL clear of
/// the plate, so the up-sweep covers that underside with a hex lattice of arms
/// at the registered reach spacing — the geometry that walls a mouth in — and
/// there is genuine vertical room under it for a bore-wide passage.
///
/// The room matters and is not incidental. Under a slab resting at the
/// registered LIFT alone there is 2.500 mm of gap, and a 1.250 mm bore needs
/// 3.000 mm of passage; NO support field can make that drain, which would make
/// a slab fixture unwinnable and the keep-out untestable on it. The table top
/// is 12 mm up.
fn flat_underside_slab() -> plinth_mesh::Mesh {
    plinth_mesh::make_overhang_table_with([3.0, 3.0, 12.0], [30.0, 30.0, 4.0], 0.0)
        .expect("a table is a valid mesh")
}

/// The registered bore. Read rather than restated, so this test cannot drift
/// from the drill stage.
fn bore_radius_mm() -> f64 {
    plinth_core::m0_constants()
        .expect("registry")
        .placeholders()
        .hollow_drain_r_mm()
        .diagnostic()
        .get()
}

/// A drain through the middle of the flat underside, pointing straight down —
/// the Sphere's own case, with the sphere's curvature taken out so the fixture
/// tests the support field and nothing else.
fn downward_drain() -> DrainSpec {
    let radius = bore_radius_mm();
    DrainSpec::from_bore([0.0, 0.0, 14.0], [0.0, 0.0, 12.0], radius)
}

#[test]
fn a_drain_opening_into_free_air_is_connected() {
    // Out of the side of the slab, half way up, pointing away from the model:
    // nothing but air in that direction, and no arm is anywhere near it.
    let radius = bore_radius_mm();
    let sideways = DrainSpec::from_bore([13.0, 0.0, 14.0], [15.0, 0.0, 14.0], radius);
    let policy = SweepPolicy::from_registry().expect("registry");
    let plan = harness::sweep_with_drains(&flat_underside_slab(), &policy, &[sideways]);
    let escape = &plan.report.escape;
    assert_eq!(escape.verdicts.len(), 1, "one drain in, one verdict out");
    assert!(
        escape.verdicts[0].probed_voxels > 0,
        "the probe must have looked at something: {:?}",
        escape.verdicts[0]
    );
    assert!(
        escape.verdicts[0].connected,
        "a drain opening sideways into free air must read connected: {:?}",
        escape.verdicts[0]
    );
}

/// SABOTAGE. `without_drain_keep_out` deletes the placement change this lane
/// made and leaves everything else — the same mesh, the same pose, the same
/// reach, the same area law, the same escape check. If the field still reads
/// connected, the keep-out is decorative.
#[test]
fn deleting_the_keep_out_walls_the_outlet_in() {
    let drain = downward_drain();
    let sabotaged = SweepPolicy::from_registry()
        .expect("registry")
        .without_drain_keep_out();
    let plan = harness::sweep_with_drains(&flat_underside_slab(), &sabotaged, &[drain]);
    let escape = &plan.report.escape;
    assert_eq!(
        plan.report.drain_keep_out_cells, 0,
        "the sabotage must actually have removed the keep-out"
    );
    assert_eq!(
        plan.report.drain_keep_out_refusals, 0,
        "a deleted keep-out cannot refuse anything"
    );
    assert!(
        !plan.struts.is_empty(),
        "the fixture must actually get a support field, or the test proves nothing"
    );
    assert_eq!(escape.verdicts.len(), 1);
    assert!(
        escape.verdicts[0].probed_voxels > 0,
        "the probe must have looked at something: {:?}",
        escape.verdicts[0]
    );
    assert!(
        !escape.verdicts[0].connected,
        "with the keep-out deleted the hex field must close this mouth; if it does not, the \
         fixture is not adversarial and the test below proves nothing: {:?}",
        escape.verdicts[0]
    );
}

/// The keep-out's own claim, and ONLY its own claim: the mouth is clear.
///
/// It does not claim the drain then vents. Under an overhang the resin still
/// has to cross the arm field sideways, and at the registered spacing arms `s`
/// apart with radius `a` leave a free radius of `s/2 - a` — below what the bore
/// needs. Clearing a channel through that field was built and measured on this
/// branch and cost the Sphere 37 of its 139 arms and 3199 mm² of residual
/// demand, so it is not shipped (see `drain::DrainKeepOut::build`). What this
/// asserts is the part that IS true and IS the owner's stated requirement — the
/// outlet is not walled in "to a limited distance" — plus the fact that the
/// remaining shortfall is measured and published rather than hidden.
#[test]
fn the_keep_out_clears_the_mouth_and_the_rest_is_reported() {
    let drain = downward_drain();
    let policy = SweepPolicy::from_registry().expect("registry");
    let mesh = flat_underside_slab();
    let kept = harness::sweep_with_drains(&mesh, &policy, &[drain]);
    let sabotaged = harness::sweep_with_drains(
        &mesh,
        &policy.without_drain_keep_out(),
        &[drain],
    );
    assert!(
        kept.report.drain_keep_out_cells > 0,
        "the keep-out must cover working cells on a model that has a drain"
    );
    assert!(
        kept.report.drain_keep_out_refusals > 0,
        "the keep-out must actually have refused candidates -- otherwise this run and the \
         sabotage run differ in nothing but a flag"
    );
    let with = kept.report.escape.verdicts[0];
    let without = sabotaged.report.escape.verdicts[0];
    assert!(
        without.best_clearance_mm < without.required_clearance_mm,
        "without the keep-out the mouth must be walled in, or this fixture proves nothing: \
         {without:?}"
    );
    assert!(
        with.best_clearance_mm >= with.required_clearance_mm,
        "with the keep-out the mouth must be at least a bore wide: {with:?}"
    );
    assert!(
        with.best_clearance_mm > without.best_clearance_mm,
        "the keep-out must MEASURABLY open the mouth: {with:?} against {without:?}"
    );
    // And the part it does not fix is a published warning, not a silence.
    assert!(
        !with.connected,
        "this fixture is the un-fixed case on purpose; if the field now vents, the arm-field \
         crossing has changed and this test's premise needs re-measuring: {with:?}"
    );
    assert!(
        kept.report
            .render_disclosure()
            .contains("ENCLOSED"),
        "an enclosed drain must be named in the disclosure the emitted file carries"
    );
}

/// A model with no drains must be untouched: no keep-out, no verdicts, and the
/// same field the sweep placed before this lane existed.
#[test]
fn a_model_with_no_drains_is_placed_exactly_as_before() {
    let policy = SweepPolicy::from_registry().expect("registry");
    let mesh = flat_underside_slab();
    let with_none = harness::sweep_with_drains(&mesh, &policy, &[]);
    let plain = harness::sweep_with(&mesh, 0.0, &policy);
    assert_eq!(with_none.report.drain_keep_out_cells, 0);
    assert!(with_none.report.escape.verdicts.is_empty());
    assert_eq!(
        with_none.report.sweep_digest, plain.report.sweep_digest,
        "an empty drain list must not move a single arm"
    );
    assert_eq!(with_none.struts.len(), plain.struts.len());
}

/// Not an assertion — a printout, so the two failures above can be diagnosed
/// from measurements instead of guesses.
#[test]
#[ignore = "diagnostic printout, run explicitly"]
fn probe() {
    let policy = SweepPolicy::from_registry().expect("registry");
    let mesh = flat_underside_slab();
    let (lo, hi) = harness::z_span(&mesh);
    println!("mesh z span {lo} .. {hi}");
    let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for v in mesh.verts() { x0 = x0.min(v[0]); x1 = x1.max(v[0]); y0 = y0.min(v[1]); y1 = y1.max(v[1]); }
    println!("mesh x {x0}..{x1} y {y0}..{y1}");
    let plan = harness::sweep_with_drains(
        &mesh,
        &policy.without_drain_keep_out(),
        &[downward_drain(), DrainSpec::from_bore([13.0, 0.0, 14.0], [15.0, 0.0, 14.0], 1.25)],
    );
    let e = &plan.report.escape;
    println!(
        "arms {} voxels {} solid {} passable {} escaping {} pitch {}",
        plan.struts.len(), e.voxels, e.solid_voxels, e.passable_voxels, e.escaping_voxels,
        e.voxel_pitch_mm
    );
    for v in &e.verdicts {
        println!("{v:?}");
    }
    let mut radii: Vec<f64> = plan.struts.iter().map(plinth_sweep::SweepStrut::tip_radius_mm).collect();
    radii.sort_by(f64::total_cmp);
    println!("tip radii min {:?} med {:?} max {:?}", radii.first(), radii.get(radii.len()/2), radii.last());
}
