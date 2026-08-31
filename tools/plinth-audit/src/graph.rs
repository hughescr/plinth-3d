//! Support-graph reconstruction from endpoint coincidence.
//!
//! **No record carries a parent id, a child id, a group id, or any other
//! topology field** (`audit-contract.md` §3.1). Position within the record
//! array binds a support to a *model* and to nothing finer. The graph is
//! therefore rebuilt from geometry, under two rules that are not optional:
//!
//! 1. **For a ball (types 7 and 9) the connective point is
//!    `midpoint(start, end)`, not `start` and not `end`.** The poles connect to
//!    nothing. This is the single most likely way to build a wrong graph.
//! 2. **A type-6 pad has `start == end`** and contributes one point, not two.
//!    A bridge has two distinct endpoints.
//!
//! **Tolerance.** `audit-contract.md` §3.2 measured a clean five-decade empty
//! band on emitted bytes between the worst true coincidence (1e-6 mm) and the
//! smallest real separation (1e-1 mm); any tolerance inside it builds the same
//! graph. Emitted files use `JOIN_TOL_EMITTED_MM = 1e-3`; native, UI-authored
//! files need `JOIN_TOL_NATIVE_MM = 2e-2`. The class is RECONSTRUCTED from the
//! ball tessellation, never declared.
//!
//! **Ambiguity is represented, never resolved.** Where a point has more than
//! one candidate partner, where a join sits close enough to the tolerance that
//! widening it by 10x would change the answer, or where a tip has no unique
//! taper, the graph records it in [`SupportGraph::ambiguities`]. An ambiguous
//! graph is a finding, and every consumer must treat the ambiguity as
//! `indeterminate` rather than picking one.

use crate::reader::{ModelEntry, RecordKind};
use crate::vec3::{self as v, V3};
use std::collections::BTreeSet;

/// Which point of a record is being joined. The role is kept so a consumer can
/// tell "the taper's narrow end met the tip's centre" from "two arms met".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PointRole {
    /// The centre of a type-7 or type-9 ball. Its poles connect to nothing.
    BallCentre,
    /// `start` of a type 1/3/4 frustum.
    FrustumStart,
    /// `end` of a type 1/3/4 frustum.
    FrustumEnd,
    /// A type-6 pad's single point.
    PadCentre,
    BridgeStart,
    BridgeEnd,
}

impl PointRole {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::BallCentre => "centre",
            Self::FrustumStart | Self::BridgeStart => "start",
            Self::FrustumEnd | Self::BridgeEnd => "end",
            Self::PadCentre => "pad",
        }
    }
    /// `true` for the two roles that the `PATH_FREE_END` failure line reports
    /// as `end=<start|end>`.
    #[must_use]
    pub const fn is_frustum_end(self) -> bool {
        matches!(self, Self::FrustumStart | Self::FrustumEnd)
    }
}

/// One connective point of one record.
#[derive(Clone, Copy, Debug)]
pub struct ConnectivePoint {
    /// Record index within the model.
    pub record: usize,
    /// Index of this point within the record's own point list (0 or 1).
    pub slot: usize,
    pub role: PointRole,
    pub pos: V3,
    /// The solid's radius at this point, mm. A ball reports its own radius.
    pub radius_mm: f64,
}

/// A reconstructed join between two connective points.
#[derive(Clone, Copy, Debug)]
pub struct Join {
    pub a: PointRef,
    pub b: PointRef,
    pub distance_mm: f64,
}

/// `(record, slot)`.
pub type PointRef = (usize, usize);

/// Something the reconstruction could not settle. **Every variant must be
/// treated as `indeterminate` by its consumer, never resolved by picking.**
#[derive(Clone, Debug)]
pub enum Ambiguity {
    /// More than one candidate partner inside the tolerance for one point.
    MultipleCandidates {
        point: PointRef,
        candidates: Vec<(PointRef, f64)>,
    },
    /// A pair whose distance sits in `(tol, 10*tol]`: outside the tolerance,
    /// but close enough that the tolerance choice decided the answer. The
    /// contract's empty band says this should be EMPTY on emitted bytes.
    NearBand {
        a: PointRef,
        b: PointRef,
        distance_mm: f64,
    },
    /// A connective point with no partner at all. For a frustum end this is
    /// the raw material of `PATH_FREE_END`; it is reported here without
    /// judging it, because a grounded end legitimately has no partner.
    Unjoined { point: PointRef },
    /// A type-7 tip with no unique adjacent type-1 taper. `SEAT_DEPTH_*` reads
    /// the declared depth from that taper, so a tip without one cannot have
    /// its declared depth compared at all.
    TipTaperNotUnique {
        tip_record: usize,
        candidates: Vec<usize>,
    },
}

impl Ambiguity {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::MultipleCandidates { .. } => "multiple_candidates",
            Self::NearBand { .. } => "near_band",
            Self::Unjoined { .. } => "unjoined",
            Self::TipTaperNotUnique { .. } => "tip_taper_not_unique",
        }
    }
}

/// The answer to "which taper belongs to this tip".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    /// Exactly one candidate. The only value a check may act on.
    Unique(usize),
    /// More than one; a finding, and `indeterminate` for the consumer.
    Ambiguous(Vec<usize>),
    /// None at all; `reason=no_adjacent_taper` in `SEAT_DEPTH_INDETERMINATE`.
    Missing,
}

impl Link {
    #[must_use]
    pub const fn unique(&self) -> Option<usize> {
        match self {
            Self::Unique(i) => Some(*i),
            _ => None,
        }
    }
    #[must_use]
    pub const fn is_unique(&self) -> bool {
        matches!(self, Self::Unique(_))
    }
    /// The `reason=` token for a failure line when this is not unique.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Unique(_) => "unique",
            Self::Ambiguous(_) => "taper_ambiguous",
            Self::Missing => "no_adjacent_taper",
        }
    }
}

/// The reconstructed support graph for one model.
///
/// Nodes are records, indexed exactly as `ModelEntry::records` is. Edges are
/// undirected joins between connective points. Nothing here is directed:
/// rootward order is a *derived* quantity from a ground set, produced by
/// [`SupportGraph::distance_to_ground`], because a record carries no
/// orientation field either.
#[derive(Clone, Debug)]
pub struct SupportGraph {
    /// Every connective point, flat, in `(record, slot)` order.
    pub points: Vec<ConnectivePoint>,
    /// `point_index[record]` is the range of `points` this record owns.
    pub point_index: Vec<(usize, usize)>,
    pub joins: Vec<Join>,
    /// Adjacency by record: `neighbours[r]` is the sorted set of records
    /// joined to `r` by at least one point pair.
    pub neighbours: Vec<Vec<usize>>,
    pub ambiguities: Vec<Ambiguity>,
    /// The tolerance actually used, and the class it came from.
    pub join_tol_mm: f64,
    pub file_class_label: &'static str,
    /// Connected components over the join relation, per record.
    pub component_of_record: Vec<usize>,
    pub component_count: usize,
}

impl SupportGraph {
    /// Rebuild the graph for one model at the given endpoint tolerance.
    ///
    /// `join_tol_mm` must come from [`crate::reader::FileClass::join_tol_mm`].
    /// Passing a hand-picked number is how a wrong graph gets built quietly.
    #[must_use]
    pub fn build(model: &ModelEntry, join_tol_mm: f64, file_class_label: &'static str) -> Self {
        let n = model.records.len();
        let mut points: Vec<ConnectivePoint> = Vec::with_capacity(n * 2);
        let mut point_index = Vec::with_capacity(n);
        for (ri, rec) in model.records.iter().enumerate() {
            let lo = points.len();
            match rec.kind {
                RecordKind::ContactSphere | RecordKind::JointSphere => {
                    points.push(ConnectivePoint {
                        record: ri,
                        slot: 0,
                        role: PointRole::BallCentre,
                        pos: v::midpoint(rec.start_mm, rec.end_mm),
                        radius_mm: rec.r_start,
                    });
                }
                RecordKind::ContactTaper | RecordKind::Member | RecordKind::RootFlare => {
                    points.push(ConnectivePoint {
                        record: ri,
                        slot: 0,
                        role: PointRole::FrustumStart,
                        pos: rec.start_mm,
                        radius_mm: rec.r_start,
                    });
                    points.push(ConnectivePoint {
                        record: ri,
                        slot: 1,
                        role: PointRole::FrustumEnd,
                        pos: rec.end_mm,
                        radius_mm: rec.r_end,
                    });
                }
                RecordKind::RaftPadOrBridge => {
                    if rec.is_bridge() == Some(true) {
                        points.push(ConnectivePoint {
                            record: ri,
                            slot: 0,
                            role: PointRole::BridgeStart,
                            pos: rec.start_mm,
                            radius_mm: rec.r_start * 0.5,
                        });
                        points.push(ConnectivePoint {
                            record: ri,
                            slot: 1,
                            role: PointRole::BridgeEnd,
                            pos: rec.end_mm,
                            radius_mm: rec.r_start * 0.5,
                        });
                    } else {
                        points.push(ConnectivePoint {
                            record: ri,
                            slot: 0,
                            role: PointRole::PadCentre,
                            pos: rec.start_mm,
                            radius_mm: rec.r_start * 0.5,
                        });
                    }
                }
            }
            point_index.push((lo, points.len()));
        }

        // Spatial hash on the tolerance so pairing is O(points), not O(n^2).
        // A per-pair loop over 2585 records would be 3.3e6 distance calls per
        // model; the bucket form is a few times the point count.
        // The cell is sized on the NEAR-BAND radius, not on the join tolerance:
        // a +/-1 cell neighbourhood must be able to see every pair the
        // near-band diagnostic reports, not merely every pair that joins.
        let near_band = join_tol_mm * 10.0;
        let cell = near_band.max(1e-9);
        let key = |p: V3| -> [i64; 3] {
            [
                (p[0] / cell).floor() as i64,
                (p[1] / cell).floor() as i64,
                (p[2] / cell).floor() as i64,
            ]
        };
        let mut buckets: std::collections::HashMap<[i64; 3], Vec<usize>> =
            std::collections::HashMap::new();
        for (i, p) in points.iter().enumerate() {
            buckets.entry(key(p.pos)).or_default().push(i);
        }

        let mut joins: Vec<Join> = Vec::new();
        let mut ambiguities: Vec<Ambiguity> = Vec::new();
        let mut partner_count = vec![0usize; points.len()];
        let mut candidates_of: Vec<Vec<(usize, f64)>> = vec![Vec::new(); points.len()];
        let mut near_pairs: BTreeSet<(usize, usize)> = BTreeSet::new();

        for (i, pi) in points.iter().enumerate() {
            let k = key(pi.pos);
            for dz in -1..=1i64 {
                for dy in -1..=1i64 {
                    for dx in -1..=1i64 {
                        let Some(list) = buckets.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) else {
                            continue;
                        };
                        for &j in list {
                            if j <= i {
                                continue;
                            }
                            let pj = &points[j];
                            if pj.record == pi.record {
                                // A record never joins itself; its own two ends
                                // are the same solid.
                                continue;
                            }
                            let d = v::dist(pi.pos, pj.pos);
                            if d <= join_tol_mm {
                                joins.push(Join {
                                    a: (pi.record, pi.slot),
                                    b: (pj.record, pj.slot),
                                    distance_mm: d,
                                });
                                partner_count[i] += 1;
                                partner_count[j] += 1;
                                candidates_of[i].push((j, d));
                                candidates_of[j].push((i, d));
                            } else if d <= near_band {
                                near_pairs.insert((i, j));
                            }
                        }
                    }
                }
            }
        }

        // The contract's empty band says NearBand should be empty on emitted
        // bytes. If it is not, the tolerance decided the answer and that is a
        // finding, not a detail.
        for (i, j) in near_pairs {
            ambiguities.push(Ambiguity::NearBand {
                a: (points[i].record, points[i].slot),
                b: (points[j].record, points[j].slot),
                distance_mm: v::dist(points[i].pos, points[j].pos),
            });
        }
        for (i, cands) in candidates_of.iter().enumerate() {
            match cands.len() {
                0 => ambiguities.push(Ambiguity::Unjoined {
                    point: (points[i].record, points[i].slot),
                }),
                1 => {}
                _ => {
                    // Several partners is NOT ambiguity by itself: a junction
                    // ball legitimately gathers several arms, and every arm
                    // end at that junction is within tolerance of every other.
                    // It IS ambiguity when the partners do not agree with each
                    // other -- point i is within tolerance of j and of k while
                    // j and k are further apart than the tolerance. Then the
                    // clustering depends on the order the pairs were visited,
                    // and the graph has no single answer to represent.
                    let inconsistent = cands.iter().any(|&(j, _)| {
                        cands.iter().any(|&(k, _)| {
                            j != k && v::dist(points[j].pos, points[k].pos) > join_tol_mm
                        })
                    });
                    if inconsistent {
                        ambiguities.push(Ambiguity::MultipleCandidates {
                            point: (points[i].record, points[i].slot),
                            candidates: cands
                                .iter()
                                .map(|&(j, d)| ((points[j].record, points[j].slot), d))
                                .collect(),
                        });
                    }
                }
            }
        }

        // Record adjacency and components.
        let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); n];
        for j in &joins {
            neighbours[j.a.0].push(j.b.0);
            neighbours[j.b.0].push(j.a.0);
        }
        for list in &mut neighbours {
            list.sort_unstable();
            list.dedup();
        }
        let mut component_of_record = vec![usize::MAX; n];
        let mut component_count = 0usize;
        for s in 0..n {
            if component_of_record[s] != usize::MAX {
                continue;
            }
            let id = component_count;
            component_count += 1;
            let mut stack = vec![s];
            component_of_record[s] = id;
            while let Some(x) = stack.pop() {
                for &y in &neighbours[x] {
                    if component_of_record[y] == usize::MAX {
                        component_of_record[y] = id;
                        stack.push(y);
                    }
                }
            }
        }

        let mut g = Self {
            points,
            point_index,
            joins,
            neighbours,
            ambiguities,
            join_tol_mm,
            file_class_label,
            component_of_record,
            component_count,
        };
        g.audit_tip_tapers(model);
        g
    }

    fn audit_tip_tapers(&mut self, model: &ModelEntry) {
        for (ri, rec) in model.records.iter().enumerate() {
            if rec.kind != RecordKind::ContactSphere {
                continue;
            }
            let cands = self.taper_candidates(model, ri);
            if cands.len() != 1 {
                self.ambiguities.push(Ambiguity::TipTaperNotUnique {
                    tip_record: ri,
                    candidates: cands,
                });
            }
        }
    }

    fn taper_candidates(&self, model: &ModelEntry, tip_record: usize) -> Vec<usize> {
        self.neighbours[tip_record]
            .iter()
            .copied()
            .filter(|&r| model.records[r].kind == RecordKind::ContactTaper)
            .collect()
    }

    /// The type-1 TAPER adjacent to a type-7 TIP. `SEAT_DEPTH_*` reads the
    /// declared contact depth from this record's trailing slot 0, so a
    /// non-[`Link::Unique`] answer makes that comparison indeterminate.
    #[must_use]
    pub fn taper_for_tip(&self, model: &ModelEntry, tip_record: usize) -> Link {
        let mut c = self.taper_candidates(model, tip_record);
        c.sort_unstable();
        match c.len() {
            0 => Link::Missing,
            1 => Link::Unique(c[0]),
            _ => Link::Ambiguous(c),
        }
    }

    /// Records joined to `record`.
    #[must_use]
    pub fn neighbours_of(&self, record: usize) -> &[usize] {
        &self.neighbours[record]
    }

    /// The connective points of `record`.
    #[must_use]
    pub fn points_of(&self, record: usize) -> &[ConnectivePoint] {
        let (lo, hi) = self.point_index[record];
        &self.points[lo..hi]
    }

    /// Is this point joined to anything?
    #[must_use]
    pub fn is_joined(&self, point: PointRef) -> bool {
        self.joins.iter().any(|j| j.a == point || j.b == point)
    }

    /// Graph distance in edges from each record to the nearest grounded
    /// record, or `None` for a record with no route to ground at all.
    ///
    /// `is_ground` is supplied by the caller because grounding needs the model
    /// mesh and the seat verdicts, which this module deliberately does not
    /// know about (`audit-invariants.md` §3.5 clause ii). Breadth-first, so
    /// the result is the honest "graph distance to ground" clause (c) needs.
    #[must_use]
    pub fn distance_to_ground(&self, is_ground: &dyn Fn(usize) -> bool) -> Vec<Option<u32>> {
        let n = self.neighbours.len();
        let mut dist: Vec<Option<u32>> = vec![None; n];
        let mut queue = std::collections::VecDeque::new();
        for r in 0..n {
            if is_ground(r) {
                dist[r] = Some(0);
                queue.push_back(r);
            }
        }
        while let Some(x) = queue.pop_front() {
            let d = dist[x].expect("queued records carry a distance");
            for &y in &self.neighbours[x] {
                if dist[y].is_none() {
                    dist[y] = Some(d + 1);
                    queue.push_back(y);
                }
            }
        }
        dist
    }

    /// Which records still reach ground when `removed` is deleted.
    ///
    /// This is the primitive behind I8's R2 role ("removing it disconnects at
    /// least one live TIP from ground"). It is a **single-removal** test; a
    /// pair of members each redundant alone but jointly load-bearing passes,
    /// and `audit-invariants.md` §3.8 names that exposure rather than closing
    /// it.
    #[must_use]
    pub fn ground_reachable_without(
        &self,
        removed: usize,
        is_ground: &dyn Fn(usize) -> bool,
    ) -> Vec<bool> {
        let n = self.neighbours.len();
        let mut seen = vec![false; n];
        let mut stack = Vec::new();
        for r in 0..n {
            if r != removed && is_ground(r) {
                seen[r] = true;
                stack.push(r);
            }
        }
        while let Some(x) = stack.pop() {
            for &y in &self.neighbours[x] {
                if y != removed && !seen[y] {
                    seen[y] = true;
                    stack.push(y);
                }
            }
        }
        seen
    }

    /// Ambiguities touching `record`, so a per-record check can find its own
    /// without scanning.
    #[must_use]
    pub fn ambiguities_for(&self, record: usize) -> Vec<&Ambiguity> {
        self.ambiguities
            .iter()
            .filter(|a| match a {
                Ambiguity::MultipleCandidates { point, .. } | Ambiguity::Unjoined { point } => {
                    point.0 == record
                }
                Ambiguity::NearBand { a, b, .. } => a.0 == record || b.0 == record,
                Ambiguity::TipTaperNotUnique { tip_record, .. } => *tip_record == record,
            })
            .collect()
    }

    #[must_use]
    pub fn ambiguity_count(&self) -> usize {
        self.ambiguities.len()
    }
    /// Ambiguities excluding bare unjoined points, which are ordinary on
    /// grounded ends and are reported for `PATH_FREE_END` to judge.
    #[must_use]
    pub fn structural_ambiguity_count(&self) -> usize {
        self.ambiguities
            .iter()
            .filter(|a| !matches!(a, Ambiguity::Unjoined { .. }))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts as c;
    use crate::reader::{Soup, SupportRecord};

    fn rec(index: usize, kind: RecordKind, s: V3, e: V3, r1: f64, r2: f64) -> SupportRecord {
        SupportRecord {
            index,
            offset: 0,
            magic: c::SEG_MAGIC,
            raw_type: kind.wire(),
            kind,
            start_mm: s,
            end_mm: e,
            r_start: r1,
            r_end: r2,
            mesh_off: 0,
            mesh_len: 0,
            trailing: [0.0; 6],
            trailing_raw: [0u8; 24],
            soup: Soup::default(),
        }
    }

    fn model(records: Vec<SupportRecord>) -> ModelEntry {
        ModelEntry {
            index: 0,
            offset: 0,
            name: "t".into(),
            name_raw: b"t".to_vec(),
            translate_mm: [0.0, 0.0, 5.0],
            support_index_ptr: 1,
            mesh_off: 0,
            mesh_size: 0,
            model_soup: Soup::default(),
            support_index: None,
            records,
        }
    }

    /// tip ball -> taper -> member -> flare -> pad, a minimal load path.
    fn chain() -> ModelEntry {
        // Ball centre is midpoint(start, end), so poles straddle z = 0.
        let tip = rec(
            0,
            RecordKind::ContactSphere,
            [0.0, 0.0, -0.3],
            [0.0, 0.0, 0.3],
            0.3,
            0.3,
        );
        let taper = rec(
            1,
            RecordKind::ContactTaper,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            0.3,
            0.35,
        );
        let member = rec(
            2,
            RecordKind::Member,
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -4.0],
            0.35,
            0.35,
        );
        let flare = rec(
            3,
            RecordKind::RootFlare,
            [0.0, 0.0, -4.0],
            [0.0, 0.0, -4.8],
            0.35,
            1.1,
        );
        let mut pad = rec(
            4,
            RecordKind::RaftPadOrBridge,
            [0.0, 0.0, -4.8],
            [0.0, 0.0, -4.8],
            2.2,
            0.2,
        );
        pad.trailing[1] = 4.0;
        model(vec![tip, taper, member, flare, pad])
    }

    #[test]
    fn the_ball_connects_at_its_centre_not_at_its_poles() {
        let m = chain();
        let g = SupportGraph::build(&m, c::JOIN_TOL_EMITTED_MM, "emitted");
        let pts = g.points_of(0);
        assert_eq!(pts.len(), 1, "a ball contributes ONE point");
        assert_eq!(pts[0].role, PointRole::BallCentre);
        assert!(v::dist(pts[0].pos, [0.0, 0.0, 0.0]) < 1e-15);
        // The taper starts at the ball's centre, so they join.
        assert_eq!(g.neighbours_of(0), &[1]);
    }

    #[test]
    fn the_chain_is_one_component_and_its_taper_is_unique() {
        let m = chain();
        let g = SupportGraph::build(&m, c::JOIN_TOL_EMITTED_MM, "emitted");
        assert_eq!(g.component_count, 1, "one connected support");
        assert_eq!(g.taper_for_tip(&m, 0), Link::Unique(1));
        assert!(g.taper_for_tip(&m, 0).is_unique());
    }

    #[test]
    fn a_pad_contributes_one_point_and_a_bridge_two() {
        let m = chain();
        let g = SupportGraph::build(&m, c::JOIN_TOL_EMITTED_MM, "emitted");
        assert_eq!(g.points_of(4).len(), 1);
        assert_eq!(g.points_of(4)[0].role, PointRole::PadCentre);

        let mut records = m.records.clone();
        records[4] = rec(
            4,
            RecordKind::RaftPadOrBridge,
            [0.0, 0.0, -4.8],
            [3.0, 0.0, -4.8],
            2.2,
            0.2,
        );
        records[4].trailing[0] = 1.0; // bridge flag
        let m2 = model(records);
        let g2 = SupportGraph::build(&m2, c::JOIN_TOL_EMITTED_MM, "emitted");
        assert_eq!(g2.points_of(4).len(), 2);
    }

    #[test]
    fn ground_distance_and_single_removal_do_what_i5_and_i8_need() {
        let m = chain();
        let g = SupportGraph::build(&m, c::JOIN_TOL_EMITTED_MM, "emitted");
        let is_ground = |r: usize| r == 4;
        let d = g.distance_to_ground(&is_ground);
        assert_eq!(d, vec![Some(4), Some(3), Some(2), Some(1), Some(0)]);
        // Cutting the member strands the tip and the taper.
        let reach = g.ground_reachable_without(2, &is_ground);
        assert_eq!(reach, vec![false, false, false, true, true]);
    }

    #[test]
    fn a_pair_just_outside_the_tolerance_is_reported_as_near_band() {
        let mut records = chain().records;
        // Push the member's start 5e-3 mm off the taper's end: outside the
        // 1e-3 emitted tolerance, inside 10x it.
        records[2].start_mm = [0.0, 0.0, -1.005];
        let m = model(records);
        let g = SupportGraph::build(&m, c::JOIN_TOL_EMITTED_MM, "emitted");
        assert!(
            g.ambiguities
                .iter()
                .any(|a| matches!(a, Ambiguity::NearBand { .. })),
            "the tolerance decided this answer and must say so"
        );
        assert_eq!(
            g.component_count, 2,
            "the chain is broken at exactly one join"
        );
    }

    #[test]
    fn chained_coincidence_is_ambiguous_but_a_junction_ball_is_not() {
        // A CHAIN of near-coincidences: A joins B and B joins C, while A and C
        // are further apart than the tolerance. Which cluster B belongs to
        // depends on visit order, so the graph has no single answer and must
        // say so instead of picking one.
        let mut records = chain().records;
        records[2].start_mm = [0.0, 0.0, -1.000_9];
        records.push(rec(
            5,
            RecordKind::ContactTaper,
            [0.0, 0.0, -1.001_8],
            [1.0, 0.0, -0.5],
            0.3,
            0.35,
        ));
        let m = model(records);
        let g = SupportGraph::build(&m, c::JOIN_TOL_EMITTED_MM, "emitted");
        assert!(
            g.ambiguities
                .iter()
                .any(|a| matches!(a, Ambiguity::MultipleCandidates { .. })),
            "order-dependent clustering must be represented, never resolved"
        );

        // A junction ball gathering three arms is normal, not ambiguous.
        let ball = rec(
            0,
            RecordKind::JointSphere,
            [0.0, 0.0, -0.35],
            [0.0, 0.0, 0.35],
            0.35,
            0.35,
        );
        let arms: Vec<SupportRecord> = (1..4)
            .map(|i| {
                rec(
                    i,
                    RecordKind::Member,
                    [0.0, 0.0, 0.0],
                    [f64::from(i as i32), 0.0, -2.0],
                    0.35,
                    0.35,
                )
            })
            .collect();
        let mut recs = vec![ball];
        recs.extend(arms);
        let g2 = SupportGraph::build(&model(recs), c::JOIN_TOL_EMITTED_MM, "emitted");
        assert!(
            !g2.ambiguities
                .iter()
                .any(|a| matches!(a, Ambiguity::MultipleCandidates { .. })),
            "a junction ball legitimately gathers several arms"
        );
        assert_eq!(g2.neighbours_of(0).len(), 3);
    }

    #[test]
    fn a_tip_with_no_taper_is_recorded_as_a_finding() {
        let records = vec![rec(
            0,
            RecordKind::ContactSphere,
            [0.0, 0.0, -0.3],
            [0.0, 0.0, 0.3],
            0.3,
            0.3,
        )];
        let m = model(records);
        let g = SupportGraph::build(&m, c::JOIN_TOL_EMITTED_MM, "emitted");
        assert_eq!(g.taper_for_tip(&m, 0), Link::Missing);
        assert_eq!(g.taper_for_tip(&m, 0).reason(), "no_adjacent_taper");
        assert!(g
            .ambiguities
            .iter()
            .any(|a| matches!(a, Ambiguity::TipTaperNotUnique { .. })));
    }
}
