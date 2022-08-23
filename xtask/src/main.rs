mod pack;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::{env, fs, io};

use serde::Serialize;
use tempdir::TempDir;
use toml::Value;

// Use the same host-binary as is building us.
const CARGO: &'static str = env!("CARGO");

fn main() -> Result<(), LocatedError> {
    let args = Args::from_env().map_err(anchor_error())?;
    let repo = &args.repository;
    env::set_current_dir(repo).map_err(anchor_error())?;

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

    let target = Target::from_current_dir()?;
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

#[derive(Debug)]
#[allow(dead_code)]
struct LocatedError {
    location: &'static std::panic::Location<'static>,
    inner: io::Error,
}

struct Args {
    test: bool,
    repository: PathBuf,
}

struct Target {
    env: TargetStatic,
    cargo: Metadata,
}

/// The information available to templates.
#[derive(Serialize)]
struct TargetStatic {
    name: String,
    version: String,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Default, Debug)]
struct Metadata {
    /// Artifact archival method.
    pack_archive: Option<ArchiveMethod>,
    /// Artifact URL template.
    pack_artifact: Option<String>,
    /// Relative path of location for pack objects.
    /// Suggested: `target/xtest-data` or `target/xtest-data-pack`.
    pack_objects: Option<String>,
}

/// Determine how the pack objects are archived.
#[derive(Debug)]
enum ArchiveMethod {
    TarGz,
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

impl Args {
    fn from_env() -> Result<Self, io::Error> {
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

impl Target {
    pub fn from_current_dir() -> Result<Self, LocatedError> {
        let toml = fs::read("Cargo.toml").map_err(anchor_error())?;
        let toml: Value = toml::de::from_slice(&toml)
            .map_err(as_io_error)
            .map_err(anchor_error())?;

        let package = toml
            .get("package")
            .ok_or_else(undiagnosed_io_error())
            .map_err(anchor_error())?;
        let name = package
            .get("name")
            .ok_or_else(undiagnosed_io_error())
            .map_err(anchor_error())?
            .as_str()
            .ok_or_else(undiagnosed_io_error())
            .map_err(anchor_error())?
            .to_owned();
        let version = package
            .get("version")
            .ok_or_else(undiagnosed_io_error())
            .map_err(anchor_error())?
            .as_str()
            .ok_or_else(undiagnosed_io_error())
            .map_err(anchor_error())?
            .to_owned();

        let mut target = Target {
            env: TargetStatic {
                name,
                version,
                extra: {
                    let map = package
                        .as_table()
                        .ok_or_else(undiagnosed_io_error())
                        .map_err(anchor_error())?
                        .clone();
                    map.into_iter().collect()
                },
            },
            cargo: Metadata::default(),
        };

        if let Some(meta) = package.get("metadata").and_then(|v| v.get("xtest-data")) {
            target.cargo = Metadata::from_value(meta, &target)?;
        };

        Ok(target)
    }

    pub fn expected_crate_name(&self) -> PathBuf {
        format!("{}-{}.crate", &self.env.name, &self.env.version).into()
    }

    pub fn expected_dir_name(&self) -> PathBuf {
        format!("{}-{}", &self.env.name, &self.env.version).into()
    }
}

impl Metadata {
    pub fn from_value(val: &Value, target: &Target) -> Result<Self, LocatedError> {
        let mut table = val
            .as_table()
            .ok_or_else(|| {
                let err =
                    io::Error::new(io::ErrorKind::Other, "Expected metadata.xtest-data table");
                anchor_error()(err)
            })?
            .clone();

        let mut meta = Metadata::default();
        let mut template = tinytemplate::TinyTemplate::new();
        let (artifact_src, object_src);

        if let Some(archive) = table.remove("pack-archive") {
            match archive.as_str() {
                Some("tar:gz") => {
                    meta.pack_archive = Some(ArchiveMethod::TarGz);
                }
                _ => {
                    let err = io::Error::new(io::ErrorKind::Other, "Unknown archive method");
                    return Err(anchor_error()(err));
                }
            }
        }

        if let Some(artifact) = table.remove("pack-artifact") {
            if let Some(artifact) = artifact.as_str() {
                artifact_src = artifact.to_string();
                let _ = template.add_template("__main__", &artifact_src);
                let artifact = template
                    .render("__main__", &target.env)
                    .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
                    .map_err(anchor_error())?;
                meta.pack_artifact = Some(artifact);
            } else {
                let err = io::Error::new(
                    io::ErrorKind::Other,
                    "Bad value for `pack-artifact`, expected string",
                );
                return Err(anchor_error()(err));
            }
        }

        if let Some(objects) = table.remove("pack-objects") {
            if let Some(objects) = objects.as_str() {
                object_src = objects.to_string();
                let _ = template.add_template("__main__", &object_src);
                let objects = template
                    .render("__main__", &target.env)
                    .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
                    .map_err(anchor_error())?;
                meta.pack_objects = Some(objects);
            } else {
                let err = io::Error::new(
                    io::ErrorKind::Other,
                    "Bad value for `pack-objects`, expected string",
                );
                return Err(anchor_error()(err));
            }
        }

        Ok(meta)
    }
}

trait GoodOutput {
    fn success(&mut self) -> Result<(), io::Error>;
    fn output(&mut self) -> Result<Output, io::Error>;
    fn input_output(&mut self, inp: &dyn AsRef<[u8]>) -> Result<Output, io::Error>;
}

trait ParseOutput {
    fn into_string(self) -> Result<String, io::Error>;
}

impl GoodOutput for Command {
    fn success(&mut self) -> Result<(), io::Error> {
        let status = self.status()?;
        if !status.success() {
            return Err(io::ErrorKind::Other.into());
        }
        Ok(())
    }

    fn output(&mut self) -> Result<Output, io::Error> {
        self.stdout(Stdio::piped());
        let output = self.output()?;
        if !output.status.success() {
            return Err(io::ErrorKind::Other.into());
        }
        Ok(output)
    }

    fn input_output(&mut self, inp: &dyn AsRef<[u8]>) -> Result<Output, io::Error> {
        self.stdin(Stdio::piped());
        self.stdout(Stdio::piped());
        let mut child = self.spawn()?;
        let output = {
            let mut stdin = child.stdin.take().unwrap();
            std::io::Write::write(&mut stdin, inp.as_ref())?;
            // Terminate the input here.
            drop(stdin);
            child.wait_with_output()?
        };
        if !output.status.success() {
            return Err(io::ErrorKind::Other.into());
        }
        Ok(output)
    }
}

impl ParseOutput for Output {
    fn into_string(self) -> Result<String, io::Error> {
        String::from_utf8(self.stdout).map_err(as_io_error)
    }
}

/// Create an IO error, with its message just point to the source.
#[track_caller]
fn undiagnosed_io_error() -> impl FnMut() -> io::Error {
    let location = std::panic::Location::caller();
    move || io::Error::new(io::ErrorKind::Other, location.to_string())
}

/// Rewrap an error as IO error because we're lazy and this is a decent enough error type.
fn as_io_error<T>(err: T) -> io::Error
where
    T: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    io::Error::new(io::ErrorKind::Other, err)
}

/// Wrap the errors in such a way that we can figure out where they came from.
/// It's kind of amazing that this is stable o_o
#[track_caller]
fn anchor_error() -> impl FnMut(io::Error) -> LocatedError {
    let location = std::panic::Location::caller();
    move |inner| LocatedError { location, inner }
}
