//! The down-sweep: how big the supports are.
//!
//! Walk the layers back down carrying a load raster denominated in **mm² of
//! reference peel**. Per layer: spread the incoming raster into this layer's
//! cured cells with force conserved; take per cell the larger of that and this
//! layer's own peel event; let struts read their share and carry it as columns
//! without spreading; inject a landing strut's carried load at its footprint
//! before the layer spreads onward; and size each strut's section from the
//! registered area law.
//!
//! # Max over events, not sum
//!
//! Each layer's peel is a separate event. A print never peels two layers at
//! once, so the design load on a piece of material is the **largest** event
//! routed through it, not the sum of all of them. Collapsing the history into
//! one max-field before spreading is conservative rather than merely cheap:
//! allocation is monotone and positively homogeneous, so
//! `spread(max(a,b)) ≥ max(spread(a), spread(b))` cellwise. Replacing the max
//! with a sum is sabotage S7, and on the cavity box it turns a 1600 mm² plate
//! reaction into 2756 mm² — a 72 % lie, which is why that oracle exists.
//!
//! # Conservation is checked, not assumed
//!
//! Within one event force is conserved, and the spreading step asserts it to
//! 1e-9 relative every layer. A conservation break is a DEFECT IN THIS CODE,
//! not an outcome about a model, which is the one thing in this crate that is
//! allowed to be a hard error rather than a warning.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "mm-scale lengths into f32 per-cell fields, and cell or layer counts into f64 areas; a \
          count bounded by the cell-layer budget is exact in f64 and a mm-scale length is exact to \
          1e-5 mm in f32"
)]
#![expect(
    clippy::needless_range_loop,
    reason = "batch-first: these loops walk several parallel per-cell rasters at one index"
)]

use plinth_support::perception::SliceSolid;

use crate::grid::Grid;
use crate::raster::{rasterize_into, LayerMasks, RasterScratch};
use crate::report::{ModelSectionOverload, OverloadedStrut};
use crate::{Landing, SweepStrut};

/// Everything the down-sweep learned.
#[derive(Clone, Debug, Default)]
pub struct DownSweepOutcome {
    pub plate_reaction_mm2: f64,
    pub uncarried_load_mm2: f64,
    /// Every mm² of peel this sweep ever put INTO the load raster.
    ///
    /// The conservation total, and the only quantity the plate reaction and the
    /// total scar can honestly be graded against on a model whose arms are fed
    /// by more than one peel event. See the max-over-events step for the
    /// derivation and for why it equals `peak_layer_area_mm2` on every
    /// flat-bottomed fixture.
    pub injected_load_mm2: f64,
    pub peak_layer_area_mm2: f64,
    /// The largest `X_model` any model section realized, overloaded or not.
    /// Without it, "no warnings fired" cannot be told apart from "the path
    /// never ran".
    pub model_section_max_x: f64,
    pub min_layer_area_mm2: f64,
    pub conservation_residual: f64,
    pub overloaded: Vec<OverloadedStrut>,
    pub model_overload: Vec<ModelSectionOverload>,
    pub spread_search_hits: u64,
    pub strut_share_reads: u64,
    /// The lowest layer at which any load reached no carrier, and the worst
    /// gap seen there. Without these, `uncarried_load_mm2` says a number and
    /// nothing about which sweep is wrong; with them a reader can go and look
    /// at that layer.
    pub uncarried_lowest_layer: Option<usize>,
    pub uncarried_worst_gap_mm: f64,
    pub landing_injections: u64,
    pub max_combine_hits: u64,
    pub rasteriser_entries: u64,
    /// Layers whose routing field hit the sweep bound without reaching a fixed
    /// point. A WARNING: an unconverged field is too HIGH, so the sweep loses
    /// load it could have routed and says so, rather than routing load along a
    /// path it never proved.
    pub route_not_converged: u64,
}

/// Sizing inputs, every one read from the registry by the caller.
#[derive(Clone, Copy, Debug)]
pub struct SizingPolicy {
    pub x_allowed: f64,
    pub tip_radius_min_mm: f64,
    pub tip_radius_max_mm: f64,
    pub owner_growth_ceiling_mm: f64,
    pub alpha: f64,
    /// One cured lamina's reach, `κ · h`. The ONLY radius the spread search
    /// needs, and that is a theorem rather than a simplification: a load cell
    /// that has to SPAN is one whose own column did not carry one layer down,
    /// so its cured run is exactly one lamina. Every cell the up-sweep let
    /// spend more than one lamina of reach is standing on a carrier and spans
    /// nothing. No stored per-layer thickness field is therefore required, and
    /// the two sweeps still price the same journey the same way — see
    /// `reach::recompute_potential`.
    pub lamina_reach_mm: f64,
}

/// Run the whole down-sweep over `struts`, filling their radius and carried
/// profiles.
///
/// # Panics
///
/// On a conservation residual above 1e-9 relative, and on a carried profile
/// that decreases downward. Both are defects in this code rather than
/// statements about a model, which is why they are assertions and not
/// warnings.
pub fn run(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    struts: &mut [SweepStrut],
    sizing: SizingPolicy,
) -> DownSweepOutcome {
    let cells = grid.cells();
    let area = grid.cell_area_mm2();
    let mut outcome = DownSweepOutcome {
        min_layer_area_mm2: f64::INFINITY,
        ..DownSweepOutcome::default()
    };

    let mut scratch = RasterScratch::new(grid);
    let mut masks = LayerMasks::default();
    let mut load_in = vec![0.0_f64; cells];
    let mut load_out = vec![0.0_f64; cells];
    let mut carriers = vec![false; cells];
    // Layer k+1's cover — the material the load actually travels through on
    // its way down into layer k. All false above the top layer, where there is
    // no load to route.
    let mut cover_above = vec![false; cells];
    let mut nearest = vec![u32::MAX; cells];
    let mut distance = vec![f32::INFINITY; cells];
    let mut distance_prev = vec![f32::INFINITY; cells];
    let mut pending = vec![0.0_f64; struts.len()];
    let mut labels = vec![u32::MAX; cells];
    let mut parent = Vec::new();

    // Struts are indexed by the layer they are alive on. A strut occupies
    // [k_bottom, k_top] inclusive; at k_top it coexists with the model cell it
    // seats against.
    let mut alive_by_layer: Vec<Vec<u32>> = vec![Vec::new(); slices.len()];
    for strut in struts.iter() {
        strut_layers(strut, slices.len(), &mut alive_by_layer);
    }
    let layer_count = slices.len();
    for strut in struts.iter_mut() {
        let span = ledger_len(strut, layer_count);
        strut.radius_mm.clear();
        strut.carried_mm2.clear();
        strut.radius_mm.resize(span, 0.0);
        strut.carried_mm2.resize(span, 0.0);
    }

    for layer in (0..slices.len()).rev() {
        outcome.rasteriser_entries += 1;
        rasterize_into(grid, &slices[layer], &mut scratch, &mut masks);
        let model_cells = masks.carrier_cells();
        let layer_area = model_cells as f64 * area;
        if model_cells > 0 {
            outcome.peak_layer_area_mm2 = outcome.peak_layer_area_mm2.max(layer_area);
            outcome.min_layer_area_mm2 = outcome.min_layer_area_mm2.min(layer_area);
        }

        carriers.copy_from_slice(&masks.carry);
        for &id in &alive_by_layer[layer] {
            carriers[struts[id as usize].cell] = true;
        }

        // --- spread the incoming raster into this layer's carriers ----------
        //
        // The load in `load_in` sits on layer k+1's cured cells. It travels
        // laterally THROUGH layer k+1's own material — `cover_above` — to a
        // column that continues down into layer k, and then goes straight down.
        // Routing through free space instead (a plain distance transform over
        // the whole grid) is the criterion mismatch that let this sweep print
        // `residual_demand == 0` beside `uncarried_load = 595.7 mm²` about one
        // field: it is LAXER than the up-sweep in the plane and STRICTER about
        // thickness, so neither implied the other. Same metric, same cover,
        // same one-lamina bound, and now the up-sweep's rootedness implies this
        // routability by construction (`reach::recompute_potential`).
        load_out.iter_mut().for_each(|value| *value = 0.0);
        let mut incoming = 0.0_f64;
        let mut placed = 0.0_f64;
        let mut lost = 0.0_f64;
        let route = route_within_cover(
            grid,
            &cover_above,
            &carriers,
            &mut distance,
            &mut nearest,
            &mut distance_prev,
        );
        if !route.converged {
            outcome.route_not_converged += 1;
        }
        let any_root = route.any_root;
        for cell in 0..cells {
            let load = load_in[cell];
            if load <= 0.0 {
                continue;
            }
            incoming += load;
            if carriers[cell] {
                load_out[cell] += load;
                placed += load;
                continue;
            }
            outcome.spread_search_hits += 1;
            let target = if any_root { nearest[cell] } else { u32::MAX };
            let reachable = target != u32::MAX
                && f64::from(distance[cell]) <= sizing.lamina_reach_mm + grid.pitch_mm();
            if reachable {
                // Nearest first, share by area. One raster cell of carrier has
                // exactly the source cell's own area, so the accumulation
                // stops at the first carrier and its weight is `area / area`.
                // The division is kept explicit because it is the
                // renormalisation sabotage S3 removes.
                let accumulated = area;
                let weight = area / accumulated;
                load_out[target as usize] += load * weight;
                placed += load * weight;
            } else {
                lost += load;
                outcome.uncarried_lowest_layer = Some(layer);
                let gap = if target == u32::MAX {
                    f64::INFINITY
                } else {
                    f64::from(distance[cell])
                };
                if gap > outcome.uncarried_worst_gap_mm {
                    outcome.uncarried_worst_gap_mm = gap;
                }
            }
        }
        let residual = (placed + lost - incoming).abs();
        let tolerance = 1.0e-9 * incoming.max(1.0);
        assert!(
            residual <= tolerance,
            "layer {layer}: spreading lost {residual} mm2 of {incoming} mm2; force must be \
             conserved within one peel event"
        );
        outcome.conservation_residual = outcome.conservation_residual.max(residual);
        outcome.uncarried_load_mm2 += lost;

        // --- this layer's own peel event, and the maximum -------------------
        //
        // What the max ADDS over what arrived is this layer's newly injected
        // load, and its running total is the conservation quantity every oracle
        // grades against: everything injected must arrive somewhere, and
        // nothing else ever enters the system. On a flat-bottomed model only
        // the topmost layer of each column injects, so the total is that
        // model's largest cross-section and this is §C2's identity verbatim. On
        // a SLOPED underside each layer births a new crescent that injects too,
        // so the total is the model's XY silhouette instead — larger than the
        // peak layer, and correctly so, because those are different peel events
        // carried by different arms.
        let own = area * sizing.alpha;
        for cell in 0..cells {
            if carriers[cell] {
                let combined = load_out[cell].max(own);
                outcome.injected_load_mm2 += combined - load_out[cell];
                load_out[cell] = combined;
                outcome.max_combine_hits += 1;
            } else {
                load_out[cell] = 0.0;
            }
        }

        // --- struts are columns; terminations first so a parent sees its
        //     children's load before it reads its own share -------------------
        for pass in 0..2 {
            for &id in &alive_by_layer[layer] {
                let index = id as usize;
                let terminating = struts[index].k_bottom == layer;
                if (pass == 0) != terminating {
                    continue;
                }
                let cell = struts[index].cell;
                let share = load_out[cell] + pending[index];
                pending[index] = 0.0;
                load_out[cell] = 0.0;
                outcome.strut_share_reads += 1;
                let slot = layer - struts[index].k_bottom;
                let previous = if slot + 1 < struts[index].carried_mm2.len() {
                    struts[index].carried_mm2[slot + 1]
                } else {
                    0.0
                };
                // MAX over events, not sum -- the same rule the model raster
                // obeys twenty lines up. `previous` is this arm's design load
                // from every event ABOVE this layer; `share` is the load that
                // arrives HERE, in this layer's own peel event. A print never
                // peels two layers at once, so the arm must survive the larger
                // of them, not their sum.
                //
                // Summing was measured and it is a lie of unbounded size: an
                // arm's cell is a carrier on every layer it passes through, so
                // it collects at least this layer's own per-cell event every
                // layer, and a 500-layer arm charged 500 events. On the
                // 10-degree Round 100 it printed one arm carrying 2566.58 mm2
                // against a whole-model peak layer event of 682.85 mm2 -- 3.76x
                // the largest peel event anywhere in the model -- and drove the
                // plate reaction to 10.5x the peak, where §C2 of the design of
                // record states the plate sees "the largest single layer
                // cross-section". Sabotage S11 turns this back into `+` and
                // must turn `oracle_ramp` red.
                //
                // `max` keeps the profile non-decreasing downward exactly as
                // `+` did (`max(previous, share) >= previous`), so `size_arm`'s
                // flare property and the monotonicity assertion are unchanged.
                let carried = previous.max(share);
                struts[index].carried_mm2[slot] = carried;
                let radius = size_arm(carried, sizing);
                struts[index].radius_mm[slot] = radius;
                if carried > sizing.x_allowed * core::f64::consts::PI * sizing.tip_radius_max_mm
                    * sizing.tip_radius_max_mm
                {
                    outcome.overloaded.push(OverloadedStrut {
                        strut_id: struts[index].id,
                        layer,
                        carried_mm2: carried,
                        realized_x: carried
                            / (core::f64::consts::PI
                                * sizing.tip_radius_max_mm
                                * sizing.tip_radius_max_mm),
                    });
                }
                if terminating {
                    match struts[index].landing {
                        Landing::Plate => outcome.plate_reaction_mm2 += carried,
                        Landing::Model { .. } => {
                            // Injected at the footprint BEFORE this layer
                            // spreads onward, so the routed load keeps going
                            // down through the model rather than vanishing at
                            // the seat.
                            load_out[cell] += carried;
                            outcome.landing_injections += 1;
                        }
                        Landing::Strut { id: parent_id, .. } => {
                            pending[parent_id as usize] += carried;
                            outcome.landing_injections += 1;
                        }
                    }
                }
            }
        }

        // --- the model's own section is checked, never resized --------------
        if model_cells > 0 {
            check_model_sections(
                grid,
                &masks.carry,
                &load_out,
                layer,
                sizing.x_allowed,
                &mut labels,
                &mut parent,
                &mut outcome.model_overload,
                &mut outcome.model_section_max_x,
            );
        }

        std::mem::swap(&mut load_in, &mut load_out);
        // This layer's cover is the next iteration's "above".
        cover_above.copy_from_slice(&masks.cover);
    }

    // Anything still in the raster at the bottom of the stack reached the
    // plate through model material rather than through an arm.
    outcome.plate_reaction_mm2 += load_in.iter().sum::<f64>();
    if !outcome.min_layer_area_mm2.is_finite() {
        outcome.min_layer_area_mm2 = 0.0;
    }
    outcome
}

/// Which layers an arm is a CARRIER on.
///
/// `[k_bottom, k_top)` — up to but NOT including the seat layer. At the seat
/// layer the arm's cell is inside the model, and the model's own lamina there
/// is carried by whatever is under it, which is this arm one layer down. Letting
/// the arm read a share at the seat layer instead would charge it one raster
/// cell of the model's own event and leave the whole lamina's load to be picked
/// up below the tip — which reads as a hairline tip on an arm whose root is
/// correctly sized, exactly the RC4 symptom this engine exists to kill.
fn strut_layers(strut: &SweepStrut, layers: usize, alive: &mut [Vec<u32>]) {
    let top = strut.k_top.min(layers).saturating_sub(1);
    for layer in strut.k_bottom..=top {
        alive[layer].push(strut.id);
    }
}

/// The number of layers an arm's ledger spans.
fn ledger_len(strut: &SweepStrut, layers: usize) -> usize {
    let top = strut.k_top.min(layers).saturating_sub(1);
    top.saturating_sub(strut.k_bottom) + 1
}

/// `A = ℓ / X_ALLOWED`, then `r = sqrt(A/π)` inside the registered clamps.
///
/// `carried` is non-decreasing downward, so `r` is too: the arm flares toward
/// its root and its NARROWEST section is at the tip, which is where the law is
/// evaluated and exactly what `docs/physics.md`:89-92 asks for. The law holds
/// at every layer, not only at the contact.
fn size_arm(carried_mm2: f64, sizing: SizingPolicy) -> f64 {
    let required_area = carried_mm2 / sizing.x_allowed;
    let radius = (required_area / core::f64::consts::PI).sqrt();
    radius.clamp(sizing.tip_radius_min_mm, sizing.tip_radius_max_mm)
}

/// Where each loaded cell of layer k+1 sends its load, and how far it has to
/// travel through layer k+1's own material to get there.
///
/// Roots (`distance = 0`) are the cells of `cover_above` whose column is a
/// CARRIER at layer k — model material or a live arm. Travel is restricted to
/// `cover_above` and priced on the same 8-neighbour chamfer the reach potential
/// uses (`p`, `p√2`), so "the down-sweep can route this load" and "the up-sweep
/// called this cell rooted" are one predicate on one metric over one domain.
/// Returns whether any root exists at all.
///
/// This is what keeps the spread batch-first. A per-source ring search would
/// cost 5,700 probes per cell — 4e9 on one disc layer — and a per-item search
/// in a hot path is a defect, not a slow implementation.
///
/// # Why it sweeps to a fixed point
///
/// Two raster passes are exact for a free-space distance transform and are NOT
/// exact for a geodesic one: a path that has to go around a hole reverses
/// direction, and one forward/reverse pair cannot carry it. Under-converging
/// leaves `distance` too HIGH, which loses load that could in fact be routed —
/// the same false `uncarried_load` this whole change exists to remove. So it
/// sweeps until the field stops moving, under the same bound and the same
/// "report, never refuse" rule as `reach::recompute_potential`, and the caller
/// counts the layers that hit the bound.
fn route_within_cover(
    grid: &Grid,
    cover_above: &[bool],
    carriers: &[bool],
    distance: &mut [f32],
    nearest: &mut [u32],
    previous: &mut [f32],
) -> RouteOutcome {
    let cols = grid.cols();
    let rows = grid.rows();
    let pitch = grid.pitch_mm() as f32;
    let diagonal = pitch * core::f32::consts::SQRT_2;
    let mut any_root = false;
    for cell in 0..grid.cells() {
        if cover_above[cell] && carriers[cell] {
            distance[cell] = 0.0;
            nearest[cell] = u32::try_from(cell).unwrap_or(u32::MAX);
            any_root = true;
        } else {
            distance[cell] = f32::INFINITY;
            nearest[cell] = u32::MAX;
        }
    }
    if !any_root {
        return RouteOutcome {
            any_root: false,
            converged: true,
        };
    }
    let relax = |cell: usize, from: usize, step: f32, distance: &mut [f32], nearest: &mut [u32]| {
        let candidate = distance[from] + step;
        if candidate < distance[cell] {
            distance[cell] = candidate;
            nearest[cell] = nearest[from];
        }
    };
    const MAX_SWEEP_ROUNDS: usize = 8;
    let mut converged = false;
    previous.copy_from_slice(distance);
    for _ in 0..MAX_SWEEP_ROUNDS {
        for row in 0..rows {
            for col in 0..cols {
                let cell = row * cols + col;
                if !cover_above[cell] {
                    continue;
                }
                if col > 0 && cover_above[cell - 1] {
                    relax(cell, cell - 1, pitch, distance, nearest);
                }
                if row > 0 {
                    if cover_above[cell - cols] {
                        relax(cell, cell - cols, pitch, distance, nearest);
                    }
                    if col > 0 && cover_above[cell - cols - 1] {
                        relax(cell, cell - cols - 1, diagonal, distance, nearest);
                    }
                    if col + 1 < cols && cover_above[cell - cols + 1] {
                        relax(cell, cell - cols + 1, diagonal, distance, nearest);
                    }
                }
            }
        }
        for row in (0..rows).rev() {
            for col in (0..cols).rev() {
                let cell = row * cols + col;
                if !cover_above[cell] {
                    continue;
                }
                if col + 1 < cols && cover_above[cell + 1] {
                    relax(cell, cell + 1, pitch, distance, nearest);
                }
                if row + 1 < rows {
                    if cover_above[cell + cols] {
                        relax(cell, cell + cols, pitch, distance, nearest);
                    }
                    if col + 1 < cols && cover_above[cell + cols + 1] {
                        relax(cell, cell + cols + 1, diagonal, distance, nearest);
                    }
                    if col > 0 && cover_above[cell + cols - 1] {
                        relax(cell, cell + cols - 1, diagonal, distance, nearest);
                    }
                }
            }
        }
        if previous[..] == distance[..] {
            converged = true;
            break;
        }
        previous.copy_from_slice(distance);
    }
    RouteOutcome {
        any_root: true,
        converged,
    }
}

/// What one layer's routing field cost to build.
struct RouteOutcome {
    any_root: bool,
    converged: bool,
}
/// Per connected component of MODEL material, `X_model = Σ L / component area`.
///
/// **The model's own material is never sized.** The engine supports models; it
/// does not redesign them. An overloaded section is a warning naming the layer,
/// the component and the realized `X`, on a file that is still emitted.
#[expect(
    clippy::too_many_arguments,
    reason = "a per-layer component check reads the grid, the mask, the load, its layer, the limit \
          and two reusable scratch buffers, and writes one output list; a struct would move the \
          list rather than shorten it"
)]
fn check_model_sections(
    grid: &Grid,
    model: &[bool],
    load: &[f64],
    layer: usize,
    x_allowed: f64,
    labels: &mut [u32],
    parent: &mut Vec<u32>,
    out: &mut Vec<ModelSectionOverload>,
    max_x: &mut f64,
) {
    let cols = grid.cols();
    let rows = grid.rows();
    parent.clear();
    labels.fill(u32::MAX);
    for row in 0..rows {
        for col in 0..cols {
            let cell = row * cols + col;
            if !model[cell] {
                continue;
            }
            let left = (col > 0 && model[cell - 1]).then(|| labels[cell - 1]);
            let up = (row > 0 && model[cell - cols]).then(|| labels[cell - cols]);
            match (left, up) {
                (Some(a), Some(b)) => {
                    labels[cell] = find(parent, a);
                    union(parent, a, b);
                }
                (Some(a), None) | (None, Some(a)) => labels[cell] = find(parent, a),
                (None, None) => {
                    let fresh = u32::try_from(parent.len()).unwrap_or(u32::MAX);
                    parent.push(fresh);
                    labels[cell] = fresh;
                }
            }
        }
    }
    if parent.is_empty() {
        return;
    }
    let mut counts = vec![0_u64; parent.len()];
    let mut loads = vec![0.0_f64; parent.len()];
    for cell in 0..grid.cells() {
        if !model[cell] {
            continue;
        }
        let root = find(parent, labels[cell]) as usize;
        counts[root] += 1;
        loads[root] += load[cell];
    }
    let area = grid.cell_area_mm2();
    for component in 0..parent.len() {
        if counts[component] == 0 {
            continue;
        }
        let component_area = counts[component] as f64 * area;
        let realized = loads[component] / component_area;
        *max_x = max_x.max(realized);
        if realized > x_allowed {
            out.push(ModelSectionOverload {
                layer,
                component_area_mm2: component_area,
                carried_mm2: loads[component],
                realized_x: realized,
            });
        }
    }
}

fn find(parent: &mut [u32], mut node: u32) -> u32 {
    while parent[node as usize] != node {
        let grand = parent[parent[node as usize] as usize];
        parent[node as usize] = grand;
        node = grand;
    }
    node
}

fn union(parent: &mut [u32], a: u32, b: u32) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        parent[hi as usize] = lo;
    }
}
