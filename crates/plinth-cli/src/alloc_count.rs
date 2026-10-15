//! Opt-in allocation profiler (cargo feature `alloc-count` of `plinth-cli`).
//!
//! A counting wrapper around the system allocator that tells where a run's
//! allocations come from.  It changes no output byte, but it slows the run, so
//! NEVER time a run built with it.  Build and read it with
//! `scripts/alloc-profile.sh`.
//!
//! What it records (all lock-free except the sampled call-site table):
//!  * every allocation, reallocation and free: counts and bytes per size class,
//!    in sharded counters (one cache line group per thread, no shared hot line);
//!  * a snapshot of those counters once per second (`PLINTH_ALLOC_LOG`, a file
//!    path): the run's phases are read off the time axis;
//!  * a sample of allocation call sites: 1 in `SMALL_STRIDE` allocations of up to
//!    `SMALL_LIMIT` bytes, 1 in `LARGE_STRIDE` of larger ones, each sample taken
//!    with a frame-pointer walk (`DEPTH` return addresses), keyed with a
//!    10-second window; written to `PLINTH_ALLOC_SITES` (a file path) every few
//!    seconds and at exit.  Each site line holds estimated counts and bytes
//!    (sample x stride), the window, the load slide and the raw return
//!    addresses; `scripts/alloc-profile.sh` turns the addresses into names with
//!    `atos`.
//!
//! The allocator hook never allocates through itself: a thread-local flag turns
//! the sampler off while it touches its own table.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SHARDS: usize = 64;
/// Upper bounds (bytes) of the size classes; the last class is everything above.
const CLASS_LIMITS: [usize; 11] = [16, 32, 64, 128, 256, 512, 1024, 4096, 16384, 65536, 1 << 20];
const CLASSES: usize = CLASS_LIMITS.len() + 1;
const DEPTH: usize = 14;
const SMALL_LIMIT: usize = 1024;
const SMALL_STRIDE: u32 = 256;
const LARGE_STRIDE: u32 = 16;
const WINDOW_SECS: u64 = 10;

#[repr(align(128))]
struct Shard {
    allocs: [AtomicU64; CLASSES],
    bytes: [AtomicU64; CLASSES],
    frees: AtomicU64,
    reallocs: AtomicU64,
}

impl Shard {
    const fn new() -> Self {
        Self {
            allocs: [const { AtomicU64::new(0) }; CLASSES],
            bytes: [const { AtomicU64::new(0) }; CLASSES],
            frees: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
        }
    }
}

static SHARD_TABLE: [Shard; SHARDS] = [const { Shard::new() }; SHARDS];
static NEXT_THREAD: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static THREAD_SHARD: Cell<usize> = const { Cell::new(usize::MAX) };
    static IN_HOOK: Cell<bool> = const { Cell::new(false) };
    static TICK_SMALL: Cell<u32> = const { Cell::new(0) };
    static TICK_LARGE: Cell<u32> = const { Cell::new(0) };
    static STACK_BOUNDS: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

extern "C" {
    fn pthread_self() -> usize;
    fn pthread_get_stackaddr_np(thread: usize) -> usize;
    fn pthread_get_stacksize_np(thread: usize) -> usize;
    fn _dyld_get_image_vmaddr_slide(image_index: u32) -> isize;
}

#[derive(Default)]
struct Site {
    count: u64,
    bytes: u64,
}

type SiteKey = (u64, [usize; DEPTH]);
static SITES: Mutex<Option<HashMap<SiteKey, Site>>> = Mutex::new(None);
static START: Mutex<Option<Instant>> = Mutex::new(None);

fn class_of(size: usize) -> usize {
    CLASS_LIMITS.iter().position(|&l| size <= l).unwrap_or(CLASSES - 1)
}

fn shard() -> &'static Shard {
    let i = THREAD_SHARD.with(|c| {
        if c.get() == usize::MAX {
            c.set(NEXT_THREAD.fetch_add(1, Relaxed) % SHARDS);
        }
        c.get()
    });
    &SHARD_TABLE[i]
}

/// Walk the frame-pointer chain (always present on Apple arm64) into `out`.
#[inline(never)]
fn walk(out: &mut [usize; DEPTH]) -> usize {
    let mut fp: usize;
    // SAFETY: reads the frame-pointer register only.
    unsafe { core::arch::asm!("mov {}, x29", out(reg) fp) };
    let (low, high) = STACK_BOUNDS.with(|c| {
        if c.get().1 == 0 {
            // SAFETY: plain pthread queries of the current thread.
            let (top, size) = unsafe {
                let t = pthread_self();
                (pthread_get_stackaddr_np(t), pthread_get_stacksize_np(t))
            };
            c.set((top.saturating_sub(size), top));
        }
        c.get()
    });
    let mut n = 0;
    let mut skipped = 0;
    while n < DEPTH && fp >= low && fp + 16 <= high && fp & 15 == 0 {
        // SAFETY: fp is inside this thread's stack and 16-byte aligned.
        let (next, ret) = unsafe { (*(fp as *const usize), *((fp + 8) as *const usize)) };
        if skipped >= 2 {
            out[n] = ret & 0x7f_ffff_ffff;
            n += 1;
        } else {
            skipped += 1;
        }
        if next <= fp {
            break;
        }
        fp = next;
    }
    n
}

fn window_now() -> u64 {
    START
        .lock()
        .ok()
        .and_then(|g| *g)
        .map_or(0, |s| s.elapsed().as_secs() / WINDOW_SECS)
}

fn sample(size: usize) {
    let (cell, stride) = if size <= SMALL_LIMIT {
        (&TICK_SMALL, SMALL_STRIDE)
    } else {
        (&TICK_LARGE, LARGE_STRIDE)
    };
    let t = cell.with(|c| {
        let v = c.get().wrapping_add(1);
        c.set(v);
        v
    });
    if !t.is_multiple_of(stride) || IN_HOOK.with(Cell::get) {
        return;
    }
    IN_HOOK.with(|f| f.set(true));
    let mut frames = [0usize; DEPTH];
    walk(&mut frames);
    let window = window_now();
    if let Ok(mut g) = SITES.lock() {
        let site = g.get_or_insert_with(HashMap::new).entry((window, frames)).or_default();
        site.count += u64::from(stride);
        site.bytes += u64::from(stride) * size as u64;
    }
    IN_HOOK.with(|f| f.set(false));
}

#[inline]
fn count_alloc(size: usize) {
    let s = shard();
    let c = class_of(size);
    s.allocs[c].fetch_add(1, Relaxed);
    s.bytes[c].fetch_add(size as u64, Relaxed);
    sample(size);
}

/// The global allocator: the system allocator plus the counters above.
pub struct Counting;

// SAFETY: every method forwards to `System` unchanged; the counters only observe.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_alloc(layout.size());
        System.alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_alloc(layout.size());
        System.alloc_zeroed(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        shard().frees.fetch_add(1, Relaxed);
        System.dealloc(ptr, layout);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        shard().reallocs.fetch_add(1, Relaxed);
        count_alloc(new_size);
        System.realloc(ptr, layout, new_size)
    }
}

fn totals() -> ([u64; CLASSES], [u64; CLASSES], u64, u64) {
    let mut a = [0u64; CLASSES];
    let mut b = [0u64; CLASSES];
    let (mut f, mut r) = (0, 0);
    for s in &SHARD_TABLE {
        for i in 0..CLASSES {
            a[i] += s.allocs[i].load(Relaxed);
            b[i] += s.bytes[i].load(Relaxed);
        }
        f += s.frees.load(Relaxed);
        r += s.reallocs.load(Relaxed);
    }
    (a, b, f, r)
}

fn snapshot_line(elapsed: Duration) -> String {
    let (a, b, f, r) = totals();
    let mut s = format!("{:.1}\tallocs={}\tbytes={}\tfrees={}\treallocs={}", elapsed.as_secs_f64(), a.iter().sum::<u64>(), b.iter().sum::<u64>(), f, r);
    s.push_str("\tclass_allocs=");
    for (i, v) in a.iter().enumerate() {
        let _ = write!(s, "{}{v}", if i == 0 { "" } else { "," });
    }
    s.push_str("\tclass_bytes=");
    for (i, v) in b.iter().enumerate() {
        let _ = write!(s, "{}{v}", if i == 0 { "" } else { "," });
    }
    s
}

fn write_sites(path: &str) {
    IN_HOOK.with(|f| f.set(true));
    let mut out = String::new();
    // SAFETY: plain dyld query for the main image.
    let slide = unsafe { _dyld_get_image_vmaddr_slide(0) };
    let _ = writeln!(out, "#slide\t{slide}\tdepth\t{DEPTH}\tclass_limits\t{CLASS_LIMITS:?}");
    if let Ok(g) = SITES.lock() {
        if let Some(map) = g.as_ref() {
            for ((window, frames), site) in map {
                let _ = write!(out, "{window}\t{}\t{}\t", site.count, site.bytes);
                for (i, p) in frames.iter().filter(|&&p| p != 0).enumerate() {
                    let _ = write!(out, "{}{p:x}", if i == 0 { "" } else { " " });
                }
                out.push('\n');
            }
        }
    }
    let _ = std::fs::write(path, out);
    IN_HOOK.with(|f| f.set(false));
}

/// Start the once-per-second counter log and the periodic site table dump.
/// Call first thing in `main`.
pub fn start() {
    let started = Instant::now();
    IN_HOOK.with(|f| f.set(true));
    if let Ok(mut g) = START.lock() {
        *g = Some(started);
    }
    IN_HOOK.with(|f| f.set(false));
    let log = std::env::var("PLINTH_ALLOC_LOG").ok();
    let sites = std::env::var("PLINTH_ALLOC_SITES").ok();
    if log.is_none() && sites.is_none() {
        return;
    }
    let _ = std::thread::Builder::new().name("alloc-count".into()).spawn(move || {
        let mut lines = String::new();
        let mut tick = 0u64;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            IN_HOOK.with(|f| f.set(true));
            lines.push_str(&snapshot_line(started.elapsed()));
            lines.push('\n');
            if let Some(p) = &log {
                let _ = std::fs::write(p, &lines);
            }
            IN_HOOK.with(|f| f.set(false));
            tick += 1;
            if tick.is_multiple_of(5) {
                if let Some(p) = &sites {
                    write_sites(p);
                }
            }
        }
    });
}

/// Final dump at exit.
pub fn finish() {
    IN_HOOK.with(|f| f.set(true));
    let started = START.lock().ok().and_then(|g| *g);
    if let (Ok(p), Some(s)) = (std::env::var("PLINTH_ALLOC_LOG"), started) {
        let mut text = std::fs::read_to_string(&p).unwrap_or_default();
        text.push_str(&snapshot_line(s.elapsed()));
        text.push('\n');
        let _ = std::fs::write(&p, text);
    }
    IN_HOOK.with(|f| f.set(false));
    if let Ok(p) = std::env::var("PLINTH_ALLOC_SITES") {
        write_sites(&p);
    }
}
