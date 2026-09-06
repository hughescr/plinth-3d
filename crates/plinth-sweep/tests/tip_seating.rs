//! **The tip must be IN the model, not ON it.**
//!
//! The owner's ChituBox verdict of 2026-09-05, on the Sphere and the Morel
//! base: *"the spherical tip of the support does not seem to penetrate the
//! model AT ALL. It looks like it's basically just a point contact which will
//! provide almost no support ... true at every scale."*
//!
//! The cause was arithmetic, not placement. `plinth_sweep::adapter` hands the
//! lowering `seat = plate_z + k_top·h`, the BOTTOM plane of the first cured
//! model lamina, and `plinth_support::lower_sweep_arms` put the tip ball's
//! pole exactly there — zero seating depth, a contact disc of radius zero.
//!
//! These tests grade the fix on the quantity the owner actually objects to:
//! the DELIVERED contact, measured through the production sweep and the
//! production lowering, on fixtures whose geometry makes the answer
//! predictable.
//!
//! | test | what it asserts | how it is kept from passing vacuously |
//! |---|---|---|
//! | `flat_disc_tips_seat_by_the_law` | every pole is inside by at least the registered contact depth, and every delivered cut disc carries `a ≥ r_tip` | it counts the arms it graded and fails on an empty field; the sabotage below turns it red |
//! | `zero_available_depth_restores_the_defect` | zeroing the seating input reproduces the pole-on-the-surface geometry exactly | it re-asserts the FIRST test's own predicate and requires it to fail |
//! | `thin_skin_truncates_and_never_exits` | a skin thinner than the law's depth is seated to what is there, counted, and never pierced | it requires the truncation counter to FIRE, so a fixture that stopped being thin fails |
//!
//! Nothing here restates the seat law. The law's own numbers come from the
//! registry, and the assertions are on consequences a reader can check against
//! `docs/physics.md` §9 without re-deriving the formula: "deeper than the
//! registered contact depth", "the cut is no narrower than the tip", "the pole
//! never leaves the material".

// Layer counts from finite model-scale lengths, in test-local arithmetic.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_mesh::{make_disc_uniform, Mesh};
use plinth_support::{lower_sweep_arms, BuildFrame, SupportSegmentKind, SweepArm, SweepLowering};
use plinth_sweep::{arms_of, to_primitives, SweepEmission, SweepPlan, SweepPolicy};

const LIFT_MM: f64 = 5.0;
const MAX_PRIMITIVES: usize = 4_000_000;
const MAX_PRIMITIVE_BYTES: usize = 1 << 30;

fn layer_height_mm() -> f64 {
    *m0_constants()
        .expect("registry")
        .evidence()
        .layer_height_default_mm()
        .value()
}

/// The registered smallest seat depth — the law's own minimum term, read from
/// the registry rather than typed here.
fn registered_contact_depth_mm() -> f64 {
    m0_constants()
        .expect("registry")
        .placeholders()
        .default_contact_depth_mm()
        .diagnostic()
        .get()
}

/// The registered ball factor: an emitted tip ball has radius
/// `BALL_FACTOR × r_tip`, and its equator is the deepest seat that still widens
/// the cut disc.
fn registered_ball_factor() -> f64 {
    m0_constants()
        .expect("registry")
        .placeholders()
        .ball_factor_stub()
        .diagnostic()
        .get()
}

/// The depth this arm's seat must reach: the law's own minimum, or its ball's
/// equator where that is shallower — the point past which more burial cuts a
/// NARROWER disc, not a wider one.
fn required_depth_mm(tip_radius_mm: f64) -> f64 {
    registered_contact_depth_mm().min(registered_ball_factor() * tip_radius_mm)
}

fn z_span(mesh: &Mesh) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for vertex in mesh.verts() {
        min = min.min(vertex[2]);
        max = max.max(vertex[2]);
    }
    (min, max)
}

/// The production sweep and the production layer plan, exactly as
/// `prepare.rs`'s `sweep_and_emit` builds them: `plate_z = min_z − lift`, one
/// uniform plan stopping one layer above the model top.
fn plan_of(mesh: &Mesh) -> (SweepPlan, LayerPlan, BuildFrame, f64) {
    let (min_z, max_z) = z_span(mesh);
    let plate_z = min_z - LIFT_MM;
    let height = layer_height_mm();
    let count = (((max_z - plate_z) / height).ceil() as usize)
        .saturating_add(1)
        .max(1);
    let layers = LayerPlan::new(plate_z, vec![height; count]).expect("a uniform layer plan");
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    let plan = plinth_sweep::run_sweep(mesh, &layers, &policy)
        .expect("the sweep emits and warns; it never refuses");
    let frame = BuildFrame::new(
        Finite::new(plate_z).expect("a finite plate"),
        PositiveFinite::new(LIFT_MM).expect("a positive lift"),
    );
    (plan, layers, frame, min_z)
}

fn lower(plan: &SweepPlan, layers: &LayerPlan, frame: BuildFrame) -> SweepEmission {
    to_primitives(plan, frame, layers, MAX_PRIMITIVES, MAX_PRIMITIVE_BYTES)
        .expect("the lowering refuses only about the request")
}

/// Every emitted tip ball's pole, in emission order — the point the owner was
/// looking at in ChituBox.
///
/// `SphereGeometry::canonical_poles` puts the seat-side pole in `start_mm`, so
/// this reads the producer's own answer rather than recomputing a pole.
fn tip_poles(emission: &SweepEmission) -> Vec<[f64; 3]> {
    emission
        .primitives
        .iter()
        .filter(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere)
        .map(|primitive| primitive.start_mm.map(Finite::get))
        .collect()
}

// ---------------------------------------------------------------------------
// (a) The flat-bottomed fixture, through the real sweep.
// ---------------------------------------------------------------------------

/// A 50 mm disc, 3.5 mm thick: a flat down-facing underside, so every arm is
/// vertical under a horizontal face and the law's incidence term is zero.
fn flat_disc() -> Mesh {
    make_disc_uniform(50.0, 3.5, 96, 8).expect("a disc fixture")
}

#[test]
fn flat_disc_tips_seat_by_the_law() {
    let mesh = flat_disc();
    let (plan, layers, frame, min_z) = plan_of(&mesh);
    let arms = arms_of(&plan);
    let emission = lower(&plan, &layers, frame);

    assert_eq!(
        emission.arms_dropped_zero_length + emission.arms_dropped_invalid_geometry,
        0,
        "the flat disc must build every arm it offers, or the pole/arm pairing below is wrong"
    );
    let poles = tip_poles(&emission);
    assert_eq!(
        poles.len(),
        arms.len(),
        "one emitted tip ball per offered arm"
    );
    assert!(
        poles.len() >= 50,
        "a 50 mm disc must field a real support forest, not a token one; got {}",
        poles.len()
    );
    assert_eq!(
        emission.tips_truncated_by_thin_wall, 0,
        "a 3.5 mm disc is nowhere near thin enough to truncate a seat"
    );

    let mut deepest_below_surface = f64::INFINITY;
    for (index, (arm, pole)) in arms.iter().zip(&poles).enumerate() {
        // THE DEFECT'S OWN MEASUREMENT: the pole against the model surface the
        // adapter handed the lowering. Before this fix it was exactly zero.
        let embed_mm = pole[2] - arm.seat_mm[2];
        let required_mm = required_depth_mm(arm.tip_radius_mm);
        assert!(
            embed_mm >= required_mm - 1e-9,
            "arm {index}: the tip pole stands {embed_mm:.6} mm inside the model, below the \
             {required_mm:.6} mm this arm's own tip radius {:.6} mm asks for",
            arm.tip_radius_mm
        );
        // And against the model's own lowest surface, which is what the owner
        // sees: the pole is INSIDE the disc, not resting on it.
        assert!(
            pole[2] > min_z,
            "arm {index}: the tip pole sits at z {:.6}, at or below the model's lowest z \
             {min_z:.6} — that is the point contact this fix exists to end",
            pole[2]
        );
        deepest_below_surface = deepest_below_surface.min(embed_mm);
    }

    // THE AREA LAW'S OWN GUARANTEE, delivered rather than assumed: the disc the
    // seat plane cuts through each ball is no narrower than the shaft above it.
    assert_eq!(
        emission.tips_below_area_guarantee_count(),
        0,
        "every delivered cut disc must carry a >= r_tip; cut radii {:?} against tips {:?}",
        SweepLowering::distribution(&emission.tip_cut_radius_mm),
        SweepLowering::distribution(&emission.tip_radius_mm),
    );
    // The ball-cap clause is evaluated on a REAL ball, not a degenerate one.
    assert!(
        emission.tip_radius_mm.iter().any(|radius| *radius > 0.1),
        "at least one arm must carry a tip radius above 0.1 mm, or the ball term in the \
         seat law is being graded on nothing"
    );
    assert!(
        deepest_below_surface.is_finite(),
        "the loop above must have graded at least one arm"
    );
}

// ---------------------------------------------------------------------------
// (b) Sabotage: restore zero seating and require (a) to go red.
// ---------------------------------------------------------------------------

/// The knob is the seating INPUT, not a production flag: `available_seat_depth_mm`
/// is what the sweep measured of the material above each seat, and zeroing it
/// truncates every seat to nothing. That is exactly the pre-fix geometry — the
/// pole on the lamina plane — reached through the same production lowering.
#[test]
fn zero_available_depth_restores_the_defect() {
    let mesh = flat_disc();
    let (plan, layers, frame, _) = plan_of(&mesh);
    let sabotaged: Vec<SweepArm> = arms_of(&plan)
        .into_iter()
        .map(|arm| SweepArm {
            available_seat_depth_mm: 0.0,
            ..arm
        })
        .collect();
    let lowered = lower_sweep_arms(
        &sabotaged,
        frame,
        &layers,
        MAX_PRIMITIVES,
        MAX_PRIMITIVE_BYTES,
    )
    .expect("the lowering refuses only about the request");

    assert!(
        !lowered.tip_seat_depth_mm.is_empty(),
        "the sabotage must still build arms; a field of zero arms proves nothing"
    );
    // (a)'s own predicate, re-asserted here and REQUIRED to fail.
    let still_passing = lowered
        .tip_seat_depth_mm
        .iter()
        .zip(&lowered.tip_radius_mm)
        .filter(|(depth, tip)| **depth >= required_depth_mm(**tip) - 1e-9)
        .count();
    assert_eq!(
        still_passing, 0,
        "with the seating input zeroed, no tip may still meet the law's depth — \
         {still_passing} did, so the assertion in `flat_disc_tips_seat_by_the_law` \
         is not testing the seating at all"
    );
    assert!(
        lowered.tip_seat_depth_mm.iter().all(|depth| *depth == 0.0),
        "the sabotage restores EXACTLY the pre-fix geometry: a pole on the lamina plane"
    );
    assert_eq!(
        lowered.tips_truncated_by_thin_wall,
        lowered.tip_seat_depth_mm.len(),
        "every one of those seats is a truncation, and every one must be counted"
    );
    assert!(
        lowered.tips_below_area_guarantee() > 0,
        "a zero-depth seat cuts a zero-radius disc, which cannot carry the tip's own section"
    );
}

// ---------------------------------------------------------------------------
// (c) A skin thinner than the law's depth.
// ---------------------------------------------------------------------------

/// A 20 mm disc 0.06 mm thick — ONE lamina at the registered layer height,
/// against a law whose smallest term is the registered contact depth. The seat
/// has to truncate, and the pole must stop inside the skin.
///
/// The skin was 0.15 mm while the registry carried 0.300 mm. It was thinned on
/// 2026-09-05 when the owner's ruling of 2026-08-30 landed and the minimum
/// became 0.100 mm: at 0.15 mm this fixture would no longer be thin enough to
/// truncate anything and the test would have gone quietly vacuous. The
/// assertion below is what makes that impossible — it fails loudly if the skin
/// ever stops being thinner than the law's own minimum.
#[test]
fn thin_skin_truncates_and_never_exits() {
    let skin_mm = 0.06;
    let mesh = make_disc_uniform(20.0, skin_mm, 64, 4).expect("a thin-skin fixture");
    let (plan, layers, frame, min_z) = plan_of(&mesh);
    let emission = lower(&plan, &layers, frame);
    let poles = tip_poles(&emission);

    assert!(
        !poles.is_empty(),
        "the thin skin must still field arms; emit and warn, never refuse"
    );
    assert!(
        skin_mm < registered_contact_depth_mm(),
        "this fixture is only a truncation test while its skin is thinner than the law's \
         own minimum depth ({} mm)",
        registered_contact_depth_mm()
    );
    assert!(
        emission.tips_truncated_by_thin_wall > 0,
        "the thin-wall bound must FIRE here; zero truncations means the bound is not wired"
    );
    for (index, depth) in emission.tip_seat_depth_mm.iter().enumerate() {
        assert!(
            *depth <= skin_mm + 1e-9,
            "arm {index}: seated {depth:.6} mm into a {skin_mm} mm skin"
        );
    }
    let top_face = min_z + skin_mm;
    for (index, pole) in poles.iter().enumerate() {
        assert!(
            pole[2] <= top_face + 1e-9,
            "arm {index}: the tip pole at z {:.6} has come out through the skin's top face \
             at {top_face:.6} — a seat never exits the far face of what it strikes",
            pole[2]
        );
    }
}
