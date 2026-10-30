//! plinth-support: the support generator. Empty skeleton (step 1 of the build plan).

pub mod loop_canary;

pub use loop_canary::loop_canary;

/// Canary values of this crate and every crate below it, summed. Calling it from
/// production code makes a canary edit drive the real rebuild chain.
#[must_use]
pub fn canary_chain() -> u32 {
    plinth_geom::canary_chain().wrapping_add(loop_canary())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_is_geom_plus_own() {
        assert_eq!(
            canary_chain(),
            plinth_geom::loop_canary().wrapping_add(loop_canary())
        );
    }
}
