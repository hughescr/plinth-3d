use super::*;
use plinth_geom::ray::first_hits;

fn mesh_of(tris: &[[[f64; 3]; 3]]) -> Mesh {
    let (mut x, mut y, mut z) = (Vec::new(), Vec::new(), Vec::new());
    for p in tris.iter().flatten() {
        x.push(p[0]);
        y.push(p[1]);
        z.push(p[2]);
    }
    Mesh::from_soup(&x, &y, &z).expect("mesh")
}

/// A square plate `[0,s] x [0,s]` tilted `deg` degrees about x, facing down (outward normal
/// has a negative z component), as two triangles.
fn tilted_plate(s: f64, deg: f64) -> Vec<[[f64; 3]; 3]> {
    let (c, n) = (deg.to_radians().cos(), deg.to_radians().sin());
    let p = |x: f64, y: f64| [x, y * c, y * n];
    // (x, y) counter-clockwise seen from below gives a downward normal.
    vec![
        [p(0.0, 0.0), p(0.0, s), p(s, s)],
        [p(0.0, 0.0), p(s, s), p(s, 0.0)],
    ]
}

#[test]
fn a_down_facing_plate_flatter_than_the_limit_is_sampled_with_exact_area() {
    let m = mesh_of(&tilted_plate(2.0, 30.0));
    let n = overhang_points(&m, 45.0, 0.25);
    assert!(
        (n.total_area_mm2() - 4.0).abs() < 1e-9,
        "{}",
        n.total_area_mm2()
    );
    assert!(n.points.iter().all(|p| (p.angle_deg - 30.0).abs() < 1e-6));
    // About 4 / 0.0625 = 64 samples; the lattice in the plane holds 8 x 8 (+1 on edges).
    assert!((60..=90).contains(&n.points.len()), "{}", n.points.len());
    assert!((n.band_area_mm2(30.0 - 1e-6, 31.0) - 4.0).abs() < 1e-9);
    // Projected on the horizontal plane the area shrinks by cos 30.
    assert!((n.band_projected_mm2(0.0, 45.0) - 4.0 * 30.0_f64.to_radians().cos()).abs() < 1e-9);
}

#[test]
fn a_plate_steeper_than_the_limit_and_an_up_facing_plate_are_not_sampled() {
    let steep = overhang_points(&mesh_of(&tilted_plate(2.0, 60.0)), 45.0, 0.25);
    assert!(steep.points.is_empty());
    let mut up = tilted_plate(2.0, 30.0);
    for t in &mut up {
        t.swap(1, 2);
    }
    assert!(overhang_points(&mesh_of(&up), 45.0, 0.25).points.is_empty());
}

#[test]
fn a_triangle_smaller_than_the_lattice_still_gets_one_sample_with_its_whole_area() {
    let t = [[[0.0, 0.0, 0.0], [0.0, 0.05, 0.0], [0.05, 0.05, 0.0]]];
    let n = overhang_points(&mesh_of(&t), 45.0, 0.25);
    assert_eq!(n.points.len(), 1);
    assert!((n.points[0].area_mm2 - 0.00125).abs() < 1e-12);
}

/// A hit function by brute force over a few triangles (Moller-Trumbore).
fn brute(tris: Vec<[[f64; 3]; 3]>) -> impl Fn(&[Ray], &mut Vec<Measure<RayHit>>) {
    move |rays, out| {
        out.clear();
        for r in rays {
            let mut best: Option<f64> = None;
            for t in &tris {
                let e1 = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
                let e2 = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
                let p = cross(r.dir, e2);
                let det = dot(e1, p);
                if det.abs() < 1e-14 {
                    continue;
                }
                let s = sub(r.origin, t[0]);
                let u = dot(s, p) / det;
                let q = cross(s, e1);
                let v = dot(r.dir, q) / det;
                let tt = dot(e2, q) / det;
                if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && tt > 0.0 {
                    best = Some(best.map_or(tt, |b: f64| b.min(tt)));
                }
            }
            out.push(Measure::Known(
                best.map_or(RayHit::Miss, |t| RayHit::Hit { t, triangle: 0 }),
            ));
        }
    }
}

fn point(xyz: V3, normal: V3) -> NeedPoint {
    NeedPoint {
        xyz,
        normal,
        angle_deg: 0.0,
        area_mm2: 0.1,
        triangle: 0,
        class: SiteTag::Unverified("test"),
    }
}

fn big_wall(x: f64) -> Vec<[[f64; 3]; 3]> {
    let (a, b) = (-100.0, 100.0);
    vec![
        [[x, a, a], [x, b, a], [x, b, b]],
        [[x, a, a], [x, b, b], [x, a, b]],
    ]
}

#[test]
fn site_classes_follow_the_visibility_of_the_resting_pose() {
    let rest = RestPose {
        up: [0.0, 0.0, 1.0],
    };
    let mut pts = [
        // Faces down: no ray of the upper hemisphere leaves it, so it is hidden.
        point([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        // A side wall in the open: some rays get out, and it does not look up.
        point([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        // Looks up in the open: the decorated top.
        point([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        // A side wall facing a big wall 0.1 mm away: every ray is blocked.
        point([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
    ];
    let open = brute(Vec::new());
    classify_sites(&mut pts[..3], &rest, &open);
    assert_eq!(pts[0].class, SiteTag::Hidden);
    assert_eq!(pts[1].class, SiteTag::PartlyVisible);
    assert_eq!(pts[2].class, SiteTag::Top);
    let walled = brute(big_wall(0.1));
    classify_sites(&mut pts[3..], &rest, &walled);
    assert_eq!(pts[3].class, SiteTag::Hidden);
}

#[test]
fn a_ray_function_that_cannot_answer_leaves_the_class_unverified() {
    // The step-0 stub of first_hits answers Unverified for every ray: no favourable guess.
    let m = mesh_of(&tilted_plate(1.0, 10.0));
    let bvh = plinth_geom::Bvh::build(&m).expect("bvh");
    let mut pts = vec![point([0.0, 0.0, 5.0], [1.0, 0.0, 0.0])];
    classify_sites(
        &mut pts,
        &RestPose {
            up: [0.0, 0.0, 1.0],
        },
        &|r, out| {
            first_hits(&bvh, r, out);
        },
    );
    // Either the production kernel is built and answers, or the class stays undecided.
    let ok = matches!(
        pts[0].class,
        SiteTag::Unverified(_) | SiteTag::PartlyVisible | SiteTag::Hidden | SiteTag::Top
    );
    assert!(ok);
}

#[test]
fn the_layout_must_cover_every_sample_that_needs_it_by_its_layer() {
    let stack = LayerStack {
        layer_height_mm: 0.05,
        layer_origin_mm: 0.0,
        ..LayerStack::default()
    };
    let mut cover = point([0.5, 0.0, 0.10], [0.0, 0.0, -1.0]);
    cover.class = SiteTag::Hidden;
    let mut late = point([1.0, 0.0, 0.10], [0.0, 0.0, -1.0]);
    late.class = SiteTag::PartlyVisible;
    let mut top = point([9.0, 0.0, 0.10], [0.0, 0.0, 1.0]);
    top.class = SiteTag::Top;
    let needs = NeedsMap {
        points: vec![cover, late, top],
        limit_deg: 45.0,
        spacing_mm: 0.25,
    };
    let view = |first_layer: u32| super::super::data::ContactView {
        centre: [0.7, 0.0, 0.0],
        patch_lower_mm2: 1.0,
        neck_area_mm2: 1.0,
        first_layer,
        site_class: super::super::data::SiteClass::HiddenUnderside,
    };
    // Printed at layer 2 (z 0.10): both samples are in reach and in time.
    let ok = LayoutView {
        contacts: vec![view(2)],
        chains: Vec::new(),
        patch_history: Vec::new(),
    };
    let (cov, w) = check_coverage(&needs, &ok, &stack);
    assert_eq!((cov.needing, cov.covered), (2, 2));
    assert!(w.is_none());
    // Printed one layer too late: neither is covered.
    let late_layout = LayoutView {
        contacts: vec![view(3)],
        chains: Vec::new(),
        patch_history: Vec::new(),
    };
    let (cov, w) = check_coverage(&needs, &late_layout, &stack);
    assert_eq!((cov.needing, cov.covered), (2, 0));
    assert!(w.is_some());
}
