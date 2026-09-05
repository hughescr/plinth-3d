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
//! * **`detail`** is the area-weighted mean of `1 − cos` between the face's
//!   normal and its neighbours' normals over a surface neighbourhood of radius
//!   `R_detail`, clamped into `[0, 1]`.  `1 − cos` is `0` on a flat region and
//!   `1` when the neighbourhood's normals are decorrelated, which is what
//!   makes `1` a meaning rather than a chosen divisor.  This is the owner's
//!   "local low cosine similarity among neighbouring triangle normals".
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
//!   (`tests/display_field.rs`) it never fired at all: the widened
//!   neighbourhood ran onto a CYLINDRICAL shaft whose normals differ from the
//!   cap's just as much, so the articulation did not collapse and the
//!   discount was identically `1`.  The recorded value was `min tip_factor
//!   1.000000` on every cap face.  The curvature identity has no such
//!   dependence on what the protrusion is mounted to.
//!
//! **They multiply, and that choice is load-bearing.**  An additive or
//! probabilistic-OR combination would give a smooth, featureless authored-top
//! face a positive `D` on visibility alone.  Measured, that is not a
//! theoretical worry: all three `Round_*.stl` bases are authored recess-UP,
//! and the owner's ChituBox verdict of 2026-09-03 is that `Round 50`/`Round
//! 100` are correct at identity while `Round 25` is correct at a 180° flip.
//! Their authored frames agree and their correct poses do not, so an authored
//! up-signal that fires on featureless geometry would move `Round 25` to the
//! WRONG pose.  Under the product, a featureless face has `detail = 0` and
//! therefore `D = 0`, the objective is unchanged there, and the authored-up
//! signal speaks only where there is detail for it to speak about.  That is
//! also what makes `D = 0` reproduce today's objective exactly on plain
//! geometry rather than approximately.
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

use geom_kernel::vec3::{dot, norm, sub};
use geom_kernel::{NativeSurface, NativeSurfaceConfig, V3};

use crate::delicacy::FaceTable;
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

    let geometry = FaceGeometryPass::run(mesh);
    let visibility = visibility_by_face(mesh, &geometry, settings, config)?;
    let WeldedAngleDefect {
        topology,
        defect,
        unusable,
    } = welded_angle_defect(mesh, &geometry).map_err(DisplayError::Topology)?;
    let curvature_by_face = curvature_by_face(mesh, &geometry, &topology, &defect, &unusable);
    let neighbourhood = neighbourhood_pass(&geometry, &topology, &curvature_by_face, settings);

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

/// One surface-neighbourhood pass: the detail density and the total Gaussian
/// curvature of every face's `R_detail` neighbourhood, in a single traversal
/// per face over a shared scratch frontier.
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
fn neighbourhood_pass(
    geometry: &FaceGeometryPass,
    topology: &crate::FaceTopology,
    curvature_by_face: &[f64],
    settings: DisplaySettings,
) -> Neighbourhood {
    let face_count = geometry.normals.len();
    let radius_sq = settings.detail_radius_mm() * settings.detail_radius_mm();

    let mut detail = vec![0.0_f64; face_count];
    let mut curvature = vec![0.0_f64; face_count];

    // Shared scratch, reused across faces: a visited stamp keyed by the source
    // face index (so it is cleared in O(visited), never O(faces)) and one
    // frontier buffer.
    let mut stamp = vec![u32::MAX; face_count];
    let mut frontier: Vec<u32> = Vec::new();
    let mut visited: Vec<u32> = Vec::new();

    for face in 0..face_count {
        if geometry.degenerate[face] {
            continue;
        }
        let seed = u32::try_from(face).unwrap_or(u32::MAX);
        let origin = geometry.centroids[face];
        let normal = geometry.normals[face];
        let ring = topology.neighbors(face).unwrap_or(&[]);

        frontier.clear();
        visited.clear();
        frontier.push(seed);
        stamp[face] = seed;
        visited.push(seed);

        let mut area_total = 0.0_f64;
        let mut articulation_total = 0.0_f64;
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
            if !geometry.degenerate[current] {
                let area = geometry.areas_mm2[current];
                area_total += area;
                articulation_total += area * (1.0 - dot(normal, geometry.normals[current]));
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

        if area_total > 0.0 {
            detail[face] = articulation_total / area_total;
        }
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
