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
//! Every HEAD ARM — a support arm that the placement, without the rule, gave a
//! lander at its seat layer or its top alive layer — is drawn, near its seat,
//! at its OWN load's size only: every section of every shaft record whose ring
//! comes within the registered final clearance (`FINAL_CLEARANCE_GAP_MM`,
//! 0.3 mm) of the arm's seat plane is no wider than the arm's own contact
//! radius, and no section wider than that reaches the plane at all.
//!
//! # How it is graded
//!
//! On the production lowering's own records (`plinth_support::lower_sweep_arms`
//! of `arms_of`), against the seat plane the production put on the arm's tip
//! ball. Each `ContactTaper` and `Member` record is cut into stations along
//! its axis; at each station the ring (the record's own centre line, its
//! linearly interpolated radius, normal to its axis) is SAMPLED at 64 points
//! and measured against the plane. That is the grader's own method, not the
//! rule's closed form. The own contact radius is the ledger's contact slot,
//! checked against the registered area law of that slot's load.
//!
//! The fixture is the Round 25 ratchet disc at its committed flip (the pose
//! is applied directly, not searched), where cavity-ceiling arms lean across
//! and land on the ring stiffener arms' heads.
//!
//! Sabotage: `SweepPolicy::without_head_join` (the placement as it was before
//! the rule) must put a sum-sized section across a seat plane again, and
//! `the_seat_zone_of_every_head_arm_is_own_load_sized` is red on it (recorded in
//! the lane report, `lanes/head-join.md`).

use crate::contact_sizing::flipped;
use crate::harness::{x_allowed, LIFT_MM};
use crate::round_bases_ratchet::{mesh_for, place_with, round_25};

use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_mesh::Mesh;
use plinth_support::{
    lower_sweep_arms, BuildFrame, FrustumRingNormal, SeatTrim, SupportPrimitive,
    SupportSegmentKind, SweepArm,
};
// (`SweepArm` is also the unit the moved-member clearance is graded on.)
use plinth_sweep::{arms_of, Landing, StrutKind, SweepPlan, SweepPolicy};

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

fn production() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every sweep number")
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
    stations_in_zone: usize,
    /// Widest section within the seat zone.
    widest_in_zone_mm: f64,
    /// Lowest ring reach of any section WIDER than the own contact.
    lowest_wider_mm: f64,
}

/// The HEAD ARMS of a field placed WITHOUT the rule: support arms a member
/// landed on at their seat layer or their top alive layer (the two landings
/// whose load reaches the contact slot). The rule moves those landers, so the
/// set is taken from the field as the placement left it.
fn head_arms(unruled: &SweepPlan) -> Vec<u32> {
    let mut ids: Vec<u32> = unruled
        .struts
        .iter()
        .filter_map(|child| match child.landing {
            Landing::Strut { id, layer } => {
                let host = unruled.struts.iter().find(|host| host.id == id)?;
                (host.kind == StrutKind::Arm && layer + 1 >= host.k_top).then_some(id)
            }
            _ => None,
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Grade the arms `ids` of `plan`.
fn grade(plan: &SweepPlan, ids: &[u32]) -> Vec<Trunk> {
    let clearance = final_clearance_mm();
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
        let mut trunk = Trunk {
            id: host.id,
            own_mm: own,
            lowest_wider_mm: f64::INFINITY,
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
                let wider = radius > own * (1.0 + RADIUS_SLACK);
                if lowest < clearance {
                    trunk.stations_in_zone += 1;
                    trunk.widest_in_zone_mm = trunk.widest_in_zone_mm.max(radius);
                }
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
        "{label}: {} head arms graded; rule: {} head joins examined, {} in a seat zone before, \
         {} landers moved ({} down the landing chain, {} at the hard limit only; max drop {} \
         layers; smallest solid clearance {:.4} mm; {:.4} mm2), {} dependants re-landed, {} \
         clearance rows, {} pass(es); kept: no clear join {}, tie {}, not an arm {}, no surface \
         {}; unmeasured {}; emitted field in zone {:?}",
        trunks.len(),
        rule.joins_examined,
        rule.joins_in_seat_zone_before,
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
        if trunk.stations_in_zone == 0 && trunk.lowest_wider_mm.is_infinite() {
            continue;
        }
        println!(
            "  arm {:>4}: own contact {:.4} mm; {} stations in the seat zone, widest {:.4} mm; \
             sections wider than own reach to {:+.4} mm of the seat plane",
            trunk.id,
            trunk.own_mm,
            trunk.stations_in_zone,
            trunk.widest_in_zone_mm,
            trunk.lowest_wider_mm,
        );
    }
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

#[test]
fn the_seat_zone_of_every_head_arm_is_own_load_sized() {
    let unruled = round_25_flipped_with(&production().without_head_join());
    let heads = head_arms(&unruled);
    let plan = round_25_flipped_with(&production());
    let trunks = grade(&plan, &heads);
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
    assert!(
        rule.in_seat_zone_after.is_empty(),
        "the finished field's census still lists {:?}",
        rule.in_seat_zone_after
    );
    // AND THE MOVE DID NOT JUST PUT A SOLID ON THE MODEL SOMEWHERE ELSE. A
    // moved LANDER keeps the 0.3 mm hard limit from the model outside its own
    // tip ball; a member that rests on one and was re-landed with it is no
    // nearer the model than it was (up to that limit).
    let mesh = flipped(&mesh_for(&round_25()));
    let arms = arms_of(&plan);
    let arms_before = arms_of(&unruled);
    let gap_of = |arm: &SweepArm| solid_gap_mm(&mesh, arm);
    let mut landers = 0;
    for (index, strut) in plan.struts.iter().enumerate() {
        let (was, before) = unruled
            .struts
            .iter()
            .enumerate()
            .find(|(_, other)| other.id == strut.id)
            .expect("the same member in both fields");
        if before.landing == strut.landing && before.path == strut.path {
            continue;
        }
        let head_lander = matches!(before.landing, Landing::Strut { id, layer }
            if unruled.struts.iter().any(|host| host.id == id && layer + 1 >= host.k_top));
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
    }
    assert!(
        landers > 0,
        "the rule moved no lander, so this grades nothing"
    );
}

/// The grader sees the defect when the rule is off: the placement as it was
/// before the rule puts a sum-sized section across a leaned head's seat plane.
#[test]
fn without_the_rule_a_leaned_head_reaches_its_seat_plane_with_the_sum() {
    let plan = round_25_flipped_with(&production().without_head_join());
    let trunks = grade(&plan, &head_arms(&plan));
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
    // The census names the same joins the rule would have moved.
    assert!(!plan.report.head_join.in_seat_zone_after.is_empty());
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
    let on = round_25_flipped_with(&production());
    let off = round_25_flipped_with(&production().without_head_join());
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
