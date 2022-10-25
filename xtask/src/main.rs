mod args;
mod pack;
mod target;
mod util;

use self::args::XtaskCommand;
use self::util::{anchor_error, as_io_error, undiagnosed_io_error, GoodOutput, LocatedError};

use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

use tempdir::TempDir;

// Use the same host-binary as is building us.
const CARGO: &'static str = env!("CARGO");

fn main() -> Result<(), LocatedError> {
    let args = args::Args::from_env().map_err(anchor_error())?;
    let repo = &args.repository;

    let mut private_tempdir = None;
    let tmp = env::var_os("TMPDIR").map_or_else(
        || {
            let temp =
                TempDir::new_in("target", "xtest-data-").expect("to create a temporary directory");
            fs::write(temp.path().join("Cargo.toml"), WORKSPACE_BOUNDARY)
                .expect("to create a workspace boundary if the package has non");
            let temp = private_tempdir.insert(temp);
            temp.path().to_owned()
        },
        PathBuf::from,
    );

    env::set_current_dir(repo).map_err(anchor_error())?;
    let target = target::Target::from_current_dir()?;
    let pack = pack::pack(&repo, &target, &tmp)?;

    let extracted = tmp.join(target.expected_dir_name());
    // Try to remove it but ignore failure.
    let _ = fs::remove_dir_all(&extracted).map_err(anchor_error());

    // gunzip -c target/package/xtest-data-0.0.2.crate
    let crate_tar = Command::new("gunzip")
        .arg("-c")
        .arg(pack.crate_path)
        .output()
        .map_err(anchor_error())?
        .stdout;

    // tar -C /tmp --extract --file -
    Command::new("tar")
        .arg("-C")
        .arg(&tmp)
        .args(["--extract", "--file", "-"])
        .input_output(&crate_tar)
        .map_err(anchor_error())?;

    if !args.test {
        return Ok(());
    }

    // TMPDIR=/tmp CARGO_XTEST_DATA_FETCH=1 cargo test  -- --nocapture
    Command::new(CARGO)
        .current_dir(&extracted)
        .args(["test", "--no-fail-fast", "--release", "--", "--nocapture"])
        // FIXME! Woah, we may actually have found a caching bug here! When compiling via this
        // source we got outdated binaries that did not reflect the *dirty* changes introduced in
        // the source archive?
        //
        // ]$ rustc --version --verbose
        // rustc 1.61.0 (fe5b13d68 2022-05-18)
        // binary: rustc
        // commit-hash: fe5b13d681f25ee6474be29d748c65adcd91f69e
        // commit-date: 2022-05-18
        // host: x86_64-unknown-linux-gnu
        // release: 1.61.0
        // LLVM version: 14.0.0
        //
        // Anyways we'd like to share the compilation cache.
        // .env("CARGO_TARGET_DIR", repo.join("target"))
        .env("CARGO_XTEST_DATA_TMPDIR", &tmp)
        .env("CARGO_XTEST_DATA_PACK_OBJECTS", &pack.pack_path)
        .env("CARGO_XTEST_VCS_INFO", &pack.vcs_info)
        .success()
        .map_err(anchor_error())?;

    Ok(())
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
