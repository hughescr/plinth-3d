//! Native head paths, including the CAL-1 extra waist/junction/cone path.
use crate::{
    Result, goo_grid,
    replay_geom::{self as g, V},
    section::{self, SectionMesh},
};
use plinth_chitubox::{Model, RecordType as Ty, SupportRecord as Record};
use plinth_support::load::{ContactView, LayoutView, PartChain, PartKind, PartSection, SiteClass};
use serde_json::{Value, json};
use std::collections::BTreeSet;
fn point(p: [f32; 3]) -> V {
    p.map(f64::from)
}
fn zrange(r: &Record, shift: f64) -> [f64; 2] {
    let mut z = [f64::INFINITY, f64::NEG_INFINITY];
    for p in r.mesh.iter().flatten() {
        z[0] = z[0].min(f64::from(p[2]) + shift);
        z[1] = z[1].max(f64::from(p[2]) + shift);
    }
    z
}
fn section(r: &Record) -> f64 {
    let origin = point(r.start);
    let axis = section::sub(point(r.end), origin);
    let length = g::length(axis);
    if matches!(r.record_type, Ty::JOINT_BALL | Ty::TIP_BALL) {
        return section::union_area(&[SectionMesh::new(&r.mesh, origin, axis)], length / 2.0);
    }
    // Avoid the ambiguous exterior plane at an endpoint; constant rods are exact.
    section::minimum(
        &[SectionMesh::new(&r.mesh, origin, axis)],
        length * 1e-6,
        length * (1.0 - 1e-6),
        128,
    )
    .area
}
fn part(r: &Record, index: usize, kind: PartKind, shift: f64) -> PartSection {
    PartSection {
        kind,
        record: Some(index as u32),
        area_lower_mm2: section(r),
        z_range_mm: zrange(r, shift),
    }
}
fn downward_parts(
    records: &[Record],
    ti: usize,
    parts: &mut Vec<PartSection>,
    shift: f64,
    used: &mut BTreeSet<usize>,
) -> Result<Vec<usize>> {
    let mut at = point(records[ti].end);
    let mut path = vec![ti];
    let mut visited = BTreeSet::new();
    loop {
        for (ji, r) in records.iter().enumerate().filter(|(_, r)| {
            r.record_type == Ty::JOINT_BALL && g::near(g::centre(r.start, r.end), at)
        }) {
            if !parts.iter().any(|p| p.record == Some(ji as u32)) {
                parts.push(part(r, ji, PartKind::JointThroat, shift));
            }
        }
        let mut options: Vec<_> = records
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                if r.record_type != Ty::ROD || visited.contains(&i) {
                    return None;
                }
                let other = if g::near(point(r.start), at) {
                    point(r.end)
                } else if g::near(point(r.end), at) {
                    point(r.start)
                } else {
                    return None;
                };
                (other[2] < at[2] - 1e-4).then_some((i, other))
            })
            .collect();
        options.sort_by(|a, b| a.1[2].total_cmp(&b.1[2]));
        let Some((ri, next)) = options.first().copied() else {
            break;
        };
        visited.insert(ri);
        used.insert(ri);
        path.push(ri);
        parts.push(part(&records[ri], ri, PartKind::Column, shift));
        at = next;
    }
    // The CAL-1 column overlaps the flare by 0.002 mm instead of sharing an endpoint.
    let flare = records.iter().enumerate().find(|(_, r)| {
        r.record_type == Ty::BASE_FLARE
            && (g::length(section::sub(point(r.start), at)) < 0.003
                || g::length(section::sub(point(r.end), at)) < 0.003)
    });
    let Some((fi, fr)) = flare else {
        return Err(format!("tip {ti} does not reach a base flare: {at:?}").into());
    };
    parts.push(part(fr, fi, PartKind::Flare, shift));
    Ok(path)
}
fn braces(
    records: &[Record],
    used: &BTreeSet<usize>,
    layout: &mut LayoutView,
    shift: f64,
) -> Result<()> {
    let mut unassigned = Vec::new();
    for (ri, r) in records
        .iter()
        .enumerate()
        .filter(|(i, r)| r.record_type == Ty::ROD && !used.contains(i))
    {
        let low = if r.start[2] < r.end[2] {
            point(r.start)
        } else {
            point(r.end)
        };
        let owner = layout
            .chains
            .iter()
            .take(layout.contacts.len())
            .position(|ch| {
                ch.parts.iter().filter_map(|p| p.record).any(|i| {
                    let q = &records[i as usize];
                    [point(q.start), point(q.end)]
                        .iter()
                        .any(|p| (p[0] - low[0]).hypot(p[1] - low[1]) < 0.001)
                })
            });
        if let Some(ci) = owner {
            layout.chains.push(PartChain {
                contact: ci as u32,
                parts: vec![part(r, ri, PartKind::Brace, shift)],
            });
        } else {
            unassigned.push(ri);
        }
    }
    if !unassigned.is_empty() {
        return Err(format!("unassigned native rods {unassigned:?}").into());
    }
    Ok(())
}
pub fn layout(m: &Model, origin: f64, height: f64) -> Result<(LayoutView, Value)> {
    let records = &m
        .supports
        .as_ref()
        .ok_or("native piece has no supports")?
        .records;
    let shift = goo_grid::offset(m)[2];
    let mut layout = LayoutView::default();
    let mut diagnostics = Vec::new();
    let mut used = BTreeSet::new();
    for (ti, tip) in records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.record_type == Ty::TIP_CONE)
    {
        let balls: Vec<_> = records
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.record_type == Ty::TIP_BALL
                    && g::near(g::centre(r.start, r.end), point(tip.start))
            })
            .collect();
        let [(bi, ball)] = balls.as_slice() else {
            return Err(format!("tip {ti} needs exactly one ball").into());
        };
        let c = g::centre(ball.start, ball.end);
        let (patch, z) = g::patch(&m.mesh, &ball.mesh, &tip.mesh);
        if patch <= 0.0 || !z[0].is_finite() {
            return Err(format!("tip {ti} has no written patch").into());
        }
        let axis = section::sub(point(tip.end), point(tip.start));
        let len = g::length(axis);
        let mut heads = vec![
            SectionMesh::new(&ball.mesh, point(tip.start), axis),
            SectionMesh::new(&tip.mesh, point(tip.start), axis),
        ];
        // Include the far junction; unlike a conventional cone, the waist is a
        // constant-radius tip and the next tapered cone is a separate rod record.
        for r in records.iter().filter(|r| {
            r.record_type == Ty::JOINT_BALL && g::near(g::centre(r.start, r.end), point(tip.end))
        }) {
            heads.push(SectionMesh::new(&r.mesh, point(tip.start), axis));
        }
        let neck = section::minimum(&heads, 0.0, len / 2.0, 2048);
        let first = ((z[0] + shift - origin) / height).floor().max(0.0) as u32;
        let ci = layout.contacts.len();
        layout.contacts.push(ContactView {
            centre: [c[0], c[1], c[2] + shift],
            patch_lower_mm2: patch,
            neck_area_mm2: neck.area,
            first_layer: first,
            site_class: SiteClass::HiddenUnderside,
        });
        let mut parts = vec![
            PartSection {
                kind: PartKind::Patch,
                record: Some(*bi as u32),
                area_lower_mm2: patch,
                z_range_mm: [z[0] + shift, z[1] + shift],
            },
            PartSection {
                kind: PartKind::Neck,
                record: Some(ti as u32),
                area_lower_mm2: neck.area,
                z_range_mm: zrange(tip, shift),
            },
        ];
        let path = downward_parts(records, ti, &mut parts, shift, &mut used)?;
        layout.chains.push(PartChain {
            contact: ci as u32,
            parts,
        });
        diagnostics.push(json!({"contact_M":ci,"tip_record_M":ti,"ball_record_M":bi,"centre_local_M":c,"centre_plate_z_M":c[2]+shift,"patch_area_mm2_M":patch,"patch_z_range_local_M":z,"neck_area_mm2_M":neck.area,"neck_sample_step_mm_M":neck.step,"neck_axis_station_mm_M":neck.at,"path_records_M":path}));
    }
    braces(records, &used, &mut layout, shift)?;
    Ok((
        layout,
        json!({"contacts_M":diagnostics,"contact_count_M":diagnostics.len(),"note_U":"Patch area and z span are nominal intersections of the written ball/tip face half-spaces, not certified lower bounds. Small f32 nonconvexity and checker shrink allowances are not bracketed. The unchanged load API uses a tilted-disc exposure approximation, not measured patch history. Downward endpoint-connected rods carry the head; other rods add self-peel as braces, never extra capacity."}),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build, spec::PlateSpec};
    #[test]
    fn waisted_path_uses_waist_and_sabotage_fires() {
        let spec: PlateSpec =
            serde_json::from_str(include_str!("../../../fixtures/calibration/cal1.json")).unwrap();
        let specimen = spec.specimens.iter().find(|s| s.id == "S-324").unwrap();
        let p = build::specimen(specimen, &spec.head);
        let m = &p.models[0];
        let (layout, _) = layout(m, 0.0, 0.05).unwrap();
        assert_eq!(layout.contacts.len(), 4);
        let ch = &layout.chains[0];
        let minimum = ch
            .parts
            .iter()
            .min_by(|a, b| a.area_lower_mm2.total_cmp(&b.area_lower_mm2))
            .unwrap();
        assert_eq!(minimum.kind, PartKind::Neck);
        assert_eq!(minimum.record, Some(1));
        assert!(minimum.area_lower_mm2 > 0.011 && minimum.area_lower_mm2 < 0.013);
        // Sabotage: omit the waist. A canary requiring a waist-limited path must reject it.
        let mut broken = ch.clone();
        broken.parts.retain(|p| p.kind != PartKind::Neck);
        let is_waist = |c: &PartChain| {
            c.parts
                .iter()
                .min_by(|a, b| a.area_lower_mm2.total_cmp(&b.area_lower_mm2))
                .is_some_and(|p| p.kind == PartKind::Neck)
        };
        assert!(is_waist(ch));
        assert!(!is_waist(&broken));
    }
}
