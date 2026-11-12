//! plinth-geom: the mesh core.
//!
//! * [`parse_stl`] / [`read_stl_file`]: binary and ASCII STL into a [`Mesh`].
//! * [`Mesh`]: structure-of-arrays vertices (`xs`, `ys`, `zs` as f64) and a flat `u32`
//!   index array, welded and Morton-ordered, with its [`Aabb`] bounds.
//! * [`Bvh`]: a flat 32-byte-node hierarchy built once per frozen mesh.
//! * [`closest_points`] / [`closest_points_par`]: the batched closest-point query. They
//!   take slices and fill caller-owned output; there is no single-point public query.
//!   [`reference::closest_points_reference`] is the scalar reference they must equal.
//! * [`counters`]: calls and items per batch operation, to prove no per-item calls.
//! * [`shapes`]: the pure f32 mesh kernel of the native ChituBox support shapes.
//! * Step 11 batch operations, signatures first ([`measure::Measure`] answers
//!   `Unverified` until each is built): [`slice`], [`raster`] and [`label`] (layers, lane
//!   B); [`patch`], [`clearance`] and [`ray`] (construction, lane A).

pub mod aabb;
pub mod bvh;
pub mod clearance;
pub mod counters;
pub mod error;
pub mod hollow;
pub mod label;
pub mod loop_canary;
pub mod measure;
pub mod mesh;
pub mod morton;
pub mod patch;
pub mod query;
pub mod raster;
pub mod ray;
pub mod reference;
pub mod shapes;
pub mod slice;
pub mod stl;
mod tri;

#[cfg(test)]
mod testutil;

pub use aabb::Aabb;
pub use bvh::Bvh;
pub use error::{GeomError, Result};
pub use loop_canary::loop_canary;
pub use mesh::Mesh;
pub use query::{ClosestHits, Points, closest_points, closest_points_par};
pub use stl::{parse_stl, read_stl_file};

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
