//! Is a support foot ball really admitted through a sub-`NEAR_WALL_SKIP` wall?
//!
//! It was. `queries.rs`'s `cone_thickness` used to discard every cone crossing
//! closer than `NEAR_WALL_SKIP` (0.05 mm) past the nearest surface distance.
//! The discard exists for a good reason -- the ray's first crossing is the
//! ball's own entry through the face it is already resting on, which is not a
//! far side -- but a DISTANCE cannot tell "this is the near face again" from
//! "this is the far face, and the wall is thin", so it threw away both.
//!
//! The review (short-list item 11) could not settle this: "whether
//! `NEAR_WALL_SKIP` actually admits a foot ball through a thin feature ...
//! needs a constructed geometry satisfying the surrounding precondition and was
//! not built". This file is that geometry. It reproduced the breach, and it now
//! guards the fix.

use geom_kernel::{NativeSurface, NativeSurfaceConfig};

/// Production's foot-breach cone, copied from
/// `crates/plinth-support/src/anchor.rs:31-36`. The predicate is sampled with
/// exactly these, so a witness using a different cone would not be a witness
/// about shipped behaviour.
const FOOT_BREACH_HALF_DEG: f64 = 25.0;
const FOOT_BREACH_CONE_RAYS: usize = 8;

/// An axis-aligned closed box with outward-wound faces.
///
/// The winding is the same 12-triangle table `native_api.rs` uses; only the
/// corner coordinates change, so the outward normals are outward here for the
/// same reason they are there.
fn box_surface(low: [f64; 3], high: [f64; 3]) -> NativeSurface {
    const UNIT: [[f64; 3]; 8] = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ];
    const FACES: [[u32; 3]; 12] = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    let vertices: Vec<[f64; 3]> = UNIT
        .iter()
        .map(|unit| {
            [
                low[0] + unit[0] * (high[0] - low[0]),
                low[1] + unit[1] * (high[1] - low[1]),
                low[2] + unit[2] * (high[2] - low[2]),
            ]
        })
        .collect();
    NativeSurface::from_indexed(
        &vertices,
        &FACES,
        NativeSurfaceConfig {
            grid_pitch_mm: 1.0,
            max_grid_bytes: usize::MAX,
        },
    )
    .expect("axis-aligned box is a valid indexed surface")
}

/// The shared fixture: one 10 mm x 10 mm wall of thickness `wall_mm` standing
/// in the y = 0 plane, and one ball resting `STANDOFF_MM` off its near face on
/// the axis.
///
/// `STANDOFF_MM` is deliberately SMALL. The cone's off-axis rays cross the near
/// wall `standoff * 0.1034` further along the ray than the axis ray does
/// (`1/cos 25 deg - 1`). Those off-axis entry crossings only fall inside a
/// 0.05 mm near-wall skip while `standoff < 0.484 mm`; above that the old code
/// re-found the NEAR wall through the cone and called it the far side, so the
/// ball came out "breaching" for a reason unrelated to the wall's thickness. A
/// witness has to sit where the skip is the thing deciding the outcome.
const STANDOFF_MM: f64 = 0.1;
/// Ball radius. The ball's surface reaches `RADIUS_MM - STANDOFF_MM` = 0.9 mm
/// past the near face.
const RADIUS_MM: f64 = 1.0;
/// The reach precondition's slack, far below `RADIUS_MM - STANDOFF_MM`, so the
/// ball unambiguously reaches past the near wall in every case here.
const EPS_MM: f64 = 0.01;
/// The reach itself: a wall THINNER than this is inside the ball and is a real
/// breach; a wall THICKER than this is not.
const REACH_MM: f64 = RADIUS_MM - STANDOFF_MM;

fn ball_breaches_wall(wall_mm: f64) -> bool {
    let surface = box_surface([-5.0, 0.0, -5.0], [5.0, wall_mm, 5.0]);
    let result = surface
        .sphere_breach_classified(
            &[[0.0, -STANDOFF_MM, 0.0]],
            &[RADIUS_MM],
            EPS_MM,
            FOOT_BREACH_HALF_DEG,
            FOOT_BREACH_CONE_RAYS,
        )
        .expect("one centre, one radius");
    assert_eq!(
        result.near_parallel[0], COSINE_ROUNDING_NEAR_PARALLEL,
        "the near-parallel tally on this fixture is an ASSERTED zero: see its \
         definition. If it moves, a computed cardinal angle is back."
    );
    result.breached[0]
}

/// **A MEASURED FINDING, and the repair that closed it.**
///
/// This constant was `6`. A plain axis-aligned box, hit by production's
/// foot-breach cone, produced six `NearParallel` classifications per sphere —
/// on every wall thickness tested here, and independent of the verdict.
///
/// The cause named here was never a sliver face: `cone_dirs` built its
/// off-axis rays with `ph.cos()` / `ph.sin()` at `ph` = π/2, π, 3π/2, where the
/// component that should be exactly `0.0` came back as an `f64` rounding
/// residue of order `1e-17`. Those rays were parallel to the box's side walls
/// in intent and **not** parallel in bits, so their determinants landed at a
/// measured `±5.18e-18` and `±1.04e-17` — three orders of magnitude BELOW
/// `DET_EPS = 1e-14`, but not zero, which is `NearParallel` by definition where
/// exact arithmetic gives `ExactParallel`, a genuine prune.
///
/// `cone_dirs` now takes its cardinal cosines and sines from
/// `queries::turn_cos_sin` and `queries::deg_cos_sin`, which return the exact
/// value at a quarter turn instead of computing one from an unrepresentable
/// angle. The rays are parallel in bits as well as in intent, the determinants
/// are exactly zero, and the tally is **zero**.
///
/// The verdicts did not move — they never did, even when the tally was six.
/// This constant is kept, rather than deleted with its assertion, because a
/// zero that is ASSERTED is the guard: if anything reintroduces a computed
/// cardinal angle anywhere in the ray construction, this fixture is where it
/// shows up first.
const COSINE_ROUNDING_NEAR_PARALLEL: u32 = 0;

/// The control every test in this file leans on: a wall the ball CANNOT reach
/// through must not be reported as breached.
///
/// Without this, "breach" could be made unconditionally true and every
/// assertion below would still pass. 1.2 mm is thicker than the ball's 0.9 mm
/// reach with room to spare.
fn assert_a_wall_thicker_than_the_ball_is_not_breached() {
    let unreachable_mm = 1.2;
    assert!(
        unreachable_mm > REACH_MM,
        "the control wall must be outside the ball's reach to be a control"
    );
    assert!(
        !ball_breaches_wall(unreachable_mm),
        "a {RADIUS_MM} mm ball standing {STANDOFF_MM} mm off a {unreachable_mm} mm wall reaches \
         only {REACH_MM} mm into it and cannot breach it. Reporting a breach here means the \
         predicate has stopped discriminating, and every 'breach' assertion in this file is \
         then worthless."
    );
}

/// **The defect, stated as physics rather than as a constant.**
///
/// A ball that punches through a 0.1 mm wall punches through a 0.02 mm wall.
/// Nothing about the ball, its standoff, or the wall's other two dimensions
/// changes between these calls -- only the wall gets THINNER. A breach
/// predicate that answers "yes" for the thicker wall and "no" for the thinner
/// one is not conservative, not approximate and not merely coarse: it is
/// inverted, and it is inverted in the unsafe direction.
///
/// This pins monotonicity, not a value. It fixes no thickness, no skip and no
/// tolerance, so it would keep its teeth if every constant in the kernel were
/// renumbered tomorrow. That matters here specifically, because the number this
/// is really about -- `NEAR_WALL_SKIP` -- has no measurement behind it to pin
/// against (see `consts.rs`).
#[test]
fn a_thinner_wall_is_never_safer_than_a_thicker_one() {
    assert_a_wall_thicker_than_the_ball_is_not_breached();

    let thick_mm = 0.10;
    let thin_mm = 0.02;
    assert!(
        thin_mm < thick_mm && thick_mm < REACH_MM,
        "the fixture only discriminates while both walls are inside the ball's reach and the \
         second is the thinner one"
    );

    assert!(
        ball_breaches_wall(thick_mm),
        "control failed: a {RADIUS_MM} mm ball resting {STANDOFF_MM} mm off a {thick_mm} mm \
         wall reaches {REACH_MM} mm and must breach it"
    );
    assert!(
        ball_breaches_wall(thin_mm),
        "a {RADIUS_MM} mm ball breaches a {thick_mm} mm wall but is reported SAFE against a \
         {thin_mm} mm wall at the same standoff. The thinner wall cannot be the safer one. \
         A near-wall skip stated as an absolute distance discards every crossing within \
         NEAR_WALL_SKIP of the nearest surface, so a wall thinner than that skip loses its \
         far face along with its near face and the foot is admitted straight through it."
    );
}

/// **The bound: the thinnest wall this predicate can still resolve.**
///
/// With the skip stated as an absolute distance, the answer was
/// `NEAR_WALL_SKIP` itself -- measured on this fixture, the ladder resolved
/// 0.5, 0.1 and 0.05 mm and lost 0.02, 0.01 and 0.005 mm.
///
/// With the skip stated as a crossing SIGN -- keep the first crossing the ray
/// LEAVES material through, discard the one it enters through -- there is no
/// length scale left in the filter, and the resolution limit stops being a
/// thickness at all. Every wall below is resolved. An exploratory probe of the
/// same fixture carried it to 1e-18 mm without losing one, which is twelve
/// decades below the finest MSLA process resolution and well past the point
/// where a wall is a physical object; the ladder stops at 1e-8 mm because
/// asserting further would be recording arithmetic, not geometry.
///
/// What bounds the predicate now is not thickness. It is the reach
/// precondition (`r - nd > eps_mm`, checked by `breach_one`) and the
/// requirement that the far face be a distinct plane -- which
/// `SurfaceBuildError::DegenerateFace` now enforces at construction, so a
/// zero-thickness "wall" cannot reach this predicate at all.
#[test]
fn records_the_thinnest_wall_the_breach_predicate_resolves() {
    assert_a_wall_thicker_than_the_ball_is_not_breached();

    let ladder = [
        0.5, 0.1, 0.05, 0.02, 0.01, 5e-3, 1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8,
    ];
    let lost: Vec<f64> = ladder
        .iter()
        .copied()
        .filter(|&wall_mm| !ball_breaches_wall(wall_mm))
        .collect();

    assert!(
        lost.is_empty(),
        "the breach predicate loses {lost:?} of the wall ladder {ladder:?}. Every wall on it \
         is thinner than the ball's {REACH_MM} mm reach and is therefore a real breach; a \
         wall the predicate cannot resolve is a support foot admitted through a fin."
    );
}
