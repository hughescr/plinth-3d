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
//!             and edge-connected to the seat triangle through such faces
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
//! * **Edge-connected.** The far side of a thin wall, or a second sheet the
//!   ball overlaps, is not part of the patch the tip touches. The walk is
//!   over the admitted faces of the tip's own row, seeded at the admitted
//!   face nearest the seat point.
//! * **No area, no change.** Where the covered area is nil (no admitted face
//!   in the ball, or the ball could not be built) the tip keeps its seat
//!   triangle's normal, and the count says so (emit and warn, never refuse).
//! * **Flat faces are untouched.** Where the average equals the seat
//!   triangle's normal to rounding the seat triangle's own bits are kept, so
//!   a flat underside (every Round disc) is byte-identical.
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
//! * Two passes ([`refine`]): the ball depends on the seat depth, which
//!   depends on the normal. Seat along the facet normal, measure the
//!   footprint normal, re-seat along it, measure again. The largest angle
//!   between the two answers is published ([`FootprintNormalOutcome`]).
//! * Ties are left alone: a model tie may seat on a wall (decision D8), so its
//!   "admitted faces" are every face.
//! * The orientation prefilter never runs it
//!   ([`crate::SweepPolicy::without_footprint_normal`]).

use geom_kernel::{closest_on_tri, vec3, NativeSurface, NearFaceBatchLimits};
use plinth_core::LayerPlan;
use plinth_mesh::Mesh;
use rayon::prelude::*;

use crate::grid::Grid;
use crate::tip_landing::{CarriedFace, SeatFaceRule};
use crate::{StrutKind, SweepStrut};

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
    /// The seat point the tip carries today (the closest point of its seat
    /// triangle): the connected patch is the one through the admitted face
    /// nearest to it.
    pub seed_mm: [f64; 3],
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

/// A face of a row: its vertex ids, outward unit normal and clipped area.
struct RowFace {
    ids: [u32; 3],
    normal: [f64; 3],
    area_mm2: f64,
    distance_to_seed_mm: f64,
}

fn footprint_of_row(
    verts: &[[f64; 3]],
    tris: &[[u32; 3]],
    row: &[u32],
    ball: &FootprintBall,
) -> Option<FootprintNormal> {
    let mut admitted: Vec<RowFace> = Vec::with_capacity(row.len());
    for &id in row {
        let Some(&ids) = tris.get(id as usize) else { continue };
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
            ids,
            normal,
            area_mm2: triangle_disc_area([a, b, c], ball.centre_mm, ball.radius_mm),
            distance_to_seed_mm: vec3::dist(ball.seed_mm, closest_on_tri(ball.seed_mm, a, b, c)),
        });
    }
    // THE SEED: the admitted face nearest the seat point (lowest index on a tie).
    let mut seed: Option<usize> = None;
    let mut best = f64::INFINITY;
    for (index, face) in admitted.iter().enumerate() {
        if face.distance_to_seed_mm < best {
            best = face.distance_to_seed_mm;
            seed = Some(index);
        }
    }
    let seed = seed?;
    // EDGE CONNECTIVITY through admitted faces of this row: union faces that
    // share a vertex pair.
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
        if pair[0].0 == pair[1].0 && pair[0].1 == pair[1].1 {
            let (x, y) = (find(&mut parent, pair[0].2), find(&mut parent, pair[1].2));
            if x != y {
                parent[x.max(y)] = x.min(y);
            }
        }
    }
    let seed_root = find(&mut parent, seed);
    let mut sum = [0.0_f64; 3];
    let mut area = 0.0_f64;
    let mut faces = 0_usize;
    for index in 0..admitted.len() {
        if find(&mut parent, index) != seed_root {
            continue;
        }
        let face = &admitted[index];
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
    /// Tips whose footprint had no admitted, connected area: they keep the
    /// seat triangle's normal.
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

fn angle_deg(a: [f64; 3], b: [f64; 3]) -> f64 {
    vec3::dot(a, b).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Replace every support arm's seat normal in `faces` (one facet's unit normal
/// and closest point, [`crate::tip_landing::tip_seat_faces`]'s answer, in
/// strut order) by its FOOTPRINT normal. The seat point is not changed.
///
/// Two batched passes (module doc): the ball is built from the arm with the
/// current normal, the footprint measured about it, the arm re-seated on the
/// result and the footprint measured again. A rule without the footprint
/// switch, a tie, a brace, a mast, and any tip with no face are left alone.
pub fn refine(
    env: &FootprintEnv<'_>,
    struts: &[SweepStrut],
    faces: &mut [Option<CarriedFace>],
) -> FootprintNormalOutcome {
    let mut outcome = FootprintNormalOutcome {
        enabled: env.rule.footprint_normal(),
        ..FootprintNormalOutcome::default()
    };
    if !outcome.enabled {
        return outcome;
    }
    let tips: Vec<usize> = (0..struts.len().min(faces.len()))
        .filter(|&index| struts[index].kind == StrutKind::Arm && faces[index].is_some())
        .collect();
    outcome.tips = tips.len();
    if tips.is_empty() {
        return outcome;
    }
    // Per tip: the seat triangle's normal and point, the admission threshold
    // (the rule, or every down-facing face where the mesh had no admitted face
    // at all and the seat fell back), and the working normal.
    let facet: Vec<[f64; 3]> = tips.iter().map(|&i| faces[i].map_or([0.0; 3], |f| f.0)).collect();
    let seed: Vec<[f64; 3]> = tips.iter().map(|&i| faces[i].map_or([0.0; 3], |f| f.1)).collect();
    let threshold: Vec<f64> = facet
        .iter()
        .map(|normal| {
            if env.rule.admits(normal[2]) {
                env.rule.max_normal_z()
            } else {
                SeatFaceRule::any_down_facing().max_normal_z()
            }
        })
        .collect();
    let mut current = facet.clone();
    let mut first: Vec<Option<[f64; 3]>> = vec![None; tips.len()];
    let mut second: Vec<Option<[f64; 3]>> = vec![None; tips.len()];
    let mut no_ball = vec![false; tips.len()];
    let mut no_footprint = vec![false; tips.len()];
    let mut max_faces = 0_usize;
    for pass in 0..2 {
        let arms: Vec<plinth_support::SweepArm> = tips
            .iter()
            .zip(&current)
            .zip(&seed)
            .map(|((&index, &normal), &point)| {
                let strut = &struts[index];
                let (seat, base) =
                    crate::adapter::drawn_ends(env.grid, env.plate_z_mm, env.layer_height_mm, strut);
                let mut arm = crate::adapter::sweep_arm(strut, seat, base, Some(point), Vec::new());
                arm.seat_normal_mm = normal;
                arm
            })
            .collect();
        let Ok(discs) = plinth_support::tip_contact_discs(&arms, env.layer_plan) else {
            // The request itself was unusable (no layer plan): nothing is
            // measured, every tip keeps its seat triangle's normal.
            outcome.fallback_no_ball = tips.len();
            return outcome;
        };
        let mut rows_of: Vec<usize> = Vec::with_capacity(tips.len());
        let mut centres: Vec<[f64; 3]> = Vec::with_capacity(tips.len());
        let mut radii: Vec<f64> = Vec::with_capacity(tips.len());
        for (position, disc) in discs.iter().enumerate() {
            match disc {
                Some(disc) => {
                    rows_of.push(position);
                    centres.push(disc.ball_centre_mm);
                    radii.push(disc.ball_radius_mm);
                }
                None => no_ball[position] = true,
            }
        }
        let Some(pairs) = footprint_pairs(env.mesh, env.surface, &centres, &radii) else {
            outcome.fallback_no_ball = tips.len();
            return outcome;
        };
        outcome.pairs = pairs.faces.len();
        outcome.pairs_from_surface = env.surface.is_some();
        let balls: Vec<FootprintBall> = rows_of
            .iter()
            .zip(centres.iter().zip(&radii))
            .map(|(&position, (&centre_mm, &radius_mm))| FootprintBall {
                centre_mm,
                radius_mm,
                seed_mm: seed[position],
                facet_normal: facet[position],
                max_normal_z: threshold[position],
            })
            .collect();
        let found = footprint_normals(env.mesh, &pairs, &balls);
        let slot = if pass == 0 { &mut first } else { &mut second };
        for (&position, answer) in rows_of.iter().zip(&found) {
            match answer {
                Some(answer) => {
                    current[position] = answer.normal;
                    slot[position] = Some(answer.normal);
                    no_footprint[position] = false;
                    max_faces = max_faces.max(answer.faces);
                }
                None => no_footprint[position] = true,
            }
        }
    }
    outcome.max_faces_in_footprint = max_faces;
    outcome.fallback_no_ball = no_ball.iter().filter(|&&flag| flag).count();
    outcome.fallback_no_footprint = no_footprint
        .iter()
        .zip(&no_ball)
        .filter(|(&none, &ball)| none && !ball)
        .count();
    let mut sum_deg = 0.0_f64;
    for (position, &index) in tips.iter().enumerate() {
        if let (Some(a), Some(b)) = (first[position], second[position]) {
            outcome.fixed_point_residual_max_deg =
                outcome.fixed_point_residual_max_deg.max(angle_deg(a, b));
        }
        let normal = current[position];
        if normal != facet[position] {
            let moved_deg = angle_deg(normal, facet[position]);
            outcome.moved += 1;
            outcome.moved_over_1_deg += usize::from(moved_deg > 1.0);
            outcome.moved_over_5_deg += usize::from(moved_deg > 5.0);
            outcome.moved_over_15_deg += usize::from(moved_deg > 15.0);
            outcome.max_deg = outcome.max_deg.max(moved_deg);
            sum_deg += moved_deg;
            if let Some(slot) = faces.get_mut(index) {
                *slot = slot.map(|(_, point)| (normal, point));
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
    outcome
}
