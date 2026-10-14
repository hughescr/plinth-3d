//! **The width of the feature a tip touches** (`plinth_sweep::feature_chord`,
//! round 5, 2026-10-14): the narrowest chord of the cured section through the
//! seat at the strike layer, read from the real slicer's output.
//!
//! * a thin slab 0.2 mm wide: the chord through any seat in it is 0.2 mm, not
//!   the slab's 1.0 mm length;
//! * a seat beside the slab (not in material) has no chord;
//! * the layer the strike lamina is: the slab's first layer reads 0.2, the layer
//!   under it has no section;
//! * a wall farther than the reach reads as twice the reach.

use plinth_mesh::{make_box, Mesh};
use plinth_support::perception::{slice_stack, SliceSolid, SliceStackLimits};
use plinth_sweep::feature_chord::narrowest_chords_mm;

const H: f64 = 0.05;
const K_TOP: usize = 6;

fn slices(mesh: &Mesh) -> Vec<Vec<SliceSolid>> {
    #[expect(clippy::cast_precision_loss, reason = "a small layer index")]
    let planes: Vec<f64> = (0..K_TOP + 4).map(|layer| (layer as f64 + 0.5) * H).collect();
    slice_stack(mesh, &planes, SliceStackLimits::production()).expect("the fixture slices")
}

#[test]
fn the_chord_of_a_thin_slab_is_its_width_and_not_its_length() {
    // 0.2 wide in x, 1.0 long in y, three layers thick, underside at layer K_TOP.
    #[expect(clippy::cast_precision_loss, reason = "a small layer index")]
    // (`make_box` centres the box on `translate` in x and y, and puts its
    // underside at `translate` in z.)
    let mesh = make_box([0.2, 1.0, 3.0 * H], [0.0, 0.0, (K_TOP as f64) * H]).expect("a box");
    let stack = slices(&mesh);
    let points = [
        Some((K_TOP, [0.0, 0.0])),
        Some((K_TOP, [0.04, 0.3])),
        // Beside the slab: no material.
        Some((K_TOP, [0.5, 0.0])),
        // The layer under the slab: nothing there.
        Some((K_TOP - 1, [0.0, 0.0])),
        None,
    ];
    let chords = narrowest_chords_mm(&stack, &points, 1.0);
    println!("{chords:?}");
    for slot in [0, 1] {
        let chord = chords[slot].expect("in material");
        assert!((chord - 0.2).abs() < 1e-6, "slot {slot}: {chord}");
    }
    assert!(chords[2].is_none(), "a seat outside the section has no chord");
    assert!(chords[3].is_none(), "a layer with no section has no chord");
    assert!(chords[4].is_none());
}

#[test]
fn a_chord_longer_than_the_reach_is_reported_as_twice_the_reach() {
    #[expect(clippy::cast_precision_loss, reason = "a small layer index")]
    let mesh = make_box([4.0, 4.0, 3.0 * H], [0.0, 0.0, (K_TOP as f64) * H]).expect("a box");
    let stack = slices(&mesh);
    let chords = narrowest_chords_mm(&stack, &[Some((K_TOP, [0.0, 0.0]))], 0.5);
    assert_eq!(chords[0], Some(1.0), "no wall within the reach: {chords:?}");
}
