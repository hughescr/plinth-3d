//! **Craig's ruling of 2026-09-30: the design load ratio `X_ALLOWED` is 18.**
//!
//! `X_ALLOWED` is the supported model area one unit of arm section may carry:
//! a tip of radius `r` carries at most `X · π r²`. It was 12 =
//! `floor(lower(X_STAR_CONE) / 2)`, the plate-5 snap ratio with a 2x design
//! factor. The over-support attribution
//! (`plinth-evidence/.../field-2026-09-28/oversupport-attribution.md`) found
//! every law-sized tip at exactly `load / (π · 12)`, so that 2x was the whole
//! margin; Craig ruled it to 18. The registry test
//! (`plinth-core/tests/it/constants_evidence.rs`,
//! `the_design_load_ratio_is_craigs_ruling_and_sits_inside_the_measured_brackets`)
//! pins the number and the brackets. This file pins that the SWEEP follows it:
//!
//! 1. every law-sized arm tip on a flat fixture carries exactly
//!    `18 · π r²`, read against the registry, with the count of graded tips
//!    printed (a path-hit count: zero graded tips would grade nothing);
//! 2. the same field sized under a policy with `x_allowed = 12` has exactly
//!    `18 / 12 = 1.5` times the tip section PER UNIT OF TIP LOAD on those arms
//!    (the seats are identical; single-arm loads drift a little because a
//!    thinner arm occupies fewer raster cells, the field total does not), so
//!    the sizing reads the policy
//!    and no `12` is typed anywhere on the path;
//! 3. the run's disclosure text states the per-tip load ceiling
//!    `X · π · r_max²` from the X the run sized with (56.5487 mm² at 18,
//!    37.6991 mm² at 12). Before this lane the text carried a typed
//!    `37.6991`, which a change of X could not move.
//!
//! # Sabotage
//!
//! | change | what reddens |
//! |---|---|
//! | `X_ALLOWED` back to 12 in `plinth-core/src/constants/evidence.rs` | (1): the registry reads 12, not 18 |
//! | `SweepPolicy::from_registry` types `x_allowed: 12.0` | (1): every tip carries 12·πr², ratio 0.667 |
//! | the disclosure text types `37.6991` again | (3) at 18 |
//! | the down-sweep sizes with a typed 12 instead of `sizing.x_allowed` | (2): the section ratio reads 1.0, not 1.5 |

use crate::harness;

use harness::x_allowed;
use plinth_core::m0_constants;
use plinth_mesh::Mesh;
use plinth_sweep::{SweepPlan, SweepPolicy, StrutKind};

/// The flat overhang table: every arm is a lattice or island-fill arm on one
/// flat underside, so every tip is set by the area law (no creep flank, no
/// brace), which is what makes an exact grade possible.
fn table() -> Mesh {
    plinth_mesh::make_overhang_table().expect("the overhang table fixture")
}

fn registry_policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every sweep number")
}

fn clamps_mm() -> (f64, f64) {
    let constants = m0_constants().expect("the registry constructs");
    let placeholders = constants.placeholders();
    (
        placeholders.tip_radius_default_mm().diagnostic().get(),
        placeholders.tip_radius_max_legacy_mm().diagnostic().get(),
    )
}

/// `(tip radius, tip load)` of every ARM whose tip radius lies strictly inside
/// the registered clamps: the tips the area law, and nothing else, sized.
fn law_sized_tips(plan: &SweepPlan) -> Vec<(u32, f64, f64)> {
    let (minimum, maximum) = clamps_mm();
    plan.struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Arm)
        .filter_map(|strut| {
            let radius = *strut.radius_mm.last()?;
            let load = strut.tip_load_mm2();
            (radius > minimum * (1.0 + 1.0e-9) && radius < maximum * (1.0 - 1.0e-9))
                .then_some((strut.id, radius, load))
        })
        .collect()
}

#[test]
fn every_law_sized_tip_carries_eighteen_times_its_own_section() {
    let x = x_allowed();
    assert!(
        (x - 18.0).abs() < 1.0e-12,
        "X_ALLOWED is 18 by Craig's ruling of 2026-09-30; the registry reads {x}"
    );
    let policy = registry_policy();
    assert!(
        (policy.x_allowed - x).abs() < 1.0e-12,
        "the sweep policy must read X_ALLOWED from the registry: policy {} against registry {x}",
        policy.x_allowed
    );

    let plan = harness::sweep_with(&table(), 0.0, &policy);
    let arms = plan
        .struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Arm)
        .count();
    let graded = law_sized_tips(&plan);
    println!(
        "overhang table at X {x}: {arms} arms, {} law-sized tips graded (path hits)",
        graded.len()
    );
    assert!(
        !graded.is_empty(),
        "the fixture must produce law-sized tips, or this test grades nothing"
    );
    for (id, radius, load) in &graded {
        let capacity = x * core::f64::consts::PI * radius * radius;
        assert!(
            (capacity / load - 1.0).abs() < 1.0e-9,
            "arm {id}: tip radius {radius:.6} mm carries {load:.6} mm2 against a capacity of \
             X_ALLOWED * pi * r^2 = {capacity:.6} mm2 (ratio {:.6}); a law-sized tip sits at \
             exactly 1",
            capacity / load
        );
    }
}

#[test]
#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: scripts/gate-full.sh runs it")]
fn the_same_field_at_x_twelve_has_one_and_a_half_times_the_tip_section() {
    let at_registry = registry_policy();
    let mut at_twelve = registry_policy();
    at_twelve.x_allowed = 12.0;
    let mesh = table();
    let eighteen = harness::sweep_with(&mesh, 0.0, &at_registry);
    let twelve = harness::sweep_with(&mesh, 0.0, &at_twelve);

    // The overhang table is reach-bound: X decides the SIZE of its arms and not
    // where they stand, so the two fields must hold the same arms.
    let seats = |plan: &SweepPlan| {
        let mut cells: Vec<(usize, usize)> = plan
            .struts
            .iter()
            .filter(|strut| strut.kind == StrutKind::Arm)
            .map(|strut| (strut.cell, strut.k_top))
            .collect();
        cells.sort_unstable();
        cells
    };
    assert_eq!(
        seats(&eighteen),
        seats(&twelve),
        "X must not move a reach-bound seat on the overhang table"
    );

    let by_id = |plan: &SweepPlan| law_sized_tips(plan);
    let at_18 = by_id(&eighteen);
    let at_12 = by_id(&twelve);
    // The tip LOAD is not exactly X-independent: a thinner arm occupies fewer
    // raster cells, so some load routes differently (measured: arm 0 carries
    // 5.1075 mm2 at 18 against 5.1050 at 12; the largest drift on one arm is
    // 7.8 %). The law is
    // compared per unit of load, which is what X is: section / load = 1/(X·π).
    let mut compared = 0_usize;
    let mut load_drift_max = 0.0_f64;
    for (id, radius_18, load_18) in &at_18 {
        let Some((_, radius_12, load_12)) = at_12.iter().find(|(other, ..)| other == id) else {
            continue;
        };
        load_drift_max = load_drift_max.max((load_18 / load_12 - 1.0).abs());
        let section_per_load_ratio =
            (radius_12 * radius_12 / load_12) / (radius_18 * radius_18 / load_18);
        assert!(
            (section_per_load_ratio - 1.5).abs() < 1.0e-9,
            "arm {id}: section per unit load at X 12 must be 18/12 = 1.5 times that at X 18; \
             measured {section_per_load_ratio:.9}"
        );
        compared += 1;
    }
    println!(
        "overhang table: {compared} tips compared between X 18 and X 12 (path hits); largest \
         tip-load drift {:.3e}; tip scar {:.4} mm2 at 18 against {:.4} mm2 at 12",
        load_drift_max, eighteen.report.total_scar_mm2, twelve.report.total_scar_mm2
    );
    // Per arm the drift is not small on a lightly loaded arm (measured 7.8 %),
    // but the TOTAL tip load of the field is the table's own peel and must not
    // move with X.
    let total = |tips: &[(u32, f64, f64)]| tips.iter().map(|(_, _, load)| load).sum::<f64>();
    let (total_18, total_12) = (total(&at_18), total(&at_12));
    assert!(
        (total_18 / total_12 - 1.0).abs() < 1.0e-3,
        "the field's total tip load moved with X: {total_18:.4} mm2 at 18 against {total_12:.4} \
         mm2 at 12"
    );
    assert!(compared > 0, "no tip was compared, so this test graded nothing");
}

#[test]
#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: scripts/gate-full.sh runs it")]
fn the_disclosure_states_the_tip_ceiling_from_the_x_the_run_sized_with() {
    let (_, maximum) = clamps_mm();
    let mesh = table();
    // Explicit values, not `x_allowed()`: while the registry read 12 a loop
    // over [registry, 12] graded 12 twice and passed on the typed 37.6991.
    for x in [18.0, 12.0] {
        let mut policy = registry_policy();
        policy.x_allowed = x;
        let plan = harness::sweep_with(&mesh, 0.0, &policy);
        let text = plan.report.render_disclosure();
        let ceiling = format!(
            "X_ALLOWED * pi * r_max^2 = {:.4} mm2",
            x * core::f64::consts::PI * maximum * maximum
        );
        assert!(
            text.contains(&ceiling),
            "at X {x} the disclosure must state {ceiling:?}; it said:\n{text}"
        );
        if (x - 12.0).abs() > 1.0e-12 {
            assert!(
                !text.contains("37.6991"),
                "at X {x} the disclosure still carries the X-12 ceiling 37.6991 mm2:\n{text}"
            );
        }
    }
}
