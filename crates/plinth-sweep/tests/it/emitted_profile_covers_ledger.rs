//! **The support that reaches the file is at least as thick as the support
//! the area law sized** — the field report of 2026-09-28 ("the post above is
//! thicker than the post below").
//!
//! The down-sweep sizes every member layer by layer: `radius_mm[slot]` is the
//! section the carried load needs there, `π r² ≥ carried / X_ALLOWED`. Below a
//! mid-shaft junction that ledger STEPS: the trunk carries its own load plus
//! the branch's. Until this test was written the lowering drew every member as
//! ONE straight frustum from the tip radius to the root radius, so a trunk 1-2
//! mm below a junction was still drawn at almost its tip radius — thinner than
//! its own ledger, and thinner than the branch landing on it (Craig's
//! screenshot; 28 drawn inversions on the three pinned fixtures, realised
//! X up to 75 against X_ALLOWED 12).
//!
//! What is graded, on the EMITTED primitives of the production sweep and the
//! production lowering, never on the ledger restated:
//!
//! | test | what it asserts |
//! |---|---|
//! | `..._on_the_lonely_peak` / `..._on_the_brace_pair` / `..._on_the_tall_stilt_plate` | at the mid-height of every ledger slot of every member, the widest emitted section of that member there is at least the ledger radius; and at every `Landing::Strut` junction whose branch actually touches its trunk, the trunk's drawn section at the branch's base is at least the branch's drawn root |
//!
//! | `a_brace_raises_no_trunk_on_the_brace_pair` | with every brace thickened past any trunk's ledger, the lowering raises no member to a brace (rule R2, 2026-10-03: the lane never builds a brace thicker than its posts, and the brace-junction raise is deleted); every member still covers its own ledger |
//!
//! | `a_thinned_brace_and_a_detached_branch_still_fail_on_the_brace_pair` | sabotage of the real emission: a brace drawn at half its ledger is reported under-drawn, and a branch whose trunk is lifted off its root is reported detached |
//!
//! A branch whose root does not touch the trunk its ledger names (the
//! stilt plate carried 11 such un-merged branches until fix/no-dangling; every
//! fixture now pins 0) is counted EXACTLY per fixture and printed; it is not an
//! inversion and no profile can fix it.
//!
//! **What the grading reads as something other than a plain frustum** (since
//! the junction hulls of L2 and the articulated heads of v18):
//!
//! - A junction hull record (`FrustumRingNormal::Junction`) is the convex hull
//!   of the end rings that meet at a landed junction, not a frustum from its
//!   `start` to its `end`. It is read as the hull it is: the frustum between
//!   every two of its rings is a guaranteed part of it (`hull_cones`). The
//!   hull pass puts the record after the BRANCH; the host it cuts into has no
//!   shaft there, so the host's drawn solid is that record too
//!   (`with_foreign_hulls`, tied to the host by a ring that is an end of one of
//!   the host's frusta).
//! - Inside a hull the section tapers between the members that meet. A ledger
//!   slot a hull covers (its rings' heights, and the one layer above them where
//!   the junction pass drops the host's load-step collar) is graded against the
//!   thinnest ring of the hull; a slot no hull and no frustum covers is still a
//!   failure.
//! - A headed member's ball, link, cone and joint (everything above the head's
//!   joint sphere) are sized by the head rules, not the ledger, and are not
//!   graded.
//! - A branch is attached to its trunk when its root is inside a drawn trunk
//!   section, or within its own root radius of the trunk's drawn solid (shaft or
//!   hull, read without the branch's own rings: the junction pass pulls the
//!   trunk back from the landing point and fills the gap with a hull). Those
//!   branches are counted and printed as joined across a junction.
//!
//! How the tests are kept from passing vacuously: each counts the slots and
//! junctions it graded and fails on zero; each requires the member it grades
//! to be matched to its strut by where it lands; and a slot no emitted
//! primitive covers inside the member's own drawn span is a FAILURE, not a
//! skip. Slots below the member's lowest emitted point (the plate-gap span
//! under a root flare, another lane's territory) are counted and printed.
//!
//! Sabotage: revert the lowering to one frustum per member (delete the
//! breakpoint pass) and all three tests go red; the sabotage test above pins
//! that the hull reading does not blind the grading.

// Layer counts from finite model-scale lengths, in test-local arithmetic.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_mesh::{make_brace_pair, make_lonely_peak, make_tall_stilt_plate, Mesh};
use plinth_support::{
    BuildFrame, FrustumRingNormal, HullRing, SupportPrimitive, SupportSegmentKind,
};
use plinth_sweep::{to_primitives, Landing, StrutKind, SweepEmission, SweepPlan, SweepPolicy};

const LIFT_MM: f64 = 5.0;
const MAX_PRIMITIVES: usize = 4_000_000;
const MAX_PRIMITIVE_BYTES: usize = 1 << 30;
/// Float noise around an equality the construction designs for (a chord that
/// ends exactly on the ledger value), and nothing more.
const RELATIVE_SLACK: f64 = 1.0e-9;

fn layer_height_mm() -> f64 {
    *m0_constants()
        .expect("registry")
        .evidence()
        .layer_height_default_mm()
        .value()
}

fn z_span(mesh: &Mesh) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for vertex in mesh.verts() {
        min = min.min(vertex[2]);
        max = max.max(vertex[2]);
    }
    (min, max)
}

/// The production frame, exactly as `tip_seating.rs::plan_of` builds it:
/// `plate_z = min_z − lift`, one uniform plan stopping one layer above the top.
///
/// The finished plan is handed to `edit` before it is lowered — the lowering
/// is graded against the plan it was actually given.
pub(crate) fn plan_of_with(
    mesh: &Mesh,
    policy: &SweepPolicy,
    edit: impl FnOnce(&mut SweepPlan),
) -> (SweepPlan, SweepEmission) {
    let (min_z, max_z) = z_span(mesh);
    let plate_z = min_z - LIFT_MM;
    let height = layer_height_mm();
    let count = (((max_z - plate_z) / height).ceil() as usize)
        .saturating_add(1)
        .max(1);
    let layers = LayerPlan::new(plate_z, vec![height; count]).expect("a uniform layer plan");
    let mut plan = plinth_sweep::run_sweep(mesh, &layers, policy)
        .expect("the sweep emits and warns; it never refuses");
    edit(&mut plan);
    let frame = BuildFrame::new(
        Finite::new(plate_z).expect("a finite plate"),
        PositiveFinite::new(LIFT_MM).expect("a positive lift"),
    );
    let emission = to_primitives(&plan, frame, &layers, MAX_PRIMITIVES, MAX_PRIMITIVE_BYTES)
        .expect("the lowering refuses only about the request");
    (plan, emission)
}

fn point(primitive_point: [Finite; 3]) -> [f64; 3] {
    primitive_point.map(Finite::get)
}

/// The emitted primitives, grouped per member in emission order. A member
/// starts at its tip ball, or — for a brace or a mast, which carry no ball —
/// at its taper. This reads the emitted SEQUENCE; it does not rebuild it.
pub(crate) fn members(emission: &SweepEmission) -> Vec<Vec<&SupportPrimitive>> {
    let mut groups: Vec<Vec<&SupportPrimitive>> = Vec::new();
    for primitive in &emission.primitives {
        // The raft (fix/sweep-raft, 2026-09-29) follows every member in the
        // stream; it belongs to no member and carries no ledger section.
        if matches!(
            primitive.segment_kind,
            SupportSegmentKind::RaftPad | SupportSegmentKind::RaftBridge
        ) {
            continue;
        }
        // A MODEL FOOT (lane 2, 2026-10-12) is a contact ball at the LOWER
        // end of its own member: its centre is the end of the record before
        // it. It closes that member and starts none.
        let is_foot = |ball: &SupportPrimitive| {
            groups
                .last()
                .and_then(|group| group.last())
                .is_some_and(|last| {
                    let end = point(last.end_mm);
                    let centre: [f64; 3] = std::array::from_fn(|axis| {
                        0.5 * (point(ball.start_mm)[axis] + point(ball.end_mm)[axis])
                    });
                    last.segment_kind != SupportSegmentKind::ContactSphere
                        && (0..3).all(|axis| (end[axis] - centre[axis]).abs() < 1e-9)
                })
        };
        let starts_new = match primitive.segment_kind {
            SupportSegmentKind::ContactSphere => !is_foot(primitive),
            SupportSegmentKind::ContactTaper => !groups
                .last()
                .and_then(|group| group.last())
                .is_some_and(|last| last.segment_kind == SupportSegmentKind::ContactSphere),
            _ => false,
        };
        if starts_new || groups.is_empty() {
            groups.push(Vec::new());
        }
        groups.last_mut().expect("pushed above").push(primitive);
    }
    groups
}

/// The radius of the frustum from `a` (radius `ra`) to `b` (radius `rb`) at
/// height `z`, linear in its own axial parameter, or `None` outside its span.
fn frustum_radius_at(a: [f64; 3], b: [f64; 3], ra: f64, rb: f64, z: f64) -> Option<f64> {
    let (low, high) = (a[2].min(b[2]), a[2].max(b[2]));
    if (a[2] - b[2]).abs() <= 0.0 || z < low || z > high {
        return None;
    }
    let t = (a[2] - z) / (a[2] - b[2]);
    Some(ra + t * (rb - ra))
}

/// A frustum between two end-ring centres: what two rings of a junction hull
/// guarantee between them.
#[derive(Clone, Copy, Debug)]
struct Cone {
    a: [f64; 3],
    ra: f64,
    b: [f64; 3],
    rb: f64,
}

fn is_hull(primitive: &SupportPrimitive) -> bool {
    matches!(primitive.ring_normal, FrustumRingNormal::Junction(_))
}

fn ring_centre(ring: &HullRing) -> [f64; 3] {
    point(if ring.at_start {
        ring.frustum_start_mm
    } else {
        ring.frustum_end_mm
    })
}

/// **A junction hull read as the hull it is** (L2, `lower/junction.rs`): the
/// convex hull of the end rings that meet at a landed junction. A convex hull
/// holds every segment between a point of one ring and a point of another, so
/// between the centres of any two of its rings it holds at least the frustum
/// of the two ring radii (the blend of two discs is a disc of the blended
/// radius). That frustum is the guaranteed lower bound read here; the hull
/// itself is wider. The record's own `start`/`end`/radii name only its two
/// largest rings and say nothing about the third.
///
/// `without` names members whose own rings are left out: the root of a branch
/// is, trivially, inside the hull cone from the branch's own ring, so the
/// host's solid is read without the branch's rings.
fn hull_cones(primitive: &SupportPrimitive, without: &[&SupportPrimitive]) -> Vec<Cone> {
    let FrustumRingNormal::Junction(rings) = primitive.ring_normal else {
        return Vec::new();
    };
    let rings: Vec<&HullRing> = rings
        .rings()
        .filter(|ring| !without.iter().any(|member| ring_is_an_end_of(ring, member)))
        .collect();
    let mut cones = Vec::new();
    for (i, first) in rings.iter().enumerate() {
        for second in &rings[i + 1..] {
            cones.push(Cone {
                a: ring_centre(first),
                ra: first.radius_mm.get(),
                b: ring_centre(second),
                rb: second.radius_mm.get(),
            });
        }
    }
    cones
}

/// A hull ring holds the frustum it was made from as that frustum stood when
/// the hull was made; a later junction on the same shaft may have cut that
/// frustum shorter. The ring's centre and radius are still an end of a shaft
/// frustum the member is drawn with, and that is what ties a ring to a member.
fn ring_is_an_end_of(ring: &HullRing, member: &SupportPrimitive) -> bool {
    if is_hull(member)
        || matches!(
            member.segment_kind,
            SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere
        )
    {
        return false;
    }
    let centre = ring_centre(ring);
    let radius = ring.radius_mm.get();
    let same = |end: [f64; 3], end_radius: f64| {
        end.iter()
            .zip(&centre)
            .all(|(a, b)| (a - b).abs() <= 1.0e-9)
            && (end_radius - radius).abs() <= 1.0e-9
    };
    same(point(member.start_mm), member.start_radius_mm.get())
        || same(point(member.end_mm), member.end_radius_mm.get())
}

/// Every shaft frustum of `group`, and every pair of rings of its junction
/// hulls (without the rings of the `without` members), as cones: the solid a
/// member is known to draw.
fn solid_cones(group: &[&SupportPrimitive], without: &[&SupportPrimitive]) -> Vec<Cone> {
    group
        .iter()
        .filter(|primitive| {
            !matches!(
                primitive.segment_kind,
                SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere
            )
        })
        .flat_map(|primitive| {
            if is_hull(primitive) {
                hull_cones(primitive, without)
            } else {
                vec![Cone {
                    a: point(primitive.start_mm),
                    ra: primitive.start_radius_mm.get(),
                    b: point(primitive.end_mm),
                    rb: primitive.end_radius_mm.get(),
                }]
            }
        })
        .collect()
}

/// How far `p` stands outside a capped cone (0 inside): the axial overshoot
/// past a flat end cap and the lateral excess over the section there, together.
fn distance_to_cone(p: [f64; 3], cone: &Cone) -> f64 {
    let axis = [
        cone.b[0] - cone.a[0],
        cone.b[1] - cone.a[1],
        cone.b[2] - cone.a[2],
    ];
    let rel = [p[0] - cone.a[0], p[1] - cone.a[1], p[2] - cone.a[2]];
    let length_sq = axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2];
    if length_sq <= 0.0 {
        let miss = rel[0].hypot(rel[1]).hypot(rel[2]);
        return (miss - cone.ra.max(cone.rb)).max(0.0);
    }
    let length = length_sq.sqrt();
    let tau = (rel[0] * axis[0] + rel[1] * axis[1] + rel[2] * axis[2]) / length_sq;
    let overshoot = if tau < 0.0 {
        -tau * length
    } else if tau > 1.0 {
        (tau - 1.0) * length
    } else {
        0.0
    };
    let perpendicular = [
        rel[0] - tau * axis[0],
        rel[1] - tau * axis[1],
        rel[2] - tau * axis[2],
    ];
    let lateral = perpendicular[0]
        .hypot(perpendicular[1])
        .hypot(perpendicular[2]);
    let radius = cone.ra + tau.clamp(0.0, 1.0) * (cone.rb - cone.ra);
    overshoot.hypot((lateral - radius).max(0.0))
}

/// The junction hull records of the whole emission that name a frustum of
/// `group` among their rings, in addition to the ones the group holds itself.
/// The hull pass puts a hull record after the BRANCH it joins; the host it cuts
/// into is drawn thinner or shorter beside it, so the host's drawn solid there
/// is that record.
fn with_foreign_hulls<'a>(
    emission: &'a SweepEmission,
    groups: &[Vec<&'a SupportPrimitive>],
) -> Vec<Vec<&'a SupportPrimitive>> {
    let hulls: Vec<&SupportPrimitive> = emission
        .primitives
        .iter()
        .filter(|primitive| is_hull(primitive))
        .collect();
    groups
        .iter()
        .map(|group| {
            let mut extended = group.clone();
            for hull in &hulls {
                let FrustumRingNormal::Junction(rings) = hull.ring_normal else {
                    continue;
                };
                let names_a_frustum_of_the_group = rings
                    .rings()
                    .any(|ring| group.iter().any(|member| ring_is_an_end_of(ring, member)));
                if names_a_frustum_of_the_group
                    && !extended.iter().any(|held| std::ptr::eq(*held, *hull))
                {
                    extended.push(hull);
                }
            }
            extended
        })
        .collect()
}

/// The floor a junction hull sets for the section at height `z`: the thinnest
/// end ring of the hull that spans `z` (the widest such floor if several do).
/// A hull between a thick member and a thin one tapers from one to the other;
/// it is never thinner than the thinnest ring it joins, and a ledger slot it
/// covers is graded against that, not against the thick member's ledger.
///
/// The hull spans the heights of its ring centres, plus `collar_mm` above the
/// highest: the junction pass drops the host's load-step collar when it ends
/// within one layer above the landing point (`JunctionRules::layer_height_mm`),
/// so the slot of the ledger step just above the hull is the junction's too.
fn hull_floor_at(group: &[&SupportPrimitive], z: f64, collar_mm: f64) -> Option<f64> {
    group
        .iter()
        .filter_map(|primitive| {
            let FrustumRingNormal::Junction(rings) = primitive.ring_normal else {
                return None;
            };
            let low = rings
                .rings()
                .map(|r| ring_centre(r)[2])
                .fold(f64::INFINITY, f64::min);
            let high = rings
                .rings()
                .map(|r| ring_centre(r)[2])
                .fold(f64::NEG_INFINITY, f64::max);
            (z >= low && z <= high + collar_mm + 1.0e-9).then(|| {
                rings
                    .rings()
                    .map(|r| r.radius_mm.get())
                    .fold(f64::INFINITY, f64::min)
            })
        })
        .reduce(f64::max)
}

/// The height below which a headed member's post begins: the centre of the
/// head's joint sphere (the first joint sphere, after the ball, the link and
/// the cone). Everything above it is the head, sized by the head rules (ball,
/// link, cone, joint), not by the load ledger. `None` for a member without a
/// head.
fn head_floor_z(group: &[&SupportPrimitive]) -> Option<f64> {
    if group.first()?.segment_kind != SupportSegmentKind::ContactSphere {
        return None;
    }
    group
        .iter()
        .find(|primitive| primitive.segment_kind == SupportSegmentKind::JointSphere)
        .map(|joint| 0.5 * (point(joint.start_mm)[2] + point(joint.end_mm)[2]))
}

/// The widest emitted section of one member at height `z`, or `None` when no
/// primitive of the member spans `z`. Frusta are read as linear in their own
/// axial parameter (the emitted record's own definition); a ball is read as a
/// sphere cut perpendicular to its own pole axis.
pub(crate) fn drawn_radius_at(group: &[&SupportPrimitive], z: f64) -> Option<f64> {
    let mut widest: Option<f64> = None;
    for primitive in group {
        let start = point(primitive.start_mm);
        let end = point(primitive.end_mm);
        let r0 = primitive.start_radius_mm.get();
        let r1 = primitive.end_radius_mm.get();
        let here = match primitive.segment_kind {
            SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere => {
                let centre_z = 0.5 * (start[2] + end[2]);
                let axis_len = ((end[0] - start[0]).powi(2)
                    + (end[1] - start[1]).powi(2)
                    + (end[2] - start[2]).powi(2))
                .sqrt();
                if axis_len <= 0.0 || (end[2] - start[2]).abs() <= 0.0 {
                    None
                } else {
                    let along = (z - centre_z) * axis_len / (end[2] - start[2]);
                    (along.abs() < r0).then(|| (r0 * r0 - along * along).sqrt())
                }
            }
            // A junction hull record is the convex hull of end rings, not a
            // frustum from its `start` to its `end`.
            _ if is_hull(primitive) => hull_cones(primitive, &[])
                .iter()
                .filter_map(|cone| frustum_radius_at(cone.a, cone.b, cone.ra, cone.rb, z))
                .reduce(f64::max),
            _ => frustum_radius_at(start, end, r0, r1, z),
        };
        if let Some(radius) = here {
            widest = Some(widest.map_or(radius, |best: f64| best.max(radius)));
        }
    }
    widest
}

/// How far `point` sits sideways from the member's own drawn axis at its
/// height, over the frusta that span that height; `None` when none does.
fn axis_miss_at(group: &[&SupportPrimitive], point: [f64; 3]) -> Option<f64> {
    group
        .iter()
        .filter(|primitive| {
            !is_hull(primitive)
                && !matches!(
                    primitive.segment_kind,
                    SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere
                )
        })
        .filter_map(|primitive| {
            let start = self::point(primitive.start_mm);
            let end = self::point(primitive.end_mm);
            let (low, high) = (start[2].min(end[2]), start[2].max(end[2]));
            if (start[2] - end[2]).abs() <= 0.0 || point[2] < low || point[2] > high {
                return None;
            }
            let t = (start[2] - point[2]) / (start[2] - end[2]);
            let x = start[0] + t * (end[0] - start[0]);
            let y = start[1] + t * (end[1] - start[1]);
            Some((point[0] - x).hypot(point[1] - y))
        })
        .min_by(f64::total_cmp)
}

/// The member's highest emitted non-ball point and the radius drawn there —
/// for a brace, where its top seats on its host.
fn drawn_top(group: &[&SupportPrimitive]) -> Option<([f64; 3], f64)> {
    group
        .iter()
        .filter(|primitive| {
            !matches!(
                primitive.segment_kind,
                SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere
            )
        })
        .flat_map(|primitive| {
            [
                (point(primitive.start_mm), primitive.start_radius_mm.get()),
                (point(primitive.end_mm), primitive.end_radius_mm.get()),
            ]
        })
        .max_by(|a, b| a.0[2].total_cmp(&b.0[2]))
}

/// The member's lowest emitted non-ball point and the radius drawn there.
fn drawn_root(group: &[&SupportPrimitive]) -> Option<([f64; 3], f64)> {
    group
        .iter()
        .filter(|primitive| {
            !matches!(
                primitive.segment_kind,
                SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere
            )
        })
        .flat_map(|primitive| {
            [
                (point(primitive.start_mm), primitive.start_radius_mm.get()),
                (point(primitive.end_mm), primitive.end_radius_mm.get()),
            ]
        })
        .min_by(|a, b| a.0[2].total_cmp(&b.0[2]))
}

/// **Where a member's planned base stands against the line it is drawn on**
/// (re-pinned 2026-10-08, v20 milestone). The junction pass
/// (`plinth-support/src/lower/junction.rs`, landed with L2 e21327b) pulls a
/// landed branch's end back from the landing point `K` along the branch's OWN
/// axis by `t = r_max tan(d / 2)`, `r_max` the largest radius meeting at `K`
/// and `d` the branch's deflection from straight-on against the host's
/// continuation. So the planned base `(x, y)` must lie on the xy line of the
/// drawn bottom frustum's axis (perpendicular miss at most 1e-6 mm), and its
/// offset along that line may not exceed that pull-back. A plate member has no
/// pull-back, and a member whose base moved at all must carry the junction's
/// hull record (so the move is the junction pass's and nothing else's). The
/// hull record is no shaft frustum and is not read for the axis.
fn base_on_drawn_axis(
    group: &[&SupportPrimitive],
    host: Option<&[&SupportPrimitive]>,
    base: [f64; 2],
) -> Result<f64, String> {
    // (end a, end b, widest radius) of every shaft frustum.
    let shaft = |g: &[&SupportPrimitive]| -> Vec<([f64; 3], [f64; 3], f64)> {
        g.iter()
            .filter(|p| {
                !matches!(
                    p.segment_kind,
                    SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere
                ) && !matches!(p.ring_normal, FrustumRingNormal::Junction(_))
            })
            .map(|p| {
                (
                    point(p.start_mm),
                    point(p.end_mm),
                    p.start_radius_mm.get().max(p.end_radius_mm.get()),
                )
            })
            .collect()
    };
    let frusta = shaft(group);
    // The bottom frustum: the one holding the lowest point; `lo` that point.
    let (lo, hi, _) = frusta
        .iter()
        .map(|&(a, b, r)| if a[2] <= b[2] { (a, b, r) } else { (b, a, r) })
        .min_by(|x, y| x.0[2].total_cmp(&y.0[2]))
        .ok_or("no shaft frustum")?;
    let (dx, dy) = (hi[0] - lo[0], hi[1] - lo[1]);
    let run = dx.hypot(dy);
    let (rx, ry) = (base[0] - lo[0], base[1] - lo[1]);
    let (along, perp) = if run > 1.0e-9 {
        let (ux, uy) = (dx / run, dy / run);
        (rx * ux + ry * uy, (rx * uy - ry * ux).abs())
    } else {
        (0.0, rx.hypot(ry))
    };
    if perp > 1.0e-6 {
        return Err(format!("the base is {perp} mm off the drawn axis line"));
    }
    let offset = along.abs();
    if offset > 1.0e-6
        && !group
            .iter()
            .any(|p| matches!(p.ring_normal, FrustumRingNormal::Junction(_)))
    {
        return Err(format!(
            "the base is {offset} mm along the drawn axis line but the member carries no junction hull"
        ));
    }
    let norm = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    // The most the junction pass may pull this end back (a 3D length; its
    // horizontal part is at most that).
    let bound = host.map_or(0.0, |host| {
        let host_frusta = shaft(host);
        let r_max = host_frusta
            .iter()
            .chain(frusta.iter())
            .map(|f| f.2)
            .fold(0.0_f64, f64::max);
        let ends = || host_frusta.iter().flat_map(|f| [f.0, f.1]);
        let (Some(top), Some(bottom)) = (
            ends().max_by(|x, y| x[2].total_cmp(&y[2])),
            ends().min_by(|x, y| x[2].total_cmp(&y[2])),
        ) else {
            return 0.0;
        };
        let up_host = [top[0] - bottom[0], top[1] - bottom[1], top[2] - bottom[2]];
        let up_branch = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        let lens = norm(up_host) * norm(up_branch);
        if lens <= 0.0 {
            return 0.0;
        }
        // The branch's deflection from straight-on against the host below: the
        // angle between the branch's downward continuation and the host's
        // downward direction.
        let dot = up_host[0] * up_branch[0] + up_host[1] * up_branch[1] + up_host[2] * up_branch[2];
        r_max * (0.5 * (dot / lens).clamp(-1.0, 1.0).acos()).tan()
    });
    if offset <= bound + 1.0e-6 {
        Ok(offset)
    } else {
        Err(format!(
            "the base is {offset} mm along the drawn axis line, more than the junction \
             pull-back bound {bound}"
        ))
    }
}

#[derive(Default, Debug)]
struct Census {
    members: usize,
    slots_graded: usize,
    slots_below_lowest_point: usize,
    under_drawn_members: Vec<(u32, f64, f64)>,
    gaps_inside_member: usize,
    junctions_graded: usize,
    inversions: Vec<(u32, u32, f64, f64, f64)>,
    /// Branches whose root does not touch the trunk their ledger names at all
    /// (child, parent, sideways miss mm): the separate "detached junction"
    /// defect of the same field report, which no drawing of the profile can
    /// fix. Counted and printed, never graded as an inversion.
    detached: Vec<(u32, u32, f64)>,
    /// Branches whose root stands off the trunk's drawn shaft sections but
    /// within their own root radius of the trunk's drawn solid, because the
    /// junction pass cut the trunk back there (child, parent, distance mm).
    /// Joined, not graded for inversion.
    joined_across_a_junction: Vec<(u32, u32, f64)>,
    /// Ledger slots inside a head (ball, link, cone, joint): sized by the head
    /// rules, not graded against the ledger.
    slots_in_head: usize,
    /// Ledger slots inside a member's drawn span with no emitted section at
    /// all (member id, height above the plate).
    gap_slots: Vec<(u32, f64)>,
    /// Ledger slots inside a junction hull thinner than the ledger there:
    /// graded against the thinnest ring the hull joins.
    slots_in_junction_hull: usize,
    /// Brace tops graded against their host, and brace tops whose seat point
    /// is not on the host's drawn section at all (printed, not graded).
    brace_tops_graded: usize,
    brace_tops_off_host: Vec<(u32, u32, f64)>,
    member_frusta: usize,
}

fn census(plan: &SweepPlan, emission: &SweepEmission) -> Census {
    let groups = members(emission);
    assert_eq!(
        emission.arms_emitted(),
        emission.arms_offered,
        "this grading pairs members with struts in plan order, so it needs every arm emitted"
    );
    assert_eq!(
        groups.len(),
        plan.struts.len(),
        "one emitted member per strut in the plan"
    );
    let height = plan.layer_height_mm;
    let plate = plan.plate_z_mm;
    let mut out = Census {
        members: groups.len(),
        ..Census::default()
    };
    // Pair each member with its strut by WHERE IT LANDS, not only by order.
    // The landing is the base cell's centre, moved with its host's drawn axis
    // when the footprint rule slid that host's seat along its face
    // (fix/tip-trim, 2026-09-28: `footprint::followed_shifts`, zero where
    // nothing moved).
    let followed = plinth_sweep::footprint::followed_shifts(&plan.struts);
    // Each member with the junction hulls that cut into it (see
    // `with_foreign_hulls`): the host's drawn solid is its own frusta and those.
    let extended = with_foreign_hulls(emission, &groups);
    for (index, ((strut, group), follow)) in
        plan.struts.iter().zip(&groups).zip(&followed).enumerate()
    {
        let base_cell = strut.base_cell();
        let base_shift = follow.base_mm.unwrap_or([0.0; 3]);
        let base = [
            plan.grid.center_x_mm(plan.grid.col_of(base_cell)) + base_shift[0],
            plan.grid.center_y_mm(plan.grid.row_of(base_cell)) + base_shift[1],
        ];
        let (root_point, _) = drawn_root(group).expect("every member has a shaft");
        let host_group = match strut.landing {
            Landing::Strut { id, .. } => groups.get(id as usize),
            Landing::Plate | Landing::Model { .. } => None,
        };
        if let Err(why) = base_on_drawn_axis(group, host_group.map(Vec::as_slice), base) {
            panic!(
                "member {} does not land where its strut's base is: drawn {:?}, planned {:?}: {why}",
                strut.id, root_point, base
            );
        }
        out.member_frusta += group
            .iter()
            .filter(|primitive| primitive.segment_kind == SupportSegmentKind::Member)
            .count();
        let lowest = root_point[2];
        let head_floor = head_floor_z(group);
        let mut short_length = 0.0;
        let mut worst_ratio = f64::INFINITY;
        for (slot, required) in strut.radius_mm.iter().enumerate() {
            let z = plate + (strut.k_bottom + slot) as f64 * height + 0.5 * height;
            if z < lowest {
                out.slots_below_lowest_point += 1;
                continue;
            }
            // The head (ball, link, cone, joint) is sized by the head rules,
            // not by the ledger: its link and cone are ruled thinner than the
            // post below them.
            if head_floor.is_some_and(|floor| z > floor) {
                out.slots_in_head += 1;
                continue;
            }
            out.slots_graded += 1;
            let Some(drawn) = drawn_radius_at(&extended[index], z) else {
                out.gaps_inside_member += 1;
                out.gap_slots.push((strut.id, z - plate));
                continue;
            };
            // Inside a junction hull the section tapers between the members
            // that meet there; it is graded against the thinnest ring.
            let required = match hull_floor_at(&extended[index], z, height) {
                Some(floor) if floor < *required => {
                    out.slots_in_junction_hull += 1;
                    floor
                }
                _ => *required,
            };
            if drawn < required * (1.0 - RELATIVE_SLACK) {
                short_length += height;
                worst_ratio = worst_ratio.min(drawn / required);
            }
        }
        if short_length > 0.0 {
            out.under_drawn_members
                .push((strut.id, short_length, worst_ratio));
        }
    }
    // THE JUNCTION CLAUSE: a trunk is at least as thick as each branch where
    // the branch lands on it.
    for (index, (strut, group)) in plan.struts.iter().zip(&groups).enumerate() {
        let Landing::Strut { id, .. } = strut.landing else {
            continue;
        };
        let Some(parent_index) = plan.struts.iter().position(|parent| parent.id == id) else {
            continue;
        };
        let (root_point, child_root) = drawn_root(group).expect("every member has a shaft");
        let trunk = drawn_radius_at(&extended[parent_index], root_point[2]).unwrap_or(0.0);
        let miss = axis_miss_at(&extended[parent_index], root_point).unwrap_or(f64::INFINITY);
        if miss > trunk {
            // The root is not inside a drawn shaft section of the trunk. The
            // junction pass pulls the trunk's shaft back from the landing
            // point and fills the gap with a hull, so the branch's end may
            // stand on the trunk's axis in that gap or a hair past the hull's
            // end ring. It is joined when the end disc (the branch's own root
            // radius) reaches the trunk's drawn solid, shaft or hull;
            // detached when it does not.
            let reach = solid_cones(&extended[parent_index], group)
                .iter()
                .map(|cone| distance_to_cone(root_point, cone))
                .fold(f64::INFINITY, f64::min);
            if reach > child_root {
                out.detached.push((strut.id, id, reach));
            } else {
                out.joined_across_a_junction.push((strut.id, id, reach));
            }
            continue;
        }
        out.junctions_graded += 1;
        // Where the branch ends in a junction hull, the junction is the hull:
        // it tapers between the members that meet and is never thinner than
        // the thinnest ring, so the branch's root is graded against that.
        let hull_floor = [&extended[index], &extended[parent_index]]
            .into_iter()
            .filter_map(|held| hull_floor_at(held, root_point[2], height))
            .reduce(f64::max);
        let branch = hull_floor.map_or(child_root, |floor| child_root.min(floor));
        if trunk < branch * (1.0 - RELATIVE_SLACK) {
            out.inversions
                .push((strut.id, id, root_point[2] - plate, child_root, trunk));
        }
    }
    // THE SAME CLAUSE AT A BRACE'S TOP, where it seats on its host shaft.
    for (strut, group) in plan.struts.iter().zip(&groups) {
        let Some(host_id) = strut.seat_on_strut else {
            continue;
        };
        let Some(host_index) = plan.struts.iter().position(|host| host.id == host_id) else {
            continue;
        };
        let (top_point, brace_top) = drawn_top(group).expect("every member has a shaft");
        let trunk = drawn_radius_at(&extended[host_index], top_point[2]).unwrap_or(0.0);
        let miss = axis_miss_at(&extended[host_index], top_point).unwrap_or(f64::INFINITY);
        if miss > trunk {
            out.brace_tops_off_host.push((strut.id, host_id, miss));
            continue;
        }
        out.junctions_graded += 1;
        out.brace_tops_graded += 1;
        if trunk < brace_top * (1.0 - RELATIVE_SLACK) {
            out.inversions
                .push((strut.id, host_id, top_point[2] - plate, brace_top, trunk));
        }
    }
    out
}

/// `detached_expected`: how many branches the fixture is KNOWN to leave off
/// their named trunk (the separate defect). Asserted exactly, so a change in
/// that population is loud here too rather than silently absorbed.
fn grade(name: &str, mesh: &Mesh, detached_expected: usize) {
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    grade_with(name, mesh, &policy, detached_expected, |_| {});
}

fn grade_with(
    name: &str,
    mesh: &Mesh,
    policy: &SweepPolicy,
    detached_expected: usize,
    edit: impl FnOnce(&mut SweepPlan),
) {
    let (plan, emission) = plan_of_with(mesh, policy, edit);
    let found = census(&plan, &emission);
    println!(
        "{name}: brace tops graded {}, off their host (not graded) {:?}; members raised to a \
         branch {}; arm junctions inverted in the ledger itself {}",
        found.brace_tops_graded,
        found.brace_tops_off_host,
        emission.members_raised_to_a_branch,
        emission.arm_junctions_inverted_in_ledger,
    );
    assert_eq!(
        emission.arm_junctions_inverted_in_ledger, 0,
        "{name}: the down-sweep's own ledger names an arm wider than its trunk"
    );
    println!(
        "{name}: {} members, {} slots graded ({} below the lowest emitted point, not graded), \
         {} junctions graded ({} detached, not graded: {:?}), {} Member frusta emitted (path \
         hits of the breakpoint pass); under-drawn members {}, inversions {}, gaps {}",
        found.members,
        found.slots_graded,
        found.slots_below_lowest_point,
        found.junctions_graded,
        found.detached.len(),
        found.detached,
        found.member_frusta,
        found.under_drawn_members.len(),
        found.inversions.len(),
        found.gaps_inside_member,
    );
    println!(
        "{name}: {} slots in a head (not graded), {} slots in a junction hull (graded against \
         its thinnest ring), {} branches joined across a junction hull {:?}",
        found.slots_in_head,
        found.slots_in_junction_hull,
        found.joined_across_a_junction.len(),
        found.joined_across_a_junction,
    );
    assert!(
        found.slots_graded > 0,
        "{name}: anti-vacuity, no slot graded"
    );
    assert!(
        found.junctions_graded > 0,
        "{name}: anti-vacuity, no junction graded"
    );
    assert_eq!(
        found.detached.len(),
        detached_expected,
        "{name}: detached branches (child, parent, sideways miss mm): {:?}",
        found.detached
    );
    assert_eq!(
        found.gaps_inside_member, 0,
        "{name}: a ledger slot inside a member's drawn span has no emitted section at all"
    );
    let under_length: f64 = found.under_drawn_members.iter().map(|m| m.1).sum();
    assert!(
        found.under_drawn_members.is_empty() && found.inversions.is_empty(),
        "{name}: {} members drawn thinner than their ledger over {under_length:.3} mm \
         (id, length mm, worst drawn/ledger): {:?}; {} junctions where the trunk is drawn \
         thinner than the branch landing on it (child, parent, z above plate, branch root, \
         trunk there): {:?}",
        found.under_drawn_members.len(),
        found.under_drawn_members,
        found.inversions.len(),
        found.inversions
    );
}

#[test]
fn emitted_profile_covers_the_ledger_on_the_lonely_peak() {
    grade(
        "make_lonely_peak",
        &make_lonely_peak().expect("the lonely peak fixture"),
        0,
    );
}

#[test]
fn emitted_profile_covers_the_ledger_on_the_brace_pair() {
    grade(
        "make_brace_pair",
        &make_brace_pair().expect("the brace pair fixture"),
        0,
    );
}

/// **A brace raises no trunk** (rule R2, Craig ruling 2026-10-03, interim
/// pending the slenderness coupon). Re-pinned from
/// `a_brace_thicker_than_its_trunks_raises_them_on_the_brace_pair`, which
/// pinned the deleted rule: a brace thicker than the shaft it joined raised
/// every trunk and host from the joint down to its root, the "fat post with a
/// thin top" Craig saw in ChituBox. The slenderness lane now never builds a
/// diagonal thicker than the thinner post it joins (the band's
/// `thicker_than_post` refusal), so the lowering has nothing to raise.
///
/// This test forces the case the lane no longer makes — every brace on the
/// brace pair given 1.5 × the widest ledger radius in the field — and grades
/// what the lowering does with it: no member is raised to a brace (the same
/// count as the unedited field), every member still covers its own ledger,
/// and the over-thick braces now stand thicker than their hosts (graded
/// inversions), because nothing but the lane's R2 keeps them thin.
#[test]
fn a_brace_raises_no_trunk_on_the_brace_pair() {
    let mut thickened = 0_usize;
    // Graded at the seat placement the fixture was built for
    // (`harness::at_path_fixture_seating`): at the production placement the
    // brace pair places no brace (measured 2026-09-30), so there is nothing to
    // thicken. The profile rule reads no placement.
    let policy = crate::harness::at_path_fixture_seating(
        SweepPolicy::from_registry().expect("the registry supplies every sweep number"),
    );
    let mesh = make_brace_pair().expect("the brace pair fixture");
    let (_, plain) = plan_of_with(&mesh, &policy, |_| {});
    let (plan, emission) = plan_of_with(&mesh, &policy, |plan| {
        let widest = plan
            .struts
            .iter()
            .flat_map(|strut| strut.radius_mm.iter().copied())
            .fold(0.0_f64, f64::max);
        for strut in plan
            .struts
            .iter_mut()
            .filter(|strut| strut.kind == StrutKind::Brace)
        {
            for radius in &mut strut.radius_mm {
                *radius = 1.5 * widest;
            }
            thickened += 1;
        }
    });
    assert!(
        thickened > 0,
        "anti-vacuity: the brace pair must place braces for this test to mean anything"
    );
    let found = census(&plan, &emission);
    println!(
        "make_brace_pair, braces thickened: {thickened} braces; members raised to a branch {} \
         (unedited {}); {} junctions graded ({} brace tops), {} inversions; under-drawn {}",
        emission.members_raised_to_a_branch,
        plain.members_raised_to_a_branch,
        found.junctions_graded,
        found.brace_tops_graded,
        found.inversions.len(),
        found.under_drawn_members.len(),
    );
    assert_eq!(
        emission.members_raised_to_a_branch, plain.members_raised_to_a_branch,
        "a brace, however thick, raises no member"
    );
    assert!(
        found.under_drawn_members.is_empty() && found.gaps_inside_member == 0,
        "every member still covers its own ledger: {:?}; gap slots (member, z above plate): {:?}",
        found.under_drawn_members,
        found.gap_slots
    );
    assert!(
        found.brace_tops_graded > 0 && !found.inversions.is_empty(),
        "the over-thick braces stand thicker than their hosts: nothing raises a host to them"
    );
}

/// The fixture Craig's screenshot came from: 145 members, 21 attached drawn
/// inversions before the fix. It also carried the SEPARATE detached-junction
/// defect (11 branches un-merged off their trunk, field report "trunk-thinner",
/// second defect), which no drawing of the profile can repair. That defect's
/// own lane (fix/no-dangling: a trunk moves with its branches or not at all,
/// in `unmerge_overgrown` and the slenderness forced-merge loop) removed all
/// 11, so the count is now pinned at 0 and every one of those junctions is
/// graded by the junction clause like any other (measured on the merge of
/// fix/no-dangling with develop 4b58aeb: 155 members, 120 junctions graded,
/// 0 detached, 0 inversions).
#[test]
fn emitted_profile_covers_the_ledger_on_the_tall_stilt_plate() {
    grade(
        "make_tall_stilt_plate",
        &make_tall_stilt_plate().expect("the tall stilt plate fixture"),
        0,
    );
}

/// **Sabotage: the junction-hull reading must not make the grading blind.**
/// On the brace pair's real emission, (1) a brace whose shaft is drawn at half
/// its ledger must be reported under-drawn, and (2) a branch whose trunk is
/// moved off its root (every part of the trunk above its lowest frustum lifted
/// 50 mm) must be reported detached. Both are controlled against the same
/// emission unedited, which grades clean.
#[test]
fn a_thinned_brace_and_a_detached_branch_still_fail_on_the_brace_pair() {
    let policy = crate::harness::at_path_fixture_seating(
        SweepPolicy::from_registry().expect("the registry supplies every sweep number"),
    );
    let mesh = make_brace_pair().expect("the brace pair fixture");
    let (plan, emission) = plan_of_with(&mesh, &policy, |_| {});
    let clean = census(&plan, &emission);
    assert!(
        clean.under_drawn_members.is_empty()
            && clean.detached.is_empty()
            && clean.gaps_inside_member == 0,
        "control: the unedited emission grades clean"
    );

    // (1) A brace drawn at half its ledger.
    let brace_index = plan
        .struts
        .iter()
        .position(|strut| strut.kind == StrutKind::Brace)
        .expect("the brace pair places braces at this seating");
    let brace_id = plan.struts[brace_index].id;
    let mut thinned = emission.clone();
    let target: Vec<*const SupportPrimitive> = members(&thinned)[brace_index]
        .iter()
        .map(|primitive| std::ptr::from_ref::<SupportPrimitive>(primitive))
        .collect();
    let mut halved = 0_usize;
    for primitive in &mut thinned.primitives {
        let ours = target
            .iter()
            .any(|held| std::ptr::eq(*held, std::ptr::from_ref::<SupportPrimitive>(primitive)));
        let shaft = matches!(
            primitive.segment_kind,
            SupportSegmentKind::Member | SupportSegmentKind::ContactTaper
        ) && !is_hull(primitive);
        if ours && shaft {
            primitive.start_radius_mm =
                PositiveFinite::new(0.5 * primitive.start_radius_mm.get()).expect("positive");
            primitive.end_radius_mm =
                PositiveFinite::new(0.5 * primitive.end_radius_mm.get()).expect("positive");
            halved += 1;
        }
    }
    assert!(halved > 0, "anti-vacuity: the brace has a shaft to thin");
    let found = census(&plan, &thinned);
    assert!(
        found.under_drawn_members.iter().any(|m| m.0 == brace_id),
        "a brace drawn at half its ledger must be reported under-drawn: {:?}",
        found.under_drawn_members
    );

    // (2) A trunk lifted off the root of a branch that lands on it.
    let groups = members(&emission);
    let extended = with_foreign_hulls(&emission, &groups);
    let mut lifted_for = None;
    for (child_index, strut) in plan.struts.iter().enumerate() {
        let Landing::Strut { id, .. } = strut.landing else {
            continue;
        };
        let Some(parent_index) = plan.struts.iter().position(|parent| parent.id == id) else {
            continue;
        };
        let (root_point, child_root) = drawn_root(&groups[child_index]).expect("a shaft");
        let parent_shaft: Vec<&&SupportPrimitive> = groups[parent_index]
            .iter()
            .filter(|primitive| {
                !is_hull(primitive)
                    && !matches!(
                        primitive.segment_kind,
                        SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere
                    )
            })
            .collect();
        let lowest_top = parent_shaft
            .iter()
            .min_by(|a, b| {
                point(a.start_mm)[2]
                    .min(point(a.end_mm)[2])
                    .total_cmp(&point(b.start_mm)[2].min(point(b.end_mm)[2]))
            })
            .map(|bottom| point(bottom.start_mm)[2].max(point(bottom.end_mm)[2]));
        if lowest_top.is_some_and(|top| root_point[2] > top + child_root + 1.0)
            && extended[parent_index].len() > 1
        {
            lifted_for = Some((strut.id, id, parent_index, lowest_top.expect("some")));
            break;
        }
    }
    let (child_id, parent_id, parent_index, keep_below) =
        lifted_for.expect("a branch lands on a trunk above the trunk's lowest frustum");
    let mut detached_emission = emission.clone();
    let lift_targets: Vec<*const SupportPrimitive> = members(&detached_emission)[parent_index]
        .iter()
        .map(|primitive| std::ptr::from_ref::<SupportPrimitive>(primitive))
        .collect();
    for primitive in &mut detached_emission.primitives {
        let ours = lift_targets
            .iter()
            .any(|held| std::ptr::eq(*held, std::ptr::from_ref::<SupportPrimitive>(primitive)));
        let low = point(primitive.start_mm)[2].min(point(primitive.end_mm)[2]);
        if ours && is_hull(primitive) {
            // The trunk's own junction hulls go with it, rings included.
            let FrustumRingNormal::Junction(rings) = primitive.ring_normal else {
                unreachable!("is_hull")
            };
            let lift =
                |p: [Finite; 3]| [p[0], p[1], Finite::new(p[2].get() + 50.0).expect("finite")];
            let lifted = rings.0.map(|ring| {
                ring.map(|ring| HullRing {
                    frustum_start_mm: lift(ring.frustum_start_mm),
                    frustum_end_mm: lift(ring.frustum_end_mm),
                    ..ring
                })
            });
            primitive.ring_normal =
                FrustumRingNormal::Junction(plinth_support::JunctionRings(lifted));
            primitive.start_mm = lift(primitive.start_mm);
            primitive.end_mm = lift(primitive.end_mm);
        }
        if ours
            && !is_hull(primitive)
            && !matches!(
                primitive.segment_kind,
                SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere
            )
            && low >= keep_below - 1.0e-9
        {
            for end in [&mut primitive.start_mm, &mut primitive.end_mm] {
                end[2] = Finite::new(end[2].get() + 50.0).expect("finite");
            }
        }
    }
    let found = census(&plan, &detached_emission);
    assert!(
        found
            .detached
            .iter()
            .any(|d| d.0 == child_id && d.1 == parent_id),
        "branch {child_id} with its trunk {parent_id} lifted off its root must be reported \
         detached: {:?}",
        found.detached
    );
}
