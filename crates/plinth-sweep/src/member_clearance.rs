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
        let _site = crate::perf::clear_site(crate::perf::ClearSite::Descent);
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
#[expect(
    clippy::too_many_arguments,
    reason = "the field, the layer plan and the three registered quantities the census reads \
              (gap, lean-law zone factor, hidden cavity); a bundle would exist for this call only"
)]
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
        let Some((top, bottom, post_start_radius)) =
            drawn_shaft_ends(strut, arm, disc, flare.as_ref(), plate_z_mm)
        else {
            unmeasured_member[index] = true;
            continue;
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
                //
                // **A TIP WITH NO MEASURED FACE EXEMPTS ONLY ITS BALL** (round 6,
                // 2026-10-14, `NUB-CLAW.md` part 2). The zone is read off the
                // face the tip carries. Where the disc's plane is the raster
                // seat's own (no face measured, or its plane passed clear of
                // the ball), the plane says nothing about where the model is
                // and the zone off it is a guess: Deinonychus arm 143's ball
                // stood 0.137 mm inside the claw, the raster plane put the
                // zone at 0.5 mm of axis, and 0.32 mm of shaft inside the
                // claw was never measured. An unknown zone exempts nothing
                // ([`contact_zone_mm`]): the shaft past the ball is measured
                // and, inside the gap, warned.
                Some(disc) if arm.seats_into_model && !disc.face_measured => disc.ball_radius_mm,
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

/// The drawn shaft of one member as the file draws it: its top (the ball
/// centre, or the joint `K` of a headed arm, where the post starts; a member
/// with no ball starts at its seat), its bottom (its landing, or its boss top
/// for a plate member) and the radius its top span starts at. `None` where a
/// plate member's boss cannot be derived.
fn drawn_shaft_ends(
    strut: &crate::SweepStrut,
    arm: &plinth_support::SweepArm,
    disc: Option<plinth_support::TipContactDisc>,
    flare: Option<&plinth_support::SweepRootFlare>,
    plate_z_mm: f64,
) -> Option<([f64; 3], [f64; 3], f64)> {
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
        flare.and_then(|flare| flare.plate_shaft_of(arm, plate_z_mm))?.1
    } else {
        arm.base_mm
    };
    Some((top, bottom, post_start_radius))
}

/// One drawn shaft span of a member, as [`tips_inside_members`] reads it.
struct Span {
    member: usize,
    top: [f64; 3],
    bottom: [f64; 3],
    top_radius_mm: f64,
    bottom_radius_mm: f64,
}

/// A member's drawn shaft ends, `(top, bottom)`.
type ShaftEnds = Option<([f64; 3], [f64; 3])>;

/// One frustum of a head's chain: from, to, radius at from, radius at to.
type HeadFrustum = ([f64; 3], [f64; 3], f64, f64);

/// The planned joints of a field by member-index pair: the joint point on each
/// member of the pair ([`designed_joints`]).
type DesignedJoints = std::collections::HashMap<(usize, usize), Vec<([f64; 3], [f64; 3])>>;

/// Every member's drawn shaft spans, and each member's drawn `(top, bottom)`.
struct FieldSpans {
    spans: Vec<Span>,
    /// By member index: the drawn shaft's top and bottom, `None` for a member
    /// with no drawable shaft (dead, or a plate member whose boss cannot be
    /// derived, or no rise).
    ends: Vec<ShaftEnds>,
}

/// The spans the file draws every member's shaft as: ONE pass over the field,
/// shared by [`tips_inside_members`] and [`member_collisions`].
fn field_spans(
    struts: &[crate::SweepStrut],
    arms: &[plinth_support::SweepArm],
    discs: &[Option<plinth_support::TipContactDisc>],
    flare: Option<&plinth_support::SweepRootFlare>,
    plate_z_mm: f64,
) -> FieldSpans {
    let mut spans: Vec<Span> = Vec::new();
    let mut ends: Vec<ShaftEnds> = vec![None; struts.len()];
    for (index, (strut, arm)) in struts.iter().zip(arms).enumerate() {
        if strut.k_bottom == usize::MAX || strut.k_top <= strut.k_bottom {
            continue;
        }
        let disc = discs.get(index).copied().flatten();
        let Some((top, bottom, post_start_radius)) =
            drawn_shaft_ends(strut, arm, disc, flare, plate_z_mm)
        else {
            continue;
        };
        let rise = top[2] - bottom[2];
        if !(rise.is_finite() && rise > 0.0) {
            continue;
        }
        ends[index] = Some((top, bottom));
        let root = plinth_support::drawn_root_radius_mm(&arm.radius_profile_mm, arm.tip_radius_mm);
        let at = |z: f64| -> [f64; 3] {
            let t = ((top[2] - z) / rise).clamp(0.0, 1.0);
            std::array::from_fn(|axis| top[axis] + t * (bottom[axis] - top[axis]))
        };
        for span in plinth_support::drawn_spans(
            &arm.radius_profile_mm,
            arm.base_mm[2],
            arm.seat_mm[2],
            top[2],
            bottom[2],
            post_start_radius,
            root,
        ) {
            spans.push(Span {
                member: index,
                top: at(span.top_z_mm),
                bottom: at(span.bottom_z_mm),
                top_radius_mm: span.top_radius_mm.max(0.0),
                bottom_radius_mm: span.bottom_radius_mm.max(0.0),
            });
        }
    }
    FieldSpans { spans, ends }
}

/// **No tip or head lies inside another member's drawn solid** (`MODEL-FEET.md`
/// F4, 2026-10-12). Deinonychus, `for-craig-joint`: tip 145's 0.13 mm ball and
/// its whole head stood inside column 142 (82 % of the ball's surface inside
/// it, none showing), and no rule measured an arm's drawn solid against
/// another arm's tip or head (the run report said so).
///
/// Each seated tip is read as the segment from its contact ball's centre to
/// where its post starts (the ball centre itself for a straight arm, the joint
/// `K` for a headed one: the head's axis). Each OTHER member is read as the
/// spans the file draws its shaft as ([`plinth_support::drawn_spans`], from
/// [`drawn_shaft_ends`]), each a frustum. The tip is inside a member where the
/// segment comes nearer a span's axis than that span's drawn radius at the
/// nearest point. A member that lands or seats ON the tip's own arm is joined
/// to it by plan and is the landing check's to answer for, so it is not read
/// here; a member the tip's arm lands on is (case B: 145 lands on 142's post,
/// and its head is inside 142's column above the landing).
///
/// One row per buried tip, its deepest host. A WARNING census, never a gate;
/// the clearance re-route is sent each holding member that is an arm
/// ([`crate::lip_seat::clearance_violators`]). Batch-first: one contact-disc
/// batch for the field, the spans bucketed once in a plan grid, and each tip
/// reads only the buckets its own segment spans.
#[must_use]
pub(crate) fn tips_inside_members(
    struts: &[crate::SweepStrut],
    arms: &[plinth_support::SweepArm],
    layer_plan: &plinth_core::LayerPlan,
    plate_z_mm: f64,
) -> Vec<crate::report::TipInsideMember> {
    /// The bucket side, mm: a packing size for the lookup and nothing else
    /// (every span a tip could be inside is in a bucket its segment reads,
    /// whatever the size).
    const BUCKET_MM: f64 = 1.0;
    let height = layer_plan.heights_mm().first().copied().unwrap_or(0.0);
    let flare = plinth_support::SweepRootFlare::registered(height).ok();
    let discs = plinth_support::tip_contact_discs(arms, layer_plan)
        .unwrap_or_else(|_| vec![None; arms.len()]);
    let spans = field_spans(struts, arms, &discs, flare.as_ref(), plate_z_mm).spans;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a bucket index of a model-scale coordinate over a 1 mm bucket"
    )]
    let bucket = |value: f64| (value / BUCKET_MM).floor() as i64;
    let mut buckets: std::collections::HashMap<(i64, i64), Vec<usize>> = std::collections::HashMap::new();
    for (slot, span) in spans.iter().enumerate() {
        let reach = span.top_radius_mm.max(span.bottom_radius_mm);
        let (x0, x1) = (span.top[0].min(span.bottom[0]) - reach, span.top[0].max(span.bottom[0]) + reach);
        let (y0, y1) = (span.top[1].min(span.bottom[1]) - reach, span.top[1].max(span.bottom[1]) + reach);
        for bx in bucket(x0)..=bucket(x1) {
            for by in bucket(y0)..=bucket(y1) {
                buckets.entry((bx, by)).or_default().push(slot);
            }
        }
    }
    let joined_to = |host: usize, tip: usize| {
        let tip_id = struts[tip].id;
        matches!(struts[host].landing, crate::Landing::Strut { id, .. } if id == tip_id)
            || struts[host].seat_on_strut == Some(tip_id)
    };
    let mut rows = Vec::new();
    for (tip, (strut, arm)) in struts.iter().zip(arms).enumerate() {
        if strut.kind != crate::StrutKind::Arm || strut.k_bottom == usize::MAX || !arm.seats_into_model {
            continue;
        }
        let Some(disc) = discs.get(tip).copied().flatten() else { continue };
        let (from, to) = (disc.ball_centre_mm, disc.post_top_mm);
        let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut deepest: Option<(usize, f64)> = None;
        for bx in bucket(from[0].min(to[0]))..=bucket(from[0].max(to[0])) {
            for by in bucket(from[1].min(to[1]))..=bucket(from[1].max(to[1])) {
                let Some(list) = buckets.get(&(bx, by)) else { continue };
                for &slot in list {
                    let span = &spans[slot];
                    if span.member == tip || joined_to(span.member, tip) || !seen.insert(slot) {
                        continue;
                    }
                    let (distance, _, on_span) =
                        segment_segment_distance_min(from, to, span.top, span.bottom);
                    let length = geom_kernel::vec3::dist(span.top, span.bottom);
                    let t = if length > 0.0 {
                        (geom_kernel::vec3::dist(span.top, on_span) / length).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let radius = span.top_radius_mm + t * (span.bottom_radius_mm - span.top_radius_mm);
                    let depth = radius - distance;
                    if depth > CONTAINMENT_SLACK_MM && deepest.is_none_or(|(_, worst)| depth > worst) {
                        deepest = Some((span.member, depth));
                    }
                }
            }
        }
        if let Some((host, depth_mm)) = deepest {
            rows.push(crate::report::TipInsideMember {
                tip_member: strut.id,
                host_member: struts[host].id,
                host_kind: struts[host].kind,
                depth_mm,
                ball_radius_mm: disc.ball_radius_mm,
            });
        }
    }
    rows
}

/// Float noise on "the tip's axis is inside the span": the segment distance
/// read against a radius on the same numbers, and nothing more.
const CONTAINMENT_SLACK_MM: f64 = 1.0e-9;

/// The kinds of drawn piece the lowering emits for one member
/// (`plinth-support/src/lower.rs`, `lower_one_sweep_arm`): the order is the
/// order of a tie-break and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PieceKind {
    /// A shaft span: the taper and the chained members ([`plinth_support::drawn_spans`]).
    Shaft,
    /// A head's neck and cone, ball centre to joint.
    Head,
    /// The tip's contact sphere.
    Ball,
    /// A head's joint sphere.
    Joint,
    /// A plate root's boss (the `RootFlare` record).
    Boss,
    /// A model-landed arm's foot ball.
    Foot,
}

/// Two member solids that overlap on the finished field
/// ([`member_collisions`]), the lower member id first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MemberCollision {
    pub low: u32,
    pub high: u32,
    /// The deepest overlap of the two drawn solids, mm: the sum of the two
    /// drawn radii at the nearest pair of axis points less the distance
    /// between them, minimised over BOTH axes ([`piece_gap`]).
    pub depth_mm: f64,
    /// The kinds of the two pieces that overlap deepest, `low`'s first.
    pub low_piece: PieceKind,
    pub high_piece: PieceKind,
}

/// The sine of the smallest angle between two axes a planned joint is
/// excused at (5 degrees). Two axes closer to parallel than this make a lens
/// the joint rule does not excuse beyond [`JOINT_ZONE_CAP`].
const JOINT_SIN_FLOOR: f64 = 0.087_155_742_747_658_17;

/// A planned joint excuses the pieces within this many times `r_a + r_b` of
/// the joint point at most (the lens of two axes at 14.5 degrees).
const JOINT_ZONE_CAP: f64 = 4.0;

/// The margin on a joint's zone: the lens two axes make at a planned joint is
/// `(r_a + r_b) / sin(angle)` long on a joint point that sits on both axes.
/// The joint point of a lander on its host is the lander's end, which stands
/// up to the host's radius off the host's axis, and a piece's end sphere
/// reaches a radius past the cut, so the zone is a quarter longer.
const JOINT_ZONE_MARGIN: f64 = 1.25;

/// One piece the file draws, as the swept-sphere solid that encloses it: the
/// segment `a`-`b` with a radius running linearly from `ra` to `rb`. A sphere
/// is a piece with `a == b`.
#[derive(Clone, Copy, Debug)]
struct Piece {
    member: usize,
    kind: PieceKind,
    a: [f64; 3],
    b: [f64; 3],
    ra: f64,
    rb: f64,
}

impl Piece {
    fn sphere(member: usize, kind: PieceKind, centre: [f64; 3], radius: f64) -> Self {
        Self {
            member,
            kind,
            a: centre,
            b: centre,
            ra: radius,
            rb: radius,
        }
    }

    fn is_sphere(&self) -> bool {
        self.length_of() <= 0.0
    }

    fn at(&self, t: f64) -> [f64; 3] {
        std::array::from_fn(|axis| self.a[axis] + t * (self.b[axis] - self.a[axis]))
    }

    fn radius(&self, t: f64) -> f64 {
        self.ra + t * (self.rb - self.ra)
    }

    fn reach(&self) -> f64 {
        self.ra.max(self.rb)
    }

    fn length_of(&self) -> f64 {
        geom_kernel::vec3::dist(self.a, self.b)
    }
}

/// The smallest `distance - r_p(s) - r_q(t)` of two pieces over the parameter
/// rectangle `s` x `t`, and where: the NEGATIVE of the deepest overlap of the
/// two swept-sphere solids.
///
/// **Sound for tapers** (astra round 2, finding 1): the function is jointly
/// convex in `(s, t)` (a norm of an affine map less two linear radii), so its
/// minimum is the overlap, whatever the radii do. The radii at the closest
/// AXIS points are not the answer: a thin end can stand nearest while the thick
/// end of the other member is what collides (the module doc, and
/// `a_tapered_thick_end_collides_away_from_the_closest_axis_points`). The inner
/// minimum over `t` is closed-form (a hyperbola's vertex, clamped to the
/// segment; a radius that changes faster than the length is monotone and takes
/// an end); the outer is a 60-step ternary search on a convex function, which
/// errs only by leaving the minimum a few nanometres high (a depth a few
/// nanometres short, never an overlap invented).
///
/// A swept sphere encloses the file's frustum (a frustum's section at each axis
/// point lies inside the sphere of that radius there), so this is a
/// CONSERVATIVE enclosing test: it can flag a thin sliver at a flat end cap and
/// never miss an overlap of the drawn solids.
fn piece_gap(p: &Piece, q: &Piece, s: (f64, f64), t: (f64, f64)) -> (f64, f64, f64) {
    use geom_kernel::vec3;
    // The best `t` for one `s`, and its value.
    let inner = |s: f64| -> (f64, f64) {
        let point = p.at(s);
        let radius_p = p.radius(s);
        let value = |t: f64| vec3::dist(point, q.at(t)) - radius_p - q.radius(t);
        let length = q.length_of();
        if q.is_sphere() || length <= 0.0 {
            return (t.0, value(t.0));
        }
        let slope = (q.rb - q.ra) / length;
        if slope.abs() >= 1.0 {
            let (low, high) = (value(t.0), value(t.1));
            return if low <= high { (t.0, low) } else { (t.1, high) };
        }
        let direction = vec3::scale(vec3::sub(q.b, q.a), 1.0 / length);
        let offset = vec3::sub(point, q.a);
        let along = vec3::dot(offset, direction);
        let perpendicular = (vec3::dot(offset, offset) - along * along).max(0.0).sqrt();
        let vertex = along + slope * perpendicular / (1.0 - slope * slope).sqrt();
        let at = (vertex / length).clamp(t.0, t.1);
        (at, value(at))
    };
    if p.is_sphere() || s.1 <= s.0 {
        let (at, value) = inner(s.0);
        return (value, s.0, at);
    }
    let (mut low, mut high) = s;
    for _ in 0..60 {
        let third = (high - low) / 3.0;
        let (m1, m2) = (low + third, high - third);
        if inner(m1).1 <= inner(m2).1 {
            high = m2;
        } else {
            low = m1;
        }
    }
    let mid = 0.5 * (low + high);
    let best = [s.0, mid, s.1]
        .into_iter()
        .map(|at| (inner(at), at))
        .min_by(|x, y| x.0 .1.total_cmp(&y.0 .1))
        .expect("three candidates");
    ((best.0).1, best.1, (best.0).0)
}

/// The head of a headed arm as the chain of straight frusta the lowering draws
/// from the contact ball to the carried joint `K` (`push_head_segment` and
/// `push_head_chain`, `lower.rs`), as `(from, to, radius from, radius to)`.
/// The joint SPHERE at `K` is drawn by the caller. The three ways the lowering
/// cuts the chain are the three branches here (a certified link, a link, the
/// clipped polyline with its pixel merge); `None` where the lowering draws the
/// arm straight.
fn head_chain(
    arm: &plinth_support::SweepArm,
    disc: &plinth_support::TipContactDisc,
    pixel_pitch_mm: f64,
) -> Option<Vec<HeadFrustum>> {
    use geom_kernel::vec3;
    /// `LINK_MIN_LENGTH_MM` in the lowering.
    const LINK_MIN_MM: f64 = 1.0e-6;
    let head = arm.head.as_ref()?;
    let unit = vec3::normalize(head.unit)?;
    if unit[2] >= 0.0 {
        return None;
    }
    let knots = &head.profile_knots_mm;
    let length = knots.last()?[0];
    let centre = disc.ball_centre_mm;
    let ball = disc.ball_radius_mm;
    let centre_s = vec3::dot(unit, vec3::sub(centre, head.contact_mm));
    // Where the head starts: the ball centre, or the contact disc's centre on
    // the seat plane where the centre stands under it.
    let start: [f64; 3] = if centre_s < 0.0 {
        std::array::from_fn(|axis| centre[axis] - centre_s * unit[axis])
    } else {
        centre
    };
    let s_start = vec3::dot(unit, vec3::sub(start, head.contact_mm)).max(0.0);
    if s_start.partial_cmp(&length) != Some(std::cmp::Ordering::Less) {
        return None;
    }
    let chain: Vec<[f64; 2]> = if head.certified
        && head.link_mm > 0.0
        && head.link_radius_mm > 0.0
        && knots.first().is_some_and(|knot| knot[0] > s_start + LINK_MIN_MM)
    {
        std::iter::once([s_start, head.link_radius_mm]).chain(knots.iter().copied()).collect()
    } else if head.link_mm > 0.0 && head.link_radius_mm > 0.0 {
        let link = head.link_radius_mm;
        let exit = (ball * ball - link * link).max(0.0).sqrt();
        let link_end = (centre_s + exit + head.link_mm)
            .max(s_start + LINK_MIN_MM)
            .min(length - LINK_MIN_MM);
        let cone = knots.get(1..).filter(|cone| !cone.is_empty())?;
        let first = cone
            .partition_point(|knot| knot[0] < link_end + pixel_pitch_mm)
            .min(cone.len() - 1);
        [[s_start, link], [link_end, link]]
            .into_iter()
            .chain(cone[first..].iter().copied())
            .collect()
    } else {
        let past = knots
            .partition_point(|knot| knot[0] <= s_start + 1.0e-6)
            .min(knots.len() - 1);
        let first_radius = match past.checked_sub(1).map(|index| knots[index]) {
            Some(below) => {
                let above = knots.get(past).copied().unwrap_or(below);
                let t = (s_start - below[0]) / (above[0] - below[0]).max(f64::MIN_POSITIVE);
                below[1] + (above[1] - below[1]) * t.clamp(0.0, 1.0)
            }
            None => knots[0][1],
        };
        let merged = knots
            .partition_point(|knot| knot[0] < s_start + pixel_pitch_mm)
            .min(knots.len() - 1)
            .max(past);
        let first_radius = knots[past..merged]
            .iter()
            .fold(first_radius, |widest, knot| widest.max(knot[1]));
        std::iter::once([s_start, first_radius]).chain(knots[merged..].iter().copied()).collect()
    };
    let joint = head.joint_mm;
    let on_axis = |s: f64| -> [f64; 3] {
        let t = ((s - s_start) / (length - s_start)).clamp(0.0, 1.0);
        std::array::from_fn(|axis| start[axis] + (joint[axis] - start[axis]) * t)
    };
    Some(
        chain
            .windows(2)
            .map(|pair| (on_axis(pair[0][0]), on_axis(pair[1][0]), pair[0][1], pair[1][1]))
            .collect(),
    )
}

/// **Every piece the lowering draws for every member**, as swept spheres, and
/// each member's drawn shaft ends. The inventory matches `lower_one_sweep_arm`
/// piece for piece: the shaft's spans (taper and chained members), the tip's
/// contact ball, a head's frusta and joint sphere, a plate root's boss, and a
/// model-landed arm's foot ball (its foot cone is inside the shaft's span to
/// the foot, which is drawn to the full section: an enclosing piece). The one
/// piece NOT listed is a landed junction's hull, which exists only at a planned
/// joint and lies inside the joint's zone ([`member_collisions`]).
fn drawn_pieces(
    struts: &[crate::SweepStrut],
    arms: &[plinth_support::SweepArm],
    layer_plan: &plinth_core::LayerPlan,
    plate_z_mm: f64,
    pixel_pitch_mm: f64,
) -> (Vec<Piece>, Vec<ShaftEnds>) {
    let height = layer_plan.heights_mm().first().copied().unwrap_or(0.0);
    let flare = plinth_support::SweepRootFlare::registered(height).ok();
    let boss = plinth_support::RootBoss::registered().ok();
    let discs = plinth_support::tip_contact_discs(arms, layer_plan)
        .unwrap_or_else(|_| vec![None; arms.len()]);
    let feet = plinth_support::model_foot_discs(arms, layer_plan)
        .unwrap_or_else(|_| vec![None; arms.len()]);
    let field = field_spans(struts, arms, &discs, flare.as_ref(), plate_z_mm);
    let mut pieces: Vec<Piece> = Vec::with_capacity(field.spans.len() + 3 * struts.len());
    for span in &field.spans {
        pieces.push(Piece {
            member: span.member,
            kind: PieceKind::Shaft,
            a: span.top,
            b: span.bottom,
            ra: span.top_radius_mm,
            rb: span.bottom_radius_mm,
        });
    }
    for (index, (strut, arm)) in struts.iter().zip(arms).enumerate() {
        if strut.k_bottom == usize::MAX || strut.k_top <= strut.k_bottom {
            continue;
        }
        let disc = discs.get(index).copied().flatten();
        if let Some(disc) = disc.filter(|_| arm.seats_into_model) {
            pieces.push(Piece::sphere(index, PieceKind::Ball, disc.ball_centre_mm, disc.ball_radius_mm));
            if let (Some(head), Some(_)) = (arm.head.as_ref(), disc.head_joint_radius_mm) {
                if let Some(chain) = head_chain(arm, &disc, pixel_pitch_mm) {
                    for (a, b, ra, rb) in chain {
                        pieces.push(Piece {
                            member: index,
                            kind: PieceKind::Head,
                            a,
                            b,
                            ra,
                            rb,
                        });
                    }
                    pieces.push(Piece::sphere(index, PieceKind::Joint, head.joint_mm, head.joint_radius_mm));
                }
            }
        }
        if let Some(foot) = feet.get(index).copied().flatten() {
            pieces.push(Piece::sphere(index, PieceKind::Foot, foot.ball_centre_mm, foot.ball_radius_mm));
        }
        if let (Some(flare), Some(boss)) = (flare.as_ref(), boss) {
            if let Some((_, bottom)) = flare.plate_shaft_of(arm, plate_z_mm) {
                let foot_z = plate_z_mm + flare.raft_top_above_plate_mm()
                    - plinth_support::NATIVE_FOOT_PAD_OVERLAP_MM;
                if strut.landing == crate::Landing::Plate && bottom[2] > foot_z + 1.0e-9 {
                    let root = plinth_support::drawn_root_radius_mm(&arm.radius_profile_mm, arm.tip_radius_mm);
                    pieces.push(Piece {
                        member: index,
                        kind: PieceKind::Boss,
                        a: bottom,
                        b: [arm.base_mm[0], arm.base_mm[1], foot_z],
                        ra: root,
                        rb: boss.foot_radius_mm(root),
                    });
                }
            }
        }
    }
    (pieces, field.ends)
}

/// The parameter interval of the piece within `zone` of `joint`, or `None`.
fn near_interval(piece: &Piece, joint: [f64; 3], zone: f64) -> Option<(f64, f64)> {
    use geom_kernel::vec3;
    if piece.is_sphere() {
        return (vec3::dist(piece.a, joint) <= zone).then_some((0.0, 1.0));
    }
    let direction = vec3::sub(piece.b, piece.a);
    let offset = vec3::sub(piece.a, joint);
    let a = vec3::dot(direction, direction);
    let b = vec3::dot(direction, offset);
    let c = vec3::dot(offset, offset) - zone * zone;
    let discriminant = b.powi(2) - a * c;
    if discriminant < 0.0 || a <= 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let (low, high) = (((-b - root) / a).max(0.0), ((-b + root) / a).min(1.0));
    (low <= high).then_some((low, high))
}

/// The radius of the piece at the axis point nearest `point`.
fn radius_near(piece: &Piece, point: [f64; 3]) -> f64 {
    use geom_kernel::vec3;
    let direction = vec3::sub(piece.b, piece.a);
    let length_sq = vec3::dot(direction, direction);
    if length_sq <= 0.0 {
        return piece.ra;
    }
    let t = (vec3::dot(vec3::sub(point, piece.a), direction) / length_sq).clamp(0.0, 1.0);
    piece.radius(t)
}

/// The deepest overlap of two pieces of two members joined by PLANNED joints,
/// with the allowed overlap at each joint taken out: the parameter rectangle is
/// cut at the edges of every joint's zone on both pieces, and every cell that is
/// not wholly inside one joint's zone on BOTH pieces is measured. Pieces keep
/// every part away from the joint, and a part of one inside the zone is still
/// measured against the parts of the other outside it.
fn overlap_beyond_joints(p: &Piece, q: &Piece, joints: &[([f64; 3], [f64; 3])]) -> (f64, f64, f64) {
    use geom_kernel::vec3;
    let sine = if p.is_sphere() || q.is_sphere() {
        1.0
    } else {
        match (
            vec3::normalize(vec3::sub(p.b, p.a)),
            vec3::normalize(vec3::sub(q.b, q.a)),
        ) {
            (Some(x), Some(y)) => vec3::norm(vec3::cross(x, y)),
            _ => 1.0,
        }
    };
    let mut s_cuts = vec![0.0, 1.0];
    let mut t_cuts = vec![0.0, 1.0];
    let mut blocks: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for &(at_p, at_q) in joints {
        let sum = radius_near(p, at_p) + radius_near(q, at_q);
        let zone = (JOINT_ZONE_MARGIN * sum / sine.max(JOINT_SIN_FLOOR)).min(JOINT_ZONE_CAP * sum);
        if let (Some(ns), Some(nt)) = (near_interval(p, at_p, zone), near_interval(q, at_q, zone)) {
            s_cuts.extend([ns.0, ns.1]);
            t_cuts.extend([nt.0, nt.1]);
            blocks.push((ns, nt));
        }
    }
    let cells = |cuts: &mut Vec<f64>, sphere: bool| -> Vec<(f64, f64)> {
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        if sphere {
            return vec![(0.0, 0.0)];
        }
        cuts.windows(2).map(|pair| (pair[0], pair[1])).collect()
    };
    let s_cells = cells(&mut s_cuts, p.is_sphere());
    let t_cells = cells(&mut t_cuts, q.is_sphere());
    let mut deepest = (f64::INFINITY, 0.0, 0.0);
    for &s in &s_cells {
        for &t in &t_cells {
            let (mid_s, mid_t) = (0.5 * (s.0 + s.1), 0.5 * (t.0 + t.1));
            let allowed = blocks.iter().any(|(ns, nt)| {
                ns.0 <= mid_s && mid_s <= ns.1 && nt.0 <= mid_t && mid_t <= nt.1
            });
            if allowed {
                continue;
            }
            let found = piece_gap(p, q, s, t);
            if found.0 < deepest.0 {
                deepest = found;
            }
        }
    }
    deepest
}

/// **No two members' drawn solids overlap** (`SC-DESIGN.md` §4, G3), over
/// every pair of members of every kind (arm, brace, mast, tie), on the whole
/// drawn inventory of each ([`drawn_pieces`]): shaft spans, tip balls, head
/// frusta and joint spheres, root bosses, model feet. The existing
/// [`tips_inside_members`] reads only a tip's ball-centre-to-post-top segment
/// against the other shafts, so a straight arm that crosses a brace mid-shaft,
/// or a head whose joint sphere stands in a neighbour, passes it; this census
/// reads the solids.
///
/// A pair of pieces collides where the swept-sphere solids overlap by more than
/// [`CONTAINMENT_SLACK_MM`] ([`piece_gap`]: sound for tapers, conservative at
/// flat end caps). A PLANNED JOINT allows an overlap only NEAR the joint, never
/// along a whole member (the rule `member_solids_clear` reads: the trunk under a
/// post is not exempt):
///
/// * a member that lands on another (`Landing::Strut`) meets it at its drawn
///   bottom, and one that seats on another (`seat_on_strut`) at its drawn top;
/// * members within three hops of one another in the graph of those landings
///   and seats (a child, its host, the host's host, and the host's other
///   children) are a CLUSTER that meets at every joint point of the cluster
///   ([`designed_joints`]); the two diagonals of an `|X|` also meet where their
///   axes cross.
///
/// The joint's ZONE is `1.25 (r_a + r_b) / sin(angle)` along each axis from the
/// joint point (the lens two axes that cross at a planned joint overlap along),
/// at most [`JOINT_ZONE_CAP`] times `r_a + r_b` ([`overlap_beyond_joints`]).
/// Beyond it the overlap is a collision, which is how a lander that crosses its
/// host 3 mm above its joint is named.
///
/// Batch-first: ONE inventory for the field, the pieces bucketed once on a plan
/// grid, an exact axis-distance bound before any solve, and every candidate
/// pair tested once, in the lowest bucket the two share. Sorted by
/// `(low, high)`, so the answer does not depend on thread count or hash order.
#[must_use]
pub(crate) fn member_collisions(
    struts: &[crate::SweepStrut],
    arms: &[plinth_support::SweepArm],
    layer_plan: &plinth_core::LayerPlan,
    plate_z_mm: f64,
    pixel_pitch_mm: f64,
) -> Vec<MemberCollision> {
    /// The bucket side, mm: a packing size and nothing else.
    const BUCKET_MM: f64 = 1.0;
    let (pieces, ends) = drawn_pieces(struts, arms, layer_plan, plate_z_mm, pixel_pitch_mm);
    let joints = designed_joints(struts, &ends);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a bucket index of a model-scale coordinate over a 1 mm bucket"
    )]
    let bucket = |value: f64| (value / BUCKET_MM).floor() as i64;
    // Each piece's bucket rectangle `(bx0, bx1, by0, by1)` and z range.
    let extents: Vec<(i64, i64, i64, i64)> = pieces
        .iter()
        .map(|piece| {
            let reach = piece.reach();
            (
                bucket(piece.a[0].min(piece.b[0]) - reach),
                bucket(piece.a[0].max(piece.b[0]) + reach),
                bucket(piece.a[1].min(piece.b[1]) - reach),
                bucket(piece.a[1].max(piece.b[1]) + reach),
            )
        })
        .collect();
    let z_range = |piece: &Piece| {
        let reach = piece.reach();
        (
            piece.a[2].min(piece.b[2]) - reach,
            piece.a[2].max(piece.b[2]) + reach,
        )
    };
    let mut buckets: std::collections::BTreeMap<(i64, i64), Vec<usize>> = std::collections::BTreeMap::new();
    for (slot, &(x0, x1, y0, y1)) in extents.iter().enumerate() {
        for bx in x0..=x1 {
            for by in y0..=y1 {
                buckets.entry((bx, by)).or_default().push(slot);
            }
        }
    }
    let mut found: std::collections::BTreeMap<(u32, u32), (f64, PieceKind, PieceKind)> =
        std::collections::BTreeMap::new();
    for (&cell, list) in &buckets {
        for (at, &i) in list.iter().enumerate() {
            for &j in &list[at + 1..] {
                let (first, second) = (&pieces[i], &pieces[j]);
                if first.member == second.member {
                    continue;
                }
                // Test the pair once, in the lowest bucket the two share.
                let (ei, ej) = (extents[i], extents[j]);
                if (ei.0.max(ej.0), ei.2.max(ej.2)) != cell {
                    continue;
                }
                let (zi, zj) = (z_range(first), z_range(second));
                if zi.1 < zj.0 || zj.1 < zi.0 {
                    continue;
                }
                // Exact bound: no solid can reach further than its widest end
                // from its axis.
                let (axes, _, _) = segment_segment_distance_min(first.a, first.b, second.a, second.b);
                if axes - first.reach() - second.reach() >= -CONTAINMENT_SLACK_MM {
                    continue;
                }
                let (p, q) = if first.member < second.member {
                    (first, second)
                } else {
                    (second, first)
                };
                let measured = match joints.get(&(p.member, q.member)) {
                    Some(planned) => overlap_beyond_joints(p, q, planned),
                    None => piece_gap(p, q, (0.0, 1.0), (0.0, 1.0)),
                };
                let depth = -measured.0;
                if depth <= CONTAINMENT_SLACK_MM {
                    continue;
                }
                let key = (struts[p.member].id, struts[q.member].id);
                let entry = found.entry(key).or_insert((depth, p.kind, q.kind));
                let deeper = match depth.total_cmp(&entry.0) {
                    std::cmp::Ordering::Greater => true,
                    std::cmp::Ordering::Equal => (p.kind, q.kind) < (entry.1, entry.2),
                    std::cmp::Ordering::Less => false,
                };
                if deeper {
                    *entry = (depth, p.kind, q.kind);
                }
            }
        }
    }
    found
        .into_iter()
        .map(|((low, high), (depth_mm, low_piece, high_piece))| MemberCollision {
            low,
            high,
            depth_mm,
            low_piece,
            high_piece,
        })
        .collect()
}
/// How far apart in the graph of joined members (a member and the member it
/// lands on or seats on) two members may stand and still be a cluster that
/// meets at its planned joints: a child, its host, and the host's own host.
/// Three hops reach a grandchild's sibling.
const JOINT_CLUSTER_HOPS: usize = 3;

/// The planned joints of the field, by member-index pair `(low, high)`: the
/// joint points of the pair. One pass over the members.
///
/// A member's OWN joint points are where it lands on a member (its drawn
/// bottom) and where its top seats on one. Members that are RELATIVES, within
/// [`JOINT_CLUSTER_HOPS`] of one another in the graph of those landings and
/// seats, share every joint point of both: a grandchild lands on a child that
/// lands on a trunk, and the three solids all meet where the grandchild's end
/// is (Cylinder 85/88/113 on `a7ffbe3`: the 1.26 mm "collision" of a grandchild
/// with its grandparent is the cluster's joint). The two diagonals of an `|X|`
/// are relatives too (each lands on the post the other seats on) and also meet
/// where their axes cross, which is a joint of the pair.
///
/// This makes a joint a place, not a licence: an overlap is excused only within
/// the joint's zone ([`overlap_beyond_joints`]), on pieces of members that are
/// joined to each other by plan.
fn designed_joints(struts: &[crate::SweepStrut], ends: &[ShaftEnds]) -> DesignedJoints {
    let count = struts.len();
    let mut own: Vec<Vec<[f64; 3]>> = vec![Vec::new(); count];
    let mut adjacent: Vec<std::collections::BTreeSet<usize>> = vec![std::collections::BTreeSet::new(); count];
    for (member, strut) in struts.iter().enumerate() {
        let Some((top, bottom)) = ends[member] else { continue };
        if let crate::Landing::Strut { id, .. } = strut.landing {
            let host = id as usize;
            if host < count && host != member {
                own[member].push(bottom);
                adjacent[member].insert(host);
                adjacent[host].insert(member);
            }
        }
        if let Some(host) = strut.seat_on_strut {
            let host = host as usize;
            if host < count && host != member {
                own[member].push(top);
                adjacent[member].insert(host);
                adjacent[host].insert(member);
            }
        }
    }
    let mut joints: DesignedJoints = std::collections::HashMap::new();
    for first in 0..count {
        // Breadth-first to the cluster's depth, in index order.
        let mut seen: std::collections::BTreeSet<usize> = std::collections::BTreeSet::from([first]);
        let mut frontier = vec![first];
        for _ in 0..JOINT_CLUSTER_HOPS {
            let mut next = Vec::new();
            for &member in &frontier {
                for &neighbour in &adjacent[member] {
                    if seen.insert(neighbour) {
                        next.push(neighbour);
                    }
                }
            }
            frontier = next;
        }
        for &second in seen.iter().filter(|&&other| other > first) {
            let shared: Vec<([f64; 3], [f64; 3])> = own[first]
                .iter()
                .chain(&own[second])
                .map(|&point| (point, point))
                .collect();
            if !shared.is_empty() {
                joints.entry((first, second)).or_default().extend(shared);
            }
        }
    }
    // The `|X|`: braces whose landing and seat are each other's.
    let mut by_posts: std::collections::BTreeMap<(u32, u32), Vec<usize>> = std::collections::BTreeMap::new();
    for (member, strut) in struts.iter().enumerate() {
        if let (crate::StrutKind::Brace, crate::Landing::Strut { id, .. }, Some(seat)) =
            (strut.kind, strut.landing, strut.seat_on_strut)
        {
            by_posts.entry((id, seat)).or_default().push(member);
        }
    }
    for (&(lands_on, seats_on), firsts) in &by_posts {
        for &first in firsts {
            for &second in by_posts.get(&(seats_on, lands_on)).map_or(&[][..], Vec::as_slice) {
                if first >= second {
                    continue;
                }
                if let (Some((top_a, bottom_a)), Some((top_b, bottom_b))) = (ends[first], ends[second]) {
                    let (_, on_first, on_second) =
                        segment_segment_distance_min(top_a, bottom_a, top_b, bottom_b);
                    joints.entry((first, second)).or_default().push((on_first, on_second));
                }
            }
        }
    }
    joints
}

/// One straight tip whose shaft grazes the model ([`graze_census`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GrazeRow {
    /// The strut's index in the field.
    pub index: usize,
    /// Degrees between the seat normal and the drawn shaft axis from the ball
    /// centre.
    pub angle_deg: f64,
    /// The smallest measured clearance of the shaft's drawn solid, over the
    /// stretch from the ball out to `R + (g + r_t) / cos x`, mm (an upper
    /// bound under the gap counts as its value).
    pub clearance_mm: f64,
}

/// **Straight tips whose shaft grazes the model** (Craig 2026-10-08: a head
/// only where a straight tip's shaft would graze), on the arms exactly as the
/// lowering gets them. The audit's report 4, run inside the engine: for each
/// live member in `members` that seats into the model, has no drawn head
/// (none, or one given up) and is not wholly in a hidden cavity, with `x` the
/// angle between its seat normal and the drawn shaft axis from the ball
/// centre, the tip grazes when the arrival skims ([`crate::heads::skims`], at
/// the shaft's start radius) AND the drawn solid, read from the axial
/// `sqrt(R^2 - r_t^2)` where the shaft leaves the ball out to
/// `R + (g + r_t) / cos x` (the whole shaft when `cos x <= 0`), comes within
/// `gap_mm` of the model. The caller picks `members`: the conversion offers
/// support arms only; the final warning offers every member, so a brace, a
/// mast or a tie whose tip grazes is named too (warned, never converted).
///
/// `arms[i]` is the lowering input of `struts[members[i]]`. Rows come in
/// member order. Batch-first: one contact-disc batch and ONE frustum
/// clearance call for the field; a member the kernel did not decide is not a
/// grazer here (it is unmeasured, not clear).
#[expect(
    clippy::too_many_arguments,
    reason = "the field, the layer plan and the registered quantities the census reads (gap, \
              allowance, cavity); a bundle would exist for this call only"
)]
pub(crate) fn graze_census(
    surface: Option<&geom_kernel::NativeSurface>,
    struts: &[crate::SweepStrut],
    members: &[usize],
    arms: &[plinth_support::SweepArm],
    layer_plan: &plinth_core::LayerPlan,
    plate_z_mm: f64,
    gap_mm: f64,
    allowance_mm: f64,
    cavity: &crate::cavity::HiddenCavity,
) -> Vec<GrazeRow> {
    let Some(surface) = surface else {
        return Vec::new();
    };
    let height = layer_plan.heights_mm().first().copied().unwrap_or(0.0);
    let flare = plinth_support::SweepRootFlare::registered(height).ok();
    let discs = plinth_support::tip_contact_discs(arms, layer_plan)
        .unwrap_or_else(|_| vec![None; arms.len()]);
    // One row per drawn span of a candidate; the candidate's slot in `members`.
    let mut owner: Vec<usize> = Vec::new();
    let (mut p0s, mut p1s, mut top_radii, mut bottom_radii, mut inset0s, mut inset1s) = (
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    // Per slot: the angle of a candidate that skims.
    let mut angle_deg: Vec<Option<f64>> = vec![None; members.len()];
    for (slot, (&member, arm)) in members.iter().zip(arms).enumerate() {
        let strut = &struts[member];
        if !crate::slenderness::live(strut)
            || strut.head.as_ref().is_some_and(|head| !head.drawn_straight)
            || !arm.seats_into_model
            || strut.k_bottom == usize::MAX
            || strut.k_top <= strut.k_bottom
            || cavity.member_hidden(strut, crate::cavity::arm_radius_mm(arm))
        {
            continue;
        }
        let Some(disc) = discs.get(slot).copied().flatten() else {
            continue;
        };
        let top = disc.ball_centre_mm;
        let bottom = if strut.landing == crate::Landing::Plate {
            match flare.as_ref().and_then(|flare| flare.plate_shaft_of(arm, plate_z_mm)) {
                Some((_, bottom)) => bottom,
                None => continue,
            }
        } else {
            arm.base_mm
        };
        let length = geom_kernel::vec3::dist(top, bottom);
        let rise = top[2] - bottom[2];
        if !(length.is_finite() && length > 0.0 && rise > 0.0) {
            continue;
        }
        let axis = geom_kernel::vec3::scale(geom_kernel::vec3::sub(bottom, top), 1.0 / length);
        let Some(normal) = geom_kernel::vec3::normalize(disc.outward_normal) else {
            continue;
        };
        let cos_x = geom_kernel::vec3::dot(normal, axis).clamp(-1.0, 1.0);
        let r_t = arm.tip_radius_mm;
        if !crate::heads::skims(r_t, cos_x, gap_mm, allowance_mm) {
            continue;
        }
        let ball = disc.ball_radius_mm;
        let from_ball = (ball * ball - r_t * r_t).max(0.0).sqrt();
        let to = if cos_x > 0.0 { (ball + (gap_mm + r_t) / cos_x).min(length) } else { length };
        if to <= from_ball {
            continue;
        }
        angle_deg[slot] = Some(cos_x.acos().to_degrees());
        let root = plinth_support::drawn_root_radius_mm(&arm.radius_profile_mm, arm.tip_radius_mm);
        let spans = plinth_support::drawn_spans(
            &arm.radius_profile_mm,
            arm.base_mm[2],
            arm.seat_mm[2],
            top[2],
            bottom[2],
            r_t,
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
            let (from, a0) = at(span.top_z_mm);
            let (end, a1) = at(span.bottom_z_mm);
            // The stretch of this span inside the tested range; none -> skip.
            if a1 <= a0 || a1 <= from_ball || a0 >= to {
                continue;
            }
            owner.push(slot);
            p0s.push(from);
            p1s.push(end);
            top_radii.push(span.top_radius_mm.max(0.0));
            bottom_radii.push(span.bottom_radius_mm.max(0.0));
            inset0s.push((from_ball - a0).max(0.0));
            inset1s.push((a1 - to).max(0.0));
        }
    }
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
    let mut worst: Vec<Option<f64>> = vec![None; members.len()];
    for (row, &slot) in rows.iter().zip(&owner) {
        let reading = match *row {
            geom_kernel::SegmentClearanceClassified::Measured { clearance_mm, .. } => {
                Some(clearance_mm)
            }
            geom_kernel::SegmentClearanceClassified::UpperBound { clearance_mm, .. }
                if clearance_mm < gap_mm =>
            {
                Some(clearance_mm)
            }
            _ => None,
        };
        if let Some(clearance) = reading {
            if worst[slot].is_none_or(|now| clearance < now) {
                worst[slot] = Some(clearance);
            }
        }
    }
    members
        .iter()
        .enumerate()
        .filter_map(|(slot, &index)| {
            let (angle_deg, clearance_mm) = (angle_deg[slot]?, worst[slot]?);
            (clearance_mm < gap_mm - 1.0e-9).then_some(GrazeRow { index, angle_deg, clearance_mm })
        })
        .collect()
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
    use super::{graze_census, model_clearance_census, model_landing_zones_mm};
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
            thickened_mm: Vec::new(), foot_radius_mm: None,
            seat_on_strut: None,
            k_top,
            k_bottom,
            landing,
            interior: false,
            radius_mm: vec![r; k_top - k_bottom + 1],
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), held_births: Vec::new(), twin_host: false,
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
            contact_ball_radius_mm: None,
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

    /// **A tip with no measured face exempts only its ball** (round 6,
    /// 2026-10-14, `NUB-CLAW.md` part 2; Deinonychus arm 143). The same lean
    /// as the test above with a tip-sized shaft: seated on a MEASURED face its
    /// first stretch is a legitimate contact zone and stays unnamed (the
    /// control above); with no face measured the disc's plane is the raster
    /// seat's, which says nothing about where the model is, so the zone is the
    /// ball and the shaft just past it, standing within the gap of the
    /// model, is named. SABOTAGE: take the `face_measured` arm out of the
    /// seat zone and the unmeasured tip is unnamed too.
    #[test]
    fn a_tip_with_no_measured_face_exempts_only_its_ball() {
        let surface = block();
        let layers = plinth_core::LayerPlan::new(PLATE, vec![H; 400]).expect("a uniform plan");
        let nowhere = crate::cavity::HiddenCavity::none();
        let (sin, cos) = (0.819_f64, 0.574_f64);
        let census = |face: Option<[f64; 3]>| {
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
            arm.seat_face_point_mm = face;
            arm.seats_into_model = true;
            arm.tip_radius_mm = 0.2;
            arm.radius_profile_mm = vec![0.2; arm.radius_profile_mm.len()];
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
        let measured = census(Some([0.0, 0.0, 0.0]));
        assert!(measured.within_gap.is_empty(), "a measured face: contact zone only: {measured:?}");
        let unmeasured = census(None);
        assert_eq!(
            unmeasured.within_gap.len(),
            1,
            "no measured face: only the ball is exempt, the shaft past it is read: {unmeasured:?}"
        );
    }

    /// `layer + pixel`, the registered rasterisation allowance of the skim law.
    const ALLOWANCE: f64 = 0.0692;

    /// A tip of radius 0.1 seated into the flat underside `z = 2` of a slab at
    /// `(xy)`, its shaft running to a base `dx` sideways and 7 mm down, the seat
    /// normal straight down: `atan(dx / 7)` off the normal.
    fn tip_to(id: u32, xy: [f64; 2], dx: f64) -> (SweepStrut, SweepArm) {
        let on_a_support = Landing::Strut { id: 99, layer: 0 };
        let (strut, mut arm) = member(id, StrutKind::Arm, on_a_support, xy, PLATE, 2.0, 0.1);
        arm.base_mm = [xy[0] + dx, xy[1], PLATE];
        arm.seat_normal_mm = [0.0, 0.0, -1.0];
        arm.seat_face_point_mm = Some([xy[0], xy[1], 2.0]);
        arm.seats_into_model = true;
        (strut, arm)
    }

    fn graze_rows_of(
        tips: Vec<(SweepStrut, SweepArm)>,
        cavity: &crate::cavity::HiddenCavity,
    ) -> Vec<super::GrazeRow> {
        let slab = boxed([-10.0, -10.0, 2.0], [10.0, 10.0, 4.0]);
        let (struts, arms): (Vec<SweepStrut>, Vec<SweepArm>) = tips.into_iter().unzip();
        let members: Vec<usize> = (0..struts.len()).collect();
        let layers = plinth_core::LayerPlan::new(PLATE, vec![H; 400]).expect("a uniform plan");
        graze_census(Some(&slab), &struts, &members, &arms, &layers, PLATE, GAP, ALLOWANCE, cavity)
    }

    /// **The graze census** (Craig 2026-10-08: a head only where a straight
    /// shaft would graze). A tip that skims (50 degrees off the underside's
    /// normal) AND whose shaft stands within the final gap of the model beyond
    /// the ball is named, with its angle; the same tip at 20 degrees skims
    /// nothing and is not; a tip in a sealed cavity is skipped; so is one that
    /// carries a head. SABOTAGE: drop the skims test in `graze_census` and the
    /// 30 degree control (which does not skim but stands near the plane) is
    /// named; drop the clearance test and the clear 50 degree tip is named.
    #[test]
    fn the_graze_census_names_the_tip_whose_shaft_skims_and_stands_near_the_model() {
        let none = crate::cavity::HiddenCavity::none();
        let rows = graze_rows_of(vec![tip_to(0, [0.0, 0.0], 7.0 * 50.0_f64.to_radians().tan())], &none);
        assert_eq!(rows.len(), 1, "a 50 degree tip on a flat underside grazes: {rows:?}");
        assert!((rows[0].angle_deg - 50.0).abs() < 0.01, "{rows:?}");
        assert!(rows[0].clearance_mm < GAP, "{rows:?}");
        for degrees in [0.0_f64, 20.0, 30.0] {
            let square = graze_rows_of(vec![tip_to(0, [0.0, 0.0], 7.0 * degrees.to_radians().tan())], &none);
            assert!(square.is_empty(), "{degrees} degrees does not skim: {square:?}");
        }
        // A head carried: not a straight tip.
        let (mut headed_tip, arm) = tip_to(0, [0.0, 0.0], 7.0 * 50.0_f64.to_radians().tan());
        headed_tip.head = Some(crate::heads::Head {
            contact_mm: [0.0, 0.0, 2.0],
            unit: [0.0, 0.0, -1.0],
            joint_mm: [0.0, 0.0, 0.2],
            k_joint: 4,
            joint_cell: 0,
            joint_radius_mm: 0.2,
            planned_tip_radius_mm: 0.1,
            gap_mm: GAP,
            cos_lean_max: 0.5373,
            cells: Vec::new(),
            ball_allowance_mm: 0.0,
            link_mm: 0.0,
            drawn_straight: false,
            law_planned: crate::heads::HeadLaw::envelope(0.5373),
            law_drawn: None,
            ledger_radius_mm: Vec::new(),
            certified: None,
            old_law: None,
        });
        assert!(graze_rows_of(vec![(headed_tip, arm)], &none).is_empty(), "a headed arm is skipped");
    }

    /// A grazing tip standing wholly in a sealed hidden cavity is skipped
    /// (Craig ruling 2026-10-05: no final gap there); the same tip with no
    /// map, and in an open pocket, is named.
    #[test]
    fn the_graze_census_skips_a_tip_in_a_sealed_cavity() {
        use crate::cavity::fixtures::{cavity_box_at, cell, map_of_at};
        let sealed = map_of_at(&cavity_box_at(false, false, PLATE), &[], PLATE);
        let dx = 7.0 * 50.0_f64.to_radians().tan();
        let tip = || {
            let mut tip = tip_to(0, [5.0, 5.0], dx);
            tip.0.cell = cell(&sealed.1, 5.0, 5.0);
            tip
        };
        let open = graze_rows_of(vec![tip()], &crate::cavity::HiddenCavity::none());
        assert_eq!(open.len(), 1, "anti-vacuity: named without the map: {open:?}");
        let hidden = graze_rows_of(vec![tip()], &sealed.0);
        assert!(hidden.is_empty(), "a member in a sealed cavity is not read: {hidden:?}");
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

    /// **A tip inside another member's drawn solid is named** (`MODEL-FEET.md`
    /// F4, case B: tip 145's ball and head inside column 142). A 0.6 mm column
    /// stands from z = 0 to the plate; a 0.1 mm tip seated at z = -1 lands on
    /// it below. With its ball centre 0.5 mm off the column's axis the tip is
    /// inside by 0.1 mm and named with the column as its host; 0.7 mm off,
    /// the ball's SURFACE still overlaps the column but its centre is outside,
    /// and nothing is named (the rule reads the tip's axis against the host's
    /// drawn radius; SABOTAGE: reading the tip's ball radius into it, or the
    /// host's axis alone, flips one of the two). A column that LANDS on the tip's
    /// own arm is joined to it by plan and is not read.
    #[test]
    fn a_tip_inside_another_members_drawn_solid_is_named() {
        let layers = plinth_core::LayerPlan::new(PLATE, vec![H; 400]).expect("a uniform plan");
        let field = |offset: f64, column_lands_on_tip: bool| {
            let column_landing =
                if column_lands_on_tip { Landing::Strut { id: 1, layer: 0 } } else { Landing::Plate };
            let column = member(0, StrutKind::Arm, column_landing, [0.0, 0.0], PLATE, 0.0, 0.6);
            let (mut strut, mut arm) =
                member(1, StrutKind::Arm, Landing::Strut { id: 0, layer: 40 }, [offset, 0.0], -3.0, -1.0, 0.1);
            arm.base_mm = [0.0, 0.0, -3.0];
            arm.seats_into_model = true;
            arm.seat_normal_mm = [0.0, 0.0, -1.0];
            arm.seat_face_point_mm = Some(arm.seat_mm);
            strut.seat_normal = [0.0, 0.0, -1.0];
            let (struts, arms): (Vec<SweepStrut>, Vec<SweepArm>) = [column, (strut, arm)].into_iter().unzip();
            super::tips_inside_members(&struts, &arms, &layers, PLATE)
        };
        let inside = field(0.5, false);
        assert_eq!(inside.len(), 1, "{inside:?}");
        assert_eq!((inside[0].tip_member, inside[0].host_member), (1, 0));
        assert_eq!(inside[0].host_kind, StrutKind::Arm);
        // The ball centre sits a little down the leaned axis from the seat, so
        // a little nearer the column's axis than the seat's 0.5 mm.
        assert!(inside[0].depth_mm > 0.09 && inside[0].depth_mm < 0.2, "inside by about 0.1 mm: {inside:?}");
        assert!(inside[0].ball_radius_mm > 0.1, "the drawn ball is wider than the tip");
        let touching = field(0.7, false);
        assert!(touching.is_empty(), "the ball's surface overlaps, its centre is outside: {touching:?}");
        let joined = field(0.5, true);
        assert!(joined.is_empty(), "a member landed on the tip's own arm is joined by plan: {joined:?}");
    }

    /// A straight member between two 3D points, as both the plan and the
    /// lowering hold it (`member` with its two ends overridden).
    fn slanted(
        id: u32,
        kind: StrutKind,
        landing: Landing,
        base: [f64; 3],
        top: [f64; 3],
        r: f64,
    ) -> (SweepStrut, SweepArm) {
        let (strut, mut arm) = member(id, kind, landing, [base[0], base[1]], base[2], top[2], r);
        arm.base_mm = base;
        arm.seat_mm = top;
        (strut, arm)
    }

    fn collisions_of(members: Vec<(SweepStrut, SweepArm)>) -> Vec<super::MemberCollision> {
        let (struts, arms): (Vec<SweepStrut>, Vec<SweepArm>) = members.into_iter().unzip();
        let layers = plinth_core::LayerPlan::new(PLATE, vec![H; 400]).expect("a uniform plan");
        super::member_collisions(&struts, &arms, &layers, PLATE, 0.05)
    }

    /// A seated tip: a straight arm of radius `r` seated into a flat
    /// down-facing face at `top_z`, its shaft running to `base_z` below.
    fn seated_tip(id: u32, xy: [f64; 2], top_z: f64, base_z: f64, r: f64) -> (SweepStrut, SweepArm) {
        let (mut strut, mut arm) = member(id, StrutKind::Arm, Landing::Model { layer: 20 }, xy, base_z, top_z, r);
        arm.seats_into_model = true;
        arm.seat_normal_mm = [0.0, 0.0, -1.0];
        arm.seat_face_point_mm = Some(arm.seat_mm);
        strut.seat_normal = [0.0, 0.0, -1.0];
        (strut, arm)
    }

    /// A bare piece for the solver tests.
    fn piece(a: [f64; 3], b: [f64; 3], ra: f64, rb: f64) -> super::Piece {
        super::Piece {
            member: 0,
            kind: super::PieceKind::Shaft,
            a,
            b,
            ra,
            rb,
        }
    }

    /// **G3 is sound for tapers** (astra round 2, finding 1;
    /// `member_clearance.rs` module doc: the radii at the closest AXIS points
    /// are not the closest solids when the radius varies). A 10 mm post stands
    /// from 0.05 mm radius at z = 0 to 0.60 mm at z = 10. A 0.1 mm rod runs
    /// from (0.25, 0, 0), 0.25 from the post's axis beside its THIN end, to
    /// (0.60, 0, 10), exactly the post's thick radius off the axis plus nothing
    /// to spare. The axes are nearest at z = 0, where 0.05 + 0.1 < 0.25: the
    /// closest-axis rule says clear. At z = 10 the post is 0.60 wide and the
    /// rod's surface reaches 0.50 from the axis: they overlap by 0.10 mm. The
    /// solver finds that end; a brute-force grid agrees with it.
    #[test]
    fn a_tapered_thick_end_collides_away_from_the_closest_axis_points() {
        let post = piece([0.0, 0.0, 0.0], [0.0, 0.0, 10.0], 0.05, 0.60);
        let rod = piece([0.25, 0.0, 0.0], [0.60, 0.0, 10.0], 0.1, 0.1);
        let (axes, on_post, on_rod) =
            geom_kernel::segment_segment_distance_min(post.a, post.b, rod.a, rod.b);
        let at_nearest = post.radius(on_post[2] / 10.0) + rod.radius(on_rod[2] / 10.0) - axes;
        assert!(at_nearest < 0.0, "the closest-axis rule sees a gap: {at_nearest}");
        let (gap, s, t) = super::piece_gap(&post, &rod, (0.0, 1.0), (0.0, 1.0));
        // The swept sphere is a hair wider than the frustum at a slope (a
        // 0.0004 mm bulge here): an enclosure, never a miss.
        assert!((gap + 0.10).abs() < 1e-3, "the thick end overlaps by 0.10 mm: {gap}");
        assert!(s > 0.99 && t > 0.99, "at the thick ends: ({s}, {t})");
        // The brute-force grid never beats the solver (convex, so the grid's
        // best is an upper bound on the minimum) and comes within a hair of it.
        let mut brute = f64::INFINITY;
        for i in 0..=200 {
            for j in 0..=200 {
                let (s, t) = (f64::from(i) / 200.0, f64::from(j) / 200.0);
                let (p, q) = (post.at(s), rod.at(t));
                brute = brute.min(geom_kernel::vec3::dist(p, q) - post.radius(s) - rod.radius(t));
            }
        }
        assert!(gap <= brute + 1e-12 && brute - gap < 1e-3, "solver {gap} vs grid {brute}");
        // The same rod ending 0.15 mm further out clears the thick end.
        let clear = piece([0.25, 0.0, 0.0], [0.75, 0.0, 10.0], 0.1, 0.1);
        let (gap, _, _) = super::piece_gap(&post, &clear, (0.0, 1.0), (0.0, 1.0));
        assert!(gap > 0.0, "no overlap: {gap}");
    }

    /// The same defect through the census, on members as the sweep holds them:
    /// a 5 mm post drawn 0.05 mm wide at its top (z = -1) and 0.60 mm at its
    /// root (z = -6), and a 0.1 mm brace from (0.25, 0, -1), beside the thin
    /// top, to (0.60, 0, -6), at the thick root's radius. The axes are nearest
    /// at the top, where 0.05 + 0.1 < 0.25; the drawn solids overlap by about
    /// 0.1 mm low on the post. SABOTAGE: reading the radii at the closest axis
    /// points in `member_collisions` (the retired rule) turns this red.
    #[test]
    fn a_tapered_post_and_a_brace_collide_low_on_the_post_in_the_census() {
        let field = |brace_end_x: f64| {
            let (mut post, mut post_arm) =
                slanted(0, StrutKind::Arm, Landing::Model { layer: 20 }, [0.0, 0.0, -6.0], [0.0, 0.0, -1.0], 0.05);
            let n = post_arm.radius_profile_mm.len();
            let profile: Vec<f64> = (0..n)
                .map(|slot| 0.60 - 0.55 * (slot as f64 + 0.5) / n as f64)
                .collect();
            post_arm.radius_profile_mm.clone_from(&profile);
            post.radius_mm = profile;
            let brace = slanted(
                1,
                StrutKind::Brace,
                Landing::Model { layer: 20 },
                [brace_end_x, 0.0, -6.0],
                [0.25, 0.0, -1.0],
                0.1,
            );
            collisions_of(vec![(post, post_arm), brace])
        };
        let hit = field(0.60);
        assert_eq!(hit.len(), 1, "{hit:?}");
        // About 0.1 mm at the root's radius; the drawn spans stand on or above
        // the profile, a little wider than the line through it.
        assert!(hit[0].depth_mm > 0.05 && hit[0].depth_mm < 0.25, "{hit:?}");
        let clear = field(0.90);
        assert!(clear.is_empty(), "a brace 0.3 mm further out clears the root: {clear:?}");
    }

    /// The solver against a brute-force grid on a few tilted tapered pairs,
    /// radii growing and shrinking: the minimum is the grid's to a hair.
    #[test]
    fn the_piece_gap_solver_matches_a_brute_force_grid() {
        let cases = [
            (
                piece([0.0, 0.0, 0.0], [1.0, 0.5, 4.0], 0.3, 0.1),
                piece([0.9, -1.0, 1.0], [0.2, 1.0, 3.0], 0.05, 0.4),
            ),
            (
                piece([0.0, 0.0, 0.0], [0.0, 0.0, 3.0], 0.1, 0.9),
                piece([0.5, 0.5, 2.5], [-2.0, 0.0, 0.5], 0.2, 0.2),
            ),
            (
                piece([0.0, 0.0, 0.0], [3.0, 0.0, 0.0], 0.2, 0.2),
                piece([1.0, 0.6, 0.0], [1.0, 0.6, 0.0], 0.3, 0.3),
            ),
        ];
        for (p, q) in cases {
            let (gap, _, _) = super::piece_gap(&p, &q, (0.0, 1.0), (0.0, 1.0));
            let mut brute = f64::INFINITY;
            for i in 0..=300 {
                for j in 0..=300 {
                    let (s, t) = (f64::from(i) / 300.0, f64::from(j) / 300.0);
                    brute = brute.min(
                        geom_kernel::vec3::dist(p.at(s), q.at(if q.is_sphere() { 0.0 } else { t }))
                            - p.radius(s)
                            - q.radius(t),
                    );
                }
            }
            assert!(gap <= brute + 1e-9 && brute - gap < 5e-3, "solver {gap} vs grid {brute}");
        }
    }

    /// **The two diagonals of an `|X|` are joined where they cross.** Posts 0
    /// and 1 stand 2 mm apart; brace 2 lands on post 1 and seats on post 0,
    /// brace 3 the reverse, so they cross at mid-height with 0.2 mm radii. The
    /// pair is not a collision. The same two braces landed on the model
    /// instead (nothing joins them) are: the crossing is excused by the plan
    /// and by nothing else. (Cylinder `149-150`, `151-152`, `191-192` on the
    /// first run of this census were exactly this.)
    #[test]
    fn the_two_diagonals_of_an_x_are_joined_where_they_cross() {
        let field = |joined: bool| {
            let post = |id: u32, x: f64| {
                slanted(id, StrutKind::Arm, Landing::Model { layer: 20 }, [x, 0.0, -6.0], [x, 0.0, -1.0], 0.3)
            };
            let diagonal = |id: u32, from: [f64; 3], to: [f64; 3], lands: u32, seats: u32| {
                let landing = if joined { Landing::Strut { id: lands, layer: 40 } } else { Landing::Model { layer: 40 } };
                let (mut strut, arm) = slanted(id, StrutKind::Brace, landing, from, to, 0.2);
                strut.seat_on_strut = joined.then_some(seats);
                (strut, arm)
            };
            collisions_of(vec![
                post(0, 0.0),
                post(1, 2.0),
                diagonal(2, [2.0, 0.0, -5.0], [0.0, 0.0, -2.0], 1, 0),
                diagonal(3, [0.0, 0.0, -5.0], [2.0, 0.0, -2.0], 0, 1),
            ])
        };
        let joined = field(true);
        assert!(joined.iter().all(|c| (c.low, c.high) != (2, 3)), "the X crossing is a joint: {joined:?}");
        let unjoined = field(false);
        assert!(unjoined.iter().any(|c| (c.low, c.high) == (2, 3)), "anti-vacuity: {unjoined:?}");
    }

    /// **A cluster meets at its joints**: a grandchild lands on a child that
    /// lands on a trunk, and the grandchild's end stands inside the trunk's
    /// solid where the three meet (Cylinder 85 / 88 / 113). Its overlap with
    /// the trunk is the cluster's joint and is not a collision; the same
    /// grandchild landed on the plate (joined to nothing) overlaps the trunk
    /// and IS.
    #[test]
    fn a_grandchild_ending_inside_its_grandparent_at_the_cluster_joint_is_not_a_collision() {
        let field = |related: bool| {
            let trunk = slanted(0, StrutKind::Arm, Landing::Model { layer: 20 }, [0.0, 0.0, -6.0], [0.0, 0.0, -1.0], 0.5);
            let child = slanted(1, StrutKind::Arm, Landing::Strut { id: 0, layer: 40 }, [0.45, 0.0, -4.0], [3.0, 0.0, -1.5], 0.3);
            let grand_landing = if related { Landing::Strut { id: 1, layer: 40 } } else { Landing::Model { layer: 40 } };
            let grandchild = slanted(2, StrutKind::Arm, grand_landing, [0.2, 0.0, -3.9], [1.5, 0.0, -1.0], 0.2);
            collisions_of(vec![trunk, child, grandchild])
        };
        let related = field(true);
        assert!(related.iter().all(|c| (c.low, c.high) != (0, 2)), "the cluster's joint: {related:?}");
        let stranger = field(false);
        assert!(stranger.iter().any(|c| (c.low, c.high) == (0, 2)), "anti-vacuity: {stranger:?}");
    }

    /// **A tip BALL collides on its own** (the inventory, not the shaft): a
    /// 0.2 mm tip's contact ball is 0.25 mm (the owner's ratio), wider than the
    /// 0.2 mm post under it, and a 0.05 mm rod 0.27 mm beside the tip clears the
    /// post (0.25 needed) and stands inside the ball (0.30 needed). The deepest
    /// piece pair is the ball against the rod. The rod stood 0.45 mm off clears
    /// both.
    #[test]
    fn a_tip_ball_collides_though_the_post_under_it_is_clear() {
        let field = |offset: f64| {
            collisions_of(vec![
                seated_tip(0, [0.0, 0.0], -1.0, -4.0, 0.2),
                member(1, StrutKind::Mast, Landing::Plate, [offset, 0.0], PLATE, -0.8, 0.05),
            ])
        };
        let near = field(0.27);
        assert_eq!(near.len(), 1, "{near:?}");
        assert_eq!((near[0].low_piece, near[0].high_piece), (super::PieceKind::Ball, super::PieceKind::Shaft));
        assert!(near[0].depth_mm > 0.0 && near[0].depth_mm < 0.3, "{near:?}");
        let far = field(0.45);
        assert!(far.is_empty(), "{far:?}");
    }

    /// **A head collides on its own**: a headed arm's post starts at the joint
    /// `K` (z = -1.7); the head's neck runs from the contact ball (z = -1.15)
    /// down to it, 0.07 mm wide at the ball and 0.22 mm at `K`. A 0.05 mm rod
    /// standing 0.18 mm off the axis between z = -1.45 and z = -0.9 clears the
    /// post, the joint sphere and the ball (all further than their radii) and
    /// stands inside the neck. The post-only inventory sees nothing.
    #[test]
    fn a_head_collides_though_the_post_under_it_is_clear() {
        let (mut strut, mut arm) = seated_tip(0, [0.0, 0.0], -1.0, -4.0, 0.12);
        strut.head = Some(crate::heads::Head {
            contact_mm: [0.0, 0.0, -1.0],
            unit: [0.0, 0.0, -1.0],
            joint_mm: [0.0, 0.0, -1.7],
            k_joint: 4,
            joint_cell: 0,
            joint_radius_mm: 0.22,
            planned_tip_radius_mm: 0.12,
            gap_mm: GAP,
            cos_lean_max: 0.5373,
            cells: Vec::new(),
            ball_allowance_mm: 0.0,
            link_mm: 0.0,
            drawn_straight: false,
            law_planned: crate::heads::HeadLaw::envelope(0.5373),
            law_drawn: None,
            ledger_radius_mm: Vec::new(),
            certified: None,
            old_law: None,
        });
        arm.head = Some(plinth_support::SweepHead {
            contact_mm: [0.0, 0.0, -1.0],
            unit: [0.0, 0.0, -1.0],
            joint_mm: [0.0, 0.0, -1.7],
            joint_radius_mm: 0.22,
            gap_mm: GAP,
            profile_knots_mm: vec![[0.0, 0.07], [0.7, 0.22]],
            link_mm: 0.0,
            link_radius_mm: 0.0,
            certified: false,
        });
        let rod = |offset: f64| {
            member(1, StrutKind::Mast, Landing::Model { layer: 20 }, [offset, 0.0], -1.45, -0.9, 0.05)
        };
        let near = collisions_of(vec![(strut.clone(), arm.clone()), rod(0.18)]);
        assert_eq!(near.len(), 1, "{near:?}");
        assert_ne!(near[0].low_piece, super::PieceKind::Shaft, "the head's pieces, not the post: {near:?}");
        let far = collisions_of(vec![(strut, arm), rod(0.40)]);
        assert!(far.is_empty(), "{far:?}");
    }

    /// **A plate root's boss collides on its own**: two 0.2 mm posts 0.45 mm
    /// apart have shafts that clear (0.40 needed), and their bosses, 1.47 times
    /// the root at the foot, overlap near the plate.
    #[test]
    fn a_root_boss_collides_though_the_shafts_are_clear() {
        let post = |id: u32, x: f64| member(id, StrutKind::Arm, Landing::Plate, [x, 0.0], PLATE, -1.0, 0.2);
        let close = collisions_of(vec![post(0, 0.0), post(1, 0.45)]);
        assert_eq!(close.len(), 1, "{close:?}");
        assert_eq!((close[0].low_piece, close[0].high_piece), (super::PieceKind::Boss, super::PieceKind::Boss));
        let apart = collisions_of(vec![post(0, 0.0), post(1, 0.75)]);
        assert!(apart.is_empty(), "{apart:?}");
    }

    /// **G3 sees a mid-shaft crossing that `tips_inside_members` cannot**
    /// (SC-DESIGN §4, astra finding 4). A straight arm seated into the model
    /// at z = -1 stands in the x = 0 plane; a brace runs from (-1, 0, -3.5) to
    /// (1, 0, -1.5) and passes THROUGH the arm's shaft at z = -2.5, nowhere
    /// near the arm's tip. `tips_inside_members` reads only the ball-to-post-
    /// top segment against the other shafts and names nothing (the documented
    /// hole); `member_collisions` names the pair, 0.35 mm deep (0.2 + 0.15
    /// radii on a zero axis distance). The same brace moved 1 mm out of the
    /// arm's plane is not a collision.
    #[test]
    fn a_straight_arm_crossing_a_brace_mid_shaft_is_a_collision_the_tip_census_misses() {
        let layers = plinth_core::LayerPlan::new(PLATE, vec![H; 400]).expect("a uniform plan");
        let field = |brace_y: f64| {
            let (mut strut, mut arm) = slanted(
                0,
                StrutKind::Arm,
                Landing::Model { layer: 20 },
                [0.0, 0.0, -4.0],
                [0.0, 0.0, -1.0],
                0.2,
            );
            arm.seats_into_model = true;
            arm.seat_normal_mm = [0.0, 0.0, -1.0];
            arm.seat_face_point_mm = Some(arm.seat_mm);
            strut.seat_normal = [0.0, 0.0, -1.0];
            let brace = slanted(
                1,
                StrutKind::Brace,
                Landing::Model { layer: 20 },
                [-1.0, brace_y, -3.5],
                [1.0, brace_y, -1.5],
                0.15,
            );
            [(strut, arm), brace]
        };
        let (struts, arms): (Vec<SweepStrut>, Vec<SweepArm>) = field(0.0).into_iter().unzip();
        assert!(
            super::tips_inside_members(&struts, &arms, &layers, PLATE).is_empty(),
            "the hole: the tip census does not see a mid-shaft crossing"
        );
        let crossing = collisions_of(field(0.0).into());
        assert_eq!(crossing.len(), 1, "{crossing:?}");
        assert_eq!((crossing[0].low, crossing[0].high), (0, 1));
        assert!((crossing[0].depth_mm - 0.35).abs() < 1e-6, "{crossing:?}");
        let clear = collisions_of(field(1.0).into());
        assert!(clear.is_empty(), "a brace 1 mm out of the plane clears the arm: {clear:?}");
    }

    /// **A planned joint excuses the pieces near it, not the whole member.** A
    /// 0.3 mm host stands at x = 0 from z = -4 to 4. A 0.2 mm lander landing
    /// on it, with its foot ON the host's axis at z = -1 and climbing at 45
    /// degrees, overlaps the host only in the lens at its joint: not a
    /// collision. A lander whose foot is 0.35 mm off the axis at the same
    /// height and that climbs steeply back across the axis 3 mm above its
    /// joint, at 6.7 degrees (well past the joint's zone), IS: it is landed
    /// on the host by plan, and still drawn through it.
    #[test]
    fn a_lander_at_its_joint_is_clear_and_the_same_lander_crossing_its_host_above_the_joint_is_not() {
        let host = || {
            slanted(0, StrutKind::Arm, Landing::Model { layer: 20 }, [0.0, 0.0, -4.0], [0.0, 0.0, 4.0], 0.3)
        };
        let lander = |foot: [f64; 3], top: [f64; 3]| {
            slanted(1, StrutKind::Arm, Landing::Strut { id: 0, layer: 80 }, foot, top, 0.2)
        };
        let at_joint = collisions_of(vec![host(), lander([0.0, 0.0, -1.0], [3.0, 0.0, 2.0])]);
        assert!(at_joint.is_empty(), "a lander at its joint is excused: {at_joint:?}");
        let crossing = collisions_of(vec![host(), lander([0.35, 0.0, -1.0], [-0.0, 0.0, 2.0])]);
        assert_eq!(crossing.len(), 1, "a lander drawn through its host above the joint: {crossing:?}");
        assert_eq!((crossing[0].low, crossing[0].high), (0, 1));
        // The control that keeps the crossing from being the host's own
        // footprint: the same lander NOT landed on the host (no planned joint)
        // is a collision at its joint too.
        let unplanned = collisions_of(vec![
            host(),
            slanted(1, StrutKind::Arm, Landing::Model { layer: 20 }, [0.0, 0.0, -1.0], [3.0, 0.0, 2.0], 0.2),
        ]);
        assert_eq!(unplanned.len(), 1, "no planned joint, no excuse: {unplanned:?}");
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
