//! **A self-supporting wall draws no arm from its own creep.**
//!
//! Field feedback 2026-09-28 (`plinth-evidence/slice-sweep-2026-09-02/
//! field-2026-09-28/round50.md` item 5, `quills.md` item 4): Round 50's drafted
//! rim is 74° from horizontal, where the registered overhang weight is
//! `w = 0.006`, and it still took **23 minimum-tip arms** at identity; flipped,
//! the recess's drafted inner wall took **78**. The Cylinder's steep flanks drew
//! demand the same way. The mechanism is the reach potential: it charged a
//! wall's slow outward creep at the full one-lamina cost whatever the wall's
//! angle, so a 74° wall and a 20° ramp ran out of reach alike.
//!
//! The rule this file grades (`crates/plinth-sweep/src/reach.rs`,
//! `creep_step`): a creep step onto a column that did not carry one layer down
//! is charged by the registered overhang weight of the ACTUAL mesh surface the
//! step crosses, relative to a horizontal lamina, and not by the raster step at
//! full cost.
//!
//! # What is measured
//!
//! Every DEMAND arm — an [`StrutKind::Arm`] the up-sweep placed for reach, so
//! not one the congestion or slenderness lanes inserted for load
//! (`inserted_by_load`) — whose tip is above the model's bottom layer. For each,
//! this file finds, by brute force over every face, the shallowest face within
//! the tip cell's own prism (the nearest face when none is that close), and
//! reads that face's angle from horizontal off its own vertices. The
//! test re-uses no production code to decide which face a tip sits on or how
//! steep it is: `tip_landing` and `reach` do not appear below.
//!
//! The threshold is 60°: the registered logistic (centre 45°, 10-to-90 width
//! 25°) gives `w(60°) = 0.067` and `w(74°) = 0.006`, so a wall at or above 60°
//! is self-supporting in any practical sense.
//!
//! # The fixtures
//!
//! The `Round_*.stl` and `Cylinder.stl` corpus files are not committed, so both
//! are parametric here:
//!
//! * **Round 50**, revolved from the measured profile of `corpus/models/
//!   Round_50.stl` (`round50-probe/geom.txt`): plain face at `z = 0`, r ≤ 24;
//!   outer rim drafted `r 24 → 25` over `z 0 → 3.5` (74.05°); top annulus at
//!   `z = 3.5` from the recess lip at `r = 23.0` out to the rim; recess wall
//!   drafted `r 22.33 → 23.0` over `z 1.0 → 3.5` (75.0°); recess floor at
//!   `z = 1.0`; three vertical ring stiffeners from the floor to full height.
//!   `make_recessed_disc` is not used because its recess wall is vertical, and a
//!   vertical wall has no creep: the flipped defect lives on the DRAFT.
//! * **the Cylinder**, as `make_laid_tube`: a 32-sided prism lying on its side,
//!   whose facets step 11.25° at a time from the flat bottom facet to the
//!   vertical flank, so it carries steep facets at 67.5°, 78.75° and 90°.
//! * **a gable-roofed cavity** in a box (the overhang cut, owner ruling
//!   2026-09-30, at the end of this file): a face at or beyond 45° is not
//!   down-facing and charges no creep at all, so a cavity roofed at 50° needs
//!   no arm, while one roofed at 40° still does.

use crate::harness;

use geom_kernel::{closest_on_tri, vec3};
use harness::{layer_height_mm, sweep_with, z_span, LIFT_MM};
use plinth_mesh::{make_laid_tube, Mesh};
use plinth_sweep::{StrutKind, SweepPlan, SweepPolicy};

/// The steepness at and above which a wall is self-supporting for this test.
const STEEP_WALL_DEG: f64 = 60.0;

/// Revolve a closed `(r, z)` profile about the Z axis.
///
/// Test-local fixture geometry, not production logic. A profile vertex at
/// `r = 0` becomes one shared axis vertex rather than a degenerate ring, and
/// an edge along the axis contributes nothing. The winding is fixed by the
/// signed volume afterwards, so the profile's own direction does not matter.
fn revolve(profile: &[[f64; 2]], segments: usize) -> Mesh {
    let mut verts: Vec<[f64; 3]> = Vec::new();
    // Per profile vertex: the index of its first ring vertex, and whether it
    // is the single axis vertex.
    let mut ring_start: Vec<(u32, bool)> = Vec::with_capacity(profile.len());
    for point in profile {
        let start = u32::try_from(verts.len()).expect("fixture size");
        if point[0] == 0.0 {
            verts.push([0.0, 0.0, point[1]]);
            ring_start.push((start, true));
        } else {
            for segment in 0..segments {
                let turn = 2.0 * core::f64::consts::PI * segment as f64 / segments as f64;
                verts.push([point[0] * turn.cos(), point[0] * turn.sin(), point[1]]);
            }
            ring_start.push((start, false));
        }
    }
    let seg = u32::try_from(segments).expect("fixture size");
    let at = |(start, axis): (u32, bool), segment: u32| -> u32 {
        if axis {
            start
        } else {
            start + segment % seg
        }
    };
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for index in 0..profile.len() {
        let a = ring_start[index];
        let b = ring_start[(index + 1) % profile.len()];
        if a.1 && b.1 {
            continue;
        }
        for segment in 0..seg {
            let (a0, a1) = (at(a, segment), at(a, segment + 1));
            let (b0, b1) = (at(b, segment), at(b, segment + 1));
            if a.1 {
                tris.push([a0, b1, b0]);
            } else if b.1 {
                tris.push([a0, a1, b0]);
            } else {
                tris.push([a0, a1, b1]);
                tris.push([a0, b1, b0]);
            }
        }
    }
    let signed_volume: f64 = tris
        .iter()
        .map(|tri| {
            let [a, b, c] = tri.map(|index| verts[index as usize]);
            vec3::dot(a, vec3::cross(b, c)) / 6.0
        })
        .sum();
    if signed_volume < 0.0 {
        for tri in &mut tris {
            tri.swap(1, 2);
        }
    }
    Mesh::new(verts, tris).expect("a closed revolved profile is a valid mesh")
}

/// Round 50 at its authored pose: plain face down, stiffener side up.
fn round_50() -> Mesh {
    let mut profile = vec![
        [0.0, 0.0],
        [24.0, 0.0],
        [25.0, 3.5],
        [23.0, 3.5],
        [22.33, 1.0],
    ];
    // Three vertical ring stiffeners, floor to full height, outermost first.
    for (outer, inner) in [(16.62, 16.12), (9.87, 9.37), (3.12, 2.62)] {
        profile.extend([[outer, 1.0], [outer, 3.5], [inner, 3.5], [inner, 1.0]]);
    }
    profile.push([0.0, 1.0]);
    revolve(&profile, 128)
}

/// Half a turn about X: the stiffener side goes down and the plain face up.
fn flipped(mesh: &Mesh) -> Mesh {
    let verts = mesh.verts().iter().map(|v| [v[0], -v[1], -v[2]]).collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rigid rotation preserves a valid mesh")
}

/// One demand arm's tip, with the face this file measured under it.
#[derive(Debug)]
#[allow(
    dead_code,
    reason = "read only through Debug, in the failure message and the census print"
)]
struct SteepTip {
    strut: usize,
    k_top: usize,
    tip_radius_mm: f64,
    face_deg: f64,
}

/// What the tip census found on one plan.
struct Census {
    /// Demand arms above the bottom layer that were checked at all. The
    /// path-hit witness that the check ran on something.
    checked: usize,
    /// Demand arms at the bottom layer, not graded.
    at_bottom: usize,
    /// Arms inserted for load (congestion flank arms, slenderness masts), not
    /// graded, but counted on steep walls so the report can say whether the
    /// congestion lane still seats there.
    load_arms_on_steep: usize,
    steep: Vec<SteepTip>,
}

/// A face's angle from horizontal, in degrees, read off its own vertices: 0
/// for a ceiling facing straight down, 90 for a wall, 180 for a floor.
fn face_angle_deg(mesh: &Mesh, face: usize) -> f64 {
    let [a, b, c] = mesh.tris()[face].map(|index| mesh.verts()[index as usize]);
    let normal = vec3::normalize(vec3::cross(vec3::sub(b, a), vec3::sub(c, a)))
        .expect("the fixtures carry no degenerate face");
    (-normal[2]).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The angle of the surface a tip carries: among the faces that pass through
/// the tip cell's own prism — within `reach_mm` of the tip point, the distance
/// from a cell centre on the slicing plane to the far corner of the cell's
/// square one layer down — the most overhanging one (smallest angle from
/// horizontal). A tip in the corner where a flat underside meets a vertical
/// face carries the underside, and reads as the underside. With no face that
/// close, the nearest face.
fn carried_face_deg(mesh: &Mesh, point: [f64; 3], reach_mm: f64) -> f64 {
    let mut nearest = (f64::INFINITY, 0_usize);
    let mut shallowest = f64::INFINITY;
    for (face, tri) in mesh.tris().iter().enumerate() {
        let [a, b, c] = tri.map(|index| mesh.verts()[index as usize]);
        let offset = vec3::sub(point, closest_on_tri(point, a, b, c));
        let distance = vec3::dot(offset, offset);
        if distance < nearest.0 {
            nearest = (distance, face);
        }
        if distance <= reach_mm * reach_mm {
            shallowest = shallowest.min(face_angle_deg(mesh, face));
        }
    }
    if shallowest.is_finite() {
        shallowest
    } else {
        face_angle_deg(mesh, nearest.1)
    }
}

fn census(mesh: &Mesh, plan: &SweepPlan) -> Census {
    let (min_z, _) = z_span(mesh);
    let height = plan.layer_height_mm;
    let plane = |layer: usize| plan.plate_z_mm + (layer as f64 + 0.5) * height;
    // The model's bottom layer: the first slicing plane at or above its lowest
    // point.
    let bottom = (0..plan.layers)
        .find(|layer| plane(*layer) >= min_z)
        .expect("the model has a bottom layer");
    let half = 0.5 * plan.grid.pitch_mm();
    let prism_reach = (2.0 * half * half + height * height).sqrt();
    let mut out = Census {
        checked: 0,
        at_bottom: 0,
        load_arms_on_steep: 0,
        steep: Vec::new(),
    };
    for (index, strut) in plan.struts.iter().enumerate() {
        if strut.kind != StrutKind::Arm {
            continue;
        }
        if strut.k_top <= bottom {
            out.at_bottom += usize::from(!strut.inserted_by_load);
            continue;
        }
        let tip = [strut.center_mm[0], strut.center_mm[1], plane(strut.k_top)];
        let face_deg = carried_face_deg(mesh, tip, prism_reach);
        if strut.inserted_by_load {
            out.load_arms_on_steep += usize::from(face_deg >= STEEP_WALL_DEG);
            continue;
        }
        out.checked += 1;
        if face_deg >= STEEP_WALL_DEG {
            out.steep.push(SteepTip {
                strut: index,
                k_top: strut.k_top,
                tip_radius_mm: strut.tip_radius_mm(),
                face_deg,
            });
        }
    }
    out
}

fn production() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every number the sweep reads")
}

fn measure(what: &str, mesh: &Mesh) -> (SweepPlan, Census) {
    measure_with(what, mesh, &production())
}

fn measure_with(what: &str, mesh: &Mesh, policy: &SweepPolicy) -> (SweepPlan, Census) {
    let plan = sweep_with(mesh, 0.0, policy);
    let found = census(mesh, &plan);
    let arms = plan
        .struts
        .iter()
        .filter(|s| s.kind == StrutKind::Arm)
        .count();
    let hits = plan.report.counters;
    println!(
        "{what}: {} struts ({arms} arms), demand arms above the bottom layer {}, at the bottom \
         layer {}, demand arms on a wall >= {STEEP_WALL_DEG} deg {}, load-inserted arms on such a \
         wall {}; creep cells weighted {}, creep steps taken {}; residual demand {} cells, \
         uncarried {:.4} mm2; lift {LIFT_MM} mm, layer {} mm",
        plan.struts.len(),
        found.checked,
        found.at_bottom,
        found.steep.len(),
        found.load_arms_on_steep,
        hits.creep_cells_weighted,
        hits.creep_steps_taken,
        plan.report.residual_demand_cells,
        plan.report.uncarried_load_mm2,
        layer_height_mm(),
    );
    for tip in found.steep.iter().take(12) {
        println!("    {tip:?}");
    }
    (plan, found)
}

fn assert_no_steep_demand_arm(what: &str, found: &Census) {
    assert!(
        found.steep.is_empty(),
        "{what}: {} demand arm(s) tip onto a wall {STEEP_WALL_DEG} deg or steeper above the bottom \
         layer, first {:?}; a self-supporting wall's creep must not unroot it",
        found.steep.len(),
        found.steep.first(),
    );
}

/// The weighted creep rule RAN and changed a decision input on this plan, so
/// a green census cannot mean the path never fired.
fn assert_rule_fired(what: &str, plan: &SweepPlan) {
    let hits = plan.report.counters;
    assert!(
        hits.creep_cells_weighted > 0 && hits.creep_steps_taken > 0,
        "{what}: creep cells weighted {}, steps taken {}; the rule did not fire",
        hits.creep_cells_weighted,
        hits.creep_steps_taken,
    );
}

#[test]
fn round_50_at_identity_draws_no_arm_onto_its_drafted_rim() {
    let mesh = round_50();
    let (plan, found) = measure("Round 50 identity", &mesh);
    assert!(
        found.at_bottom > 0,
        "the plain underside must still be supported"
    );
    assert_rule_fired("Round 50 identity", &plan);
    assert_no_steep_demand_arm("Round 50 identity", &found);
}

#[test]
fn round_50_flipped_draws_no_arm_onto_its_drafted_recess_wall() {
    let mesh = flipped(&round_50());
    let (plan, found) = measure("Round 50 flipped", &mesh);
    // The recess floor is a ceiling at this pose, 2.5 mm up: its arms are the
    // path-hit witness that arms above the bottom layer were checked at all.
    assert!(
        found.checked > 0,
        "the flipped recess ceiling must still be supported"
    );
    assert_rule_fired("Round 50 flipped", &plan);
    assert_no_steep_demand_arm("Round 50 flipped", &found);
}

#[test]
fn a_laid_cylinder_draws_no_arm_onto_its_steep_flanks() {
    let mesh = make_laid_tube().expect("the laid tube fixture");
    let (plan, found) = measure("laid tube", &mesh);
    assert!(
        found.checked > 0,
        "the tube's lower hull must still be supported above its feet"
    );
    assert_rule_fired("laid tube", &plan);
    assert_no_steep_demand_arm("laid tube", &found);
}

/// Sabotage, kept in the tree: put the full-cost creep back
/// (`SweepPolicy::without_creep_weight`) and the drafted rim draws its
/// minimum-tip arms again. Measured before the fix, at d103c48: 23 on this
/// fixture, which is exactly the count the field report measured on the real
/// `Round_50.stl`.
#[test]
fn the_full_cost_creep_draws_the_rim_arms_again() {
    let mesh = round_50();
    let policy = production().without_creep_weight();
    let (plan, found) = measure_with("Round 50 identity, full-cost creep", &mesh, &policy);
    assert_eq!(
        plan.report.counters.creep_cells_weighted, 0,
        "the knob must switch the rule off"
    );
    assert!(
        !found.steep.is_empty(),
        "with the full-cost creep the drafted rim drew no arm, so this file cannot tell the rule \
         from its absence"
    );
}

// ---------------------------------------------------------------------------
// The overhang cut (owner ruling 2026-09-30)
// ---------------------------------------------------------------------------
//
// A face at or beyond `OVERHANG_ANGLE_DEG` (45°) is not down-facing by the
// engine's own test (`n_z < -sin 45°`, the orientation fold and plinth-support's
// contact perception), and it charges NO creep. Before the ruling it paid the
// logistic's tail: ω(50°) = 0.293 of a lamina per pitch of creep. The
// creep-arms diagnosis (`plinth-evidence/slice-sweep-2026-09-02/
// field-2026-09-28/creep-arms-diagnosis.md`, items 5-6) found that tail
// deciding 95-99 % of the Cylinder's and Sphere's cavity-ceiling demand.
//
// The fixture is a box with a gable-roofed cavity. Every face of the cavity
// above its floor is a vertical wall or a roof plane at `roof_deg`. Arithmetic
// before any code ran, at 50°: the roof closes 10 mm in from each eave, so the
// tail charges `10 × 0.293 / 2.130 = 1.37` of a lamina at the ridge, and the
// cells within about 2.7 mm of the ridge are demand; the gable ends root only
// the 2.13 mm next to them, so the middle of the ridge needs arms. Under the cut
// the same roof charges 0, so the cavity needs no arm at all. At 40° the roof is
// down-facing, the cut does not touch it, and ω(40°) = 0.707 makes the whole
// middle of the roof demand.

/// Cavity half-width across X, from the centre to each eave.
const CAVITY_HALF_WIDTH_MM: f64 = 10.0;
/// Cavity half-length along Y; the gable ends stand here.
const CAVITY_HALF_LENGTH_MM: f64 = 7.0;
const CAVITY_FLOOR_Z_MM: f64 = 3.0;
const CAVITY_EAVE_Z_MM: f64 = 5.0;
const SHELL_MM: f64 = 3.0;

/// A closed prism: the convex `(x, z)` polygon `section` extruded along Y over
/// `[-half_length, half_length]`, wound OUTWARD (`inward == false`) or into the
/// void (`inward == true`, a cavity). Test-local fixture geometry; the winding
/// is fixed by the signed volume, as in [`revolve`].
fn extrude_xz(
    section: &[[f64; 2]],
    half_length: f64,
    inward: bool,
) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    let n = u32::try_from(section.len()).expect("fixture size");
    let mut verts: Vec<[f64; 3]> = Vec::with_capacity(2 * section.len());
    for y in [-half_length, half_length] {
        for point in section {
            verts.push([point[0], y, point[1]]);
        }
    }
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for i in 0..n {
        let j = (i + 1) % n;
        tris.push([i, j, n + j]);
        tris.push([i, n + j, n + i]);
    }
    for i in 1..n - 1 {
        tris.push([0, i + 1, i]);
        tris.push([n, n + i, n + i + 1]);
    }
    let signed_volume: f64 = tris
        .iter()
        .map(|tri| {
            let [a, b, c] = tri.map(|index| verts[index as usize]);
            vec3::dot(a, vec3::cross(b, c)) / 6.0
        })
        .sum();
    if (signed_volume < 0.0) != inward {
        for tri in &mut tris {
            tri.swap(1, 2);
        }
    }
    (verts, tris)
}

/// A solid box holding a gable-roofed cavity whose roof planes rise at
/// `roof_deg` from horizontal, with a 3 mm shell all round.
fn gable_cavity_box(roof_deg: f64) -> Mesh {
    let ridge_z = CAVITY_EAVE_Z_MM + CAVITY_HALF_WIDTH_MM * roof_deg.to_radians().tan();
    let outer = [
        [-CAVITY_HALF_WIDTH_MM - SHELL_MM, 0.0],
        [CAVITY_HALF_WIDTH_MM + SHELL_MM, 0.0],
        [CAVITY_HALF_WIDTH_MM + SHELL_MM, ridge_z + SHELL_MM],
        [-CAVITY_HALF_WIDTH_MM - SHELL_MM, ridge_z + SHELL_MM],
    ];
    let cavity = [
        [-CAVITY_HALF_WIDTH_MM, CAVITY_FLOOR_Z_MM],
        [CAVITY_HALF_WIDTH_MM, CAVITY_FLOOR_Z_MM],
        [CAVITY_HALF_WIDTH_MM, CAVITY_EAVE_Z_MM],
        [0.0, ridge_z],
        [-CAVITY_HALF_WIDTH_MM, CAVITY_EAVE_Z_MM],
    ];
    let (mut verts, mut tris) = extrude_xz(&outer, CAVITY_HALF_LENGTH_MM + SHELL_MM, false);
    let (inner_verts, inner_tris) = extrude_xz(&cavity, CAVITY_HALF_LENGTH_MM, true);
    let offset = u32::try_from(verts.len()).expect("fixture size");
    verts.extend(inner_verts);
    tris.extend(inner_tris.iter().map(|tri| tri.map(|index| index + offset)));
    Mesh::new(verts, tris).expect("a box with a closed cavity is a valid mesh")
}

/// Demand arms (placed for reach, not inserted for load) whose tip is inside
/// the cavity: above its floor and within its footprint.
fn cavity_arms(plan: &SweepPlan) -> usize {
    let plane = |layer: usize| plan.plate_z_mm + (layer as f64 + 0.5) * plan.layer_height_mm;
    plan.struts
        .iter()
        .filter(|strut| {
            strut.kind == StrutKind::Arm
                && !strut.inserted_by_load
                && plane(strut.k_top) > CAVITY_FLOOR_Z_MM
                && strut.center_mm[0].abs() < CAVITY_HALF_WIDTH_MM
                && strut.center_mm[1].abs() < CAVITY_HALF_LENGTH_MM
        })
        .count()
}

fn measure_cavity(what: &str, roof_deg: f64, policy: &SweepPolicy) -> (SweepPlan, usize) {
    let mesh = gable_cavity_box(roof_deg);
    let (plan, found) = measure_with(what, &mesh, policy);
    assert!(
        found.at_bottom > 0,
        "{what}: the box's underside must still be supported (census witness)"
    );
    let inside = cavity_arms(&plan);
    println!(
        "{what}: cavity arms {inside}; creep cells priced at zero by the overhang cut {}",
        plan.report.counters.creep_cells_free
    );
    (plan, inside)
}

#[test]
fn a_cavity_roofed_at_50_deg_draws_no_creep_arm() {
    let (plan, inside) = measure_cavity("gable 50 deg", 50.0, &production());
    assert_rule_fired("gable 50 deg", &plan);
    assert!(
        plan.report.counters.creep_cells_free > 0,
        "the overhang cut priced no creep cell at zero, so this test cannot see it"
    );
    assert_eq!(
        inside, 0,
        "a roof at 50 deg is not down-facing; its creep must not unroot the cavity"
    );
    assert_eq!(plan.report.residual_demand_cells, 0, "nothing is left bare");
    assert!(
        plan.report.uncarried_load_mm2 <= 0.0,
        "uncarried load {:.4} mm2",
        plan.report.uncarried_load_mm2
    );
}

/// Sabotage, kept in the tree: let the 50° roof pay the logistic's tail
/// (`SweepPolicy::without_creep_overhang_cut`) and the cavity draws its creep
/// arms again. This is the rule before the 2026-09-30 ruling, so it is also
/// the red the test above was written against.
#[test]
fn the_logistic_tail_draws_the_cavity_arms_again() {
    let policy = production().without_creep_overhang_cut();
    let (plan, inside) = measure_cavity("gable 50 deg, logistic tail", 50.0, &policy);
    assert_eq!(
        plan.report.counters.creep_cells_free, 0,
        "the knob must switch the cut off"
    );
    assert!(
        inside > 0,
        "with the tail the 50 deg roof drew no arm, so this file cannot tell the cut from its \
         absence"
    );
}

/// The cut drops no down-facing demand: a 40° roof is down-facing, pays
/// ω(40°) = 0.707, and its cavity is still supported from inside.
#[test]
fn a_cavity_roofed_at_40_deg_still_draws_its_arms() {
    let (plan, inside) = measure_cavity("gable 40 deg", 40.0, &production());
    assert!(
        inside > 0,
        "a down-facing 40 deg roof lost its cavity arms under the overhang cut"
    );
    assert_eq!(plan.report.residual_demand_cells, 0, "nothing is left bare");
}
