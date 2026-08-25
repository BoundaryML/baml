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
//!
//! # Two flavors: honest and prefix-accelerated
//!
//! The stdlib is the same ~50 files on every compile, so re-deriving it per
//! test is pure waste — and under `cargo nextest`, which runs each test in its
//! own process, no in-process cache can amortize it. The fix is a
//! [`StdlibPrefix`]: the stdlib's typed interfaces plus its bytecode slice,
//! built once per toolchain by a build script (see `baml_tests::stdlib_prefix`)
//! and handed to the compile helpers.
//!
//! - [`compile_source`] and friends derive everything honestly. They are the
//!   reference implementation, and the control arm the equivalence oracle in
//!   `baml_tests` compares the fast path against. They are not deprecated.
//! - [`compile_source_with_prefix`] and friends splice a prefix in. The output
//!   is **byte-identical** to the honest path (pinned by that oracle) because
//!   the stdlib *sources* stay in the database — only its interface derivation
//!   and bytecode lowering are skipped.
//!
//! Keeping the sources is load-bearing, not incidental. A database that mounts
//! the stdlib as a source-less precompiled package (what
//! [`ProjectDatabase::set_project_root_with_precompiled_stdlib`] builds, and
//! what runtime `reflect.Package.compile` uses) is much faster still, but it is
//! not a faithful substitute: with no stdlib bodies to look through, a direct
//! sysop call lowers to a plain `call` instead of `sys_op`, and checks that
//! walk stdlib bodies or declaration sites go quiet. Emit helpers therefore
//! never use that mode.

use std::path::Path;

use baml_compiler_diagnostics::{Diagnostic, Severity};
pub use baml_compiler2_emit::OptLevel;
use baml_compiler2_emit::{
    CompileOptions, generate_project_bytecode_with_opt, generate_project_bytecode_with_stdlib,
};
use bex_vm_types::Program;

use crate::{ProjectDatabase, collect_diagnostics, stdlib_prefix::StdlibPrefix};

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

/// Set up a test database whose stdlib interface derivation is served from
/// `prefix` instead of re-derived from source.
///
/// The stdlib *sources* stay in the database, so name resolution, sysop
/// recognition and every body-walking check behave exactly as they do without
/// a prefix — only the per-package `PackageInterface` derivation is skipped.
pub fn setup_test_db_with_prefix(prefix: &StdlibPrefix, source: &str) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    db.set_seeded_stdlib_interface(prefix.interfaces.clone());
    db.add_file("test.baml", source);
    db
}

/// [`setup_test_db_with_prefix`] for a project of several files, for tests
/// that need cross-file or namespaced (`ns_<name>/`) layout.
pub fn setup_multi_file_db_with_prefix(
    prefix: &StdlibPrefix,
    files: &[(&str, &str)],
) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    db.set_seeded_stdlib_interface(prefix.interfaces.clone());
    for (path, content) in files {
        db.add_file(*path, content);
    }
    db
}

/// Diagnostics for the project's **user** files only.
///
/// [`collect_diagnostics`] deliberately re-checks every file on every call,
/// because narrowing the set on a long-lived, edited database could serve stale
/// diagnostics for a clean file that depends on a changed signature. A test
/// database is neither long-lived nor edited: it is populated once and read
/// once, so nothing can be stale and the stdlib's ~50 files — which no test
/// asserts on, and which the callers below already filter out — need not be
/// checked at all.
///
/// The package-level pass runs too, so cross-file duplicate and shadow
/// diagnostics (which belong to no single file) are not lost.
pub fn check_user_files(db: &ProjectDatabase) -> Vec<Diagnostic> {
    let files = db.get_source_files();
    let mut diagnostics: Vec<Diagnostic> =
        files.iter().flat_map(|file| db.check_file(*file)).collect();
    diagnostics.extend(crate::check::package_level_diagnostics(db, &files));
    // Same total order [`collect_diagnostics`] imposes, so a caller that
    // snapshots the list sees no difference from narrowing the checked set.
    crate::check::sort_diagnostics(&mut diagnostics);
    diagnostics
}

/// Panic with every error rendered, or return if there are none.
#[track_caller]
fn assert_clean(errors: &[&Diagnostic]) {
    use std::fmt::Write;

    if errors.is_empty() {
        return;
    }
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

/// [`assert_no_diagnostic_errors`] over [`check_user_files`].
#[track_caller]
pub fn assert_no_user_diagnostic_errors(db: &ProjectDatabase) {
    let diagnostics = check_user_files(db);
    let errors: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .collect();
    assert_clean(&errors);
}

/// [`compile_source_with_opt`] accelerated by `prefix`.
///
/// Byte-identical to the honest path — pinned by the equivalence oracle in
/// `baml_tests` — because the prefix only replaces work whose inputs are the
/// stdlib sources that are still present in the database.
///
/// # Panics
///
/// If `opt` differs from the level `prefix` was built at: the spliced program
/// must be lowered the same way as the user code emitted on top of it.
pub fn compile_source_with_prefix(prefix: &StdlibPrefix, source: &str, opt: OptLevel) -> Program {
    compile_multi_file_with_prefix(prefix, &[("test.baml", source)], opt)
}

/// [`compile_multi_file`] accelerated by `prefix`. See
/// [`compile_source_with_prefix`] for the guarantees and the panic.
pub fn compile_multi_file_with_prefix(
    prefix: &StdlibPrefix,
    files: &[(&str, &str)],
    opt: OptLevel,
) -> Program {
    assert_eq!(
        prefix.opt, opt,
        "stdlib prefix was lowered at {:?} but the caller asked to compile at {opt:?}; \
         the spliced prefix and the user code emitted on top of it must agree",
        prefix.opt
    );
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    db.set_seeded_stdlib_interface(prefix.interfaces.clone());
    for (path, content) in files {
        db.add_file(*path, content);
    }
    assert_no_user_diagnostic_errors(&db);

    let opts = CompileOptions {
        emit_test_cases: false,
    };
    generate_project_bytecode_with_stdlib(&db, &opts, opt, &prefix.program)
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
