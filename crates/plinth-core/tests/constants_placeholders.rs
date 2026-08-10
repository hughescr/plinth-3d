use plinth_core::{
    AnchorCandidateCap, BeamWidth, BoundKind, BridgeSagLimit, CcoSplitSlidePolicy, ConstantValue,
    ConstantValueView, ConstantsRegistry, DecimationAdaptationPolicy, DecimationRatioSearchPolicy,
    DecimationSamplingPolicy, DrainDirection, DrainDirections, FixBraceIterationCap,
    HollowExtraDrainPolicy, M0EvidenceConstants, M0PlaceholderConstants, NonNegativeFinite,
    PlacementXyMm, PositiveFinite, RegistryProvenanceClass, SeedSpacingFactor,
    SphereTessellationPolicy, TrialSetting, UnitFraction,
};
use std::path::Path;

const EXPECTED_PLACEHOLDER_IDS: [&str; 262] = [
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
    "DECIMATION_PROBE_GRID_PITCH_MM",
    "DECIMATION_PROBE_MAX_GRID_BYTES",
    "DECIMATION_PROBE_NEAREST_UPPER_BOUND_MM",
    "DECIMATION_PROTECTED_CAP_FRACTION",
    "DECIMATION_RATIO_SEARCH",
    "DECIMATION_RECHECK_SLACK",
    "DECIMATION_SCALE_ACCEPTANCE_MM",
    "DECIMATION_SEED_ERROR_MM",
    "DECIMATION_UTILITY_MIN_INPUT_FACES",
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
    "HOLLOW_VENT_DIRECTIONS",
    "HOLLOW_VENT_R_MM",
    "HOLLOW_VENT_SAMPLE_DIVISOR",
    "HOLLOW_VENT_TOP_BAND_MM",
    "HOLLOW_VOLUME_BALANCE_REL_TOL",
    "HOLLOW_WALL_DEFAULT_MM",
    "HOLLOW_WALL_HANDLING_HEAVY_MM",
    "INITIAL_AXIS_MAJORITY_CAP",
    "INITIAL_AXIS_MAJORITY_STEP_MM",
    "ISLAND_ATTACH_REACH_MM",
    "ISLAND_BIRTH_LINK_R_MM",
    "ISLAND_MERGE_RADIUS_MM",
    "JOINT_BALL_FACTOR",
    "KAPPA",
    "LIFT_MM",
    "LIFT_TOL_MM",
    "L_SCAR_MAX_MM",
    "MATERIAL_COST_PER_MM",
    "MAX_DEFLECTION_MM",
    "MAX_SEGMENT_ANGLE_DEG",
    "MAX_TRUNK_R_MM",
    "MAX_UNBRACED_SHAFT_R_MM",
    "MINIMA_BIRTH_MATCH_LAYERS",
    "MINIMA_CONNECT_LAYERS",
    "MINIMA_CONTACT_RADIUS_MM",
    "MINIMA_EPS_MM",
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
    "ON_AXIS_TOL_MM",
    "ORIENT_SCAR_WEIGHT",
    "ORIENT_WEIGHT_HEIGHT_MM",
    "ORIENT_WEIGHT_PEAK_XSEC_CM2",
    "ORIENT_WEIGHT_SUPPORT_AREA_CM2",
    "OVERHANG_ANGLE_DEG",
    "P8_BALL_SLIDE_MAX_MM",
    "P8_BALL_SLIDE_STEP_MM",
    "P8_JUNCTION_RESEAT_ENABLE",
    "P8_JUNCTION_RESEAT_MAX_MM",
    "P8_JUNCTION_RESEAT_STEP_MM",
    "P8_MAX_RELOCATE_ROUNDS",
    "P8_MAX_RELOCATE_SWEEPS",
    "P8_MAX_UNRESOLVED",
    "P8_MAX_UNRESOLVED_WEIGHT",
    "P8_POCKET_CORRIDOR_PAD_MM",
    "P8_POCKET_MIN_MEMBERS",
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
    "PLATE_BAND_MM",
    "PULL_DIST_MM",
    "REWIRE_DEGENERATE_LEN_MM",
    "REWIRE_MAX_SLIDE_EVALS",
    "REWIRE_ROLL_EPS",
    "REWIRE_ROLL_WINDOW",
    "REWIRE_TWIG_MERGE_LEN_MM",
    "REWIRE_WORK_BUDGET",
    "SCAR_COST_WEIGHT",
    "SLICE_UNION_MAX_ABS_MM",
    "SLICE_UNION_QUANTUM_MM",
    "SPACING_FLOOR_MM",
    "STANDOFF_MM",
    "STRUT_FALLBACK_MAX_ANGLE_DEG",
    "STRUT_MAX_LENGTH_MM",
    "STRUT_MIN_LENGTH_SAVED_MM",
    "STRUT_MODEL_ANCHOR_MAX_CANDIDATES",
    "STRUT_MODEL_ANCHOR_MAX_DELICACY",
    "STRUT_MODEL_ANCHOR_MIN_LEN_MM",
    "STUB_LENGTH_MM",
    "SUPPORT_BRACE_PENETRATION_MAX_ROWS",
    "SUPPORT_BRACE_PENETRATION_MAX_SCRATCH_BYTES",
    "SUPPORT_BRACE_PENETRATION_MAX_TOTAL_HITS",
    "SUPPORT_BRACE_PENETRATION_MAX_TOTAL_INTERVALS",
    "SUPPORT_BRACE_PENETRATION_MAX_TOTAL_SAMPLES",
    "SUPPORT_BRACE_PENETRATION_MAX_TOTAL_WORK",
    "SUPPORT_CONSTRUCTION_MAX_CONTACTS",
    "SUPPORT_CONSTRUCTION_MAX_LOAD_ROWS",
    "SUPPORT_CONSTRUCTION_MAX_SCRATCH_BYTES",
    "SUPPORT_CONSTRUCTION_MAX_WORK_ITEMS",
    "SUPPORT_KNN_MAX_LOGICAL_PEAK_BYTES",
    "SUPPORT_KNN_MAX_NEIGHBORS",
    "SUPPORT_KNN_MAX_QUERY_POINTS",
    "SUPPORT_KNN_MAX_SOURCE_POINTS",
    "SUPPORT_KNN_MAX_WORK_FLOOR",
    "SUPPORT_PARITY_GRID_MAX_BYTES_FLOOR",
    "SUPPORT_PERCEPTION_MAX_ISLAND_NEIGHBOR_BYTES",
    "SUPPORT_PERCEPTION_MAX_ISLAND_NEIGHBOR_PAIRS",
    "SUPPORT_PERCEPTION_MAX_LOOP_VERTICES",
    "SUPPORT_PERCEPTION_MAX_MINIMA_VERTICES",
    "SUPPORT_PERCEPTION_MAX_OVERLAP_BYTES",
    "SUPPORT_PERCEPTION_MAX_OVERLAP_EDGE_TESTS",
    "SUPPORT_PERCEPTION_MAX_OVERLAP_PAIRS",
    "SUPPORT_PERCEPTION_MAX_OVERLAP_VERTEX_TESTS",
    "SUPPORT_PERCEPTION_MAX_PLANES",
    "SUPPORT_PERCEPTION_MAX_SLICE_INTERSECTIONS",
    "SUPPORT_PERCEPTION_MAX_SLICE_OUTPUT_BYTES",
    "SUPPORT_PERCEPTION_MAX_TRIANGLE_PLANE_TESTS",
    "SUPPORT_PREPARATION_COVERAGE_SAMPLES_MAX_VIRTUAL_CELLS",
    "SUPPORT_PREPARATION_COVERAGE_SCORE_MATRIX_MAX_ENTRIES",
    "SUPPORT_PREPARATION_MAX_PRIMITIVES",
    "SUPPORT_PREPARATION_MAX_PRIMITIVE_BYTES",
    "SUPPORT_RAFT_MAX_BYTES",
    "SUPPORT_RAFT_MAX_CANDIDATES",
    "SUPPORT_RAFT_MAX_PRIMITIVES",
    "SUPPORT_RAFT_MAX_ROOTS",
    "SUPPORT_ROUTE_CCO_MAX_SEAT_EVALUATIONS",
    "SUPPORT_ROUTE_CCO_MAX_SEAT_REFUSAL_ROWS",
    "SUPPORT_SPECIALIZED_MAX_COVERAGE_OPERATIONS",
    "SUPPORT_SPECIALIZED_MAX_COVERAGE_SCRATCH_BYTES",
    "SUPPORT_SPECIALIZED_MAX_GATE_CONTACT_MATRIX_BYTES",
    "SUPPORT_SPECIALIZED_MAX_GATE_OPERATIONS",
    "SUPPORT_SPECIALIZED_MAX_GATE_PEAK_BYTES",
    "SUPPORT_SPECIALIZED_MAX_GATE_VARIANTS",
    "SUPPORT_SPECIALIZED_MAX_GATE_VECTOR_BYTES",
    "SUPPORT_SPECIALIZED_MAX_GATE_WINDOW_VARIANTS",
    "SUPPORT_SPECIALIZED_MAX_LOAD_OPERATIONS",
    "SUPPORT_SPECIALIZED_MAX_LOAD_SCRATCH_BYTES",
    "SUPPORT_SPECIALIZED_MAX_MERGE_COMMIT_BYTES",
    "SUPPORT_SPECIALIZED_MAX_MERGE_DESCRIPTOR_OPERATIONS",
    "SUPPORT_SPECIALIZED_MAX_MERGE_ITERATIONS",
    "SUPPORT_SPECIALIZED_MAX_MERGE_PAIRS",
    "SUPPORT_SPECIALIZED_MAX_MERGE_PROPOSALS",
    "SUPPORT_SPECIALIZED_MAX_MERGE_PROPOSAL_BYTES",
    "SUPPORT_SPECIALIZED_MAX_MERGE_TOTAL_PROPOSALS",
    "SUPPORT_SPECIALIZED_MAX_SATISFACTION_NEIGHBOR_BYTES",
    "SUPPORT_SPECIALIZED_MAX_SATISFACTION_NEIGHBOR_PAIRS",
    "SUPPORT_SPECIALIZED_MAX_SATISFACTION_TESTS",
    "SUPPORT_SPECIALIZED_MAX_SYNTHETIC_NEIGHBOR_BYTES",
    "SUPPORT_SPECIALIZED_MAX_SYNTHETIC_NEIGHBOR_PAIRS",
    "SURFACE_TREE_MAX_POINTS",
    "SURFACE_TREE_PITCH_MM",
    "SURFACE_TREE_SEED",
    "TARGET_SPACING_MM",
    "TENSILE_GREEN_MPA",
    "THIN_FEATURE_MM",
    "TIP_RADIUS_DEFAULT_MM",
    "TIP_RADIUS_MAX_LEGACY_MM",
    "TIP_SAFETY_FACTOR",
    "TOL_MM",
    "TRANSIENT_SHOOT_COUNT_EXPONENT",
    "TRUNK_SEARCH_DIRECTIONS",
    "TRUNK_SEARCH_RADII_MM",
    "TWIN_SINGLE_BRACE_XY_MM",
    "VOXEL_PITCH_MM",
];

struct ExpectedRow {
    id: &'static str,
    value: &'static str,
    unit: &'static str,
    shape: &'static str,
}

macro_rules! rows {
    ($($id:literal, $value:literal, $unit:literal, $shape:literal;)+) => {
        [$(ExpectedRow { id: $id, value: $value, unit: $unit, shape: $shape },)+]
    };
}

static EXPECTED_ROWS: [ExpectedRow; 262] = rows! {
    "A_ABS_MM2", "2.0", "mm²", "scalar";
    "BALL_BREACH_EPS_MM", "0.05", "mm", "scalar";
    "BALL_FACTOR_FOOT", "1.4", "1", "scalar";
    "BALL_FACTOR_STUB", "1.5", "1", "scalar";
    "BRACE_LADDER_Z_MM", "8.0", "mm", "scalar";
    "BRACE_NEIGHBOR_MAX_XY_MM", "15.0", "mm", "scalar";
    "BRACE_REACH_MAX_FACTOR_X", "1.5", "1", "scalar";
    "BRACE_R_MM", "0.5", "mm", "scalar";
    "BRACING_FALLBACK_FRACTIONS", "[0.5,0.25,0.75,0.1,0.9]", "1", "list[5]";
    "BRACING_MAX_STACKED_FALLBACKS", "6", "count", "scalar";
    "BRACING_MIN_CROSS_LINK_XY_MM", "0.7071067811865476", "mm", "scalar";
    "BRACING_MODEL_RUNG_MAX_CANDIDATES", "6", "count", "scalar";
    "BRACING_MODEL_RUNG_MIN_LENGTH_MM", "1.0", "mm", "scalar";
    "BRACING_MODEL_RUNG_Z_TOL_MM", "0.75", "mm", "scalar";
    "BRACING_REACH_TIER_BASE_FACTOR", "1.25", "1", "scalar";
    "BRANCH_ANGLE_DEG", "60.0", "°", "scalar";
    "BRIDGE_FREE_GAP_MM", "1.0", "mm", "scalar";
    "BRIDGE_SAG_SPAN_APPROX_MM", "{section_mm:0.3,span_mm:3.0}", "mm", "record{section_mm,span_mm}";
    "CCO_BIFURCATION_HOSTS", "5", "count", "scalar";
    "CCO_CONE_TOLERANCE_DEG", "0.000001", "°", "scalar";
    "CCO_FERMAT_MAX_ITERATIONS", "12", "count", "scalar";
    "CCO_MIN_SPLIT_LENGTH_MM", "0.5", "mm", "scalar";
    "CCO_SPLIT_SLIDE_POLICY", "{cap:40,end:0.97,start:0.03,step:0.03}", "1", "record{cap,end,start,step}";
    "CCO_XIA_ALPHA", "0.5", "1", "scalar";
    "CHITUBOX_DEFAULT_PLACEMENT_XY_MM", "{x:0.0,y:0.0}", "mm", "record{x,y}";
    "CHITUBOX_EMIT_N_SIDES", "24", "count", "scalar";
    "CHITUBOX_PAD_DIAMETER_MM", "2.2", "mm", "scalar";
    "CHITUBOX_PAD_N_SIDES", "20", "count", "scalar";
    "CHITUBOX_PLATE_MESH_TOL_MM", "0.00001", "mm", "scalar";
    "CHITUBOX_RAFT_HEIGHT_MM", "0.2", "mm", "scalar";
    "CHITUBOX_ROOT_HEIGHT_MM", "0.5", "mm", "scalar";
    "CHITUBOX_ROOT_LAYERS", "10", "count", "scalar";
    "CHITUBOX_SPHERE_MESH_TESSELLATION_POLICY", "{n_lat:8,n_lon:16,triangles:224}", "count", "record{n_lat,n_lon,triangles}";
    "CLEARANCE_SAMPLE_STEP_MM", "0.3", "mm", "scalar";
    "COINCIDE_TOL_MM", "0.15", "mm", "scalar";
    "COLINEAR_ANGLE_DEG", "10.0", "°", "scalar";
    "COLINEAR_RADIUS_FRAC", "0.2", "1", "scalar";
    "CONNECTIVITY_AXIS_ATTACH_TOL_MM", "0.25", "mm", "scalar";
    "CONNECTIVITY_EMBED_PAD_MM", "0.45", "mm", "scalar";
    "COVERAGE_RADIUS_FACTOR", "1.5", "1", "scalar";
    "COVERAGE_REPAIR_MAX_ATTEMPTS", "8", "count", "scalar";
    "DECIMATION_ADAPTATION_POLICY", "{adaptations:3,error_multiplier:0.6,feature_multiplier:2.0}", "1", "record{adaptations,error_multiplier,feature_multiplier}";
    "DECIMATION_FEATURE_HALO_MM", "0.4", "mm", "scalar";
    "DECIMATION_FEATURE_PROBE_RATIO", "0.2", "1", "scalar";
    "DECIMATION_P999_FACTOR", "0.001", "1", "scalar";
    "DECIMATION_P999_SAMPLE_POLICY", "{cap:150000,seed:1}", "count", "record{cap,seed}";
    "DECIMATION_P99_FACTOR", "0.0005", "1", "scalar";
    "DECIMATION_P99_SAMPLE_POLICY", "{cap:120000,seed:42}", "count", "record{cap,seed}";
    "DECIMATION_PROBE_GRID_PITCH_MM", "1.0", "mm", "scalar";
    "DECIMATION_PROBE_MAX_GRID_BYTES", "67108864", "bytes", "scalar";
    "DECIMATION_PROBE_NEAREST_UPPER_BOUND_MM", "10000.0", "mm", "scalar";
    "DECIMATION_PROTECTED_CAP_FRACTION", "0.12", "1", "scalar";
    "DECIMATION_RATIO_SEARCH", "{bisections:5,lower:0.05,upper:1.0}", "1", "record{bisections,lower,upper}";
    "DECIMATION_RECHECK_SLACK", "1.1", "1", "scalar";
    "DECIMATION_SCALE_ACCEPTANCE_MM", "interval(10.0,inclusive,300.0,inclusive)", "mm", "interval";
    "DECIMATION_SEED_ERROR_MM", "0.05", "mm", "scalar";
    "DECIMATION_UTILITY_MIN_INPUT_FACES", "32768", "count", "scalar";
    "DECIMATION_VERTEX_GROUP_FACTOR", "10.0", "1", "scalar";
    "DEFAULT_CONTACT_DEPTH_MM", "0.3", "mm", "scalar";
    "DELICACY_CURVATURE_WEIGHT", "0.6", "1", "scalar";
    "DELICACY_THINNESS_WEIGHT", "0.4", "1", "scalar";
    "EMIT_REQUIRE_PASSED_GATES_DEFAULT", "true", "1", "scalar";
    "ENCLOSURE_PROOF_TOL_MM", "0.018", "mm", "scalar";
    "ENCLOSURE_TOL_MM", "0.02", "mm", "scalar";
    "ENGINE_SURFACE_TREE_MAX_POINTS", "400000", "count", "scalar";
    "ENGINE_SURFACE_TREE_PITCH_MM", "0.3", "mm", "scalar";
    "EPS_COS", "0.000000001", "1", "scalar";
    "EXPECTED_CHITUBOX_API_VERSION", "1", "count", "scalar";
    "FLARE_LEN_MM", "1.5", "mm", "scalar";
    "FLARE_R_FACTOR", "1.6", "1", "scalar";
    "GATE_ON_BALL_BREACH_DEFAULT", "true", "1", "scalar";
    "GATE_ON_REMOVAL_DEFAULT", "false", "1", "scalar";
    "GRIP_CLEAR_MM", "5.0", "mm", "scalar";
    "GRIP_HANDLE_LEN_MM", "10.0", "mm", "scalar";
    "GRIP_HANDLE_R_MM", "1.0", "mm", "scalar";
    "HOLLOW_ALLOW_OUTPUT_DEFAULT", "false", "1", "scalar";
    "HOLLOW_BREATHING_VOXEL_PITCH_MM", "0.4", "mm", "scalar";
    "HOLLOW_BULK_MIN_MM", "10.0", "mm", "scalar";
    "HOLLOW_CONTAINMENT_MIN_FRACTION", "0.9", "1", "scalar";
    "HOLLOW_CONTAINMENT_SAMPLES", "32", "count", "scalar";
    "HOLLOW_DRAIN_DIRECTIONS", "[{x:0.0,y:0.0,z:-1.0},{x:0.5,y:0.0,z:-1.0},{x:-0.5,y:0.0,z:-1.0},{x:0.0,y:0.5,z:-1.0},{x:0.0,y:-0.5,z:-1.0}]", "1", "list[5]<record{x,y,z}>";
    "HOLLOW_DRAIN_R_MM", "1.5", "mm", "scalar";
    "HOLLOW_EXIT_RAY_MAX_MM", "45.0", "mm", "scalar";
    "HOLLOW_EXTRA_DRAIN_POLICY", "{cylinder_sides:24,exit_max_mm:8.0,extensions_mm:[4.0,8.0,9.0],max_extra_drains:3,min_pocket_ml:0.5}", "mixed", "record{cylinder_sides,exit_max_mm,extensions_mm,max_extra_drains,min_pocket_ml}";
    "HOLLOW_LOBE_BAND_MM", "15.0", "mm", "scalar";
    "HOLLOW_LOBE_MIN_ML", "0.5", "mL", "scalar";
    "HOLLOW_ORIENT_ENABLE_DEFAULT", "true", "1", "scalar";
    "HOLLOW_PITCH_MM", "0.5", "mm", "scalar";
    "HOLLOW_SAVE_MIN_FRAC", "0.25", "1", "scalar";
    "HOLLOW_SAVE_MIN_ML", "15.0", "mL", "scalar";
    "HOLLOW_SHARD_MAX_DIM_MM", "1.0", "mm", "scalar";
    "HOLLOW_SHARD_MAX_FACES", "64", "count", "scalar";
    "HOLLOW_SHARD_MAX_VOLUME_MM3", "0.01", "mm³", "scalar";
    "HOLLOW_VENT_DIRECTIONS", "[{x:1.0,y:0.0,z:0.0},{x:-1.0,y:0.0,z:0.0},{x:0.0,y:1.0,z:0.0},{x:0.0,y:-1.0,z:0.0}]", "1", "list[4]<record{x,y,z}>";
    "HOLLOW_VENT_R_MM", "1.25", "mm", "scalar";
    "HOLLOW_VENT_SAMPLE_DIVISOR", "40", "count", "scalar";
    "HOLLOW_VENT_TOP_BAND_MM", "2.0", "mm", "scalar";
    "HOLLOW_VOLUME_BALANCE_REL_TOL", "0.02", "1", "scalar";
    "HOLLOW_WALL_DEFAULT_MM", "2.5", "mm", "scalar";
    "HOLLOW_WALL_HANDLING_HEAVY_MM", "3.0", "mm", "scalar";
    "INITIAL_AXIS_MAJORITY_CAP", "80", "count", "scalar";
    "INITIAL_AXIS_MAJORITY_STEP_MM", "0.12", "mm", "scalar";
    "ISLAND_ATTACH_REACH_MM", "0.2", "mm", "scalar";
    "ISLAND_BIRTH_LINK_R_MM", "0.6", "mm", "scalar";
    "ISLAND_MERGE_RADIUS_MM", "2.0", "mm", "scalar";
    "JOINT_BALL_FACTOR", "1.0", "1", "scalar";
    "KAPPA", "1.25", "1", "scalar";
    "LIFT_MM", "5.0", "mm", "scalar";
    "LIFT_TOL_MM", "0.000001", "mm", "scalar";
    "L_SCAR_MAX_MM", "2.5", "mm", "scalar";
    "MATERIAL_COST_PER_MM", "0.05", "per-mm", "scalar";
    "MAX_DEFLECTION_MM", "0.15", "mm", "scalar";
    "MAX_SEGMENT_ANGLE_DEG", "45.0", "°", "scalar";
    "MAX_TRUNK_R_MM", "2.4", "mm", "scalar";
    "MAX_UNBRACED_SHAFT_R_MM", "0.9", "mm", "scalar";
    "MINIMA_BIRTH_MATCH_LAYERS", "2.0", "layers", "scalar";
    "MINIMA_CONNECT_LAYERS", "2.0", "layers", "scalar";
    "MINIMA_CONTACT_RADIUS_MM", "1.0", "mm", "scalar";
    "MINIMA_EPS_MM", "0.02", "mm", "scalar";
    "MINIMA_SUPPORT_AREA_TIPS", "1.0", "1", "scalar";
    "MIN_CONTACTS_PER_ISLAND", "1", "count", "scalar";
    "MIN_ILLEGAL_LEN_MM", "0.05", "mm", "scalar";
    "MIN_SHAFT_R_MM", "0.3", "mm", "scalar";
    "MIN_SHOOT_R_MM", "0.35", "mm", "scalar";
    "MIN_TRUNK_R_MM", "0.8", "mm", "scalar";
    "MODEL_BRACE_MAX_LEN_MM", "5.0", "mm", "scalar";
    "MODULUS_GREEN_GPA", "0.5", "GPa", "scalar";
    "MURRAY_TOLERANCE", "0.05", "1", "scalar";
    "NEAR_PLATE_MM", "1.5", "mm", "scalar";
    "ON_AXIS_TOL_MM", "0.2", "mm", "scalar";
    "ORIENT_SCAR_WEIGHT", "0.4", "1", "scalar";
    "ORIENT_WEIGHT_HEIGHT_MM", "0.3", "1/mm", "scalar";
    "ORIENT_WEIGHT_PEAK_XSEC_CM2", "3.0", "1/cm²", "scalar";
    "ORIENT_WEIGHT_SUPPORT_AREA_CM2", "0.5", "1/cm²", "scalar";
    "OVERHANG_ANGLE_DEG", "45.0", "°", "scalar";
    "P8_BALL_SLIDE_MAX_MM", "2.0", "mm", "scalar";
    "P8_BALL_SLIDE_STEP_MM", "0.25", "mm", "scalar";
    "P8_JUNCTION_RESEAT_ENABLE", "true", "1", "scalar";
    "P8_JUNCTION_RESEAT_MAX_MM", "1.0", "mm", "scalar";
    "P8_JUNCTION_RESEAT_STEP_MM", "0.25", "mm", "scalar";
    "P8_MAX_RELOCATE_ROUNDS", "4", "count", "scalar";
    "P8_MAX_RELOCATE_SWEEPS", "8", "count", "scalar";
    "P8_MAX_UNRESOLVED", "10", "count", "scalar";
    "P8_MAX_UNRESOLVED_WEIGHT", "3.0", "1", "scalar";
    "P8_POCKET_CORRIDOR_PAD_MM", "0.5", "mm", "scalar";
    "P8_POCKET_MIN_MEMBERS", "8", "count", "scalar";
    "P8_REAIM_AZIMUTH_DEG", "[0,45,90,135,180,225,270,315]", "°", "list[8]";
    "P8_REAIM_LENGTH_FACTORS", "[1.0,0.75,0.5]", "1", "list[3]";
    "P8_REAIM_POLAR_DEG", "[0,10,20,30]", "°", "list[4]";
    "P8_REAIM_ROUTE_GROWTH_MAX_FACTOR", "1.5", "1", "scalar";
    "P8_REAIM_VOLUME_DELTA_MAX_ML", "0.02", "mL", "scalar";
    "P8_RELOCATE_ENABLE", "true", "1", "scalar";
    "P8_RESEAT_ENABLE", "true", "1", "scalar";
    "P8_RESEAT_MAX_SEEDS", "24", "count", "scalar";
    "P8_RESEAT_RADIUS_MM", "2.0", "mm", "scalar";
    "P8_RESIN_DELTA_MAX", "0.03", "1", "scalar";
    "P8_RESIZE_FIXED_POINT_MAX_ROUNDS", "4", "count", "scalar";
    "P8_SITE_COMMIT_BUDGET", "2", "count", "scalar";
    "P8_STRUT_GROWTH_MAX_FACTOR", "1.25", "1", "scalar";
    "PEEL_ELEVATIONS_DEG", "[0,25,45,65,90]", "°", "list[5]";
    "PEEL_PRESSURE_PA", "3000.0", "Pa", "scalar";
    "PERF_ANCHOR_CAP", "{state:\"disabled\"}", "count", "trial_setting<anchor_candidate_cap>";
    "PERF_BEAM_K", "{state:\"disabled\"}", "count", "trial_setting<beam_width>";
    "PERF_FIX_ITER_CAPS", "{state:\"disabled\"}", "count", "trial_setting<brace_iteration_cap>";
    "PERF_SEED_SPACING_FACTOR", "{state:\"disabled\"}", "1", "trial_setting<seed_spacing_factor>";
    "PIPELINE_HOLLOW_ENABLE_DEFAULT", "true", "1", "scalar";
    "PIPELINE_P8_RELOCATE_DEFAULT", "false", "1", "scalar";
    "PLATE_BAND_MM", "0.2", "mm", "scalar";
    "PULL_DIST_MM", "15.0", "mm", "scalar";
    "REWIRE_DEGENERATE_LEN_MM", "0.0001", "mm", "scalar";
    "REWIRE_MAX_SLIDE_EVALS", "50", "count", "scalar";
    "REWIRE_ROLL_EPS", "0.0025", "1", "scalar";
    "REWIRE_ROLL_WINDOW", "20", "count", "scalar";
    "REWIRE_TWIG_MERGE_LEN_MM", "0.5", "mm", "scalar";
    "REWIRE_WORK_BUDGET", "{state:\"disabled\"}", "count", "trial_setting<nonzero_u64>";
    "SCAR_COST_WEIGHT", "1.0", "1", "scalar";
    "SLICE_UNION_MAX_ABS_MM", "10000.0", "mm", "scalar";
    "SLICE_UNION_QUANTUM_MM", "0.00000001", "mm", "scalar";
    "SPACING_FLOOR_MM", "2.5", "mm", "scalar";
    "STANDOFF_MM", "0.6", "mm", "scalar";
    "STRUT_FALLBACK_MAX_ANGLE_DEG", "60.0", "°", "scalar";
    "STRUT_MAX_LENGTH_MM", "20.0", "mm", "scalar";
    "STRUT_MIN_LENGTH_SAVED_MM", "8.0", "mm", "scalar";
    "STRUT_MODEL_ANCHOR_MAX_CANDIDATES", "40", "count", "scalar";
    "STRUT_MODEL_ANCHOR_MAX_DELICACY", "0.5", "1", "scalar";
    "STRUT_MODEL_ANCHOR_MIN_LEN_MM", "5.0", "mm", "scalar";
    "STUB_LENGTH_MM", "2.5", "mm", "scalar";
    "SUPPORT_BRACE_PENETRATION_MAX_ROWS", "100000", "count", "scalar";
    "SUPPORT_BRACE_PENETRATION_MAX_SCRATCH_BYTES", "100000000000", "B", "scalar";
    "SUPPORT_BRACE_PENETRATION_MAX_TOTAL_HITS", "1000000000", "count", "scalar";
    "SUPPORT_BRACE_PENETRATION_MAX_TOTAL_INTERVALS", "1000000000", "count", "scalar";
    "SUPPORT_BRACE_PENETRATION_MAX_TOTAL_SAMPLES", "10000000000", "count", "scalar";
    "SUPPORT_BRACE_PENETRATION_MAX_TOTAL_WORK", "1000000000", "count", "scalar";
    "SUPPORT_CONSTRUCTION_MAX_CONTACTS", "1000000", "count", "scalar";
    "SUPPORT_CONSTRUCTION_MAX_LOAD_ROWS", "10000000", "count", "scalar";
    "SUPPORT_CONSTRUCTION_MAX_SCRATCH_BYTES", "536870912", "B", "scalar";
    "SUPPORT_CONSTRUCTION_MAX_WORK_ITEMS", "4000000", "count", "scalar";
    "SUPPORT_KNN_MAX_LOGICAL_PEAK_BYTES", "2147483648", "B", "scalar";
    "SUPPORT_KNN_MAX_NEIGHBORS", "1", "count", "scalar";
    "SUPPORT_KNN_MAX_QUERY_POINTS", "20000000", "count", "scalar";
    "SUPPORT_KNN_MAX_SOURCE_POINTS", "20000000", "count", "scalar";
    "SUPPORT_KNN_MAX_WORK_FLOOR", "100000000", "count", "scalar";
    "SUPPORT_PARITY_GRID_MAX_BYTES_FLOOR", "67108864", "B", "scalar";
    "SUPPORT_PERCEPTION_MAX_ISLAND_NEIGHBOR_BYTES", "2147483648", "B", "scalar";
    "SUPPORT_PERCEPTION_MAX_ISLAND_NEIGHBOR_PAIRS", "100000000", "count", "scalar";
    "SUPPORT_PERCEPTION_MAX_LOOP_VERTICES", "100000000", "count", "scalar";
    "SUPPORT_PERCEPTION_MAX_MINIMA_VERTICES", "20000000", "count", "scalar";
    "SUPPORT_PERCEPTION_MAX_OVERLAP_BYTES", "2147483648", "B", "scalar";
    "SUPPORT_PERCEPTION_MAX_OVERLAP_EDGE_TESTS", "100000000", "count", "scalar";
    "SUPPORT_PERCEPTION_MAX_OVERLAP_PAIRS", "100000000", "count", "scalar";
    "SUPPORT_PERCEPTION_MAX_OVERLAP_VERTEX_TESTS", "100000000", "count", "scalar";
    "SUPPORT_PERCEPTION_MAX_PLANES", "20000", "count", "scalar";
    "SUPPORT_PERCEPTION_MAX_SLICE_INTERSECTIONS", "100000000", "count", "scalar";
    "SUPPORT_PERCEPTION_MAX_SLICE_OUTPUT_BYTES", "2147483648", "B", "scalar";
    "SUPPORT_PERCEPTION_MAX_TRIANGLE_PLANE_TESTS", "100000000", "count", "scalar";
    "SUPPORT_PREPARATION_COVERAGE_SAMPLES_MAX_VIRTUAL_CELLS", "2000000", "count", "scalar";
    "SUPPORT_PREPARATION_COVERAGE_SCORE_MATRIX_MAX_ENTRIES", "65536", "count", "scalar";
    "SUPPORT_PREPARATION_MAX_PRIMITIVES", "200000", "count", "scalar";
    "SUPPORT_PREPARATION_MAX_PRIMITIVE_BYTES", "134217728", "B", "scalar";
    "SUPPORT_RAFT_MAX_BYTES", "134217728", "B", "scalar";
    "SUPPORT_RAFT_MAX_CANDIDATES", "100000", "count", "scalar";
    "SUPPORT_RAFT_MAX_PRIMITIVES", "200000", "count", "scalar";
    "SUPPORT_RAFT_MAX_ROOTS", "100000", "count", "scalar";
    "SUPPORT_ROUTE_CCO_MAX_SEAT_EVALUATIONS", "536870912", "count", "scalar";
    "SUPPORT_ROUTE_CCO_MAX_SEAT_REFUSAL_ROWS", "536870912", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_COVERAGE_OPERATIONS", "2000000000", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_COVERAGE_SCRATCH_BYTES", "536870912", "B", "scalar";
    "SUPPORT_SPECIALIZED_MAX_GATE_CONTACT_MATRIX_BYTES", "536870912", "B", "scalar";
    "SUPPORT_SPECIALIZED_MAX_GATE_OPERATIONS", "2000000000", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_GATE_PEAK_BYTES", "1073741824", "B", "scalar";
    "SUPPORT_SPECIALIZED_MAX_GATE_VARIANTS", "20000001", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_GATE_VECTOR_BYTES", "536870912", "B", "scalar";
    "SUPPORT_SPECIALIZED_MAX_GATE_WINDOW_VARIANTS", "1024", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_LOAD_OPERATIONS", "2000000000", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_LOAD_SCRATCH_BYTES", "536870912", "B", "scalar";
    "SUPPORT_SPECIALIZED_MAX_MERGE_COMMIT_BYTES", "536870912", "B", "scalar";
    "SUPPORT_SPECIALIZED_MAX_MERGE_DESCRIPTOR_OPERATIONS", "2000000000", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_MERGE_ITERATIONS", "100000", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_MERGE_PAIRS", "20000000", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_MERGE_PROPOSALS", "20000000", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_MERGE_PROPOSAL_BYTES", "536870912", "B", "scalar";
    "SUPPORT_SPECIALIZED_MAX_MERGE_TOTAL_PROPOSALS", "100000000", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_SATISFACTION_NEIGHBOR_BYTES", "2147483648", "B", "scalar";
    "SUPPORT_SPECIALIZED_MAX_SATISFACTION_NEIGHBOR_PAIRS", "100000000", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_SATISFACTION_TESTS", "100000000", "count", "scalar";
    "SUPPORT_SPECIALIZED_MAX_SYNTHETIC_NEIGHBOR_BYTES", "536870912", "B", "scalar";
    "SUPPORT_SPECIALIZED_MAX_SYNTHETIC_NEIGHBOR_PAIRS", "20000000", "count", "scalar";
    "SURFACE_TREE_MAX_POINTS", "150000", "count", "scalar";
    "SURFACE_TREE_PITCH_MM", "0.4", "mm", "scalar";
    "SURFACE_TREE_SEED", "0", "count", "scalar";
    "TARGET_SPACING_MM", "4.0", "mm", "scalar";
    "TENSILE_GREEN_MPA", "0.288", "MPa", "scalar";
    "THIN_FEATURE_MM", "1.5", "mm", "scalar";
    "TIP_RADIUS_DEFAULT_MM", "0.2", "mm", "scalar";
    "TIP_RADIUS_MAX_LEGACY_MM", "1.0", "mm", "scalar";
    "TIP_SAFETY_FACTOR", "8.0", "1", "scalar";
    "TOL_MM", "0.05", "mm", "scalar";
    "TRANSIENT_SHOOT_COUNT_EXPONENT", "0.125", "1", "scalar";
    "TRUNK_SEARCH_DIRECTIONS", "12", "count", "scalar";
    "TRUNK_SEARCH_RADII_MM", "[0.0,2.0,4.0,7.0,10.0,14.0,19.0,25.0]", "mm", "list[8]";
    "TWIN_SINGLE_BRACE_XY_MM", "3.0", "mm", "scalar";
    "VOXEL_PITCH_MM", "0.4", "mm", "scalar";
};

fn component() -> M0PlaceholderConstants {
    M0PlaceholderConstants::new().expect("all 262 placeholder declarations must construct")
}

#[test]
fn exact_handwritten_membership_is_stable_sorted_and_all_placeholder() {
    let descriptors = component().descriptors().unwrap();
    let ids = descriptors
        .iter()
        .map(|descriptor| descriptor.id().as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, EXPECTED_PLACEHOLDER_IDS);
    assert_eq!(descriptors.len(), 262);
    assert!(descriptors
        .windows(2)
        .all(|pair| pair[0].id() < pair[1].id()));
    assert!(descriptors.iter().all(|descriptor| {
        descriptor.provenance().class() == RegistryProvenanceClass::Placeholder
    }));
    assert!(descriptors.iter().all(|descriptor| {
        let plinth_core::RegistryProvenanceView::Placeholder { note } =
            descriptor.provenance().view()
        else {
            return false;
        };
        note.contains("non-AcceptSafe") && !note.trim().is_empty()
    }));
}

#[test]
fn every_placeholder_value_unit_and_shape_matches_the_independent_table() {
    let descriptors = component().descriptors().unwrap();
    for (descriptor, expected) in descriptors.iter().zip(EXPECTED_ROWS.iter()) {
        assert_eq!(descriptor.id().as_str(), expected.id, "ID drift");
        assert_eq!(
            value_key(descriptor.value()),
            expected.value,
            "value drift for {}",
            expected.id
        );
        assert_eq!(
            descriptor.unit(),
            expected.unit,
            "unit drift for {}",
            expected.id
        );
        assert_eq!(
            descriptor.shape(),
            expected.shape,
            "shape drift for {}",
            expected.id
        );
    }
}

fn value_key(value: &ConstantValue) -> String {
    match value.view() {
        ConstantValueView::Bool(value) => value.to_string(),
        ConstantValueView::SignedInteger(value) => value.to_string(),
        ConstantValueView::UnsignedInteger(value) => value.to_string(),
        ConstantValueView::FiniteScalar(value) => finite_key(value),
        ConstantValueView::Text(value) => format!("\"{value}\""),
        ConstantValueView::FiniteInterval(value) => format!(
            "interval({},{},{},{})",
            finite_key(value.lower()),
            bound_key(value.lower_bound()),
            finite_key(value.upper()),
            bound_key(value.upper_bound())
        ),
        ConstantValueView::LoadSharing(_) => {
            panic!("placeholder table contains no load-sharing row")
        }
        ConstantValueView::List(values) => format!(
            "[{}]",
            values.iter().map(value_key).collect::<Vec<_>>().join(",")
        ),
        ConstantValueView::Record(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(field, value)| format!("{field}:{}", value_key(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn finite_key(value: f64) -> String {
    let mut value = value.to_string();
    if !value.contains(['.', 'e', 'E']) {
        value.push_str(".0");
    }
    value
}

const fn bound_key(bound: BoundKind) -> &'static str {
    match bound {
        BoundKind::Inclusive => "inclusive",
        BoundKind::Exclusive => "exclusive",
    }
}

#[test]
fn evidence_and_placeholder_fixture_builds_one_293_entry_structural_batch() {
    let mut descriptors = component()
        .descriptors()
        .unwrap()
        .into_iter()
        .collect::<Vec<_>>();
    descriptors.extend(M0EvidenceConstants::new().unwrap().descriptors().unwrap());
    let registry = ConstantsRegistry::build_batch(descriptors).unwrap();
    assert_eq!(registry.len(), 293);
    assert_eq!(
        registry
            .iter()
            .map(|descriptor| descriptor.id().as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        293
    );
}

/// The engine's implied tip area ratio must not EXCEED the H2 allowance.
///
/// `FrozenTipLoadPolicy::tip_radius_mm` sizes tips so that
/// `pi*r^2 = A * peel_pressure_pa * tip_safety_factor / tensile_green_pa`,
/// i.e. it already IS an area-ratio law with ratio
/// `X_engine = tensile_green_pa / (peel_pressure_pa * tip_safety_factor)`.
///
/// THE ALLOWANCE ANCHOR IS `X_ALLOWED`.  There is exactly one registered
/// area-ratio allowance in this engine and it is the Derived evidence constant
/// `X_ALLOWED = floor(lower(X_STAR_CONE)/2) = 12`, chained from the measured
/// plate interval.  Contact physics PR-4 reads THAT entry — it does not
/// re-declare the number as a hand-typed placeholder, because a duplicate would
/// let a plate recalibration move `X_STAR_CONE` while the contact threshold
/// silently stayed at the stale hand-typed 12.  Reading the chained constant is
/// what makes a recalibration flow through.
///
/// This is deliberately a ONE-SIDED bound and not an exact-equality coupling.
/// `X_engine` = 12 is an internal CONSISTENCY identity that is a function of
/// the arbitrary Python-era `tip_safety_factor` = 8 — had that been 6 the
/// identity would read 16 — so it is NOT an independent measurement of
/// `X_ALLOWED`.  Hard-coupling the two would let a future recalibration of
/// `peel_pressure_pa` or `tensile_green_mpa` silently move the contact-area
/// threshold, or break this unrelated gate.  What matters physically is only
/// the safety direction: a LARGER implied ratio means less section per unit
/// load than the calibration plate allows.
#[test]
fn engine_implied_tip_area_ratio_does_not_exceed_the_h2_allowance() {
    let constants = M0PlaceholderConstants::new().expect("placeholder constants");
    let peel_pressure_pa = constants.peel_pressure_pa().diagnostic().get();
    let tensile_pa = constants.tensile_green_mpa().diagnostic().get() * 1.0e6;
    let safety_factor = constants.tip_safety_factor().diagnostic().get();
    // The allowance comes from the EVIDENCE registry's `X_ALLOWED`, the sole
    // registered anchor, so a plate recalibration reaches this gate.
    let evidence = M0EvidenceConstants::new().expect("evidence constants");
    let allowance = *evidence.x_allowed().value();
    assert!(
        (allowance - 12.0).abs() < 1.0e-12,
        "X_ALLOWED is the anchor and currently reads {allowance}"
    );

    let implied = tensile_pa / (peel_pressure_pa * safety_factor);
    assert!(
        implied <= allowance * (1.0 + 1.0e-12),
        "engine implied area ratio {implied} exceeds the H2 allowance {allowance}: \
         the tip policy would provision LESS section per unit load than the \
         calibration plate permits"
    );

    // SOFT review trigger — records rather than fails.  A strictly smaller
    // implied ratio is safe, but means the engine and the measured allowance
    // have drifted apart and the pairing should be re-examined.
    if implied < allowance * (1.0 - 1.0e-12) {
        println!(
            "REVIEW TRIGGER: engine implied tip area ratio {implied} is now strictly \
             more conservative than the H2 allowance {allowance}. This is safe, but the \
             pairing in contact-physics PR-4 should be re-examined against the current \
             calibration evidence."
        );
    }
}

#[test]
fn named_authority_paths_exist_without_runtime_ledger_parsing() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (subject, relative) in [
        (
            "placeholder numeric and grade authority",
            "docs/engineering-journal/03-physics-and-constants.md",
        ),
        (
            "performance trial domain definitions",
            "docs/engineering-journal/specs/SPEC-V5-PERF.md",
        ),
    ] {
        let path = repository.join(relative);
        assert!(
            path.is_file(),
            "required source for {subject} is missing: {}",
            path.display()
        );
    }
}

#[test]
fn scalar_domains_reject_invalid_values_and_preserve_signed_zero() {
    assert!(PositiveFinite::new(1.0).is_ok());
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(PositiveFinite::new(invalid).is_err());
    }
    assert!(NonNegativeFinite::new(-1.0).is_err());
    assert!(NonNegativeFinite::new(f64::NAN).is_err());
    assert_eq!(
        NonNegativeFinite::new(-0.0).unwrap().get().to_bits(),
        (-0.0_f64).to_bits()
    );
    for invalid in [-0.1, 1.1, f64::NAN] {
        assert!(UnitFraction::new(invalid).is_err());
    }
    assert_eq!(UnitFraction::new(0.0).unwrap().get(), 0.0);
    assert_eq!(UnitFraction::new(1.0).unwrap().get(), 1.0);

    assert!(serde_json::from_str::<PositiveFinite>("0").is_err());
    assert!(serde_json::from_str::<NonNegativeFinite>("-1").is_err());
    assert!(serde_json::from_str::<UnitFraction>("1.01").is_err());
}

#[test]
fn coupled_structures_validate_domains_accessors_and_strict_wire_shapes() {
    let bridge = BridgeSagLimit::new(3.0, 0.3).unwrap();
    assert_eq!((bridge.span_mm(), bridge.section_mm()), (3.0, 0.3));
    assert_strict_roundtrip(&bridge, r#"{"span_mm":3.0,"section_mm":0.3}"#);
    assert!(BridgeSagLimit::new(0.0, 0.3).is_err());

    let slide = CcoSplitSlidePolicy::new(0.03, 0.97, 0.03, 40).unwrap();
    assert_eq!(
        (slide.start(), slide.end(), slide.step(), slide.cap().get()),
        (0.03, 0.97, 0.03, 40)
    );
    assert_strict_roundtrip(&slide, r#"{"start":0.03,"end":0.97,"step":0.03,"cap":40}"#);
    assert!(CcoSplitSlidePolicy::new(0.9, 0.1, 0.03, 40).is_err());
    assert!(CcoSplitSlidePolicy::new(0.03, 0.97, 0.95, 40).is_err());
    assert!(CcoSplitSlidePolicy::new(0.03, 0.97, 0.03, 0).is_err());

    let placement = PlacementXyMm::new(-0.0, 0.0).unwrap();
    assert_eq!(placement.x().to_bits(), (-0.0_f64).to_bits());
    assert_eq!(placement.y().to_bits(), 0.0_f64.to_bits());
    assert!(PlacementXyMm::new(f64::NAN, 0.0).is_err());
    assert_strict_roundtrip(&placement, r#"{"x":-0.0,"y":0.0}"#);

    let sphere = SphereTessellationPolicy::new(8, 16, 224).unwrap();
    assert_eq!(
        (
            sphere.n_lat().get(),
            sphere.n_lon().get(),
            sphere.triangles().get()
        ),
        (8, 16, 224)
    );
    assert!(SphereTessellationPolicy::new(8, 16, 225).is_err());
    assert!(SphereTessellationPolicy::new(0, 16, 224).is_err());
    assert_strict_roundtrip(&sphere, r#"{"n_lat":8,"n_lon":16,"triangles":224}"#);

    let adaptation = DecimationAdaptationPolicy::new(3, 2.0, 0.6).unwrap();
    assert_eq!(
        (
            adaptation.adaptations().get(),
            adaptation.feature_multiplier(),
            adaptation.error_multiplier()
        ),
        (3, 2.0, 0.6)
    );
    assert!(DecimationAdaptationPolicy::new(0, 2.0, 0.6).is_err());
    assert!(DecimationAdaptationPolicy::new(3, 0.0, 0.6).is_err());
    assert!(DecimationAdaptationPolicy::new(3, 2.0, 1.1).is_err());
    assert_strict_roundtrip(
        &adaptation,
        r#"{"adaptations":3,"feature_multiplier":2.0,"error_multiplier":0.6}"#,
    );

    let sample = DecimationSamplingPolicy::new(42, 120_000).unwrap();
    assert_eq!((sample.seed(), sample.cap().get()), (42, 120_000));
    assert!(DecimationSamplingPolicy::new(42, 0).is_err());
    assert_strict_roundtrip(&sample, r#"{"seed":42,"cap":120000}"#);

    let ratio = DecimationRatioSearchPolicy::new(0.05, 1.0, 5).unwrap();
    assert_eq!(
        (ratio.lower(), ratio.upper(), ratio.bisections().get()),
        (0.05, 1.0, 5)
    );
    assert!(DecimationRatioSearchPolicy::new(1.0, 0.05, 5).is_err());
    assert!(DecimationRatioSearchPolicy::new(0.05, 1.0, 0).is_err());
    assert_strict_roundtrip(&ratio, r#"{"lower":0.05,"upper":1.0,"bisections":5}"#);
}

#[test]
fn hollow_structures_validate_every_condition_and_strict_wire_shape() {
    let directions = DrainDirections::new([
        DrainDirection::new(0.0, 0.0, -1.0).unwrap(),
        DrainDirection::new(0.5, 0.0, -1.0).unwrap(),
        DrainDirection::new(-0.5, 0.0, -1.0).unwrap(),
        DrainDirection::new(0.0, 0.5, -1.0).unwrap(),
        DrainDirection::new(0.0, -0.5, -1.0).unwrap(),
    ])
    .unwrap();
    assert_eq!(directions.directions().len(), 5);
    assert!(directions
        .directions()
        .iter()
        .all(|direction| direction.z() < 0.0));
    assert_eq!(directions.directions()[1].x(), 0.5);
    assert_eq!(directions.directions()[3].y(), 0.5);
    assert!(DrainDirection::new(0.0, 0.0, 0.0).is_err());
    assert!(DrainDirection::new(f64::NAN, 0.0, -1.0).is_err());
    let invalid = [DrainDirection::new(0.0, 0.0, 1.0).unwrap(); 5];
    assert!(DrainDirections::new(invalid).is_err());
    let wire = serde_json::to_string(&directions).unwrap();
    assert_eq!(
        serde_json::from_str::<DrainDirections>(&wire).unwrap(),
        directions
    );
    assert!(serde_json::from_str::<DrainDirections>(
        r#"[{"x":0,"y":0,"z":1},{"x":0,"y":0,"z":1},{"x":0,"y":0,"z":1},{"x":0,"y":0,"z":1},{"x":0,"y":0,"z":1}]"#
    )
    .is_err());
    assert!(serde_json::from_str::<DrainDirection>(r#"{"x":0,"y":0,"z":-1,"extra":0}"#).is_err());

    let extra = HollowExtraDrainPolicy::new(0.5, 3, 8.0, &[4.0, 8.0, 9.0], 24).unwrap();
    assert_eq!(extra.min_pocket_ml(), 0.5);
    assert_eq!(extra.max_extra_drains().get(), 3);
    assert_eq!(extra.exit_max_mm(), 8.0);
    assert_eq!(extra.extensions_mm().collect::<Vec<_>>(), [4.0, 8.0, 9.0]);
    assert_eq!(extra.cylinder_sides().get(), 24);
    assert!(HollowExtraDrainPolicy::new(0.5, 0, 8.0, &[4.0], 24).is_err());
    assert!(HollowExtraDrainPolicy::new(0.5, 3, 8.0, &[8.0, 4.0], 24).is_err());
    assert!(HollowExtraDrainPolicy::new(0.5, 3, 8.0, &[], 24).is_err());
    assert_strict_roundtrip(
        &extra,
        r#"{"min_pocket_ml":0.5,"max_extra_drains":3,"exit_max_mm":8.0,"extensions_mm":[4.0,8.0,9.0],"cylinder_sides":24}"#,
    );
}

#[test]
fn performance_types_are_disabled_strict_and_reject_invalid_enabled_values() {
    assert!(BeamWidth::new(0).is_err());
    assert!(AnchorCandidateCap::new(0).is_err());
    assert!(FixBraceIterationCap::new(0).is_err());
    for invalid in [1.0, 0.0, f64::NAN] {
        assert!(SeedSpacingFactor::new(invalid).is_err());
    }
    assert_eq!(BeamWidth::new(4).unwrap().get().get(), 4);
    assert_eq!(AnchorCandidateCap::new(4).unwrap().get().get(), 4);
    assert_eq!(FixBraceIterationCap::new(2).unwrap().get().get(), 2);
    assert_eq!(SeedSpacingFactor::new(1.4).unwrap().get(), 1.4);

    let constants = component();
    assert!(constants.perf_anchor_cap().diagnostic().is_disabled());
    assert!(constants.perf_beam_k().diagnostic().is_disabled());
    assert!(constants.perf_fix_iter_caps().diagnostic().is_disabled());
    assert!(constants
        .perf_seed_spacing_factor()
        .diagnostic()
        .is_disabled());
    assert!(constants.rewire_work_budget().diagnostic().is_disabled());
    assert!(constants
        .perf_fix_iter_caps()
        .note
        .contains("only the consumed brace-iteration cap"));
    assert!(constants
        .rewire_work_budget()
        .note
        .contains("no wall-clock fallback"));

    let disabled: TrialSetting<BeamWidth> =
        serde_json::from_str(r#"{"state":"disabled"}"#).unwrap();
    assert!(disabled.is_disabled());
    assert!(disabled.enabled_value().is_none());
    assert_eq!(
        serde_json::to_string(&disabled).unwrap(),
        r#"{"state":"disabled"}"#
    );
    let enabled = TrialSetting::enabled(BeamWidth::new(4).unwrap());
    assert_eq!(enabled.enabled_value().unwrap().get().get(), 4);
    assert_eq!(
        serde_json::to_string(&enabled).unwrap(),
        r#"{"state":"enabled","value":4}"#
    );
    assert_eq!(
        serde_json::from_str::<TrialSetting<BeamWidth>>(r#"{"state":"enabled","value":4}"#)
            .unwrap(),
        enabled
    );
    assert!(
        serde_json::from_str::<TrialSetting<BeamWidth>>(r#"{"state":"enabled","value":0}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<TrialSetting<BeamWidth>>(r#"{"state":"disabled","value":4}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<TrialSetting<BeamWidth>>(r#"{"state":"disabled","extra":1}"#)
            .is_err()
    );
}

#[test]
fn named_accessors_preserve_lift_contact_bridge_and_search_distinctions() {
    let constants = component();
    assert_eq!(constants.lift_mm().diagnostic().get(), 5.0);
    assert_eq!(constants.default_contact_depth_mm().diagnostic().get(), 0.3);
    assert_eq!(constants.bridge_free_gap_mm().diagnostic().get(), 1.0);
    assert_eq!(
        constants.bridge_sag_span_approx_mm().diagnostic().span_mm(),
        3.0
    );
    assert_eq!(
        constants
            .bridge_sag_span_approx_mm()
            .diagnostic()
            .section_mm(),
        0.3
    );
    assert_eq!(constants.tip_radius_default_mm().diagnostic().get(), 0.2);
    assert_eq!(
        constants.cco_split_slide_policy().diagnostic().cap().get(),
        40
    );
    assert_eq!(
        constants
            .decimation_ratio_search()
            .diagnostic()
            .bisections()
            .get(),
        5
    );
    assert_eq!(
        constants
            .hollow_extra_drain_policy()
            .diagnostic()
            .extensions_mm()
            .collect::<Vec<_>>(),
        [4.0, 8.0, 9.0]
    );
}

fn assert_strict_roundtrip<T>(value: &T, exact: &str)
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + PartialEq + std::fmt::Debug + Clone,
{
    assert_eq!(serde_json::to_string(value).unwrap(), exact);
    assert_eq!(serde_json::from_str::<T>(exact).unwrap(), value.clone());
    let with_unknown = exact
        .strip_suffix('}')
        .map(|prefix| format!("{prefix},\"unknown\":0}}"));
    if let Some(with_unknown) = with_unknown {
        assert!(serde_json::from_str::<T>(&with_unknown).is_err());
    }
}
