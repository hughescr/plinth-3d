//! `--perf` writes `<perf-dir>/<model>.orient.json` (the record and the
//! pose-search diagnostics), and turning `--perf` on changes nothing the search
//! decides.
//!
//! Its own test program, not a module of `tests/it`: the perf switch is
//! process-wide, and switching it on would make every other search in that
//! program write the same model's sidecar. This program has one test.
//! `PLINTH_ORIENT_DIAG` is read from the environment, so the test refuses to
//! run under one rather than assert about a configuration it did not make.

use std::time::Duration;

use plinth_mesh::make_recessed_disc;
use plinth_orient::diag::SweepDiagnostics;
use plinth_orient::search::{search, SweepRequest};

#[test]
fn perf_on_writes_the_orient_sidecar_and_the_search_is_unchanged() {
    assert!(
        std::env::var_os("PLINTH_ORIENT_DIAG").is_none(),
        "unset PLINTH_ORIENT_DIAG to run this test"
    );
    let mesh = make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128).expect("a disc");
    let request = SweepRequest::registered().expect("the registered sweep configuration");

    // Perf off: nothing is collected, nothing is written.
    let off = search(&mesh, &request, None).expect("a disc is scorable");

    let dir = std::env::temp_dir().join(format!("plinth-orient-json-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    geom_kernel::perf::enable(dir.clone(), Duration::from_secs(3600));
    geom_kernel::perf::begin_model("Disc 50");
    let on = search(&mesh, &request, None).expect("a disc is scorable");
    geom_kernel::perf::end_model();

    assert_eq!(on.record(), off.record(), "perf on does not change the record");
    assert_eq!(on.record().search_digest, off.record().search_digest);
    assert_eq!(on.winner(), off.winner());

    let path = dir.join("Disc 50.orient.json");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("orient.json is JSON");
    assert_eq!(
        value["record"]["search_digest"].as_str(),
        Some(on.record().search_digest.as_str())
    );
    let diagnostics: SweepDiagnostics =
        serde_json::from_value(value["diagnostics"].clone()).expect("the diagnostics read back");
    assert_eq!(diagnostics.search_digest, on.record().search_digest);
    assert!(diagnostics.reproduces_grid, "{:?}", diagnostics.notes);
    assert!(dir.join("Disc 50.perf.json").exists(), "the perf sidecar sits beside it");
    let _ = std::fs::remove_dir_all(&dir);
}
