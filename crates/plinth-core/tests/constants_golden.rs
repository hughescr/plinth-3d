use plinth_core::{m0_constants, ConstantId, RegistryProvenanceClass};
use std::fs;
use std::path::Path;

// This is a hand-maintained cross-component inventory. It deliberately does
// not read the source ledger or the generated Markdown golden.
const EXPECTED_CONSTANT_IDS: [&str; 207] = [
    "A_ABS_MM2",
    "BALL_BREACH_EPS_MM",
    "BALL_FACTOR_FOOT",
    "BALL_FACTOR_STUB",
    "BRACE_LADDER_Z_MM",
    "BRACE_NEIGHBOR_MAX_XY_MM",
    "BRACE_REACH_MAX_FACTOR_X",
    "BRACE_R_MM",
    "BRACING_FALLBACK_FRACTIONS",
    "BRACING_MAX_STACKED_FALLBACKS",
    "BRACING_MIN_CROSS_LINK_XY_MM",
    "BRACING_MODEL_RUNG_MAX_CANDIDATES",
    "BRACING_MODEL_RUNG_MIN_LENGTH_MM",
    "BRACING_MODEL_RUNG_Z_TOL_MM",
    "BRACING_REACH_TIER_BASE_FACTOR",
    "BRANCH_ANGLE_DEG",
    "BRIDGE_FREE_GAP_MM",
    "BRIDGE_SAG_SPAN_APPROX_MM",
    "CCO_BIFURCATION_HOSTS",
    "CCO_CONE_TOLERANCE_DEG",
    "CCO_FERMAT_MAX_ITERATIONS",
    "CCO_MIN_SPLIT_LENGTH_MM",
    "CCO_SPLIT_SLIDE_POLICY",
    "CCO_XIA_ALPHA",
    "CCO_X_PAIR_LOAD_DISCOUNT",
    "CHITUBOX_DEFAULT_PLACEMENT_XY_MM",
    "CHITUBOX_EMIT_N_SIDES",
    "CHITUBOX_PAD_DIAMETER_MM",
    "CHITUBOX_PAD_N_SIDES",
    "CHITUBOX_PLATE_MESH_TOL_MM",
    "CHITUBOX_RAFT_HEIGHT_MM",
    "CHITUBOX_ROOT_HEIGHT_MM",
    "CHITUBOX_ROOT_LAYERS",
    "CHITUBOX_SPHERE_MESH_TESSELLATION_POLICY",
    "CLEARANCE_SAMPLE_STEP_MM",
    "COINCIDE_TOL_MM",
    "COLINEAR_ANGLE_DEG",
    "COLINEAR_RADIUS_FRAC",
    "CONNECTIVITY_AXIS_ATTACH_TOL_MM",
    "CONNECTIVITY_EMBED_PAD_MM",
    "COVERAGE_RADIUS_FACTOR",
    "COVERAGE_REPAIR_MAX_ATTEMPTS",
    "DECIMATION_ADAPTATION_POLICY",
    "DECIMATION_FEATURE_HALO_MM",
    "DECIMATION_FEATURE_PROBE_RATIO",
    "DECIMATION_P999_FACTOR",
    "DECIMATION_P999_SAMPLE_POLICY",
    "DECIMATION_P99_FACTOR",
    "DECIMATION_P99_SAMPLE_POLICY",
    "DECIMATION_PROTECTED_CAP_FRACTION",
    "DECIMATION_RATIO_SEARCH",
    "DECIMATION_RECHECK_SLACK",
    "DECIMATION_SCALE_ACCEPTANCE_MM",
    "DECIMATION_SEED_ERROR_MM",
    "DECIMATION_VERTEX_GROUP_FACTOR",
    "DEFAULT_CONTACT_DEPTH_MM",
    "DELICACY_CURVATURE_WEIGHT",
    "DELICACY_THINNESS_WEIGHT",
    "EMIT_REQUIRE_PASSED_GATES_DEFAULT",
    "ENCLOSURE_PROOF_TOL_MM",
    "ENCLOSURE_TOL_MM",
    "ENGINE_SURFACE_TREE_MAX_POINTS",
    "ENGINE_SURFACE_TREE_PITCH_MM",
    "EPS_COS",
    "EXPECTED_CHITUBOX_API_VERSION",
    "FLARE_LEN_MM",
    "FLARE_R_FACTOR",
    "GATE_ON_BALL_BREACH_DEFAULT",
    "GATE_ON_REMOVAL_DEFAULT",
    "GRIP_CLEAR_MM",
    "GRIP_HANDLE_LEN_MM",
    "GRIP_HANDLE_R_MM",
    "HOLLOW_ALLOW_OUTPUT_DEFAULT",
    "HOLLOW_BREATHING_VOXEL_PITCH_MM",
    "HOLLOW_BULK_MIN_MM",
    "HOLLOW_CONTAINMENT_MIN_FRACTION",
    "HOLLOW_CONTAINMENT_SAMPLES",
    "HOLLOW_DRAIN_DIRECTIONS",
    "HOLLOW_DRAIN_R_MM",
    "HOLLOW_EXIT_RAY_MAX_MM",
    "HOLLOW_EXTRA_DRAIN_POLICY",
    "HOLLOW_LOBE_BAND_MM",
    "HOLLOW_LOBE_MIN_ML",
    "HOLLOW_ORIENT_ENABLE_DEFAULT",
    "HOLLOW_PITCH_MM",
    "HOLLOW_SAVE_MIN_FRAC",
    "HOLLOW_SAVE_MIN_ML",
    "HOLLOW_SHARD_MAX_DIM_MM",
    "HOLLOW_SHARD_MAX_FACES",
    "HOLLOW_SHARD_MAX_VOLUME_MM3",
    "HOLLOW_VENT_R_MM",
    "HOLLOW_VOLUME_BALANCE_REL_TOL",
    "HOLLOW_WALL_DEFAULT_MM",
    "ISLAND_ATTACH_REACH_MM",
    "ISLAND_BIRTH_LINK_R_MM",
    "ISLAND_MERGE_RADIUS_MM",
    "JOINT_BALL_FACTOR",
    "KAPPA",
    "LAYER_HEIGHT_DEFAULT_MM",
    "LIFT_MM",
    "LIFT_TOL_MM",
    "LOAD_SHARING",
    "L_SCAR_MAX_MM",
    "MATERIAL_COST_PER_MM",
    "MAX_DEFLECTION_MM",
    "MAX_SEGMENT_ANGLE_DEG",
    "MAX_TRUNK_R_MM",
    "MAX_UNBRACED_SHAFT_R_MM",
    "MINIMA_BIRTH_MATCH_LAYERS",
    "MINIMA_CONNECT_LAYERS",
    "MINIMA_CONTACT_RADIUS_MM",
    "MINIMA_DROP_INTERIOR_FACES",
    "MINIMA_EPS_MM",
    "MINIMA_REQUIRE_ISLAND_BIRTH",
    "MINIMA_SUPPORT_AREA_TIPS",
    "MIN_CONTACTS_PER_ISLAND",
    "MIN_ILLEGAL_LEN_MM",
    "MIN_SHAFT_R_MM",
    "MIN_SHOOT_R_MM",
    "MIN_TRUNK_R_MM",
    "MODEL_BRACE_MAX_LEN_MM",
    "MODULUS_GREEN_GPA",
    "MURRAY_TOLERANCE",
    "NEAR_PLATE_MM",
    "N_EFF_CAP",
    "ON_AXIS_TOL_MM",
    "ORIENT_SCAR_WEIGHT",
    "OVERHANG_ANGLE_DEG",
    "P8_BALL_SLIDE_MAX_MM",
    "P8_BALL_SLIDE_STEP_MM",
    "P8_CHORD_CANON_MICRON",
    "P8_JUNCTION_RESEAT_ENABLE",
    "P8_JUNCTION_RESEAT_MAX_MM",
    "P8_JUNCTION_RESEAT_STEP_MM",
    "P8_MAX_RELOCATE_ROUNDS",
    "P8_MAX_RELOCATE_SWEEPS",
    "P8_MAX_UNRESOLVED",
    "P8_MAX_UNRESOLVED_WEIGHT",
    "P8_POCKET_CORRIDOR_PAD_MM",
    "P8_POCKET_MIN_MEMBERS",
    "P8_POCKET_VOXEL_MM",
    "P8_REAIM_AZIMUTH_DEG",
    "P8_REAIM_LENGTH_FACTORS",
    "P8_REAIM_POLAR_DEG",
    "P8_REAIM_ROUTE_GROWTH_MAX_FACTOR",
    "P8_REAIM_VOLUME_DELTA_MAX_ML",
    "P8_RELOCATE_ENABLE",
    "P8_RESEAT_ENABLE",
    "P8_RESEAT_MAX_SEEDS",
    "P8_RESEAT_RADIUS_MM",
    "P8_RESIN_DELTA_MAX",
    "P8_RESIZE_FIXED_POINT_MAX_ROUNDS",
    "P8_SITE_COMMIT_BUDGET",
    "P8_STRUT_GROWTH_MAX_FACTOR",
    "PEEL_ELEVATIONS_DEG",
    "PEEL_PRESSURE_PA",
    "PERF_ANCHOR_CAP",
    "PERF_BEAM_K",
    "PERF_FIX_ITER_CAPS",
    "PERF_SEED_SPACING_FACTOR",
    "PIPELINE_HOLLOW_ENABLE_DEFAULT",
    "PIPELINE_P8_RELOCATE_DEFAULT",
    "PIXEL_PITCH_X_UM",
    "PIXEL_PITCH_Y_UM",
    "PLATE_BAND_MM",
    "PRINT_FLOOR_CHORD_MM",
    "PULL_DIST_MM",
    "REWIRE_DEGENERATE_LEN_MM",
    "REWIRE_MAX_SLIDE_EVALS",
    "REWIRE_ROLL_EPS",
    "REWIRE_ROLL_WINDOW",
    "REWIRE_TWIG_MERGE_LEN_MM",
    "REWIRE_WORK_BUDGET",
    "SATURN_BUILD_VOLUME_MM",
    "SATURN_LEADING_RELEASE_EDGE",
    "SATURN_RELEASE_FILM_THICKNESS_MM",
    "SATURN_RELEASE_FRONT_SWEEP_DIRECTION",
    "SCAR_COST_WEIGHT",
    "SPACING_FLOOR_MM",
    "STANDOFF_MM",
    "STRUT_FALLBACK_MAX_ANGLE_DEG",
    "STRUT_MAX_LENGTH_MM",
    "STRUT_MIN_LENGTH_SAVED_MM",
    "STRUT_MODEL_ANCHOR_MAX_CANDIDATES",
    "STRUT_MODEL_ANCHOR_MAX_DELICACY",
    "STRUT_MODEL_ANCHOR_MIN_LEN_MM",
    "STUB_LENGTH_MM",
    "SURFACE_TREE_MAX_POINTS",
    "SURFACE_TREE_PITCH_MM",
    "SURFACE_TREE_SEED",
    "TARGET_SPACING_MM",
    "TENSILE_GREEN_MPA",
    "THIN_FEATURE_MM",
    "TIP_DIAMETER_MIN_ALLOWED_MM",
    "TIP_RADIUS_DEFAULT_MM",
    "TIP_RADIUS_MAX_LEGACY_MM",
    "TIP_RADIUS_MIN_ALLOWED_MM",
    "TIP_SAFETY_FACTOR",
    "TOL_MM",
    "TRANSIENT_SHOOT_COUNT_EXPONENT",
    "TRUNK_SEARCH_DIRECTIONS",
    "TRUNK_SEARCH_RADII_MM",
    "TWIN_SINGLE_BRACE_XY_MM",
    "VOXEL_PITCH_MM",
    "X_ALLOWED",
    "X_STAR_CONE",
    "X_STAR_CYLINDER",
];

fn golden_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs/rust-migration/constants.md")
}

#[test]
fn global_registry_is_one_complete_atomic_207_constant_publication() {
    let first = m0_constants().expect("M0 constants registry must construct");
    let second = m0_constants().expect("M0 constants registry must construct");
    assert!(
        std::ptr::eq(first, second),
        "global registry must be initialized once"
    );

    let registry = first.registry();
    assert_eq!(registry.len(), EXPECTED_CONSTANT_IDS.len());
    let actual_ids = registry
        .iter()
        .map(|descriptor| descriptor.id().as_str())
        .collect::<Vec<_>>();
    assert_eq!(actual_ids, EXPECTED_CONSTANT_IDS);
    for id in EXPECTED_CONSTANT_IDS {
        assert!(
            registry.get(&ConstantId::from(id)).is_some(),
            "global registry omitted independently expected constant {id}"
        );
    }

    let classes = registry
        .iter()
        .fold([0_usize; 4], |mut counts, descriptor| {
            match descriptor.provenance().class() {
                RegistryProvenanceClass::Measured => counts[0] += 1,
                RegistryProvenanceClass::Derived => counts[1] += 1,
                RegistryProvenanceClass::Placeholder => counts[2] += 1,
                RegistryProvenanceClass::Pending => counts[3] += 1,
            }
            counts
        });
    assert_eq!(classes, [16, 4, 187, 0]);
}

#[test]
fn saturn4_ultra_profile_is_wired_from_the_typed_global_declarations() {
    let profile = m0_constants()
        .expect("M0 constants registry must construct")
        .saturn4_ultra();
    assert_eq!(profile.id().as_str(), "saturn4-ultra");
    assert_eq!(
        (
            profile.build_volume_mm().value().x(),
            profile.build_volume_mm().value().y(),
            profile.build_volume_mm().value().z(),
        ),
        (211.68, 118.37, 220.0),
    );
    assert_eq!(*profile.pixel_pitch_x_um().value(), 14.0);
    assert_eq!(*profile.pixel_pitch_y_um().value(), 19.0);
    assert_eq!(*profile.default_layer_height_mm().value(), 0.050);
    assert_eq!(*profile.lift_default_mm().diagnostic(), 5.0);
    assert_eq!(
        profile.leading_release_edge().value().as_str(),
        "negative_y"
    );
    assert_eq!(
        profile.release_front_sweep_direction().value().as_str(),
        "positive_y"
    );
    assert_eq!(*profile.release_film_thickness_mm().value(), 0.127);
}

#[test]
fn rendered_constants_golden_matches_the_file_audited_fresh_from_disk() {
    let path = golden_path();
    assert!(
        path.is_file(),
        "required constants Markdown golden is missing: {}",
        path.display()
    );
    let on_disk = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "required constants Markdown golden cannot be read at {}: {error}",
            path.display()
        )
    });
    let rendered = m0_constants()
        .expect("M0 constants registry must construct")
        .registry()
        .render_markdown();
    assert_eq!(
        on_disk, rendered,
        "constants Markdown golden is stale; update the producer output, then audit the written file"
    );
}
