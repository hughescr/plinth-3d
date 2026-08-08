//! Export design phase E2: the first end-to-end composition.
//!
//! Until this file existed, no code path anywhere in the workspace ran
//! `run_support` -> `FinalSupportAuthority::into_supported_model` ->
//! `plinth_emit::write` on a real model. Every stage was reachable and every
//! stage was tested; the arrows between the last three had never been joined,
//! so a `.chitubox` from the native engine's own sealed plan had never been
//! produced. This test joins them and then judges the file it wrote by
//! re-reading it, which is `chitubox_io`'s standing discipline: a write is not
//! done until a fresh parse of the bytes on disk agrees.
//!
//! Deliberately absent: any new frozen digest. Support-gate work runs
//! concurrently with the export track and legitimately moves member counts;
//! digest authority stays with `plinth-support`'s own integration goldens
//! (design record OQ-3). Everything asserted here is a completion, structural
//! or self-consistency property that a changed member count cannot break.
//!
//! The composition itself is shared verbatim with
//! `examples/first_artifact.rs`, which writes the owner's GUI-acceptance
//! artifact; the test and the artifact therefore cannot drift apart.

#[path = "../examples/shared/first_artifact_chain.rs"]
mod chain;

use std::path::PathBuf;

use chitubox_io::SegmentType;
use plinth_hollow::{HollowDisposition, SolidDecisionReason};
use plinth_mesh::make_overhang_table;

/// A per-test scratch path that is removed on the way in and on the way out.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-first-artifact-{label}-{}.chitubox",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The corpus STLs are provisioned inputs, not source, so they are gitignored
/// and fetched by the M0 corpus step. Follow the precedent already set by
/// `plinth-decimate`'s `corpus_budgets` gate: name the missing path in the
/// failure rather than skipping silently.
fn require_corpus_stl(name: &str) -> PathBuf {
    let path = chain::corpus_stl(name);
    assert!(
        path.is_file(),
        "RequiredCorpusModelMissing: required corpus model {} is missing; \
         provision corpus/models per the M0 corpus step",
        path.display()
    );
    path
}

/// Assert every property of the written artifact that does not depend on how
/// many members the support gate happened to keep.
fn assert_artifact_is_sound(outcome: &chain::ChainOutcome, audit: &chain::DiskAudit) {
    let model = &outcome.model;

    // The composition reached the end: a sealed run produced primitives, and
    // every primitive became one ChituBox record.
    assert!(
        outcome.primitive_count > 0,
        "{model}: the sealed run lowered zero primitives, so nothing was emitted"
    );
    assert_eq!(
        outcome.emit.support_record_count, outcome.primitive_count,
        "{model}: emit dropped or invented support records"
    );
    assert_eq!(
        audit.support_record_count, outcome.primitive_count,
        "{model}: the reopened file disagrees with the emitted record count"
    );

    // The reopened file is the one this crate claims to write.
    assert_eq!(audit.model_name, chain::EMITTED_MODEL_NAME);
    assert_eq!(
        audit.model_triangle_count, outcome.prepared_faces,
        "{model}: the reopened model mesh lost triangles"
    );
    assert_eq!(
        audit.file_bytes, outcome.emit.artifact_bytes,
        "{model}: the emit report's byte count disagrees with the file on disk"
    );

    // Winding, re-derived from the binary32 vertices on disk rather than
    // trusted from the producer. `plinth-emit` winds every synthesised mesh
    // outward by signed volume; a closed outward-wound soup has strictly
    // positive signed volume, and that is translation invariant, so the
    // bbox-centring emit applies cannot flatter the result.
    assert_eq!(
        audit.outward_wound_records,
        audit.support_record_count,
        "{model}: {} of {} support meshes on disk are inward-wound (min signed volume {} mm^3)",
        audit.support_record_count - audit.outward_wound_records,
        audit.support_record_count,
        audit.min_record_signed_volume_mm3,
    );
    assert!(
        audit.min_record_signed_volume_mm3 > 0.0,
        "{model}: a support mesh on disk encloses no volume"
    );
    assert!(
        audit.model_signed_volume_mm3 > 0.0,
        "{model}: the model mesh on disk is inward-wound"
    );

    // Every record carries one of the six frozen segment types.
    for segment_type in audit.records_by_segment_type.keys() {
        assert!(
            [
                SegmentType::TIP.0,
                SegmentType::TAPER.0,
                SegmentType::SHAFT.0,
                SegmentType::JOINER.0,
                SegmentType::FLARE.0,
                SegmentType::PAD_OR_BRIDGE.0,
            ]
            .contains(segment_type),
            "{model}: unknown wire segment type {segment_type} reached disk"
        );
    }

    // E1's native preview fallback is present. ChituBox refused a from-scratch
    // project without it (SPEC-CHITUBOX-RS R1), so an artifact intended for the
    // GUI gate that lacks it is not worth handing over.
    assert_eq!(
        audit.preview_bytes, 26_996,
        "{model}: preview span is {} bytes, not the native fallback's 26,996",
        audit.preview_bytes
    );
    assert!(
        audit.preview_is_native_fallback,
        "{model}: the embedded preview is not the pinned native asset"
    );

    // The independent format gate: fresh read, parse, reserialize, byte-equal.
    assert!(
        audit.format_gate_passed,
        "{model}: the plinth-verify format gate refused the artifact"
    );
}

/// The corpus Sphere is the model this phase exists for: it seals natively and
/// it is the one model with a Python-era baseline artifact to compare against.
#[test]
fn corpus_sphere_seals_lowers_and_emits_a_reparseable_chitubox() {
    require_corpus_stl("Sphere");
    let scratch = Scratch::new("sphere");

    let outcome = chain::run_corpus_model("Sphere", scratch.path())
        .expect("Sphere must reach a written file");
    let audit = chain::audit_disk_artifact(scratch.path(), "Sphere")
        .expect("reopen and audit the artifact");

    assert_artifact_is_sound(&outcome, &audit);

    // OQ-1, measured rather than assumed. The design record asked whether the
    // real hollow stage completes on corpus models and expected a refusal if it
    // did not. It completes: the Sphere hollows, drains and drills. The refusal
    // is one stage later — the sealed support run will not take the drilled
    // shell — so the artifact is emitted from the oriented solid, exactly as
    // every existing sealed-run gate and the whole OD-8 census do (F14).
    //
    // Pinned as an equality, not a `matches!`, so that the day the drilled shell
    // becomes supportable (or the hollow stage changes its verdict) this fails
    // and says so, instead of quietly changing what the owner is handed.
    match &outcome.supported_geometry {
        chain::SupportedGeometry::DrilledShellRefusedBySupport {
            drilled_faces,
            refusal,
        } => {
            assert!(
                *drilled_faces > outcome.prepared_faces,
                "the drilled shell should have more faces than the oriented solid \
                 that replaced it ({drilled_faces} vs {})",
                outcome.prepared_faces
            );
            assert!(
                refusal.contains("Preparation"),
                "the drilled Sphere shell now fails somewhere other than Preparation: {refusal}"
            );
        }
        other => panic!(
            "Sphere's supported geometry is now {other:?}; re-derive what the export \
             seam emits before trusting this artifact"
        ),
    }
    assert_eq!(
        outcome.hollow_disposition,
        HollowDisposition::Solid {
            reason: SolidDecisionReason::BelowThreshold
        },
        "the emitted Sphere carries the frozen evidence base's solid handoff"
    );

    // All three of the emitter's mesh families are exercised by this one model:
    // frusta (taper/joiner/flare), spheres (tip/shaft) and the raft.
    assert!(
        audit.frustum_records() > 0,
        "Sphere emitted no frustum records: {:?}",
        audit.records_by_segment_type
    );
    assert!(
        audit.sphere_records() > 0,
        "Sphere emitted no sphere records: {:?}",
        audit.records_by_segment_type
    );
    assert!(
        audit.raft_records() > 0,
        "Sphere emitted no raft records: {:?}",
        audit.records_by_segment_type
    );

    // Frame invariant: the one lift the seal chose, carried once.
    assert_eq!(outcome.lift_mm, chain::LIFT_MM);
}

/// A second model through the same seam, procedurally generated so this half of
/// the phase needs no provisioned corpus at all.
#[test]
fn procedural_overhang_table_seals_lowers_and_emits_a_reparseable_chitubox() {
    let scratch = Scratch::new("overhang-table");
    let mesh = make_overhang_table().expect("procedural overhang table");

    let outcome = chain::run_procedural_model("make_overhang_table", mesh, scratch.path())
        .expect("make_overhang_table must reach a written file");
    let audit = chain::audit_disk_artifact(scratch.path(), "make_overhang_table")
        .expect("reopen and audit the artifact");

    assert_artifact_is_sound(&outcome, &audit);
    assert_eq!(outcome.lift_mm, chain::LIFT_MM);
}
