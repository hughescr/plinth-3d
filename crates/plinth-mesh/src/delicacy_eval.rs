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

use std::ops::Range;

use geom_kernel::perf::{self, Span, Work};
use geom_kernel::vec3::{add, cross, dot, norm, normalize, scale, sub};
use geom_kernel::{NativeSurface, NativeSurfaceConfig, V3};

use crate::delicacy::{
    DelicacyBuildConfig, DelicacyError, DelicacyFaceValue, DelicacyField, DelicacySettings,
    FaceTable,
};
use crate::surface_par::{spread_range, Exact, ScratchPool, SpreadHooks, SpreadPlan, WaveStats};
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
    evaluate_delicacy_with(mesh, lod, settings, config, DelicacyEvaluation::Exact)
}

/// How the curvature spread of [`evaluate_delicacy_with`] is evaluated.
///
/// The thinness proxy is the same under both; only the curvature spread
/// differs, and only on a mesh large enough for the difference to pay
/// (see [`protected_proxy`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DelicacyEvaluation {
    /// Every curved welded vertex is spread over its own ball. The reference
    /// the registered tests and the recorded outputs were made with.
    Exact,
    /// Protected proxy (the production default, 2026-10-15): vertices that
    /// carry real curvature are spread exactly; the many low-curvature vertices
    /// of a dense mesh are merged into per-patch clusters first, so the walk
    /// count falls by about the cluster size. Output-changing by design.
    #[default]
    ProtectedProxy,
}

/// [`evaluate_delicacy`], with the curvature evaluation named.
///
/// # Errors
///
/// As [`evaluate_delicacy`].
pub fn evaluate_delicacy_with(
    mesh: &Mesh,
    lod: impl Into<String>,
    settings: DelicacySettings,
    config: DelicacyBuildConfig,
    evaluation: DelicacyEvaluation,
) -> Result<DelicacyField, DelicacyError> {
    config.validate()?;
    let denominator = settings.curvature_weight() + settings.thinness_weight();
    if !(denominator.is_finite() && denominator > 0.0) {
        return Err(DelicacyError::InvalidSetting {
            name: "curvature_weight + thinness_weight",
            value: denominator,
        });
    }

    // Write-only stage marks (X0, 2026-10-10): where the face table's time
    // goes beside the `delicacy.curvature` span. Inert unless `--perf` is on.
    let bind_mark = perf::stage("orient.delicacy.bind");
    let table = FaceTable::bind(mesh, lod)?;
    drop(bind_mark);
    let face_count = table.face_count();
    if face_count == 0 {
        return DelicacyField::from_face_values(table, settings, Vec::new());
    }

    let geometry_mark = perf::stage("orient.delicacy.geometry");
    let geometry = FaceGeometryPass::run(mesh);
    drop(geometry_mark);
    let (curvature, _counts) =
        curvature_by_face(mesh, &geometry, settings.thin_feature_mm(), evaluation)?;
    let thinness_mark = perf::stage("orient.delicacy.thinness");
    let thinness = thinness_by_face(mesh, &geometry, settings.thin_feature_mm(), config)?;
    drop(thinness_mark);

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
/// walk per curved vertex over a per-worker scratch frontier and a stamp that
/// is never cleared (each vertex stamps with its own index, and each vertex is
/// walked exactly once in the run).
///
/// # Parallel, and bit-identical to the serial walk (2026-10-09)
///
/// The walks run as fixed vertex blocks in waves (`surface_par`); each block
/// returns its `(face, value * inside)` pairs in walk order, and a serial
/// replay applies the blocks in ascending vertex order. Every face therefore
/// receives the same addends in the same order as the serial walk, at any
/// thread count (see `surface_par`'s module doc). The serial body is kept as
/// `curvature_by_face_serial` and every test compares against it bit for bit.
fn curvature_by_face(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
    radius_mm: f64,
    evaluation: DelicacyEvaluation,
) -> Result<(Vec<f64>, CurvatureSpreadCounts), DelicacyError> {
    let _perf_span = perf::span(Span::Curvature);
    let (by_face, spread) = match evaluation {
        DelicacyEvaluation::Exact => curvature_parallel(mesh, geometry, radius_mm)?,
        DelicacyEvaluation::ProtectedProxy => curvature_proxy_parallel(mesh, geometry, radius_mm)?,
    };
    // Write-only perf charge, once per spread (never read back).
    if perf::enabled() {
        let counts = spread.counts;
        perf::add(Work::DelicacyVerticesSpread, perf::count(counts.vertices_spread));
        perf::add(Work::DelicacyPairsMeasured, perf::count(counts.pairs_measured));
        perf::add(Work::DelicacyPairsCharged, perf::count(counts.pairs_charged));
        perf::add(Work::DelicacyBlocks, perf::count(spread.waves.blocks));
        perf::add(Work::DelicacyWaves, perf::count(spread.waves.waves));
        perf::add(
            Work::DelicacyOverflowSplits,
            perf::count(spread.waves.overflow_splits),
        );
        perf::add(
            Work::DelicacyFacesMultiplyCharged,
            perf::count(spread.faces_multiply_charged),
        );
    }
    Ok((by_face, spread.counts))
}

/// The production spread: serial setup, then the walks in the kernel pool.
pub(crate) fn curvature_parallel(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
    radius_mm: f64,
) -> Result<(Vec<f64>, SpreadOutcome), DelicacyError> {
    let setup = curvature_setup(mesh, geometry)?;
    let inputs = setup.inputs(mesh, geometry, radius_mm * radius_mm);
    let spread = geom_kernel::run_in_pool(|| {
        spread_all::<Exact>(&inputs, &SpreadPlan::PRODUCTION)
    })
    .map_err(|error| DelicacyError::Kernel(error.to_string()))?;
    let by_face = charge_to_by_face(geometry, &spread.charge);
    Ok((by_face, spread))
}

/// Cluster cell edge, in cells per spread radius. A cluster holds the
/// low-curvature vertices of one cell that face the same way, so it spans at
/// most `radius / CLUSTER_CELLS_PER_RADIUS` per axis.
const CLUSTER_CELLS_PER_RADIUS: f64 = 4.0;

/// Normalized angle defect at and above which a vertex is PROTECTED: it is
/// never merged, so every tip, corner and sharp point of a delicate feature
/// is spread from its own position exactly as in the exact evaluation.
/// (0.1 is 0.63 rad of angle defect at one vertex: a cube corner reads 0.25.)
const PROTECTED_DEFECT: f64 = 0.1;

/// Fewer curved vertices than this and the exact walk is already cheap; the
/// proxy does not engage, so small fixtures and minis stay exact.
const PROXY_MIN_CURVED_VERTICES: usize = 20_000;

/// The reduced walk list of the protected proxy: one item per cluster or
/// protected vertex, in ascending representative-vertex order.
struct ProxyItems {
    value: Vec<f64>,
    position: Vec<V3>,
    offsets: Vec<usize>,
    incident: Vec<usize>,
}

/// Which way a vertex faces, as one of six axis classes of the area-weighted
/// sum of its incident face normals. Two vertices of one cell merge only when
/// they face alike, so the two sides of a thin sheet, fin or blade (opposite
/// normals, a fraction of a cell apart) are never merged into one walk that
/// would bleed across the wall.
fn facing_class(setup: &CurvatureSetup, geometry: &FaceGeometryPass, vertex: usize) -> u8 {
    let mut sum = [0.0_f64; 3];
    for &face in &setup.incident[setup.offsets[vertex]..setup.offsets[vertex + 1]] {
        let weighted = scale(geometry.normals[face], geometry.areas_mm2[face]);
        sum = add(sum, weighted);
    }
    let mut axis = 0_usize;
    for candidate in 1..3 {
        if sum[candidate].abs() > sum[axis].abs() {
            axis = candidate;
        }
    }
    // `axis < 3`, so the class is `< 6`.
    u8::try_from(axis * 2 + usize::from(sum[axis] < 0.0)).unwrap_or(0)
}

/// Builds the protected-proxy walk list, or `None` when the mesh has too few
/// curved vertices for the proxy to pay (the caller then walks exactly).
///
/// Every protected vertex (defect at or above [`PROTECTED_DEFECT`]) is one item
/// at its own position with its own value. The rest are merged per
/// `(cell, facing)` group, and within a group per LOCALLY CONNECTED piece: two
/// members join a piece only when a chain of mesh edges whose both endpoints
/// are members of the same group links them. (A cell and a facing class alone
/// would merge two nearby fins or antlers that are disconnected, or joined only
/// by a path outside the spread ball; the walk of the piece's representative
/// could then never reach the other fin and that fin would lose its curvature.
/// A global component label would not be enough either: two members of one
/// component can be linked only through a remote path.) A piece's value is the
/// SUM of its members' values (the curvature mass is conserved), and it is
/// spread once from the member nearest the value-weighted centroid, so the
/// centre lies on the surface and, the piece being edge-connected inside one
/// cell, within one spread radius of every member.
/// Everything is serial and in ascending vertex order, so the list does not
/// depend on a thread count.
///
/// `min_curved` is the engagement threshold ([`PROXY_MIN_CURVED_VERTICES`] in
/// production; a test lowers it to exercise the merge on a small fixture).
fn protected_proxy(
    mesh: &Mesh,
    setup: &CurvatureSetup,
    geometry: &FaceGeometryPass,
    radius_mm: f64,
    min_curved: usize,
) -> Option<ProxyItems> {
    let curved = setup.vertex_curvature.iter().filter(|&&value| value > 0.0).count();
    if curved < min_curved {
        return None;
    }
    let welded_of_source = setup.topology.source_vertex_to_welded();
    let tris = mesh.tris();
    let cell = radius_mm / CLUSTER_CELLS_PER_RADIUS;
    let mut protected: Vec<(u32, f64)> = Vec::new();
    let mut keyed: Vec<((i64, i64, i64, u8), u32)> = Vec::new();
    for (vertex, &value) in setup.vertex_curvature.iter().enumerate() {
        if value.is_nan() || value <= 0.0 {
            continue;
        }
        // `welded_count < u32::MAX`, checked in `curvature_setup`.
        let id = u32::try_from(vertex).unwrap_or(u32::MAX);
        if value >= PROTECTED_DEFECT {
            protected.push((id, value));
            continue;
        }
        let at = setup.position[vertex];
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a cell index of a coordinate in mm divided by a cell edge of a fraction of a \
                      mm; the cast saturates on a non-finite or absurd coordinate, which only \
                      changes which cluster the vertex joins, never a value"
        )]
        let key = (
            (at[0] / cell).floor() as i64,
            (at[1] / cell).floor() as i64,
            (at[2] / cell).floor() as i64,
            facing_class(setup, geometry, vertex),
        );
        keyed.push((key, id));
    }
    keyed.sort_unstable();

    let mut items: Vec<(u32, f64)> = protected;
    // Scratch reused across groups (cleared per group, never reallocated per
    // item): the union-find parents and the per-piece sums.
    let mut parent: Vec<usize> = Vec::new();
    let mut total: Vec<f64> = Vec::new();
    let mut centroid: Vec<V3> = Vec::new();
    let mut nearest: Vec<f64> = Vec::new();
    let mut representative: Vec<u32> = Vec::new();
    let mut start = 0_usize;
    while start < keyed.len() {
        let mut end = start + 1;
        while end < keyed.len() && keyed[end].0 == keyed[start].0 {
            end += 1;
        }
        let group = &keyed[start..end];
        // Locally connected pieces: union-find over the mesh edges whose both
        // endpoints are members of this group. The root of a piece is its
        // lowest member index, so the pieces and their order are deterministic.
        let members = group.len();
        parent.clear();
        parent.extend(0..members);
        for (local, &(_, id)) in group.iter().enumerate() {
            let vertex = id as usize;
            for &face in &setup.incident[setup.offsets[vertex]..setup.offsets[vertex + 1]] {
                for corner in tris[face] {
                    let other = welded_of_source[corner as usize];
                    if other == id {
                        continue;
                    }
                    if let Ok(found) = group.binary_search_by_key(&other, |&(_, member)| member) {
                        let (a, b) = (find_root(&mut parent, local), find_root(&mut parent, found));
                        if a != b {
                            parent[a.max(b)] = a.min(b);
                        }
                    }
                }
            }
        }
        total.clear();
        total.resize(members, 0.0);
        centroid.clear();
        centroid.resize(members, [0.0; 3]);
        for (local, &(_, id)) in group.iter().enumerate() {
            let root = find_root(&mut parent, local);
            let value = setup.vertex_curvature[id as usize];
            total[root] += value;
            centroid[root] = add(centroid[root], scale(setup.position[id as usize], value));
        }
        // Per piece, the member nearest the weighted centre; the first of equals
        // in ascending vertex order wins, as for a whole group before.
        nearest.clear();
        nearest.resize(members, f64::INFINITY);
        representative.clear();
        representative.resize(members, u32::MAX);
        for (local, &(_, id)) in group.iter().enumerate() {
            let root = find_root(&mut parent, local);
            let at = scale(centroid[root], 1.0 / total[root]);
            let offset = sub(setup.position[id as usize], at);
            let distance = dot(offset, offset);
            if distance < nearest[root] {
                nearest[root] = distance;
                representative[root] = id;
            }
        }
        for root in 0..members {
            if representative[root] != u32::MAX {
                items.push((representative[root], total[root]));
            }
        }
        start = end;
    }
    items.sort_unstable_by_key(|&(id, _)| id);

    let mut value = Vec::with_capacity(items.len());
    let mut position = Vec::with_capacity(items.len());
    let mut offsets = Vec::with_capacity(items.len() + 1);
    let mut incident = Vec::new();
    offsets.push(0_usize);
    for &(id, total) in &items {
        let vertex = id as usize;
        value.push(total);
        position.push(setup.position[vertex]);
        incident.extend_from_slice(
            &setup.incident[setup.offsets[vertex]..setup.offsets[vertex + 1]],
        );
        offsets.push(incident.len());
    }
    Some(ProxyItems {
        value,
        position,
        offsets,
        incident,
    })
}

/// The root of `at` in a union-find whose roots are the lowest index of their
/// set (path halving).
fn find_root(parent: &mut [usize], mut at: usize) -> usize {
    while parent[at] != at {
        parent[at] = parent[parent[at]];
        at = parent[at];
    }
    at
}

/// The protected-proxy spread: the same walk and replay as
/// [`curvature_parallel`], over the reduced item list.
pub(crate) fn curvature_proxy_parallel(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
    radius_mm: f64,
) -> Result<(Vec<f64>, SpreadOutcome), DelicacyError> {
    curvature_proxy_with(mesh, geometry, radius_mm, PROXY_MIN_CURVED_VERTICES)
}

/// [`curvature_proxy_parallel`] with the engagement threshold named.
fn curvature_proxy_with(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
    radius_mm: f64,
    min_curved: usize,
) -> Result<(Vec<f64>, SpreadOutcome), DelicacyError> {
    let setup = curvature_setup(mesh, geometry)?;
    let radius_sq = radius_mm * radius_mm;
    let reduced = protected_proxy(mesh, &setup, geometry, radius_mm, min_curved);
    let inputs = match &reduced {
        Some(items) => CurvatureSpreadInputs {
            mesh,
            geometry,
            topology: &setup.topology,
            value: &items.value,
            position: &items.position,
            offsets: &items.offsets,
            incident: &items.incident,
            radius_sq,
        },
        None => setup.inputs(mesh, geometry, radius_sq),
    };
    let spread = geom_kernel::run_in_pool(|| spread_all::<Exact>(&inputs, &SpreadPlan::PRODUCTION))
        .map_err(|error| DelicacyError::Kernel(error.to_string()))?;
    let by_face = charge_to_by_face(geometry, &spread.charge);
    Ok((by_face, spread))
}

/// What the curvature walks read, built once and serially: the welded
/// topology, each vertex's normalized absolute angle defect, each vertex's
/// position and its non-degenerate incident faces as one CSR table.
struct CurvatureSetup {
    topology: crate::FaceTopology,
    vertex_curvature: Vec<f64>,
    position: Vec<V3>,
    offsets: Vec<usize>,
    incident: Vec<usize>,
}

impl CurvatureSetup {
    fn inputs<'a>(
        &'a self,
        mesh: &'a Mesh,
        geometry: &'a FaceGeometryPass,
        radius_sq: f64,
    ) -> CurvatureSpreadInputs<'a> {
        CurvatureSpreadInputs {
            mesh,
            geometry,
            topology: &self.topology,
            value: &self.vertex_curvature,
            position: &self.position,
            offsets: &self.offsets,
            incident: &self.incident,
            radius_sq,
        }
    }
}

fn curvature_setup(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
) -> Result<CurvatureSetup, DelicacyError> {
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

    // The block streams index faces and vertices as `u32`, and the stamp uses
    // `u32::MAX` for "unstamped". A mesh that large cannot be built (the mesh
    // itself indexes with `u32`), so this is a reported limit, not a wrap.
    if tris.len() >= u32::MAX as usize || welded_count >= u32::MAX as usize {
        return Err(DelicacyError::Kernel(
            "the curvature spread indexes faces and vertices with u32".to_owned(),
        ));
    }
    Ok(CurvatureSetup {
        topology,
        vertex_curvature,
        position,
        offsets,
        incident,
    })
}

/// The flat, read-only view the curvature walks share (a struct of slices, so
/// a block walk is a pure function of it).
pub(crate) struct CurvatureSpreadInputs<'a> {
    mesh: &'a Mesh,
    geometry: &'a FaceGeometryPass,
    topology: &'a crate::FaceTopology,
    /// Normalized absolute angle defect per welded vertex.
    value: &'a [f64],
    position: &'a [V3],
    /// CSR offsets of each vertex's non-degenerate incident faces.
    offsets: &'a [usize],
    incident: &'a [usize],
    radius_sq: f64,
}

/// One worker's reusable walk state: the face stamp and the frontier.
///
/// The stamp is keyed by the walking vertex's index and is never cleared:
/// each vertex is walked exactly once in the run, so a stamp left by another
/// vertex's walk (on this worker, in another block) never equals the current
/// vertex.
struct SpreadScratch {
    stamp: Vec<u32>,
    frontier: Vec<usize>,
}

/// What one vertex block contributes, in walk order.
struct BlockContributions {
    /// Charged faces, in the order the serial walk charges them.
    faces: Vec<u32>,
    /// `value * inside` for the face at the same slot.
    amounts: Vec<f64>,
    /// `Some(s)`: vertices `[block.start, s)` are in this output and `s` is the
    /// first unprocessed vertex (`start < s < end`). `None`: the whole block.
    stopped_at: Option<u32>,
    counts: CurvatureSpreadCounts,
}

/// The `charge` array and everything counted while building it.
pub(crate) struct SpreadOutcome {
    charge: Vec<f64>,
    pub(crate) counts: CurvatureSpreadCounts,
    /// Faces that received at least two addends: the only faces where the
    /// order of addition can matter.
    pub(crate) faces_multiply_charged: usize,
    pub(crate) waves: WaveStats,
}

/// One block of the spread: the serial loop body for `vertices`, except that a
/// charge is PUSHED as `(face, value * inside)` instead of added.
///
/// The pair cap is checked only AFTER a vertex's walk completes, so a block
/// always finishes its first vertex and `stopped_at` is never the block's own
/// start (nor its end: the last vertex never stops).
fn spread_block(
    inp: &CurvatureSpreadInputs<'_>,
    vertices: Range<u32>,
    cap: usize,
    scratch: &mut SpreadScratch,
    out: &mut BlockContributions,
) {
    let tris = inp.mesh.tris();
    let SpreadScratch { stamp, frontier } = scratch;
    for vertex_id in vertices.clone() {
        let vertex = vertex_id as usize;
        let value = inp.value[vertex];
        if value <= 0.0 {
            continue;
        }
        out.counts.vertices_spread += 1;
        let centre = inp.position[vertex];
        frontier.clear();
        for &face in &inp.incident[inp.offsets[vertex]..inp.offsets[vertex + 1]] {
            // A face incident at two corners (a sliver) is listed twice.
            if stamp[face] != vertex_id {
                stamp[face] = vertex_id;
                frontier.push(face);
            }
        }
        let ring = frontier.len();
        let mut head = 0_usize;
        while head < frontier.len() {
            let face = frontier[head];
            head += 1;
            out.counts.pairs_measured += 1;
            let inside = ball_face_area_mm2(inp.mesh, inp.geometry, face, centre, inp.radius_sq);
            if inside <= 0.0 {
                continue;
            }
            out.counts.pairs_charged += 1;
            if head > ring {
                out.counts.pairs_charged_beyond_ring += 1;
            }
            // `face < tris.len() < u32::MAX`, checked in `curvature_setup`.
            out.faces.push(u32::try_from(face).unwrap_or(u32::MAX));
            out.amounts.push(value * inside);
            // Expand only from a face that is itself inside the ball, so the
            // neighbourhood stays connected through the surface.
            if let Some(neighbours) = inp.topology.neighbors(face) {
                for &next in neighbours {
                    let next = next as usize;
                    if next >= tris.len()
                        || inp.geometry.degenerate[next]
                        || stamp[next] == vertex_id
                    {
                        continue;
                    }
                    stamp[next] = vertex_id;
                    frontier.push(next);
                }
            }
        }
        if out.faces.len() >= cap && vertex_id + 1 < vertices.end {
            out.stopped_at = Some(vertex_id + 1);
            break;
        }
    }
}

/// Runs the whole spread in the AMBIENT rayon pool (the production caller
/// installs the kernel pool; tests install pools of their own widths).
fn spread_all<H: SpreadHooks>(inp: &CurvatureSpreadInputs<'_>, plan: &SpreadPlan) -> SpreadOutcome {
    let face_count = inp.mesh.tris().len();
    let welded_count = inp.value.len();
    let scratch_pool = ScratchPool::<SpreadScratch>::new();
    let spread = |block: Range<u32>| {
        let mut scratch = scratch_pool.take(|| SpreadScratch {
            stamp: vec![u32::MAX; face_count],
            frontier: Vec::new(),
        });
        let mut out = BlockContributions {
            faces: Vec::new(),
            amounts: Vec::new(),
            stopped_at: None,
            counts: CurvatureSpreadCounts::default(),
        };
        spread_block(inp, block, plan.pairs_per_block_cap, &mut scratch, &mut out);
        scratch_pool.put(scratch);
        out
    };

    let mut charge = vec![0.0_f64; face_count];
    // Addends received per face, saturating at 2 (only "more than one" matters).
    let mut addends = vec![0_u8; face_count];
    let mut counts = CurvatureSpreadCounts::default();
    let mut faces_multiply_charged = 0_usize;
    let mut waves = WaveStats::default();
    let mut replay = |out: &BlockContributions| {
        for (&face, &amount) in out.faces.iter().zip(&out.amounts) {
            let face = face as usize;
            charge[face] += amount;
            match addends[face] {
                0 => addends[face] = 1,
                1 => {
                    addends[face] = 2;
                    faces_multiply_charged += 1;
                }
                _ => {}
            }
        }
        counts.vertices_spread += out.counts.vertices_spread;
        counts.pairs_measured += out.counts.pairs_measured;
        counts.pairs_charged += out.counts.pairs_charged;
        counts.pairs_charged_beyond_ring += out.counts.pairs_charged_beyond_ring;
    };
    // `welded_count < u32::MAX`, checked in `curvature_setup`.
    spread_range::<H, _, _, _, _>(
        0..u32::try_from(welded_count).unwrap_or(u32::MAX),
        plan.block_vertices,
        plan,
        &mut waves,
        &spread,
        &|out: &BlockContributions| out.stopped_at,
        &mut replay,
    );
    SpreadOutcome {
        charge,
        counts,
        faces_multiply_charged,
        waves,
    }
}

/// Reads each face's summed charge as its mean over the face's area.
fn charge_to_by_face(geometry: &FaceGeometryPass, charge: &[f64]) -> Vec<f64> {
    let mut by_face = Vec::with_capacity(charge.len());
    for (face, &charged) in charge.iter().enumerate() {
        let area = geometry.areas_mm2[face];
        // `is_nan() || <= 0` is `!(area > 0.0)`, NaN included.
        if geometry.degenerate[face] || area.is_nan() || area <= 0.0 {
            by_face.push(0.0);
            continue;
        }
        by_face.push((charged / area).clamp(0.0, 1.0));
    }
    by_face
}

/// The serial walk exactly as it stood before the parallel spread (2026-10-09):
/// frozen former production code, kept as the test oracle and the witness
/// example's reference. Not a re-implementation: setup, the per-pair body and
/// the final normalisation are the same functions and lines production uses;
/// only the schedule (one vertex at a time, adding straight into `charge`)
/// differs.
#[cfg(any(test, feature = "serial-oracle"))]
pub(crate) fn curvature_by_face_serial(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
    radius_mm: f64,
) -> Result<(Vec<f64>, CurvatureSpreadCounts), DelicacyError> {
    let setup = curvature_setup(mesh, geometry)?;
    let radius_sq = radius_mm * radius_mm;
    let tris = mesh.tris();
    let welded_count = setup.vertex_curvature.len();
    let (position, offsets, incident, topology) =
        (&setup.position, &setup.offsets, &setup.incident, &setup.topology);
    let mut charge = vec![0.0_f64; tris.len()];
    let mut stamp = vec![usize::MAX; tris.len()];
    let mut frontier: Vec<usize> = Vec::new();
    let mut counts = CurvatureSpreadCounts::default();
    for vertex in 0..welded_count {
        let value = setup.vertex_curvature[vertex];
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
    Ok((charge_to_by_face(geometry, &charge), counts))
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
pub(crate) mod tests {
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
    pub(crate) fn gridded_cube(n: u32) -> Mesh {
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
                curvature_by_face(&mesh, &geometry, radius, DelicacyEvaluation::Exact)
                    .expect("curvature");
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

    // ---------------------------------------------------------------------
    // The parallel spread against its frozen serial oracle (2026-10-09).
    // ---------------------------------------------------------------------

    /// Runs `body` inside a local rayon pool of exactly `threads` workers.
    fn in_pool<R: Send>(threads: usize, body: impl FnOnce() -> R + Send) -> R {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("local rayon pool")
            .install(body)
    }

    /// A sabotage: replay each wave's blocks in descending order.
    struct ReverseBlocks;
    impl SpreadHooks for ReverseBlocks {
        const REVERSE_BLOCKS: bool = true;
    }

    /// A sabotage: never replay an overflow remainder.
    struct DropRemainder;
    impl SpreadHooks for DropRemainder {
        const DROP_REMAINDER: bool = true;
    }

    fn bits(values: &[f64]) -> Vec<u64> {
        values.iter().map(|value| value.to_bits()).collect()
    }

    /// The fixtures the order of the replay can show on: a gridded cube (eight
    /// curved corners whose balls overlap many faces), a thin fin, a recessed
    /// disc.
    fn spread_fixtures() -> Vec<(&'static str, Mesh)> {
        vec![
            ("gridded_cube(8)", gridded_cube(8)),
            ("gridded_cube(16)", gridded_cube(16)),
            ("thin_fin", make_thin_fin().expect("thin fin")),
            (
                "recessed_disc",
                crate::make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("recessed disc"),
            ),
        ]
    }

    /// Runs the spread of `mesh` under a hook type and a plan, in a pool of
    /// `threads`, and returns `(by_face, outcome)`.
    fn run_spread<H: SpreadHooks>(
        mesh: &Mesh,
        plan: SpreadPlan,
        threads: usize,
    ) -> (Vec<f64>, SpreadOutcome) {
        let geometry = FaceGeometryPass::run(mesh);
        let radius = settings().thin_feature_mm();
        let setup = curvature_setup(mesh, &geometry).expect("setup");
        let inputs = setup.inputs(mesh, &geometry, radius * radius);
        let outcome = in_pool(threads, || spread_all::<H>(&inputs, &plan));
        (charge_to_by_face(&geometry, &outcome.charge), outcome)
    }

    fn plan(block_vertices: u32, cap: usize) -> SpreadPlan {
        SpreadPlan {
            block_vertices,
            wave_blocks: 32,
            pairs_per_block_cap: cap,
        }
    }

    /// **The parallel spread is the serial walk, bit for bit.** Across pool
    /// widths 1, 3 and 8, block lengths 1, 7 and 512 and caps 1, 3 and 1 Mi,
    /// on four fixtures: every face's bits equal the oracle's, and the work
    /// counters equal the oracle's, which proves every vertex was processed
    /// exactly once (none dropped, none repeated by an overflow remainder).
    #[test]
    fn the_parallel_spread_equals_the_serial_oracle_bit_for_bit() {
        let radius = settings().thin_feature_mm();
        for (name, mesh) in spread_fixtures() {
            let geometry = FaceGeometryPass::run(&mesh);
            let (serial, serial_counts) =
                curvature_by_face_serial(&mesh, &geometry, radius).expect("serial oracle");
            for threads in [1_usize, 3, 4, 8, 16] {
                for block in [1_u32, 7, 512] {
                    for cap in [1_usize, 3, 1 << 20] {
                        let (parallel, outcome) =
                            run_spread::<Exact>(&mesh, plan(block, cap), threads);
                        assert_eq!(
                            bits(&parallel),
                            bits(&serial),
                            "{name}: threads {threads} block {block} cap {cap}"
                        );
                        assert_eq!(
                            outcome.counts, serial_counts,
                            "{name}: threads {threads} block {block} cap {cap}"
                        );
                    }
                }
            }
        }
    }

    /// The production entry (`curvature_by_face`, kernel pool, production plan)
    /// equals the oracle on the same fixtures.
    #[test]
    fn the_production_entry_equals_the_serial_oracle() {
        let radius = settings().thin_feature_mm();
        for (name, mesh) in spread_fixtures() {
            let geometry = FaceGeometryPass::run(&mesh);
            let (serial, serial_counts) =
                curvature_by_face_serial(&mesh, &geometry, radius).expect("serial oracle");
            let (parallel, counts) =
                curvature_by_face(&mesh, &geometry, radius, DelicacyEvaluation::Exact)
                    .expect("parallel spread");
            assert_eq!(bits(&parallel), bits(&serial), "{name}");
            assert_eq!(counts, serial_counts, "{name}");
        }
    }

    /// A UV sphere of radius `radius` mm with `rings x segments` quads, each
    /// ring's vertices nudged radially by a deterministic ripple so the angle
    /// defects are noisy and positive and negative alike, like a decimated
    /// dense scan. Closed, outward-wound.
    fn rippled_sphere(rings: u32, segments: u32, radius: f64, ripple: f64) -> Mesh {
        let mut verts: Vec<V3> = vec![[0.0, 0.0, radius]];
        for ring in 1..rings {
            let polar = std::f64::consts::PI * f64::from(ring) / f64::from(rings);
            for segment in 0..segments {
                let azimuth = std::f64::consts::TAU * f64::from(segment) / f64::from(segments);
                let wobble = 1.0
                    + ripple * (f64::from((ring * 31 + segment * 17) % 7) - 3.0) / 3.0;
                let r = radius * wobble;
                verts.push([
                    r * polar.sin() * azimuth.cos(),
                    r * polar.sin() * azimuth.sin(),
                    r * polar.cos(),
                ]);
            }
        }
        verts.push([0.0, 0.0, -radius]);
        let south = u32::try_from(verts.len() - 1).expect("small");
        let at = |ring: u32, segment: u32| 1 + (ring - 1) * segments + segment % segments;
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for segment in 0..segments {
            tris.push([0, at(1, segment), at(1, segment + 1)]);
        }
        for ring in 1..rings - 1 {
            for segment in 0..segments {
                let (a, b) = (at(ring, segment), at(ring, segment + 1));
                let (c, d) = (at(ring + 1, segment), at(ring + 1, segment + 1));
                tris.push([a, c, b]);
                tris.push([b, c, d]);
            }
        }
        for segment in 0..segments {
            tris.push([south, at(rings - 1, segment + 1), at(rings - 1, segment)]);
        }
        Mesh::new(verts, tris).expect("a closed sphere")
    }

    /// Appends one closed rectangular slab of `nx x ny` cells of `pitch` mm,
    /// lower-left corner at (0, 0), flat bottom at `z_bottom`, and a gently
    /// rippled top at about `z_top` (amplitude `amplitude`, every top vertex a
    /// small nonzero angle defect, none sharp). Returns the top vertex index of
    /// grid point (i, j) as `first + j * (nx + 1) + i`.
    fn push_rippled_slab(
        verts: &mut Vec<V3>,
        tris: &mut Vec<[u32; 3]>,
        (nx, ny): (u32, u32),
        pitch: f64,
        (z_bottom, z_top, amplitude): (f64, f64, f64),
    ) {
        let stride = nx + 1;
        let first_top = u32::try_from(verts.len()).expect("small");
        for j in 0..=ny {
            for i in 0..=nx {
                let ripple = (0.5 * f64::from(i)).sin() * (0.6 * f64::from(j)).sin();
                verts.push([
                    f64::from(i) * pitch,
                    f64::from(j) * pitch,
                    z_top + amplitude * ripple,
                ]);
            }
        }
        let first_bottom = u32::try_from(verts.len()).expect("small");
        for j in 0..=ny {
            for i in 0..=nx {
                verts.push([f64::from(i) * pitch, f64::from(j) * pitch, z_bottom]);
            }
        }
        let top = |i: u32, j: u32| first_top + j * stride + i;
        let bottom = |i: u32, j: u32| first_bottom + j * stride + i;
        for j in 0..ny {
            for i in 0..nx {
                tris.push([top(i, j), top(i + 1, j), top(i + 1, j + 1)]);
                tris.push([top(i, j), top(i + 1, j + 1), top(i, j + 1)]);
                tris.push([bottom(i, j), bottom(i + 1, j + 1), bottom(i + 1, j)]);
                tris.push([bottom(i, j), bottom(i, j + 1), bottom(i + 1, j + 1)]);
            }
        }
        // The perimeter, counter-clockwise from above, as directed grid edges.
        let mut ring: Vec<(u32, u32)> = Vec::new();
        ring.extend((0..nx).map(|i| (i, 0)));
        ring.extend((0..ny).map(|j| (nx, j)));
        ring.extend((0..nx).map(|i| (nx - i, ny)));
        ring.extend((0..ny).map(|j| (0, ny - j)));
        for (index, &(pi, pj)) in ring.iter().enumerate() {
            let (qi, qj) = ring[(index + 1) % ring.len()];
            tris.push([top(qi, qj), top(pi, pj), bottom(pi, pj)]);
            tris.push([top(qi, qj), bottom(pi, pj), bottom(qi, qj)]);
        }
    }

    /// **Two nearby same-facing patches that are not locally connected both
    /// keep their curvature under the proxy** (review of 85cc41e).
    ///
    /// Two 8 x 8 mm rippled plates are stacked 0.04 mm apart, so their upward
    /// faces share every (cell, facing) key (cell 0.375 mm) while the only
    /// surface path between them runs round the plates' edges, far outside the
    /// 1.5 mm spread ball. Merging by (cell, facing) alone puts both plates'
    /// vertices in one walk that starts on one plate, and the other plate's
    /// curvature moves away. The lower plate is nearly flat and the upper ten
    /// times as rippled, so a mass that lands on the wrong plate is large.
    /// With the engagement threshold lowered to engage the proxy on this
    /// fixture, both plates must hold their own walks in nearly every cell,
    /// and each plate's interior fold must stay within 5% of the exact fold:
    /// the merge moves a cell's mass by at most one cell (0.375 mm) inside a
    /// 1.5 mm ball, which measured 0.2% on both plates. Sabotage (every member
    /// of a cell joins one piece, no connectivity split) fails the walk count:
    /// the lower plate then keeps 0 walks.
    #[test]
    fn two_nearby_unconnected_same_facing_patches_both_keep_their_curvature() {
        let mut verts = Vec::new();
        let mut tris = Vec::new();
        // The lower plate is nearly flat, the upper ten times as rippled, so
        // one plate's mass landing on the other plate's walks is large.
        push_rippled_slab(&mut verts, &mut tris, (80, 80), 0.1, (-1.0, 0.10, 0.002));
        push_rippled_slab(&mut verts, &mut tris, (80, 80), 0.1, (0.15, 0.30, 0.02));
        let mesh = Mesh::new(verts, tris).expect("two closed slabs");
        let geometry = FaceGeometryPass::run(&mesh);
        let setup = curvature_setup(&mesh, &geometry).expect("setup");
        let radius = settings().thin_feature_mm();
        let min_curved = 2_000;

        let items = protected_proxy(&mesh, &setup, &geometry, radius, min_curved)
            .expect("the lowered threshold engages the proxy");
        let curved = setup.vertex_curvature.iter().filter(|&&v| v > 0.0).count();
        assert!(items.value.len() * 4 < curved, "{} items for {curved}", items.value.len());
        let on_plate = |lower: f64, upper: f64| {
            items
                .position
                .iter()
                .zip(&items.value)
                .filter(|(p, &v)| {
                    v < PROTECTED_DEFECT
                        && p[2] > lower
                        && p[2] < upper
                        && p[0] > 2.0
                        && p[0] < 6.0
                        && p[1] > 2.0
                        && p[1] < 6.0
                })
                .count()
        };
        let lower_walks = on_plate(0.05, 0.14);
        let upper_walks = on_plate(0.25, 0.35);
        // The 4 x 4 mm zone holds about 114 cells; each plate needs its own walk
        // in nearly every one (a merged pair would give one walk per cell, to one
        // plate only).
        assert!(lower_walks > 100, "lower plate has {lower_walks} merged walks");
        assert!(upper_walks > 100, "upper plate has {upper_walks} merged walks");

        let (exact, _) = curvature_parallel(&mesh, &geometry, radius).expect("exact");
        let (proxy, _) = curvature_proxy_with(&mesh, &geometry, radius, min_curved).expect("proxy");
        let interior_fold = |by_face: &[f64], lower: f64, upper: f64| -> f64 {
            let verts = mesh.verts();
            mesh.tris()
                .iter()
                .enumerate()
                .filter(|(_, t)| {
                    let at = t.map(|corner| verts[corner as usize]);
                    let (cx, cy, cz) = (
                        (at[0][0] + at[1][0] + at[2][0]) / 3.0,
                        (at[0][1] + at[1][1] + at[2][1]) / 3.0,
                        (at[0][2] + at[1][2] + at[2][2]) / 3.0,
                    );
                    cz > lower && cz < upper && cx > 2.5 && cx < 5.5 && cy > 2.5 && cy < 5.5
                })
                .map(|(face, _)| by_face[face] * geometry.areas_mm2[face])
                .sum()
        };
        for (name, lower, upper) in [("lower", 0.05, 0.14), ("upper", 0.25, 0.35)] {
            let want = interior_fold(&exact, lower, upper);
            let got = interior_fold(&proxy, lower, upper);
            assert!(want > 0.0, "{name}: the exact fold is {want}");
            let relative = (got - want).abs() / want;
            assert!(
                relative < 0.05,
                "{name} plate: proxy fold {got} vs exact {want} (off by {relative})"
            );
        }
    }

    /// On a mesh too small to pay, the proxy does not engage and the field is
    /// the exact field bit for bit.
    #[test]
    fn the_proxy_is_the_exact_field_on_a_small_mesh() {
        for mesh in [gridded_cube(8), rippled_sphere(24, 48, 4.0, 0.002)] {
            let exact = evaluate_delicacy_with(
                &mesh,
                "l",
                settings(),
                config(),
                DelicacyEvaluation::Exact,
            )
            .expect("exact");
            let proxy = evaluate_delicacy_with(
                &mesh,
                "l",
                settings(),
                config(),
                DelicacyEvaluation::ProtectedProxy,
            )
            .expect("proxy");
            assert_eq!(exact.digest(), proxy.digest());
        }
    }

    /// The proxy engages on a dense mesh, merges the many small vertices into
    /// few walks, conserves their curvature mass, keeps every protected
    /// vertex as its own walk, and lands near the exact field.
    #[test]
    fn the_proxy_merges_small_vertices_keeps_the_sharp_ones_and_conserves_mass() {
        let mesh = rippled_sphere(180, 360, 6.0, 0.0005);
        let geometry = FaceGeometryPass::run(&mesh);
        let setup = curvature_setup(&mesh, &geometry).expect("setup");
        let radius = settings().thin_feature_mm();
        let items = protected_proxy(&mesh, &setup, &geometry, radius, PROXY_MIN_CURVED_VERTICES)
            .expect("dense mesh engages the proxy");
        let curved = setup.vertex_curvature.iter().filter(|&&v| v > 0.0).count();
        assert!(curved >= PROXY_MIN_CURVED_VERTICES, "{curved}");
        assert!(
            items.value.len() * 3 < curved,
            "{} items for {curved} curved vertices: the proxy merged almost nothing",
            items.value.len()
        );
        let mass_before: f64 = setup.vertex_curvature.iter().sum();
        let mass_after: f64 = items.value.iter().sum();
        assert!(
            (mass_before - mass_after).abs() <= 1e-9 * mass_before,
            "{mass_before} vs {mass_after}"
        );
        // Every protected vertex is its own item with its own value.
        let protected = setup
            .vertex_curvature
            .iter()
            .filter(|&&v| v >= PROTECTED_DEFECT)
            .count();
        let own = items.value.iter().filter(|&&v| v >= PROTECTED_DEFECT).count();
        assert!(protected > 0, "the ripple should leave some sharp vertices");
        assert!(own >= protected, "{own} items at or above the protected defect for {protected} vertices");

        let exact = evaluate_delicacy_with(&mesh, "l", settings(), config(), DelicacyEvaluation::Exact)
            .expect("exact");
        let proxy =
            evaluate_delicacy_with(&mesh, "l", settings(), config(), DelicacyEvaluation::ProtectedProxy)
                .expect("proxy");
        let worst = exact
            .face_values()
            .iter()
            .zip(proxy.face_values())
            .map(|(a, b)| (a.curvature() - b.curvature()).abs())
            .fold(0.0_f64, f64::max);
        assert!(worst < 0.15, "worst per-face curvature difference {worst}");
    }

    /// The proxy field does not depend on the thread count.
    #[test]
    fn the_proxy_field_is_bit_reproducible_across_thread_counts() {
        let mesh = rippled_sphere(180, 360, 6.0, 0.0005);
        let digest = |threads: usize| {
            in_pool(threads, || {
                evaluate_delicacy_with(
                    &mesh,
                    "l",
                    settings(),
                    config(),
                    DelicacyEvaluation::ProtectedProxy,
                )
                .expect("proxy")
                .digest()
                .to_owned()
            })
        };
        assert_eq!(digest(1), digest(5));
    }

    /// The two sides of a thin sheet face opposite ways, so they can never share
    /// a cluster: a merged walk would otherwise bleed across the wall.
    #[test]
    fn opposite_faces_of_a_thin_sheet_get_different_facing_classes() {
        let mesh = gridded_cube(8);
        // Flatten the cube into a 0.05 mm sheet.
        let flat: Vec<V3> = mesh.verts().iter().map(|v| [v[0], v[1], v[2] * 0.05 / 8.0]).collect();
        let sheet = Mesh::new(flat, mesh.tris().to_vec()).expect("sheet");
        let geometry = FaceGeometryPass::run(&sheet);
        let setup = curvature_setup(&sheet, &geometry).expect("setup");
        let mut top = None;
        let mut bottom = None;
        for (vertex, p) in setup.position.iter().enumerate() {
            // Interior vertices of the two big faces.
            if p[0] > 0.5 && p[0] < 7.5 && p[1] > 0.5 && p[1] < 7.5 {
                if p[2] > 0.0 {
                    top = Some(vertex);
                } else {
                    bottom = Some(vertex);
                }
            }
        }
        let top = facing_class(&setup, &geometry, top.expect("a top vertex"));
        let bottom = facing_class(&setup, &geometry, bottom.expect("a bottom vertex"));
        assert_ne!(top, bottom);
    }

    /// Path hits: the faces where order can matter exist on the fixture, and a
    /// small cap really forces the overflow path. At block 7, cap 3 on
    /// `gridded_cube(8)` the corner vertex (vertex 0) charges more than 3
    /// pairs, so its block stops after one vertex and the rest of the block
    /// runs again as a remainder.
    #[test]
    fn the_replay_order_matters_somewhere_and_the_overflow_path_fires() {
        let multiply_charged: Vec<usize> = spread_fixtures()
            .iter()
            .map(|(_, mesh)| {
                run_spread::<Exact>(mesh, plan(7, 1 << 20), 3)
                    .1
                    .faces_multiply_charged
            })
            .collect();
        assert!(
            multiply_charged.iter().any(|&faces| faces > 0),
            "no fixture has a face with two addends; none can show order: {multiply_charged:?}"
        );
        let mesh = gridded_cube(8);
        let (_, outcome) = run_spread::<Exact>(&mesh, plan(7, 3), 3);
        assert!(outcome.waves.overflow_splits > 0, "{:?}", outcome.waves);
        assert!(outcome.waves.blocks > outcome.waves.waves && outcome.waves.waves >= 1);
        // The production plan on the same mesh needs no overflow.
        let (_, calm) = run_spread::<Exact>(&mesh, SpreadPlan::PRODUCTION, 3);
        assert_eq!(calm.waves.overflow_splits, 0);
    }

    /// `stopped_at` is the FIRST UNPROCESSED vertex of a block: strictly
    /// inside it, never the block's own start (every block makes progress) and
    /// never its end (the last vertex never stops), and an early stop never
    /// leaves a vertex's walk half done (the prefix plus the remainder covers
    /// the block).
    #[test]
    fn a_stopped_block_reports_a_first_unprocessed_vertex_strictly_inside_it() {
        let mesh = gridded_cube(8);
        let geometry = FaceGeometryPass::run(&mesh);
        let setup = curvature_setup(&mesh, &geometry).expect("setup");
        let radius = settings().thin_feature_mm();
        let inputs = setup.inputs(&mesh, &geometry, radius * radius);
        let vertices = u32::try_from(inputs.value.len()).expect("small mesh");
        let mut stopped = 0;
        for block_len in [1_u32, 2, 7, 512] {
            for block in crate::surface_par::chunk(&(0..vertices), block_len) {
                for cap in [1_usize, 3, 1 << 20] {
                    let mut scratch = SpreadScratch {
                        stamp: vec![u32::MAX; mesh.tris().len()],
                        frontier: Vec::new(),
                    };
                    let mut out = BlockContributions {
                        faces: Vec::new(),
                        amounts: Vec::new(),
                        stopped_at: None,
                        counts: CurvatureSpreadCounts::default(),
                    };
                    spread_block(&inputs, block.clone(), cap, &mut scratch, &mut out);
                    if let Some(at) = out.stopped_at {
                        stopped += 1;
                        assert!(
                            block.start < at && at < block.end,
                            "block {block:?} cap {cap} stopped at {at}"
                        );
                    }
                }
            }
        }
        assert!(stopped > 0, "no block ever stopped; the cap rule was not exercised");
    }

    /// An empty range runs no blocks, and `chunk` refuses a zero length.
    #[test]
    fn an_empty_vertex_range_runs_no_blocks() {
        let mut stats = WaveStats::default();
        in_pool(2, || {
            spread_range::<Exact, (), _, _, _>(
                4..4,
                8,
                &SpreadPlan::PRODUCTION,
                &mut stats,
                &|_: Range<u32>| (),
                &|(): &()| None,
                &mut |(): &()| {},
            );
        });
        assert_eq!(stats, WaveStats::default());
    }

    /// **Sabotage 1 (replay order).** Replaying a wave's blocks in the wrong
    /// order must change at least one face's bits on at least one fixture; if
    /// no fixture can see it, the fixtures are too sparse to show order and
    /// the equality test above proves nothing about the replay.
    #[test]
    fn sabotage_reversed_block_replay_changes_the_bits() {
        let mut seen = 0;
        for (name, mesh) in spread_fixtures() {
            let (honest, _) = run_spread::<Exact>(&mesh, plan(7, 1 << 20), 3);
            let (broken, _) = run_spread::<ReverseBlocks>(&mesh, plan(7, 1 << 20), 3);
            if bits(&honest) != bits(&broken) {
                seen += 1;
                println!("{name}: reversed replay changes the bits");
            }
        }
        assert!(seen > 0, "no fixture shows the replay order; use a denser one");
    }

    /// **Sabotage 2 (remainder).** Dropping the overflow remainder at block 7,
    /// cap 3 must change the result: those vertices are never spread.
    #[test]
    fn sabotage_dropping_the_overflow_remainder_changes_the_bits() {
        let mesh = gridded_cube(8);
        let (honest, _) = run_spread::<Exact>(&mesh, plan(7, 3), 3);
        let (broken, outcome) = run_spread::<DropRemainder>(&mesh, plan(7, 3), 3);
        assert!(outcome.waves.overflow_splits > 0);
        assert_ne!(bits(&honest), bits(&broken));
    }
}
