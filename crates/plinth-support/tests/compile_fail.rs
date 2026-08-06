#[test]
fn support_domain_invariants_cannot_be_fabricated_outside_the_crate() {
    trybuild::TestCases::new().compile_fail("tests/compile_fail/*.rs");
}
