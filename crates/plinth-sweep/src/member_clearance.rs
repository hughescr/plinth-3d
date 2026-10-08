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
//!    that span's wider end (the member-against-member test still reads it so:
//!    an error toward refusal; the member-against-model census reads the drawn
//!    frustum since 2026-10-04) (round-3 review, 2026-09-29: the closest pair of
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
    model_intrusion_tapered(
        grid,
        history,
        layer_height_mm,
        from,
        to,
        (radius_mm, radius_mm),
        gap_mm,
    )
}

/// [`model_intrusion`] for the frustum the file draws: the radius runs
/// linearly from `radii.0` at `from` to `radii.1` at `to`, and each cured cell
/// is read at the frustum's true clearance to it, the least over the axis of
/// the distance minus the radius there ([`frustum_clearance_to_column`];
/// fix/drawn-solid-clearance, 2026-10-04: the thickening rung's drawn root is
/// one chord from a step to the root, and the wider end read all along its
/// length refused a raise that the census, which reads the frustum, finds
/// clear; the radius at the nearest axis point alone accepted a thick end that
/// stands inside the gap, challenger round 2).
pub(crate) fn model_intrusion_tapered(
    grid: &Grid,
    history: &ColumnHistory,
    layer_height_mm: f64,
    from: [f64; 3],
    to: [f64; 3],
    radii: (f64, f64),
    gap_mm: f64,
) -> Option<(usize, f64)> {
    let radius_mm = radii.0.max(radii.1);
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
                if distance >= reach {
                    continue;
                }
                if (radii.0 - radii.1).abs() <= f64::EPSILON {
                    return Some((cell, distance));
                }
                // The frustum's own clearance to this cell: the least over the
                // axis of `distance − radius there` (the radius at the nearest
                // axis point alone is not it; the thick end can be nearer the
                // cell's solid than the thin end the axes meet at).
                let (clearance, at_distance) = frustum_clearance_to_column(from, to, radii, x, y, bottom, top);
                if clearance < gap_mm.max(0.0) + raster_half_diagonal_mm(grid) {
                    return Some((cell, at_distance));
                }
            }
        }
    }
    None
}

/// The clearance of the frustum about the axis `from`–`to` (radius linear from
/// `radii.0` at `from` to `radii.1` at `to`) to the vertical segment over
/// `(x, y)` from `z_low` to `z_high`: the minimum over the axis of the point's
/// distance to that segment minus the radius there, and the centreline
/// distance at the minimiser.
///
/// The distance from a point moving along a line to a convex set is convex in
/// the parameter, and the radius is linear in it, so `distance − radius` is
/// convex and a ternary search finds its minimum (the least value seen is
/// returned, which is an achievable clearance, never one below the true
/// minimum). 40 rounds narrow the bracket to `(2/3)^40`, about 1e-7 of the
/// axis.
fn frustum_clearance_to_column(
    from: [f64; 3],
    to: [f64; 3],
    radii: (f64, f64),
    x: f64,
    y: f64,
    z_low: f64,
    z_high: f64,
) -> (f64, f64) {
    let at = |s: f64| -> (f64, f64) {
        let point: [f64; 3] = std::array::from_fn(|axis| from[axis] + s * (to[axis] - from[axis]));
        let flat = (point[0] - x).hypot(point[1] - y);
        let vertical = point[2] - point[2].clamp(z_low, z_high);
        let distance = flat.hypot(vertical);
        (distance - (radii.0 + s * (radii.1 - radii.0)).max(0.0), distance)
    };
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    let mut best = at(0.0);
    let end = at(1.0);
    if end.0 < best.0 {
        best = end;
    }
    for _ in 0..40 {
        let (a, b) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
        let (fa, fb) = (at(a), at(b));
        for probe in [fa, fb] {
            if probe.0 < best.0 {
                best = probe;
            }
        }
        if fa.0 < fb.0 {
            hi = b;
        } else {
            lo = a;
        }
    }
    best
}

/// **A straight shaft's drawn solid against the model, outside its own seat
/// zone and (on the model) its own landing zone**, on the raster
/// (fix/drawn-solid-clearance, 2026-10-04; `clearance-census-diagnosis.md`
/// R1): the shaft from its base cell at layer `base.1` to its seat cell at
/// layer `seat.1`, a solid of radius `radius_mm`, against the cured material
/// the column history knows.
///
/// The zones are the raster's own form of the census's
/// ([`model_clearance_census`]): the seat end is exempt for
/// `zone_factor · (radius + gap)` along the axis, and so is the base end
/// where the shaft lands on the MODEL (a plate or a member landing has no
/// model face to strike). A shaft rises off the face it stands against at
/// `n·m ≥ 1 / zone_factor` at the steepest tilt the overhang law admits
/// ([`crate::lean::LeanLaw::face_zone_factor`]), and it first stands `gap`
/// clear of that face's plane at `(gap + radius) / (n·m)` along its axis
/// ([`landing_zone_mm`]), so the exemption is never shorter than the real
/// zone. A shaft that lies wholly inside its zones is not judged.
///
/// Returns the first offending cell and its distance, `None` when it clears.
#[expect(
    clippy::too_many_arguments,
    reason = "the shaft's two ends, its kind, its radius and gap, and the raster it is read on"
)]
pub(crate) fn shaft_model_intrusion(
    grid: &Grid,
    history: &ColumnHistory,
    layer_height_mm: f64,
    seat: (usize, usize),
    base: (usize, usize),
    lands_on_model: bool,
    radius_mm: f64,
    gap_mm: f64,
    zone_factor: f64,
) -> Option<(usize, f64)> {
    let top = axis_point(grid, layer_height_mm, seat.0, seat.1);
    let bottom = axis_point(grid, layer_height_mm, base.0, base.1);
    let zone = zone_factor.max(1.0) * (radius_mm.max(0.0) + gap_mm.max(0.0));
    inset_segment_intrusion(
        grid,
        history,
        layer_height_mm,
        top,
        bottom,
        zone,
        if lands_on_model { zone } else { 0.0 },
        (radius_mm, radius_mm),
        gap_mm,
    )
}

/// What a placement-time shaft test reads, bundled: the raster the sweep has
/// built so far, its frame, and the model's surface where it could be built.
/// The surface turns the raster's flag into the census's own verdict and
/// gives each contact zone the normal of the face it is on
/// ([`ShaftRead::intrusion`]).
#[derive(Clone, Copy)]
pub(crate) struct ShaftRead<'a> {
    pub grid: &'a Grid,
    pub history: &'a ColumnHistory,
    pub layer_height_mm: f64,
    /// The plate's z in the mesh's frame (the raster's frame has the plate at 0).
    pub plate_z_mm: f64,
    pub surface: Option<&'a geom_kernel::NativeSurface>,
    /// [`crate::lean::LeanLaw::face_zone_factor`]: the longest zone, used where
    /// no surface is there to name the face.
    pub zone_factor: f64,
}

impl ShaftRead<'_> {
    fn kernel_point(&self, point: [f64; 3]) -> [f64; 3] {
        [point[0], point[1], point[2] + self.plate_z_mm]
    }

    /// The face the shaft's seat at `seat` stands against: the model's nearest
    /// face to the seat's axis point and its outward normal. One nearest-point
    /// query; `None` without a surface or where the query names no face.
    pub(crate) fn seat_face(&self, seat: (usize, usize)) -> Option<ContactFace> {
        let surface = self.surface?;
        let point = self.kernel_point(axis_point(self.grid, self.layer_height_mm, seat.0, seat.1));
        landing_faces(surface, &[point], &[true])
            .into_iter()
            .next()
            .flatten()
            .map(|(point, normal)| ContactFace { point, normal })
    }

    /// **A straight shaft's drawn solid against the model, read as the census
    /// reads it** (challenger round 3, 2026-10-04): the shaft from its base cell
    /// at layer `base.1` to its seat cell at layer `seat.1`, radius
    /// `radius_mm`, outside its OWN contact zones and no others. Returns the
    /// first offending cell and its distance, `None` when it clears.
    ///
    /// * **The zones are the real ones.** The seat zone is where the capsule
    ///   first stands `gap` clear of the seat face's plane,
    ///   `(gap + r − h) / (n·m)` along the axis, `n` that face's outward normal
    ///   (`seat_face`, one query per seat), `m` the axis from the seat to the
    ///   base and `h` the seat's height over the plane; a model landing's zone
    ///   is the same at its landing face
    ///   ([`landing_zone_mm`]). Neither is longer than the longest the lean law
    ///   admits (`n·m` is never taken below `1 / zone_factor`, which can only
    ///   test MORE). The old read took the longest zone for every shaft, so a
    ///   0.6 mm branch under a flat seat (zone 0.35 mm) was exempt whole.
    /// * **The raster only flags.** Its reach carries half a cell's diagonal,
    ///   so a cell it flags may still be clear. A flagged shaft is confirmed
    ///   by the kernel's clearance predicate ([`geom_kernel::SegmentClearanceClassified`]),
    ///   the one the finished-field census runs, on the same axis and zones; it
    ///   is refused only when that predicate reads it under the gap (or reads
    ///   only a bound that does not clear it). Where the surface names no face
    ///   for an end, that end's zone is zero ([`contact_zone_mm`]): its
    ///   contact reads as inside the gap and the shaft is refused, never
    ///   excused. Without a surface at all the raster's flag stands, with the
    ///   longest zones: placement cannot refuse every seated shaft, and the
    ///   finished-field census, which cannot run either, says so in a
    ///   WARNING.
    pub(crate) fn intrusion(
        &self,
        seat: (usize, usize),
        seat_face: Option<ContactFace>,
        base: (usize, usize),
        lands_on_model: bool,
        radius_mm: f64,
        gap_mm: f64,
    ) -> Option<(usize, f64)> {
        let (grid, history, height) = (self.grid, self.history, self.layer_height_mm);
        let Some(surface) = self.surface else {
            return shaft_model_intrusion(
                grid,
                history,
                height,
                seat,
                base,
                lands_on_model,
                radius_mm,
                gap_mm,
                self.zone_factor,
            );
        };
        let top = axis_point(grid, height, seat.0, seat.1);
        let bottom = axis_point(grid, height, base.0, base.1);
        let length = geom_kernel::vec3::dist(top, bottom);
        if !(length.is_finite() && length > 0.0) {
            return None;
        }
        let radius = radius_mm.max(0.0);
        let gap = gap_mm.max(0.0);
        let down = geom_kernel::vec3::scale(geom_kernel::vec3::sub(bottom, top), 1.0 / length);
        let seat_zone = contact_zone_mm(
            seat_face,
            self.kernel_point(top),
            down,
            radius,
            gap,
            self.zone_factor,
        );
        let landing_zone = if lands_on_model {
            let base_point = self.kernel_point(bottom);
            let face = contact_faces(surface, &[base_point]).into_iter().next().flatten();
            contact_zone_mm(
                face,
                base_point,
                geom_kernel::vec3::scale(down, -1.0),
                radius,
                gap,
                self.zone_factor,
            )
        } else {
            0.0
        };
        let flagged = inset_segment_intrusion(
            grid,
            history,
            height,
            top,
            bottom,
            seat_zone,
            landing_zone,
            (radius, radius),
            gap,
        )?;
        let verdict = surface
            .segment_clearance_classified(
                &[self.kernel_point(top)],
                &[self.kernel_point(bottom)],
                &[radius],
                &[radius],
                0.0,
                &[seat_zone],
                &[landing_zone],
            )
            .ok()
            .and_then(|rows| rows.into_iter().next());
        match verdict {
            Some(geom_kernel::SegmentClearanceClassified::Measured { clearance_mm, .. })
                if clearance_mm >= gap - 1.0e-9 =>
            {
                None
            }
            _ => Some(flagged),
        }
    }
}

/// The face a shaft's contact is on: the model's nearest surface point and
/// the face's outward unit normal, in the mesh's frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ContactFace {
    pub point: [f64; 3],
    pub normal: [f64; 3],
}

/// The face each point (in the mesh's frame) stands against: the model's
/// nearest surface point and that face's outward unit normal, `None` where
/// the query names no face. One nearest-point batch for all of them.
pub(crate) fn contact_faces(
    surface: &geom_kernel::NativeSurface,
    points: &[[f64; 3]],
) -> Vec<Option<ContactFace>> {
    let all = vec![true; points.len()];
    landing_faces(surface, points, &all)
        .into_iter()
        .map(|face| face.map(|(point, normal)| ContactFace { point, normal }))
        .collect()
}

/// **A shaft's own CONTACT ZONE at one end, from the face it stands against**
/// (challenger round 3, 2026-10-04; the one form every placement-time read
/// uses: the up-sweep's descent test and the thickening rung's refusal): the
/// length along the axis, from the contact end `end` (mesh frame) into the
/// shaft along the unit `into`, over which a solid of radius `radius` still
/// stands within `gap` of the face's plane, `(gap + r − h) / (n·m)`, `n` the
/// face's outward normal, `m = into`, `h` the end's height over the plane.
///
/// `n·m` is never taken below `1 / zone_factor`, the steepest the lean law
/// admits, and the zone is never longer than `zone_factor · (r + gap)`, the
/// longest. A flat seat's zone is `gap + r`, not the longest.
///
/// **Without a face the zone is ZERO** (challenger round 5, 2026-10-05): no
/// surface, or a query that named no face, says nothing about how long the
/// zone is, and a zone is an exemption, so an unknown one exempts nothing.
/// The retired read gave the longest zone there, which on a short member
/// exempted the whole solid and so suppressed every read of it before the
/// reader's own "nothing can be proved without a surface" refusal could run.
/// A reader that gets zero here reads the contact itself as inside the gap;
/// it must never take that as proof of an existing violation.
pub(crate) fn contact_zone_mm(
    face: Option<ContactFace>,
    end: [f64; 3],
    into: [f64; 3],
    radius: f64,
    gap: f64,
    zone_factor: f64,
) -> f64 {
    let longest = zone_factor.max(1.0) * (radius + gap);
    let Some(face) = face else {
        return 0.0;
    };
    // The least `n·m` the lean law admits: a face steeper than that is read
    // as if it were exactly that steep (a longer zone is never given).
    let steepest = 1.0 / zone_factor.max(1.0);
    let along = geom_kernel::vec3::dot(face.normal, into).max(steepest);
    let over = geom_kernel::vec3::dot(face.normal, geom_kernel::vec3::sub(end, face.point));
    ((gap + radius - over) / along).clamp(0.0, longest)
}

/// [`model_intrusion`] on the axis `top`–`bottom` with `inset_top` mm of its
/// top end and `inset_bottom` mm of its bottom end left out (a seat zone, a
/// landing zone). `None` where the two insets use the whole axis.
#[expect(
    clippy::too_many_arguments,
    reason = "the segment's two ends and insets, its radius and gap, and the raster it is read on"
)]
pub(crate) fn inset_segment_intrusion(
    grid: &Grid,
    history: &ColumnHistory,
    layer_height_mm: f64,
    top: [f64; 3],
    bottom: [f64; 3],
    inset_top_mm: f64,
    inset_bottom_mm: f64,
    radii_top_bottom: (f64, f64),
    gap_mm: f64,
) -> Option<(usize, f64)> {
    let length = geom_kernel::vec3::dist(top, bottom);
    if !(length.is_finite() && length > 0.0) {
        return None;
    }
    if inset_top_mm + inset_bottom_mm >= length {
        return None;
    }
    let along = |from: [f64; 3], towards: [f64; 3], mm: f64| -> [f64; 3] {
        std::array::from_fn(|axis| from[axis] + (towards[axis] - from[axis]) * (mm / length))
    };
    let from = along(bottom, top, inset_bottom_mm);
    let to = along(top, bottom, inset_top_mm);
    // The radius at each end of what is left, on the linear taper.
    let (r_top, r_bottom) = radii_top_bottom;
    let radius_at = |from_top_mm: f64| r_top + (r_bottom - r_top) * (from_top_mm / length);
    model_intrusion_tapered(
        grid,
        history,
        layer_height_mm,
        from,
        to,
        (radius_at(length - inset_bottom_mm), radius_at(inset_top_mm)),
        gap_mm,
    )
}

/// **A member's own LANDING zone on the model** (fix/morel-long-posts,
/// 2026-10-02): for each member that lands on the model, how far up its axis
/// from its base the solid still stands within `tier` of the face it lands
/// on, in mm; `0` for a member that does not land on the model.
///
/// A landing strikes the face: its base stands one lamina into the cured
/// layer it lands on (`docs/physics.md`:284-324), so a clearance measured
/// from base to seat reads that strike as an intrusion of about a lamina on
/// EVERY model landing. That is the member's contact, exactly as the seat
/// zone is at the other end ([`crate::head_join`]'s `own_seat_zone_mm`), and
/// it is exempted the same way: the face the base lands on is the plane
/// through its nearest surface point with that face's outward normal `n`;
/// with `m` the unit axis from base to seat and `s` the base's signed height
/// over that plane, the capsule of radius `r` first stands `tier` clear of
/// the plane at `(tier + r − s) / (n·m)` along the axis. Measured on the
/// Morel base (for-craig-v10): a model re-landing under the mushroom caps
/// read −0.05 to −0.09 mm before the exemption, i.e. its own strike.
///
/// `f64::INFINITY` where the axis does not rise off the face (`n·m ≤ 0`) or
/// the face is not measured: nothing of such a member can be judged, and the
/// caller reads an unmeasured row as the strict answer.
///
/// Batch-first: one nearest-point query for all members.
pub(crate) fn model_landing_zones_mm(
    surface: &geom_kernel::NativeSurface,
    seats: &[[f64; 3]],
    bases: &[[f64; 3]],
    radii: &[f64],
    on_model: &[bool],
    tier: f64,
) -> Vec<f64> {
    let faces = landing_faces(surface, bases, on_model);
    (0..bases.len())
        .map(|i| match faces.get(i).copied().flatten() {
            None if on_model.get(i).copied().unwrap_or(false) => f64::INFINITY,
            None => 0.0,
            Some(face) => landing_zone_mm(face, bases[i], seats[i], radii[i], tier),
        })
        .collect()
}

/// The face each model-landed base strikes: its nearest surface point and
/// that face's outward unit normal; `None` for a member not on the model, or
/// a face the query could not name. One nearest-point batch.
fn landing_faces(
    surface: &geom_kernel::NativeSurface,
    bases: &[[f64; 3]],
    on_model: &[bool],
) -> Vec<Option<([f64; 3], [f64; 3])>> {
    let n = bases.len();
    let wanted: Vec<usize> = (0..n)
        .filter(|&i| on_model.get(i).copied().unwrap_or(false))
        .collect();
    let mut out = vec![None; n];
    if wanted.is_empty() {
        return out;
    }
    let points: Vec<[f64; 3]> = wanted.iter().map(|&i| bases[i]).collect();
    let (_, closest, faces, normals) = surface.project(&points);
    for (slot, &i) in wanted.iter().enumerate() {
        if faces[slot] >= 0 {
            out[i] = geom_kernel::vec3::normalize(normals[slot]).map(|normal| (closest[slot], normal));
        }
    }
    out
}

/// [`model_landing_zones_mm`] for one member of radius `radius` on a landing
/// face `(point, outward normal)`: `(tier + r − s) / (n·m)`, `m` the unit
/// axis from `base` to `top`, `s` the base's signed height over the face
/// plane; `f64::INFINITY` where the axis does not rise off the face.
fn landing_zone_mm(
    face: ([f64; 3], [f64; 3]),
    base: [f64; 3],
    top: [f64; 3],
    radius: f64,
    tier: f64,
) -> f64 {
    let (point, normal) = face;
    let Some(axis) = geom_kernel::vec3::normalize(geom_kernel::vec3::sub(top, base)) else {
        return f64::INFINITY;
    };
    let along = geom_kernel::vec3::dot(normal, axis);
    if along <= 0.0 {
        return f64::INFINITY;
    }
    let height = geom_kernel::vec3::dot(normal, geom_kernel::vec3::sub(base, point));
    ((tier + radius - height) / along).max(0.0)
}

/// **Every member's drawn solid against the model, on the finished field**
/// (fix/morel-long-posts, 2026-10-02): the production census of the rule
/// "a support member's drawn solid clears the model by the final gap outside
/// its own seat zone and, on the model, its own landing zone" — arm shafts
/// included, which no rule measured before (the run disclosure said so: "no
/// radius-aware clearance of an ARM's own shaft against the model").
///
/// Each member is measured as the file draws it: the straight shaft from its
/// ball centre (its seat, for a member without a ball) to its landing, or to
/// its boss top for a plate member ([`plinth_support::SweepRootFlare::plate_shaft_of`]),
/// cut into the spans the lowering draws it as
/// ([`plinth_support::drawn_spans`]), each read as the frustum the file draws
/// (radius linear from its top end radius to its bottom end radius, taken at
/// each face's nearest axis point, not the wider end all along). Its own
/// seat zone is the part of the axis inside the tip ball or within `tier` of
/// the seat plane, `max(R, (tier + r − d) / (n·m))` from the ball centre
/// (`d` the centre's height over the plane on the support side); its landing
/// zone is [`model_landing_zones_mm`]'s. The root flare, the boss and the raft
/// are not shafts and are not measured here.
///
/// A WARNING quantity, never a gate: emit and warn. Batch-first: one contact
/// disc batch, one nearest-point batch and one clearance batch for the field.
pub(crate) fn model_clearance_census(
    surface: Option<&geom_kernel::NativeSurface>,
    struts: &[crate::SweepStrut],
    arms: &[plinth_support::SweepArm],
    layer_plan: &plinth_core::LayerPlan,
    plate_z_mm: f64,
    tier: f64,
    zone_factor: f64,
    cavity: &crate::cavity::HiddenCavity,
) -> crate::report::MemberModelClearance {
    let mut census = crate::report::MemberModelClearance {
        gap_mm: tier,
        ..crate::report::MemberModelClearance::default()
    };
    let Some(surface) = surface else {
        return census;
    };
    census.ran = true;
    let height = layer_plan.heights_mm().first().copied().unwrap_or(0.0);
    let flare = plinth_support::SweepRootFlare::registered(height).ok();
    let discs = plinth_support::tip_contact_discs(arms, layer_plan)
        .unwrap_or_else(|_| vec![None; arms.len()]);
    let on_model: Vec<bool> = struts
        .iter()
        .map(|strut| matches!(strut.landing, crate::Landing::Model { .. }))
        .collect();
    let bases: Vec<[f64; 3]> = arms.iter().map(|arm| arm.base_mm).collect();
    let faces = landing_faces(surface, &bases, &on_model);

    // One row per drawn span: (member, p0, p1, radius at p0, radius at p1, inset
    // at p0, inset at p1). The two radii are the span's two DRAWN end radii
    // (the file draws a span as one frustum): the kernel reads the radius at
    // the nearest point of each face, not the wider end for the whole span.
    let mut owner: Vec<usize> = Vec::new();
    let (mut p0s, mut p1s, mut top_radii, mut bottom_radii, mut inset0s, mut inset1s) = (
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let mut measured_member = vec![false; arms.len()];
    let mut unmeasured_member = vec![false; arms.len()];
    for (index, (strut, arm)) in struts.iter().zip(arms).enumerate() {
        if strut.k_bottom == usize::MAX || strut.k_top <= strut.k_bottom {
            continue;
        }
        // The final gap does not apply inside a hidden cavity (Craig ruling
        // 2026-10-05, on his 2026-08-31 ruling that supports hidden in a
        // cavity are never removed or cleaned): a member whose whole axis
        // stands in enclosed air ([`crate::cavity`], whatever it lands on)
        // is not measured, so it is neither counted nor re-routed. Counted
        // apart, so the census says what it left out. Asked of the member's
        // DRAWN SOLID (the widest of its profile), not its centreline: a
        // thick post whose solid reaches a voxel the cavity map does not
        // call hidden is measured.
        if cavity.member_hidden(strut, crate::cavity::arm_radius_mm(arm)) {
            census.members_hidden += 1;
            continue;
        }
        let disc = discs.get(index).copied().flatten();
        // The POST starts at the ball centre, or at the joint `K` of a headed
        // arm: the neck and the joint sphere are the contact, and the post's
        // own zone there is zero by construction.
        let top = match disc {
            Some(disc) if arm.seats_into_model => disc.post_top_mm,
            _ => arm.seat_mm,
        };
        let post_start_radius = disc
            .and_then(|disc| disc.head_joint_radius_mm)
            .unwrap_or(arm.tip_radius_mm);
        let bottom = if strut.landing == crate::Landing::Plate {
            match flare.as_ref().and_then(|flare| flare.plate_shaft_of(arm, plate_z_mm)) {
                Some((_, bottom)) => bottom,
                None => {
                    unmeasured_member[index] = true;
                    continue;
                }
            }
        } else {
            arm.base_mm
        };
        let length = geom_kernel::vec3::dist(top, bottom);
        let rise = top[2] - bottom[2];
        if !(length.is_finite() && length > 0.0 && rise > 0.0) {
            unmeasured_member[index] = true;
            continue;
        }
        let axis = geom_kernel::vec3::scale(geom_kernel::vec3::sub(bottom, top), 1.0 / length);
        let root = plinth_support::drawn_root_radius_mm(&arm.radius_profile_mm, arm.tip_radius_mm);
        let spans = plinth_support::drawn_spans(
            &arm.radius_profile_mm,
            arm.base_mm[2],
            arm.seat_mm[2],
            top[2],
            bottom[2],
            post_start_radius,
            root,
        );
        let at = |z: f64| -> ([f64; 3], f64) {
            let t = ((top[2] - z) / rise).clamp(0.0, 1.0);
            (
                std::array::from_fn(|axis_index| top[axis_index] + t * (bottom[axis_index] - top[axis_index])),
                t * length,
            )
        };
        for span in spans {
            let radius = span.top_radius_mm.max(span.bottom_radius_mm).max(0.0);
            let (from, a0) = at(span.top_z_mm);
            let (to, a1) = at(span.bottom_z_mm);
            if a1 <= a0 {
                continue;
            }
            let seat_zone = match disc {
                // A headed arm's post stands outside the contact zone.
                Some(disc) if arm.seats_into_model && disc.head_joint_radius_mm.is_some() => 0.0,
                // **The exempt zone is sized at the CONTACT, never at the span**
                // (v19 fix, 2026-10-08, ruling 2d). The zone is the stretch
                // over which the arm's own tip-sized shaft still stands within
                // the gap of the seat plane, so it is read at the contact
                // ball's radius and capped by the lean law's factor, as every
                // other contact zone is ([`contact_zone_mm`]). It used each
                // span's OWN radius, so a fat span, steeply off the normal,
                // exempted itself (v19 Cylinder: record 460, r 0.965, was
                // exempt out to 3.0 mm and its 0.144 mm intrusion was never
                // read). Any span wider than the tip, outside the ball, is
                // always measured now.
                Some(disc) if arm.seats_into_model => disc.ball_radius_mm.max(contact_zone_mm(
                    Some(ContactFace {
                        point: disc.centre_mm,
                        normal: disc.outward_normal,
                    }),
                    disc.ball_centre_mm,
                    axis,
                    disc.ball_radius_mm,
                    tier,
                    zone_factor,
                )),
                _ => 0.0,
            };
            let landing_zone = match faces.get(index).copied().flatten() {
                Some(face) => landing_zone_mm(face, bottom, top, radius, tier),
                None if on_model[index] => f64::INFINITY,
                None => 0.0,
            };
            let inset0 = (seat_zone - a0).max(0.0);
            let inset1 = (landing_zone - (length - a1)).max(0.0);
            if inset0 + inset1 >= a1 - a0 {
                census.rows_in_own_zones += 1;
                continue;
            }
            owner.push(index);
            p0s.push(from);
            p1s.push(to);
            top_radii.push(span.top_radius_mm.max(0.0));
            bottom_radii.push(span.bottom_radius_mm.max(0.0));
            if (span.top_radius_mm - span.bottom_radius_mm).abs() > geom_kernel::TAPER_EPS_MM {
                census.rows_tapered += 1;
            }
            inset0s.push(inset0);
            inset1s.push(inset1);
        }
    }
    census.rows = owner.len();
    let rows = surface
        .segment_frustum_clearance_classified(
            &p0s,
            &p1s,
            &top_radii,
            &bottom_radii,
            0.0,
            &inset0s,
            &inset1s,
        )
        .unwrap_or_default();
    let mut worst: Vec<Option<(f64, bool, [f64; 3])>> = vec![None; arms.len()];
    for (row, &index) in rows.iter().zip(&owner) {
        let reading = match *row {
            geom_kernel::SegmentClearanceClassified::Measured { clearance_mm, witness } => {
                Some((clearance_mm, false, witness))
            }
            // A one-sided bound already under the gap is a finding; one over
            // it decides nothing.
            geom_kernel::SegmentClearanceClassified::UpperBound {
                clearance_mm,
                witness,
                ..
            } if clearance_mm < tier => Some((clearance_mm, true, witness)),
            _ => None,
        };
        match reading {
            Some(reading) => {
                measured_member[index] = true;
                if worst[index].is_none_or(|(now, _, _)| reading.0 < now) {
                    worst[index] = Some(reading);
                }
            }
            None => unmeasured_member[index] = true,
        }
    }
    census.members_measured = measured_member.iter().filter(|m| **m).count();
    census.members_unmeasured = unmeasured_member.iter().filter(|m| **m).count();
    for (index, reading) in worst.iter().enumerate() {
        let Some((clearance_mm, upper_bound, witness)) = *reading else {
            continue;
        };
        if census
            .smallest_clearance_mm
            .is_none_or(|smallest| clearance_mm < smallest)
        {
            census.smallest_clearance_mm = Some(clearance_mm);
        }
        if clearance_mm < tier - 1.0e-9 {
            census.within_gap.push(crate::report::MemberNearModel {
                member: struts[index].id,
                kind: struts[index].kind,
                landing: struts[index].landing,
                clearance_mm,
                upper_bound,
                witness_mm: witness,
            });
        }
    }
    census
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

/// Axis-aligned boxes `(low corner, high corner)` as one model surface, for
/// the tests that read a placement against the census's own predicate.
#[cfg(test)]
pub(crate) fn boxes_surface(boxes: &[([f64; 3], [f64; 3])]) -> geom_kernel::NativeSurface {
    let (verts, tris) = boxes_mesh(boxes);
    indexed_surface(&verts, &tris)
}

/// The vertices and triangles of [`boxes_surface`], so a test can move a
/// vertex (a face tilted by one unit in the last place, say) before the
/// surface is built: corner `c` of box `b` is vertex `8 b + c`, bit `k` of
/// `c` set meaning the high coordinate on axis `k`.
#[cfg(test)]
pub(crate) fn boxes_mesh(boxes: &[([f64; 3], [f64; 3])]) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for &(low, high) in boxes {
        let base = u32::try_from(verts.len()).expect("a few vertices");
        for corner in 0..8_usize {
            verts.push(std::array::from_fn(|k| if corner >> k & 1 == 0 { low[k] } else { high[k] }));
        }
        for t in [
            [0, 2, 1], [1, 2, 3], [4, 5, 6], [5, 7, 6], [0, 1, 4], [1, 5, 4],
            [2, 6, 3], [3, 6, 7], [0, 4, 2], [2, 4, 6], [1, 3, 5], [3, 7, 5],
        ] {
            tris.push([base + t[0], base + t[1], base + t[2]]);
        }
    }
    (verts, tris)
}

/// One model surface from indexed triangles, at the tests' grid pitch.
#[cfg(test)]
pub(crate) fn indexed_surface(verts: &[[f64; 3]], tris: &[[u32; 3]]) -> geom_kernel::NativeSurface {
    let pitch = 0.1;
    let estimate = geom_kernel::NativeSurface::estimate_index_memory_from_indexed(verts, tris, pitch)
        .expect("an estimate");
    geom_kernel::NativeSurface::from_indexed(
        verts,
        tris,
        geom_kernel::NativeSurfaceConfig {
            grid_pitch_mm: pitch,
            max_grid_bytes: estimate.max_axis_grid_bytes(),
        },
    )
    .expect("a surface")
}

/// **The census of every member's drawn solid, and the landing zone it
/// grants a member on the model** (fix/morel-long-posts, 2026-10-02), on a
/// box whose top face is `z = 2` and whose `+X` wall is `x = 5`.
#[cfg(test)]
mod census_tests {
    use super::{model_clearance_census, model_landing_zones_mm};
    use crate::{Landing, SeatFace, StrutKind, SweepStrut};
    use geom_kernel::{NativeSurface, NativeSurfaceConfig};
    use plinth_support::{SweepArm, SweepArmLanding};

    const GAP: f64 = 0.3;
    const H: f64 = 0.05;
    const PLATE: f64 = -5.0;
    /// The lean law's zone factor (`hypot(max step, h) / h`) at the registered
    /// 45 degree law over 25 degrees of transition.
    const ZONE: f64 = 1.84;

    fn boxed(low: [f64; 3], high: [f64; 3]) -> NativeSurface {
        let verts: Vec<[f64; 3]> = (0..8)
            .map(|corner| std::array::from_fn(|k| if corner >> k & 1 == 0 { low[k] } else { high[k] }))
            .collect();
        let tris: Vec<[u32; 3]> = vec![
            [0, 2, 1],
            [1, 2, 3],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 4],
            [1, 5, 4],
            [2, 6, 3],
            [3, 6, 7],
            [0, 4, 2],
            [2, 4, 6],
            [1, 3, 5],
            [3, 7, 5],
        ];
        let pitch = 0.1;
        let estimate = NativeSurface::estimate_index_memory_from_indexed(&verts, &tris, pitch)
            .expect("an estimate");
        NativeSurface::from_indexed(
            &verts,
            &tris,
            NativeSurfaceConfig {
                grid_pitch_mm: pitch,
                max_grid_bytes: estimate.max_axis_grid_bytes(),
            },
        )
        .expect("a surface")
    }

    fn block() -> NativeSurface {
        boxed([-5.0, -5.0, 0.0], [5.0, 5.0, 2.0])
    }

    /// A vertical member over `(x, y)` from `top_z` down to `base_z`, of
    /// constant radius `r`, as both the plan and the lowering hold it.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "heights over the plate, in whole layers, in a small test frame"
    )]
    fn member(
        id: u32,
        kind: StrutKind,
        landing: Landing,
        xy: [f64; 2],
        base_z: f64,
        top_z: f64,
        r: f64,
    ) -> (SweepStrut, SweepArm) {
        let k_top = ((top_z - PLATE) / H).round() as usize;
        let k_bottom = ((base_z - PLATE) / H).round() as usize;
        let strut = SweepStrut {
            id,
            cell: 0,
            center_mm: xy,
            kind,
            min_radius_mm: r,
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top,
            k_bottom,
            landing,
            interior: false,
            radius_mm: vec![r; k_top - k_bottom + 1],
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(),
            inserted_by_load: false,
            path: Vec::new(),
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
            seat_face: SeatFace::default(),
            seat_normal: [0.0; 3],
        };
        let arm = SweepArm {
            seat_mm: [xy[0], xy[1], top_z],
            base_mm: [xy[0], xy[1], base_z],
            tip_radius_mm: r,
            radius_profile_mm: vec![r; k_top - k_bottom + 1],
            seat_normal_mm: [0.0; 3],
            seat_face_point_mm: None,
            landing: match landing {
                Landing::Plate => SweepArmLanding::Plate,
                Landing::Model { .. } | Landing::Strut { .. } => SweepArmLanding::Landed,
            },
            seats_into_model: false,
            head: None,
        };
        (strut, arm)
    }

    /// A vertical plate post over `(0, 0)` from `top_z` to the plate, drawn as
    /// ONE tapered span: tip radius `tip`, root radius `root`. The ledger sits at
    /// the tip radius everywhere but its first slot, which is the root
    /// (`drawn_root_radius_mm` reads the first entry) and lies under the plate
    /// boss, so no slot boundary inside the shaft asks for more than the chord
    /// has and the drawing is the single chord the thickening produces.
    fn tapered_post(top_z: f64, tip: f64, root: f64) -> (SweepStrut, SweepArm) {
        let (strut, mut arm) = member(0, StrutKind::Arm, Landing::Plate, [0.0, 0.0], PLATE, top_z, tip);
        let mut profile = vec![tip; arm.radius_profile_mm.len()];
        profile[0] = root;
        arm.radius_profile_mm = profile.clone();
        let mut strut = strut;
        strut.radius_mm = profile;
        (strut, arm)
    }

    fn census_of(
        surface: &NativeSurface,
        post: (SweepStrut, SweepArm),
    ) -> (crate::report::MemberModelClearance, Vec<plinth_support::DrawnSpan>) {
        let (struts, arms) = (vec![post.0], vec![post.1]);
        let layers = plinth_core::LayerPlan::new(PLATE, vec![H; 400]).expect("a uniform plan");
        let none = crate::cavity::HiddenCavity::none();
        let census = model_clearance_census(Some(surface), &struts, &arms, &layers, PLATE, GAP, ZONE, &none);
        let root = plinth_support::drawn_root_radius_mm(&arms[0].radius_profile_mm, arms[0].tip_radius_mm);
        let spans = plinth_support::drawn_spans(
            &arms[0].radius_profile_mm,
            arms[0].base_mm[2],
            arms[0].seat_mm[2],
            arms[0].seat_mm[2],
            PLATE + 1.0,
            arms[0].tip_radius_mm,
            root,
        );
        (census, spans)
    }

    /// **A MEMBER IN A HIDDEN CAVITY IS NOT COUNTED** (Craig ruling
    /// 2026-10-05: the 0.3 mm final gap does not apply inside a hidden cavity,
    /// on his 2026-08-31 ruling that supports there are never removed or
    /// cleaned; challenger 2026-10-05: the authority is the CAVITY, not the
    /// scar flag). Two 0.2 mm posts stand on a support 0.35 mm from a wall
    /// face, 0.15 mm from it: inside the gap. Neither is flagged
    /// [`crate::SweepStrut::interior`] (that flag is false for a member that
    /// lands on a support, which is the point of the control: a branch
    /// standing wholly in a cavity on an interior post).
    ///
    /// - no cavity map: both are named (anti-vacuity);
    /// - the map of an open-sided POCKET (a ceiling over the same floor, so
    ///   model stands over the column, open to the outside on one face): both
    ///   are named. This is the negative control the scar flag failed;
    /// - the map of a SEALED cavity: neither is named, and `members_hidden`
    ///   counts both (the path hit), so the clearance re-route, which sends
    ///   the arms this census names, does not move them.
    #[test]
    fn a_member_in_a_hidden_cavity_is_not_counted() {
        use crate::cavity::fixtures::{cavity_box_at, cell, map_of_at};
        let wall = boxed([5.35, 3.0, -6.0], [7.0, 7.0, 6.0]);
        let layers = plinth_core::LayerPlan::new(PLATE, vec![H; 400]).expect("a uniform plan");
        let sealed = map_of_at(&cavity_box_at(false, false, PLATE), &[], PLATE);
        let pocket = map_of_at(&cavity_box_at(true, false, PLATE), &[], PLATE);
        assert!(sealed.0.census().enclosed_voxels > 0, "{:?}", sealed.0.census());
        assert_eq!(pocket.0.census().enclosed_voxels, 0, "{:?}", pocket.0.census());
        let census_with = |cavity: &crate::cavity::HiddenCavity, grid: &crate::grid::Grid| {
            let on_a_support = Landing::Strut { id: 9, layer: 0 };
            let mut one = member(0, StrutKind::Arm, on_a_support, [5.0, 5.0], PLATE, 5.0, 0.2);
            let mut two = member(1, StrutKind::Arm, on_a_support, [5.0, 5.5], PLATE, 5.0, 0.2);
            one.0.cell = cell(grid, 5.0, 5.0);
            two.0.cell = cell(grid, 5.0, 5.5);
            assert!(!one.0.interior && !two.0.interior, "anti-vacuity: the scar flag is false for both");
            let (struts, arms) = (vec![one.0, two.0], vec![one.1, two.1]);
            model_clearance_census(Some(&wall), &struts, &arms, &layers, PLATE, GAP, ZONE, cavity)
        };
        let named = |census: &crate::report::MemberModelClearance| {
            census.within_gap.iter().map(|near| near.member).collect::<Vec<_>>()
        };
        let open = census_with(&crate::cavity::HiddenCavity::none(), &sealed.1);
        assert_eq!(named(&open), vec![0, 1], "anti-vacuity: both posts are inside the gap: {open:?}");
        assert_eq!(open.members_hidden, 0, "{open:?}");
        let in_a_pocket = census_with(&pocket.0, &pocket.1);
        assert_eq!(named(&in_a_pocket), vec![0, 1], "a pocket open to the outside is no cavity: {in_a_pocket:?}");
        assert_eq!(in_a_pocket.members_hidden, 0, "{in_a_pocket:?}");
        let hidden = census_with(&sealed.0, &sealed.1);
        assert!(named(&hidden).is_empty(), "members in a sealed cavity are not counted: {hidden:?}");
        assert_eq!(hidden.members_hidden, 2, "path hit: both left out as hidden: {hidden:?}");
    }

    /// RED before the fix (rule R3). A plate post drawn as one chord from 0.3 mm
    /// at the tip to 0.9 mm at the root, beside a wall 1.0 mm from the axis that
    /// stands only beside the upper part (drawn radius at most 0.6 mm there):
    /// the drawn solid clears the wall by at least 0.4 mm, and the census read it
    /// at the root radius (1.0 - 0.9 = 0.1 mm) and named it.
    #[test]
    fn a_tapered_span_is_read_at_its_drawn_radius() {
        let wall = boxed([1.0, -1.0, 0.5], [3.0, 1.0, 6.0]);
        let (census, spans) = census_of(&wall, tapered_post(5.0, 0.3, 0.9));
        assert!(
            spans.len() == 1 && spans[0].top_radius_mm < spans[0].bottom_radius_mm,
            "the post must be one tapered chord: {spans:?}"
        );
        assert_eq!(census.members_measured, 1, "{census:?}");
        assert_eq!(census.rows_tapered, 1, "the path-hit witness: the one span is tapered: {census:?}");
        assert!(census.within_gap.is_empty(), "the drawn solid clears by >= 0.4 mm: {census:?}");
    }

    /// The same chord with the wall beside its thick part is named, and at the
    /// radius drawn there, not the root's: the reading is between the root
    /// reading (0.1 mm) and the gap. (Guards the rule from relaxing a wall that
    /// the solid really reaches.)
    #[test]
    fn a_tapered_span_beside_a_wall_at_its_thick_part_is_still_named() {
        let wall = boxed([1.0, -1.0, -3.0], [3.0, 1.0, -1.0]);
        let (census, _) = census_of(&wall, tapered_post(5.0, 0.3, 0.9));
        assert_eq!(census.within_gap.len(), 1, "{census:?}");
        let reading = census.within_gap[0].clearance_mm;
        assert!(reading > 0.12 && reading < GAP, "read at the drawn radius there: {reading}");
    }

    /// A landing STRIKES its face one lamina deep, so the plain clearance
    /// reads every model landing as an intrusion; the landing zone is exactly
    /// the stretch over which the solid is within the gap of that face, so
    /// the rest of the member reads the gap and no less, and a zone a
    /// hundredth of a millimetre short reads under it.
    #[test]
    fn a_landing_reads_its_own_strike_only_inside_its_landing_zone() {
        let surface = block();
        let r = 0.1;
        let base = [0.0, 0.0, 2.0 - H];
        let seat = [0.0, 0.0, 6.0];
        let zone = model_landing_zones_mm(&surface, &[seat], &[base], &[r], &[true], GAP)[0];
        assert!(
            (zone - (GAP + r + H)).abs() < 1e-9,
            "a vertical landing one lamina into a flat face: zone {zone}"
        );
        let read = |inset: f64| {
            surface
                .segment_clearance_classified(&[seat], &[base], &[r], &[r], 0.0, &[0.0], &[inset])
                .expect("one row")[0]
                .clearance_mm()
                .expect("measured")
        };
        assert!(read(0.0) < 0.0, "the strike itself: {}", read(0.0));
        assert!(read(zone) >= GAP - 1e-9, "outside the zone: {}", read(zone));
        assert!(read(zone - 0.01) < GAP, "a zone too short: {}", read(zone - 0.01));
        // A member that does not land on the model has no landing zone.
        let zone_off_the_model =
            model_landing_zones_mm(&surface, &[seat], &[base], &[r], &[false], GAP)[0];
        assert_eq!(
            zone_off_the_model.partial_cmp(&0.0),
            Some(std::cmp::Ordering::Equal),
            "{zone_off_the_model}"
        );
    }

    /// **The seat zone is the CONTACT's, never the span's** (v19 fix,
    /// 2026-10-08, ruling 2d). A straight tip (tip radius 0.2) leans 55 degrees
    /// off the flat underside it seats on (the model's face at `z = 0`) and
    /// its shaft is drawn 0.965 mm thick, as the v19 Cylinder's record 460 was.
    /// That fat span is deep inside the model 0.7 mm from the ball: its own
    /// radius used to stretch the exempt zone to where the solid just clears
    /// the gap (about 2.0 mm), so the intrusion was never read. Sized at the
    /// contact ball it is read and named. The control: the same lean with a
    /// tip-sized shaft is a legitimate contact zone and stays unnamed.
    /// SABOTAGE: put `radius` (the span's own) back in the zone and the first
    /// assertion goes red.
    #[test]
    fn a_fat_off_normal_span_does_not_exempt_itself_from_the_census() {
        let surface = block();
        let layers = plinth_core::LayerPlan::new(PLATE, vec![H; 400]).expect("a uniform plan");
        let nowhere = crate::cavity::HiddenCavity::none();
        // 55 degrees off the normal: inside the lean law (the zone factor's
        // 1 / 1.84 allows up to 57.1).
        let (sin, cos) = (0.819_f64, 0.574_f64);
        let census_with_shaft = |shaft: f64| {
            let (mut strut, mut arm) = member(
                0,
                StrutKind::Arm,
                Landing::Strut { id: 9, layer: 0 },
                [0.0, 0.0],
                -2.565,
                0.0,
                0.2,
            );
            arm.base_mm = [5.0 * sin, 0.0, -5.0 * cos];
            arm.seat_normal_mm = [0.0, 0.0, -1.0];
            arm.seat_face_point_mm = Some([0.0, 0.0, 0.0]);
            arm.seats_into_model = true;
            arm.tip_radius_mm = 0.2;
            arm.radius_profile_mm = vec![shaft; arm.radius_profile_mm.len()];
            strut.radius_mm = arm.radius_profile_mm.clone();
            model_clearance_census(
                Some(&surface),
                &[strut],
                &[arm],
                &layers,
                PLATE,
                GAP,
                ZONE,
                &nowhere,
            )
        };
        let fat = census_with_shaft(0.965);
        assert_eq!(fat.members_measured, 1, "{fat:?}");
        assert_eq!(fat.within_gap.len(), 1, "the fat span's intrusion is read: {fat:?}");
        assert!(
            fat.within_gap[0].clearance_mm < 0.0,
            "it is inside the model: {}",
            fat.within_gap[0].clearance_mm
        );
        let thin = census_with_shaft(0.2);
        assert!(
            thin.within_gap.is_empty(),
            "a tip-sized shaft on the same lean is inside its contact zone only: {thin:?}"
        );
    }

    /// The census names the ARM whose drawn shaft passes the wall inside the
    /// gap, and only it: the same arm a millimetre out, an arm that lands on
    /// the top face, and a mast clear of everything, are clear. (Until this
    /// census no rule measured an arm's own shaft against the model.)
    #[test]
    fn the_census_names_the_shaft_that_passes_the_wall_inside_the_gap() {
        let surface = block();
        let r = 0.15;
        let near = 5.0 + r + 0.15;
        let far = 5.0 + r + 1.0;
        let members = [
            member(0, StrutKind::Arm, Landing::Plate, [near, 0.0], PLATE, 4.0, r),
            member(1, StrutKind::Arm, Landing::Plate, [far, 0.0], PLATE, 4.0, r),
            member(2, StrutKind::Arm, Landing::Model { layer: 139 }, [0.0, 0.0], 2.0 - H, 6.0, r),
            member(3, StrutKind::Mast, Landing::Plate, [far, 3.0], PLATE, 4.0, r),
        ];
        let (struts, arms): (Vec<SweepStrut>, Vec<SweepArm>) = members.into_iter().unzip();
        let layers = plinth_core::LayerPlan::new(PLATE, vec![H; 400]).expect("a uniform plan");
        let nowhere = crate::cavity::HiddenCavity::none();
        let census = model_clearance_census(Some(&surface), &struts, &arms, &layers, PLATE, GAP, ZONE, &nowhere);
        assert!(census.ran);
        assert_eq!(census.members_unmeasured, 0, "{census:?}");
        assert_eq!(census.members_measured, 4, "{census:?}");
        assert_eq!(census.rows_tapered, 0, "four constant-radius members: no tapered row: {census:?}");
        let named: Vec<u32> = census.within_gap.iter().map(|near| near.member).collect();
        assert_eq!(named, vec![0], "{census:?}");
        let gap = census.within_gap[0].clearance_mm;
        assert!((gap - 0.15).abs() < 1e-6, "the near shaft stands {gap} mm off the wall");
        // No surface, no census: the census says it did not run.
        let none = model_clearance_census(None, &struts, &arms, &layers, PLATE, GAP, ZONE, &nowhere);
        assert!(!none.ran && none.within_gap.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::{member_intrusion, model_intrusion, model_intrusion_tapered, MemberSolid};
    use crate::grid::lay_out_at;
    use crate::up_sweep::{ColumnHistory, ShaftSpan};

    /// **The raster reads the FRUSTUM** (fix/drawn-solid-clearance,
    /// 2026-10-04): a member drawn as one chord from 0.1 mm at its top to
    /// 0.5 mm at its foot, passing a cured column 0.7 mm off its axis that
    /// stands only beside its THIN upper half. Read at the wider end all
    /// along (0.5 + 0.3 + half a cell) it enters the column; read as the
    /// frustum (0.1 to 0.3 mm there) it clears. A column beside the thick
    /// foot is still entered.
    #[test]
    fn the_raster_reads_a_tapered_member_as_the_frustum_it_is() {
        let grid = lay_out_at(0.0, 0.0, 0.05, 100, 100);
        let h = 0.05;
        let (x, y) = (grid.center_x_mm(50), grid.center_y_mm(50));
        let column_at = |k_bottom: u32, k_top: u32| {
            let mut history = ColumnHistory::new(&grid);
            let cell = grid.index(50, 50);
            history.model_spans.push(ShaftSpan { k_bottom, k_top, id: 0, next: u32::MAX, landable_end: u32::MAX });
            history.model_head[cell] = 0;
            history
        };
        // The member: top at z = 4.0, foot at z = 0.0, 0.7 mm beside the column.
        let (top, foot) = ([x + 0.7, y, 4.0], [x + 0.7, y, 0.0]);
        // The column over z 3.0..3.5, in the thin upper half (radius about 0.2).
        let upper = column_at(60, 70);
        let gap = 0.3;
        assert!(
            model_intrusion(&grid, &upper, h, foot, top, 0.5, gap).is_some(),
            "read at the wider end all along, the member enters the column"
        );
        assert!(
            model_intrusion_tapered(&grid, &upper, h, foot, top, (0.5, 0.1), gap).is_none(),
            "read as the frustum, the thin upper half clears it"
        );
        // The same column over z 0.2..0.7, beside the thick foot (radius about 0.45).
        let lower = column_at(4, 14);
        assert!(
            model_intrusion_tapered(&grid, &lower, h, foot, top, (0.5, 0.1), gap).is_some(),
            "beside the thick foot the frustum still enters it"
        );
    }

    /// **THE FRUSTUM'S TRUE CLEARANCE, NOT THE CLEARANCE AT THE NEAREST AXIS
    /// POINT** (challenger round 2, 2026-10-04). A 1 mm axis drawn from 0.1 mm
    /// at its thin end to 0.9 mm at its thick end, and a one-lamina cured
    /// column 0.65 mm off the axis beside the THIN end. The nearest axis point
    /// to the column is the thin end (0.65 mm), where the radius is 0.1: that
    /// reading gives 0.55 mm and accepts. But at the thick end the solid is
    /// `√(0.65² + 1²) − 0.9 = 0.292 mm` from the column, under the 0.3 mm gap
    /// (and the raster's half-diagonal on top). The clearance of a frustum to
    /// a point is the minimum over the axis of `distance − radius`, and that is
    /// convex in the axis parameter. Hand-derived; red before the fix.
    #[test]
    fn a_taper_is_judged_by_distance_minus_radius_not_at_the_nearest_axis_point() {
        let grid = lay_out_at(0.0, 0.0, 0.05, 100, 100);
        let h = 0.05;
        let (x, y) = (grid.center_x_mm(50), grid.center_y_mm(50));
        let mut history = ColumnHistory::new(&grid);
        // One lamina at layer 20, z 1.0..1.05.
        history.model_spans.push(ShaftSpan { k_bottom: 20, k_top: 20, id: 0, next: u32::MAX, landable_end: u32::MAX });
        history.model_head[grid.index(50, 50)] = 0;
        // The axis runs z 1.0 (thin end) down to z 0.0 (thick end), 0.65 mm off.
        let (thin, thick) = ([x + 0.65, y, 1.0], [x + 0.65, y, 0.0]);
        let gap = 0.3;
        assert!(
            model_intrusion_tapered(&grid, &history, h, thick, thin, (0.9, 0.1), gap).is_some(),
            "the thick end stands 0.292 mm from the column: inside the final gap"
        );
        // Control: the same column beside a taper that is thin all the way
        // clears (0.65 − 0.12 − 0.3 − 0.035 > 0).
        assert!(model_intrusion_tapered(&grid, &history, h, thick, thin, (0.12, 0.1), gap).is_none());
        // Control: the cured column stays clear of a thick end that is really farther.
        let far = ([x + 0.8, y, 1.0], [x + 0.8, y, 0.0]);
        assert!(model_intrusion_tapered(&grid, &history, h, far.1, far.0, (0.9, 0.1), gap).is_none());
    }

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
            landable_end: u32::MAX,
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
    /// The lowering draws this profile as three spans: 0.8 mm below `z = 8`,
    /// then the bare step as a cone ABOVE it (junction round, 2026-10-08: no
    /// flat shoulder) from 0.8 mm at `z = 8` to 0.1 mm at
    /// `z_c = 8 + 0.7·tan(32.5°) = 8.4459` (the registered lean law's
    /// saturated slope, `1/tan(45° − 25°/2)` mm of radius per mm), then 0.1 mm
    /// to the tip. The walk reads each span round-ended about its axis at its
    /// wider end's radius (a capsule, which holds the frustum): the cone's top
    /// end `(0,0,z_c)` is `(0.6 − (z_c − 7.9)·s)/√(1 + s²) = 0.4927` mm from
    /// the candidate's line, `s = 0.2/1.1`, so the walk reports `−0.4073` mm,
    /// beyond the flat-ended `−0.3182`: the capsule errs only toward refusal.
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
        let cone_top = 8.0 + 0.7 * 32.5_f64.to_radians().tan();
        let expected = (0.6 - (cone_top - 7.9) * s) / (1.0 + s * s).sqrt() - 0.8 - 0.1;
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
