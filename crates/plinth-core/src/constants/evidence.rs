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
    support_knn_max_work_per_face: Derived<u64>,
    support_knn_work_per_face_observed: Measured<u64>,
    support_parity_grid_max_bytes_per_face: Derived<u64>,
    support_parity_grid_refs_per_face_observed: Measured<f64>,
    tip_diameter_min_allowed_mm: Measured<f64>,
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
            "docs/reference/plate-06-results.md",
            "§5 Finding 1",
            plate_6_date.clone(),
        )?;
        let release_front_source = EvidenceSource::project_record(
            EvidenceId::new("saturn4-ultra-release-front-plate6")?,
            "docs/reference/plate-06-results.md",
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
            "docs/reference/acceptance-run-2026-07-24-contactfix.md",
            "Provenance and What moved",
            exact_date("2026-07-24")?,
        )?;

        let load_sharing_value = LoadSharing::additive(6.0)?;
        let load_sharing = Measured::new(
            load_sharing_value,
            plate_6_source,
            None,
            "Plate 6 validated one cap area (30 mm²), one arm diameter (0.798 mm), and one arm length (4.00 mm). No strip failed, so there is no measured upper bracket; N > 6 is extrapolation. The fin root contributes about 19.75% section bonus, so the result is at least N× and never evidence for super-additive sharing. X_tot is algebraically degenerate with total arm area, and the ratio rests on plate 5. The tight N=2 flip, X_tot 30.077 alive versus 30.481 dead, is a 1.33% window and does not prove exact additivity. Plate 6 §5 Finding 3 records survival on both axes at X_arm=60.15, excluding zipper collapse at that load while leaving zero-variance and near-boundary axis effects untested; the four X-axis cells were not zipper tests. docs/reference/plate-06-results.md §6 records the B6 override: re-scoping a preregistered void after seeing data is exactly what preregistration blocks, making these points post-hoc and leaving that counterargument unanswered.",
        )?;
        let n_eff_cap_value = load_sharing_value
            .n_eff_cap()
            .ok_or(M0EvidenceConstantsError::NonAdditiveLoadSharing)?;

        let tip_diameter_min_allowed_mm = Measured::new(
            0.150,
            EvidenceSource::owner_decision(
                EvidenceId::new("tip-diameter-floor-owner-decision")?,
                "docs/rust-migration/03-physics-and-constants.md",
                "§3.2",
                on_or_before_date("2026-07-31")?,
            )?,
            None,
            "Owner-authorized hard validator floor for this machine and resin; it is not a target or a cap.",
        )?;
        let tip_radius_value = *tip_diameter_min_allowed_mm.value() / 2.0;

        let x_star_cone = Measured::new(
            FiniteInterval::new(25.00, 25.34, BoundKind::Inclusive, BoundKind::Inclusive)?,
            EvidenceSource::calibration_plate(
                PlateId::from("COUPON-PLATE-5-compliant-vs-stiff"),
                "docs/reference/plate-05-results.md",
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
            "docs/rust-migration/support-resource-caps-2026-08-05.md",
            "§2 Measured requirement",
            exact_date("2026-08-05")?,
        )?;

        Ok(Self {
            cco_x_pair_load_discount: Derived::new(
                0.840_896_415_253_714_5,
                std::iter::empty::<&str>(),
                "Proof-only fourth-root relation: (F/2)^0.25/F^0.25 = 0.5^0.25. The proof is in docs/reference/physics_transient_peel.md and the relation is implemented in both cco.py and bracing.py.",
            ),
            layer_height_default_mm: Measured::new(
                0.050,
                EvidenceSource::project_record(
                    EvidenceId::new("saturn4-ultra-layer-profile-plate6")?,
                    "docs/reference/plate-06-results.md",
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
                    "docs/specs/SPEC-P8-RELOCATE.md",
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
                    "docs/specs/SPEC-P8-RELOCATE.md",
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
            pixel_pitch_y_um: Measured::new(
                19.0,
                vendor_source.clone(),
                None,
                "The vendor's second pitch maps to project Y; retrieval date 2026-07-31 is metadata, while 2025-01-22 is the evidence date.",
            )?,
            print_floor_chord_mm: Measured::new(
                0.08,
                EvidenceSource::owner_decision(
                    EvidenceId::new("print-floor-owner-decision-2026-07-24")?,
                    "docs/specs/SPEC-EMBED-POLICY.md",
                    "PRINT-FLOOR amendment and Normative constants",
                    exact_date("2026-07-24")?,
                )?,
                None,
                "Owner-ratified print-floor chord below which a rule-4 unanchored transit is moot.",
            )?,
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
                    "docs/rust-migration/03-physics-and-constants.md",
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
            support_knn_max_work_per_face: Derived::new(
                2_048,
                ["SUPPORT_KNN_WORK_PER_FACE_OBSERVED"],
                "Cap coefficient by the single support resource-cap rule: next_power_of_two(2 × observed) = next_power_of_two(2 × 696) = next_power_of_two(1392) = 2048. It bounds kNN work at a constant multiple of the support mesh's own face count, so an all-pairs (face²) batch is still refused for any mesh above 2048 faces. Single-specimen basis: its one input was observed on the Weeping Treant v1.2 alone (429,120 faces, 2026-08-05), so this coefficient is n = 1. A second large-model measurement triggers re-derivation over both specimens; it must never be inherited silently as a treant-shaped constant. It scales exactly one batch, the perception minima-to-face kNN — the specialized coverage-route kNN batch shares the KnnBatchLimits shape but has no measurement of record and stays at SUPPORT_KNN_MAX_WORK_FLOOR.",
            ),
            support_knn_work_per_face_observed: Measured::new(
                696,
                treant_cap_source.clone(),
                None,
                "Weeping Treant v1.2, 429,120 support faces: the minima-to-face kNN batch planned source_count × query_count = 298,667,520 work units, exactly 429,120 × 696, so the batch costs 696 work units per face. Single-specimen basis: the Weeping Treant v1.2 alone, 429,120 faces, 2026-08-05. It is the only corpus model above 4,760 faces, so this observation has no second specimen and no spread; a second large-model measurement triggers re-derivation of SUPPORT_KNN_MAX_WORK_PER_FACE rather than silent inheritance of a treant-shaped constant.",
            )?,
            support_parity_grid_max_bytes_per_face: Derived::new(
                512,
                ["SUPPORT_PARITY_GRID_REFS_PER_FACE_OBSERVED"],
                "Cap coefficient by the single support resource-cap rule applied to the exact parity-grid projection 81·faces + 36·references + 4 bytes (rust/geom_kernel/src/parity.rs axis_memory_bounds): observed bytes per face = 81 + 36 × 2.8303132 = 182.8913, and next_power_of_two(2 × 182.8913) = next_power_of_two(366) = 512. The 4.2× margin this leaves over the observation (11.97 admitted refs/face against 2.83 observed) is wider than the kNN coefficient's 1.7× and is ratified as such: one uniform rule for both coefficients is worth more than a tighter grid bound, and refs/face is the quantity measured to vary 526 ↔ 2.83 across the corpus. Single-specimen basis: its one input was observed on the Weeping Treant v1.2 alone (429,120 faces, 2026-08-05), so this coefficient is n = 1. A second large-model measurement triggers re-derivation over both specimens; it must never be inherited silently as a treant-shaped constant.",
            ),
            support_parity_grid_refs_per_face_observed: Measured::new(
                1_214_544.0 / 429_120.0,
                treant_cap_source,
                None,
                "Weeping Treant v1.2, 429,120 support faces: the axis parity grid projected a peak of 78,482,308 bytes, which inverts through 81·faces + 36·references + 4 to exactly 1,214,544 (cell, face) references. References, not faces, dominate that projection, and their ratio to faces falls as a mesh refines — 526 per face on the 244-face Cylinder against 2.83 here — so the treant is the tight case and the byte floor governs every coarse model. Single-specimen basis: the Weeping Treant v1.2 alone, 429,120 faces, 2026-08-05. The 526 ↔ 2.83 spread is the reason that matters: this quantity is measured to vary by 186× across the corpus, and one point on a curve that steep is not a distribution. A second large-model measurement triggers re-derivation of SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE rather than silent inheritance of a treant-shaped constant.",
            )?,
            tip_diameter_min_allowed_mm,
            tip_radius_min_allowed_mm: Derived::new(
                tip_radius_value,
                ["TIP_DIAMETER_MIN_ALLOWED_MM"],
                "Exact diameter-to-radius conversion: TIP_DIAMETER_MIN_ALLOWED_MM / 2 = 0.075 mm; do not substitute the 0.200 mm default.",
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
                    "docs/reference/plate-06-results.md",
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
    pub fn descriptors(&self) -> Result<[ConstantDescriptor; 24], RegistryError> {
        Ok([
            ConstantDescriptor::from_derived(
                ConstantId::from("CCO_X_PAIR_LOAD_DISCOUNT"),
                "1",
                SCALAR,
                &self.cco_x_pair_load_discount,
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
