//! Fetch files for a packed file.
use core::fmt;
use std::path::Path;

use crate::{
    target::Target,
    util::{anchor_error, LocatedError},
};

use super::pack_archive::PackedArtifacts;

pub struct Download {
    /// FIXME: change this type to a shared one?
    pub artifact: PackedArtifacts,
}

#[derive(Debug)]
enum DlError {
    NoArtifactLocation,
    TooManyRedirects {
        location: String,
        response: ureq::Response,
    },
    BadRequest {
        location: String,
        response: ureq::Response,
    },
}

pub fn download(target: &Target, tmp: &Path) -> Result<Download, LocatedError> {
    match &target.cargo.pack_artifact {
        None => Err(anchor_error()(DlError::NoArtifactLocation)),
        Some(archive) => {
            let request = ureq::get(archive);
            let response = request.call().map_err(anchor_error())?;

            // Turn HTTP into actions for us.
            // Success = continue, 300-400 report actionable errors, rest non-actionable one.
            match response.status() {
                200..=299 => {}
                300..=399 => {
                    return Err(anchor_error()(DlError::TooManyRedirects {
                        location: archive.to_string(),
                        response,
                    }));
                }
                400..=499 => {
                    return Err(anchor_error()(DlError::BadRequest {
                        location: archive.to_string(),
                        response,
                    }));
                }
                _ => {
                    return Err(anchor_error()(DlError::BadRequest {
                        location: archive.to_string(),
                        response,
                    }));
                }
            }

            let artifact = tmp.join("_vcs_file.tar.gz");
            let mut reader = response.into_reader();

            // We can write over the file
            let mut writer = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&artifact)
                .map_err(anchor_error())?;

            std::io::copy(&mut reader, &mut writer).map_err(anchor_error())?;
            Ok(Download {
                artifact: PackedArtifacts { path: artifact },
            })
        }
    }
}

impl fmt::Display for DlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            DlError::NoArtifactLocation => write!(f, "No `` specified in `Cargo.toml`"),
            DlError::TooManyRedirects { location, response } => {
                write!(
                    f,
                    r#"Server sent too many redirects following artifact location {}.
Try following it with your browser?
Technical details: {status} {status_text}"#,
                    location,
                    status = response.status(),
                    status_text = response.status_text(),
                )
            }
            DlError::BadRequest { location, response } => {
                write!(
                    f,
                    r#"Bad request following artifact location {}
Technical details: {status} {status_text}
{text}"#,
                    location,
                    status = response.status(),
                    status_text = response.status_text(),
                    // FIXME: actual, optional response text?
                    text = "<server response could not be read>",
                )
            }
        }
    }
}

impl std::error::Error for DlError {}
