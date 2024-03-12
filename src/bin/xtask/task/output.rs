use std::path::PathBuf;

use super::artifacts::PackedArtifacts;
use crate::{
    target::{LocalSource, Target},
    util::{anchor_error, LocatedError},
};

pub fn write_artifacts(
    source: &LocalSource,
    target: &Target,
    packed: &PackedArtifacts,
) -> Result<PathBuf, LocatedError> {
    let target_dir = source.target_directory(target);
    let () = std::fs::create_dir_all(&target_dir).map_err(anchor_error())?;

    // Base the name off the naming schema for `.crate` files.
    let name = {
        let mut crate_ = target.expected_crate_name();
        crate_.set_extension("xtest-data");
        crate_
    };

    let target = target_dir.join(name);
    let _n = std::fs::copy(&packed.path, &target).map_err(anchor_error())?;

    Ok(target)
}
