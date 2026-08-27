//! Frozen bottom-connected raft planning over live forest members.
//!
//! This module stops at semantic primitives.  `plinth-emit` remains the only
//! owner of mesh and ChituBox record construction.

use plinth_core::{m0_constants, ContentDigest, Finite, PositiveFinite};

use crate::{
    forest::{Forest, MemberKind, Owner},
    joints::FilteredLiveView,
    FrustumRingNormal, SupportError, SupportPrimitive, SupportSegmentKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaftLimits {
    max_candidates: usize,
    max_roots: usize,
    max_primitives: usize,
    max_bytes: usize,
}

impl RaftLimits {
    pub(crate) const fn new(
        max_candidates: usize,
        max_roots: usize,
        max_primitives: usize,
        max_bytes: usize,
    ) -> Self {
        Self {
            max_candidates,
            max_roots,
            max_primitives,
            max_bytes,
        }
    }

    #[must_use]
    pub const fn try_new(
        max_candidates: usize,
        max_roots: usize,
        max_primitives: usize,
        max_bytes: usize,
    ) -> Option<Self> {
        if max_candidates == 0 || max_roots == 0 || max_primitives == 0 || max_bytes == 0 {
            return None;
        }
        Some(Self {
            max_candidates,
            max_roots,
            max_primitives,
            max_bytes,
        })
    }

    pub(crate) const fn is_valid(self) -> bool {
        self.max_candidates != 0
            && self.max_roots != 0
            && self.max_primitives != 0
            && self.max_bytes != 0
    }

    pub(crate) const fn authority_fingerprint(self) -> [usize; 4] {
        [
            self.max_candidates,
            self.max_roots,
            self.max_primitives,
            self.max_bytes,
        ]
    }
}

/// A canonical plate-root location and its rootward column radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlateRoot {
    xy_mm: [Finite; 2],
    root_radius_mm: PositiveFinite,
}

impl PlateRoot {
    #[must_use]
    pub const fn xy_mm(self) -> [Finite; 2] {
        self.xy_mm
    }

    #[must_use]
    pub const fn root_radius_mm(self) -> PositiveFinite {
        self.root_radius_mm
    }
}

/// Deterministic semantic output of the current bottom-connected raft pass.
#[derive(Clone, Debug, PartialEq)]
pub struct RaftPlan {
    forest_digest: u64,
    live_view_digest: u64,
    plate_z_bits: u64,
    roots: Vec<PlateRoot>,
    primitives: Vec<SupportPrimitive>,
}

impl RaftPlan {
    pub(crate) fn validate(
        &self,
        forest: &Forest,
        view: &FilteredLiveView,
        plate_z_mm: Finite,
    ) -> Result<(), SupportError> {
        view.validate(forest)
            .map_err(|_| SupportError::StaleSupportAuthority)?;
        if self.forest_digest != forest.digest()
            || self.live_view_digest != view.digest()
            || self.plate_z_bits != plate_z_mm.get().to_bits()
        {
            return Err(SupportError::StaleSupportAuthority);
        }
        Ok(())
    }

    #[must_use]
    pub fn roots(&self) -> &[PlateRoot] {
        &self.roots
    }

    #[must_use]
    pub fn primitives(&self) -> &[SupportPrimitive] {
        &self.primitives
    }

    /// Stable digest over the canonical semantic output, for parity fixtures.
    #[must_use]
    pub fn digest(&self) -> ContentDigest {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut state = OFFSET;
        let mut write = |value: f64| {
            for byte in value.to_bits().to_le_bytes() {
                state ^= u64::from(byte);
                state = state.wrapping_mul(PRIME);
            }
        };
        write(f64::from_bits(self.forest_digest));
        write(f64::from_bits(self.live_view_digest));
        write(f64::from_bits(self.plate_z_bits));
        write(self.roots.len() as f64);
        for root in &self.roots {
            write(root.xy_mm[0].get());
            write(root.xy_mm[1].get());
            write(root.root_radius_mm.get());
        }
        for primitive in &self.primitives {
            write(primitive.segment_kind as u8 as f64);
            for value in primitive.start_mm {
                write(value.get());
            }
            for value in primitive.end_mm {
                write(value.get());
            }
            write(primitive.start_radius_mm.get());
            write(primitive.end_radius_mm.get());
        }
        ContentDigest(state)
    }
}

/// Plans the frozen semantic raft foundation from live forest members whose
/// rootward endpoint falls in the registered plate band.
///
/// Python pad/bridge parity is separate from layer-height- and
/// clearance-aware per-member root bosses. A future controller or emitter
/// must not treat deduplicated [`SupportSegmentKind::RootFlare`] primitives as
/// a complete `member_chain_specs` certificate.
///
/// Candidate roots sort by exact `(x, y)` before the frozen tolerance dedup;
/// consequently a tolerance cluster is represented by its lexically first
/// point.  Bridges appear before pads, matching the current native order.
pub(crate) fn plan_bottom_connected(
    forest: &Forest,
    view: &FilteredLiveView,
    plate_z_mm: Finite,
    limits: RaftLimits,
) -> Result<RaftPlan, SupportError> {
    view.validate(forest)
        .map_err(|_| SupportError::StaleSupportAuthority)?;
    if !limits.is_valid() {
        return Err(SupportError::InvalidResourceLimits);
    }
    let constants = m0_constants().map_err(|_| SupportError::NoLegalSupportPlan)?;
    let policy = constants.placeholders();
    let plate_band = policy.plate_band_mm().diagnostic().get();
    let tolerance = policy.chitubox_plate_mesh_tol_mm().diagnostic().get();
    let pad_diameter = *policy.chitubox_pad_diameter_mm().diagnostic();
    let raft_height = *policy.chitubox_raft_height_mm().diagnostic();

    let candidate_count = view
        .keys()
        .iter()
        .copied()
        .filter_map(|key| forest.member(key).map(|member| (key, member)))
        .filter(|(key, member)| match (member.kind(), member.owner()) {
            (MemberKind::Stub, Owner::Contact(contact)) => forest.terminal(*contact) == Some(*key),
            (MemberKind::Stub, _) => false,
            _ => true,
        })
        .filter(|(_, member)| {
            let point = member.p1_mm();
            (point[2].get() - plate_z_mm.get()).abs() <= plate_band
        })
        .count();
    if candidate_count > limits.max_candidates {
        return Err(SupportError::SupportInputLimit);
    }
    let candidate_bytes = candidate_count
        .checked_mul(std::mem::size_of::<PlateRoot>())
        .ok_or(SupportError::SupportScratchLimit)?;
    if candidate_bytes > limits.max_bytes {
        return Err(SupportError::SupportScratchLimit);
    }

    let mut candidates = Vec::with_capacity(candidate_count);
    for key in view.keys().iter().copied() {
        let member = forest
            .member(key)
            .ok_or(SupportError::StaleSupportAuthority)?;
        let eligible = match (member.kind(), member.owner()) {
            (MemberKind::Stub, Owner::Contact(contact)) => forest.terminal(*contact) == Some(key),
            (MemberKind::Stub, _) => false,
            _ => true,
        };
        if eligible {
            let point = member.p1_mm();
            if (point[2].get() - plate_z_mm.get()).abs() <= plate_band {
                candidates.push(PlateRoot {
                    xy_mm: [point[0], point[1]],
                    root_radius_mm: member.rootward_radius_mm().get(),
                });
            }
        }
    }
    candidates.sort_unstable_by(|a, b| {
        a.xy_mm[0]
            .get()
            .total_cmp(&b.xy_mm[0].get())
            .then_with(|| a.xy_mm[1].get().total_cmp(&b.xy_mm[1].get()))
            .then_with(|| a.root_radius_mm.get().total_cmp(&b.root_radius_mm.get()))
    });

    let mut accepted = 0;
    for read in 0..candidates.len() {
        let candidate = candidates[read];
        let duplicate = candidates[..accepted].iter().any(|prior| {
            let dx = candidate.xy_mm[0].get() - prior.xy_mm[0].get();
            let dy = candidate.xy_mm[1].get() - prior.xy_mm[1].get();
            (dx * dx + dy * dy).sqrt() <= tolerance
        });
        if !duplicate {
            candidates.swap(accepted, read);
            accepted += 1;
        }
    }
    candidates.truncate(accepted);
    let roots = candidates;
    if roots.len() > limits.max_roots {
        return Err(SupportError::SupportInputLimit);
    }
    let primitive_count = roots.len().saturating_mul(2).saturating_sub(1);
    if primitive_count > limits.max_primitives {
        return Err(SupportError::SupportPrimitiveLimit);
    }
    let primitive_bytes = primitive_count
        .checked_mul(std::mem::size_of::<SupportPrimitive>())
        .ok_or(SupportError::SupportScratchLimit)?;
    let visited_bytes = roots
        .len()
        .checked_mul(std::mem::size_of::<u8>())
        .ok_or(SupportError::SupportScratchLimit)?;
    let edge_bytes = roots
        .len()
        .saturating_sub(1)
        .checked_mul(std::mem::size_of::<(usize, usize)>())
        .ok_or(SupportError::SupportScratchLimit)?;
    let scratch = candidate_bytes
        .checked_add(primitive_bytes)
        .and_then(|bytes| bytes.checked_add(visited_bytes))
        .and_then(|bytes| bytes.checked_add(edge_bytes))
        .ok_or(SupportError::SupportScratchLimit)?;
    if scratch > limits.max_bytes {
        return Err(SupportError::SupportScratchLimit);
    }

    let mut primitives = Vec::with_capacity(primitive_count);
    for (i, j) in euclidean_mst_edges(&roots) {
        primitives.push(raft_primitive(
            roots[i].xy_mm,
            roots[j].xy_mm,
            plate_z_mm,
            pad_diameter,
            raft_height,
        ));
    }
    for root in &roots {
        primitives.push(raft_primitive(
            root.xy_mm,
            root.xy_mm,
            plate_z_mm,
            pad_diameter,
            raft_height,
        ));
    }
    if primitives
        .iter()
        .any(|primitive| !primitive.is_semantically_valid())
    {
        return Err(SupportError::InvalidPrimitive);
    }
    Ok(RaftPlan {
        forest_digest: forest.digest(),
        live_view_digest: view.digest(),
        plate_z_bits: plate_z_mm.get().to_bits(),
        roots,
        primitives,
    })
}

fn raft_primitive(
    start_xy: [Finite; 2],
    end_xy: [Finite; 2],
    plate_z_mm: Finite,
    pad_diameter: PositiveFinite,
    raft_height: PositiveFinite,
) -> SupportPrimitive {
    SupportPrimitive {
        segment_kind: SupportSegmentKind::RaftPadOrBridge,
        start_mm: [start_xy[0], start_xy[1], plate_z_mm],
        end_mm: [end_xy[0], end_xy[1], plate_z_mm],
        start_radius_mm: pad_diameter,
        end_radius_mm: raft_height,
        contact_depth_mm: None,
        ring_normal: FrustumRingNormal::SegmentAxis,
        sphere_geometry: None,
    }
}

fn euclidean_mst_edges(roots: &[PlateRoot]) -> Vec<(usize, usize)> {
    if roots.len() < 2 {
        return Vec::new();
    }
    let mut visited = vec![0_u8; roots.len()];
    visited[0] = 1;
    let mut edges = Vec::with_capacity(roots.len() - 1);
    while edges.len() + 1 < roots.len() {
        let mut best: Option<(f64, usize, usize, usize, usize)> = None;
        for i in 0..roots.len() {
            if visited[i] == 0 {
                continue;
            }
            for j in 0..roots.len() {
                if visited[j] != 0 {
                    continue;
                }
                let dx = roots[j].xy_mm[0].get() - roots[i].xy_mm[0].get();
                let dy = roots[j].xy_mm[1].get() - roots[i].xy_mm[1].get();
                let (lo, hi) = if i < j { (i, j) } else { (j, i) };
                let candidate = (dx * dx + dy * dy, lo, hi, i, j);
                if best.is_none_or(|current| {
                    candidate
                        .0
                        .total_cmp(&current.0)
                        .then(candidate.1.cmp(&current.1))
                        .then(candidate.2.cmp(&current.2))
                        .then(candidate.3.cmp(&current.3))
                        .then(candidate.4.cmp(&current.4))
                        == std::cmp::Ordering::Less
                }) {
                    best = Some(candidate);
                }
            }
        }
        let (_, lo, hi, _, new_vertex) = best.expect("an unvisited root has a connecting edge");
        edges.push((lo, hi));
        visited[new_vertex] = 1;
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forest::{
        Member, MemberKind, MemberSpec, Owner, RootwardEndRadius, TerminalEndRadius,
    };
    use crate::joints::{FilteredLiveView, LiveViewLimits};

    fn number(value: f64) -> Finite {
        Finite::new(value).expect("finite fixture")
    }

    fn radius(value: f64) -> PositiveFinite {
        PositiveFinite::new(value).expect("positive fixture")
    }

    fn member_for(point: [f64; 3], ordinal: u32) -> Member {
        Member::new(MemberSpec {
            p0_mm: [number(point[0]), number(point[1]), number(point[2] + 2.0)],
            p1_mm: point.map(number),
            r0_mm: TerminalEndRadius::new(radius(0.4)),
            r1_mm: RootwardEndRadius::new(radius(0.6 + point[0] * 0.01 + point[1] * 0.001)),
            kind: MemberKind::Trunk,
            owner: Owner::Generated {
                family: "raft-test".into(),
                ordinal,
            },
            model_contact_end: None,
            carried_mm2: number(1.0),
        })
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn forest_for(points: &[[f64; 3]]) -> Forest {
        let mut forest = Forest::new();
        for (ordinal, point) in points.iter().enumerate() {
            forest.add_member(member_for(*point, ordinal as u32));
        }
        forest
    }

    fn plan(points: &[[f64; 3]]) -> RaftPlan {
        let forest = forest_for(points);
        let view = FilteredLiveView::current(&forest, &[], LiveViewLimits::new(100, 100, 100_000))
            .unwrap();
        plan_bottom_connected(
            &forest,
            &view,
            number(0.0),
            RaftLimits::new(100, 100, 200, 100_000),
        )
        .expect("valid raft plan")
    }

    #[test]
    fn empty_single_and_multiple_roots_have_current_primitive_counts() {
        assert!(plan(&[]).primitives().is_empty());

        let one = plan(&[[3.0, -2.0, 0.0]]);
        assert_eq!(one.primitives().len(), 1, "one pad");
        assert_eq!(
            one.primitives()[0].segment_kind,
            SupportSegmentKind::RaftPadOrBridge
        );

        let four = plan(&[
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 2.0, 0.0],
            [2.0, 2.0, 0.0],
        ]);
        assert_eq!(four.primitives().len(), 7, "3 bridges + 4 pads");
        assert!(four
            .primitives()
            .iter()
            .all(SupportPrimitive::is_semantically_valid));
    }

    #[test]
    fn live_members_at_plate_include_braces() {
        let mut forest = forest_for(&[[0.0, 0.0, 0.0]]);
        forest.add_brace(member_for([2.0, 0.0, 0.0], 1));

        let view = FilteredLiveView::current(&forest, &[], LiveViewLimits::new(100, 100, 100_000))
            .unwrap();
        let raft = plan_bottom_connected(
            &forest,
            &view,
            number(0.0),
            RaftLimits::new(100, 100, 200, 100_000),
        )
        .expect("valid raft plan");
        assert_eq!(raft.roots().len(), 2);
        assert_eq!(raft.primitives().len(), 3, "one bridge and two pads");
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn frozen_sort_tolerance_dedup_and_prim_tie_order_match_python() {
        let raft = plan(&[
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.000_009, 0.0, 0.0], // CHITUBOX_PLATE_MESH_TOL_MM dedup
            [7.0, 7.0, 0.21],      // outside the PLATE_BAND_MM
        ]);
        assert_eq!(raft.roots().len(), 3);
        assert_eq!(euclidean_mst_edges(raft.roots()), vec![(0, 1), (0, 2)]);
        let bridges = raft
            .primitives()
            .iter()
            .take(2)
            .map(|p| (p.start_mm, p.end_mm))
            .collect::<Vec<_>>();
        assert_eq!(bridges[0].0.map(Finite::get), [0.0, 0.0, 0.0]);
        assert_eq!(bridges[0].1.map(Finite::get), [0.0, 1.0, 0.0]);
        assert_eq!(bridges[1].1.map(Finite::get), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn semantic_primitives_are_invariant_to_live_member_insertion_order() {
        for count in [1_usize, 4, 16] {
            let points = (0..count)
                .map(|index| [(index % 4) as f64, (index / 4) as f64, 0.0])
                .collect::<Vec<_>>();
            let mut reversed = points.clone();
            reversed.reverse();
            assert_eq!(plan(&points).primitives(), plan(&reversed).primitives());
        }
    }

    #[test]
    fn plan_authority_rejects_a_changed_forest_or_plate() {
        let mut forest = forest_for(&[[0.0, 0.0, 0.0]]);
        let view = FilteredLiveView::current(&forest, &[], LiveViewLimits::new(100, 100, 100_000))
            .unwrap();
        let plan = plan_bottom_connected(
            &forest,
            &view,
            number(0.0),
            RaftLimits::new(100, 100, 200, 100_000),
        )
        .unwrap();
        assert!(plan.validate(&forest, &view, number(0.0)).is_ok());
        forest.add_member(member_for([2.0, 0.0, 0.0], 2));
        assert!(plan.validate(&forest, &view, number(0.0)).is_err());
        let original = forest_for(&[[0.0, 0.0, 0.0]]);
        let original_view =
            FilteredLiveView::current(&original, &[], LiveViewLimits::new(100, 100, 100_000))
                .unwrap();
        assert!(plan
            .validate(&original, &original_view, number(0.1))
            .is_err());
    }

    #[test]
    fn caller_owned_candidate_root_primitive_and_byte_caps_are_exact() {
        let forest = forest_for(&[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);
        let view =
            FilteredLiveView::current(&forest, &[], LiveViewLimits::new(2, 1, 10_000)).unwrap();
        let exact_bytes = 2 * std::mem::size_of::<PlateRoot>()
            + 3 * std::mem::size_of::<SupportPrimitive>()
            + 2 * std::mem::size_of::<u8>()
            + std::mem::size_of::<(usize, usize)>();
        assert!(plan_bottom_connected(
            &forest,
            &view,
            number(0.0),
            RaftLimits::new(2, 2, 3, exact_bytes),
        )
        .is_ok());
        for limits in [
            RaftLimits::new(1, 2, 3, exact_bytes),
            RaftLimits::new(2, 1, 3, exact_bytes),
        ] {
            assert_eq!(
                plan_bottom_connected(&forest, &view, number(0.0), limits).unwrap_err(),
                SupportError::SupportInputLimit
            );
        }
        assert_eq!(
            plan_bottom_connected(
                &forest,
                &view,
                number(0.0),
                RaftLimits::new(2, 2, 2, exact_bytes),
            )
            .unwrap_err(),
            SupportError::SupportPrimitiveLimit
        );
        assert_eq!(
            plan_bottom_connected(
                &forest,
                &view,
                number(0.0),
                RaftLimits::new(2, 2, 3, exact_bytes - 1),
            )
            .unwrap_err(),
            SupportError::SupportScratchLimit
        );

        let dedup_forest = forest_for(&[[0.0, 0.0, 0.0], [0.000_009, 0.0, 0.0]]);
        let dedup_view =
            FilteredLiveView::current(&dedup_forest, &[], LiveViewLimits::new(2, 1, 10_000))
                .unwrap();
        let dedup_bytes = 2 * std::mem::size_of::<PlateRoot>()
            + std::mem::size_of::<SupportPrimitive>()
            + std::mem::size_of::<u8>();
        assert!(plan_bottom_connected(
            &dedup_forest,
            &dedup_view,
            number(0.0),
            RaftLimits::new(2, 1, 1, dedup_bytes),
        )
        .is_ok());
    }
}
