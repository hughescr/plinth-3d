//! Public-API refusal suite for malformed decimation execution budgets.
//!
//! `DecimationBudget::validate_for_execution` claims to refuse incomplete or
//! impossible execution inputs *before any probe surface allocation or native
//! call*. These tests hold the public entry point `plinth_decimate::run` to
//! that claim: every malformed probe field is refused as a typed configuration
//! error, and the refusal provably precedes the scale gate and the feature
//! probe, which are the first two pieces of geometric work `run` would
//! otherwise perform.

use plinth_decimate::{
    run, BudgetConfigError, BudgetSearchOutcome, DecimationBudget, DecimationError, ProbeConfig,
};
use plinth_mesh::Mesh;

const GRID_BYTES: usize = 64 * 1024 * 1024;
const NEAREST_UPPER_BOUND_MM: f64 = 10_000.0;
const GRID_PITCH_MM: f64 = 1.0;

/// `ratified_default` only reads the registered Saturn pixel pitch; it does
/// not validate the probe, so malformed probes construct fine. That is exactly
/// the point: refusal is owned by the execution boundary, not by construction.
fn budget(
    grid_pitch_mm: f64,
    max_grid_bytes: usize,
    nearest_upper_bound_mm: f64,
) -> DecimationBudget {
    DecimationBudget::ratified_default(ProbeConfig::new(
        grid_pitch_mm,
        max_grid_bytes,
        nearest_upper_bound_mm,
    ))
    .expect("registered pixel pitch")
}

fn well_formed_budget() -> DecimationBudget {
    budget(GRID_PITCH_MM, GRID_BYTES, NEAREST_UPPER_BOUND_MM)
}

fn in_scale_mesh() -> Mesh {
    plinth_mesh::make_box([10.0; 3], [0.0; 3]).expect("in-scale box fixture")
}

/// Below `SearchPolicy::scale_min_mm`, so a well-formed budget provably reaches
/// and trips the scale gate on this mesh.
fn out_of_scale_mesh() -> Mesh {
    plinth_mesh::make_box([1.0e-3; 3], [0.0; 3]).expect("out-of-scale box fixture")
}

/// A single zero-area triangle spanning 10 mm: in scale, so it clears the scale
/// gate, but no probe surface can be built from it.
fn degenerate_in_scale_mesh() -> Mesh {
    Mesh::new(
        vec![[0.0, 0.0, 0.0], [5.0, 0.0, 0.0], [10.0, 0.0, 0.0]],
        vec![[0, 1, 2]],
    )
    .expect("finite, index-valid degenerate soup")
}

/// One batched table of every malformed probe field with the typed error it
/// must produce. Rows are (grid pitch, grid byte cap, nearest upper bound).
fn malformed_probe_table() -> Vec<(DecimationBudget, BudgetConfigError)> {
    [
        (0.0, GRID_BYTES, NEAREST_UPPER_BOUND_MM),
        (-1.0, GRID_BYTES, NEAREST_UPPER_BOUND_MM),
        (f64::NAN, GRID_BYTES, NEAREST_UPPER_BOUND_MM),
        (f64::INFINITY, GRID_BYTES, NEAREST_UPPER_BOUND_MM),
        (GRID_PITCH_MM, 0, NEAREST_UPPER_BOUND_MM),
        (GRID_PITCH_MM, usize::MAX, NEAREST_UPPER_BOUND_MM),
        (GRID_PITCH_MM, GRID_BYTES, -1.0),
        (GRID_PITCH_MM, GRID_BYTES, f64::NAN),
        (GRID_PITCH_MM, GRID_BYTES, f64::INFINITY),
    ]
    .into_iter()
    .map(|(grid_pitch_mm, max_grid_bytes, nearest_upper_bound_mm)| {
        let expected = if !grid_pitch_mm.is_finite() || grid_pitch_mm <= 0.0 {
            BudgetConfigError::InvalidGridPitch {
                value: grid_pitch_mm,
            }
        } else if max_grid_bytes == 0 || max_grid_bytes == usize::MAX {
            BudgetConfigError::InvalidGridBytes {
                value: max_grid_bytes,
            }
        } else {
            BudgetConfigError::InvalidNearestUpperBound {
                value: nearest_upper_bound_mm,
            }
        };
        (
            budget(grid_pitch_mm, max_grid_bytes, nearest_upper_bound_mm),
            expected,
        )
    })
    .collect()
}

/// `f64` equality is not usable for the NaN rows, so compare payloads by bit
/// pattern. Written without a wildcard arm so a new `BudgetConfigError`
/// variant fails to compile here instead of slipping through untested.
fn same_config_error(actual: &BudgetConfigError, expected: &BudgetConfigError) -> bool {
    match (actual, expected) {
        (
            BudgetConfigError::InvalidGridPitch { value: actual },
            BudgetConfigError::InvalidGridPitch { value: expected },
        )
        | (
            BudgetConfigError::InvalidNearestUpperBound { value: actual },
            BudgetConfigError::InvalidNearestUpperBound { value: expected },
        ) => actual.to_bits() == expected.to_bits(),
        (
            BudgetConfigError::InvalidGridBytes { value: actual },
            BudgetConfigError::InvalidGridBytes { value: expected },
        ) => actual == expected,
        (
            BudgetConfigError::Constants { message: actual },
            BudgetConfigError::Constants { message: expected },
        )
        | (
            BudgetConfigError::PixelPitch { message: actual },
            BudgetConfigError::PixelPitch { message: expected },
        ) => actual == expected,
        (
            BudgetConfigError::InvalidGridPitch { .. }
            | BudgetConfigError::InvalidGridBytes { .. }
            | BudgetConfigError::InvalidNearestUpperBound { .. }
            | BudgetConfigError::Constants { .. }
            | BudgetConfigError::PixelPitch { .. },
            _,
        ) => false,
    }
}

/// Stable label per variant, again without a wildcard arm, so the reachability
/// assertion in `every_budget_config_error_variant_is_reachable_from_the_public_run`
/// must be revisited whenever the enum grows.
fn variant_label(error: &BudgetConfigError) -> &'static str {
    match error {
        BudgetConfigError::InvalidGridPitch { .. } => "InvalidGridPitch",
        BudgetConfigError::InvalidGridBytes { .. } => "InvalidGridBytes",
        BudgetConfigError::InvalidNearestUpperBound { .. } => "InvalidNearestUpperBound",
        BudgetConfigError::Constants { .. } => "Constants",
        BudgetConfigError::PixelPitch { .. } => "PixelPitch",
    }
}

#[test]
fn every_malformed_probe_field_is_refused_as_a_typed_config_error() {
    let mesh = in_scale_mesh();
    for (budget, expected) in malformed_probe_table() {
        let run_error = match run(&mesh, budget) {
            Err(DecimationError::Config(error)) => error,
            other => panic!("expected a typed config refusal for {budget:?}, got {other:?}"),
        };
        assert!(
            same_config_error(&run_error, &expected),
            "run refused {budget:?} with {run_error:?}, expected {expected:?}"
        );

        // Asserting the policy separately pins that `run` delegates to
        // `validate_for_execution` rather than re-implementing the rule.
        let policy_error = budget
            .validate_for_execution()
            .expect_err("policy must refuse the same malformed budget");
        assert!(
            same_config_error(&policy_error, &expected),
            "policy refused {budget:?} with {policy_error:?}, expected {expected:?}"
        );
    }
}

#[test]
fn ordering_precedence_is_config_then_scale() {
    let mesh = out_of_scale_mesh();

    // Baseline: under a well-formed budget this mesh reaches the scale gate.
    assert!(
        matches!(
            run(&mesh, well_formed_budget()),
            Ok(BudgetSearchOutcome::SkippedScaleSuspicious { .. })
        ),
        "the out-of-scale fixture must reach and trip the scale gate"
    );

    // The load-bearing assertion: the same mesh now returns the config refusal
    // instead of the scale skip, which can only happen if refusal preceded the
    // scale computation — hence before any probe surface or native call.
    assert_eq!(
        run(&mesh, budget(GRID_PITCH_MM, 0, NEAREST_UPPER_BOUND_MM)),
        Err(DecimationError::Config(
            BudgetConfigError::InvalidGridBytes { value: 0 }
        ))
    );
}

#[test]
fn refusal_precedes_any_probe_surface_construction() {
    let mesh = degenerate_in_scale_mesh();

    // Baseline: this mesh clears the scale gate and then fails in the feature
    // probe, so it is a live witness that surface construction was attempted.
    assert!(
        matches!(
            run(&mesh, well_formed_budget()),
            Err(DecimationError::FeatureProtection(_))
        ),
        "the degenerate fixture must fail during probe surface construction"
    );

    // With a malformed budget the same mesh never reaches that failure.
    assert_eq!(
        run(&mesh, budget(GRID_PITCH_MM, 0, NEAREST_UPPER_BOUND_MM)),
        Err(DecimationError::Config(
            BudgetConfigError::InvalidGridBytes { value: 0 }
        ))
    );
}

#[test]
fn every_budget_config_error_variant_is_reachable_from_the_public_run() {
    let mesh = in_scale_mesh();
    let mut observed: Vec<&'static str> = malformed_probe_table()
        .into_iter()
        .map(|(budget, _)| match run(&mesh, budget) {
            Err(DecimationError::Config(error)) => variant_label(&error),
            other => panic!("expected a typed config refusal for {budget:?}, got {other:?}"),
        })
        .collect();
    observed.sort_unstable();
    observed.dedup();

    assert_eq!(
        observed,
        [
            "InvalidGridBytes",
            "InvalidGridPitch",
            "InvalidNearestUpperBound"
        ],
        "every execution-boundary refusal variant must be exercised from the public run"
    );
}
