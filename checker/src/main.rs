//! plinth-check: the independent checker. Empty skeleton (step 1 of the build plan): it
//! does no checking yet.

#![warn(clippy::cast_possible_truncation)]

mod loop_canary;

fn main() {
    // Keeps the canary in the binary so a canary edit relinks it.
    std::hint::black_box(loop_canary::loop_canary());
}

#[cfg(test)]
mod tests {
    use super::loop_canary::loop_canary;

    #[test]
    fn canary_is_stable_within_a_run() {
        let first = loop_canary();
        let second = loop_canary();
        assert_eq!(first, second);
    }
}
