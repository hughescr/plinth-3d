//! CAL-1 only: calibration piece, not a print candidate for any model.
//! No production planner, construction API, load classifier or checker exceptions live here.

pub mod build;
pub mod goo;
pub mod goo_grid;
pub mod light;
pub mod plate;
pub mod plate_report;
mod plate_sections;
pub mod replay;
mod replay_geom;
mod replay_native;
pub mod replicates;
pub mod section;
pub mod slice_rules;
pub mod spec;
pub mod witness;
#[cfg(test)]
mod witness_tests;

pub const LABEL: &str = "calibration piece, not a print candidate";
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Native-compatible blank background, not a misleading preview of the three-model source.
pub fn blank_preview() -> plinth_chitubox::Preview {
    plinth_chitubox::Preview::solid(800, 600, plinth_chitubox::preview::BACKGROUND)
        .expect("fixed preview below native pixel limit")
}

/// Body-only reference STL in the recorded XY pose. ChituBox/H7 centre the body
/// on import; STL has no lift field. These are not supported print files.
pub fn reference_stl(model: &plinth_chitubox::Model) -> Vec<u8> {
    let mut mesh = model.mesh.clone();
    for p in mesh.iter_mut().flatten() {
        p[0] += model.translate[0];
        p[1] += model.translate[1];
    }
    stl(&mesh)
}

/// Binary STL with the calibration warning in its header. Vertices are the written f32s.
pub fn stl(mesh: &[plinth_chitubox::Triangle]) -> Vec<u8> {
    let mut bytes = vec![0; 80];
    bytes[..LABEL.len()].copy_from_slice(LABEL.as_bytes());
    bytes.extend_from_slice(
        &u32::try_from(mesh.len())
            .expect("small calibration mesh")
            .to_le_bytes(),
    );
    for tri in mesh {
        bytes.extend_from_slice(&[0; 12]);
        for coordinate in tri.iter().flatten() {
            bytes.extend_from_slice(&coordinate.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 2]);
    }
    bytes
}
