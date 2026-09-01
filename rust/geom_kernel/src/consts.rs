//! Named tolerance constants, copied VERBATIM from geometry_exact.py with a
//! comment linking each to its source line. Changing any of these is a parity
//! break; they are the decision skins the whole suite pins.
//!
//! # These are CHOSEN BEHAVIOR PINS, not derivations
//!
//! `docs/specs/geometry-adjudication.md` §O5 ruled on the labels this module
//! used to carry. "Oracle-pinned" is a PROVENANCE record — it says where a
//! number came from, not why it is right. Under this project's physics-first
//! doctrine a constant is clean only when it provably cannot decide an outcome
//! (a prune), or when the uncertainty it bounds routes to refusal. A chosen
//! constant that decides an outcome is doctrine DEBT even when its label is
//! honest.
//!
//! Every value below is therefore labelled "chosen behavior pin: value carried
//! verbatim from `<oracle location>` for parity; not derived", and each entry
//! says whether it can decide an outcome. **No value changed** when the labels
//! were corrected; the doctrine debt register lives in §O5 of the adjudication.

/// Möller barycentric / t-parameter slack.
///
/// **Chosen behavior pin:** value carried verbatim from geometry_exact.py:40
/// (`EPS = 1e-6`) for parity; not derived.
///
/// DECIDES an outcome, at every acceptance-region edge: it sets the barycentric
/// shell width and the `t` slack. The shared-edge-consistency rationale in
/// `moller.rs` argues the DIRECTION of the slack, not the magnitude 1e-6; no
/// measurement establishes 1e-6 over 1e-5 or 1e-7. Debt register entry D1.
pub const EPS: f64 = 1e-6;

/// Determinant validity guard for Möller.
///
/// **Chosen behavior pin:** value carried verbatim from geometry_exact.py:122
/// (`np.abs(det) > 1e-14`) for parity; not derived.
///
/// DECIDED containment parity until §O4: a nonzero-band interior crossing was
/// asserted a miss, and the claim in `moller.rs` that the guard was "a PRUNE —
/// not a verdict" is measured FALSE for that configuration
/// (`tests/ray_class.rs`). On the DECISION path the band now routes to
/// [`crate::RayTriangleClass::NearParallel`], which every caller maps to its
/// refusal side. What remains debt (register entry D2) is the magnitude: the
/// comparison is against `2A·|dv|·cos θ`, so where the band BEGINS still moves
/// with the representation of the direction vector.
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

/// Segment-vs-segment DENOMINATOR validity guard, in **mm⁴**.
///
/// **Chosen behavior pin:** value carried verbatim from geometry_exact.py:298
/// (`> 1e-14`) for parity; not derived.
///
/// Guards `denom = |u|²|v|² − (u·v)²` in
/// [`crate::segment_segment_distance`], which is an mm⁴ quantity.
///
/// # Why the name split (§O2)
///
/// The one constant `SEGSEG_EPS` used to guard `denom` (mm⁴) AND `a`/`e`
/// (mm²). One number cannot be a correct threshold in two different
/// dimensions, and the mm² use had a measured consequence: a genuinely nonzero
/// segment of length 1e-7 mm has `a = 9.999999999999998e-15`, fails
/// `a > 1e-14`, and is treated as zero-length. The split is **byte-neutral by
/// construction** — the NAME split, not the value; both literals stay `1e-14`,
/// so every parity pin holds and only the contract stops lying.
///
/// [`crate::segment_segment_distance_min`] uses NEITHER: it has no epsilon at
/// all.
pub const SEGSEG_DENOM_EPS_MM4: f64 = 1e-14;

/// Segment SQUARED-LENGTH degeneracy guard, in **mm²**.
///
/// **Chosen behavior pin:** value carried verbatim from geometry_exact.py:298
/// (`> 1e-14`) for parity; not derived. See
/// [`SEGSEG_DENOM_EPS_MM4`] for why one name became two.
///
/// Guards `a = |p1q1|²` and `e = |p2q2|²` in
/// [`crate::segment_segment_distance`]: below this a segment is treated as a
/// point.
pub const SEGSEG_LEN2_EPS_MM2: f64 = 1e-14;

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
