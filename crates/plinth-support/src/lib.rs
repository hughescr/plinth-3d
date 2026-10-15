//! Data boundary for current support and raft behavior.
//!
//! Support planning depends on hollowing, never on emission. The scene retains semantic support
//! kinds and f64 geometry; `plinth-emit` alone maps them to ChituBox record identities and f32.
//!
//! The implemented foundation is intentionally leaf-shaped: this module owns the typed scene
//! boundary and perception; [`forest`] owns generational topology, [`dyadic`] supplies bounded
//! candidate payloads, [`member_sizing`] retains diagnostic parity, and [`raft`] is a frozen
//! semantic foundation rather than an end-to-end parity claim.
//! Model-anchor discovery and final foot validation live in [`anchor`]. A future CCO controller
//! is the authority allowed to connect these leaves. There is no public `run` stub.
#![forbid(unsafe_code)]

/// Compile a census instrumentation site only when the `census` feature is on.
///
/// Wrapping the CALL SITE rather than making the recorder a no-op is the point
/// (MAJOR-8): some sites build real per-row classification vectors to hand the
/// census, and with the feature off none of that work is emitted at all.
/// Invoke by path — `crate::census_record!(...)` — so module declaration order
/// is irrelevant.
macro_rules! census_record {
    ($($body:tt)*) => {
        #[cfg(feature = "census")]
        {
            $($body)*
        }
    };
}
pub(crate) use census_record;

pub mod anchor;
#[allow(
    dead_code,
    reason = "crate-private current-parity bracing is consumed by the pending support runner"
)]
mod brace;
#[allow(
    dead_code,
    reason = "crate-private CCO controller is consumed by the pending support runner integration"
)]
mod cco;
/// THROWAWAY INSTRUMENTATION -- 2026-08-04 embed-incidence census.
///
/// QUARANTINED behind the `census` cargo feature (MAJOR-8).  With the feature
/// off — which is every ordinary build, including `cargo test --workspace` —
/// this module does not exist and `census_record!` expands to nothing, so no
/// instrumentation reaches a production code path.  The census example declares
/// `required-features = ["census"]`, so `cargo run --features census --example
/// embed_incidence_census` still produces the OD-8 delta measurement.
#[cfg(feature = "census")]
pub mod census;
/// Contact physics (PR-1..PR-4): seat depth, mask extent, emergence
/// admissibility and area sufficiency.  Pure and kernel-free by design, so the
/// candidate-gather and clearance loops can call it per member without turning
/// a batched stage into a per-item native call.  Crate-private and never
/// re-exported, matching the sealing `designed_embed` was credited for.
// The allow is scoped and temporary, exactly as for `designed_embed` above: this
// commit lands the physics core and its unit tests as one reviewable unit, and
// the production callers (brace/cco/anchor/initial/rewire/runner) land in the
// commits that follow. The allow comes off with the last of them.
#[allow(
    dead_code,
    reason = "contact physics core lands before its production callers; allow removed as they wire in"
)]
pub(crate) mod contact;
mod coverage;
/// The coverage-closure controller (FU-56): shared blocking predicate,
/// registered request, typed refusal evidence and batch-first nomination.
/// `pub` because [`SupportRunError::CoverageClosure`] carries
/// [`coverage_closure::CoverageClosureRefusal`] in its public error surface,
/// and `plinth_pipeline::gates::registered_coverage_request` delegates to
/// [`coverage_closure::registered_coverage_request`].
pub mod coverage_closure;
mod coverage_measurement;
pub mod coverage_repair;
// Crate-private on purpose: a `pub` mask domain is exactly why dead-code lints
// never fired on it before. The consumers that must mint and read these masks
// (brace/cco/rewire) are gated on open owner decisions, so the allow is scoped
// and temporary.
#[allow(
    dead_code,
    reason = "crate-private designed embed masks await the gated verifier wiring in brace/cco/rewire"
)]
pub(crate) mod designed_embed;
/// The SEALED-INPUT RULE: the one place the digest-memo policy for this crate
/// is written, and the only constructor for a sealed fingerprint.  Read it
/// before adding, moving, or trusting any cached digest.
pub(crate) mod digest_seal;
pub mod dyadic;
pub mod forest;
pub mod hull;
#[allow(
    dead_code,
    reason = "crate-private initial terminal construction is consumed by the pending support runner integration"
)]
pub(crate) mod initial;
#[allow(
    dead_code,
    reason = "crate-private current-parity joint planning is consumed by the pending support runner"
)]
mod joints;
pub mod legacy_compat;
/// **Stage B's vocabulary** — the steering no-good and the confirmation round.
/// Deliberately its own module: the one hard obligation on the mechanism is
/// that the no-good set is never a field of anything a `digest()` walks, and a
/// set that lives in a module of its own, reached only as a borrowed
/// parameter, is easier to keep that way than one folded into the controller.
#[allow(
    dead_code,
    reason = "the vocabulary and its tests land one commit ahead of the attempt-0 loop that \
              mints, holds and drives it — the ordering is deliberate, because the \
              digest-isolation proof this module carries is what makes the loop's termination \
              argument non-circular and it must be green BEFORE the loop exists; \
              docs/specs/seal-semantics.md §\"Stage B — the legalization retry (attempt 0 \
              only)\" pieces 3-5 (\"The abandon outcome\", \"The loop\", \"The confirmation \
              round\") name every reader"
)]
pub(crate) mod legalization;
#[allow(
    dead_code,
    reason = "crate-private current-parity lowering is consumed by the pending support runner"
)]
mod lower;
pub mod member_sizing;
pub mod perception;
#[allow(
    dead_code,
    reason = "crate-private bounded raft planning is consumed by the pending support runner"
)]
pub mod raft;
pub(crate) mod rewire;
#[allow(
    dead_code,
    reason = "crate-private sealed preparation is consumed by the pending support controller"
)]
pub(crate) mod runner;
#[allow(
    dead_code,
    reason = "the frozen seat zone lands one commit ahead of the clearance sites that read it; \
              docs/physics.md §\"6. The seat's own contact zone — the honest exemption\" \
              describes the mechanism they implement"
)]
pub(crate) mod seat_zone;
pub mod specialized;
pub mod transient;

use geom_kernel::vec3::{dot, sub};
use geom_kernel::{NativeSurface, NativeSurfaceConfig, SurfaceBuildError};
use plinth_core::{m0_constants, Finite, PositiveFinite};
use plinth_hollow::HollowHistory;
use plinth_mesh::{build_face_topology, Mesh, WeldPolicy, TOPOLOGY_CUT_CLOSED_SOLID_MASK};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use coverage_closure::{
    coverage_patch_blocks_verdict, coverage_patch_needs_repair,
    registered_coverage_censor_area_mm2, registered_coverage_request, BlockedCause,
    CoverageClosureGap, CoverageClosureIsland, CoverageClosureReason, CoverageClosureRefusal,
    CoverageClosureSetupError, CoverageScoreShare, RouteRepairOutcome,
};
pub use coverage_measurement::{
    CoverageGap, CoverageMeasurement, CoverageMeasurementError, CoverageMeasurementEvidence,
    CoverageMeasurementIslandEvidence, CoverageMeasurementLimits, CoverageMeasurementPatchEvidence,
    CoverageMeasurementRequest, CoverageMeasurementResourcePlan,
};
pub use coverage_repair::{
    repair_step_outcome, ConfirmationRoundDisclosure, ConfirmationRoundOutcome, CoverageRepairStep,
    CoverageRepairStepWire, LegalizationDisclosure, LegalizationDisturbance, NoncoverageComponent,
    NoncoverageComponentSet, NoncoverageVector, OwnContactCondemnation, OwnContactNoWinnerEvidence,
    OwnContactNoWinnerStage, RepairGateVector, RepairPrescreenDisclosure, RepairRunReport,
    RepairStepOutcome, RepairTrailDisclosure, RepairTrialTrail, RepairVeto, RepairVetoClass,
    RepairVetoCounts, RepairVetoDisclosure, RepairVetoIndex, RouteDelta, StageClock,
    StageLine, TRIAL_TRAIL_DIR_ENV,
};
pub use initial::ContactConstructionLimits;
/// The slice-sweep adapter's one geometry door — see `lower.rs`. The rest of
/// `lower` stays crate-private: this exposes the primitive chain, not the
/// forest data model behind it.
pub use lower::{
    carried_ball_radii_mm, drawn_root_radius_mm, drawn_spans, lower_sweep_arms,
    lower_sweep_arms_with_hull_reference,
    model_foot_discs, tip_contact_discs, tip_shaft_seat_bites,
    DrawnSpan, ModelFoot, RootBoss, ShaftSeatBite, SweepArm, SweepArmLanding, SweepLowering,
    SweepHead, SweepLoweringError, SweepRootFlare, TipContactDisc, NATIVE_FOOT_PAD_OVERLAP_MM,
};
pub use hull::{convex_hull, frustum_ring, ConvexHull};
/// TEST-ONLY, behind the quarantined `seat-minimum-probe` feature.
#[cfg(feature = "seat-minimum-probe")]
pub use lower::lower_sweep_arms_with_pinned_seat_minimum;
pub use runner::{
    run_support, AbandonedContactDisclosure, EmittedContactDisclosure, FinalLiveMemberEvidence,
    FinalSupportAuthority, GrownContactTipDisclosure, OwnerlessIslandDisclosure,
    RoutedPhysicalContactEvidence, SupportPreparationInput, SupportPreparationLimits,
    SupportPreparationLimitsError, SupportRunError, SupportRunPhase, SupportVerificationError,
    SupportVerificationSnapshot, TipGrowthRungDisclosure, UnroutedContactReason,
    UnroutedRegularRemoval, UnroutedSpecializedContact,
};
pub use specialized::{RepairRowSource, RepairRowWire};

/// Request data shared by the support and raft pass.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportRequest {
    /// The single lift value used by support placement and later emission.
    pub lift_mm: PositiveFinite,
}

/// Which normal defines capped-frustum end rings. A plate-root column uses
/// horizontal rings even when its centreline is angled; every other rod uses
/// ordinary centreline-normal rings.
///
/// [`Self::Junction`] marks a `Member` record that is not a frustum at all: the
/// convex hull of the end rings of the neighbour frusta it names (the junction
/// hull, [`hull`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrustumRingNormal {
    SegmentAxis,
    PlateZ,
    Junction(JunctionRings),
}

/// One end ring of a neighbour frustum a junction hull is built over: the
/// neighbour's own `start` and `end` (so the emitter makes the ring with the same
/// axis vector and sign the neighbour is drawn with), the ring's radius, and which
/// end of the neighbour it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullRing {
    pub frustum_start_mm: [Finite; 3],
    pub frustum_end_mm: [Finite; 3],
    pub radius_mm: PositiveFinite,
    /// The ring at the neighbour's start (`true`) or its end (`false`).
    pub at_start: bool,
    /// The neighbour's rings are horizontal ([`FrustumRingNormal::PlateZ`]).
    pub plate_z: bool,
}

/// The rings of one junction hull: two or three (the branch, the host below and
/// the host above), unused slots `None`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JunctionRings(pub [Option<HullRing>; 3]);

impl JunctionRings {
    /// The rings that are present, in slot order.
    pub fn rings(&self) -> impl Iterator<Item = &HullRing> {
        self.0.iter().flatten()
    }
}

/// Closed support-domain vocabulary. Wire-format identities belong to `plinth-emit`.
///
/// `RaftPad` and `RaftBridge` are two distinct variants, not one variant plus
/// a geometric test, precisely so no consumer has to infer which raft shape a
/// primitive is from whether its endpoints happen to coincide. The producer
/// in `raft.rs` always knows which one it is building; it says so here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportSegmentKind {
    ContactSphere,
    ContactTaper,
    JointSphere,
    Member,
    RootFlare,
    RaftPad,
    RaftBridge,
}

/// Canonical authority for native sphere records.  The serialized endpoints
/// are a projection of this value, never an alternate geometric input.
#[derive(Clone, Debug, PartialEq)]
pub struct SphereGeometry(SphereGeometryKind);

/// Private so that the only way to create a sphere authority is through its
/// checked constructors.  In particular, safe external Rust cannot smuggle a
/// zero or non-unit contact axis into the emit boundary.
#[derive(Clone, Debug, PartialEq)]
enum SphereGeometryKind {
    Tip {
        center_mm: [Finite; 3],
        outward_axis_unit: [Finite; 3],
        exact_record_poles: Option<(PositiveFinite, [Finite; 3], [Finite; 3])>,
    },
    Joint {
        center_mm: [Finite; 3],
    },
}

impl SphereGeometry {
    /// Normalize an arbitrary finite outward direction once at the typed
    /// boundary, yielding the canonical semantic axis for a type-7 contact.
    pub fn tip(center_mm: [Finite; 3], axis: [Finite; 3]) -> Result<Self, SupportError> {
        let axis = axis.map(Finite::get);
        let length_squared = axis.iter().map(|value| value * value).sum::<f64>();
        if !length_squared.is_finite() || length_squared <= 0.0 {
            return Err(SupportError::InvalidPrimitive);
        }
        let length = length_squared.sqrt();
        let outward_axis_unit = axis
            .map(|value| Finite::new(value / length).map_err(|_| SupportError::InvalidPrimitive))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| SupportError::InvalidPrimitive)?;
        Ok(Self(SphereGeometryKind::Tip {
            center_mm,
            outward_axis_unit,
            exact_record_poles: None,
        }))
    }

    /// Bind a contact sphere to the Python record arithmetic: its metadata poles are authored
    /// directly as `p0` and `p0 + 2*r*u`, rather than reconstructed from the rounded center.
    pub(crate) fn tip_with_exact_record_poles(
        center_mm: [Finite; 3],
        outward_axis_unit: [Finite; 3],
        radius: PositiveFinite,
        start_mm: [Finite; 3],
        end_mm: [Finite; 3],
    ) -> Result<Self, SupportError> {
        let base = Self::tip(center_mm, outward_axis_unit)?;
        let Self(SphereGeometryKind::Tip {
            center_mm,
            outward_axis_unit,
            ..
        }) = base
        else {
            unreachable!("tip constructor always yields tip geometry")
        };
        Ok(Self(SphereGeometryKind::Tip {
            center_mm,
            outward_axis_unit,
            exact_record_poles: Some((radius, start_mm, end_mm)),
        }))
    }

    /// Checked convenience constructor for callers that have not yet crossed
    /// the finite-number boundary.  This deliberately rejects NaN and
    /// infinity rather than allowing them to become geometric authority.
    pub fn tip_from_f64(center_mm: [f64; 3], axis: [f64; 3]) -> Result<Self, SupportError> {
        let center_mm = finite_array(center_mm)?;
        let axis = finite_array(axis)?;
        Self::tip(center_mm, axis)
    }

    #[must_use]
    pub const fn joint(center_mm: [Finite; 3]) -> Self {
        Self(SphereGeometryKind::Joint { center_mm })
    }

    /// Checked f64 convenience constructor matching [`Self::tip_from_f64`].
    pub fn joint_from_f64(center_mm: [f64; 3]) -> Result<Self, SupportError> {
        Ok(Self::joint(finite_array(center_mm)?))
    }

    /// Derive the canonical serialized poles, rejecting finite inputs whose
    /// arithmetic would overflow the finite semantic boundary.
    pub fn canonical_poles(
        &self,
        radius: PositiveFinite,
    ) -> Result<([Finite; 3], [Finite; 3]), SupportError> {
        let radius = radius.get();
        match &self.0 {
            SphereGeometryKind::Tip {
                center_mm,
                outward_axis_unit,
                exact_record_poles,
                ..
            } => {
                if let Some((bound_radius, start, end)) = exact_record_poles {
                    if *bound_radius
                        != PositiveFinite::new(radius)
                            .map_err(|_| SupportError::InvalidPrimitive)?
                    {
                        return Err(SupportError::InvalidPrimitive);
                    }
                    return Ok((*start, *end));
                }
                let axis = outward_axis_unit.map(Finite::get);
                let center = center_mm.map(Finite::get);
                let start = finite_array(std::array::from_fn(|index| {
                    center[index] - radius * axis[index]
                }))?;
                let end = finite_array(std::array::from_fn(|index| {
                    center[index] + radius * axis[index]
                }))?;
                Ok((start, end))
            }
            SphereGeometryKind::Joint { center_mm } => {
                let center = center_mm.map(Finite::get);
                let start = finite_array([center[0], center[1], center[2] + radius])?;
                let end = finite_array([center[0], center[1], center[2] - radius])?;
                Ok((start, end))
            }
        }
    }

    #[must_use]
    pub fn is_tip(&self) -> bool {
        matches!(&self.0, SphereGeometryKind::Tip { .. })
    }

    #[must_use]
    pub fn is_joint(&self) -> bool {
        matches!(&self.0, SphereGeometryKind::Joint { .. })
    }

    /// Immutable, already-normalized semantic authority for independently
    /// checking the type-7 projection.
    #[must_use]
    pub fn tip_authority(&self) -> Option<([Finite; 3], [Finite; 3])> {
        match &self.0 {
            SphereGeometryKind::Tip {
                center_mm,
                outward_axis_unit,
                ..
            } => Some((*center_mm, *outward_axis_unit)),
            SphereGeometryKind::Joint { .. } => None,
        }
    }

    /// Immutable semantic authority for independently checking a type-9
    /// vertical joint sphere.
    #[must_use]
    pub fn joint_center(&self) -> Option<[Finite; 3]> {
        match &self.0 {
            SphereGeometryKind::Tip { .. } => None,
            SphereGeometryKind::Joint { center_mm } => Some(*center_mm),
        }
    }

    fn has_finite_canonical_fields(&self) -> bool {
        let finite = |values: &[Finite; 3]| values.iter().all(|value| value.get().is_finite());
        match &self.0 {
            SphereGeometryKind::Tip {
                center_mm,
                outward_axis_unit,
                exact_record_poles,
                ..
            } => {
                finite(center_mm)
                    && finite(outward_axis_unit)
                    && exact_record_poles
                        .as_ref()
                        .is_none_or(|(_, start, end)| finite(start) && finite(end))
                    && outward_axis_unit
                        .map(Finite::get)
                        .iter()
                        .map(|value| value * value)
                        .sum::<f64>()
                        .is_finite()
            }
            SphereGeometryKind::Joint { center_mm } => finite(center_mm),
        }
    }
}

fn finite_array(values: [f64; 3]) -> Result<[Finite; 3], SupportError> {
    values
        .map(|value| Finite::new(value).map_err(|_| SupportError::InvalidPrimitive))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| SupportError::InvalidPrimitive)
}

/// The one physical frame selected while planning supports.  `plate_z_mm` is
/// expressed in the same un-centred model-local coordinates as every support
/// primitive; the emitter centres both together before serialising.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildFrame {
    plate_z_mm: Finite,
    lift_mm: PositiveFinite,
}

impl BuildFrame {
    #[must_use]
    pub const fn new(plate_z_mm: Finite, lift_mm: PositiveFinite) -> Self {
        Self {
            plate_z_mm,
            lift_mm,
        }
    }

    #[must_use]
    pub const fn plate_z_mm(self) -> Finite {
        self.plate_z_mm
    }

    #[must_use]
    pub const fn lift_mm(self) -> PositiveFinite {
        self.lift_mm
    }
}

/// One semantic f64 support/raft primitive.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportPrimitive {
    pub segment_kind: SupportSegmentKind,
    pub start_mm: [Finite; 3],
    pub end_mm: [Finite; 3],
    pub start_radius_mm: PositiveFinite,
    pub end_radius_mm: PositiveFinite,
    /// The support planner's configured contact embed depth.  It exists only
    /// on a type-1 contact taper and is written verbatim into that record's
    /// trailing field by `plinth-emit`; it is never a format default.
    pub contact_depth_mm: Option<PositiveFinite>,
    /// Explicit native geometry provenance for capped-frustum end rings.
    pub ring_normal: FrustumRingNormal,
    /// Required authority for type-7/type-9 sphere records; absent for
    /// frusta and raft records.
    pub sphere_geometry: Option<SphereGeometry>,
    /// **This primitive is cut to the model mesh** (fix/tip-trim, 2026-09-28;
    /// cut to the mesh, not a plane, Craig 2026-10-12). Present on a seated
    /// arm's tip ball, on the first segment of its shaft, and on every further
    /// `Member` span of that shaft whose solid reaches the model side of the
    /// seat plane (review-fixes 2026-09-29: the drawn profile can cut the shaft
    /// into records inside that band), and on nothing else. The emitter keeps
    /// only the part of the primitive outside the model solid that holds the
    /// shaft end ([`SeatTrim`]); the record's own `start`/`end`/radii are
    /// untouched.
    pub seat_trim: Option<SeatTrim>,
}

/// **THE SEAT A TIP IS CUT TO** (fix/tip-trim, 2026-09-28; the rule changed
/// 2026-10-12): a point on the model face a tip carries, that face's OUTWARD
/// unit normal, and the direction the shaft leaves the ball in.
///
/// *The rule (Craig 2026-10-12: "cut using the actual model mesh").* A tip
/// ball and its arm's shaft (the first segment, and every further span that
/// reaches the model side of the seat plane) keep `primitive ∖ model solid`,
/// and of that only the connected piece that holds the SHAFT END: a piece past
/// the model's far side (the far side of a thin wall the ball straddles, a
/// sliver beyond a ridge) does not touch the shaft, would print as a loose
/// bit, and is dropped and WARNED with its volume. No plane cuts anything. The
/// contact is the model surface inside the ball, wherever that surface is.
///
/// Until 2026-10-12 the kept part was `primitive ∩ {p : (p − point)·n ≥ 0} ∖
/// model solid`. A plane through ONE seat point fits only a flat face: on a
/// curved or tilted underside it removed ball that never touched the model,
/// and the contact the file got was 32 % of plan on Bat_Cat_Owl and 30 % on
/// Deinonychus (`plinth-evidence/.../quality-2026-10-10/joint/
/// TIP-TRUNCATION.md`). The plane survives only as data: the seat law's
/// normal, the planned contact disc ([`crate::TipContactDisc`]), and which
/// shaft spans are cut to the model at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeatTrim {
    point_mm: [Finite; 3],
    outward_normal: [Finite; 3],
    /// The unit direction the shaft leaves the ball in (ball centre → base).
    /// The emitter keeps the piece nearest the shaft end along it.
    shaft_unit: [Finite; 3],
}

impl SeatTrim {
    /// A seat plane through `point_mm` with outward normal `outward`,
    /// normalised here once. The shaft direction defaults to the outward
    /// normal ([`Self::with_shaft_unit`] sets the arm's own).
    ///
    /// # Errors
    ///
    /// [`SupportError::InvalidPrimitive`] for a non-finite point or a zero or
    /// non-finite normal.
    pub fn new(point_mm: [f64; 3], outward: [f64; 3]) -> Result<Self, SupportError> {
        let unit = geom_kernel::vec3::normalize(outward).ok_or(SupportError::InvalidPrimitive)?;
        let unit = finite_array(unit)?;
        Ok(Self {
            point_mm: finite_array(point_mm)?,
            outward_normal: unit,
            shaft_unit: unit,
        })
    }

    /// The same seat with the shaft leaving the ball along `shaft` (normalised
    /// here). A zero or non-finite direction keeps the one it had.
    #[must_use]
    pub fn with_shaft_unit(self, shaft: [f64; 3]) -> Self {
        match geom_kernel::vec3::normalize(shaft).and_then(|unit| finite_array(unit).ok()) {
            Some(shaft_unit) => Self { shaft_unit, ..self },
            None => self,
        }
    }

    #[must_use]
    pub fn point_mm(&self) -> [f64; 3] {
        self.point_mm.map(Finite::get)
    }

    #[must_use]
    pub fn outward_normal(&self) -> [f64; 3] {
        self.outward_normal.map(Finite::get)
    }

    /// The unit direction the shaft leaves the ball in.
    #[must_use]
    pub fn shaft_unit(&self) -> [f64; 3] {
        self.shaft_unit.map(Finite::get)
    }

    /// Signed distance of `p` from the plane: positive on the SUPPORT side,
    /// negative on the model side.
    #[must_use]
    pub fn support_side_distance_mm(&self, p: [f64; 3]) -> f64 {
        geom_kernel::vec3::dot(
            geom_kernel::vec3::sub(p, self.point_mm()),
            self.outward_normal(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportReport {
    pub support_primitive_count: usize,
    pub raft_primitive_count: usize,
}

/// Accumulated report state after support planning; it contains no historical mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportHistory {
    hollow: HollowHistory,
    support: SupportReport,
    frame: BuildFrame,
}

impl SupportHistory {
    /// Builds passive accumulated report state; this does not construct a supported model.
    #[must_use]
    pub fn from_reports(hollow: HollowHistory, support: SupportReport, frame: BuildFrame) -> Self {
        Self {
            hollow,
            support,
            frame,
        }
    }

    #[must_use]
    pub const fn hollow(&self) -> &HollowHistory {
        &self.hollow
    }

    #[must_use]
    pub const fn support(&self) -> &SupportReport {
        &self.support
    }

    /// The sole lift authority shared by support planning and emission.
    #[must_use]
    pub const fn lift_mm(&self) -> PositiveFinite {
        self.frame.lift_mm()
    }

    /// The one plate plane selected by support planning.
    #[must_use]
    pub const fn plate_z_mm(&self) -> Finite {
        self.frame.plate_z_mm()
    }

    #[must_use]
    pub fn into_reports(self) -> (HollowHistory, SupportReport, BuildFrame) {
        (self.hollow, self.support, self.frame)
    }
}

/// Move-only supported scene with one model mesh plus support and raft primitives.
#[derive(Debug)]
pub struct SupportedModel {
    mesh: Mesh,
    primitives: Vec<SupportPrimitive>,
    history: SupportHistory,
}

impl SupportedModel {
    /// Constructs a scene only when its sole mesh, primitive vector, and
    /// auditable report agree. This is the production hand-off constructor for
    /// support planning and the emitter; no caller can manufacture a scene
    /// with stale counts or an invalid source mesh.
    pub fn from_parts(
        mesh: Mesh,
        primitives: Vec<SupportPrimitive>,
        history: SupportHistory,
    ) -> Result<Self, SupportError> {
        mesh.audit_strict()
            .map_err(|_| SupportError::InvalidModelMesh)?;
        let reported = history
            .support()
            .support_primitive_count
            .checked_add(history.support().raft_primitive_count)
            .ok_or(SupportError::InconsistentPrimitiveCount)?;
        if reported != primitives.len() {
            return Err(SupportError::InconsistentPrimitiveCount);
        }
        if primitives
            .iter()
            .any(|primitive| !primitive.is_semantically_valid())
        {
            return Err(SupportError::InvalidPrimitive);
        }
        Ok(Self {
            mesh,
            primitives,
            history,
        })
    }

    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub fn primitives(&self) -> &[SupportPrimitive] {
        &self.primitives
    }

    #[must_use]
    pub const fn history(&self) -> &SupportHistory {
        &self.history
    }

    /// The one lift value selected by support planning and consumed by emit.
    #[must_use]
    pub const fn lift_mm(&self) -> PositiveFinite {
        self.history.lift_mm()
    }

    /// The sole physical plate plane shared by support planning and emission.
    #[must_use]
    pub const fn plate_z_mm(&self) -> Finite {
        self.history.plate_z_mm()
    }
    /// Moves the sole model mesh, all f64 primitives, and report history together.
    #[must_use]
    pub fn into_parts(self) -> (Mesh, Vec<SupportPrimitive>, SupportHistory) {
        (self.mesh, self.primitives, self.history)
    }
}

impl SupportPrimitive {
    fn is_semantically_valid(&self) -> bool {
        let nonzero_frustum = !matches!(
            self.segment_kind,
            SupportSegmentKind::ContactTaper
                | SupportSegmentKind::Member
                | SupportSegmentKind::RootFlare
                | SupportSegmentKind::RaftBridge
        ) || self.start_mm != self.end_mm;
        // A pad is a single point by construction; enforcing that here (rather
        // than only at the `RaftPad`/`RaftBridge` producer) means the emitter
        // can trust `segment_kind` alone and never re-derive the pad/bridge
        // shape from endpoint equality.
        let raft_pad_is_a_point =
            self.segment_kind != SupportSegmentKind::RaftPad || self.start_mm == self.end_mm;
        let taper_depth_matches = (self.segment_kind == SupportSegmentKind::ContactTaper)
            == self.contact_depth_mm.is_some();
        let ring_provenance_matches = match self.ring_normal {
            FrustumRingNormal::SegmentAxis => true,
            FrustumRingNormal::PlateZ => matches!(
                self.segment_kind,
                SupportSegmentKind::Member | SupportSegmentKind::ContactTaper
            ),
            // A junction hull is a `Member` record with at least two rings and
            // no seat trim.
            FrustumRingNormal::Junction(rings) => {
                self.segment_kind == SupportSegmentKind::Member
                    && self.seat_trim.is_none()
                    && rings.rings().count() >= 2
            }
        };
        let sphere_poles_match = match (self.segment_kind, &self.sphere_geometry) {
            (SupportSegmentKind::ContactSphere, Some(geometry)) if geometry.is_tip() => {
                self.start_radius_mm == self.end_radius_mm
                    && geometry.has_finite_canonical_fields()
                    && {
                        geometry
                            .canonical_poles(self.start_radius_mm)
                            .is_ok_and(|(start, end)| self.start_mm == start && self.end_mm == end)
                    }
            }
            (SupportSegmentKind::JointSphere, Some(geometry)) if geometry.is_joint() => {
                self.start_radius_mm == self.end_radius_mm
                    && geometry.has_finite_canonical_fields()
                    && {
                        geometry
                            .canonical_poles(self.start_radius_mm)
                            .is_ok_and(|(start, end)| self.start_mm == start && self.end_mm == end)
                    }
            }
            (SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere, _) => false,
            (_, None) => true,
            _ => false,
        };
        // Only a seated tip and the spans of its shaft are trimmed to a seat
        // face; on a joint, a root flare or a raft record the plane would be a
        // lie about what the member touches.
        let trim_on_a_seated_member = self.seat_trim.is_none()
            || matches!(
                self.segment_kind,
                SupportSegmentKind::ContactSphere
                    | SupportSegmentKind::ContactTaper
                    | SupportSegmentKind::Member
            );
        nonzero_frustum
            && raft_pad_is_a_point
            && taper_depth_matches
            && ring_provenance_matches
            && sphere_poles_match
            && trim_on_a_seated_member
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum SupportError {
    #[error("no legal support and raft plan exists for the model")]
    NoLegalSupportPlan,
    #[error("support planning produced an invalid f64 primitive")]
    InvalidPrimitive,
    #[error("supported model mesh fails the strict production geometry audit")]
    InvalidModelMesh,
    #[error("support report primitive counts do not match the scene primitive vector")]
    InconsistentPrimitiveCount,
    #[error("support planning resource limits must be nonzero")]
    InvalidResourceLimits,
    #[error("support planning input exceeds its caller-owned limit")]
    SupportInputLimit,
    #[error("support planning output exceeds its caller-owned primitive limit")]
    SupportPrimitiveLimit,
    #[error("support planning exceeds its caller-owned scratch-memory limit")]
    SupportScratchLimit,
    #[error("support planning authority is stale or belongs to another filtered live view")]
    StaleSupportAuthority,
}

/// Stable identity of one legal source-face contact seat.
///
/// S0 assigns it directly from the source face id. Later placement can add a
/// local row identity without severing the face provenance this value pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContactCandidateId(pub u64);

/// A legal, exterior, down-facing contact seat on the one support input mesh.
///
/// `normal_mm` is deliberately the native surface normal for `face_id`, never
/// a vertex normal. `supported_area_mm2` and `overhang_excess` are retained as
/// evidence for the subsequent placement/load stages; S0 makes no placement
/// decision from them.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactCandidate {
    pub id: ContactCandidateId,
    pub face_id: u32,
    pub point_mm: [f64; 3],
    pub normal: [f64; 3],
    pub supported_area_mm2: f64,
    pub overhang_excess: f64,
    pub delicacy: f64,
}

/// Immutable source-face candidate pool in deterministic source-face order.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactCandidatePool {
    candidates: Vec<ContactCandidate>,
}

impl ContactCandidatePool {
    #[must_use]
    pub fn candidates(&self) -> &[ContactCandidate] {
        &self.candidates
    }

    /// Stable FNV-1a digest for thread-count and integration checks.
    #[must_use]
    pub fn digest(&self) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut state = OFFSET;
        for candidate in &self.candidates {
            for byte in candidate.id.0.to_le_bytes() {
                state ^= u64::from(byte);
                state = state.wrapping_mul(PRIME);
            }
            for byte in candidate.face_id.to_le_bytes() {
                state ^= u64::from(byte);
                state = state.wrapping_mul(PRIME);
            }
            for value in candidate
                .point_mm
                .iter()
                .chain(candidate.normal.iter())
                .chain(
                    [
                        candidate.supported_area_mm2,
                        candidate.overhang_excess,
                        candidate.delicacy,
                    ]
                    .iter(),
                )
            {
                for byte in value.to_bits().to_le_bytes() {
                    state ^= u64::from(byte);
                    state = state.wrapping_mul(PRIME);
                }
            }
        }
        state
    }
}

/// Perception inputs whose values are not globally registered constants.
///
/// The three behavioral thresholds are read from `plinth-core` by
/// [`SupportPerceptionConfig::current`]. The column pitch is deliberately a
/// local typed field: current Python fixes it at 1 mm but M0 has no registered
/// authority for it yet, so this crate must not invent a second global owner.
#[derive(Clone, Copy, Debug)]
pub struct SupportPerceptionConfig<'a> {
    pub plate_z_mm: Finite,
    pub column_pitch_mm: PositiveFinite,
    pub surface_grid_pitch_mm: PositiveFinite,
    pub max_grid_bytes: usize,
    /// Required face-order sidecar. S0 transports it; it does not recreate a
    /// competing delicacy implementation.
    pub delicacy_by_face: Option<&'a [f64]>,
    overhang_angle_deg: PositiveFinite,
    bridge_free_gap_mm: PositiveFinite,
    near_plate_mm: PositiveFinite,
}

impl<'a> SupportPerceptionConfig<'a> {
    /// Binds the current perception thresholds to their registered M0 values.
    pub fn current(
        plate_z_mm: Finite,
        column_pitch_mm: PositiveFinite,
        surface_grid_pitch_mm: PositiveFinite,
        max_grid_bytes: usize,
        delicacy_by_face: Option<&'a [f64]>,
    ) -> Result<Self, SupportPerceptionError> {
        let constants = m0_constants().map_err(|error| {
            SupportPerceptionError::RegisteredConstantsUnavailable(error.to_string())
        })?;
        let placeholders = constants.placeholders();
        Ok(Self {
            plate_z_mm,
            column_pitch_mm,
            surface_grid_pitch_mm,
            max_grid_bytes,
            delicacy_by_face,
            overhang_angle_deg: *placeholders.overhang_angle_deg().diagnostic(),
            bridge_free_gap_mm: *placeholders.bridge_free_gap_mm().diagnostic(),
            near_plate_mm: *placeholders.near_plate_mm().diagnostic(),
        })
    }
}

/// Named refusal modes for the S0 perception boundary.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum SupportPerceptionError {
    #[error("support perception requires a face-order delicacy sidecar")]
    MissingDelicacySidecar,
    #[error("delicacy sidecar has {actual} rows; support mesh has {expected} faces")]
    DelicacyLength { expected: usize, actual: usize },
    #[error("delicacy sidecar value {index} is not finite")]
    NonFiniteDelicacy { index: usize },
    #[error("support perception rejected strict mesh geometry: {0}")]
    StrictMesh(String),
    #[error("support perception rejected open or invalid topology at face {face} (cut flags {cut_flags:#x})")]
    OpenOrInvalidTopology { face: usize, cut_flags: u8 },
    #[error("support perception could not build the sole native surface: {0}")]
    NativeSurface(String),
    #[error("support perception column coordinate cannot be represented for face {face}")]
    UnqueryableColumn { face: usize },
    #[error("registered M0 constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
}

/// Perceive legal regular-contact seats on the current hollowed or solid-fallback mesh.
///
/// This leaf is intentionally not a support plan and does not expose `run`.
/// It first validates a closed strict mesh, then constructs exactly one
/// `NativeSurface::from_indexed` authority. Candidate face centroids and their
/// column ray origins are gathered before one native batched ray call and
/// scattered back to source-face order.
pub fn perceive_contacts(
    mesh: &Mesh,
    config: &SupportPerceptionConfig<'_>,
) -> Result<ContactCandidatePool, SupportPerceptionError> {
    let (topology, delicacy) = validate_perception_input(mesh, config)?;
    let surface = NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: config.surface_grid_pitch_mm.get(),
            max_grid_bytes: config.max_grid_bytes,
        },
    )
    .map_err(|error| SupportPerceptionError::NativeSurface(error.to_string()))?;
    perceive_contacts_on_surface(mesh, config, &topology, delicacy, &surface)
}

pub(crate) fn validate_perception_input<'a>(
    mesh: &Mesh,
    config: &'a SupportPerceptionConfig<'a>,
) -> Result<(plinth_mesh::FaceTopology, &'a [f64]), SupportPerceptionError> {
    mesh.audit_strict()
        .map_err(|error| SupportPerceptionError::StrictMesh(error.to_string()))?;
    let topology = build_face_topology(mesh, WeldPolicy::ExactBits)
        .map_err(|error| SupportPerceptionError::StrictMesh(error.to_string()))?;
    // The cut classes that refuse here are the closed-solid gate's, read from
    // `plinth_mesh` rather than restated as "any flag". Per the owner ruling
    // of 2026-08-12 (FU-47,
    // `docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`,
    // §4) a point-touch is legitimate sculpting -- two fingertips meeting --
    // and a model that hollows and drills has to be able to reach A5 as well,
    // or the ruling's outcome (such models print) is not delivered. Open,
    // non-manifold, wrongly wound, duplicated and degenerate faces are
    // refused exactly as before: perception's ray work needs a closed
    // oriented surface, and a point-touch is one.
    if let Some((face, &cut_flags)) = topology
        .cut_flags()
        .iter()
        .enumerate()
        .find(|(_, flags)| **flags & TOPOLOGY_CUT_CLOSED_SOLID_MASK != 0)
    {
        return Err(SupportPerceptionError::OpenOrInvalidTopology { face, cut_flags });
    }

    let delicacy = config
        .delicacy_by_face
        .ok_or(SupportPerceptionError::MissingDelicacySidecar)?;
    if delicacy.len() != mesh.tris().len() {
        return Err(SupportPerceptionError::DelicacyLength {
            expected: mesh.tris().len(),
            actual: delicacy.len(),
        });
    }
    if let Some((index, _)) = delicacy
        .iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(SupportPerceptionError::NonFiniteDelicacy { index });
    }

    Ok((topology, delicacy))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the integer \
          bounds and maps NaN to 0, and every site in this function is range-checked before the cast \
          or clamped into range after it (read site by site, 2026-08-26); integer-width casts in \
          mesh/voxel index arithmetic, NOT individually audited -- triaged as a class, and the one \
          architecture item the class carried (geom_kernel's 32-bit BVH index width) is now a named, \
          enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
pub(crate) fn perceive_contacts_on_surface(
    _mesh: &Mesh,
    config: &SupportPerceptionConfig<'_>,
    _topology: &plinth_mesh::FaceTopology,
    delicacy: &[f64],
    surface: &NativeSurface,
) -> Result<ContactCandidatePool, SupportPerceptionError> {
    let geometry = surface.geometry();
    let normals = geometry.normals();
    let centroids = geometry.centroids();
    let areas = geometry.areas();
    let exterior = surface.exterior_face_mask();
    let down_limit = -config.overhang_angle_deg.get().to_radians().sin();
    let plate_z = config.plate_z_mm.get();
    let near_plate = config.near_plate_mm.get();

    let candidates: Vec<usize> = (0..surface.nfaces())
        .filter(|&face| {
            normals[face][2] < down_limit && exterior[face] && {
                let plate_gap = centroids[face][2] - plate_z;
                !(plate_gap >= 0.0 && plate_gap <= near_plate)
            }
        })
        .collect();
    if candidates.is_empty() {
        return Ok(ContactCandidatePool {
            candidates: Vec::new(),
        });
    }

    let xmin = candidates
        .iter()
        .map(|&face| centroids[face][0])
        .fold(f64::INFINITY, f64::min);
    let ymin = candidates
        .iter()
        .map(|&face| centroids[face][1])
        .fold(f64::INFINITY, f64::min);
    let pitch = config.column_pitch_mm.get();
    let mut columns: std::collections::BTreeMap<(i64, i64), Vec<usize>> =
        std::collections::BTreeMap::new();
    for &face in &candidates {
        let x = ((centroids[face][0] - xmin) / pitch).floor();
        let y = ((centroids[face][1] - ymin) / pitch).floor();
        if !x.is_finite()
            || !y.is_finite()
            || x < i64::MIN as f64
            || x > i64::MAX as f64
            || y < i64::MIN as f64
            || y > i64::MAX as f64
        {
            return Err(SupportPerceptionError::UnqueryableColumn { face });
        }
        columns.entry((x as i64, y as i64)).or_default().push(face);
    }
    let bounds = geometry.bounds();
    let ztop = bounds[1][2] + 1.0;
    let t_max = ztop - bounds[0][2] + 1.0;
    let origins: Vec<[f64; 3]> = columns
        .keys()
        .map(|&(x, y)| {
            [
                xmin + (x as f64 + 0.5) * pitch,
                ymin + (y as f64 + 0.5) * pitch,
                ztop,
            ]
        })
        .collect();
    let directions = vec![[0.0, 0.0, -1.0]; origins.len()];
    // `docs/specs/geometry-adjudication.md` §EXT-1 site 3
    // (`support.perception.columns`), disposition RULED in revision 4: a
    // `NearParallel` on a column's ray changes NO `grounded` bit.
    //
    // `grounded[face]` below is a MEASURED-DISTANCE claim -- "a surface exists
    // within the bridge gap under this face" -- and it is an EXEMPTION from the
    // whole support machinery. An undecidable crossing carries no usable `t`
    // (the primitive contract: non-`Hit` products must not cross an API
    // boundary), so affirming `grounded` from one would assert a distance
    // nobody measured, and an exemption may grow only on proof. The skipped
    // crossing is therefore treated as ABSENT -- which is what this arithmetic
    // already does, since an undecidable face contributes no entry to `t_all`
    // -- and the affected faces STAY candidates, routed INTO scrutiny (seat
    // discovery, foot validation, clearance acceptance) rather than out of it.
    // Behaviour here is unchanged by the migration: the classified walk's
    // accepted-hit set is bit-identical to the retired `cast_rays`.
    //
    // The per-column tally is `cast.near_parallel[column]`. It is deliberately
    // NOT carried on `ContactCandidatePool`: by owner ruling the per-model
    // ledger of §EXT-4 is replaced by the kernel's process-global counters
    // (`geom_kernel::telemetry`, site `kernel.walk.collecting`), which answer
    // the run-rate question this tally was raised to answer.
    let cast = surface
        .cast_rays_classified(&origins, &directions, t_max)
        .map_err(|error| SupportPerceptionError::NativeSurface(error.to_string()))?;
    let (offsets, hits) = (cast.offsets, cast.t_all);

    let mut grounded = vec![false; surface.nfaces()];
    for (column, faces) in columns.values().enumerate() {
        let start = offsets[column] as usize;
        let end = offsets[column + 1] as usize;
        for &face in faces {
            let face_z = centroids[face][2];
            let below = hits[start..end]
                .iter()
                .map(|&t| ztop - t)
                .filter(|&z| z < face_z - 1e-3)
                .fold(f64::NEG_INFINITY, f64::max);
            grounded[face] = below.is_finite() && face_z - below <= config.bridge_free_gap_mm.get();
        }
    }

    let candidates = candidates
        .into_iter()
        .filter(|&face| !grounded[face])
        .map(|face| ContactCandidate {
            id: ContactCandidateId(face as u64),
            face_id: face as u32,
            point_mm: centroids[face],
            normal: normals[face],
            supported_area_mm2: areas[face],
            overhang_excess: -normals[face][2] - down_limit.abs(),
            delicacy: delicacy[face],
        })
        .collect();
    Ok(ContactCandidatePool { candidates })
}

// S1 owns coverage policy and its boundary; `dyadic` owns the shared bounded
// traversal and preflight used by coverage and additive candidate payloads.
/// One caller-owned support island.  S1 deliberately consumes this sidecar
/// rather than trying to rediscover islands (which belongs to perception).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportIsland {
    pub id: u32,
    pub face_ids: Vec<u32>,
}

/// The one generic dyadic A3 sample representation used by S1 coverage.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageSample {
    pub island_id: u32,
    pub face_id: u32,
    pub component_id: u32,
    pub point_mm: [f64; 3],
    pub area_mm2: f64,
}

/// A selected, face-owned S0 contact with the evidence S2 needs to route it.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedContact {
    pub island_id: u32,
    pub candidate_id: ContactCandidateId,
    pub served_face_id: u32,
    pub point_mm: [f64; 3],
    pub normal: [f64; 3],
    pub delicacy: f64,
    pub supported_area_mm2: f64,
}

mod selected_contacts;
pub use selected_contacts::SelectedContacts;

/// Current placement policy.  Resource limits remain request fields because
/// M0 records them as non-authoritative guards rather than global constants.
#[derive(Clone, Copy, Debug)]
pub struct ContactSelectionConfig<'a> {
    /// Required S0-delicacy sidecar; it is revalidated at this boundary.
    pub delicacy_by_face: Option<&'a [f64]>,
    /// Explicit integration request for the single dyadic sample builder.
    pub coverage_samples_max_virtual_cells: usize,
    /// Exact compact A3 payload cap (frozen default: 160 MiB).
    pub coverage_samples_max_compact_payload_bytes: usize,
    /// Maximum terminal cells processed by one generation batch.
    pub coverage_samples_max_working_cells: usize,
    /// Conservative cap applied independently to build and validation plans.
    pub coverage_samples_max_peak_bytes: usize,
    /// Explicit integration request for bounded candidate/sample scoring.
    pub coverage_score_matrix_max_entries: usize,
    /// Independent caller-owned cap for the canonical native-surface grid.
    pub surface_max_grid_bytes: usize,
    target_spacing_mm: PositiveFinite,
    spacing_floor_mm: PositiveFinite,
    coverage_radius_factor: PositiveFinite,
    voxel_pitch_mm: PositiveFinite,
    min_contacts_per_island: usize,
    coverage_repair_max_attempts: usize,
}

impl<'a> ContactSelectionConfig<'a> {
    /// Binds all result-changing S1 policy values to the M0 registry.
    pub fn current(
        delicacy_by_face: Option<&'a [f64]>,
        max_coverage_samples: usize,
        max_score_matrix_entries: usize,
        surface_max_grid_bytes: usize,
    ) -> Result<Self, ContactSelectionError> {
        let constants = m0_constants().map_err(|error| {
            ContactSelectionError::RegisteredConstantsUnavailable(error.to_string())
        })?;
        let policy = constants.placeholders();
        Ok(Self {
            delicacy_by_face,
            coverage_samples_max_virtual_cells: max_coverage_samples,
            coverage_samples_max_compact_payload_bytes: 160 * 1024 * 1024,
            coverage_samples_max_working_cells: 65_536,
            coverage_samples_max_peak_bytes: 512 * 1024 * 1024,
            coverage_score_matrix_max_entries: max_score_matrix_entries,
            surface_max_grid_bytes,
            target_spacing_mm: *policy.target_spacing_mm().diagnostic(),
            spacing_floor_mm: *policy.spacing_floor_mm().diagnostic(),
            coverage_radius_factor: *policy.coverage_radius_factor().diagnostic(),
            voxel_pitch_mm: *policy.voxel_pitch_mm().diagnostic(),
            min_contacts_per_island: usize::try_from(
                policy.min_contacts_per_island().diagnostic().get(),
            )
            .map_err(|_| ContactSelectionError::CountNotRepresentable)?,
            coverage_repair_max_attempts: usize::try_from(
                policy.coverage_repair_max_attempts().diagnostic().get(),
            )
            .map_err(|_| ContactSelectionError::CountNotRepresentable)?,
        })
    }

    /// Frozen production refusal defaults from `support_placement.config`.
    pub fn current_defaults(
        delicacy_by_face: Option<&'a [f64]>,
        surface_max_grid_bytes: usize,
    ) -> Result<Self, ContactSelectionError> {
        Self::current(delicacy_by_face, 2_000_000, 65_536, surface_max_grid_bytes)
    }

    /// The registered bound on coverage-closure repair rounds
    /// (`COVERAGE_REPAIR_MAX_ATTEMPTS`, registered value 8). The
    /// coverage-closure controller (FU-56) is this constant's production
    /// reader; the legacy pre-routing repair loop this config also drives
    /// reads the private field directly.
    #[must_use]
    pub const fn coverage_repair_max_attempts(&self) -> usize {
        self.coverage_repair_max_attempts
    }
}

/// Named refusals at the S1 sidecar/coverage boundary.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum ContactSelectionError {
    #[error("contact selection requires a face-order delicacy sidecar")]
    MissingDelicacySidecar,
    #[error(
        "contact-selection delicacy sidecar has {actual} rows; support mesh has {expected} faces"
    )]
    DelicacyLength { expected: usize, actual: usize },
    #[error("contact-selection delicacy sidecar value {index} is not finite")]
    NonFiniteDelicacy { index: usize },
    #[error("contact selection requires at least one support island")]
    MissingIslands,
    #[error("support island {island_id} has no source faces")]
    EmptyIsland { island_id: u32 },
    #[error("support island {island_id} has duplicate or unordered source face {face_id}")]
    NonCanonicalIsland { island_id: u32, face_id: u32 },
    #[error("support island {island_id} names source face {face_id} outside mesh")]
    IslandFaceOutOfRange { island_id: u32, face_id: u32 },
    #[error("S0 candidate {candidate_id:?} has invalid source face {face_id}")]
    CandidateFaceOutOfRange {
        candidate_id: ContactCandidateId,
        face_id: u32,
    },
    #[error("S0 candidate {candidate_id:?} disagrees with required delicacy sidecar")]
    CandidateDelicacyMismatch { candidate_id: ContactCandidateId },
    #[error("S0 candidate {candidate_id:?} is not finite in {field}")]
    CandidateNonFinite {
        candidate_id: ContactCandidateId,
        field: &'static str,
    },
    #[error("S0 candidate {candidate_id:?} has an invalid supported area")]
    CandidateSupportedArea { candidate_id: ContactCandidateId },
    #[error("S0 candidate id {candidate_id:?} is duplicated")]
    DuplicateCandidateId { candidate_id: ContactCandidateId },
    #[error("S0 candidate {candidate_id:?} is not on its source face")]
    CandidatePointOffSourceFace { candidate_id: ContactCandidateId },
    #[error("S0 candidate {candidate_id:?} normal disagrees with its source face")]
    CandidateNormalMismatch { candidate_id: ContactCandidateId },
    #[error("contact selection could not construct the canonical surface: {0}")]
    CandidateSurface(String),
    #[error("canonical surface grid exceeds caller limit {limit} bytes")]
    CandidateSurfaceGridLimit { limit: usize },
    #[error("contact-selection resource request must be positive")]
    InvalidResourceRequest,
    #[error("dyadic coverage requires {required} virtual cells; limit is {limit}")]
    CoverageVirtualCellLimit { required: usize, limit: usize },
    #[error("dyadic coverage compact payload requires {required} bytes; limit is {limit}")]
    CoverageCompactPayloadLimit { required: usize, limit: usize },
    #[error("dyadic coverage {phase} peak requires {required} bytes; limit is {limit}")]
    CoveragePeakLimit {
        phase: &'static str,
        required: usize,
        limit: usize,
    },
    #[error("dyadic coverage resource arithmetic overflowed")]
    CoverageResourceOverflow,
    #[error("contact selection could not build face topology: {0}")]
    Topology(String),
    #[error("support island {island_id} has no legal S0 contact candidate")]
    NoLegalCandidate { island_id: u32 },
    #[error("coverage remains unsatisfied for support island {island_id} after {attempts} repair attempts")]
    UnsatisfiedCoverage { island_id: u32, attempts: usize },
    #[error("registered M0 constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
    #[error("registered count cannot be represented on this platform")]
    CountNotRepresentable,
    #[error("coverage assessment has {actual} distances for {expected} samples")]
    CoverageAssessmentLength { expected: usize, actual: usize },
    #[error("coverage assessment scratch requires {required} bytes; limit is {limit}")]
    CoverageAssessmentScratchLimit { required: usize, limit: usize },
    #[error("coverage assessment requires {required} operations; limit is {limit}")]
    CoverageAssessmentOperationLimit { required: usize, limit: usize },
}

/// Select current regular contacts from S0 candidates and prove A3 coverage.
///
/// Frozen call graph: `engine.prepare` builds `placement.candidate_pool` and
/// `coverage.build_coverage_samples`; `engine.run` calls
/// `placement.sample_candidates`, then `verify.coverage_report`, then bounded
/// `_rank_coverage_repair_rows`.  S0 replaces the candidate-pool build here;
/// S1 ports those three production-called selection/coverage decisions.  It
/// intentionally excludes the dead `_add_fix_contacts` and all CCO/forest,
/// brace, rewire, primitive-meshing, and emit paths.
pub fn select_contacts(
    mesh: &Mesh,
    candidates: &ContactCandidatePool,
    islands: &[SupportIsland],
    config: &ContactSelectionConfig<'_>,
) -> Result<SelectedContacts, ContactSelectionError> {
    // The pre-routing repair loop is retained only as an explicit compatibility
    // adapter while the specialized runner is wired.  New production routing
    // consumes `select_contacts_baseline` plus the immutable repair pool.
    select_contacts_with_work(mesh, candidates, islands, config).map(|(selected, _)| selected)
}

/// Obtain the frozen baseline placement and A3 samples without invoking the
/// legacy pre-routing coverage repair loop.  This reuses the one selection
/// implementation so baseline and compatibility paths cannot drift.
pub(crate) fn select_contacts_baseline(
    mesh: &Mesh,
    candidates: &ContactCandidatePool,
    islands: &[SupportIsland],
    config: &ContactSelectionConfig<'_>,
) -> Result<SelectedContacts, ContactSelectionError> {
    select_contacts_with_mode(mesh, candidates, islands, config, false)
        .map(|(selected, _)| selected)
}

fn select_contacts_with_work(
    mesh: &Mesh,
    candidates: &ContactCandidatePool,
    islands: &[SupportIsland],
    config: &ContactSelectionConfig<'_>,
) -> Result<(SelectedContacts, SelectionWork), ContactSelectionError> {
    select_contacts_with_mode(mesh, candidates, islands, config, true)
}

fn select_contacts_with_mode(
    mesh: &Mesh,
    candidates: &ContactCandidatePool,
    islands: &[SupportIsland],
    config: &ContactSelectionConfig<'_>,
    legacy_pre_routing_repair: bool,
) -> Result<(SelectedContacts, SelectionWork), ContactSelectionError> {
    let delicacy = config
        .delicacy_by_face
        .ok_or(ContactSelectionError::MissingDelicacySidecar)?;
    if delicacy.len() != mesh.tris().len() {
        return Err(ContactSelectionError::DelicacyLength {
            expected: mesh.tris().len(),
            actual: delicacy.len(),
        });
    }
    if let Some((index, _)) = delicacy
        .iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(ContactSelectionError::NonFiniteDelicacy { index });
    }
    if islands.is_empty() {
        return Err(ContactSelectionError::MissingIslands);
    }
    if config.coverage_samples_max_virtual_cells == 0
        || config.coverage_samples_max_compact_payload_bytes == 0
        || config.coverage_samples_max_working_cells == 0
        || config.coverage_samples_max_peak_bytes == 0
        || config.coverage_score_matrix_max_entries == 0
        || config.surface_max_grid_bytes == 0
    {
        return Err(ContactSelectionError::InvalidResourceRequest);
    }
    for island in islands {
        if island.face_ids.is_empty() {
            return Err(ContactSelectionError::EmptyIsland {
                island_id: island.id,
            });
        }
        let mut previous = None;
        for &face_id in &island.face_ids {
            if usize::try_from(face_id)
                .ok()
                .is_none_or(|face| face >= mesh.tris().len())
            {
                return Err(ContactSelectionError::IslandFaceOutOfRange {
                    island_id: island.id,
                    face_id,
                });
            }
            if previous.is_some_and(|old| old >= face_id) {
                return Err(ContactSelectionError::NonCanonicalIsland {
                    island_id: island.id,
                    face_id,
                });
            }
            previous = Some(face_id);
        }
    }
    validate_candidate_pool(mesh, candidates, delicacy, config)?;

    // Build membership authority once. Island face ids are canonical and the
    // final candidate comparator is total (validated ids are unique), so
    // gathering face buckets cannot change frozen selection order.
    let mut work = SelectionWork::default();
    let mut candidates_by_face = vec![Vec::new(); mesh.tris().len()];
    for candidate in candidates.candidates() {
        candidates_by_face[candidate.face_id as usize].push(candidate);
        work.candidate_membership_evaluations =
            work.candidate_membership_evaluations.saturating_add(1);
    }

    let topology = build_face_topology(mesh, WeldPolicy::ExactBits)
        .map_err(|error| ContactSelectionError::Topology(error.to_string()))?;
    let samples = build_dyadic_coverage_samples(mesh, islands, topology.component_ids(), config)?;
    let mut selected: Vec<SelectedContact> = Vec::new();
    for island in islands {
        let eligible_count = island.face_ids.iter().fold(0_usize, |count, &face_id| {
            count.saturating_add(candidates_by_face[face_id as usize].len())
        });
        let mut eligible = Vec::with_capacity(eligible_count);
        for &face_id in &island.face_ids {
            let bucket = &candidates_by_face[face_id as usize];
            work.candidate_membership_evaluations = work
                .candidate_membership_evaluations
                .saturating_add(bucket.len());
            eligible.extend_from_slice(bucket);
        }
        if eligible.is_empty() {
            return Err(ContactSelectionError::NoLegalCandidate {
                island_id: island.id,
            });
        }
        // `sample_candidates`: low Z wins; exact geometric ties retain a
        // deterministic least-scar preference before source identity.
        eligible.sort_by(|left, right| {
            left.point_mm[2]
                .total_cmp(&right.point_mm[2])
                .then_with(|| left.delicacy.total_cmp(&right.delicacy))
                .then_with(|| left.point_mm[0].total_cmp(&right.point_mm[0]))
                .then_with(|| left.point_mm[1].total_cmp(&right.point_mm[1]))
                .then_with(|| left.id.cmp(&right.id))
        });
        let (island_contacts, island_work) = select_island_contacts(
            island.id,
            &eligible,
            config.target_spacing_mm.get(),
            config.min_contacts_per_island,
        );
        work.distance_evaluations = work
            .distance_evaluations
            .saturating_add(island_work.distance_evaluations);
        debug_assert!(config.spacing_floor_mm.get() > 0.0);
        selected.extend(island_contacts);
    }

    let coverage_radius_mm = config.coverage_radius_factor.get() * config.target_spacing_mm.get();
    let mut coverage_index = CoverageContactIndex::new(coverage_radius_mm);
    for contact in &selected {
        coverage_index.insert(contact, &topology, &mut work);
    }
    if !legacy_pre_routing_repair {
        return Ok((SelectedContacts::new(selected, samples), work));
    }
    for attempt in 0..=config.coverage_repair_max_attempts {
        let failing = failing_coverage_samples(
            &samples,
            &coverage_index,
            &topology,
            config.voxel_pitch_mm.get().powi(2),
            CoverageAssessmentLimits {
                max_scratch_bytes: config.coverage_samples_max_peak_bytes,
                max_operations: config
                    .coverage_samples_max_virtual_cells
                    .checked_mul(256)
                    .ok_or(ContactSelectionError::CoverageResourceOverflow)?,
            },
            &mut work,
        )?;
        if failing.is_empty() {
            return Ok((SelectedContacts::new(selected, samples), work));
        }
        if attempt == config.coverage_repair_max_attempts {
            return Err(ContactSelectionError::UnsatisfiedCoverage {
                island_id: samples[failing[0]].island_id,
                attempts: attempt,
            });
        }
        let repair = rank_coverage_repair_rows(
            candidates.candidates(),
            &samples,
            &failing,
            &selected,
            topology.component_ids(),
            coverage_radius_mm,
            config.coverage_score_matrix_max_entries,
        )
        .ok_or(ContactSelectionError::UnsatisfiedCoverage {
            island_id: samples[failing[0]].island_id,
            attempts: attempt,
        })?;
        let repair_contact = selected_contact(repair.0, repair.1);
        coverage_index.insert(&repair_contact, &topology, &mut work);
        selected.push(repair_contact);
    }
    unreachable!("bounded repair loop returns before exhaustion")
}

/// Validate caller-owned S0 evidence once, before selection can sort or score it.
/// The projection is deliberately one gathered batch over the canonical surface.
fn validate_candidate_pool(
    mesh: &Mesh,
    candidates: &ContactCandidatePool,
    delicacy: &[f64],
    config: &ContactSelectionConfig<'_>,
) -> Result<(), ContactSelectionError> {
    let mut ids = std::collections::BTreeSet::new();
    for candidate in candidates.candidates() {
        let face = usize::try_from(candidate.face_id).map_err(|_| {
            ContactSelectionError::CandidateFaceOutOfRange {
                candidate_id: candidate.id,
                face_id: candidate.face_id,
            }
        })?;
        if face >= mesh.tris().len() {
            return Err(ContactSelectionError::CandidateFaceOutOfRange {
                candidate_id: candidate.id,
                face_id: candidate.face_id,
            });
        }
        if candidate.delicacy.to_bits() != delicacy[face].to_bits() {
            return Err(ContactSelectionError::CandidateDelicacyMismatch {
                candidate_id: candidate.id,
            });
        }
        if !ids.insert(candidate.id) {
            return Err(ContactSelectionError::DuplicateCandidateId {
                candidate_id: candidate.id,
            });
        }
        if !candidate.point_mm.iter().all(|value| value.is_finite()) {
            return Err(ContactSelectionError::CandidateNonFinite {
                candidate_id: candidate.id,
                field: "point_mm",
            });
        }
        if !candidate.normal.iter().all(|value| value.is_finite()) {
            return Err(ContactSelectionError::CandidateNonFinite {
                candidate_id: candidate.id,
                field: "normal",
            });
        }
        for (field, value) in [
            ("supported_area_mm2", candidate.supported_area_mm2),
            ("overhang_excess", candidate.overhang_excess),
            ("delicacy", candidate.delicacy),
        ] {
            if !value.is_finite() {
                return Err(ContactSelectionError::CandidateNonFinite {
                    candidate_id: candidate.id,
                    field,
                });
            }
        }
        if candidate.supported_area_mm2 < 0.0 {
            return Err(ContactSelectionError::CandidateSupportedArea {
                candidate_id: candidate.id,
            });
        }
    }
    if candidates.candidates().is_empty() {
        return Ok(());
    }
    let surface = NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: config.voxel_pitch_mm.get(),
            max_grid_bytes: config.surface_max_grid_bytes,
        },
    )
    .map_err(|error| match error {
        SurfaceBuildError::GridBudget { limit_bytes, .. } => {
            ContactSelectionError::CandidateSurfaceGridLimit { limit: limit_bytes }
        }
        other => ContactSelectionError::CandidateSurface(other.to_string()),
    })?;
    let points: Vec<[f64; 3]> = candidates
        .candidates()
        .iter()
        .map(|candidate| candidate.point_mm)
        .collect();
    let (distances, _, _faces, _) = surface.project(&points);
    let canonical_normals = surface.geometry().normals();
    for (row, candidate) in candidates.candidates().iter().enumerate() {
        let face = candidate.face_id as usize;
        // A dyadic nomination may be a shared source vertex.  The nearest
        // projection then has several exact owners and the BVH's deterministic
        // tie winner need not be the nominated source face.  Candidate pools
        // are opaque and crate-built; distance plus the separately checked
        // source-face normal is the correct boundary here.
        if distances[row] > 1.0e-6 {
            return Err(ContactSelectionError::CandidatePointOffSourceFace {
                candidate_id: candidate.id,
            });
        }
        if candidate
            .normal
            .iter()
            .zip(canonical_normals[face])
            .any(|(actual, expected)| (actual - expected).abs() > 1.0e-6)
        {
            return Err(ContactSelectionError::CandidateNormalMismatch {
                candidate_id: candidate.id,
            });
        }
    }
    Ok(())
}

fn selected_contact(island_id: u32, candidate: &ContactCandidate) -> SelectedContact {
    SelectedContact {
        island_id,
        candidate_id: candidate.id,
        served_face_id: candidate.face_id,
        point_mm: candidate.point_mm,
        normal: candidate.normal,
        delicacy: candidate.delicacy,
        supported_area_mm2: candidate.supported_area_mm2,
    }
}

pub(crate) fn build_dyadic_coverage_samples(
    mesh: &Mesh,
    islands: &[SupportIsland],
    components: &[u32],
    config: &ContactSelectionConfig<'_>,
) -> Result<Vec<CoverageSample>, ContactSelectionError> {
    coverage::build_dyadic_coverage_samples(mesh, islands, components, config)
}

#[cfg(test)]
fn build_dyadic_coverage_samples_with_high_water(
    mesh: &Mesh,
    islands: &[SupportIsland],
    components: &[u32],
    config: &ContactSelectionConfig<'_>,
) -> Result<(Vec<CoverageSample>, usize), ContactSelectionError> {
    coverage::build_dyadic_coverage_samples_with_high_water(mesh, islands, components, config)
}

const COVERAGE_SAMPLE_BYTES: usize = 49;
const COVERAGE_PLAN_FACE_BATCH: usize = 2_048;
const COVERAGE_METADATA_BYTES: usize = 330;

#[derive(Clone, Debug, PartialEq, Eq)]
struct CoverageBuildPlan {
    total_cells: usize,
    source_faces: usize,
    working_cells: usize,
    compact_bytes: usize,
    build_peak_bytes: usize,
    validation_peak_bytes: usize,
}

#[cfg(test)]
fn plan_dyadic_coverage(
    mesh: &Mesh,
    islands: &[SupportIsland],
    config: &ContactSelectionConfig<'_>,
) -> Result<CoverageBuildPlan, ContactSelectionError> {
    coverage::plan_dyadic_coverage(mesh, islands, config)
}

fn checked_sum(values: &[usize]) -> Result<usize, ContactSelectionError> {
    values.iter().try_fold(0_usize, |sum, value| {
        sum.checked_add(*value)
            .ok_or(ContactSelectionError::CoverageResourceOverflow)
    })
}

fn checked_mul(left: usize, right: usize) -> Result<usize, ContactSelectionError> {
    left.checked_mul(right)
        .ok_or(ContactSelectionError::CoverageResourceOverflow)
}

fn coverage_compact_bytes(
    cells: usize,
    source_faces: usize,
    islands: usize,
) -> Result<usize, ContactSelectionError> {
    checked_sum(&[
        checked_mul(COVERAGE_SAMPLE_BYTES, cells)?,
        checked_mul(24, source_faces)?,
        checked_mul(8, islands)?,
        COVERAGE_METADATA_BYTES,
    ])
}

fn coverage_build_peak_bytes(
    cells: usize,
    source_faces: usize,
    working_cells: usize,
    islands: usize,
    mesh_faces: usize,
    mesh_vertices: usize,
) -> Result<usize, ContactSelectionError> {
    let retained = coverage_compact_bytes(cells, source_faces, islands)?;
    let planning = checked_sum(&[
        checked_mul(35, source_faces)?,
        checked_mul(256, source_faces.min(COVERAGE_PLAN_FACE_BATCH))?,
    ])?;
    let generation = checked_sum(&[
        checked_mul(49, cells)?,
        checked_mul(9, source_faces)?,
        checked_mul(192, working_cells)?,
    ])?;
    let scratch = [
        checked_mul(24, cells)?,
        checked_mul(24, mesh_faces)?,
        checked_mul(24, mesh_vertices)?,
        checked_mul(8, source_faces)?,
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    let completion = checked_sum(&[
        retained,
        checked_mul(106, cells)?,
        checked_mul(25, source_faces)?,
        checked_mul(16, islands)?,
        16,
        scratch,
    ])?;
    Ok(planning.max(generation).max(completion))
}

fn coverage_validation_peak_bytes(
    cells: usize,
    source_faces: usize,
    islands: usize,
    mesh_faces: usize,
    mesh_vertices: usize,
) -> Result<usize, ContactSelectionError> {
    let retained = coverage_compact_bytes(cells, source_faces, islands)?;
    let scratch = [
        checked_mul(24, cells)?,
        checked_sum(&[
            checked_mul(8, cells)?,
            checked_mul(49, source_faces)?,
            checked_mul(8, islands)?,
        ])?,
        [
            checked_mul(24, cells)?,
            checked_mul(24, mesh_faces)?,
            checked_mul(24, mesh_vertices)?,
            checked_mul(8, source_faces)?,
        ]
        .into_iter()
        .max()
        .unwrap_or(0),
        checked_sum(&[checked_mul(40, cells)?, checked_mul(8, mesh_faces)?])?,
        checked_sum(&[checked_mul(123, cells)?, checked_mul(72, mesh_faces)?])?,
        checked_sum(&[checked_mul(16, cells)?, checked_mul(16, source_faces)?])?,
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    retained
        .checked_add(scratch)
        .ok_or(ContactSelectionError::CoverageResourceOverflow)
}

/// **docs/specs/coverage-repair.md §6 clause 1's objective, in the spec's own order.**
///
/// > the coverage objective strictly improves lexicographically in this order:
/// > total failing area, maximum failing-patch area, failing-patch count,
/// > worst gap
///
/// Censored, because §6 says "failing": a patch counts here only if its
/// uncovered area is above the registered A3 censor (`voxel_pitch_mm²`,
/// 0.16 mm²) or it touches an ambiguous topology cut. Sub-censor dust is
/// §4's "diagnostic only" and may not justify committing a contact.
///
/// `worst_gap_mm` is `+inf` when a failing route has no routed contact at all;
/// `specialized::coverage_key` orders that as worse than any finite distance,
/// which is the reading the pre-write gate already uses.
///
/// Public since the A4 controller: it is the quantity the accept test compares
/// and the quantity the refusal evidence prints, so it crosses the crate
/// boundary into `plinth-pipeline`'s sidecar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CoverageObjective {
    pub failing_uncovered_area_mm2: f64,
    pub max_patch_area_mm2: f64,
    pub failing_patch_count: usize,
    pub worst_gap_mm: f64,
}

pub(crate) struct CoverageAssessment {
    pub(crate) objective: CoverageObjective,
    pub(crate) failing_sample_indices: Vec<usize>,
    pub(crate) patches: Vec<CoverageAssessmentPatch>,
    /// Every sample whose nearest routed contact is farther than the request
    /// radius, ascending, populated ONLY in
    /// [`CoverageAssessmentMode::MeasurementEvidence`].
    ///
    /// **This is the traversal's own `uncovered_indices` scratch vector,
    /// moved rather than copied.** It is already charged in
    /// [`CoverageAssessmentResourcePlan::scratch_bytes`] (one of the four
    /// per-sample `usize` vectors in `traversal_scratch_bytes`) and therefore
    /// in `phase_peak_bytes` and in the measurement's `logical_peak_bytes`;
    /// retaining it past the phase allocates nothing new and raises no peak,
    /// so no charge moves and no ceiling is re-stated. Retained-after-return
    /// bytes stay strictly below the phase peak that admitted them.
    ///
    /// It exists because the closure controller's nomination scoring must
    /// know WHICH samples still need covering: scoring a candidate against
    /// every sample on a blocking patch's faces counts already-covered area
    /// and lets a row near a large covered region outrank the row that closes
    /// the residual gap. That is the same failing-row discipline the legacy
    /// `rank_coverage_repair_rows` already applies, sourced from the one
    /// measurement rather than re-derived beside it.
    pub(crate) uncovered_sample_indices: Vec<usize>,
}

/// The repair controller needs only the objective and failing rows.  The
/// verifier additionally retains a deterministic projection of the same
/// traversal; keeping that choice private prevents evidence allocation from
/// leaking into the repair hot path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoverageAssessmentMode {
    RepairNoEvidence,
    MeasurementEvidence,
}

/// Projection of the existing uncovered-patch traversal.  It is retained
/// alongside (not instead of) the repair objective so repair semantics stay
/// byte-for-byte unchanged.
pub(crate) struct CoverageAssessmentPatch {
    pub(crate) island_id: u32,
    pub(crate) component_id: u32,
    pub(crate) representative_face_id: u32,
    pub(crate) sorted_face_ids: Vec<u32>,
    pub(crate) uncovered_area_mm2: f64,
    pub(crate) cut_ambiguous: bool,
    pub(crate) assessment_failed: bool,
    pub(crate) worst_gap_mm: f64,
}

#[derive(Clone, Copy)]
pub(crate) struct CoverageAssessmentLimits {
    pub(crate) max_scratch_bytes: usize,
    pub(crate) max_operations: usize,
}

#[cfg(test)]
impl CoverageAssessmentLimits {
    pub(crate) const UNBOUNDED: Self = Self {
        max_scratch_bytes: usize::MAX,
        max_operations: usize::MAX,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CoverageAssessmentResourcePlan {
    /// Traversal-owned vectors.  This remains the repair accounting contract.
    pub(crate) scratch_bytes: usize,
    /// Retained traversal evidence which is live after the scratch phase.
    pub(crate) evidence_bytes: usize,
    /// The assessment phase high-water mark, including evidence conversion.
    pub(crate) phase_peak_bytes: usize,
    pub(crate) operations: usize,
}

#[derive(Clone, Copy)]
struct UncoveredFaceGroup {
    island_id: u32,
    face_id: u32,
    area_mm2: f64,
    sample_start: usize,
    sample_end: usize,
}

pub(crate) fn coverage_assessment_resource_plan(
    samples: &[CoverageSample],
    topology: &plinth_mesh::FaceTopology,
) -> Result<CoverageAssessmentResourcePlan, ContactSelectionError> {
    coverage_assessment_resource_plan_for(
        samples,
        topology,
        CoverageAssessmentMode::RepairNoEvidence,
    )
}

pub(crate) fn coverage_measurement_assessment_resource_plan(
    samples: &[CoverageSample],
    topology: &plinth_mesh::FaceTopology,
    island_row_count: usize,
) -> Result<CoverageAssessmentResourcePlan, ContactSelectionError> {
    coverage_assessment_resource_plan_for(
        samples,
        topology,
        CoverageAssessmentMode::MeasurementEvidence,
    )
    .and_then(|plan| {
        // A flat `(island, component, face)` row is sorted/deduplicated before
        // its face storage is transferred into final island evidence.  The
        // caller supplies the exact preflighted row count because membership
        // belongs to the sealed snapshot, not this shared traversal.
        let island_rows = island_row_count
            .checked_mul(std::mem::size_of::<(u32, u32, u32)>())
            .ok_or(ContactSelectionError::CoverageResourceOverflow)?;
        let island_faces = island_row_count
            .checked_mul(std::mem::size_of::<u32>())
            .ok_or(ContactSelectionError::CoverageResourceOverflow)?;
        let island_evidence = island_row_count
            .checked_mul(std::mem::size_of::<
                crate::coverage_measurement::CoverageMeasurementIslandEvidence,
            >())
            .ok_or(ContactSelectionError::CoverageResourceOverflow)?;
        let evidence_bytes = plan
            .evidence_bytes
            .checked_add(island_rows)
            .and_then(|bytes| bytes.checked_add(island_faces))
            .and_then(|bytes| bytes.checked_add(island_evidence))
            .ok_or(ContactSelectionError::CoverageResourceOverflow)?;
        let phase_peak_bytes = plan
            .scratch_bytes
            .checked_add(evidence_bytes)
            .ok_or(ContactSelectionError::CoverageResourceOverflow)?;
        Ok(CoverageAssessmentResourcePlan {
            evidence_bytes,
            phase_peak_bytes,
            ..plan
        })
    })
}

fn coverage_assessment_resource_plan_for(
    samples: &[CoverageSample],
    topology: &plinth_mesh::FaceTopology,
    mode: CoverageAssessmentMode,
) -> Result<CoverageAssessmentResourcePlan, ContactSelectionError> {
    let count = samples.len();
    let traversal_scratch_bytes = count
        .checked_mul(
            std::mem::size_of::<usize>() * 4
                + std::mem::size_of::<u32>()
                + std::mem::size_of::<u8>()
                + std::mem::size_of::<UncoveredFaceGroup>(),
        )
        .ok_or(ContactSelectionError::CoverageResourceOverflow)?;
    let log = usize::BITS as usize - count.max(1).leading_zeros() as usize;
    let neighbor_visits = samples.iter().try_fold(0usize, |total, sample| {
        total
            .checked_add(
                topology
                    .neighbors(sample.face_id as usize)
                    .unwrap_or(&[])
                    .len(),
            )
            .ok_or(ContactSelectionError::CoverageResourceOverflow)
    })?;
    let operations = count
        .checked_mul(log.saturating_mul(4).saturating_add(4))
        .and_then(|work| {
            neighbor_visits
                .checked_mul(log.max(1))
                .and_then(|neighbors| work.checked_add(neighbors))
        })
        .ok_or(ContactSelectionError::CoverageResourceOverflow)?;
    let (scratch_bytes, evidence_bytes) = match mode {
        CoverageAssessmentMode::RepairNoEvidence => (traversal_scratch_bytes, 0),
        CoverageAssessmentMode::MeasurementEvidence => (
            traversal_scratch_bytes
                .checked_add(
                    count
                        .checked_mul(std::mem::size_of::<usize>())
                        .ok_or(ContactSelectionError::CoverageResourceOverflow)?,
                )
                .ok_or(ContactSelectionError::CoverageResourceOverflow)?,
            count
                .checked_mul(std::mem::size_of::<CoverageAssessmentPatch>())
                .and_then(|bytes| {
                    count
                        .checked_mul(std::mem::size_of::<u32>())
                        .and_then(|faces| bytes.checked_add(faces))
                })
                // Final patch rows coexist with the traversal rows while the face
                // buffers are moved, rather than copied, into immutable evidence.
                .and_then(|bytes| {
                    count
                        .checked_mul(std::mem::size_of::<
                            crate::coverage_measurement::CoverageMeasurementPatchEvidence,
                        >())
                        .and_then(|rows| bytes.checked_add(rows))
                })
                .ok_or(ContactSelectionError::CoverageResourceOverflow)?,
        ),
    };
    let phase_peak_bytes = scratch_bytes
        .checked_add(evidence_bytes)
        .ok_or(ContactSelectionError::CoverageResourceOverflow)?;
    Ok(CoverageAssessmentResourcePlan {
        scratch_bytes,
        evidence_bytes,
        phase_peak_bytes,
        operations,
    })
}

pub(crate) fn assess_coverage(
    samples: &[CoverageSample],
    topology: &plinth_mesh::FaceTopology,
    nearest_mm: &[f64],
    radius_mm: f64,
    area_floor_mm2: f64,
    limits: CoverageAssessmentLimits,
) -> Result<CoverageAssessment, ContactSelectionError> {
    assess_coverage_with_mode(
        samples,
        topology,
        nearest_mm,
        radius_mm,
        area_floor_mm2,
        limits,
        CoverageAssessmentMode::RepairNoEvidence,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn assess_coverage_with_mode(
    samples: &[CoverageSample],
    topology: &plinth_mesh::FaceTopology,
    nearest_mm: &[f64],
    radius_mm: f64,
    area_floor_mm2: f64,
    limits: CoverageAssessmentLimits,
    mode: CoverageAssessmentMode,
) -> Result<CoverageAssessment, ContactSelectionError> {
    if nearest_mm.len() != samples.len() {
        return Err(ContactSelectionError::CoverageAssessmentLength {
            expected: samples.len(),
            actual: nearest_mm.len(),
        });
    }
    let plan = coverage_assessment_resource_plan_for(samples, topology, mode)?;
    if plan.phase_peak_bytes > limits.max_scratch_bytes {
        return Err(ContactSelectionError::CoverageAssessmentScratchLimit {
            required: plan.phase_peak_bytes,
            limit: limits.max_scratch_bytes,
        });
    }
    if plan.operations > limits.max_operations {
        return Err(ContactSelectionError::CoverageAssessmentOperationLimit {
            required: plan.operations,
            limit: limits.max_operations,
        });
    }
    let mut uncovered_indices = Vec::with_capacity(samples.len());
    for (sample_index, &nearest) in nearest_mm.iter().enumerate() {
        if nearest > radius_mm {
            uncovered_indices.push(sample_index);
        }
    }
    uncovered_indices
        .sort_unstable_by_key(|&index| (samples[index].island_id, samples[index].face_id, index));
    let mut groups = Vec::with_capacity(samples.len());
    let mut start = 0usize;
    while start < uncovered_indices.len() {
        let first = &samples[uncovered_indices[start]];
        let mut end = start + 1;
        let mut area = first.area_mm2;
        while end < uncovered_indices.len() {
            let sample = &samples[uncovered_indices[end]];
            if (sample.island_id, sample.face_id) != (first.island_id, first.face_id) {
                break;
            }
            area += sample.area_mm2;
            end += 1;
        }
        groups.push(UncoveredFaceGroup {
            island_id: first.island_id,
            face_id: first.face_id,
            area_mm2: area,
            sample_start: start,
            sample_end: end,
        });
        start = end;
    }
    let mut assessment = CoverageAssessment {
        objective: CoverageObjective::default(),
        failing_sample_indices: Vec::with_capacity(samples.len()),
        uncovered_sample_indices: Vec::new(),
        patches: match mode {
            CoverageAssessmentMode::RepairNoEvidence => Vec::new(),
            CoverageAssessmentMode::MeasurementEvidence => Vec::with_capacity(samples.len()),
        },
    };
    let mut failing_islands = Vec::with_capacity(samples.len());
    let mut visited = vec![false; groups.len()];
    let mut stack = Vec::with_capacity(samples.len());
    let mut patch_samples = Vec::with_capacity(samples.len());
    let mut patch_groups = match mode {
        CoverageAssessmentMode::RepairNoEvidence => Vec::new(),
        CoverageAssessmentMode::MeasurementEvidence => Vec::with_capacity(samples.len()),
    };
    for group_index in 0..groups.len() {
        if visited[group_index] {
            continue;
        }
        visited[group_index] = true;
        stack.clear();
        stack.push(group_index);
        let mut patch_area = 0.0;
        let mut patch_ambiguous = false;
        patch_samples.clear();
        patch_groups.clear();
        while let Some(current_index) = stack.pop() {
            let current = groups[current_index];
            patch_area += current.area_mm2;
            patch_ambiguous |= topology.cut_flags()[current.face_id as usize] != 0;
            patch_samples
                .extend_from_slice(&uncovered_indices[current.sample_start..current.sample_end]);
            if mode == CoverageAssessmentMode::MeasurementEvidence {
                patch_groups.push(current_index);
            }
            for &neighbor in topology.neighbors(current.face_id as usize).unwrap_or(&[]) {
                if let Ok(neighbor_index) = groups
                    .binary_search_by_key(&(current.island_id, neighbor), |group| {
                        (group.island_id, group.face_id)
                    })
                {
                    if !visited[neighbor_index] {
                        visited[neighbor_index] = true;
                        stack.push(neighbor_index);
                    }
                }
            }
        }
        if patch_area > area_floor_mm2 || patch_ambiguous {
            assessment.objective.failing_uncovered_area_mm2 += patch_area;
            assessment.objective.max_patch_area_mm2 =
                assessment.objective.max_patch_area_mm2.max(patch_area);
            assessment.objective.failing_patch_count =
                assessment.objective.failing_patch_count.saturating_add(1);
            assessment
                .failing_sample_indices
                .extend_from_slice(&patch_samples);
            failing_islands.push(groups[group_index].island_id);
        }
        if mode == CoverageAssessmentMode::MeasurementEvidence {
            // Groups are unique `(island, face)` rows and a patch is confined
            // to one island, so this produces one face ID per group without a
            // duplicate buffer or a dedup/shrink pass.
            let mut face_ids = Vec::with_capacity(patch_groups.len());
            face_ids.extend(patch_groups.iter().map(|&index| groups[index].face_id));
            face_ids.sort_unstable();
            let representative_face_id = *face_ids.first().expect("uncovered patch has a face");
            let component_id =
                samples[*patch_samples.first().expect("uncovered patch has a sample")].component_id;
            let worst_gap_mm = patch_samples
                .iter()
                .fold(0.0_f64, |worst, &index| worst.max(nearest_mm[index]));
            assessment.patches.push(CoverageAssessmentPatch {
                island_id: groups[group_index].island_id,
                component_id,
                representative_face_id,
                sorted_face_ids: face_ids,
                uncovered_area_mm2: patch_area,
                cut_ambiguous: patch_ambiguous,
                assessment_failed: patch_area > area_floor_mm2 || patch_ambiguous,
                worst_gap_mm,
            });
        }
    }
    assessment.failing_sample_indices.sort_unstable();
    if mode == CoverageAssessmentMode::MeasurementEvidence {
        // The traversal is finished with `uncovered_indices` (its group
        // slices were consumed above), so this is a move of an
        // already-charged buffer, not a new allocation. Sorted ascending so
        // a consumer can binary-search it and so two runs over the same
        // measurement produce the identical list.
        uncovered_indices.sort_unstable();
        assessment.uncovered_sample_indices = uncovered_indices;
    }
    failing_islands.sort_unstable();
    failing_islands.dedup();
    assessment.objective.worst_gap_mm = samples
        .iter()
        .zip(nearest_mm)
        .filter_map(|(sample, nearest)| {
            failing_islands
                .binary_search(&sample.island_id)
                .is_ok()
                .then_some(*nearest)
        })
        .fold(0.0, f64::max);
    Ok(assessment)
}

fn failing_coverage_samples(
    samples: &[CoverageSample],
    contacts: &CoverageContactIndex,
    topology: &plinth_mesh::FaceTopology,
    area_floor_mm2: f64,
    limits: CoverageAssessmentLimits,
    work: &mut SelectionWork,
) -> Result<Vec<usize>, ContactSelectionError> {
    let nearest = samples
        .iter()
        .map(|sample| {
            if contacts.covers(sample, work) {
                0.0
            } else {
                f64::INFINITY
            }
        })
        .collect::<Vec<_>>();
    Ok(assess_coverage(
        samples,
        topology,
        &nearest,
        contacts.radius_mm,
        area_floor_mm2,
        limits,
    )?
    .failing_sample_indices)
}

#[allow(
    clippy::float_cmp,
    reason = "a determinism tie-break, not a geometric decision: when two \
              accumulated scores are bit-equal the lowest row index wins.  An \
              epsilon here would be a magic number deciding which candidate is \
              chosen, which the physics-first constants rule forbids"
)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the integer \
          bounds and maps NaN to 0, and every site in this function is range-checked before the cast \
          or clamped into range after it (read site by site, 2026-08-26). See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
pub(crate) fn rank_coverage_repair_rows<'a>(
    candidates: &'a [ContactCandidate],
    samples: &[CoverageSample],
    failing: &[usize],
    selected: &[SelectedContact],
    components: &[u32],
    radius_mm: f64,
    matrix_budget: usize,
) -> Option<(u32, &'a ContactCandidate)> {
    let radius_sq = radius_mm * radius_mm;
    let mut best: Option<(f64, usize, u32)> = None;
    // The score matrix remains tiled in both axes; 65,536 is the registered
    // production work bound even when a caller supplied a larger diagnostic
    // cap.
    let matrix_budget = matrix_budget.clamp(1, 65_536);
    let sample_tile = ((matrix_budget as f64).sqrt() as usize).max(1);
    let candidate_tile = (matrix_budget / sample_tile).max(1);
    let mut route_keys = std::collections::BTreeSet::new();
    for &row in failing {
        route_keys.insert((samples[row].island_id, samples[row].component_id));
    }
    for (island_id, component_id) in route_keys {
        let candidate_rows: Vec<usize> = candidates
            .iter()
            .enumerate()
            .filter_map(|(row, candidate)| {
                (components[candidate.face_id as usize] == component_id
                    && samples.iter().any(|sample| {
                        sample.island_id == island_id && sample.face_id == candidate.face_id
                    })
                    && !selected.iter().any(|contact| {
                        contact.island_id == island_id && contact.candidate_id == candidate.id
                    }))
                .then_some(row)
            })
            .collect();
        let sample_rows: Vec<usize> = failing
            .iter()
            .copied()
            .filter(|&row| {
                samples[row].island_id == island_id && samples[row].component_id == component_id
            })
            .collect();
        for candidate_block in candidate_rows.chunks(candidate_tile) {
            let mut scores = vec![0.0; candidate_block.len()];
            for sample_block in sample_rows.chunks(sample_tile) {
                debug_assert!(candidate_block.len() * sample_block.len() <= matrix_budget);
                for (candidate_offset, &candidate_row) in candidate_block.iter().enumerate() {
                    for &sample_row in sample_block {
                        let sample = &samples[sample_row];
                        if squared_distance(candidates[candidate_row].point_mm, sample.point_mm)
                            <= radius_sq
                        {
                            scores[candidate_offset] += sample.area_mm2;
                        }
                    }
                }
            }
            for (candidate_offset, &candidate_row) in candidate_block.iter().enumerate() {
                let score = scores[candidate_offset];
                if best.is_none_or(|(best_score, best_row, _)| {
                    score > best_score || (score == best_score && candidate_row < best_row)
                }) {
                    best = Some((score, candidate_row, island_id));
                }
            }
        }
    }
    best.map(|(_, candidate_row, island_id)| (island_id, &candidates[candidate_row]))
}

/// Squared distance `|left - right|^2` in mm^2, plain (no FMA).
///
/// Canon item C7 (`docs/specs/geometry-canon.md` §2 step 2(c), §1 OP-4/OP-9):
/// this was a `mul_add` chain. Every caller compares the result against a
/// squared radius, so the fused form was a second definition of "near" a few
/// ULP away from the kernel's, inside a crate whose house rule
/// (`rust/geom_kernel/src/vec3.rs:1-5`, spec sec 5.1) forbids FMA.
fn squared_distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let d = sub(left, right);
    dot(d, d)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SelectionWork {
    candidate_membership_evaluations: usize,
    distance_evaluations: usize,
    coverage_index_rows: usize,
    coverage_distance_evaluations: usize,
}

type SpatialCell = (i64, i64, i64);
type CoverageCells = std::collections::BTreeMap<SpatialCell, Vec<[f64; 3]>>;

struct CoverageContactIndex {
    radius_mm: f64,
    groups: std::collections::BTreeMap<(u32, u32), CoverageCells>,
}

impl CoverageContactIndex {
    fn new(radius_mm: f64) -> Self {
        Self {
            radius_mm,
            groups: std::collections::BTreeMap::new(),
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the \
              integer bounds and maps NaN to 0, and every site in this function is range-checked \
              before the cast or clamped into range after it (read site by site, 2026-08-26). See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn key(&self, point: [f64; 3]) -> (i64, i64, i64) {
        let cell = |coordinate: f64| (coordinate / self.radius_mm).floor() as i64;
        (cell(point[0]), cell(point[1]), cell(point[2]))
    }

    fn insert(
        &mut self,
        contact: &SelectedContact,
        topology: &plinth_mesh::FaceTopology,
        work: &mut SelectionWork,
    ) {
        let component_id = topology.component_ids()[contact.served_face_id as usize];
        let cell = self.key(contact.point_mm);
        self.groups
            .entry((contact.island_id, component_id))
            .or_default()
            .entry(cell)
            .or_default()
            .push(contact.point_mm);
        work.coverage_index_rows = work.coverage_index_rows.saturating_add(1);
    }

    fn covers(&self, sample: &CoverageSample, work: &mut SelectionWork) -> bool {
        let Some(cells) = self.groups.get(&(sample.island_id, sample.component_id)) else {
            return false;
        };
        let (x, y, z) = self.key(sample.point_mm);
        for nx in x.saturating_sub(1)..=x.saturating_add(1) {
            for ny in y.saturating_sub(1)..=y.saturating_add(1) {
                for nz in z.saturating_sub(1)..=z.saturating_add(1) {
                    if let Some(points) = cells.get(&(nx, ny, nz)) {
                        for &point in points {
                            work.coverage_distance_evaluations =
                                work.coverage_distance_evaluations.saturating_add(1);
                            // Preserve the frozen closed radius and square-root
                            // comparison exactly at the coverage boundary.
                            if squared_distance(point, sample.point_mm).sqrt() <= self.radius_mm {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }
}

/// Incremental fixed-radius index for the ordered Poisson pass. A point closer
/// than `cell_width` can only occupy its own cell or one of the 26 neighbours,
/// so selection does not rescan the growing output for every candidate.
struct PoissonGrid {
    cell_width: f64,
    cells: std::collections::BTreeMap<(i64, i64, i64), Vec<[f64; 3]>>,
}

impl PoissonGrid {
    fn new(cell_width: f64) -> Self {
        Self {
            cell_width,
            cells: std::collections::BTreeMap::new(),
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the \
              integer bounds and maps NaN to 0, and every site in this function is range-checked \
              before the cast or clamped into range after it (read site by site, 2026-08-26). See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn key(&self, point: [f64; 3]) -> (i64, i64, i64) {
        let cell = |coordinate: f64| (coordinate / self.cell_width).floor() as i64;
        (cell(point[0]), cell(point[1]), cell(point[2]))
    }

    fn accepts(&self, point: [f64; 3], work: &mut SelectionWork) -> bool {
        let (x, y, z) = self.key(point);
        let x0 = x.saturating_sub(1);
        let x1 = x.saturating_add(1);
        let y0 = y.saturating_sub(1);
        let y1 = y.saturating_add(1);
        let z0 = z.saturating_sub(1);
        let z1 = z.saturating_add(1);
        for nx in x0..=x1 {
            for ny in y0..=y1 {
                for nz in z0..=z1 {
                    if let Some(points) = self.cells.get(&(nx, ny, nz)) {
                        for &selected in points {
                            work.distance_evaluations = work.distance_evaluations.saturating_add(1);
                            // Preserve the frozen comparison, including its square-root
                            // rounding at an exact spacing boundary.
                            if squared_distance(selected, point).sqrt() < self.cell_width {
                                return false;
                            }
                        }
                    }
                }
            }
        }
        true
    }

    fn insert(&mut self, point: [f64; 3]) {
        self.cells.entry(self.key(point)).or_default().push(point);
    }
}

fn select_island_contacts(
    island_id: u32,
    eligible: &[&ContactCandidate],
    target_spacing_mm: f64,
    min_contacts_per_island: usize,
) -> (Vec<SelectedContact>, SelectionWork) {
    let mut work = SelectionWork::default();
    let mut grid = PoissonGrid::new(target_spacing_mm);
    let mut chosen = vec![false; eligible.len()];
    let mut contacts = Vec::new();

    for (row, candidate) in eligible.iter().enumerate() {
        if grid.accepts(candidate.point_mm, &mut work) {
            chosen[row] = true;
            grid.insert(candidate.point_mm);
            contacts.push(selected_contact(island_id, candidate));
        }
    }

    let required = min_contacts_per_island.min(eligible.len());
    if contacts.len() >= required {
        return (contacts, work);
    }

    // The Poisson result has fewer than the fixed policy minimum here. Compute
    // each candidate's nearest selected distance once, then update that cache
    // only against each newly inserted fallback. This preserves the frozen
    // farthest-point ordering without repeatedly rescanning selected contacts.
    let mut nearest_distance_sq = vec![f64::INFINITY; eligible.len()];
    for (row, candidate) in eligible.iter().enumerate() {
        if chosen[row] {
            continue;
        }
        for contact in &contacts {
            work.distance_evaluations = work.distance_evaluations.saturating_add(1);
            nearest_distance_sq[row] = nearest_distance_sq[row]
                .min(squared_distance(contact.point_mm, candidate.point_mm));
        }
    }

    while contacts.len() < required {
        let best_row = (0..eligible.len())
            .filter(|&row| !chosen[row])
            .max_by(|&left, &right| {
                nearest_distance_sq[left]
                    .total_cmp(&nearest_distance_sq[right])
                    .then_with(|| eligible[right].id.cmp(&eligible[left].id))
            })
            .expect("minimum fallback has an unselected legal candidate");
        chosen[best_row] = true;
        let best = eligible[best_row];
        contacts.push(selected_contact(island_id, best));
        if contacts.len() == required {
            break;
        }
        for (row, candidate) in eligible.iter().enumerate() {
            if chosen[row] {
                continue;
            }
            work.distance_evaluations = work.distance_evaluations.saturating_add(1);
            nearest_distance_sq[row] =
                nearest_distance_sq[row].min(squared_distance(best.point_mm, candidate.point_mm));
        }
    }
    (contacts, work)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_decimate::{run as run_decimation, DecimationAudit, DecimationBudget, ProbeConfig};
    use plinth_hollow::{
        HollowDisposition, HollowHistory, HollowReport, HollowedModel, SolidDecisionReason,
    };
    use plinth_orient::{
        BaselineSelection, OrientationHistory, OrientationProvenance, OrientationReport,
        RigidTransform,
    };
    use std::path::PathBuf;

    fn fixture_decimation_audit() -> DecimationAudit {
        let mesh = plinth_mesh::make_box([10.0; 3], [0.0; 3]).expect("closed audit fixture");
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
                .expect("registered fixture budget");
        let outcome = run_decimation(&mesh, budget).expect("fixture decimation succeeds");
        let (_, audit) = outcome
            .into_decimated_model()
            .expect("fixture scale is in contract")
            .into_parts();
        audit
    }

    const TEST_GRID_BUDGET_BYTES: usize = 64 * 1024 * 1024;

    fn config<'a>(_mesh: &Mesh, delicacy: &'a [f64]) -> SupportPerceptionConfig<'a> {
        SupportPerceptionConfig::current(
            Finite::new(-5.0).expect("finite fixture plate"),
            PositiveFinite::new(1.0).expect("positive fixture column pitch"),
            PositiveFinite::new(0.5).expect("positive fixture native-grid pitch"),
            TEST_GRID_BUDGET_BYTES,
            Some(delicacy),
        )
        .expect("registered M0 constants are available")
    }

    fn perceive_fixture(mesh: &Mesh) -> ContactCandidatePool {
        let delicacy = vec![0.25; mesh.tris().len()];
        perceive_contacts(mesh, &config(mesh, &delicacy)).expect("closed fixture is perceptible")
    }

    fn fixture_history(primitives: &[SupportPrimitive]) -> SupportHistory {
        let decimation = fixture_decimation_audit();
        let orientation = OrientationHistory::from_reports(
            decimation,
            OrientationReport {
                provenance: OrientationProvenance::Pinned {
                    pinned_report: PathBuf::from("fixture.pipeline.json"),
                },
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        );
        let hollow = HollowHistory::from_reports(
            orientation,
            HollowReport::solid_decision(SolidDecisionReason::BelowThreshold),
        );
        let raft_primitive_count = primitives
            .iter()
            .filter(|primitive| {
                matches!(
                    primitive.segment_kind,
                    SupportSegmentKind::RaftPad | SupportSegmentKind::RaftBridge
                )
            })
            .count();
        SupportHistory::from_reports(
            hollow,
            SupportReport {
                support_primitive_count: primitives.len() - raft_primitive_count,
                raft_primitive_count,
            },
            BuildFrame::new(
                Finite::new(-3.0).expect("fixture plate"),
                PositiveFinite::new(3.0).expect("fixture lift"),
            ),
        )
    }

    fn non_sphere_primitive(kind: SupportSegmentKind, point: [f64; 3]) -> SupportPrimitive {
        non_sphere_primitive_span(kind, point, point)
    }

    fn non_sphere_primitive_span(
        kind: SupportSegmentKind,
        start: [f64; 3],
        end: [f64; 3],
    ) -> SupportPrimitive {
        SupportPrimitive {
            segment_kind: kind,
            start_mm: start.map(|value| Finite::new(value).expect("fixture coordinate")),
            end_mm: end.map(|value| Finite::new(value).expect("fixture coordinate")),
            start_radius_mm: PositiveFinite::new(1.0).expect("fixture radius"),
            end_radius_mm: PositiveFinite::new(0.5).expect("fixture radius"),
            contact_depth_mm: (kind == SupportSegmentKind::ContactTaper)
                .then(|| PositiveFinite::new(0.2).expect("fixture contact depth")),
            ring_normal: FrustumRingNormal::SegmentAxis,
            sphere_geometry: None,
            seat_trim: None,
        }
    }

    const THREAD_SWEEP_CHILD: &str = "PLINTH_SUPPORT_THREAD_SWEEP_CHILD";
    const THREAD_SWEEP_SIGNATURE: &str = "PLINTH_SUPPORT_THREAD_SWEEP_SIGNATURE=";

    fn sweep_threads(test_name: &str) -> Option<Vec<String>> {
        if std::env::var(THREAD_SWEEP_CHILD).as_deref() == Ok("1") {
            return None;
        }
        Some(
            [1, 4, 16]
                .into_iter()
                .map(|threads| {
                    let output = std::process::Command::new(
                        std::env::current_exe().expect("current test executable"),
                    )
                    .args(["--exact", test_name, "--nocapture"])
                    .env("PLINTH_THREADS", threads.to_string())
                    .env(THREAD_SWEEP_CHILD, "1")
                    .output()
                    .expect("spawn isolated thread-sweep test");
                    assert!(
                        output.status.success(),
                        "thread sweep child {threads} failed:\n{}\n{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr),
                    );
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let sentinel = format!("\n{THREAD_SWEEP_SIGNATURE}");
                    assert_eq!(
                        stdout.matches(&sentinel).count(),
                        1,
                        "thread sweep child must emit exactly one signature sentinel:\n{stdout}"
                    );
                    stdout
                        .split_once(&sentinel)
                        .and_then(|(_, signature)| signature.lines().next())
                        .map(str::to_owned)
                        .expect("thread sweep child must report its deterministic signature")
                })
                .collect(),
        )
    }

    #[test]
    fn fixture_contacts_are_exterior_down_faces_in_source_order() {
        for mesh in [
            plinth_mesh::make_raking_blade().expect("raking blade"),
            plinth_mesh::make_overhang_table().expect("overhang table"),
            plinth_mesh::make_thin_fin().expect("thin fin"),
        ] {
            let pool = perceive_fixture(&mesh);
            assert!(
                !pool.candidates().is_empty(),
                "fixture must retain a real underside contact candidate"
            );
            let mut previous = None;
            for candidate in pool.candidates() {
                assert!(candidate.normal[2] < -45_f64.to_radians().sin());
                assert_eq!(candidate.id.0, u64::from(candidate.face_id));
                assert!(candidate.supported_area_mm2 > 0.0);
                assert!(candidate.overhang_excess > 0.0);
                if let Some(face) = previous {
                    assert!(face < candidate.face_id, "source face order is stable");
                }
                previous = Some(candidate.face_id);
            }
        }
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn contact_normal_is_the_specific_shared_face_normal_not_vertex_average() {
        let mesh = Mesh::new(
            vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 2.0],
            ],
            vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]],
        )
        .expect("closed tetrahedron");
        let pool = perceive_fixture(&mesh);
        let base = pool
            .candidates()
            .iter()
            .find(|candidate| candidate.face_id == 0)
            .expect("downward base is a candidate");
        assert_eq!(base.normal, [0.0, 0.0, -1.0]);
        assert_ne!(base.normal, [0.0, 0.0, -0.5]);
    }

    #[test]
    fn exact_overhang_threshold_is_excluded_and_steeper_underside_is_included() {
        let prism = |depth: f64| {
            Mesh::new(
                vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 0.0, -depth],
                    [0.0, 1.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [0.0, 1.0, -depth],
                ],
                vec![
                    [0, 2, 1],
                    [3, 4, 5],
                    [0, 1, 4],
                    [0, 4, 3],
                    [0, 3, 5],
                    [0, 5, 2],
                    [1, 2, 5],
                    [1, 5, 4],
                ],
            )
            .expect("closed triangular prism")
        };
        assert!(perceive_fixture(&prism(1.0)).candidates().is_empty());
        let steeper_mesh = prism(0.99);
        let steeper = perceive_fixture(&steeper_mesh);
        assert!(
            steeper
                .candidates()
                .iter()
                .any(|candidate| candidate.normal[2] < -45_f64.to_radians().sin()),
            "{steeper:?}"
        );
    }

    #[test]
    fn near_plate_down_faces_are_excluded() {
        let mesh = plinth_mesh::make_box_default().expect("closed box");
        let delicacy = vec![0.0; mesh.tris().len()];
        let config = SupportPerceptionConfig::current(
            Finite::new(-1.0).expect("finite fixture plate"),
            PositiveFinite::new(1.0).expect("positive fixture column pitch"),
            PositiveFinite::new(0.5).expect("positive fixture native-grid pitch"),
            TEST_GRID_BUDGET_BYTES,
            Some(&delicacy),
        )
        .expect("registered M0 constants are available");
        assert!(perceive_contacts(&mesh, &config)
            .expect("closed box")
            .candidates()
            .is_empty());
    }

    /// **A point-touch reaches perception.** Per the owner ruling of
    /// 2026-08-12 (FU-47, §4) a point-touch is legitimate sculpting, not a
    /// defect. A5 is the last stage that could still refuse such a model, and
    /// it did: `validate_perception_input` refused any nonzero cut flag,
    /// `TOPOLOGY_CUT_POINT_ONLY` included, so a mini with two fingertips
    /// meeting passed hollowing and then died here. Open topology is still
    /// refused — the test below this one proves that — so this is one cut
    /// class leaving the check, not the check leaving.
    #[test]
    fn a_point_touch_mesh_is_perceived_rather_than_refused_as_open_topology() {
        let mesh = plinth_mesh::make_pinched_boxes().expect("point-touch fixture");
        let delicacy = vec![0.0; mesh.tris().len()];
        perceive_contacts(&mesh, &config(&mesh, &delicacy))
            .expect("a point-touch is not open or invalid topology (FU-47)");
    }

    #[test]
    fn strict_open_and_missing_delicacy_inputs_fail_named() {
        let open = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("representable open triangle");
        let delicacy = vec![0.0];
        assert!(matches!(
            perceive_contacts(&open, &config(&open, &delicacy)),
            Err(SupportPerceptionError::OpenOrInvalidTopology { .. })
        ));

        let closed = plinth_mesh::make_box_default().expect("closed box");
        let missing = SupportPerceptionConfig::current(
            Finite::new(-5.0).expect("finite plate"),
            PositiveFinite::new(1.0).expect("positive column pitch"),
            PositiveFinite::new(0.5).expect("positive grid pitch"),
            TEST_GRID_BUDGET_BYTES,
            None,
        )
        .expect("registered constants");
        assert!(matches!(
            perceive_contacts(&closed, &missing),
            Err(SupportPerceptionError::MissingDelicacySidecar)
        ));

        let degenerate = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("representable degenerate mesh");
        let degenerate_delicacy = vec![0.0];
        assert!(matches!(
            perceive_contacts(&degenerate, &config(&degenerate, &degenerate_delicacy)),
            Err(SupportPerceptionError::StrictMesh(_))
        ));
    }

    #[test]
    fn gathered_native_perception_is_thread_count_invariant() {
        if let Some(signatures) =
            sweep_threads("tests::gathered_native_perception_is_thread_count_invariant")
        {
            assert!(signatures.windows(2).all(|pair| pair[0] == pair[1]));
            return;
        }
        let expected = std::env::var("PLINTH_THREADS")
            .expect("isolated sweep config")
            .parse::<usize>()
            .expect("positive isolated thread count");
        assert_eq!(geom_kernel::current_threads(), expected);
        let mesh = plinth_mesh::make_overhang_table().expect("overhang table");
        let delicacy = vec![0.75; mesh.tris().len()];
        let candidate_digest = perceive_contacts(&mesh, &config(&mesh, &delicacy))
            .expect("closed fixture")
            .digest();
        println!("\n{THREAD_SWEEP_SIGNATURE}{candidate_digest:?}");
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn consuming_scene_transition_moves_model_and_primitives_together() {
        let mesh = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        )
        .expect("the test tetrahedron is a valid mesh");
        let allocation = mesh.verts().as_ptr();
        let decimation = fixture_decimation_audit();
        let orientation = OrientationHistory::from_reports(
            decimation,
            OrientationReport {
                provenance: OrientationProvenance::Pinned {
                    pinned_report: PathBuf::from("baseline/model.pipeline.json"),
                },
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        );
        let hollowed =
            HollowedModel::solid_decision(mesh, orientation, SolidDecisionReason::BelowThreshold)
                .expect("public solid fallback must enter the support type path");
        let (mesh, hollow) = hollowed.into_parts();
        let primitives = vec![SupportPrimitive {
            segment_kind: SupportSegmentKind::JointSphere,
            start_mm: [
                Finite::new(0.0).expect("finite fixture coordinate"),
                Finite::new(0.0).expect("finite fixture coordinate"),
                Finite::new(0.5).expect("finite fixture coordinate"),
            ],
            end_mm: [
                Finite::new(0.0).expect("finite fixture coordinate"),
                Finite::new(0.0).expect("finite fixture coordinate"),
                Finite::new(-0.5).expect("finite fixture coordinate"),
            ],
            start_radius_mm: PositiveFinite::new(0.5).expect("positive fixture radius"),
            end_radius_mm: PositiveFinite::new(0.5).expect("positive fixture radius"),
            contact_depth_mm: None,
            ring_normal: FrustumRingNormal::SegmentAxis,
            sphere_geometry: Some(SphereGeometry::joint([
                Finite::new(0.0).unwrap(),
                Finite::new(0.0).unwrap(),
                Finite::new(0.0).unwrap(),
            ])),
            seat_trim: None,
        }];
        let lift_mm = PositiveFinite::new(5.0).expect("positive fixture lift");
        let mismatch = SupportedModel::from_parts(
            mesh.clone(),
            Vec::new(),
            SupportHistory::from_reports(
                hollow.clone(),
                SupportReport {
                    support_primitive_count: 1,
                    raft_primitive_count: 0,
                },
                BuildFrame::new(Finite::new(-5.0).unwrap(), lift_mm),
            ),
        );
        assert!(matches!(
            mismatch,
            Err(SupportError::InconsistentPrimitiveCount)
        ));
        let mut malformed_poles = primitives.clone();
        malformed_poles[0].end_mm[0] = Finite::new(0.25).unwrap();
        let malformed = SupportedModel::from_parts(
            mesh.clone(),
            malformed_poles,
            SupportHistory::from_reports(
                hollow.clone(),
                SupportReport {
                    support_primitive_count: 1,
                    raft_primitive_count: 0,
                },
                BuildFrame::new(Finite::new(-5.0).unwrap(), lift_mm),
            ),
        );
        assert!(matches!(malformed, Err(SupportError::InvalidPrimitive)));
        let mut malformed_normal = primitives.clone();
        malformed_normal[0].ring_normal = FrustumRingNormal::PlateZ;
        let malformed = SupportedModel::from_parts(
            mesh.clone(),
            malformed_normal,
            SupportHistory::from_reports(
                hollow.clone(),
                SupportReport {
                    support_primitive_count: 1,
                    raft_primitive_count: 0,
                },
                BuildFrame::new(Finite::new(-5.0).unwrap(), lift_mm),
            ),
        );
        assert!(matches!(malformed, Err(SupportError::InvalidPrimitive)));
        assert!(matches!(
            SphereGeometry::tip(
                [Finite::new(0.0).unwrap(); 3],
                [Finite::new(0.0).unwrap(); 3],
            ),
            Err(SupportError::InvalidPrimitive)
        ));
        assert!(matches!(
            SphereGeometry::tip_from_f64([0.0; 3], [f64::NAN, 0.0, 1.0]),
            Err(SupportError::InvalidPrimitive)
        ));
        assert!(matches!(
            SphereGeometry::tip_from_f64([0.0; 3], [f64::INFINITY, 0.0, 1.0]),
            Err(SupportError::InvalidPrimitive)
        ));
        let model = SupportedModel::from_parts(
            mesh,
            primitives,
            SupportHistory::from_reports(
                hollow,
                SupportReport {
                    support_primitive_count: 1,
                    raft_primitive_count: 0,
                },
                BuildFrame::new(Finite::new(-5.0).unwrap(), lift_mm),
            ),
        )
        .expect("scene report and primitive vector agree");
        let (mesh, primitives, history) = model.into_parts();
        assert_eq!(mesh.verts().as_ptr(), allocation);
        assert_eq!(primitives.len(), 1);
        assert_eq!(history.support().support_primitive_count, 1);
        assert_eq!(history.lift_mm(), lift_mm);
        assert_eq!(history.plate_z_mm().get(), -5.0);
        assert_eq!(
            history.hollow().hollow().disposition(),
            HollowDisposition::Solid {
                reason: SolidDecisionReason::BelowThreshold
            }
        );
    }

    #[test]
    fn canonical_pole_overflow_is_rejected_at_supported_model_boundary() {
        let radius = PositiveFinite::new(f64::MAX).expect("finite positive radius");
        let tip = SphereGeometry::tip(
            [Finite::new(f64::MAX).expect("finite center"); 3],
            [
                Finite::new(1.0).expect("finite axis"),
                Finite::new(0.0).expect("finite axis"),
                Finite::new(0.0).expect("finite axis"),
            ],
        )
        .expect("finite semantic authority");
        let joint = SphereGeometry::joint([Finite::new(f64::MAX).expect("finite center"); 3]);
        for (kind, sphere_geometry) in [
            (SupportSegmentKind::ContactSphere, tip),
            (SupportSegmentKind::JointSphere, joint),
        ] {
            let primitive = SupportPrimitive {
                segment_kind: kind,
                start_mm: [Finite::new(0.0).expect("finite fixture coordinate"); 3],
                end_mm: [Finite::new(0.0).expect("finite fixture coordinate"); 3],
                start_radius_mm: radius,
                end_radius_mm: radius,
                contact_depth_mm: None,
                ring_normal: FrustumRingNormal::SegmentAxis,
                sphere_geometry: Some(sphere_geometry),
                seat_trim: None,
            };
            let history = fixture_history(std::slice::from_ref(&primitive));
            assert!(matches!(
                SupportedModel::from_parts(small_closed_tetra(), vec![primitive], history),
                Err(SupportError::InvalidPrimitive)
            ));
        }
    }

    #[test]
    fn semantic_boundary_rejects_zero_length_frusta_but_accepts_coincident_raft_pad() {
        for kind in [
            SupportSegmentKind::ContactTaper,
            SupportSegmentKind::Member,
            SupportSegmentKind::RootFlare,
        ] {
            let primitive = non_sphere_primitive(kind, [0.0; 3]);
            let history = fixture_history(std::slice::from_ref(&primitive));
            assert!(matches!(
                SupportedModel::from_parts(small_closed_tetra(), vec![primitive], history),
                Err(SupportError::InvalidPrimitive)
            ));
        }

        let raft_pad = non_sphere_primitive(SupportSegmentKind::RaftPad, [0.0; 3]);
        let history = fixture_history(std::slice::from_ref(&raft_pad));
        let model = SupportedModel::from_parts(small_closed_tetra(), vec![raft_pad], history)
            .expect("coincident raft primitive is a valid pad");
        assert_eq!(
            model.primitives()[0].segment_kind,
            SupportSegmentKind::RaftPad
        );
    }

    /// The pad/bridge shape is decided by `segment_kind` alone, not inferred
    /// from whether the endpoints happen to coincide -- so the validator
    /// must enforce, not merely permit, that agreement: a `RaftPad` whose
    /// endpoints differ, or a `RaftBridge` whose endpoints coincide, is
    /// exactly as invalid as a zero-length frustum.
    #[test]
    fn raft_pad_and_bridge_shapes_are_enforced_not_merely_permitted() {
        let non_point_pad =
            non_sphere_primitive_span(SupportSegmentKind::RaftPad, [0.0; 3], [3.0, 0.0, 0.0]);
        let history = fixture_history(std::slice::from_ref(&non_point_pad));
        assert!(
            matches!(
                SupportedModel::from_parts(small_closed_tetra(), vec![non_point_pad], history),
                Err(SupportError::InvalidPrimitive)
            ),
            "a RaftPad with distinct endpoints is not a point and must be rejected"
        );

        let coincident_bridge = non_sphere_primitive(SupportSegmentKind::RaftBridge, [0.0; 3]);
        let history = fixture_history(std::slice::from_ref(&coincident_bridge));
        assert!(
            matches!(
                SupportedModel::from_parts(small_closed_tetra(), vec![coincident_bridge], history),
                Err(SupportError::InvalidPrimitive)
            ),
            "a RaftBridge with coincident endpoints connects nothing and must be rejected"
        );

        let bridge =
            non_sphere_primitive_span(SupportSegmentKind::RaftBridge, [0.0; 3], [3.0, 0.0, 0.0]);
        let history = fixture_history(std::slice::from_ref(&bridge));
        assert!(
            SupportedModel::from_parts(small_closed_tetra(), vec![bridge], history).is_ok(),
            "a RaftBridge with distinct endpoints is exactly the valid shape"
        );
    }

    fn selection_config<'a>(delicacy: &'a [f64]) -> ContactSelectionConfig<'a> {
        ContactSelectionConfig::current(Some(delicacy), 200_000, 64, 64 * 1024 * 1024)
            .expect("registered selection constants")
    }

    fn selection_candidate(id: u64, point_mm: [f64; 3]) -> ContactCandidate {
        ContactCandidate {
            id: ContactCandidateId(id),
            face_id: 0,
            point_mm,
            normal: [0.0, 0.0, -1.0],
            supported_area_mm2: 1.0,
            overhang_excess: 1.0,
            delicacy: 0.0,
        }
    }

    fn reference_island_selection(
        island_id: u32,
        eligible: &[&ContactCandidate],
        spacing: f64,
        minimum: usize,
    ) -> Vec<SelectedContact> {
        let mut selected = Vec::new();
        for candidate in eligible {
            if selected.iter().all(|contact: &SelectedContact| {
                squared_distance(contact.point_mm, candidate.point_mm).sqrt() >= spacing
            }) {
                selected.push(selected_contact(island_id, candidate));
            }
        }
        while selected.len() < minimum.min(eligible.len()) {
            let best = eligible
                .iter()
                .filter(|candidate| {
                    !selected
                        .iter()
                        .any(|contact| contact.candidate_id == candidate.id)
                })
                .max_by(|left, right| {
                    let nearest = |candidate: &&ContactCandidate| {
                        selected
                            .iter()
                            .map(|contact| squared_distance(contact.point_mm, candidate.point_mm))
                            .min_by(f64::total_cmp)
                            .unwrap_or(f64::INFINITY)
                    };
                    nearest(left)
                        .total_cmp(&nearest(right))
                        .then_with(|| right.id.cmp(&left.id))
                })
                .expect("reference fallback candidate");
            selected.push(selected_contact(island_id, best));
        }
        selected
    }

    #[test]
    fn spatial_poisson_selection_matches_frozen_pairwise_reference() {
        let cases = [
            vec![
                [0.0, 0.0, 0.0],
                [1.49, 0.0, 0.0],
                [1.5, 0.0, 0.0],
                [3.0, 0.0, 0.0],
            ],
            vec![
                [-1.49, -1.49, -1.49],
                [0.0, 0.0, 0.0],
                [1.49, 1.49, 1.49],
                [4.0, 4.0, 4.0],
            ],
            vec![
                [-3.0, 0.0, 0.0],
                [-1.5, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [1.5, 0.0, 0.0],
                [3.0, 0.0, 0.0],
            ],
        ];
        for points in cases {
            let candidates: Vec<_> = points
                .into_iter()
                .enumerate()
                .map(|(row, point)| selection_candidate(row as u64, point))
                .collect();
            let eligible: Vec<_> = candidates.iter().collect();
            let expected = reference_island_selection(9, &eligible, 1.5, 3);
            let (actual, _) = select_island_contacts(9, &eligible, 1.5, 3);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn spatial_poisson_preserves_fallback_distance_and_id_ties() {
        let candidates = [
            selection_candidate(10, [0.0, 0.0, 0.0]),
            selection_candidate(11, [0.0, 0.0, 0.0]),
            selection_candidate(12, [0.0, 0.0, 0.0]),
            selection_candidate(13, [0.0, 0.0, 0.0]),
        ];
        let eligible: Vec<_> = candidates.iter().collect();
        let expected = reference_island_selection(4, &eligible, 1.5, 3);
        let (actual, _) = select_island_contacts(4, &eligible, 1.5, 3);
        assert_eq!(actual, expected);
        assert_eq!(
            actual
                .iter()
                .map(|contact| contact.candidate_id)
                .collect::<Vec<_>>(),
            vec![
                ContactCandidateId(10),
                ContactCandidateId(11),
                ContactCandidateId(12)
            ]
        );
    }

    #[test]
    fn spatial_poisson_work_is_bounded_for_high_cardinality_fallback() {
        const COUNT: usize = 20_000;
        let candidates: Vec<_> = (0..COUNT)
            .map(|row| selection_candidate(row as u64, [0.0, 0.0, 0.0]))
            .collect();
        let eligible: Vec<_> = candidates.iter().collect();
        let (selected, work) = select_island_contacts(1, &eligible, 1.5, 3);
        assert_eq!(selected.len(), 3);
        assert!(
            work.distance_evaluations < COUNT * 3,
            "incremental selection performed {} distance evaluations",
            work.distance_evaluations
        );
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn overlapping_islands_use_one_candidate_face_index_not_pool_rescans() {
        const COUNT: usize = 512;
        const ISLANDS: usize = 512;
        let mesh = small_closed_tetra();
        let delicacy = vec![0.25; mesh.tris().len()];
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 0.4,
                max_grid_bytes: 64 * 1024 * 1024,
            },
        )
        .expect("fixture surface");
        let geometry = surface.geometry();
        let candidate_for_face = |id: u64, face: usize| ContactCandidate {
            id: ContactCandidateId(id),
            face_id: face as u32,
            point_mm: geometry.centroids()[face],
            normal: geometry.normals()[face],
            supported_area_mm2: geometry.areas()[face],
            overhang_excess: 1.0,
            delicacy: delicacy[face],
        };
        let mut candidate_rows = Vec::with_capacity(COUNT);
        candidate_rows.push(candidate_for_face(0, 0));
        candidate_rows.extend((1..COUNT).map(|id| candidate_for_face(id as u64, 1)));
        let pool = ContactCandidatePool {
            candidates: candidate_rows,
        };
        let islands: Vec<_> = (0..ISLANDS)
            .map(|id| SupportIsland {
                id: id as u32,
                face_ids: vec![0],
            })
            .collect();

        let (selected, work) =
            select_contacts_with_work(&mesh, &pool, &islands, &selection_config(&delicacy))
                .expect("overlapping logical owners select from the indexed face bucket");
        assert_eq!(selected.contacts().len(), ISLANDS);
        assert_eq!(
            work.candidate_membership_evaluations,
            COUNT + ISLANDS,
            "membership work is one index row per candidate plus one gathered row per island"
        );
        assert_eq!(work.distance_evaluations, 0);
        assert_eq!(work.coverage_index_rows, ISLANDS);
        assert_eq!(
            work.coverage_distance_evaluations,
            selected.coverage_samples().len(),
            "each sample queries only its one island/component-local contact"
        );
    }

    #[test]
    fn coverage_contact_index_preserves_closed_radius_and_route_ownership() {
        let mesh = small_closed_tetra();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).expect("topology");
        let component_id = topology.component_ids()[0];
        let contact = SelectedContact {
            island_id: 7,
            candidate_id: ContactCandidateId(1),
            served_face_id: 0,
            point_mm: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, -1.0],
            delicacy: 0.0,
            supported_area_mm2: 1.0,
        };
        let mut work = SelectionWork::default();
        let mut index = CoverageContactIndex::new(1.0);
        index.insert(&contact, &topology, &mut work);
        let sample = |island_id, component_id, x| CoverageSample {
            island_id,
            face_id: 0,
            component_id,
            point_mm: [x, 0.0, 0.0],
            area_mm2: 1.0,
        };

        assert!(index.covers(&sample(7, component_id, 1.0), &mut work));
        assert!(!index.covers(
            &sample(7, component_id, f64::from_bits(1.0_f64.to_bits() + 1)),
            &mut work
        ));
        assert!(!index.covers(&sample(8, component_id, 0.0), &mut work));
        assert!(!index.covers(&sample(7, component_id.wrapping_add(1), 0.0), &mut work));
    }

    fn pool_islands(pool: &ContactCandidatePool) -> Vec<SupportIsland> {
        vec![SupportIsland {
            id: 7,
            face_ids: pool
                .candidates()
                .iter()
                .map(|candidate| candidate.face_id)
                .collect(),
        }]
    }

    fn small_closed_tetra() -> Mesh {
        Mesh::new(
            vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 2.0],
            ],
            vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]],
        )
        .expect("closed tetrahedron")
    }

    #[test]
    fn shared_coverage_assessment_aggregates_connected_faces_and_ambiguity_cuts() {
        let mesh = small_closed_tetra();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let samples = vec![
            CoverageSample {
                island_id: 0,
                face_id: 0,
                component_id: topology.component_ids()[0],
                point_mm: [0.0, 0.0, 0.0],
                area_mm2: 0.1,
            },
            CoverageSample {
                island_id: 0,
                face_id: 1,
                component_id: topology.component_ids()[1],
                point_mm: [0.0, 0.0, 0.0],
                area_mm2: 0.1,
            },
        ];
        let connected = assess_coverage(
            &samples,
            &topology,
            &[2.0, 3.0],
            1.0,
            0.16,
            CoverageAssessmentLimits::UNBOUNDED,
        )
        .unwrap();
        assert_eq!(connected.failing_sample_indices, vec![0, 1]);
        assert_eq!(
            connected.objective,
            CoverageObjective {
                failing_uncovered_area_mm2: 0.2,
                max_patch_area_mm2: 0.2,
                failing_patch_count: 1,
                worst_gap_mm: 3.0,
            }
        );
        let below_floor = assess_coverage(
            &samples,
            &topology,
            &[2.0, 0.0],
            1.0,
            0.16,
            CoverageAssessmentLimits::UNBOUNDED,
        )
        .unwrap();
        assert!(below_floor.failing_sample_indices.is_empty());

        let open = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let open_topology = build_face_topology(&open, WeldPolicy::ExactBits).unwrap();
        let ambiguous = assess_coverage(
            &[CoverageSample {
                island_id: 0,
                face_id: 0,
                component_id: 0,
                point_mm: [0.0, 0.0, 0.0],
                area_mm2: 0.01,
            }],
            &open_topology,
            &[2.0],
            1.0,
            0.16,
            CoverageAssessmentLimits::UNBOUNDED,
        )
        .unwrap();
        assert_eq!(ambiguous.failing_sample_indices, vec![0]);
        assert_eq!(ambiguous.objective.failing_patch_count, 1);
    }

    #[test]
    fn coverage_assessment_scratch_and_operation_caps_are_preflighted_exactly() {
        let mesh = small_closed_tetra();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let samples = vec![CoverageSample {
            island_id: 0,
            face_id: 0,
            component_id: topology.component_ids()[0],
            point_mm: [0.0, 0.0, 0.0],
            area_mm2: 0.1,
        }];
        let plan = coverage_assessment_resource_plan(&samples, &topology).unwrap();
        assert!(assess_coverage(
            &samples,
            &topology,
            &[2.0],
            1.0,
            0.0,
            CoverageAssessmentLimits {
                max_scratch_bytes: plan.scratch_bytes,
                max_operations: plan.operations,
            },
        )
        .is_ok());
        assert!(matches!(
            assess_coverage(
                &samples,
                &topology,
                &[2.0],
                1.0,
                0.0,
                CoverageAssessmentLimits {
                    max_scratch_bytes: plan.scratch_bytes - 1,
                    max_operations: plan.operations,
                },
            ),
            Err(ContactSelectionError::CoverageAssessmentScratchLimit { .. })
        ));
        assert!(matches!(
            assess_coverage(
                &samples,
                &topology,
                &[2.0],
                1.0,
                0.0,
                CoverageAssessmentLimits {
                    max_scratch_bytes: plan.scratch_bytes,
                    max_operations: plan.operations - 1,
                },
            ),
            Err(ContactSelectionError::CoverageAssessmentOperationLimit { .. })
        ));
    }

    #[test]
    fn repair_assessment_keeps_its_original_plan_and_omits_measurement_evidence() {
        let mesh = small_closed_tetra();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let samples = vec![CoverageSample {
            island_id: 0,
            face_id: 0,
            component_id: topology.component_ids()[0],
            point_mm: [0.0, 0.0, 0.0],
            area_mm2: 0.1,
        }];
        let plan = coverage_assessment_resource_plan(&samples, &topology).unwrap();
        assert_eq!(plan.evidence_bytes, 0);
        assert_eq!(plan.phase_peak_bytes, plan.scratch_bytes);
        let assessment = assess_coverage(
            &samples,
            &topology,
            &[2.0],
            1.0,
            0.0,
            CoverageAssessmentLimits::UNBOUNDED,
        )
        .unwrap();
        assert_eq!(assessment.failing_sample_indices, vec![0]);
        assert!(assessment.patches.is_empty());
    }

    #[test]
    fn measurement_assessment_plan_checks_island_evidence_arithmetic() {
        let mesh = small_closed_tetra();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        assert!(matches!(
            coverage_measurement_assessment_resource_plan(&[], &topology, usize::MAX),
            Err(ContactSelectionError::CoverageResourceOverflow)
        ));
    }

    #[test]
    fn measurement_assessment_preflights_evidence_and_sorts_patch_faces() {
        let mesh = small_closed_tetra();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let samples = vec![
            CoverageSample {
                island_id: 0,
                face_id: 1,
                component_id: topology.component_ids()[1],
                point_mm: [0.0, 0.0, 0.0],
                area_mm2: 0.1,
            },
            CoverageSample {
                island_id: 0,
                face_id: 0,
                component_id: topology.component_ids()[0],
                point_mm: [0.0, 0.0, 0.0],
                area_mm2: 0.1,
            },
        ];
        let plan = coverage_assessment_resource_plan_for(
            &samples,
            &topology,
            CoverageAssessmentMode::MeasurementEvidence,
        )
        .unwrap();
        assert!(matches!(
            assess_coverage_with_mode(
                &samples,
                &topology,
                &[2.0, 2.0],
                1.0,
                0.0,
                CoverageAssessmentLimits {
                    max_scratch_bytes: plan.phase_peak_bytes - 1,
                    max_operations: plan.operations,
                },
                CoverageAssessmentMode::MeasurementEvidence,
            ),
            Err(ContactSelectionError::CoverageAssessmentScratchLimit { .. })
        ));
        let assessment = assess_coverage_with_mode(
            &samples,
            &topology,
            &[2.0, 2.0],
            1.0,
            0.0,
            CoverageAssessmentLimits {
                max_scratch_bytes: plan.phase_peak_bytes,
                max_operations: plan.operations,
            },
            CoverageAssessmentMode::MeasurementEvidence,
        )
        .unwrap();
        assert_eq!(assessment.patches.len(), 1);
        assert_eq!(assessment.patches[0].sorted_face_ids, vec![0, 1]);
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn measurement_evidence_preserves_boundary_and_repair_contracts() {
        let mesh = small_closed_tetra();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let component = topology.component_ids()[0];
        let samples = [CoverageSample {
            island_id: 9,
            face_id: 0,
            component_id: component,
            point_mm: [0.0, 0.0, 0.0],
            area_mm2: 0.1,
        }];

        let at_radius = assess_coverage_with_mode(
            &samples,
            &topology,
            &[1.0],
            1.0,
            0.1,
            CoverageAssessmentLimits::UNBOUNDED,
            CoverageAssessmentMode::MeasurementEvidence,
        )
        .unwrap();
        assert!(at_radius.patches.is_empty(), "radius equality is covered");

        let next = f64::from_bits(1.0_f64.to_bits() + 1);
        let measurement = assess_coverage_with_mode(
            &samples,
            &topology,
            &[next],
            1.0,
            0.1,
            CoverageAssessmentLimits::UNBOUNDED,
            CoverageAssessmentMode::MeasurementEvidence,
        )
        .unwrap();
        assert_eq!(measurement.patches.len(), 1);
        let patch = &measurement.patches[0];
        assert_eq!(
            (
                patch.island_id,
                patch.component_id,
                patch.representative_face_id
            ),
            (9, component, 0)
        );
        assert_eq!(patch.sorted_face_ids, vec![0]);
        assert_eq!(patch.uncovered_area_mm2, 0.1);
        assert_eq!(patch.worst_gap_mm, next);
        assert!(!patch.cut_ambiguous);
        assert!(!patch.assessment_failed, "area equality does not fail");

        let above_floor = assess_coverage_with_mode(
            &samples,
            &topology,
            &[next],
            1.0,
            f64::from_bits(0.1_f64.to_bits() - 1),
            CoverageAssessmentLimits::UNBOUNDED,
            CoverageAssessmentMode::MeasurementEvidence,
        )
        .unwrap();
        assert!(
            above_floor.patches[0].assessment_failed,
            "strictly above floor fails"
        );

        let no_contact = assess_coverage_with_mode(
            &samples,
            &topology,
            &[f64::INFINITY],
            1.0,
            0.1,
            CoverageAssessmentLimits::UNBOUNDED,
            CoverageAssessmentMode::MeasurementEvidence,
        )
        .unwrap();
        assert!(no_contact.patches[0].worst_gap_mm.is_infinite());

        let repair = assess_coverage(
            &samples,
            &topology,
            &[next],
            1.0,
            f64::from_bits(0.1_f64.to_bits() - 1),
            CoverageAssessmentLimits::UNBOUNDED,
        )
        .unwrap();
        assert_eq!(repair.objective, above_floor.objective);
        assert_eq!(
            repair.failing_sample_indices,
            above_floor.failing_sample_indices
        );
        assert!(repair.patches.is_empty(), "repair mode retains no evidence");

        let open = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let open_topology = build_face_topology(&open, WeldPolicy::ExactBits).unwrap();
        let ambiguous = assess_coverage_with_mode(
            &[CoverageSample {
                island_id: 3,
                face_id: 0,
                component_id: 0,
                point_mm: [0.0, 0.0, 0.0],
                area_mm2: 0.01,
            }],
            &open_topology,
            &[next],
            1.0,
            1.0,
            CoverageAssessmentLimits::UNBOUNDED,
            CoverageAssessmentMode::MeasurementEvidence,
        )
        .unwrap();
        assert!(ambiguous.patches[0].cut_ambiguous);
        assert!(ambiguous.patches[0].assessment_failed);
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn s1_consumes_s0_fixture_and_retains_face_specific_evidence() {
        let mesh = small_closed_tetra();
        let pool = perceive_fixture(&mesh);
        let delicacy = vec![0.25; mesh.tris().len()];
        let selected = select_contacts(
            &mesh,
            &pool,
            &pool_islands(&pool),
            &selection_config(&delicacy),
        )
        .expect("S0 legal pool can be selected");
        assert!(!selected.contacts().is_empty());
        assert!(!selected.coverage_samples().is_empty());
        for contact in selected.contacts() {
            let candidate = pool
                .candidates()
                .iter()
                .find(|candidate| candidate.id == contact.candidate_id)
                .expect("selection retains an S0 candidate identity");
            assert_eq!(contact.served_face_id, candidate.face_id);
            assert_eq!(
                contact.normal, candidate.normal,
                "never substitute a vertex normal"
            );
            assert_eq!(contact.point_mm, candidate.point_mm);
            assert_eq!(contact.supported_area_mm2, candidate.supported_area_mm2);
        }
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).expect("topology");
        let mut work = SelectionWork::default();
        let mut contact_index = CoverageContactIndex::new(4.0 * 1.5);
        for contact in selected.contacts() {
            contact_index.insert(contact, &topology, &mut work);
        }
        assert!(failing_coverage_samples(
            selected.coverage_samples(),
            &contact_index,
            &topology,
            0.4,
            CoverageAssessmentLimits::UNBOUNDED,
            &mut work,
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn separated_and_overlapping_islands_remain_logically_owned() {
        let mesh = small_closed_tetra();
        let delicacy = vec![0.25; mesh.tris().len()];
        let pool = ContactCandidatePool {
            candidates: vec![
                ContactCandidate {
                    id: ContactCandidateId(0),
                    face_id: 0,
                    point_mm: [0.0, -0.333_333, 0.0],
                    normal: [0.0, 0.0, -1.0],
                    supported_area_mm2: 2.0,
                    overhang_excess: 1.0,
                    delicacy: 0.25,
                },
                ContactCandidate {
                    id: ContactCandidateId(1),
                    face_id: 1,
                    point_mm: [0.0, -0.666_667, 0.666_667],
                    normal: [0.0, -0.894_427, 0.447_214],
                    supported_area_mm2: 2.236,
                    overhang_excess: 1.0,
                    delicacy: 0.25,
                },
            ],
        };
        let islands = vec![
            SupportIsland {
                id: 10,
                face_ids: vec![0],
            },
            SupportIsland {
                id: 20,
                face_ids: vec![1],
            },
            SupportIsland {
                id: 30,
                face_ids: vec![0],
            },
        ];
        let selected = select_contacts(&mesh, &pool, &islands, &selection_config(&delicacy))
            .expect("each named island has a legal contact");
        for island_id in [10, 20, 30] {
            assert!(selected
                .contacts()
                .iter()
                .any(|contact| contact.island_id == island_id));
        }
        let first = selected
            .contacts()
            .iter()
            .find(|contact| contact.island_id == 10)
            .unwrap();
        let overlap = selected
            .contacts()
            .iter()
            .find(|contact| contact.island_id == 30)
            .unwrap();
        assert_eq!(
            first.candidate_id, overlap.candidate_id,
            "overlap is a logical owner, not a fake new seat"
        );
    }

    #[test]
    fn delicacy_and_exact_id_ties_have_stable_selection_order() {
        let mesh = plinth_mesh::make_overhang_table().expect("table fixture");
        let original = perceive_fixture(&mesh);
        let mut candidates = original.candidates()[..2].to_vec();
        candidates[0].point_mm[2] = 0.0;
        candidates[1].point_mm[2] = 0.0;
        let mut delicacy = vec![0.25; mesh.tris().len()];
        delicacy[candidates[0].face_id as usize] = 0.8;
        delicacy[candidates[1].face_id as usize] = 0.1;
        candidates[0].delicacy = 0.8;
        candidates[1].delicacy = 0.1;
        let pool = ContactCandidatePool { candidates };
        let islands = vec![SupportIsland {
            id: 1,
            face_ids: pool
                .candidates()
                .iter()
                .map(|candidate| candidate.face_id)
                .collect(),
        }];
        let selected = select_contacts(&mesh, &pool, &islands, &selection_config(&delicacy))
            .expect("tie fixture selects");
        assert_eq!(selected.contacts()[0].candidate_id, pool.candidates()[1].id);

        let mut equal = pool.clone();
        for candidate in &mut equal.candidates {
            candidate.delicacy = 0.2;
            delicacy[candidate.face_id as usize] = 0.2;
        }
        let selected = select_contacts(&mesh, &equal, &islands, &selection_config(&delicacy))
            .expect("exact-id tie fixture selects");
        let expected = equal
            .candidates()
            .iter()
            .map(|candidate| candidate.id)
            .min()
            .unwrap();
        assert_eq!(selected.contacts()[0].candidate_id, expected);
    }

    /// SEALED-INPUT RULE oracle ([`crate::digest_seal`]).  The seal taken in
    /// `SelectedContacts::new` must equal the full fingerprint walk, and must
    /// survive a `Clone` — a clone carries both vectors the walk folds, so it
    /// carries the seal truthfully.
    #[test]
    fn selected_contacts_seal_equals_the_full_walk() {
        let mesh = small_closed_tetra();
        let delicacy = vec![0.25; mesh.tris().len()];
        let pool = perceive_fixture(&mesh);
        let islands = pool_islands(&pool);
        let selected = select_contacts(&mesh, &pool, &islands, &selection_config(&delicacy))
            .expect("seal-oracle fixture selects");
        assert_eq!(selected.digest(), selected.digest_full_walk());
        let cloned = selected.clone();
        assert_eq!(cloned.digest(), cloned.digest_full_walk());
        assert_eq!(cloned.digest(), selected.digest());

        let empty = SelectedContacts::new(Vec::new(), Vec::new());
        assert_eq!(empty.digest(), empty.digest_full_walk());
    }

    #[test]
    fn malformed_candidate_pool_and_missing_sidecars_fail_named() {
        let mesh = plinth_mesh::make_box_default().expect("box fixture");
        let delicacy = vec![0.0; mesh.tris().len()];
        let pool = ContactCandidatePool {
            candidates: vec![ContactCandidate {
                id: ContactCandidateId(0),
                face_id: 0,
                point_mm: [10_000.0, 10_000.0, 10_000.0],
                normal: [0.0, 0.0, -1.0],
                supported_area_mm2: 1.0,
                overhang_excess: 1.0,
                delicacy: 0.0,
            }],
        };
        let islands = vec![SupportIsland {
            id: 1,
            face_ids: vec![0],
        }];
        assert!(matches!(
            select_contacts(&mesh, &pool, &islands, &selection_config(&delicacy)),
            Err(ContactSelectionError::CandidatePointOffSourceFace { .. })
        ));
        let missing = ContactSelectionConfig::current(None, 1, 1, 1).expect("constants");
        assert!(matches!(
            select_contacts(&mesh, &pool, &islands, &missing),
            Err(ContactSelectionError::MissingDelicacySidecar)
        ));
    }

    #[test]
    fn candidate_pool_boundary_rejects_each_named_malformed_input() {
        let mesh = small_closed_tetra();
        let delicacy = vec![0.25; mesh.tris().len()];
        let good = perceive_fixture(&mesh);
        let islands = pool_islands(&good);
        let candidate = good.candidates()[0].clone();
        let check = |pool: ContactCandidatePool| {
            select_contacts(&mesh, &pool, &islands, &selection_config(&delicacy))
        };

        let mut pool = ContactCandidatePool {
            candidates: vec![candidate.clone()],
        };
        pool.candidates[0].face_id = u32::MAX;
        assert!(matches!(
            check(pool),
            Err(ContactSelectionError::CandidateFaceOutOfRange { .. })
        ));
        let mut pool = ContactCandidatePool {
            candidates: vec![candidate.clone()],
        };
        pool.candidates[0].point_mm[0] = f64::NAN;
        assert!(matches!(
            check(pool),
            Err(ContactSelectionError::CandidateNonFinite {
                field: "point_mm",
                ..
            })
        ));
        let mut pool = ContactCandidatePool {
            candidates: vec![candidate.clone()],
        };
        pool.candidates[0].normal[0] = f64::NAN;
        assert!(matches!(
            check(pool),
            Err(ContactSelectionError::CandidateNonFinite {
                field: "normal",
                ..
            })
        ));
        let mut pool = ContactCandidatePool {
            candidates: vec![candidate.clone()],
        };
        pool.candidates[0].supported_area_mm2 = -1.0;
        assert!(matches!(
            check(pool),
            Err(ContactSelectionError::CandidateSupportedArea { .. })
        ));
        let pool = ContactCandidatePool {
            candidates: vec![candidate.clone(), candidate.clone()],
        };
        assert!(matches!(
            check(pool),
            Err(ContactSelectionError::DuplicateCandidateId { .. })
        ));
        let mut pool = ContactCandidatePool {
            candidates: vec![candidate.clone()],
        };
        pool.candidates[0].point_mm[0] += 1.0;
        assert!(matches!(
            check(pool),
            Err(ContactSelectionError::CandidatePointOffSourceFace { .. })
        ));
        let mut pool = ContactCandidatePool {
            candidates: vec![candidate.clone()],
        };
        pool.candidates[0].normal = [1.0, 0.0, 0.0];
        assert!(matches!(
            check(pool),
            Err(ContactSelectionError::CandidateNormalMismatch { .. })
        ));
        let mut pool = ContactCandidatePool {
            candidates: vec![candidate],
        };
        pool.candidates[0].delicacy = 0.5;
        assert!(matches!(
            check(pool),
            Err(ContactSelectionError::CandidateDelicacyMismatch { .. })
        ));
    }

    #[test]
    fn coverage_repair_selects_other_source_face_in_the_same_component_at_every_tile_boundary() {
        // This is an independent A4 score vector: the failing patch belongs to
        // face 0, while row 1 is the only unoccupied same-component seat that
        // covers it.  The frozen Python ranker deliberately does not require
        // candidate.face_id == sample.face_id.
        let candidates = vec![
            ContactCandidate {
                id: ContactCandidateId(10),
                face_id: 0,
                point_mm: [0.0; 3],
                normal: [0.0, 0.0, -1.0],
                supported_area_mm2: 1.0,
                overhang_excess: 1.0,
                delicacy: 0.0,
            },
            ContactCandidate {
                id: ContactCandidateId(20),
                face_id: 1,
                point_mm: [4.0, 0.0, 0.0],
                normal: [0.0, 0.0, -1.0],
                supported_area_mm2: 1.0,
                overhang_excess: 1.0,
                delicacy: 0.0,
            },
        ];
        let samples = vec![
            CoverageSample {
                island_id: 9,
                face_id: 0,
                component_id: 3,
                point_mm: [8.0, 0.0, 0.0],
                area_mm2: 2.0,
            },
            CoverageSample {
                island_id: 9,
                face_id: 0,
                component_id: 3,
                point_mm: [8.5, 0.0, 0.0],
                area_mm2: 3.0,
            },
            CoverageSample {
                island_id: 9,
                face_id: 1,
                component_id: 3,
                point_mm: [4.0, 0.0, 0.0],
                area_mm2: 1.0,
            },
        ];
        let occupied = vec![selected_contact(9, &candidates[0])];
        for budget in [1, 2, 3, 4, 16] {
            let (island, repair) = rank_coverage_repair_rows(
                &candidates,
                &samples,
                &[0, 1],
                &occupied,
                &[3, 3],
                5.0,
                budget,
            )
            .expect("different-face candidate must repair the failing patch");
            assert_eq!(island, 9);
            assert_eq!(repair.id, ContactCandidateId(20));
        }
    }

    #[test]
    fn repair_ranking_keeps_zero_score_rows_and_breaks_ties_by_source_order() {
        let candidates = vec![
            ContactCandidate {
                id: ContactCandidateId(10),
                face_id: 0,
                point_mm: [100.0, 0.0, 0.0],
                normal: [0.0, 0.0, -1.0],
                supported_area_mm2: 1.0,
                overhang_excess: 1.0,
                delicacy: 0.0,
            },
            ContactCandidate {
                id: ContactCandidateId(20),
                face_id: 0,
                point_mm: [101.0, 0.0, 0.0],
                normal: [0.0, 0.0, -1.0],
                supported_area_mm2: 1.0,
                overhang_excess: 1.0,
                delicacy: 0.0,
            },
        ];
        let samples = vec![CoverageSample {
            island_id: 9,
            face_id: 0,
            component_id: 3,
            point_mm: [0.0, 0.0, 0.0],
            area_mm2: 1.0,
        }];
        let (island, repair) =
            rank_coverage_repair_rows(&candidates, &samples, &[0], &[], &[3], 1.0, 65_536)
                .expect("zero-score rows remain eligible");
        assert_eq!(island, 9);
        assert_eq!(repair.id, ContactCandidateId(10));
    }

    #[test]
    fn dyadic_preflight_matches_independent_frozen_byte_vector() {
        let mesh = small_closed_tetra();
        let delicacy = vec![0.25; mesh.tris().len()];
        let config = selection_config(&delicacy);
        let islands = vec![SupportIsland {
            id: 7,
            face_ids: vec![0],
        }];
        let plan = plan_dyadic_coverage(&mesh, &islands, &config).expect("preflight");
        assert_eq!(plan.total_cells, 64);
        assert_eq!(plan.source_faces, 1);
        assert_eq!(plan.working_cells, 64);
        assert_eq!(plan.compact_bytes, 3_498);
        assert_eq!(plan.build_peak_bytes, 15_433);
        assert_eq!(plan.validation_peak_bytes, 11_658);
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn dyadic_preflight_refuses_each_cap_at_the_exact_next_byte_or_cell() {
        let mesh = small_closed_tetra();
        let delicacy = vec![0.25; mesh.tris().len()];
        let islands = vec![SupportIsland {
            id: 7,
            face_ids: vec![0],
        }];
        let base = selection_config(&delicacy);

        let mut config = base;
        config.coverage_samples_max_virtual_cells = 63;
        assert_eq!(
            plan_dyadic_coverage(&mesh, &islands, &config),
            Err(ContactSelectionError::CoverageVirtualCellLimit {
                required: 64,
                limit: 63
            })
        );
        let mut config = base;
        config.coverage_samples_max_working_cells = 63;
        let windowed = plan_dyadic_coverage(&mesh, &islands, &config).unwrap();
        assert_eq!(windowed.total_cells, 64);
        assert_eq!(windowed.working_cells, 63);
        let mut config = base;
        config.coverage_samples_max_compact_payload_bytes = 3_497;
        assert_eq!(
            plan_dyadic_coverage(&mesh, &islands, &config),
            Err(ContactSelectionError::CoverageCompactPayloadLimit {
                required: 3_498,
                limit: 3_497
            })
        );
        let mut config = base;
        config.coverage_samples_max_peak_bytes = 15_432;
        assert_eq!(
            plan_dyadic_coverage(&mesh, &islands, &config),
            Err(ContactSelectionError::CoveragePeakLimit {
                phase: "build",
                required: 15_433,
                limit: 15_432
            })
        );

        // Add unrelated closed components so validation's 72F term exceeds
        // build peak while the one-face coverage payload remains unchanged.
        let mut verts = Vec::new();
        let mut tris = Vec::new();
        for shell in 0..100_u32 {
            let x = f64::from(shell) * 10.0;
            let base_vertex = verts.len() as u32;
            verts.extend([
                [x - 1.0, -1.0, 0.0],
                [x + 1.0, -1.0, 0.0],
                [x, 1.0, 0.0],
                [x, 0.0, 2.0],
            ]);
            tris.extend([
                [base_vertex, base_vertex + 2, base_vertex + 1],
                [base_vertex, base_vertex + 1, base_vertex + 3],
                [base_vertex + 1, base_vertex + 2, base_vertex + 3],
                [base_vertex + 2, base_vertex, base_vertex + 3],
            ]);
        }
        let many = Mesh::new(verts, tris).expect("closed component batch");
        // Independent frozen equations for N=64,S=1,I=1,W=64,F=400,V=400.
        let build = 19_939;
        let validation = 40_170;
        assert_eq!(coverage_build_peak_bytes(64, 1, 64, 1, 400, 400), Ok(build));
        assert_eq!(
            coverage_validation_peak_bytes(64, 1, 1, 400, 400),
            Ok(validation)
        );
        let mut config = base;
        config.coverage_samples_max_peak_bytes = validation - 1;
        assert_eq!(
            plan_dyadic_coverage(&many, &islands, &config),
            Err(ContactSelectionError::CoveragePeakLimit {
                phase: "validation",
                required: validation,
                limit: validation - 1
            })
        );
    }

    #[test]
    fn validation_peak_vectors_are_independent_and_pin_multiple_axes() {
        // N=16,S=2,I=3,W=7,F=10,V=12. These literals are evaluated from the
        // normative equations, not from another implementation helper.
        assert_eq!(coverage_compact_bytes(16, 2, 3), Ok(1_186));
        assert_eq!(coverage_build_peak_bytes(16, 2, 7, 3, 10, 12), Ok(3_380));
        assert_eq!(coverage_validation_peak_bytes(16, 2, 3, 10, 12), Ok(3_874));
    }

    #[test]
    fn planning_streams_malicious_overlapping_islands_without_plan_rows() {
        let mesh = small_closed_tetra();
        let delicacy = vec![0.25; mesh.tris().len()];
        let mut config = selection_config(&delicacy);
        config.coverage_samples_max_virtual_cells = 1_000;
        let islands: Vec<SupportIsland> = (0..50_000_u32)
            .map(|id| SupportIsland {
                id,
                face_ids: vec![0],
            })
            .collect();
        dyadic::clear_generic_dyadic_test_observations();
        assert_eq!(
            plan_dyadic_coverage(&mesh, &islands, &config),
            Err(ContactSelectionError::CoverageVirtualCellLimit {
                required: 50_000,
                limit: 1_000,
            })
        );
        assert_eq!(dyadic::preflight_face_hook_count(), 0);
        assert_eq!(
            std::mem::size_of::<CoverageBuildPlan>(),
            6 * std::mem::size_of::<usize>()
        );
    }

    #[test]
    fn one_face_larger_than_working_window_is_bit_identical_and_bounded() {
        let mesh = small_closed_tetra();
        let delicacy = vec![0.25; mesh.tris().len()];
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let islands = vec![SupportIsland {
            id: 7,
            face_ids: vec![0],
        }];
        let mut expected = None;
        for working in [1, 3, 7, 63, 64] {
            let mut config = selection_config(&delicacy);
            config.coverage_samples_max_working_cells = working;
            let (samples, observed) = build_dyadic_coverage_samples_with_high_water(
                &mesh,
                &islands,
                topology.component_ids(),
                &config,
            )
            .unwrap();
            assert_eq!(samples.len(), 64);
            assert!(observed <= working, "observed {observed} > W={working}");
            assert_eq!(observed, working.min(64));
            if let Some(ref prior) = expected {
                assert_eq!(&samples, prior);
            } else {
                expected = Some(samples);
            }
        }
    }

    #[test]
    fn generic_dyadic_builder_is_face_order_independent_and_preflight_has_no_side_effects() {
        let mesh = small_closed_tetra();
        let original = mesh.clone();
        let delicacy = vec![0.25; mesh.tris().len()];
        let config = selection_config(&delicacy);
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let forward = vec![
            SupportIsland {
                id: 10,
                face_ids: vec![0],
            },
            SupportIsland {
                id: 20,
                face_ids: vec![1],
            },
        ];
        let reverse = vec![forward[1].clone(), forward[0].clone()];
        let a = build_dyadic_coverage_samples(&mesh, &forward, topology.component_ids(), &config)
            .unwrap();
        let b = build_dyadic_coverage_samples(&mesh, &reverse, topology.component_ids(), &config)
            .unwrap();
        assert_eq!(a, b);
        let mut refused = config;
        refused.coverage_samples_max_virtual_cells = 1;
        assert!(matches!(
            plan_dyadic_coverage(&mesh, &forward, &refused),
            Err(ContactSelectionError::CoverageVirtualCellLimit { .. })
        ));
        assert_eq!(mesh, original);
        assert_eq!(forward[0].face_ids, vec![0]);
    }

    #[test]
    fn candidate_and_coverage_adapters_enter_the_same_generic_dyadic_skeleton() {
        let mesh = small_closed_tetra();
        let islands = [SupportIsland {
            id: 7,
            face_ids: vec![0],
        }];
        let candidate_mesh = mesh.clone();
        let candidate_islands = islands.clone();
        let candidate = std::thread::spawn(move || {
            dyadic::clear_generic_dyadic_test_observations();
            dyadic::build_candidate_payload(
                &candidate_mesh,
                &candidate_islands,
                &dyadic::CandidatePayloadConfig::current(
                    8,
                    1_000_000,
                    3_000_000,
                    192 * 1024 * 1024,
                    64,
                )
                .expect("candidate config"),
            )
            .expect("candidate adapter");
            dyadic::generic_dyadic_invocation_types()
        });
        let coverage = std::thread::spawn(move || {
            dyadic::clear_generic_dyadic_test_observations();
            let delicacy = vec![0.25; mesh.tris().len()];
            let config = selection_config(&delicacy);
            let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).expect("topology");
            build_dyadic_coverage_samples(&mesh, &islands, topology.component_ids(), &config)
                .expect("coverage adapter");
            dyadic::generic_dyadic_invocation_types()
        });
        let candidate = candidate.join().expect("candidate thread");
        let coverage = coverage.join().expect("coverage thread");
        assert_eq!(candidate, vec![dyadic::candidate_policy_type_name()]);
        assert_eq!(coverage, vec![coverage::coverage_policy_type_name()]);
    }

    #[test]
    fn native_surface_grid_cap_is_required_and_refuses_before_selection() {
        let mesh = small_closed_tetra();
        let delicacy = vec![0.25; mesh.tris().len()];
        let pool = perceive_fixture(&mesh);
        let islands = pool_islands(&pool);
        let mut config = selection_config(&delicacy);
        config.surface_max_grid_bytes = 1;
        assert!(matches!(
            select_contacts(&mesh, &pool, &islands, &config),
            Err(ContactSelectionError::CandidateSurfaceGridLimit { limit: 1 })
        ));
        let defaults =
            ContactSelectionConfig::current_defaults(Some(&delicacy), 64 * 1024 * 1024).unwrap();
        assert_eq!(defaults.coverage_samples_max_virtual_cells, 2_000_000);
        assert_eq!(
            defaults.coverage_samples_max_compact_payload_bytes,
            160 * 1024 * 1024
        );
        assert_eq!(defaults.coverage_samples_max_working_cells, 65_536);
        assert_eq!(defaults.coverage_samples_max_peak_bytes, 512 * 1024 * 1024);
        assert_eq!(defaults.coverage_score_matrix_max_entries, 65_536);
    }

    #[test]
    fn s1_selection_digest_is_thread_setting_invariant_with_batched_pool_validation() {
        if let Some(signatures) = sweep_threads(
            "tests::s1_selection_digest_is_thread_setting_invariant_with_batched_pool_validation",
        ) {
            assert!(signatures.windows(2).all(|pair| pair[0] == pair[1]));
            return;
        }
        let expected = std::env::var("PLINTH_THREADS")
            .expect("isolated sweep config")
            .parse::<usize>()
            .expect("positive isolated thread count");
        assert_eq!(geom_kernel::current_threads(), expected);
        let mesh = small_closed_tetra();
        let pool = perceive_fixture(&mesh);
        let delicacy = vec![0.25; mesh.tris().len()];
        let islands = pool_islands(&pool);
        let digest = select_contacts(&mesh, &pool, &islands, &selection_config(&delicacy))
            .expect("selection uses only already-gathered S0 evidence")
            .digest();
        println!("\n{THREAD_SWEEP_SIGNATURE}{digest:?}");
    }
}

#[cfg(test)]
mod near_parallel_perception_tests {
    use super::*;

    /// A closed box standing above the plate, with ONE side-wall vertex nudged
    /// in x by `dx` millimetres.
    ///
    /// For a straight-down column ray `d = (0,0,-1)` the Möller determinant of a
    /// face reduces to `(e1 x e2)_z` exactly, so an exactly vertical wall gives
    /// `det == 0` — `ExactParallel`, a genuine prune — and a wall tilted by `dx`
    /// gives `det = 2 * dx`. A nudge of `2.5e-15 mm` therefore puts `det` at
    /// `5e-15`: inside `DET_EPS = 1e-14` and nonzero, which is undecidable.
    fn nudged_box(dx: f64) -> Mesh {
        let vertices = vec![
            [-1.0, -1.0, 5.0],
            [1.0, -1.0, 5.0],
            [1.0, 1.0, 5.0],
            [-1.0, 1.0, 5.0],
            [-1.0, -1.0, 9.0],
            [1.0 + dx, -1.0, 9.0],
            [1.0 + dx, 1.0, 9.0],
            [-1.0, 1.0, 9.0],
        ];
        let faces = vec![
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
        Mesh::new(vertices, faces).expect("fixture mesh")
    }

    fn perception_config(delicacy: &[f64]) -> SupportPerceptionConfig<'_> {
        SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(0.25).unwrap(),
            PositiveFinite::new(0.5).unwrap(),
            8 * 1024 * 1024,
            Some(delicacy),
        )
        .expect("registered perception constants")
    }

    /// **`docs/specs/geometry-adjudication.md` §EXT-1 site 3, the revision-4
    /// RULING — a `NearParallel` on a column's ray changes NO `grounded` bit,
    /// and the affected faces STAY candidates.**
    ///
    /// `grounded[face]` is a MEASURED-DISTANCE claim ("a surface exists within
    /// the bridge gap below this face") and it is an EXEMPTION from the entire
    /// support machinery. An undecidable crossing carries no usable `t`, so
    /// affirming `grounded` from one would assert a distance nobody measured,
    /// and an exemption may grow only on proof. Candidacy routes a face INTO
    /// scrutiny — seat discovery, foot validation, clearance acceptance, each
    /// with its own refusal — while grounding routes it OUT, past perception's
    /// island discovery, where no downstream coverage refusal can ever see it.
    ///
    /// The input class: a column whose ray grazes a face at a determinant the
    /// arithmetic cannot resolve. The pair below differs by `2.5e-15 mm` of
    /// wall tilt and by nothing else.
    #[test]
    fn an_undecidable_column_ray_changes_no_grounded_bit_and_keeps_its_candidates() {
        let delicacy = vec![0.0_f64; 12];

        let perceive = |dx: f64| {
            let mesh = nudged_box(dx);
            let config = perception_config(&delicacy);
            let (topology, delicacy_by_face) =
                validate_perception_input(&mesh, &config).expect("strict fixture mesh");
            let surface = NativeSurface::from_indexed(
                mesh.verts(),
                mesh.tris(),
                NativeSurfaceConfig {
                    grid_pitch_mm: config.surface_grid_pitch_mm.get(),
                    max_grid_bytes: config.max_grid_bytes,
                },
            )
            .expect("fixture surface");
            let pool =
                perceive_contacts_on_surface(&mesh, &config, &topology, delicacy_by_face, &surface)
                    .expect("perception runs");
            let mut faces: Vec<u32> = pool.candidates.iter().map(|row| row.face_id).collect();
            faces.sort_unstable();
            (faces, surface)
        };

        let (clean_faces, clean_surface) = perceive(0.0);
        let (planted_faces, planted_surface) = perceive(2.5e-15);

        // NON-VACUITY at both ends, measured on the SAME rays perception casts:
        // the exactly-vertical fixture prunes cleanly, the nudged one does not.
        let column_probe = |surface: &NativeSurface| {
            let bounds = surface.geometry().bounds();
            let ztop = bounds[1][2] + 1.0;
            let t_max = ztop - bounds[0][2] + 1.0;
            let origins: Vec<[f64; 3]> = (0..9)
                .map(|k| {
                    let step = 0.25 * f64::from(k) - 1.0;
                    [step, 0.0, ztop]
                })
                .collect();
            let dirs = vec![[0.0, 0.0, -1.0]; origins.len()];
            surface
                .cast_rays_classified(&origins, &dirs, t_max)
                .expect("the column probe runs")
                .total_near_parallel()
        };
        assert_eq!(
            column_probe(&clean_surface),
            0,
            "the exactly-vertical walls must be a GENUINE prune (det == 0), or \
             the control half of this pair proves nothing"
        );
        assert!(
            column_probe(&planted_surface) > 0,
            "the nudged wall must put a column ray in the undecidable band, or \
             the planted half of this pair proves nothing"
        );

        // THE RULING. The candidate set does not move.
        assert!(
            !clean_faces.is_empty(),
            "the box's underside must offer candidates at all"
        );
        assert_eq!(
            planted_faces, clean_faces,
            "an undecidable column ray must not exempt any face from support: \
             the skipped crossing is treated as ABSENT, which is both what the \
             arithmetic already does and the fail-closed side"
        );
    }
}
