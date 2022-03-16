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
//! use std::path::PathBuf;
//!
//! // or any other file you want to use.
//! let mut datazip = PathBuf::from("tests/data.zip");
//! xtest_data::setup!().rewrite([&mut datazip]).build();
//!
//! // … and the crate works its magic to make this succeed.
//! assert!(datazip.exists(), "{}", datazip.display());
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

use std::{borrow::Cow, env, ffi::OsString, fs, io, path::Path, path::PathBuf};
use tinyjson::JsonValue;

/// A file or tree that was registered from [`Setup`].
///
/// This is a key into [`FsData`]. You can retrieve the local path using [`FsData::path()`]. The
/// returned path is either the local path on disk, when you are currently developing under a local
/// checkout of the version control system, or the path into which the file has been checked out.
#[derive(Debug)]
pub struct Files {
    key: usize,
}

#[derive(Debug)]
enum Managed {
    // TODO: have a spec for the glob `<dir>/**.ext`?
    Files(PathBuf),
}

type FsItem<'lt> = &'lt mut PathBuf;

/// The product of `Setup`, ensuring local file system accessible test resources.
///
/// This object is used to retrieve the local paths of resources that have been registered with the
/// method [`Setup::add()`].
#[derive(Debug)]
pub struct FsData {
    /// Map all configured items to their paths.
    /// This map will essentially be constant and we do not care about the VCS interpretation.
    map: Vec<PathBuf>,
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
    /// All files and tree that are owned by the `Setup`.
    /// Note: we never intend to remove anything from here. If we did we would have to do some kind
    /// of remapping data structure to ensure that `Files` does not access the wrong item.
    relative_files: Vec<Managed>,
    /// Resources where we do 'simple' path replacement in a filter style.
    ///
    /// Note on ergonomics: We MAY take several different kinds of paths in the future to allow the
    /// glob-style usage (`tests/samples/*.png`) to be efficiently executed. However, we should NOT
    /// change the public API for this. We may well do some wrapping internally but the calls
    /// should map to exactly one variant of any such item; and the enum variant should not be
    /// directly exposed.
    ///
    /// This is based on the needs to perform more imports and additional calls to wrap locals in
    /// those items. Basically, adding the crate should not be much more complex than making all
    /// paths a variable and then throwing a `xtest_data::setup!()` on top.
    unmanaged: Vec<FsItem<'paths>>,
}

/// A builder to configure desired test data paths.
///
/// This is created through [`setup!`] instead of a usual method as it must gather some information
/// from the _callers_ environment first.
///
/// This is a builder and after configuration, its [`Setup::build()`] method should be called. Note
/// the lifetime on this struct. This is either the lifetime of paths borrowed from the caller,
/// which it will rewrite, or it can be `'static` when it owns all of the paths. The latter case
/// requires them to be registered with [`Setup::add()`].
///
/// On a VCS copy of the surrounding package this will simply collect and validate the information,
/// canonicalizing paths to be interpreted from the Manifest in the process.
///
/// However, when executed in the source tree from `.crate` then it will rewrite them all to refer
/// to a local copy of the data instead. That is, if it is allowed to, since by default we merely
/// provide a detailed report of data paths, repository location, and commit information that would
/// _need_ to be fetched before aborting. When the environment has opted into our access of network
/// (and might have overridden the repository path) then we will perform the actual access,
/// checkout, and rewrite.
#[must_use = "This is only a builder. Call `build` to perform validation/fetch/etc."]
#[derive(Debug)]
pub struct Setup<'paths> {
    repository: OsString,
    manifest: &'static str,
    /// Have we determined to be local or in a crate?.
    source: Source,
    /// The resources that we store.
    resources: Resources<'paths>,
    /// A git pack archive with files.
    pack_objects: Option<OsString>,
}

/// The options determined from the compile time environment of the crate that called us.
///
/// This is every environment data we are gather from the `setup` macro, which allows us to get the
/// environment flags passed to the _calling_ crate instead of our own. Please do not construct
/// this directly since doing so could affect the integrity of the information.
///
/// This is independent from the data gathered from the _runtime_ environment. It is combined with
/// that information in `Setup::build`.
#[doc(hidden)]
pub struct EnvOptions {
    pub pkg_repository: &'static str,
    pub manifest_dir: &'static str,
    pub target_tmpdir: Option<&'static str>,
}

/// Create a builder to configure local test data.
///
/// This evaluates to an instance of [`Setup`].
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
    };
}

#[doc(hidden)]
pub fn _setup(options: EnvOptions) -> Setup<'static> {
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

    let (source, pack_objects);
    if vcs_info_path.exists() {
        // Allow the override.
        trait GetKey {
            fn get_key(&self, key: &str) -> Option<&Self>;
        }
        impl GetKey for JsonValue {
            fn get_key(&self, key: &str) -> Option<&Self> {
                self.get::<std::collections::HashMap<_, _>>()?.get(key)
            }
        }

        let data =
            fs::read_to_string(vcs_info_path).unwrap_or_else(|mut err| inconclusive(&mut err));
        let vcs: JsonValue = data
            .parse()
            .unwrap_or_else(|mut err| inconclusive(&mut err));
        let commit_id = vcs
            .get_key("git")
            .unwrap_or_else(|| inconclusive(&mut "VCS does not contain a git section."))
            .get_key("sha1")
            .unwrap_or_else(|| inconclusive(&mut "VCS commit ID not recognized."))
            .get::<String>()
            .map(|id| git::CommitId::from(&**id))
            .unwrap_or_else(|| inconclusive(&mut "VCS commit ID is not a string"));

        // Okay, that makes sense. We know _what_ to access.
        // Now let's also try to find out how we will access it. Let's find `git`.
        // To shell out to because we are lazy.
        let git = git::Git::new().unwrap_or_else(|mut err| inconclusive(&mut err));

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

        pack_objects = std::env::var_os("CARGO_XTEST_DATA_PACK_OBJECTS");
        source = Source::VcsFromManifest {
            commit_id,
            git,
            datadir,
        };
    } else {
        // Check that we can recognize tracked files.
        let git = git::Git::new().unwrap_or_else(|mut err| inconclusive(&mut err));
        source = Source::Local(git);
        pack_objects = std::env::var_os("CARGO_XTEST_DATA_PACK_OBJECTS");
    };

    // And finally this must be valid.
    if repository.is_empty() {
        inconclusive(&mut "The repository must have a valid URL");
    }

    Setup {
        repository,
        manifest,
        source,
        resources: Resources::default(),
        pack_objects,
    }
}

impl<'lt> Setup<'lt> {
    /// Register some paths to rewrite their location.
    ///
    /// The paths should be relative to the crate's manifest. For example, to refer to data in your
    /// `tests` directory you would use `PathBuf::from("tests/data.zip")`.
    ///
    /// The paths will be registered internally. If the repository is local they will be rewritten
    /// to be relative to the manifest location. If the repository is a crate distribution then the
    /// paths will be sparsely checked out (meaning: only that path will be downloaded from the VCS
    /// working dir and you can't expect any other files to be present).
    ///
    /// Those actions will happen when you call [`Setup::build()`].
    ///
    /// # Example
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use xtest_data::setup;
    ///
    /// let mut path = PathBuf::from("tests/data.zip");
    /// setup!().rewrite([&mut path]).build();
    ///
    /// assert!(path.exists(), "{}", path.display());
    /// ```
    pub fn rewrite(mut self, iter: impl IntoIterator<Item = &'lt mut PathBuf>) -> Self {
        self.resources.unmanaged.extend(iter);
        self
    }

    /// Register the path of a file or a tree of files.
    ///
    /// The return value is a key that can later be used in [`FsData`]. All the files under this
    /// location will be checked out when `Setup::build()` is called in a crate-build.
    ///
    /// # Example
    ///
    /// ```
    /// let mut vcs = xtest_data::setup!();
    /// let datazip = vcs.add("tests/data.zip");
    /// let testdata = vcs.build();
    ///
    /// let path = testdata.path(&datazip);
    /// assert!(path.exists(), "{}", path.display());
    /// ```

    pub fn add(&mut self, path: impl AsRef<Path>) -> Files {
        fn path_impl(resources: &mut Resources, path: &Path) -> usize {
            let item = Managed::Files(path.to_owned());
            let key = resources.relative_files.len();
            resources.relative_files.push(item);
            key
        }

        let key = path_impl(&mut self.resources, path.as_ref());
        Files { key }
    }

    /// Run the final validation and perform rewrites.
    ///
    /// Returns the frozen dictionary of file mappings that had been registered with
    /// [`Setup::add()`]. This allows retrieving the final data paths for those items.
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

                if let Some(pack_objects) = self.pack_objects {
                    std::fs::create_dir_all(&pack_objects)
                        .unwrap_or_else(|mut err| inconclusive(&mut err));
                    dir.pack_objects(&git, &mut self.resources.path_specs(), pack_objects);
                }

                map = vec![];
                self.resources.relative_files.iter().for_each(|path| {
                    map.push(datapath.join(path.as_path()));
                });

                self.resources
                    .unmanaged
                    .into_iter()
                    .for_each(|item| set_root(datapath, item));
            }
            Source::VcsFromManifest {
                commit_id,
                datadir,
                git,
            } => {
                let origin = git::Origin {
                    url: self.repository,
                };

                let gitpath = datadir.join("xtest-data-git");
                let datapath = unique_dir(&datadir, "xtest-data-tree")
                    .unwrap_or_else(|mut err| inconclusive(&mut err));

                let shallow;
                if let Some(pack_objects) = self.pack_objects {
                    shallow = git.bare(gitpath, &commit_id);
                    shallow.unpack(&git, &pack_objects);
                } else {
                    let origin = git.consent_to_use(
                        &gitpath,
                        &datapath,
                        &origin,
                        &commit_id,
                        &mut self.resources.as_paths(),
                        &mut self.resources.path_specs(),
                    );

                    shallow = git.shallow_clone(gitpath, &origin);
                    shallow.fetch(&git, &commit_id, &origin);
                }

                shallow.checkout(
                    &git,
                    &datapath,
                    &commit_id,
                    &mut self.resources.path_specs(),
                );
                map = vec![];
                self.resources.relative_files.iter().for_each(|path| {
                    map.push(datapath.join(path.as_path()));
                });
                self.resources
                    .unmanaged
                    .into_iter()
                    .for_each(|item| set_root(&datapath, item));
            }
        }

        // In the end we just discard some information.
        // We don't really need it anymore after the checks.
        //
        // TODO: of course we could avoid actually checking files onto the disk if we had some kind
        // of `io::Read` abstraction that read them straight from `git cat` instead. But chances
        // are you'll like your files and directory structures.
        FsData { map }
    }
}

impl Resources<'_> {
    pub fn as_paths(&self) -> impl Iterator<Item = &'_ Path> {
        let values = self.relative_files.iter().map(Managed::as_path);
        let unmanaged = self.unmanaged.iter().map(|x| Path::new(x));
        values.chain(unmanaged)
    }

    pub fn path_specs(&self) -> impl Iterator<Item = git::PathSpec<'_>> {
        let values = self.relative_files.iter().map(Managed::as_path_spec);
        let unmanaged = self.unmanaged.iter().map(|x| git::PathSpec::Path(&**x));
        values.chain(unmanaged)
    }
}

impl FsData {
    /// Retrieve the rewritten path of a file or tree of files.
    pub fn path(&self, file: &Files) -> &Path {
        self.map.get(file.key).unwrap().as_path()
    }
}

impl Managed {
    pub fn as_path(&self) -> &Path {
        match self {
            Managed::Files(path) => path,
        }
    }

    fn as_path_spec(&self) -> git::PathSpec<'_> {
        match self {
            Managed::Files(path) => git::PathSpec::Path(path),
        }
    }
}

fn set_root(path: &Path, dir: &mut PathBuf) {
    *dir = path.join(&*dir)
}

// We do not use tempdir. This should already be done by our environment (e.g. cargo).
fn unique_dir(base: &Path, prefix: &str) -> Result<PathBuf, std::io::Error> {
    let mut rng = nanorand::tls::tls_rng();
    assert!(matches!(
        Path::new(prefix).components().next(),
        Some(std::path::Component::Normal(_))
    ));
    assert!(Path::new(prefix).components().nth(1).is_none());

    let mut buffer = prefix.to_string();
    let mut generate_name = move || -> PathBuf {
        use nanorand::Rng;
        const TABLE: &str = "0123456789abcdef";
        let num: [u8; 8] = rng.rand();

        buffer.clear();
        buffer.push_str(prefix);

        for byte in num {
            let (low, hi) = (usize::from(byte & 0xf), usize::from((byte >> 4) & 0xf));
            buffer.push_str(&TABLE[low..low + 1]);
            buffer.push_str(&TABLE[hi..hi + 1]);
        }

        base.join(&buffer)
    };

    loop {
        let path = generate_name();
        match fs::create_dir(&path) {
            Ok(_) => return Ok(path),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(other) => return Err(other),
        }
    }
}

#[cold]
#[track_caller]
fn inconclusive(err: &mut dyn std::fmt::Display) -> ! {
    eprintln!("xtest-data failed to setup.");
    eprintln!("Information: {}", err);
    panic!();
}
