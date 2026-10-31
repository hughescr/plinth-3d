use super::{Reject, Section};
use plinth_geom::{
    clearance::Solid,
    patch::Ball,
    shapes::{self, Triangle},
};
use std::collections::BTreeMap;

pub(super) const ENVELOPE: f64 = 1e-5;
/// Native part role, not a planner-selected arbitrary record type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    /// Contact ball.
    TipBall,
    /// Flared contact cone.
    TipCone,
    /// Whole joint ball.
    JointBall,
    /// Column or brace.
    Rod,
    /// Plate flare.
    BaseFlare,
    /// Connected raft pad.
    RaftPad,
    /// Connected raft bridge.
    RaftBridge,
}
/// Read-only field description. Constructing this data does not admit a part to a plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PartFields {
    /// Sphere poles and native radius.
    Ball {
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
    },
    /// Frustum end centres and radii.
    Frustum {
        start: [f32; 3],
        end: [f32; 3],
        r1: f32,
        r2: f32,
    },
    /// Raft pad.
    Pad { centre: [f32; 2], top: f32 },
    /// Raft bridge.
    Bridge {
        from: [f32; 2],
        to: [f32; 2],
        top: f32,
    },
}
/// Exact fields and native vertices checked at construction. No public constructor.
#[derive(Debug)]
pub struct NativePart {
    kind: PartKind,
    fields: PartFields,
    mesh: Vec<Triangle>,
    inner: Option<Ball>,
}
impl NativePart {
    /// Native role.
    pub fn kind(&self) -> PartKind {
        self.kind
    }
    /// Exact rounded fields.
    pub fn fields(&self) -> PartFields {
        self.fields
    }
    /// Exact checked vertices (emission must copy these).
    pub fn mesh(&self) -> &[Triangle] {
        &self.mesh
    }
    /// Collision-free digest: the complete ordered field and vertex bit words.
    pub fn field_words(&self) -> Vec<u32> {
        let mut words = vec![self.kind as u32];
        match self.fields {
            PartFields::Ball { start, end, radius } => {
                words.extend(
                    start
                        .into_iter()
                        .chain(end)
                        .chain([radius])
                        .map(f32::to_bits),
                );
            }
            PartFields::Frustum { start, end, r1, r2 } => {
                words.extend(
                    start
                        .into_iter()
                        .chain(end)
                        .chain([r1, r2])
                        .map(f32::to_bits),
                );
            }
            PartFields::Pad { centre, top } => {
                words.extend(centre.into_iter().chain([top]).map(f32::to_bits));
            }
            PartFields::Bridge { from, to, top } => {
                words.extend(from.into_iter().chain(to).chain([top]).map(f32::to_bits));
            }
        }
        words.extend(self.mesh.iter().flatten().flatten().map(|v| v.to_bits()));
        words
    }
    pub(super) fn copy_exact(&self) -> Self {
        Self {
            kind: self.kind,
            fields: self.fields,
            mesh: self.mesh.clone(),
            inner: self.inner,
        }
    }
    pub(super) fn ball(
        kind: PartKind,
        c: [f32; 3],
        axis: [f32; 3],
        r: f32,
    ) -> Result<Self, Reject> {
        let start = std::array::from_fn(|i| c[i] - axis[i] * r);
        let end = std::array::from_fn(|i| c[i] + axis[i] * r);
        let mesh = shapes::sphere(start, end, r);
        let mut part = Self {
            kind,
            fields: PartFields::Ball {
                start,
                end,
                radius: r,
            },
            mesh,
            inner: None,
        };
        // Fields and mesh are immutable. Retain the certificate rather than welding
        // and validating the same 480 faces again for every capacity/column query.
        part.inner = Some(part.ball_inner()?);
        Ok(part)
    }
    pub(super) fn frustum(
        kind: PartKind,
        start: [f32; 3],
        end: [f32; 3],
        r1: Section,
        r2: Section,
    ) -> Result<Self, Reject> {
        if distance(start, end) < 1e-4 {
            return Err(Reject::UnknownShape);
        }
        let mesh = shapes::frustum(start, end, r1.radius(), r2.radius());
        if mesh.iter().flatten().flatten().any(|v| !v.is_finite()) {
            return Err(Reject::NonFinite);
        }
        let part = Self {
            kind,
            fields: PartFields::Frustum {
                start,
                end,
                r1: r1.radius(),
                r2: r2.radius(),
            },
            mesh,
            inner: None,
        };
        if !part.frustum_enclosed() {
            return Err(Reject::MeasureExhausted);
        }
        let across = 2.0 * part.frustum_inner()?;
        if across < 0.102 {
            return Err(Reject::TooThin { across });
        }
        Ok(part)
    }
    fn frustum_enclosed(&self) -> bool {
        let PartFields::Frustum { start, end, r1, r2 } = self.fields else {
            return false;
        };
        let a = start.map(f64::from);
        let b = end.map(f64::from);
        let axis = sub(b, a);
        let length = norm(axis);
        let axis = axis.map(|x| x / length);
        self.mesh.iter().flatten().all(|v| {
            let d = sub(v.map(f64::from), a);
            let t = dot(d, axis);
            let clamped = t.clamp(0.0, length);
            let radial = norm(std::array::from_fn(|i| d[i] - axis[i] * t));
            let radius = f64::from(r1) + f64::from(r2 - r1) * clamped / length;
            (t - clamped).hypot((radial - radius).max(0.0)) < ENVELOPE - 1e-9
        })
    }
    // Intersection of all inward side halfspaces contains a coaxial circular
    // frustum of this minimum radius. Computing from actual side faces includes
    // rotation, translation and native rounding, rather than trusting field radii.
    pub(super) fn frustum_inner(&self) -> Result<f64, Reject> {
        let PartFields::Frustum { start, end, .. } = self.fields else {
            return Err(Reject::UnknownShape);
        };
        let a = start.map(f64::from);
        let b = end.map(f64::from);
        let axis = sub(b, a);
        let axis = axis.map(|x| x / norm(axis));
        let mid = std::array::from_fn(|i| (a[i] + b[i]) * 0.5);
        let mut radius = f64::INFINITY;
        for t in &self.mesh[..32] {
            let [p, q, r] = t.map(|v| v.map(f64::from));
            let n = cross(sub(q, p), sub(r, p));
            let len = norm(n);
            if len < 1e-16 {
                return Err(Reject::NarrowJoin);
            }
            let n = n.map(|v| v / len);
            let sign = dot(sub(mid, p), n).signum();
            let radial = (1.0 - dot(axis, n).powi(2)).max(0.0).sqrt();
            if radial < 1e-8 {
                return Err(Reject::NarrowJoin);
            }
            let distance = (dot(sub(a, p), n) * sign).min(dot(sub(b, p), n) * sign);
            if distance <= 0.0 {
                return Err(Reject::NarrowJoin);
            }
            let a_bound = plinth_geom::patch::plane_distance_bounds(a, [p, q, r])
                .ok_or(Reject::NarrowJoin)?;
            let b_bound = plinth_geom::patch::plane_distance_bounds(b, [p, q, r])
                .ok_or(Reject::NarrowJoin)?;
            // Euclidean plane distance is <= in-plane radial distance. This
            // deliberately conservative inner cylinder avoids normalizing a nearly
            // axial face without an arithmetic enclosure.
            radius = radius.min(a_bound.lo.min(b_bound.lo));
        }
        if radius <= 0.0 {
            return Err(Reject::NarrowJoin);
        }
        Ok(radius)
    }
    pub(super) fn pad(centre: [f32; 2], top: f32) -> Self {
        Self {
            kind: PartKind::RaftPad,
            fields: PartFields::Pad { centre, top },
            mesh: shapes::pad(centre, top, top - 0.2, 1.1),
            inner: None,
        }
    }
    pub(super) fn bridge(from: [f32; 2], to: [f32; 2], top: f32) -> Self {
        Self {
            kind: PartKind::RaftBridge,
            fields: PartFields::Bridge { from, to, top },
            mesh: shapes::bridge(from, to, top, top - 0.2, 1.1),
            inner: None,
        }
    }
    pub(super) fn solid(&self) -> Option<Solid> {
        match self.fields {
            PartFields::Ball { start, end, radius } => Some(Solid::Ball(Ball {
                centre: std::array::from_fn(|i| f64::from((start[i] + end[i]) * 0.5)),
                radius: f64::from(radius) + ENVELOPE,
            })),
            PartFields::Frustum { start, end, r1, r2 } => Some(Solid::Frustum {
                start: start.map(f64::from),
                end: end.map(f64::from),
                r_start: f64::from(r1) + ENVELOPE,
                r_end: f64::from(r2) + ENVELOPE,
            }),
            _ => None,
        }
    }
    /// Validate the native closed radial ball and its inner/outer containments.
    /// Closure, consistent edges and a degree-one outward radial map certify the
    /// surface; vertex inscription alone is not used as a capacity certificate.
    pub(super) fn ball_inner(&self) -> Result<Ball, Reject> {
        if let Some(inner) = self.inner {
            return Ok(inner);
        }
        let PartFields::Ball { start, end, radius } = self.fields else {
            return Err(Reject::UnknownShape);
        };
        let c = std::array::from_fn(|i| f64::from((start[i] + end[i]) * 0.5));
        let mut edges = BTreeMap::<([u32; 3], [u32; 3]), (u32, i32)>::new();
        let mut inner = f64::INFINITY;
        let mut sign = 0.0f64;
        let mut solid_angle = 0.0f64;
        let key = |p: [f32; 3]| {
            p.map(|x| {
                if x.abs() < f32::MIN_POSITIVE {
                    0
                } else {
                    x.to_bits()
                }
            })
        };
        for t in &self.mesh {
            for i in 0..3 {
                let a = key(t[i]);
                let b = key(t[(i + 1) % 3]);
                let (k, s) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let e = edges.entry(k).or_default();
                e.0 += 1;
                e.1 += s;
            }
            let [a, b, d] = t.map(|v| v.map(f64::from));
            let n = cross(sub(b, a), sub(d, a));
            let len = norm(n);
            if len < 1e-16 {
                return Err(Reject::PatchUnverified);
            }
            let n = n.map(|x| x / len);
            let h = dot(sub(a, c), n);
            if sign.abs() < 0.5 {
                sign = h.signum();
            }
            if h * sign <= 0.0 {
                return Err(Reject::PatchUnverified);
            }
            let bound = plinth_geom::patch::plane_distance_bounds(c, [a, b, d])
                .ok_or(Reject::PatchUnverified)?;
            inner = inner.min(bound.lo);
            // All radial face orientations agree. Their positive spherical areas
            // sum to one sphere: the radial map has degree one, so no folded or
            // multiply covered native surface can supply a capacity lower bound.
            let u = sub(a, c);
            let v = sub(b, c);
            let w = sub(d, c);
            let numerator = dot(u, cross(v, w)).abs();
            let denominator = norm(u) * norm(v) * norm(w)
                + dot(u, v) * norm(w)
                + dot(v, w) * norm(u)
                + dot(w, u) * norm(v);
            solid_angle += 2.0 * numerator.atan2(denominator);
        }
        if (solid_angle - 4.0 * std::f64::consts::PI).abs() > 1e-6
            || edges.values().any(|&(n, s)| n != 2 || s != 0)
            || self
                .mesh
                .iter()
                .flatten()
                .any(|v| norm(sub(v.map(f64::from), c)) > f64::from(radius) + ENVELOPE)
            || inner <= 0.0
        {
            return Err(Reject::PatchUnverified);
        }
        Ok(Ball {
            centre: c,
            radius: inner,
        })
    }
}
pub(super) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(super) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
pub(super) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(super) fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
pub(super) fn distance(a: [f32; 3], b: [f32; 3]) -> f64 {
    norm(sub(a.map(f64::from), b.map(f64::from)))
}
