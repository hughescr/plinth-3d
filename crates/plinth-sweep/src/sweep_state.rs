//! **What a sweep carries across checkpoint C** (SC-1, `SC-DESIGN.md` §2).
//!
//! `run_sweep_inner` is two halves. [`crate::plan_to_c`] runs everything up to
//! the start of `Span::StageJoints` (the head sizing, the joint certificate and
//! the finishing of the field: seat faces, footprint slides, real contact,
//! feet, censuses). [`crate::finish`] runs the rest. What the second half reads
//! of the first is split in three, by what may be done with it:
//!
//! * [`SweepEnv`]: immutable after C, `Send + Sync`, shared by `Arc` between
//!   every trial and every finalist. The grid, the slices, the drain keep-out,
//!   the column history with its hidden cavities, the laws and the sizing
//!   policy, the raw drain and vent specs (the escape check reads them), the
//!   model surface in a `OnceLock` (built on first need, the same function and
//!   the same bits as before; a `OnceCell` is not `Sync`), and the scalar
//!   summary of the reach state.
//! * [`LaneState`]: owned, `Clone`. The members and the down-sweep outcome of
//!   the last pass, and every outcome and tally a lane filled before C that
//!   `finish` publishes. `finish` takes it BY VALUE, so a caller that wants to
//!   finish the same state twice finishes two clones and the original is never
//!   touched ([`EditSeed::finish_clone`]).
//! * [`GreedyFinal`]: the few reads `finish` makes of the up-sweep's greedy
//!   scratch (the stranded seat cells, the path-hit counters, the placement
//!   heads' outcome); the scratch buffers themselves are lane-internal and die
//!   at C.
//!
//! Not carried, on purpose: the layer cache (it borrows its grid and slices, so
//! it cannot live beside them; `finish` never reads it, and the edit rebuilds it
//! from the env), `GreedyScratch` buffers, `replay_context`, the births, the
//! lip-seat refusals and the lip and tie locks.
//!
//! An [`EditSeed`] is an env, the lane state AT C, an identity key and the
//! digest of the plan the seed finished into. Nothing consumes it yet; the edit
//! transaction (SC-3) will, and refuses it on any key mismatch ([`SeedMiss`]).

use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use geom_kernel::NativeSurface;
use plinth_core::{LayerPlan, TipContactProfiles};
use plinth_mesh::Mesh;
use plinth_support::perception::SliceSolid;

use crate::adapter::LoadStepTaper;
use crate::drain::{DrainKeepOut, DrainSpec};
use crate::down_sweep::{DownSweepOutcome, RunSeed, SizingPolicy, UnrootedColumns};
use crate::grid::Grid;
use crate::heads::{HeadRule, HeadsOutcome};
use crate::lean::LeanLaw;
use crate::reach::ReachState;
use crate::slenderness::{SlendernessLaw, SlendernessOutcome};
use crate::tip_landing::SeatFaceRule;
use crate::up_sweep::{ColumnHistory, DescentTally, GreedyScratch, ShaftSpan};
use crate::{
    births, congestion, head_join, landing_check, lip_seat, orient_demand, provenance, report,
    stranded_branch, SweepError, SweepPlan, SweepPolicy, SweepStrut,
};

/// The scalars of the reach state that the report reads after C. The state's
/// per-cell arrays (potential, anchors, clearance, six `f32` per cell) are
/// needed by nothing after the up-sweep, so a seed keeps this and not them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ReachSummary {
    pub tau_at_binding_reach_max_mm: f64,
    pub u_from_vertical_max: f64,
    pub tau_vertical_over_used_max: f64,
    pub chamfer_not_converged: u64,
    pub reach_tests: u64,
    pub vertical_inheritance_hits: u64,
    pub creep_cells_weighted: u64,
    pub creep_steps_taken: u64,
    pub creep_cells_free: u64,
    pub sightlines_clear: u64,
    pub sightlines_blocked: u64,
}

impl ReachSummary {
    pub(crate) const fn of(reach: &ReachState) -> Self {
        Self {
            tau_at_binding_reach_max_mm: reach.tau_at_binding_reach_max_mm,
            u_from_vertical_max: reach.u_from_vertical_max,
            tau_vertical_over_used_max: reach.tau_vertical_over_used_max,
            chamfer_not_converged: reach.chamfer_not_converged,
            reach_tests: reach.reach_tests,
            vertical_inheritance_hits: reach.vertical_inheritance_hits,
            creep_cells_weighted: reach.creep_cells_weighted,
            creep_steps_taken: reach.creep_steps_taken,
            creep_cells_free: reach.creep_cells_free,
            sightlines_clear: reach.sightlines_clear,
            sightlines_blocked: reach.sightlines_blocked,
        }
    }
}

/// Everything `finish` reads of the sweep that is fixed at C. Immutable and
/// shared; `Send + Sync` (asserted below).
pub struct SweepEnv {
    pub(crate) grid: Grid,
    pub(crate) coarsening_steps: u32,
    pub(crate) layers: usize,
    pub(crate) height: f64,
    pub(crate) plate_z: f64,
    pub(crate) planes: Vec<f64>,
    pub(crate) slices: Vec<Vec<SliceSolid>>,
    pub(crate) keep_out: DrainKeepOut,
    pub(crate) history: ColumnHistory,
    /// The raw drain and vent specs the sweep ran with: the escape check reads
    /// the drains at the end, and the identity key compares both.
    pub(crate) drains: Vec<DrainSpec>,
    pub(crate) vents: Vec<DrainSpec>,
    pub(crate) reach: ReachSummary,
    pub(crate) unrooted: UnrootedColumns,
    pub(crate) sizing: SizingPolicy,
    pub(crate) law: LeanLaw,
    pub(crate) slenderness_law: SlendernessLaw,
    pub(crate) load_step_taper: LoadStepTaper,
    pub(crate) seat_face_rule: SeatFaceRule,
    pub(crate) head_registry: Option<(&'static TipContactProfiles, HeadRule)>,
    /// Whether the model surface had been built when the slenderness context
    /// was first made (the early build the descent test and the thickening
    /// refusal ask for, and nothing else, precedes it). The context holds the
    /// surface only if so; `finish` rebuilds the context and must hold the same.
    pub(crate) surface_in_slenderness_context: bool,
    /// The model's surface index, built on first need by whoever reads it
    /// first (`None` inside when it could not be built).
    pub(crate) model_surface: OnceLock<Option<NativeSurface>>,
}

impl SweepEnv {
    /// The drains the sweep ran with.
    #[must_use]
    pub fn drains(&self) -> &[DrainSpec] {
        &self.drains
    }

    /// The vents the sweep ran with.
    #[must_use]
    pub fn vents(&self) -> &[DrainSpec] {
        &self.vents
    }

    /// A LOWER-BOUND estimate of the env's heap bytes: the slices, the column
    /// history arrays and spans, the keep-out and unrooted tables. The model
    /// surface index (see [`EditSeed::approx_bytes`]) and the hidden-cavity
    /// lattice are not counted (their types do not expose a size); the RSS of
    /// a run is the full measure.
    #[must_use]
    pub fn approx_bytes(&self) -> u64 {
        let mut bytes = std::mem::size_of::<Self>() as u64;
        for layer in &self.slices {
            bytes += (layer.capacity() * std::mem::size_of::<SliceSolid>()) as u64;
            for solid in layer {
                bytes += (solid.outer().points_xy_mm().len() * 16) as u64;
                for hole in solid.holes() {
                    bytes += (hole.points_xy_mm().len() * 16) as u64;
                }
            }
        }
        let history = &self.history;
        let cells = self.grid.cells() as u64;
        bytes += cells * 4 * 5 // top_model, top_strut, top_strut_id, column_max_model, shaft_head
            + cells // top_model_rooted
            + cells * 4 // model_head
            + ((history.model_spans.len() + history.shaft_spans.len())
                * std::mem::size_of::<ShaftSpan>()) as u64
            + (history.joinable_tops.len() * 24) as u64;
        if self.keep_out.blocked_cells() > 0 {
            bytes += cells * 4;
        }
        bytes += (self.unrooted.total() * 4) as u64;
        bytes += (self.planes.len() * 8) as u64;
        bytes
    }
}

/// What `finish` reads of the up-sweep's greedy scratch.
#[derive(Clone, Debug, Default)]
pub(crate) struct GreedyFinal {
    pub stranded_seat_cells: Vec<(usize, usize)>,
    pub islands_compared: u64,
    pub islands_realigned: u64,
    pub seats_saved: u64,
    pub island_seats: u64,
    pub stranded_seats: u64,
    pub twin_merges: u64,
    pub twin_headed_hosts: u64,
    pub island_hex_bound: u64,
    /// The placement heads' outcome and residuals (`HeadsState` without its
    /// accepted capsules, which nothing reads after the up-sweep).
    pub heads: HeadsOutcome,
    pub residuals_deg: Vec<f64>,
}

impl GreedyFinal {
    pub(crate) fn of(greedy: &mut GreedyScratch) -> Self {
        let heads = std::mem::take(&mut greedy.heads);
        Self {
            stranded_seat_cells: std::mem::take(&mut greedy.stranded_seat_cells),
            islands_compared: greedy.islands_compared,
            islands_realigned: greedy.islands_realigned,
            seats_saved: greedy.seats_saved,
            island_seats: greedy.island_seats,
            stranded_seats: greedy.stranded_seats,
            twin_merges: greedy.twin_merges,
            twin_headed_hosts: greedy.twin_headed_hosts,
            island_hex_bound: greedy.island_hex_bound,
            heads: heads.outcome,
            residuals_deg: heads.residuals_deg,
        }
    }
}

/// The owned half of the state at C: the members and every outcome and tally
/// `finish` reads or publishes. `Clone` so a trial finishes a copy.
#[derive(Clone, Debug)]
pub struct LaneState {
    pub(crate) struts: Vec<SweepStrut>,
    pub(crate) outcome: DownSweepOutcome,
    pub(crate) landings: landing_check::LandingLedger,
    pub(crate) moves: provenance::Moves,
    pub(crate) removal_coverage: report::RemovalCoverageReport,
    pub(crate) removals: u64,
    pub(crate) removals_refused_by_span: u64,
    pub(crate) removal_passes_rolled_back: u64,
    pub(crate) merges_refused_by_radius: usize,
    pub(crate) merges_kept_carrying: usize,
    pub(crate) merges_moved_with_trunk: usize,
    pub(crate) unmerge_stale_landings: usize,
    pub(crate) slenderness_outcome: SlendernessOutcome,
    pub(crate) congestion_outcome: congestion::CongestionOutcome,
    pub(crate) stranded_branch_outcome: stranded_branch::StrandedBranchOutcome,
    pub(crate) graze_outcome: HeadsOutcome,
    pub(crate) head_join_outcome: head_join::HeadJoinOutcome,
    pub(crate) lip_seat_outcome: lip_seat::LipSeatOutcome,
    pub(crate) clearance_reroute_outcome: lip_seat::LipSeatOutcome,
    pub(crate) demand_cells_total: usize,
    pub(crate) residual_demand_cells: usize,
    pub(crate) unrooted_landings: usize,
    pub(crate) keep_out_refusals: usize,
    pub(crate) arms_leaned: usize,
    pub(crate) arms_merged: usize,
    pub(crate) lean_paths_blocked: usize,
    pub(crate) junctions_mid_shaft_descent: usize,
    pub(crate) lean_candidates_priced: u64,
    pub(crate) rasteriser_entries: u64,
    pub(crate) lift_assertions: u64,
    pub(crate) descent_tally: DescentTally,
    pub(crate) birth_tally: births::BirthTally,
    pub(crate) greedy: GreedyFinal,
    /// The keep-out's refusal counter READ AT C. The counter is an atomic on a
    /// shared env, so a trial that ran the keep-out would move it; the report
    /// takes this value instead, which is what it read before the split.
    pub(crate) drain_keep_out_refusals: u64,
}

impl LaneState {
    /// The members at C.
    #[must_use]
    pub fn struts(&self) -> &[SweepStrut] {
        &self.struts
    }

    /// The down-sweep outcome of the last pass before C.
    #[must_use]
    pub const fn outcome(&self) -> &DownSweepOutcome {
        &self.outcome
    }

    /// Heap bytes of the members and the route log (lower bound).
    fn approx_bytes(&self) -> u64 {
        let mut bytes = std::mem::size_of::<Self>() as u64;
        for strut in &self.struts {
            bytes += std::mem::size_of::<SweepStrut>() as u64
                + 8 * (strut.path.len()
                    + strut.radius_mm.len()
                    + strut.carried_mm2.len()
                    + strut.thickened_mm.len()) as u64;
        }
        let log = &self.outcome.route_log;
        bytes += 8
            * (log.hops.iter().map(Vec::len).sum::<usize>()
                + log.reads.iter().map(Vec::len).sum::<usize>()) as u64;
        bytes += 16 * self.greedy.stranded_seat_cells.len() as u64;
        bytes
    }
}

/// Why a seed does not belong to a request. The first failing check in key
/// order; every one is a discard of the edit, counted, never a reuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedMiss {
    /// The mesh is not bit-equal to the one the seed was made on.
    Mesh,
    /// The layer plan is not bit-equal.
    Layers,
    /// The sweep policy differs (after [`orient_demand::production_measurement_policy`]
    /// on both sides).
    Policy,
    /// The drains differ.
    Drains,
    /// The vents differ.
    Vents,
    /// The plan the seed finished into is not the plan the caller holds.
    FirstPlan,
}

/// The identity of what a seed was made from. Two sweeps of equal keys have
/// equal state at C, because the sweep is a deterministic function of exactly
/// these inputs.
#[derive(Clone, Debug, PartialEq)]
pub struct SeedKey {
    mesh: (usize, usize, u64),
    layers: (usize, u64),
    policy: SweepPolicy,
    drains: Vec<[u64; 7]>,
    vents: Vec<[u64; 7]>,
}

fn mix(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
}

fn mesh_bits(mesh: &Mesh) -> (usize, usize, u64) {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for vertex in mesh.verts() {
        for coordinate in vertex {
            mix(&mut hash, coordinate.to_bits());
        }
    }
    for triangle in mesh.tris() {
        for corner in triangle {
            mix(&mut hash, u64::from(*corner));
        }
    }
    let mut welded = DefaultHasher::new();
    format!("{:?}", mesh.weld_policy()).hash(&mut welded);
    mix(&mut hash, welded.finish());
    (mesh.verts().len(), mesh.tris().len(), hash)
}

fn layer_bits(layers: &LayerPlan) -> (usize, u64) {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    mix(&mut hash, layers.z0_mm().to_bits());
    for height in layers.heights_mm() {
        mix(&mut hash, height.to_bits());
    }
    (layers.heights_mm().len(), hash)
}

fn hole_bits(holes: &[DrainSpec]) -> Vec<[u64; 7]> {
    holes
        .iter()
        .map(|hole| {
            [
                hole.outlet_mm[0].to_bits(),
                hole.outlet_mm[1].to_bits(),
                hole.outlet_mm[2].to_bits(),
                hole.outward_axis[0].to_bits(),
                hole.outward_axis[1].to_bits(),
                hole.outward_axis[2].to_bits(),
                hole.radius_mm.to_bits(),
            ]
        })
        .collect()
}

impl SeedKey {
    /// The key of a sweep of `mesh` on `layers` under `policy` with these
    /// holes. The policy is normalized with
    /// [`orient_demand::production_measurement_policy`], so a seed made under
    /// the measurement policy (shake check deferred) and used under the
    /// emission policy compare equal, as the winner-plan reuse key does.
    #[must_use]
    pub fn of(
        mesh: &Mesh,
        layers: &LayerPlan,
        policy: &SweepPolicy,
        drains: &[DrainSpec],
        vents: &[DrainSpec],
    ) -> Self {
        Self {
            mesh: mesh_bits(mesh),
            layers: layer_bits(layers),
            policy: orient_demand::production_measurement_policy(*policy),
            drains: hole_bits(drains),
            vents: hole_bits(vents),
        }
    }

    /// The first part of `self` that differs from the key of the request, in
    /// key order, or `None` when they are equal.
    #[must_use]
    pub fn miss(&self, request: &Self) -> Option<SeedMiss> {
        if self.mesh != request.mesh {
            return Some(SeedMiss::Mesh);
        }
        if self.layers != request.layers {
            return Some(SeedMiss::Layers);
        }
        if self.policy != request.policy {
            return Some(SeedMiss::Policy);
        }
        if self.drains != request.drains {
            return Some(SeedMiss::Drains);
        }
        if self.vents != request.vents {
            return Some(SeedMiss::Vents);
        }
        None
    }
}

/// The first plan's state at C, kept for an edit of that plan.
pub struct EditSeed {
    env: Arc<SweepEnv>,
    lane: LaneState,
    key: SeedKey,
    /// The `sweep_digest` of the plan this seed finished into.
    p1_digest: String,
    bytes: u64,
}

impl fmt::Debug for EditSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditSeed")
            .field("members", &self.lane.struts.len())
            .field("bytes", &self.bytes)
            .field("p1_digest", &self.p1_digest)
            .finish_non_exhaustive()
    }
}

impl EditSeed {
    /// `surface_bytes` is the retained size of the model surface index if the
    /// env holds one (its type exposes no size; the caller passes the
    /// kernel's own estimate for the mesh and pitch it was built from).
    pub(crate) fn new(
        env: Arc<SweepEnv>,
        lane: LaneState,
        key: SeedKey,
        p1_digest: String,
        surface_bytes: u64,
    ) -> Self {
        let bytes = env.approx_bytes() + surface_bytes + lane.approx_bytes();
        Self {
            env,
            lane,
            key,
            p1_digest,
            bytes,
        }
    }

    /// The shared, immutable env.
    #[must_use]
    pub const fn env(&self) -> &Arc<SweepEnv> {
        &self.env
    }

    /// The state at C. Never mutated: a user clones it ([`Self::lane_clone`]).
    #[must_use]
    pub const fn lane(&self) -> &LaneState {
        &self.lane
    }

    /// A copy of the state at C: the working state of an edit.
    #[must_use]
    pub fn lane_clone(&self) -> LaneState {
        self.lane.clone()
    }

    /// The routing of the last pass before C, as the seed the next sizing of
    /// the same field takes its pass 1 from (SC-2). `None` where the sweep
    /// logged no route.
    #[must_use]
    pub fn run_seed(&self) -> Option<RunSeed> {
        self.lane.outcome.seed()
    }

    /// The identity key.
    #[must_use]
    pub const fn key(&self) -> &SeedKey {
        &self.key
    }

    /// The `sweep_digest` of the plan this seed finished into.
    #[must_use]
    pub fn p1_digest(&self) -> &str {
        &self.p1_digest
    }

    /// An estimate of the seed's heap bytes: [`SweepEnv::approx_bytes`], the
    /// lane's members and route log, and the model surface index (the
    /// kernel's own retained-size estimate).
    #[must_use]
    pub const fn approx_bytes(&self) -> u64 {
        self.bytes
    }

    /// Whether this seed was made from exactly this request and finished into
    /// the plan whose `sweep_digest` is `first_digest`; else the first
    /// difference. The caller discards the edit on `Some`.
    #[must_use]
    pub fn miss(
        &self,
        mesh: &Mesh,
        layers: &LayerPlan,
        policy: &SweepPolicy,
        drains: &[DrainSpec],
        vents: &[DrainSpec],
        first_digest: &str,
    ) -> Option<SeedMiss> {
        self.key
            .miss(&SeedKey::of(mesh, layers, policy, drains, vents))
            .or_else(|| (self.p1_digest != first_digest).then_some(SeedMiss::FirstPlan))
    }

    /// Finish a CLONE of the state at C: the plan the sweep would have
    /// returned. The seed is untouched, so this can be called any number of
    /// times and gives equal plans.
    ///
    /// # Errors
    ///
    /// As the sweep: a cancelled run.
    pub fn finish_clone(
        &self,
        mesh: &Mesh,
        layers: &LayerPlan,
        policy: &SweepPolicy,
    ) -> Result<SweepPlan, SweepError> {
        crate::finish(&self.env, self.lane.clone(), mesh, layers, policy)
    }
}

// The seed crosses the rayon race (retained plans) and is shared by the edit's
// trials; a field that is not `Send + Sync` fails to compile here, not at a
// distant call site.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SweepEnv>();
    assert_send_sync::<LaneState>();
    assert_send_sync::<EditSeed>();
    assert_send_sync::<Arc<EditSeed>>();
};
