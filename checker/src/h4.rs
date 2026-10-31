//! H4: the model is lifted on supports: its lowest point is at least 5.0 mm above the plate,
//! and no model surface rests on the plate, a raft or the base of a support.
//!
//! The file's only height field is the model's translate Z (format reference section 6.2),
//! read as "height of the model's lowest point above the build plate": the world height of a
//! vertex is `z - lowest z + Z`, so the lowest point is exactly Z. Both numbers are the
//! written f32 values, read as exact f64, so the lift needs no allowance. The meaning of Z is
//! "inferred" in the format reference; when supports exist the checker confirms it from
//! the other side: the lowest support point must sit at the plate level that Z implies
//! (design 7.2). If it does not, the plate level is unknown and H4 is unverified.
//!
//! The clearance between the model and the plate parts (flares, mat pads, raft pads and
//! bridges) is measured with the H6 distance search: it must be certified above zero.

use crate::h6;
use crate::json::Json;
use crate::outcome::{MAX_LOCATED, Outcome, at, worst};
use crate::report::Status;
use crate::scene::Scene;
use crate::status::Located;
use crate::v3::V;

/// How closely the lowest support point must match the plate level implied by Z, mm.
pub const PLATE_AGREE_MM: f64 = 1e-4;

fn lowest_model_vertex(scene: &Scene) -> Option<V> {
    scene
        .model
        .iter()
        .flatten()
        .copied()
        .min_by(|a, b| a[2].total_cmp(&b[2]))
}

/// The lowest z over all support meshes, local frame.
fn lowest_support_z(scene: &Scene) -> Option<f64> {
    scene
        .parts
        .iter()
        .flat_map(|p| p.tris.iter().flatten())
        .map(|v| v[2])
        .min_by(f64::total_cmp)
}

/// Lowest-point check: `(status, note)`.
fn lift(scene: &Scene) -> (Status, String, Vec<Located>) {
    let limit = scene.limits.h4_lift_mm;
    let z = scene.translate[2];
    let Some(low) = lowest_model_vertex(scene) else {
        return (
            Status::Unverified,
            "the model has no triangles".to_owned(),
            Vec::new(),
        );
    };
    let world_low = low[2] - scene.model_min_z + z;
    if world_low < limit {
        let loc = at(
            None,
            Some(scene.world(low)),
            None,
            format!(
                "the lowest model point is {world_low} mm above the plate, under the {limit} mm limit"
            ),
        );
        return (
            Status::Violation,
            format!("lowest model point {world_low} mm < {limit} mm"),
            vec![loc],
        );
    }
    (
        Status::Pass,
        format!("lowest model point {world_low} mm above the plate (limit {limit} mm)"),
        Vec::new(),
    )
}

/// Plate-level cross-check against the lowest support point: `Some(why)` when it fails.
pub fn plate_disagreement(scene: &Scene) -> Option<String> {
    let low = lowest_support_z(scene)?;
    let plate = scene.plate_z();
    ((low - plate).abs() > PLATE_AGREE_MM).then(|| {
        format!(
            "the lowest support point is at local z {low}, but translate Z implies the plate at local z {plate}: the plate level is not established"
        )
    })
}

/// H4.
#[must_use]
pub fn measure(scene: &Scene) -> Outcome {
    let (lift_status, lift_note, mut located) = lift(scene);
    let mut statuses = vec![lift_status];
    let mut notes = vec![lift_note];
    if let Some(why) = plate_disagreement(scene) {
        statuses.push(Status::Unverified);
        notes.push(why);
    } else if scene.parts.is_empty() {
        notes.push(
            "no support records, so the plate level rests on translate Z alone (not cross-checked)"
                .to_owned(),
        );
    }
    let mut min_clear: Option<f64> = None;
    for (pi, p) in scene
        .parts
        .iter()
        .enumerate()
        .filter(|(_, p)| h6::is_plate_part(p))
    {
        if let Some(why) = &p.invalid {
            statuses.push(Status::Unverified);
            notes.push(format!("record {} ({}): {why}", p.rec, p.role.name()));
            continue;
        }
        let g = h6::part_gap(scene, pi, 0.5, 1e-5);
        min_clear = Some(min_clear.map_or(g.lb, |m: f64| m.min(g.lb)));
        if g.ub < 1e-5 {
            statuses.push(Status::Violation);
            notes.push(format!(
                "record {} ({}): the model touches or overlaps it",
                p.rec,
                p.role.name()
            ));
            located.push(at(
                Some(p.rec),
                Some(scene.world(p.bbox.centre())),
                None,
                format!("the model touches or overlaps this {}", p.role.name()),
            ));
        } else if g.lb <= 0.0 {
            statuses.push(Status::Unverified);
            notes.push(format!(
                "record {} ({}): the clearance to the model is not certified above zero",
                p.rec,
                p.role.name()
            ));
        } else {
            statuses.push(Status::Pass);
        }
    }
    located.truncate(MAX_LOCATED);
    let detail = Json::obj([
        ("limit_mm", Json::Num(scene.limits.h4_lift_mm)),
        ("translate_z_mm", Json::Num(scene.translate[2])),
        (
            "smallest_plate_part_clearance_lower_mm",
            min_clear.map_or(Json::Null, Json::Num),
        ),
    ]);
    let status = worst(&statuses);
    let reason = notes.join("; ");
    match status {
        Status::Pass => Outcome::pass(reason),
        Status::Violation => Outcome::violation(reason).with_located(located),
        Status::Unverified => Outcome::unverified(reason).with_located(located),
        Status::NotApplicable => Outcome::not_applicable(reason),
    }
    .with_detail(detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{self as kit, test_limits as limits};
    use crate::tri::Tri;

    /// A 10 mm cube, outward winding, lowest face at z = -5 (local).
    fn cube() -> Vec<Tri> {
        let h = 5.0;
        let p = |i: u8| {
            [
                if i & 1 == 0 { -h } else { h },
                if i & 2 == 0 { -h } else { h },
                if i & 4 == 0 { -h } else { h },
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
            out.push([p(q[0]), p(q[1]), p(q[2])]);
            out.push([p(q[0]), p(q[2]), p(q[3])]);
        }
        out
    }

    fn run(z: f64, parts: Vec<crate::parts::Part>) -> Outcome {
        measure(&Scene::from_parts(&limits(), cube(), [0.0, 0.0, z], parts))
    }

    #[test]
    fn the_lift_is_exact_at_five_and_a_violation_below() {
        assert_eq!(run(5.0, Vec::new()).status, Status::Pass);
        assert_eq!(run(4.95, Vec::new()).status, Status::Violation);
        let o = run(4.999_999, Vec::new());
        assert_eq!(o.status, Status::Violation);
        assert!(o.located[0].location.xyz.is_some());
    }

    #[test]
    fn a_raft_under_the_model_is_a_violation_and_a_distant_one_passes() {
        // Plate at local z = -5 - 5 = -10. Raft pad 0.2 thick on the plate: far from the cube.
        let pad = kit::raft_pad(0, [0.0, 0.0], -9.8);
        assert_eq!(run(5.0, vec![pad]).status, Status::Pass);
        // The same pad lifted to touch the cube bottom (z = -5).
        let touching = kit::raft_pad(0, [0.0, 0.0], -4.9);
        let o = run(5.0, vec![touching]);
        // Its bottom is then at -5.1, so the plate-level cross-check also fails; the touch
        // is still reported as the violation.
        assert_eq!(o.status, Status::Violation, "{o:?}");
    }

    #[test]
    fn a_plate_level_that_disagrees_with_the_supports_is_unverified() {
        // The pad bottom at -5.8 does not match the plate level -10.
        let pad = kit::raft_pad(0, [0.0, 0.0], -5.6);
        let o = run(5.0, vec![pad]);
        assert_eq!(o.status, Status::Unverified, "{o:?}");
    }
}
