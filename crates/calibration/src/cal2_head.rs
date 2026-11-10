//! CAL-2 coupon construction. Heads are plinth's own: `head_candidate` supplies the ball,
//! standoff, cone and joint, `PlanBuilder` admits them, and the frozen plan is written by
//! `plinth_emit`. The one input chosen here is the cone-start radius, solved so the section
//! where the cone leaves the ball (as `neck_area` in the checker measures it) is the
//! designed diameter. Only the 1.2 mm section is a plain waisted column.
use crate::cal2_body;
use crate::cal2_spec::{Cal2Spec, Item, Kind};
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
    pub side: f64,
    /// Designed section area of one head, mm^2.
    pub design_area: f64,
    /// Plinth's certified patch interval per head (none for plain columns), mm^2.
    pub patch: Vec<[f64; 2]>,
    pub head: Option<HeadDesign>,
}

/// Body side from the target ratio, rounded to 0.01 mm.
fn side_for(item: &Item, area: f64) -> f64 {
    let heads = (item.grid * item.grid) as f64;
    ((item.ratio * heads * area).sqrt() * 100.0).round() / 100.0
}

fn grid_sites(side: f64, n: usize) -> Vec<[f64; 2]> {
    let h = side / n as f64;
    (0..n * n)
        .map(|i| {
            [
                -side / 2.0 + (i % n) as f64 * h + h / 2.0,
                -side / 2.0 + (i / n) as f64 * h + h / 2.0,
            ]
        })
        .collect()
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
    item: &Item,
    side: f64,
    number: Option<u32>,
    design: HeadDesign,
) -> crate::Result<HeadsBuilt> {
    let tris = cal2_body::body(side, spec.body_thickness_mm, number);
    let (body, limits) = posed(spec, &tris)?;
    let face = f64::from(body.mesh().bounds().min[2] as f32);
    let sites = grid_sites(side, item.grid);
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
    item: &Item,
    side: f64,
    number: Option<u32>,
) -> crate::Result<(Model, Vec<[f32; 2]>)> {
    let tris = cal2_body::body(side, spec.body_thickness_mm, number);
    let (body, _) = posed(spec, &tris)?;
    let face = body.mesh().bounds().min[2] as f32;
    let plate = body.plate_z();
    let sites: Vec<[f32; 2]> = grid_sites(side, item.grid)
        .iter()
        .map(|s| s.map(|v| v as f32))
        .collect();
    let mut specs: Vec<Spec> = sites
        .iter()
        .flat_map(|&xy| column_specs(xy, face, plate, (item.neck_mm / 2.0) as f32))
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
        "C{number:02} {} n{:.2} x{} R{:.0}; {}",
        spec.rows[item.row].id,
        item.neck_mm,
        item.grid * item.grid,
        item.ratio,
        crate::LABEL
    );
    let shown = (item.kind != Kind::Witness).then_some(number);
    let mut built = match item.kind {
        Kind::Column => {
            let area = polygon_area(item.neck_mm / 2.0);
            let side = side_for(item, area);
            let (model, sites) = plain_columns(spec, item, side, shown)?;
            Built {
                model,
                sites,
                side,
                design_area: area,
                patch: vec![],
                head: None,
            }
        }
        Kind::Head | Kind::Witness => {
            let head = design(item.neck_mm)?;
            let area = polygon_area(head.exit_radius);
            let side = side_for(item, area);
            let (model, sites, patch) = plinth_heads(spec, item, side, shown, head)?;
            Built {
                model,
                sites,
                side,
                design_area: area,
                patch,
                head: Some(head),
            }
        }
    };
    built.model.name = name;
    Ok(built)
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
            assert_eq!(balls, it.grid * it.grid);
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
        let want = it.ratio * 4.0 * polygon_area(0.6);
        assert!((b.side * b.side / want - 1.0).abs() < 0.005);
        assert!(b.head.is_none() && b.patch.is_empty());
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
