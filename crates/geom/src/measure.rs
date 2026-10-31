//! The answer of a batch measurement that may be refused (design `step11-construction.md`
//! section 10, step 0).
//!
//! A batch function that is not built yet, or cannot decide one item, answers
//! [`Measure::Unverified`] for that item. It never answers a favourable number. Every batch
//! function fills one answer per input item, in input order.

/// One measured value, or the reason it was not measured.
#[derive(Debug, Clone, PartialEq)]
pub enum Measure<T> {
    /// The measurement.
    Known(T),
    /// Not measured, and why.
    Unverified(&'static str),
}

impl<T> Measure<T> {
    /// The value, if it was measured.
    #[must_use]
    pub fn known(&self) -> Option<&T> {
        match self {
            Self::Known(v) => Some(v),
            Self::Unverified(_) => None,
        }
    }

    /// True when the measurement was refused.
    #[must_use]
    pub fn is_unverified(&self) -> bool {
        matches!(self, Self::Unverified(_))
    }
}

/// Refill `out` with `n` copies of `Unverified(why)`: the answer of a batch function that is
/// not built yet.
pub fn fill_unverified<T>(out: &mut Vec<Measure<T>>, n: usize, why: &'static str) {
    out.clear();
    out.extend((0..n).map(|_| Measure::Unverified(why)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_gives_one_refusal_per_item() {
        let mut out: Vec<Measure<f64>> = vec![Measure::Known(1.0)];
        fill_unverified(&mut out, 3, "not built");
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(Measure::is_unverified));
        assert_eq!(out[0].known(), None);
    }
}
