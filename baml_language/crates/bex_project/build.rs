//! Embed the stdlib interface blobs this crate's transient runtime compiler
//! serves its stdlib roots from.
//!
//! Cargo keys `OUT_DIR` and reruns this script with the exact compiler
//! dependency graph, so the embedded bytes cannot outlive the build that
//! produced their `PackageInterface` layout.
//!
//! The interface derivation lives in `baml_db::stdlib_prefix`, shared with
//! the test-side artifact in `baml_tests`.

use baml_db::{ProjectDatabase, stdlib_prefix::stdlib_interfaces};

#[path = "src/precompiled_stdlib_config.rs"]
mod precompiled_stdlib_config;

fn main() {
    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let interfaces = stdlib_interfaces(&db);
    let artifact = (precompiled_stdlib_config::artifact_key(), interfaces);
    let bytes = borsh::to_vec(&artifact).expect("serialize compiler-built stdlib artifact");

    let out_dir = std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR for build scripts");
    std::fs::write(
        std::path::PathBuf::from(out_dir).join("stdlib_interfaces.borsh"),
        bytes,
    )
    .expect("write compiler-built stdlib artifact");
}
