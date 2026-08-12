//! The repair pre-pass against the **production** closed-solid gate.
//!
//! `plinth-mesh`'s own repair tests assert against a locally-written
//! equivalent of the gate, which is fine for measuring a repair and useless as
//! a claim about production: a paraphrase that drifts would keep passing. This
//! file makes the claim that matters by calling
//! [`plinth_hollow::validate_closed_mesh`] itself — the same function
//! `create_shell` and `validate_oriented_solid` call, unchanged by this work —
//! on each of the three census fixtures, before and after repair.
//!
//! **The gate is not relaxed anywhere in this change.** These tests would fail
//! if it were: they require a refusal *before* repair as well as a pass after.

use plinth_hollow::{validate_closed_mesh, HollowError};
use plinth_mesh::{
    make_box_default, make_duplicated_face_box, make_dust_shard_box, make_pinched_boxes,
    repair_mesh,
};

#[test]
fn the_repair_prepass_rescues_each_census_defect_class_at_the_real_gate() {
    for (class, mesh) in [
        ("dust shard", make_dust_shard_box().expect("shard fixture")),
        (
            "duplicate face",
            make_duplicated_face_box().expect("duplicate fixture"),
        ),
        ("pinch vertex", make_pinched_boxes().expect("pinch fixture")),
    ] {
        assert!(
            matches!(
                validate_closed_mesh(&mesh),
                Err(HollowError::NotClosedOrOriented)
            ),
            "{class}: the fixture must be refused by the unmodified gate first"
        );
        let repaired = repair_mesh(&mesh).expect("repair");
        assert!(
            repaired.evidence().changed(),
            "{class}: something must actually have been repaired"
        );
        validate_closed_mesh(repaired.mesh()).unwrap_or_else(|error| {
            panic!(
                "{class}: repaired output must pass the unchanged gate, got {error} after {}",
                repaired.evidence().summary_line()
            )
        });
    }
}

/// A model the gate already accepts is handed straight back, so the pre-pass
/// cannot turn a passing model into a refused one.
#[test]
fn a_model_the_gate_already_accepts_is_untouched() {
    let mesh = make_box_default().expect("box");
    validate_closed_mesh(&mesh).expect("the box passes the gate to begin with");
    let repaired = repair_mesh(&mesh).expect("repair");
    assert!(!repaired.evidence().changed());
    validate_closed_mesh(repaired.mesh()).expect("and still passes it");
    assert_eq!(repaired.mesh().verts(), mesh.verts());
    assert_eq!(repaired.mesh().tris(), mesh.tris());
}
