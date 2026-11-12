//! Non-convex material sections, including holes, from the written model triangles.
//! A vertical decomposition pairs crossings by parity; it never takes a cavity's hull.

use super::{LAYER_MM, P2, ROUND_MM, Reading, Sec, Walk, overlap_by};
use crate::outcome::at;
use crate::scene::Scene;
use crate::v3::Aabb;
use std::collections::BTreeSet;

/// Scene-local resource limits; tests can force each refusal without global state.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Budget {
    pub segments: usize,
    pub sections: usize,
    pub strip_tests: usize,
    pub stored_sections: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            segments: 8192,
            sections: 16384,
            strip_tests: 4_000_000,
            stored_sections: 2_000_000,
        }
    }
}

type Segment = [P2; 2];

fn segments(scene: &Scene, z: f64) -> Result<Vec<Segment>, String> {
    let bb = Aabb {
        lo: [f64::NEG_INFINITY, f64::NEG_INFINITY, z],
        hi: [f64::INFINITY, f64::INFINITY, z],
    };
    let mut out = Vec::new();
    for i in scene.model_near(&bb) {
        let t = scene.model[i as usize];
        let mut points = [[0.0; 2]; 2];
        let mut count = 0;
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            // Half-open edge ownership avoids duplicate crossings at a vertex.
            if (a[2] <= z && b[2] > z) || (b[2] <= z && a[2] > z) {
                let s = (z - a[2]) / (b[2] - a[2]);
                points[count] = [a[0] + s * (b[0] - a[0]), a[1] + s * (b[1] - a[1])];
                count += 1;
            }
        }
        if count == 2 && (points[0][0] - points[1][0]).hypot(points[0][1] - points[1][1]) > 1e-12 {
            out.push(points);
        }
    }
    if out.len() > scene.model_slice_budget.segments {
        return Err("model section exceeds the 8192-segment budget".to_owned());
    }
    Ok(out)
}

fn y_at(s: &Segment, x: f64) -> f64 {
    s[0][1] + (x - s[0][0]) * (s[1][1] - s[0][1]) / (s[1][0] - s[0][0])
}

/// Convex tiles whose union is exactly the material section, not its filled outline.
pub(super) fn sections(scene: &Scene, z: f64) -> Result<Vec<Sec>, String> {
    let seg = segments(scene, z)?;
    let mut xs: Vec<f64> = seg.iter().flatten().map(|p| p[0]).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
    if xs.len().saturating_mul(seg.len()) > scene.model_slice_budget.strip_tests {
        return Err("model slice exceeds four million strip/segment tests".to_owned());
    }
    let mut out = Vec::new();
    let mut crossings: Vec<&Segment> = Vec::new();
    for ab in xs.windows(2) {
        if ab[1] - ab[0] < 1e-9 {
            continue;
        }
        let x = 0.5 * (ab[0] + ab[1]);
        crossings.clear();
        crossings.extend(
            seg.iter()
                .filter(|s| x > s[0][0].min(s[1][0]) && x < s[0][0].max(s[1][0])),
        );
        crossings.sort_by(|a, b| y_at(a, x).total_cmp(&y_at(b, x)));
        if !crossings.len().is_multiple_of(2) {
            return Err("model section has an odd crossing count".to_owned());
        }
        for pair in crossings.chunks_exact(2) {
            let poly = vec![
                [ab[0], y_at(pair[0], ab[0])],
                [ab[1], y_at(pair[0], ab[1])],
                [ab[1], y_at(pair[1], ab[1])],
                [ab[0], y_at(pair[1], ab[0])],
            ];
            if super::poly_area(&poly) > 1e-14 {
                out.push(Sec::from_poly(poly));
            }
            if out.len() > scene.model_slice_budget.sections {
                return Err("model section exceeds the 16384-tile budget".to_owned());
            }
        }
    }
    Ok(out)
}

/// Tiles of the same model share a real edge, not a gap. A point contact is not enough.
pub(super) fn joined(a: &Sec, b: &Sec) -> bool {
    let shared_x = (a.bb[2].min(b.bb[2]) - a.bb[0].max(b.bb[0])).abs() < 1e-10;
    let shared_y = (a.bb[3].min(b.bb[3]) - a.bb[1].max(b.bb[1])).abs() < 1e-10;
    let edge = |p: P2, q: P2, r: P2, s: P2| {
        let d = [q[0] - p[0], q[1] - p[1]];
        let l = d[0].hypot(d[1]);
        if l < super::OVERLAP_MM {
            return false;
        }
        let along = |v: P2| ((v[0] - p[0]) * d[0] + (v[1] - p[1]) * d[1]) / l;
        let off = |v: P2| ((v[0] - p[0]) * d[1] - (v[1] - p[1]) * d[0]).abs() / l;
        off(r) < 1e-10
            && off(s) < 1e-10
            && along(r).max(along(s)).min(l) - along(r).min(along(s)).max(0.0) > super::OVERLAP_MM
    };
    (shared_x || shared_y)
        && a.poly.iter().enumerate().any(|(i, p)| {
            b.poly.iter().enumerate().any(|(j, r)| {
                edge(
                    *p,
                    a.poly[(i + 1) % a.poly.len()],
                    *r,
                    b.poly[(j + 1) % b.poly.len()],
                )
            })
        })
}

/// Include model events in each layer's plane batch when checking model-standing feet.
pub(super) fn events(scene: &Scene, bottom: f64, top: f64, planes: &mut Vec<f64>) {
    let bb = Aabb {
        lo: [f64::NEG_INFINITY, f64::NEG_INFINITY, bottom],
        hi: [f64::INFINITY, f64::INFINITY, top],
    };
    for i in scene.model_near(&bb) {
        for p in scene.model[i as usize] {
            if p[2] > bottom + super::EDGE_MARGIN && p[2] < top - super::EDGE_MARGIN {
                planes.push(p[2] - super::PLANE_DELTA);
                planes.push(p[2] + super::PLANE_DELTA);
            }
        }
    }
    planes.sort_by(f64::total_cmp);
    planes.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
}

/// Compute first-print events once, not per foot at every sample plane.
pub(super) fn foot_events(
    scene: &Scene,
    feet: &BTreeSet<usize>,
    layers: usize,
    z0: f64,
    reading: Reading,
) -> Vec<Vec<(usize, f64)>> {
    let mut events = vec![Vec::new(); layers];
    for &pi in feet {
        if let Some(start) = super::seat::first_thick(&scene.parts[pi], reading.sliver) {
            let z = start + ROUND_MM;
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "bounded layer index"
            )]
            let k = ((z - z0) / LAYER_MM).max(0.0).floor() as usize;
            if let Some(batch) = events.get_mut(k) {
                batch.push((pi, z));
            }
        }
    }
    events
}

/// A separate rooted support cannot disguise a foot with only a future model attachment.
pub(super) fn check_foot_starts(
    state: &mut Walk<'_>,
    secs: &[(usize, Sec)],
    events: &[(usize, f64)],
    z: f64,
    k: usize,
    reading: Reading,
) {
    let scene = state.scene;
    for &(pi, start) in events {
        if state.reported.contains(&pi) || (z - start).abs() > ROUND_MM {
            continue;
        }
        let own = secs.iter().find(|(part, _)| *part == pi);
        let Some((_, own)) = own else {
            state.reported.insert(pi);
            state.relies_on_model.push(format!(
                "record {} model-standing foot has no section within ROUND_MM of its first thick height; its start is unverified",
                scene.parts[pi].rec
            ));
            continue;
        };
        if !secs.iter().any(|(part, material)| {
            *part == scene.parts.len() && overlap_by(own, material, reading.depth)
        }) {
            state.reported.insert(pi);
            let mut place = scene.parts[pi].bbox.centre();
            place[2] = z;
            state.located.push(at(
                Some(scene.parts[pi].rec),
                Some(scene.world(place)),
                u32::try_from(k).ok(),
                "model-standing foot has no model-material overlap at its first printable section; later model attachment cannot root its start",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{closed_box, test_limits};

    #[test]
    fn unavailable_model_slices_keep_floating_columns_and_foot_reliance() {
        use crate::{h3, patch, report::Status, testkit as kit};
        for budget_kind in 0..5 {
            let mut mesh = closed_box([-2.0, -2.0, 0.0], [2.0, 2.0, 2.0]);
            if budget_kind == 0 {
                mesh.pop();
            }
            let mut s = Scene::from_parts(
                &test_limits(),
                mesh,
                [0.0, 0.0, 5.0],
                vec![
                    kit::part(0, 8, [0.0, 0.0, -0.3], [0.0, 0.0, 0.3], 0.3, 0.3),
                    kit::part(1, 3, [8.0, 0.0, -1.0], [8.0, 0.0, 1.0], 0.3, 0.3),
                    kit::raft_pad(2, [-8.0, 0.0], -4.8),
                ],
            );
            match budget_kind {
                1 => s.model_slice_budget.segments = 0,
                2 => s.model_slice_budget.sections = 0,
                3 => s.model_slice_budget.strip_tests = 0,
                4 => s.model_slice_budget.stored_sections = 0,
                _ => {}
            }
            let p = patch::measure(&s);
            assert!(
                h3::feet_of(&s, &p).contains(&0),
                "must exercise a model-standing foot"
            );
            let w = h3::walk(&s, &p, Reading::strict()).expect("support walk continues");
            assert!(w.located.iter().any(|l| l.location.record == Some(1)));
            assert!(
                w.relies_on_model.iter().any(|why| why.contains("record 0")),
                "{:?}",
                w.relies_on_model
            );
            assert!(
                w.relies_on_model
                    .iter()
                    .any(|why| why.contains("model slicing unavailable"))
            );
            let out = h3::measure(&s, &p);
            assert_eq!(out.status, Status::Violation, "case {budget_kind}: {out:?}");
            assert!(
                out.located.iter().any(|l| l.location.record == Some(1)),
                "{out:?}"
            );
            assert!(
                out.detail.render().contains("record 0"),
                "foot uncertainty remains reported: {out:?}"
            );
            if budget_kind > 0 {
                assert!(
                    out.located.iter().any(|l| l.location.record.is_none()),
                    "unsupported model island must survive the budget fallback: {out:?}"
                );
            }
        }
    }

    #[test]
    fn a_missing_first_thick_section_is_unverified_not_silently_skipped() {
        use crate::testkit as kit;
        let s = Scene::from_parts(
            &test_limits(),
            closed_box([-2.0; 3], [2.0; 3]),
            [0.0; 3],
            vec![kit::part(0, 8, [0.0, 0.0, -0.3], [0.0, 0.0, 0.3], 0.3, 0.3)],
        );
        let mut state = Walk {
            scene: &s,
            z0: -5.0,
            reported: BTreeSet::new(),
            located: vec![],
            relies_on_model: vec![],
            pieces: 0,
            unrooted_pieces: 0,
            planes_total: 0,
        };
        check_foot_starts(&mut state, &[], &[(0, 0.1)], 0.1, 0, Reading::strict());
        assert!(state.located.is_empty());
        assert_eq!(state.relies_on_model.len(), 1);
        assert!(state.relies_on_model[0].contains("no section within ROUND_MM"));
    }

    #[test]
    fn a_section_keeps_the_void_empty() {
        let mut model = closed_box([-3.0; 3], [3.0; 3]);
        model.extend(
            closed_box([-2.0; 3], [2.0; 3])
                .into_iter()
                .map(|[a, b, c]| [a, c, b]),
        );
        let scene = Scene::from_parts(&test_limits(), model, [0.0; 3], vec![]);
        let secs = sections(&scene, 0.123).expect("section");
        let area: f64 = secs.iter().map(|s| super::super::poly_area(&s.poly)).sum();
        assert!((area - 20.0).abs() < 1e-8, "{area}");
        assert!(
            secs.iter()
                .all(|s| !super::super::inside([0.0; 2], &s.poly))
        );
    }
}
