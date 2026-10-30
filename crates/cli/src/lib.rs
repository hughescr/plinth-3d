//! plinth-cli library: the command line's logic, so `plinth-acceptance` can call it.
//! Empty skeleton (step 1 of the build plan).

pub mod loop_canary;

pub use loop_canary::loop_canary;

/// Canary values of this crate and every crate below it, summed. Calling it from
/// production code makes a canary edit drive the real rebuild chain.
#[must_use]
pub fn canary_chain() -> u32 {
    plinth_geom::loop_canary()
        .wrapping_add(plinth_support::loop_canary())
        .wrapping_add(plinth_chitubox::loop_canary())
        .wrapping_add(plinth_emit::loop_canary())
        .wrapping_add(loop_canary())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_is_emit_chain_plus_own() {
        assert_eq!(
            canary_chain(),
            plinth_emit::canary_chain().wrapping_add(loop_canary())
        );
    }
}
