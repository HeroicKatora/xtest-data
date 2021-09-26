use std::path::PathBuf;

#[test]
fn integration_test_ourselves() {
    let mut vcs = xtest_data::setup!();
    let datazip = vcs.add("tests/data.zip");
    let testdata = vcs.build();

    let path = testdata.path(&datazip);
    assert!(path.exists(), "{}", path.display());
}

#[test]
fn simple_integration() {
    let mut path = PathBuf::from("tests/data.zip");
    xtest_data::setup!().rewrite([&mut path]).build();
    // 'Magically' changed.
    assert!(path.exists(), "{}", path.display());
}
