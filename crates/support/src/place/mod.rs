//! Private, bounded placement search. No admitted contact is moved or enlarged.
//! Exhausted samples are not a proof that no legal anchor exists.

mod layout;
mod search;
#[cfg(test)]
mod tests;

use crate::construct::{ContactCandidate, Contacts, PlanBuilder};
use crate::load::needs::{self, NeedsMap, RestPose, SiteTag};
use crate::load::{self, LayerStack, LayoutView, LoadReport, ModelLayers};
use plinth_geom::Bvh;

pub use layout::{add_members, layout_from_contacts};

/// Largest modelled load through any fixed section of each chain, across ALL
/// layers. ContactLoad::load_mm2 is only the load at the worst RATIO (often an
/// early partial patch), and must not size columns for the later maximum load.
#[must_use]
pub fn routing_loads(detail: &load::LoadDetail) -> Vec<f64> {
    let mut loads = vec![0.0_f64; detail.contacts.len()];
    for s in &detail.sections {
        if s.kind != load::PartKind::Patch
            && let Some(load) = loads.get_mut(s.chain as usize)
        {
            *load = load.max(s.tributary.0 * s.area_mm2);
        }
    }
    loads
}

/// Search settings **[S]**, not spacing laws or physical guarantees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanSettings {
    /// Maximum private rounds.
    pub max_rounds: u32,
    /// Maximum distance to an earlier contact, mm.
    pub coverage_reach_mm: f64,
    /// Inner and outer radii for load additions, mm.
    pub ring_mm: [f64; 2],
}
impl Default for PlanSettings {
    fn default() -> Self {
        Self {
            max_rounds: 8,
            coverage_reach_mm: 3.0,
            ring_mm: [1.0, 2.5],
        }
    }
}
/// How the bounded search ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    /// All modelled constraints are satisfied (not checker acceptance).
    Satisfied,
    /// Samples or rounds exhausted; does not prove impossibility.
    SearchBudgetExhausted,
    /// The final exact route replay failed; not a search-completeness claim.
    RouteFailed,
    /// Search stopped early but the complete final replay still has a load or
    /// anchor miss. The search budget was not exhausted.
    FinalReplayUnsatisfied,
    /// Reserved for a complete geometric impossibility proof, never sampling.
    ProvedNoLegalAnchor,
    /// Invalid or unavailable inputs prevented planning.
    NotBuilt,
}
/// Private proposal. The caller must probe and atomically commit it before routing.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// Search outcome, not print eligibility.
    pub termination: Termination,
    /// Exact fixed-size candidates, stable order.
    pub candidates: Vec<ContactCandidate>,
    /// Full member/brace/raft preview succeeded before any contact commit.
    pub routed: bool,
    /// Exact remaining routing blocker, if any; not a checker measurement.
    pub routing_blocker: Option<String>,
    /// Last modelled load result.
    pub load: LoadReport,
    /// The matching load view, in the plate frame.
    pub layout: LayoutView,
    /// Classified sub-45 degree needs, including the visible rim band.
    pub needs: NeedsMap,
    /// Admission rejections counted by reason.
    pub rejections: Vec<(String, u64)>,
}

/// Build a classified site pool, seed first-layer islands, cover overhang and reach,
/// then size and add contacts in private batches. Uses P5 = 12 **[O]**.
#[must_use]
pub fn plan_contacts(
    builder: &PlanBuilder<Contacts>,
    layers: &ModelLayers,
    settings: &PlanSettings,
) -> Proposal {
    let mesh = builder.model().mesh();
    let mut sites = needs::overhang_points(mesh, 89.9, 0.25);
    // First-layer anchors need samples at the actual lower edge, not only at
    // lattice centres. Stay slightly inside each face to retain its normal.
    let mut edge_sites = Vec::new();
    for t in 0..mesh.triangle_count() {
        let [a, b, c] = mesh.triangle_corners(t);
        let u: [f64; 3] = std::array::from_fn(|k| b[k] - a[k]);
        let v: [f64; 3] = std::array::from_fn(|k| c[k] - a[k]);
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let len = n.iter().map(|x| x * x).sum::<f64>().sqrt();
        if len <= 0. || n[2] >= -0.001 * len {
            continue;
        }
        for vertex in [a, b, c] {
            edge_sites.push(needs::NeedPoint {
                xyz: std::array::from_fn(|k| {
                    vertex[k] * 0.99998 + (a[k] + b[k] + c[k]) / 3. * 0.00002
                }),
                normal: n.map(|x| x / len),
                angle_deg: (-n[2] / len).acos().to_degrees(),
                area_mm2: 0.,
                triangle: t as u32,
                class: SiteTag::Unverified("not classified"),
            });
        }
    }
    // Edge samples are search alternatives only, not additional measured needs area.
    let needs_count = sites.points.len();
    sites.points.extend(edge_sites);
    if let Ok(bvh) = Bvh::build(mesh) {
        // The approved underside points along rotated +Z (design section 0).
        // Resting UP points the opposite way, toward the decorated top. Using
        // the underside normal here would reverse the visibility classes.
        let a = builder.model().keep_up().rotation_x_deg.to_radians();
        needs::classify_sites(
            &mut sites.points,
            &RestPose {
                up: [0., a.sin(), -a.cos()],
            },
            &|rays, out| plinth_geom::ray::first_hits(&bvh, rays, out),
        );
    }
    let mut needs = NeedsMap {
        points: sites.points[..needs_count]
            .iter()
            .copied()
            .filter(|p| p.angle_deg < 45.)
            .collect(),
        limit_deg: 45.,
        spacing_mm: 0.25,
    };
    sites
        .points
        .retain(|p| matches!(p.class, SiteTag::Hidden | SiteTag::PartlyVisible));
    let shift = -f64::from(builder.model().plate_z());
    // Reports and pictures have one frame: plate coordinates, never native local z.
    for p in &mut needs.points {
        p.xyz[2] += shift;
    }
    let mut result = search::run(builder, layers, &sites.points, &needs, settings, shift);
    result.needs = needs;
    result
}

/// An absent layer stack is explicitly unverified, never an empty successful plan.
fn unavailable(stack: &LayerStack) -> bool {
    stack.layers.is_empty() || !stack.layer_height_mm.is_finite() || stack.layer_height_mm <= 0.
}
