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
//!
//! **Slow tests.** The lane loop (`scripts/test-fast.sh`) must finish this program in about a
//! minute, so a test that sweeps a heavy fixture (the laid tube, the stilt plate, the Cylinder,
//! a dense orientation search) is slow: about 130 of the 276. A slow test is either
//! `#[cfg_attr(not(feature = "slow-tests"), ignore = "...")]` (it appears as ignored), or its
//! whole file is declared below under `#[cfg(feature = "slow-tests")]` (it is not built). The
//! `slow-tests` feature turns them on: `scripts/gate-full.sh` and `scripts/test-fast.sh --slow`
//! do it. A new test that sweeps a heavy fixture goes in the same way; a file that another
//! file imports from stays built and ignores test by test.

#[path = "../harness/mod.rs"]
mod harness;

mod brace_frame_bands;
#[cfg(feature = "slow-tests")]
mod brace_not_in_flare;
mod brace_to_model_tie;
#[cfg(feature = "slow-tests")]
mod brace_window_cap;
mod bracing;
mod bracing_rework;
mod cancel;
mod chitubox_geometry;
mod clearance_census;
#[cfg(feature = "slow-tests")]
mod congestion_lane;
mod contact_growth;
mod contact_sizing;
#[cfg(feature = "slow-tests")]
mod drain_connectivity;
#[cfg(feature = "slow-tests")]
mod drain_rim_routing;
mod emitted_profile_covers_ledger;
mod every_member_rests_on_something;
mod feature_chord;
#[cfg(feature = "slow-tests")]
mod flank_relief;
mod footprint_keeps_seat;
mod footprint_moves_hold_the_rules;
mod footprint_normal;
mod frame_ledger;
#[cfg(feature = "slow-tests")]
mod head_first;
mod head_hookup;
mod head_join;
mod heads_everywhere;
mod high_poly_keeps_every_rung;
#[cfg(feature = "slow-tests")]
mod joint_certificate;
mod lattice_align;
mod layer_cache_budget;
#[cfg(feature = "slow-tests")]
mod lip_seat_sized;
mod load_insertion;
mod mast_root_clearance;
mod member_clearance;
mod member_profile;
mod model_landing_scar;
#[cfg(feature = "slow-tests")]
mod morel_long_posts;
mod morel_pose;
#[cfg(feature = "slow-tests")]
mod neighbour_cover;
#[cfg(feature = "slow-tests")]
mod oracle_cavity;
mod oracle_disc;
mod oracle_ramp;
#[cfg(feature = "slow-tests")]
mod oracle_tee;
mod per_event_sizing;
mod perf_counters;
mod orient_demand;
mod orient_demand_parallel;
mod pitch_invariance;
mod plate_root_contact;
#[cfg(feature = "slow-tests")]
mod rigid_share_diagnostic;
#[cfg(feature = "slow-tests")]
mod pose_demand_probe;
#[cfg(feature = "slow-tests")]
mod prefilter_no_brace;
mod root_disc_as_drawn;
mod round_bases_ratchet;
mod sabotage;
mod seat_face;
mod seated_line;
mod seeded_down_sweep;
mod shaft_landing;
mod shaft_seat_trim;
mod slenderness_lane;
mod stage_shake_diagnostic;
mod tapered_load_steps;
mod tip_contact_scar;
mod tip_seating;
#[cfg(feature = "slow-tests")]
mod toed_slab_pose;
mod twin_merge;
mod vacated_column;
#[cfg(feature = "slow-tests")]
mod wall_creep;
#[cfg(feature = "slow-tests")]
mod winner_plan_retained;
mod x_allowed_ruling;
