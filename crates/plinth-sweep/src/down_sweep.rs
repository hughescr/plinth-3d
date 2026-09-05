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

use crate::drain::DrainKeepOut;
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
    /// Arms this down-sweep added because the AREA LAW asked for them — see
    /// [`insert_for_load`]. Zero on a field where reach already placed enough
    /// section, so it also tells "the path never ran" apart from "the path ran
    /// and found nothing to do" when read beside `insertion_rounds`.
    pub arms_inserted_by_load: u64,
    /// Layers where the bounded search hit `InsertionPolicy::max_rounds`
    /// while the area law still wanted more carriers, i.e. an ARTIFICIAL
    /// limit was hit, not a physical one. Error direction: UNDER-supported —
    /// more rounds might still have found a footprint, so a nonzero count
    /// here is a search bound to loosen (`max_rounds`), not evidence that the
    /// geometry has no room. Disjoint from `insertion_geometry_exhausted`;
    /// their sum is the count the old, conflated `insertion_not_converged`
    /// reported.
    pub insertion_rounds_exhausted: u64,
    /// Layers where a round ran and [`insert_for_load`] placed zero arms
    /// while the area law still wanted more carriers, i.e. the search found
    /// NOWHERE left to put another carrier — a physical exhaustion, not a
    /// bound. Error direction: genuinely OVER-supported at that layer; more
    /// rounds would not help, and the arms that remain over the limit are
    /// named in `overloaded`. Disjoint from `insertion_rounds_exhausted`;
    /// their sum is the count the old, conflated `insertion_not_converged`
    /// reported.
    pub insertion_geometry_exhausted: u64,
    /// Insertion rounds actually executed, summed over layers. A path-hit
    /// counter: without it, `arms_inserted_by_load == 0` cannot be told apart
    /// from an insertion step that never ran.
    pub insertion_rounds: u64,
}

/// Whether the down-sweep may add arms of its own, and how many arms the whole
/// field may hold.
///
/// **This is not a threshold and it decides nothing.** The trigger for adding
/// an arm is the registered area law itself — a tip may carry
/// `X_ALLOWED · π · r_max²` and no more — and `max_rounds` bounds a search
/// whose non-convergence is REPORTED, split by cause
/// (`DownSweepOutcome::insertion_rounds_exhausted` for the bound,
/// `insertion_geometry_exhausted` for the physics), rather than hidden or
/// conflated. `max_struts` is the caller's existing resource authority
/// (`crate::SweepLimits`), passed through so a field the insertion grows
/// cannot outrun it.
#[derive(Clone, Copy, Debug)]
pub struct InsertionPolicy {
    pub enabled: bool,
    pub max_struts: usize,
    /// Bounded rounds per layer. Each round computes the number of carriers
    /// the area law asks for and puts that many in at once, so one round
    /// normally suffices and further rounds only handle the case where the
    /// first round's own arms are themselves over the limit.
    pub max_rounds: usize,
}

impl InsertionPolicy {
    /// The production authority: insertion on, four rounds, the caller's own
    /// strut cap.
    #[must_use]
    pub const fn new(max_struts: usize) -> Self {
        Self {
            enabled: true,
            max_struts,
            max_rounds: 4,
        }
    }

    /// Insertion off — reach places the field alone. Used by the measurement
    /// that shows what load-driven insertion changes, and by nothing in the
    /// production path.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            max_struts: 0,
            max_rounds: 0,
        }
    }
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
/// profiles, and ADDING arms wherever the area law asks for section the
/// up-sweep's reach criterion never had a reason to place.
///
/// # Two placement criteria, one field
///
/// Reach answers "can cured material span from here to a carrier". The area
/// law answers "can this arm carry what is routed through it". They are
/// different questions and the up-sweep can only ask the first: at the top of
/// the up-sweep no load has been routed yet, so nothing there knows that a
/// covering which is correct for reach concentrates 123 mm² of peel on one
/// tip whose registered maximum is `X_ALLOWED · π · r_max² = 37.699 mm²`.
/// Before this step the engine's only answer was to clamp the radius at
/// `r_max` and warn — 1800 warned arm-layers on the laid-over Cylinder, 514 on
/// `oracle_ramp` — which is an honest warning about a field the engine could
/// have fixed and did not.
///
/// The trigger is the area law and nothing else. No threshold is introduced:
/// an arm gets siblings exactly when the load routed to it at some layer
/// exceeds what one tip may carry, and it gets `ceil(routed / ℓ_max) − 1` of
/// them, spaced by the hex spacing that puts that many carriers on the
/// footprint the load actually came from.
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
    struts: &mut Vec<SweepStrut>,
    sizing: SizingPolicy,
    insertion: InsertionPolicy,
    keep_out: &DrainKeepOut,
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
    // Layer k+1's CARRIERS, as opposed to its cover: model that carried plus every
    // arm alive there. Only the injection accounting reads it — see the max step.
    let mut carriers_above = vec![false; cells];
    let mut nearest = vec![u32::MAX; cells];
    let mut distance = vec![f32::INFINITY; cells];
    let mut distance_prev = vec![f32::INFINITY; cells];
    let mut pending = vec![0.0_f64; struts.len()];
    let mut labels = vec![u32::MAX; cells];
    let mut parent = Vec::new();
    // Insertion scratch, allocated once. Arms this sweep adds are "growing"
    // until their column meets something: their landing is discovered by the
    // descent itself, which is the same "first thing you hit going down" rule
    // `up_sweep::descend` applies, read off the layers this pass is already
    // walking instead of off a history that only exists going up.
    let mut growing: Vec<usize> = Vec::new();
    let mut landing_target = vec![u32::MAX; cells];
    let mut insert = InsertScratch::new(cells);
    let mut layer_arms: Vec<LayerArm> = Vec::new();
    // The largest peel one tip may legally carry, from the registered area law
    // and the registered maximum radius. `crates/plinth-support/src/
    // perception.rs`:610-612 computes the same quantity for the legacy engine.
    let l_max = sizing.x_allowed
        * core::f64::consts::PI
        * sizing.tip_radius_max_mm
        * sizing.tip_radius_max_mm;

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
        landing_target.fill(u32::MAX);
        for &id in &alive_by_layer[layer] {
            let cell = struts[id as usize].cell;
            carriers[cell] = true;
            // What a growing arm lands ON if its column meets this one. Only
            // arms whose extent is already fixed are landing targets; a growing
            // arm can never be one, because an insertion candidate must not be
            // a carrier and a growing arm's cell always is.
            landing_target[cell] = id;
        }
        for &index in &growing {
            carriers[struts[index].cell] = true;
        }

        // --- spread the incoming raster into this layer's carriers, and add
        //     the carriers the AREA LAW asks for ---------------------------
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
        //
        // The rounds are the LOAD criterion joining the reach criterion. A
        // round routes the load, sees how much of it lands on each arm, and —
        // where one arm collects more than `ℓ_max = X_ALLOWED · π · r_max²` —
        // puts the extra carriers the area law asks for into that arm's own
        // routed footprint and routes again. Arriving at a fixed point is the
        // normal case in one round, because the round inserts the whole
        // shortfall at once rather than one arm at a time; the bound exists for
        // the case where the new arms are themselves over the limit, and
        // hitting it is REPORTED (`insertion_rounds_exhausted`), never a
        // refusal.
        let mut incoming;
        let mut placed;
        let mut lost;
        let mut spread_hits;
        let mut max_hits;
        let mut injected_here;
        let mut worst_gap;
        let mut any_lost;
        let mut route_converged;
        let mut round = 0_usize;
        loop {
            load_out.iter_mut().for_each(|value| *value = 0.0);
            incoming = 0.0;
            placed = 0.0;
            lost = 0.0;
            spread_hits = 0_u64;
            max_hits = 0_u64;
            injected_here = 0.0_f64;
            worst_gap = 0.0;
            any_lost = false;
            let route = route_within_cover(
                grid,
                &cover_above,
                &carriers,
                &mut distance,
                &mut nearest,
                &mut distance_prev,
            );
            route_converged = route.converged;
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
                spread_hits += 1;
                let target = if any_root { nearest[cell] } else { u32::MAX };
                let reachable = target != u32::MAX
                    && f64::from(distance[cell]) <= sizing.lamina_reach_mm + grid.pitch_mm();
                if reachable {
                    // Nearest first, share by area. One raster cell of carrier
                    // has exactly the source cell's own area, so the
                    // accumulation stops at the first carrier and its weight is
                    // `area / area`. The division is kept explicit because it
                    // is the renormalisation sabotage S3 removes.
                    let accumulated = area;
                    let weight = area / accumulated;
                    load_out[target as usize] += load * weight;
                    placed += load * weight;
                } else {
                    lost += load;
                    any_lost = true;
                    let gap = if target == u32::MAX {
                        f64::INFINITY
                    } else {
                        f64::from(distance[cell])
                    };
                    if gap > worst_gap {
                        worst_gap = gap;
                    }
                }
            }

            // --- this layer's own peel event, and the maximum ---------------
            //
            // What the max ADDS over what arrived is this layer's newly
            // injected load, and its running total is the conservation quantity
            // every oracle grades against: everything injected must arrive
            // somewhere, and nothing else ever enters the system. On a
            // flat-bottomed model only the topmost layer of each column
            // injects, so the total is that model's largest cross-section and
            // this is §C2's identity verbatim. On a SLOPED underside each layer
            // births a new crescent that injects too, so the total is the
            // model's XY silhouette instead — larger than the peak layer, and
            // correctly so, because those are different peel events carried by
            // different arms.
            //
            // The injection is counted only where the column did NOT carry one
            // layer up, which is the derivation's own words — "a column injects
            // its own event exactly once, at the top of each contiguous cured
            // run". For MODEL material the gate changes nothing and cannot: a
            // column that carried above arrives with at least `own`, so
            // `combined - load_out` is already zero there. For an ARM cell it
            // changes everything, and that is the accounting defect it fixes:
            // an arm ZEROES its own cell every layer when it reads its share,
            // so without the gate the cell looks freshly born on the next layer
            // down and injects `own` again, once per layer, for the whole
            // length of the arm. `max` then discards all but one of those, so
            // they never reach the plate — and `injected_load_mm2`, which the
            // oracles grade the plate reaction and the scar against, was
            // inflated by every one of them. On `oracle_ramp` the inflation was
            // 708.99 mm2 against a 20-degree-tilted Round 25's XY silhouette of
            // about 491 mm2, and it grew with the arm count, which is exactly
            // the wrong direction for a conservation total.
            let own = area * sizing.alpha;
            for cell in 0..cells {
                if carriers[cell] {
                    let combined = load_out[cell].max(own);
                    if !carriers_above[cell] {
                        injected_here += combined - load_out[cell];
                    }
                    load_out[cell] = combined;
                    max_hits += 1;
                } else {
                    load_out[cell] = 0.0;
                }
            }

            if !insertion.enabled {
                break;
            }
            // Counted HERE, after the enable gate, so it is a witness that the
            // area law was consulted on this layer and not merely that the
            // down-sweep looped.
            outcome.insertion_rounds += 1;
            insert.hosts.clear();
            for &id in &alive_by_layer[layer] {
                insert.hosts.push(struts[id as usize].cell);
            }
            for &index in &growing {
                insert.hosts.push(struts[index].cell);
            }
            if !insert.hosts.iter().any(|&cell| load_out[cell] > l_max) {
                break;
            }
            if round >= insertion.max_rounds {
                // Rounds-exhausted: the max_rounds BOUND was hit, not the
                // geometry. Whether more rounds would have found a footprint
                // is unknown -- this is reported separately from a genuine
                // geometry exhaustion so the two are never conflated.
                outcome.insertion_rounds_exhausted += 1;
                break;
            }
            let added = insert_for_load(
                grid,
                layer,
                keep_out,
                &masks.cover,
                &cover_above,
                &mut carriers,
                &load_in,
                &load_out,
                &nearest,
                &distance,
                l_max,
                area,
                insertion.max_struts,
                struts,
                &mut growing,
                &mut pending,
                &mut insert,
            );
            if added == 0 {
                // Geometry-exhausted: the area law wants more section here and
                // the search found nowhere to put it -- a physical limit, not
                // the round bound above. Emit and warn: the arms that remain
                // over the limit are named one by one in `overloaded`.
                outcome.insertion_geometry_exhausted += 1;
                break;
            }
            outcome.arms_inserted_by_load += added;
            round += 1;
        }
        if !route_converged {
            outcome.route_not_converged += 1;
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
        outcome.spread_search_hits += spread_hits;
        outcome.max_combine_hits += max_hits;
        outcome.injected_load_mm2 += injected_here;
        if any_lost {
            outcome.uncarried_lowest_layer = Some(layer);
            if worst_gap > outcome.uncarried_worst_gap_mm {
                outcome.uncarried_worst_gap_mm = worst_gap;
            }
        }

        // --- struts are columns; terminations first so a parent sees its
        //     children's load before it reads its own share -------------------
        //
        // A growing arm's ledger is written top-down while it grows, so its
        // slot counts DOWN from the seat and is reversed into the
        // `k_bottom`-upward layout every other reader expects the moment it
        // lands.
        layer_arms.clear();
        for &index in growing.iter().rev() {
            let strut = &struts[index];
            let cell = strut.cell;
            let slot = strut.k_top - 1 - layer;
            let (terminating, landing) = if masks.carry[cell] {
                (true, Landing::Model { layer })
            } else if landing_target[cell] != u32::MAX {
                (
                    true,
                    Landing::Strut {
                        id: landing_target[cell],
                        layer,
                    },
                )
            } else if layer == 0 {
                (true, Landing::Plate)
            } else {
                (false, Landing::Plate)
            };
            layer_arms.push(LayerArm {
                index,
                slot,
                previous_slot: slot.checked_sub(1),
                terminating,
                landing,
                is_growing: true,
            });
        }
        for &id in &alive_by_layer[layer] {
            let index = id as usize;
            let strut = &struts[index];
            layer_arms.push(LayerArm {
                index,
                slot: layer - strut.k_bottom,
                previous_slot: Some(layer - strut.k_bottom + 1),
                terminating: strut.k_bottom == layer,
                landing: strut.landing,
                is_growing: false,
            });
        }
        for pass in 0..2 {
            for arm_order in 0..layer_arms.len() {
                let arm = layer_arms[arm_order];
                let index = arm.index;
                if (pass == 0) != arm.terminating {
                    continue;
                }
                let cell = struts[index].cell;
                let share = load_out[cell] + pending[index];
                pending[index] = 0.0;
                load_out[cell] = 0.0;
                outcome.strut_share_reads += 1;
                let slot = arm.slot;
                if struts[index].carried_mm2.len() <= slot {
                    struts[index].carried_mm2.resize(slot + 1, 0.0);
                    struts[index].radius_mm.resize(slot + 1, 0.0);
                }
                let previous = arm
                    .previous_slot
                    .and_then(|previous| struts[index].carried_mm2.get(previous).copied())
                    .unwrap_or(0.0);
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
                if arm.terminating {
                    match arm.landing {
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
                    if arm.is_growing {
                        // The descent found its landing. Fix the arm's extent
                        // and flip its ledger into the `k_bottom`-upward
                        // layout, which is the layout every other reader —
                        // `tip_radius_mm`, `tip_load_mm2`, the adapter — has
                        // always assumed.
                        struts[index].k_bottom = layer;
                        struts[index].landing = arm.landing;
                        struts[index].carried_mm2.reverse();
                        struts[index].radius_mm.reverse();
                    }
                }
            }
        }
        // Growing arms that landed leave the growing set, in one retain rather
        // than a per-arm removal.
        growing.retain(|&index| struts[index].k_bottom == usize::MAX);

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
        carriers_above.copy_from_slice(&carriers);
    }

    // Anything still in the raster at the bottom of the stack reached the
    // plate through model material rather than through an arm.
    outcome.plate_reaction_mm2 += load_in.iter().sum::<f64>();
    if !outcome.min_layer_area_mm2.is_finite() {
        outcome.min_layer_area_mm2 = 0.0;
    }
    outcome
}

/// One arm's participation in ONE layer, resolved once and then read twice
/// (terminations first, everything else second).
#[derive(Clone, Copy, Debug)]
struct LayerArm {
    index: usize,
    /// Index into the arm's own ledger. Counts UP from `k_bottom` for an arm
    /// whose extent is already fixed, and DOWN from the seat for one that is
    /// still growing; the ledger is reversed the moment it lands.
    slot: usize,
    previous_slot: Option<usize>,
    terminating: bool,
    landing: Landing,
    is_growing: bool,
}

/// Per-layer scratch for the load-driven insertion, allocated once per run.
struct InsertScratch {
    /// Cells of the arms alive at this layer, in the order they are read.
    hosts: Vec<usize>,
    /// `host_slot[cell]` is the index into `hosts` of the arm sitting in that
    /// cell, or `u32::MAX`. Lets the footprints of EVERY overloaded arm be
    /// collected in ONE pass over the raster instead of one pass per arm.
    host_slot: Vec<u32>,
    buckets: Vec<Vec<usize>>,
    accepted: Vec<usize>,
}

impl InsertScratch {
    fn new(cells: usize) -> Self {
        Self {
            hosts: Vec::new(),
            host_slot: vec![u32::MAX; cells],
            buckets: Vec::new(),
            accepted: Vec::new(),
        }
    }
}

/// Put in the carriers the AREA LAW asks for, where reach never had a reason to.
///
/// # The trigger is the law, not a threshold
///
/// An arm is relieved exactly when the load ROUTED to it at this layer exceeds
/// `ℓ_max = X_ALLOWED · π · r_max²`, the largest peel one tip may legally
/// carry. That is the registered area law read at the registered maximum
/// radius and nothing else; there is no tuned number here to move.
///
/// Only the ROUTED part is relievable, and that is why it is the trigger rather
/// than the arm's total carried load. An arm can also be over the limit because
/// of what it inherited from higher layers (`previous`) or because a child arm
/// landed on it (`pending`); neither of those is load this layer's geometry can
/// redistribute, and inserting siblings for them would add arms that carry
/// nothing. Inherited overload is relieved where it was born — at the highest
/// layer where the arm's own routed share went over, which this pass reaches
/// first because it walks DOWNWARD.
///
/// # How many, and where
///
/// `n = ceil(routed / ℓ_max)` carriers are wanted where there is now one, so
/// `n − 1` go in. They are chosen from the arm's own routed footprint — the
/// cells whose load the routing field currently sends to this arm — because a
/// carrier anywhere else cannot take that load. The footprint is further
/// restricted to cells that are a genuine down-face at this layer
/// (`cover_above && !cover_here`), so the tip seats against cured material
/// above it and stands in free space itself, which is what every arm the
/// up-sweep places also does.
///
/// Spacing is a hex lattice, and its pitch is DERIVED rather than borrowed:
/// `n` carriers spread over the footprint's own area `A` sit at the hex
/// nearest-neighbour spacing `sqrt(2A / (√3 n))`, because a hex cell of
/// spacing `s` has area `√3 s² / 2`. Equivalently it is the spacing at which
/// one arm carries exactly `ℓ_max` at this footprint's measured load density,
/// so it is the area law again in the plane rather than a second rule. Using
/// the UP-sweep's reach spacing here would be wrong in the only direction that
/// matters: reach spacing is fixed at `√3 ρ ≈ 3.4 mm` and every footprint cell
/// is by construction within one lamina reach `ρ = 2.13 mm` of the arm, so a
/// reach-spaced rule can never place a single sibling.
///
/// Candidates are ordered by routed load, then by distance from the arm being
/// relieved (farthest first — the load that has travelled furthest is the load
/// a new carrier most surely takes), then by cell index, so the choice is
/// deterministic and does not depend on floating point beyond the two
/// quantities it is actually about.
#[expect(
    clippy::too_many_arguments,
    reason = "one layer's insertion genuinely reads the grid, its layer index, two cover masks, the \
          carrier mask it updates, the load raster on both sides of the spread, the routing \
          field's two outputs, the area law's limit, the cell area and the strut cap, and writes \
          the strut list, the growing set and the pending ledger; a struct would move the \
          argument list rather than shorten it"
)]
fn insert_for_load(
    grid: &Grid,
    layer: usize,
    keep_out: &DrainKeepOut,
    cover_here: &[bool],
    cover_above: &[bool],
    carriers: &mut [bool],
    load_in: &[f64],
    load_out: &[f64],
    nearest: &[u32],
    distance: &[f32],
    l_max: f64,
    cell_area_mm2: f64,
    max_struts: usize,
    struts: &mut Vec<SweepStrut>,
    growing: &mut Vec<usize>,
    pending: &mut Vec<f64>,
    scratch: &mut InsertScratch,
) -> u64 {
    if l_max <= 0.0 {
        return 0;
    }
    // One pass to index the hosts, one pass over the raster to bucket every
    // overloaded arm's footprint at once. No per-arm scan of the grid.
    scratch.buckets.clear();
    scratch.buckets.resize(scratch.hosts.len(), Vec::new());
    let mut any_overloaded = false;
    for (slot, &cell) in scratch.hosts.iter().enumerate() {
        if load_out[cell] > l_max {
            scratch.host_slot[cell] = u32::try_from(slot).unwrap_or(u32::MAX);
            any_overloaded = true;
        }
    }
    if !any_overloaded {
        return 0;
    }
    for cell in 0..grid.cells() {
        if load_in[cell] <= 0.0 || carriers[cell] || !cover_above[cell] || cover_here[cell] {
            continue;
        }
        // The area law does not get to overrule the drain. An arm the load
        // wants here would stand in, or descend through, the cylinder the resin
        // leaves by; the overload it would have relieved is reported instead
        // (`overloaded`), which is the emit-and-warn outcome.
        if !keep_out.admits(cell, layer) {
            continue;
        }
        let target = nearest[cell];
        if target == u32::MAX {
            continue;
        }
        let slot = scratch.host_slot[target as usize];
        if slot == u32::MAX {
            continue;
        }
        scratch.buckets[slot as usize].push(cell);
    }

    let mut inserted = 0_u64;
    for slot in 0..scratch.hosts.len() {
        let host_cell = scratch.hosts[slot];
        if scratch.host_slot[host_cell] == u32::MAX {
            continue;
        }
        scratch.host_slot[host_cell] = u32::MAX;
        let routed = load_out[host_cell];
        let footprint = core::mem::take(&mut scratch.buckets[slot]);
        if footprint.is_empty() {
            continue;
        }
        let wanted = (routed / l_max).ceil().max(1.0);
        #[expect(
            clippy::cast_sign_loss,
            reason = "a ratio of two positive loads, ceil-ed and floored at 1"
        )]
        let wanted_carriers = wanted as usize;
        let extra = wanted_carriers.saturating_sub(1).min(footprint.len());
        if extra == 0 {
            continue;
        }
        let footprint_area = footprint.len() as f64 * cell_area_mm2;
        let separation_mm = (2.0 * footprint_area
            / (3.0_f64.sqrt() * wanted))
            .sqrt()
            .max(grid.pitch_mm());
        let mut order = footprint;
        order.sort_unstable_by(|&a, &b| {
            load_in[b]
                .total_cmp(&load_in[a])
                .then_with(|| distance[b].total_cmp(&distance[a]))
                .then_with(|| a.cmp(&b))
        });
        scratch.accepted.clear();
        for &cell in &order {
            if scratch.accepted.len() >= extra || struts.len() >= max_struts {
                break;
            }
            if crate::chamfer_distance_mm(grid, cell, host_cell) < separation_mm {
                continue;
            }
            if scratch
                .accepted
                .iter()
                .any(|&taken| crate::chamfer_distance_mm(grid, cell, taken) < separation_mm)
            {
                continue;
            }
            scratch.accepted.push(cell);
        }
        for &cell in &scratch.accepted {
            let id = u32::try_from(struts.len()).unwrap_or(u32::MAX);
            struts.push(SweepStrut {
                id,
                cell,
                center_mm: [
                    grid.center_x_mm(grid.col_of(cell)),
                    grid.center_y_mm(grid.row_of(cell)),
                ],
                k_top: layer + 1,
                // Still growing: the descent decides where this stops, the
                // same way `up_sweep::descend` does, and until it does there is
                // no honest value to put here.
                k_bottom: usize::MAX,
                landing: Landing::Plate,
                interior: false,
                radius_mm: Vec::new(),
                carried_mm2: Vec::new(),
                // Placed by the area law, not by reach: it roots no demand
                // cell, and the removal pass must not be told otherwise.
                covered_demand: Vec::new(),
                inserted_by_load: true,
            });
            growing.push(struts.len() - 1);
            pending.push(0.0);
            carriers[cell] = true;
            inserted += 1;
        }
    }
    // Leave the index clean for the next round.
    for &cell in &scratch.hosts {
        scratch.host_slot[cell] = u32::MAX;
    }
    inserted
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
