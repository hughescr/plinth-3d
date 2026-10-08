//! Report 2: intrusion into the model, from the record HEADERS.
//!
//! For every taper, member, flare and joint record, sample the surface of the
//! solid its header describes (start, end, radii): the lateral surface at
//! [`AXIAL_STEP_MM`] spacing and [`AZIMUTH_SAMPLES`] points per ring, plus the
//! two cap centres; a joint ball gets a Fibonacci set. This is PRE-TRIM: the
//! emitter's seat trim edits the record mesh, never the header, so a trim can
//! not hide a penetration here.
//!
//! The depth of a sample is its distance to the model surface when it lies
//! inside the solid, else 0. Two depths are kept per record. `depth_mm`
//! excludes only the points inside the record's OWN contact ball sphere (the
//! tip balls adjacent to the record in the graph): the ball is the contact and
//! sits in the model by design, and a taper's part inside the ball is covered
//! by it. `depth_with_ball_mm` excludes nothing; it is what the v19 diagnosis
//! measured (record 459: 0.354 with the ball sphere, 0.242 beyond it).
//!
//! Batch shape: the sample rings of one record are cut into runs; the model
//! triangles within [`NEAR_MM`] of a run's box are fetched once per run, and
//! every sample of the run tests against that short list. Near samples that
//! cannot raise a maximum skip the parity test; far samples (no triangle within
//! [`NEAR_MM`]) always take it, and a far sample found inside gets its exact
//! depth from the unbounded nearest-surface search.

use super::{par_map, ring_points, GeoCtx};
use crate::mesh::RayHit;
use crate::primitives::Primitive;
use crate::reader::RecordKind;
use crate::vec3::{self as v, Aabb, V3};

/// Axial spacing of sample rings.
pub const AXIAL_STEP_MM: f64 = 0.05;
/// Points per ring.
pub const AZIMUTH_SAMPLES: usize = 24;
/// Search radius of the cheap local triangle fetch. It only speeds the common
/// case (a sample near the surface); it never limits an outcome. A sample with
/// no triangle within this radius still gets the inside test, and when it is
/// inside its depth comes from an unbounded nearest-surface search, so a
/// record buried deeper than this reports its true depth.
pub const NEAR_MM: f64 = 1.0;
/// Rings per run of one triangle fetch.
const RUN_RINGS: usize = 12;
/// Reporting threshold: records deeper than this are counted.
pub const COUNT_ABOVE_MM: f64 = 0.01;

#[derive(Clone, Debug)]
pub struct RecordIntrusion {
    pub record: usize,
    pub kind: RecordKind,
    /// Deepest sample outside the record's own contact ball sphere, mm.
    pub depth_mm: f64,
    /// Deepest sample anywhere on the record's header surface, ball sphere
    /// included. The v19 diagnosis measured this one (459 0.355, 496 0.248);
    /// the part of a taper inside its own ball is covered by the ball, which
    /// is seated in the model on purpose.
    pub depth_with_ball_mm: f64,
    pub worst_point: V3,
    /// Samples whose inside test was indeterminate (16 grazing directions).
    pub indeterminate: usize,
    pub samples: usize,
}

#[derive(Clone, Debug, Default)]
pub struct IntrusionReport {
    pub records_checked: usize,
    pub samples: usize,
    pub indeterminate: usize,
    /// Records whose depth exceeds [`COUNT_ABOVE_MM`].
    pub count_above: usize,
    /// All records with a positive depth, deepest first.
    pub ranked: Vec<RecordIntrusion>,
}

impl IntrusionReport {
    /// The depth of one record, 0 when it never enters the model.
    #[must_use]
    pub fn depth_of(&self, record: usize) -> f64 {
        self.ranked
            .iter()
            .find(|r| r.record == record)
            .map_or(0.0, |r| r.depth_mm)
    }
}

/// Rings of sample points for one primitive, grouped so a run is contiguous.
fn sample_rings(prim: &Primitive) -> Vec<Vec<V3>> {
    match *prim {
        Primitive::Frustum { a, ra, b, rb } => {
            let axis_v = v::sub(b, a);
            let len = v::norm(axis_v);
            let Some(u) = v::normalize(axis_v) else {
                return Vec::new();
            };
            let n = ((len / AXIAL_STEP_MM).ceil() as usize).max(1);
            let mut rings = Vec::with_capacity(n + 3);
            rings.push(vec![a]);
            for i in 0..=n {
                let f = i as f64 / n as f64;
                let centre = v::add(a, v::mul(axis_v, f));
                let r = ra + (rb - ra) * f;
                // Stagger alternate rings by half a step.
                let phase = if i % 2 == 1 {
                    std::f64::consts::PI / AZIMUTH_SAMPLES as f64
                } else {
                    0.0
                };
                rings.push(ring_points(centre, u, r, AZIMUTH_SAMPLES, phase));
            }
            rings.push(vec![b]);
            rings
        }
        Primitive::Ball { centre, r } => {
            // Fibonacci sphere.
            const N: usize = 256;
            let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
            let pts: Vec<V3> = (0..N)
                .map(|i| {
                    let z = 1.0 - (2.0 * i as f64 + 1.0) / N as f64;
                    let rr = (1.0 - z * z).max(0.0).sqrt();
                    let t = golden * i as f64;
                    [
                        centre[0] + r * rr * t.cos(),
                        centre[1] + r * rr * t.sin(),
                        centre[2] + r * z,
                    ]
                })
                .collect();
            vec![pts]
        }
        _ => Vec::new(),
    }
}

fn measure_record(ctx: &GeoCtx, ri: usize) -> Option<RecordIntrusion> {
    let rec = ctx.rec(ri);
    let prim = ctx.prims[ri]?;
    // Own contact balls: tip records joined to this record in the graph.
    let balls: Vec<(V3, f64)> = ctx
        .graph
        .neighbours_of(ri)
        .iter()
        .filter(|&&n| ctx.rec(n).kind == RecordKind::ContactSphere)
        .filter_map(|&n| ctx.rec(n).ball_centre().map(|c| (c, ctx.rec(n).r_start)))
        .collect();
    let rings = sample_rings(&prim);
    let mut hits: Vec<RayHit> = Vec::new();
    // `best` counts only samples outside the own ball; `best_all` counts every
    // sample, the way the v19 diagnosis measured it.
    let mut best = 0.0f64;
    let mut best_all = 0.0f64;
    let mut worst = [0.0; 3];
    let mut indeterminate = 0usize;
    let mut samples = 0usize;
    for run in rings.chunks(RUN_RINGS) {
        let mut bb = Aabb::empty();
        for p in run.iter().flatten() {
            bb.add_point(*p);
        }
        // Every triangle within NEAR_MM of the run's box. A sample whose
        // nearest listed triangle is farther than NEAR_MM is therefore farther
        // than NEAR_MM from the whole model; the list may well be empty.
        let near = ctx.mesh.triangles_in_box(&bb.expanded(NEAR_MM));
        let tris: Vec<[V3; 3]> = near.iter().map(|&t| ctx.mesh.tri(t)).collect();
        for &p in run.iter().flatten() {
            let in_ball = balls.iter().any(|(c, r)| v::dist(*c, p) < *r);
            if !in_ball {
                samples += 1;
            }
            let mut d = f64::INFINITY;
            for t in &tris {
                d = d.min(v::point_triangle_closest(p, t[0], t[1], t[2]).0);
            }
            let known = d <= NEAR_MM;
            // Near samples skip the inside test when they cannot beat any depth
            // they could still raise. A far sample has no such bound: it may be
            // deep inside the solid, so it always gets the test.
            if known && !(d > best_all || (!in_ball && d > best)) {
                continue;
            }
            match ctx.inside(p, &mut hits) {
                Some(true) => {
                    // Far inside points get their exact depth from the
                    // unbounded nearest-surface search.
                    let d = if known {
                        d
                    } else {
                        ctx.mesh
                            .nearest_surface(p)
                            .map_or(NEAR_MM, |h| h.distance_mm)
                    };
                    best_all = best_all.max(d);
                    if !in_ball && d > best {
                        best = d;
                        worst = p;
                    }
                }
                Some(false) => {}
                None => indeterminate += 1,
            }
        }
    }
    Some(RecordIntrusion {
        record: ri,
        kind: rec.kind,
        depth_mm: best,
        depth_with_ball_mm: best_all,
        worst_point: worst,
        indeterminate,
        samples,
    })
}

#[must_use]
pub fn measure(ctx: &GeoCtx) -> IntrusionReport {
    let ids: Vec<usize> = (0..ctx.model.records.len())
        .filter(|&i| {
            matches!(
                ctx.rec(i).kind,
                RecordKind::ContactTaper
                    | RecordKind::Member
                    | RecordKind::RootFlare
                    | RecordKind::JointSphere
            )
        })
        .collect();
    let rows = par_map(ids.len(), ctx.params.threads, |k| measure_record(ctx, ids[k]));
    let mut rep = IntrusionReport {
        records_checked: ids.len(),
        ..IntrusionReport::default()
    };
    for r in rows.into_iter().flatten() {
        rep.samples += r.samples;
        rep.indeterminate += r.indeterminate;
        if r.depth_mm > COUNT_ABOVE_MM {
            rep.count_above += 1;
        }
        if r.depth_with_ball_mm > 0.0 {
            rep.ranked.push(r);
        }
    }
    rep.ranked.sort_by(|a, b| {
        (b.depth_mm, b.depth_with_ball_mm)
            .partial_cmp(&(a.depth_mm, a.depth_with_ball_mm))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rep
}
