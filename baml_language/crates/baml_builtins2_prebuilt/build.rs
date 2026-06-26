//! Compiles the embedded BAML stdlib once, at build time, into two artifacts
//! that a normal `baml` invocation loads instead of recompiling the stdlib:
//!
//! - `stdlib_prefix.bin`: the emit `EmitState` prefix (globals/object pool/type
//!   tags) — lets `get_bytecode` skip re-lowering+re-emitting the stdlib.
//! - `stdlib_hir.bin`: per-file pre-lowered AST (`PrecompiledFile`) keyed by
//!   builtin virtual path — lets `file_semantic_index` skip lex/parse/lower for
//!   stdlib files and re-run only the semantic-index builder.
//!
//! The stdlib source is frozen per compiler version (embedded in `baml_builtins2`
//! via `include_str!`), so both artifacts are deterministic.

use std::{collections::HashMap, path::PathBuf};

fn main() {
    use baml_db::{baml_compiler2_hir, baml_compiler2_tir};

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    // Rebuild the artifacts whenever the stdlib sources change.
    println!("cargo:rerun-if-changed={manifest_dir}/../baml_builtins2/baml_std");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // No user files: `set_project_root` loads only the embedded stdlib builtins.
    let mut db = baml_project::ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("/__baml_stdlib_precompile__"));

    // --- Emit prefix: core emit passes (1-4) over the stdlib, at OptLevel::Two
    // (matching `ProjectDatabase::get_bytecode`'s default).
    let prefix = baml_db::baml_compiler2_emit::precompile_stdlib(
        &db,
        baml_db::baml_compiler2_emit::OptLevel::Two,
    );
    let prefix_bytes = borsh::to_vec(&prefix).expect("serialize stdlib EmitState");
    std::fs::write(out_dir.join("stdlib_prefix.bin"), &prefix_bytes)
        .expect("write prefix artifact");

    // --- HIR prefix: snapshot each builtin file's FileSemanticIndex. The cache
    // is not installed at build time, so file_semantic_index builds from source
    // here; PrecompiledFile::from_index captures it in borsh-friendly form.
    let mut hir: HashMap<String, baml_compiler2_hir::precompiled::PrecompiledFile> = HashMap::new();
    for file in baml_compiler2_hir::compiler2_all_files(&db) {
        let path = file.path(&db).to_string_lossy().to_string();
        let index = baml_compiler2_hir::file_semantic_index(&db, file);
        hir.insert(
            path,
            baml_compiler2_hir::precompiled::PrecompiledFile::from_index(&db, index),
        );
    }
    let hir_bytes = borsh::to_vec(&hir).expect("serialize stdlib HIR prefix");
    std::fs::write(out_dir.join("stdlib_hir.bin"), &hir_bytes).expect("write HIR artifact");

    // --- TIR prefix: each stdlib package's fully-resolved PackageInterface (the
    // signature graph: class fields, enum variants, type aliases, function
    // signatures, throw sets). Resolving these is the dominant cost of the first
    // user-file type-check; this does it once, here, so a normal compile loads it.
    let mut tir: HashMap<String, baml_compiler2_tir::package_interface::PackageInterface> =
        HashMap::new();
    let mut implements: HashMap<String, baml_compiler2_tir::interfaces::ImplementsRegistry> =
        HashMap::new();
    let mut seen = std::collections::HashSet::new();
    for file in baml_compiler2_hir::compiler2_all_files(&db) {
        let pkg_name = baml_compiler2_hir::file_package::file_package(&db, file).package;
        if !seen.insert(pkg_name.clone()) {
            continue;
        }
        let pkg_id = baml_compiler2_hir::package::PackageId::new(&db, pkg_name.clone());
        let pi = baml_compiler2_tir::package_interface::package_interface(&db, pkg_id);
        tir.insert(pkg_name.to_string(), pi.clone());
        let reg = baml_compiler2_tir::interfaces::package_implements_registry(&db, pkg_id);
        implements.insert(pkg_name.to_string(), reg.clone());
    }
    let tir_bytes = borsh::to_vec(&tir).expect("serialize stdlib TIR interfaces");
    std::fs::write(out_dir.join("stdlib_tir.bin"), &tir_bytes).expect("write TIR artifact");
    let impl_bytes = borsh::to_vec(&implements).expect("serialize stdlib implements registries");
    std::fs::write(out_dir.join("stdlib_implements.bin"), &impl_bytes)
        .expect("write implements artifact");

    println!(
        "cargo:warning=baml_builtins2_prebuilt: EmitState prefix = {} bytes, HIR prefix = {} bytes ({} files), TIR prefix = {} bytes ({} packages)",
        prefix_bytes.len(),
        hir_bytes.len(),
        hir.len(),
        tir_bytes.len(),
        tir.len()
    );
}
