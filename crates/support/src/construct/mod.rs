//! The sealed construction API (lane A; design `step11-construction.md` section 6).
//!
//! Step 0 holds the shape of the API: the typestate (`Contacts`, `Routed`), the batch
//! `probe` and the rejection list. Every `probe` answers a rejection that says the check is
//! not built, so nothing is admitted by accident. Lane A replaces the bodies, seals the types
//! (private fields, no `Clone` on quotes) and adds the compile-fail tests.

use plinth_geom::Mesh;
use std::marker::PhantomData;

/// A candidate's id within one batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CandId(pub u32);

/// Why a candidate was refused (design section 6.1).
#[derive(Debug, Clone, PartialEq)]
pub enum Reject {
    /// The patch upper bound is over the admission bound.
    AreaOverLimit {
        /// Patch upper bound, mm².
        hi: f64,
        /// The bound, mm².
        max: f64,
    },
    /// The patch cannot be bounded on the written mesh.
    PatchUnverified,
    /// The head makes a second patch on another model sheet.
    SecondSheet,
    /// The patch would fuse with another within the separation.
    WouldFuse {
        /// The other candidate.
        with: CandId,
    },
    /// The head region overlaps a reserved region.
    RegionBlocked {
        /// The candidate whose region it is.
        by: CandId,
    },
    /// The neck or cone touches the model inside the region.
    HeadTouchesModel,
    /// The gap to the model decreases along the head.
    DepartureNotMonotone,
    /// The ball's lowest point is not covered by the cone or joint.
    PoleUncovered,
    /// A part outside the region is closer than the build gap.
    GapTooSmall {
        /// Certified gap lower bound, mm.
        gap: f64,
    },
    /// A part would print before its carrier.
    PrintOrder {
        /// The layer where it breaks.
        layer: u32,
    },
    /// A section is thinner than the construction minimum.
    TooThin {
        /// Width across flats, mm.
        across: f64,
    },
    /// A join is narrower than the construction minimum.
    NarrowJoin,
    /// The raft touches the model.
    RaftTouchesModel,
    /// The contact's certified capacity does not carry its load.
    InsufficientCapacity,
    /// A value is not finite.
    NonFinite,
    /// The plan changed since the quote was made.
    Stale,
    /// The handle belongs to another plan.
    ForeignHandle,
    /// The shape is not one the native files reproduce.
    UnknownShape,
    /// The measurement did not reach the required width within its work budget.
    MeasureExhausted,
    /// A search or memory budget ran out.
    ResourceExhausted,
    /// The check is not built yet (step 0 stub).
    NotBuilt,
}

/// Why a pose cannot be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoseError {
    /// The pose step is not built yet (step 0 stub).
    NotBuilt,
}

/// The recorded keep-up rotation of the approved STL.
#[derive(Debug, Clone, PartialEq)]
pub struct KeepUpRecord {
    /// Rotation about X from the original, degrees (115 for the Round bases).
    pub rotation_x_deg: f64,
    /// SHA-256 of the approved STL.
    pub approved_sha256: String,
}

/// The hard limits and build values, parsed from `limits.toml` and the design.
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    /// H1 admission bound for the patch upper bound, mm².
    pub h1_admission_mm2: f64,
    /// H4 minimum lift of the lowest model point, mm.
    pub h4_lift_mm: f64,
    /// H6 build gap outside the contact regions, mm.
    pub h6_build_gap_mm: f64,
    /// H9 construction minimum width across flats, mm.
    pub h9_across_flats_mm: f64,
    /// The contact-surrounding region length, mm.
    pub region_length_mm: f64,
}

/// The posed model: the f32 mesh that will be written, its lift and hash.
#[derive(Debug)]
pub struct PosedModel {
    _private: (),
}

impl PosedModel {
    /// Centre and lift `input` so the lowest written vertex is at the H4 lift.
    ///
    /// # Errors
    /// [`PoseError`] when the pose cannot be made.
    pub fn new(input: &Mesh, keep_up: &KeepUpRecord, limits: &Limits) -> Result<Self, PoseError> {
        let _ = (input, keep_up, limits);
        Err(PoseError::NotBuilt)
    }
}

/// The layer profile and process record a plan is made under.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessRecord {
    /// Layer height, mm.
    pub layer_height_mm: f64,
    /// Layer origin, mm.
    pub layer_origin_mm: f64,
}

/// One proposed contact.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactCandidate {
    /// Id within the batch.
    pub id: CandId,
    /// Surface point the ball touches, mm.
    pub site: [f32; 3],
    /// Ball radius, mm.
    pub ball_r: f32,
    /// Distance of the ball centre outside the surface, mm.
    pub standoff: f32,
    /// Head axis, unit vector from the ball centre toward the joint.
    pub axis: [f32; 3],
    /// Neck radius, mm.
    pub neck: f32,
    /// Joint radius, mm.
    pub joint_r: f32,
    /// Distance from ball centre to joint centre, mm.
    pub head_len: f32,
}

/// An admitted-to-be contact, bound to one plan state. Not `Clone`.
#[derive(Debug)]
pub struct Quote {
    cand: CandId,
    rev: u64,
}

impl Quote {
    /// The candidate this quote answers.
    #[must_use]
    pub fn cand(&self) -> CandId {
        self.cand
    }

    /// The plan revision it was made at.
    #[must_use]
    pub fn rev(&self) -> u64 {
        self.rev
    }
}

/// Typestate: contacts may still be admitted.
#[derive(Debug)]
pub struct Contacts;
/// Typestate: contacts are final; members are admitted.
#[derive(Debug)]
pub struct Routed;

/// The plan under construction. The type parameter is the typestate.
#[derive(Debug)]
pub struct PlanBuilder<S> {
    rev: u64,
    _state: PhantomData<S>,
}

impl PlanBuilder<Contacts> {
    /// Start a plan on a posed model.
    #[must_use]
    pub fn new(model: PosedModel, limits: Limits, process: ProcessRecord) -> Self {
        let _ = (model, limits, process);
        Self {
            rev: 0,
            _state: PhantomData,
        }
    }

    /// The revision a quote made now would be bound to.
    #[must_use]
    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// Check a batch of candidates, in parallel and purely, including against each other.
    ///
    /// `out` is cleared and refilled with exactly one answer per candidate.
    pub fn probe(&self, cands: &[ContactCandidate], out: &mut Vec<Result<Quote, Reject>>) {
        out.clear();
        out.extend(cands.iter().map(|_| Err(Reject::NotBuilt)));
    }

    /// Move to the routing state. No contact can be admitted afterwards.
    #[must_use]
    pub fn into_routing(self) -> PlanBuilder<Routed> {
        PlanBuilder {
            rev: self.rev,
            _state: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stub_admits_nothing() {
        let cand = ContactCandidate {
            id: CandId(0),
            site: [0.0; 3],
            ball_r: 0.2,
            standoff: 0.05,
            axis: [0.0, 0.0, -1.0],
            neck: 0.1,
            joint_r: 0.25,
            head_len: 2.6,
        };
        let limits = Limits {
            h1_admission_mm2: 0.48,
            h4_lift_mm: 5.0,
            h6_build_gap_mm: 0.32,
            h9_across_flats_mm: 0.102,
            region_length_mm: 3.0,
        };
        let mesh = Mesh::new(
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
            vec![0.0; 3],
            vec![0, 1, 2],
        )
        .unwrap();
        let keep = KeepUpRecord {
            rotation_x_deg: 115.0,
            approved_sha256: String::new(),
        };
        assert_eq!(
            PosedModel::new(&mesh, &keep, &limits).unwrap_err(),
            PoseError::NotBuilt
        );
        let mut out = Vec::new();
        let plan = PlanBuilder::new(
            PosedModel { _private: () },
            limits,
            ProcessRecord {
                layer_height_mm: 0.05,
                layer_origin_mm: 0.0,
            },
        );
        plan.probe(&[cand, cand], &mut out);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|r| matches!(r, Err(Reject::NotBuilt))));
    }
}
