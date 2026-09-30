//! **The Morel base lands textured top UP, at its production pose search, by
//! a margin no support-placement change can erase.**
//!
//! The owner flagged the Morel base upside down on 2026-09-03 (textured top
//! against the supports), the display-face lane fixed it, and develop
//! `0f70cf2` turned it over again: "flipped 177.3 deg". Nothing in the
//! orientation objective moved between those two runs. Placement lanes
//! (contact sizing, wall creep, head join) moved the MEASURED support terms
//! of the flipped pose — its model-landing scar fell 5.53 → 4.23 mm², its
//! burden 0.0183 → 0.0160 cm² — and that was enough, because the identity
//! pose led by 0.006 points out of 5.69
//! (`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/lanes/morel-pose.md`).
//!
//! The cause is a rule, not a margin. The display half of the scar term was
//! `area × fragility × D`: the display field entered as a `(1 + D)`
//! multiplier on fragility, so the textured top — large, sturdy, low
//! fragility — was charged 6.685 mm² of display for turning face down, which
//! is less than the 21.3 mm² of fragility the flip saves on the base's
//! underside. The rule now (`plinth-orient/src/score.rs`, and the two contact
//! scars in `model_landing.rs` / `tip_landing.rs`): the two harms ADD,
//! `area × (fragility + D)`. A support spoils the look of a display face
//! however sturdy the face is.
//!
//! # What "a clear margin" means here
//!
//! Stated as a physical property, not a number: the committed pose must beat
//! EVERY textured-top-down direction of the grid even when that direction's
//! supports cost NOTHING — its support term, model-landing scar and
//! tip-contact scar all set to zero, leaving its peel, height and
//! down-facing scar, which are properties of the mesh and the pose alone.
//! The support terms are the only ones a placement lane can move, so if the
//! committed pose wins against free supports, no placement change can turn
//! the base over again (it would have to make the flipped pose's supports
//! cost less than nothing). Under the old multiplicative rule this bound
//! failed on develop `0f70cf2`; the flip is also dropped by the proxy tiers
//! now, so it never reaches the production sweep at all.
//!
//! # Why this test is `#[ignore]`d
//!
//! The Morel base STL is a local, uncommitted mini
//! (`minis-to-process/Morel base.stl`, the same file
//! `plinth-decimate/tests/it/local_minis_repair.rs` reads). Set
//! `PLINTH_MINIS_DIR` to point elsewhere. The gate-profile guard of the rule
//! itself is `plinth-orient`'s
//! `display_face_objective::a_display_face_costs_the_same_to_turn_down_however_sturdy_it_is`.

use std::path::PathBuf;

use plinth_core::m0_constants;
use plinth_decimate::{BudgetConfigError, ProbeConfig};
use plinth_mesh::{WeldPolicy, load_binary_stl};
use plinth_orient::search::{EXACT_LOD, SearchTier, search};
use plinth_orient::{
    CandidateOutcome, CandidateTerms, ObjectiveWeights, OrientFaceTable, OrientSearchLimits,
    UpDirection, registered_sweep_request, score_candidates,
};
use plinth_sweep::SweepDemand;

fn morel_path() -> PathBuf {
    let directory = std::env::var_os("PLINTH_MINIS_DIR").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join("minis-to-process")
        },
        PathBuf::from,
    );
    directory.join("Morel base.stl")
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
#[ignore = "reads the local, uncommitted minis-to-process/Morel base.stl"]
fn the_morel_base_lands_textured_top_up_by_more_than_its_supports_can_move() {
    let path = morel_path();
    let raw = load_binary_stl(&path, WeldPolicy::None)
        .unwrap_or_else(|error| panic!("MorelMissing: {}: {error}", path.display()));
    // The production chain, in the order `prepare.rs` runs it: repair, the
    // registered decimation, then the dense search with the production
    // sweep measuring demand at the commit tier.
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

    let winner_up = record.winner.up_model_frame.map(|value| value.get());
    let commit = record
        .tiers
        .iter()
        .find(|tier| tier.tier == SearchTier::Commit)
        .expect("a commit tier");
    for entry in &commit.ranking {
        let up = entry.up_model_frame.map(|value| value.get());
        let terms = &entry.terms;
        println!(
            "rank {:>2} up ({:+.4},{:+.4},{:+.4}) score {:.5} support {:.5} cm2 fragility {:.3} \
             display {:.3} landing {:?} tip {:?}",
            entry.rank,
            up[0],
            up[1],
            up[2],
            terms.score.get(),
            terms.support_area_cm2.get(),
            terms.fragility_raw_mm2.get(),
            terms.display_raw_mm2.get(),
            terms.landing_scar_raw_mm2.map(|value| value.get()),
            terms.tip_scar_raw_mm2.map(|value| value.get()),
        );
    }

    // (1) Textured top UP: the authored +Z is the build-up side.
    assert!(
        winner_up[2] > 0.0,
        "the Morel base committed up = ({:+.4}, {:+.4}, {:+.4}) in the model frame: its authored \
         top, the textured display face, points DOWN into the supports",
        winner_up[0],
        winner_up[1],
        winner_up[2],
    );

    // (2) FREE SUPPORTS: every textured-top-down direction of the grid
    // (every up with a negative authored z, as the coarse tier ranked them
    // all), scored at the EXACT LOD the commit tier scores at, with its
    // support term and its contact scars set to ZERO, still scores worse than
    // the committed winner. Its peel, height and down-facing scar are
    // properties of the mesh and the pose; only the support terms depend on
    // placement.
    let weights = ObjectiveWeights::registered().expect("registered objective weights");
    let table = OrientFaceTable::build(&decimated, EXACT_LOD).expect("the exact-LOD table");
    let winner_score = record.winner.terms.score.get();

    // The table is the one the commit tier scored with: the winner's fold
    // halves must be the same bits.
    let winner_direction = UpDirection::new(winner_up).expect("the winner is a unit direction");
    let rescored_winner = scored_terms(&table, &[winner_direction], weights);
    assert_eq!(
        (
            rescored_winner[0].fragility_raw_mm2.to_bits(),
            rescored_winner[0].display_raw_mm2.to_bits()
        ),
        (
            record.winner.terms.fragility_raw_mm2.get().to_bits(),
            record.winner.terms.display_raw_mm2.get().to_bits()
        ),
        "the rebuilt exact-LOD table does not reproduce the winner's recorded fold; the bound \
         below would be measured on a different table"
    );

    let coarse = record
        .tiers
        .iter()
        .find(|tier| tier.tier == SearchTier::Coarse)
        .expect("a coarse tier");
    let flipped: Vec<UpDirection> = coarse
        .ranking
        .iter()
        .map(|entry| entry.up_model_frame.map(|value| value.get()))
        .filter(|up| up[2] < 0.0)
        .map(|up| UpDirection::new(up).expect("a grid direction is a unit direction"))
        .collect();
    assert!(
        flipped.len() > 100,
        "only {} textured-top-down directions were ranked; the grid is not what this test assumes",
        flipped.len()
    );
    let terms = scored_terms(&table, &flipped, weights);
    let (closest, bound) = flipped
        .iter()
        .zip(&terms)
        .map(|(up, terms)| {
            (
                *up,
                terms.score - weights.support_area_cm2() * terms.support_area_cm2,
            )
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("at least one flipped direction");
    let margin = bound - winner_score;
    println!(
        "{} textured-top-down directions; the best of them with FREE supports scores {bound:.5} \
         at {:?} against the winner's {winner_score:.5}: margin {margin:.5}",
        flipped.len(),
        closest.get()
    );
    assert!(
        margin > 0.0,
        "with its supports costing nothing, the textured-top-down direction {:?} scores {bound:.5}, \
         {:.5} BELOW the committed {winner_score:.5}: the pose still hangs on what the supports \
         cost, so a placement change can turn the base over again",
        closest.get(),
        -margin,
    );
}

fn scored_terms(
    table: &OrientFaceTable,
    candidates: &[UpDirection],
    weights: ObjectiveWeights,
) -> Vec<CandidateTerms> {
    score_candidates(table, candidates, weights, OrientSearchLimits::production())
        .expect("scoring must not refuse")
        .outcomes()
        .iter()
        .map(|outcome| match outcome {
            CandidateOutcome::Scored(terms) => *terms,
            CandidateOutcome::Refused(refusal) => panic!("candidate refused: {refusal:?}"),
        })
        .collect()
}
