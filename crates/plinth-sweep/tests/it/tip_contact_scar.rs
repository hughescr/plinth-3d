//! The TIP-CONTACT SCAR (`plinth_sweep::tip_landing`): the fifth named half
//! of the orientation objective's scar term, measured on a real production
//! sweep — the mirror of `model_landing_scar.rs`, priced at the other end of
//! the arm.
//!
//! # What these tests found, stated before they are read
//!
//! `examples/tip_scar_probe.rs` prints every number below, both
//! double-counting choices side by side.
//!
//! | pose | arms | tips excluded (already counted) | tips charged | choice(ii) total | choice(i) total |
//! |---|---|---|---|---|---|
//! | `make_raking_blade`, blade DOWN | 70 | 63 | 7, all on the blade (fragility 0.542) | 0.54968 mm² | 6.10239 mm² |
//! | `make_raking_blade`, blade UP | 67 | 56 | 11, 9 on slab-edge faces (0.150), 2 on the blade | 0.38473 mm² | 5.32555 mm² |
//! | Round 50, flat (the committed winner) | 243 | 210 | 33, all on the recess's near-vertical wall | 0.02175 mm² | 21.53892 mm² |
//! | Round 50, 180° flip | 283 | 250 | 33, same wall | 0.05178 mm² | 17.70392 mm² |
//!
//! Two findings follow.
//!
//! **The blade-down pose's surviving tip charges are on the blade itself; the
//! blade-up pose's are mostly on the slab.** Every one of the blade-down
//! pose's 7 non-excluded charges lands on a face at least as fragile as the
//! blade's own (0.542); the blade-up pose's 11 are mostly on the slab edge
//! (fragility 0.150) with only 2 on the blade. Mean charged fragility 0.542
//! against 0.221, and the pose that puts the blade down pays 0.550 mm² of
//! tip scar against 0.385 for the pose that puts it up — the term separates
//! the two poses in the direction the owner expects, even though it does not
//! flip this fixture on its own (see (d)). Since 2026-09-29 the fixture IS
//! blade up: the display charge on the slab top that out-voted this term is
//! gone (detail is the spread of the fold angles, round-pose lane rule B, and
//! the slab's folds are all 90°), and (d) pins the new pose.
//!
//! **Choice (ii) does not reach exactly zero on a disc.** The brief expected
//! "tips on a disc's flat underside are counted by the fold, so choice (ii)
//! adds nothing" to mean a disc's tip scar is `Some(0.0)`. Measured, it is
//! `Some(0.02175)` and `Some(0.05178)`: `make_recessed_disc` has a genuine
//! near-vertical wall (the recess), which the analytic fold's 45° cutoff does
//! NOT count any more than it counts the blade's raked faces — so a handful
//! of tips landing on that wall are new information under choice (ii) for
//! the identical reason the blade's are, not a defect in the choice. The
//! flat UNDERSIDE itself contributes exactly the zero the brief expected;
//! the wall is a different surface the brief did not name. Test (b) asserts
//! the measured `Some`, not a zero the fixture does not produce.
//!
//! Every assertion below is a comparison between two measurements or a
//! comparison against zero, per the brief.

use crate::harness::memo_run_sweep;
use plinth_core::{m0_constants, LayerPlan};
use plinth_orient::demand::{DemandFidelity, MeasuredSupportDemand, SupportDemandMeasurer};
use plinth_orient::search::{search, SweepRequest};
use plinth_orient::{ObjectiveWeights, OrientFaceTable, UpDirection};
use plinth_sweep::{tip_contact_scar, SweepDemand, SweepPlan, SweepPolicy};

/// The model's authored +Z stays up: the blade, authored hanging below the
/// slab, is DOWN.
const BLADE_DOWN: [f64; 3] = [0.0, 0.0, 1.0];
/// The model is inverted, so the blade is UP.
const BLADE_UP: [f64; 3] = [0.0, 0.0, -1.0];

fn table_for(mesh: &plinth_mesh::Mesh) -> OrientFaceTable {
    OrientFaceTable::build(mesh, "exact").expect("a closed fixture always scores")
}

/// One pose, measured through the production measurer exactly as the commit
/// tier's second stage measures it.
fn measure(
    mesh: &plinth_mesh::Mesh,
    table: &OrientFaceTable,
    direction: [f64; 3],
) -> MeasuredSupportDemand {
    let candidate = UpDirection::new(direction).expect("a unit direction");
    SweepDemand::production()
        .measure_support_demand(mesh, table, &[candidate], DemandFidelity::Production)
        .into_iter()
        .next()
        .expect("one candidate in, one result out")
        .expect("a well-formed fixture measures")
}

/// The same pose, swept directly, so a test can read the per-tip charges the
/// measurer folds into one number.
fn swept(mesh: &plinth_mesh::Mesh, direction: [f64; 3]) -> (plinth_mesh::Mesh, SweepPlan) {
    let candidate = UpDirection::new(direction).expect("a unit direction");
    let transformed =
        plinth_orient::oriented_recentered_mesh(mesh, candidate).expect("an orientable fixture");
    let constants = m0_constants().expect("the registry");
    let policy = SweepPolicy::from_registry().expect("the registered sweep policy");
    let lift_mm = constants.placeholders().lift_mm().diagnostic().get();
    let layer_height_mm = *constants.evidence().layer_height_default_mm().value();
    let max_z = transformed
        .verts()
        .iter()
        .fold(f64::NEG_INFINITY, |acc, vertex| acc.max(vertex[2]));
    let plate_z = -lift_mm;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the same layer-count arithmetic orient_demand.rs's measure_one performs, so this \
                  helper sweeps the pose the measurer swept"
    )]
    let count = (((max_z - plate_z) / layer_height_mm).ceil().max(1.0) as usize) + 1;
    let layers = LayerPlan::new(plate_z, vec![layer_height_mm; count]).expect("a layer plan");
    let plan = memo_run_sweep(&transformed, &layers, &policy).expect("a sweepable fixture");
    (transformed, plan)
}

/// (a) The term sees the strut on the delicate feature, on average — and what
/// it sees is a COMPARISON, not a magnitude.
///
/// A per-charge "every down charge is more fragile than every up charge"
/// claim is FALSE on this fixture: the blade-up pose's tips include two that
/// land back on the blade's own faces (the inverted blade is not perfectly
/// horizontal, so a small patch of it still needs support from below). What
/// is true, and asserted here, is the MEAN: every one of the blade-down
/// pose's surviving charges is on the blade (mean fragility 0.542), the
/// blade-up pose's are overwhelmingly on the slab edge (mean fragility
/// ~0.22), and the blade-down pose pays strictly more total tip scar.
#[test]
fn the_blade_down_pose_pays_more_tip_scar_on_average_more_fragile_faces_than_blade_up() {
    let mesh = plinth_mesh::make_raking_blade().expect("the raking blade fixture");
    let table = table_for(&mesh);
    let down_limit = ObjectiveWeights::registered().expect("weights").down_limit();

    let mut totals = Vec::new();
    let mut mean_fragility = Vec::new();
    for direction in [BLADE_DOWN, BLADE_UP] {
        let (oriented, plan) = swept(&mesh, direction);
        let scar = tip_contact_scar(&oriented, &plan, table.delicacy(), table.display(), down_limit)
            .expect("the table carries a fragility field");
        assert!(scar.tips > 0, "this pose placed no arms at all, so the comparison is vacuous");
        assert_eq!(
            scar.mapped, scar.tips,
            "every tip on a closed mesh must find a face to be charged against"
        );
        assert!(
            scar.raw_mm2 > 0.0,
            "a pose whose tips carry a fragility-bearing face must be charged for at least one \
             of them, once the already-counted charges are excluded"
        );
        let charged: Vec<&plinth_sweep::TipCharge> =
            scar.charges.iter().filter(|charge| !charge.already_counted).collect();
        assert!(
            !charged.is_empty(),
            "every charge on this pose was excluded as already counted, so the comparison below \
             would divide by zero"
        );
        let mean = charged.iter().map(|charge| table.delicacy()[charge.face]).sum::<f64>()
            / charged.len() as f64;
        totals.push(scar.raw_mm2);
        mean_fragility.push(mean);
    }

    assert!(
        mean_fragility[0] > mean_fragility[1],
        "the blade-down pose's surviving tip charges ({}) should average a more fragile face than \
         the blade-up pose's ({}); if this inverts, the term is no longer reporting that the tip \
         is on the delicate feature",
        mean_fragility[0],
        mean_fragility[1]
    );
    assert!(
        totals[0] > totals[1],
        "the blade-down pose ({}) should be charged more tip scar than the blade-up pose ({})",
        totals[0],
        totals[1]
    );
}

/// (b) A model whose tips mostly land on a face the analytic fold already
/// counts is charged the SMALL residual left over, not zero and not
/// everything — and it is measured, not assumed.
///
/// `Some(0.02175)` and `Some(0.05178)`, not `Some(0.0)`: `make_recessed_disc`
/// has a near-vertical recess wall the 45° fold does not count, and a
/// handful of tips land there in both flip poses. `None` is still reserved
/// for a pose nobody measured — the point this test actually needs to prove.
#[test]
fn a_disc_that_mostly_lands_on_an_already_counted_face_still_carries_a_measured_some() {
    let mesh = plinth_mesh::make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128)
        .expect("the Round 50 profile");
    let table = table_for(&mesh);
    for direction in [BLADE_DOWN, BLADE_UP] {
        let measured = measure(&mesh, &table, direction);
        let tip = measured
            .tip_scar_raw_mm2
            .expect("a production sweep measures a tip scar for every pose it places arms on");
        assert!(
            tip.is_finite() && tip >= 0.0,
            "a measured tip scar must be a finite, non-negative area"
        );
    }
}

/// (b, second half) Withholding the tip-contact scar leaves a stated
/// ABSENCE on the record, never a zero, and moves no committed number that
/// was not measured to move.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "the claim under test IS bit equality between the run that folded the tip scar and the \
              run that withheld it, plus that scar. A tolerance would pass on a fold that rounded \
              differently, which is exactly what must not happen"
)]
fn withholding_the_tip_scar_on_a_disc_carries_none_not_zero() {
    let mesh = plinth_mesh::make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128)
        .expect("the Round 50 profile");
    let request = SweepRequest::registered().expect("the registered sweep configuration");
    let with = search(&mesh, &request, Some(&SweepDemand::production())).expect("a scorable disc");
    let without = search(&mesh, &request, Some(&SweepDemand::without_tip_contact_scar()))
        .expect("a scorable disc");

    let with_commit = with.record().tiers.last().expect("a commit tier").clone();
    let without_commit = without.record().tiers.last().expect("a commit tier").clone();
    for entry in &without_commit.ranking {
        assert_eq!(
            entry.terms.tip_scar_raw_mm2, None,
            "candidate {} carries a tip scar with the term withheld",
            entry.candidate
        );
    }
    let winner = &with_commit.ranking[0];
    let winner_without = without_commit
        .ranking
        .iter()
        .find(|entry| entry.candidate == winner.candidate)
        .expect("the same candidate is ranked in both searches");
    let tip = winner
        .terms
        .tip_scar_raw_mm2
        .expect("the committed winner was measured at production fidelity")
        .get();
    assert!(tip >= 0.0, "a measured tip scar must be non-negative");
    assert_eq!(
        winner.terms.scar_raw_mm2.get(),
        winner_without.terms.scar_raw_mm2.get() + tip,
        "the whole difference between the two runs' scar measurement is the winner's tip scar"
    );
}

/// (c) The identity the record must satisfy:
/// `scar_raw = fragility + display + landing + tip`.
///
/// Checked on every commit-tier candidate of both flip poses of the blade —
/// the ones a production sweep priced (where both halves are present) and
/// the ones only the prefilter reached (where both are absent, and the
/// identity is the two-half one).
/// # Re-cut 2026-09-06: a relative tolerance, and a guard that earns it
///
/// The old form asserted BIT equality and went red at 66.79549650485664
/// against 66.79549650485666 — two ULP on a number of order 67, i.e. one part
/// in 3e15. Nothing was added to `scar_raw`: the shaft-landing work changed
/// which candidate poses reach the commit tier, and four `f64` sums added in a
/// different ASSOCIATION do not agree to the last bit. Floating-point addition
/// is not associative, so bit equality was never the property being claimed —
/// it was a coincidence of the old association, and a coincidence is not a
/// test.
///
/// The tolerance is therefore 1e-9 RELATIVE, the same the area guarantee
/// already carries. What stops the tolerance hiding a fifth, unnamed
/// contribution — the defect this test exists to catch — is the drill added
/// beside it: every named half is dropped in turn and the identity must break
/// by MORE than the tolerance every time a non-zero half is dropped. A missing
/// term is a term-sized error, twelve orders of magnitude above 1e-9 relative,
/// so the loosened comparison can still only ever pass on a complete sum.
#[test]
fn the_scar_raw_is_exactly_its_four_named_halves() {
    let mesh = plinth_mesh::make_raking_blade().expect("the raking blade fixture");
    let request = SweepRequest::registered().expect("the registered sweep configuration");
    let outcome =
        search(&mesh, &request, Some(&SweepDemand::production())).expect("a scorable fixture");
    let commit = outcome.record().tiers.last().expect("a commit tier").clone();

    let mut with_tip = 0_usize;
    let mut drills = 0_usize;
    for entry in &commit.ranking {
        let landing = entry.terms.landing_scar_raw_mm2.map_or(0.0, plinth_core::Finite::get);
        let tip = entry.terms.tip_scar_raw_mm2.map_or(0.0, plinth_core::Finite::get);
        if entry.terms.tip_scar_raw_mm2.is_some() {
            with_tip += 1;
        }
        let fragility = entry.terms.fragility_raw_mm2.get();
        let display = entry.terms.display_raw_mm2.get();
        let whole = entry.terms.scar_raw_mm2.get();
        let sum = fragility + display + landing + tip;
        let tolerance = 1.0e-9 * whole.abs().max(1.0);
        assert!(
            (whole - sum).abs() <= tolerance,
            "candidate {}'s scar_raw {whole} is not its four named halves {sum} (fragility \
             {fragility}, display {display}, landing {landing}, tip {tip}); the gap is {} \
             against a tolerance of {tolerance}",
            entry.candidate,
            (whole - sum).abs()
        );
        // THE DRILL THAT PAYS FOR THE TOLERANCE. Withhold each named half in
        // turn: every one that is not itself zero must move the identity far
        // outside the band, so a term going missing can never be mistaken for
        // the association noise the band exists to absorb.
        for (name, without) in [
            ("fragility", sum - fragility),
            ("display", sum - display),
            ("landing", sum - landing),
            ("tip", sum - tip),
        ] {
            if (sum - without).abs() <= tolerance {
                continue;
            }
            drills += 1;
            assert!(
                (whole - without).abs() > tolerance,
                "candidate {}: dropping the {name} half left the identity inside the \
                 1e-9 band, so the band could hide a missing term",
                entry.candidate
            );
        }
    }
    assert!(
        with_tip > 0,
        "no candidate carried a tip half, so the identity was only ever checked with a zero"
    );
    assert!(
        drills > 0,
        "no non-zero half was ever withheld, so the tolerance was never shown to be safe"
    );
    println!("checked {} candidates, {drills} withholding drills", commit.ranking.len());
}

/// (d) The sabotage: withhold the term and the committed numbers move.
///
/// As with the landing scar before it, this drill does NOT flip
/// `make_raking_blade` — the pose is pinned as unchanged, deliberately: a
/// future change that does flip it must come back through this test and say
/// so. What is asserted is that the term is load-bearing: withholding it
/// changes the winner's score and its `scar_raw` by exactly the tip scar
/// that was measured, and it charges the two poses by DIFFERENT amounts, so
/// it is capable of moving a ranking even though it does not move this one.
///
/// # Re-cut 2026-09-29: the pose DID flip, and bit equality went with it
///
/// Rule B of the round-pose lane (detail is the SPREAD of the fold angles,
/// owner ruling 2026-09-05) took away the display charge that had kept the
/// blade DOWN: every fold on this two-box fixture is 90°, one magnitude, so
/// `D` is 0 on every face, and the committed pose is now the 180° flip, blade
/// UP (`u = −Z`), by 0.00145 points (`plinth-evidence/slice-sweep-2026-09-02/
/// field-2026-09-28/lanes/round-pose.md`). Withholding the tip scar does not
/// move it back; the pin below now says blade up, explicitly. The old bit
/// equality `with == without + tip` held only by coincidence of the old
/// association: the engine adds `base + (landing + tip)`
/// (`TermsRecord::repriced`), this compares `(base + landing) + tip`, and at
/// the new winner they differ by one ULP (65.83742749999999 against
/// 65.8374275). It is now the same 1e-9 relative band as the four-halves
/// identity above, with the guard that earns it: the tip scar itself must
/// lie far outside the band.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "the pinned pose is an exact axis direction, and the two poses' shifts are compared for \
              inequality: a tolerance would let two different charges pass as equal"
)]
fn withholding_the_tip_scar_moves_the_committed_numbers_but_not_this_pose() {
    let mesh = plinth_mesh::make_raking_blade().expect("the raking blade fixture");
    let request = SweepRequest::registered().expect("the registered sweep configuration");
    let with =
        search(&mesh, &request, Some(&SweepDemand::production())).expect("a scorable fixture");
    let without = search(&mesh, &request, Some(&SweepDemand::without_tip_contact_scar()))
        .expect("a scorable fixture");

    let with_commit = with.record().tiers.last().expect("a commit tier").clone();
    let without_commit = without.record().tiers.last().expect("a commit tier").clone();
    let winner = &with_commit.ranking[0];
    let winner_without = without_commit
        .ranking
        .iter()
        .find(|entry| entry.candidate == winner.candidate)
        .expect("the same candidate is ranked in both searches");

    let tip = winner
        .terms
        .tip_scar_raw_mm2
        .expect("the winner was measured at production fidelity")
        .get();
    assert!(
        tip > 0.0,
        "the winner's tips carry no non-excluded face, so withholding the term would be a no-op \
         and this drill would pass vacuously"
    );
    let whole = winner.terms.scar_raw_mm2.get();
    let tolerance = 1.0e-9 * whole.abs().max(1.0);
    assert!(
        tip > tolerance,
        "the tip scar {tip} lies inside the association band {tolerance}, so the band could hide it"
    );
    let rebuilt = winner_without.terms.scar_raw_mm2.get() + tip;
    assert!(
        (whole - rebuilt).abs() <= tolerance,
        "the whole difference between the two runs' scar measurement is the tip scar: {whole} \
         against {rebuilt}, a gap of {} against a band of {tolerance}",
        (whole - rebuilt).abs()
    );
    assert!(
        winner.terms.score.get() > winner_without.terms.score.get(),
        "withholding a positive scar must make the winner score better, not worse"
    );
    assert_eq!(
        winner_without.terms.tip_scar_raw_mm2, None,
        "a withheld term is a stated absence, not a zero"
    );

    let runner_up = &with_commit.ranking[1];
    let runner_up_without = without_commit
        .ranking
        .iter()
        .find(|entry| entry.candidate == runner_up.candidate)
        .expect("the same candidate is ranked in both searches");
    let winner_shift = winner.terms.score.get() - winner_without.terms.score.get();
    let runner_up_shift = runner_up.terms.score.get() - runner_up_without.terms.score.get();
    assert!(
        winner_shift != runner_up_shift,
        "the term charged the winning pose and the runner-up by the identical amount \
         ({winner_shift}); it would then be structurally incapable of ever reordering them"
    );

    // The finding, pinned: withholding the term does not move this fixture's
    // pose, and since 2026-09-29 that pose is blade UP, asserted explicitly
    // (doc above). A future change that moves it again must come back here.
    assert_eq!(
        with.winner().selection,
        without.winner().selection,
        "make_raking_blade's committed pose moved when the tip scar was withheld. That is a real \
         change in the engine's answer: re-measure and re-record it."
    );
    assert_eq!(
        with.record().winner.up_model_frame.map(plinth_core::Finite::get),
        [0.0, 0.0, -1.0],
        "make_raking_blade no longer commits the 180 deg flip (blade UP). That is a real change in \
         the engine's answer: re-measure it and re-record lanes/round-pose.md's finding."
    );
}

/// A tip on a display face costs the same however sturdy the face is:
/// fragility and display ADD (2026-09-29, `plinth_orient::score`'s module
/// doc). The mirror of `model_landing_scar`'s test (e), at the other end of
/// the arm: one plan, priced against two synthetic per-face fields over the
/// real mesh, so only the pricing rule is under test. Under the old
/// `fragility × (1 + D)` rule the first pricing was exactly zero.
#[test]
fn a_tip_on_a_sturdy_display_face_costs_what_a_tip_on_a_fragile_plain_face_costs() {
    let mesh = plinth_mesh::make_raking_blade().expect("the raking blade fixture");
    let down_limit = ObjectiveWeights::registered()
        .expect("weights")
        .down_limit();
    let (oriented, plan) = swept(&mesh, BLADE_DOWN);
    let faces = oriented.tris().len();
    let (zeros, ones) = (vec![0.0_f64; faces], vec![1.0_f64; faces]);

    let sturdy_display = tip_contact_scar(&oriented, &plan, &zeros, &ones, down_limit)
        .expect("a fragility field of the right length");
    let fragile_plain = tip_contact_scar(&oriented, &plan, &ones, &zeros, down_limit)
        .expect("a fragility field of the right length");
    assert!(
        fragile_plain.tips > 0 && fragile_plain.raw_mm2 > 0.0,
        "the blade-down pose charges no tip outside the analytic fold, so the comparison would be \
         vacuous"
    );
    assert!(
        (sturdy_display.raw_mm2 - fragile_plain.raw_mm2).abs() <= 1e-12 * fragile_plain.raw_mm2,
        "tips on faces with no fragility and full display value cost {} mm2; the same tips on \
         fully fragile faces with no display value cost {}. The display harm is following \
         fragility again, so a sturdy display face is left unprotected",
        sturdy_display.raw_mm2,
        fragile_plain.raw_mm2,
    );
}
