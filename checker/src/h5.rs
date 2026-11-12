//! H5 topology prerequisites and opt-in fixed-resolution hollow search. Undeclared scenes
//! retain the old topology-only exemption below (a known blind spot). Declared scenes never
//! receive that exemption: even an exactly convex body contradicts a hollow declaration.
//!
//! * vertices are welded only where they are equal in f32; every edge is used twice, once in
//!   each direction (a closed, consistently oriented surface);
//! * the surface has no self-intersection;
//! * exactly one shell, enclosing a positive volume, with Euler characteristic 2, which means
//!   no inner shell (no enclosed void) and no through-hole (so no drain bore can exist).
//!
//! For undeclared scenes only, H5 is then "not applicable". Anything uncertain is "unverified", never
//! "not applicable". An inner shell facing inwards inside the outer one is a sealed cavity
//! with no opening in the surface: that is a violation (it can drain nowhere).
//!
//! The open underside recess of a base is not a cavity: the surface is still one closed
//! shell. The cup test of FR-17a is separate (it needs the print order).

use crate::json::Json;
use crate::mesh::{self, Welded};
use crate::outcome::{Outcome, at};
use crate::scene::Scene;
use crate::tri::{Bvh, Tri, tri_box};
use crate::v3::{Aabb, V, add, cross, dot, len, mul, sub};

/// Counts that came out of the mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct Topology {
    pub triangles: usize,
    pub closed_and_oriented: bool,
    pub bad_edges: usize,
    pub shells: u32,
    pub volumes: Vec<f64>,
    pub euler: Vec<i64>,
    /// Pairs of triangles that intersect (not counting shared vertices and edges).
    pub intersections: usize,
    /// Pairs the exact-enough test could not decide.
    pub ambiguous: usize,
    /// Triangle where the first intersection or ambiguity was seen.
    pub first_problem: Option<usize>,
    /// A vertex of the first inward-facing shell (a void), if there is one.
    pub void_at: Option<V>,
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the triangles were read from f32"
)]
fn coords32(model: &[Tri]) -> Vec<f32> {
    model
        .iter()
        .flatten()
        .flat_map(|p| p.map(|x| x as f32))
        .collect()
}

/// Sign of the volume `orient(a, b, c, d)`; 0 when it is too close to call.
fn orient(a: V, b: V, c: V, d: V, scale: f64) -> i32 {
    let v = dot(sub(a, d), cross(sub(b, d), sub(c, d)));
    let eps = 1e-12 * scale * scale * scale;
    if v > eps {
        1
    } else if v < -eps {
        -1
    } else {
        0
    }
}

/// Where a point of the triangle's plane lies against the triangle: 1 inside, -1 outside,
/// 0 on the boundary (within 1e-12 of the triangle's area).
fn locate(pt: V, t: &Tri) -> i32 {
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let nn = dot(n, n).max(1e-300);
    let w = [
        dot(cross(sub(t[1], pt), sub(t[2], pt)), n) / nn,
        dot(cross(sub(t[2], pt), sub(t[0], pt)), n) / nn,
        dot(cross(sub(t[0], pt), sub(t[1], pt)), n) / nn,
    ];
    if w.iter().any(|x| *x < -1e-12) {
        -1
    } else if w.iter().all(|x| *x > 1e-12) {
        1
    } else {
        0
    }
}

/// A segment lying in the plane of `t`: does it cross or enter the triangle? Touching only
/// at the boundary counts as not crossing; an endpoint on the boundary is undecided.
fn coplanar_segment(p: V, q: V, t: &Tri) -> Result<bool, ()> {
    let (lp, lq) = (locate(p, t), locate(q, t));
    if lp == 1 || lq == 1 {
        return Ok(true);
    }
    if lp == 0 || lq == 0 {
        return Err(());
    }
    // Both outside: it crosses if it separates the triangle's corners strictly.
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let side = |c: V| dot(cross(sub(q, p), sub(c, p)), n);
    let vals = [side(t[0]), side(t[1]), side(t[2])];
    let signs = vals.map(|s| i32::from(s > 1e-12) - i32::from(s < -1e-12));
    if signs.contains(&0) || (signs[0] == signs[1] && signs[1] == signs[2]) {
        return Ok(false);
    }
    // The line cuts the triangle along a chord; the segment crosses if the chord overlaps
    // the segment's extent along the line.
    let d = sub(q, p);
    let span = dot(d, d);
    let mut chord = [f64::INFINITY, f64::NEG_INFINITY];
    for k in 0..3 {
        let j = (k + 1) % 3;
        if signs[k] != signs[j] {
            let s = vals[k] / (vals[k] - vals[j]);
            let pt = add(t[k], mul(sub(t[j], t[k]), s));
            let along = dot(sub(pt, p), d);
            chord = [chord[0].min(along), chord[1].max(along)];
        }
    }
    Ok(chord[0] < span && chord[1] > 0.0)
}

/// Does the open segment `p-q` pass through triangle `t`? `Err(())` if too close to call.
fn segment_through(p: V, q: V, t: &Tri, scale: f64) -> Result<bool, ()> {
    let (s1, s2) = (
        orient(t[0], t[1], t[2], p, scale),
        orient(t[0], t[1], t[2], q, scale),
    );
    if s1 == 0 && s2 == 0 {
        return coplanar_segment(p, q, t);
    }
    if s1 == 0 || s2 == 0 {
        // One end on the plane: it meets the plane only there.
        let on = if s1 == 0 { p } else { q };
        return match locate(on, t) {
            -1 => Ok(false),
            _ => Err(()),
        };
    }
    if s1 == s2 {
        return Ok(false);
    }
    // The segment crosses the plane strictly: find where, and see if that point is in the
    // triangle.
    let (d1, d2) = (
        dot(cross(sub(t[1], t[0]), sub(t[2], t[0])), sub(p, t[0])),
        dot(cross(sub(t[1], t[0]), sub(t[2], t[0])), sub(q, t[0])),
    );
    let x = add(p, mul(sub(q, p), d1 / (d1 - d2)));
    match locate(x, t) {
        1 => Ok(true),
        -1 => Ok(false),
        _ => Err(()),
    }
}

/// Intersection test for two triangles with `shared` vertices in common.
fn pair_state(a: &Tri, b: &Tri, ia: [u32; 3], ib: [u32; 3]) -> Result<bool, ()> {
    let shared_a: Vec<usize> = (0..3).filter(|&k| ib.contains(&ia[k])).collect();
    let scale = [a, b]
        .iter()
        .flat_map(|t| t.iter())
        .map(|v| len(*v))
        .fold(1.0, f64::max)
        .max(1.0);
    let edges = |t: &Tri, skip: &[usize]| -> Vec<(V, V)> {
        // Edge k joins vertex k and k+1; skip any edge that touches a shared vertex.
        (0..3)
            .filter(|&k| !skip.contains(&k) && !skip.contains(&((k + 1) % 3)))
            .map(|k| (t[k], t[(k + 1) % 3]))
            .collect()
    };
    let shared_b: Vec<usize> = (0..3).filter(|&k| ia.contains(&ib[k])).collect();
    if shared_a.len() >= 2 {
        // Edge-adjacent: only a fold-back (coplanar and overlapping) intersects.
        let na = cross(sub(a[1], a[0]), sub(a[2], a[0]));
        let nb = cross(sub(b[1], b[0]), sub(b[2], b[0]));
        let c = len(cross(na, nb)) / (len(na) * len(nb)).max(1e-300);
        return if c < 1e-9 && dot(na, nb) < 0.0 {
            Err(())
        } else {
            Ok(false)
        };
    }
    let mut hit = false;
    for (p, q) in edges(a, &shared_a) {
        hit |= segment_through(p, q, b, scale)?;
    }
    for (p, q) in edges(b, &shared_b) {
        hit |= segment_through(p, q, a, scale)?;
    }
    Ok(hit)
}

fn self_intersections(w: &Welded, tris: &[Tri]) -> (usize, usize, Option<usize>) {
    let boxes: Vec<Aabb> = tris.iter().map(|t| tri_box(t).inflated(1e-9)).collect();
    let bvh = Bvh::build(&boxes);
    let (mut hits, mut unclear, mut first) = (0usize, 0usize, None);
    let mut cand = Vec::new();
    for i in 0..tris.len() {
        cand.clear();
        bvh.query(&boxes[i], &mut cand);
        for &j in cand.iter().filter(|&&j| j as usize > i) {
            let j = j as usize;
            match pair_state(&tris[i], &tris[j], w.tris[i], w.tris[j]) {
                Ok(true) => {
                    hits += 1;
                    first.get_or_insert(i);
                }
                Ok(false) => {}
                Err(()) => {
                    unclear += 1;
                    first.get_or_insert(i);
                }
            }
        }
    }
    (hits, unclear, first)
}

/// Analyse the model's topology.
#[must_use]
pub fn analyze(scene: &Scene) -> Option<Topology> {
    let w = mesh::weld(&coords32(&scene.model))?;
    let edges = mesh::edge_report(&w);
    let (shell_of, n) = mesh::shells(&w);
    let volumes = mesh::shell_volumes(&w, &shell_of, n);
    let euler: Vec<i64> = (0..n)
        .map(|s| mesh::shell_euler(&w, &shell_of, s))
        .collect();
    let (intersections, ambiguous, first_problem) = if edges.closed_and_oriented() {
        self_intersections(&w, &scene.model)
    } else {
        (0, 0, None)
    };
    // A vertex of the first inward-facing shell, to say where a void is.
    let void_at = volumes
        .iter()
        .position(|v| *v < 0.0)
        .and_then(|s| shell_of.iter().position(|x| usize::try_from(*x) == Ok(s)))
        .map(|i| crate::v3::f(w.verts[i]));
    Some(Topology {
        void_at,
        triangles: w.tris.len(),
        closed_and_oriented: edges.closed_and_oriented(),
        bad_edges: edges.bad,
        shells: n,
        volumes,
        euler,
        intersections,
        ambiguous,
        first_problem,
    })
}

fn detail(t: &Topology) -> Json {
    Json::obj([
        ("triangles", Json::Uint(t.triangles as u64)),
        ("closed_and_oriented", Json::Bool(t.closed_and_oriented)),
        ("bad_edges", Json::Uint(t.bad_edges as u64)),
        ("shells", Json::Uint(u64::from(t.shells))),
        (
            "shell_volumes_mm3",
            Json::Arr(t.volumes.iter().map(|v| Json::Num(*v)).collect()),
        ),
        (
            "shell_euler_characteristics",
            Json::Arr(t.euler.iter().map(|e| Json::Num(*e as f64)).collect()),
        ),
        ("self_intersections", Json::Uint(t.intersections as u64)),
        ("undecided_triangle_pairs", Json::Uint(t.ambiguous as u64)),
    ])
}

/// H5.
#[must_use]
pub fn measure(scene: &Scene) -> Outcome {
    let Some(t) = analyze(scene) else {
        return Outcome::unverified("the model mesh is empty or has a non-finite coordinate");
    };
    let d = detail(&t);
    if !t.closed_and_oriented {
        return Outcome::unverified(format!(
            "the model surface is not a closed, consistently oriented mesh ({} bad edges): cavities cannot be counted",
            t.bad_edges
        ))
        .with_detail(d);
    }
    if t.intersections > 0 || t.ambiguous > 0 {
        // Topology ambiguity must not hide a witnessed intrusion on a declared hollow.
        let combined = if scene.declared_hollow {
            let hollow = scene.hollow();
            if t.intersections == 0 && !hollow.intrusion.is_empty() {
                return hollow
                    .outcome()
                    .with_detail(Json::obj([("topology", d), ("hollow", hollow.detail())]));
            }
            Json::obj([("topology", d), ("hollow", hollow.detail())])
        } else {
            d
        };
        let first = t
            .first_problem
            .and_then(|i| scene.model.get(i))
            .map(|tri| scene.world(tri[0]));
        return Outcome::unverified(format!(
            "the model surface may intersect itself ({} crossing pair(s), {} undecided pair(s)): the shell count is not trusted",
            t.intersections, t.ambiguous
        ))
        .with_located(vec![at(None, first, None, "first suspect triangle")])
        .with_detail(combined);
    }
    if !scene.declared_hollow && t.shells == 1 && t.volumes[0] > 0.0 && t.euler[0] == 2 {
        return Outcome::not_applicable(
            "H5: not applicable — emitted model has 0 enclosed cavities and 0 drain bores; topology verified.",
        )
        .with_detail(d);
    }
    let inner = t.volumes.iter().filter(|v| **v < 0.0).count();
    if inner > 0 && t.volumes.iter().filter(|v| **v > 0.0).count() == 1 {
        return Outcome::violation(format!(
            "the model has {inner} enclosed void(s): a shell facing inwards inside the outer one. A closed void has no drain or vent."
        ))
        .with_located(vec![at(
            None,
            t.void_at.map(|p| scene.world(p)),
            None,
            "enclosed cavity with no opening",
        )])
        .with_detail(d);
    }
    if !scene.declared_hollow {
        return Outcome::unverified(format!(
            "the model has {} shell(s) and Euler characteristics {:?}: not the single simple shell the proof needs, and no drain or vent measurement is built",
            t.shells, t.euler
        )).with_detail(d);
    }
    let mut out = scene.hollow().outcome();
    out.detail = Json::obj([("topology", d), ("hollow", out.detail)]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Status;
    use crate::testkit::test_limits;

    fn cube(c: V, h: f64, inward: bool) -> Vec<Tri> {
        let p = |i: u8| {
            [
                c[0] + if i & 1 == 0 { -h } else { h },
                c[1] + if i & 2 == 0 { -h } else { h },
                c[2] + if i & 4 == 0 { -h } else { h },
            ]
        };
        let quads = [
            [0u8, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        let mut out = Vec::new();
        for q in quads {
            let (a, b, c2, d) = (p(q[0]), p(q[1]), p(q[2]), p(q[3]));
            if inward {
                out.push([a, c2, b]);
                out.push([a, d, c2]);
            } else {
                out.push([a, b, c2]);
                out.push([a, c2, d]);
            }
        }
        out
    }

    fn run(model: Vec<Tri>) -> Outcome {
        measure(&Scene::from_parts(
            &test_limits(),
            model,
            [0.0; 3],
            Vec::new(),
        ))
    }

    #[test]
    fn a_solid_cube_proves_not_applicable_with_the_exact_wording() {
        let o = run(cube([0.0; 3], 5.0, false));
        assert_eq!(o.status, Status::NotApplicable, "{o:?}");
        assert_eq!(
            o.reason,
            "H5: not applicable — emitted model has 0 enclosed cavities and 0 drain bores; topology verified."
        );
    }

    #[test]
    fn an_inner_void_is_a_violation_and_a_bad_shell_is_unverified() {
        let mut m = cube([0.0; 3], 5.0, false);
        m.extend(cube([0.0; 3], 2.0, true));
        assert_eq!(run(m).status, Status::Violation);
        let mut open = cube([0.0; 3], 5.0, false);
        open.pop();
        assert_eq!(run(open).status, Status::Unverified);
        // A flipped face.
        let mut flipped = cube([0.0; 3], 5.0, false);
        flipped[3].swap(1, 2);
        assert_eq!(run(flipped).status, Status::Unverified);
    }

    #[test]
    fn two_cubes_that_cross_are_unverified_and_two_apart_are_not_one_shell() {
        let mut crossing = cube([0.0; 3], 5.0, false);
        crossing.extend(cube([4.0, 1.0, 0.5], 5.0, false));
        let o = run(crossing);
        assert_eq!(o.status, Status::Unverified, "{o:?}");
        assert!(o.reason.contains("intersect itself"), "{}", o.reason);
        let mut apart = cube([0.0; 3], 5.0, false);
        apart.extend(cube([20.0, 0.0, 0.0], 5.0, false));
        let o2 = run(apart);
        assert_eq!(o2.status, Status::Unverified, "{o2:?}");
    }

    #[test]
    fn a_tunnel_is_a_hole_the_proof_does_not_call_applicable() {
        // A square tube (a ring in section): genus 1.
        let (a, b, h) = (2.0, 1.0, 5.0);
        let corner = |half: f64, i: usize, z: f64| {
            let (sx, sy) = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)][i % 4];
            [sx * half, sy * half, z]
        };
        let (lo, hi) = (-0.5 * h, 0.5 * h);
        let mut m = Vec::new();
        for i in 0..4 {
            for (half, flip) in [(a, false), (b, true)] {
                let (b0, b1) = (corner(half, i, lo), corner(half, i + 1, lo));
                let (t0, t1) = (corner(half, i, hi), corner(half, i + 1, hi));
                if flip {
                    m.push([b0, t1, b1]);
                    m.push([b0, t0, t1]);
                } else {
                    m.push([b0, b1, t1]);
                    m.push([b0, t1, t0]);
                }
            }
            for (z, up) in [(hi, true), (lo, false)] {
                let (o0, o1) = (corner(a, i, z), corner(a, i + 1, z));
                let (i0, i1) = (corner(b, i, z), corner(b, i + 1, z));
                if up {
                    m.push([o0, o1, i1]);
                    m.push([o0, i1, i0]);
                } else {
                    m.push([o0, i1, o1]);
                    m.push([o0, i0, i1]);
                }
            }
        }
        let o = measure(&Scene::from_parts(
            &test_limits(),
            m,
            [0.0, 0.0, 5.0],
            Vec::new(),
        ));
        assert_eq!(o.status, Status::Unverified, "{o:?}");
        assert!(o.reason.contains("Euler"), "{}", o.reason);
    }
}
