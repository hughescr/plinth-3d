//! Frozen construction plans to native records. This path is deliberately separate
//! from the CLI's transport path while the contact planner remains NotBuilt.
//! No support geometry is selected, moved, grown or repaired here.

use crate::shapes::{self, FrustumRole, Spec, SphereRole};
use plinth_chitubox::{Model, Preset, Preview, Project, SupportRecord, Supports};
use plinth_support::construct::{NativePart, PartFields, PartKind, SupportPlan};

/// Emission refused, rather than silently changing certified geometry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A plan part did not match its native role.
    UnknownPart,
    /// Native builder refused the exact fields.
    Shape(shapes::ShapeError),
    /// Native mesh differed from what construction checked.
    GeometryChanged,
    /// Preview could not be made.
    Preview,
}

fn spec(part: &NativePart) -> Result<Spec, Error> {
    let kind = part.kind();
    Ok(match part.fields() {
        PartFields::Ball { start, end, radius } => Spec::Sphere {
            role: match kind {
                PartKind::TipBall => SphereRole::TipBall,
                PartKind::JointBall => SphereRole::JointBall,
                _ => return Err(Error::UnknownPart),
            },
            start,
            end,
            radius,
        },
        PartFields::Frustum { start, end, r1, r2 } if kind == PartKind::TipCone => Spec::TipCone {
            start,
            end,
            r1,
            r2,
            contact: 0.1,
        },
        PartFields::Frustum { start, end, r1, r2 } => Spec::Frustum {
            role: match kind {
                PartKind::Rod => FrustumRole::Rod,
                PartKind::BaseFlare => FrustumRole::BaseFlare,
                _ => return Err(Error::UnknownPart),
            },
            start,
            end,
            r1,
            r2,
        },
        PartFields::Pad { centre, top } => Spec::RaftPad {
            centre,
            z_top: top,
            thickness: 0.2,
            diameter: 2.2,
        },
        PartFields::Bridge { from, to, top } => Spec::RaftBridge {
            from,
            to,
            z_top: top,
            thickness: 0.2,
            width: 2.2,
        },
    })
}

/// Build the record wrapper using the step-10 builders, then require every
/// checked vertex bit to match. Output contains the exact checked mesh.
pub fn records(plan: &SupportPlan) -> Result<Vec<SupportRecord>, Error> {
    plan.parts()
        .iter()
        .map(|part| {
            let mut record = shapes::build(&spec(part)?).map_err(Error::Shape)?;
            let bits = |mesh: &[plinth_chitubox::Triangle]| {
                mesh.iter()
                    .flatten()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
            };
            if bits(&record.mesh) != bits(part.mesh()) {
                return Err(Error::GeometryChanged);
            }
            record.mesh = part.mesh().to_vec();
            Ok(record)
        })
        .collect()
}

/// A diagnostic project from an immutable plan. Geometry acceptance, the load
/// model and H8 remain independent obligations; this function grants no status.
pub fn project(plan: &SupportPlan) -> Result<Project, Error> {
    let records = records(plan)?;
    let attrs = plan
        .parts()
        .iter()
        .map(|part| match part.kind() {
            PartKind::RaftPad | PartKind::RaftBridge => shapes::RAFT_ATTR,
            _ => 0x0400,
        })
        .collect();
    let preview = Preview::solid(800, 600, plinth_chitubox::preview::BACKGROUND)
        .map_err(|_| Error::Preview)?;
    Ok(Project {
        preview: Some(preview),
        models: vec![Model {
            name: "fixed-size construction diagnostic, not a print candidate".into(),
            translate: plan.model().translate(),
            mesh: plan.model_triangles().to_vec(),
            supports: Some(Supports {
                presets: vec![Preset {
                    id: 0,
                    name: "Light".into(),
                }],
                attrs,
                unreferenced_blob: Vec::new(),
                records,
            }),
        }],
        ..Project::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_geom::Mesh;
    use plinth_support::construct::*;

    #[test]
    fn frozen_plan_roundtrip_preserves_every_field_and_vertex_bit() {
        let mesh = Mesh::new(
            vec![-8., 32., 32., -8.],
            vec![-25., -25., 15., 15.],
            vec![0.; 4],
            vec![0, 2, 1, 0, 3, 2],
        )
        .unwrap();
        let limits = Limits::current().unwrap();
        let keep = KeepUpRecord {
            rotation_x_deg: 115.,
            approved_sha256: "0".repeat(64),
        };
        let posed = PosedModel::new(&mesh, &keep, &limits).unwrap();
        let mut builder = PlanBuilder::new(
            posed,
            limits,
            ProcessRecord {
                layer_height_mm: 0.05,
                layer_origin_mm: 0.,
            },
        );
        let candidate = ContactCandidate {
            id: CandId(0),
            site: [0.; 3],
            ball_r: 0.212,
            standoff: 0.0772,
            axis: [0., 0., -1.],
            neck: 0.2,
            joint_r: 0.3,
            head_len: 2.4,
        };
        let mut out = Vec::new();
        builder.probe(&[candidate], &mut out);
        let contact = builder
            .commit(QuoteBatch::from_results(out).unwrap())
            .unwrap()[0];
        let mut builder = builder.into_routing();
        let mut out = Vec::new();
        builder.probe_members(
            &[MemberProposal::Column {
                contact,
                section: Section::new(0.25).unwrap(),
            }],
            &mut out,
        );
        builder
            .commit_members(MemberQuoteBatch::from_results(out).unwrap())
            .unwrap();
        builder.raft_connected().unwrap();
        let plan = builder.freeze().unwrap();
        let expected = project(&plan).unwrap();
        let bytes = expected.to_bytes().unwrap();
        let decoded = plinth_chitubox::parse(&bytes).unwrap();
        assert_eq!(decoded.to_bytes().unwrap(), bytes);
        assert_eq!(
            decoded.models[0].translate.map(f32::to_bits),
            [12_f32, -5., 5.].map(f32::to_bits)
        );
        // Centred local vertices plus the written native translation preserve
        // the STL's XY position, not a silently re-centred plate placement.
        for (local, original) in decoded.models[0]
            .mesh
            .iter()
            .zip(mesh.indices().chunks_exact(3))
        {
            for k in 0..3 {
                assert_eq!(
                    f64::from(local[k][0] + decoded.models[0].translate[0]).to_bits(),
                    mesh.xs()[original[k] as usize].to_bits()
                );
                assert_eq!(
                    f64::from(local[k][1] + decoded.models[0].translate[1]).to_bits(),
                    mesh.ys()[original[k] as usize].to_bits()
                );
            }
        }
        let records = &decoded.models[0].supports.as_ref().unwrap().records;
        for (record, part) in records.iter().zip(plan.parts()) {
            assert_eq!(record.mesh, part.mesh());
            match part.fields() {
                PartFields::Ball { start, end, radius } => {
                    assert_eq!(record.start.map(f32::to_bits), start.map(f32::to_bits));
                    assert_eq!(record.end.map(f32::to_bits), end.map(f32::to_bits));
                    assert_eq!(record.r1.to_bits(), radius.to_bits());
                }
                PartFields::Frustum { start, end, r1, r2 } => {
                    assert_eq!(record.start.map(f32::to_bits), start.map(f32::to_bits));
                    assert_eq!(record.end.map(f32::to_bits), end.map(f32::to_bits));
                    assert_eq!(record.r1.to_bits(), r1.to_bits());
                    assert_eq!(record.r2.to_bits(), r2.to_bits());
                }
                _ => {}
            }
        }
        assert_eq!(records.len(), 6);
    }
}
