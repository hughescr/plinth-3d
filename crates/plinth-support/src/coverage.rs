//! Thin typed A3 adapter over the generic dyadic two-pass builder.
use std::cmp::Ordering;

use super::*;
use crate::dyadic::{
    self, DyadicBuildConfig, DyadicBuildError, DyadicFace, DyadicLeaf, DyadicNominations,
    DyadicPlan, DyadicRowPolicy,
};

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
    let policy = CoverageRowPolicy {
        components,
        config,
        island_count: islands.len(),
    };
    let built = dyadic::build_rows(mesh, islands, coverage_build_config(config), &policy)
        .map_err(map_build_error)?;
    Ok((built.rows, built.high_water))
}

#[cfg(test)]
pub(super) fn plan_dyadic_coverage(
    mesh: &Mesh,
    islands: &[SupportIsland],
    config: &ContactSelectionConfig<'_>,
) -> Result<CoverageBuildPlan, ContactSelectionError> {
    let policy = CoverageRowPolicy {
        components: &[],
        config,
        island_count: islands.len(),
    };
    let preflight = dyadic::preflight(mesh, islands, coverage_build_config(config), &policy)
        .map_err(map_build_error)?;
    Ok(preflight.metadata)
}

fn coverage_build_config(config: &ContactSelectionConfig<'_>) -> DyadicBuildConfig {
    DyadicBuildConfig {
        limits: dyadic::DyadicLimits {
            max_virtual_cells: config.coverage_samples_max_virtual_cells,
            max_working_cells: config.coverage_samples_max_working_cells,
        },
        cell_edge_limit_mm: config.voxel_pitch_mm.get(),
        max_refine_depth: None,
    }
}

struct CoverageRowPolicy<'a, 'config> {
    components: &'a [u32],
    config: &'a ContactSelectionConfig<'config>,
    island_count: usize,
}

#[cfg(test)]
pub(super) fn coverage_policy_type_name() -> &'static str {
    std::any::type_name::<CoverageRowPolicy<'static, 'static>>()
}

impl DyadicRowPolicy for CoverageRowPolicy<'_, '_> {
    type Row = CoverageSample;
    type Metadata = CoverageBuildPlan;
    type FaceMetadata = ();
    type Error = ContactSelectionError;

    fn prepare_face(&self, _: DyadicFace) -> Result<Self::FaceMetadata, Self::Error> {
        Ok(())
    }

    fn check_aggregate(&self, plan: DyadicPlan) -> Result<(), Self::Error> {
        let compact_bytes =
            coverage_compact_bytes(plan.total_cells, plan.source_faces, self.island_count)?;
        if compact_bytes > self.config.coverage_samples_max_compact_payload_bytes {
            return Err(ContactSelectionError::CoverageCompactPayloadLimit {
                required: compact_bytes,
                limit: self.config.coverage_samples_max_compact_payload_bytes,
            });
        }
        let build_peak_bytes = coverage_build_peak_bytes(
            plan.total_cells,
            plan.source_faces,
            plan.working_cells,
            self.island_count,
            plan.mesh_faces,
            plan.mesh_vertices,
        )?;
        if build_peak_bytes > self.config.coverage_samples_max_peak_bytes {
            return Err(ContactSelectionError::CoveragePeakLimit {
                phase: "build",
                required: build_peak_bytes,
                limit: self.config.coverage_samples_max_peak_bytes,
            });
        }
        let validation_peak_bytes = coverage_validation_peak_bytes(
            plan.total_cells,
            plan.source_faces,
            self.island_count,
            plan.mesh_faces,
            plan.mesh_vertices,
        )?;
        if validation_peak_bytes > self.config.coverage_samples_max_peak_bytes {
            return Err(ContactSelectionError::CoveragePeakLimit {
                phase: "validation",
                required: validation_peak_bytes,
                limit: self.config.coverage_samples_max_peak_bytes,
            });
        }
        Ok(())
    }

    fn finish_preflight(&self, plan: DyadicPlan) -> Result<Self::Metadata, Self::Error> {
        let compact_bytes =
            coverage_compact_bytes(plan.total_cells, plan.source_faces, self.island_count)?;
        let build_peak_bytes = coverage_build_peak_bytes(
            plan.total_cells,
            plan.source_faces,
            plan.working_cells,
            self.island_count,
            plan.mesh_faces,
            plan.mesh_vertices,
        )?;
        let validation_peak_bytes = coverage_validation_peak_bytes(
            plan.total_cells,
            plan.source_faces,
            self.island_count,
            plan.mesh_faces,
            plan.mesh_vertices,
        )?;
        Ok(CoverageBuildPlan {
            total_cells: plan.total_cells,
            source_faces: plan.source_faces,
            working_cells: plan.working_cells,
            compact_bytes,
            build_peak_bytes,
            validation_peak_bytes,
        })
    }

    fn row_capacity(&self, metadata: &Self::Metadata) -> usize {
        metadata.total_cells
    }

    fn nominations(&self, _: &DyadicLeaf<'_, Self::FaceMetadata>) -> DyadicNominations {
        DyadicNominations::Centroid
    }

    fn map_row(&self, leaf: &DyadicLeaf<'_, Self::FaceMetadata>, point_mm: [f64; 3]) -> Self::Row {
        CoverageSample {
            island_id: leaf.island_id,
            face_id: leaf.face_id,
            component_id: self.components[leaf.face_id as usize],
            point_mm,
            area_mm2: leaf.leaf_area_mm2,
        }
    }

    fn dedup_order(&self, left: &Self::Row, right: &Self::Row) -> Ordering {
        coverage_row_order(left, right)
    }

    fn same_row(&self, _: &mut Self::Row, _: &mut Self::Row) -> bool {
        false
    }

    fn output_order(&self, left: &Self::Row, right: &Self::Row) -> Ordering {
        coverage_row_order(left, right)
    }
}

fn coverage_row_order(left: &CoverageSample, right: &CoverageSample) -> Ordering {
    left.island_id
        .cmp(&right.island_id)
        .then_with(|| left.face_id.cmp(&right.face_id))
        .then_with(|| left.point_mm[0].total_cmp(&right.point_mm[0]))
        .then_with(|| left.point_mm[1].total_cmp(&right.point_mm[1]))
        .then_with(|| left.point_mm[2].total_cmp(&right.point_mm[2]))
}

fn map_build_error(error: DyadicBuildError<ContactSelectionError>) -> ContactSelectionError {
    match error {
        DyadicBuildError::InvalidLimits => ContactSelectionError::InvalidResourceRequest,
        DyadicBuildError::Overflow => ContactSelectionError::CoverageResourceOverflow,
        DyadicBuildError::VirtualCellLimit { required, limit } => {
            ContactSelectionError::CoverageVirtualCellLimit { required, limit }
        }
        DyadicBuildError::Policy(error) => error,
        DyadicBuildError::RefinementDepthLimit { .. }
        | DyadicBuildError::FaceOutOfRange { .. }
        | DyadicBuildError::DegenerateFace { .. } => ContactSelectionError::InvalidResourceRequest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_triangles() -> Mesh {
        Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [0.4, 0.0, 0.0],
                [0.0, 0.4, 0.0],
                [1.0, 0.0, 0.0],
                [1.4, 0.0, 0.0],
                [1.0, 0.4, 0.0],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .expect("finite coverage fixture")
    }

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

    #[test]
    fn multi_face_coverage_vector_pins_order_and_retains_equal_rows() {
        let rows = build_dyadic_coverage_samples(
            &two_triangles(),
            &[SupportIsland {
                id: 11,
                // Coverage has no dedup policy: both visits remain evidence.
                face_ids: vec![1, 0, 0],
            }],
            &[7, 8],
            &ContactSelectionConfig::current(None, 12, 1, 1024 * 1024)
                .expect("registered coverage config"),
        )
        .expect("multi-face coverage payload");
        let area = 0.020_000_000_000_000_004;
        assert_eq!(
            rows,
            vec![
                CoverageSample {
                    island_id: 11,
                    face_id: 0,
                    component_id: 7,
                    point_mm: [0.066_666_666_666_666_67, 0.066_666_666_666_666_67, 0.0],
                    area_mm2: area
                },
                CoverageSample {
                    island_id: 11,
                    face_id: 0,
                    component_id: 7,
                    point_mm: [0.066_666_666_666_666_67, 0.066_666_666_666_666_67, 0.0],
                    area_mm2: area
                },
                CoverageSample {
                    island_id: 11,
                    face_id: 0,
                    component_id: 7,
                    point_mm: [0.066_666_666_666_666_67, 0.266_666_666_666_666_66, 0.0],
                    area_mm2: area
                },
                CoverageSample {
                    island_id: 11,
                    face_id: 0,
                    component_id: 7,
                    point_mm: [0.066_666_666_666_666_67, 0.266_666_666_666_666_66, 0.0],
                    area_mm2: area
                },
                CoverageSample {
                    island_id: 11,
                    face_id: 0,
                    component_id: 7,
                    point_mm: [0.133_333_333_333_333_33, 0.133_333_333_333_333_33, 0.0],
                    area_mm2: area
                },
                CoverageSample {
                    island_id: 11,
                    face_id: 0,
                    component_id: 7,
                    point_mm: [0.133_333_333_333_333_33, 0.133_333_333_333_333_33, 0.0],
                    area_mm2: area
                },
                CoverageSample {
                    island_id: 11,
                    face_id: 0,
                    component_id: 7,
                    point_mm: [0.266_666_666_666_666_66, 0.066_666_666_666_666_67, 0.0],
                    area_mm2: area
                },
                CoverageSample {
                    island_id: 11,
                    face_id: 0,
                    component_id: 7,
                    point_mm: [0.266_666_666_666_666_66, 0.066_666_666_666_666_67, 0.0],
                    area_mm2: area
                },
                CoverageSample {
                    island_id: 11,
                    face_id: 1,
                    component_id: 8,
                    point_mm: [1.066_666_666_666_666_7, 0.066_666_666_666_666_67, 0.0],
                    area_mm2: 0.019_999_999_999_999_997
                },
                CoverageSample {
                    island_id: 11,
                    face_id: 1,
                    component_id: 8,
                    point_mm: [1.066_666_666_666_666_7, 0.266_666_666_666_666_66, 0.0],
                    area_mm2: 0.019_999_999_999_999_997
                },
                CoverageSample {
                    island_id: 11,
                    face_id: 1,
                    component_id: 8,
                    point_mm: [1.133_333_333_333_333_3, 0.133_333_333_333_333_33, 0.0],
                    area_mm2: 0.019_999_999_999_999_997
                },
                CoverageSample {
                    island_id: 11,
                    face_id: 1,
                    component_id: 8,
                    point_mm: [1.266_666_666_666_666_6, 0.066_666_666_666_666_67, 0.0],
                    area_mm2: 0.019_999_999_999_999_997
                },
            ]
        );
    }

    #[test]
    fn coverage_working_window_one_is_exact_and_zero_is_refused() {
        let mesh = two_triangles();
        let islands = [SupportIsland {
            id: 11,
            face_ids: vec![0],
        }];
        let mut config = ContactSelectionConfig::current(None, 4, 1, 1024 * 1024)
            .expect("registered coverage config");
        config.coverage_samples_max_working_cells = 1;
        let (_, high_water) =
            build_dyadic_coverage_samples_with_high_water(&mesh, &islands, &[7, 8], &config)
                .expect("one-cell coverage window");
        assert_eq!(high_water, 1);
        config.coverage_samples_max_working_cells = 0;
        assert_eq!(
            build_dyadic_coverage_samples(&mesh, &islands, &[7, 8], &config),
            Err(ContactSelectionError::InvalidResourceRequest)
        );
    }

    #[test]
    fn positive_edge_collinear_coverage_face_retains_zero_area_samples() {
        let mesh = Mesh::new(
            vec![[0.0, 0.0, 0.0], [0.4, 0.0, 0.0], [0.8, 0.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("finite collinear fixture");
        let rows = build_dyadic_coverage_samples(
            &mesh,
            &[SupportIsland {
                id: 3,
                face_ids: vec![0],
            }],
            &[4],
            &ContactSelectionConfig::current(None, 16, 1, 1024 * 1024)
                .expect("registered coverage config"),
        )
        .expect("coverage accepts positive-edge collinear source face");
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|row| row.area_mm2 == 0.0));
    }
}
