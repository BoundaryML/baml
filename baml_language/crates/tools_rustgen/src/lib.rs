//! Generates the builtin / IO source files that used to be produced by the
//! `build.rs` of `sys_types`, `sys_ops`, `bex_vm`, and `bex_vm_types`.
//!
//! Moving this codegen out of `build.rs` and checking the output into the tree
//! removes `baml_builtins2_codegen` (and the `baml_compiler2_ast` subtree) from
//! those crates' *build*-dependency graph — which is otherwise compiled a
//! second time for the host and re-run on every `ast` edit.
//!
//! Freshness is enforced by the `up_to_date` test (run under `cargo test`) plus
//! a CI `git diff --exit-code` gate. Regenerate with `cargo run -p tools_rustgen`.

use std::path::PathBuf;

use anyhow::{Context, Result};

/// One generated file: path relative to the `crates/` directory, plus contents.
pub struct GenFile {
    pub rel_path: &'static str,
    pub contents: String,
}

/// The `crates/` directory of the workspace (shared with `baml_rustgen_check`).
pub fn crates_dir() -> PathBuf {
    baml_rustgen_check::crates_dir()
}

fn file(rel_path: &'static str, mut body: String) -> GenFile {
    // Prepend the header in place so `body` is consumed (no needless clone).
    body.insert_str(0, &baml_rustgen_check::header(rel_path));
    GenFile {
        rel_path,
        contents: body,
    }
}

/// Render every generated file (in memory). Same calls the old `build.rs` made.
///
/// ⚠️ MAINTAINER NOTE — keeping the freshness guards in sync when you change this list:
///
/// * **Adding a generated file here** → also add a matching
///   `baml_rustgen_check::assert_generated_matches_baml_std("<crate>/src/<file>.rs")`
///   line to the owning crate's `build.rs` (and a `[build-dependencies]
///   baml_rustgen_check` if that crate doesn't have a build.rs yet). Otherwise the
///   file is still guaranteed fresh by the `up_to_date` test below, but loses
///   the fast per-build check.
/// * **A new crate that `include!`s an existing generated file** needs no new
///   build.rs — the file is already guarded by its owning crate + the test.
///   A new crate with its OWN generated file follows the "adding a file" rule.
/// * **Coverage caveat:** `baml_rustgen_check`'s build.rs hash only covers
///   `$rust_function` / `$rust_io_function` / `$compiler_intrinsic` *function
///   signatures*. Files driven by class/struct definitions (e.g. the
///   error/panic enums and IO structs from `class` decls) or by codegen-logic
///   changes are guarded ONLY by the `up_to_date` test, not by build.rs. The
///   test (real regen + diff) is the catch-all backstop — ensure CI runs it.
pub fn render_all() -> Result<Vec<GenFile>> {
    let (vm_builtins, io_builtins, class_defs) = baml_builtins2_codegen::extract_native_builtins()
        .map_err(|e| anyhow::anyhow!("failed to extract builtins from BAML stdlib: {e}"))?;

    Ok(vec![
        // sys_types/build.rs
        file(
            "sys_types/src/io_generated.rs",
            baml_builtins2_codegen::generate_io_structs(&io_builtins, &class_defs),
        ),
        file(
            "sys_types/src/runtime_io.rs",
            baml_builtins2_codegen::generate_runtime_io(
                &io_builtins,
                &class_defs,
                "super::generated",
            ),
        ),
        // sys_ops/build.rs
        file(
            "sys_ops/src/io_generated.rs",
            baml_builtins2_codegen::generate_io_traits(
                &io_builtins,
                &class_defs,
                "sys_types::generated",
            ),
        ),
        file(
            "sys_ops/src/io_adapter.rs",
            baml_builtins2_codegen::generate_io_adapter(
                &io_builtins,
                &class_defs,
                "sys_types::generated",
            ),
        ),
        // bex_vm/build.rs
        file(
            "bex_vm/src/package_baml/nativefunctions_generated.rs",
            baml_builtins2_codegen::generate_native_trait(&vm_builtins, &class_defs),
        ),
        // bex_vm_types/build.rs
        file(
            "bex_vm_types/src/sys_op_generated.rs",
            baml_builtins2_codegen::generate_sys_op_enum(&io_builtins),
        ),
        file(
            "bex_vm_types/src/errors_generated.rs",
            baml_builtins2_codegen::generate_error_enums(&class_defs),
        ),
        file(
            "bex_vm_types/src/panics_generated.rs",
            baml_builtins2_codegen::generate_panic_enums(&class_defs),
        ),
    ])
}

/// Write every generated file into the tree. Returns the absolute paths written.
pub fn write_all() -> Result<Vec<PathBuf>> {
    let base = crates_dir();
    let mut written = Vec::new();
    for gf in render_all()? {
        let path = base.join(gf.rel_path);
        std::fs::write(&path, &gf.contents)
            .with_context(|| format!("writing {}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}
