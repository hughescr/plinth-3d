//! CAL-1 calibration assembly, never a production print-candidate command.
use std::path::Path;
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [spec, input, measured, output] => plinth_calibration::plate_report::assemble(
            Path::new(spec),
            Path::new(input),
            Path::new(measured),
            Path::new(output),
        ),
        _ => Err("usage: cal1-plate <cal1.json> <read-only-individual-directory> <measured-summary.json> <empty-output-directory>".into()),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("CAL-1: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
