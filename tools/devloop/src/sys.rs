//! The only unsafe code in the runner: four libc calls (macOS and Linux share these values).
#![allow(unsafe_code)]

use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::raw::{c_double, c_int};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const SIGTERM: i32 = 15;
pub const SIGKILL: i32 = 9;
const SIGHUP: c_int = 1;
const SIGINT: c_int = 2;
const LOCK_EX: c_int = 2;
const LOCK_NB: c_int = 4;

unsafe extern "C" {
    safe fn kill(pid: i32, sig: c_int) -> c_int;
    safe fn flock(fd: c_int, op: c_int) -> c_int;
    safe fn signal(sig: c_int, handler: usize) -> usize;
    unsafe fn getloadavg(avg: *mut c_double, n: c_int) -> c_int;
}

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_sig: c_int) {
    STOP.store(true, Ordering::SeqCst);
}

/// Route INT, TERM and HUP to a flag the supervisors poll, so they can stop their children.
pub fn install_stop_handlers() {
    for sig in [SIGINT, SIGTERM, SIGHUP] {
        signal(sig, on_signal as extern "C" fn(c_int) as usize);
    }
}

#[must_use]
pub fn stop_requested() -> bool {
    STOP.load(Ordering::SeqCst)
}

/// Send `sig` to every process of group `pgid`. True if the call succeeded; callers that stop
/// a group check `group_alive` afterwards instead.
pub fn kill_group(pgid: i32, sig: i32) -> bool {
    pgid > 1 && kill(-pgid, sig) == 0
}

/// True while any process of group `pgid` exists (signal 0 only tests).
#[must_use]
pub fn group_alive(pgid: i32) -> bool {
    pgid > 1 && kill(-pgid, 0) == 0
}

/// The 1-minute load average, or 0.0 when the system will not say.
#[must_use]
pub fn load1() -> f64 {
    let mut v: c_double = 0.0;
    // SAFETY: one valid, writable double and a count of 1.
    let n = unsafe { getloadavg(&raw mut v, 1) };
    if n == 1 { v } else { 0.0 }
}

/// Take an exclusive kernel lock on `file`, waiting up to `limit`. The lock dies with the process.
pub fn lock_exclusive(file: &File, limit: Duration) -> bool {
    let start = Instant::now();
    loop {
        if flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) == 0 {
            return true;
        }
        if start.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
