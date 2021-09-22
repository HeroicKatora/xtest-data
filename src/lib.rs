mod git;

use std::{fs, path::Path, path::PathBuf};
use serde_json::Value;

#[derive(Debug)]
pub struct File {
    local_path: &'static str,
}

#[derive(Debug)]
pub struct Tree {
    local_path: &'static str,
}

/// The product of `Vcs`, ensuring local file system accessible test resources.
pub struct FsData {
}

#[derive(Debug)]
enum Source {
    /// The data source is the crate's repository at a specific commit id.
    VcsFromManifest {
        /// TODO: we should support other commit identifiers.
        commit_id: [u8; 20],
        /// Evidence how we plan to access the source.
        git: git::Git,
    },
    /// The data will be relative to the crate manifest.
    Local,
}

#[derive(Default, Debug)]
struct Resources {
    relative_files: Vec<&'static Path>,
    relative_dirs: Vec<&'static Path>,
}

/// A builder for test data.
///
/// On a VCS copy of the surrounding package this will simply collect and validate the information.
/// However, when executed in an unpacked `.crate` then, instead, we provide a detailed report of
/// necessary data before we abort.
#[must_use = "This is only a builder. Call `build` to perform validation/fetch/etc."]
#[derive(Debug)]
pub struct Vcs {
    repository: &'static str,
    manifest: &'static str,
    /// Have we determined to be local or in a crate?.
    source: Source,
    resources: Resources,
}

/// Perform the configuration of local or remote data.
///
/// When developing locally this checks the plausibility of cargo data and then tries to determine
/// if `git` is in use (other VCS are welcome but need to be supported by cargo first).
///
/// ## Panics
///
/// There is no VCS in use.
pub fn setup() -> Vcs {
    let repository = env!("CARGO_PKG_REPOSITORY");
    let manifest = env!("CARGO_MANIFEST_DIR");

    let vcs_info_path = Path::new(manifest).join(".cargo_vcs_info.json");

    let source = if vcs_info_path.exists() {
        let data = fs::read_to_string(vcs_info_path)
            .unwrap_or_else(|mut err| inconclusive(&mut err));
        let vcs: Value = serde_json::from_str(&data)
            .unwrap_or_else(|mut err| inconclusive(&mut err));
        let commit_id: [u8; 20] = vcs
            .get("git")
            .unwrap_or_else(|| inconclusive(&mut "VCS does not contain a git section."))
            .get("sha1")
            .unwrap_or_else(|| inconclusive(&mut "VCS commit ID not recognized."))
            .as_str()
            .map(|st| {
                use hex::FromHex;
                <[u8; 20]>::from_hex(st).unwrap_or_else(|mut err| inconclusive(&mut err))
            })
            .unwrap_or_else(|| inconclusive(&mut "VCS commit ID is not a string"));

        // Okay, that makes sense. We know _what_ to access.
        // Now let's also try to find out how we will access it. Let's find `git`.
        // To shell out to because we are lazy.
        let git = git::Git::new()
            .unwrap_or_else(|mut err| inconclusive(&mut err));

        Source::VcsFromManifest {
            commit_id,
            git,
        }
    } else {
        Source::Local
    };

    Vcs {
        repository,
        manifest,
        source,
        resources: Resources::default(),
    }
}

impl Vcs {
    pub fn file(&mut self) -> File {
        todo!()
    }

    pub fn tree(&mut self) -> Tree {
        todo!()
    }

    pub fn build(self) -> FsData {
        todo!()
    }
}

impl File {
    pub fn to_path(&self, fs: &FsData) -> PathBuf {
        todo!()
    }
}

impl Tree {
    pub fn to_path(&self, fs: &FsData) -> PathBuf {
        todo!()
    }
}

#[cold]
fn inconclusive(err: &mut dyn std::fmt::Display) -> ! {
    eprintln!("xtest-data failed to setup.");
    eprintln!("Information: {}", err);
    std::process::abort()
}
