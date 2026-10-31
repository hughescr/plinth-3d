use super::native::{ENVELOPE, cross, distance, dot, norm, sub};
use super::*;
use plinth_geom::{
    Bvh, ClosestHits, Points,
    clearance::{self, Solid},
    closest_points,
    measure::Measure,
    patch::{self, Ball},
};

struct Prepared {
    contact: Contact,
    outer: Ball,
    inner: Ball,
    cone: Solid,
    joint: Solid,
    normal: [f64; 3],
    site: [f64; 3],
    axis: [f64; 3],
}

pub(super) fn probe(
    plan: &PlanBuilder<Contacts>,
    cands: &[ContactCandidate],
    out: &mut Vec<Result<Quote, Reject>>,
) {
    out.clear();
    if cands.len() > 4096 || plan.contacts.len() + cands.len() > 4096 {
        out.extend(cands.iter().map(|_| Err(Reject::ResourceExhausted)));
        return;
    }
    if !plan.process.valid() {
        out.extend(cands.iter().map(|_| Err(Reject::NonFinite)));
        return;
    }
    let Ok(bvh) = Bvh::build(plan.model.mesh()) else {
        out.extend(cands.iter().map(|_| Err(Reject::PatchUnverified)));
        return;
    };
    // Sanitize only the query buffer, not candidates: invalid fields are rejected in
    // prepare. One malformed candidate must not poison a valid sibling's query.
    let points: Vec<_> = cands
        .iter()
        .map(|c| {
            c.site
                .map(|v| if v.is_finite() { f64::from(v) } else { 0.0 })
        })
        .collect();
    let points = Points::new(
        points.iter().map(|p| p[0]).collect(),
        points.iter().map(|p| p[1]).collect(),
        points.iter().map(|p| p[2]).collect(),
    )
    .expect("sanitized sites");
    let mut hits = ClosestHits::with_len(cands.len());
    if closest_points(&bvh, &points, &mut hits).is_err() {
        out.extend(cands.iter().map(|_| Err(Reject::MeasureExhausted)));
        return;
    }
    let prepared: Vec<_> = cands
        .iter()
        .enumerate()
        .map(|(i, c)| prepare(plan, c, hits.tri[i] as usize, hits.dist2[i]))
        .collect();
    let valid: Vec<_> = prepared.iter().filter_map(|p| p.as_ref().ok()).collect();
    let balls: Vec<_> = valid.iter().flat_map(|p| [p.outer, p.inner]).collect();
    let solids: Vec<_> = valid.iter().flat_map(|p| [p.joint, p.cone]).collect();
    let mut components = Vec::new();
    patch::ball_patch_components(&bvh, &balls, &mut components);
    let mut areas = Vec::new();
    let mut gaps = Vec::new();
    patch::ball_patches_with_workers(&bvh, &balls, &mut areas, plan.workers);
    clearance::solid_gaps_with_workers(&bvh, &solids, &mut gaps, plan.workers);
    let mut at = 0;
    for result in prepared {
        match result {
            Err(e) => out.push(Err(e)),
            Ok(mut p) => {
                let measured = if components[at] != 1 {
                    Err(Reject::SecondSheet)
                } else {
                    check_measurements(
                        plan,
                        &p,
                        &areas[at],
                        &areas[at + 1],
                        &gaps[at],
                        &gaps[at + 1],
                    )
                };
                at += 2;
                out.push(measured.map(|patch| {
                    p.contact.patch = patch;
                    Quote {
                        plan: plan.id,
                        rev: plan.rev,
                        contact: p.contact,
                    }
                }));
            }
        }
    }
    // Symmetric intra-batch conflicts: no ordering-dependent winner. Separate batches
    // are checked again at commit, so they cannot bypass this stage.
    let mut rejects = vec![None; cands.len()];
    for (i, r) in out.iter().enumerate() {
        let Ok(q) = r else { continue };
        for old in &plan.contacts {
            if let Err(e) = pair(&q.contact, old) {
                rejects[i] = Some(e);
                break;
            }
        }
        for (j, other) in out[..i].iter().enumerate() {
            if let Ok(other) = other
                && let Err(e) = pair(&q.contact, &other.contact)
            {
                rejects[i] = Some(e);
                rejects[j] = Some(pair(&other.contact, &q.contact).unwrap_err());
            }
        }
    }
    for (r, reject) in out.iter_mut().zip(rejects) {
        if let Some(e) = reject {
            *r = Err(e);
        }
    }
}

fn prepare(
    plan: &PlanBuilder<Contacts>,
    c: &ContactCandidate,
    tri: usize,
    site_distance: f64,
) -> Result<Prepared, Reject> {
    let fields = c
        .site
        .into_iter()
        .chain(c.axis)
        .chain([c.ball_r, c.standoff, c.neck, c.joint_r, c.head_len]);
    if fields.clone().any(|v| !v.is_finite() || v.abs() > 100.0) {
        return Err(Reject::NonFinite);
    }
    if site_distance > 1e-10 {
        return Err(Reject::PatchUnverified);
    }
    let neck = Section::new(c.neck)?;
    let end = Section::new(c.joint_r / 1.04)?;
    if c.ball_r <= 0.0
        || c.standoff < 0.0
        || c.standoff >= c.ball_r
        || c.joint_r < 0.26
        || c.head_len < 1.5
        || f64::from(c.head_len + c.joint_r) > 2.95
        || f64::from(c.head_len + c.joint_r) > plan.limits.region
        || c.neck > c.joint_r / 1.04
    {
        return Err(Reject::UnknownShape);
    }
    let mesh = plan.model.mesh();
    let t = std::array::from_fn::<_, 3, _>(|k| {
        let i = mesh.indices()[3 * tri + k] as usize;
        [mesh.xs()[i], mesh.ys()[i], mesh.zs()[i]]
    });
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let n = n.map(|v| v / norm(n));
    let axis = c.axis.map(f64::from);
    if (norm(axis) - 1.0).abs() > 1e-6 || axis[2] >= -0.01 {
        return Err(Reject::PoleUncovered);
    }
    let centre =
        std::array::from_fn(|i| (f64::from(c.site[i]) + n[i] * f64::from(c.standoff)) as f32);
    let joint = std::array::from_fn(|i| centre[i] + c.axis[i] * c.head_len);
    let ball = NativePart::ball(PartKind::TipBall, centre, c.axis, c.ball_r)?;
    let joint_ball = NativePart::ball(PartKind::JointBall, joint, [0.0, 0.0, -1.0], c.joint_r)?;
    let cone = NativePart::frustum(PartKind::TipCone, centre, joint, neck, end)?;
    let inner = ball.ball_inner()?;
    let joint_inner = joint_ball.ball_inner()?;
    if joint_inner.radius < f64::from(end.radius()).max(0.25) + ENVELOPE {
        return Err(Reject::NarrowJoin);
    }
    let Solid::Ball(outer) = ball.solid().ok_or(Reject::UnknownShape)? else {
        return Err(Reject::UnknownShape);
    };
    let cone_solid = cone.solid().ok_or(Reject::UnknownShape)?;
    let joint_solid = joint_ball.solid().ok_or(Reject::UnknownShape)?;
    if let Solid::Ball(joint_envelope) = joint_solid
        && norm(sub(outer.centre, joint_envelope.centre)) + joint_envelope.radius > 2.95
    {
        return Err(Reject::UnknownShape);
    }
    // Throughout the cone, every cross-section contains a disc at least as wide as
    // the checked start section. No thin lens joins: both ends reach ball centres.
    // The lowest ball pole must be strictly inside the cone's inscribed polygon.
    let pole_t = -f64::from(c.ball_r) * axis[2];
    let pole_rad = f64::from(c.ball_r) * (1.0 - axis[2] * axis[2]).max(0.0).sqrt();
    let cone_rad =
        f64::from(c.neck) + (f64::from(end.radius() - c.neck)) * pole_t / f64::from(c.head_len);
    if pole_t > f64::from(c.head_len)
        || cone_rad * (std::f64::consts::PI / 16.0).cos() - ENVELOPE < pole_rad
    {
        return Err(Reject::PoleUncovered);
    }
    Ok(Prepared {
        contact: Contact {
            cand: c.id,
            centre,
            radius: c.ball_r,
            joint,
            joint_radius: c.joint_r,
            patch: CappedPatch(PatchBound { lo: 0.0, hi: 0.0 }),
            parts: vec![ball, cone, joint_ball],
        },
        outer,
        inner,
        cone: cone_solid,
        joint: joint_solid,
        normal: n,
        site: std::array::from_fn(|i| {
            f64::from(c.site[i]) - n[i] * dot(sub(c.site.map(f64::from), t[0]), n)
        }),
        axis,
    })
}

fn check_measurements(
    plan: &PlanBuilder<Contacts>,
    p: &Prepared,
    outer: &Measure<patch::AreaBounds>,
    inner: &Measure<patch::AreaBounds>,
    joint: &Measure<clearance::GapBounds>,
    cone: &Measure<clearance::GapBounds>,
) -> Result<CappedPatch, Reject> {
    // Joint always outside the region. Check it BEFORE any departure restriction so
    // a nearby second sheet is a located GapTooSmall, never a region exemption.
    let joint = joint.known().ok_or(Reject::MeasureExhausted)?;
    if joint.lo < plan.limits.gap {
        return Err(Reject::GapTooSmall { gap: joint.lo });
    }
    let outer = outer.known().ok_or(Reject::MeasureExhausted)?;
    let inner = inner.known().ok_or(Reject::MeasureExhausted)?;
    if inner.lo <= 0.0 {
        return Err(Reject::PatchUnverified);
    }
    let patch = PatchBound {
        lo: inner.lo.min(section_capacity(p)),
        hi: outer.hi,
    }
    .cap(&plan.limits)?;
    let cone = cone.known().ok_or(Reject::MeasureExhausted)?;
    if cone.lo - ENVELOPE <= 0.02 {
        return Err(Reject::HeadTouchesModel);
    }
    // Phase-one certified approach domain: normal departure from a global supporting
    // plane. Non-normal/concave approaches are refused, not approximated by a centreline.
    if norm(sub(p.axis, p.normal)) > 1e-6 {
        return Err(Reject::DepartureNotMonotone);
    }
    let mesh = plan.model.mesh();
    for i in 0..mesh.vertex_count() {
        let v = [mesh.xs()[i], mesh.ys()[i], mesh.zs()[i]];
        if dot(sub(v, p.site), p.normal) > 1e-6 {
            return Err(Reject::DepartureNotMonotone);
        }
    }
    certify_planar_departure(plan, p)?;
    Ok(patch)
}
fn certify_planar_departure(plan: &PlanBuilder<Contacts>, p: &Prepared) -> Result<(), Reject> {
    let mesh = plan.model.mesh();
    let mut edges = std::collections::BTreeMap::<(u32, u32), u32>::new();
    let at = |ix: u32| {
        let i = ix as usize;
        [mesh.xs()[i], mesh.ys()[i], mesh.zs()[i]]
    };
    for tri in mesh.indices().chunks_exact(3) {
        if tri
            .iter()
            .all(|&ix| dot(sub(at(ix), p.site), p.normal).abs() < 1e-7)
        {
            for k in 0..3 {
                let a = tri[k];
                let b = tri[(k + 1) % 3];
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
    }
    if edges.is_empty() {
        return Err(Reject::DepartureNotMonotone);
    }
    let PartFields::Frustum { r1, r2, start, end } = p.contact.parts[1].fields() else {
        return Err(Reject::UnknownShape);
    };
    let displacement = sub(end.map(f64::from), start.map(f64::from));
    let along = dot(displacement, p.normal);
    let lateral = norm(std::array::from_fn(|i| {
        displacement[i] - along * p.normal[i]
    }));
    let required = f64::from(r1.max(r2)) + lateral + 2.0 * ENVELOPE;
    // The entire projected cone fits within the flat face union: no finite edge
    // or hole can become its nearest surface instead of the supporting plane.
    for ((a, b), count) in edges {
        if count > 2 {
            return Err(Reject::PatchUnverified);
        }
        if count == 2 {
            continue;
        }
        let a = at(a);
        let b = at(b);
        let d = sub(b, a);
        let len2 = dot(d, d);
        if len2 < 1e-20 {
            return Err(Reject::PatchUnverified);
        }
        let t = (dot(sub(p.site, a), d) / len2).clamp(0.0, 1.0);
        let nearest = std::array::from_fn(|i| a[i] + t * d[i]);
        if norm(sub(nearest, p.site)) < required {
            return Err(Reject::DepartureNotMonotone);
        }
    }
    Ok(())
}
fn section_capacity(p: &Prepared) -> f64 {
    p.contact.parts[1]
        .frustum_inner()
        .map_or(0.0, |radius| std::f64::consts::PI * radius * radius)
}
/// Refuse a fixed tip at its actual certified lower capacity. It must be resized in a
/// private proposal or supplied additional contacts; never grow an admitted contact.
pub fn require_capacity(quote: &Quote, peel_area: f64) -> Result<(), Reject> {
    if !peel_area.is_finite() || peel_area < 0.0 {
        return Err(Reject::NonFinite);
    }
    if quote.full_capacity() < peel_area {
        Err(Reject::InsufficientCapacity)
    } else {
        Ok(())
    }
}

pub(super) fn pair(a: &Contact, b: &Contact) -> Result<(), Reject> {
    if a.cand == b.cand {
        return Err(Reject::RegionBlocked { by: b.cand });
    }
    if distance(a.centre, b.centre) - f64::from(a.radius + b.radius) - 2.0 * ENVELOPE < 0.012 {
        return Err(Reject::WouldFuse { with: b.cand });
    }
    // Conservative bounding balls of the complete heads. This reserves MORE than the
    // ball/cone region and cannot let two true regions overlap.
    let ar = distance(a.centre, a.joint) + f64::from(a.joint_radius);
    let br = distance(b.centre, b.joint) + f64::from(b.joint_radius);
    if distance(a.centre, b.centre) < ar + br + 2.0 * ENVELOPE {
        return Err(Reject::RegionBlocked { by: b.cand });
    }
    Ok(())
}
