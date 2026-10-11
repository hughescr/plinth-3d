//! The pose-search diagnostics run in their own perf phase, and the pipeline's
//! phases are the same with them on.
//!
//! `prod-all` and `extra=` run production sweeps, which charge work counters
//! and wall time to whatever perf phase is current. They must not be charged
//! to the phase the search itself runs in (`OrientTable`): the sidecar would
//! then misreport what the search cost. This test sets perf on, runs the
//! search with a measurer that records the phase it is called in, and reads the
//! final sidecar.
//!
//! Its own test program, with one test: the perf switch and the phase are
//! process-wide, so no other test may run beside it.

use std::sync::Mutex;
use std::time::Duration;

use geom_kernel::perf::{self, Phase, Work};
use plinth_mesh::{make_overhang_table, Mesh};
use plinth_orient::demand::{DemandFidelity, MeasuredSupportDemand, SupportDemandMeasurer};
use plinth_orient::diag::DiagnosticsConfig;
use plinth_orient::search::{search_with_diagnostics, SweepRequest};
use plinth_orient::{OrientFaceTable, UpDirection};

/// `(who called, fidelity, the perf phase it was called in)`.
type Seen = Mutex<Vec<(&'static str, DemandFidelity, Phase)>>;

static SEEN: Seen = Mutex::new(Vec::new());

/// The perf phase right now: replace it, then put it back.
fn current_phase() -> Phase {
    let phase = perf::set_phase(Phase::Other);
    perf::set_phase(phase);
    phase
}

fn measured(area: f64, fidelity: DemandFidelity) -> MeasuredSupportDemand {
    MeasuredSupportDemand {
        support_area_cm2: area,
        struts: 1,
        braces: 0,
        masts: 0,
        total_scar_mm2: 0.25,
        achieved_pitch_mm: 0.05,
        layers: 10,
        residual_demand_cells: 0,
        uncarried_load_mm2: 0.0,
        model_landings: 0,
        landing_scar_raw_mm2: None,
        tip_scar_raw_mm2: None,
        fidelity,
    }
}

/// Records the phase of every batch and charges one unit of work per candidate,
/// as a real sweep charges its counters.
struct Probe {
    who: &'static str,
}

impl SupportDemandMeasurer for Probe {
    fn measure_support_demand(
        &self,
        _mesh: &Mesh,
        _table: &OrientFaceTable,
        candidates: &[UpDirection],
        fidelity: DemandFidelity,
    ) -> Vec<Result<MeasuredSupportDemand, String>> {
        SEEN.lock()
            .expect("the log")
            .push((self.who, fidelity, current_phase()));
        perf::add(Work::SweepRuns, candidates.len() as u64);
        candidates
            .iter()
            .map(|candidate| Ok(measured(0.5 + 0.25 * candidate.get()[0].abs(), fidelity)))
            .collect()
    }

    fn diagnostic_measurer(&self) -> Option<Box<dyn SupportDemandMeasurer>> {
        Some(Box::new(Probe { who: "diagnostic" }))
    }
}

fn work_in(sidecar: &serde_json::Value, phase: &str) -> u64 {
    sidecar["phases"]
        .as_array()
        .expect("phases")
        .iter()
        .find(|row| row["phase"] == phase)
        .map_or(0, |row| row["work"]["sweep.runs"].as_u64().unwrap_or(0))
}

#[test]
fn the_diagnostic_production_sweeps_are_charged_to_their_own_phase() {
    assert!(
        std::env::var_os("PLINTH_ORIENT_DIAG").is_none(),
        "unset PLINTH_ORIENT_DIAG to run this test"
    );
    let mesh = make_overhang_table().expect("the fixture");
    let request = SweepRequest::registered().expect("the registered sweep configuration");
    let dir = std::env::temp_dir().join(format!("plinth-orient-phase-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    perf::enable(dir.clone(), Duration::from_secs(3600));
    perf::begin_model("phase probe");
    perf::set_phase(Phase::OrientTable);

    let config = DiagnosticsConfig {
        prod_all: true,
        ..DiagnosticsConfig::default()
    };
    let (outcome, diagnostics) =
        search_with_diagnostics(&mesh, &request, Some(&Probe { who: "search" }), &config)
            .expect("the fixture is scorable");
    assert_eq!(current_phase(), Phase::OrientTable, "the phase the search was in comes back");
    perf::end_model();

    let seen = std::mem::take(&mut *SEEN.lock().expect("the log"));
    let calls = |who: &str, fidelity: DemandFidelity| -> Vec<Phase> {
        seen.iter()
            .filter(|(w, f, _)| *w == who && *f == fidelity)
            .map(|(_, _, phase)| *phase)
            .collect()
    };
    assert_eq!(calls("search", DemandFidelity::Prefilter), [Phase::Prefilter]);
    assert_eq!(calls("search", DemandFidelity::Production), [Phase::Production]);
    assert_eq!(
        calls("diagnostic", DemandFidelity::Production),
        [Phase::Diagnostic],
        "the prod-all batch ran in the diagnostic phase"
    );

    // The sidecar: the diagnostic batch's counters sit in the Diagnostic phase,
    // the search's own in Prefilter and Production, and none in OrientTable.
    let text = std::fs::read_to_string(dir.join("phase probe.perf.json")).expect("the sidecar");
    let sidecar: serde_json::Value = serde_json::from_str(&text).expect("json");
    let rows = outcome.record().tiers[2].ranking.len() as u64;
    assert_eq!(work_in(&sidecar, "Diagnostic"), rows, "{text}");
    assert_eq!(work_in(&sidecar, "Prefilter"), rows, "{text}");
    assert_eq!(work_in(&sidecar, "Production"), 3, "{text}");
    assert_eq!(work_in(&sidecar, "OrientTable"), 0, "{text}");
    assert_eq!(diagnostics.production_all.expect("prod-all rows").len() as u64, rows);
    let _ = std::fs::remove_dir_all(&dir);
}
