//! Typed A3 wrapper over the shared dyadic sampler.
use super::*;
use crate::dyadic::{self, DyadicError, DyadicLimits};

pub(super) fn build_dyadic_coverage_samples(
    mesh: &Mesh,
    islands: &[SupportIsland],
    components: &[u32],
    config: &ContactSelectionConfig<'_>,
) -> Result<Vec<CoverageSample>, ContactSelectionError> {
    build_dyadic_coverage_samples_with_high_water(mesh, islands, components, config)
        .map(|(rows, _)| rows)
}

pub(super) fn build_dyadic_coverage_samples_with_high_water(
    mesh: &Mesh,
    islands: &[SupportIsland],
    components: &[u32],
    config: &ContactSelectionConfig<'_>,
) -> Result<(Vec<CoverageSample>, usize), ContactSelectionError> {
    let plan = plan_dyadic_coverage(mesh, islands, config)?;
    let mut rows = Vec::with_capacity(plan.total_cells);
    let mut high_water = 0;
    for island in islands {
        for &face_id in &island.face_ids {
            let face = mesh.tris()[face_id as usize];
            let cell = [
                mesh.verts()[face[0] as usize],
                mesh.verts()[face[1] as usize],
                mesh.verts()[face[2] as usize],
            ];
            let depth = dyadic::refine_depth(max_edge_mm(cell), config.voxel_pitch_mm.get());
            let cells = dyadic::terminal_cells(depth).map_err(map_error)?;
            let area = triangle_area_mm2(cell) / cells as f64;
            let limits = DyadicLimits {
                max_virtual_cells: cells,
                max_working_cells: config.coverage_samples_max_working_cells,
            };
            let seen = dyadic::for_each_leaf(cell, depth, limits, |leaf| {
                let [a, b, c] = leaf;
                rows.push(CoverageSample {
                    island_id: island.id,
                    face_id,
                    component_id: components[face_id as usize],
                    point_mm: [
                        (a[0] + b[0] + c[0]) / 3.0,
                        (a[1] + b[1] + c[1]) / 3.0,
                        (a[2] + b[2] + c[2]) / 3.0,
                    ],
                    area_mm2: area,
                });
            })
            .map_err(map_error)?;
            high_water = high_water.max(seen);
        }
    }
    rows.sort_unstable_by(|a, b| {
        a.island_id
            .cmp(&b.island_id)
            .then_with(|| a.face_id.cmp(&b.face_id))
            .then_with(|| a.point_mm[0].total_cmp(&b.point_mm[0]))
            .then_with(|| a.point_mm[1].total_cmp(&b.point_mm[1]))
            .then_with(|| a.point_mm[2].total_cmp(&b.point_mm[2]))
    });
    Ok((rows, high_water))
}
fn map_error(error: DyadicError) -> ContactSelectionError {
    match error {
        DyadicError::Overflow => ContactSelectionError::CoverageResourceOverflow,
        DyadicError::VirtualCellLimit { required, limit } => {
            ContactSelectionError::CoverageVirtualCellLimit { required, limit }
        }
        DyadicError::InvalidLimits => ContactSelectionError::InvalidResourceRequest,
    }
}
