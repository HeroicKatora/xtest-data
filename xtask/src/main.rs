use std::{env, fs, io};
use std::process::{Command, Stdio, Output};
use std::path::{Path, PathBuf};

use toml::Value;

// Use the same host-binary as is building us.
const CARGO: &'static str = env!("CARGO");

fn main() -> Result<(), io::Error> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Not built in the xtest-data repository");
    env::set_current_dir(repo)
        .expect("Not built in the xtest-data repository");

    let args  = Args::default();

    let target = Target::from_current_dir()?;
    let filename = target.expected_crate_name();

    let tmp = env::var_os("TMPDIR")
        .map_or_else(|| Path::new("/tmp").to_owned(), PathBuf::from);
    let extracted = tmp.join(target.expected_dir_name());

    Command::new(CARGO)
        .args(["package", "--no-verify"])
        .success()?;
    fs::remove_dir_all(&extracted)?;
    // gunzip -c target/package/xtest-data-0.0.2.crate
    let crate_tar = Command::new("gunzip")
        .arg("-c")
        .arg(Path::new("target/package").join(filename))
        .output()?
        .stdout;
    // tar -C /tmp --extract --file -
    Command::new("tar")
        .arg("-C")
        .arg(&tmp)
        .args(["--extract", "--file", "-"])
        .input_output(&crate_tar)?;

    if !args.test {
        return Ok(())
    }

    // TMPDIR=/tmp CARGO_XTEST_DATA_FETCH=1 cargo test  -- --nocapture
    Command::new(CARGO)
        .current_dir(&extracted)
        .args(["test", "--", "--nocapture"])
        .env("TMPDIR", &tmp)
        .env("CARGO_XTEST_DATA_FETCH", "yes")
        .env("CARGO_XTEST_DATA_REPOSITORY_ORIGIN", format!("file://{}", repo.display()))
        .success()?;

    Ok(())
}

struct Args {
    test: bool,
}

struct Target {
    name: String,
    version: String,
}

impl Default for Args {
    fn default() -> Self {
        let mut args = env::args().skip(1);
        match args.next().as_ref().map(String::as_str) {
            None => panic!("No command given"),
            Some("test") => Args { test: true },
            Some("prepare") => Args { test: false },
            _ => panic!("Invalid command given"),
        }
    }
}

impl Target {
    pub fn from_current_dir() -> Result<Self, io::Error> {
        let toml = fs::read("Cargo.toml")?;
        let toml: Value = toml::de::from_slice(&toml)
            .map_err(as_io_error)?;
        let package = toml.get("package")
            .ok_or_else(undiagnosed_io_error)?;
        let name = package.get("name")
            .ok_or_else(undiagnosed_io_error)?
            .as_str()
            .ok_or_else(undiagnosed_io_error)?
            .to_owned();
        let version = package.get("version")
            .ok_or_else(undiagnosed_io_error)?
            .as_str()
            .ok_or_else(undiagnosed_io_error)?
            .to_owned();
        Ok(Target { name, version })
    }

    pub fn expected_crate_name(&self) -> PathBuf {
        format!("{}-{}.crate", &self.name, &self.version).into()
    }

    pub fn expected_dir_name(&self) -> PathBuf {
        format!("{}-{}", &self.name, &self.version).into()
    }
}

trait GoodOutput {
    fn success(&mut self) -> Result<(), io::Error>;
    fn output(&mut self) -> Result<Output, io::Error>;
    fn input_output(&mut self, inp: &dyn AsRef<[u8]>)
        -> Result<Output, io::Error>;
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

fn undiagnosed_io_error() -> io::Error {
    io::ErrorKind::Other.into()
}

fn as_io_error<T>(err: T) -> io::Error
    where T: Into<Box<dyn std::error::Error + Send + Sync>>
{
    io::Error::new(io::ErrorKind::Other, err)
}
