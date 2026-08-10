//! The dense sweep, end to end on real geometry.
//!
//! What is asserted here is the part of the design that is a *contract* rather
//! than a number: identity wins ties (T8), the sweep is never worse than the
//! legacy 13 it contains (T5), the grid does not move silently (T12), a
//! degraded proxy is named rather than substituted silently (T10), the record
//! is reproducible bit for bit (T7's single-process half), and a searched
//! winner's recorded transform is the rotation its direction defines (T13's
//! transform half).

use plinth_mesh::{load_binary_stl, make_box, make_box_default, Mesh, WeldPolicy};
use plinth_orient::grid::{
    direction_set_digest, fibonacci_directions, CandidateBlock, GridSettings, SweepGrid,
};
use plinth_orient::search::{search, ProxyFallbackReason, SearchTier, SweepRequest, EXACT_LOD};
use plinth_orient::{
    score_candidates, BaselineSelection, CandidateOutcome, ObjectiveWeights, OrientFaceTable,
    OrientSearchLimits,
};
use std::path::{Path, PathBuf};

fn baseline(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/baseline")
        .join(name)
}

fn committed(name: &str) -> Mesh {
    load_binary_stl(baseline(name), WeldPolicy::None)
        .unwrap_or_else(|error| panic!("committed fixture {name} must load: {error}"))
}

fn request() -> SweepRequest {
    SweepRequest::registered().expect("the registered sweep configuration")
}

/// **T8 — the tie-break, on a shape whose ties are exact.**
///
/// A cube has six face-down poses that agree in every term to the last bit:
/// same down-facing area, same section at every height, same height, same
/// delicacy. Six candidates attain the minimum, and exactly one of them is
/// authored-up. It must win, because the legacy writer seeded `best` with
/// `np.eye(4)` and two committed baselines (`Cylinder`, `make_box`) record the
/// identity outcome that seed produced.
///
/// This is what makes identity-as-`CandidateId`-0 load-bearing rather than
/// tidy: change the enumeration order and this test fails.
#[test]
fn a_cube_ties_six_ways_and_identity_wins() {
    let mesh = make_box_default().expect("cube fixture");
    let outcome = search(&mesh, &request()).expect("a cube is scorable");

    assert_eq!(outcome.winner().id, 0, "identity is CandidateId 0");
    assert_eq!(outcome.winner().selection, BaselineSelection::Identity);
    assert_eq!(outcome.winner().block, CandidateBlock::Legacy);

    // The tie is real, not assumed: the runner-up scores the same to the bit
    // and lost only on id.
    let record = outcome.record();
    let runner_up = record.runner_up.as_ref().expect("a cube has a runner-up");
    assert_eq!(
        runner_up.terms.score.get().to_bits(),
        record.winner.terms.score.get().to_bits(),
        "a cube's face-down poses must tie exactly, or this test is proving something else"
    );
    assert!(
        runner_up.candidate > record.winner.candidate,
        "the tie must have been broken toward the lower id"
    );
    // And the record says so rather than claiming a resolved decision.
    assert!(!record.fidelity.decision_resolved);
}

/// **T5 — never worse than the search it replaces.**
///
/// D-12 keeps the legacy 13 permanently "so the sweep is never worse by
/// construction". That claim rests on two structural facts — the 13 are always
/// enumerated, and dedupe keeps the earlier index so they cannot be displaced —
/// and this is the measurement that the facts hold: the committed winner's
/// exact score is no worse than the best of the 13, scored on the same exact
/// table.
#[test]
fn the_dense_winner_is_never_worse_than_the_best_legacy_candidate() {
    for name in [
        "make_toed_slab.prepped.stl",
        "make_overhang_table.prepped.stl",
        "Sphere.prepped.stl",
    ] {
        let mesh = committed(name);
        let outcome = search(&mesh, &request()).expect("a committed fixture is scorable");

        let table = OrientFaceTable::build(&mesh, EXACT_LOD).expect("exact table");
        let weights = ObjectiveWeights::registered().expect("registered objective");
        let grid = SweepGrid::build(&table, GridSettings::registered().unwrap())
            .expect("the grid builds on the exact table too");
        let legacy: Vec<_> = grid
            .candidates()
            .iter()
            .filter(|candidate| candidate.block == CandidateBlock::Legacy)
            .map(|candidate| candidate.direction)
            .collect();
        assert_eq!(legacy.len(), 13);
        let limits = OrientSearchLimits::production(table.face_count(), table.vertex_count());
        let scored = score_candidates(&table, &legacy, weights, limits).expect("legacy scoring");
        let best = scored
            .outcomes()
            .iter()
            .filter_map(|outcome| match outcome {
                CandidateOutcome::Scored(terms) => Some(terms.score),
                CandidateOutcome::Refused(_) => None,
            })
            .fold(f64::INFINITY, f64::min);

        assert!(
            outcome.record().winner.terms.score.get() <= best,
            "{name}: the dense winner scored {} against the legacy best {best}",
            outcome.record().winner.terms.score.get()
        );
    }
}

/// **The escort, on the model that needed it.**
///
/// The committed `Sphere` baseline records X −20 as the legacy search's answer.
/// The dense sweep agrees — and could not have, without the legacy block being
/// escorted to the exact tier: the L1 proxy ranks that pose **478th of 643**,
/// so the refine tier never sees it and it appears in no proxy ranking at all.
/// Unescorted, the funnel committed to a candidate scoring 16.3173 where this
/// one scores 16.3009.
///
/// This is what a fidelity ledger is for. `refine_rank_of_final_winner` is
/// `None` rather than a sentinel, which is the record saying outright that the
/// answer it committed to was one no proxy ever promoted.
#[test]
fn the_sphere_commits_to_the_pose_its_proxies_ranked_478th() {
    let mesh = committed("Sphere.prepped.stl");
    let outcome = search(&mesh, &request()).expect("the sphere is scorable");
    let record = outcome.record();

    assert_eq!(
        record.winner.selection,
        BaselineSelection::AxisDegrees {
            axis: plinth_orient::BaselineAxis::X,
            degrees: plinth_core::Finite::new(-20.0).unwrap(),
        },
        "the dense sweep must reach the same pose the committed Sphere baseline records"
    );
    assert_eq!(record.winner.block, CandidateBlock::Legacy);
    assert_eq!(
        record.fidelity.refine_rank_of_final_winner, None,
        "the winner reached the exact tier by escort, not by promotion"
    );
    assert!(
        record
            .fidelity
            .coarse_rank_of_final_winner
            .is_some_and(|rank| rank > request().ladder.refine_top_k()),
        "the coarse proxy must really have ranked it out of promotion range: {:?}",
        record.fidelity.coarse_rank_of_final_winner
    );
    assert!(!record.fidelity.proxy_agreed_with_exact);
    assert!(record.tiers[2].escorted > 0);
}

/// **T12 — the grid is frozen.**
///
/// The 512-point lattice is a pure function of one integer, so this golden
/// fails the moment the expression order, the golden-angle constant or the `z`
/// formula moves — without reference to any mesh. The per-model candidate-set
/// digest is asserted separately, inside the record, where the dedupe and the
/// rest poses join in.
#[test]
fn the_fibonacci_lattice_is_a_frozen_golden() {
    let directions = fibonacci_directions(512).expect("the 512-point lattice");
    assert_eq!(
        direction_set_digest(&directions),
        "f10c343324d390bd3f72074484270d67db57f401991e7e8320b86fd292bce051"
    );
}

/// **T10 — no silent substitution.**
///
/// A 12-face box is far below the 256-face search-proxy floor, so both proxy
/// rungs are refused — the simplifier gets there first and returns no triangles
/// at all at a 10 % target. Which of the two reasons fires is not the point;
/// what must not happen is what the retired heuristic did, which was to quietly
/// score the full mesh and say nothing. Every tier here reports the exact LOD by
/// name, and both fallbacks are in the record with a stated cause.
#[test]
fn a_model_below_the_proxy_floor_names_its_fallbacks() {
    let mesh = make_box([4.0, 4.0, 4.0], [0.0, 0.0, 0.0]).expect("a small box");
    let outcome = search(&mesh, &request()).expect("a small box is still scorable");
    let record = outcome.record();

    assert_eq!(
        record.fallbacks.len(),
        2,
        "both proxy rungs must be refused"
    );
    for fallback in &record.fallbacks {
        assert!(
            matches!(
                fallback.reason,
                ProxyFallbackReason::BelowFaceFloor { .. } | ProxyFallbackReason::SimplifyRefused
            ),
            "a proxy this small must be refused for a proxy reason: {fallback:?}"
        );
        assert_eq!(fallback.used_lod, EXACT_LOD);
        assert!(!fallback.detail.is_empty(), "a fallback must say why");
    }
    for tier in &record.tiers {
        assert_eq!(tier.lod, EXACT_LOD, "every tier fell back to the exact LOD");
    }
    assert_eq!(record.lods.len(), 1, "no proxy LOD was admitted");
}

/// **T7's single-process half — the record is a pure function of the mesh.**
///
/// Two searches over the same mesh in the same process produce the same
/// `search_digest`, and the digest covers everything: grid, ladder, every
/// tier's full ranking, the fidelity ledger and the work counters. Nothing here
/// reads a clock or a seed, so this is a determinism claim rather than a
/// coincidence. The cross-thread-count half lives in `orient_thread_sweep.rs`.
#[test]
fn two_searches_over_one_mesh_agree_bit_for_bit() {
    let mesh = committed("make_toed_slab.prepped.stl");
    let first = search(&mesh, &request()).expect("first search");
    let second = search(&mesh, &request()).expect("second search");
    assert_eq!(first.record().search_digest, second.record().search_digest);
    assert_eq!(first.record(), second.record());
    assert_eq!(first.record().search_digest.len(), 64);
}

/// **The funnel is the recorded one, and it narrows.**
#[test]
fn the_three_tiers_promote_the_recorded_widths() {
    let mesh = committed("Sphere.prepped.stl");
    let outcome = search(&mesh, &request()).expect("the sphere is scorable");
    let record = outcome.record();
    let request = request();

    assert_eq!(record.tiers.len(), 3);
    assert_eq!(record.tiers[0].tier, SearchTier::Coarse);
    assert_eq!(record.tiers[1].tier, SearchTier::Refine);
    assert_eq!(record.tiers[2].tier, SearchTier::Commit);

    assert_eq!(record.tiers[0].scored, record.grid.candidates);
    assert!(record.tiers[0].promoted <= request.ladder.refine_top_k());
    assert_eq!(record.tiers[1].scored, record.tiers[0].promoted);
    assert!(record.tiers[1].promoted <= request.ladder.commit_top_k());
    // The commit tier scores the promoted set plus the escorted legacy block,
    // which is what makes "never worse" hold at the tier that decides.
    assert_eq!(
        record.tiers[2].scored,
        record.tiers[1].promoted + record.tiers[2].escorted
    );
    assert!(record.tiers[2].escorted <= record.grid.legacy);

    // Commitment happens at the production mesh and only there.
    assert_eq!(record.tiers[2].lod, EXACT_LOD);
    assert_eq!(record.tiers[2].fidelity, plinth_core::Fidelity::Exact);
    assert_eq!(
        record.model_fingerprint,
        record
            .lods
            .iter()
            .find(|lod| lod.lod == EXACT_LOD)
            .expect("the exact LOD is recorded")
            .mesh_fingerprint
    );

    // The grid the record describes is the grid that was searched.
    assert_eq!(record.grid.legacy, 13);
    assert_eq!(
        record.grid.candidates,
        record.grid.legacy + record.grid.rest_poses + record.grid.fibonacci
    );
    assert_eq!(record.grid.z_spin_deg.get(), 0.0);
}

/// **T13's transform half — a searched winner is a rotation anyone can replay.**
///
/// The report's transform must be exactly the rotation the winning direction
/// defines, because that is what makes the winner pinnable: row 2 of the
/// rotation is `u` by construction, so `(R·v)_z = v·u` and a later run that
/// replays `up_model_frame` lands on the same frame.
#[test]
fn the_recorded_transform_is_the_rotation_the_winning_direction_defines() {
    let mesh = committed("make_raking_blade.prepped.stl");
    let outcome = search(&mesh, &request()).expect("the raking blade is scorable");
    let record = outcome.record();
    let direction = outcome.winner().direction;

    let rotation = direction.rotation_to_build_frame();
    for (component, recorded) in rotation[2].iter().zip(record.winner.up_model_frame) {
        assert_eq!(
            component.to_bits(),
            recorded.get().to_bits(),
            "row 2 of the winning rotation is the winning direction, by construction"
        );
    }

    // And the selection round-trips: whatever kind it is, it names the same
    // direction the record's winner names.
    match record.winner.selection {
        BaselineSelection::Identity => {
            assert_eq!(direction.get(), [0.0, 0.0, 1.0]);
        }
        BaselineSelection::AxisDegrees { .. } => {
            assert_eq!(record.winner.block, CandidateBlock::Legacy);
        }
        BaselineSelection::UpDirection { up_model_frame } => {
            assert_eq!(
                up_model_frame.map(|value| value.get()),
                record.winner.up_model_frame.map(|value| value.get())
            );
        }
    }
}
