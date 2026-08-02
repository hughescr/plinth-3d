//! Frozen bottom-connected raft planning over the live support forest.
//!
//! This module stops at semantic primitives.  `plinth-emit` remains the only
//! owner of mesh and ChituBox record construction.

use plinth_core::{m0_constants, ContentDigest, Finite, PositiveFinite};

use crate::{
    forest::Forest, FrustumRingNormal, SupportError, SupportPrimitive, SupportSegmentKind,
};

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
    roots: Vec<PlateRoot>,
    primitives: Vec<SupportPrimitive>,
}

impl RaftPlan {
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

/// Plans the current Python-compatible raft from *live*, non-brace forest
/// members whose rootward endpoint falls in the registered plate band.
///
/// Candidate roots sort by exact `(x, y)` before the frozen tolerance dedup;
/// consequently a tolerance cluster is represented by its lexically first
/// point.  Bridges appear before pads, matching the current native order.
pub fn plan_bottom_connected(
    forest: &Forest,
    plate_z_mm: Finite,
) -> Result<RaftPlan, SupportError> {
    let constants = m0_constants().map_err(|_| SupportError::NoLegalSupportPlan)?;
    let policy = constants.placeholders();
    let plate_band = policy.plate_band_mm().diagnostic().get();
    let tolerance = policy.chitubox_plate_mesh_tol_mm().diagnostic().get();
    let pad_diameter = *policy.chitubox_pad_diameter_mm().diagnostic();
    let raft_height = *policy.chitubox_raft_height_mm().diagnostic();
    let flare_length = *policy.flare_len_mm().diagnostic();
    let flare_factor = policy.flare_r_factor().diagnostic().get();

    let mut candidates: Vec<PlateRoot> = forest
        .live_members()
        .into_iter()
        .filter_map(|key| forest.member(key))
        .filter_map(|member| {
            let point = member.p1_mm();
            ((point[2].get() - plate_z_mm.get()).abs() <= plate_band).then_some(PlateRoot {
                xy_mm: [point[0], point[1]],
                root_radius_mm: member.rootward_radius_mm().get(),
            })
        })
        .collect();
    candidates.sort_by(|a, b| {
        a.xy_mm[0]
            .get()
            .total_cmp(&b.xy_mm[0].get())
            .then_with(|| a.xy_mm[1].get().total_cmp(&b.xy_mm[1].get()))
            .then_with(|| a.root_radius_mm.get().total_cmp(&b.root_radius_mm.get()))
    });

    let mut roots = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let duplicate = roots.iter().any(|prior: &PlateRoot| {
            let dx = candidate.xy_mm[0].get() - prior.xy_mm[0].get();
            let dy = candidate.xy_mm[1].get() - prior.xy_mm[1].get();
            (dx * dx + dy * dy).sqrt() <= tolerance
        });
        if !duplicate {
            roots.push(candidate);
        }
    }

    let mut primitives = Vec::with_capacity(roots.len().saturating_mul(3).saturating_sub(1));
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
    for root in &roots {
        let bottom_z = finite(plate_z_mm.get() + raft_height.get())?;
        let top_z = finite(bottom_z.get() + flare_length.get())?;
        let flare_radius = PositiveFinite::new(root.root_radius_mm.get() * flare_factor)
            .map_err(|_| SupportError::InvalidPrimitive)?;
        primitives.push(SupportPrimitive {
            segment_kind: SupportSegmentKind::RootFlare,
            start_mm: [root.xy_mm[0], root.xy_mm[1], top_z],
            end_mm: [root.xy_mm[0], root.xy_mm[1], bottom_z],
            start_radius_mm: root.root_radius_mm,
            end_radius_mm: flare_radius,
            contact_depth_mm: None,
            ring_normal: FrustumRingNormal::SegmentAxis,
            sphere_geometry: None,
        });
    }
    if primitives
        .iter()
        .any(|primitive| !primitive.is_semantically_valid())
    {
        return Err(SupportError::InvalidPrimitive);
    }
    Ok(RaftPlan { roots, primitives })
}

fn finite(value: f64) -> Result<Finite, SupportError> {
    Finite::new(value).map_err(|_| SupportError::InvalidPrimitive)
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
    let mut visited = vec![false; roots.len()];
    visited[0] = true;
    let mut edges = Vec::with_capacity(roots.len() - 1);
    while edges.len() + 1 < roots.len() {
        let mut best: Option<(f64, usize, usize, usize, usize)> = None;
        for i in 0..roots.len() {
            if !visited[i] {
                continue;
            }
            for j in 0..roots.len() {
                if visited[j] {
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
        visited[new_vertex] = true;
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forest::{
        Member, MemberKind, MemberSpec, Owner, RootwardEndRadius, TerminalEndRadius,
    };

    fn number(value: f64) -> Finite {
        Finite::new(value).expect("finite fixture")
    }

    fn radius(value: f64) -> PositiveFinite {
        PositiveFinite::new(value).expect("positive fixture")
    }

    fn forest_for(points: &[[f64; 3]]) -> Forest {
        let mut forest = Forest::new();
        for (ordinal, point) in points.iter().enumerate() {
            forest.add_member(Member::new(MemberSpec {
                p0_mm: [number(point[0]), number(point[1]), number(point[2] + 2.0)],
                p1_mm: point.map(number),
                r0_mm: TerminalEndRadius::new(radius(0.4)),
                r1_mm: RootwardEndRadius::new(radius(0.6 + point[0] * 0.01 + point[1] * 0.001)),
                kind: MemberKind::Trunk,
                owner: Owner::Generated {
                    family: "raft-test".into(),
                    ordinal: ordinal as u32,
                },
                embed_end: None,
                carried_mm2: number(1.0),
            }));
        }
        forest
    }

    fn plan(points: &[[f64; 3]]) -> RaftPlan {
        plan_bottom_connected(&forest_for(points), number(0.0)).expect("valid raft plan")
    }

    #[test]
    fn empty_single_and_multiple_roots_have_current_primitive_counts() {
        assert!(plan(&[]).primitives().is_empty());

        let one = plan(&[[3.0, -2.0, 0.0]]);
        assert_eq!(one.primitives().len(), 2, "one pad plus one flare");
        assert_eq!(
            one.primitives()[0].segment_kind,
            SupportSegmentKind::RaftPadOrBridge
        );
        assert_eq!(
            one.primitives()[1].segment_kind,
            SupportSegmentKind::RootFlare
        );

        let four = plan(&[
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 2.0, 0.0],
            [2.0, 2.0, 0.0],
        ]);
        assert_eq!(four.primitives().len(), 11, "3 bridges + 4 pads + 4 flares");
        assert!(four
            .primitives()
            .iter()
            .all(SupportPrimitive::is_semantically_valid));
    }

    #[test]
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
    fn digest_is_invariant_to_live_member_insertion_order_for_1_4_and_16_roots() {
        for count in [1_usize, 4, 16] {
            let points = (0..count)
                .map(|index| [(index % 4) as f64, (index / 4) as f64, 0.0])
                .collect::<Vec<_>>();
            let mut reversed = points.clone();
            reversed.reverse();
            assert_eq!(
                plan(&points).digest(),
                plan(&reversed).digest(),
                "{count} roots"
            );
        }
    }
}
