//! **The width of the feature a tip touches**, read from the cured section.
//!
//! A tip's cured joint is only as strong as the narrowest section it passes
//! through (round 5, `BALLS-FLARE.md`): a ball wider than the model feature it
//! seats on adds no strength, it wraps the feature's flanks, and at removal the
//! break moves into the model. The width that matters is the narrowest chord of
//! the cured model section through the seat, at the strike layer: the first
//! lamina the printer cures over the seat (`slices[k_top]`, cut at the layer's
//! mid-plane by `slice_stack`).
//!
//! # The measure
//!
//! For a seat point `q` inside the section at the strike layer, the chord in
//! direction `d` is the length of the segment of the line `q + t d` that stays
//! in material: from the nearest boundary crossing behind `q` to the nearest
//! ahead of it (holes count as boundary). The feature's width is the minimum
//! over directions. The directions are a search resolution
//! ([`DIRECTIONS`] across a half turn, 5° apart): the minimum over a finite
//! set can only read a chord a little wide (0.1 % for a strip, whose chord
//! grows as `1 / cos` of the angle off square). A line that meets no boundary within `reach_mm` has a
//! chord of at least `2 reach_mm`, which no ball ceiling reads as a limit.
//!
//! # Batch-first
//!
//! The edges of each layer are gathered once and shared by every tip seated
//! in it; the tips run in parallel (rayon over tips).

use plinth_support::perception::SliceSolid;
use rayon::prelude::*;

/// Directions probed across a half turn: a search resolution (see the module
/// doc), not a physical constant.
pub const DIRECTIONS: usize = 36;

/// One boundary edge of a layer's section.
#[derive(Clone, Copy)]
struct Edge {
    a: [f64; 2],
    b: [f64; 2],
}

/// Every boundary edge of one layer (outer loops and holes alike).
fn layer_edges(solids: &[SliceSolid]) -> Vec<Edge> {
    let mut edges = Vec::new();
    for solid in solids {
        for ring in std::iter::once(solid.outer()).chain(solid.holes()) {
            let points = ring.points_xy_mm();
            for (index, &a) in points.iter().enumerate() {
                // (A zero-length edge crosses no ray: both readers skip it.)
                let b = points[(index + 1) % points.len()];
                edges.push(Edge { a, b });
            }
        }
    }
    edges
}

fn cross(u: [f64; 2], v: [f64; 2]) -> f64 {
    u[0] * v[1] - u[1] * v[0]
}

/// Whether `q` lies in material: an odd number of boundary crossings on the
/// ray toward +x (half-open in y, so a vertex is counted once).
fn in_material(edges: &[Edge], q: [f64; 2]) -> bool {
    let mut crossings = 0_usize;
    for edge in edges {
        if (edge.a[1] > q[1]) == (edge.b[1] > q[1]) {
            continue;
        }
        let t = (q[1] - edge.a[1]) / (edge.b[1] - edge.a[1]);
        let x = edge.a[0] + t * (edge.b[0] - edge.a[0]);
        if x > q[0] {
            crossings += 1;
        }
    }
    crossings % 2 == 1
}

/// The narrowest chord of the section through `q`, mm, or `None` where `q` is
/// not in material at this layer.
fn narrowest_chord(edges: &[Edge], q: [f64; 2], reach_mm: f64) -> Option<f64> {
    if !in_material(edges, q) {
        return None;
    }
    // Edges that can matter: those with a point within `reach` of `q` in both
    // axes. A boundary beyond it is not a limit on any ball.
    let near: Vec<&Edge> = edges
        .iter()
        .filter(|edge| {
            let (lo_x, hi_x) = (edge.a[0].min(edge.b[0]), edge.a[0].max(edge.b[0]));
            let (lo_y, hi_y) = (edge.a[1].min(edge.b[1]), edge.a[1].max(edge.b[1]));
            hi_x >= q[0] - reach_mm
                && lo_x <= q[0] + reach_mm
                && hi_y >= q[1] - reach_mm
                && lo_y <= q[1] + reach_mm
        })
        .collect();
    let mut best = 2.0 * reach_mm;
    for step in 0..DIRECTIONS {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a direction index below DIRECTIONS is exact in f64"
        )]
        let theta = std::f64::consts::PI * (step as f64) / (DIRECTIONS as f64);
        let d = [theta.cos(), theta.sin()];
        let (mut ahead, mut behind) = (reach_mm, reach_mm);
        for edge in &near {
            let e = [edge.b[0] - edge.a[0], edge.b[1] - edge.a[1]];
            let denom = cross(d, e);
            if denom.abs() < 1e-15 {
                continue;
            }
            let from = [edge.a[0] - q[0], edge.a[1] - q[1]];
            let t = cross(from, e) / denom;
            let s = cross(from, d) / denom;
            // Half-open along the edge: a vertex belongs to one edge only.
            if !(0.0..1.0).contains(&s) {
                continue;
            }
            if t > 0.0 {
                ahead = ahead.min(t);
            } else if t < 0.0 {
                behind = behind.min(-t);
            }
        }
        best = best.min(ahead + behind);
    }
    Some(best)
}

/// **The narrowest cured chord through every seat**, mm: for each `points`
/// entry `(layer, [x, y])`, the section `slices[layer]` is read at `[x, y]`.
/// `None` for a tip with no seat, a layer with no section, or a seat that is
/// not inside material at that layer (no limit is then known). Chords of
/// `2 · reach_mm` or more are returned as `2 · reach_mm`.
#[must_use]
pub fn narrowest_chords_mm(
    slices: &[Vec<SliceSolid>],
    points: &[Option<(usize, [f64; 2])>],
    reach_mm: f64,
) -> Vec<Option<f64>> {
    // One edge list per layer that any tip stands in.
    let mut layers: Vec<usize> = points.iter().flatten().map(|&(layer, _)| layer).collect();
    layers.sort_unstable();
    layers.dedup();
    let edges: Vec<(usize, Vec<Edge>)> = layers
        .into_iter()
        .filter_map(|layer| slices.get(layer).map(|solids| (layer, layer_edges(solids))))
        .collect();
    points
        .par_iter()
        .map(|point| {
            let &(layer, q) = point.as_ref()?;
            let slot = edges.binary_search_by_key(&layer, |(layer, _)| *layer).ok()?;
            narrowest_chord(&edges[slot].1, q, reach_mm)
        })
        .collect()
}
