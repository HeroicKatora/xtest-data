mod args;
mod target;
mod task;
mod util;

use self::args::XtaskCommand;
use self::util::{anchor_error, as_io_error, undiagnosed_io_error, LocatedError};

use std::path::PathBuf;
use std::{env, fs};

use clap::Parser;
use tempdir::TempDir;

// Use the same host-binary as is building us.
const CARGO: &'static str = env!("CARGO");

fn main() -> Result<(), LocatedError> {
    let mut private_tempdir = None;
    match XtaskCommand::parse() {
        XtaskCommand::Ci { path, allow_dirty } => {
            let source = target::LocalSource::with_simple_repository(&path).with_dirty(allow_dirty);
            let target = target::Target::from_dir(&source)?;

            let tmp = mk_tmpdir(&mut private_tempdir, &target);
            let package = task::pack::pack(&source, &target, &tmp)?;

            let packed = task::artifacts::pack(&package.pack_path, &target, &tmp)?;
            let unpacked = task::artifacts::unpack(&packed, &target, &tmp)?;

            let test =
                task::test::test(&package.crate_, &target, &unpacked, &package.vcs_info, &tmp)?;

            let output = task::output::write_artifacts(&source, &target, &packed)?;
            eprintln!("Test success: {:?}", test);
            eprintln!("Package:\t{}", package.crate_.path.display());
            eprint!("Created:\t");
            println!("{}", output.display());
            Ok(())
        }
        XtaskCommand::Package { path, allow_dirty } => {
            let source = target::LocalSource::with_simple_repository(&path).with_dirty(allow_dirty);
            let target = target::Target::from_dir(&source)?;

            let tmp = mk_tmpdir(&mut private_tempdir, &target);
            let packed = task::pack::pack(&source, &target, &tmp)?;

            let archive = task::artifacts::pack(&packed.pack_path, &target, &tmp)?;
            let output = task::output::write_artifacts(&source, &target, &archive)?;

            // FIXME: print instructions
            eprintln!("Created:\t{}", output.display());
            Ok(())
        }
        XtaskCommand::FetchArtifacts {
            path,
            pack_artifact,
            output,
        } => {
            // Prepare the sources, crate etc.
            let source = target::CrateSource {
                path: path.to_owned(),
            };

            let target = target::Target::from_crate(&source)?;
            let tmp = mk_tmpdir(&mut private_tempdir, &target);

            let archive = match pack_artifact {
                None => {
                    let download = task::dl::download(&target, &tmp)?;
                    download.artifact
                }
                // FIXME(clean code): we shouldn't build something from `task` but rather have the
                // task return an agreed-on interface data type.
                Some(artifact) => task::artifacts::PackedArtifacts {
                    path: artifact.to_owned(),
                },
            };

            let location = match output {
                Some(location) => location,
                None => target.expected_crate_name().join("target/xtest-data"),
            };

            let unpack = task::artifacts::unpack(&archive, &target, &tmp)?;
            let _ = std::fs::remove_dir_all(&location);
            let _ = std::fs::create_dir_all(location.parent().unwrap());

            std::fs::rename(&unpack.path, &location).map_err(anchor_error())?;
            eprint!("Created:\t");
            println!("{}", location.display());

            Ok(())
        }
        XtaskCommand::Test {
            path,
            pack_artifact,
        } => {
            // Prepare the sources, crate etc.
            let source = target::CrateSource {
                path: path.to_owned(),
            };

            let target = target::Target::from_crate(&source)?;
            let tmp = mk_tmpdir(&mut private_tempdir, &target);

            let archive = match pack_artifact {
                None => {
                    let download = task::dl::download(&target, &tmp)?;
                    download.artifact
                }
                // FIXME(clean code): we shouldn't build something from `task` but rather have the
                // task return an agreed-on interface data type.
                Some(artifact) => task::artifacts::PackedArtifacts {
                    path: artifact.to_owned(),
                },
            };

            let unpack = task::artifacts::unpack(&archive, &target, &tmp)?;

            let test =
                task::test::test(&source, &target, &unpack, &target::VcsInfo::FromCrate, &tmp)?;

            eprintln!("Test successful: {:?}", test);
            Ok(())
        }
    }
}

fn mk_tmpdir(private_tempdir: &mut Option<TempDir>, target: &target::Target) -> PathBuf {
    env::var_os("TMPDIR").map_or_else(
        || {
            let temp =
                TempDir::new_in("target", "xtest-data-").expect("to create a temporary directory");
            // A cargo.toml file that defines a workspace.
            // Otherwise, if we extract some crate into `target/xtest-data-??/ but the current crate is in a
            // workspace then we incorrectly detect the current directory as the crate's workspace—and fail
            // because it surely does not include its target directory as members. This is because the
            // _normalized_ Cargo.toml does not include workspace definitions.
            let boundary = format!(
                r#"
[workspace]
members = ["{}"]
"#,
                target.expected_dir_name().display()
            );
            fs::write(temp.path().join("Cargo.toml"), boundary)
                .expect("to create a workspace boundary if the package has non");
            let temp = private_tempdir.insert(temp);
            temp.path().to_owned()
        },
        PathBuf::from,
    )
}
