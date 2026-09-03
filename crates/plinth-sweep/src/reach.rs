//! Reach, rootedness, and cured thickness.
//!
//! # The quantity
//!
//! A cell is **rooted** when a dimensionless potential `U` reaches it at or
//! below 1. `U` measures one thing only: the LATERAL run of cured material,
//! accumulated across every layer since the last true root, priced at
//! `1 / (κ · τ)` with `κ` the registered [`LAMINA_REACH_RATIO`] and `τ` the
//! cured thickness of the material being entered. On a fresh lamina `τ = h`, so
//! `U ≤ 1` is "no further than `κ·h = 2.13 mm` of sideways travel over
//! nothing", which is the measured covering radius. Vertical travel is FREE:
//! material laid on cured material spans nothing at all, so there is no length
//! to charge for it.
//!
//! # `τ` is spent, and that is the open ruling in this module
//!
//! §5.2 prices a step at `1/(κ·τ)` and §C4 rules that safe because the scaling
//! "never sets a number in this document's acceptance set". **Measured, it sets
//! several.** It prices travel through the body of a 3.5 mm disc at a
//! seventieth of a lamina's cost, so a whole sloped underside is reached from a
//! handful of arms at its low edge — Round 100 tilted 10° takes ten arms and
//! 829 overload warnings — and the orientation objective, which scores a pose
//! by the demand this sweep measures, then prefers the tilt.
//!
//! The alternative was built and measured too, and it fails §C4's own rim-ramp
//! check: with `τ` reported and never spent, Round 50's drafted rim asks for 23
//! arms it does not need. [`weight`] carries the full three-way comparison. The
//! design's form ships because it is the only one that keeps the acceptance set
//! green; that it decides a pose by extrapolating `κ` from 50 µm to 3.5 mm is
//! an owner ruling, reported rather than settled here.
//!
//! # What seeds the field, and what sabotage S2 breaks
//!
//! One thing carries `U = 0`: a strut's centre column. A cell whose own column
//! CARRIED one layer down inherits that column's potential unchanged — it is
//! not re-seeded. **Rootedness is not a per-layer property**, and that is the
//! whole point (§5.1): a ramp overlaps the layer below at every single layer,
//! so no per-layer test can ever see it, and only an accumulating `U` unroots
//! its far end. Sabotage S2 re-seeds `U := 0` over any cured cell, which roots
//! a flat disc entirely, takes it to zero arms, and must turn the disc oracle
//! red.
//!
//! # Why the down-sweep can route load along every path this accepts
//!
//! `U[c] ≤ 1` says the lateral run since the last true root is at most `κ·h`.
//! The run WITHIN one layer, back to a cell whose column carried one layer
//! down, is a sub-portion of that, so it is at most `κ·h` too — and that is
//! exactly the question the down-sweep asks of every load cell, on the same
//! 8-neighbour chamfer metric over the same cured cover
//! ([`crate::down_sweep`]). So `residual_demand == 0` implies
//! `uncarried_load == 0` and
//! [`crate::report::SweepRunReport::sweeps_disagree`] reports any survival of
//! the contradiction rather than leaving a reader to compare two counters.
//!
//! [`LAMINA_REACH_RATIO`]: plinth_core::M0EvidenceConstants::lamina_reach_ratio

#![expect(
    clippy::cast_possible_truncation,
    reason = "the sweep keeps its per-cell fields in f32 to hold a 100 mm model at 0.05 mm in a \
          working set that fits cache; every f64 -> f32 cast here is a mm-scale length or a layer \
          count, whose magnitudes are 1e-2..1e3 and whose 24-bit mantissa resolves 1e-5 mm -- four \
          orders below the raster pitch this code discretizes to"
)]
#![expect(
    clippy::needless_range_loop,
    reason = "batch-first: these loops walk SEVERAL parallel per-cell rasters at one index, which is \
          the shape this crate exists to have. Zipping iterators over three or four arrays reads \
          worse and, where the loop also writes back into one of them, does not compile"
)]

use crate::grid::Grid;

/// Per-layer reach state. One layer is live at a time; `u_prev` and `tau_run`
/// carry across layers, which is what makes the potential accumulate.
#[derive(Clone, Debug)]
pub struct ReachState {
    /// Reach potential at the current layer; `f32::INFINITY` where unreachable.
    pub u: Vec<f32>,
    /// The previous layer's potential, for vertical inheritance.
    pub u_prev: Vec<f32>,
    /// Cured run length in LAYERS, per cell, ending at the current layer.
    pub tau_run: Vec<u32>,
    /// In-plane chamfer distance from each carrier cell to the nearest
    /// non-carrier cell, in mm. **Computed and reported, not applied** — see
    /// [`advance_thickness`].
    pub in_plane_mm: Vec<f32>,
    /// Cured thickness actually used, in mm.
    pub tau_used_mm: Vec<f32>,
    /// The previous layer's cured thickness, reported only.
    pub tau_prev_mm: Vec<f32>,
    /// One cured lamina in mm — the minimum any step cost may be priced at,
    /// and the whole cost wherever `tau_used_mm` has not been advanced.
    pub lamina_mm: f32,
    /// Highest `tau_run · h / tau_used` ratio seen; above 2 is warned.
    pub tau_vertical_over_used_max: f64,
    /// Largest `tau_used` at which a rootedness decision actually bound.
    pub tau_at_binding_reach_max_mm: f64,
    /// The largest potential any cell ever inherited from the layer below.
    ///
    /// The climb itself is free (see [`recompute_potential`]), so this is a
    /// pure carry-over rather than an accumulation: it says how close material
    /// that was standing on something came to unrooting anyway, which is
    /// always because of LATERAL run somewhere below it.
    pub u_from_vertical_max: f64,
    /// Longest continuous cured run seen, in layers. Reported, never spent.
    pub tau_run_max: u32,
    /// Times a third chamfer pass changed something — a warning, never a
    /// refusal.
    pub chamfer_not_converged: u64,
    /// Path-hit counters. A green oracle must never be able to mean "the path
    /// never ran".
    pub reach_tests: u64,
    pub vertical_inheritance_hits: u64,
}

impl ReachState {
    #[must_use]
    pub fn new(grid: &Grid) -> Self {
        let cells = grid.cells();
        Self {
            u: vec![f32::INFINITY; cells],
            u_prev: vec![f32::INFINITY; cells],
            tau_run: vec![0; cells],
            in_plane_mm: vec![0.0; cells],
            tau_used_mm: vec![0.0; cells],
            tau_prev_mm: vec![0.0; cells],
            lamina_mm: 0.0,
            tau_vertical_over_used_max: 1.0,
            tau_at_binding_reach_max_mm: 0.0,
            u_from_vertical_max: 0.0,
            tau_run_max: 0,
            chamfer_not_converged: 0,
            reach_tests: 0,
            vertical_inheritance_hits: 0,
        }
    }

    #[must_use]
    pub fn is_rooted(&self, cell: usize) -> bool {
        self.u[cell] <= 1.0
    }
}

/// Advance the cured-thickness fields into layer `k`.
///
/// `tau_used = vertical run × h`.
///
/// # Deviation from the design of record's D5, stated rather than hidden
///
/// D5 specifies `τ = min(vertical run, 2 × in-plane distance transform of
/// Mcarry)`, to remove the `1/cos θ` over-statement the vertical run carries on
/// a wall inclined `θ` from vertical. **That form is not applied here, because
/// as written it is wrong at every boundary cell of every solid.** The in-plane
/// distance transform measures distance to the SURFACE, not section thickness:
/// the rim cell of a plain 50 mm disc reads one cell, so the cap would put
/// `τ = 0.1 mm` on the rim of a solid disc, charge `70 / 42.6 = 1.64` for the
/// rim's own climb, and demand rim supports — which is exactly the outcome the
/// design's §C4 rim-ramp check rejects, and the reason it rejects the naive
/// `τ ≡ h` form in the first place.
///
/// The quantity D5 wants is a **section thickness** — twice the radius of the
/// largest inscribed disc CONTAINING the cell, an opening rather than a
/// distance transform — and computing it needs machinery this lane does not
/// have. So v1 uses the vertical run, **computes the in-plane distance
/// transform anyway**, and reports the ratio the cap would have imposed as
/// `tau_vertical_over_tau_used_max`, so a model where the over-statement
/// actually binds is visible rather than silent. The residual error is bounded
/// by `1/cos θ` and is in the unsafe direction; closing it with the opening
/// form is a v1.1 item.
#[expect(
    clippy::cast_precision_loss,
    reason = "a cured run length in layers, bounded by the layer count at ~1e4, is exact in f32"
)]
pub fn advance_thickness(
    grid: &Grid,
    cover: &[bool],
    carry: &[bool],
    layer_height_mm: f64,
    state: &mut ReachState,
) {
    let cells = grid.cells();
    state.lamina_mm = layer_height_mm as f32;
    state.tau_prev_mm.copy_from_slice(&state.tau_used_mm);
    // Keyed on `carry`, not `cover`, and that choice is load-bearing rather
    // than cosmetic. `τ_run ≥ 2` is the ONLY way a cell may spend more than one
    // lamina of reach, and counting on the carrying mask makes `τ_run ≥ 2`
    // equivalent to "this column carried one layer down" — which is exactly the
    // cell the down-sweep will route load into. Counting on `cover` would grant
    // the extra reach to a cell standing on an eroded hairline span that the
    // down-sweep does not accept as a carrier, and the two sweeps would part
    // company again on a rim one cell wide.
    for cell in 0..cells {
        state.tau_run[cell] = if carry[cell] {
            state.tau_run[cell].saturating_add(1)
        } else {
            0
        };
        state.tau_run_max = state.tau_run_max.max(state.tau_run[cell]);
    }
    chamfer_distance(grid, carry, &mut state.in_plane_mm);
    let pitch = grid.pitch_mm() as f32;
    let height = layer_height_mm as f32;
    for cell in 0..cells {
        if !cover[cell] {
            state.tau_used_mm[cell] = 0.0;
            continue;
        }
        let vertical = state.tau_run[cell] as f32 * height;
        state.tau_used_mm[cell] = vertical.max(height);
        // Reported, not applied. This is what D5's cap WOULD have said.
        let in_plane = (2.0 * (state.in_plane_mm[cell] - 0.5 * pitch)).max(height);
        if in_plane > 0.0 {
            let ratio = f64::from(vertical / in_plane);
            if ratio > state.tau_vertical_over_used_max {
                state.tau_vertical_over_used_max = ratio;
            }
        }
    }
}

/// Recompute `U` over the current layer.
///
/// `roots` names cells whose potential is zero — strut centre columns at every
/// layer they occupy, which is the only true root; **model material is never a
/// root by itself.**
pub fn recompute_potential(
    grid: &Grid,
    cover: &[bool],
    kappa: f64,
    layer_height_mm: f64,
    roots: &[usize],
    state: &mut ReachState,
) {
    let cells = grid.cells();
    // The one lamina every step cost is denominated in. Set here as well as in
    // `advance_thickness` so a caller that only recomputes the potential (the
    // unit tests, and `relax_from_root` through them) cannot read a zero.
    state.lamina_mm = layer_height_mm as f32;
    // Step 1 — vertical inheritance, carried FORWARD and not charged:
    // `U[c] = U_prev[c]`.
    //
    // # Why the climb is free, where §5.2 charges it
    //
    // §5.2 integrates `1/(κ·τ)` along a path through 3-space, so a layer of
    // vertical travel costs `h/(κ·τ)` like any other step. But `κ` measures a
    // SPAN: how far cured material may reach sideways over nothing before the
    // peel tears it off. Material laid directly on cured material below spans
    // nothing at all — it is stacking, not cantilevering — so there is no
    // length to charge, and charging one is a units error rather than a
    // conservative choice.
    //
    // §C4 reached the same OUTCOME by a different route: it keeps the charge
    // and relies on `τ = k·h` growing to make the sum `H_K/κ ≤ 0.194`
    // negligible. That works only while `τ` is spent, and spending `τ` is what
    // [`weight`] now refuses to do. Making the climb structurally free instead
    // of numerically small is the stronger form of §C4's own requirement: a
    // solid block cannot unroot from the inside at ANY height, rather than at
    // heights below the one where the harmonic sum reaches 1.
    //
    // What still accumulates is LATERAL run, and that is the whole point: a
    // ramp overlaps the layer below at every layer, so no per-layer test can
    // ever see it (that is sabotage S2, and the defect §5.1 says this design
    // exists to kill), while its lateral run adds up and unroots its far end
    // after `κ·h` of it.
    //
    // # Two variants tried before this one, recorded so neither is tried again
    //
    // * **Rescaling the inherited value** by `τ_prev/τ_now` DECAYS `U` every
    //   layer, so a ramp's potential converges to a fixed point near 0.2 and
    //   never reaches 1. That is the form LANE 3's Round 100 ran: a 100 mm disc
    //   tilted 10° declared entirely rooted by 10 arms while the down-sweep
    //   lost 595.7 mm² of its load.
    // * **Re-seeding `U := 0`** over every cell that stands on rooted material
    //   makes rootedness a per-layer property, which is sabotage S2's mutation
    //   in all but name. Measured on `oracle_ramp`: a 20° underside took **one
    //   arm** for the whole disc, because a 0.137 mm-per-layer creep is
    //   re-rooted every layer and never accumulates. `residual_demand == 0`
    //   beside 472.45 mm² of uncarried load was that variant printing the same
    //   contradiction from the other side.
    //
    // # The property the two sweeps need
    //
    // The down-sweep must be able to route load down every path this sweep
    // calls rooted, or `residual_demand == 0` and `uncarried_load > 0` are
    // printed about one field and one of them is a lie. With the climb free and
    // every lateral step priced at `1/(κ·h)`, `U[c] ≤ 1` says: the LATERAL run
    // accumulated since the last true root is at most `κ·h`. The run WITHIN
    // this one layer, back to a cell whose column carried one layer down, is a
    // sub-portion of that run, so it is also at most `κ·h`. That is exactly the
    // question the down-sweep asks of every load cell, on the same 8-neighbour
    // chamfer metric over the same cured cover
    // (`down_sweep::route_within_cover`), so `residual_demand == 0` IMPLIES
    // `uncarried_load == 0`. `SweepRunReport::sweeps_disagree` reports any
    // survival of the contradiction, and `oracle_ramp` is the sloped fixture
    // that exercises it — the flat oracles cannot, because on them a demand
    // cell's lateral run is zero.
    for cell in 0..cells {
        if !cover[cell] {
            state.u[cell] = f32::INFINITY;
            continue;
        }
        // `u_prev` is `+∞` wherever layer k−1 did not CARRY (see
        // [`commit_layer`]), so this is "carried below AND that material was
        // itself rooted".
        if state.u_prev[cell] <= 1.0 {
            let below = state.u_prev[cell];
            state.u[cell] = below;
            state.vertical_inheritance_hits += 1;
            // The largest potential any cell carried up from the layer below —
            // how close inherited material came to unrooting. The climb adds
            // nothing to it, so this is a pure carry-over and is published as
            // one.
            if f64::from(below) > state.u_from_vertical_max {
                state.u_from_vertical_max = f64::from(below);
            }
        } else {
            state.u[cell] = f32::INFINITY;
        }
    }
    for &root in roots {
        state.u[root] = 0.0;
    }

    // Step 2 — chamfer sweeps until the field stops moving.
    //
    // # Deviation from the design of record's §5.2, stated rather than hidden
    //
    // §5.2 runs exactly two sweeps and calls a third a convergence CHECK, on
    // the argument that "`U` is monotone in the sweep direction, so two chamfer
    // sweeps give the exact 8-neighbour geodesic". **That is true of a constant
    // step cost and false of this one.** The step cost here is `1/(κ·τ)` and
    // `τ` varies per cell by three orders of magnitude — one lamina in a fresh
    // slab against 20 mm in a leg that has been curing for 400 layers — which
    // makes this a variable-cost (Eikonal) field, and a two-pass raster scan is
    // not exact on one. Measured on `make_overhang_table`, the fixed two-pass
    // form left the slab's fringe around each leg unrooted and put arms inside
    // it, which is the reach test failing to see a root it had already proved.
    //
    // Sweeping to a fixed point costs one extra pass pair on the layers that
    // need it and none on the layers that do not. The bound exists so a
    // pathological field cannot spin; exceeding it is REPORTED and leaves the
    // field conservative (an unconverged `U` is too HIGH, so the sweep
    // over-supports rather than under-supports).
    const MAX_SWEEP_ROUNDS: usize = 8;
    let mut before = state.u.clone();
    let mut converged = false;
    for _ in 0..MAX_SWEEP_ROUNDS {
        chamfer_forward(grid, cover, kappa, state);
        chamfer_reverse(grid, cover, kappa, state);
        if state.u == before {
            converged = true;
            break;
        }
        before.copy_from_slice(&state.u);
    }
    if !converged {
        state.chamfer_not_converged += 1;
    }
    // Step 3 is the caller's `is_rooted`.
    state.reach_tests += 1;
}

/// Cost per millimetre of LATERAL travel through cured material: `1/(κ·h)`,
/// one lamina, everywhere.
///
/// # Why `τ` is reported and never spent — a decision taken against §5.2
///
/// §5.2 prices a step at `1/(κ·τ)` with `τ` the cured run, and §C4 argues the
/// shape is physics (`L_crit = τ·√(σ/3P)` is exactly linear in `τ`) while
/// claiming the scaling "never sets a number in this document's acceptance
/// set", because every demand decision there is taken at a birth layer where
/// `τ = h`.
///
/// **That claim was measured and it is false.** With `1/(κ·τ)`, travel through
/// the thick body of a tilted disc is priced at a seventieth of a lamina's
/// cost, so a whole sloped underside is reached from a handful of arms at its
/// low edge — and the orientation objective, which scores a pose by the demand
/// the sweep measures, then PREFERS the tilted pose. Measured on Round 25: a
/// 3.6° tilt took 37 arms against the flat pose's 60 and won on that basis;
/// Round 100 tilted 10° took 10 arms under a 100 mm disc, with 829 overloaded
/// arms warning that those ten could not carry it. A thickness bonus that no
/// coupon in this repository measured is exactly what "a constant may prune a
/// search but never decide an outcome" forbids, and choosing a build pose is an
/// outcome.
///
/// `κ` was measured on single 50 µm laminae (plates 1 and 4). Pricing every
/// lateral step at that measured value keeps the reach test inside the regime
/// it was measured in. `τ` is still computed and still published
/// (`tau_vertical_over_tau_used_max`, `max_tau_at_binding_reach_mm`), so a
/// future green-strength coupon that measures the thickness law can be spent
/// deliberately rather than inherited by accident.
///
/// # The owner's ruling, 2026-09-03: one lamina, and arms placed by LOAD
///
/// The fix lane put this to the owner as a choice — license the thickness law
/// with a coupon at a second thickness, or place arms by LOAD as well as by
/// reach — and the ruling was the second. `τ` is spent NOWHERE from here on.
/// Two things follow, and both were needed before the one-lamina form could
/// stand:
///
/// * **The two sweeps now use one predicate.** The down-sweep bounds a load's
///   lateral travel at one lamina (`down_sweep::route_within_cover`); while
///   this function priced a step at `1/(κ·τ)` the up-sweep was strictly more
///   permissive in the plane, so `residual_demand == 0` never implied
///   `uncarried_load == 0` and the engine could print both about one field.
///   Same metric, same domain, same bound, one predicate.
/// * **Being over-reached is no longer the same thing as being under-carried.**
///   The one-lamina form places more arms; the down-sweep's load-driven
///   insertion (`down_sweep::insert_for_load`) is what makes the arms it places
///   carry what is routed through them, instead of clamping at `r_max` and
///   warning.
///
/// # What the one-lamina form does to §C4's rim ramp, arithmetically
///
/// Round 50's rim is drafted `θ = atan(1.0 / 3.487) = 16.0°` off vertical: it
/// rises 3.487 mm over 70 layers of `h = 0.05 mm` while moving 1.000 mm
/// outward. Per layer it steps `h·tanθ = 0.05 × 0.2868 = 0.014340 mm`, and over
/// its whole height that accumulates to `70 × 0.014340 = 1.0038 mm` against
/// `κ·h = 42.6006450 × 0.05 = 2.1300323 mm` of one lamina's reach — a cost of
/// **0.471, not 1.64**. §C4's "a single-lamina-only variant would charge
/// 70/42.6 = 1.64" charges the rim one FULL lamina step per layer, which is the
/// cost of 70 layers each creeping 0.05 mm, not 0.0143 mm. The rim does not
/// unroot on its own account under either form, and the arithmetic says so
/// before any code runs.
///
/// What the fix lane measured as "23 rim arms" is therefore not the rim's own
/// run: it is the bottom layer's covering being spent to its last millimetre
/// first. A hex covering pitched at exactly `√3·ρ` leaves its worst interstice
/// at `U = 1.000` by construction, so a cell that starts the climb already at
/// the limit unroots on the first lateral step, wherever that step is. That is
/// a fact about a zero-margin covering, not about the rim.
fn weight(state: &ReachState, kappa: f64, _cell: usize) -> f32 {
    // One lamina, everywhere. `state.tau_used_mm` is still advanced and still
    // published (`tau_vertical_over_tau_used_max`,
    // `max_tau_at_binding_reach_mm`), so a future green-strength coupon at a
    // second thickness can license the scaling deliberately rather than have it
    // inherited by accident. Sabotage S1 is what proves this is read.
    let lamina = state.lamina_mm;
    if lamina <= 0.0 {
        return f32::INFINITY;
    }
    (1.0 / (kappa as f32 * lamina)).max(0.0)
}

fn chamfer_forward(grid: &Grid, cover: &[bool], kappa: f64, state: &mut ReachState) {
    let cols = grid.cols();
    let pitch = grid.pitch_mm() as f32;
    let diagonal = pitch * core::f32::consts::SQRT_2;
    for row in 0..grid.rows() {
        for col in 0..cols {
            let cell = row * cols + col;
            if !cover[cell] {
                continue;
            }
            let w = weight(state, kappa, cell);
            let mut best = state.u[cell];
            if col > 0 {
                best = best.min(state.u[cell - 1] + pitch * w);
            }
            if row > 0 {
                best = best.min(state.u[cell - cols] + pitch * w);
                if col > 0 {
                    best = best.min(state.u[cell - cols - 1] + diagonal * w);
                }
                if col + 1 < cols {
                    best = best.min(state.u[cell - cols + 1] + diagonal * w);
                }
            }
            state.u[cell] = best;
        }
    }
}

fn chamfer_reverse(grid: &Grid, cover: &[bool], kappa: f64, state: &mut ReachState) {
    let cols = grid.cols();
    let rows = grid.rows();
    let pitch = grid.pitch_mm() as f32;
    let diagonal = pitch * core::f32::consts::SQRT_2;
    for row in (0..rows).rev() {
        for col in (0..cols).rev() {
            let cell = row * cols + col;
            if !cover[cell] {
                continue;
            }
            let w = weight(state, kappa, cell);
            let mut best = state.u[cell];
            if col + 1 < cols {
                best = best.min(state.u[cell + 1] + pitch * w);
            }
            if row + 1 < rows {
                best = best.min(state.u[cell + cols] + pitch * w);
                if col + 1 < cols {
                    best = best.min(state.u[cell + cols + 1] + diagonal * w);
                }
                if col > 0 {
                    best = best.min(state.u[cell + cols - 1] + diagonal * w);
                }
            }
            state.u[cell] = best;
        }
    }
}

/// Relax `U` outward from one newly inserted root, stopping where the
/// potential passes 1.
///
/// Bounded by construction: nothing beyond `U = 1` is enqueued, so the visited
/// set is the reach ball and never the layer.
pub fn relax_from_root(
    grid: &Grid,
    cover: &[bool],
    kappa: f64,
    root: usize,
    state: &mut ReachState,
    queue: &mut Vec<usize>,
    touched: &mut Vec<usize>,
) {
    let cols = grid.cols();
    let rows = grid.rows();
    let pitch = grid.pitch_mm() as f32;
    let diagonal = pitch * core::f32::consts::SQRT_2;
    queue.clear();
    state.u[root] = 0.0;
    touched.push(root);
    queue.push(root);
    while let Some(cell) = queue.pop() {
        let base = state.u[cell];
        if base > 1.0 {
            continue;
        }
        let row = cell / cols;
        let col = cell % cols;
        for (dr, dc) in [
            (-1_isize, -1_isize),
            (-1, 0),
            (-1, 1),
            (0, -1),
            (0, 1),
            (1, -1),
            (1, 0),
            (1, 1),
        ] {
            let Some(nr) = row.checked_add_signed(dr) else {
                continue;
            };
            let Some(nc) = col.checked_add_signed(dc) else {
                continue;
            };
            if nr >= rows || nc >= cols {
                continue;
            }
            let neighbour = nr * cols + nc;
            if !cover[neighbour] {
                continue;
            }
            let step = if dr == 0 || dc == 0 { pitch } else { diagonal };
            let candidate = base + step * weight(state, kappa, neighbour);
            if candidate < state.u[neighbour] {
                state.u[neighbour] = candidate;
                touched.push(neighbour);
                if candidate <= 1.0 {
                    queue.push(neighbour);
                }
            }
        }
    }
}

/// Carry the current layer's potential into the next, through `carry` only.
///
/// A cell that this layer COVERED but did not CARRY (an eroded hairline span,
/// [`crate::raster::rasterize_into`]) hands nothing upward. That keeps
/// "vertical inheritance" and "the down-sweep will route load into this column"
/// the same statement: the down-sweep's carriers are `carry ∪ struts`, so a
/// cell inheriting from a cover-only span would be rooted here and routable
/// nowhere.
pub fn commit_layer(state: &mut ReachState, carry: &[bool]) {
    for cell in 0..state.u.len() {
        state.u_prev[cell] = if carry.get(cell).copied().unwrap_or(false) {
            state.u[cell]
        } else {
            f32::INFINITY
        };
    }
}

/// Two-pass 8-neighbour chamfer distance to the nearest non-member cell, in mm.
fn chamfer_distance(grid: &Grid, member: &[bool], out: &mut [f32]) {
    let cols = grid.cols();
    let rows = grid.rows();
    let pitch = grid.pitch_mm() as f32;
    let diagonal = pitch * core::f32::consts::SQRT_2;
    for cell in 0..grid.cells() {
        out[cell] = if member[cell] { f32::INFINITY } else { 0.0 };
    }
    for row in 0..rows {
        for col in 0..cols {
            let cell = row * cols + col;
            if out[cell] == 0.0 {
                continue;
            }
            let mut best = out[cell];
            if col > 0 {
                best = best.min(out[cell - 1] + pitch);
            }
            if row > 0 {
                best = best.min(out[cell - cols] + pitch);
                if col > 0 {
                    best = best.min(out[cell - cols - 1] + diagonal);
                }
                if col + 1 < cols {
                    best = best.min(out[cell - cols + 1] + diagonal);
                }
            }
            // Outside the grid is not material. The working raster carries a
            // margin of at least one maximum tip radius on every side, so this
            // clamp binds only on a degenerate grid, and it binds in the
            // under-stating direction.
            if row == 0 || col == 0 || col + 1 == cols || row + 1 == rows {
                best = best.min(pitch);
            }
            out[cell] = best;
        }
    }
    for row in (0..rows).rev() {
        for col in (0..cols).rev() {
            let cell = row * cols + col;
            if out[cell] == 0.0 {
                continue;
            }
            let mut best = out[cell];
            if col + 1 < cols {
                best = best.min(out[cell + 1] + pitch);
            }
            if row + 1 < rows {
                best = best.min(out[cell + cols] + pitch);
                if col + 1 < cols {
                    best = best.min(out[cell + cols + 1] + diagonal);
                }
                if col > 0 {
                    best = best.min(out[cell + cols - 1] + diagonal);
                }
            }
            out[cell] = best;
        }
    }
}

#[cfg(test)]
mod tests {
    // Test-local arithmetic over hand-written, bounded fixture values: cell
    // counts from positive mm lengths, and exact pins on a run length that is a
    // whole number of pitches by construction.
    #![allow(clippy::cast_sign_loss, clippy::float_cmp)]

    use super::*;
    use crate::grid::plan_grid;

    #[test]
    fn the_in_plane_transform_measures_a_slabs_half_width() {
        let (grid, _) = plan_grid(0.0, 0.0, 4.0, 4.0, 1, 0.1, 0.0, 1_000_000_000);
        let mut member = vec![false; grid.cells()];
        for row in 10..30 {
            for col in 10..30 {
                member[grid.index(row, col)] = true;
            }
        }
        let mut out = vec![0.0_f32; grid.cells()];
        chamfer_distance(&grid, &member, &mut out);
        // The centre of a 20-cell square is 10 cells from the outside.
        let centre = out[grid.index(20, 20)];
        assert!((f64::from(centre) - 10.0 * grid.pitch_mm()).abs() < 1.0e-6);
        // A boundary cell is exactly one cell in.
        let edge = out[grid.index(10, 20)];
        assert!((f64::from(edge) - grid.pitch_mm()).abs() < 1.0e-6);
    }

    #[test]
    fn the_potential_reaches_exactly_one_lamina_of_reach_from_a_root() {
        let (grid, _) = plan_grid(0.0, 0.0, 12.0, 12.0, 1, 0.05, 0.0, 2_000_000_000);
        let cover = vec![true; grid.cells()];
        let mut state = ReachState::new(&grid);
        // One fresh lamina everywhere.
        state.tau_used_mm.fill(0.05);
        let root = grid.index(grid.rows() / 2, grid.cols() / 2);
        recompute_potential(&grid, &cover, 42.600_645, 0.05, &[root], &mut state);
        let reach_cells = 42.600_645 * 0.05 / grid.pitch_mm();
        // Along a row, the last rooted cell sits at the measured reach.
        let mut last = 0_usize;
        for col in grid.cols() / 2..grid.cols() {
            if state.is_rooted(grid.index(grid.rows() / 2, col)) {
                last = col - grid.cols() / 2;
            }
        }
        assert!(
            (last as f64 - reach_cells).abs() <= 1.0,
            "rooted run {last} cells against a measured reach of {reach_cells:.2} cells"
        );
    }
    /// A solid column never unroots; a creeping ramp does, at the run length
    /// `κ·h` buys it.
    ///
    /// These two pull in opposite directions, which is why they are pinned
    /// together. The column is §C4's requirement (a plain disc's own body must
    /// not turn into demand a hundred layers up, where no arm can physically
    /// go), and it is bought by the `1/(κ·τ)` step cost: a column rooted at its
    /// base pays `Σ 1/(κ·k) = H_K/κ`, which is 0.155 over 400 layers. The ramp
    /// is what the sweep exists to catch and what a per-layer re-seeded
    /// rootedness cannot see: each layer overlaps the one below, so no single
    /// step is ever too long, and only the ACCUMULATED lateral run unroots it.
    ///
    /// The two cases differ in one thing only — whether the material moves
    /// sideways — so this test grades the accumulation and nothing else.
    #[test]
    fn a_column_never_unroots_and_a_creeping_ramp_unroots_at_the_accumulated_run() {
        let (grid, _) = plan_grid(0.0, 0.0, 24.0, 4.0, 1, 0.05, 0.0, 2_000_000_000);
        let kappa = 42.600_645;
        let height = 0.05;
        let reach_mm = kappa * height;
        let row = grid.rows() / 2;

        // ---- a solid block, one root, 400 layers, no root re-seeded --------
        let cover = vec![true; grid.cells()];
        let mut state = ReachState::new(&grid);
        let root = grid.index(row, grid.cols() / 2);
        advance_thickness(&grid, &cover, &cover, height, &mut state);
        recompute_potential(&grid, &cover, kappa, height, &[root], &mut state);
        let far = grid.index(row, grid.cols() / 2 + 40);
        assert!(state.is_rooted(far), "a cell 2 mm from a root must be rooted");
        commit_layer(&mut state, &cover);
        for _ in 2..=400 {
            advance_thickness(&grid, &cover, &cover, height, &mut state);
            recompute_potential(&grid, &cover, kappa, height, &[], &mut state);
            commit_layer(&mut state, &cover);
        }
        assert!(
            state.is_rooted(far),
            "a solid column unrooted itself from the inside after 400 layers; potential {}, \
             against the H_K/κ bound of {:.4} §C4 predicts",
            state.u[far],
            (1..=400).map(|k| 1.0 / (kappa * f64::from(k))).sum::<f64>(),
        );
        assert!(
            state.u_from_vertical_max < 1.0,
            "vertical inheritance alone accumulated {:.4} over 400 layers; §C4 requires the climb \
             never to unroot anything on its own",
            state.u_from_vertical_max,
        );

        // ---- a shell creeping sideways every layer -------------------------
        //
        // The band always overlaps the layer below, so the overlap inherits and
        // only the new leading cell has to be reached laterally. No single step
        // is ever long; the RUN is what binds, and a rootedness re-seeded per
        // layer cannot see it at all.
        //
        // The band is TWO cells wide on purpose. A shell that never thickens
        // holds `τ` at one or two laminae, so every step is priced at
        // `1/(κ·h)`, which is the regime `κ` was actually measured in. Widen it
        // and the band stops being a shell: the run-until-unrooted below
        // measures the thickness bonus instead, which is the separate claim the
        // thick-ramp case makes.
        let run_until_unrooted = |band_cells: usize, step_mm: f64| -> f64 {
            let step_cells = (step_mm / grid.pitch_mm()).round().max(1.0) as usize;
            let mut state = ReachState::new(&grid);
            let mut cover = vec![false; grid.cells()];
            let mut start = 0_usize;
            for col in start..start + band_cells {
                cover[grid.index(row, col)] = true;
            }
            let seed = grid.index(row, start + band_cells / 2);
            advance_thickness(&grid, &cover, &cover, height, &mut state);
            recompute_potential(&grid, &cover, kappa, height, &[seed], &mut state);
            commit_layer(&mut state, &cover);
            let mut travelled = 0.0_f64;
            loop {
                cover.fill(false);
                start += step_cells;
                if start + band_cells >= grid.cols() {
                    return f64::INFINITY;
                }
                for col in start..start + band_cells {
                    cover[grid.index(row, col)] = true;
                }
                advance_thickness(&grid, &cover, &cover, height, &mut state);
                recompute_potential(&grid, &cover, kappa, height, &[], &mut state);
                let leading = grid.index(row, start + band_cells - 1);
                if !state.is_rooted(leading) {
                    return travelled;
                }
                travelled += step_cells as f64 * grid.pitch_mm();
                commit_layer(&mut state, &cover);
            }
        };

        // A SHELL creeping one cell a layer unroots after roughly the run one
        // lamina spans, not after one step. Banded either side by a factor of
        // two, because the leading cell also pays for the width of its own band
        // and the exact number is a discretisation, not a physical claim.
        let shell = run_until_unrooted(2, grid.pitch_mm());
        assert!(
            shell.is_finite() && shell >= 0.5 * reach_mm && shell <= 2.0 * reach_mm,
            "a two-cell shell creeping {:.3} mm a layer ran {shell:.4} mm before unrooting, \
             against the {reach_mm:.4} mm one lamina spans; a re-seeded rootedness would run \
             forever",
            grid.pitch_mm(),
        );
        // A band that steps further in ONE layer than a lamina spans unroots at
        // once, which is the case a run-length rule alone would miss.
        assert_eq!(
            run_until_unrooted(2, 2.8),
            0.0,
            "a 2.8 mm step is beyond the measured reach {reach_mm:.4} mm and must unroot at once"
        );
        // And the counterpart, stated so it is a RULING and not an accident.
        //
        // **Owner's ruling, 2026-09-03: thickness buys nothing here.** This
        // assertion used to require a 4 mm-THICK band to run FOREVER, because
        // `1/(κ·τ)` prices travel through 80 laminae of cured section at 1/80th
        // of a lamina's cost — the design of record's §5.2 form and §C4's
        // decision. That was the one place in this crate where a decision was
        // taken outside the regime `κ` was measured in (single 50 µm laminae,
        // extrapolated to a 3.5 mm section, a factor of 70), and it decided
        // build poses. The ruling clamps every lateral step to one lamina and
        // pays for the arms that costs with load-driven insertion in the
        // down-sweep instead.
        //
        // So a thick band now runs exactly as far as a thin one — and the
        // expected distance is a HAND-DERIVED number, not the thin case's:
        // the band's leading cell starts `(cells/2 − 1)·p = 1.9500 mm` from the
        // seed at its centre, which is `0.9155` of the reach already spent, so
        // only `ρ − 1.95 = 0.1800 mm` of run is left, and at one cell a layer it
        // unroots on the fourth step, after `3 × 0.05 = 0.1500 mm` travelled.
        // Anyone who restores the thickness bonus will see this go red, and
        // should know they are changing a ruling rather than a detail.
        let thick_cells = (4.0 / grid.pitch_mm()).round() as usize;
        let thick = run_until_unrooted(thick_cells, grid.pitch_mm());
        let offset_mm = (thick_cells / 2 - 1) as f64 * grid.pitch_mm();
        let predicted_mm = ((reach_mm - offset_mm) / grid.pitch_mm()).floor() * grid.pitch_mm();
        assert!(
            thick.is_finite() && (thick - predicted_mm).abs() <= grid.pitch_mm(),
            "a 4 mm-thick band ran {thick:.4} mm before unrooting against the {predicted_mm:.4} \
             mm one lamina's reach {reach_mm:.4} mm leaves after the band's own {offset_mm:.4} \
             mm half-width; thickness must buy nothing"
        );
    }
}

#[cfg(test)]
mod relax_tests {
    use super::*;
    use crate::grid::plan_grid;

    #[test]
    fn one_insertion_roots_a_whole_reach_ball() {
        let (grid, _) = plan_grid(0.0, 0.0, 12.0, 12.0, 1, 0.05, 0.0, 2_000_000_000);
        let cover = vec![true; grid.cells()];
        let mut state = ReachState::new(&grid);
        state.tau_used_mm.fill(0.05);
        state.lamina_mm = 0.05;
        let root = grid.index(grid.rows() / 2, grid.cols() / 2);
        let mut queue = Vec::new();
        let mut touched = Vec::new();
        relax_from_root(&grid, &cover, 42.600_645, root, &mut state, &mut queue, &mut touched);
        let rooted = (0..grid.cells()).filter(|cell| state.is_rooted(*cell)).count();
        let reach_mm = 42.600_645 * 0.05;
        let ideal = core::f64::consts::PI * reach_mm * reach_mm / grid.cell_area_mm2();
        println!("rooted {rooted} cells against an ideal disc of {ideal:.0}");
        assert!(
            rooted as f64 > 0.85 * ideal,
            "one insertion rooted {rooted} cells against a reach disc of {ideal:.0}"
        );
    }
}
