//! `baml_db` — the concrete database crate of the BAML compiler.
//!
//! Provides [`ProjectDatabase`] (rust-analyzer's `RootDatabase` analog): the
//! Salsa database that owns the source-root table, the seed/mount inputs, and
//! implements every compiler `Db` trait — plus the project-level diagnostics
//! collectors ([`check`]), test helpers ([`testing`]), and the pure
//! project-discovery utilities ([`discovery`], [`project_resolution`]).
//!
//! It also re-exports the compiler crates so downstream code has a single
//! import point:
//!
//! ```ignore
//! use baml_db::{FileId, ProjectDatabase, SourceFile, baml_compiler2_hir, baml_compiler_parser};
//! ```

pub mod check;
pub mod db;
pub mod discovery;
pub mod project_resolution;
pub mod stdlib_prefix;
pub mod testing;

// Re-export all public APIs
pub use baml_base::*;
pub use baml_compiler_diagnostics;
pub use baml_compiler_lexer;
pub use baml_compiler_parser;
pub use baml_compiler_syntax;
pub use baml_compiler2_emit;
pub use baml_compiler2_hir;
pub use baml_compiler2_hir_ty;
pub use baml_compiler2_mir;
pub use baml_compiler2_ppir;
pub use check::{
    CheckResult, NarrowedDiagnostics, check_file, check_files_parallel,
    collect_compiler2_diagnostics, collect_compiler2_diagnostics_narrowed, collect_diagnostics,
    collect_package_level_diagnostics, prime_file_indexes_parallel,
};
pub use db::{EventCallback, ProjectDatabase, SourceRootError, SourceRootSpec, canonicalize_lossy};
pub use discovery::discover_baml_files;
pub use project_resolution::{
    BAML_SRC_DIR, BAML_TOML, find_baml_project_root, find_baml_project_root_from_ancestors,
    project_search_dir, project_source_root, resolve_project_search_start,
};
pub use salsa::Setter;
pub use testing::{
    OptLevel, assert_no_diagnostic_errors, compile_multi_file, compile_source,
    compile_source_with_opt, setup_test_db,
};
