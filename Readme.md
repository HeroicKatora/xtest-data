Fetch auxiliary test data when testing published crates.

# What this library is

This library addresses the problem that integration test suites and
documentation tests can not be ran from the published `.crate` archive alone,
if they depend on auxiliary data files that should not be shipped to downstream
packages and end users.

## Motivation

As a developer of a library, you will write some integration with the goal of
ensuring correct functionality of your code. Typically, these will be executed
in a CI pipeline before release. However, what if someone else—e.g. an Open
Source OS distribution—wants to repackage your code? In some cases they might
need to perform simple, small modifications: rewrite dependencies, apply
compilation options like hardening flags, etc. After those modifications it's
unclear if the end product still conforms to its own expectations. Thus will
want to run the integration test suite again. That's where the library comes in.
It should ensure that:

* It is unobtrusive in that it does not require modification to the code that
  is used when included as a dependency.
* Tests should be reproducible from the packaged `.crate`, and an author can
  check this property locally and during pre-release checks.
* Auxiliary data files required for tests are referenced unambiguously.
* It does not make unmodifiable assumptions about the source of test data.

## How to apply

Integrate this package as a dev-dependency into your tests. This allows
utilizing the library component to provide a compelling experience for testing
distributed packages without the need to distribute the test data itself. The
main goal of this package is: if the tests run in your CI pipeline where your
complete repository is available, then they should also work with the package
distribution.

```rust
let mut path = PathBuf::from("tests/data.zip");
xtest_data::setup!()
    .rewrite([&mut path])
    .build();
// 'Magically' changed.
assert!(path.exists(), "{}", path.display());
```

Note the calls above are typed as infallible but they are not total—they will
panic when something is missing since this indicates absent data. The reasoning
is that this indicates a faulty setup, not something the test should handle.
The expectation of the library is that you access all data through this library
instead of as a direct path.

# Details

## Usage for crate authors

For the basic usage, see the above section [How to apply](#How-to-apply). For
more advanced API usage consult [the documentation](https://docs.rs/xtest-data/).
The complete interface is not much more complex than the simple version above.

There is one additional detail if you want to check that your crate
successfully passes the tests on a crate distribution. For this you can
repurpose the `xtask` of this crate:

```bash
cd path/to/xtest-data
cargo run -p xtask -- --path to/your/crate test
```

Hint: if you add the source repository of `xtest-data` as a submodule and
modify your workspace to include the `xtask` folder then you can always execute
the `xtask` from your own crate.

The xtask will:
1. Run `cargo package` to create the `.crate` archive. Note that this requires
   the sources selected for the crate to be unmodified.
2. Decompress and unpack this archive into a temporary directory.
3. Compile the package with `xtest-data` overrides for local development (see
   next section). In particular: `CARGO_XTEST_DATA_REPOSITORY_ORIGIN` will
   point to the selected path as a `file://` url; `CARGO_XTEST_DATA_TMPDIR`
   will be set to a temporary directory create within the `target` directory;
   `CARGO_TARGET_DIR` will also point to the target directory.

This keeps the `rustc` cached data around while otherwise simulating a fresh
distribution compilation.


## Customization points for packagers

In all settings, the `xtest_data` will inspect the following:
* The `Cargo.toml` file located in the `CARGO_MANIFEST_DIR` will be read,
  decoded and must at least contain the keys `package.name`, `package.version`,
  `package.repository`.

In a non-source setting (i.e. when running from a downloaded crate) the
`xtest_data` package will read the following environment variables:

* `CARGO_XTEST_DATA_TMPDIR` (fallback: `TMPDIR`) is required to be set when any
  of the tests are _NOT_ integration tests.
* `CARGO_XTEST_DATA_REPOSITORY_ORIGIN`: Can be set to override the Git url that
  is used as the source repository. Otherwise the repository from the package
  data is used.
* `CARGO_XTEST_DATA_FETCH`: If set to `1`, `yes`, `true` then it will try to
  make a network connection, fetch data from the source repository. If _not_
  set then it will print a plan of what it intended to do and which files it
  would request (as git pathspecs) from which commit, and then panic.

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

## Fulfillment of goals

* The package is a pure dev-dependency and there is focus on introducing a
  small amount of dependencies. (Any patches to minimize this further are
  welcome. We might add a toggle to disable locks and its dependencies if
  non-parallel test execution is good enough?)
* The `xtask` tool can be used for local development and CI (we use it in our
  own pipeline for example). It's not strongly linked to the implementation,
  just the public interface, so it is possible to replace it with your own
  logic.
* Auxiliary files are referenced by the commit object ID of the distributed
  crate, which implies a particular tree-ish from which they are retrieved.
  This is equivalent to descending a Merkle tree which lends itself to
  efficient signatures etc.
* It is possible to overwrite the source repository as long as it provides a
  git compatible server. For example, you might pre-clone the source commit and
  provide the data via a local `file://` repository.

## Known problems

When fetching data, git may repeatedly ask for credentials and is pretty slow.
This issue should not occur when `git` supports `sparse-checkout`. This is
because we are shelling out to Git and `git checkout`, which we utilize to very
selectively unshallow the commit at the exact path specs which we require, does
not keep the connection alive—even when you give it multiple pathspecs at the
same time through `--pathspecs-from-file=-`. With `sparse-checkout`, however,
we only call this once which lowers the number of connection attempts. A
workaround is to setup a local agent and purge that afterwards or to create a
short-lived token instead.

## Ideas for future work

As a [cargo xtask][cargo-xtask]. However, the idea of an xtask is that the
exact setup is not uploaded with the main package and just a local dev-tool.
Still, we could help with the test setup.

Add this as a git submodule (or subtree). This should allow you to configure a
dependency on data files in a separate repository and not tracked by `git`
itself. This package does not mind where you add it as long as you configure it
to be in _your_ workspace. Then setup a command alias to this package.

[cargo-xtask]: https://github.com/matklad/cargo-xtask
