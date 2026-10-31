//! Controls for the H1 and H2 measurement on shapes whose truth is known (design 7.2,
//! "Validated on"). Test code only: the truths are exact formulas or a fine grid written here.

use crate::h12;
use crate::patch::{self, FUSE_MM};
use crate::report::Status;
use crate::scene::Scene;
use crate::testkit::{self as kit, test_limits};
use crate::tri::Tri;
use crate::v3::{V, add, cross, dot, frame, mul, sub, unit};

/// A small deterministic generator (SplitMix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// About 1,000 triangles tiling the square `[-6, 6]^2` at height `z = 0`, vertices jittered,
/// facing down (outward normal -z).
fn plane_down(rng: &mut Rng) -> Vec<Tri> {
    let n = 22usize;
    let pt = |i: usize, j: usize, rng: &mut Rng| -> V {
        let edge = i == 0 || j == 0 || i == n || j == n;
        let jitter = if edge { 0.0 } else { 0.25 };
        [
            -6.0 + 12.0 * (i as f64 + jitter * (rng.next() - 0.5)) / n as f64,
            -6.0 + 12.0 * (j as f64 + jitter * (rng.next() - 0.5)) / n as f64,
            0.0,
        ]
    };
    let grid: Vec<Vec<V>> = (0..=n)
        .map(|i| (0..=n).map(|j| pt(i, j, rng)).collect())
        .collect();
    let mut tris = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let (a, b, c, d) = (
                grid[i][j],
                grid[i + 1][j],
                grid[i + 1][j + 1],
                grid[i][j + 1],
            );
            // Clockwise seen from +z: outward normal -z.
            tris.push([a, c, b]);
            tris.push([a, d, c]);
        }
    }
    tris
}

/// The first 12 of the sweep: one pass over the radii, twice. The whole sweep is
/// `e2e_a_sphere_on_a_tessellated_plane_is_bracketed_over_ninety_cases` (L2 only).
#[test]
fn a_sphere_on_a_tessellated_plane_is_bracketed_within_the_stated_width() {
    sphere_on_plane_sweep(12);
}

/// The whole 90-case sweep (left out of L1 by its name, run by L2).
#[test]
fn e2e_a_sphere_on_a_tessellated_plane_is_bracketed_over_ninety_cases() {
    sphere_on_plane_sweep(90);
}

fn sphere_on_plane_sweep(count: usize) {
    let mut rng = Rng(12345);
    let radii = [0.05f32, 0.1, 0.2, 0.39, 0.42, 1.0];
    let (mut worst_excess, mut worst_width, mut cases) = (0.0f64, 0.0f64, 0);
    for k in 0..count {
        let r = radii[k % radii.len()];
        let ratio = 0.99 * rng.next();
        #[allow(clippy::cast_possible_truncation)]
        let s = (f64::from(r) * ratio) as f32;
        let (cx, cy) = (-2.0 + 4.0 * rng.next(), -2.0 + 4.0 * rng.next());
        #[allow(clippy::cast_possible_truncation)]
        let (cx, cy) = (cx as f32, cy as f32);
        // Centre below the plane by `s`: the ball pokes `r - s` into it.
        let ball = kit::part(0, 7, [cx, cy, -s - r], [cx, cy, -s + r], r, r);
        assert!(ball.is_valid());
        let scene = Scene::from_parts(&test_limits(), plane_down(&mut rng), [0.0; 3], vec![ball]);
        let out = patch::measure(&scene);
        let truth = std::f64::consts::PI * (f64::from(r).powi(2) - f64::from(s).powi(2));
        let a = &out.reps[0];
        assert!(
            a.max_certain_lo <= truth + 1e-9 && a.max_hi >= truth - 1e-9,
            "r {r} s {s}: [{}, {}] vs {truth}",
            a.max_certain_lo,
            a.max_hi
        );
        worst_excess = worst_excess.max(a.max_hi - truth);
        worst_width = worst_width.max(a.max_hi - a.max_certain_lo);
        // The inscribed mesh is inside the analytic solid.
        assert!(out.reps[1].max_hi <= a.max_hi + 1e-9);
        cases += 1;
    }
    println!(
        "{cases} cases; largest excess {worst_excess:.2e} mm2, widest interval {worst_width:.2e} mm2"
    );
    assert!(worst_excess <= 0.010, "excess {worst_excess}");
    assert!(worst_width <= 0.010, "width {worst_width}");
}

/// A ball whose analytic patch on a plane is `target` mm2, at radius `r`.
fn ball_with_patch(r: f32, target: f64) -> crate::parts::Part {
    let s2 = f64::from(r).powi(2) - target / std::f64::consts::PI;
    #[allow(clippy::cast_possible_truncation)]
    let s = s2.sqrt() as f32;
    kit::part(0, 7, [0.0, 0.0, -s - r], [0.0, 0.0, -s + r], r, r)
}

fn plate_two_triangles() -> Vec<Tri> {
    vec![
        [[-5.0, -5.0, 0.0], [5.0, 5.0, 0.0], [5.0, -5.0, 0.0]],
        [[-5.0, -5.0, 0.0], [-5.0, 5.0, 0.0], [5.0, 5.0, 0.0]],
    ]
}

#[test]
fn the_limit_is_decided_on_the_written_mesh_not_on_the_analytic_patch() {
    // `ball_with_patch` names the ANALYTIC patch area; the written 16-gon mesh has about 96.3%
    // of it. A ball whose analytic patch is 0.505 mm2 (over the limit) has a written patch of
    // 0.486 mm2: the verdict is the written mesh's (a pass), and the analytic number is only
    // information.
    for (target, want, over_in_analytic) in [
        (0.48, Status::Pass, false),
        (0.49, Status::Pass, false),
        (0.505, Status::Pass, true),
        (0.54, Status::Violation, true),
    ] {
        let scene = Scene::from_parts(
            &test_limits(),
            plate_two_triangles(),
            [0.0; 3],
            vec![ball_with_patch(0.45, target)],
        );
        let out = patch::measure(&scene);
        let o = h12::h1(&scene, &out);
        assert_eq!(o.status, want, "target {target}: {o:?}");
        if over_in_analytic {
            assert!(out.analytic().max_certain_lo > 0.5, "{:?}", out.analytic());
        }
        if want == Status::Pass && over_in_analytic {
            assert!(out.mesh().max_hi < 0.5, "{:?}", out.mesh());
            let info = o.detail.get("analytic_information").expect("information");
            let Some(crate::json::Json::Num(lo)) = info.get("largest_lower_mm2") else {
                panic!("{info:?}");
            };
            assert!(*lo > 0.5, "{lo}");
        }
    }
}

#[test]
fn a_written_patch_within_the_bracket_of_the_limit_is_not_a_pass() {
    // Bisect the analytic target to the first written patch whose upper bound is over the
    // limit: its interval still reaches under it, so the rule says unverified, not pass.
    let h1_of = |target: f64| {
        let scene = Scene::from_parts(
            &test_limits(),
            plate_two_triangles(),
            [0.0; 3],
            vec![ball_with_patch(0.45, target)],
        );
        let out = patch::measure(&scene);
        (out.mesh().max_hi, h12::h1(&scene, &out))
    };
    let (mut lo, mut hi) = (0.50, 0.54);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if h1_of(mid).0 > 0.5 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let (upper, o) = h1_of(hi);
    assert!(upper > 0.5 && upper < 0.5 + 1e-3, "{upper}");
    assert_eq!(o.status, Status::Unverified, "{o:?}");
}

#[test]
fn the_analytic_and_the_written_mesh_patches_differ_by_more_than_the_width_at_048() {
    // Information: a 0.48 mm2 analytic patch, plane normal to the ball axis, is at least 0.0122
    // mm2 larger than the patch of the written 16-gon mesh, so the two intervals are kept
    // apart; the analytic one decides nothing.
    let plate = vec![
        [[-5.0, -5.0, 0.0], [5.0, 5.0, 0.0], [5.0, -5.0, 0.0]],
        [[-5.0, -5.0, 0.0], [-5.0, 5.0, 0.0], [5.0, 5.0, 0.0]],
    ];
    let scene = Scene::from_parts(
        &test_limits(),
        plate,
        [0.0; 3],
        vec![ball_with_patch(0.45, 0.48)],
    );
    let out = patch::measure(&scene);
    let (a, m) = (&out.reps[0], &out.reps[1]);
    let diff = a.max_certain_lo - m.max_hi;
    println!(
        "analytic [{}, {}] mesh [{}, {}] gap {diff}",
        a.max_certain_lo, a.max_hi, m.max_certain_lo, m.max_hi
    );
    assert!(
        diff >= 0.0122,
        "analytic lower bound minus mesh upper bound: {diff}"
    );
    // The two intervals are separate: the combined interval is not narrower than their gap.
    assert!(a.max_hi - m.max_certain_lo > a.max_hi - a.max_certain_lo);
}

#[test]
fn a_cylinder_cut_by_a_tilted_plane_has_the_ellipse_area() {
    for deg in [0.0f64, 30.0, 45.0, 60.0] {
        // A cylinder of radius 0.3 whose axis is `deg` from the plane's normal, crossing it.
        let th = deg.to_radians();
        let axis = [th.sin(), 0.0, -th.cos()];
        let (a, b) = (mul(axis, -3.0), mul(axis, 3.0));
        #[allow(clippy::cast_possible_truncation)]
        let cyl = kit::part(
            0,
            3,
            [a[0] as f32, a[1] as f32, a[2] as f32],
            [b[0] as f32, b[1] as f32, b[2] as f32],
            0.3,
            0.3,
        );
        let plate = vec![
            [[-5.0, -5.0, 0.0], [5.0, 5.0, 0.0], [5.0, -5.0, 0.0]],
            [[-5.0, -5.0, 0.0], [-5.0, 5.0, 0.0], [5.0, 5.0, 0.0]],
        ];
        let scene = Scene::from_parts(&test_limits(), plate, [0.0; 3], vec![cyl]);
        let out = patch::measure(&scene);
        let truth = std::f64::consts::PI * 0.09 / th.cos();
        let a = &out.reps[0];
        assert!(
            a.max_certain_lo <= truth + 1e-9 && a.max_hi >= truth - 1e-9,
            "{deg} deg: [{}, {}] vs {truth}",
            a.max_certain_lo,
            a.max_hi
        );
        assert!(a.max_hi - a.max_certain_lo < 0.005, "{a:?}");
    }
}

/// Patch area of a ball on a model by a grid, for shapes without a closed form.
fn grid_area(model: &[Tri], c: V, r: f64, step: f64) -> f64 {
    let mut total = 0.0;
    for t in model {
        let n = unit(cross(sub(t[1], t[0]), sub(t[2], t[0])));
        let s = dot(n, sub(c, t[0]));
        if s.abs() >= r {
            continue;
        }
        let rho = (r * r - s * s).sqrt();
        let q = sub(c, mul(n, s));
        let (e1, e2) = frame(n);
        let k = (rho / step).ceil() as i64;
        for i in -k..=k {
            for j in -k..=k {
                let p = add(
                    q,
                    add(
                        mul(e1, (i as f64 + 0.5) * step),
                        mul(e2, (j as f64 + 0.5) * step),
                    ),
                );
                let inside_tri =
                    (0..3).all(|e| dot(cross(sub(t[(e + 1) % 3], t[e]), sub(p, t[e])), n) >= 0.0);
                if inside_tri && dot(sub(p, c), sub(p, c)) <= r * r {
                    total += step * step;
                }
            }
        }
    }
    total
}

/// A downward-facing wedge: two big triangles meeting at the line x = 0, z = 0 with the given
/// angle between the faces measured through the model (180 is flat).
fn wedge(angle_deg: f64) -> Vec<Tri> {
    let half = (180.0 - angle_deg).to_radians();
    // Face A lies in z = 0 for x < 0; face B turns up (into the model) by `half` at x = 0.
    let (a0, a1) = ([-6.0, -6.0, 0.0], [-6.0, 6.0, 0.0]);
    let (o0, o1) = ([0.0, -6.0, 0.0], [0.0, 6.0, 0.0]);
    let d = [6.0 * half.cos(), 0.0, 6.0 * half.sin()];
    let (b0, b1) = (add(o0, d), add(o1, d));
    vec![[a0, o1, o0], [a0, a1, o1], [o0, b1, b0], [o0, o1, b1]]
}

#[test]
fn a_sphere_on_a_convex_edge_or_in_a_concave_recess_matches_a_fine_grid() {
    // 115, 90 and 65 degrees through the model are convex edges; 245, 270 and 295 are the
    // concave recesses of the same faces.
    for angle in [115.0, 90.0, 65.0, 245.0, 270.0, 295.0] {
        let model = wedge(angle);
        // The centre sits below the edge, a little to the side, so the ball reaches both faces.
        let c = [0.1f32, 0.0, -0.15];
        let r = 0.4f32;
        let ball = kit::part(0, 7, [c[0], c[1], c[2] - r], [c[0], c[1], c[2] + r], r, r);
        let scene = Scene::from_parts(&test_limits(), model.clone(), [0.0; 3], vec![ball]);
        let out = patch::measure(&scene);
        let truth = grid_area(
            &model,
            [f64::from(c[0]), f64::from(c[1]), f64::from(c[2])],
            f64::from(r),
            0.002,
        );
        let a = &out.reps[0];
        assert!(
            a.max_certain_lo - 0.004 <= truth && truth <= a.max_hi + 0.004,
            "{angle} deg: [{}, {}] vs grid {truth}",
            a.max_certain_lo,
            a.max_hi
        );
        assert!(a.max_hi - a.max_certain_lo < 0.01);
    }
}

#[test]
fn two_parallel_sheets_and_a_concave_recess_give_one_patch_per_sheet_or_one_across() {
    // Two sheets 0.3 mm apart (a wall with two faces), a ball between them touching both.
    let sheet_a = vec![
        [[-5.0, -5.0, 0.0], [5.0, 5.0, 0.0], [5.0, -5.0, 0.0]],
        [[-5.0, -5.0, 0.0], [-5.0, 5.0, 0.0], [5.0, 5.0, 0.0]],
    ];
    let mut model = sheet_a;
    // The upper sheet faces up (outward normal +z) at z = -0.3.
    model.push([[-5.0, -5.0, -0.3], [5.0, -5.0, -0.3], [5.0, 5.0, -0.3]]);
    model.push([[-5.0, -5.0, -0.3], [5.0, 5.0, -0.3], [-5.0, 5.0, -0.3]]);
    let r = 0.25f32;
    let ball = kit::part(0, 7, [0.0, 0.0, -0.15 - r], [0.0, 0.0, -0.15 + r], r, r);
    let scene = Scene::from_parts(&test_limits(), model, [0.0; 3], vec![ball]);
    let out = patch::measure(&scene);
    let a = &out.reps[0];
    // The ball reaches both sheets: two separate patches of the same chain.
    assert_eq!(a.comps.len(), 2, "{a:?}");
    let each = std::f64::consts::PI * (0.25f64.powi(2) - 0.15f64.powi(2));
    for c in &a.comps {
        assert!(
            (c.hi - each).abs() < 2e-3 && (c.certain_lo - each).abs() < 2e-3,
            "{c:?} vs {each}"
        );
    }
}

#[test]
fn fusing_is_decided_at_ten_micrometres_for_patches_on_a_tessellated_plane() {
    let mut rng = Rng(777);
    for (gap, fused) in [(0.0f64, true), (0.004, true), (0.012, false), (0.05, false)] {
        let r = 0.2f32;
        let x2 = 2.0 * f64::from(r) + gap;
        #[allow(clippy::cast_possible_truncation)]
        let (x1, x2) = (-0.5f32, (x2 - 0.5) as f32);
        let b1 = kit::part(0, 7, [x1, 0.0, -r], [x1, 0.0, r], r, r);
        let b2 = kit::part(1, 7, [x2, 0.0, -r], [x2, 0.0, r], r, r);
        let scene = Scene::from_parts(&test_limits(), plane_down(&mut rng), [0.0; 3], vec![b1, b2]);
        let out = patch::measure(&scene);
        let o = h12::h2(&scene, &out);
        let want = if fused {
            Status::Violation
        } else {
            Status::Pass
        };
        assert_eq!(o.status, want, "gap {gap}: {o:?}");
    }
    let _ = FUSE_MM;
}
