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
    decimation_max_absolute_deviation_mm: Derived<f64>,
    decimation_max_p999_deviation_mm: Derived<f64>,
    decimation_max_p99_deviation_mm: Derived<f64>,
    final_clearance_gap_mm: Measured<f64>,
    layer_height_default_mm: Measured<f64>,
    load_sharing: Measured<LoadSharing>,
    minima_drop_interior_faces: Measured<bool>,
    minima_require_island_birth: Measured<bool>,
    n_eff_cap: Derived<f64>,
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

        Ok(Self {
            cco_x_pair_load_discount: Derived::new(
                0.840_896_415_253_714_5,
                std::iter::empty::<&str>(),
                "Proof-only fourth-root relation: (F/2)^0.25/F^0.25 = 0.5^0.25. The proof is in docs/engineering-journal/reference/physics_transient_peel.md and the relation is implemented in both cco.py and bracing.py.",
            ),
            decimation_max_absolute_deviation_mm: Derived::new(
                decimation_max_absolute_deviation_value,
                ["PRINT_FLOOR_CHORD_MM"],
                "Worst-case (max) two-sided deviation a decimated mesh may carry and still be authorized as downstream-useful. Exact identity, not arithmetic: the bound IS the registered print-floor chord re-expressed as a deviation limit, on the rule that a deviation below the smallest chord the process can print cannot produce a printed difference. It exists as a separate declaration because it is a decimation authority with its own re-derivation trigger, and because the search's own p99/p99.9 budget is blind to the worst point: on the Weeping Treant v1.2 at 520 faces removed the measured max is 0.0566357 mm while p99.9 is 0.000073 mm and p99 is exactly 0, a factor of 776 (docs/engineering-journal/decimation-design-2026-08-06.md §8.3). Measured consequence on the one large specimen, recorded so this bound is never mistaken for a permissive one: it admits only (2.125 %, 4.453 %] reduction on that model, and the topology gate independently stops it at ≈0.2 % first, so no corpus model reaches it today. LAYER_HEIGHT_DEFAULT_MM (0.050 mm) was the runner-up and was NOT chosen: the Phase-1 max-vs-ratio curve does not discriminate between them (0.050 mm admits ≈0.12 %), so the choice rests on provenance alone, and a Z-axis layer quantum applied to an XY deviation is the weaker argument.",
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
            layer_height_default_mm: Measured::new(
                0.050,
                EvidenceSource::project_record(
                    EvidenceId::new("saturn4-ultra-layer-profile-plate6")?,
                    "docs/engineering-journal/reference/plate-06-results.md",
                    "§1 Layer height",
                    exact_date("2026-07-30")?,
                )?,
                None,
                "Plate 6 project profile layer height; this is not a vendor default.",
            )?,
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
            pixel_pitch_x_um: Measured::new(
                14.0,
                vendor_source.clone(),
                None,
                "The vendor's first pitch maps to project X; retrieval date 2026-07-31 is metadata, while 2025-01-22 is the evidence date.",
            )?,
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
                "Cap coefficient by the single support resource-cap rule: next_power_of_two(2 × observed) = next_power_of_two(2 × 23) = next_power_of_two(46) = 64. The batch unit is one (coverage_samples + 1) × (contacts + 1) product element, which is the shape of the specialized gate's own routing scan, so the coefficient bounds gate work at a constant multiple of the batch the gate was handed. It does NOT scale with face count, and deliberately so: 24-face make_raking_blade charges 612,887,212 gate operations where 429,120-face Weeping Treant charges 12,714,828,544, a 20× spread against an 18,000× face spread. What the coefficient still refuses is the gate's proposal-variant re-evaluation, which is quadratic in the batch rather than linear in it: the treant's 39,223-proposal merge gate plans 21,755,124,103,146 operations on the same 553,835,949-unit batch, 613× this coefficient, and is refused (FU-8c). Single-specimen basis: its one input was observed on the Weeping Treant v1.2 alone (429,120 faces, 2026-08-06), so this coefficient is n = 1. A second large-model measurement triggers re-derivation over both specimens; it must never be inherited silently as a treant-shaped constant. Addendum 2026-08-06 (FU-8c closed at a3308ec) — the sentence above is left standing and is now past tense: the mechanism it describes is gone. The gate no longer re-evaluates the contact set once per merge proposal (GateBatchIndex plus per-proposal route-key deltas), and gate_resource_plan is repriced to (samples + demands + 1) × contacts + measured proposal work. The treant's stated demand on that same 553,835,949-unit batch fell 108×, from 21,755,124,103,146 to 201,480,535,329, against this UNCHANGED cap and its unchanged 35,445,500,736 materialization — no cap, floor or coefficient was touched. The coefficient's verdict is unchanged: 201,480,535,329 is 363.8× this coefficient's linear budget on that batch, so the treant is STILL REFUSED at this gate, 5.68× over. 363.8 is no more adoptable as a coefficient than 613 was, and for the same reason: it would take the Cylinder (already at 89.6 % of the floor) and most of the corpus off their floors. FU-8c is closed on the defect it named, not on the treant reaching a seal; the residual 196,264,386,588 (97.4 % of the demand) is per-proposal sources × samples work on the perturbed route key and is opened as FU-8d. FU-23 re-derivation trigger, re-pointed to match the follow-ups ledger: this coefficient and SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED are to be re-derived after FU-8d lands, not after FU-8c — FU-8c has landed and the observed input was NOT re-derived, because the gate's charged work is still dominated by the residual FU-8d names. docs/engineering-journal/CURRENT-STATUS.md's follow-ups ledger is the authority on that ordering. Addendum 2026-08-06 (FU-8d phase F2) — everything above is left standing; the demand it prices moved again and this coefficient again did not. F2 answers a merge proposal's nearest from the baseline sweep's retained witnesses plus ONE merged-seat kNN batch, so on the same 553,835,949-unit batch the stated demand fell a further 27.6×, from 201,480,535,329 to 7,299,316,559 — 13.2× this coefficient's linear budget, and 20.6 % of the unchanged 35,445,500,736 materialization, so gate_resource_plan now PASSES. No cap, floor or coefficient was touched by F2 either. The treant is still refused at this gate, one site later: variant_specialized_owner_face_count's descriptor budget crosses at variant 275 of 39,224, required 35,445,618,202 against limit 35,445,500,736. That residual is FU-8d part (2), the per-variant load/owner-face path, and phase F4 is what addresses it. Ruling R-f stands and is NOT yet satisfied: this coefficient and SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED are to be re-derived only after FU-8d phase F3 lands AND only from a treant run that COMPLETES the gate. F2's run does not complete it, so nothing was re-derived here. Addendum 2026-08-06 (FU-8d phases F3 and F4) — everything above is left standing, and the clause that has stopped being true is \"the treant is still refused at this gate\". F3 finalizes the load partition once per batch instead of once per variant, deletes the per-variant window matrices, collapses four per-proposal planning passes into one behind a count-derived pre-check, and replaces the gate's hand-built variants × faces load model with what specialized_load_resource_plan itself reports. The descriptor scan that crossed at variant 275 falls to 136,791,451 charged once and unique_face_finalizer_work ceases to exist as a term, while the honest load figure (6,592,813,742) is 1.43× the 4,618,272,984 model it replaced — so the stated demand ROSE, to 9,273,857,317, and that rise is a quadratic repriced honestly rather than a bound relaxed. That is 16.7× this coefficient's linear budget on the same 553,835,949-unit batch and 26.2 % of the unchanged 35,445,500,736 materialization; exact_operations is 10,457,910,996 (29.5 %). The batch is ADMITTED and EXECUTES: the treant evaluates 39,224 variants, accepts a merge and opens the next. No cap, floor or coefficient was touched by F3 or F4 either. Ruling R-f is STILL NOT satisfied and this coefficient is STILL NOT re-derived: F4's clean run at the F3 tip — PLINTH_THREADS=4, RSS watchdog armed at 90 GB and never fired — is an EXCEEDS-TIMEBOX at 600 s, so the treant never completes the gate stage and there is no completed-gate measurement to re-derive from. What consumes the 600 s was sampled directly rather than inferred, and is opened as FU-8e: the per-proposal fill_proposal_nearest_delta, which rebuilds a PointCloudIndex kd-tree over a ONE-point cloud once per proposal and is dominated by allocator traffic, plus the per-variant assess_coverage second behind it. Neither is a cap to raise. On the 12,714,828,544 this entry quotes as the treant's proposal-free gate charge, see the ruling note on SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED: F3's repricing plausibly supersedes that figure as an input, no re-measurement of it exists at this tip, and R-f blocks re-derivation of either constant regardless. Addendum 2026-08-07 (FU-8e phases E1-E4) — everything above is left standing, and this addendum exists to record that nothing in it moved. FU-8e removed the merge gate's per-proposal kd-tree construction (a one-source kNN fast path in the kernel, argued bit-identical to the tree's answer in five clauses and pinned on f64::to_bits) and its per-variant assess_coverage (a variant whose rewritten rows cannot have moved the coverage objective is handed the batch baseline's objective). The demand this coefficient prices is UNCHANGED at 9,273,857,317 against the unchanged 35,445,500,736 materialization — 16.7x this coefficient's linear budget on the same 553,835,949-unit batch — because plan_proposal_nearest_delta was deliberately not touched: work() is still 1 x touched_rows, and the treant's touched_rows re-measured at 1,041,544,686, to the row. No cap, floor or coefficient was touched by E1, E2, E3 or E4. Two mechanism claims in the addendum above are corrected rather than rewritten: the kd-trees were FREE (one one-point PointCloudIndex costs 83.3 ns, so 39,223 of them total about 3.27 ms per batch), and what cost was the query loop behind them, about 1.04e9 query iterations and 2.08e9 heap allocations per batch; assess_coverage's cost was likewise not allocation churn but two unconditional full scans of the nearest array per variant, about 254 GB of reads per batch. Measured 2026-08-07 under ruling R-g: the merge round falls 63.83 s to 14.68 s (4.35x), coverage per variant 17.05 s to 0.001 s, zero coverage fallbacks across 33 rounds and about 1.29 million proposal variants, peak RSS 2.446 GiB, watchdog armed at 90 GB and never fired, no typed refusal at any cap. Ruling R-f is STILL NOT satisfied and this coefficient is STILL NOT re-derived: the run is an EXCEEDS-TIMEBOX at 601 s having completed 33 merge rounds, and the tournament commits exactly one merge per round, so at most 1,365 rounds by the contact bound and about 460 at make_raking_blade's completion ratio put the gate stage at roughly 1.8 to 5.5 hours — 11x to 33x the timebox. The successor is opened as FU-8f (lazy first-safe gate acceptance plus non-incremental batch preparation), recorded as a finding AWAITING OWNER RULING and deliberately not built, because lazy acceptance severs charged-batch from executed-batch and would re-found GateRoutePlanMismatch a second time after F3. Arithmetic corrected 2026-08-07 (records fix): the kd-tree construction total in the addendum above was first written as 16.7 microseconds, which was an arithmetic slip in the design pass and not a second measurement -- 39,223 x 83.3 ns = 3.267 ms, and 16.7 microseconds across 39,223 builds would have implied 0.426 ns per construction. The per-build 83.3 ns is RETAINED as the design pass's measurement of record; only the product is corrected, and the conclusion it was quoted for is unchanged, because 3.27 ms is 0.009 % of the 37.33 s fill_proposal_nearest_delta frame: construction stays negligible and the fix still targets the query loop. No measured quantity, cap, floor or coefficient moves. See docs/engineering-journal/support-resource-caps-2026-08-05.md section 12. Addendum 2026-08-07 (FU-8f phase G3) — everything above is left standing, and this addendum exists to record that nothing this coefficient prices moved, plus the two clauses above that have stopped being true. The demand is UNCHANGED at 9,273,857,317 against the unchanged 35,445,500,736 materialization — 16.7x this coefficient's linear budget on the same 553,835,949-unit batch — because gate_resource_plan still prices plan.variants = proposals + 1, the exact_operations fold still folds all 39,224, and the refusal check still runs BEFORE the variant loop. G3 changes EXECUTION, not charge. First stopped clause: \"The batch is ADMITTED and EXECUTES: the treant evaluates 39,224 variants, accepts a merge and opens the next\" now reads — the batch is CHARGED for 39,224 variants and EXECUTES a PREFIX of them, stopping at the first variant that is gate_not_worse than the batch baseline, which was proposal 0 in all 28 rounds ever observed across two models. The executed prefix is metered as GateBatchOutcome::executed_variant_count and reported as SpecializedGateEvidence::evaluated_variant_count, beside gathered_pair_count (renamed from evaluated_pair_count, which had always counted proposals GATHERED rather than evaluated). Second stopped clause: FU-8f is no longer \"a finding AWAITING OWNER RULING and deliberately not built\" — the ruling came (Craig, 2026-08-07) and G3 built it, re-founding GateRoutePlanMismatch a SECOND time on a prefix-wise comparison: the batch executed a prefix of the work it planned, the first executed_variant_count variants, in plan order, each exactly once, at the peak the plan stated. Charged and executed are severed and BOTH are stated; charging the prefix was considered and refused, because a bound not knowable until after the work that determines it bounds nothing. No cap, floor or coefficient was touched by G3, and all seven frozen OD-8 digests are byte-identical at its tip. Ruling R-f is STILL NOT satisfied and this coefficient is STILL NOT re-derived: a treant run at the G3 tip, on a QUIET machine with no concurrent workflow — unlike every measurement quoted above — is an EXCEEDS-TIMEBOX at 600 s in both permitted attempts (peak RSS 3,479,600 KB, watchdog armed at 90 GB and never fired), so no run has yet COMPLETED the gate. Counting the tournament's rounds is FU-8f phase G4's deliverable. See docs/engineering-journal/support-resource-caps-2026-08-05.md section 13. Addendum 2026-08-07 (FU-8f phases G4 and G5) — everything above is left standing, THIS COEFFICIENT IS UNCHANGED at 64, and two things in it have moved. First, G4 delivered the round count on a quiet machine: two attempts, EXCEEDS-TIMEBOX at 602 s each, 436 and 430 merge rounds, contacts 1,366 -> 931 and 1,366 -> 937, exactly one merge committed per round throughout, peak RSS 2.554 and 2.528 GiB with the watchdog armed at 90 GB and never fired. So the clause 'Counting the tournament's rounds is phase G4's deliverable' is now past tense, and R-f is still NOT satisfied because neither attempt COMPLETED the gate. Second, G5 repriced the load term and the demand this coefficient prices has FALLEN, by 4,748,065,084: gate_resource_plan's stated demand goes 9,273,857,317 -> 4,525,792,233 and exact_operations goes 10,457,910,996 -> 5,709,845,912, entirely because the gate's load term goes 6,592,813,742 -> 1,844,748,658 (baseline 5,543,831,115 -> 795,766,031, proposal bound 1,048,982,627 unchanged). baseline_rows 554,926,572, demand_rows 42,949,185 and proposal work 2,083,167,818 are byte-identical. That is 4,525,792,233 against the unchanged 35,445,500,736 materialization -- 12.8 % where it was 26.2 %, and 8.2x this coefficient's linear budget on the same 553,835,949-unit batch where it was 16.7x. A falling demand can create no refusal. No cap, floor or coefficient was raised, lowered or touched by G4 or G5, and all seven frozen OD-8 digests are byte-identical at the G5 tip. SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED is STILL NOT re-derived: R-f wants a run that completes the gate, and G5 ran no completion attempt -- its single measurement run was stopped at 260 s having reached the gate stage, which is not a completed tournament. See docs/engineering-journal/support-resource-caps-2026-08-05.md section 15.",
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
    pub fn descriptors(&self) -> Result<[ConstantDescriptor; 33], RegistryError> {
        Ok([
            ConstantDescriptor::from_derived(
                ConstantId::from("CCO_X_PAIR_LOAD_DISCOUNT"),
                "1",
                SCALAR,
                &self.cco_x_pair_load_discount,
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
