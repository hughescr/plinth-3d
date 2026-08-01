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

fn pool_for(k: usize) -> Arc<ThreadPool> {
    let mut guard = pools().lock().unwrap();
    if let Some((_, p)) = guard.iter().find(|(kk, _)| *kk == k) {
        return p.clone();
    }
    let p = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(k)
            .build()
            .expect("failed to build rayon pool"),
    );
    guard.push((k, p.clone()));
    p
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
