#[cfg(all(not(miri), not(coverage_nightly), feature = "alloc"))]
#[test]
fn compilation() {
    let t = trybuild::TestCases::new();
    t.pass("tests/compilation/not-local.rs");
    t.compile_fail("tests/compilation/local.rs");
}
