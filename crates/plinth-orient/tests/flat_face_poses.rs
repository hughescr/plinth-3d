//! **A flat-faced part is measured BOTH ways up before the search commits.**
//!
//! Owner ruling 2026-09-28 (Round 50 diagnosis, `plinth-evidence/slice-sweep-
//! 2026-09-02/field-2026-09-28/round50.md`): all three `Round_*` bases are
//! authored glue face DOWN, and the correct print pose is the 180° flip,
//! glue face UP. On the 50 and 100 mm discs that flip was never measured: the
//! coarse proxy ranked it 76th of 646 and 482nd of 644 and promoted only the
//! top 24, which were all ~45° tilts exploiting the step in the proxy's
//! support term. The legacy escort carries identity in by right and nothing
//! carried the flip.
//!
//! The rule under test: **the poses that lay a part's flat face horizontal —
//! that face against the plate, and that face on top — reach the commit tier
//! by right**, exactly as the legacy 13 do. A proxy may rank them anywhere; it
//! may not discard them.
//!
//! The fixture is the parametric `Round_*` member (`make_recessed_disc`,
//! authored plain face down, recess up), with no demand measurer, so this is
//! the funnel alone and runs in seconds. `round_bases_ratchet.rs` in
//! `plinth-sweep` is the same question asked end to end, with the production
//! measurer and the real placement sweep.

use plinth_mesh::{make_recessed_disc, Mesh};
use plinth_orient::grid::GridSettings;
use plinth_orient::search::{search, SearchTier, SweepRequest};

fn request() -> SweepRequest {
    SweepRequest::registered().expect("the registered sweep configuration")
}

/// `(name, mesh)` for the two discs whose flip the recorded funnel dropped.
/// Dimensions are `round_bases_ratchet.rs`'s own, so the two files grade one
/// family.
fn discs() -> Vec<(&'static str, Mesh)> {
    vec![
        (
            "Round 50",
            make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128).expect("the Round 50 profile"),
        ),
        (
            "Round 100",
            make_recessed_disc(49.0, 50.0, 3.5, 2.5, 47.5, 128).expect("the Round 100 profile"),
        ),
    ]
}

/// The commit tier's ranking contains a candidate within the search's OWN
/// dedupe tolerance of `target` — the tolerance at which the search itself
/// calls two directions one candidate, so nothing is invented here.
fn commit_tier_holds(record: &plinth_orient::search::DenseSweepRecord, target: [f64; 3]) -> bool {
    let cosine = GridSettings::registered()
        .expect("registered grid settings")
        .dedupe_cosine();
    let commit = record
        .tiers
        .iter()
        .find(|tier| tier.tier == SearchTier::Commit)
        .expect("the commit tier ran");
    commit.ranking.iter().any(|entry| {
        let u = entry.up_model_frame;
        u[0].get() * target[0] + u[1].get() * target[1] + u[2].get() * target[2] > cosine
    })
}

#[test]
fn a_flat_faced_disc_reaches_the_commit_tier_both_ways_up() {
    for (name, mesh) in discs() {
        let outcome = search(&mesh, &request(), None).expect("a disc is scorable");
        let record = outcome.record();
        let commit = &record.tiers[2];
        println!(
            "{name}: commit tier scored {} (promoted {} + escorted {}), winner {:?}",
            commit.scored,
            record.tiers[1].promoted,
            commit.escorted,
            record.winner.up_model_frame,
        );
        // Path-hit witness: the grid named both sides as flat faces, and the
        // escort carried exactly one pose beyond the legacy 13 (the flip;
        // identity is legacy candidate 0 already).
        println!(
            "{name}: grid flat faces {}, flat-face poses {}, legacy {}",
            record.grid.flat_faces, record.grid.flat_face_poses, record.grid.legacy
        );
        assert_eq!(record.grid.flat_faces, 2, "{name}: a disc has one flat face per side");
        assert_eq!(record.grid.flat_face_poses, 2, "{name}: both resting poses are named");
        assert!(
            commit.escorted <= record.grid.legacy + 1,
            "{name}: the flat faces may add at most the flip to the escort"
        );
        assert!(
            commit_tier_holds(record, [0.0, 0.0, 1.0]),
            "{name}: identity (plain face DOWN on the plate) did not reach the commit tier"
        );
        assert!(
            commit_tier_holds(record, [0.0, 0.0, -1.0]),
            "{name}: the 180 deg flip (plain glue face UP) did not reach the commit tier; a proxy \
             discarded the pose that lays the part's flat face horizontal"
        );
    }
}

/// **A part with no flat face gets no escort beyond the legacy 13.** The
/// committed `Sphere` baseline: every rest-pose bin is a small cap of a
/// curved surface, so no bin's within-tolerance area outweighs the rest of
/// its side, the grid names no flat face, and the commit tier is exactly the
/// one the recorded funnel built before this rule existed.
#[test]
fn a_sphere_has_no_flat_face_and_no_extra_escort() {
    let mesh = plinth_mesh::load_binary_stl(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/baseline/Sphere.prepped.stl"),
        plinth_mesh::WeldPolicy::None,
    )
    .expect("the committed Sphere baseline loads");
    let outcome = search(&mesh, &request(), None).expect("the sphere is scorable");
    let record = outcome.record();
    assert_eq!(record.grid.flat_faces, 0, "a sphere has no flat face");
    assert_eq!(record.grid.flat_face_poses, 0);
    assert!(record.tiers[2].escorted <= record.grid.legacy);
}
