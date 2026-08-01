//! The one future Plinth entry point.  M1 scaffolds its stable binary name only;
//! scoring is deliberately unavailable until the complete harness lands.

use std::process::ExitCode;

fn main() -> ExitCode {
    let argument = std::env::args().nth(1);
    match argument.as_deref() {
        None | Some("--help") | Some("-h") => {
            println!("plinth {}\n\nUsage: plinth <subcommand>\n\nSubcommands are not implemented in this scaffold.", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--version") | Some("-V") => {
            println!("plinth {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(subcommand) => {
            eprintln!("plinth: subcommand `{subcommand}` is not implemented in this milestone");
            ExitCode::from(2)
        }
    }
}
