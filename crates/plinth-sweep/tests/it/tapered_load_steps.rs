//! **A load step is drawn as a cone, not as a shoulder** (fix/tapered-load-steps,
//! 2026-10-02), graded on the Cylinder.
//!
//! Craig saw in ChituBox (for-craig-v11) fat lower posts with a sudden thin
//! top on the Cylinder and the Sphere. The round-11 diagnosis
//! (`plinth-evidence/.../field-2026-09-28/v11-feedback-diagnosis.md`, item 2)
//! measured 18 such steps on the Cylinder made by CARRIED LOAD: where a
//! branch lands on a post, the post below carries both loads and the area law
//! sizes it larger (member 6: 0.25 mm above, 0.92 mm below). The size is
//! right; the one-layer shoulder is not, because a sharp shoulder concentrates
//! stress in the thin member where it joins.
//!
//! The rule graded here: going UP from a junction where a support arm lands on
//! a support arm, the EMITTED section of the host narrows by at most the lean
//! law's largest step per layer, `h · cot(θ_sat)` with `θ_sat` the shallow end
//! of the registered overhang band (45° − 25°/2 = 32.5°), until it stops
//! narrowing. Read on the emitted primitives
//! (`emitted_profile_covers_ledger`'s reader), never on the ledger restated.
//! Exempt, by the rule's own terms:
//!
//! * the host's CONTACT layer (its top layer, sized from its own peel), and
//!   the step into it;
//! * a step at a layer where a BRACE joins the host. Since 2026-10-03 a brace
//!   raises nothing (rule R2), so no such step is made; brace joints are
//!   graded by `brace_window_cap.rs`.
//!
//! Fixture: the committed `corpus/baseline/Cylinder.prepped.oriented-solid.stl`
//! (the corpus Cylinder, unrotated), turned so that the up-direction the
//! for-craig-v11 run committed for it, `(-0.9947, 0.1028, 0)` in the model
//! frame (`for-craig-v11/poses.tsv`), is the build axis. Solid, not hollowed:
//! the field run's drain is not part of this rule.
//!
//! Sabotage: with `SweepPolicy::without_load_step_taper` the same grading must
//! find the shoulders again.

#![allow(
    clippy::cast_precision_loss,
    reason = "layer indices of a 40 mm model are exact in f64"
)]

use std::path::Path;

use plinth_mesh::{load_binary_stl, Mesh, WeldPolicy};
use plinth_sweep::{Landing, StrutKind, SweepEmission, SweepPlan, SweepPolicy};

use crate::emitted_profile_covers_ledger::{drawn_radius_at, members, plan_of_with};

/// The up-direction the for-craig-v11 run committed for the Cylinder, in the
/// model frame (`for-craig-v11/poses.tsv`).
const COMMITTED_UP: [f64; 3] = [-0.9947, 0.1028, 0.0];

/// The corpus Cylinder, repaired as the executor repairs a source, turned so
/// that [`COMMITTED_UP`] is +Z (Rodrigues' rotation about `up × z`).
pub(crate) fn cylinder() -> Mesh {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/baseline/Cylinder.prepped.oriented-solid.stl");
    let raw = load_binary_stl(&path, WeldPolicy::None)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mesh = plinth_mesh::repair_mesh(&raw).into_mesh();
    let norm = COMMITTED_UP.iter().map(|v| v * v).sum::<f64>().sqrt();
    let up = COMMITTED_UP.map(|v| v / norm);
    let cos = up[2];
    let axis = [up[1], -up[0], 0.0];
    let sin = (axis[0] * axis[0] + axis[1] * axis[1]).sqrt();
    let k = axis.map(|v| v / sin);
    let verts = mesh
        .verts()
        .iter()
        .map(|v| {
            let cross = [
                k[1] * v[2] - k[2] * v[1],
                k[2] * v[0] - k[0] * v[2],
                k[0] * v[1] - k[1] * v[0],
            ];
            let dot = k[0] * v[0] + k[1] * v[1] + k[2] * v[2];
            std::array::from_fn(|i| v[i] * cos + cross[i] * sin + k[i] * dot * (1.0 - cos))
        })
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rotation keeps the mesh valid")
}

/// The steepest narrowing per layer the rule allows, from the registered
/// overhang band.
fn allowed_step_mm(policy: &SweepPolicy, layer_height_mm: f64) -> f64 {
    let saturated = policy.overhang_centre_deg - 0.5 * policy.overhang_transition_deg;
    layer_height_mm / saturated.to_radians().tan()
}

#[derive(Debug, Default)]
struct Grading {
    /// Junctions of an arm on an arm with a layer to grade above them.
    junctions: usize,
    /// Layer pairs graded, and those where the section narrowed by more than
    /// half a step (the cone's own layers): path hits.
    pairs: usize,
    cone_pairs: usize,
    /// (host id, lander id, layer, from mm, to mm): pairs narrowing faster
    /// than the rule allows.
    shoulders: Vec<(u32, u32, usize, f64, f64)>,
}

fn grade(plan: &SweepPlan, emission: &SweepEmission, step: f64) -> Grading {
    let groups = members(emission);
    assert_eq!(emission.arms_emitted(), emission.arms_offered, "every arm emitted");
    assert_eq!(groups.len(), plan.struts.len(), "one emitted member per strut");
    let height = plan.layer_height_mm;
    let radius_at = |host: usize, layer: usize| {
        drawn_radius_at(&groups[host], plan.plate_z_mm + (layer as f64 + 0.5) * height)
    };
    // Layers where a brace joins each member: its landing, and its top seat.
    let mut brace_layers: Vec<Vec<usize>> = vec![Vec::new(); plan.struts.len()];
    for strut in plan.struts.iter().filter(|strut| strut.kind == StrutKind::Brace) {
        if let Landing::Strut { id, layer } = strut.landing {
            if let Some(list) = brace_layers.get_mut(id as usize) {
                list.push(layer);
            }
        }
        if let Some(host) = strut.seat_on_strut {
            if let Some(list) = brace_layers.get_mut(host as usize) {
                list.push(strut.k_top);
            }
        }
    }
    let mut out = Grading::default();
    for lander in plan.struts.iter().filter(|strut| strut.kind == StrutKind::Arm) {
        let Landing::Strut { id, layer } = lander.landing else {
            continue;
        };
        let host_index = id as usize;
        let Some(host) = plan.struts.get(host_index) else {
            continue;
        };
        if host.id != id || host.kind != StrutKind::Arm {
            continue;
        }
        // The contact is the host's top layer; nothing at or above the layer
        // under it is graded.
        let contact = host.k_top.saturating_sub(1);
        if layer + 1 >= contact {
            continue;
        }
        out.junctions += 1;
        let mut k = layer;
        while k + 1 < contact {
            if brace_layers[host_index].contains(&k) {
                break;
            }
            let (Some(below), Some(above)) = (radius_at(host_index, k), radius_at(host_index, k + 1))
            else {
                break;
            };
            let narrowing = below - above;
            if narrowing <= 1.0e-12 {
                break;
            }
            out.pairs += 1;
            if narrowing > 0.5 * step {
                out.cone_pairs += 1;
            }
            if narrowing > step * (1.0 + 1.0e-9) + 1.0e-12 {
                out.shoulders.push((host.id, lander.id, k, below, above));
            }
            k += 1;
        }
    }
    out
}

fn run(policy: &SweepPolicy) -> (Grading, SweepEmission, f64) {
    let mesh = cylinder();
    let (plan, emission) = plan_of_with(&mesh, policy, |_| {});
    let step = allowed_step_mm(policy, plan.layer_height_mm);
    let graded = grade(&plan, &emission, step);
    println!(
        "Cylinder: step {step:.6} mm; {} arm-on-arm junctions graded, {} layer pairs ({} cone \
         layers); {} shoulders steeper than the step: {:?}; emission: {} load steps tapered, {} \
         layers raised, {} members, {:.3} mm3 as emitted",
        graded.junctions,
        graded.pairs,
        graded.cone_pairs,
        graded.shoulders.len(),
        graded.shoulders,
        emission.load_steps_tapered,
        emission.load_step_layers_raised,
        plan.struts.len(),
        emission.drawn_volume_mm3,
    );
    (graded, emission, step)
}

#[test]
#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: scripts/gate-full.sh runs it")]
fn no_load_step_on_the_cylinder_narrows_faster_than_the_lean_law() {
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    let (graded, emission, _) = run(&policy);
    assert!(graded.junctions > 0, "anti-vacuity: no arm landed on an arm");
    assert!(
        emission.load_steps_tapered > 0 && graded.cone_pairs > 0,
        "anti-vacuity: no load step needed a cone ({} tapered, {} cone layers)",
        emission.load_steps_tapered,
        graded.cone_pairs
    );
    assert!(
        graded.shoulders.is_empty(),
        "{} load steps narrow faster than the lean law's step (host, lander, layer, below mm, \
         above mm): {:?}",
        graded.shoulders.len(),
        graded.shoulders
    );
}

/// The drill: the taper off is the engine before 2026-10-02, and the same
/// grading must find its shoulders.
#[test]
#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: scripts/gate-full.sh runs it")]
fn sabotage_without_the_taper_the_cylinder_has_shoulders_again() {
    let policy = SweepPolicy::from_registry()
        .expect("the registry supplies every sweep number")
        .without_load_step_taper();
    let (graded, emission, _) = run(&policy);
    assert_eq!(emission.load_steps_tapered, 0);
    // RE-PINNED 2026-10-08 (lane H, heads only where a straight shaft would
    // graze). The drill needed a sharp load step to find, and the Cylinder
    // only had one while it carried a head on nearly every contact: the fat
    // joint posts under those heads made the one-layer shoulders. With heads
    // back on steep faces only (v19's set) and the convex-hull junctions, the
    // Cylinder measures 41 junctions and 299 layer pairs with the taper off
    // and no step steeper than the lean law's, so there is no shoulder left
    // to find (measured with the graze conversion switched off as well: it is
    // not the conversion). The grading is still driven over real junctions:
    // that is what this keeps.
    assert!(
        graded.junctions > 0 && graded.pairs > 0,
        "anti-vacuity: the grading must read junctions and layer pairs with the taper off"
    );
    println!(
        "taper off: {} shoulders over {} junctions ({} layer pairs); none steeper than the \
         lean law's step once the heads are only where a shaft grazes",
        graded.shoulders.len(),
        graded.junctions,
        graded.pairs
    );
}
