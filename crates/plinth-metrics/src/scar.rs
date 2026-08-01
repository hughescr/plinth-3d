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
    #[error("model {source_index} cannot build the {index} spatial index")]
    SpatialIndex {
        source_index: usize,
        index: &'static str,
    },
    #[error("model {source_index} diagnostic sample count must be positive, got {n_samples}")]
    InvalidSampleCount {
        source_index: usize,
        n_samples: usize,
    },
    #[error("model {source_index} diagnostic point {point} coordinate {component} is not finite")]
    InvalidDiagnosticPoint {
        source_index: usize,
        point: usize,
        component: usize,
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
                DelicacyField::build(source_index, model.mesh(), settings, 0, 20_000)?.at_points(
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

/// One sampled raw face in the independent delicacy field.
///
/// `face_index` is always the original soup index, even when the frozen
/// exterior classifier selected a proper subset for area sampling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DelicacySample {
    pub point: Vec3,
    pub normal: Vec3,
    pub face_index: usize,
}

/// Honest work performed by an immutable indexed field build or batch query.
///
/// These are result-owned observations, not a mutable global counter or a
/// production tuning hook.  They make the release gate reproducible without
/// changing any field decision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DelicacyIndexStats {
    pub triangle_nodes_visited: usize,
    pub triangle_faces_tested: usize,
    pub vertex_grid_candidates: usize,
    pub sample_kd_nodes_visited: usize,
    pub centre_kd_nodes_visited: usize,
}

/// Result of a diagnostic whole-slice delicacy lookup.
#[derive(Clone, Debug, PartialEq)]
pub struct DelicacyPointQuery {
    pub weights: Vec<f64>,
    pub stats: DelicacyIndexStats,
}

/// Immutable, auditor-owned delicacy field.
///
/// Construction consumes an explicit legacy MT19937 seed.  The normal scar
/// metric path deliberately passes zero, while diagnostics can demonstrate a
/// different frozen stream without relying on process-global RNG state.
pub struct DelicacyField {
    exterior_mask: Vec<bool>,
    selected_faces: Vec<usize>,
    samples: Vec<DelicacySample>,
    curvature_scores: Vec<f64>,
    thinness_scores: Vec<f64>,
    weights: Vec<f64>,
    sample_index: PointKdTree,
    stats: DelicacyIndexStats,
}

impl DelicacyField {
    /// Builds the full independent field with an explicit sampling seed and
    /// sample-count ceiling.
    pub fn build_diagnostic(
        source_index: usize,
        mesh: &Mesh,
        seed: u32,
        n_samples: usize,
    ) -> Result<Self, ScarMetricError> {
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
        Self::build(source_index, mesh, settings, seed, n_samples)
    }

    fn build(
        source_index: usize,
        mesh: &Mesh,
        settings: Settings,
        seed: u32,
        n_samples: usize,
    ) -> Result<Self, ScarMetricError> {
        if n_samples == 0 {
            return Err(ScarMetricError::InvalidSampleCount {
                source_index,
                n_samples,
            });
        }
        let mut stats = DelicacyIndexStats::default();
        let triangles = TriangleBvh::build(mesh);
        let exterior = exterior_selection(source_index, mesh, &triangles, &mut stats)?;
        let count = n_samples.min(mesh.triangles().len().max(2_000));
        let samples = sample_faces(source_index, mesh, &exterior.selected_faces, seed, count)?;
        let welded = welded_defects(source_index, mesh)?;
        let curvature = curvature_scores(
            source_index,
            &samples,
            &welded,
            settings.thin_feature_mm,
            &mut stats,
        )?;
        let thinness = thinness_scores(
            source_index,
            mesh,
            &samples,
            settings.thin_feature_mm,
            &mut stats,
        )?;
        let mut weights = curvature
            .iter()
            .zip(&thinness)
            .map(|(curvature, thinness)| {
                settings.curvature_weight * curvature + settings.thinness_weight * thinness
            })
            .collect::<Vec<_>>();
        normalize_weights(&mut weights);
        let sample_index = PointKdTree::build(samples.iter().map(|sample| sample.point).collect());
        Ok(Self {
            exterior_mask: exterior.mask,
            selected_faces: exterior.selected_faces,
            samples,
            curvature_scores: curvature,
            thinness_scores: thinness,
            weights,
            sample_index,
            stats,
        })
    }

    #[must_use]
    pub fn samples(&self) -> &[DelicacySample] {
        &self.samples
    }

    #[must_use]
    pub fn exterior_mask(&self) -> &[bool] {
        &self.exterior_mask
    }

    #[must_use]
    pub fn selected_face_indices(&self) -> &[usize] {
        &self.selected_faces
    }

    #[must_use]
    pub fn curvature_scores(&self) -> &[f64] {
        &self.curvature_scores
    }

    #[must_use]
    pub fn thinness_scores(&self) -> &[f64] {
        &self.thinness_scores
    }

    #[must_use]
    pub fn weights(&self) -> &[f64] {
        &self.weights
    }

    #[must_use]
    pub fn build_stats(&self) -> DelicacyIndexStats {
        self.stats
    }

    /// Looks up every point in one deterministic nearest-sample batch.
    pub fn query_points(
        &self,
        source_index: usize,
        points: &[Vec3],
    ) -> Result<DelicacyPointQuery, ScarMetricError> {
        for (point, value) in points.iter().enumerate() {
            if let Some(component) = value.iter().position(|coordinate| !coordinate.is_finite()) {
                return Err(ScarMetricError::InvalidDiagnosticPoint {
                    source_index,
                    point,
                    component,
                });
            }
        }
        let mut stats = DelicacyIndexStats::default();
        let nearest = self
            .sample_index
            .nearest_batch(points, &mut stats.sample_kd_nodes_visited);
        let mut weights = Vec::with_capacity(points.len());
        for (contact, sample) in nearest.into_iter().enumerate() {
            let weight = self.weights[sample];
            if !weight.is_finite() {
                return Err(ScarMetricError::NonFinite {
                    source_index,
                    contact,
                    field: "delicacy",
                });
            }
            weights.push(weight);
        }
        Ok(DelicacyPointQuery { weights, stats })
    }

    fn at_points(&self, source_index: usize, points: &[Vec3]) -> Result<Vec<f64>, ScarMetricError> {
        Ok(self.query_points(source_index, points)?.weights)
    }
}

struct ExteriorSelection {
    mask: Vec<bool>,
    selected_faces: Vec<usize>,
}

fn exterior_selection(
    source_index: usize,
    mesh: &Mesh,
    triangles: &TriangleBvh,
    stats: &mut DelicacyIndexStats,
) -> Result<ExteriorSelection, ScarMetricError> {
    let face_count = mesh.triangles().len();
    let mut probes = Vec::with_capacity(face_count * 6);
    for face in 0..face_count {
        let (a, b, c) = mesh.triangle_points(face);
        let normal = face_normal(source_index, mesh, face)?;
        for barycentric in [[1.0 / 3.0; 3], [0.6, 0.2, 0.2], [0.2, 0.6, 0.2]] {
            let point = add(
                scale(a, barycentric[0]),
                add(scale(b, barycentric[1]), scale(c, barycentric[2])),
            );
            for offset in [0.05, 0.15] {
                probes.push(add(point, scale(normal, offset)));
            }
        }
    }
    let inside = triangles.frozen_contains_batch(mesh, &probes, stats);
    let mask = (0..face_count)
        .map(|face| {
            let start = face * 6;
            inside[start..start + 6]
                .iter()
                .filter(|inside| **inside)
                .count()
                < 4
        })
        .collect::<Vec<_>>();
    let exterior = mask
        .iter()
        .enumerate()
        .filter_map(|(face, exterior)| exterior.then_some(face))
        .collect::<Vec<_>>();
    // A shell has no meaningful exterior subset when every face has the same
    // frozen classification.  Preserve full-face sampling in either case.
    let selected_faces = if exterior.is_empty() || exterior.len() == face_count {
        (0..face_count).collect()
    } else {
        exterior
    };
    Ok(ExteriorSelection {
        mask,
        selected_faces,
    })
}

fn sample_faces(
    source_index: usize,
    mesh: &Mesh,
    faces: &[usize],
    seed: u32,
    count: usize,
) -> Result<Vec<DelicacySample>, ScarMetricError> {
    let mut areas = Vec::with_capacity(faces.len());
    let mut total = 0.0;
    for &face in faces {
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
    let mut rng = LegacyMt19937::seeded(seed);
    // NumPy's mesh.sample first consumes the whole face-uniform vector, then
    // consumes its 2*n barycentric uniforms.  Interleaving changes the stream.
    let selected = (0..count)
        .map(|_| faces[lower_bound(&areas, rng.next_f64() * total)])
        .collect::<Vec<_>>();
    let mut samples = Vec::with_capacity(count);
    for face in selected {
        let mut u = rng.next_f64();
        let mut v = rng.next_f64();
        if u + v > 1.0 {
            u = 1.0 - u;
            v = 1.0 - v;
        }
        let (a, b, c) = mesh.triangle_points(face);
        samples.push(DelicacySample {
            point: add(a, add(scale(sub(b, a), u), scale(sub(c, a), v))),
            normal: face_normal(source_index, mesh, face)?,
            face_index: face,
        });
    }
    Ok(samples)
}

struct WeldedVertices {
    vertices: Vec<Vec3>,
    defects: Vec<f64>,
}

fn curvature_scores(
    source_index: usize,
    samples: &[DelicacySample],
    welded: &WeldedVertices,
    thin: f64,
    stats: &mut DelicacyIndexStats,
) -> Result<Vec<f64>, ScarMetricError> {
    let count = samples.len();
    let sub_count = count.min(4_000);
    let indices = integer_linspace(count, sub_count);
    let radius = thin.max(0.5);
    let grid = VertexRadiusGrid::build(&welded.vertices, &welded.defects, radius, source_index)?;
    let mut values = Vec::with_capacity(sub_count);
    for &sample in &indices {
        values.push(grid.signed_abs_sum(samples[sample].point, &mut stats.vertex_grid_candidates));
    }
    let divisor = curvature_divisor(&values);
    let propagator = PointKdTree::build(
        indices
            .iter()
            .map(|&sample| samples[sample].point)
            .collect(),
    );
    let nearest = propagator.nearest_batch(
        &samples
            .iter()
            .map(|sample| sample.point)
            .collect::<Vec<_>>(),
        &mut stats.sample_kd_nodes_visited,
    );
    Ok(nearest
        .into_iter()
        .map(|candidate| (values[candidate] / divisor).clamp(0.0, 1.0))
        .collect())
}

fn thinness_scores(
    source_index: usize,
    mesh: &Mesh,
    samples: &[DelicacySample],
    thin: f64,
    stats: &mut DelicacyIndexStats,
) -> Result<Vec<f64>, ScarMetricError> {
    if !(thin.is_finite() && thin > 0.0) {
        return Err(ScarMetricError::ArithmeticOverflow {
            source_index,
            metric: "thin feature",
        });
    }
    let centres = (0..mesh.triangles().len())
        .map(|face| {
            let (a, b, c) = mesh.triangle_points(face);
            face_normal(source_index, mesh, face)
                .map(|normal| (scale(add(add(a, b), c), 1.0 / 3.0), normal))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let centres_index = PointKdTree::build(centres.iter().map(|(centre, _)| *centre).collect());
    let query = samples
        .iter()
        .map(|sample| sample.point)
        .collect::<Vec<_>>();
    let nearest = centres_index.nearest_k_within_batch(
        &query,
        40.min(centres.len()),
        thin * 4.0,
        &mut stats.centre_kd_nodes_visited,
    );
    nearest
        .into_iter()
        .enumerate()
        .map(|(sample_index, near)| {
            let sample = samples[sample_index];
            let mut thickness = f64::INFINITY;
            // K is selected before filtering normals.  In particular, a
            // 41st opposite face cannot replace one of the first forty.
            for (distance, face) in near {
                if dot(sample.normal, centres[face].1) < -0.3 {
                    thickness = thickness.min(distance);
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

fn welded_defects(source_index: usize, mesh: &Mesh) -> Result<WeldedVertices, ScarMetricError> {
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
        let angle = |left: Vec3, right: Vec3| {
            let denominator = length(left) * length(right);
            (denominator.is_finite() && denominator > 0.0)
                .then(|| (dot(left, right) / denominator).clamp(-1.0, 1.0).acos())
        };
        let first = angle(sub(b, a), sub(c, a));
        let second = angle(sub(a, b), sub(c, b));
        let (Some(first), Some(second)) = (first, second) else {
            return Err(ScarMetricError::ArithmeticOverflow {
                source_index,
                metric: "welded angle defect",
            });
        };
        let third = PI - first - second;
        let angles = if first < 1.0e-8 || second < 1.0e-8 || third < 1.0e-8 {
            [0.0; 3]
        } else {
            [first, second, third]
        };
        for (vertex, angle) in ids.iter().zip(angles) {
            sums[*vertex] += angle;
        }
    }
    Ok(WeldedVertices {
        vertices,
        defects: sums.into_iter().map(|sum| 2.0 * PI - sum).collect(),
    })
}

const AXIS_BARYCENTRIC_EPSILON: f64 = 1.0e-9;
const AXIS_DETERMINANT_EPSILON: f64 = 1.0e-14;
const AXIS_CROSSING_CLUSTER_MM: f64 = 0.005;
const BVH_LEAF_FACES: usize = 12;

#[derive(Clone, Copy)]
struct FaceBounds {
    min: Vec3,
    max: Vec3,
    centre: Vec3,
}

#[derive(Clone, Copy)]
struct BvhNode {
    min: Vec3,
    max: Vec3,
    start: usize,
    end: usize,
    children: Option<(usize, usize)>,
}

/// Deterministic face index owned by this audit module.  It is intentionally
/// separate from the public brute-force geometry APIs, so field construction
/// cannot accidentally inherit producer-side geometry behaviour.
struct TriangleBvh {
    order: Vec<usize>,
    nodes: Vec<BvhNode>,
    root: usize,
}

impl TriangleBvh {
    fn build(mesh: &Mesh) -> Self {
        let bounds = (0..mesh.triangles().len())
            .map(|face| {
                let (a, b, c) = mesh.triangle_points(face);
                let min = [
                    a[0].min(b[0]).min(c[0]),
                    a[1].min(b[1]).min(c[1]),
                    a[2].min(b[2]).min(c[2]),
                ];
                let max = [
                    a[0].max(b[0]).max(c[0]),
                    a[1].max(b[1]).max(c[1]),
                    a[2].max(b[2]).max(c[2]),
                ];
                FaceBounds {
                    min,
                    max,
                    centre: scale(add(add(a, b), c), 1.0 / 3.0),
                }
            })
            .collect::<Vec<_>>();
        let mut order = Vec::with_capacity(bounds.len());
        let mut nodes = Vec::with_capacity(bounds.len() * 2);
        let root = Self::build_node(&bounds, (0..bounds.len()).collect(), &mut order, &mut nodes);
        Self { order, nodes, root }
    }

    fn build_node(
        bounds: &[FaceBounds],
        mut faces: Vec<usize>,
        order: &mut Vec<usize>,
        nodes: &mut Vec<BvhNode>,
    ) -> usize {
        let (min, max) = face_bounds(bounds, &faces);
        let node = nodes.len();
        nodes.push(BvhNode {
            min,
            max,
            start: 0,
            end: 0,
            children: None,
        });
        if faces.len() <= BVH_LEAF_FACES {
            let start = order.len();
            order.append(&mut faces);
            nodes[node].start = start;
            nodes[node].end = order.len();
            return node;
        }
        let extent = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
        let axis = (0..3)
            .max_by(|left, right| extent[*left].total_cmp(&extent[*right]))
            .unwrap_or(0);
        faces.sort_by(|left, right| {
            bounds[*left].centre[axis]
                .total_cmp(&bounds[*right].centre[axis])
                .then(left.cmp(right))
        });
        let right = faces.split_off(faces.len() / 2);
        let left = Self::build_node(bounds, faces, order, nodes);
        let right = Self::build_node(bounds, right, order, nodes);
        nodes[node].children = Some((left, right));
        node
    }

    fn frozen_contains_batch(
        &self,
        mesh: &Mesh,
        points: &[Vec3],
        stats: &mut DelicacyIndexStats,
    ) -> Vec<bool> {
        points
            .iter()
            .map(|point| {
                (0..3)
                    .filter(|axis| self.axis_inside(mesh, *point, *axis, stats))
                    .count()
                    >= 2
            })
            .collect()
    }

    fn axis_inside(
        &self,
        mesh: &Mesh,
        point: Vec3,
        axis: usize,
        stats: &mut DelicacyIndexStats,
    ) -> bool {
        let mut crossings = Vec::new();
        self.axis_crossings_node(mesh, self.root, point, axis, &mut crossings, stats);
        crossings.sort_by(f64::total_cmp);
        let mut clusters = 0;
        let mut previous = None;
        for crossing in crossings {
            if previous.is_none_or(|last: f64| crossing - last > AXIS_CROSSING_CLUSTER_MM) {
                clusters += 1;
            }
            previous = Some(crossing);
        }
        clusters % 2 == 1
    }

    fn axis_crossings_node(
        &self,
        mesh: &Mesh,
        node: usize,
        point: Vec3,
        axis: usize,
        crossings: &mut Vec<f64>,
        stats: &mut DelicacyIndexStats,
    ) {
        stats.triangle_nodes_visited += 1;
        let node = self.nodes[node];
        if node.max[axis] <= point[axis] + AXIS_BARYCENTRIC_EPSILON {
            return;
        }
        if (0..3)
            .filter(|candidate| *candidate != axis)
            .any(|candidate| {
                point[candidate] < node.min[candidate] - AXIS_BARYCENTRIC_EPSILON
                    || point[candidate] > node.max[candidate] + AXIS_BARYCENTRIC_EPSILON
            })
        {
            return;
        }
        if let Some((left, right)) = node.children {
            self.axis_crossings_node(mesh, left, point, axis, crossings, stats);
            self.axis_crossings_node(mesh, right, point, axis, crossings, stats);
            return;
        }
        for &face in &self.order[node.start..node.end] {
            stats.triangle_faces_tested += 1;
            let (a, b, c) = mesh.triangle_points(face);
            if let Some(crossing) = axis_triangle_crossing(point, axis, a, b, c) {
                crossings.push(crossing);
            }
        }
    }
}

fn face_bounds(bounds: &[FaceBounds], faces: &[usize]) -> (Vec3, Vec3) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for &face in faces {
        for axis in 0..3 {
            min[axis] = min[axis].min(bounds[face].min[axis]);
            max[axis] = max[axis].max(bounds[face].max[axis]);
        }
    }
    (min, max)
}

fn point_box_squared_distance(point: Vec3, min: Vec3, max: Vec3) -> f64 {
    (0..3)
        .map(|axis| {
            if point[axis] < min[axis] {
                min[axis] - point[axis]
            } else if point[axis] > max[axis] {
                point[axis] - max[axis]
            } else {
                0.0
            }
        })
        .map(|distance| distance * distance)
        .sum()
}

fn axis_triangle_crossing(point: Vec3, axis: usize, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
    let projected = match axis {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    };
    let ab = sub(b, a);
    let ac = sub(c, a);
    let to_point = sub(point, a);
    let determinant = ab[projected[0]] * ac[projected[1]] - ab[projected[1]] * ac[projected[0]];
    if determinant.abs() <= AXIS_DETERMINANT_EPSILON {
        return None;
    }
    let alpha = (to_point[projected[0]] * ac[projected[1]]
        - to_point[projected[1]] * ac[projected[0]])
        / determinant;
    let beta = (ab[projected[0]] * to_point[projected[1]]
        - ab[projected[1]] * to_point[projected[0]])
        / determinant;
    if alpha < -AXIS_BARYCENTRIC_EPSILON
        || beta < -AXIS_BARYCENTRIC_EPSILON
        || alpha + beta > 1.0 + AXIS_BARYCENTRIC_EPSILON
    {
        return None;
    }
    let crossing = a[axis] + alpha * ab[axis] + beta * ac[axis];
    (crossing > point[axis] + AXIS_BARYCENTRIC_EPSILON).then_some(crossing)
}

#[derive(Clone, Copy)]
struct KdNode {
    point: usize,
    min: Vec3,
    max: Vec3,
    children: [Option<usize>; 2],
}

/// Deterministic point KD tree used for both full sample propagation and
/// bounded face-centre neighbour selection.
struct PointKdTree {
    points: Vec<Vec3>,
    nodes: Vec<KdNode>,
    root: usize,
}

impl PointKdTree {
    fn build(points: Vec<Vec3>) -> Self {
        debug_assert!(!points.is_empty());
        let mut nodes = Vec::with_capacity(points.len());
        let root = Self::build_node(&points, (0..points.len()).collect(), &mut nodes);
        Self {
            points,
            nodes,
            root,
        }
    }

    fn build_node(points: &[Vec3], mut ids: Vec<usize>, nodes: &mut Vec<KdNode>) -> usize {
        let (min, max) = point_bounds(points, &ids);
        let axis = (0..3)
            .max_by(|left, right| (max[*left] - min[*left]).total_cmp(&(max[*right] - min[*right])))
            .unwrap_or(0);
        ids.sort_by(|left, right| {
            points[*left][axis]
                .total_cmp(&points[*right][axis])
                .then(left.cmp(right))
        });
        let median = ids.len() / 2;
        let point = ids[median];
        let right_ids = ids.split_off(median + 1);
        ids.pop();
        let node = nodes.len();
        nodes.push(KdNode {
            point,
            min,
            max,
            children: [None, None],
        });
        let left = (!ids.is_empty()).then(|| Self::build_node(points, ids, nodes));
        let right = (!right_ids.is_empty()).then(|| Self::build_node(points, right_ids, nodes));
        nodes[node].children = [left, right];
        node
    }

    fn nearest_batch(&self, points: &[Vec3], visits: &mut usize) -> Vec<usize> {
        points
            .iter()
            .map(|point| {
                let mut best = (f64::INFINITY, usize::MAX);
                self.nearest_node(self.root, *point, &mut best, visits);
                best.1
            })
            .collect()
    }

    fn nearest_node(&self, node: usize, point: Vec3, best: &mut (f64, usize), visits: &mut usize) {
        *visits += 1;
        let node = self.nodes[node];
        if point_box_squared_distance(point, node.min, node.max) > best.0 {
            return;
        }
        let candidate = (
            distance_squared_between(point, self.points[node.point]),
            node.point,
        );
        if candidate
            .0
            .total_cmp(&best.0)
            .then(candidate.1.cmp(&best.1))
            .is_lt()
        {
            *best = candidate;
        }
        self.visit_nearest_children(node.children, point, best, visits);
    }

    fn nearest_k_within_batch(
        &self,
        points: &[Vec3],
        limit: usize,
        maximum_distance: f64,
        visits: &mut usize,
    ) -> Vec<Vec<(f64, usize)>> {
        points
            .iter()
            .map(|point| {
                let mut nearest = Vec::with_capacity(limit);
                self.nearest_k_node(
                    self.root,
                    *point,
                    limit,
                    maximum_distance,
                    &mut nearest,
                    visits,
                );
                nearest
            })
            .collect()
    }

    fn nearest_k_node(
        &self,
        node: usize,
        point: Vec3,
        limit: usize,
        maximum_distance: f64,
        nearest: &mut Vec<(f64, usize)>,
        visits: &mut usize,
    ) {
        *visits += 1;
        let node = self.nodes[node];
        let box_distance = point_box_squared_distance(point, node.min, node.max).sqrt();
        let worst = if nearest.len() < limit {
            maximum_distance
        } else {
            nearest
                .last()
                .map_or(maximum_distance, |value| value.0.min(maximum_distance))
        };
        if box_distance > worst {
            return;
        }
        let distance = distance(point, self.points[node.point]);
        if distance < maximum_distance {
            insert_nearest(nearest, (distance, node.point), limit);
        }
        self.visit_nearest_k_children(
            node.children,
            point,
            limit,
            maximum_distance,
            nearest,
            visits,
        );
    }

    fn visit_nearest_children(
        &self,
        children: [Option<usize>; 2],
        point: Vec3,
        best: &mut (f64, usize),
        visits: &mut usize,
    ) {
        match children {
            [Some(left), Some(right)] => {
                let left_distance =
                    point_box_squared_distance(point, self.nodes[left].min, self.nodes[left].max);
                let right_distance =
                    point_box_squared_distance(point, self.nodes[right].min, self.nodes[right].max);
                if left_distance <= right_distance {
                    self.nearest_node(left, point, best, visits);
                    self.nearest_node(right, point, best, visits);
                } else {
                    self.nearest_node(right, point, best, visits);
                    self.nearest_node(left, point, best, visits);
                }
            }
            [Some(child), None] | [None, Some(child)] => {
                self.nearest_node(child, point, best, visits)
            }
            [None, None] => {}
        }
    }

    fn visit_nearest_k_children(
        &self,
        children: [Option<usize>; 2],
        point: Vec3,
        limit: usize,
        maximum_distance: f64,
        nearest: &mut Vec<(f64, usize)>,
        visits: &mut usize,
    ) {
        match children {
            [Some(left), Some(right)] => {
                let left_distance =
                    point_box_squared_distance(point, self.nodes[left].min, self.nodes[left].max);
                let right_distance =
                    point_box_squared_distance(point, self.nodes[right].min, self.nodes[right].max);
                if left_distance <= right_distance {
                    self.nearest_k_node(left, point, limit, maximum_distance, nearest, visits);
                    self.nearest_k_node(right, point, limit, maximum_distance, nearest, visits);
                } else {
                    self.nearest_k_node(right, point, limit, maximum_distance, nearest, visits);
                    self.nearest_k_node(left, point, limit, maximum_distance, nearest, visits);
                }
            }
            [Some(child), None] | [None, Some(child)] => {
                self.nearest_k_node(child, point, limit, maximum_distance, nearest, visits);
            }
            [None, None] => {}
        }
    }
}

fn point_bounds(points: &[Vec3], ids: &[usize]) -> (Vec3, Vec3) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for &id in ids {
        for axis in 0..3 {
            min[axis] = min[axis].min(points[id][axis]);
            max[axis] = max[axis].max(points[id][axis]);
        }
    }
    (min, max)
}

/// Exact-coordinate welded vertex grid.  The grid only accelerates radius
/// lookup; identity remains the canonical IEEE key, with +0 and -0 equal.
struct VertexRadiusGrid {
    radius: f64,
    vertices: Vec<Vec3>,
    defects: Vec<f64>,
    cells: BTreeMap<[i64; 3], Vec<usize>>,
}

impl VertexRadiusGrid {
    fn build(
        vertices: &[Vec3],
        defects: &[f64],
        radius: f64,
        source_index: usize,
    ) -> Result<Self, ScarMetricError> {
        if !(radius.is_finite() && radius > 0.0) {
            return Err(ScarMetricError::SpatialIndex {
                source_index,
                index: "welded-vertex radius",
            });
        }
        let mut cells = BTreeMap::new();
        for (index, vertex) in vertices.iter().enumerate() {
            let cell = radius_cell(*vertex, radius).ok_or(ScarMetricError::SpatialIndex {
                source_index,
                index: "welded-vertex radius",
            })?;
            cells.entry(cell).or_insert_with(Vec::new).push(index);
        }
        Ok(Self {
            radius,
            vertices: vertices.to_vec(),
            defects: defects.to_vec(),
            cells,
        })
    }

    fn signed_abs_sum(&self, point: Vec3, candidates: &mut usize) -> f64 {
        let Some(cell) = radius_cell(point, self.radius) else {
            return 0.0;
        };
        let mut sum = 0.0;
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let key = [
                        cell[0].saturating_add(x),
                        cell[1].saturating_add(y),
                        cell[2].saturating_add(z),
                    ];
                    if let Some(vertices) = self.cells.get(&key) {
                        for &vertex in vertices {
                            *candidates += 1;
                            if distance_squared_between(self.vertices[vertex], point)
                                <= self.radius * self.radius
                            {
                                sum += self.defects[vertex];
                            }
                        }
                    }
                }
            }
        }
        sum.abs()
    }
}

fn radius_cell(point: Vec3, radius: f64) -> Option<[i64; 3]> {
    point
        .map(|component| (component / radius).floor())
        .map(|component| {
            (component.is_finite() && component >= i64::MIN as f64 && component <= i64::MAX as f64)
                .then_some(component as i64)
        })
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .and_then(|components| components.try_into().ok())
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
fn normalize_weights(weights: &mut [f64]) {
    let maximum = weights
        .iter()
        .copied()
        .filter(|value| *value > 0.0)
        .fold(0.0_f64, f64::max);
    let divisor = if maximum > 0.0 { maximum } else { 1.0 };
    for weight in weights {
        *weight = (*weight / divisor).clamp(0.0, 1.0);
    }
}
fn curvature_divisor(values: &[f64]) -> f64 {
    let percentile = linear_percentile(values, 95.0);
    let c_hi = if percentile == 0.0 { 1.0 } else { percentile };
    c_hi.max(1.0e-9)
}
fn integer_linspace(length: usize, count: usize) -> Vec<usize> {
    if count == 1 {
        vec![0]
    } else {
        let step = (length - 1) as f64 / (count - 1) as f64;
        (0..count)
            .map(|index| {
                if index + 1 == count {
                    length - 1
                } else {
                    (index as f64 * step).floor() as usize
                }
            })
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
        anchored_entry_is_eligible, contact_patch_area, curvature_divisor,
        endpoint_matches_midpoint, exterior_selection, integer_linspace, linear_percentile,
        normalize_weights, parallel_stored_segments, positive_stored_radius, sample_faces,
        stored_equal, stored_scaled_equal, thinness_scores, welded_defects, DelicacyField,
        DelicacyIndexStats, DelicacySample, LegacyMt19937, PointKdTree, RealInterval, Settings,
        TriangleBvh, VertexRadiusGrid,
    };
    use crate::{Mesh, Vec3};
    #[test]
    fn legacy_seed_zero_matches_numpy_randomstate_prefix() {
        let mut rng = LegacyMt19937::seeded(0);
        assert!((rng.next_f64() - 0.548_813_503_927_324_8).abs() < f64::EPSILON);
        assert!((rng.next_f64() - 0.715_189_366_372_419_5).abs() < f64::EPSILON);
    }
    #[test]
    fn numpy_integer_linspace_and_percentile_are_linear() {
        assert_eq!(integer_linspace(8, 4), [0, 2, 4, 7]);
        assert_eq!(integer_linspace(8, 1), [0]);
        assert_eq!(integer_linspace(4_001, 4_000).last(), Some(&4_000));
        assert!((linear_percentile(&[0.0, 10.0, 20.0, 30.0], 95.0) - 28.5).abs() < 1.0e-12);
    }

    #[test]
    fn sampling_consumes_all_face_uniforms_before_barycentric_uniforms() {
        let mesh = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                [0.0, 2.0, 0.0],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .expect("two valid faces");
        let samples = sample_faces(0, &mesh, &[0, 1], 0, 2_000).expect("legacy sample batch");
        // Golden values deliberately include the RNG position after 2,000
        // face draws; an interleaved implementation produces different bits.
        assert_eq!(
            samples[..3]
                .iter()
                .map(|sample| (sample.face_index, sample.point.map(f64::to_bits)))
                .collect::<Vec<_>>(),
            vec![
                (1, [4_600_462_382_681_693_984, 4_607_397_835_103_780_724, 0]),
                (1, [4_607_390_989_415_816_823, 4_602_688_197_223_824_590, 0]),
                (1, [4_608_128_562_182_748_440, 4_603_631_867_479_093_218, 0]),
            ]
        );
    }

    #[test]
    fn exact_welding_signed_zero_and_signed_defects_are_preserved() {
        let faces = [
            [[0.0, -0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]],
            [[0.0, -0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
        ];
        let vertices = faces.into_iter().flatten().collect::<Vec<_>>();
        let mesh = Mesh::new(vertices, vec![[0, 1, 2], [3, 4, 5], [6, 7, 8], [9, 10, 11]])
            .expect("closed duplicated-corner tetrahedron");
        let welded = welded_defects(0, &mesh).expect("welded tetra defects");
        assert_eq!(welded.vertices.len(), 4);
        assert!((welded.defects.iter().sum::<f64>() - 4.0 * std::f64::consts::PI).abs() < 1.0e-12);

        let grid =
            VertexRadiusGrid::build(&[[0.0, 0.0, 0.0], [0.25, 0.0, 0.0]], &[2.0, -2.0], 0.5, 0)
                .expect("grid");
        let mut candidates = 0;
        assert_eq!(grid.signed_abs_sum([0.0, 0.0, 0.0], &mut candidates), 0.0);
        let distinct = VertexRadiusGrid::build(
            &[[0.0, 0.0, 0.0], [f64::from(f32::from_bits(1)), 0.0, 0.0]],
            &[1.0, 1.0],
            0.5,
            0,
        )
        .expect("exact near-equal vertices remain distinct");
        assert_eq!(distinct.vertices.len(), 2);
    }

    #[test]
    fn kd_selection_is_stable_bounded_and_strict() {
        let tied = PointKdTree::build(vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]]);
        let mut visits = 0;
        assert_eq!(tied.nearest_batch(&[[0.0, 0.0, 0.0]], &mut visits), [0]);
        let forty_one = PointKdTree::build(
            (0..41)
                .map(|index| [index as f64 * 0.01, 0.0, 0.0])
                .collect(),
        );
        let nearest = forty_one.nearest_k_within_batch(&[[0.0, 0.0, 0.0]], 40, 1.0, &mut visits);
        assert_eq!(nearest[0].len(), 40);
        assert_eq!(nearest[0].last().map(|value| value.1), Some(39));
        let strict = tied.nearest_k_within_batch(&[[0.0, 0.0, 0.0]], 2, 1.0, &mut visits);
        assert!(
            strict[0].is_empty(),
            "distance exactly at the bound is excluded"
        );
    }

    #[test]
    fn thinness_uses_the_nearest_forty_before_opposite_filtering() {
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for face in 0..41 {
            let centre = [(face + 1) as f64 * 0.01, 0.0, 0.0];
            let base = vertices.len();
            let points = [
                [centre[0] - 0.001, -0.001, 0.0],
                [centre[0] + 0.001, -0.001, 0.0],
                [centre[0], 0.002, 0.0],
            ];
            vertices.extend(points);
            triangles.push(if face == 40 {
                [base, base + 2, base + 1]
            } else {
                [base, base + 1, base + 2]
            });
        }
        let mesh = Mesh::new(vertices, triangles).expect("forty-one face centres");
        let samples = [DelicacySample {
            point: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            face_index: 0,
        }];
        let mut stats = DelicacyIndexStats::default();
        let scores = thinness_scores(0, &mesh, &samples, 1.0, &mut stats).expect("thinness");
        assert_eq!(scores, [0.0]);
        let (a, b, c) = mesh.triangle_points(40);
        let opposite_centre = reference_scale(reference_add(reference_add(a, b), c), 1.0 / 3.0);
        assert!(
            1.0 - reference_squared_distance(samples[0].point, opposite_centre).sqrt() > 0.0,
            "the excluded 41st opposite face would score hot"
        );
    }

    #[test]
    fn diagnostic_counts_and_points_fail_with_their_source_index() {
        let mesh = cubes_mesh(&[[0.0, 0.0, 0.0]]);
        assert!(matches!(
            DelicacyField::build_diagnostic(73, &mesh, 9, 0),
            Err(super::ScarMetricError::InvalidSampleCount {
                source_index: 73,
                n_samples: 0
            })
        ));
        let field = DelicacyField::build_diagnostic(73, &mesh, 9, 7).expect("seven samples");
        assert_eq!(field.samples().len(), 7);
        assert!(matches!(
            field.query_points(73, &[[0.0, 0.0, 0.0], [0.0, 1.0, f64::NAN]]),
            Err(super::ScarMetricError::InvalidDiagnosticPoint {
                source_index: 73,
                point: 1,
                component: 2
            })
        ));
    }

    #[test]
    fn parity_only_exterior_does_not_promote_near_surface_probes() {
        let normal = reference_normal([1.0, -1.0, 0.0], [0.0, 1.0, -1.0], [-1.0, 0.0, 1.0]);
        let base = [[1.0, -1.0, 0.0], [0.0, 1.0, -1.0], [-1.0, 0.0, 1.0]];
        let first = tetra_soup(base, reference_scale(normal, -2.0));
        let second_base = base.map(|point| reference_add(point, reference_scale(normal, 0.05)));
        let second = tetra_soup(second_base, reference_scale(normal, -2.0));
        let third_base = base.map(|point| {
            reference_add(reference_scale(point, 10.0), reference_scale(normal, 0.10))
        });
        let third = tetra_soup(third_base, reference_scale(normal, 5.0));
        let vertices = first
            .into_iter()
            .chain(second)
            .chain(third)
            .collect::<Vec<_>>();
        let triangles = (0..12)
            .map(|face| [face * 3, face * 3 + 1, face * 3 + 2])
            .collect::<Vec<_>>();
        let mesh = Mesh::new(vertices, triangles).expect("three tetrahedra");

        let candidate = base;
        let probes = [[1.0 / 3.0; 3], [0.6, 0.2, 0.2], [0.2, 0.6, 0.2]]
            .into_iter()
            .flat_map(|weights| {
                let point = reference_add(
                    reference_scale(candidate[0], weights[0]),
                    reference_add(
                        reference_scale(candidate[1], weights[1]),
                        reference_scale(candidate[2], weights[2]),
                    ),
                );
                [0.05, 0.15].map(|offset| reference_add(point, reference_scale(normal, offset)))
            })
            .collect::<Vec<_>>();
        for pair in probes.chunks_exact(2) {
            assert!(
                reference_dot(reference_sub(pair[0], second_base[0]), normal).abs() < 1.0e-12,
                "the first probe is on the middle tetrahedron's face"
            );
            assert!(!reference_contains(&mesh, pair[0]));
            assert!(reference_contains(&mesh, pair[1]));
        }
        assert_eq!(
            probes
                .iter()
                .filter(|point| reference_contains(&mesh, **point))
                .count(),
            3,
            "parity alone sees only the three probes inside the upper tetrahedron"
        );
        let bvh = TriangleBvh::build(&mesh);
        let mut stats = DelicacyIndexStats::default();
        let inside = bvh.frozen_contains_batch(&mesh, &probes, &mut stats);
        assert_eq!(inside.iter().filter(|inside| **inside).count(), 3);
        let selection = exterior_selection(0, &mesh, &bvh, &mut stats).expect("exterior");
        assert!(
            selection.mask[0],
            "three of six inside leaves the candidate exterior"
        );
    }

    #[test]
    fn frozen_exterior_subset_retains_original_face_mapping() {
        let outer = cube_soup(2.0, [0.0, 0.0, 0.0]);
        let inner = cube_soup(0.5, [0.0, 0.0, 0.0]);
        let vertices = outer.into_iter().chain(inner).collect::<Vec<_>>();
        let triangles = (0..24)
            .map(|face| [face * 3, face * 3 + 1, face * 3 + 2])
            .collect::<Vec<_>>();
        let mesh = Mesh::new(vertices, triangles).expect("nested cubes");
        let bvh = TriangleBvh::build(&mesh);
        let mut stats = DelicacyIndexStats::default();
        let selection = exterior_selection(0, &mesh, &bvh, &mut stats).expect("exterior field");
        assert_eq!(selection.selected_faces, (0..12).collect::<Vec<_>>());
        assert!(
            stats.triangle_faces_tested < 24 * 24 * 6 * 4,
            "BVH avoids the complete parity Cartesian scan"
        );
    }

    #[test]
    fn frozen_exterior_keeps_odd_coverage_for_overlap_and_triple_overlap() {
        for centres in [
            vec![[0.0, 0.0, 0.0], [0.75, 0.0, 0.0]],
            vec![[-0.75, 0.0, 0.0], [0.0, 0.0, 0.0], [0.75, 0.0, 0.0]],
        ] {
            let mesh = cubes_mesh(&centres);
            let expected = reference_exterior(&mesh);
            let bvh = TriangleBvh::build(&mesh);
            let mut stats = DelicacyIndexStats::default();
            let selection =
                exterior_selection(0, &mesh, &bvh, &mut stats).expect("frozen exterior");
            assert_eq!(
                selection.selected_faces, expected.1,
                "independent parity oracle"
            );
            assert_eq!(selection.mask, expected.0, "complete exterior mask");
            assert!(
                selection.selected_faces.len() < mesh.triangles().len()
                    && !selection.selected_faces.is_empty(),
                "overlap produces a proper frozen exterior subset"
            );
            let samples = sample_faces(0, &mesh, &selection.selected_faces, 23, 2_000)
                .expect("mapped samples");
            assert!(samples.iter().all(|sample| {
                sample.normal == super::face_normal(0, &mesh, sample.face_index).expect("normal")
            }));
        }
    }

    #[test]
    fn batch_contact_lookup_uses_nearest_sample_index_ties() {
        let field = DelicacyField {
            exterior_mask: vec![true, true],
            selected_faces: vec![9, 4],
            samples: vec![
                DelicacySample {
                    point: [-1.0, 0.0, 0.0],
                    normal: [1.0, 0.0, 0.0],
                    face_index: 9,
                },
                DelicacySample {
                    point: [1.0, 0.0, 0.0],
                    normal: [-1.0, 0.0, 0.0],
                    face_index: 4,
                },
            ],
            curvature_scores: vec![0.0, 0.0],
            thinness_scores: vec![0.0, 0.0],
            weights: vec![0.25, 0.75],
            sample_index: PointKdTree::build(vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]]),
            stats: DelicacyIndexStats::default(),
        };
        assert_eq!(
            field
                .query_points(0, &[[0.0, 0.0, 0.0]])
                .expect("batch lookup")
                .weights,
            [0.25]
        );
    }

    #[test]
    fn indexed_triangle_batch_avoids_representative_cartesian_scan() {
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        let mut points = Vec::new();
        for y in 0..16 {
            for x in 0..16 {
                let base = vertices.len();
                let origin = [x as f64 * 10.0, y as f64 * 10.0, 0.0];
                vertices.extend([
                    origin,
                    [origin[0] + 1.0, origin[1], 0.0],
                    [origin[0], origin[1] + 1.0, 0.0],
                ]);
                triangles.push([base, base + 1, base + 2]);
                points.push([origin[0] + 0.2, origin[1] + 0.2, 0.4]);
            }
        }
        let mesh = Mesh::new(vertices, triangles).expect("separated field faces");
        let bvh = TriangleBvh::build(&mesh);
        let mut stats = DelicacyIndexStats::default();
        let inside = bvh.frozen_contains_batch(&mesh, &points, &mut stats);
        assert_eq!(inside, vec![false; points.len()]);
        assert!(
            stats.triangle_faces_tested < mesh.triangles().len() * points.len() / 8,
            "test-only visit count proves the indexed batch does not scan every face"
        );
    }

    #[test]
    fn closed_curvature_radius_includes_its_boundary_only() {
        let grid = VertexRadiusGrid::build(&[[0.0, 0.0, 0.0]], &[3.0], 0.5, 0).expect("grid");
        let mut candidates = 0;
        assert_eq!(grid.signed_abs_sum([0.5, 0.0, 0.0], &mut candidates), 3.0);
        assert_eq!(
            grid.signed_abs_sum(
                [f64::from_bits(0.5_f64.to_bits() + 1), 0.0, 0.0],
                &mut candidates
            ),
            0.0
        );
    }

    #[test]
    fn indexed_field_matches_the_straightforward_complete_small_mesh_oracle() {
        let outer = cube_soup(2.0, [0.0, 0.0, 0.0]);
        let inner = cube_soup(0.5, [0.0, 0.0, 0.0]);
        let vertices = outer.into_iter().chain(inner).collect::<Vec<_>>();
        let triangles = (0..24)
            .map(|face| [face * 3, face * 3 + 1, face * 3 + 2])
            .collect::<Vec<_>>();
        let mesh = Mesh::new(vertices, triangles).expect("nested cubes");
        let settings = test_settings();
        let bvh = TriangleBvh::build(&mesh);
        let mut stats = DelicacyIndexStats::default();
        let selection = exterior_selection(0, &mesh, &bvh, &mut stats).expect("indexed exterior");
        let (expected_mask, expected_selected) = reference_exterior(&mesh);
        assert_eq!(selection.mask, expected_mask, "complete exterior mask");
        assert_eq!(
            selection.selected_faces, expected_selected,
            "selected original face ids"
        );

        let expected_samples = reference_samples(&mesh, &expected_selected, 17, 2_000);
        let field = DelicacyField::build(0, &mesh, settings, 17, 2_000).expect("indexed field");
        assert_eq!(field.exterior_mask(), expected_mask);
        assert_eq!(field.selected_face_indices(), expected_selected);
        assert_eq!(field.samples.len(), expected_samples.len());
        for (actual, expected) in field.samples.iter().zip(&expected_samples) {
            assert_eq!(actual.face_index, expected.face_index);
            assert_eq!(
                actual.point.map(f64::to_bits),
                expected.point.map(f64::to_bits)
            );
            assert_eq!(
                actual.normal.map(f64::to_bits),
                expected.normal.map(f64::to_bits)
            );
        }

        let (vertices, defects) = reference_welded_defects(&mesh);
        let expected_curvature = reference_curvature(&field.samples, &vertices, &defects, 1.5);
        let expected_thinness = reference_thinness(&mesh, &field.samples, 1.5);
        let expected_weights = reference_blend(&expected_curvature, &expected_thinness);
        assert_close_slices(field.curvature_scores(), &expected_curvature);
        assert_close_slices(field.thinness_scores(), &expected_thinness);
        assert_close_slices(field.weights(), &expected_weights);

        let contact = [[0.0, 0.0, 1.0], [0.0, 0.0, 0.25]];
        let expected_lookup = contact
            .iter()
            .map(|point| {
                let sample = field
                    .samples
                    .iter()
                    .enumerate()
                    .min_by(|(left, left_sample), (right, right_sample)| {
                        reference_squared_distance(*point, left_sample.point)
                            .total_cmp(&reference_squared_distance(*point, right_sample.point))
                            .then(left.cmp(right))
                    })
                    .expect("samples")
                    .0;
                field.weights[sample]
            })
            .collect::<Vec<_>>();
        assert_eq!(
            field
                .query_points(0, &contact)
                .expect("indexed contacts")
                .weights,
            expected_lookup
        );
    }

    #[test]
    fn curvature_and_blend_zero_fallbacks_are_explicit() {
        assert_eq!(curvature_divisor(&[0.0, 0.0]), 1.0);
        assert!((curvature_divisor(&[0.0, 2.0, 4.0]) - 3.8).abs() < 1.0e-12);
        let mut zero = vec![0.0, 0.0];
        normalize_weights(&mut zero);
        assert_eq!(zero, [0.0, 0.0]);
        let mut mixed = vec![0.0, 0.5, 2.0];
        normalize_weights(&mut mixed);
        assert_eq!(mixed, [0.0, 0.25, 1.0]);
    }

    fn test_settings() -> Settings {
        Settings {
            curvature_weight: 0.6,
            thinness_weight: 0.4,
            thin_feature_mm: 1.5,
            eps_cos: 1.0e-9,
            tol_mm: 0.05,
            stub_factor: 1.5,
            foot_factor: 1.4,
        }
    }

    fn reference_exterior(mesh: &Mesh) -> (Vec<bool>, Vec<usize>) {
        let mask = (0..mesh.triangles().len())
            .map(|face| {
                let (a, b, c) = mesh.triangle_points(face);
                let normal = reference_normal(a, b, c);
                let inside = [[1.0 / 3.0; 3], [0.6, 0.2, 0.2], [0.2, 0.6, 0.2]]
                    .into_iter()
                    .flat_map(|barycentric| {
                        let point = reference_add(
                            reference_scale(a, barycentric[0]),
                            reference_add(
                                reference_scale(b, barycentric[1]),
                                reference_scale(c, barycentric[2]),
                            ),
                        );
                        [0.05, 0.15]
                            .map(|offset| reference_add(point, reference_scale(normal, offset)))
                    })
                    .filter(|point| reference_contains(mesh, *point))
                    .count();
                inside < 4
            })
            .collect::<Vec<_>>();
        let exterior = mask
            .iter()
            .enumerate()
            .filter_map(|(face, exterior)| exterior.then_some(face))
            .collect::<Vec<_>>();
        let selected = if exterior.is_empty() || exterior.len() == mesh.triangles().len() {
            (0..mesh.triangles().len()).collect()
        } else {
            exterior
        };
        (mask, selected)
    }

    fn reference_contains(mesh: &Mesh, point: Vec3) -> bool {
        (0..3)
            .filter(|axis| {
                let mut crossings = (0..mesh.triangles().len())
                    .filter_map(|face| {
                        let (a, b, c) = mesh.triangle_points(face);
                        reference_axis_crossing(point, *axis, a, b, c)
                    })
                    .collect::<Vec<_>>();
                crossings.sort_by(f64::total_cmp);
                let mut clusters = 0;
                let mut previous = None;
                for crossing in crossings {
                    if previous.is_none_or(|last: f64| crossing - last > 0.005) {
                        clusters += 1;
                    }
                    previous = Some(crossing);
                }
                clusters % 2 == 1
            })
            .count()
            >= 2
    }

    fn reference_axis_crossing(point: Vec3, axis: usize, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
        let projected = match axis {
            0 => [1, 2],
            1 => [0, 2],
            _ => [0, 1],
        };
        let ab = reference_sub(b, a);
        let ac = reference_sub(c, a);
        let from_a = reference_sub(point, a);
        let denominator = ab[projected[0]] * ac[projected[1]] - ab[projected[1]] * ac[projected[0]];
        if denominator.abs() <= 1.0e-14 {
            return None;
        }
        let first = (from_a[projected[0]] * ac[projected[1]]
            - from_a[projected[1]] * ac[projected[0]])
            / denominator;
        let second = (ab[projected[0]] * from_a[projected[1]]
            - ab[projected[1]] * from_a[projected[0]])
            / denominator;
        if first < -1.0e-9 || second < -1.0e-9 || first + second > 1.0 + 1.0e-9 {
            return None;
        }
        let crossing = a[axis] + first * ab[axis] + second * ac[axis];
        (crossing > point[axis] + 1.0e-9).then_some(crossing)
    }

    fn reference_samples(
        mesh: &Mesh,
        faces: &[usize],
        seed: u32,
        count: usize,
    ) -> Vec<DelicacySample> {
        let mut cumulative = Vec::with_capacity(faces.len());
        let mut total = 0.0;
        for &face in faces {
            let (a, b, c) = mesh.triangle_points(face);
            total +=
                0.5 * reference_length(reference_cross(reference_sub(b, a), reference_sub(c, a)));
            cumulative.push(total);
        }
        let mut rng = ReferenceMt19937::seeded(seed);
        let selected = (0..count)
            .map(|_| {
                let target = rng.next_f64() * total;
                let local = cumulative
                    .iter()
                    .position(|value| *value >= target)
                    .unwrap_or(cumulative.len() - 1);
                faces[local]
            })
            .collect::<Vec<_>>();
        selected
            .into_iter()
            .map(|face| {
                let mut first = rng.next_f64();
                let mut second = rng.next_f64();
                if first + second > 1.0 {
                    first = 1.0 - first;
                    second = 1.0 - second;
                }
                let (a, b, c) = mesh.triangle_points(face);
                DelicacySample {
                    point: reference_add(
                        a,
                        reference_add(
                            reference_scale(reference_sub(b, a), first),
                            reference_scale(reference_sub(c, a), second),
                        ),
                    ),
                    normal: reference_normal(a, b, c),
                    face_index: face,
                }
            })
            .collect()
    }

    struct ReferenceMt19937 {
        state: [u32; 624],
        index: usize,
    }

    impl ReferenceMt19937 {
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

        fn next_u32(&mut self) -> u32 {
            if self.index == 624 {
                for index in 0..624 {
                    let combined = (self.state[index] & 0x8000_0000)
                        | (self.state[(index + 1) % 624] & 0x7fff_ffff);
                    self.state[index] = self.state[(index + 397) % 624]
                        ^ (combined >> 1)
                        ^ if combined & 1 == 0 { 0 } else { 0x9908_b0df };
                }
                self.index = 0;
            }
            let mut value = self.state[self.index];
            self.index += 1;
            value ^= value >> 11;
            value ^= (value << 7) & 0x9d2c_5680;
            value ^= (value << 15) & 0xefc6_0000;
            value ^ (value >> 18)
        }

        fn next_f64(&mut self) -> f64 {
            let high = (self.next_u32() >> 5) as f64;
            let low = (self.next_u32() >> 6) as f64;
            (high * 67_108_864.0 + low) / 9_007_199_254_740_992.0
        }
    }

    fn reference_welded_defects(mesh: &Mesh) -> (Vec<Vec3>, Vec<f64>) {
        let mut ids = std::collections::BTreeMap::new();
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        for face in 0..mesh.triangles().len() {
            let (a, b, c) = mesh.triangle_points(face);
            let mut face_ids = [0; 3];
            for (corner, point) in [a, b, c].into_iter().enumerate() {
                let key = point.map(|coordinate| {
                    if coordinate == 0.0 {
                        0
                    } else {
                        coordinate.to_bits()
                    }
                });
                let next = vertices.len();
                face_ids[corner] = *ids.entry(key).or_insert_with(|| {
                    vertices.push(point);
                    next
                });
            }
            faces.push(face_ids);
        }
        let mut sums = vec![0.0; vertices.len()];
        for (face, ids) in faces.iter().enumerate() {
            let (a, b, c) = mesh.triangle_points(face);
            let angle = |left: Vec3, right: Vec3| {
                (reference_dot(left, right) / (reference_length(left) * reference_length(right)))
                    .clamp(-1.0, 1.0)
                    .acos()
            };
            let first = angle(reference_sub(b, a), reference_sub(c, a));
            let second = angle(reference_sub(a, b), reference_sub(c, b));
            let third = std::f64::consts::PI - first - second;
            let angles = if [first, second, third]
                .into_iter()
                .any(|value| value < 1.0e-8)
            {
                [0.0; 3]
            } else {
                [first, second, third]
            };
            for (vertex, angle) in ids.iter().zip(angles) {
                sums[*vertex] += angle;
            }
        }
        (
            vertices,
            sums.into_iter()
                .map(|sum| 2.0 * std::f64::consts::PI - sum)
                .collect(),
        )
    }

    fn reference_curvature(
        samples: &[DelicacySample],
        vertices: &[Vec3],
        defects: &[f64],
        thin: f64,
    ) -> Vec<f64> {
        let indices = reference_linspace(samples.len(), samples.len().min(4_000));
        let radius = thin.max(0.5);
        let values = indices
            .iter()
            .map(|sample| {
                vertices
                    .iter()
                    .zip(defects)
                    .filter(|(vertex, _)| {
                        reference_squared_distance(**vertex, samples[*sample].point)
                            <= radius * radius
                    })
                    .map(|(_, defect)| *defect)
                    .sum::<f64>()
                    .abs()
            })
            .collect::<Vec<_>>();
        let percentile = reference_percentile(&values, 0.95);
        let divisor = if percentile == 0.0 { 1.0 } else { percentile }.max(1.0e-9);
        samples
            .iter()
            .map(|sample| {
                let nearest = indices
                    .iter()
                    .enumerate()
                    .min_by(|(left, left_index), (right, right_index)| {
                        reference_squared_distance(sample.point, samples[**left_index].point)
                            .total_cmp(&reference_squared_distance(
                                sample.point,
                                samples[**right_index].point,
                            ))
                            .then(left.cmp(right))
                    })
                    .expect("curvature samples")
                    .0;
                (values[nearest] / divisor).clamp(0.0, 1.0)
            })
            .collect()
    }

    fn reference_linspace(length: usize, count: usize) -> Vec<usize> {
        if count == 1 {
            return vec![0];
        }
        let step = (length - 1) as f64 / (count - 1) as f64;
        let mut values = (0..count)
            .map(|index| (index as f64 * step).floor() as usize)
            .collect::<Vec<_>>();
        values[count - 1] = length - 1;
        values
    }

    fn reference_percentile(values: &[f64], quantile: f64) -> f64 {
        let mut values = values.to_vec();
        values.sort_by(f64::total_cmp);
        let position = quantile * (values.len() - 1) as f64;
        let lower = position.floor() as usize;
        let upper = position.ceil() as usize;
        values[lower] + (values[upper] - values[lower]) * (position - lower as f64)
    }

    fn reference_thinness(mesh: &Mesh, samples: &[DelicacySample], thin: f64) -> Vec<f64> {
        let centres = (0..mesh.triangles().len())
            .map(|face| {
                let (a, b, c) = mesh.triangle_points(face);
                (
                    reference_scale(reference_add(reference_add(a, b), c), 1.0 / 3.0),
                    reference_normal(a, b, c),
                )
            })
            .collect::<Vec<_>>();
        samples
            .iter()
            .map(|sample| {
                let mut near = centres
                    .iter()
                    .enumerate()
                    .filter_map(|(face, (centre, _))| {
                        let distance = reference_squared_distance(sample.point, *centre).sqrt();
                        (distance < 4.0 * thin).then_some((distance, face))
                    })
                    .collect::<Vec<_>>();
                near.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
                near.truncate(40);
                let thickness = near
                    .into_iter()
                    .filter(|(_, face)| reference_dot(sample.normal, centres[*face].1) < -0.3)
                    .map(|(distance, _)| distance)
                    .fold(f64::INFINITY, f64::min);
                if thickness.is_finite() {
                    (1.0 - thickness / thin).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            })
            .collect()
    }

    fn reference_blend(curvature: &[f64], thinness: &[f64]) -> Vec<f64> {
        let mut values = curvature
            .iter()
            .zip(thinness)
            .map(|(curvature, thinness)| 0.6 * curvature + 0.4 * thinness)
            .collect::<Vec<_>>();
        let maximum = values
            .iter()
            .copied()
            .filter(|value| *value > 0.0)
            .fold(0.0_f64, f64::max);
        let divisor = if maximum > 0.0 { maximum } else { 1.0 };
        for value in &mut values {
            *value = (*value / divisor).clamp(0.0, 1.0);
        }
        values
    }

    fn reference_add(left: Vec3, right: Vec3) -> Vec3 {
        [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
    }

    fn reference_sub(left: Vec3, right: Vec3) -> Vec3 {
        [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
    }

    fn reference_scale(value: Vec3, factor: f64) -> Vec3 {
        [value[0] * factor, value[1] * factor, value[2] * factor]
    }

    fn reference_dot(left: Vec3, right: Vec3) -> f64 {
        left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
    }

    fn reference_cross(left: Vec3, right: Vec3) -> Vec3 {
        [
            left[1] * right[2] - left[2] * right[1],
            left[2] * right[0] - left[0] * right[2],
            left[0] * right[1] - left[1] * right[0],
        ]
    }

    fn reference_length(value: Vec3) -> f64 {
        reference_dot(value, value).sqrt()
    }

    fn reference_squared_distance(left: Vec3, right: Vec3) -> f64 {
        reference_dot(reference_sub(left, right), reference_sub(left, right))
    }

    fn reference_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
        let value = reference_cross(reference_sub(b, a), reference_sub(c, a));
        reference_scale(value, 1.0 / reference_length(value))
    }

    fn assert_close_slices(left: &[f64], right: &[f64]) {
        assert_eq!(left.len(), right.len());
        for (index, (left, right)) in left.iter().zip(right).enumerate() {
            assert!(
                (left - right).abs() < 1.0e-12,
                "index {index}: {left} != {right}"
            );
        }
    }

    fn cube_soup(side: f64, centre: Vec3) -> Vec<Vec3> {
        let h = side * 0.5;
        let vertices = [
            [-h, -h, -h],
            [h, -h, -h],
            [h, h, -h],
            [-h, h, -h],
            [-h, -h, h],
            [h, -h, h],
            [h, h, h],
            [-h, h, h],
        ]
        .map(|point| {
            [
                point[0] + centre[0],
                point[1] + centre[1],
                point[2] + centre[2],
            ]
        });
        [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ]
        .into_iter()
        .flat_map(|triangle| triangle.into_iter().map(|index| vertices[index]))
        .collect()
    }

    fn cubes_mesh(centres: &[Vec3]) -> Mesh {
        let vertices = centres
            .iter()
            .flat_map(|centre| cube_soup(2.0, *centre))
            .collect::<Vec<_>>();
        let triangles = (0..centres.len() * 12)
            .map(|face| [face * 3, face * 3 + 1, face * 3 + 2])
            .collect::<Vec<_>>();
        Mesh::new(vertices, triangles).expect("cube soups")
    }

    fn tetra_soup(base: [Vec3; 3], apex: Vec3) -> Vec<Vec3> {
        [
            [base[0], base[1], base[2]],
            [base[0], apex, base[1]],
            [base[1], apex, base[2]],
            [base[2], apex, base[0]],
        ]
        .into_iter()
        .flatten()
        .collect()
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
