//! Turns the `slow-tests` switch on from the environment. Test-only: nothing under `src/` reads it.
//!
//! The slow tests in `tests/it` are `#[cfg_attr(not(feature = "slow-tests"), ignore = "...")]`, or
//! their file sits under `#[cfg(feature = "slow-tests")]`. A Cargo feature would be the plain
//! switch, but `scripts/test-fast.sh` and `scripts/gate-full.sh` run one `cargo test -p <crate>`
//! per test program, and `--features plinth-sweep/slow-tests` is an error for a crate that does not
//! depend on plinth-sweep directly (plinth-cli, say) while a build without it would rebuild
//! plinth-sweep and everything above it for every program. An environment variable is the same
//! for every one of those cargo calls, so they all agree and nothing rebuilds.
//!
//! `PLINTH_SLOW_TESTS=1 cargo test ...` runs the slow tests; without it they are ignored or not
//! built. `--features slow-tests` still works and means the same.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=PLINTH_SLOW_TESTS");
    let on = std::env::var("PLINTH_SLOW_TESTS").is_ok_and(|v| !v.is_empty() && v != "0");
    if on {
        println!("cargo::rustc-cfg=feature=\"slow-tests\"");
    }
}
