//! The end-to-end hollow chain, driven on the committed synthetic fixtures.
//!
//! **The gap this closes.** Before this file no test drove a model through the
//! whole chain. `crates/plinth-hollow/src/drains.rs:3079` was the last
//! assertion in the crate's own tests, and the refill, the audit rescue and
//! the `TrappedCavity` fixpoint check are all downstream of it. The two
//! regressions that did name the later stages were guarded only by `#[ignore]`d
//! tests reading STLs that are not in git
//! (`crates/plinth-hollow/src/lib.rs:4868`,
//! `crates/plinth-decimate/tests/local_minis_repair.rs:79`), so on a clean
//! checkout they guarded nothing at all.
//!
//! Everything here reads only files committed under `tests/fixtures/`, resolved
//! from `CARGO_MANIFEST_DIR`. No corpus, no absolute path, no home directory.
//!
//! **Which entry points.** [`plinth_hollow::run`] is the production stage
//! entry, and it cannot be called from outside the crate tree: it takes a
//! `plinth_orient::OrientedModel`, which has no public constructor — the only
//! ways to obtain one are a pinned baseline report or a dense orientation
//! sweep, and the sweep would rotate these fixtures out of the deliberate
//! plate-down pose their geometry is dimensioned for. So this file calls the
//! four production functions `run` itself calls, in `run`'s own order and with
//! `run`'s own limits:
//!
//! 1. [`create_shell_recording_repair`] under [`H0AdmissionLimits::production`]
//!    — cavity selection,
//! 2. [`drains::plan_drains`] — drain planning,
//! 3. [`drilling::drill_planned_shell`] — drilling, audit rescue and refill,
//! 4. [`drilling::HollowedAndDrilled::into_hollowed_model`] — the shipped
//!    [`HollowReport`].
//!
//! Nothing between those calls is re-implemented here: `run`'s remaining work
//! is the printer-profile check and the `HollowedWithSolidReserve` wrapper,
//! neither of which touches the geometry. What this file does **not** cover is
//! therefore exactly that wrapper and the orientation stage above it.
//!
//! **Expected values are never recomputed from production logic.** Where a
//! test needs a number it comes from one of three places: the fixture's own
//! stated geometry (`tests/fixtures/PROVENANCE.md` records the boxes each
//! fixture was built from, in millimetres), an analytic reference derived
//! from that geometry, or the M0 constants registry read live. No test here
//! re-derives what the engine derives.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use plinth_core::m0_constants;
use plinth_decimate::{run as run_decimation, DecimationAudit, DecimationBudget, ProbeConfig};
use plinth_hollow::{
    create_shell_recording_repair, drains, drilling, registered_minimum_wall_mm, DrainRecord,
    H0AdmissionLimits, HollowDisposition, HollowError, HollowReport, HollowSettings,
};
use plinth_mesh::{load_binary_stl, make_box, Mesh, WeldPolicy};
use plinth_orient::{
    BaselineSelection, OrientationHistory, OrientationProvenance, OrientationReport, RigidTransform,
};

// ---------------------------------------------------------------------------
// Fixture geometry, restated from `tests/fixtures/PROVENANCE.md`.
//
// These are the fixture's OWN dimensions, in millimetres, as recorded in that
// document. They are inputs to this test, not expected outputs of the
// engine: an assertion built on them says "a drain must be inside this box of
// space", which is a claim about the model, not a paraphrase of the planner.
// ---------------------------------------------------------------------------

/// `legs-uneven.stl`: the leg that reaches the plate, `box(12, 12, 40, -12, 0, 20)`.
const LONG_LEG_X_MM: (f64, f64) = (-18.0, -6.0);
const LONG_LEG_Z_MM: (f64, f64) = (0.0, 40.0);
/// `legs-uneven.stl`: the leg that stops partway up, `box(12, 12, 15, 12, 0, 32.5)`.
const SHORT_LEG_X_MM: (f64, f64) = (6.0, 18.0);
const SHORT_LEG_Z_MM: (f64, f64) = (25.0, 40.0);

/// `cube.stl`: the owner's supplied 20 mm cube, sitting on the plate.
const CUBE_SIDE_MM: f64 = 20.0;

// ---------------------------------------------------------------------------
// Chain plumbing
// ---------------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_mesh(name: &str) -> Mesh {
    let path = fixtures_dir().join(name);
    load_binary_stl(&path, WeldPolicy::None)
        .unwrap_or_else(|error| panic!("committed fixture {}: {error}", path.display()))
}

fn registered_settings() -> HollowSettings {
    HollowSettings::from_registered_constants().expect("registered M0 hollow settings")
}

/// A minimal, real [`DecimationAudit`], produced by running the decimation
/// stage on a box. `into_hollowed_model` needs an [`OrientationHistory`] to
/// build the shipped report and there is no hand-built stand-in for one; this
/// is the same construction the crate's own tests and
/// `plinth-support`'s integration test use.
fn fixture_orientation_history() -> OrientationHistory {
    let mesh = make_box([10.0; 3], [0.0; 3]).expect("closed audit fixture");
    let budget =
        DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
            .expect("registered fixture decimation budget");
    let audit: DecimationAudit = run_decimation(&mesh, budget)
        .expect("fixture decimation succeeds")
        .into_decimated_model()
        .expect("fixture scale is in contract")
        .into_parts()
        .1;
    OrientationHistory::from_reports(
        audit,
        OrientationReport {
            provenance: OrientationProvenance::Pinned {
                pinned_report: PathBuf::from("tests/fixtures/PROVENANCE.md"),
            },
            selection: BaselineSelection::Identity,
            transform: RigidTransform::identity(),
        },
    )
}

/// Where in the chain a model stopped, when it did not reach a shipped report.
#[derive(Debug)]
enum ChainStop {
    CavitySelection(HollowError),
    DrainPlanning(drains::DrainPlanningRefusal),
    Drilling(drilling::DrillingRefusal),
}

impl std::fmt::Display for ChainStop {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CavitySelection(error) => write!(formatter, "cavity selection refused: {error}"),
            Self::DrainPlanning(reason) => write!(formatter, "drain planning refused: {reason:?}"),
            Self::Drilling(reason) => write!(formatter, "drilling refused: {reason:?}"),
        }
    }
}

/// What one complete run of the chain produced.
///
/// The whole point of keeping these together is that the interesting claims
/// are *joins*: "the planner found two drains AND nothing was refilled AND the
/// shipped report names both". A test that could only see one stage could not
/// make them.
struct ChainOutcome {
    /// Closed cavity surfaces cavity selection retained, before any drilling.
    cavity_components: usize,
    /// The shipped report, exactly as `run` hands it to support planning. It
    /// carries the planner's own plan (`drain_plan()`, planned holes only) and
    /// the drilling proof (`drilling_evidence()`, planned **and** audit holes).
    report: HollowReport,
}

impl ChainOutcome {
    fn evidence(&self) -> &drilling::DrillingEvidence {
        self.report
            .drilling_evidence()
            .expect("a drilled report carries its drilling evidence")
    }

    fn plan(&self) -> &drains::DrainPlan {
        self.report
            .drain_plan()
            .expect("a drilled report carries the planner's plan")
    }

    /// Drains the **planner** committed to, before drilling's audit rescue.
    fn planned_drains(&self) -> Vec<&drains::HoleGeometry> {
        self.plan()
            .holes()
            .iter()
            .filter(|hole| hole.role == drains::HoleRole::Drain)
            .collect()
    }

    /// Every drain in the drilled article, whatever put it there.
    fn drilled_drains(&self) -> Vec<&drilling::DrilledHoleEvidence> {
        self.evidence()
            .holes()
            .iter()
            .filter(|hole| hole.role() == drains::HoleRole::Drain)
            .collect()
    }
}

/// The real chain, in `run`'s order, under `run`'s limits.
fn run_chain(mesh: Mesh) -> Result<ChainOutcome, ChainStop> {
    let settings = registered_settings();
    let candidate = create_shell_recording_repair(mesh, settings, H0AdmissionLimits::production())
        .map_err(|failure| ChainStop::CavitySelection(failure.into_error()))?;
    let cavity_components = candidate.cavity().cavity_components();
    let planned = drains::plan_drains(candidate)
        .map_err(|failure| ChainStop::DrainPlanning(failure.into_reason()))?;
    let drilled = drilling::drill_planned_shell(planned)
        .map_err(|failure| ChainStop::Drilling(failure.into_reason()))?;
    let model = drilled.into_hollowed_model(fixture_orientation_history());
    // The shipped report is read off the model support planning would receive,
    // not off the drilling result directly, so anything `into_hollowed_model`
    // drops on the way is dropped here too.
    let report = model.history().hollow().clone();
    assert_eq!(
        model
            .cavity_evidence()
            .expect("a hollowed model carries its cavity evidence")
            .cavity_components(),
        cavity_components,
        "the model's carried cavity evidence disagrees with the shell it was built from"
    );
    Ok(ChainOutcome {
        cavity_components,
        report,
    })
}

fn run_chain_or_panic(name: &str) -> ChainOutcome {
    run_chain(fixture_mesh(name)).unwrap_or_else(|stop| panic!("{name} did not ship: {stop}"))
}

/// `true` when `point` lies inside the axis-aligned `x`/`z` window, with `y`
/// ignored. The fixtures are all prisms in `y`, so `x` and `z` alone name a
/// limb unambiguously.
fn inside_xz(point: [f64; 3], x_mm: (f64, f64), z_mm: (f64, f64)) -> bool {
    point[0] >= x_mm.0 && point[0] <= x_mm.1 && point[2] >= z_mm.0 && point[2] <= z_mm.1
}

fn hole_centre(hole: &drilling::DrilledHoleEvidence) -> [f64; 3] {
    hole.center_mm().map(plinth_core::Finite::get)
}

fn planned_centre(hole: &drains::HoleGeometry) -> [f64; 3] {
    hole.center_mm.map(plinth_core::Finite::get)
}

// ---------------------------------------------------------------------------
// Coverage guard
// ---------------------------------------------------------------------------

/// Every committed fixture is either driven through the chain below or is
/// named here with the reason it is not. A fixture added to the directory
/// without a decision recorded fails this test rather than being silently
/// uncovered — which is the failure mode that produced the gap this file
/// exists to close.
const FIXTURES_DRIVEN_THROUGH_THE_CHAIN: &[&str] =
    &["cube.stl", "legs-uneven.stl", "cascade-pocket.stl"];

/// `thin-fin.stl`'s intended consumer is `geom_kernel`'s near-wall-skip breach
/// predicate, not this crate: its 0.02 mm fin is 25x below the registered
/// `HOLLOW_PITCH_MM`, so H0's surface-nets reconstruction of the carved cavity
/// comes back with a spurious extra closed surface and drilling refuses with
/// `SourceSurfaceCount { found: 3, expected: 2 }`. That refusal is its own
/// finding — see `thin_fin_bulk_should_reach_a_drilled_shell` below, which
/// states the correct outcome and is ignored until it holds.
const FIXTURES_NOT_HOLLOW_CHAIN_CASES: &[&str] = &["thin-fin.stl"];

#[test]
fn every_committed_fixture_has_a_recorded_decision() {
    let mut on_disk = BTreeSet::new();
    for entry in std::fs::read_dir(fixtures_dir()).expect("fixtures directory") {
        let entry = entry.expect("fixtures directory entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".stl") {
            on_disk.insert(name);
        }
    }
    let decided: BTreeSet<String> = FIXTURES_DRIVEN_THROUGH_THE_CHAIN
        .iter()
        .chain(FIXTURES_NOT_HOLLOW_CHAIN_CASES)
        .map(|name| (*name).to_owned())
        .collect();
    assert_eq!(
        on_disk, decided,
        "a committed fixture has no recorded decision (or a recorded decision names a fixture \
         that is gone); add it to FIXTURES_DRIVEN_THROUGH_THE_CHAIN or state why it is not a \
         hollow-chain case"
    );
}

// ---------------------------------------------------------------------------
// cube.stl -- "the chain runs"
// ---------------------------------------------------------------------------

/// The baseline case: a 20 mm cube goes in, a drilled hollow shell comes out.
///
/// One chamber, at least one drain, and a realized wall at or above the
/// registered hard minimum. The cavity is checked against the cube's own
/// analytic erosion — side minus twice `wall + cavity_march_margin`, both read
/// live off the settings — rather than against a number copied out of a
/// previous run.
#[test]
fn cube_hollows_with_one_chamber_a_drain_and_a_legal_wall() {
    let settings = registered_settings();
    let outcome = run_chain_or_panic("cube.stl");

    assert!(
        matches!(
            outcome.report.disposition(),
            HollowDisposition::HollowedAndDrilled
        ),
        "cube.stl must ship hollow, got {:?}",
        outcome.report.disposition()
    );
    assert_eq!(
        outcome.cavity_components, 1,
        "a solid cube has exactly one interior"
    );
    assert!(
        !outcome.drilled_drains().is_empty(),
        "a shipped hollow with no drain is a sealed bag of resin: {:?}",
        outcome.evidence().holes()
    );

    // FU-55: the shell measured its own realized wall, and the hard minimum is
    // read from the registry, never restated.
    let minimum_wall_mm = registered_minimum_wall_mm()
        .expect("registered HOLLOW_WALL_MIN_MM")
        .get();
    let realized = outcome
        .report
        .realized_wall()
        .expect("a hollowed report carries its realized wall");
    assert!(
        realized.wall_mm().get() >= minimum_wall_mm,
        "realized wall {realized:?} is under the registered {minimum_wall_mm} mm minimum"
    );

    // **The cavity, bracketed from the cube's own geometry.** Cavity
    // eligibility is `edt > wall + margin` sampled on a lattice quantised at
    // the frozen `HOLLOW_PITCH_MM`, so the erosion the cube actually receives
    // on each face is strictly more than that threshold and at most one pitch
    // more — it cannot be less (the wall would be thinner than asked) and it
    // cannot be more (the lattice would have thrown away more than the one
    // voxel layer quantisation can account for). Both ends are the fixture's
    // own side length and the live registry; neither is a figure copied out of
    // a previous run.
    let threshold_mm = settings.wall_thickness_mm().get() + settings.cavity_march_margin_mm().get();
    let pitch_mm = settings.voxel_pitch_mm().get();
    let largest_mm3 = (CUBE_SIDE_MM - 2.0 * threshold_mm).powi(3);
    let smallest_mm3 = (CUBE_SIDE_MM - 2.0 * (threshold_mm + pitch_mm)).powi(3);
    let shipped_mm3 = outcome.evidence().final_cavity_mm3().get();
    assert!(
        shipped_mm3 > smallest_mm3 && shipped_mm3 <= largest_mm3,
        "shipped cavity {shipped_mm3} mm3 is outside the cube's own bracket \
         ({smallest_mm3}, {largest_mm3}] mm3 -- a {CUBE_SIDE_MM} mm cube eroded per face by \
         more than the {threshold_mm} mm threshold and by at most one {pitch_mm} mm pitch more"
    );

    // And the shipped cavity has to agree with the wall the shell says it
    // realized: a report claiming one wall while carrying a cavity sized for
    // another is internally inconsistent, however plausible either number
    // looks on its own.
    let realized_mm3 = (CUBE_SIDE_MM - 2.0 * realized.wall_mm().get()).powi(3);
    assert!(
        (shipped_mm3 - realized_mm3).abs() / realized_mm3 < 0.02,
        "shipped cavity {shipped_mm3} mm3 does not match the {realized:?} the same report \
         claims, which for a {CUBE_SIDE_MM} mm cube is {realized_mm3} mm3"
    );

    // Nothing was given up, and everything that shipped drains.
    assert_eq!(outcome.evidence().refilled_voxels(), 0);
    assert!(outcome.evidence().refilled_pockets().is_empty());
    assert_eq!(
        outcome.evidence().drainable_voxels(),
        outcome.evidence().cavity_voxels(),
        "every shipped cavity voxel must drain"
    );

    // And the report a downstream reader actually gets names the drains.
    assert_eq!(
        outcome.report.drains().len(),
        outcome.drilled_drains().len(),
        "the shipped report must list every drain the drilling proof records"
    );
    assert_eq!(
        outcome.report.cavities().len(),
        1,
        "one chamber, one cavity record"
    );
}

// ---------------------------------------------------------------------------
// legs-uneven.stl -- the drain-banding case
// ---------------------------------------------------------------------------

/// **What must stay true**, whatever the drain-banding fix does: both legs of
/// `legs-uneven.stl` ship hollow, with a drain in each, and no part of the
/// cavity is refilled back to material.
///
/// This asserts the *outcome* and deliberately not the mechanism. Today the
/// short leg's drain comes from drilling's audit rescue rather than from drain
/// planning; after the short-list-3 fix it should come from planning. Either
/// way the drain has to be inside the short leg, so this test survives the fix
/// and would fail the moment the leg were dropped.
#[test]
fn legs_uneven_ships_both_legs_hollow_with_a_drain_in_each() {
    let outcome = run_chain_or_panic("legs-uneven.stl");

    assert!(
        matches!(
            outcome.report.disposition(),
            HollowDisposition::HollowedAndDrilled
        ),
        "legs-uneven.stl must ship hollow, got {:?}",
        outcome.report.disposition()
    );

    let drains: Vec<[f64; 3]> = outcome
        .drilled_drains()
        .into_iter()
        .map(hole_centre)
        .collect();
    assert!(
        drains
            .iter()
            .any(|centre| inside_xz(*centre, LONG_LEG_X_MM, LONG_LEG_Z_MM)),
        "no drain inside the long leg (x {LONG_LEG_X_MM:?}, z {LONG_LEG_Z_MM:?}): {drains:?}"
    );
    assert!(
        drains
            .iter()
            .any(|centre| inside_xz(*centre, SHORT_LEG_X_MM, SHORT_LEG_Z_MM)),
        "no drain inside the short leg (x {SHORT_LEG_X_MM:?}, z {SHORT_LEG_Z_MM:?}): the short \
         leg's cavity would ship as a sealed pocket or be refilled solid: {drains:?}"
    );

    assert_eq!(
        outcome.evidence().refilled_voxels(),
        0,
        "no part of this model may be given back to material: {:?}",
        outcome.evidence().refilled_pockets()
    );
    assert!(outcome.evidence().refilled_pockets().is_empty());
    assert_eq!(
        outcome.evidence().drainable_voxels(),
        outcome.evidence().cavity_voxels(),
        "every shipped cavity voxel must drain"
    );
}

/// **The correct behaviour the short-list-3 fix must deliver.**
///
/// The two legs are one 6-connected cavity chamber, and `drains.rs`'s
/// `low_lobes` admits planned-drain candidates only within `HOLLOW_LOBE_BAND_MM`
/// of that single chamber's one global low point — the long leg's cavity
/// bottom, near `z = 2.5`. The short leg's own cavity bottom sits near
/// `z = 27.5`, far outside that band, so **drain planning never offers it a
/// candidate**: measured today, `plan_drains` commits to exactly one drain,
/// down the long leg.
///
/// The model still ships hollow only because drilling's audit rescue —
/// a safety net for pockets the planner could not foresee — notices the
/// trapped limb and drills it a dedicated hole. That is the net doing the
/// planner's job. A limb one size smaller, or one with no clear exit inside
/// `HOLLOW_EXTRA_DRAIN_POLICY.exit_max_mm`, gets no such reprieve and is
/// refilled solid.
///
/// So the claim below is the one the fix has to make true: the *planner*
/// finds both legs, and the rescue has nothing left to do.
#[test]
#[ignore = "fails until the drain-banding fix in REVIEW.md short-list 3"]
fn legs_uneven_drain_planning_should_reach_both_legs_without_the_audit_rescue() {
    let outcome = run_chain_or_panic("legs-uneven.stl");

    let planned: Vec<[f64; 3]> = outcome
        .planned_drains()
        .into_iter()
        .map(planned_centre)
        .collect();
    assert_eq!(
        planned.len(),
        2,
        "drain planning must give each leg its own drain, not band the whole chamber at the \
         model's single low point: {planned:?}"
    );
    assert!(
        planned
            .iter()
            .any(|centre| inside_xz(*centre, LONG_LEG_X_MM, LONG_LEG_Z_MM)),
        "no planned drain inside the long leg: {planned:?}"
    );
    assert!(
        planned
            .iter()
            .any(|centre| inside_xz(*centre, SHORT_LEG_X_MM, SHORT_LEG_Z_MM)),
        "no planned drain inside the short leg -- this is the banding gap itself: {planned:?}"
    );

    // The rescue is a net, not a mechanism. Once planning sees both legs there
    // is nothing for it to catch on this fixture.
    assert_eq!(
        outcome.evidence().audit_added_drains(),
        0,
        "the audit rescue is still carrying the planner: {:?}",
        outcome.evidence().holes()
    );
}

// ---------------------------------------------------------------------------
// cascade-pocket.stl -- the refill and the fixpoint check
// ---------------------------------------------------------------------------

/// The branched case: a spine with two side chambers hung from their own tops.
/// One is rescued by the audit pass, the other is under the registered minimum
/// pocket volume and is refilled back to material.
///
/// This is the only committed fixture that reaches drilling's refill path *and*
/// its `TrappedCavity` fixpoint demand (`drilling.rs` re-derives drainability
/// against the final hole set and refill mask and requires the count to match
/// exactly). Reaching a shipped report at all is therefore itself the
/// assertion the review asked for; the rest checks that what was given up was
/// only what the registered policy says cannot be rescued.
#[test]
fn cascade_pocket_ships_with_only_the_unrescuable_pocket_refilled() {
    let outcome = run_chain_or_panic("cascade-pocket.stl");

    assert!(
        matches!(
            outcome.report.disposition(),
            HollowDisposition::HollowedAndDrilled
        ),
        "cascade-pocket.stl must ship hollow, got {:?}",
        outcome.report.disposition()
    );

    // Everything retained drains. This is the `TrappedCavity` invariant seen
    // from the outside: drilling would have refused rather than shipped a
    // pocket that does not drain.
    assert_eq!(
        outcome.evidence().drainable_voxels(),
        outcome.evidence().cavity_voxels(),
        "a shipped voxel that does not drain is trapped resin"
    );

    // Whatever was refilled must be something the registered policy refuses to
    // rescue on size. The threshold is read live; nothing here restates it.
    let minimum_pocket_ml = m0_constants()
        .expect("M0 constants")
        .placeholders()
        .hollow_extra_drain_policy()
        .diagnostic()
        .min_pocket_ml();
    for pocket in outcome.evidence().refilled_pockets() {
        assert!(
            pocket.volume_ml().get() < minimum_pocket_ml,
            "a pocket of {} mL was refilled although the registered policy would rescue \
             anything at or above {minimum_pocket_ml} mL -- the rescue pass gave up on \
             something it was supposed to drill",
            pocket.volume_ml().get()
        );
    }

    // The refill is accounted for: what shipped plus what was handed back is
    // what the engine's own volume balance says was removed, to within the
    // tolerance the engine states for itself.
    let residual = outcome.evidence().volume_balance_residual_mm3().get().abs();
    let tolerance = outcome.evidence().volume_balance_tolerance_mm3().get();
    assert!(
        residual <= tolerance,
        "volume balance residual {residual} mm3 exceeds the stated tolerance {tolerance} mm3"
    );
}

// ---------------------------------------------------------------------------
// thin-fin.stl -- a live finding, not a hollow-chain case yet
// ---------------------------------------------------------------------------

/// **A finding, recorded as a failing claim rather than left unguarded.**
///
/// `thin-fin.stl` is a 20 mm cube — arithmetically identical to `cube.stl`,
/// which hollows and drills cleanly above — carrying a 0.02 mm fin, half of
/// `NEAR_WALL_SKIP`, for `geom_kernel`'s breach predicate. The bulk is a
/// genuine hollowing target and cavity selection and drain planning both
/// succeed on it; drilling then refuses with
/// `SourceSurfaceCount { found: 3, expected: 2 }`, because at the registered
/// 0.5 mm `HOLLOW_PITCH_MM` the sub-voxel fin makes H0's surface-nets
/// reconstruction produce a spurious third closed surface.
///
/// A feature finer than the voxel pitch must not be able to make the engine
/// refuse a model it would otherwise hollow. This states that; it is ignored
/// because it does not hold today.
#[test]
#[ignore = "fails until a sub-voxel-pitch feature stops breaking H0's cavity reconstruction (SourceSurfaceCount { found: 3, expected: 2 })"]
fn thin_fin_bulk_should_reach_a_drilled_shell() {
    let outcome = run_chain_or_panic("thin-fin.stl");
    assert!(matches!(
        outcome.report.disposition(),
        HollowDisposition::HollowedAndDrilled
    ));
}

// ---------------------------------------------------------------------------
// The report's own diagnostic: telling chambers apart
// ---------------------------------------------------------------------------

/// Two 24 mm cubes joined by a slim bridge: one closed body whose interior is
/// **two** separate cavity chambers, each earning its own drain and its own
/// vent, with nothing refilled.
///
/// None of the committed STL fixtures produces two chambers that both ship —
/// `cube.stl` and `thin-fin.stl` have one interior each, and `legs-uneven.stl`
/// and `cascade-pocket.stl` are each a single 6-connected chamber however many
/// limbs they have — so the report's per-chamber bookkeeping cannot be
/// witnessed on them. This is the smallest model that witnesses it. The union
/// is done by the same Manifold backend the engine itself uses; nothing about
/// the hollow chain is re-implemented here.
fn two_chamber_bar() -> Mesh {
    use plinth_solid::{BooleanLimits, ManifoldBackend, Mesh as SolidMesh};

    // Two cubes 32 mm apart with the second lifted 20 mm, joined by a 4 mm bar
    // that reaches 1 mm inside each. Two solids meeting on a coincident plane
    // are not reliably welded by a union, so the bridge overlaps.
    let parts = [
        make_box([24.0; 3], [0.0, 0.0, 0.0]).expect("first chamber"),
        make_box([24.0; 3], [32.0, 0.0, 20.0]).expect("second chamber"),
        make_box([12.0, 4.0, 4.0], [16.0, 0.0, 19.0]).expect("bridge"),
    ];
    let operands: Vec<SolidMesh> = parts
        .iter()
        .map(|mesh| {
            SolidMesh::new(
                mesh.verts().to_vec(),
                mesh.tris().iter().map(|tri| tri.map(u64::from)).collect(),
            )
            .expect("boolean operand")
        })
        .collect();
    let welded = ManifoldBackend
        .union_limited(&operands, BooleanLimits::UNLIMITED)
        .expect("fixture union");
    let low = welded.vertices().iter().fold([f64::MAX; 3], |low, vertex| {
        [
            low[0].min(vertex[0]),
            low[1].min(vertex[1]),
            low[2].min(vertex[2]),
        ]
    });
    Mesh::new(
        welded
            .vertices()
            .iter()
            .map(|vertex| [vertex[0] - low[0], vertex[1] - low[1], vertex[2] - low[2]])
            .collect(),
        welded
            .triangles()
            .iter()
            .map(|tri| {
                [
                    u32::try_from(tri[0]).expect("fixture index"),
                    u32::try_from(tri[1]).expect("fixture index"),
                    u32::try_from(tri[2]).expect("fixture index"),
                ]
            })
            .collect(),
    )
    .expect("welded fixture in the positive octant")
}

/// The engine keeps the two chambers apart all the way to drilling — the
/// planner attributes one drain to each lobe and neither is refilled — so this
/// establishes that the *information* exists before the report is built.
#[test]
fn the_engine_keeps_the_two_chambers_apart_through_drilling() {
    let outcome = run_chain(two_chamber_bar()).expect("the two-chamber bar ships");
    assert_eq!(
        outcome.cavity_components, 2,
        "the bridged bar must select two chambers"
    );
    let lobes: BTreeSet<Option<u32>> = outcome
        .planned_drains()
        .into_iter()
        .map(|hole| hole.lobe_id)
        .collect();
    assert_eq!(
        lobes.len(),
        2,
        "the planner must attribute one drain to each chamber: {lobes:?}"
    );
    assert_eq!(outcome.evidence().refilled_voxels(), 0);
}

/// **The diagnostic the shipped report owes a reader.** Two chambers, two
/// drains, and a report that can say which drain serves which chamber.
///
/// `drilling.rs`'s `into_hollowed_model` writes a literal `cavity_id: 1` onto
/// every [`DrainRecord`] and emits exactly one [`CavityRecord`], also numbered
/// `1`, carrying the whole model's cavity volume — regardless of how many
/// chambers the engine actually kept apart (which the test above proves it
/// does). So a downstream reader of a two-chamber model sees one chamber with
/// two drains. It cannot tell that apart from a genuinely single-chamber model
/// that happens to carry two drains — and it therefore cannot notice the case
/// that matters, where both drains serve the same chamber and the other one
/// ships sealed.
///
/// [`CavityRecord`]: plinth_hollow::CavityRecord
#[test]
#[ignore = "fails until drilling.rs:418 stops collapsing every chamber to cavity_id: 1"]
fn the_shipped_report_can_tell_the_two_chambers_apart() {
    let outcome = run_chain(two_chamber_bar()).expect("the two-chamber bar ships");
    assert_eq!(outcome.cavity_components, 2);

    assert_eq!(
        outcome.report.cavities().len(),
        2,
        "the report must carry one cavity record per shipped chamber: {:?}",
        outcome.report.cavities()
    );
    let cavity_ids: BTreeSet<u32> = outcome
        .report
        .cavities()
        .iter()
        .map(|cavity| cavity.cavity_id)
        .collect();
    assert_eq!(
        cavity_ids.len(),
        2,
        "the two cavity records must carry distinct ids: {cavity_ids:?}"
    );

    let drain_cavity_ids: BTreeSet<u32> = outcome
        .report
        .drains()
        .iter()
        .map(|drain: &DrainRecord| drain.cavity_id)
        .collect();
    assert_eq!(
        drain_cavity_ids, cavity_ids,
        "each drain must name the chamber it serves; every drain records cavity_id 1 today"
    );
}
