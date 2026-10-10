//! The f64 mesh substrate for Plinth.
//!
//! Meshes are deliberately retained as their source supplied them.  In
//! particular, binary STL is an indexed *triangle soup* on disk: loading it
//! creates three vertices per facet and never performs an implicit weld,
//! reorder, repair, or f32 in-memory conversion.
#![forbid(unsafe_code)]

mod delicacy;
mod delicacy_eval;
mod display;
mod display_eval;
mod fixtures;
mod io;
mod repair;
#[cfg(feature = "serial-oracle")]
pub mod surface_oracle;
mod surface_par;
mod topology;
mod triage;

pub use delicacy::{
    DelicacyBuildConfig, DelicacyError, DelicacyFaceValue, DelicacyField, DelicacySettings,
    FaceTable, DELICACY_FIELD_SCHEMA, DELICACY_FIELD_SCHEMA_VERSION,
};
pub use delicacy_eval::evaluate_delicacy;
pub use display::{
    DisplayBuildConfig, DisplayError, DisplayFaceValue, DisplayField, DisplaySettings,
    DISPLAY_FIELD_SCHEMA, DISPLAY_FIELD_SCHEMA_VERSION,
};
pub use display_eval::evaluate_display;
pub use fixtures::{
    dual_cavity_junction_heights_mm, dual_cavity_pawn_analytic_volume_mm3, make_box,
    make_box_default, make_doubled_sheet_box, make_dual_cavity_pawn,
    make_dual_cavity_pawn_with_throat_radius, make_duplicated_face_box, make_dust_shard_box,
    make_brace_pair, make_cavity_box, make_disc_fan, make_disc_uniform, make_edge_shared_boxes,
    make_laid_tube, make_laid_tube_with,
    make_lonely_peak, make_overhang_table,
    make_overhang_table_thin, make_overhang_table_with, make_pinched_boxes, make_recessed_disc,
    make_raking_blade, make_raking_blade_with, make_thin_fin, make_thin_fin_with, make_toed_slab,
    make_peaks_with, make_tall_stilt_plate, make_toed_slab_three_shells, make_toed_slab_with,
    make_triply_wound_face_box,
    make_uncovered_rim_table,
    DUAL_CAVITY_CAVITY_THROAT_ABOVE_BORE_MM, DUAL_CAVITY_CAVITY_THROAT_AT_BORE_MM,
    DUAL_CAVITY_CENTRE_SPACING_MM, DUAL_CAVITY_LOWER_ARC_STEPS, DUAL_CAVITY_LOWER_SPHERE_R_MM,
    DUAL_CAVITY_LOWER_SPHERE_Z_MM, DUAL_CAVITY_MIN_THROAT_R_MM, DUAL_CAVITY_SLICES,
    DUAL_CAVITY_THROAT_ABOVE_BORE_MM, DUAL_CAVITY_THROAT_SEPARATED_MM, DUAL_CAVITY_UPPER_ARC_STEPS,
    DUAL_CAVITY_UPPER_SPHERE_R_MM,
};
pub use io::{
    load_binary_stl, load_binary_stls, turn_binary_stl_x180, write_binary_stl, write_binary_stls,
    MeshIoError,
};
pub use repair::{
    repair_mesh, DuplicateDisposition, DuplicateFaceEvidence, DuplicateFaceGroup,
    EmptyResultRefusal, RepairEvidence, RepairedMesh, ResidualTopology, ShardStripEvidence,
};
pub use topology::{
    build_face_topology, certify_closed_components, mesh_fingerprint, validate_face_topology,
    ClosedComponent, ClosedComponentCertificate, FaceTopology, PointOnlyVertex, TopologyCounts,
    TopologyDigest, TopologyError, TopologySchema, TopologyValidationError, WeldedView,
    FACE_TOPOLOGY_SCHEMA, FACE_TOPOLOGY_SCHEMA_VERSION, TOPOLOGY_CUT_BOUNDARY,
    TOPOLOGY_CUT_CLOSED_SOLID_MASK, TOPOLOGY_CUT_DEGENERATE, TOPOLOGY_CUT_DUPLICATE,
    TOPOLOGY_CUT_NONMANIFOLD, TOPOLOGY_CUT_POINT_ONLY, TOPOLOGY_CUT_SAME_DIRECTION,
};
pub use triage::{
    containment_ray_counters, reset_containment_ray_counters, triage_components, triage_settings,
    ComponentDescription, ContainmentRayCounters, PreHollowed, TriageAdmissionError, TriageError,
    TriageSettings, TriageSummary, TriageTopologyAdmission, TriageTopologyLimits, TriagedMesh,
};

use geom_kernel::vec3::{cross, dot, normalize, sub};
use thiserror::Error;

/// An owned f64 triangle mesh.  Vertex and triangle ordering are source data.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    verts: Vec<[f64; 3]>,
    tris: Vec<[u32; 3]>,
    weld_policy: WeldPolicy,
}

impl Mesh {
    /// Builds a finite, indexed mesh without altering its representation.
    pub fn new(verts: Vec<[f64; 3]>, tris: Vec<[u32; 3]>) -> Result<Self, MeshError> {
        Self::new_unwelded(verts, tris)
    }

    pub(crate) fn new_unwelded(
        verts: Vec<[f64; 3]>,
        tris: Vec<[u32; 3]>,
    ) -> Result<Self, MeshError> {
        Self::validate_parts(&verts, &tris)?;
        Ok(Self {
            verts,
            tris,
            weld_policy: WeldPolicy::None,
        })
    }

    /// Builds a mesh whose arrays were already explicitly welded by the
    /// caller.  This is crate-private so an IO path cannot falsely advertise
    /// a welding policy without changing the arrays it returns.
    pub(crate) fn new_welded(
        verts: Vec<[f64; 3]>,
        tris: Vec<[u32; 3]>,
        weld_policy: WeldPolicy,
    ) -> Result<Self, MeshError> {
        Self::validate_parts(&verts, &tris)?;
        debug_assert!(!matches!(weld_policy, WeldPolicy::None));
        Ok(Self {
            verts,
            tris,
            weld_policy,
        })
    }

    /// Borrow source-order vertices without permitting mutation.
    #[must_use]
    pub fn verts(&self) -> &[[f64; 3]] {
        &self.verts
    }

    /// Borrow source-order triangle indices without permitting mutation.
    #[must_use]
    pub fn tris(&self) -> &[[u32; 3]] {
        &self.tris
    }

    /// Bytes reserved by this mesh's two owned buffers, measured from their
    /// actual capacities rather than their logical lengths. Consumers that
    /// enforce a process memory envelope must account for this retained
    /// storage while they build additional working fields.
    #[must_use]
    pub fn retained_buffer_bytes(&self) -> Option<usize> {
        self.verts
            .capacity()
            .checked_mul(std::mem::size_of::<[f64; 3]>())
            .and_then(|vertices| {
                self.tris
                    .capacity()
                    .checked_mul(std::mem::size_of::<[u32; 3]>())
                    .and_then(|triangles| vertices.checked_add(triangles))
            })
    }

    /// Audits geometry required by semantic consumers without changing this
    /// source-preserving triangle soup.
    ///
    /// [`Mesh::new`] deliberately accepts finite, index-valid raw STL soup,
    /// including repeated indices and zero-area faces.  Call this method at a
    /// boundary which instead requires every face to name three distinct
    /// vertices and enclose a nonzero area.  It is a single pass over the
    /// mesh's local arrays: it neither welds, repairs, reorders, nor mutates
    /// source vertices or triangles.
    ///
    /// The audit uses an adaptive robust 2D orientation predicate against the
    /// original coordinates on the XY, YZ, and ZX projections.  Its private
    /// numerical envelope accepts zero (including signed zero) and nonzero
    /// coordinates with an inclusive magnitude from 2^-400 through 2^400.
    /// Those bounds are implementation safety limits for this predicate, not
    /// printer or physics constants.  A finite coordinate subtraction that
    /// cannot be represented is reported as an arithmetic failure rather than
    /// guessed about.
    pub fn audit_strict(&self) -> Result<(), StrictMeshAuditError> {
        for (triangle, indices) in self.tris.iter().enumerate() {
            if indices[0] == indices[1] {
                return Err(StrictMeshAuditError::RepeatedVertexIndex {
                    triangle,
                    first_corner: 0,
                    second_corner: 1,
                    vertex: indices[0],
                });
            }
            if indices[1] == indices[2] {
                return Err(StrictMeshAuditError::RepeatedVertexIndex {
                    triangle,
                    first_corner: 1,
                    second_corner: 2,
                    vertex: indices[1],
                });
            }
            if indices[2] == indices[0] {
                return Err(StrictMeshAuditError::RepeatedVertexIndex {
                    triangle,
                    first_corner: 2,
                    second_corner: 0,
                    vertex: indices[2],
                });
            }

            let a = self.verts[indices[0] as usize];
            let b = self.verts[indices[1] as usize];
            let c = self.verts[indices[2] as usize];
            if let Some(error) = strict_triangle_error(triangle, a, b, c) {
                return Err(error);
            }
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<(), MeshError> {
        Self::validate_parts(&self.verts, &self.tris)
    }

    fn validate_parts(verts: &[[f64; 3]], tris: &[[u32; 3]]) -> Result<(), MeshError> {
        if verts.is_empty() {
            return Err(MeshError::NoVertices);
        }
        if tris.is_empty() {
            return Err(MeshError::NoTriangles);
        }
        for (vertex, value) in verts.iter().enumerate() {
            for (axis, coordinate) in value.iter().enumerate() {
                if !coordinate.is_finite() {
                    return Err(MeshError::NonFiniteVertex { vertex, axis });
                }
            }
        }
        for (triangle, indices) in tris.iter().enumerate() {
            for (corner, index) in indices.iter().enumerate() {
                if (*index as usize) >= verts.len() {
                    return Err(MeshError::IndexOutOfBounds {
                        triangle,
                        corner,
                        index: *index,
                        vertex_count: verts.len(),
                    });
                }
            }
        }
        Ok(())
    }

    /// The policy explicitly selected for this mesh's construction or load.
    #[must_use]
    pub const fn weld_policy(&self) -> WeldPolicy {
        self.weld_policy
    }

    /// The minimum and maximum Z coordinates in source vertex order.
    #[must_use]
    pub fn bounds_z(&self) -> (f64, f64) {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for vertex in &self.verts {
            min = min.min(vertex[2]);
            max = max.max(vertex[2]);
        }
        (min, max)
    }

    /// Unit-face-normal Z components in triangle order.
    pub fn face_normals_z(&self) -> Result<Vec<f64>, MeshError> {
        self.tris
            .iter()
            .enumerate()
            .map(|(triangle, indices)| {
                let a = self.verts[indices[0] as usize];
                let b = self.verts[indices[1] as usize];
                let c = self.verts[indices[2] as usize];
                // Canonical face normal: `geom_kernel::vec3` cross +
                // `normalize` (docs/specs/geometry-canon.md §1 OP-4 and OP-6,
                // triage rule (d)). The squared-norm overflow refusal is kept
                // here because it is the one this crate's callers read; with it
                // satisfied, `normalize` is `None` on exactly the remaining
                // former refusal, the zero-length cross product.
                //
                // The SPELLING moves bits: the kernel multiplies by one
                // reciprocal where this function divided (kernel `normalize`
                // doc, "Spelling, and the ULP it costs"). Measured over the
                // eight-model corpus: 27.26 % of normal components move, all
                // by exactly 1 ULP, at most 1.110e-16. This method has no
                // production consumer outside this crate's own tests today, so
                // nothing downstream can see it; that is a fact about the
                // present call graph, not a licence for a future caller to
                // assume the value is threshold-stable.
                let normal = cross(sub(b, a), sub(c, a));
                if !dot(normal, normal).is_finite() {
                    return Err(MeshError::NormalArithmeticOverflow { triangle });
                }
                let unit = normalize(normal).ok_or(MeshError::DegenerateTriangle { triangle })?;
                if !unit[2].is_finite() {
                    return Err(MeshError::NormalArithmeticOverflow { triangle });
                }
                Ok(unit[2])
            })
            .collect()
    }
}

/// A deliberately explicit vertex welding choice.
///
/// M2's IO applies only [`WeldPolicy::None`].  The other variants remain
/// vocabulary for the later topology/triage operation; requesting either at
/// this boundary returns a typed unsupported-policy error rather than merely
/// recording a false label.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeldPolicy {
    /// Preserve the source triangle soup exactly (the STL default).
    None,
    /// Weld only numerically identical coordinates. Distinct finite values,
    /// including adjacent one-ULP values, remain separate; `+0.0` and `-0.0`
    /// weld because they are the same physical coordinate despite their
    /// different IEEE sign bits. The historical variant name refers to the
    /// nonzero finite-value comparison.
    ExactBits,
    /// Weld coordinates within this positive, finite distance.
    Epsilon(f64),
}

/// Semantic failures at a mesh API boundary.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum MeshError {
    #[error("mesh has no vertices")]
    NoVertices,
    #[error("mesh has no triangles")]
    NoTriangles,
    #[error("vertex {vertex} coordinate {axis} is not finite")]
    NonFiniteVertex { vertex: usize, axis: usize },
    #[error("triangle {triangle} corner {corner} references vertex {index}, but only {vertex_count} vertices exist")]
    IndexOutOfBounds {
        triangle: usize,
        corner: usize,
        index: u32,
        vertex_count: usize,
    },
    #[error("triangle {triangle} has zero area")]
    DegenerateTriangle { triangle: usize },
    #[error("triangle {triangle} overflowed while calculating its normal")]
    NormalArithmeticOverflow { triangle: usize },
    #[error("fixture parameter {parameter} must be finite and greater than zero, got {value}")]
    InvalidPositiveFixtureParameter { parameter: &'static str, value: f64 },
    #[error("fixture parameter {parameter} must be finite, got {value}")]
    InvalidFiniteFixtureParameter { parameter: &'static str, value: f64 },
    /// An added admission bound (FU-8b permits adding a bound; it never
    /// permits loosening one). Distinct from
    /// [`Self::InvalidPositiveFixtureParameter`] because "outside an open
    /// interval" is a different, more specific claim than "not positive" and
    /// deserves its own reason in the error rather than a misreported one.
    #[error("fixture parameter {parameter} must be finite and within ({min}, {max}), got {value}")]
    InvalidRangeFixtureParameter {
        parameter: &'static str,
        value: f64,
        min: f64,
        max: f64,
    },
    #[error("fixture has too many vertices for u32 triangle indices")]
    FixtureIndexOverflow,
}

/// A strict semantic-audit failure for an otherwise representable [`Mesh`].
///
/// These errors are intentionally separate from [`MeshError`]: construction
/// establishes that raw source soup is finite and index-valid, while strict
/// audit establishes the stronger geometric contract required by a caller.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum StrictMeshAuditError {
    #[error(
        "triangle {triangle} repeats vertex {vertex} at corners {first_corner} and {second_corner}"
    )]
    RepeatedVertexIndex {
        triangle: usize,
        first_corner: usize,
        second_corner: usize,
        vertex: u32,
    },
    #[error("triangle {triangle} has zero area or collinear distinct vertices")]
    ZeroAreaOrCollinear { triangle: usize },
    #[error("triangle {triangle} overflowed while deriving strict-audit geometry")]
    DerivedArithmeticOverflow { triangle: usize },
    #[error(
        "triangle {triangle} corner {corner} axis {axis} coordinate {value} is outside the robust-predicate envelope"
    )]
    PredicateOutOfDomain {
        triangle: usize,
        corner: usize,
        axis: usize,
        value: f64,
    },
    #[error(
        "triangle {triangle} robust orientation on axes {first_axis}/{second_axis} returned non-finite {value}"
    )]
    PredicateNonFiniteResult {
        triangle: usize,
        first_axis: usize,
        second_axis: usize,
        value: f64,
    },
}

// Exact powers of two: an input to the adaptive predicate is either zero or
// within this inclusive magnitude envelope.  This avoids relying on its
// unsupported underflow/overflow regimes while preserving all normal model
// and promoted-f32 coordinates.
const PREDICATE_MIN_ABS: f64 = f64::from_bits(623_u64 << 52); // 2^-400
const PREDICATE_MAX_ABS: f64 = f64::from_bits(1423_u64 << 52); // 2^400
const PROJECTIONS: [(usize, usize); 3] = [(0, 1), (1, 2), (2, 0)];

/// Returns the first strict-audit failure, or `None` once one projection
/// proves nonzero area.  A safe later projection can prove area despite an
/// earlier unsafe one, so failures are retained until all projections finish.
fn strict_triangle_error(
    triangle: usize,
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
) -> Option<StrictMeshAuditError> {
    let vertices = [a, b, c];
    let mut first_failure = None;

    for (first_axis, second_axis) in PROJECTIONS {
        // `robust::orient2d(a, b, c)` starts from precisely these four
        // subtractions.  Check them before the coordinate envelope so a
        // derived overflow has deterministic precedence within a projection.
        let subtractions = [
            a[first_axis] - c[first_axis],
            a[second_axis] - c[second_axis],
            b[first_axis] - c[first_axis],
            b[second_axis] - c[second_axis],
        ];
        if subtractions.iter().any(|value| !value.is_finite()) {
            first_failure
                .get_or_insert(StrictMeshAuditError::DerivedArithmeticOverflow { triangle });
            continue;
        }

        let mut out_of_domain = None;
        for (corner, vertex) in vertices.iter().enumerate() {
            for axis in [first_axis, second_axis] {
                let value = vertex[axis];
                if value != 0.0 && !(PREDICATE_MIN_ABS..=PREDICATE_MAX_ABS).contains(&value.abs()) {
                    out_of_domain = Some(StrictMeshAuditError::PredicateOutOfDomain {
                        triangle,
                        corner,
                        axis,
                        value,
                    });
                    break;
                }
            }
            if out_of_domain.is_some() {
                break;
            }
        }
        if let Some(error) = out_of_domain {
            first_failure.get_or_insert(error);
            continue;
        }

        let result = robust::orient2d(
            robust::Coord {
                x: a[first_axis],
                y: a[second_axis],
            },
            robust::Coord {
                x: b[first_axis],
                y: b[second_axis],
            },
            robust::Coord {
                x: c[first_axis],
                y: c[second_axis],
            },
        );
        if !result.is_finite() {
            first_failure.get_or_insert(StrictMeshAuditError::PredicateNonFiniteResult {
                triangle,
                first_axis,
                second_axis,
                value: result,
            });
        } else if result != 0.0 {
            return None;
        }
    }

    if first_failure.is_none() {
        Some(StrictMeshAuditError::ZeroAreaOrCollinear { triangle })
    } else {
        first_failure
    }
}
