//! CAL-2 coupon construction. Heads are plinth's own: `head_candidate` supplies the ball,
//! standoff, cone and joint, `PlanBuilder` admits them, and the frozen plan is written by
//! `plinth_emit`. The one input chosen here is the cone-start radius, solved so the section
//! where the cone leaves the ball (as `neck_area` in the checker measures it) is the
//! designed diameter. Only the 1.2 mm section is a plain waisted column. The body is sized
//! to the capacity plinth's load model takes per contact (the certified patch lower bound or
//! a smaller section of the path), read from the load view of a trial build of one head.
use crate::cal2_contact::{Part, parts_touch, touches_body};
use crate::cal2_measure::HeadLoad;
use crate::cal2_spec::{Cal2Spec, Item, Kind};
use crate::{cal2_body, cal2_measure, cal2_shape};
use plinth_chitubox::{Model, Preset, RecordType, SupportRecord, Supports};
use plinth_emit::shapes::{
    self, FrustumRole, LIGHT_PRESET_ID, RAFT_ATTR, Spec, check_support_presets, support_attr,
};
use plinth_support::{
    construct::{
        Contact, KeepUpRecord, Limits, MemberProposal, MemberQuoteBatch, PlanBuilder, PosedModel,
        ProcessRecord, QuoteBatch, Section, SupportPlan,
    },
    load::LayerStack,
    place::{add_members, head_candidate, layout_from_contacts},
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
    /// Body area at full load, mm^2: ratio x heads x per-head load-model capacity, less the
    /// guard.
    pub full_area: f64,
    /// The per-head sections the body was sized to, from a trial build of one head, mm^2.
    pub sections: HeadSections,
    /// Plinth's certified patch interval per head (none for plain columns), mm^2.
    pub patch: Vec<[f64; 2]>,
    pub head: Option<HeadDesign>,
    /// The load view of the frozen plan (none for plain columns).
    pub load: Option<HeadLoad>,
    /// The raised number (and tab) on the top face; none where the face is too small.
    pub features: Option<cal2_body::Features>,
}

/// The sections of one head chain, mm^2. `minimum` is what plinth's load model divides by in
/// the first loaded layer: the smallest of the certified patch printed by then (a little
/// under its lower bound) and every section of the path, taken from the load view. The patch
/// only grows, so no later layer has less. `mesh_patch`, `cone_exit` and `mesh_minimum` are
/// measured from the written record meshes, for comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadSections {
    pub cone_exit: f64,
    /// Ball section in the plane of the body face, sliced from the written ball.
    pub mesh_patch: f64,
    /// plinth's certified patch lower bound (the mesh patch for a plain column).
    pub certified_patch: f64,
    /// Smallest section on the path as sliced from the written records.
    pub mesh_minimum: f64,
    /// The load model's capacity per contact.
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
    let (model, load) = match head {
        Some(h) => {
            let b = plinth_heads(spec, &tris, &sites, h, true)?;
            (b.model, b.load)
        }
        None => (plain_columns(spec, item.neck_mm, &tris, &sites)?.0, None),
    };
    let c = cal2_measure::first_chain(&model, item.kind)?;
    // The load model's capacity of the one head in the first loaded layer: the smallest
    // section of its chain, the patch as printed by then (a little under its certified
    // lower bound for the first layers).
    let (certified_patch, minimum) = match &load {
        Some(l) => (
            l.layout.contacts[0].patch_lower_mm2,
            l.first_layer_capacity(0),
        ),
        None => (c.patch, c.minimum()),
    };
    Ok(HeadSections {
        cone_exit: c.neck,
        mesh_patch: c.patch,
        certified_patch,
        mesh_minimum: c.minimum(),
        minimum,
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

/// The model, its head sites, plinth's certified patch interval per head, and the load view.
struct HeadsBuilt {
    model: Model,
    sites: Vec<[f32; 2]>,
    patch: Vec<[f64; 2]>,
    load: Option<HeadLoad>,
}

/// The load view of a frozen plan, made as the planner and the report make it
/// (`layout_from_contacts` and `add_members`), with the planner's certified patch history:
/// the patch printed by each layer's top, `ball_patches_below` capped at the contact's lower
/// bound. The body's first layer starts at its lowest point (the face the heads touch).
fn head_load(plan: &SupportPlan) -> crate::Result<HeadLoad> {
    let shift = -f64::from(plan.model().plate_z());
    let contacts: Vec<&Contact> = plan.contacts().iter().collect();
    let stack = LayerStack {
        layer_height_mm: LAYER,
        ..LayerStack::default()
    };
    let mut layout = layout_from_contacts(&contacts, &[], &stack, shift);
    add_members(plan, &mut layout);
    let mesh = plan.model().mesh();
    let face = mesh.bounds().min[2];
    let (mut queries, mut owners) = (Vec::new(), Vec::new());
    for (i, c) in contacts.iter().enumerate() {
        let ball = c
            .inner_ball()
            .map_err(|e| format!("contact {i}: no certified ball: {e:?}"))?;
        // Layers up to the one holding the ball's top; the patch is whole by then, and the
        // last entry holds for every layer above.
        let layers = ((ball.centre[2] + ball.radius - face) / LAYER)
            .ceil()
            .max(1.0) as usize;
        for j in 1..=layers {
            queries.push((ball, face + j as f64 * LAYER));
            owners.push(i);
        }
    }
    let bvh = plinth_geom::Bvh::build(mesh).map_err(|e| format!("body bvh: {e:?}"))?;
    let mut bounds = Vec::new();
    plinth_geom::patch::ball_patches_below_with_workers(&bvh, &queries, &mut bounds, 1);
    let mut history = vec![Vec::new(); contacts.len()];
    for (((_, z), i), area) in queries.iter().zip(owners).zip(bounds) {
        let lower = area
            .known()
            .map_or(0.0, |a| a.lo)
            .min(contacts[i].patch().lower());
        history[i].push((z + shift, lower));
    }
    Ok(HeadLoad { layout, history })
}

fn plinth_heads(
    spec: &Cal2Spec,
    tris: &[cal2_body::Tri],
    sites: &[[f64; 2]],
    design: HeadDesign,
    with_load: bool,
) -> crate::Result<HeadsBuilt> {
    let (body, limits) = posed(spec, tris)?;
    let sites_f32: Vec<[f32; 2]> = sites.iter().map(|s| s.map(|v| v as f32)).collect();
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
    let load = with_load.then(|| head_load(&plan)).transpose()?;
    let model = plinth_emit::constructed::project(&plan)
        .map_err(|e| format!("emit: {e:?}"))?
        .models
        .remove(0);
    Ok(HeadsBuilt {
        model,
        sites: sites_f32,
        patch,
        load,
    })
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
        .map(|i| {
            if i < supports {
                support_attr()
            } else {
                RAFT_ATTR
            }
        })
        .collect();
    let model = Model {
        name: String::new(),
        translate: body.translate(),
        mesh: body.triangles().to_vec(),
        supports: Some(Supports {
            presets: vec![Preset {
                id: u64::from(LIGHT_PRESET_ID),
                name: "Light".into(),
            }],
            attrs,
            unreferenced_blob: Vec::new(),
            records,
        }),
    };
    Ok((model, sites))
}

/// Lowest and highest z of a record's stored mesh.
fn z_span(r: &plinth_chitubox::SupportRecord) -> (f64, f64) {
    r.mesh
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(f64::from(v[2])), hi.max(f64::from(v[2])))
        })
}

fn column_key(p: [f32; 3]) -> [i64; 2] {
    [p[0], p[1]].map(|v| (f64::from(v) * 1000.0).round() as i64)
}

/// What ChituBox needs of a written model before its supports draw and reach the plate,
/// checked on the written records (never on the plan): every non-raft attr names a preset
/// of the table and not preset 0 (ChituBox draws nothing for id 0, see [`LIGHT_PRESET_ID`]),
/// and in every support column (one XY) the record meshes form one continuous path from the
/// body's lowest face down to a raft pad whose underside is the plate, at local
/// `z = body_min_z - translate_z` (format spec section 6.2). Returns the number of columns.
///
/// # Errors
/// The first column or attr that fails.
pub fn check_supports_reach_plate(m: &Model) -> Result<usize, String> {
    const TOUCH: f64 = 0.01;
    let s = m.supports.as_ref().ok_or("the model has no supports")?;
    check_support_presets(s)?;
    let body_min = m
        .mesh
        .iter()
        .flatten()
        .map(|v| f64::from(v[2]))
        .fold(f64::INFINITY, f64::min);
    let plate = body_min - f64::from(m.translate[2]);
    let mut by_column: std::collections::BTreeMap<[i64; 2], Vec<&SupportRecord>> =
        Default::default();
    for r in s
        .records
        .iter()
        .filter(|r| r.record_type != RecordType::RAFT)
    {
        by_column.entry(column_key(r.start)).or_default().push(r);
    }
    for (key, recs) in &by_column {
        let at = format!("support column at {key:?} (1/1000 mm)");
        let mut spans: Vec<(f64, f64)> = recs.iter().map(|r| z_span(r)).collect();
        spans.sort_by(|a, b| b.1.total_cmp(&a.1));
        if spans[0].1 < body_min - 1e-4 {
            return Err(format!(
                "{at}: its top {:.3} is below the body face {body_min:.3}",
                spans[0].1
            ));
        }
        let mut low = spans[0].0;
        for &(lo, hi) in &spans[1..] {
            if hi < low - TOUCH {
                return Err(format!("{at}: gap between {hi:.3} and {low:.3}"));
            }
            low = low.min(lo);
        }
    }
    if by_column.is_empty() {
        return Err("no support column".into());
    }
    check_contacts(m, s, &by_column, plate)?;
    Ok(by_column.len())
}

/// The XY-and-Z half of the support-path check, on the stored meshes (see
/// [`crate::cal2_contact`]): per column, the records form one touching component; its top
/// record touches or overlaps the body mesh; its lowest record touches a raft part whose
/// underside is the plate. A column moved sideways (alone or with the whole support set)
/// off the body or off its pad fails here though its Z intervals and metadata still agree.
fn check_contacts(
    m: &Model,
    s: &Supports,
    by_column: &std::collections::BTreeMap<[i64; 2], Vec<&SupportRecord>>,
    plate: f64,
) -> Result<(), String> {
    let body = Part::new(&m.mesh)?;
    let body_bvh = body.bvh()?;
    let rafts = s
        .records
        .iter()
        .filter(|r| r.record_type == RecordType::RAFT)
        .map(|r| Ok((Part::new(&r.mesh)?, z_span(r).0)))
        .collect::<Result<Vec<_>, String>>()?;
    let raft_bvhs = rafts
        .iter()
        .map(|(p, _)| p.bvh())
        .collect::<Result<Vec<_>, _>>()?;
    for (key, recs) in by_column {
        let at = format!("support column at {key:?} (1/1000 mm)");
        let parts = recs
            .iter()
            .map(|r| Part::new(&r.mesh))
            .collect::<Result<Vec<_>, _>>()?;
        let bvhs = parts.iter().map(Part::bvh).collect::<Result<Vec<_>, _>>()?;
        let spans: Vec<(f64, f64)> = recs.iter().map(|r| z_span(r)).collect();
        let top = (0..parts.len())
            .max_by(|&a, &b| spans[a].1.total_cmp(&spans[b].1))
            .unwrap_or(0);
        // One touching component from the top record: no part floats off sideways.
        let mut reached = vec![false; parts.len()];
        reached[top] = true;
        let mut queue = vec![top];
        while let Some(i) = queue.pop() {
            for j in 0..parts.len() {
                if !reached[j] && parts_touch(&parts[i], &bvhs[i], &parts[j], &bvhs[j])? {
                    reached[j] = true;
                    queue.push(j);
                }
            }
        }
        let cut = reached.iter().filter(|r| !**r).count();
        if cut > 0 {
            return Err(format!(
                "{at}: {cut} of {} parts do not touch the path from the top (sideways break)",
                parts.len()
            ));
        }
        if !touches_body(&parts[top], &body_bvh)? {
            return Err(format!(
                "{at}: its top part touches the body mesh nowhere (supports off the body)"
            ));
        }
        let low = spans.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
        let mut under = Vec::new();
        for (i, (part, bvh)) in parts.iter().zip(&bvhs).enumerate() {
            if spans[i].0 > low + 1e-3 {
                continue;
            }
            for ((raft, raft_lo), raft_bvh) in rafts.iter().zip(&raft_bvhs) {
                if parts_touch(part, bvh, raft, raft_bvh)? {
                    under.push(*raft_lo);
                }
            }
        }
        if under.is_empty() {
            return Err(format!(
                "{at}: no raft pad under it: the lowest support {low:.3} touches none"
            ));
        }
        if !under.iter().any(|lo| (lo - plate).abs() <= 1e-3) {
            return Err(format!(
                "{at}: lowest support {low:.3}, the raft it touches starts at {:.3}, plate {plate:.3}",
                under[0]
            ));
        }
    }
    Ok(())
}

/// Build one coupon. The name is set here; the placement translation is added later.
///
/// # Errors
/// plinth refusing a head, or any geometry the writer cannot take.
pub fn build(spec: &Cal2Spec, item: &Item, number: u32) -> crate::Result<Built> {
    build_with(spec, item, number, true)
}

/// The same coupon without the certified patch history of its heads (`load` is none for
/// plinth-headed coupons), which is the slow part of a build. For layout and footprints
/// only: such a coupon cannot be measured.
///
/// # Errors
/// As [`build`].
pub fn build_for_layout(spec: &Cal2Spec, item: &Item, number: u32) -> crate::Result<Built> {
    build_with(spec, item, number, false)
}

fn build_with(spec: &Cal2Spec, item: &Item, number: u32, with_load: bool) -> crate::Result<Built> {
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
    // Size the body to the capacity plinth's load model takes per contact: the certified
    // patch lower bound, or any smaller section on its path.
    let sections = head_sections(spec, item, head)?;
    let full_area = item.ratio * item.heads as f64 * sections.minimum * (1.0 - GUARD);
    let plan = cal2_shape::plan(item, spec.hold_layers, full_area)?;
    // The number is raised on the top face, above every loaded layer; a face too small for
    // the smallest legible number (the formation witnesses) carries none.
    let features = plan
        .top
        .and_then(|face| cal2_body::Features::on(face, number));
    let tris = cal2_body::body(&plan.parts, features.as_ref());
    let (model, sites, patch, load) = match head {
        Some(h) => {
            let b = plinth_heads(spec, &tris, &plan.sites, h, with_load)?;
            (b.model, b.sites, b.patch, b.load)
        }
        None => {
            let (m, s) = plain_columns(spec, item.neck_mm, &tris, &plan.sites)?;
            (m, s, vec![], None)
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
        load,
        features,
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
            let b = build_for_layout(&s, it, 3).unwrap();
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

    /// Every coupon of the matrix carries a raised number on its top face, above every loaded
    /// layer, except the formation witnesses, whose 0.82 to 1.62 mm faces are smaller than
    /// the smallest legible number. Rings carry it on their top surface.
    #[test]
    fn every_coupon_but_the_witnesses_carries_a_legible_number_above_the_loaded_layers() {
        let s = spec();
        for it in s.items().iter().filter(|i| i.copy == 0) {
            let b = build_for_layout(&s, it, 47).unwrap();
            let name = format!("{:?} {} R{}", it.variant, it.neck_mm, it.ratio);
            match (&b.features, it.kind) {
                (None, Kind::Witness) => {}
                (Some(f), k) if k != Kind::Witness => {
                    assert!(f.pixel >= 0.3 && f.number == 47, "{name}");
                    // The number's boxes start at the body's top, which is the highest of the
                    // body's own faces: nothing of the body is above them but the number.
                    let raised = cal2_body::raised_boxes(b.model.mesh.as_slice());
                    assert_eq!(raised, f.boxes(), "{name}");
                    let z = |init: f32, pick: fn(f32, f32) -> f32| {
                        b.model.mesh.iter().flatten().map(|p| p[2]).fold(init, pick)
                    };
                    let height = z(f32::MIN, f32::max) - z(f32::MAX, f32::min);
                    assert!(
                        (f64::from(height) - (f.z + cal2_body::RAISE)).abs() < 1e-4,
                        "{name}"
                    );
                    // Rings carry no orientation tab: the number sits on the ring itself.
                    assert!(f.tab.is_none() || b.dims["shape"] != "ring", "{name}");
                }
                other => panic!("{name}: {other:?}"),
            }
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
        assert!(sec.minimum <= sec.certified_patch && sec.minimum < sec.cone_exit);
        // plinth's certified lower bound is under the written ball's plane section.
        assert!(sec.certified_patch < sec.mesh_patch);
        let ratio = sec.mesh_patch / sec.cone_exit;
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

    /// The model as the file holds it: written and parsed back.
    fn written(m: &Model) -> Model {
        let p = plinth_chitubox::Project {
            models: vec![m.clone()],
            ..plinth_chitubox::Project::default()
        };
        plinth_chitubox::parse(&p.to_bytes().unwrap())
            .unwrap()
            .models
            .remove(0)
    }

    /// The CAL-2 defect (owner, in ChituBox, 2026-11-10): every coupon floated 5 mm above the
    /// plate with no supports drawn, because the records carried preset id 0. Every coupon
    /// of the matrix, 1.20 mm columns included, must hold in its written records one
    /// continuous support path per head from the body face down to a pad on the plate, with
    /// a preset ChituBox draws (the same preset as CAL-1's coupons).
    #[test]
    fn every_coupon_has_a_continuous_drawn_support_path_from_each_head_to_the_plate() {
        let s = spec();
        for it in s.items().iter().filter(|i| i.copy == 0) {
            let b = build_for_layout(&s, it, 3).unwrap();
            let m = written(&b.model);
            let name = format!("{:?} {} R{}", it.variant, it.neck_mm, it.ratio);
            assert_eq!(check_supports_reach_plate(&m), Ok(it.heads), "{name}");
            let sup = m.supports.as_ref().unwrap();
            assert_eq!(sup.presets.len(), 1, "{name}");
            assert_eq!(sup.presets[0].id, u64::from(LIGHT_PRESET_ID), "{name}");
            for (a, r) in sup.attrs.iter().zip(&sup.records) {
                let want = if r.record_type == RecordType::RAFT {
                    RAFT_ATTR
                } else {
                    support_attr()
                };
                assert_eq!(*a, want, "{name}");
            }
            // The lowest support point is the plate: local body minimum less the lift.
            let body_min = m
                .mesh
                .iter()
                .flatten()
                .map(|v| v[2])
                .fold(f32::MAX, f32::min);
            let low = sup
                .records
                .iter()
                .map(|r| z_span(r).0)
                .fold(f64::INFINITY, f64::min);
            assert!(
                (low - (f64::from(body_min) - s.lift_mm)).abs() < 1e-3,
                "{name}: lowest support {low}"
            );
        }
    }

    /// Sabotage: the check sees each way the first CAL-2 file failed or could fail.
    #[test]
    fn the_support_path_check_refuses_preset_zero_a_missing_part_and_a_floating_column() {
        let s = spec();
        let it = s
            .items()
            .into_iter()
            .find(|i| i.copy == 0 && i.kind == Kind::Head)
            .unwrap();
        let m = written(&build_for_layout(&s, &it, 3).unwrap().model);
        assert!(check_supports_reach_plate(&m).is_ok());
        // The CAL-2 file as first written: preset id 0, attr kind 4.
        let mut bad = m.clone();
        let sup = bad.supports.as_mut().unwrap();
        sup.presets[0].id = 0;
        for (a, r) in sup.attrs.iter_mut().zip(&sup.records) {
            if r.record_type != RecordType::RAFT {
                *a = 0x0400;
            }
        }
        assert!(
            check_supports_reach_plate(&bad)
                .unwrap_err()
                .contains("preset 0")
        );
        // Attr naming a preset that is not in the table.
        let mut bad = m.clone();
        bad.supports.as_mut().unwrap().presets[0].id = 2;
        assert!(check_supports_reach_plate(&bad).is_err());
        // No column rods and flares (heads only, nothing descends).
        let mut bad = m.clone();
        let sup = bad.supports.as_mut().unwrap();
        let keep: Vec<bool> = sup
            .records
            .iter()
            .map(|r| r.record_type != RecordType::ROD && r.record_type != RecordType::BASE_FLARE)
            .collect();
        let mut k = keep.iter();
        sup.records.retain(|_| *k.next().unwrap());
        let mut k = keep.iter();
        sup.attrs.retain(|_| *k.next().unwrap());
        let e = check_supports_reach_plate(&bad).unwrap_err();
        assert!(e.contains("gap") || e.contains("lowest support"), "{e}");
        // Whole model lifted off its pads: the plate is 1 mm below the pad undersides.
        let mut bad = m.clone();
        bad.translate[2] += 1.0;
        assert!(check_supports_reach_plate(&bad).is_err());
        // Pads removed.
        let mut bad = m;
        let sup = bad.supports.as_mut().unwrap();
        let keep: Vec<bool> = sup
            .records
            .iter()
            .map(|r| r.record_type != RecordType::RAFT)
            .collect();
        let mut k = keep.iter();
        sup.records.retain(|_| *k.next().unwrap());
        let mut k = keep.iter();
        sup.attrs.retain(|_| *k.next().unwrap());
        assert!(
            check_supports_reach_plate(&bad)
                .unwrap_err()
                .contains("no raft pad")
        );
    }

    /// Move one record by `d`: its mesh always, its start and end as well when `meta`.
    fn shift(r: &mut SupportRecord, d: [f32; 3], meta: bool) {
        let add = |v: &mut [f32; 3]| {
            for (c, dc) in v.iter_mut().zip(d) {
                *c += dc;
            }
        };
        r.mesh.iter_mut().flatten().for_each(add);
        if meta {
            add(&mut r.start);
            add(&mut r.end);
        }
    }

    /// The written model of the first coupon of `kind` with at least `heads` heads.
    fn coupon(kind: Kind, heads: usize) -> Model {
        let s = spec();
        let it = s
            .items()
            .into_iter()
            .find(|i| i.copy == 0 && i.kind == kind && i.heads >= heads)
            .unwrap();
        written(&build_for_layout(&s, &it, 3).unwrap().model)
    }

    /// Every record of the column that holds `records[i]`, and its raft pad record numbers.
    fn column_of(m: &Model, i: usize) -> (Vec<usize>, Vec<usize>) {
        let recs = &m.supports.as_ref().unwrap().records;
        let key = column_key(recs[i].start);
        let at = |r: &SupportRecord| column_key(r.start) == key;
        let all = |raft: bool| -> Vec<usize> {
            (0..recs.len())
                .filter(|&j| (recs[j].record_type == RecordType::RAFT) == raft && at(&recs[j]))
                .collect()
        };
        (all(false), all(true))
    }

    /// Defect found by the cross-family review of the first check: it compared Z intervals
    /// and metadata only. The reviewer's case: every support record (rafts too) moved 10 mm
    /// in X off a body that is under 1 mm wide, written and parsed, was accepted.
    #[test]
    fn the_support_path_check_refuses_a_whole_support_set_moved_sideways_off_the_body() {
        let m = coupon(Kind::Witness, 1);
        assert_eq!(check_supports_reach_plate(&m), Ok(1));
        for meta in [true, false] {
            let mut bad = m.clone();
            for r in &mut bad.supports.as_mut().unwrap().records {
                shift(r, [10.0, 0.0, 0.0], meta);
            }
            let e = check_supports_reach_plate(&written(&bad)).unwrap_err();
            assert!(e.contains("body mesh nowhere"), "meta {meta}: {e}");
        }
    }

    /// One column (with its pad, so it is whole in itself) moved sideways off its body,
    /// leaving the other columns on theirs: for heads and for plain columns, with the
    /// metadata moved or left behind.
    #[test]
    fn the_support_path_check_refuses_one_column_moved_sideways_off_its_body() {
        for kind in [Kind::Head, Kind::Column] {
            let m = coupon(kind, 2);
            let n = check_supports_reach_plate(&m).unwrap();
            assert!(n >= 2, "{kind:?}");
            let first = (0..m.supports.as_ref().unwrap().records.len())
                .find(|&i| m.supports.as_ref().unwrap().records[i].record_type != RecordType::RAFT)
                .unwrap();
            let (column, pad) = column_of(&m, first);
            for meta in [true, false] {
                let mut bad = m.clone();
                for &i in column.iter().chain(&pad) {
                    shift(
                        &mut bad.supports.as_mut().unwrap().records[i],
                        [30.0, 0.0, 0.0],
                        meta,
                    );
                }
                let e = check_supports_reach_plate(&written(&bad)).unwrap_err();
                assert!(e.contains("body mesh nowhere"), "{kind:?} meta {meta}: {e}");
            }
        }
    }

    /// A column whose pad is somewhere else, and one part of a column off the rest of it.
    #[test]
    fn the_support_path_check_refuses_a_column_off_its_raft_pad_and_a_part_off_its_column() {
        let m = coupon(Kind::Witness, 1);
        let first = m
            .supports
            .as_ref()
            .unwrap()
            .records
            .iter()
            .position(|r| r.record_type != RecordType::RAFT)
            .unwrap();
        let (column, pad) = column_of(&m, first);
        assert!(!pad.is_empty());
        // The pad moved clear of the column (10 mm; the pad and flare are 2.2 mm across).
        for meta in [true, false] {
            let mut bad = m.clone();
            for &i in &pad {
                shift(
                    &mut bad.supports.as_mut().unwrap().records[i],
                    [10.0, 0.0, 0.0],
                    meta,
                );
            }
            let e = check_supports_reach_plate(&written(&bad)).unwrap_err();
            assert!(e.contains("no raft pad"), "meta {meta}: {e}");
        }
        // A pad shifted 0.5 mm still carries the flare: overlap is contact.
        let mut ok = m.clone();
        for &i in &pad {
            shift(
                &mut ok.supports.as_mut().unwrap().records[i],
                [0.5, 0.0, 0.0],
                true,
            );
        }
        assert_eq!(check_supports_reach_plate(&written(&ok)), Ok(1));
        // The base flare alone moved 3 mm sideways: the path from the head breaks.
        let flare = *column
            .iter()
            .find(|&&i| {
                m.supports.as_ref().unwrap().records[i].record_type == RecordType::BASE_FLARE
            })
            .unwrap();
        // Mesh only (the record still says it is in the column): the touch test sees it.
        let mut bad = m.clone();
        shift(
            &mut bad.supports.as_mut().unwrap().records[flare],
            [3.0, 0.0, 0.0],
            false,
        );
        let e = check_supports_reach_plate(&written(&bad)).unwrap_err();
        assert!(e.contains("sideways break"), "{e}");
        // With its metadata too the flare is a column of its own, which fails on its top.
        let mut bad = m.clone();
        shift(
            &mut bad.supports.as_mut().unwrap().records[flare],
            [3.0, 0.0, 0.0],
            true,
        );
        assert!(check_supports_reach_plate(&written(&bad)).is_err());
    }

    /// Supports and pads together lifted 0.5 mm: they still reach the body and each other,
    /// but the pad underside is no longer the plate.
    #[test]
    fn the_support_path_check_refuses_pads_off_the_plate() {
        let mut bad = coupon(Kind::Witness, 1);
        for r in &mut bad.supports.as_mut().unwrap().records {
            shift(r, [0.0, 0.0, 0.5], true);
        }
        let e = check_supports_reach_plate(&written(&bad)).unwrap_err();
        assert!(e.contains("plate"), "{e}");
    }
}
