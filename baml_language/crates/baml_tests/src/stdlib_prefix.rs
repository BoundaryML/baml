//! The stdlib slice this crate's compile helpers splice in, compiled once at
//! build time by `build.rs`.
//!
//! # Why
//!
//! Every helper that builds a fresh `ProjectDatabase` re-derives the whole
//! stdlib: ~9 CPU-seconds, against a few milliseconds for the snippet actually
//! under test. `cargo nextest` runs each test in its own process, so a
//! `LazyLock` would be re-initialized per test and buy nothing — the slice has
//! to be embedded at build time.
//!
//! # What it does not change
//!
//! Output is **byte-identical** to `baml_project::testing`'s honest helpers, at
//! every optimization level. The stdlib sources stay in the database; only its
//! interface derivation and bytecode lowering are served from the prefix, and
//! both are pure functions of those same sources.
//! `tests/stdlib_prefix_equivalence.rs` compiles a corpus both ways and
//! compares the serialized programs, so a divergence fails CI instead of
//! quietly changing what the suite tests.
//!
//! This is why the helpers here do **not** mount the stdlib as a source-less
//! precompiled package the way `reflect.Package.compile` does. That is far
//! faster again, but without stdlib bodies a direct sysop call lowers to `call`
//! instead of `sys_op`, and body-walking checks go quiet. Speed is not worth
//! testing a different artifact than we ship.
//!
//! # Layering
//!
//! The derivation itself lives in `baml_project::stdlib_prefix`, shared with
//! `bex_project`'s build script. Each consumer embeds its own artifact because
//! their requirements differ: `bex_project` ships one optimization level inside
//! production binaries where size matters, this crate carries every level for
//! tests, where it does not.

use std::{collections::BTreeMap, sync::LazyLock};

use baml_project::{ProjectDatabase, stdlib_prefix::decode_artifact, testing};
pub use baml_project::{
    stdlib_prefix::{OptLevel, StdlibPrefix},
    testing::{assert_no_diagnostic_errors, assert_no_user_diagnostic_errors, check_user_files},
};
use bex_vm_types::Program;

#[path = "../build_stdlib_prefix_config.rs"]
mod config;

const ARTIFACT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stdlib_prefix.borsh"));

/// Decoded once per process, on first use. Under `cargo test` that is once for
/// a whole binary; under `cargo nextest` it is once per test, which is still
/// one borsh decode instead of one stdlib compile.
static PREFIXES: LazyLock<BTreeMap<OptLevel, StdlibPrefix>> =
    LazyLock::new(|| decode_artifact(&config::artifact_key(), ARTIFACT));

/// The build-time stdlib slice for `opt`.
///
/// # Panics
///
/// If the artifact carries no slice for `opt`. That is a build-configuration
/// bug (add the level to `config::OPT_LEVELS`), not a runtime condition, so it
/// fails loudly rather than silently falling back to an honest compile — which
/// would look like a mysterious slowdown instead of a fixable mistake.
pub fn prefix(opt: OptLevel) -> &'static StdlibPrefix {
    PREFIXES.get(&opt).unwrap_or_else(|| {
        panic!("the embedded stdlib prefix carries no slice for {opt:?}; add it to OPT_LEVELS")
    })
}

/// Set up a test database from BAML source code.
pub fn setup_test_db(source: &str) -> ProjectDatabase {
    testing::setup_test_db_with_prefix(prefix(OptLevel::One), source)
}

/// [`setup_test_db`] for a project of several files.
pub fn setup_multi_file_db(files: &[(&str, &str)]) -> ProjectDatabase {
    testing::setup_multi_file_db_with_prefix(prefix(OptLevel::One), files)
}

/// Compile BAML source with default optimization (`OptLevel::One`).
pub fn compile_source(source: &str) -> Program {
    compile_source_with_opt(source, OptLevel::One)
}

/// Compile BAML source with a specific optimization level.
pub fn compile_source_with_opt(source: &str, opt: OptLevel) -> Program {
    testing::compile_source_with_prefix(prefix(opt), source, opt)
}

/// Compile multiple BAML files at the given relative paths in one project.
pub fn compile_multi_file(files: &[(&str, &str)]) -> Program {
    testing::compile_multi_file_with_prefix(prefix(OptLevel::One), files, OptLevel::One)
}
