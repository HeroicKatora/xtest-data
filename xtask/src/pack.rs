//! Module to create packfile and associated data for a source repository.
use super::{anchor_error, as_io_error, GoodOutput, LocatedError, CARGO};
use crate::target::Target;

use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) struct PackedData {
    pub vcs_info: PathBuf,
    pub pack_path: PathBuf,
    pub crate_path: PathBuf,
}

const GIT: &'static str = "git";

pub(crate) fn pack(repo: &Path, target: &Target, tmp: &Path) -> Result<PackedData, LocatedError> {
    let filename = target.expected_crate_name();
    let repo = repo.canonicalize().map_err(anchor_error())?;
    // FIXME: read cargo metadata for sub folder?
    let crate_path = Path::new("target/package").join(filename);

    let commit = Command::new(GIT)
        .arg("--git-dir")
        .arg(repo.join(".git"))
        .args([
            "show",
            "HEAD",
            "--oneline",
            "--summary",
            "--no-abbrev-commit",
        ])
        .output()
        .map_err(anchor_error())?
        .stdout;

    let commit = commit.split(|&c| c == b' ').next().unwrap();
    let commit = std::str::from_utf8(commit)
        .map_err(as_io_error)
        .map_err(anchor_error())?;

    let packdir = repo.join("target").join("xtest-data");

    Command::new(CARGO)
        .args(["test"])
        .env("CARGO_XTEST_DATA_PACK_OBJECTS", &packdir)
        .success()
        .map_err(anchor_error())?;

    Command::new(CARGO)
        .args(["package", "--allow-dirty", "--no-verify"])
        .success()
        .map_err(anchor_error())?;

    let vcs_info = tmp.join(".xtest_vcs_info.json");
    let vcs_info_data = format!(
        r#"{{ "git": {{ "sha1": "{}" }}, "path_in_vcs": "" }}"#,
        commit
    );

    std::fs::write(&vcs_info, vcs_info_data).map_err(anchor_error())?;

    Ok(PackedData {
        vcs_info,
        // FIXME: depending on Target selection, pack into an archive.
        pack_path: packdir,
        crate_path,
    })
}