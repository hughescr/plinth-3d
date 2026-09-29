//! **Every member rests on something.** The "porcupine quills" of 2026-09-28:
//! support arms whose tip is seated in the model and whose LOWER end stops in
//! the air, because the thing they were landed on was moved out from under
//! them afterwards (`plinth-evidence/.../field-2026-09-28/quills.md`).
//!
//! The rule under test: *support comes before what rests on it.* The lowest
//! point of every member must print on cured material — the plate, the model,
//! or another member that already stands there and reaches lower — and that
//! chain must end at the plate or the model. A tip seated in the model holds
//! only the member's TOP; it grounds nothing below it, so it is not a root
//! here.
//!
//! **The check is on the LOWERED geometry, not on the ledger.** It reads the
//! two points and two radii [`plinth_sweep::arms_of`] hands the emit boundary
//! and nothing else: no `Landing`, no parent id, no layer. A ledger-only form
//! (`parent.cell_at(layer) == child.base_cell()`) would restate the engine's
//! own bookkeeping, and it misses the un-merge route, where the ledger still
//! names a parent that no longer stands under the child.
//!
//! Tolerance is one layer height (0.05 mm): a lower end is on something when
//! it is within one lamina of it. The quills' lower ends measured at least
//! 0.447 mm from the model and 0.13 mm from the nearest member, so the verdict
//! does not depend on this choice.
//!
//! Frame: the fixture at its authored (identity) pose, recentred as the
//! orientation stage recentres it, plate at `-LIFT`, one layer more than the
//! span — the frame whose plan digests reproduced production exactly in the
//! diagnosis.

// Layer counts from finite model-scale lengths, in test-local arithmetic.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use geom_kernel::{closest_on_tri, segment_segment_distance, vec3};
use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::Mesh;
use plinth_orient::UpDirection;
use plinth_sweep::{arms_of, run_sweep, SweepPlan, SweepPolicy};

/// What the lowered field looks like from underneath.
struct Rooting {
    /// Members whose lower end rests, directly or through other members, on
    /// nothing that reaches the plate or the model. Plan strut ids.
    unrooted: Vec<u32>,
    /// Path-hit counts: how many members each clause rooted directly.
    on_plate: usize,
    on_model: usize,
    on_member: usize,
}

fn swept(mesh: &Mesh) -> (Mesh, SweepPlan) {
    let up = UpDirection::new([0.0, 0.0, 1.0]).expect("a unit direction");
    let mesh = plinth_orient::oriented_recentered_mesh(mesh, up).expect("an orientable fixture");
    let constants = m0_constants().expect("the registry");
    let policy = SweepPolicy::from_registry().expect("the registered sweep policy");
    let lift_mm = constants.placeholders().lift_mm().diagnostic().get();
    let height = *constants.evidence().layer_height_default_mm().value();
    let (min_z, max_z) = mesh
        .verts()
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v[2]), hi.max(v[2]))
        });
    let plate_z = min_z - lift_mm;
    let count = (((max_z - plate_z) / height).ceil().max(1.0) as usize) + 1;
    let layers = LayerPlan::new(plate_z, vec![height; count]).expect("a layer plan");
    let plan = run_sweep(&mesh, &layers, &policy).expect("the sweep emits and warns");
    (mesh, plan)
}

/// Distance from `point` to the nearest model triangle. Brute force over the
/// fixture's handful of triangles, through the workspace's one closest-point
/// routine.
fn model_distance(mesh: &Mesh, point: [f64; 3]) -> f64 {
    let verts = mesh.verts();
    mesh.tris()
        .iter()
        .map(|tri| {
            let a = verts[tri[0] as usize];
            let b = verts[tri[1] as usize];
            let c = verts[tri[2] as usize];
            vec3::dist(point, closest_on_tri(point, a, b, c))
        })
        .fold(f64::INFINITY, f64::min)
}

/// Classify the lower end of every lowered member, then close the relation
/// transitively: a member resting on a member is rooted only if that member is.
fn rooting(mesh: &Mesh, plan: &SweepPlan) -> Rooting {
    let tol = plan.layer_height_mm;
    let arms = arms_of(plan);
    let n = arms.len();
    // (lowest point, lowest z, seat, base, radius at seat, radius at base)
    let low: Vec<[f64; 3]> = arms
        .iter()
        .map(|arm| if arm.base_mm[2] <= arm.seat_mm[2] { arm.base_mm } else { arm.seat_mm })
        .collect();
    let mut rooted = vec![false; n];
    let mut rests_on: Vec<Vec<usize>> = vec![Vec::new(); n];
    let (mut on_plate, mut on_model, mut on_member) = (0, 0, 0);
    for i in 0..n {
        let point = low[i];
        if point[2] <= plan.plate_z_mm + tol {
            rooted[i] = true;
            on_plate += 1;
            continue;
        }
        if model_distance(mesh, point) <= tol {
            rooted[i] = true;
            on_model += 1;
            continue;
        }
        for (j, other) in arms.iter().enumerate() {
            // Only a member that EXTENDS BELOW this point can hold it up.
            if j == i || low[j][2] >= point[2] - 1.0e-9 {
                continue;
            }
            let (distance, _, on_axis) =
                segment_segment_distance(point, point, other.seat_mm, other.base_mm);
            let length = vec3::dist(other.seat_mm, other.base_mm);
            let t = if length > 0.0 { vec3::dist(other.seat_mm, on_axis) / length } else { 0.0 };
            let root = other.root_radius_mm.max(other.tip_radius_mm);
            let radius = other.tip_radius_mm + (root - other.tip_radius_mm) * t;
            if distance - radius <= tol {
                rests_on[i].push(j);
            }
        }
        if !rests_on[i].is_empty() {
            on_member += 1;
        }
    }
    loop {
        let mut changed = false;
        for i in 0..n {
            if !rooted[i] && rests_on[i].iter().any(|&j| rooted[j]) {
                rooted[i] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let unrooted = (0..n)
        .filter(|&i| !rooted[i])
        .map(|i| plan.struts[i].id)
        .collect();
    Rooting {
        unrooted,
        on_plate,
        on_model,
        on_member,
    }
}

fn graded(name: &str, mesh: &Mesh) -> Rooting {
    let (mesh, plan) = swept(mesh);
    let rooting = rooting(&mesh, &plan);
    let lane = &plan.report.slenderness_lane;
    // The engine's own ledger verdict, printed beside the geometric one so a
    // disagreement is visible. It is NOT what this test asserts on.
    eprintln!("{name}: {}", plan.report.render_landing_check().trim_end());
    eprintln!(
        "{name}: digest {}; {} members; lower end on plate {} / model {} / member {}; \
         unrooted {} {:?}; forced merges {}, braces {}, masts {}, merges put back {}; \
         path hits: merges refused because the arm carries someone {}, stale shafts refused {}, \
         over-grown merges kept because the arm carries branches {}",
        plan.report.sweep_digest,
        plan.struts.len(),
        rooting.on_plate,
        rooting.on_model,
        rooting.on_member,
        rooting.unrooted.len(),
        rooting.unrooted,
        lane.merges_forced,
        lane.braces_added,
        lane.masts_dropped,
        lane.merges_undone_by_radius,
        lane.merges_refused_carrying,
        lane.merges_refused_stale_shaft,
        plan.report.merges_kept_carrying,
    );
    // NOT VACUOUS: the field must contain members that rest on other members,
    // or the transitive clause was never exercised.
    assert!(
        rooting.on_member > 0,
        "{name}: no member rests on another, so the member clause never ran"
    );
    rooting
}

/// The small case: two forced merges, one of them a daisy chain. Strut 3 was
/// landed on strut 5 at layer 93, and strut 5 was then itself merged away and
/// starts at layer 97, so strut 3's lower end is in the air.
#[test]
fn raking_blade_has_no_member_resting_on_nothing() {
    let mesh = plinth_mesh::make_raking_blade().expect("the raking blade fixture");
    let rooting = graded("make_raking_blade", &mesh);
    assert_eq!(rooting.unrooted, Vec::<u32>::new());
}

/// The un-merge route: the after-lane `unmerge_overgrown` put parents back on
/// their straight columns and left the children that had landed on their
/// leaned shafts hanging 2.8-3.3 mm away.
#[test]
fn overhang_table_has_no_member_resting_on_nothing() {
    let mesh = plinth_mesh::make_overhang_table().expect("the overhang table fixture");
    let rooting = graded("make_overhang_table", &mesh);
    assert_eq!(rooting.unrooted, Vec::<u32>::new());
}

/// The pre-lane un-merge route: "16 merges undone because the trunk could not
/// grow enough" moved trunks that eleven children had landed on, and those
/// children's tapers ended 3.15-3.34 mm from anything
/// (`field-2026-09-28/trunk-thinner.md`, "Second, separate defect").
#[test]
fn tall_stilt_plate_has_no_member_resting_on_nothing() {
    let mesh = plinth_mesh::make_tall_stilt_plate().expect("the tall stilt plate fixture");
    let rooting = graded("make_tall_stilt_plate", &mesh);
    assert_eq!(rooting.unrooted, Vec::<u32>::new());
}
