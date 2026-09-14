//! Embed the stdlib slice this crate's transient runtime compiler splices in.
//!
//! Cargo keys `OUT_DIR` and reruns this script with the exact compiler
//! dependency graph, so the embedded bytes cannot outlive the build that
//! produced their `Program`/`PackageInterface` layouts.
//!
//! The derivation itself lives in `baml_db::stdlib_prefix`, shared with
//! the test-side artifact in `baml_tests`. This artifact deliberately
//! carries only [`precompiled_stdlib_config::OPT_LEVEL`]: it ships inside
//! production binaries, where each additional level is a few more megabytes for
//! no runtime benefit.

use baml_db::stdlib_prefix::build_stdlib_prefix;

#[path = "src/precompiled_stdlib_config.rs"]
mod precompiled_stdlib_config;

fn main() {
    let prefix = build_stdlib_prefix(precompiled_stdlib_config::OPT_LEVEL);
    let artifact = (
        precompiled_stdlib_config::artifact_key(),
        prefix.interfaces,
        prefix.program,
    );
    let bytes = borsh::to_vec(&artifact).expect("serialize compiler-built stdlib artifact");

    let out_dir = std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR for build scripts");
    std::fs::write(
        std::path::PathBuf::from(out_dir).join("stdlib_prefix.borsh"),
        bytes,
    )
    .expect("write compiler-built stdlib artifact");
}
