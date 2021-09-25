//! Fetch test data in packaged crate tests.
//!
//! # For crate authors
//!
//! Drop these lines into your _integration tests_ (due to a limitation in `cargo` this will only
//! work in integration tests right now¹). Note that this requires your repository—through the URL
//! contained in `Cargo.toml`—to be readable by the environment where you wish to test the packaged
//! crate.
//!
//! ```rust
//! let mut vcs = xtest_data::setup!();
//! // or any other file you want to use.
//! let datazip = vcs.file("tests/data.zip");
//! let testdata = vcs.build();
//!
//! // access its path via the xtest-data object, not directly
//! let path = testdata.file(&datazip);
//! // … and the crate works its magic to make this succeed.
//! assert!(path.exists(), "{}", path.display());
//! ```
//!
//! # For packagers
//!
//! The `.crate` file you have downloaded is a `.tar.gz` in disguise. When you unpack it for your
//! local build steps etc., verify that this package contains `Cargo.toml.orig` as well as a
//! `.cargo_vcs_info.json` file; and that the latter file has git commit information.
//!
//! Then you can then run the tests:
//!
//! ```bash
//! cargo test -- --nocapture
//! ```
//!
//! Don't worry, this won't access the network yet.  In the first step it will only verify the
//! basic installation. It will then panic while printing information on what it _would have_ done
//! and instructions on how to proceed. You can opt into allow network access by default with:
//!
//! ```bash
//! CARGO_XTEST_DATA_FETCH=yes cargo test -- --nocapture
//! ```
//!
//! ¹We need a place to store a shallow clone of the crate's source repository.
#![forbid(unsafe_code)]
mod git;

use std::{borrow::Cow, env, fs, ffi::OsString, path::Path, path::PathBuf};
use serde_json::Value;
use slotmap::{DefaultKey, SecondaryMap, SlotMap};

/// A file that was registered from [`Vcs`].
///
/// This is a key into [`FsData`]. You can retrieve the local path using [`FsData::file()`]. The
/// returned path is either the local path on disk, when you are currently developing under a local
/// checkout of the version control system, or the path into which the file has been checked out.
#[derive(Debug)]
pub struct File {
    key: DefaultKey,
}

/// A key into [`FsData`] that was previously registered from [`Vcs`].
///
/// This is a key into [`FsData`]. You can retrieve the local path using [`FsData::tree()`]. The
/// returned path is either the local path on disk, when you are currently developing under a local
/// checkout of the version control system, or the path into which the whole tree has been checked
/// out.
#[derive(Debug)]
pub struct Tree {
    key: DefaultKey,
}

#[derive(Debug)]
enum Managed {
    FilePath(PathBuf),
    Tree(PathBuf),
}

#[derive(Debug)]
pub enum FsItem<'lt> {
    File(&'lt mut PathBuf),
    Tree(&'lt mut PathBuf),
}

/// The product of `Vcs`, ensuring local file system accessible test resources.
///
/// This object is used to retrieve the local paths of resources that have been registered with the
/// methods [`Vcs::file()`] and [`Vcs::tree()`] before.
#[derive(Debug)]
pub struct FsData {
    /// Map all configured items to their paths.
    /// This map will essentially be constant and we do not care about the VCS interpretation.
    map: SecondaryMap<DefaultKey, PathBuf>,
}

#[derive(Debug)]
enum Source {
    /// The data source is the crate's repository at a specific commit id.
    VcsFromManifest {
        /// TODO: we should support other commit identifiers.
        commit_id: git::CommitId,
        /// Evidence how we plan to access the source.
        git: git::Git,
        /// The directory where we may put git-dir and checkout of the resources.
        datadir: PathBuf,
    },
    /// The data will be relative to the crate manifest.
    Local(git::Git),
}

#[derive(Default, Debug)]
struct Resources<'paths> {
    relative_files: SlotMap<DefaultKey, Managed>,
    /// Resources where we do 'simple' path replacement in a filter style.
    unmanaged: Vec<FsItem<'paths>>,
}

/// A builder for test data.
///
/// On a VCS copy of the surrounding package this will simply collect and validate the information.
/// However, when executed in an unpacked `.crate` then, instead, we provide a detailed report of
/// necessary data before we abort.
#[must_use = "This is only a builder. Call `build` to perform validation/fetch/etc."]
#[derive(Debug)]
pub struct Vcs<'paths> {
    repository: OsString,
    manifest: &'static str,
    /// Have we determined to be local or in a crate?.
    source: Source,
    /// The resources that we store.
    resources: Resources<'paths>,
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
/// * There is no VCS in use.
/// * We could not determine how to use the VCS of the repository.
/// * The repository URL as configured in `Cargo.toml` is not valid.
/// * We could not create a bare repository in the directory `${CARGO_TARGET_TMPDIR}`.
///
/// When executing from the distribution form of a package, we will also panic if any of the
/// following are true:
/// * The commit ID that is being read from `.cargo_vcs_info.json` can not be fetched from the
///   remote repository.
/// * There is no `.cargo_vcs_info.json` and the manifest is _not_ in a VCS folder.
///
/// Note that the eventual call to `build()` has some additional panics.
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
pub fn _setup(options: EnvOptions) -> Vcs<'static> {
    let EnvOptions {
        pkg_repository: repository,
        manifest_dir: manifest,
        target_tmpdir: tmpdir,
    } = options;
    if repository.is_empty() {
        inconclusive(&mut "The crate must have a valid URL in `package.repository`");
    }

    // Now allow the override.
    let repository = env::var_os("CARGO_XTEST_DATA_REPOSITORY_ORIGIN")
        .unwrap_or_else(|| OsString::from(repository));

    // Make sure this is an integration test, or at least we have the dir.
    // We don't want to block building over this (e.g. the crate itself here) but we _do_ want to
    // restrict running this `setup` function
    let integration_test_tempdir = tmpdir.map(Path::new);

    let vcs_info_path = Path::new(manifest).join(".cargo_vcs_info.json");

    let source = if vcs_info_path.exists() {
        // Allow the override.

        let data = fs::read_to_string(vcs_info_path)
            .unwrap_or_else(|mut err| inconclusive(&mut err));
        let vcs: Value = serde_json::from_str(&data)
            .unwrap_or_else(|mut err| inconclusive(&mut err));
        let commit_id = vcs
            .get("git")
            .unwrap_or_else(|| inconclusive(&mut "VCS does not contain a git section."))
            .get("sha1")
            .unwrap_or_else(|| inconclusive(&mut "VCS commit ID not recognized."))
            .as_str()
            .map(git::CommitId::from)
            .unwrap_or_else(|| inconclusive(&mut "VCS commit ID is not a string"));

        // Okay, that makes sense. We know _what_ to access.
        // Now let's also try to find out how we will access it. Let's find `git`.
        // To shell out to because we are lazy.
        let git = git::Git::new()
            .unwrap_or_else(|mut err| inconclusive(&mut err));

        let datadir = integration_test_tempdir
            .map(Cow::Borrowed)
            .or_else(|| {
                    let environment_temp = std::env::var_os("CARGO_XTEST_DATA_TMPDIR")
                        .or_else(|| std::env::var_os("TMPDIR"))
                        .map(PathBuf::from)?;
                    // TODO: nah, in this case we should have some distinguisher for the exact crate
                    // name and version in the tmpdir. At least that would catch the gravest of errors
                    // when testing many crates at the same time. (Although sharing the git dir would
                    // be an advantage).
                    Some(Cow::Owned(environment_temp))
                })
            .expect("This setup must only be called in an integration test or benchmark, or with an explicit TMPDIR")
            .into_owned();

        Source::VcsFromManifest {
            commit_id,
            git,
            datadir,
        }
    } else {
        // Check that we can recognize tracked files.
        let git = git::Git::new()
            .unwrap_or_else(|mut err| inconclusive(&mut err));
        Source::Local(git)
    };

    // And finally this must be valid.
    if repository.is_empty() {
        inconclusive(&mut "The repository must have a valid URL");
    }

    Vcs {
        repository,
        manifest,
        source,
        resources: Resources::default(),
    }
}

impl<'lt> Vcs<'lt> {
    /// Register some paths to rewrite to their location.
    ///
    /// The paths will be registered internally. If the repository is local they will be rewritten
    /// to be relative to the manifest location. If the repository is a crate distribution then the
    /// paths will be sparsely checked out (meaning: only that path will be downloaded from the VCS
    /// working dir and you can't expect any other files to be present).
    ///
    /// Both of those actions will happen when you call [`build()`].
    pub fn filter(mut self, iter: impl IntoIterator<Item=FsItem<'lt>>) -> Self {
        self.resources.unmanaged.extend(iter);
        self
    }


    /// Register the path of a file.
    ///
    /// The return value is a key that can later be used in [`FsData`].
    pub fn file(&mut self, path: impl AsRef<Path>) -> File {
        fn path_impl(resources: &mut Resources, path: &Path) -> DefaultKey {
            let item = Managed::FilePath(path.to_owned());
            resources.relative_files.insert(item)
        }

        let key = path_impl(&mut self.resources, path.as_ref());
        File { key }
    }

    pub fn tree(&mut self, path: impl AsRef<Path>) -> Tree {
        fn path_impl(resources: &mut Resources, path: &Path) -> DefaultKey {
            let item = Managed::Tree(path.to_owned());
            resources.relative_files.insert(item)
        }

        let key = path_impl(&mut self.resources, path.as_ref());
        Tree { key }
    }

    /// Run the final validation and return the frozen dictionary of file data.
    ///
    /// ## Panics
    ///
    /// This will panic if:
    /// * Any registered file or tree is not tracked in the VCS.
    /// * You have not allowed retrieving data from the VCS.
    /// * It was not possible to retrieve the data from the VCS.
    pub fn build(self) -> FsData {
        let mut map;
        match self.source {
            Source::Local(git) => {
                let dir = git::CrateDir::new(self.manifest, &git);
                let datapath = Path::new(self.manifest);
                dir.tracked(&git, &mut self.resources.path_specs());
                map = SecondaryMap::new();
                self.resources.relative_files
                    .iter()
                    .for_each(|(key, path)| {
                        map.insert(key, datapath.join(path.as_path()));
                    });
                self.resources.unmanaged
                    .into_iter()
                    .for_each(|item| item.root(&datapath));
            }
            Source::VcsFromManifest { commit_id, datadir, git, } => {
                let origin = git::Origin { url: self.repository };

                let gitpath = datadir.join("xtest-data-git");
                let datapath = datadir.join("xtest-data-tree");

                git.consent_to_use(
                    &gitpath,
                    &datapath,
                    &origin,
                    &commit_id,
                    &mut self.resources.as_paths(),
                    &mut self.resources.path_specs());

                fs::create_dir_all(&datapath).unwrap_or_else(|mut err| inconclusive(&mut err));
                let shallow = git.shallow_clone(gitpath, origin);

                shallow.fetch(&git, &commit_id);
                shallow.checkout(&git, &datapath, &commit_id, &mut self.resources.path_specs());
                map = SecondaryMap::new();
                self.resources.relative_files
                    .iter()
                    .for_each(|(key, path)| {
                        map.insert(key, datapath.join(path.as_path()));
                    });
                self.resources.unmanaged
                    .into_iter()
                    .for_each(|item| item.root(&datapath));
            }
        }

        // In the end we just discard some information.
        // We don't really need it anymore after the checks.
        //
        // TODO: of course we could avoid actually checking files onto the disk if we had some kind
        // of `io::Read` abstraction that read them straight from `git cat` instead. But chances
        // are you'll like your files and directory structures.
        FsData {
            map,
        }
    }
}

impl Resources<'_> {
    pub fn as_paths(&self) -> impl Iterator<Item=&'_ Path> {
        let values = self.relative_files.values().map(Managed::as_path);
        let unmanaged = self.unmanaged.iter().map(FsItem::as_path);
        values.chain(unmanaged)
    }

    pub fn path_specs(&self) -> impl Iterator<Item=git::PathSpec<'_>> {
        let values = self.relative_files.values().map(Managed::as_path_spec);
        let unmanaged = self.unmanaged.iter().map(FsItem::as_path_spec);
        values.chain(unmanaged)
    }
}

impl FsData {
    pub fn file(&self, file: &File) -> &Path {
        self.map.get(file.key).unwrap().as_path()
    }

    pub fn tree(&self, tree: &Tree) -> &Path {
        self.map.get(tree.key).unwrap().as_path()
    }
}

impl Managed {
    pub fn as_path(&self) -> &Path {
        match self {
            Managed::Tree(path) | Managed::FilePath(path) => path,
        }
    }

    fn as_path_spec(&self) -> git::PathSpec<'_> {
        match self {
            Managed::FilePath(path) => git::PathSpec::Path(path),
            // FIXME: more accurate would be to have a spec for the glob `<dir>/**`.
            Managed::Tree(path) => git::PathSpec::Path(path),
        }
    }
}

impl FsItem<'_> {
    pub fn as_path(&self) -> &Path {
        match self {
            FsItem::Tree(path) | FsItem::File(path) => path,
        }
    }

    fn as_path_spec(&self) -> git::PathSpec<'_> {
        match self {
            FsItem::File(path) => git::PathSpec::Path(path),
            // FIXME: more accurate would be to have a spec for the glob `<dir>/**`.
            FsItem::Tree(path) => git::PathSpec::Path(path),
        }
    }

    fn root(self, root: &Path) {
        let path = self.into_mut();
        *path = root.join(&path);
    }
}

impl<'lt> FsItem<'lt> {
    fn into_mut(self) -> &'lt mut PathBuf {
        match self {
            FsItem::Tree(path) | FsItem::File(path) => path,
        }
    }
}

#[cold]
fn inconclusive(err: &mut dyn std::fmt::Display) -> ! {
    eprintln!("xtest-data failed to setup.");
    eprintln!("Information: {}", err);
    panic!();
}
