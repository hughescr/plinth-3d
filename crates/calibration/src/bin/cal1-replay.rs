//! Read-only CAL-1 prediction job. Never writes a native project.
use std::path::Path;
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [project, json, markdown] => plinth_calibration::replay::run(Path::new(project), Path::new(json), Path::new(markdown)),
        _ => Err("usage: cal1-replay <read-only-frozen-project.chitubox> <predictions.json> <predictions.md>".into()),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("CAL-1 replay: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
