//! The crate's determinism-contract constants must equal their M0 registry
//! declarations (every-record-site discipline; the ledger row is
//! cross-checked by plinth-core's FU-24 consistency gate).

#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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
/// exactly the same tolerance the kernel already merges plane-slice
/// crossings on, so quantization is lossless with respect to stitched
/// vertices. `plinth-support::perception`'s private `LOOP_MERGE_MM` is
/// itself just `geom_kernel::PLANE_SLICE_MERGE_TOLERANCE_MM` (a public
/// constant `plinth-layers` already depends on), so this pins against that
/// constant directly rather than against a bare literal that could drift
/// from it unnoticed.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn quantum_is_exactly_the_stitch_key() {
    assert_eq!(
        plinth_layers::SLICE_UNION_QUANTUM_MM,
        geom_kernel::PLANE_SLICE_MERGE_TOLERANCE_MM
    );
}
