//! Pose-search diagnostics (pose-stability design, Lane 0) through the public
//! API, on the `Round_*` disc: the one fixture family whose flat faces make
//! the G4 escort fire. The unit tests in `src/diag.rs` cover the boundaries
//! and the sabotage; this file checks that the diagnostics describe the same
//! funnel the record does, and change nothing in it.

use plinth_mesh::make_recessed_disc;
use plinth_orient::diag::DiagnosticsConfig;
use plinth_orient::grid::CandidateKey;
use plinth_orient::search::{search, search_with_diagnostics, SweepRequest};

fn disc() -> plinth_mesh::Mesh {
    make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128).expect("the Round 50 profile")
}

#[test]
fn the_diagnostics_leave_the_search_record_and_digest_alone() {
    let mesh = disc();
    let request = SweepRequest::registered().expect("the registered sweep configuration");
    let plain = search(&mesh, &request, None).expect("a disc is scorable");
    let (with, _) =
        search_with_diagnostics(&mesh, &request, None, &DiagnosticsConfig::default()).expect("a disc is scorable");
    assert_eq!(with.record(), plain.record());
    assert_eq!(with.record().search_digest, plain.record().search_digest);
    assert_eq!(with.winner(), plain.winner());
}

#[test]
fn a_disc_s_flat_face_escort_is_explained_by_its_g4_rows() {
    let mesh = disc();
    let request = SweepRequest::registered().expect("the registered sweep configuration");
    let (outcome, diagnostics) =
        search_with_diagnostics(&mesh, &request, None, &DiagnosticsConfig::default()).expect("a disc is scorable");
    let record = outcome.record();

    assert!(diagnostics.reproduces_grid, "{:?}", diagnostics.notes);
    assert_eq!(record.grid.flat_faces, 2);
    assert_eq!(diagnostics.g4.flat_poses, record.grid.flat_faces);
    assert_eq!(diagnostics.g4.escorted_ids.len(), record.grid.flat_face_poses);

    // Both flat poses ride to the commit tier, and the escort log names why.
    let commit_ids: Vec<usize> = record.tiers[2].ranking.iter().map(|row| row.candidate).collect();
    for id in &diagnostics.g4.escorted_ids {
        assert!(commit_ids.contains(id), "flat pose {id} did not reach the commit tier");
        assert!(
            diagnostics
                .escort
                .by_right
                .iter()
                .any(|row| row.source == "flat_face" && row.id == *id),
            "no escort row for flat pose {id}"
        );
    }

    // Each flat pose is covered by a candidate inside the dedupe radius, and
    // the cover names a key (the id would move if an earlier direction did).
    for cover in &diagnostics.g4.covers {
        let covering = cover.covering.as_ref().expect("a flat pose is covered");
        assert!(covering.dot_minus_cosine > 0.0);
        assert!(matches!(
            covering.key,
            CandidateKey::Legacy(_) | CandidateKey::Rest(_) | CandidateKey::Fib(_)
        ));
        assert_eq!(covering.key, diagnostics.candidates[covering.id].key);
    }

    // The two flat poses are each far from the flat-share boundary.
    let flat: Vec<_> = diagnostics.g4.poses.iter().filter(|pose| pose.is_flat).collect();
    assert_eq!(flat.len(), 2);
    assert!(flat.iter().all(|pose| pose.margin_rel > 0.5), "{flat:?}");
}
