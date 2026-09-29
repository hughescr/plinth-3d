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
//!    do not share volume: their axes are at least `r₁ + r₂` apart. Members
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
    /// Drawn radius per layer from `k_bottom` upward (the member's DRAWN
    /// profile); never read below `fallback_radius_mm`, the member's own
    /// minimum.
    pub profile: &'a [f64],
    pub k_bottom: usize,
    pub k_top: usize,
    pub fallback_radius_mm: f64,
}

impl MemberSolid<'_> {
    /// The drawn radius at the point `t ∈ [0, 1]` along the axis from `from`
    /// (the base, at `k_bottom`) to `to` (the seat, at `k_top`): the larger of
    /// the two layers the point lies between, so a taper is never read thin.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a fraction of a layer span bounded by the plan's own layer count"
    )]
    pub fn radius_at(&self, t: f64) -> f64 {
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

/// The first standing member whose solid the solid of radius `radius_mm` about
/// `from`–`to` would enter, other than the ones `joined` excuses; `None` when
/// it enters none. Two solids touch when their axes are closer than the sum of
/// their radii at the closest points.
pub(crate) fn member_intrusion<'a>(
    members: impl IntoIterator<Item = MemberSolid<'a>>,
    from: [f64; 3],
    to: [f64; 3],
    radius_mm: f64,
    joined: impl Fn(u32) -> bool,
) -> Option<(u32, f64)> {
    for member in members {
        if joined(member.id) {
            continue;
        }
        // Bounding-box rejection with the member's largest drawn radius.
        let widest = member
            .profile
            .iter()
            .copied()
            .fold(member.fallback_radius_mm, f64::max);
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
        let length = geom_kernel::vec3::dist(member.from, member.to);
        let t = if length > 0.0 {
            geom_kernel::vec3::dist(member.from, on_member) / length
        } else {
            0.0
        };
        let gap = distance - radius_mm - member.radius_at(t);
        if gap < 0.0 {
            return Some((member.id, gap));
        }
    }
    None
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
        };
        let a = [-1.0, 0.0, 0.0];
        let b = [1.0, 0.0, 0.0];
        assert!(member_intrusion([other.clone()], a, b, 0.14, |_| false).is_none());
        assert!(member_intrusion([other.clone()], a, b, 0.2, |_| false).is_some());
        assert!(member_intrusion([other], a, b, 0.2, |id| id == 7).is_none());
    }
}
