//! Deterministic rayon parallelism (spec sec 4). The kernel NEVER uses rayon's
//! global pool (Amendment A4 — a one-shot global pool would make the
//! thread-sweep test inert). It keeps a cache of explicit per-count pools; each
//! batch call re-reads PLINTH_THREADS and runs under the matching pool.
//!
//! The only sanctioned pattern is index-parallel map-collect: every output lands
//! in the slot of its own index, so results are bit-identical for any thread
//! count. No float reduce/fold/sum across queries, ever (sec 4.2).

use rayon::ThreadPool;
use std::sync::{Arc, Mutex, OnceLock};

// A Vec (not a HashMap — sec 4.2 / R6) of (k, pool); linear scan over the few
// distinct thread counts a process ever uses.
type PoolList = Mutex<Vec<(usize, Arc<ThreadPool>)>>;
static POOLS: OnceLock<PoolList> = OnceLock::new();

fn pools() -> &'static PoolList {
    POOLS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Effective thread count the next batch call will use: PLINTH_THREADS if
/// set and > 0, else all available cores (the multi-core mandate).
pub fn current_threads() -> usize {
    match std::env::var("PLINTH_THREADS") {
        Ok(s) => match s.trim().parse::<usize>() {
            Ok(0) | Err(_) => all_cores(),
            Ok(k) => k,
        },
        Err(_) => all_cores(),
    }
}

fn all_cores() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .max(1)
}

/// A pool of the requested width could not be constructed.
///
/// Carries Rayon's own build-failure text rather than a bare unit so a caller
/// that surfaces this in its own error type keeps the reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolBuildError(String);

impl std::fmt::Display for PoolBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PoolBuildError {}

fn try_pool_for(k: usize) -> Result<Arc<ThreadPool>, PoolBuildError> {
    let mut guard = pools().lock().unwrap();
    if let Some((_, p)) = guard.iter().find(|(kk, _)| *kk == k) {
        return Ok(p.clone());
    }
    let p = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(k)
            .build()
            .map_err(|error| PoolBuildError(error.to_string()))?,
    );
    guard.push((k, p.clone()));
    // Released before returning: the cache is now shared by every caller that
    // used to build a private pool, so holding it past the last use would
    // serialise them on a lock they no longer need.
    drop(guard);
    Ok(p)
}

fn pool_for(k: usize) -> Arc<ThreadPool> {
    try_pool_for(k).expect("failed to build rayon pool")
}

/// Number of workers in the explicit pool selected for the next batch.
/// Unlike [`current_threads`], this proves the pool was constructed with the
/// requested width; it does not claim Rayon scheduled work on every worker.
pub(crate) fn configured_pool_threads() -> usize {
    pool_for(current_threads()).current_num_threads()
}

/// Run `f` under the pool for the current thread count. `f` typically issues an
/// index-parallel `par_iter().map().collect()` (the sanctioned pattern).
pub fn run<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    let k = current_threads();
    let pool = pool_for(k);
    pool.install(f)
}

/// [`run`], but surfacing a pool-construction failure instead of panicking.
///
/// This exists for callers OUTSIDE the kernel that were each building a private
/// `ThreadPoolBuilder` per call at the same `PLINTH_THREADS` width this module
/// already caches. Measured on `a4_transactional_controller`: constructing and
/// dropping those private pools was 1,294 of 9,491 samples (13.6%) of the test
/// thread's wall time, all of it `pthread_create`/teardown rather than work.
/// Sharing the cache removes that without changing a result: the sanctioned
/// pattern is still index-parallel map-collect, the width still re-reads
/// `PLINTH_THREADS` on every call, and the pool is still explicit rather than
/// rayon's global one (Amendment A4).
pub fn try_run<F, R>(f: F) -> Result<R, PoolBuildError>
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    let k = current_threads();
    let pool = try_pool_for(k)?;
    Ok(pool.install(f))
}
