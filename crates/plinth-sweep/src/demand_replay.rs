//! The multi-layer reach check, re-run over a proposed arm set before the
//! removal pass deletes anything (lane neighbour-cover, 2026-09-30).
//!
//! # Why the removal pass needs it
//!
//! Until this lane the removal pass (`remove_under_loaded`) accepted an arm's
//! demand as covered only by a survivor SEATED ON THE SAME LAYER within one
//! lamina's reach. That test can be wrong in both directions:
//!
//! * it is blind to every layer above the seat. An arm roots its demand at
//!   its seat, and the material built on that demand inherits the potential
//!   upward ([`crate::reach::recompute_potential`]), so removing the arm can
//!   unroot cells several layers higher that no seat-layer test looks at;
//! * it measures a distance in the plane (`ReachMetric`), while the
//!   potential travels only THROUGH cured cover. Across a gap in the cover
//!   the straight line is shorter than any path the lamina can take.
//!
//! Both are answered the same way: re-run the up-sweep's own reach potential,
//! layer by layer, over the arm set that would remain, and compare it with the
//! same potential over the arm set as it stands. An arm set that leaves a
//! demand cell unrooted which the current set roots is refused. Nothing here
//! is a new criterion: it is the placement's own question, asked again of the
//! field the removal would leave.
//!
//! # What an arm on another layer can and cannot do
//!
//! An arm seated at layer `k` is a root at `k` and nowhere else; material
//! above it inherits that root through vertical inheritance. So an arm seated
//! BELOW a demand cell can cover it (through the cured material between), and
//! an arm seated ABOVE it cannot: when the lower layer is cured and peeled the
//! upper arm's column has not yet touched the model. That is the printing
//! order, not a modelling choice, and the replay carries it by construction
//! because it walks the layers in build order.
//!
//! # Batch-first
//!
//! One replay evaluates the JOINT counterfactual — every tentatively doomed
//! arm gone at once — so the interaction "A and B together uncover what
//! neither does alone" is the case evaluated. Where it finds a cell the full
//! set roots and the proposed set does not, it restores the NEAREST doomed arm
//! that could have rooted it (seated at or below the cell's layer) and checks
//! again. A restored arm on the same layer is relaxed in place; one on a lower
//! layer changes every layer from its seat upward, so the replay restarts. The
//! set the last replay passes is therefore the set that is removed, and it was
//! verified whole. Restarts are bounded by the registered removal pass count;
//! running out restores EVERY doomed arm, which is the over-support direction.

#![expect(
    clippy::cast_possible_truncation,
    reason = "the potential is kept in f32 by `reach`; the one f64 -> f32 cast here is the \
          first-loss ratio (about 1.2), compared against that f32 potential and never stored"
)]
#![expect(
    clippy::needless_range_loop,
    reason = "batch-first: the layer loop walks the slice stack, the plane list and the per-layer \
          root lists at one index, and writes two reach states as it goes"
)]

use plinth_mesh::Mesh;
use plinth_support::perception::SliceSolid;

use crate::grid::Grid;
use crate::layer_cache::{BytePool, LayerCache};
use crate::reach::{self, ReachState, SurfaceCreep};
use crate::SweepStrut;

/// Everything the replay reads. The same inputs the up-sweep read, so the
/// replay of an unchanged arm set reproduces the placement's own potential.
pub(crate) struct ReplayContext<'a> {
    pub grid: &'a Grid,
    pub slices: &'a [Vec<SliceSolid>],
    /// The plan's layer cache, when the caller has one. It is used only if
    /// its key matches `grid` and `slices` ([`LayerCache::resolve`]); a
    /// missing or mismatched cache is rebuilt here, never trusted.
    pub layer_cache: Option<&'a LayerCache<'a>>,
    /// The slicing plane of each layer; a layer's material is born in
    /// `[plane - h, plane]`, exactly as the up-sweep loads the creep band.
    pub planes: &'a [f64],
    pub mesh: &'a Mesh,
    pub kappa: f64,
    pub layer_height_mm: f64,
    pub layers: usize,
    /// The up-sweep's metric ([`crate::SweepPolicy::straight_reach`]): the
    /// straight length, or the 8-neighbour chamfer when it is off. The replay
    /// must price a run as the placement did, or it reproduces a different
    /// potential from the one the seats were placed on.
    pub straight_reach: bool,
    pub creep_weight: bool,
    /// The up-sweep's overhang cut ([`crate::SweepPolicy::creep_overhang_cut`]),
    /// for the same reason as `straight_reach`.
    pub creep_overhang_cut: bool,
    pub overhang_centre_deg: f64,
    pub overhang_transition_deg: f64,
    /// Upper bound on restarts; the registered removal pass count.
    pub max_restarts: u32,
    /// `LAMINA_REACH_RATIO_FIRST_LOSS / LAMINA_REACH_RATIO`: the potential at
    /// which the plates 1 + 4 coupon first lost a lamina (2.592 mm against the
    /// placement's 2.130 mm). **Reporting only**: it says how far past the
    /// placement's reach each refused cell lies, and decides nothing.
    pub first_loss_ratio: f64,
    /// The up-sweep's births ([`crate::births`]): a bridged birth with no arm
    /// of its layer standing in it is unrooted here exactly as the placement
    /// left it, in both arm sets.
    pub births: &'a crate::births::Births,
}

/// What one call measured. Path-hit counters and the refusals.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReplayOutcome {
    /// Full replays run (1 + restarts). Zero means nothing was doomed.
    pub replays: u64,
    /// Layers on which the proposed set's potential was computed. The witness
    /// that the check ran over the model, not merely that it was called.
    pub trial_layers: u64,
    /// Cover cells unrooted by the CURRENT arm set on the first replay: the
    /// placement's residual demand, re-measured. Equal to the up-sweep's own
    /// residual when no arm has been added or moved since placement.
    pub baseline_unrooted_cells: u64,
    /// Cells the proposed set would have left unrooted that the current set
    /// roots, summed over every check. Each one restored an arm.
    pub newly_unrooted_cells: u64,
    /// Of `newly_unrooted_cells`, those whose potential was still at or below
    /// the first-loss ratio: past the placement's reach, inside the coupon's
    /// first-loss bracket. **Reporting only.**
    pub newly_unrooted_within_first_loss: u64,
    /// The largest finite potential among `newly_unrooted_cells`: how far past
    /// the placement's reach (`U = 1`) the worst refused cell lies. Zero when
    /// none was found. Cells no root reaches at all are not in this number.
    pub newly_unrooted_max_u: f64,
    /// Doomed arms the replay restored.
    pub refused: u64,
    /// Of `refused`, the arms every cell that restored them lay inside the
    /// first-loss bracket (finite and at most `first_loss_ratio`), on the
    /// layer where they were restored. **Reporting only, and partial**: the
    /// layers above a restored seat are never examined without the arm, so
    /// this is not the count a reach at the bracket's far end would free
    /// (see `report::RemovalCoverageReport::refused_by_replay_within_first_loss`).
    pub refused_within_first_loss: u64,
    /// Whether the restart bound ran out and every doomed arm was restored.
    pub exhausted: bool,
    /// Layers the replays actually walked, summed over every replay. A replay
    /// that returns early (a restart, a restore-all, or the carries agreeing
    /// above the highest doomed seat) walks fewer than `ReplayContext::layers`.
    /// `perf` charges `cells * layers_visited` as `replay.cell_layers`; it used
    /// to charge `cells * layers` at the top of every replay, which over-counted
    /// every early exit (X0, 2026-10-10).
    pub layers_visited: u64,
}

/// The reference implementation: every replay of a call starts from layer 0
/// and recomputes both potentials, as the replay did before lane RP
/// (2026-10-15). It is the oracle [`refuse_unrooted_demand`] is held to (the
/// tests, and `PLINTH_SWEEP_VERIFY_REPLAY=1` in a run) and charges no counter.
fn reference_refuse_unrooted_demand(
    context: &ReplayContext<'_>,
    struts: &[SweepStrut],
    doomed: &mut [bool],
) -> (ReplayOutcome, Vec<f32>) {
    let mut outcome = ReplayOutcome::default();
    if context.layers == 0 || !doomed.iter().any(|flag| *flag) {
        return (outcome, Vec::new());
    }
    // Every arm is a root at its seat layer and seat cell, which is exactly
    // where the up-sweep relaxed it (`up_sweep::place_layer`). Load-placed arms
    // are real tips too and count the same, in both sets.
    let mut roots_by_layer: Vec<Vec<usize>> = vec![Vec::new(); context.layers];
    for (index, strut) in struts.iter().enumerate() {
        if strut.kind == crate::StrutKind::Arm && strut.k_top < context.layers {
            roots_by_layer[strut.k_top].push(index);
        }
    }
    // The worst potential among the cells that restored each arm; `NaN` for an
    // arm restored wholesale, with no cell of its own to name. Reporting only.
    let mut blame_u = vec![f32::NEG_INFINITY; struts.len()];
    let mut verified = false;
    for restart in 0..=context.max_restarts {
        outcome.replays += 1;
        let pass = reference_replay_once(
            context,
            struts,
            &roots_by_layer,
            doomed,
            &mut blame_u,
            &mut outcome,
            restart == 0,
        );
        if matches!(pass, Pass::Verified) || !doomed.iter().any(|flag| *flag) {
            verified = true;
            break;
        }
    }
    if !verified {
        // Out of restarts: keep everything. Over-support, never a bare cell.
        restore_all(doomed, &mut blame_u, &mut outcome);
        outcome.exhausted = true;
    }
    let ratio = context.first_loss_ratio as f32;
    outcome.refused_within_first_loss = blame_u
        .iter()
        .filter(|u| u.is_finite() && **u <= ratio)
        .count() as u64;
    (outcome, blame_u)
}

/// The `replay.cell_layers` charge: the cells of the grid times the layers the
/// replays walked, saturating.
fn replay_cell_layers(cells: usize, layers_visited: u64) -> u64 {
    crate::perf::count(cells).saturating_mul(layers_visited)
}

fn restore_all(doomed: &mut [bool], blame_u: &mut [f32], outcome: &mut ReplayOutcome) {
    for (flag, blame) in doomed.iter_mut().zip(blame_u.iter_mut()) {
        if *flag {
            *flag = false;
            *blame = f32::NAN;
            outcome.refused += 1;
        }
    }
}

enum Pass {
    Verified,
    Restart,
    /// The pose's cancel token was set ([`crate::cancel`]): the walk stopped
    /// at a layer boundary and `doomed` holds a half-judged answer. Only a
    /// pose that is being discarded can see this, and its caller
    /// (`remove_under_loaded`'s stage) ends the pose at the next safe point
    /// before anything reads the answer.
    Cancelled,
}

fn reference_replay_once(
    context: &ReplayContext<'_>,
    struts: &[SweepStrut],
    roots_by_layer: &[Vec<usize>],
    doomed: &mut [bool],
    blame_u: &mut [f32],
    outcome: &mut ReplayOutcome,
    first: bool,
) -> Pass {
    let ratio = context.first_loss_ratio as f32;
    let grid = context.grid;
    let doomed_layers = || {
        doomed
            .iter()
            .enumerate()
            .filter(|(_, flag)| **flag)
            .map(|(index, _)| struts[index].k_top)
    };
    let Some(lowest) = doomed_layers().min() else {
        return Pass::Verified;
    };
    let highest = doomed_layers().max().unwrap_or(lowest);
    let mut creep = if context.creep_weight {
        SurfaceCreep::new(
            context.mesh,
            grid,
            context.overhang_centre_deg,
            context.overhang_transition_deg,
            context.creep_overhang_cut,
        )
    } else {
        SurfaceCreep::disabled(grid)
    };
    // The layers' masks are the plan's cached ones (a function of the slices
    // and the grid alone), not re-rasterised for every replay.
    // (A layer the cache's byte budget did not keep is computed on demand.)
    let layer_cache = LayerCache::resolve(context.layer_cache, grid, context.slices);
    let mut layer_reader = layer_cache.reader();
    // The up-sweep's own metric switch (`run_sweep`), applied to both sets.
    let new_state = || {
        let mut state = ReachState::new(grid);
        if !context.straight_reach {
            state.straight_limit = f32::NEG_INFINITY;
        }
        state
    };
    // "Nearest" doomed arm, for the restore below, in the same metric.
    let metric = crate::ReachMetric {
        reach_mm: context.kappa * context.layer_height_mm,
        straight: context.straight_reach,
    };
    let mut current = new_state();
    let mut proposed: Option<ReachState> = None;
    let mut trial_done = false;
    let mut queue = Vec::new();
    let mut touched = Vec::new();
    let mut fresh = Vec::new();
    let relax = |state: &mut ReachState,
                 cover: &[bool],
                 cell: usize,
                 queue: &mut Vec<usize>,
                 touched: &mut Vec<usize>| {
        touched.clear();
        reach::relax_from_root(grid, cover, context.kappa, cell, state, queue, touched);
    };

    for layer in 0..context.layers {
        // Every exit below, a return or the end of the loop, has walked
        // exactly the layers counted here.
        outcome.layers_visited += 1;
        // The layer below is prepared with this one above it: the recompute
        // reads this layer's cell list and clearance as that layer's "above".
        layer_reader.prepare(layer.saturating_sub(1), layer > 0);
        let masks = layer_reader.masks(layer);
        if !layer_reader.cover_any(layer) {
            // The up-sweep's own empty-layer reset, for both sets.
            current.u.fill(f32::INFINITY);
            reach::commit_layer(&mut current, &masks.carry);
            if let Some(state) = proposed.as_mut() {
                state.u.fill(f32::INFINITY);
                reach::commit_layer(state, &masks.carry);
            }
            continue;
        }
        creep.load_layer(grid, context.planes[layer] - context.layer_height_mm, context.planes[layer]);
        // Below the lowest doomed seat the two sets are the same set, so the
        // proposed one is only started there, from the current one's carry.
        if proposed.is_none() && !trial_done && layer >= lowest {
            let mut state = new_state();
            state.u_prev.copy_from_slice(&current.u_prev);
            proposed = Some(state);
        }
        reach::recompute_potential_layer(
            grid,
            &layer_reader,
            layer,
            context.kappa,
            context.layer_height_mm,
            &[],
            &creep,
            &mut current,
        );
        // The up-sweep's bridged births, read where it read them: before any
        // root of this layer relaxes.
        let bridged_current = context.births.bridged(layer, &masks.cover, &current);
        for &index in &roots_by_layer[layer] {
            relax(&mut current, &masks.cover, struts[index].cell, &mut queue, &mut touched);
        }
        if !bridged_current.is_empty() {
            let roots = crate::births::BirthRoots::of(
                layer,
                roots_by_layer[layer].iter().map(|&index| &struts[index]),
            );
            context
                .births
                .unroot_unseated(layer, &bridged_current, &roots, &masks.cover, &mut current);
        }
        if first {
            outcome.baseline_unrooted_cells += masks
                .cover
                .iter()
                .zip(&current.u)
                .filter(|(filled, u)| **filled && **u > 1.0)
                .count() as u64;
        }
        let Some(trial) = proposed.as_mut() else {
            reach::commit_layer(&mut current, &masks.carry);
            continue;
        };
        outcome.trial_layers += 1;
        reach::recompute_potential_layer(
            grid,
            &layer_reader,
            layer,
            context.kappa,
            context.layer_height_mm,
            &[],
            &creep,
            trial,
        );
        let bridged_trial = context.births.bridged(layer, &masks.cover, trial);
        for &index in &roots_by_layer[layer] {
            if !doomed[index] {
                relax(trial, &masks.cover, struts[index].cell, &mut queue, &mut touched);
            }
        }
        // Every cell the current set roots and the proposed one does not
        // restores the nearest doomed arm that could have rooted it.
        loop {
            // A bridged birth no surviving arm of this layer stands in is
            // unrooted, however a relax reached it (again after a restore).
            if !bridged_trial.is_empty() {
                let roots = crate::births::BirthRoots::of(
                    layer,
                    roots_by_layer[layer]
                        .iter()
                        .filter(|&&index| !doomed[index])
                        .map(|&index| &struts[index]),
                );
                context
                    .births
                    .unroot_unseated(layer, &bridged_trial, &roots, &masks.cover, trial);
            }
            fresh.clear();
            for cell in 0..grid.cells() {
                if masks.cover[cell] && trial.u[cell] > 1.0 && current.u[cell] <= 1.0 {
                    fresh.push(cell);
                }
            }
            if fresh.is_empty() {
                break;
            }
            outcome.newly_unrooted_cells += fresh.len() as u64;
            for &cell in &fresh {
                let u = trial.u[cell];
                if u.is_finite() {
                    outcome.newly_unrooted_max_u = outcome.newly_unrooted_max_u.max(f64::from(u));
                    if u <= ratio {
                        outcome.newly_unrooted_within_first_loss += 1;
                    }
                }
            }
            let mut restart = false;
            let mut restored_any = false;
            for &cell in &fresh {
                let u = trial.u[cell];
                if u <= 1.0 {
                    // Rooted by an arm restored earlier in this sweep of `fresh`.
                    continue;
                }
                let blamed = doomed
                    .iter()
                    .enumerate()
                    .filter(|(index, flag)| **flag && struts[*index].k_top <= layer)
                    .map(|(index, _)| (metric.distance_mm(grid, cell, struts[index].cell), index))
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let Some((_, index)) = blamed else {
                    continue;
                };
                doomed[index] = false;
                // A non-finite `u` (no root reaches the cell at all) is kept
                // as infinity, which no bracket contains.
                blame_u[index] = blame_u[index].max(u);
                outcome.refused += 1;
                restored_any = true;
                if struts[index].k_top == layer {
                    relax(trial, &masks.cover, struts[index].cell, &mut queue, &mut touched);
                } else {
                    restart = true;
                }
            }
            if restart {
                return Pass::Restart;
            }
            if !restored_any {
                // No doomed arm at or below this layer is left to blame, yet
                // the proposed set roots less than the current one. Nothing
                // proposed can be trusted: keep every arm.
                restore_all(doomed, blame_u, outcome);
                return Pass::Verified;
            }
        }
        reach::commit_layer(&mut current, &masks.carry);
        reach::commit_layer(trial, &masks.carry);
        // Above the highest doomed seat the two sets place the same roots, so
        // once the two carries agree every layer above agrees too. The first
        // replay still walks the current set to the top, so the residual it
        // reports is the whole model's and can be held against the placement's.
        if layer >= highest && trial.u_prev == current.u_prev {
            if !first {
                return Pass::Verified;
            }
            proposed = None;
            trial_done = true;
        }
    }
    Pass::Verified
}

// ---------------------------------------------------------------------------
// The call's reuse (lane RP, 2026-10-15)
// ---------------------------------------------------------------------------
//
// One call replays the same arm set over and over: the CURRENT set's potential
// (the baseline) does not depend on which arms are doomed, and a restart that
// restores an arm seated at layer `s` leaves every layer below `s` exactly as
// the replay before it computed them. So a call keeps, per layer, what the
// baseline left (the rooted cells and the carry). A restart then skips every
// baseline layer already computed (a cached layer costs a read, a layer past
// the cache is computed and appended), and starts the proposed set at the
// lowest doomed seat, from the cached baseline carry just below it. The
// proposed set is walked again from there: its layers are the one cost this
// reuse does not cut.
//
// The logical counts (`layers_visited`, `trial_layers`) are what a fresh replay
// reports. What the reuse saved is told apart in [`ReplayWork`].
//
// The baseline walk and the proposed walk are two chains, and only the proposed
// one reads the other (it is held against the baseline's rooted cells each
// layer). A call that is big enough ([`overlap_worthwhile`]) therefore walks
// the baseline a chunk ahead of the proposed walk, the two side by side in the
// one rayon pool (`run_walks`; no thread outside the pool), and the proposed
// walk takes each layer's baseline in layer order and commits it to the cache
// itself, so the cache, the budget decisions, every counter and the answer are
// what the serial walk gives. A smaller call walks the same chunks one after
// the other on its own thread: the same code.
//
// Every kept byte is drawn from the process-wide layer-cache pool
// ([`BytePool::process`]). When the pool refuses a baseline layer the call's
// cache is switched off and every later replay of the call is a fresh one.
// Nothing here changes a result at any budget.
// `PLINTH_SWEEP_VERIFY_REPLAY=1` runs the reference alongside every call and
// panics on any difference.

/// What the call's cache saved and what it still computed (the physical work,
/// as against [`ReplayOutcome`]'s logical counts, which equal a fresh replay's).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReplayWork {
    /// Layers whose baseline potential was computed here.
    pub baseline_computed: u64,
    /// ... of those, in replays after the first.
    pub baseline_computed_restart: u64,
    /// Baseline layers a replay walked or skipped without computing.
    pub baseline_reused: u64,
    /// Layers whose proposed potential was computed here.
    pub trial_computed: u64,
    /// Bytes the call's cache held at its end.
    pub cache_bytes: u64,
}

/// One layer of the CURRENT arm set's potential, as later replays need it.
struct BaselineLayer {
    /// Bit `c` is set where `c` is cover and the potential there is at most 1:
    /// the cells the proposed set is held against. Empty for a layer with no
    /// cover.
    rooted: Vec<u64>,
    /// The potential carried up from this layer, at its carry cells (ascending,
    /// as [`crate::layer_cache::LayerReader::carry_cells`]); infinity elsewhere.
    carry: Vec<f32>,
}

impl BaselineLayer {
    fn bytes(&self) -> u64 {
        (std::mem::size_of::<Self>() + self.rooted.capacity() * 8 + self.carry.capacity() * 4) as u64
    }
}

/// The bytes one call holds from the process pool.
struct Budget {
    pool: &'static BytePool,
    held: u64,
}

impl Budget {
    /// Take `bytes`; `false` when the pool is full.
    fn take(&mut self, bytes: u64) -> bool {
        let granted = self.pool.try_acquire(bytes);
        if granted {
            self.held += bytes;
        }
        granted
    }
}

impl Drop for Budget {
    fn drop(&mut self) {
        self.pool.release(self.held);
    }
}

/// Everything one call keeps across its replays.
struct CallCache {
    budget: Budget,
    /// Off after the pool refused a baseline layer: every replay is then fresh.
    enabled: bool,
    /// Layers `0..base.len()` of the baseline, in order.
    base: Vec<BaselineLayer>,
    /// Whether the replay about to run is the call's first.
    first: bool,
    work: ReplayWork,
}

impl CallCache {
    fn new(pool: &'static BytePool) -> Self {
        Self {
            budget: Budget { pool, held: 0 },
            enabled: true,
            base: Vec::new(),
            first: true,
            work: ReplayWork::default(),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Test-only fault injection: 1 reads a stale baseline. 0 is production.
    static SABOTAGE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn sabotage() -> u8 {
    SABOTAGE.with(std::cell::Cell::get)
}

#[cfg(not(test))]
const fn sabotage() -> u8 {
    0
}

/// Whether `PLINTH_SWEEP_VERIFY_REPLAY=1`: run the reference alongside every
/// call and panic on any difference (for the gate runs, not for production).
fn verify_replay() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("PLINTH_SWEEP_VERIFY_REPLAY").is_ok_and(|value| value.trim() == "1")
    })
}

/// The set bits of `words`, ascending.
fn set_bits(words: &[u64]) -> impl Iterator<Item = usize> + '_ {
    words.iter().enumerate().flat_map(|(word, &bits)| {
        let mut rest = bits;
        std::iter::from_fn(move || {
            if rest == 0 {
                return None;
            }
            let bit = rest.trailing_zeros() as usize;
            rest &= rest - 1;
            Some(word * 64 + bit)
        })
    })
}

/// The rooted set of a layer: cover cells at a potential of at most 1. With
/// `unrooted`, also counts the cover cells above 1 (the placement's residual).
fn rooted_words(cover: &[bool], u: &[f32], mut unrooted: Option<&mut u64>) -> Vec<u64> {
    let mut words = Vec::with_capacity(cover.len().div_ceil(64));
    for (cover, u) in cover.chunks(64).zip(u.chunks(64)) {
        let mut bits = 0_u64;
        for (bit, (filled, u)) in cover.iter().zip(u).enumerate() {
            if !*filled {
                continue;
            }
            if *u <= 1.0 {
                bits |= 1 << bit;
            } else if *u > 1.0 {
                if let Some(count) = unrooted.as_deref_mut() {
                    *count += 1;
                }
            }
        }
        words.push(bits);
    }
    words
}

/// The potential carried up from the layer, at its carry cells.
fn carry_values(state: &ReachState, carry_cells: &[u32]) -> Vec<f32> {
    carry_cells.iter().map(|&cell| state.u_prev[cell as usize]).collect()
}

/// Put `carry` (at `carry_cells`) into a fresh state's carried potential.
fn scatter_carry(state: &mut ReachState, carry_cells: &[u32], carry: &[f32]) {
    for (&cell, &value) in carry_cells.iter().zip(carry) {
        state.u_prev[cell as usize] = value;
    }
}

/// Restore every tentatively doomed arm whose removal would leave a demand
/// cell unrooted that the current field roots. `doomed` is edited in place;
/// what is still doomed on return passed a replay of the whole model.
///
/// Every replay of the call after the first reuses what the call already
/// computed (see the section comment above); the answer is the one a fresh
/// replay of every restart would give.
pub(crate) fn refuse_unrooted_demand(
    context: &ReplayContext<'_>,
    struts: &[SweepStrut],
    doomed: &mut [bool],
) -> ReplayOutcome {
    if verify_replay() && !crate::cancel::cancelled() {
        let mut expected_doomed = doomed.to_vec();
        let expected = reference_refuse_unrooted_demand(context, struts, &mut expected_doomed);
        let got = refuse_counted(context, struts, doomed, BytePool::process());
        // A cancelled pose (`crate::cancel`) truncated the cached replay at a
        // layer boundary while the reference ran whole: they cannot agree and
        // nothing reads the answer (the caller ends the pose at a safe point).
        if crate::cancel::cancelled() {
            return got.0;
        }
        assert!(
            same_answer(&got.0, doomed, &got.2, &expected.0, &expected_doomed, &expected.1),
            "PLINTH_SWEEP_VERIFY_REPLAY: the cached replay differs from the reference\n\
             cached    {:?} {doomed:?}\nreference {:?} {expected_doomed:?}",
            got.0,
            expected.0
        );
        return got.0;
    }
    refuse_counted(context, struts, doomed, BytePool::process()).0
}

/// Whether two calls answered alike: the outcome, the doomed flags and the
/// blame potentials by bits.
fn same_answer(
    outcome: &ReplayOutcome,
    doomed: &[bool],
    blame: &[f32],
    expected: &ReplayOutcome,
    expected_doomed: &[bool],
    expected_blame: &[f32],
) -> bool {
    outcome == expected
        && doomed == expected_doomed
        && blame.len() == expected_blame.len()
        && blame
            .iter()
            .zip(expected_blame)
            .all(|(a, b)| a.to_bits() == b.to_bits())
}

/// [`refuse_unrooted_demand`] with its cache drawn from `pool`, returning the
/// physical work it did and the blame potentials as well.
fn refuse_counted(
    context: &ReplayContext<'_>,
    struts: &[SweepStrut],
    doomed: &mut [bool],
    pool: &'static BytePool,
) -> (ReplayOutcome, ReplayWork, Vec<f32>) {
    let mut outcome = ReplayOutcome::default();
    if context.layers == 0 || !doomed.iter().any(|flag| *flag) {
        return (outcome, ReplayWork::default(), Vec::new());
    }
    let _call_span = crate::perf::span(crate::perf::Span::ReplayCall);
    let mut roots_by_layer: Vec<Vec<usize>> = vec![Vec::new(); context.layers];
    for (index, strut) in struts.iter().enumerate() {
        if strut.kind == crate::StrutKind::Arm && strut.k_top < context.layers {
            roots_by_layer[strut.k_top].push(index);
        }
    }
    let mut blame_u = vec![f32::NEG_INFINITY; struts.len()];
    let mut cache = CallCache::new(pool);
    let mut verified = false;
    for restart in 0..=context.max_restarts {
        outcome.replays += 1;
        cache.first = restart == 0;
        let pass = replay_once(
            context,
            struts,
            &roots_by_layer,
            doomed,
            &mut blame_u,
            &mut outcome,
            &mut cache,
        );
        if matches!(pass, Pass::Verified | Pass::Cancelled) || !doomed.iter().any(|flag| *flag) {
            verified = true;
            break;
        }
    }
    if !verified {
        // Out of restarts: keep everything. Over-support, never a bare cell.
        restore_all(doomed, &mut blame_u, &mut outcome);
        outcome.exhausted = true;
    }
    cache.work.cache_bytes = cache.budget.held;
    let work = cache.work;
    crate::perf::add(
        crate::PerfWork::ReplayCellLayers,
        replay_cell_layers(context.grid.cells(), outcome.layers_visited),
    );
    crate::perf::add(crate::PerfWork::ReplayBaselineLayers, work.baseline_computed);
    crate::perf::add(
        crate::PerfWork::ReplayBaselineLayersRestart,
        work.baseline_computed_restart,
    );
    crate::perf::add(crate::PerfWork::ReplayBaselineLayersReused, work.baseline_reused);
    crate::perf::add(crate::PerfWork::ReplayTrialLayers, work.trial_computed);
    crate::perf::add(crate::PerfWork::ReplayRestarts, outcome.replays.saturating_sub(1));
    crate::perf::add(crate::PerfWork::ReplayCacheBytes, work.cache_bytes);
    let ratio = context.first_loss_ratio as f32;
    outcome.refused_within_first_loss = blame_u
        .iter()
        .filter(|u| u.is_finite() && **u <= ratio)
        .count() as u64;
    (outcome, work, blame_u)
}

/// One replay of the call. The same walk as [`reference_replay_once`], with the
/// baseline taken from the call's cache where it holds it.
fn replay_once(
    context: &ReplayContext<'_>,
    struts: &[SweepStrut],
    roots_by_layer: &[Vec<usize>],
    doomed: &mut [bool],
    blame_u: &mut [f32],
    outcome: &mut ReplayOutcome,
    cache: &mut CallCache,
) -> Pass {
    let ratio = context.first_loss_ratio as f32;
    let grid = context.grid;
    let doomed_layers = || {
        doomed
            .iter()
            .enumerate()
            .filter(|(_, flag)| **flag)
            .map(|(index, _)| struts[index].k_top)
    };
    let Some(lowest) = doomed_layers().min() else {
        return Pass::Verified;
    };
    let highest = doomed_layers().max().unwrap_or(lowest);
    crate::perf::add(crate::PerfWork::ReplayCalls, 1);
    let first = cache.first;
    let layer_cache = LayerCache::resolve(context.layer_cache, grid, context.slices);
    let mut layer_reader = layer_cache.reader();
    let metric = crate::ReachMetric {
        reach_mm: context.kappa * context.layer_height_mm,
        straight: context.straight_reach,
    };
    let CallCache {
        budget,
        enabled,
        base,
        work,
        ..
    } = cache;
    let mut proposed: Option<ReachState> = None;
    let mut start = 0_usize;
    if *enabled && !first {
        // Below the lowest doomed seat the two sets are the same set and the
        // baseline is cached (when the cache reaches that far).
        start = lowest.min(base.len());
        if start == lowest && start > 0 {
            let mut state = new_reach_state(context);
            layer_reader.prepare(start - 1, false);
            scatter_carry(&mut state, layer_reader.carry_cells(start - 1), &base[start - 1].carry);
            proposed = Some(state);
        } else if start == lowest {
            proposed = Some(new_reach_state(context));
        }
        work.baseline_reused += start as u64;
        outcome.layers_visited += start as u64;
    }
    let creep = if context.creep_weight {
        SurfaceCreep::new(
            context.mesh,
            grid,
            context.overhang_centre_deg,
            context.overhang_transition_deg,
            context.creep_overhang_cut,
        )
    } else {
        SurfaceCreep::disabled(grid)
    };
    // The CURRENT set's potential does not depend on which arms are doomed, so
    // its walk (the baseline) runs on a thread of its own, a few layers ahead of
    // the proposed set's walk, which reads each layer's result in layer order
    // and commits it to the call's cache exactly as a serial walk would. The
    // baseline walk starts at the first layer the cache does not hold, from the
    // carry the cache holds just below it.
    let produce_from = if *enabled { base.len() } else { start };
    let mut current = new_reach_state(context);
    if start > 0 && produce_from < context.layers {
        assert!(
            *enabled && produce_from == base.len() && produce_from > 0,
            "the replay joined the baseline at layer {produce_from} with {} layers cached",
            base.len()
        );
        layer_reader.prepare(produce_from - 1, false);
        scatter_carry(
            &mut current,
            layer_reader.carry_cells(produce_from - 1),
            &base[produce_from - 1].carry,
        );
    }
    let baseline = BaselineWalker::new(BaselineWalk {
        context,
        struts,
        roots_by_layer,
        layer_cache: &layer_cache,
        from: produce_from,
        current,
        first,
        lowest,
        want_snapshot: proposed.is_none(),
    });
    let walk = TrialWalk {
        context,
        struts,
        roots_by_layer,
        doomed,
        blame_u,
        outcome,
        budget,
        enabled,
        base,
        work,
        first,
        lowest,
        highest,
        start,
        proposed,
        creep,
        layer_reader,
        metric,
        ratio,
        trial_done: false,
        queue: Vec::new(),
        touched: Vec::new(),
        fresh: Vec::new(),
    };
    let overlap =overlap_worthwhile(context, context.layers.saturating_sub(produce_from));
    run_walks(baseline, walk, produce_from, overlap)
}

/// Whether the two walks overlap: a layer's walk costs about its cell count and
/// a hand-over costs a rayon task, so a small grid or a short walk is taken
/// serially (the same chunks, in the same order, on the one thread).
fn overlap_worthwhile(context: &ReplayContext<'_>, layers_to_walk: usize) -> bool {
    #[cfg(test)]
    if let Some(forced) = FORCE_OVERLAP.with(std::cell::Cell::get) {
        return forced;
    }
    layers_to_walk >= OVERLAP_MIN_LAYERS
        && context.grid.cells() >= OVERLAP_MIN_CELLS
        && rayon::current_num_threads() > 1
}

/// Fewest grid cells for which the walks overlap.
const OVERLAP_MIN_CELLS: usize = 16_384;
/// Fewest layers of baseline to walk for which the walks overlap.
const OVERLAP_MIN_LAYERS: usize = 16;
/// Layers per hand-over: the baseline walk finishes this many layers ahead
/// while the proposed walk takes the chunk before.
const CHUNK_LAYERS: usize = 8;

#[cfg(test)]
thread_local! {
    /// Test-only: `Some(true)` or `Some(false)` forces the walks to overlap or
    /// to run serially; `None` is production.
    static FORCE_OVERLAP: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// The baseline layers of `from..to` that the cache does not hold (those at or
/// above `produce_from`), in layer order.
fn produce_chunk(
    baseline: &mut BaselineWalker<'_, '_>,
    produce_from: usize,
    from: usize,
    to: usize,
) -> Vec<ProducedLayer> {
    (from.max(produce_from)..to)
        .map(|_| baseline.step().expect("the baseline walk ended before the last layer"))
        .collect()
}

/// Walk both chains over the layers, chunk by chunk. The proposed walk is the
/// only one that reads the other (a layer's baseline), so while it takes chunk
/// `c` the baseline walk finishes chunk `c + 1`, the two side by side in the
/// one rayon pool (`rayon::join` never blocks on work that was not started: a
/// chunk nobody steals runs on the calling thread, after the other, giving
/// the serial walk). The proposed walk commits each layer's baseline to the
/// call's cache in layer order, so the cache, every counter and the answer are
/// what the serial walk gives.
fn run_walks(
    mut baseline: BaselineWalker<'_, '_>,
    mut walk: TrialWalk<'_, '_>,
    produce_from: usize,
    overlap: bool,
) -> Pass {
    let layers = walk.context.layers;
    // The cancel token is a thread-local of the pose's driving thread, and
    // `rayon::join` may run the proposed walk's chunk on any worker (one that
    // is driving another pose, or none). The chunk polls the token of the pose
    // that owns this call, read here and re-installed around it.
    let token = crate::cancel::current_token();
    let mut from = walk.start;
    let mut ready = produce_chunk(
        &mut baseline,
        produce_from,
        from,
        (from + CHUNK_LAYERS).min(layers),
    );
    while from < layers {
        let to = (from + CHUNK_LAYERS).min(layers);
        let after = (to + CHUNK_LAYERS).min(layers);
        let chunk = std::mem::take(&mut ready);
        let (next, flow) = if overlap && to < layers {
            rayon::join(
                || produce_chunk(&mut baseline, produce_from, to, after),
                || {
                    crate::cancel::with_carried(&token, || {
                        walk.chunk(from, to, chunk, produce_from)
                    })
                },
            )
        } else {
            let flow = walk.chunk(from, to, chunk, produce_from);
            let next = if flow.is_none() && to < layers {
                produce_chunk(&mut baseline, produce_from, to, after)
            } else {
                Vec::new()
            };
            (next, flow)
        };
        if let Some(pass) = flow {
            return pass;
        }
        ready = next;
        from = to;
    }
    Pass::Verified
}

/// A fresh reach state, in the up-sweep's metric.
fn new_reach_state(context: &ReplayContext<'_>) -> ReachState {
    let mut state = ReachState::new(context.grid);
    if !context.straight_reach {
        state.straight_limit = f32::NEG_INFINITY;
    }
    state
}

/// One layer of the baseline walk, as it goes to the proposed walk.
struct ProducedLayer {
    layer: usize,
    /// The layer has no cover: nothing was computed, the carry is all infinity.
    empty: bool,
    record: BaselineLayer,
    /// The cover cells the current set leaves unrooted here, counted on the
    /// first replay only.
    unrooted: u64,
    /// The current set's carried potential on entry to this layer: sent with
    /// the layer the proposed set starts at, when it starts from the walk
    /// rather than from the cache.
    snapshot: Option<Vec<f32>>,
}

/// What the baseline walk reads and starts from.
struct BaselineWalk<'a, 'c> {
    context: &'a ReplayContext<'c>,
    struts: &'a [SweepStrut],
    roots_by_layer: &'a [Vec<usize>],
    layer_cache: &'a LayerCache<'c>,
    /// The first layer to walk.
    from: usize,
    /// The carried potential on entry to `from`.
    current: ReachState,
    first: bool,
    lowest: usize,
    /// Whether the proposed set starts from this walk's carry (at the first
    /// layer with cover at or above `lowest`).
    want_snapshot: bool,
}

/// The CURRENT arm set's potential, one layer per [`Self::step`], from
/// `BaselineWalk::from` up. Run on a thread of its own or by the proposed walk
/// itself; either way it produces the same layers in the same order.
struct BaselineWalker<'a, 'c> {
    context: &'a ReplayContext<'c>,
    struts: &'a [SweepStrut],
    roots_by_layer: &'a [Vec<usize>],
    layer_reader: crate::layer_cache::LayerReader<'a, 'c>,
    creep: SurfaceCreep<'c>,
    queue: Vec<usize>,
    touched: Vec<usize>,
    current: ReachState,
    first: bool,
    lowest: usize,
    want_snapshot: bool,
    /// The next layer to produce.
    next: usize,
}

impl<'a, 'c> BaselineWalker<'a, 'c> {
    fn new(walk: BaselineWalk<'a, 'c>) -> Self {
        let context = walk.context;
        let creep = if context.creep_weight {
            SurfaceCreep::new(
                context.mesh,
                context.grid,
                context.overhang_centre_deg,
                context.overhang_transition_deg,
                context.creep_overhang_cut,
            )
        } else {
            SurfaceCreep::disabled(context.grid)
        };
        Self {
            context,
            struts: walk.struts,
            roots_by_layer: walk.roots_by_layer,
            layer_reader: walk.layer_cache.reader(),
            creep,
            queue: Vec::new(),
            touched: Vec::new(),
            current: walk.current,
            first: walk.first,
            lowest: walk.lowest,
            want_snapshot: walk.want_snapshot,
            next: walk.from,
        }
    }

    /// The next layer's baseline; `None` above the top layer.
    fn step(&mut self) -> Option<ProducedLayer> {
        let context = self.context;
        let grid = context.grid;
        let layer = self.next;
        if layer >= context.layers {
            return None;
        }
        self.next += 1;
        // The layer below is prepared with this one above it: the recompute
        // reads this layer's cell list and clearance as that layer's "above".
        self.layer_reader.prepare(layer.saturating_sub(1), layer > 0);
        let masks = self.layer_reader.masks(layer);
        let carry_cells = self.layer_reader.carry_cells(layer);
        if !self.layer_reader.cover_any(layer) {
            // The up-sweep's own empty-layer reset.
            self.current.u.fill(f32::INFINITY);
            reach::commit_layer(&mut self.current, &masks.carry);
            return Some(ProducedLayer {
                layer,
                empty: true,
                record: BaselineLayer {
                    rooted: Vec::new(),
                    carry: vec![f32::INFINITY; carry_cells.len()],
                },
                unrooted: 0,
                snapshot: None,
            });
        }
        self.creep.load_layer(
            grid,
            context.planes[layer] - context.layer_height_mm,
            context.planes[layer],
        );
        let snapshot = if self.want_snapshot && layer >= self.lowest {
            self.want_snapshot = false;
            Some(self.current.u_prev.clone())
        } else {
            None
        };
        let _baseline_span = crate::perf::span(if self.first {
            crate::perf::Span::ReplayBaseline
        } else {
            crate::perf::Span::ReplayBaselineRestart
        });
        reach::recompute_potential_layer(
            grid,
            &self.layer_reader,
            layer,
            context.kappa,
            context.layer_height_mm,
            &[],
            &self.creep,
            &mut self.current,
        );
        // The up-sweep's bridged births, read where it read them: before any
        // root of this layer relaxes.
        let bridged_current = context.births.bridged(layer, &masks.cover, &self.current);
        for &index in &self.roots_by_layer[layer] {
            self.touched.clear();
            reach::relax_from_root(
                grid,
                &masks.cover,
                context.kappa,
                self.struts[index].cell,
                &mut self.current,
                &mut self.queue,
                &mut self.touched,
            );
        }
        if !bridged_current.is_empty() {
            let roots = crate::births::BirthRoots::of(
                layer,
                self.roots_by_layer[layer].iter().map(|&index| &self.struts[index]),
            );
            context.births.unroot_unseated(
                layer,
                &bridged_current,
                &roots,
                &masks.cover,
                &mut self.current,
            );
        }
        let mut unrooted = 0_u64;
        let rooted = rooted_words(
            &masks.cover,
            &self.current.u,
            self.first.then_some(&mut unrooted),
        );
        reach::commit_layer(&mut self.current, &masks.carry);
        Some(ProducedLayer {
            layer,
            empty: false,
            record: BaselineLayer {
                rooted,
                carry: carry_values(&self.current, carry_cells),
            },
            unrooted,
            snapshot,
        })
    }
}

/// What the proposed walk reads, edits and reports.
struct TrialWalk<'a, 'c> {
    context: &'a ReplayContext<'c>,
    struts: &'a [SweepStrut],
    roots_by_layer: &'a [Vec<usize>],
    doomed: &'a mut [bool],
    blame_u: &'a mut [f32],
    outcome: &'a mut ReplayOutcome,
    budget: &'a mut Budget,
    enabled: &'a mut bool,
    base: &'a mut Vec<BaselineLayer>,
    work: &'a mut ReplayWork,
    first: bool,
    lowest: usize,
    highest: usize,
    start: usize,
    /// The proposed set's potential where it already runs at `start`.
    proposed: Option<ReachState>,
    creep: SurfaceCreep<'c>,
    layer_reader: crate::layer_cache::LayerReader<'a, 'c>,
    metric: crate::ReachMetric,
    ratio: f32,
    /// Set on the first replay once the two carries agree above the highest
    /// doomed seat: the proposed set is done and only the baseline walk goes on.
    trial_done: bool,
    queue: Vec<usize>,
    touched: Vec<usize>,
    fresh: Vec<usize>,
}

impl TrialWalk<'_, '_> {
    /// The layers `from..to`. `produced` holds the baseline of those at or
    /// above `produce_from` (the ones the call's cache did not hold), in order.
    /// `Some(pass)` ends the replay.
    fn chunk(
        &mut self,
        from: usize,
        to: usize,
        produced: Vec<ProducedLayer>,
        produce_from: usize,
    ) -> Option<Pass> {
        let mut produced = produced.into_iter();
        for layer in from..to {
            let item = (layer >= produce_from)
                .then(|| produced.next().expect("the baseline walk fell short of the chunk"));
            if let Some(pass) = self.layer(layer, item) {
                return Some(pass);
            }
        }
        None
    }

    /// One layer of the proposed set's walk, held against the baseline: the
    /// layer's baseline comes from the call's cache, or from `produced` (the
    /// baseline walk) and is committed to the cache here, in order. `Some(pass)`
    /// ends the replay.
    fn layer(&mut self, layer: usize, produced: Option<ProducedLayer>) -> Option<Pass> {
        let context = self.context;
        let struts = self.struts;
        let roots_by_layer = self.roots_by_layer;
        let doomed = &mut *self.doomed;
        let blame_u = &mut *self.blame_u;
        let outcome = &mut *self.outcome;
        let budget = &mut *self.budget;
        let enabled = &mut *self.enabled;
        let base = &mut *self.base;
        let work = &mut *self.work;
        let (first, lowest, highest, ratio) = (self.first, self.lowest, self.highest, self.ratio);
        let metric = &self.metric;
        let proposed = &mut self.proposed;
        let creep = &mut self.creep;
        let layer_reader = &mut self.layer_reader;
        let trial_done = &mut self.trial_done;
        let queue = &mut self.queue;
        let touched = &mut self.touched;
        let fresh = &mut self.fresh;
        let grid = context.grid;
        let relax = |state: &mut ReachState,
                     cover: &[bool],
                     cell: usize,
                     queue: &mut Vec<usize>,
                     touched: &mut Vec<usize>| {
            touched.clear();
            reach::relax_from_root(grid, cover, context.kappa, cell, state, queue, touched);
        };

        // S0 A/B resolution of the CX cancel point (7373216) onto rr's
        // restructured walk: one thread-local read per layer.
        if crate::cancel::cancelled() {
            return Some(Pass::Cancelled);
        }
        outcome.layers_visited += 1;
        let cached = *enabled && layer < base.len();
        let mut produced_empty = false;
        let mut local: Option<BaselineLayer> = None;
        if cached {
            work.baseline_reused += 1;
            assert!(
                produced.is_none() && proposed.is_some(),
                "a cached baseline layer was walked before the proposed set started"
            );
        } else {
            let produced = produced.expect("an uncached layer came without its baseline");
            assert_eq!(produced.layer, layer, "the baseline walk is out of step with the proposed walk");
            let ProducedLayer {
                empty,
                record,
                unrooted,
                snapshot,
                ..
            } = produced;
            work.baseline_computed += 1;
            work.baseline_computed_restart += u64::from(!first);
            outcome.baseline_unrooted_cells += unrooted;
            produced_empty = empty;
            if empty {
                if *enabled && layer == base.len() {
                    if budget.take(record.bytes()) {
                        base.push(record);
                    } else {
                        *enabled = false;
                    }
                }
            } else if *enabled && layer == base.len() && budget.take(record.bytes()) {
                base.push(record);
            } else {
                *enabled = false;
                local = Some(record);
            }
            // Below the lowest doomed seat the two sets are the same set, so the
            // proposed one is only started there, from the current one's carry.
            if let Some(snapshot) = snapshot {
                assert!(
                    proposed.is_none() && !*trial_done && layer >= lowest,
                    "the proposed set starts from a baseline that was not walked"
                );
                let mut state = new_reach_state(context);
                state.u_prev = snapshot;
                *proposed = Some(state);
            }
            if proposed.is_none() {
                return None;
            }
        }
        // The layer below is prepared with this one above it: the recompute
        // reads this layer's cell list and clearance as that layer's "above".
        layer_reader.prepare(layer.saturating_sub(1), layer > 0);
        let masks = layer_reader.masks(layer);
        let carry_cells = layer_reader.carry_cells(layer);
        let empty = if cached {
            !layer_reader.cover_any(layer)
        } else {
            produced_empty
        };
        if empty {
            // The up-sweep's own empty-layer reset.
            if let Some(state) = proposed.as_mut() {
                state.u.fill(f32::INFINITY);
                reach::commit_layer(state, &masks.carry);
            }
            return None;
        }
        let base_layer: &BaselineLayer = if cached {
            &base[layer]
        } else {
            local.as_ref().unwrap_or_else(|| &base[layer])
        };
        let trial = proposed.as_mut()?;
        creep.load_layer(
            grid,
            context.planes[layer] - context.layer_height_mm,
            context.planes[layer],
        );
        outcome.trial_layers += 1;
        work.trial_computed += 1;
        let _trial_span = crate::perf::span(crate::perf::Span::ReplayTrial);
        reach::recompute_potential_layer(
            grid,
            layer_reader,
            layer,
            context.kappa,
            context.layer_height_mm,
            &[],
            creep,
            trial,
        );
        let bridged_trial = context.births.bridged(layer, &masks.cover, trial);
        for &index in &roots_by_layer[layer] {
            if !doomed[index] {
                relax(trial, &masks.cover, struts[index].cell, queue, touched);
            }
        }
        // The cells the current set roots (`rooted`); a stale baseline is the
        // sabotage that claims every cell rooted.
        let every_cell;
        let rooted: &[u64] = if sabotage() == 1 && cached {
            // Sabotage: a stale baseline that claims every cell rooted.
            let cells = trial.u.len();
            every_cell = (0..base_layer.rooted.len())
                .map(|word| match cells.saturating_sub(word * 64).min(64) {
                    64 => u64::MAX,
                    left => (1_u64 << left) - 1,
                })
                .collect::<Vec<u64>>();
            &every_cell
        } else {
            &base_layer.rooted
        };
        // Every cell the current set roots and the proposed one does not
        // restores the nearest doomed arm that could have rooted it.
        loop {
            // A bridged birth no surviving arm of this layer stands in is
            // unrooted, however a relax reached it (again after a restore).
            if !bridged_trial.is_empty() {
                let roots = crate::births::BirthRoots::of(
                    layer,
                    roots_by_layer[layer]
                        .iter()
                        .filter(|&&index| !doomed[index])
                        .map(|&index| &struts[index]),
                );
                context
                    .births
                    .unroot_unseated(layer, &bridged_trial, &roots, &masks.cover, trial);
            }
            fresh.clear();
            fresh.extend(set_bits(rooted).filter(|&cell| trial.u[cell] > 1.0));
            if fresh.is_empty() {
                break;
            }
            outcome.newly_unrooted_cells += fresh.len() as u64;
            for &cell in fresh.iter() {
                let u = trial.u[cell];
                if u.is_finite() {
                    outcome.newly_unrooted_max_u = outcome.newly_unrooted_max_u.max(f64::from(u));
                    if u <= ratio {
                        outcome.newly_unrooted_within_first_loss += 1;
                    }
                }
            }
            let mut restart = false;
            let mut restored_any = false;
            for &cell in fresh.iter() {
                let u = trial.u[cell];
                if u <= 1.0 {
                    // Rooted by an arm restored earlier in this sweep of `fresh`.
                    continue;
                }
                let blamed = doomed
                    .iter()
                    .enumerate()
                    .filter(|(index, flag)| **flag && struts[*index].k_top <= layer)
                    .map(|(index, _)| (metric.distance_mm(grid, cell, struts[index].cell), index))
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let Some((_, index)) = blamed else {
                    continue;
                };
                doomed[index] = false;
                // A non-finite `u` (no root reaches the cell at all) is kept
                // as infinity, which no bracket contains.
                blame_u[index] = blame_u[index].max(u);
                outcome.refused += 1;
                restored_any = true;
                if struts[index].k_top == layer {
                    relax(trial, &masks.cover, struts[index].cell, queue, touched);
                } else {
                    restart = true;
                }
            }
            if restart {
                return Some(Pass::Restart);
            }
            if !restored_any {
                // No doomed arm at or below this layer is left to blame, yet
                // the proposed set roots less than the current one. Nothing
                // proposed can be trusted: keep every arm.
                restore_all(doomed, blame_u, outcome);
                return Some(Pass::Verified);
            }
        }
        reach::commit_layer(trial, &masks.carry);
        // Above the highest doomed seat the two sets place the same roots, so
        // once the two carries agree every layer above agrees too. The first
        // replay still walks the current set to the top, so the residual it
        // reports is the whole model's and can be held against the placement's.
        if layer >= highest
            && carry_cells
                .iter()
                .zip(&base_layer.carry)
                // Exact equality, as the reference's `Vec<f32> ==`.
                .all(|(&cell, value)| trial.u_prev[cell as usize].eq(value))
        {
            if !first {
                return Some(Pass::Verified);
            }
            *proposed = None;
            *trial_done = true;
        }
        None
    }
}

// ---------------------------------------------------------------------------
// The cover a seat move leaves (lane tc, tip cover, 2026-10-16)
// ---------------------------------------------------------------------------

/// **A tip whose seat the footprint rule slid** ([`crate::footprint`]): the
/// member, where its contact now stands, and its contact disc's radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MovedSeat {
    /// The member's index in the strut list.
    pub index: usize,
    /// The slid seat, in the plane (`center_mm` plus the slide).
    pub xy_mm: [f64; 2],
    /// The radius of the contact disc the moved ball cuts. Where the slid
    /// seat's own column is not cover at the seat layer, the root is the
    /// nearest cover cell inside this disc: the ball touches the model there.
    pub contact_radius_mm: f64,
    /// The UNGROWN balls before and after the slide (centre, radius), when
    /// known: the root moves by as much as the centre of the cells the ball
    /// HOLDS at the seat layer moved ([`moved_root_cell`]).
    pub placed_ball: Option<([f64; 3], f64)>,
    pub ball: Option<([f64; 3], f64)>,
}

/// **A demand cell a set of seat moves left unrooted** that the placed seats
/// rooted ([`stranded_by_moves`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StrandedCell {
    pub layer: usize,
    pub cell: usize,
    /// The cell's potential with the moves (`> 1`; infinite where no root
    /// reaches it at all).
    pub u: f32,
    /// The moved member blamed: of the moved seats at or below the cell's
    /// layer whose root moved, the one whose PLACED root is nearest the cell
    /// (the one that was covering it), in the up-sweep's metric; ties to the
    /// lower index.
    pub blamed: usize,
    /// The cell's column was not stranded one layer down: a cell of model
    /// SURFACE the moves left, not one that only inherits a stranded cell
    /// below it (the count of [`crate::reach::commit_layer`]'s carry upward).
    pub surface: bool,
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "raster coordinates of a seat on the model are small and clamped to the grid"
)]
fn clamped_cell(grid: &Grid, xy: [f64; 2]) -> (usize, usize) {
    let clamp = |coord: f64, count: usize| -> usize {
        (coord.floor().max(0.0) as usize).min(count.saturating_sub(1))
    };
    (clamp(grid.row_coord(xy[1]), grid.rows()), clamp(grid.col_coord(xy[0]), grid.cols()))
}

/// The cover cells a ball HOLDS at a layer, with their centres: its section
/// at the layer's mid-plane, or at the one below where the cell is cured onto
/// it, covers the whole cell (`Ground::holds` in [`crate::footprint`]). In
/// cell order.
fn held_cells(
    grid: &Grid,
    cover: &[bool],
    (centre, radius): ([f64; 3], f64),
    mids: [Option<f64>; 2],
) -> Vec<(usize, [f64; 2])> {
    let pitch = grid.pitch_mm();
    let mut held = Vec::new();
    if !(radius > 0.0 && pitch > 0.0) {
        return held;
    }
    let half_diagonal = crate::member_clearance::raster_half_diagonal_mm(grid);
    let (row, col) = clamped_cell(grid, [centre[0], centre[1]]);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a ball radius of at most a few mm over the pitch is small"
    )]
    let span = (radius / pitch).ceil() as usize + 1;
    let sections: Vec<f64> = mids
        .iter()
        .flatten()
        .map(|z| radius * radius - (z - centre[2]) * (z - centre[2]))
        .filter(|section| *section > 0.0)
        .collect();
    for r in row.saturating_sub(span)..=(row + span).min(grid.rows().saturating_sub(1)) {
        for c in col.saturating_sub(span)..=(col + span).min(grid.cols().saturating_sub(1)) {
            let cell = grid.index(r, c);
            if !cover[cell] {
                continue;
            }
            let at = [grid.center_x_mm(c), grid.center_y_mm(r)];
            let rho = (at[0] - centre[0]).hypot(at[1] - centre[1]) + half_diagonal;
            if sections.iter().any(|section| rho * rho <= *section) {
                held.push((cell, at));
            }
        }
    }
    held
}

fn centroid(cells: &[(usize, [f64; 2])]) -> [f64; 2] {
    #[expect(clippy::cast_precision_loss, reason = "a patch's cell count is small")]
    let count = cells.len().max(1) as f64;
    let sum = cells.iter().fold([0.0, 0.0], |sum, (_, at)| [sum[0] + at[0], sum[1] + at[1]]);
    [sum[0] / count, sum[1] / count]
}

/// The cell a moved seat roots at its seat layer. The planner roots an arm at
/// ONE cell, its seat column (`placed_cell`). A slide moves the tip's contact,
/// so the root moves with it:
///
/// 1. the root moves by as much as the CONTACT moved: the centre of the cover
///    cells the ball holds at this layer ([`held_cells`]: the one point of the
///    contact patch, as the printer view roots a tip, lane unroot's
///    `SEEDCENTRE`), after the slide less before it. Where either ball is
///    unknown or holds no whole cell here (a tip finer than the raster, or a
///    ball that only meets this layer at its edge), the contact is taken to
///    move with the seat: the slide itself. A slide that leaves the contact
///    where it was leaves the root where it was, whatever the offset between
///    the seat column and the contact's centre;
/// 2. the root is the cell there when it is cover; else the cover cell
///    nearest there within the contact disc's radius and one pixel (the
///    printer cannot part two solids closer, the tolerance `keeps_seat` reads;
///    ties to the lower index); else the cell there, a stray root
///    ([`reach::relax_from_root`] handles it).
///
/// `mids` are the mid-planes of this layer and of the one under it.
fn moved_root_cell(
    grid: &Grid,
    cover: &[bool],
    placed_cell: usize,
    seat: &MovedSeat,
    mids: [Option<f64>; 2],
) -> usize {
    let placed_xy = [
        grid.center_x_mm(grid.col_of(placed_cell)),
        grid.center_y_mm(grid.row_of(placed_cell)),
    ];
    let contact_shift = match (seat.placed_ball, seat.ball) {
        (Some(placed), Some(moved)) => {
            let before = held_cells(grid, cover, placed, mids);
            let after = held_cells(grid, cover, moved, mids);
            (!before.is_empty() && !after.is_empty()).then(|| {
                let (from, to) = (centroid(&before), centroid(&after));
                [to[0] - from[0], to[1] - from[1]]
            })
        }
        _ => None,
    };
    // The seat column is the placed cell's centre, so without a contact shift
    // the target is the slid seat itself.
    let target = contact_shift.map_or(seat.xy_mm, |shift| {
        [placed_xy[0] + shift[0], placed_xy[1] + shift[1]]
    });
    let (row, col) = clamped_cell(grid, target);
    let own = grid.index(row, col);
    let pitch = grid.pitch_mm();
    if cover.get(own).copied().unwrap_or(false) || !(pitch > 0.0) {
        return own;
    }
    let tolerance = seat.contact_radius_mm.max(0.0) + pitch;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a contact radius of at most a few mm over the pitch is small"
    )]
    let span = (tolerance / pitch).ceil() as usize;
    let mut best: Option<(f64, usize)> = None;
    for r in row.saturating_sub(span)..=(row + span).min(grid.rows().saturating_sub(1)) {
        for c in col.saturating_sub(span)..=(col + span).min(grid.cols().saturating_sub(1)) {
            let cell = grid.index(r, c);
            if !cover[cell] {
                continue;
            }
            let distance = (grid.center_x_mm(c) - target[0]).hypot(grid.center_y_mm(r) - target[1]);
            if distance > tolerance {
                continue;
            }
            // Nearest first, ties to the lower cell index (`total_cmp`: exact).
            if best.is_none_or(|(d, at)| distance.total_cmp(&d).then(cell.cmp(&at)).is_lt()) {
                best = Some((distance, cell));
            }
        }
    }
    best.map_or(own, |(_, cell)| cell)
}

/// **The demand cells a set of seat slides leaves unrooted** (lane tc,
/// 2026-10-16): the up-sweep's own reach potential, walked layer by layer over
/// the field twice, once with every arm rooted where it was PLACED and once
/// with each moved arm rooted where its slid seat stands, and every cover cell
/// the first roots (`U ≤ 1`) and the second does not.
///
/// It is the placement's own question asked of the field the slides leave
/// (the removal replay asks it of the field a removal would leave,
/// [`refuse_unrooted_demand`]); nothing here is a new limit.
///
/// What a slide changes, and what it does not:
///
/// * the moved arm stays a root at its seat LAYER (`k_top`). The footprint
///   rule refuses a slide that climbs off the laminae the tip was placed under
///   ([`crate::footprint`]'s `keeps_seat`), so the tip still meets them; a
///   root filed one layer off by rounding the drawn height is the artifact
///   lane unroot measured on Bat_Cat_Owl, not a change in support;
/// * it roots ONE cell, as every arm does: its placed root moved by as much
///   as its contact on the seat layer moved (the centre of the cells its ball
///   holds there, [`moved_root_cell`]: the printer view's one-point rooting).
///   The slid seat's own column can stand over another piece of the layer
///   than the one the ball touches (Bat_Cat_Owl's tip under the strap: its
///   centre slid onto the layer below while its ball still held the birth
///   above), and a ball's contact centre stands off its seat column wherever
///   the arm leans, by as much before the slide as after;
/// * the births rule reads the PLACED roots in both walks. A slide keeps the
///   material its placed ball held over nothing joined to ground
///   (`keeps_ground`), so its contact with a birth is what it was; a seat cell
///   that falls out of a 0.01 mm² birth by flooring is lane unroot's other
///   artifact;
/// * what changes is the LATERAL reach from the root, and that is what is
///   compared.
///
/// Every cell found is blamed on one moved seat ([`StrandedCell::blamed`]).
/// The walk stops once the two carries agree above the highest moved seat:
/// every layer above then agrees too.
pub(crate) fn stranded_by_moves(
    context: &ReplayContext<'_>,
    struts: &[SweepStrut],
    moved: &[MovedSeat],
) -> Vec<StrandedCell> {
    let mut stranded = Vec::new();
    let grid = context.grid;
    let moved: Vec<MovedSeat> = moved
        .iter()
        .copied()
        .filter(|seat| {
            struts.get(seat.index).is_some_and(|strut| {
                strut.kind == crate::StrutKind::Arm && strut.k_top < context.layers
            })
        })
        .collect();
    if context.layers == 0 || moved.is_empty() {
        return stranded;
    }
    let mut roots_by_layer: Vec<Vec<usize>> = vec![Vec::new(); context.layers];
    for (index, strut) in struts.iter().enumerate() {
        if strut.kind == crate::StrutKind::Arm && strut.k_top < context.layers {
            roots_by_layer[strut.k_top].push(index);
        }
    }
    let mut moved_of: Vec<Option<usize>> = vec![None; struts.len()];
    for (slot, seat) in moved.iter().enumerate() {
        moved_of[seat.index] = Some(slot);
    }
    // Whether each moved seat's root left its placed cell (set at its seat
    // layer, which the walk reaches before any layer it can be blamed on): a
    // slide that leaves the root where it was strands nothing and is never
    // blamed.
    let mut root_moved = vec![false; moved.len()];
    let lowest = moved.iter().map(|seat| struts[seat.index].k_top).min().unwrap_or(0);
    let highest = moved.iter().map(|seat| struts[seat.index].k_top).max().unwrap_or(0);
    let mut creep = if context.creep_weight {
        SurfaceCreep::new(
            context.mesh,
            grid,
            context.overhang_centre_deg,
            context.overhang_transition_deg,
            context.creep_overhang_cut,
        )
    } else {
        SurfaceCreep::disabled(grid)
    };
    let layer_cache = LayerCache::resolve(context.layer_cache, grid, context.slices);
    let mut layer_reader = layer_cache.reader();
    let metric = crate::ReachMetric {
        reach_mm: context.kappa * context.layer_height_mm,
        straight: context.straight_reach,
    };
    let mut current = new_reach_state(context);
    let mut proposed: Option<ReachState> = None;
    let mut queue = Vec::new();
    let mut touched = Vec::new();
    let mut below = vec![false; grid.cells()];
    let mut here = vec![false; grid.cells()];
    let relax = |state: &mut ReachState,
                 cover: &[bool],
                 cell: usize,
                 queue: &mut Vec<usize>,
                 touched: &mut Vec<usize>| {
        touched.clear();
        reach::relax_from_root(grid, cover, context.kappa, cell, state, queue, touched);
    };
    for layer in 0..context.layers {
        if crate::cancel::cancelled() {
            return Vec::new();
        }
        layer_reader.prepare(layer.saturating_sub(1), layer > 0);
        let masks = layer_reader.masks(layer);
        if !layer_reader.cover_any(layer) {
            current.u.fill(f32::INFINITY);
            reach::commit_layer(&mut current, &masks.carry);
            if let Some(state) = proposed.as_mut() {
                state.u.fill(f32::INFINITY);
                reach::commit_layer(state, &masks.carry);
            }
            below.fill(false);
            continue;
        }
        creep.load_layer(grid, context.planes[layer] - context.layer_height_mm, context.planes[layer]);
        if proposed.is_none() && layer >= lowest {
            let mut state = new_reach_state(context);
            state.u_prev.copy_from_slice(&current.u_prev);
            proposed = Some(state);
        }
        reach::recompute_potential_layer(
            grid,
            &layer_reader,
            layer,
            context.kappa,
            context.layer_height_mm,
            &[],
            &creep,
            &mut current,
        );
        let bridged_current = context.births.bridged(layer, &masks.cover, &current);
        for &index in &roots_by_layer[layer] {
            relax(&mut current, &masks.cover, struts[index].cell, &mut queue, &mut touched);
        }
        // The births rule reads the PLACED roots in both walks (see the doc).
        let placed_roots = || {
            crate::births::BirthRoots::of(layer, roots_by_layer[layer].iter().map(|&index| &struts[index]))
        };
        if !bridged_current.is_empty() {
            context
                .births
                .unroot_unseated(layer, &bridged_current, &placed_roots(), &masks.cover, &mut current);
        }
        let Some(trial) = proposed.as_mut() else {
            reach::commit_layer(&mut current, &masks.carry);
            continue;
        };
        reach::recompute_potential_layer(
            grid,
            &layer_reader,
            layer,
            context.kappa,
            context.layer_height_mm,
            &[],
            &creep,
            trial,
        );
        let bridged_trial = context.births.bridged(layer, &masks.cover, trial);
        // The mid-planes of this layer and the one under it (a layer's
        // material is born in `[plane - h, plane]`).
        let half = 0.5 * context.layer_height_mm;
        let mids = [
            Some(context.planes[layer] - half),
            layer.checked_sub(1).map(|below| context.planes[below] - half),
        ];
        for &index in &roots_by_layer[layer] {
            let cell = match moved_of[index] {
                Some(slot) => {
                    let cell =
                        moved_root_cell(grid, &masks.cover, struts[index].cell, &moved[slot], mids);
                    root_moved[slot] = cell != struts[index].cell;
                    cell
                }
                None => struts[index].cell,
            };
            relax(trial, &masks.cover, cell, &mut queue, &mut touched);
        }
        if !bridged_trial.is_empty() {
            context
                .births
                .unroot_unseated(layer, &bridged_trial, &placed_roots(), &masks.cover, trial);
        }
        here.fill(false);
        for cell in 0..grid.cells() {
            if !(masks.cover[cell] && trial.u[cell] > 1.0 && current.u[cell] <= 1.0) {
                continue;
            }
            here[cell] = true;
            // The nearest moved seat at or below the layer whose root moved;
            // the nearest moved seat of all only if none did (two identical
            // root sets cannot strand a cell, so that is never expected).
            let nearest = |only_moved: bool| {
                moved
                    .iter()
                    .enumerate()
                    .filter(|(slot, seat)| {
                        struts[seat.index].k_top <= layer && (root_moved[*slot] || !only_moved)
                    })
                    .map(|(_, seat)| {
                        (metric.distance_mm(grid, cell, struts[seat.index].cell), seat.index)
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            };
            let blamed = nearest(true).or_else(|| nearest(false));
            if let Some((_, blamed)) = blamed {
                stranded.push(StrandedCell {
                    layer,
                    cell,
                    u: trial.u[cell],
                    blamed,
                    surface: !below[cell],
                });
            }
        }
        std::mem::swap(&mut below, &mut here);
        reach::commit_layer(&mut current, &masks.carry);
        reach::commit_layer(trial, &masks.carry);
        if layer >= highest && trial.u_prev == current.u_prev {
            break;
        }
    }
    stranded
}

#[cfg(test)]
mod tests {
    //! Hand-derived geometry. A slab `S1` 4 mm long (x 0..4), 1 mm wide
    //! (y −0.5..0.5), its underside at z 1.0, so its first layer is layer 20
    //! at 0.05 mm (plane 1.025). Reach is one lamina, κ·h = 42.6006450 × 0.05
    //! = 2.1300 mm, measured as the straight length (`ReplayContext::
    //! straight_reach`, the production metric since lane lattice-align): a
    //! cell `dx` along and `dy` across is `√(dx² + dy²)` away. Under the
    //! 8-neighbour chamfer it is `(dx − dy) + √2·dy` when `dx ≥ dy`; every
    //! verdict below is the same in both metrics.
    //!
    //! The farthest cell of `S1` from the nearest of two arms at x 0.9 and
    //! 3.1 is at x 2.0, y ±0.5: `√(1.1² + 0.5²) = 1.208 mm` (chamfer
    //! `1.307 mm`), inside reach. The far end, x 4.0, from x 0.9 alone:
    //! `3.1 mm` in both, outside it.
    //!
    //! No production function decides anything in these tests: the expected
    //! verdicts are the two numbers above.

    use super::{refuse_unrooted_demand, ReplayContext};
    use crate::grid::{plan_grid, Grid};
    use crate::{Landing, StrutKind, SweepStrut};
    use plinth_mesh::{make_box, Mesh};
    use plinth_support::perception::{slice_stack, SliceStackLimits};

    const H: f64 = 0.05;
    const LAYERS: usize = 50;
    const KAPPA: f64 = 42.600_645;

    /// `S1` alone, or `S1` and `S2`: the same slab again, directly above it
    /// from z 2.0 (layer 40), and not touching it.
    fn fixture(with_upper: bool) -> Mesh {
        let lower = make_box([4.0, 1.0, 0.5], [2.0, 0.0, 1.0]).expect("S1");
        if !with_upper {
            return lower;
        }
        let upper = make_box([4.0, 1.0, 0.5], [2.0, 0.0, 2.0]).expect("S2");
        let offset = u32::try_from(lower.verts().len()).expect("small");
        let mut verts = lower.verts().to_vec();
        verts.extend_from_slice(upper.verts());
        let mut tris = lower.tris().to_vec();
        tris.extend(upper.tris().iter().map(|t| t.map(|v| v + offset)));
        Mesh::new(verts, tris).expect("two disjoint boxes")
    }

    fn cell_at(grid: &Grid, x: f64, y: f64) -> usize {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "fixture coordinates inside the grid's own bounds"
        )]
        let (col, row) = (grid.col_coord(x).floor() as usize, grid.row_coord(y).floor() as usize);
        row * grid.cols() + col
    }

    fn arm(grid: &Grid, id: u32, x: f64, k_top: usize) -> SweepStrut {
        SweepStrut {
            id,
            cell: cell_at(grid, x, 0.0),
            center_mm: [x, 0.0],
            kind: StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(), foot_radius_mm: None,
            seat_on_strut: None,
            k_top,
            k_bottom: 0,
            landing: Landing::Plate,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), held_births: Vec::new(), twin_host: false,
            inserted_by_load: false,
            path: Vec::new(),
            seat_face: crate::SeatFace::default(),
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
        }
    }

    fn run(
        mesh: &Mesh,
        place: &dyn Fn(&Grid) -> Vec<SweepStrut>,
        doomed: &mut Vec<bool>,
    ) -> super::ReplayOutcome {
        let (grid, _) = plan_grid(0.0, -0.5, 4.0, 0.5, LAYERS, H, 1.0, u64::MAX);
        let planes: Vec<f64> = (0..LAYERS).map(|l| (l as f64 + 0.5) * H).collect();
        let slices = slice_stack(mesh, &planes, SliceStackLimits::production()).expect("slices");
        let struts = place(&grid);
        if doomed.is_empty() {
            doomed.resize(struts.len(), false);
        }
        let context = ReplayContext {
            grid: &grid,
            slices: &slices,
            layer_cache: None,
            planes: &planes,
            mesh,
            kappa: KAPPA,
            layer_height_mm: H,
            layers: LAYERS,
            straight_reach: true,
            creep_weight: false,
            creep_overhang_cut: true,
            overhang_centre_deg: 45.0,
            overhang_transition_deg: 25.0,
            max_restarts: 3,
            first_loss_ratio: 2.592 / 2.130,
            births: &crate::births::Births::none(),
        };
        refuse_unrooted_demand(&context, &struts, doomed)
    }

    /// Three arms on `S1` at x 0.9, 2.0 and 3.1. The outer two reach every
    /// cell (worst 1.307 mm), so the middle one is not needed and stays
    /// doomed; nothing is refused and nothing is found unrooted.
    #[test]
    fn an_arm_whose_cells_two_neighbours_reach_is_let_go() {
        let mesh = fixture(false);
        let place =
            |g: &Grid| vec![arm(g, 0, 0.9, 20), arm(g, 1, 2.0, 20), arm(g, 2, 3.1, 20)];
        let mut doomed = vec![false, true, false];
        let outcome = run(&mesh, &place, &mut doomed);
        assert_eq!(doomed, vec![false, true, false], "{outcome:?}");
        assert_eq!(outcome.refused, 0);
        assert_eq!(outcome.newly_unrooted_cells, 0);
        assert_eq!(outcome.baseline_unrooted_cells, 0, "all three root the whole slab");
        assert!(outcome.replays >= 1 && outcome.trial_layers > 0, "the replay ran: {outcome:?}");
    }

    /// The same slab with two arms, x 0.9 and 3.1, and the one at 3.1 doomed:
    /// the far end is 3.1 mm from x 0.9, outside reach, so the arm is kept,
    /// and the cells that made it so are counted.
    #[test]
    fn an_arm_whose_far_cells_no_neighbour_reaches_is_kept() {
        let mesh = fixture(false);
        let place = |g: &Grid| vec![arm(g, 0, 0.9, 20), arm(g, 1, 3.1, 20)];
        let mut doomed = vec![false, true];
        let outcome = run(&mesh, &place, &mut doomed);
        assert_eq!(doomed, vec![false, false], "{outcome:?}");
        assert_eq!(outcome.refused, 1);
        assert!(outcome.newly_unrooted_cells > 0, "{outcome:?}");
    }

    /// BUILD ORDER. The same two arms on `S1`, plus an arm seated on `S2`
    /// directly above the doomed one (x 3.1, layer 40). In straight-line
    /// distance it stands on top of every cell the doomed arm roots, so a
    /// test that took coverage from an arm on ANY layer by distance alone
    /// would let the doomed arm go. It must not: when layer 20 cures and
    /// peels, the arm at layer 40 has not reached the model yet.
    #[test]
    fn an_arm_seated_above_does_not_cover_the_layer_below() {
        let mesh = fixture(true);
        let place =
            |g: &Grid| vec![arm(g, 0, 0.9, 20), arm(g, 1, 3.1, 20), arm(g, 2, 3.1, 40)];
        let mut doomed = vec![false, true, false];
        let outcome = run(&mesh, &place, &mut doomed);
        assert_eq!(doomed, vec![false, false, false], "{outcome:?}");
        assert_eq!(outcome.refused, 1);
        // And S2's own arm, doomed, is kept too: nothing else roots S2.
        let mut doomed = vec![false, false, true];
        let outcome = run(&mesh, &place, &mut doomed);
        assert_eq!(doomed, vec![false, false, false], "{outcome:?}");
    }

    /// X0 (2026-10-10): `replay.cell_layers` is charged for the layers a
    /// replay WALKED. The old charge, `cells * context.layers` at the top of
    /// every replay, counted all 50 layers here although this replay stops at
    /// layer 20. A duplicate of the arm at x 2.0 is doomed: the proposed set
    /// equals the current one, so a non-first replay sees equal carries at the
    /// highest doomed seat (layer 20) and returns there, 21 layers walked
    /// (layers 0..=20). The first replay of the same call walks the whole
    /// model, so only the early exit tells the two charges apart.
    #[test]
    fn a_replay_that_exits_early_is_charged_only_for_the_layers_it_walked() {
        use super::{replay_cell_layers, replay_once, CallCache, Pass, ReplayOutcome};
        let mesh = fixture(false);
        let (grid, _) = plan_grid(0.0, -0.5, 4.0, 0.5, LAYERS, H, 1.0, u64::MAX);
        let planes: Vec<f64> = (0..LAYERS).map(|l| (l as f64 + 0.5) * H).collect();
        let slices = slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("slices");
        let struts = vec![
            arm(&grid, 0, 0.9, 20),
            arm(&grid, 1, 2.0, 20),
            arm(&grid, 2, 2.0, 20),
            arm(&grid, 3, 3.1, 20),
        ];
        let context = ReplayContext {
            grid: &grid,
            slices: &slices,
            layer_cache: None,
            planes: &planes,
            mesh: &mesh,
            kappa: KAPPA,
            layer_height_mm: H,
            layers: LAYERS,
            straight_reach: true,
            creep_weight: false,
            creep_overhang_cut: true,
            overhang_centre_deg: 45.0,
            overhang_transition_deg: 25.0,
            max_restarts: 3,
            first_loss_ratio: 2.592 / 2.130,
            births: &crate::births::Births::none(),
        };
        let mut roots: Vec<Vec<usize>> = vec![Vec::new(); LAYERS];
        roots[20] = vec![0, 1, 2, 3];
        let mut doomed = vec![false, false, true, false];
        let mut blame = vec![f32::NEG_INFINITY; struts.len()];
        let mut outcome = ReplayOutcome::default();
        let mut cache = CallCache::new(crate::layer_cache::BytePool::process());
        cache.first = false;
        let pass = replay_once(&context, &struts, &roots, &mut doomed, &mut blame, &mut outcome, &mut cache);
        assert!(matches!(pass, Pass::Verified));
        assert_eq!(outcome.layers_visited, 21, "layers 0..=20 were walked: {outcome:?}");
        let cells = grid.cells() as u64;
        assert_eq!(replay_cell_layers(grid.cells(), outcome.layers_visited), cells * 21);
        assert_ne!(
            replay_cell_layers(grid.cells(), outcome.layers_visited),
            cells * LAYERS as u64,
            "the old charge, cells * context.layers"
        );

        // The first replay of the same field walks the whole model.
        let mut doomed = vec![false, false, true, false];
        let whole = refuse_unrooted_demand(&context, &struts, &mut doomed);
        assert_eq!(doomed, vec![false, false, true, false], "{whole:?}");
        assert_eq!(whole.replays, 1);
        assert_eq!(whole.layers_visited, LAYERS as u64, "{whole:?}");
    }

    /// The production early-exit call, shared by the child below and its
    /// parent. Four arms on the two-slab fixture: `G` (0.9) and `H` (3.1) are
    /// kept on `S1`; `A` duplicates `G` (same cell, layer 20) and is doomed;
    /// `D` (0.9, layer 40) is doomed and is the only root of `S2`. At layer 40
    /// the first replay finds `S2` unrooted in the proposed set and blames the
    /// nearest doomed arm: `A` and `D` tie on distance and `A` has the lower
    /// index, so `A` (seated BELOW the layer) is restored and the replay will
    /// restart; the next cells of the same layer blame `D`, which is restored
    /// too. The replay returns `Restart` at layer 40 with nothing left doomed,
    /// so the call ends after ONE replay that walked 41 layers (0..=40), not 50.
    fn production_early_exit() -> (super::ReplayOutcome, Vec<bool>, usize) {
        let mesh = fixture(true);
        let (grid, _) = plan_grid(0.0, -0.5, 4.0, 0.5, LAYERS, H, 1.0, u64::MAX);
        let planes: Vec<f64> = (0..LAYERS).map(|l| (l as f64 + 0.5) * H).collect();
        let slices = slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("slices");
        let struts = vec![
            arm(&grid, 0, 0.9, 20),
            arm(&grid, 1, 3.1, 20),
            arm(&grid, 2, 0.9, 20),
            arm(&grid, 3, 0.9, 40),
        ];
        let context = ReplayContext {
            grid: &grid,
            slices: &slices,
            layer_cache: None,
            planes: &planes,
            mesh: &mesh,
            kappa: KAPPA,
            layer_height_mm: H,
            layers: LAYERS,
            straight_reach: true,
            creep_weight: false,
            creep_overhang_cut: true,
            overhang_centre_deg: 45.0,
            overhang_transition_deg: 25.0,
            max_restarts: 3,
            first_loss_ratio: 2.592 / 2.130,
            births: &crate::births::Births::none(),
        };
        let mut doomed = vec![false, false, true, true];
        let outcome = refuse_unrooted_demand(&context, &struts, &mut doomed);
        (outcome, doomed, grid.cells())
    }

    const EARLY_EXIT_CHILD_MODE: &str = "PLINTH_REPLAY_CELL_LAYERS_CHILD";
    const EARLY_EXIT_CHILD_TEST: &str =
        "demand_replay::tests::child_runs_the_production_early_exit";

    /// Not a test of its own: the body the parent below launches in a fresh
    /// process, where the process-global perf counters are its own. Without
    /// the marker variable it returns at once. It switches counting on, makes
    /// the production call and records what the charge must be.
    #[test]
    fn child_runs_the_production_early_exit() {
        let Some(dir) = std::env::var_os("PLINTH_REPLAY_CELL_LAYERS_DIR") else {
            return;
        };
        if std::env::var_os(EARLY_EXIT_CHILD_MODE).is_none() {
            return;
        }
        let dir = std::path::PathBuf::from(dir);
        crate::perf::enable(dir.clone(), crate::perf::DEFAULT_CHECKPOINT);
        crate::perf::begin_model("replay");
        let (outcome, doomed, cells) = production_early_exit();
        crate::perf::end_model();
        assert_eq!(doomed, vec![false; 4], "{outcome:?}");
        assert_eq!(outcome.replays, 1, "{outcome:?}");
        assert_eq!(outcome.refused, 2, "{outcome:?}");
        assert_eq!(outcome.layers_visited, 41, "layers 0..=40 were walked: {outcome:?}");
        std::fs::write(dir.join("expected"), format!("{}", cells as u64 * outcome.layers_visited))
            .expect("write the expected charge");
    }

    /// X0 regression witness for the counter the PRODUCTION path emits. The
    /// child makes a `refuse_unrooted_demand` call with perf on in which the
    /// replay exits early; the parent reads the sidecar and requires
    /// `replay.cell_layers` to be `cells * layers walked`, and different from
    /// the old charge `cells * context.layers * replays`. Sabotage check:
    /// charging `context.layers * outcome.replays` at the `perf::add` in
    /// `refuse_unrooted_demand` fails this test (and only this one).
    #[test]
    fn the_emitted_replay_cell_layers_counter_charges_the_layers_walked() {
        let dir = std::env::temp_dir().join(format!(
            "plinth-replay-cell-layers-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("scratch directory");
        let status = std::process::Command::new(std::env::current_exe().expect("this test binary"))
            .arg("--exact")
            .arg(EARLY_EXIT_CHILD_TEST)
            .arg("--nocapture")
            .env(EARLY_EXIT_CHILD_MODE, "1")
            .env("PLINTH_REPLAY_CELL_LAYERS_DIR", &dir)
            .status()
            .expect("launch the child");
        assert!(status.success(), "child failed: {status}");
        let expected: u64 = std::fs::read_to_string(dir.join("expected"))
            .expect("the child wrote its expectation")
            .parse()
            .expect("a count");
        let text = std::fs::read_to_string(dir.join("replay.perf.json")).expect("final sidecar");
        assert!(text.contains("\"partial\":false"), "{text}");
        let totals = &text[text.find("\"totals\"").expect("totals")..];
        let work = &totals[totals.find("\"work\":{").expect("work object")..];
        let at = work.find("\"replay.cell_layers\":").expect("the counter is emitted");
        let emitted: u64 = work[at + "\"replay.cell_layers\":".len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .expect("a count");
        let (_, _, cells) = production_early_exit();
        let old_charge = cells as u64 * LAYERS as u64;
        assert!(expected > 0 && expected < old_charge, "{expected} vs {old_charge}");
        assert_eq!(emitted, expected, "emitted replay.cell_layers is not cells * layers walked");
        assert_ne!(emitted, old_charge, "the old charge, cells * layers * replays");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// JOINT REVIEW 2026-10-13. A birth's own arm is doomed, and a neighbour
    /// that survives covers ONE ordinary cell of the birth (reach, not
    /// contact), its seat outside the birth and its tip ball unable to hold
    /// it. Replay must restore the birth's arm: before the fix it took it
    /// away (0 refusals, 0 newly unrooted), because every covered cell counted
    /// as a root of the birth.
    ///
    /// Three runs on one real sliced two-box mesh, the real births, raster and
    /// reach: the neighbour covering one cell, covering all but one cell
    /// (partial), and holding the whole birth by footprint, which is the only
    /// kind of coverage that lets the birth's own arm go.
    #[test]
    fn a_neighbours_reach_over_part_of_a_birth_does_not_let_its_own_arm_go() {
        use crate::births::Births;
        let body = make_box([2.03, 0.3, 1.0], [1.015, 0.15, 0.0]).expect("body");
        let island = make_box([0.3, 0.3, 0.5], [2.2, 0.15, 0.5]).expect("island");
        let offset = u32::try_from(body.verts().len()).expect("small");
        let mut verts = body.verts().to_vec();
        verts.extend_from_slice(island.verts());
        let mut tris = body.tris().to_vec();
        tris.extend(island.tris().iter().map(|t| t.map(|v| v + offset)));
        let mesh = Mesh::new(verts, tris).expect("two boxes");
        let grid = crate::grid::lay_out_at(-0.5, -0.5, 0.05, 80, 40);
        let planes: Vec<f64> = (0..20).map(|l| (l as f64 + 0.5) * 0.05).collect();
        let slices = slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("slices");
        let births = Births::build(&grid, &slices, 0.014, 0.019);
        let birth = births.at(10).first().expect("a birth at layer 10").clone();
        assert!(birth.cells.len() > 2, "a birth of several cells: {birth:?}");
        let at = |id: u32, x: f64, y: f64| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "fixture coordinates inside the grid's own bounds"
            )]
            let cell = grid.index(grid.row_coord(y).floor() as usize, grid.col_coord(x).floor() as usize);
            let mut strut = arm(&grid, id, x, 10);
            strut.cell = cell;
            strut.center_mm = [x, y];
            strut
        };
        let context = ReplayContext {
            grid: &grid,
            slices: &slices,
            layer_cache: None,
            planes: &planes,
            mesh: &mesh,
            kappa: KAPPA,
            layer_height_mm: H,
            layers: 20,
            straight_reach: true,
            creep_weight: false,
            creep_overhang_cut: false,
            overhang_centre_deg: 45.0,
            overhang_transition_deg: 10.0,
            max_restarts: 5,
            first_loss_ratio: 1.2,
            births: &births,
        };
        let replay = |covered: Vec<usize>, held: bool| {
            let mut host = at(1, 2.025, 0.125);
            assert!(!birth.cells.contains(&host.cell), "the neighbour's seat is outside the birth");
            host.covered_demand = covered;
            if held {
                host.held_births = vec![(10, birth.id)];
            }
            let mut direct = at(2, 2.225, 0.125);
            direct.covered_demand = birth.cells.clone();
            direct.held_births = vec![(10, birth.id)];
            let mut ground = at(0, 1.025, 0.125);
            ground.k_top = 0;
            let field = vec![ground, host, direct];
            let mut doomed = vec![false, false, true];
            let outcome = refuse_unrooted_demand(&context, &field, &mut doomed);
            (doomed[2], outcome)
        };
        let last = *birth.cells.last().expect("cells");
        let (removed, outcome) = replay(vec![last], false);
        assert!(!removed, "one ordinary covered cell holds nothing: {outcome:?}");
        assert_eq!(outcome.refused, 1, "{outcome:?}");
        assert_eq!(outcome.newly_unrooted_cells, birth.cells.len() as u64, "{outcome:?}");
        let most = birth.cells[..birth.cells.len() - 1].to_vec();
        let (removed, outcome) = replay(most, false);
        assert!(!removed, "all but one covered cell holds nothing either: {outcome:?}");
        let (removed, outcome) = replay(birth.cells.clone(), true);
        assert!(removed, "a whole birth held by footprint lets the birth's arm go: {outcome:?}");
        assert_eq!(outcome.refused, 0, "{outcome:?}");
    }

    /// JOINT REVIEW 2026-10-13, `birth_alias`. Two births of one layer fall in
    /// the same working-raster cell (a 0.4 mm grid, cell 21) while only one
    /// fits the holder's tip ball: A (the small box, 25 pixels) lies inside a
    /// 0.22 mm ball centred at (0.3, 0.2); B (the long box, 100 pixels, 0.44 to
    /// 0.78 mm in X) does not. The holder holds A by footprint. Replay must
    /// still restore B's own doomed arm: before the fix the hold was kept as
    /// cells, so A's hold covered B too and B's arm went (0 refusals), leaving
    /// B's first layer with nothing under it.
    ///
    /// One real sliced three-box mesh, the real births, raster and reach. Two
    /// controls: nothing held (B's arm is restored, one refusal) and B itself
    /// held (B's arm goes).
    #[test]
    fn a_birth_sharing_the_working_cells_of_a_held_birth_is_not_held_with_it() {
        use crate::births::Births;
        let boxes = [
            make_box([0.1, 0.5, 1.0], [0.05, 0.0, 0.0]).expect("wall"),
            make_box([0.015, 0.020, 0.5], [0.1175, 0.05, 0.5]).expect("small"),
            make_box([0.086, 0.020, 0.5], [0.153, 0.085, 0.5]).expect("long"),
        ];
        let (mut verts, mut tris) = (Vec::new(), Vec::new());
        for part in &boxes {
            let offset = u32::try_from(verts.len()).expect("small");
            verts.extend(part.verts().iter().map(|v| [v[0] * 4.0, v[1] * 4.0, v[2]]));
            tris.extend(part.tris().iter().map(|t| t.map(|v| v + offset)));
        }
        let mesh = Mesh::new(verts, tris).expect("three boxes");
        let grid = crate::grid::lay_out_at(0.0, -0.8, 0.4, 10, 10);
        let planes: Vec<f64> = (0..20).map(|l| (l as f64 + 0.5) * 0.05).collect();
        let slices = slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("slices");
        let births = Births::build(&grid, &slices, 0.014, 0.019);
        assert_eq!(births.at(10).len(), 2, "two births at layer 10: {:?}", births.at(10));
        let (a, b) = (&births.at(10)[0], &births.at(10)[1]);
        assert_eq!(a.cells, b.cells, "the births share their working cells");
        assert_eq!(a.cells.len(), 1);
        let ball = ([0.3, 0.2], 0.22);
        assert!(a.inside_disc(ball.0, ball.1), "A fits the holder's ball: {a:?}");
        assert!(!b.inside_disc(ball.0, ball.1), "B does not: {b:?}");
        assert_ne!(a.id, b.id);
        let at = |id: u32, x: f64, y: f64, k_top: usize| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "fixture coordinates inside the grid's own bounds"
            )]
            let cell = grid.index(grid.row_coord(y).floor() as usize, grid.col_coord(x).floor() as usize);
            let mut strut = arm(&grid, id, x, k_top);
            strut.cell = cell;
            strut.center_mm = [x, y];
            strut
        };
        let context = ReplayContext {
            grid: &grid,
            slices: &slices,
            layer_cache: None,
            planes: &planes,
            mesh: &mesh,
            kappa: 42.6,
            layer_height_mm: H,
            layers: 20,
            straight_reach: true,
            creep_weight: false,
            creep_overhang_cut: false,
            overhang_centre_deg: 45.0,
            overhang_transition_deg: 10.0,
            max_restarts: 5,
            first_loss_ratio: 1.2,
            births: &births,
        };
        let replay = |held: &[usize]| {
            let mut host = at(1, 0.3, 0.2, 10);
            assert!(!a.cells.contains(&host.cell), "the holder's seat is outside the births' cell");
            host.covered_demand = a.cells.clone();
            host.held_births = held.iter().map(|&id| (10, id)).collect();
            host.radius_mm = vec![0.22; 11];
            let own = at(2, 0.742, 0.34, 10);
            let ground = at(0, 0.2, 0.2, 0);
            let mut doomed = vec![false, false, true];
            let outcome = refuse_unrooted_demand(&context, &[ground, host, own], &mut doomed);
            (doomed[2], outcome)
        };
        let (removed, outcome) = replay(&[]);
        assert!(!removed, "CONTROL: nothing held, B's arm stays: {outcome:?}");
        assert_eq!(outcome.refused, 1, "{outcome:?}");
        let (removed, outcome) = replay(&[a.id]);
        assert!(!removed, "A held by footprint does not hold B in the same cells: {outcome:?}");
        assert_eq!(outcome.refused, 1, "{outcome:?}");
        let (removed, outcome) = replay(&[a.id, b.id]);
        assert!(removed, "CONTROL: B held itself lets B's arm go: {outcome:?}");
        assert_eq!(outcome.refused, 0, "{outcome:?}");
    }

    /// The joint counterfactual. Doom the arm at 0.9 AND the middle one: the
    /// replay finds the near end unrooted, restores the NEAREST doomed arm
    /// (0.9, not 2.0), and the middle one still goes.
    #[test]
    fn a_joint_removal_restores_the_nearest_arm_and_lets_the_other_go() {
        let mesh = fixture(false);
        let place =
            |g: &Grid| vec![arm(g, 0, 0.9, 20), arm(g, 1, 2.0, 20), arm(g, 2, 3.1, 20)];
        let mut doomed = vec![true, true, false];
        let outcome = run(&mesh, &place, &mut doomed);
        assert_eq!(doomed, vec![false, true, false], "{outcome:?}");
        assert_eq!(outcome.refused, 1);
    }

    // ---- lane RP: the call's reuse is held to the reference ----------------

    use super::{refuse_counted, reference_refuse_unrooted_demand, same_answer};
    use crate::births::Births;
    use crate::layer_cache::BytePool;

    /// Three boxes stacked as a staircase, each overlapping the one below by
    /// 0.05 mm and reaching further out than it, so every step is an overhang
    /// wider than one lamina's reach (2.13 mm): its far end is rooted only by
    /// an arm seated on the step's own layers or above the first layer.
    fn staircase() -> Mesh {
        let parts = [
            make_box([2.0, 1.0, 0.55], [1.0, 0.0, 1.0]).expect("lower"),
            make_box([4.6, 1.0, 0.55], [2.3, 0.0, 1.5]).expect("middle"),
            make_box([7.0, 1.0, 0.5], [3.5, 0.0, 2.0]).expect("upper"),
        ];
        let (mut verts, mut tris) = (Vec::new(), Vec::new());
        for part in &parts {
            let offset = u32::try_from(verts.len()).expect("small");
            verts.extend_from_slice(part.verts());
            tris.extend(part.tris().iter().map(|t| t.map(|v| v + offset)));
        }
        Mesh::new(verts, tris).expect("a stacked staircase")
    }

    /// A small deterministic generator (an LCG), so a failing scenario is
    /// reproducible from its number.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) % n
        }
    }

    /// What a sweep over many scenarios saw.
    #[derive(Default, Debug)]
    struct Sweep {
        scenarios: usize,
        mismatches: usize,
        restarts: u64,
        baseline_reused: u64,
        accounting_breaks: usize,
        layers_saved: u64,
        /// The first scenario whose answer differed, for pinning.
        example: String,
    }

    /// Run `scenarios` random fields and doomed sets through the cached call and
    /// the reference, under each setting, and count the answers that differ.
    fn sweep(scenarios: usize, seed: u64, pool: &'static BytePool, stop_at_first: bool) -> Sweep {
        let mesh = staircase();
        let (grid, _) = plan_grid(0.0, -0.5, 7.0, 0.5, LAYERS, H, 1.0, u64::MAX);
        let planes: Vec<f64> = (0..LAYERS).map(|l| (l as f64 + 0.5) * H).collect();
        let slices = slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("slices");
        let births = Births::none();
        let mut rng = Rng(seed);
        let mut tally = Sweep::default();
        let xs = [0.4, 1.2, 2.2, 3.0, 3.8, 4.6, 5.4, 6.2];
        let layers = [20, 22, 24, 26, 29, 30, 31, 33, 36, 39, 40, 42, 44, 46, 48];
        for _ in 0..scenarios {
            if stop_at_first && tally.mismatches > 0 {
                break;
            }
            let count = 5 + rng.below(8) as usize;
            let mut struts = vec![arm(&grid, 0, 0.4, 20)];
            for id in 1..count {
                let x = xs[rng.below(xs.len() as u64) as usize];
                let layer = layers[rng.below(layers.len() as u64) as usize];
                struts.push(arm(&grid, id as u32, x, layer));
            }
            let mut doomed: Vec<bool> = (0..count).map(|i| i > 0 && rng.below(2) == 0).collect();
            if !doomed.iter().any(|flag| *flag) {
                doomed[count - 1] = true;
            }
            for (creep, straight, restarts) in [(false, true, 4), (true, false, 1), (true, true, 6)] {
                let context = ReplayContext {
                    grid: &grid,
                    slices: &slices,
                    layer_cache: None,
                    planes: &planes,
                    mesh: &mesh,
                    kappa: KAPPA,
                    layer_height_mm: H,
                    layers: LAYERS,
                    straight_reach: straight,
                    creep_weight: creep,
                    creep_overhang_cut: true,
                    overhang_centre_deg: 45.0,
                    overhang_transition_deg: 25.0,
                    max_restarts: restarts,
                    first_loss_ratio: 2.592 / 2.130,
                    births: &births,
                };
                let mut expected_doomed = doomed.clone();
                let (expected, expected_blame) =
                    reference_refuse_unrooted_demand(&context, &struts, &mut expected_doomed);
                let mut got_doomed = doomed.clone();
                let (outcome, work, blame) = refuse_counted(&context, &struts, &mut got_doomed, pool);
                tally.scenarios += 1;
                if !same_answer(&outcome, &got_doomed, &blame, &expected, &expected_doomed, &expected_blame) {
                    tally.mismatches += 1;
                    if tally.example.is_empty() {
                        let arms: Vec<(f64, usize)> = struts
                            .iter()
                            .map(|s| (s.center_mm[0], s.k_top))
                            .collect();
                        tally.example = format!(
                            "arms {arms:?} doomed {doomed:?} setting {:?}",
                            (creep, straight, restarts)
                        );
                    }
                }
                tally.restarts += outcome.replays.saturating_sub(1);
                tally.baseline_reused += work.baseline_reused;
                if work.baseline_computed + work.baseline_reused != outcome.layers_visited
                    || work.trial_computed != outcome.trial_layers
                {
                    tally.accounting_breaks += 1;
                }
                tally.layers_saved += work.baseline_reused;
            }
        }
        tally
    }

    /// Sets the fault injection for one scope and puts production back.
    struct Sabotaged;

    impl Sabotaged {
        fn with(kind: u8) -> Self {
            super::SABOTAGE.with(|cell| cell.set(kind));
            Self
        }
    }

    impl Drop for Sabotaged {
        fn drop(&mut self) {
            super::SABOTAGE.with(|cell| cell.set(0));
        }
    }

    fn pool_of(limit: u64) -> &'static BytePool {
        Box::leak(Box::new(BytePool::new(limit)))
    }

    /// THE ORACLE. On random fields over a staircase with overhangs wider than
    /// one lamina's reach, with random doomed sets and three settings (creep
    /// on or off, chamfer or straight reach, one to six restarts), the call
    /// that reuses its baseline gives, by value, the reference's answer: the
    /// doomed flags, the outcome (refusals, newly unrooted cells and their worst
    /// potential, the residual, the layers visited, the trial layers,
    /// exhaustion) and the blame potentials by bits. The same sweep must have
    /// exercised the reuse: restarts, and baseline layers skipped.
    #[test]
    fn a_cached_call_gives_the_reference_answer() {
        let tally = sweep(100, 7, pool_of(u64::MAX / 2), false);
        assert_eq!(tally.mismatches, 0, "{tally:?}");
        assert!(tally.restarts > 0, "no scenario restarted: {tally:?}");
        assert!(tally.baseline_reused > 0, "the baseline was never reused: {tally:?}");
    }

    /// A pool that refuses (nothing, a few layers' bytes, all of it) changes no
    /// answer: the cache switches itself off and the call is a fresh one.
    #[test]
    fn a_cache_the_pool_refuses_changes_no_answer() {
        for limit in [0, 600, 40_000, 400_000, u64::MAX / 2] {
            let tally = sweep(30, 11, pool_of(limit), false);
            assert_eq!(tally.mismatches, 0, "limit {limit}: {tally:?}");
            if limit == 0 {
                assert_eq!(tally.baseline_reused, 0, "{tally:?}");
            }
        }
    }

    /// Runs `run` with the two walks forced to overlap (`true`) or to run
    /// serially (`false`) when called from this thread.
    fn with_overlap<T>(overlap: bool, run: impl FnOnce() -> T) -> T {
        super::FORCE_OVERLAP.with(|cell| cell.set(Some(overlap)));
        let out = run();
        super::FORCE_OVERLAP.with(|cell| cell.set(None));
        out
    }

    /// The same sweeps with the two walks overlapping, at every pool budget:
    /// the answer, the blame potentials by bits and the work counters are the
    /// reference's, and a restart still reuses the cache.
    #[test]
    fn overlapped_walks_give_the_reference_answer() {
        with_overlap(true, || {
            let tally = sweep(60, 7, pool_of(u64::MAX / 2), false);
            assert_eq!(tally.mismatches, 0, "{tally:?}");
            assert_eq!(tally.accounting_breaks, 0, "{tally:?}");
            assert!(tally.restarts > 0, "no scenario restarted: {tally:?}");
            assert!(tally.baseline_reused > 0, "the baseline was never reused: {tally:?}");
            for limit in [0, 600, 40_000, 400_000] {
                let tally = sweep(15, 11, pool_of(limit), false);
                assert_eq!(tally.mismatches, 0, "limit {limit}: {tally:?}");
                assert_eq!(tally.accounting_breaks, 0, "limit {limit}: {tally:?}");
            }
        });
    }

    /// One field on the staircase, one doomed set, both engines: the cached
    /// call's answer and work, and whether it equals the reference's.
    fn staircase_case(arms: &[(f64, usize)], doomed: &[bool]) -> (super::ReplayOutcome, super::ReplayWork, Vec<bool>, bool) {
        let mesh = staircase();
        let (grid, _) = plan_grid(0.0, -0.5, 7.0, 0.5, LAYERS, H, 1.0, u64::MAX);
        let planes: Vec<f64> = (0..LAYERS).map(|l| (l as f64 + 0.5) * H).collect();
        let slices = slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("slices");
        let births = Births::none();
        let struts: Vec<SweepStrut> = arms
            .iter()
            .enumerate()
            .map(|(id, &(x, layer))| arm(&grid, id as u32, x, layer))
            .collect();
        let context = ReplayContext {
            grid: &grid,
            slices: &slices,
            layer_cache: None,
            planes: &planes,
            mesh: &mesh,
            kappa: KAPPA,
            layer_height_mm: H,
            layers: LAYERS,
            straight_reach: true,
            creep_weight: false,
            creep_overhang_cut: true,
            overhang_centre_deg: 45.0,
            overhang_transition_deg: 25.0,
            max_restarts: 4,
            first_loss_ratio: 2.592 / 2.130,
            births: &births,
        };
        let mut expected_doomed = doomed.to_vec();
        let (expected, expected_blame) =
            reference_refuse_unrooted_demand(&context, &struts, &mut expected_doomed);
        let mut got_doomed = doomed.to_vec();
        let (outcome, work, blame) =
            refuse_counted(&context, &struts, &mut got_doomed, pool_of(u64::MAX / 2));
        let same = same_answer(&outcome, &got_doomed, &blame, &expected, &expected_doomed, &expected_blame);
        (outcome, work, got_doomed, same)
    }

    /// A restart that restores an arm seated far below the layer that found
    /// the cells. `A` (x 4.0, layer 31) lowers the carried potential at the
    /// middle step's edge, so the upper step's overhang at layer 40 is rooted
    /// out to x 6.1 with it and 5.1 without it; layer 40 finds the cells
    /// between unrooted and blames `A`, nine layers below. Every fresh cell
    /// restores one doomed arm, so `Q` (which `H` makes redundant) is restored
    /// too and the call ends after one replay: the cached call and the
    /// reference agree on that, by value.
    #[test]
    fn a_far_restoration_is_answered_as_the_reference_answers_it() {
        let arms = [(0.4, 20), (1.2, 20), (3.0, 30), (4.0, 31)];
        let (outcome, _, doomed, same) = staircase_case(&arms, &[false, true, false, true]);
        assert!(same, "cached and reference differ: {outcome:?} {doomed:?}");
        assert_eq!(outcome.refused, 2, "{outcome:?}");
        assert_eq!(doomed, vec![false; 4], "{outcome:?}");
    }

    /// SABOTAGE. The fault, injected in the cached path, must turn the oracle
    /// sweep red that is green without it: a stale baseline that claims every
    /// cell rooted (1).
    #[test]
    fn the_oracle_goes_red_under_sabotage() {
        let clean = sweep(60, 99, pool_of(u64::MAX / 2), false);
        assert_eq!(clean.mismatches, 0, "{clean:?}");
        let _fault = Sabotaged::with(1);
        let broken = sweep(300, 99, pool_of(u64::MAX / 2), true);
        assert!(broken.mismatches > 0, "the stale baseline was not caught: {broken:?}");
    }

    /// The work counters account for every logical layer exactly once: each
    /// layer the reference would walk was either computed or reused (baseline),
    /// and each trial layer computed; and a call with restarts computed fewer
    /// layers than the fresh replays it equals.
    #[test]
    fn the_work_counters_account_for_every_layer() {
        let tally = sweep(60, 7, pool_of(u64::MAX / 2), false);
        assert_eq!(tally.accounting_breaks, 0, "{tally:?}");
        assert!(tally.layers_saved > 0, "a restart saved nothing: {tally:?}");
    }

    // ---- lane tc: the cover a seat slide leaves ----------------------------

    use super::{stranded_by_moves, MovedSeat, StrandedCell};

    /// `S1` with arms at x 0.9 and 3.1, the second slid to `to_x`: the cells
    /// the slid field leaves unrooted that the placed field rooted.
    fn slid(to_x: f64) -> (Vec<StrandedCell>, Grid) {
        let mesh = fixture(false);
        let (grid, _) = plan_grid(0.0, -0.5, 4.0, 0.5, LAYERS, H, 1.0, u64::MAX);
        let planes: Vec<f64> = (0..LAYERS).map(|l| (l as f64 + 0.5) * H).collect();
        let slices = slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("slices");
        let struts = vec![arm(&grid, 0, 0.9, 20), arm(&grid, 1, 3.1, 20)];
        let context = ReplayContext {
            grid: &grid,
            slices: &slices,
            layer_cache: None,
            planes: &planes,
            mesh: &mesh,
            kappa: KAPPA,
            layer_height_mm: H,
            layers: LAYERS,
            straight_reach: true,
            creep_weight: false,
            creep_overhang_cut: true,
            overhang_centre_deg: 45.0,
            overhang_transition_deg: 25.0,
            max_restarts: 3,
            first_loss_ratio: 2.592 / 2.130,
            births: &crate::births::Births::none(),
        };
        let moved =
            [MovedSeat {
                index: 1,
                xy_mm: [to_x, 0.0],
                contact_radius_mm: 0.1,
                placed_ball: None,
                ball: None,
            }];
        (stranded_by_moves(&context, &struts, &moved), grid)
    }

    /// The arm at 3.1 slid to 1.5: the far end of the slab, x 4.0, is then
    /// 2.5 mm from it (and 3.1 mm from the arm at 0.9), past the 2.13 mm
    /// reach, so the cells past x 1.5 + 2.13 = 3.63 on the slab's first layer
    /// are stranded, blamed on the slid arm, and every one of them inherits
    /// upward through the slab. Hand-derived: no cell left of 3.63 is
    /// stranded, less one cell plus one bend: the any-angle field is an upper
    /// bound on the straight length, up to a step long where a path must bend
    /// round the cover's edge (`reach`'s module doc), and the slab's cover is
    /// rounded out one cell.
    #[test]
    fn a_slide_that_moves_a_root_away_from_its_far_cells_strands_them() {
        let (stranded, grid) = slid(1.5);
        let first: Vec<&StrandedCell> = stranded.iter().filter(|cell| cell.layer == 20).collect();
        assert!(!first.is_empty(), "the far end was stranded: {stranded:?}");
        assert!(stranded.iter().all(|cell| cell.blamed == 1), "blamed on the slid arm");
        assert!(first.iter().all(|cell| cell.surface), "the first layer is surface");
        assert!(
            stranded.iter().filter(|cell| cell.layer > 20).all(|cell| !cell.surface),
            "the layers above only inherit it"
        );
        let slack = 2.0 * grid.pitch_mm();
        for cell in &stranded {
            let x = grid.center_x_mm(grid.col_of(cell.cell));
            assert!(x > 3.63 - slack, "a cell at x {x} is within reach of the slid root");
        }
        assert!(
            first.iter().any(|cell| grid.center_x_mm(grid.col_of(cell.cell)) > 3.9),
            "the far end itself is stranded"
        );
    }

    /// The same arm slid only to 3.0: every cell of the slab is still within
    /// 2.13 mm of a root, so nothing is stranded.
    #[test]
    fn a_slide_that_keeps_every_cell_in_reach_strands_nothing() {
        let (stranded, _) = slid(3.0);
        assert!(stranded.is_empty(), "{stranded:?}");
    }

    /// The root moves by as much as the CONTACT moves: an arm whose seat
    /// column is said to have slid to x 1.5 but whose ball holds the same
    /// cells of the seat layer before and after (a ball at x 3.1, sunk 0.1 mm
    /// into the slab's underside, radius 0.3) keeps its placed root, and
    /// nothing is stranded. SABOTAGE: without the balls the root follows the
    /// column and the far end is stranded (the first test above).
    #[test]
    fn a_tip_whose_contact_stays_put_keeps_its_root() {
        let mesh = fixture(false);
        let (grid, _) = plan_grid(0.0, -0.5, 4.0, 0.5, LAYERS, H, 1.0, u64::MAX);
        let planes: Vec<f64> = (0..LAYERS).map(|l| (l as f64 + 0.5) * H).collect();
        let slices = slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("slices");
        let struts = vec![arm(&grid, 0, 0.9, 20), arm(&grid, 1, 3.1, 20)];
        let context = ReplayContext {
            grid: &grid,
            slices: &slices,
            layer_cache: None,
            planes: &planes,
            mesh: &mesh,
            kappa: KAPPA,
            layer_height_mm: H,
            layers: LAYERS,
            straight_reach: true,
            creep_weight: false,
            creep_overhang_cut: true,
            overhang_centre_deg: 45.0,
            overhang_transition_deg: 25.0,
            max_restarts: 3,
            first_loss_ratio: 2.592 / 2.130,
            births: &crate::births::Births::none(),
        };
        let ball = Some(([3.1, 0.0, 1.1], 0.3));
        let moved = [MovedSeat {
            index: 1,
            xy_mm: [1.5, 0.0],
            contact_radius_mm: 0.1,
            placed_ball: ball,
            ball,
        }];
        let stranded = stranded_by_moves(&context, &struts, &moved);
        assert!(stranded.is_empty(), "{stranded:?}");
    }
}
