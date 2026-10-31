//! The connected raft (lane A; design `step11-construction.md` section 5.7). Step 0 holds
//! the signatures only.

/// Pad diameter at each base, mm (native rule).
pub const PAD_DIAMETER_MM: f32 = 2.2;
/// Pad and bridge thickness, mm (native rule).
pub const THICKNESS_MM: f32 = 0.2;
/// Bridge width, mm (native rule).
pub const BRIDGE_WIDTH_MM: f32 = 2.2;

/// The bridges of a connected raft: pairs of pad indices along the minimum spanning tree of
/// the pad centres. `out` is cleared and refilled.
///
/// The stub leaves `out` empty: it builds no bridge and so proves no connection.
pub fn bridge_pairs(pad_centres: &[[f32; 2]], out: &mut Vec<[u32; 2]>) {
    let _ = pad_centres;
    out.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stub_builds_no_bridge() {
        let mut out = vec![[0, 1]];
        bridge_pairs(&[[0.0, 0.0], [3.0, 0.0]], &mut out);
        assert!(out.is_empty());
    }
}
