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
//!   vertex — `|2π − Σ incident corner angles| / 2π` — read by every point of
//!   the surface within `THIN_FEATURE_MM` of that vertex, and per face as the
//!   area-weighted mean of what its points read.  The per-vertex value is
//!   scale-free and dimensionless: a flat region scores 0, a cube corner 0.25,
//!   a needle tip approaches 1.  **Since 2026-10-01 it is no longer the mean
//!   of the face's three corners**, which charged a corner's curvature over a
//!   whole face and so read the triangulation rather than the shape (the
//!   toed-slab pose; [`curvature_by_face`] has the record).  The spread radius
//!   is the registered thin-feature length already read below, not a new
//!   number.  No percentile, no sampling.
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
//! one curvature spread (a surface walk per curved vertex over one shared
//! scratch frontier), one geometry-kernel index build, **one** batched ray cast for every face at
//! once, and one blend pass.  There is no per-face spatial query.  Every fold
//! is serial in source index order, so the result does not depend on how the
//! kernel's index-parallel map-collect was scheduled.

use geom_kernel::perf::{self, Span, Work};
use geom_kernel::vec3::{add, cross, dot, norm, normalize, scale, sub};
use geom_kernel::{NativeSurface, NativeSurfaceConfig, V3};

use crate::delicacy::{
    DelicacyBuildConfig, DelicacyError, DelicacyFaceValue, DelicacyField, DelicacySettings,
    FaceTable,
};
use crate::{
    build_face_topology, Mesh, TopologyError, WeldPolicy, TOPOLOGY_CUT_BOUNDARY, TOPOLOGY_CUT_DEGENERATE,
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
    let (curvature, _counts) = curvature_by_face(mesh, &geometry, settings.thin_feature_mm())?;
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
/// **Shared with [`crate::display_eval`]** (2026-09-04). The display-face
/// evaluator needs the same three source-order arrays plus per-face area, and
/// deriving them twice from the same mesh would be two spellings of one
/// normal — the exact class of drift `geometry-canon.md` §1 exists to
/// prevent. `areas_mm2` is written here and read by the display evaluator
/// and, since 2026-10-01, by delicacy's curvature proxy, which turns each
/// face's ball-intersected charge into a per-area mean.
pub(crate) struct FaceGeometryPass {
    pub(crate) centroids: Vec<V3>,
    pub(crate) normals: Vec<V3>,
    pub(crate) degenerate: Vec<bool>,
    pub(crate) areas_mm2: Vec<f64>,
}

impl FaceGeometryPass {
    pub(crate) fn run(mesh: &Mesh) -> Self {
        let verts = mesh.verts();
        let tris = mesh.tris();
        let mut centroids = Vec::with_capacity(tris.len());
        let mut normals = Vec::with_capacity(tris.len());
        let mut degenerate = Vec::with_capacity(tris.len());
        let mut areas_mm2 = Vec::with_capacity(tris.len());
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
            // Canonical face normal: `geom_kernel::vec3` cross + `normalize`
            // (docs/specs/geometry-canon.md §1 OP-4 and OP-6, triage rule (d)).
            // `normalize`'s `None` is exactly this site's former
            // `length.is_finite() && length > 0.0` guard, so no input class
            // changes side; the SPELLING does move bits, by <= 1 ULP per
            // component, because the kernel multiplies by one reciprocal where
            // this file divided three times (kernel `normalize` doc, "Spelling,
            // and the ULP it costs").
            let raw = cross(e1, e2);
            areas_mm2.push(0.5 * norm(raw));
            if let Some(unit) = normalize(raw) {
                normals.push(unit);
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
            areas_mm2,
        }
    }
}

/// The welded angle defect `2π − Σ incident corner angles`, per welded vertex.
///
/// Extracted from [`curvature_by_face`] on 2026-09-04 so the display-face
/// evaluator can read the SIGN of the same measurement (a positive defect is
/// an elliptic, outward-bulging vertex — the tip of a protrusion; a groove is
/// developable and reads ~0) without a second, differently-spelled curvature
/// in this crate. The arithmetic, its order and its exclusion rules are
/// unchanged: `defect` holds the raw signed value for every welded vertex,
/// `unusable` marks the vertices whose incident faces do not close one
/// manifold disc, and both consumers apply their own reading on top.
pub(crate) struct WeldedAngleDefect {
    pub(crate) topology: crate::FaceTopology,
    pub(crate) defect: Vec<f64>,
    pub(crate) unusable: Vec<bool>,
}

pub(crate) fn welded_angle_defect(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
) -> Result<WeldedAngleDefect, TopologyError> {
    let topology = build_face_topology(mesh, WeldPolicy::ExactBits)?;
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

    let mut defect = Vec::with_capacity(welded_count);
    for sum in angle_sum {
        defect.push(std::f64::consts::TAU - sum);
    }
    Ok(WeldedAngleDefect {
        topology,
        defect,
        unusable,
    })
}

/// What one curvature spread did, counted, so a test can show the spread
/// reached the faces it claims to (path hits) rather than infer it from a
/// value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CurvatureSpreadCounts {
    /// Welded vertices with a nonzero curvature, each spread once.
    pub(crate) vertices_spread: usize,
    /// `(vertex, face)` pairs whose ball-face intersection was measured.
    pub(crate) pairs_measured: usize,
    /// Pairs that received a nonzero share of a vertex's curvature.
    pub(crate) pairs_charged: usize,
    /// Charged pairs whose face is NOT incident to the vertex: the faces a
    /// 1-ring (barycentric or Voronoi) distribution could never reach.
    pub(crate) pairs_charged_beyond_ring: usize,
}

/// The curvature proxy: each welded vertex's normalized absolute angle
/// defect, spread at that value over the surface within `radius_mm` of the
/// vertex, and read per face as its mean over the face's area.
///
/// # Why a fixed radius, and not the corners of the face (2026-10-01)
///
/// Until 2026-10-01 a face scored the plain mean of its three corners' values
/// over its WHOLE area. That is a barycentric distribution: the fold
/// `Σ area × curvature` over any face set equals `Σ_v K_v × A_v`, where `A_v`
/// is a third of the area of the vertex's incident faces in the set. `A_v` is
/// a property of the TRIANGULATION, not of the shape, so the same flat slab top
/// paid 135.000 raw mm² as two triangles whose corners are cube corners and
/// 81.059 as the 70 triangles of the unioned toed slab's underside, and that
/// decided the slab's pose (`plinth-evidence/slice-sweep-2026-09-02/
/// field-2026-09-28/lanes/toed-slab-fixture.md`). Voronoi areas have the same
/// defect: any 1-ring area shrinks as the mesh is refined.
///
/// The angle defect is a VERTEX quantity (discrete Gauss-Bonnet: it is the
/// integrated Gaussian curvature the vertex carries), so it is placed at the
/// vertex and spread over a neighbourhood whose size is a LENGTH, not a mesh
/// count: every point of the surface within `radius_mm` of the vertex reads
/// the vertex's value, and a face's curvature is the area-weighted mean of
/// what its points read. Then
///
/// * a flat face's interior farther than `radius_mm` from every curved vertex
///   contributes exactly zero, however it is triangulated;
/// * the fold over any set of faces is `Σ_v K_v × area(set ∩ ball(v,
///   radius_mm))`, which is a property of the surface alone. Re-triangulating
///   a region (subdividing a flat face, adding zero-defect vertices along a
///   crease) moves no term.
///
/// # The radius is the registered thin-feature length
///
/// `radius_mm` is `THIN_FEATURE_MM` (1.5 mm), the one length this evaluator
/// already reads, and the same physical scale: the thinness proxy calls a wall
/// delicate when it is thinner than this length, and the material within this
/// distance of a convex corner or a tip is exactly such a wall (the solid
/// within distance `d` of a cube corner is at most `d` thick). No new constant.
///
/// # The neighbourhood is surface-connected
///
/// A face is reached only through faces that themselves intersect the ball,
/// starting from the vertex's own faces, and its share is the EXACT area of
/// the face inside the ball (disc ∩ triangle in the face's plane). A purely
/// Euclidean ball would also bleed through any unconnected wall within the
/// radius; requiring a connected path inside the ball keeps the spread on
/// the vertex's own side of the surface except where the surface itself
/// turns back within the radius (a thin edge, which is delicate anyway).
///
/// # Saturation
///
/// A face's mean is capped at 1, the field's range. Two vertices whose balls
/// overlap add, which is the right reading of two curved vertices within the
/// thin-feature length of each other; the cap binds only where the summed
/// curvature within one ball exceeds a full turn of normalized defect.
///
/// The weld is [`WeldPolicy::ExactBits`] and is not the caller's choice. A
/// binary STL is a triangle soup with three vertices per facet, so under
/// `WeldPolicy::None` no two faces would share a vertex, every edge would look
/// like a boundary and the whole field would collapse to zero curvature — a
/// silent, plausible-looking wrong answer. `Epsilon` would go the other way and
/// introduce a welding tolerance this evaluator has no measurement for. Exact
/// welding is the only policy under which the angle-defect identity means what
/// it says, so it is stated here rather than taken as a parameter.
///
/// # Batch-first
///
/// One shared angle-defect accumulation, one vertex-to-face table, then one
/// walk per curved vertex over a shared scratch frontier and a stamp that is
/// never cleared (each vertex stamps with its own index). Serial in vertex
/// order, so every face's sum is accumulated in the same order on every run.
fn curvature_by_face(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
    radius_mm: f64,
) -> Result<(Vec<f64>, CurvatureSpreadCounts), DelicacyError> {
    let _perf_span = perf::span(Span::Curvature);
    // The accumulation itself lives in `welded_angle_defect`, shared with
    // the display-face evaluator. The per-vertex READING delicacy takes of it
    // — normalized ABSOLUTE defect — is unchanged; what changed on 2026-10-01
    // is how a vertex's value reaches the faces (see the doc above).
    let WeldedAngleDefect {
        topology,
        defect,
        unusable,
    } = welded_angle_defect(mesh, geometry).map_err(DelicacyError::Topology)?;
    let welded_of_source = topology.source_vertex_to_welded();
    let welded_count = defect.len();
    let tris = mesh.tris();
    let verts = mesh.verts();

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
        let defect = defect[vertex];
        if !defect.is_finite() {
            vertex_curvature.push(0.0);
            continue;
        }
        vertex_curvature.push((defect.abs() / std::f64::consts::TAU).min(1.0));
    }

    // Each welded vertex's position (exact weld: every source vertex of one
    // welded vertex carries the same bits, so any one of them is the position)
    // and its non-degenerate incident faces, as one CSR table.
    let mut position = vec![[0.0_f64; 3]; welded_count];
    let mut offsets = vec![0_usize; welded_count + 1];
    for (face, triangle) in tris.iter().enumerate() {
        for &corner in triangle {
            let welded = welded_of_source[corner as usize] as usize;
            position[welded] = verts[corner as usize];
            if !geometry.degenerate[face] {
                offsets[welded + 1] += 1;
            }
        }
    }
    for vertex in 0..welded_count {
        offsets[vertex + 1] += offsets[vertex];
    }
    let mut incident = vec![0_usize; offsets[welded_count]];
    let mut cursor = offsets.clone();
    for (face, triangle) in tris.iter().enumerate() {
        if geometry.degenerate[face] {
            continue;
        }
        for &corner in triangle {
            let welded = welded_of_source[corner as usize] as usize;
            incident[cursor[welded]] = face;
            cursor[welded] += 1;
        }
    }

    let radius_sq = radius_mm * radius_mm;
    let mut charge = vec![0.0_f64; tris.len()];
    let mut stamp = vec![usize::MAX; tris.len()];
    let mut frontier: Vec<usize> = Vec::new();
    let mut counts = CurvatureSpreadCounts::default();
    for vertex in 0..welded_count {
        let value = vertex_curvature[vertex];
        if value <= 0.0 {
            continue;
        }
        counts.vertices_spread += 1;
        let centre = position[vertex];
        frontier.clear();
        for &face in &incident[offsets[vertex]..offsets[vertex + 1]] {
            // A face incident at two corners (a sliver) is listed twice.
            if stamp[face] != vertex {
                stamp[face] = vertex;
                frontier.push(face);
            }
        }
        let ring = frontier.len();
        let mut head = 0_usize;
        while head < frontier.len() {
            let face = frontier[head];
            head += 1;
            counts.pairs_measured += 1;
            let inside = ball_face_area_mm2(mesh, geometry, face, centre, radius_sq);
            if inside <= 0.0 {
                continue;
            }
            counts.pairs_charged += 1;
            if head > ring {
                counts.pairs_charged_beyond_ring += 1;
            }
            charge[face] += value * inside;
            // Expand only from a face that is itself inside the ball, so the
            // neighbourhood stays connected through the surface.
            if let Some(neighbours) = topology.neighbors(face) {
                for &next in neighbours {
                    let next = next as usize;
                    if next >= tris.len() || geometry.degenerate[next] || stamp[next] == vertex {
                        continue;
                    }
                    stamp[next] = vertex;
                    frontier.push(next);
                }
            }
        }
    }

    let mut by_face = Vec::with_capacity(tris.len());
    for (face, &charged) in charge.iter().enumerate() {
        let area = geometry.areas_mm2[face];
        // `is_nan() || <= 0` is `!(area > 0.0)`, NaN included.
        if geometry.degenerate[face] || area.is_nan() || area <= 0.0 {
            by_face.push(0.0);
            continue;
        }
        by_face.push((charged / area).clamp(0.0, 1.0));
    }
    // Write-only perf charge, once per spread (never read back).
    if perf::enabled() {
        perf::add(Work::DelicacyVerticesSpread, perf::count(counts.vertices_spread));
        perf::add(Work::DelicacyPairsMeasured, perf::count(counts.pairs_measured));
        perf::add(Work::DelicacyPairsCharged, perf::count(counts.pairs_charged));
    }
    Ok((by_face, counts))
}

/// The exact area of `face` inside the ball of radius `sqrt(radius_sq)`
/// about `centre`: the disc the ball cuts from the face's plane, intersected
/// with the triangle.
///
/// The disc-triangle area is the standard signed decomposition: with the
/// disc's centre `O`, `area(disc ∩ ABC) = Σ_edges area(disc ∩ triangle(O, P,
/// Q))` with signs, and each edge's term is a straight-sided triangle where
/// the edge runs inside the disc and a circular sector where it runs outside.
/// Every operation is a `geom_kernel::vec3` call in 3D, with the face's own
/// unit normal giving the sign of each wedge, so no 2D frame is built.
fn ball_face_area_mm2(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
    face: usize,
    centre: V3,
    radius_sq: f64,
) -> f64 {
    let verts = mesh.verts();
    let triangle = mesh.tris()[face];
    let normal = geometry.normals[face];
    let corners = [
        verts[triangle[0] as usize],
        verts[triangle[1] as usize],
        verts[triangle[2] as usize],
    ];
    let height = dot(sub(centre, corners[0]), normal);
    let disc_sq = radius_sq - height * height;
    if disc_sq.is_nan() || disc_sq <= 0.0 {
        return 0.0;
    }
    let foot = sub(centre, scale(normal, height));
    let relative = [
        sub(corners[0], foot),
        sub(corners[1], foot),
        sub(corners[2], foot),
    ];
    let mut total = 0.0_f64;
    for edge in 0..3 {
        total += disc_wedge_area(relative[edge], relative[(edge + 1) % 3], disc_sq, normal);
    }
    total.clamp(0.0, geometry.areas_mm2[face])
}

/// Signed area of `disc(origin, sqrt(disc_sq)) ∩ triangle(origin, p, q)`,
/// positive when `p → q` turns counter-clockwise about `normal`.
fn disc_wedge_area(p: V3, q: V3, disc_sq: f64, normal: V3) -> f64 {
    let along = sub(q, p);
    let a = dot(along, along);
    if a.is_nan() || a <= 0.0 {
        return 0.0;
    }
    // |p + t·along|² = disc_sq  ⇔  a t² + 2 b t + c = 0, whose reduced
    // discriminant is b² − a·c (the half-b form; `b * b` is not a typo for
    // `a * b`, which `clippy::suspicious_operation_groupings` suspects).
    let b = dot(p, along);
    let c = dot(p, p) - disc_sq;
    let b_squared = b * b;
    let discriminant = b_squared - a * c;
    let mut cuts = [0.0_f64; 4];
    let mut count = 1_usize;
    if discriminant > 0.0 {
        let root = discriminant.sqrt();
        for t in [(-b - root) / a, (-b + root) / a] {
            if t > 0.0 && t < 1.0 {
                cuts[count] = t;
                count += 1;
            }
        }
    }
    cuts[count] = 1.0;
    count += 1;
    let mut area = 0.0_f64;
    for window in cuts[..count].windows(2) {
        let (from, to) = (window[0], window[1]);
        if to <= from {
            continue;
        }
        let start = add(p, scale(along, from));
        let end = add(p, scale(along, to));
        let middle = add(p, scale(along, 0.5 * (from + to)));
        let wedge = dot(cross(start, end), normal);
        area += if dot(middle, middle) <= disc_sq {
            0.5 * wedge
        } else {
            0.5 * disc_sq * wedge.atan2(dot(start, end))
        };
    }
    area
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
///
/// # Undecidable crossings saturate to the THIN extreme
///
/// The cast is [`NativeSurface::cast_rays_classified`], which reports per ray
/// how many faces it reached whose Möller determinant landed in
/// `0 < |det| <= DET_EPS` — the band where whether the ray crosses that face is
/// undecidable at f64 scale (`docs/specs/geometry-adjudication.md` §O4, §EXT-1
/// row 2).  Those faces contribute no crossing, so before this ruling a real
/// opposite wall could be dropped out of the list in silence and the face read
/// as arbitrarily THICK — up to full saturation, thinness `0`.
///
/// That is the dangerous direction.  Delicacy exists to protect fragile
/// geometry, and understating fragility is what lets an anchor land on a
/// needle.  So a ray with a nonzero tally saturates the OTHER way: thinness
/// `1.0`, the maximum delicacy contribution, whatever crossings the walk did
/// accept — an accepted crossing only bounds the thickness from above, and a
/// skipped one can only be nearer.  The event is counted by the kernel's own
/// `kernel.walk.collecting` telemetry atomic
/// ([`geom_kernel::telemetry::near_parallel_counts`]), so a nonzero corpus
/// reading is visible to the owner rather than silently load-bearing.
///
/// Uncertainty is tolerated here — rather than refused — only because every
/// direction it can move the outcome is the PROTECTIVE one: more delicacy, more
/// faces withheld from anchoring.  It never moves a face toward acceptance.
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
    let cast = surface
        .cast_rays_classified(&geometry.centroids, &directions, thin_feature_mm)
        .map_err(|error| DelicacyError::Kernel(error.to_string()))?;
    let offsets = &cast.offsets;
    let distances = &cast.t_all;
    let hit_faces = &cast.face_all;
    let face_ids = surface.face_ids();

    let mut by_face = Vec::with_capacity(tris.len());
    for face in 0..tris.len() {
        if geometry.degenerate[face] {
            by_face.push(0.0);
            continue;
        }
        // The undecidable band, saturated to the thin extreme.  Read BEFORE the
        // accepted crossings, because an accepted crossing does not resolve the
        // uncertainty: it bounds the thickness from above, and the crossing the
        // arithmetic could not decide may be nearer still.
        if cast.near_parallel[face] > 0 {
            by_face.push(1.0);
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

    /// Two triangles, arranged so that face 0's inward thinness ray reaches a
    /// face whose Möller determinant lands INSIDE the undecidable band and
    /// face 1's reaches nothing undecidable at all.
    ///
    /// Every number here is chosen so the determinant is exact in f64 rather
    /// than approximately small, which is what makes the test a pin instead of
    /// a hope.
    ///
    /// * **Face 0** is `(-1,0,5), (1,0,5), (0,2,5)`: flat in `z = 5`, so
    ///   `E1 x E2 = (0,0,4)`, `|n| = 4` and `normalize` divides by an exact
    ///   power of two — the unit normal is exactly `(0,0,1)` and the inward ray
    ///   direction exactly `(-0,-0,-1)`. Its centroid x is `(-1+1+0)/3 = 0`
    ///   exactly.
    /// * **Face 1** is `(0,0,4), (0,2,4), (2e-15,1,4.5)`, so `E1 = (0,2,0)` and
    ///   `E2 = (2e-15,1,0.5)`. For a straight-down ray the Möller determinant
    ///   reduces to `E1.x*E2.y - E1.y*E2.x = 0*1 - 2*2e-15 = -4e-15`: nonzero,
    ///   and inside `DET_EPS = 1e-14`. Geometrically it is a wall standing
    ///   almost exactly parallel to the ray, tilted by 2e-15 mm over 2 mm.
    ///
    /// Face 0's ray therefore reaches exactly one undecidable face, 0.5 mm
    /// below it — well inside the 1.5 mm saturation length. Face 1's own ray
    /// runs along `-x` and reaches no undecidable face (its determinant against
    /// face 0 is -1.6e-14, OUTSIDE the band, which is the near miss that makes
    /// the contrast between the two rows meaningful).
    fn planted_near_parallel_pair() -> Mesh {
        Mesh::new(
            vec![
                [-1.0, 0.0, 5.0],
                [1.0, 0.0, 5.0],
                [0.0, 2.0, 5.0],
                [0.0, 0.0, 4.0],
                [0.0, 2.0, 4.0],
                [2e-15, 1.0, 4.5],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .expect("planted near-parallel pair")
    }

    /// **A ray that reached an undecidable face scores the face as THIN.**
    ///
    /// The input class: a face whose inward, saturation-length thinness ray
    /// reaches at least one triangle whose Möller determinant lands in
    /// `0 < |det| <= DET_EPS`. Whether that triangle is crossed is undecidable
    /// at f64 scale, so the crossing list the thickness was read from is
    /// incomplete in an unknown direction and the opposite wall may be nearer
    /// than anything on it — possibly touching. Delicacy exists to protect
    /// fragile geometry, so the row saturates to the thin extreme.
    ///
    /// Face 1 in the same mesh, same batch, is the control: its ray reaches
    /// nothing undecidable and finds no wall facing back, so it scores 0 and
    /// shows the saturation is a per-ROW answer rather than a batch-wide one.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "exact pins over hand-written fixture values whose determinant is exact in f64"
    )]
    fn an_undecidable_crossing_saturates_that_face_to_the_thin_extreme() {
        let mesh = planted_near_parallel_pair();
        let before = geom_kernel::telemetry::near_parallel_counts();
        let built = field(&mesh, "planted-lod");
        let after = geom_kernel::telemetry::near_parallel_counts();

        let values = built.face_values();
        assert_eq!(values.len(), 2);
        assert_eq!(
            values[0].thinness(),
            1.0,
            "the undecidable row must saturate to the thin extreme, not report a thickness \
             nobody measured"
        );
        assert_eq!(
            values[1].thinness(),
            0.0,
            "the control row's ray reached no undecidable face; saturation must not spill \
             across the batch"
        );
        // Both faces are all-boundary, so curvature is 0 and the blend is the
        // thinness weight alone: the saturation really does reach the field
        // value orientation scoring reads.
        assert_eq!(values[0].curvature(), 0.0);
        assert_eq!(values[0].weight(), settings().thinness_weight());

        // The path was exercised, measured rather than assumed: the collecting
        // walk's process-global counter moved. Global counters only ever grow
        // and other tests in this binary share them, so the pin is a delta
        // minimum, while the exact per-row figure above is the strong claim.
        assert!(
            after.collecting_walk - before.collecting_walk >= 1,
            "the kernel's collecting-walk near-parallel counter did not move: {before:?} -> \
             {after:?}; this fixture did not reach the classified path"
        );
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
        // normalized curvature is exactly 1/4.
        //
        // RE-PINNED 2026-10-01 (scar-fold-invariance lane). This test used to
        // pin every face at 1/4, because a face scored the mean of its three
        // corners over its whole area. The value is now spread over the
        // surface within the thin-feature length `r` of each corner, so a
        // face reads `1/4 × (its area within r of its corners) / its area`.
        // On the 10 mm box every corner's ball lies inside its own triangles
        // (the nearest opposite edge is 7.07 mm away), so a triangle holds
        // one circular sector of radius `r` per corner, and its three corner
        // angles sum to π: `1/4 × (π r² / 2) / area`, exactly.
        let mesh = make_box_default().expect("box");
        let built = field(&mesh, "source-lod");
        let radius = settings().thin_feature_mm();
        let geometry = FaceGeometryPass::run(&mesh);
        for (face, value) in built.face_values().iter().enumerate() {
            let expected =
                0.25 * 0.5 * std::f64::consts::PI * radius * radius / geometry.areas_mm2[face];
            assert!(
                (value.curvature() - expected).abs() < 1e-12,
                "face {face}: cube corner curvature {} is not the analytic {expected}",
                value.curvature()
            );
        }
    }

    /// A closed `[0, 8]³` box whose every face is an `n x n` grid of 2-triangle
    /// cells, with integer-exact coordinates so `n = 1` and `n = 8` describe
    /// the same surface bit for bit and weld exactly.
    fn gridded_cube(n: u32) -> Mesh {
        let coordinate = |i: u32| 8.0 * f64::from(i) / f64::from(n);
        let mut index = std::collections::HashMap::new();
        let mut verts = Vec::new();
        let mut tris = Vec::new();
        // (fixed axis, at max end?, u, v) with u x v the outward normal.
        for (fixed, at_max, u, v) in [
            (0, true, 1, 2),
            (0, false, 2, 1),
            (1, true, 2, 0),
            (1, false, 0, 2),
            (2, true, 0, 1),
            (2, false, 1, 0),
        ] {
            for a in 0..n {
                for b in 0..n {
                    let mut corner = |da: u32, db: u32| -> u32 {
                        let mut grid = [0_u32; 3];
                        grid[fixed] = if at_max { n } else { 0 };
                        grid[u] = a + da;
                        grid[v] = b + db;
                        *index.entry(grid).or_insert_with(|| {
                            verts.push(grid.map(coordinate));
                            u32::try_from(verts.len() - 1).expect("small mesh")
                        })
                    };
                    let (p00, p10, p11, p01) =
                        (corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1));
                    tris.push([p00, p10, p11]);
                    tris.push([p00, p11, p01]);
                }
            }
        }
        Mesh::new(verts, tris).expect("gridded cube")
    }

    /// **The curvature fold is a property of the surface, not of its
    /// triangles** (2026-10-01).
    ///
    /// The same cube as 12 triangles and as 768: the fold `Σ area ×
    /// curvature` over each of its six sides must equal the analytic
    /// `4 corners × 1/4 × π r² / 4` both times. The path-hit counts show the
    /// fine mesh's spread really did leave the corners' own triangles
    /// (which a 1-ring, barycentric or Voronoi, distribution never can), and
    /// that the coarse mesh's never had to.
    #[test]
    fn the_curvature_fold_does_not_depend_on_the_triangulation() {
        let radius = settings().thin_feature_mm();
        let per_side = 4.0 * 0.25 * std::f64::consts::PI * radius * radius / 4.0;
        for (n, expect_beyond_ring) in [(1_u32, false), (8, true)] {
            let mesh = gridded_cube(n);
            let geometry = FaceGeometryPass::run(&mesh);
            let (curvature, counts) =
                curvature_by_face(&mesh, &geometry, radius).expect("curvature");
            // Only the 8 cube corners are curved; grid vertices on a face or
            // along an edge have zero defect up to the last bit of `acos`.
            assert!(
                counts.vertices_spread >= 8,
                "n = {n}: only {} curved vertices spread",
                counts.vertices_spread
            );
            assert_eq!(
                counts.pairs_charged_beyond_ring > 0,
                expect_beyond_ring,
                "n = {n}: {counts:?}"
            );
            let mut sides = [0.0_f64; 6];
            for (face, value) in curvature.iter().enumerate() {
                let normal = geometry.normals[face];
                let axis = (0..3)
                    .max_by(|&a, &b| normal[a].abs().total_cmp(&normal[b].abs()))
                    .expect("three axes");
                let side = axis * 2 + usize::from(normal[axis] > 0.0);
                sides[side] += geometry.areas_mm2[face] * value;
            }
            for (side, fold) in sides.iter().enumerate() {
                assert!(
                    (fold - per_side).abs() < 1e-9 * per_side,
                    "n = {n}, side {side}: fold {fold} is not the analytic {per_side}"
                );
            }
        }
    }

    /// The disc-triangle area against closed forms: a triangle wholly inside
    /// the disc, a disc wholly inside the triangle, and a quarter disc in a
    /// right-angled corner.
    #[test]
    fn the_ball_face_area_matches_closed_forms() {
        let mesh = Mesh::new(
            vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("one triangle");
        let geometry = FaceGeometryPass::run(&mesh);
        let pi = std::f64::consts::PI;
        // Quarter disc at the right-angled corner.
        let quarter = ball_face_area_mm2(&mesh, &geometry, 0, [0.0, 0.0, 0.0], 1.5 * 1.5);
        assert!((quarter - pi * 2.25 / 4.0).abs() < 1e-12, "{quarter}");
        // A whole disc inside the triangle, the ball's centre 0.5 mm off the
        // plane: the disc radius is sqrt(1 - 0.25).
        let whole = ball_face_area_mm2(&mesh, &geometry, 0, [2.0, 2.0, 0.5], 1.0);
        assert!((whole - pi * 0.75).abs() < 1e-12, "{whole}");
        // The whole triangle inside the ball.
        let all = ball_face_area_mm2(&mesh, &geometry, 0, [3.0, 3.0, 0.0], 400.0);
        assert!((all - 50.0).abs() < 1e-9, "{all}");
        // A ball that misses the plane: exactly +0.0, compared bit for bit.
        assert_eq!(
            ball_face_area_mm2(&mesh, &geometry, 0, [3.0, 3.0, 2.0], 1.0).to_bits(),
            0.0_f64.to_bits()
        );
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
