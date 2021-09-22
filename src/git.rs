use std::path::PathBuf;

/// How we access `git` repositories.
#[derive(Debug)]
pub(crate) struct Git {
    bin: PathBuf,
}

impl Git {
    pub fn new() -> Result<Self, impl std::fmt::Display> {
        which::which("git").map(|bin| Git { bin })
    }
}
