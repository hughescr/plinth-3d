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
#[cfg(test)]
mod local_tests;
mod members;
mod native;
#[cfg(test)]
mod tests;
mod types;

pub use contact::require_capacity;
pub use members::{
    Anchor, ColumnCheck, MemberId, MemberProposal, MemberQuote, MemberQuoteBatch, RaftId,
};
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
/// Located private-route failure. Sites must be blocked and re-planned, not
/// committed and then repaired. Empty candidates means a whole-layout failure.
#[derive(Debug)]
pub struct RoutingFailure {
    /// Exact construction rejection.
    pub reject: Reject,
    /// Candidate identities responsible for the rejected members.
    pub candidates: Vec<CandId>,
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
    fn copy_exact(&self) -> Self {
        Self {
            cand: self.cand,
            centre: self.centre,
            radius: self.radius,
            joint: self.joint,
            joint_radius: self.joint_radius,
            patch: CappedPatch(PatchBound {
                lo: self.patch.0.lo,
                hi: self.patch.0.hi,
            }),
            parts: self.parts.iter().map(NativePart::copy_exact).collect(),
        }
    }
    /// Desired load-sized radius for private resizing, rounded upward.
    pub fn required_column_radius(&self, load: f64) -> f32 {
        let r = f64::from(self.joint_radius / 1.04)
            .max((load / 12.0 / 3.0615).sqrt())
            .max(0.25);
        let f = r as f32;
        if f64::from(f) < r { f.next_up() } else { f }
    }
    /// Fixed column admitted by this head. A load that exceeds the available
    /// joint is a capacity miss, not a broken join. Private planning first tries
    /// larger heads and neighbours; §5.4's capped fallback retains the legal
    /// geometry and the final load replay reports its located overload.
    pub fn column_radius(&self, load: f64) -> f32 {
        self.required_column_radius(load)
            .min(self.joint_radius / 1.04)
    }
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
    /// Geometry certified by this private proposal; immutable until commit.
    pub fn contact(&self) -> &Contact {
        &self.contact
    }
    /// Same solid-against-solid H2/H6 guards used again at atomic commit.
    pub fn compatible(&self, other: &Self) -> bool {
        self.plan == other.plan && contact::pair(&self.contact, &other.contact).is_ok()
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
    patch_cache: contact::PatchCache,
    workers: usize,
    model: PosedModel,
    limits: Limits,
    process: ProcessRecord,
    contacts: Vec<Contact>,
    members: Vec<NativePart>,
    member_roles: Vec<members::MemberRole>,
    columns: Vec<members::ColumnAnchor>,
    braces: Vec<members::BraceAnchor>,
    raft: bool,
    _state: PhantomData<S>,
}
impl<S> PlanBuilder<S> {
    /// The immutable frozen model; sites use its local frame.
    pub fn model(&self) -> &PosedModel {
        &self.model
    }
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
            patch_cache: std::sync::Mutex::default(),
            workers: 1,
            model,
            limits,
            process,
            contacts: Vec::new(),
            members: Vec::new(),
            member_roles: Vec::new(),
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
        contact::probe(self, cands, out, false);
    }
    /// Batch-measure mutually exclusive private alternatives. These quotes cannot
    /// bypass pair guards: commit checks the whole selected batch again.
    pub fn probe_alternatives(
        &self,
        cands: &[ContactCandidate],
        out: &mut Vec<Result<Quote, Reject>>,
    ) {
        contact::probe(self, cands, out, true);
    }
    /// Private member feasibility on exact copies of measured quotes. No quote is
    /// cloned or admitted to this plan. Results correspond to the supplied order.
    pub fn probe_columns(&self, quotes: &[&Quote], loads: &[f64]) -> Vec<Result<(), Reject>> {
        if quotes.len() != loads.len()
            || quotes
                .iter()
                .any(|q| q.plan != self.id || q.rev != self.rev)
        {
            return quotes.iter().map(|_| Err(Reject::ForeignHandle)).collect();
        }
        let mut scratch = Self::new(
            self.model.copy_exact(),
            self.limits.clone(),
            self.process.clone(),
        );
        scratch.workers = self.workers;
        scratch.contacts = quotes.iter().map(|q| q.contact.copy_exact()).collect();
        let proposals: Result<Vec<_>, _> = scratch
            .contacts
            .iter()
            .zip(loads)
            .enumerate()
            .map(|(ix, (c, &load))| {
                Section::new(c.column_radius(load)).map(|section| MemberProposal::Column {
                    contact: ContactId {
                        plan: scratch.id,
                        ix,
                    },
                    section,
                })
            })
            .collect();
        let Ok(proposals) = proposals else {
            return quotes.iter().map(|_| Err(Reject::NonFinite)).collect();
        };
        let routed = scratch.into_routing();
        let mut out = Vec::new();
        routed.probe_members(&proposals, &mut out);
        out.into_iter().map(|q| q.map(|_| ())).collect()
    }
    /// Replay the entire private route (columns, required braces, raft, freeze)
    /// before mutating the real builder. A successful column probe alone is not a
    /// routed plan. Exact copies cannot acquire stronger certificates than commit.
    pub fn preview_routing(&self, quotes: &[&Quote], loads: &[f64]) -> Result<(), Reject> {
        self.preview_routing_located(quotes, loads)
            .map_err(|f| f.reject)
    }
    /// Full route replay with candidate identities for feedback into the next
    /// private search round. Uses the actual tributary loads, not zero-load rods.
    pub fn preview_routing_located(
        &self,
        quotes: &[&Quote],
        loads: &[f64],
    ) -> Result<(), RoutingFailure> {
        self.preview_plan(quotes, loads).map(|_| ())
    }
    /// Exact private frozen route for the final load replay, with no real commit.
    pub fn preview_plan(
        &self,
        quotes: &[&Quote],
        loads: &[f64],
    ) -> Result<SupportPlan, RoutingFailure> {
        let failure = |reject, candidates| RoutingFailure { reject, candidates };
        if quotes.len() != loads.len()
            || quotes
                .iter()
                .any(|q| q.plan != self.id || q.rev != self.rev)
        {
            return Err(failure(Reject::ForeignHandle, Vec::new()));
        }
        let mut scratch = Self::new(
            self.model.copy_exact(),
            self.limits.clone(),
            self.process.clone(),
        );
        scratch.workers = self.workers;
        for (i, q) in quotes.iter().enumerate() {
            for other in &quotes[..i] {
                contact::pair(&q.contact, &other.contact)
                    .map_err(|e| failure(e, vec![q.cand()]))?;
            }
        }
        scratch.contacts = quotes.iter().map(|q| q.contact.copy_exact()).collect();
        let columns = scratch
            .contacts
            .iter()
            .zip(loads)
            .enumerate()
            .map(|(ix, (head, &load))| {
                Section::new(head.column_radius(load)).map(|section| MemberProposal::Column {
                    contact: ContactId {
                        plan: scratch.id,
                        ix,
                    },
                    section,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| failure(e, Vec::new()))?;
        let mut routed = scratch.into_routing();
        let mut out = Vec::new();
        routed.probe_members(&columns, &mut out);
        if let Some(e) = out.iter().find_map(|q| q.as_ref().err()) {
            return Err(failure(
                e.clone(),
                out.iter()
                    .zip(quotes)
                    .filter_map(|(q, head)| q.is_err().then_some(head.cand()))
                    .collect(),
            ));
        }
        routed
            .commit_members(
                MemberQuoteBatch::from_results(out).map_err(|e| failure(e, Vec::new()))?,
            )
            .map_err(|e| failure(e, Vec::new()))?;
        routed.finish_preview()
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
            patch_cache: self.patch_cache,
            workers: self.workers,
            model: self.model,
            limits: self.limits,
            process: self.process,
            contacts: self.contacts,
            members: self.members,
            member_roles: self.member_roles,
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
    unbraced: Vec<u32>,
    model: PosedModel,
    parts: Vec<NativePart>,
    contacts: Vec<Contact>,
    process: ProcessRecord,
}
impl SupportPlan {
    /// Long columns without an admitted X-pair, indexed by contact for pictures.
    pub fn unbraced_columns(&self) -> &[u32] {
        &self.unbraced
    }
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
