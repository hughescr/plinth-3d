//! Cooperative cancellation of one pose's sweep (perf lane B1, 2026-10-14).
//!
//! The orientation search prices several finalist poses at production pitch
//! in parallel and waits for the slowest. A pose that provably cannot beat a
//! finished one (`plinth_orient::demand::ProductionRace`) is told to stop so
//! the batch returns when the pose that decides it is done, not when the
//! slowest loser is.
//!
//! # The mechanism
//!
//! A pose's measurement runs its whole sweep on one thread (`measure_one`),
//! so the token is a thread-local, installed for the duration of that call by
//! [`with_token`]. The sweep driver reads it at its **safe points**
//! ([`checkpoint`]): between layers of the upward pass, before every
//! down-sweep (the stage that takes seconds), and before each of the larger
//! passes. A safe point returns [`SweepError::Cancelled`] and the driver
//! unwinds through its ordinary `?` path, so nothing is half-written and no
//! state outlives the call.
//!
//! Inner `par_iter` regions never check the token: the check runs only on the
//! driving thread, between regions. A pose that steals another pose's job
//! while it waits on a region nests a second [`with_token`] scope; the guard
//! restores the outer token on exit, so each pose reads its own.
//!
//! # What it does not do
//!
//! It decides nothing. Whether a pose may be stopped is the caller's proof
//! (`plinth_orient::search`); this module only carries the flag. With no
//! token installed, [`checkpoint`] is a thread-local read that returns `Ok`,
//! so the emitted-field sweep and every other caller are untouched.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::SweepError;

thread_local! {
    static TOKEN: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// Restores the previous token when the scope ends, on any exit path.
struct Scope {
    previous: Option<Arc<AtomicBool>>,
}

impl Drop for Scope {
    fn drop(&mut self) {
        TOKEN.with(|token| *token.borrow_mut() = self.previous.take());
    }
}

/// Runs `work` with `token` installed as this thread's cancel flag. A sweep
/// run inside it returns [`SweepError::Cancelled`] at the first safe point
/// after the flag is set.
pub fn with_token<R>(token: &Arc<AtomicBool>, work: impl FnOnce() -> R) -> R {
    let previous = TOKEN.with(|slot| slot.borrow_mut().replace(Arc::clone(token)));
    let _scope = Scope { previous };
    work()
}

/// `true` when this thread's token is installed and set.
#[must_use]
pub fn cancelled() -> bool {
    TOKEN.with(|token| {
        token
            .borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
    })
}

/// A safe point: `Err(SweepError::Cancelled)` once the token is set.
///
/// # Errors
///
/// [`SweepError::Cancelled`] when this thread's token has been set.
pub fn checkpoint() -> Result<(), SweepError> {
    if cancelled() {
        Err(SweepError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_token_never_cancels() {
        assert!(checkpoint().is_ok());
        assert!(!cancelled());
    }

    #[test]
    fn a_set_token_cancels_only_inside_its_scope() {
        let token = Arc::new(AtomicBool::new(false));
        with_token(&token, || {
            assert!(checkpoint().is_ok());
            token.store(true, Ordering::Release);
            assert!(matches!(checkpoint(), Err(SweepError::Cancelled)));
        });
        assert!(checkpoint().is_ok(), "the scope ended");
    }

    #[test]
    fn nested_scopes_restore_the_outer_token() {
        let outer = Arc::new(AtomicBool::new(false));
        let inner = Arc::new(AtomicBool::new(true));
        with_token(&outer, || {
            with_token(&inner, || assert!(cancelled()));
            assert!(!cancelled(), "the outer token is back");
        });
    }
}
