//! Scoped sampled-section diagnostics. These are not a plinth load-model replay.
use crate::{
    goo_grid,
    section::{self, SectionMesh},
};
use plinth_chitubox::{Model, RecordType};
use serde_json::{Value, json};

pub fn measure(m: &Model, id: &str) -> Value {
    let frame = goo_grid::offset(m);
    if matches!(id, "K-15" | "K-60") {
        let section = SectionMesh::new(&m.mesh, [0.0, 0.0, -frame[2]], [0.0, 0.0, 1.0]);
        let top = if id == "K-15" { 9.60 } else { 9.70 };
        let min = section::minimum(&[section], 0.001, top - 0.001, 2000);
        return json!({"status":"M: sampled horizontal written-body sections","scope":"base, arm, flare and cap, excluding exterior top/bottom planes; not a certified continuous lower bound","minimum_section_mm2":min.area,"world_z_mm":min.at,"station_step_mm":min.step});
    }
    if id == "W" {
        let paths:Vec<_>=(0..6).map(|i| {
            let xy=[i as f32*2.5-6.25,0.0];
            let triangles=crate::witness::column(&m.mesh,xy);
            let section=SectionMesh::new(&triangles,[0.0,0.0,-frame[2]],[0.0,0.0,1.0]);
            let min=section::minimum(&[section],0.001,4.88,2000);
            json!({"path":i,"minimum_section_mm2":min.area,"world_z_mm":min.at,"station_step_mm":min.step})
        }).collect();
        return json!({"status":"M: sampled written-body horizontal sections","scope":"six loaded paths, from raft through ball equator; excludes unloaded crown; not a certified continuous lower bound","paths":paths});
    }
    if matches!(id, "A0" | "A1") {
        let supports = m.supports.as_ref().unwrap();
        let paths:Vec<_>=(1..20).step_by(2).map(|i| {
            let cone=&supports.records[i];
            let ball=supports.records.iter().filter(|r|r.record_type==RecordType::TIP_BALL).find(|r|
                (0..3).all(|k|((r.start[k]+r.end[k])/2.0-cone.start[k]).abs()<2e-5)).unwrap();
            let origin=cone.start.map(f64::from);
            let axis=section::sub(cone.end.map(f64::from),origin);
            let length=section::dot(axis,axis).sqrt();
            let min=section::minimum(&[SectionMesh::new(&cone.mesh,origin,axis),SectionMesh::new(&ball.mesh,origin,axis)],0.0,length/2.0,10000);
            json!({"cone_record":i,"minimum_axis_normal_section_mm2":min.area,"axis_station_mm":min.at,"station_step_mm":min.step})
        }).collect();
        return json!({"status":"M: ten axis-normal main-head diagnostics only","scope":"cone/ball exposed transition; NOT sliced horizontal neck area, NOT whole-piece minimum; lip contacts, branches and patch-limited sections Unverified here","paths":paths,"frozen_qualifying_A1b_records_M":[1,11,13,15,17,19],"observations_only_records_M":[3,5,7,9]});
    }
    json!({"status":"M: see coupon_contacts for the written waist sections; patch/junction/cone/column dimensions unchanged"})
}

/// Minimum caliper width of the written 16-gon, measured from projected vertices.
/// All edge directions are included by visiting every pair, not just the X/Y box.
pub fn waist_width(mesh: &[plinth_chitubox::Triangle]) -> f64 {
    let mut points: Vec<_> = mesh
        .iter()
        .flatten()
        .map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect();
    points.sort_unstable_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    let mut minimum = f64::INFINITY;
    for (i, a) in points.iter().enumerate() {
        for b in points.iter().skip(i + 1) {
            let v = [b[1] - a[1], a[0] - b[0]];
            let len = v[0].hypot(v[1]);
            if len == 0.0 {
                continue;
            }
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for p in &points {
                let t = (p[0] * v[0] + p[1] * v[1]) / len;
                lo = lo.min(t);
                hi = hi.max(t);
            }
            minimum = minimum.min(hi - lo);
        }
    }
    minimum
}
#[cfg(test)]
mod tests {
    #[test]
    fn caliper_detects_subminimum_and_reports_polygon_not_circle() {
        let mesh = plinth_geom::shapes::frustum([0.0, 0.0, 0.0], [0.0, 0.0, 0.5], 0.0625, 0.0625);
        let width = super::waist_width(&mesh);
        assert!(width > 0.122 && width < 0.123);
        let broken = plinth_geom::shapes::frustum([0.0, 0.0, 0.0], [0.0, 0.0, 0.5], 0.049, 0.049);
        assert!(super::waist_width(&broken) < 0.1);
    }
}
