use std::path::PathBuf;
use std::process::Command;
use url::Url;

use crate::inconclusive;

/// How we access `git` repositories.
#[derive(Debug)]
pub(crate) struct Git {
    bin: PathBuf,
}

pub(crate) struct ShallowBareRepository {
    origin: Origin,
    path: PathBuf,
}

pub(crate) struct Origin {
    pub url: Url,
}

impl Git {
    pub fn new() -> Result<Self, impl std::fmt::Display> {
        which::which("git").map(|bin| Git { bin })
    }

    /// Prepare `path` as a shallow clone of `origin`.
    /// Aborts if this isn't possible (see error handling policy).
    pub fn shallow_clone(&self, path: PathBuf, origin: Origin)
        -> ShallowBareRepository
    {
        let repo = ShallowBareRepository {
            origin,
            path,
        };

        if !repo.path.exists() {
            let mut cmd = repo.exec(self);
            // clone [options…]
            cmd.args(["clone", "--bare", "--no-checkout", "--filter=blob:none", "--depth=1", "--"]);
            // <repository>
            cmd.arg(repo.origin.url.as_str());
            // [<target>]
            cmd.arg(&repo.path);
            cmd.status().unwrap_or_else(|mut err| inconclusive(&mut err));
        } else {
            // Test that the repo in fact exists and is recognized by git.
            let mut cmd = repo.exec(self);
            cmd.args(["symbolic-ref", "HEAD"]);
            cmd.status().unwrap_or_else(|mut err| inconclusive(&mut err));
        }

        repo
    }
}

impl ShallowBareRepository {
    pub fn exec(&self, git: &Git) -> Command {
        let mut cmd = Command::new(&git.bin);
        cmd.arg("--git-dir");
        cmd.arg(&self.path);
        cmd
    }
}
