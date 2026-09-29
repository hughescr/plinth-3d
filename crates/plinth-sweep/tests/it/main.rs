//! The one integration-test program for `plinth-sweep`.
//!
//! Each file below used to be its own test program, and cargo links every
//! test program separately against the whole dependency graph. As modules of
//! one program they share that link. Test names gain the file stem as a prefix
//! (`orient_demand::name`), so `cargo test -p plinth-sweep --test it
//! orient_demand::` runs one former file.
//!
//! The shared helpers stay at `tests/harness/mod.rs`, so a top-level test file
//! can still declare `mod harness;`; here they are one module that every file
//! reaches as `crate::harness`. A new top-level `tests/<name>.rs` still builds
//! and runs on its own, but a new test belongs here as `tests/it/<name>.rs`
//! plus a `mod` line. See `docs/development.md`, "Where integration tests go".

#[path = "../harness/mod.rs"]
mod harness;

mod bracing;
mod congestion_lane;
mod contact_sizing;
mod drain_connectivity;
mod emitted_profile_covers_ledger;
mod every_member_rests_on_something;
mod flank_relief;
mod footprint_moves_hold_the_rules;
mod load_insertion;
mod mast_root_clearance;
mod member_clearance;
mod model_landing_scar;
mod oracle_cavity;
mod oracle_disc;
mod oracle_ramp;
mod oracle_tee;
mod orient_demand;
mod orient_demand_parallel;
mod pitch_invariance;
mod plate_root_contact;
mod pose_demand_probe;
mod root_disc_as_drawn;
mod round_bases_ratchet;
mod sabotage;
mod shaft_landing;
mod shaft_seat_trim;
mod slenderness_lane;
mod tip_contact_scar;
mod tip_seating;
mod vacated_column;
mod wall_creep;
