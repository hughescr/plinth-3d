//! CAL-2 coupon construction. Heads are plinth's own: `head_candidate` supplies the ball,
//! standoff, cone and joint, `PlanBuilder` admits them, and the frozen plan is written by
//! `plinth_emit`. The one input chosen here is the cone-start radius, solved so the section
//! where the cone leaves the ball (as `neck_area` in the checker measures it) is the
//! designed diameter. Only the 1.2 mm section is a plain waisted column. The body is sized
//! to the per-contact minimum section, measured from a trial build of one head.
use crate::cal2_spec::{Cal2Spec, Item, Kind};
use crate::{cal2_body, cal2_measure, cal2_shape};
use plinth_chitubox::{Model, Preset, Supports};
use plinth_emit::shapes::{self, FrustumRole, RAFT_ATTR, Spec};
use plinth_support::{
    construct::{
        KeepUpRecord, Limits, MemberProposal, MemberQuoteBatch, PlanBuilder, PosedModel,
        ProcessRecord, QuoteBatch, Section,
    },
    place::head_candidate,
};

/// Largest cone-start radius plinth admits (ball radius limit 0.42 mm / 1.06).
const MAX_NECK_RADIUS: f64 = 0.396;
/// Plain-column geometry, mm below the body face (design **[S]**: calibration only).
const WAIST_LOW: f64 = 1.2;
const WAIST_HIGH: f64 = 0.8;
const TAPER_LOW: f64 = 2.0;
const COLUMN_RADIUS: f32 = 1.0;
const LAYER: f64 = 0.05;

/// Area of a written 16-sided section of circumradius `r`.
#[must_use]
pub fn polygon_area(r: f64) -> f64 {
    8.0 * r * r * (std::f64::consts::PI / 8.0).sin()
}

/// First station along the cone where its surface leaves the ball: the smallest t with
/// `t^2 + r(t)^2 >= R^2` (the checker's `exit_t`).
#[must_use]
pub fn exit_t(ra: f64, rb: f64, l: f64, big_r: f64) -> f64 {
    let k = (rb - ra) / l;
    let (qa, qb, qc) = (1.0 + k * k, 2.0 * ra * k, ra * ra - big_r * big_r);
    if qc >= 0.0 {
        return 0.0;
    }
    let disc = qb * qb - 4.0 * qa * qc;
    ((-qb + disc.max(0.0).sqrt()) / (2.0 * qa)).clamp(0.0, l)
}

/// The designed head: cone-start radius and the radius at the cone's exit from the ball.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadDesign {
    pub neck_radius: f32,
    pub exit_radius: f64,
}

fn exit_radius(ra: f32) -> f64 {
    let c = head_candidate(0, [0.0; 3], [0.0, 0.0, -1.0], ra, 0.0, 0.0);
    let (ra, rb, l) = (
        f64::from(ra),
        f64::from(c.joint_r) / 1.04,
        f64::from(c.head_len),
    );
    let t = exit_t(ra, rb, l, f64::from(c.ball_r));
    ra + (rb - ra) * t / l
}

/// Solve the cone-start radius so the exit section has diameter `neck_mm`.
///
/// # Errors
/// The diameter is outside what plinth's head path can make.
pub fn design(neck_mm: f64) -> crate::Result<HeadDesign> {
    let want = neck_mm / 2.0;
    if exit_radius(0.05) > want || exit_radius(MAX_NECK_RADIUS as f32) < want {
        return Err(format!("plinth's head path cannot make a {neck_mm} mm neck").into());
    }
    let (mut lo, mut hi) = (0.05_f64, MAX_NECK_RADIUS);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if exit_radius(mid as f32) < want {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let neck_radius = (0.5 * (lo + hi)) as f32;
    Ok(HeadDesign {
        neck_radius,
        exit_radius: exit_radius(neck_radius),
    })
}

/// Everything the layout and report need from one built coupon.
#[derive(Debug)]
pub struct Built {
    pub model: Model,
    pub sites: Vec<[f32; 2]>,
    /// Designed body dimensions and profile, for the report.
    pub dims: serde_json::Value,
    /// Body area at full load, mm^2: ratio x heads x per-head minimum section, less the guard.
    pub full_area: f64,
    /// The per-head sections the body was sized to, from a trial build of one head, mm^2.
    pub sections: HeadSections,
    /// Plinth's certified patch interval per head (none for plain columns), mm^2.
    pub patch: Vec<[f64; 2]>,
    pub head: Option<HeadDesign>,
}

/// The sections of one head chain, mm^2. `minimum` is what plinth's load model takes as the
/// contact's limiting section: the smallest of the face patch, the cone exit and the path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadSections {
    pub cone_exit: f64,
    pub patch: f64,
    pub minimum: f64,
}

/// Fraction of the target area held back when sizing, so rounding and slicing noise can
/// never put a layer over the target ratio.
pub const GUARD: f64 = 0.0005;

/// A 4 mm, 2 mm tall square with one site at its centre, for measuring one head.
fn trial_body() -> (Vec<cal2_body::Tri>, Vec<[f64; 2]>) {
    let rect = cal2_body::Shape::Rect {
        lo: [-2.0; 2],
        hi: [2.0; 2],
    };
    let part = cal2_body::Part {
        shape: rect,
        z0: 0.0,
        z1: 2.0,
    };
    (cal2_body::body(&[part], None), vec![[0.0, 0.0]])
}

/// Build one head by the same path as the coupon and measure it from the built records.
fn head_sections(
    spec: &Cal2Spec,
    item: &Item,
    head: Option<HeadDesign>,
) -> crate::Result<HeadSections> {
    let (tris, sites) = trial_body();
    let model = match head {
        Some(h) => plinth_heads(spec, &tris, &sites, h)?.0,
        None => plain_columns(spec, item.neck_mm, &tris, &sites)?.0,
    };
    let c = cal2_measure::first_chain(&model, item.kind)?;
    Ok(HeadSections {
        cone_exit: c.neck,
        patch: c.patch,
        minimum: c.minimum(),
    })
}

fn posed(spec: &Cal2Spec, tris: &[cal2_body::Tri]) -> crate::Result<(PosedModel, Limits)> {
    let limits = Limits::current().map_err(|e| format!("limits: {e:?}"))?;
    let keep = KeepUpRecord {
        rotation_x_deg: 0.0,
        approved_sha256: "0".repeat(64),
    };
    let model = PosedModel::new(&cal2_body::mesh(tris)?, &keep, &limits)
        .map_err(|e| format!("posing the body: {e:?}"))?;
    if (f64::from(model.translate()[2]) - spec.lift_mm).abs() > 1e-9 {
        return Err("plinth lift differs from the spec lift".into());
    }
    Ok((model, limits))
}

/// The model, its head sites, and plinth's certified patch interval per head.
type HeadsBuilt = (Model, Vec<[f32; 2]>, Vec<[f64; 2]>);

fn plinth_heads(
    spec: &Cal2Spec,
    tris: &[cal2_body::Tri],
    sites: &[[f64; 2]],
    design: HeadDesign,
) -> crate::Result<HeadsBuilt> {
    let (body, limits) = posed(spec, tris)?;
    let face = f64::from(body.mesh().bounds().min[2] as f32);
    let cands: Vec<_> = sites
        .iter()
        .enumerate()
        .map(|(i, s)| {
            head_candidate(
                i as u32,
                [s[0], s[1], face],
                [0.0, 0.0, -1.0],
                design.neck_radius,
                0.0,
                0.0,
            )
        })
        .collect();
    let mut builder = PlanBuilder::new(
        body,
        limits,
        ProcessRecord {
            layer_height_mm: LAYER,
            layer_origin_mm: 0.0,
        },
    );
    let mut out = Vec::new();
    builder.probe(&cands, &mut out);
    if let Some((i, e)) = out
        .iter()
        .enumerate()
        .find_map(|(i, r)| Some((i, r.as_ref().err()?)))
    {
        return Err(format!("plinth refused head {i}: {e:?}").into());
    }
    let patch = out
        .iter()
        .flatten()
        .map(|q| [q.contact().patch().lower(), q.contact().patch().upper()])
        .collect();
    let ids = builder
        .commit(QuoteBatch::from_results(out).map_err(|e| format!("{e:?}"))?)
        .map_err(|e| format!("commit: {e:?}"))?;
    let mut routed = builder.into_routing();
    let columns = ids
        .iter()
        .map(|&contact| {
            let radius = routed.get(contact)?.column_radius(0.0);
            Section::new(radius).map(|section| MemberProposal::Column { contact, section })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("column section: {e:?}"))?;
    let mut out = Vec::new();
    routed.probe_members(&columns, &mut out);
    routed
        .commit_members(MemberQuoteBatch::from_results(out).map_err(|e| format!("{e:?}"))?)
        .map_err(|e| format!("columns: {e:?}"))?;
    routed
        .raft_connected()
        .map_err(|e| format!("raft: {e:?}"))?;
    let plan = routed.freeze().map_err(|e| format!("freeze: {e:?}"))?;
    let model = plinth_emit::constructed::project(&plan)
        .map_err(|e| format!("emit: {e:?}"))?
        .models
        .remove(0);
    Ok((
        model,
        sites.iter().map(|s| s.map(|v| v as f32)).collect(),
        patch,
    ))
}

fn frustum(role: FrustumRole, start: [f32; 3], end: [f32; 3], r: (f32, f32)) -> Spec {
    Spec::Frustum {
        role,
        start,
        end,
        r1: r.0,
        r2: r.1,
    }
}

/// One plain waisted column: flare, pillar, lower taper, waist cylinder, upper taper.
fn column_specs(xy: [f32; 2], face: f32, plate: f32, waist_r: f32) -> Vec<Spec> {
    let at = |z: f32| [xy[0], xy[1], z];
    let z = |below: f64| face - below as f32;
    vec![
        frustum(
            FrustumRole::BaseFlare,
            at(plate + 1.198),
            at(plate + 0.198),
            (COLUMN_RADIUS, 1.1),
        ),
        frustum(
            FrustumRole::Rod,
            at(z(TAPER_LOW)),
            at(plate + 1.196),
            (COLUMN_RADIUS, COLUMN_RADIUS),
        ),
        frustum(
            FrustumRole::Rod,
            at(z(WAIST_LOW)),
            at(z(TAPER_LOW)),
            (waist_r, COLUMN_RADIUS),
        ),
        frustum(
            FrustumRole::Rod,
            at(z(WAIST_HIGH)),
            at(z(WAIST_LOW)),
            (waist_r, waist_r),
        ),
        // The top ends 0.1 mm inside the body so the column and body are one solid.
        frustum(
            FrustumRole::Rod,
            at(face + 0.1),
            at(z(WAIST_HIGH)),
            (COLUMN_RADIUS, waist_r),
        ),
    ]
}

fn plain_columns(
    spec: &Cal2Spec,
    neck_mm: f64,
    tris: &[cal2_body::Tri],
    sites: &[[f64; 2]],
) -> crate::Result<(Model, Vec<[f32; 2]>)> {
    let (body, _) = posed(spec, tris)?;
    let face = body.mesh().bounds().min[2] as f32;
    let plate = body.plate_z();
    let sites: Vec<[f32; 2]> = sites.iter().map(|s| s.map(|v| v as f32)).collect();
    let mut specs: Vec<Spec> = sites
        .iter()
        .flat_map(|&xy| column_specs(xy, face, plate, (neck_mm / 2.0) as f32))
        .collect();
    let supports = specs.len();
    let mut bridges = Vec::new();
    plinth_support::raft::bridge_pairs(&sites, &mut bridges);
    specs.extend(sites.iter().map(|&centre| Spec::RaftPad {
        centre,
        z_top: plate + 0.2,
        thickness: 0.2,
        diameter: 2.2,
    }));
    specs.extend(bridges.iter().map(|&[a, b]| Spec::RaftBridge {
        from: sites[a as usize],
        to: sites[b as usize],
        z_top: plate + 0.2,
        thickness: 0.2,
        width: 2.2,
    }));
    let records = shapes::build_all(&specs).map_err(|(i, e)| format!("part {i}: {e:?}"))?;
    let attrs = (0..records.len())
        .map(|i| if i < supports { 0x0400 } else { RAFT_ATTR })
        .collect();
    let model = Model {
        name: String::new(),
        translate: body.translate(),
        mesh: body.triangles().to_vec(),
        supports: Some(Supports {
            presets: vec![Preset {
                id: 0,
                name: "Light".into(),
            }],
            attrs,
            unreferenced_blob: Vec::new(),
            records,
        }),
    };
    Ok((model, sites))
}

/// Build one coupon. The name is set here; the placement translation is added later.
///
/// # Errors
/// plinth refusing a head, or any geometry the writer cannot take.
pub fn build(spec: &Cal2Spec, item: &Item, number: u32) -> crate::Result<Built> {
    let name = format!(
        "C{number:02} {} n{:.2} x{} R{:.0} {}; {}",
        spec.rows[item.row].id,
        item.neck_mm,
        item.heads,
        item.ratio,
        item.variant.name(),
        crate::LABEL
    );
    let head = (item.kind != Kind::Column)
        .then(|| design(item.neck_mm))
        .transpose()?;
    // Size the body to plinth's own limiting section: the smallest section on each
    // contact's path, which for these heads is the face patch.
    let sections = head_sections(spec, item, head)?;
    let full_area = item.ratio * item.heads as f64 * sections.minimum * (1.0 - GUARD);
    let plan = cal2_shape::plan(item, spec.hold_layers, full_area)?;
    let features = (item.kind != Kind::Witness)
        .then_some(plan.top)
        .flatten()
        .filter(|&(lo, hi, _)| cal2_body::Features::fit(hi[0] - lo[0], hi[1] - lo[1]))
        .map(|(lo, hi, z)| cal2_body::Features { lo, hi, z, number });
    let tris = cal2_body::body(&plan.parts, features.as_ref());
    let (model, sites, patch) = match head {
        Some(h) => plinth_heads(spec, &tris, &plan.sites, h)?,
        None => {
            let (m, s) = plain_columns(spec, item.neck_mm, &tris, &plan.sites)?;
            (m, s, vec![])
        }
    };
    let mut model = model;
    model.name = name;
    Ok(Built {
        model,
        sites,
        dims: plan.dims,
        full_area,
        sections,
        patch,
        head,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Cal2Spec {
        serde_json::from_str(include_str!("../../../fixtures/calibration/cal2.json")).unwrap()
    }

    #[test]
    fn every_head_row_is_built_by_plinth_and_the_written_head_is_the_planners_head() {
        let s = spec();
        for it in s
            .items()
            .iter()
            .filter(|i| i.copy == 0 && i.kind != Kind::Column)
        {
            let b = build(&s, it, 3).unwrap();
            let want = head_candidate(
                0,
                [0.0; 3],
                [0.0, 0.0, -1.0],
                b.head.unwrap().neck_radius,
                0.0,
                0.0,
            );
            let recs = &b.model.supports.as_ref().unwrap().records;
            let ball = recs
                .iter()
                .find(|r| r.record_type == plinth_chitubox::RecordType::TIP_BALL)
                .unwrap();
            let cone = recs
                .iter()
                .find(|r| r.record_type == plinth_chitubox::RecordType::TIP_CONE)
                .unwrap();
            // The written fields are exactly head_candidate's, never a re-derived copy.
            assert_eq!(ball.r1.to_bits(), want.ball_r.to_bits());
            assert_eq!(cone.r1.to_bits(), want.neck.to_bits());
            assert!((cone.r2 - want.joint_r / 1.04).abs() < 1e-6);
            let len = ((cone.end[2] - cone.start[2]).powi(2)).sqrt();
            assert!((len - want.head_len).abs() < 1e-5);
            // plinth admitted every patch under its own H1 maximum.
            assert!(b.patch.iter().all(|p| p[1] <= 0.48), "{:?}", b.patch);
            // The head count is the grid, one tip ball per head.
            let balls = recs
                .iter()
                .filter(|r| r.record_type == plinth_chitubox::RecordType::TIP_BALL)
                .count();
            assert_eq!(balls, it.heads);
        }
    }

    #[test]
    fn plain_column_waist_is_the_narrowest_and_body_area_follows_the_waist() {
        let s = spec();
        let it = s
            .items()
            .into_iter()
            .find(|i| i.kind == Kind::Column)
            .unwrap();
        let b = build(&s, &it, 3).unwrap();
        // The waist is the minimum section of a column.
        assert!((b.sections.minimum / polygon_area(0.6) - 1.0).abs() < 0.01);
        let side = b.dims["width_mm"].as_f64().unwrap();
        let want = it.ratio * 4.0 * b.sections.minimum;
        assert!(side * side <= want && side * side / want > 0.99);
        assert!(b.head.is_none() && b.patch.is_empty());
    }

    #[test]
    fn heads_are_sized_to_the_face_patch_not_the_cone_exit() {
        let s = spec();
        let it = s
            .items()
            .into_iter()
            .find(|i| i.kind == Kind::Head && (i.neck_mm - 0.55).abs() < 1e-9)
            .unwrap();
        let b = build(&s, &it, 3).unwrap();
        let sec = b.sections;
        // Recorded finding: the face patch is 6 to 8 percent under the cone exit.
        assert!(sec.minimum <= sec.patch && sec.minimum < sec.cone_exit);
        let ratio = sec.patch / sec.cone_exit;
        assert!(ratio > 0.90 && ratio < 0.96, "{ratio}");
        let want = it.ratio * it.heads as f64 * sec.minimum;
        assert!(b.full_area < want && b.full_area > want * 0.999);
    }

    #[test]
    fn exit_radius_solution_hits_each_designed_diameter() {
        for d in [0.40, 0.55, 0.80] {
            let h = design(d).unwrap();
            assert!((h.exit_radius * 2.0 - d).abs() < 1e-5, "{d}: {h:?}");
            // plinth's ball limit 0.42 mm: the cone-start radius stays inside it.
            assert!(f64::from(1.06 * h.neck_radius) <= 0.42);
        }
        assert!(design(1.2).is_err(), "plinth's head cannot make 1.2 mm");
        assert!(design(0.05).is_err());
    }
}
