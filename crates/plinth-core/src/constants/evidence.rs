use super::{ConstantDescriptor, RegistryError};
use crate::{
    BoundKind, BuildVolumeError, BuildVolumeMm, CalendarDate, ConstantId, Derived, EvidenceDate,
    EvidenceId, EvidenceSource, FiniteInterval, LoadSharing, LoadSharingError, Measured,
    MicronGranularity, PlateId, ProvenanceError, YDirection,
};
use thiserror::Error;

const SCALAR: &str = "scalar";

/// Construction failures for the all-at-once M0 evidence declaration set.
#[derive(Clone, Debug, PartialEq, Error)]
pub enum M0EvidenceConstantsError {
    #[error(transparent)]
    Provenance(#[from] ProvenanceError),
    #[error(transparent)]
    LoadSharing(#[from] LoadSharingError),
    #[error(transparent)]
    BuildVolume(#[from] BuildVolumeError),
    #[error("the evidence declaration for LOAD_SHARING must be additive")]
    NonAdditiveLoadSharing,
}

/// The closed M0 set whose values have measured or derived authority.
///
/// This is a passive component, not a global registry. Its private wrapper
/// fields retain provenance at the declaration boundary, while
/// [`Self::descriptors`] provides one immutable batch for a registry owner.
#[derive(Clone, Debug, PartialEq)]
pub struct M0EvidenceConstants {
    cco_x_pair_load_discount: Derived<f64>,
    condition_shift_max: Measured<f64>,
    decimation_max_absolute_deviation_mm: Derived<f64>,
    decimation_max_p999_deviation_mm: Derived<f64>,
    decimation_max_p99_deviation_mm: Derived<f64>,
    final_clearance_gap_mm: Measured<f64>,
    l_first_loss_mm: Measured<f64>,
    l_span_allowed_mm: Derived<f64>,
    l_span_crit_mm: Derived<f64>,
    l_survived_max_mm: Measured<f64>,
    lamina_reach_ratio: Derived<f64>,
    lamina_reach_ratio_audit_gate: Derived<f64>,
    lamina_reach_ratio_first_loss: Derived<f64>,
    layer_height_default_mm: Measured<f64>,
    load_sharing: Measured<LoadSharing>,
    minima_drop_interior_faces: Measured<bool>,
    minima_require_island_birth: Measured<bool>,
    n_eff_cap: Derived<f64>,
    orient_peel_area_exponent_high: Measured<f64>,
    orient_peel_area_exponent_low: Measured<f64>,
    orient_peel_reference_area_mm2: Measured<f64>,
    orient_peel_reference_pressure_pa_high: Measured<f64>,
    orient_peel_reference_pressure_pa_low: Measured<f64>,
    p8_chord_canon_micron: Measured<MicronGranularity>,
    p8_pocket_voxel_mm: Measured<f64>,
    pixel_pitch_x_um: Measured<f64>,
    pixel_pitch_y_um: Measured<f64>,
    print_floor_chord_mm: Measured<f64>,
    saturn_build_volume_mm: Measured<BuildVolumeMm>,
    saturn_leading_release_edge: Measured<YDirection>,
    saturn_release_film_thickness_mm: Measured<f64>,
    saturn_release_front_sweep_direction: Measured<YDirection>,
    support_gate_max_work_per_batch_unit: Derived<u64>,
    support_gate_work_per_batch_unit_observed: Measured<u64>,
    support_knn_max_work_per_face: Derived<u64>,
    support_knn_work_per_face_observed: Measured<u64>,
    support_load_max_work_per_batch_unit: Derived<u64>,
    support_load_work_per_batch_unit_observed: Measured<u64>,
    support_parity_grid_max_bytes_per_face: Derived<u64>,
    support_parity_grid_refs_per_face_observed: Measured<f64>,
    tip_diameter_min_allowed_mm: Measured<f64>,
    tip_radius_max_grown_scar_mm: Measured<f64>,
    tip_radius_min_allowed_mm: Derived<f64>,
    x_allowed: Derived<f64>,
    x_star_cone: Measured<FiniteInterval>,
    x_star_cylinder: Measured<FiniteInterval>,
}

impl M0EvidenceConstants {
    /// Constructs the complete evidence-grade declaration set or publishes none of it.
    pub fn new() -> Result<Self, M0EvidenceConstantsError> {
        let plate_6_date = exact_date("2026-07-31")?;
        let plate_6_source = EvidenceSource::calibration_plate(
            PlateId::from("COUPON-PLATE-6-support-arm-capacity"),
            "docs/engineering-journal/reference/plate-06-results.md",
            "§5 Finding 1",
            plate_6_date.clone(),
        )?;
        let release_front_source = EvidenceSource::project_record(
            EvidenceId::new("saturn4-ultra-release-front-plate6")?,
            "docs/engineering-journal/reference/plate-06-results.md",
            "§1 Printer",
            plate_6_date,
        )?;
        let vendor_date = exact_date("2025-01-22")?;
        let vendor_source = EvidenceSource::vendor_document(
            EvidenceId::new("elegoo-saturn4-ultra-16k-press-2025-01-22")?,
            "docs/datasheets/printers/saturn-4-ultra-16k.md",
            "Physical parameters plus project axis mapping",
            vendor_date,
        )?;
        let minima_source = EvidenceSource::project_record(
            EvidenceId::new("minima-contact-fix-2026-07-24")?,
            "docs/engineering-journal/reference/acceptance-run-2026-07-24-contactfix.md",
            "Provenance and What moved",
            exact_date("2026-07-24")?,
        )?;

        let load_sharing_value = LoadSharing::additive(6.0)?;
        let load_sharing = Measured::new(
            load_sharing_value,
            plate_6_source,
            None,
            "Plate 6 validated one cap area (30 mm²), one arm diameter (0.798 mm), and one arm length (4.00 mm). No strip failed, so there is no measured upper bracket; N > 6 is extrapolation. The fin root contributes about 19.75% section bonus, so the result is at least N× and never evidence for super-additive sharing. X_tot is algebraically degenerate with total arm area, and the ratio rests on plate 5. The tight N=2 flip, X_tot 30.077 alive versus 30.481 dead, is a 1.33% window and does not prove exact additivity. Plate 6 §5 Finding 3 records survival on both axes at X_arm=60.15, excluding zipper collapse at that load while leaving zero-variance and near-boundary axis effects untested; the four X-axis cells were not zipper tests. docs/engineering-journal/reference/plate-06-results.md §6 records the B6 override: re-scoping a preregistered void after seeing data is exactly what preregistration blocks, making these points post-hoc and leaving that counterargument unanswered.",
        )?;
        let n_eff_cap_value = load_sharing_value
            .n_eff_cap()
            .ok_or(M0EvidenceConstantsError::NonAdditiveLoadSharing)?;

        let tip_diameter_min_allowed_mm = Measured::new(
            0.100,
            EvidenceSource::owner_decision(
                EvidenceId::new("tip-diameter-floor-owner-decision")?,
                "docs/engineering-journal/03-physics-and-constants.md",
                "§3.2",
                exact_date("2026-08-19")?,
            )?,
            None,
            "Owner-authorized hard validator MINIMUM for this machine and resin; it is not a target or a cap. 0.100 mm DIAMETER (0.050 mm RADIUS) since the owner ruling of 2026-08-19, SUPERSEDING the earlier 0.150 mm diameter (0.075 mm radius) decision of on-or-before 2026-07-31. The supersession is field data beating a desk number: the owner prints 0.1 mm DIAMETER cylindrical tips in ChituBox today, on tiny islands in narrow gaps, so a validator that refuses 0.1 mm diameter would refuse geometry the machine and resin demonstrably render. The earlier value was an owner decision with no coupon behind it and no production reader, which is why it could sit un-noticed BELOW nothing and ABOVE the live sizing clamp. UNITS TRAP: this constant is a DIAMETER and its derived twin TIP_RADIUS_MIN_ALLOWED_MM is a RADIUS; a number transcribed between them without halving or doubling lands 2x wrong. This value now AGREES with the live sizing clamp TIP_RADIUS_DEFAULT_MM = 0.050 mm radius (0.100 mm diameter), closing the conflict that ruling flagged rather than leaving two records disagreeing. Thin-tip formability below 0.100 mm diameter remains uncalibrated, and this minimum is what refuses it.",
        )?;
        let tip_radius_value = *tip_diameter_min_allowed_mm.value() / 2.0;

        let x_star_cone = Measured::new(
            FiniteInterval::new(25.00, 25.34, BoundKind::Inclusive, BoundKind::Inclusive)?,
            EvidenceSource::calibration_plate(
                PlateId::from("COUPON-PLATE-5-compliant-vs-stiff"),
                "docs/engineering-journal/reference/plate-05-results.md",
                "§1 and §5",
                on_or_before_date("2026-07-30")?,
            )?,
            None,
            "Plate 5 conical-arm bracket with one boundary inversion at 25.00; retain the interval rather than a midpoint.",
        )?;
        let x_allowed_value = (x_star_cone.value().lower() / 2.0).floor();

        // The one instrumented Weeping Treant run that measured what the
        // support pipeline's execution-authority caps actually have to admit.
        let treant_cap_source = EvidenceSource::corpus_study(
            EvidenceId::new("support-resource-caps-weeping-treant-2026-08-05")?,
            "docs/engineering-journal/support-resource-caps-2026-08-05.md",
            "§2 Measured requirement",
            exact_date("2026-08-05")?,
        )?;

        // The re-measurement of the same model after FU-8b removed the three
        // quadratics that made the 2026-08-05 gate and load figures charges
        // against a defect rather than statements of demand.
        let treant_batch_source = EvidenceSource::corpus_study(
            EvidenceId::new("support-gate-load-batch-caps-weeping-treant-2026-08-06")?,
            "docs/engineering-journal/support-resource-caps-2026-08-05.md",
            "§7 FU-8b post-fix measurement (2026-08-06)",
            exact_date("2026-08-06")?,
        )?;

        // Hoisted because the three decimation deviation bounds are derived
        // from these two measurements' own values, not from restated literals.
        let pixel_pitch_y_um = Measured::new(
            19.0,
            vendor_source.clone(),
            None,
            "The vendor's second pitch maps to project Y; retrieval date 2026-07-31 is metadata, while 2025-01-22 is the evidence date.",
        )?;
        let print_floor_chord_mm = Measured::new(
            0.08,
            EvidenceSource::owner_decision(
                EvidenceId::new("print-floor-owner-decision-2026-07-24")?,
                "docs/engineering-journal/specs/SPEC-EMBED-POLICY.md",
                "PRINT-FLOOR amendment and Normative constants",
                exact_date("2026-07-24")?,
            )?,
            None,
            "Owner-ratified print-floor chord below which a rule-4 unanchored transit is moot.",
        )?;
        // µm / 1000 is the µm→mm conversion, so this is exactly one addressable
        // step of the coarser printer axis, and half of it is the quantization
        // half-step the printer already introduces.
        let decimation_max_p999_deviation_value = *pixel_pitch_y_um.value() / 1000.0;
        let decimation_max_p99_deviation_value = decimation_max_p999_deviation_value / 2.0;
        let decimation_max_absolute_deviation_value = *print_floor_chord_mm.value();

        // Hoisted for the same reason the decimation bounds are: the lamina
        // reach ratios and the span allowance are DERIVED and must be computed
        // from their parents' own values at run time, never transcribed as a
        // literal (`docs/specs/audit-span-criterion.md`:225-229 states that
        // rule for exactly this family).
        let pixel_pitch_x_um = Measured::new(
            14.0,
            vendor_source.clone(),
            None,
            "The vendor's first pitch maps to project X; retrieval date 2026-07-31 is metadata, while 2025-01-22 is the evidence date.",
        )?;
        let layer_height_default_mm = Measured::new(
            0.050,
            EvidenceSource::project_record(
                EvidenceId::new("saturn4-ultra-layer-profile-plate6")?,
                "docs/engineering-journal/reference/plate-06-results.md",
                "§1 Layer height",
                exact_date("2026-07-30")?,
            )?,
            None,
            "Plate 6 project profile layer height; this is not a vendor default.",
        )?;
        let overhang_ladder_pool_source = EvidenceSource::calibration_plate(
            PlateId::from("COUPON-PLATE-1-overhang-tilt"),
            "docs/specs/audit-span-criterion.md",
            "§2.5 The bracket (pooling plates 1 and 4)",
            exact_date("2026-07-29")?,
        )?;
        let l_survived_max_mm = Measured::new(
            2.130_032_25,
            overhang_ladder_pool_source.clone(),
            None,
            "THE LARGEST FLAT-OVERHANG SPAN AT WHICH NO LOSS WAS RECORDED on either unsupported-overhang ladder plate, pooled on the shared length axis at 0.050 mm layer height, this printer, this resin (plate 1 rung 14 = plate 4 rung 6 = 0.7982 * 1.2169^5; plate 4's own cells at that rung are UNREAD). It is the conservative end of the measured loss bracket L_crit in [2.130032, 4.670956] mm, whose onset is bracketed to a single ladder rung [2.130032, 2.592036], width 1.2169x. The specimens were SINGLE-ROOT CANTILEVERS (audit-span-criterion.md §2.1: each finger cantilevered from a rigid spine, no support anywhere), so reading this length as a COVERING RADIUS -- no point of a down-facing region further than this from a carrier -- is the conservative reading, because a point between two carriers is stiffer than a free cantilever end. This is a DESIGN-TIME PLACEMENT input. It is deliberately NOT the audit's gate value L_SPAN_ALLOWED_MM, which divides this by CONDITION_SHIFT_MAX and subtracts two pixel diagonals because a gate exists to refuse a finished file; subtracting uncertainty is right for a gate and wrong for placing a seat (audit-span-criterion.md:196-229 rule 2). Two INDEPENDENT field data points sit inside the measured bracket and far outside the gate's: the owner's own 4.0 mm ChituBox spacing practice (reach 2.3094 mm) and his 47-tip Cylinder print on 706.86 mm2 (reach 2.4074 mm). SUPERSEDED BY any coupon at a second layer thickness or a second overhang angle.",
        )?;
        let l_first_loss_mm = Measured::new(
            2.592_036,
            EvidenceSource::calibration_plate(
                PlateId::from("COUPON-PLATE-4-overhang-extended"),
                "docs/specs/audit-span-criterion.md",
                "§2.5 first loss recorded at (plate 4, G1aU, 1 of 5)",
                exact_date("2026-07-29")?,
            )?,
            None,
            "The shortest flat-overhang span at which ANY loss was recorded: plate 4, specimen G1aU, 1 of 5. It is the upper end of the onset bracket whose lower end is L_SURVIVED_MAX_MM. REPORTING ONLY -- no placement or acceptance decision may read it or the ratio derived from it. It exists so a run report can state where the chosen placement value sits inside the measured bracket rather than asserting the choice is safe.",
        )?;
        let condition_shift_max = Measured::new(
            1.53,
            EvidenceSource::calibration_plate(
                PlateId::from("COUPON-PLATE-4-overhang-extended"),
                "docs/engineering-journal/reference/plate-04-results.md",
                "§3.3 length alone does not determine survival",
                exact_date("2026-07-29")?,
            )?,
            None,
            "The upper end of the only measured condition-shift magnitude on this printer, 1.31-1.53x, between the favourable condition (plate 1, the side that survived where the two plates overlap and disagree) and the unfavourable one. THE CAUSE IS NOT IDENTIFIED -- plate 4 §3.3 records the effect and §4 records that the inference built on it did not succeed; orientation, Y station and film age are aliased with one another there, so this is a magnitude without a mechanism. The larger end is registered because a larger divisor is the conservative choice for a gate. It is a GATE input only: it divides L_SURVIVED_MAX_MM to produce L_SPAN_CRIT_MM and through it the audit's L_SPAN_ALLOWED_MM. No placement decision reads it.",
        )?;
        let l_span_crit_value = *l_survived_max_mm.value() / *condition_shift_max.value();
        // One in-layer pixel diagonal, the audit's own instrument uncertainty
        // under a full-containment fill rule; computed from the two registered
        // pitches rather than transcribed (audit-span-criterion.md:213-218).
        let pixel_diagonal_mm = (*pixel_pitch_x_um.value() / 1000.0)
            .hypot(*pixel_pitch_y_um.value() / 1000.0);
        let l_span_allowed_value = l_span_crit_value - 2.0 * pixel_diagonal_mm;
        let lamina_reach_ratio_value = *l_survived_max_mm.value() / *layer_height_default_mm.value();
        let lamina_reach_ratio_first_loss_value =
            *l_first_loss_mm.value() / *layer_height_default_mm.value();
        let lamina_reach_ratio_audit_gate_value =
            l_span_allowed_value / *layer_height_default_mm.value();

        Ok(Self {
            cco_x_pair_load_discount: Derived::new(
                0.840_896_415_253_714_5,
                std::iter::empty::<&str>(),
                "Proof-only fourth-root relation: (F/2)^0.25/F^0.25 = 0.5^0.25. The proof is in docs/engineering-journal/reference/physics_transient_peel.md and the relation is implemented in both cco.py and bracing.py.",
            ),
            condition_shift_max,
            decimation_max_absolute_deviation_mm: Derived::new(
                decimation_max_absolute_deviation_value,
                ["PRINT_FLOOR_CHORD_MM"],
                "Worst-case (max) two-sided deviation a decimated mesh may carry and still be authorized as downstream-useful. Exact identity, not arithmetic: the bound IS the registered print-floor chord re-expressed as a deviation limit, on the rule that a deviation below the smallest chord the process can print cannot produce a printed difference. It exists as a separate declaration because it is a decimation authority with its own re-derivation trigger, and because the search's own p99/p99.9 budget is blind to the worst point: on the Weeping Treant v1.2 at 520 faces removed the measured max is 0.0566357 mm while p99.9 is 0.000073 mm and p99 is exactly 0, a factor of 776 (recorded in docs/engineering-journal/decimation-design-2026-08-06.md §8.3, a file retired without replacement by the 2026-08-25 docs consolidation (`fb97b62`); the measurement stands, its source citation does not resolve). Measured consequence on the one large specimen, recorded so this bound is never mistaken for a permissive one: it admits only (2.125 %, 4.453 %] reduction on that model, and the topology gate independently stops it at ≈0.2 % first, so no corpus model reaches it today. LAYER_HEIGHT_DEFAULT_MM (0.050 mm) was the runner-up and was NOT chosen: the Phase-1 max-vs-ratio curve does not discriminate between them (0.050 mm admits ≈0.12 %), so the choice rests on provenance alone, and a Z-axis layer quantum applied to an XY deviation is the weaker argument.",
            ),
            decimation_max_p999_deviation_mm: Derived::new(
                decimation_max_p999_deviation_value,
                ["PIXEL_PITCH_Y_UM"],
                "Typical-deviation ceiling at the 99.9th percentile for the decimation downstream-utility witness: PIXEL_PITCH_Y_UM / 1000 = 19 / 1000 = 0.019 mm, where the /1000 is the µm→mm conversion, so the bound is exactly one addressable step of the coarser printer axis. The printer addresses the XY plane on a 14 × 19 µm lattice, and a deviation at or below one lattice step on the coarser axis cannot move a printed surface by more than one addressable step. It is numerically identical to the limit the budget search already enforces (PIXEL_PITCH_Y_UM × DECIMATION_P999_FACTOR) and is nonetheless a separate declaration, because DECIMATION_P999_FACTOR is a non-AcceptSafe selected fraction and arithmetic from a non-AcceptSafe value cannot improve its authority. The two are pinned to agree bit-for-bit by decimation_witness_bounds_agree_with_the_search_policy_bounds in crates/plinth-decimate/src/lib.rs, so a move in either is loud.",
            ),
            decimation_max_p99_deviation_mm: Derived::new(
                decimation_max_p99_deviation_value,
                ["DECIMATION_MAX_P999_DEVIATION_MM"],
                "Typical-deviation ceiling at the 99th percentile for the decimation downstream-utility witness: DECIMATION_MAX_P999_DEVIATION_MM / 2 = 0.0095 mm, half an addressable step of the coarser printer axis, which is the quantization half-step the printer already introduces. Numerically identical to the search's PIXEL_PITCH_Y_UM × DECIMATION_P99_FACTOR and separately declared for the same reason its p99.9 sibling is: the search factor is a non-AcceptSafe selected fraction. Pinned bit-for-bit against that product by decimation_witness_bounds_agree_with_the_search_policy_bounds in crates/plinth-decimate/src/lib.rs.",
            ),
            final_clearance_gap_mm: Measured::new(
                0.300,
                EvidenceSource::owner_decision(
                    EvidenceId::new("final-clearance-demanded-gap-owner-field-data")?,
                    "docs/engineering-journal/03-physics-and-constants.md",
                    "§3.5",
                    exact_date("2026-08-22")?,
                )?,
                None,
                "EXTERNAL REQUIREMENT -- field data, NOT physics. Craig ruling 2026-08-22, field experience: 0.3mm gap features are preserved fine in prints. THE QUANTITY: a TRUE surface-to-surface gap in mm -- the shortest distance from the arm's own SOLID SURFACE to the model surface, i.e. the kernel's axis-to-surface distance MINUS the arm radius at the nearest station. It is not a centreline distance and not a radius; a number carried between the two lands one arm-radius wrong. WHAT IT DECIDES: the FINAL clearance pass, and only that pass -- the one whose verdict actually refuses an arm. Today that is brace::exact_clearance_rows and the grading of load-assigned radii, over BOTH contact-declaring members (struts, model rungs, CCO model anchors) and non-contact members (shoots, transients), which are graded full-length against the whole surface. WHAT IT IS NOT: the placement-family search margin. STANDOFF_MM (0.600 mm) grades CCO candidacy clearance_survival, rewire changed-set validation, and brace evaluate_candidates (owner ruling R-1495, 2026-08-22), where the demand PRUNES a search and refuses nothing final; a candidate it drops leaves the tournament, it does not condemn a contact. The two demands are deliberately different numbers with deliberately different jobs, and the ONE predicate reads whichever its stage registered -- see the two-tier ruling of 2026-08-22 and STANDOFF_MM's own entry. WHY THIS IS THE ONLY REFUSING NUMBER: a search margin may be generous because its cost is a lost candidate; the final demand's cost is a refused model, so it must be the number the owner will actually accept off the plate. No physics derives 0.300 mm and none is claimed for it. It is registered as an honestly-labelled external requirement so that a future coupon measurement of gap-feature preservation supersedes it rather than colliding with it. HONESTY PRECONDITION: this number only means what it says because the clearance test it feeds grades a TRUE gap. Before the honest-exemption change of 2026-08-22 the test skipped a radius-proportional length of every contact-declaring arm and read its verdict at the boundary station, so the graded quantity was neither the true gap nor a fixed function of it; the measured audit (.git/mask-honesty-evidence) found the old rule excusing an arm 0.242 mm INSIDE the model while refusing a same-axis thinner arm with 0.569 mm of real gap. A demanded gap registered against that predicate would have been a number about nothing. STATUS OF THAT PRECONDITION -- MET, as of commits 0a72bc8 and edcadd6, and recorded here rather than assumed (raised as challenge finding B2, 2026-08-22; cleared by honest-exemption-2026-08-22.md section 8 steps 3-6, which is the park its section 10.7 describes). Every contact's seat zone is now FROZEN at its creation site, at all four production mints -- the initial stub mint, brace model rungs, cco model anchors, and cco bifurcation, which inherits its host's zone by move rather than recomputing it -- and all four grading sites (brace::exact_clearance_rows, brace::evaluate_candidates, cco::clearance_survival, rewire::validate_changed_geometry) hand the kernel inset0 = inset1 = 0 and grade the FULL axis against the whole surface MINUS that frozen face set. The retired radius-proportional PR-2 mask reaches no distance call any more: it survives only as the transit window of brace::axis_illegal_rows, the penetration-interval screen, where a length measured from the seat is the right shape because the seat embed IS a length along the axis, and where zeroing it would condemn every seated arm for its own designed embed. The graded quantity is therefore a TRUE gap and this 0.300 is demanded of it. MEASURED through the production final pass by brace::tests::a_thinner_arm_never_grades_worse_than_a_fatter_one_through_the_production_final_pass -- the trip-wire's successor, on the same table-plus-foreign-panel fixture at |cos| = 0.2: a thin arm (r = 0.20 mm) grades -0.45 and is REFUSED, and a fat arm (r = 0.90 mm) on the SAME axis and the SAME surface grades -1.15 and is also REFUSED, so thin >= fat. At the pre-ruling single demand of 0.600 the same two rows read -0.75 and -1.45, both refused. BEFORE the change that pair read -0.45 REFUSED against +0.05 LEGAL (-0.75 against -0.25 at 0.600), which is the inversion this precondition was about: the thin arm did not move and the fat arm went from LEGAL to REFUSED. Under the frozen rule, monotonicity in the radius is a THEOREM rather than a fact about that fixture -- the tested station set is the whole axis and the tested face set is frozen, so the graded clearance is C - r_max - gap. The value is NOT the defect and never moved for it; the predicate was, and it is fixed.",
            )?,
            l_first_loss_mm,
            l_span_allowed_mm: Derived::new(
                l_span_allowed_value,
                ["L_SPAN_CRIT_MM", "PIXEL_PITCH_X_UM", "PIXEL_PITCH_Y_UM"],
                "THE AUDIT'S GATE CONSTANT for unsupported span: L_SPAN_CRIT_MM - 2 * d2 = 1.3449762463 mm, where d2 = hypot(PIXEL_PITCH_X_UM, PIXEL_PITCH_Y_UM) / 1000 = 0.0236008474 mm is one in-layer pixel diagonal. The subtraction is the audit's own instrument uncertainty under a full-containment raster fill rule (audit-invariants.md §1.6): the cured region is a subset of the true region, so a measured span understates the true span by at most one pixel footprint at each end, and rule 2 of the audit contract subtracts uncertainty from an allowance rather than adding it. Registered in plinth-core so the ENGINE can state, in a run report, whether the field it placed will pass the audit's own check; the audit crate at tools/plinth-audit computes its own copy from its own inputs and is deliberately not allowed to share this type (Cargo.toml:2-11). PLACEMENT MUST NOT READ THIS. Placement and gating are different jobs: a gate that subtracts a 1.53x unidentified condition shift and two pixel diagonals is right to refuse a finished file and wrong to place a seat, and placing at this value would place 2.5x denser than the measured evidence supports. See the placement-vs-gate ruling request in docs/plans/slice-sweep-core-2026-09-02.md §14 OR-2.",
            ),
            l_span_crit_mm: Derived::new(
                l_span_crit_value,
                ["L_SURVIVED_MAX_MM", "CONDITION_SHIFT_MAX"],
                "L_SURVIVED_MAX_MM / CONDITION_SHIFT_MAX = 2.1300322500 / 1.53 = 1.3921779412 mm. Moves the conservative end of the measured loss bracket out of plate 1's favourable condition and into the unfavourable one, using the only measured condition-shift magnitude available and its larger end. An intermediate of the audit's gate value; it decides nothing on its own.",
            ),
            l_survived_max_mm,
            lamina_reach_ratio: Derived::new(
                lamina_reach_ratio_value,
                ["L_SURVIVED_MAX_MM", "LAYER_HEIGHT_DEFAULT_MM"],
                "THE PLACEMENT REACH OF ONE CURED LAMINA PER UNIT OF ITS OWN THICKNESS: L_SURVIVED_MAX_MM / LAYER_HEIGHT_DEFAULT_MM = 2.1300322500 / 0.050 = 42.6006450, dimensionless. Expressed as a RATIO so it can be applied at other cured thicknesses: a cantilever of thickness t under uniform pressure P fails at L_crit = t * sqrt(sigma_f / 3P), which is exactly linear in t, and that linear-in-thickness SHAPE is the only thing the beam model supplies here. It is a COVERING RADIUS, not a bridge span -- see L_SURVIVED_MAX_MM's own note for why the single-root cantilever specimens make that the conservative reading. THE MAGNITUDE DISAGREES WITH THE REGISTRY BY 7.5x AND THAT IS DISCLOSED, NOT RECONCILED: with TENSILE_GREEN_MPA = 0.288 MPa and PEEL_PRESSURE_PA = 3000 Pa the same beam model predicts a ratio of sqrt(288/9) = 5.657, where the plates measured 42.6. Both of those inputs are registered on the diagnostic, non-AcceptSafe side of the ledger, and their own notes call them potentially about four times low, so this is most likely a fact about them rather than about the plates -- but it is unexplained, it is a standing question against any future green-strength coupon, and no constant is invented to close it. THE MEASUREMENT GOVERNS. Consequences that follow arithmetically and are stated so they are never rediscovered as surprises: hex nearest-neighbour spacing sqrt(3) * reach = 3.6892 mm at one lamina, denser than the owner's own 4.0 mm ChituBox practice; hex Voronoi cell area (3*sqrt(3)/2) * reach^2 = 11.7876 mm2 per seat. SUPERSEDED BY any coupon at a second layer thickness or a second overhang angle.",
            ),
            lamina_reach_ratio_audit_gate: Derived::new(
                lamina_reach_ratio_audit_gate_value,
                ["L_SPAN_ALLOWED_MM", "LAYER_HEIGHT_DEFAULT_MM"],
                "L_SPAN_ALLOWED_MM / LAYER_HEIGHT_DEFAULT_MM = 1.3449762463 / 0.050 = 26.8995249, dimensionless. What the AUDIT allows when it gates an emitted file, expressed in the same units as LAMINA_REACH_RATIO so the two can be printed side by side. REPORTING ONLY -- no placement or acceptance decision in this engine may read it. It exists so a run report can state whether a placed field will pass its own audit's span check, and so the difference between placing and gating is disclosed rather than discovered.",
            ),
            lamina_reach_ratio_first_loss: Derived::new(
                lamina_reach_ratio_first_loss_value,
                ["L_FIRST_LOSS_MM", "LAYER_HEIGHT_DEFAULT_MM"],
                "L_FIRST_LOSS_MM / LAYER_HEIGHT_DEFAULT_MM = 2.592036 / 0.050 = 51.84072, dimensionless. The rung at which the FIRST loss was recorded. REPORTING ONLY -- no decision may read it. It exists so a run report can state where the placement value sits inside the measured bracket [42.6006450, 51.84072] rather than claiming the chosen end is safe.",
            ),
            layer_height_default_mm,
            load_sharing,
            minima_drop_interior_faces: Measured::new(
                true,
                minima_source.clone(),
                None,
                "Measured contact-fix disposition; see §3.7 for the old-rule false-contact rates and per-layer propagation context.",
            )?,
            minima_require_island_birth: Measured::new(
                true,
                minima_source,
                None,
                "Measured contact-fix disposition; see §3.7 for the old-rule false-contact rates and per-layer propagation context.",
            )?,
            n_eff_cap: Derived::new(
                n_eff_cap_value,
                ["LOAD_SHARING"],
                "Reads the measured Additive payload's largest tested group size, 6.0; it is not a separate measurement.",
            ),
            orient_peel_area_exponent_low: Measured::new(
                0.24,
                EvidenceSource::project_record(
                    EvidenceId::new("orient-peel-area-exponent-porosity-manizani-2026-07-22")?,
                    "docs/evidence/literature/LITERATURE-NOTES.md",
                    "§1b(i), line 813: Manizani Table 1, four 20 mm lattice discs of differing porosity at a constant outer envelope, FEP 100 µm, speed 50 mm/min -- 2.68x area change -> 1.26x force change",
                    exact_date("2026-07-22")?,
                )?,
                None,
                "EXTERNAL REQUIREMENT -- literature, NOT measured on this machine and NOT with this resin. The LOW end of the peel area-exponent bracket: n ~ 0.24, from varying POROSITY at a constant outer envelope (Manizani et al., four 20 mm lattice discs, FEP 100 µm, ~0.8 mm/s). Registered per docs/plans/slice-sweep-core-2026-09-02.md §12 D14: the literature gives two exponents from two DIFFERENT experiments and neither is preferred over the other, so both are registered and LANE 3's orientation objective evaluates its peel-force term at all four corners of {this, ORIENT_PEEL_AREA_EXPONENT_HIGH} x {ORIENT_PEEL_REFERENCE_PRESSURE_PA_LOW, ORIENT_PEEL_REFERENCE_PRESSURE_PA_HIGH} rather than picking one. A future coupon at a second geometry supersedes this rather than colliding with it.",
            )?,
            orient_peel_area_exponent_high: Measured::new(
                0.46,
                EvidenceSource::project_record(
                    EvidenceId::new("orient-peel-area-exponent-envelope-manizani-2026-07-22")?,
                    "docs/evidence/literature/LITERATURE-NOTES.md",
                    "§1b(ii), line 815: Manizani Fig. 4, three pyramids shrinking WITH their envelope over 33 printed layers, FEP 100 µm -- 59x area reduction -> 6.5x force reduction",
                    exact_date("2026-07-22")?,
                )?,
                None,
                "EXTERNAL REQUIREMENT -- literature, NOT measured on this machine and NOT with this resin. The HIGH end of the peel area-exponent bracket: n ~ 0.46, from shrinking the outer envelope WITH the area (Manizani et al., three pyramids over 33 printed layers, FEP). For a POSE COMPARISON the envelope changes with the candidate direction, so this is the more apt of the two exponents by the design record's own reasoning (docs/plans/slice-sweep-core-2026-09-02.md §12, 'The exponent and pressure are a bracket') -- it is not therefore preferred as the sole value, it is evaluated as one bracket corner alongside ORIENT_PEEL_AREA_EXPONENT_LOW.",
            )?,
            orient_peel_reference_area_mm2: Measured::new(
                314.0,
                EvidenceSource::project_record(
                    EvidenceId::new("orient-peel-reference-area-manizani-2026-07-22")?,
                    "docs/evidence/literature/LITERATURE-NOTES.md",
                    "§1a and the peel_pressure_pa review, lines 770-774 and 826: Manizani Table 3, solid 20 mm-diameter disc, A = 314.16 mm^2, rounded to 314 in this review's own anchor statement",
                    exact_date("2026-07-22")?,
                )?,
                None,
                "EXTERNAL REQUIREMENT -- literature, NOT measured on this machine and NOT with this resin. The reference cross-section area the peel-force bracket F(A) = P_ref * A_ref * (A/A_ref)^n is anchored at: a solid 20 mm-diameter disc, 314 mm^2 (314.16 mm^2 unrounded), the same specimen ORIENT_PEEL_REFERENCE_PRESSURE_PA_LOW was measured on and the reference area of the independent Pan et al. series ORIENT_PEEL_REFERENCE_PRESSURE_PA_HIGH reads. Registered per docs/plans/slice-sweep-core-2026-09-02.md §12.",
            )?,
            orient_peel_reference_pressure_pa_low: Measured::new(
                9000.0,
                EvidenceSource::project_record(
                    EvidenceId::new("orient-peel-reference-pressure-manizani-2mms-2026-07-22")?,
                    "docs/evidence/literature/LITERATURE-NOTES.md",
                    "lines 136 and 772: Manizani Table 3, FEP 100 µm, 2 mm/s (120 mm/min), A = 314 mm^2, derived peel pressure ~9.6 kPa, rounded to the review's own anchor of 9 kPa at line 826",
                    exact_date("2026-07-22")?,
                )?,
                None,
                "EXTERNAL REQUIREMENT -- literature, NOT measured on this machine and NOT with this resin. The LOW end of the peel reference-pressure bracket: P_ref ~ 9000 Pa at A_ref = ORIENT_PEEL_REFERENCE_AREA_MM2, from Manizani et al.'s own in-situ load-cell measurement on FEP 100 µm at ~2 mm/s, a lift speed comparable to this project's own Saturn 4 practice. This is the review's own recommended anchor ('anchored at P_ref ~ 9 kPa, A_ref = 314 mm^2', LITERATURE-NOTES.md:826), not this project's own PEEL_PRESSURE_PA (3000 Pa, itself flagged in that same review as 'below every peel pressure measured in these papers'). Registered per docs/plans/slice-sweep-core-2026-09-02.md §12 D14 as the low corner of a bracket, alongside ORIENT_PEEL_REFERENCE_PRESSURE_PA_HIGH.",
            )?,
            orient_peel_reference_pressure_pa_high: Measured::new(
                19600.0,
                EvidenceSource::project_record(
                    EvidenceId::new("orient-peel-reference-pressure-pan-cylinder-2026-07-22")?,
                    "docs/evidence/literature/lit-green-peel.md",
                    "line 28: Pan, He, Xu & Feinerman 2017 (peer-reviewed), solid cylinder at fixed 314 mm^2 cross-section, 1.56 mm/s lift speed, 6.16 N -> 1.96 N/cm^2",
                    exact_date("2026-07-22")?,
                )?,
                None,
                "EXTERNAL REQUIREMENT -- literature, NOT measured on this machine and NOT with this resin, and measured on 2 mm PDMS-coated glass rather than this project's FEP/nFEP film -- a materially stickier interface class, flagged rather than hidden. The HIGH end of the peel reference-pressure bracket: 6.16 N / 3.14 cm^2 = 1.9618 N/cm^2 = 19.6 kPa, the top of the peer-reviewed Pan et al. cylinder/hexagon/triangle/star series at the SAME 314 mm^2 reference area as ORIENT_PEEL_REFERENCE_PRESSURE_PA_LOW ('1.56-1.96 N/cm^2', lit-green-peel.md:28). Registered per docs/plans/slice-sweep-core-2026-09-02.md §12 D14 as the high corner of a bracket; do not average it with the low corner, evaluate both.",
            )?,
            p8_chord_canon_micron: Measured::new(
                MicronGranularity::ONE_MICRON,
                EvidenceSource::corpus_study(
                    EvidenceId::new("phase8-micron-chord-canonicalization-2026-07-24")?,
                    "docs/engineering-journal/specs/SPEC-P8-RELOCATE.md",
                    "§5.3 Layer 3/amendment G",
                    exact_date("2026-07-24")?,
                )?,
                None,
                "f64 chords canonicalize to integer microns at one-micron comparison resolution.",
            )?,
            p8_pocket_voxel_mm: Measured::new(
                6.0,
                EvidenceSource::owner_decision(
                    EvidenceId::new("p8-pocket-voxel-owner-decision")?,
                    "docs/engineering-journal/specs/SPEC-P8-RELOCATE.md",
                    "§4.1",
                    exact_date("2026-07-25")?,
                )?,
                None,
                "Owner-approved Phase-8 pocket voxel size.",
            )?,
            pixel_pitch_x_um,
            pixel_pitch_y_um,
            print_floor_chord_mm,
            saturn_build_volume_mm: Measured::new(
                BuildVolumeMm::new(211.68, 118.37, 220.0)?,
                vendor_source,
                None,
                "The vendor's first, second, and third dimensions map to project X, Y, and Z; retrieval date 2026-07-31 is metadata, while 2025-01-22 is the evidence date.",
            )?,
            saturn_leading_release_edge: Measured::new(
                YDirection::NegativeY,
                release_front_source.clone(),
                None,
                "Project leading release edge is NegativeY.",
            )?,
            saturn_release_film_thickness_mm: Measured::new(
                0.127,
                EvidenceSource::installed_hardware(
                    EvidenceId::new("saturn4-ultra-pfa-install-2026-07-25")?,
                    "docs/engineering-journal/03-physics-and-constants.md",
                    "§3.7 release film",
                    exact_date("2026-07-25")?,
                )?,
                None,
                "Nominal thickness identifies the installed official Elegoo PFA and is not peel evidence.",
            )?,
            saturn_release_front_sweep_direction: Measured::new(
                YDirection::PositiveY,
                release_front_source,
                None,
                "Project release-front sweep direction is PositiveY: the front moves -Y → +Y.",
            )?,
            support_gate_max_work_per_batch_unit: Derived::new(
                64,
                ["SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED"],
                "Cap coefficient by the single support resource-cap rule: next_power_of_two(2 × observed) = next_power_of_two(2 × 23) = next_power_of_two(46) = 64. The batch unit is one (coverage_samples + 1) × (contacts + 1) product element, which is the shape of the specialized gate's own routing scan, so the coefficient bounds gate work at a constant multiple of the batch the gate was handed. It does NOT scale with face count, and deliberately so: 24-face make_raking_blade charges 612,887,212 gate operations where 429,120-face Weeping Treant charges 12,714,828,544, a 20× spread against an 18,000× face spread. What the coefficient still refuses is the gate's proposal-variant re-evaluation, which is quadratic in the batch rather than linear in it: the treant's 39,223-proposal merge gate plans 21,755,124,103,146 operations on the same 553,835,949-unit batch, 613× this coefficient, and is refused (FU-8c). Single-specimen basis: its one input was observed on the Weeping Treant v1.2 alone (429,120 faces, 2026-08-06), so this coefficient is n = 1. A second large-model measurement triggers re-derivation over both specimens; it must never be inherited silently as a treant-shaped constant. Addendum 2026-08-06 (FU-8c closed at a3308ec) — the sentence above is left standing and is now past tense: the mechanism it describes is gone. The gate no longer re-evaluates the contact set once per merge proposal (GateBatchIndex plus per-proposal route-key deltas), and gate_resource_plan is repriced to (samples + demands + 1) × contacts + measured proposal work. The treant's stated demand on that same 553,835,949-unit batch fell 108×, from 21,755,124,103,146 to 201,480,535,329, against this UNCHANGED cap and its unchanged 35,445,500,736 materialization — no cap, floor or coefficient was touched. The coefficient's verdict is unchanged: 201,480,535,329 is 363.8× this coefficient's linear budget on that batch, so the treant is STILL REFUSED at this gate, 5.68× over. 363.8 is no more adoptable as a coefficient than 613 was, and for the same reason: it would take the Cylinder (already at 89.6 % of the floor) and most of the corpus off their floors. FU-8c is closed on the defect it named, not on the treant reaching a seal; the residual 196,264,386,588 (97.4 % of the demand) is per-proposal sources × samples work on the perturbed route key and is opened as FU-8d. FU-23 re-derivation trigger, re-pointed to match the follow-ups ledger: this coefficient and SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED are to be re-derived after FU-8d lands, not after FU-8c — FU-8c has landed and the observed input was NOT re-derived, because the gate's charged work is still dominated by the residual FU-8d names. docs/engineering-journal/CURRENT-STATUS.md's follow-ups ledger was the authority on that ordering; that file was retired without a like-for-like replacement by the 2026-08-25 docs consolidation (`fb97b62`), so the ordering claim above is historical record, not a live-checkable citation. Addendum 2026-08-06 (FU-8d phase F2) — everything above is left standing; the demand it prices moved again and this coefficient again did not. F2 answers a merge proposal's nearest from the baseline sweep's retained witnesses plus ONE merged-seat kNN batch, so on the same 553,835,949-unit batch the stated demand fell a further 27.6×, from 201,480,535,329 to 7,299,316,559 — 13.2× this coefficient's linear budget, and 20.6 % of the unchanged 35,445,500,736 materialization, so gate_resource_plan now PASSES. No cap, floor or coefficient was touched by F2 either. The treant is still refused at this gate, one site later: variant_specialized_owner_face_count's descriptor budget crosses at variant 275 of 39,224, required 35,445,618,202 against limit 35,445,500,736. That residual is FU-8d part (2), the per-variant load/owner-face path, and phase F4 is what addresses it. Ruling R-f stands and is NOT yet satisfied: this coefficient and SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED are to be re-derived only after FU-8d phase F3 lands AND only from a treant run that COMPLETES the gate. F2's run does not complete it, so nothing was re-derived here. Addendum 2026-08-06 (FU-8d phases F3 and F4) — everything above is left standing, and the clause that has stopped being true is \"the treant is still refused at this gate\". F3 finalizes the load partition once per batch instead of once per variant, deletes the per-variant window matrices, collapses four per-proposal planning passes into one behind a count-derived pre-check, and replaces the gate's hand-built variants × faces load model with what specialized_load_resource_plan itself reports. The descriptor scan that crossed at variant 275 falls to 136,791,451 charged once and unique_face_finalizer_work ceases to exist as a term, while the honest load figure (6,592,813,742) is 1.43× the 4,618,272,984 model it replaced — so the stated demand ROSE, to 9,273,857,317, and that rise is a quadratic repriced honestly rather than a bound relaxed. That is 16.7× this coefficient's linear budget on the same 553,835,949-unit batch and 26.2 % of the unchanged 35,445,500,736 materialization; exact_operations is 10,457,910,996 (29.5 %). The batch is ADMITTED and EXECUTES: the treant evaluates 39,224 variants, accepts a merge and opens the next. No cap, floor or coefficient was touched by F3 or F4 either. Ruling R-f is STILL NOT satisfied and this coefficient is STILL NOT re-derived: F4's clean run at the F3 tip — PLINTH_THREADS=4, RSS watchdog armed at 90 GB and never fired — is an EXCEEDS-TIMEBOX at 600 s, so the treant never completes the gate stage and there is no completed-gate measurement to re-derive from. What consumes the 600 s was sampled directly rather than inferred, and is opened as FU-8e: the per-proposal fill_proposal_nearest_delta, which rebuilds a PointCloudIndex kd-tree over a ONE-point cloud once per proposal and is dominated by allocator traffic, plus the per-variant assess_coverage second behind it. Neither is a cap to raise. On the 12,714,828,544 this entry quotes as the treant's proposal-free gate charge, see the ruling note on SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED: F3's repricing plausibly supersedes that figure as an input, no re-measurement of it exists at this tip, and R-f blocks re-derivation of either constant regardless. Addendum 2026-08-07 (FU-8e phases E1-E4) — everything above is left standing, and this addendum exists to record that nothing in it moved. FU-8e removed the merge gate's per-proposal kd-tree construction (a one-source kNN fast path in the kernel, argued bit-identical to the tree's answer in five clauses and pinned on f64::to_bits) and its per-variant assess_coverage (a variant whose rewritten rows cannot have moved the coverage objective is handed the batch baseline's objective). The demand this coefficient prices is UNCHANGED at 9,273,857,317 against the unchanged 35,445,500,736 materialization — 16.7x this coefficient's linear budget on the same 553,835,949-unit batch — because plan_proposal_nearest_delta was deliberately not touched: work() is still 1 x touched_rows, and the treant's touched_rows re-measured at 1,041,544,686, to the row. No cap, floor or coefficient was touched by E1, E2, E3 or E4. Two mechanism claims in the addendum above are corrected rather than rewritten: the kd-trees were FREE (one one-point PointCloudIndex costs 83.3 ns, so 39,223 of them total about 3.27 ms per batch), and what cost was the query loop behind them, about 1.04e9 query iterations and 2.08e9 heap allocations per batch; assess_coverage's cost was likewise not allocation churn but two unconditional full scans of the nearest array per variant, about 254 GB of reads per batch. Measured 2026-08-07 under ruling R-g: the merge round falls 63.83 s to 14.68 s (4.35x), coverage per variant 17.05 s to 0.001 s, zero coverage fallbacks across 33 rounds and about 1.29 million proposal variants, peak RSS 2.446 GiB, watchdog armed at 90 GB and never fired, no typed refusal at any cap. Ruling R-f is STILL NOT satisfied and this coefficient is STILL NOT re-derived: the run is an EXCEEDS-TIMEBOX at 601 s having completed 33 merge rounds, and the tournament commits exactly one merge per round, so at most 1,365 rounds by the contact bound and about 460 at make_raking_blade's completion ratio put the gate stage at roughly 1.8 to 5.5 hours — 11x to 33x the timebox. The successor is opened as FU-8f (lazy first-safe gate acceptance plus non-incremental batch preparation), recorded as a finding AWAITING OWNER RULING and deliberately not built, because lazy acceptance severs charged-batch from executed-batch and would re-found GateRoutePlanMismatch a second time after F3. Arithmetic corrected 2026-08-07 (records fix): the kd-tree construction total in the addendum above was first written as 16.7 microseconds, which was an arithmetic slip in the design pass and not a second measurement -- 39,223 x 83.3 ns = 3.267 ms, and 16.7 microseconds across 39,223 builds would have implied 0.426 ns per construction. The per-build 83.3 ns is RETAINED as the design pass's measurement of record; only the product is corrected, and the conclusion it was quoted for is unchanged, because 3.27 ms is 0.009 % of the 37.33 s fill_proposal_nearest_delta frame: construction stays negligible and the fix still targets the query loop. No measured quantity, cap, floor or coefficient moves. See docs/engineering-journal/support-resource-caps-2026-08-05.md section 12. Addendum 2026-08-07 (FU-8f phase G3) — everything above is left standing, and this addendum exists to record that nothing this coefficient prices moved, plus the two clauses above that have stopped being true. The demand is UNCHANGED at 9,273,857,317 against the unchanged 35,445,500,736 materialization — 16.7x this coefficient's linear budget on the same 553,835,949-unit batch — because gate_resource_plan still prices plan.variants = proposals + 1, the exact_operations fold still folds all 39,224, and the refusal check still runs BEFORE the variant loop. G3 changes EXECUTION, not charge. First stopped clause: \"The batch is ADMITTED and EXECUTES: the treant evaluates 39,224 variants, accepts a merge and opens the next\" now reads — the batch is CHARGED for 39,224 variants and EXECUTES a PREFIX of them, stopping at the first variant that is gate_not_worse than the batch baseline, which was proposal 0 in all 28 rounds ever observed across two models. The executed prefix is metered as GateBatchOutcome::executed_variant_count and reported as SpecializedGateEvidence::evaluated_variant_count, beside gathered_pair_count (renamed from evaluated_pair_count, which had always counted proposals GATHERED rather than evaluated). Second stopped clause: FU-8f is no longer \"a finding AWAITING OWNER RULING and deliberately not built\" — the ruling came (Craig, 2026-08-07) and G3 built it, re-founding GateRoutePlanMismatch a SECOND time on a prefix-wise comparison: the batch executed a prefix of the work it planned, the first executed_variant_count variants, in plan order, each exactly once, at the peak the plan stated. Charged and executed are severed and BOTH are stated; charging the prefix was considered and refused, because a bound not knowable until after the work that determines it bounds nothing. No cap, floor or coefficient was touched by G3, and all seven frozen OD-8 digests are byte-identical at its tip. Ruling R-f is STILL NOT satisfied and this coefficient is STILL NOT re-derived: a treant run at the G3 tip, on a QUIET machine with no concurrent workflow — unlike every measurement quoted above — is an EXCEEDS-TIMEBOX at 600 s in both permitted attempts (peak RSS 3,479,600 KB, watchdog armed at 90 GB and never fired), so no run has yet COMPLETED the gate. Counting the tournament's rounds is FU-8f phase G4's deliverable. See docs/engineering-journal/support-resource-caps-2026-08-05.md section 13. Addendum 2026-08-07 (FU-8f phases G4 and G5) — everything above is left standing, THIS COEFFICIENT IS UNCHANGED at 64, and two things in it have moved. First, G4 delivered the round count on a quiet machine: two attempts, EXCEEDS-TIMEBOX at 602 s each, 436 and 430 merge rounds, contacts 1,366 -> 931 and 1,366 -> 937, exactly one merge committed per round throughout, peak RSS 2.554 and 2.528 GiB with the watchdog armed at 90 GB and never fired. So the clause 'Counting the tournament's rounds is phase G4's deliverable' is now past tense, and R-f is still NOT satisfied because neither attempt COMPLETED the gate. Second, G5 repriced the load term and the demand this coefficient prices has FALLEN, by 4,748,065,084: gate_resource_plan's stated demand goes 9,273,857,317 -> 4,525,792,233 and exact_operations goes 10,457,910,996 -> 5,709,845,912, entirely because the gate's load term goes 6,592,813,742 -> 1,844,748,658 (baseline 5,543,831,115 -> 795,766,031, proposal bound 1,048,982,627 unchanged). baseline_rows 554,926,572, demand_rows 42,949,185 and proposal work 2,083,167,818 are byte-identical. That is 4,525,792,233 against the unchanged 35,445,500,736 materialization -- 12.8 % where it was 26.2 %, and 8.2x this coefficient's linear budget on the same 553,835,949-unit batch where it was 16.7x. A falling demand can create no refusal. No cap, floor or coefficient was raised, lowered or touched by G4 or G5, and all seven frozen OD-8 digests are byte-identical at the G5 tip. SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED is STILL NOT re-derived: R-f wants a run that completes the gate, and G5 ran no completion attempt -- its single measurement run was stopped at 260 s having reached the gate stage, which is not a completed tournament. See docs/engineering-journal/support-resource-caps-2026-08-05.md section 15.",
            ),
            support_gate_work_per_batch_unit_observed: Measured::new(
                23,
                treant_batch_source.clone(),
                None,
                "Weeping Treant v1.2, 429,120 support faces, measured 2026-08-06 with FU-8b defects (a), (b) and (c) fixed: the specialized gate's proposal-free batch stated exact_operations = 12,714,828,544 over 405,146 coverage samples and 1,366 contacts, i.e. (405,146 + 1) × (1,366 + 1) = 553,835,949 batch units, so the batch costs 12,714,828,544 / 553,835,949 = 22.958 work units per batch unit, recorded as the ceiling 23. The pre-fix figure for the same batch was not a demand at all: it was the charge counter tripping at limit + 1 inside a selection sort. Single-specimen basis: the Weeping Treant v1.2 alone, 429,120 faces, 2026-08-06. The corpus's proposal-free batches sit between 1.90 and 2.09 work units per batch unit, so this observation is 11× the corpus band and has no second specimen at its own scale; a second large-model measurement triggers re-derivation of SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT rather than silent inheritance. Ruling note 2026-08-06 (FU-8d record close) — this is the primary note on 12,714,828,544, and every other site carrying that figure points here. Read it as a dated FU-8b-era measurement of that batch, not as a current input. FU-8d phase F3 reprices what the gate charges for the same stage: the load phase is priced from specialized_load_resource_plan's own figure instead of the gate's variants × faces model, the per-variant owner-face descriptor scan is charged once rather than per variant, and unique_face_finalizer_work ceases to exist as a term. That repricing PLAUSIBLY SUPERSEDES 12,714,828,544 as the input to this constant, and therefore the 22.958 → 23 above. It is only plausibly superseded because NO RE-MEASUREMENT EXISTS AT THIS TIP: the census prints scaled-cap materializations and stage errors, not per-batch demands, so nothing on this branch restates the proposal-free batch's charge under F3's pricing. Nothing here is corrected: 23 stands as recorded, unchanged and un-re-derived. Re-derivation is blocked in either case by ruling R-f — it requires a treant run that COMPLETES the gate stage, and phase F4's two clean runs are EXCEEDS-TIMEBOX at 607 s and 602 s with no typed refusal. When R-f is satisfied, the proposal-free batch charge and this ratio are to be re-measured together; neither is to be inherited into the other. Recorded in docs/engineering-journal/support-resource-caps-2026-08-05.md §7.1.",
            )?,
            support_knn_max_work_per_face: Derived::new(
                2_048,
                ["SUPPORT_KNN_WORK_PER_FACE_OBSERVED"],
                "Cap coefficient by the single support resource-cap rule: next_power_of_two(2 × observed) = next_power_of_two(2 × 696) = next_power_of_two(1392) = 2048. It bounds kNN work at a constant multiple of the support mesh's own face count, so an all-pairs (face²) batch is still refused for any mesh above 2048 faces. Single-specimen basis: its one input was observed on the Weeping Treant v1.2 alone (429,120 faces, 2026-08-05), so this coefficient is n = 1. A second large-model measurement triggers re-derivation over both specimens; it must never be inherited silently as a treant-shaped constant. It scales exactly one batch, the perception minima-to-face kNN — the specialized coverage-route kNN batch shares the KnnBatchLimits shape but has no measurement of record and stays at SUPPORT_KNN_MAX_WORK_FLOOR. Witness registration 2026-08-07 (FU-25 Phase 2): the registered downstream-utility witness support-face-scaled-resource-demand (plinth_decimate::SUPPORT_FACE_SCALED_RESOURCE_DEMAND, crates/plinth-decimate/src/lib.rs) names THIS coefficient as one of the two registered execution-authority resources whose demand is an exact function of the support-mesh face count F alone — here the minima-to-face kNN batch's source_count, which is F itself. That witness is the only thing that authorizes a reduced mesh to leave decimation, and it is minted only when this demand strictly falls. This clause is the reverse half of that reference: the witness names the constant and the constant names the witness, so neither side can move without the other being visible from its own registry entry. It changes no coefficient, no cap and nothing in the n = 1 basis above.",
            ),
            support_knn_work_per_face_observed: Measured::new(
                696,
                treant_cap_source.clone(),
                None,
                "Weeping Treant v1.2, 429,120 support faces: the minima-to-face kNN batch planned source_count × query_count = 298,667,520 work units, exactly 429,120 × 696, so the batch costs 696 work units per face. Single-specimen basis: the Weeping Treant v1.2 alone, 429,120 faces, 2026-08-05. It is the only corpus model above 4,760 faces, so this observation has no second specimen and no spread; a second large-model measurement triggers re-derivation of SUPPORT_KNN_MAX_WORK_PER_FACE rather than silent inheritance of a treant-shaped constant.",
            )?,
            support_load_max_work_per_batch_unit: Derived::new(
                64,
                ["SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED"],
                "Cap coefficient by the single support resource-cap rule: next_power_of_two(2 × observed) = next_power_of_two(2 × 31) = next_power_of_two(62) = 64. The batch unit is one (load rows + 1) × (contacts + 1) product element, which is the shape of the load finalizer's own dense nearest scan, so the coefficient bounds load work at a constant multiple of the batch the finalizer was handed rather than at a constant. It admits the ~27 units per row-contact pair the finalizer spends (owner_log + 4, with owner_log = 23 on the treant) with 2.1× headroom, and still refuses anything quadratic in the row set. Single-specimen basis: its one input was observed on the Weeping Treant v1.2 alone (429,120 faces, 2026-08-06), so this coefficient is n = 1. A second large-model measurement triggers re-derivation over both specimens; it must never be inherited silently as a treant-shaped constant. Correction 2026-08-06 (record fix F1): the input was restated from 30 to 31 after the demand was attributed to its real batch; the coefficient is unchanged at 64, because next_power_of_two rounds both 60 and 62 to 64. Addendum 2026-08-07 (FU-8f phase G5) — every sentence above is left standing, THIS COEFFICIENT IS UNCHANGED at 64, and its observed input is deliberately NOT re-derived. The demand it was derived from has been repriced downward: G5 retired load_resource_plan_from_counts's rows x contacts x (owner_log + 4) scan term -- the model of a dense contacts x rows scan that FU-8f phase G1 had already replaced with a CSR transpose, and which G1 left standing and over-stating on purpose -- and replaced it with the transpose's own terms, Sigma |eligible_owners| x (log(contacts) + 2) x 2 for the two eligibility traversals plus cells x (LOAD_WITNESS_SLOTS + 5) for the fill and the scan. On the same 130,548-row / 1,366-contact batch this coefficient was derived from, measured 2026-08-07 on a quiet machine, one instrumented release run, instrumentation reverted by editing before the gates: 5,402,535,619 -> 795,359,775, a 6.79x fall. The clause 'It admits the ~27 units per row-contact pair the finalizer spends (owner_log + 4, with owner_log = 23 on the treant) with 2.1x headroom' is therefore HISTORY: the finalizer no longer spends per row-contact pair at all, because it no longer visits row-contact pairs. Its per-unit spend on this batch is now 795,359,775 / 178,460,483 = 4.46, which the rule would round to a coefficient of 8. RE-DERIVING IT IS REFUSED HERE, for a reason that is a cap and not an oversight: 64 -> 8 would LOWER this cap eightfold, and cap reduction is the direction that creates refusals. FU-23/R4's re-derivation trigger is a SECOND SPECIMEN, not the same specimen getting cheaper. The derivation input of record therefore stays 31, dated 2026-08-06, and crates/plinth-support/tests/resource_cap_policy.rs pins both figures side by side -- TREANT_LOAD_OPERATIONS_REQUIRED as the derivation input and TREANT_LOAD_OPERATIONS_REPRICED as what the treant now asks. The finding that follows, recorded because it changes what this coefficient is for: 795,359,775 is BELOW the 2,000,000,000 flat floor, so the treant no longer leaves the floor at this cap and no corpus model does. The scaling is kept regardless. No cap, floor or coefficient was raised, lowered or touched by G5, and all seven frozen OD-8 digests are byte-identical at its tip. See docs/engineering-journal/support-resource-caps-2026-08-05.md section 15. Correction 2026-08-07 (verification fix): the hypothetical coefficient in the addendum above was stated as 8, and 8 is not what the pinned rule yields. The rule (the_batch_product_coefficients_are_the_rule_applied_to_their_own_observations, crates/plinth-core/tests/constants_evidence.rs) is next_power_of_two(2 x div_ceil(demand, units)) -- div_ceil FIRST, then x2, then next_power_of_two, which rounds UP: div_ceil(795,359,775, 178,460,483) = 5, 2 x 5 = 10, next_power_of_two(10) = 16. The earlier note rounded 2 x 4.46 down to 8 and skipped the div_ceil. 64 -> 16 would LOWER this cap FOURfold, not eightfold. The refusal to re-derive stands exactly as recorded and for the same reason -- cap reduction is the direction that creates refusals, and FU-23/R4's trigger is a second specimen, not the same specimen getting cheaper; only the arithmetic of the hypothetical is corrected.",
            ),
            support_load_work_per_batch_unit_observed: Measured::new(
                31,
                treant_batch_source,
                None,
                "Weeping Treant v1.2, 429,120 support faces, measured 2026-08-06 with FU-8b defects (a), (b) and (c) fixed: finalize_specialized_load_partition (crates/plinth-support/src/perception.rs) planned operations = 5,402,535,619 over 130,548 rows and 1,366 contacts, i.e. (130,548 + 1) × (1,366 + 1) = 178,460,483 batch units, so the partition costs 5,402,535,619 / 178,460,483 = 30.28 work units per batch unit, recorded as the ceiling 31. The 130,548 rows are 16,627 generic load-eligibility rows plus the 113,921 distinct faces of the 7,971,451 specialized (owner, face) pairs. The pre-fix figure was 2.95e11 against a plan that predicted 7,988,078 rows — 16,627 base rows plus one row per pair — because the prediction counted pairs where the builder had always produced one row per distinct face; FU-8b fix (B) corrected the prediction, and the predicted row count fell 61.2× to the measured 130,548. Correction 2026-08-06 (record fix F1): this entry previously paired the same 5,402,535,619 demand with 134,368 rows and 183,682,423 batch units, giving 29.41 and a ceiling of 30. That row count belongs to no batch this build plans; one scratch-instrumented treant run printed resource.rows = 130,548 at the load checkpoint alongside operations = 5,402,535,619, and the census scaled-cap row prints the same 130,548 (materialized 11,421,470,912). Single-specimen basis: the Weeping Treant v1.2 alone, 429,120 faces, 2026-08-06; Sphere, the next largest corpus model, plans 574 rows, so there is no second specimen at this scale and a second large-model measurement triggers re-derivation of SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT rather than silent inheritance. Correction 2026-08-06 (record fix F10): the preceding clause previously read \"the next largest corpus model plans 66 rows\". 66 is the Cylinder's load batch, not the next largest model's; the next largest corpus model is Sphere (4,760 faces, against the treant's 429,120), and its load batch is 574 rows, as CORPUS_LOAD_BATCHES pins in crates/plinth-support/tests/resource_cap_policy.rs. The n = 1 conclusion is unchanged and if anything better supported: 574 rows is still 227x below the treant's 130,548, so the corpus offers no second specimen anywhere near this batch scale.",
            )?,
            support_parity_grid_max_bytes_per_face: Derived::new(
                512,
                ["SUPPORT_PARITY_GRID_REFS_PER_FACE_OBSERVED"],
                "Cap coefficient by the single support resource-cap rule applied to the exact parity-grid projection 81·faces + 36·references + 4 bytes (rust/geom_kernel/src/parity.rs axis_memory_bounds): observed bytes per face = 81 + 36 × 2.8303132 = 182.8913, and next_power_of_two(2 × 182.8913) = next_power_of_two(366) = 512. The 4.2× margin this leaves over the observation (11.97 admitted refs/face against 2.83 observed) is wider than the kNN coefficient's 1.7× and is ratified as such: one uniform rule for both coefficients is worth more than a tighter grid bound, and refs/face is the quantity measured to vary 526 ↔ 2.83 across the corpus. Single-specimen basis: its one input was observed on the Weeping Treant v1.2 alone (429,120 faces, 2026-08-05), so this coefficient is n = 1. A second large-model measurement triggers re-derivation over both specimens; it must never be inherited silently as a treant-shaped constant. Witness registration 2026-08-07 (FU-25 Phase 2): the registered downstream-utility witness support-face-scaled-resource-demand (plinth_decimate::SUPPORT_FACE_SCALED_RESOURCE_DEMAND, crates/plinth-decimate/src/lib.rs) names THIS coefficient as the second of the two registered execution-authority resources whose demand is an exact function of the support-mesh face count F alone — here the 81·F per-triangle term of the projection 81·faces + 36·references + 4 that this coefficient bounds. The witness deliberately claims nothing about the 36·references term, for exactly the 526 ↔ 2.83 reason recorded above: R falls sublinearly as a mesh coarsens and could in principle be flat, so only the 81·F term is exactly provable from a face-count reduction. This clause is the reverse half of that reference: the witness names the constant and the constant names the witness, so neither side can move without the other being visible from its own registry entry. It changes no coefficient, no cap and nothing in the n = 1 basis above.",
            ),
            support_parity_grid_refs_per_face_observed: Measured::new(
                1_214_544.0 / 429_120.0,
                treant_cap_source,
                None,
                "Weeping Treant v1.2, 429,120 support faces: the axis parity grid projected a peak of 78,482,308 bytes, which inverts through 81·faces + 36·references + 4 to exactly 1,214,544 (cell, face) references. References, not faces, dominate that projection, and their ratio to faces falls as a mesh refines — 526 per face on the 244-face Cylinder against 2.83 here — so the treant is the tight case and the byte floor governs every coarse model. Single-specimen basis: the Weeping Treant v1.2 alone, 429,120 faces, 2026-08-05. The 526 ↔ 2.83 spread is the reason that matters: this quantity is measured to vary by 186× across the corpus, and one point on a curve that steep is not a distribution. A second large-model measurement triggers re-derivation of SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE rather than silent inheritance of a treant-shaped constant.",
            )?,
            tip_diameter_min_allowed_mm,
            tip_radius_max_grown_scar_mm: Measured::new(
                0.400,
                EvidenceSource::owner_decision(
                    EvidenceId::new("tip-grown-scar-maximum-owner-field-data")?,
                    "docs/engineering-journal/03-physics-and-constants.md",
                    "§3.2",
                    exact_date("2026-08-20")?,
                )?,
                None,
                "EXTERNAL REQUIREMENT -- field data, NOT physics and NOT a wet-finger estimate. The owner's measured field ceiling for a support tip: 0.400 mm RADIUS = 0.800 mm DIAMETER, ruled by Craig on 2026-08-20 and evidenced by his own printing practice -- the heaviest support he has ever needed is a 0.8 mm DIAMETER sphere, and he has never needed a larger scar on a model. UNITS TRAP: this registry speaks RADIUS and both the owner and the ChituBox UI speak DIAMETER; state both or the number lands 2x wrong. No physics derives this number and none is claimed for it. It is the largest scar the owner will accept, registered as an honestly-labelled external requirement so that a future coupon measurement of thin-tip attachment supersedes it rather than colliding with it. WHAT IT BOUNDS, exactly: GROWTH-FOR-ATTACHMENT only. PR-8's grow-to-attest ladder enlarges a tip BEYOND what the load law asked for, purely so the attestation ring can reach solid, and that enlargement is a scar the operator did not choose. The ascending ladder and its descending refinement scan therefore truncate at min(this bound, TIP_RADIUS_MAX_LEGACY_MM) = 0.400 mm radius (0.800 mm diameter), and a contact whose cure would need a larger tip is REFUSED through the existing disclosed ABANDONED LOAD line, which states the seat, the load, the rungs tried, and this bound by name as the stopper. WHAT IT DOES NOT BOUND: tips the LOAD LAW sizes. The area law -- pi*r^2 >= carried_area/X with X = X_ALLOWED = 12 -- may legitimately demand up to the registered TIP_RADIUS_MAX_LEGACY_MM of 1.000 mm radius (2.000 mm diameter) for a genuinely heavy area, and this bound neither clamps nor refuses such a tip: a load-sized tip above 0.400 mm radius that attests is emitted unchanged. The two are different appetites -- carrying weight versus reaching solid -- and only the second is bounded here. This constant WEAKENS NO REFUSAL: it can only turn a cure into a refusal, never a refusal into a cure.",
            )?,
            tip_radius_min_allowed_mm: Derived::new(
                tip_radius_value,
                ["TIP_DIAMETER_MIN_ALLOWED_MM"],
                "Exact diameter-to-radius conversion: TIP_DIAMETER_MIN_ALLOWED_MM / 2 = 0.050 mm RADIUS, from 0.100 mm DIAMETER. Was 0.075 mm radius (0.150 mm diameter) until the owner ruling of 2026-08-19 superseded its parent. This is a RADIUS and its parent is a DIAMETER; halve or double, never substitute. The warning this note used to carry -- do not substitute the tip-sizing default -- is now moot in VALUE and live in KIND: TIP_RADIUS_DEFAULT_MM is also 0.050 mm radius (0.100 mm diameter) since the same ruling, but it is the GENERATION clamp minimum and this is the VALIDATOR minimum. They agree today by ruling, not by derivation, and neither may be written in terms of the other.",
            ),
            x_allowed: Derived::new(
                x_allowed_value,
                ["X_STAR_CONE"],
                "Binding commitment: floor(lower(X_STAR_CONE)/2) = floor(25.00/2) = 12. X_STAR_CYLINDER supplies only the margin check 19.91/12 = 1.66× and is not a derivation input.",
            ),
            x_star_cone,
            x_star_cylinder: Measured::new(
                FiniteInterval::new(19.91, 30.48, BoundKind::Exclusive, BoundKind::Exclusive)?,
                EvidenceSource::calibration_plate(
                    PlateId::from("COUPON-PLATE-6-support-arm-capacity"),
                    "docs/engineering-journal/reference/plate-06-results.md",
                    "§5 Finding 2",
                    exact_date("2026-07-31")?,
                )?,
                None,
                "Cylindrical-arm bracket with n = 1 per bracket edge, conditional on 4.00 mm arms and a 30 mm² cap; retain the interval and never its midpoint.",
            )?,
        })
    }

    #[must_use]
    pub const fn cco_x_pair_load_discount(&self) -> &Derived<f64> {
        &self.cco_x_pair_load_discount
    }

    #[must_use]
    pub const fn decimation_max_absolute_deviation_mm(&self) -> &Derived<f64> {
        &self.decimation_max_absolute_deviation_mm
    }

    #[must_use]
    pub const fn decimation_max_p999_deviation_mm(&self) -> &Derived<f64> {
        &self.decimation_max_p999_deviation_mm
    }

    #[must_use]
    pub const fn decimation_max_p99_deviation_mm(&self) -> &Derived<f64> {
        &self.decimation_max_p99_deviation_mm
    }

    /// The TRUE surface-to-surface gap, in mm, the FINAL clearance pass
    /// demands between an arm's solid surface and the model.
    ///
    /// 0.300 mm, Craig's ruling of 2026-08-22 from field experience. This is
    /// the only refusing clearance number; `STANDOFF_MM` (0.600 mm) is the
    /// placement-family SEARCH margin and refuses nothing final.
    #[must_use]
    pub const fn final_clearance_gap_mm(&self) -> &Measured<f64> {
        &self.final_clearance_gap_mm
    }

    #[must_use]
    pub const fn condition_shift_max(&self) -> &Measured<f64> {
        &self.condition_shift_max
    }

    #[must_use]
    pub const fn l_first_loss_mm(&self) -> &Measured<f64> {
        &self.l_first_loss_mm
    }

    #[must_use]
    pub const fn l_span_allowed_mm(&self) -> &Derived<f64> {
        &self.l_span_allowed_mm
    }

    #[must_use]
    pub const fn l_span_crit_mm(&self) -> &Derived<f64> {
        &self.l_span_crit_mm
    }

    #[must_use]
    pub const fn l_survived_max_mm(&self) -> &Measured<f64> {
        &self.l_survived_max_mm
    }

    /// The placement reach ratio: reach of one cured lamina per unit of its
    /// own cured thickness. See the registered note; this is the value the
    /// slice-sweep places against.
    #[must_use]
    pub const fn lamina_reach_ratio(&self) -> &Derived<f64> {
        &self.lamina_reach_ratio
    }

    /// Reporting only. No decision may read this.
    #[must_use]
    pub const fn lamina_reach_ratio_audit_gate(&self) -> &Derived<f64> {
        &self.lamina_reach_ratio_audit_gate
    }

    /// Reporting only. No decision may read this.
    #[must_use]
    pub const fn lamina_reach_ratio_first_loss(&self) -> &Derived<f64> {
        &self.lamina_reach_ratio_first_loss
    }

    #[must_use]
    pub const fn layer_height_default_mm(&self) -> &Measured<f64> {
        &self.layer_height_default_mm
    }

    #[must_use]
    pub const fn load_sharing(&self) -> &Measured<LoadSharing> {
        &self.load_sharing
    }

    #[must_use]
    pub const fn minima_drop_interior_faces(&self) -> &Measured<bool> {
        &self.minima_drop_interior_faces
    }

    #[must_use]
    pub const fn minima_require_island_birth(&self) -> &Measured<bool> {
        &self.minima_require_island_birth
    }

    #[must_use]
    pub const fn n_eff_cap(&self) -> &Derived<f64> {
        &self.n_eff_cap
    }

    /// The HIGH end of the peel area-exponent bracket, n ~ 0.46 (Manizani,
    /// envelope shrinking with area). See [`Self::orient_peel_area_exponent_low`].
    #[must_use]
    pub const fn orient_peel_area_exponent_high(&self) -> &Measured<f64> {
        &self.orient_peel_area_exponent_high
    }

    /// The LOW end of the peel area-exponent bracket, n ~ 0.24 (Manizani,
    /// porosity at constant envelope). Evaluated alongside
    /// [`Self::orient_peel_area_exponent_high`]; neither is preferred (§12 D14).
    #[must_use]
    pub const fn orient_peel_area_exponent_low(&self) -> &Measured<f64> {
        &self.orient_peel_area_exponent_low
    }

    /// The reference cross-section area, 314 mm², the peel-force bracket is
    /// anchored at.
    #[must_use]
    pub const fn orient_peel_reference_area_mm2(&self) -> &Measured<f64> {
        &self.orient_peel_reference_area_mm2
    }

    /// The HIGH end of the peel reference-pressure bracket, ~19.6 kPa (Pan et
    /// al., PDMS, flagged as a stickier interface than this project's FEP/nFEP).
    #[must_use]
    pub const fn orient_peel_reference_pressure_pa_high(&self) -> &Measured<f64> {
        &self.orient_peel_reference_pressure_pa_high
    }

    /// The LOW end of the peel reference-pressure bracket, ~9 kPa (Manizani,
    /// FEP, ~2 mm/s -- the review's own recommended anchor).
    #[must_use]
    pub const fn orient_peel_reference_pressure_pa_low(&self) -> &Measured<f64> {
        &self.orient_peel_reference_pressure_pa_low
    }

    #[must_use]
    pub const fn p8_chord_canon_micron(&self) -> &Measured<MicronGranularity> {
        &self.p8_chord_canon_micron
    }

    #[must_use]
    pub const fn p8_pocket_voxel_mm(&self) -> &Measured<f64> {
        &self.p8_pocket_voxel_mm
    }

    #[must_use]
    pub const fn pixel_pitch_x_um(&self) -> &Measured<f64> {
        &self.pixel_pitch_x_um
    }

    #[must_use]
    pub const fn pixel_pitch_y_um(&self) -> &Measured<f64> {
        &self.pixel_pitch_y_um
    }

    #[must_use]
    pub const fn print_floor_chord_mm(&self) -> &Measured<f64> {
        &self.print_floor_chord_mm
    }

    #[must_use]
    pub const fn saturn_build_volume_mm(&self) -> &Measured<BuildVolumeMm> {
        &self.saturn_build_volume_mm
    }

    #[must_use]
    pub const fn saturn_leading_release_edge(&self) -> &Measured<YDirection> {
        &self.saturn_leading_release_edge
    }

    #[must_use]
    pub const fn saturn_release_film_thickness_mm(&self) -> &Measured<f64> {
        &self.saturn_release_film_thickness_mm
    }

    #[must_use]
    pub const fn saturn_release_front_sweep_direction(&self) -> &Measured<YDirection> {
        &self.saturn_release_front_sweep_direction
    }

    #[must_use]
    pub const fn support_gate_max_work_per_batch_unit(&self) -> &Derived<u64> {
        &self.support_gate_max_work_per_batch_unit
    }

    #[must_use]
    pub const fn support_gate_work_per_batch_unit_observed(&self) -> &Measured<u64> {
        &self.support_gate_work_per_batch_unit_observed
    }

    #[must_use]
    pub const fn support_load_max_work_per_batch_unit(&self) -> &Derived<u64> {
        &self.support_load_max_work_per_batch_unit
    }

    #[must_use]
    pub const fn support_load_work_per_batch_unit_observed(&self) -> &Measured<u64> {
        &self.support_load_work_per_batch_unit_observed
    }

    #[must_use]
    pub const fn support_knn_max_work_per_face(&self) -> &Derived<u64> {
        &self.support_knn_max_work_per_face
    }

    #[must_use]
    pub const fn support_knn_work_per_face_observed(&self) -> &Measured<u64> {
        &self.support_knn_work_per_face_observed
    }

    #[must_use]
    pub const fn support_parity_grid_max_bytes_per_face(&self) -> &Derived<u64> {
        &self.support_parity_grid_max_bytes_per_face
    }

    #[must_use]
    pub const fn support_parity_grid_refs_per_face_observed(&self) -> &Measured<f64> {
        &self.support_parity_grid_refs_per_face_observed
    }

    #[must_use]
    pub const fn tip_diameter_min_allowed_mm(&self) -> &Measured<f64> {
        &self.tip_diameter_min_allowed_mm
    }

    /// The MAXIMUM tip a grow-for-attachment rescue may reach, mm RADIUS.
    ///
    /// 0.400 mm RADIUS = 0.800 mm DIAMETER. An external requirement carrying
    /// the owner's field data, not a physics result: it bounds the scar that
    /// PR-8's growth ladder is allowed to leave on a model, and it bounds
    /// nothing the LOAD LAW sizes.
    #[must_use]
    pub const fn tip_radius_max_grown_scar_mm(&self) -> &Measured<f64> {
        &self.tip_radius_max_grown_scar_mm
    }

    #[must_use]
    pub const fn tip_radius_min_allowed_mm(&self) -> &Derived<f64> {
        &self.tip_radius_min_allowed_mm
    }

    #[must_use]
    pub const fn x_allowed(&self) -> &Derived<f64> {
        &self.x_allowed
    }

    #[must_use]
    pub const fn x_star_cone(&self) -> &Measured<FiniteInterval> {
        &self.x_star_cone
    }

    #[must_use]
    pub const fn x_star_cylinder(&self) -> &Measured<FiniteInterval> {
        &self.x_star_cylinder
    }

    /// Returns the complete declaration batch in stable ID order.
    pub fn descriptors(&self) -> Result<[ConstantDescriptor; 46], RegistryError> {
        Ok([
            ConstantDescriptor::from_derived(
                ConstantId::from("CCO_X_PAIR_LOAD_DISCOUNT"),
                "1",
                SCALAR,
                &self.cco_x_pair_load_discount,
            )?,
            // Sorted into place rather than appended. The design of record's
            // D12 says to append at the end of the declaration order; the
            // renderer-order test in `tests/constants_evidence.rs` walks
            // EXPECTED_EVIDENCE_IDS against the SORTED rendered table and
            // requires monotone positions, so appending would break a test the
            // design did not account for. Sorted placement satisfies both.
            ConstantDescriptor::from_measured(
                ConstantId::from("CONDITION_SHIFT_MAX"),
                "1",
                SCALAR,
                &self.condition_shift_max,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("DECIMATION_MAX_ABSOLUTE_DEVIATION_MM"),
                "mm",
                SCALAR,
                &self.decimation_max_absolute_deviation_mm,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("DECIMATION_MAX_P999_DEVIATION_MM"),
                "mm",
                SCALAR,
                &self.decimation_max_p999_deviation_mm,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("DECIMATION_MAX_P99_DEVIATION_MM"),
                "mm",
                SCALAR,
                &self.decimation_max_p99_deviation_mm,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("FINAL_CLEARANCE_GAP_MM"),
                "mm",
                SCALAR,
                &self.final_clearance_gap_mm,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("LAMINA_REACH_RATIO"),
                "1",
                SCALAR,
                &self.lamina_reach_ratio,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("LAMINA_REACH_RATIO_AUDIT_GATE"),
                "1",
                SCALAR,
                &self.lamina_reach_ratio_audit_gate,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("LAMINA_REACH_RATIO_FIRST_LOSS"),
                "1",
                SCALAR,
                &self.lamina_reach_ratio_first_loss,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("LAYER_HEIGHT_DEFAULT_MM"),
                "mm",
                SCALAR,
                &self.layer_height_default_mm,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("LOAD_SHARING"),
                "1",
                "load_sharing",
                &self.load_sharing,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("L_FIRST_LOSS_MM"),
                "mm",
                SCALAR,
                &self.l_first_loss_mm,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("L_SPAN_ALLOWED_MM"),
                "mm",
                SCALAR,
                &self.l_span_allowed_mm,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("L_SPAN_CRIT_MM"),
                "mm",
                SCALAR,
                &self.l_span_crit_mm,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("L_SURVIVED_MAX_MM"),
                "mm",
                SCALAR,
                &self.l_survived_max_mm,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("MINIMA_DROP_INTERIOR_FACES"),
                "1",
                SCALAR,
                &self.minima_drop_interior_faces,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("MINIMA_REQUIRE_ISLAND_BIRTH"),
                "1",
                SCALAR,
                &self.minima_require_island_birth,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("N_EFF_CAP"),
                "count",
                SCALAR,
                &self.n_eff_cap,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("ORIENT_PEEL_AREA_EXPONENT_HIGH"),
                "1",
                SCALAR,
                &self.orient_peel_area_exponent_high,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("ORIENT_PEEL_AREA_EXPONENT_LOW"),
                "1",
                SCALAR,
                &self.orient_peel_area_exponent_low,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("ORIENT_PEEL_REFERENCE_AREA_MM2"),
                "mm²",
                SCALAR,
                &self.orient_peel_reference_area_mm2,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("ORIENT_PEEL_REFERENCE_PRESSURE_PA_HIGH"),
                "Pa",
                SCALAR,
                &self.orient_peel_reference_pressure_pa_high,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("ORIENT_PEEL_REFERENCE_PRESSURE_PA_LOW"),
                "Pa",
                SCALAR,
                &self.orient_peel_reference_pressure_pa_low,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("P8_CHORD_CANON_MICRON"),
                "µm",
                SCALAR,
                &self.p8_chord_canon_micron,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("P8_POCKET_VOXEL_MM"),
                "mm",
                SCALAR,
                &self.p8_pocket_voxel_mm,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("PIXEL_PITCH_X_UM"),
                "µm",
                SCALAR,
                &self.pixel_pitch_x_um,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("PIXEL_PITCH_Y_UM"),
                "µm",
                SCALAR,
                &self.pixel_pitch_y_um,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("PRINT_FLOOR_CHORD_MM"),
                "mm",
                SCALAR,
                &self.print_floor_chord_mm,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("SATURN_BUILD_VOLUME_MM"),
                "mm",
                "record{x,y,z}",
                &self.saturn_build_volume_mm,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("SATURN_LEADING_RELEASE_EDGE"),
                "enum",
                "y_direction",
                &self.saturn_leading_release_edge,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("SATURN_RELEASE_FILM_THICKNESS_MM"),
                "mm",
                SCALAR,
                &self.saturn_release_film_thickness_mm,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("SATURN_RELEASE_FRONT_SWEEP_DIRECTION"),
                "enum",
                "y_direction",
                &self.saturn_release_front_sweep_direction,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT"),
                "count/unit",
                SCALAR,
                &self.support_gate_max_work_per_batch_unit,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED"),
                "count/unit",
                SCALAR,
                &self.support_gate_work_per_batch_unit_observed,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("SUPPORT_KNN_MAX_WORK_PER_FACE"),
                "count/face",
                SCALAR,
                &self.support_knn_max_work_per_face,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("SUPPORT_KNN_WORK_PER_FACE_OBSERVED"),
                "count/face",
                SCALAR,
                &self.support_knn_work_per_face_observed,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT"),
                "count/unit",
                SCALAR,
                &self.support_load_max_work_per_batch_unit,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED"),
                "count/unit",
                SCALAR,
                &self.support_load_work_per_batch_unit_observed,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE"),
                "B/face",
                SCALAR,
                &self.support_parity_grid_max_bytes_per_face,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("SUPPORT_PARITY_GRID_REFS_PER_FACE_OBSERVED"),
                "count/face",
                SCALAR,
                &self.support_parity_grid_refs_per_face_observed,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("TIP_DIAMETER_MIN_ALLOWED_MM"),
                "mm",
                SCALAR,
                &self.tip_diameter_min_allowed_mm,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("TIP_RADIUS_MAX_GROWN_SCAR_MM"),
                "mm",
                SCALAR,
                &self.tip_radius_max_grown_scar_mm,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("TIP_RADIUS_MIN_ALLOWED_MM"),
                "mm",
                SCALAR,
                &self.tip_radius_min_allowed_mm,
            )?,
            ConstantDescriptor::from_derived(
                ConstantId::from("X_ALLOWED"),
                "1",
                SCALAR,
                &self.x_allowed,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("X_STAR_CONE"),
                "1",
                "interval",
                &self.x_star_cone,
            )?,
            ConstantDescriptor::from_measured(
                ConstantId::from("X_STAR_CYLINDER"),
                "1",
                "interval",
                &self.x_star_cylinder,
            )?,
        ])
    }
}

fn exact_date(value: &str) -> Result<EvidenceDate, ProvenanceError> {
    Ok(EvidenceDate::exact(CalendarDate::new(value)?))
}

fn on_or_before_date(value: &str) -> Result<EvidenceDate, ProvenanceError> {
    Ok(EvidenceDate::on_or_before(CalendarDate::new(value)?))
}
