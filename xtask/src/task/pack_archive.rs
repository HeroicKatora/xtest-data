use std::path::{Path, PathBuf};

use crate::{target::Target, task::pack::PackedData, util::LocatedError};

pub struct PackArchive {
    /// Path to a file containing the final archive.
    ///
    /// Upload this to CI so that it is available at the path indicate in your `Cargo.toml`!
    pub path: PathBuf,
}

pub fn pack(data: &PackedData, target: &Target, tmp: &Path) -> Result<PackArchive, LocatedError> {
    todo!()
}
