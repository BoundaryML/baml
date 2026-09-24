//! Embed the stdlib slice this crate's transient runtime compiler splices in.
//!
//! Cargo keys `OUT_DIR` and reruns this script with the exact compiler
//! dependency graph, so the embedded bytes cannot outlive the build that
//! produced their `Program`/`PackageInterface` layouts.
//!
//! The interface derivation lives in `baml_db::stdlib_prefix`, shared with
//! the test-side artifact in `baml_tests`. This artifact deliberately
//! carries only [`precompiled_stdlib_config::OPT_LEVEL`]: it ships inside
//! production binaries, where each additional level is a few more megabytes for
//! no runtime benefit.
//!
//! The bytecode half is still the flat stdlib `Program` slice: the runtime
//! compiler emits its package against that image's index layout and the graft
//! binds the result by name. Both move onto per-package outputs with the
//! loader that consumes them, and this slice dies then.

use baml_db::{
    ProjectDatabase, baml_compiler2_emit::generate_stdlib_program, stdlib_prefix::stdlib_interfaces,
};

#[path = "src/precompiled_stdlib_config.rs"]
mod precompiled_stdlib_config;

fn main() {
    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let interfaces = stdlib_interfaces(&db);
    let program = generate_stdlib_program(&db, precompiled_stdlib_config::OPT_LEVEL)
        .expect("the embedded stdlib always compiles cleanly");
    let artifact = (
        precompiled_stdlib_config::artifact_key(),
        interfaces,
        program,
    );
    let bytes = borsh::to_vec(&artifact).expect("serialize compiler-built stdlib artifact");

    let out_dir = std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR for build scripts");
    std::fs::write(
        std::path::PathBuf::from(out_dir).join("stdlib_prefix.borsh"),
        bytes,
    )
    .expect("write compiler-built stdlib artifact");
}
