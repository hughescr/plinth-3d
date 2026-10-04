//! **TIES TO THE MODEL: the last rung of the slenderness ladder before a
//! warning** (Craig ruling 2026-10-03, decision D8 of
//! `field-2026-09-28/bracing-mechanics.md`, revision 2).
//!
//! # Why
//!
//! A tall post that no other rung can hold — every `|X|` to another post
//! passes through the model, a mast would be thicker than the post it braces,
//! thickening runs past the maximum radius — was a warning. The lonely peak's
//! three posts and the brace pair's long posts stand beside the model's own
//! 2 x 2 mm columns. A member from such a post to the column's own surface
//! holds the post sideways, and the column is far stiffer than any post:
//! `I = a⁴/12 = 1.33 mm⁴` for the 2 mm column against `π r⁴/4 = 0.003 mm⁴`
//! for a 0.25 mm post. So the tie's far end is held, not swaying with a
//! partner as a brace band does (`frame_bands`), and the tie point is what a
//! fixed-point tie is assumed to be, to the extent the tie itself is stiff.
//!
//! # What a tie is
//!
//! [`crate::StrutKind::Tie`]: a member that LANDS on the post at `k_bottom`
//! and rises at the overhang law's saturated lean (the brace angle, decision
//! D3) to the model, where it SEATS with a rounded tip exactly as a support
//! arm does — the same seat law, ball, seat trim and contact sizing in the
//! lowering, the same clearance census, and its contact is a scar.
//!
//! **The routing hands it no peel.** The engine routes all of a cell's peel
//! to one carrier, straight down a column of model material where there is
//! one; a tie's seat is on a wall, under no face the routing loads, so the
//! column's peel goes down the column to the seats under it. The tie is
//! therefore, like a brace, neither a carrier nor a reader of peel. Granting
//! it an arm's one-cell floor would book load nobody injected (measured: a
//! load gap of −0.0075 mm² on the brace pair from three ties). The pull a
//! tie may share with the column by stiffness is not in the routing model
//! (an open question in the lane report). Its radius is at least its own
//! slenderness radius `max(r_min, ℓ / S)`, as thick as the frame needs, and
//! (R2, decision D1) never more than the post's drawn radius at the joint: a
//! tie is a brace to the model.
//!
//! # How many, and how thick: counted through a frame
//!
//! A tie is not assumed to be a fixed point. The post's run, clamped at its
//! lower tie, carries the S-law's uniform drag; each tie is a beam rigidly
//! joined to the post and FIXED at the model (a seated tip fused into the
//! model). The frame ([`crate::frame`]) is solved for drag in X and in Y,
//! and the post top's worst-direction sway, the largest singular value of the
//! 2 x 2 compliance of the top, is read in pixels with the same
//! normalisation the one-pixel law uses: a free post at `L = S r` sways one
//! pixel ([`tied_post_sway`]). A tie set is built only where that sway is
//! within one pixel; the ties are thickened only as far as the frame needs,
//! never past the post (R2).
//!
//! A tie holds every direction because its far end is held: a tie across
//! its own plane bends as a beam fixed at both ends, `12 E I_t / ℓ³`, which
//! for the short ties this rung builds is far stiffer than the post's run
//! above it. The frame check is what makes that a measurement rather than an
//! assumption; a tie set that does not hold in some direction is not built.
//! The finished field's run measure then credits a tie, like any landing on a
//! post, in every direction, from the layer its seat prints (tie timing).
//!
//! # Batch-first
//!
//! The contact search is one pass over the cells of the reach box at one
//! layer. Candidate ties are screened together: one surface projection, one
//! contact-disc derivation and one clearance census for all of them.

use crate::frame::Frame;
use crate::grid::Grid;
use crate::up_sweep::ColumnHistory;

/// Every model SURFACE cell at `layer` within `reach_mm` of `centre`
/// (horizontally), nearest first: a cell cured at `layer` with at least one
/// of its four neighbours not cured there, so a tie arriving from outside can
/// meet it. Ties are broken by the lower cell index. One pass over the reach
/// box.
#[must_use]
pub(crate) fn surface_contacts(
    grid: &Grid,
    history: &ColumnHistory,
    centre: usize,
    layer: usize,
    reach_mm: f64,
) -> Vec<(f64, usize)> {
    let (rows, cols) = (grid.rows(), grid.cols());
    if centre >= grid.cells() || !(reach_mm.is_finite() && reach_mm > 0.0) || rows == 0 || cols == 0
    {
        return Vec::new();
    }
    let pitch = grid.pitch_mm();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite positive reach over the grid pitch, rounded up to a cell count"
    )]
    let span = (reach_mm / pitch).ceil() as usize;
    let (row, col) = (grid.row_of(centre), grid.col_of(centre));
    let (r0, r1) = (row.saturating_sub(span), (row + span).min(rows - 1));
    let (c0, c1) = (col.saturating_sub(span), (col + span).min(cols - 1));
    let cured = |r: usize, c: usize| history.model_blocks(r * cols + c, layer);
    let mut found: Vec<(f64, usize)> = Vec::new();
    for r in r0..=r1 {
        for c in c0..=c1 {
            if !cured(r, c) {
                continue;
            }
            let open = (r == 0 || !cured(r - 1, c))
                || (r + 1 >= rows || !cured(r + 1, c))
                || (c == 0 || !cured(r, c - 1))
                || (c + 1 >= cols || !cured(r, c + 1));
            if !open {
                continue;
            }
            let cell = r * cols + c;
            let distance = crate::cell_distance_mm(grid, centre, cell);
            if distance > 0.0 && distance <= reach_mm {
                found.push((distance, cell));
            }
        }
    }
    found.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    found
}

/// **Whether a tie's contact disc lies wholly on the model** — the footprint
/// rule's question (`footprint.rs`: a tip's whole contact disc lies on its
/// seat face), asked of a disc that may stand in a WALL, where the footprint
/// rule's carried-face rasters (down-facing faces) do not apply.
///
/// The disc ([`plinth_support::tip_contact_discs`], the lowering's own
/// derivation) is sampled at its centre and on its rim, no more than one grid
/// pitch apart; each sample is moved one pitch INTO the model along the
/// seat's inward normal and must fall in a cured cell at its layer. One pitch
/// is the raster's own resolution: a cured cell centre stands for material
/// anywhere in its cell. A disc that hangs off a column's corner fails.
#[must_use]
pub(crate) fn disc_on_model(
    grid: &Grid,
    history: &ColumnHistory,
    layer_height_mm: f64,
    plate_z_mm: f64,
    disc: &plinth_support::TipContactDisc,
) -> bool {
    use geom_kernel::vec3;
    let pitch = grid.pitch_mm();
    let Some(normal) = vec3::normalize(disc.outward_normal) else {
        return false;
    };
    if !(disc.radius_mm.is_finite() && disc.radius_mm > 0.0 && layer_height_mm > 0.0) {
        return false;
    }
    // Two unit vectors in the disc's plane.
    let helper = if normal[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
    let Some(u) = vec3::normalize(vec3::cross(normal, helper)) else {
        return false;
    };
    let v = vec3::cross(normal, u);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite positive circumference over the grid pitch, rounded up to a count"
    )]
    let rim = ((core::f64::consts::TAU * disc.radius_mm / pitch).ceil() as usize).max(4);
    let inside = |p: [f64; 3]| -> bool {
        let q = vec3::sub(p, vec3::scale(normal, pitch));
        let col = grid.col_coord(q[0]).floor();
        let row = grid.row_coord(q[1]).floor();
        let layer = ((q[2] - plate_z_mm) / layer_height_mm).floor();
        if !(col >= 0.0 && row >= 0.0 && layer >= 0.0) {
            return false;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "non-negative floored coordinates, checked against the grid below"
        )]
        let (col, row, layer) = (col as usize, row as usize, layer as usize);
        col < grid.cols() && row < grid.rows() && history.model_blocks(row * grid.cols() + col, layer)
    };
    if !inside(disc.centre_mm) {
        return false;
    }
    (0..rim).all(|step| {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a rim sample index bounded by a small count is exact in f64"
        )]
        let angle = core::f64::consts::TAU * step as f64 / rim as f64;
        let (s, c) = angle.sin_cos();
        let p = vec3::add(
            disc.centre_mm,
            vec3::add(vec3::scale(u, c * disc.radius_mm), vec3::scale(v, s * disc.radius_mm)),
        );
        inside(p)
    })
}

/// One tie of a tied post, for [`tied_post_sway`]: where it meets the post
/// (its height on the post's chord), where it is fixed in the model, and its
/// radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TieMember {
    pub junction_z_mm: f64,
    pub seat_mm: [f64; 3],
    pub radius_mm: f64,
}

/// The study's mesh along a post (`frame_bands::POST_DIVISIONS`, the
/// `bands2.ts` discretisation): 24 even divisions plus every tie junction. A
/// discretisation choice, not physics; it only sets where the uniform drag is
/// lumped (the beam elements are exact under nodal loads).
const POST_DIVISIONS: u32 = 24;

/// **How far a tied post's top sways, in pixels, in its worst direction.**
///
/// The post is the straight chord from `base` (clamped: the run's lower tie)
/// to `top` (free), drawn at `post_radius_mm` (the run's equivalent radius);
/// each tie is a beam from the post's chord at its junction height to its
/// seat, rigidly joined to the post and FIXED at the model. The S-law's
/// uniform drag `q` (per unit length of post) is applied in X, then in Y.
/// With `E = q = 1` a free post at `L = S r` sways `S⁴ / 2π` (the one-pixel
/// law: `q L⁴ / (8 E I)` with `I = π r⁴ / 4`), so the sway in pixels is
/// `u · 2π / S⁴`, where `u` is the largest singular value of the top's 2 x 2
/// horizontal compliance — the move under unit drag in the worst horizontal
/// direction. `E`, `q` and the pixel cancel into `S`, exactly as they do in
/// the law the run is measured against.
///
/// `None` where the input is not a post (zero length, a non-positive
/// radius) or the frame could not be solved.
#[must_use]
pub(crate) fn tied_post_sway(
    base: [f64; 3],
    top: [f64; 3],
    post_radius_mm: f64,
    ties: &[TieMember],
    ratio: f64,
) -> Option<f64> {
    let rise = top[2] - base[2];
    let sane = |v: f64| v.is_finite() && v > 0.0;
    if !(sane(rise) && sane(post_radius_mm) && sane(ratio)) {
        return None;
    }
    let at = |z: f64| -> [f64; 3] {
        let t = ((z - base[2]) / rise).clamp(0.0, 1.0);
        std::array::from_fn(|axis| base[axis] + t * (top[axis] - base[axis]))
    };
    let mut levels: Vec<f64> = (0..=POST_DIVISIONS)
        .map(|i| base[2] + rise * f64::from(i) / f64::from(POST_DIVISIONS))
        .collect();
    for tie in ties {
        if !(tie.junction_z_mm > base[2] && tie.junction_z_mm < top[2] && sane(tie.radius_mm)) {
            return None;
        }
        levels.push(tie.junction_z_mm);
    }
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    let mut frame = Frame::new();
    let mut column: Vec<usize> = Vec::with_capacity(levels.len());
    let mut previous = frame.node(at(levels[0]), true);
    column.push(previous);
    for &z in &levels[1..] {
        let node = frame.node(at(z), false);
        if node == previous {
            continue;
        }
        frame.member(previous, node, post_radius_mm);
        column.push(node);
        previous = node;
    }
    for tie in ties {
        let junction = frame.node(at(tie.junction_z_mm), false);
        let seat = frame.node(tie.seat_mm, true);
        frame.member(junction, seat, tie.radius_mm);
    }
    let top_node = *column.last()?;
    // Drag per unit LENGTH of post: each node takes half of each neighbouring
    // division's length.
    let stretch = geom_kernel::vec3::dist(base, top) / rise;
    let share: Vec<f64> = (0..column.len())
        .map(|i| {
            let z = |at: usize| frame.point(column[at])[2];
            let below = if i == 0 { z(i) } else { z(i - 1) };
            let above = if i + 1 < column.len() { z(i + 1) } else { z(i) };
            0.5 * (above - below) * stretch
        })
        .collect();
    let mut compliance = [[0.0_f64; 2]; 2];
    for (direction, unit) in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]].into_iter().enumerate() {
        let loads: Vec<(usize, [f64; 3])> = column
            .iter()
            .zip(&share)
            .map(|(&node, &w)| (node, unit.map(|c| c * w)))
            .collect();
        let solved = frame.solve(&loads);
        if !solved.solved.get(top_node).copied().unwrap_or(false) {
            return None;
        }
        let moved = solved.moved[top_node];
        compliance[0][direction] = moved[0];
        compliance[1][direction] = moved[1];
    }
    // The largest singular value of the 2 x 2 compliance: the square root of
    // the larger eigenvalue of CᵀC.
    let [[a, b], [c, d]] = compliance;
    let (p, q, r) = (a * a + c * c, a * b + c * d, b * b + d * d);
    let largest = 0.5 * (p + r) + (0.25 * (p - r) * (p - r) + q * q).sqrt();
    let worst = largest.max(0.0).sqrt();
    Some(worst * core::f64::consts::TAU / ratio.powi(4))
}

#[cfg(test)]
mod tests {
    use super::{tied_post_sway, TieMember};

    const S: f64 = 40.0;

    /// The one-pixel law on its own: an untied post of length `L` and radius
    /// `r` sways `(L / (S r))⁴` pixels (the uniform-load cantilever, which
    /// the lumped beam frame reproduces to the lumping's own error).
    #[test]
    fn an_untied_post_sways_by_the_one_pixel_law() {
        for (length, radius) in [(35.0, 0.40), (35.0, 0.25), (12.0, 0.30)] {
            let sway = tied_post_sway([0.0; 3], [0.0, 0.0, length], radius, &[], S)
                .expect("a post solves");
            let law = (length / (S * radius)).powi(4);
            assert!(
                (sway - law).abs() <= 1.0e-3 * law,
                "L {length} r {radius}: frame {sway} against the law {law}"
            );
        }
    }

    /// A tie far stiffer than the post, at mid-height, holds the post as a
    /// clamp would: the top sways as the upper half alone, `(L / 2 / (S r))⁴`,
    /// to within the lower half's small rotation at the joint.
    #[test]
    fn a_stiff_tie_at_mid_height_is_close_to_a_clamp() {
        let (length, radius) = (35.0, 0.25);
        let tie = TieMember {
            junction_z_mm: 0.5 * length,
            seat_mm: [1.2, 0.0, 0.5 * length + 0.8],
            radius_mm: 0.6,
        };
        let sway =
            tied_post_sway([0.0; 3], [0.0, 0.0, length], radius, &[tie], S).expect("solves");
        let half = (0.5 * length / (S * radius)).powi(4);
        assert!(sway >= half * (1.0 - 1.0e-6), "tied {sway} below the clamp {half}");
        assert!(sway <= half * 1.10, "tied {sway} more than 10 % over the clamp {half}");
    }

    /// A thin tie bends across its plane: the same post tied by a minimum
    /// radius tie sways more than with a stiff one, in its worst direction.
    #[test]
    fn a_thinner_tie_holds_less_and_the_worst_direction_is_read() {
        let (length, radius) = (35.0, 0.25);
        let tie = |r: f64| TieMember {
            junction_z_mm: 0.5 * length,
            seat_mm: [1.2, 0.0, 0.5 * length + 0.8],
            radius_mm: r,
        };
        let stiff = tied_post_sway([0.0; 3], [0.0, 0.0, length], radius, &[tie(0.6)], S).unwrap();
        let thin = tied_post_sway([0.0; 3], [0.0, 0.0, length], radius, &[tie(0.05)], S).unwrap();
        let untied = tied_post_sway([0.0; 3], [0.0, 0.0, length], radius, &[], S).unwrap();
        assert!(thin > stiff && thin < untied, "stiff {stiff} thin {thin} untied {untied}");
    }
}
