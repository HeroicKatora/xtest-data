Implements fetching test data in distributed crates.

## How to use

Integrate this package as a dev-dependency into your tests. This allows
utilizing the library component to provide a compelling experience for testing
distributed packages without the need to distribute the test data itself.

It's expected that you also use this package to _register_ test data folder but
the also to _access_ the test data. The latter step isn't required but offers a
validation layer. In particular, the library will assert that the files are
accessible through the current VCS state.

```rust
let xdata = xtest_data::setup();
let xfile = xdata.file("tests/data.bin");
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
fetch—and then instantly hard abort. However, if the environment variable
`CARGO_XTASK_TEST_DATA_FETCH` is set to `yes`, `true` or `1` then we will try
to download and checkout requested files to the relative location.

## Ideas

As a [cargo xtask][cargo-xtask].

Add this as a git submodule (or subtree). This should allow you to configure a
dependency on data files in a separate repository and not tracked by `git`
itself. This package does not mind where you add it as long as you configure it
to be in _your_ workspace. Then setup a command alias to this
package.

[cargo-xtask]: https://github.com/matklad/cargo-xtask
