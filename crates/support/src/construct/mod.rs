//! Sealed fixed-size construction. Geometry only moves forward: contacts → routing →
//! frozen plan. Quotes are consumed atomically and tied to one builder revision.
//!
//! ```compile_fail
//! use plinth_support::construct::PatchBound;
//! let b = PatchBound { lo: 0.1, hi: 0.3 };
//! ```
//! ```compile_fail
//! use plinth_support::construct::{CappedPatch, PatchBound};
//! fn forge(b: PatchBound) { let _ = CappedPatch(b); }
//! ```
//! ```compile_fail
//! use plinth_support::construct::Section;
//! let s = Section { r: 0.04 };
//! ```
//! ```compile_fail
//! use plinth_support::construct::{PlanBuilder, Contacts, ContactId};
//! fn edit(p: &mut PlanBuilder<Contacts>, id: ContactId) { p.get_mut(id); }
//! ```
//! ```compile_fail
//! use plinth_support::construct::ContactId;
//! let id = ContactId { plan: 1, ix: 0 };
//! ```
//! ```compile_fail
//! use plinth_support::construct::Quote;
//! fn copy(q: Quote) { q.clone(); }
//! ```
//! ```compile_fail
//! use plinth_support::construct::{PlanBuilder, Routed};
//! fn late(p: &PlanBuilder<Routed>) { p.probe(&[], &mut Vec::new()); }
//! ```
//! ```compile_fail,E0308
//! use plinth_support::construct::{ContactId, MemberProposal, PlanBuilder, Routed};
//! fn bare(p: &PlanBuilder<Routed>, contact: ContactId) {
//!     p.probe_members(&[MemberProposal::Column { contact, section: 0.04 }], &mut Vec::new());
//! }
//! ```
//! ```compile_fail
//! use plinth_support::construct::SupportPlan;
//! fn edit(p: &mut SupportPlan) { p.parts.clear(); }
//! ```
//! ```compile_fail,E0451
//! use plinth_geom::Mesh;
//! use plinth_support::construct::{KeepUpRecord, PosedModel};
//! fn forge(mesh: Mesh, keep: KeepUpRecord) {
//!     let _ = PosedModel {
//!         mesh, triangles: Vec::new(), translate: [0.0, 0.0, 4.0], keep, plate: 0.0,
//!     };
//! }
//! ```

mod contact;
mod members;
mod native;
#[cfg(test)]
mod tests;
mod types;

pub use contact::require_capacity;
pub use members::{Anchor, MemberId, MemberProposal, MemberQuote, MemberQuoteBatch, RaftId};
pub use native::{NativePart, PartFields, PartKind};
pub use types::{
    CappedPatch, KeepUpRecord, Limits, PatchBound, PoseError, PosedModel, ProcessRecord, Section,
};

use plinth_geom::shapes::Triangle;
use std::{
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_PLAN: AtomicU64 = AtomicU64::new(1);

/// Candidate id, scoped to a plan; duplicate ids are refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CandId(pub u32);
/// Contact handle. No public constructor or mutable lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContactId {
    plan: u64,
    ix: usize,
}
/// Admission failures. No rejection is a favourable measurement.
#[derive(Debug, Clone, PartialEq)]
pub enum Reject {
    /// H1 upper bound exceeds the build maximum, mm².
    AreaOverLimit { hi: f64, max: f64 },
    /// Geometry cannot certify a patch.
    PatchUnverified,
    /// More than one model sheet is touched.
    SecondSheet,
    /// Patch envelopes could fuse.
    WouldFuse { with: CandId },
    /// Reserved head regions conflict.
    RegionBlocked { by: CandId },
    /// Cone touches model outside the ball.
    HeadTouchesModel,
    /// Departure cannot be certified monotone.
    DepartureNotMonotone,
    /// Bottom ball cap has no earlier carrier.
    PoleUncovered,
    /// Outside-region gap is insufficient, mm.
    GapTooSmall { gap: f64 },
    /// Member is earlier than its carrier.
    PrintOrder { layer: u32 },
    /// Width across flats, mm, is too small.
    TooThin { across: f64 },
    /// Rod ends do not reach a containing joint centre.
    NarrowJoin,
    /// Raft cannot keep the model gap.
    RaftTouchesModel,
    /// Certified lower section does not carry requested load.
    InsufficientCapacity,
    /// A numeric field is not finite or outside the bounded domain.
    NonFinite,
    /// Consumed quote was for an earlier revision.
    Stale,
    /// Quote or handle belongs to another plan.
    ForeignHandle,
    /// No certified native shape exists.
    UnknownShape,
    /// Geometric bound could not be established.
    MeasureExhausted,
    /// Bounded work or storage limit exceeded.
    ResourceExhausted,
    /// This operation is not built.
    NotBuilt,
}
/// Whole-batch rejection. Failure leaves the builder unchanged.
pub type BatchReject = Reject;
/// Freeze failed; no partially frozen plan escapes.
pub type IncompletePlan = Reject;
/// Proposed fields, already at native f32 precision. Site is in the posed model's
/// local frame. The outward normal is recovered from the frozen surface, not supplied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactCandidate {
    /// Stable candidate id.
    pub id: CandId,
    /// Surface point, mm.
    pub site: [f32; 3],
    /// Ball radius, mm.
    pub ball_r: f32,
    /// Outward normal standoff, mm.
    pub standoff: f32,
    /// Direction toward joint; unit vector.
    pub axis: [f32; 3],
    /// Cone start radius (conservatively at least the H9 minimum), mm.
    pub neck: f32,
    /// Joint radius, mm.
    pub joint_r: f32,
    /// Ball-to-joint centre distance, mm.
    pub head_len: f32,
}
/// An admitted contact, immutable even before freeze.
#[derive(Debug)]
pub struct Contact {
    cand: CandId,
    centre: [f32; 3],
    radius: f32,
    joint: [f32; 3],
    joint_radius: f32,
    patch: CappedPatch,
    parts: Vec<NativePart>,
}
impl Contact {
    /// Candidate identity.
    pub fn candidate_id(&self) -> CandId {
        self.cand
    }
    /// Rounded ball centre.
    pub fn centre(&self) -> [f32; 3] {
        self.centre
    }
    /// Rounded joint centre.
    pub fn joint(&self) -> [f32; 3] {
        self.joint
    }
    /// Joint radius, mm.
    pub fn joint_radius(&self) -> f32 {
        self.joint_radius
    }
    /// Certified patch interval, mm².
    pub fn patch(&self) -> &PatchBound {
        &self.patch.0
    }
    /// Exact checked native parts.
    pub fn parts(&self) -> &[NativePart] {
        &self.parts
    }
}
/// Uncopyable contact quote, bound to exact model, process, limits and revision by plan id.
#[derive(Debug)]
pub struct Quote {
    plan: u64,
    rev: u64,
    contact: Contact,
}
impl Quote {
    /// Candidate id.
    pub fn cand(&self) -> CandId {
        self.contact.cand
    }
    /// Plan revision.
    pub fn rev(&self) -> u64 {
        self.rev
    }
    /// Certified lower-bound capacity at full patch (not early-layer capacity), mm² of peel.
    pub fn full_capacity(&self) -> f64 {
        self.contact.patch.0.lo * 12.0
    }
}
/// Uncopyable all-or-nothing contact batch.
#[derive(Debug)]
pub struct QuoteBatch {
    quotes: Vec<Quote>,
}
impl QuoteBatch {
    /// Collect every answer; any rejection consumes and refuses the entire batch.
    pub fn from_results(results: Vec<Result<Quote, Reject>>) -> Result<Self, Reject> {
        Ok(Self {
            quotes: results.into_iter().collect::<Result<_, _>>()?,
        })
    }
}
/// Contacts may still be admitted.
#[derive(Debug)]
pub struct Contacts;
/// Contacts are final; rooted members may now be admitted.
#[derive(Debug)]
pub struct Routed;
/// Stateful builder. No mutable views, removals, or contact edits.
#[derive(Debug)]
pub struct PlanBuilder<S> {
    id: u64,
    rev: u64,
    workers: usize,
    model: PosedModel,
    limits: Limits,
    process: ProcessRecord,
    contacts: Vec<Contact>,
    members: Vec<NativePart>,
    columns: Vec<members::ColumnAnchor>,
    braces: Vec<members::BraceAnchor>,
    raft: bool,
    _state: PhantomData<S>,
}
impl<S> PlanBuilder<S> {
    /// Current revision.
    pub fn rev(&self) -> u64 {
        self.rev
    }
    /// Read-only contact lookup, rejecting foreign handles before indexing.
    pub fn get(&self, id: ContactId) -> Result<&Contact, Reject> {
        if id.plan != self.id {
            return Err(Reject::ForeignHandle);
        }
        self.contacts.get(id.ix).ok_or(Reject::ForeignHandle)
    }
    /// Number of committed contacts, not records or printable sites.
    pub fn contact_count(&self) -> usize {
        self.contacts.len()
    }
}
impl PlanBuilder<Contacts> {
    /// Fix model, limits and process identity for the lifetime of the plan.
    pub fn new(model: PosedModel, limits: Limits, process: ProcessRecord) -> Self {
        Self {
            id: NEXT_PLAN.fetch_add(1, Ordering::Relaxed),
            rev: 0,
            workers: 1,
            model,
            limits,
            process,
            contacts: Vec::new(),
            members: Vec::new(),
            columns: Vec::new(),
            braces: Vec::new(),
            raft: false,
            _state: PhantomData,
        }
    }
    /// Set this plan's explicit worker allocation before probes. Default is one.
    pub fn with_workers(mut self, workers: usize) -> Result<Self, Reject> {
        if !(1..=8).contains(&workers) {
            return Err(Reject::ResourceExhausted);
        }
        self.workers = workers;
        Ok(self)
    }
    /// Pure whole-layout probe; pair conflicts are checked inside this batch too.
    pub fn probe(&self, cands: &[ContactCandidate], out: &mut Vec<Result<Quote, Reject>>) {
        contact::probe(self, cands, out);
    }
    /// Consume one batch, sorted by candidate id, or change nothing.
    pub fn commit(&mut self, batch: QuoteBatch) -> Result<Vec<ContactId>, BatchReject> {
        let mut qs = batch.quotes;
        for q in &qs {
            if q.plan != self.id {
                return Err(Reject::ForeignHandle);
            }
            if q.rev != self.rev {
                return Err(Reject::Stale);
            }
        }
        qs.sort_by_key(Quote::cand);
        for (i, q) in qs.iter().enumerate() {
            for old in self
                .contacts
                .iter()
                .chain(qs[..i].iter().map(|q| &q.contact))
            {
                contact::pair(&q.contact, old)?;
            }
        }
        let ids = (0..qs.len())
            .map(|i| ContactId {
                plan: self.id,
                ix: self.contacts.len() + i,
            })
            .collect();
        self.contacts.extend(qs.into_iter().map(|q| q.contact));
        self.rev += 1;
        Ok(ids)
    }
    /// Seal contacts before routing. No contacts can be admitted in this state.
    pub fn into_routing(self) -> PlanBuilder<Routed> {
        PlanBuilder {
            id: self.id,
            rev: self.rev,
            workers: self.workers,
            model: self.model,
            limits: self.limits,
            process: self.process,
            contacts: self.contacts,
            members: self.members,
            columns: self.columns,
            braces: self.braces,
            raft: self.raft,
            _state: PhantomData,
        }
    }
}
/// Frozen immutable construction result, not evidence of print eligibility.
#[derive(Debug)]
pub struct SupportPlan {
    model: PosedModel,
    parts: Vec<NativePart>,
    contacts: Vec<Contact>,
    process: ProcessRecord,
}
impl SupportPlan {
    /// Exact checked record geometry.
    pub fn parts(&self) -> &[NativePart] {
        &self.parts
    }
    /// Posed model frozen before placement.
    pub fn model(&self) -> &PosedModel {
        &self.model
    }
    /// Contacts, not record count.
    pub fn contacts(&self) -> &[Contact] {
        &self.contacts
    }
    /// Process settings, not stored in native files.
    pub fn process(&self) -> &ProcessRecord {
        &self.process
    }
    /// The native model triangles (not a reconstructed world-space copy).
    pub fn model_triangles(&self) -> &[Triangle] {
        self.model.triangles()
    }
}
