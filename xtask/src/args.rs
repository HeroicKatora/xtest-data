use super::undiagnosed_io_error;
use clap::Parser;

use std::path::{Path, PathBuf};
use std::{env, io};

pub struct Args {
    pub test: bool,
    pub repository: PathBuf,
}

impl Args {
    pub(crate) fn from_env() -> Result<Self, io::Error> {
        let mut args = env::args().skip(1);
        let test;
        let mut repository = None;

        loop {
            match args.next().as_ref().map(String::as_str) {
                None => panic!("No command given"),
                Some("--path") => {
                    let argument = args.next().expect("Missing argument to `--path`");
                    let canonical = Path::new(&argument).canonicalize()?;
                    repository = Some(canonical);
                }
                Some("test") => {
                    test = true;
                    break;
                }
                Some("prepare") => {
                    test = false;
                    break;
                }
                _ => panic!("Invalid command given"),
            }
        }

        let default_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(undiagnosed_io_error())?;

        let repository = repository.map_or_else(|| default_path.to_owned(), PathBuf::from);

        Ok(Args { test, repository })
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(rename_all = "kebab-case")]
pub enum XtaskCommand {
    /// Run an integration test for a repository.
    ///
    /// This will:
    /// - Package the repository
    /// - Create the archive of test data
    /// - Unpack the crate into a temporary location
    /// - Prepare the test data from a file
    /// - Run tests with the test data
    Ci {
        /// The path to the source repository.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Pack the source data, but do not run the full integration test.
    ///
    /// This will only create the pack archive according to the instructions but it will not re-run
    /// the full test suite on a cleanroom unpack of the archive.
    #[command(alias = "pack")]
    Prepare {
        /// The path to the source repository.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// If we should allow a dirty repository.
        ///
        /// This will fail to do the right thing if any test data is dirty. Unlike cargo, this
        /// _will_ write a custom `vcs_info` file to use. However, all test data must a reachable
        /// within the tree given by the current VCS (otherwise it wouldn't be part of the pack).
        allow_dirty: bool,
    },
    /// Test a crate archive.
    ///
    /// This command may download the test archive data.
    #[command(id = "test")]
    Test {
        /// A path to a `.crate` archive, or an unpacked version.
        ///
        /// The relevant difference to a source repository is the presence of a `.cargo_vcs_info`
        /// file that provides the stable reference to the exact VCS state.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Overwrite path to the downloaded `pack-artifact`.
        #[arg(id = "pack-artifact")]
        pack_artifact: Option<PathBuf>,
    },
}
