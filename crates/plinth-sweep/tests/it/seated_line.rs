//! **A junction goes on the shaft the file draws, and a seated ball's shaft
//! starts at the ball's centre** (fix lane A, 2026-10-12).
//!
//! # The defect this pins
//!
//! The lowering draws a seated member's shaft from its ball's CENTRE, which
//! the seat law puts off the seat along the seat normal, and which the contact
//! growth (`plinth_sweep::contact_growth`) puts further off: it keeps the
//! ungrown ball's deepest point and moves the centre out along the normal. A
//! member landed on that shaft was placed on the seat-to-base line. On
//! Deinonychus (`joint/out-fix/final`, slicer view) post 162's ball grew to
//! 0.533 mm, its drawn shaft stood 0.25 mm from the planned axis where arm
//! 160 landed, and arm 160's first cured layer, record 692 at 8.675 mm,
//! printed over nothing. Only a LEANING plate post's drawn line was read
//! (`adapter::drawn_line_offset`), so the vertical post and every member
//! landed on another member moved nothing.
//!
//! # What is graded
//!
//! On the lowering's own line ([`plinth_support::SweepRootFlare::shaft_line_of`]):
//! the offset [`plinth_sweep::adapter::drawn_line_offset`] returns puts a
//! planned point ON the drawn shaft (within 1e-9 mm), for a member landed on
//! another member and for a vertical plate post, both with a grown ball on a
//! tilted seat. The sabotage is the rule before the fix: no offset, and the
//! planned point stands off the drawn shaft by more than a thin host's radius.

use plinth_support::{SweepArm, SweepArmLanding, SweepRootFlare};

const H: f64 = 0.05;
const PLATE: f64 = -5.0;

/// A member seated at (0, 0, 3) along a normal tilted 37 degrees off straight
/// down, its tip 0.0872 mm (Deinonychus post 162's), its ball grown to 0.533.
fn seated(landing: SweepArmLanding, base: [f64; 3], grown: Option<f64>) -> SweepArm {
    let (sin, cos) = 37.0_f64.to_radians().sin_cos();
    SweepArm {
        seat_mm: [0.0, 0.0, 3.0],
        base_mm: base,
        tip_radius_mm: 0.0872,
        radius_profile_mm: Vec::new(),
        seat_normal_mm: [-sin, 0.0, -cos],
        seat_face_point_mm: Some([0.0, 0.0, 3.0]),
        landing,
        seats_into_model: true,
        head: None,
        contact_ball_radius_mm: grown,
    }
}

/// The distance from `point` to the line through `a` and `b`.
fn off_line(point: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = geom_kernel::vec3::sub(b, a);
    let t = geom_kernel::vec3::dot(geom_kernel::vec3::sub(point, a), d) / geom_kernel::vec3::dot(d, d);
    geom_kernel::vec3::dist(point, geom_kernel::vec3::add(a, geom_kernel::vec3::scale(d, t)))
}

fn graded(name: &str, arm: &SweepArm) {
    let flare = SweepRootFlare::registered(H).expect("the registered flare");
    let (top, bottom) = flare.shaft_line_of(arm, PLATE).expect("a seated member's shaft");
    // A lander 1 mm under the seat, on the planned line.
    let t = 1.0 / (arm.seat_mm[2] - arm.base_mm[2]);
    let planned: [f64; 3] =
        std::array::from_fn(|axis| arm.seat_mm[axis] + t * (arm.base_mm[axis] - arm.seat_mm[axis]));
    let before = off_line(planned, top, bottom);
    let offset =
        plinth_sweep::adapter::drawn_line_offset(&flare, arm, PLATE).expect("the drawn line is off the planned one");
    let followed: [f64; 3] = std::array::from_fn(|axis| {
        planned[axis] + (1.0 - t) * offset.0[axis] + t * offset.1[axis]
    });
    let after = off_line(followed, top, bottom);
    println!("{name}: the planned point stands {before:.4} mm off the drawn shaft; followed, {after:.2e} mm");
    // Sabotage (the rule before the fix: no offset for this member): the
    // lander stays on the planned line, further off the shaft than a thin
    // host's own radius (Deinonychus post 162: 0.087-0.128 mm).
    assert!(before > 0.15, "{name}: anti-vacuity, the grown ball moves the shaft: {before}");
    assert!(after < 1e-9, "{name}: the junction follows the drawn shaft: {after}");
}

#[test]
fn a_lander_on_a_grown_balls_shaft_ends_on_the_drawn_shaft() {
    graded(
        "landed member",
        &seated(SweepArmLanding::Landed, [0.6, 0.0, 0.0], Some(0.533)),
    );
    graded(
        "vertical plate post",
        &seated(SweepArmLanding::Plate, [0.0, 0.0, PLATE], Some(0.533)),
    );
}

#[test]
fn a_member_with_no_offset_draws_none() {
    // A straight-down seat with the owner's ball: the ball's centre is on
    // the planned line, and nothing moves, bit for bit.
    let flare = SweepRootFlare::registered(H).expect("the registered flare");
    let mut arm = seated(SweepArmLanding::Plate, [0.0, 0.0, PLATE], None);
    arm.seat_normal_mm = [0.0, 0.0, -1.0];
    assert_eq!(plinth_sweep::adapter::drawn_line_offset(&flare, &arm, PLATE), None);
}
