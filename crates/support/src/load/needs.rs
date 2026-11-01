//! The overhang and site-needs map (design `step11-construction.md` sections 5.2 and 5.3).
//!
//! Down-facing faces flatter than 45° (P11 **[C]**) are sampled on a lattice of 0.25 mm
//! **[S]** in the plane of each triangle. Every sample carries its share of the exact triangle
//! area, so the areas of any subset add up exactly. Site class comes from batched ray casts
//! in the resting pose; until a ray function that answers is supplied, the class is
//! `Unverified`, never a favourable guess.

use plinth_geom::Mesh;
use plinth_geom::measure::Measure;
use plinth_geom::ray::{Ray, RayHit};

use super::data::{LayerStack, LayoutView, LoadWarning, WarningClass};

/// P11: faces flatter than this angle from horizontal need support **[C]**.
pub const OVERHANG_DEG: f64 = 45.0;
/// Sample lattice spacing in the plane of each face, mm **[S]**.
pub const SAMPLE_SPACING_MM: f64 = 0.25;
/// A contact covers an overhang sample within this distance, mm **[S]**.
pub const COVER_REACH_MM: f64 = 3.0;
/// Directions of the visibility test **[S]**.
pub const SITE_DIRECTIONS: usize = 64;

/// The site class of a sample (design section 5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteTag {
    /// Class 1: no ray in the upper hemisphere of the resting pose gets out.
    Hidden,
    /// Class 2: some rays get out, and the face does not look up.
    PartlyVisible,
    /// Class 3: the decorated top, which looks up in the resting pose and is visible.
    Top,
    /// Not decided, and why.
    Unverified(&'static str),
}

/// One sample on a face that needs support.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeedPoint {
    /// Position, mm (print pose).
    pub xyz: [f64; 3],
    /// Outward unit normal.
    pub normal: [f64; 3],
    /// Angle of the face from horizontal, degrees (0 flat, facing down).
    pub angle_deg: f64,
    /// The share of the triangle's area this sample stands for, mm².
    pub area_mm2: f64,
    /// Mesh triangle.
    pub triangle: u32,
    /// Site class.
    pub class: SiteTag,
}

/// The overhang samples of a mesh.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NeedsMap {
    /// Samples, in triangle order.
    pub points: Vec<NeedPoint>,
    /// The angle limit used, degrees.
    pub limit_deg: f64,
    /// The lattice spacing used, mm.
    pub spacing_mm: f64,
}

impl NeedsMap {
    /// Total area of the faces flatter than the limit, mm² (exact triangle sums).
    #[must_use]
    pub fn total_area_mm2(&self) -> f64 {
        self.points.iter().map(|p| p.area_mm2).sum()
    }

    /// Area with an angle in `[lo, hi)` degrees, mm².
    #[must_use]
    pub fn band_area_mm2(&self, lo: f64, hi: f64) -> f64 {
        self.points
            .iter()
            .filter(|p| p.angle_deg >= lo && p.angle_deg < hi)
            .map(|p| p.area_mm2)
            .sum()
    }

    /// Area with an angle in `[lo, hi)` degrees projected on the horizontal plane, mm². This
    /// is the plan-view area the design's section 0 table lists (surface area times the
    /// cosine of the angle from horizontal).
    #[must_use]
    pub fn band_projected_mm2(&self, lo: f64, hi: f64) -> f64 {
        self.points
            .iter()
            .filter(|p| p.angle_deg >= lo && p.angle_deg < hi)
            .map(|p| p.area_mm2 * p.angle_deg.to_radians().cos())
            .sum()
    }

    /// Area of the samples of one class, mm².
    #[must_use]
    pub fn class_area_mm2(&self, pred: impl Fn(SiteTag) -> bool) -> f64 {
        self.points
            .iter()
            .filter(|p| pred(p.class))
            .map(|p| p.area_mm2)
            .sum()
    }
}

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: V3) -> Option<V3> {
    let l = dot(a, a).sqrt();
    (l > 0.0).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

/// Sample the down-facing triangles of `mesh` that are flatter than `limit_deg`.
#[must_use]
pub fn overhang_points(mesh: &Mesh, limit_deg: f64, spacing: f64) -> NeedsMap {
    let cos_limit = limit_deg.to_radians().cos();
    let mut points = Vec::new();
    for t in 0..mesh.triangle_count() {
        let [a, b, c] = mesh.triangle_corners(t);
        let n2 = cross(sub(b, a), sub(c, a));
        let area = 0.5 * dot(n2, n2).sqrt();
        let Some(n) = unit(n2) else { continue };
        if -n[2] <= cos_limit {
            continue;
        }
        let angle = (-n[2]).clamp(-1.0, 1.0).acos().to_degrees();
        #[allow(clippy::cast_possible_truncation)]
        let id = t as u32;
        let samples = lattice(a, b, c, n, spacing);
        let share = area / samples.len() as f64;
        points.extend(samples.into_iter().map(|xyz| NeedPoint {
            xyz,
            normal: n,
            angle_deg: angle,
            area_mm2: share,
            triangle: id,
            class: SiteTag::Unverified("site class not computed"),
        }));
    }
    NeedsMap {
        points,
        limit_deg,
        spacing_mm: spacing,
    }
}

/// Lattice points of spacing `s` in the plane of triangle `abc` that fall inside it; the
/// centroid when none does. The lattice is anchored at the point of the plane nearest the
/// origin, so triangles in one plane share it.
fn lattice(a: V3, b: V3, c: V3, n: V3, s: f64) -> Vec<V3> {
    let u = unit(cross([0.0, 0.0, 1.0], n)).unwrap_or([1.0, 0.0, 0.0]);
    let v = cross(n, u);
    let o: V3 = [n[0] * dot(n, a), n[1] * dot(n, a), n[2] * dot(n, a)];
    let uv = |p: V3| {
        let d = sub(p, o);
        [dot(d, u), dot(d, v)]
    };
    let (pa, pb, pc) = (uv(a), uv(b), uv(c));
    let lo = [pa[0].min(pb[0]).min(pc[0]), pa[1].min(pb[1]).min(pc[1])];
    let hi = [pa[0].max(pb[0]).max(pc[0]), pa[1].max(pb[1]).max(pc[1])];
    let area2 = (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0]);
    let mut out = Vec::new();
    if area2.abs() > 0.0 {
        #[allow(clippy::cast_possible_truncation)]
        let (i0, i1, j0, j1) = (
            (lo[0] / s).ceil() as i64,
            (hi[0] / s).floor() as i64,
            (lo[1] / s).ceil() as i64,
            (hi[1] / s).floor() as i64,
        );
        for i in i0..=i1 {
            for j in j0..=j1 {
                #[allow(clippy::cast_precision_loss)]
                let q = [i as f64 * s, j as f64 * s];
                let w0 =
                    ((pb[0] - q[0]) * (pc[1] - q[1]) - (pb[1] - q[1]) * (pc[0] - q[0])) / area2;
                let w1 =
                    ((pc[0] - q[0]) * (pa[1] - q[1]) - (pc[1] - q[1]) * (pa[0] - q[0])) / area2;
                let w2 = 1.0 - w0 - w1;
                if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                    out.push([
                        o[0] + q[0] * u[0] + q[1] * v[0],
                        o[1] + q[0] * u[1] + q[1] * v[1],
                        o[2] + q[0] * u[2] + q[1] * v[2],
                    ]);
                }
            }
        }
    }
    if out.is_empty() {
        out.push([
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        ]);
    }
    out
}

/// The resting pose: which way is up when the base stands on the table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RestPose {
    /// Unit vector, in the print-pose frame, that points up when the base rests on the
    /// table with its underside down. For the approved Round bases it is the top normal.
    pub up: V3,
}

/// A batch ray function: one answer per ray, in order.
pub type RayBatch<'a> = &'a dyn Fn(&[Ray], &mut Vec<Measure<RayHit>>);

/// 64 directions spread evenly over the hemisphere around `up` (a Fibonacci spiral).
fn hemisphere(up: V3, n: usize) -> Vec<V3> {
    let w = unit(up).unwrap_or([0.0, 0.0, 1.0]);
    let helper = if w[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = unit(cross(w, helper)).unwrap_or([1.0, 0.0, 0.0]);
    let v = cross(w, u);
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    (0..n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let (z, phi) = ((i as f64 + 0.5) / n as f64, golden * i as f64);
            let r = (1.0 - z * z).sqrt();
            [
                w[0] * z + r * (phi.cos() * u[0] + phi.sin() * v[0]),
                w[1] * z + r * (phi.cos() * u[1] + phi.sin() * v[1]),
                w[2] * z + r * (phi.cos() * u[2] + phi.sin() * v[2]),
            ]
        })
        .collect()
}

/// Decide the site class of every sample with one batch of rays. A sample is hidden when no
/// ray of the upper hemisphere that leaves its face gets out; the decorated top is a visible
/// sample whose normal looks up. A ray that could not be answered leaves the sample
/// `Unverified`.
pub fn classify_sites(points: &mut [NeedPoint], rest: &RestPose, rays_fn: RayBatch<'_>) {
    let dirs = hemisphere(rest.up, SITE_DIRECTIONS);
    let mut rays = Vec::new();
    let mut owner = Vec::new();
    for (pi, p) in points.iter().enumerate() {
        let origin = [
            p.xyz[0] + 1e-4 * p.normal[0],
            p.xyz[1] + 1e-4 * p.normal[1],
            p.xyz[2] + 1e-4 * p.normal[2],
        ];
        for d in dirs.iter().filter(|d| dot(**d, p.normal) > 1e-6) {
            rays.push(Ray { origin, dir: *d });
            owner.push(pi);
        }
    }
    let mut hits = Vec::new();
    rays_fn(&rays, &mut hits);
    let mut escaped = vec![0_u32; points.len()];
    let mut unknown = vec![false; points.len()];
    if hits.len() == rays.len() {
        for (h, &pi) in hits.iter().zip(&owner) {
            match h {
                Measure::Known(RayHit::Miss) => escaped[pi] += 1,
                Measure::Known(RayHit::Hit { .. }) => {}
                Measure::Unverified(_) => unknown[pi] = true,
            }
        }
    } else {
        unknown.iter_mut().for_each(|u| *u = true);
    }
    for (pi, p) in points.iter_mut().enumerate() {
        p.class = if unknown[pi] {
            SiteTag::Unverified("a visibility ray could not be answered")
        } else if escaped[pi] == 0 {
            SiteTag::Hidden
        } else if dot(p.normal, rest.up) > 0.5 {
            SiteTag::Top
        } else {
            SiteTag::PartlyVisible
        };
    }
}

/// How well the layout covers the overhang samples.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Coverage {
    /// Samples that need cover (hidden, partly visible or undecided).
    pub needing: usize,
    /// Of those, the covered ones.
    pub covered: usize,
    /// Area of the samples that need cover, mm².
    pub needing_mm2: f64,
    /// Area of the uncovered ones, mm².
    pub uncovered_mm2: f64,
}

/// Check every sample that needs cover (class hidden or partly visible; an undecided sample
/// counts as needing it) for a contact within [`COVER_REACH_MM`] whose ball is printed no
/// later than the sample's layer. Returns the counts and the warning of design section 4.8.
#[must_use]
pub fn check_coverage(
    needs: &NeedsMap,
    layout: &LayoutView,
    stack: &LayerStack,
) -> (Coverage, Option<LoadWarning>) {
    let mut cov = Coverage::default();
    let mut missed: Vec<&NeedPoint> = Vec::new();
    let r2 = COVER_REACH_MM * COVER_REACH_MM;
    for p in &needs.points {
        if p.class == SiteTag::Top {
            continue;
        }
        cov.needing += 1;
        cov.needing_mm2 += p.area_mm2;
        let layer = stack_layer(stack, p.xyz[2]);
        let covered = layout.contacts.iter().any(|c| {
            let d = sub(c.centre, p.xyz);
            dot(d, d) <= r2 && c.first_layer <= layer
        });
        if covered {
            cov.covered += 1;
        } else {
            cov.uncovered_mm2 += p.area_mm2;
            missed.push(p);
        }
    }
    // Every missed sample is located, not just the first. This complete list is
    // carried into both FR-21 and run.json, with triangle and exposure layer.
    let locations = missed.iter().enumerate().map(|(i, p)| format!(
        "Coverage marker C{}: triangle {}, layer {}, ({:.4}, {:.4}, {:.4}) mm; add a support here.",
        i + 1, p.triangle, stack_layer(stack, p.xyz[2]), p.xyz[0], p.xyz[1], p.xyz[2]
    )).collect::<Vec<_>>().join(" ");
    let warning = missed.first().map(|first| LoadWarning {
        class: WarningClass::Intervention,
        kind: "overhang without a contact within 3.0 mm".to_owned(),
        layers: vec![stack_layer(stack, first.xyz[2])],
        contacts: Vec::new(),
        location_mm: Some(first.xyz),
        message: format!(
            "{} of {} sub-45 degree samples ({:.2} mm² of {:.2} mm²) have no contact within \
                 {COVER_REACH_MM} mm that is printed by their layer. The first is at \
                 ({:.2}, {:.2}, {:.2}) mm. Each remaining point is a located intervention. {locations}",
            missed.len(),
            cov.needing,
            cov.uncovered_mm2,
            cov.needing_mm2,
            first.xyz[0],
            first.xyz[1],
            first.xyz[2]
        ),
    });
    (cov, warning)
}

fn stack_layer(stack: &LayerStack, z: f64) -> u32 {
    let k = ((z - stack.layer_origin_mm) / stack.layer_height_mm).floor();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        k.max(0.0) as u32
    }
}

#[cfg(test)]
mod tests;
