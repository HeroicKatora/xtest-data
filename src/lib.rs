mod git;

use std::{fs, path::Path, path::PathBuf};
use serde_json::Value;
use url::Url;

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
    repository: Url,
    manifest: &'static str,
    /// Have we determined to be local or in a crate?.
    source: Source,
    resources: Resources,
    datadir: PathBuf,
}

/// The options determined from the environment.
///
/// This is every environment data we are gather from the `setup` macro, which allows us to get the
/// environment flags passed to the _calling_ crate instead of our own. Please do not construct
/// this directly since doing so could affect the integrity of the information.
#[doc(hidden)]
pub struct EnvOptions {
    pub pkg_repository: &'static str,
    pub manifest_dir: &'static str,
    pub target_tmpdir: Option<&'static str>,
}

/// Perform the configuration of local or remote data.
///
/// This can be ran in _integration tests_ (and in integration tests only) to ensure that those can
/// be replicated from a source distribution of the package, while actually using additional data
/// stored in your repository. The commit ID of the head, stored inside the package, is used for
/// bit-by-bit reproducibility of the test data.
///
/// You can rely on this package only using data within the git tree associated with the commit ID
/// stored in the package. As a tester downstream, if the maintainer of the package signs their
/// crates, and you validate that signature, then by extension and Git's content addressability all
/// data is ensured to have been signed-off by the maintainer.
/// 
/// When developing locally this checks the plausibility of cargo data and then tries to determine
/// if `git` is in use (other VCS are welcome but need to be supported by cargo first).
///
/// ## Panics
///
/// This function _panics_ if any of the following is true:
/// * The function is called outside of an integration test.
///
/// Also, this function **aborts** the process if any of the following are true:
/// * There is no VCS in use.
/// * We could not determine how to use the VCS of the repository.
/// * The repository URL as configured in `Cargo.toml` is not valid.
/// * We could not create a bare repository in the directory `${CARGO_TARGET_TMPDIR}`.
///
/// When executing from the distribution form of a package, we will also abort if any of the
/// following are true:
/// * The commit ID that is being read from `.cargo_vcs_info.json` can not be fetched from the
///   remote repository.
///
/// Note that the eventual call to `build()` has some additional panics and aborts.
#[macro_export]
macro_rules! setup {
    () => {
        $crate::_setup($crate::EnvOptions {
            // FIXME: technically this isn't critical information.
            // We could rely on the user passing one to us since we will fail when that is not a
            // git repository with the correct commit ID. That's just their fault.
            pkg_repository: env!("CARGO_PKG_REPOSITORY"),
            manifest_dir: env!("CARGO_MANIFEST_DIR"),
            target_tmpdir: option_env!("CARGO_TARGET_TMPDIR"),
        })
    }
}

#[doc(hidden)]
pub fn _setup(options: EnvOptions) -> Vcs {
    let EnvOptions {
        pkg_repository: repository,
        manifest_dir: manifest,
        target_tmpdir: tmpdir,
    } = options;
    // Make sure this is an integration test, or at least we have the dir.
    // We don't want to block building over this (e.g. the crate itself here) but we _do_ want to
    // restrict running this `setup` function
    let integration_test_tempdir = tmpdir
        .expect("This setup must only be called in an integration test or benchmark");

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

    if repository.is_empty() {
        inconclusive(&mut "The repository must have a valid URL");
    }

    // Always parse the repository address, ensuring we can access it.
    let repository = repository
        .parse()
        .unwrap_or_else(|mut  err| inconclusive(&mut err));

    Vcs {
        repository,
        manifest,
        source,
        resources: Resources::default(),
        datadir: PathBuf::from(integration_test_tempdir),
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
