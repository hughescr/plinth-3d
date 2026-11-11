//! CAL-2 calibration plate assembly, never a production print-candidate command.
use std::path::Path;

fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [spec, output] => {
            plinth_calibration::cal2_plate::assemble(Path::new(spec), Path::new(output))
        }
        _ => Err("usage: cal2-plate <cal2.json> <empty-output-directory>".into()),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("CAL-2: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
