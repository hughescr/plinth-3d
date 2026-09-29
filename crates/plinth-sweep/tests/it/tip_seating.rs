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
//! **And the tip must go to its FULL depth even in a thin skin.** The owner
//! ruled again on 2026-09-05, after seeing the first fix: *"'contact depth'
//! 0.1 mm is a fake number — once supports overlap the model they are the same
//! voxels ... Push the support tip into the model as deeply as it needs to go
//! to reach the contact area that is needed, then TRUNCATE the geometry beyond
//! the model's surface so the tip cannot protrude on the other side of any thin
//! model layer."* So the thin-wall bound moved off the SEAT and onto the
//! emitted GEOMETRY, and the law's smallest term stopped being a typed number.
//!
//! | test | what it asserts | how it is kept from passing vacuously |
//! |---|---|---|
//! | `flat_disc_tips_seat_by_the_law` | every pole is inside by at least the law's own depth, and every delivered cut disc carries `a ≥ r_tip` | it counts the arms it graded and fails on an empty field; the sabotage below turns it red |
//! | `zero_available_depth_clips_but_never_retracts` | zeroing the seating input changes the CLIP, never the seat | it requires every seat to stay at full depth AND every ball to be clipped |
//! | `thin_skin_seats_in_full_and_clips_at_the_far_face` | a skin thinner than the ball is seated to the law's full depth, and the ball is clipped exactly at the skin's top face | it requires the skin to be thinner than the law's depth, and the clip counter to FIRE |
//! | `a_thick_disc_never_clips_and_moves_only_by_the_allowance` | on thick material nothing clips, and re-running with the retired 0.100 mm pinned moves ONLY the arms that term binds | it requires both populations to be non-empty |
//!
//! Nothing here restates the seat law. The law's own numbers come from the
//! registry, and the assertions are on consequences a reader can check against
//! `docs/physics.md` §9 without re-deriving the formula: "at least as deep as
//! the law asks", "the cut is no narrower than the tip", "no geometry above the
//! far face".

// Layer counts from finite model-scale lengths, in test-local arithmetic.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_mesh::{make_disc_uniform, Mesh};
use plinth_support::{
    lower_sweep_arms, BuildFrame, SphereGeometry, SupportSegmentKind, SweepArm, SweepLowering,
};
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

/// The law's own smallest term — the DERIVED rasterisation allowance, one
/// layer in Z plus one addressable pixel in plane.
///
/// Both halves come from the registry. The typed `DEFAULT_CONTACT_DEPTH_MM`
/// stood here until the owner's ruling of 2026-09-05 retired it from deciding
/// seats; it survives only as the ChituBox taper record's own field.
fn seat_minimum_mm() -> f64 {
    layer_height_mm() + registered_pixel_pitch_mm()
}

/// One addressable pixel on the coarser in-plane axis of the registered
/// printer profile, in mm.
fn registered_pixel_pitch_mm() -> f64 {
    let printer = m0_constants().expect("registry").saturn4_ultra();
    printer
        .pixel_pitch_x_um()
        .value()
        .max(*printer.pixel_pitch_y_um().value())
        / 1000.0
}

/// The retired typed depth, read from the registry so the probe below pins the
/// number the engine actually carried rather than a literal.
fn retired_contact_depth_mm() -> f64 {
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
    seat_minimum_mm().min(registered_ball_factor() * tip_radius_mm)
}

/// The ball-cap clause's own depth for a tip of this radius:
/// `(f − sqrt(f² − 1)) · r_tip`, the depth at which the cut disc equals the
/// shaft. Derived from the registered ball factor, never typed.
fn ball_clause_depth_mm(tip_radius_mm: f64) -> f64 {
    let factor = registered_ball_factor();
    (factor - (factor * factor - 1.0).max(0.0).sqrt()) * tip_radius_mm
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
        "a 3.5 mm disc is nowhere near thin enough to clip a ball"
    );

    // # Re-cut 2026-09-06, after the production cause was found and fixed
    //
    // This loop went red on arm 196 at 0.0585 mm against the 0.0690 mm its own
    // tip radius asked for. The tip was neither merged wrongly nor mis-sized:
    // the arm LEANED 57 degrees off vertical, its nearest mesh face was the
    // vertical rim wall (`tip_landing::tip_seat_normals` refuses a face that is
    // not down-facing), so its seat normal went unmeasured — and the lowering
    // then substituted THE ARM'S OWN AXIS for the missing normal. That
    // substitution is exact for a vertical arm and loses a factor of
    // `cos(tilt)` for every other one, so the law asked for 0.1070 mm of
    // burial along the axis and the model received 0.0585 mm of it. The
    // fallback is now straight down (`lower.rs`, `seat_tip`), which is the
    // direction the law's smallest term — one layer in Z — is denominated in.
    //
    // So the assertion below is unchanged in form and is now a THEOREM rather
    // than a coincidence, and two stronger claims are asserted beside it: the
    // burial covers the tilted end disc's own projection, and the leaned
    // population that broke it is non-empty.
    let mut deepest_below_surface = f64::INFINITY;
    let mut leaned_arms = 0_usize;
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
        // THE TILT CLAUSE, delivered rather than assumed. This disc's carried
        // face is horizontal, so an arm running `tilt` off vertical presents an
        // end disc whose projection on that face is `r_tip · sin(tilt)` deep;
        // burial shallower than that engages part of the cross-section and
        // smears the rest (`docs/physics.md` §9, clause (b)). A leaned arm is
        // exactly what the old axis fallback under-delivered, so this is the
        // claim that would go red again if it came back.
        let axis = [
            arm.base_mm[0] - arm.seat_mm[0],
            arm.base_mm[1] - arm.seat_mm[1],
            arm.base_mm[2] - arm.seat_mm[2],
        ];
        let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        let sin_tilt = (axis[0] * axis[0] + axis[1] * axis[1]).sqrt() / length;
        if sin_tilt > 1e-9 {
            leaned_arms += 1;
        }
        assert!(
            embed_mm >= arm.tip_radius_mm * sin_tilt - 1e-9,
            "arm {index}: buried {embed_mm:.6} mm under a horizontal face while leaning with \
             sin(tilt) {sin_tilt:.6} and a tip radius of {:.6} mm, so part of its end disc never \
             meets the model",
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
    assert!(
        leaned_arms > 0,
        "no arm on this disc leaned, so the tilt clause above — the one the axis fallback broke \
         — was graded on nothing"
    );
    println!(
        "graded {} arms, {leaned_arms} of them leaned; shallowest burial {deepest_below_surface:.6} mm",
        arms.len()
    );
}

// ---------------------------------------------------------------------------
// (b) Sabotage: zero the seating input, and require the CLIP to move, not the
//     seat.
// ---------------------------------------------------------------------------

/// The knob is the seating INPUT, not a production flag: `available_seat_depth_mm`
/// is what the sweep measured of the material above each seat.
///
/// Before the owner's ruling of 2026-09-05 zeroing it retracted every tip back
/// onto the lamina plane — the original point-contact defect, restored. Under
/// the ruling it may do no such thing: the seat stays at the full depth the
/// area law asked for, and what zeroing changes is where the emitted ball is
/// CUT. This test is the guard on that: a seat that still moves when the
/// thickness moves is a retraction, and retraction is what the ruling forbids.
#[test]
fn zero_available_depth_clips_but_never_retracts() {
    let mesh = flat_disc();
    let (plan, layers, frame, _) = plan_of(&mesh);
    let arms = arms_of(&plan);
    let honest = lower_sweep_arms(&arms, frame, &layers, MAX_PRIMITIVES, MAX_PRIMITIVE_BYTES)
        .expect("the lowering refuses only about the request");
    let sabotaged: Vec<SweepArm> = arms
        .iter()
        .map(|arm| SweepArm {
            available_seat_depth_mm: 0.0,
            ..arm.clone()
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
    assert_eq!(
        lowered.tip_seat_depth_mm, honest.tip_seat_depth_mm,
        "zeroing the available thickness moved a SEAT. Under the owner's ruling of \
         2026-09-05 the tip goes to the depth the contact area needs whatever the skin \
         above it is; only the emitted geometry is cut"
    );
    // (a)'s own predicate must STILL hold with the thickness zeroed.
    let short = lowered
        .tip_seat_depth_mm
        .iter()
        .zip(&lowered.tip_radius_mm)
        .filter(|(depth, tip)| **depth < required_depth_mm(**tip) - 1e-9)
        .count();
    assert_eq!(
        short, 0,
        "{short} tips fell short of the law's depth with the thickness zeroed — that is \
         the retraction the ruling replaced with a clip"
    );
    assert_eq!(
        lowered.tips_below_area_guarantee(),
        0,
        "a clipped tip still carries its full cut disc; the area guarantee cannot depend \
         on how thick the skin above it is"
    );
    // And the clip DID fire, on every arm: with no material above the seat, no
    // part of any ball may be emitted above the seat plane.
    assert_eq!(
        lowered.tips_truncated_by_thin_wall,
        lowered.tip_seat_depth_mm.len(),
        "with zero material above the seat every ball must be clipped at the seat plane"
    );
    for (index, primitive) in lowered
        .primitives
        .iter()
        .filter(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere)
        .enumerate()
    {
        let clip = primitive
            .sphere_geometry
            .as_ref()
            .and_then(SphereGeometry::clip_above_z_mm);
        assert!(
            clip.is_some(),
            "tip {index} reached the emit boundary with no clip plane, so its ball would \
             be written whole and would stand proud of the surface"
        );
    }
}

// ---------------------------------------------------------------------------
// (c) A skin thinner than the ball: seat in full, clip at the far face.
// ---------------------------------------------------------------------------

/// A 20 mm disc 0.06 mm thick — ONE lamina at the registered layer height,
/// against a law whose smallest term is the derived rasterisation allowance.
///
/// The skin was 0.15 mm while the registry carried a 0.300 mm typed depth, and
/// 0.06 mm since 2026-09-05. It has to stay thinner than the law's own minimum
/// or this test grades nothing, and the assertion below fails loudly if it ever
/// stops being so.
///
/// What the fixture proves changed on 2026-09-05: the seat is NO LONGER cut
/// back to the skin. It goes to the full law depth, the ball is clipped at the
/// skin's own top face, and the area guarantee holds where it used to fail.
#[test]
fn thin_skin_seats_in_full_and_clips_at_the_far_face() {
    let skin_mm = 0.06;
    let mesh = make_disc_uniform(20.0, skin_mm, 64, 4).expect("a thin-skin fixture");
    let (plan, layers, frame, min_z) = plan_of(&mesh);
    let arms = arms_of(&plan);
    let emission = lower(&plan, &layers, frame);

    assert!(
        !emission.tip_seat_depth_mm.is_empty(),
        "the thin skin must still field arms; emit and warn, never refuse"
    );
    assert!(
        skin_mm < seat_minimum_mm(),
        "this fixture only grades a clip while its skin is thinner than the law's own \
         minimum depth ({} mm)",
        seat_minimum_mm()
    );
    assert!(
        emission.tips_truncated_by_thin_wall > 0,
        "the clip must FIRE here; zero clips means the far face is not being read"
    );

    // THE RULING ITSELF: full depth, not skin depth.
    let mut seated_past_the_skin = 0_usize;
    for (index, (arm, depth)) in arms.iter().zip(&emission.tip_seat_depth_mm).enumerate() {
        let required_mm = required_depth_mm(arm.tip_radius_mm);
        assert!(
            *depth >= required_mm - 1e-9,
            "arm {index}: seated {depth:.6} mm, short of the {required_mm:.6} mm the law \
             asks of a {:.6} mm tip. A thin skin may cut the GEOMETRY, never the seat",
            arm.tip_radius_mm
        );
        if *depth > skin_mm + 1e-9 {
            seated_past_the_skin += 1;
        }
    }
    assert!(
        seated_past_the_skin > 0,
        "no seat went deeper than the skin, so this fixture is not exercising the clip \
         at all — it would pass under the retracting behaviour the ruling replaced"
    );

    // AND THE AREA GUARANTEE, which used to fail on exactly this fixture family.
    assert_eq!(
        emission.tips_below_area_guarantee_count(),
        0,
        "a clipped tip carries the cut disc the law sized; cut radii {:?} against tips {:?}",
        SweepLowering::distribution(&emission.tip_cut_radius_mm),
        SweepLowering::distribution(&emission.tip_radius_mm),
    );

    // THE CLIP PLANE, on the emitted ball, at the skin's own far face. The
    // mesh-level consequence — no triangle above it, and the cut ball still
    // closed — is graded in `plinth-emit`'s own unit tests, on the primitive
    // this one proves production hands over.
    let top_face = min_z + skin_mm;
    let mut clipped = 0_usize;
    for (index, primitive) in emission
        .primitives
        .iter()
        .filter(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere)
        .enumerate()
    {
        let Some(clip) = primitive
            .sphere_geometry
            .as_ref()
            .and_then(SphereGeometry::clip_above_z_mm)
        else {
            continue;
        };
        clipped += 1;
        assert!(
            clip.get() <= top_face + layer_height_mm() + 1e-9,
            "tip {index}: the clip plane at z {:.6} stands above the skin's top face \
             {top_face:.6}, so geometry could still be emitted outside the model",
            clip.get()
        );
    }
    assert_eq!(
        clipped, emission.tips_truncated_by_thin_wall,
        "every counted clip must carry a clip plane on its own emitted ball"
    );
}

// ---------------------------------------------------------------------------
// (d) Thick material: nothing clips, and only the allowance term moved.
// ---------------------------------------------------------------------------

/// The 26eaab7 comparison, run rather than asserted.
///
/// On a 3.5 mm disc no ball can reach a far face, so the clip must never fire
/// and every emitted primitive must be exactly what the seat law alone
/// produces. The one thing this commit changed for such a model is PR-1(a): the
/// law's smallest term went from the typed 0.100 mm to the derived allowance.
/// So the test re-runs the identical lowering with that term PINNED back to
/// 0.100 mm (the `seat-minimum-probe` feature, which no production build
/// enables) and requires:
///
/// * every arm whose BALL clause binds — the term this commit did not touch —
///   to emit bit-identical geometry under both; and
/// * every arm whose minimum term binds to move, and to move only along its own
///   axis.
///
/// Both populations are required to be non-empty, so neither half can pass by
/// examining nothing.
#[test]
fn a_thick_disc_never_clips_and_moves_only_by_the_allowance() {
    let mesh = flat_disc();
    let (plan, layers, frame, _) = plan_of(&mesh);
    let arms = arms_of(&plan);
    let now = lower_sweep_arms(&arms, frame, &layers, MAX_PRIMITIVES, MAX_PRIMITIVE_BYTES)
        .expect("the lowering refuses only about the request");
    let pinned = plinth_support::lower_sweep_arms_with_pinned_seat_minimum(
        &arms,
        frame,
        &layers,
        MAX_PRIMITIVES,
        MAX_PRIMITIVE_BYTES,
        retired_contact_depth_mm(),
    )
    .expect("the lowering refuses only about the request");

    assert_eq!(
        now.tips_truncated_by_thin_wall, 0,
        "a 3.5 mm disc has no far face any ball can reach; the clip must not fire"
    );
    assert!(
        now.primitives
            .iter()
            .filter(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere)
            .all(|primitive| primitive
                .sphere_geometry
                .as_ref()
                .and_then(SphereGeometry::clip_above_z_mm)
                .is_none()),
        "an unclipped model must carry no clip plane on any ball, or the emitter will cut \
         geometry nobody asked it to cut"
    );
    assert_eq!(
        now.primitives.len(),
        pinned.primitives.len(),
        "moving the law's smallest term may move geometry; it may not add or drop a \
         primitive"
    );

    // The law takes a MAXIMUM over its terms, so lowering one term can only
    // lower a seat, and by no more than that term moved. Stated as a
    // consequence rather than by recomputing the maximum here, which is the
    // trap a test that re-implements production logic falls into.
    let step_mm = retired_contact_depth_mm() - seat_minimum_mm();
    assert!(
        step_mm > 0.0,
        "the derived allowance {} mm must be SHALLOWER than the typed depth {} mm it \
         replaced, or this comparison is grading the wrong direction",
        seat_minimum_mm(),
        retired_contact_depth_mm()
    );
    let mut moved = 0_usize;
    let mut unmoved = 0_usize;
    for (index, (here, there)) in now
        .tip_seat_depth_mm
        .iter()
        .zip(&pinned.tip_seat_depth_mm)
        .enumerate()
    {
        assert!(
            *here <= *there + 1e-12,
            "arm {index}: lowering the law's smallest term deepened a seat, from {there} \
             to {here}. A maximum cannot do that"
        );
        assert!(
            *there - *here <= step_mm + 1e-12,
            "arm {index}: the seat moved {} mm, more than the {step_mm} mm the term \
             itself moved, so something other than PR-1(a) changed",
            *there - *here
        );
        if *there - *here > 1e-12 {
            moved += 1;
        } else {
            unmoved += 1;
            // An unmoved arm is one another term binds, and the only way that
            // holds is if its depth already exceeded the term that moved.
            //
            // THE INCIDENCE CLAUSE JOINED THIS LIST ON 2026-09-06, and it did
            // so because the fix of that date made it reachable rather than
            // because the test was too strict. While an unmeasured seat normal
            // fell back to the arm's own axis, every incidence read zero and
            // the flank term was `process_resolution` flat; with the fallback
            // corrected to the vertical, a leaned arm reads a real `sin(theta)`
            // and `process_resolution + r_tip·sin(theta)` can be the binding
            // term -- and, where that term exceeds the ball's own equator, the
            // EQUATOR CAP is what binds instead. Arm 203 is one of those: a
            // minimum-radius tip leaning at sin(theta) = 0.837 asks for
            // 0.0919 mm and is held at its equator, 0.0750 mm.
            let arm = &arms[index];
            let axis = [
                arm.base_mm[0] - arm.seat_mm[0],
                arm.base_mm[1] - arm.seat_mm[1],
                arm.base_mm[2] - arm.seat_mm[2],
            ];
            let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
            let sin_tilt = (axis[0] * axis[0] + axis[1] * axis[1]).sqrt() / length;
            let flank_mm = layer_height_mm() + now.tip_radius_mm[index] * sin_tilt;
            let equator_mm = registered_ball_factor() * now.tip_radius_mm[index];
            assert!(
                *here >= retired_contact_depth_mm() - 1e-12
                    || (*here - ball_clause_depth_mm(now.tip_radius_mm[index])).abs() < 1e-12
                    || (*here - flank_mm).abs() < 1e-12
                    || (*here - equator_mm).abs() < 1e-12,
                "arm {index} did not move, yet its depth {here} is below the retired \
                 {} mm and is none of its own ball clause's depth, its own incidence \
                 clause's {flank_mm} mm, or its own ball equator at {equator_mm} mm — so \
                 nothing explains why the term that moved did not move it",
                retired_contact_depth_mm()
            );
        }
    }
    assert!(
        moved > 0 && unmoved > 0,
        "this comparison needs BOTH populations to say anything: {moved} arms moved with \
         the term and {unmoved} were bound by another term"
    );

    // AND THE GEOMETRY, not just the depth: an arm whose seat did not move must
    // emit a bit-identical tip ball under both.
    let spheres = |lowering: &SweepLowering| -> Vec<[u64; 3]> {
        lowering
            .primitives
            .iter()
            .filter(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere)
            .map(|primitive| primitive.start_mm.map(|value| value.get().to_bits()))
            .collect()
    };
    let (here_poles, there_poles) = (spheres(&now), spheres(&pinned));
    assert_eq!(here_poles.len(), now.tip_seat_depth_mm.len());
    for (index, (here, there)) in here_poles.iter().zip(&there_poles).enumerate() {
        let unchanged = pinned.tip_seat_depth_mm[index] - now.tip_seat_depth_mm[index] <= 1e-12;
        if unchanged {
            assert_eq!(
                here, there,
                "arm {index}'s seat did not move, so its emitted ball must be identical"
            );
        }
    }
}
