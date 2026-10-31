use super::native::{ENVELOPE, distance, dot, norm, sub};
use super::*;
use plinth_geom::{Bvh, clearance};

/// Member handle, scoped to a plan. Columns are carriers; braces are not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberId {
    plan: u64,
    ix: usize,
}
/// Rooting identity. Construction accepts contacts or already committed columns only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    /// Build plate.
    Plate,
    /// Rooted column and native height.
    Column(MemberId, f32),
    /// Admitted model contact.
    Contact(ContactId),
}
/// Fixed-size member proposals. No raw radius or unrooted endpoint API.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemberProposal {
    /// One vertical plate-rooted column and its flare per contact.
    Column {
        contact: ContactId,
        section: Section,
    },
    /// A 45-degree brace between already committed columns.
    Brace {
        from: MemberId,
        to: MemberId,
        from_z: f32,
        to_z: f32,
        section: Section,
    },
}
#[derive(Debug)]
pub(super) struct ColumnAnchor {
    contact: usize,
    top: [f32; 3],
    bottom: [f32; 3],
    radius: f32,
    member: usize,
}
/// Uncopyable member quote for one revision and exact checked field/vertex set.
#[derive(Debug)]
pub struct MemberQuote {
    plan: u64,
    rev: u64,
    parts: Vec<NativePart>,
    column: Option<ColumnAnchor>,
    brace: Option<BraceAnchor>,
}
#[derive(Debug, Clone, Copy)]
pub(super) struct BraceAnchor {
    from: usize,
    to: usize,
    from_z: f32,
    to_z: f32,
}
#[derive(Debug, Clone, Copy)]
pub(super) enum MemberRole {
    Column(usize),
    Flare,
    Brace(BraceAnchor),
}
impl MemberQuote {
    fn role(&self, part: usize) -> MemberRole {
        if let Some(c) = &self.column {
            if part == 0 {
                MemberRole::Column(c.contact)
            } else {
                MemberRole::Flare
            }
        } else {
            MemberRole::Brace(self.brace.expect("prepared brace"))
        }
    }
}

/// Private column certificate for mutually exclusive contact alternatives.
#[derive(Debug)]
pub struct ColumnCheck {
    quote: MemberQuote,
    head: Contact,
    gap: f64,
}
impl ColumnCheck {
    /// Radius actually measured against the model, heads and members. Reusing
    /// this certificate for a wider load-sized column is forbidden.
    pub fn radius(&self) -> f32 {
        self.quote
            .column
            .as_ref()
            .expect("column certificate")
            .radius
    }
    /// Exact pair guards reused by final routing; no model query in this hot path.
    pub fn compatible(&self, other: &Self) -> bool {
        if let (Some(a), Some(b)) = (&self.quote.column, &other.quote.column)
            && (f64::from(a.top[2] - a.bottom[2]) > crate::route::BRACE_FREE_LENGTH_MM
                || f64::from(b.top[2] - b.bottom[2]) > crate::route::BRACE_FREE_LENGTH_MM)
            && xy_distance(a, b) < 4.0
        {
            // A 45-degree X has node separation equal to this horizontal span.
            // No long column can have a nearer neighbour than the 4 mm nodes.
            return false;
        }
        for (column, head) in [(&self.quote, &other.head), (&other.quote, &self.head)] {
            for p in &column.parts {
                for h in &head.parts {
                    let (Some(a), Some(b)) = (p.solid(), h.solid()) else {
                        return false;
                    };
                    if clearance::solid_pair_lower(a, b) < self.gap {
                        return false;
                    }
                }
            }
        }
        other
            .quote
            .parts
            .iter()
            .enumerate()
            .all(|(k, p)| keep_member(&self.quote, p, other.quote.role(k), self.gap).is_ok())
    }
}

impl PlanBuilder<Contacts> {
    /// Batch probe column/model and same-chain clearance for alternatives. Cross-
    /// chain guards run during selection and again during real member admission.
    pub fn probe_column_alternatives(
        &self,
        quotes: &[&Quote],
        loads: &[f64],
    ) -> Vec<Result<ColumnCheck, Reject>> {
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
        let routed = scratch.into_routing();
        let prepared: Vec<_> = routed
            .contacts
            .iter()
            .zip(loads)
            .enumerate()
            .map(|(ix, (head, &load))| {
                let section = Section::new(head.column_radius(load))?;
                routed.prepare_member(MemberProposal::Column {
                    contact: ContactId {
                        plan: routed.id,
                        ix,
                    },
                    section,
                })
            })
            .collect();
        let solids: Vec<_> = prepared
            .iter()
            .filter_map(|q| q.as_ref().ok())
            .flat_map(|q| q.parts.iter().filter_map(NativePart::solid))
            .collect();
        let Ok(bvh) = Bvh::build(self.model.mesh()) else {
            return quotes
                .iter()
                .map(|_| Err(Reject::MeasureExhausted))
                .collect();
        };
        let mut gaps = Vec::new();
        clearance::solid_gaps_with_workers(&bvh, &solids, &mut gaps, self.workers);
        let mut at = 0;
        prepared
            .into_iter()
            .enumerate()
            .map(|(ix, q)| {
                let q = q?;
                let measured = &gaps[at..at + q.parts.len()];
                at += q.parts.len();
                for g in measured {
                    let g = g.known().ok_or(Reject::MeasureExhausted)?;
                    if g.lo - ENVELOPE < self.limits.gap {
                        return Err(Reject::GapTooSmall { gap: g.lo });
                    }
                }
                // Own flare has no same-chain exemption.
                for h in &routed.contacts[ix].parts {
                    if clearance::solid_pair_lower(
                        q.parts[1].solid().ok_or(Reject::UnknownShape)?,
                        h.solid().ok_or(Reject::UnknownShape)?,
                    ) < self.limits.gap
                    {
                        return Err(Reject::RegionBlocked {
                            by: quotes[ix].cand(),
                        });
                    }
                }
                Ok(ColumnCheck {
                    quote: q,
                    head: routed.contacts[ix].copy_exact(),
                    gap: self.limits.gap,
                })
            })
            .collect()
    }
}

/// All-or-nothing member batch.
#[derive(Debug)]
pub struct MemberQuoteBatch {
    quotes: Vec<MemberQuote>,
}
impl MemberQuoteBatch {
    /// Reject the entire batch if any member failed admission.
    pub fn from_results(results: Vec<Result<MemberQuote, Reject>>) -> Result<Self, Reject> {
        Ok(Self {
            quotes: results.into_iter().collect::<Result<_, _>>()?,
        })
    }
}
/// Connected raft handle, scoped to this plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaftId {
    plan: u64,
}

impl PlanBuilder<Routed> {
    pub(super) fn finish_preview(mut self) -> Result<(), RoutingFailure> {
        let failure = |reject, candidates| RoutingFailure { reject, candidates };
        let mut blocked = Vec::new();
        for (i, c) in self.columns.iter().enumerate() {
            if f64::from(c.top[2] - c.bottom[2]) <= crate::route::BRACE_FREE_LENGTH_MM {
                continue;
            }
            let nearest = self
                .columns
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .min_by(|(_, a), (_, b)| xy_distance(c, a).total_cmp(&xy_distance(c, b)));
            if nearest.is_none_or(|(_, other)| {
                let d = xy_distance(c, other);
                !(4.0..=15.0).contains(&d)
                    || self.model.plate + 4.2 + d as f32 > c.top[2].min(other.top[2])
            }) {
                blocked.push(self.contacts[c.contact].cand);
            }
        }
        if !blocked.is_empty() {
            return Err(failure(Reject::PrintOrder { layer: 0 }, blocked));
        }
        let braces = self.brace_proposals().map_err(|e| failure(e, Vec::new()))?;
        let mut out = Vec::new();
        self.probe_members(&braces, &mut out);
        if let Some(e) = out.iter().find_map(|q| q.as_ref().err()) {
            let mut candidates = Vec::new();
            for (answer, proposal) in out.iter().zip(&braces) {
                if answer.is_err()
                    && let MemberProposal::Brace { from, to, .. } = proposal
                {
                    for id in [from, to] {
                        if let Ok(c) = self.column(*id) {
                            candidates.push(self.contacts[c.contact].cand);
                        }
                    }
                }
            }
            candidates.sort();
            candidates.dedup();
            return Err(failure(e.clone(), candidates));
        }
        self.commit_members(
            MemberQuoteBatch::from_results(out).map_err(|e| failure(e, Vec::new()))?,
        )
        .map_err(|e| failure(e, Vec::new()))?;
        self.raft_connected().map_err(|e| failure(e, Vec::new()))?;
        self.freeze()
            .map(|_| ())
            .map_err(|e| failure(e, Vec::new()))
    }
    /// Probe fixed-size members against the model in one kernel batch, then keep
    /// every admitted head's reservation. Earlier columns may carry braces, but a
    /// sibling quote cannot be its carrier.
    pub fn probe_members(
        &self,
        proposals: &[MemberProposal],
        out: &mut Vec<Result<MemberQuote, Reject>>,
    ) {
        out.clear();
        if self.raft || proposals.len() > 8192 {
            out.extend(proposals.iter().map(|_| Err(Reject::ResourceExhausted)));
            return;
        }
        let mut prepared: Vec<_> = proposals.iter().map(|p| self.prepare_member(*p)).collect();
        let braces: Vec<_> = prepared
            .iter()
            .filter_map(|q| q.as_ref().ok().and_then(|q| q.brace))
            .collect();
        if let Err(e) = self.check_node_spacing(&braces) {
            // Member batches are committed atomically. Conservatively refuse all
            // proposed braces in an invalid node layout, but keep column results.
            for q in &mut prepared {
                if q.as_ref().is_ok_and(|q| q.brace.is_some()) {
                    *q = Err(e.clone());
                }
            }
        }
        let solids: Vec<_> = prepared
            .iter()
            .filter_map(|p| p.as_ref().ok())
            .flat_map(|q| q.parts.iter().filter_map(NativePart::solid))
            .collect();
        let Ok(bvh) = Bvh::build(self.model.mesh()) else {
            out.extend(proposals.iter().map(|_| Err(Reject::MeasureExhausted)));
            return;
        };
        let mut gaps = Vec::new();
        clearance::solid_gaps_with_workers(&bvh, &solids, &mut gaps, self.workers);
        let mut at = 0;
        for prepared in prepared {
            match prepared {
                Err(e) => out.push(Err(e)),
                Ok(q) => {
                    let count = q.parts.len();
                    let reject = gaps[at..at + count].iter().find_map(|g| match g.known() {
                        None => Some(Reject::MeasureExhausted),
                        Some(g) if g.lo - ENVELOPE < self.limits.gap => Some(Reject::GapTooSmall {
                            gap: (g.lo - ENVELOPE).max(0.0),
                        }),
                        _ => None,
                    });
                    at += count;
                    out.push(reject.map_or_else(|| self.keep_heads(&q).map(|()| q), Err));
                }
            }
        }
        // Symmetric pair failures: neither input order can choose a winner.
        let mut rejected = vec![None; out.len()];
        for (i, result) in out.iter().enumerate() {
            let Ok(q) = result else {
                continue;
            };
            for (part, &role) in self.members.iter().zip(&self.member_roles) {
                if let Err(e) = keep_member(q, part, role, self.limits.gap) {
                    rejected[i] = Some(e);
                    break;
                }
            }
            for (j, other) in out[..i].iter().enumerate() {
                let Ok(other) = other else {
                    continue;
                };
                for (k, part) in other.parts.iter().enumerate() {
                    if let Err(e) = keep_member(q, part, other.role(k), self.limits.gap) {
                        rejected[i] = Some(e.clone());
                        rejected[j] = Some(e);
                    }
                }
            }
        }
        for (answer, reject) in out.iter_mut().zip(rejected) {
            if let Some(e) = reject {
                *answer = Err(e);
            }
        }
    }
    // Actual finite solids, with native representation envelopes included. Only
    // a column's own centre-to-centre joint connection is exempt.
    fn keep_heads(&self, quote: &MemberQuote) -> Result<(), Reject> {
        for (ix, head) in self.contacts.iter().enumerate() {
            let own_column = quote.column.as_ref().is_some_and(|c| c.contact == ix);
            for part in &quote.parts {
                if own_column && part.kind() == PartKind::Rod {
                    continue;
                }
                for head_part in &head.parts {
                    let lower = clearance::solid_pair_lower(
                        part.solid().ok_or(Reject::UnknownShape)?,
                        head_part.solid().ok_or(Reject::UnknownShape)?,
                    );
                    if lower < self.limits.gap {
                        return Err(Reject::RegionBlocked { by: head.cand });
                    }
                }
            }
        }
        Ok(())
    }
    fn prepare_member(&self, proposal: MemberProposal) -> Result<MemberQuote, Reject> {
        let (parts, column, brace) = match proposal {
            MemberProposal::Column { contact, section } => {
                let c = self.get(contact)?;
                if section.radius() < 0.25
                    || f64::from(section.radius()) + ENVELOPE > c.parts[2].ball_inner()?.radius
                {
                    return Err(Reject::NarrowJoin);
                }
                if self.columns.iter().any(|c| c.contact == contact.ix) {
                    return Err(Reject::NarrowJoin);
                }
                let top = c.joint;
                let bottom = [top[0], top[1], self.model.plate + 1.196];
                if top[2] <= bottom[2] + 0.05 {
                    return Err(Reject::PrintOrder { layer: 0 });
                }
                let flare_bottom = [top[0], top[1], self.model.plate + 0.198];
                let flare_top = [top[0], top[1], self.model.plate + 1.198];
                let rod = NativePart::frustum(PartKind::Rod, top, bottom, section, section)?;
                let flare = NativePart::frustum(
                    PartKind::BaseFlare,
                    flare_top,
                    flare_bottom,
                    section,
                    Section::new(1.1)?,
                )?;
                (
                    vec![rod, flare],
                    Some(ColumnAnchor {
                        contact: contact.ix,
                        top,
                        bottom,
                        radius: section.radius(),
                        member: 0,
                    }),
                    None,
                )
            }
            MemberProposal::Brace {
                from,
                to,
                from_z,
                to_z,
                section,
            } => {
                let a = self.column(from)?;
                let b = self.column(to)?;
                if !from_z.is_finite() || !to_z.is_finite() {
                    return Err(Reject::NonFinite);
                }
                if from == to || section.radius() > a.radius.min(b.radius) {
                    return Err(Reject::NarrowJoin);
                }
                let start = [a.top[0], a.top[1], from_z];
                let end = [b.top[0], b.top[1], to_z];
                let xy = ((f64::from(start[0] - end[0])).powi(2)
                    + (f64::from(start[1] - end[1])).powi(2))
                .sqrt();
                if xy > 15.0
                    || (f64::from(from_z - to_z).abs() - xy).abs() > 0.001
                    || from_z < self.model.plate + 4.2
                    || to_z < self.model.plate + 4.2
                    || from_z > a.top[2]
                    || to_z > b.top[2]
                    || from_z < a.bottom[2]
                    || to_z < b.bottom[2]
                {
                    return Err(Reject::PrintOrder {
                        layer: ((f64::from(from_z - self.model.plate)
                            / self.process.layer_height_mm)
                            .max(0.0)) as u32,
                    });
                }
                (
                    vec![NativePart::frustum(
                        PartKind::Rod,
                        start,
                        end,
                        section,
                        section,
                    )?],
                    None,
                    Some(BraceAnchor {
                        from: a.contact,
                        to: b.contact,
                        from_z,
                        to_z,
                    }),
                )
            }
        };
        Ok(MemberQuote {
            plan: self.id,
            rev: self.rev,
            parts,
            column,
            brace,
        })
    }
    fn column(&self, id: MemberId) -> Result<&ColumnAnchor, Reject> {
        if id.plan != self.id {
            return Err(Reject::ForeignHandle);
        }
        self.columns
            .iter()
            .find(|c| c.member == id.ix)
            .ok_or(Reject::ForeignHandle)
    }
    fn check_node_spacing(&self, proposed: &[BraceAnchor]) -> Result<(), Reject> {
        for column in &self.columns {
            let mut nodes: Vec<f32> = self
                .braces
                .iter()
                .chain(proposed)
                .filter_map(|b| {
                    if b.from == column.contact {
                        Some(b.from_z)
                    } else if b.to == column.contact {
                        Some(b.to_z)
                    } else {
                        None
                    }
                })
                .collect();
            nodes.sort_by(f32::total_cmp);
            nodes.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
            if nodes.windows(2).any(|w| w[1] - w[0] < 4.0) {
                return Err(Reject::PrintOrder { layer: 0 });
            }
        }
        Ok(())
    }
    /// Consume a whole member batch or leave the builder unchanged.
    pub fn commit_members(
        &mut self,
        batch: MemberQuoteBatch,
    ) -> Result<Vec<MemberId>, BatchReject> {
        for (i, q) in batch.quotes.iter().enumerate() {
            if q.plan != self.id {
                return Err(Reject::ForeignHandle);
            }
            if q.rev != self.rev {
                return Err(Reject::Stale);
            }
            if let Some(c) = &q.column
                && (self.columns.iter().any(|old| old.contact == c.contact)
                    || batch.quotes[..i].iter().any(|old| {
                        old.column
                            .as_ref()
                            .is_some_and(|old| old.contact == c.contact)
                    }))
            {
                return Err(Reject::NarrowJoin);
            }
        }
        let braces: Vec<_> = batch.quotes.iter().filter_map(|q| q.brace).collect();
        self.check_node_spacing(&braces)?;
        for (i, q) in batch.quotes.iter().enumerate() {
            for (part, &role) in self.members.iter().zip(&self.member_roles) {
                keep_member(q, part, role, self.limits.gap)?;
            }
            for other in &batch.quotes[..i] {
                for (k, part) in other.parts.iter().enumerate() {
                    keep_member(q, part, other.role(k), self.limits.gap)?;
                }
            }
        }
        let mut ids = Vec::new();
        for mut q in batch.quotes {
            let ix = self.members.len();
            self.member_roles
                .extend((0..q.parts.len()).map(|k| q.role(k)));
            ids.push(MemberId { plan: self.id, ix });
            if let Some(mut c) = q.column.take() {
                c.member = ix;
                self.columns.push(c);
            }
            if let Some(brace) = q.brace.take() {
                self.braces.push(brace);
            }
            self.members.extend(q.parts);
        }
        self.rev += 1;
        Ok(ids)
    }
    /// Build pads and deterministic MST bridges. Each nonzero bridge contains its
    /// centreline and overlaps the pad at BOTH endpoints by positive volume. Thus
    /// tree connectivity proves connectivity of the actual closed solids, not just
    /// of a bridge list. All vertices keep the model clearance by a Z-separation proof.
    pub fn raft_connected(&mut self) -> Result<RaftId, Reject> {
        if self.raft || self.columns.len() != self.contacts.len() || self.columns.is_empty() {
            return Err(Reject::NotBuilt);
        }
        let top = self.model.plate + 0.2;
        if self.model.mesh().bounds().min[2] - f64::from(top) < self.limits.gap + ENVELOPE {
            return Err(Reject::RaftTouchesModel);
        }
        // Pads and bridges lie below top. Keep even the conservative complete-head
        // reservations above them; the model-only gap does not prove this fact.
        for head in &self.contacts {
            let radius = distance(head.centre, head.joint)
                + f64::from(head.joint_radius.max(head.radius))
                + 2.0 * ENVELOPE;
            if f64::from(head.centre[2]) - radius <= f64::from(top) + ENVELOPE {
                return Err(Reject::RegionBlocked { by: head.cand });
            }
        }
        let mut centres: Vec<_> = self.columns.iter().map(|c| [c.top[0], c.top[1]]).collect();
        centres.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
        centres.dedup_by(|a, b| {
            a.iter()
                .zip(b.iter())
                .all(|(a, b)| a.to_bits() == b.to_bits())
        });
        let mut bridges = Vec::new();
        crate::raft::bridge_pairs(&centres, &mut bridges);
        if bridges.len() + 1 != centres.len() {
            return Err(Reject::MeasureExhausted);
        }
        let mut parts: Vec<_> = centres.iter().map(|&c| NativePart::pad(c, top)).collect();
        for [a, b] in bridges {
            parts.push(NativePart::bridge(
                centres[a as usize],
                centres[b as usize],
                top,
            ));
        }
        // Native f32 rounding must not collapse the overlap or thickness.
        if parts
            .iter()
            .any(|p| p.mesh().iter().flatten().flatten().any(|v| !v.is_finite()))
        {
            return Err(Reject::NonFinite);
        }
        self.members.extend(parts);
        self.raft = true;
        self.rev += 1;
        Ok(RaftId { plan: self.id })
    }
    fn has_x_pair(&self, a: usize, b: usize) -> bool {
        let orient = |brace: &BraceAnchor| {
            if brace.from == a && brace.to == b {
                Some((brace.from_z, brace.to_z))
            } else if brace.from == b && brace.to == a {
                Some((brace.to_z, brace.from_z))
            } else {
                None
            }
        };
        self.braces.iter().filter_map(orient).any(|(lo, hi)| {
            hi - lo >= 4.0
                && self
                    .braces
                    .iter()
                    .filter_map(orient)
                    .any(|(x, y)| (x - hi).abs() < 1e-5 && (y - lo).abs() < 1e-5)
        })
    }
    /// Propose the owner's 45° X-pair to each long column's nearest carrier.
    /// Insufficient vertical room is a rejection, not permission to change the rule.
    pub fn brace_proposals(&self) -> Result<Vec<MemberProposal>, Reject> {
        let mut pairs = std::collections::BTreeSet::new();
        for (i, c) in self.columns.iter().enumerate() {
            if f64::from(c.top[2] - c.bottom[2]) <= crate::route::BRACE_FREE_LENGTH_MM {
                continue;
            }
            let (j, other) = self
                .columns
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .min_by(|(_, a), (_, b)| {
                    xy_distance(c, a)
                        .total_cmp(&xy_distance(c, b))
                        .then(a.contact.cmp(&b.contact))
                })
                .ok_or(Reject::PrintOrder { layer: 0 })?;
            let distance = xy_distance(c, other);
            if !(4.0..=15.0).contains(&distance) {
                return Err(Reject::PrintOrder { layer: 0 });
            }
            pairs.insert((i.min(j), i.max(j)));
        }
        let mut proposals = Vec::new();
        for (a, b) in pairs {
            let from = &self.columns[a];
            let to = &self.columns[b];
            let lo = self.model.plate + 4.2;
            let hi = lo + xy_distance(from, to) as f32;
            if hi > from.top[2].min(to.top[2]) {
                return Err(Reject::PrintOrder { layer: 0 });
            }
            let section = Section::new(0.25)?;
            let from = MemberId {
                plan: self.id,
                ix: from.member,
            };
            let to = MemberId {
                plan: self.id,
                ix: to.member,
            };
            proposals.push(MemberProposal::Brace {
                from,
                to,
                from_z: lo,
                to_z: hi,
                section,
            });
            proposals.push(MemberProposal::Brace {
                from,
                to,
                from_z: hi,
                to_z: lo,
                section,
            });
        }
        Ok(proposals)
    }
    /// Freeze only a complete rooted fixed-size layout. Long unbraced columns are
    /// not silently accepted: bracing completion is currently a caller obligation
    /// checked here by the presence of a brace at every long column.
    pub fn freeze(self) -> Result<SupportPlan, IncompletePlan> {
        if !self.raft || self.columns.len() != self.contacts.len() {
            return Err(Reject::NotBuilt);
        }
        for c in &self.columns {
            if f64::from(c.top[2] - c.bottom[2]) > crate::route::BRACE_FREE_LENGTH_MM {
                let nearest = self
                    .columns
                    .iter()
                    .filter(|other| other.contact != c.contact)
                    .min_by(|a, b| {
                        xy_distance(c, a)
                            .total_cmp(&xy_distance(c, b))
                            .then(a.contact.cmp(&b.contact))
                    })
                    .ok_or(Reject::PrintOrder { layer: 0 })?;
                if xy_distance(c, nearest) > 15.0 || !self.has_x_pair(c.contact, nearest.contact) {
                    return Err(Reject::PrintOrder { layer: 0 });
                }
            }
        }
        let mut parts = Vec::new();
        for c in &self.contacts {
            // Rebuild-free transfer of the exact checked meshes into the frozen plan.
            // Contacts are retained separately below, so parts are copied byte-for-byte.
            for p in &c.parts {
                parts.push(p.copy_exact());
            }
        }
        parts.extend(self.members);
        Ok(SupportPlan {
            model: self.model,
            parts,
            contacts: self.contacts,
            process: self.process,
        })
    }
}
fn designated_join(a: MemberRole, b: MemberRole) -> bool {
    match (a, b) {
        (MemberRole::Column(c), MemberRole::Brace(b))
        | (MemberRole::Brace(b), MemberRole::Column(c)) => b.from == c || b.to == c,
        (MemberRole::Brace(a), MemberRole::Brace(b)) => {
            let (lo, hi) = if a.from == b.from && a.to == b.to {
                (b.from_z, b.to_z)
            } else if a.from == b.to && a.to == b.from {
                (b.to_z, b.from_z)
            } else {
                return false;
            };
            // The ONLY brace/brace join is the exact crossed X-pair, not a near lens.
            a.from_z.to_bits() == hi.to_bits() && a.to_z.to_bits() == lo.to_bits()
        }
        _ => false,
    }
}

fn segment_distance(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> f64 {
    let u = sub(b, a);
    let v = sub(d, c);
    let w = sub(a, c);
    let aa = dot(u, u);
    let bb = dot(u, v);
    let cc = dot(v, v);
    let dd = dot(u, w);
    let ee = dot(v, w);
    let project = |p, x, delta, length: f64| {
        let t = (dot(sub(p, x), delta) / length).clamp(0.0, 1.0);
        std::array::from_fn(|k| x[k] + t * delta[k])
    };
    let mut directions = vec![
        sub(a, project(a, c, v, cc)),
        sub(b, project(b, c, v, cc)),
        sub(project(c, a, u, aa), c),
        sub(project(d, a, u, aa), d),
    ];
    let det = aa * cc - bb * bb;
    if det > 1e-12 * aa * cc {
        let s = (bb * ee - cc * dd) / det;
        let t = (aa * ee - bb * dd) / det;
        directions.push(std::array::from_fn(|k| w[k] + s * u[k] - t * v[k]));
    }
    // Directions are merely proposals. Only separating support planes establish
    // lower bounds; even an inaccurate/ill-conditioned closest-point solve cannot
    // produce a false pass. Parallel and intersecting axes are covered as well.
    directions
        .into_iter()
        .filter_map(|n| {
            let length = norm(n);
            if length < 1e-12 {
                return None;
            }
            let n = n.map(|v| v / length);
            let lower = dot(sub(a, c), n)
                .min(dot(sub(b, c), n))
                .min(dot(sub(a, d), n))
                .min(dot(sub(b, d), n));
            Some((lower - 1e-7).max(0.0))
        })
        .fold(0.0, f64::max)
}

fn keep_member(
    q: &MemberQuote,
    other: &NativePart,
    role: MemberRole,
    gap: f64,
) -> Result<(), Reject> {
    let PartFields::Frustum {
        start: c,
        end: d,
        r1: cr,
        r2: dr,
    } = other.fields()
    else {
        return Err(Reject::UnknownShape);
    };
    for (k, part) in q.parts.iter().enumerate() {
        if designated_join(q.role(k), role)
            || matches!((q.role(k), role), (MemberRole::Flare, MemberRole::Flare))
        {
            continue; // Owner permits plate bases to merge, never heads.
        }
        let PartFields::Frustum {
            start: a,
            end: b,
            r1: ar,
            r2: br,
        } = part.fields()
        else {
            return Err(Reject::UnknownShape);
        };
        let capsule = segment_distance(
            a.map(f64::from),
            b.map(f64::from),
            c.map(f64::from),
            d.map(f64::from),
        ) - f64::from(ar.max(br))
            - f64::from(cr.max(dr))
            - 2.0 * ENVELOPE;
        let lower = capsule.max(clearance::solid_pair_lower(
            part.solid().ok_or(Reject::UnknownShape)?,
            other.solid().ok_or(Reject::UnknownShape)?,
        ));
        if lower < gap {
            return Err(Reject::GapTooSmall {
                gap: lower.max(0.0),
            });
        }
    }
    Ok(())
}

fn xy_distance(a: &ColumnAnchor, b: &ColumnAnchor) -> f64 {
    (f64::from(a.top[0] - b.top[0]).powi(2) + f64::from(a.top[1] - b.top[1]).powi(2)).sqrt()
}

#[cfg(test)]
#[path = "member_separation_tests.rs"]
mod separation_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construct::tests::{builder, candidate, plate};
    #[test]
    fn member_gap_straddles_h6_build_reserve() {
        for gap in [0.31f64, 0.33] {
            let mut p = builder(&plate());
            let mut out = Vec::new();
            p.probe(&[candidate(0, 0.)], &mut out);
            let contacts = p.commit(QuoteBatch::from_results(out).unwrap()).unwrap();
            let mut p = p.into_routing();
            // PRIVATE fixture isolates member admission from the still-restricted
            // contact departure domain. Add a finite vertical sheet beside the rod
            // after creating the carrier. No production model-mutation API exists.
            // The sheet is below the joint and above the flare; analytic rod gap
            // is x - 0.25. These are not whole-plan/concave-admission controls.
            let x = 0.25 + gap;
            let mesh = plinth_geom::Mesh::new(
                vec![-30., 30., 30., -30., x, x, x, x],
                vec![-30., -30., 30., 30., -0.1, 0.1, 0.1, -0.1],
                vec![0., 0., 0., 0., -3.35, -3.35, -3.25, -3.25],
                vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7],
            )
            .unwrap();
            // Preserve coordinates and plate so only this routing guard changes.
            p.model.mesh_for_member_gap_test(mesh);
            let proposals = [MemberProposal::Column {
                contact: contacts[0],
                section: Section::new(0.25).unwrap(),
            }];
            let mut out = Vec::new();
            p.probe_members(&proposals, &mut out);
            if gap < 0.32 {
                assert!(
                    matches!(&out[0], Err(Reject::GapTooSmall { gap })
                    if (gap - 0.31).abs() < 0.0001),
                    "gap rejection missing: {:?}",
                    out[0].as_ref().err()
                );
            } else {
                p.commit_members(MemberQuoteBatch::from_results(out).unwrap())
                    .unwrap();
                assert_eq!(p.columns.len(), 1);
            }
        }
    }

    #[test]
    fn member_rejections_and_private_raft_fault_injection_have_witnesses() {
        let mut p = builder(&plate());
        let mut out = Vec::new();
        p.probe(&[candidate(0, -10.), candidate(1, 10.)], &mut out);
        let contacts = p.commit(QuoteBatch::from_results(out).unwrap()).unwrap();
        let mut p = p.into_routing();
        let mut errors = vec![
            p.prepare_member(MemberProposal::Column {
                contact: contacts[0],
                section: Section::new(0.4).unwrap(),
            })
            .unwrap_err(),
        ];
        let proposals: Vec<_> = contacts
            .iter()
            .map(|&contact| MemberProposal::Column {
                contact,
                section: Section::new(0.25).unwrap(),
            })
            .collect();
        let mut out = Vec::new();
        p.probe_members(&proposals, &mut out);
        let ids = p
            .commit_members(MemberQuoteBatch::from_results(out).unwrap())
            .unwrap();
        errors.push(
            p.prepare_member(MemberProposal::Brace {
                from: ids[0],
                to: ids[1],
                from_z: p.model.plate + 1.0,
                to_z: p.model.plate + 2.0,
                section: Section::new(0.25).unwrap(),
            })
            .unwrap_err(),
        );
        // The public API cannot lower the plate or forge a pose. This PRIVATE unit
        // fault injection exercises the redundant raft/model guard, not a legal API path.
        p.model.plate = -0.1;
        errors.push(p.raft_connected().unwrap_err());
        assert!(!p.raft);
        errors.push(p.freeze().unwrap_err());
        let counts = crate::construct::tests::rejection_counts(&errors);
        for path in ["NarrowJoin", "PrintOrder", "NotBuilt", "RaftTouchesModel"] {
            assert_eq!(counts.get(path), Some(&1));
        }
        eprintln!(
            "member rejection hits (RaftTouchesModel uses private state corruption): {counts:?}"
        );
    }
}
