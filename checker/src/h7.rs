//! H7: model fidelity. The model in the file must be the input STL under the recorded pose
//! and nothing else: by default the pose is the identity rotation plus centring on the STL's
//! bounding box (design 7.2), with the centre going into translate X and Y.
//!
//! The comparison is triangle by triangle: the file's triangle, shifted by the STL's
//! bounding-box centre, must equal one STL triangle up to a cyclic rotation of its vertices
//! (the order of triangles is free; reversed winding is not a rotation). Each STL triangle
//! is used once, so a missing duplicate fails. The vertex tolerance is 1e-5 mm. The triangle
//! count, the enclosed volume and the surface area must agree as well, and translate X and
//! Y must put the model where the STL had it. Allowed changes: none.
//!
//! The pose may include a rotation about X (`--pose-rotate-x-deg`, applied to the STL before
//! centring): the approved Round bases are the originals turned +115 degrees, so the
//! originals with that rotation reproduce them to under 1e-6 mm. If the caller records the
//! STL's SHA-256 (`--stl-sha256`), a different STL is a violation. The checker does not
//! learn the pose or the hash from the file; both are claims it is handed.

use crate::json::Json;
use crate::outcome::{Outcome, at};
use crate::scene::Scene;
use crate::stl::Stl;
use crate::tri::Tri;
use crate::v3::{V, cross, dist, dot, f, len, sub};
use std::collections::BTreeMap;

/// Largest vertex difference that still counts as the same vertex, mm.
pub const VERTEX_TOL_MM: f64 = 1e-5;
/// Largest difference between translate X, Y and the STL's centre, mm.
pub const TRANSLATE_TOL_MM: f64 = 0.01;

fn stl_tris(s: &Stl) -> Vec<Tri> {
    s.coords
        .chunks_exact(9)
        .map(|t| {
            [
                f([t[0], t[1], t[2]]),
                f([t[3], t[4], t[5]]),
                f([t[6], t[7], t[8]]),
            ]
        })
        .collect()
}

fn centre_of(tris: &[Tri]) -> V {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in tris.iter().flatten() {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    [
        0.5 * (lo[0] + hi[0]),
        0.5 * (lo[1] + hi[1]),
        0.5 * (lo[2] + hi[2]),
    ]
}

fn volume_area(tris: &[Tri]) -> (f64, f64) {
    tris.iter().fold((0.0, 0.0), |(v, a), t| {
        let c = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        (v + dot(t[0], cross(t[1], t[2])) / 6.0, a + 0.5 * len(c))
    })
}

/// Largest vertex difference of `a` against `b` over the three cyclic rotations of `b`
/// (smallest of the three).
fn rotated_dev(a: &Tri, b: &Tri) -> f64 {
    (0..3)
        .map(|r| {
            (0..3)
                .map(|k| dist(a[k], b[(k + r) % 3]))
                .fold(0.0, f64::max)
        })
        .fold(f64::INFINITY, f64::min)
}

fn centroid_cell(t: &Tri) -> [i64; 3] {
    let c = crate::v3::centroid(t);
    #[allow(clippy::cast_possible_truncation, reason = "a grid cell index")]
    c.map(|x| (x / 1e-3).floor() as i64)
}

/// Result of matching.
struct Matching {
    unmatched_file: Vec<usize>,
    unmatched_stl: usize,
    worst_dev: f64,
}

/// Match `file` (already moved into the STL's frame) against `stl`, one to one.
fn match_triangles(file: &[Tri], stl: &[Tri]) -> Matching {
    let mut used = vec![false; stl.len()];
    let mut unmatched = Vec::new();
    let mut worst_dev = 0.0f64;
    // Fast path: same index.
    for (i, t) in file.iter().enumerate() {
        match stl.get(i) {
            Some(s) if rotated_dev(t, s) <= VERTEX_TOL_MM => {
                used[i] = true;
                worst_dev = worst_dev.max(rotated_dev(t, s));
            }
            _ => unmatched.push(i),
        }
    }
    // Slow path: the rest, by centroid cell.
    let mut cells: BTreeMap<[i64; 3], Vec<usize>> = BTreeMap::new();
    for (j, s) in stl.iter().enumerate().filter(|(j, _)| !used[*j]) {
        cells.entry(centroid_cell(s)).or_default().push(j);
    }
    let mut still = Vec::new();
    for &i in &unmatched {
        let c = centroid_cell(&file[i]);
        let mut found = None;
        'cells: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let key = [c[0] + dx, c[1] + dy, c[2] + dz];
                    for &j in cells.get(&key).into_iter().flatten() {
                        if !used[j] && rotated_dev(&file[i], &stl[j]) <= VERTEX_TOL_MM {
                            found = Some(j);
                            break 'cells;
                        }
                    }
                }
            }
        }
        match found {
            Some(j) => {
                used[j] = true;
                worst_dev = worst_dev.max(rotated_dev(&file[i], &stl[j]));
            }
            None => still.push(i),
        }
    }
    Matching {
        unmatched_stl: used.iter().filter(|u| !**u).count(),
        unmatched_file: still,
        worst_dev,
    }
}

/// The pose the model was given from the input STL, and the hash the STL should have.
///
/// The pose is a rotation about X (degrees, applied before the centring), as the recorded
/// keep-up pose of the Round bases is (+115 degrees; design section 0). `0.0` is no rotation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pose {
    pub rotate_x_deg: f64,
    /// SHA-256 the input STL is recorded to have (hex), if the caller states one.
    pub expected_sha256: Option<String>,
    /// SHA-256 of the STL bytes the checker was given (filled by the caller).
    pub actual_sha256: Option<String>,
}

fn rotate_x(tris: &[Tri], deg: f64) -> Vec<Tri> {
    if deg.abs() < 1e-12 {
        return tris.to_vec();
    }
    let (s, c) = deg.to_radians().sin_cos();
    tris.iter()
        .map(|t| t.map(|v| [v[0], v[1] * c - v[2] * s, v[1] * s + v[2] * c]))
        .collect()
}

/// H7 against the STL the file claims to come from, with no rotation.
#[must_use]
pub fn measure(scene: &Scene, stl: &Stl) -> Outcome {
    measure_with(scene, stl, &Pose::default())
}

/// H7 against the STL under the recorded pose.
#[must_use]
pub fn measure_with(scene: &Scene, stl: &Stl, pose: &Pose) -> Outcome {
    if let (Some(want), Some(got)) = (&pose.expected_sha256, &pose.actual_sha256)
        && !want.eq_ignore_ascii_case(got)
    {
        return Outcome::violation(format!(
            "the input STL has SHA-256 {got}, but the recorded one is {want}: not the file the pose was recorded for"
        ))
        .with_located(vec![at(None, None, None, "input STL hash differs from the record")]);
    }
    let reference = rotate_x(&stl_tris(stl), pose.rotate_x_deg);
    let c = centre_of(&reference);
    let (n_file, n_stl) = (scene.model.len(), reference.len());
    let (vol_f, area_f) = volume_area(&scene.model);
    let (vol_s, area_s) = volume_area(&reference);
    let detail = |matched: Option<&Matching>| {
        Json::obj([
            ("triangles_file", Json::Uint(n_file as u64)),
            ("triangles_stl", Json::Uint(n_stl as u64)),
            ("volume_file_mm3", Json::Num(vol_f)),
            ("volume_stl_mm3", Json::Num(vol_s)),
            ("area_file_mm2", Json::Num(area_f)),
            ("area_stl_mm2", Json::Num(area_s)),
            ("vertex_tolerance_mm", Json::Num(VERTEX_TOL_MM)),
            (
                "worst_matched_vertex_difference_mm",
                matched.map_or(Json::Null, |m| Json::Num(m.worst_dev)),
            ),
            (
                "pose",
                Json::str(format!(
                    "rotation {} degrees about X, then centred on the STL bounding box in f32; STL hash {}",
                    pose.rotate_x_deg,
                    match (&pose.expected_sha256, &pose.actual_sha256) {
                        (Some(_), Some(_)) => "matches the record",
                        _ => "not recorded, so not checked",
                    }
                )),
            ),
        ])
    };
    if n_file != n_stl {
        return Outcome::violation(format!(
            "the file holds {n_file} model triangles, the input STL {n_stl}"
        ))
        .with_located(vec![at(None, None, None, "triangle count differs")])
        .with_detail(detail(None));
    }
    let moved: Vec<Tri> = scene
        .model
        .iter()
        .map(|t| t.map(|v| [v[0] + c[0], v[1] + c[1], v[2] + c[2]]))
        .collect();
    let m = match_triangles(&moved, &reference);
    let mut problems = Vec::new();
    let mut located = Vec::new();
    if let Some(&first) = m.unmatched_file.first() {
        problems.push(format!(
            "{} model triangle(s) are not in the input STL (within {VERTEX_TOL_MM} mm, cyclic vertex order); {} STL triangle(s) are not in the file",
            m.unmatched_file.len(),
            m.unmatched_stl
        ));
        located.push(at(
            None,
            Some(scene.world(scene.model[first][0])),
            None,
            format!("model triangle {first} differs from the input"),
        ));
    }
    let rel = |a: f64, b: f64| (a - b).abs() / b.abs().max(1e-12);
    if rel(vol_f, vol_s) > 1e-5 || rel(area_f, area_s) > 1e-5 {
        problems.push(format!(
            "volume {vol_f} against {vol_s} mm3 or area {area_f} against {area_s} mm2 differ"
        ));
        located.push(at(None, None, None, "volume or area differs"));
    }
    let dxy = (scene.translate[0] - c[0]).hypot(scene.translate[1] - c[1]);
    if dxy > TRANSLATE_TOL_MM {
        problems.push(format!(
            "translate X, Y is {dxy} mm from where the input STL had the model"
        ));
        located.push(at(
            None,
            Some(scene.translate),
            None,
            "the model was moved in the plan view",
        ));
    }
    if problems.is_empty() {
        return Outcome::pass(format!(
            "all {n_file} model triangles equal the input STL's within {VERTEX_TOL_MM} mm (largest difference {:.2e} mm); volume and area agree; placed where the STL had it",
            m.worst_dev
        ))
        .with_detail(detail(Some(&m)));
    }
    Outcome::violation(problems.join("; "))
        .with_located(located)
        .with_detail(detail(Some(&m)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Status;
    use crate::stl::StlFormat;
    use crate::testkit::test_limits;

    fn tetra() -> Vec<Tri> {
        let (o, x, y, z) = (
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [0.0, 6.0, 0.0],
            [0.0, 0.0, 8.0],
        );
        vec![[o, y, x], [o, x, z], [o, z, y], [x, y, z]]
    }

    fn stl_of(tris: &[Tri]) -> Stl {
        #[allow(clippy::cast_possible_truncation, reason = "STL is f32")]
        let coords = tris.iter().flatten().flatten().map(|x| *x as f32).collect();
        Stl {
            format: StlFormat::Binary,
            coords,
        }
    }

    /// The file for `stl_tris`: centred on the bounding box, translate set to the centre.
    fn scene_for(stl_tris: &[Tri], edit: impl Fn(&mut Vec<Tri>)) -> (Scene, Stl) {
        let c = centre_of(stl_tris);
        let mut local: Vec<Tri> = stl_tris
            .iter()
            .map(|t| t.map(|v| [v[0] - c[0], v[1] - c[1], v[2] - c[2]]))
            .collect();
        edit(&mut local);
        (
            Scene::from_parts(&test_limits(), local, [c[0], c[1], 5.0], Vec::new()),
            stl_of(stl_tris),
        )
    }

    #[test]
    fn the_same_model_passes_even_with_reordered_triangles_and_rotated_vertices() {
        let (s, stl) = scene_for(&tetra(), |_| {});
        assert_eq!(measure(&s, &stl).status, Status::Pass);
        let (s2, stl2) = scene_for(&tetra(), |m| {
            m.reverse();
            for t in m.iter_mut() {
                t.rotate_left(1);
            }
        });
        assert_eq!(measure(&s2, &stl2).status, Status::Pass);
    }

    #[test]
    fn the_smallest_changes_are_violations() {
        let moved_vertex = |m: &mut Vec<Tri>| m[1][2][2] += 0.001;
        let dropped = |m: &mut Vec<Tri>| {
            m.pop();
        };
        let scaled = |m: &mut Vec<Tri>| {
            for v in m.iter_mut().flatten() {
                *v = [v[0] * 1.001, v[1] * 1.001, v[2] * 1.001];
            }
        };
        let reversed = |m: &mut Vec<Tri>| m[0].swap(1, 2);
        let nudged_beyond = |m: &mut Vec<Tri>| m[2][0][0] += 2e-5;
        type Edit<'a> = &'a dyn Fn(&mut Vec<Tri>);
        let cases: [(&str, Edit); 5] = [
            ("moved vertex", &moved_vertex),
            ("dropped triangle", &dropped),
            ("scale 1.001", &scaled),
            ("reversed winding", &reversed),
            ("2e-5 nudge", &nudged_beyond),
        ];
        for (name, edit) in cases {
            let (s, stl) = scene_for(&tetra(), edit);
            let o = measure(&s, &stl);
            assert_eq!(o.status, Status::Violation, "{name}: {o:?}");
            assert!(!o.located.is_empty(), "{name}");
        }
        // Inside the tolerance is accepted.
        let (s, stl) = scene_for(&tetra(), |m| m[2][0][0] += 5e-6);
        assert_eq!(measure(&s, &stl).status, Status::Pass);
    }

    #[test]
    fn a_recorded_rotation_about_x_is_a_pose_and_an_unrecorded_one_is_a_change() {
        // The file holds the tetrahedron turned 115 degrees about X, centred.
        let turned = rotate_x(&tetra(), 115.0);
        let (scene, _) = scene_for(&turned, |_| {});
        let stl = stl_of(&tetra());
        let with = |deg: f64, want: Option<&str>, got: Option<&str>| {
            measure_with(
                &scene,
                &stl,
                &Pose {
                    rotate_x_deg: deg,
                    expected_sha256: want.map(str::to_owned),
                    actual_sha256: got.map(str::to_owned),
                },
            )
        };
        assert_eq!(with(115.0, None, None).status, Status::Pass);
        assert_eq!(with(0.0, None, None).status, Status::Violation);
        assert_eq!(with(114.0, None, None).status, Status::Violation);
        // The recorded hash must match the STL's.
        assert_eq!(with(115.0, Some("ab"), Some("AB")).status, Status::Pass);
        let bad = with(115.0, Some("ab"), Some("cd"));
        assert_eq!(bad.status, Status::Violation, "{bad:?}");
    }

    #[test]
    fn an_undeclared_move_in_the_plan_view_is_a_violation() {
        let (mut s, stl) = scene_for(&tetra(), |_| {});
        s.translate[0] += 3.0;
        let o = measure(&s, &stl);
        assert_eq!(o.status, Status::Violation, "{o:?}");
    }
}
