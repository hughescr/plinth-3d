//! Small synthetic shapes for unit tests (design section 6, T0).

pub(crate) type Soup = (Vec<f64>, Vec<f64>, Vec<f64>);

/// A unit cube `[0,1]^3` as a 36-corner soup (12 triangles), outward winding.
pub(crate) fn cube_soup() -> Soup {
    let c: [[f64; 3]; 8] = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ];
    let quads = [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [2, 3, 7, 6],
        [1, 2, 6, 5],
        [3, 0, 4, 7],
    ];
    soup_from_triangles(
        &quads
            .iter()
            .flat_map(|q| [[c[q[0]], c[q[1]], c[q[2]]], [c[q[0]], c[q[2]], c[q[3]]]])
            .collect::<Vec<_>>(),
    )
}

/// A triangle list as a soup.
pub(crate) fn soup_from_triangles(tris: &[[[f64; 3]; 3]]) -> Soup {
    let mut s = (Vec::new(), Vec::new(), Vec::new());
    for p in tris.iter().flatten() {
        s.0.push(p[0]);
        s.1.push(p[1]);
        s.2.push(p[2]);
    }
    s
}

/// A flat square plate `[0,4]^2` at z = 0 made of two triangles.
pub(crate) fn plate_soup() -> Soup {
    soup_from_triangles(&[
        [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 4.0, 0.0]],
        [[0.0, 0.0, 0.0], [4.0, 4.0, 0.0], [0.0, 4.0, 0.0]],
    ])
}

/// Assert two points are equal to within 1e-12 per coordinate (clippy denies `==` on
/// floats; these known-answer values are exact up to rounding).
#[track_caller]
pub(crate) fn assert_pt(got: [f64; 3], want: [f64; 3]) {
    let ok = got.iter().zip(&want).all(|(g, w)| (g - w).abs() <= 1e-12);
    assert!(ok, "got {got:?}, want {want:?}");
}

/// The worker count this test may use: the slot allocation `scripts/t` exports as
/// `NEXTEST_TEST_THREADS` (design section 5.6); 1 when it is absent. Library code never
/// reads the environment.
#[allow(
    clippy::disallowed_methods,
    reason = "test-only read of the slot allocation set by scripts/t"
)]
pub(crate) fn slot_threads() -> usize {
    std::env::var("NEXTEST_TEST_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .clamp(1, 8)
}

/// True when the runner started this test in the bench epoch: `scripts/t` in the bench
/// slot selects the nextest profile `bench`, and nextest exports its name.
#[allow(
    clippy::disallowed_methods,
    reason = "test-only read of the profile name nextest exports"
)]
pub(crate) fn in_bench_epoch() -> bool {
    std::env::var("NEXTEST_PROFILE").is_ok_and(|p| p == "bench")
}

/// Run a parallel batch with `threads` workers and fail unless the request, the tasks made
/// and the distinct threads that really ran stay within `limit`. Every test that starts
/// workers goes through here, so an over-allocation fails the test (allocation witness).
#[track_caller]
pub(crate) fn par_within(
    limit: usize,
    bvh: &crate::Bvh<'_>,
    queries: &crate::Points,
    out: &mut crate::ClosestHits,
    threads: usize,
) -> crate::counters::ParReport {
    assert!(
        threads <= limit,
        "test asks for {threads} workers; allowed {limit}"
    );
    // Drop any stale report, run the PUBLIC entrypoint, then read what it measured.
    let _ = crate::counters::take_par();
    crate::closest_points_par(bvh, queries, out, threads).expect("parallel");
    let r = crate::counters::take_par().expect("closest_points_par left no report");
    assert_report_within(limit, r);
    r
}

/// Fail when a measured report shows more tasks or threads than `limit`.
#[track_caller]
pub(crate) fn assert_report_within(limit: usize, r: crate::counters::ParReport) {
    assert!(
        r.tasks <= limit && r.threads <= limit,
        "query used {} tasks on {} threads; allowed {limit}",
        r.tasks,
        r.threads
    );
}
