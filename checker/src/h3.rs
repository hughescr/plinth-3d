//! H3: print-order continuity. Layer by layer from the plate up, every piece of support in a
//! layer connects to the plate, or to support printed in that layer or below. Nothing relies
//! on geometry printed later.
//!
//! The checker slices the written support meshes with its own slicer (each part is a convex
//! polytope, so a section is a convex polygon). The plate is at the support minimum, which
//! must agree with the plate level the model's translate Z implies; layers are 0.05 mm.
//!
//! **Sample planes.** Each layer is sliced at its bottom, middle and top and at every vertex
//! height of any part inside it (at least 1e-4 mm from the layer's faces). Every section keeps
//! its plane, and the continuity is checked at corresponding planes, so the answer cannot
//! depend on which plane a slicer takes:
//!
//! * in one plane, sections of different parts are joined when they overlap by at least 0.01 mm
//!   (the penetration depth of the two convex polygons, or one lies inside the other);
//! * a section is **rooted** by the layer below when, for *every* plane of the layer below,
//!   some rooted section of that plane overlaps it (a plane of the layer below where nothing
//!   overlaps it means the layer, sliced there, leaves it unsupported);
//! * a component of one plane is rooted when one of its sections is rooted, or, in layer 0,
//!   holds a part that stands on the plate.
//!
//! **Verdict scope.** The slicer reads the written triangles, and the verdict is measured on
//! them alone: valid for ChituBox v1.4.0 if it slices the stored support meshes, pending the
//! H8 differential slice test. The written vertices are exact in f64 (they are f32 values), so
//! the only uncertainty is the rounding of the section arithmetic: an overlap depth or a
//! section width within `ROUND_MM` (1e-9 mm, far over the rounding at these sizes) of its
//! threshold could be on either side of it, so the walk is made under two readings of the
//! thresholds (`Reading`): the strict one (connect less, print more) must be clean for a pass,
//! and a finding is a violation only if the lenient one (connect more, print less) has it too;
//! one found only by the strict reading is unverified. The solid the fields name is not read:
//! how far its end heights and its sides differ from the written mesh is reported as
//! information (`analytic_information`) and decides nothing.
//!
//! **End events are proved, not sampled.** No set of sample planes excludes an air gap between
//! two sample heights (a gap of 1e-6 to 9e-5 mm between coaxial rods lies between all of
//! them), or a thin start that hangs beside its carrier until a later, wider section reaches
//! it. So, besides the walk, the start of every part that stands neither on the plate nor on
//! the model (a foot) is proved to stand on other, seated support over the whole stretch in
//! which it prints (from the first height its section is not a sliver to the first height it
//! is wide enough to be rooted), on the written meshes; what cannot be proved is a violation
//! when the part is certainly in the air where it first prints, and otherwise unverified. The
//! proof is in `seat`. Faces that meet exactly, with no overlap along the axis to cover the
//! rounding, are unverified, never a pass.
//!
//! **Model.** The model is not seeded as rooted. Its births (the lowest points of its solid,
//! where a new piece of each layer first appears; see `islands`) are the islands, and each must
//! be held by a rooted support section in its first layer or the layer below. A model that is
//! not one closed, outward-wound mesh gives no island list, and then H3 is unverified. A
//! birth that is large enough to be certainly over the small-island size and has no support
//! under it is a violation; a small one, whose growth and joining are not measured (P18), is
//! unverified. With model-standing feet, the model is sliced into material tiles (holes stay
//! empty) in the same print-order walk. Only already rooted material can root a foot.

use crate::islands::{self, Birth};
use crate::json::Json;
use crate::outcome::{MAX_LOCATED, MEASURED_ON, Outcome, VERDICT_SCOPE, at};
use crate::parts::{Part, Role};
use crate::patch::PatchOut;
use crate::scene::{LAYER_MM, Scene};
use crate::status::Located;
use crate::tri::Tri;
use crate::v3::V;
use std::collections::{BTreeMap, BTreeSet};

/// Two sections connect when they overlap by at least this much, mm.
pub const OVERLAP_MM: f64 = 0.01;
/// A part lengthened past its end faces is sliced this far inside the face, mm.
const EDGE_FACE_MM: f64 = 1e-6;
/// A vertex height only adds a sample plane this far inside the layer, mm.
const EDGE_MARGIN: f64 = 1e-4;
/// The bottom and top sample planes sit this far inside the layer, mm.
const PLANE_DELTA: f64 = 2e-6;
/// Sample planes closer than this are one plane, mm. An air gap shorter than this inside a
/// layer can lie between two planes.
const PLANE_SPACING_MM: f64 = 2e-4;
/// Grid cell for neighbour search, mm.
const CELL_MM: f64 = 2.0;
/// A part that starts within this of the plate stands on it, mm.
const ON_PLATE_MM: f64 = 1e-3;
/// Most layers sliced (1 m of height); a taller file is unverified, not an endless loop.
const MAX_LAYERS: usize = 20_000;

pub type P2 = [f64; 2];

/// A convex section of a part by one plane.
#[derive(Debug, Clone)]
pub struct Sec {
    pub poly: Vec<P2>,
    pub bb: [f64; 4],
}

impl Sec {
    /// A section from a convex polygon in counter-clockwise order.
    #[must_use]
    pub fn from_poly(poly: Vec<P2>) -> Self {
        let mut bb = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for v in &poly {
            bb = [
                bb[0].min(v[0]),
                bb[1].min(v[1]),
                bb[2].max(v[0]),
                bb[3].max(v[1]),
            ];
        }
        Self { poly, bb }
    }
}

/// Convex hull (counter-clockwise) of 2D points.
fn hull(points: &[P2]) -> Vec<P2> {
    let mut p = points.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12);
    if p.len() < 3 {
        return Vec::new();
    }
    let cross = |o: P2, a: P2, b: P2| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut h: Vec<P2> = Vec::new();
    for pass in 0..2 {
        let start = h.len();
        let order: Vec<P2> = if pass == 0 {
            p.clone()
        } else {
            p.iter().rev().copied().collect()
        };
        for q in order {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h
}

fn poly_area(p: &[P2]) -> f64 {
    0.5 * (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            a[0] * b[1] - a[1] * b[0]
        })
        .sum::<f64>()
}

/// The points whose hull is the section of a convex mesh by the plane `z = c`: the vertices on
/// the plane and the crossings of the edges that cross it.
fn section_points(tris: &[Tri], c: f64) -> Vec<P2> {
    let mut pts: Vec<P2> = Vec::new();
    for t in tris {
        for k in 0..3 {
            let (p, q) = (t[k], t[(k + 1) % 3]);
            let (dp, dq) = (p[2] - c, q[2] - c);
            if dp.abs() <= 1e-9 {
                pts.push([p[0], p[1]]);
            }
            if (dp < 0.0 && dq > 0.0) || (dp > 0.0 && dq < 0.0) {
                let s = dp / (dp - dq);
                pts.push([p[0] + (q[0] - p[0]) * s, p[1] + (q[1] - p[1]) * s]);
            }
        }
    }
    pts
}

/// The points whose hull contains every section of a convex mesh between `z1` and `z2`: the
/// sections at the two heights and the vertices in between, seen from above.
fn slab_points(tris: &[Tri], z1: f64, z2: f64) -> Vec<P2> {
    let mut pts = section_points(tris, z1);
    pts.extend(section_points(tris, z2));
    pts.extend(
        tris.iter()
            .flatten()
            .filter(|v| v[2] > z1 && v[2] < z2)
            .map(|v| [v[0], v[1]]),
    );
    pts
}

/// The section of a convex mesh by the plane `z = c`, as a convex polygon (or none).
fn section(tris: &[Tri], c: f64, sliver: f64) -> Option<Sec> {
    let poly = hull(&section_points(tris, c));
    if poly.len() < 3 || poly_area(&poly) < 1e-14 || poly_width(&poly) < sliver {
        // A sliver thinner than the overlap depth (the tangent line at the bottom of a
        // horizontal rod, the first sliver of a ball) neither prints nor roots anything.
        return None;
    }
    Some(Sec::from_poly(poly))
}

/// The width of a convex polygon (its least extent over the directions of its edges).
fn poly_width(poly: &[P2]) -> f64 {
    (0..poly.len())
        .map(|i| {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            let n = [q[1] - p[1], p[0] - q[0]];
            let l = n[0].hypot(n[1]).max(1e-300);
            let (lo, hi) = poly
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                    let d = (n[0] * v[0] + n[1] * v[1]) / l;
                    (lo.min(d), hi.max(d))
                });
            hi - lo
        })
        .fold(f64::INFINITY, f64::min)
}

fn inside(p: P2, poly: &[P2]) -> bool {
    (0..poly.len()).all(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) >= -1e-9
    })
}

/// Do two convex polygons overlap by at least `OVERLAP_MM` (penetration depth), or does one
/// lie inside the other?
#[must_use]
pub fn overlap(a: &Sec, b: &Sec) -> bool {
    overlap_by(a, b, OVERLAP_MM)
}

/// Do two convex polygons overlap by at least `need` (penetration depth), or does one lie
/// inside the other?
#[must_use]
pub fn overlap_by(a: &Sec, b: &Sec, need: f64) -> bool {
    if a.bb[0] > b.bb[2] || b.bb[0] > a.bb[2] || a.bb[1] > b.bb[3] || b.bb[1] > a.bb[3] {
        return false;
    }
    if a.poly.iter().all(|p| inside(*p, &b.poly)) || b.poly.iter().all(|p| inside(*p, &a.poly)) {
        return true;
    }
    let mut depth = f64::INFINITY;
    for poly in [&a.poly, &b.poly] {
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            let n = [q[1] - p[1], p[0] - q[0]];
            let l = n[0].hypot(n[1]);
            if l < 1e-300 {
                continue;
            }
            let n = [n[0] / l, n[1] / l];
            let range = |s: &Sec| {
                s.poly
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                        let d = n[0] * v[0] + n[1] * v[1];
                        (lo.min(d), hi.max(d))
                    })
            };
            let (ra, rb) = (range(a), range(b));
            depth = depth.min(ra.1.min(rb.1) - ra.0.max(rb.0));
            if depth < need {
                return false;
            }
        }
    }
    depth >= need
}

/// The bound on the rounding of the section arithmetic, mm: a depth or a width this close to
/// its threshold is not told from it. The written vertices are exact; the sections are
/// computed in f64 (about 1e-13 mm at these coordinates), so this is a conservative bound.
pub const ROUND_MM: f64 = 1e-9;

/// End faces of two parts this close meet: the proof slices `EDGE_FACE_MM` inside an end face,
/// so a part that ends within twice that of such a height is not told from one that reaches
/// it, mm.
pub const TOUCH_MM: f64 = 2.0 * EDGE_FACE_MM;

/// How the thresholds are read, around the rounding of the section arithmetic (`ROUND_MM`) and
/// the meeting of end faces (`TOUCH_MM`). A reading is **strict** when it connects less and
/// prints more (a pass in it holds whatever the rounding) and **lenient** when it connects more
/// and prints less (an unrooted piece in it is unrooted whatever the rounding).
#[derive(Debug, Clone, Copy)]
pub struct Reading {
    /// Two sections connect when they overlap by at least this, mm.
    depth: f64,
    /// A section thinner than this neither prints nor roots, mm.
    sliver: f64,
    /// A part still counts as there at a height its top end falls short of by up to this, mm.
    reach: f64,
}

impl Reading {
    #[must_use]
    pub fn strict() -> Self {
        Self {
            depth: OVERLAP_MM + ROUND_MM,
            sliver: OVERLAP_MM - ROUND_MM,
            reach: ROUND_MM,
        }
    }

    #[must_use]
    pub fn lenient() -> Self {
        Self {
            depth: OVERLAP_MM - ROUND_MM,
            sliver: OVERLAP_MM + ROUND_MM,
            reach: TOUCH_MM,
        }
    }
}

/// A part's layer range and its z extent, from the written mesh.
struct PartInfo {
    /// The written mesh's own lowest and highest point.
    zmin: f64,
    zmax: f64,
    k_lo: usize,
    k_hi: usize,
    on_plate: bool,
}

fn part_info(p: &Part, z0: f64) -> PartInfo {
    let mut zs: Vec<f64> = p.tris.iter().flatten().map(|v| v[2]).collect();
    zs.sort_by(f64::total_cmp);
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    let (zmin, zmax) = (zs[0], zs[zs.len() - 1]);
    let layer = |z: f64| ((z - z0) / LAYER_MM).max(0.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a layer index"
    )]
    let k_lo = (layer(zmin) + 1e-9).floor() as usize;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a layer index"
    )]
    let k_hi = ((layer(zmax) - 1e-9).ceil() as usize)
        .saturating_sub(1)
        .max(k_lo);
    PartInfo {
        zmin,
        zmax,
        k_lo,
        k_hi,
        on_plate: zmin - z0 <= ON_PLATE_MM,
    }
}

type Grid = BTreeMap<(i64, i64), Vec<usize>>;

#[allow(clippy::cast_possible_truncation, reason = "a grid cell index")]
fn cells(bb: &[f64; 4]) -> impl Iterator<Item = (i64, i64)> {
    let c = |x: f64| (x / CELL_MM).floor() as i64;
    let (x0, x1, y0, y1) = (c(bb[0]), c(bb[2]), c(bb[1]), c(bb[3]));
    (x0..=x1).flat_map(move |x| (y0..=y1).map(move |y| (x, y)))
}

fn grid_of(secs: &[(usize, Sec)]) -> Grid {
    let mut g = Grid::new();
    for (i, (_, s)) in secs.iter().enumerate() {
        for c in cells(&s.bb) {
            g.entry(c).or_default().push(i);
        }
    }
    g
}

/// The sections in the cells a box touches, each once, in ascending order.
fn neighbours(g: &Grid, bb: &[f64; 4]) -> Vec<usize> {
    let mut v: Vec<usize> = cells(bb)
        .flat_map(|c| g.get(&c).into_iter().flatten().copied())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn find(u: &mut [usize], mut x: usize) -> usize {
    while u[x] != x {
        u[x] = u[u[x]];
        x = u[x];
    }
    x
}

/// One sample plane of a layer: the sections of the parts alive at that height, and what the
/// walk decided about each.
#[derive(Debug)]
struct Plane {
    z: f64,
    /// Original support sample, not an extra local material event.
    support_sample: bool,
    /// (part index, section)
    secs: Vec<(usize, Sec)>,
    /// Connected to the plate through support printed in this layer or below.
    rooted: Vec<bool>,
    /// Connected to the plate only through material that stands on the model.
    reliant: Vec<bool>,
    grid: Grid,
}

/// What the rooting needs besides the sections.
struct Roots<'a> {
    infos: &'a [PartInfo],
    /// Parts that are feet standing on the model.
    feet: &'a BTreeSet<usize>,
    reading: Reading,
}

/// Sample plane heights of layer `k`: the bottom, middle and top, and, for every part that
/// starts or ends inside the layer, a plane in the middle of its stretch of the layer and a
/// plane just above its end. A part that ends and the part that continues it with an air gap
/// are then never both missing from every plane, and no part is missed by all planes.
/// Planes closer than `PLANE_SPACING_MM` are one plane.
fn layer_planes(infos: &[PartInfo], k: usize, z0: f64) -> Vec<f64> {
    let bottom = z0 + k as f64 * LAYER_MM;
    let top = bottom + LAYER_MM;
    let mut planes = vec![
        bottom + PLANE_DELTA,
        0.5 * (bottom + top),
        top - PLANE_DELTA,
    ];
    for info in infos.iter().filter(|i| (i.k_lo..=i.k_hi).contains(&k)) {
        let zmax = info.zmax;
        let (lo, hi) = (info.zmin.max(bottom), zmax.min(top));
        let starts_inside = info.zmin > bottom + EDGE_MARGIN;
        let ends_inside = zmax < top - EDGE_MARGIN;
        if starts_inside || ends_inside {
            planes.push(0.5 * (lo + hi));
        }
        if ends_inside {
            planes.push(zmax + EDGE_MARGIN);
        }
    }
    planes.retain(|z| *z > bottom && *z < top);
    planes.sort_by(f64::total_cmp);
    planes.dedup_by(|a, b| (*a - *b).abs() < PLANE_SPACING_MM);
    planes
}

/// Rooting of one plane's sections: `(rooted, reliant)` per section.
fn root_plane(
    secs: &[(usize, Sec)],
    grid: &Grid,
    prev: Option<&[Plane]>,
    roots: &Roots,
) -> (Vec<bool>, Vec<bool>) {
    let n = secs.len();
    let mut uf: Vec<usize> = (0..n).collect();
    for (i, (_, s)) in secs.iter().enumerate() {
        for j in neighbours(grid, &s.bb).into_iter().filter(|&j| j > i) {
            let both_model = secs[i].0 >= roots.infos.len() && secs[j].0 >= roots.infos.len();
            if overlap_by(s, &secs[j].1, roots.reading.depth)
                || (both_model && model::joined(s, &secs[j].1))
            {
                let (a, b) = (find(&mut uf, i), find(&mut uf, j));
                uf[a] = b;
            }
        }
    }
    let (mut root_comp, mut rely_comp) = (vec![false; n], vec![false; n]);
    for (i, (part, s)) in secs.iter().enumerate() {
        let (rooted, covered) = match prev {
            None => (roots.infos.get(*part).is_some_and(|i| i.on_plate), false),
            Some(planes) => below(
                s,
                planes,
                roots.reading.depth,
                *part >= roots.infos.len() || roots.feet.contains(part),
            ),
        };
        let r = find(&mut uf, i);
        root_comp[r] |= rooted;
        rely_comp[r] |= roots.feet.contains(part) || covered;
    }
    let rooted: Vec<bool> = (0..n).map(|i| root_comp[find(&mut uf, i)]).collect();
    let reliant = (0..n)
        .map(|i| !rooted[i] && rely_comp[find(&mut uf, i)])
        .collect();
    (rooted, reliant)
}

/// Is `s` rooted by the layer below (for every plane of it some rooted section overlaps `s`),
/// and is it at least covered (rooted or reliant sections overlap it in every plane)?
fn below(s: &Sec, planes: &[Plane], depth: f64, full_batch: bool) -> (bool, bool) {
    let (mut all_rooted, mut all_covered) = (true, true);
    for pl in planes.iter().filter(|p| full_batch || p.support_sample) {
        let (mut any_rooted, mut any_covered) = (false, false);
        for j in neighbours(&pl.grid, &s.bb) {
            // A rooted overlap settles both answers for this plane (rooted implies covered).
            if (pl.rooted[j] || (!any_covered && pl.reliant[j]))
                && overlap_by(s, &pl.secs[j].1, depth)
            {
                any_rooted |= pl.rooted[j];
                any_covered = true;
                if any_rooted {
                    break;
                }
            }
        }
        all_rooted &= any_rooted;
        all_covered &= any_covered;
        if !all_covered {
            // Neither answer can come back.
            break;
        }
    }
    (all_rooted, all_covered)
}

/// What the layer walk collects.
struct Walk<'a> {
    scene: &'a Scene,
    z0: f64,
    reported: BTreeSet<usize>,
    located: Vec<Located>,
    relies_on_model: Vec<String>,
    pieces: usize,
    unrooted_pieces: usize,
    planes_total: usize,
}

impl Walk<'_> {
    fn plane(&mut self, pl: &Plane, k: usize, check_contacts: bool) -> (usize, usize) {
        let (scene, z0) = (self.scene, self.z0);
        let candidates: Vec<_> = pl
            .secs
            .iter()
            .enumerate()
            .filter(|(i, (part, _))| {
                check_contacts
                    && *part < scene.parts.len()
                    && !pl.rooted[*i]
                    && !pl.reliant[*i]
                    && !self.reported.contains(part)
            })
            .map(|(_, (part, sec))| (*part, sec))
            .collect();
        let (possible, batches, tiles) = model::possible_carriers(scene, pl.z, &candidates);
        self.pieces += pl
            .secs
            .iter()
            .filter(|(part, _)| *part < scene.parts.len())
            .count();
        self.planes_total += 1;
        for (i, (part, s)) in pl
            .secs
            .iter()
            .enumerate()
            .filter(|(i, (part, _))| *part < scene.parts.len() && !pl.rooted[*i])
        {
            self.unrooted_pieces += 1;
            if !self.reported.insert(*part) {
                continue;
            }
            let p = &scene.parts[*part];
            if pl.reliant[i] {
                self.relies_on_model.push(format!(
                    "record {} ({}) relies on model material whose print-order rooting could not be established (first seen in layer {k})",
                    p.rec,
                    p.role.name()
                ));
                continue;
            }
            if let Some(evidence) = possible.get(part) {
                self.relies_on_model.push(format!(
                    "record {} ({}) {evidence} in layer {k}; its connection through that material to the plate is not proved",
                    p.rec, p.role.name()
                ));
                continue;
            }
            let n = s.poly.len() as f64;
            let c = s
                .poly
                .iter()
                .fold([0.0; 2], |a, v| [a[0] + v[0] / n, a[1] + v[1] / n]);
            let place: V = scene.world([c[0], c[1], pl.z.max(z0)]);
            self.located.push(at(
                Some(p.rec),
                Some(place),
                u32::try_from(k).ok(),
                format!(
                    "this {} has no connection to the plate or to support printed in this layer or below (sliced at {:.4} mm above the plate)",
                    p.role.name(),
                    pl.z - z0
                ),
            ));
        }
        (batches, tiles)
    }
}

/// The finished support walk.
struct Walked {
    layers: Vec<Vec<Plane>>,
    z0: f64,
    located: Vec<Located>,
    relies_on_model: Vec<String>,
    pieces: usize,
    unrooted_pieces: usize,
    planes: usize,
    model_slice_batches: usize,
    model_tiles: usize,
}

/// Why the walk cannot be done, as an unverified outcome.
fn walk_refusal(scene: &Scene) -> Option<String> {
    if scene.parts.is_empty() {
        return Some("the file has no support records: the lifted model has nothing under it, so none of its islands is held".to_owned());
    }
    let invalid: Vec<String> = scene
        .invalid_parts()
        .iter()
        .map(|p| {
            format!(
                "record {} ({}): {}",
                p.rec,
                p.role.name(),
                p.invalid.as_deref().unwrap_or("")
            )
        })
        .collect();
    if let Some(first) = invalid.first() {
        return Some(format!(
            "{} part(s) are not the native shape; first: {first}",
            invalid.len()
        ));
    }
    crate::h4::plate_disagreement(scene)
}

mod walk;
use walk::run as walk;

/// Is the birth held: in every plane of its first layer at or above its height, or in every
/// plane of the layer below, a rooted support section overlaps its footprint?
fn anchored(w: &Walked, b: &Birth, depth: f64, support_count: usize) -> bool {
    let held = |planes: &[Plane], from: f64| {
        let mut any = false;
        for pl in planes.iter().filter(|p| p.support_sample && p.z >= from) {
            any = true;
            let hit = pl.secs.iter().enumerate().any(|(i, (part, s))| {
                *part < support_count
                    && pl.rooted[i]
                    && b.footprint.iter().any(|f| overlap_by(s, f, depth))
            });
            if !hit {
                return false;
            }
        }
        any
    };
    let own = w.layers.get(b.layer).is_some_and(|p| held(p, b.z - 1e-9));
    let below = b
        .layer
        .checked_sub(1)
        .and_then(|k| w.layers.get(k))
        .is_some_and(|p| held(p, f64::NEG_INFINITY));
    own || below
}

/// What one reading of the thresholds found.
struct Assessed {
    located: Vec<Located>,
    unverified: Vec<String>,
    listed: Vec<Json>,
    islands_note: String,
}

/// The findings of a finished walk and of the model's births under it.
fn assess(
    scene: &Scene,
    walked: &Walked,
    model_births: &Result<Vec<Birth>, String>,
    depth: f64,
) -> Assessed {
    let mut located = walked.located.clone();
    let mut unverified: Vec<String> = walked.relies_on_model.clone();
    let mut listed: Vec<Json> = Vec::new();
    let mut islands_note = String::new();
    match model_births {
        Err(why) => unverified.push(format!("model islands not listed: {why}")),
        Ok(births) => {
            for b in births {
                let held = anchored(walked, b, depth, scene.parts.len());
                listed.push(Json::obj([
                    ("layer", Json::Uint(b.layer as u64)),
                    ("height_above_plate_mm", Json::Num(b.z - walked.z0)),
                    ("x_extent_mm", Json::Num(b.extent[0])),
                    ("y_extent_mm", Json::Num(b.extent[1])),
                    ("held_by_support", Json::Bool(held)),
                ]));
                if held {
                    continue;
                }
                let place = scene.world(b.vertex);
                if b.certainly_over_small_size() {
                    located.push(at(
                        None,
                        Some(place),
                        u32::try_from(b.layer).ok(),
                        format!(
                            "a model island first appears in layer {} ({:.3} x {:.3} mm, over the small-island size) with no support section under it",
                            b.layer, b.extent[0], b.extent[1]
                        ),
                    ));
                } else {
                    unverified.push(format!(
                        "a small model island first appears in layer {} with no support section under it; its growth and joining (P18) are not measured",
                        b.layer
                    ));
                }
            }
            islands_note = format!("{} model island(s) listed", births.len());
        }
    }
    Assessed {
        located,
        unverified,
        listed,
        islands_note,
    }
}

/// Does `lenient` hold the same finding as `f` (the same record, or, for a model island, the
/// same layer)?
fn same_finding(f: &Located, lenient: &[Located]) -> bool {
    lenient.iter().any(|l| {
        l.location.record == f.location.record
            && (f.location.record.is_some() || l.location.layer == f.location.layer)
    })
}

/// How far the solid the fields name differs from the written mesh of each part, as report
/// information only (it decides nothing): the largest difference of the lowest and of the
/// highest point, with the record, and the largest distance the analytic solid stands outside
/// the mesh sideways (`Part::sag`). A raft part has no analytic solid but its mesh.
fn analytic_information(scene: &Scene) -> Json {
    let mut worst: Option<(f64, usize)> = None;
    let (mut sideways, mut lowest) = (0.0f64, 0.0f64);
    for p in scene.parts.iter().filter(|p| p.role != Role::Raft) {
        let Some(analytic) = p.analytic.as_ref() else {
            continue;
        };
        let a = analytic.support([0.0, 0.0, 1.0]);
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for v in p.tris.iter().flatten() {
            lo = lo.min(v[2]);
            hi = hi.max(v[2]);
        }
        let d = (lo - a.0).abs().max((a.1 - hi).abs());
        if worst.is_none_or(|(w, _)| d > w) {
            worst = Some((d, p.rec));
        }
        lowest = lowest.max((lo - a.0).abs());
        sideways = sideways.max(p.sag);
    }
    Json::obj([
        ("decides_nothing", Json::Bool(true)),
        ("largest_lowest_point_difference_mm", Json::Num(lowest)),
        (
            "largest_end_height_difference_mm",
            worst.map_or(Json::Null, |(d, _)| Json::Num(d)),
        ),
        (
            "largest_end_height_difference_record",
            worst.map_or(Json::Null, |(_, r)| Json::Uint(r as u64)),
        ),
        ("largest_sideways_difference_mm", Json::Num(sideways)),
    ])
}

/// The parts that stand on the model: feet whose written mesh touches it.
fn feet_of(scene: &Scene, patches: &PatchOut) -> BTreeSet<usize> {
    let touching: BTreeSet<usize> = patches
        .mesh()
        .comps
        .iter()
        .flat_map(|c| c.parts.clone())
        .collect();
    touching
        .into_iter()
        .filter(|&i| matches!(scene.parts[i].role, Role::FootBall | Role::FootCone))
        .collect()
}

pub(crate) mod model;
mod seat;
use seat::seat_findings;

/// H3.
#[must_use]
pub fn measure(scene: &Scene, patches: &PatchOut) -> Outcome {
    // The thresholds are read twice around the rounding of the section arithmetic (see
    // `Reading`). A pass needs the strict reading to be clean. A finding is a violation only
    // when the lenient reading has it too; otherwise it is unverified.
    let strict = Reading::strict();
    let lenient = Reading::lenient();
    let walked = match walk(scene, patches, strict) {
        Ok(w) => w,
        Err(why) => return Outcome::unverified(why),
    };
    let model_births = islands::births(scene, walked.z0);
    let mut found = assess(scene, &walked, &model_births, strict.depth);
    if !found.located.is_empty() {
        let sure = walk(scene, patches, lenient)
            .ok()
            .map(|w| assess(scene, &w, &model_births, lenient.depth).located);
        let (certain, doubtful): (Vec<Located>, Vec<Located>) = found
            .located
            .into_iter()
            .partition(|f| sure.as_ref().is_some_and(|s| same_finding(f, s)));
        found.located = certain;
        for d in doubtful {
            found.unverified.push(format!(
                "{} (found only when overlaps and slivers are read within the rounding of their thresholds)",
                d.reason
            ));
        }
    }
    // The end-event proof: the sample planes above cannot see an air gap that falls between
    // them (see the module comment).
    let seat = seat_findings(scene, patches, walked.z0, strict, lenient);
    let (seat_checked, seat_certain, seat_undecided) =
        (seat.checked, seat.located.len(), seat.unverified.len());
    found.located.extend(seat.located);
    found.unverified.extend(seat.unverified);
    let Assessed {
        located,
        unverified,
        listed,
        islands_note,
    } = found;
    let detail = Json::obj([
        (
            "end_event_seats",
            Json::obj([
                ("parts_checked", Json::Uint(seat_checked as u64)),
                ("unseated_certain", Json::Uint(seat_certain as u64)),
                ("unseated_undecided", Json::Uint(seat_undecided as u64)),
                ("thin_first_stretches", Json::Uint(seat.stretches as u64)),
                (
                    "seat_width_mm",
                    Json::Num((4.0 * OVERLAP_MM).max(2.0 * strict.depth)),
                ),
            ]),
        ),
        (
            "model_standing_feet_checked",
            Json::Uint(feet_of(scene, patches).len() as u64),
        ),
        (
            "model_slice_batches",
            Json::Uint(walked.model_slice_batches as u64),
        ),
        ("model_tiles", Json::Uint(walked.model_tiles as u64)),
        ("layer_height_mm", Json::Num(LAYER_MM)),
        ("layers", Json::Uint(walked.layers.len() as u64)),
        ("sample_planes", Json::Uint(walked.planes as u64)),
        ("pieces", Json::Uint(walked.pieces as u64)),
        ("unrooted_pieces", Json::Uint(walked.unrooted_pieces as u64)),
        ("overlap_mm", Json::Num(OVERLAP_MM)),
        ("rounding_bound_mm", Json::Num(ROUND_MM)),
        ("measured_on", Json::str(MEASURED_ON)),
        ("verdict_scope", Json::str(VERDICT_SCOPE)),
        ("analytic_information", analytic_information(scene)),
        ("model_islands", Json::Arr(listed)),
        ("unverified_items", Json::strings(&unverified)),
        (
            "model_rooting",
            Json::str(
                "model material is rooted only through local sections reached from earlier rooted sections or independently rooted support in the same layer; paths outside the local window remain unverified and future material is not seeded; a local contact does not by itself prove rooting",
            ),
        ),
    ]);
    let note = format!("{islands_note}; {seat_checked} start(s) proved seated");
    finish(scene, &walked, located, unverified, &note, detail)
}

fn finish(
    scene: &Scene,
    walked: &Walked,
    mut located: Vec<Located>,
    unverified: Vec<String>,
    islands_note: &str,
    detail: Json,
) -> Outcome {
    if !located.is_empty() {
        let n = located.len();
        located.truncate(MAX_LOCATED);
        return Outcome::violation(format!(
            "{n} support part(s) or model island(s) have no connection to the plate or to support printed in that layer or below"
        ))
        .with_located(located)
        .with_detail(detail);
    }
    if let Some(first) = unverified.first() {
        return Outcome::unverified(format!(
            "{} item(s) not decided; first: {first}",
            unverified.len()
        ))
        .with_detail(detail);
    }
    Outcome::pass(format!(
        "all {} support parts connect to the plate through support printed in the same layer or below, in all {} layers of {LAYER_MM} mm and at every sample plane; {islands_note}, each held by support",
        scene.parts.len(),
        walked.layers.len()
    ))
    .with_detail(detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch;
    use crate::report::Status;
    use crate::testkit::{self as kit, test_limits};

    /// A model whose lowest point is at local z = -5, translate Z 5: the plate is at z = -10.
    fn scene(parts: Vec<Part>) -> Scene {
        let model: Vec<Tri> = vec![[[-1.0, -1.0, -5.0], [1.0, -1.0, -5.0], [0.0, 1.0, 0.0]]];
        Scene::from_parts(&test_limits(), model, [0.0, 0.0, 5.0], parts)
    }

    /// The support-side result alone: the violating parts, by record.
    fn floating(parts: Vec<Part>) -> Vec<(usize, u32)> {
        let s = scene(parts);
        let p = patch::measure(&s);
        let w = walk(&s, &p, Reading::strict()).expect("walkable");
        w.located
            .iter()
            .map(|l| {
                (
                    l.location.record.expect("record") as usize,
                    l.location.layer.expect("layer"),
                )
            })
            .collect()
    }

    #[test]
    fn a_column_on_a_pad_with_a_brace_that_starts_on_the_column_is_continuous() {
        let pad = kit::raft_pad(0, [0.0, 0.0], -9.8);
        let column = kit::part(1, 3, [0.0, 0.0, -9.8], [0.0, 0.0, -5.0], 0.3, 0.3);
        // A 45-degree brace that starts on the column axis and rises away from it.
        let brace = kit::part(2, 3, [0.0, 0.0, -8.2], [3.0, 0.0, -5.2], 0.15, 0.15);
        assert!(floating(vec![pad, column, brace]).is_empty());
    }

    #[test]
    fn a_brace_that_starts_one_layer_before_its_carrier_is_a_violation() {
        let pad = kit::raft_pad(0, [0.0, 0.0], -9.8);
        let column = kit::part(1, 3, [0.0, 0.0, -9.8], [0.0, 0.0, -5.0], 0.3, 0.3);
        // The brace is joined to the column at its top end but its other end hangs in the
        // air, 1.8 mm above the plate with nothing under it: its lowest layers float.
        let brace = kit::part(2, 3, [3.0, 0.0, -8.2], [0.0, 0.0, -5.2], 0.15, 0.15);
        let f = floating(vec![pad, column, brace]);
        assert_eq!(f[0].0, 2, "{f:?}");
        // The first floating layer is reported: the brace's lowest point is at plate + 1.8 mm,
        // layer 36 at 0.05 mm.
        assert!((33..=37).contains(&f[0].1), "{f:?}");
    }

    #[test]
    fn a_ball_pole_that_hangs_below_its_cone_is_floating() {
        let pad = kit::raft_pad(0, [0.0, 0.0], -9.8);
        let column = kit::part(1, 3, [0.0, 0.0, -9.8], [0.0, 0.0, -7.0], 0.3, 0.3);
        // A ball far from the column: nothing under it.
        let ball = kit::part(2, 9, [4.0, 0.0, -7.3], [4.0, 0.0, -6.7], 0.3, 0.3);
        assert_eq!(floating(vec![pad, column, ball])[0].0, 2);
    }

    #[test]
    fn a_ball_at_the_end_of_an_arm_floats_unless_the_arm_covers_its_pole() {
        // A rooted column with a horizontal arm at z = -8 ending at a ball centre. The ball's
        // lowest layers lie under the arm unless the arm is thick enough to contain them.
        let arm_and_ball = |arm_r: f32, ball_r: f32| {
            let pad = kit::raft_pad(0, [0.0, 0.0], -9.8);
            let column = kit::part(1, 3, [0.0, 0.0, -9.8], [0.0, 0.0, -7.0], 0.4, 0.4);
            let arm = kit::part(2, 3, [0.0, 0.0, -8.0], [3.0, 0.0, -8.0], arm_r, arm_r);
            let ball = kit::part(
                3,
                9,
                [3.0, 0.0, -8.0 - ball_r],
                [3.0, 0.0, -8.0 + ball_r],
                ball_r,
                ball_r,
            );
            floating(vec![pad, column, arm, ball])
        };
        // Arm radius 0.15 under a ball of radius 0.3: the ball's lower cap floats.
        let thin = arm_and_ball(0.15, 0.3);
        assert_eq!(thin[0].0, 3, "{thin:?}");
        // Arm radius 0.3 under a ball of radius 0.28: the arm contains the ball's lower half.
        assert!(arm_and_ball(0.3, 0.28).is_empty());
    }

    #[test]
    fn an_air_gap_of_0_02_mm_between_coaxial_posts_in_one_layer_is_a_violation() {
        // A plate-rooted post ends 3.01 mm above the plate; a coaxial post begins at 3.03 mm.
        // Both are in layer 60 [3.00, 3.05), at different heights: they are never in one
        // sample plane, and at the middle plane (3.025) nothing is there at all.
        let pad = kit::raft_pad(0, [0.0, 0.0], -9.8);
        let low = kit::part(1, 3, [0.0, 0.0, -9.8], [0.0, 0.0, -6.99], 0.3, 0.3);
        let high = kit::part(2, 3, [0.0, 0.0, -6.97], [0.0, 0.0, -5.0], 0.3, 0.3);
        let f = floating(vec![pad, low, high]);
        assert_eq!(f.first().map(|x| x.0), Some(2), "{f:?}");
        // Closing the air gap makes it continuous.
        let pad = kit::raft_pad(0, [0.0, 0.0], -9.8);
        let low = kit::part(1, 3, [0.0, 0.0, -9.8], [0.0, 0.0, -6.99], 0.3, 0.3);
        let high = kit::part(2, 3, [0.0, 0.0, -6.995], [0.0, 0.0, -5.0], 0.3, 0.3);
        assert!(floating(vec![pad, low, high]).is_empty());
    }

    #[test]
    fn an_overlap_of_the_written_meshes_decides_and_the_analytic_difference_is_information() {
        // Rod B is held to the plate-rooted column A only by a section overlap of about 0.0105
        // mm in the written meshes: over the 0.01 threshold by 0.0005 mm. The analytic rods
        // stand up to their polygon sagitta (0.0058 mm at radius 0.3) outside the written
        // 16-gons, so the analytic overlap could be under the threshold; the verdict is the
        // written meshes' (a pass), and the difference is reported as information.
        let build = |d: f32| {
            vec![
                kit::raft_pad(0, [0.0, 3000.0], -9.8),
                kit::part(1, 3, [0.0, 3000.0, -9.85], [0.0, 3000.0, -4.95], 0.3, 0.3),
                kit::part(2, 3, [d, 3000.0, -9.0], [d, 3000.0, -6.0], 0.3, 0.3),
            ]
        };
        // Find the offset at which the written sections overlap by 0.0105 mm to 0.0125 mm.
        let mut chosen = None;
        for k in 0..400 {
            let d = 0.55 + 0.0002 * k as f32;
            let parts = build(d);
            let (a, b) = (
                section(&parts[1].tris, -7.0, 0.0).expect("section"),
                section(&parts[2].tris, -7.0, 0.0).expect("section"),
            );
            if overlap_by(&a, &b, 0.0105) && !overlap_by(&a, &b, 0.0125) {
                chosen = Some(d);
                break;
            }
        }
        let d = chosen.expect("an offset with the wanted overlap");
        // A closed model over the rods, its underside held by the column A.
        let closed = |parts: Vec<Part>| {
            Scene::from_parts(
                &test_limits(),
                kit::closed_box([-5.0, 2990.0, -5.0], [5.0, 3010.0, 5.0]),
                [0.0, 0.0, 5.0],
                parts,
            )
        };
        let s = closed(build(d));
        let p = patch::measure(&s);
        // Both readings of the threshold find B connected: the written overlap is over it by
        // far more than the rounding.
        for reading in [Reading::strict(), Reading::lenient()] {
            let w = walk(&s, &p, reading).expect("walkable");
            assert!(w.located.is_empty(), "{:?}", w.located);
        }
        let o = measure(&s, &p);
        assert_eq!(o.status, Status::Pass, "{o:?}");
        // The analytic difference (a sagitta far over the 0.0005 mm margin) is information.
        let info = o.detail.get("analytic_information").expect("information");
        let Some(Json::Num(sideways)) = info.get("largest_sideways_difference_mm") else {
            panic!("{info:?}");
        };
        assert!(*sideways > 5e-3, "{sideways}");
        assert_eq!(info.get("decides_nothing"), Some(&Json::Bool(true)));
        // Overlapped deeply (0.1 mm more) it is rooted too: a pass.
        let s = closed(build(d - 0.1));
        let p = patch::measure(&s);
        let o = measure(&s, &p);
        assert_eq!(o.status, Status::Pass, "{o:?}");
    }

    #[test]
    fn an_overlap_within_the_rounding_of_the_depth_threshold_is_read_both_ways() {
        // Two squares that overlap by 0.01 + 5e-10 mm: connected under the lenient reading,
        // not under the strict one, so a pass and a certain violation are both out.
        let square =
            |x: f64| Sec::from_poly(vec![[x, 0.0], [x + 1.0, 0.0], [x + 1.0, 1.0], [x, 1.0]]);
        let (a, b) = (square(0.0), square(1.0 - (OVERLAP_MM + 5e-10)));
        assert!(!overlap_by(&a, &b, Reading::strict().depth));
        assert!(overlap_by(&a, &b, Reading::lenient().depth));
        // A clear overlap and a clear gap read the same under both.
        let near = square(1.0 - 2.0 * OVERLAP_MM);
        let far = square(1.0 + OVERLAP_MM);
        for r in [Reading::strict(), Reading::lenient()] {
            assert!(overlap_by(&a, &near, r.depth));
            assert!(!overlap_by(&a, &far, r.depth));
        }
    }

    /// Two coaxial radius 0.3 rods at y = 3000 whose fields leave an end gap of 8e-4 mm
    /// (`lo_top` to `hi_bot`), each written mesh end moved `shift` toward the other (the
    /// canonical mesh allows 2 units in the last place, 4.9e-4 mm, there), under a closed model
    /// held by the upper rod.
    fn end_gap_scene(shift: f32) -> Scene {
        let (lo_top, hi_bot) = (-6.9755f32, -6.9747f32);
        let lo = ([0.0, 3000.0, -9.85], [0.0, 3000.0, lo_top]);
        let hi = ([0.0, 3000.0, hi_bot], [0.0, 3000.0, -4.95]);
        let low = kit::part_with_mesh(
            1,
            &kit::rec(3, lo.0, lo.1, 0.3, 0.3),
            &kit::frustum_coords(lo.0, [0.0, 3000.0, lo_top + shift], 0.3, 0.3),
        );
        let high = kit::part_with_mesh(
            2,
            &kit::rec(3, hi.0, hi.1, 0.3, 0.3),
            &kit::frustum_coords([0.0, 3000.0, hi_bot - shift], hi.1, 0.3, 0.3),
        );
        assert!(low.is_valid(), "{:?}", low.invalid);
        assert!(high.is_valid(), "{:?}", high.invalid);
        Scene::from_parts(
            &test_limits(),
            kit::closed_box([-5.0, 2990.0, -5.0], [5.0, 3010.0, 5.0]),
            [0.0, 0.0, 5.0],
            vec![kit::raft_pad(0, [0.0, 3000.0], -9.8), low, high],
        )
    }

    #[test]
    fn an_end_gap_of_the_fields_that_the_written_meshes_close_is_decided_on_the_meshes() {
        // Round-3 review: the fields leave a gap that the written meshes (valid, inside 2
        // units in the last place) close by 1e-4 mm. The verdict is measured on the written
        // triangles: they are continuous, so it is a pass; the gap the analytic solids keep
        // appears as information (the ends differ in height by at least the 4.5e-4 mm shift).
        let s = end_gap_scene(4.5e-4);
        let (lo, hi) = (&s.parts[1], &s.parts[2]);
        let top = lo
            .tris
            .iter()
            .flatten()
            .map(|v| v[2])
            .fold(f64::MIN, f64::max);
        let bottom = hi
            .tris
            .iter()
            .flatten()
            .map(|v| v[2])
            .fold(f64::MAX, f64::min);
        assert!(top > bottom, "the written meshes overlap: {top} > {bottom}");
        let p = patch::measure(&s);
        let o = measure(&s, &p);
        assert_eq!(o.status, Status::Pass, "{o:?}");
        let info = o.detail.get("analytic_information").expect("information");
        let Some(Json::Num(d)) = info.get("largest_end_height_difference_mm") else {
            panic!("{info:?}");
        };
        assert!(
            *d >= 4.0e-4,
            "the analytic ends differ from the meshes' by {d}"
        );
        // With the ends where the fields put them the written meshes have the real gap too:
        // a violation.
        let s = end_gap_scene(0.0);
        let p = patch::measure(&s);
        let o = measure(&s, &p);
        assert_eq!(o.status, Status::Violation, "{o:?}");
    }

    /// Round-5 review: radius 0.3 rods on the axis x = y = 0, the lower from z = -9.85 to
    /// -6.9755, the upper from there plus `gap` to -4.95, on a pad at -9.8 and under a closed
    /// cube lifted 5 mm that the upper rod holds. A negative `gap` overlaps the rods.
    fn coaxial_rods(gap: f32) -> Scene {
        let top = -6.9755f32;
        cube_scene(vec![
            kit::raft_pad(0, [0.0, 0.0], -9.8),
            kit::part(1, 3, [0.0, 0.0, -9.85], [0.0, 0.0, top], 0.3, 0.3),
            kit::part(2, 3, [0.0, 0.0, top + gap], [0.0, 0.0, -4.95], 0.3, 0.3),
        ])
    }

    fn status_of(s: &Scene) -> Status {
        let p = patch::measure(s);
        measure(s, &p).status
    }

    #[test]
    fn an_end_face_gap_between_coaxial_rods_is_never_a_pass() {
        // The sample planes of the walk lie at least 1e-4 mm from a part end, so a gap of 1e-6
        // to 9e-5 mm fell between them and passed. The start of the upper rod is now proved to
        // stand on a part alive at its start: a gap of the written meshes beyond `TOUCH_MM`
        // (2e-6 mm) is a violation, one inside it is undecided, and neither is a pass.
        for gap in [
            9.5e-7f32, 1.0e-5, 5.0e-5, 9.0e-5, 1.5e-4, 3.0e-4, 1.0e-3, 0.02,
        ] {
            let s = coaxial_rods(gap);
            let o = {
                let p = patch::measure(&s);
                measure(&s, &p)
            };
            assert_ne!(o.status, Status::Pass, "gap {gap}: {o:?}");
            if gap >= 1.0e-5 {
                assert_eq!(o.status, Status::Violation, "gap {gap}: {o:?}");
                assert!(
                    o.located.iter().any(|l| l.location.record == Some(2)),
                    "gap {gap}: {o:?}"
                );
            }
        }
        // Rods that overlap by 1e-3, 1e-2 and 5e-2 mm are one post: a pass.
        for overlap in [1.0e-3f32, 1.0e-2, 5.0e-2] {
            let o = status_of(&coaxial_rods(-overlap));
            assert_eq!(o, Status::Pass, "overlap {overlap}");
        }
        // Faces that meet exactly are not an overlap of the written meshes: undecided.
        assert_eq!(status_of(&coaxial_rods(0.0)), Status::Unverified);
    }

    #[test]
    fn two_floating_parts_that_start_together_do_not_seat_each_other() {
        // Round-5 review: a second upper rod, 0.1 mm to the side, starts at the same height as
        // the first. Each overlaps the other at the height where the other is wide enough, so
        // "some other part is alive there" held for both; neither is reached from the plate,
        // and the end-face gap under them lies between every sample plane.
        let top = -6.9755f32;
        for gap in [1.0e-5f32, 5.0e-5, 9.0e-5, 1.5e-4] {
            let s = cube_scene(vec![
                kit::raft_pad(0, [0.0, 0.0], -9.8),
                kit::part(1, 3, [0.0, 0.0, -9.85], [0.0, 0.0, top], 0.3, 0.3),
                kit::part(2, 3, [0.0, 0.0, top + gap], [0.0, 0.0, -4.95], 0.3, 0.3),
                kit::part(3, 3, [0.1, 0.0, top + gap], [0.1, 0.0, -6.0], 0.3, 0.3),
            ]);
            let p = patch::measure(&s);
            let o = measure(&s, &p);
            assert_ne!(o.status, Status::Pass, "gap {gap}: {o:?}");
            if gap >= 1.0e-5 {
                assert_eq!(o.status, Status::Violation, "gap {gap}: {o:?}");
                for rec in [2, 3] {
                    assert!(
                        o.located.iter().any(|l| l.location.record == Some(rec)),
                        "gap {gap}, record {rec}: {o:?}"
                    );
                }
            }
        }
        // The same two upper rods on a lower rod that overlaps both are seated by it, and the
        // overlapping pair add nothing wrong: a pass.
        let s = cube_scene(vec![
            kit::raft_pad(0, [0.0, 0.0], -9.8),
            kit::part(1, 3, [0.0, 0.0, -9.85], [0.0, 0.0, top], 0.3, 0.3),
            kit::part(2, 3, [0.0, 0.0, top - 0.05], [0.0, 0.0, -4.95], 0.3, 0.3),
            kit::part(3, 3, [0.1, 0.0, top - 0.05], [0.1, 0.0, -6.0], 0.3, 0.3),
        ]);
        assert_eq!(status_of(&s), Status::Pass);
    }

    #[test]
    fn a_start_in_the_air_over_a_gap_that_no_sample_plane_sees_is_a_violation_in_its_own_layer() {
        // The same rods, the upper one ending in the layer of the gap: no later layer relies
        // on the gap, so only the start of the upper rod can show it.
        let top = -6.9755f32;
        let s = cube_scene(vec![
            kit::raft_pad(0, [0.0, 0.0], -9.8),
            kit::part(1, 3, [0.0, 0.0, -9.85], [0.0, 0.0, top], 0.3, 0.3),
            // A short rod above the gap that stands clear of the cube.
            kit::part(
                2,
                3,
                [0.0, 0.0, top + 2.0e-5],
                [0.0, 0.0, top + 0.01],
                0.3,
                0.3,
            ),
            kit::part(3, 3, [0.0, 0.0, -6.0], [0.0, 0.0, -4.95], 0.3, 0.3),
        ]);
        let p = patch::measure(&s);
        let o = measure(&s, &p);
        assert_ne!(o.status, Status::Pass, "{o:?}");
        assert!(
            o.located.iter().any(|l| l.location.record == Some(2)),
            "{o:?}"
        );
    }

    #[test]
    fn model_standing_foot_uses_earlier_sliced_material_not_its_future_attachment() {
        let check = |bottom: f32| {
            let mut parts = vec![
                kit::raft_pad(0, [0.0, 0.0], -4.8),
                kit::part(1, 3, [0.0, 0.0, -4.85], [0.0, 0.0, 0.1], 0.4, 0.4),
            ];
            parts.push(kit::part(
                2,
                8,
                [1.0, 0.0, bottom],
                [1.0, 0.0, bottom + 0.6],
                0.3,
                0.3,
            ));
            parts.push(kit::part(
                3,
                3,
                [1.0, 0.0, bottom + 0.2],
                [1.0, 0.0, 4.0],
                0.2,
                0.2,
            ));
            let s = Scene::from_parts(
                &test_limits(),
                kit::closed_box([-2.0, -2.0, 0.0], [2.0, 2.0, 2.0]),
                [0.0, 0.0, 5.0],
                parts,
            );
            let p = patch::measure(&s);
            let w = walk(&s, &p, Reading::strict()).expect("material can be sliced");
            (
                w.located.iter().any(|l| l.location.record == Some(2)),
                measure(&s, &p),
            )
        };
        let (floats, good) = check(1.7);
        assert!(!floats, "{good:?}");
        assert_eq!(good.status, Status::Pass, "{good:?}");
        let (floats, bad) = check(-0.3);
        assert!(floats, "{bad:?}");
        assert_eq!(bad.status, Status::Violation, "{bad:?}");
    }

    #[test]
    fn a_separate_root_cannot_hide_a_future_model_attachment_on_a_foot() {
        let s = Scene::from_parts(
            &test_limits(),
            kit::closed_box([-2.0, -2.0, 0.0], [2.0, 2.0, 2.0]),
            [0.0, 0.0, 5.0],
            vec![
                kit::raft_pad(0, [0.0, 0.0], -4.8),
                kit::part(1, 3, [0.0, 0.0, -4.85], [0.0, 0.0, 0.1], 0.4, 0.4),
                // The foot is carried by this plate-rooted rod, but touches the model only later.
                kit::part(2, 8, [0.0, 0.0, -0.3], [0.0, 0.0, 0.3], 0.3, 0.3),
            ],
        );
        let out = measure(&s, &patch::measure(&s));
        assert_eq!(out.status, Status::Violation, "{out:?}");
        assert!(
            out.located
                .iter()
                .any(|l| l.location.record == Some(2)
                    && l.reason.contains("first printable section")),
            "{out:?}"
        );
    }

    #[test]
    fn an_inner_wall_printed_before_the_foot_can_root_it() {
        let mut model = kit::closed_box([-3.0, -3.0, 0.0], [3.0, 3.0, 6.0]);
        model.extend(
            kit::closed_box([-2.0, -2.0, 1.0], [2.0, 2.0, 5.0])
                .into_iter()
                .map(|[a, b, c]| [a, c, b]),
        );
        let s = Scene::from_parts(
            &test_limits(),
            model,
            [0.0, 0.0, 5.0],
            vec![
                kit::raft_pad(0, [0.0, 0.0], -4.8),
                kit::part(1, 3, [0.0, 0.0, -4.85], [0.0, 0.0, 0.1], 0.4, 0.4),
                kit::part(2, 8, [1.0, 0.0, 0.7], [1.0, 0.0, 1.3], 0.3, 0.3),
                kit::part(3, 3, [1.0, 0.0, 0.9], [1.0, 0.0, 3.0], 0.2, 0.2),
            ],
        );
        let p = patch::measure(&s);
        let out = measure(&s, &p);
        assert_eq!(out.status, Status::Pass, "{out:?}");
        assert_eq!(
            out.detail.get("model_standing_feet_checked"),
            Some(&Json::Uint(1))
        );
        // H3 proves order, not drainage: this deliberately sealed control still fails H5.
        assert_eq!(crate::h5::measure(&s).status, Status::Violation);
    }

    #[test]
    fn no_supports_means_unverified_not_pass() {
        let s = scene(Vec::new());
        let p = patch::measure(&s);
        assert_eq!(measure(&s, &p).status, Status::Unverified);
    }

    /// A closed 10 mm cube centred at the origin lifted 5 mm: the plate is at local z = -10.
    fn cube_scene(parts: Vec<Part>) -> Scene {
        Scene::from_parts(
            &test_limits(),
            kit::closed_box([-5.0; 3], [5.0; 3]),
            [0.0, 0.0, 5.0],
            parts,
        )
    }

    /// A pad, a post and a tip ball centred 0.05 mm below the cube's underside, at x.
    fn head_on_cube(x: f32) -> Vec<Part> {
        let pad = kit::raft_pad(0, [x, 0.0], -9.8);
        // The column starts 0.05 mm inside the pad (the generator's overlap), not exactly on its
        // top face: a start that falls on a layer boundary within the rounding of the file is
        // not told from a hair of air by the sample planes.
        let column = kit::part(1, 3, [x, 0.0, -9.85], [x, 0.0, -5.1], 0.25, 0.25);
        let tip = kit::part(2, 7, [x, 0.0, -5.3], [x, 0.0, -4.8], 0.25, 0.25);
        vec![pad, column, tip]
    }

    #[test]
    fn a_cube_floating_over_the_plate_with_no_support_under_it_is_a_violation_and_a_held_one_passes()
     {
        // The posts stand clear of the cube: its flat underside is an island nothing holds.
        let s = cube_scene(head_on_cube(8.0));
        let p = patch::measure(&s);
        let o = measure(&s, &p);
        assert_eq!(o.status, Status::Violation, "{o:?}");
        assert!(o.reason.contains("model island"), "{}", o.reason);
        // A head under its middle holds the underside.
        let s = cube_scene(head_on_cube(0.0));
        let p = patch::measure(&s);
        let o = measure(&s, &p);
        assert_eq!(o.status, Status::Pass, "{o:?}");
    }
}
