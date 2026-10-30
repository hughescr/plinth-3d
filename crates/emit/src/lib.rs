//! plinth-emit: support plan to native ChituBox records. Empty skeleton (step 1 of the
//! build plan).

pub mod loop_canary;

pub use loop_canary::loop_canary;

/// Canary values of this crate and every crate below it, summed. Calling it from
/// production code makes a canary edit drive the real rebuild chain.
#[must_use]
pub fn canary_chain() -> u32 {
    plinth_support::canary_chain()
        .wrapping_add(plinth_chitubox::canary_chain())
        .wrapping_add(loop_canary())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_is_support_chain_plus_chitubox_plus_own() {
        let flat = plinth_support::canary_chain()
            .wrapping_add(plinth_chitubox::loop_canary())
            .wrapping_add(loop_canary());
        assert_eq!(canary_chain(), flat);
    }
}
