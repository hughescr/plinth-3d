//! **The toed slab lands flat (authored top down, toes up) at its production
//! pose search, and by a margin, not a tie.**
//!
//! At X 18 the slab stood on its toes, which is worse on scar, removal, print
//! risk and resin (`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/
//! creep-arms-diagnosis.md` Q2). That diagnosis traced the 0.015 % tie to the
//! fixture: three overlapping shells whose buried toe caps charged the flipped
//! pose 1.919 raw mm2 of down-facing area. The toed-slab-fixture lane
//! (2026-09-30) made the fixture one closed solid
//! (`plinth-mesh` `fixtures::toed_slab_is_one_closed_solid_with_no_buried_toe_caps`).
//! This test runs the production chain in `prepare.rs`'s order (repair,
//! registered decimation, dense search with the production sweep demand).
//! Scores of the two poses that matter, MEASURED:
//!
//! | fixture, objective | on its toes (0,0,+1) | flat (0,0,-1) | toes - flat |
//! |---|---|---|---|
//! | three shells, fold per triangle | 7.191398 | 7.192495 | -0.001096 (toes win, a tie) |
//! | one solid, fold per triangle | 6.975059 | 7.184816 | -0.209757 (toes win) |
//! | one solid, fold by length (`04443a5`) | 6.661658 | 6.646539 | **+0.015118 (flat wins)** |
//! | three shells, fold by length (sabotage) | 6.651946 | 6.671738 | -0.019792 (toes win) |
//!
//! The last row is this test run on `make_toed_slab_three_shells` (2026-10-01,
//! RED as it should be): with the buried toe caps back, the flipped pose has 66
//! down-facing faces and a fold of 7.3587 raw mm2 against 1.0603, and stands on
//! its toes again. So BOTH changes are needed for the flat pose: the union and
//! the length-spread fold. The field input `corpus/models/make_toed_slab.stl`
//! is still the three-shell mesh.
//!
//! The middle row was the lane's first result: the fragility fold charged a
//! corner's angle defect over each whole face, so it read the TRIANGULATION of
//! the flat underside, and the union's 70-triangle underside paid 81.059 raw
//! mm2 where the two-triangle top paid 135.000. The scar-fold-invariance lane
//! (`04443a5`) spreads the fold by length instead; on the solid the flipped
//! pose's fold is now 1.0603 raw mm2 against 3.9718 on its toes.
//!
//! **What carries the margin.** Peel (3.638541) and height (10 mm) are equal in
//! the two poses and support burden nearly so (0.007383 vs 0.007461 cm2). The
//! scar term is 0.004307 flat against 0.019386 on its toes, a difference of
//! 0.015079 of the 0.015118 margin. Under it, the measured tip scar is
//! 0.0158 raw mm2 flat against 0.8747 on its toes: the toes-down pose puts its
//! tips on the toe cones. So the decision is a physical one, and the test pins
//! the tip-scar ordering beside the score ordering.
//!
//! **The 0.01-point bound.** It is not a physics constant; it separates a
//! decision from the 0.0011-point tie the three-shell fixture turned on (about
//! 9 x that tie). The measured margin, 0.015118, clears it by about 1.5 x, which
//! is narrow: a change that moves the scar term's weight or the tip sizing can
//! cross it, and that is a finding to report, not a bound to lower.

use std::time::Instant;

use plinth_core::m0_constants;
use plinth_decimate::{BudgetConfigError, ProbeConfig};
use plinth_orient::demand::{DemandFidelity, SupportDemandMeasurer};
use plinth_orient::registered_sweep_request;
use plinth_orient::search::{search, SearchTier};
use plinth_sweep::SweepDemand;

/// How long the dense search took before the round-13 frame bands, seconds:
/// this test at `a73e40a` and `bf5802f` took 38 to 40 s (round-13 milestone
/// bisect), and `bin/plinth-a73e40a` on the same slab through the CLI
/// measured 3.66 s for the prefilter's 21 poses plus 33.85 s for the three
/// production poses, 37.5 s (`lanes/band-speed.md`, 4 threads).
const A73E40A_SEARCH_S: f64 = 38.0;

/// The speed guard's bound: 2.5 times [`A73E40A_SEARCH_S`]. Measured on an
/// idle machine at 4 threads, 2026-10-04 (`lanes/band-speed.md`): `70e2639`
/// (the frame bands, unfixed), through the CLI, 80.65 s for the prefilter and
/// 33.87 s for the production poses, **114.5 s, 3.0 times**, over the bound;
/// the band-speed lane's search, through the CLI 15.3 s and 34.3 s and in this
/// test **49.9 s, 1.3 times**, a factor 1.9 under it, so a neighbour test
/// that takes half the machine does not trip it. A bound, not a target: the
/// search is allowed to get slower for a reason, and the reason is then
/// written in this constant's derivation.
///
/// Re-measured 2026-10-04 by lane prefilter-no-brace (Craig ruling "Skip
/// bracing in the quick pass": the prefilter runs no tie pass, only the
/// finalists are braced): **36.9 s in this test** (the prefilter's 21 poses
/// 3.4 s, down from 23.7 s; the 3 production poses 33.4 s), 0.97 times
/// [`A73E40A_SEARCH_S`] and a factor 2.6 under this bound. The bound is
/// generous on purpose and catches the round-13 class of regression (3 times
/// a73e40a). **It is scaled by the machine's load**, measured in the same run:
/// the production stage's code did not change in the lane that made this
/// guard, so its time over [`A73E40A_PRODUCTION_S`] (never below 1) is how
/// much slower this machine is than the idle one the numbers come from. The
/// unscaled bound failed on a machine with load average 13 (search 225.9 s,
/// production stage 201.6 s, a factor 6, prefilter 24.2 s: the search was
/// innocent). The prefilter's SHARE of the production stage cannot be moved by
/// load, and is guarded on its own ([`PREFILTER_SHARE_BOUND`]).
const SEARCH_BOUND_S: f64 = 2.5 * A73E40A_SEARCH_S;

/// The production stage's time at `a73e40a` (the three finalists, 4 threads,
/// idle machine), seconds: the yardstick for the machine's load in this run.
const A73E40A_PRODUCTION_S: f64 = 33.85;

/// The prefilter's time as a share of the production stage's, in the same
/// run, so load cancels. Measured on an idle machine at 4 threads: `a73e40a`
/// 3.66 / 33.85 = 0.11; lane prefilter-no-brace 3.44 / 33.42 = 0.10 (and
/// 24.18 / 201.63 = 0.12 on a machine at load average 13). The regressions it
/// exists to catch: the brace ladder run for every prefilter pose, 23.7 / 33 =
/// 0.72 (band-speed lane) and 80.65 / 33.87 = 2.4 (`70e2639`). The bound 0.5
/// is 4.8 times the measured share and 0.7 times the smaller regression.
const PREFILTER_SHARE_BOUND: f64 = 0.5;

/// The real measurer, timed per fidelity, so the guard can read the two stages
/// apart. It delegates every call and changes nothing.
struct TimedDemand {
    inner: SweepDemand,
    /// Seconds spent in `[prefilter, production]` calls.
    spent: std::sync::Mutex<[f64; 2]>,
}

impl SupportDemandMeasurer for TimedDemand {
    fn measure_support_demand(
        &self,
        mesh: &plinth_mesh::Mesh,
        table: &plinth_orient::OrientFaceTable,
        candidates: &[plinth_orient::UpDirection],
        fidelity: DemandFidelity,
    ) -> Vec<Result<plinth_orient::demand::MeasuredSupportDemand, String>> {
        let started = Instant::now();
        let measured = self.inner.measure_support_demand(mesh, table, candidates, fidelity);
        let slot = usize::from(fidelity == DemandFidelity::Production);
        self.spent.lock().expect("the timing lock")[slot] += started.elapsed().as_secs_f64();
        measured
    }
}

fn registered_decimation_budget() -> plinth_decimate::DecimationBudget {
    let placeholders = m0_constants().expect("registry").placeholders();
    let probe = ProbeConfig::new(
        placeholders
            .decimation_probe_grid_pitch_mm()
            .diagnostic()
            .get(),
        usize::try_from(
            placeholders
                .decimation_probe_max_grid_bytes()
                .diagnostic()
                .get(),
        )
        .map_err(|_| BudgetConfigError::InvalidGridBytes { value: 0 })
        .expect("registered grid byte count fits a usize"),
        placeholders
            .decimation_probe_nearest_upper_bound_mm()
            .diagnostic()
            .get(),
    );
    let budget = plinth_decimate::DecimationBudget::ratified_default(probe)
        .expect("the ratified probe config builds a budget");
    budget
        .validate_for_execution()
        .expect("the registered budget is executable");
    budget
}

#[test]
fn the_toed_slab_lands_flat_at_its_production_pose() {
    let raw = plinth_mesh::make_toed_slab().expect("procedural toed slab");
    let repaired = plinth_mesh::repair_mesh(&raw).into_mesh();
    let decimated = plinth_decimate::run(&repaired, registered_decimation_budget())
        .expect("decimation must run")
        .into_decimated_model()
        .expect("scale sanity must pass")
        .into_parts()
        .0;
    let request = registered_sweep_request().expect("the registered sweep request");
    let demand = TimedDemand { inner: SweepDemand::production(), spent: std::sync::Mutex::new([0.0; 2]) };
    let started = Instant::now();
    let outcome = search(&decimated, &request, Some(&demand))
        .expect("the dense orientation search must not refuse");
    let elapsed = started.elapsed().as_secs_f64();
    let [prefilter_s, production_s] = *demand.spent.lock().expect("the timing lock");
    // How much slower than the idle machine the numbers come from: the
    // production stage's code is not what these guards watch.
    let load = (production_s / A73E40A_PRODUCTION_S).max(1.0);
    let bound = SEARCH_BOUND_S * load;
    let share = prefilter_s / production_s;
    println!(
        "the dense search took {elapsed:.1} s (bound {SEARCH_BOUND_S:.0} s x load {load:.2} = {bound:.0} s): \
         prefilter {prefilter_s:.1} s, production {production_s:.1} s, share {share:.3} (bound {PREFILTER_SHARE_BOUND})"
    );
    // THE SPEED GUARDS. The search is the toed slab's whole cost, and the
    // round-13 frame bands once made it 3 times slower than before them (and
    // the process tests' slab 60 times slower): see `SEARCH_BOUND_S`.
    assert!(
        elapsed <= bound,
        "the toed slab's dense orientation search took {elapsed:.1} s, over the {bound:.0} s bound \
         (2.5 times the {A73E40A_SEARCH_S:.1} s it took before the frame bands, times the load \
         {load:.2}): a rule has made the search cost far more than it did (the round-13 \
         regression, `lanes/band-speed.md`)"
    );
    // The prefilter only ranks poses; bracing it (the round-13 regression) made
    // it a multiple of the production stage instead of a tenth.
    assert!(
        share <= PREFILTER_SHARE_BOUND,
        "the quick pass took {prefilter_s:.1} s against the production stage's {production_s:.1} s \
         (share {share:.2}, bound {PREFILTER_SHARE_BOUND}): the prefilter is running the brace \
         ladder again (Craig ruling 2026-10-04, 'skip bracing in the quick pass')"
    );
    let record = outcome.record();
    let commit = record
        .tiers
        .iter()
        .find(|tier| tier.tier == SearchTier::Commit)
        .expect("a commit tier");
    for entry in &commit.ranking {
        let up = entry.up_model_frame.map(|value| value.get());
        let terms = &entry.terms;
        println!(
            "rank {:>2} up ({:+.4},{:+.4},{:+.4}) score {:.6} support {:.6} cm2 peel {:.6} \
             height {:.3} mm scar term {:.6} fragility {:.4} down-facing {} display {:.4} \
             landing {:?} tip {:?}",
            entry.rank,
            up[0],
            up[1],
            up[2],
            terms.score.get(),
            terms.support_area_cm2.get(),
            terms.peel_term.get(),
            terms.height_mm.get(),
            terms.scar_term.get(),
            terms.fragility_raw_mm2.get(),
            terms.down_facing_faces,
            terms.display_raw_mm2.get(),
            terms.landing_scar_raw_mm2.map(|value| value.get()),
            terms.tip_scar_raw_mm2.map(|value| value.get()),
        );
    }
    let winner_up = record.winner.up_model_frame.map(|value| value.get());
    assert!(
        winner_up[2] < -0.999,
        "the toed slab committed up = ({:+.4}, {:+.4}, {:+.4}) in the model frame, not the flat \
         pose (0, 0, -1): authored top down on the supports, toes up",
        winner_up[0],
        winner_up[1],
        winner_up[2],
    );
    // A CLEAR margin, not a tie: the best toes-down entry of the commit tier
    // must trail the flat winner by more than the 0.0011 points the three-shell
    // fixture's pose turned on.
    let best_toes_down = commit
        .ranking
        .iter()
        .filter(|entry| entry.up_model_frame[2].get() > 0.0)
        .min_by(|a, b| a.terms.score.get().total_cmp(&b.terms.score.get()))
        .expect("the commit tier ranks a toes-down pose");
    let margin = best_toes_down.terms.score.get() - record.winner.terms.score.get();
    println!("flat wins by {margin:.6} points");
    assert!(
        margin > 0.01,
        "the flat pose wins by only {margin:.6} points: a tie, not a decision"
    );
    // And it wins for the physical reason: the flat pose's tips scar the model
    // less than the best toes-down pose's (MEASURED 0.0158 vs 0.8747 raw mm2).
    let tip_scar = |terms: &plinth_orient::search::TermsRecord| {
        terms
            .tip_scar_raw_mm2
            .map(|value| value.get())
            .expect("a commit-tier pose measured by the production sweep has a tip scar")
    };
    let (flat_tip, toes_tip) = (
        tip_scar(&record.winner.terms),
        tip_scar(&best_toes_down.terms),
    );
    assert!(
        flat_tip < toes_tip,
        "the flat pose won, but its tips scar {flat_tip:.4} raw mm2 against {toes_tip:.4} on its \
         toes: the margin is not the scar the module doc says carries it"
    );
}
