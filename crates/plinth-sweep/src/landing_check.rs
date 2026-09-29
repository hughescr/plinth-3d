//! **Support comes before what rests on it.** The landing-validity check that
//! runs after every pass that edits landings.
//!
//! # Why this exists
//!
//! On 2026-09-28 the owner found "porcupine quills" in ChituBox: support arms
//! whose tip was seated in the model and whose lower end stopped in the air
//! (`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/quills.md`). 157
//! of 983 members on the Cylinder. Two passes made them, and nothing noticed:
//!
//! * the slenderness lane's forced merge re-landed arms that other arms had
//!   already landed on, against a shaft index stamped before the loop began,
//!   so a child landed at layer `L` on a parent that then started at `L' > L`;
//! * `unmerge_overgrown` put parents back on their old straight columns and
//!   left the children that had landed on the leaned shaft 3 mm away.
//!
//! Each pass kept its own ledger consistent, and the down-sweep booked the lost
//! child's load onto a parent that was no longer there, so the published
//! "0 unrooted landings" (which counts only up-sweep refusals) stayed zero.
//!
//! # The rule
//!
//! A `Landing::Strut { id, layer }` is valid only when the parent STANDS THERE
//! NOW: it exists, it spans `layer` (`k_bottom <= layer <= k_top`), the child's
//! own base is at that layer, and the parent's axis occupies the child's base
//! cell at that layer. A member is ROOTED when its landing is the plate or the
//! model, or a valid landing on a rooted member. A brace's TOP is a lateral
//! tie, not a load path, but a tie to a member that has moved away ties
//! nothing, so detached brace tops are published beside the landings.
//!
//! # What it does with a violation
//!
//! Nothing but count it. **Emit and warn, never refuse**: the file is still
//! written, and the count is published on it as a warning together with the
//! whole-run load gap, `injected - plate reaction - uncarried`, which is the
//! load the down-sweep handed to a parent that was not there to take it.
//!
//! # Cost
//!
//! One pass over the struts and one memoised walk down each landing chain:
//! `O(struts)` per check. No grid, no per-layer scan, no rebuild of the shaft
//! index — the check reads each parent's own `cell_at`, which is `O(1)`.

use crate::{Landing, StrutKind, SweepStrut};

/// One landing-validity check of one strut set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LandingCheck {
    /// `Landing::Strut` landings examined. The path-hit witness: a clean check
    /// on a field with no member-on-member landings proves nothing.
    pub strut_landings: usize,
    /// Members landed on a parent that is not there at the landing layer: the
    /// parent is missing, its span does not include the layer, or the child's
    /// own base is not at that layer. Strut ids.
    pub parent_absent: Vec<u32>,
    /// Members whose parent spans the layer but stands in a DIFFERENT cell
    /// there — the parent was moved sideways out from under the child.
    pub parent_elsewhere: Vec<u32>,
    /// Braces whose top seats on a member that no longer stands in the brace's
    /// seat cell at the brace's top layer.
    pub brace_tops_detached: Vec<u32>,
    /// Every member whose chain of landings does not end at the plate or the
    /// model: the two lists above, and everything that rests on them.
    pub unrooted: Vec<u32>,
}

impl LandingCheck {
    /// Invalid landings of either kind.
    #[must_use]
    pub fn invalid_landings(&self) -> usize {
        self.parent_absent.len() + self.parent_elsewhere.len()
    }

    /// Whether this check found nothing to warn about.
    #[must_use]
    pub fn clean(&self) -> bool {
        self.unrooted.is_empty() && self.brace_tops_detached.is_empty()
    }
}

/// The check after one named pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LandingPass {
    pub pass: &'static str,
    pub invalid_landings: usize,
    pub unrooted: usize,
    pub brace_tops_detached: usize,
}

/// Every landing check one run made, and the finished field's own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LandingLedger {
    /// One entry per pass that edits landings, in the order they ran.
    pub passes: Vec<LandingPass>,
    /// The check of the strut set this file contains.
    pub finished: LandingCheck,
    /// `injected - plate reaction - uncarried`, in mm², on the finished field.
    /// Zero when every mm² the down-sweep routed arrived somewhere it can
    /// name; the load a vanished parent swallowed shows up here and nowhere
    /// else.
    pub load_gap_mm2: f64,
    /// The part of `load_gap_mm2` the down-sweep can name: injected peel
    /// carried under a larger event in the same arm
    /// (`DownSweepOutcome::absorbed_event_load_mm2`). Not lost.
    pub absorbed_mm2: f64,
    /// Reads that absorbed an event. The path-hit witness for `absorbed_mm2`.
    pub absorbed_reads: u64,
}

impl LandingLedger {
    /// The load that went NOWHERE the down-sweep can name:
    /// `injected - plate reaction - uncarried - absorbed`. Zero on a field
    /// whose every landing stands; a parent that is not there to pick up a
    /// child's load shows up here and nowhere else.
    #[must_use]
    pub fn unexplained_gap_mm2(&self) -> f64 {
        self.load_gap_mm2 - self.absorbed_mm2
    }

    /// Record the check of one pass.
    pub fn record(&mut self, pass: &'static str, check: &LandingCheck) {
        self.passes.push(LandingPass {
            pass,
            invalid_landings: check.invalid_landings(),
            unrooted: check.unrooted.len(),
            brace_tops_detached: check.brace_tops_detached.len(),
        });
    }

    /// The most unrooted members any pass left behind — a pass that broke a
    /// chain and a later pass that happened to mend it are both visible.
    #[must_use]
    pub fn worst_pass_unrooted(&self) -> usize {
        self.passes
            .iter()
            .map(|pass| pass.unrooted)
            .max()
            .unwrap_or(0)
    }
}

/// Whether `parent` stands at `(cell, layer)` in the field as it is NOW.
///
/// The one predicate every landing-editing pass shares: the slenderness merge
/// asks it before it accepts a shaft from its index, the un-merge asks it
/// before it trusts a landing, and this module asks it of every landing after
/// every pass. `O(1)`.
#[must_use]
pub fn stands_at(parent: &SweepStrut, cell: usize, layer: usize) -> bool {
    parent.k_bottom != usize::MAX
        && parent.k_bottom <= layer
        && layer <= parent.k_top
        && parent.cell_at(layer) == cell
}

/// Which members something else rests on or is tied to, indexed by strut
/// position. A member flagged here may not be moved: moving it moves the
/// ground from under its dependants.
#[must_use]
pub fn carries_someone(struts: &[SweepStrut]) -> Vec<bool> {
    let mut carries = vec![false; struts.len()];
    for strut in struts {
        if let Landing::Strut { id, .. } = strut.landing {
            if let Some(flag) = carries.get_mut(id as usize) {
                *flag = true;
            }
        }
        if let Some(parent) = strut.seat_on_strut {
            if let Some(flag) = carries.get_mut(parent as usize) {
                *flag = true;
            }
        }
    }
    carries
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Root {
    Unknown,
    Walking,
    Rooted,
    Unrooted,
}

/// Check every landing of one strut set.
#[must_use]
pub fn check_landings(struts: &[SweepStrut]) -> LandingCheck {
    let mut check = LandingCheck::default();
    // Per strut: whether its OWN landing is valid (plate and model always are),
    // and whom it rests on when it rests on a member.
    let mut own_valid = vec![true; struts.len()];
    let mut parent_of: Vec<Option<usize>> = vec![None; struts.len()];
    for (index, strut) in struts.iter().enumerate() {
        if strut.kind == StrutKind::Brace {
            if let Some(seat_on) = strut.seat_on_strut {
                let tied = struts
                    .get(seat_on as usize)
                    .is_some_and(|other| stands_at(other, strut.cell, strut.k_top));
                if !tied {
                    check.brace_tops_detached.push(strut.id);
                }
            }
        }
        let Landing::Strut { id, layer } = strut.landing else {
            if strut.k_bottom == usize::MAX {
                // Never landed at all: nothing under it.
                own_valid[index] = false;
                check.parent_absent.push(strut.id);
            }
            continue;
        };
        check.strut_landings += 1;
        parent_of[index] = Some(id as usize);
        let Some(parent) = struts.get(id as usize) else {
            own_valid[index] = false;
            check.parent_absent.push(strut.id);
            continue;
        };
        let spans = parent.k_bottom != usize::MAX
            && parent.k_bottom <= layer
            && layer <= parent.k_top
            && strut.k_bottom == layer;
        if !spans {
            own_valid[index] = false;
            check.parent_absent.push(strut.id);
        } else if !stands_at(parent, strut.base_cell(), layer) {
            own_valid[index] = false;
            check.parent_elsewhere.push(strut.id);
        }
    }

    // Close the relation down every chain, memoised, iteratively: a member is
    // rooted when its own landing is valid and what it rests on is rooted. A
    // cycle roots nothing.
    let mut state = vec![Root::Unknown; struts.len()];
    let mut chain: Vec<usize> = Vec::new();
    for start in 0..struts.len() {
        if state[start] != Root::Unknown {
            continue;
        }
        chain.clear();
        let mut at = start;
        let verdict = loop {
            match state[at] {
                Root::Rooted => break Root::Rooted,
                Root::Unrooted | Root::Walking => break Root::Unrooted,
                Root::Unknown => {}
            }
            if !own_valid[at] {
                state[at] = Root::Unrooted;
                break Root::Unrooted;
            }
            match parent_of[at] {
                None => {
                    state[at] = Root::Rooted;
                    break Root::Rooted;
                }
                Some(parent) => {
                    state[at] = Root::Walking;
                    chain.push(at);
                    at = parent;
                }
            }
        };
        for &member in &chain {
            state[member] = verdict;
        }
    }
    check.unrooted = struts
        .iter()
        .zip(&state)
        .filter(|(_, root)| **root != Root::Rooted)
        .map(|(strut, _)| strut.id)
        .collect();
    check
}

#[cfg(test)]
mod tests {
    use super::{check_landings, stands_at};
    use crate::{Landing, StrutKind, SweepStrut};

    fn arm(id: u32, cell: usize, k_top: usize, k_bottom: usize, landing: Landing) -> SweepStrut {
        SweepStrut {
            id,
            cell,
            center_mm: [0.0, 0.0],
            kind: StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top,
            k_bottom,
            landing,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(),
            inserted_by_load: false,
            path: Vec::new(),
            seat_solid_layers: 0,
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
        }
    }

    /// A child on a straight trunk that stands at its layer and cell is
    /// rooted; the same child after the trunk has been shortened above its
    /// landing layer is not, and neither is anything resting on the child.
    #[test]
    fn a_landing_on_a_parent_that_has_moved_is_named_with_everything_above_it() {
        let trunk = arm(0, 5, 100, 0, Landing::Plate);
        let mut child = arm(1, 6, 60, 40, Landing::Strut { id: 0, layer: 40 });
        child.path = std::iter::once(5)
            .chain(std::iter::repeat_n(6, 19))
            .collect();
        let grandchild = arm(2, 7, 80, 50, Landing::Strut { id: 1, layer: 50 });
        let mut grandchild = grandchild;
        grandchild.path = std::iter::once(6)
            .chain(std::iter::repeat_n(7, 29))
            .collect();

        let clean = check_landings(&[trunk.clone(), child.clone(), grandchild.clone()]);
        assert_eq!(clean.strut_landings, 2);
        assert!(clean.clean(), "{clean:?}");

        // The trunk is merged away above the child's landing layer.
        let mut moved = trunk;
        moved.k_bottom = 45;
        moved.landing = Landing::Model { layer: 45 };
        let broken = check_landings(&[moved, child, grandchild]);
        assert_eq!(broken.parent_absent, vec![1]);
        assert_eq!(broken.unrooted, vec![1, 2]);
    }

    /// The un-merge route: the parent still spans the layer but stands three
    /// cells away.
    #[test]
    fn a_parent_moved_sideways_is_named_as_elsewhere() {
        let mut trunk = arm(0, 5, 100, 0, Landing::Plate);
        let child = {
            let mut child = arm(1, 9, 60, 40, Landing::Strut { id: 0, layer: 40 });
            child.path = std::iter::once(8)
                .chain(std::iter::repeat_n(9, 19))
                .collect();
            child
        };
        trunk.path = (0..100)
            .map(|layer| if layer == 40 { 8 } else { 5 })
            .collect();
        assert!(stands_at(&trunk, 8, 40));
        assert!(check_landings(&[trunk.clone(), child.clone()]).clean());
        trunk.path.clear();
        let broken = check_landings(&[trunk, child]);
        assert_eq!(broken.parent_elsewhere, vec![1]);
        assert_eq!(broken.unrooted, vec![1]);
    }

    /// A cycle roots nothing, and the walk terminates.
    #[test]
    fn a_cycle_of_landings_is_unrooted() {
        let one = arm(0, 5, 100, 50, Landing::Strut { id: 1, layer: 50 });
        let two = arm(1, 5, 100, 50, Landing::Strut { id: 0, layer: 50 });
        let check = check_landings(&[one, two]);
        assert_eq!(check.unrooted, vec![0, 1]);
    }
}
