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

/// A git commit ID.
/// This is treated as opaque string data. Usually it's a Sha1 hash (20 byte, hex-encoded).
#[derive(Debug)]
pub(crate) struct CommitId(String);

pub(crate) enum PathSpec<'lt> {
    Path(&'lt Path),
}

impl Git {
    pub fn new() -> Result<Self, impl std::fmt::Display> {
        which::which("git").map(|bin| Git { bin })
    }

    pub fn consent_to_use(
        &self,
        gitpath: &Path,
        datapath: &Path,
        origin: &Origin,
        commit: &CommitId,
        resources: &mut dyn Iterator<Item=&Path>,
        pathspecs: &mut dyn Iterator<Item=PathSpec>,
    ) {
        let specs = resources.zip(pathspecs);

        let var = std::env::var("CARGO_XTEST_DATA_FETCH")
            .map_or_else(|err| {
                match err {
                    std::env::VarError::NotPresent => None,
                    std::env::VarError::NotUnicode(_) => Some("no".into()),
                }
            }, Some);

        match var.as_ref().map(String::as_str) {
            Some("yes") | Some("1") | Some("true") => {},
            _ => {
                eprintln!("These tests require additional data from a remote source.");
                eprintln!("Here is what we planned to do.");
                eprintln!("Set up bare Git dir in: {}", gitpath.display());
                eprintln!("Git Origin: {}", origin.url);
                eprintln!("Fetch Commit: {}", commit.0);
                eprintln!("Checkout files into: {}", datapath.display());
                for (resource, pathspec) in specs {
                    eprintln!("  into {}: {}", resource.display(), pathspec);
                }
                eprintln!("Explicit consent can be given by setting CARGO_XTEST_DATA_FETCH=1");
                inconclusive(&mut "refusing to continue without explicit agreement to network (see error log).")
            }
        }
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

impl From<&'_ str> for CommitId {
    fn from(st: &'_ str) -> CommitId {
        assert!(st.len() >= 40, "Unlikely to be a safe Git Object ID in vcs pin file: {}", st);
        CommitId(st.to_owned())
    }
}

impl CrateDir {
    pub fn new(path: &'static str, git: &Git) -> Self {
        let dir = CrateDir {
            path: Path::new(path).to_owned()
        };

        let mut cmd = dir.exec(git);
        cmd.args(["status", "--short"]);
        cmd.status().unwrap_or_else(|mut err| inconclusive(&mut err));
        dir
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
        cmd.stdout(Stdio::piped());
        cmd.args(["status", "--no-renames", "--ignored=matching", "--porcelain=v2", "--short", "-z"]);
        cmd.arg("--");
        let mut any = false;
        cmd.args(paths.map(|st| {
            any = true;
            st.to_string()
        }));

        if !any {
            return;
        }

        let output = cmd.output().unwrap_or_else(|mut err| inconclusive(&mut err));
        let items = String::from_utf8(output.stdout)
            .unwrap_or_else(|mut err| inconclusive(&mut err));
        for item in items.split('\0') {
            if item.starts_with('!') {
                eprintln!("{}", item);
                inconclusive(&mut "Your test depends on ignored file(s)");
            } else if item.starts_with('?') {
                eprintln!("{}", item);
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
        cmd.stderr(Stdio::piped());
        cmd
    }

    pub fn fetch(&self, git: &Git, head: &CommitId) {
        let mut cmd = self.exec(git);
        cmd.args(["fetch", "--filter=blob:none", "--depth=1"]);
        cmd.arg(self.origin.url.as_str());
        cmd.arg(&head.0);
        let exit = cmd.output()
            .unwrap_or_else(|mut err| inconclusive(&mut err));
        if !exit.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&exit.stderr));
            inconclusive(&mut "Git operation was not successful");
        }
    }

    pub fn checkout(
        &self,
        git: &Git,
        worktree: &Path,
        head: &CommitId,
        paths: &mut dyn Iterator<Item=PathSpec<'_>>,
    ) {
        let mut simple_filter = vec![];
        let paths: Vec<_> = paths
            .filter_map(|path| {
                if let Some(sparse_compatible) = path.as_encompassing_path() {
                    // Look, we don't have proper escaping for it yet and no NUL separator.
                    let format = sparse_compatible.display().to_string();
                    if format.contains('\n') || format.contains('\0') /* ?? */ {
                        return Some(path)
                    }

                    simple_filter.push(path);
                    None
                } else { 
                    Some(path)
                }
            })
            .collect();

        // First setup sparse-checkout
        // Note that this is in beta and not supported, so let's fallback if necessary.
        let try_sparse_checkout = || -> std::io::Result<()> {
            let mut cmd = self.exec(git);
            cmd.arg("--work-tree");
            cmd.arg(worktree);
            cmd.args(["sparse-checkout", "set", "--stdin"]);
            cmd.stdin(Stdio::piped());
            let mut running = cmd.spawn()?;
            let stdin = running.stdin.as_mut().expect("Spawned with stdio-piped");
            for path in &simple_filter {
                let simple = path.as_encompassing_path().unwrap().display().to_string();
                use std::io::Write;
                // > This includes interpreting pathnames that begin with a double quote (") as C-style quoted strings.
                // Since there is no NUL separation (yet?) we use this.
                eprintln!("{}", simple);
                writeln!(stdin, "{}", simple).unwrap_or_else(|mut err| inconclusive(&mut err));
            }
            running.stdin = None;
            let exit = running.wait_with_output()?;
            if !exit.status.success() {
                return Err(std::io::ErrorKind::Other.into())
            }
            Ok(())
        };

        if let Err(err) = try_sparse_checkout() {
            eprintln!("Version of Git appears to not support sparse-checkout: {}", err);
            let mut all_again = simple_filter.into_iter().chain(paths);
            return self.checkout_fallback_slow(git, worktree, head, &mut all_again);
        }

        let mut cmd = self.exec(git);
        cmd.arg("--work-tree");
        eprintln!("Checkout to worktree {}", worktree.display());
        cmd.arg(worktree);
        cmd.arg("checkout");
        cmd.arg("--force");
        cmd.arg(&head.0);
        let exit = cmd.output()
            .unwrap_or_else(|mut err| inconclusive(&mut err));
        if !exit.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&exit.stderr));
            inconclusive(&mut "Git operation was not successful");
        }

        self.checkout_fallback_slow(git, worktree, head, &mut paths.into_iter());
    }


    /// A version of `checkout` that uses checkout and a list pathspecs from stdin to determine the
    /// files in the worktree. However, it appears that this cases git to open a connection to the
    /// remote _for every single one_.
    pub fn checkout_fallback_slow(
        &self,
        git: &Git,
        worktree: &Path,
        head: &CommitId,
        paths: &mut dyn Iterator<Item=PathSpec<'_>>,
    ) {
        let mut cmd = self.exec(git);
        cmd.arg("--work-tree");
        cmd.arg(worktree);
        cmd.args(["checkout", "--no-guess", "--force"]);
        cmd.args(["--pathspec-from-file=-", "--pathspec-file-nul"]);
        cmd.arg(&head.0);
        cmd.stdin(Stdio::piped());
        let mut running = cmd.spawn().unwrap_or_else(|mut err| inconclusive(&mut err));
        let stdin = running.stdin.as_mut().expect("Spawned with stdio-piped");
        for path in paths {
            use std::io::Write;
            write!(stdin, "{}\0", path).unwrap_or_else(|mut err| inconclusive(&mut err));
        }
        running.stdin = None;
        let exit = running.wait_with_output()
            .unwrap_or_else(|mut err| inconclusive(&mut err));
        if !exit.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&exit.stderr));
            inconclusive(&mut "Git operation was not successful");
        }
    }
}

impl PathSpec<'_> {
    /// For git sparse checkout.
    pub fn as_encompassing_path(&self) -> Option<&Path> {
        match self {
            PathSpec::Path(path) => Some(path),
            // Should return None for a glob-filtered path since that is not supported.
        }
    }
}

impl core::fmt::Display for PathSpec<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            PathSpec::Path(path) => write!(f, ":(top,literal){}", path.display()),
        }
    }
}
