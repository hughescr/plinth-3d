//! Minimal 3-vector helpers over `[f64; 3]`. All operations are written to
//! mirror the reference element order of geometry_exact.py; no FMA, no reassociation
//! (spec sec 5.1). Kept tiny and `#[inline]` — but the geometry primitives that
//! feed bit-parity pins live in closest.rs/moller.rs and control their own
//! evaluation order there, not here.

pub type V3 = [f64; 3];

#[inline]
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
pub fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[inline]
pub fn dot(a: V3, b: V3) -> f64 {
    // Left-to-right association, matching the reference reduction over 3
    // contiguous elements.
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
pub fn norm(a: V3) -> f64 {
    // The reference norm computes sqrt(sum of squares); sqrt is correctly
    // rounded on both sides.
    dot(a, a).sqrt()
}

#[inline]
pub fn dist(a: V3, b: V3) -> f64 {
    norm(sub(a, b))
}
