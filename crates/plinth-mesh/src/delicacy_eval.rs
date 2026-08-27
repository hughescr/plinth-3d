//! The producer-side delicacy evaluator.
//!
//! [`delicacy.rs`](crate::delicacy) owns the immutable contract — the
//! `FaceTable` binding, the settings, the validated face values and the
//! digest.  This module is the batched geometry that fills it: one
//! `FaceTable`-scoped, source-order value per face of the mesh the table is
//! bound to.
//!
//! # Why this exists, and what it is deliberately not
//!
//! Owner ruling 2026-08-10 (`docs/engineering-journal/decisions/`) resolved the
//! standing missing-producer defect by building the evaluator here, inside
//! `plinth-mesh`, rather than by reaching into the auditor.  Two consequences
//! are structural and must survive every later edit:
//!
//! * **Delicacy is a property of the `FaceTable` at the LOD being scored**
//!   (`docs/engineering-journal/04-migration-plan.md` §"The defect this
//!   milestone fixes").  Every quantity below is computed
//!   from the mesh the table was bound to and from nothing else.  The Python
//!   defect — delicacy taken from the full mesh while areas came from a
//!   decimated proxy, so scar magnitudes moved with the decimation ratio —
//!   cannot recur here, because the field and its evaluation mesh are one
//!   object.
//! * **The independent auditor is gone, and nothing replaced it.**  Until
//!   2026-08-26 `plinth-metrics`'s `scar.rs` carried its own, deliberately
//!   different delicacy field, with no dependency in either direction, so the
//!   audit was never checking a thing against itself.  That crate was deleted
//!   by owner ruling as an orphan: it had no production consumer once the
//!   `plinth score` subcommand was removed.  Deleting it also deleted the
//!   second opinion, so this module is now the SOLE delicacy implementation
//!   and its output is checked only by its own tests.  That is a real loss of
//!   cross-checking, recorded here rather than quietly dropped: anything that
//!   restores an independent delicacy audit must still not be expressed in
//!   terms of this module, or it will be checking a thing against itself.
//!
//! # The blend
//!
//! The shape is the one `03-physics-and-constants.md` §3.8 records as a
//! Placeholder: a curvature proxy and a thinness proxy, blended at the
//! registered `DELICACY_CURVATURE_WEIGHT` / `DELICACY_THINNESS_WEIGHT`
//! (0.6 / 0.4), with the thinness proxy saturating at the registered
//! `THIN_FEATURE_MM` (1.5 mm).  Those three values are the only registered
//! numbers this evaluator reads, and they are Placeholder-classed guesses, not
//! measured truths — the field selects where support arms may land, it never
//! accepts a model.
//!
//! * **Curvature** is the normalized absolute angle defect of the welded
//!   vertex — `|2π − Σ incident corner angles| / 2π`, averaged over the face's
//!   three corners.  It is scale-free and dimensionless: a flat region scores
//!   0, a cube corner 0.25, a needle tip approaches 1.  No neighbourhood
//!   radius, no percentile, no sampling, and therefore no constant beyond the
//!   2π that the definition itself supplies.
//! * **Thinness** is `1 − thickness / THIN_FEATURE_MM`, clamped into `[0, 1]`,
//!   where `thickness` is the distance from the face centroid, along its own
//!   inward normal, to the first surface that faces back at it.  A wall
//!   thicker than the saturation length scores 0.
//!
//! Both proxies are exhaustive over faces: no sampling, so no seed, so nothing
//! for a thread count or a wall clock to move.
//!
//! # Batch-first
//!
//! Repo rule D-37.  The evaluation is a fixed sequence of whole-array passes:
//! one face-geometry pass, one topology build, one welded angle accumulation,
//! one geometry-kernel index build, **one** batched ray cast for every face at
//! once, and one blend pass.  There is no per-face spatial query.  Every fold
//! is serial in source index order, so the result does not depend on how the
//! kernel's index-parallel map-collect was scheduled.

use geom_kernel::{NativeSurface, NativeSurfaceConfig, V3};

use crate::delicacy::{
    DelicacyBuildConfig, DelicacyError, DelicacyFaceValue, DelicacyField, DelicacySettings,
    FaceTable,
};
use crate::{
    build_face_topology, Mesh, WeldPolicy, TOPOLOGY_CUT_BOUNDARY, TOPOLOGY_CUT_DEGENERATE,
    TOPOLOGY_CUT_DUPLICATE, TOPOLOGY_CUT_NONMANIFOLD, TOPOLOGY_CUT_POINT_ONLY,
    TOPOLOGY_CUT_SAME_DIRECTION,
};

/// Every cut that makes a welded vertex's angle defect meaningless.
///
/// The angle defect of a vertex is `2π − Σ incident corner angles`, and that
/// identity assumes the incident faces close one manifold disc around it.  At
/// a boundary, a non-manifold seam, a duplicated face or a degenerate corner
/// they do not, and the sum is an artifact of the defect rather than a
/// measurement of the shape.  Such vertices are scored 0 curvature — the
/// conservative direction, since delicacy only ever *removes* anchor sites.
const NON_MANIFOLD_CUTS: u8 = TOPOLOGY_CUT_BOUNDARY
    | TOPOLOGY_CUT_NONMANIFOLD
    | TOPOLOGY_CUT_DUPLICATE
    | TOPOLOGY_CUT_DEGENERATE
    | TOPOLOGY_CUT_SAME_DIRECTION
    | TOPOLOGY_CUT_POINT_ONLY;

/// Builds the producer-side delicacy field for `mesh` at the named LOD.
///
/// `lod` names the level of detail being scored and is bound into the returned
/// field's `FaceTable` alongside the mesh fingerprint, so a field can never be
/// read against a mesh or a LOD it was not measured on.
///
/// # Errors
///
/// [`DelicacyError::Topology`] when the mesh cannot be welded or its topology
/// cannot be built; [`DelicacyError::IndexBudgetExceeded`] when the shared
/// geometry kernel's conservative, allocation-free memory estimate for this
/// mesh exceeds `config.max_index_bytes` — measured before a single index
/// buffer is allocated, and never reduced to fit;
/// [`DelicacyError::Kernel`] when the kernel refuses the mesh or the batch;
/// [`DelicacyError::InvalidSetting`] when the blend weights sum to zero, which
/// would make the blend undefined rather than merely unweighted; and
/// [`DelicacyError::InvalidFaceValue`] if a computed component escapes `[0, 1]`
/// — a bug in this module, reported rather than clamped away.
pub fn evaluate_delicacy(
    mesh: &Mesh,
    lod: impl Into<String>,
    settings: DelicacySettings,
    config: DelicacyBuildConfig,
) -> Result<DelicacyField, DelicacyError> {
    config.validate()?;
    let denominator = settings.curvature_weight() + settings.thinness_weight();
    if !(denominator.is_finite() && denominator > 0.0) {
        return Err(DelicacyError::InvalidSetting {
            name: "curvature_weight + thinness_weight",
            value: denominator,
        });
    }

    let table = FaceTable::bind(mesh, lod)?;
    let face_count = table.face_count();
    if face_count == 0 {
        return DelicacyField::from_face_values(table, settings, Vec::new());
    }

    let geometry = FaceGeometryPass::run(mesh);
    let curvature = curvature_by_face(mesh, &geometry)?;
    let thinness = thinness_by_face(mesh, &geometry, settings.thin_feature_mm(), config)?;

    // One blend pass, serial in face order.  Both proxies are already in
    // `[0, 1]`, and the convex combination of two values in `[0, 1]` is in
    // `[0, 1]`; the clamp is floating-point hygiene against a last-bit
    // overshoot, not a policy that could hide an out-of-range proxy — a proxy
    // that genuinely escaped its range is refused by `DelicacyFaceValue::new`
    // before it reaches here.
    let mut values = Vec::with_capacity(face_count);
    for face in 0..face_count {
        let weight = (settings.curvature_weight() * curvature[face]
            + settings.thinness_weight() * thinness[face])
            / denominator;
        values.push(DelicacyFaceValue::new(
            curvature[face],
            thinness[face],
            weight.clamp(0.0, 1.0),
        )?);
    }
    DelicacyField::from_face_values(table, settings, values)
}

/// Per-face geometry derived in one source-order pass over the mesh.
///
/// `normals` are unit length except on a degenerate face, where the normal is
/// not defined at all; `degenerate` says which, so no later pass has to
/// rediscover it from a magnitude comparison.
struct FaceGeometryPass {
    centroids: Vec<V3>,
    normals: Vec<V3>,
    degenerate: Vec<bool>,
}

impl FaceGeometryPass {
    fn run(mesh: &Mesh) -> Self {
        let verts = mesh.verts();
        let tris = mesh.tris();
        let mut centroids = Vec::with_capacity(tris.len());
        let mut normals = Vec::with_capacity(tris.len());
        let mut degenerate = Vec::with_capacity(tris.len());
        for triangle in tris {
            let a = verts[triangle[0] as usize];
            let b = verts[triangle[1] as usize];
            let c = verts[triangle[2] as usize];
            centroids.push([
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ]);
            let e1 = sub(b, a);
            let e2 = sub(c, a);
            let cross = cross(e1, e2);
            let length = norm(cross);
            if length.is_finite() && length > 0.0 {
                normals.push([cross[0] / length, cross[1] / length, cross[2] / length]);
                degenerate.push(false);
            } else {
                // A zero-area face has no outward direction.  It is given the
                // build direction so the batched cast below stays well-defined
                // for every row, and its own result is discarded.
                normals.push([0.0, 0.0, 1.0]);
                degenerate.push(true);
            }
        }
        Self {
            centroids,
            normals,
            degenerate,
        }
    }
}

/// The curvature proxy: normalized absolute angle defect, welded, averaged
/// over each face's three corners.
///
/// The weld is [`WeldPolicy::ExactBits`] and is not the caller's choice. A
/// binary STL is a triangle soup with three vertices per facet, so under
/// `WeldPolicy::None` no two faces would share a vertex, every edge would look
/// like a boundary and the whole field would collapse to zero curvature — a
/// silent, plausible-looking wrong answer. `Epsilon` would go the other way and
/// introduce a welding tolerance this evaluator has no measurement for. Exact
/// welding is the only policy under which the angle-defect identity means what
/// it says, so it is stated here rather than taken as a parameter.
fn curvature_by_face(mesh: &Mesh, geometry: &FaceGeometryPass) -> Result<Vec<f64>, DelicacyError> {
    let topology =
        build_face_topology(mesh, WeldPolicy::ExactBits).map_err(DelicacyError::Topology)?;
    let welded_of_source = topology.source_vertex_to_welded();
    let welded_count = topology.welded_vertex_count() as usize;
    let cuts = topology.cut_flags();
    let verts = mesh.verts();
    let tris = mesh.tris();

    // One accumulation pass, serial in face order, then serial in corner
    // order.  `unusable` is written in the same pass so the exclusion decision
    // never depends on a second traversal seeing a partially built sum.
    let mut angle_sum = vec![0.0_f64; welded_count];
    let mut unusable = vec![false; welded_count];
    for (face, triangle) in tris.iter().enumerate() {
        let corners = [
            welded_of_source[triangle[0] as usize] as usize,
            welded_of_source[triangle[1] as usize] as usize,
            welded_of_source[triangle[2] as usize] as usize,
        ];
        if geometry.degenerate[face] || cuts[face] & NON_MANIFOLD_CUTS != 0 {
            for corner in corners {
                unusable[corner] = true;
            }
            continue;
        }
        let points = [
            verts[triangle[0] as usize],
            verts[triangle[1] as usize],
            verts[triangle[2] as usize],
        ];
        for corner in 0..3 {
            let at = points[corner];
            let first = sub(points[(corner + 1) % 3], at);
            let second = sub(points[(corner + 2) % 3], at);
            let scale = norm(first) * norm(second);
            if !(scale.is_finite() && scale > 0.0) {
                unusable[corners[corner]] = true;
                continue;
            }
            angle_sum[corners[corner]] += (dot(first, second) / scale).clamp(-1.0, 1.0).acos();
        }
    }

    // `|2π − Σ angles| / 2π`.  A closed convex vertex has a defect in
    // `[0, 2π)`, so the quotient lands in `[0, 1)` without a clamp; the `min`
    // covers a saddle whose incident angles exceed `4π`, which is a real if
    // rare shape rather than an arithmetic accident.
    let mut vertex_curvature = Vec::with_capacity(welded_count);
    for vertex in 0..welded_count {
        if unusable[vertex] {
            vertex_curvature.push(0.0);
            continue;
        }
        let defect = std::f64::consts::TAU - angle_sum[vertex];
        if !defect.is_finite() {
            vertex_curvature.push(0.0);
            continue;
        }
        vertex_curvature.push((defect.abs() / std::f64::consts::TAU).min(1.0));
    }

    let mut by_face = Vec::with_capacity(tris.len());
    for triangle in tris {
        let mean = (vertex_curvature[welded_of_source[triangle[0] as usize] as usize]
            + vertex_curvature[welded_of_source[triangle[1] as usize] as usize]
            + vertex_curvature[welded_of_source[triangle[2] as usize] as usize])
            / 3.0;
        by_face.push(mean.clamp(0.0, 1.0));
    }
    Ok(by_face)
}

/// The thinness proxy: one batched inward ray cast, saturating at
/// `thin_feature_mm`.
///
/// The ray from each face centroid runs along that face's own inward normal
/// and is cut at the saturation length, because a wall thicker than the
/// saturation length scores 0 whatever its exact thickness — the bound is the
/// physics, not a budget.  A hit counts only when the face it lands on faces
/// back (`n_hit · n_face < 0`), which is what makes it the *opposite wall*
/// rather than a neighbour across a convex crease, and which excludes the
/// originating face itself without needing a self-hit epsilon: a face's normal
/// dotted with itself is `+1`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn thinness_by_face(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
    thin_feature_mm: f64,
    config: DelicacyBuildConfig,
) -> Result<Vec<f64>, DelicacyError> {
    let verts = mesh.verts();
    let tris = mesh.tris();

    // Charge before allocating.  The kernel's estimate is conservative and
    // allocation-free; a mesh over the cap is refused naming both numbers, and
    // the evaluation is never quietly coarsened to fit under it.
    let estimate = NativeSurface::estimate_index_memory_from_indexed(verts, tris, thin_feature_mm)
        .map_err(|error| DelicacyError::Kernel(error.to_string()))?;
    if estimate.peak_bytes() > config.max_index_bytes {
        return Err(DelicacyError::IndexBudgetExceeded {
            charge: estimate.peak_bytes(),
            cap: config.max_index_bytes,
            what: "delicacy thinness query index",
        });
    }
    let surface = NativeSurface::from_indexed(
        verts,
        tris,
        NativeSurfaceConfig {
            // The pitch is the saturation length: the one length scale this
            // evaluator already has, registered, rather than a second number
            // to keep aligned with the first.  Only the kernel's parity grid
            // reads it, and the thinness answer comes from the BVH, so the
            // pitch cannot move a delicacy value.
            grid_pitch_mm: thin_feature_mm,
            max_grid_bytes: estimate.max_axis_grid_bytes(),
        },
    )
    .map_err(|error| DelicacyError::Kernel(error.to_string()))?;

    let mut directions = Vec::with_capacity(tris.len());
    for normal in &geometry.normals {
        directions.push([-normal[0], -normal[1], -normal[2]]);
    }
    // The single batched call.  One cast for every face, not one call per
    // face.
    let (offsets, distances, hit_faces) = surface
        .cast_rays(&geometry.centroids, &directions, thin_feature_mm)
        .map_err(|error| DelicacyError::Kernel(error.to_string()))?;
    let face_ids = surface.face_ids();

    let mut by_face = Vec::with_capacity(tris.len());
    for face in 0..tris.len() {
        if geometry.degenerate[face] {
            by_face.push(0.0);
            continue;
        }
        let start = offsets[face] as usize;
        let end = offsets[face + 1] as usize;
        let mut thickness = None;
        // Hits arrive sorted by distance, so the first that faces back is the
        // nearest one that does.
        for hit in start..end {
            let distance = distances[hit];
            if !distance.is_finite() || distance <= 0.0 {
                continue;
            }
            let internal = hit_faces[hit];
            if internal < 0 {
                continue;
            }
            let Some(&global) = face_ids.get(internal as usize) else {
                continue;
            };
            let global = global as usize;
            if global >= geometry.normals.len() || geometry.degenerate[global] {
                continue;
            }
            if dot(geometry.normals[global], geometry.normals[face]) < 0.0 {
                thickness = Some(distance);
                break;
            }
        }
        by_face.push(match thickness {
            Some(thickness) => (1.0 - thickness / thin_feature_mm).clamp(0.0, 1.0),
            None => 0.0,
        });
    }
    Ok(by_face)
}

#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[inline]
fn norm(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{make_box_default, make_thin_fin, make_thin_fin_with, Mesh};

    fn settings() -> DelicacySettings {
        DelicacySettings::production().expect("production delicacy settings")
    }

    fn config() -> DelicacyBuildConfig {
        DelicacyBuildConfig::production(1_000_000)
    }

    fn field(mesh: &Mesh, lod: &str) -> DelicacyField {
        evaluate_delicacy(mesh, lod, settings(), config()).expect("delicacy field")
    }

    #[test]
    fn every_face_is_scored_in_source_order_and_in_range() {
        let mesh = make_box_default().expect("box");
        let built = field(&mesh, "source-lod");
        assert_eq!(built.face_values().len(), mesh.tris().len());
        assert_eq!(built.validate_mesh(&mesh), Ok(()));
        for value in built.face_values() {
            assert!((0.0..=1.0).contains(&value.curvature()));
            assert!((0.0..=1.0).contains(&value.thinness()));
            assert!((0.0..=1.0).contains(&value.weight()));
        }
    }

    #[test]
    fn the_field_is_bit_reproducible() {
        // No sampling, no seed, no clock: two builds of the same mesh must
        // agree byte for byte, which is what makes the field usable as a
        // digest input downstream.
        let mesh = make_thin_fin().expect("thin fin");
        assert_eq!(
            field(&mesh, "source-lod").digest(),
            field(&mesh, "source-lod").digest()
        );
    }

    #[test]
    fn a_cube_corner_scores_the_analytic_angle_defect() {
        // Every vertex of a box is a right-angled corner: three incident
        // quarter-turns, so the defect is exactly `2π − 3·(π/2) = π/2` and the
        // normalized curvature is exactly 1/4.  Every face has three such
        // corners, so every face scores 1/4.
        let mesh = make_box_default().expect("box");
        let built = field(&mesh, "source-lod");
        for value in built.face_values() {
            assert!(
                (value.curvature() - 0.25).abs() < 1e-12,
                "cube corner curvature {} is not the analytic 0.25",
                value.curvature()
            );
        }
    }

    #[test]
    fn a_thick_box_is_not_thin_and_a_thin_fin_is() {
        // The saturation length is 1.5 mm.  A 20 mm box is nowhere near it
        // and must score exactly zero thinness; a fin thinner than the
        // saturation length must score above zero on the faces that face each
        // other across it.
        let thick = make_box_default().expect("box");
        let thick_field = field(&thick, "source-lod");
        assert!(thick_field
            .face_values()
            .iter()
            .all(|value| value.thinness() == 0.0));

        let fin = make_thin_fin().expect("thin fin");
        let fin_field = field(&fin, "source-lod");
        assert!(
            fin_field
                .face_values()
                .iter()
                .any(|value| value.thinness() > 0.0),
            "a thin fin scored no thinness anywhere"
        );
    }

    #[test]
    fn thinness_tracks_the_wall_it_measures() {
        // A thinner fin is more delicate than a thicker one at the same
        // saturation length.  This is the property the scar term rests on, so
        // it is asserted directly rather than inferred from a digest.
        let thinner =
            make_thin_fin_with([20.0, 20.0, 4.0], [0.4, 10.0, 15.0], 0.0).expect("0.4 mm fin");
        let thicker =
            make_thin_fin_with([20.0, 20.0, 4.0], [1.2, 10.0, 15.0], 0.0).expect("1.2 mm fin");
        let peak = |mesh: &Mesh| {
            field(mesh, "source-lod")
                .face_values()
                .iter()
                .map(|value| value.thinness())
                .fold(0.0_f64, f64::max)
        };
        assert!(
            peak(&thinner) > peak(&thicker),
            "0.4 mm fin peak thinness {} did not exceed the 1.2 mm fin's {}",
            peak(&thinner),
            peak(&thicker)
        );
    }

    #[test]
    fn the_blend_is_the_registered_convex_combination() {
        let mesh = make_thin_fin().expect("thin fin");
        let settings = settings();
        let built = field(&mesh, "source-lod");
        let denominator = settings.curvature_weight() + settings.thinness_weight();
        for value in built.face_values() {
            let expected = (settings.curvature_weight() * value.curvature()
                + settings.thinness_weight() * value.thinness())
                / denominator;
            assert!((value.weight() - expected).abs() < 1e-15);
        }
    }

    #[test]
    fn the_lod_is_bound_into_the_field() {
        // Two LOD names over the same mesh are two different fields: the
        // recorded ruling makes delicacy a property of the FaceTable at the
        // LOD being scored, and the digest has to be able to tell them apart.
        let mesh = make_box_default().expect("box");
        assert_ne!(field(&mesh, "exact").digest(), field(&mesh, "l1").digest());
        assert_eq!(field(&mesh, "exact").face_table().lod(), "exact");
    }

    #[test]
    fn an_index_budget_below_the_charge_refuses_naming_both_numbers() {
        let mesh = make_box_default().expect("box");
        let error = evaluate_delicacy(
            &mesh,
            "source-lod",
            settings(),
            DelicacyBuildConfig { max_index_bytes: 1 },
        )
        .expect_err("a one-byte index budget must refuse");
        match error {
            DelicacyError::IndexBudgetExceeded { charge, cap, what } => {
                assert!(charge > 1);
                assert_eq!(cap, 1);
                assert_eq!(what, "delicacy thinness query index");
            }
            other => panic!("expected a named budget refusal, got {other}"),
        }
    }
}
