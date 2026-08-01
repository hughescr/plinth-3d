//! Independent scar audit of the support records stored in a `.chitubox` file.
//!
//! This module intentionally owns both its contact reconstruction and its
//! delicacy field.  In particular it has no producer-side contact list and no
//! dependency on the mesh or verify crates.  Queries are batch-shaped: gather
//! all model points first, make one query per primitive, then scatter results.

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

/// One ordered model input to [`scar_metric_rows`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScarMetricInput {
    pub source_index: usize,
}

/// One M1 scar row in normative table order.
#[derive(Clone, Debug, PartialEq)]
pub struct ScarMetricRow {
    pub source_index: usize,
    values: [MetricValue; SCAR_METRIC_ORDER.len()],
}

impl ScarMetricRow {
    #[must_use]
    pub fn entries(&self) -> [(MetricId, MetricValue); SCAR_METRIC_ORDER.len()] {
        std::array::from_fn(|index| (MetricId::from(SCAR_METRIC_ORDER[index]), self.values[index]))
    }

    #[must_use]
    pub fn metric_set(&self) -> MetricSet {
        MetricSet::new(self.entries())
    }
}

/// Fail-closed errors from the artifact-side reconstruction and audit.
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
    #[error("model {source_index} record {record} has non-positive or unequal sphere radii")]
    InvalidSphereRadii { source_index: usize, record: usize },
    #[error("model {source_index} record {record} claims a sphere but its triangle soup is not a sphere")]
    NonSphereMesh { source_index: usize, record: usize },
    #[error("model {source_index} type-9 record {record} is not a vertical two-pole ball")]
    InvalidFootBallAxis { source_index: usize, record: usize },
    #[error("model {source_index} type-7 record {record} has no unique type-1 stub member")]
    UnmatchedTip { source_index: usize, record: usize },
    #[error("model {source_index} type-1 record {record} has no unique stub or foot contact")]
    UnmatchedTaper { source_index: usize, record: usize },
    #[error("model {source_index} record {record} has ambiguous {class} contact identity")]
    AmbiguousContact {
        source_index: usize,
        record: usize,
        class: &'static str,
    },
    #[error("model {source_index} type-1 record {record} matches both the stub and foot classes")]
    BothContactClasses { source_index: usize, record: usize },
    #[error("model {source_index} type-7 record {tip} and type-1 record {member} have invalid stub geometry: {detail}")]
    StubGeometry {
        source_index: usize,
        tip: usize,
        member: usize,
        detail: &'static str,
    },
    #[error("model {source_index} member record {member} and type-9 record {ball} have invalid foot geometry: {detail}")]
    FootGeometry {
        source_index: usize,
        member: usize,
        ball: usize,
        detail: &'static str,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContactClass {
    Stub,
    Foot,
}

#[derive(Clone, Copy, Debug)]
struct Contact {
    class: ContactClass,
    member: usize,
    ball: usize,
    /// Designed contact endpoint, followed by the member's opposite endpoint.
    anchor: Vec3,
    other: Vec3,
    radius_at_anchor: f64,
    radius_at_other: f64,
    ball_radius: f64,
}

#[derive(Clone, Copy)]
struct Settings {
    curvature_weight: f64,
    thinness_weight: f64,
    thin_feature_mm: f64,
    eps_cos: f64,
    tol_mm: f64,
    stub_factor: f64,
    foot_factor: f64,
}

#[derive(Clone, Copy)]
struct Scar {
    area: f64,
    reference: Vec3,
}

/// Computes all three M1 scar metrics for a whole reopened artifact.
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
    let values = constants.placeholders();
    let settings = Settings {
        curvature_weight: values.delicacy_curvature_weight().diagnostic().get(),
        thinness_weight: values.delicacy_thinness_weight().diagnostic().get(),
        thin_feature_mm: values.thin_feature_mm().diagnostic().get(),
        eps_cos: values.eps_cos().diagnostic().get(),
        tol_mm: values.tol_mm().diagnostic().get(),
        stub_factor: values.ball_factor_stub().diagnostic().get(),
        foot_factor: values.ball_factor_foot().diagnostic().get(),
    };

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
            let contacts = reconstruct_contacts(source_index, model.supports(), settings)?;
            let scars = contact_scars(source_index, model.mesh(), &contacts, settings)?;
            let scar_area = checked_sum(
                source_index,
                "scar_area",
                scars.iter().map(|scar| scar.area),
            )?;
            let scar_worst_patch = scars.iter().map(|scar| scar.area).fold(0.0_f64, f64::max);
            let weights = if scars.is_empty() {
                Vec::new()
            } else {
                DelicacyField::build(source_index, model.mesh(), settings)?.at_points(
                    source_index,
                    &scars.iter().map(|scar| scar.reference).collect::<Vec<_>>(),
                )?
            };
            let scar_weighted = checked_sum(
                source_index,
                "scar_weighted",
                scars
                    .iter()
                    .zip(weights)
                    .map(|(scar, weight)| scar.area * weight),
            )?;
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

fn reconstruct_contacts(
    source_index: usize,
    supports: &[DiskSupport],
    settings: Settings,
) -> Result<Vec<Contact>, ScarMetricError> {
    let mut tips = Vec::new();
    let mut tapers = Vec::new();
    let mut joiners = Vec::new();
    let mut balls = Vec::new();
    for (index, support) in supports.iter().enumerate() {
        match support.segment_type() {
            SegmentType::TIP => {
                sphere(source_index, index, support)?;
                tips.push(index);
            }
            SegmentType::TAPER => tapers.push(index),
            SegmentType::JOINER => joiners.push(index),
            SegmentType::SHAFT => {
                sphere(source_index, index, support)?;
                foot_ball(source_index, index, support)?;
                balls.push(index);
            }
            _ => {}
        }
    }

    // Form the complete bipartite candidate set before resolving identity;
    // adjacency is deliberately not a contact rule.
    let mut stub_by_tip: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut stub_by_taper: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &tip in &tips {
        for &taper in &tapers {
            if stub_candidate(supports[tip].clone(), supports[taper].clone(), settings)? {
                stub_by_tip.entry(tip).or_default().push(taper);
                stub_by_taper.entry(taper).or_default().push(tip);
            }
        }
    }
    let mut contacts = Vec::new();
    let mut stub_tapers = BTreeMap::new();
    for tip in tips {
        let matches = stub_by_tip.get(&tip).map_or(&[][..], Vec::as_slice);
        if matches.len() != 1 {
            return Err(if matches.is_empty() {
                ScarMetricError::UnmatchedTip {
                    source_index,
                    record: tip,
                }
            } else {
                ScarMetricError::AmbiguousContact {
                    source_index,
                    record: tip,
                    class: "stub",
                }
            });
        }
        let taper = matches[0];
        if stub_by_taper
            .get(&taper)
            .is_none_or(|other| other.len() != 1)
        {
            return Err(ScarMetricError::AmbiguousContact {
                source_index,
                record: taper,
                class: "stub",
            });
        }
        stub_tapers.insert(taper, tip);
        contacts.push(stub_contact(source_index, tip, taper, supports, settings)?);
    }

    #[derive(Clone, Copy)]
    struct FootCandidate {
        member: usize,
        ball: usize,
        endpoint: bool,
    }
    let mut foot_candidates = Vec::new();
    for &member in tapers.iter().chain(joiners.iter()) {
        let support = &supports[member];
        let (r0, r1) = support.radii();
        if support.segment_type() == SegmentType::JOINER && !f32_close(r0, r1) {
            continue;
        }
        for endpoint in [false, true] {
            let (point, radius) = if endpoint {
                (support.end(), r1)
            } else {
                (support.start(), r0)
            };
            if !positive(radius) {
                continue;
            }
            for &ball in &balls {
                if ball <= member {
                    continue;
                }
                let centre = midpoint(supports[ball].start(), supports[ball].end());
                let (ball_radius, _) = supports[ball].radii();
                if near(point, centre) && f32_close(ball_radius, settings.foot_factor * radius) {
                    foot_candidates.push(FootCandidate {
                        member,
                        ball,
                        endpoint,
                    });
                }
            }
        }
    }
    let mut foot_by_member: BTreeMap<usize, Vec<FootCandidate>> = BTreeMap::new();
    let mut foot_by_ball: BTreeMap<usize, Vec<FootCandidate>> = BTreeMap::new();
    for candidate in foot_candidates {
        foot_by_member
            .entry(candidate.member)
            .or_default()
            .push(candidate);
        foot_by_ball
            .entry(candidate.ball)
            .or_default()
            .push(candidate);
    }
    for taper in tapers {
        let feet = foot_by_member.get(&taper).map_or(&[][..], Vec::as_slice);
        if stub_tapers.contains_key(&taper) {
            if !feet.is_empty() {
                return Err(ScarMetricError::BothContactClasses {
                    source_index,
                    record: taper,
                });
            }
        } else if feet.len() != 1 {
            return Err(if feet.is_empty() {
                ScarMetricError::UnmatchedTaper {
                    source_index,
                    record: taper,
                }
            } else {
                ScarMetricError::AmbiguousContact {
                    source_index,
                    record: taper,
                    class: "foot",
                }
            });
        } else {
            let candidate = feet[0];
            if foot_by_ball
                .get(&candidate.ball)
                .is_none_or(|other| other.len() != 1)
            {
                return Err(ScarMetricError::AmbiguousContact {
                    source_index,
                    record: candidate.ball,
                    class: "foot",
                });
            }
            contacts.push(foot_contact(
                source_index,
                candidate.member,
                candidate.ball,
                candidate.endpoint,
                supports,
                settings,
            )?);
        }
    }
    for joiner in joiners {
        let feet = foot_by_member.get(&joiner).map_or(&[][..], Vec::as_slice);
        if feet.len() > 1 {
            return Err(ScarMetricError::AmbiguousContact {
                source_index,
                record: joiner,
                class: "foot",
            });
        }
        if let Some(candidate) = feet.first().copied() {
            if foot_by_ball
                .get(&candidate.ball)
                .is_none_or(|other| other.len() != 1)
            {
                return Err(ScarMetricError::AmbiguousContact {
                    source_index,
                    record: candidate.ball,
                    class: "foot",
                });
            }
            contacts.push(foot_contact(
                source_index,
                candidate.member,
                candidate.ball,
                candidate.endpoint,
                supports,
                settings,
            )?);
        }
    }
    contacts.sort_by_key(|contact| (contact.member, contact.ball));
    Ok(contacts)
}

fn sphere(
    source_index: usize,
    record: usize,
    support: &DiskSupport,
) -> Result<(Vec3, f64), ScarMetricError> {
    let (r0, r1) = support.radii();
    if !positive(r0) || !f32_close(r0, r1) {
        return Err(ScarMetricError::InvalidSphereRadii {
            source_index,
            record,
        });
    }
    let centre = midpoint(support.start(), support.end());
    let radius = r0;
    if !f32_close(distance(support.start(), support.end()), 2.0 * radius)
        || support
            .mesh()
            .vertices()
            .iter()
            .any(|vertex| !f32_close(distance(*vertex, centre), radius))
    {
        return Err(ScarMetricError::NonSphereMesh {
            source_index,
            record,
        });
    }
    Ok((centre, radius))
}

fn foot_ball(
    source_index: usize,
    record: usize,
    support: &DiskSupport,
) -> Result<(), ScarMetricError> {
    let (radius, _) = support.radii();
    let start = support.start();
    let end = support.end();
    if !f32_close(start[0], end[0])
        || !f32_close(start[1], end[1])
        || start[2] <= end[2]
        || !f32_close(start[2] - end[2], 2.0 * radius)
    {
        return Err(ScarMetricError::InvalidFootBallAxis {
            source_index,
            record,
        });
    }
    Ok(())
}

fn stub_candidate(
    tip: DiskSupport,
    taper: DiskSupport,
    settings: Settings,
) -> Result<bool, ScarMetricError> {
    let (centre, ball_radius) = sphere(usize::MAX, usize::MAX, &tip)?;
    let (r0, r1) = taper.radii();
    if !positive(r0) || !positive(r1) || !near(taper.start(), centre) {
        return Ok(false);
    }
    let axis = sub(taper.end(), taper.start());
    let poles = sub(tip.end(), tip.start());
    if !positive(length(axis)) || !positive(length(poles)) {
        return Ok(false);
    }
    Ok(f32_close(ball_radius, settings.stub_factor * r0) && parallel(axis, poles))
}

fn stub_contact(
    source_index: usize,
    tip_index: usize,
    taper_index: usize,
    supports: &[DiskSupport],
    settings: Settings,
) -> Result<Contact, ScarMetricError> {
    let tip = &supports[tip_index];
    let taper = &supports[taper_index];
    let (centre, ball_radius) = sphere(source_index, tip_index, tip)?;
    let (r0, r1) = taper.radii();
    if !positive(r0) || !positive(r1) {
        return Err(ScarMetricError::StubGeometry {
            source_index,
            tip: tip_index,
            member: taper_index,
            detail: "non-positive taper radius",
        });
    }
    if !near(taper.start(), centre) {
        return Err(ScarMetricError::StubGeometry {
            source_index,
            tip: tip_index,
            member: taper_index,
            detail: "taper does not start at ball centre",
        });
    }
    if !f32_close(ball_radius, settings.stub_factor * r0) {
        return Err(ScarMetricError::StubGeometry {
            source_index,
            tip: tip_index,
            member: taper_index,
            detail: "ball-factor mismatch",
        });
    }
    let outward = sub(taper.end(), centre);
    if !positive(length(outward)) || !parallel(outward, sub(tip.end(), tip.start())) {
        return Err(ScarMetricError::StubGeometry {
            source_index,
            tip: tip_index,
            member: taper_index,
            detail: "pole and taper axes are not aligned",
        });
    }
    let start_offset = sub(tip.start(), centre);
    let end_offset = sub(tip.end(), centre);
    let anchor = if dot(start_offset, outward) < dot(end_offset, outward) {
        tip.start()
    } else {
        tip.end()
    };
    Ok(Contact {
        class: ContactClass::Stub,
        member: taper_index,
        ball: tip_index,
        anchor,
        other: taper.end(),
        radius_at_anchor: r0,
        radius_at_other: r1,
        ball_radius,
    })
}

fn foot_contact(
    source_index: usize,
    member: usize,
    ball: usize,
    endpoint_is_end: bool,
    supports: &[DiskSupport],
    settings: Settings,
) -> Result<Contact, ScarMetricError> {
    let support = &supports[member];
    let ball_support = &supports[ball];
    let (centre, ball_radius) = sphere(source_index, ball, ball_support)?;
    foot_ball(source_index, ball, ball_support)?;
    let (r0, r1) = support.radii();
    if support.segment_type() == SegmentType::JOINER && !f32_close(r0, r1) {
        return Err(ScarMetricError::FootGeometry {
            source_index,
            member,
            ball,
            detail: "type-3 foot member is not constant-radius",
        });
    }
    let (endpoint, endpoint_radius, other, other_radius) = if endpoint_is_end {
        (support.end(), r1, support.start(), r0)
    } else {
        (support.start(), r0, support.end(), r1)
    };
    if !near(endpoint, centre) || !f32_close(ball_radius, settings.foot_factor * endpoint_radius) {
        return Err(ScarMetricError::FootGeometry {
            source_index,
            member,
            ball,
            detail: "endpoint or ball-factor mismatch",
        });
    }
    if !positive(length(sub(other, centre))) {
        return Err(ScarMetricError::FootGeometry {
            source_index,
            member,
            ball,
            detail: "zero-length member axis",
        });
    }
    Ok(Contact {
        class: ContactClass::Foot,
        member,
        ball,
        anchor: centre,
        other,
        radius_at_anchor: endpoint_radius,
        radius_at_other: other_radius,
        ball_radius,
    })
}

fn contact_scars(
    source_index: usize,
    mesh: &Mesh,
    contacts: &[Contact],
    settings: Settings,
) -> Result<Vec<Scar>, ScarMetricError> {
    if contacts.is_empty() {
        return Ok(Vec::new());
    }
    let anchors = contacts
        .iter()
        .map(|contact| contact.anchor)
        .collect::<Vec<_>>();
    let closest = closest_points(mesh, &anchors)?;
    let contained = contains_points(mesh, &anchors)?;
    let rays = contacts
        .iter()
        .map(|contact| Ray {
            origin: contact.anchor,
            direction: sub(contact.other, contact.anchor),
        })
        .collect::<Vec<_>>();
    let intersections = ray_intersections(mesh, &rays)?;
    let mut crossings = Vec::with_capacity(contacts.len());
    let mut probes = Vec::new();
    let mut probe_owner = Vec::new();
    for (index, (contact, hits)) in contacts.iter().zip(intersections).enumerate() {
        let member_length = length(sub(contact.other, contact.anchor));
        let mut unique = Vec::new();
        for hit in hits {
            if hit.distance <= member_length + settings.tol_mm
                && hit.distance > 0.0
                && unique
                    .last()
                    .is_none_or(|last: &crate::RayHit| !f32_close(last.distance, hit.distance))
            {
                unique.push(hit);
            }
        }
        for event in 0..unique.len() {
            let next = unique
                .get(event + 1)
                .map_or(member_length, |hit| hit.distance.min(member_length));
            if next > unique[event].distance {
                let distance = (unique[event].distance + next) * 0.5;
                probes.push(add(
                    contact.anchor,
                    scale(unit(sub(contact.other, contact.anchor)), distance),
                ));
                probe_owner.push((index, event));
            }
        }
        crossings.push(unique);
    }
    let probe_inside = contains_points(mesh, &probes)?;
    let mut after = vec![Vec::new(); contacts.len()];
    for ((contact, event), inside) in probe_owner.into_iter().zip(probe_inside) {
        if after[contact].len() == event {
            after[contact].push(inside);
        }
    }
    let mut scars = Vec::with_capacity(contacts.len());
    for (index, contact) in contacts.iter().enumerate() {
        let member_axis = sub(contact.other, contact.anchor);
        let member_length = length(member_axis);
        let anchored =
            contained[index] || closest[index].squared_distance.sqrt() <= settings.tol_mm;
        let mut interval = if anchored { member_length } else { 0.0 };
        let mut exit_triangle = closest[index].triangle_index;
        if anchored {
            for (event, hit) in crossings[index].iter().enumerate() {
                if hit.distance <= settings.tol_mm {
                    continue;
                }
                if after[index].get(event).copied() == Some(false) {
                    interval = hit.distance.min(member_length);
                    exit_triangle = hit.triangle_index;
                    break;
                }
            }
        }
        let fraction = if member_length == 0.0 {
            0.0
        } else {
            interval / member_length
        };
        let radius = contact.radius_at_anchor
            + (contact.radius_at_other - contact.radius_at_anchor) * fraction;
        let normal = face_normal(mesh, exit_triangle)?;
        let cosine = dot(unit(member_axis), normal).abs();
        let shaft = if interval <= settings.tol_mm {
            0.0
        } else {
            (PI * radius * radius / cosine.max(settings.eps_cos)).min(2.0 * radius * interval)
        };
        let d = closest[index].squared_distance.sqrt();
        let ball = PI * (contact.ball_radius * contact.ball_radius - d * d).max(0.0);
        let area = shaft + ball;
        if !area.is_finite() {
            return Err(ScarMetricError::NonFinite {
                source_index,
                contact: index,
                field: "patch area",
            });
        }
        let reference = match contact.class {
            ContactClass::Stub => contact.anchor,
            ContactClass::Foot => contact.anchor,
        };
        scars.push(Scar { area, reference });
    }
    Ok(scars)
}

#[derive(Clone, Copy)]
struct Sample {
    point: Vec3,
    normal: Vec3,
}

struct DelicacyField {
    samples: Vec<Sample>,
    weights: Vec<f64>,
}

impl DelicacyField {
    fn build(
        source_index: usize,
        mesh: &Mesh,
        settings: Settings,
    ) -> Result<Self, ScarMetricError> {
        let samples = sample_faces(source_index, mesh)?;
        let curvature = curvature_scores(source_index, mesh, &samples, settings.thin_feature_mm)?;
        let thinness = thinness_scores(source_index, mesh, &samples, settings.thin_feature_mm)?;
        let mut weights = curvature
            .into_iter()
            .zip(thinness)
            .map(|(curvature, thinness)| {
                settings.curvature_weight * curvature + settings.thinness_weight * thinness
            })
            .collect::<Vec<_>>();
        let maximum = weights
            .iter()
            .copied()
            .filter(|value| *value > 0.0)
            .fold(0.0_f64, f64::max);
        let divisor = if maximum > 0.0 { maximum } else { 1.0 };
        for weight in &mut weights {
            *weight = (*weight / divisor).clamp(0.0, 1.0);
        }
        Ok(Self { samples, weights })
    }

    fn at_points(&self, source_index: usize, points: &[Vec3]) -> Result<Vec<f64>, ScarMetricError> {
        points
            .iter()
            .enumerate()
            .map(|(contact, point)| {
                let mut nearest = 0;
                let mut distance_squared = f64::INFINITY;
                for (index, sample) in self.samples.iter().enumerate() {
                    let candidate = distance_squared_between(*point, sample.point);
                    if candidate < distance_squared {
                        distance_squared = candidate;
                        nearest = index;
                    }
                }
                let weight = self.weights[nearest];
                if weight.is_finite() {
                    Ok(weight)
                } else {
                    Err(ScarMetricError::NonFinite {
                        source_index,
                        contact,
                        field: "delicacy",
                    })
                }
            })
            .collect()
    }
}

fn sample_faces(source_index: usize, mesh: &Mesh) -> Result<Vec<Sample>, ScarMetricError> {
    let face_count = mesh.triangles().len();
    let count = face_count.clamp(2_000, 20_000);
    let mut areas = Vec::with_capacity(face_count);
    let mut total = 0.0;
    for face in 0..face_count {
        let (a, b, c) = mesh.triangle_points(face);
        total += 0.5 * length(cross(sub(b, a), sub(c, a)));
        areas.push(total);
    }
    if !(total.is_finite() && total > 0.0) {
        return Err(ScarMetricError::ArithmeticOverflow {
            source_index,
            metric: "sampling area",
        });
    }
    let mut rng = LegacyMt19937::seeded(0);
    let faces = (0..count)
        .map(|_| lower_bound(&areas, rng.next_f64() * total))
        .collect::<Vec<_>>();
    let mut samples = Vec::with_capacity(count);
    for face in faces {
        let mut u = rng.next_f64();
        let mut v = rng.next_f64();
        if u + v > 1.0 {
            u = 1.0 - u;
            v = 1.0 - v;
        }
        let (a, b, c) = mesh.triangle_points(face);
        samples.push(Sample {
            point: add(a, add(scale(sub(b, a), u), scale(sub(c, a), v))),
            normal: face_normal(mesh, face)?,
        });
    }
    Ok(samples)
}

fn curvature_scores(
    source_index: usize,
    mesh: &Mesh,
    samples: &[Sample],
    thin: f64,
) -> Result<Vec<f64>, ScarMetricError> {
    let (vertices, defects) = welded_defects(source_index, mesh)?;
    let count = samples.len();
    let sub_count = count.min(4_000);
    let indices = integer_linspace(count, sub_count);
    let radius = thin.max(0.5);
    let mut values = Vec::with_capacity(sub_count);
    for &sample in &indices {
        values.push(
            vertices
                .iter()
                .zip(&defects)
                .filter(|(vertex, _)| {
                    distance_squared_between(**vertex, samples[sample].point) <= radius * radius
                })
                .map(|(_, defect)| defect.abs())
                .sum::<f64>(),
        );
    }
    let percentile = linear_percentile(&values, 95.0).max(1.0e-9);
    let mut scores = Vec::with_capacity(count);
    for sample in samples {
        let mut nearest = 0;
        let mut nearest_distance = f64::INFINITY;
        for (candidate, &index) in indices.iter().enumerate() {
            let distance = distance_squared_between(sample.point, samples[index].point);
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest = candidate;
            }
        }
        scores.push((values[nearest] / percentile).clamp(0.0, 1.0));
    }
    Ok(scores)
}

fn thinness_scores(
    source_index: usize,
    mesh: &Mesh,
    samples: &[Sample],
    thin: f64,
) -> Result<Vec<f64>, ScarMetricError> {
    let centres = (0..mesh.triangles().len())
        .map(|face| {
            let (a, b, c) = mesh.triangle_points(face);
            face_normal(mesh, face).map(|normal| (scale(add(add(a, b), c), 1.0 / 3.0), normal))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let limit = 40.min(centres.len());
    let search = thin * 4.0;
    samples
        .iter()
        .enumerate()
        .map(|(sample_index, sample)| {
            let mut near = Vec::new();
            for (face, (centre, _)) in centres.iter().enumerate() {
                let distance = distance(sample.point, *centre);
                if distance <= search {
                    insert_nearest(&mut near, (distance, face), limit);
                }
            }
            let mut thickness = f64::INFINITY;
            // Selecting K happens before normal filtering, matching the frozen audit.
            for (_, face) in near {
                if dot(sample.normal, centres[face].1) < -0.3 {
                    thickness = thickness.min(distance(sample.point, centres[face].0));
                }
            }
            let score = if thickness.is_finite() {
                (1.0 - thickness / thin).clamp(0.0, 1.0)
            } else {
                0.0
            };
            if score.is_finite() {
                Ok(score)
            } else {
                Err(ScarMetricError::NonFinite {
                    source_index,
                    contact: sample_index,
                    field: "thinness",
                })
            }
        })
        .collect()
}

fn welded_defects(
    source_index: usize,
    mesh: &Mesh,
) -> Result<(Vec<Vec3>, Vec<f64>), ScarMetricError> {
    let mut ids = BTreeMap::new();
    let mut vertices = Vec::new();
    let mut face_ids = Vec::with_capacity(mesh.triangles().len());
    for face in 0..mesh.triangles().len() {
        let (a, b, c) = mesh.triangle_points(face);
        let mut ids_for_face = [0; 3];
        for (corner, value) in [a, b, c].into_iter().enumerate() {
            let key = canonical_key(value);
            let next = vertices.len();
            let id = *ids.entry(key).or_insert_with(|| {
                vertices.push(value);
                next
            });
            ids_for_face[corner] = id;
        }
        face_ids.push(ids_for_face);
    }
    let mut sums = vec![0.0; vertices.len()];
    for (face, ids) in face_ids.iter().enumerate() {
        let (a, b, c) = mesh.triangle_points(face);
        for (at, left, right) in [
            (ids[0], sub(b, a), sub(c, a)),
            (ids[1], sub(a, b), sub(c, b)),
            (ids[2], sub(a, c), sub(b, c)),
        ] {
            let denominator = length(left) * length(right);
            if !(denominator.is_finite() && denominator > 0.0) {
                return Err(ScarMetricError::ArithmeticOverflow {
                    source_index,
                    metric: "welded angle defect",
                });
            }
            sums[at] += (dot(left, right) / denominator).clamp(-1.0, 1.0).acos();
        }
    }
    Ok((
        vertices,
        sums.into_iter().map(|sum| 2.0 * PI - sum).collect(),
    ))
}

fn face_normal(mesh: &Mesh, face: usize) -> Result<Vec3, ScarMetricError> {
    let (a, b, c) = mesh.triangle_points(face);
    let normal = cross(sub(b, a), sub(c, a));
    if !positive(length(normal)) {
        return Err(ScarMetricError::ArithmeticOverflow {
            source_index: 0,
            metric: "face normal",
        });
    }
    Ok(unit(normal))
}

fn checked_sum<I: Iterator<Item = f64>>(
    source_index: usize,
    metric: &'static str,
    values: I,
) -> Result<f64, ScarMetricError> {
    let mut sum = 0.0;
    for value in values {
        sum += value;
        if !sum.is_finite() {
            return Err(ScarMetricError::ArithmeticOverflow {
                source_index,
                metric,
            });
        }
    }
    Ok(sum)
}

fn metric_value(
    source_index: usize,
    field: &'static str,
    value: f64,
) -> Result<MetricValue, ScarMetricError> {
    if !value.is_finite() {
        return Err(ScarMetricError::NonFinite {
            source_index,
            contact: 0,
            field,
        });
    }
    MetricValue::new(value).map_err(|_| ScarMetricError::NonFinite {
        source_index,
        contact: 0,
        field,
    })
}

fn insert_nearest(values: &mut Vec<(f64, usize)>, candidate: (f64, usize), limit: usize) {
    let position = values.partition_point(|value| {
        value
            .0
            .total_cmp(&candidate.0)
            .then(value.1.cmp(&candidate.1))
            .is_lt()
    });
    if position < limit {
        values.insert(position, candidate);
        if values.len() > limit {
            values.pop();
        }
    }
}

fn lower_bound(values: &[f64], target: f64) -> usize {
    values
        .partition_point(|value| *value < target)
        .min(values.len() - 1)
}
fn integer_linspace(length: usize, count: usize) -> Vec<usize> {
    if count == 1 {
        vec![0]
    } else {
        (0..count)
            .map(|index| ((index as f64 * (length - 1) as f64) / (count - 1) as f64) as usize)
            .collect()
    }
}
fn linear_percentile(values: &[f64], percentile: f64) -> f64 {
    let mut ordered = values.to_vec();
    ordered.sort_by(f64::total_cmp);
    let position = percentile / 100.0 * (ordered.len() - 1) as f64;
    let low = position.floor() as usize;
    ordered[low] + (ordered[position.ceil() as usize] - ordered[low]) * (position - low as f64)
}
fn canonical_key(value: Vec3) -> (u64, u64, u64) {
    fn bit(value: f64) -> u64 {
        if value == 0.0 {
            0
        } else {
            value.to_bits()
        }
    }
    (bit(value[0]), bit(value[1]), bit(value[2]))
}
fn f32_close(left: f64, right: f64) -> bool {
    if left == right {
        return true;
    }
    if !(left.is_finite() && right.is_finite()) {
        return false;
    }
    let ulp = |value: f64| {
        let value = value.abs() as f32;
        let next = f32::from_bits(value.to_bits().saturating_add(1));
        (next - value).abs() as f64
    };
    (left - right).abs() <= 16.0 * ulp(left).max(ulp(right)).max(f32::MIN_POSITIVE as f64)
}
fn near(left: Vec3, right: Vec3) -> bool {
    left.into_iter()
        .zip(right)
        .all(|(left, right)| f32_close(left, right))
}
fn parallel(left: Vec3, right: Vec3) -> bool {
    let left_length = length(left);
    let right_length = length(right);
    left_length > 0.0
        && right_length > 0.0
        && 1.0 - (dot(left, right) / (left_length * right_length)).abs()
            <= 64.0 * f32::EPSILON as f64
}
fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
fn midpoint(left: Vec3, right: Vec3) -> Vec3 {
    scale(add(left, right), 0.5)
}
fn add(left: Vec3, right: Vec3) -> Vec3 {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}
fn sub(left: Vec3, right: Vec3) -> Vec3 {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}
fn scale(value: Vec3, factor: f64) -> Vec3 {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}
fn dot(left: Vec3, right: Vec3) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}
fn cross(left: Vec3, right: Vec3) -> Vec3 {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}
fn length(value: Vec3) -> f64 {
    dot(value, value).sqrt()
}
fn distance(left: Vec3, right: Vec3) -> f64 {
    length(sub(left, right))
}
fn distance_squared_between(left: Vec3, right: Vec3) -> f64 {
    dot(sub(left, right), sub(left, right))
}
fn unit(value: Vec3) -> Vec3 {
    scale(value, 1.0 / length(value))
}

/// The legacy `RandomState` MT19937 stream used by NumPy's frozen scorer.
struct LegacyMt19937 {
    state: [u32; 624],
    index: usize,
}
impl LegacyMt19937 {
    fn seeded(seed: u32) -> Self {
        let mut state = [0; 624];
        state[0] = seed;
        for index in 1..624 {
            state[index] = 1_812_433_253u32
                .wrapping_mul(state[index - 1] ^ (state[index - 1] >> 30))
                .wrapping_add(index as u32);
        }
        Self { state, index: 624 }
    }
    fn twist(&mut self) {
        for index in 0..624 {
            let bits =
                (self.state[index] & 0x8000_0000) | (self.state[(index + 1) % 624] & 0x7fff_ffff);
            self.state[index] = self.state[(index + 397) % 624]
                ^ (bits >> 1)
                ^ if bits & 1 == 0 { 0 } else { 0x9908_b0df };
        }
        self.index = 0;
    }
    fn next_u32(&mut self) -> u32 {
        if self.index == 624 {
            self.twist();
        }
        let mut value = self.state[self.index];
        self.index += 1;
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c_5680;
        value ^= (value << 15) & 0xefc6_0000;
        value ^ (value >> 18)
    }
    fn next_f64(&mut self) -> f64 {
        let first = (self.next_u32() >> 5) as f64;
        let second = (self.next_u32() >> 6) as f64;
        (first * 67_108_864.0 + second) / 9_007_199_254_740_992.0
    }
}

#[cfg(test)]
mod tests {
    use super::{integer_linspace, linear_percentile, LegacyMt19937};
    #[test]
    fn legacy_seed_zero_matches_numpy_randomstate_prefix() {
        let mut rng = LegacyMt19937::seeded(0);
        assert!((rng.next_f64() - 0.548_813_503_927_324_8).abs() < f64::EPSILON);
        assert!((rng.next_f64() - 0.715_189_366_372_419_5).abs() < f64::EPSILON);
    }
    #[test]
    fn numpy_integer_linspace_and_percentile_are_linear() {
        assert_eq!(integer_linspace(8, 4), [0, 2, 4, 7]);
        assert!((linear_percentile(&[0.0, 10.0, 20.0, 30.0], 95.0) - 28.5).abs() < 1.0e-12);
    }
}
