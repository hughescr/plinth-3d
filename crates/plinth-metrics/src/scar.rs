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
    #[error("model {source_index} contact {contact} has an ambiguous segment boundary at {distance_mm} mm")]
    BoundaryAmbiguity {
        source_index: usize,
        contact: usize,
        distance_mm: f64,
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
            // Type-9 is also the ordinary joint-ball encoding.  It only claims
            // contact identity when a complete foot relation exists, so an
            // unrelated or malformed ordinary ball is not itself an error.
            SegmentType::SHAFT => balls.push(index),
            _ => {}
        }
    }

    // Form the complete bipartite candidate set before resolving identity;
    // adjacency is deliberately not a contact rule.
    let mut stub_by_tip: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut stub_by_taper: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &tip in &tips {
        for &taper in &tapers {
            if stub_candidate(tip, taper, supports, settings) {
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
        for endpoint in [false, true] {
            let (point, radius) = if endpoint {
                (support.end(), r1)
            } else {
                (support.start(), r0)
            };
            for &ball in &balls {
                if ball <= member {
                    continue;
                }
                if foot_candidate(point, radius, support, &supports[ball], settings) {
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
    if !positive_stored_radius(r0) || !stored_equal(r0, r1) {
        return Err(ScarMetricError::InvalidSphereRadii {
            source_index,
            record,
        });
    }
    let centre = midpoint(support.start(), support.end());
    let radius = r0;
    if !segment_length_matches_radius(support.start(), support.end(), radius, 2.0)
        || !sphere_mesh_matches(support, radius)
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
    if !positive_stored_radius(radius)
        || !stored_equal(start[0], end[0])
        || !stored_equal(start[1], end[1])
        || start[2] <= end[2]
        || !segment_length_matches_radius(start, end, radius, 2.0)
    {
        return Err(ScarMetricError::InvalidFootBallAxis {
            source_index,
            record,
        });
    }
    Ok(())
}

fn stub_candidate(
    tip_index: usize,
    taper_index: usize,
    supports: &[DiskSupport],
    settings: Settings,
) -> bool {
    let tip = &supports[tip_index];
    let taper = &supports[taper_index];
    let (ball_radius, other_ball_radius) = tip.radii();
    let (r0, r1) = taper.radii();
    positive_stored_radius(ball_radius)
        && stored_equal(ball_radius, other_ball_radius)
        && positive_stored_radius(r0)
        && positive_stored_radius(r1)
        && endpoint_matches_midpoint(taper.start(), tip.start(), tip.end())
        && stored_scaled_equal(ball_radius, settings.stub_factor, r0)
        && segment_nonzero(taper.start(), taper.end())
        // Axis feasibility is a post-identity geometry validation.  The
        // identity-bearing center and factor predicates above are exact
        // preimage-intersection tests over the stored binary32 fields.
        && parallel_stored_segments(taper.start(), taper.end(), tip.start(), tip.end())
}

fn foot_candidate(
    endpoint: Vec3,
    endpoint_radius: f64,
    member: &DiskSupport,
    ball: &DiskSupport,
    settings: Settings,
) -> bool {
    let (r0, r1) = member.radii();
    let (ball_radius, ball_radius_end) = ball.radii();
    let start = ball.start();
    let end = ball.end();
    positive_stored_radius(endpoint_radius)
        && positive_stored_radius(ball_radius)
        && stored_equal(ball_radius, ball_radius_end)
        && (member.segment_type() != SegmentType::JOINER || stored_equal(r0, r1))
        && endpoint_matches_midpoint(endpoint, start, end)
        && stored_scaled_equal(ball_radius, settings.foot_factor, endpoint_radius)
        && stored_equal(start[0], end[0])
        && stored_equal(start[1], end[1])
        && start[2] > end[2]
        && segment_length_matches_radius(start, end, ball_radius, 2.0)
        && sphere_mesh_matches(ball, ball_radius)
        && segment_nonzero(member.start(), member.end())
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
    if !positive_stored_radius(r0) || !positive_stored_radius(r1) {
        return Err(ScarMetricError::StubGeometry {
            source_index,
            tip: tip_index,
            member: taper_index,
            detail: "non-positive taper radius",
        });
    }
    if !endpoint_matches_midpoint(taper.start(), tip.start(), tip.end()) {
        return Err(ScarMetricError::StubGeometry {
            source_index,
            tip: tip_index,
            member: taper_index,
            detail: "taper does not start at ball centre",
        });
    }
    if !stored_scaled_equal(ball_radius, settings.stub_factor, r0) {
        return Err(ScarMetricError::StubGeometry {
            source_index,
            tip: tip_index,
            member: taper_index,
            detail: "ball-factor mismatch",
        });
    }
    let outward = sub(taper.end(), centre);
    if !positive(length(outward))
        || !parallel_stored_segments(taper.start(), taper.end(), tip.start(), tip.end())
    {
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
    if support.segment_type() == SegmentType::JOINER && !stored_equal(r0, r1) {
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
    if !endpoint_matches_midpoint(endpoint, ball_support.start(), ball_support.end())
        || !stored_scaled_equal(ball_radius, settings.foot_factor, endpoint_radius)
    {
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
    let other_endpoints = contacts
        .iter()
        .map(|contact| contact.other)
        .collect::<Vec<_>>();
    let closest = closest_points(
        mesh,
        &anchors
            .iter()
            .chain(&other_endpoints)
            .copied()
            .collect::<Vec<_>>(),
    )?;
    let rays = contacts
        .iter()
        .map(|contact| Ray {
            origin: contact.anchor,
            direction: sub(contact.other, contact.anchor),
        })
        .collect::<Vec<_>>();
    let intersections = ray_intersections(mesh, &rays)?;
    #[derive(Clone, Copy)]
    struct InteriorProbe {
        contact: usize,
        start: f64,
        end: f64,
        exit_triangle: Option<usize>,
    }
    let mut intervals = Vec::new();
    let mut probes = Vec::new();
    let merge_bound = intersection_merge_bound(mesh, contacts);
    for (index, (contact, hits)) in contacts.iter().zip(intersections).enumerate() {
        let member_length = length(sub(contact.other, contact.anchor));
        let mut unique = Vec::new();
        for hit in hits {
            if hit.distance <= member_length + merge_bound
                && hit.distance >= -merge_bound
                && unique.last().is_none_or(|last: &crate::RayHit| {
                    !ray_hit_equal(last.distance, hit.distance, merge_bound)
                })
            {
                unique.push(crate::RayHit {
                    distance: hit.distance.clamp(0.0, member_length),
                    triangle_index: hit.triangle_index,
                });
            }
        }
        let mut start = 0.0;
        for event in unique.iter().copied().chain(std::iter::once(crate::RayHit {
            distance: member_length,
            triangle_index: usize::MAX,
        })) {
            if event.distance > start && !ray_hit_equal(event.distance, start, merge_bound) {
                let distance = (start + event.distance) * 0.5;
                probes.push(add(
                    contact.anchor,
                    scale(unit(sub(contact.other, contact.anchor)), distance),
                ));
                intervals.push(InteriorProbe {
                    contact: index,
                    start,
                    end: event.distance,
                    exit_triangle: (event.triangle_index != usize::MAX)
                        .then_some(event.triangle_index),
                });
            }
            start = event.distance;
        }
    }
    let probe_inside = contains_points(mesh, &probes)?;
    let probe_closest = closest_points(mesh, &probes)?;
    let mut scars = Vec::with_capacity(contacts.len());
    for (index, contact) in contacts.iter().enumerate() {
        let member_axis = sub(contact.other, contact.anchor);
        let member_length = length(member_axis);
        // A tip is not made interior merely because it is near the mesh.  The
        // midpoint must classify as actual solid, and the interval entry must
        // be within the physical anchoring tolerance of the designed endpoint.
        let selected = intervals
            .iter()
            .zip(&probe_inside)
            .zip(&probe_closest)
            .find_map(|((interval, inside), closest)| {
                if interval.contact != index
                    || !anchored_entry_is_eligible(interval.start, settings.tol_mm)
                {
                    return None;
                }
                if closest.squared_distance <= merge_bound * merge_bound {
                    return Some(Err(ScarMetricError::BoundaryAmbiguity {
                        source_index,
                        contact: index,
                        distance_mm: (interval.start + interval.end) * 0.5,
                    }));
                }
                (*inside).then_some(Ok(*interval))
            });
        let selected = selected.transpose()?;
        let (interval_length, exit_triangle) =
            selected.map_or((0.0, closest[index].triangle_index), |interval| {
                (
                    interval.end - interval.start,
                    interval
                        .exit_triangle
                        .unwrap_or(closest[contacts.len() + index].triangle_index),
                )
            });
        let normal = face_normal(source_index, mesh, exit_triangle)?;
        let cosine = dot(unit(member_axis), normal).abs();
        let d = closest[index].squared_distance.sqrt();
        let area = contact_patch_area(
            member_length,
            interval_length,
            contact.radius_at_anchor,
            contact.radius_at_other,
            contact.ball_radius,
            d,
            cosine,
            settings,
        );
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

fn anchored_entry_is_eligible(entry: f64, tolerance: f64) -> bool {
    entry.is_finite() && tolerance.is_finite() && entry >= 0.0 && entry <= tolerance
}

#[allow(clippy::too_many_arguments)]
fn contact_patch_area(
    member_length: f64,
    anchored_length: f64,
    radius_at_anchor: f64,
    radius_at_other: f64,
    ball_radius: f64,
    ball_distance: f64,
    cosine: f64,
    settings: Settings,
) -> f64 {
    let fraction = if member_length > 0.0 {
        (anchored_length / member_length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let radius = radius_at_anchor + (radius_at_other - radius_at_anchor) * fraction;
    let shaft = if anchored_length <= settings.tol_mm {
        0.0
    } else {
        (PI * radius * radius / cosine.max(settings.eps_cos)).min(2.0 * radius * anchored_length)
    };
    let ball = PI * (ball_radius * ball_radius - ball_distance * ball_distance).max(0.0);
    shaft + ball
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
            normal: face_normal(source_index, mesh, face)?,
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
            face_normal(source_index, mesh, face)
                .map(|normal| (scale(add(add(a, b), c), 1.0 / 3.0), normal))
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

fn face_normal(source_index: usize, mesh: &Mesh, face: usize) -> Result<Vec3, ScarMetricError> {
    let (a, b, c) = mesh.triangle_points(face);
    let normal = cross(sub(b, a), sub(c, a));
    if !positive(length(normal)) {
        return Err(ScarMetricError::ArithmeticOverflow {
            source_index,
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
/// Exact real preimage of one finite binary32 value under IEEE-754
/// round-to-nearest, ties-to-even.  Endpoint closure matters: a midpoint tie
/// belongs to the adjacent value whose significand is even, never both.
#[derive(Clone, Copy, Debug)]
struct RealInterval {
    lo: f64,
    hi: f64,
    lo_closed: bool,
    hi_closed: bool,
}

impl RealInterval {
    fn stored(value: f64) -> Option<Self> {
        let stored = value as f32;
        if !stored.is_finite() || f64::from(stored).to_bits() != value.to_bits() {
            return None;
        }
        let bits = stored.to_bits();
        let half_min_subnormal = f64::from(f32::from_bits(1)) * 0.5;
        if bits == 0 {
            return Some(Self {
                lo: 0.0,
                hi: half_min_subnormal,
                lo_closed: true,
                hi_closed: true,
            });
        }
        if bits == 0x8000_0000 {
            return Some(Self {
                lo: -half_min_subnormal,
                hi: -0.0,
                lo_closed: true,
                hi_closed: true,
            });
        }
        if bits == 0x7f7f_ffff {
            let previous = f32::from_bits(bits - 1);
            return Some(Self {
                lo: midpoint_exact(f64::from(previous), value),
                hi: value + 2.0_f64.powi(103),
                lo_closed: false,
                hi_closed: false,
            });
        }
        if bits == 0xff7f_ffff {
            let next = f32::from_bits(bits - 1);
            return Some(Self {
                lo: value - 2.0_f64.powi(103),
                hi: midpoint_exact(value, f64::from(next)),
                lo_closed: false,
                hi_closed: false,
            });
        }
        let previous = next_down_f32(stored)?;
        let next = next_up_f32(stored)?;
        let even = bits & 1 == 0;
        Some(Self {
            lo: midpoint_exact(f64::from(previous), value),
            hi: midpoint_exact(value, f64::from(next)),
            lo_closed: even,
            hi_closed: even,
        })
    }

    fn add(self, other: Self) -> Self {
        Self {
            lo: self.lo + other.lo,
            hi: self.hi + other.hi,
            lo_closed: self.lo_closed && other.lo_closed,
            hi_closed: self.hi_closed && other.hi_closed,
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            lo: self.lo - other.hi,
            hi: self.hi - other.lo,
            lo_closed: self.lo_closed && other.hi_closed,
            hi_closed: self.hi_closed && other.lo_closed,
        }
    }

    fn scaled(self, factor: f64) -> Self {
        debug_assert!(factor.is_finite());
        let a = self.lo * factor;
        let b = self.hi * factor;
        if factor >= 0.0 {
            Self {
                lo: a,
                hi: b,
                lo_closed: self.lo_closed,
                hi_closed: self.hi_closed,
            }
        } else {
            Self {
                lo: b,
                hi: a,
                lo_closed: self.hi_closed,
                hi_closed: self.lo_closed,
            }
        }
    }

    fn overlaps(self, other: Self) -> bool {
        if self.hi < other.lo || other.hi < self.lo {
            return false;
        }
        if self.hi == other.lo {
            return self.hi_closed && other.lo_closed;
        }
        if other.hi == self.lo {
            return other.hi_closed && self.lo_closed;
        }
        true
    }

    fn contains_zero(self) -> bool {
        (self.lo < 0.0 && self.hi > 0.0)
            || (self.lo == 0.0 && self.lo_closed)
            || (self.hi == 0.0 && self.hi_closed)
    }
}

fn midpoint_exact(left: f64, right: f64) -> f64 {
    left * 0.5 + right * 0.5
}

fn next_down_f32(value: f32) -> Option<f32> {
    let bits = value.to_bits();
    let next = if value > 0.0 {
        bits.checked_sub(1)?
    } else {
        bits.checked_add(1)?
    };
    let value = f32::from_bits(next);
    value.is_finite().then_some(value)
}

fn next_up_f32(value: f32) -> Option<f32> {
    let bits = value.to_bits();
    let next = if value >= 0.0 {
        bits.checked_add(1)?
    } else {
        bits.checked_sub(1)?
    };
    let value = f32::from_bits(next);
    value.is_finite().then_some(value)
}

fn stored_equal(left: f64, right: f64) -> bool {
    RealInterval::stored(left)
        .zip(RealInterval::stored(right))
        .is_some_and(|(left, right)| left.overlaps(right))
}

fn stored_scaled_equal(left: f64, factor: f64, right: f64) -> bool {
    factor.is_finite()
        && RealInterval::stored(left)
            .zip(RealInterval::stored(right))
            .is_some_and(|(left, right)| left.overlaps(right.scaled(factor)))
}

fn segment_length_matches_radius(start: Vec3, end: Vec3, radius: f64, factor: f64) -> bool {
    vector_intervals(start, end)
        .zip(RealInterval::stored(radius))
        .is_some_and(|(components, radius)| {
            norm_interval(components).overlaps(radius.scaled(factor))
        })
}

fn point_radius_matches_sphere(vertex: Vec3, start: Vec3, end: Vec3, radius: f64) -> bool {
    let component = |axis| {
        Some(
            RealInterval::stored(vertex[axis])?.sub(
                RealInterval::stored(start[axis])?
                    .add(RealInterval::stored(end[axis])?)
                    .scaled(0.5),
            ),
        )
    };
    let components = component(0)
        .zip(component(1))
        .zip(component(2))
        .map(|((x, y), z)| [x, y, z]);
    components
        .zip(RealInterval::stored(radius))
        .is_some_and(|(components, radius)| norm_interval(components).overlaps(radius))
}

fn vector_intervals(start: Vec3, end: Vec3) -> Option<[RealInterval; 3]> {
    Some([
        RealInterval::stored(end[0])?.sub(RealInterval::stored(start[0])?),
        RealInterval::stored(end[1])?.sub(RealInterval::stored(start[1])?),
        RealInterval::stored(end[2])?.sub(RealInterval::stored(start[2])?),
    ])
}

fn norm_interval(components: [RealInterval; 3]) -> RealInterval {
    let mut minimum_squared = 0.0;
    let mut maximum_squared = 0.0;
    for component in components {
        let minimum = if component.contains_zero() {
            0.0
        } else {
            component.lo.abs().min(component.hi.abs())
        };
        let maximum = component.lo.abs().max(component.hi.abs());
        minimum_squared += minimum * minimum;
        maximum_squared += maximum * maximum;
    }
    RealInterval {
        lo: minimum_squared.sqrt(),
        hi: maximum_squared.sqrt(),
        lo_closed: true,
        hi_closed: true,
    }
}

fn endpoint_matches_midpoint(endpoint: Vec3, start: Vec3, end: Vec3) -> bool {
    (0..3).all(|axis| {
        RealInterval::stored(endpoint[axis])
            .zip(RealInterval::stored(start[axis]))
            .zip(RealInterval::stored(end[axis]))
            .is_some_and(|((endpoint, start), end)| endpoint.overlaps(start.add(end).scaled(0.5)))
    })
}

fn sphere_mesh_matches(support: &DiskSupport, radius: f64) -> bool {
    support
        .mesh()
        .vertices()
        .iter()
        .all(|vertex| point_radius_matches_sphere(*vertex, support.start(), support.end(), radius))
}

fn segment_nonzero(start: Vec3, end: Vec3) -> bool {
    vector_intervals(start, end).is_some_and(|components| {
        let interval = norm_interval(components);
        interval.hi > 0.0
    })
}

fn parallel_stored_segments(
    left_start: Vec3,
    left_end: Vec3,
    right_start: Vec3,
    right_end: Vec3,
) -> bool {
    let Some(left) = vector_intervals(left_start, left_end) else {
        return false;
    };
    let Some(right) = vector_intervals(right_start, right_end) else {
        return false;
    };
    if norm_interval(left).hi == 0.0 || norm_interval(right).hi == 0.0 {
        return false;
    }
    // Conservative post-match validation for the nonlinear axis predicate.
    // The original endpoint intervals flow through each product; no derived
    // vector is reinterpreted as an independently stored binary32 value.
    [
        interval_product(left[1], right[2]).sub(interval_product(left[2], right[1])),
        interval_product(left[2], right[0]).sub(interval_product(left[0], right[2])),
        interval_product(left[0], right[1]).sub(interval_product(left[1], right[0])),
    ]
    .into_iter()
    .all(RealInterval::contains_zero)
}

fn interval_product(left: RealInterval, right: RealInterval) -> RealInterval {
    let values = [
        (left.lo * right.lo, left.lo_closed && right.lo_closed),
        (left.lo * right.hi, left.lo_closed && right.hi_closed),
        (left.hi * right.lo, left.hi_closed && right.lo_closed),
        (left.hi * right.hi, left.hi_closed && right.hi_closed),
    ];
    let lo = values
        .iter()
        .map(|value| value.0)
        .fold(f64::INFINITY, f64::min);
    let hi = values
        .iter()
        .map(|value| value.0)
        .fold(f64::NEG_INFINITY, f64::max);
    RealInterval {
        lo,
        hi,
        lo_closed: values.iter().any(|value| value.0 == lo && value.1),
        hi_closed: values.iter().any(|value| value.0 == hi && value.1),
    }
}

fn intersection_merge_bound(mesh: &Mesh, contacts: &[Contact]) -> f64 {
    let coordinate_scale = mesh
        .vertices()
        .iter()
        .flat_map(|vertex| vertex.iter())
        .chain(
            contacts
                .iter()
                .flat_map(|contact| contact.anchor.iter().chain(contact.other.iter())),
        )
        .map(|value| value.abs())
        .fold(0.0_f64, f64::max);
    let length_scale = contacts
        .iter()
        .map(|contact| length(sub(contact.other, contact.anchor)))
        .fold(0.0_f64, f64::max);
    let scale = coordinate_scale.max(length_scale);
    // `geometry::ray_triangle_distance` normalizes the ray and triangle, then
    // evaluates Moller-Trumbore.  Counting every add/subtract/multiply/divide
    // along the distance path gives at most 48 rounded binary64 operations.
    // Higham's gamma_n bound therefore supplies a scale-derived merge radius;
    // it is an error analysis of this operation graph, not a tuned epsilon.
    let operations = 48.0;
    let gamma = operations * f64::EPSILON / (1.0 - operations * f64::EPSILON);
    gamma * scale
}

fn ray_hit_equal(left: f64, right: f64, bound: f64) -> bool {
    (left - right).abs() <= bound
}
fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
fn positive_stored_radius(value: f64) -> bool {
    let stored = value as f32;
    stored.is_finite()
        && stored.is_normal()
        && stored.is_sign_positive()
        && f64::from(stored).to_bits() == value.to_bits()
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
    use super::{
        anchored_entry_is_eligible, contact_patch_area, endpoint_matches_midpoint,
        integer_linspace, linear_percentile, parallel_stored_segments, positive_stored_radius,
        stored_equal, stored_scaled_equal, LegacyMt19937, RealInterval, Settings,
    };
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

    #[test]
    fn binary32_preimages_assign_midpoint_ties_to_even_only() {
        let one = 1.0_f32 as f64;
        let next = f32::from_bits(1.0_f32.to_bits() + 1) as f64;
        let two_away = f32::from_bits(1.0_f32.to_bits() + 2) as f64;
        let one_interval = RealInterval::stored(one).unwrap();
        let next_interval = RealInterval::stored(next).unwrap();
        assert!(one_interval.hi_closed);
        assert!(!next_interval.lo_closed);
        assert_eq!(one_interval.hi, next_interval.lo);
        assert!(!stored_equal(one, next));
        assert!(!stored_equal(one, two_away));

        let negative_even = -(two_away as f32);
        let negative_odd = -f32::from_bits(1.0_f32.to_bits() + 1);
        let even_interval = RealInterval::stored(f64::from(negative_even)).unwrap();
        let odd_interval = RealInterval::stored(f64::from(negative_odd)).unwrap();
        assert!(even_interval.hi_closed);
        assert!(!odd_interval.lo_closed);
        assert_eq!(even_interval.hi, odd_interval.lo);
    }

    #[test]
    fn binary32_preimages_preserve_signed_zero_and_reject_non_stored_values() {
        let positive_zero = RealInterval::stored(0.0).unwrap();
        let negative_zero = RealInterval::stored(-0.0).unwrap();
        assert_eq!(positive_zero.lo.to_bits(), 0.0_f64.to_bits());
        assert_eq!(negative_zero.hi.to_bits(), (-0.0_f64).to_bits());
        assert!(stored_equal(0.0, -0.0));
        assert!(RealInterval::stored(f64::from(f32::from_bits(1))).is_some());
        assert!(RealInterval::stored(f64::INFINITY).is_none());
        assert!(RealInterval::stored(1.0 + f64::EPSILON).is_none());
        assert!(!positive_stored_radius(0.0));
        assert!(!positive_stored_radius(f64::from(f32::from_bits(1))));
    }

    #[test]
    fn factor_and_midpoint_constraints_propagate_stored_endpoint_intervals() {
        assert!(stored_scaled_equal(0.3_f32.into(), 1.5, 0.2_f32.into()));
        assert!(stored_scaled_equal(0.35_f32.into(), 1.4, 0.25_f32.into()));
        let first_outside = (1..=8)
            .find(|offset| {
                !stored_scaled_equal(
                    f64::from(f32::from_bits(0.35_f32.to_bits() + offset)),
                    1.4,
                    f64::from(0.25_f32),
                )
            })
            .expect("a nearby binary32 lies beyond the exact factor interval");
        assert!(stored_scaled_equal(
            f64::from(f32::from_bits(0.35_f32.to_bits() + first_outside - 1)),
            1.4,
            f64::from(0.25_f32),
        ));
        assert!(endpoint_matches_midpoint(
            [f64::from(0.3_f32), f64::from(-1.5_f32), f64::from(1.4_f32),],
            [f64::from(0.1_f32), f64::from(-2.0_f32), f64::from(1.0_f32),],
            [f64::from(0.5_f32), f64::from(-1.0_f32), f64::from(1.8_f32),],
        ));

        let exact = parallel_stored_segments(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
        );
        assert!(exact, "reversed poles describe the same axis");
        let first_outside = (1..=16)
            .find(|offset| {
                let x = f64::from(f32::from_bits(*offset));
                !parallel_stored_segments(
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [0.0, 0.0, 0.0],
                    [x, 0.0, 1.0],
                )
            })
            .expect("an axis perturbation eventually leaves the endpoint boxes");
        if first_outside > 1 {
            let x = f64::from(f32::from_bits(first_outside - 1));
            assert!(parallel_stored_segments(
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0],
                [x, 0.0, 1.0],
            ));
        }
    }

    #[test]
    fn closed_form_patch_uses_anchored_length_and_never_applies_an_area_cap() {
        let settings = Settings {
            curvature_weight: 0.0,
            thinness_weight: 0.0,
            thin_feature_mm: 1.0,
            eps_cos: 0.1,
            tol_mm: 0.05,
            stub_factor: 1.5,
            foot_factor: 1.4,
        };
        assert!(anchored_entry_is_eligible(0.05, 0.05));
        assert!(!anchored_entry_is_eligible(
            f64::from_bits(0.05_f64.to_bits() + 1),
            0.05,
        ));
        // T/L=.25 gives r_x=.25, independently of the absolute exit position.
        let expected_shaft = (std::f64::consts::PI * 0.25_f64.powi(2) / 0.5).min(2.0 * 0.25 * 0.5);
        let expected_ball = std::f64::consts::PI * (0.3_f64.powi(2) - 0.1_f64.powi(2));
        let area = contact_patch_area(2.0, 0.5, 0.2, 0.4, 0.3, 0.1, 0.5, settings);
        assert_eq!(area, expected_shaft + expected_ball);
        assert!(
            area > 0.2,
            "A_patch > a hypothetical A_max remains unclamped"
        );

        let at_tol = contact_patch_area(2.0, 0.05, 0.2, 0.4, 0.3, 0.1, 0.5, settings);
        assert_eq!(at_tol, expected_ball);
        let above_tol = contact_patch_area(
            2.0,
            f64::from_bits(0.05_f64.to_bits() + 1),
            0.2,
            0.4,
            0.3,
            0.1,
            0.0,
            settings,
        );
        assert!(above_tol > expected_ball);

        let perpendicular = contact_patch_area(1.0, 1.0, 0.2, 0.2, 0.0, 0.0, 1.0, settings);
        assert_eq!(perpendicular, std::f64::consts::PI * 0.2_f64.powi(2));
        let grazing = contact_patch_area(1.0, 1.0, 0.2, 0.2, 0.0, 0.0, 0.0, settings);
        assert_eq!(grazing, 2.0 * 0.2 * 1.0);
        let ellipse_chord_boundary = std::f64::consts::PI * 0.2 / (2.0 * 1.0);
        let boundary = contact_patch_area(
            1.0,
            1.0,
            0.2,
            0.2,
            0.0,
            0.0,
            ellipse_chord_boundary,
            settings,
        );
        assert_eq!(boundary, 2.0 * 0.2 * 1.0);
    }
}
