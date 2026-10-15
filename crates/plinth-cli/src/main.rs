use std::process::ExitCode;

use plinth_cli::{execute::execute, parse_args, CliAction, PREPARE_USAGE, ROOT_USAGE};

// Optional global allocator, chosen by cargo feature.  The binary only: the
// library crates keep the system allocator.  `alloc-count` wins when both
// features are on (so `--all-features` builds): the counting allocator wraps
// the system allocator.
#[cfg(all(feature = "mimalloc", not(feature = "alloc-count")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "alloc-count")]
mod alloc_count;
#[cfg(feature = "alloc-count")]
#[global_allocator]
static GLOBAL: alloc_count::Counting = alloc_count::Counting;

fn main() -> ExitCode {
    #[cfg(feature = "alloc-count")]
    alloc_count::start();
    let code = run();
    #[cfg(feature = "alloc-count")]
    alloc_count::finish();
    code
}

fn run() -> ExitCode {
    match parse_args(std::env::args_os().skip(1)) {
        Ok(CliAction::RootHelp) => {
            println!("plinth {}\n\n{ROOT_USAGE}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(CliAction::PrepareHelp) => {
            println!("{PREPARE_USAGE}");
            ExitCode::SUCCESS
        }
        Ok(CliAction::Version) => {
            println!("plinth {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        // The exit code from here on is the pipeline's own projection, not a
        // second contract: 0 when every model reached an artifact, 3 whenever
        // the pre-write physics gates refused one -- whatever else the run also
        // hit, because 3 is the code callers act on -- and otherwise the first
        // non-zero stage code in report order. A usage error below still exits 2.
        //
        // The whole run happens inside the one execution-budget pool, so the
        // kernel's batches and every bare `par_iter` share `k` workers instead
        // of a kernel pool plus rayon's global pool.
        Ok(CliAction::Prepare(selection)) => {
            let (code, budget) = geom_kernel::par::install_budget(move || execute(selection));
            if let Err(error) = budget {
                eprintln!(
                    "[threads] WARNING budget pool not built ({error}); per-call pools were used"
                );
            }
            code
        }
        Err(error) => {
            eprintln!("plinth: {error}\n\n{ROOT_USAGE}");
            ExitCode::from(2)
        }
    }
}
