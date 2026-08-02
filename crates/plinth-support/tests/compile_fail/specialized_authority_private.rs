use plinth_support::specialized::{SpecializedDemand, SpecializedDemandKind};

fn main() {
    let _forged = SpecializedDemand {
        kind: SpecializedDemandKind::Minimum,
        owner_island_id: 0,
        coverage_face_id: 0,
        coverage_component_id: 0,
        nominated_position_mm: [0.0; 3],
        nominated_normal: [0.0, 0.0, -1.0],
        region_points_mm: vec![[0.0; 3]],
        birth_z_mm: None,
        birth_layer_index: None,
        birth_footprint: None,
    };
}
