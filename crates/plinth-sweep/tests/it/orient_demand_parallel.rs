//! Item 5 of the 2026-09-03 slice-sweep review follow-ups: the finalist
//! production sweeps `SweepDemand::measure_support_demand` runs for the
//! orientation search are now index-parallel (`orient_demand.rs`'s
//! `par_iter`), because the Weeping Treant spent 99.68 s of a 252 s prepare
//! in that one serial `.map` (orient-term/measure/Weeping_Treant.log).
//!
//! This proves the parallel batch is bit-identical to the same measurer
//! called one candidate at a time, in the same order — the one-at-a-time
//! calls are the serial reference, going through the exact same
//! `measure_one` body, just never running two candidates concurrently.
//! Nothing about `measure_one` reads or writes state shared across
//! candidates (each call gets its own oriented mesh, its own layer plan and
//! its own sweep scratch), so the two must agree exactly, not merely
//! approximately.

use plinth_mesh::make_recessed_disc;
use plinth_orient::demand::{DemandFidelity, SupportDemandMeasurer};
use plinth_orient::UpDirection;
use plinth_orient::OrientFaceTable;
use plinth_sweep::SweepDemand;

/// A handful of the same up-directions the dense sweep's prefilter actually
/// produces on a disc (see `orient-term/measure/Round_25.log`), so the
/// fixture exercises real candidate geometry rather than a synthetic one.
fn candidates() -> Vec<UpDirection> {
    vec![
        UpDirection::MODEL_UP,
        UpDirection::new([0.0, 0.0, -1.0]).expect("a valid direction"),
        UpDirection::new([0.0625, 0.0, 0.9980]).expect("a valid direction"),
        UpDirection::new([-0.1736, 0.0, 0.9848]).expect("a valid direction"),
        UpDirection::new([0.0, 0.1736, 0.9848]).expect("a valid direction"),
    ]
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "the exact claim under test is bit-identity between the parallel batch and the \
          one-at-a-time serial reference on the SAME inputs through the SAME code path; a \
          tolerance would hide the very divergence this test exists to catch"
)]
fn the_parallel_batch_is_bit_identical_to_one_candidate_at_a_time() {
    let mesh = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("the Round 25 profile");
    let measurer = SweepDemand::production();
    // Prefilter, not Production: the fixture only needs to prove the batch
    // and the one-at-a-time calls agree, and the coarser raster keeps the
    // test cheap. `measure_one` branches on fidelity twice -- which pitch it
    // reads, and whether it prices model landings -- and neither branch is a
    // shared-state one, so the batch-vs-serial question this test asks is the
    // same question at either fidelity.
    let fidelity = DemandFidelity::Prefilter;
    let candidates = candidates();

    let table = OrientFaceTable::build(&mesh, "exact").expect("a closed fixture always scores");
    let batch = measurer.measure_support_demand(&mesh, &table, &candidates, fidelity);
    assert_eq!(batch.len(), candidates.len(), "the batch dropped or duplicated a candidate");

    let serial: Vec<_> = candidates
        .iter()
        .map(|&candidate| {
            let one = std::slice::from_ref(&candidate);
            measurer
                .measure_support_demand(&mesh, &table, one, fidelity)
                .into_iter()
                .next()
                .expect("one candidate in, one result out")
        })
        .collect();

    let mut any_ok = false;
    for (index, (parallel, alone)) in batch.iter().zip(serial.iter()).enumerate() {
        match (parallel, alone) {
            (Ok(p), Ok(s)) => {
                any_ok = true;
                assert_eq!(
                    p.struts, s.struts,
                    "candidate {index}: strut count diverged between the parallel batch and the \
                     one-at-a-time reference"
                );
                assert_eq!(
                    p.support_area_cm2, s.support_area_cm2,
                    "candidate {index}: burden (support_area_cm2) diverged"
                );
                assert_eq!(
                    p.total_scar_mm2, s.total_scar_mm2,
                    "candidate {index}: total_scar_mm2 diverged"
                );
                assert_eq!(
                    p.achieved_pitch_mm, s.achieved_pitch_mm,
                    "candidate {index}: achieved_pitch_mm diverged"
                );
                assert_eq!(p.layers, s.layers, "candidate {index}: layers diverged");
                assert_eq!(
                    p.residual_demand_cells, s.residual_demand_cells,
                    "candidate {index}: residual_demand_cells diverged"
                );
                assert_eq!(
                    p.uncarried_load_mm2, s.uncarried_load_mm2,
                    "candidate {index}: uncarried_load_mm2 diverged"
                );
                assert_eq!(p.fidelity, s.fidelity, "candidate {index}: fidelity diverged");
            }
            (Err(parallel_error), Err(serial_error)) => {
                assert_eq!(
                    parallel_error, serial_error,
                    "candidate {index}: parallel and serial disagree on the failure message"
                );
            }
            _ => panic!(
                "candidate {index}: parallel and serial disagree on success/failure: \
                 {parallel:?} vs {alone:?}"
            ),
        }
    }
    assert!(any_ok, "every candidate failed; this comparison would be about nothing");
}
