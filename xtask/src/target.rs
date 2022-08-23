//! Parse a target's configuration.
use super::{anchor_error, as_io_error, undiagnosed_io_error, LocatedError};

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

use serde::Serialize;
use toml::Value;

/// Full target information.
pub struct Target {
    pub env: TargetStatic,
    pub cargo: Metadata,
}

/// The information available to templates.
#[derive(Serialize)]
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

impl Target {
    pub(crate) fn from_current_dir() -> Result<Self, LocatedError> {
        let toml = std::fs::read("Cargo.toml").map_err(anchor_error())?;
        let toml: Value = toml::de::from_slice(&toml)
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
