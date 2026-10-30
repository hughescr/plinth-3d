//! The `plinth` binary. Empty skeleton (step 1 of the build plan): it does no work yet.

fn main() {
    // Keeps the canary chain in the binary so a canary edit relinks it.
    std::hint::black_box(plinth_cli::canary_chain());
}
