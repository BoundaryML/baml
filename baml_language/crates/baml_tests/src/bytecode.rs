//! Shared test utilities for BAML bytecode testing.
//!
//! This module provides common infrastructure for testing bytecode compilation
//! and execution in `bex_vm`.
//!
//! # Contents
//!
//! - [`ProjectDatabase`] (re-exported): The database used for compilation.
//! - [`compile_source`]: Compiles BAML source to a VM program.
//! - [`assert_vm_executes`], [`assert_vm_fails`]: Test assertion helpers.
//! - [`Program`], [`FailingProgram`]: Test input types.
//!
//! # Usage
//!
//! ```ignore
//! use baml_tests::bytecode::{Program, ExecState, Value, assert_vm_executes};
//!
//! assert_vm_executes(Program {
//!     source: "function main() -> int { 42 }",
//!     function: "main",
//!     expected: ExecState::Complete(Value::Int(42)),
//! });
//! ```

#![allow(clippy::needless_pass_by_value)] // Test utilities intentionally take ownership

use std::path::Path;

pub use baml_project::ProjectDatabase;
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{ConstValue, ObjectIndex, Program as VmProgram};

// Re-export test types from crate::vm
pub use crate::vm::{
    BlockEvent, ExecState, Instance, Instruction, Notification, Object, Value, Variant,
};

/// Backwards-compatible alias for code that still references `TestDatabase`.
pub type TestDatabase = ProjectDatabase;

//
// ────────────────────────────────────────────────────────── COMPILATION ─────
//

/// Set up a test database from BAML source code.
///
/// Creates a `ProjectDatabase`, sets a project root, and adds the source as
/// `test.baml`. Builtins are loaded automatically via `set_project_root()`.
pub fn setup_test_db(source: &str) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    db.add_file("test.baml", source);
    db
}

/// Assert that a `ProjectDatabase` has no diagnostic errors.
///
/// Panics with a descriptive message if any error-level diagnostics are found.
/// Warnings and info-level diagnostics are ignored.
#[track_caller]
pub fn assert_no_diagnostic_errors(db: &ProjectDatabase) {
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

/// Compile BAML source code into a VM program.
///
/// Also checks for diagnostic errors and panics if any are found.
pub fn compile_source(source: &str) -> VmProgram {
    let db = setup_test_db(source);
    assert_no_diagnostic_errors(&db);

    let project = db.get_project().unwrap();
    let all_files = project.files(&db).clone();
    baml_compiler_emit::compile_files(&db, &all_files)
        .expect("compile_files should succeed for valid test source")
}

//
// ──────────────────────────────────────────────────── VM TEST UTILITIES ─────
//

/// Helper struct for testing VM execution.
pub struct ProgramInput<Expect> {
    pub source: &'static str,
    pub function: &'static str,
    pub expected: Expect,
}

/// Test input for successful VM execution.
pub type Program = ProgramInput<ExecState>;

/// Test input for VM execution that should fail.
pub type FailingProgram = ProgramInput<bex_vm::errors::VmError>;

/// Test input for VM execution with watch/emit states.
pub type WatchProgram = ProgramInput<Vec<Vec<Notification>>>;

/// Assert that VM execution fails with the expected error.
pub fn assert_vm_fails(input: FailingProgram) -> anyhow::Result<()> {
    assert_vm_fails_with_inspection(input, |_vm| Ok(()))
}

/// Assert that VM execution fails, with access to inspect the VM state.
pub fn assert_vm_fails_with_inspection(
    input: FailingProgram,
    inspect: impl FnOnce(&BexVm) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, result) = setup_and_exec_program(input.source, input.function)?;

    assert_eq!(
        result,
        Err(input.expected),
        "VM execution result mismatch for function '{}'",
        input.function
    );

    inspect(&vm)?;

    Ok(())
}

/// Assert that VM execution succeeds with the expected result.
#[track_caller]
pub fn assert_vm_executes(input: Program) -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(input, |_vm| Ok(()))
}

/// Assert that VM execution succeeds, with access to inspect the VM state.
#[track_caller]
pub fn assert_vm_executes_with_inspection(
    input: Program,
    inspect: impl FnOnce(&BexVm) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, result) = setup_and_exec_program(input.source, input.function)?;
    let result = result?;

    let test_result = ExecState::from_vm_exec_state(result, &vm)?;

    assert_eq!(
        test_result, input.expected,
        "VM execution result mismatch for function '{}'",
        input.function
    );

    inspect(&vm)?;

    Ok(())
}

/// Collects all VM execution states by repeatedly calling `exec()` until completion.
pub fn collect_vm_exec_states(
    source: &'static str,
    function: &str,
) -> anyhow::Result<(BexVm, Vec<ExecState>)> {
    let program = compile_source(source);

    let function_index = program
        .function_index(function)
        .ok_or_else(|| anyhow::anyhow!("function '{function}' not found"))?;

    let mut vm = BexVm::from_program(program)?;
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &[]);

    let mut states = Vec::new();

    loop {
        let result = vm.exec()?;
        // Skip SpanNotify states — these are span lifecycle events from
        // traced function calls that aren't relevant for watch/emit tests.
        if matches!(result, VmExecState::SpanNotify(_)) {
            continue;
        }
        let is_complete = matches!(result, VmExecState::Complete(_));
        let test_state = ExecState::from_vm_exec_state(result, &vm)?;
        states.push(test_state);

        if is_complete {
            break;
        }
    }

    Ok((vm, states))
}

/// Assert that VM execution emits the expected watch notifications.
#[track_caller]
pub fn assert_vm_emits(input: WatchProgram) -> anyhow::Result<()> {
    assert_vm_emits_with_inspection(input, |_vm, _states| Ok(()))
}

/// Assert that VM execution emits notifications, with access to inspect the VM state.
#[track_caller]
pub fn assert_vm_emits_with_inspection(
    input: WatchProgram,
    inspect: impl FnOnce(&BexVm, &[ExecState]) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, states) = collect_vm_exec_states(input.source, input.function)?;

    let emit_states: Vec<Vec<Notification>> = states
        .iter()
        .filter_map(|state| match state {
            ExecState::Emit(roots) => Some(roots.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(
        emit_states, input.expected,
        "VM emit states mismatch for function '{}'",
        input.function
    );

    inspect(&vm, &states)?;

    Ok(())
}

fn setup_and_exec_program(
    source: &'static str,
    function: &str,
) -> Result<(BexVm, Result<VmExecState, bex_vm::errors::VmError>), anyhow::Error> {
    let program = compile_source(source);

    let function_index = program
        .function_index(function)
        .ok_or_else(|| anyhow::anyhow!("function '{function}' not found"))?;

    let mut vm = BexVm::from_program(program)?;
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &[]);

    // Loop past SpanNotify states. Traced function calls yield SpanNotify
    // before reaching the actual result.
    let result = loop {
        let result = vm.exec();
        match &result {
            Ok(VmExecState::SpanNotify(_)) => continue,
            _ => break result,
        }
    };
    Ok((vm, result))
}

//
// ────────────────────────────────────────────────── BYTECODE TEST UTILS ─────
//

/// Helper struct for testing VM execution with direct bytecode.
pub struct BytecodeProgram {
    pub arity: usize,
    /// Number of additional frame-local slots to preallocate.
    pub real_local_count: usize,
    pub instructions: Vec<bex_vm_types::Instruction>,
    pub constants: Vec<ConstValue>,
    pub expected: VmExecState,
}

/// Assert that direct bytecode execution succeeds with the expected result.
pub fn assert_vm_executes_bytecode(input: BytecodeProgram) -> anyhow::Result<()> {
    assert_vm_executes_bytecode_with_inspection(input, |_vm, _result| Ok(()))
}

/// Assert that direct bytecode execution succeeds, with access to inspect the VM state.
pub fn assert_vm_executes_bytecode_with_inspection(
    input: BytecodeProgram,
    inspect: impl FnOnce(&BexVm, VmExecState) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let function = bex_vm_types::Function {
        name: "test_fn".to_string(),
        arity: input.arity,
        real_local_count: input.real_local_count,
        bytecode: bex_vm_types::Bytecode {
            source_lines: vec![1; input.instructions.len()],
            scopes: vec![0; input.instructions.len()],
            instructions: input.instructions,
            constants: input.constants,
            resolved_constants: Vec::new(), // Populated by BexHeap at load time
            jump_tables: Vec::new(),
        },
        kind: bex_vm_types::FunctionKind::Bytecode,
        locals_in_scope: {
            let mut names = Vec::with_capacity(input.arity + input.real_local_count + 1);
            names.push("<fn test_fn>".to_string());
            names.resize_with(names.capacity(), String::new);
            vec![names]
        },
        span: baml_base::Span::fake(),
        block_notifications: Vec::new(),
        viz_nodes: Vec::new(),
        return_type: baml_type::Ty::Null,
        param_names: Vec::new(),
        param_types: Vec::new(),
        body_meta: None,
        trace: false,
    };

    let mut program = VmProgram::new();
    let fn_idx = program.add_object(bex_vm_types::Object::Function(Box::new(function)));
    program.add_global(ConstValue::Object(ObjectIndex::from_raw(fn_idx)));
    program
        .function_indices
        .insert("test_fn".to_string(), fn_idx);

    let mut vm = BexVm::from_program(program)?;
    // Get HeapPtr for function from the heap
    let function_ptr = vm.heap.compile_time_ptr(fn_idx);
    vm.set_entry_point(function_ptr, &[]);

    let result = vm.exec()?;

    assert_eq!(
        result, input.expected,
        "VM execution result mismatch for bytecode test",
    );

    inspect(&vm, result)?;

    Ok(())
}
