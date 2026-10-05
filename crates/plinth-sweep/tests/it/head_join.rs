//! **A lander joins a support arm far enough below its head that the section
//! carrying the sum clears the seat** (head-join, 2026-09-29).
//!
//! # The defect
//!
//! After the contact-sizing rule the ledger's contact slot holds the arm's own
//! peel, and the lander's load is booked one slot under it. On a LEANED head
//! arm the drawn shaft is then sum-sized from 0.05 mm under the seat plane,
//! and its tilted ring rises through the plane: the Round 25 ring stiffener's
//! arms 39, 41 and 44 lean about 53°, and their sum-sized `Member` span starts
//! one slot under the seat, crosses the plane and is trimmed there. The file's
//! contact is the own-sized ball joined with a trimmed sum-sized section; on
//! arms 39 and 44 that section is wider than the ball
//! (`plinth-evidence/.../field-2026-09-28/lanes/contact-sizing.md` §5).
//!
//! # The rule graded here
//!
//! Every support arm a member lands on, AT ANY LAYER, is drawn, in its seat
//! zone, at its OWN load's size only: every section of every shaft record
//! that carries a lander's load (at or under the highest layer a member lands
//! on it at) and whose ring comes within the registered final clearance
//! (`FINAL_CLEARANCE_GAP_MM`, 0.3 mm) of the arm's seat plane AND whose disc
//! comes within it of the model itself is no wider than the arm's own contact
//! radius, and no such section wider than that reaches the plane at all.
//! Where one is left (the rule found no admissible join), the census names
//! exactly that arm.
//!
//! The scope is geometry, not the landing layer: `cb28bdd` graded (and moved)
//! head landings only, and on Round 50 a lander one or three layers under a
//! ~57° head, booked into the same slot, kept its sum across the seat plane
//! by 0.33 to 0.48 mm (arms 66, 68, 83, 98), unwarned. The model half of the
//! zone keeps the infinite seat plane honest under a steep face (Round 25
//! arm 6: the plane passes below the model where its lander joins).
//!
//! # How it is graded
//!
//! On the production lowering's own records (`plinth_support::lower_sweep_arms`
//! of `arms_of`), against the seat plane the production put on the arm's tip
//! ball. Each `ContactTaper` and `Member` record is cut into stations along
//! its axis; at each station the ring (the record's own centre line, its
//! linearly interpolated radius, normal to its axis) is SAMPLED at 64 points
//! and measured against the plane, and its disc (9 rings of 64 points)
//! against every model triangle by brute force. That is the grader's own
//! method, not the rule's closed form or the kernel's predicate. The own
//! contact radius is the ledger's contact slot, checked against the
//! registered area law of that slot's load.
//!
//! The fixtures are the Round 25 and Round 50 ratchet discs at their
//! committed flip (the pose is applied directly, not searched), where
//! cavity-ceiling arms lean across and land on or near the ring stiffener
//! arms' heads.
//!
//! Sabotage: `SweepPolicy::without_head_join` (the placement as it was before
//! the rule) must put a sum-sized section across a seat plane again, and
//! `the_seat_zone_of_every_head_arm_is_own_load_sized` is red on it (recorded in
//! the lane report, `lanes/head-join.md`).

use crate::contact_sizing::flipped;
use crate::harness::{x_allowed, LIFT_MM};
use crate::round_bases_ratchet::{mesh_for, place_with, round_25, round_50};

use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_mesh::Mesh;
use plinth_support::{
    lower_sweep_arms, BuildFrame, FrustumRingNormal, SeatTrim, SupportPrimitive,
    SupportSegmentKind, SweepArm,
};
// (`SweepArm` is also the unit the moved-member clearance is graded on.)
use plinth_sweep::{arms_of, Footprint, Landing, StrutKind, SweepPlan, SweepPolicy};

/// Stations per shaft record, ends included.
const STATIONS: usize = 33;
/// Samples per ring.
const RING_SAMPLES: usize = 64;
/// Relative slack on "no wider than": float noise around an equality the
/// construction designs for, and nothing more.
const RADIUS_SLACK: f64 = 1.0e-9;

fn final_clearance_mm() -> f64 {
    *m0_constants()
        .expect("registry")
        .evidence()
        .final_clearance_gap_mm()
        .value()
}

fn round_25_flipped_with(policy: &SweepPolicy) -> SweepPlan {
    place_with(&flipped(&mesh_for(&round_25())), "Round 25 flipped", policy)
}

/// The production policy WITH THE LIP-SEAT RULE OFF (re-scoped 2026-09-29,
/// fix/lip-seat). The head-join fixtures exist for their leaned heads (Round 25
/// arms 39, 41, 44 at ~53° from vertical) with a lander's sum-sized section
/// in the seat zone. The lip-seat rule, which runs before the head-join rule,
/// re-lands exactly those heads to meet their faces within the angle at which
/// the seat trim keeps their section, so on the production field this fixture
/// no longer shows the defect this rule exists for (measured: 0 joins in a
/// seat zone, rule on or off). These tests grade the head-join rule on the
/// field it was built against; `plinth-pipeline` `lip_seat::` grades the
/// production field.
///
/// AND AT THE SEAT PLACEMENT THE FIXTURE WAS BUILT FOR (2026-09-30,
/// `harness::at_path_fixture_seating`): under the straight-line reach and the
/// island seating, Round 25's rim is seated as one row and no arm lands on a
/// leaned head any more (measured: 0 heads with landers on Round 25, 0 joins
/// in a seat zone on Round 50), so every test here would grade nothing. The
/// head-join rule reads no placement.
///
/// **Re-pinned 2026-10-04 (fix/drawn-solid-clearance):** and under the
/// centreline-only descent these fixtures were built on. The flipped Round 25
/// and 50 seat arms on the ring lip's drafted wall and run along it; the
/// drawn-solid descent refuses those descents, replaces them, and the heads
/// the tests below grade (a lander on a leaned head, a head already clear)
/// no longer occur on the field: measured 0 joins in a seat zone before, 0
/// heads already clear, so each would compare nothing. The head-join rule
/// reads no placement clearance. (The production Round discs, as drawn, are
/// byte-identical under the new descent.)
fn production() -> SweepPolicy {
    crate::harness::at_path_fixture_seating(
        SweepPolicy::from_registry()
            .expect("the registry supplies every sweep number")
            .without_lip_seat()
            .without_drawn_solid_descent(),
    )
}

/// `r = sqrt(ℓ / (X · π))` inside the registered clamps: the area law, read
/// from the registry here rather than from the crate under test.
fn area_law_radius_mm(carried_mm2: f64) -> f64 {
    let constants = m0_constants().expect("the registry constructs");
    let placeholders = constants.placeholders();
    let minimum = placeholders.tip_radius_default_mm().diagnostic().get();
    let maximum = placeholders.tip_radius_max_legacy_mm().diagnostic().get();
    (carried_mm2 / (x_allowed() * core::f64::consts::PI))
        .sqrt()
        .clamp(minimum, maximum)
}

/// Lower ONE arm with the production lowering.
fn lower_one(plan: &SweepPlan, arm: &SweepArm) -> Vec<SupportPrimitive> {
    let height = plan.layer_height_mm;
    let layers = LayerPlan::new(plan.plate_z_mm, vec![height; plan.layers.max(1)])
        .expect("a uniform layer plan");
    let frame = BuildFrame::new(
        Finite::new(plan.plate_z_mm).expect("finite plate"),
        PositiveFinite::new(LIFT_MM).expect("positive lift"),
    );
    lower_sweep_arms(std::slice::from_ref(arm), frame, &layers, 1_000, 1 << 24)
        .expect("the lowering refuses only about the request")
        .primitives
}

/// The lowest signed distance (support side positive) any sample of the ring
/// of radius `radius` about `centre`, normal to `axis`, reaches from `trim`'s
/// plane.
fn ring_lowest(centre: [f64; 3], radius: f64, axis: [f64; 3], trim: SeatTrim) -> f64 {
    let reference = if axis[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let e1 = geom_kernel::vec3::normalize(geom_kernel::vec3::cross(axis, reference))
        .expect("a ring direction");
    let e2 = geom_kernel::vec3::cross(axis, e1);
    let mut lowest = f64::INFINITY;
    for step in 0..RING_SAMPLES {
        #[expect(clippy::cast_precision_loss, reason = "a sample index below 64")]
        let angle = std::f64::consts::TAU * step as f64 / RING_SAMPLES as f64;
        let point = geom_kernel::vec3::add(
            centre,
            geom_kernel::vec3::add(
                geom_kernel::vec3::scale(e1, radius * angle.cos()),
                geom_kernel::vec3::scale(e2, radius * angle.sin()),
            ),
        );
        lowest = lowest.min(trim.support_side_distance_mm(point));
    }
    lowest
}

/// What the grader found on one trunk.
#[derive(Debug, Default)]
struct Trunk {
    id: u32,
    own_mm: f64,
    /// Stations in the seat zone: the ring within the final clearance of the
    /// seat plane AND the section within it of the model itself.
    stations_in_zone: usize,
    /// Widest section within the seat zone.
    widest_in_zone_mm: f64,
    /// Lowest ring reach, from the seat plane, of any section WIDER than the
    /// own contact whose disc is within the final clearance of the model.
    lowest_wider_mm: f64,
    /// Diagnostic: the same reach over EVERY wider section, the model's
    /// distance not asked (the infinite plane alone), and the model's distance
    /// at that section.
    lowest_wider_plane_only_mm: f64,
    model_gap_there_mm: f64,
}

/// Every SUPPORT ARM a member lands on, at ANY layer, in any of `plans`.
/// Which of them has a seat zone to grade is decided by geometry in
/// [`grade`], not by the landing layer: a lander one or three layers under a
/// leaned head is booked into the same slot as one at the head.
fn landed_arms(plans: &[&SweepPlan]) -> Vec<u32> {
    let mut ids: Vec<u32> = plans
        .iter()
        .flat_map(|plan| {
            plan.struts.iter().filter_map(|child| match child.landing {
                Landing::Strut { id, .. } => {
                    let host = plan.struts.iter().find(|host| host.id == id)?;
                    (host.kind == StrutKind::Arm).then_some(id)
                }
                _ => None,
            })
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Rings per disc, centre included, for the grader's model distance.
const DISC_RINGS: usize = 8;

/// The model, as the grader measures a section against it: every triangle
/// and its box.
struct Model<'a> {
    mesh: &'a Mesh,
    boxes: Vec<([f64; 3], [f64; 3])>,
}

impl<'a> Model<'a> {
    fn new(mesh: &'a Mesh) -> Self {
        let verts = mesh.verts();
        let boxes = mesh
            .tris()
            .iter()
            .map(|tri| {
                let corners = tri.map(|vertex| verts[vertex as usize]);
                let low =
                    std::array::from_fn(|k| corners.iter().map(|c| c[k]).fold(f64::MAX, f64::min));
                let high =
                    std::array::from_fn(|k| corners.iter().map(|c| c[k]).fold(f64::MIN, f64::max));
                (low, high)
            })
            .collect();
        Self { mesh, boxes }
    }

    /// The smallest distance, in mm, from the DISC of radius `radius` about
    /// `centre`, normal to `axis`, to the model surface, or `INFINITY` when no
    /// triangle is within `reach` of the disc. Brute force: the disc sampled on
    /// `DISC_RINGS` rings (the centre and `k·R/8`) of `RING_SAMPLES` points,
    /// each measured to every triangle whose box is within reach. A disc that
    /// enters the model crosses its surface on a spoke, so a sample lands
    /// within half the sampling pitch (at most `R/16`, 0.06 mm at the widest
    /// section on these fixtures) of the surface: the grader cannot miss an
    /// entry by more than that.
    fn disc_gap_mm(&self, centre: [f64; 3], radius: f64, axis: [f64; 3], reach: f64) -> f64 {
        let verts = self.mesh.verts();
        let near: Vec<&[u32; 3]> = self
            .mesh
            .tris()
            .iter()
            .zip(&self.boxes)
            .filter(|(_, (low, high))| {
                let outside: f64 = (0..3)
                    .map(|k| {
                        (low[k] - centre[k])
                            .max(centre[k] - high[k])
                            .max(0.0)
                            .powi(2)
                    })
                    .sum();
                outside.sqrt() <= radius + reach
            })
            .map(|(tri, _)| tri)
            .collect();
        if near.is_empty() {
            return f64::INFINITY;
        }
        let reference = if axis[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let e1 = geom_kernel::vec3::normalize(geom_kernel::vec3::cross(axis, reference))
            .expect("a ring direction");
        let e2 = geom_kernel::vec3::cross(axis, e1);
        let mut gap = f64::INFINITY;
        for ring in 0..=DISC_RINGS {
            #[expect(clippy::cast_precision_loss, reason = "a ring index below 9")]
            let r = radius * ring as f64 / DISC_RINGS as f64;
            let samples = if ring == 0 { 1 } else { RING_SAMPLES };
            for step in 0..samples {
                #[expect(clippy::cast_precision_loss, reason = "a sample index below 64")]
                let angle = std::f64::consts::TAU * step as f64 / RING_SAMPLES as f64;
                let point = geom_kernel::vec3::add(
                    centre,
                    geom_kernel::vec3::add(
                        geom_kernel::vec3::scale(e1, r * angle.cos()),
                        geom_kernel::vec3::scale(e2, r * angle.sin()),
                    ),
                );
                for tri in &near {
                    let closest = geom_kernel::closest_on_tri(
                        point,
                        verts[tri[0] as usize],
                        verts[tri[1] as usize],
                        verts[tri[2] as usize],
                    );
                    gap = gap.min(geom_kernel::vec3::norm(geom_kernel::vec3::sub(
                        point, closest,
                    )));
                }
            }
        }
        gap
    }
}

/// Grade the arms `ids` of `plan`, placed over `mesh`.
///
/// **The seat zone is where the seat plane is the model.** A section is in
/// its arm's seat zone when its ring comes within the final clearance of the
/// seat plane AND its disc comes within the final clearance of the model
/// itself. The plane alone is infinite: under a steep face it passes well
/// below the model (Round 25 arm 6, face 75° from horizontal: a section
/// 1.25 mm under the seat crosses it where there is no model), and a section
/// there touches nothing.
fn grade(plan: &SweepPlan, ids: &[u32], mesh: &Mesh) -> Vec<Trunk> {
    let clearance = final_clearance_mm();
    let model = Model::new(mesh);
    let arms = arms_of(plan);
    let mut out = Vec::new();
    for (index, host) in plan.struts.iter().enumerate() {
        if !ids.contains(&host.id) || host.kind != StrutKind::Arm {
            continue;
        }
        // THE OWN CONTACT: the ledger's contact slot, by the area law of the
        // load in that slot (the contact-sizing rule).
        let contact_load = *host.carried_mm2.last().expect("a sized arm");
        let thickened = host
            .thickened_mm
            .get(host.k_top - 1)
            .copied()
            .unwrap_or(0.0);
        let expected = area_law_radius_mm(contact_load)
            .max(host.min_radius_mm)
            .max(thickened);
        let own = host.tip_radius_mm();
        assert!(
            (own - expected).abs() < 1.0e-9,
            "arm {}: contact radius {own} mm against the area law's {expected} mm for its own \
             {contact_load} mm2",
            host.id
        );
        let primitives = lower_one(plan, &arms[index]);
        let Some(trim) = primitives
            .iter()
            .find(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere)
            .and_then(|ball| ball.seat_trim)
        else {
            continue;
        };
        // WHERE A LANDER'S LOAD CAN BE: at or under the highest layer any
        // member lands on this arm at. A section above that carries the arm's
        // own peel only, and a section there wider than the contact is the
        // arm's own (a thicken rung's taper: Round 25 arm 63, Round 50 arms
        // 135 and 139, each 0.05 mm at the contact and thickened under it),
        // not a sum this rule places.
        let highest_landing = plan
            .struts
            .iter()
            .filter_map(|child| match child.landing {
                Landing::Strut { id, layer } if id == host.id => Some(layer),
                _ => None,
            })
            .max();
        #[expect(clippy::cast_precision_loss, reason = "a layer index below 2^52")]
        let sum_top_z = highest_landing.map_or(f64::NEG_INFINITY, |layer| {
            plan.plate_z_mm + (layer + 1) as f64 * plan.layer_height_mm
        });
        let mut trunk = Trunk {
            id: host.id,
            own_mm: own,
            lowest_wider_mm: f64::INFINITY,
            lowest_wider_plane_only_mm: f64::INFINITY,
            model_gap_there_mm: f64::INFINITY,
            ..Trunk::default()
        };
        for primitive in &primitives {
            if !matches!(
                primitive.segment_kind,
                SupportSegmentKind::ContactTaper | SupportSegmentKind::Member
            ) {
                continue;
            }
            let start = primitive.start_mm.map(Finite::get);
            let end = primitive.end_mm.map(Finite::get);
            let axis = match primitive.ring_normal {
                FrustumRingNormal::SegmentAxis => {
                    geom_kernel::vec3::normalize(geom_kernel::vec3::sub(end, start))
                        .expect("an axis")
                }
                FrustumRingNormal::PlateZ => [0.0, 0.0, 1.0],
            };
            for station in 0..STATIONS {
                #[expect(clippy::cast_precision_loss, reason = "a station index below 33")]
                let t = station as f64 / (STATIONS - 1) as f64;
                let centre: [f64; 3] = std::array::from_fn(|k| start[k] + t * (end[k] - start[k]));
                let radius = primitive.start_radius_mm.get()
                    + t * (primitive.end_radius_mm.get() - primitive.start_radius_mm.get());
                let lowest = ring_lowest(centre, radius, axis, trim);
                if lowest >= clearance {
                    continue;
                }
                let wider = radius > own * (1.0 + RADIUS_SLACK) && centre[2] <= sum_top_z;
                let gap = model.disc_gap_mm(centre, radius, axis, clearance);
                if wider && lowest < trunk.lowest_wider_plane_only_mm {
                    trunk.lowest_wider_plane_only_mm = lowest;
                    trunk.model_gap_there_mm = gap;
                }
                if gap >= clearance {
                    continue;
                }
                trunk.stations_in_zone += 1;
                trunk.widest_in_zone_mm = trunk.widest_in_zone_mm.max(radius);
                if wider {
                    trunk.lowest_wider_mm = trunk.lowest_wider_mm.min(lowest);
                }
            }
        }
        out.push(trunk);
    }
    out
}

fn print(plan: &SweepPlan, trunks: &[Trunk], label: &str) {
    let rule = &plan.report.head_join;
    println!(
        "{label}: {} landed arms graded; rule: {} joins on arms examined, {} in a seat zone \
         before ({} near the plane only, off the model; {} candidates admitted off the model; {} \
         section-to-model rows), \
         {} landers moved ({} down the landing chain, {} at the hard limit only; max drop {} \
         layers; smallest solid clearance {:.4} mm; {:.4} mm2), {} dependants re-landed, {} \
         clearance rows, {} pass(es); kept: no clear join {}, tie {}, not an arm {}, no surface \
         {}; unmeasured {}; emitted field in zone {:?}",
        trunks.len(),
        rule.joins_examined,
        rule.joins_in_seat_zone_before,
        rule.joins_off_the_model,
        rule.candidates_off_the_model,
        rule.model_rows,
        rule.landers_moved,
        rule.landers_moved_down_the_chain,
        rule.landers_at_hard_limit,
        rule.max_drop_layers,
        rule.min_lander_clearance_mm,
        rule.load_moved_mm2,
        rule.dependants_relanded,
        rule.clearance_rows,
        rule.passes,
        rule.kept_no_clear_join,
        rule.kept_carrying_a_tie,
        rule.kept_not_an_arm,
        rule.kept_surface_unavailable,
        rule.joins_unmeasured,
        rule.in_seat_zone_after,
    );
    for trunk in trunks {
        if trunk.stations_in_zone == 0 && trunk.lowest_wider_plane_only_mm.is_infinite() {
            continue;
        }
        println!(
            "  arm {:>4}: own contact {:.4} mm; {} stations in the seat zone, widest {:.4} mm; \
             sections wider than own reach to {:+.4} mm of the seat plane where the model is \
             (plane alone {:+.4} mm, model {:.4} mm away there)",
            trunk.id,
            trunk.own_mm,
            trunk.stations_in_zone,
            trunk.widest_in_zone_mm,
            trunk.lowest_wider_mm,
            trunk.lowest_wider_plane_only_mm,
            trunk.model_gap_there_mm,
        );
    }
}

/// Every trunk the grader finds with a section wider than its own contact in
/// its seat zone must be NAMED by the emitted field's census (the prepare
/// log's warning): emit and warn. Returns the ones it does not name.
fn unwarned(plan: &SweepPlan, trunks: &[Trunk]) -> Vec<u32> {
    let (wider_in_zone, _) = offenders(trunks);
    let listed: Vec<u32> = plan
        .report
        .head_join
        .in_seat_zone_after
        .iter()
        .map(|(_, trunk)| *trunk)
        .collect();
    wider_in_zone
        .into_iter()
        .filter(|id| !listed.contains(id))
        .collect()
}

/// The trunks whose seat zone holds a section wider than their own contact,
/// and the trunks where such a section reaches the seat plane itself.
fn offenders(trunks: &[Trunk]) -> (Vec<u32>, Vec<u32>) {
    let clearance = final_clearance_mm();
    let wider_in_zone = trunks
        .iter()
        .filter(|trunk| trunk.lowest_wider_mm < clearance)
        .map(|trunk| trunk.id)
        .collect();
    let crossing = trunks
        .iter()
        .filter(|trunk| trunk.lowest_wider_mm < 0.0)
        .map(|trunk| trunk.id)
        .collect();
    (wider_in_zone, crossing)
}

/// A mesh-vertex-coincidence tolerance for "this triangle lies in the seat's
/// own plane". Not a registered constant and decides no outcome: it only
/// selects the triangles of a member's own flat seat face, which the member
/// is meant to touch.
const COPLANAR_EPS_MM: f64 = 1.0e-6;

/// The smallest gap, in mm, between a member's drawn solid (its axis from
/// `seat` to `base` at `radius`, as the lowering is handed it) and the model
/// surface, sampled at 401 points along the axis and measured to every
/// triangle except those lying in the member's OWN seat plane (the face it is
/// seated on); the first ball diameter from the seat is not sampled. The
/// grader's own brute force, not the kernel's BVH predicate.
fn solid_gap_mm(mesh: &Mesh, arm: &SweepArm) -> f64 {
    let (seat, base) = (arm.seat_mm, arm.base_mm);
    let radius = arm
        .radius_profile_mm
        .iter()
        .copied()
        .fold(arm.tip_radius_mm, f64::max);
    let skip_mm = 3.0 * arm.tip_radius_mm;
    let verts = mesh.verts();
    let own_face = |tri: &[u32; 3]| {
        arm.seat_face_point_mm.is_some_and(|point| {
            tri.iter().all(|&vertex| {
                let offset = geom_kernel::vec3::sub(verts[vertex as usize], point);
                geom_kernel::vec3::dot(offset, arm.seat_normal_mm).abs() < COPLANAR_EPS_MM
            })
        })
    };
    let length = geom_kernel::vec3::norm(geom_kernel::vec3::sub(base, seat));
    let mut gap = f64::INFINITY;
    for step in 0..=400 {
        let t = f64::from(step) / 400.0;
        if t * length < skip_mm {
            continue;
        }
        let point: [f64; 3] = std::array::from_fn(|k| seat[k] + t * (base[k] - seat[k]));
        for tri in mesh.tris().iter().filter(|tri| !own_face(tri)) {
            let near = geom_kernel::closest_on_tri(
                point,
                verts[tri[0] as usize],
                verts[tri[1] as usize],
                verts[tri[2] as usize],
            );
            gap = gap.min(geom_kernel::vec3::norm(geom_kernel::vec3::sub(point, near)) - radius);
        }
    }
    gap
}

/// Whether the footprint rule moved member `id`'s drawn axis: its own seat
/// was slid onto its face, or it lands on (or tops out on) a member that was,
/// down the chain. Read off the finished field's own records.
fn footprint_slid(plan: &SweepPlan) -> impl Fn(u32) -> bool + '_ {
    move |id: u32| {
        let mut next = Some(id);
        let mut steps = 0;
        while let Some(at) = next {
            let Some(strut) = plan.struts.iter().find(|strut| strut.id == at) else {
                return false;
            };
            if strut.seat_face.footprint == Footprint::MovedOntoFace {
                return true;
            }
            next = match strut.landing {
                Landing::Strut { id, .. } => Some(id),
                _ => strut.seat_on_strut,
            };
            steps += 1;
            if steps > plan.struts.len() {
                return false;
            }
        }
        false
    }
}

/// Whether member `id` of `before` has the same landing and path in `after`.
fn unmoved(before: &SweepPlan, after: &SweepPlan, id: u32) -> bool {
    let find = |plan: &SweepPlan| plan.struts.iter().find(|strut| strut.id == id).cloned();
    match (find(before), find(after)) {
        (Some(was), Some(now)) => was.landing == now.landing && was.path == now.path,
        _ => false,
    }
}

#[test]
fn the_seat_zone_of_every_head_arm_is_own_load_sized() {
    let mesh = flipped(&mesh_for(&round_25()));
    let unruled = round_25_flipped_with(&production().without_head_join());
    let plan = round_25_flipped_with(&production());
    let heads = landed_arms(&[&unruled, &plan]);
    let trunks = grade(&plan, &heads, &mesh);
    print(&plan, &trunks, "Round 25 flipped");
    assert!(
        trunks.iter().any(|trunk| trunk.stations_in_zone > 0),
        "no head arm has a shaft section in its seat zone, so this test grades nothing"
    );
    let (wider_in_zone, crossing) = offenders(&trunks);
    assert!(
        crossing.is_empty(),
        "arms {crossing:?}: a section wider than the arm's own contact reaches its seat plane"
    );
    assert!(
        wider_in_zone.is_empty(),
        "arms {wider_in_zone:?}: a section wider than the arm's own contact stands within the \
         {} mm final clearance of its seat plane",
        final_clearance_mm()
    );
    // Last, so a broken rule reddens on the geometry above first.
    let rule = &plan.report.head_join;
    assert!(
        rule.joins_in_seat_zone_before > 0 && rule.landers_moved > 0,
        "the rule must have found and moved a head join on this fixture (before {}, moved {})",
        rule.joins_in_seat_zone_before,
        rule.landers_moved
    );
    // RE-DERIVED 2026-09-30 (fix/chitubox-geometry), measured. Until then this
    // read "the finished field's census is empty". The census is taken on the
    // EMITTED field, after the footprint rule, which slides a seat along its
    // face so its whole contact disc lies on it and re-tilts that member's
    // shaft (and every member joined to it). With the owner's ChituBox ball
    // ratio, plate arm 42 slides 0.447 mm for its disc, and the re-tilted ring
    // of the sum-sized section under lander 77's join on its head comes within
    // the clearance: the census lists (77, 42). Contact area is the only seat
    // criterion (challenger ruling 2026-09-29, `footprint.rs`: a slide that
    // puts a disc back on its face is taken whatever it does to the shaft,
    // and the census warns). Measured the other way: refusing every slide
    // that lists a join (a head-join guard in the footprint move check)
    // empties the census and leaves arm 42 with 45 % of its disc on the face,
    // the outcome that ruling rejected (Round_100 arm 195, 22 %). So what the
    // census may still list here is exactly the joins whose HOST's drawn axis
    // a later footprint slide re-tilted (the sum-sized section is the
    // host's): the head-join rule itself left none, and the geometric grade
    // above (no section wider than its own contact in a seat zone) stays
    // strict.
    let slid = footprint_slid(&plan);
    let unaccounted: Vec<(u32, u32)> = rule
        .in_seat_zone_after
        .iter()
        .copied()
        .filter(|(_, host)| !slid(*host))
        .collect();
    assert!(
        unaccounted.is_empty(),
        "the finished field's census lists {unaccounted:?}, which no footprint slide re-tilted: \
         the head-join rule left them"
    );
    let (landers, _) = moved_members_clear_the_model(&unruled, &plan, &mesh);
    assert!(
        landers > 0,
        "the rule moved no lander, so this grades nothing"
    );
}

/// AND THE MOVE DID NOT JUST PUT A SOLID ON THE MODEL SOMEWHERE ELSE. A moved
/// LANDER (whose old host did not move) keeps the 0.3 mm hard limit from the
/// model outside its own tip ball; a member that rests on a moved one and was
/// re-landed with it is no nearer the model than it was (up to that limit).
/// Returns the landers moved, and how many of them had landed BELOW their
/// host's top alive layer (not at its head).
fn moved_members_clear_the_model(
    unruled: &SweepPlan,
    plan: &SweepPlan,
    mesh: &Mesh,
) -> (usize, usize) {
    let arms = arms_of(plan);
    let arms_before = arms_of(unruled);
    let gap_of = |arm: &SweepArm| solid_gap_mm(mesh, arm);
    let mut landers = 0;
    let mut below_head = 0;
    for (index, strut) in plan.struts.iter().enumerate() {
        let (was, before) = unruled
            .struts
            .iter()
            .enumerate()
            .find(|(_, other)| other.id == strut.id)
            .expect("the same member in both fields");
        // The rule moves support arms only. A cross or a mast is placed by the
        // ties AFTER the rule, on whatever field it left, so the same id names
        // a different member in the two fields.
        if strut.kind != StrutKind::Arm
            || before.kind != StrutKind::Arm
            || (before.landing == strut.landing && before.path == strut.path)
        {
            continue;
        }
        let head_lander = matches!(before.landing, Landing::Strut { id, .. }
            if unmoved(unruled, plan, id));
        let under_the_head = matches!(before.landing, Landing::Strut { id, layer }
            if unruled.struts.iter().any(|host| host.id == id && layer + 1 < host.k_top));
        let gap_before = gap_of(&arms_before[was]);
        let gap = gap_of(&arms[index]);
        let required = if head_lander {
            final_clearance_mm()
        } else {
            final_clearance_mm().min(gap_before)
        };
        println!(
            "  moved {} {:>4}: {:?} -> {:?}; solid gap to the model {gap_before:+.4} -> \
             {gap:+.4} mm (required {required:+.4})",
            if head_lander {
                "lander   "
            } else {
                "dependant"
            },
            strut.id,
            before.landing,
            strut.landing
        );
        assert!(
            gap >= required - 5.0e-3,
            "member {}: re-landed at {:?}, its solid comes {gap:.4} mm from the model \
             (required {required:.4})",
            strut.id,
            strut.landing
        );
        landers += usize::from(head_lander);
        below_head += usize::from(head_lander && under_the_head);
    }
    (landers, below_head)
}

/// The grader sees the defect when the rule is off: the placement as it was
/// before the rule puts a sum-sized section across a leaned head's seat plane.
#[test]
fn without_the_rule_a_leaned_head_reaches_its_seat_plane_with_the_sum() {
    let mesh = flipped(&mesh_for(&round_25()));
    let plan = round_25_flipped_with(&production().without_head_join());
    let trunks = grade(&plan, &landed_arms(&[&plan]), &mesh);
    print(&plan, &trunks, "Round 25 flipped, rule off");
    let (wider_in_zone, crossing) = offenders(&trunks);
    assert!(
        !crossing.is_empty(),
        "with the rule off no sum-sized section reaches a seat plane, so the fixture no longer \
         shows the defect this rule exists for"
    );
    assert!(wider_in_zone.len() >= crossing.len());
    assert_eq!(
        plan.report.head_join.landers_moved, 0,
        "the sabotage knob must move nothing"
    );
    // The census names every arm the grader finds, with the rule off too.
    let missing = unwarned(&plan, &trunks);
    assert!(
        missing.is_empty(),
        "arms {missing:?}: a section wider than own contact in the seat zone, and the census \
         does not warn"
    );
}

/// **The seat plane is not the model under a steep face.** Round 25 arm 6
/// (vertical, face 75° from horizontal): its lander's sum-sized section,
/// 1.25 mm under the seat, crosses the infinite seat plane with no model
/// within reach. That section touches nothing, so the rule leaves the join
/// where it is: every lander of an arm whose sums cross the plane only off
/// the model keeps its landing and its path.
///
/// **Graded at the seat faces before 2026-09-30, and why.** Arm 6's seat
/// plane is a face 75° from horizontal, and the seat-face ruling of
/// 2026-09-30 (`tip_landing::SeatFaceRule`) no longer lets a tip carry a face
/// steeper than the overhang band's steep end (57.5°): measured on this
/// fixture, arm 6 then carries the shallow underside beside it and no sum
/// crosses any seat plane off the model, so the test graded nothing. The
/// head-join rule's off-model branch reads the seat plane it is handed and not
/// how the face was chosen, so the branch is graded with the pre-ruling seat
/// faces (`without_seat_face_band`) on both plans; the ruled seat faces are
/// graded by `seat_face.rs` and `plinth-pipeline tip_trim::`.
#[test]
fn a_sum_that_crosses_the_seat_plane_off_the_model_is_left_alone() {
    let mesh = flipped(&mesh_for(&round_25()));
    let unruled = round_25_flipped_with(&production().without_seat_face_band().without_head_join());
    let plan = round_25_flipped_with(&production().without_seat_face_band());
    let clearance = final_clearance_mm();
    let off_model: Vec<u32> = grade(&unruled, &landed_arms(&[&unruled]), &mesh)
        .into_iter()
        .filter(|trunk| {
            trunk.lowest_wider_plane_only_mm < clearance && trunk.lowest_wider_mm.is_infinite()
        })
        .map(|trunk| trunk.id)
        .collect();
    println!(
        "Round 25 flipped: arms whose sums cross the seat plane only off the model {off_model:?}; \
         rule: {} joins near the plane only",
        plan.report.head_join.joins_off_the_model
    );
    assert!(
        !off_model.is_empty(),
        "no sum crosses a seat plane off the model on this fixture, so this test grades nothing"
    );
    let mut compared = 0;
    for child in &unruled.struts {
        let Landing::Strut { id, .. } = child.landing else {
            continue;
        };
        if !off_model.contains(&id) || child.kind != StrutKind::Arm {
            continue;
        }
        assert!(
            unmoved(&unruled, &plan, child.id),
            "member {} on arm {id}: its sum crosses the seat plane only where there is no \
             model, and the rule moved it",
            child.id
        );
        compared += 1;
    }
    assert!(
        compared > 0,
        "no lander on such an arm, so this test compares nothing"
    );
    // Last, so a broken zone reddens on the geometry above first: the rule's
    // own path-hit witness for the model half of the zone.
    assert!(
        plan.report.head_join.joins_off_the_model > 0,
        "the rule counted no join near the seat plane only, off the model"
    );
}

fn round_50_flipped_with(policy: &SweepPolicy) -> SweepPlan {
    place_with(&flipped(&mesh_for(&round_50())), "Round 50 flipped", policy)
}

/// **Round 50 at its committed flip: a lander NEAR the head, not at it.** The
/// outer ring's stiffener arms lean about 57°, and landers there land one or
/// three layers under the seat layer (arms 66, 68, 83, 98 on `cb28bdd`),
/// where the contact-sizing rule books them into the same slot as a head
/// join. Scoping by landing layer skipped them: their sum-sized sections
/// crossed the seat plane by 0.33 to 0.48 mm, and no warning named them.
/// Every arm a member lands on is graded, by geometry.
///
/// A join no admissible place exists for is kept and warned (emit and warn):
/// on this fixture lander 189 on arm 87, whose head join `cb28bdd` already
/// kept for that reason. So the finished field is held to: the census names
/// EXACTLY the arms the grader still finds, each one a join the rule kept
/// with a reason; and the rule moved landers that were under the head, not at
/// it, each clear of the model.
#[test]
fn on_round_50_no_lander_near_a_head_puts_the_sum_in_its_seat_zone() {
    let mesh = flipped(&mesh_for(&round_50()));
    let unruled = round_50_flipped_with(&production().without_head_join());
    let plan = round_50_flipped_with(&production());
    let trunks = grade(&plan, &landed_arms(&[&unruled, &plan]), &mesh);
    print(&plan, &trunks, "Round 50 flipped");
    let before = grade(&unruled, &landed_arms(&[&unruled]), &mesh);
    print(&unruled, &before, "Round 50 flipped, rule off");
    let (_, crossing_before) = offenders(&before);
    assert!(
        !crossing_before.is_empty(),
        "with the rule off no sum-sized section reaches a seat plane on Round 50, so the \
         fixture no longer shows the defect"
    );
    let missing = unwarned(&plan, &trunks);
    assert!(
        missing.is_empty(),
        "arms {missing:?}: a section wider than own contact in the seat zone, and the census \
         does not warn"
    );
    let (mut wider_in_zone, _) = offenders(&trunks);
    wider_in_zone.sort_unstable();
    let rule = &plan.report.head_join;
    let mut listed: Vec<u32> = rule
        .in_seat_zone_after
        .iter()
        .map(|(_, trunk)| *trunk)
        .collect();
    listed.sort_unstable();
    listed.dedup();
    assert_eq!(
        listed, wider_in_zone,
        "the census must name exactly the arms whose seat zone still holds a sum"
    );
    let kept = rule.kept_no_clear_join
        + rule.kept_carrying_a_tie
        + rule.kept_not_an_arm
        + rule.kept_surface_unavailable;
    assert!(
        rule.in_seat_zone_after.len() <= kept,
        "{} joins left in a seat zone, {kept} kept with a reason: the rule left a join it \
         neither moved nor accounted for",
        rule.in_seat_zone_after.len()
    );
    let (landers, below_head) = moved_members_clear_the_model(&unruled, &plan, &mesh);
    assert!(
        below_head > 0 && landers > below_head,
        "the rule must have moved landers from under a head as well as at one ({below_head} of \
         {landers})"
    );
}

/// A head the rule has no reason to touch keeps its join: every head arm that
/// the rule-off field already has clear of its seat zone (Round 25 arm 42, the
/// vertical head) keeps its lander where it was, and its own axis, with the
/// rule on. Its drawn SECTIONS are not pinned: a neighbour's move changes
/// where the down-sweep routes a cell or two of the model's peel, and the
/// area law then re-sizes the shaft (measured on arm 42: 10.4425 to 10.49 mm²
/// under the contact). That is the load path answering the new field, not
/// the rule touching this head.
#[test]
fn a_head_that_already_clears_keeps_its_join() {
    // RE-SCOPED 2026-09-30 (fix/chitubox-geometry), measured: with the owner's
    // ChituBox ball ratios the production Round 25 flipped has no head that
    // already clears ("no head was already clear, so this test compares
    // nothing"). Graded at the X its path was built at
    // (`crate::harness::PATH_FIXTURE_X_ALLOWED` = 12); the rule reads no X.
    let at = crate::harness::at_path_fixture_x;
    let on = round_25_flipped_with(&at(production()));
    let off = round_25_flipped_with(&at(production().without_head_join()));
    let listed: Vec<u32> = off
        .report
        .head_join
        .in_seat_zone_after
        .iter()
        .map(|(_, trunk)| *trunk)
        .collect();
    let arms_on = arms_of(&on);
    let arms_off = arms_of(&off);
    let mut compared = 0;
    for (index, strut) in off.struts.iter().enumerate() {
        let head_lander = off.struts.iter().any(|child| {
            matches!(child.landing, Landing::Strut { id, layer }
                if id == strut.id && layer + 1 >= strut.k_top)
        });
        if !head_lander || strut.kind != StrutKind::Arm || listed.contains(&strut.id) {
            continue;
        }
        let twin = on
            .struts
            .iter()
            .position(|other| other.id == strut.id)
            .expect("the same arm in both fields");
        println!(
            "  arm {:>4}: head already clear; seat {:?} base {:?}; tip {:.4} -> {:.4}, \
             root {:.4} -> {:.4}",
            strut.id,
            arms_off[index].seat_mm,
            arms_off[index].base_mm,
            arms_off[index].tip_radius_mm,
            arms_on[twin].tip_radius_mm,
            arms_off[index]
                .radius_profile_mm
                .first()
                .copied()
                .unwrap_or(0.0),
            arms_on[twin]
                .radius_profile_mm
                .first()
                .copied()
                .unwrap_or(0.0),
        );
        assert_eq!(
            (arms_on[twin].seat_mm, arms_on[twin].base_mm),
            (arms_off[index].seat_mm, arms_off[index].base_mm),
            "arm {}: its head already cleared and the rule moved its axis",
            strut.id
        );
        for child in &off.struts {
            if !matches!(child.landing, Landing::Strut { id, .. } if id == strut.id) {
                continue;
            }
            let twin_child = on
                .struts
                .iter()
                .find(|other| other.id == child.id)
                .expect("the same lander in both fields");
            assert_eq!(
                (twin_child.landing, &twin_child.path),
                (child.landing, &child.path),
                "arm {}: its head already cleared and the rule moved its lander {}",
                strut.id,
                child.id
            );
        }
        compared += 1;
    }
    assert!(
        compared > 0,
        "no head was already clear, so this test compares nothing"
    );
}
