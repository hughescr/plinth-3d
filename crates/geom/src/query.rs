//! Batched closest-point queries. Every query takes slices and fills caller-owned output
//! slices; there is no single-point public query (design section 8).

use crate::bvh::Bvh;
use crate::counters::{self, Op, ParReport};
use crate::error::{GeomError, Result};

/// The smallest work unit handed to one thread, in query points (design section 8:
/// coarse tasks only; tiny tasks measured 1.9x slower).
pub const MIN_POINTS_PER_TASK: usize = 1000;

/// Query points in structure-of-arrays layout. Finite by construction.
#[derive(Debug, Clone, PartialEq)]
pub struct Points {
    xs: Vec<f64>,
    ys: Vec<f64>,
    zs: Vec<f64>,
}

impl Points {
    /// Build a point set.
    ///
    /// # Errors
    /// Returns an error for unequal lengths or a non-finite coordinate.
    pub fn new(xs: Vec<f64>, ys: Vec<f64>, zs: Vec<f64>) -> Result<Self> {
        for (what, v) in [("query ys", &ys), ("query zs", &zs)] {
            if v.len() != xs.len() {
                return Err(GeomError::LengthMismatch {
                    what,
                    expected: xs.len(),
                    actual: v.len(),
                });
            }
        }
        for (what, v) in [("query xs", &xs), ("query ys", &ys), ("query zs", &zs)] {
            if let Some(index) = v.iter().position(|c| !c.is_finite()) {
                return Err(GeomError::NonFinite { what, index });
            }
        }
        Ok(Self { xs, ys, zs })
    }

    /// Number of points.
    #[must_use]
    pub fn len(&self) -> usize {
        self.xs.len()
    }

    /// True when there are no points.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.xs.is_empty()
    }

    /// The x coordinates.
    #[must_use]
    pub fn xs(&self) -> &[f64] {
        &self.xs
    }

    /// The y coordinates.
    #[must_use]
    pub fn ys(&self) -> &[f64] {
        &self.ys
    }

    /// The z coordinates.
    #[must_use]
    pub fn zs(&self) -> &[f64] {
        &self.zs
    }
}

/// Results of a closest-point batch, one entry per query point: the mesh triangle number
/// (ties go to the lowest), the closest point on it and the squared distance.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosestHits {
    /// Closest triangle number per query.
    pub tri: Vec<u32>,
    /// Closest point x.
    pub x: Vec<f64>,
    /// Closest point y.
    pub y: Vec<f64>,
    /// Closest point z.
    pub z: Vec<f64>,
    /// Squared distance from the query to the closest point.
    pub dist2: Vec<f64>,
}

impl ClosestHits {
    /// An output buffer for `n` queries.
    #[must_use]
    pub fn with_len(n: usize) -> Self {
        Self {
            tri: vec![0; n],
            x: vec![0.0; n],
            y: vec![0.0; n],
            z: vec![0.0; n],
            dist2: vec![0.0; n],
        }
    }

    /// Number of results.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tri.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tri.is_empty()
    }

    fn check_len(&self, n: usize) -> Result<()> {
        for (what, len) in [
            ("hits.tri", self.tri.len()),
            ("hits.x", self.x.len()),
            ("hits.y", self.y.len()),
            ("hits.z", self.z.len()),
            ("hits.dist2", self.dist2.len()),
        ] {
            if len != n {
                return Err(GeomError::LengthMismatch {
                    what,
                    expected: n,
                    actual: len,
                });
            }
        }
        Ok(())
    }

    /// Split into `tasks` disjoint pieces whose sizes differ by at most one, so every
    /// piece holds at least `len / tasks` results. `tasks` must be at least 1.
    fn split_mut(&mut self, tasks: usize) -> Vec<OutChunk<'_>> {
        let n = self.tri.len();
        let (base, extra) = (n / tasks, n % tasks);
        let mut rest = (
            &mut self.tri[..],
            &mut self.x[..],
            &mut self.y[..],
            &mut self.z[..],
            &mut self.dist2[..],
        );
        let mut out = Vec::with_capacity(tasks);
        let mut start = 0;
        for t in 0..tasks {
            let len = base + usize::from(t < extra);
            let (tri, r0) = rest.0.split_at_mut(len);
            let (x, r1) = rest.1.split_at_mut(len);
            let (y, r2) = rest.2.split_at_mut(len);
            let (z, r3) = rest.3.split_at_mut(len);
            let (dist2, r4) = rest.4.split_at_mut(len);
            rest = (r0, r1, r2, r3, r4);
            out.push(OutChunk {
                start,
                tri,
                x,
                y,
                z,
                dist2,
            });
            start += len;
        }
        out
    }
}

/// Number of parallel tasks for `n` queries on up to `threads` threads: every task gets at
/// least [`MIN_POINTS_PER_TASK`] queries, so a batch smaller than two minimum tasks runs as
/// one task.
fn task_count(n: usize, threads: usize) -> usize {
    threads.max(1).min(n / MIN_POINTS_PER_TASK).max(1)
}

/// A disjoint piece of the output together with the index of its first query.
struct OutChunk<'a> {
    start: usize,
    tri: &'a mut [u32],
    x: &'a mut [f64],
    y: &'a mut [f64],
    z: &'a mut [f64],
    dist2: &'a mut [f64],
}

impl OutChunk<'_> {
    /// The batch kernel: answer every query of this piece. Each result depends only on its
    /// own query, so any split into pieces gives identical output.
    fn fill(&mut self, bvh: &Bvh<'_>, q: &Points) {
        let s = self.start;
        let n = self.tri.len();
        let (qx, qy, qz) = (&q.xs[s..s + n], &q.ys[s..s + n], &q.zs[s..s + n]);
        for k in 0..n {
            let hit = bvh.nearest([qx[k], qy[k], qz[k]]);
            self.tri[k] = hit.tri;
            self.x[k] = hit.point[0];
            self.y[k] = hit.point[1];
            self.z[k] = hit.point[2];
            self.dist2[k] = hit.dist2;
        }
    }
}

/// For every query point, find the closest point on the mesh. One batch call on one
/// thread; counted as [`Op::ClosestPoints`] with one item per query.
///
/// # Errors
/// Returns [`GeomError::LengthMismatch`] when `out` does not have one slot per query.
pub fn closest_points(bvh: &Bvh<'_>, queries: &Points, out: &mut ClosestHits) -> Result<()> {
    out.check_len(queries.len())?;
    counters::record(Op::ClosestPoints, queries.len());
    let n = queries.len();
    if n == 0 {
        return Ok(());
    }
    for mut c in out.split_mut(1) {
        c.fill(bvh, queries);
    }
    Ok(())
}

/// Like [`closest_points`] but spread over up to `threads` threads in coarse pieces of at
/// least [`MIN_POINTS_PER_TASK`] queries. The output is identical for every `threads`.
/// The calling thread is one of the workers; counted once, on the calling thread.
///
/// Each finished batch also leaves a [`ParReport`] on the calling thread (the tasks made and
/// the distinct threads that really ran them); read it with [`counters::take_par`], so a
/// caller can prove it stayed within its thread allocation.
///
/// # Errors
/// Returns [`GeomError::LengthMismatch`] when `out` does not have one slot per query.
pub fn closest_points_par(
    bvh: &Bvh<'_>,
    queries: &Points,
    out: &mut ClosestHits,
    threads: usize,
) -> Result<()> {
    out.check_len(queries.len())?;
    counters::record(Op::ClosestPoints, queries.len());
    let n = queries.len();
    if n == 0 {
        counters::record_par(ParReport {
            tasks: 0,
            threads: 0,
        });
        return Ok(());
    }
    let mut chunks = out.split_mut(task_count(n, threads));
    let tasks = chunks.len();
    let ids = std::thread::scope(|s| {
        let last = chunks.pop();
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|mut c| {
                s.spawn(move || {
                    c.fill(bvh, queries);
                    std::thread::current().id()
                })
            })
            .collect();
        let mut ids = Vec::with_capacity(tasks);
        if let Some(mut c) = last {
            c.fill(bvh, queries);
            ids.push(std::thread::current().id());
        }
        for h in handles {
            ids.push(h.join().unwrap_or_else(|p| std::panic::resume_unwind(p)));
        }
        ids
    });
    let mut distinct = Vec::with_capacity(ids.len());
    for id in ids {
        if !distinct.contains(&id) {
            distinct.push(id);
        }
    }
    counters::record_par(ParReport {
        tasks,
        threads: distinct.len(),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Mesh;
    use crate::testutil::{
        assert_pt, assert_report_within, cube_soup, in_bench_epoch, par_within, plate_soup,
        slot_threads,
    };

    fn pts(v: &[[f64; 3]]) -> Points {
        Points::new(
            v.iter().map(|p| p[0]).collect(),
            v.iter().map(|p| p[1]).collect(),
            v.iter().map(|p| p[2]).collect(),
        )
        .expect("points")
    }

    #[test]
    fn cube_known_answers_inside_outside_edge_corner() {
        let (x, y, z) = cube_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("cube");
        let b = Bvh::build(&m).expect("bvh");
        let q = pts(&[
            [0.5, 0.5, 3.0],  // above the top face
            [-2.0, 0.5, 0.5], // left of the x=0 face
            [2.0, 2.0, 0.5],  // beside the vertical edge at x=1, y=1
            [2.0, 2.0, 2.0],  // beyond the corner (1,1,1)
            [0.25, 0.5, 0.5], // inside: nearest wall is x = 0 at distance 0.25
        ]);
        let mut o = ClosestHits::with_len(q.len());
        closest_points(&b, &q, &mut o).expect("query");
        let want = [
            ([0.5, 0.5, 1.0], 4.0),
            ([0.0, 0.5, 0.5], 4.0),
            ([1.0, 1.0, 0.5], 2.0),
            ([1.0, 1.0, 1.0], 3.0),
            ([0.0, 0.5, 0.5], 0.0625),
        ];
        for (i, (p, d2)) in want.iter().enumerate() {
            assert_pt([o.x[i], o.y[i], o.z[i]], *p);
            assert_eq!(o.dist2[i].to_bits(), f64::to_bits(*d2), "query {i}");
        }
    }

    #[test]
    fn ties_go_to_the_lowest_triangle() {
        // Query above the shared diagonal of the plate: both triangles are equally close.
        let (x, y, z) = plate_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("plate");
        let b = Bvh::build(&m).expect("bvh");
        let q = pts(&[[2.0, 2.0, 1.0]]);
        let mut o = ClosestHits::with_len(1);
        closest_points(&b, &q, &mut o).expect("query");
        assert_eq!(o.tri[0], 0);
        assert_eq!(o.dist2[0].to_bits(), 1.0_f64.to_bits());
    }

    #[test]
    fn output_length_is_checked_and_empty_batches_work() {
        let (x, y, z) = plate_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("plate");
        let b = Bvh::build(&m).expect("bvh");
        let q = pts(&[[0.0, 0.0, 1.0], [1.0, 1.0, 1.0]]);
        let mut short = ClosestHits::with_len(1);
        assert!(matches!(
            closest_points(&b, &q, &mut short),
            Err(GeomError::LengthMismatch {
                expected: 2,
                actual: 1,
                ..
            })
        ));
        assert!(matches!(
            closest_points_par(&b, &q, &mut short, 1),
            Err(GeomError::LengthMismatch { .. })
        ));
        let none = pts(&[]);
        let mut o = ClosestHits::with_len(0);
        closest_points(&b, &none, &mut o).expect("empty");
        closest_points_par(&b, &none, &mut o, 1).expect("empty par");
        assert!(o.is_empty() && none.is_empty());
    }

    #[test]
    fn points_reject_bad_input() {
        assert!(matches!(
            Points::new(vec![0.0], vec![0.0, 1.0], vec![0.0]),
            Err(GeomError::LengthMismatch { .. })
        ));
        assert!(matches!(
            Points::new(vec![0.0, f64::NAN], vec![0.0; 2], vec![0.0; 2]),
            Err(GeomError::NonFinite {
                what: "query xs",
                index: 1
            })
        ));
    }

    #[test]
    fn every_parallel_task_meets_the_minimum_size() {
        let m = MIN_POINTS_PER_TASK;
        let ns = [
            1,
            m - 1,
            m,
            m + 1,
            2 * m - 1,
            2 * m,
            2 * m + 1,
            3 * m - 1,
            3 * m,
            3 * m + 1,
            40 * m + 1,
            41 * m - 1,
        ];
        for threads in [0, 1, 2, 3, 8, 40, 64] {
            for n in ns {
                let tasks = task_count(n, threads);
                assert!(
                    tasks >= 1 && tasks <= threads.max(1),
                    "n {n} threads {threads}"
                );
                if tasks > 1 {
                    assert!(n / tasks >= m, "n {n} threads {threads} tasks {tasks}");
                }
                let mut o = ClosestHits::with_len(n);
                let sizes: Vec<(usize, usize)> = o
                    .split_mut(tasks)
                    .iter()
                    .map(|c| (c.start, c.tri.len()))
                    .collect();
                assert_eq!(sizes.len(), tasks);
                let mut next = 0;
                for &(start, len) in &sizes {
                    assert_eq!(start, next, "n {n} threads {threads}");
                    if tasks > 1 {
                        assert!(len >= m, "n {n} threads {threads}: task of {len}");
                    }
                    next += len;
                }
                assert_eq!(next, n);
            }
        }
        // Fewer than two minimum tasks always runs as one task.
        assert_eq!(task_count(2 * m - 1, 8), 1);
        assert_eq!(task_count(2 * m, 8), 2);
        assert_eq!(task_count(100 * m, 8), 8);
    }

    fn boundary_points(n: usize) -> Points {
        Points::new(
            (0..n).map(|i| (i % 97) as f64 * 0.05 - 1.0).collect(),
            (0..n).map(|i| (i % 89) as f64 * 0.07 - 0.5).collect(),
            (0..n).map(|i| (i % 13) as f64 * 0.3 - 1.0).collect(),
        )
        .expect("points")
    }

    /// Parallel output equals serial output at every `threads` in `counts`, for batch
    /// sizes around the task boundaries; each run must make exactly the expected tasks.
    fn check_parallel_equals_serial(limit: usize, counts: &[usize], ns: &[usize]) {
        let (x, y, z) = plate_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("plate");
        let b = Bvh::build(&m).expect("bvh");
        for &n in ns {
            let q = boundary_points(n);
            let mut a = ClosestHits::with_len(n);
            closest_points(&b, &q, &mut a).expect("serial");
            for &threads in counts {
                let mut p = ClosestHits::with_len(n);
                let r = par_within(limit, &b, &q, &mut p, threads);
                assert_eq!(a, p, "n {n} threads {threads}");
                let want = task_count(n, threads);
                assert_eq!(
                    (r.tasks, r.threads),
                    (want, want),
                    "n {n} threads {threads}"
                );
            }
        }
    }

    #[test]
    fn threads_parallel_equals_serial_around_the_task_boundaries() {
        let alloc = slot_threads();
        let k = MIN_POINTS_PER_TASK;
        let counts: Vec<usize> = (1..=alloc).collect();
        check_parallel_equals_serial(
            alloc,
            &counts,
            &[k - 1, k, k + 1, 2 * k - 1, 2 * k, 2 * k + 1, 3 * k + 7],
        );
    }

    /// The 3 and 8 worker cases. They start more workers than a writing slot may use, so
    /// they run only in the bench epoch: the `bench` nextest profile selects them, the
    /// default profile's `default-filter` skips them and nextest counts them as skipped.
    /// Started outside the bench epoch, the test fails; it never passes without running.
    #[test]
    fn threads_bench_parallel_equals_serial_at_3_and_8_workers() {
        assert!(
            in_bench_epoch() && slot_threads() >= 8,
            "bench-epoch test started outside the bench epoch (profile bench, 8 threads)"
        );
        let k = MIN_POINTS_PER_TASK;
        check_parallel_equals_serial(
            8,
            &[3, 8],
            &[k - 1, 2 * k, 3 * k + 7, 8 * k - 1, 8 * k, 8 * k + 5],
        );
    }

    /// Allocation witness: a query given the whole allocation makes exactly that many tasks
    /// on exactly that many distinct threads, and the check fails for anything above it.
    #[test]
    fn threads_witness_query_uses_exactly_the_allocation() {
        let alloc = slot_threads();
        let (x, y, z) = plate_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("plate");
        let b = Bvh::build(&m).expect("bvh");
        let n = 40 * MIN_POINTS_PER_TASK;
        let q = boundary_points(n);
        let mut o = ClosestHits::with_len(n);
        let r = par_within(alloc, &b, &q, &mut o, alloc);
        assert_eq!((r.tasks, r.threads), (alloc, alloc));
    }

    #[test]
    #[should_panic(expected = "allowed")]
    fn witness_fails_a_request_above_the_allocation() {
        let alloc = slot_threads();
        let (x, y, z) = plate_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("plate");
        let b = Bvh::build(&m).expect("bvh");
        let q = boundary_points(4);
        let mut o = ClosestHits::with_len(4);
        // Refused before any worker starts.
        par_within(alloc, &b, &q, &mut o, alloc + 1);
    }

    #[test]
    #[should_panic(expected = "allowed")]
    fn witness_fails_a_report_above_the_allocation() {
        let alloc = slot_threads();
        assert_report_within(
            alloc,
            ParReport {
                tasks: alloc + 1,
                threads: alloc + 1,
            },
        );
    }

    #[test]
    fn threads_counters_see_one_call_per_batch_not_per_item() {
        let (x, y, z) = plate_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("plate");
        let b = Bvh::build(&m).expect("bvh");
        let n = 2500;
        let q = Points::new(
            (0..n).map(|i| f64::from(i) * 0.001).collect(),
            vec![1.0; n as usize],
            vec![2.0; n as usize],
        )
        .expect("points");
        let mut o = ClosestHits::with_len(q.len());
        counters::reset();
        closest_points(&b, &q, &mut o).expect("serial");
        par_within(slot_threads(), &b, &q, &mut o, slot_threads());
        let c = counters::snapshot(Op::ClosestPoints);
        assert_eq!((c.calls, c.items), (2, 5000));
        assert_eq!(c.mean_items_per_call(), Some(2500.0));
    }
}
