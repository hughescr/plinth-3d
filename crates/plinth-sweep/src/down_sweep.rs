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
//! routed through it, not the sum of all of them. Replacing the max with a sum
//! is sabotage S7, and on the cavity box it turns a 1600 mm² plate reaction
//! into 2756 mm² — a 72 % lie, which is why that oracle exists.
//!
//! # The raster routes; the per-event pass sizes (rule a′, 2026-09-29)
//!
//! This pass collapses the history into ONE max-field raster before spreading.
//! That is conservative (`spread(max(a,b)) ≥ max(spread(a), spread(b))`), and
//! on a body whose load walks down its own flanks to one line it is badly
//! so: the per-column maxima of events that never peel together are SUMMED
//! where they meet, and the Cylinder's lowest-line arm was charged 41.71 mm²
//! where no single event puts more than 14.70 mm² on it. So the raster is kept
//! for what it decides well — where each cell's load goes, which footprint an
//! inserted arm comes from, and the whole-run accounting identities — and the
//! SIZES come from [`crate::per_event`], which follows the routing this pass
//! records one event at a time and takes the maximum only after.
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

use std::collections::{HashMap, HashSet};

use plinth_support::perception::SliceSolid;

use crate::drain::DrainKeepOut;
use crate::grid::Grid;
use crate::layer_cache::{BytePool, Components, LayerCache};
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
    /// Injected peel carried UNDER A LARGER EVENT in the same arm: at a read
    /// where the load was put into the raster, `max` keeps the larger of the
    /// arm's own events and the smaller never reaches the plate reaction. Not
    /// lost — the arm is sized for the larger, and the two peel at different
    /// instants — but a term of the whole-run identity,
    /// `injected = plate + uncarried + absorbed + (load that went nowhere)`.
    /// Per read it is `previous + fed - max(previous, own)`, where `fed` is
    /// `own` when that load was put into the raster and zero when it is the
    /// arm's own non-injected per-layer duplicate; a duplicate that out-weighs
    /// everything the arm carried from above makes the term negative.
    /// Measured 2026-09-29: the Cylinder 1.4625 mm² over 8 reads, the Sphere
    /// 0.4950 over 6, `make_laid_tube` 3.85 over 14 (3 of them negative), and
    /// in each case the whole of `injected - plate - uncarried`.
    pub absorbed_event_load_mm2: f64,
    /// Reads with a non-zero absorbed term. The path-hit witness.
    pub absorbed_event_reads: u64,
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
    /// Straight sightlines the routing field priced and found clear, and those
    /// that crossed empty space and bent instead (`reach::any_angle_relax`).
    /// The path-hit witnesses that load travels on the same straight-line
    /// metric the reach potential measures, and still goes round a hole.
    pub route_sightlines_clear: u64,
    pub route_sightlines_blocked: u64,
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
    /// Layers where the child-before-trunk ordering could not be topologically
    /// sorted. Physically impossible (a member cannot land on itself through a
    /// chain); reported rather than asserted so a defect here is visible in a
    /// run report instead of taking the process down.
    pub merge_order_cycles: u64,
    /// Reads where a lander's load arrived at a support arm's CONTACT slot
    /// (its top ledger slot, the one `tip_radius_mm` sizes the contact ball
    /// from) and was booked one slot lower, into the shaft below the contact
    /// (2026-09-29). The path-hit witness for that rule: zero on a field where
    /// no arm lands on another arm's head.
    pub contact_merges_moved_below: u64,
    /// The load those reads moved, in mm². It still reaches the plate: the
    /// shaft below the contact carries it.
    pub contact_merge_load_moved_mm2: f64,
    /// Loads that landed on a HEADED arm between its contact and its joint and
    /// were held until the arm's joint layer, so the tip and the head slots
    /// stay sized for the arm's own load (v19 fix, 2026-10-08), and the load
    /// they moved, in mm². The path-hit witness for that hold: zero on a field
    /// where no member joined a head span.
    pub head_span_merges_held_to_joint: u64,
    pub head_span_load_held_mm2: f64,
    /// Reads where a lander's load arrived at the contact slot of an arm that
    /// has NO slot below it (a one-layer arm that terminates at its own tip
    /// layer). There is no shaft under the contact to take the load, so it
    /// stays in the one slot, as before 2026-09-29, and is counted here
    /// rather than dropped.
    pub contact_merges_kept_at_single_slot: u64,
    /// Times the area-law trigger was evaluated. With per-event sizing: once
    /// per per-event pass (every pass reads every member's per-event routed
    /// load against `ℓ_max`); under the max-field sabotage, once per layer. A
    /// path-hit counter: without it, `arms_inserted_by_load == 0` cannot be
    /// told apart from an insertion step that never ran.
    pub insertion_rounds: u64,
    /// `(layer, cell)` pairs where a LEANED arm's own axis one layer up was
    /// offered to the routing field as a root, i.e. where the arm stands in a
    /// different column here than it does over the material it holds. Zero on a
    /// field with no leaned arm, so it tells "no arm leaned" apart from "the
    /// tip-column rule never ran".
    pub tip_column_roots_offered: u64,
    /// Load, in mm², whose nearest root was one of those tip columns. An UPPER
    /// bound on what the seat-blind accounting lost: some of it would have
    /// reached a further carrier instead, and the rest had no carrier at all.
    /// Zero is the honest reading "the rule changed no routing", which is what
    /// tells an inert rule apart from an absent one.
    pub tip_column_root_load_mm2: f64,
    /// Load, in mm², that the routing field deposited on a column holding no
    /// carrier. Always a defect in the roots offered, never a physical
    /// outcome; it is included in `uncarried_load_mm2` and named here so a
    /// wrong root can never read as conserved.
    pub misrouted_load_mm2: f64,
    /// What the per-event sizing measured on the field this call left
    /// ([`crate::per_event`]). Default (disabled) under the sabotage knob.
    pub per_event: crate::per_event::PerEventReport,
    /// The routing the last pass of this call recorded, which the per-event
    /// sizing followed. Kept so a diagnostic on the finished field
    /// ([`crate::rigid_share`]) follows the same paths. Empty under the
    /// sabotage knob.
    pub route_log: crate::per_event::RouteLog,
    /// Per member (strut index): whether any peel event's load reaches it.
    /// A member no event reaches is no load's nearest carrier. Empty under the
    /// sabotage knob.
    pub reached_by_event: Vec<bool>,
    /// `(layer, column)` pairs where a model column the up-sweep left
    /// UNROOTED was refused as a routing root ([`UnrootedColumns`]). The
    /// path-hit witness for the unrooted-column rule: zero on a field the
    /// up-sweep rooted everywhere, where the rule cannot change anything.
    pub unrooted_columns_refused: u64,
    /// Load, in mm², that entered an unrooted column and went SIDEWAYS to a
    /// rooted carrier in reach instead of down that column.
    pub unrooted_load_rerouted_mm2: f64,
    /// Load, in mm², that entered an unrooted column with no rooted carrier in
    /// reach, and so went down its own column as it always did.
    pub unrooted_load_kept_mm2: f64,
    /// Reads where an unrooted column whose carried load had been sent
    /// sideways did NOT take its own layer's peel back into the raster
    /// ([`UnrootedColumns`], "the ledger"). The path-hit witness for that
    /// ledger rule.
    pub handed_columns_held: u64,
    /// The load those reads kept out of the raster, in mm²: what the
    /// max-field ledger would otherwise have re-added to a re-routed column at
    /// every layer without ever counting it as injected.
    pub handed_own_withheld_mm2: f64,
    /// Held events whose next step was NOT sideways (the column is rooted
    /// there, has no rooted carrier in reach, or ends), so they re-entered
    /// the raster one layer down, counted as injected peel.
    pub handed_events_reentered: u64,
    /// The peel those re-entries put back into the raster, mm². Part of
    /// `injected_load_mm2`.
    pub handed_reentered_mm2: f64,
    /// The triggers the geometry refused, as `(per_event::entry(member,
    /// layer), limit bits)`: the area-law limit the search was asked to relieve
    /// them above. Sorted. Includes the refusals a seed carried in, so the next
    /// run on the same field inherits all of them ([`RunSeed`]). A trigger is
    /// carried only against the very limit it was refused at: a refusal at the
    /// area law's `ℓ_max` says nothing about the same tip under a contact cap.
    pub refused: Vec<(u64, u64)>,
    /// Which trigger placed each arm this call inserted: `(arm id,
    /// per_event::entry(host, layer))`, in insertion order. Empty under the
    /// sabotage knob (its trigger is no per-event entry; it reads `u64::MAX`).
    pub inserted_by: Vec<(u32, u64)>,
    /// The digest of the field this call left and of everything its first
    /// pass reads ([`GeometryDigest`]). `None` under the sabotage knob.
    pub geometry_digest: Option<GeometryDigest>,
    /// The accounting of the pass that logged [`Self::route_log`], for a
    /// seeded run to take its pass 1 from ([`RunSeed`]). `None` under the
    /// sabotage knob.
    pub accounting: Option<DownSweepAccounting>,
    /// Whether this call took its pass 1 from a [`RunSeed`] instead of
    /// walking the layers. The path-hit witness for the seeded pass.
    pub seeded_pass: bool,
    /// Whether a seed was offered and refused (stale digest, no accounting
    /// snapshot or no log), so pass 1 walked the layers as it always did.
    pub seed_stale: bool,
    /// Triggers a carried refusal kept this call from re-trying.
    pub refusals_carried_skipped: u64,
}

/// What the layers' first pass reads, so a seed can be told from a stale one:
/// the layer cache's key (grid and every slice coordinate), and a digest of
/// the members (kind, extent, landing, joint layer, the cell of every layer
/// they stand on), of the unrooted columns and of the sizing fields the pass
/// reads. A radius, a minimum radius or a thickened layer is NOT in it: pass 1
/// routes and counts without them, and the per-event rewrite applies the
/// caller's own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometryDigest {
    layers: crate::layer_cache::LayerKey,
    field: u64,
}

fn mix(hash: u64, word: u64) -> u64 {
    (hash.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95)
}

/// The digest of the field and routing inputs pass 1 reads. `O(sum of the
/// members' layers)`, a few hundred thousand mixes on the largest model.
fn field_digest(
    layers: &LayerCache<'_>,
    struts: &[SweepStrut],
    sizing: SizingPolicy,
    unrooted: &UnrootedColumns,
) -> GeometryDigest {
    let layer_count = layers.slices().len();
    let mut hash = mix(0, struts.len() as u64);
    for strut in struts {
        hash = mix(hash, u64::from(strut.id));
        hash = mix(hash, strut.kind as u64);
        hash = mix(hash, strut.cell as u64);
        hash = mix(hash, strut.k_top as u64);
        hash = mix(hash, strut.k_bottom as u64);
        hash = match strut.landing {
            Landing::Plate => mix(hash, 1),
            Landing::Model { layer } => mix(mix(hash, 2), layer as u64),
            Landing::Strut { id, layer } => mix(mix(mix(hash, 3), u64::from(id)), layer as u64),
        };
        hash = mix(hash, strut.joint_hold_layer().map_or(u64::MAX, |layer| layer as u64));
        if strut.k_bottom != usize::MAX {
            // The cell at `layer + 1` is read too, for the top layer.
            for layer in strut.k_bottom..=strut.k_top.min(layer_count) {
                hash = mix(hash, strut.cell_at(layer) as u64);
            }
        }
    }
    for layer in 0..layer_count {
        let cells = unrooted.at(layer);
        hash = mix(hash, cells.len() as u64);
        for &cell in cells {
            hash = mix(hash, u64::from(cell));
        }
    }
    for value in [sizing.x_allowed, sizing.alpha, sizing.lamina_reach_mm] {
        hash = mix(hash, value.to_bits());
    }
    for flag in [
        sizing.straight_reach,
        sizing.tip_column_roots,
        sizing.handed_column_ledger,
        sizing.per_event,
    ] {
        hash = mix(hash, u64::from(flag));
    }
    GeometryDigest {
        layers: *layers.key(),
        field: hash,
    }
}

/// The accounting of one top-down pass: every field of [`DownSweepOutcome`]
/// that pass 1 computes and that is not recomputed by the per-event rewrite
/// that follows it, plus the max-field ledger's worst load (which pass 1
/// reads from the members before the rewrite overwrites it).
///
/// Taken from the pass that logged the route ([`RunSeed`]); a seeded pass 1
/// puts it back. The destructuring in [`Self::of`] names every field of the
/// outcome without `..`, so a new field fails to compile until it is classed
/// as a snapshot field or as recomputed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DownSweepAccounting {
    plate_reaction_mm2: f64,
    uncarried_load_mm2: f64,
    injected_load_mm2: f64,
    absorbed_event_load_mm2: f64,
    absorbed_event_reads: u64,
    peak_layer_area_mm2: f64,
    model_section_max_x: f64,
    min_layer_area_mm2: f64,
    conservation_residual: f64,
    model_overload: Vec<ModelSectionOverload>,
    spread_search_hits: u64,
    strut_share_reads: u64,
    uncarried_lowest_layer: Option<usize>,
    uncarried_worst_gap_mm: f64,
    landing_injections: u64,
    max_combine_hits: u64,
    rasteriser_entries: u64,
    route_not_converged: u64,
    route_sightlines_clear: u64,
    route_sightlines_blocked: u64,
    merge_order_cycles: u64,
    contact_merges_moved_below: u64,
    contact_merge_load_moved_mm2: f64,
    head_span_merges_held_to_joint: u64,
    head_span_load_held_mm2: f64,
    contact_merges_kept_at_single_slot: u64,
    tip_column_roots_offered: u64,
    tip_column_root_load_mm2: f64,
    misrouted_load_mm2: f64,
    unrooted_columns_refused: u64,
    unrooted_load_rerouted_mm2: f64,
    unrooted_load_kept_mm2: f64,
    handed_columns_held: u64,
    handed_own_withheld_mm2: f64,
    handed_events_reentered: u64,
    handed_reentered_mm2: f64,
    /// The largest ledger load any member held after the pass, before the
    /// per-event rewrite.
    worst_max_field_mm2: f64,
}

impl DownSweepAccounting {
    /// The snapshot of `outcome`, whose `per_event.worst_member_max_field_mm2`
    /// must already hold the pass's max-field worst load.
    fn of(outcome: &DownSweepOutcome) -> Self {
        let DownSweepOutcome {
            plate_reaction_mm2,
            uncarried_load_mm2,
            injected_load_mm2,
            absorbed_event_load_mm2,
            absorbed_event_reads,
            peak_layer_area_mm2,
            model_section_max_x,
            min_layer_area_mm2,
            conservation_residual,
            overloaded: _,
            model_overload,
            spread_search_hits,
            strut_share_reads,
            uncarried_lowest_layer,
            uncarried_worst_gap_mm,
            landing_injections,
            max_combine_hits,
            rasteriser_entries,
            route_not_converged,
            route_sightlines_clear,
            route_sightlines_blocked,
            arms_inserted_by_load: _,
            insertion_rounds_exhausted: _,
            insertion_geometry_exhausted: _,
            merge_order_cycles,
            contact_merges_moved_below,
            contact_merge_load_moved_mm2,
            head_span_merges_held_to_joint,
            head_span_load_held_mm2,
            contact_merges_kept_at_single_slot,
            insertion_rounds: _,
            tip_column_roots_offered,
            tip_column_root_load_mm2,
            misrouted_load_mm2,
            per_event,
            route_log: _,
            reached_by_event: _,
            unrooted_columns_refused,
            unrooted_load_rerouted_mm2,
            unrooted_load_kept_mm2,
            handed_columns_held,
            handed_own_withheld_mm2,
            handed_events_reentered,
            handed_reentered_mm2,
            refused: _,
            inserted_by: _,
            geometry_digest: _,
            accounting: _,
            seeded_pass: _,
            seed_stale: _,
            refusals_carried_skipped: _,
        } = outcome;
        Self {
            plate_reaction_mm2: *plate_reaction_mm2,
            uncarried_load_mm2: *uncarried_load_mm2,
            injected_load_mm2: *injected_load_mm2,
            absorbed_event_load_mm2: *absorbed_event_load_mm2,
            absorbed_event_reads: *absorbed_event_reads,
            peak_layer_area_mm2: *peak_layer_area_mm2,
            model_section_max_x: *model_section_max_x,
            min_layer_area_mm2: *min_layer_area_mm2,
            conservation_residual: *conservation_residual,
            model_overload: model_overload.clone(),
            spread_search_hits: *spread_search_hits,
            strut_share_reads: *strut_share_reads,
            uncarried_lowest_layer: *uncarried_lowest_layer,
            uncarried_worst_gap_mm: *uncarried_worst_gap_mm,
            landing_injections: *landing_injections,
            max_combine_hits: *max_combine_hits,
            rasteriser_entries: *rasteriser_entries,
            route_not_converged: *route_not_converged,
            route_sightlines_clear: *route_sightlines_clear,
            route_sightlines_blocked: *route_sightlines_blocked,
            merge_order_cycles: *merge_order_cycles,
            contact_merges_moved_below: *contact_merges_moved_below,
            contact_merge_load_moved_mm2: *contact_merge_load_moved_mm2,
            head_span_merges_held_to_joint: *head_span_merges_held_to_joint,
            head_span_load_held_mm2: *head_span_load_held_mm2,
            contact_merges_kept_at_single_slot: *contact_merges_kept_at_single_slot,
            tip_column_roots_offered: *tip_column_roots_offered,
            tip_column_root_load_mm2: *tip_column_root_load_mm2,
            misrouted_load_mm2: *misrouted_load_mm2,
            unrooted_columns_refused: *unrooted_columns_refused,
            unrooted_load_rerouted_mm2: *unrooted_load_rerouted_mm2,
            unrooted_load_kept_mm2: *unrooted_load_kept_mm2,
            handed_columns_held: *handed_columns_held,
            handed_own_withheld_mm2: *handed_own_withheld_mm2,
            handed_events_reentered: *handed_events_reentered,
            handed_reentered_mm2: *handed_reentered_mm2,
            worst_max_field_mm2: per_event.worst_member_max_field_mm2,
        }
    }

    /// Write the snapshot's fields into a fresh pass-1 outcome.
    fn to_outcome(&self) -> DownSweepOutcome {
        DownSweepOutcome {
            plate_reaction_mm2: self.plate_reaction_mm2,
            uncarried_load_mm2: self.uncarried_load_mm2,
            injected_load_mm2: self.injected_load_mm2,
            absorbed_event_load_mm2: self.absorbed_event_load_mm2,
            absorbed_event_reads: self.absorbed_event_reads,
            peak_layer_area_mm2: self.peak_layer_area_mm2,
            model_section_max_x: self.model_section_max_x,
            min_layer_area_mm2: self.min_layer_area_mm2,
            conservation_residual: self.conservation_residual,
            model_overload: self.model_overload.clone(),
            spread_search_hits: self.spread_search_hits,
            strut_share_reads: self.strut_share_reads,
            uncarried_lowest_layer: self.uncarried_lowest_layer,
            uncarried_worst_gap_mm: self.uncarried_worst_gap_mm,
            landing_injections: self.landing_injections,
            max_combine_hits: self.max_combine_hits,
            rasteriser_entries: self.rasteriser_entries,
            route_not_converged: self.route_not_converged,
            route_sightlines_clear: self.route_sightlines_clear,
            route_sightlines_blocked: self.route_sightlines_blocked,
            merge_order_cycles: self.merge_order_cycles,
            contact_merges_moved_below: self.contact_merges_moved_below,
            contact_merge_load_moved_mm2: self.contact_merge_load_moved_mm2,
            head_span_merges_held_to_joint: self.head_span_merges_held_to_joint,
            head_span_load_held_mm2: self.head_span_load_held_mm2,
            contact_merges_kept_at_single_slot: self.contact_merges_kept_at_single_slot,
            tip_column_roots_offered: self.tip_column_roots_offered,
            tip_column_root_load_mm2: self.tip_column_root_load_mm2,
            misrouted_load_mm2: self.misrouted_load_mm2,
            unrooted_columns_refused: self.unrooted_columns_refused,
            unrooted_load_rerouted_mm2: self.unrooted_load_rerouted_mm2,
            unrooted_load_kept_mm2: self.unrooted_load_kept_mm2,
            handed_columns_held: self.handed_columns_held,
            handed_own_withheld_mm2: self.handed_own_withheld_mm2,
            handed_events_reentered: self.handed_events_reentered,
            handed_reentered_mm2: self.handed_reentered_mm2,
            ..DownSweepOutcome::default()
        }
    }
}

/// What a finished call hands to the next call on the same field, so that
/// call's first pass need not walk the layers again (SC-2).
///
/// * `log` — the routing the finished call's last pass recorded.
/// * `accounting` — that pass's [`DownSweepAccounting`]. The log holds hops and
///   reads only; the outcome's scalar accounting (loads, overloads, route
///   counters) is not derivable from it, so it travels with it.
/// * `refused` — the triggers the geometry refused, with the limit each was
///   refused at.
/// * `geometry_digest` — what the log and the accounting are valid for.
///
/// A seed whose digest does not equal the digest of the field it is offered to,
/// or that holds no accounting or no log, is STALE: the call walks pass 1 for
/// real and says so (`DownSweepOutcome::seed_stale`). A seeded pass 1 is equal
/// by bits, whole outcome and every member, to the pass 1 it replaces.
///
/// A call whose last pass PLACED arms leaves no accounting: that pass read its
/// growing arms first in each layer, so its log orders the reads differently
/// from a walk of the finished field, and the per-event sizing takes the first
/// reader of a cell. Its seed is stale by construction and the next call walks.
#[derive(Clone, Debug)]
pub struct RunSeed {
    pub log: crate::per_event::RouteLog,
    pub accounting: Option<DownSweepAccounting>,
    pub refused: Vec<(u64, u64)>,
    pub geometry_digest: Option<GeometryDigest>,
}

impl DownSweepOutcome {
    /// The seed this call leaves for the next call on the same field, or
    /// `None` where it left no log (the sabotage knob).
    #[must_use]
    pub fn seed(&self) -> Option<RunSeed> {
        if self.route_log.is_empty() {
            return None;
        }
        Some(RunSeed {
            log: self.route_log.clone(),
            accounting: self.accounting.clone(),
            refused: self.refused.clone(),
            geometry_digest: self.geometry_digest,
        })
    }
}

/// `PLINTH_SWEEP_VERIFY_INCREMENTAL=1`: every seeded pass 1 is also walked for
/// real on a copy of the members and compared (log and accounting by bits);
/// any difference panics. For gate runs, not for production. Read once.
fn verify_incremental() -> bool {
    #[cfg(test)]
    if TEST_VERIFY.with(std::cell::Cell::get) {
        return true;
    }
    static VERIFY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *VERIFY.get_or_init(|| {
        std::env::var_os("PLINTH_SWEEP_VERIFY_INCREMENTAL").is_some_and(|value| value == "1")
    })
}

/// The model columns the up-sweep left UNROOTED, layer by layer, and the rule
/// the down-sweep applies to them (drain-rim routing, 2026-09-30).
///
/// # The disagreement this closes
///
/// The up-sweep roots a cell when a rooted carrier lies within reach of it
/// ([`crate::reach::ReachState::is_rooted`]). Where it cannot place one -- a
/// drain's keep-out, measured on the Sphere's drain rim -- it leaves the cell
/// UNROOTED and says so (`residual_demand`). The material ABOVE such a cell is
/// often still rooted, LATERALLY, from a ring of arms beside it. The
/// down-sweep used to ignore that: it sent a continuing model column's load
/// straight down whatever the column stood on, so peel load walked down the
/// shell and into the unrooted rim, where nothing carries it. On the Sphere
/// every mm² lost was on the same 100 rim cells, and it rose 376.26 -> 405.00
/// mm² when the arm rings moved one or two layers up (lattice-align, round 7,
/// "Blocker 2").
///
/// # The rule
///
/// One predicate, the up-sweep's own: a model column that is not rooted at
/// layer `k` is not a routing root at `k`. Load arriving in it from `k + 1`
/// goes sideways, through the cover it is in, to the nearest ROOTED carrier
/// within the same one-lamina bound every other routed cell obeys -- the
/// carrier the up-sweep's reach claimed that material for -- and only where
/// there is none does it go down its own column, exactly as before. An arm is
/// always a root, wherever it stands.
///
/// # The ledger
///
/// The max-field raster keeps, per carrier cell, the larger of what arrived
/// and the cell's own peel. In a continuing column that `max` is a no-op: the
/// column already carries at least its own peel from above. A column whose
/// carried load was just sent sideways arrives EMPTY, and the `max` then put
/// its own peel back -- an event the injection gate never counted, because the
/// column did not start there -- and the next layer sent it sideways again.
/// Every layer of a re-routed column re-added one cell of peel: built that way
/// the whole-run identity `injected = plate + uncarried + absorbed` missed by
/// -558 mm² on the Sphere. So a continuing model column whose load was handed
/// sideways holds its own peel OUT of the raster at that layer, and where the
/// held event goes is decided one layer down by the same rule:
///
/// * sideways again: the raster moves nothing (the larger event the column
///   carried already went to a rooted carrier, and this one is carried under
///   it, as in any continuing column), and the hop is recorded although the
///   raster holds nothing there, so the per-event pass ([`crate::per_event`]),
///   which routes and sizes every event on its own, sends it sideways too;
/// * anywhere else (the column is rooted there, has no rooted carrier in
///   reach, or ends): the event RE-ENTERS the raster, counted as injected, and
///   goes on as a cell's own load always did.
///
/// Holding it out for good also closes the identity, and lies the other way:
/// measured on a slab with a drain, the raster then read 0 mm² uncarried where
/// one single event lost 3.51 mm². With the re-entry the raster reads at least
/// what any one event loses (`tests/it/drain_rim_routing.rs`).
#[derive(Clone, Debug, Default)]
pub struct UnrootedColumns {
    by_layer: Vec<Vec<u32>>,
    total: usize,
}

impl UnrootedColumns {
    /// No unrooted column anywhere: the rule is off (and costs nothing).
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Room for `layers` layers, none recorded yet.
    #[must_use]
    pub fn with_layers(layers: usize) -> Self {
        Self {
            by_layer: vec![Vec::new(); layers],
            total: 0,
        }
    }

    /// Record, at `layer`, every covered cell the up-sweep's reach leaves
    /// unrooted. The predicate is [`crate::reach::ReachState::is_rooted`]
    /// itself, read once per cell on a layer that has any residual demand.
    pub fn record_layer(&mut self, layer: usize, cover: &[bool], reach: &crate::reach::ReachState) {
        let Some(cells) = self.by_layer.get_mut(layer) else {
            return;
        };
        self.total -= cells.len();
        cells.clear();
        for (cell, covered) in cover.iter().enumerate() {
            if *covered && !reach.is_rooted(cell) {
                cells.push(u32::try_from(cell).unwrap_or(u32::MAX));
            }
        }
        self.total += cells.len();
    }

    /// The cells recorded at `layer` (empty where none, or past the record).
    #[must_use]
    pub fn at(&self, layer: usize) -> &[u32] {
        self.by_layer.get(layer).map_or(&[], Vec::as_slice)
    }

    /// `(layer, cell)` entries recorded, over every layer.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.total
    }

    /// Whether nothing is recorded, i.e. the rule cannot change anything.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.total == 0
    }
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
pub struct InsertionPolicy<'a> {
    pub enabled: bool,
    pub max_struts: usize,
    /// Bounded rounds per layer. Each round computes the number of carriers
    /// the area law asks for and puts that many in at once, so one round
    /// normally suffices and further rounds only handle the case where the
    /// first round's own arms are themselves over the limit.
    pub max_rounds: usize,
    /// **The contact each tip could really make**, measured on a first run of
    /// this model (round 5 review, 2026-10-14): a tip whose load needs more
    /// contact than its feature gives is relieved at its tip layer by the same
    /// insertion that relieves an arm over the area law's maximum, so a second
    /// contact is placed beside it. `None` is the area law's limit alone.
    pub contact_caps: Option<&'a ContactCaps>,
}

/// The contact area (mm²) each seated tip of a first run really made, keyed by
/// the tip's seat column and seat layer, for a second run to relieve
/// ([`InsertionPolicy::contact_caps`]).
#[derive(Clone, Debug, Default)]
pub struct ContactCaps {
    by_tip: HashMap<(usize, usize), f64>,
}

impl ContactCaps {
    /// Record that the tip seated in column `cell` at layer `k_top` made
    /// `contact_mm2` of real contact.
    pub fn insert(&mut self, cell: usize, k_top: usize, contact_mm2: f64) {
        self.by_tip.insert((cell, k_top), contact_mm2);
    }

    /// Tips recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_tip.len()
    }

    /// Whether no tip is recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_tip.is_empty()
    }
}

impl InsertionPolicy<'_> {
    /// The most peel one event may route into `strut` at `layer` before it is
    /// relieved: the area law's `l_max`, and at a tip's own layer no more than
    /// the contact that tip really made carries (`contact × X_ALLOWED`).
    fn limit(&self, strut: &SweepStrut, layer: usize, l_max: f64, x_allowed: f64) -> f64 {
        match self.contact_caps {
            Some(caps) if layer + 1 == strut.k_top => caps
                .by_tip
                .get(&(strut.cell, strut.k_top))
                .map_or(l_max, |contact| l_max.min(contact * x_allowed)),
            _ => l_max,
        }
    }
}

impl InsertionPolicy<'static> {
    /// The production authority: insertion on, four rounds, the caller's own
    /// strut cap.
    #[must_use]
    pub const fn new(max_struts: usize) -> Self {
        Self {
            enabled: true,
            max_struts,
            max_rounds: 4,
            contact_caps: None,
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
            contact_caps: None,
        }
    }
}

impl<'a> InsertionPolicy<'a> {
    /// The same authority with a first run's tip contacts to relieve.
    #[must_use]
    pub const fn with_contact_caps(self, caps: &'a ContactCaps) -> Self {
        Self {
            contact_caps: Some(caps),
            ..self
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
    /// Whether a routed run is measured as its straight length, the same
    /// metric the reach potential uses. See
    /// [`crate::SweepPolicy::straight_reach`]; `false` is the sabotage form.
    pub straight_reach: bool,
    /// Whether an arm roots the column its own axis occupies ONE LAYER UP as
    /// well as the column it occupies here. See
    /// [`crate::SweepPolicy::tip_column_roots`]; `false` is the sabotage form.
    pub tip_column_roots: bool,
    /// Whether members are sized for their worst SINGLE peel event
    /// ([`crate::per_event`], rule a′, owner-approved 2026-09-29) and the load
    /// insertion is triggered by one event's routed load. **On in
    /// production.** `false` is the sabotage form: the max-field ledger, which
    /// sums the per-column maxima of different events where they meet
    /// ([`crate::SweepPolicy::with_max_field_sizing`]).
    pub per_event: bool,
    /// Whether a model column whose carried load was handed sideways holds its
    /// own peel out of the max-field raster until its next step decides where
    /// that event goes ([`UnrootedColumns`], "the ledger"). **On in production**, and it reads nothing unless an unrooted
    /// column was recorded. `false` is the sabotage form: the ledger as the
    /// round-7 experiment built it, which re-adds one cell of un-injected peel
    /// per re-routed layer ([`crate::SweepPolicy::without_handed_column_ledger`]).
    pub handed_column_ledger: bool,
    /// The uniform layer height `h`, mm: layer `k` stands at
    /// `plate_z_mm + k·h`. Read only by the head minimum
    /// ([`apply_head_minimum`]), which needs the height of a layer over the
    /// plate to place a head's section along its axis.
    pub layer_height_mm: f64,
    /// The plate's z in the drawn frame, mm (see [`Self::layer_height_mm`]).
    pub plate_z_mm: f64,
}

/// **The articulated head's per-layer MINIMUM radius**, applied beside the
/// area law, `min_radius_mm` and `thickened_mm` on every down-sweep run
/// (Craig 2026-10-06; `heads.rs` carries the law). Every layer is a minimum
/// and never a cap: the ledger only grows. A strut with no head is untouched,
/// bit for bit.
///
/// `r_t` is THIS run's tip radius, the area law at the contact slot, and the
/// contact slot keeps the pure area law (the ball is the contact, so it is
/// never raised). Then, with the head's law `law` ([`crate::heads::Head::law`]:
/// the planned one on every pass, the drawn one after
/// [`resize_heads_to_drawn_posts`]):
///
/// * head layers `k_joint+1` up to, not including, the contact slot (the
///   ledger's last slot, layer `k_top - 1`): `head_section_law_mm(r_t, s(k),
///   law)`, where `s(k)` is the axial distance from `D` of the head axis at
///   the layer's z, clamped to `[0, |DK|]`;
/// * the joint layer: `r_K,eff = final_joint_radius_law_mm(r_t, |DK|, law)`,
///   the head law at the measured `|DK|` and the FINAL tip. `K` is frozen, so
///   the lever is fixed whatever the ball's size: the joint follows the final
///   tip down as well as up, and no planned value enters;
/// * post layers below the joint: `post_section_law_mm(r_led, r_t, lever,
///   law)`, with the lever `|DK|·(1 − σ/ℓ)` falling linearly down the post's
///   chord from `K` to its base (`σ/ℓ` is the chord fraction at the layer's
///   z). The post never narrows going down (the engine's own rule for a
///   printed member), so the section is carried down as a running maximum. It
///   starts at the post's OWN section at the lever `|DK|`, NOT at the joint's
///   radius (Craig's ruling A, v18: each part is sized for its own
///   requirement; the joint size is never carried down the column).
///
/// The ledger on entry is the pure area law; a copy is kept in
/// [`crate::heads::Head::ledger_radius_mm`] for the final re-size and a
/// downgrade. Returns the effective joint radius, or `None` when nothing was
/// applied.
fn apply_head_minimum(strut: &mut SweepStrut, sizing: SizingPolicy) -> Option<f64> {
    // On entry `radius_mm` is the pure area-law ledger (the sizing pass wrote it
    // from scratch): keep a copy, the final re-size and a downgrade rebuild the
    // strut from it.
    let slots = strut.radius_mm.len();
    let head = strut.head.as_mut()?;
    head.ledger_radius_mm.clone_from(&strut.radius_mm);
    // A pass that re-sizes the ledger is the old law again: the certificate is
    // rewritten by the final sizing only.
    head.certified = None;
    head.old_law = None;
    let law = head.law();
    size_head_from_ledger(strut, sizing, law, slots)
}

/// **Re-size every live headed strut for the post it is DRAWN with** (v19
/// fix, Craig 2026-10-07: "size joints and posts for their ACTUAL lean").
///
/// Run once, after the last pass that re-routes or re-lands and before the
/// downgrade and the finished-field census. For each headed strut that is
/// drawn as a head, the post is the straight line the lowering draws from the
/// EXACT joint `K` (not its cell centre) to the shaft's drawn end: the boss
/// top for a plate-rooted post, the host-shifted base for a landed one
/// ([`drawn_shafts`], not the planned K-to-base chord): its lean gives `c = cos` (no clamp at
/// the lean law's maximum: a post drawn at 60.9 degrees grows its joint) and
/// the angle to the head's axis gives `|cos phi'|` and `sin phi'`, enveloped
/// over the post's segments ([`crate::heads::HeadLaw::enveloped`]) and stored
/// as [`crate::heads::Head::law_drawn`]. The head, joint and post slots are
/// then rebuilt from the saved area-law ledger
/// ([`crate::heads::Head::ledger_radius_mm`]) under that law, so a radius may
/// SHRINK as well as grow. `K` and `|DK|` stay frozen. One pass over
/// `struts`; returns how many were re-sized.
pub fn resize_heads_to_drawn_posts(
    grid: &Grid,
    struts: &mut [SweepStrut],
    sizing: SizingPolicy,
) -> usize {
    let mut resized = 0;
    let shafts = drawn_shafts(grid, struts, sizing);
    for (strut, shaft) in struts.iter_mut().zip(shafts) {
        let Some(head) = strut.head.as_ref() else {
            continue;
        };
        let slots = strut.radius_mm.len();
        let Some((post_top, post_base)) = shaft else {
            continue;
        };
        if head.drawn_straight || head.ledger_radius_mm.len() != slots {
            continue;
        }
        // The drawn post is one straight segment from `K` to the shaft's end.
        let Some(segment) = crate::heads::HeadLaw::of_post(
            head.unit,
            std::array::from_fn(|axis| post_base[axis] - post_top[axis]),
        ) else {
            continue;
        };
        let Some(law) = crate::heads::HeadLaw::enveloped(&[segment]) else {
            continue;
        };
        let Some(head) = strut.head.as_mut() else {
            continue;
        };
        head.law_drawn = Some(law);
        strut.radius_mm.clone_from(&head.ledger_radius_mm);
        head.certified = None;
        if size_head_from_ledger(strut, sizing, law, slots).is_some() {
            resized += 1;
        }
    }
    resized
}

/// **The final re-size with the joint certificate** (Craig 2026-10-11: "the
/// joint is mis-sized, fix THAT"). [`resize_heads_to_drawn_posts`] first sizes
/// every head on the old law (today's geometry, and the fallback); then
/// [`crate::joint_size::certify`] rewrites the cone, joint and post of every
/// head it can certify, and leaves the rest on the old law, counted
/// ([`crate::heads::JointCertificate`]). Returns what the old re-size returns.
pub(crate) fn resize_heads_certified(
    grid: &Grid,
    struts: &mut [SweepStrut],
    sizing: SizingPolicy,
    ctx: &crate::joint_size::JointContext<'_>,
    pitch_mm: f64,
) -> usize {
    let resized = resize_heads_to_drawn_posts(grid, struts, sizing);
    if resized == 0 {
        return resized;
    }
    let arms = drawn_arms(grid, struts, sizing, crate::slenderness::live);
    crate::joint_size::certify(struts, sizing, ctx, arms, pitch_mm);
    resized
}

/// A live member as the lowering gets it: its arm (head payload on the old law
/// until the certificate rewrites it), and the shaft line it is drawn on.
pub(crate) struct DrawnArm {
    pub arm: plinth_support::SweepArm,
    /// The shaft's top: the joint `K` of a headed member, the plate shaft's
    /// ball centre, or the member's seat.
    pub top: [f64; 3],
    /// The shaft's bottom: the boss top of a plate member, else its landing.
    pub bottom: [f64; 3],
}

/// **The shaft the file draws for each live headed strut**, `(top, bottom)` in
/// the drawn frame, or `None` for a strut the final re-size leaves alone (no
/// head, a head already drawn straight, a dead strut). One batch pass.
///
/// This is the line the lowering emits, not the planned K-to-base chord:
/// * a plate-rooted post ends at its BOSS TOP over the foot
///   ([`plinth_support::SweepRootFlare::plate_shaft_of`], the lowering's own
///   answer), which is higher than the base and so steeper than the chord;
/// * a landed post's base moves with the drawing offset of the host it lands
///   on ([`crate::footprint::followed_shifts_drawn`], the same displacements
///   the adapter applies when it builds the arms).
fn drawn_shafts(
    grid: &Grid,
    struts: &[SweepStrut],
    sizing: SizingPolicy,
) -> Vec<Option<([f64; 3], [f64; 3])>> {
    let candidate = |strut: &SweepStrut| {
        strut.head.as_ref().is_some_and(|head| {
            !head.drawn_straight
                && head.ledger_radius_mm.len() == strut.radius_mm.len()
                && crate::slenderness::live(strut)
        })
    };
    drawn_arms(grid, struts, sizing, candidate)
        .into_iter()
        .map(|drawn| drawn.map(|drawn| (drawn.top, drawn.bottom)))
        .collect()
}

/// [`drawn_shafts`] for every member `include` names, with its arm: one batch
/// pass. The shaft of a member with no plate shaft is `(K or seat, base)`.
pub(crate) fn drawn_arms(
    grid: &Grid,
    struts: &[SweepStrut],
    sizing: SizingPolicy,
    candidate: impl Fn(&SweepStrut) -> bool,
) -> Vec<Option<DrawnArm>> {
    if !struts.iter().any(&candidate) {
        return (0..struts.len()).map(|_| None).collect();
    }
    let (plate, h) = (sizing.plate_z_mm, sizing.layer_height_mm);
    let flare = plinth_support::SweepRootFlare::registered(h).ok();
    // Each member's drawn line off its planned one, exactly as the adapter
    // builds the offsets the hosts' junctions follow.
    let offsets: Vec<Option<crate::adapter::LineOffset>> = struts
        .iter()
        .map(|strut| {
            if strut.head.as_ref().is_some_and(|head| head.drawn_straight) {
                crate::adapter::downgraded_line_offset(flare.as_ref(), grid, plate, h, strut)
            } else {
                flare.as_ref().and_then(|flare| {
                    crate::adapter::planned_line_offset(flare, grid, plate, h, strut)
                })
            }
        })
        .collect();
    let followed = if offsets.iter().any(Option::is_some) {
        crate::footprint::followed_shifts_drawn(struts, &offsets)
    } else {
        crate::footprint::followed_shifts(struts)
    };
    struts
        .iter()
        .zip(followed)
        .map(|(strut, followed)| {
            if !candidate(strut) {
                return None;
            }
            let (seat, base) = crate::adapter::drawn_ends(grid, plate, h, strut);
            let moved = |point: [f64; 3], by: Option<[f64; 3]>| -> [f64; 3] {
                by.map_or(point, |by| std::array::from_fn(|axis| point[axis] + by[axis]))
            };
            let seat = moved(seat, followed.seat_mm);
            let base = moved(base, followed.base_mm);
            let arm = crate::adapter::sweep_arm(strut, seat, base, None, Vec::new());
            let plate_shaft = match &flare {
                Some(flare) if strut.landing == crate::Landing::Plate => {
                    flare.plate_shaft_of(&arm, plate)
                }
                _ => None,
            };
            // A headed member's post starts at the carried joint.
            let own_top = strut
                .head
                .as_ref()
                .filter(|head| !head.drawn_straight)
                .map_or(seat, |head| head.joint_mm);
            let (top, bottom) = plate_shaft.unwrap_or((own_top, base));
            Some(DrawnArm { arm, top, bottom })
        })
        .collect()
}

/// The head, joint and post slots of `strut` under `law`, written over the
/// ledger now in `radius_mm` (each slot is its ledger value or the law's
/// section, whichever is larger). See [`apply_head_minimum`] for the layout.
/// Returns the joint's radius.
#[expect(
    clippy::cast_precision_loss,
    reason = "layer indices bounded by the plan's own layer count are exact in f64"
)]
fn size_head_from_ledger(
    strut: &mut SweepStrut,
    sizing: SizingPolicy,
    law: crate::heads::HeadLaw,
    slots: usize,
) -> Option<f64> {
    let head = strut.head.as_ref()?;
    // The ledger spans layers `k_bottom ..= k_top - 1` (`ledger_len`), so the
    // contact slot is the LAST slot, and the joint must stand under it.
    let alive = strut.k_bottom != usize::MAX
        && strut.k_top > strut.k_bottom
        && slots == strut.k_top - strut.k_bottom
        && head.k_joint >= strut.k_bottom
        && head.k_joint - strut.k_bottom + 1 < slots;
    if !alive {
        return None;
    }
    let h = sizing.layer_height_mm;
    let r_t = strut.radius_mm[slots - 1];
    let joint_slot = head.k_joint - strut.k_bottom;
    let z_of = |layer: usize| sizing.plate_z_mm + layer as f64 * h;
    // The lever is the measured `|DK|` (the joint stands the ball's height
    // further out than the clearance alone, `Head::ball_allowance_mm`). `K`
    // and `|DK|` are frozen at placement, so the lever is fixed whatever the
    // ball's size. The joint is the head law at `|DK|` and the FINAL tip: no
    // placement maximum applies, and it is not held at the planned radius (a
    // final tip below the planned one gets the smaller ball).
    let to_k: [f64; 3] = std::array::from_fn(|axis| head.joint_mm[axis] - head.contact_mm[axis]);
    let length = to_k.iter().map(|v| v * v).sum::<f64>().sqrt();
    let r_k_eff = crate::heads::final_joint_radius_law_mm(r_t, length, law);
    // The head: axial distance from D along u to the layer's z.
    let u_z = head.unit[2];
    let mut section = Vec::with_capacity(slots);
    for slot in (joint_slot + 1)..(slots - 1) {
        let z = z_of(strut.k_bottom + slot);
        let s = if u_z.abs() > f64::EPSILON {
            ((z - head.contact_mm[2]) / u_z).clamp(0.0, length)
        } else {
            0.0
        };
        section.push((slot, crate::heads::head_section_law_mm(r_t, s, law)));
    }
    // The post: the chord from K down to the base at the post's own z span.
    let k_z = head.joint_mm[2];
    let base_z = z_of(strut.k_bottom);
    let span = k_z - base_z;
    // The running maximum starts at the post's own section at the full lever
    // `|DK|` over the ledger just under `K`, not at the joint's radius.
    let mut running = joint_slot.checked_sub(1).map_or(0.0, |below| {
        crate::heads::post_section_law_mm(strut.radius_mm[below], r_t, length, law)
    });
    for slot in (0..joint_slot).rev() {
        let z = z_of(strut.k_bottom + slot);
        let fraction = if span > f64::EPSILON {
            ((k_z - z) / span).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let lever = length * (1.0 - fraction);
        let wanted = crate::heads::post_section_law_mm(strut.radius_mm[slot], r_t, lever, law);
        running = running.max(wanted).max(strut.radius_mm[slot]);
        section.push((slot, running));
    }
    section.push((joint_slot, r_k_eff));
    for (slot, radius) in section {
        strut.radius_mm[slot] = strut.radius_mm[slot].max(radius);
    }
    Some(r_k_eff)
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
/// tip whose registered maximum is `X_ALLOWED · π · r_max²` (37.699 mm² at the
/// X_ALLOWED of 12 this was written against; 56.549 mm² at 18 since Craig's
/// ruling of 2026-09-30).
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
/// # Sized per event (rule a′, 2026-09-29)
///
/// The top-down pass below routes ONE max-over-events raster, and that raster
/// stays the engine's routing and accounting device: it decides where every
/// cell's load goes, which footprint an inserted arm is chosen from, and the
/// whole-run identities (`injected = plate + uncarried + absorbed`). It no
/// longer SIZES anything. Where its per-column maxima meet at a lowest line it
/// sums the maxima of events that never peel together, so after each pass the
/// per-event pass ([`crate::per_event`]) routes every event on its own along
/// the paths this pass recorded and rewrites every member's ledger with the
/// largest load any single event puts there.
///
/// The insertion trigger moves with it. The area law still decides — an arm
/// gets siblings when the load routed to it exceeds `ℓ_max` — but the load it
/// reads is ONE event's, taken from the previous pass of this call: the first
/// pass inserts nothing, and while the per-event pass finds an arm over the
/// limit, another pass inserts there (at most `max_rounds` of them). An
/// insertion can move load that a lower trigger was computed without; the next
/// pass re-measures, so an arm is never sized on a stale load, only possibly
/// placed on one.
///
/// # Panics
///
/// On a conservation residual above 1e-9 relative, per layer of the max-field
/// raster or per peel event of the per-event pass. Both are defects in this
/// code rather than statements about a model, which is why they are
/// assertions and not warnings.
pub fn run(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    struts: &mut Vec<SweepStrut>,
    sizing: SizingPolicy,
    insertion: InsertionPolicy,
    keep_out: &DrainKeepOut,
    unrooted: &UnrootedColumns,
) -> DownSweepOutcome {
    // A caller with no plan-wide cache builds one for this call; every pass of
    // this run still shares it. The plan's own driver builds one per plan and
    // calls [`run_with`].
    let layers = LayerCache::build(grid, slices);
    run_with(&layers, struts, sizing, insertion, keep_out, unrooted)
}

/// [`run`] over a [`LayerCache`] built once for the plan: the layers' masks,
/// cover clearance and model components are read from it, never recomputed.
/// The result is the result of [`run`] on `layers.grid()` and `layers.slices()`
/// to the bit.
///
/// # Panics
///
/// As [`run`].
pub fn run_with(
    layers: &LayerCache<'_>,
    struts: &mut Vec<SweepStrut>,
    sizing: SizingPolicy,
    insertion: InsertionPolicy,
    keep_out: &DrainKeepOut,
    unrooted: &UnrootedColumns,
) -> DownSweepOutcome {
    run_seeded(layers, struts, sizing, insertion, keep_out, unrooted, None)
}

/// [`run_with`] that may take its pass 1 from the [`RunSeed`] a finished call
/// on the same field left ([`DownSweepOutcome::seed`]).
///
/// Pass 1 inserts nothing (its trigger table is empty): it only routes, counts
/// and records the log the per-event sizing follows. On a field and inputs
/// equal to the seed's ([`GeometryDigest`]) that work is already done, so a
/// seeded pass 1 clears the members' ledgers as the walk does, takes its
/// accounting from the seed's snapshot and its routing from the seed's log, and
/// goes straight to the per-event sizing. The outcome, the members and the
/// triggers it hands to pass 2 equal those of the walked pass 1 to the bit.
/// Insertion off (`InsertionPolicy::disabled()`) makes the whole call that one
/// pass.
///
/// The seed's refusals are carried: a trigger the geometry refused in the
/// seeded call, at the same limit, is not re-tried here
/// (`refusals_carried_skipped`). A stale seed carries nothing.
///
/// # Panics
///
/// As [`run`].
pub fn run_seeded(
    layers: &LayerCache<'_>,
    struts: &mut Vec<SweepStrut>,
    sizing: SizingPolicy,
    insertion: InsertionPolicy,
    keep_out: &DrainKeepOut,
    unrooted: &UnrootedColumns,
    seed: Option<&RunSeed>,
) -> DownSweepOutcome {
    let grid = layers.grid();
    let slices = layers.slices();
    // Write-only `perf` marks (X0): the sizing site this run was called from
    // (named by the `sweep.down.calls.*` charge just before the call), the
    // run's own span, and the reuse probe's fingerprint of the members.
    let site = crate::perf::size_site_peek();
    let _size_span = crate::perf::size_span();
    let _run_span = crate::perf::span(crate::perf::Span::DownRun);
    let l_max = sizing.x_allowed
        * core::f64::consts::PI
        * sizing.tip_radius_max_mm
        * sizing.tip_radius_max_mm;
    if !sizing.per_event {
        // SABOTAGE: today's order, max over events first and sum after.
        let outcome = sweep_pass(
            layers,
            struts,
            sizing,
            insertion,
            keep_out,
            unrooted,
            None,
            None,
            &mut Vec::new(),
            &mut PassCtl::off(),
        );
        for strut in struts.iter_mut() {
            apply_head_minimum(strut, sizing);
        }
        return outcome;
    }
    // A SEED IS USED ONLY AGAINST THE FIELD IT WAS MADE ON. Anything short of
    // that -- no accounting snapshot, a log of another depth, a digest that
    // differs -- is stale: pass 1 walks, and nothing is carried.
    let usable_seed = seed.and_then(|seed| {
        let accounting = seed.accounting.as_ref()?;
        let fits = seed.log.hops.len() == slices.len()
            && seed.log.reads.len() == slices.len()
            && seed.geometry_digest == Some(field_digest(layers, struts, sizing, unrooted));
        fits.then_some((seed, accounting))
    });
    let seed_stale = seed.is_some() && usable_seed.is_none();
    let mut triggers: HashMap<u64, f64> = HashMap::new();
    // Refused triggers and the limit each was refused at. The carried ones are
    // kept apart so a skipped re-try can be counted.
    let mut refused: HashMap<u64, u64> = HashMap::new();
    let mut carried: HashMap<u64, u64> = HashMap::new();
    if let Some((seed, _)) = usable_seed {
        for &(key, limit_bits) in &seed.refused {
            let (member, layer) = crate::per_event::unpack(key);
            let same_limit = struts.get(member).is_some_and(|strut| {
                insertion.limit(strut, layer, l_max, sizing.x_allowed).to_bits() == limit_bits
            });
            if same_limit {
                carried.insert(key, limit_bits);
            }
        }
    }
    let mut skipped = 0_u64;
    let mut inserted = 0_u64;
    let mut inserted_by: Vec<(u32, u64)> = Vec::new();
    let mut geometry_exhausted = 0_u64;
    let mut evaluations = 0_u64;
    let mut passes = 0_usize;
    crate::reuse_probe::begin(grid.cells(), slices.len(), struts, site);
    // PERF LANE D3x: what the pass before left for this one (its trail and
    // its routing log) and the per-event sizing of its routing.
    let mut trail: Option<PassTrail<'_>> = None;
    let mut carried_log: Option<crate::per_event::RouteLog> = None;
    // The sizing of the last pass's routing, with the members' prints it was
    // sized for.
    let mut prev_sized: Option<(crate::per_event::PerEventSizing, Vec<MemberPrint>)> = None;
    // THE LAST CALL'S LAST PASS, from the plan's layer cache, for this call's
    // first pass to resume from: only against the same sizing policy and the
    // same unrooted columns (what a walk reads besides the members, which the
    // pass compares one by one), and only when this call records and does not
    // take its pass 1 from a seed.
    let run_key = run_key(sizing, insertion, keep_out, unrooted, slices.len());
    let mut stash_in = if insertion.enabled && usable_seed.is_none() {
        layers.take_stash().filter(|stash| stash.key == run_key)
    } else {
        None
    };
    loop {
        let seeded_here = if passes == 0 { usable_seed } else { None };
        let mut log;
        let mut refused_here = Vec::new();
        let mut outcome;
        let worst_max_field;
        let mut report = PassReport::default();
        if let Some((seed, accounting)) = seeded_here {
            // The walk's own first act on the members: every ledger cleared to
            // its length. The per-event rewrite below fills every slot.
            let layer_count = slices.len();
            if verify_incremental() {
                verify_seeded_pass(
                    layers, struts, sizing, insertion, keep_out, unrooted, seed, accounting,
                );
            }
            for strut in struts.iter_mut() {
                let span = ledger_len(strut, layer_count);
                strut.radius_mm.clear();
                strut.carried_mm2.clear();
                strut.radius_mm.resize(span, 0.0);
                strut.carried_mm2.resize(span, 0.0);
            }
            log = seed.log.clone();
            outcome = accounting.to_outcome();
            worst_max_field = accounting.worst_max_field_mm2;
        } else {
            log = crate::per_event::RouteLog::new(slices.len());
            // A pass after the first resumes from the trail of the one before
            // it. Only a call that can have a next pass records one.
            let base = if passes > 0 {
                trail.take().zip(carried_log.take())
            } else {
                stash_in.take().and_then(|stash| {
                    PassTrail::adopt(layers.pool(), stash.trail, stash.trail_bytes).map(|adopted| {
                        prev_sized = stash.sized;
                        (adopted, stash.log)
                    })
                })
            };
            let mut ctl = if insertion.enabled {
                PassCtl::recording(layers.pool(), base)
            } else {
                PassCtl::off()
            };
            outcome = sweep_pass(
                layers,
                struts,
                sizing,
                insertion,
                keep_out,
                unrooted,
                Some(&triggers),
                Some(&mut log),
                &mut refused_here,
                &mut ctl,
            );
            // A pass that stopped for a cancel (`sweep_pass_in`) left half a
            // field: neither the per-event sizing nor the stash may read it.
            if crate::cancel::cancelled() {
                return outcome;
            }
            trail = ctl.out.take();
            report = ctl.report;
            worst_max_field = struts
                .iter()
                .flat_map(|strut| strut.carried_mm2.iter().copied())
                .fold(0.0_f64, f64::max);
        }
        passes += 1;
        if passes == 1 {
            crate::reuse_probe::end_first_pass();
        }
        inserted += outcome.arms_inserted_by_load;
        inserted_by.append(&mut outcome.inserted_by);
        geometry_exhausted += outcome.insertion_geometry_exhausted;
        refused.extend(refused_here);
        let sized = {
            let _per_event_span = crate::perf::span(crate::perf::Span::PerEventSize);
            let members: &[SweepStrut] = struts;
            let size_now = || {
                crate::per_event::size(layers, members, &log, sizing, &|member, layer| {
                    insertion.limit(&members[member], layer, l_max, sizing.x_allowed)
                })
            };
            // THE SIZING FOLLOWS THE LOG AND THE MEMBERS' STRUCTURE, nothing
            // else (`per_event::size` reads each member's extent, landing,
            // kind, joint layer and cells, never its ledger). A pass that
            // resumed from the pass before, placed no arm and recorded the
            // log that pass recorded, to the bit, has the same members and the
            // same log: its sizing is the pass before's.
            let prints_now: Vec<MemberPrint> = if insertion.enabled {
                members.iter().map(|strut| member_print(strut, slices.len())).collect()
            } else {
                Vec::new()
            };
            let same_input = (report.log_unchanged
                || sabotage::dropped(sabotage::REUSE_LOG)
                    && (report.resumed_at.is_some() || report.skipped_all))
                && report.placed_nothing;
            let sabotaged = sabotage::dropped(sabotage::REUSE_LOG);
            let sized = match prev_sized
                .take()
                .filter(|(_, prints)| same_input && (sabotaged || *prints == prints_now))
            {
                Some((previous, _)) => {
                    if verify_incremental() {
                        let fresh = size_now();
                        if let Some(why) = sized_difference(&previous, &fresh) {
                            panic!("reused per-event sizing differs from a fresh one: {why}");
                        }
                    }
                    crate::perf::add(crate::PerfWork::SweepDownPerEventReused, 1);
                    previous
                }
                None => {
                    let fresh = size_now();
                    if verify_incremental() {
                        // The cached layer walk against every layer
                        // rasterised and scanned afresh.
                        let reference = crate::per_event::size_rasterized(
                            layers,
                            members,
                            &log,
                            sizing,
                            &|member, layer| {
                                insertion.limit(&members[member], layer, l_max, sizing.x_allowed)
                            },
                        );
                        if let Some(why) = sized_difference(&reference, &fresh) {
                            panic!("cached per-event walk differs from a rasterised one: {why}");
                        }
                    }
                    fresh
                }
            };
            if insertion.enabled {
                prev_sized = Some((sized.clone(), prints_now));
            }
            sized
        };
        outcome.overloaded.clear();
        let (worst, worst_id) = apply_per_event_ledger(
            grid,
            slices.len(),
            struts,
            &sized.best,
            sizing,
            l_max,
            &mut outcome.overloaded,
        );
        // Write-only (M0): how many members' sized ledgers this pass changed.
        crate::reuse_probe::pass_done(struts);
        outcome.per_event = sized.report;
        outcome.reached_by_event = sized.reached;
        outcome.per_event.passes = passes;
        outcome.per_event.worst_member_max_field_mm2 = worst_max_field;
        outcome.per_event.worst_member_mm2 = worst;
        outcome.per_event.worst_member_id = worst_id;
        if insertion.enabled {
            evaluations += 1;
        }
        // A trigger the geometry already refused is not re-tried: the search
        // found nowhere to put a carrier, and it would find nowhere again.
        // That holds across calls too, for a refusal at the same limit that a
        // seed carried in.
        let open: HashMap<u64, f64> = sized
            .triggers
            .into_iter()
            .filter(|(key, _)| {
                if refused.contains_key(key) {
                    return false;
                }
                if carried.contains_key(key) {
                    skipped += 1;
                    return false;
                }
                true
            })
            .collect();
        let done = !insertion.enabled || open.is_empty();
        let exhausted = !done && passes > insertion.max_rounds;
        if done || exhausted {
            // A pass that PLACED arms read its growing arms first in each
            // layer, where a walk of the finished field reads them by id: the
            // same reads, in another order. The per-event sizing takes the
            // first reader of a cell, so that order is part of the log, and
            // such a log is not the pass 1 of a walk on the field it left.
            // It leaves no accounting snapshot, and the next call walks.
            let last_pass_inserted = outcome.arms_inserted_by_load > 0;
            outcome.inserted_by = inserted_by;
            outcome.refusals_carried_skipped = skipped;
            outcome.seeded_pass = usable_seed.is_some();
            outcome.seed_stale = seed_stale;
            let mut all_refused = carried;
            all_refused.extend(refused);
            outcome.refused = all_refused.into_iter().collect();
            outcome.refused.sort_unstable();
            outcome.geometry_digest = Some(field_digest(layers, struts, sizing, unrooted));
            outcome.arms_inserted_by_load = inserted;
            outcome.insertion_geometry_exhausted = geometry_exhausted;
            outcome.insertion_rounds = evaluations;
            outcome.insertion_rounds_exhausted = if exhausted {
                let layers: HashSet<usize> = open
                    .keys()
                    .map(|key| crate::per_event::unpack(*key).1)
                    .collect();
                layers.len() as u64
            } else {
                0
            };
            // Write-only (M0): the run's row, with its routing's hash.
            crate::reuse_probe::run_end(passes, inserted, &log);
            // What the next call on this field takes its pass 1 from: this
            // pass's accounting, taken before anything else changes it.
            outcome.accounting = (!last_pass_inserted).then(|| DownSweepAccounting::of(&outcome));
            // Leave the last pass for the next call on this plan.
            if insertion.enabled {
                if let Some(mut kept) = trail.take() {
                    let sized = prev_sized.take();
                    let extra = approx_log_bytes(&log)
                        + sized.as_ref().map_or(0, |(sized, prints)| {
                            approx_sized_bytes(sized) + (prints.len() * core::mem::size_of::<MemberPrint>()) as u64
                        });
                    // CHARGED ONCE: the trail holds only its own bytes (an
                    // adopted stash gave back its log and sizing bytes when it
                    // left the cache, and the trail took only its own), so the
                    // log and the sizing are added here and nowhere else.
                    let trail_bytes = kept.bytes;
                    if kept.charge(extra) {
                        let (trail, bytes) = kept.into_data();
                        layers.put_stash(Stash {
                            key: run_key,
                            trail,
                            trail_bytes,
                            bytes,
                            log: log.clone(),
                            sized,
                        });
                    }
                }
            }
            outcome.route_log = log;
            return outcome;
        }
        carried_log = Some(log);
        triggers = open;
    }
}

/// The shadow check of a seeded pass 1 (`PLINTH_SWEEP_VERIFY_INCREMENTAL=1`):
/// walk pass 1 for real on a copy of the members and require its log and its
/// accounting to equal the seed's, by bits.
///
/// # Panics
///
/// On any difference.
#[expect(
    clippy::too_many_arguments,
    reason = "the walk's own inputs plus the seed under check"
)]
fn verify_seeded_pass(
    layers: &LayerCache<'_>,
    struts: &[SweepStrut],
    sizing: SizingPolicy,
    insertion: InsertionPolicy,
    keep_out: &DrainKeepOut,
    unrooted: &UnrootedColumns,
    seed: &RunSeed,
    accounting: &DownSweepAccounting,
) {
    let mut copy = struts.to_vec();
    let mut log = crate::per_event::RouteLog::new(layers.slices().len());
    let mut sink = Vec::new();
    let mut walked = sweep_pass(
        layers,
        &mut copy,
        sizing,
        insertion,
        keep_out,
        unrooted,
        Some(&HashMap::new()),
        Some(&mut log),
        &mut sink,
        &mut PassCtl::off(),
    );
    // A cancelled pose's walk stopped at a layer boundary: its log is a prefix
    // of the seed's, not a difference. The caller ends the pose at its next
    // safe point.
    if crate::cancel::cancelled() {
        return;
    }
    walked.per_event.worst_member_max_field_mm2 = copy
        .iter()
        .flat_map(|strut| strut.carried_mm2.iter().copied())
        .fold(0.0_f64, f64::max);
    assert!(
        log == seed.log,
        "seeded pass 1: the walked route log differs from the seed's"
    );
    let walked = DownSweepAccounting::of(&walked);
    assert_eq!(
        format!("{walked:?}"),
        format!("{accounting:?}"),
        "seeded pass 1: the walked accounting differs from the seed's"
    );
}

/// Rewrite every member's ledger with its per-event design load, and name the
/// members over `ℓ_max`.
///
/// The design load at a layer is the running maximum, from the tip down, of
/// the largest cumulative force any one event reached there
/// ([`crate::per_event`]). An arm also never reads less than one cell of its
/// own layer's peel, which is the floor the max-field ledger always gave it.
/// Radius is the registered area law, then the member's own minimum and any
/// thickened layer, exactly as the top-down pass applies them. Returns the
/// heaviest member's load and id.
fn apply_per_event_ledger(
    grid: &Grid,
    layers: usize,
    struts: &mut [SweepStrut],
    best: &[Vec<f64>],
    sizing: SizingPolicy,
    l_max: f64,
    overloaded: &mut Vec<OverloadedStrut>,
) -> (f64, u32) {
    let floor = grid.cell_area_mm2() * sizing.alpha;
    let mut worst = 0.0_f64;
    let mut worst_id = u32::MAX;
    for (member, strut) in struts.iter_mut().enumerate() {
        if strut.k_bottom == usize::MAX {
            continue;
        }
        let Some(best) = best.get(member) else {
            continue;
        };
        // Only an arm reads peel. A model tie (2026-10-03) seats a tip in the
        // model too, but its seat is under no face the routing loads (a
        // wall's load goes straight down the model's own column), so the one
        // cell this floor grants would be load nobody injected (measured on
        // the brace pair: a load gap of -0.0075 mm2 from three ties).
        let reads_peel = strut.kind == crate::StrutKind::Arm;
        let alive = strut.k_top.min(layers) > strut.k_bottom;
        let slots = strut.carried_mm2.len().min(best.len());
        let mut running = 0.0_f64;
        for slot in (0..slots).rev() {
            let own = if reads_peel && alive { floor } else { 0.0 };
            running = running.max(best[slot]).max(own);
            let layer = strut.k_bottom + slot;
            strut.carried_mm2[slot] = running;
            let thickened = strut.thickened_mm.get(layer).copied().unwrap_or(0.0);
            strut.radius_mm[slot] = size_arm(running, sizing)
                .max(strut.min_radius_mm)
                .max(thickened);
            if running > l_max {
                overloaded.push(OverloadedStrut {
                    strut_id: strut.id,
                    layer,
                    carried_mm2: running,
                    realized_x: running
                        / (core::f64::consts::PI
                            * sizing.tip_radius_max_mm
                            * sizing.tip_radius_max_mm),
                });
            }
        }
        // A headed member's per-layer minimum, beside the area law above.
        apply_head_minimum(strut, sizing);
        if running > worst {
            worst = running;
            worst_id = strut.id;
        }
    }
    // Top of the stack first, then by id: the order the top-down pass named
    // them in.
    overloaded.sort_by(|a, b| b.layer.cmp(&a.layer).then(a.strut_id.cmp(&b.strut_id)));
    (worst, worst_id)
}

/// One top-down pass over the max-over-events raster: route, insert, read,
/// size.
///
/// `triggers` is `None` in the sabotage form, where the insertion is triggered
/// by the max-field load itself and this pass's ledger is final. Otherwise it
/// holds the per-event loads, above `ℓ_max`, that the previous pass measured,
/// keyed by [`crate::per_event::entry`]; an empty table inserts nothing.
/// `log` records the routing for the per-event pass. `refused` collects the
/// triggers the geometry could not relieve.
#[expect(
    clippy::too_many_arguments,
    reason = "the pass reads the grid, the slices, the policies and the keep-out it always read, \
          plus the trigger table it is driven by and the two records it writes for the per-event \
          pass; a struct would move the argument list rather than shorten it"
)]
fn sweep_pass(
    layers: &LayerCache<'_>,
    struts: &mut Vec<SweepStrut>,
    sizing: SizingPolicy,
    insertion: InsertionPolicy,
    keep_out: &DrainKeepOut,
    unrooted: &UnrootedColumns,
    triggers: Option<&HashMap<u64, f64>>,
    mut log: Option<&mut crate::per_event::RouteLog>,
    refused: &mut Vec<(u64, u64)>,
    ctl: &mut PassCtl<'_>,
) -> DownSweepOutcome {
    if !(verify_incremental() && ctl.base.is_some()) {
        let outcome = sweep_pass_checked(
            layers, struts, sizing, insertion, keep_out, unrooted, triggers, log, refused, ctl,
        );
        #[cfg(test)]
        TEST_REPORTS.with(|reports| reports.borrow_mut().push(ctl.report));
        return outcome;
    }
    // THE SHADOW CHECK OF A RESUMED PASS (`PLINTH_SWEEP_VERIFY_INCREMENTAL=1`):
    // the whole pass walked from the top on copies of everything it writes,
    // recording its own trail, then the resumed pass for real. The outcome,
    // the members (ledgers included), the route log, the refusals and the
    // trail they leave must agree by bits.
    let pool = ctl.pool.expect("a pass that resumes also records");
    let mut full_struts = struts.clone();
    let mut full_log = log
        .as_deref()
        .map(|_| crate::per_event::RouteLog::new(layers.slices().len()));
    let mut full_refused = refused.clone();
    let mut full_ctl = PassCtl::recording(pool, None);
    let full = sweep_pass_checked(
        layers,
        &mut full_struts,
        sizing,
        insertion,
        keep_out,
        unrooted,
        triggers,
        full_log.as_mut(),
        &mut full_refused,
        &mut full_ctl,
    );
    let resumed = sweep_pass_checked(
        layers,
        struts,
        sizing,
        insertion,
        keep_out,
        unrooted,
        triggers,
        log.as_deref_mut(),
        refused,
        ctl,
    );
    // A cancelled pose (`crate::cancel`) stopped one or both walks at a layer
    // boundary: the two halves are not comparable and nothing reads them (the
    // flag only ever goes from clear to set, so a clear flag here means both
    // walks ran whole). The caller ends the pose at its next safe point.
    if crate::cancel::cancelled() {
        return resumed;
    }
    if let Some(difference) = pass_difference(
        (&full, &full_struts, full_log.as_ref(), &full_refused),
        (&resumed, struts, log.as_deref(), refused),
    ) {
        panic!("resumed sweep pass differs from the full pass: {difference}");
    }
    match (&full_ctl.out, &ctl.out) {
        (Some(a), Some(b)) => {
            if let Some(why) = a.difference(b) {
                panic!("resumed sweep pass left another trail than the full pass: {why}");
            }
        }
        (None, None) => {}
        _ => panic!("resumed sweep pass and full pass disagree on whether they left a trail"),
    }
    #[cfg(test)]
    TEST_REPORTS.with(|reports| reports.borrow_mut().push(ctl.report));
    resumed
}

/// [`sweep_pass`] without the resume shadow check: the sparse walk, or under
/// `PLINTH_SWEEP_VERIFY_SPARSE=1` the dense and the sparse walk compared.
#[expect(
    clippy::too_many_arguments,
    reason = "the pass reads the grid, the slices, the policies and the keep-out it always read, \
          plus the trigger table it is driven by and the records it writes; a struct would move \
          the argument list rather than shorten it"
)]
fn sweep_pass_checked(
    layers: &LayerCache<'_>,
    struts: &mut Vec<SweepStrut>,
    sizing: SizingPolicy,
    insertion: InsertionPolicy,
    keep_out: &DrainKeepOut,
    unrooted: &UnrootedColumns,
    triggers: Option<&HashMap<u64, f64>>,
    log: Option<&mut crate::per_event::RouteLog>,
    refused: &mut Vec<(u64, u64)>,
    ctl: &mut PassCtl<'_>,
) -> DownSweepOutcome {
    if !verify_sparse() {
        return sweep_pass_in(
            PassMode::Sparse,
            layers,
            struts,
            sizing,
            insertion,
            keep_out,
            unrooted,
            triggers,
            log,
            refused,
            ctl,
        );
    }
    // THE SHADOW CHECK (`PLINTH_SWEEP_VERIFY_SPARSE=1`): the same pass walked
    // dense on copies of everything the pass writes, then walked sparse for
    // real; the outcome, the members, the route log and the refusals must
    // agree by bits.
    let mut dense_struts = struts.clone();
    let mut dense_log = log.as_deref().map(|_| crate::per_event::RouteLog::new(layers.slices().len()));
    let mut dense_refused = refused.clone();
    let dense = sweep_pass_in(
        PassMode::Dense,
        layers,
        &mut dense_struts,
        sizing,
        insertion,
        keep_out,
        unrooted,
        triggers,
        dense_log.as_mut(),
        &mut dense_refused,
        &mut PassCtl::off(),
    );
    let log_given = log.is_some();
    let mut sparse_log = log;
    let sparse = sweep_pass_in(
        PassMode::Sparse,
        layers,
        struts,
        sizing,
        insertion,
        keep_out,
        unrooted,
        triggers,
        sparse_log.as_deref_mut(),
        refused,
        ctl,
    );
    // See the resume shadow check: a cancelled pose's halves are not comparable.
    if crate::cancel::cancelled() {
        return sparse;
    }
    if let Some(difference) = pass_difference(
        (&dense, &dense_struts, dense_log.as_ref(), &dense_refused),
        (&sparse, struts, sparse_log.as_deref(), refused),
    ) {
        panic!("sparse sweep pass differs from the dense pass (log given: {log_given}): {difference}");
    }
    sparse
}

/// `PLINTH_SWEEP_VERIFY_SPARSE=1`: every down-sweep pass is also walked dense,
/// over every cell of every layer, and compared with the sparse walk by bits
/// (outcome, members, route log, refusals); any difference panics. For gate
/// runs, not for production. Read once.
fn verify_sparse() -> bool {
    static VERIFY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *VERIFY.get_or_init(|| {
        std::env::var_os("PLINTH_SWEEP_VERIFY_SPARSE").is_some_and(|value| value == "1")
    })
}

/// What one pass leaves, for the comparison of two walks of it.
type PassResult<'a> = (
    &'a DownSweepOutcome,
    &'a [SweepStrut],
    Option<&'a crate::per_event::RouteLog>,
    &'a [(u64, u64)],
);

/// The first difference between two walks of one pass, by bits: the complete
/// `Debug` of the outcome and of every member (an `f64` prints its shortest
/// round-trip form, so equal text is equal bits and `-0.0` differs from
/// `0.0`), the route log by `==`, the refusals by `==`.
fn pass_difference(a: PassResult<'_>, b: PassResult<'_>) -> Option<String> {
    if format!("{:?}", a.0) != format!("{:?}", b.0) {
        return Some("outcome".to_owned());
    }
    if a.1.len() != b.1.len() {
        return Some(format!("member count {} vs {}", a.1.len(), b.1.len()));
    }
    for (index, (left, right)) in a.1.iter().zip(b.1).enumerate() {
        if format!("{left:?}") != format!("{right:?}") {
            return Some(format!("member {index}"));
        }
    }
    if a.2 != b.2 {
        return Some("route log".to_owned());
    }
    if a.3 != b.3 {
        return Some("refused triggers".to_owned());
    }
    None
}

// ---------------------------------------------------------------------------
// PERF LANE D3x (2026-10-15): RESUMABLE PASSES.
//
// A pass walks the layers from the top and, on a plan's second and later passes
// of a call, most of that walk re-computes what the pass before it computed:
// the passes differ only where the trigger table asks for an insertion and
// where the arms the pass before inserted now stand. Everything ABOVE the
// highest such layer is the same walk, to the bit, and everything BELOW the
// lowest such layer is the same walk again once the walk's state has come back
// to the one the pass before had there.
//
// So a pass that is told to record leaves a TRAIL: the complete state it
// handed from layer to layer at every `CHECKPOINT_STRIDE`-th boundary, the
// members' ledgers as the walk left them, and its accounting. The next pass
// resumes from the lowest checkpoint at or above the highest layer whose
// inputs changed, walks on, and stops at the first checkpoint boundary below
// every changed layer whose state equals the trail's, taking everything below
// from the trail. A pass whose inputs are the trail's to the bit walks
// nothing. Everything is exact by construction and checked by shadow
// (`PLINTH_SWEEP_VERIFY_INCREMENTAL=1`): the full pass is walked on a copy and
// compared with the resumed one, outcome, members, log, refusals and trail, by
// bits.
// ---------------------------------------------------------------------------

/// Layers between two checkpoints of a pass. Not a physical quantity: it
/// trades the bytes and the time of recording a checkpoint against the layers
/// a resumed pass re-walks above its first changed layer and below its last,
/// and no result depends on it.
const CHECKPOINT_STRIDE: usize = 16;

#[cfg(test)]
thread_local! {
    /// Tests drive the checkpoint stride, force the resume shadow check on,
    /// and read what each pass reported.
    static TEST_STRIDE: std::cell::Cell<usize> = const { std::cell::Cell::new(CHECKPOINT_STRIDE) };
    static TEST_VERIFY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TEST_REPORTS: std::cell::RefCell<Vec<PassReport>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
fn checkpoint_stride() -> usize {
    TEST_STRIDE.with(std::cell::Cell::get)
}

#[cfg(not(test))]
const fn checkpoint_stride() -> usize {
    CHECKPOINT_STRIDE
}

/// Everything the walk reads of one member, as one hash: the identity, kind,
/// extent, landing, joint layer, the cell of every layer it stands on, and the
/// two minima the raw radius takes. Two passes whose member `i` has the same
/// print walk it the same way. The ledger is not in it (the walk writes it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemberPrint {
    hash: u64,
    k_top: usize,
    k_bottom: usize,
}

fn member_print(strut: &SweepStrut, layers: usize) -> MemberPrint {
    let mut hash = mix(0x3d3d_0003, u64::from(strut.id));
    hash = mix(hash, strut.kind as u64);
    hash = mix(hash, strut.cell as u64);
    hash = mix(hash, strut.k_top as u64);
    hash = mix(hash, strut.k_bottom as u64);
    hash = match strut.landing {
        Landing::Plate => mix(hash, 1),
        Landing::Model { layer } => mix(mix(hash, 2), layer as u64),
        Landing::Strut { id, layer } => mix(mix(mix(hash, 3), u64::from(id)), layer as u64),
    };
    hash = mix(hash, strut.joint_hold_layer().map_or(u64::MAX, |layer| layer as u64));
    hash = mix(hash, strut.min_radius_mm.to_bits());
    hash = mix(hash, strut.thickened_mm.len() as u64);
    for radius in &strut.thickened_mm {
        hash = mix(hash, radius.to_bits());
    }
    if strut.k_bottom != usize::MAX {
        for layer in strut.k_bottom..=strut.k_top.min(layers) {
            hash = mix(hash, strut.cell_at(layer) as u64);
        }
    }
    MemberPrint {
        hash,
        k_top: strut.k_top,
        k_bottom: strut.k_bottom,
    }
}

/// The accounting of a walk with the counters an insertion ATTEMPT moves
/// cleared. An attempt that places nothing changes no state the walk carries
/// (it reads the layer and writes scratch), and moves exactly these two
/// counters and the caller's refusal list; a trail holds the accounting of a
/// walk that placed nothing, so the counters are the attempts' alone, and the
/// next pass's own attempts are its own.
fn pure_outcome(outcome: &DownSweepOutcome) -> DownSweepOutcome {
    let mut pure = outcome.clone();
    pure.insertion_geometry_exhausted = 0;
    pure.insertion_rounds_exhausted = 0;
    pure
}

/// The pending loads without their trailing zeros: members a pass inserts are
/// appended to the list, with nothing pending, so two passes' lists agree on
/// the members they share whatever their lengths.
fn trimmed(pending: &[f64]) -> &[f64] {
    let end = pending.iter().rposition(|load| load.to_bits() != 0).map_or(0, |last| last + 1);
    &pending[..end]
}

/// The per-event sizing difference of two sizings of one field, by bits.
fn sized_difference(
    a: &crate::per_event::PerEventSizing,
    b: &crate::per_event::PerEventSizing,
) -> Option<String> {
    if format!("{:?}", a.best) != format!("{:?}", b.best) {
        return Some("best".to_owned());
    }
    let sorted = |sizing: &crate::per_event::PerEventSizing| {
        let mut triggers: Vec<(u64, u64)> =
            sizing.triggers.iter().map(|(key, load)| (*key, load.to_bits())).collect();
        triggers.sort_unstable();
        triggers
    };
    if sorted(a) != sorted(b) {
        return Some("triggers".to_owned());
    }
    if format!("{:?}", a.report) != format!("{:?}", b.report) {
        return Some("report".to_owned());
    }
    if a.reached != b.reached {
        return Some("reached".to_owned());
    }
    None
}

/// The state a pass hands from one layer to the next, at the boundary before
/// layer `at` is processed: the live cells of the load raster and their loads,
/// the layer above's carriers, the layer above's handed columns, the loads
/// children handed to parents that have not read them yet, and the accounting
/// so far. Taken only while the pass has placed nothing, so no arm is growing
/// and the member list is the pass's start list. Every other buffer the walk
/// keeps across layers is either cleared before it is read (the second load
/// raster, the carrier flags two layers up, the per-layer scratch) or reset at
/// the end of the layer that set it.
struct Checkpoint {
    at: usize,
    sources: Vec<(u32, f64)>,
    carriers: Vec<u32>,
    handed: Vec<usize>,
    pending: Vec<f64>,
    outcome: DownSweepOutcome,
}

impl Checkpoint {
    fn take(
        at: usize,
        sources: &[u32],
        load_in: &[f64],
        carriers: &[u32],
        handed: &[usize],
        pending: &[f64],
        outcome: &DownSweepOutcome,
    ) -> Self {
        Self {
            at,
            sources: sources.iter().map(|&cell| (cell, load_in[cell as usize])).collect(),
            carriers: carriers.to_vec(),
            handed: handed.to_vec(),
            pending: trimmed(pending).to_vec(),
            outcome: pure_outcome(outcome),
        }
    }

    /// An estimate of the bytes this checkpoint holds.
    fn bytes(&self) -> u64 {
        let outcome = core::mem::size_of::<DownSweepOutcome>()
            + self.outcome.overloaded.len() * core::mem::size_of::<OverloadedStrut>()
            + self.outcome.model_overload.len() * core::mem::size_of::<ModelSectionOverload>()
            + self.outcome.inserted_by.len() * core::mem::size_of::<(u32, u64)>();
        (core::mem::size_of::<Self>()
            + self.sources.len() * core::mem::size_of::<(u32, f64)>()
            + self.carriers.len() * 4
            + self.handed.len() * core::mem::size_of::<usize>()
            + self.pending.len() * 8
            + outcome) as u64
    }

    /// Whether the walk's state now is this checkpoint's, bit for bit.
    fn matches(
        &self,
        sources: &[u32],
        load_in: &[f64],
        carriers: &[u32],
        handed: &[usize],
        pending: &[f64],
        outcome: &DownSweepOutcome,
    ) -> bool {
        self.sources.len() == sources.len()
            && self
                .sources
                .iter()
                .zip(sources)
                .all(|(&(cell, load), &now)| cell == now && load.to_bits() == load_in[now as usize].to_bits())
            && self.carriers == carriers
            && self.handed == handed
            && self.pending.len() == trimmed(pending).len()
            && self.pending.iter().zip(pending).all(|(a, b)| a.to_bits() == b.to_bits())
            && format!("{:?}", self.outcome) == format!("{:?}", pure_outcome(outcome))
    }

    /// The first difference between two checkpoints, for the shadow check.
    fn difference(&self, other: &Self) -> Option<String> {
        if self.at != other.at {
            return Some(format!("at {} vs {}", self.at, other.at));
        }
        let same = self.sources.len() == other.sources.len()
            && self
                .sources
                .iter()
                .zip(&other.sources)
                .all(|(a, b)| a.0 == b.0 && a.1.to_bits() == b.1.to_bits());
        if !same {
            return Some(format!("sources at {}", self.at));
        }
        if self.carriers != other.carriers {
            return Some(format!("carriers at {}", self.at));
        }
        if self.handed != other.handed {
            return Some(format!("handed at {}", self.at));
        }
        let pending = self.pending.len() == other.pending.len()
            && self.pending.iter().zip(&other.pending).all(|(a, b)| a.to_bits() == b.to_bits());
        if !pending {
            return Some(format!("pending at {}", self.at));
        }
        if format!("{:?}", self.outcome) != format!("{:?}", other.outcome) {
            return Some(format!("outcome at {}", self.at));
        }
        None
    }
}

/// What a recording pass leaves for the walk after it. Holds its bytes in the
/// layer cache's pool (the pool's limit is the budget: a trail the pool will
/// not take is not kept, and the next pass walks in full) and gives them back
/// when it is dropped.
pub(crate) struct PassTrail<'p> {
    data: TrailData,
    pool: &'p BytePool,
    bytes: u64,
}

/// A trail's contents, with no claim on the pool (the layer cache keeps one
/// between calls, charged by the cache itself).
#[derive(Default)]
pub(crate) struct TrailData {
    layers: usize,
    cells: usize,
    /// The walk print of every member as the pass started.
    prints: Vec<MemberPrint>,
    /// Checkpoints, highest layer first.
    checkpoints: Vec<Checkpoint>,
    /// Every member's ledger, `(carried, radius)`, as the walk left it (before
    /// the per-event rewrite), the members the pass ended with.
    raw: Vec<(Vec<f64>, Vec<f64>)>,
    /// The accounting the walk ended with, when it placed nothing (attempt
    /// counters cleared, [`pure_outcome`]).
    end: Option<DownSweepOutcome>,
}

impl core::ops::Deref for PassTrail<'_> {
    type Target = TrailData;

    fn deref(&self) -> &TrailData {
        &self.data
    }
}

impl core::ops::DerefMut for PassTrail<'_> {
    fn deref_mut(&mut self) -> &mut TrailData {
        &mut self.data
    }
}

impl<'p> PassTrail<'p> {
    fn new(pool: &'p BytePool, layers: usize, cells: usize) -> Self {
        Self {
            data: TrailData {
                layers,
                cells,
                ..TrailData::default()
            },
            pool,
            bytes: 0,
        }
    }

    /// A trail over `data` that the pool has agreed to hold `bytes` for;
    /// `None` when it will not.
    fn adopt(pool: &'p BytePool, data: TrailData, bytes: u64) -> Option<Self> {
        // Lazy: a trail that was never charged must not exist, because its
        // drop gives `bytes` back to the pool.
        pool.try_acquire(bytes).then(|| Self { data, pool, bytes })
    }

    /// The contents and the bytes they were charged (given back to the pool
    /// as the trail goes).
    fn into_data(mut self) -> (TrailData, u64) {
        (core::mem::take(&mut self.data), self.bytes)
    }

    /// Take `bytes` from the pool for this trail; `false` (and nothing held)
    /// when the pool will not give them.
    fn charge(&mut self, bytes: u64) -> bool {
        if self.pool.try_acquire(bytes) {
            self.bytes += bytes;
            true
        } else {
            false
        }
    }

    fn refund(&mut self, bytes: u64) {
        let bytes = bytes.min(self.bytes);
        self.bytes -= bytes;
        self.pool.release(bytes);
    }

    /// The bytes a sealed trail holds beyond its checkpoints: the ledgers and
    /// the end accounting.
    fn sealed_bytes(raw: &[(Vec<f64>, Vec<f64>)]) -> u64 {
        raw.iter().map(|(c, r)| ((c.len() + r.len()) * 8 + 48) as u64).sum::<u64>()
            + core::mem::size_of::<DownSweepOutcome>() as u64
    }
}

impl TrailData {
    /// The first difference between two trails, for the shadow check.
    fn difference(&self, other: &Self) -> Option<String> {
        if self.prints != other.prints {
            return Some("member prints".to_owned());
        }
        if self.checkpoints.len() != other.checkpoints.len() {
            return Some(format!(
                "{} checkpoints vs {}",
                self.checkpoints.len(),
                other.checkpoints.len()
            ));
        }
        for (a, b) in self.checkpoints.iter().zip(&other.checkpoints) {
            if let Some(why) = a.difference(b) {
                return Some(why);
            }
        }
        let raw = self.raw.len() == other.raw.len()
            && self.raw.iter().zip(&other.raw).all(|(a, b)| {
                a.0.len() == b.0.len()
                    && a.1.len() == b.1.len()
                    && a.0.iter().zip(&b.0).all(|(x, y)| x.to_bits() == y.to_bits())
                    && a.1.iter().zip(&b.1).all(|(x, y)| x.to_bits() == y.to_bits())
            });
        if !raw {
            return Some("raw ledgers".to_owned());
        }
        if format!("{:?}", self.end) != format!("{:?}", other.end) {
            return Some("end accounting".to_owned());
        }
        None
    }
}

impl Drop for PassTrail<'_> {
    fn drop(&mut self) {
        self.pool.release(self.bytes);
    }
}

/// What a down-sweep call leaves its plan's layer cache: its last pass's trail
/// and routing log, and the per-event sizing of that routing with the prints of
/// the members it was sized for. `key` says which sizing policy and unrooted
/// columns the pass walked under; the next call uses the stash only against
/// the same key, and compares its members with the trail's one by one.
pub(crate) struct Stash {
    key: u64,
    trail: TrailData,
    /// The bytes the trail alone holds (what an adopting call charges).
    trail_bytes: u64,
    /// The bytes the trail, the log and the sizing hold (what the cache
    /// charges its pool while it keeps them); never less than `trail_bytes`.
    pub(crate) bytes: u64,
    log: crate::per_event::RouteLog,
    sized: Option<(crate::per_event::PerEventSizing, Vec<MemberPrint>)>,
}

/// What a pass reads besides the members and the layers: EVERY field of the
/// sizing policy and of the insertion policy (the destructurings name them
/// all, so a new field fails to compile until it is hashed), the contact caps
/// by value (order-free over the tips: a map's iteration order is not an
/// input), the drain keep-out (every word a placement reads) and the unrooted
/// columns. A stash is used only against an equal key; when in doubt a field
/// belongs here, because a missed field resumes a pass under inputs it was not
/// walked under.
fn run_key(
    sizing: SizingPolicy,
    insertion: InsertionPolicy,
    keep_out: &DrainKeepOut,
    unrooted: &UnrootedColumns,
    layers: usize,
) -> u64 {
    let InsertionPolicy {
        enabled,
        max_struts,
        max_rounds,
        contact_caps,
    } = insertion;
    let SizingPolicy {
        x_allowed,
        tip_radius_min_mm,
        tip_radius_max_mm,
        owner_growth_ceiling_mm,
        alpha,
        lamina_reach_mm,
        straight_reach,
        tip_column_roots,
        per_event,
        handed_column_ledger,
        layer_height_mm,
        plate_z_mm,
    } = sizing;
    let mut hash = mix(0x3d3d_0001, layers as u64);
    for value in [
        x_allowed,
        tip_radius_min_mm,
        tip_radius_max_mm,
        owner_growth_ceiling_mm,
        alpha,
        lamina_reach_mm,
        layer_height_mm,
        plate_z_mm,
    ] {
        hash = mix(hash, value.to_bits());
    }
    for flag in [straight_reach, tip_column_roots, per_event, handed_column_ledger] {
        hash = mix(hash, u64::from(flag));
    }
    hash = mix(hash, u64::from(enabled));
    hash = mix(hash, max_struts as u64);
    hash = mix(hash, max_rounds as u64);
    hash = match contact_caps {
        None => mix(hash, 0),
        Some(caps) => {
            // A sum of per-tip hashes does not depend on the map's order.
            let tips = caps.by_tip.iter().fold(0_u64, |sum, (&(cell, k_top), contact)| {
                let tip = mix(mix(mix(0x3d3d_0002, cell as u64), k_top as u64), contact.to_bits());
                sum.wrapping_add(tip)
            });
            mix(mix(mix(hash, 1), caps.by_tip.len() as u64), tips)
        }
    };
    keep_out.digest_words(|word| hash = mix(hash, word));
    for layer in 0..layers {
        let cells = unrooted.at(layer);
        hash = mix(hash, cells.len() as u64);
        for &cell in cells {
            hash = mix(hash, u64::from(cell));
        }
    }
    hash
}

/// An estimate of the bytes of a routing log.
fn approx_log_bytes(log: &crate::per_event::RouteLog) -> u64 {
    log.hops
        .iter()
        .chain(&log.reads)
        .map(|entries| (entries.len() * 8 + 24) as u64)
        .sum()
}

/// An estimate of the bytes of a per-event sizing.
fn approx_sized_bytes(sized: &crate::per_event::PerEventSizing) -> u64 {
    (sized.best.iter().map(|slots| slots.len() * 8 + 24).sum::<usize>()
        + sized.reached.len()
        + sized.triggers.len() * 24
        + core::mem::size_of::<crate::per_event::PerEventReport>()) as u64
}

/// What a resumed pass did, for the counters and for the caller's own reuse
/// decisions. The tests read every field; the production caller reads the
/// ones it decides on.
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct PassReport {
    /// The layer whose checkpoint the walk started from.
    pub(crate) resumed_at: Option<usize>,
    /// The layer boundary the walk stopped at, on state equal to the base's.
    pub(crate) stopped_at: Option<usize>,
    /// Nothing differed from the base: no layer was walked.
    pub(crate) skipped_all: bool,
    pub(crate) walked: u64,
    pub(crate) layers: u64,
    /// The routing log is the base's, to the bit (a resumed pass only).
    pub(crate) log_unchanged: bool,
    /// The pass placed no arm: the members it ends with are the ones it began
    /// with.
    pub(crate) placed_nothing: bool,
}

/// How a pass records and resumes. [`PassCtl::off`] is the pass as it always
/// was.
pub(crate) struct PassCtl<'p> {
    /// `Some` records a trail for the next pass.
    pool: Option<&'p BytePool>,
    /// The trail of the pass before, and that pass's routing log, to resume from.
    base: Option<(PassTrail<'p>, crate::per_event::RouteLog)>,
    /// The trail this pass left.
    out: Option<PassTrail<'p>>,
    report: PassReport,
}

impl<'p> PassCtl<'p> {
    pub(crate) const fn off() -> Self {
        Self {
            pool: None,
            base: None,
            out: None,
            report: PassReport {
                resumed_at: None,
                stopped_at: None,
                skipped_all: false,
                walked: 0,
                layers: 0,
                log_unchanged: false,
                placed_nothing: false,
            },
        }
    }

    pub(crate) fn recording(
        pool: &'p BytePool,
        base: Option<(PassTrail<'p>, crate::per_event::RouteLog)>,
    ) -> Self {
        Self {
            pool: Some(pool),
            base,
            ..Self::off()
        }
    }
}

/// How a pass walks its cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PassMode {
    /// Perf lane D2 (2026-10-15): every loop visits only the ACTIVE cells it
    /// can act on, in ascending cell order (the order the dense walk visits
    /// them in), and every clear touches only the cells that were set.
    Sparse,
    /// The walk as it stood before D2: every loop over every cell, every clear
    /// a fill. The oracle the sparse walk is checked against.
    Dense,
}

/// Cell classes the sabotage tests drop from the sparse walk's active sets
/// (`cfg(test)`; a constant `false` otherwise), each of which must turn the
/// sparse-equals-dense test red.
#[cfg(test)]
pub(crate) mod sabotage {
    use std::cell::Cell;

    /// A growing or newly inserted arm's cell is left out of the carrier list.
    pub const EXTRA_CARRIERS: u16 = 1;
    /// A tip-root column outside the cover is left out of the routing roots.
    pub const TIP_CELLS: u16 = 2;
    /// A handed column's re-entered peel is left out of the spread sources.
    pub const REENTERED: u16 = 4;
    /// A cell that holds load without being a carrier is left out of the next
    /// layer's sources.
    pub const ARM_CELLS: u16 = 8;
    /// The last cover cell of every layer is left out of the routing sweeps.
    pub const COVER_TAIL: u16 = 16;
    /// The first carrier of every layer is left out of the carrier list.
    pub const CARRIER_HEAD: u16 = 32;
    // Perf lane D3x: each invalidation rule of the resumed walk, dropped.
    /// The trigger table's layers do not bound where a resumed walk starts.
    pub const RESUME_TRIGGERS: u16 = 64;
    /// A changed or new member's layers do not bound where a resumed walk starts.
    pub const RESUME_MEMBERS: u16 = 128;
    /// The pending loads handed to a parent are not restored.
    pub const RESUME_PENDING: u16 = 256;
    /// The handed columns of the layer above are not restored.
    pub const RESUME_HANDED: u16 = 512;
    /// The early stop does not compare the walk's state with the base's.
    pub const RESUME_STOP_UNCHECKED: u16 = 1024;
    /// The members' ledgers above the start are not restored.
    pub const RESUME_LEDGER: u16 = 2048;
    /// The previous pass's per-event sizing is reused whatever the log and the
    /// members say.
    pub const REUSE_LOG: u16 = 4096; // sic: the sizing's whole reuse guard (log and member prints)
    /// The early stop ignores the lowest layer a difference touches.
    pub const RESUME_STOP_LOWEST: u16 = 8192;
    /// The restored outcome accumulators are fresh, not the checkpoint's.
    pub const RESUME_OUTCOME: u16 = 16384;
    /// The early stop does not watch the walk's ledger writes.
    pub const RESUME_STOP_LEDGER: u16 = 32768;

    thread_local! {
        static DROPPED: Cell<u16> = const { Cell::new(0) };
    }

    pub fn set(mask: u16) {
        DROPPED.with(|dropped| dropped.set(mask));
    }

    pub fn dropped(class: u16) -> bool {
        DROPPED.with(|dropped| dropped.get() & class != 0)
    }
}

#[cfg(not(test))]
mod sabotage {
    pub const EXTRA_CARRIERS: u16 = 0;
    pub const TIP_CELLS: u16 = 0;
    pub const REENTERED: u16 = 0;
    pub const ARM_CELLS: u16 = 0;
    pub const COVER_TAIL: u16 = 0;
    pub const CARRIER_HEAD: u16 = 0;
    pub const RESUME_TRIGGERS: u16 = 0;
    pub const RESUME_MEMBERS: u16 = 0;
    pub const RESUME_PENDING: u16 = 0;
    pub const RESUME_HANDED: u16 = 0;
    pub const RESUME_STOP_UNCHECKED: u16 = 0;
    pub const RESUME_LEDGER: u16 = 0;
    pub const REUSE_LOG: u16 = 0;
    pub const RESUME_STOP_LOWEST: u16 = 0;
    pub const RESUME_OUTCOME: u16 = 0;
    pub const RESUME_STOP_LEDGER: u16 = 0;

    #[inline(always)]
    pub const fn dropped(_class: u16) -> bool {
        false
    }
}

/// The union of two ascending, duplicate-free lists of cells, ascending and
/// duplicate-free, into `out`.
fn union_into(a: &[u32], b: &[u32], out: &mut Vec<u32>) {
    out.clear();
    out.reserve(a.len() + b.len());
    let (mut i, mut j) = (0_usize, 0_usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            core::cmp::Ordering::Less => {
                out.push(a[i]);
                i += 1;
            }
            core::cmp::Ordering::Greater => {
                out.push(b[j]);
                j += 1;
            }
            core::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
}

/// Ascending and duplicate-free.
fn sort_dedup(cells: &mut Vec<u32>) {
    cells.sort_unstable();
    cells.dedup();
}

/// A raster cell's load counts as live for the spread loop: anything the dense
/// loop does not skip (`load <= 0.0`).
#[inline]
#[expect(
    clippy::neg_cmp_op_on_partial_ord,
    reason = "the exact negation of the dense loop's `load <= 0.0` skip, a NaN included"
)]
fn live(load: f64) -> bool {
    !(load <= 0.0)
}

/// Cells the loops of a pass visited, by loop (perf lane D2's witness that the
/// sparse walk fired: compare with `sweep.down.cell_layers`, what the dense
/// walk visits per loop).
#[derive(Clone, Copy, Debug, Default)]
struct PassVisits {
    /// The routing field: its initialisation and every relax round.
    route: u64,
    /// The spread loop's sources.
    spread: u64,
    /// The max loop's cells.
    max: u64,
    /// Every clear and reset (the load rasters, the per-layer masks, the
    /// routing field).
    fill: u64,
    /// The end-of-layer work: the model-section check, the next layer's
    /// sources.
    tail: u64,
}

#[cfg(test)]
thread_local! {
    /// The visits of the last pass this thread walked (tests read them).
    static LAST_VISITS: std::cell::Cell<PassVisits> = const {
        std::cell::Cell::new(PassVisits { route: 0, spread: 0, max: 0, fill: 0, tail: 0 })
    };
}

impl PassVisits {
    fn add(&mut self, other: &Self) {
        self.route += other.route;
        self.spread += other.spread;
        self.max += other.max;
        self.fill += other.fill;
        self.tail += other.tail;
    }

    fn publish(&self) {
        #[cfg(test)]
        LAST_VISITS.with(|last| last.set(*self));
        crate::perf::add(crate::PerfWork::SweepDownVisitRoute, self.route);
        crate::perf::add(crate::PerfWork::SweepDownVisitSpread, self.spread);
        crate::perf::add(crate::PerfWork::SweepDownVisitMax, self.max);
        crate::perf::add(crate::PerfWork::SweepDownVisitFill, self.fill);
        crate::perf::add(crate::PerfWork::SweepDownVisitTail, self.tail);
    }
}

/// [`sweep_pass`] in the given mode.
#[expect(
    clippy::too_many_arguments,
    reason = "the pass reads the grid, the slices, the policies and the keep-out it always read, \
          plus the trigger table it is driven by and the two records it writes for the per-event \
          pass; a struct would move the argument list rather than shorten it"
)]
fn sweep_pass_in<'p>(
    mode: PassMode,
    layers: &LayerCache<'_>,
    struts: &mut Vec<SweepStrut>,
    sizing: SizingPolicy,
    insertion: InsertionPolicy,
    keep_out: &DrainKeepOut,
    unrooted: &UnrootedColumns,
    triggers: Option<&HashMap<u64, f64>>,
    mut log: Option<&mut crate::per_event::RouteLog>,
    refused: &mut Vec<(u64, u64)>,
    ctl: &mut PassCtl<'p>,
) -> DownSweepOutcome {
    let dense = mode == PassMode::Dense;
    let grid = layers.grid();
    let slices = layers.slices();
    let cells = grid.cells();
    let area = grid.cell_area_mm2();
    crate::perf::add(crate::PerfWork::SweepDownPasses, 1);
    crate::perf::add(
        crate::PerfWork::SweepDownCellLayers,
        crate::perf::count(cells).saturating_mul(crate::perf::count(slices.len())),
    );
    let _pass_span = crate::perf::span(crate::perf::Span::SweepPass);
    // Pass 1 of a per-event run (its trigger table is empty) feeds the reuse
    // probe's band hashes when `PLINTH_PERF_BANDS=1` (write-only).
    let probe_bands = triggers.is_some_and(HashMap::is_empty) && crate::reuse_probe::collecting();
    if probe_bands {
        crate::reuse_probe::begin_bands(slices.len(), cells, grid.cols());
    }
    // M0 (2026-10-14): the pass trace (per layer: time, eligible and written
    // cells per loop, what the layer hands down). `None`, and every use below
    // a skipped branch, while `perf` counting is off.
    let mut trace = crate::reuse_probe::PassTrace::begin(slices.len(), cells);
    let mut outcome = DownSweepOutcome {
        min_layer_area_mm2: f64::INFINITY,
        ..DownSweepOutcome::default()
    };

    // THE ACTIVE CELLS (perf lane D2). The dense walk visits every cell of
    // the grid in six loops per layer and clears four rasters with fills, while
    // about 2 % of the cells can be acted on. In sparse mode every loop below
    // walks an ASCENDING list of the cells it can act on (the order the dense
    // loop visits them in, which fixes every float sum) and every clear walks
    // the list of cells that were set. In dense mode `full` stands for every
    // one of those lists, and every clear is a fill: that mode is the walk
    // as it stood before the lists, the oracle for them.
    let full: Vec<u32> = if dense {
        (0..u32::try_from(cells).unwrap_or(u32::MAX)).collect()
    } else {
        Vec::new()
    };
    // Cells visited by each loop, over the pass (`sweep.down.visit.*`).
    let mut visits = PassVisits::default();
    // The cells of `load_in` that can hold load, ascending (the spread loop's
    // sources); and, for each load raster, the cells that may be non-zero (what
    // its clear must reach). The two rasters swap roles with their lists.
    let mut sources: Vec<u32> = Vec::new();
    let mut sources_next: Vec<u32> = Vec::new();
    let mut dirty_in: Vec<u32> = Vec::new();
    let mut dirty_out: Vec<u32> = Vec::new();
    // Columns whose held peel re-entered the raster this layer (not yet in
    // `sources`).
    let mut reentered: Vec<u32> = Vec::new();
    // Cells a load was deposited on that are not carriers (misrouted).
    let mut stray: Vec<u32> = Vec::new();
    let mut max_cells: Vec<u32> = Vec::new();
    // Arms' own cells this layer, any kind, growing or fixed: where a landing
    // can put load whether or not the cell is a carrier.
    let mut arm_cells: Vec<u32> = Vec::new();
    // Carrier cells: the model's (the layer's cache list) plus the arms' that
    // stand outside it, ascending; one list per raster of `carriers` /
    // `carriers_above`, which swap roles together.
    let mut carrier_list: Vec<u32> = Vec::new();
    let mut carrier_list_above: Vec<u32> = Vec::new();
    let mut carrier_extras: Vec<u32> = Vec::new();
    let mut extras_sorted: Vec<u32> = Vec::new();
    // Cells set in the per-layer rasters below, so the next layer clears only
    // those.
    let mut continues_cells: Vec<u32> = Vec::new();
    let mut landing_cells: Vec<u32> = Vec::new();
    let mut tip_cells: Vec<u32> = Vec::new();
    let mut load_in = vec![0.0_f64; cells];
    let mut load_out = vec![0.0_f64; cells];
    let mut carriers = vec![false; cells];
    // Layer k+1's cover — the material the load actually travels through on
    // its way down into layer k — is `layers.cover_above(k)`, read from the
    // plan's cache: all false above the top layer, where there is no load to
    // route.
    // Per-layer scratch of the model-section check's load sums.
    let mut component_loads: Vec<f64> = Vec::new();
    let mut reader = layers.reader();
    // Layer k+1's CARRIERS, as opposed to its cover: model that carried plus every
    // arm alive there. Only the injection accounting reads it — see the max step.
    let mut carriers_above = vec![false; cells];
    // Cells where an ARM already stood one layer up, in its own axis rather
    // than in this column: the continuation flag the injection gate reads
    // beside `carriers_above` so a leaned arm counts one event, not one per
    // column it crosses.
    let mut continues_above = vec![false; cells];
    // WHERE AN ARM'S AXIS STANDS ONE LAYER UP, AND WHICH COLUMN THAT ARM IS IN
    // HERE. `u32::MAX` everywhere else.
    //
    // For a straight arm the two are the same cell and this array changes
    // nothing. For a LEANED arm they differ, and the difference is the whole of
    // the defect it exists to close: the arm's TIP stands in the seat column at
    // `k_top` -- which is the column the up-sweep rooted when it placed the arm
    // -- while the arm's body one layer down stands somewhere else, usually
    // OUTSIDE the model's own footprint at `k_top`. The routing field's root
    // test is `cover_above[cell] && carriers[cell]`, so that body column is
    // rejected as a root, and every cell the arm rooted in the up-sweep then
    // reads "no carrier anywhere I can walk to" -- an infinite gap, not a
    // marginal one. Measured on the Morel base at layer 107: 1433 of 1450 lost
    // cells were rooted by one of eight leaned arms, each of which reads
    // `carrier true, cover_above false` one layer under its own seat.
    //
    // The entry is an ALIAS, not a carrier: the load lands on the arm and the
    // arm reads its share at `cell_at(layer)`, so the routing field is told
    // "root here, deposit there" rather than being told the arm is in two
    // columns at once. Marking it a carrier instead would collect load in a
    // cell no arm ever reads, and carry it down a column with nothing in it.
    let mut tip_root = vec![u32::MAX; cells];
    // Whether each cell's chosen root is one of those tip columns. Carried with
    // `nearest` through the chamfer walk, so the accounting names every mm²
    // that reached its arm through the new rule and not only the cells sitting
    // directly on a tip.
    let mut via_tip = vec![false; cells];
    let mut nearest = vec![u32::MAX; cells];
    let mut distance = vec![f32::INFINITY; cells];
    // The dense walk's convergence snapshot; the sparse walk tracks whether a
    // round improved anything instead, and needs no copy.
    let mut distance_prev = vec![f32::INFINITY; if dense { cells } else { 0 }];
    let mut route_anchor = vec![crate::reach::NO_ANCHOR; cells];
    // THE UNROOTED-COLUMN RULE's scratch ([`UnrootedColumns`]). Allocated only
    // when the up-sweep left something unrooted; on every other field the
    // rule reads nothing and the pass is the pass it always was.
    let rule_active = !unrooted.is_empty();
    let rule_cells = if rule_active { cells } else { 0 };
    // This layer: model columns the up-sweep left unrooted, which are not
    // routing roots.
    let mut refused_root = vec![false; rule_cells];
    // The roots the routing field is offered: the carriers less `refused_root`.
    let mut roots = vec![false; rule_cells];
    // This layer: continuing model columns whose carried load went sideways
    // ("handed"), so they hold their own peel out of the raster. Set only at
    // recorded cells, and cleared there. `handed_prev` lists the layer above's.
    let mut handed = vec![false; rule_cells];
    let mut handed_prev: Vec<usize> = Vec::new();
    // The one cell's own peel event: what a handed column holds back, and
    // what re-enters the raster when its next step is not sideways.
    let own_peel = area * sizing.alpha;
    // This round: unrooted columns whose load (or, empty, whose next event) the
    // routing sent to a rooted carrier.
    let mut sent_sideways = vec![false; rule_cells];
    // Cells a still-growing arm stands in at the start of this layer: arms,
    // never model columns, so never refused as roots.
    let mut growing_here = vec![false; rule_cells];
    let mut growing_marked: Vec<usize> = Vec::new();
    let mut pending = vec![0.0_f64; struts.len()];
    // Per cell, this layer: whether the load an arm would read there is peel
    // that was put into the raster (arrived from above, or injected here), as
    // opposed to the arm's own non-injected per-layer duplicate. Allocated once.
    let mut peel_fed = vec![false; cells];
    // Insertion scratch, allocated once. Arms this sweep adds are "growing"
    // until their column meets something: their landing is discovered by the
    // descent itself, which is the same "first thing you hit going down" rule
    // `up_sweep::descend` applies, read off the layers this pass is already
    // walking instead of off a history that only exists going up.
    let mut growing: Vec<usize> = Vec::new();
    let mut landing_target = vec![u32::MAX; cells];
    let mut insert = InsertScratch::new(cells);
    let mut layer_arms: Vec<LayerArm> = Vec::new();
    // Pass-0 ordering scratch, allocated once per run.
    let mut pass_order: Vec<usize> = Vec::new();
    let mut pass_indegree: Vec<u32> = Vec::new();
    let mut pass_slot: Vec<u32> = Vec::new();
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

    // ---- D3x: the trail of the pass before, and the trail this pass leaves.
    let record_pool = ctl.pool.filter(|_| !dense);
    let tracked = record_pool.is_some();
    let prints: Vec<MemberPrint> = if tracked {
        struts.iter().map(|strut| member_print(strut, layer_count)).collect()
    } else {
        Vec::new()
    };
    let n_start = struts.len();
    let mut trail: Option<PassTrail<'p>> =
        record_pool.map(|pool| PassTrail::new(pool, layer_count, cells));
    // Where the walk starts, the base checkpoints below that (candidates for
    // the early stop), the base's ledgers and accounting, which members read
    // the same as the base's, and the lowest layer any difference touches.
    let mut resumed_at: Option<usize> = None;
    let mut lower: Vec<Checkpoint> = Vec::new();
    let mut lower_next = 0_usize;
    let mut base_raw: Vec<(Vec<f64>, Vec<f64>)> = Vec::new();
    let mut base_end: Option<DownSweepOutcome> = None;
    let mut same: Vec<bool> = Vec::new();
    let mut lowest_diff = usize::MAX;
    let mut stop_open = false;
    let mut ledger_clean = true;
    let mut log_dirty = false;
    let mut stopped_at: Option<usize> = None;
    if let (Some(_), Some((mut base, base_log))) = (record_pool, ctl.base.take()) {
        let usable = !probe_bands
            && triggers.is_some()
            && log.is_some()
            && base.layers == layer_count
            && base.cells == cells
            && base_log.hops.len() == layer_count
            && base_log.reads.len() == layer_count;
        if usable {
            // THE LAYERS THE INPUTS DIFFER ON: where the trigger table asks for
            // an insertion (every entry, whether or not it is over its limit
            // now), and where a member is not the base's member (new, gone or
            // changed in any way the walk reads). The walk above the highest of
            // them is the base's walk.
            let mut highest: Option<usize> = None;
            let mut lowest = usize::MAX;
            let top_layer = layer_count.saturating_sub(1);
            let mut touch = |top: usize, bottom: usize| {
                let top = top.min(top_layer);
                highest = Some(highest.map_or(top, |h| h.max(top)));
                lowest = lowest.min(bottom.min(top));
            };
            if let Some(table) = triggers {
                if !sabotage::dropped(sabotage::RESUME_TRIGGERS) {
                    for &key in table.keys() {
                        let layer = crate::per_event::unpack(key).1;
                        touch(layer, layer);
                    }
                }
            }
            same = (0..struts.len())
                .map(|member| {
                    member < base.prints.len()
                        && prints[member] == base.prints[member]
                        && base.raw.get(member).is_some_and(|(carried, radius)| {
                            carried.len() == struts[member].carried_mm2.len()
                                && radius.len() == struts[member].radius_mm.len()
                        })
                })
                .collect();
            if !sabotage::dropped(sabotage::RESUME_MEMBERS) {
                for member in 0..struts.len().max(base.prints.len()) {
                    let (old, new) = (base.prints.get(member), prints.get(member));
                    if old.is_some() && old == new {
                        continue;
                    }
                    for print in [old, new].into_iter().flatten() {
                        touch(
                            print.k_top,
                            if print.k_bottom == usize::MAX { 0 } else { print.k_bottom },
                        );
                    }
                }
            }
            match highest {
                None => {
                    // Nothing differs: the base's pass is this pass. Walk
                    // nothing; take its accounting and ledgers; keep its trail.
                    if let Some(end) = base.end.take() {
                        if same.iter().all(|&same| same) && base.raw.len() == struts.len() {
                            for (strut, (carried, radius)) in struts.iter_mut().zip(&base.raw) {
                                strut.carried_mm2.copy_from_slice(carried);
                                strut.radius_mm.copy_from_slice(radius);
                            }
                            if let Some(into) = log.as_deref_mut() {
                                *into = base_log;
                            }
                            let done = end.clone();
                            base.end = Some(end);
                            ctl.out = Some(base);
                            ctl.report = PassReport {
                                skipped_all: true,
                                layers: layer_count as u64,
                                log_unchanged: true,
                                placed_nothing: true,
                                ..PassReport::default()
                            };
                            if let Some(t) = trace.take() {
                                t.finish();
                            }
                            crate::perf::add(crate::PerfWork::SweepDownLayersSkipped, layer_count as u64);
                            crate::perf::add(crate::PerfWork::SweepDownPassesSkipped, 1);
                            return done;
                        }
                        base.end = Some(end);
                    }
                }
                Some(top) => {
                    if let Some(index) = base.checkpoints.iter().rposition(|cp| cp.at >= top) {
                        lower = base.checkpoints.split_off(index + 1);
                        let lower_bytes: u64 = lower.iter().map(Checkpoint::bytes).sum();
                        base.refund(lower_bytes);
                        let cp = &base.checkpoints[index];
                        resumed_at = Some(cp.at);
                        for &(cell, load) in &cp.sources {
                            load_in[cell as usize] = load;
                            sources.push(cell);
                        }
                        union_into(&cp.carriers, &sources, &mut dirty_in);
                        for &cell in &cp.carriers {
                            carriers_above[cell as usize] = true;
                        }
                        carrier_list_above.extend_from_slice(&cp.carriers);
                        if !sabotage::dropped(sabotage::RESUME_HANDED) {
                            handed_prev.extend_from_slice(&cp.handed);
                        }
                        if !sabotage::dropped(sabotage::RESUME_PENDING) {
                            let held = cp.pending.len().min(pending.len());
                            pending[..held].copy_from_slice(&cp.pending[..held]);
                        }
                        if !sabotage::dropped(sabotage::RESUME_OUTCOME) {
                            outcome = cp.outcome.clone();
                        }
                        base_raw = core::mem::take(&mut base.raw);
                        base.refund(PassTrail::sealed_bytes(&base_raw));
                        base_end = base.end.take();
                        if !sabotage::dropped(sabotage::RESUME_LEDGER) {
                            for (member, (carried, radius)) in base_raw.iter().enumerate() {
                                if same.get(member).copied().unwrap_or(false) {
                                    struts[member].carried_mm2.copy_from_slice(carried);
                                    struts[member].radius_mm.copy_from_slice(radius);
                                }
                            }
                        }
                        base.prints.clone_from(&prints);
                        lowest_diff = if sabotage::dropped(sabotage::RESUME_STOP_LOWEST) {
                            usize::MAX
                        } else {
                            lowest
                        };
                        stop_open = base_end.is_some() && !lower.is_empty();
                        if let Some(into) = log.as_deref_mut() {
                            *into = base_log;
                        }
                        trail = Some(base);
                    }
                }
            }
        }
    }
    if let Some(trail) = trail.as_mut() {
        trail.prints.clone_from(&prints);
    }
    let track_ledger = stop_open && !sabotage::dropped(sabotage::RESUME_STOP_LEDGER);
    let first_layer = resumed_at.map_or(layer_count, |at| at + 1);
    let mut recording = trail.is_some();
    let mut walked = 0_u64;
    let stride = checkpoint_stride();

    for layer in (0..first_layer).rev() {
        // A cancelled pose (`crate::cancel`) leaves the walk here, at a layer
        // boundary, with an empty outcome and the members half written. Its
        // callers read none of it: `run_seeded` returns at once, and every
        // down-sweep call site ends the pose at its next safe point. The
        // trail and the stash are dropped unsealed, which refunds their bytes.
        if crate::cancel::cancelled() {
            return DownSweepOutcome::default();
        }
        walked += 1;
        if tracked {
            // A checkpoint boundary: the state this pass hands to `layer`.
            let boundary =
                layer > 0 && layer % stride == 0 && layer + 1 < layer_count;
            let placed_nothing =
                growing.is_empty() && struts.len() == n_start && outcome.arms_inserted_by_load == 0;
            if boundary && stop_open && placed_nothing && layer < lowest_diff
                && (ledger_clean || !track_ledger)
            {
                while lower_next < lower.len() && lower[lower_next].at > layer {
                    lower_next += 1;
                }
                if lower_next < lower.len()
                    && lower[lower_next].at == layer
                    && (sabotage::dropped(sabotage::RESUME_STOP_UNCHECKED)
                        || lower[lower_next].matches(
                            &sources,
                            &load_in,
                            &carrier_list_above,
                            &handed_prev,
                            &pending,
                            &outcome,
                        ))
                {
                    stopped_at = Some(layer);
                    walked -= 1;
                    break;
                }
            }
            if boundary && recording && resumed_at != Some(layer) {
                if let Some(trail) = trail.as_mut().filter(|_| placed_nothing) {
                    let checkpoint = Checkpoint::take(
                        layer,
                        &sources,
                        &load_in,
                        &carrier_list_above,
                        &handed_prev,
                        &pending,
                        &outcome,
                    );
                    if trail.charge(checkpoint.bytes()) {
                        trail.checkpoints.push(checkpoint);
                    } else {
                        recording = false;
                    }
                } else {
                    recording = false;
                }
            }
        }
        let layer_log_before = match (resumed_at, log.as_deref_mut()) {
            (Some(_), Some(log)) => Some((
                core::mem::take(&mut log.hops[layer]),
                core::mem::take(&mut log.reads[layer]),
            )),
            _ => None,
        };
        if let Some(t) = trace.as_mut() {
            t.layer_begin();
        }
        outcome.rasteriser_entries += 1;
        // The layer's masks, its cover one layer up and that cover's
        // clearance come from the plan's cache (`layer_cache`), built once.
        // A layer the cache's byte budget did not keep is computed here, on
        // demand, into the reader's slots (same values, same order).
        reader.prepare(layer, true);
        let masks = reader.masks(layer);
        let cover_above = reader.cover_above(layer);
        let model_cells = reader.carrier_cells(layer);
        let layer_area = model_cells as f64 * area;
        if model_cells > 0 {
            outcome.peak_layer_area_mm2 = outcome.peak_layer_area_mm2.max(layer_area);
            outcome.min_layer_area_mm2 = outcome.min_layer_area_mm2.min(layer_area);
        }

        // The active lists this layer reads from the cache: the cells of the
        // model (`carry`) here, and of the cover above (the routing field's
        // domain).
        let carry_cells: &[u32] = if dense { &full } else { reader.carry_cells(layer) };
        let cover_cells_above: &[u32] = if dense {
            &full
        } else {
            reader.cover_cells_above(layer)
        };
        let mut layer_visits = PassVisits::default();
        if dense {
            carriers.copy_from_slice(&masks.carry);
            continues_above.fill(false);
            landing_target.fill(u32::MAX);
            tip_root.fill(u32::MAX);
            layer_visits.fill += 4 * cells as u64;
        } else {
            // `carriers` holds the layer two up (the rasters swapped at the
            // end of the last layer): clear what it set, set this layer's.
            for &cell in &carrier_list {
                carriers[cell as usize] = false;
            }
            for &cell in carry_cells {
                carriers[cell as usize] = true;
            }
            for &cell in &continues_cells {
                continues_above[cell as usize] = false;
            }
            for &cell in &landing_cells {
                landing_target[cell as usize] = u32::MAX;
            }
            for &cell in &tip_cells {
                tip_root[cell as usize] = u32::MAX;
            }
            layer_visits.fill += (carrier_list.len()
                + carry_cells.len()
                + continues_cells.len()
                + landing_cells.len()
                + tip_cells.len()) as u64;
        }
        carrier_list.clear();
        carrier_extras.clear();
        continues_cells.clear();
        landing_cells.clear();
        tip_cells.clear();
        for &id in &alive_by_layer[layer] {
            // The arm's cell AT THIS LAYER, not its seat cell: a leaned arm
            // occupies a different column at every layer it crosses, and
            // marking its seat column all the way down would collect load in a
            // column the arm is not in.
            let cell = struts[id as usize].cell_at(layer);
            // Only an ARM carries peel load. A brace or a mast is lateral
            // stiffening: it has no tip ball and, by its own admissibility, no
            // model above its columns, so it must never be offered as a
            // carrier or a routing root (reviewer finding 2026-09-06: without
            // this filter a brace under cover would be grown by the area law
            // and emitted load-bearing with no contact geometry at either end).
            // A model tie (2026-10-03) is lateral stiffening too: its tip is
            // on a wall, under no face the routing loads.
            if struts[id as usize].kind != crate::StrutKind::Arm {
                continue;
            }
            carriers[cell] = true;
            if !masks.carry[cell] {
                carrier_extras.push(cell as u32);
            }
            // A LEANED ARM IS ONE MEMBER, NOT A NEW ONE PER COLUMN, AND THE
            // INJECTION GATE HAS TO KNOW IT. The gate below counts an event
            // only where the column did not carry one layer up, which is what
            // stops a vertical arm injecting `own` once per layer for its whole
            // length. A leaned arm defeats that test by construction: it stands
            // in a DIFFERENT column at every layer it crosses, so its cell here
            // reads "nothing above me" and injects again, and again. The
            // inflation is invisible in the emitted file — the gate feeds
            // `injected_load_mm2` alone, and `max` discards the duplicates
            // before they can reach the plate — but it drove the pipeline's
            // conservation ratio to 0.9869 on `make_thin_fin` with nothing
            // actually lost, which is a lying metric of exactly the kind the
            // gate was written to end. The arm's OWN cell one layer up is where
            // its material was; if that carried, this cell continues the same
            // cured run.
            let above = struts[id as usize].cell_at(layer + 1);
            if carriers_above[above] {
                continues_above[cell] = true;
                continues_cells.push(cell as u32);
            }
            // AND THE SAME CELL IS WHERE THIS ARM HOLDS SOMETHING UP. The
            // material at `above` on layer k+1 rests on this arm's own axis,
            // with no lateral travel to pay for; the arm is where the up-sweep
            // rooted that column, and this is the down-sweep saying so. First
            // arm wins, in `alive_by_layer` order, which is arm id order.
            if sizing.tip_column_roots && above != cell && tip_root[above] == u32::MAX {
                tip_root[above] = u32::try_from(cell).unwrap_or(u32::MAX);
                tip_cells.push(above as u32);
                outcome.tip_column_roots_offered += 1;
            }
            // What a growing arm lands ON if its column meets this one. Only
            // arms whose extent is already fixed are landing targets; a growing
            // arm can never be one, because an insertion candidate must not be
            // a carrier and a growing arm's cell always is.
            landing_target[cell] = id;
            landing_cells.push(cell as u32);
        }
        // Every arm still growing steps down one layer before this layer is
        // read. Straight down is the whole of the step today; the path exists
        // so a growing arm can lean the same way a seated one does.
        for &index in &growing {
            let next = growing_cell(&struts[index]);
            struts[index]
                .path
                .push(u32::try_from(next).unwrap_or(u32::MAX));
        }
        for &index in &growing {
            let cell = growing_cell(&struts[index]);
            carriers[cell] = true;
            if !masks.carry[cell] {
                carrier_extras.push(cell as u32);
            }
        }
        // THE UNROOTED-COLUMN RULE: which model columns at this layer are not
        // routing roots. A column the up-sweep left unrooted here, holding no
        // arm; an arm is a root wherever it stands. Sparse: only the recorded
        // cells are touched, and they are cleared again at the end of the
        // layer.
        let unrooted_here = unrooted.at(layer);
        if rule_active {
            for &index in &growing {
                let cell = growing_cell(&struts[index]);
                growing_here[cell] = true;
                growing_marked.push(cell);
            }
            for &cell in unrooted_here {
                let cell = cell as usize;
                if masks.carry[cell] && landing_target[cell] == u32::MAX && !growing_here[cell] {
                    refused_root[cell] = true;
                    outcome.unrooted_columns_refused += 1;
                }
            }
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
        let mut tip_root_load;
        let mut misrouted;
        let mut spread_hits;
        let mut max_hits;
        let mut injected_here;
        let mut worst_gap;
        let mut any_lost;
        let mut route_converged;
        let mut unrooted_rerouted;
        let mut unrooted_kept;
        let mut held_reads;
        let mut held_load;
        // Peel of handed columns one layer up that re-entered the raster here
        // (once per layer, in the first round).
        let mut reentered_here = 0.0_f64;
        let mut reentered_reads = 0_u64;
        let mut round = 0_usize;
        // The routing field's clearance is `cover_above`'s, which does not
        // change between the rounds below (nor between passes: it is cached).
        let route_clear = reader.clearance_above(layer);
        // Per-event mode inserts at most once per layer per pass: the trigger
        // table was measured before this pass, so after an insertion only the
        // re-route below is needed, and the next pass re-measures.
        let mut inserted_here = false;
        loop {
            if let Some(t) = trace.as_mut() {
                // The cells the clears below change.
                t.counting(|rec| {
                    rec.fill_elig += load_out
                        .iter()
                        .zip(peel_fed.iter())
                        .filter(|(load, fed)| **load != 0.0 || **fed)
                        .count() as u64;
                });
            }
            {
                let _fill_span = crate::perf::span(crate::perf::Span::PassDense);
                if dense {
                    load_out.iter_mut().for_each(|value| *value = 0.0);
                    peel_fed.fill(false);
                    layer_visits.fill += 2 * cells as u64;
                } else if round == 0 {
                    // `load_out` holds the raster two layers up and
                    // `peel_fed` the flags the last layer's max loop set (its
                    // carriers).
                    for &cell in &dirty_out {
                        load_out[cell as usize] = 0.0;
                    }
                    for &cell in &carrier_list_above {
                        peel_fed[cell as usize] = false;
                    }
                    layer_visits.fill += (dirty_out.len() + carrier_list_above.len()) as u64;
                    dirty_out.clear();
                } else {
                    // What the last round deposited on and flagged: its
                    // carriers (the strays were zeroed by the max loop).
                    for &cell in &carrier_list {
                        load_out[cell as usize] = 0.0;
                        peel_fed[cell as usize] = false;
                    }
                    layer_visits.fill += 2 * carrier_list.len() as u64;
                }
            }
            // THIS ROUND'S CARRIER LIST: the layer's model cells and the arms'
            // cells that stand outside them, ascending.
            if !dense {
                extras_sorted.clear();
                extras_sorted.extend(
                    carrier_extras
                        .iter()
                        .copied()
                        .filter(|&cell| !masks.carry[cell as usize]),
                );
                sort_dedup(&mut extras_sorted);
                if sabotage::dropped(sabotage::EXTRA_CARRIERS) {
                    extras_sorted.clear();
                }
                union_into(carry_cells, &extras_sorted, &mut carrier_list);
                if sabotage::dropped(sabotage::CARRIER_HEAD) && !carrier_list.is_empty() {
                    carrier_list.remove(0);
                }
            }
            stray.clear();
            incoming = 0.0;
            placed = 0.0;
            lost = 0.0;
            tip_root_load = 0.0;
            misrouted = 0.0;
            spread_hits = 0_u64;
            max_hits = 0_u64;
            injected_here = 0.0_f64;
            worst_gap = 0.0;
            any_lost = false;
            unrooted_rerouted = 0.0_f64;
            unrooted_kept = 0.0_f64;
            held_reads = 0_u64;
            held_load = 0.0_f64;
            // The roots: every carrier, less the unrooted model columns. A
            // round can add carriers (the insertion below), so this is
            // rebuilt per round; it is the carrier raster itself wherever the
            // rule has nothing to refuse.
            let roots_used = rule_active && !unrooted_here.is_empty();
            let offered: &[bool] = if roots_used {
                if dense {
                    roots.copy_from_slice(&carriers);
                } else {
                    for &cell in &carrier_list {
                        roots[cell as usize] = carriers[cell as usize];
                    }
                }
                for &cell in unrooted_here {
                    let cell = cell as usize;
                    sent_sideways[cell] = false;
                    handed[cell] = false;
                    if refused_root[cell] {
                        roots[cell] = false;
                    }
                }
                &roots
            } else {
                &carriers
            };
            let route_started = trace.is_some().then(std::time::Instant::now);
            let route_span = crate::perf::span(crate::perf::Span::PassRoute);
            let route = route_within_cover(
                grid,
                cover_above,
                offered,
                &tip_root,
                RouteActive {
                    cover_cells: cover_cells_above,
                    tip_cells: if sabotage::dropped(sabotage::TIP_CELLS) {
                        &[]
                    } else {
                        &tip_cells
                    },
                    dense,
                },
                if sizing.straight_reach {
                    (sizing.lamina_reach_mm + grid.pitch_mm()) as f32
                } else {
                    f32::NEG_INFINITY
                },
                RouteFields {
                    distance: &mut distance,
                    nearest: &mut nearest,
                    via_tip: &mut via_tip,
                    previous: &mut distance_prev,
                    anchor: &mut route_anchor,
                    clear: route_clear,
                },
            );
            drop(route_span);
            if roots_used && !dense {
                // The roots copy is scratch: leave it all false again.
                for &cell in &carrier_list {
                    roots[cell as usize] = false;
                }
            }
            layer_visits.route += route.visited;
            if let (Some(t), Some(started)) = (trace.as_mut(), route_started) {
                // The counting inside the call is not route time.
                let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
                t.cur.route_ns += elapsed.saturating_sub(route.counting_ns);
                t.cur.count_ns += route.counting_ns;
                t.cur.route_rounds += route.rounds;
                t.cur.route_relax += route.relax_calls;
                t.cur.route_elig += route.eligible;
                t.cur.route_written += route.written;
            }
            outcome.route_sightlines_clear += route.sightlines.clear;
            outcome.route_sightlines_blocked += route.sightlines.blocked;
            route_converged = route.converged;
            let any_root = route.any_root;
            // THE ROUTING THIS LAYER ENDS WITH, recorded for the per-event
            // pass: every loaded cell of the layer above that does not go
            // straight down, and where it goes. Each round re-routes, so each
            // round's record replaces the last and the final round's stands.
            if let Some(log) = log.as_deref_mut() {
                log.hops[layer].clear();
            }
            // A HANDED column one layer up -- one whose carried load went
            // sideways there -- holds nothing in this raster, but it holds its
            // own layer's peel event, which has not gone anywhere yet. Where
            // that event goes is decided here, by the same rule:
            //
            // * still unrooted, with a rooted carrier in reach: sideways, like
            //   the load above it. The raster moves nothing (the larger event
            //   already went that way), and the hop is recorded so the
            //   per-event pass, which does route this event, sends it there
            //   too rather than down into the unrooted material;
            // * anywhere else (rooted here, no rooted carrier in reach, or the
            //   column ends): the event goes on as the cell's own load always
            //   did, so it RE-ENTERS the raster here and is counted as injected
            //   -- it is peel the raster now carries that it did not carry
            //   before, and from here the column is an ordinary one. Decided
            //   once per layer, in the first round; a later round only adds
            //   roots.
            if rule_active {
                for &cell in &handed_prev {
                    let target = if any_root { nearest[cell] } else { u32::MAX };
                    let reachable = target != u32::MAX
                        && f64::from(distance[cell]) <= sizing.lamina_reach_mm + grid.pitch_mm();
                    if refused_root[cell] && reachable {
                        // With load (a lander put some here), the loop below
                        // sends it and records the hop.
                        if load_in[cell] <= 0.0 {
                            sent_sideways[cell] = true;
                            if let Some(log) = log.as_deref_mut() {
                                log.record_hop(layer, cell, Some(target as usize));
                            }
                        }
                    } else if round == 0 && load_in[cell] < own_peel {
                        reentered_here += own_peel - load_in[cell];
                        reentered_reads += 1;
                        load_in[cell] = own_peel;
                        reentered.push(cell as u32);
                    }
                }
            }
            // A re-entered column may have held nothing: it is a source now.
            if !reentered.is_empty() {
                if !dense && !sabotage::dropped(sabotage::REENTERED) {
                    sort_dedup(&mut reentered);
                    union_into(&sources, &reentered, &mut sources_next);
                    core::mem::swap(&mut sources, &mut sources_next);
                }
                reentered.clear();
            }
            if let Some(t) = trace.as_mut() {
                // The cells the spread loop acts on: those holding load.
                t.counting(|rec| {
                    rec.spread_elig += load_in.iter().filter(|load| **load > 0.0).count() as u64;
                });
            }
            let loop_started = trace.is_some().then(std::time::Instant::now);
            let dense_span = crate::perf::span(crate::perf::Span::PassDense);
            let spread_cells: &[u32] = if dense { &full } else { &sources };
            layer_visits.spread += spread_cells.len() as u64;
            for &cell in spread_cells {
                let cell = cell as usize;
                let load = load_in[cell];
                if load <= 0.0 {
                    continue;
                }
                incoming += load;
                let refused = rule_active && refused_root[cell];
                if carriers[cell] && !refused {
                    load_out[cell] += load;
                    placed += load;
                    continue;
                }
                let target = if any_root { nearest[cell] } else { u32::MAX };
                let reachable = target != u32::MAX
                    && f64::from(distance[cell]) <= sizing.lamina_reach_mm + grid.pitch_mm();
                if refused {
                    if !reachable {
                        // No rooted carrier in reach: the load goes down its
                        // own column, as it always did, and is counted.
                        unrooted_kept += load;
                        load_out[cell] += load;
                        placed += load;
                        if !carriers[cell] {
                            stray.push(cell as u32);
                        }
                        continue;
                    }
                    sent_sideways[cell] = true;
                    unrooted_rerouted += load;
                }
                spread_hits += 1;
                if let Some(log) = log.as_deref_mut() {
                    log.record_hop(layer, cell, reachable.then_some(target as usize));
                }
                if reachable {
                    // Nearest first, share by area. One raster cell of carrier
                    // has exactly the source cell's own area, so the
                    // accumulation stops at the first carrier and its weight is
                    // `area / area`. The division is kept explicit because it
                    // is the renormalisation sabotage S3 removes.
                    let accumulated = area;
                    let weight = area / accumulated;
                    load_out[target as usize] += load * weight;
                    if !carriers[target as usize] {
                        stray.push(target);
                    }
                    placed += load * weight;
                    if via_tip[cell] {
                        // The root this load found is a leaned arm's own axis
                        // one layer up. Under the seat-blind accounting that
                        // root did not exist, so this mm² either travelled
                        // further to some other carrier or was lost outright.
                        tip_root_load += load * weight;
                    }
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
            //
            // THE LEDGER OF A HANDED COLUMN ([`UnrootedColumns`]). A continuing
            // model column whose carried load the routing just sent sideways
            // arrives here empty, and `max` would put its own peel back: an
            // event the gate below never counts (the column did not start
            // here), which the next layer sends sideways again, once per
            // layer. Such a column keeps its own peel OUT of the raster here
            // and the read is counted. Where that event goes is decided one
            // layer down, by the same rule: sideways again (the raster moves
            // nothing; it is carried under the larger event the column already
            // handed on), or anywhere else, where it re-enters the raster as
            // counted injected peel. Only model columns: an arm reads its own
            // per-layer event as it always has.
            let own = own_peel;
            // The max loop's field before it runs (write-only measurement).
            if let (Some(t), Some(started)) = (trace.as_mut(), loop_started) {
                t.cur.loop_ns += u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
            }
            let max_before: Option<Vec<f64>> = trace.as_mut().map(|t| t.counting(|_| load_out.clone()));
            let max_started = trace.is_some().then(std::time::Instant::now);
            // The cells the max loop acts on: every carrier, and any other
            // cell the spread put load on (it is zeroed and counted lost).
            let max_list: &[u32] = if dense {
                &full
            } else if stray.is_empty() {
                &carrier_list
            } else {
                sort_dedup(&mut stray);
                union_into(&carrier_list, &stray, &mut max_cells);
                &max_cells
            };
            layer_visits.max += max_list.len() as u64;
            for &cell in max_list {
                let cell = cell as usize;
                if carriers[cell] {
                    let injects = !carriers_above[cell] && !continues_above[cell];
                    if rule_active && sizing.handed_column_ledger && !injects {
                        let model_column = masks.carry[cell]
                            && landing_target[cell] == u32::MAX
                            && !growing_here[cell];
                        if model_column && sent_sideways[cell] {
                            handed[cell] = true;
                            if own > load_out[cell] {
                                held_reads += 1;
                                held_load += own - load_out[cell];
                            }
                            peel_fed[cell] = load_out[cell] > 0.0;
                            max_hits += 1;
                            continue;
                        }
                    }
                    let combined = load_out[cell].max(own);
                    peel_fed[cell] = load_out[cell] > 0.0 || (injects && combined > load_out[cell]);
                    if injects {
                        injected_here += combined - load_out[cell];
                    }
                    load_out[cell] = combined;
                    max_hits += 1;
                } else {
                    // Load can only be deposited on a carrier; anything that
                    // reached a non-carrier column was MISROUTED and is lost
                    // load, counted as such rather than zeroed in silence
                    // (reviewer finding 2026-09-06: a root naming the wrong
                    // column read green on every conservation check).
                    if load_out[cell] > 0.0 {
                        misrouted += load_out[cell];
                    }
                    load_out[cell] = 0.0;
                }
            }
            if let (Some(t), Some(started)) = (trace.as_mut(), max_started) {
                t.cur.loop_ns += u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
            }
            drop(dense_span);
            if let (Some(t), Some(before)) = (trace.as_mut(), max_before) {
                // Eligible: a carrier, or a cell holding load (the loop clears
                // load off a non-carrier). Written: the load changed.
                t.counting(|rec| {
                    for cell in 0..cells {
                        let touched = carriers[cell] || before[cell] != 0.0;
                        rec.max_elig += u64::from(touched);
                        rec.max_written +=
                            u64::from(before[cell].to_bits() != load_out[cell].to_bits());
                    }
                });
            }

            if !insertion.enabled {
                break;
            }
            let _insertion_span = crate::perf::span(crate::perf::Span::PassInsertion);
            insert.hosts.clear();
            insert.host_load.clear();
            insert.host_limit.clear();
            insert.host_keys.clear();
            match triggers {
                // THE PER-EVENT TRIGGER. The load an arm is relieved for is
                // what ONE peel event routes into it at this layer, measured
                // by the previous pass; the max-field `load_out` here holds
                // the sum of different events' column maxima and would insert
                // arms for load that never arrives at once. Only arms the
                // table names are hosts, and each is relieved once per pass.
                Some(table) => {
                    if inserted_here || table.is_empty() {
                        break;
                    }
                    for &id in &alive_by_layer[layer] {
                        let key = crate::per_event::entry(id as usize, layer);
                        if let Some(&load) = table.get(&key) {
                            let limit = insertion.limit(
                                &struts[id as usize],
                                layer,
                                l_max,
                                sizing.x_allowed,
                            );
                            if load > limit {
                                insert.hosts.push(struts[id as usize].cell_at(layer));
                                insert.host_load.push(load);
                                insert.host_limit.push(limit);
                                insert.host_keys.push(key);
                            }
                        }
                    }
                    if insert.hosts.is_empty() {
                        break;
                    }
                }
                // SABOTAGE: today's trigger, the max-field load.
                None => {
                    // Counted HERE, after the enable gate, so it is a witness
                    // that the area law was consulted on this layer and not
                    // merely that the down-sweep looped.
                    outcome.insertion_rounds += 1;
                    for &id in &alive_by_layer[layer] {
                        insert.hosts.push(struts[id as usize].cell_at(layer));
                    }
                    for &index in &growing {
                        insert.hosts.push(growing_cell(&struts[index]));
                    }
                    for &cell in &insert.hosts {
                        insert.host_load.push(load_out[cell]);
                        insert.host_limit.push(l_max);
                    }
                    if !insert.host_load.iter().any(|&load| load > l_max) {
                        break;
                    }
                }
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
                cover_above,
                &mut carriers,
                &load_in,
                spread_cells,
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
                if let Some(t) = trace.as_mut() {
                    t.cur.exhausted += 1;
                    t.cur.hosts += insert.hosts.len() as u64;
                    t.cur.round = round as u64;
                }
                outcome.insertion_geometry_exhausted += 1;
                refused.extend(
                    insert
                        .host_keys
                        .iter()
                        .zip(&insert.host_limit)
                        .map(|(key, limit)| (*key, limit.to_bits())),
                );
                break;
            }
            // The arms just placed stand in cells that were not carriers: they
            // are in the next round's carrier list.
            for &index in growing.iter().rev().take(added as usize) {
                carrier_extras.push(growing_cell(&struts[index]) as u32);
            }
            outcome.arms_inserted_by_load += added;
            outcome.inserted_by.append(&mut insert.placed_by);
            if let Some(t) = trace.as_mut() {
                t.cur.inserted += added;
                t.cur.hosts += insert.hosts.len() as u64;
                t.cur.round = round as u64;
            }
            inserted_here = triggers.is_some();
            round += 1;
        }
        if !dense {
            // The routing field is scratch across layers: leave the cells it
            // set at their defaults for the next layer's call.
            for &cell in cover_cells_above.iter().chain(&tip_cells) {
                let cell = cell as usize;
                distance[cell] = f32::INFINITY;
                nearest[cell] = u32::MAX;
                via_tip[cell] = false;
                route_anchor[cell] = crate::reach::NO_ANCHOR;
            }
            layer_visits.fill += (cover_cells_above.len() + tip_cells.len()) as u64;
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
        outcome.uncarried_load_mm2 += lost + misrouted;
        outcome.misrouted_load_mm2 += misrouted;
        outcome.tip_column_root_load_mm2 += tip_root_load;
        outcome.unrooted_load_rerouted_mm2 += unrooted_rerouted;
        outcome.unrooted_load_kept_mm2 += unrooted_kept;
        outcome.handed_columns_held += held_reads;
        outcome.handed_own_withheld_mm2 += held_load;
        outcome.handed_events_reentered += reentered_reads;
        outcome.handed_reentered_mm2 += reentered_here;
        outcome.spread_search_hits += spread_hits;
        outcome.max_combine_hits += max_hits;
        // A handed column's held event that re-entered the raster is peel put
        // INTO the raster here, and the conservation total books it so.
        outcome.injected_load_mm2 += injected_here + reentered_here;
        if any_lost || misrouted > 0.0 {
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
        arm_cells.clear();
        for &index in growing.iter().rev() {
            let strut = &struts[index];
            let cell = growing_cell(strut);
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
        // A CHILD MUST BE READ BEFORE THE TRUNK IT LANDS ON. Both are
        // terminating arms, so both are in pass 0, and the pass 0 order was
        // "growing arms, then alive arms by id". A FIXED child whose id is
        // higher than its parent's -- which a re-run of this sweep produces the
        // moment a grown arm is fixed and re-read -- was then processed AFTER
        // the parent had already drained `pending`, and its load was written
        // into a slot nobody would ever read again. Silent, and unbounded.
        //
        // The order is derived rather than assumed: an edge runs from a child
        // to the parent it lands on WHEN THAT PARENT ALSO TERMINATES ON THIS
        // LAYER (a parent that does not is read one layer further down, where
        // `pending` still holds the child's load), and Kahn's algorithm emits
        // children first. A cycle is physically impossible -- a member cannot
        // land on itself through a chain -- and if one ever appeared the
        // leftovers are appended in their original order rather than dropped,
        // because a lost arm is worse than a mis-ordered one.
        pass_order.clear();
        pass_indegree.clear();
        for (position, arm) in layer_arms.iter().enumerate() {
            if arm.terminating {
                pass_order.push(position);
            }
        }
        pass_indegree.resize(pass_order.len(), 0_u32);
        if pass_order.len() > 1 {
            pass_slot.clear();
            pass_slot.resize(struts.len(), u32::MAX);
            for (slot, &position) in pass_order.iter().enumerate() {
                pass_slot[layer_arms[position].index] = u32::try_from(slot).unwrap_or(u32::MAX);
            }
            for &position in pass_order.iter() {
                if let Landing::Strut { id, layer: at } = layer_arms[position].landing {
                    if at == layer {
                        if let Some(&parent_slot) = pass_slot.get(id as usize) {
                            if parent_slot != u32::MAX {
                                pass_indegree[parent_slot as usize] += 1;
                            }
                        }
                    }
                }
            }
            let mut emitted = Vec::with_capacity(pass_order.len());
            let mut queue: Vec<usize> = (0..pass_order.len())
                .filter(|&slot| pass_indegree[slot] == 0)
                .collect();
            let mut head = 0_usize;
            while head < queue.len() {
                let slot = queue[head];
                head += 1;
                emitted.push(pass_order[slot]);
                if let Landing::Strut { id, layer: at } = layer_arms[pass_order[slot]].landing {
                    if at == layer {
                        if let Some(&parent_slot) = pass_slot.get(id as usize) {
                            if parent_slot != u32::MAX {
                                let parent_slot = parent_slot as usize;
                                pass_indegree[parent_slot] -= 1;
                                if pass_indegree[parent_slot] == 0 {
                                    queue.push(parent_slot);
                                }
                            }
                        }
                    }
                }
            }
            if emitted.len() < pass_order.len() {
                for slot in 0..pass_order.len() {
                    if pass_indegree[slot] != 0 {
                        emitted.push(pass_order[slot]);
                    }
                }
                outcome.merge_order_cycles += 1;
            }
            pass_order.clone_from(&emitted);
        }
        for pass in 0..2 {
            for step in 0..layer_arms.len() {
                let arm_order = if pass == 0 {
                    match pass_order.get(step) {
                        Some(&position) => position,
                        None => break,
                    }
                } else {
                    step
                };
                let arm = layer_arms[arm_order];
                let index = arm.index;
                if (pass == 0) != arm.terminating {
                    continue;
                }
                let cell = if arm.is_growing {
                    growing_cell(&struts[index])
                } else {
                    struts[index].cell_at(layer)
                };
                // The cell this member reads or lands in: load may sit there
                // at the end of the layer, carrier or not.
                arm_cells.push(cell as u32);
                // TWO DIFFERENT QUANTITIES, AND FOLDING THEM TOGETHER WAS A
                // DEFECT. `own` is what this layer's own peel event routes to
                // this arm's cell -- one of a sequence of events the arm must
                // survive the LARGEST of. `merged` is what a child arm that
                // landed on this trunk hands it, and that is not an event of
                // this arm's at all: it is a second load arriving at the same
                // instant, through a different member. Until 2026-09-04 both
                // went into one `share` and were MAXed against the load from
                // above, so a trunk already carrying more than its child handed
                // it took ZERO from the merge and printed under-sized, with
                // nothing red: the layer conservation assertion is computed
                // before this point, and `overloaded` reads the same
                // under-stated number.
                //
                // AND ONLY AN ARM READS A PEEL EVENT (2026-09-29). A brace or a
                // mast is lateral stiffening; the carrier pass above already
                // refuses to offer either one as a carrier. But a brace ENDS
                // in an arm's cell — its lower end on the arm's shaft, its top
                // at its seat arm — and reading `load_out` there took the
                // arm's own per-layer event, which the arm's `max` over its
                // events already covers and the injection gate never counted,
                // and handed it back to the arm through `pending`, SUMMED on
                // top. Each brace landing grew its trunk by one cell's event
                // that was never put into the raster, so the plate saw more
                // than was injected (`make_brace_pair` -0.0275 mm², eleven
                // events; on `make_laid_tube` enough to grow a root under a
                // mast that had been placed clear of it). A non-arm member
                // leaves the cell's load where it is, for the arm that stands
                // there, and carries only what lands on it.
                let reads_peel = struts[index].kind == crate::StrutKind::Arm;
                let own = if reads_peel { load_out[cell] } else { 0.0 };
                // A LANDER'S LOAD NEVER SIZES THE CONTACT (2026-09-29). An
                // arm's top slot is its CONTACT: `tip_radius_mm` reads it, the
                // contact ball is drawn from it, and the scar identity sums it.
                // A child that lands on this arm's head (at its `k_top`, or at
                // its top alive layer) hands its load to `pending`, and reading
                // that here put the child's load into the slot that sizes the
                // PARENT's contact with the model. That load never crosses the
                // parent's scar: it enters the head and goes down the shaft.
                // Measured on the Round 25 ring stiffener: arm 39's ball drawn
                // at 0.746 mm where its own peel asks for 0.400 mm, and the
                // scar identity read 1.10 on a double count
                // (`lanes/wall-creep-diagnosis.md`).
                //
                // So at the contact slot of a support arm the merged load is
                // left in `pending` and read ONE SLOT LOWER, by the shaft under
                // the contact, which then carries the sum. Only an arm has a
                // contact: a brace's or a mast's top slot is member, and keeps
                // the load. An arm that terminates at its own tip layer has no
                // slot below, so there the load stays in its one slot, as
                // before, and is counted. Sabotage: `defer_to_shaft = false`
                // turns `contact_sizing::` red.
                let at_contact = reads_peel && layer + 1 == struts[index].k_top;
                // AND NO LOAD ENTERS A HEADED ARM BETWEEN ITS CONTACT AND ITS
                // JOINT (v19 fix, 2026-10-08; physics: a load that joins in the
                // head span does not pass through the contact). It waits in
                // `pending` until the arm reaches its joint's layer, so the
                // tip, the head slots and the joint's own lever are sized for
                // the arm's own load, and the shaft below the joint carries
                // the sum. Same hold as the contact slot's above.
                let in_head_span = reads_peel
                    && struts[index]
                        .joint_hold_layer()
                        .is_some_and(|joint| layer > joint);
                let defer_to_shaft = (at_contact || in_head_span) && !arm.terminating;
                let merged = if defer_to_shaft {
                    if pending[index] > 0.0 {
                        if at_contact {
                            outcome.contact_merges_moved_below += 1;
                            outcome.contact_merge_load_moved_mm2 += pending[index];
                        } else if struts[index].joint_hold_layer().is_some_and(|joint| layer == joint + 1)
                        {
                            // Counted once per held load: at the last layer
                            // before the joint, where it is released.
                            outcome.head_span_merges_held_to_joint += 1;
                            outcome.head_span_load_held_mm2 += pending[index];
                        }
                    }
                    0.0
                } else {
                    if at_contact && pending[index] > 0.0 {
                        outcome.contact_merges_kept_at_single_slot += 1;
                    }
                    core::mem::take(&mut pending[index])
                };
                if reads_peel {
                    load_out[cell] = 0.0;
                }
                // WHO READ THIS CELL, AND IN WHAT ORDER, for the per-event
                // pass: the first arm took the cell's load, and a member that
                // lands on the model here hands its own back to whoever reads
                // after it.
                if let Some(log) = log.as_deref_mut() {
                    log.record_read(layer, cell, index, reads_peel);
                }
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
                // `+` did (`max(previous, own) >= previous`), so `size_arm`'s
                // flare property and the monotonicity assertion are unchanged;
                // and `merged` is non-negative, so adding it keeps that too.
                //
                // The max is over THIS ARM'S OWN events; a merged child's load
                // is summed on top of the winner, because both members are
                // loaded by the same peel event and the junction carries their
                // sum. Sabotage: dropping `+ merged` restores the defect and
                // must turn `a_merge_onto_a_trunk_already_carrying_more_still_grows_it`
                // red.
                let carried = combine_carried(previous, own, merged);
                // TWO PEEL EVENTS IN ONE ARM, AND THE SMALLER IS CARRIED UNDER
                // THE LARGER. When the load read here was put into the raster
                // (it is not the arm's own per-layer duplicate), `max` keeps
                // one of two events that were both injected, so the plate
                // reaction books one of them and the injected total books both.
                // That is the max-over-events rule working as designed — the
                // events peel at different instants and the arm is sized for
                // the larger — but it is a term of the whole-run identity, so
                // it is named here: `injected = plate + uncarried + absorbed`,
                // and whatever is left over is load that went nowhere
                // (`landing_check`). `O(1)` per read; the flag is cleared so a
                // second member in the cell cannot count it again.
                if reads_peel {
                    // Injected peel in: what the arm carried from above plus
                    // what it reads here if that was put into the raster.
                    // Out: the max. The difference is the absorbed event (or,
                    // negative, a non-injected per-layer duplicate that
                    // out-weighed everything the arm carried from above).
                    let fed = if peel_fed[cell] { own } else { 0.0 };
                    let absorbed = previous + fed - previous.max(own);
                    if absorbed != 0.0 {
                        outcome.absorbed_event_load_mm2 += absorbed;
                        outcome.absorbed_event_reads += 1;
                    }
                }
                peel_fed[cell] = false;
                // D3x: a member the base walked the same way must write the
                // ledger the base wrote, or the base's tail is not this walk's.
                if track_ledger
                    && !arm.is_growing
                    && same.get(index).copied().unwrap_or(false)
                    && struts[index].carried_mm2[slot].to_bits() != carried.to_bits()
                {
                    ledger_clean = false;
                }
                struts[index].carried_mm2[slot] = carried;
                // THE AREA LAW SIZES AN ARM; A BRACE AND A MAST ARE SIZED BY THEIR
                // OWN STRAIGHTNESS. `min_radius_mm` is zero for every support
                // arm, so this `max` is the identity on every field the
                // slenderness lane did not touch. Where it is not zero it is a
                // MINIMUM and never a cap: a brace that some load does reach
                // still grows under the area law above it.
                // AND THE SLENDERNESS LANE'S THICKEN RUNG (2026-09-29): a
                // per-layer MINIMUM, empty on every member the lane did not
                // thicken, non-decreasing downward by construction, so the
                // profile keeps its shape.
                let thickened = struts[index]
                    .thickened_mm
                    .get(layer)
                    .copied()
                    .unwrap_or(0.0);
                let radius = size_arm(carried, sizing)
                    .max(struts[index].min_radius_mm)
                    .max(thickened);
                if track_ledger
                    && !arm.is_growing
                    && same.get(index).copied().unwrap_or(false)
                    && struts[index].radius_mm[slot].to_bits() != radius.to_bits()
                {
                    ledger_clean = false;
                }
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
                        // The path was written top-down beside the ledger and
                        // is flipped with it, so `cell_at` reads the same
                        // `k_bottom`-upward layout as `radius_mm`.
                        struts[index].path.reverse();
                    }
                }
            }
        }
        // Growing arms that landed leave the growing set, in one retain rather
        // than a per-arm removal.
        growing.retain(|&index| struts[index].k_bottom == usize::MAX);

        // The reuse probe's band hashes of this layer, after the member phase
        // (write-only; pass 1 of a per-event run, `PLINTH_PERF_BANDS=1`).
        if probe_bands {
            let _hash_span = crate::perf::span(crate::perf::Span::PassBandHash);
            let arm_values: Vec<(usize, u64)> = layer_arms
                .iter()
                .filter(|arm| !arm.is_growing)
                .map(|arm| {
                    let strut = &struts[arm.index];
                    let value = crate::reuse_probe::slot_value(
                        strut.carried_mm2.get(arm.slot).copied().unwrap_or(0.0),
                        strut.radius_mm.get(arm.slot).copied().unwrap_or(0.0),
                    );
                    (strut.cell_at(layer), value)
                })
                .collect();
            crate::reuse_probe::layer_done(layer, grid.cols(), &load_out, &carriers, &arm_values);
        }

        if let Some(t) = trace.as_mut() {
            t.counting(|rec| rec.fp = crate::reuse_probe::layer_fingerprint(&load_out, &carriers));
        }
        // --- the model's own section is checked, never resized --------------
        let tail_started = trace.is_some().then(std::time::Instant::now);
        let end_span = crate::perf::span(crate::perf::Span::PassDense);
        if model_cells > 0 {
            check_model_sections(
                grid,
                &masks.carry,
                carry_cells,
                reader.components(layer),
                &load_out,
                layer,
                sizing.x_allowed,
                &mut component_loads,
                &mut outcome.model_overload,
                &mut outcome.model_section_max_x,
            );
            layer_visits.tail += carry_cells.len() as u64;
        }

        if !dense {
            // WHERE THIS LAYER LEFT LOAD, for the next one: every carrier and
            // every cell an arm reads or lands in. Nothing else holds load
            // (the max loop zeroed the rest). `dirty_out` is empty here (the
            // first round's clear emptied it); it becomes the list its raster
            // is cleared by once the rasters swap, and the live cells of it
            // the next layer's spread sources.
            sort_dedup(&mut arm_cells);
            if sabotage::dropped(sabotage::ARM_CELLS) {
                arm_cells.retain(|&cell| carriers[cell as usize]);
            }
            union_into(&carrier_list, &arm_cells, &mut dirty_out);
            sources_next.clear();
            sources_next.extend(
                dirty_out
                    .iter()
                    .copied()
                    .filter(|&cell| live(load_out[cell as usize])),
            );
            layer_visits.tail += dirty_out.len() as u64;
            core::mem::swap(&mut dirty_in, &mut dirty_out);
            core::mem::swap(&mut sources, &mut sources_next);
        }
        std::mem::swap(&mut load_in, &mut load_out);
        // The end-of-layer copies' cells (write-only measurement): eligible =
        // set before or after, written = changed.
        let tail_before = tail_started.map_or(0, |started| {
            u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
        });
        let copy_cells = trace.as_mut().map(|t| {
            t.counting(|_| {
                let (mut eligible, mut written) = (0_u64, 0_u64);
                for cell in 0..cells {
                    let (cover_was, cover_is) = (cover_above[cell], masks.cover[cell]);
                    let (carry_was, carry_is) = (carriers_above[cell], carriers[cell]);
                    eligible += u64::from(cover_was || cover_is || carry_was || carry_is);
                    written += u64::from(cover_was != cover_is || carry_was != carry_is);
                }
                (eligible, written)
            })
        });
        // This layer's cover is the next iteration's "above": the cache hands
        // it over with no copy. This layer's carriers are too, and `carriers`
        // is re-filled from the next layer's mask before anything reads it,
        // so the two buffers swap roles instead of one being copied into the
        // other.
        let copy_started = trace.is_some().then(std::time::Instant::now);
        std::mem::swap(&mut carriers_above, &mut carriers);
        core::mem::swap(&mut carrier_list_above, &mut carrier_list);
        drop(end_span);
        if let (Some(t), Some(started), Some((eligible, written))) =
            (trace.as_mut(), copy_started, copy_cells)
        {
            t.cur.tail_ns = tail_before
                + u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
            t.cur.copy_elig += eligible;
            t.cur.copy_written += written;
        }
        if rule_active {
            handed_prev.clear();
            for &cell in unrooted_here {
                let cell = cell as usize;
                if handed[cell] {
                    handed_prev.push(cell);
                }
                handed[cell] = false;
                refused_root[cell] = false;
                sent_sideways[cell] = false;
            }
            for &cell in &growing_marked {
                growing_here[cell] = false;
            }
            growing_marked.clear();
        }
        if let (Some((old_hops, old_reads)), Some(log)) = (layer_log_before, log.as_deref()) {
            if log.hops[layer] != old_hops || log.reads[layer] != old_reads {
                log_dirty = true;
            }
        }
        visits.add(&layer_visits);
        if let Some(t) = trace.as_mut() {
            t.cur.route_visit = layer_visits.route;
            t.cur.spread_visit = layer_visits.spread;
            t.cur.max_visit = layer_visits.max;
            t.cur.fill_visit = layer_visits.fill;
            t.cur.tail_visit = layer_visits.tail;
            t.layer_end(layer);
        }
    }
    if let Some(t) = trace.take() {
        t.finish();
    }
    visits.publish();
    crate::perf::add(crate::PerfWork::SweepDownLayersWalked, walked);
    crate::perf::add(
        crate::PerfWork::SweepDownLayersSkipped,
        (layer_count as u64).saturating_sub(walked),
    );

    if let Some(stop) = stopped_at {
        // THE EARLY STOP. The state at this boundary is the base's, and no
        // layer below it differs in its inputs, so the walk below it is the
        // base's walk: its ledgers, its routing log (this pass never touched
        // the layers below, so the log still holds it) and its accounting. Only
        // the insertion attempts' counters are this pass's own.
        for (member, (carried, radius)) in base_raw.iter().enumerate() {
            if !same.get(member).copied().unwrap_or(false) {
                continue;
            }
            let strut = &mut struts[member];
            let slots = if strut.k_bottom == usize::MAX {
                0
            } else {
                stop.saturating_sub(strut.k_bottom)
            }
            .min(strut.carried_mm2.len());
            strut.carried_mm2[..slots].copy_from_slice(&carried[..slots]);
            strut.radius_mm[..slots].copy_from_slice(&radius[..slots]);
        }
        let end = base_end.take().expect("a pass that can stop has the base's accounting");
        let mut done = end.clone();
        done.insertion_geometry_exhausted = outcome.insertion_geometry_exhausted;
        done.insertion_rounds_exhausted = outcome.insertion_rounds_exhausted;
        if let Some(mut sealed) = trail.take() {
            for checkpoint in lower.drain(lower_next..) {
                if !sealed.charge(checkpoint.bytes()) {
                    break;
                }
                sealed.checkpoints.push(checkpoint);
            }
            ctl.out = seal_trail(sealed, struts, Some(end));
        }
        ctl.report = PassReport {
            resumed_at,
            stopped_at,
            skipped_all: false,
            walked,
            layers: layer_count as u64,
            log_unchanged: !log_dirty,
            placed_nothing: true,
        };
        crate::perf::add(crate::PerfWork::SweepDownPassesResumed, 1);
        crate::perf::add(crate::PerfWork::SweepDownPassesStopped, 1);
        return done;
    }

    // Anything still in the raster at the bottom of the stack reached the
    // plate through model material rather than through an arm. In sparse mode
    // the cells that can hold load are the live ones of `sources`; the cells
    // left out hold zero, which adds nothing to the sum.
    outcome.plate_reaction_mm2 += if dense {
        load_in.iter().sum::<f64>()
    } else {
        sources.iter().map(|&cell| load_in[cell as usize]).sum::<f64>()
    };
    if !outcome.min_layer_area_mm2.is_finite() {
        outcome.min_layer_area_mm2 = 0.0;
    }
    if tracked {
        let placed_nothing = struts.len() == n_start && outcome.arms_inserted_by_load == 0;
        if let Some(sealed) = trail.take() {
            let end = placed_nothing.then(|| pure_outcome(&outcome));
            ctl.out = seal_trail(sealed, struts, end);
            if ctl.out.is_none() {
                crate::perf::add(crate::PerfWork::SweepDownTrailRefused, 1);
            }
        }
        ctl.report = PassReport {
            resumed_at,
            stopped_at: None,
            skipped_all: false,
            walked,
            layers: layer_count as u64,
            log_unchanged: resumed_at.is_some() && !log_dirty,
            placed_nothing,
        };
        if resumed_at.is_some() {
            crate::perf::add(crate::PerfWork::SweepDownPassesResumed, 1);
        }
    }
    outcome
}

/// Seal a recording pass's trail: the members' ledgers as the walk left them
/// and the accounting it ended with (`None` when it placed an arm). `None`
/// when the pool will not take the ledgers' bytes.
fn seal_trail<'p>(
    mut trail: PassTrail<'p>,
    struts: &[SweepStrut],
    end: Option<DownSweepOutcome>,
) -> Option<PassTrail<'p>> {
    let raw: Vec<(Vec<f64>, Vec<f64>)> = struts
        .iter()
        .map(|strut| (strut.carried_mm2.clone(), strut.radius_mm.clone()))
        .collect();
    if !trail.charge(PassTrail::sealed_bytes(&raw)) {
        return None;
    }
    crate::perf::add(crate::PerfWork::SweepDownTrailBytes, trail.bytes);
    trail.raw = raw;
    trail.end = end;
    Some(trail)
}

/// The cell a still-growing arm occupies at the layer it has just stepped to.
///
/// A growing arm's path is written TOP-DOWN, so its last entry is its current
/// one; `SweepStrut::cell_at` cannot serve here because `k_bottom` is still
/// `usize::MAX` until the descent lands.
fn growing_cell(strut: &SweepStrut) -> usize {
    strut
        .path
        .last()
        .map_or(strut.cell, |cell| *cell as usize)
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
    /// The load that triggers each host's relief, parallel to `hosts`: one
    /// event's routed load in production, the max-field load under sabotage.
    host_load: Vec<f64>,
    /// The load each host is relieved above, parallel to `hosts`: the area
    /// law's `l_max`, or at a tip's own layer no more than the contact that
    /// tip really made carries ([`InsertionPolicy::contact_caps`]).
    host_limit: Vec<f64>,
    /// Each host's per-event trigger key, parallel to `hosts` (empty under
    /// sabotage), so a trigger the geometry refused is not re-tried.
    host_keys: Vec<u64>,
    /// `host_slot[cell]` is the index into `hosts` of the arm sitting in that
    /// cell, or `u32::MAX`. Lets the footprints of EVERY overloaded arm be
    /// collected in ONE pass over the raster instead of one pass per arm.
    host_slot: Vec<u32>,
    buckets: Vec<Vec<usize>>,
    accepted: Vec<usize>,
    /// `(arm id, trigger key)` of the arms the last [`insert_for_load`] call
    /// placed, in order; `u64::MAX` where there is no per-event key.
    placed_by: Vec<(u32, u64)>,
}

impl InsertScratch {
    fn new(cells: usize) -> Self {
        Self {
            hosts: Vec::new(),
            host_load: Vec::new(),
            host_limit: Vec::new(),
            host_keys: Vec::new(),
            host_slot: vec![u32::MAX; cells],
            buckets: Vec::new(),
            accepted: Vec::new(),
            placed_by: Vec::new(),
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
    load_cells: &[u32],
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
    scratch.placed_by.clear();
    if l_max <= 0.0 {
        return 0;
    }
    // One pass to index the hosts, one pass over the raster to bucket every
    // overloaded arm's footprint at once. No per-arm scan of the grid.
    scratch.buckets.clear();
    scratch.buckets.resize(scratch.hosts.len(), Vec::new());
    let mut any_overloaded = false;
    for (slot, &cell) in scratch.hosts.iter().enumerate() {
        if scratch.host_load[slot] > scratch.host_limit[slot] {
            scratch.host_slot[cell] = u32::try_from(slot).unwrap_or(u32::MAX);
            any_overloaded = true;
        }
    }
    if !any_overloaded {
        return 0;
    }
    // Only a cell holding load can be in a footprint: `load_cells` lists every
    // such cell, ascending, so the footprints fill in the order the whole-grid
    // scan filled them.
    for &cell in load_cells {
        let cell = cell as usize;
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
        let routed = scratch.host_load[slot];
        let footprint = core::mem::take(&mut scratch.buckets[slot]);
        if footprint.is_empty() {
            continue;
        }
        let wanted = (routed / scratch.host_limit[slot]).ceil().max(1.0);
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
            // The insertion's own spacing test, unchanged by the 2026-09-30
            // metric change: it is a spacing rule for the area law's extra
            // carriers, not a reach test, and it keeps the chamfer it was
            // built and measured with (`insertion_spacing_mm`).
            if insertion_spacing_mm(grid, cell, host_cell) < separation_mm {
                continue;
            }
            if scratch
                .accepted
                .iter()
                .any(|&taken| insertion_spacing_mm(grid, cell, taken) < separation_mm)
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
                kind: crate::StrutKind::Arm,
                min_radius_mm: 0.0,
                thickened_mm: Vec::new(), foot_radius_mm: None,
                seat_on_strut: None,
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
                covered_demand: Vec::new(), held_births: Vec::new(), twin_host: false,
                inserted_by_load: true,
                // Written TOP-DOWN while the arm grows, reversed with the
                // ledger the moment it lands.
                path: vec![u32::try_from(cell).unwrap_or(u32::MAX)],
                // A down-sweep arm's descent is grown here, not read out of the
                // up-sweep's column history, so there is no recorded straight
                // descent to put it back to and the merge-relief pass leaves it
                // alone by construction.
                seat_face: crate::SeatFace::default(),
                seat_normal: [0.0; 3],
                straight_fallback: None,
                fallback_path: Vec::new(),
                head: None,
            });
            scratch
                .placed_by
                .push((id, scratch.host_keys.get(slot).copied().unwrap_or(u64::MAX)));
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

/// What one arm carries at one layer, from the three quantities that reach it.
///
/// * `previous` — this arm's design load from every peel event ABOVE this layer.
/// * `own` — what THIS layer's peel event routes to this arm's own cell.
/// * `merged` — what a child arm that landed on this trunk hands it.
///
/// `previous` and `own` are the SAME arm's successive events and a print never
/// peels two layers at once, so the arm must survive the larger of them, not
/// their sum. `merged` is not an event of this arm's at all: it is a second
/// load arriving at the same instant through a different member, and a junction
/// carries the sum of the members that meet in it.
///
/// Folding `merged` into `own` and MAXing all three was the shape of this code
/// until 2026-09-04, and it is a silent under-size: a trunk already carrying
/// more from above than its child hands it took ZERO from the merge, with the
/// layer conservation assertion computed before this point and `overloaded`
/// reading the same under-stated number. `a_merge_is_added_to_the_trunk...`
/// below is the drill, and removing `+ merged` here turns it red.
pub(crate) fn combine_carried(previous: f64, own: f64, merged: f64) -> f64 {
    previous.max(own) + merged
}

/// `A = ℓ / X_ALLOWED`, then `r = sqrt(A/π)` inside the registered clamps.
///
/// `carried` is non-decreasing downward, so `r` is too: the arm flares toward
/// its root and its NARROWEST section is at the tip, which is where the law is
/// evaluated and exactly what `docs/physics.md`:89-92 asks for. The law holds
/// at every layer, not only at the contact.
pub(crate) fn size_arm(carried_mm2: f64, sizing: SizingPolicy) -> f64 {
    let required_area = carried_mm2 / sizing.x_allowed;
    let radius = (required_area / core::f64::consts::PI).sqrt();
    radius.clamp(sizing.tip_radius_min_mm, sizing.tip_radius_max_mm)
}

/// Where each loaded cell of layer k+1 sends its load, and how far it has to
/// travel through layer k+1's own material to get there.
///
/// Roots (`distance = 0`) are the cells of `cover_above` whose column is a
/// CARRIER at layer k — model material or a live arm. Travel is restricted to
/// `cover_above` and priced on the same any-angle metric the reach potential
/// uses ([`crate::reach::any_angle_relax`]: the straight length wherever the
/// sightline lies in the cover, one raster step where it does not), so "the
/// down-sweep can route this load" and "the up-sweep called this cell rooted"
/// are one predicate on one metric over one domain. `limit_mm` is the routing
/// bound the caller tests `distance` against; beyond it the field is still an
/// upper bound but is no longer measured exactly. Returns whether any root
/// exists at all.
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
    tip_root: &[u32],
    active: RouteActive<'_>,
    limit_mm: f32,
    fields: RouteFields<'_>,
) -> RouteOutcome {
    let RouteFields {
        distance,
        nearest,
        via_tip,
        previous,
        anchor,
        clear,
    } = fields;
    let cols = grid.cols();
    let rows = grid.rows();
    let pitch = grid.pitch_mm() as f32;
    let mut sight = crate::reach::Sightlines::default();
    let mut any_root = false;
    // One cell of the initialisation. The fields hold their defaults
    // everywhere the caller has not set them (the sparse caller resets what it
    // set after every layer; the dense walk re-initialises every cell here).
    let mut init = |cell: usize,
                    distance: &mut [f32],
                    nearest: &mut [u32],
                    via_tip: &mut [bool],
                    anchor: &mut [u32]| {
        via_tip[cell] = false;
        anchor[cell] = crate::reach::NO_ANCHOR;
        if cover_above[cell] && carriers[cell] {
            distance[cell] = 0.0;
            nearest[cell] = u32::try_from(cell).unwrap_or(u32::MAX);
            anchor[cell] = nearest[cell];
            any_root = true;
        } else if tip_root[cell] != u32::MAX {
            // A LEANED ARM'S OWN AXIS, ONE LAYER UP. The load in this column
            // rests on that arm with no lateral travel at all, so the root sits
            // at distance zero and names the column the arm actually occupies
            // here. The plain carrier test wins where both apply, so this can
            // only ADD roots -- it never redirects load that already had one.
            distance[cell] = 0.0;
            nearest[cell] = tip_root[cell];
            anchor[cell] = u32::try_from(cell).unwrap_or(u32::MAX);
            via_tip[cell] = true;
            any_root = true;
        } else {
            distance[cell] = f32::INFINITY;
            nearest[cell] = u32::MAX;
        }
    };
    // THE CELLS THE FIELD CAN BE SET ON: a root is a covered carrier or a tip
    // column, and a relaxed cell is covered. Every other cell keeps its
    // default, so the sparse walk initialises these and nothing else.
    let mut visited: u64;
    if active.dense {
        for cell in 0..grid.cells() {
            init(cell, distance, nearest, via_tip, anchor);
        }
        visited = grid.cells() as u64;
    } else {
        for &cell in active.cover_cells.iter().chain(active.tip_cells) {
            init(cell as usize, distance, nearest, via_tip, anchor);
        }
        visited = (active.cover_cells.len() + active.tip_cells.len()) as u64;
    }
    // The covered cells the relax rounds walk, ascending.
    let covered: &[u32] = if sabotage::dropped(sabotage::COVER_TAIL) && !active.cover_cells.is_empty() {
        &active.cover_cells[..active.cover_cells.len() - 1]
    } else {
        active.cover_cells
    };
    if !any_root {
        return RouteOutcome {
            any_root: false,
            converged: true,
            sightlines: sight,
            eligible: 0,
            written: 0,
            counting_ns: 0,
            rounds: 0,
            relax_calls: 0,
            visited,
        };
    }
    // Nothing to route where every covered cell is already a root, and a
    // root can never be improved on: the passes would change nothing, so they
    // are not run, and the clearance they would read is not built.
    visited += covered.len() as u64;
    if !covered
        .iter()
        .any(|&cell| cover_above[cell as usize] && distance[cell as usize] > 0.0)
    {
        return RouteOutcome {
            any_root: true,
            converged: true,
            sightlines: sight,
            eligible: 0,
            written: 0,
            counting_ns: 0,
            rounds: 0,
            relax_calls: 0,
            visited,
        };
    }
    // Write-only measurement (M0): the cells the relax rounds can change, and
    // the field they start from. Nothing is read or cloned with counting off;
    // the time it takes is returned so the caller can leave it out of the
    // route time.
    let mut counting_ns = 0_u64;
    let measured = if crate::perf::enabled() {
        let _count_span = crate::perf::span(crate::perf::Span::CountOverhead);
        let started = std::time::Instant::now();
        let eligible = (0..grid.cells())
            .filter(|&cell| cover_above[cell] && distance[cell] > 0.0)
            .count() as u64;
        let initial = distance.to_vec();
        counting_ns += u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        Some((eligible, initial))
    } else {
        None
    };
    // Write-only measurement (M0): relax calls and sweep rounds.
    let mut relax_calls = 0_u64;
    let mut rounds_run = 0_u64;
    // Whether the call improved the cell's distance (a write is always a
    // strict decrease: `any_angle_relax` answers only a price below the
    // cell's current one).
    let mut relax = |cell: usize,
                     from: usize,
                     distance: &mut [f32],
                     nearest: &mut [u32],
                     via_tip: &mut [bool],
                     anchor: &mut [u32]|
     -> bool {
        relax_calls += 1;
        if let Some((value, origin, source)) = crate::reach::any_angle_relax(
            cols,
            pitch,
            cover_above,
            clear,
            distance,
            anchor,
            cell,
            from,
            1.0,
            limit_mm,
            &mut sight,
        ) {
            distance[cell] = value;
            anchor[cell] = origin;
            nearest[cell] = nearest[source];
            // The attribution travels with the root, so a cell served BY a
            // tip-column root through the cover is counted as such and not only
            // the cell that sits directly on the arm.
            via_tip[cell] = via_tip[source];
            true
        } else {
            false
        }
    };
    const MAX_SWEEP_ROUNDS: usize = 8;
    let mut converged = false;
    if active.dense {
        previous.copy_from_slice(distance);
    }
    for _ in 0..MAX_SWEEP_ROUNDS {
        rounds_run += 1;
        visited += 2 * covered.len() as u64;
        // The round's improvements: the sparse walk's stand-in for comparing
        // the field with a snapshot of itself. Exact, because every write is a
        // strict decrease: a cell that was written differs from its value at
        // the start of the round, and a cell that was not is equal to it.
        let mut improved = false;
        // Forward: ascending cell order, which is the dense walk's row-major
        // order; only covered cells do anything.
        for &cell in covered {
            let cell = cell as usize;
            // A root (distance 0) cannot be improved on.
            if !cover_above[cell] || distance[cell] == 0.0 {
                continue;
            }
            let (row, col) = (cell / cols, cell % cols);
            if col > 0 && cover_above[cell - 1] {
                improved |= relax(cell, cell - 1, distance, nearest, via_tip, anchor);
            }
            if row > 0 {
                if cover_above[cell - cols] {
                    improved |= relax(cell, cell - cols, distance, nearest, via_tip, anchor);
                }
                if col > 0 && cover_above[cell - cols - 1] {
                    improved |= relax(cell, cell - cols - 1, distance, nearest, via_tip, anchor);
                }
                if col + 1 < cols && cover_above[cell - cols + 1] {
                    improved |= relax(cell, cell - cols + 1, distance, nearest, via_tip, anchor);
                }
            }
        }
        for &cell in covered.iter().rev() {
            let cell = cell as usize;
            // A root (distance 0) cannot be improved on.
            if !cover_above[cell] || distance[cell] == 0.0 {
                continue;
            }
            let (row, col) = (cell / cols, cell % cols);
            if col + 1 < cols && cover_above[cell + 1] {
                improved |= relax(cell, cell + 1, distance, nearest, via_tip, anchor);
            }
            if row + 1 < rows {
                if cover_above[cell + cols] {
                    improved |= relax(cell, cell + cols, distance, nearest, via_tip, anchor);
                }
                if col + 1 < cols && cover_above[cell + cols + 1] {
                    improved |= relax(cell, cell + cols + 1, distance, nearest, via_tip, anchor);
                }
                if col > 0 && cover_above[cell + cols - 1] {
                    improved |= relax(cell, cell + cols - 1, distance, nearest, via_tip, anchor);
                }
            }
        }
        if active.dense {
            // The walk as it stood: compare the whole field with a snapshot.
            if previous[..] == distance[..] {
                converged = true;
                break;
            }
            previous.copy_from_slice(distance);
        } else if !improved {
            converged = true;
            break;
        }
    }
    let (eligible, written) = measured.map_or((0, 0), |(eligible, initial)| {
        let _count_span = crate::perf::span(crate::perf::Span::CountOverhead);
        let started = std::time::Instant::now();
        let written = initial
            .iter()
            .zip(distance.iter())
            .filter(|(before, after)| before.to_bits() != after.to_bits())
            .count() as u64;
        counting_ns += u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        (eligible, written)
    });
    RouteOutcome {
        any_root: true,
        converged,
        sightlines: sight,
        eligible,
        written,
        counting_ns,
        rounds: rounds_run,
        relax_calls,
        visited,
    }
}

/// The 8-neighbour chamfer distance between two cells, in mm: the spacing
/// metric [`insert_for_load`] was built and measured with. Kept for that one
/// spacing rule when the reach and routing metric became the straight length
/// (2026-09-30); moving it is a separate change with its own measurement
/// (the load-insertion fixture refuses every candidate under the Euclidean
/// spacing, measured in lane lattice-align).
fn insertion_spacing_mm(grid: &Grid, a: usize, b: usize) -> f64 {
    let rows = grid.row_of(a).abs_diff(grid.row_of(b));
    let cols = grid.col_of(a).abs_diff(grid.col_of(b));
    let (long, short) = if rows > cols { (rows, cols) } else { (cols, rows) };
    #[expect(
        clippy::cast_precision_loss,
        reason = "cell offsets are bounded by the grid side (~1e4) and exact in f64"
    )]
    let (long, short) = (long as f64, short as f64);
    grid.pitch_mm() * ((long - short) + core::f64::consts::SQRT_2 * short)
}

/// The per-cell rasters one routing field writes, reused across every layer.
struct RouteFields<'a> {
    distance: &'a mut [f32],
    nearest: &'a mut [u32],
    via_tip: &'a mut [bool],
    previous: &'a mut [f32],
    anchor: &'a mut [u32],
    /// `cover_above`'s clearance field (`reach::clearance_mm`), from the
    /// plan's layer cache: it is a function of the cover alone.
    clear: &'a [f32],
}

/// What one layer's routing field cost to build.
struct RouteOutcome {
    any_root: bool,
    converged: bool,
    sightlines: crate::reach::Sightlines,
    /// Write-only measurement (M0, filled only while `perf` counting is on):
    /// covered cells that are not roots (the cells the relax rounds visit),
    /// and of those, the cells whose distance the rounds changed.
    eligible: u64,
    written: u64,
    /// Time the measurement above took, in ns (zero with counting off).
    counting_ns: u64,
    /// Sweep rounds run and relax calls made (counted always; two integer
    /// adds beside the work each one stands for).
    rounds: u64,
    relax_calls: u64,
    /// Cells the field's initialisation and relax rounds visited.
    visited: u64,
}

/// The cells one routing call can act on (perf lane D2).
#[derive(Clone, Copy)]
struct RouteActive<'a> {
    /// The ascending cells of `cover_above`.
    cover_cells: &'a [u32],
    /// The cells holding a tip-column root; they may lie outside the cover.
    tip_cells: &'a [u32],
    /// `cover_cells` lists every cell of the grid and `previous` holds the
    /// whole field: the walk as it stood before the lists.
    dense: bool,
}

/// Per connected component of MODEL material, `X_model = Σ L / component area`.
///
/// **The model's own material is never sized.** The engine supports models; it
/// does not redesign them. An overloaded section is a warning naming the layer,
/// the component and the realized `X`, on a file that is still emitted.
#[expect(
    clippy::too_many_arguments,
    reason = "a per-layer component check reads the grid, the layer's cached components, the load, \
          its layer, the limit and a reusable scratch buffer, and writes one output list and one \
          maximum; a struct would move the list rather than shorten it"
)]
fn check_model_sections(
    grid: &Grid,
    model: &[bool],
    cells: &[u32],
    components: &Components,
    load: &[f64],
    layer: usize,
    x_allowed: f64,
    loads: &mut Vec<f64>,
    out: &mut Vec<ModelSectionOverload>,
    max_x: &mut f64,
) {
    // `cells` lists the model cells (every cell of the grid, where the caller
    // walks dense), ascending.
    // The components, their numbering and their cell counts are the layer
    // cache's (a function of the model mask alone): `roots` lists the
    // component of each model cell in ascending cell order, which is the order
    // the mask is walked here. What is left is the load sum: ascending cell
    // order within each component, as it always was.
    if components.counts.is_empty() {
        return;
    }
    loads.clear();
    loads.resize(components.counts.len(), 0.0);
    debug_assert_eq!(
        components.roots.len(),
        model.iter().filter(|filled| **filled).count(),
        "one root per model cell"
    );
    let mut roots = components.roots.iter();
    for &cell in cells {
        let cell = cell as usize;
        if !model[cell] {
            continue;
        }
        let &root = roots.next().expect("one root per model cell");
        loads[root as usize] += load[cell];
    }
    let area = grid.cell_area_mm2();
    for component in 0..components.counts.len() {
        if components.counts[component] == 0 {
            continue;
        }
        let component_area = components.counts[component] as f64 * area;
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

#[cfg(test)]
mod cached_components_tests {
    //! The model-section check reads its components from the layer cache. The
    //! ORACLE here is the check as it stood before the cache (labels, union-
    //! find and load sums recomputed per call), copied unchanged; the cached
    //! check must report the same overloads, in the same order, with the same
    //! float bits, on masks with many components, holes and diagonals.

    use super::{check_model_sections, Components, ModelSectionOverload};
    use crate::grid::{lay_out_at, Grid};

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

    /// The pre-cache `check_model_sections`, verbatim.
    fn oracle(
        grid: &Grid,
        model: &[bool],
        load: &[f64],
        layer: usize,
        x_allowed: f64,
        out: &mut Vec<ModelSectionOverload>,
        max_x: &mut f64,
    ) {
        let cols = grid.cols();
        let rows = grid.rows();
        let mut parent: Vec<u32> = Vec::new();
        let mut labels = vec![u32::MAX; grid.cells()];
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
                        labels[cell] = find(&mut parent, a);
                        union(&mut parent, a, b);
                    }
                    (Some(a), None) | (None, Some(a)) => labels[cell] = find(&mut parent, a),
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
            let root = find(&mut parent, labels[cell]) as usize;
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

    /// xorshift64*: deterministic masks and loads, no dependency.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        fn unit(&mut self) -> f64 {
            (self.next() >> 11) as f64 / (1_u64 << 53) as f64
        }
    }

    #[test]
    fn cached_components_report_what_the_per_call_labelling_reported() {
        let grid = lay_out_at(0.0, 0.0, 0.05, 61, 43);
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let mut multi_component = 0_usize;
        let mut overloads_seen = 0_usize;
        for seed_density in [0.15, 0.3, 0.45, 0.55, 0.62, 0.8] {
            for _ in 0..40 {
                let model: Vec<bool> = (0..grid.cells()).map(|_| rng.unit() < seed_density).collect();
                let load: Vec<f64> = (0..grid.cells())
                    .map(|_| if rng.unit() < 0.4 { rng.unit() * 0.9 } else { 0.0 })
                    .collect();
                let mut labels = Vec::new();
                let mut parent = Vec::new();
                let components = Components::of(&grid, &model, &mut labels, &mut parent);
                multi_component += usize::from(components.counts.iter().filter(|c| **c > 0).count() > 1);
                let (mut want, mut want_max) = (Vec::new(), 0.0_f64);
                oracle(&grid, &model, &load, 7, 0.5, &mut want, &mut want_max);
                let (mut got, mut got_max) = (Vec::new(), 0.0_f64);
                let mut scratch = Vec::new();
                // Both walks of the model cells: all of them, and the listed ones.
                let all: Vec<u32> = (0..grid.cells() as u32).collect();
                let listed: Vec<u32> = (0..grid.cells() as u32).filter(|&c| model[c as usize]).collect();
                let (mut listed_got, mut listed_max) = (Vec::new(), 0.0_f64);
                check_model_sections(&grid, &model, &listed, &components, &load, 7, 0.5, &mut scratch, &mut listed_got, &mut listed_max);
                assert_eq!(listed_got.len(), want.len(), "listed overload count");
                for (a, b) in want.iter().zip(&listed_got) {
                    assert_eq!(a.carried_mm2.to_bits(), b.carried_mm2.to_bits());
                    assert_eq!(a.realized_x.to_bits(), b.realized_x.to_bits());
                }
                assert_eq!(want_max.to_bits(), listed_max.to_bits(), "listed max X");
                check_model_sections(&grid, &model, &all, &components, &load, 7, 0.5, &mut scratch, &mut got, &mut got_max);
                assert_eq!(want.len(), got.len(), "overload count");
                for (a, b) in want.iter().zip(&got) {
                    assert_eq!(a.layer, b.layer);
                    assert_eq!(a.component_area_mm2.to_bits(), b.component_area_mm2.to_bits());
                    assert_eq!(a.carried_mm2.to_bits(), b.carried_mm2.to_bits());
                    assert_eq!(a.realized_x.to_bits(), b.realized_x.to_bits());
                }
                assert_eq!(want_max.to_bits(), got_max.to_bits(), "max X");
                overloads_seen += want.len();
            }
        }
        // The comparison has teeth: many layers had several components and
        // some reported overloads.
        assert!(multi_component > 100, "{multi_component} multi-component masks");
        assert!(overloads_seen > 100, "{overloads_seen} overloads");
    }

    #[test]
    fn an_empty_mask_has_no_components_and_reports_nothing() {
        let grid = lay_out_at(0.0, 0.0, 0.05, 8, 8);
        let model = vec![false; grid.cells()];
        let mut labels = Vec::new();
        let mut parent = Vec::new();
        let components = Components::of(&grid, &model, &mut labels, &mut parent);
        assert!(components.counts.is_empty() && components.roots.is_empty());
        let (mut out, mut max_x) = (Vec::new(), 0.0_f64);
        let mut scratch = Vec::new();
        let all: Vec<u32> = (0..grid.cells() as u32).collect();
        check_model_sections(&grid, &model, &all, &components, &vec![1.0; grid.cells()], 0, 0.1, &mut scratch, &mut out, &mut max_x);
        assert!(out.is_empty());
        assert_eq!(max_x.to_bits(), 0.0_f64.to_bits());
    }
}

#[cfg(test)]
mod merge_load_tests {
    use super::{combine_carried, size_arm, SizingPolicy};

    fn sizing() -> SizingPolicy {
        SizingPolicy {
            x_allowed: 12.0,
            tip_radius_min_mm: 0.05,
            tip_radius_max_mm: 1.0,
            owner_growth_ceiling_mm: 0.4,
            alpha: 1.0,
            lamina_reach_mm: 2.13,
            straight_reach: true,
            tip_column_roots: true,
            per_event: true,
            handed_column_ledger: true,
            layer_height_mm: 0.05,
            plate_z_mm: 0.0,
        }
    }

    /// THE DEFECT, AND ITS SABOTAGE. A trunk already carrying 20 mm² from above
    /// takes a child handing it 8 mm² at a layer whose own event is only
    /// 4 mm². The junction must carry 28 mm², and the radius must be exactly
    /// the area law's answer for 28. Under the old `previous.max(own + merged)`
    /// the trunk read 20 and printed at the radius for 20: the merge bought it
    /// nothing, and nothing anywhere went red. Removing `+ merged` from
    /// `combine_carried` reproduces that and turns this test red on both
    /// assertions.
    #[test]
    fn a_merge_is_added_to_the_trunk_even_when_the_trunk_already_carries_more() {
        let previous = 20.0;
        let own = 4.0;
        let merged = 8.0;
        let carried = combine_carried(previous, own, merged);
        assert!(
            (carried - 28.0).abs() < 1.0e-12,
            "a junction carries the sum of the members that meet in it; got {carried}"
        );
        let sizing = sizing();
        let grown = size_arm(carried, sizing);
        let ungrown = size_arm(previous, sizing);
        let expected = (28.0 / sizing.x_allowed / core::f64::consts::PI).sqrt();
        assert!(
            (grown - expected).abs() < 1.0e-12,
            "the trunk must grow to exactly the area law's radius for the sum: {grown} vs {expected}"
        );
        assert!(
            grown > ungrown,
            "the merge must grow the trunk; {grown} is no larger than the {ungrown} it had"
        );
    }

    /// And the max is still a max over the arm's OWN events, so the fix does
    /// not quietly reintroduce sabotage S11's summing of peel events.
    #[test]
    fn the_arms_own_events_are_still_maxed_and_never_summed() {
        assert!((combine_carried(50.0, 10.0, 0.0) - 50.0).abs() < 1.0e-12);
        assert!((combine_carried(10.0, 50.0, 0.0) - 50.0).abs() < 1.0e-12);
        // 500 layers of a 1 mm² event on a 40 mm² arm stay 40 mm², not 500.
        let mut carried = 40.0;
        for _ in 0..500 {
            carried = combine_carried(carried, 1.0, 0.0);
        }
        assert!((carried - 40.0).abs() < 1.0e-12, "peel events must not sum");
    }

    /// The profile must stay non-decreasing downward, which is what
    /// `size_arm`'s flare property and the monotonicity assertion rest on.
    #[test]
    fn the_carried_profile_never_decreases_downward() {
        for &(previous, own, merged) in &[
            (0.0, 0.0, 0.0),
            (50.0, 0.0, 0.0),
            (50.0, 10.0, 8.0),
            (0.0, 3.0, 90.0),
        ] {
            assert!(combine_carried(previous, own, merged) >= previous);
        }
    }
}

#[cfg(test)]
mod sparse_walk_tests {
    //! **The sparse walk equals the dense walk** (perf lane D2, 2026-10-15).
    //!
    //! `sweep_pass_in` walks a pass in two modes. DENSE is the pass as it stood
    //! before the active-cell lists: every loop over every cell, every clear a
    //! fill, the routing field converged against a snapshot of itself. SPARSE
    //! visits only the cells each loop can act on and tracks the cells it
    //! sets. The two must agree BY BITS on everything a pass leaves: the whole
    //! outcome, every member (the complete `Debug`: an `f64` prints its
    //! shortest round-trip form, so equal text is equal bits and `-0.0`
    //! differs from `0.0`), the route log, the refused triggers.
    //!
    //! The fixtures are scenes of convex prisms (rotated rectangles, so
    //! diagonals and corridors), arms straight and leaned, braces, columns the
    //! up-sweep left unrooted, and three ways to drive insertion. 200 are
    //! seeded at random; five are named. Each class of active cell has a
    //! SABOTAGE: drop it from the sparse walk and the comparison must go red.

    use std::collections::HashMap;

    use plinth_mesh::Mesh;
    use plinth_support::perception::{slice_stack, SliceSolid, SliceStackLimits};

    use super::{
        pass_difference, sabotage, sweep_pass_in, ContactCaps, DownSweepOutcome, DrainKeepOut, InsertionPolicy,
        PassCtl, PassMode, SizingPolicy, UnrootedColumns,
    };
    use crate::grid::{lay_out_at, Grid};
    use crate::layer_cache::LayerCache;
    use crate::per_event::{entry, RouteLog};
    use crate::{Landing, SeatFace, StrutKind, SweepStrut};

    const H: f64 = 0.05;
    const PITCH: f64 = 0.1;
    const SIDE: usize = 84;
    const ORIGIN: f64 = -4.2;
    const LAYERS: usize = 30;

    /// xorshift64*: deterministic scenes, no dependency.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        fn unit(&mut self) -> f64 {
            (self.next() >> 11) as f64 / (1_u64 << 53) as f64
        }

        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.unit()
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    fn grid() -> Grid {
        lay_out_at(ORIGIN, ORIGIN, PITCH, SIDE, SIDE)
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a coordinate inside the fixture grid"
    )]
    fn cell_at(x: f64, y: f64) -> usize {
        let (col, row) = (((x - ORIGIN) / PITCH) as usize, ((y - ORIGIN) / PITCH) as usize);
        grid().index(row.min(SIDE - 1), col.min(SIDE - 1))
    }

    /// A convex prism over a counter-clockwise polygon, outward-wound.
    fn prism(poly: &[[f64; 2]], z0: f64, z1: f64) -> Mesh {
        let n = poly.len();
        let mut verts: Vec<[f64; 3]> = poly.iter().map(|p| [p[0], p[1], z0]).collect();
        verts.extend(poly.iter().map(|p| [p[0], p[1], z1]));
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for i in 1..n - 1 {
            tris.push([0, (i + 1) as u32, i as u32]);
            tris.push([n as u32, (n + i) as u32, (n + i + 1) as u32]);
        }
        for i in 0..n {
            let j = (i + 1) % n;
            let (bi, bj, ti, tj) = (i as u32, j as u32, (n + i) as u32, (n + j) as u32);
            tris.push([bi, bj, tj]);
            tris.push([bi, tj, ti]);
        }
        Mesh::new(verts, tris).expect("a closed convex prism")
    }

    fn rectangle(cx: f64, cy: f64, half_a: f64, half_b: f64, angle: f64) -> Vec<[f64; 2]> {
        let (s, c) = angle.sin_cos();
        [[-half_a, -half_b], [half_a, -half_b], [half_a, half_b], [-half_a, half_b]]
            .iter()
            .map(|p| [cx + p[0] * c - p[1] * s, cy + p[0] * s + p[1] * c])
            .collect()
    }

    /// One scene: what the pass reads.
    struct Scene {
        slices: Vec<Vec<SliceSolid>>,
        struts: Vec<SweepStrut>,
        unrooted: UnrootedColumns,
        sizing: SizingPolicy,
        /// 0: per-event with a table of hosts; 1: max-field insertion
        /// (`triggers = None`); 2: per-event with an empty table.
        drive: u8,
        insertion_on: bool,
        /// `(strut, layer)` entries the per-event table names.
        hosts: Vec<(usize, usize)>,
    }

    fn sizing(x_allowed: f64) -> SizingPolicy {
        SizingPolicy {
            x_allowed,
            tip_radius_min_mm: 0.05,
            tip_radius_max_mm: 1.0,
            owner_growth_ceiling_mm: 0.4,
            alpha: 1.0,
            lamina_reach_mm: 2.13,
            straight_reach: true,
            tip_column_roots: true,
            per_event: true,
            handed_column_ledger: true,
            layer_height_mm: H,
            plate_z_mm: 0.0,
        }
    }

    fn strut(id: u32, kind: StrutKind, seat: usize, k_top: usize, path: Vec<u32>) -> SweepStrut {
        SweepStrut {
            id,
            cell: seat,
            center_mm: [0.0, 0.0],
            kind,
            min_radius_mm: if kind == StrutKind::Arm { 0.0 } else { 0.1 },
            thickened_mm: Vec::new(),
            foot_radius_mm: None,
            seat_on_strut: None,
            k_top,
            k_bottom: 0,
            landing: Landing::Plate,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(),
            held_births: Vec::new(),
            twin_host: false,
            inserted_by_load: false,
            path,
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
            seat_face: SeatFace::default(),
            seat_normal: [0.0; 3],
        }
    }

    /// An arm seated at `seat` under layer `k_top`, descending straight or by
    /// a random walk of one-cell steps (`lean` = the chance of a step).
    fn arm(rng: &mut Rng, id: u32, seat: usize, k_top: usize, lean: f64) -> SweepStrut {
        let g = grid();
        let mut path = Vec::new();
        if lean > 0.0 {
            let (mut row, mut col) = (g.row_of(seat), g.col_of(seat));
            let mut walk = vec![0_u32; k_top];
            for layer in (0..k_top).rev() {
                if rng.unit() < lean {
                    col = (col + 1 + rng.below(3)).saturating_sub(2).min(SIDE - 1);
                    row = (row + 1 + rng.below(3)).saturating_sub(2).min(SIDE - 1);
                }
                walk[layer] = u32::try_from(g.index(row, col)).expect("a small grid");
            }
            path = walk;
        }
        strut(id, StrutKind::Arm, seat, k_top, path)
    }

    fn slices_of(prisms: &[(Vec<[f64; 2]>, usize, usize)]) -> Vec<Vec<SliceSolid>> {
        let planes: Vec<f64> = (0..LAYERS).map(|l| (l as f64 + 0.5) * H).collect();
        let mut slices: Vec<Vec<SliceSolid>> = vec![Vec::new(); LAYERS];
        for (poly, z0_layer, z1_layer) in prisms {
            let mesh = prism(poly, *z0_layer as f64 * H, *z1_layer as f64 * H);
            let stack = slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("slices");
            for (layer, solids) in stack.into_iter().enumerate() {
                slices[layer].extend(solids);
            }
        }
        slices
    }

    /// A scene of 2-5 prisms in distinct tiles, arms under the overhanging
    /// ones, and the random extras.
    fn random_scene(seed: u64) -> Scene {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed.wrapping_mul(0xD6E8_FEB8_6659_FD93).wrapping_add(1));
        let mut tiles: Vec<(f64, f64)> = Vec::new();
        for ix in -1..=1 {
            for iy in -1..=1 {
                tiles.push((f64::from(ix) * 2.4, f64::from(iy) * 2.4));
            }
        }
        let count = 2 + rng.below(4);
        let mut prisms = Vec::new();
        let mut struts: Vec<SweepStrut> = Vec::new();
        let mut hosts = Vec::new();
        for _ in 0..count {
            let (cx, cy) = tiles.swap_remove(rng.below(tiles.len()));
            let (a, b) = (rng.range(0.25, 0.85), rng.range(0.15, 0.75));
            let angle = rng.range(0.0, core::f64::consts::PI);
            let z0 = if rng.unit() < 0.25 { 0 } else { 2 + rng.below(12) };
            let z1 = (z0 + 3 + rng.below(12)).min(LAYERS - 1);
            prisms.push((rectangle(cx, cy, a, b, angle), z0, z1));
            if z0 >= 2 {
                for _ in 0..1 + rng.below(3) {
                    let (dx, dy) = (rng.range(-a, a) * 0.8, rng.range(-b, b) * 0.8);
                    let (s, c) = angle.sin_cos();
                    let (x, y) = (cx + dx * c - dy * s, cy + dx * s + dy * c);
                    let lean = if rng.unit() < 0.4 { rng.range(0.1, 0.6) } else { 0.0 };
                    let id = struts.len() as u32;
                    let mut member = arm(&mut rng, id, cell_at(x, y), z0, lean);
                    if rng.unit() < 0.15 {
                        member.k_bottom = rng.below(z0.max(1));
                        if member.k_bottom > 0 && !member.path.is_empty() {
                            // A ledger-length path keeps `cell_at` honest.
                            member.path = member.path[member.k_bottom..].to_vec();
                        }
                    }
                    for layer in member.k_bottom..member.k_top {
                        if rng.unit() < 0.15 {
                            hosts.push((struts.len(), layer));
                        }
                    }
                    struts.push(member);
                }
            }
        }
        // An arm in free space and a brace, now and then.
        if rng.unit() < 0.5 {
            let id = struts.len() as u32;
            let seat = cell_at(rng.range(-3.5, 3.5), rng.range(-3.5, 3.5));
            let k_top = 3 + rng.below(10);
            struts.push(arm(&mut rng, id, seat, k_top, 0.0));
        }
        if rng.unit() < 0.4 {
            let id = struts.len() as u32;
            let seat = cell_at(rng.range(-3.0, 3.0), rng.range(-3.0, 3.0));
            struts.push(strut(id, StrutKind::Brace, seat, 5 + rng.below(8), Vec::new()));
        }
        // Columns the up-sweep left unrooted, on a third of the layers.
        let mut unrooted = UnrootedColumns::with_layers(LAYERS);
        if rng.unit() < 0.6 {
            for layer in 0..LAYERS {
                if rng.unit() < 0.4 {
                    let (x, y) = (rng.range(-3.0, 3.0), rng.range(-3.0, 3.0));
                    let base = cell_at(x, y);
                    let mut cells: Vec<u32> = (0..40)
                        .map(|_| {
                            let (dc, dr) = (rng.below(9), rng.below(9));
                            u32::try_from(base + dr * SIDE + dc).expect("a small grid")
                        })
                        .filter(|&c| (c as usize) < SIDE * SIDE)
                        .collect();
                    cells.sort_unstable();
                    cells.dedup();
                    unrooted.total += cells.len();
                    unrooted.by_layer[layer] = cells;
                }
            }
        }
        let mut sizing = sizing(rng.range(0.15, 3.0));
        sizing.straight_reach = rng.unit() < 0.85;
        sizing.tip_column_roots = rng.unit() < 0.85;
        sizing.handed_column_ledger = rng.unit() < 0.85;
        // A short reach loses the load that is too far from every arm.
        if rng.unit() < 0.6 {
            sizing.lamina_reach_mm = rng.range(0.3, 2.13);
        }
        Scene {
            slices: slices_of(&prisms),
            struts,
            unrooted,
            sizing,
            drive: u8::try_from(rng.below(3)).expect("below 3"),
            insertion_on: rng.unit() < 0.85,
            hosts,
        }
    }

    type Walk = (DownSweepOutcome, Vec<SweepStrut>, RouteLog, Vec<(u64, u64)>);

    fn walk(mode: PassMode, scene: &Scene) -> Walk {
        let g = grid();
        let cache = LayerCache::build(&g, &scene.slices);
        let mut struts = scene.struts.clone();
        let mut log = RouteLog::new(LAYERS);
        let mut refused = Vec::new();
        let insertion = if scene.insertion_on {
            InsertionPolicy::new(1000)
        } else {
            InsertionPolicy::disabled()
        };
        let table: HashMap<u64, f64> = match scene.drive {
            0 => scene.hosts.iter().map(|&(m, l)| (entry(m, l), 1.0e3)).collect(),
            _ => HashMap::new(),
        };
        let triggers = (scene.drive != 1).then_some(&table);
        let outcome = sweep_pass_in(
            mode,
            &cache,
            &mut struts,
            scene.sizing,
            insertion,
            &DrainKeepOut::none(),
            &scene.unrooted,
            triggers,
            (scene.drive != 1).then_some(&mut log),
            &mut refused,
            &mut PassCtl::off(),
        );
        (outcome, struts, log, refused)
    }

    fn compare(scene: &Scene) -> Option<String> {
        let dense = walk(PassMode::Dense, scene);
        let sparse = walk(PassMode::Sparse, scene);
        let given = scene.drive != 1;
        pass_difference(
            (&dense.0, &dense.1, given.then_some(&dense.2), &dense.3),
            (&sparse.0, &sparse.1, given.then_some(&sparse.2), &sparse.3),
        )
    }

    /// What a set of scenes exercised, summed over them.
    #[derive(Default, Debug)]
    struct Reach {
        scenes: usize,
        inserted: u64,
        lost: usize,
        tip_roots: u64,
        refused_roots: u64,
        handed_held: u64,
        reentered: u64,
        rerouted: usize,
        multi_round: usize,
        leaned: usize,
        braces: usize,
    }

    fn reach_of(scene: &Scene, dense: &DownSweepOutcome, into: &mut Reach) {
        into.scenes += 1;
        into.inserted += dense.arms_inserted_by_load;
        into.lost += usize::from(dense.uncarried_load_mm2 > 0.0);
        into.tip_roots += dense.tip_column_roots_offered;
        into.refused_roots += dense.unrooted_columns_refused;
        into.handed_held += dense.handed_columns_held;
        into.reentered += dense.handed_events_reentered;
        into.rerouted += usize::from(dense.unrooted_load_rerouted_mm2 > 0.0);
        into.multi_round += usize::from(dense.insertion_rounds > 1);
        into.leaned += scene.struts.iter().filter(|s| s.is_leaned()).count();
        into.braces += scene.struts.iter().filter(|s| s.kind != StrutKind::Arm).count();
    }

    #[test]
    fn the_sparse_walk_equals_the_dense_walk_on_200_seeded_scenes() {
        let mut reach = Reach::default();
        for seed in 0..200_u64 {
            let scene = random_scene(seed);
            let dense = walk(PassMode::Dense, &scene);
            let sparse = walk(PassMode::Sparse, &scene);
            let given = scene.drive != 1;
            if let Some(why) = pass_difference(
                (&dense.0, &dense.1, given.then_some(&dense.2), &dense.3),
                (&sparse.0, &sparse.1, given.then_some(&sparse.2), &sparse.3),
            ) {
                panic!("seed {seed}: sparse != dense: {why}");
            }
            reach_of(&scene, &dense.0, &mut reach);
        }
        println!("200 scenes reached {reach:?}");
        // The comparison has teeth: the scenes exercised the paths the lists
        // were written for.
        assert!(reach.inserted > 50, "{reach:?}");
        assert!(reach.lost > 20, "{reach:?}");
        assert!(reach.tip_roots > 50, "{reach:?}");
        assert!(reach.refused_roots > 100, "{reach:?}");
        assert!(reach.handed_held > 20, "{reach:?}");
        assert!(reach.reentered > 5, "{reach:?}");
        assert!(reach.rerouted > 5, "{reach:?}");
        assert!(reach.leaned > 50 && reach.braces > 20, "{reach:?}");
    }

    /// One long thin slab: load travels along a corridor to a single arm.
    fn corridor_scene() -> Scene {
        let slab = (rectangle(0.0, 0.0, 3.2, 0.3, 0.0), 8, 11);
        let mut rng = Rng(7);
        let struts = vec![arm(&mut rng, 0, cell_at(-3.0, 0.0), 8, 0.0)];
        Scene {
            slices: slices_of(&[slab]),
            struts,
            unrooted: UnrootedColumns::with_layers(LAYERS),
            // A reach longer than the slab: the arm at one end collects it all.
            sizing: SizingPolicy { lamina_reach_mm: 8.0, ..sizing(0.2) },
            drive: 1,
            insertion_on: true,
            hosts: Vec::new(),
        }
    }

    /// A thin slab on a 30 degree diagonal, an arm at one end: the cover is a
    /// staircase a bounding box would be 20 times too wide for.
    fn diagonal_scene() -> Scene {
        let slab = (rectangle(0.0, 0.0, 3.0, 0.3, 0.55), 8, 11);
        let mut rng = Rng(9);
        let struts = vec![arm(&mut rng, 0, cell_at(-2.4, -1.4), 8, 0.0)];
        Scene {
            slices: slices_of(&[slab]),
            struts,
            unrooted: UnrootedColumns::with_layers(LAYERS),
            sizing: SizingPolicy { lamina_reach_mm: 8.0, ..sizing(0.2) },
            drive: 1,
            insertion_on: true,
            hosts: Vec::new(),
        }
    }

    /// Two slabs that do not touch, one arm under each of three places.
    fn disconnected_scene() -> Scene {
        let slabs = [
            (rectangle(-2.0, 0.0, 0.9, 0.7, 0.3), 6, 9),
            (rectangle(2.0, 1.0, 0.8, 0.8, 1.1), 6, 10),
        ];
        let mut rng = Rng(11);
        let struts = vec![
            arm(&mut rng, 0, cell_at(-2.2, 0.1), 6, 0.0),
            arm(&mut rng, 1, cell_at(2.2, 1.2), 6, 0.0),
            arm(&mut rng, 2, cell_at(1.7, 0.6), 6, 0.0),
        ];
        Scene {
            slices: slices_of(&slabs),
            struts,
            unrooted: UnrootedColumns::with_layers(LAYERS),
            sizing: sizing(0.6),
            drive: 1,
            insertion_on: true,
            hosts: Vec::new(),
        }
    }

    /// A slab held by an arm that leans three cells over its descent, so its
    /// body stands outside the slab's cover and the tip column is the root.
    fn leaned_tip_scene() -> Scene {
        let slab = (rectangle(0.0, 0.0, 1.2, 0.5, 0.0), 12, 15);
        let g = grid();
        let seat = cell_at(0.2, 0.0);
        let (row, col) = (g.row_of(seat), g.col_of(seat));
        let path: Vec<u32> = (0..12)
            .map(|layer| {
                let shift = (12 - layer).min(7);
                u32::try_from(g.index(row + shift, col)).expect("a small grid")
            })
            .collect();
        Scene {
            slices: slices_of(&[slab]),
            struts: vec![strut(0, StrutKind::Arm, seat, 12, path)],
            unrooted: UnrootedColumns::with_layers(LAYERS),
            sizing: sizing(1.0),
            drive: 1,
            insertion_on: true,
            hosts: Vec::new(),
        }
    }

    /// A slab with columns the up-sweep left unrooted: their load goes
    /// sideways to the arm beside them, and their own peel is held out then
    /// re-enters.
    fn handed_column_scene() -> Scene {
        let slab = (rectangle(0.0, 0.0, 1.0, 0.6, 0.0), 6, 16);
        let mut rng = Rng(13);
        let struts = vec![arm(&mut rng, 0, cell_at(0.8, 0.0), 6, 0.0)];
        let mut unrooted = UnrootedColumns::with_layers(LAYERS);
        for layer in 3..14 {
            let cells: Vec<u32> = (0..SIDE * SIDE)
                .filter(|&c| {
                    let (x, y) = (
                        ORIGIN + ((c % SIDE) as f64 + 0.5) * PITCH,
                        ORIGIN + ((c / SIDE) as f64 + 0.5) * PITCH,
                    );
                    // Two bands that come and go with the layer.
                    (x + 0.3 * (layer % 3) as f64) > -0.6 && x < -0.1 && y.abs() < 0.4
                })
                .map(|c| u32::try_from(c).expect("a small grid"))
                .collect();
            unrooted.total += cells.len();
            unrooted.by_layer[layer] = cells;
        }
        Scene {
            slices: slices_of(&[slab]),
            struts,
            unrooted,
            sizing: sizing(0.8),
            drive: 1,
            insertion_on: true,
            hosts: Vec::new(),
        }
    }

    /// A child arm that lands on a BRACE standing in free space, which in turn
    /// lands on the model: the child's load comes out of the brace's cell,
    /// where no carrier stands, at the bottom of the stack.
    fn brace_landing_scene() -> Scene {
        let slab = (rectangle(0.0, 0.0, 1.0, 0.6, 0.0), 10, 13);
        let mut rng = Rng(17);
        let mut under_slab = arm(&mut rng, 0, cell_at(0.0, 0.0), 10, 0.0);
        under_slab.landing = Landing::Plate;
        // The child: alive on layers 3..7, hanging in free space, landing on
        // the brace at layer 3.
        let mut child = arm(&mut rng, 1, cell_at(2.5, 2.0), 7, 0.0);
        child.k_bottom = 3;
        child.landing = Landing::Strut { id: 2, layer: 3 };
        // The brace: alive on 0..6 over a cell no model reaches, "landing" on
        // the model at layer 0.
        let mut brace = strut(2, StrutKind::Brace, cell_at(2.5, 2.0), 6, Vec::new());
        brace.landing = Landing::Model { layer: 0 };
        Scene {
            slices: slices_of(&[slab]),
            struts: vec![under_slab, child, brace],
            unrooted: UnrootedColumns::with_layers(LAYERS),
            sizing: sizing(2.0),
            drive: 1,
            insertion_on: false,
            hosts: Vec::new(),
        }
    }

    fn named() -> Vec<(&'static str, Scene)> {
        vec![
            ("corridor", corridor_scene()),
            ("diagonal", diagonal_scene()),
            ("disconnected", disconnected_scene()),
            ("leaned tip", leaned_tip_scene()),
            ("handed column", handed_column_scene()),
            ("brace landing", brace_landing_scene()),
        ]
    }

    #[test]
    fn the_sparse_walk_equals_the_dense_walk_on_the_named_fixtures() {
        for (name, scene) in named() {
            let dense = walk(PassMode::Dense, &scene);
            if let Some(why) = compare(&scene) {
                panic!("{name}: sparse != dense: {why}");
            }
            let outcome = &dense.0;
            println!(
                "{name}: inserted {}, uncarried {:.3}, tip roots {}, refused {}, held {}, reentered {}, \
                 rounds {}",
                outcome.arms_inserted_by_load,
                outcome.uncarried_load_mm2,
                outcome.tip_column_roots_offered,
                outcome.unrooted_columns_refused,
                outcome.handed_columns_held,
                outcome.handed_events_reentered,
                outcome.insertion_rounds,
            );
            // Anti-vacuity: each fixture reaches what its name says.
            match name {
                "corridor" | "diagonal" => {
                    assert!(outcome.spread_search_hits > 0, "{name}: nothing spread");
                    assert!(outcome.arms_inserted_by_load > 0, "{name}: no insertion");
                }
                "disconnected" => assert!(outcome.spread_search_hits > 0, "{name}"),
                "brace landing" => assert!(
                    outcome.landing_injections >= 2 && outcome.plate_reaction_mm2 > 0.0,
                    "{name}: the child's load did not reach the brace"
                ),
                "leaned tip" => assert!(
                    outcome.tip_column_roots_offered > 0 && outcome.tip_column_root_load_mm2 > 0.0,
                    "{name}: the tip column rooted nothing"
                ),
                "handed column" => assert!(
                    outcome.unrooted_columns_refused > 0
                        && outcome.unrooted_load_rerouted_mm2 > 0.0
                        && outcome.handed_columns_held > 0,
                    "{name}: no column was handed"
                ),
                _ => unreachable!(),
            }
        }
    }

    /// SABOTAGE. Leave a class of active cell out of the sparse walk and some
    /// scene must come out different from the dense walk (or the walk must
    /// trip its own conservation assertion): the comparison sees that class.
    fn red_with(class: u16, label: &str) {
        sabotage::set(class);
        let mut seen = 0_usize;
        let scenes = named()
            .into_iter()
            .map(|(_, scene)| scene)
            .chain((0..200).map(random_scene));
        for scene in scenes {
            let dense = walk(PassMode::Dense, &scene);
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| walk(PassMode::Sparse, &scene)));
            match outcome {
                Err(_) => seen += 1,
                Ok(sparse) => {
                    let given = scene.drive != 1;
                    seen += usize::from(
                        pass_difference(
                            (&dense.0, &dense.1, given.then_some(&dense.2), &dense.3),
                            (&sparse.0, &sparse.1, given.then_some(&sparse.2), &sparse.3),
                        )
                        .is_some(),
                    );
                }
            }
        }
        sabotage::set(0);
        println!("sabotage {label}: {seen} scenes differ");
        assert!(seen > 0, "dropping {label} did not change any scene: the comparison cannot see it");
    }

    #[test]
    fn sabotage_dropping_the_arms_outside_the_model_from_the_carrier_list_is_red() {
        red_with(sabotage::EXTRA_CARRIERS, "arm cells outside the model from the carrier list");
    }

    #[test]
    fn sabotage_dropping_reentered_handed_columns_from_the_sources_is_red() {
        red_with(sabotage::REENTERED, "re-entered handed columns from the spread sources");
    }

    #[test]
    fn sabotage_dropping_the_last_cover_cell_from_the_routing_rounds_is_red() {
        red_with(sabotage::COVER_TAIL, "the last covered cell from the routing rounds");
    }

    #[test]
    fn sabotage_dropping_the_first_carrier_is_red() {
        red_with(sabotage::CARRIER_HEAD, "the first carrier cell");
    }

    #[test]
    fn sabotage_dropping_load_cells_that_are_not_carriers_from_the_next_sources_is_red() {
        red_with(sabotage::ARM_CELLS, "arm cells that are not carriers from the next sources");
    }

    /// What one routing call leaves, as comparable bits.
    #[derive(Debug, PartialEq)]
    struct RouteBits {
        distance: Vec<u32>,
        nearest: Vec<u32>,
        via_tip: Vec<bool>,
        anchor: Vec<u32>,
        any_root: bool,
        converged: bool,
        rounds: u64,
        relax_calls: u64,
        clear: u64,
        blocked: u64,
    }

    /// One routing call on random masks, dense or sparse.
    fn route_once(seed: u64, dense: bool) -> RouteBits {
        use super::{route_within_cover, RouteActive, RouteFields};
        let g = lay_out_at(0.0, 0.0, 0.1, 31, 23);
        let cells = g.cells();
        let mut rng = Rng(0xA5A5_5A5A_1234_5678 ^ seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        // A few random blobs and a corridor make the cover; holes in it.
        let mut cover = vec![false; cells];
        for _ in 0..1 + rng.below(4) {
            let (r0, c0) = (rng.below(g.rows()), rng.below(g.cols()));
            let (h, w) = (1 + rng.below(8), 1 + rng.below(14));
            for r in r0..(r0 + h).min(g.rows()) {
                for c in c0..(c0 + w).min(g.cols()) {
                    cover[g.index(r, c)] = true;
                }
            }
        }
        for cell in 0..cells {
            if cover[cell] && rng.unit() < 0.06 {
                cover[cell] = false;
            }
        }
        let mut carriers: Vec<bool> = (0..cells).map(|c| cover[c] && rng.unit() < 0.04).collect();
        // Arm cells outside the cover also stand as carriers.
        for _ in 0..rng.below(4) {
            carriers[rng.below(cells)] = true;
        }
        // Tip roots: some inside the cover, some outside it.
        let mut tip_root = vec![u32::MAX; cells];
        let mut tip_cells = Vec::new();
        for _ in 0..rng.below(5) {
            let above = rng.below(cells);
            if tip_root[above] == u32::MAX {
                tip_root[above] = u32::try_from(rng.below(cells)).expect("a small grid");
                tip_cells.push(u32::try_from(above).expect("a small grid"));
            }
        }
        let mut clear = Vec::new();
        if rng.unit() < 0.7 {
            crate::reach::clearance_mm(&g, &cover, &mut clear);
        }
        let cover_cells: Vec<u32> =
            (0..cells).filter(|&c| cover[c]).map(|c| u32::try_from(c).expect("a small grid")).collect();
        let full: Vec<u32> = (0..u32::try_from(cells).expect("a small grid")).collect();
        let mut distance = vec![f32::INFINITY; cells];
        let mut nearest = vec![u32::MAX; cells];
        let mut via_tip = vec![false; cells];
        let mut anchor = vec![crate::reach::NO_ANCHOR; cells];
        let mut previous = vec![f32::INFINITY; if dense { cells } else { 0 }];
        let limit = if rng.unit() < 0.8 { rng.range(0.3, 3.0) as f32 } else { f32::NEG_INFINITY };
        let outcome = route_within_cover(
            &g,
            &cover,
            &carriers,
            &tip_root,
            RouteActive {
                cover_cells: if dense { &full } else { &cover_cells },
                tip_cells: if sabotage::dropped(sabotage::TIP_CELLS) { &[] } else { &tip_cells },
                dense,
            },
            limit,
            RouteFields {
                distance: &mut distance,
                nearest: &mut nearest,
                via_tip: &mut via_tip,
                previous: &mut previous,
                anchor: &mut anchor,
                clear: &clear,
            },
        );
        RouteBits {
            distance: distance.iter().map(|d| d.to_bits()).collect(),
            nearest,
            via_tip,
            anchor,
            any_root: outcome.any_root,
            converged: outcome.converged,
            rounds: outcome.rounds,
            relax_calls: outcome.relax_calls,
            clear: outcome.sightlines.clear,
            blocked: outcome.sightlines.blocked,
        }
    }

    /// 200 random masks: the routing field the sparse walk builds equals the
    /// dense field on every cell, by bits, with the same rounds and relax calls.
    #[test]
    fn the_sparse_routing_field_equals_the_dense_field_on_200_random_masks() {
        let (mut multi_round, mut tip_outside, mut rooted) = (0, 0, 0);
        for seed in 0..200 {
            let (dense, sparse) = (route_once(seed, true), route_once(seed, false));
            assert_eq!(dense, sparse, "seed {seed}");
            multi_round += usize::from(dense.rounds > 1);
            rooted += usize::from(dense.any_root);
            tip_outside += usize::from(dense.via_tip.iter().any(|&t| t));
        }
        assert!(multi_round > 100 && rooted > 150 && tip_outside > 40, "{multi_round} {rooted} {tip_outside}");
    }

    /// SABOTAGE: without its tip columns the sparse routing field is wrong
    /// wherever a tip root stands outside the cover.
    #[test]
    fn sabotage_dropping_tip_roots_from_the_routing_field_is_red() {
        sabotage::set(sabotage::TIP_CELLS);
        let differing = (0..200).filter(|&seed| route_once(seed, true) != route_once(seed, false)).count();
        sabotage::set(0);
        assert!(differing > 10, "{differing} masks differ");
    }

    /// The sparse walk visits a small share of the cells the dense walk does,
    /// and the counters say so (the witness that the sparse path fired).
    #[test]
    fn the_sparse_walk_visits_a_small_share_of_the_cells() {
        for (name, scene) in named() {
            let _ = walk(PassMode::Dense, &scene);
            let dense = super::LAST_VISITS.with(std::cell::Cell::get);
            let _ = walk(PassMode::Sparse, &scene);
            let sparse = super::LAST_VISITS.with(std::cell::Cell::get);
            println!("{name}: dense {dense:?} sparse {sparse:?}");
            let cell_layers = (grid().cells() * LAYERS) as u64;
            // Dense: the init plus every relax round walk every cell, and the
            // spread and max loops walk every cell, once per insertion round.
            assert!(dense.spread >= cell_layers && dense.max >= cell_layers, "{name}: {dense:?}");
            assert!(dense.route >= cell_layers, "{name}: {dense:?}");
            // Sparse: a few percent of that, loop by loop.
            for (loop_name, sparse_visits, dense_visits) in [
                ("route", sparse.route, dense.route),
                ("spread", sparse.spread, dense.spread),
                ("max", sparse.max, dense.max),
                ("fill", sparse.fill, dense.fill),
            ] {
                assert!(
                    sparse_visits * 5 < dense_visits,
                    "{name}: sparse {loop_name} visited {sparse_visits} of the dense {dense_visits}"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // PERF LANE D3x: a resumed pass equals the full pass.
    //
    // Each scene is walked as a CHAIN of passes the way a call walks them:
    // pass 1 with an empty trigger table (recording a trail), then pass 2 and 3
    // with trigger tables, each resumed from the trail of the pass before. With
    // the shadow check forced on, every resumed pass is also walked in full on a
    // copy and the two must agree by bits on the outcome, every member (ledgers
    // included), the route log, the refusals and the trail they leave. Between
    // pass 1 and pass 2 the members may be EDITED the way a caller edits them
    // between calls (a tip moved, an arm added mid-stack, a member removed and
    // the ids remapped, a minimum raised, a tip lowered). Every invalidation
    // rule has a SABOTAGE.
    // -----------------------------------------------------------------------

    use super::{PassReport, PassTrail, TEST_REPORTS, TEST_STRIDE, TEST_VERIFY};
    use crate::layer_cache::BytePool;

    type Base = Option<(PassTrail<'static>, RouteLog)>;

    /// One pass of `scene`'s field over `struts`, driven by `table`, resumed
    /// from `base` and leaving the trail the next pass resumes from.
    fn chain_pass(
        cache: &LayerCache<'_>,
        scene: &Scene,
        struts: &mut Vec<SweepStrut>,
        table: &HashMap<u64, f64>,
        base: &mut Base,
    ) -> DownSweepOutcome {
        let mut log = RouteLog::new(LAYERS);
        let mut refused = Vec::new();
        let insertion = if scene.insertion_on {
            InsertionPolicy::new(1000)
        } else {
            InsertionPolicy::disabled()
        };
        let mut ctl = super::PassCtl::recording(BytePool::process(), base.take());
        let outcome = super::sweep_pass(
            cache,
            struts,
            scene.sizing,
            insertion,
            &DrainKeepOut::none(),
            &scene.unrooted,
            Some(table),
            Some(&mut log),
            &mut refused,
            &mut ctl,
        );
        if let Some(trail) = ctl.out.take() {
            *base = Some((trail, log));
        }
        outcome
    }

    /// A trigger table over `hosts`, each far over any limit.
    fn table_of(hosts: &[(usize, usize)]) -> HashMap<u64, f64> {
        hosts.iter().map(|&(member, layer)| (entry(member, layer), 1.0e3)).collect()
    }

    /// What the caller may do to the members between two calls.
    fn edit(struts: &mut Vec<SweepStrut>, kind: usize, rng: &mut Rng) {
        if struts.is_empty() {
            return;
        }
        let at = rng.below(struts.len());
        match kind {
            // A tip moved one column over, seat and descent.
            1 => {
                let g = grid();
                let shift = |cell: usize| g.index(g.row_of(cell), (g.col_of(cell) + 1).min(SIDE - 1));
                let member = &mut struts[at];
                member.cell = shift(member.cell);
                for cell in &mut member.path {
                    *cell = u32::try_from(shift(*cell as usize)).expect("a small grid");
                }
            }
            // An arm added mid-stack.
            2 => {
                let id = u32::try_from(struts.len()).expect("a few members");
                let seat = cell_at(rng.range(-3.0, 3.0), rng.range(-3.0, 3.0));
                let k_top = 2 + rng.below(LAYERS - 4);
                struts.push(arm(rng, id, seat, k_top, 0.0));
            }
            // A member removed, the ids after it remapped.
            3 => {
                struts.remove(at);
                for (index, member) in struts.iter_mut().enumerate() {
                    member.id = u32::try_from(index).expect("a few members");
                }
            }
            // A minimum raised: the raw radius changes, the routing does not.
            4 => struts[at].min_radius_mm = 0.3,
            // A tip lowered one layer: its alias one layer up moves.
            5 => {
                let member = &mut struts[at];
                if member.k_top > member.k_bottom.saturating_add(2) && member.k_bottom != usize::MAX {
                    member.k_top -= 1;
                }
            }
            // A thickened member.
            6 => struts[at].thickened_mm = vec![0.2; LAYERS],
            _ => {}
        }
    }

    /// What a set of chains exercised, summed over them.
    #[derive(Default, Debug)]
    struct Resumed {
        chains: usize,
        passes: usize,
        resumed: usize,
        stopped: usize,
        skipped: usize,
        walked: u64,
        layers: u64,
        placed_after_resume: u64,
        log_unchanged: usize,
    }

    /// The hosts of `scene`, a random subset.
    fn some_hosts(scene: &Scene, struts: &[SweepStrut], rng: &mut Rng) -> Vec<(usize, usize)> {
        let mut hosts: Vec<(usize, usize)> =
            scene.hosts.iter().copied().filter(|_| rng.unit() < 0.5).collect();
        // And a few alive (member, layer) pairs the scene did not name.
        for _ in 0..rng.below(4) {
            if struts.is_empty() {
                break;
            }
            let member = rng.below(struts.len());
            let strut = &struts[member];
            if strut.k_bottom != usize::MAX && strut.k_top > strut.k_bottom {
                hosts.push((member, strut.k_bottom + rng.below(strut.k_top - strut.k_bottom)));
            }
        }
        hosts
    }

    /// Walk one scene as a chain of three passes, an edit before the second,
    /// under the shadow check. Panics where a resumed pass differs.
    fn chain_scene(seed: u64, stride: usize, into: &mut Resumed) {
        let scene = random_scene(seed);
        let mut rng = Rng(0xD3D3_0000 ^ seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let g = grid();
        let cache = LayerCache::build(&g, &scene.slices);
        let mut struts = scene.struts.clone();
        let mut base: Base = None;
        TEST_STRIDE.with(|cell| cell.set(stride));
        TEST_VERIFY.with(|cell| cell.set(true));
        TEST_REPORTS.with(|reports| reports.borrow_mut().clear());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chain_pass(&cache, &scene, &mut struts, &HashMap::new(), &mut base);
            // Pass 2: edited members (often none), a table (often empty).
            let kind = if rng.unit() < 0.5 { 0 } else { 1 + rng.below(6) };
            edit(&mut struts, kind, &mut rng);
            let table = if rng.unit() < 0.25 {
                HashMap::new()
            } else {
                table_of(&some_hosts(&scene, &struts, &mut rng))
            };
            let second = chain_pass(&cache, &scene, &mut struts, &table, &mut base);
            // Pass 3: a table over the members as pass 2 left them.
            let table = table_of(&some_hosts(&scene, &struts, &mut rng));
            let third = chain_pass(&cache, &scene, &mut struts, &table, &mut base);
            (second.arms_inserted_by_load, third.arms_inserted_by_load)
        }));
        TEST_STRIDE.with(|cell| cell.set(super::CHECKPOINT_STRIDE));
        TEST_VERIFY.with(|cell| cell.set(false));
        let reports: Vec<PassReport> = TEST_REPORTS.with(|reports| reports.borrow().clone());
        match result {
            Err(payload) => std::panic::resume_unwind(payload),
            Ok(_) => {
                into.chains += 1;
                for report in reports {
                    into.passes += 1;
                    into.resumed += usize::from(report.resumed_at.is_some());
                    into.stopped += usize::from(report.stopped_at.is_some());
                    into.skipped += usize::from(report.skipped_all);
                    into.walked += report.walked;
                    into.layers += report.layers;
                    into.log_unchanged += usize::from(report.log_unchanged);
                    into.placed_after_resume +=
                        u64::from(report.resumed_at.is_some() && !report.placed_nothing);
                }
            }
        }
    }

    #[test]
    fn a_resumed_pass_equals_the_full_pass_on_200_seeded_chains() {
        let mut total = Resumed::default();
        for seed in 0..200_u64 {
            chain_scene(seed, 3, &mut total);
        }
        println!("200 chains reached {total:?}");
        // The comparison has teeth: the chains resumed, stopped early, skipped
        // whole passes, and inserted arms from a resumed walk.
        assert!(total.resumed > 150, "{total:?}");
        assert!(total.stopped > 10, "{total:?}");
        assert!(total.skipped > 5, "{total:?}");
        assert!(total.placed_after_resume > 20, "{total:?}");
        assert!(total.log_unchanged > 20, "{total:?}");
        assert!(total.walked * 10 < total.layers * 9, "{total:?}");
    }

    #[test]
    fn a_resumed_pass_equals_the_full_pass_at_every_stride() {
        for stride in [1, 2, 5, 16] {
            let mut total = Resumed::default();
            for seed in 200..240_u64 {
                chain_scene(seed, stride, &mut total);
            }
            println!("stride {stride}: {total:?}");
            assert!(total.resumed > 0, "stride {stride}: {total:?}");
        }
    }

    /// A child arm lands on the CONTACT slot of its parent: the load waits in
    /// `pending` for the layer below, across a checkpoint boundary.
    fn pending_scene() -> Scene {
        let slab = (rectangle(0.0, 0.0, 1.4, 0.8, 0.0), 10, 15);
        let mut rng = Rng(21);
        let parent = arm(&mut rng, 0, cell_at(-0.8, 0.0), 10, 0.0);
        let mut child = arm(&mut rng, 1, cell_at(0.6, 0.1), 10, 0.0);
        child.k_bottom = 9;
        child.landing = Landing::Strut { id: 0, layer: 9 };
        Scene {
            slices: slices_of(&[slab]),
            struts: vec![parent, child],
            unrooted: UnrootedColumns::with_layers(LAYERS),
            sizing: sizing(0.5),
            drive: 0,
            insertion_on: true,
            hosts: Vec::new(),
        }
    }

    /// Run the named scene as a chain with these hosts in pass 2, at a stride
    /// of 1 (a boundary before every layer).
    fn named_chain(scene: &Scene, hosts: &[(usize, usize)]) -> Resumed {
        let g = grid();
        let cache = LayerCache::build(&g, &scene.slices);
        let mut struts = scene.struts.clone();
        let mut base: Base = None;
        TEST_STRIDE.with(|cell| cell.set(1));
        TEST_VERIFY.with(|cell| cell.set(true));
        TEST_REPORTS.with(|reports| reports.borrow_mut().clear());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            chain_pass(&cache, scene, &mut struts, &HashMap::new(), &mut base);
            chain_pass(&cache, scene, &mut struts, &table_of(hosts), &mut base);
            chain_pass(&cache, scene, &mut struts, &table_of(hosts), &mut base);
        }));
        TEST_STRIDE.with(|cell| cell.set(super::CHECKPOINT_STRIDE));
        TEST_VERIFY.with(|cell| cell.set(false));
        let reports: Vec<PassReport> = TEST_REPORTS.with(|reports| reports.borrow().clone());
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
        let mut total = Resumed::default();
        for report in reports {
            total.passes += 1;
            total.resumed += usize::from(report.resumed_at.is_some());
            total.stopped += usize::from(report.stopped_at.is_some());
            total.skipped += usize::from(report.skipped_all);
        }
        total
    }

    /// Two calls on one plan, members edited between them, the second resuming
    /// from the plan's stash under the shadow check; and the same second call
    /// on a plan with no stash. They must agree by bits on everything.
    fn two_calls(seed: u64, stride: usize, into: &mut Resumed) {
        let scene = random_scene(seed);
        if !scene.insertion_on {
            return;
        }
        let mut rng = Rng(0xCA11_0000 ^ seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let g = grid();
        let cache = LayerCache::build(&g, &scene.slices);
        let fresh_cache = LayerCache::build(&g, &scene.slices);
        let mut struts = scene.struts.clone();
        let run = |cache: &LayerCache<'_>, struts: &mut Vec<SweepStrut>| {
            super::run_with(
                cache,
                struts,
                scene.sizing,
                InsertionPolicy::new(1000),
                &DrainKeepOut::none(),
                &scene.unrooted,
            )
        };
        TEST_STRIDE.with(|cell| cell.set(stride));
        TEST_VERIFY.with(|cell| cell.set(true));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(&cache, &mut struts);
            let kind = if rng.unit() < 0.3 { 0 } else { 1 + rng.below(6) };
            edit(&mut struts, kind, &mut rng);
            let mut for_fresh = struts.clone();
            TEST_REPORTS.with(|reports| reports.borrow_mut().clear());
            let resumed = run(&cache, &mut struts);
            let reports: Vec<PassReport> = TEST_REPORTS.with(|reports| reports.borrow().clone());
            // The same call with no stash to resume from.
            TEST_VERIFY.with(|cell| cell.set(false));
            let full = run(&fresh_cache, &mut for_fresh);
            assert_eq!(format!("{resumed:?}"), format!("{full:?}"), "seed {seed}: outcome");
            assert!(resumed.route_log == full.route_log, "seed {seed}: route log");
            assert_eq!(struts.len(), for_fresh.len(), "seed {seed}: members");
            for (index, (a, b)) in struts.iter().zip(&for_fresh).enumerate() {
                assert_eq!(format!("{a:?}"), format!("{b:?}"), "seed {seed}: member {index}");
            }
            reports
        }));
        TEST_STRIDE.with(|cell| cell.set(super::CHECKPOINT_STRIDE));
        TEST_VERIFY.with(|cell| cell.set(false));
        match result {
            Err(payload) => std::panic::resume_unwind(payload),
            Ok(reports) => {
                into.chains += 1;
                for report in reports {
                    into.passes += 1;
                    into.resumed += usize::from(report.resumed_at.is_some());
                    into.stopped += usize::from(report.stopped_at.is_some());
                    into.skipped += usize::from(report.skipped_all);
                    into.walked += report.walked;
                    into.layers += report.layers;
                }
            }
        }
    }

    #[test]
    fn a_second_call_resumes_from_the_first_calls_last_pass() {
        let mut total = Resumed::default();
        for seed in 0..150_u64 {
            two_calls(seed, 3, &mut total);
        }
        println!("150 two-call chains reached {total:?}");
        assert!(total.resumed > 60, "{total:?}");
        assert!(total.skipped > 3, "{total:?}");
        assert!(total.walked * 10 < total.layers * 9, "{total:?}");
    }

    #[test]
    fn sabotage_stopping_above_the_second_calls_lowest_edit_is_red() {
        sabotage::set(sabotage::RESUME_STOP_LOWEST);
        let mut red = 0_usize;
        for seed in 0..150_u64 {
            if std::panic::catch_unwind(|| two_calls(seed, 3, &mut Resumed::default())).is_err() {
                red += 1;
            }
        }
        sabotage::set(0);
        assert!(red > 0, "the two-call comparison cannot see a stop above an edit");
    }

    #[test]
    fn sabotage_reusing_the_sizing_whatever_the_log_says_is_red() {
        sabotage::set(sabotage::REUSE_LOG);
        let mut red = 0_usize;
        for seed in 0..150_u64 {
            if std::panic::catch_unwind(|| two_calls(seed, 3, &mut Resumed::default())).is_err() {
                red += 1;
            }
        }
        sabotage::set(0);
        println!("sabotage per-event reuse: {red} two-call chains differ");
        assert!(red > 0, "the shadow check cannot see a sizing reused over a changed log");
    }

    /// Everything a down-sweep call reads besides the members and the layers.
    #[derive(Clone)]
    struct Inputs {
        sizing: SizingPolicy,
        max_struts: usize,
        max_rounds: usize,
        caps: Option<ContactCaps>,
        keep_out: DrainKeepOut,
        unrooted: UnrootedColumns,
    }

    impl Inputs {
        fn of(scene: &Scene) -> Self {
            Self {
                sizing: scene.sizing,
                max_struts: 1000,
                max_rounds: 4,
                caps: None,
                keep_out: DrainKeepOut::none(),
                unrooted: scene.unrooted.clone(),
            }
        }

        fn run(&self, cache: &LayerCache<'_>, struts: &mut Vec<SweepStrut>) -> super::DownSweepOutcome {
            let mut insertion = InsertionPolicy::new(self.max_struts);
            insertion.max_rounds = self.max_rounds;
            if let Some(caps) = &self.caps {
                insertion = insertion.with_contact_caps(caps);
            }
            super::run_with(cache, struts, self.sizing, insertion, &self.keep_out, &self.unrooted)
        }
    }

    /// A cap of `contact_mm2` on every tip the members have.
    fn caps_of(struts: &[SweepStrut], contact_mm2: f64) -> ContactCaps {
        let mut caps = ContactCaps::default();
        for strut in struts {
            caps.insert(strut.cell, strut.k_top, contact_mm2);
        }
        caps
    }

    /// Two calls on ONE plan cache with different inputs, the members as the
    /// first call left them; the second must equal the same call on a fresh
    /// cache, by bits. Returns the arms the second call inserted.
    fn two_inputs(seed: u64, label: &str, second: &dyn Fn(&Scene, &[SweepStrut]) -> Inputs) -> u64 {
        let scene = random_scene(seed);
        if !scene.insertion_on {
            return 0;
        }
        let g = grid();
        let cache = LayerCache::build(&g, &scene.slices);
        let fresh_cache = LayerCache::build(&g, &scene.slices);
        let mut struts = scene.struts.clone();
        Inputs::of(&scene).run(&cache, &mut struts);
        let inputs = second(&scene, &struts);
        let mut for_fresh = struts.clone();
        let reused = inputs.run(&cache, &mut struts);
        let full = inputs.run(&fresh_cache, &mut for_fresh);
        assert_eq!(format!("{reused:?}"), format!("{full:?}"), "{label} seed {seed}: outcome");
        assert!(reused.route_log == full.route_log, "{label} seed {seed}: route log");
        assert_eq!(struts.len(), for_fresh.len(), "{label} seed {seed}: members");
        for (index, (a, b)) in struts.iter().zip(&for_fresh).enumerate() {
            assert_eq!(format!("{a:?}"), format!("{b:?}"), "{label} seed {seed}: member {index}");
        }
        reused.arms_inserted_by_load
    }

    fn two_inputs_over_seeds(label: &str, second: &dyn Fn(&Scene, &[SweepStrut]) -> Inputs) -> u64 {
        (0..60_u64).map(|seed| two_inputs(seed, label, second)).sum()
    }

    /// The review's case: no caps on the first call, a tip cap below its routed
    /// load on the second. The second call must find the relief insertions a
    /// fresh call finds.
    #[test]
    fn a_second_call_with_contact_caps_is_not_served_the_first_calls_sizing() {
        let inserted = two_inputs_over_seeds("caps added", &|scene, struts| Inputs {
            caps: Some(caps_of(struts, 1.0e-6)),
            ..Inputs::of(scene)
        });
        assert!(inserted > 0, "no seed placed a relief arm under the caps: the test cannot see the bug");
    }

    #[test]
    fn a_second_call_with_other_contact_cap_values_equals_a_fresh_cache() {
        two_inputs_over_seeds("caps changed", &|scene, struts| Inputs {
            caps: Some(caps_of(struts, 0.02)),
            ..Inputs::of(scene)
        });
        // First tight, then loose: the second call must not keep the tight sizing.
        for seed in 0..60_u64 {
            let scene = random_scene(seed);
            if !scene.insertion_on {
                continue;
            }
            let g = grid();
            let cache = LayerCache::build(&g, &scene.slices);
            let fresh_cache = LayerCache::build(&g, &scene.slices);
            let mut struts = scene.struts.clone();
            let tight = Inputs { caps: Some(caps_of(&struts, 1.0e-6)), ..Inputs::of(&scene) };
            tight.run(&cache, &mut struts);
            let loose = Inputs { caps: Some(caps_of(&struts, 1.0e6)), ..Inputs::of(&scene) };
            let mut for_fresh = struts.clone();
            let reused = loose.run(&cache, &mut struts);
            let full = loose.run(&fresh_cache, &mut for_fresh);
            assert_eq!(format!("{reused:?}"), format!("{full:?}"), "tight then loose seed {seed}");
            assert!(reused.route_log == full.route_log, "tight then loose seed {seed}: route log");
        }
    }

    #[test]
    fn a_second_call_with_another_sizing_policy_equals_a_fresh_cache() {
        two_inputs_over_seeds("sizing", &|scene, _| {
            let mut inputs = Inputs::of(scene);
            inputs.sizing.x_allowed *= 0.5;
            inputs
        });
    }

    #[test]
    fn a_second_call_with_other_insertion_limits_equals_a_fresh_cache() {
        two_inputs_over_seeds("max_rounds", &|scene, _| Inputs { max_rounds: 1, ..Inputs::of(scene) });
        two_inputs_over_seeds("max_struts", &|scene, struts| Inputs {
            max_struts: struts.len(),
            ..Inputs::of(scene)
        });
    }

    #[test]
    fn a_second_call_with_other_unrooted_columns_equals_a_fresh_cache() {
        two_inputs_over_seeds("unrooted", &|scene, struts| {
            let mut inputs = Inputs::of(scene);
            let mut unrooted = UnrootedColumns::with_layers(LAYERS);
            for strut in struts {
                for layer in strut.k_bottom.min(LAYERS)..strut.k_top.min(LAYERS) {
                    unrooted.by_layer[layer].push(strut.cell_at(layer) as u32);
                    unrooted.total += 1;
                }
            }
            inputs.unrooted = unrooted;
            inputs
        });
    }

    #[test]
    fn a_second_call_with_a_drain_keep_out_equals_a_fresh_cache() {
        two_inputs_over_seeds("keep-out", &|scene, struts| {
            let g = grid();
            let mut inputs = Inputs::of(scene);
            // A drain mouth over the first member's seat column, running out below.
            let at = struts.first().map_or(0, |strut| strut.cell);
            let (row, col) = (at / g.cols(), at % g.cols());
            let x = ORIGIN + (col as f64 + 0.5) * PITCH;
            let y = ORIGIN + (row as f64 + 0.5) * PITCH;
            let drain = crate::drain::DrainSpec {
                outlet_mm: [x, y, H * LAYERS as f64],
                outward_axis: [0.0, 0.0, -1.0],
                radius_mm: 0.3,
            };
            inputs.keep_out = DrainKeepOut::build(&g, &[drain], 0.1, 0.5, LAYERS, 0.0, H);
            inputs
        });
    }

    /// Every field of the run key moves it (a field the key misses would
    /// resume a pass under inputs it was not walked under).
    #[test]
    fn the_run_key_names_every_input() {
        let scene = random_scene(3);
        let base = Inputs::of(&scene);
        let key = |inputs: &Inputs| {
            let mut insertion = InsertionPolicy::new(inputs.max_struts);
            insertion.max_rounds = inputs.max_rounds;
            if let Some(caps) = &inputs.caps {
                insertion = insertion.with_contact_caps(caps);
            }
            super::run_key(inputs.sizing, insertion, &inputs.keep_out, &inputs.unrooted, LAYERS)
        };
        let same = key(&base);
        assert_eq!(same, key(&base.clone()));
        let mut changed: Vec<(&str, Inputs)> = Vec::new();
        changed.push(("max_struts", Inputs { max_struts: 7, ..base.clone() }));
        changed.push(("max_rounds", Inputs { max_rounds: 1, ..base.clone() }));
        changed.push(("caps", Inputs { caps: Some(caps_of(&scene.struts, 0.5)), ..base.clone() }));
        let mut other_value = caps_of(&scene.struts, 0.5);
        other_value.insert(scene.struts[0].cell, scene.struts[0].k_top, 0.25);
        changed.push(("cap value", Inputs { caps: Some(other_value), ..base.clone() }));
        let g = grid();
        let drain = crate::drain::DrainSpec {
            outlet_mm: [0.0, 0.0, 1.0],
            outward_axis: [0.0, 0.0, -1.0],
            radius_mm: 0.3,
        };
        changed.push((
            "keep_out",
            Inputs { keep_out: DrainKeepOut::build(&g, &[drain], 0.1, 0.5, LAYERS, 0.0, H), ..base.clone() },
        ));
        let mut unrooted = UnrootedColumns::with_layers(LAYERS);
        unrooted.by_layer[2].push(5);
        unrooted.total = 1;
        changed.push(("unrooted", Inputs { unrooted, ..base.clone() }));
        let mut sized = base.clone();
        sized.sizing.alpha += 0.25;
        changed.push(("sizing", sized));
        for (name, inputs) in &changed {
            assert_ne!(key(inputs), same, "the run key ignores {name}");
        }
        // The key of a cap set does not depend on the order the tips went in.
        let tips: Vec<(usize, usize)> = scene.struts.iter().map(|s| (s.cell, s.k_top)).collect();
        let mut forward = ContactCaps::default();
        let mut backward = ContactCaps::default();
        for &(cell, k_top) in &tips {
            forward.insert(cell, k_top, 0.1);
        }
        for &(cell, k_top) in tips.iter().rev() {
            backward.insert(cell, k_top, 0.1);
        }
        assert_eq!(
            key(&Inputs { caps: Some(forward), ..base.clone() }),
            key(&Inputs { caps: Some(backward), ..base })
        );
    }

    /// A pool that refuses a trail's bytes must be left as it was: the trail
    /// that was never charged must not give anything back.
    #[test]
    fn a_failed_adoption_leaves_the_pool_unchanged() {
        let pool = crate::layer_cache::BytePool::new(100);
        assert!(pool.try_acquire(60));
        let before = pool.used();
        assert!(PassTrail::adopt(&pool, super::TrailData::default(), 50).is_none());
        assert_eq!(pool.used(), before, "a refused adoption changed the pool");
        // And one that is taken is charged once and given back once.
        let adopted = PassTrail::adopt(&pool, super::TrailData::default(), 40).expect("room for 40");
        assert_eq!(pool.used(), before + 40);
        drop(adopted);
        assert_eq!(pool.used(), before);
    }

    /// N identical calls on one plan leave the pool charged the same after
    /// each (the stash's bytes are charged once, not once more per reuse).
    #[test]
    fn identical_calls_charge_the_pool_once() {
        let mut compared = 0_usize;
        for seed in 0..40_u64 {
            let scene = random_scene(seed);
            if !scene.insertion_on {
                continue;
            }
            let pool = crate::layer_cache::BytePool::new(u64::MAX / 4);
            let g = grid();
            let cache = LayerCache::build_in(&g, &scene.slices, &pool);
            let mut struts = scene.struts.clone();
            let inputs = Inputs::of(&scene);
            inputs.run(&cache, &mut struts);
            let mut charged = Vec::new();
            for _ in 0..6 {
                inputs.run(&cache, &mut struts);
                charged.push(pool.used());
            }
            assert!(
                charged.windows(2).all(|pair| pair[0] == pair[1]),
                "seed {seed}: the pool charge moves across identical calls: {charged:?}"
            );
            compared += 1;
        }
        assert!(compared > 10, "{compared} seeds compared");
    }

    #[test]
    fn a_pending_child_load_survives_the_checkpoint() {
        let scene = pending_scene();
        // Pass 1 puts the child's load in the parent's pending slot at layer 9,
        // where it waits across the boundary before layer 8; a trigger at 8
        // resumes exactly there.
        let total = named_chain(&scene, &[(0, 8)]);
        assert!(total.resumed >= 1, "{total:?}");
    }

    #[test]
    fn handed_and_leaned_fixtures_resume_exactly() {
        for (name, scene) in named() {
            if scene.struts.is_empty() {
                continue;
            }
            for layer in [1, 3, 5, 6, 8] {
                let member = scene.struts.iter().position(|s| s.k_bottom <= layer && layer < s.k_top);
                if let Some(member) = member {
                    let total = named_chain(&scene, &[(member, layer)]);
                    assert!(total.passes == 3, "{name}: {total:?}");
                }
            }
        }
    }

    /// SABOTAGE: drop one invalidation or restore rule and some chain must come
    /// out different from the full pass (the shadow check panics).
    fn chain_red_with(class: u16, label: &str) {
        let red = chain_reds(class);
        println!("sabotage {label}: {red} chains differ");
        assert!(red > 0, "dropping {label} did not change any chain: the shadow check cannot see it");
    }

    /// The chains (of the seeded and the named) the shadow check calls
    /// different with `class` dropped.
    fn chain_reds(class: u16) -> usize {
        sabotage::set(class);
        let mut red = 0_usize;
        let mut differs = |run: &dyn Fn()| {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).is_err() {
                red += 1;
            }
        };
        for seed in 0..120_u64 {
            differs(&|| chain_scene(seed, 2, &mut Resumed::default()));
        }
        differs(&|| {
            named_chain(&pending_scene(), &[(0, 8)]);
        });
        for (_, scene) in named() {
            for layer in [3, 5, 6] {
                if let Some(member) =
                    scene.struts.iter().position(|s| s.k_bottom <= layer && layer < s.k_top)
                {
                    differs(&|| {
                        named_chain(&scene, &[(member, layer)]);
                    });
                }
            }
        }
        sabotage::set(0);
        red
    }

    #[test]
    fn sabotage_ignoring_the_trigger_layers_is_red() {
        chain_red_with(sabotage::RESUME_TRIGGERS, "the trigger table's layers");
    }

    #[test]
    fn sabotage_ignoring_the_changed_members_is_red() {
        chain_red_with(sabotage::RESUME_MEMBERS, "the changed members' layers");
    }

    #[test]
    fn sabotage_dropping_the_pending_loads_is_red() {
        chain_red_with(sabotage::RESUME_PENDING, "the pending loads");
    }

    #[test]
    fn sabotage_dropping_the_handed_columns_is_red() {
        chain_red_with(sabotage::RESUME_HANDED, "the handed columns");
    }

    #[test]
    fn sabotage_stopping_without_comparing_the_state_is_red() {
        chain_red_with(sabotage::RESUME_STOP_UNCHECKED, "the stop's state comparison");
    }

    #[test]
    fn sabotage_dropping_the_restored_ledgers_is_red() {
        chain_red_with(sabotage::RESUME_LEDGER, "the restored ledgers");
    }

    #[test]
    fn sabotage_stopping_above_the_lowest_changed_layer_is_red() {
        chain_red_with(sabotage::RESUME_STOP_LOWEST, "the lowest changed layer");
    }

    #[test]
    fn sabotage_dropping_the_restored_accounting_is_red() {
        chain_red_with(sabotage::RESUME_OUTCOME, "the restored accounting");
    }

    #[test]
    fn the_ledger_watch_is_defence_in_depth_and_no_result_leans_on_it() {
        // HONEST LIMIT: the ledger watch is the stop's third guard, behind the
        // state comparison and the trigger and member bounds, and no fixture
        // here (seeded chains, two-call chains, the named scenes) makes it the
        // only thing standing between a stop and a wrong tail: a stop needs the
        // load rasters, the carriers, the handed and pending loads and every
        // accounting sum equal to the base's, and an arm whose carried load
        // differs while all of those agree has not been built. So this test
        // cannot be red; it records that dropping the watch changes no chain,
        // which is the evidence that it is defence in depth and not a rule
        // the results lean on.
        let unwatched = chain_reds(sabotage::RESUME_STOP_LEDGER);
        println!("stop without the ledger watch: {unwatched} chains differ");
        assert_eq!(unwatched, 0, "the ledger watch now decides a result: give it a sabotage test");
    }
}
