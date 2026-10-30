//! Lock-wait detection on the streamed cargo log (design dev-loop.md section 3.2).
//!
//! A wait on the shared package cache is counted and timed (from its `Blocking` line to the
//! next line the command prints). A wait on anything else (the build directory) is a hard
//! failure the caller acts on at once.

use std::time::{Duration, Instant};

const BLOCKING: &str = "Blocking waiting for file lock";

#[derive(Debug, Default)]
pub struct LockWatch {
    /// Package-cache waits seen.
    pub pc_count: u32,
    /// Total time spent in package-cache waits.
    pub pc_wait: Duration,
    /// A wait on a build directory (or any lock that is not the package cache) was seen.
    pub build_dir: bool,
    pending: Option<Instant>,
}

impl LockWatch {
    /// Feed one log line, stamped with the moment it was read.
    pub fn feed(&mut self, at: Instant, line: &str) {
        if let Some(start) = self.pending.take() {
            self.pc_wait += at.saturating_duration_since(start);
        }
        if !line.contains(BLOCKING) {
            return;
        }
        if line.contains("package cache") {
            self.pc_count += 1;
            self.pending = Some(at);
        } else {
            self.build_dir = true;
        }
    }

    /// The command is over: a wait that was still open ends now.
    pub fn finish(&mut self, at: Instant) {
        if let Some(start) = self.pending.take() {
            self.pc_wait += at.saturating_duration_since(start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn package_cache_wait_is_timed_to_the_next_line() {
        let t0 = Instant::now();
        let mut w = LockWatch::default();
        w.feed(t0, "   Compiling a v0.0.0\n");
        w.feed(
            t0 + ms(100),
            "    Blocking waiting for file lock on package cache\n",
        );
        w.feed(t0 + ms(1600), "   Compiling b v0.0.0\n");
        w.feed(t0 + ms(1700), "   Compiling c v0.0.0\n");
        assert_eq!(w.pc_count, 1);
        assert_eq!(w.pc_wait, ms(1500));
        assert!(!w.build_dir);
    }

    #[test]
    fn two_waits_add_up() {
        let t0 = Instant::now();
        let mut w = LockWatch::default();
        w.feed(t0, "Blocking waiting for file lock on package cache\n");
        w.feed(
            t0 + ms(300),
            "Blocking waiting for file lock on package cache\n",
        );
        w.feed(t0 + ms(900), "done\n");
        assert_eq!(w.pc_count, 2);
        assert_eq!(w.pc_wait, ms(900));
    }

    #[test]
    fn build_directory_wait_is_a_hard_failure() {
        let mut w = LockWatch::default();
        w.feed(
            Instant::now(),
            "    Blocking waiting for file lock on build directory\n",
        );
        assert!(w.build_dir);
        assert_eq!(w.pc_count, 0);
    }

    #[test]
    fn open_wait_ends_when_the_command_ends() {
        let t0 = Instant::now();
        let mut w = LockWatch::default();
        w.feed(t0, "Blocking waiting for file lock on package cache\n");
        w.finish(t0 + ms(2500));
        assert_eq!(w.pc_wait, ms(2500));
        w.finish(t0 + ms(9000));
        assert_eq!(w.pc_wait, ms(2500), "finishing twice adds nothing");
    }

    #[test]
    fn ordinary_lines_change_nothing() {
        let mut w = LockWatch::default();
        w.feed(Instant::now(), "   Compiling x v1.0.0\n");
        assert_eq!(
            (w.pc_count, w.pc_wait, w.build_dir),
            (0, Duration::ZERO, false)
        );
    }
}
