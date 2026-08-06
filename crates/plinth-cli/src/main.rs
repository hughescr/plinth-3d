use std::process::ExitCode;

use plinth_cli::{parse_args, CliAction, PREPARE_USAGE, ROOT_USAGE};

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
        Ok(CliAction::Prepare(_selection)) => {
            eprintln!("plinth prepare: execution is not implemented in this parser milestone");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("plinth: {error}\n\n{ROOT_USAGE}");
            ExitCode::from(2)
        }
    }
}
