use std::process::ExitCode;

use plinth_cli::{execute::execute, parse_args, CliAction, PREPARE_USAGE, ROOT_USAGE};

fn main() -> ExitCode {
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
        // second contract: 0 when every model reached an artifact, 3 when the
        // pre-write physics gates refused one, and the stage's own code
        // otherwise. A usage error below still exits 2.
        Ok(CliAction::Prepare(selection)) => execute(selection),
        Err(error) => {
            eprintln!("plinth: {error}\n\n{ROOT_USAGE}");
            ExitCode::from(2)
        }
    }
}
