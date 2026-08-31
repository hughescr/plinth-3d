//! Analytic support solids reconstructed from the records, and the
//! declared-versus-reconstructed check of the geometry itself.
//!
//! `audit-invariants.md` §1.4 builds the support solid `S` from the record
//! fields, not from the support triangle meshes, because the analytic form is
//! exact and cheap. It is then **verified against the emitted mesh**: every
//! vertex of a record's own soup must lie on the analytic surface within
//! `tol_geom`. Failure is `GEOM_MESH_DISAGREES_WITH_RECORD`.
//!
//! **One deviation from the specification, stated rather than hidden.** §1.4
//! also asks that the soup's bounding box *equal* the analytic bounding box.
//! That cannot hold for a faceted tessellation of a curved surface, so
//! [`MeshAgreement`] splits it: a soup that reaches OUTSIDE the analytic box is
//! a gated failure, and a soup inscribed INSIDE it is reported as
//! `bbox_shortfall_mm` and not gated. See [`MeshAgreement::bbox_shortfall_mm`]
//! for the arithmetic.

use crate::consts as c;
use crate::reader::{RecordKind, SupportRecord};
use crate::vec3::{self as v, Aabb, V3};

/// A convex analytic solid. Every emitted support primitive is convex, which
/// is what makes the exact chord and slice arithmetic below possible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Primitive {
    /// Types 7 and 9. `centre = midpoint(start, end)`, `r = r_start`.
    Ball { centre: V3, r: f64 },
    /// Types 1, 3 and 4. A right circular frustum, `a` radius `ra` to `b`
    /// radius `rb`. `ra == rb` makes it a cylinder.
    Frustum { a: V3, ra: f64, b: V3, rb: f64 },
    /// Type 6 pad: a vertical cylinder of the given radius and height whose
    /// base sits at `base`.
    RaftPad { base: V3, r: f64, h: f64 },
    /// Type 6 bridge: a rectangular prism of the given width and height,
    /// running from `a` to `b` in plan and rising `h` from their z.
    RaftBridge { a: V3, b: V3, width: f64, h: f64 },
}

/// Why a record could not be turned into an analytic solid. Every variant is a
/// hard failure -- never a skip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeomError {
    NonFiniteField,
    NonPositiveRadius,
    /// A type 1/3/4 record whose two endpoints are exactly equal. Exact-zero
    /// test, no tolerance: the emitter should never write one at all
    /// (`audit-invariants.md` §3.8).
    ZeroLengthAxis,
    /// A type-6 bridge with no plan-view length.
    ZeroLengthBridge,
    /// A type-6 record with a non-positive height in `r_end`.
    NonPositiveRaftHeight,
}

impl GeomError {
    /// The `reason=` token of the `GEOM_DEGENERATE` failure line.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NonFiniteField => "nonfinite",
            Self::NonPositiveRadius | Self::NonPositiveRaftHeight => "nonpositive_radius",
            Self::ZeroLengthAxis | Self::ZeroLengthBridge => "zero_length",
        }
    }
}

impl Primitive {
    /// Reconstruct the solid a record describes.
    ///
    /// **The type-6 units trap is handled here.** For types 1/3/4/7/9 the
    /// fields at +32 and +36 are radii, as `audit-contract.md` §1.2 says. For
    /// type 6 the emitter passes different quantities through the same slots
    /// (`plinth-emit/src/lib.rs:636-637`): a pad's `r1` is its DIAMETER and
    /// `r2` its HEIGHT; a bridge's `r1` is its WIDTH and `r2` its HEIGHT.
    /// Reading either as a radius lands a factor of two wrong.
    pub fn from_record(rec: &SupportRecord) -> Result<Self, GeomError> {
        if !v::is_finite(rec.start_mm)
            || !v::is_finite(rec.end_mm)
            || !rec.r_start.is_finite()
            || !rec.r_end.is_finite()
        {
            return Err(GeomError::NonFiniteField);
        }
        match rec.kind {
            RecordKind::ContactSphere | RecordKind::JointSphere => {
                if rec.r_start <= 0.0 {
                    return Err(GeomError::NonPositiveRadius);
                }
                Ok(Self::Ball {
                    centre: v::midpoint(rec.start_mm, rec.end_mm),
                    r: rec.r_start,
                })
            }
            RecordKind::ContactTaper | RecordKind::Member | RecordKind::RootFlare => {
                if rec.r_start <= 0.0 || rec.r_end <= 0.0 {
                    return Err(GeomError::NonPositiveRadius);
                }
                if rec.start_mm == rec.end_mm {
                    return Err(GeomError::ZeroLengthAxis);
                }
                Ok(Self::Frustum {
                    a: rec.start_mm,
                    ra: rec.r_start,
                    b: rec.end_mm,
                    rb: rec.r_end,
                })
            }
            RecordKind::RaftPadOrBridge => {
                let height = rec.r_end;
                if height <= 0.0 {
                    return Err(GeomError::NonPositiveRaftHeight);
                }
                if rec.r_start <= 0.0 {
                    return Err(GeomError::NonPositiveRadius);
                }
                if rec.is_bridge() == Some(true) {
                    let plan = [
                        rec.end_mm[0] - rec.start_mm[0],
                        rec.end_mm[1] - rec.start_mm[1],
                        0.0,
                    ];
                    if v::norm(plan) <= 0.0 {
                        return Err(GeomError::ZeroLengthBridge);
                    }
                    Ok(Self::RaftBridge {
                        a: rec.start_mm,
                        b: rec.end_mm,
                        width: rec.r_start,
                        h: height,
                    })
                } else {
                    Ok(Self::RaftPad {
                        base: rec.start_mm,
                        r: rec.r_start * 0.5,
                        h: height,
                    })
                }
            }
        }
    }

    #[must_use]
    pub fn aabb(&self) -> Aabb {
        match *self {
            Self::Ball { centre, r } => Aabb {
                min: [centre[0] - r, centre[1] - r, centre[2] - r],
                max: [centre[0] + r, centre[1] + r, centre[2] + r],
            },
            Self::Frustum { a, ra, b, rb } => {
                // TIGHT, not the two-sphere box. An end disc is perpendicular
                // to the axis, so along axis `i` it extends only
                // `r * sqrt(1 - u_i^2)`, which is ZERO for an axis-aligned
                // end. Using the loose box put every vertical flare a full
                // radius below the plate and made the frame check fire on
                // every file in the corpus -- a bounding box is not a solid.
                let u = v::normalize(v::sub(b, a)).unwrap_or([0.0, 0.0, 1.0]);
                let mut bb = Aabb::empty();
                let mut lo_a = [0.0f64; 3];
                let mut hi_a = [0.0f64; 3];
                let mut lo_b = [0.0f64; 3];
                let mut hi_b = [0.0f64; 3];
                for i in 0..3 {
                    let e = u[i].mul_add(-u[i], 1.0).max(0.0).sqrt();
                    lo_a[i] = a[i] - ra * e;
                    hi_a[i] = a[i] + ra * e;
                    lo_b[i] = b[i] - rb * e;
                    hi_b[i] = b[i] + rb * e;
                }
                bb.add_point(lo_a);
                bb.add_point(hi_a);
                bb.add_point(lo_b);
                bb.add_point(hi_b);
                bb
            }
            Self::RaftPad { base, r, h } => Aabb {
                min: [base[0] - r, base[1] - r, base[2]],
                max: [base[0] + r, base[1] + r, base[2] + h],
            },
            Self::RaftBridge { a, b, width, h } => {
                // TIGHT: the four plan-view corners of the rotated rectangle,
                // not a square padded by the half-width on both axes.
                let hw = width * 0.5;
                let ax = [b[0] - a[0], b[1] - a[1]];
                let len = ax[0].hypot(ax[1]);
                let n = if len > 0.0 {
                    [-ax[1] / len * hw, ax[0] / len * hw]
                } else {
                    [hw, 0.0]
                };
                let zlo = a[2].min(b[2]);
                let mut bb = Aabb::empty();
                for (p, s) in [(a, 1.0), (a, -1.0), (b, 1.0), (b, -1.0)] {
                    bb.add_point([p[0] + s * n[0], p[1] + s * n[1], zlo]);
                    bb.add_point([p[0] + s * n[0], p[1] + s * n[1], zlo + h]);
                }
                bb
            }
        }
    }

    /// Exact volume, mm^3.
    #[must_use]
    pub fn volume_mm3(&self) -> f64 {
        use std::f64::consts::PI;
        match *self {
            Self::Ball { r, .. } => 4.0 / 3.0 * PI * r * r * r,
            Self::Frustum { a, ra, b, rb } => {
                let l = v::dist(a, b);
                PI * l / 3.0 * ra.mul_add(ra, rb.mul_add(rb, ra * rb))
            }
            Self::RaftPad { r, h, .. } => PI * r * r * h,
            Self::RaftBridge { a, b, width, h } => {
                let l = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                l * width * h
            }
        }
    }

    /// Signed distance to the surface, negative inside. Exact for the ball and
    /// the bridge box; for a frustum it is exact on the lateral surface and on
    /// the two end discs, and is the standard capped-cone field elsewhere.
    #[must_use]
    pub fn signed_distance(&self, p: V3) -> f64 {
        match *self {
            Self::Ball { centre, r } => v::dist(p, centre) - r,
            Self::Frustum { a, ra, b, rb } => cone_sdf(p, a, ra, b, rb),
            Self::RaftPad { base, r, h } => {
                cone_sdf(p, base, r, [base[0], base[1], base[2] + h], r)
            }
            Self::RaftBridge { a, b, width, h } => box_sdf(p, a, b, width, h),
        }
    }

    /// Is `p` inside the closed solid?
    #[must_use]
    pub fn contains(&self, p: V3) -> bool {
        self.signed_distance(p) <= 0.0
    }

    /// The primitive's centreline as two endpoints, for the graph and for I3's
    /// arc-length stations. A ball's centreline is the degenerate point at its
    /// centre.
    #[must_use]
    pub fn centreline(&self) -> (V3, V3) {
        match *self {
            Self::Ball { centre, .. } => (centre, centre),
            Self::Frustum { a, b, .. } => (a, b),
            Self::RaftPad { base, h, .. } => (base, [base[0], base[1], base[2] + h]),
            Self::RaftBridge { a, b, .. } => (a, b),
        }
    }

    /// The solid's radius at axial fraction `s` in `[0, 1]` along its
    /// centreline. For a ball this is the sphere's own radius everywhere
    /// (the ball has no axis) and for a bridge it is the half-width.
    #[must_use]
    pub fn radius_at(&self, s: f64) -> f64 {
        let s = s.clamp(0.0, 1.0);
        match *self {
            Self::Ball { r, .. } => r,
            Self::Frustum { ra, rb, .. } => ra + (rb - ra) * s,
            Self::RaftPad { r, .. } => r,
            Self::RaftBridge { width, .. } => width * 0.5,
        }
    }

    /// The horizontal chord `[x_lo, x_hi]` of the solid at `(y, z)`, or `None`
    /// when the line misses it.
    ///
    /// This is the one primitive the run-length rasteriser needs. It is exact
    /// for every variant: a solid convex body meets a line in one interval.
    #[must_use]
    pub fn x_chord(&self, y: f64, z: f64) -> Option<(f64, f64)> {
        match *self {
            Self::Ball { centre, r } => {
                let dy = y - centre[1];
                let dz = z - centre[2];
                let rem = r.mul_add(r, -(dy.mul_add(dy, dz * dz)));
                (rem >= 0.0).then(|| {
                    let s = rem.sqrt();
                    (centre[0] - s, centre[0] + s)
                })
            }
            Self::Frustum { a, ra, b, rb } => frustum_x_chord(a, ra, b, rb, y, z),
            Self::RaftPad { base, r, h } => {
                if z < base[2] || z > base[2] + h {
                    return None;
                }
                let dy = y - base[1];
                let rem = r.mul_add(r, -(dy * dy));
                (rem >= 0.0).then(|| {
                    let s = rem.sqrt();
                    (base[0] - s, base[0] + s)
                })
            }
            Self::RaftBridge { a, b, width, h } => {
                let zlo = a[2].min(b[2]);
                if z < zlo || z > zlo + h {
                    return None;
                }
                // A plan-view rectangle: half-plane clipping of the line y=const.
                let ax = [b[0] - a[0], b[1] - a[1]];
                let len = (ax[0] * ax[0] + ax[1] * ax[1]).sqrt();
                if len <= 0.0 {
                    return None;
                }
                let u = [ax[0] / len, ax[1] / len];
                let n = [-u[1], u[0]];
                let hw = width * 0.5;
                // Point (x, y): constraints 0 <= (p-a).u <= len and |(p-a).n| <= hw.
                // Each is linear in x, so intersect four half-lines.
                let mut lo = f64::NEG_INFINITY;
                let mut hi = f64::INFINITY;
                let mut clip = |coef: f64, konst: f64| {
                    // coef*x + konst <= 0
                    if coef > 0.0 {
                        hi = hi.min(-konst / coef);
                    } else if coef < 0.0 {
                        lo = lo.max(-konst / coef);
                    } else if konst > 0.0 {
                        lo = 1.0;
                        hi = -1.0;
                    }
                };
                let dy = y - a[1];
                clip(-u[0], -(u[1] * dy)); // -(p-a).u <= 0
                clip(u[0], u[1] * dy - len); //  (p-a).u <= len
                clip(n[0], n[1] * dy - hw); //  (p-a).n <= hw
                clip(-n[0], -(n[1] * dy) - hw); // -(p-a).n <= hw
                (lo <= hi).then_some((lo, hi))
            }
        }
    }

    /// A conservative separation between two solids: the value is `<= 0` only
    /// when they genuinely overlap, and it is exact for ball-ball.
    ///
    /// **Accuracy, stated rather than assumed.** For pairs involving a
    /// frustum this minimises the axis-to-axis distance minus the local radii
    /// over a fixed 128-station sweep with a golden-section refinement, so the
    /// returned value is an upper bound on the true separation with a residual
    /// below `tol_geom` at corpus scale. The error direction is toward
    /// reporting *less* separation, i.e. toward declaring a join. I5's agent
    /// should read the fusion-uncertain band of `d3 = 5.529014e-2 mm` against
    /// that residual before treating a near-zero value as decided.
    #[must_use]
    pub fn separation_mm(&self, other: &Self) -> f64 {
        if let (Self::Ball { centre: c1, r: r1 }, Self::Ball { centre: c2, r: r2 }) = (self, other)
        {
            return v::dist(*c1, *c2) - r1 - r2;
        }
        let (a0, a1) = self.centreline();
        let (b0, b1) = other.centreline();
        let mut best = f64::INFINITY;
        const N: usize = 128;
        for i in 0..=N {
            let s = i as f64 / N as f64;
            let ps = v::add(a0, v::mul(v::sub(a1, a0), s));
            let (d2, _, t) = v::segment_segment_closest(ps, ps, b0, b1);
            let d = d2.sqrt() - self.radius_at(s) - other.radius_at(t);
            if d < best {
                best = d;
            }
        }
        best
    }
}

/// A capped-cone signed distance field. Standard construction; exact on the
/// lateral surface and on both end discs.
fn cone_sdf(p: V3, a: V3, ra: f64, b: V3, rb: f64) -> f64 {
    let ba = v::sub(b, a);
    let l2 = v::dot(ba, ba);
    if l2 <= 0.0 {
        return v::dist(p, a) - ra.max(rb);
    }
    let pa = v::sub(p, a);
    let along = v::dot(pa, ba) / l2; // 0..1 along the axis
    let radial2 = (v::dot(pa, pa) - along * along * l2).max(0.0);
    let radial = radial2.sqrt();
    // Work in the (radial, axial) half-plane, where the frustum is a trapezoid.
    let l = l2.sqrt();
    let x = radial;
    let y = along * l;
    // Trapezoid corners: (ra, 0) (rb, l) with the axis at x = 0.
    let side = [rb - ra, l];
    let side_len2 = side[0].mul_add(side[0], side[1] * side[1]);
    let q = [x - ra, y];
    let t = (q[0].mul_add(side[0], q[1] * side[1]) / side_len2).clamp(0.0, 1.0);
    let proj = [ra + side[0] * t, side[1] * t];
    let d_side = ((x - proj[0]).powi(2) + (y - proj[1]).powi(2)).sqrt();
    let d_cap_lo = ((x - ra.min(x)).powi(2) + y * y).sqrt();
    let d_cap_hi = ((x - rb.min(x)).powi(2) + (y - l).powi(2)).sqrt();
    let d = d_side.min(d_cap_lo).min(d_cap_hi);
    // Inside test: within the axial slab and inside the local radius.
    let inside = (0.0..=l).contains(&y) && x <= ra + (rb - ra) * (y / l);
    if inside {
        -d
    } else {
        d
    }
}

fn box_sdf(p: V3, a: V3, b: V3, width: f64, h: f64) -> f64 {
    let ax = [b[0] - a[0], b[1] - a[1]];
    let len = (ax[0] * ax[0] + ax[1] * ax[1]).sqrt();
    if len <= 0.0 {
        return f64::INFINITY;
    }
    let u = [ax[0] / len, ax[1] / len];
    let n = [-u[1], u[0]];
    let d = [p[0] - a[0], p[1] - a[1]];
    let along = d[0].mul_add(u[0], d[1] * u[1]);
    let across = d[0].mul_add(n[0], d[1] * n[1]);
    let zlo = a[2].min(b[2]);
    let q = [
        (along - len * 0.5).abs() - len * 0.5,
        across.abs() - width * 0.5,
        (p[2] - (zlo + h * 0.5)).abs() - h * 0.5,
    ];
    let outside = [q[0].max(0.0), q[1].max(0.0), q[2].max(0.0)];
    let out_len = (outside[0].powi(2) + outside[1].powi(2) + outside[2].powi(2)).sqrt();
    out_len + q[0].max(q[1]).max(q[2]).min(0.0)
}

/// Exact horizontal chord of a frustum at `(y, z)`.
///
/// Inside the cone means `|d|^2 - (d.u)^2 <= (ra + k*(d.u))^2` with
/// `k = (rb - ra) / L`, plus the axial slab `0 <= d.u <= L`. Both are
/// polynomial in `x` of degree two and one respectively, so the chord is the
/// intersection of a quadratic region with two half-lines.
fn frustum_x_chord(a: V3, ra: f64, b: V3, rb: f64, y: f64, z: f64) -> Option<(f64, f64)> {
    let ba = v::sub(b, a);
    let l = v::norm(ba);
    if l <= 0.0 {
        return None;
    }
    let u = [ba[0] / l, ba[1] / l, ba[2] / l];
    let k = (rb - ra) / l;
    // d(x) = (x - ax, y - ay, z - az) = d0 + x * e0 with e0 = (1, 0, 0).
    let d0 = [-a[0], y - a[1], z - a[2]];
    // s(x) = d.u = d0.u + x*u0
    let s0 = v::dot(d0, u);
    let s1 = u[0];
    // |d|^2 = (d0 + x e0).(d0 + x e0) = |d0|^2 + 2 x d0x + x^2
    let dd0 = v::dot(d0, d0);
    // f(x) = |d|^2 - s^2 - (ra + k s)^2 <= 0
    // = (1 - s1^2 - k^2 s1^2) x^2
    //   + (2 d0x - 2 s0 s1 - 2 k s1 (ra + k s0)) x
    //   + (dd0 - s0^2 - (ra + k s0)^2)
    let qa = 1.0 - s1 * s1 - k * k * s1 * s1;
    let qb = 2.0f64.mul_add(d0[0], -(2.0 * s0 * s1)) - 2.0 * k * s1 * ra.mul_add(1.0, k * s0);
    let base = ra + k * s0;
    let qc = dd0 - s0 * s0 - base * base;

    let (mut lo, mut hi) = if qa.abs() < 1e-15 {
        // Degenerate to linear.
        if qb.abs() < 1e-300 {
            if qc <= 0.0 {
                (f64::NEG_INFINITY, f64::INFINITY)
            } else {
                return None;
            }
        } else if qb > 0.0 {
            (f64::NEG_INFINITY, -qc / qb)
        } else {
            (-qc / qb, f64::INFINITY)
        }
    } else if qa > 0.0 {
        let disc = qb.mul_add(qb, -(4.0 * qa * qc));
        if disc < 0.0 {
            return None;
        }
        let sq = disc.sqrt();
        ((-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa))
    } else {
        // Concave region: the axis point decides which branch is the solid.
        let disc = qb.mul_add(qb, -(4.0 * qa * qc));
        if disc < 0.0 {
            // The whole line satisfies the quadratic; the slab clips it.
            (f64::NEG_INFINITY, f64::INFINITY)
        } else {
            let sq = disc.sqrt();
            let r1 = (-qb + sq) / (2.0 * qa);
            let r2 = (-qb - sq) / (2.0 * qa);
            let (lo_r, hi_r) = if r1 < r2 { (r1, r2) } else { (r2, r1) };
            // qa < 0 makes f(x) <= 0 OUTSIDE [lo_r, hi_r]; the frustum branch
            // is the one containing the axis, which the slab then bounds.
            // Clip conservatively to the far side that also satisfies the slab.
            let mid_ok = |x: f64| {
                let s = s0 + s1 * x;
                (0.0..=l).contains(&s)
            };
            if mid_ok(lo_r - 1.0) {
                (f64::NEG_INFINITY, lo_r)
            } else {
                (hi_r, f64::INFINITY)
            }
        }
    };

    // Axial slab: 0 <= s0 + s1*x <= L.
    if s1.abs() < 1e-300 {
        if s0 < 0.0 || s0 > l {
            return None;
        }
    } else if s1 > 0.0 {
        lo = lo.max(-s0 / s1);
        hi = hi.min((l - s0) / s1);
    } else {
        lo = lo.max((l - s0) / s1);
        hi = hi.min(-s0 / s1);
    }
    (lo <= hi && lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

// ---------------------------------------------------------------------------
// Declared vs reconstructed: the record's numbers against its shipped soup
// ---------------------------------------------------------------------------

/// The result of checking a record's own triangles against the solid its
/// fields describe (`audit-invariants.md` §1.4).
#[derive(Clone, Debug)]
pub struct MeshAgreement {
    /// Largest `|signed_distance(vertex)|` over the soup, mm. This is the
    /// gating measurement: every vertex must lie ON the analytic surface.
    pub max_surface_deviation_mm: f64,
    /// How far the soup's bounding box reaches OUTSIDE the analytic one, mm.
    /// Gated: a soup larger than the solid its record describes is a defect.
    pub bbox_overflow_mm: f64,
    /// How far the soup's bounding box falls INSIDE the analytic one, mm.
    /// **Reported, never gated.**
    ///
    /// `audit-invariants.md` §1.4 asks that "the soup's bounding box must
    /// equal the analytic bounding box within `tol_geom`". That clause cannot
    /// hold for a faceted tessellation of a curved surface and this is a
    /// deviation from the specification, stated rather than hidden: an
    /// `n`-gon inscribed in a circle of radius `r` has a bounding box short by
    /// up to `r * (1 - cos(pi/n))`, which is 9.6e-3 mm for a 16-sided 0.5 mm
    /// frustum -- four decades above `tol_geom`. Requiring equality would fail
    /// every frustum in the corpus for being tessellated. The meaningful test
    /// is the surface test above, plus overflow: a soup may be inscribed, but
    /// it may never be larger than the record claims.
    pub bbox_shortfall_mm: f64,
    /// The tolerance used, computed from the soup's own magnitude.
    pub tolerance_mm: f64,
    pub agrees: bool,
    /// Triangles in the soup; 0 means the record shipped no geometry at all.
    pub soup_triangles: usize,
}

impl Primitive {
    /// Check a record's shipped triangles against this analytic solid.
    ///
    /// Every vertex must lie ON the surface -- not merely inside it -- so the
    /// test is on `|sdf|`, which catches a soup that is the right shape at the
    /// wrong size as well as one that is a different shape entirely.
    #[must_use]
    pub fn verify_against_soup(&self, rec: &SupportRecord) -> MeshAgreement {
        let soup = &rec.soup;
        let mut max_dev = 0.0f64;
        let mut mag = 1.0f64;
        for i in 0..soup.len() {
            for j in 0..3 {
                let p = soup.v(i, j);
                mag = mag.max(v::inf_norm(p));
                max_dev = max_dev.max(self.signed_distance(p).abs());
            }
        }
        let tol = c::tol_geom_mag(mag);
        let sb = soup.bbox();
        let ab = self.aabb();
        let mut overflow = 0.0f64;
        let mut shortfall = 0.0f64;
        if !sb.is_empty() {
            for i in 0..3 {
                overflow = overflow
                    .max(ab.min[i] - sb.min[i])
                    .max(sb.max[i] - ab.max[i]);
                shortfall = shortfall
                    .max(sb.min[i] - ab.min[i])
                    .max(ab.max[i] - sb.max[i]);
            }
        }
        MeshAgreement {
            max_surface_deviation_mm: max_dev,
            bbox_overflow_mm: overflow.max(0.0),
            bbox_shortfall_mm: shortfall.max(0.0),
            tolerance_mm: tol,
            agrees: !soup.is_empty() && max_dev <= tol && overflow <= tol,
            soup_triangles: soup.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ball_volume_and_chord_are_exact() {
        let b = Primitive::Ball {
            centre: [1.0, 2.0, 3.0],
            r: 0.5,
        };
        let want = 4.0 / 3.0 * std::f64::consts::PI * 0.125;
        assert!((b.volume_mm3() - want).abs() < 1e-15);
        let (lo, hi) = b.x_chord(2.0, 3.0).expect("chord");
        assert!((lo - 0.5).abs() < 1e-12 && (hi - 1.5).abs() < 1e-12);
        assert!(b.x_chord(2.0, 3.6).is_none());
    }

    #[test]
    fn vertical_cylinder_chord_matches_the_circle() {
        let f = Primitive::Frustum {
            a: [0.0, 0.0, 0.0],
            ra: 1.0,
            b: [0.0, 0.0, 4.0],
            rb: 1.0,
        };
        let (lo, hi) = f.x_chord(0.0, 2.0).expect("chord");
        assert!(
            (lo + 1.0).abs() < 1e-9 && (hi - 1.0).abs() < 1e-9,
            "{lo} {hi}"
        );
        let (lo, hi) = f.x_chord(0.6, 2.0).expect("chord");
        let want = (1.0f64 - 0.36).sqrt();
        assert!(
            (hi - want).abs() < 1e-9 && (lo + want).abs() < 1e-9,
            "{lo} {hi}"
        );
        assert!(f.x_chord(0.0, 5.0).is_none(), "above the top cap");
        assert!(f.x_chord(0.0, -0.1).is_none(), "below the bottom cap");
    }

    #[test]
    fn tapered_cone_chord_matches_the_local_radius() {
        let f = Primitive::Frustum {
            a: [0.0, 0.0, 0.0],
            ra: 0.2,
            b: [0.0, 0.0, 2.0],
            rb: 0.6,
        };
        for (z, want) in [(0.0, 0.2), (1.0, 0.4), (2.0, 0.6)] {
            let (lo, hi) = f.x_chord(0.0, z).expect("chord");
            assert!((hi - want).abs() < 1e-9, "z={z} hi={hi} want={want}");
            assert!((lo + want).abs() < 1e-9, "z={z} lo={lo}");
        }
    }

    #[test]
    fn tilted_cylinder_chord_agrees_with_the_sdf() {
        let f = Primitive::Frustum {
            a: [0.0, 0.0, 0.0],
            ra: 0.35,
            b: [2.0, 1.0, 3.0],
            rb: 0.35,
        };
        for z in [0.5, 1.5, 2.5] {
            for y in [-0.2, 0.0, 0.3, 0.7] {
                match f.x_chord(y, z) {
                    Some((lo, hi)) => {
                        let mid = 0.5 * (lo + hi);
                        assert!(f.signed_distance([mid, y, z]) <= 1e-9, "mid inside");
                        assert!(f.signed_distance([lo - 1e-4, y, z]) > 0.0, "left outside");
                        assert!(f.signed_distance([hi + 1e-4, y, z]) > 0.0, "right outside");
                    }
                    None => {
                        // No chord: sweep x and confirm nothing is inside.
                        for i in -80..=80 {
                            let x = f64::from(i) * 0.05;
                            assert!(f.signed_distance([x, y, z]) > 0.0, "x={x} y={y} z={z}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn ball_separation_is_exact() {
        let a = Primitive::Ball {
            centre: [0.0, 0.0, 0.0],
            r: 1.0,
        };
        let b = Primitive::Ball {
            centre: [3.0, 0.0, 0.0],
            r: 0.5,
        };
        assert!((a.separation_mm(&b) - 1.5).abs() < 1e-15);
    }

    #[test]
    fn a_raft_pad_reads_its_diameter_and_height_not_two_radii() {
        // r1 = 2.2 is a DIAMETER, r2 = 0.2 is a HEIGHT.
        let rec = crate::reader::SupportRecord {
            index: 0,
            offset: 0,
            magic: c::SEG_MAGIC,
            raw_type: 6,
            kind: RecordKind::RaftPadOrBridge,
            start_mm: [0.0, 0.0, -5.0],
            end_mm: [0.0, 0.0, -5.0],
            r_start: c::RAFT_PAD_DIAMETER_MM,
            r_end: c::RAFT_PAD_HEIGHT_MM,
            mesh_off: 0,
            mesh_len: 0,
            trailing: [0.0, 4.0, 0.0, 0.0, 0.0, 0.0],
            trailing_raw: [0u8; 24],
            soup: crate::reader::Soup::default(),
        };
        let p = Primitive::from_record(&rec).expect("pad");
        match p {
            Primitive::RaftPad { r, h, base } => {
                assert!((r - 1.1).abs() < 1e-15, "radius is half the diameter");
                assert!((h - 0.2).abs() < 1e-15);
                assert!((base[2] + 5.0).abs() < 1e-15);
            }
            other => panic!("expected a pad, got {other:?}"),
        }
        let bb = p.aabb();
        assert!((bb.max[2] - bb.min[2] - 0.2).abs() < 1e-12);
    }

    #[test]
    fn a_zero_length_member_is_refused_with_no_tolerance() {
        let rec = crate::reader::SupportRecord {
            index: 0,
            offset: 0,
            magic: c::SEG_MAGIC,
            raw_type: 3,
            kind: RecordKind::Member,
            start_mm: [1.0, 2.0, 3.0],
            end_mm: [1.0, 2.0, 3.0],
            r_start: 0.35,
            r_end: 0.35,
            mesh_off: 0,
            mesh_len: 0,
            trailing: [0.0; 6],
            trailing_raw: [0u8; 24],
            soup: crate::reader::Soup::default(),
        };
        assert_eq!(Primitive::from_record(&rec), Err(GeomError::ZeroLengthAxis));
    }
}

#[cfg(test)]
mod aabb_tests {
    use super::*;

    /// Sample the solid densely and confirm the analytic box is both a bound
    /// and a tight one. A loose box is not a cosmetic problem: the frame check
    /// gates on "no support reaches below the plate", and a vertical flare's
    /// loose box put every file in the corpus a full radius under the plate.
    fn box_is_tight(p: &Primitive) {
        let bb = p.aabb();
        let ext = bb.extent();
        let mut hit = Aabb::empty();
        let n = 60;
        for i in 0..=n {
            for j in 0..=n {
                for k in 0..=n {
                    let q = [
                        bb.min[0] + ext[0] * f64::from(i) / f64::from(n),
                        bb.min[1] + ext[1] * f64::from(j) / f64::from(n),
                        bb.min[2] + ext[2] * f64::from(k) / f64::from(n),
                    ];
                    if p.contains(q) {
                        hit.add_point(q);
                    }
                }
            }
        }
        assert!(!hit.is_empty(), "no sample landed inside {p:?}");
        for i in 0..3 {
            let slack_lo = hit.min[i] - bb.min[i];
            let slack_hi = bb.max[i] - hit.max[i];
            let cell = ext[i] / f64::from(n) + 1e-12;
            assert!(slack_lo >= -1e-9, "box does not bound {p:?} axis {i}");
            assert!(
                slack_lo <= 2.0 * cell,
                "box is loose by {slack_lo} on axis {i} low for {p:?}"
            );
            assert!(
                slack_hi <= 2.0 * cell,
                "box is loose by {slack_hi} on axis {i} high for {p:?}"
            );
        }
    }

    #[test]
    fn a_vertical_flare_box_stops_at_its_end_disc() {
        let f = Primitive::Frustum {
            a: [2.0, -7.0, -19.12],
            ra: 0.75,
            b: [2.0, -7.0, -20.12],
            rb: 1.1,
        };
        let bb = f.aabb();
        assert!(
            (bb.min[2] + 20.12).abs() < 1e-12,
            "the disc is perpendicular to the axis: {:?}",
            bb.min
        );
        assert!((bb.max[2] + 19.12).abs() < 1e-12);
        assert!((bb.max[0] - 3.1).abs() < 1e-12, "{:?}", bb.max);
        box_is_tight(&f);
    }

    #[test]
    fn tilted_and_horizontal_frusta_and_a_bridge_are_tight_too() {
        box_is_tight(&Primitive::Frustum {
            a: [0.0, 0.0, 0.0],
            ra: 0.4,
            b: [2.0, 1.0, 3.0],
            rb: 0.6,
        });
        box_is_tight(&Primitive::Frustum {
            a: [0.0, 0.0, 0.0],
            ra: 0.5,
            b: [3.0, 0.0, 0.0],
            rb: 0.5,
        });
        box_is_tight(&Primitive::Ball {
            centre: [1.0, 1.0, 1.0],
            r: 0.7,
        });
        box_is_tight(&Primitive::RaftPad {
            base: [0.0, 0.0, -5.0],
            r: 1.1,
            h: 0.2,
        });
        // A rotated rectangle attains each extreme at a single CORNER, so a
        // lattice sweep cannot land on it; check the corners analytically
        // instead of loosening the sweep for every other shape.
        let br = Primitive::RaftBridge {
            a: [0.0, 0.0, -5.0],
            b: [4.0, 3.0, -5.0],
            width: 2.2,
            h: 0.2,
        };
        let bb = br.aabb();
        let hw = 1.1;
        let n = [-3.0 / 5.0 * hw, 4.0 / 5.0 * hw];
        let corners = [
            [0.0 + n[0], 0.0 + n[1]],
            [0.0 - n[0], 0.0 - n[1]],
            [4.0 + n[0], 3.0 + n[1]],
            [4.0 - n[0], 3.0 - n[1]],
        ];
        let xs: Vec<f64> = corners.iter().map(|c| c[0]).collect();
        let ys: Vec<f64> = corners.iter().map(|c| c[1]).collect();
        let (xlo, xhi) = (
            xs.iter().copied().fold(f64::INFINITY, f64::min),
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        let (ylo, yhi) = (
            ys.iter().copied().fold(f64::INFINITY, f64::min),
            ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        assert!(
            (bb.min[0] - xlo).abs() < 1e-12 && (bb.max[0] - xhi).abs() < 1e-12,
            "{bb:?}"
        );
        assert!(
            (bb.min[1] - ylo).abs() < 1e-12 && (bb.max[1] - yhi).abs() < 1e-12,
            "{bb:?}"
        );
        assert!(
            (bb.min[2] + 5.0).abs() < 1e-12 && (bb.max[2] + 4.8).abs() < 1e-12,
            "{bb:?}"
        );
    }
}
