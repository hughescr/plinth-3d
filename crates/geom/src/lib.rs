//! plinth-geom: mesh core. Empty skeleton (step 1 of the build plan).

pub mod loop_canary;

pub use loop_canary::loop_canary;

/// Canary values of this crate and every crate below it, summed. Calling it from
/// production code makes a canary edit drive the real rebuild chain.
#[must_use]
pub fn canary_chain() -> u32 {
    loop_canary()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_is_own_canary() {
        assert_eq!(canary_chain(), loop_canary());
    }
}
