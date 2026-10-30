//! Edit target for `scripts/loop-bench` (design section 3.3). The bench edits the returned
//! value and times the rebuild chain this crate drives.

/// The canary value. Only the loop bench edits it.
#[inline(never)]
#[must_use]
pub fn loop_canary() -> u32 {
    3
}
