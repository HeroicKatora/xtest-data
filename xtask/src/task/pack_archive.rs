//! Implement the packing specification.
use core::fmt;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    target::{ArchiveMethod, Target},
    util::{anchor_error, GoodOutput, LocatedError},
};

#[derive(Debug)]
pub struct PackedArtifacts {
    /// Path to a file containing the final archive.
    ///
    /// Upload this to CI so that it is available at the path indicate in your `Cargo.toml`!
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct UnpackedArchive {
    /// Path to a tree structure containing VCS object files.
    ///
    /// Import them to the VCS(git) to be able to recreate the commit state and checkout files
    /// associated with a precise snapshot.
    pub path: PathBuf,
}

#[derive(Debug)]
enum PackError {
    NoPackSpecification,
}

pub fn pack(
    data: &UnpackedArchive,
    target: &Target,
    tmp: &Path,
) -> Result<PackedArtifacts, LocatedError> {
    let ArchiveMethod::TarGz = target
        .cargo
        .pack_archive
        .as_ref()
        .ok_or_else(|| anchor_error()(PackError::NoPackSpecification))?;

    // Invert: tar -C /tmp --extract --file -
    let create_tar = Command::new("tar")
        .args(["--create", "--file", "-"])
        .arg("-C")
        .arg(&data.path)
        .arg(".")
        .input_output(&[])
        .map_err(anchor_error())?
        .stdout;

    // Invert: gunzip -c target/package/xtest-data-0.0.2.crate
    let crate_gz = Command::new("gzip")
        .arg("-c")
        .input_output(&create_tar)
        .map_err(anchor_error())?
        .stdout;

    let artifact = tmp.join("artifact.tar.gz");
    let () = std::fs::write(&artifact, &crate_gz).map_err(anchor_error())?;

    Ok(PackedArtifacts {
        path: artifact,
    })
}

/// Turn one artifact file into a source directory of artifacts.
pub fn unpack(
    pack: &PackedArtifacts,
    target: &Target,
    tmp: &Path,
) -> Result<UnpackedArchive, LocatedError> {
    let ArchiveMethod::TarGz = target
        .cargo
        .pack_archive
        .as_ref()
        .ok_or_else(|| anchor_error()(PackError::NoPackSpecification))?;

    // gunzip -c target/package/xtest-data-0.0.2.crate
    let crate_tar = Command::new("gunzip")
        .arg("-c")
        .arg(&pack.path)
        .output()
        .map_err(anchor_error())?
        .stdout;

    let target = tmp.join("artifacts");
    std::fs::create_dir(&target).map_err(anchor_error())?;

    // tar -C /tmp --extract --file -
    Command::new("tar")
        .arg("-C")
        .arg(&target)
        .args(["--extract", "--file", "-"])
        .input_output(&crate_tar)
        .map_err(anchor_error())?;

    Ok(UnpackedArchive { path: target })
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            PackError::NoPackSpecification => write!(f, "No `` specified in `Cargo.toml`"),
        }
    }
}

impl std::error::Error for PackError {}
