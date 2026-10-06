//! THE FOOTPRINT NORMAL: the seat normal of a support tip is the AREA-WEIGHTED
//! AVERAGE of the model's outward normal over the patch the tip ball actually
//! touches, never the normal of one facet (Craig ruling 2026-10-05).
//!
//! # The ruling, and the defect it removes
//!
//! Until this module every path read ONE facet: the nearest facet the
//! seat-face rule admits ([`crate::tip_landing::tip_seat_faces`]). Its unit
//! normal became the seat law's `abs_cos`, the plane the emitter trims the
//! ball to, the contact disc and the lip-seat approach angle. That facet is a
//! tessellation accident: cut the same surface into triangles another way and
//! the nearest facet, so the seat depth and the trim plane, change. Measured
//! on the round-16 files (`field-2026-09-28/seven-tip-design.md` section 1)
//! the single facet's normal differs from the footprint's by up to 38.6° on
//! the Morel base (p90 5.3°), 14.1° on the Sphere and 8.8° on the Cylinder, and
//! by 0 on the Round discs (their footprint is one flat patch).
//!
//! # What is measured
//!
//! For every support arm, with its tip ball of radius `R` centred at `C`
//! (the ball the lowering draws, [`plinth_support::tip_contact_discs`]):
//!
//! ```text
//! footprint   = ball ∩ model surface
//!             restricted to the faces the seat-face rule admits,
//!             and connected to the SEAT TRIANGLE through such faces,
//!             across edges that themselves enter the ball
//! area(f)     = exact area of triangle f inside the ball
//!             = area of triangle f ∩ the disc the ball cuts in f's plane,
//!               radius sqrt(R² - d²), centred on the projection of C
//! normal      = normalize( Σ area(f) · n(f) )
//! ```
//!
//! * **Exact weights.** [`triangle_disc_area`] is a closed form (a sum over
//!   the three edges of the signed area of the disc inside the triangle fan
//!   from the disc's centre). It uses selects only, no data-dependent branch.
//!   A patch cut into triangles in different ways has the same total area,
//!   and a triangle's weight does not depend on its size, so the average is
//!   tessellation-invariant (to rounding).
//! * **Admitted faces only.** A wall or a face steeper than the registered
//!   overhang band is never part of a footprint: a tip never seats on one
//!   (ruling 2026-09-30). The band's steep end and the lean law's steep end
//!   are the two ends of one registered band, so any average of admitted
//!   normals is inside the printable cone and needs no clamp.
//! * **Connected through the ball.** The far side of a thin wall, or a second
//!   sheet the ball overlaps, is not part of the patch the tip touches. Two
//!   admitted triangles are joined when they share a mesh edge AND that edge
//!   enters the ball: the footprint is the connected component of
//!   `ball ∩ surface`, and a shared edge wholly outside the ball joins nothing
//!   there. That is what makes the patch independent of the tessellation:
//!   cut a triangle along a line and the new edges lie in the old triangle,
//!   so a join through them is a join through the same point set. (A first
//!   cut joined triangles on a shared edge wherever it lay; a ridge edge
//!   outside the ball then joined two sides that a finer cut of the same
//!   surface left apart.)
//! * **Seeded at the seat triangle.** The walk starts at the seat triangle the
//!   seat-face measurement chose ([`crate::tip_landing::tip_seat_faces_with_ids`]),
//!   by identity. Where that triangle's patch does not reach the ball (the ball
//!   overlaps only some other, disconnected sheet) there is NO footprint and
//!   the tip keeps the seat triangle's own normal; the walk never re-seeds on
//!   another sheet.
//! * **No area, no change.** Where the covered area is nil (no admitted face
//!   in the ball, or the ball could not be built) the tip keeps its seat
//!   triangle's normal, and the count says so (emit and warn, never refuse).
//! * **Flat faces are untouched.** Where the average equals the seat
//!   triangle's normal to rounding the seat triangle's own bits are kept, so
//!   a flat underside (every Round disc) is byte-identical.
//!
//! # The normal describes the geometry the file gets
//!
//! The ball a tip is drawn with depends on the normal (the seat law reads it),
//! and the footprint grade and the seat moves ([`crate::footprint`]) read the
//! ball. So the measurement is made on the arm as it is drawn, three times:
//!
//! 1. **Before the moves** ([`refine`]): the arm as planned, for the lip-seat
//!    survey, the head-join measurement and the move search.
//! 2. **Inside the move validation** ([`crate::footprint::apply`]): each
//!    proposed slide is re-measured at the MOVED ball (the seat triangle
//!    re-derived at the moved seat, [`reseat_sites`]), and the disc that is
//!    graded against the face, the section test and the lowering are all
//!    built from that normal. A move is accepted on the geometry it will have.
//! 3. **On the emitted field** ([`remeasure_emitted`]): the arms exactly as the
//!    adapter hands them to the lowering (seat slides, followers' ends), the
//!    normal measured once more, and every tip whose normal changed has its
//!    disc graded again ([`crate::footprint::regrade`]).
//!
//! # Batch design
//!
//! * One flat CSR array of `(tip, face)` pairs per pass, made by ONE
//!   [`geom_kernel::NativeSurface::faces_near_segments_batch_classified`] call
//!   (a degenerate segment at the ball centre, `within = R`) over the surface
//!   the head-join rule already built. Only where that surface is absent (its
//!   memory cap) does the pass scan the faces itself, the same
//!   `tips × faces` shape the seat-face measurement has always had
//!   ([`footprint_pairs`]).
//! * One parallel pass over tips ([`footprint_normals`]): rayon over rows,
//!   each row a segmented sum, no atomics, no shared mutable state.
//! * Two passes per measurement ([`measure_arms`]): the ball depends on the
//!   seat depth, which depends on the normal. Seat along the starting normal,
//!   measure the footprint normal, re-seat along it, measure again. The
//!   largest angle between the two answers is published
//!   ([`FootprintNormalOutcome`]).
//! * Ties are left alone: a model tie may seat on a wall (decision D8), so its
//!   "admitted faces" are every face.
//! * The orientation prefilter never runs it
//!   ([`crate::SweepPolicy::without_footprint_normal`]).

use geom_kernel::{closest_on_tri, vec3, NativeSurface, NearFaceBatchLimits};
use plinth_core::LayerPlan;
use plinth_mesh::Mesh;
use plinth_support::SweepArm;
use rayon::prelude::*;

use crate::grid::Grid;
use crate::tip_landing::{CarriedFace, SeatFaceRule};
use crate::{Footprint, StrutKind, SweepStrut};

/// How far two unit normals may differ, per component, and still be the same
/// normal: a few ULPs of `f64`. A footprint average that lands inside this of
/// the seat triangle's own normal IS that normal, and its bits are kept, so a
/// flat face is not perturbed by the rounding of a sum. Numerical noise only;
/// it decides nothing about geometry.
const SAME_NORMAL_ULPS: f64 = 8.0;

/// Signed area of `disc(0, rho) ∩ triangle(0, a, b)` in plane coordinates.
///
/// Branch-free: the segment's overlap with the disc is the parameter interval
/// `[t1, t2]` clamped to the segment, and an empty or missing overlap collapses
/// the interval to a point on the segment, so the three terms (sector, chord
/// triangle, sector) sum to the plain sector either way.
fn disc_fan_area(a: [f64; 2], b: [f64; 2], rho: f64) -> f64 {
    let cross = |u: [f64; 2], v: [f64; 2]| u[0] * v[1] - u[1] * v[0];
    let dot = |u: [f64; 2], v: [f64; 2]| u[0] * v[0] + u[1] * v[1];
    let sector = |u: [f64; 2], v: [f64; 2]| 0.5 * rho * rho * cross(u, v).atan2(dot(u, v));
    let d = [b[0] - a[0], b[1] - a[1]];
    let a_len2 = dot(d, d);
    // A zero-length edge contributes nothing: with `a_len2` taken as one and
    // `d = 0`, both roots are 0 and the sectors of `a → a` vanish.
    let safe = if a_len2 > 0.0 { a_len2 } else { 1.0 };
    let half_b = dot(a, d);
    let c = dot(a, a) - rho * rho;
    #[expect(
        clippy::suspicious_operation_groupings,
        reason = "the quarter discriminant (b/2)^2 - a*c of a*t^2 + b*t + c, with a = `safe`: \
                  `half_b * half_b` is meant, not `safe * half_b`"
    )]
    let discriminant = (half_b * half_b - safe * c).max(0.0);
    let root = discriminant.sqrt();
    let t1 = ((-half_b - root) / safe).clamp(0.0, 1.0);
    let t2 = ((-half_b + root) / safe).clamp(0.0, 1.0);
    let p1 = [a[0] + t1 * d[0], a[1] + t1 * d[1]];
    let p2 = [a[0] + t2 * d[0], a[1] + t2 * d[1]];
    sector(a, p1) + 0.5 * cross(p1, p2) + sector(p2, b)
}

/// The exact area of `triangle ∩ ball`, in mm².
///
/// The ball cuts the triangle's plane in a disc of radius `sqrt(R² − d²)`
/// centred on the projection of the ball's centre onto that plane (`d` = the
/// centre's distance from the plane); the area is that disc's overlap with the
/// triangle, in closed form ([`disc_fan_area`] over the three edges).
///
/// `0` for a degenerate triangle, a non-finite input, or a ball that does not
/// reach the plane.
#[must_use]
pub fn triangle_disc_area(triangle: [[f64; 3]; 3], centre_mm: [f64; 3], radius_mm: f64) -> f64 {
    let [a, b, c] = triangle;
    let e1 = vec3::sub(b, a);
    let normal = vec3::cross(e1, vec3::sub(c, a));
    let length = vec3::norm(normal);
    let e1_length = vec3::norm(e1);
    let usable = length > 0.0 && e1_length > 0.0 && length.is_finite() && radius_mm > 0.0;
    if !usable {
        return 0.0;
    }
    let unit = vec3::scale(normal, 1.0 / length);
    let offset = vec3::dot(vec3::sub(centre_mm, a), unit);
    let rho_squared = radius_mm * radius_mm - offset * offset;
    let reaches_plane = rho_squared > 0.0;
    if !reaches_plane {
        return 0.0;
    }
    let rho = rho_squared.sqrt();
    let foot = vec3::sub(centre_mm, vec3::scale(unit, offset));
    let ux = vec3::scale(e1, 1.0 / e1_length);
    let uy = vec3::cross(unit, ux);
    let plane = |p: [f64; 3]| {
        let w = vec3::sub(p, foot);
        [vec3::dot(w, ux), vec3::dot(w, uy)]
    };
    let [pa, pb, pc] = [plane(a), plane(b), plane(c)];
    let signed = disc_fan_area(pa, pb, rho) + disc_fan_area(pb, pc, rho) + disc_fan_area(pc, pa, rho);
    signed.abs()
}

/// The faces within each ball, as one flat CSR array: row `i` is
/// `faces[offsets[i]..offsets[i + 1]]`, face ids ascending.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FootprintPairs {
    pub offsets: Vec<usize>,
    pub faces: Vec<u32>,
}

impl FootprintPairs {
    /// Row `i`'s faces.
    #[must_use]
    pub fn row(&self, i: usize) -> &[u32] {
        match (self.offsets.get(i), self.offsets.get(i + 1)) {
            (Some(&lo), Some(&hi)) => self.faces.get(lo..hi).unwrap_or(&[]),
            _ => &[],
        }
    }

    /// Rows held.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }
}

/// Every face within `radii_mm[i]` of `centres_mm[i]`, for every ball, as CSR.
///
/// With a `surface` it is ONE
/// [`NativeSurface::faces_near_segments_batch_classified`] call (a degenerate
/// segment at the centre). Without one (the surface index's memory cap, or a
/// refused query) it scans `balls × faces` in parallel, with a bounding-box
/// reject before the exact closest-point test. `None` only when the surface
/// refused AND the inputs are unusable (a length mismatch).
#[must_use]
pub fn footprint_pairs(
    mesh: &Mesh,
    surface: Option<&NativeSurface>,
    centres_mm: &[[f64; 3]],
    radii_mm: &[f64],
) -> Option<FootprintPairs> {
    if centres_mm.len() != radii_mm.len() {
        return None;
    }
    let rows: Vec<Vec<u32>> = surface
        .and_then(|surface| {
            let limits = NearFaceBatchLimits {
                max_rows: centres_mm.len(),
                max_faces_per_row: surface.nfaces(),
                max_total_faces: usize::MAX,
            };
            surface
                .faces_near_segments_batch_classified(centres_mm, centres_mm, radii_mm, limits)
                .ok()
                .map(|found| found.rows)
        })
        .unwrap_or_else(|| scan_rows(mesh, centres_mm, radii_mm));
    let mut offsets = Vec::with_capacity(rows.len() + 1);
    let mut faces = Vec::with_capacity(rows.iter().map(Vec::len).sum());
    offsets.push(0);
    for row in rows {
        faces.extend(row);
        offsets.push(faces.len());
    }
    Some(FootprintPairs { offsets, faces })
}

/// The scan fallback of [`footprint_pairs`]: one parallel pass, every ball
/// against every face.
fn scan_rows(mesh: &Mesh, centres_mm: &[[f64; 3]], radii_mm: &[f64]) -> Vec<Vec<u32>> {
    let verts = mesh.verts();
    let tris = mesh.tris();
    centres_mm
        .par_iter()
        .zip(radii_mm.par_iter())
        .map(|(&centre, &radius)| {
            let mut row = Vec::new();
            for (id, tri) in tris.iter().enumerate() {
                let (Some(&a), Some(&b), Some(&c)) = (
                    verts.get(tri[0] as usize),
                    verts.get(tri[1] as usize),
                    verts.get(tri[2] as usize),
                ) else {
                    continue;
                };
                let outside_box = (0..3).any(|axis| {
                    let lo = a[axis].min(b[axis]).min(c[axis]);
                    let hi = a[axis].max(b[axis]).max(c[axis]);
                    centre[axis] + radius < lo || centre[axis] - radius > hi
                });
                if outside_box {
                    continue;
                }
                let closest = closest_on_tri(centre, a, b, c);
                if vec3::dist(centre, closest) <= radius {
                    if let Ok(id) = u32::try_from(id) {
                        row.push(id);
                    }
                }
            }
            row
        })
        .collect()
}

/// One tip's question to [`footprint_normals`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootprintBall {
    /// The tip ball's centre and radius.
    pub centre_mm: [f64; 3],
    pub radius_mm: f64,
    /// The SEAT TRIANGLE's mesh face id: the connected patch is the one that
    /// contains it. Where that triangle is not in the ball (or not admitted)
    /// the answer is "no footprint".
    pub seat_face: u32,
    /// The seat triangle's own outward unit normal: the answer where the
    /// footprint has no area, and the bits kept where the average equals it.
    pub facet_normal: [f64; 3],
    /// A face is admitted when its outward unit normal's `z` is below this
    /// ([`SeatFaceRule::max_normal_z`]).
    pub max_normal_z: f64,
}

/// What [`footprint_normals`] found for one tip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootprintNormal {
    /// The area-weighted outward unit normal.
    pub normal: [f64; 3],
    /// The footprint's area, mm².
    pub area_mm2: f64,
    /// Faces in the connected, admitted footprint.
    pub faces: usize,
}

/// The footprint normal of every ball, in ball order: `None` where the
/// footprint has no area (the caller keeps the seat triangle's normal).
///
/// One parallel pass over rows; each row is a segmented sum with no shared
/// state. `pairs` is [`footprint_pairs`]'s answer for the same balls.
#[must_use]
pub fn footprint_normals(
    mesh: &Mesh,
    pairs: &FootprintPairs,
    balls: &[FootprintBall],
) -> Vec<Option<FootprintNormal>> {
    let verts = mesh.verts();
    let tris = mesh.tris();
    balls
        .par_iter()
        .enumerate()
        .map(|(index, ball)| footprint_of_row(verts, tris, pairs.row(index), ball))
        .collect()
}

/// A face of a row: its mesh id, vertex ids, outward unit normal and clipped
/// area.
struct RowFace {
    face: u32,
    ids: [u32; 3],
    normal: [f64; 3],
    area_mm2: f64,
}

/// The distance from `point` to the segment `a`–`b`.
fn distance_to_segment(point: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = vec3::sub(b, a);
    let length_squared = vec3::dot(d, d);
    let safe = if length_squared > 0.0 { length_squared } else { 1.0 };
    let t = (vec3::dot(vec3::sub(point, a), d) / safe).clamp(0.0, 1.0);
    vec3::dist(point, vec3::add(a, vec3::scale(d, t)))
}

fn footprint_of_row(
    verts: &[[f64; 3]],
    tris: &[[u32; 3]],
    row: &[u32],
    ball: &FootprintBall,
) -> Option<FootprintNormal> {
    let mut admitted: Vec<RowFace> = Vec::with_capacity(row.len());
    for &face in row {
        let Some(&ids) = tris.get(face as usize) else { continue };
        let (Some(&a), Some(&b), Some(&c)) = (
            verts.get(ids[0] as usize),
            verts.get(ids[1] as usize),
            verts.get(ids[2] as usize),
        ) else {
            continue;
        };
        // The very spelling `tip_landing::seat_faces` builds a facet's unit
        // normal with, so a one-facet footprint reproduces its bits.
        let Some(normal) = vec3::normalize(vec3::cross(vec3::sub(b, a), vec3::sub(c, a))) else {
            continue;
        };
        let is_admitted = normal[2] < ball.max_normal_z;
        if !is_admitted {
            continue;
        }
        admitted.push(RowFace {
            face,
            ids,
            normal,
            area_mm2: triangle_disc_area([a, b, c], ball.centre_mm, ball.radius_mm),
        });
    }
    // THE SEED IS THE SEAT TRIANGLE, by identity. A row is ascending by face id.
    // If the seat triangle is not an admitted face of the ball there is no
    // footprint: the ball may overlap another, disconnected sheet, and that
    // sheet is not what this tip seats on.
    let seed = admitted.iter().position(|face| face.face == ball.seat_face)?;
    // EDGE CONNECTIVITY through admitted faces of this row: union two faces
    // that share a mesh edge when that edge ENTERS the ball. An edge wholly
    // outside the ball joins nothing: the ball touches the two faces on its
    // two separate sides.
    let mut parent: Vec<usize> = (0..admitted.len()).collect();
    fn find(parent: &mut [usize], mut node: usize) -> usize {
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }
    let mut edges: Vec<(u32, u32, usize)> = Vec::with_capacity(admitted.len() * 3);
    for (index, face) in admitted.iter().enumerate() {
        let [a, b, c] = face.ids;
        edges.extend([(a, b), (b, c), (c, a)].map(|(p, q)| (p.min(q), p.max(q), index)));
    }
    edges.sort_unstable();
    for pair in edges.windows(2) {
        if pair[0].0 != pair[1].0 || pair[0].1 != pair[1].1 {
            continue;
        }
        let (Some(&p), Some(&q)) = (verts.get(pair[0].0 as usize), verts.get(pair[0].1 as usize))
        else {
            continue;
        };
        if distance_to_segment(ball.centre_mm, p, q) > ball.radius_mm {
            continue;
        }
        let (x, y) = (find(&mut parent, pair[0].2), find(&mut parent, pair[1].2));
        if x != y {
            parent[x.max(y)] = x.min(y);
        }
    }
    let seed_root = find(&mut parent, seed);
    let mut sum = [0.0_f64; 3];
    let mut area = 0.0_f64;
    let mut faces = 0_usize;
    for (index, face) in admitted.iter().enumerate() {
        if find(&mut parent, index) != seed_root {
            continue;
        }
        sum = vec3::add(sum, vec3::scale(face.normal, face.area_mm2));
        area += face.area_mm2;
        faces += 1;
    }
    // NO AREA, NO FOOTPRINT: a covered area at rounding level of the ball's
    // own cross-section is nothing to average.
    let has_area = area.is_finite() && area > f64::EPSILON * ball.radius_mm * ball.radius_mm;
    if !has_area {
        return None;
    }
    let mean = vec3::normalize(sum)?;
    let tolerance = SAME_NORMAL_ULPS * f64::EPSILON;
    let same_as_facet = (0..3).all(|axis| (mean[axis] - ball.facet_normal[axis]).abs() <= tolerance);
    Some(FootprintNormal {
        normal: if same_as_facet { ball.facet_normal } else { mean },
        area_mm2: area,
        faces,
    })
}

/// What the footprint pass did on one finished plan, published beside the seat
/// faces ([`crate::report::SweepRunReport::seat_footprint_normal`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FootprintNormalOutcome {
    /// Whether the pass ran at all (`SweepPolicy::footprint_normal`).
    pub enabled: bool,
    /// Support arms given a footprint normal or the seat triangle's.
    pub tips: usize,
    /// `(tip, face)` pairs of the second pass.
    pub pairs: usize,
    /// Whether the pairs came from the surface index (one batched call) or
    /// from the scan fallback.
    pub pairs_from_surface: bool,
    /// Tips whose ball could not be built (no seat plane): they keep the seat
    /// triangle's normal.
    pub fallback_no_ball: usize,
    /// Tips whose footprint had no admitted, connected area (the seat
    /// triangle's patch did not reach the ball): they keep the seat
    /// triangle's normal.
    pub fallback_no_footprint: usize,
    /// Tips whose normal is not the seat triangle's, bit for bit.
    pub moved: usize,
    /// Of those, the angle from the seat triangle's normal beyond 1, 5 and 15
    /// degrees (reporting bins only).
    pub moved_over_1_deg: usize,
    pub moved_over_5_deg: usize,
    pub moved_over_15_deg: usize,
    /// Largest and mean (over all `tips`) angle from the seat triangle's normal.
    pub max_deg: f64,
    pub mean_deg: f64,
    /// The largest angle between the first pass's normal and the second's: how
    /// far the re-seated ball moved the answer.
    pub fixed_point_residual_max_deg: f64,
    /// Most faces in any tip's connected footprint.
    pub max_faces_in_footprint: usize,
    /// Slid tips re-measured at their moved ball inside the move validation
    /// (path hits of [`crate::footprint::apply`]'s re-measure), and of those
    /// the largest angle the slide changed the normal by.
    pub slid_remeasured: usize,
    pub slid_max_change_deg: f64,
    /// THE EMITTED FIELD, measured once more ([`remeasure_emitted`]): the tips
    /// whose normal differed from the one the moves were validated with, the
    /// largest such change, and the tips whose disc, graded again on the final
    /// ball, is not wholly on its face.
    pub final_changed: usize,
    pub final_max_change_deg: f64,
    pub final_regraded: usize,
    /// `(strut index, fraction of the disc samples on the face)`.
    pub final_off_face: Vec<(usize, f64)>,
}

/// What [`measure_arms`] reads.
pub struct MeasureEnv<'a> {
    pub mesh: &'a Mesh,
    /// The model's surface index, where one was built.
    pub surface: Option<&'a NativeSurface>,
    pub layer_plan: &'a LayerPlan,
}

/// What [`refine`] reads.
pub struct FootprintEnv<'a> {
    pub mesh: &'a Mesh,
    /// The model's surface index, where one was built.
    pub surface: Option<&'a NativeSurface>,
    pub grid: &'a Grid,
    pub plate_z_mm: f64,
    pub layer_height_mm: f64,
    pub layer_plan: &'a LayerPlan,
    pub rule: SeatFaceRule,
}

impl<'a> FootprintEnv<'a> {
    fn measure_env(&self) -> MeasureEnv<'a> {
        MeasureEnv {
            mesh: self.mesh,
            surface: self.surface,
            layer_plan: self.layer_plan,
        }
    }
}

/// One support arm's seat, as the footprint measurement needs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TipSite {
    /// The seat triangle's mesh face id: the nearest face the seat-face rule
    /// admits to [`Self::landing_mm`] (moved by the tip's slide, if any).
    pub face: u32,
    /// The tip's landing point as the sweep planned it (before any slide):
    /// [`reseat_sites`] moves it by the slide to find the seat triangle there.
    pub landing_mm: [f64; 3],
    /// The seat triangle's outward unit normal.
    pub facet_normal: [f64; 3],
    /// The admission threshold ([`SeatFaceRule::max_normal_z`], or the
    /// any-down-facing one where the mesh had no admitted face).
    pub max_normal_z: f64,
}

/// One tip's measured normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measured {
    /// The normal to seat with: the footprint's, or the seat triangle's where
    /// there is no footprint or no ball.
    pub normal: [f64; 3],
    /// The ball could not be built (no seat plane) in the last pass.
    pub no_ball: bool,
    /// The last pass found no admitted, connected area (and a ball existed).
    pub no_footprint: bool,
    /// The angle between the first pass's answer and the second's.
    pub residual_deg: f64,
    /// Faces in the last pass's footprint.
    pub faces: usize,
}

/// A measurement of many tips, and what it cost.
pub struct Measurement {
    /// In arm order; `None` where the arm has no site.
    pub tips: Vec<Option<Measured>>,
    pub pairs: usize,
    pub pairs_from_surface: bool,
}

fn angle_deg(a: [f64; 3], b: [f64; 3]) -> f64 {
    vec3::dot(a, b).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Measure the footprint normal of every arm that has a site, ON THE ARM AS
/// GIVEN (its seat, base and tip radius; the first pass seats it on its seat
/// triangle's normal, [`TipSite::facet_normal`], and the `seat_normal_mm` the
/// arm carries is not read): two
/// batched passes (module doc). `None` only when the request itself was
/// unusable (no layer plan): nothing is measured.
///
/// `arms` and `sites` are index-aligned. An arm with no site, and a brace or
/// mast, is not measured.
#[must_use]
pub fn measure_arms(
    env: &MeasureEnv<'_>,
    arms: &[SweepArm],
    sites: &[Option<TipSite>],
) -> Option<Measurement> {
    let tips: Vec<usize> = (0..arms.len().min(sites.len()))
        .filter(|&index| sites[index].is_some())
        .collect();
    let mut result = Measurement {
        tips: vec![None; arms.len()],
        pairs: 0,
        pairs_from_surface: env.surface.is_some(),
    };
    if tips.is_empty() {
        return Some(result);
    }
    // The profile never reaches the disc: the seat derivation reads the seat,
    // the base, the tip radius and the normal. The FIRST pass seats on the
    // seat triangle's normal whatever the arm carries, so the answer depends
    // on the geometry alone and is the same bits wherever it is measured.
    let mut working: Vec<SweepArm> = tips
        .iter()
        .map(|&index| SweepArm {
            radius_profile_mm: Vec::new(),
            seat_normal_mm: sites[index].map_or(arms[index].seat_normal_mm, |s| s.facet_normal),
            ..arms[index].clone()
        })
        .collect();
    let mut first: Vec<Option<[f64; 3]>> = vec![None; tips.len()];
    let mut last: Vec<Option<FootprintNormal>> = vec![None; tips.len()];
    let mut no_ball = vec![false; tips.len()];
    let mut second: Vec<Option<[f64; 3]>> = vec![None; tips.len()];
    for pass in 0..2 {
        let discs = plinth_support::tip_contact_discs(&working, env.layer_plan).ok()?;
        let mut rows_of: Vec<usize> = Vec::with_capacity(tips.len());
        let mut centres: Vec<[f64; 3]> = Vec::with_capacity(tips.len());
        let mut radii: Vec<f64> = Vec::with_capacity(tips.len());
        for (position, disc) in discs.iter().enumerate() {
            match disc {
                Some(disc) => {
                    no_ball[position] = false;
                    rows_of.push(position);
                    centres.push(disc.ball_centre_mm);
                    radii.push(disc.ball_radius_mm);
                }
                None => no_ball[position] = true,
            }
        }
        let pairs = footprint_pairs(env.mesh, env.surface, &centres, &radii)?;
        result.pairs = pairs.faces.len();
        let balls: Vec<FootprintBall> = rows_of
            .iter()
            .zip(centres.iter().zip(&radii))
            .filter_map(|(&position, (&centre_mm, &radius_mm))| {
                let site = sites[tips[position]]?;
                Some(FootprintBall {
                    centre_mm,
                    radius_mm,
                    seat_face: site.face,
                    facet_normal: site.facet_normal,
                    max_normal_z: site.max_normal_z,
                })
            })
            .collect();
        let found = footprint_normals(env.mesh, &pairs, &balls);
        last.fill(None);
        for (&position, answer) in rows_of.iter().zip(&found) {
            last[position] = *answer;
            if let Some(answer) = answer {
                working[position].seat_normal_mm = answer.normal;
                let slot = if pass == 0 { &mut first } else { &mut second };
                slot[position] = Some(answer.normal);
            }
        }
    }
    for (position, &index) in tips.iter().enumerate() {
        let Some(site) = sites[index] else { continue };
        let answer = last[position];
        result.tips[index] = Some(Measured {
            normal: answer.map_or(site.facet_normal, |found| found.normal),
            no_ball: no_ball[position],
            no_footprint: answer.is_none() && !no_ball[position],
            residual_deg: match (first[position], second[position]) {
                (Some(a), Some(b)) => angle_deg(a, b),
                _ => 0.0,
            },
            faces: answer.map_or(0, |found| found.faces),
        });
    }
    Some(result)
}

/// One face's corners and outward unit normal; `None` for a face with a
/// missing vertex or no normal (zero area).
type FaceCornersNormal = Option<([[f64; 3]; 3], [f64; 3])>;

/// The nearest admitted face to each query point, as the seat-face rule picks
/// a tip's seat triangle: one parallel `points × faces` pass over the faces
/// whose outward unit normal's `z` is below the query's own threshold, lowest
/// face id on a tie. `(face id, its unit normal)`.
fn nearest_admitted(mesh: &Mesh, queries: &[([f64; 3], f64)]) -> Vec<Option<(u32, [f64; 3])>> {
    let verts = mesh.verts();
    let tris = mesh.tris();
    // One read of every face's unit normal, in the spelling `seat_faces` uses.
    let faces: Vec<FaceCornersNormal> = tris
        .iter()
        .map(|tri| {
            let a = *verts.get(tri[0] as usize)?;
            let b = *verts.get(tri[1] as usize)?;
            let c = *verts.get(tri[2] as usize)?;
            let unit = vec3::normalize(vec3::cross(vec3::sub(b, a), vec3::sub(c, a)))?;
            Some(([a, b, c], unit))
        })
        .collect();
    queries
        .par_iter()
        .map(|&(point, max_normal_z)| {
            let mut best = f64::INFINITY;
            let mut found: Option<(u32, [f64; 3])> = None;
            for (id, face) in faces.iter().enumerate() {
                let Some(([a, b, c], unit)) = face else { continue };
                if !(unit[2] < 0.0 && unit[2] < max_normal_z) {
                    continue;
                }
                let offset = vec3::sub(point, closest_on_tri(point, *a, *b, *c));
                let distance = vec3::dot(offset, offset);
                if distance < best {
                    if let Ok(id) = u32::try_from(id) {
                        best = distance;
                        found = Some((id, *unit));
                    }
                }
            }
            found
        })
        .collect()
}

/// The sites of tips that SLID, re-derived at the slid seat: the seat triangle
/// is the nearest admitted face to the planned landing point moved by the
/// slide ([`TipSite::landing_mm`] + `shifts_mm[i]`), exactly the rule that
/// chose it before the slide. A tip with no slide (`None` or a zero shift) and
/// a tip with no site keep theirs; a tip no admitted face is found for keeps
/// its old site.
///
/// One batched pass over the slid tips ([`nearest_admitted`]).
#[must_use]
#[expect(
    clippy::float_cmp,
    reason = "an exactly zero shift is a tip that did not move; any other shift, however small, \
              moved the seat, and its seat triangle is re-derived"
)]
pub fn reseat_sites(
    mesh: &Mesh,
    sites: &[Option<TipSite>],
    shifts_mm: &[Option<[f64; 3]>],
) -> Vec<Option<TipSite>> {
    let slid: Vec<usize> = (0..sites.len().min(shifts_mm.len()))
        .filter(|&index| sites[index].is_some() && shifts_mm[index].is_some_and(|s| s != [0.0; 3]))
        .collect();
    let mut out = sites.to_vec();
    if slid.is_empty() {
        return out;
    }
    let queries: Vec<([f64; 3], f64)> = slid
        .iter()
        .filter_map(|&index| {
            let site = sites[index]?;
            let shift = shifts_mm[index]?;
            Some((vec3::add(site.landing_mm, shift), site.max_normal_z))
        })
        .collect();
    for (&index, found) in slid.iter().zip(nearest_admitted(mesh, &queries)) {
        if let (Some(site), Some((face, facet_normal))) = (sites[index], found) {
            out[index] = Some(TipSite {
                face,
                facet_normal,
                ..site
            });
        }
    }
    out
}

/// Replace every support arm's seat normal in `faces` (one facet's unit normal
/// and closest point, [`crate::tip_landing::tip_seat_faces_with_ids`]'s answer,
/// in strut order, `ids` the seat triangles) by its FOOTPRINT normal. The seat
/// point is not changed. Also returns every measured tip's [`TipSite`] (strut
/// order), for the re-measurements after the seats slide.
///
/// Two batched passes (module doc): the ball is built from the arm with the
/// seat triangle's normal, the footprint measured about it, the arm re-seated
/// on the result and the footprint measured again. A rule without the
/// footprint switch, a tie, a brace, a mast, and any tip with no face are left
/// alone.
#[expect(
    clippy::float_cmp,
    reason = "identity, not closeness: `footprint_normal` returns the seat triangle's own normal \
              bit for bit when the average lands within SAME_NORMAL_ULPS of it, so unequal means \
              the normal was replaced"
)]
pub fn refine(
    env: &FootprintEnv<'_>,
    struts: &[SweepStrut],
    faces: &mut [Option<CarriedFace>],
    ids: &[Option<u32>],
) -> (FootprintNormalOutcome, Vec<Option<TipSite>>) {
    let mut outcome = FootprintNormalOutcome {
        enabled: env.rule.footprint_normal(),
        ..FootprintNormalOutcome::default()
    };
    let mut sites: Vec<Option<TipSite>> = vec![None; struts.len()];
    if !outcome.enabled {
        return (outcome, sites);
    }
    let landings = crate::tip_landing::tip_landings_of(
        env.grid,
        struts,
        env.plate_z_mm,
        env.layer_height_mm,
    );
    let mut landing_of: Vec<Option<[f64; 3]>> = vec![None; struts.len()];
    for landing in &landings {
        if let Some(slot) = landing_of.get_mut(landing.strut) {
            *slot = Some(landing.point_mm);
        }
    }
    let tips: Vec<usize> = (0..struts.len().min(faces.len()).min(ids.len()))
        .filter(|&index| {
            struts[index].kind == StrutKind::Arm
                && faces[index].is_some()
                && ids[index].is_some()
                && landing_of[index].is_some()
        })
        .collect();
    outcome.tips = tips.len();
    if tips.is_empty() {
        return (outcome, sites);
    }
    // Per tip: the seat triangle's identity and normal, and the admission
    // threshold (the rule, or every down-facing face where the mesh had no
    // admitted face at all and the seat fell back).
    for &index in &tips {
        let (Some((normal, _)), Some(face), Some(landing_mm)) =
            (faces[index], ids[index], landing_of[index])
        else {
            continue;
        };
        sites[index] = Some(TipSite {
            face,
            landing_mm,
            facet_normal: normal,
            max_normal_z: if env.rule.admits(normal[2]) {
                env.rule.max_normal_z()
            } else {
                SeatFaceRule::any_down_facing().max_normal_z()
            },
        });
    }
    let arms: Vec<SweepArm> = struts
        .iter()
        .enumerate()
        .map(|(index, strut)| {
            let (seat, base) =
                crate::adapter::drawn_ends(env.grid, env.plate_z_mm, env.layer_height_mm, strut);
            let point = faces.get(index).copied().flatten().map(|(_, point)| point);
            crate::adapter::sweep_arm(strut, seat, base, point, Vec::new())
        })
        .collect();
    let Some(measurement) = measure_arms(&env.measure_env(), &arms, &sites) else {
        // The request itself was unusable (no layer plan): nothing is
        // measured, every tip keeps its seat triangle's normal.
        outcome.fallback_no_ball = tips.len();
        return (outcome, sites);
    };
    outcome.pairs = measurement.pairs;
    outcome.pairs_from_surface = measurement.pairs_from_surface;
    let mut sum_deg = 0.0_f64;
    for &index in &tips {
        let (Some(measured), Some(site)) = (measurement.tips[index], sites[index]) else {
            continue;
        };
        outcome.fallback_no_ball += usize::from(measured.no_ball);
        outcome.fallback_no_footprint += usize::from(measured.no_footprint);
        outcome.max_faces_in_footprint = outcome.max_faces_in_footprint.max(measured.faces);
        outcome.fixed_point_residual_max_deg = outcome
            .fixed_point_residual_max_deg
            .max(measured.residual_deg);
        if measured.normal != site.facet_normal {
            let moved_deg = angle_deg(measured.normal, site.facet_normal);
            outcome.moved += 1;
            outcome.moved_over_1_deg += usize::from(moved_deg > 1.0);
            outcome.moved_over_5_deg += usize::from(moved_deg > 5.0);
            outcome.moved_over_15_deg += usize::from(moved_deg > 15.0);
            outcome.max_deg = outcome.max_deg.max(moved_deg);
            sum_deg += moved_deg;
            if let Some(slot) = faces.get_mut(index) {
                *slot = slot.map(|(_, point)| (measured.normal, point));
            }
        }
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a tip count is exact in f64"
    )]
    {
        outcome.mean_deg = sum_deg / tips.len() as f64;
    }
    (outcome, sites)
}

/// The normals of tips SLID by the footprint rule, measured on the moved arms
/// (`moved`, each with its seat and seat-face point shifted by `shifts_mm[i]`),
/// with the seat triangle re-derived at the slid seat ([`reseat_sites`]).
/// `sites[i]` is arm `i`'s site before the slide. An arm with no site (the
/// pass is off, a brace, a mast, a tie) keeps the normal it carries. One
/// batched [`measure_arms`] call; this is the re-measurement a proposed slide
/// is validated with ([`crate::footprint::apply`]).
#[must_use]
pub fn remeasure_slid(
    env: &MeasureEnv<'_>,
    sites: &[Option<TipSite>],
    moved: &[SweepArm],
    shifts_mm: &[[f64; 3]],
) -> Vec<[f64; 3]> {
    let shifts: Vec<Option<[f64; 3]>> = shifts_mm.iter().map(|&shift| Some(shift)).collect();
    let sites = reseat_sites(env.mesh, sites, &shifts);
    let measurement = measure_arms(env, moved, &sites);
    moved
        .iter()
        .enumerate()
        .map(|(index, arm)| {
            measurement
                .as_ref()
                .and_then(|found| found.tips.get(index).copied().flatten())
                .map_or(arm.seat_normal_mm, |measured| measured.normal)
        })
        .collect()
}

/// What [`remeasure_emitted`] changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmittedRemeasure {
    /// Strut indices whose seat normal was not the measured one on the
    /// emitted geometry, and now is.
    pub changed: Vec<usize>,
    /// The largest angle among those changes, degrees.
    pub max_change_deg: f64,
}

/// MEASURE THE EMITTED FIELD: every measured tip's footprint normal on the
/// arms exactly as the adapter hands them to the lowering (`emitted`: seat
/// slides, followers' base and seat ends, junction offsets all applied, each
/// arm carrying `strut.seat_normal`), with the seat triangle re-derived for
/// every tip that slid. A tip whose normal differs from the one it carries is
/// given the measured one, so the normal on a strut describes the ball it is
/// drawn with. One more batched [`measure_arms`] call.
///
/// `sites` is [`refine`]'s answer; `None` for a tip it did not measure.
#[expect(
    clippy::float_cmp,
    reason = "identity, not closeness: a strut must carry exactly the normal measured on its \
              emitted ball, so any bit of difference is a change to write and to report"
)]
pub fn remeasure_emitted(
    env: &MeasureEnv<'_>,
    struts: &mut [SweepStrut],
    emitted: &[SweepArm],
    sites: &[Option<TipSite>],
) -> EmittedRemeasure {
    let shifts: Vec<Option<[f64; 3]>> = struts
        .iter()
        .map(|strut| {
            (strut.seat_face.footprint == Footprint::MovedOntoFace)
                .then_some(strut.seat_face.shift_mm)
        })
        .collect();
    let sites = reseat_sites(env.mesh, sites, &shifts);
    let mut report = EmittedRemeasure::default();
    let Some(measurement) = measure_arms(env, emitted, &sites) else {
        return report;
    };
    for (index, strut) in struts.iter_mut().enumerate() {
        let Some(Some(measured)) = measurement.tips.get(index) else {
            continue;
        };
        if measured.normal != strut.seat_normal {
            report.max_change_deg = report
                .max_change_deg
                .max(angle_deg(measured.normal, strut.seat_normal));
            strut.seat_normal = measured.normal;
            report.changed.push(index);
        }
    }
    report
}
