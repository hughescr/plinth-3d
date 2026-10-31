//! plinth-support: the support generator.
//!
//! Step 11 module map (each is a stub until its lane fills it; see
//! `docs/design/step11-construction.md` section 10):
//!
//! * [`load`]: the FR-20a load model and its plain-data API (lane B).
//! * [`report_data`]: the data the report and pictures are made from (lane B).
//! * [`construct`]: the sealed construction API, `probe` and `commit` (lane A).
//! * [`place`]: the planner (lane A).
//! * [`route`]: columns, braces and bases (lane A).
//! * [`raft`]: the connected raft (lane A).

pub mod construct;
pub mod load;
pub mod loop_canary;
pub mod place;
pub mod raft;
pub mod report_data;
pub mod route;

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
