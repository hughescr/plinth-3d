//! Perf lane B1: cooperative cancellation of a pose's production sweep.
//!
//! The orientation search stops a finalist pose once a finished one provably
//! beats it (`plinth_orient::demand::ProductionRace`, `plinth_sweep::cancel`).
//! These tests pin the sweep's half of that: a set token ends the sweep at its
//! next safe point with `RaceOutcome::Stopped` (not a result, not a failure
//! string), a clear token changes nothing, and a stop returns promptly instead
//! of at the end of the sweep.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use plinth_mesh::make_recessed_disc;
use plinth_orient::demand::{DemandFidelity, ProductionRace, RaceOutcome, SupportDemandMeasurer};
use plinth_orient::{OrientFaceTable, UpDirection};
use plinth_sweep::SweepDemand;

fn poses() -> Vec<UpDirection> {
    vec![
        UpDirection::MODEL_UP,
        UpDirection::new([0.0625, 0.0, 0.9980]).expect("a valid direction"),
    ]
}

#[test]
fn a_clear_token_changes_nothing_and_a_set_one_is_stopped_before_it_starts() {
    let mesh = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("the Round 25 profile");
    let table = OrientFaceTable::build(&mesh, "exact").expect("a closed fixture always scores");
    let measurer = SweepDemand::production();
    let poses = poses();

    // Pose 1 is stopped before it starts; pose 0 is untouched by the race.
    let race = ProductionRace::never_stops(poses.len());
    race.token(1)
        .expect("a token")
        .store(true, Ordering::Release);
    let raced = measurer.measure_production_race(&mesh, &table, &poses, &race);
    assert!(matches!(raced[1], RaceOutcome::Stopped), "pose 1 never ran");
    let plain =
        measurer.measure_support_demand(&mesh, &table, &poses[..1], DemandFidelity::Production);
    let RaceOutcome::Measured(raced_zero) = &raced[0] else {
        panic!("pose 0 should have measured, got {:?}", raced[0]);
    };
    assert_eq!(
        Some(raced_zero),
        plain[0].as_ref().ok(),
        "a pose the race leaves alone measures exactly what it measures without a race"
    );
}

#[test]
fn a_stop_during_the_sweep_returns_at_a_safe_point_not_at_the_end() {
    let mesh = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("the Round 25 profile");
    let table = OrientFaceTable::build(&mesh, "exact").expect("a closed fixture always scores");
    let measurer = SweepDemand::production();
    let pose = [UpDirection::new([0.0625, 0.0, 0.9980]).expect("a valid direction")];

    // The whole sweep, for scale.
    let whole = Instant::now();
    let full = measurer.measure_support_demand(&mesh, &table, &pose, DemandFidelity::Production);
    let whole = whole.elapsed();
    assert!(full[0].is_ok(), "the fixture sweeps");

    // The same sweep, with its token set a tenth of the way in.
    let race = ProductionRace::never_stops(1);
    let token = std::sync::Arc::clone(race.token(0).expect("a token"));
    let delay = whole / 10;
    let setter = std::thread::spawn(move || {
        std::thread::sleep(delay);
        token.store(true, Ordering::Release);
    });
    let started = Instant::now();
    let outcome = measurer.measure_production_race(&mesh, &table, &pose, &race);
    let stopped_after = started.elapsed();
    setter.join().expect("the setter ran");
    assert!(
        matches!(outcome[0], RaceOutcome::Stopped),
        "a token set mid-sweep ends it: {:?}",
        outcome[0]
    );
    assert!(
        stopped_after < whole.mul_f64(0.8) + Duration::from_millis(50),
        "stopped after {stopped_after:?} of a {whole:?} sweep: it ran to the end"
    );
}

/// A stop can land inside ANY stage (a down-sweep layer walk, a lip-seat round,
/// the removal replay, a tie pass, the seat stage), so every stage that polls
/// the token returns half a result when it sees it set. This sweeps the stop
/// point across the whole sweep: each run must end as `Stopped` (or as the
/// finished measurement when the stop came too late), never panic on a
/// half-written field, and never return a measurement that differs from the
/// untouched one (a cancelled computation must not leak a value into live
/// logic).
#[test]
fn a_stop_at_any_point_ends_stopped_or_measures_exactly_what_it_would_have() {
    let mesh = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("the Round 25 profile");
    let table = OrientFaceTable::build(&mesh, "exact").expect("a closed fixture always scores");
    let measurer = SweepDemand::production();
    let pose = [UpDirection::new([0.0625, 0.0, 0.9980]).expect("a valid direction")];

    let whole = Instant::now();
    let full = measurer.measure_support_demand(&mesh, &table, &pose, DemandFidelity::Production);
    let whole = whole.elapsed();
    let full = full[0].as_ref().expect("the fixture sweeps");

    let mut stopped = 0_usize;
    for step in 1..=15_u32 {
        let race = ProductionRace::never_stops(1);
        let token = std::sync::Arc::clone(race.token(0).expect("a token"));
        let delay = whole.mul_f64(f64::from(step) / 16.0);
        let setter = std::thread::spawn(move || {
            std::thread::sleep(delay);
            token.store(true, Ordering::Release);
        });
        let outcome = measurer.measure_production_race(&mesh, &table, &pose, &race);
        setter.join().expect("the setter ran");
        match &outcome[0] {
            RaceOutcome::Stopped => stopped += 1,
            RaceOutcome::Measured(measured) => assert_eq!(
                measured, full,
                "a stop that came too late must leave the measurement untouched (step {step})"
            ),
            other => panic!("a stop must end as Stopped, got {other:?} (step {step})"),
        }
    }
    assert!(stopped > 0, "no stop landed inside the sweep: the sweep is too short to test");
}

/// A token a different pose installed on this thread: set, and already
/// cancelled. A rayon worker that steals a plain job while it waits inside a
/// raced pose looks exactly like this.
fn inherited_cancelled_token() -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true))
}

#[test]
fn a_plain_measurement_ignores_a_cancelled_token_it_did_not_set() {
    let mesh = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("the Round 25 profile");
    let table = OrientFaceTable::build(&mesh, "exact").expect("a closed fixture always scores");
    let measurer = SweepDemand::production();
    let pose = [UpDirection::new([0.0625, 0.0, 0.9980]).expect("a valid direction")];

    let clean = measurer.measure_support_demand(&mesh, &table, &pose, DemandFidelity::Production);
    let token = inherited_cancelled_token();
    let stolen = plinth_sweep::cancel::with_token(&token, || {
        measurer.measure_support_demand(&mesh, &table, &pose, DemandFidelity::Production)
    });
    assert!(
        stolen[0].is_ok(),
        "a job stolen under another pose's cancelled token must complete: {:?}",
        stolen[0]
    );
    assert_eq!(
        stolen[0], clean[0],
        "and measure exactly what it measures with no token on the thread"
    );
    assert!(
        token.load(Ordering::Acquire),
        "the pose's own token is untouched"
    );
}

#[test]
fn a_public_sweep_entry_ignores_a_cancelled_token_it_did_not_set() {
    let mesh = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("the Round 25 profile");
    let policy = plinth_sweep::SweepPolicy::from_registry().expect("the registry supplies the sweep");
    let token = inherited_cancelled_token();
    // `sweep_with_uncached` always runs the sweep through `run_sweep_with_drains`,
    // and panics on a refusal, which a `Cancelled` would be.
    let plan = plinth_sweep::cancel::with_token(&token, || {
        crate::harness::sweep_with_uncached(&mesh, &policy)
    });
    assert!(!plan.struts.is_empty(), "the sweep ran to the end");
}
