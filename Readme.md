Fetch auxiliary test data when testing published crates.

# What this library is

This library addresses the problem that integration test suites and
documentation tests can not be ran from the published `.crate` archive alone,
if they depend on auxiliary data files that should not be shipped to downstream
packages and end users.

For this task it augments `Cargo.toml` with additional fields that describe how
an artifact archive composed from VCS files that are associated with the exact
version at which they were created. The packed data and exact version is then
referenced when executing test from the `.crate` archive. A small runtime
component unpacks the data and rewrites file paths to a substitute file tree.

## How to test crates

This repository contains a reference implementation for interpreting the
auxiliary metadata. It's simple to test crates depending on this library:

```bash
# test for developers
cargo run --bin xtask --features bin-xtask -- test <path-to-repo>
# test for packager
cargo run --bin xtask --features bin-xtask -- crate-test <crate>
# prepare a test but delay its execution
eval `cargo run --bin xtask --features bin-xtask -- fetch-artifacts <crate>`
```

For an offline use, where archives are handled by yourself:

```bash
# Prepare .crate and .xtest-data archives:
cargo run --bin xtask --features bin-xtask -- package
# on stdout, e.g.: ./target/xtest-data/xtest-data-1.0.0-beta.3.xtest-data

# < -- Any method to upload/download/exchange archives -- >

# After downloading both files again:
eval `cargo run --bin xtask --features bin-xtask -- \
  fetch-artifacts xtest-data-1.0.0-beta.3.crate \
  --pack-artifact xtest-data-1.0.0-beta.3.xtest-data`
# Now proceed with regular testing
```

## How to apply

Integrate this package as a dev-dependency into your tests.

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

## How to use offline

First, export the self-contained object-pack collection with your test runs.

```
CARGO_XTEST_DATA_PACK_OBJECTS="$(pwd)/target/xtest-data" cargo test
zip xtest-data.zip -r target/xtest-data
```

This allows utilizing the library component to provide a compelling experience
for testing distributed packages with the test data as a separate archive. You
can of course pack `target/xtest-data` in any other shape or form you prefer.
When testing a crate archive reverse these steps:

```
unzip xtest-data.zip
CARGO_XTEST_DATA_PACK_OBJECTS="$(pwd)/target/xtest-data" cargo test
```

# Details

## Usage for crate authors

For the basic usage, see the above section [How to apply](#How-to-apply). For
more advanced API usage consult [the documentation](https://docs.rs/xtest-data/).
The complete interface is not much more complex than the simple version above.

There is one additional detail if you want to check that your crate
successfully passes the tests on a crate distribution. For this you can
repurpose the `xtask` of this crate as a binary:

```bash
cd path/to/xtest-data
cargo run --bin xtask --features bin-xtask -- \
  --path to/your/crate test
```

Hint: if you add the source repository of `xtest-data` as a submodule and
modify your workspace to include the `xtask` folder then you can always execute
the `xtask` from your own crate.

The xtask will:
1. Run `cargo package` to create the `.crate` archive and accompanying pack
   directory. Note that this requires the sources selected for the crate to be
   unmodified.
2. Stop, if `test` is not selected. Otherwise, decompress and unpack this
   archive into a temporary directory.
3. Compile the package with `xtest-data` overrides for local development (see
   next section). In particular: `CARGO_XTEST_DATA_PACK_OBJECTS` will point to
   the pack output directory; `CARGO_XTEST_DATA_TMPDIR` will be set to a
   temporary directory create within the `target` directory; `CARGO_TARGET_DIR`
   will also point to the target directory.

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
  of the tests are _NOT_ integration tests. Simply put, the setup creates some
  auxiliary data files but it can not guarantee cleaning them up. This makes an
  explicit effort to communicate this to the environment. Feel free to contest
  this reasoning if you feel your use-case were better addressed with an
  implicit, leaking temporary directory.
* `CARGO_XTEST_DATA_PACK_OBJECTS`: A directory for git pack objects (see `man
  git pack-objects`). Pack files are written to this directory when running
  tests from source, and read from this directory when running tests from a
  `.crate` archive. These are the same objects that would be fetched when doing
  a shallow  and sparse clone from the source repository.
* `CARGO_XTEST_VCS_INFO`: Path to a file with version control information as
  json, equivalent in structure to cargo's generated VCS information. This will
  force xtest into VCS mode, where resources are replaced with data from the
  pack object(s). Can be used to either force crates to supply internal vcs
  information or to supplement such information. For example, packages
  generated with `cargo package --allow-dirty` will not include such a file,
  and this can be used to override with a forced selection.

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
* A full offline mode with minimal auxiliary source archives is provided.
  Building the crate without executing tests does not require any test data.
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
