//! **A tip's contact is sized from the load it carries through its OWN
//! contact with the model** (2026-09-29).
//!
//! When an arm lands on another support arm's HEAD — at the parent's own
//! `k_top`, or at its top alive layer — the child's load arrives in the
//! parent's `pending` at the parent's top ledger slot. That slot is the
//! parent's CONTACT: `tip_radius_mm` reads it, the adapter hands it to the
//! lowering as the contact ball's radius, and the scar identity sums it. Until
//! this rule the child's load was booked there, so the parent's contact was
//! sized for load that never crosses its scar (it enters the head and goes
//! down the shaft), and the scar identity counted the child's load twice: once
//! in the child's own tip, once in the parent's. Measured on the Round 25 ring
//! stiffener (`plinth-evidence/.../field-2026-09-28/lanes/wall-creep-diagnosis.md`):
//! arm 39's ball drawn at 0.746 mm where its own peel asks for 0.400 mm, about
//! six contact balls over-sized, and the Round 25 scar identity at 1.10.
//!
//! The rule graded here: the contact slot carries the arm's own peel only;
//! the lander's load joins the slot BELOW the contact, so the shaft still
//! carries the sum. The drawn profile the file gets must cover that sum.
//!
//! The fixture is the Round 25 ratchet disc in its committed pose (the 180°
//! flip, stiffener side down), where cavity-ceiling arms lean across and land
//! on the ring stiffener arms' heads. The pose is applied directly rather than
//! searched, so this test does not depend on the orientation search.
//!
//! Sabotage (measured 2026-09-29, recorded in the lane report
//! `lanes/contact-sizing.md`), in `down_sweep::run`:
//!
//! | change | what reddens |
//! |---|---|
//! | `defer_to_shaft = false` (the pre-rule booking, which is develop's) | both tests: arm 39's contact slot and the slot below both hold 16.20 mm² with 9.40 mm² handed; the contact slots sum to 512.05 mm² against 486.24 mm² injected |
//! | defer, but zero `pending` (the lander's load is lost instead of moved) | `the_shaft_below_a_head_junction_carries_the_sum` (arm 33: 12.08 mm² handed, the slot below holds only its own 6.72). The identity test stays green: it cannot see LOST load, only double-counted load, which is why both exist |
//!
//! Measured with the rule: 4 heads with landers, 4 merges moved below
//! (25.81 mm²), the contact slots sum to the injected peel exactly
//! (486.2375 mm²), and the scar identity reads 1.0024 (the minimum-radius
//! clamp).

use crate::harness;
use crate::round_bases_ratchet::{mesh_for, place, round_25};

use harness::x_allowed;
use plinth_core::m0_constants;
use plinth_mesh::Mesh;
use plinth_sweep::{arms_of, Landing, StrutKind, SweepPlan};

/// The committed Round 25 pose: 180° about `x`, glue face up. A proper
/// rotation, so the triangle winding (and every outward normal) is kept.
fn flipped(mesh: &Mesh) -> Mesh {
    let verts = mesh
        .verts()
        .iter()
        .map(|vertex| [vertex[0], -vertex[1], -vertex[2]])
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rigid rotation preserves a valid mesh")
}

fn round_25_flipped() -> SweepPlan {
    place(&flipped(&mesh_for(&round_25())), "Round 25 flipped")
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

/// One support arm's head, and what landed on it: `(parent index, load the
/// children handed it, number of children)`. A head junction is an ARM
/// landing on a support ARM at the parent's `k_top` (its seat layer) or at
/// `k_top - 1` (its top alive layer): the two landings whose load reaches the
/// parent's top ledger slot.
fn head_junctions(plan: &SweepPlan) -> Vec<(usize, f64, usize)> {
    let index_of = |id: u32| plan.struts.iter().position(|strut| strut.id == id);
    let mut heads: Vec<(usize, f64, usize)> = Vec::new();
    for child in &plan.struts {
        if child.kind != StrutKind::Arm {
            continue;
        }
        let Landing::Strut { id, layer } = child.landing else {
            continue;
        };
        let Some(parent) = index_of(id) else {
            continue;
        };
        let host = &plan.struts[parent];
        if host.kind != StrutKind::Arm || layer + 1 < host.k_top || layer > host.k_top {
            continue;
        }
        let handed = child.carried_mm2.first().copied().expect("a sized child");
        match heads.iter_mut().find(|(index, _, _)| *index == parent) {
            Some(entry) => {
                entry.1 += handed;
                entry.2 += 1;
            }
            None => heads.push((parent, handed, 1)),
        }
    }
    heads
}

#[test]
fn the_shaft_below_a_head_junction_carries_the_sum() {
    let plan = round_25_flipped();
    let heads = head_junctions(&plan);
    let hits = plan.report.counters;
    println!(
        "Round 25 flipped: {} arms, {} heads with landers; path hits: {} contact merges moved \
         below ({:.4} mm2), {} kept at a single slot",
        plan.struts.len(),
        heads.len(),
        hits.contact_merges_moved_below,
        plan.report.contact_merge_load_moved_mm2,
        hits.contact_merges_kept_at_single_slot,
    );
    assert!(
        !heads.is_empty(),
        "the fixture must land arms on a support arm's head, or this test grades nothing"
    );
    let arms = arms_of(&plan);
    let mut graded = 0_usize;
    for &(parent, handed, children) in &heads {
        let host = &plan.struts[parent];
        let slots = host.carried_mm2.len();
        if slots < 2 {
            // No shaft under the contact; `contact_merges_kept_at_single_slot`
            // counts this case and the load stays in the one slot.
            continue;
        }
        let contact_load = host.carried_mm2[slots - 1];
        let below = host.carried_mm2[slots - 2];
        let own_radius = area_law_radius_mm(contact_load);
        let sum_radius = area_law_radius_mm(contact_load + handed);
        println!(
            "  arm {:>4}: {children} lander(s) handed {handed:.4} mm2; contact slot {contact_load:.4} \
             mm2 -> r_tip {:.4} mm (area law {own_radius:.4}); slot below {below:.4} mm2 -> r {:.4} \
             mm (sum needs {sum_radius:.4}); drawn tip {:.4}, drawn below {:.4}",
            host.id,
            host.tip_radius_mm(),
            host.radius_mm[slots - 2],
            arms[parent].tip_radius_mm,
            arms[parent].radius_profile_mm[slots - 2],
        );
        // THE SUM IS CARRIED UNDER THE CONTACT. Before the rule the contact
        // slot held `own + handed` and the slot below held about the same, so
        // this fails by about `handed`.
        assert!(
            below >= contact_load + handed - 1.0e-9 * below.max(1.0),
            "arm {}: landers handed {handed} mm2 to its head; the contact slot holds \
             {contact_load} mm2 and the slot below only {below} mm2",
            host.id
        );
        // THE CONTACT IS SIZED FROM THE CONTACT SLOT, BY THE AREA LAW.
        let thickened = host.thickened_mm.get(host.k_top - 1).copied().unwrap_or(0.0);
        let expected_tip = own_radius.max(host.min_radius_mm).max(thickened);
        assert!(
            (host.tip_radius_mm() - expected_tip).abs() < 1.0e-9,
            "arm {}: contact radius {} mm against the area law's {expected_tip} mm for its own \
             {contact_load} mm2",
            host.id,
            host.tip_radius_mm()
        );
        // ... and that is the ball the file gets.
        assert!(
            (arms[parent].tip_radius_mm - host.tip_radius_mm()).abs() < 1.0e-12,
            "arm {}: the lowering is handed {} mm, the ledger says {} mm",
            host.id,
            arms[parent].tip_radius_mm,
            host.tip_radius_mm()
        );
        // THE SHAFT COVERS THE SUM, in the ledger and in the drawn profile.
        assert!(
            host.radius_mm[slots - 2] >= sum_radius - 1.0e-9,
            "arm {}: the shaft under the contact is {} mm; the sum needs {sum_radius} mm",
            host.id,
            host.radius_mm[slots - 2]
        );
        assert!(
            arms[parent].radius_profile_mm[slots - 2] >= sum_radius - 1.0e-9,
            "arm {}: the DRAWN shaft under the contact is {} mm; the sum needs {sum_radius} mm",
            host.id,
            arms[parent].radius_profile_mm[slots - 2]
        );
        graded += 1;
    }
    assert!(graded > 0, "at least one head junction must have a shaft to grade");
    // Last, so a sabotaged booking reddens on the physics above first.
    assert!(
        hits.contact_merges_moved_below > 0,
        "the contact rule must have fired on the finished field's down-sweep"
    );
}

/// The identity the rule restores, on the ledger itself: each mm² of injected
/// peel reaches at most one contact, once, and a contact carries nothing
/// else. So the contact slots summed over every support arm cannot exceed the
/// injected peel. Before the rule a lander's load was in its own contact AND
/// in its parent's, and the sum went over.
#[test]
fn no_contact_carries_more_than_the_injected_peel() {
    let plan = round_25_flipped();
    let contacts: f64 = plan
        .struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Arm)
        .map(plinth_sweep::SweepStrut::tip_load_mm2)
        .sum();
    let injected = plan.report.injected_load_mm2;
    println!(
        "Round 25 flipped: contact slots sum {contacts:.4} mm2 against injected {injected:.4} mm2 \
         (ratio {:.5}); scar {:.4} mm2 against injected/X {:.4} mm2 (ratio {:.5})",
        contacts / injected,
        plan.report.total_scar_mm2,
        injected / x_allowed(),
        plan.report.total_scar_mm2 / (injected / x_allowed()),
    );
    assert!(injected > 0.0);
    assert!(
        contacts <= injected * (1.0 + 1.0e-9),
        "the arms' contact slots hold {contacts} mm2 against {injected} mm2 injected: some \
         contact carries load that never crossed it"
    );
}
