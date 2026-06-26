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
    use baml_db::{baml_compiler_parser, baml_compiler2_ast, baml_compiler2_hir};

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

    // --- HIR prefix: per-file pre-lowered AST for each builtin file.
    let mut hir: HashMap<String, baml_compiler2_hir::precompiled::PrecompiledFile> = HashMap::new();
    for file in baml_compiler2_hir::compiler2_all_files(&db) {
        let path = file.path(&db).to_string_lossy().to_string();
        let tree = baml_compiler_parser::syntax_tree(&db, file);
        let range = tree.text_range();
        let (items, diags, env_var_refs) =
            baml_compiler2_ast::lower_file_with_path(&tree, Some(std::path::Path::new(&path)));
        assert!(
            diags.is_empty(),
            "stdlib file {path} produced CST->AST lowering diagnostics; the HIR \
             prefix assumes the frozen stdlib lowers cleanly"
        );
        hir.insert(
            path,
            baml_compiler2_hir::precompiled::PrecompiledFile {
                items,
                env_var_refs,
                range_start: range.start().into(),
                range_end: range.end().into(),
            },
        );
    }
    let hir_bytes = borsh::to_vec(&hir).expect("serialize stdlib HIR prefix");
    std::fs::write(out_dir.join("stdlib_hir.bin"), &hir_bytes).expect("write HIR artifact");

    println!(
        "cargo:warning=baml_builtins2_prebuilt: EmitState prefix = {} bytes, HIR prefix = {} bytes ({} files)",
        prefix_bytes.len(),
        hir_bytes.len(),
        hir.len()
    );
}
