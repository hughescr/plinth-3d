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
            let depth =
                dyadic::refine_depth(dyadic::max_edge_mm(cell), config.voxel_pitch_mm.get());
            let cells = dyadic::terminal_cells(depth).map_err(map_dyadic_error)?;
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
            .map_err(map_dyadic_error)?;
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
pub(super) fn map_dyadic_error(error: DyadicError) -> ContactSelectionError {
    match error {
        DyadicError::Overflow => ContactSelectionError::CoverageResourceOverflow,
        DyadicError::VirtualCellLimit { required, limit } => {
            ContactSelectionError::CoverageVirtualCellLimit { required, limit }
        }
        DyadicError::InvalidLimits => ContactSelectionError::InvalidResourceRequest,
        DyadicError::RegisteredConstantsUnavailable(message) => {
            ContactSelectionError::RegisteredConstantsUnavailable(message)
        }
        DyadicError::InvalidRegisteredGeometry
        | DyadicError::RefinementDepthLimit { .. }
        | DyadicError::CandidateLimit { .. }
        | DyadicError::CompactPayloadLimit { .. }
        | DyadicError::FaceOutOfRange { .. }
        | DyadicError::DegenerateFace { .. } => ContactSelectionError::InvalidResourceRequest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_one_coverage_payload_matches_full_frozen_row_order() {
        let mesh = Mesh::new(
            vec![[0.0, 0.0, 0.0], [0.4, 0.0, 0.0], [0.0, 0.4, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("finite coverage triangle");
        let config = ContactSelectionConfig::current(None, 4, 1, 1024 * 1024)
            .expect("registered coverage config");
        let rows = build_dyadic_coverage_samples(
            &mesh,
            &[SupportIsland {
                id: 11,
                face_ids: vec![0],
            }],
            &[7],
            &config,
        )
        .expect("depth-one coverage");
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows.iter().map(|row| row.point_mm).collect::<Vec<_>>(),
            vec![
                [0.066_666_666_666_666_67, 0.066_666_666_666_666_67, 0.0],
                [0.066_666_666_666_666_67, 0.266_666_666_666_666_66, 0.0],
                [0.133_333_333_333_333_33, 0.133_333_333_333_333_33, 0.0],
                [0.266_666_666_666_666_66, 0.066_666_666_666_666_67, 0.0],
            ]
        );
        assert!(rows.iter().all(|row| row.island_id == 11
            && row.face_id == 0
            && row.component_id == 7
            && row.area_mm2 == 0.020_000_000_000_000_004));
    }
}
