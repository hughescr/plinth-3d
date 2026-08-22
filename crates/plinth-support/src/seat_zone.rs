//! The seat's own contact zone `Z` — the honest contact exemption (#36).
//!
//! # What this replaces, and why the replacement is a different KIND of rule
//!
//! A member seated into the model is legitimately inside the material it is
//! seated in. Something must be exempt from its clearance test or every seated
//! arm refuses itself. Until 2026-08-22 the exempt thing was a LENGTH: the
//! PR-2 mask `u* = (h + r_max + standoff + resolution)/|cos|`, handed to the
//! kernel as an inset, skipped that much axis from the contact end and read the
//! verdict at the boundary station.
//!
//! The measured audit in `.git/mask-honesty-evidence` is what retires it. Three
//! facts, all MEASURED on the Giant Bat:
//!
//! * the skip is proportional to the radius, so a fatter arm is tested over a
//!   SHORTER stretch of its own axis;
//! * on slot 250 that inverted the verdict — an arm at `r = 1.712694 mm` passed
//!   while standing 0.242160 mm INSIDE the model at `u = 0.6903`, and the same
//!   axis at `r = 0.901736 mm` was refused with 0.568799 mm of true gap at its
//!   own boundary station;
//! * both refusals were FOREIGN-FEATURE proximity — the opposite wall of a
//!   crevice — not the seat's own surface at all.
//!
//! The last fact is the design: on the seat's own tangent plane nothing beyond
//! the natural clearing station can violate, so every millimetre the mask
//! skipped past the contact print bought nothing and hid everything.
//!
//! # The rule this module implements
//!
//! EXEMPT A TARGET, NEVER A LENGTH. Test the FULL axis (`inset0 = inset1 = 0`)
//! against the whole surface EXCEPT the seat's own contact zone
//!
//! ```text
//! Z = the connected component, containing the seat's own face, of
//!     { x in surface : dist(x, axis) < r_seat + standoff_placement }
//! ```
//!
//! and FREEZE `Z` at contact creation. It is stored with the contact and never
//! recomputed from a later radius.
//!
//! # Why the freeze is what makes monotonicity unconditional
//!
//! Fix an axis and a frozen `Z`. The graded clearance is
//! `min over the axis of dist(x, surface \ Z) - (r_max + demanded_gap)`.
//! The first term does not mention the radius at all: the tested station set is
//! the whole axis either way, and the tested face set is `surface \ Z`, which is
//! frozen. So the clearance is `C - r_max - gap` with `C` a constant of the
//! axis, and it is MONOTONE DECREASING in `r_max`. A thinner arm therefore can
//! never grade worse than a fatter one on the same axis. THEOREM, not a
//! measurement.
//!
//! Recompute `Z` from the final radius instead and the theorem dies: a fatter
//! arm claims a wider band, a wider band swallows more faces, and the fatter arm
//! is graded against strictly less surface than the thin one. That is the same
//! shape of defect as the skip-length, one level up, and it is exactly the
//! sabotage the frozen-Z test reproduces.
//!
//! # Batch-first
//!
//! One `faces_near_segments_batch` call for a whole row set gathers every band;
//! the flood fills then run in parallel over rows against the crate's one
//! `FaceTopology`. There is no per-item native call.

use std::collections::BTreeSet;
use std::sync::Arc;

use geom_kernel::{NativeSurface, NearFaceBatchError, NearFaceBatchLimits};
use plinth_mesh::FaceTopology;
use rayon::prelude::*;

/// One contact whose zone is being frozen.
///
/// Every field is a CREATION-TIME value. `band_radius_mm` in particular is
/// `r_seat + standoff_placement` at the radius the contact was minted with; a
/// later resize must never rewrite it, which is the whole point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SeatZoneRequest {
    pub(crate) p0_mm: [f64; 3],
    pub(crate) p1_mm: [f64; 3],
    /// The global face ID the seat point projected onto at creation.
    pub(crate) seat_face_id: u32,
    /// `r_seat + standoff_placement`, FROZEN.
    pub(crate) band_radius_mm: f64,
}

/// Why a batch of zones could not be frozen.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SeatZoneError {
    /// The kernel refused the band collection.
    Band(String),
    /// A seat named a face the topology does not have. This is a mesh/topology
    /// disagreement, not a geometry outcome, so it fails rather than silently
    /// yielding an empty zone.
    SeatFaceOutOfRange { row: usize, face_id: u32 },
}

impl std::fmt::Display for SeatZoneError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Band(reason) => write!(formatter, "seat-zone band collection failed: {reason}"),
            Self::SeatFaceOutOfRange { row, face_id } => write!(
                formatter,
                "seat-zone row {row} named face {face_id}, which the retained topology does not have"
            ),
        }
    }
}

impl std::error::Error for SeatZoneError {}

impl From<NearFaceBatchError> for SeatZoneError {
    fn from(error: NearFaceBatchError) -> Self {
        Self::Band(error.to_string())
    }
}

/// Freeze one zone per request, in ONE batched band call plus a parallel flood
/// fill per row.
///
/// Each returned slice is SORTED ascending and deduplicated, which is the order
/// `NativeSurface::segment_clearance_excluding` requires of an exclusion set.
///
/// An empty zone is a legal answer and means "exempt nothing, grade the whole
/// surface". It arises when the seat face is not itself inside the band, which
/// a seated or on-surface contact cannot produce (its own face is at distance
/// zero from the axis) but a degenerate request could. Empty is the SAFE
/// direction: it can only make a verdict stricter.
pub(crate) fn freeze_seat_zones(
    surface: &NativeSurface,
    topology: &FaceTopology,
    requests: &[SeatZoneRequest],
    limits: NearFaceBatchLimits,
) -> Result<Vec<Arc<[u32]>>, SeatZoneError> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    let face_count = topology.n_faces();
    for (row, request) in requests.iter().enumerate() {
        if request.seat_face_id as usize >= face_count {
            return Err(SeatZoneError::SeatFaceOutOfRange {
                row,
                face_id: request.seat_face_id,
            });
        }
    }

    let p0 = requests.iter().map(|row| row.p0_mm).collect::<Vec<_>>();
    let p1 = requests.iter().map(|row| row.p1_mm).collect::<Vec<_>>();
    let radii = requests
        .iter()
        .map(|row| row.band_radius_mm)
        .collect::<Vec<_>>();
    // THE one batched native call for the whole row set.
    let bands = surface.faces_near_segments_batch(&p0, &p1, &radii, limits)?;

    Ok(bands
        .into_par_iter()
        .zip(requests.into_par_iter())
        .map(|(band, request)| flood_fill(topology, &band, request.seat_face_id))
        .collect())
}

/// The connected component of `band` that contains `seat_face_id`.
///
/// `band` is sorted, so membership is a binary search and the fill needs no
/// allocation proportional to the mesh — only to the component it finds. That
/// matters: the mesh is hundreds of thousands of faces and a seat zone is tens.
fn flood_fill(topology: &FaceTopology, band: &[u32], seat_face_id: u32) -> Arc<[u32]> {
    if band.binary_search(&seat_face_id).is_err() {
        return Arc::from(Vec::new());
    }
    let mut seen = BTreeSet::new();
    let mut stack = vec![seat_face_id];
    seen.insert(seat_face_id);
    while let Some(face) = stack.pop() {
        let Some(neighbors) = topology.neighbors(face as usize) else {
            continue;
        };
        for &neighbor in neighbors {
            if band.binary_search(&neighbor).is_err() {
                continue;
            }
            if seen.insert(neighbor) {
                stack.push(neighbor);
            }
        }
    }
    // `BTreeSet` iteration is ascending, which is the sortedness the kernel's
    // exclusion set requires; no second sort is needed and none is done.
    Arc::from(seen.into_iter().collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    use geom_kernel::testkit::build_from_tris;
    use plinth_mesh::{build_face_topology, Mesh, WeldPolicy};

    /// Two square walls facing each other across a 3 mm crevice, each split into
    /// two triangles so the walls have interior adjacency and the flood fill has
    /// something to walk.
    ///
    /// Faces 0,1 are the near wall at `x = 0`; faces 2,3 the far wall at
    /// `x = 3`. The two walls share no vertex, so no adjacency joins them: a
    /// fill started on one can never reach the other, which is the property the
    /// exemption depends on.
    fn crevice_parts() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let vertices = vec![
            [0.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 10.0, 10.0],
            [0.0, 0.0, 10.0],
            [3.0, 0.0, 0.0],
            [3.0, 10.0, 0.0],
            [3.0, 10.0, 10.0],
            [3.0, 0.0, 10.0],
        ];
        let faces = vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]];
        (vertices, faces)
    }

    fn crevice_surface() -> NativeSurface {
        let (vertices, faces) = crevice_parts();
        let mut tris = Vec::new();
        for face in &faces {
            for index in face {
                tris.extend_from_slice(&vertices[*index as usize]);
            }
        }
        build_from_tris(tris)
    }

    fn crevice_topology() -> FaceTopology {
        let (vertices, faces) = crevice_parts();
        let mesh = Mesh::new(vertices, faces).expect("crevice mesh");
        build_face_topology(&mesh, WeldPolicy::ExactBits).expect("crevice topology")
    }

    fn uncapped() -> NearFaceBatchLimits {
        NearFaceBatchLimits {
            max_rows: usize::MAX,
            max_faces_per_row: usize::MAX,
            max_total_faces: usize::MAX,
        }
    }

    #[test]
    fn a_zone_stops_at_the_crevice_and_never_crosses_to_the_foreign_wall() {
        let surface = crevice_surface();
        let topology = crevice_topology();
        // An axis 1.0 mm from the near wall and 2.0 mm from the far one, with a
        // band wide enough to touch BOTH.
        let zones = freeze_seat_zones(
            &surface,
            &topology,
            &[SeatZoneRequest {
                p0_mm: [1.0, 2.0, 2.0],
                p1_mm: [1.0, 2.0, 6.0],
                seat_face_id: 0,
                band_radius_mm: 2.5,
            }],
            uncapped(),
        )
        .expect("zones");
        assert_eq!(
            zones[0].as_ref(),
            &[0, 1],
            "the band contains all four faces, but only the seat's own wall is \
             CONNECTED to the seat face. The far wall is a foreign feature and \
             must stay graded — that is the blind spot the retired skip-length \
             opened"
        );
    }

    /// One wall at `x = 0`, ten `y` strips of two triangles each, all connected.
    ///
    /// A short axis parked in front of one strip reaches further ALONG the wall
    /// as its band widens, and every face it reaches is connected to the seat —
    /// so here, unlike the crevice, the zone genuinely grows with the band. That
    /// is what makes this the fixture for the freeze.
    fn strip_wall_parts() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let mut vertices = Vec::new();
        for step in 0..=10 {
            vertices.push([0.0, f64::from(step), 0.0]);
            vertices.push([0.0, f64::from(step), 10.0]);
        }
        let mut faces = Vec::new();
        for strip in 0..10_u32 {
            let base = strip * 2;
            faces.push([base, base + 2, base + 3]);
            faces.push([base, base + 3, base + 1]);
        }
        (vertices, faces)
    }

    fn strip_wall() -> (NativeSurface, FaceTopology) {
        let (vertices, faces) = strip_wall_parts();
        let mut tris = Vec::new();
        for face in &faces {
            for index in face {
                tris.extend_from_slice(&vertices[*index as usize]);
            }
        }
        let mesh = Mesh::new(vertices, faces).expect("strip mesh");
        (
            build_from_tris(tris),
            build_face_topology(&mesh, WeldPolicy::ExactBits).expect("strip topology"),
        )
    }

    #[test]
    fn the_zone_does_not_move_when_the_radius_does_because_it_is_never_recomputed() {
        let (surface, topology) = strip_wall();
        // The request a THIN arm minted at creation: 1.0 mm off the wall, band
        // 1.5 mm.
        let frozen = SeatZoneRequest {
            p0_mm: [1.0, 5.0, 5.0],
            p1_mm: [1.0, 5.2, 5.0],
            seat_face_id: 10,
            band_radius_mm: 1.5,
        };
        let first = freeze_seat_zones(&surface, &topology, &[frozen], uncapped()).expect("zones");
        let again = freeze_seat_zones(&surface, &topology, &[frozen], uncapped()).expect("zones");
        assert_eq!(first[0], again[0], "freezing is deterministic");

        // What a RECOMPUTE from a later, LARGER radius would have produced.
        let recomputed = freeze_seat_zones(
            &surface,
            &topology,
            &[SeatZoneRequest {
                band_radius_mm: 3.0,
                ..frozen
            }],
            uncapped(),
        )
        .expect("zones");
        assert!(
            recomputed[0].len() > first[0].len(),
            "a wider band swallows strictly more of the wall ({} faces against \
             {}). THAT is why the zone must be frozen: recompute it from the \
             final radius and a fatter arm is graded against strictly less \
             surface than a thin one on the same axis, which is the retired \
             skip-length's inversion rebuilt one level up",
            recomputed[0].len(),
            first[0].len()
        );
        assert!(
            first[0].iter().all(|face| recomputed[0].contains(face)),
            "and the growth is pure addition — the thin arm's zone is a subset, \
             so nothing the thin arm was already forgiven becomes graded"
        );
    }

    #[test]
    fn a_seat_face_outside_its_own_band_exempts_nothing() {
        let surface = crevice_surface();
        let topology = crevice_topology();
        let zones = freeze_seat_zones(
            &surface,
            &topology,
            &[SeatZoneRequest {
                p0_mm: [1.0, 2.0, 2.0],
                p1_mm: [1.0, 2.0, 6.0],
                // The FAR wall, 2.0 mm away, with a band that reaches 1.5 mm.
                seat_face_id: 2,
                band_radius_mm: 1.5,
            }],
            uncapped(),
        )
        .expect("zones");
        assert!(
            zones[0].is_empty(),
            "an empty zone grades the whole surface, which is the safe \
             direction: it can only make a verdict stricter"
        );
    }

    #[test]
    fn a_seat_face_the_topology_does_not_have_is_a_failure_not_an_empty_zone() {
        let surface = crevice_surface();
        let topology = crevice_topology();
        let error = freeze_seat_zones(
            &surface,
            &topology,
            &[SeatZoneRequest {
                p0_mm: [1.0, 2.0, 2.0],
                p1_mm: [1.0, 2.0, 6.0],
                seat_face_id: 99,
                band_radius_mm: 1.5,
            }],
            uncapped(),
        )
        .expect_err("out-of-range seat face");
        assert_eq!(
            error,
            SeatZoneError::SeatFaceOutOfRange {
                row: 0,
                face_id: 99
            }
        );
    }

    #[test]
    fn zones_come_back_sorted_and_deduplicated() {
        let surface = crevice_surface();
        let topology = crevice_topology();
        let zones = freeze_seat_zones(
            &surface,
            &topology,
            &[SeatZoneRequest {
                p0_mm: [1.0, 2.0, 2.0],
                p1_mm: [1.0, 2.0, 6.0],
                seat_face_id: 1,
                band_radius_mm: 2.5,
            }],
            uncapped(),
        )
        .expect("zones");
        let zone = zones[0].as_ref();
        assert!(zone.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
