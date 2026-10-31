//! Contact patches on the model (H1) and the distance between them (H2).
//!
//! A **patch** is the connected area of model surface that a support touches or enters. For
//! one model triangle `T` and one support part `S` the area is `T ∩ S`, a convex polygon.
//! The checker brackets it from both sides with polygons it can clip exactly:
//!
//! * the **inner** polygon is `T` clipped by a polytope that lies inside `S`, so it is a
//!   subset of the true region (its area is a lower bound);
//! * the **outer** polygon is `T` clipped by a polytope that contains `S`, so it is a superset
//!   (an upper bound).
//!
//! Two representations are measured. The **written mesh** (exact half-spaces of its faces,
//! moved by a 2e-6 mm slack for the planarity of its quads, and by the record's measured
//! deviation from its canonical mesh) is the verdict source: the rules
//! read the triangles the file holds (`PatchOut::mesh`), valid for ChituBox v1.4.0 if it
//! slices the stored support meshes (pending the H8 differential slice test). The
//! **analytic** solid the record fields name (a ball is a disc in the plane of `T`; a
//! frustum is bracketed by 360-sided polytopes whose faces are chords and tangents of the
//! cone) is measured for information only (`PatchOut::analytic`): no verdict reads it. A chain's parts
//! (ball, cone, joint) are counted once: the union of convex polygons is exact by
//! inclusion-exclusion. Patches touching across a seam between triangles are one patch. A
//! model plane is rejected for a part only by the extent of the representation being measured
//! (the written mesh may stand outside the analytic solid by its validated envelope, so the
//! analytic extent never rejects a mesh candidate).
//! Everything is f64; the arithmetic error (about 1e-15 relative) is far below the
//! bracketing width (below 1e-4 mm2 for any contact up to 0.8 mm across).

use crate::par::par_map;
use crate::parts::{Geom, Part, Role};
use crate::poly::{self, Poly};
use crate::scene::Scene;
use crate::solid::{HalfSpace, Solid, frustum_polytope};
use crate::tri::Tri;
use crate::v3::{Aabb, V, add, cross, dot, frame, mul, sub, unit};

/// Sides of the polytopes that bracket an analytic cone or disc.
pub const SIDES: usize = 360;
/// Patches closer than this are fused (H2), mm (design: the measurement resolution).
pub const FUSE_MM: f64 = 0.010;
/// Slack for the planarity of the written mesh's quads, mm.
const MESH_SLACK: f64 = 2e-6;
/// Polygons this close are touching, mm.
const TOUCH_MM: f64 = 1e-7;
/// Rounding allowance added to every upper area, mm2.
const FP_SLACK: f64 = 1e-9;

/// Which solid is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rep {
    /// The solid the record fields name.
    Analytic,
    /// The closed mesh the file holds.
    Mesh,
}

impl Rep {
    pub const ALL: [Rep; 2] = [Rep::Analytic, Rep::Mesh];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Analytic => "analytic",
            Self::Mesh => "mesh",
        }
    }
}

/// One part's bracketing polygons on one model triangle.
#[derive(Debug, Clone)]
struct PartPoly {
    part: usize,
    tri: u32,
    inner: Poly,
    outer: Poly,
    normal: V,
}

/// One connected group of a chain's polygons on one triangle.
#[derive(Debug, Clone)]
pub struct Piece {
    pub chain: usize,
    pub tri: u32,
    pub parts: Vec<usize>,
    pub inner: Vec<Poly>,
    pub outer: Vec<Poly>,
    /// Area of the union of the inner polygons (lower bound), mm2.
    pub lo: f64,
    /// Area of the union of the outer polygons (upper bound), mm2.
    pub hi: f64,
}

/// A connected patch (one chain).
#[derive(Debug, Clone)]
pub struct Comp {
    pub chain: usize,
    /// Upper bound of the area if everything that may touch does, mm2.
    pub hi: f64,
    /// Sum of the inner areas of the pieces, mm2.
    pub lo: f64,
    /// Lower bound of the area of the part of it that certainly touches, mm2.
    pub certain_lo: f64,
    pub parts: Vec<usize>,
    /// A point on it (a vertex of an outer polygon).
    pub at: V,
    /// Indices of its pieces in the representation's piece list.
    pub piece_ids: Vec<usize>,
    /// Its pieces touch only by their outer polygons for part of it: whether it is one patch
    /// or several is not certain (the favourable reading is never taken).
    pub split: bool,
}

/// The closest approach of two distinct patches (of different chains, or of one chain on
/// different model sheets or places).
#[derive(Debug, Clone)]
pub struct PairGap {
    /// The chains of the two patches (equal for two patches of one chain).
    pub a: usize,
    pub b: usize,
    /// Lower bound of the distance between the patches, mm.
    pub dlb: f64,
    /// Upper bound, mm (infinite when a patch has no certain area).
    pub dub: f64,
    pub at: V,
}

/// The patches of one representation.
#[derive(Debug, Clone)]
pub struct RepOut {
    pub rep: Rep,
    pub comps: Vec<Comp>,
    pub pairs: Vec<PairGap>,
    /// Largest upper bound of any patch, mm2.
    pub max_hi: f64,
    /// Largest lower bound of any certainly-connected patch, mm2.
    pub max_certain_lo: f64,
    /// Chains whose patches may be one (a union too big to bracket tightly).
    pub loose: Vec<usize>,
}

/// Everything the patch measurement found.
#[derive(Debug, Clone)]
pub struct PatchOut {
    /// `[analytic, mesh]`, in the order of `Rep::ALL`. Read them through `mesh` (the verdict
    /// source) and `analytic` (information only).
    pub reps: [RepOut; 2],
    /// Why the measurement is not decided, if it is not.
    pub unverified: Vec<String>,
}

impl PatchOut {
    /// The patches of the written mesh: the only representation a verdict is read from.
    #[must_use]
    pub fn mesh(&self) -> &RepOut {
        &self.reps[1]
    }

    /// The patches of the solid the record fields name. Information only: it decides nothing.
    #[must_use]
    pub fn analytic(&self) -> &RepOut {
        &self.reps[0]
    }
}

fn tri_poly(t: &Tri) -> Poly {
    t.to_vec()
}

fn tri_plane(t: &Tri) -> Option<(V, f64)> {
    let n = unit(cross(sub(t[1], t[0]), sub(t[2], t[0])));
    (crate::v3::len(n) > 0.0).then(|| (n, dot(n, t[0])))
}

/// In-plane half-spaces of a regular `SIDES`-gon around `q` with circumradius-or-apothem `rho`.
fn disc_spaces(n: V, q: V, rho: f64, inner: bool) -> Vec<HalfSpace> {
    let (e1, e2) = frame(n);
    let k = if inner {
        (std::f64::consts::PI / SIDES as f64).cos()
    } else {
        1.0
    };
    (0..SIDES)
        .map(|j| {
            let phi = std::f64::consts::TAU * (j as f64 + 0.5) / SIDES as f64;
            let w = add(mul(e1, phi.cos()), mul(e2, phi.sin()));
            (w, dot(w, q) + rho * k)
        })
        .collect()
}

/// The planes moved outwards (`sign = 1`) or inwards (`-1`) by their slack and by the record's
/// measured deviation `dev` from its canonical mesh (a hull within `dev` of the written one
/// lies in the written planes moved out by `dev`, and holds the written planes moved in by
/// `dev`), so an area or a separation holds for the canonical mesh too.
fn mesh_spaces(planes: &[HalfSpace], slack: &[f64], dev: f64, sign: f64) -> Vec<HalfSpace> {
    planes
        .iter()
        .zip(slack)
        .map(|((n, o), s)| (*n, o + sign * (MESH_SLACK + s + dev)))
        .collect()
}

/// How far the outer polytope of a written mesh (its planes moved out by their slack) can
/// stand beyond the extent of the mesh's vertices, mm. Neighbouring faces of every native shape
/// turn by at least 11.25 degrees, so a corner moves out by at most `1 / sin(5.6 deg)` (under
/// 11) times the plane shift.
fn mesh_pad(mesh: &Solid) -> f64 {
    let Solid::Poly { slack, dev, .. } = mesh else {
        return 1e-3;
    };
    let worst = slack.iter().copied().fold(0.0, f64::max);
    1e-6 + 11.0 * (MESH_SLACK + worst + dev)
}

/// The (inner, outer) half-spaces of `part` in representation `rep` for the plane `n . x = o`,
/// or `None` when the plane misses the part.
fn spaces(part: &Part, rep: Rep, n: V, o: f64) -> Option<(Vec<HalfSpace>, Vec<HalfSpace>)> {
    let analytic = part.analytic.as_ref()?;
    let mesh = part.mesh.as_ref()?;
    // The plane is rejected only against the extent of the representation being measured. The
    // written mesh may stand outside the analytic solid by its vertex rounding, so the
    // analytic extent never rejects a mesh candidate.
    let (lo, hi, pad) = if rep == Rep::Mesh || part.role == Role::Raft {
        let (lo, hi) = mesh.support(n);
        (lo, hi, mesh_pad(mesh))
    } else {
        let (lo, hi) = analytic.support(n);
        (lo, hi, 1e-6)
    };
    if o < lo - pad || o > hi + pad {
        return None;
    }
    let from_mesh = |s: &Solid| match s {
        Solid::Poly {
            planes, slack, dev, ..
        } => Some((
            mesh_spaces(planes, slack, *dev, -1.0),
            mesh_spaces(planes, slack, *dev, 1.0),
        )),
        _ => None,
    };
    if rep == Rep::Mesh || part.role == Role::Raft {
        return from_mesh(mesh);
    }
    match (&part.geom, analytic) {
        (Geom::Ball { c, r }, _) => {
            let s = dot(n, *c) - o;
            let rho = (r * r - s * s).max(0.0).sqrt();
            if rho < 1e-9 {
                return None;
            }
            let q = sub(*c, mul(n, s));
            Some((disc_spaces(n, q, rho, true), disc_spaces(n, q, rho, false)))
        }
        (Geom::Frust { a, ra, rb, .. }, Solid::Frust { u, l, .. }) => {
            let frame = (*a, *u, *l);
            Some((
                frustum_polytope(frame, (*ra, *rb), SIDES, true),
                frustum_polytope(frame, (*ra, *rb), SIDES, false),
            ))
        }
        _ => from_mesh(mesh),
    }
}

/// Every (part, triangle) polygon pair of one part, in both representations.
fn part_polys(scene: &Scene, pi: usize) -> [Vec<PartPoly>; 2] {
    let part = &scene.parts[pi];
    let mut out = [Vec::new(), Vec::new()];
    if !part.is_valid() {
        return out;
    }
    let cand = scene.model_near(&part.bbox.inflated(1e-6));
    for ti in cand {
        let t = &scene.model[ti as usize];
        let Some((n, o)) = tri_plane(t) else { continue };
        for (slot, rep) in out.iter_mut().zip(Rep::ALL) {
            let Some((ihs, ohs)) = spaces(part, rep, n, o) else {
                continue;
            };
            let outer = poly::clip_all(&tri_poly(t), &ohs);
            if poly::area(&outer) <= 1e-12 {
                continue;
            }
            let inner = poly::clip_all(&tri_poly(t), &ihs);
            slot.push(PartPoly {
                part: pi,
                tri: ti,
                inner,
                outer,
                normal: n,
            });
        }
    }
    out
}

/// Group the polygons of one chain on one triangle into connected pieces.
fn pieces_of(chain: usize, group: &[&PartPoly]) -> Vec<Piece> {
    let n = group.len();
    let mut uf: Vec<usize> = (0..n).collect();
    fn find(u: &mut [usize], mut x: usize) -> usize {
        while u[x] != x {
            u[x] = u[u[x]];
            x = u[x];
        }
        x
    }
    for i in 0..n {
        for j in i + 1..n {
            if poly::distance(&group[i].outer, &group[j].outer) <= TOUCH_MM {
                let (a, b) = (find(&mut uf, i), find(&mut uf, j));
                uf[a] = b;
            }
        }
    }
    let mut roots: Vec<usize> = (0..n).map(|i| find(&mut uf, i)).collect();
    roots.sort_unstable();
    roots.dedup();
    roots
        .into_iter()
        .map(|r| {
            let members: Vec<&&PartPoly> = (0..n)
                .filter(|&i| find(&mut uf, i) == r)
                .map(|i| &group[i])
                .collect();
            let inner: Vec<Poly> = members
                .iter()
                .filter(|m| poly::area(&m.inner) > 0.0)
                .map(|m| m.inner.clone())
                .collect();
            let outer: Vec<Poly> = members.iter().map(|m| m.outer.clone()).collect();
            let nrm = members[0].normal;
            let lo = poly::union_area(&inner, nrm)
                .unwrap_or_else(|| inner.iter().map(|p| poly::area(p)).fold(0.0, f64::max));
            let hi = poly::union_area(&outer, nrm)
                .unwrap_or_else(|| outer.iter().map(|p| poly::area(p)).sum())
                + FP_SLACK;
            Piece {
                chain,
                tri: members[0].tri,
                parts: members.iter().map(|m| m.part).collect(),
                inner,
                outer,
                lo,
                hi,
            }
        })
        .collect()
}

fn min_poly_dist(a: &[Poly], b: &[Poly]) -> f64 {
    let mut best = f64::INFINITY;
    for p in a {
        for q in b {
            best = best.min(poly::distance(p, q));
        }
    }
    best
}

fn piece_box(p: &Piece) -> Aabb {
    Aabb::of_points(p.outer.iter().flatten().copied())
}

/// Union-find over `n` items.
fn components(n: usize, edge: impl Fn(usize, usize) -> bool) -> Vec<usize> {
    let mut uf: Vec<usize> = (0..n).collect();
    fn find(u: &mut [usize], mut x: usize) -> usize {
        while u[x] != x {
            u[x] = u[u[x]];
            x = u[x];
        }
        x
    }
    for i in 0..n {
        for j in i + 1..n {
            if edge(i, j) {
                let (a, b) = (find(&mut uf, i), find(&mut uf, j));
                uf[a] = b;
            }
        }
    }
    (0..n).map(|i| find(&mut uf, i)).collect()
}

fn comps_of(chain: usize, ids: &[usize], all: &[Piece]) -> (Vec<Comp>, bool) {
    let pieces: Vec<&Piece> = ids.iter().map(|&i| &all[i]).collect();
    let boxes: Vec<Aabb> = pieces
        .iter()
        .map(|p| piece_box(p).inflated(TOUCH_MM))
        .collect();
    let touch = |inner: bool| {
        components(pieces.len(), |i, j| {
            if !boxes[i].overlaps(&boxes[j]) {
                return false;
            }
            let (a, b) = (&pieces[i], &pieces[j]);
            if inner {
                min_poly_dist(&a.inner, &b.inner) <= TOUCH_MM
            } else {
                min_poly_dist(&a.outer, &b.outer) <= TOUCH_MM
            }
        })
    };
    let possible = touch(false);
    let certain = touch(true);
    let mut roots = possible.clone();
    roots.sort_unstable();
    roots.dedup();
    let comps = roots
        .into_iter()
        .map(|r| {
            let idx: Vec<usize> = (0..pieces.len()).filter(|&i| possible[i] == r).collect();
            let hi = idx.iter().map(|&i| pieces[i].hi).sum();
            let lo = idx.iter().map(|&i| pieces[i].lo).sum();
            let mut parts: Vec<usize> = idx.iter().flat_map(|&i| pieces[i].parts.clone()).collect();
            parts.sort_unstable();
            parts.dedup();
            let at = pieces[idx[0]].outer[0][0];
            // Certainly-connected sub-patches inside this possible patch.
            let mut cr: Vec<usize> = idx.iter().map(|&i| certain[i]).collect();
            cr.sort_unstable();
            cr.dedup();
            let certain_lo = cr
                .iter()
                .map(|&c| {
                    idx.iter()
                        .filter(|&&i| certain[i] == c)
                        .map(|&i| pieces[i].lo)
                        .sum::<f64>()
                })
                .fold(0.0, f64::max);
            Comp {
                chain,
                hi,
                lo,
                certain_lo,
                parts,
                at,
                piece_ids: idx.iter().map(|&i| ids[i]).collect(),
                split: cr.len() > 1,
            }
        })
        .collect();
    let loose = pieces.iter().any(|p| p.inner.len() > poly::MAX_UNION);
    (comps, loose)
}

/// The closest approach of every pair of distinct patches (components), including two patches
/// of one chain (for instance on the two faces of a thin wall) and excluding the pieces of one
/// patch. Pairs farther apart than the fusing distance plus a margin are not listed.
fn pair_gaps(pieces: &[Piece], comps: &[Comp]) -> Vec<PairGap> {
    let boxes: Vec<Aabb> = pieces
        .iter()
        .map(|p| piece_box(p).inflated(FUSE_MM + 1e-3))
        .collect();
    let mut comp_of = vec![usize::MAX; pieces.len()];
    for (ci, c) in comps.iter().enumerate() {
        for &p in &c.piece_ids {
            comp_of[p] = ci;
        }
    }
    let mut best: Vec<(usize, usize, PairGap)> = Vec::new();
    for i in 0..pieces.len() {
        for j in i + 1..pieces.len() {
            if comp_of[i] == comp_of[j] || !boxes[i].overlaps(&boxes[j]) {
                continue;
            }
            let dlb = min_poly_dist(&pieces[i].outer, &pieces[j].outer);
            if dlb > FUSE_MM + 1e-3 {
                continue;
            }
            let dub = min_poly_dist(&pieces[i].inner, &pieces[j].inner);
            let (ca, cb) = (comp_of[i].min(comp_of[j]), comp_of[i].max(comp_of[j]));
            let at = pieces[i].outer[0][0];
            match best.iter_mut().find(|(x, y, _)| *x == ca && *y == cb) {
                Some((_, _, g)) => {
                    if dlb < g.dlb {
                        g.dlb = dlb;
                        g.at = at;
                    }
                    g.dub = g.dub.min(dub);
                }
                None => best.push((
                    ca,
                    cb,
                    PairGap {
                        a: comps[ca].chain,
                        b: comps[cb].chain,
                        dlb,
                        dub,
                        at,
                    },
                )),
            }
        }
    }
    best.into_iter().map(|(_, _, g)| g).collect()
}

fn rep_out(scene: &Scene, rep: Rep, polys: &[PartPoly]) -> RepOut {
    let chain_of = |p: &PartPoly| scene.chains.of_part[p.part];
    let mut keys: Vec<(usize, u32)> = polys.iter().map(|p| (chain_of(p), p.tri)).collect();
    keys.sort_unstable();
    keys.dedup();
    let mut pieces = Vec::new();
    for (c, t) in keys {
        let group: Vec<&PartPoly> = polys
            .iter()
            .filter(|p| chain_of(p) == c && p.tri == t)
            .collect();
        pieces.extend(pieces_of(c, &group));
    }
    let mut chain_ids: Vec<usize> = pieces.iter().map(|p| p.chain).collect();
    chain_ids.dedup();
    chain_ids.sort_unstable();
    chain_ids.dedup();
    let mut comps = Vec::new();
    let mut loose = Vec::new();
    for c in chain_ids {
        let mine: Vec<usize> = (0..pieces.len())
            .filter(|&i| pieces[i].chain == c)
            .collect();
        let (cs, is_loose) = comps_of(c, &mine, &pieces);
        comps.extend(cs);
        if is_loose {
            loose.push(c);
        }
    }
    let pairs = pair_gaps(&pieces, &comps);
    let max_hi = comps.iter().map(|c| c.hi).fold(0.0, f64::max);
    let max_certain_lo = comps.iter().map(|c| c.certain_lo).fold(0.0, f64::max);
    RepOut {
        rep,
        comps,
        pairs,
        max_hi,
        max_certain_lo,
        loose,
    }
}

/// Measure every patch and every pair of patches on the written mesh (the verdict source) and,
/// for information, on the solid the fields name.
#[must_use]
pub fn measure(scene: &Scene) -> PatchOut {
    let mut unverified = Vec::new();
    for p in scene.invalid_parts() {
        unverified.push(format!(
            "record {} ({}): {}",
            p.rec,
            p.role.name(),
            p.invalid.as_deref().unwrap_or("not valid")
        ));
    }
    for (i, c) in scene.chains.chains.iter().enumerate() {
        if let Some(why) = &c.ambiguous {
            let rec = scene.parts[c.parts[0]].rec;
            unverified.push(format!("chain {i} at record {rec}: {why}"));
        }
    }
    let ixs: Vec<usize> = (0..scene.parts.len()).collect();
    let per_part = par_map(&ixs, scene.threads, |&i| part_polys(scene, i));
    let reps = Rep::ALL.map(|rep| {
        let k = usize::from(rep == Rep::Mesh);
        let polys: Vec<PartPoly> = per_part.iter().flat_map(|p| p[k].clone()).collect();
        rep_out(scene, rep, &polys)
    });
    PatchOut { reps, unverified }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{self as kit, test_limits as limits};
    use crate::tri::Tri;

    /// A big square plate (two triangles) with its top at z = 0, facing +z, ball below... the
    /// model is above the support: plate facing down (-z) at z = 0.
    fn plate_down() -> Vec<Tri> {
        let (a, b, c, d) = (
            [-5.0, -5.0, 0.0],
            [5.0, -5.0, 0.0],
            [5.0, 5.0, 0.0],
            [-5.0, 5.0, 0.0],
        );
        // Outward normal -z: clockwise seen from +z.
        vec![[a, c, b], [a, d, c]]
    }

    #[test]
    fn a_ball_on_a_plate_has_the_disc_area_in_both_representations() {
        for (radius, standoff) in [(0.2f32, 0.05f32), (0.4, 0.0), (0.05, 0.02), (0.42, 0.1)] {
            // Ball centre below the plate (z = -standoff), poles vertical.
            let c = -standoff;
            let ball = kit::part(
                0,
                7,
                [0.0, 0.0, c - radius],
                [0.0, 0.0, c + radius],
                radius,
                radius,
            );
            let scene = Scene::from_parts(&limits(), plate_down(), [0.0; 3], vec![ball]);
            let out = measure(&scene);
            assert!(out.unverified.is_empty(), "{:?}", out.unverified);
            let truth =
                std::f64::consts::PI * (f64::from(radius).powi(2) - f64::from(standoff).powi(2));
            let analytic = &out.reps[0];
            assert!(
                analytic.max_certain_lo <= truth + 1e-6 && analytic.max_hi >= truth - 1e-6,
                "analytic [{}, {}] vs {truth}",
                analytic.max_certain_lo,
                analytic.max_hi
            );
            assert!(
                analytic.max_hi - analytic.max_certain_lo < 2e-3,
                "{analytic:?}"
            );
            let mesh = &out.reps[1];
            // The mesh is inscribed in the sphere: never larger, at most ~3% smaller.
            assert!(mesh.max_hi <= analytic.max_hi + 1e-9);
            assert!(
                mesh.max_hi >= 0.9 * truth,
                "mesh {} vs {truth}",
                mesh.max_hi
            );
        }
    }

    #[test]
    fn a_chain_counts_its_ball_and_cone_once_and_two_chains_can_fuse() {
        // Tip ball r 0.3 touching the plate, a tip cone that starts at its centre and runs
        // away from the plate, one ball alone 0.5 mm away (its patches 0.005 apart).
        let ball = kit::part(0, 7, [0.0, 0.0, -0.3], [0.0, 0.0, 0.3], 0.3, 0.3);
        let cone = kit::part(1, 1, [0.0, 0.0, 0.0], [0.0, 0.0, -3.0], 0.2, 0.9);
        let scene = Scene::from_parts(&limits(), plate_down(), [0.0; 3], vec![ball, cone]);
        let out = measure(&scene);
        let a = &out.reps[0];
        assert_eq!(a.comps.len(), 1, "{a:?}");
        // The ball's patch is a disc of r 0.3 (centre on the plate): 0.2827 mm2. The cone
        // starts at the ball centre and is inside the ball there: no extra area.
        assert!((a.comps[0].hi - 0.28274).abs() < 2e-3, "{}", a.comps[0].hi);
        assert!(a.pairs.is_empty());
    }

    #[test]
    fn patches_closer_than_ten_micrometres_are_fused_and_twelve_are_not() {
        for (gap, fused) in [(0.005f64, true), (0.012, false), (0.0, true)] {
            let r = 0.3f32;
            let x2 = 2.0 * f64::from(r) + gap;
            let b1 = kit::part(0, 7, [0.0, 0.0, -0.3], [0.0, 0.0, 0.3], r, r);
            #[allow(clippy::cast_possible_truncation)]
            let b2 = kit::part(1, 7, [x2 as f32, 0.0, -0.3], [x2 as f32, 0.0, 0.3], r, r);
            let scene = Scene::from_parts(&limits(), plate_down(), [0.0; 3], vec![b1, b2]);
            let out = measure(&scene);
            // The analytic solid is the larger one: its patches are the closest.
            // (Pairs farther apart than the fusing distance plus a margin are not listed.)
            let analytic = &out.reps[0];
            if fused {
                assert_eq!(analytic.pairs.len(), 1, "gap {gap}: {analytic:?}");
                assert!(analytic.pairs[0].dub < FUSE_MM, "gap {gap}");
            } else {
                for rep in &out.reps {
                    for pair in &rep.pairs {
                        assert!(pair.dlb >= FUSE_MM, "gap {gap}: {pair:?}");
                    }
                }
            }
        }
    }
}
