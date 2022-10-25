//! Implement the packing specification.
use std::path::{Path, PathBuf};

use crate::{target::Target, task::pack::PackedData, util::LocatedError};

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

pub fn pack(
    data: &UnpackedArchive,
    target: &Target,
    tmp: &Path,
) -> Result<PackedArtifacts, LocatedError> {
    todo!()
}


pub fn unpack(
    data: &PackedArtifacts,
    target: &Target,
    tmp: &Path,
) -> Result<UnpackedArchive, LocatedError> {
    todo!()
}
