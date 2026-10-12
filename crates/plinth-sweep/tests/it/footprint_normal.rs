//! The footprint normal (Craig ruling 2026-10-05): a support tip's seat normal
//! is the area-weighted average of the model's outward normal over the patch
//! its ball touches, not the normal of one facet.
//!
//! These tests drive [`plinth_sweep::footprint_normal`] directly, on meshes
//! small enough that every expected number is derived by hand:
//!
//! * `triangle_disc_area_*`: the closed-form kernel against exact areas
//!   (half disc, quarter disc, a triangle inside the disc, a disc inside a
//!   triangle, a square split along either diagonal);
//! * `the_normal_is_the_area_weighted_admitted_patch`: two admitted planes
//!   meeting at a ridge, a ball over the ridge, `n = normalize(a1·n1 + a2·n2)`
//!   to 1e-12, and the same answer for two different tessellations of the
//!   same surface (the old single-facet normal differs between them);
//! * `a_wall_in_the_ball_is_not_part_of_the_footprint`: admission;
//! * `a_sheet_behind_the_patch_is_cut_off_by_connectivity`: the far side of a
//!   thin wall (a second admitted sheet the ball overlaps, sharing no edge);
//! * `a_ridge_edge_outside_the_ball_does_not_join_the_two_sides`: connectivity
//!   is the BALL's: a shared edge wholly outside the ball joins nothing, so a
//!   surface cut two ways gives one answer per side (challenger round 2);
//! * `a_ball_that_overlaps_only_another_sheet_has_no_footprint`: the walk is
//!   seeded at the seat triangle by identity and never re-seeds on another
//!   sheet;
//! * `a_slid_tip_is_measured_at_its_moved_ball`: a slide is validated with the
//!   normal of the patch under the MOVED ball, the seat triangle re-derived
//!   there;
//! * `a_flat_face_keeps_its_seat_triangles_bits`: flat discs unchanged;
//! * `the_surface_pairs_equal_the_scan_pairs`: the batched surface call and
//!   the scan fallback give the same rows.
//!
//! Sabotage drills (each run by hand against the production code, recorded in
//! `lanes/footprint-normal.md`): replace the sector term by a chord (the
//! half-disc test fails); drop the admission filter (the wall test fails);
//! drop the connectivity walk (the second-sheet test fails); weight by the
//! triangle's whole area (the tessellation test fails).

use geom_kernel::{closest_on_tri, NativeSurface, NativeSurfaceConfig};
use plinth_core::LayerPlan;
use plinth_mesh::Mesh;
use plinth_support::{SweepArm, SweepArmLanding};
use plinth_sweep::footprint_normal::{
    footprint_normals, footprint_pairs, measure_arms, remeasure_slid, reseat_sites,
    triangle_disc_area, FootprintBall, MeasureEnv, TipSite,
};
use std::f64::consts::PI;

/// The registered band's steep end, 57.5° from horizontal: `n_z < -cos 57.5°`.
fn admitted_z() -> f64 {
    -57.5_f64.to_radians().cos()
}

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

// ---- the kernel ---------------------------------------------------------

#[test]
fn triangle_disc_area_is_exact_for_half_and_quarter_discs() {
    let origin = [0.0, 0.0, 0.0];
    // One edge along the y axis, the apex far to the right: the triangle holds
    // exactly the right half of the unit disc.
    let half = triangle_disc_area([[0.0, -50.0, 0.0], [0.0, 50.0, 0.0], [100.0, 0.0, 0.0]], origin, 1.0);
    assert!(close(half, PI / 2.0, 1e-12), "half disc: {half}");
    // The same triangle wound the other way.
    let half_flipped =
        triangle_disc_area([[0.0, 50.0, 0.0], [0.0, -50.0, 0.0], [100.0, 0.0, 0.0]], origin, 1.0);
    assert!(close(half_flipped, PI / 2.0, 1e-12), "half disc, other winding: {half_flipped}");
    // The first quadrant.
    let quarter = triangle_disc_area([[0.0, 0.0, 0.0], [50.0, 0.0, 0.0], [0.0, 50.0, 0.0]], origin, 1.0);
    assert!(close(quarter, PI / 4.0, 1e-12), "quarter disc: {quarter}");
    // A ball whose centre stands 0.6 over the plane cuts a disc of radius 0.8.
    let high = triangle_disc_area(
        [[0.0, -50.0, 0.0], [0.0, 50.0, 0.0], [100.0, 0.0, 0.0]],
        [0.0, 0.0, 0.6],
        1.0,
    );
    assert!(close(high, PI * 0.64 / 2.0, 1e-12), "half of a 0.8 disc: {high}");
    // The same triangle put in a tilted plane gives the same area.
    let s = 0.3_f64.sin();
    let c = 0.3_f64.cos();
    let tilt = |[x, y, z]: [f64; 3]| [x, c * y - s * z, s * y + c * z];
    let tilted = triangle_disc_area(
        [
            tilt([0.0, -50.0, 0.0]),
            tilt([0.0, 50.0, 0.0]),
            tilt([100.0, 0.0, 0.0]),
        ],
        tilt(origin),
        1.0,
    );
    assert!(close(tilted, PI / 2.0, 1e-12), "half disc in a tilted plane: {tilted}");
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "an area that is exactly zero by construction (no overlap, degenerate triangle) is pinned exactly"
)]
fn triangle_disc_area_is_exact_inside_and_outside() {
    // A disc wholly inside a big triangle: the disc.
    let inside_disc = triangle_disc_area(
        [[-10.0, -10.0, 0.0], [10.0, -10.0, 0.0], [0.0, 10.0, 0.0]],
        [0.0, 0.0, 0.6],
        1.0,
    );
    assert!(close(inside_disc, PI * 0.64, 1e-12), "{inside_disc}");
    // A triangle wholly inside the disc: the triangle.
    let small = triangle_disc_area([[0.0, 0.0, 0.0], [0.1, 0.0, 0.0], [0.0, 0.1, 0.0]], [0.0; 3], 1.0);
    assert!(close(small, 0.005, 1e-15), "{small}");
    // A triangle the ball does not reach, and a degenerate one.
    assert_eq!(
        triangle_disc_area([[5.0, 0.0, 0.0], [6.0, 0.0, 0.0], [5.0, 1.0, 0.0]], [0.0; 3], 1.0),
        0.0
    );
    assert_eq!(
        triangle_disc_area([[0.0, 0.0, 2.0], [1.0, 0.0, 2.0], [0.0, 1.0, 2.0]], [0.0; 3], 1.0),
        0.0,
        "a plane the ball does not reach"
    );
    assert_eq!(
        triangle_disc_area([[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]], [0.0; 3], 1.0),
        0.0
    );
}

#[test]
fn triangle_disc_area_does_not_depend_on_how_a_square_is_split() {
    // A unit square straddling the centre of a ball, off to one side so the
    // disc crosses two edges and a corner; split along each diagonal.
    let centre = [0.35, 0.2, 0.25];
    let radius = 0.6;
    let p = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
    let along_02 = triangle_disc_area([p[0], p[1], p[2]], centre, radius)
        + triangle_disc_area([p[0], p[2], p[3]], centre, radius);
    let along_13 = triangle_disc_area([p[0], p[1], p[3]], centre, radius)
        + triangle_disc_area([p[1], p[2], p[3]], centre, radius);
    assert!(close(along_02, along_13, 1e-12), "{along_02} vs {along_13}");
    assert!(along_02 > 0.1 && along_02 < PI * (radius * radius - 0.0625), "{along_02}");
}

// ---- meshes -------------------------------------------------------------

/// A downward ridge along y at x = 0 over `[-2, 2] × [-2, 2]`: the right plane
/// rises at `slope_right` (z = slope_right · x), the left at `slope_left`
/// (z = -slope_left · x). Outward normals point down and away from the ridge.
/// `cells` is the grid per side along x (even, so the ridge is a grid line);
/// `flip` alternates which diagonal each cell is cut along.
fn ridge(slope_left: f64, slope_right: f64, cells: usize, flip: bool) -> Mesh {
    let mut verts = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let nx = cells * 2;
    let ny = cells;
    let at = |i: usize, j: usize| -> [f64; 3] {
        let x = -2.0 + 4.0 * (i as f64) / (nx as f64);
        let y = -2.0 + 4.0 * (j as f64) / (ny as f64);
        let z = if x >= 0.0 { slope_right * x } else { -slope_left * x };
        [x, y, z]
    };
    for j in 0..=ny {
        for i in 0..=nx {
            verts.push(at(i, j));
        }
    }
    let index = |i: usize, j: usize| u32::try_from(j * (nx + 1) + i).expect("small mesh");
    for j in 0..ny {
        for i in 0..nx {
            let (a, b, c, d) = (index(i, j), index(i + 1, j), index(i + 1, j + 1), index(i, j + 1));
            let diagonal_ac = (i + j) % 2 == 0 && flip || (i + j) % 2 == 1 && !flip;
            let cell: [[u32; 3]; 2] = if diagonal_ac {
                [[a, c, b], [a, d, c]]
            } else {
                [[a, d, b], [b, d, c]]
            };
            for tri in cell {
                // Wind every triangle so its normal points down.
                let [p, q, r] = tri.map(|v| verts[v as usize]);
                let ux = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
                let uy = [r[0] - p[0], r[1] - p[1], r[2] - p[2]];
                let nz = ux[0] * uy[1] - ux[1] * uy[0];
                tris.push(if nz < 0.0 { tri } else { [tri[0], tri[2], tri[1]] });
            }
        }
    }
    Mesh::new(verts, tris).expect("a ridge")
}

fn unit_normal(slope: f64, away: f64) -> [f64; 3] {
    // Surface z = slope·|x| on the right (away = +1) or left (away = -1): outward
    // normal (away·slope, 0, -1) / sqrt(1 + slope²).
    let norm = (1.0 + slope * slope).sqrt();
    [away * slope / norm, 0.0, -1.0 / norm]
}

/// The mesh face nearest `point`: the seat triangle of a tip landing there
/// (no admission filter: the tests that need one say so).
fn face_near(mesh: &Mesh, point: [f64; 3]) -> u32 {
    let verts = mesh.verts();
    let mut best = (f64::INFINITY, 0_u32);
    for (id, tri) in mesh.tris().iter().enumerate() {
        let [a, b, c] = tri.map(|v| verts[v as usize]);
        let distance = geom_kernel::vec3::dist(point, closest_on_tri(point, a, b, c));
        if distance < best.0 {
            best = (distance, u32::try_from(id).expect("small mesh"));
        }
    }
    best.1
}

fn ball(mesh: &Mesh, centre: [f64; 3], radius: f64, seed: [f64; 3], facet: [f64; 3]) -> FootprintBall {
    FootprintBall {
        centre_mm: centre,
        radius_mm: radius,
        seat_face: face_near(mesh, seed),
        facet_normal: facet,
        max_normal_z: admitted_z(),
    }
}

fn normal_of(mesh: &Mesh, b: FootprintBall) -> Option<[f64; 3]> {
    let pairs = footprint_pairs(mesh, None, &[b.centre_mm], &[b.radius_mm]).expect("pairs");
    footprint_normals(mesh, &pairs, &[b])[0].map(|found| found.normal)
}

// ---- the average --------------------------------------------------------

/// Plane areas of `ball ∩ plane ∩ half-space` by brute-force sampling, for the
/// expected weights (independent of the closed form).
fn sampled_areas(
    centre: [f64; 3],
    radius: f64,
    slope_left: f64,
    slope_right: f64,
) -> (f64, f64) {
    let steps = 4000_usize;
    let cell = 4.0 / steps as f64;
    let (mut left, mut right) = (0.0, 0.0);
    for ix in 0..steps {
        for iy in 0..steps {
            let x = -2.0 + (ix as f64 + 0.5) * cell;
            let y = -2.0 + (iy as f64 + 0.5) * cell;
            let z = if x >= 0.0 { slope_right * x } else { -slope_left * x };
            let d2 = (x - centre[0]).powi(2) + (y - centre[1]).powi(2) + (z - centre[2]).powi(2);
            if d2 <= radius * radius {
                // Projected cell area times the surface's area factor.
                let slope = if x >= 0.0 { slope_right } else { slope_left };
                let area = cell * cell * (1.0 + slope * slope).sqrt();
                if x >= 0.0 {
                    right += area;
                } else {
                    left += area;
                }
            }
        }
    }
    (left, right)
}

#[test]
fn the_normal_is_the_area_weighted_admitted_patch() {
    let (slope_left, slope_right) = (30.0_f64.to_radians().tan(), 50.0_f64.to_radians().tan());
    let n_left = unit_normal(slope_left, -1.0);
    let n_right = unit_normal(slope_right, 1.0);
    // A ball over the ridge, off to the right so the two weights differ.
    let centre = [0.15, 0.1, 0.35];
    let radius = 0.7;
    let seed = [0.2, 0.1, 0.2 * slope_right];

    let coarse = ridge(slope_left, slope_right, 2, true);
    let fine = ridge(slope_left, slope_right, 6, false);
    let a = normal_of(&coarse, ball(&coarse, centre, radius, seed, n_right))
        .expect("footprint on the coarse cut");
    let c = normal_of(&fine, ball(&fine, centre, radius, seed, n_right))
        .expect("footprint on the fine cut");

    // The same surface cut two ways: the same normal, to rounding.
    for axis in 0..3 {
        assert!(close(a[axis], c[axis], 1e-12), "tessellation changed the normal: {a:?} vs {c:?}");
    }
    // And it is the area-weighted mean of the two planes' normals, with the
    // weights taken independently by sampling.
    let (area_left, area_right) = sampled_areas(centre, radius, slope_left, slope_right);
    let sum = [
        area_left * n_left[0] + area_right * n_right[0],
        0.0,
        area_left * n_left[2] + area_right * n_right[2],
    ];
    let length = (sum[0] * sum[0] + sum[2] * sum[2]).sqrt();
    let expected = [sum[0] / length, 0.0, sum[2] / length];
    for axis in 0..3 {
        assert!(
            close(a[axis], expected[axis], 2e-3),
            "weights {area_left:.4} / {area_right:.4}: got {a:?}, expected {expected:?}"
        );
    }
    // It is NOT either facet's normal: it leans between them.
    assert!(a[0] > n_left[0] + 1e-3 && a[0] < n_right[0] - 1e-3, "{a:?} between {n_left:?} and {n_right:?}");
    // Moving the seed point across the ridge (the old "nearest facet" flips
    // from the right plane to the left) does not change the footprint.
    let b_left = ball(&coarse, centre, radius, [-0.2, 0.1, 0.2 * slope_left], n_left);
    let flipped = normal_of(&coarse, b_left).expect("footprint with the seed on the left");
    for axis in 0..3 {
        assert!(close(a[axis], flipped[axis], 1e-12), "the seed side changed the patch: {a:?} vs {flipped:?}");
    }
}

#[test]
fn a_wall_in_the_ball_is_not_part_of_the_footprint() {
    // A flat underside z = 0 over [-2, 2]², and a vertical wall rising from its
    // edge at x = 0.3 (outward normal +x: the underside is the floor of a
    // step). The ball overlaps both; only the underside is admitted.
    let verts = vec![
        [-2.0, -2.0, 0.0],
        [0.3, -2.0, 0.0],
        [0.3, 2.0, 0.0],
        [-2.0, 2.0, 0.0],
        [0.3, -2.0, 1.0],
        [0.3, 2.0, 1.0],
    ];
    let tris = vec![[0, 3, 1], [1, 3, 2], [1, 2, 4], [4, 2, 5]];
    let mesh = Mesh::new(verts, tris).expect("a step");
    let b = ball(&mesh, [0.0, 0.0, 0.2], 0.6, [0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
    let found = normal_of(&mesh, b).expect("a footprint");
    assert!(
        found[0].abs() < 1e-15 && (found[2] + 1.0).abs() < 1e-15,
        "the wall's +x normal must not tilt the average: {found:?}"
    );
}

#[test]
fn a_sheet_behind_the_patch_is_cut_off_by_connectivity() {
    // Sheet A: the flat underside z = 0. Sheet B: a second admitted face, tilted
    // 30°, passing 0.3 mm above A's plane inside the ball, sharing no edge with
    // A (a thin wall's far side, or a second sheet). The ball overlaps both.
    let tilt = 30.0_f64.to_radians();
    let (s, c) = tilt.sin_cos();
    let a = [
        [-2.0, -2.0, 0.0],
        [2.0, -2.0, 0.0],
        [2.0, 2.0, 0.0],
        [-2.0, 2.0, 0.0],
    ];
    // B: z = 0.3 + tan(tilt)·x over x in [-0.5, 0.5], y in [-0.5, 0.5].
    let t = s / c;
    let b = [
        [-0.5, -0.5, 0.3 - 0.5 * t],
        [0.5, -0.5, 0.3 + 0.5 * t],
        [0.5, 0.5, 0.3 + 0.5 * t],
        [-0.5, 0.5, 0.3 - 0.5 * t],
    ];
    let mut verts = a.to_vec();
    verts.extend(b);
    // A faces down; B faces down and toward +x? Its outward normal is
    // (t, 0, -1)/norm — wind it so z < 0.
    let tris = vec![[0, 3, 1], [1, 3, 2], [4, 7, 5], [5, 7, 6]];
    let mesh = Mesh::new(verts, tris).expect("two sheets");
    // Seeded on sheet A.
    let on_a = normal_of(&mesh, ball(&mesh, [0.0, 0.0, 0.25], 0.6, [0.0, 0.0, 0.0], [0.0, 0.0, -1.0]))
        .expect("a footprint on A");
    assert!(
        on_a[0].abs() < 1e-15 && (on_a[2] + 1.0).abs() < 1e-15,
        "sheet B shares no edge with A and must not tilt A's patch: {on_a:?}"
    );
    // Seeded on sheet B: B alone, its own normal.
    let on_b = normal_of(
        &mesh,
        ball(&mesh, [0.0, 0.0, 0.25], 0.6, [0.0, 0.0, 0.3], unit_normal(t, 1.0)),
    )
    .expect("a footprint on B");
    let expected = unit_normal(t, 1.0);
    for axis in 0..3 {
        assert!(close(on_b[axis], expected[axis], 1e-12), "{on_b:?} vs {expected:?}");
    }
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "the pin IS bit identity: a flat face must keep its seat triangle's normal unperturbed"
)]
fn a_flat_face_keeps_its_seat_triangles_bits() {
    // A flat underside cut into triangles of very different sizes, every normal
    // the same plane's: whatever the patch, the answer is the seat triangle's
    // normal, bit for bit (a Round disc must not be perturbed by a rounded sum).
    let mut verts = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let n = 7_usize;
    for j in 0..=n {
        for i in 0..=n {
            let x = -2.0 + 4.0 * (i * i) as f64 / (n * n) as f64 * if i % 2 == 0 { 1.0 } else { 0.97 };
            let y = -2.0 + 4.0 * j as f64 / n as f64;
            verts.push([x, y, 1.0]);
        }
    }
    let index = |i: usize, j: usize| u32::try_from(j * (n + 1) + i).expect("small");
    for j in 0..n {
        for i in 0..n {
            let (a, b, c, d) = (index(i, j), index(i + 1, j), index(i + 1, j + 1), index(i, j + 1));
            tris.push([a, d, b]);
            tris.push([b, d, c]);
        }
    }
    let mesh = Mesh::new(verts, tris).expect("a flat patch");
    let facet = [0.0, 0.0, -1.0];
    let found = normal_of(&mesh, ball(&mesh, [0.2, 0.3, 1.2], 0.9, [0.2, 0.3, 1.0], facet))
        .expect("a footprint");
    assert_eq!(found, facet, "flat face: the seat triangle's bits");
    let pairs = footprint_pairs(&mesh, None, &[[0.2, 0.3, 1.2]], &[0.9]).expect("pairs");
    let found = footprint_normals(
        &mesh,
        &pairs,
        &[ball(&mesh, [0.2, 0.3, 1.2], 0.9, [0.2, 0.3, 1.0], facet)],
    )[0]
        .expect("a footprint");
    let disc = PI * (0.9_f64 * 0.9 - 0.2 * 0.2);
    assert!(close(found.area_mm2, disc, 1e-12), "area {} vs {disc}", found.area_mm2);
    assert!(found.faces > 4, "{} faces", found.faces);
}

#[test]
fn a_footprint_with_no_admitted_area_has_no_answer() {
    // Only a wall under the ball: nothing admitted, so the caller keeps the
    // seat triangle's normal.
    let verts = vec![[0.3, -2.0, 0.0], [0.3, 2.0, 0.0], [0.3, -2.0, 1.0], [0.3, 2.0, 1.0]];
    let tris = vec![[0, 1, 2], [2, 1, 3]];
    let mesh = Mesh::new(verts, tris).expect("a wall");
    assert!(
        normal_of(&mesh, ball(&mesh, [0.0, 0.0, 0.2], 0.6, [0.3, 0.0, 0.2], [0.0, 0.0, -1.0])).is_none()
    );
}

// ---- connectivity is the ball's, not the mesh's --------------------------

/// Two downward triangles sharing a ridge edge at `z = 1.2`, wholly outside a
/// unit ball at the origin, with their apexes (-0.8, 0, 0) and (0.8, 0, 0)
/// inside it: both normals are admitted (56.31° from horizontal), both
/// triangles meet the ball, and the shared edge does not. Wound so every
/// normal points down.
fn ridge_outside_the_ball(cut: bool) -> Mesh {
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let push = |verts: &mut Vec<[f64; 3]>, p: [f64; 3]| -> u32 {
        verts.push(p);
        u32::try_from(verts.len() - 1).expect("small mesh")
    };
    let add = |verts: &mut Vec<[f64; 3]>, tris: &mut Vec<[u32; 3]>, tri: [u32; 3]| {
        let [p, q, r] = tri.map(|v| verts[v as usize]);
        let u = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
        let v = [r[0] - p[0], r[1] - p[1], r[2] - p[2]];
        let nz = u[0] * v[1] - u[1] * v[0];
        tris.push(if nz < 0.0 { tri } else { [tri[0], tri[2], tri[1]] });
    };
    let ridge_a = push(&mut verts, [0.0, -2.0, 1.2]);
    let ridge_b = push(&mut verts, [0.0, 2.0, 1.2]);
    if !cut {
        for side in [-1.0_f64, 1.0] {
            let apex = push(&mut verts, [0.8 * side, 0.0, 0.0]);
            add(&mut verts, &mut tris, [ridge_a, ridge_b, apex]);
        }
        return Mesh::new(verts, tris).expect("two triangles");
    }
    // The same two triangles, each cut along the line x = ±0.1: the apex
    // triangle, and the strip beside the ridge in two triangles. The strip
    // stands at z >= 1.05, outside the ball.
    for side in [-1.0_f64, 1.0] {
        let apex = push(&mut verts, [0.8 * side, 0.0, 0.0]);
        let near_a = push(&mut verts, [0.1 * side, -1.75, 1.05]);
        let near_b = push(&mut verts, [0.1 * side, 1.75, 1.05]);
        add(&mut verts, &mut tris, [apex, near_a, near_b]);
        add(&mut verts, &mut tris, [near_a, ridge_a, ridge_b]);
        add(&mut verts, &mut tris, [near_a, ridge_b, near_b]);
    }
    Mesh::new(verts, tris).expect("two cut triangles")
}

#[test]
fn a_ridge_edge_outside_the_ball_does_not_join_the_two_sides() {
    // The challenger's static counterexample: whole or cut, the patch under
    // the ball that holds the LEFT seat triangle is the left triangle's alone,
    // so the normal is the left facet's (56.31° off horizontal), never the
    // straight-down average of both sides.
    // The left triangle rises 1.2 over 0.8 toward the ridge: z = 1.5 (x + 0.8),
    // so its outward (downward) normal is (1.5, 0, -1) / sqrt(3.25).
    let left_normal = {
        let length = 3.25_f64.sqrt();
        [1.5 / length, 0.0, -1.0 / length]
    };
    assert!(left_normal[2] < admitted_z(), "the premise: admitted, {left_normal:?}");
    let mut answers = Vec::new();
    for cut in [false, true] {
        let mesh = ridge_outside_the_ball(cut);
        for seat in [[-0.4, 0.0, 0.6], [0.4, 0.0, 0.6]] {
            let b = ball(&mesh, [0.0; 3], 1.0, seat, left_normal);
            // Both triangles meet the ball: the premise of the failure.
            let pairs = footprint_pairs(&mesh, None, &[b.centre_mm], &[b.radius_mm]).expect("pairs");
            assert!(pairs.row(0).len() >= 2, "cut={cut}: the ball must meet both sides");
            let found = normal_of(&mesh, b).expect("a footprint");
            answers.push((cut, seat[0], found));
        }
    }
    for &(cut, seat_x, found) in &answers {
        assert!(
            found[0].abs() > 0.8 && found[2] < -0.5 && found[2] > -0.6,
            "cut={cut} seat x {seat_x}: not one side's facet normal (a straight-down average): {found:?}"
        );
        // The side the seat triangle is on: the left triangle's normal leans
        // +x (it rises toward the ridge), the right one's -x.
        assert_eq!(found[0].is_sign_negative(), seat_x > 0.0, "cut={cut} seat x {seat_x}: {found:?}");
        assert!(close(found[2], -1.0 / 3.25_f64.sqrt(), 1e-12), "cut={cut} seat x {seat_x}: {found:?}");
    }
    // Whole and cut give the same normal for the same side, to rounding
    // (answers: whole-left, whole-right, cut-left, cut-right).
    for (whole, cut) in [(0, 2), (1, 3)] {
        for axis in 0..3 {
            assert!(
                close(answers[whole].2[axis], answers[cut].2[axis], 1e-12),
                "tessellation changed the normal: {:?} vs {:?}",
                answers[whole],
                answers[cut]
            );
        }
    }
}

#[test]
fn a_ball_that_overlaps_only_another_sheet_has_no_footprint() {
    // Sheet A is the flat underside z = 0, sheet B a tilted sheet 0.3 mm above
    // it, joined to A by no edge. A ball whose centre stands 0.8 over A's plane
    // with radius 0.6 reaches B only. The seat triangle is on A: the patch
    // that contains it does not reach the ball, so there is NO footprint (the
    // tip keeps A's normal), and the walk never re-seeds on B.
    let t = 30.0_f64.to_radians().tan();
    let mut verts = vec![
        [-2.0, -2.0, 0.0],
        [2.0, -2.0, 0.0],
        [2.0, 2.0, 0.0],
        [-2.0, 2.0, 0.0],
    ];
    verts.extend([
        [-0.5, -0.5, 0.3 - 0.5 * t],
        [0.5, -0.5, 0.3 + 0.5 * t],
        [0.5, 0.5, 0.3 + 0.5 * t],
        [-0.5, 0.5, 0.3 - 0.5 * t],
    ]);
    let tris = vec![[0, 3, 1], [1, 3, 2], [4, 7, 5], [5, 7, 6]];
    let mesh = Mesh::new(verts, tris).expect("two sheets");
    let centre = [0.0, 0.0, 0.8];
    let pairs = footprint_pairs(&mesh, None, &[centre], &[0.6]).expect("pairs");
    let row = pairs.row(0);
    assert!(!row.is_empty() && row.iter().all(|&face| face >= 2), "the ball reaches B only: {row:?}");
    let on_a = ball(&mesh, centre, 0.6, [0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
    assert!(normal_of(&mesh, on_a).is_none(), "A's patch is absent from the ball");
    // The same ball with the seat triangle on B is B's own footprint.
    let on_b = normal_of(
        &mesh,
        ball(&mesh, centre, 0.6, [0.0, 0.0, 0.3], unit_normal(t, 1.0)),
    )
    .expect("a footprint on B");
    let expected = unit_normal(t, 1.0);
    for axis in 0..3 {
        assert!(close(on_b[axis], expected[axis], 1e-12), "{on_b:?} vs {expected:?}");
    }
}

// ---- the normal follows the ball ------------------------------------------

#[test]
#[expect(
    clippy::float_cmp,
    reason = "a tip with no footprint keeps the normal it was given, bit for bit, and that is what is pinned"
)]
fn a_slid_tip_is_measured_at_its_moved_ball() {
    // A vertical arm under the right plane of a 30°/50° ridge, then SLID along
    // the surface far across the ridge onto the left plane. The normal the
    // slide is validated with must be the normal of the patch under the MOVED
    // ball (the left plane), found from the seat triangle re-derived at the
    // slid seat. Keeping the pre-slide measurement, or seeding the walk at the
    // old seat triangle (now far outside the ball), gives the right plane's.
    let (slope_left, slope_right) = (30.0_f64.to_radians().tan(), 50.0_f64.to_radians().tan());
    let n_left = unit_normal(slope_left, -1.0);
    let n_right = unit_normal(slope_right, 1.0);
    let mesh = ridge(slope_left, slope_right, 10, true);
    let layer_plan = LayerPlan::new(-12.0, vec![0.05; 400]).expect("a uniform layer plan");
    let env = MeasureEnv {
        mesh: &mesh,
        surface: None,
        layer_plan: &layer_plan,
    };
    let seat_at = |x: f64| [x, 0.0, if x >= 0.0 { slope_right * x } else { -slope_left * x }];
    let arm_at = |seat: [f64; 3], normal: [f64; 3]| SweepArm {
        seat_mm: seat,
        base_mm: [seat[0], seat[1], seat[2] - 10.0],
        tip_radius_mm: 0.3,
        radius_profile_mm: Vec::new(),
        seat_normal_mm: normal,
        seat_face_point_mm: Some(seat),
        landing: SweepArmLanding::Landed,
        seats_into_model: true,
        head: None,
        contact_ball_radius_mm: None,
    };
    let before = seat_at(0.3);
    let site = TipSite {
        face: face_near(&mesh, before),
        landing_mm: before,
        facet_normal: n_right,
        max_normal_z: admitted_z(),
    };
    // Before the slide the ball is over the ridge's right flank and the old
    // triangle is in it.
    let at_rest = measure_arms(&env, &[arm_at(before, n_right)], &[Some(site)]).expect("measured");
    let at_rest = at_rest.tips[0].expect("a tip");
    assert!(!at_rest.no_ball && !at_rest.no_footprint, "{at_rest:?}");

    let after = seat_at(-1.2);
    let shift = [after[0] - before[0], 0.0, after[2] - before[2]];
    let moved = arm_at(after, n_right);
    // The reseated site is a triangle of the left plane.
    let reseated = reseat_sites(&mesh, &[Some(site)], &[Some(shift)]);
    let reseated = reseated[0].expect("a site");
    assert_ne!(reseated.face, site.face);
    for (&got, &want) in reseated.facet_normal.iter().zip(&n_left) {
        assert!(close(got, want, 1e-12), "{:?}", reseated.facet_normal);
    }
    let slid = remeasure_slid(&env, &[Some(site)], core::slice::from_ref(&moved), &[shift]);
    for axis in 0..3 {
        assert!(
            close(slid[0][axis], n_left[axis], 1e-9),
            "the slid ball sits on the left plane: {:?} vs {n_left:?}",
            slid[0]
        );
    }
    // The stale measurement: the old seat triangle against the moved ball is
    // nowhere near it, so the ball has no footprint through it, and the tip
    // would keep the old (right) plane's normal.
    let stale = measure_arms(&env, &[moved], &[Some(site)]).expect("measured");
    let stale = stale.tips[0].expect("a tip");
    assert!(stale.no_footprint, "{stale:?}");
    assert_eq!(stale.normal, n_right);
    assert!(
        geom_kernel::vec3::dot(slid[0], stale.normal) < (1.0_f64 - 1e-3),
        "the two answers differ by more than rounding"
    );
}

// ---- the pair producer --------------------------------------------------

#[test]
fn the_surface_pairs_equal_the_scan_pairs() {
    let mesh = ridge(0.4, 1.1, 8, true);
    let surface = NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: 0.25,
            max_grid_bytes: 64 << 20,
        },
    )
    .expect("a surface");
    let centres = [[0.15, 0.1, 0.35], [-1.0, 0.7, 0.9], [1.3, -0.4, 1.6], [0.0, 0.0, 9.0]];
    let radii = [0.7, 0.5, 0.45, 0.3];
    let by_surface = footprint_pairs(&mesh, Some(&surface), &centres, &radii).expect("surface pairs");
    let by_scan = footprint_pairs(&mesh, None, &centres, &radii).expect("scan pairs");
    assert_eq!(by_surface, by_scan);
    assert!(by_scan.row(0).len() > 4, "the ridge ball reaches several faces");
    assert!(by_scan.row(3).is_empty(), "a ball in the air reaches none");
    // The normals agree too.
    let balls: Vec<FootprintBall> = centres
        .iter()
        .zip(&radii)
        .map(|(&c, &r)| ball(&mesh, c, r, [c[0], c[1], 0.0], [0.0, 0.0, -1.0]))
        .collect();
    assert_eq!(
        footprint_normals(&mesh, &by_surface, &balls),
        footprint_normals(&mesh, &by_scan, &balls)
    );
}
