//! Layer-specific FR-21 sharing, conditioning, and critical-location evidence.
use crate::json::Json;
use plinth_support::load::{GroupLoad, SectionDetail};
use plinth_support::report_data::{LoadData, ReportData, WrittenMetrics};
use std::fmt::Write as _;

pub const SHARING_CHANGE: &str = "Sharing-model change [O]: nearest-3D tributary sizing (old) is replaced by rigid-carrier, tension-only spring sharing plus the additive group requirement, both at P5=12. Stiffness is proportional to certified section available at that exposure [U], not a physical reaction bound. Separate islands retain separate groups until merger. Tributary remains a labelled sensitivity diagnostic only. This change is not proof that the old high ratio represented no risk. Singular/ill-conditioned/unbalanced responses remain unresolved; provisional area-weighted shares guide construction only.";
pub const LIMITATIONS: &str = "The section-proportional spring stiffness is unmeasured. A moving peel front is replaced by a single layer resultant. The rigid part cannot certify local transfer through thin growing edges or narrow connections. Angled heads, neck bending, column stability, growing-brace bending and redistribution remain unmeasured. Print observation plan: bottom lip—lost lower layers and torn edges; rib—rib loss before merger and tears at its connection; newly active patches—patch/neck breaks; long columns—movement or brace failure. Record damage from support removal separately. No physical print has run.";
fn n(v: f64) -> Json {
    Json::Num(v)
}
fn u(v: u32) -> Json {
    Json::Uint(u64::from(v))
}
fn z(data: &ReportData, k: u32) -> f64 {
    data.process.lift_mm + f64::from(k) * data.process.layer_height_mm
}
fn group_at(ld: &LoadData, contact: u32, k: u32) -> Option<&GroupLoad> {
    ld.detail
        .groups
        .iter()
        .find(|g| g.layer == k && g.active.contains(&contact))
}
fn section_json(data: &ReportData, ld: &LoadData, s: &SectionDetail, value: (f64, u32)) -> Json {
    let g = group_at(ld, s.chain, value.1);
    Json::obj([
        ("ratio", n(value.0)),
        ("contact", u(s.chain)),
        ("part", Json::str(format!("{:?}", s.kind))),
        ("native_record", s.record.map_or(Json::Null, u)),
        ("layer", u(value.1)),
        ("plate_z_mm", n(z(data, value.1))),
        (
            "picture_marker",
            Json::str(format!("07/12, contact {}", s.chain)),
        ),
        (
            "equilibrium_and_conditioning_pass",
            Json::Bool(g.is_some_and(|g| g.equilibrium_assessed)),
        ),
        ("condition_number", g.map_or(Json::Null, |g| n(g.condition))),
        (
            "force_residual",
            g.map_or(Json::Null, |g| n(g.force_residual)),
        ),
        (
            "moment_residual",
            g.map_or(Json::Null, |g| n(g.moment_residual)),
        ),
    ])
}
pub fn maxima(data: &ReportData, ld: &LoadData) -> Json {
    type Pick = fn(&SectionDetail) -> (f64, u32);
    let models: [(&str, Pick); 4] = [
        ("rigid_lever", |s| s.rigid_lever),
        ("additive", |s| s.additive),
        ("equal_share", |s| s.equal_share),
        ("tributary_diagnostic", |s| s.tributary),
    ];
    Json::obj(models.map(|(name, pick)| {
        let best = ld
            .detail
            .sections
            .iter()
            .filter(|s| {
                name != "rigid_lever"
                    || group_at(ld, s.chain, pick(s).1).is_some_and(|g| g.equilibrium_assessed)
            })
            .max_by(|a, b| pick(a).0.total_cmp(&pick(b).0));
        (
            name,
            best.map_or_else(
                || {
                    Json::obj([(
                        "unverified",
                        Json::str(
                            "No valid equilibrium/conditioning result; not a zero-ratio pass",
                        ),
                    )])
                },
                |s| section_json(data, ld, s, pick(s)),
            ),
        )
    }))
}
fn state_json(data: &ReportData, ld: &LoadData, g: &GroupLoad, c: u32) -> Json {
    let Some(s) = g.states.iter().find(|s| s.contact == c) else {
        return Json::Null;
    };
    let part = ld
        .layout
        .chains
        .get(c as usize)
        .and_then(|ch| ch.parts.get(s.part as usize));
    Json::obj([
        ("layer", u(g.layer)),
        ("plate_z_mm", n(z(data, g.layer))),
        ("component", u(g.component)),
        ("contact", u(c)),
        ("picture_marker", Json::str(format!("07/12, contact {c}"))),
        ("printed_patch_mm2", n(s.patch_mm2)),
        ("limiting_section_mm2", n(s.section_mm2)),
        ("part", Json::str(format!("{:?}", part.map(|p| p.kind)))),
        (
            "native_record",
            part.and_then(|p| p.record).map_or(Json::Null, u),
        ),
        ("available_contacts", u(g.active.len() as u32)),
        ("assigned_load_mm2", n(s.load_mm2)),
        ("load_is_provisional", Json::Bool(!g.equilibrium_assessed)),
        (
            "rigid_lever",
            if g.equilibrium_assessed {
                n(s.ratios.rigid_lever)
            } else {
                Json::Null
            },
        ),
        ("additive", n(s.ratios.additive)),
        ("equal_share", n(s.ratios.equal_share)),
        ("tributary_diagnostic", n(s.ratios.tributary)),
        ("local_transfer_assessed", Json::Bool(false)),
        (
            "equilibrium_and_conditioning_pass",
            Json::Bool(g.equilibrium_assessed),
        ),
        ("condition_number", n(g.condition)),
        ("force_residual", n(g.force_residual)),
        ("normalised_moment_residual", n(g.moment_residual)),
        ("outside_contact_spread_mm", n(g.outside_hull_mm)),
        (
            "patch_lower_bound_certified",
            Json::Bool(
                ld.layout
                    .patch_history
                    .get(c as usize)
                    .is_some_and(|h| !h.is_empty()),
            ),
        ),
    ])
}
fn locations(ld: &LoadData) -> Vec<(&'static str, u32, u32)> {
    let mut out = Vec::new();
    if let Some(g) = ld.detail.groups.iter().find(|g| !g.active.is_empty()) {
        let c = *g.active.first().expect("active");
        out.push(("bottom lip", c, g.layer));
    }
    if let Some(island) = ld.structure.islands.iter().find(|i| i.first_layer > 0)
        && let Some(g) = ld
            .detail
            .groups
            .iter()
            .filter(|g| {
                g.component == island.component
                    && island.merged.is_none_or(|(_, k)| g.layer < k)
                    && !g.active.is_empty()
            })
            .max_by(|a, b| a.x_group.total_cmp(&b.x_group))
    {
        let c = *g.active.first().expect("active");
        out.push(("rib island before merger", c, g.layer));
    }
    if let Some((g, s)) = ld
        .detail
        .groups
        .iter()
        .flat_map(|g| g.states.iter().map(move |s| (g, s)))
        .filter(|(_, s)| {
            ld.layout
                .contacts
                .get(s.contact as usize)
                .is_some_and(|c| s.patch_mm2 < c.patch_lower_mm2 - 1e-9)
        })
        .max_by(|(_, a), (_, b)| {
            a.ratios
                .rigid_lever
                .max(a.ratios.additive)
                .total_cmp(&b.ratios.rigid_lever.max(b.ratios.additive))
        })
    {
        out.push(("worst newly active patch", s.contact, g.layer));
    }
    out
}
pub fn critical(data: &ReportData, ld: &LoadData) -> Json {
    Json::Arr(
        locations(ld)
            .into_iter()
            .map(|(name, c, k)| {
                let g = group_at(ld, c, k).expect("located group");
                let full = ld.layout.contacts[c as usize].patch_lower_mm2;
                let mut history = Vec::new();
                for g in ld.detail.groups.iter().filter(|g| g.active.contains(&c)) {
                    history.push(state_json(data, ld, g, c));
                    if g.states
                        .iter()
                        .any(|s| s.contact == c && s.patch_mm2 >= full - 1e-9)
                    {
                        break;
                    }
                }
                Json::obj([
                    ("region", Json::str(name)),
                    ("critical_exposure", state_json(data, ld, g, c)),
                    ("activation_to_patch_growth", Json::Arr(history)),
                ])
            })
            .collect(),
    )
}
pub fn written_json(m: Option<&WrittenMetrics>) -> Json {
    m.map_or(Json::Null, |m| {
        let bins = [(0.0,2.0),(2.0,3.0),(3.0,4.0),(4.0,f64::INFINITY)];
        Json::obj([
            ("tip_ball_radius_histogram",Json::Arr(m.radius_histogram.iter().map(|(r,c)|Json::obj([("radius_mm",n(*r)),("count",u(*c))])).collect())),
            ("nearest_column_spacing_histogram",Json::Arr(bins.map(|(lo,hi)|Json::obj([
                ("minimum_inclusive_mm",n(lo)),("maximum_exclusive_mm",n(hi)),
                ("columns",u(m.nearest_column_mm.iter().filter(|&&d|d>=lo && d<hi).count() as u32))])).to_vec())),
            ("nearest_column_spacing_each_mm",Json::Arr(m.nearest_column_mm.iter().map(|&v|n(v)).collect())),
            ("long_columns",u(m.long_columns.len() as u32)),
            ("braced_long_columns",u(m.long_columns.iter().filter(|c|c.2).count() as u32)),
            ("unbraced_long_columns",u(m.long_columns.iter().filter(|c|!c.2).count() as u32)),
            ("brace_diagonals",u(m.braces)),
            ("long_column_details",Json::Arr(m.long_columns.iter().map(|(r,l,b)|Json::obj([
                ("native_record",u(*r)),("length_mm",n(*l)),("has_brace",Json::Bool(*b))])).collect())),
            ("spacing_definition",Json::str("Each written vertical column contributes once; nearest XY distance among all columns, not all-pairs spacing. A brace presence is not proof of stability.")),
        ])
    })
}
pub fn text(out: &mut String, data: &ReportData, ld: &LoadData) {
    let _ = writeln!(
        out,
        "  Numerical thresholds set before run [S]: principal tilt stiffness condition <=1e6; relative force and normalised moment residual <=1e-6. A singular row fails conditioning even with zero residual."
    );
    let _ = writeln!(
        out,
        "  PER-MODEL MAXIMA OVER ALL LAYERS (separate, not the values at another model's maximum):\n{}",
        maxima(data, ld).render()
    );
    let _ = writeln!(
        out,
        "  CRITICAL LOCATIONS AND ACTIVATION-TO-PATCH-GROWTH HISTORY:\n{}",
        critical(data, ld).render()
    );
    let _ = writeln!(out, "  {LIMITATIONS}");
    let _ = writeln!(
        out,
        "  WRITTEN SUPPORT MEASUREMENTS:\n{}",
        written_json(data.written_metrics.as_ref()).render()
    );
    if data.identity.approved_sha256
        == "853709e4063de2e3c9dc7a646f13d5ede5a2f26c4a4372ab220cfe153480750a"
    {
        let _ = writeln!(
            out,
            "  Before/after reference [M]: old file SHA256 9157c177866f5cb685bbd070ed3b40a167f7bb38dfda4286bff34888d17004d1 had 63 contacts, 44 at ball radius 0.4134 mm, 58 columns with nearest neighbour <2 mm, 18/51 long columns braced, 389/1524 coverage samples uncovered. Compare the new written-file measurements above; no contact count is a target."
        );
    }
}

/// Decode contact sizes and actual column/brace endpoints from the emitted file.
pub fn measure_written(m: &plinth_chitubox::Model) -> WrittenMetrics {
    use plinth_chitubox::RecordType as T;
    let Some(s) = &m.supports else {
        return WrittenMetrics::default();
    };
    let mut radii: Vec<_> = s
        .records
        .iter()
        .filter(|r| r.record_type == T::TIP_BALL)
        .map(|r| r.r1)
        .collect();
    radii.sort_by(f32::total_cmp);
    let mut histogram: Vec<(f64, u32)> = Vec::new();
    for r in radii {
        if let Some(last) = histogram
            .last_mut()
            .filter(|x| x.0.to_bits() == f64::from(r).to_bits())
        {
            last.1 += 1;
        } else {
            histogram.push((f64::from(r), 1));
        }
    }
    let rods: Vec<_> = s
        .records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.record_type == T::ROD)
        .collect();
    let vertical = |r: &plinth_chitubox::SupportRecord| {
        (r.start[0] - r.end[0]).abs() < 1e-6 && (r.start[1] - r.end[1]).abs() < 1e-6
    };
    let columns: Vec<_> = rods.iter().copied().filter(|(_, r)| vertical(r)).collect();
    let braces: Vec<_> = rods.iter().copied().filter(|(_, r)| !vertical(r)).collect();
    let mut result = WrittenMetrics {
        radius_histogram: histogram,
        braces: braces.len() as u32,
        ..WrittenMetrics::default()
    };
    for (i, r) in &columns {
        let nearest = columns
            .iter()
            .filter(|(j, _)| i != j)
            .map(|(_, q)| {
                f64::from(r.start[0] - q.start[0]).hypot(f64::from(r.start[1] - q.start[1]))
            })
            .fold(f64::INFINITY, f64::min);
        result.nearest_column_mm.push(nearest);
        let length = f64::from((r.start[2] - r.end[2]).abs());
        if length > 4.0 {
            let braced = braces.iter().any(|(_, b)| {
                [b.start, b.end]
                    .iter()
                    .any(|p| (p[0] - r.start[0]).abs() < 1e-5 && (p[1] - r.start[1]).abs() < 1e-5)
            });
            result.long_columns.push((*i as u32, length, braced));
        }
    }
    result
}
