use super::*;
use plinth_support::construct::{KeepUpRecord, Limits, PosedModel, RestingUp};

fn fixture(name: &str) -> (Mesh, Hollowed, plinth_chitubox::Project) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../test-data/chitubox-native/{name}.stl"));
    let original = plinth_geom::read_stl_file(&path).unwrap();
    let h = hollow::hollow(&original).unwrap();
    let posed = PosedModel::new(
        &h.mesh,
        &KeepUpRecord::resting_up(RestingUp::PlusZ, "a".repeat(64)),
        &Limits::current().unwrap(),
    )
    .unwrap();
    let (mut project, _) = plinth_emit::transport_project("REVIEW ONLY", &h.mesh, 1).unwrap();
    project.models[0].mesh = posed.triangles().to_vec();
    project.models[0].translate = posed.translate();
    (original, h, project)
}

#[test]
fn wall_and_bores_measured_after_production_model_serialization() {
    for name in ["Sphere", "Cylinder"] {
        let (original, h, project) = fixture(name);
        let bytes = project.to_bytes().unwrap();
        let evidence = written(&original, &h, &bytes).unwrap();
        assert!(
            matches!(evidence.get("minimum_wall_lower_bound_mm"), Some(Json::Num(n)) if *n >= 2.5)
        );
        let decoded = plinth_chitubox::parse(&bytes).unwrap();
        hollow::closed(&soup(&decoded.models[0].mesh).unwrap()).unwrap();
    }
}

#[test]
fn serialized_29mm_bore_and_24mm_wall_controls_are_detected() {
    let (original, h, project) = fixture("Cylinder");
    // Mutate shared positions identically so each negative remains closed/manifold.
    for (thin_wall, reason) in [(false, "diameter"), (true, "wall")] {
        let mut broken = project.clone();
        for v in broken.models[0].mesh.iter_mut().flatten() {
            let r = v[0].hypot(v[1]);
            if thin_wall {
                if v[2].abs() < 14. && r < 14. {
                    if v[2].abs() > 12.4 {
                        v[2] += v[2].signum() * 0.101;
                    }
                    if r > 10. {
                        let scale = (r + 0.101) / r;
                        v[0] *= scale;
                        v[1] *= scale;
                    }
                }
            } else if r < 1.51 {
                v[0] *= 2.9 / 3.0002;
                v[1] *= 2.9 / 3.0002;
            }
        }
        let bytes = broken.to_bytes().unwrap();
        let decoded = plinth_chitubox::parse(&bytes).unwrap();
        hollow::closed(&soup(&decoded.models[0].mesh).unwrap()).unwrap();
        let error = written(&original, &h, &bytes).unwrap_err();
        assert!(error.contains(reason), "{error}");
    }
}
