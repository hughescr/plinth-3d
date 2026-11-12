//! Bounded, independent hollow measurements on emitted triangles, explicitly opt-in.
//! The declaration supplies applicability only, never planner geometry or inside labels.
//!
//! The first search is deliberately conservative: 0.10 mm surface cells are UNKNOWN, not
//! empty. A missing route even with those cells open is a failure. A route relying on them,
//! an unresolved boundary pocket, or a non-axial mouth is unverified. This is not FR-17a.

use crate::json::Json;
use crate::outcome::{MAX_LOCATED, Outcome};
use crate::scene::Scene;
use crate::status::Located;
use crate::tri::Tri;
use crate::v3::{Aabb, V, cross, dot, len, mul, sub};

mod bores;
mod grid;
#[cfg(test)]
mod tests;
use grid::{FREE, Grid, MATERIAL, UNKNOWN};

pub const CELL_MM: f64 = 0.10;

#[derive(Debug, Clone)]
pub struct Bore {
    pub centre: V,
    pub axis: usize,
    pub span: [f64; 2],
    pub mouth: V,
    pub radius: f64,
    pub chamber: usize,
}

#[derive(Debug)]
pub struct Hollow {
    pub bores: Vec<Bore>,
    pub cavities: usize,
    pub boundary_cells: usize,
    pub cavity_cells: usize,
    pub missing_drain: usize,
    pub missing_vent: usize,
    pub intrusion: Vec<Located>,
    pub undecided_intrusions: usize,
    pub widths_short: usize,
    pub bulk_routes_missing: usize,
    pub distinct_mouths_missing: usize,
    pub reason: Option<String>,
    pub discovery_incomplete: bool,
    /// H5 Pass and the inside exemption are unreachable by construction today: every
    /// surface triangle leaves unresolved cells, and contact-bearing parts touch that surface.
    /// This deliberate fail-closed boundary needs future positive certification work.
    pub drained: bool,
    grid: Option<Grid>,
}

/// Convexity, unlike genus zero, proves that there cannot be a hidden cavity.
/// This is a one-sided proof on written vertices; no tolerance can hide a concavity.
pub fn convex_material(scene: &Scene) -> bool {
    if scene.model.len() > 4096 {
        return false;
    }
    scene.model.iter().all(|t| {
        let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        len(n) > 0.0
            && scene
                .model
                .iter()
                .flatten()
                .all(|p| dot(n, sub(*p, t[0])) <= 0.0)
    })
}

impl Hollow {
    pub fn analyze(scene: &Scene) -> Self {
        let mut out = Self {
            bores: vec![],
            cavities: 0,
            boundary_cells: 0,
            cavity_cells: 0,
            missing_drain: 0,
            missing_vent: 0,
            intrusion: vec![],
            undecided_intrusions: 0,
            widths_short: 0,
            bulk_routes_missing: 0,
            distinct_mouths_missing: 0,
            reason: None,
            discovery_incomplete: false,
            drained: false,
            grid: None,
        };
        if !scene.declared_hollow {
            out.reason =
                Some("hollow analysis not applicable: scene is not declared hollow".to_owned());
            return out;
        }
        if !scene.model_closed {
            out.reason =
                Some("hollow search needs a closed, oriented material boundary".to_owned());
            return out;
        }
        if convex_material(scene) {
            out.reason = Some("declared hollow but no cavity was found: exactly convex material contradicts the declaration; zero mouths found".to_owned());
            out.discovery_incomplete = true;
            return out;
        }
        let mut g = match Grid::build(scene) {
            Ok(g) => g,
            Err(why) => {
                out.reason = Some(why);
                return out;
            }
        };
        let chambers = g.chambers();
        out.cavities = chambers.len();
        out.cavity_cells = chambers.iter().map(Vec::len).sum();
        out.boundary_cells = g.model.iter().filter(|c| **c == UNKNOWN).count();
        let mut discovery_reasons = Vec::new();
        for (ci, cells) in chambers.iter().enumerate() {
            out.bores
                .extend(bores::find(scene, &g, ci, cells, &mut discovery_reasons));
        }
        if out.bores.is_empty() {
            discovery_reasons.push("zero mouths found: the bounded extreme-layer detector cannot resolve off-extreme openings, non-axial holes or a straight through-bore without a wider chamber".to_owned());
        }
        out.discovery_incomplete = !discovery_reasons.is_empty();
        if !discovery_reasons.is_empty() {
            out.reason = Some(discovery_reasons.join("; "));
        }
        for b in &out.bores {
            // Undo the two-sided 1e-7 width-measurement reserve before declaring a
            // definite shortfall; an exactly 3 mm written throat is not below P20.
            if 2.0 * b.radius + 2.1e-7 < scene.limits.drain_min_mm {
                out.widths_short += 1;
            }
        }
        let (hits, unknown) = bores::intrusions(scene, &out.bores);
        out.intrusion = hits;
        out.undecided_intrusions = unknown;
        if let Err(why) = g.assembly(scene) {
            out.reason = Some(why);
        } else {
            let down = g.directed(true, true);
            let air = g.directed(false, true);
            for (ci, cells) in chambers.iter().enumerate() {
                // A broad open recess is not automatically an enclosed hollow. Only an
                // independently measured through-wall throat establishes this bounded
                // detector's hollow class. Other shapes remain unverified, not failures.
                if out.bores.iter().any(|b| b.chamber == ci) {
                    // Support material replaces resin; it is an obstruction, not a
                    // trapped resin cell. Keep all remaining voids, even tiny pockets.
                    out.missing_drain +=
                        usize::from(cells.iter().any(|&i| g.assembly[i] != MATERIAL && !down[i]));
                    out.missing_vent +=
                        usize::from(cells.iter().any(|&i| g.assembly[i] != MATERIAL && !air[i]));
                }
            }
            out.bulk_routes_missing = g.bulk_routes(&chambers, scene.limits.drain_min_mm, true);
            out.distinct_mouths_missing = (0..chambers.len())
                .filter(|&ci| {
                    out.bores
                        .iter()
                        .filter(|b| b.chamber == ci && 2.0 * b.radius >= scene.limits.drain_min_mm)
                        .count()
                        < 2
                })
                .count();
            // Boundary ambiguity already forbids passing. Do not run an expensive strict
            // search when its result cannot change that decision.
            let strict = out.boundary_cells == 0 && {
                let strict_down = g.directed(true, false);
                let strict_air = g.directed(false, false);
                chambers
                    .iter()
                    .flatten()
                    .filter(|&&i| g.assembly[i] != MATERIAL)
                    .all(|&i| strict_down[i] && strict_air[i])
                    && g.bulk_routes(&chambers, scene.limits.drain_min_mm, false) == 0
            };
            out.drained = !out.discovery_incomplete
                && !chambers.is_empty()
                && strict
                && out.boundary_cells == 0
                && out.widths_short == 0
                && out.distinct_mouths_missing == 0
                && out.bulk_routes_missing == 0
                && out.intrusion.is_empty()
                && out.undecided_intrusions == 0;
        }
        if out.cavities == 0 {
            out.reason = Some("zero mouths found: the fixed grid did not resolve an interior chamber; off-extreme openings or straight through-bores may be missed; absence is not proved".to_owned());
        }
        out.grid = Some(g);
        out
    }

    /// A whole emitted part must occupy definite cells of one established drained chamber.
    /// Vertex-only tests cannot prove this: a brace can cross the wall between inside ends.
    /// No inside label or native record flag is consulted.
    pub fn wholly_inside(&self, scene: &Scene, pi: usize) -> bool {
        if !self.drained {
            return false;
        }
        let Some(g) = &self.grid else {
            return false;
        };
        let p = &scene.parts[pi];
        if !p.is_valid() {
            return false;
        }
        let Some(range) = g.covered_box(&p.bbox) else {
            return false;
        };
        // A box proof is intentionally stronger than a sampled triangle proof. Boundary-
        // crossing and attachment parts keep outside rules unless the whole solid is proved.
        let mut chamber = None;
        for z in range[2].clone() {
            for y in range[1].clone() {
                for x in range[0].clone() {
                    let i = g.index(x, y, z);
                    if g.model[i] != FREE || g.labels[i] == 0 {
                        return false;
                    }
                    if chamber.is_some_and(|c| c != g.labels[i]) {
                        return false;
                    }
                    chamber = Some(g.labels[i]);
                }
            }
        }
        chamber.is_some()
    }

    pub fn detail(&self) -> Json {
        Json::obj([
            ("cell_mm", Json::Num(CELL_MM)),
            (
                "cavities_found_before_clearance_erosion",
                Json::Uint(self.cavities as u64),
            ),
            (
                "definite_cavity_cells",
                Json::Uint(self.cavity_cells as u64),
            ),
            (
                "boundary_cells_unresolved",
                Json::Uint(self.boundary_cells as u64),
            ),
            (
                "residual_volume_uncertainty_upper_mm3",
                Json::Num(self.boundary_cells as f64 * CELL_MM.powi(3)),
            ),
            (
                "chambers_without_downhill_route_even_optimistically",
                Json::Uint(self.missing_drain as u64),
            ),
            (
                "chambers_without_plate_side_vent_even_optimistically",
                Json::Uint(self.missing_vent as u64),
            ),
            (
                "support_parts_intruding",
                Json::Uint(self.intrusion.len() as u64),
            ),
            (
                "undecided_intrusions",
                Json::Uint(self.undecided_intrusions as u64),
            ),
            ("drainage_proved", Json::Bool(self.drained)),
            (
                "mouths_below_minimum_width",
                Json::Uint(self.widths_short as u64),
            ),
            (
                "chambers_without_optimistic_bulk_width_routes",
                Json::Uint(self.bulk_routes_missing as u64),
            ),
            (
                "chambers_without_two_measured_minimum_width_mouths",
                Json::Uint(self.distinct_mouths_missing as u64),
            ),
            (
                "bulk_route_rule",
                Json::str(
                    "separate Euclidean-clearance erosion of the full assembly, then directed floods at the configured drain width; pre-erosion pockets remain in the denominator; missing bulk routes are unverified, not a flow failure proof",
                ),
            ),
            (
                "effective_width_rule",
                Json::str(
                    "3 mm nominal means at least 3 mm clear emitted bore, not the approximately 2 mm wet-resin gap; widths below are swept axial inscribed diameters",
                ),
            ),
            (
                "scope",
                Json::str(
                    "bounded finished-part axial-mouth search; all assembly meshes and the plate obstruct routes; not FR-17a, suction, viscosity or wash certification; unresolved surface cells prohibit a pass and prohibit interior exemptions",
                ),
            ),
            (
                "mouths",
                Json::Arr(
                    self.bores
                        .iter()
                        .map(|b| {
                            Json::obj([
                                ("chamber", Json::Uint(b.chamber as u64)),
                                ("local_mouth_mm", Json::Arr(b.mouth.map(Json::Num).to_vec())),
                                ("clear_diameter_lower_mm", Json::Num(2.0 * b.radius)),
                                ("bore_axis", Json::Uint(b.axis as u64)),
                                (
                                    "local_bore_span_mm",
                                    Json::Arr(b.span.map(Json::Num).to_vec()),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "search_refusal",
                self.reason.as_ref().map_or(Json::Null, Json::str),
            ),
        ])
    }

    pub fn outcome(&self) -> Outcome {
        let d = self.detail();
        if self.discovery_incomplete {
            return Outcome::unverified(
                self.reason
                    .as_deref()
                    .unwrap_or("mouth discovery is incomplete"),
            )
            .with_located(self.intrusion.iter().take(MAX_LOCATED).cloned().collect())
            .with_detail(d);
        }
        if !self.intrusion.is_empty()
            || self.missing_drain > 0
            || self.missing_vent > 0
            || self.widths_short > 0
        {
            let mut hits = self.intrusion.clone();
            hits.truncate(MAX_LOCATED);
            return Outcome::violation(format!(
                "{} support part(s) enter a measured bore or local mouth keep-out; {} chamber(s) lack a downhill drain and {} lack a plate-side vent even with uncertain cells open; {} measured mouth(s) are narrower than the configured P20 minimum",
                self.intrusion.len(), self.missing_drain, self.missing_vent, self.widths_short
            )).with_located(hits).with_detail(d);
        }
        if self.drained {
            return Outcome::pass(format!("{} drained and vented chamber(s), every opening at least the configured nominal clear width; no support intrusion", self.cavities)).with_detail(d);
        }
        Outcome::unverified(self.reason.clone().unwrap_or_else(|| format!(
            "{} cavity candidate(s), {} axial mouth(s); {} boundary cells unresolved, {} narrow/uncertain widths and {} undecided support intersections; drainage and interior exemptions are not proved",
            self.cavities, self.bores.len(), self.boundary_cells, self.widths_short, self.undecided_intrusions
        ))).with_detail(d)
    }
}

/// Surface/cell intersection: separate along box axes, the face normal and edge/box axes.
/// No sampled corner test (which misses triangles cutting a cell between its corners).
fn triangle_cell(t: &Tri, bb: &Aabb) -> bool {
    let c = bb.centre();
    let h = mul(sub(bb.hi, bb.lo), 0.5);
    let v = t.map(|p| sub(p, c));
    let e = [sub(v[1], v[0]), sub(v[2], v[1]), sub(v[0], v[2])];
    let separated = |n: V| {
        let r = (0..3).map(|k| h[k] * n[k].abs()).sum::<f64>() + 1e-10 * len(n);
        let p = v.map(|p| dot(n, p));
        p.iter().all(|p| *p > r) || p.iter().all(|p| *p < -r)
    };
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    !axes.iter().any(|a| separated(*a))
        && !separated(cross(e[0], e[1]))
        && !e
            .iter()
            .any(|e| axes.iter().any(|a| separated(cross(*e, *a))))
}
