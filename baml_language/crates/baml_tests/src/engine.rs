//! Unified test infrastructure for bytecode snapshots + BexExternalValue execution.
//!
//! Combines bytecode compilation display (via `display_program`) with VM execution
//! through `BexEngine` (which handles `BexExternalValue` ↔ VM value conversions).
//!
//! # Usage
//!
//! ```ignore
//! use baml_tests::baml_test;
//! use bex_engine::BexExternalValue;
//!
//! #[tokio::test]
//! async fn my_test() {
//!     let output = baml_test!("
//!         function main() -> int { 42 }
//!     ");
//!
//!     insta::assert_snapshot!(output.bytecode, @"...");
//!     assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
//! }
//! ```

use std::{path::Path, sync::Arc};

pub use baml_compiler2_emit::OptLevel;
use baml_project::ProjectDatabase;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_vm::debug::{BytecodeFormat, display_program};
use bex_vm_types::{Function, Object, Program};
pub use indexmap::IndexMap;
use sys_native::SysOpsExt;

/// Set up a test database from BAML source code.
fn setup_test_db(source: &str) -> ProjectDatabase {
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
fn assert_no_diagnostic_errors(db: &ProjectDatabase) {
    use baml_compiler_diagnostics::Severity;

    let project = db.get_project().expect("project must be set");
    let all_files = db.get_source_files();
    let diagnostics = baml_project::collect_diagnostics(db, project, &all_files);

    // Build a set of file IDs that belong to user files (not builtins).
    let user_file_ids: std::collections::HashSet<_> =
        all_files.iter().map(|f| f.file_id(db)).collect();

    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .filter(|d| {
            // Only include errors from user files; skip builtin stdlib errors.
            d.primary_span()
                .map(|span| user_file_ids.contains(&span.file_id))
                .unwrap_or(false)
        })
        .collect();
    if !errors.is_empty() {
        let mut msg = String::from("Compilation produced diagnostic errors:\n");
        for (i, err) in errors.iter().enumerate() {
            msg.push_str(&format!("  {}. [{}] {}\n", i + 1, err.code(), err.message));
        }
        panic!("{msg}");
    }
}

/// Output of a unified test: bytecode display + execution result.
pub struct TestOutput {
    /// Textual bytecode display of all user-defined functions (for insta snapshots).
    pub bytecode: String,
    /// VM execution result (may be an error for error-testing scenarios).
    pub result: Result<BexExternalValue, bex_engine::EngineError>,
}

/// Compile BAML source with default optimization (OptLevel::One).
pub fn compile_source(source: &str) -> Program {
    compile_source_with_opt(source, OptLevel::One)
}

/// Compile BAML source with a specific optimization level.
pub fn compile_source_with_opt(source: &str, opt: OptLevel) -> Program {
    let db = setup_test_db(source);
    assert_no_diagnostic_errors(&db);

    let opts = baml_compiler2_emit::CompileOptions {
        emit_test_cases: false,
    };
    baml_compiler2_emit::generate_project_bytecode_with_opt(&db, &opts, opt)
        .expect("generate_project_bytecode should succeed for valid test source")
}

/// Extract user-defined functions from a program and display them in textual format.
///
/// Strips the `"user."` package prefix from function names so snapshots show
/// `function main()` rather than `function user.main()`.
pub fn display_user_functions(program: &Program) -> String {
    let mut functions: Vec<(String, &Function)> = program
        .function_indices
        .iter()
        .filter(|(name, _)| {
            !name.starts_with("baml.")
                && !name.starts_with("testing.")
                && !name.starts_with("assert.")
                && !name.starts_with("env.")
        })
        .filter_map(|(name, idx)| match program.objects.get(*idx) {
            Some(Object::Function(f)) => {
                // Strip leading "user." package prefix for display.
                let display_name = name
                    .strip_prefix("user.")
                    .unwrap_or(name.as_str())
                    .to_owned();
                Some((display_name, &**f))
            }
            _ => None,
        })
        .collect();
    functions.sort_by(|(a, _), (b, _)| a.cmp(b));
    display_program(&functions, BytecodeFormat::Textual)
}

/// Resolve a user-provided entry name to the fully-qualified name used in the program.
///
/// Compiler2 qualifies function names with their package (e.g. `"user.main"`).
/// Test code passes bare names (`"main"`), so we try both the bare name and the
/// `"user.<name>"` qualified form, returning whichever is present.
fn resolve_entry_name(program: &Program, entry: &str) -> String {
    // Try exact match first.
    if program.function_index(entry).is_some() {
        return entry.to_owned();
    }
    // Try with "user." prefix (compiler2 qualifies user functions).
    let qualified = format!("user.{entry}");
    if program.function_indices.contains_key(qualified.as_str()) {
        return qualified;
    }
    panic!("function '{entry}' not found in program (tried '{entry}' and 'user.{entry}')")
}

/// Resolve named arguments to positional order using function parameter names.
fn resolve_args(
    program: &Program,
    entry: &str,
    args: IndexMap<&str, BexExternalValue>,
) -> Vec<BexExternalValue> {
    let resolved_entry = resolve_entry_name(program, entry);
    let function_idx = program
        .function_index(&resolved_entry)
        .unwrap_or_else(|| panic!("function '{entry}' not found in program"));

    let function = match program.objects.get(function_idx) {
        Some(Object::Function(f)) => f,
        other => panic!(
            "expected Function object for '{entry}', got {:?}",
            other.map(std::mem::discriminant)
        ),
    };

    for provided in args.keys() {
        if !function.param_names.iter().any(|p| p == provided) {
            panic!("unexpected argument '{provided}' for function '{entry}'");
        }
    }

    if args.len() != function.param_names.len() {
        panic!(
            "argument count mismatch for function '{entry}': expected {}, got {}",
            function.param_names.len(),
            args.len()
        );
    }

    function
        .param_names
        .iter()
        .map(|param_name| {
            args.get(param_name.as_str())
                .cloned()
                .unwrap_or_else(|| panic!("missing argument '{param_name}' for function '{entry}'"))
        })
        .collect()
}

/// Compile BAML source, display bytecode, and execute the entry function.
///
/// This is the core function behind the `baml_test!` macro. It:
/// 1. Compiles the source to bytecode
/// 2. Displays all user-defined functions in textual format (for insta snapshots)
/// 3. Resolves named arguments to positional order
/// 4. Executes the entry function via `BexEngine` and returns the result as `Result<BexExternalValue, EngineError>`
pub async fn run_test(
    source: &str,
    entry: &str,
    args: IndexMap<&str, BexExternalValue>,
    opt: OptLevel,
) -> TestOutput {
    let program = compile_source_with_opt(source, opt);

    // Display bytecode before the engine consumes the program.
    let bytecode = display_user_functions(&program);

    // Resolve the entry name (bare "main" → "user.main" for compiler2 output).
    let resolved_entry = resolve_entry_name(&program, entry);

    // Resolve named args to positional before the engine consumes the program.
    let positional_args = resolve_args(&program, entry, args);

    // Create engine and execute.
    let engine = BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), None)
        .expect("Failed to create BexEngine");
    let engine = Arc::new(engine);

    let result = engine
        .call_function(
            &resolved_entry,
            positional_args,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    TestOutput { bytecode, result }
}

/// Like `run_test` but at `OptLevel::Two` (includes MIR constant folding).
pub async fn run_test_mir_optimized(
    source: &str,
    entry: &str,
    args: IndexMap<&str, BexExternalValue>,
) -> TestOutput {
    run_test(source, entry, args, OptLevel::Two).await
}
