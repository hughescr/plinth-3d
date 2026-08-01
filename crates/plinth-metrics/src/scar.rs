//! Independent scar metrics over type-7/type-1 contact pairs written to disk.
//!
//! The scorer deliberately reconstructs contact geometry from the emitted
//! ChituBox records.  It does not inspect a producer-side contact list, does
//! not use support meshes as a proxy for scars, and does not depend on the
//! producer's delicacy implementation.  The public operation is whole-artifact
//! and ordered: gather one row per model, call once, then scatter the rows.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use chitubox_io::SegmentType;
use plinth_core::{m0_constants, MetricId, MetricSet, MetricValue};
use thiserror::Error;

use crate::{
    closest_points, contains_points, ray_intersections, DiskArtifact, DiskSupport, Mesh,
    MetricsError, Ray, Vec3,
};

/// Fixed M1 scar metric order from `04-migration-plan.md` §3.
pub const SCAR_METRIC_ORDER: [&str; 3] = ["scar_area", "scar_worst_patch", "scar_weighted"];

const CONTACT_PAIR_TOL_MM: f64 = 1.0e-5;
const RAY_START_TOL_MM: f64 = 1.0e-9;

/// One ordered model input to [`scar_metric_rows`].
///
/// The row deliberately carries no producer contact list: the emitted adjacent
/// type-7/type-1 pair is the scar-contact authority.  `source_index` makes a
/// stale caller unable to silently score a different disk model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScarMetricInput {
    pub source_index: usize,
}

/// One M1 scar row in the normative fixed order.
#[derive(Clone, Debug, PartialEq)]
pub struct ScarMetricRow {
    pub source_index: usize,
    values: [MetricValue; SCAR_METRIC_ORDER.len()],
}

impl ScarMetricRow {
    /// Entries in stable M1 table order.
    #[must_use]
    pub fn entries(&self) -> [(MetricId, MetricValue); SCAR_METRIC_ORDER.len()] {
        std::array::from_fn(|index| (MetricId::from(SCAR_METRIC_ORDER[index]), self.values[index]))
    }

    /// Converts this ordered transport row into core's passive DTO.
    #[must_use]
    pub fn metric_set(&self) -> MetricSet {
        MetricSet::new(self.entries())
    }
}

/// Fail-closed structural and numerical failures for independent scar metrics.
#[derive(Debug, Error)]
pub enum ScarMetricError {
    #[error(
        "scar metric input count {actual} does not match disk artifact model count {expected}"
    )]
    InputCount { expected: usize, actual: usize },
    #[error("scar metric input row {row} names source model {actual}, expected {expected}")]
    InputSourceIndex {
        row: usize,
        expected: usize,
        actual: usize,
    },
    #[error(
        "model {source_index} type-7 record {tip_index} is not followed by a type-1 contact taper"
    )]
    OrphanTip {
        source_index: usize,
        tip_index: usize,
    },
    #[error("model {source_index} type-1 record {taper_index} has no immediately preceding type-7 contact tip")]
    OrphanTaper {
        source_index: usize,
        taper_index: usize,
    },
    #[error("model {source_index} contact pair type-7 record {tip_index}/type-1 record {taper_index} has invalid sphere radii")]
    InvalidTipSphere {
        source_index: usize,
        tip_index: usize,
        taper_index: usize,
    },
    #[error("model {source_index} contact pair type-7 record {tip_index}/type-1 record {taper_index} has a zero-length taper axis")]
    ZeroTaperAxis {
        source_index: usize,
        tip_index: usize,
        taper_index: usize,
    },
    #[error("model {source_index} contact pair type-7 record {tip_index}/type-1 record {taper_index} does not start at the tip sphere centre")]
    ContactPairGeometry {
        source_index: usize,
        tip_index: usize,
        taper_index: usize,
    },
    #[error("model {source_index} contact pair type-7 record {tip_index}/type-1 record {taper_index} has invalid taper radii")]
    InvalidTaperRadius {
        source_index: usize,
        tip_index: usize,
        taper_index: usize,
    },
    #[error("model {source_index} contact {contact} produced non-finite {field}")]
    NonFinite {
        source_index: usize,
        contact: usize,
        field: &'static str,
    },
    #[error("model {source_index} arithmetic overflow while computing {metric}")]
    ArithmeticOverflow {
        source_index: usize,
        metric: &'static str,
    },
    #[error("M0 constants registry is unavailable: {detail}")]
    Constants { detail: String },
    #[error(transparent)]
    Geometry(#[from] MetricsError),
}

#[derive(Clone, Copy, Debug)]
struct ContactPair {
    /// The planned deepest contact point: the inner pole of the type-7 ball.
    contact_point: Vec3,
    /// From the deepest point through solid, opposite the free taper direction.
    inward_axis: Vec3,
    ball_radius_mm: f64,
    neck_radius_mm: f64,
    root_radius_mm: f64,
    member_length_mm: f64,
}

#[derive(Clone, Copy, Debug)]
struct Face {
    centroid: Vec3,
    normal: Vec3,
}

#[derive(Clone, Debug)]
struct FaceTable {
    faces: Vec<Face>,
    /// Exact f64 coordinate keys preserve the disk soup's signed-zero identity.
    neighbours: Vec<Vec<usize>>,
    thinness: Vec<f64>,
}

#[derive(Clone, Copy, Debug)]
struct ContactScar {
    surface_point: Vec3,
    area_mm2: f64,
    equivalent_radius_mm: f64,
    delicacy: f64,
}

/// Computes all three M1 scar metrics for every model in one opened artifact.
///
/// Contact identity is the exact adjacent `[type 7, type 1]` pair emitted by
/// `member_chain_specs`: type 7 owns the contact sphere; type 1 owns the taper
/// and therefore its neck/root radii.  Type 9 is a joint sphere, never a
/// contact.  A missing or ambiguous mapping is a named error rather than a
/// zero scar.  The patch quantity follows `SPEC-EMBED-POLICY`'s
/// SCAR-AREA-CAP formula verbatim; overlapping equivalent patch discs form one
/// connected scar patch and the largest component is `scar_worst_patch`.
pub fn scar_metric_rows(
    artifact: &DiskArtifact,
    inputs: &[ScarMetricInput],
) -> Result<Vec<ScarMetricRow>, ScarMetricError> {
    if artifact.models().len() != inputs.len() {
        return Err(ScarMetricError::InputCount {
            expected: artifact.models().len(),
            actual: inputs.len(),
        });
    }
    let constants = m0_constants().map_err(|error| ScarMetricError::Constants {
        detail: error.to_string(),
    })?;
    let settings = constants.placeholders();
    let curvature_weight = settings.delicacy_curvature_weight().diagnostic().get();
    let thinness_weight = settings.delicacy_thinness_weight().diagnostic().get();
    let thin_feature_mm = settings.thin_feature_mm().diagnostic().get();
    let eps_cos = settings.eps_cos().diagnostic().get();
    let tol_mm = settings.tol_mm().diagnostic().get();

    artifact
        .models()
        .iter()
        .zip(inputs)
        .enumerate()
        .map(|(row, (model, input))| {
            let source_index = model.source_index();
            if input.source_index != source_index {
                return Err(ScarMetricError::InputSourceIndex {
                    row,
                    expected: source_index,
                    actual: input.source_index,
                });
            }
            let pairs = contact_pairs(source_index, model.supports())?;
            let scars = contact_scars(
                source_index,
                model.mesh(),
                &pairs,
                ScarSettings {
                    curvature_weight,
                    thinness_weight,
                    thin_feature_mm,
                    eps_cos,
                    tol_mm,
                },
            )?;
            let scar_area = checked_sum(
                source_index,
                "scar_area",
                scars.iter().map(|scar| scar.area_mm2),
            )?;
            let scar_weighted = checked_sum(
                source_index,
                "scar_weighted",
                scars.iter().map(|scar| scar.area_mm2 * scar.delicacy),
            )?;
            let scar_worst_patch = largest_patch(source_index, &scars)?;
            Ok(ScarMetricRow {
                source_index,
                values: [
                    metric_value(source_index, "scar_area", scar_area)?,
                    metric_value(source_index, "scar_worst_patch", scar_worst_patch)?,
                    metric_value(source_index, "scar_weighted", scar_weighted)?,
                ],
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
struct ScarSettings {
    curvature_weight: f64,
    thinness_weight: f64,
    thin_feature_mm: f64,
    eps_cos: f64,
    tol_mm: f64,
}

fn contact_pairs(
    source_index: usize,
    supports: &[DiskSupport],
) -> Result<Vec<ContactPair>, ScarMetricError> {
    let mut pairs = Vec::new();
    for (index, support) in supports.iter().enumerate() {
        if support.segment_type() == SegmentType::TIP {
            let Some(taper) = supports.get(index + 1) else {
                return Err(ScarMetricError::OrphanTip {
                    source_index,
                    tip_index: index,
                });
            };
            if taper.segment_type() != SegmentType::TAPER {
                return Err(ScarMetricError::OrphanTip {
                    source_index,
                    tip_index: index,
                });
            }
            pairs.push(validate_contact_pair(source_index, index, support, taper)?);
        } else if support.segment_type() == SegmentType::TAPER
            && (index == 0 || supports[index - 1].segment_type() != SegmentType::TIP)
        {
            return Err(ScarMetricError::OrphanTaper {
                source_index,
                taper_index: index,
            });
        }
    }
    Ok(pairs)
}

fn validate_contact_pair(
    source_index: usize,
    tip_index: usize,
    tip: &DiskSupport,
    taper: &DiskSupport,
) -> Result<ContactPair, ScarMetricError> {
    let taper_index = tip_index + 1;
    let (tip_start_radius, tip_end_radius) = tip.radii();
    if !(tip_start_radius.is_finite() && tip_end_radius.is_finite())
        || tip_start_radius <= 0.0
        || tip_end_radius <= 0.0
        || (tip_start_radius - tip_end_radius).abs() > CONTACT_PAIR_TOL_MM
    {
        return Err(ScarMetricError::InvalidTipSphere {
            source_index,
            tip_index,
            taper_index,
        });
    }
    let contact_point = tip.start();
    let sphere_center = midpoint(tip.start(), tip.end());
    if distance(sphere_center, taper.start()) > CONTACT_PAIR_TOL_MM {
        return Err(ScarMetricError::ContactPairGeometry {
            source_index,
            tip_index,
            taper_index,
        });
    }
    let taper_direction = sub(taper.end(), contact_point);
    let member_length_mm = length(taper_direction);
    if !member_length_mm.is_finite() || member_length_mm <= 0.0 {
        return Err(ScarMetricError::ZeroTaperAxis {
            source_index,
            tip_index,
            taper_index,
        });
    }
    let (neck_radius_mm, root_radius_mm) = taper.radii();
    if !(neck_radius_mm.is_finite() && root_radius_mm.is_finite())
        || neck_radius_mm <= 0.0
        || root_radius_mm <= 0.0
    {
        return Err(ScarMetricError::InvalidTaperRadius {
            source_index,
            tip_index,
            taper_index,
        });
    }
    Ok(ContactPair {
        contact_point,
        inward_axis: scale(taper_direction, -1.0 / member_length_mm),
        ball_radius_mm: tip_start_radius,
        neck_radius_mm,
        root_radius_mm,
        member_length_mm,
    })
}

fn contact_scars(
    source_index: usize,
    mesh: &Mesh,
    pairs: &[ContactPair],
    settings: ScarSettings,
) -> Result<Vec<ContactScar>, ScarMetricError> {
    if pairs.is_empty() {
        return Ok(Vec::new());
    }
    // Gather all contact queries, perform each geometric operation once for
    // the model, then scatter ordered answers.  There is no per-contact public
    // query path.
    let points = pairs
        .iter()
        .map(|pair| pair.contact_point)
        .collect::<Vec<_>>();
    let nearest = closest_points(mesh, &points)?;
    let inside = contains_points(mesh, &points)?;
    let rays = pairs
        .iter()
        .map(|pair| Ray {
            origin: pair.contact_point,
            direction: pair.inward_axis,
        })
        .collect::<Vec<_>>();
    let hits = ray_intersections(mesh, &rays)?;
    let faces = FaceTable::build(source_index, mesh, settings.thin_feature_mm)?;

    pairs
        .iter()
        .zip(nearest.iter().zip(inside.iter().zip(hits.iter())))
        .enumerate()
        .map(|(contact, (pair, (nearest, (inside, hits))))| {
            let exit = inside.then(|| hits.iter().find(|hit| hit.distance > RAY_START_TOL_MM));
            let (interval_length, cosine) = match exit.flatten() {
                Some(hit) => {
                    let normal = faces.faces[hit.triangle_index].normal;
                    (hit.distance, dot(pair.inward_axis, normal).abs())
                }
                None => (0.0, 0.0),
            };
            let radius_at_exit = pair.neck_radius_mm
                + (pair.root_radius_mm - pair.neck_radius_mm)
                    * (interval_length / pair.member_length_mm).clamp(0.0, 1.0);
            let shaft_area = if interval_length <= settings.tol_mm {
                0.0
            } else {
                (PI * radius_at_exit.powi(2) / cosine.max(settings.eps_cos))
                    .min(2.0 * radius_at_exit * interval_length)
            };
            let surface_distance = nearest.squared_distance.sqrt();
            let ball_area = PI * (pair.ball_radius_mm.powi(2) - surface_distance.powi(2)).max(0.0);
            let area_mm2 = shaft_area + ball_area;
            let equivalent_radius_mm = (area_mm2 / PI).sqrt();
            let delicacy = faces.delicacy(nearest.triangle_index, settings);
            for (field, value) in [
                ("interval length", interval_length),
                ("radius at exit", radius_at_exit),
                ("shaft area", shaft_area),
                ("ball area", ball_area),
                ("patch area", area_mm2),
                ("patch radius", equivalent_radius_mm),
                ("delicacy", delicacy),
            ] {
                if !value.is_finite() || value < 0.0 {
                    return Err(ScarMetricError::NonFinite {
                        source_index,
                        contact,
                        field,
                    });
                }
            }
            Ok(ContactScar {
                surface_point: nearest.point,
                area_mm2,
                equivalent_radius_mm,
                delicacy,
            })
        })
        .collect()
}

impl FaceTable {
    fn build(
        source_index: usize,
        mesh: &Mesh,
        thin_feature_mm: f64,
    ) -> Result<Self, ScarMetricError> {
        let mut faces = Vec::with_capacity(mesh.triangles().len());
        let mut by_vertex: BTreeMap<(u64, u64, u64), Vec<usize>> = BTreeMap::new();
        for (index, triangle) in mesh.triangles().iter().enumerate() {
            let [a, b, c] = *triangle;
            let pa = mesh.vertices()[a];
            let pb = mesh.vertices()[b];
            let pc = mesh.vertices()[c];
            let normal =
                unit(cross(sub(pb, pa), sub(pc, pa))).ok_or(ScarMetricError::NonFinite {
                    source_index,
                    contact: index,
                    field: "face normal",
                })?;
            faces.push(Face {
                centroid: scale(add(add(pa, pb), pc), 1.0 / 3.0),
                normal,
            });
            for point in [pa, pb, pc] {
                by_vertex.entry(vector_key(point)).or_default().push(index);
            }
        }
        let mut neighbours = vec![Vec::new(); faces.len()];
        for connected in by_vertex.values() {
            for &face in connected {
                neighbours[face].extend(connected.iter().copied().filter(|other| *other != face));
            }
        }
        for connected in &mut neighbours {
            connected.sort_unstable();
            connected.dedup();
        }
        // The frozen Python proxy searches face centres only out to four
        // thin-feature widths.  This deterministic uniform grid implements
        // that same bounded opposite-normal search without a producer KD-tree
        // and is built once per model, rather than once per contact.
        let cell_width = 4.0 * thin_feature_mm;
        let mut cells: BTreeMap<(i64, i64, i64), Vec<usize>> = BTreeMap::new();
        for (index, face) in faces.iter().enumerate() {
            cells
                .entry(
                    grid_cell(face.centroid, cell_width).ok_or(ScarMetricError::NonFinite {
                        source_index,
                        contact: index,
                        field: "face grid coordinate",
                    })?,
                )
                .or_default()
                .push(index);
        }
        let mut thinness = Vec::with_capacity(faces.len());
        for (index, face) in faces.iter().enumerate() {
            let cell = grid_cell(face.centroid, cell_width).ok_or(ScarMetricError::NonFinite {
                source_index,
                contact: index,
                field: "face grid coordinate",
            })?;
            let mut nearest = f64::INFINITY;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(candidates) =
                            cells.get(&(cell.0 + dx, cell.1 + dy, cell.2 + dz))
                        {
                            for &candidate in candidates {
                                let other = faces[candidate];
                                if dot(face.normal, other.normal) < -0.3 {
                                    nearest = nearest.min(distance(face.centroid, other.centroid));
                                }
                            }
                        }
                    }
                }
            }
            thinness.push(nearest);
        }
        Ok(Self {
            faces,
            neighbours,
            thinness,
        })
    }

    fn delicacy(&self, face_index: usize, settings: ScarSettings) -> f64 {
        let face = self.faces[face_index];
        let curvature = match self.neighbours[face_index].as_slice() {
            [] => 0.0,
            neighbours => {
                neighbours
                    .iter()
                    .map(|&other| {
                        (1.0 - dot(face.normal, self.faces[other].normal)).clamp(0.0, 2.0) * 0.5
                    })
                    .sum::<f64>()
                    / neighbours.len() as f64
            }
        };
        // The table is built once.  A deterministic whole-table scan is slow
        // by design but avoids borrowing a producer KD-tree or delicacy field.
        let thinness = (1.0 - self.thinness[face_index] / settings.thin_feature_mm).clamp(0.0, 1.0);
        (settings.curvature_weight * curvature + settings.thinness_weight * thinness)
            .clamp(0.0, 1.0)
    }
}

fn largest_patch(source_index: usize, scars: &[ContactScar]) -> Result<f64, ScarMetricError> {
    if scars.is_empty() {
        return Ok(0.0);
    }
    let mut parents = (0..scars.len()).collect::<Vec<_>>();
    for left in 0..scars.len() {
        for right in left + 1..scars.len() {
            if distance(scars[left].surface_point, scars[right].surface_point)
                <= scars[left].equivalent_radius_mm + scars[right].equivalent_radius_mm
            {
                union(&mut parents, left, right);
            }
        }
    }
    let mut areas: BTreeMap<usize, f64> = BTreeMap::new();
    for (index, scar) in scars.iter().enumerate() {
        let root = find(&mut parents, index);
        *areas.entry(root).or_insert(0.0) += scar.area_mm2;
    }
    checked_sum(source_index, "scar_worst_patch", areas.values().copied())?;
    areas
        .values()
        .copied()
        .max_by(f64::total_cmp)
        .ok_or(ScarMetricError::ArithmeticOverflow {
            source_index,
            metric: "scar_worst_patch",
        })
}

fn find(parents: &mut [usize], index: usize) -> usize {
    if parents[index] != index {
        parents[index] = find(parents, parents[index]);
    }
    parents[index]
}

fn union(parents: &mut [usize], left: usize, right: usize) {
    let left_root = find(parents, left);
    let right_root = find(parents, right);
    if left_root != right_root {
        // Lowest stable index wins, independent of input traversal accidents.
        if left_root < right_root {
            parents[right_root] = left_root;
        } else {
            parents[left_root] = right_root;
        }
    }
}

fn checked_sum(
    source_index: usize,
    metric: &'static str,
    values: impl IntoIterator<Item = f64>,
) -> Result<f64, ScarMetricError> {
    let sum = values.into_iter().try_fold(0.0, |sum, value| {
        let next = sum + value;
        next.is_finite().then_some(next)
    });
    sum.ok_or(ScarMetricError::ArithmeticOverflow {
        source_index,
        metric,
    })
}

fn metric_value(
    source_index: usize,
    metric: &'static str,
    value: f64,
) -> Result<MetricValue, ScarMetricError> {
    MetricValue::new(value).map_err(|_| ScarMetricError::ArithmeticOverflow {
        source_index,
        metric,
    })
}

fn vector_key(value: Vec3) -> (u64, u64, u64) {
    (value[0].to_bits(), value[1].to_bits(), value[2].to_bits())
}
fn grid_cell(value: Vec3, width: f64) -> Option<(i64, i64, i64)> {
    let coordinate = |component: f64| {
        let scaled = (component / width).floor();
        (scaled.is_finite() && scaled >= i64::MIN as f64 && scaled <= i64::MAX as f64)
            .then_some(scaled as i64)
    };
    Some((
        coordinate(value[0])?,
        coordinate(value[1])?,
        coordinate(value[2])?,
    ))
}
fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: Vec3, factor: f64) -> Vec3 {
    [a[0] * factor, a[1] * factor, a[2] * factor]
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn length(value: Vec3) -> f64 {
    dot(value, value).sqrt()
}
fn distance(a: Vec3, b: Vec3) -> f64 {
    length(sub(a, b))
}
fn midpoint(a: Vec3, b: Vec3) -> Vec3 {
    scale(add(a, b), 0.5)
}
fn unit(value: Vec3) -> Option<Vec3> {
    let length = length(value);
    (length.is_finite() && length > 0.0).then(|| scale(value, 1.0 / length))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> ScarSettings {
        ScarSettings {
            curvature_weight: 0.6,
            thinness_weight: 0.4,
            thin_feature_mm: 1.5,
            eps_cos: 1.0e-9,
            tol_mm: 0.05,
        }
    }

    #[test]
    fn delicacy_uses_the_authoritative_point_six_point_four_blend_and_one_point_five_boundary() {
        // Two adjacent right-angle faces give curvature = (1 - 0) / 2 = .5
        // at face zero and no opposite-facing match, hence .6 * .5 = .3.
        let curved = FaceTable {
            faces: vec![
                Face {
                    centroid: [0.0; 3],
                    normal: [0.0, 0.0, 1.0],
                },
                Face {
                    centroid: [0.0; 3],
                    normal: [1.0, 0.0, 0.0],
                },
            ],
            neighbours: vec![vec![1], vec![0]],
            thinness: vec![f64::INFINITY, f64::INFINITY],
        };
        assert!((curved.delicacy(0, settings()) - 0.3).abs() < 1.0e-12);

        // The score saturates at zero exactly at the 1.5 mm thin-feature
        // boundary, rather than treating it as a radius or a diameter.
        let at_boundary = FaceTable {
            faces: vec![Face {
                centroid: [0.0; 3],
                normal: [0.0, 0.0, 1.0],
            }],
            neighbours: vec![vec![]],
            thinness: vec![1.5],
        };
        assert_eq!(at_boundary.delicacy(0, settings()), 0.0);

        let mixed = FaceTable {
            faces: vec![
                Face {
                    centroid: [0.0; 3],
                    normal: [0.0, 0.0, 1.0],
                },
                Face {
                    centroid: [0.0; 3],
                    normal: [1.0, 0.0, 0.0],
                },
            ],
            neighbours: vec![vec![1], vec![0]],
            thinness: vec![0.75, f64::INFINITY],
        };
        // .6 * .5 curvature + .4 * .5 thinness.
        assert!((mixed.delicacy(0, settings()) - 0.5).abs() < 1.0e-12);
    }

    #[test]
    fn union_find_clustering_is_order_deterministic() {
        let scars = vec![
            ContactScar {
                surface_point: [0.0; 3],
                area_mm2: 1.0,
                equivalent_radius_mm: 1.0,
                delicacy: 0.0,
            },
            ContactScar {
                surface_point: [1.0, 0.0, 0.0],
                area_mm2: 2.0,
                equivalent_radius_mm: 1.0,
                delicacy: 0.0,
            },
            ContactScar {
                surface_point: [9.0, 0.0, 0.0],
                area_mm2: 4.0,
                equivalent_radius_mm: 1.0,
                delicacy: 0.0,
            },
        ];
        assert_eq!(largest_patch(0, &scars).unwrap(), 4.0);
        let reordered = vec![scars[2], scars[1], scars[0]];
        assert_eq!(largest_patch(0, &reordered).unwrap(), 4.0);
    }
}
