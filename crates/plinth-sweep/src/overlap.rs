//! Run a long job with a short one beside it on the shared pool (perf lane
//! `ov`, 2026-10-15).
//!
//! The second contact's re-plan is the longest stage of a run and uses about
//! one core; the pool's other workers are idle. [`overlap`] hands them a side
//! job (diagnostics of the first plan, a speculative emission of it) that
//! would otherwise run after the re-plan, serially.
//!
//! It is [`rayon::join`] and nothing more, so: the side job starts on an idle
//! worker at once, or runs on the caller's thread after the main job when no
//! worker is free (the serial schedule, with the same results); the call
//! returns when both are done; a panic in the main job unwinds out of the call
//! with its original payload after the side job has finished. The side job's
//! own panic does NOT unwind here: it comes back as a value, because whether it
//! matters depends on a choice the main job makes (a speculation is dropped
//! when the main job picks another plan), and the caller re-raises it with
//! [`std::panic::resume_unwind`] only when it does.
//!
//! The side job may run on any worker, and the sweep's readings poll the
//! thread's cancel token ([`crate::cancel`]). The caller's token is read here,
//! on the calling thread, and installed for the side job with
//! [`crate::cancel::with_carried`], so the job polls its owner's flag (or none)
//! and never the flag of whichever pose its worker is driving.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Run `main` and `side` together; see the module doc.
pub fn overlap<A, B, FA, FB>(main: FA, side: FB) -> (A, std::thread::Result<B>)
where
    A: Send,
    B: Send,
    FA: FnOnce() -> A + Send,
    FB: FnOnce() -> B + Send,
{
    let token = crate::cancel::current_token();
    let run = move || {
        rayon::join(main, move || {
            crate::cancel::with_carried(&token, || catch_unwind(AssertUnwindSafe(side)))
        })
    };
    // The whole process body runs inside the budget pool; from anywhere else
    // the join goes to the kernel's cached pool, never rayon's global one.
    if rayon::current_thread_index().is_some() {
        run()
    } else {
        geom_kernel::par::run(run)
    }
}
