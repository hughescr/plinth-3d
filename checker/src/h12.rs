//! H1 (contact patch area) and H2 (no fused contacts) from the patch measurement.
//!
//! **Verdict scope.** Both rules are measured on the written triangles (`PatchOut::mesh`);
//! the verdict is valid for ChituBox v1.4.0 if it slices the stored support meshes, pending the
//! H8 differential slice test. The patches of the solid the fields name are reported as
//! information (`analytic_information`) and decide nothing.
//!
//! **H1.** Every contact patch is at most 0.50 mm2. A patch passes only if its upper bound is
//! within the limit; it violates only if the lower bound of a certainly-connected patch is
//! above it; anything between is unverified.
//!
//! Every patch interval must also be no wider than 0.010 mm2 per patch (design 7.2): a wider
//! interval (a cap coplanar with a model face, say) is a measurement that did not resolve the
//! patch and is unverified, whatever its upper end.
//!
//! **H2.** Two distinct patches closer than 0.010 mm are fused: patches of different chains
//! (including a patch shared by two chains, distance 0) and also two patches of one chain
//! (for instance on the two faces of a thin wall). Certain when the upper bound of the
//! distance is under it; unverified when only the lower bound is, or when a patch may be
//! several.

use crate::json::Json;
use crate::outcome::{MAX_LOCATED, MEASURED_ON, Outcome, VERDICT_SCOPE, at};
use crate::patch::{Comp, FUSE_MM, PatchOut, RepOut};
use crate::scene::Scene;
use crate::status::Located;

/// The widest interval a patch area may have (per patch), mm2 (design 7.2).
pub const MAX_WIDTH_MM2: f64 = 0.010;

fn rec_of(scene: &Scene, chain: usize) -> usize {
    scene.parts[scene.chains.chains[chain].parts[0]].rec
}

fn rep_json(scene: &Scene, r: &RepOut) -> Json {
    let mut comps: Vec<_> = r.comps.iter().collect();
    comps.sort_by(|a, b| b.hi.total_cmp(&a.hi));
    let top: Vec<Json> = comps
        .iter()
        .take(10)
        .map(|c| {
            Json::obj([
                ("record", Json::Uint(rec_of(scene, c.chain) as u64)),
                ("area_lower_mm2", Json::Num(c.certain_lo)),
                ("area_upper_mm2", Json::Num(c.hi)),
            ])
        })
        .collect();
    Json::obj([
        ("representation", Json::str(r.rep.name())),
        ("patches", Json::Uint(r.comps.len() as u64)),
        ("largest_upper_mm2", Json::Num(r.max_hi)),
        ("largest_lower_mm2", Json::Num(r.max_certain_lo)),
        (
            "widest_interval_mm2",
            Json::Num(
                r.comps
                    .iter()
                    .map(|c| c.hi - c.certain_lo)
                    .fold(0.0, f64::max),
            ),
        ),
        ("largest", Json::Arr(top)),
    ])
}

fn inside_patch(scene: &Scene, c: &Comp) -> bool {
    !c.parts.is_empty() && c.parts.iter().all(|&pi| scene.wholly_inside(pi))
}

fn inside_chain(scene: &Scene, ci: usize) -> bool {
    let parts = &scene.chains.chains[ci].parts;
    !parts.is_empty() && parts.iter().all(|&pi| scene.wholly_inside(pi))
}

/// H1.
#[must_use]
pub fn h1(scene: &Scene, p: &PatchOut) -> Outcome {
    if scene.parts.is_empty() {
        return Outcome::not_applicable(
            "the file has no support records: no contact patch exists on the model",
        );
    }
    let limit = scene.limits.h1_patch_area_mm2;
    let r = p.mesh();
    let detail = Json::obj([
        ("limit_mm2", Json::Num(limit)),
        ("measured_on", Json::str(MEASURED_ON)),
        ("verdict_scope", Json::str(VERDICT_SCOPE)),
        ("written_mesh", rep_json(scene, r)),
        ("analytic_information", rep_json(scene, p.analytic())),
    ]);
    let mut located: Vec<Located> = Vec::new();
    let outside: Vec<_> = r.comps.iter().filter(|c| !inside_patch(scene, c)).collect();
    for c in outside.iter().filter(|c| c.certain_lo > limit) {
        located.push(at(
            Some(rec_of(scene, c.chain)),
            Some(scene.world(c.at)),
            None,
            format!(
                "a contact patch of the written mesh is at least {:.4} mm2, over the {limit} mm2 limit",
                c.certain_lo
            ),
        ));
    }
    if !located.is_empty() {
        located.truncate(MAX_LOCATED);
        return Outcome::violation(format!(
            "a contact patch is certainly over {limit} mm2 (lower bound {:.4} mm2)",
            r.max_certain_lo
        ))
        .with_located(located)
        .with_detail(detail);
    }
    let upper = outside.iter().map(|c| c.hi).fold(0.0, f64::max);
    if !p.unverified.is_empty() {
        return Outcome::unverified(format!(
            "{} part(s) or chain(s) could not be measured; first: {}",
            p.unverified.len(),
            p.unverified[0]
        ))
        .with_detail(detail);
    }
    if !r.loose.is_empty() {
        return Outcome::unverified(
            "a chain makes too many overlapping polygons on one triangle to bracket exactly",
        )
        .with_detail(detail);
    }
    // Every patch interval must be narrow enough to mean something: a pass rests on the
    // upper end, and an upper end far above the lower end is a measurement that did not
    // resolve the patch (a coplanar cap, say), whatever the limit.
    let widest = outside
        .iter()
        .map(|c| (c.hi - c.certain_lo, c))
        .max_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((width, c)) = widest.filter(|(w, _)| *w > MAX_WIDTH_MM2) {
        return Outcome::unverified(format!(
            "a contact patch of record {} is only bracketed to [{:.4}, {:.4}] mm2 on the written mesh: the interval is {width:.4} mm2 wide, over the {MAX_WIDTH_MM2} mm2 the checker requires",
            rec_of(scene, c.chain),
            c.certain_lo,
            c.hi
        ))
        .with_located(vec![at(
            Some(rec_of(scene, c.chain)),
            Some(scene.world(c.at)),
            None,
            "this patch interval is too wide to decide",
        )])
        .with_detail(detail);
    }
    if upper > limit {
        return Outcome::unverified(format!(
            "the largest patch is in [{:.4}, {upper:.4}] mm2, straddling the {limit} mm2 limit",
            r.max_certain_lo
        ))
        .with_detail(detail);
    }
    Outcome::pass(format!(
        "{} contact patch(es); the largest has an upper bound of {upper:.4} mm2 on the written mesh (limit {limit} mm2)",
        r.comps.len()
    ))
    .with_detail(detail)
}

/// H2.
#[must_use]
pub fn h2(scene: &Scene, p: &PatchOut) -> Outcome {
    if scene.parts.is_empty() {
        return Outcome::not_applicable(
            "the file has no support records: no contact patch exists on the model",
        );
    }
    let mut located: Vec<Located> = Vec::new();
    let mut maybe = 0usize;
    let mut closest_lb = f64::INFINITY;
    let r = p.mesh();
    // A patch that may be one or several (its pieces touch only by their outer polygons) is
    // not read as one.
    maybe += r
        .comps
        .iter()
        .filter(|c| c.split && !inside_patch(scene, c))
        .count();
    for g in &r.pairs {
        if inside_chain(scene, g.a) && inside_chain(scene, g.b) {
            continue;
        }
        closest_lb = closest_lb.min(g.dlb);
        if g.dub < FUSE_MM {
            located.push(at(
                Some(rec_of(scene, g.a)),
                Some(scene.world(g.at)),
                None,
                format!(
                    "patches of records {} and {} are at most {:.4} mm apart on the written mesh, under {FUSE_MM} mm: fused",
                    rec_of(scene, g.a),
                    rec_of(scene, g.b),
                    g.dub
                ),
            ));
        } else if g.dlb < FUSE_MM {
            maybe += 1;
        }
    }
    let analytic_closest = p
        .analytic()
        .pairs
        .iter()
        .map(|g| g.dlb)
        .fold(f64::INFINITY, f64::min);
    let detail = Json::obj([
        ("measured_on", Json::str(MEASURED_ON)),
        ("verdict_scope", Json::str(VERDICT_SCOPE)),
        (
            "analytic_information_closest_pair_lower_mm",
            if analytic_closest.is_finite() {
                Json::Num(analytic_closest)
            } else {
                Json::Null
            },
        ),
        ("fuse_distance_mm", Json::Num(FUSE_MM)),
        (
            "closest_pair_lower_mm",
            if closest_lb.is_finite() {
                Json::Num(closest_lb)
            } else {
                Json::Null
            },
        ),
        ("fused_pairs", Json::Uint(located.len() as u64)),
        ("undecided_pairs", Json::Uint(maybe as u64)),
    ]);
    if !located.is_empty() {
        let n = located.len();
        located.truncate(MAX_LOCATED);
        return Outcome::violation(format!(
            "{n} pair(s) of contact patches closer than {FUSE_MM} mm"
        ))
        .with_located(located)
        .with_detail(detail);
    }
    if !p.unverified.is_empty() || maybe > 0 {
        let why = p.unverified.first().cloned().unwrap_or_else(|| {
            format!(
                "{maybe} pair(s) of patches are between the fused and clear distances, or a patch may be several"
            )
        });
        return Outcome::unverified(format!("fused contacts not decided: {why}"))
            .with_detail(detail);
    }
    Outcome::pass(format!(
        "no two contact patches of different supports are closer than {FUSE_MM} mm (closest listed pair at least {} mm apart)",
        if closest_lb.is_finite() {
            format!("{closest_lb:.4}")
        } else {
            format!("{:.3}", FUSE_MM + 1e-3)
        }
    ))
    .with_detail(detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch;
    use crate::report::Status;
    use crate::testkit::{self as kit, test_limits};

    fn scene(model: Vec<crate::tri::Tri>, parts: Vec<crate::parts::Part>) -> Scene {
        Scene::from_parts(&test_limits(), model, [0.0; 3], parts)
    }

    #[test]
    fn two_patches_of_one_ball_on_the_two_faces_of_a_thin_slab_are_fused() {
        // A radius 0.1 ball inside a closed slab 0.005 mm thick touches both faces: two
        // patches of one chain, 0.005 mm apart.
        let slab = kit::closed_box([-5.0, -5.0, -0.0025], [5.0, 5.0, 0.0025]);
        let ball = kit::part(0, 7, [0.0, 0.0, -0.1], [0.0, 0.0, 0.1], 0.1, 0.1);
        let s = scene(slab, vec![ball]);
        let p = patch::measure(&s);
        assert_eq!(p.reps[0].comps.len(), 2, "{:?}", p.reps[0].comps);
        let o = h2(&s, &p);
        assert_eq!(o.status, Status::Violation, "{o:?}");
        // The same ball on a slab 0.05 mm thick: two patches, 0.05 mm apart, not fused.
        let slab = kit::closed_box([-5.0, -5.0, -0.025], [5.0, 5.0, 0.025]);
        let ball = kit::part(0, 7, [0.0, 0.0, -0.1], [0.0, 0.0, 0.1], 0.1, 0.1);
        let s = scene(slab, vec![ball]);
        let p = patch::measure(&s);
        assert_eq!(h2(&s, &p).status, Status::Pass);
    }

    #[test]
    fn a_written_cap_off_its_fields_by_more_than_the_rounding_is_refused_not_decoded() {
        // The fields end 8e-6 mm below the model face z = 0, the written cap stands 2e-6 mm
        // above it: 1e-5 mm off the fields, far over 2 units in the last place at z = 5
        // (9.5e-7 mm). The canonical-mesh validation refuses the record, so H1 cannot pass.
        let plate = kit::closed_box([-5.0, -5.0, 0.0], [5.0, 5.0, 2.0]);
        let (s, e) = ([0.0, 0.0, -5.0], [0.0, 0.0, -0.000_008]);
        let rec = kit::rec(3, s, e, 0.45, 0.45);
        let coords = kit::frustum_coords(s, [0.0, 0.0, 0.000_002], 0.45, 0.45);
        let rod = kit::part_with_mesh(0, &rec, &coords);
        assert!(
            rod.invalid.is_some(),
            "a cap 1e-5 mm off the fields is not canonical"
        );
        let s = scene(plate, vec![rod]);
        let p = patch::measure(&s);
        let o = h1(&s, &p);
        assert_ne!(o.status, Status::Pass, "{o:?}");
    }

    #[test]
    fn a_written_cap_inside_the_model_by_the_rounding_is_a_patch_of_the_mesh_not_a_pass() {
        // The fields end 4e-7 mm below the model face z = 0, the written cap stands 4e-7 mm
        // above it: 8e-7 mm off the fields, inside the 2 units in the last place (9.5e-7 mm at
        // z = 5) the canonical mesh allows. The analytic solid has no patch; the mesh is cut
        // across its whole 16-gon (0.62 mm2, over the 0.50 limit). The broad phase must not
        // reject it by the analytic extent.
        let plate = kit::closed_box([-5.0, -5.0, 0.0], [5.0, 5.0, 2.0]);
        let (s, e) = ([0.0, 0.0, -5.0], [0.0, 0.0, -0.000_000_4]);
        let rec = kit::rec(3, s, e, 0.45, 0.45);
        let coords = kit::frustum_coords(s, [0.0, 0.0, 0.000_000_4], 0.45, 0.45);
        let rod = kit::part_with_mesh(0, &rec, &coords);
        assert!(rod.is_valid(), "{:?}", rod.invalid);
        let s = scene(plate, vec![rod]);
        let p = patch::measure(&s);
        assert!(p.reps[0].comps.is_empty(), "{:?}", p.reps[0].comps);
        assert!(
            p.reps[1].max_hi > 0.5,
            "the mesh patch must be found: {:?}",
            p.reps[1]
        );
        let o = h1(&s, &p);
        assert_ne!(o.status, Status::Pass, "{o:?}");
    }

    #[test]
    fn a_rod_cap_coplanar_with_a_model_face_is_not_bracketed_tightly_enough_to_pass() {
        // The model face is the plane z = 0 facing down; a radius 0.3 rod ends exactly on it.
        // The cap's patch is 0.283 mm2 whichever way the mesh is read, but the mesh interval
        // for a coplanar cap is [0, 0.28]: far over the 0.010 mm2 the checker requires.
        let plate = kit::closed_box([-5.0, -5.0, 0.0], [5.0, 5.0, 2.0]);
        let rod = kit::part(0, 3, [0.0, 0.0, -3.0], [0.0, 0.0, 0.0], 0.3, 0.3);
        let s = scene(plate, vec![rod]);
        let p = patch::measure(&s);
        let o = h1(&s, &p);
        assert_eq!(o.status, Status::Unverified, "{o:?}");
        assert!(o.reason.contains("wide"), "{}", o.reason);
        // An ordinary tip ball on the same face is bracketed to well under 0.010 mm2.
        let plate = kit::closed_box([-5.0, -5.0, 0.0], [5.0, 5.0, 2.0]);
        let ball = kit::part(0, 7, [0.0, 0.0, -0.35], [0.0, 0.0, 0.35], 0.35, 0.35);
        let s = scene(plate, vec![ball]);
        let p = patch::measure(&s);
        let o = h1(&s, &p);
        assert_eq!(o.status, Status::Pass, "{o:?}");
    }
}
