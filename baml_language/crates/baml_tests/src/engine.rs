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

pub use baml_compiler_emit::OptLevel;
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

/// Assert that a `ProjectDatabase` has no diagnostic errors.
#[track_caller]
fn assert_no_diagnostic_errors(db: &ProjectDatabase) {
    use baml_compiler_diagnostics::Severity;

    let project = db.get_project().expect("project must be set");
    let all_files = db.get_source_files();
    let diagnostics = baml_project::collect_diagnostics(db, project, &all_files);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
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
    compile_source_with_opt(source, baml_compiler_emit::OptLevel::One)
}

/// Compile BAML source with a specific optimization level.
pub fn compile_source_with_opt(source: &str, opt: baml_compiler_emit::OptLevel) -> Program {
    let db = setup_test_db(source);
    assert_no_diagnostic_errors(&db);

    let project = db.get_project().unwrap();
    let all_files = project.files(&db).clone();
    let options = baml_compiler_emit::CompileOptions {
        emit_test_cases: false,
    };
    baml_compiler_emit::compile_files(&db, &all_files, opt, &options)
        .expect("compile_files should succeed for valid test source")
}

/// Extract user-defined functions from a program and display them in textual format.
fn display_user_functions(program: &Program) -> String {
    let mut functions: Vec<(String, &Function)> = program
        .function_indices
        .iter()
        .filter(|(name, _)| !name.starts_with("baml."))
        .filter_map(|(name, idx)| match program.objects.get(*idx) {
            Some(Object::Function(f)) => Some((name.clone(), &**f)),
            _ => None,
        })
        .collect();
    functions.sort_by(|(a, _), (b, _)| a.cmp(b));
    display_program(&functions, BytecodeFormat::Textual)
}

/// Resolve named arguments to positional order using function parameter names.
fn resolve_args(
    program: &Program,
    entry: &str,
    args: IndexMap<&str, BexExternalValue>,
) -> Vec<BexExternalValue> {
    let function_idx = program
        .function_index(entry)
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
    opt: baml_compiler_emit::OptLevel,
) -> TestOutput {
    let program = compile_source_with_opt(source, opt);

    // Display bytecode before the engine consumes the program.
    let bytecode = display_user_functions(&program);

    // Resolve named args to positional before the engine consumes the program.
    let positional_args = resolve_args(&program, entry, args);

    // Create engine and execute.
    let engine = BexEngine::new(program, Arc::new(sys_types::SysOps::native()), None)
        .expect("Failed to create BexEngine");

    let result = engine
        .call_function(
            entry,
            positional_args,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await;

    TestOutput { bytecode, result }
}
