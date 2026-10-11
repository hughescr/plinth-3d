//! **One support per spike** (owner ruling 2026-10-10, `DEINO-DIAGNOSIS.md`).
//!
//! A sharp tip first appears as a hairline that the raster covers but does not
//! carry, so the next layer seats a second arm one layer above the first. The
//! two tip balls overlap. The second arm's shaft then blocks the first arm's
//! head and leans away by itself. The rule: a seat whose tip ball overlaps the
//! ball of an arm already seated in the same column raises that arm to the new
//! layer instead of drawing a second arm, and the raised arm is sized for both
//! loads.
//!
//! | test | what it asserts | how it is kept from passing vacuously |
//! |---|---|---|
//! | `a_spike_gets_one_arm_not_two` | with the rule, no two arms of a straight spike stand with tip balls that overlap at the smallest tip, and the arm count falls | the merged-seat counter is positive with the rule on and zero off, and the off run MUST hold an overlapping pair |
//! | `a_merged_host_is_marked_and_stays_straight` | the arm a twin is merged into is marked `twin_host` and stays in its column | the marked-arm count must be positive with the rule on and zero off |
//! | `a_merged_spike_keeps_all_its_load` | the merge loses no load: the plate carries what it carried, nothing is left uncarried, and the two sweeps agree | the off run is the control, and the merged-seat counter must be positive |
//!
//! The second test is the one that failed the first form of the rule (the twin
//! seat dropped, the earlier arm left where it was): 75 mm2 of Deinonychus load
//! reached no carrier.
//!
//! SABOTAGE (run by hand): make `twin_host` in `up_sweep.rs` return `None` and
//! both tests go red; drop the `raise_host_seat` call and keep the early return
//! and the second goes red.

use plinth_mesh::Mesh;
use plinth_sweep::{StrutKind, SweepPlan, SweepPolicy, SweepStrut};

use crate::harness;

const PITCH_MM: f64 = 0.05;

/// A square pyramid, apex DOWN at the origin, its square top `height_mm` up,
/// `half_width_mm` either side and slid `lean_mm` along x: a spike. The only
/// thing under it is air.
fn spike(half_width_mm: f64, height_mm: f64, lean_mm: f64) -> Mesh {
    let (w, h) = (half_width_mm, height_mm);
    let verts = vec![
        [0.0, 0.0, 0.0],
        [-w + lean_mm, -w, h],
        [w + lean_mm, -w, h],
        [w + lean_mm, w, h],
        [-w + lean_mm, w, h],
    ];
    let tris = vec![[1, 2, 3], [1, 3, 4], [0, 2, 1], [0, 3, 2], [0, 4, 3], [0, 1, 4]];
    Mesh::new(verts, tris).expect("a closed pyramid")
}

fn arms(plan: &SweepPlan) -> Vec<&SweepStrut> {
    plan.struts
        .iter()
        .filter(|s| s.kind == StrutKind::Arm && !s.inserted_by_load)
        .collect()
}

/// Pairs of arms whose seats stand closer than twice the smallest tip the
/// registry allows. No ball is smaller than that tip (the ball factor is above
/// one), so a pair closer than twice it has overlapping balls at any size.
fn overlapping_pairs(plan: &SweepPlan, policy: &SweepPolicy) -> usize {
    let limit_mm = 2.0 * policy.tip_radius_min_mm;
    let h = plan.layer_height_mm;
    let arms = arms(plan);
    let mut pairs = 0;
    for (i, a) in arms.iter().enumerate() {
        for b in &arms[i + 1..] {
            #[expect(clippy::cast_precision_loss, reason = "layer indices are exact in f64")]
            let dz = (a.k_top as f64 - b.k_top as f64) * h;
            let dx = a.center_mm[0] - b.center_mm[0];
            let dy = a.center_mm[1] - b.center_mm[1];
            if (dx * dx + dy * dy + dz * dz).sqrt() < limit_mm {
                pairs += 1;
            }
        }
    }
    pairs
}

/// The same spike with the rule on and with it off, and the policy they ran at.
fn runs(mesh: &Mesh) -> (SweepPlan, SweepPlan, SweepPolicy) {
    let policy = SweepPolicy::from_registry()
        .expect("the registry")
        .with_working_pitch_mm(PITCH_MM);
    let on = harness::sweep_with(mesh, PITCH_MM, &policy);
    let off = harness::sweep_with(mesh, PITCH_MM, &policy.without_twin_merge());
    (on, off, policy)
}

#[test]
fn a_spike_gets_one_arm_not_two() {
    // Straight spikes: the rule leaves no overlapping pair at all.
    for (w, h) in [(0.8, 6.0), (0.5, 4.0)] {
        let (on, off, policy) = runs(&spike(w, h, 0.0));
        assert!(
            on.report.counters.twin_seats_merged > 0,
            "spike {w} x {h}: the rule must fire on a spike, or this test grades nothing"
        );
        assert_eq!(off.report.counters.twin_seats_merged, 0, "the rule is off in the control");
        let (pairs_on, pairs_off) = (overlapping_pairs(&on, &policy), overlapping_pairs(&off, &policy));
        assert!(pairs_off > 0, "spike {w} x {h}: the control must hold twin seats ({pairs_off})");
        assert_eq!(pairs_on, 0, "spike {w} x {h}: {pairs_on} overlapping pairs survive the rule");
        assert!(
            arms(&on).len() < arms(&off).len(),
            "spike {w} x {h}: {} arms against {} without the rule",
            arms(&on).len(),
            arms(&off).len()
        );
    }
    // A leaned spike: the seat walks across cells, so only the same-column
    // twins merge; the rule must still remove some and add none.
    let (on, off, policy) = runs(&spike(0.4, 6.0, 2.0));
    assert!(on.report.counters.twin_seats_merged > 0);
    assert!(overlapping_pairs(&on, &policy) < overlapping_pairs(&off, &policy));
}

/// The arm a twin is merged into is marked (`twin_host`), which is what keeps
/// the passes that re-route arms (congestion lean, lip-seat, graze, the
/// slenderness merges) from leaning it once its twin is gone; on a straight
/// spike it stays straight to the plate. The control (rule off) has no host.
#[test]
fn a_merged_host_is_marked_and_stays_straight() {
    for (w, h) in [(0.8, 6.0), (0.5, 4.0)] {
        let (on, off, _) = runs(&spike(w, h, 0.0));
        let hosts: Vec<&SweepStrut> = arms(&on).into_iter().filter(|arm| arm.twin_host).collect();
        assert!(!hosts.is_empty(), "spike {w} x {h}: no arm was marked, so this grades nothing");
        assert!(
            u64::try_from(hosts.len()).expect("small") <= on.report.counters.twin_seats_merged,
            "spike {w} x {h}: more hosts than merges"
        );
        assert!(
            hosts.iter().all(|arm| !arm.is_leaned()),
            "spike {w} x {h}: a merged host was leaned away from its column"
        );
        assert!(arms(&off).iter().all(|arm| !arm.twin_host), "spike {w} x {h}: the control marked a host");
    }
}

#[test]
fn a_merged_spike_keeps_all_its_load() {
    for (w, h, lean) in [(0.8, 6.0, 0.0), (0.5, 4.0, 0.0), (1.0, 3.0, 1.5), (0.4, 6.0, 2.0)] {
        let (on, off, _) = runs(&spike(w, h, lean));
        assert!(on.report.counters.twin_seats_merged > 0, "spike {w} x {h} x {lean}: the rule must fire");
        assert!(
            on.report.uncarried_load_mm2.abs() < 1e-9,
            "spike {w} x {h} x {lean}: {} mm2 of load reached no carrier",
            on.report.uncarried_load_mm2
        );
        assert!(!on.report.sweeps_disagree, "spike {w} x {h} x {lean}: the two sweeps disagree");
        // The run's own conservation test (the down-sweep asserts it layer by
        // layer) leaves nothing behind, and the plate still carries a load of
        // the same size as without the rule. (The totals differ by a few
        // cells: each seat injects its own cell's event, 0.0025 mm2, and a
        // merged seat injects none.)
        assert!(on.report.conservation_residual.abs() < 1e-9);
        let (reaction_on, reaction_off) = (on.report.plate_reaction_mm2, off.report.plate_reaction_mm2);
        let cell_mm2 = PITCH_MM * PITCH_MM;
        let merged = f64::from(u32::try_from(on.report.counters.twin_seats_merged).expect("small"));
        assert!(
            (reaction_on - reaction_off).abs() <= 2.0 * merged * cell_mm2,
            "spike {w} x {h} x {lean}: the plate carries {reaction_on} mm2 against {reaction_off} without the rule, \
             {merged} seats merged"
        );
    }
}
