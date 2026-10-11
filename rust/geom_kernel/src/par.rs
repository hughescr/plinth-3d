//! Deterministic rayon parallelism (spec sec 4). The kernel NEVER uses rayon's
//! global pool (Amendment A4 — a one-shot global pool would make the
//! thread-sweep test inert). It keeps a cache of explicit per-count pools; each
//! batch call re-reads PLINTH_THREADS and runs under the matching pool.
//!
//! The only sanctioned pattern is index-parallel map-collect: every output lands
//! in the slot of its own index, so results are bit-identical for any thread
//! count. No float reduce/fold/sum across queries, ever (sec 4.2).
//!
//! # One execution budget (2026-10-10)
//!
//! A process that calls [`install_budget`] (the `plinth prepare` binary does)
//! runs its whole body INSIDE one `k`-wide pool, and that pool is also the
//! cached pool for width `k`. Every kernel batch ([`run`], [`try_run`]) and
//! every bare `par_iter` in the engine then lands in the same registry, so the
//! process holds `k` workers plus the sleeping main thread, never `1 + k + 16`
//! (a kernel pool plus rayon's global pool). The pool sets its own width, so
//! `RAYON_NUM_THREADS` is ignored by the binary.
//!
//! # Default width
//!
//! `PLINTH_THREADS` (a positive integer) wins. Otherwise the width is detected
//! once per process, never hard-coded: on macOS the performance-core count
//! (`sysctl hw.perflevel0.physicalcpu`), and on any failure, or elsewhere,
//! [`std::thread::available_parallelism`]. [`Budget::source`] says which one
//! decided, so the run log can show it.

use rayon::{ThreadBuilder, ThreadPool};
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

// A Vec (not a HashMap — sec 4.2 / R6) of (k, pool); linear scan over the few
// distinct thread counts a process ever uses.
type PoolList = Mutex<Vec<(usize, Arc<ThreadPool>)>>;
static POOLS: OnceLock<PoolList> = OnceLock::new();

fn pools() -> &'static PoolList {
    POOLS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Who decided the thread count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetSource {
    /// `PLINTH_THREADS` held a positive integer.
    Env,
    /// macOS: `sysctl hw.perflevel0.physicalcpu` (performance cores).
    PerfCores,
    /// `std::thread::available_parallelism` (not macOS, or the sysctl failed).
    AllCores,
}

impl BudgetSource {
    /// Text for the run log.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Env => "PLINTH_THREADS",
            Self::PerfCores => "sysctl hw.perflevel0.physicalcpu",
            Self::AllCores => "available_parallelism",
        }
    }
}

/// A thread count and where it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub threads: usize,
    pub source: BudgetSource,
}

/// The width the next batch call will use, with its source: `PLINTH_THREADS`
/// if set to a positive integer, else the detected default.
pub fn resolve_threads() -> Budget {
    resolve_with(
        std::env::var("PLINTH_THREADS").ok().as_deref(),
        default_budget(),
    )
}

/// Effective thread count the next batch call will use: PLINTH_THREADS if
/// set and > 0, else the detected default (see the module doc).
pub fn current_threads() -> usize {
    resolve_threads().threads
}

/// The override rule, apart from the environment so a test can drive it.
fn resolve_with(env: Option<&str>, default: Budget) -> Budget {
    match env.and_then(|text| text.trim().parse::<usize>().ok()) {
        Some(threads) if threads > 0 => Budget {
            threads,
            source: BudgetSource::Env,
        },
        _ => default,
    }
}

/// The detected default, found once per process (the sysctl is a child
/// process; the answer cannot change while the process runs).
fn default_budget() -> Budget {
    static DEFAULT: OnceLock<Budget> = OnceLock::new();
    *DEFAULT.get_or_init(|| {
        choose_default(
            perf_core_count(),
            std::thread::available_parallelism().ok().map(|n| n.get()),
        )
    })
}

/// Pick the default from the two detections: performance cores when known,
/// else all cores, else 1. Never zero.
fn choose_default(perf_cores: Option<usize>, available: Option<usize>) -> Budget {
    match perf_cores {
        Some(threads) if threads > 0 => Budget {
            threads,
            source: BudgetSource::PerfCores,
        },
        _ => Budget {
            threads: available.unwrap_or(1).max(1),
            source: BudgetSource::AllCores,
        },
    }
}

/// Read the sysctl text: one positive integer, surrounding whitespace allowed.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_core_count(text: &str) -> Option<usize> {
    text.trim().parse::<usize>().ok().filter(|&n| n > 0)
}

#[cfg(target_os = "macos")]
fn perf_core_count() -> Option<usize> {
    // Absolute path: the lookup must not depend on PATH. Any failure (no such
    // key on an Intel Mac, a sandbox denial, odd output) is `None`, and the
    // caller falls back to `available_parallelism`.
    let output = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "hw.perflevel0.physicalcpu"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_core_count(std::str::from_utf8(&output.stdout).ok()?)
}

#[cfg(not(target_os = "macos"))]
const fn perf_core_count() -> Option<usize> {
    None
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

// ---------------------------------------------------------------------------
// The execution budget: one pool for the whole process.
// ---------------------------------------------------------------------------

static BUDGET: OnceLock<Budget> = OnceLock::new();

// Path-hit counters (telemetry only: relaxed atomics, never read by engine
// logic, so they cannot move a result).
static INLINE: AtomicU64 = AtomicU64::new(0);
static CROSS: AtomicU64 = AtomicU64::new(0);
static COLD: AtomicU64 = AtomicU64::new(0);

/// How the kernel batch calls reached their pool since the process started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetCounts {
    /// The installed budget, or `None` when the process never installed one.
    pub installed: Option<Budget>,
    /// The caller already ran on a worker of the target pool.
    pub inline: u64,
    /// The caller was a worker of some OTHER pool.
    pub cross: u64,
    /// The caller was not a rayon worker at all.
    pub cold: u64,
}

/// Read the path-hit counters.
pub fn budget_counts() -> BudgetCounts {
    BudgetCounts {
        installed: BUDGET.get().copied(),
        inline: INLINE.load(Ordering::Relaxed),
        cross: CROSS.load(Ordering::Relaxed),
        cold: COLD.load(Ordering::Relaxed),
    }
}

fn classify(pool: &ThreadPool) {
    let counter = if pool.current_thread_index().is_some() {
        &INLINE
    } else if rayon::current_thread_index().is_some() {
        &CROSS
    } else {
        &COLD
    };
    counter.fetch_add(1, Ordering::Relaxed);
}

/// The ONE way a budget pool is built. `spawn` starts each worker; production
/// passes [`spawn_std`], a test passes a handler that fails on purpose.
///
/// No `use_current_thread()`: that option writes the caller's thread-local
/// registry pointer before the other workers are spawned, and a later spawn
/// failure leaves it pointing at the terminated registry (rayon-core 1.13
/// `registry.rs` 303-314, 282-283). A plain `build()` leaves the caller
/// untouched on failure, which is what the fallback in [`install_budget`] needs.
fn build_budget_with<F>(k: usize, spawn: F) -> Result<ThreadPool, PoolBuildError>
where
    F: FnMut(ThreadBuilder) -> io::Result<()>,
{
    rayon::ThreadPoolBuilder::new()
        .num_threads(k)
        // 8 MiB = main's stack: the top-level code now runs on a worker.
        .stack_size(8 << 20)
        .thread_name(|index| format!("plinth-{index}"))
        .spawn_handler(spawn)
        .build()
        .map_err(|error| PoolBuildError(error.to_string()))
}

/// Start one worker on a std thread with the name and stack the builder asks for.
fn spawn_std(worker: ThreadBuilder) -> io::Result<()> {
    let mut builder = std::thread::Builder::new();
    if let Some(name) = worker.name() {
        builder = builder.name(name.to_owned());
    }
    if let Some(size) = worker.stack_size() {
        builder = builder.stack_size(size);
    }
    builder.spawn(|| worker.run()).map(drop)
}

/// Run `f` inside the process's one `k`-wide budget pool (`k` from
/// [`resolve_threads`]) and make that pool the cached pool for width `k`.
///
/// The calling thread blocks without spinning while a worker runs `f`. If the
/// pool cannot be built, `f` runs on the calling thread exactly as it did
/// before this existed (per-call cached pools) and the build error is returned
/// so the caller can warn (emit and warn, never refuse).
pub fn install_budget<R, F>(f: F) -> (R, Result<Budget, PoolBuildError>)
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    let budget = resolve_threads();
    match build_budget_with(budget.threads, spawn_std) {
        Ok(pool) => {
            let pool = Arc::new(pool);
            {
                let mut guard = pools().lock().unwrap();
                guard.retain(|(k, _)| *k != budget.threads);
                guard.push((budget.threads, pool.clone()));
            }
            let _ = BUDGET.set(budget);
            (pool.install(f), Ok(budget))
        }
        Err(error) => (f(), Err(error)),
    }
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
    classify(&pool);
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
    classify(&pool);
    Ok(pool.install(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;

    fn budget(threads: usize, source: BudgetSource) -> Budget {
        Budget { threads, source }
    }

    #[test]
    fn override_wins_only_when_a_positive_integer() {
        let default = budget(12, BudgetSource::PerfCores);
        assert_eq!(
            resolve_with(Some("3"), default),
            budget(3, BudgetSource::Env)
        );
        assert_eq!(
            resolve_with(Some(" 7\n"), default),
            budget(7, BudgetSource::Env)
        );
        for bad in ["0", "", "abc", "-2", "1.5"] {
            assert_eq!(resolve_with(Some(bad), default), default, "{bad:?}");
        }
        assert_eq!(resolve_with(None, default), default);
    }

    #[test]
    fn default_prefers_perf_cores_then_falls_back_and_is_never_zero() {
        assert_eq!(
            choose_default(Some(12), Some(16)),
            budget(12, BudgetSource::PerfCores)
        );
        assert_eq!(
            choose_default(None, Some(8)),
            budget(8, BudgetSource::AllCores)
        );
        // A zero from the sysctl is a failure, not a width.
        assert_eq!(
            choose_default(Some(0), Some(8)),
            budget(8, BudgetSource::AllCores)
        );
        assert_eq!(
            choose_default(None, None),
            budget(1, BudgetSource::AllCores)
        );
        assert_eq!(
            choose_default(None, Some(0)),
            budget(1, BudgetSource::AllCores)
        );
    }

    #[test]
    fn sysctl_text_parses_one_positive_integer() {
        assert_eq!(parse_core_count("12\n"), Some(12));
        assert_eq!(parse_core_count(" 4 "), Some(4));
        for bad in ["", "0", "-1", "abc", "12 13", "sysctl: unknown oid"] {
            assert_eq!(parse_core_count(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn detected_default_is_positive_and_stable() {
        let first = default_budget();
        assert!(first.threads > 0);
        assert_ne!(first.source, BudgetSource::Env);
        assert_eq!(default_budget(), first);
        #[cfg(target_os = "macos")]
        if let Some(perf) = perf_core_count() {
            assert_eq!(first, budget(perf, BudgetSource::PerfCores));
        }
    }

    /// The R2 scenario: worker 1 fails to start after worker 0 did. The build
    /// must fail cleanly, leave the caller outside every pool, register
    /// nothing, and leave `run` working. Sabotage (recorded once by hand): add
    /// `.use_current_thread()` to `build_budget_with`; the
    /// `current_thread_index` assertion must then fail.
    #[test]
    fn a_failed_budget_build_leaves_no_trace_and_run_still_works() {
        let result = build_budget_with(3, |worker| {
            if worker.index() == 1 {
                Err(io::Error::other("injected"))
            } else {
                std::thread::Builder::new().spawn(|| worker.run()).map(drop)
            }
        });
        assert!(result.is_err(), "the injected spawn failure must surface");
        assert_eq!(
            rayon::current_thread_index(),
            None,
            "the caller must not be left registered in a dead pool"
        );
        assert!(
            BUDGET.get().is_none(),
            "a failed build must not set the budget"
        );
        let parallel: Vec<u64> = run(|| (0..1000_u64).into_par_iter().map(|i| i * i + 1).collect());
        let serial: Vec<u64> = (0..1000_u64).map(|i| i * i + 1).collect();
        assert_eq!(parallel, serial);
    }

    #[test]
    fn a_built_budget_pool_has_the_named_width_and_workers() {
        let pool = build_budget_with(3, spawn_std).expect("budget pool");
        assert_eq!(pool.current_num_threads(), 3);
        let names: Vec<Option<String>> = pool.install(|| {
            (0..3)
                .into_par_iter()
                .with_max_len(1)
                .map(|_| std::thread::current().name().map(str::to_owned))
                .collect()
        });
        assert!(names
            .iter()
            .all(|name| name.as_deref().is_some_and(|n| n.starts_with("plinth-"))));
    }
}
