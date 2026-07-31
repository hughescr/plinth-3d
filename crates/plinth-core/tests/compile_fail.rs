#[test]
fn placeholder_cannot_reach_acceptance() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/compile_fail/*.rs");
}
