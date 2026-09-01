//! Minimal 3-vector helpers over `[f64; 3]`. All operations are written to
//! mirror the reference element order of geometry_exact.py; no FMA, no reassociation
//! (spec sec 5.1). Kept tiny and `#[inline]` — but the geometry primitives that
//! feed bit-parity pins live in closest.rs/moller.rs and control their own
//! evaluation order there, not here.
//!
//! # Canon status
//!
//! This module is the surviving 3-vector algebra for the whole workspace
//! (`docs/specs/geometry-canon.md` §1 OP-6, OP-7, OP-9). Every private copy of
//! `add`/`sub`/`scale`/`dot`/`cross`/`norm`/`dist`/`normalize` outside
//! `tools/plinth-audit/` is retired in favour of these, and a site that needs a
//! squared length spells it `dot(sub(a, b), sub(a, b))` rather than growing a
//! fourteenth private helper.
//!
//! # Degenerate-input contract, module-wide
//!
//! * **Nothing here guards non-finite input.** `NaN` and `±inf` components
//!   propagate through every function except [`normalize`], which is the one
//!   place the canon requires a refusal. A caller that must refuse non-finite
//!   input checks it at the call site, in its own error vocabulary.
//! * **Nothing here allocates, panics, or has a fallible path.** The only
//!   function that can decline to answer is [`normalize`], and it does so with
//!   `Option`, never with a sentinel vector.
//! * **No function returns a value meaning "measured nothing" that reads as a
//!   pass.** That is the failure class this consolidation exists to close:
//!   a `[NaN, NaN, NaN]` direction compares `false` against every `<` and `>`
//!   downstream, so a guard reads it as "not violated". See [`normalize`].
//!
//! # Range contract
//!
//! Magnitudes are millimetres on a 218 × 123 mm build plate. [`norm`] and
//! [`dist`] compute `dot(a, a).sqrt()`, which overflows to `inf` and underflows
//! to `0.0` at magnitudes no printable geometry reaches; the exact measured
//! boundaries are on [`norm`]. `hypot` would extend that range and is
//! deliberately NOT used (canon §1 OP-6: it costs a permanent 2-ULP divergence
//! on 20 % of values to buy a range nothing in this engine can occupy).

pub type V3 = [f64; 3];

/// Componentwise `a - b`.
///
/// Degenerate input: none. Non-finite components propagate; `sub(a, a)` is
/// exactly `[0.0, 0.0, 0.0]` for finite `a`, and `[NaN; 3]` for non-finite `a`.
#[inline]
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Componentwise `a + b`.
///
/// Degenerate input: none. Non-finite components propagate.
#[inline]
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Componentwise `a * s`.
///
/// Degenerate input: none. `scale(a, 0.0)` is `[0.0, 0.0, 0.0]` for finite `a`
/// (sign of zero follows IEEE-754), and `[NaN; 3]` where `a` has an infinite
/// component. Non-finite `s` propagates.
#[inline]
pub fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// Left-to-right dot product `a·b`, in the fixed element order every bit-parity
/// pin in this kernel depends on (spec sec 5.1).
///
/// Canonical for the whole workspace wherever the value can reach a comparison
/// or a verdict (canon §1 OP-7). The one surviving FMA dot,
/// `coverage_evidence.rs:927`, is evidence-only and exempt by owner ruling.
///
/// Degenerate input: none. The result overflows to `±inf` for operands beyond
/// ~1e154 and underflows to `0.0` for operands below ~1e-162; both magnitudes
/// are unreachable on a build plate. Non-finite components propagate, including
/// the `inf * 0.0 = NaN` case.
#[inline]
pub fn dot(a: V3, b: V3) -> f64 {
    // Left-to-right association, matching the reference reduction over 3
    // contiguous elements.
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Cross product `a × b`, plain (no FMA), in the pinned element order.
///
/// Degenerate input: **parallel, antiparallel, and zero operands return exactly
/// `[0.0, 0.0, 0.0]`** — there is no error path and no tolerance. A zero cross
/// product is NOT a direction. A caller deriving a normal or an axis from it
/// must test the result before using it, normally by putting it through
/// [`normalize`] and handling `None`; reading `[0, 0, 0]` as a direction is the
/// defect `mesh.rs`'s exact `length <= 0.0` face-normal predicate exists to
/// prevent (canon §1 OP-4).
#[inline]
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Euclidean length `sqrt(a·a)`, in mm for mm input.
///
/// Canonical `norm` for the workspace (canon §1 OP-6). The expanded spelling
/// `(x*x + y*y + z*z).sqrt()` is bit-identical to this one (measured: 0
/// differences over 2,000,000 vectors in a 60 mm cube), so those sites
/// consolidate for free; the `hypot` chains are a different formula and are
/// retired, not merged.
///
/// # Degenerate-input contract
///
/// * `norm([0.0, 0.0, 0.0])` is exactly `0.0`. There is no epsilon.
/// * Any `NaN` component yields `NaN`; any infinite component yields `inf`
///   (`sqrt(inf) = inf`), except that a `NaN` produced by `inf * 0.0` inside
///   the dot yields `NaN`. Non-finite input is NOT refused here — [`normalize`]
///   is the function that refuses.
/// * **Overflow:** the SUM of the squares overflows before any individual
///   square does. Measured: `norm([1e154; 3])` is `inf`, `norm([1e153; 3])` is
///   finite, and `norm([1e200; 3])` is `inf` rather than `1.732e200`.
/// * **Underflow:** each square rounds to zero before the sum. Measured:
///   `norm([1e-162; 3])` is exactly `0.0`, `norm([2e-162; 3])` is not, and
///   `norm([1e-200; 3])` is exactly `0.0` rather than `1.732e-200`.
///
/// Both boundaries are accepted and documented rather than fixed: a 218 mm
/// plate cannot produce either magnitude, and `hypot` would buy that range at
/// the cost of a second formula in the tree.
#[inline]
pub fn norm(a: V3) -> f64 {
    // The reference norm computes sqrt(sum of squares); sqrt is correctly
    // rounded on both sides.
    dot(a, a).sqrt()
}

/// Distance between two points, `norm(sub(a, b))`, in mm for mm input.
///
/// Degenerate input: `dist(a, a)` is exactly `0.0` for finite `a`. The
/// overflow/underflow boundaries of [`norm`] apply to the DIFFERENCE, not to
/// the operands, so two 1e200-magnitude points a millimetre apart still measure
/// 1 mm while their subtraction stays finite. Non-finite input propagates.
#[inline]
pub fn dist(a: V3, b: V3) -> f64 {
    norm(sub(a, b))
}

/// Unit vector in the direction of `a`, or `None` when `a` has no direction.
///
/// Canonical `normalize` for the workspace (canon §1 OP-6, plan item C4). It
/// replaces three incumbent zero-guards, one of which
/// (`plinth-support/src/joints.rs:1575`) had none at all and returned a
/// measured `[NaN, NaN, NaN]` for the zero vector and `[inf, inf, inf]` for
/// `[1e-200; 3]`.
///
/// # Degenerate-input contract
///
/// Returns `None` — never a sentinel vector — exactly when `norm(a)` is not
/// finite or not strictly positive. That is:
///
/// * the zero vector `[0.0, 0.0, 0.0]` (and `[-0.0, -0.0, -0.0]`);
/// * any input with a `NaN` or infinite component;
/// * any input whose squared norm overflows to `inf` (measured: `[1e154; 3]`
///   refuses, `[1e153; 3]` does not);
/// * any input whose squared norm underflows to `0.0` (measured: `[1e-162; 3]`
///   and `[1e-200; 3]` refuse, `[2e-162; 3]` does not). Ruled acceptable rather
///   than fixed with `hypot`: a vector that short does not encode a physical
///   direction in this engine, so a caller needing a direction from one has an
///   upstream defect (canon §5 item 3).
///
/// `Some(u)` guarantees every component of `u` is finite.
///
/// # Where `Some` is NOT a unit vector — read this before trusting the result
///
/// `Some(u)` guarantees `|u| = 1` to within a few ULP **only when `dot(a, a)` is
/// a normal `f64`**, i.e. for components down to about 1.5e-154. Between the
/// underflow-to-zero boundary and that point the squared norm is SUBNORMAL, so
/// it carries too few bits to give a correct length, and the answer is finite
/// but not unit. Measured on the `[m, m, m]` family:
///
/// | input | result |
/// |---|---|
/// | `[1e-162; 3]` | `None` (squared norm is exactly 0) |
/// | `[2e-162; 3]` | `Some`, length **8.998e-1** |
/// | `[1e-158; 3]` | `Some`, length 1.0000000082 |
/// | `[8e-157; 3]` | `Some`, length off by more than 1e-12 |
/// | `[9e-157; 3]` | `Some`, length within 1e-12 of 1 |
/// | `[1e-154; 3]` and above | `Some`, length exactly 1.0 |
///
/// The whole band is ~150 orders of magnitude below anything a 218 mm plate can
/// produce, so nothing printable reaches it — but it is a real gap between this
/// function's guarantee and the canon's prose, which describes the entire
/// sub-1e-154 range as refusing. The canon's RULED FORMULA is implemented here
/// verbatim and unchanged; the discrepancy is between that formula and the
/// sentence describing it, and closing it (by also refusing a subnormal
/// `dot(a, a)`) is a spec amendment, not a consolidation. Recorded rather than
/// silently changed.
///
/// # Spelling, and the ULP it costs
///
/// The body is `scale(a, 1.0 / norm(a))` — one reciprocal and three
/// multiplies, NOT three divisions. The two are not bit-identical: measured,
/// `normalize([3.0, 4.0, 0.0])` is `[0.6000000000000001, 0.8, 0.0]`, where
/// componentwise division gives `[0.6, 0.8, 0.0]`. This spelling is the canon's.
/// A caller retiring a divide-based private copy should expect movement of ≤ 1
/// ULP per component and attribute it here rather than hunting it elsewhere.
///
/// # The contract callers must honour
///
/// **No caller may unwrap `None` into a default vector.** Each maps it to its
/// own typed refusal — `drilling` to `InvalidHoleGeometry`, `drains` to
/// `NonFinite`, `joints` to the typed refusal chosen by the C4 implementer. A
/// model that newly refuses here is the fix working: it was never validly
/// measured before, because a NaN direction reads as "not violated" against
/// every `<` and `>` downstream.
#[inline]
#[must_use]
pub fn normalize(a: V3) -> Option<V3> {
    let n = norm(a);
    (n.is_finite() && n > 0.0).then(|| scale(a, 1.0 / n))
}
