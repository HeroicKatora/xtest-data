use std::path::PathBuf;

use clap::Parser;

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
        /// If we should allow a dirty repository.
        ///
        /// This will fail to do the right thing if any test data is dirty. Unlike cargo, this
        /// _will_ write a custom `vcs_info` file to use. However, all test data must a reachable
        /// within the tree given by the current VCS (otherwise it wouldn't be part of the pack).
        #[arg(long, default_value = "false")]
        allow_dirty: bool,
    },
    /// Pack the source data, but do not run the full integration test.
    ///
    /// This will only create the pack archive according to the instructions but it will not re-run
    /// the full test suite on a cleanroom unpack of the archive.
    #[command(alias = "pack")]
    Package {
        /// The path to the source repository.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// If we should allow a dirty repository.
        ///
        /// This will fail to do the right thing if any test data is dirty. Unlike cargo, this
        /// _will_ write a custom `vcs_info` file to use. However, all test data must a reachable
        /// within the tree given by the current VCS (otherwise it wouldn't be part of the pack).
        #[arg(long, default_value = "false")]
        allow_dirty: bool,
    },
    /// _Only_ perform the download step.
    ///
    /// Prepare the artifacts into a directly by running the suitable steps. The output directory
    /// is suitable for use as a `CARGO_XTEST_DATA_PACK_OBJECTS` variable.
    FetchArtifacts {
        /// The path to the source crate archive.
        path: PathBuf,
        /// Provide a downloaded `pack-artifact`.
        #[arg(id = "pack-artifact")]
        pack_artifact: Option<PathBuf>,
        /// Provide an explicit write location. Otherwise, a default is chosen based on the crate
        /// name, version, and target directory.
        output: Option<PathBuf>,
    },
    /// Test a crate archive.
    ///
    /// This command may download the test archive data.
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
