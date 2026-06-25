//! Compiles the embedded BAML stdlib into a `Program` once, at build time, and
//! serializes it to `$OUT_DIR/stdlib_program.bin` for `lib.rs` to `include_bytes!`.
//!
//! The stdlib source is frozen per compiler version (embedded in `baml_builtins2`
//! via `include_str!`), so its compiled bytecode is identical on every run. This
//! moves that work out of every `baml` invocation.

use std::path::PathBuf;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    // Rebuild the artifact whenever the stdlib sources change.
    println!("cargo:rerun-if-changed={manifest_dir}/../baml_builtins2/baml_std");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // No user files: `set_project_root` loads only the embedded stdlib builtins,
    // so `precompile_stdlib` runs the core emit passes (1-4) over the stdlib only
    // and returns the reusable `EmitState` prefix (stdlib globals, object pool,
    // and type-tag cursors). A normal compile resumes from this instead of
    // recompiling the stdlib. Built at `OptLevel::Two` to match `get_bytecode`.
    let mut db = baml_project::ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("/__baml_stdlib_precompile__"));

    let prefix = baml_db::baml_compiler2_emit::precompile_stdlib(
        &db,
        baml_db::baml_compiler2_emit::OptLevel::Two,
    );

    let bytes = borsh::to_vec(&prefix).expect("serialize stdlib EmitState");
    std::fs::write(out_dir.join("stdlib_prefix.bin"), &bytes).expect("write artifact");
    println!(
        "cargo:warning=baml_builtins2_prebuilt: stdlib EmitState prefix = {} bytes",
        bytes.len()
    );
}
