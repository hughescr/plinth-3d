use super::native::{ENVELOPE, cross, distance, dot, norm, sub};
use super::*;
use plinth_geom::{
    Bvh, ClosestHits, Points,
    clearance::{self, Solid},
    closest_points,
    measure::Measure,
    patch::{self, Ball},
};

// [S] Upper edge of the §5.4 tip family (largest proposed tip about 0.39–0.42 mm),
// NOT the checker's wider validation domain. Compare native f32 fields directly.
const MAX_TIP_BALL_R: f32 = 0.42;

#[cfg(test)]
#[path = "probe_batch_tests.rs"]
mod probe_batch_tests;

type PatchArea = Measure<patch::AreaBounds>;
pub(super) type PatchCache =
    std::sync::Mutex<std::collections::BTreeMap<[u64; 4], (PatchArea, usize)>>;
struct PatchBatch {
    areas: Vec<PatchArea>,
    components: Vec<usize>,
}

fn cached_patches(plan: &PlanBuilder<Contacts>, bvh: &Bvh<'_>, balls: &[Ball]) -> PatchBatch {
    // Fan/length alternatives share canonical enclosing/contained measurement
    // spheres. The immutable model and the full bit key bind every cache entry.
    let key = |b: &Ball| {
        [
            b.centre[0].to_bits(),
            b.centre[1].to_bits(),
            b.centre[2].to_bits(),
            b.radius.to_bits(),
        ]
    };
    let mut cache = plan.patch_cache.lock().expect("patch cache poisoned");
    let missing: std::collections::BTreeMap<_, _> = balls
        .iter()
        .filter(|b| !cache.contains_key(&key(b)))
        .map(|b| (key(b), *b))
        .collect();
    let queries: Vec<_> = missing.values().copied().collect();
    let mut components = Vec::new();
    let mut areas = Vec::new();
    patch::ball_patch_components(bvh, &queries, &mut components);
    patch::ball_patches_with_workers(bvh, &queries, &mut areas, plan.workers);
    for ((k, area), count) in missing.keys().zip(areas).zip(components) {
        cache.insert(*k, (area, count));
    }
    PatchBatch {
        areas: balls.iter().map(|b| cache[&key(b)].0.clone()).collect(),
        components: balls.iter().map(|b| cache[&key(b)].1).collect(),
    }
}

struct Prepared {
    contact: Contact,
    outer: Ball,
    inner: Ball,
    cone: Solid,
    joint: Solid,
}

pub(super) fn probe(
    plan: &PlanBuilder<Contacts>,
    cands: &[ContactCandidate],
    out: &mut Vec<Result<Quote, Reject>>,
    alternatives: bool,
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
    let solids: Vec<_> = valid.iter().flat_map(|p| [p.joint, p.cone]).collect();
    let mut gaps = Vec::new();
    clearance::solid_gaps_with_workers(&bvh, &solids, &mut gaps, plan.workers);
    let cones: Vec<_> = valid.iter().map(|p| p.cone).collect();
    let mut departures = Vec::new();
    clearance::capped_departures(&bvh, &cones, &mut departures);
    // Profiled hot path: patch subdivision dominated proposals whose finite
    // heads already fail clearance/departure. Reject those first, then measure
    // only the survivors in ONE patch batch. This changes no admitted geometry
    // or search domain, and leaves every unresolved measurement closed.
    let geometry = geometry_rejections(&gaps, &departures, plan.limits.gap, alternatives);
    let balls: Vec<_> = valid
        .iter()
        .zip(&geometry)
        .filter(|(_, result)| result.is_ok())
        .flat_map(|(p, _)| [p.outer, p.inner])
        .collect();
    let PatchBatch { areas, components } = cached_patches(plan, &bvh, &balls);
    let mut at = 0;
    let mut patch_at = 0;
    for result in prepared {
        match result {
            Err(e) => out.push(Err(e)),
            Ok(mut p) => {
                if let Err(e) = &geometry[at / 2] {
                    out.push(Err(e.clone()));
                    at += 2;
                    continue;
                }
                let measured =
                    if areas[patch_at].is_unverified() || areas[patch_at + 1].is_unverified() {
                        Err(Reject::MeasureExhausted)
                    } else if components[patch_at] != 1 {
                        Err(Reject::SecondSheet)
                    } else {
                        check_measurements(
                            plan,
                            &p,
                            &areas[patch_at],
                            &areas[patch_at + 1],
                            &gaps[at],
                            &gaps[at + 1],
                        )
                    };
                let measured = measured.and_then(|patch| match departures[at / 2].known() {
                    Some(true) => Ok(patch),
                    Some(false) => Err(Reject::DepartureNotMonotone),
                    None => Err(Reject::MeasureExhausted),
                });
                at += 2;
                patch_at += 2;
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
        for (j, other) in out[..i].iter().enumerate().filter(|_| !alternatives) {
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

fn geometry_rejections(
    gaps: &[Measure<clearance::GapBounds>],
    departures: &[Measure<bool>],
    gap: f64,
    alternatives: bool,
) -> Vec<Result<(), Reject>> {
    departures
        .iter()
        .enumerate()
        .map(|(i, departure)| {
            // Public whole-layout probes preserve diagnostic precedence. Private
            // alternatives need a rejection certificate, not every possible reason.
            if !alternatives {
                return Ok(());
            }
            let joint = gaps[2 * i].known().ok_or(Reject::MeasureExhausted)?;
            if joint.lo < gap {
                return Err(Reject::GapTooSmall { gap: joint.lo });
            }
            let cone = gaps[2 * i + 1].known().ok_or(Reject::MeasureExhausted)?;
            if cone.lo - ENVELOPE <= 0.02 {
                return Err(Reject::HeadTouchesModel);
            }
            match departure.known() {
                Some(true) => Ok(()),
                Some(false) => Err(Reject::DepartureNotMonotone),
                None => Err(Reject::MeasureExhausted),
            }
        })
        .collect()
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
        || c.ball_r > MAX_TIP_BALL_R
        || c.standoff < 0.0
        || c.standoff >= c.ball_r
        || c.joint_r < 0.26
        || c.head_len < 1.5
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
    // Measure from the CONTACT SITE, not the displaced ball centre. These are
    // enclosures of the rounded fields AND the native mesh, including ENVELOPE.
    // §5.4's 2.95 mm complete-head maximum is stricter than the 3.0 mm region:
    // fail closed on a longer ball/cone rather than grant a >0.02 mm exemption to
    // any tail outside the region. The joint still needs its full 0.32 mm gap.
    let site = c.site.map(f64::from);
    if [Solid::Ball(outer), cone_solid, joint_solid]
        .into_iter()
        .any(|solid| {
            let reach = reach_from_site(solid, site);
            reach > 2.95 || reach > plan.limits.region
        })
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
    // Fan between outward normal and vertical, at most 35 degrees from normal.
    // Rounded geometry, not this directional guard, establishes clearance.
    let vertical = [0.0, 0.0, -1.0];
    if !plan.model.orientation_valid
        || dot(axis, n) < 35.0_f64.to_radians().cos() - 1e-6
        || axis[2] > n[2] + 1e-6
        || axis[0] * n[0] + axis[1] * n[1] < -1e-6
        || dot(axis, cross(n, vertical)).abs() > 1e-6
    {
        return Err(Reject::DepartureNotMonotone);
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
        // Canonicalise only the measurement spheres, never native geometry.
        // f32 pole averaging at |coordinate| <= 101 moves the sphere centre by
        // < 8e-6 mm. Two ENVELOPE radii enclose the original outer sphere; the
        // inner radius loses the exact centre displacement and rounds DOWN to
        // 1e-5 mm. These enclosures let fan/length variants share a patch query.
        outer: Ball {
            centre: centre.map(f64::from),
            radius: f64::from(c.ball_r) + 2.0 * ENVELOPE,
        },
        inner: Ball {
            centre: centre.map(f64::from),
            radius: ((inner.radius - norm(sub(inner.centre, centre.map(f64::from)))) * 100000.0)
                .floor()
                / 100000.0,
        },
        cone: cone_solid,
        joint: joint_solid,
    })
}

fn reach_from_site(solid: Solid, site: [f64; 3]) -> f64 {
    // A finite frustum is the convex hull of its two end discs. Enclosing both
    // discs in this site-centred ball encloses their entire hull. This conservative
    // bound needs no angular sampling; 1e-8 mm encloses arithmetic at <=100 mm.
    let reach = match solid {
        Solid::Ball(ball) => norm(sub(ball.centre, site)) + ball.radius,
        Solid::Frustum {
            start,
            end,
            r_start,
            r_end,
        } => (norm(sub(start, site)) + r_start).max(norm(sub(end, site)) + r_end),
    };
    reach + 1e-8
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
    Ok(patch)
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
    // Sum radii in f64, not a rounded-down f32 sum. The patch is contained in
    // its inflated ball; this is a lower bound on patch-envelope separation.
    if distance(a.centre, b.centre)
        - f64::from(a.radius)
        - f64::from(b.radius)
        - 2.0 * ENVELOPE
        - 1e-7
        < 0.012
    {
        return Err(Reject::WouldFuse { with: b.cand });
    }
    for ap in &a.parts {
        for bp in &b.parts {
            let gap = clearance::solid_pair_lower(
                ap.solid().ok_or(Reject::UnknownShape)?,
                bp.solid().ok_or(Reject::UnknownShape)?,
            );
            // Regions are the ball/cone solids, not enclosing spheres. Different
            // chains must not fuse at the 0.010 mm resolution: keep the same
            // 0.012 mm construction reserve as patches, not a numerical epsilon.
            // A joint is always outside the region and retains the full build gap.
            let required = if ap.kind() == PartKind::JointBall || bp.kind() == PartKind::JointBall {
                0.32
            } else {
                0.012
            };
            if gap < required {
                return Err(Reject::RegionBlocked { by: b.cand });
            }
        }
    }
    Ok(())
}
