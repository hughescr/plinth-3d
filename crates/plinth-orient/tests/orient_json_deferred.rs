//! `search_deferred` writes no `<model>.orient.json`: the file waits in a
//! `PendingOutput` until the caller publishes it, and a dropped one never
//! appears. This is what lets the pipeline run orientation beside decimation's
//! diagnostics and still leave nothing behind when the diagnostics fail.
//!
//! Its own test program, with one test, for the reason `orient_json_sidecar.rs`
//! gives: the perf switch is process-wide. `PLINTH_ORIENT_DIAG` is read from
//! the environment, so the test refuses to run under one.

use std::time::Duration;

use plinth_mesh::make_recessed_disc;
use plinth_orient::search::{search, search_deferred, SweepRequest};

#[test]
fn a_deferred_search_writes_nothing_until_its_output_is_published() {
    assert!(
        std::env::var_os("PLINTH_ORIENT_DIAG").is_none(),
        "unset PLINTH_ORIENT_DIAG to run this test"
    );
    let mesh = make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128).expect("a disc");
    let request = SweepRequest::registered().expect("the registered sweep configuration");
    let dir = std::env::temp_dir().join(format!("plinth-orient-deferred-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    geom_kernel::perf::enable(dir.clone(), Duration::from_secs(3600));
    let json = |model: &str| dir.join(format!("{model}.orient.json"));

    // Dropped: the search ran, its outcome is returned, and no file ever appears.
    geom_kernel::perf::begin_model("dropped");
    let (outcome, pending) = search_deferred(&mesh, &request, None).expect("a disc is scorable");
    assert!(!pending.is_empty(), "the diagnostics produced an orient.json");
    assert!(!json("dropped").exists(), "a deferred search must write nothing");
    drop(pending);
    geom_kernel::perf::end_model();
    assert!(!json("dropped").exists(), "a dropped pending output is never written");

    // Published: the same bytes the immediate search writes.
    geom_kernel::perf::begin_model("published");
    let (deferred, pending) = search_deferred(&mesh, &request, None).expect("scorable");
    assert!(!json("published").exists());
    pending.publish();
    assert!(json("published").exists(), "publish writes the file");
    geom_kernel::perf::end_model();

    geom_kernel::perf::begin_model("immediate");
    let immediate = search(&mesh, &request, None).expect("scorable");
    geom_kernel::perf::end_model();
    assert_eq!(deferred.record(), immediate.record());
    assert_eq!(outcome.record(), immediate.record());
    let value = |model: &str| {
        let bytes = std::fs::read(json(model)).expect("orient.json");
        serde_json::from_slice::<serde_json::Value>(&bytes).expect("json")
    };
    assert_eq!(value("published"), value("immediate"));
    let _ = std::fs::remove_dir_all(&dir);
}
