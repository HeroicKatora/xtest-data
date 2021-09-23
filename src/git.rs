use std::path::{PathBuf, Path};
use std::process::{Command, Stdio};
use url::Url;

use crate::inconclusive;

/// How we access `git` repositories.
#[derive(Debug)]
pub(crate) struct Git {
    bin: PathBuf,
}

/// A bare repository created by us.
pub(crate) struct ShallowBareRepository {
    origin: Origin,
    path: PathBuf,
}

/// The repository containing the manifest of the crate to integration test.
pub(crate) struct CrateDir {
    path: PathBuf,
}

pub(crate) struct Origin {
    pub url: Url,
}

pub(crate) enum PathSpec<'lt> {
    Path(&'lt Path),
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

impl CrateDir {
    pub fn new(path: &'static str) -> Self {
        CrateDir { path: Path::new(path).to_owned() }
    }

    pub fn exec(&self, git: &Git) -> Command {
        let mut cmd = Command::new(&git.bin);
        cmd.current_dir(&self.path);
        // Ensure we open _no_ handles.
        // Override this later if necessary.
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd
    }

    pub fn tracked(
        &self,
        git: &Git,
        paths: &mut dyn Iterator<Item=PathSpec<'_>>,
    ) {
        let mut cmd = self.exec(git);
        cmd.args(["status", "--no-renames", "--ignored=matching", "--porcelain=v2", "--short", "-z"]);
        cmd.arg("--");
        cmd.args(paths.map(|st| st.to_string()));
        cmd.stdout(Stdio::piped());
        let output = cmd.output().unwrap_or_else(|mut err| inconclusive(&mut err));
        let items = String::from_utf8(output.stdout)
            .unwrap_or_else(|mut err| inconclusive(&mut err));
        for item in items.split('\0') {
            if item.starts_with('!') {
                inconclusive(&mut "Your test depends on ignored file(s)");
            } else if item.starts_with('?') {
                inconclusive(&mut "Your test depends on untracked file(s)");
            }
        }
    }
}

impl ShallowBareRepository {
    pub fn exec(&self, git: &Git) -> Command {
        let mut cmd = Command::new(&git.bin);
        cmd.arg("--git-dir");
        cmd.arg(&self.path);
        // Ensure we open _no_ handles.
        // Override this later if necessary.
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd
    }

    pub fn fetch(&self, git: &Git, head: [u8; 20]) {
        let mut cmd = self.exec(git);
        cmd.args(["fetch", "--filter=blob:none", "--depth=1"]);
        cmd.arg(hex::encode(head));
        cmd.status().unwrap_or_else(|mut err| inconclusive(&mut err));
    }

    pub fn checkout(
        &self,
        git: &Git,
        worktree: &Path,
        head: [u8; 20],
        paths: &mut dyn Iterator<Item=PathSpec<'_>>,
    ) {
        let mut cmd = self.exec(git);
        cmd.arg("--work-tree");
        cmd.arg(worktree);
        cmd.args(["checkout", "--no-guess", "--force"]);
        cmd.args(["--pathspec-from-file=-", "--pathspec-file-nul"]);
        cmd.arg(hex::encode(head));
        cmd.stdin(Stdio::piped());
        let mut running = cmd.spawn().unwrap_or_else(|mut err| inconclusive(&mut err));
        let stdin = running.stdin.as_mut().expect("Spawned with stdio-piped");
        for path in paths {
            use std::io::Write;
            write!(stdin, "{}\0", path).unwrap_or_else(|mut err| inconclusive(&mut err));
        }
        running.stdin = None;
    }
}

impl core::fmt::Display for PathSpec<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            PathSpec::Path(path) => write!(f, ":(top,literal){}", path.display()),
        }
    }
}
