//! Extraction of the owner's zero-based instance 2, with numeric Light guards.
use crate::{
    Result,
    section::{self, SectionMesh},
};
use plinth_chitubox::{Project, RecordType as Ty};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TipMeasurement {
    pub record: usize,
    pub attr: String,
    pub old_radius: f32,
    pub new_radius: f32,
    pub a0_area_mm2: f64,
    pub a1_area_mm2: f64,
    pub a0_axial_mm: f64,
    pub a1_axial_mm: f64,
    pub a0_world_xyz: [f64; 3],
    pub a1_world_xyz: [f64; 3],
    pub area_ratio: f64,
    pub station_step_mm: f64,
    pub changed_fields: Vec<String>,
}

/// Keep the complete native model, including its translate and all support data.
/// Only the name is labelled; the source preview is dropped (it shows three models).
pub fn extract(source: &Project) -> Result<Project> {
    let mut model = source
        .models
        .get(2)
        .ok_or("source has no instance #2")?
        .clone();
    let sup = model
        .supports
        .as_ref()
        .ok_or("Light instance has no supports")?;
    let main: Vec<_> = sup
        .records
        .iter()
        .filter(|r| {
            r.record_type == Ty::TIP_CONE && (r.r1 - 0.2).abs() < 1e-6 && (r.r2 - 0.6).abs() < 1e-6
        })
        .collect();
    let balls = sup
        .records
        .iter()
        .filter(|r| r.record_type == Ty::TIP_BALL && (r.r1 - 0.25).abs() < 1e-6)
        .count();
    if main.len() != 10
        || balls != 10
        || !sup
            .records
            .iter()
            .any(|r| r.record_type == Ty::ROD && (r.r1 - 0.6).abs() < 1e-6)
    {
        return Err("instance #2 does not match Light: need ten 0.5 mm balls and 0.40→1.2 cones, 1.2 mm columns".into());
    }
    model.name = format!("A0/A1; {}", crate::LABEL);
    Ok(Project {
        header_f16: source.header_f16,
        preview: Some(crate::blank_preview()),
        models: vec![model],
    })
}

pub fn thin(a0: &Project) -> Result<Project> {
    let mut a1 = a0.clone();
    let sup = a1.models[0].supports.as_mut().ok_or("no Light supports")?;
    let mut count = 0;
    for r in &mut sup.records {
        if r.record_type == Ty::TIP_CONE && (r.r1 - 0.2).abs() < 1e-6 && (r.r2 - 0.6).abs() < 1e-6 {
            r.r1 = 0.052;
            r.mesh = plinth_geom::shapes::frustum(r.start, r.end, r.r1, r.r2);
            count += 1;
        }
    }
    if count != 10 {
        return Err("expected exactly ten main tip edits".into());
    }
    Ok(a1)
}

/// Typed field-by-field bit comparison of the re-read projects. Pointer fields are
/// rebuilt by the writer, not semantic fields. Names and previews are identical.
pub fn compare(a0: &Project, a1: &Project) -> Result<Vec<TipMeasurement>> {
    let mut undo = a1.clone();
    let (m0, m1) = (&a0.models[0], &a1.models[0]);
    let (s0, s1) = (m0.supports.as_ref().unwrap(), m1.supports.as_ref().unwrap());
    if s0.records.len() != s1.records.len() {
        return Err("record count changed".into());
    }
    let min_model = m0
        .mesh
        .iter()
        .flatten()
        .map(|p| f64::from(p[2]))
        .fold(f64::INFINITY, f64::min);
    let mut out = vec![];
    for (i, (r0, r1)) in s0.records.iter().zip(&s1.records).enumerate() {
        if r0 == r1 {
            continue;
        }
        let mut reverted = r1.clone();
        reverted.r1 = r0.r1;
        reverted.mesh = r0.mesh.clone();
        if reverted != *r0
            || r0.record_type != Ty::TIP_CONE
            || (r0.r1 - 0.2).abs() > 1e-6
            || (r1.r1 - 0.052).abs() > 1e-6
        {
            return Err(format!("record {i}: change outside main cone r1 and mesh").into());
        }
        // Find the contact ball by its centre, not by an assumed adjacency/order.
        let ball = s0
            .records
            .iter()
            .filter(|r| r.record_type == Ty::TIP_BALL)
            .find(|r| (0..3).all(|k| ((r.start[k] + r.end[k]) / 2.0 - r0.start[k]).abs() < 2e-5))
            .ok_or("no contact ball at cone start")?;
        let origin = r0.start.map(f64::from);
        let axis = section::sub(r0.end.map(f64::from), origin);
        let length = section::dot(axis, axis).sqrt();
        let measure = |r: &plinth_chitubox::SupportRecord| {
            let meshes = [
                SectionMesh::new(&r.mesh, origin, axis),
                SectionMesh::new(&ball.mesh, origin, axis),
            ];
            // Full ball→cone transition, not a radius read at the hidden cone start.
            section::minimum(&meshes, 0.0, length * 0.5, 10000)
        };
        let (old, new) = (measure(r0), measure(r1));
        let point = |t: f64| {
            let mut p = std::array::from_fn(|k| {
                origin[k] + axis[k] * t / length + f64::from(m0.translate[k])
            });
            p[2] -= min_model;
            p
        };
        out.push(TipMeasurement {
            record: i,
            attr: format!("0x{:x}", s0.attrs[i]),
            old_radius: r0.r1,
            new_radius: r1.r1,
            a0_area_mm2: old.area,
            a1_area_mm2: new.area,
            a0_axial_mm: old.at,
            a1_axial_mm: new.at,
            a0_world_xyz: point(old.at),
            a1_world_xyz: point(new.at),
            area_ratio: old.area / new.area,
            station_step_mm: old.step,
            changed_fields: vec![
                "r1 at record +32 (f32 bits 0x3e4ccccd → 0x3d54fdf4)".into(),
                "mesh vertex coordinates".into(),
            ],
        });
        // Revert only the two permitted fields, not the entire record. The byte
        // comparison must still catch any other field's bits (including -0/+0).
        let restored = &mut undo.models[0].supports.as_mut().unwrap().records[i];
        restored.r1 = r0.r1;
        restored.mesh = r0.mesh.clone();
    }
    if out.len() != 10 || undo.to_bytes()? != a0.to_bytes()? {
        return Err("A1 differs outside precisely ten r1 fields and ten meshes".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn new_radius_bits_are_documented_correctly() {
        assert_eq!(0.052f32.to_bits(), 0x3d54_fdf4);
    }

    #[test]
    fn field_diff_rejects_even_an_extra_signed_zero_bit() {
        use super::*;
        use plinth_chitubox::{Model, SupportRecord, Supports};
        let mut records = vec![];
        for i in 0..10 {
            let x = i as f32 * 2.0;
            let a = [x, 0.0, 0.25];
            let b = [x, 0.0, -0.25];
            records.push(SupportRecord {
                record_type: Ty::TIP_BALL,
                start: a,
                end: b,
                r1: 0.25,
                r2: 0.25,
                tail: [0; 6],
                mesh: plinth_geom::shapes::sphere(a, b, 0.25),
            });
            let start = [x, 0.0, 0.0];
            let end = [x, 0.0, -2.0];
            records.push(SupportRecord {
                record_type: Ty::TIP_CONE,
                start,
                end,
                r1: 0.2,
                r2: 0.6,
                tail: [0; 6],
                mesh: plinth_geom::shapes::frustum(start, end, 0.2, 0.6),
            });
        }
        let a0 = Project {
            models: vec![Model {
                name: crate::LABEL.into(),
                translate: [0.0; 3],
                mesh: vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]],
                supports: Some(Supports {
                    attrs: vec![0xc01; records.len()],
                    records,
                    ..Supports::default()
                }),
            }],
            ..Project::default()
        };
        let mut a1 = thin(&a0).unwrap();
        a1.models[0].supports.as_mut().unwrap().records[1].start[0] = -0.0;
        let error = compare(&a0, &a1).unwrap_err().to_string();
        assert!(
            error.contains("outside precisely ten r1 fields and ten meshes"),
            "{error}"
        );
    }
}
