Implements fetching test data in distributed crates.

## How to use

Integrate this package as a dev-dependency into your tests. This allows
utilizing the library component to provide a compelling experience for testing
distributed packages without the need to distribute the test data itself. The
main goal of this package is: if the tests run in your CI pipeline where your
complete repository is available, then they should also work with the package
distribution.

It's expected that you also use this package to _register_ test data folder and
then also to _access_ the test data. The latter step isn't required but offers a
validation layer. In particular, the library will assert that the files are
accessible through the current VCS state.

```rust
let setup = xtest_data::setup!();
let xfile = setup.file("tests/data.bin");
let xdata = setup.build();

let path = xdata.file(&xfile);
```

Note the calls above are infallible—they will panic when something is missing
since this indicates absent data.

## How it works

When `cargo` packages a `.crate`, it will include a file called
`.cargo_vcs_info.json` which contains basic version information, i.e. the
commit ID that was used as the basis of creation of the archive. When the
methods of this crate run, they detect the presence or absence of this file to
determine if data can be fetched (we also detect the repository information
from `Cargo.toml`).

If we seem to be running outside the development repository, then by default we
won't do anything but validate the information, debug print what we _plan_ to
fetch—and then instantly panic. However, if the environment variable
`CARGO_XTEST_DATA_FETCH` is set to `yes`, `true` or `1` then we will try
to download and checkout requested files to the relative location.

## Known problems

When fetching data, git keeps asking for credentials. This is because we are
shelling out to Git and `git checkout`, which we utilize to very selectively
unshallow the commit at the exact path specs which we require, does not keep
the connection alive—even when you give it multiple pathspecs at the same time
through `--pathspecs-from-file=-`. A workaround is to setup a local agent and
purge that afterwards or to create a short-lived token instead.

## Ideas for future work

As a [cargo xtask][cargo-xtask]. However, the idea of an xtask is that the
exact setup is not uploaded with the main package and just a local dev-tool.
Still, we could help with the test setup.

Add this as a git submodule (or subtree). This should allow you to configure a
dependency on data files in a separate repository and not tracked by `git`
itself. This package does not mind where you add it as long as you configure it
to be in _your_ workspace. Then setup a command alias to this package.

[cargo-xtask]: https://github.com/matklad/cargo-xtask
