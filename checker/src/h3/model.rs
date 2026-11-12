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

fn segments(scene: &Scene, z: f64, region: [f64; 4]) -> Result<Vec<Segment>, String> {
    // The vertical parity rays need crossings outside the Y window too. The BVH
    // query is bounded in X and Z; only tiles clipped to the XY window are stored.
    let bb = Aabb {
        lo: [region[0], f64::NEG_INFINITY, z],
        hi: [region[2], f64::INFINITY, z],
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
pub(super) fn sections(scene: &Scene, z: f64, region: [f64; 4]) -> Result<Vec<Sec>, String> {
    let seg = segments(scene, z, region)?;
    let mut xs: Vec<f64> = seg
        .iter()
        .flatten()
        .map(|p| p[0])
        .filter(|x| *x > region[0] && *x < region[2])
        .collect();
    xs.extend([region[0], region[2]]);
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
            let poly = clip_y(clip_y(poly, region[1], true), region[3], false);
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

fn clip_y(poly: Vec<P2>, y: f64, above: bool) -> Vec<P2> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (da, db) = (a[1] - y, b[1] - y);
        if if above { da >= 0.0 } else { da <= 0.0 } {
            out.push(a);
        }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            out.push([a[0] + (b[0] - a[0]) * da / (da - db), y]);
        }
    }
    out
}

/// A bounded proof neighbourhood around model-standing feet. Regions that overlap
/// are merged once before the walk, so nearby feet share slices, not recompute them.
/// A path leaving these windows is not proved rooted: the foot retains reliance
/// (unverified), never a pass or a fabricated floating-part violation.
#[derive(Debug)]
pub(super) struct Window {
    pub xy: [f64; 4],
    pub top: f64,
    pub can_root: bool,
}

pub(super) fn windows(scene: &Scene, feet: &BTreeSet<usize>) -> Vec<Window> {
    let mut out: Vec<Window> = Vec::new();
    for &pi in feet {
        let bb = scene.parts[pi].bbox.inflated(super::CELL_MM);
        let mut next = Window {
            xy: [bb.lo[0], bb.lo[1], bb.hi[0], bb.hi[1]],
            top: scene.parts[pi].bbox.hi[2],
            can_root: false,
        };
        let mut i = 0;
        while i < out.len() {
            let b = out[i].xy;
            if next.xy[0] <= b[2] && b[0] <= next.xy[2] && next.xy[1] <= b[3] && b[1] <= next.xy[3]
            {
                let old = out.remove(i);
                next.xy = [
                    next.xy[0].min(b[0]),
                    next.xy[1].min(b[1]),
                    next.xy[2].max(b[2]),
                    next.xy[3].max(b[3]),
                ];
                next.top = next.top.max(old.top);
                i = 0;
            } else {
                i += 1;
            }
        }
        out.push(next);
    }
    // A conservative broad phase, not a rooting proof: even connections through
    // future support count here. If no plate-reachable box can enter a window's
    // material, propagation inside it cannot prove anything. Still slice every
    // foot's first-print event, to reject future-only model attachments.
    let boxes: Vec<_> = scene
        .parts
        .iter()
        .map(|p| Aabb::of_points(p.tris.iter().flatten().copied()).inflated(super::TOUCH_MM))
        .collect();
    let tree = crate::tri::Bvh::build(&boxes);
    let mut reached = vec![false; boxes.len()];
    let mut queue: Vec<_> = boxes
        .iter()
        .enumerate()
        .filter(|(_, b)| b.lo[2] <= scene.plate_z() + super::ON_PLATE_MM)
        .map(|(i, _)| i)
        .collect();
    for &i in &queue {
        reached[i] = true;
    }
    let mut at = 0;
    let mut near = Vec::new();
    while at < queue.len() {
        near.clear();
        tree.query(&boxes[queue[at]], &mut near);
        for &i in &near {
            let i = i as usize;
            if !reached[i] {
                reached[i] = true;
                queue.push(i);
            }
        }
        at += 1;
    }
    for w in &mut out {
        let material = Aabb {
            lo: [w.xy[0], w.xy[1], scene.model_min_z],
            hi: [w.xy[2], w.xy[3], w.top],
        };
        w.can_root = boxes
            .iter()
            .zip(&reached)
            .any(|(b, r)| *r && b.overlaps(&material));
    }
    out
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
pub(super) fn events(
    scene: &Scene,
    bottom: f64,
    top: f64,
    region: [f64; 4],
    planes: &mut Vec<f64>,
) {
    let bb = Aabb {
        lo: [region[0], region[1], bottom],
        hi: [region[2], region[3], top],
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

/// Batch local material evidence for newly unrooted support sections. This can
/// only make a finding uncertain, never root a part. Contact is with the union
/// of tiles; a slice refusal also cannot prove that a section is in clear air.
pub(super) fn possible_carriers(
    scene: &Scene,
    z: f64,
    candidates: &[(usize, &Sec)],
) -> (
    std::collections::BTreeMap<usize, &'static str>,
    usize,
    usize,
) {
    if !scene.model_closed || z < scene.model_min_z {
        return (std::collections::BTreeMap::new(), 0, 0);
    }
    let mut groups: Vec<([f64; 4], Vec<usize>)> = Vec::new();
    for (pi, s) in candidates {
        let mut bb = [
            s.bb[0] - ROUND_MM,
            s.bb[1] - ROUND_MM,
            s.bb[2] + ROUND_MM,
            s.bb[3] + ROUND_MM,
        ];
        let mut ids = vec![*pi];
        let mut i = 0;
        while i < groups.len() {
            let b = groups[i].0;
            if bb[0] <= b[2] && b[0] <= bb[2] && bb[1] <= b[3] && b[1] <= bb[3] {
                let (_, old) = groups.remove(i);
                ids.extend(old);
                bb = [
                    bb[0].min(b[0]),
                    bb[1].min(b[1]),
                    bb[2].max(b[2]),
                    bb[3].max(b[3]),
                ];
                i = 0;
            } else {
                i += 1;
            }
        }
        groups.push((bb, ids));
    }
    let mut possible = std::collections::BTreeMap::new();
    let (mut batches, mut tiles) = (0, 0);
    for (bb, ids) in groups {
        batches += 1;
        match sections(scene, z, bb) {
            Ok(material) => {
                tiles += material.len();
                for (pi, own) in candidates.iter().filter(|(pi, _)| ids.contains(pi)) {
                    if material
                        .iter()
                        .any(|s| overlap_by(own, s, super::OVERLAP_MM + ROUND_MM))
                    {
                        possible.insert(*pi, "overlaps local model material by the strict depth");
                    } else if material.iter().any(|s| overlap_by(own, s, -ROUND_MM)) {
                        possible.insert(*pi, "partly overlaps local model material");
                    }
                }
            }
            Err(_) => possible.extend(ids.into_iter().map(|pi| {
                (
                    pi,
                    "may overlap model material: the local slice is unavailable",
                )
            })),
        }
    }
    (possible, batches, tiles)
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
        let material: Vec<_> = secs
            .iter()
            .filter(|(part, _)| *part == scene.parts.len())
            .map(|(_, s)| s)
            .collect();
        if material.iter().any(|s| overlap_by(own, s, reading.depth)) {
            continue;
        }
        state.reported.insert(pi);
        // A union touches the foot iff at least one of its tiles touches it. A
        // partial overlap, or an overlap split over tiles, is not proof of air.
        if material.iter().any(|s| overlap_by(own, s, -ROUND_MM)) {
            state.relies_on_model.push(format!(
                "record {} model-standing foot partly overlaps model material at its first printable section; continuous support of the thin start is not proved",
                scene.parts[pi].rec
            ));
        } else {
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
                    kit::part(0, 8, [1.9, 0.0, 0.1], [1.9, 0.0, 0.7], 0.3, 0.3),
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
    fn partial_or_tile_split_foot_contact_is_uncertain_but_clear_air_is_a_violation() {
        use crate::testkit as kit;
        let scene = Scene::from_parts(
            &test_limits(),
            closed_box([-2.0; 3], [2.0; 3]),
            [0.0; 3],
            vec![kit::part(0, 8, [0.0, 0.0, -0.3], [0.0, 0.0, 0.3], 0.3, 0.3)],
        );
        let rect = |x0, x1| Sec::from_poly(vec![[x0, -1.0], [x1, -1.0], [x1, 1.0], [x0, 1.0]]);
        let own = Sec::from_poly(vec![
            [-0.006, -0.1],
            [0.006, -0.1],
            [0.006, 0.1],
            [-0.006, 0.1],
        ]);
        for (tiles, certain) in [
            (vec![rect(-0.003, 1.0)], false),
            (vec![rect(-1.0, 0.0), rect(0.0, 1.0)], false),
            (vec![rect(2.0, 3.0)], true),
        ] {
            let mut state = Walk {
                scene: &scene,
                z0: -5.0,
                reported: BTreeSet::new(),
                located: vec![],
                relies_on_model: vec![],
                pieces: 0,
                unrooted_pieces: 0,
                planes_total: 0,
            };
            let mut secs = vec![(0, own.clone())];
            secs.extend(tiles.into_iter().map(|s| (1, s)));
            check_foot_starts(&mut state, &secs, &[(0, 0.1)], 0.1, 0, Reading::strict());
            assert_eq!(state.located.len(), usize::from(certain));
            assert_eq!(state.relies_on_model.len(), usize::from(!certain));
        }
    }

    #[test]
    fn no_model_touching_foot_skips_all_material_work_even_with_zero_budgets() {
        use crate::{h3, patch, testkit as kit};
        let mut s = Scene::from_parts(
            &test_limits(),
            closed_box([-2.0, -2.0, 0.0], [2.0, 2.0, 2.0]),
            [0.0, 0.0, 5.0],
            vec![
                kit::raft_pad(0, [0.0, 0.0], -4.8),
                kit::part(1, 3, [0.0, 0.0, -4.85], [0.0, 0.0, 0.1], 0.4, 0.4),
            ],
        );
        s.model_slice_budget = Budget {
            segments: 0,
            sections: 0,
            strip_tests: 0,
            stored_sections: 0,
        };
        let p = patch::measure(&s);
        assert!(h3::feet_of(&s, &p).is_empty());
        let w = h3::walk(&s, &p, Reading::strict()).expect("support walk");
        assert_eq!((w.model_slice_batches, w.model_tiles), (0, 0));
        assert!(w.relies_on_model.is_empty());
        assert!(w.located.is_empty());
    }

    #[test]
    fn local_parity_keeps_offscreen_void_boundaries_and_clips_material() {
        let mut model = closed_box([-3.0; 3], [3.0; 3]);
        model.extend(
            closed_box([-2.0; 3], [2.0; 3])
                .into_iter()
                .map(|[a, b, c]| [a, c, b]),
        );
        let scene = Scene::from_parts(&test_limits(), model, [0.0; 3], vec![]);
        assert!(
            sections(&scene, 0.123, [-0.5, -0.5, 0.5, 0.5])
                .expect("void")
                .is_empty()
        );
        let tiles = sections(&scene, 0.123, [-0.5, 1.5, 0.5, 2.5]).expect("local wall");
        let area: f64 = tiles.iter().map(|s| super::super::poly_area(&s.poly)).sum();
        assert!((area - 0.5).abs() < 1e-8, "{area}");
        assert!(
            tiles
                .iter()
                .all(|s| s.bb[0] >= -0.5 && s.bb[1] >= 2.0 && s.bb[2] <= 0.5 && s.bb[3] <= 2.5)
        );
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
        let secs = sections(&scene, 0.123, [-3.0, -3.0, 3.0, 3.0]).expect("section");
        let area: f64 = secs.iter().map(|s| super::super::poly_area(&s.poly)).sum();
        assert!((area - 20.0).abs() < 1e-8, "{area}");
        assert!(
            secs.iter()
                .all(|s| !super::super::inside([0.0; 2], &s.poly))
        );
    }
}
