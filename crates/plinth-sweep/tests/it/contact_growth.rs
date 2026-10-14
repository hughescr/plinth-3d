//! **The real contact, and the ball grown until it reaches plan** (Craig
//! 2026-10-12: the tip is cut to the model mesh; area governs, the seat depth
//! stays). `plinth_sweep::contact_growth` on synthetic undersides:
//!
//! * a DOME (the lowest point of a curved figure, Bat_Cat_Owl tip 0's case):
//!   the ungrown ball takes less than its planned disc, the grown ball reaches
//!   it, and the grown ball goes no deeper into the model along the seat
//!   normal;
//! * a FLAT underside: the real contact is the planned disc, nothing grows
//!   (the growth never fires where it is not needed);
//! * a limit: the same dome with the growth limit at the ungrown ball is left
//!   short and named, never refused;
//! * a patch smaller than the ball (a knife edge, a thin feature): a larger
//!   ball finds no more surface, and the tip is named for it.

use plinth_core::{Finite, LayerPlan, PositiveFinite};
use plinth_mesh::Mesh;
use plinth_support::{
    lower_sweep_arms, BuildFrame, SupportPrimitive, SupportSegmentKind, SweepArm,
    SweepArmLanding,
};
use plinth_sweep::contact_growth::{grow, ShortReason};
use plinth_sweep::footprint_normal::{MeasureEnv, TipSite};

/// One pixel of area on the Saturn 4 Ultra grid, about (0.02 mm)².
const PIXEL_AREA_MM2: f64 = 0.0004;

/// The registered band's steep end, 57.5° from horizontal.
fn admitted_z() -> f64 {
    -57.5_f64.to_radians().cos()
}

/// A down-facing sheet `z = height(x, y)` over `[-half, half]²`, `cells` per
/// side, every face wound so its normal points down.
#[expect(clippy::cast_precision_loss, reason = "small test grids")]
fn sheet(half: f64, cells: usize, height: impl Fn(f64, f64) -> f64) -> Mesh {
    let mut verts = Vec::new();
    for j in 0..=cells {
        for i in 0..=cells {
            let x = -half + 2.0 * half * (i as f64) / (cells as f64);
            let y = -half + 2.0 * half * (j as f64) / (cells as f64);
            verts.push([x, y, height(x, y)]);
        }
    }
    let id = |i: usize, j: usize| u32::try_from(j * (cells + 1) + i).expect("small mesh");
    let mut tris = Vec::new();
    for j in 0..cells {
        for i in 0..cells {
            for tri in [
                [id(i, j), id(i + 1, j), id(i + 1, j + 1)],
                [id(i, j), id(i + 1, j + 1), id(i, j + 1)],
            ] {
                let [p, q, r] = tri.map(|v| verts[v as usize]);
                let u = [q[0] - p[0], q[1] - p[1]];
                let v = [r[0] - p[0], r[1] - p[1]];
                let nz = u[0] * v[1] - u[1] * v[0];
                tris.push(if nz < 0.0 { tri } else { [tri[0], tri[2], tri[1]] });
            }
        }
    }
    Mesh::new(verts, tris).expect("a valid sheet")
}

/// The face of `mesh` nearest `point`, and its outward unit normal.
fn site_at(mesh: &Mesh, point: [f64; 3]) -> TipSite {
    let verts = mesh.verts();
    let mut best = (f64::INFINITY, 0_u32, [0.0; 3]);
    for (id, tri) in mesh.tris().iter().enumerate() {
        let [a, b, c] = tri.map(|v| verts[v as usize]);
        let distance =
            geom_kernel::vec3::dist(point, geom_kernel::closest_on_tri(point, a, b, c));
        if distance < best.0 {
            let normal = geom_kernel::vec3::normalize(geom_kernel::vec3::cross(
                geom_kernel::vec3::sub(b, a),
                geom_kernel::vec3::sub(c, a),
            ))
            .expect("a proper face");
            best = (distance, u32::try_from(id).expect("small mesh"), normal);
        }
    }
    TipSite {
        face: best.1,
        landing_mm: point,
        facet_normal: best.2,
        max_normal_z: admitted_z(),
    }
}

fn arm(seat: [f64; 3]) -> SweepArm {
    SweepArm {
        seat_mm: seat,
        base_mm: [seat[0], seat[1], seat[2] - 10.0],
        tip_radius_mm: 0.3,
        radius_profile_mm: Vec::new(),
        seat_normal_mm: [0.0, 0.0, -1.0],
        seat_face_point_mm: Some(seat),
        landing: SweepArmLanding::Landed,
        seats_into_model: true,
        head: None,
        contact_ball_radius_mm: None,
    }
}

fn plan() -> LayerPlan {
    LayerPlan::new(-12.0, vec![0.05; 400]).expect("a uniform layer plan")
}

/// A dome of radius of curvature 1 mm, its lowest point at the origin.
fn dome() -> Mesh {
    sheet(1.5, 150, |x, y| (x * x + y * y) / 2.0)
}

#[test]
fn a_ball_under_a_dome_grows_until_its_real_contact_reaches_plan_at_the_same_depth() {
    let mesh = dome();
    let layers = plan();
    let env = MeasureEnv {
        mesh: &mesh,
        surface: None,
        layer_plan: &layers,
    };
    let seat = [0.0, 0.0, 0.0];
    let site = site_at(&mesh, seat);
    let (outcome, grown) = grow(&env, &[arm(seat)], &[Some(site)], 2.0, PIXEL_AREA_MM2);
    println!("{outcome:?}");
    assert_eq!(outcome.tips_measured, 1);
    // The path fires: the ungrown ball takes less than its plan.
    assert_eq!(outcome.short_ungrown, 1, "{outcome:?}");
    assert!(outcome.real_ungrown_mm2 + PIXEL_AREA_MM2 < outcome.planned_mm2);
    // The grown ball reaches it.
    assert_eq!(outcome.grown, 1, "{outcome:?}");
    assert!(outcome.short.is_empty(), "{outcome:?}");
    assert!(outcome.real_drawn_mm2 + PIXEL_AREA_MM2 >= outcome.planned_mm2);
    let radius = grown[0].expect("a grown ball");
    // THE SEAT DEPTH STAYS: the grown ball's deepest point along the seat
    // normal is the ungrown ball's.
    let discs = plinth_support::tip_contact_discs(
        &[
            arm(seat),
            SweepArm {
                contact_ball_radius_mm: Some(radius),
                ..arm(seat)
            },
        ],
        &layers,
    )
    .expect("discs");
    let (ungrown, drawn) = (discs[0].expect("a disc"), discs[1].expect("a disc"));
    assert!(drawn.ball_radius_mm > ungrown.ball_radius_mm);
    // The point furthest INTO the model: against the outward normal.
    let deepest = |disc: &plinth_support::TipContactDisc| {
        disc.ball_centre_mm[2] - disc.ball_radius_mm * disc.outward_normal[2]
    };
    assert!(
        (deepest(&ungrown) - deepest(&drawn)).abs() < 1e-12,
        "the grown ball reaches {} where the ungrown one reached {}",
        deepest(&drawn),
        deepest(&ungrown)
    );
    // The PLAN is the ungrown ball's disc whatever the drawn ball is.
    assert!((ungrown.radius_mm - drawn.radius_mm).abs() < 1e-15);
}

/// The same mesh as a triangle SOUP: every corner its own vertex index, as a
/// model read from an STL arrives (Bat_Cat_Owl's footprint was one face).
fn soup(mesh: &Mesh) -> Mesh {
    let verts: Vec<[f64; 3]> = mesh
        .tris()
        .iter()
        .flat_map(|tri| tri.map(|v| mesh.verts()[v as usize]))
        .collect();
    let tris: Vec<[u32; 3]> = (0..mesh.tris().len())
        .map(|face| {
            let base = u32::try_from(3 * face).expect("small mesh");
            [base, base + 1, base + 2]
        })
        .collect();
    Mesh::new(verts, tris).expect("a soup")
}

/// The real contact joins faces by POSITION: a triangle soup measures the same
/// contact as the indexed mesh. (Joined by index alone, the soup's footprint
/// is the seat triangle and nothing else.)
#[test]
fn a_triangle_soup_measures_the_same_real_contact_as_the_indexed_mesh() {
    let indexed = dome();
    let loose = soup(&indexed);
    let layers = plan();
    let seat = [0.0, 0.0, 0.0];
    let measure = |mesh: &Mesh| {
        let env = MeasureEnv {
            mesh,
            surface: None,
            layer_plan: &layers,
        };
        grow(&env, &[arm(seat)], &[Some(site_at(mesh, seat))], 2.0, PIXEL_AREA_MM2).0
    };
    let (a, b) = (measure(&indexed), measure(&loose));
    assert!(a.real_ungrown_mm2 > 0.5 * a.planned_mm2, "{a:?}");
    assert!(
        (a.real_ungrown_mm2 - b.real_ungrown_mm2).abs() < 1e-12,
        "indexed {a:?}, soup {b:?}"
    );
    assert!((a.real_drawn_mm2 - b.real_drawn_mm2).abs() < 1e-12);
}

#[test]
fn a_flat_underside_takes_its_whole_planned_disc_and_grows_nothing() {
    let mesh = sheet(1.5, 60, |_, _| 0.0);
    let layers = plan();
    let env = MeasureEnv {
        mesh: &mesh,
        surface: None,
        layer_plan: &layers,
    };
    let seat = [0.013, -0.021, 0.0];
    let site = site_at(&mesh, seat);
    let (outcome, grown) = grow(&env, &[arm(seat)], &[Some(site)], 2.0, PIXEL_AREA_MM2);
    assert_eq!(outcome.tips_measured, 1);
    assert_eq!(outcome.short_ungrown, 0, "{outcome:?}");
    assert_eq!(outcome.grown, 0, "{outcome:?}");
    assert!(grown[0].is_none());
    assert!(
        (outcome.real_ungrown_mm2 - outcome.planned_mm2).abs() < 1e-9,
        "a flat face takes exactly the planned disc: {outcome:?}"
    );
}

#[test]
fn a_limit_leaves_the_dome_tip_short_and_named() {
    let mesh = dome();
    let layers = plan();
    let env = MeasureEnv {
        mesh: &mesh,
        surface: None,
        layer_plan: &layers,
    };
    let seat = [0.0, 0.0, 0.0];
    let site = site_at(&mesh, seat);
    let ungrown = plinth_support::tip_contact_discs(&[arm(seat)], &layers).expect("discs")[0]
        .expect("a disc")
        .ball_radius_mm;
    let (outcome, grown) = grow(&env, &[arm(seat)], &[Some(site)], ungrown, PIXEL_AREA_MM2);
    assert!(grown[0].is_none(), "{outcome:?}");
    assert_eq!(outcome.short.len(), 1, "{outcome:?}");
    assert_eq!(outcome.short[0].reason, ShortReason::BallLimit);
    assert!(outcome.short[0].real_mm2 < outcome.short[0].planned_mm2);
}

/// Whether `point` lies inside one of `primitives` other than its contact
/// sphere: a frustum (radius interpolated along its axis) or a sphere. Read
/// from the lowering's own output, not from the rule under test.
fn inside_other_solid(primitives: &[SupportPrimitive], point: [f64; 3]) -> bool {
    primitives.iter().any(|piece| {
        if piece.segment_kind == SupportSegmentKind::ContactSphere {
            return false;
        }
        let a = piece.start_mm.map(Finite::get);
        let b = piece.end_mm.map(Finite::get);
        let axis = geom_kernel::vec3::sub(b, a);
        let length = geom_kernel::vec3::norm(axis);
        if piece.sphere_geometry.is_some() || length <= 1e-12 {
            let centre: [f64; 3] = std::array::from_fn(|i| 0.5 * (a[i] + b[i]));
            return geom_kernel::vec3::dist(point, centre) <= piece.start_radius_mm.get();
        }
        let t = geom_kernel::vec3::dot(geom_kernel::vec3::sub(point, a), axis) / (length * length);
        if !(0.0..=1.0).contains(&t) {
            return false;
        }
        let radius = piece.start_radius_mm.get()
            + t * (piece.end_radius_mm.get() - piece.start_radius_mm.get());
        let foot = geom_kernel::vec3::add(a, geom_kernel::vec3::scale(axis, t));
        geom_kernel::vec3::dist(point, foot) <= radius
    })
}

/// The arm lowered alone by the production lowering, its drawn ball's lowest
/// point and radius, and how many pole collars the lowering drew.
fn drawn_ball_pole(arm: &SweepArm, layers: &LayerPlan) -> (Vec<SupportPrimitive>, [f64; 3], f64, usize) {
    let frame = BuildFrame::new(
        Finite::new(layers.z0_mm()).expect("finite plate"),
        PositiveFinite::new(1.0).expect("positive lift"),
    );
    let lowering = lower_sweep_arms(std::slice::from_ref(arm), frame, layers, 1_000, 1 << 24)
        .expect("the lowering refuses only about the request");
    let collars = lowering.tip_pole_collars;
    let primitives = lowering.primitives;
    let ball = primitives
        .iter()
        .find(|piece| piece.segment_kind == SupportSegmentKind::ContactSphere)
        .expect("a seated tip draws its ball");
    let (a, b) = (ball.start_mm.map(Finite::get), ball.end_mm.map(Finite::get));
    let radius = ball.start_radius_mm.get();
    let pole = [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1]), 0.5 * (a[2] + b[2]) - radius];
    (primitives, pole, radius, collars)
}

/// **A grown ball prints from its own shaft** (joint review P1, 2026-10-12).
/// The dome tip seated 45° up the dome's flank, on an arm along the face
/// normal there (inside the lean law): it is short of plan, as the dome's
/// lowest tip is, and growing it moves the ball out along the normal until
/// its lowest point leaves the 0.3 mm taper and cures over nothing
/// (Bat_Cat_Owl's 0.270 mm ball on a 0.097 mm taper). The growth stops at the
/// largest ball whose lowest point the shaft still holds, named, and the
/// lowering's own records hold that point; a ball a little larger does not.
#[test]
fn a_grown_ball_on_an_oblique_shaft_keeps_its_lowest_point_inside_its_shaft() {
    let mesh = dome();
    let layers = plan();
    let env = MeasureEnv {
        mesh: &mesh,
        surface: None,
        layer_plan: &layers,
    };
    // On z = r^2 / 2 the slope is r, so the face 45 degrees off horizontal is
    // at r = tan 45; its outward normal leans 45 degrees from straight down.
    let r = 45.0_f64.to_radians().tan();
    let seat = [r, 0.0, r * r / 2.0];
    let normal = geom_kernel::vec3::normalize([r, 0.0, -1.0]).expect("a normal");
    let oblique = SweepArm {
        base_mm: geom_kernel::vec3::add(seat, geom_kernel::vec3::scale(normal, 10.0)),
        seat_normal_mm: normal,
        ..arm(seat)
    };
    let site = site_at(&mesh, seat);
    let (outcome, grown) = grow(
        &env,
        std::slice::from_ref(&oblique),
        &[Some(site)],
        2.0,
        PIXEL_AREA_MM2,
    );
    println!("{outcome:?}; drawn {grown:?}");
    // The path fires: the tip is short, its shaft bounds its growth, and the
    // bound is what leaves it short (a growth it wanted and did not get).
    assert_eq!(outcome.short_ungrown, 1, "{outcome:?}");
    assert_eq!(outcome.carried_bound, 1, "{outcome:?}");
    assert_eq!(outcome.stopped_by_overhang, 1, "{outcome:?}");
    assert_eq!(outcome.short.len(), 1, "{outcome:?}");
    assert_eq!(outcome.short[0].reason, ShortReason::Overhang);
    let drawn = grown[0].expect("the tip still grows up to its shaft's bound");
    let draw = |radius: f64| {
        drawn_ball_pole(
            &SweepArm {
                contact_ball_radius_mm: Some(radius),
                ..oblique.clone()
            },
            &layers,
        )
    };
    let (pieces, pole, radius, collars) = draw(drawn);
    assert!((radius - drawn).abs() < 1e-9, "the file draws the grown ball");
    assert_eq!(collars, 0, "at the bound the shaft itself holds the lowest point");
    assert!(
        inside_other_solid(&pieces, pole),
        "the drawn ball's lowest point {pole:?} is outside its shaft"
    );
    // The bound is tight: one percent larger, the bare shaft no longer holds
    // the lowest point, and the lowering's pole collar
    // (`plinth_support::lower::pole_collar`, joint review 2026-10-13) is what
    // holds it then.
    let (pieces, pole, _, collars) = draw(1.01 * drawn);
    assert_eq!(collars, 1, "a ball past the bound needs the collar, so the bound is tight");
    assert!(
        inside_other_solid(&pieces, pole),
        "the collar holds the lowest point {pole:?} of a ball past the bound"
    );
}

#[test]
fn a_patch_smaller_than_the_ball_is_named_when_a_larger_ball_finds_no_more_surface() {
    // A flat patch 0.24 mm square: the planned disc does not fit on it.
    let mesh = sheet(0.12, 12, |_, _| 0.0);
    let layers = plan();
    let env = MeasureEnv {
        mesh: &mesh,
        surface: None,
        layer_plan: &layers,
    };
    let seat = [0.0, 0.0, 0.0];
    let site = site_at(&mesh, seat);
    let (outcome, _) = grow(&env, &[arm(seat)], &[Some(site)], 2.0, PIXEL_AREA_MM2);
    assert_eq!(outcome.short.len(), 1, "{outcome:?}");
    assert_eq!(outcome.short[0].reason, ShortReason::NoMoreSurface, "{outcome:?}");
    assert!((outcome.short[0].real_mm2 - 0.24 * 0.24).abs() < 1e-9, "{outcome:?}");
}
