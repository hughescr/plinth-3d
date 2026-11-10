//! H9 and thin lenses between thick rods. Two radius 0.3 posts whose 16-gon meshes share a lens
//! only 0.01 mm thick are a thin join, unless both posts are held without the lens. The checker
//! treats a redundant contact between independently held thick parts as incidental (not a
//! join), but a lens that something depends on, alone or in a cycle of such lenses, must be
//! refused. Each case below pins one side of that rule.
//!
//! The 16-gon rods meet along their vertex directions (multiples of 22.5 degrees), so every
//! lens here runs along x or y.

use super::{Built, Case, LIFT, P, V, cube10, head, pad, plate, rod};

/// Two radius 0.3 posts side by side, `offset` apart in x, on overlapping raft pads. Their
/// meshes (16-gons) share a lens whose thickness is `0.6 - offset` (a little more for a
/// polygon's flats). Both posts stand on their own pad, so each is held without the other.
fn two_posts(offset: f32) -> Built {
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    for x in [6.0, 6.0 + offset] {
        recs.push(rod([x, 6.0, z0 + 0.198], [x, 6.0, z0 + 3.0], 0.3));
        recs.push(pad(x, 6.0, LIFT));
    }
    cube10(recs, LIFT)
}

/// A radius 0.3 post on its own raft pad, and a second radius 0.3 post `offset` beside it
/// that has no pad and starts above the plate: the second post is held by the first through
/// their shared lens (`0.6 - offset` thick, a little more for a polygon's flats) and by
/// nothing else, so the lens is a necessary join, not a redundant one.
fn post_held_only_by_a_lens(offset: f32) -> Built {
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(rod([6.0, 6.0, z0 + 0.198], [6.0, 6.0, z0 + 3.0], 0.3));
    recs.push(pad(6.0, 6.0, LIFT));
    recs.push(rod(
        [6.0 + offset, 6.0, z0 + 2.0],
        [6.0 + offset, 6.0, z0 + 4.0],
        0.3,
    ));
    cube10(recs, LIFT)
}

fn h9_thick_rod_held_only_by_a_thin_offset_lens() -> Built {
    // Offset 0.59: the two 16-gon rods share a lens about 0.01 mm thick. The second rod has no
    // pad and hangs 2 mm above the plate: that lens is all that holds it, so it must be refused.
    post_held_only_by_a_lens(0.59)
}

fn h9_thick_rods_held_only_by_a_cycle_of_thin_lenses() -> Built {
    // Four radius 0.3 posts at the corners of a 0.59 mm square (the rods meet along the
    // square's sides, which are vertex directions of the polygon; the diagonals do not touch).
    // Only the first stands on a pad. The other three hang 2 mm above the plate; each side of
    // the square is a thin lens. No lens is a bridge (each hung post has two), but the four
    // lenses form a cycle of uncertified contacts, and a cycle of weak contacts must not
    // certify itself as redundant.
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(rod([6.0, 6.0, z0 + 0.198], [6.0, 6.0, z0 + 3.0], 0.3));
    recs.push(pad(6.0, 6.0, LIFT));
    for (x, y) in [(6.59, 6.0), (6.59, 6.59), (6.0, 6.59)] {
        recs.push(rod([x, y, z0 + 2.0], [x, y, z0 + 4.0], 0.3));
    }
    cube10(recs, LIFT)
}

fn h9_redundant_thin_offset_lens_between_rooted_rods_control() -> Built {
    // The same 0.59 offset, but each rod stands on its own pad and the pads overlap: the lens is a
    // redundant incidental contact between independently held thick parts, not a join.
    two_posts(0.59)
}

fn h9_deep_overlap_between_thick_rods_control() -> Built {
    // Offset 0.5: a fat join, nothing thin.
    two_posts(0.5)
}

pub(crate) const CASES: &[Case] = &[
    Case {
        rule: "H9",
        name: "thick rod held only by a thin offset lens",
        expect: V,
        build: h9_thick_rod_held_only_by_a_thin_offset_lens,
    },
    Case {
        rule: "H9",
        name: "thick rods held only by a cycle of thin lenses",
        expect: V,
        build: h9_thick_rods_held_only_by_a_cycle_of_thin_lenses,
    },
    Case {
        rule: "H9",
        name: "redundant thin offset lens between rooted rods (control)",
        expect: P,
        build: h9_redundant_thin_offset_lens_between_rooted_rods_control,
    },
    Case {
        rule: "H9",
        name: "thick rods overlapping deeply (control)",
        expect: P,
        build: h9_deep_overlap_between_thick_rods_control,
    },
];
