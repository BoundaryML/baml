//! The stdlib slice this crate's compile helpers splice in, compiled once at
//! build time by `build.rs`.
//!
//! # Why
//!
//! Every helper that builds a fresh `ProjectDatabase` re-derives the whole
//! stdlib, repeating much more work than a small test snippet needs.
//! `cargo nextest` runs each test in its own process, so a
//! `LazyLock` would be re-initialized per test and buy nothing — the slice has
//! to be embedded at build time.
//!
//! # What it does not change
//!
//! Output is **byte-identical** to `baml_db::testing`'s honest helpers, at
//! every optimization level. The stdlib sources stay in the database; only its
//! interface derivation and bytecode lowering are served from the prefix, and
//! both are pure functions of those same sources.
//! `baml_tests/tests/stdlib_prefix_equivalence.rs` compiles a corpus both ways and
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
//! The derivation itself lives in `baml_db::stdlib_prefix`. This crate builds
//! the test artifacts once for all consumers, without depending on the engine,
//! VM, or the full compiler-test harness. Runtime/host tests can use it as a
//! dev-dependency without cycles. Each optimization level is a separate
//! artifact so a nextest process only deserializes the level it uses.

use std::sync::LazyLock;

use baml_db::{ProjectDatabase, stdlib_prefix::decode_artifact, testing};
pub use baml_db::{
    stdlib_prefix::{OptLevel, StdlibPrefix},
    testing::{assert_no_diagnostic_errors, assert_no_user_diagnostic_errors, check_user_files},
};
use bex_vm_types::Program;

#[path = "../build_stdlib_prefix_config.rs"]
mod config;

// Nextest starts a fresh process per test. Keep each level independently lazy:
// most tests use One, so decoding Zero and Two would only allocate unused data.
static PREFIX_ZERO: LazyLock<StdlibPrefix> = LazyLock::new(|| {
    decode_prefix(
        OptLevel::Zero,
        0,
        include_bytes!(concat!(env!("OUT_DIR"), "/stdlib_prefix_0.borsh")),
    )
});
static PREFIX_ONE: LazyLock<StdlibPrefix> = LazyLock::new(|| {
    decode_prefix(
        OptLevel::One,
        1,
        include_bytes!(concat!(env!("OUT_DIR"), "/stdlib_prefix_1.borsh")),
    )
});
static PREFIX_TWO: LazyLock<StdlibPrefix> = LazyLock::new(|| {
    decode_prefix(
        OptLevel::Two,
        2,
        include_bytes!(concat!(env!("OUT_DIR"), "/stdlib_prefix_2.borsh")),
    )
});

fn decode_prefix(opt: OptLevel, raw: u8, artifact: &[u8]) -> StdlibPrefix {
    let mut prefixes = decode_artifact(&config::artifact_key(raw), artifact);
    assert_eq!(prefixes.len(), 1, "expected one stdlib prefix per artifact");
    prefixes
        .remove(&opt)
        .unwrap_or_else(|| panic!("the embedded stdlib prefix carries no slice for {opt:?}"))
}

/// The build-time stdlib slice for `opt`, decoded once per process on first use.
pub fn prefix(opt: OptLevel) -> &'static StdlibPrefix {
    match opt {
        OptLevel::Zero => &PREFIX_ZERO,
        OptLevel::One => &PREFIX_ONE,
        OptLevel::Two => &PREFIX_TWO,
    }
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
