//! The one integration-test program for `plinth-pipeline`.
//!
//! Each file below used to be its own test program, and cargo links every
//! test program separately against the whole dependency graph. As modules of
//! one program they share that link. Test names gain the file stem as a prefix
//! (`first_artifact::name`).
//!
//! `chain` is the composition `examples/first_artifact.rs` also runs
//! (`examples/shared/first_artifact_chain.rs`). It used to be compiled once per
//! file that named it; here it is one module that every file reaches as
//! `crate::chain`. A new top-level `tests/<name>.rs` still builds and runs on
//! its own, but a new test belongs here as `tests/it/<name>.rs` plus a `mod`
//! line. See `docs/development.md`, "Where integration tests go".

#[path = "../../examples/shared/first_artifact_chain.rs"]
mod chain;

mod a4_noncoverage_gate_drift;
mod a4_production_door;
mod cylinder_drain_ring;
mod deinonychus_witness_spots;
mod dual_cavity_real_pipeline;
mod first_artifact;
mod hollow_first;
mod lip_seat;
mod load_gate_area_law;
mod load_gate_warning_mode_corpus_survey;
mod member_clearance_emitted;
mod model_feet_emitted;
mod no_private_vector_algebra;
mod orient_as_drawn;
mod plate_contact;
mod prepare_folder;
mod repair_prepass;
mod round100_emit;
mod tip_trim;
mod winner_plan_reuse;
