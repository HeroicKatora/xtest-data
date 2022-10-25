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

            let tmp = mk_tmpdir(&mut private_tempdir);
            let packed = task::pack::pack(&source, &target, &tmp)?;

            let test = task::test::test(&packed.crate_, &target, &packed, &tmp)?;
            eprintln!("{:?}", packed.pack_path);
            Ok(())
        }
        XtaskCommand::Prepare { path, allow_dirty } => {
            let source = target::LocalSource::with_simple_repository(&path).with_dirty(allow_dirty);
            let target = target::Target::from_dir(&source)?;

            let tmp = mk_tmpdir(&mut private_tempdir);
            let packed = task::pack::pack(&source, &target, &tmp)?;

            let archive = task::pack_archive::pack(&packed, &target, &tmp)?;
            // FIXME: print instructions
            eprintln!("{:?}", packed.pack_path);
            Ok(())
        }
        XtaskCommand::Test {
            path,
            pack_artifact,
        } => {
            let source = target::CrateSource {
                path: path.to_owned(),
            };

            let target = target::Target::from_crate(&source)?;
            let tmp = mk_tmpdir(&mut private_tempdir);

            let archive = match pack_artifact {
                None => {
                    todo!("Unimplemented function signature: {:x}", task::dl::download as usize);
                },
                // FIXME(clean code): we shouldn't build something from `task` but rather have the
                // task return an agreed-on interface data type.
                Some(artifact) => task::pack_archive::PackArchive {
                    path: artifact.to_owned(),
                },
            };

            todo!()
        }
    }
}

fn mk_tmpdir(private_tempdir: &mut Option<TempDir>) -> PathBuf {
    env::var_os("TMPDIR").map_or_else(
        || {
            let temp =
                TempDir::new_in("target", "xtest-data-").expect("to create a temporary directory");
            fs::write(temp.path().join("Cargo.toml"), WORKSPACE_BOUNDARY)
                .expect("to create a workspace boundary if the package has non");
            let temp = private_tempdir.insert(temp);
            temp.path().to_owned()
        },
        PathBuf::from,
    )
}

// A cargo.toml file that defines a workspace.
// Otherwise, if we extract some crate into `target/xtest-data-??/ but the current crate is in a
// workspace then we incorrectly detect the current directory as the crate's workspace—and fail
// because it surely does not include its target directory as members. This is because the
// _normalized_ Cargo.toml does not include workspace definitions.
const WORKSPACE_BOUNDARY: &'static str = r#"
[workspace]
members = ["*"]
"#;
