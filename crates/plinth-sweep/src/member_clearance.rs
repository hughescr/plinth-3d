//! **A member's DRAWN SOLID, not its centreline** (fix/member-clearance,
//! 2026-09-29).
//!
//! Until this lane the only collision test a brace diagonal or a mast got was
//! the centreline CELL at each layer (`slenderness::segment_path`): is the
//! cell the axis rounds to cured model at this layer, and does another
//! member's axis stand in the same cell at the same layer. The member's radius
//! was never in the test and there was no clearance. Measured on develop
//! `c33dc8e` (`brace-collide-diagnosis.md`):
//!
//! * the stilt plate's one `|X|` between posts 75 and 69 passed its axis
//!   0.048 mm from a leg's corner and its 0.292 mm solid entered the leg by
//!   0.244 mm;
//! * four long crosses on the same plate passed 0.1-0.4 mm apart, and their
//!   solids overlapped each other in 8 places by up to 0.454 mm.
//!
//! The two rules here, both on the straight axis the lowering draws a member
//! along (`adapter::drawn_ends`: seat over its seat cell at `k_top`, base over
//! its base cell at `k_bottom`):
//!
//! 1. **Member against model.** A brace or a mast is a member, not a seat; its
//!    solid of radius `r` clears the cured model by the registered FINAL
//!    clearance gap (`FINAL_CLEARANCE_GAP_MM`, 0.3 mm, the owner's ruling of
//!    2026-08-22) along its whole axis, both end layers included.
//! 2. **Member against member.** Two members that are not joined to each other
//!    do not share volume: the candidate's axis is at least `r₁ + r₂` from
//!    the standing member's axis on every span the file draws it as, `r₂`
//!    that span's wider end (round-3 review, 2026-09-29: the closest pair of
//!    axes is not the closest pair of solids when the radius varies). Members
//!    that ARE joined (a diagonal and the post it seats on or lands on, or two
//!    members of which one lands on the other) meet at the planned joint and
//!    are not tested against each other.
//!
//! # How the model is read, and the one resolution term
//!
//! The model is the up-sweep's own record of cured material,
//! [`ColumnHistory`]'s per-column model spans: a cell cured over layers
//! `kb..=kt` is the vertical segment over that cell's centre from `kb·h` to
//! `(kt + 1)·h` (layer `k`'s lamina stands on the plane `k·h`; a seat at `k` is
//! the bottom plane of the lamina above it). The raster knows the surface only
//! to within its own cell, so a cured cell centre counts as model out to
//! HALF THE CELL DIAGONAL (`pitch·√2/2`): a member clears when its axis is
//! farther than `r + gap + pitch·√2/2` from every cured cell centre. That term
//! is the raster's resolution, not a tuned margin; it vanishes as the pitch
//! does.
//!
//! # Batch-first
//!
//! One member costs one pass over the cells of its own bounding box, and each
//! cured cell there one walk of that cell's (run-length) model spans with an
//! O(1) segment-to-segment distance. Nothing is rasterised per member and no
//! per-layer field is built: a distance field per layer is `cells × layers`
//! (2.9 GB of `f32` on Round 100) for a question asked of a few thousand
//! members.

use geom_kernel::segment_segment_distance_min;

use crate::grid::Grid;
use crate::up_sweep::ColumnHistory;

/// A point on a member's axis, in the sweep's own frame: the cell's centre in
/// X and Y, and `layer · h` above the plate.
#[expect(
    clippy::cast_precision_loss,
    reason = "a layer index bounded by the plan's own layer count is exact in f64"
)]
pub(crate) fn axis_point(grid: &Grid, layer_height_mm: f64, cell: usize, layer: usize) -> [f64; 3] {
    [
        grid.center_x_mm(grid.col_of(cell)),
        grid.center_y_mm(grid.row_of(cell)),
        layer as f64 * layer_height_mm,
    ]
}

/// The raster's own resolution: half a cell's diagonal. A cured cell centre
/// stands for material anywhere in its cell.
pub(crate) fn raster_half_diagonal_mm(grid: &Grid) -> f64 {
    grid.pitch_mm() * core::f64::consts::FRAC_1_SQRT_2
}

/// Whether the solid of radius `radius_mm` about the straight axis `from`–`to`
/// stays farther than `gap_mm` from every cured model cell (plus the raster's
/// half-diagonal, see the module doc).
///
/// Returns the first offending cell and its distance when it does not, so a
/// caller can count or report it; `None` when the member clears.
pub(crate) fn model_intrusion(
    grid: &Grid,
    history: &ColumnHistory,
    layer_height_mm: f64,
    from: [f64; 3],
    to: [f64; 3],
    radius_mm: f64,
    gap_mm: f64,
) -> Option<(usize, f64)> {
    let reach = radius_mm.max(0.0) + gap_mm.max(0.0) + raster_half_diagonal_mm(grid);
    if !reach.is_finite() {
        return None;
    }
    let pitch = grid.pitch_mm();
    let (x_lo, x_hi) = (from[0].min(to[0]) - reach, from[0].max(to[0]) + reach);
    let (y_lo, y_hi) = (from[1].min(to[1]) - reach, from[1].max(to[1]) + reach);
    let (z_lo, z_hi) = (from[2].min(to[2]) - reach, from[2].max(to[2]) + reach);
    let col_of = |x: f64| ((x - grid.center_x_mm(0)) / pitch).floor();
    let row_of = |y: f64| ((y - grid.center_y_mm(0)) / pitch).floor();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite coordinate over the grid pitch, floored and clamped into the grid"
    )]
    let clamp = |value: f64, limit: usize| -> usize {
        if value <= 0.0 {
            0
        } else {
            (value as usize).min(limit.saturating_sub(1))
        }
    };
    let (cols, rows) = (grid.cols(), grid.rows());
    if cols == 0 || rows == 0 {
        return None;
    }
    let (c0, c1) = (clamp(col_of(x_lo), cols), clamp(col_of(x_hi) + 1.0, cols));
    let (r0, r1) = (clamp(row_of(y_lo), rows), clamp(row_of(y_hi) + 1.0, rows));
    // The axis's own horizontal projection, for the cheap 2-D rejection: the
    // 3-D distance to a vertical segment is never less than the 2-D distance
    // from its foot to the projected axis.
    let flat_from = [from[0], from[1], 0.0];
    let flat_to = [to[0], to[1], 0.0];
    for row in r0..=r1 {
        let y = grid.center_y_mm(row);
        for col in c0..=c1 {
            let cell = row * cols + col;
            let Some(&head) = history.model_head.get(cell) else {
                continue;
            };
            if head == u32::MAX {
                continue;
            }
            let x = grid.center_x_mm(col);
            let foot = [x, y, 0.0];
            let (flat, _, _) = segment_segment_distance_min(flat_from, flat_to, foot, foot);
            if flat >= reach {
                continue;
            }
            let mut slot = head;
            while slot != u32::MAX {
                let span = history.model_spans[slot as usize];
                slot = span.next;
                let (bottom, top) = (
                    f64::from(span.k_bottom) * layer_height_mm,
                    (f64::from(span.k_top) + 1.0) * layer_height_mm,
                );
                if top < z_lo || bottom > z_hi {
                    continue;
                }
                let (distance, _, _) =
                    segment_segment_distance_min(from, to, [x, y, bottom], [x, y, top]);
                if distance < reach {
                    return Some((cell, distance));
                }
            }
        }
    }
    None
}

/// One standing member's solid as the tie rungs test against: its straight
/// axis and the radius it is drawn at along it.
#[derive(Clone, Debug)]
pub(crate) struct MemberSolid<'a> {
    pub id: u32,
    pub from: [f64; 3],
    pub to: [f64; 3],
    /// Drawn radius per slot from `k_bottom` upward (the member's DRAWN
    /// profile, the one the lowering gets); never read below
    /// `fallback_radius_mm`, the member's own minimum.
    pub profile: &'a [f64],
    pub k_bottom: usize,
    pub k_top: usize,
    pub fallback_radius_mm: f64,
    /// The member's tip radius (`SweepStrut::tip_radius_mm`), which the
    /// lowering starts the shaft's top span at where it is wider.
    pub tip_radius_mm: f64,
}

impl MemberSolid<'_> {
    /// The member's widest drawn radius anywhere along it: no drawn span is
    /// wider than the widest of the profile, the tip and the root, and the
    /// root is the wider of the profile's first slot and the tip.
    fn widest_radius_mm(&self) -> f64 {
        self.profile
            .iter()
            .copied()
            .fold(self.fallback_radius_mm.max(self.tip_radius_mm), f64::max)
    }

    /// The straight spans the lowering draws this member as
    /// ([`plinth_support::drawn_spans`], the one rule): each a chord between
    /// two radii, standing on or above the profile at every height, from the
    /// seat (`to`) down to the base (`from`), in the member's own frame. `None`
    /// when the member has no height to span.
    fn drawn_spans(&self) -> Option<Vec<plinth_support::DrawnSpan>> {
        let (base_z, seat_z) = (self.from[2], self.to[2]);
        // `NaN` ends have no height either.
        if seat_z.partial_cmp(&base_z) != Some(core::cmp::Ordering::Greater) {
            return None;
        }
        let root = plinth_support::drawn_root_radius_mm(self.profile, self.tip_radius_mm);
        Some(plinth_support::drawn_spans(
            self.profile,
            base_z,
            seat_z,
            seat_z,
            base_z,
            self.tip_radius_mm,
            root,
        ))
    }

    /// The point of the axis at height `z` (the axis runs from `from` at the
    /// base to `to` at the seat).
    fn axis_at(&self, z: f64) -> [f64; 3] {
        let rise = self.to[2] - self.from[2];
        let t = if rise > 0.0 {
            ((z - self.from[2]) / rise).clamp(0.0, 1.0)
        } else {
            0.0
        };
        std::array::from_fn(|axis| self.from[axis] + t * (self.to[axis] - self.from[axis]))
    }

    /// The RETIRED reading, kept for the path-hit witness only (it never
    /// decides): the drawn radius at the point `t ∈ [0, 1]` along the axis,
    /// the larger of the two layers the point lies between.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a fraction of a layer span bounded by the plan's own layer count"
    )]
    fn retired_closest_point_radius_mm(&self, t: f64) -> f64 {
        let span = self.k_top.saturating_sub(self.k_bottom);
        let at = (t.clamp(0.0, 1.0) * span as f64).floor() as usize;
        let below = self.profile.get(at).copied();
        let above = self.profile.get(at + 1).copied();
        match (below, above) {
            (Some(a), Some(b)) => a.max(b),
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => self
                .profile
                .last()
                .copied()
                .unwrap_or(self.fallback_radius_mm),
        }
        .max(self.fallback_radius_mm)
    }
}

/// One member-against-member refusal: the standing member entered and the
/// most negative radius gap found over its drawn spans.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MemberIntrusion {
    pub id: u32,
    pub gap_mm: f64,
}

/// The verdict of [`member_intrusion`]: the first standing member entered
/// (`None` when the candidate clears them all), and what the RETIRED
/// closest-axes reading (`c33dc8e`..`0f70cf2`: the radius at the closest axis
/// point, the larger of the two layers it lies between) would have said about
/// the same candidate. The second is a witness only and never decides: where
/// the two verdicts differ, this rule changed the field (the path-hit
/// witness of the 2026-09-29 round-3 review fix, both directions).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MemberVerdict {
    pub entered: Option<MemberIntrusion>,
    pub retired_reading_refused: bool,
}

impl MemberVerdict {
    /// Refused here, where the retired reading admitted.
    pub fn refused_beyond_the_closest_axes(&self) -> bool {
        self.entered.is_some() && !self.retired_reading_refused
    }

    /// Admitted here, where the retired reading refused (it read the slot
    /// above the closest point too, and a thin span clears where that slot
    /// was wide).
    pub fn admitted_where_the_retired_reading_refused(&self) -> bool {
        self.entered.is_none() && self.retired_reading_refused
    }
}

/// Whether the solid of radius `radius_mm` about `from`–`to` would enter a
/// standing member, other than the ones `joined` excuses (see
/// [`MemberVerdict`]).
///
/// **The rule (round-3 review, 2026-09-29).** A standing member's radius
/// varies along it, so the closest pair of AXES need not be the closest pair
/// of SOLIDS: a wide section a little way from the closest axis point can
/// reach the candidate while the thin section at that point does not. So the
/// standing member is measured as the SPANS the file draws it as
/// ([`MemberSolid::drawn_spans`], the lowering's own rule): a span is a
/// frustum whose radius nowhere exceeds the wider of its two end radii, and
/// the two solids share volume when, on SOME span, the candidate's axis comes
/// closer to that span's axis than `radius_mm` plus that wider radius. A
/// candidate that clears every span clears the drawn shaft; the only error is
/// toward refusal (a frustum read at its wider end). The ledger's step profile
/// is NOT a bound: a span is a chord and can stand wider than the ledger at a
/// slot it bridges.
///
/// Batch-first: a member whose axis is farther than `radius_mm` plus its
/// WIDEST radius from the candidate is cleared by that one distance (the
/// common case); only one that is not is walked, one O(1) segment distance
/// per drawn span. The retired reading costs one more O(1) distance on a
/// member that reaches the walk.
pub(crate) fn member_intrusion<'a>(
    members: impl IntoIterator<Item = MemberSolid<'a>>,
    from: [f64; 3],
    to: [f64; 3],
    radius_mm: f64,
    joined: impl Fn(u32) -> bool,
) -> MemberVerdict {
    let mut verdict = MemberVerdict {
        entered: None,
        retired_reading_refused: false,
    };
    for member in members {
        if joined(member.id) {
            continue;
        }
        // Bounding-box rejection with the member's largest drawn radius.
        let widest = member.widest_radius_mm();
        let reach = radius_mm + widest;
        let apart = (0..3).any(|axis| {
            from[axis].min(to[axis]) - reach > member.from[axis].max(member.to[axis])
                || from[axis].max(to[axis]) + reach < member.from[axis].min(member.to[axis])
        });
        if apart {
            continue;
        }
        let (distance, _, on_member) =
            segment_segment_distance_min(from, to, member.from, member.to);
        // Every radius the retired reading could read is at most `widest`, so
        // a member cleared above was cleared by it too.
        if distance - reach >= 0.0 {
            continue;
        }
        if !verdict.retired_reading_refused {
            let length = geom_kernel::vec3::dist(member.from, member.to);
            let t = if length > 0.0 {
                geom_kernel::vec3::dist(member.from, on_member) / length
            } else {
                0.0
            };
            verdict.retired_reading_refused =
                distance - radius_mm - member.retired_closest_point_radius_mm(t) < 0.0;
        }
        if verdict.entered.is_none() {
            verdict.entered = walk_spans(&member, from, to, radius_mm, widest, distance);
        }
        if verdict.entered.is_some() && verdict.retired_reading_refused {
            break;
        }
    }
    verdict
}

/// The per-span walk of [`member_intrusion`] on one standing member whose
/// widest radius does not clear the candidate: the most negative gap over its
/// drawn spans, or `None` when every span clears. `closest_axes_mm` is the
/// whole-axis distance (a member with no height is read at its widest there).
fn walk_spans(
    member: &MemberSolid<'_>,
    from: [f64; 3],
    to: [f64; 3],
    radius_mm: f64,
    widest: f64,
    closest_axes_mm: f64,
) -> Option<MemberIntrusion> {
    let Some(spans) = member.drawn_spans() else {
        // No height to span: the member is read at its widest.
        return Some(MemberIntrusion {
            id: member.id,
            gap_mm: closest_axes_mm - radius_mm - widest,
        });
    };
    let mut worst: Option<f64> = None;
    for span in &spans {
        let bound = span
            .top_radius_mm
            .max(span.bottom_radius_mm)
            .max(member.fallback_radius_mm);
        let (distance, _, _) = segment_segment_distance_min(
            from,
            to,
            member.axis_at(span.top_z_mm),
            member.axis_at(span.bottom_z_mm),
        );
        let gap = distance - radius_mm - bound;
        if gap < 0.0 && worst.is_none_or(|worst_gap| gap < worst_gap) {
            worst = Some(gap);
        }
    }
    worst.map(|gap_mm| MemberIntrusion {
        id: member.id,
        gap_mm,
    })
}

#[cfg(test)]
mod tests {
    use super::{member_intrusion, model_intrusion, MemberSolid};
    use crate::grid::lay_out_at;
    use crate::up_sweep::{ColumnHistory, ShaftSpan};

    /// A single cured column, one cell wide, over layers 10..=20; a member
    /// passing beside it is refused inside `r + gap + half-diagonal` and
    /// cleared outside it.
    #[test]
    fn a_member_clears_a_cured_column_only_beyond_radius_gap_and_half_a_cell() {
        let grid = lay_out_at(0.0, 0.0, 0.05, 100, 100);
        let mut history = ColumnHistory::new(&grid);
        let cell = grid.index(50, 50);
        history.model_spans.push(ShaftSpan {
            k_bottom: 10,
            k_top: 20,
            id: 0,
            next: u32::MAX,
        });
        history.model_head[cell] = 0;
        let h = 0.05;
        let x = grid.center_x_mm(50);
        let y = grid.center_y_mm(50);
        let half = super::raster_half_diagonal_mm(&grid);
        let (r, gap) = (0.1, 0.3);
        // A vertical member beside the column at distance d, over its layers.
        let at =
            |d: f64| model_intrusion(&grid, &history, h, [x + d, y, 0.2], [x + d, y, 1.2], r, gap);
        assert!(at(r + gap + half - 1.0e-6).is_some());
        assert!(at(r + gap + half + 1.0e-6).is_none());
        // Entirely below the cured run, farther than the reach: clear.
        let below = model_intrusion(
            &grid,
            &history,
            h,
            [x, y, 0.0],
            [x, y, 0.5 - (r + gap + half) - 1.0e-6],
            r,
            gap,
        );
        assert!(below.is_none(), "{below:?}");
    }

    /// Two crossing axes 0.3 mm apart: radii 0.1 + 0.1 clear, 0.2 + 0.15 do not;
    /// a joined member is never tested.
    #[test]
    fn two_members_interpenetrate_when_their_axes_are_closer_than_their_radii() {
        let profile = [0.15_f64; 11];
        let other = MemberSolid {
            id: 7,
            from: [0.0, -1.0, 0.3],
            to: [0.0, 1.0, 0.3],
            profile: &profile,
            k_bottom: 0,
            k_top: 10,
            fallback_radius_mm: 0.05,
            tip_radius_mm: 0.15,
        };
        let a = [-1.0, 0.0, 0.0];
        let b = [1.0, 0.0, 0.0];
        assert!(member_intrusion([other.clone()], a, b, 0.14, |_| false)
            .entered
            .is_none());
        assert!(member_intrusion([other.clone()], a, b, 0.2, |_| false)
            .entered
            .is_some());
        assert!(member_intrusion([other], a, b, 0.2, |id| id == 7)
            .entered
            .is_none());
    }

    /// A standing arm on the axis `(0,0,0)–(0,0,10)` drawn from `profile`,
    /// one slot per `10 / profile.len()` mm, tip `profile.last()`.
    fn standing(profile: &[f64]) -> MemberSolid<'_> {
        MemberSolid {
            id: 3,
            from: [0.0, 0.0, 0.0],
            to: [0.0, 0.0, 10.0],
            profile,
            k_bottom: 0,
            k_top: profile.len(),
            fallback_radius_mm: 0.05,
            tip_radius_mm: profile.last().copied().unwrap_or(0.05),
        }
    }

    /// The cross-family review's case (round 3, 2026-09-29). A standing arm on
    /// the axis `(0,0,0)–(0,0,10)`, one slot per mm: radius 0.8 mm up to
    /// `z = 8`, 0.1 mm above it (to the tip at `z = 10`). A candidate of
    /// radius 0.1 mm from `(0.6, 0, 7.9)` to `(0.4, 0, 9)`.
    ///
    /// The closest AXIS pair is at `z = 9` (0.4 mm apart, where the arm is
    /// 0.1 mm): `0.4 − 0.1 − 0.1 = +0.2`, clear. But at `z = 8` the
    /// candidate's axis is `0.6 − 0.2·(0.1/1.1) = 0.5818` mm from the arm's,
    /// inside its 0.8 mm section: the flat-ended solids overlap by
    /// `0.5818 − 0.8 − 0.1 = −0.3182` mm. Hand-derived.
    ///
    /// The lowering draws this profile as two spans, 0.8 mm below `z = 8` and
    /// 0.1 mm above. The walk reads each span round-ended about its axis (a
    /// capsule, which holds the frustum): the 0.8 mm span's top end `(0,0,8)`
    /// is `(0.6 − 0.1·s)/√(1 + s²) = 0.5724` mm from the candidate's line,
    /// `s = 0.2/1.1`, so the walk reports `−0.3276` mm, beyond the true
    /// `−0.3182`: the capsule errs only toward refusal.
    ///
    /// Control: the same candidate beside an arm of 0.1 mm all along is clear
    /// (0.4 mm axis gap against 0.2 mm of radii), so the rule does not refuse
    /// by the widest radius alone.
    #[test]
    fn a_wider_section_away_from_the_closest_axes_is_still_entered() {
        let mut profile = [0.8_f64; 10];
        profile[8] = 0.1;
        profile[9] = 0.1;
        let a = [0.6, 0.0, 7.9];
        let b = [0.4, 0.0, 9.0];
        let verdict = member_intrusion([standing(&profile)], a, b, 0.1, |_| false);
        // The path-hit witness: the retired closest-axes reading admitted it.
        assert!(verdict.refused_beyond_the_closest_axes(), "{verdict:?}");
        let found = verdict.entered.expect(
            "the candidate's solid enters the arm's 0.8 mm section at z = 7.9..8, although \
             the closest axes (z = 9) are 0.2 mm clear",
        );
        assert_eq!(found.id, 3);
        let s: f64 = 0.2 / 1.1;
        let expected = (0.6 - 0.1 * s) / (1.0 + s * s).sqrt() - 0.8 - 0.1;
        assert!(
            (found.gap_mm - expected).abs() < 1.0e-9,
            "{found:?} against {expected}"
        );
        let flat_ended = (0.6 - 0.1 * s) - 0.8 - 0.1;
        assert!(found.gap_mm <= flat_ended, "{found:?} against {flat_ended}");
        let thin = [0.1_f64; 10];
        assert!(member_intrusion([standing(&thin)], a, b, 0.1, |_| false)
            .entered
            .is_none());
    }

    /// A span is a CHORD, not the ledger's step. The profile (base to tip)
    /// `0.8, 0.1, 0.1, 0.1, 0.8` over 2 mm slots is drawn as ONE 0.8 mm span
    /// (the chord between the two 0.8 mm ends never dips below the ledger), so
    /// at `z = 5`, where the ledger says 0.1 mm, the file's solid is 0.8 mm. A
    /// candidate of 0.1 mm crossing at 0.6 mm from the axis there enters it by
    /// `0.6 − 0.8 − 0.1 = −0.3` mm, although the ledger at that height (0.1 mm)
    /// and the retired closest-axes reading say `+0.4`. Sabotage: a bound read
    /// from the ledger's local slots goes red here.
    #[test]
    fn a_span_that_bridges_thin_slots_is_measured_at_its_drawn_radius() {
        let profile = [0.8, 0.1, 0.1, 0.1, 0.8];
        let a = [-1.0, 0.6, 5.0];
        let b = [1.0, 0.6, 5.0];
        let verdict = member_intrusion([standing(&profile)], a, b, 0.1, |_| false);
        assert!(verdict.refused_beyond_the_closest_axes(), "{verdict:?}");
        let found = verdict
            .entered
            .expect("the drawn 0.8 mm span reaches the candidate at z = 5");
        assert!((found.gap_mm - (-0.3)).abs() < 1.0e-9, "{found:?}");
    }

    /// The other direction: the retired reading read the slot ABOVE the
    /// closest point too, so it refused where the drawn solid clears. Profile
    /// (base to tip, 1 mm slots) `0.1 × 6, 0.8, 0.1 × 3` is drawn as three
    /// spans: 0.1 mm to `z = 6`, 0.8 mm from 6 to 7, 0.1 mm above. A 0.1 mm
    /// candidate crossing at 0.6 mm from the axis at `z = 5.1` is 0.4 mm clear
    /// of the thin span, and the wide span's end `(0,0,6)` is
    /// `√(0.6² + 0.9²) = 1.0817` mm away (`+0.1817` against 0.9 mm of radii):
    /// clear. The retired reading, at slot 5, took `max(0.1, 0.8)` and refused
    /// (`0.6 − 0.8 − 0.1 = −0.3`). Hand-derived.
    #[test]
    fn a_thin_span_below_a_wide_one_is_admitted_where_the_retired_reading_refused() {
        let profile = [0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.8, 0.1, 0.1, 0.1];
        let a = [-1.0, 0.6, 5.1];
        let b = [1.0, 0.6, 5.1];
        let verdict = member_intrusion([standing(&profile)], a, b, 0.1, |_| false);
        assert!(verdict.entered.is_none(), "{verdict:?}");
        assert!(
            verdict.admitted_where_the_retired_reading_refused(),
            "{verdict:?}"
        );
    }
}
