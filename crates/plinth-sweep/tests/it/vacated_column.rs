//! **A LEANED ARM MUST STILL CARRY THE SEAT IT WAS PLACED TO ROOT.**
//!
//! The up-sweep roots a cell by putting an arm's TIP in that cell's column. The
//! down-sweep routes that cell's load onto whatever stands one layer lower, and
//! its root test is `cover_above[cell] && carriers[cell]` — the column must
//! both hold a carrier AND lie inside the model's own footprint at the layer
//! being routed. A straight arm satisfies both by construction: it stands in
//! its own seat column, and the seat column is a model cell. **A leaned arm
//! stands somewhere else one layer down**, usually just outside the model's
//! footprint, and the routing field then finds no root at all for the material
//! the arm is physically holding up — an INFINITE gap, not a marginal one.
//!
//! Measured on the Morel base at layer 107 before the fix: 1450 cells lost,
//! 1433 of them rooted in the up-sweep by one of eight leaned arms, every one
//! of which reads `carrier true, cover_above false` one layer under its own
//! seat. The engine printed `residual demand 0 cells` beside
//! `uncarried load 23.2200 mm2` about one field and raised its own "the two
//! sweeps contradict each other" warning.
//!
//! This fixture is that shape in miniature, built from two boxes. The drill is
//! two-way — [`SweepPolicy::without_tip_column_roots`] restores the seat-blind
//! accounting and the same field must lose the island again, with an infinite
//! gap — so a green result here cannot mean "this fixture had nothing to lose".

// `uncarried_load_mm2` is a running sum of the load the routing field failed to
// place. It is EXACTLY zero when the field placed all of it — nothing is
// accumulated, so no rounding can occur — and "no load was lost" is the claim
// being made, not "little load was lost". A tolerance here would let the defect
// back in one lamina at a time.
#![expect(clippy::float_cmp, reason = "zero lost load is exact, not approximate")]

use crate::harness;

use harness::sweep_with;
use plinth_mesh::{make_box, make_overhang_table_with, Mesh};
use plinth_sweep::{SweepPlan, SweepPolicy};

/// Glue disjoint boxes into one mesh. Index-shifted concatenation only; no
/// geometry is computed here.
fn glue(parts: &[Mesh]) -> Mesh {
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for part in parts {
        let base = u32::try_from(verts.len()).expect("a fixture with fewer than 4e9 vertices");
        verts.extend_from_slice(part.verts());
        tris.extend(
            part.tris()
                .iter()
                .map(|tri| [tri[0] + base, tri[1] + base, tri[2] + base]),
        );
    }
    Mesh::new(verts, tris).expect("disjoint boxes are a valid mesh")
}

/// An island in mid-air whose only cheap descent leans off its own footprint.
///
/// Two parts, and every dimension is there for a measured reason:
///
/// * **the shelf** — a plate ONE layer thick, hanging in free air. It is the
///   lowest overhang in the model, so the up-sweep seats arms under it first
///   and they run straight to the plate. Their shafts are what the island's arm
///   merges onto, and one layer of thickness is what lets the descent's path
///   check pass: `up_sweep::lean_path` refuses any path cell with model at or
///   above it, so a thicker shelf would wall its own trunk off from every
///   approach.
/// * **the island** — a small slab hanging well above the shelf with nothing
///   whatever underneath it. A straight descent from its underside is the whole
///   height of the model and a merge onto a shelf arm is a fraction of that, so
///   the one arm the up-sweep puts under it leans WEST and stands in free air
///   one layer down.
///
/// The island is narrow in `x` — the lean's direction — and short enough in `y`
/// for ONE arm to root all of it. Both matter: a lean closes at most one
/// lamina's sideways travel per layer, so the seat has to be within a cell or
/// two of the western edge for the first step to leave the footprint (which is
/// exactly the boundary seat the Morel base produced eight of), and a second,
/// straighter arm anywhere on the island would leave a root behind and turn the
/// island back into a mere reach shortfall.
///
/// Measured on this fixture: 3 arms, the island's one seated at layer 122 and
/// standing one column west of its own seat at 121, `uncarried 0.2100 mm²` with
/// a gap of `inf` under the seat-blind rule and `0` with the rule in place.
fn island_over_a_vacating_lean() -> Mesh {
    let shelf = make_box([0.4, 3.0, 0.05], [-1.8, 0.0, 3.90]).expect("the shelf");
    let island = make_box([0.2, 1.5, 0.6], [-1.1, 0.0, 5.0]).expect("the island");
    glue(&[shelf, island])
}

/// Arms whose axis is in a different column one layer under their own seat —
/// the population this whole rule is about. Counted off the finished field
/// rather than off a counter in the crate under test.
fn arms_vacating_their_seat_column(plan: &SweepPlan) -> usize {
    plan.struts
        .iter()
        .filter(|strut| {
            strut.k_top > strut.k_bottom && strut.cell_at(strut.k_top - 1) != strut.cell
        })
        .count()
}

#[test]
fn a_leaned_arm_carries_the_island_it_roots_and_the_seat_blind_rule_drops_it() {
    let mesh = island_over_a_vacating_lean();
    // Graded at the seat placement the fixture was built for
    // (`harness::at_path_fixture_seating`): at the production placement no arm
    // under the island leans out of its seat column (measured 2026-09-30: 0),
    // so the tip-column rule would be graded on nothing. The rule reads no
    // placement.
    //
    // RE-SCOPED 2026-09-30 (fix/chitubox-geometry), measured: graded WITH THE
    // LIP-SEAT RULE OFF, as the head-join fixtures are. With the owner's
    // ChituBox ball ratio (a smaller ball on every tip) the lip-seat rule,
    // which measures the seat trim's bite into a shaft with the ball the
    // lowering draws, re-lands the island's leaned arm and its neighbour
    // (ids 2 and 1) at a smaller angle to their seats. The island arm then
    // stands nearly straight over the plate, and the seat-blind sabotage loses
    // 0 mm²: the fixture no longer has the defect's shape. With the lip-seat
    // rule off the fixture is exactly the one measured below: seat-blind
    // 0.2100 mm² uncarried at an infinite gap, the rule 0 (44 tip-column roots
    // offered, 0.245 mm² through them). The rule graded here is the
    // down-sweep's accounting, which reads no lip-seat decision.
    //
    // MERGED 2026-10-01 (land of fix/chitubox-geometry onto develop 3372093):
    // the two re-scopings answer two different causes (the seat placement from
    // fix/lattice-align, the ball ratio from fix/chitubox-geometry), and the
    // merged tree has both, so the fixture is graded under both.
    let policy = harness::at_path_fixture_seating(
        SweepPolicy::from_registry()
            .expect("registry")
            .without_lip_seat(),
    );
    let with = sweep_with(&mesh, 0.0, &policy);
    let without = sweep_with(&mesh, 0.0, &policy.without_tip_column_roots());

    // ANTI-VACUITY FIRST. A fixture with no arm that leaves its own seat column
    // proves nothing either way, and neither does a rule that never fires.
    let vacating = arms_vacating_their_seat_column(&with);
    assert!(
        vacating > 0,
        "the fixture must actually lean an arm out of its seat column; it leaned {vacating}"
    );
    assert!(
        with.report.tip_column_roots_offered > 0,
        "the tip-column rule must have run: {} roots offered",
        with.report.tip_column_roots_offered
    );
    assert!(
        with.report.tip_column_root_load_mm2 > 0.0,
        "load must actually have arrived through a tip-column root, or the rule is present and \
         inert: {} mm2",
        with.report.tip_column_root_load_mm2
    );

    // THE CLAIM. Every mm² the up-sweep declared rooted reaches a carrier.
    assert_eq!(
        with.report.uncarried_load_mm2, 0.0,
        "load left uncarried at layer {:?}, worst gap {} mm",
        with.report.uncarried_lowest_layer, with.report.uncarried_worst_gap_mm
    );
    assert!(
        !with.report.sweeps_disagree,
        "the two sweeps must agree: residual demand {} cells beside {} mm2 uncarried",
        with.report.residual_demand_cells, with.report.uncarried_load_mm2
    );

    // THE SABOTAGE. Same fixture, seat-blind accounting: the load the leaned arm
    // holds up reaches no carrier at all.
    assert!(
        without.report.uncarried_load_mm2 > 0.0,
        "the seat-blind rule must lose the island's load, or this fixture is not the defect's \
         shape; it lost {} mm2",
        without.report.uncarried_load_mm2
    );
    assert!(
        without.report.sweeps_disagree,
        "and it must lose it while the up-sweep still calls the island rooted: residual demand {} \
         cells",
        without.report.residual_demand_cells
    );
    // An INFINITE gap, not a marginal one. That is what tells this defect apart
    // from a reach shortfall: the material sits over an island with no carrier
    // anywhere it can walk to, which is exactly what the Morel base printed.
    assert!(
        !without.report.uncarried_worst_gap_mm.is_finite(),
        "the seat-blind rule leaves an ISLAND, so the worst gap is infinite, not {} mm",
        without.report.uncarried_worst_gap_mm
    );
    // AND THE UP-SWEEP NEVER MOVED. Both runs root the same cells, so the whole
    // difference is down-sweep accounting and not a second field.
    assert_eq!(
        with.report.residual_demand_cells,
        without.report.residual_demand_cells
    );
}

#[test]
fn the_rule_fires_on_an_ordinary_overhang_table_without_changing_its_verdict() {
    // The contrived island proves the mechanism. This proves the rule is not a
    // fixture-only path: an overhang table merges arms onto its legs' trunks,
    // so tip columns are offered in bulk — and because that field already
    // carried all of its load, the knob must not change whether it does.
    let mesh =
        make_overhang_table_with([2.0, 2.0, 6.0], [8.0, 8.0, 2.0], 0.0).expect("the table");
    let policy = SweepPolicy::from_registry().expect("registry");
    let with = sweep_with(&mesh, 0.0, &policy);
    let without = sweep_with(&mesh, 0.0, &policy.without_tip_column_roots());

    assert!(
        with.report.tip_column_roots_offered > 0,
        "the rule must bite on a production-shaped fixture, not only on the island"
    );
    assert_eq!(with.report.uncarried_load_mm2, 0.0);
    assert_eq!(without.report.uncarried_load_mm2, 0.0);
    assert!(!with.report.sweeps_disagree);
}
