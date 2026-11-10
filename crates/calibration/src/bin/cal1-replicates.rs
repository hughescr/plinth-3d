//! R-C4 replacement job; archive inputs before running.
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = match args.as_slice() {
        [out] => plinth_calibration::replicates::run(std::path::Path::new(out)),
        _ => Err("usage: cal1-replicates PLATE_DIRECTORY_WITH_superseded-A1b".into()),
    };
    if let Err(e) = result {
        eprintln!("CAL-1 replacement: {e}");
        std::process::exit(1);
    }
}
