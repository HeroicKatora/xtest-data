/// Based on a done package task, produce the CI archive according to a target spec.
pub mod artifacts;
/// Based on a target spec, prepare the pack archive.
pub mod dl;
/// A `cargo package` that runs all relevant tests, and adds vcs_info_data when dirty.
pub mod pack;
/// Based on a crate archive and CI archive, unpack and retest.
pub mod test;
/// Create non-temporary files.
pub mod output;
