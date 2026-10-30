//! Run one command in its own process group with a deadline, a streamed log and lock-wait
//! detection (design dev-loop.md section 3.2). The group is the unit: TERM, then KILL after a
//! grace period, then a check that no member is left.

use crate::lockwatch::LockWatch;
use crate::sys;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Spec {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    /// Stdout and stderr of the command, merged, as the command wrote them.
    pub log: PathBuf,
    /// Send TERM to the group after this long.
    pub term_after: Duration,
    /// Send KILL this long after TERM if the group is still there.
    pub grace: Duration,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum End {
    /// The command ended by itself: exit code, or 128 + signal.
    Exited(i32),
    TimedOut,
    /// A wait on a build directory lock; the group was stopped at once.
    LockWait,
    /// The runner was asked to stop (signal or aggregate deadline).
    Interrupted,
}

#[derive(Debug)]
pub struct Ran {
    pub end: End,
    /// Start to the moment the end was known (before any stopping or cleanup).
    pub elapsed: Duration,
    pub watch: LockWatch,
    /// The command ended but members of its group were still running (killed here).
    pub leaked: bool,
    /// No member of the group exists any more.
    pub group_gone: bool,
    pub pgid: i32,
}

type Line = (Instant, String);

fn exit_code(status: std::process::ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// Stop the group: TERM, wait up to `grace` for it to go, then KILL. True if it is gone.
fn stop_group(child: &mut Child, pgid: i32, grace: Duration) -> bool {
    sys::kill_group(pgid, sys::SIGTERM);
    let start = Instant::now();
    while start.elapsed() < grace {
        let leader_done = child.try_wait().ok().flatten().is_some();
        if leader_done && !sys::group_alive(pgid) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    sys::kill_group(pgid, sys::SIGKILL);
    let _ = child.wait();
    wait_group_gone(pgid, Duration::from_secs(1))
}

fn wait_group_gone(pgid: i32, limit: Duration) -> bool {
    let start = Instant::now();
    while sys::group_alive(pgid) {
        if start.elapsed() >= limit {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
    true
}

/// Feed every line still in flight to the watch; stop when the reader is done or `limit` passes.
fn drain(rx: &Receiver<Line>, watch: &mut LockWatch, limit: Duration) {
    let start = Instant::now();
    while let Some(left) = limit.checked_sub(start.elapsed()) {
        match rx.recv_timeout(left) {
            Ok((at, line)) => watch.feed(at, &line),
            Err(_) => return,
        }
    }
}

fn spawn_reader(
    reader: std::io::PipeReader,
    mut log: File,
) -> (Receiver<Line>, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let mut r = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match r.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let at = Instant::now();
                    let _ = log.write_all(&buf);
                    if tx
                        .send((at, String::from_utf8_lossy(&buf).into_owned()))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });
    (rx, handle)
}

/// Wait for the command to end by itself, or for a reason to stop it.
fn wait_for_end(
    child: &mut Child,
    rx: &Receiver<Line>,
    watch: &mut LockWatch,
    spec: &Spec,
    abort: &dyn Fn() -> bool,
    start: Instant,
) -> End {
    loop {
        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok((at, line)) => {
                watch.feed(at, &line);
                if watch.build_dir {
                    return End::LockWait;
                }
                continue;
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
        }
        if let Ok(Some(status)) = child.try_wait() {
            return End::Exited(exit_code(status));
        }
        if abort() || sys::stop_requested() {
            return End::Interrupted;
        }
        if start.elapsed() >= spec.term_after {
            return End::TimedOut;
        }
    }
}

/// Run the command to its end, a timeout, a build-directory lock wait or an interruption.
pub fn run(spec: &Spec, abort: &dyn Fn() -> bool) -> Result<Ran, String> {
    let log =
        File::create(&spec.log).map_err(|e| format!("cannot write {}: {e}", spec.log.display()))?;
    let (pipe_r, pipe_w) = std::io::pipe().map_err(|e| format!("cannot make a pipe: {e}"))?;
    let (prog, args) = spec.argv.split_first().ok_or("empty command")?;
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .current_dir(&spec.cwd)
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null());
    cmd.stdout(pipe_w.try_clone().map_err(|e| e.to_string())?)
        .stderr(pipe_w)
        .process_group(0);
    let start = Instant::now();
    let spawned = cmd.spawn();
    drop(cmd); // closes the runner's copies of the write end, so the reader sees EOF
    let mut child = spawned.map_err(|e| format!("cannot start {prog}: {e}"))?;
    let pgid = i32::try_from(child.id()).map_err(|e| e.to_string())?;
    let (rx, reader) = spawn_reader(pipe_r, log);
    let mut watch = LockWatch::default();

    let end = wait_for_end(&mut child, &rx, &mut watch, spec, abort, start);
    let elapsed = start.elapsed();
    watch.finish(Instant::now());
    let mut leaked = false;
    let group_gone = if matches!(end, End::Exited(_)) {
        // Zombies of a just-ended group can linger for a moment; give them 300 ms to vanish.
        leaked = !wait_group_gone(pgid, Duration::from_millis(300));
        if leaked {
            stop_group(&mut child, pgid, Duration::ZERO)
        } else {
            true
        }
    } else {
        stop_group(&mut child, pgid, spec.grace)
    };
    drain(&rx, &mut watch, Duration::from_secs(2));
    if reader.is_finished() {
        let _ = reader.join();
    }
    Ok(Ran {
        end,
        elapsed,
        watch,
        leaked,
        group_gone,
        pgid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    fn spec(script: &str, term_ms: u64, grace_ms: u64) -> Spec {
        let dir = std::env::temp_dir();
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Spec {
            argv: vec!["sh".into(), "-c".into(), script.into()],
            cwd: dir.clone(),
            env: vec![],
            log: dir.join(format!("devloop-test-{}-{n}.log", std::process::id())),
            term_after: Duration::from_millis(term_ms),
            grace: Duration::from_millis(grace_ms),
        }
    }

    fn go(s: &Spec) -> Ran {
        let r = run(s, &|| false).expect("runs");
        let _ = std::fs::remove_file(&s.log);
        r
    }

    #[test]
    fn a_normal_exit_keeps_its_code_and_output() {
        let s = spec("echo out; echo err >&2; exit 3", 5000, 100);
        let r = run(&s, &|| false).expect("runs");
        let text = std::fs::read_to_string(&s.log).expect("log");
        let _ = std::fs::remove_file(&s.log);
        assert_eq!(r.end, End::Exited(3));
        assert!(r.group_gone && !r.leaked);
        assert!(text.contains("out") && text.contains("err"));
    }

    #[test]
    fn the_deadline_stops_the_whole_group() {
        let r = go(&spec("sleep 30 & sleep 30 & sleep 30", 200, 500));
        assert_eq!(r.end, End::TimedOut);
        assert!(r.group_gone, "children of the shell must be gone too");
        assert!(r.elapsed < Duration::from_secs(2));
    }

    #[test]
    fn a_command_that_ignores_term_is_killed_after_the_grace() {
        let r = go(&spec("trap '' TERM; sleep 30", 200, 300));
        assert_eq!(r.end, End::TimedOut);
        assert!(r.group_gone);
    }

    #[test]
    fn a_process_left_behind_is_a_leak_and_is_killed() {
        let r = go(&spec("sleep 30 >/dev/null 2>&1 & echo done", 5000, 200));
        assert_eq!(r.end, End::Exited(0));
        assert!(r.leaked);
        assert!(r.group_gone);
    }

    #[test]
    fn a_build_directory_wait_stops_the_command_at_once() {
        let r = go(&spec(
            "echo 'Blocking waiting for file lock on build directory' >&2; sleep 30",
            20_000,
            300,
        ));
        assert_eq!(r.end, End::LockWait);
        assert!(r.watch.build_dir && r.group_gone);
        assert!(r.elapsed < Duration::from_secs(3));
    }

    #[test]
    fn a_package_cache_wait_is_timed_and_does_not_stop_anything() {
        let r = go(&spec(
            "echo 'Blocking waiting for file lock on package cache' >&2; sleep 0.3; echo go",
            5000,
            200,
        ));
        assert_eq!(r.end, End::Exited(0));
        assert_eq!(r.watch.pc_count, 1);
        assert!(
            r.watch.pc_wait >= Duration::from_millis(250),
            "{:?}",
            r.watch.pc_wait
        );
        assert!(!r.watch.build_dir);
    }

    #[test]
    fn the_abort_callback_interrupts() {
        let s = spec("sleep 30", 20_000, 300);
        let r = run(&s, &|| true).expect("runs");
        let _ = std::fs::remove_file(&s.log);
        assert_eq!(r.end, End::Interrupted);
        assert!(r.group_gone);
    }

    #[test]
    fn a_missing_program_is_an_error_not_a_pass() {
        let mut s = spec("true", 1000, 100);
        s.argv = vec!["/nonexistent/devloop-test-prog".into()];
        assert!(run(&s, &|| false).is_err());
    }
}
