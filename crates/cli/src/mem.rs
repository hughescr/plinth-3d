//! Peak memory of the job. `ru_maxrss` is a lifetime high-water mark, so this is a job-level
//! figure only (design section 8): the plinth process's own peak plus the largest child
//! (the checker runs one at a time and starts no grandchildren). It reports; it does not
//! prevent. The only unsafe-free route to `getrusage` is the `nix` wrapper (owner-approved).

use nix::sys::resource::{UsageWho, getrusage};

/// Peak resident memory, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peak {
    /// This process.
    pub own: u64,
    /// The largest child that has finished (the checker).
    pub largest_child: u64,
}

impl Peak {
    /// Conservative job figure: both peaks added, as if they coexisted.
    #[must_use]
    pub fn job(self) -> u64 {
        self.own.saturating_add(self.largest_child)
    }
}

/// `ru_maxrss` is bytes on macOS and KiB elsewhere.
fn bytes_of(maxrss: i64) -> u64 {
    let n = u64::try_from(maxrss).unwrap_or(0);
    if cfg!(target_os = "macos") {
        n
    } else {
        n.saturating_mul(1024)
    }
}

/// # Errors
/// Returns the OS error text when `getrusage` fails.
pub fn peak() -> Result<Peak, String> {
    let own = getrusage(UsageWho::RUSAGE_SELF).map_err(|e| format!("getrusage(self): {e}"))?;
    let kids =
        getrusage(UsageWho::RUSAGE_CHILDREN).map_err(|e| format!("getrusage(children): {e}"))?;
    Ok(Peak {
        own: bytes_of(own.max_rss()),
        largest_child: bytes_of(kids.max_rss()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_peak_is_positive_and_job_adds_both() {
        let p = peak().unwrap();
        assert!(p.own > 0, "{p:?}");
        assert_eq!(p.job(), p.own + p.largest_child);
    }
}
