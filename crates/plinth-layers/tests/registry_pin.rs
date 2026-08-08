//! The crate's determinism-contract constants must equal their M0 registry
//! declarations (every-record-site discipline; the ledger row is
//! cross-checked by plinth-core's FU-24 consistency gate).

#[test]
fn quantum_and_domain_match_the_m0_registry() {
    let constants = plinth_core::m0_constants().expect("M0 constants registry must construct");
    assert_eq!(
        constants
            .placeholders()
            .slice_union_quantum_mm()
            .diagnostic()
            .get(),
        plinth_layers::SLICE_UNION_QUANTUM_MM,
    );
    assert_eq!(
        constants
            .placeholders()
            .slice_union_max_abs_mm()
            .diagnostic()
            .get(),
        plinth_layers::SLICE_UNION_MAX_ABS_MM,
    );
}

/// The quantum is the stitch key: the design's OQ-1 i64 branch fixed it at
/// exactly 1e-8 mm so quantization is lossless with respect to stitched
/// vertices. Perception's `LOOP_MERGE_MM` is private; this pins the value the
/// phase-C cutover will rely on.
#[test]
fn quantum_is_exactly_the_stitch_key() {
    assert_eq!(plinth_layers::SLICE_UNION_QUANTUM_MM, 1.0e-8);
}
