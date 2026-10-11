//! The producer-side **display-face** evaluator.
//!
//! [`display.rs`](crate::display) owns the immutable contract; this module is
//! the batched geometry that fills it, one `FaceTable`-scoped, source-order
//! value per face.
//!
//! # The three factors, and why they multiply
//!
//! ```text
//! D(f) = visibility(f) × detail(f) × tip_factor(f)
//! ```
//!
//! * **`visibility`** is the cosine-weighted, unoccluded fraction of the
//!   AUTHORED upper hemisphere:
//!
//!   ```text
//!   visibility(f) = Σ_k max(0, n_f·v_k)·unoccluded(f, v_k) / Σ_k max(0, ẑ·v_k)
//!   ```
//!
//!   over a Fibonacci lattice `{v_k}` on the upper hemisphere.  The
//!   denominator is the same sum for a face pointing straight up, so an
//!   unoccluded up-facing face scores exactly `1`, a horizontal face scores
//!   `1/2` of that lobe overlap, and a face whose normal points straight down
//!   scores exactly `0` — **by construction, with no cutoff to choose.**  This
//!   is the owner's "top matters more than bottom along the AUTHORED Z axis",
//!   measured rather than asserted, and it is the factor that decides the
//!   Morel base, whose textured ground and whose relief-text underside BOTH
//!   carry detail.
//! * **`detail`** is the SPREAD of the fold angles inside a surface
//!   neighbourhood of radius `R_detail`: `1 − GM/AM` of the angles between
//!   adjacent faces' normals, each fold weighted by the normal change it
//!   carries (`length × angle`), folds below the printer's change resolution
//!   excluded ([`spread_of_folds`], [`neighbourhood_pass`]).  It is `0` on a
//!   flat region AND on uniform change — a crease, a regular rib or groove, a
//!   uniformly faceted wall, every fold the same angle — and rises toward `1`
//!   as the fold angles spread over orders of magnitude.  `0` and the limit
//!   `1` are meanings (no spread; unbounded spread), not chosen divisors.
//!   This is the owner's ruling of 2026-09-05, built 2026-09-28: "detail" is
//!   a WIDE DISTRIBUTION of normal-change magnitudes (small, medium and large
//!   together), and a crease or a regular rib is "change", not detail — so
//!   the SPREAD of the local normal-change distribution, not its mean.
//!
//!   **What it replaced, and why.**  Until 2026-09-28 `detail` was the
//!   area-weighted MEAN of `1 − cos` between the face's normal and its
//!   neighbours' — the owner's first reading, "local low cosine similarity
//!   among neighbouring triangle normals".  A mean cannot tell a crease from
//!   texture: every face within `R_detail` of a 90° rib edge scored high.  On
//!   `Round_100.stl` that put 24.1 mm² of display charge on the stiffener
//!   ribs and rim annulus of the glue-face-UP pose — the owner's correct pose
//!   — and was the deciding term against it (`plinth-evidence/slice-sweep-
//!   2026-09-02/field-2026-09-28/round50.md` Q3).  Measured on the fixtures in
//!   `tests/it/display_field.rs`: a regular 1 mm V-groove field scored D 0.0594
//!   against an irregular bump texture's 0.0762 under the mean; under the
//!   spread the grooves read as change.
//! * **`tip_factor`** discounts the confound the owner named: protrusion tips
//!   articulate sharply and are the BEST places to land supports.
//!
//!   ```text
//!   tip_factor(f) = 1 − min(1, K(f) / 2π)
//!   ```
//!
//!   where `K(f)` is the TOTAL Gaussian curvature of the same `R_detail`
//!   neighbourhood — the sum of the SIGNED welded angle defects
//!   ([`crate::delicacy_eval::welded_angle_defect`], shared, not re-derived)
//!   lumped onto faces.  This is Gauss-Bonnet, not a tuning: a patch that has
//!   closed all the way over into a tip has turned through a full hemisphere
//!   of directions and its total curvature IS `2π`, whatever its size or
//!   tessellation; a near-planar patch has total curvature `≈ 0` however
//!   violently it articulates, because a texture's convex crests and concave
//!   troughs cancel exactly.  So carving, grooves and rims are not discounted
//!   at all and a nub is discounted in proportion to how far it has closed.
//!
//!   **This replaced a two-radius test, on evidence, not on taste.**  The
//!   first form compared articulation at the scar radius against articulation
//!   at `R_detail`, expecting a tip's to COLLAPSE as the neighbourhood ran
//!   off the tip onto its shaft.  Measured on a 0.35 mm capped peg
//!   (`tests/it/display_field.rs`) it never fired at all: the widened
//!   neighbourhood ran onto a CYLINDRICAL shaft whose normals differ from the
//!   cap's just as much, so the articulation did not collapse and the
//!   discount was identically `1`.  The recorded value was `min tip_factor
//!   1.000000` on every cap face.  The curvature identity has no such
//!   dependence on what the protrusion is mounted to.
//!
//! **They multiply, and that choice is load-bearing.**  An additive or
//! probabilistic-OR combination would give a smooth, featureless authored-top
//! face a positive `D` on visibility alone, and would make the authored frame
//! itself an orientation vote on every part.  Under the product, a
//! featureless face has `detail = 0` and therefore `D = 0`, the objective is
//! unchanged there, and the authored-up signal speaks only where there is
//! detail for it to speak about.  That is also what makes `D = 0` reproduce
//! the pre-display objective exactly on plain geometry rather than
//! approximately.
//!
//! **THE RECORD, CORRECTED 2026-09-28.**  This paragraph used to justify the
//! product with the claim that "`Round 50`/`Round 100` are correct at
//! identity while `Round 25` is correct at a 180° flip".  That was wrong. All
//! three `Round_*.stl` bases are authored glue face DOWN (the plain plane at
//! the authored `z` minimum, the recess and stiffener ribs at the maximum),
//! and the owner's ruling of 2026-09-28 is that all three print glue face UP —
//! the 180° flip. `Round 50` and `Round 100` came out at identity only
//! because the orientation funnel never measured the flip (`plinth-orient`'s
//! `search::escort`, flat-face poses) and, on `Round 100`, because the old
//! mean-based `detail` read the ribs as detail. The featureless glue face is
//! the display face; no detail measure can see that positively, and none
//! should push against it.
//!
//! # Batch-first
//!
//! Repo rule D-37.  One face-geometry pass; one topology build; one shared
//! angle-defect accumulation; one geometry-kernel index build; one batched ray
//! cast **per view direction** (whole-mesh arrays, `DISPLAY_VIEW_DIRECTIONS`
//! calls, never one call per face); and one surface-neighbourhood pass that
//! reuses a single scratch frontier across faces.  There is no per-face
//! spatial query and no per-face kernel call anywhere in this file.  Every
//! fold is serial in source index order.

use std::ops::Range;

use geom_kernel::perf::{self, Span, Work};
use geom_kernel::vec3::{cross, dot, norm, sub};
use geom_kernel::{NativeSurface, NativeSurfaceConfig, V3};

use crate::delicacy::FaceTable;
use crate::surface_par::ScratchPool;
use crate::delicacy_eval::{welded_angle_defect, FaceGeometryPass, WeldedAngleDefect};
use crate::display::{
    DisplayBuildConfig, DisplayError, DisplayFaceValue, DisplayField, DisplaySettings,
};
use crate::Mesh;

/// Builds the display field for `mesh` at the named LOD.
///
/// `lod` is bound into the returned field's `FaceTable` alongside the mesh
/// fingerprint, so a field can never be read against a mesh or a LOD it was
/// not measured on.
///
/// # Errors
///
/// [`DisplayError::Topology`] when the mesh cannot be welded;
/// [`DisplayError::IndexBudgetExceeded`] when the shared kernel's conservative,
/// allocation-free memory estimate exceeds `config.max_index_bytes` — measured
/// before a single index buffer is allocated and never reduced to fit;
/// [`DisplayError::Kernel`] when the kernel refuses the mesh or a batch;
/// [`DisplayError::InvalidResourceLimit`] on an unusable guard; and
/// [`DisplayError::InvalidFaceValue`] if a computed factor escapes `[0, 1]` —
/// a bug in this module, reported rather than clamped away.
pub fn evaluate_display(
    mesh: &Mesh,
    lod: impl Into<String>,
    settings: DisplaySettings,
    config: DisplayBuildConfig,
) -> Result<DisplayField, DisplayError> {
    config.validate()?;
    let table = FaceTable::bind(mesh, lod).map_err(DisplayError::Delicacy)?;
    let face_count = table.face_count();
    if face_count == 0 {
        return DisplayField::from_face_values(table, settings, Vec::new());
    }

    // Write-only stage marks (X0, 2026-10-10), beside the
    // `display.neighbourhood` span. Inert unless `--perf` is on.
    let geometry_mark = perf::stage("orient.display.geometry");
    let geometry = FaceGeometryPass::run(mesh);
    drop(geometry_mark);
    let visibility_mark = perf::stage("orient.display.visibility");
    let visibility = visibility_by_face(mesh, &geometry, settings, config)?;
    drop(visibility_mark);
    let welded_mark = perf::stage("orient.display.welded_folds");
    let WeldedAngleDefect {
        topology,
        defect,
        unusable,
    } = welded_angle_defect(mesh, &geometry).map_err(DisplayError::Topology)?;
    let curvature_by_face = curvature_by_face(mesh, &geometry, &topology, &defect, &unusable);
    let folds = fold_pass(mesh, &geometry, &topology);
    drop(welded_mark);
    let neighbourhood = neighbourhood_pass(
        mesh,
        &geometry,
        &topology,
        &folds,
        &curvature_by_face,
        settings,
    )?;

    let mut values = Vec::with_capacity(face_count);
    for (face, &visible) in visibility.iter().enumerate() {
        if geometry.degenerate[face] {
            values.push(DisplayFaceValue::new(0.0, 0.0, 1.0, 0.0)?);
            continue;
        }
        let detail = neighbourhood.detail[face].clamp(0.0, 1.0);
        // The tip test: Gauss-Bonnet, normalized by the total curvature of a
        // hemisphere. Negative total curvature (a saddle field) is not a tip
        // and is not a reason to INFLATE D either, so the ratio is clamped at
        // both ends before it is subtracted.
        let closed_over = (neighbourhood.curvature[face] / std::f64::consts::TAU).clamp(0.0, 1.0);
        let tip_factor = (1.0 - closed_over).clamp(0.0, 1.0);
        let weight = (visible * detail * tip_factor).clamp(0.0, 1.0);
        values.push(DisplayFaceValue::new(visible, detail, tip_factor, weight)?);
    }
    DisplayField::from_face_values(table, settings, values)
}

/// The Fibonacci lattice on the UPPER hemisphere, in the recorded expression
/// order.
///
/// Character for character `plinth_orient::grid::fibonacci_directions`'s
/// expression with `z` restricted to `(0, 1]`: `z = 1 − (2i + 1)/(2n)` instead
/// of `1 − (2i + 1)/n`, which is that map's own top half at the same
/// generating angle `phi = i·golden`.  It is spelled here rather than imported
/// because `plinth-mesh` is BELOW `plinth-orient` in the dependency order; the
/// duplication is one line of arithmetic and is pinned by a test that checks
/// unit length, a strictly positive `z`, and the closed-form cosine sum.
fn hemisphere_directions(count: usize) -> Vec<V3> {
    let n = count as f64;
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    let mut directions = Vec::with_capacity(count);
    for index in 0..count {
        let i = index as f64;
        let z = 1.0 - (2.0 * i + 1.0) / (2.0 * n);
        let phi = i * golden;
        let radius = (1.0 - z * z).max(0.0).sqrt();
        let raw = [radius * phi.cos(), radius * phi.sin(), z];
        let length = norm(raw);
        directions.push(if length > 0.0 {
            [raw[0] / length, raw[1] / length, raw[2] / length]
        } else {
            [0.0, 0.0, 1.0]
        });
    }
    directions
}

/// The visibility factor: one batched, whole-mesh ray cast per view direction.
///
/// # Self-hits need no epsilon
///
/// The ray leaves the face centroid along a direction with `n_f·v > 0`, so it
/// departs its own plane immediately and its own triangle is excluded by FACE
/// IDENTITY, not by a distance tolerance: a hit whose global face id is the
/// originating face is skipped, and every other hit must have `t > 0`.  There
/// is no offset, no bias, and therefore no tuned length anywhere in this
/// query.
///
/// # Undecidable crossings count as OCCLUDED
///
/// `cast_rays_classified` reports per ray how many faces it reached whose
/// Möller determinant landed in the undecidable band
/// (`docs/specs/geometry-adjudication.md` §EXT-2).  A ray with a nonzero tally
/// is treated as blocked and contributes nothing.  The direction is stated
/// rather than assumed convenient: it can only LOWER `D`, and a lower `D`
/// leaves the orientation objective nearer to what it was before this field
/// existed.  Claiming visibility the arithmetic could not establish would be
/// the opposite: an invented protection.
fn visibility_by_face(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
    settings: DisplaySettings,
    config: DisplayBuildConfig,
) -> Result<Vec<f64>, DisplayError> {
    let verts = mesh.verts();
    let tris = mesh.tris();
    let face_count = tris.len();

    // Charge before allocating; a mesh over the cap is refused naming both
    // numbers and the evaluation is never quietly coarsened to fit.
    let estimate =
        NativeSurface::estimate_index_memory_from_indexed(verts, tris, config.index_grid_pitch_mm)
            .map_err(|error| DisplayError::Kernel(error.to_string()))?;
    if estimate.peak_bytes() > config.max_index_bytes {
        return Err(DisplayError::IndexBudgetExceeded {
            charge: estimate.peak_bytes(),
            cap: config.max_index_bytes,
            what: "display visibility query index",
        });
    }
    let surface = NativeSurface::from_indexed(
        verts,
        tris,
        NativeSurfaceConfig {
            grid_pitch_mm: config.index_grid_pitch_mm,
            max_grid_bytes: estimate.max_axis_grid_bytes(),
        },
    )
    .map_err(|error| DisplayError::Kernel(error.to_string()))?;
    let face_ids = surface.face_ids();

    // The ray length is the model's own bounding-box diagonal: the longest
    // straight line that can stay inside the model, so nothing that could
    // occlude the face is out of range and nothing beyond the model is tested.
    // A measurement of the mesh, not a budget.
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for vertex in verts {
        for axis in 0..3 {
            if vertex[axis] < lo[axis] {
                lo[axis] = vertex[axis];
            }
            if vertex[axis] > hi[axis] {
                hi[axis] = vertex[axis];
            }
        }
    }
    let t_max = norm(sub(hi, lo));
    if !(t_max.is_finite() && t_max > 0.0) {
        return Ok(vec![0.0; face_count]);
    }

    let directions = hemisphere_directions(settings.view_directions());
    let denominator: f64 = directions.iter().map(|v| v[2].max(0.0)).sum();
    if !(denominator.is_finite() && denominator > 0.0) {
        return Err(DisplayError::InvalidSetting {
            name: "view direction cosine sum",
            value: denominator,
        });
    }

    let mut numerator = vec![0.0_f64; face_count];
    let mut origins: Vec<V3> = Vec::with_capacity(face_count);
    let mut rays: Vec<V3> = Vec::with_capacity(face_count);
    let mut rows: Vec<usize> = Vec::with_capacity(face_count);
    for direction in &directions {
        origins.clear();
        rays.clear();
        rows.clear();
        for face in 0..face_count {
            if geometry.degenerate[face] {
                continue;
            }
            if dot(geometry.normals[face], *direction) > 0.0 {
                origins.push(geometry.centroids[face]);
                rays.push(*direction);
                rows.push(face);
            }
        }
        if rows.is_empty() {
            continue;
        }
        // The single batched call for this direction: one cast for every face
        // that could see it, not one call per face.
        let cast = surface
            .cast_rays_classified(&origins, &rays, t_max)
            .map_err(|error| DisplayError::Kernel(error.to_string()))?;
        for (row, &face) in rows.iter().enumerate() {
            if cast.near_parallel[row] > 0 {
                continue;
            }
            // `try_from`, not `as`: the kernel's offsets are `i64` and a
            // negative or oversized one is a kernel defect, which must surface
            // as "this ray contributed nothing" rather than as a wrapped index.
            let (Ok(start), Ok(end)) = (
                usize::try_from(cast.offsets[row]),
                usize::try_from(cast.offsets[row + 1]),
            ) else {
                continue;
            };
            let mut occluded = false;
            for hit in start..end {
                let distance = cast.t_all[hit];
                if !distance.is_finite() || distance <= 0.0 {
                    continue;
                }
                let internal = cast.face_all[hit];
                if internal < 0 {
                    continue;
                }
                let Some(&global) = face_ids.get(usize::try_from(internal).unwrap_or(usize::MAX))
                else {
                    continue;
                };
                if global as usize == face {
                    continue;
                }
                occluded = true;
                break;
            }
            if !occluded {
                numerator[face] += dot(geometry.normals[face], *direction);
            }
        }
    }

    Ok(numerator
        .into_iter()
        .map(|value| (value / denominator).clamp(0.0, 1.0))
        .collect())
}

/// Per-face LUMPED Gaussian curvature: each welded vertex's SIGNED angle
/// defect divided equally among the faces incident to it.
///
/// The lumping is an exact partition, so summing this array over any face set
/// that contains all of a vertex's incident faces recovers that vertex's whole
/// defect — which is what makes the neighbourhood sum below a discrete
/// Gauss-Bonnet integral rather than an approximation of one.  A vertex whose
/// incident faces do not close one manifold disc contributes `0`, the same
/// exclusion [`crate::delicacy_eval`]'s curvature proxy makes and for the same
/// reason: its defect is an artifact of the cut, not a measurement of shape.
///
/// The SIGN is kept.  That is the whole difference from delicacy's reading of
/// the same accumulation, which takes the absolute value: a bump and a dimple
/// are equally fragile, and they are opposites here.
fn curvature_by_face(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
    topology: &crate::FaceTopology,
    defect: &[f64],
    unusable: &[bool],
) -> Vec<f64> {
    let welded_of_source = topology.source_vertex_to_welded();
    let tris = mesh.tris();
    let mut valence = vec![0_u32; defect.len()];
    for (face, triangle) in tris.iter().enumerate() {
        if geometry.degenerate[face] {
            continue;
        }
        for corner in triangle {
            valence[welded_of_source[*corner as usize] as usize] += 1;
        }
    }
    let mut by_face = Vec::with_capacity(tris.len());
    for (face, triangle) in tris.iter().enumerate() {
        if geometry.degenerate[face] {
            by_face.push(0.0);
            continue;
        }
        let mut total = 0.0_f64;
        for corner in triangle {
            let vertex = welded_of_source[*corner as usize] as usize;
            if unusable[vertex] || valence[vertex] == 0 || !defect[vertex].is_finite() {
                continue;
            }
            total += defect[vertex] / f64::from(valence[vertex]);
        }
        by_face.push(total);
    }
    by_face
}

struct Neighbourhood {
    detail: Vec<f64>,
    curvature: Vec<f64>,
}

/// Per welded face adjacency, slot-aligned with
/// [`crate::FaceTopology::indices`]: the FOLD angle between the two faces'
/// normals and the length of the edge they share. One pass, before any
/// neighbourhood is walked, so each fold is measured once and every
/// neighbourhood that contains it reads the same number.
struct Folds {
    angle: Vec<f64>,
    length: Vec<f64>,
}

/// Measures every fold once. The angle is `atan2(|n × m|, n · m)`, which stays
/// accurate at the small angles `acos` loses; the shared edge is found by
/// welded vertex identity, so a fold between two faces that meet only at a
/// point (no shared edge) has zero length and carries nothing.
fn fold_pass(mesh: &Mesh, geometry: &FaceGeometryPass, topology: &crate::FaceTopology) -> Folds {
    let welded = topology.source_vertex_to_welded();
    let indptr = topology.indptr();
    let indices = topology.indices();
    let tris = mesh.tris();
    let verts = mesh.verts();
    let mut angle = vec![0.0_f64; indices.len()];
    let mut length = vec![0.0_f64; indices.len()];
    for (face, corners) in tris.iter().enumerate() {
        let (Some(&start), Some(&end)) = (indptr.get(face), indptr.get(face + 1)) else {
            continue;
        };
        if geometry.degenerate[face] {
            continue;
        }
        let welded_corners = corners.map(|corner| welded[corner as usize]);
        for slot in start as usize..end as usize {
            let other = indices[slot] as usize;
            if other >= tris.len() || geometry.degenerate[other] {
                continue;
            }
            let other_welded = tris[other].map(|corner| welded[corner as usize]);
            let mut shared = [0_usize; 2];
            let mut count = 0_usize;
            for (position, id) in welded_corners.iter().enumerate() {
                if count < 2 && other_welded.contains(id) {
                    shared[count] = position;
                    count += 1;
                }
            }
            if count < 2 {
                continue;
            }
            let a = verts[corners[shared[0]] as usize];
            let b = verts[corners[shared[1]] as usize];
            length[slot] = norm(sub(b, a));
            let n = geometry.normals[face];
            let m = geometry.normals[other];
            angle[slot] = norm(cross(n, m)).atan2(dot(n, m));
        }
    }
    Folds { angle, length }
}

/// The SPREAD of a set of fold angles, each weighted by the normal change it
/// carries: `1 − GM/AM`, where GM and AM are the weighted geometric and
/// arithmetic means of the angles.
///
/// * By the AM–GM inequality it lies in `[0, 1)`, and it is `0` exactly when
///   every fold has the same angle — a crease, a regular rib, a regular
///   groove, a uniformly faceted cylinder: CHANGE, not detail (owner ruling
///   2026-09-05). It rises toward `1` as the angles spread over orders of
///   magnitude — small, medium and large together.
/// * It is `1 − exp(−L)`, where `L = ln AM − mean(ln θ)` is the mean log
///   deviation (Theil's L), the standard scale-free spread of a positive
///   quantity: multiplying every angle by the same factor leaves it
///   unchanged, so a sharp regular rib and a shallow regular rib are equally
///   NOT detail. For log-normally spread angles it is `1 − exp(−σ²/2)`.
/// * The weight is `length × angle`, the normal change the fold carries (the
///   discrete integral of the surface's turning along it). A flat edge
///   carries none and so is not in the distribution at all — which is what
///   lets a crease on a flat face read as ONE magnitude rather than as a
///   0-or-90° mixture.
///
/// `sums` is `(Σw, Σw·θ, Σw·ln θ, min θ, max θ)`. When every fold holds the
/// same angle the answer is `0` by definition, and it is returned as exactly
/// `0` rather than as whatever last-bit residue `exp(ln θ)/θ` leaves.
fn spread_of_folds(sums: (f64, f64, f64, f64, f64)) -> f64 {
    let (weight, weighted_angle, weighted_log, smallest, largest) = sums;
    if weight <= 0.0 || smallest >= largest {
        return 0.0;
    }
    let arithmetic = weighted_angle / weight;
    let geometric = (weighted_log / weight).exp();
    if arithmetic <= 0.0 || !arithmetic.is_finite() || !geometric.is_finite() {
        return 0.0;
    }
    (1.0 - geometric / arithmetic).clamp(0.0, 1.0)
}

/// One surface-neighbourhood pass: the detail (the spread of the fold angles)
/// and the total Gaussian curvature of every face's `R_detail` neighbourhood,
/// in a single traversal per face over a shared scratch frontier.
///
/// # Which folds a neighbourhood holds
///
/// Every welded edge BOTH of whose faces are in the neighbourhood, counted
/// once, whose fold angle `θ` exceeds the change resolution
/// ([`DisplaySettings::change_resolution_rad`], `atan(0.019 mm / R_detail)` =
/// 1.36°): a fold whose excursion across the whole neighbourhood stays inside
/// one addressable printer step — the deviation the decimated mesh is already
/// authorized to carry — is indistinguishable from tessellation or
/// decimation noise and is not a change. Without that bound a flat but
/// non-axis-aligned plane, whose coplanar triangles fold by float noise alone,
/// would read its noise as a maximally wide distribution; the scale-free
/// statistic cannot tell `1e-8` from `1e-1` rad, so the resolution has to be
/// stated, and it is inherited rather than chosen.
///
/// # The neighbourhood is SURFACE-restricted, and the radius is Euclidean
///
/// Expansion follows the welded face adjacency
/// ([`crate::FaceTopology::neighbors`]) and the radius test is the Euclidean
/// distance between face centroids.  That combination is deliberate: a purely
/// Euclidean ball would bleed through a thin wall and read the OPPOSITE side
/// of a fin as this face's neighbourhood, which is a thinness measurement
/// (delicacy already owns that) and not a detail measurement.  Restricting
/// expansion to the surface graph makes the bleed inexpressible; using the
/// straight-line distance as the radius keeps the test exact and cheap.  It is
/// a geodesic UPPER BOUND, so the neighbourhood is never larger than the
/// stated radius asks for.
///
/// # The 1-ring is always included
///
/// On a mesh whose triangles are larger than `R_detail` — a 25 mm disc with 96
/// segments, a search proxy at 2 % of the production face count — a radius
/// test alone would return the face itself and measure nothing.  The
/// edge-adjacent ring is therefore always in the neighbourhood, so the answer
/// degrades to the immediate dihedral rather than to zero.  That is a
/// statement about mesh density, not a tolerance: on a production mesh the
/// radius binds and the 1-ring is a subset of it.  **It also means that on a
/// mesh coarser than `R_detail` this field measures TESSELLATION rather than
/// detail**; the committed orientation score is computed at the Exact LOD, and
/// the published per-model summary is what makes a coarse reading visible
/// rather than silent.
///
/// # A face larger than `R_detail` counts only its border band
///
/// The detail term is defined per unit area at the perceptual scale
/// `R_detail`.  A point deeper than `R_detail` inside a flat triangle sees
/// nothing but its own plane, so it carries no detail whatever its
/// neighbours do; only the band within `R_detail` of the face's edges can
/// see across them.  The neighbourhood's spread is therefore scaled by the
/// band fraction `min(1, perimeter x R_detail / area)`, which is exactly 1 on
/// any triangle smaller than the scale (a production mesh) and falls as a
/// triangle grows past it.  Without this a 20 mm slab top made of two
/// triangles read as articulated surface because its only neighbours were its
/// own side walls, and that spurious detail on a large area out-voted a
/// genuinely delicate blade authored on the underside (make_raking_blade,
/// 2026-09-04).  No new constant: the band is the radius the term already has.
///
/// # Parallel, and bit-identical to the serial walk (2026-10-09)
///
/// The pass is a GATHER: face `f` writes only `detail[f]` and `curvature[f]`,
/// and every sum inside a walk is formed in that walk's own frontier order,
/// which depends only on the topology.  So fixed 1024-face blocks run as an
/// index-parallel map-collect and are concatenated in block order.  A block
/// takes pooled scratch (stamps, frontier, member list) and the stamps are
/// keyed by the GLOBAL face index, which is unique across the run, so a stamp
/// another block or worker left behind never equals the current seed.  The
/// serial body is kept as `neighbourhood_pass_serial`; every test compares
/// against it bit for bit.
fn neighbourhood_pass(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
    topology: &crate::FaceTopology,
    folds: &Folds,
    curvature_by_face: &[f64],
    settings: DisplaySettings,
) -> Result<Neighbourhood, DisplayError> {
    let _perf_span = perf::span(Span::DisplayPass);
    let band_fraction = border_band_fraction(mesh, geometry, settings.detail_radius_mm());
    let inputs = NeighbourhoodInputs {
        geometry,
        topology,
        folds,
        curvature_by_face,
        band_fraction: &band_fraction,
        radius_sq: settings.detail_radius_mm() * settings.detail_radius_mm(),
        resolution: settings.change_resolution_rad(),
    };
    let (neighbourhood, counts) =
        geom_kernel::run_in_pool(|| {
            neighbourhood_blocks::<ExactWalk>(&inputs, NEIGHBOURHOOD_BLOCK_FACES)
        })
        .map_err(|error| DisplayError::Kernel(error.to_string()))?;
    if perf::enabled() {
        perf::add(Work::DisplayFaces, perf::count(counts.faces_walked));
        perf::add(Work::DisplayPairsMeasured, perf::count(counts.pairs_measured));
        perf::add(Work::DisplayMembersTotal, perf::count(counts.members_total));
    }
    Ok(neighbourhood)
}

/// Faces per block of the neighbourhood pass (execution only, never a value).
const NEIGHBOURHOOD_BLOCK_FACES: usize = 1024;

/// The flat, read-only view every neighbourhood walk shares.
struct NeighbourhoodInputs<'a> {
    geometry: &'a FaceGeometryPass,
    topology: &'a crate::FaceTopology,
    folds: &'a Folds,
    curvature_by_face: &'a [f64],
    band_fraction: &'a [f64],
    radius_sq: f64,
    resolution: f64,
}

/// What the walks did, counted (path hits), summed over blocks as integers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct WalkCounts {
    /// Non-degenerate faces whose neighbourhood was walked.
    faces_walked: usize,
    /// Frontier entries examined across all walks.
    pairs_measured: usize,
    /// Faces found inside their seed's neighbourhood, across all walks. Every
    /// walk has its own face as a member, so this exceeds `faces_walked`
    /// exactly when some walk expanded beyond its seed.
    members_total: usize,
}

/// One worker's reusable walk state.
struct WalkScratch {
    stamp: Vec<u32>,
    inside_stamp: Vec<u32>,
    frontier: Vec<u32>,
    visited: Vec<u32>,
    members: Vec<usize>,
}

/// One block's per-face results, in face order.
struct NeighbourhoodBlock {
    detail: Vec<f64>,
    curvature: Vec<f64>,
    counts: WalkCounts,
}

/// The seed and reset rules of a walk. The production type ([`ExactWalk`]) is
/// the serial rule; the test module implements deliberately broken ones to
/// show the tests can see them.
trait WalkHooks {
    /// The stamp value that marks "visited by this face's walk".
    fn seed(face: usize) -> u32;
    /// Whether the visited stamps are reset after each walk.
    const RESET_STAMPS: bool = true;
    /// Whether the expansion loop skips a face's first neighbour.
    const SKIP_FIRST_NEIGHBOUR: bool = false;
}

/// The production rules: the seed is the global face index, stamps are reset.
struct ExactWalk;
impl WalkHooks for ExactWalk {
    fn seed(face: usize) -> u32 {
        u32::try_from(face).unwrap_or(u32::MAX)
    }
}

/// Runs every block in the ambient pool and concatenates in block order.
fn neighbourhood_blocks<W: WalkHooks + Sync>(
    inp: &NeighbourhoodInputs<'_>,
    block_faces: usize,
) -> (Neighbourhood, WalkCounts) {
    use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

    assert!(block_faces >= 1, "a block holds at least one face");
    let face_count = inp.geometry.normals.len();
    let pool = ScratchPool::<WalkScratch>::new();
    let blocks: Vec<Range<usize>> = (0..face_count)
        .step_by(block_faces)
        .map(|start| start..face_count.min(start + block_faces))
        .collect();
    let results: Vec<NeighbourhoodBlock> = blocks
        .par_iter()
        .map(|block| {
            let mut scratch = pool.take(|| WalkScratch {
                stamp: vec![u32::MAX; face_count],
                inside_stamp: vec![u32::MAX; face_count],
                frontier: Vec::new(),
                visited: Vec::new(),
                members: Vec::new(),
            });
            let out = neighbourhood_block::<W>(inp, block.clone(), &mut scratch);
            pool.put(scratch);
            out
        })
        .collect();
    let mut detail = Vec::with_capacity(face_count);
    let mut curvature = Vec::with_capacity(face_count);
    let mut counts = WalkCounts::default();
    for block in results {
        detail.extend_from_slice(&block.detail);
        curvature.extend_from_slice(&block.curvature);
        counts.faces_walked += block.counts.faces_walked;
        counts.pairs_measured += block.counts.pairs_measured;
        counts.members_total += block.counts.members_total;
    }
    (Neighbourhood { detail, curvature }, counts)
}

/// The per-face walk of the serial body, verbatim, for the faces of `faces`.
fn neighbourhood_block<W: WalkHooks>(
    inp: &NeighbourhoodInputs<'_>,
    faces: Range<usize>,
    scratch: &mut WalkScratch,
) -> NeighbourhoodBlock {
    let geometry = inp.geometry;
    let topology = inp.topology;
    let folds = inp.folds;
    let indptr = topology.indptr();
    let indices = topology.indices();
    let WalkScratch {
        stamp,
        inside_stamp,
        frontier,
        visited,
        members,
    } = scratch;
    let mut detail = Vec::with_capacity(faces.len());
    let mut curvature = Vec::with_capacity(faces.len());
    let mut counts = WalkCounts::default();

    for face in faces {
        if geometry.degenerate[face] {
            detail.push(0.0);
            curvature.push(0.0);
            continue;
        }
        let seed = W::seed(face);
        let origin = geometry.centroids[face];
        let ring = topology.neighbors(face).unwrap_or(&[]);

        frontier.clear();
        visited.clear();
        members.clear();
        let own = u32::try_from(face).unwrap_or(u32::MAX);
        frontier.push(own);
        stamp[face] = seed;
        visited.push(own);

        let mut curvature_total = 0.0_f64;

        let mut head = 0_usize;
        while head < frontier.len() {
            let current = frontier[head] as usize;
            head += 1;
            let inside = current == face
                || u32::try_from(current).is_ok_and(|index| ring.contains(&index))
                || {
                    let offset = sub(geometry.centroids[current], origin);
                    dot(offset, offset) <= inp.radius_sq
                };
            if !inside {
                continue;
            }
            inside_stamp[current] = seed;
            members.push(current);
            if !geometry.degenerate[current] {
                curvature_total += inp.curvature_by_face[current];
            }
            // Expand only from faces that are themselves inside; a face outside
            // the radius is neither counted nor walked through.
            if let Some(neighbours) = topology.neighbors(current) {
                let skip = usize::from(W::SKIP_FIRST_NEIGHBOUR);
                for &next in neighbours.iter().skip(skip) {
                    if stamp[next as usize] == seed {
                        continue;
                    }
                    stamp[next as usize] = seed;
                    visited.push(next);
                    frontier.push(next);
                }
            }
        }

        counts.faces_walked += 1;
        counts.pairs_measured += frontier.len();
        counts.members_total += members.len();

        // The folds wholly inside the neighbourhood, each counted once (from
        // its lower-indexed face), above the change resolution.
        let mut sums = (0.0_f64, 0.0_f64, 0.0_f64, f64::INFINITY, 0.0_f64);
        for &member in members.iter() {
            let (Some(&start), Some(&end)) = (indptr.get(member), indptr.get(member + 1)) else {
                continue;
            };
            let range = start as usize..end as usize;
            for ((&other, &angle), &length) in indices[range.clone()]
                .iter()
                .zip(&folds.angle[range.clone()])
                .zip(&folds.length[range])
            {
                let other = other as usize;
                if other <= member || inside_stamp.get(other) != Some(&seed) {
                    continue;
                }
                if angle <= inp.resolution {
                    continue;
                }
                let weight = length * angle;
                sums.0 += weight;
                sums.1 += weight * angle;
                sums.2 += weight * angle.ln();
                sums.3 = sums.3.min(angle);
                sums.4 = sums.4.max(angle);
            }
        }
        detail.push(spread_of_folds(sums) * inp.band_fraction[face]);
        curvature.push(curvature_total);
        if W::RESET_STAMPS {
            for &touched in visited.iter() {
                stamp[touched as usize] = u32::MAX;
            }
        }
    }
    NeighbourhoodBlock {
        detail,
        curvature,
        counts,
    }
}

/// Both neighbourhood passes on one mesh, for the witness example
/// (`surface_oracle`).
#[cfg(feature = "serial-oracle")]
pub(crate) struct NeighbourhoodComparison {
    pub(crate) parallel_detail: Vec<f64>,
    pub(crate) parallel_curvature: Vec<f64>,
    pub(crate) serial_detail: Vec<f64>,
    pub(crate) serial_curvature: Vec<f64>,
    pub(crate) faces_walked: usize,
    pub(crate) members_total: usize,
    pub(crate) non_degenerate_faces: usize,
}

/// Runs the display field's own setup, then the parallel and the serial
/// neighbourhood pass over it.
#[cfg(feature = "serial-oracle")]
pub(crate) fn compare_neighbourhood_passes(
    mesh: &Mesh,
) -> Result<NeighbourhoodComparison, DisplayError> {
    let settings = DisplaySettings::production()?;
    let geometry = FaceGeometryPass::run(mesh);
    let WeldedAngleDefect {
        topology,
        defect,
        unusable,
    } = welded_angle_defect(mesh, &geometry).map_err(DisplayError::Topology)?;
    let curvature = curvature_by_face(mesh, &geometry, &topology, &defect, &unusable);
    let folds = fold_pass(mesh, &geometry, &topology);
    let serial = neighbourhood_pass_serial(mesh, &geometry, &topology, &folds, &curvature, settings);
    let band_fraction = border_band_fraction(mesh, &geometry, settings.detail_radius_mm());
    let inputs = NeighbourhoodInputs {
        geometry: &geometry,
        topology: &topology,
        folds: &folds,
        curvature_by_face: &curvature,
        band_fraction: &band_fraction,
        radius_sq: settings.detail_radius_mm() * settings.detail_radius_mm(),
        resolution: settings.change_resolution_rad(),
    };
    let (parallel, counts) = geom_kernel::run_in_pool(|| {
        neighbourhood_blocks::<ExactWalk>(&inputs, NEIGHBOURHOOD_BLOCK_FACES)
    })
    .map_err(|error| DisplayError::Kernel(error.to_string()))?;
    Ok(NeighbourhoodComparison {
        parallel_detail: parallel.detail,
        parallel_curvature: parallel.curvature,
        serial_detail: serial.detail,
        serial_curvature: serial.curvature,
        faces_walked: counts.faces_walked,
        members_total: counts.members_total,
        non_degenerate_faces: geometry.degenerate.iter().filter(|d| !**d).count(),
    })
}

/// The serial pass exactly as it stood before the parallel one (2026-10-09):
/// frozen former production code, the test oracle and the witness example's
/// reference. Counters are not charged.
#[cfg(any(test, feature = "serial-oracle"))]
fn neighbourhood_pass_serial(
    mesh: &Mesh,
    geometry: &FaceGeometryPass,
    topology: &crate::FaceTopology,
    folds: &Folds,
    curvature_by_face: &[f64],
    settings: DisplaySettings,
) -> Neighbourhood {
    let face_count = geometry.normals.len();
    let radius_sq = settings.detail_radius_mm() * settings.detail_radius_mm();
    let resolution = settings.change_resolution_rad();
    let indptr = topology.indptr();
    let indices = topology.indices();

    let mut detail = vec![0.0_f64; face_count];
    let mut curvature = vec![0.0_f64; face_count];
    let band_fraction = border_band_fraction(mesh, geometry, settings.detail_radius_mm());

    // Shared scratch, reused across faces: a visited stamp keyed by the source
    // face index (so it is cleared in O(visited), never O(faces)), an INSIDE
    // stamp keyed the same way (never cleared: a stamp equal to this seed can
    // only have been written during this seed's walk), one frontier buffer
    // and one list of the faces found inside.
    let mut stamp = vec![u32::MAX; face_count];
    let mut inside_stamp = vec![u32::MAX; face_count];
    let mut frontier: Vec<u32> = Vec::new();
    let mut visited: Vec<u32> = Vec::new();
    let mut members: Vec<usize> = Vec::new();

    for face in 0..face_count {
        if geometry.degenerate[face] {
            continue;
        }
        let seed = u32::try_from(face).unwrap_or(u32::MAX);
        let origin = geometry.centroids[face];
        let ring = topology.neighbors(face).unwrap_or(&[]);

        frontier.clear();
        visited.clear();
        members.clear();
        frontier.push(seed);
        stamp[face] = seed;
        visited.push(seed);

        let mut curvature_total = 0.0_f64;

        let mut head = 0_usize;
        while head < frontier.len() {
            let current = frontier[head] as usize;
            head += 1;
            let inside = current == face
                || u32::try_from(current).is_ok_and(|index| ring.contains(&index))
                || {
                    let offset = sub(geometry.centroids[current], origin);
                    dot(offset, offset) <= radius_sq
                };
            if !inside {
                continue;
            }
            inside_stamp[current] = seed;
            members.push(current);
            if !geometry.degenerate[current] {
                curvature_total += curvature_by_face[current];
            }
            // Expand only from faces that are themselves inside; a face outside
            // the radius is neither counted nor walked through.
            if let Some(neighbours) = topology.neighbors(current) {
                for &next in neighbours {
                    if stamp[next as usize] == seed {
                        continue;
                    }
                    stamp[next as usize] = seed;
                    visited.push(next);
                    frontier.push(next);
                }
            }
        }

        // The folds wholly inside the neighbourhood, each counted once (from
        // its lower-indexed face), above the change resolution.
        let mut sums = (0.0_f64, 0.0_f64, 0.0_f64, f64::INFINITY, 0.0_f64);
        for &member in &members {
            let (Some(&start), Some(&end)) = (indptr.get(member), indptr.get(member + 1)) else {
                continue;
            };
            let range = start as usize..end as usize;
            for ((&other, &angle), &length) in indices[range.clone()]
                .iter()
                .zip(&folds.angle[range.clone()])
                .zip(&folds.length[range])
            {
                let other = other as usize;
                if other <= member || inside_stamp.get(other) != Some(&seed) {
                    continue;
                }
                if angle <= resolution {
                    continue;
                }
                let weight = length * angle;
                sums.0 += weight;
                sums.1 += weight * angle;
                sums.2 += weight * angle.ln();
                sums.3 = sums.3.min(angle);
                sums.4 = sums.4.max(angle);
            }
        }
        detail[face] = spread_of_folds(sums) * band_fraction[face];
        curvature[face] = curvature_total;
        for &touched in &visited {
            stamp[touched as usize] = u32::MAX;
        }
    }

    Neighbourhood { detail, curvature }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{make_box_default, make_recessed_disc};

    fn settings() -> DisplaySettings {
        DisplaySettings::production().expect("production display settings")
    }
    fn config() -> DisplayBuildConfig {
        DisplayBuildConfig::production(1_000_000).expect("production display config")
    }

    #[test]
    fn the_hemisphere_lattice_is_unit_length_and_strictly_above_the_equator() {
        let directions = hemisphere_directions(64);
        assert_eq!(directions.len(), 64);
        for direction in &directions {
            assert!((norm(*direction) - 1.0).abs() < 1e-12);
            assert!(direction[2] > 0.0, "a view direction fell on or below the equator");
        }
        // The lattice's z values are the midpoints of 64 equal slabs of the
        // upper half, so their sum is exactly 64/2 = 32 in closed form.
        let sum: f64 = directions.iter().map(|d| d[2]).sum();
        assert!((sum - 32.0).abs() < 1e-9, "cosine sum was {sum}");
    }

    /// A closed box: the top face is unoccluded from the whole upper
    /// hemisphere and the bottom face is seen from none of it.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "the claim IS exact zero: `max(0, n·v)` is zero for every upper-hemisphere `v`                   when `n` points straight down, so a tolerance would pass on a field that had                   stopped producing it by construction"
    )]
    fn a_closed_box_sees_its_top_and_never_its_bottom() {
        let mesh = make_box_default().expect("box");
        let field = evaluate_display(&mesh, "l0", settings(), config()).expect("display field");
        let geometry = FaceGeometryPass::run(&mesh);
        let mut top = 0.0_f64;
        let mut bottom = 0.0_f64;
        for (face, value) in field.face_values().iter().enumerate() {
            if geometry.normals[face][2] > 0.9 {
                top = top.max(value.visibility());
            }
            if geometry.normals[face][2] < -0.9 {
                bottom = bottom.max(value.visibility());
            }
        }
        assert!(top > 0.9, "the box's top face visibility was {top}");
        assert_eq!(bottom, 0.0, "the box's bottom face was visible from above");
    }

    /// Every published factor stays inside its contract on real geometry, and
    /// the field is reproducible.
    #[test]
    fn every_factor_is_in_range_and_the_field_is_reproducible() {
        let mesh = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 48).expect("disc");
        let first = evaluate_display(&mesh, "l0", settings(), config()).expect("display field");
        let second = evaluate_display(&mesh, "l0", settings(), config()).expect("display field");
        assert_eq!(first.digest(), second.digest());
        for value in first.face_values() {
            for component in [
                value.visibility(),
                value.detail(),
                value.tip_factor(),
                value.weight(),
            ] {
                assert!((0.0..=1.0).contains(&component));
            }
            assert!(
                (value.weight() - value.visibility() * value.detail() * value.tip_factor()).abs()
                    < 1e-12
            );
        }
    }
}

/// Fraction of each face's area that lies within `radius_mm` of one of its
/// own edges, bounded above by the strip estimate `perimeter x radius / area`
/// and clamped to `[0, 1]`. Exactly 1 for every triangle whose inradius is
/// below `radius_mm`, which is every triangle of a production-density mesh.
fn border_band_fraction(mesh: &Mesh, geometry: &FaceGeometryPass, radius_mm: f64) -> Vec<f64> {
    let verts = mesh.verts();
    mesh.tris()
        .iter()
        .enumerate()
        .map(|(face, triangle)| {
            if geometry.degenerate[face] || geometry.areas_mm2[face] <= 0.0 {
                return 0.0;
            }
            let a = verts[triangle[0] as usize];
            let b = verts[triangle[1] as usize];
            let c = verts[triangle[2] as usize];
            let perimeter = norm(sub(b, a)) + norm(sub(c, b)) + norm(sub(a, c));
            (perimeter * radius_mm / geometry.areas_mm2[face]).clamp(0.0, 1.0)
        })
        .collect()
}

#[cfg(test)]
mod band_tests {
    use super::*;

    /// A 10 mm block whose top face borders one VERTICAL wall and one wall
    /// sheared 3 mm in `y`, so a top triangle's neighbourhood holds two
    /// different fold angles. (A plain box's top holds only 90° folds, which
    /// is one magnitude — change, not detail, since 2026-09-28 — and would
    /// score 0 at every size, proving nothing about size.)
    fn sheared_block() -> Mesh {
        let verts = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 3.0, 10.0],
            [10.0, 0.0, 10.0],
            [10.0, 10.0, 10.0],
            [0.0, 10.0, 10.0],
        ];
        let tris = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        Mesh::new(verts, tris).expect("sheared block")
    }

    /// **The same flat face is less "detail" the larger it is.** A block and
    /// the same block scaled ten times share every fold angle, so under a
    /// per-triangle reading they would score alike; but a 10x face is mostly
    /// interior that sees nothing beyond its own plane, and its top must read
    /// well below the small block's. This is the make_raking_blade defect of
    /// 2026-09-04 in miniature: a two-triangle slab top scoring as carving.
    #[test]
    fn a_large_flat_face_carries_less_detail_than_the_same_face_small() {
        let small = sheared_block();
        let big = Mesh::new(
            small.verts().iter().map(|v| [v[0] * 10.0, v[1] * 10.0, v[2] * 10.0]).collect(),
            small.tris().to_vec(),
        )
        .expect("scaled block");
        let settings = DisplaySettings::production().expect("display settings");
        let config = DisplayBuildConfig::production(1_000_000).expect("display config");
        let top_detail = |mesh: &Mesh| {
            let field = evaluate_display(mesh, "l0", settings, config).expect("display field");
            let geometry = FaceGeometryPass::run(mesh);
            field
                .face_values()
                .iter()
                .enumerate()
                .filter(|(face, _)| geometry.normals[*face][2] > 0.9)
                .map(|(_, value)| value.detail())
                .fold(0.0_f64, f64::max)
        };
        let (small_top, big_top) = (top_detail(&small), top_detail(&big));
        println!("block top detail: small {small_top:.6}, 10x {big_top:.6}");
        assert!(
            small_top > 0.0,
            "the small block's top must see its two different wall folds"
        );
        assert!(
            big_top < 0.5 * small_top,
            "10x block top detail {big_top:.6} is not well below the small block's \
             {small_top:.6}; a face far larger than the detail radius is being read as \
             articulated surface"
        );
    }
}

/// The parallel neighbourhood pass against its frozen serial oracle
/// (2026-10-09), with the sabotages that show the tests can see a broken one.
#[cfg(test)]
mod walk_tests {
    use super::*;
    use crate::delicacy_eval::tests::gridded_cube;
    use crate::{make_recessed_disc, make_thin_fin};

    /// A sabotage: every face uses seed 0 AND the visited stamps are never
    /// reset, so a later face's walk finds its neighbours already stamped and
    /// does not push them (members shrink), and a stale inside-stamp admits
    /// folds from outside the neighbourhood. This is the hazard a parallel
    /// pass must avoid: a reused seed with pooled scratch.
    struct CollideSeeds;
    impl WalkHooks for CollideSeeds {
        fn seed(_face: usize) -> u32 {
            0
        }
        const RESET_STAMPS: bool = false;
    }

    /// A sabotage: the expansion loop skips each face's first neighbour, so an
    /// eligible face is never visited.
    struct DropFirstNeighbour;
    impl WalkHooks for DropFirstNeighbour {
        fn seed(face: usize) -> u32 {
            ExactWalk::seed(face)
        }
        const SKIP_FIRST_NEIGHBOUR: bool = true;
    }

    /// A positive control: only the stamp reset is skipped, with unique global
    /// seeds. A stale stamp can never equal a later seed, so the bits must NOT
    /// move; this is what lets pooled scratch go uncleaned between blocks.
    struct SkipResetOnly;
    impl WalkHooks for SkipResetOnly {
        fn seed(face: usize) -> u32 {
            ExactWalk::seed(face)
        }
        const RESET_STAMPS: bool = false;
    }

    /// Everything a pass reads, built the way `evaluate_display` builds it.
    struct Prepared {
        geometry: FaceGeometryPass,
        topology: crate::FaceTopology,
        folds: Folds,
        curvature: Vec<f64>,
        band_fraction: Vec<f64>,
    }

    fn settings() -> DisplaySettings {
        DisplaySettings::production().expect("display settings")
    }

    fn prepare(mesh: &Mesh) -> Prepared {
        let geometry = FaceGeometryPass::run(mesh);
        let WeldedAngleDefect {
            topology,
            defect,
            unusable,
        } = welded_angle_defect(mesh, &geometry).expect("welded defect");
        let curvature = curvature_by_face(mesh, &geometry, &topology, &defect, &unusable);
        let folds = fold_pass(mesh, &geometry, &topology);
        let band_fraction = border_band_fraction(mesh, &geometry, settings().detail_radius_mm());
        Prepared {
            geometry,
            topology,
            folds,
            curvature,
            band_fraction,
        }
    }

    impl Prepared {
        fn inputs(&self) -> NeighbourhoodInputs<'_> {
            let radius = settings().detail_radius_mm();
            NeighbourhoodInputs {
                geometry: &self.geometry,
                topology: &self.topology,
                folds: &self.folds,
                curvature_by_face: &self.curvature,
                band_fraction: &self.band_fraction,
                radius_sq: radius * radius,
                resolution: settings().change_resolution_rad(),
            }
        }
    }

    fn in_pool<R: Send>(threads: usize, body: impl FnOnce() -> R + Send) -> R {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("local rayon pool")
            .install(body)
    }

    fn bits(values: &[f64]) -> Vec<u64> {
        values.iter().map(|value| value.to_bits()).collect()
    }

    fn fixtures() -> Vec<(&'static str, Mesh)> {
        vec![
            ("gridded_cube(8)", gridded_cube(8)),
            ("gridded_cube(16)", gridded_cube(16)),
            ("thin_fin", make_thin_fin().expect("thin fin")),
            (
                "recessed_disc",
                make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("recessed disc"),
            ),
        ]
    }

    fn serial_of(mesh: &Mesh, prepared: &Prepared) -> Neighbourhood {
        neighbourhood_pass_serial(
            mesh,
            &prepared.geometry,
            &prepared.topology,
            &prepared.folds,
            &prepared.curvature,
            settings(),
        )
    }

    fn differs(parallel: &Neighbourhood, serial: &Neighbourhood) -> bool {
        bits(&parallel.detail) != bits(&serial.detail)
            || bits(&parallel.curvature) != bits(&serial.curvature)
    }

    /// **The parallel pass is the serial walk, bit for bit**, in pools of 1, 3
    /// and 8 workers and blocks of 1, 7 and 1024 faces, on four fixtures, with
    /// both outputs (detail and neighbourhood curvature) compared by bits.
    #[test]
    fn the_parallel_pass_equals_the_serial_oracle_bit_for_bit() {
        for (name, mesh) in fixtures() {
            let prepared = prepare(&mesh);
            let serial = serial_of(&mesh, &prepared);
            for threads in [1_usize, 3, 4, 8, 16] {
                for block in [1_usize, 7, 1024] {
                    let inputs = prepared.inputs();
                    let (parallel, _) =
                        in_pool(threads, || neighbourhood_blocks::<ExactWalk>(&inputs, block));
                    assert!(
                        !differs(&parallel, &serial),
                        "{name}: threads {threads} block {block}"
                    );
                }
            }
        }
    }

    /// The production entry (`neighbourhood_pass`, kernel pool) equals the
    /// oracle.
    #[test]
    fn the_production_entry_equals_the_serial_oracle() {
        for (name, mesh) in fixtures() {
            let prepared = prepare(&mesh);
            let serial = serial_of(&mesh, &prepared);
            let parallel = neighbourhood_pass(
                &mesh,
                &prepared.geometry,
                &prepared.topology,
                &prepared.folds,
                &prepared.curvature,
                settings(),
            )
            .expect("parallel pass");
            assert!(!differs(&parallel, &serial), "{name}");
        }
    }

    /// Path hits: every non-degenerate face is walked, the walks really
    /// expand (the members of all walks exceed the walks themselves), and the
    /// folds above the change resolution exist so the detail term is not
    /// vacuously zero.
    #[test]
    fn the_walks_expand_and_every_face_is_walked() {
        let mut any_detail = false;
        for (name, mesh) in fixtures() {
            let prepared = prepare(&mesh);
            let inputs = prepared.inputs();
            let (parallel, counts) =
                in_pool(3, || neighbourhood_blocks::<ExactWalk>(&inputs, 7));
            let walkable = prepared.geometry.degenerate.iter().filter(|d| !**d).count();
            assert_eq!(counts.faces_walked, walkable, "{name}");
            assert!(counts.members_total > counts.faces_walked, "{name}: {counts:?}");
            assert!(counts.pairs_measured >= counts.members_total, "{name}: {counts:?}");
            any_detail |= parallel.detail.iter().any(|detail| *detail > 0.0);
        }
        // A cube holds one fold magnitude (change, not detail), so only some
        // fixtures carry detail; the detail term must be non-vacuous overall.
        assert!(any_detail, "no fixture has any detail; the bit comparison would be vacuous");
    }

    /// **Sabotage 1 (colliding seeds).** With every face on seed 0 and no
    /// reset, the walks change (one worker, so the order is fixed): the bits
    /// differ from the oracle on at least one face of every fixture that has
    /// neighbourhoods to share.
    #[test]
    fn sabotage_colliding_seeds_change_the_bits() {
        let mut seen = 0;
        for (name, mesh) in fixtures() {
            let prepared = prepare(&mesh);
            let serial = serial_of(&mesh, &prepared);
            let inputs = prepared.inputs();
            let (broken, _) = in_pool(1, || neighbourhood_blocks::<CollideSeeds>(&inputs, 7));
            if differs(&broken, &serial) {
                seen += 1;
            } else {
                println!("{name}: colliding seeds did not change the bits");
            }
        }
        assert!(seen > 0, "no fixture could see colliding seeds");
    }

    /// **Sabotage 2 (dropped neighbour).** Skipping each face's first
    /// neighbour must change the bits on at least one face.
    #[test]
    fn sabotage_dropping_the_first_neighbour_changes_the_bits() {
        let mut seen = 0;
        for (_, mesh) in fixtures() {
            let prepared = prepare(&mesh);
            let serial = serial_of(&mesh, &prepared);
            let inputs = prepared.inputs();
            let (broken, _) = in_pool(3, || neighbourhood_blocks::<DropFirstNeighbour>(&inputs, 7));
            if differs(&broken, &serial) {
                seen += 1;
            }
        }
        assert!(seen > 0, "no fixture could see a dropped neighbour");
    }

    /// **Positive control.** Skipping ONLY the stamp reset, with unique global
    /// seeds, leaves every bit unchanged at every width and block size:
    /// pooled scratch needs no clearing between faces or blocks.
    #[test]
    fn skipping_only_the_stamp_reset_does_not_change_the_bits() {
        for (name, mesh) in fixtures() {
            let prepared = prepare(&mesh);
            let serial = serial_of(&mesh, &prepared);
            let inputs = prepared.inputs();
            for threads in [1_usize, 3, 4, 8, 16] {
                for block in [1_usize, 7, 1024] {
                    let (parallel, _) = in_pool(threads, || {
                        neighbourhood_blocks::<SkipResetOnly>(&inputs, block)
                    });
                    assert!(
                        !differs(&parallel, &serial),
                        "{name}: threads {threads} block {block}"
                    );
                }
            }
        }
    }
}
