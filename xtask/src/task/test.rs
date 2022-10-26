use std::{path::Path, process::Command};

use crate::target::{CrateSource, Target, VcsInfo};
use crate::util::{anchor_error, GoodOutput, LocatedError};
use crate::CARGO;

use super::artifacts::UnpackedArchive;

#[derive(Debug)]
pub struct TestResult {}

pub fn test(
    crate_: &CrateSource,
    target: &Target,
    // FIXME: relax to not use `PackedData` but an optional vcs override and pack path.
    pack: &UnpackedArchive,
    vcs_info: &VcsInfo,
    tmp: &Path,
) -> Result<TestResult, LocatedError> {
    let extracted = tmp.join(target.expected_dir_name());
    // Try to remove it but ignore failure.
    let _ = std::fs::remove_dir_all(&extracted).map_err(anchor_error());

    // gunzip -c target/package/xtest-data-0.0.2.crate
    let crate_tar = Command::new("gunzip")
        .arg("-c")
        .arg(&crate_.path)
        .output()
        .map_err(anchor_error())?
        .stdout;

    // tar -C /tmp --extract --file -
    Command::new("tar")
        .arg("-C")
        .arg(&tmp)
        .args(["--extract", "--file", "-"])
        .input_output(&crate_tar)
        .map_err(anchor_error())?;

    // TMPDIR=/tmp CARGO_XTEST_DATA_FETCH=1 cargo test  -- --nocapture
    Command::new(CARGO)
        .current_dir(&extracted)
        .args(["test", "--no-fail-fast", "--release", "--", "--nocapture"])
        // FIXME! Woah, we may actually have found a caching bug here! When compiling via this
        // source we got outdated binaries that did not reflect the *dirty* changes introduced in
        // the source archive?
        //
        // ]$ rustc --version --verbose
        // rustc 1.61.0 (fe5b13d68 2022-05-18)
        // binary: rustc
        // commit-hash: fe5b13d681f25ee6474be29d748c65adcd91f69e
        // commit-date: 2022-05-18
        // host: x86_64-unknown-linux-gnu
        // release: 1.61.0
        // LLVM version: 14.0.0
        //
        // Anyways we'd like to share the compilation cache.
        // .env("CARGO_TARGET_DIR", repo.join("target"))
        .env("CARGO_XTEST_DATA_TMPDIR", &tmp)
        .env("CARGO_XTEST_DATA_PACK_OBJECTS", &pack.path)
        .envs({
            if let VcsInfo::Overwrite { path } = vcs_info {
                Some(("CARGO_XTEST_VCS_INFO", path))
            } else {
                None
            }
        })
        .success()
        .map_err(anchor_error())?;

    Ok(TestResult {})
}
