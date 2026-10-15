//! The execution budget, across the process boundary (2026-10-10).
//!
//! `plinth prepare` runs inside ONE pool whose width is `PLINTH_THREADS`, or
//! else the auto-detected default. This test pins what no in-process test can:
//!
//! * the process never grows past `k` workers + main + slack, even when
//!   `RAYON_NUM_THREADS` asks for more (the old layout was `1 + k + 16`);
//! * the kernel's batches ran `inline` in that pool (`cross` and `cold` read
//!   zero in the `[threads]` line, a measured path-hit count and not a claim);
//! * the pose parallelism ran (two `[orient-demand]` sweep lines);
//! * the default path (no `PLINTH_THREADS`) logs the detected width and its
//!   source, and the override path logs `PLINTH_THREADS`;
//! * the emitted `.chitubox` is BYTE-IDENTICAL at 1, 3, 4 and the default
//!   width (the serial oracle, installed, against the parallel runs).
//!
//! One model, four process runs. Sabotage (recorded once by hand in the lane
//! notes): bypass `install_budget` in `main.rs`; this test must then fail on the
//! peak thread count and on `cross`/`cold` above zero.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const MODEL: &str = "make_thin_fin.prepped.stl";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("plinth-budget-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("in")).expect("scratch input folder");
        std::fs::copy(
            workspace_root().join("corpus/baseline").join(MODEL),
            path.join("in/mini.stl"),
        )
        .expect("place committed fixture");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// OS threads of a live process, or 0 when it cannot be read.
fn thread_count(pid: u32) -> usize {
    if cfg!(target_os = "linux") {
        std::fs::read_dir(format!("/proc/{pid}/task")).map_or(0, Iterator::count)
    } else {
        // `ps -M` prints a header and then one line per thread.
        Command::new("ps")
            .args(["-M", "-p", &pid.to_string()])
            .output()
            .map_or(0, |out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .count()
                    .saturating_sub(1)
            })
    }
}

struct Run {
    stderr: String,
    peak_threads: usize,
    artifact: Vec<u8>,
}

/// One `plinth prepare` run. `threads` of `None` leaves `PLINTH_THREADS` unset.
fn prepare(scratch: &Scratch, label: &str, threads: Option<&str>, rayon_threads: &str) -> Run {
    let out_dir = scratch.0.join(format!("out-{label}"));
    let stdout_path = scratch.0.join(format!("{label}.out"));
    let stderr_path = scratch.0.join(format!("{label}.err"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_plinth"));
    command
        .args(["prepare", "--input-models-dir"])
        .arg(scratch.0.join("in"))
        .args([
            "--output",
            "saturn4-ultra",
            "--orient",
            "dense",
            "--placement",
            "sweep",
        ])
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--no-cache")
        .env("RAYON_NUM_THREADS", rayon_threads)
        .stdout(Stdio::from(
            std::fs::File::create(&stdout_path).expect("stdout file"),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(&stderr_path).expect("stderr file"),
        ));
    match threads {
        Some(value) => command.env("PLINTH_THREADS", value),
        None => command.env_remove("PLINTH_THREADS"),
    };
    let mut child = command.spawn().expect("spawn plinth");
    let pid = child.id();
    let mut peak_threads = 0;
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll plinth") {
            break status;
        }
        peak_threads = peak_threads.max(thread_count(pid));
        std::thread::sleep(Duration::from_millis(100));
    };
    let stdout = std::fs::read_to_string(&stdout_path).expect("stdout");
    let stderr = std::fs::read_to_string(&stderr_path).expect("stderr");
    assert_eq!(
        status.code(),
        Some(0),
        "{label}: stdout {stdout}\nstderr {stderr}"
    );
    let artifact = std::fs::read(out_dir.join("mini.chitubox")).expect("emitted artifact");
    Run {
        stderr,
        peak_threads,
        artifact,
    }
}

/// The `[threads]` line of a run, or a failure that shows the whole log.
fn threads_line(run: &Run) -> &str {
    run.stderr
        .lines()
        .find(|line| line.starts_with("[threads] "))
        .unwrap_or_else(|| panic!("no [threads] line in: {}", run.stderr))
}

/// `inline N, cross C, cold D` from the line.
fn path_hits(line: &str) -> (u64, u64, u64) {
    let tail = line.rsplit("): ").next().expect("line tail");
    let number = |name: &str| -> u64 {
        let after = tail
            .split(&format!("{name} "))
            .nth(1)
            .unwrap_or_else(|| panic!("{name} missing in {line}"));
        after
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|digits| digits.parse().ok())
            .unwrap_or_else(|| panic!("{name} not a number in {line}"))
    };
    (number("inline"), number("cross"), number("cold"))
}

/// What the operating system itself says the default should be.
fn expected_default() -> (usize, &'static str) {
    if cfg!(target_os = "macos") {
        let perf = Command::new("/usr/sbin/sysctl")
            .args(["-n", "hw.perflevel0.physicalcpu"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .and_then(|text| text.trim().parse::<usize>().ok())
            .filter(|&n| n > 0);
        if let Some(perf) = perf {
            return (perf, "sysctl hw.perflevel0.physicalcpu");
        }
    }
    let all = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    (all, "available_parallelism")
}

#[test]
fn one_budget_pool_holds_the_thread_count_and_the_bytes_at_every_width() {
    let scratch = Scratch::new("budget");

    // Override path, with a rayon width that would add 7 global workers if the
    // budget were bypassed.
    let three = prepare(&scratch, "t3", Some("3"), "7");
    assert!(
        three.peak_threads > 0,
        "the thread poll must have read the process at least once"
    );
    assert!(
        three.peak_threads <= 5,
        "3 workers + main + 1 slack, got {} (RAYON_NUM_THREADS=7 must be ignored)",
        three.peak_threads
    );
    let line = threads_line(&three);
    assert!(
        line.starts_with("[threads] budget 3 (PLINTH_THREADS): "),
        "{line}"
    );
    let (inline, cross, cold) = path_hits(line);
    assert!(
        inline > 0,
        "kernel batches ran inline in the budget pool: {line}"
    );
    assert_eq!(
        (cross, cold),
        (0, 0),
        "no batch left the budget pool: {line}"
    );
    let sweeps = three.stderr.matches("[orient-demand]").count();
    assert!(
        sweeps >= 2,
        "prefilter and production sweeps ran: {}",
        three.stderr
    );

    // The serial oracle, installed, and a second parallel width.
    let one = prepare(&scratch, "t1", Some("1"), "7");
    assert!(threads_line(&one).starts_with("[threads] budget 1 (PLINTH_THREADS): "));
    let four = prepare(&scratch, "t4", Some("4"), "7");
    assert!(threads_line(&four).starts_with("[threads] budget 4 (PLINTH_THREADS): "));

    // Default path: the detected width, its source, and the same bytes.
    let (width, source) = expected_default();
    let default = prepare(&scratch, "default", None, "2");
    let line = threads_line(&default);
    assert!(
        line.starts_with(&format!("[threads] budget {width} ({source}): ")),
        "expected the detected default {width} from {source}: {line}"
    );
    let (_, cross, cold) = path_hits(line);
    assert_eq!((cross, cold), (0, 0), "{line}");
    assert!(
        default.peak_threads <= width + 2,
        "default width {width}: peak {} threads",
        default.peak_threads
    );

    eprintln!(
        "thread_budget: peak OS threads {} at PLINTH_THREADS=3, {} at the default width {width}; \
         {}; {}",
        three.peak_threads,
        default.peak_threads,
        threads_line(&three),
        threads_line(&default),
    );
    assert!(!three.artifact.is_empty());
    for (label, run) in [("1", &one), ("4", &four), ("default", &default)] {
        assert!(
            three.artifact == run.artifact,
            "the .chitubox at {label} thread(s) differs from the one at 3"
        );
    }
}
