//! The mandatory production QEM-decimation substrate for Plinth.
//!
//! This crate is deliberately a narrow adapter around `meshopt`'s locked QEM
//! simplifier.  It is not an orientation aid or an optional cleanup pass: M3
//! makes its print-scale output the only mesh every later production stage may
//! consume.  Probing, protection-halo selection, error-budget measurement,
//! and bisection belong to later M3 leaves; this leaf only performs one
//! deterministic locked simplification request.
#![forbid(unsafe_code)]

use std::mem::size_of;

use geom_kernel::{NativeSurface, NativeSurfaceConfig};
use meshopt::{simplify_with_locks, typed_to_bytes, SimplifyOptions, VertexDataAdapter};
use plinth_mesh::{Mesh, StrictMeshAuditError};
use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};
use thiserror::Error;

/// A single locked QEM request.  `true` means protected: the vertex is passed
/// to meshoptimizer as `meshopt_SimplifyVertex_Lock`, never as a weight.
#[derive(Clone, Debug, PartialEq)]
pub struct SimplifyRequest {
    /// Desired output face fraction, rounded down to a whole face with a
    /// one-face minimum.  The derived target is then multiplied by three.
    pub target_ratio: f64,
    /// One hard lock per source vertex. `true` means protected.
    pub vertex_locks: Vec<bool>,
}

/// The completed mesh and evidence required by later protection/error stages.
#[derive(Clone, Debug, PartialEq)]
pub struct SimplifyOutcome {
    /// Compact, source-f64 output mesh.
    pub mesh: Mesh,
    /// Output vertex number -> original source vertex number, in ascending
    /// source-ID order. This is the authoritative remapping for later stages.
    pub retained_original_vertex_ids: Vec<u32>,
    pub input_face_count: usize,
    pub output_face_count: usize,
    pub input_vertex_count: usize,
    pub output_vertex_count: usize,
    pub requested_ratio: f64,
    pub target_face_count: usize,
    pub achieved_ratio: f64,
    /// meshoptimizer's native advisory result only; it is *not* Plinth's
    /// two-sided geometric error budget (D2 owns that measurement).
    pub meshopt_advisory_error: f32,
    /// Largest absolute coordinate delta introduced only in the f64->f32
    /// adapter input. Output coordinates remain original f64 values.
    pub max_input_quantization_delta: f64,
    pub locked_vertex_count: usize,
    /// `true` only when target ratio was exactly one and the adapter bypassed
    /// meshoptimizer to preserve source topology and f64 bits exactly.
    pub identity_bypass: bool,
}

/// The boundary failures that D0 can establish without inventing repair.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum SimplifyError {
    #[error("strict mesh audit failed: {0}")]
    StrictAudit(#[from] StrictMeshAuditError),
    #[error("target ratio must be finite and in (0, 1], got {ratio}")]
    InvalidRatio { ratio: f64 },
    #[error("vertex-lock length {actual} does not equal vertex count {expected}")]
    LockLength { expected: usize, actual: usize },
    #[error("source vertex count {vertex_count} exceeds meshopt's u32 index space")]
    VertexCountTooLarge { vertex_count: usize },
    #[error("source index count {index_count} exceeds meshopt's accepted u32 count")]
    IndexCountTooLarge { index_count: usize },
    #[error("vertex {vertex} axis {axis} cannot be represented as finite f32: {value}")]
    CoordinateNotFiniteF32 {
        vertex: usize,
        axis: usize,
        value: f64,
    },
    #[error("meshopt vertex adapter rejected the prepared f32 buffer: {message}")]
    VertexAdapter { message: String },
    #[error("meshopt returned malformed index count {index_count}")]
    MalformedResultIndexCount { index_count: usize },
    #[error("meshopt returned out-of-range index {index} at result position {position}; vertex count {vertex_count}")]
    ResultIndexOutOfRange {
        position: usize,
        index: u32,
        vertex_count: usize,
    },
    #[error("meshopt returned no triangles")]
    EmptyResult,
    #[error("mesh construction after deterministic compaction failed: {message}")]
    OutputMesh { message: String },
    #[error("hard-locked source vertex {vertex} disappeared from the result")]
    LockedVertexRemoved { vertex: usize },
}

/// Explicit construction and nearest-query limits for D1 probe surfaces.
/// There are deliberately no hidden grid-pitch or nearest-distance defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeConfig {
    pub surface: NativeSurfaceConfig,
    pub nearest_upper_bound_mm: f64,
}

/// Select all points for final verification or capped deterministic samples for
/// exploratory probes. Sampling uses ChaCha8 as Plinth's ratified replacement,
/// not a claim of byte-identical numpy RNG output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbePopulation {
    Full,
    Sampled,
}

/// Reproducible selection evidence without retaining a large point copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionEvidence {
    pub source_count: usize,
    pub selected_count: usize,
    pub seed: Option<u64>,
    pub selected_index_digest: u64,
}

/// Two-sided nearest-surface evidence for later M3 budget policy.
#[derive(Clone, Debug, PartialEq)]
pub struct TwoSidedProbe {
    pub candidate_face_count: usize,
    pub forward: SelectionEvidence,
    pub backward: SelectionEvidence,
    pub p99_mm: f64,
    pub p99_9_mm: f64,
    pub max_mm: Option<f64>,
    /// Digest of the finite, source-order gathered distances before percentile
    /// sorting; this permits deterministic report comparisons across threads.
    pub distance_digest: u64,
}

/// Typed D1 failures. No non-finite distance or bounded-query miss is ignored.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum ProbeError {
    #[error("strict {side} mesh audit failed: {message}")]
    StrictAudit { side: &'static str, message: String },
    #[error("nearest upper bound must be finite and non-negative, got {value}")]
    InvalidNearestUpperBound { value: f64 },
    #[error("failed to construct {side} nearest surface: {message}")]
    SurfaceBuild { side: &'static str, message: String },
    #[error("{side} point {point} has non-finite {axis} coordinate {value}")]
    NonFinitePoint {
        side: &'static str,
        point: usize,
        axis: usize,
        value: f64,
    },
    #[error("{side} nearest query returned no hit at point {point}")]
    NoHit { side: &'static str, point: usize },
    #[error("{side} nearest query returned non-finite distance {distance} at point {point}")]
    NonFiniteDistance {
        side: &'static str,
        point: usize,
        distance: f64,
    },
    #[error("cannot compute a percentile over an empty distance population")]
    EmptyPopulation,
}

const FORWARD_CAP: usize = 150_000;
const BACKWARD_CAP: usize = 120_000;
const FORWARD_SEED: u64 = 1;
const BACKWARD_SEED: u64 = 42;

/// Builds one candidate surface and queries every original vertex in one batch.
/// Returned distances preserve original vertex order for feature-map generation.
pub fn backward_error_map(
    original: &Mesh,
    candidate: &Mesh,
    config: ProbeConfig,
) -> Result<Vec<f64>, ProbeError> {
    audit_probe_inputs(original, candidate, config)?;
    let candidate_surface = surface(candidate, "candidate", config.surface)?;
    query_batch(
        &candidate_surface,
        original.verts(),
        config.nearest_upper_bound_mm,
        "backward",
    )
}

/// Performs exactly two whole-batch nearest queries: candidate face centres to
/// original, and original vertices to candidate. Both result populations are
/// concatenated before NumPy-compatible linear percentiles are calculated.
pub fn probe_two_sided(
    original: &Mesh,
    candidate: &Mesh,
    population: ProbePopulation,
    config: ProbeConfig,
) -> Result<TwoSidedProbe, ProbeError> {
    audit_probe_inputs(original, candidate, config)?;
    let original_surface = surface(original, "original", config.surface)?;
    let candidate_surface = surface(candidate, "candidate", config.surface)?;
    let all_forward = face_centres(candidate)?;
    let (forward_indices, forward) =
        select_points(all_forward.len(), population, FORWARD_CAP, FORWARD_SEED);
    let (backward_indices, backward) = select_points(
        original.verts().len(),
        population,
        BACKWARD_CAP,
        BACKWARD_SEED,
    );
    let forward_points = forward_indices
        .iter()
        .map(|&index| all_forward[index])
        .collect::<Vec<_>>();
    let backward_points = backward_indices
        .iter()
        .map(|&index| original.verts()[index])
        .collect::<Vec<_>>();
    let mut distances = query_batch(
        &original_surface,
        &forward_points,
        config.nearest_upper_bound_mm,
        "forward",
    )?;
    distances.extend(query_batch(
        &candidate_surface,
        &backward_points,
        config.nearest_upper_bound_mm,
        "backward",
    )?);
    if distances.is_empty() {
        return Err(ProbeError::EmptyPopulation);
    }
    let mut sorted = distances.clone();
    sorted.sort_by(f64::total_cmp);
    let p99_mm = linear_percentile(&sorted, 0.99)?;
    let p99_9_mm = linear_percentile(&sorted, 0.999)?;
    let max_mm = sorted.last().copied();
    Ok(TwoSidedProbe {
        candidate_face_count: candidate.tris().len(),
        forward,
        backward,
        p99_mm,
        p99_9_mm,
        max_mm,
        distance_digest: digest_f64_bits(&distances),
    })
}

fn audit_probe_inputs(
    original: &Mesh,
    candidate: &Mesh,
    config: ProbeConfig,
) -> Result<(), ProbeError> {
    original
        .audit_strict()
        .map_err(|error| ProbeError::StrictAudit {
            side: "original",
            message: error.to_string(),
        })?;
    candidate
        .audit_strict()
        .map_err(|error| ProbeError::StrictAudit {
            side: "candidate",
            message: error.to_string(),
        })?;
    if !config.nearest_upper_bound_mm.is_finite() || config.nearest_upper_bound_mm < 0.0 {
        return Err(ProbeError::InvalidNearestUpperBound {
            value: config.nearest_upper_bound_mm,
        });
    }
    Ok(())
}

fn surface(
    mesh: &Mesh,
    side: &'static str,
    config: NativeSurfaceConfig,
) -> Result<NativeSurface, ProbeError> {
    NativeSurface::from_indexed(mesh.verts(), mesh.tris(), config).map_err(|error| {
        ProbeError::SurfaceBuild {
            side,
            message: error.to_string(),
        }
    })
}

fn query_batch(
    surface: &NativeSurface,
    points: &[[f64; 3]],
    upper_bound: f64,
    side: &'static str,
) -> Result<Vec<f64>, ProbeError> {
    for (point, value) in points.iter().enumerate() {
        for (axis, coordinate) in value.iter().copied().enumerate() {
            if !coordinate.is_finite() {
                return Err(ProbeError::NonFinitePoint {
                    side,
                    point,
                    axis,
                    value: coordinate,
                });
            }
        }
    }
    let (distances, indices) = surface.query(points, upper_bound);
    debug_assert_eq!(distances.len(), points.len());
    debug_assert_eq!(indices.len(), points.len());
    for (point, &distance) in distances.iter().enumerate() {
        if indices[point] < 0 || indices[point] as usize >= surface.nfaces() {
            return Err(ProbeError::NoHit { side, point });
        }
        if !distance.is_finite() {
            return Err(ProbeError::NonFiniteDistance {
                side,
                point,
                distance,
            });
        }
    }
    Ok(distances)
}

fn face_centres(mesh: &Mesh) -> Result<Vec<[f64; 3]>, ProbeError> {
    mesh.tris()
        .iter()
        .enumerate()
        .map(|(face, indices)| {
            let a = mesh.verts()[indices[0] as usize];
            let b = mesh.verts()[indices[1] as usize];
            let c = mesh.verts()[indices[2] as usize];
            let centre = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            for (axis, value) in centre.iter().copied().enumerate() {
                if !value.is_finite() {
                    return Err(ProbeError::NonFinitePoint {
                        side: "forward",
                        point: face,
                        axis,
                        value,
                    });
                }
            }
            Ok(centre)
        })
        .collect()
}

fn select_points(
    source_count: usize,
    population: ProbePopulation,
    cap: usize,
    seed: u64,
) -> (Vec<usize>, SelectionEvidence) {
    let use_sample = matches!(population, ProbePopulation::Sampled) && source_count > cap;
    let mut indices = (0..source_count).collect::<Vec<_>>();
    if use_sample {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        for index in 0..cap {
            let chosen = index + uniform_index(&mut rng, source_count - index);
            indices.swap(index, chosen);
        }
        indices.truncate(cap);
        indices.sort_unstable();
    }
    let evidence = SelectionEvidence {
        source_count,
        selected_count: indices.len(),
        seed: use_sample.then_some(seed),
        selected_index_digest: digest_indices(&indices),
    };
    (indices, evidence)
}

fn uniform_index(rng: &mut ChaCha8Rng, upper: usize) -> usize {
    let upper = upper as u64;
    let zone = u64::MAX - u64::MAX % upper;
    loop {
        let value = rng.next_u64();
        if value < zone {
            return (value % upper) as usize;
        }
    }
}

fn linear_percentile(sorted: &[f64], quantile: f64) -> Result<f64, ProbeError> {
    if sorted.is_empty() {
        return Err(ProbeError::EmptyPopulation);
    }
    let rank = (sorted.len() - 1) as f64 * quantile;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    Ok(sorted[lower] + (sorted[upper] - sorted[lower]) * (rank - lower as f64))
}

fn digest_indices(indices: &[usize]) -> u64 {
    digest_words(indices.iter().map(|&index| index as u64))
}
fn digest_f64_bits(values: &[f64]) -> u64 {
    digest_words(values.iter().map(|value| value.to_bits()))
}
fn digest_words(words: impl IntoIterator<Item = u64>) -> u64 {
    let mut state = 0xcbf2_9ce4_8422_2325_u64;
    for word in words {
        for byte in word.to_le_bytes() {
            state ^= u64::from(byte);
            state = state.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    state
}

/// Simplifies `mesh` once using upstream locked QEM.
///
/// The target-face rounding rule is `max(1, floor(input_faces * ratio))`.
/// It is converted to a target index count by multiplication by three.  When
/// topology or locks prevent reaching that target, the successful constrained
/// result is returned unchanged for later M3 policy to decide.
pub fn simplify_locked(
    mesh: &Mesh,
    request: &SimplifyRequest,
) -> Result<SimplifyOutcome, SimplifyError> {
    mesh.audit_strict()?;
    if !request.target_ratio.is_finite()
        || !(0.0 < request.target_ratio && request.target_ratio <= 1.0)
    {
        return Err(SimplifyError::InvalidRatio {
            ratio: request.target_ratio,
        });
    }
    if request.vertex_locks.len() != mesh.verts().len() {
        return Err(SimplifyError::LockLength {
            expected: mesh.verts().len(),
            actual: request.vertex_locks.len(),
        });
    }
    if mesh.verts().len() > u32::MAX as usize {
        return Err(SimplifyError::VertexCountTooLarge {
            vertex_count: mesh.verts().len(),
        });
    }
    let index_count =
        mesh.tris()
            .len()
            .checked_mul(3)
            .ok_or(SimplifyError::IndexCountTooLarge {
                index_count: usize::MAX,
            })?;
    if index_count > u32::MAX as usize {
        return Err(SimplifyError::IndexCountTooLarge { index_count });
    }

    let input_faces = mesh.tris().len();
    let target_faces = target_face_count(input_faces, request.target_ratio);
    let locked_vertex_count = request
        .vertex_locks
        .iter()
        .filter(|&&locked| locked)
        .count();
    let (positions, max_input_quantization_delta) = quantize_positions(mesh.verts())?;

    if request.target_ratio == 1.0 {
        return Ok(identity_outcome(
            mesh,
            request,
            target_faces,
            max_input_quantization_delta,
            locked_vertex_count,
        ));
    }

    let indices = flatten_indices(mesh.tris());
    let adapter = VertexDataAdapter::new(typed_to_bytes(&positions), size_of::<[f32; 3]>(), 0)
        .map_err(|error| SimplifyError::VertexAdapter {
            message: error.to_string(),
        })?;
    let mut advisory_error = 0.0_f32;
    // `ErrorAbsolute` makes f32::MAX a deliberately permissive absolute ceiling:
    // face ratio controls D0. D2 supplies Plinth's real two-sided measurement.
    let result = simplify_with_locks(
        &indices,
        &adapter,
        &request.vertex_locks,
        target_faces * 3,
        f32::MAX,
        SimplifyOptions::ErrorAbsolute,
        Some(&mut advisory_error),
    );
    compact_result(
        mesh,
        request,
        result,
        ResultMetadata {
            input_faces,
            target_faces,
            meshopt_advisory_error: advisory_error,
            max_input_quantization_delta,
            locked_vertex_count,
        },
    )
}

fn target_face_count(input_faces: usize, ratio: f64) -> usize {
    ((input_faces as f64 * ratio).floor() as usize).max(1)
}

fn quantize_positions(vertices: &[[f64; 3]]) -> Result<(Vec<[f32; 3]>, f64), SimplifyError> {
    let mut positions = Vec::with_capacity(vertices.len());
    let mut max_delta = 0.0_f64;
    for (vertex, point) in vertices.iter().enumerate() {
        let mut converted = [0.0_f32; 3];
        for (axis, value) in point.iter().copied().enumerate() {
            let quantized = value as f32;
            if !quantized.is_finite() {
                return Err(SimplifyError::CoordinateNotFiniteF32 {
                    vertex,
                    axis,
                    value,
                });
            }
            max_delta = max_delta.max((value - f64::from(quantized)).abs());
            converted[axis] = quantized;
        }
        positions.push(converted);
    }
    Ok((positions, max_delta))
}

fn flatten_indices(triangles: &[[u32; 3]]) -> Vec<u32> {
    let mut indices = Vec::with_capacity(triangles.len() * 3);
    for triangle in triangles {
        indices.extend_from_slice(triangle);
    }
    indices
}

fn identity_outcome(
    mesh: &Mesh,
    request: &SimplifyRequest,
    target_face_count: usize,
    max_input_quantization_delta: f64,
    locked_vertex_count: usize,
) -> SimplifyOutcome {
    SimplifyOutcome {
        mesh: mesh.clone(),
        retained_original_vertex_ids: (0..mesh.verts().len() as u32).collect(),
        input_face_count: mesh.tris().len(),
        output_face_count: mesh.tris().len(),
        input_vertex_count: mesh.verts().len(),
        output_vertex_count: mesh.verts().len(),
        requested_ratio: request.target_ratio,
        target_face_count,
        achieved_ratio: 1.0,
        meshopt_advisory_error: 0.0,
        max_input_quantization_delta,
        locked_vertex_count,
        identity_bypass: true,
    }
}

struct ResultMetadata {
    input_faces: usize,
    target_faces: usize,
    meshopt_advisory_error: f32,
    max_input_quantization_delta: f64,
    locked_vertex_count: usize,
}

fn compact_result(
    mesh: &Mesh,
    request: &SimplifyRequest,
    result: Vec<u32>,
    metadata: ResultMetadata,
) -> Result<SimplifyOutcome, SimplifyError> {
    if result.is_empty() {
        return Err(SimplifyError::EmptyResult);
    }
    if !result.len().is_multiple_of(3) {
        return Err(SimplifyError::MalformedResultIndexCount {
            index_count: result.len(),
        });
    }
    let mut retained = vec![false; mesh.verts().len()];
    for (position, &index) in result.iter().enumerate() {
        let original = index as usize;
        if original >= retained.len() {
            return Err(SimplifyError::ResultIndexOutOfRange {
                position,
                index,
                vertex_count: retained.len(),
            });
        }
        retained[original] = true;
    }
    for (vertex, &locked) in request.vertex_locks.iter().enumerate() {
        if locked && !retained[vertex] {
            return Err(SimplifyError::LockedVertexRemoved { vertex });
        }
    }
    let retained_original_vertex_ids = retained
        .iter()
        .enumerate()
        .filter_map(|(index, &used)| used.then_some(index as u32))
        .collect::<Vec<_>>();
    let mut original_to_output = vec![u32::MAX; retained.len()];
    let compact_vertices = retained_original_vertex_ids
        .iter()
        .enumerate()
        .map(|(output, &original)| {
            original_to_output[original as usize] = output as u32;
            mesh.verts()[original as usize]
        })
        .collect::<Vec<_>>();
    let compact_triangles = result
        .chunks_exact(3)
        .map(|face| {
            [
                original_to_output[face[0] as usize],
                original_to_output[face[1] as usize],
                original_to_output[face[2] as usize],
            ]
        })
        .collect::<Vec<_>>();
    let compact_mesh = Mesh::new(compact_vertices, compact_triangles).map_err(|error| {
        SimplifyError::OutputMesh {
            message: error.to_string(),
        }
    })?;
    compact_mesh
        .audit_strict()
        .map_err(|error| SimplifyError::OutputMesh {
            message: error.to_string(),
        })?;
    Ok(SimplifyOutcome {
        output_face_count: compact_mesh.tris().len(),
        output_vertex_count: compact_mesh.verts().len(),
        mesh: compact_mesh,
        retained_original_vertex_ids,
        input_face_count: metadata.input_faces,
        input_vertex_count: mesh.verts().len(),
        requested_ratio: request.target_ratio,
        target_face_count: metadata.target_faces,
        achieved_ratio: result.len() as f64 / 3.0 / metadata.input_faces as f64,
        meshopt_advisory_error: metadata.meshopt_advisory_error,
        max_input_quantization_delta: metadata.max_input_quantization_delta,
        locked_vertex_count: metadata.locked_vertex_count,
        identity_bypass: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn octahedron() -> Mesh {
        Mesh::new(
            vec![
                [1.1, 0.0, 0.0],
                [-1.1, 0.0, 0.0],
                [0.0, 1.1, 0.0],
                [0.0, -1.1, 0.0],
                [0.0, 0.0, 1.1],
                [0.0, 0.0, -1.1],
            ],
            vec![
                [0, 2, 4],
                [2, 1, 4],
                [1, 3, 4],
                [3, 0, 4],
                [2, 0, 5],
                [1, 2, 5],
                [3, 1, 5],
                [0, 3, 5],
            ],
        )
        .unwrap()
    }
    fn request(ratio: f64, locks: Vec<bool>) -> SimplifyRequest {
        SimplifyRequest {
            target_ratio: ratio,
            vertex_locks: locks,
        }
    }

    fn probe_config(bound: f64) -> ProbeConfig {
        ProbeConfig {
            surface: NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: usize::MAX,
            },
            nearest_upper_bound_mm: bound,
        }
    }

    fn triangle(z: f64) -> Mesh {
        Mesh::new(
            vec![[0.0, 0.0, z], [3.0, 0.0, z], [0.0, 3.0, z]],
            vec![[0, 1, 2]],
        )
        .unwrap()
    }

    #[test]
    fn ratio_one_is_exact_identity() {
        let mesh = octahedron();
        let outcome = simplify_locked(&mesh, &request(1.0, vec![false; 6])).unwrap();
        assert!(outcome.identity_bypass);
        assert_eq!(outcome.mesh, mesh);
        assert_eq!(outcome.retained_original_vertex_ids, vec![0, 1, 2, 3, 4, 5]);
    }
    #[test]
    fn unlocked_mesh_reduces_and_stays_strict() {
        let outcome = simplify_locked(&octahedron(), &request(0.5, vec![false; 6])).unwrap();
        assert!(outcome.output_face_count < outcome.input_face_count);
        outcome.mesh.audit_strict().unwrap();
    }
    #[test]
    fn all_locks_report_constrained_success() {
        let outcome = simplify_locked(&octahedron(), &request(0.25, vec![true; 6])).unwrap();
        assert_eq!(outcome.output_face_count, 8);
        assert_eq!(outcome.achieved_ratio, 1.0);
        assert_eq!(outcome.locked_vertex_count, 6);
    }
    #[test]
    fn protected_bits_survive() {
        let mesh = octahedron();
        let mut locks = vec![false; 6];
        locks[2] = true;
        let outcome = simplify_locked(&mesh, &request(0.5, locks)).unwrap();
        let unlocked = simplify_locked(&mesh, &request(0.5, vec![false; 6])).unwrap();
        let output = outcome
            .retained_original_vertex_ids
            .iter()
            .position(|&id| id == 2)
            .unwrap();
        assert_eq!(
            outcome.mesh.verts()[output][0].to_bits(),
            mesh.verts()[2][0].to_bits()
        );
        assert_eq!(
            outcome.mesh.verts()[output][1].to_bits(),
            mesh.verts()[2][1].to_bits()
        );
        assert!(unlocked.output_face_count < unlocked.input_face_count);
        assert!(!unlocked.retained_original_vertex_ids.contains(&2));
    }
    #[test]
    fn rejects_bad_boundaries() {
        let mesh = octahedron();
        for ratio in [f64::NAN, 0.0, -1.0, 1.01] {
            assert!(matches!(
                simplify_locked(&mesh, &request(ratio, vec![false; 6])),
                Err(SimplifyError::InvalidRatio { .. })
            ));
        }
        assert!(matches!(
            simplify_locked(&mesh, &request(0.5, vec![])),
            Err(SimplifyError::LockLength { .. })
        ));
        let overflow = Mesh::new(
            vec![[f64::MAX, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        assert!(matches!(
            simplify_locked(&overflow, &request(0.5, vec![false; 3])),
            Err(SimplifyError::CoordinateNotFiniteF32 { .. })
        ));
        let degenerate = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        assert!(matches!(
            simplify_locked(&degenerate, &request(0.5, vec![false; 3])),
            Err(SimplifyError::StrictAudit(_))
        ));
    }
    #[test]
    fn reports_nonbinary_quantization_without_leaking_it_to_output() {
        let mesh = octahedron();
        let outcome = simplify_locked(&mesh, &request(1.0, vec![false; 6])).unwrap();
        assert!(outcome.max_input_quantization_delta > 0.0);
        assert_eq!(
            outcome.mesh.verts()[0][0].to_bits(),
            mesh.verts()[0][0].to_bits()
        );
    }
    #[test]
    fn repeated_calls_are_identical() {
        let mesh = octahedron();
        let input = request(0.5, vec![false; 6]);
        assert_eq!(
            simplify_locked(&mesh, &input).unwrap(),
            simplify_locked(&mesh, &input).unwrap()
        );
    }

    /// The CI gate runs this under `PLINTH_THREADS=1`, `4`, and `16`; D0 is
    /// one whole-mesh meshopt call and must not vary with the workspace-wide
    /// scheduler setting.
    #[test]
    fn plinth_threads_does_not_change_the_deterministic_outcome() {
        let mesh = octahedron();
        let request = request(0.5, vec![false; 6]);
        let first = simplify_locked(&mesh, &request).unwrap();
        let second = simplify_locked(&mesh, &request).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn numpy_linear_percentiles_cover_tiny_odd_even_and_boundaries() {
        assert_eq!(linear_percentile(&[4.0], 0.99).unwrap(), 4.0);
        assert_eq!(linear_percentile(&[0.0, 10.0], 0.99).unwrap(), 9.9);
        assert_eq!(linear_percentile(&[0.0, 10.0, 20.0], 0.5).unwrap(), 10.0);
        assert_eq!(
            linear_percentile(&[0.0, 10.0, 20.0, 30.0], 0.0).unwrap(),
            0.0
        );
        assert_eq!(
            linear_percentile(&[0.0, 10.0, 20.0, 30.0], 1.0).unwrap(),
            30.0
        );
        assert!(
            (linear_percentile(&[0.0, 10.0, 20.0, 30.0], 0.999).unwrap() - 29.97).abs() < 1e-12
        );
    }

    #[test]
    fn batched_two_sided_probe_has_exact_parallel_plane_distances() {
        let original = triangle(1.0);
        let candidate = triangle(0.0);
        let backward = backward_error_map(&original, &candidate, probe_config(2.0)).unwrap();
        assert_eq!(backward, vec![1.0; 3]);
        let report = probe_two_sided(
            &original,
            &candidate,
            ProbePopulation::Full,
            probe_config(2.0),
        )
        .unwrap();
        assert_eq!(report.forward.selected_count, 1);
        assert_eq!(report.backward.selected_count, 3);
        assert_eq!(report.p99_mm, 1.0);
        assert_eq!(report.p99_9_mm, 1.0);
        assert_eq!(report.max_mm, Some(1.0));
    }

    #[test]
    fn backward_probe_exposes_a_lost_thin_feature_that_forward_misses() {
        let original = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [10.0, 0.0, 0.0],
                [0.0, 10.0, 0.0],
                [1.0, 1.0, 10.0],
                [2.0, 1.0, 10.0],
                [1.0, 2.0, 10.0],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .unwrap();
        let candidate = Mesh::new(
            vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let report = probe_two_sided(
            &original,
            &candidate,
            ProbePopulation::Full,
            probe_config(20.0),
        )
        .unwrap();
        assert_eq!(report.forward.selected_count, 1);
        assert!(report.p99_mm > 9.9);
        let map = backward_error_map(&original, &candidate, probe_config(20.0)).unwrap();
        assert_eq!(&map[..3], &[0.0, 0.0, 0.0]);
        assert!(map[3..].iter().all(|&distance| distance > 9.9));
    }

    #[test]
    fn capped_selection_is_seeded_sorted_and_reproducible() {
        let (first_indices, first) = select_points(
            FORWARD_CAP + 11,
            ProbePopulation::Sampled,
            FORWARD_CAP,
            FORWARD_SEED,
        );
        let (again_indices, again) = select_points(
            FORWARD_CAP + 11,
            ProbePopulation::Sampled,
            FORWARD_CAP,
            FORWARD_SEED,
        );
        let (_, different_seed) = select_points(
            BACKWARD_CAP + 11,
            ProbePopulation::Sampled,
            BACKWARD_CAP,
            BACKWARD_SEED,
        );
        assert_eq!(first.selected_count, FORWARD_CAP);
        assert_eq!(first.seed, Some(1));
        assert_eq!(first, again);
        assert_eq!(first_indices, again_indices);
        assert!(first_indices.windows(2).all(|pair| pair[0] < pair[1]));
        assert_ne!(
            first.selected_index_digest,
            different_seed.selected_index_digest
        );
        let (full_indices, full) =
            select_points(3, ProbePopulation::Full, FORWARD_CAP, FORWARD_SEED);
        assert_eq!(full_indices, vec![0, 1, 2]);
        assert_eq!(full.seed, None);
    }

    #[test]
    fn bounded_no_hit_and_invalid_config_are_typed() {
        let original = triangle(1.0);
        let candidate = triangle(0.0);
        assert!(matches!(
            backward_error_map(&original, &candidate, probe_config(0.5)),
            Err(ProbeError::NoHit {
                side: "backward",
                ..
            })
        ));
        assert!(matches!(
            probe_two_sided(
                &original,
                &candidate,
                ProbePopulation::Full,
                probe_config(f64::INFINITY)
            ),
            Err(ProbeError::InvalidNearestUpperBound { .. })
        ));
    }

    #[test]
    fn probe_report_is_thread_count_invariant() {
        let original = triangle(1.0);
        let candidate = triangle(0.0);
        let first = probe_two_sided(
            &original,
            &candidate,
            ProbePopulation::Full,
            probe_config(2.0),
        )
        .unwrap();
        let second = probe_two_sided(
            &original,
            &candidate,
            ProbePopulation::Full,
            probe_config(2.0),
        )
        .unwrap();
        assert_eq!(first, second);
    }
}
