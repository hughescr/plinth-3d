#[test]
fn sphere_semantics_cannot_be_fabricated_outside_the_crate() {
    trybuild::TestCases::new().compile_fail("tests/compile_fail/*.rs");
}
