//! **The toed slab should land flat (authored top down, toes up) at its
//! production pose search. MEASURED RED, and kept as the red test for whoever
//! takes the decision this measurement hands back.**
//!
//! At X 18 the slab stands on its toes, which is worse on scar, removal, print
//! risk and resin (`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/
//! creep-arms-diagnosis.md` Q2). That diagnosis traced the 0.015 % tie to the
//! fixture: three overlapping shells whose buried toe caps charged the flipped
//! pose 1.919 raw mm2 of down-facing area. The toed-slab-fixture lane
//! (2026-09-30) made the fixture one closed solid
//! (`plinth-mesh` `fixtures::toed_slab_is_one_closed_solid_with_no_buried_toe_caps`)
//! and ran this test, the production chain in `prepare.rs`'s order (repair,
//! registered decimation, dense search with the production sweep demand):
//!
//! | fixture | on its toes (0,0,+1) | flat (0,0,-1) | flat - toes |
//! |---|---|---|---|
//! | three shells (old) | 7.191398, fragility fold 135.000 | 7.192495, fold 136.919 | +0.001096 |
//! | one solid (now) | 6.975059, fold 81.059 | 7.184816, fold 135.000 | +0.209757 |
//!
//! The union removed the flipped pose's 1.919 as the diagnosis said, but it
//! ALSO cut the toes-down pose's fold by 53.94 raw mm2, so the slab now stands
//! on its toes by a clear margin. That 53.94 is not a physical change: the
//! fragility of a face is 0.6 x the mean angle defect of its three corners
//! (`delicacy_eval.rs`), so each of the old underside's two 450 mm2 triangles,
//! all three corners cube corners (defect 0.25), was charged 0.15 / mm2 over
//! its whole flat area (900 x 0.15 = 135.000 exactly). The flat top still is
//! two such triangles, so the flipped pose still pays 135.000. The unioned
//! underside is 70 triangles, each with ONE or two cube corners and the rest
//! toe-ring vertices of smaller defect, and pays 81.059 (0.094 / mm2). The
//! fold reads the TRIANGULATION of a flat face, not its shape.
//!
//! The lane brief forbids changing the orientation objective, so this test is
//! ignored rather than made to pass. Un-ignore it when the objective (or the
//! delicacy field) stops charging a vertex's curvature over a whole face's
//! area, or when the owner rules otherwise.

use plinth_core::m0_constants;
use plinth_decimate::{BudgetConfigError, ProbeConfig};
use plinth_orient::registered_sweep_request;
use plinth_orient::search::{SearchTier, search};
use plinth_sweep::SweepDemand;

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
#[ignore = "MEASURED RED 2026-09-30: the unioned toed slab commits on its toes by 0.2098 points, \
            because the scar fold charges a corner's curvature over a whole face's area; the lane \
            may not change the objective (see the module doc)"]
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
    let demand = SweepDemand::production();
    let outcome = search(&decimated, &request, Some(&demand))
        .expect("the dense orientation search must not refuse");
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
        .map(|entry| entry.terms.score.get())
        .min_by(f64::total_cmp)
        .expect("the commit tier ranks a toes-down pose");
    let margin = best_toes_down - record.winner.terms.score.get();
    println!("flat wins by {margin:.6} points");
    assert!(
        margin > 0.01,
        "the flat pose wins by only {margin:.6} points: a tie, not a decision"
    );
}
