//! Test helpers for compiling BAML source into bytecode.
//!
//! These utilities panic on compile errors, so they are appropriate for test
//! code only. Production callers should use [`crate::collect_diagnostics`] and
//! [`baml_compiler2_emit::generate_project_bytecode_with_opt`] directly.
//!
//! # These are for white-box tests, not behavior tests
//!
//! Reach for these only when the *subject* of the test is something Rust can
//! see but BAML cannot: emitted bytecode, VM/heap state, salsa invalidation,
//! diagnostics, or a host-boundary value.
//!
//! If the test just compiles some BAML, runs a function, and asserts the
//! returned value or a catchable throw, it does not belong in Rust at all —
//! write it as a `test` block in `crates/baml_tests/baml_src/`, where the whole
//! corpus compiles once instead of once per test. See
//! `baml_language/TEST_INSTRUCTIONS.md` ("Where does a new test go?").

use std::path::Path;

use baml_compiler_diagnostics::Severity;
pub use baml_compiler2_emit::OptLevel;
use baml_compiler2_emit::{CompileOptions, generate_project_bytecode_with_opt};
use bex_vm_types::Program;

use crate::{ProjectDatabase, collect_diagnostics};

/// Set up a test database from BAML source code.
pub fn setup_test_db(source: &str) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    db.add_file("test.baml", source);
    db
}

/// Assert that a `ProjectDatabase` has no diagnostic errors in user files.
///
/// Builtin stdlib files (paths starting with `<builtin>/`) may have known
/// pre-existing errors that don't affect user code correctness. Only errors
/// in user-provided source files are checked here.
#[track_caller]
pub fn assert_no_diagnostic_errors(db: &ProjectDatabase) {
    let all_files = db.get_source_files();
    let diagnostics = collect_diagnostics(db);

    let user_file_ids: std::collections::HashSet<_> =
        all_files.iter().map(|f| f.file_id(db)).collect();

    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .filter(|d| {
            d.primary_span()
                .map(|span| user_file_ids.contains(&span.file_id))
                .unwrap_or(false)
        })
        .collect();
    if !errors.is_empty() {
        use std::fmt::Write;
        let mut msg = String::from("Compilation produced diagnostic errors:\n");
        for (i, err) in errors.iter().enumerate() {
            let _ = writeln!(
                msg,
                "  {}. [{}] {}",
                i + 1,
                err.code(),
                err.message_with_primary_label()
            );
        }
        panic!("{msg}");
    }
}

/// Compile BAML source with default optimization (`OptLevel::One`).
pub fn compile_source(source: &str) -> Program {
    compile_source_with_opt(source, OptLevel::One)
}

/// Compile BAML source with a specific optimization level.
pub fn compile_source_with_opt(source: &str, opt: OptLevel) -> Program {
    let db = setup_test_db(source);
    assert_no_diagnostic_errors(&db);

    let opts = CompileOptions {
        emit_test_cases: false,
    };
    generate_project_bytecode_with_opt(&db, &opts, opt)
        .expect("generate_project_bytecode should succeed for valid test source")
}

/// Compile multiple BAML files at the given relative paths in one project.
/// Use when a test needs cross-file or namespaced (`ns_<name>/`) layout,
/// which `compile_source`'s single-file helper can't express.
pub fn compile_multi_file(files: &[(&str, &str)]) -> Program {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    for (path, content) in files {
        db.add_file(*path, content);
    }
    assert_no_diagnostic_errors(&db);

    let opts = CompileOptions {
        emit_test_cases: false,
    };
    generate_project_bytecode_with_opt(&db, &opts, OptLevel::One)
        .expect("generate_project_bytecode should succeed for valid test source")
}
