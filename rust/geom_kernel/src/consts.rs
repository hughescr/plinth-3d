//! Named tolerance constants, copied VERBATIM from geometry_exact.py with a
//! comment linking each to its source line. Changing any of these is a parity
//! break; they are the decision skins the whole suite pins.

/// Möller barycentric / t-parameter slack (geometry_exact.py:40 `EPS = 1e-6`).
pub const EPS: f64 = 1e-6;

/// Determinant / denominator validity guard for Möller
/// (geometry_exact.py:122 `np.abs(det) > 1e-14`).
pub const DET_EPS: f64 = 1e-14;

/// Ericson `closest_on_tri` denominator regularizer
/// (geometry_exact.py:89 `+ 1e-300`).
pub const CLOSEST_DENOM_REG: f64 = 1e-300;

/// Parity 2D-containment / z-strict-above slack
/// (geometry_exact.py:251,257 `>= -1e-9`, `> pch + 1e-9`).
pub const PARITY_SLACK: f64 = 1e-9;

/// Parity crossing-cluster dedup in axis units
/// (geometry_exact.py:214 `zdedup=5e-3`).
pub const ZDEDUP: f64 = 5e-3;

/// Arc-location dedup tolerance for cast_rays / intervals in mm
/// (geometry_exact.py:134 `tol_mm=3e-3`).
pub const DEDUP_LOC_MM: f64 = 3e-3;

/// segseg / closest den validity guard (geometry_exact.py:298 `> 1e-14`).
pub const SEGSEG_EPS: f64 = 1e-14;

/// Parity den validity guard (geometry_exact.py:234 `np.abs(den) > 1e-14`).
pub const PARITY_DEN_EPS: f64 = 1e-14;

/// sphere_breach near-wall skip margin (geometry_exact.py:842 `nd[i] + 0.05`).
///
/// RETIRED, and deliberately kept.  `breach_one` no longer discards a crossing
/// by absolute distance: a distance cannot tell "this is the near face again"
/// from "this is the far face, and the wall is thin", so the skip threw away
/// both and a sub-0.05 mm feature reported NO BREACH.  The replacement asks the
/// question the skip was a proxy for -- whether the ray runs with or against
/// the face's outward normal -- and is documented at the filter itself in
/// `queries.rs`; `tests/near_wall_thin_feature.rs` is the witness.
///
/// The NAME still carries meaning outside this crate: it is what
/// `crates/plinth-hollow/tests/fixtures/PROVENANCE.md` names as the limit the
/// 0.02 mm thin-fin fixture was cut to straddle, and what
/// `crates/plinth-support/src/contact.rs` distinguishes its own mask margin
/// from.  Deleting the constant would strand those references, so it stays,
/// labelled, rather than being quietly removed.
#[allow(
    dead_code,
    reason = "retired predicate constant kept as the named limit its fixtures and \
              docs still cite; see the doc comment above"
)]
pub const NEAR_WALL_SKIP: f64 = 0.05;

/// Threshold-band telemetry half-width (spec R3 / sec 4.2): a CONTINUOUS
/// decision quantity landing within this of its threshold is counted in
/// `band_counter` (result-inert diagnostics, reported in the full-run log so a
/// float-divergence knife-edge cannot pass unobserved). Scoped to the sec 5.4
/// item-3 continuous quantities: clearance vs 0, thickness vs r−eps, interval
/// length vs min_illegal_len, and the breach reach precondition r−nd vs eps.
pub const THRESHOLD_BAND_MM: f64 = 1e-6;
