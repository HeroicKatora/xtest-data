#[test]
fn integration_test_ourselves() {
    let mut vcs = xtest_data::setup!();
    let datazip = vcs.file("tests/data.zip");
    let testdata = vcs.build();

    let path = testdata.file(&datazip);
    assert!(path.exists(), "{}", path.display());
}
