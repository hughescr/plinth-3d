//! The two BVH walks under classification: the hit set did not move, the two
//! walks agree on the uncertainty, and the uncertainty can actually fire.
//!
//! # What must hold, and why each is asserted rather than argued
//!
//! 1. **The accepted-hit set is unchanged.** Classification surfaces the
//!    determinant guard; it does not alter the acceptance predicate. Checked
//!    against `testkit::brute_moller_hits`, which walks EVERY triangle with the
//!    pinned primitive and no BVH — a structurally different route to the same
//!    set, not a copy of the walk.
//! 2. **The collecting and counting walks agree on the tally, per ray.**
//!    They are mirror twins by standing rule. **Nothing enforces this on a
//!    caller's behalf:** `CensusDrift` covers hits, subsegments, samples and
//!    intervals and has never covered tallies, so the comparison is made HERE,
//!    explicitly, rather than assumed to exist somewhere.
//! 3. **The uncertain arm fires when a near-parallel face is planted.** A
//!    counter that cannot fire is the failure mode this project keeps
//!    reproducing, so the planted fixture proves it can — and a clean fixture
//!    proves it is not simply always on.

use geom_kernel::telemetry::{near_parallel_counts, NearParallelSite};
use geom_kernel::testkit::{
    brute_moller_hits, build_from_tris, build_surface, grid_mesh, sample_rays,
};
use geom_kernel::NativeSurface;

/// The §O4 counterexample triangle, alone on a surface.
///
/// Vertices `(0,0,0)`, `(2,0,0)`, `(0,2,0)`, so `e1 = (2,0,0)` and
/// `e2 = (0,2,0)` — the exact literals whose determinant against the ray below
/// is `-8e-15`.
fn near_parallel_face() -> NativeSurface {
    build_from_tris(vec![
        0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0, //
    ])
}

/// The ray that puts that face in the band: origin just under the plane,
/// direction almost in it.
const NP_ORIGIN: [f64; 3] = [0.4, 0.5, -1.0e-16];
const NP_DIRECTION: [f64; 3] = [1.0, 0.0, 2.0e-15];

/// A ray that hits the same face squarely, so the fixture can be shown to be
/// capable of producing a decided crossing too.
const CLEAN_ORIGIN: [f64; 3] = [0.4, 0.5, 1.0];
const CLEAN_DIRECTION: [f64; 3] = [0.0, 0.0, -1.0];

#[test]
#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "ragged batch offsets are `i64` by the kernel's batch contract and are \
              non-negative and monotone by construction; indexing with them is the \
              intended use"
)]
fn the_classified_walk_keeps_the_hit_set_the_pinned_primitive_would_accept() {
    let mesh = grid_mesh(6);
    let surface = build_surface(&mesh);
    let rays = sample_rays(&mesh, 240);
    let origins: Vec<[f64; 3]> = rays.iter().map(|r| r.0).collect();
    let dirs: Vec<[f64; 3]> = rays.iter().map(|r| r.1).collect();

    let cast = surface
        .cast_rays_classified(&origins, &dirs, 1.0)
        .expect("matched batch lengths");

    let mut compared = 0_u32;
    let mut total_faces = 0_usize;
    for (index, (origin, dir)) in rays.iter().enumerate() {
        // The brute oracle: every triangle, the pinned primitive, no BVH and no
        // dedup. Dedup can only MERGE neighbours, so the walk's face set is a
        // subset of the brute set and every walk face must appear in it.
        let brute = brute_moller_hits(&mesh, *origin, *dir, 1.0);
        let lo = cast.offsets[index] as usize;
        let hi = cast.offsets[index + 1] as usize;
        for face in &cast.face_all[lo..hi] {
            let face = u32::try_from(*face).expect("global face IDs are non-negative");
            assert!(
                brute.contains(&face),
                "ray {index} kept face {face}, which the pinned brute-force walk \
                 does not accept: the classification moved the hit set"
            );
            total_faces += 1;
        }
        // And nothing the brute walk found may have vanished, beyond what dedup
        // legitimately merges: the retained count is at least 1 whenever the
        // brute walk found anything.
        assert_eq!(
            brute.is_empty(),
            hi == lo,
            "ray {index}: the walk and the brute oracle disagree about whether \
             this ray crosses the surface at all"
        );
        compared += 1;
    }

    // COUNTED ROWS: a sampling that produced no crossings would pass every
    // assertion above having compared nothing.
    assert_eq!(compared, 240, "every sampled ray must be compared");
    assert!(
        total_faces >= 100,
        "the probe mesh must actually be crossed; only {total_faces} kept faces"
    );
    assert!(
        !cast.any_near_parallel(),
        "the clean probe mesh must not fire the uncertain arm, or the planted \
         fixture below proves nothing by contrast: {:?}",
        cast.near_parallel
    );
}

#[test]
#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "ragged batch offsets are `i64` by the kernel's batch contract and are \
              non-negative and monotone by construction; the difference of two of \
              them is a row length"
)]
fn the_two_walks_agree_on_crossings_and_on_the_tally_ray_by_ray() {
    // Both a clean mesh and the planted face, so the agreement is checked where
    // the tally is zero AND where it is not.
    for (label, surface, origins, dirs) in [
        (
            "grid probe",
            build_surface(&grid_mesh(5)),
            sample_rays(&grid_mesh(5), 120)
                .iter()
                .map(|r| r.0)
                .collect::<Vec<_>>(),
            sample_rays(&grid_mesh(5), 120)
                .iter()
                .map(|r| r.1)
                .collect::<Vec<_>>(),
        ),
        (
            "planted near-parallel face",
            near_parallel_face(),
            vec![NP_ORIGIN, CLEAN_ORIGIN],
            vec![NP_DIRECTION, CLEAN_DIRECTION],
        ),
    ] {
        let census = surface
            .cast_rays_census_classified(&origins, &dirs, 1.0)
            .expect("matched batch lengths");
        let cast = surface
            .cast_rays_classified(&origins, &dirs, 1.0)
            .expect("matched batch lengths");

        assert_eq!(
            census.near_parallel, cast.near_parallel,
            "{label}: the counting and collecting walks disagree on the \
             uncertainty tally. Nothing else in this kernel checks that."
        );
        for index in 0..origins.len() {
            let kept = (cast.offsets[index + 1] - cast.offsets[index]) as usize;
            assert!(
                kept <= census.crossings[index],
                "{label} ray {index}: dedup can only merge, so the kept count \
                 ({kept}) must not exceed the counted crossings ({})",
                census.crossings[index]
            );
        }
    }
}

/// **THE PLANTED FIXTURE.** A face the arithmetic cannot decide produces a
/// nonzero tally, on both walks, and moves the process-global counters for both
/// walk sites.
///
/// Ray 0 is the §O4 counterexample: a real interior crossing at `t = 0.05` that
/// the acceptance predicate reports as no crossing. Ray 1 hits the same face
/// squarely from above. The contrast is the point — the same surface produces a
/// decided answer and an undecided one, so the tally is measuring the RAY's
/// conditioning, not the mesh's.
#[test]
fn a_planted_near_parallel_face_fires_the_tally_and_both_walk_counters() {
    let surface = near_parallel_face();
    let origins = [NP_ORIGIN, CLEAN_ORIGIN];
    let dirs = [NP_DIRECTION, CLEAN_DIRECTION];

    let before = near_parallel_counts();
    let census = surface
        .cast_rays_census_classified(&origins, &dirs, 1.0)
        .expect("matched batch lengths");
    let cast = surface
        .cast_rays_classified(&origins, &dirs, 1.0)
        .expect("matched batch lengths");
    let after = near_parallel_counts();

    // The undecidable ray.
    assert_eq!(
        cast.near_parallel[0], 1,
        "the planted face is undecidable for ray 0 and must be counted"
    );
    assert_eq!(
        cast.offsets[1] - cast.offsets[0],
        0,
        "and it must NOT be reported as a crossing: no crossing was established"
    );
    assert_eq!(census.crossings[0], 0);
    assert_eq!(census.near_parallel[0], 1, "both walks see the same face");

    // The decided ray, on the same surface.
    assert_eq!(
        cast.near_parallel[1], 0,
        "a square hit on the same face is decided, so the tally is not simply \
         always on"
    );
    assert_eq!(
        cast.offsets[2] - cast.offsets[1],
        1,
        "and it is a real crossing"
    );
    assert_eq!(census.crossings[1], 1);

    // Batch-level convenience readers, so they are not untested surface.
    assert!(cast.any_near_parallel());
    assert_eq!(cast.total_near_parallel(), 1);
    assert!(census.any_near_parallel());
    assert_eq!(census.total_near_parallel(), 1);

    // THE COUNTERS. Global and monotone, so the assertion is a delta and `>=`:
    // another test in this binary can only ever add.
    assert!(
        after.collecting_walk - before.collecting_walk >= 1,
        "the collecting-walk counter did not move: {before:?} -> {after:?}"
    );
    assert!(
        after.counting_walk - before.counting_walk >= 1,
        "the counting-walk counter did not move: {before:?} -> {after:?}"
    );
}

/// The counter report is complete and stable: one row per site, every site,
/// zeros included.
///
/// A run report that silently omits a site is indistinguishable from a site
/// that never fired, which is the whole class of defect this instrumentation
/// exists to avoid.
#[test]
fn the_counter_report_names_every_site_and_prints_zero_rows() {
    let counts = near_parallel_counts();
    let report = counts.report();
    assert_eq!(
        report.lines().count(),
        NearParallelSite::ALL.len(),
        "one row per site, whatever the values: {report}"
    );
    for site in NearParallelSite::ALL {
        assert!(
            report.contains(site.label()),
            "site {} is missing from the report: {report}",
            site.label()
        );
    }
    assert_eq!(
        counts.total(),
        counts.collecting_walk + counts.counting_walk + counts.seg_tri_pierce,
        "the total is the sum of the sites it names, not a separate tally"
    );
    for site in NearParallelSite::ALL {
        // `get` must route to the field the site names, or the report's rows
        // would be labelled with one site's name and another's number.
        let _ = counts.get(site);
    }
    assert_eq!(
        counts.get(NearParallelSite::CollectingWalk),
        counts.collecting_walk
    );
    assert_eq!(
        counts.get(NearParallelSite::CountingWalk),
        counts.counting_walk
    );
    assert_eq!(
        counts.get(NearParallelSite::SegTriPierce),
        counts.seg_tri_pierce
    );
}

/// The segment chain's own planted case, at the walk level: an undecidable
/// pierce keeps the face OUT of a proximity band unless a measured candidate
/// puts it there.
///
/// Bands are exemptions, and an exemption grows only on proof (§EXT-2
/// justification 12). The tally is what keeps the omission from being silent.
#[test]
fn an_undecidable_pierce_is_counted_by_the_band_walk_without_growing_the_band() {
    let surface = near_parallel_face();
    // A segment lying essentially IN the face's plane: its endpoints are ~1e-16
    // from the triangle, so a tiny band admits it on the MEASURED endpoint
    // distance while the pierce candidate stays undecidable.
    let p0 = [NP_ORIGIN];
    let p1 = [[1.4, 0.5, 1.9e-15]];

    let generous = geom_kernel::NearFaceBatchLimits {
        max_rows: usize::MAX,
        max_faces_per_row: usize::MAX,
        max_total_faces: usize::MAX,
    };
    let band = surface
        .faces_near_segments_batch_classified(&p0, &p1, &[1.0e-9], generous)
        .expect("matched batch lengths");
    assert_eq!(
        band.near_parallel,
        [1],
        "the undecidable pierce is counted even though the face joined the band \
         on its measured distance"
    );
    assert_eq!(
        band.rows,
        vec![vec![0]],
        "the face is in the band because it was MEASURED within 1e-9 mm, not \
         because the pierce was assumed"
    );

    // The refusal-direction half: at a band radius far below the measured
    // distance the face must NOT be admitted, undecidable pierce or not.
    let far_probe_p0 = [[0.4, 0.5, -1.0]];
    let far_probe_p1 = [[1.4, 0.5, -1.0 + 2.0e-15]];
    let tight = surface
        .faces_near_segments_batch_classified(&far_probe_p0, &far_probe_p1, &[0.1], generous)
        .expect("matched batch lengths");
    assert_eq!(
        tight.rows,
        vec![Vec::<u32>::new()],
        "1 mm away with a 0.1 mm band: an unproven pierce must never add a face"
    );
}
