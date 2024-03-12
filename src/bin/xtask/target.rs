//! Parse a target's configuration.
use crate::util::GoodOutput;

use super::{anchor_error, as_io_error, undiagnosed_io_error, LocatedError};

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use toml::Value;

/// A local file tree containing a source folder.
pub struct LocalSource {
    pub cargo: PathBuf,
    /// Allow this source tree to be dirty? May be best-effort.
    pub dirty: bool,
}

/// A local path to a `.crate` archive.
pub struct CrateSource {
    pub path: PathBuf,
}

pub enum VcsInfo {
    FromCrate,
    Overwrite { path: PathBuf },
}

/// Full target information.
#[derive(Debug)]
pub struct Target {
    pub env: TargetStatic,
    pub cargo: Metadata,
}

/// The information available to templates.
#[derive(Debug, Serialize)]
pub struct TargetStatic {
    pub name: String,
    pub version: String,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

/// The derived metadata read and interpreted from `Cargo.toml`'s metadata section.
#[derive(Default, Debug)]
pub struct Metadata {
    /// Artifact archival method.
    pub pack_archive: Option<ArchiveMethod>,
    /// Artifact URL template.
    pub pack_artifact: Option<String>,
    /// Relative path of location for pack objects.
    /// Suggested: `target/xtest-data` or `target/xtest-data-pack`.
    pub pack_objects: Option<String>,
}

/// Determine how the pack objects are archived.
#[derive(Debug)]
pub enum ArchiveMethod {
    TarGz,
}

impl LocalSource {
    pub fn with_simple_repository(path: &Path) -> Self {
        LocalSource {
            cargo: path.join("Cargo.toml"),
            dirty: false,
        }
    }

    pub fn with_dirty(self, dirty: bool) -> Self {
        LocalSource { dirty, ..self }
    }

    pub fn target_directory(&self, _: &Target) -> PathBuf {
        // FIXME: use metadata for actual target directory.
        self.cargo.parent().unwrap().join("target/xtest-data")
    }
}

impl Target {
    pub(crate) fn from_dir(spec: &LocalSource) -> Result<Self, LocatedError> {
        let toml = std::fs::read(&spec.cargo).map_err(anchor_error())?;
        Self::from_toml(&toml)
    }

    pub(crate) fn from_crate(archive: &CrateSource) -> Result<Self, LocatedError> {
        let crate_tar = Command::new("gunzip")
            .arg("-c")
            .arg(&archive.path)
            .output()
            .map_err(anchor_error())?
            .stdout;

        let toml = Command::new("tar")
            .arg("-O")
            .args(["--extract", "--file", "-", "--wildcards", "*/Cargo.toml"])
            .input_output(&crate_tar)
            .map_err(anchor_error())?;

        Self::from_toml(&toml.stdout)
    }

    pub(crate) fn from_toml(toml: &[u8]) -> Result<Self, LocatedError> {
        let toml = core::str::from_utf8(toml).map_err(anchor_error())?;

        let toml: Value = toml::de::from_str(toml)
            .map_err(as_io_error)
            .map_err(anchor_error())?;

        let package = toml
            .get("package")
            .ok_or_else(undiagnosed_io_error())
            .map_err(anchor_error())?;
        let name = package
            .get("name")
            .ok_or_else(undiagnosed_io_error())
            .map_err(anchor_error())?
            .as_str()
            .ok_or_else(undiagnosed_io_error())
            .map_err(anchor_error())?
            .to_owned();
        let version = package
            .get("version")
            .ok_or_else(undiagnosed_io_error())
            .map_err(anchor_error())?
            .as_str()
            .ok_or_else(undiagnosed_io_error())
            .map_err(anchor_error())?
            .to_owned();

        let mut target = Target {
            env: TargetStatic {
                name,
                version,
                extra: {
                    let map = package
                        .as_table()
                        .ok_or_else(undiagnosed_io_error())
                        .map_err(anchor_error())?
                        .clone();
                    map.into_iter().collect()
                },
            },
            cargo: Metadata::default(),
        };

        if let Some(meta) = package.get("metadata").and_then(|v| v.get("xtest-data")) {
            target.cargo = Metadata::from_value(meta, &target)?;
        };

        Ok(target)
    }

    pub fn expected_crate_name(&self) -> PathBuf {
        format!("{}-{}.crate", &self.env.name, &self.env.version).into()
    }

    pub fn expected_dir_name(&self) -> PathBuf {
        format!("{}-{}", &self.env.name, &self.env.version).into()
    }
}

impl Metadata {
    pub(crate) fn from_value(val: &Value, target: &Target) -> Result<Self, LocatedError> {
        let mut table = val
            .as_table()
            .ok_or_else(|| {
                let err =
                    io::Error::new(io::ErrorKind::Other, "Expected metadata.xtest-data table");
                anchor_error()(err)
            })?
            .clone();

        let mut meta = Metadata::default();
        let mut template = tinytemplate::TinyTemplate::new();
        let (artifact_src, object_src);

        if let Some(archive) = table.remove("pack-archive") {
            match archive.as_str() {
                Some("tar:gz") => {
                    meta.pack_archive = Some(ArchiveMethod::TarGz);
                }
                _ => {
                    let err = io::Error::new(io::ErrorKind::Other, "Unknown archive method");
                    return Err(anchor_error()(err));
                }
            }
        }

        if let Some(artifact) = table.remove("pack-artifact") {
            if let Some(artifact) = artifact.as_str() {
                artifact_src = artifact.to_string();
                let _ = template.add_template("__main__", &artifact_src);
                let artifact = template
                    .render("__main__", &target.env)
                    .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
                    .map_err(anchor_error())?;
                meta.pack_artifact = Some(artifact);
            } else {
                let err = io::Error::new(
                    io::ErrorKind::Other,
                    "Bad value for `pack-artifact`, expected string",
                );
                return Err(anchor_error()(err));
            }
        }

        if let Some(objects) = table.remove("pack-objects") {
            if let Some(objects) = objects.as_str() {
                object_src = objects.to_string();
                let _ = template.add_template("__main__", &object_src);
                let objects = template
                    .render("__main__", &target.env)
                    .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
                    .map_err(anchor_error())?;
                meta.pack_objects = Some(objects);
            } else {
                let err = io::Error::new(
                    io::ErrorKind::Other,
                    "Bad value for `pack-objects`, expected string",
                );
                return Err(anchor_error()(err));
            }
        }

        Ok(meta)
    }
}
