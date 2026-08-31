//! The seat reconstruction of `audit-invariants.md` §3.1, computed ONCE and
//! shared.
//!
//! `h_delivered` is not I1's private number. I2 positions its cut plane at it,
//! I3 starts its station walk at it, I5's ground clause (ii) depends on it and
//! I6's seat zone is bounded by it. If four checks each recomputed it they
//! would disagree at the margin and the audit would grade four different
//! solids. So this module computes the **reconstruction** -- geometry only --
//! and I1 applies the **predicate** and emits the failure lines on top of it.
//!
//! Nothing here decides pass or fail. [`Seat::indeterminate`] carries the exact
//! `reason=` token a consuming check must report, and an indeterminate seat
//! fails whichever invariant asked.

use crate::consts as c;
use crate::graph::{Link, SupportGraph};
use crate::mesh::{Inside, Mesh};
use crate::reader::{ModelEntry, RecordKind};
use crate::vec3::{self as v, Aabb, V3};

/// Which term of the `h_req` maximum is binding for a contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingTerm {
    /// The ruled `default_contact_depth_mm` (0.100 mm).
    Default,
    /// `layer_height + r_tip * sin(theta)`.
    Incidence,
    /// `ball_seat_factor(f) * r_tip`.
    Ball,
}

impl BindingTerm {
    /// The `binding_term=` token of the `SEAT_DEPTH_SHALLOW` failure line.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Incidence => "incidence",
            Self::Ball => "ball",
        }
    }
}

/// The reconstructed seat of one contact. Geometry only; no verdict.
#[derive(Clone, Debug)]
pub struct Seat {
    /// `contact_id` = the record index of the type-7 TIP record.
    pub contact_id: usize,
    /// The type-1 TAPER adjacent to this tip, or why there is not one.
    pub taper: Link,

    /// `start` of the TIP record: the ball's deepest point, its south pole.
    pub p0: V3,
    /// `normalize(end - start)`, the outward unit axis. RECONSTRUCTED.
    pub u: Option<V3>,
    /// `midpoint(start, end)`, the ball centre.
    pub centre: V3,
    /// `R = r1 = r2`, the ball radius. DECLARED, cross-checked below.
    pub r_ball_mm: f64,

    /// `r_tip_A = R / f`, from the TIP record.
    pub r_tip_a_mm: f64,
    /// `r_tip_B = r_start` of the adjacent TAPER record.
    pub r_tip_b_mm: Option<f64>,
    /// `r_tip_C` = radius of the sphere fitted to the TIP soup. RECONSTRUCTED.
    pub r_tip_c_mm: Option<f64>,
    /// Centre of that fitted sphere, for the same three-way comparison.
    pub fitted_centre_mm: Option<V3>,
    /// Largest pairwise difference among the three radii, mm.
    pub r_tip_spread_mm: f64,
    /// The tolerance the three-way comparison was made at.
    pub r_tip_tol_mm: f64,
    /// All three agree within `tol_geom`. Disagreement is
    /// `SEAT_DEPTH_DECLARED_MISMATCH` reason `tip_radius_three_way`.
    pub r_tip_agrees: bool,
    /// The tip radius the requirement is computed from: `r_tip_A`, which is
    /// the one the TIP record itself carries.
    pub r_tip_mm: f64,

    /// `h_delivered`: distance from `p0` to the first crossing of the model
    /// surface travelling along `+u`. **Zero when `p0` is outside `M`** --
    /// which is itself a hard failure (`SEAT_DEPTH_ZERO`).
    pub h_delivered_mm: Option<f64>,
    /// Where that crossing landed.
    pub entry_point_mm: Option<V3>,
    /// The outward model normal at the entry crossing. RECONSTRUCTED.
    pub entry_normal: Option<V3>,
    /// The model triangle crossed. When [`Seat::entry_on_mesh_boundary`] is
    /// set, several triangles met the axis at the same distance and this is
    /// the first of them; `entry_normal` is their area-weighted mean.
    pub entry_triangle: Option<usize>,
    /// The axis left through an edge or a vertex shared by several facets.
    /// The DISTANCE is still determined; the NORMAL is the thing the
    /// 5-degree cross-check then has to settle.
    pub entry_on_mesh_boundary: bool,
    /// The surface component of the entry triangle; this is what routes the
    /// contact for the ownership partition (§2.2).
    pub entry_component: Option<usize>,
    /// Perpendicular distance from `p0` to the nearest point of the model
    /// surface. **Diagnostic only** -- the seat formula's cut plane is
    /// perpendicular to the axis, not to the surface.
    pub h_perp_mm: Option<f64>,

    /// `|u . n|` at the entry.
    pub abs_cos: Option<f64>,
    /// `sqrt(max(0, 1 - abs_cos^2))`.
    pub sin_theta: Option<f64>,
    /// The area-weighted mean normal over the ball's intersection with the
    /// model surface, used to cross-check `entry_normal`.
    pub mean_normal: Option<V3>,
    /// Angle between `entry_normal` and `mean_normal`, degrees.
    pub normal_disagreement_deg: Option<f64>,
    /// Set when that angle exceeds 5 degrees: the incidence term is then not
    /// established and the contact is indeterminate.
    pub normal_ambiguous: bool,

    /// `h_req(c) = max(default, layer + r_tip*sin_theta, ball_seat*r_tip)`.
    pub h_required_mm: Option<f64>,
    pub binding_term: Option<BindingTerm>,
    /// The three terms in order, for the `measured` block.
    pub h_terms_mm: Option<[f64; 3]>,

    /// The declared contact depth from the TAPER's trailing slot 0. **Known to
    /// lie**: every emitted taper declares 0.3.
    pub declared_depth_mm: Option<f64>,

    /// The exact `reason=` token when the seat could not be established.
    /// `None` means every quantity above is established.
    pub indeterminate: Option<&'static str>,
}

impl Seat {
    /// The `tol_geom` appropriate to this seat's own magnitudes.
    #[must_use]
    pub fn tol_geom_mm(&self) -> f64 {
        c::tol_geom(self.p0, self.entry_point_mm.unwrap_or(self.p0))
    }
    /// `h_delivered` when it is established, else `None`.
    #[must_use]
    pub const fn delivered(&self) -> Option<f64> {
        self.h_delivered_mm
    }
    /// The point on the axis at the delivered depth. I2's cut plane passes
    /// through it and I2's patch component is the one containing it.
    #[must_use]
    pub fn axis_point_at_depth(&self) -> Option<V3> {
        let u = self.u?;
        let h = self.h_delivered_mm?;
        Some(v::add(self.p0, v::mul(u, h)))
    }
}

/// Reconstruct every contact's seat for one model.
///
/// `ball_factor` is the declared `ball_factor_stub`. `model_mesh` must be the
/// welded model mesh; a mesh that is not closed makes every parity query
/// indeterminate, which is the correct cascade.
#[must_use]
pub fn reconstruct_seats(
    model: &ModelEntry,
    mesh: &Mesh,
    graph: &SupportGraph,
    ball_factor: f64,
    layer_height_mm: f64,
    default_contact_depth_mm: f64,
) -> Vec<Seat> {
    model
        .contact_records()
        .into_iter()
        .map(|ri| {
            reconstruct_one(
                model,
                mesh,
                graph,
                ri,
                ball_factor,
                layer_height_mm,
                default_contact_depth_mm,
            )
        })
        .collect()
}

fn reconstruct_one(
    model: &ModelEntry,
    mesh: &Mesh,
    graph: &SupportGraph,
    ri: usize,
    f: f64,
    layer_height_mm: f64,
    default_contact_depth_mm: f64,
) -> Seat {
    let rec = &model.records[ri];
    debug_assert_eq!(rec.kind, RecordKind::ContactSphere);
    let p0 = rec.start_mm;
    let centre = v::midpoint(rec.start_mm, rec.end_mm);
    let r_ball = rec.r_start;
    let u = v::normalize(v::sub(rec.end_mm, rec.start_mm));

    let taper = graph.taper_for_tip(model, ri);
    let r_tip_a = r_ball / f;
    let r_tip_b = taper.unique().map(|t| model.records[t].r_start);
    let (fitted_centre, r_tip_c_ball) = fit_sphere(rec);
    // The fitted sphere is the BALL, radius R; the tip radius it implies is
    // R / f, the same reduction the record's own radius gets.
    let r_tip_c = r_tip_c_ball.map(|r| r / f);

    let mut radii: Vec<f64> = vec![r_tip_a];
    if let Some(b) = r_tip_b {
        radii.push(b);
    }
    if let Some(cc) = r_tip_c {
        radii.push(cc);
    }
    let rmin = radii.iter().copied().fold(f64::INFINITY, f64::min);
    let rmax = radii.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let spread = rmax - rmin;
    let r_tol = c::tol_geom(p0, centre).max(c::radius_tol_mm(r_ball));
    let r_tip_agrees = spread <= r_tol;

    let declared_depth_mm = taper
        .unique()
        .and_then(|t| model.records[t].declared_contact_depth_mm());

    let mut seat = Seat {
        contact_id: ri,
        taper,
        p0,
        u,
        centre,
        r_ball_mm: r_ball,
        r_tip_a_mm: r_tip_a,
        r_tip_b_mm: r_tip_b,
        r_tip_c_mm: r_tip_c,
        fitted_centre_mm: fitted_centre,
        r_tip_spread_mm: spread,
        r_tip_tol_mm: r_tol,
        r_tip_agrees,
        r_tip_mm: r_tip_a,
        h_delivered_mm: None,
        entry_point_mm: None,
        entry_normal: None,
        entry_triangle: None,
        entry_on_mesh_boundary: false,
        entry_component: None,
        h_perp_mm: None,
        abs_cos: None,
        sin_theta: None,
        mean_normal: None,
        normal_disagreement_deg: None,
        normal_ambiguous: false,
        h_required_mm: None,
        binding_term: None,
        h_terms_mm: None,
        declared_depth_mm,
        indeterminate: None,
    };

    // h_perp is diagnostic and cheap; take it whatever else happens.
    seat.h_perp_mm = mesh.nearest_surface(p0).map(|h| h.distance_mm);

    let Some(u) = u else {
        seat.indeterminate = Some("tip_axis_degenerate");
        return seat;
    };

    match mesh.point_inside(p0) {
        Inside::Indeterminate => {
            seat.indeterminate = Some("ray_parity_exhausted");
            return seat;
        }
        Inside::Outside => {
            // A ball resting on or outside the surface transfers load across a
            // tangent point and has no cut disc. Zero is established, not
            // unknown, so this is NOT indeterminate -- it is I1's hard failure.
            seat.h_delivered_mm = Some(0.0);
        }
        Inside::Inside => {
            let tol = c::tol_geom_at(p0);
            // The axis direction is the question, so the parity test's
            // retry-another-direction escape does not apply here. Vertex and
            // edge crossings are RESOLVED rather than refused; see
            // `Mesh::axis_crossing`. Measured on the corpus: the engine seats
            // every contact exactly on a model vertex, so every axis ray
            // leaves through one.
            match mesh.axis_crossing(p0, u, tol) {
                Err(_) => {
                    seat.indeterminate = Some("ray_parity_exhausted");
                    return seat;
                }
                Ok(None) => {
                    // Inside the solid but the axis never leaves it: the mesh
                    // is not the solid it claims to be.
                    seat.indeterminate = Some("ray_parity_exhausted");
                    return seat;
                }
                Ok(Some(h)) => {
                    seat.h_delivered_mm = Some(h.t);
                    seat.entry_point_mm = Some(h.point);
                    seat.entry_normal = Some(h.normal);
                    seat.entry_triangle = h.triangles.first().copied();
                    seat.entry_on_mesh_boundary = h.on_mesh_boundary;
                    seat.entry_component = h
                        .triangles
                        .first()
                        .map(|t| mesh.component_of_tri[*t] as usize);
                }
            }
        }
    }

    if let Some(n) = seat.entry_normal {
        let abs_cos = v::dot(u, n).abs().min(1.0);
        seat.abs_cos = Some(abs_cos);
        seat.sin_theta = Some(abs_cos.mul_add(-abs_cos, 1.0).max(0.0).sqrt());

        // Cross-check against the area-weighted mean normal over the ball's
        // intersection with the model surface.
        //
        // APPROXIMATION, stated: the mean is taken over WHOLE triangles whose
        // closest point to the centre lies within R, weighted by full triangle
        // area, rather than over the clipped intersection. On a mesh whose
        // facets are much smaller than R the two agree; on a coarse mesh the
        // approximation is why the 5-degree gate exists at all.
        let q = Aabb {
            min: [centre[0] - r_ball, centre[1] - r_ball, centre[2] - r_ball],
            max: [centre[0] + r_ball, centre[1] + r_ball, centre[2] + r_ball],
        };
        let mut acc = [0.0f64; 3];
        let mut wsum = 0.0f64;
        for ti in mesh.triangles_in_box(&q) {
            let [a, b, cc] = mesh.tri(ti);
            let (d, _) = v::point_triangle_closest(centre, a, b, cc);
            if d <= r_ball {
                let w = mesh.areas[ti];
                acc = v::add(acc, v::mul(mesh.normals[ti], w));
                wsum += w;
            }
        }
        if wsum > 0.0 {
            if let Some(mean) = v::normalize(acc) {
                seat.mean_normal = Some(mean);
                let cosang = v::dot(mean, n).clamp(-1.0, 1.0);
                let deg = cosang.acos().to_degrees();
                seat.normal_disagreement_deg = Some(deg);
                if deg > 5.0 {
                    seat.normal_ambiguous = true;
                    seat.indeterminate = Some("normal_ambiguous");
                }
            }
        }
    }

    if let Some(sin_theta) = seat.sin_theta {
        let t_default = default_contact_depth_mm;
        let t_incidence = layer_height_mm + r_tip_a * sin_theta;
        let t_ball = c::ball_seat_factor(f) * r_tip_a;
        let terms = [t_default, t_incidence, t_ball];
        let (mut which, mut best) = (BindingTerm::Default, t_default);
        if t_incidence > best {
            which = BindingTerm::Incidence;
            best = t_incidence;
        }
        if t_ball > best {
            which = BindingTerm::Ball;
            best = t_ball;
        }
        seat.h_required_mm = Some(best);
        seat.binding_term = Some(which);
        seat.h_terms_mm = Some(terms);
    } else if seat.h_delivered_mm == Some(0.0) {
        // No entry means no incidence; the requirement is still defined by the
        // two incidence-free terms, so a zero seat can still be graded.
        let t_default = default_contact_depth_mm;
        let t_ball = c::ball_seat_factor(f) * r_tip_a;
        let terms = [t_default, layer_height_mm, t_ball];
        let (which, best) = if t_ball > t_default {
            (BindingTerm::Ball, t_ball)
        } else {
            (BindingTerm::Default, t_default)
        };
        seat.h_required_mm = Some(best);
        seat.binding_term = Some(which);
        seat.h_terms_mm = Some(terms);
    }

    if taper_missing(&seat.taper) && seat.indeterminate.is_none() {
        // The DECLARED comparison cannot be made without the taper. The
        // geometry above stands; the comparison does not.
        seat.indeterminate = Some(seat.taper.reason());
    }
    seat
}

const fn taper_missing(l: &Link) -> bool {
    !matches!(l, Link::Unique(_))
}

/// Algebraic least-squares sphere fit to a record's own soup vertices.
///
/// Solves `|p|^2 = 2 c . p + (r^2 - |c|^2)` in the four unknowns
/// `(cx, cy, cz, k)` by normal equations with Gaussian elimination. Returns
/// `None` for a soup too small or too degenerate to determine a sphere -- which
/// is itself worth reporting, never a reason to assume the declared radius.
#[must_use]
pub fn fit_sphere(rec: &crate::reader::SupportRecord) -> (Option<V3>, Option<f64>) {
    let soup = &rec.soup;
    let n = soup.len() * 3;
    if n < 4 {
        return (None, None);
    }
    let mut ata = [[0.0f64; 4]; 4];
    let mut atb = [0.0f64; 4];
    for i in 0..soup.len() {
        for j in 0..3 {
            let p = soup.v(i, j);
            let row = [2.0 * p[0], 2.0 * p[1], 2.0 * p[2], 1.0];
            let b = v::dot(p, p);
            for a in 0..4 {
                for c2 in 0..4 {
                    ata[a][c2] += row[a] * row[c2];
                }
                atb[a] += row[a] * b;
            }
        }
    }
    let Some(x) = solve4(ata, atb) else {
        return (None, None);
    };
    let centre = [x[0], x[1], x[2]];
    let r2 = x[3] + v::dot(centre, centre);
    if !r2.is_finite() || r2 <= 0.0 {
        return (Some(centre), None);
    }
    (Some(centre), Some(r2.sqrt()))
}

fn solve4(mut a: [[f64; 4]; 4], mut b: [f64; 4]) -> Option<[f64; 4]> {
    for col in 0..4 {
        let mut piv = col;
        for r in (col + 1)..4 {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        if a[piv][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        for r in (col + 1)..4 {
            let f = a[r][col] / a[col][col];
            for cc in col..4 {
                a[r][cc] -= f * a[col][cc];
            }
            b[r] -= f * b[col];
        }
    }
    let mut x = [0.0f64; 4];
    for r in (0..4).rev() {
        let mut s = b[r];
        for cc in (r + 1)..4 {
            s -= a[r][cc] * x[cc];
        }
        x[r] = s / a[r][r];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts as c;
    use crate::reader::{Soup, SupportRecord};

    fn sphere_soup(centre: V3, r: f64) -> Soup {
        // A coarse UV sphere; only the fit is under test.
        let (lat, lon) = (8usize, 16usize);
        let mut tris = Vec::new();
        let pt = |la: usize, lo: usize| -> [f32; 3] {
            let th = std::f64::consts::PI * la as f64 / lat as f64;
            let ph = 2.0 * std::f64::consts::PI * lo as f64 / lon as f64;
            [
                (centre[0] + r * th.sin() * ph.cos()) as f32,
                (centre[1] + r * th.sin() * ph.sin()) as f32,
                (centre[2] + r * th.cos()) as f32,
            ]
        };
        for la in 0..lat {
            for lo in 0..lon {
                let (a, b, cc, d) = (
                    pt(la, lo),
                    pt(la + 1, lo),
                    pt(la + 1, lo + 1),
                    pt(la, lo + 1),
                );
                tris.push([a, b, cc]);
                tris.push([a, cc, d]);
            }
        }
        Soup {
            tris,
            source_offset: 0,
        }
    }

    #[test]
    fn the_sphere_fit_recovers_centre_and_radius() {
        let rec = SupportRecord {
            index: 0,
            offset: 0,
            magic: c::SEG_MAGIC,
            raw_type: 7,
            kind: RecordKind::ContactSphere,
            start_mm: [1.0, 2.0, 2.7],
            end_mm: [1.0, 2.0, 3.3],
            r_start: 0.3,
            r_end: 0.3,
            mesh_off: 0,
            mesh_len: 0,
            trailing: [0.0; 6],
            trailing_raw: [0u8; 24],
            soup: sphere_soup([1.0, 2.0, 3.0], 0.3),
        };
        let (cn, r) = fit_sphere(&rec);
        let cn = cn.expect("centre");
        let r = r.expect("radius");
        assert!(v::dist(cn, [1.0, 2.0, 3.0]) < 1e-5, "{cn:?}");
        assert!((r - 0.3).abs() < 1e-5, "{r}");
    }

    #[test]
    fn a_soup_too_small_to_determine_a_sphere_says_so() {
        let rec = SupportRecord {
            index: 0,
            offset: 0,
            magic: c::SEG_MAGIC,
            raw_type: 7,
            kind: RecordKind::ContactSphere,
            start_mm: [0.0, 0.0, 0.0],
            end_mm: [0.0, 0.0, 0.6],
            r_start: 0.3,
            r_end: 0.3,
            mesh_off: 0,
            mesh_len: 0,
            trailing: [0.0; 6],
            trailing_raw: [0u8; 24],
            soup: Soup::default(),
        };
        assert_eq!(fit_sphere(&rec).1, None);
    }

    #[test]
    fn the_binding_term_switches_where_the_ruling_says_it_should() {
        // At normal incidence the ball term overtakes the 0.100 mm default
        // only for r_tip > 0.100 / 0.3819660112501051 = 0.261803 mm.
        let f = c::BALL_FACTOR_STUB;
        let bsf = c::ball_seat_factor(f);
        assert!((0.100 / bsf - 0.261_803_398_874_989_5).abs() < 1e-12);
        assert!(bsf * 0.26 < 0.100, "below the switch the default binds");
        assert!(bsf * 0.27 > 0.100, "above it the ball term binds");
        // The incidence term overtakes the default only for
        // r_tip * sin(theta) > 0.050 mm.
        let incidence = |r_sin: f64| c::LAYER_HEIGHT_DEFAULT_MM + r_sin;
        assert!(incidence(0.049) < c::DEFAULT_CONTACT_DEPTH_MM);
        assert!(incidence(0.051) > c::DEFAULT_CONTACT_DEPTH_MM);
    }
}
