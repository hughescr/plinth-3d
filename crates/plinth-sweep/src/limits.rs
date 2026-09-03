//! Explicit, caller-owned resource authority for the sweep.
//!
//! A NEW type, deliberately not `SupportPreparationLimits`: that one is shaped
//! for the legacy kNN and gate-batch resource model and carries fields this
//! engine has no analogue for (design of record D8). Reusing it would make
//! every cap here a claim about work this crate never does.

/// Resource caps. Every one of these REFUSES OVERSIZED WORK; none of them
/// changes a successful plan, and none of them can turn a shortfall into a
/// refusal — a shortfall is a warning on an emitted file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SweepLimits {
    /// Cells × layers the working raster may occupy before the pitch doubles.
    /// This one COARSENS; it never refuses.
    pub cell_layer_budget: u64,
    /// Planes handed to one slice batch.
    pub max_planes: usize,
    /// Struts one model may carry. A model that needs more is a finding about
    /// the model or the engine, and the refusal names the count.
    pub max_struts: usize,
    /// Bytes the kernel's surface grid may occupy.
    pub max_surface_grid_bytes: usize,
}

impl SweepLimits {
    /// The production authority.
    ///
    /// `cell_layer_budget` is read from the registry by
    /// [`crate::SweepPolicy::from_registry`]; the value here is the same one
    /// and exists so a caller that builds limits by hand does not silently get
    /// zero.
    #[must_use]
    pub const fn production() -> Self {
        Self {
            cell_layer_budget: 2_000_000_000,
            max_planes: 20_000,
            max_struts: 2_000_000,
            max_surface_grid_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

impl Default for SweepLimits {
    fn default() -> Self {
        Self::production()
    }
}
