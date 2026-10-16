//! Rayon's global pool, which a bare `par_iter` builds lazily, takes its worker stack from
//! `RUST_MIN_STACK` (rayon 1.x leaves the size to `std::thread::Builder`). Its 2 MiB default is
//! too small for the nested pose sweeps a loaded test run stacks on one worker, and a stack
//! overflow aborts the whole test program, not one test. `.cargo/config.toml` sets the variable
//! to the stack the CLI's own pool has; this test fails if that line goes missing.
//!
//! A program started outside cargo (the test binary run by hand) has no such setting and fails
//! here on purpose: run it with `RUST_MIN_STACK=8388608`.

use geom_kernel::par::WORKER_STACK_BYTES;

#[test]
fn the_test_process_gives_rayons_global_pool_the_production_worker_stack() {
    let set = std::env::var("RUST_MIN_STACK").ok().and_then(|v| v.trim().parse::<usize>().ok());
    assert!(
        set.is_some_and(|bytes| bytes >= WORKER_STACK_BYTES),
        "RUST_MIN_STACK is {set:?}, but the global pool's workers need at least {WORKER_STACK_BYTES} \
         bytes (the CLI pool's stack). .cargo/config.toml [env] sets it under cargo; outside cargo \
         run with RUST_MIN_STACK={WORKER_STACK_BYTES}."
    );
}
