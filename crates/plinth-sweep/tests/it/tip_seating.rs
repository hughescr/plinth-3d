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
//! | `the_seat_trim_is_the_carried_face_and_never_moves_the_seat` | every ball and first shaft segment carries the seat trim, its plane is the model's own underside, and dropping the face point moves no seat | it counts balls and trimmed tapers and requires them equal and non-zero |
//! | `thin_skin_seats_in_full_and_is_trimmed_to_its_underside` | a skin thinner than the ball is seated to the law's full depth, and the ball is trimmed to the skin's underside | it requires the skin to be thinner than the law's depth, and every tip to carry the trim |
//! | `a_thick_disc_moves_only_by_the_allowance` | re-running with the law's smallest term pinned (0.150 mm since 2026-09-30, the retired 0.100 mm before) moves ONLY the arms that term binds, and moves no trim plane | it requires both populations to be non-empty |
//!
//! (Until 2026-09-28 the thin-wall bound was a horizontal "far face" clip on
//! the ball's top; the seat trim — the ball and first shaft segment cut to the
//! seat face's plane and out of the model — replaced it, fix/tip-trim.)
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

/// The retired typed depth, read from the registry so the probe below pins a
/// number the engine actually carried rather than a literal.
fn retired_contact_depth_mm() -> f64 {
    m0_constants()
        .expect("registry")
        .placeholders()
        .default_contact_depth_mm()
        .diagnostic()
        .get()
}

/// **The seat minimum the thick-disc probe pins: 1.5 × the retired depth,
/// 0.150 mm. RE-PINNED 2026-09-30 (fix/chitubox-geometry), and why.**
///
/// It was the retired 0.100 mm itself. The owner's ChituBox ball ratios
/// (`CHITUBOX_TIP_CONTACT_PROFILES`: 1.25-1.5 by tip size, where every sweep
/// ball was 1.5 × r before) make the ball clause `(f − √(f² − 1))·r` deeper on
/// every tip above 0.1 mm, and on this disc every seat is then 0.104 mm or
/// deeper, by its ball clause or its incidence clause: the retired depth binds
/// no arm and the comparison split nothing (measured: 0 moved, 236 unmoved;
/// seat-depth deciles 0.104, 0.158, 0.171, 0.184, 0.190 ... 0.274 mm). The
/// claim graded is that the law is a MAXIMUM, so moving its smallest term
/// moves only the arms that term binds; that holds for any pin above the
/// allowance, and 0.150 mm splits this disc (about a tenth of its arms seat
/// below it). A test fixture choice, not a registered number.
fn probe_pin_mm() -> f64 {
    1.5 * retired_contact_depth_mm()
}

/// The registered ball factor at this tip's size: an emitted sweep tip ball has
/// radius `f(r_tip) × r_tip`, `f` read off the owner's ChituBox profiles
/// (`CHITUBOX_TIP_CONTACT_PROFILES`, 2026-09-30; until then the single
/// Placeholder `BALL_FACTOR_STUB` = 1.5), and its equator is the deepest seat
/// that still widens the cut disc.
fn registered_ball_factor(tip_radius_mm: f64) -> f64 {
    m0_constants()
        .expect("registry")
        .evidence()
        .chitubox_tip_contact_profiles()
        .value()
        .ball_factor(tip_radius_mm)
}

/// The depth this arm's seat must reach: the law's own minimum, or its ball's
/// equator where that is shallower — the point past which more burial cuts a
/// NARROWER disc, not a wider one.
fn required_depth_mm(tip_radius_mm: f64) -> f64 {
    seat_minimum_mm().min(registered_ball_factor(tip_radius_mm) * tip_radius_mm)
}

/// The ball-cap clause's own depth for a tip of this radius:
/// `(f − sqrt(f² − 1)) · r_tip`, the depth at which the cut disc equals the
/// shaft. Derived from the registered ball factor, never typed.
fn ball_clause_depth_mm(tip_radius_mm: f64) -> f64 {
    let factor = registered_ball_factor(tip_radius_mm);
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
    let plan = crate::harness::memo_run_sweep(mesh, &layers, &policy)
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
        emission.tips_trimmed_to_seat,
        poles.len(),
        "every seated tip must reach the emitter with its seat trim"
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
        // A HEADED arm (a straight tip whose shaft would graze the model gets
        // an angled head, Craig 2026-10-08) is excused: its end disc is the
        // head's, square to the face by construction, and the chord from its
        // seat to its base is the head plus a post, not the axis the clause
        // reads. The disc's grazers are exactly such arms (arm 177: 34 degrees
        // off the normal, clearance 0.01 mm).
        let straight_tip = arm.head.is_none();
        if straight_tip && sin_tilt > 1e-9 {
            leaned_arms += 1;
        }
        assert!(
            !straight_tip || embed_mm >= arm.tip_radius_mm * sin_tilt - 1e-9,
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
// (b) The seat trim is the carried FACE, and it never moves the seat.
// ---------------------------------------------------------------------------

/// Every seated ball and the first segment of its shaft carry the seat trim,
/// and the trim's plane is the model's own underside — not the raster seat,
/// which the layer grid can put half a layer off it (fix/tip-trim,
/// 2026-09-28). The trim is GEOMETRY only: dropping the measured face point
/// (the plane then falls back to the raster seat) must not move one seat,
/// which is the owner's 2026-09-05 ruling that the tip is never retracted.
#[test]
fn the_seat_trim_is_the_carried_face_and_never_moves_the_seat() {
    let mesh = flat_disc();
    let (plan, layers, frame, min_z) = plan_of(&mesh);
    // STRAIGHT tips only: a headed arm (a graze conversion, Craig 2026-10-08)
    // always carries its own seat plane, the head's, so it is trimmed with a
    // measured face whether or not the arm's face point is dropped. This test
    // is about the face point of a straight tip.
    let arms: Vec<SweepArm> =
        arms_of(&plan).into_iter().filter(|arm| arm.head.is_none()).collect();
    let honest = lower_sweep_arms(&arms, frame, &layers, MAX_PRIMITIVES, MAX_PRIMITIVE_BYTES)
        .expect("the lowering refuses only about the request");
    let faceless: Vec<SweepArm> = arms
        .iter()
        .map(|arm| SweepArm {
            seat_face_point_mm: None,
            ..arm.clone()
        })
        .collect();
    let lowered = lower_sweep_arms(
        &faceless,
        frame,
        &layers,
        MAX_PRIMITIVES,
        MAX_PRIMITIVE_BYTES,
    )
    .expect("the lowering refuses only about the request");
    assert!(
        !honest.tip_seat_depth_mm.is_empty(),
        "a field of zero arms proves nothing"
    );
    assert_eq!(
        lowered.tip_seat_depth_mm, honest.tip_seat_depth_mm,
        "dropping the face point moved a SEAT; the trim may cut geometry, never the seat"
    );
    assert_eq!(honest.tips_trimmed_without_a_measured_face, 0);
    assert_eq!(
        lowered.tips_trimmed_without_a_measured_face,
        lowered.tip_seat_depth_mm.len()
    );

    // Every ball and every FIRST shaft segment carries the trim; nothing else
    // does HERE. A `Member` span carries it only where its solid reaches the
    // model side of the seat plane, which a flat underside above a vertical
    // shaft never allows (a sloped face can: `shaft_seat_trim.rs`).
    let mut balls = 0_usize;
    let mut tapers = 0_usize;
    for primitive in &honest.primitives {
        match primitive.segment_kind {
            SupportSegmentKind::ContactSphere => balls += 1,
            // A brace's or a mast's taper seats on nothing and carries none.
            SupportSegmentKind::ContactTaper if primitive.seat_trim.is_none() => continue,
            SupportSegmentKind::ContactTaper => tapers += 1,
            _ => {
                assert!(
                    primitive.seat_trim.is_none(),
                    "{:?} carries a seat trim",
                    primitive.segment_kind
                );
                continue;
            }
        }
        let trim = primitive.seat_trim.unwrap_or_else(|| {
            panic!(
                "a {:?} reached the emitter untrimmed",
                primitive.segment_kind
            )
        });
        // THE CARRIED FACE: this disc's underside, z = min_z, facing down.
        let normal = trim.outward_normal();
        assert!(
            normal[0].abs() < 1e-9 && normal[1].abs() < 1e-9 && (normal[2] + 1.0).abs() < 1e-9,
            "a flat underside's seat plane faces straight down; got {normal:?}"
        );
        assert!(
            (trim.point_mm()[2] - min_z).abs() < 1e-9,
            "the seat plane stands at z {}, not on the model's underside {min_z}",
            trim.point_mm()[2]
        );
    }
    assert_eq!(balls, honest.tip_seat_depth_mm.len());
    assert_eq!(
        tapers, balls,
        "every seated arm's first segment is trimmed with its ball"
    );
    assert_eq!(
        honest.tips_below_area_guarantee_at_face(),
        0,
        "the contact disc the trim leaves ON the face must carry a >= r_tip"
    );
}

// ---------------------------------------------------------------------------
// (c) A skin thinner than the ball: seat in full, trim to the underside.
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
/// back to the skin. It goes to the full law depth and the area guarantee
/// holds where it used to fail. Since 2026-09-28 the emitted ball is trimmed
/// to the skin's UNDERSIDE (the seat plane), which keeps it off the far face
/// with room to spare; until then a horizontal clip at the far face did that.
#[test]
fn thin_skin_seats_in_full_and_is_trimmed_to_its_underside() {
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
    assert_eq!(
        emission.tips_trimmed_to_seat,
        emission.tip_seat_depth_mm.len(),
        "the trim must reach every tip; a tip without it would be written whole"
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

    // THE SEAT PLANE, on every emitted ball, at the skin's own UNDERSIDE: the
    // emitter keeps only what lies below it, so nothing of the ball can reach
    // the skin's far face 0.06 mm above. The mesh-level consequence — no
    // vertex past the plane or inside the model, the cut ball still closed —
    // is graded in `plinth-emit`'s own unit tests and end to end in
    // `plinth-pipeline`'s `tip_trim`, on the primitive this one proves
    // production hands over.
    let top_face = min_z + skin_mm;
    let mut trimmed = 0_usize;
    for (index, primitive) in emission
        .primitives
        .iter()
        .filter(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere)
        .enumerate()
    {
        let trim = primitive
            .seat_trim
            .unwrap_or_else(|| panic!("tip {index} reached the emitter with no seat trim"));
        trimmed += 1;
        assert!(
            (trim.point_mm()[2] - min_z).abs() < 1e-9 && trim.outward_normal()[2] < -1.0 + 1e-9,
            "tip {index}: the seat plane through {:?} facing {:?} is not the skin's \
             underside z {min_z:.6}; the far face {top_face:.6} would not be protected",
            trim.point_mm(),
            trim.outward_normal()
        );
    }
    assert_eq!(trimmed, emission.tips_trimmed_to_seat);
}

// ---------------------------------------------------------------------------
// (d) Thick material: only the allowance term moved.
// ---------------------------------------------------------------------------

/// The 26eaab7 comparison, run rather than asserted.
///
/// On a 3.5 mm disc every emitted primitive must be exactly what the seat law
/// alone produces, and every trim plane is the underside whatever the law's
/// terms are. The one thing that commit changed for such a model is PR-1(a): the
/// law's smallest term went from the typed 0.100 mm to the derived allowance.
/// So the test re-runs the identical lowering with that term PINNED above the
/// allowance (the `seat-minimum-probe` feature, which no production build
/// enables; the pin was the retired 0.100 mm until 2026-09-30 and is 0.150 mm
/// since, see [`probe_pin_mm`] for the measured reason) and requires:
///
/// * every arm whose BALL clause binds — the term this commit did not touch —
///   to emit bit-identical geometry under both; and
/// * every arm whose minimum term binds to move, and to move only along its own
///   axis.
///
/// Both populations are required to be non-empty, so neither half can pass by
/// examining nothing.
#[test]
fn a_thick_disc_moves_only_by_the_allowance() {
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
        probe_pin_mm(),
    )
    .expect("the lowering refuses only about the request");
    // The seat trim is a property of the FACE, not of the seat law: moving
    // the law's smallest term must leave every ball's trim plane unchanged.
    let trims = |lowering: &SweepLowering| -> Vec<_> {
        lowering
            .primitives
            .iter()
            .filter(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere)
            .map(|primitive| primitive.seat_trim)
            .collect()
    };
    assert_eq!(
        trims(&now),
        trims(&pinned),
        "the seat law's minimum moved a trim plane; the plane is the carried face's"
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
    let step_mm = probe_pin_mm() - seat_minimum_mm();
    assert!(
        step_mm > 0.0,
        "the derived allowance {} mm must be SHALLOWER than the pinned depth {} mm, or this \
         comparison is grading the wrong direction",
        seat_minimum_mm(),
        probe_pin_mm()
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
            let equator_mm =
                registered_ball_factor(now.tip_radius_mm[index]) * now.tip_radius_mm[index];
            assert!(
                *here >= probe_pin_mm() - 1e-12
                    || (*here - ball_clause_depth_mm(now.tip_radius_mm[index])).abs() < 1e-12
                    || (*here - flank_mm).abs() < 1e-12
                    || (*here - equator_mm).abs() < 1e-12,
                "arm {index} did not move, yet its depth {here} is below the pinned \
                 {} mm and is none of its own ball clause's depth, its own incidence \
                 clause's {flank_mm} mm, or its own ball equator at {equator_mm} mm — so \
                 nothing explains why the term that moved did not move it",
                probe_pin_mm()
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
