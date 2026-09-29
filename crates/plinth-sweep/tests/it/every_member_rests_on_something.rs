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
//! **The smallest defect this check cannot see.** It reads each member as
//! the straight line `seat -> base` that `arms_of` hands the lowering, and
//! that line is neither what the plan lands on nor exactly what the file
//! draws. Landings are made on a member's RASTERISED PATH (`cell_at`), which
//! need not be straight; and the lowering draws a plate-rooted arm as a taper
//! to a root-boss top above the base point plus a vertical flare
//! (`plinth_support::lower_sweep_arms`). Wherever the drawn solid departs from
//! the path, a member landed on the path can miss it. Measured 2026-09-28 on `make_laid_tube` at its production pose (plan
//! digest `0335589d6ee25341`, reproduced exactly by this file's frame): this
//! check finds 0 unrooted members, while the emitted-bytes audit
//! (`plinth-audit` I5 clause (d)) finds 76 — 2 arm ends and 70 brace ends
//! 0-0.54 mm off the drawn shaft, and 4 brace ends inside the 0.2 mm raft gap.
//! Which of the two departures accounts for each one was not traced member by
//! member. The class belongs to the emit boundary and to brace placement, not
//! to the landing rule this file grades; the audit, not this test, sees it.
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
    /// The engine's own whole-run identity on the finished field,
    /// `injected - plate reaction - uncarried`, in mm², and the injected
    /// total it is measured against. Read from the report, never recomputed.
    load_gap_mm2: f64,
    injected_mm2: f64,
}

/// The load gap closes: every mm² the down-sweep put into the load raster
/// arrives at the plate or is named as uncarried, and nothing arrives that was
/// never put in. A lost landing makes it positive (the quills: 47.21 mm² on the
/// Cylinder); a member that hands its parent load it never carried makes it
/// negative. Tolerance: rounding of the per-layer sums, relative to the total.
fn assert_load_gap_closes(name: &str, rooting: &Rooting) {
    let tolerance = 1.0e-9 * rooting.injected_mm2.max(1.0);
    assert!(
        rooting.load_gap_mm2.abs() <= tolerance,
        "{name}: load gap {} mm2 (injected {} mm2) must be 0",
        rooting.load_gap_mm2,
        rooting.injected_mm2
    );
}

fn swept(mesh: &Mesh) -> (Mesh, SweepPlan) {
    swept_with(
        mesh,
        &SweepPolicy::from_registry().expect("the registered sweep policy"),
    )
}

fn swept_with(mesh: &Mesh, policy: &SweepPolicy) -> (Mesh, SweepPlan) {
    let up = UpDirection::new([0.0, 0.0, 1.0]).expect("a unit direction");
    let mesh = plinth_orient::oriented_recentered_mesh(mesh, up).expect("an orientable fixture");
    let constants = m0_constants().expect("the registry");
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
    let plan = run_sweep(&mesh, &layers, policy).expect("the sweep emits and warns");
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
        .map(|arm| {
            if arm.base_mm[2] <= arm.seat_mm[2] {
                arm.base_mm
            } else {
                arm.seat_mm
            }
        })
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
            let t = if length > 0.0 {
                vec3::dist(other.seat_mm, on_axis) / length
            } else {
                0.0
            };
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
        load_gap_mm2: plan.report.landing_check.load_gap_mm2,
        injected_mm2: plan.report.injected_load_mm2,
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
    assert_load_gap_closes("make_raking_blade", &rooting);
}

/// The un-merge route: the after-lane `unmerge_overgrown` put parents back on
/// their straight columns and left the children that had landed on their
/// leaned shafts hanging 2.8-3.3 mm away.
#[test]
fn overhang_table_has_no_member_resting_on_nothing() {
    let mesh = plinth_mesh::make_overhang_table().expect("the overhang table fixture");
    let rooting = graded("make_overhang_table", &mesh);
    assert_eq!(rooting.unrooted, Vec::<u32>::new());
    assert_load_gap_closes("make_overhang_table", &rooting);
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
    assert_load_gap_closes("make_tall_stilt_plate", &rooting);
}

/// **A brace carries no peel load, so it hands its trunk none.** A brace ends
/// on an arm's shaft, in the arm's own cell. The down-sweep let the brace read
/// that cell's per-layer event — an event the arm's own `max` over its events
/// had already covered and that the injection gate never counted — and hand it
/// to the arm as a merged load, summed on top. Every brace landing grew its
/// trunk by one cell's event that was never put into the raster, so the plate
/// saw more than was injected: `make_brace_pair` -0.0275 mm² at `d103c48`, 11
/// events of 0.0025 mm², and every model with braces the same way (Round 25
/// -0.085, the Cylinder -1.04 once its quills were fixed). On
/// `make_laid_tube` that phantom load grew a load-inserted arm from the minimum
/// root to 0.0521 mm after a mast had been placed 0.3606 mm from it, and the
/// mast's flared disc lapped the root by 0.0013 mm.
#[test]
fn a_brace_hands_its_trunk_no_load() {
    let mesh = plinth_mesh::make_brace_pair().expect("the brace pair fixture");
    let (mesh, plan) = swept(&mesh);
    let braces: Vec<_> = plan
        .struts
        .iter()
        .filter(|strut| strut.kind == plinth_sweep::StrutKind::Brace)
        .collect();
    let landed = braces
        .iter()
        .filter(|brace| matches!(brace.landing, plinth_sweep::Landing::Strut { .. }))
        .count();
    eprintln!(
        "make_brace_pair: {} braces, {landed} landed on a member; load gap {} mm2",
        braces.len(),
        plan.report.landing_check.load_gap_mm2
    );
    // NOT VACUOUS: braces must land on members for the claim to mean anything.
    assert!(landed > 0, "no brace lands on a member");
    let carrying: Vec<(u32, f64)> = braces
        .iter()
        .filter_map(|brace| {
            let most = brace.carried_mm2.iter().copied().fold(0.0_f64, f64::max);
            (most > 0.0).then_some((brace.id, most))
        })
        .collect();
    assert!(
        carrying.is_empty(),
        "braces carrying peel load: {carrying:?}"
    );
    let rooting = rooting(&mesh, &plan);
    assert_load_gap_closes("make_brace_pair", &rooting);
}

/// **The twenty landings that named a parent 3.0-3.4 mm away** (`quills.md`,
/// "Not established"). On the overhang table with the slenderness lane OFF,
/// the pre-lane `unmerge_overgrown` put nine over-grown trunks back on their
/// straight plate columns and left twenty children that had landed on the
/// leaned shafts naming a parent whose axis now stood 3.0-3.4 mm from their
/// base (children 29-32, 42-45, 60-63, 76-79, 87-88 at layers 456-457 and 98,
/// 100 at 461; parents 25, 27, 36, 38, 53, 55, 71, 73, 83). The geometric
/// check above missed them because each child still touched a neighbouring
/// member; the ledger did not, and the down-sweep booked their load onto a
/// parent that was not there. A trunk now moves with its branches, so every
/// landing is measured against where its parent stands.
///
/// This grades the ledger's own geometry — the distance between the child's
/// base cell and the cell its named parent occupies at the landing layer —
/// not a re-derivation of the rule. Measured: 20 at 3.0-3.4 mm with the
/// branches left behind, 0 now.
#[test]
fn every_landing_names_a_parent_standing_under_it_on_the_overhang_table() {
    let mesh = plinth_mesh::make_overhang_table().expect("the overhang table fixture");
    let mut policy = SweepPolicy::from_registry().expect("the registered sweep policy");
    policy.slenderness = false;
    let (_, plan) = swept_with(&mesh, &policy);
    let grid = &plan.grid;
    let mut landings = 0_usize;
    let mut off: Vec<(u32, u32, usize, f64)> = Vec::new();
    for strut in &plan.struts {
        let plinth_sweep::Landing::Strut { id, layer } = strut.landing else {
            continue;
        };
        landings += 1;
        let under = plan.struts[id as usize].cell_at(layer);
        let base = strut.base_cell();
        let distance = (grid.center_x_mm(grid.col_of(under)) - grid.center_x_mm(grid.col_of(base)))
            .hypot(grid.center_y_mm(grid.row_of(under)) - grid.center_y_mm(grid.row_of(base)));
        if distance > 0.0 {
            off.push((strut.id, id, layer, distance));
        }
    }
    eprintln!(
        "make_overhang_table, slenderness off: digest {}; {landings} member-on-member landings; \
         {} branches put back with their trunk; {} off their parent",
        plan.report.sweep_digest,
        plan.report.merges_moved_with_trunk,
        off.len()
    );
    assert!(
        landings > 0,
        "no member-on-member landing, so nothing was graded"
    );
    assert!(
        off.is_empty(),
        "landings whose named parent stands elsewhere (child, parent, layer, mm): {off:?}"
    );
    assert!(
        plan.report.merges_moved_with_trunk > 0,
        "the un-merge never moved a branch with its trunk, so this fixture no longer exercises it"
    );
}
