//! The repair pre-pass against the **production** closed-solid gate.
//!
//! `plinth-mesh`'s own repair tests assert against a locally-written
//! equivalent of the gate, which is fine for measuring a repair and useless as
//! a claim about production: a paraphrase that drifts would keep passing. This
//! file makes the claim that matters by calling
//! [`plinth_hollow::validate_closed_mesh`] itself — the same function
//! `create_shell` and `validate_oriented_solid` call, unchanged by this work —
//! on the census fixtures, before and after repair.
//!
//! **The gate is not relaxed anywhere in this change.** The duplicate-face row
//! would fail if it were: it requires a refusal *before* repair as well as a
//! pass after. The pinch row proves the opposite point on purpose — per the
//! owner ruling of 2026-08-12 (FU-47), a point-touch is not a closed-solid
//! defect, so `make_pinched_boxes()` must pass the gate *unrepaired*, and the
//! edge-shared fixture (non-manifold edge plus pinch) must still be refused,
//! proving the mask took out exactly one cut class.
//!
//! **The dust-shard row moved out of the "refused before" claim for the same
//! reason.** `make_dust_shard_box()`'s only defect is a tetrahedron kissing
//! the box at one vertex — a point-touch — so under FU-47 the raw fixture
//! *already* passes the gate. The shard-strip action does not read a cut
//! flag, though: it classifies dust by its own six triage criteria, so the
//! pre-pass strips the shard regardless of whether the gate would have
//! refused it. That is proven separately below, against the real gate.

use plinth_hollow::{create_shell, validate_closed_mesh, HollowError, HollowSettings};
use plinth_mesh::{
    make_box_default, make_duplicated_face_box, make_dust_shard_box, make_edge_shared_boxes,
    make_pinched_boxes, repair_mesh,
};

#[test]
fn the_repair_prepass_rescues_the_duplicate_face_defect_at_the_real_gate() {
    let mesh = make_duplicated_face_box().expect("duplicate fixture");
    assert!(
        matches!(
            validate_closed_mesh(&mesh),
            Err(HollowError::NotClosedOrOriented)
        ),
        "the fixture must be refused by the unmodified gate first"
    );
    let repaired = repair_mesh(&mesh);
    assert!(
        repaired.evidence().changed(),
        "something must actually have been repaired"
    );
    validate_closed_mesh(repaired.mesh()).unwrap_or_else(|error| {
        panic!(
            "repaired output must pass the unchanged gate, got {error} after {}",
            repaired.evidence().summary_line()
        )
    });
}

/// The shard fixture's only defect is a point-touch, which per FU-47 already
/// passes the gate raw. The pre-pass strips it anyway — triage classifies
/// dust independent of the closed-solid gate — and the gate accepts the
/// result too, trivially, since a plain box always did.
#[test]
fn a_dust_shard_already_passes_the_real_gate_and_is_stripped_anyway() {
    let mesh = make_dust_shard_box().expect("shard fixture");
    validate_closed_mesh(&mesh).expect("a point-touch is not a closed-solid defect (FU-47)");
    let repaired = repair_mesh(&mesh);
    assert!(
        repaired.evidence().changed(),
        "triage still strips the shard on its own criteria: {}",
        repaired.evidence().summary_line()
    );
    assert_eq!(
        repaired.mesh().tris().len(),
        12,
        "the shard's four faces are gone, the box remains"
    );
    validate_closed_mesh(repaired.mesh()).expect("still clean after the strip");
}

/// A point-touch passes the gate as-is, and the pre-pass does not touch it:
/// per FU-47 it is not a defect, so there is nothing to rescue.
#[test]
fn a_pinch_vertex_passes_the_gate_unrepaired() {
    let mesh = make_pinched_boxes().expect("pinch fixture");
    validate_closed_mesh(&mesh).expect("a point-touch is not a closed-solid defect (FU-47)");
    let repaired = repair_mesh(&mesh);
    assert!(
        !repaired.evidence().changed(),
        "nothing needed repairing: {}",
        repaired.evidence().summary_line()
    );
    assert_eq!(repaired.mesh().verts(), mesh.verts());
    assert_eq!(repaired.mesh().tris(), mesh.tris());
}

/// A non-manifold edge is still refused even though it also carries
/// point-only pinches at its endpoints: the mask removed one cut class, not
/// five.
#[test]
fn an_edge_shared_pinch_is_still_refused_by_the_gate() {
    let mesh = make_edge_shared_boxes().expect("edge-shared fixture");
    assert!(
        matches!(
            validate_closed_mesh(&mesh),
            Err(HollowError::NotClosedOrOriented)
        ),
        "a non-manifold edge is still a closed-solid defect after FU-47"
    );
}

/// A model the gate already accepts is handed straight back, so the pre-pass
/// cannot turn a passing model into a refused one.
#[test]
fn a_model_the_gate_already_accepts_is_untouched() {
    let mesh = make_box_default().expect("box");
    validate_closed_mesh(&mesh).expect("the box passes the gate to begin with");
    let repaired = repair_mesh(&mesh);
    assert!(!repaired.evidence().changed());
    validate_closed_mesh(repaired.mesh()).expect("and still passes it");
    assert_eq!(repaired.mesh().verts(), mesh.verts());
    assert_eq!(repaired.mesh().tris(), mesh.tris());
}

/// **The ruling's outcome, at the production entry point rather than at the
/// helper.**
///
/// Passing [`validate_closed_mesh`] is not the same thing as being accepted:
/// `create_shell` calls it and then immediately certifies the same mesh's
/// closed components, and the certificate used to refuse **any** nonzero cut
/// flag. A point-touch model therefore passed the relaxed gate and was
/// refused one line later as `Topology(CertificateNotClosedOrOriented)` —
/// FU-47's outcome (models like this print) undelivered, under an error less
/// accurate than the one the ruling retired. The certificate now uses the
/// same `TOPOLOGY_CUT_CLOSED_SOLID_MASK` the gate does.
///
/// This also covers the solid reserve: `HollowedModel::solid` validates its
/// fallback mesh through the very same `validate_oriented_solid` this call
/// exercises, so a model refused here would have had no solid path either.
///
/// The two 10 mm cubes hollow to two ~166 mm³ cavities, and only the larger
/// of the two is a resin-flow interior (FU-47: a point never connects two
/// interiors), which is under the registered savings threshold — so the
/// expected verdict is a *savings* verdict. What must never come back is a
/// topology or closure refusal.
#[test]
fn a_point_touch_model_is_judged_on_savings_not_refused_on_topology() {
    let mesh = make_pinched_boxes().expect("pinch fixture");
    let settings = HollowSettings::from_registered_constants().expect("registered M0 settings");
    match create_shell(mesh, settings) {
        Err(HollowError::IneligibleSavings { .. }) => {}
        Ok(_) => panic!("the fixture's cavities are below the savings threshold; it must not hollow"),
        Err(error @ (HollowError::Topology(_) | HollowError::NotClosedOrOriented)) => panic!(
            "a point-touch is not a closed-solid defect (FU-47), yet H0 refused it as {error}"
        ),
        Err(other) => panic!("unexpected refusal {other}"),
    }
}
