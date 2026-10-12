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

use plinth_core::LayerPlan;
use plinth_mesh::Mesh;
use plinth_support::{SweepArm, SweepArmLanding};
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
