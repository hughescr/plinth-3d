//! **Which pass last moved each member** (diagnosis item 10 of
//! `DEINO-DIAGNOSIS.md`, 2026-10-10). Log text only: nothing here reads back
//! into placement, so the emitted bytes do not depend on it.
//!
//! The sweep runs its passes one after the other on one strut set. After each
//! pass [`Moves::note`] compares every member's descent (its lowest layer, the
//! kind of thing it lands on and the cells of its path) with the descent seen
//! after the pass before, and names the pass against each member that
//! changed. A member is keyed by its seat (cell, top layer, kind), which no
//! pass changes and which survives the renumbering a removal does; the
//! landing's host id is left out of the comparison for the same reason.
//!
//! Batch-first: one pass over the strut set per call, one map lookup per member.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use crate::{Landing, SweepStrut};

/// A member's seat: its cell, top layer and kind.
type Seat = (usize, usize, u8);

/// The last pass that changed each member's descent.
#[derive(Debug, Default)]
pub(crate) struct Moves {
    seen: BTreeMap<Seat, (u64, &'static str)>,
}

fn seat_of(strut: &SweepStrut) -> Seat {
    (strut.cell, strut.k_top, strut.kind as u8)
}

/// A hash of the descent: lowest layer, landing kind and layer, path cells.
fn descent_hash(strut: &SweepStrut) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    strut.k_bottom.hash(&mut hasher);
    match strut.landing {
        Landing::Plate => 0_u8.hash(&mut hasher),
        Landing::Model { layer } => {
            1_u8.hash(&mut hasher);
            layer.hash(&mut hasher);
        }
        Landing::Strut { layer, .. } => {
            2_u8.hash(&mut hasher);
            layer.hash(&mut hasher);
        }
    }
    strut.path.hash(&mut hasher);
    hasher.finish()
}

impl Moves {
    /// Record that `pass` has just run on `struts`: every member whose descent
    /// differs from the last note (or that is new) is credited to `pass`.
    pub(crate) fn note(&mut self, pass: &'static str, struts: &[SweepStrut]) {
        for strut in struts {
            let hash = descent_hash(strut);
            let entry = self.seen.entry(seat_of(strut)).or_insert((hash, pass));
            if entry.0 != hash {
                *entry = (hash, pass);
            }
        }
    }

    /// The pass that last changed `strut`'s descent, or `"not recorded"`.
    pub(crate) fn last(&self, strut: &SweepStrut) -> &'static str {
        self.seen
            .get(&seat_of(strut))
            .map_or("not recorded", |&(_, pass)| pass)
    }
}
