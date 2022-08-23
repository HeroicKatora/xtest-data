use super::undiagnosed_io_error;

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
