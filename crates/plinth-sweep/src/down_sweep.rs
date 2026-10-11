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
        if size_head_from_ledger(strut, sizing, law, slots).is_some() {
            resized += 1;
        }
    }
    resized
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
    if !struts.iter().any(candidate) {
        return vec![None; struts.len()];
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
            let head = strut.head.as_ref()?;
            let (seat, base) = crate::adapter::drawn_ends(grid, plate, h, strut);
            let moved = |point: [f64; 3], by: Option<[f64; 3]>| -> [f64; 3] {
                by.map_or(point, |by| std::array::from_fn(|axis| point[axis] + by[axis]))
            };
            let base = moved(base, followed.base_mm);
            let arm = crate::adapter::sweep_arm(
                strut,
                moved(seat, followed.seat_mm),
                base,
                None,
                Vec::new(),
            );
            let plate_shaft = match &flare {
                Some(flare) if strut.landing == crate::Landing::Plate => {
                    flare.plate_shaft_of(&arm, plate)
                }
                _ => None,
            };
            Some(plate_shaft.unwrap_or((head.joint_mm, base)))
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
            grid,
            slices,
            struts,
            sizing,
            insertion,
            keep_out,
            unrooted,
            None,
            None,
            &mut Vec::new(),
        );
        for strut in struts.iter_mut() {
            apply_head_minimum(strut, sizing);
        }
        return outcome;
    }
    let mut triggers: HashMap<u64, f64> = HashMap::new();
    let mut refused: HashSet<u64> = HashSet::new();
    let mut inserted = 0_u64;
    let mut geometry_exhausted = 0_u64;
    let mut evaluations = 0_u64;
    let mut passes = 0_usize;
    crate::reuse_probe::begin(grid.cells(), slices.len(), struts, site);
    loop {
        let mut log = crate::per_event::RouteLog::new(slices.len());
        let mut refused_here = Vec::new();
        let mut outcome = sweep_pass(
            grid,
            slices,
            struts,
            sizing,
            insertion,
            keep_out,
            unrooted,
            Some(&triggers),
            Some(&mut log),
            &mut refused_here,
        );
        passes += 1;
        if passes == 1 {
            crate::reuse_probe::end_first_pass();
        }
        inserted += outcome.arms_inserted_by_load;
        geometry_exhausted += outcome.insertion_geometry_exhausted;
        refused.extend(refused_here);
        let worst_max_field = struts
            .iter()
            .flat_map(|strut| strut.carried_mm2.iter().copied())
            .fold(0.0_f64, f64::max);
        let sized = {
            let _per_event_span = crate::perf::span(crate::perf::Span::PerEventSize);
            crate::per_event::size(grid, slices, struts, &log, sizing, l_max)
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
        let open: HashMap<u64, f64> = sized
            .triggers
            .into_iter()
            .filter(|(key, _)| !refused.contains(key))
            .collect();
        let done = !insertion.enabled || open.is_empty();
        let exhausted = !done && passes > insertion.max_rounds;
        if done || exhausted {
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
            outcome.route_log = log;
            return outcome;
        }
        triggers = open;
    }
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
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    struts: &mut Vec<SweepStrut>,
    sizing: SizingPolicy,
    insertion: InsertionPolicy,
    keep_out: &DrainKeepOut,
    unrooted: &UnrootedColumns,
    triggers: Option<&HashMap<u64, f64>>,
    mut log: Option<&mut crate::per_event::RouteLog>,
    refused: &mut Vec<u64>,
) -> DownSweepOutcome {
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
    let mut distance_prev = vec![f32::INFINITY; cells];
    let mut route_anchor = vec![crate::reach::NO_ANCHOR; cells];
    let mut route_clear: Vec<f32> = Vec::with_capacity(cells);
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

    for layer in (0..slices.len()).rev() {
        outcome.rasteriser_entries += 1;
        {
            let _prefix_span = crate::perf::span(crate::perf::Span::PassPrefix);
            rasterize_into(grid, &slices[layer], &mut scratch, &mut masks);
        }
        let model_cells = masks.carrier_cells();
        let layer_area = model_cells as f64 * area;
        if model_cells > 0 {
            outcome.peak_layer_area_mm2 = outcome.peak_layer_area_mm2.max(layer_area);
            outcome.min_layer_area_mm2 = outcome.min_layer_area_mm2.min(layer_area);
        }

        carriers.copy_from_slice(&masks.carry);
        continues_above.fill(false);
        landing_target.fill(u32::MAX);
        tip_root.fill(u32::MAX);
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
            }
            // AND THE SAME CELL IS WHERE THIS ARM HOLDS SOMETHING UP. The
            // material at `above` on layer k+1 rests on this arm's own axis,
            // with no lateral travel to pay for; the arm is where the up-sweep
            // rooted that column, and this is the down-sweep saying so. First
            // arm wins, in `alive_by_layer` order, which is arm id order.
            if sizing.tip_column_roots && above != cell && tip_root[above] == u32::MAX {
                tip_root[above] = u32::try_from(cell).unwrap_or(u32::MAX);
                outcome.tip_column_roots_offered += 1;
            }
            // What a growing arm lands ON if its column meets this one. Only
            // arms whose extent is already fixed are landing targets; a growing
            // arm can never be one, because an insertion candidate must not be
            // a carrier and a growing arm's cell always is.
            landing_target[cell] = id;
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
            carriers[growing_cell(&struts[index])] = true;
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
        // The routing field's clearance is built at most once per layer:
        // `cover_above` does not change between the rounds below.
        let mut route_clear_built = false;
        // Per-event mode inserts at most once per layer per pass: the trigger
        // table was measured before this pass, so after an insertion only the
        // re-route below is needed, and the next pass re-measures.
        let mut inserted_here = false;
        loop {
            {
                let _fill_span = crate::perf::span(crate::perf::Span::PassDense);
                load_out.iter_mut().for_each(|value| *value = 0.0);
                peel_fed.fill(false);
            }
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
            let offered: &[bool] = if rule_active && !unrooted_here.is_empty() {
                roots.copy_from_slice(&carriers);
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
            let route_span = crate::perf::span(crate::perf::Span::PassRoute);
            let route = route_within_cover(
                grid,
                &cover_above,
                offered,
                &tip_root,
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
                    clear: &mut route_clear,
                    clear_built: &mut route_clear_built,
                },
            );
            drop(route_span);
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
                    }
                }
            }
            let dense_span = crate::perf::span(crate::perf::Span::PassDense);
            for cell in 0..cells {
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
            for cell in 0..cells {
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
            drop(dense_span);

            if !insertion.enabled {
                break;
            }
            let _insertion_span = crate::perf::span(crate::perf::Span::PassInsertion);
            insert.hosts.clear();
            insert.host_load.clear();
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
                            if load > l_max {
                                insert.hosts.push(struts[id as usize].cell_at(layer));
                                insert.host_load.push(load);
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
                &cover_above,
                &mut carriers,
                &load_in,
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
                refused.extend(insert.host_keys.iter().copied());
                break;
            }
            outcome.arms_inserted_by_load += added;
            inserted_here = triggers.is_some();
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

        // --- the model's own section is checked, never resized --------------
        let end_span = crate::perf::span(crate::perf::Span::PassDense);
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
        drop(end_span);
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
    }

    // Anything still in the raster at the bottom of the stack reached the
    // plate through model material rather than through an arm.
    outcome.plate_reaction_mm2 += load_in.iter().sum::<f64>();
    if !outcome.min_layer_area_mm2.is_finite() {
        outcome.min_layer_area_mm2 = 0.0;
    }
    outcome
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
    /// Each host's per-event trigger key, parallel to `hosts` (empty under
    /// sabotage), so a trigger the geometry refused is not re-tried.
    host_keys: Vec<u64>,
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
            host_load: Vec::new(),
            host_keys: Vec::new(),
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
        if scratch.host_load[slot] > l_max {
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
        let routed = scratch.host_load[slot];
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
                thickened_mm: Vec::new(),
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
                covered_demand: Vec::new(), twin_host: false,
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
        clear_built,
    } = fields;
    let cols = grid.cols();
    let rows = grid.rows();
    let pitch = grid.pitch_mm() as f32;
    let mut sight = crate::reach::Sightlines::default();
    let mut any_root = false;
    for cell in 0..grid.cells() {
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
    }
    if !any_root {
        return RouteOutcome {
            any_root: false,
            converged: true,
            sightlines: sight,
        };
    }
    // Nothing to route where every covered cell is already a root, and a
    // root can never be improved on: the passes would change nothing, so they
    // are not run, and the clearance they would read is not built.
    if !(0..grid.cells()).any(|cell| cover_above[cell] && distance[cell] > 0.0) {
        return RouteOutcome {
            any_root: true,
            converged: true,
            sightlines: sight,
        };
    }
    if !*clear_built {
        let _clearance_span = crate::perf::span(crate::perf::Span::PassClearance);
        crate::reach::clearance_mm(grid, cover_above, clear);
        *clear_built = true;
    }
    let clear: &[f32] = clear;
    let mut relax = |cell: usize,
                     from: usize,
                     distance: &mut [f32],
                     nearest: &mut [u32],
                     via_tip: &mut [bool],
                     anchor: &mut [u32]| {
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
        }
    };
    const MAX_SWEEP_ROUNDS: usize = 8;
    let mut converged = false;
    previous.copy_from_slice(distance);
    for _ in 0..MAX_SWEEP_ROUNDS {
        for row in 0..rows {
            for col in 0..cols {
                let cell = row * cols + col;
                // A root (distance 0) cannot be improved on.
                if !cover_above[cell] || distance[cell] == 0.0 {
                    continue;
                }
                if col > 0 && cover_above[cell - 1] {
                    relax(cell, cell - 1, distance, nearest, via_tip, anchor);
                }
                if row > 0 {
                    if cover_above[cell - cols] {
                        relax(cell, cell - cols, distance, nearest, via_tip, anchor);
                    }
                    if col > 0 && cover_above[cell - cols - 1] {
                        relax(cell, cell - cols - 1, distance, nearest, via_tip, anchor);
                    }
                    if col + 1 < cols && cover_above[cell - cols + 1] {
                        relax(cell, cell - cols + 1, distance, nearest, via_tip, anchor);
                    }
                }
            }
        }
        for row in (0..rows).rev() {
            for col in (0..cols).rev() {
                let cell = row * cols + col;
                // A root (distance 0) cannot be improved on.
                if !cover_above[cell] || distance[cell] == 0.0 {
                    continue;
                }
                if col + 1 < cols && cover_above[cell + 1] {
                    relax(cell, cell + 1, distance, nearest, via_tip, anchor);
                }
                if row + 1 < rows {
                    if cover_above[cell + cols] {
                        relax(cell, cell + cols, distance, nearest, via_tip, anchor);
                    }
                    if col + 1 < cols && cover_above[cell + cols + 1] {
                        relax(cell, cell + cols + 1, distance, nearest, via_tip, anchor);
                    }
                    if col > 0 && cover_above[cell + cols - 1] {
                        relax(cell, cell + cols - 1, distance, nearest, via_tip, anchor);
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
        sightlines: sight,
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
    /// `cover_above`'s clearance field (`reach::clearance_mm`), built at most
    /// once per layer, on the first round that needs it: the cover does not
    /// change between rounds. The caller resets `clear_built` per layer.
    clear: &'a mut Vec<f32>,
    clear_built: &'a mut bool,
}

/// What one layer's routing field cost to build.
struct RouteOutcome {
    any_root: bool,
    converged: bool,
    sightlines: crate::reach::Sightlines,
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
