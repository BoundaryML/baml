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
use bex_vm_types::{ConstValue, GlobalPool, ObjectIndex, Program as VmProgram};

// Re-export test types from crate::vm
pub use crate::vm::{
    BlockEvent, ExecState, Instance, Instruction, Notification, Object, Value, Variant,
};

/// Backwards-compatible alias for code that still references `TestDatabase`.
pub type TestDatabase = ProjectDatabase;

/// Create a [`BexVm`] from a compiled program for direct VM testing.
///
/// Bypasses `BexEngine` — these tests exercise raw bytecode execution,
/// exec-state sequencing, and watch/emit behaviour the engine does not expose.
/// Production callers must use `BexEngine`.
pub fn make_vm(program: VmProgram) -> Result<BexVm, bex_vm::errors::VmError> {
    let bytecode = bex_vm::convert_program(program)?;

    let objects: Vec<_> = bytecode.objects.into_iter().collect();
    let heap = bex_heap::BexHeap::new(objects);

    let globals: Vec<_> = bytecode
        .globals
        .into_iter()
        .map(|cv| cv.to_value(|idx| heap.compile_time_ptr(idx.into_raw())))
        .collect();

    Ok(BexVm::new(heap, GlobalPool::from_vec(globals)))
}

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

    let Some(project) = db.get_project() else {
        panic!("project must be set");
    };
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

    let Some(project) = db.get_project() else {
        panic!("project must be set");
    };
    let options = baml_compiler_emit::CompileOptions {
        emit_test_cases: false,
    };
    match baml_compiler_emit::compile_files(
        &db,
        project.files(&db),
        baml_compiler_emit::OptLevel::One,
        &options,
    ) {
        Ok(program) => program,
        Err(err) => panic!("compile_files should succeed for valid test source: {err}"),
    }
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

    let mut vm = make_vm(program)?;
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

    let mut expected_iter = input.expected.iter();
    let mut emit_index = 0;

    for state in &states {
        let ExecState::Emit(roots) = state else {
            continue;
        };

        let Some(expected) = expected_iter.next() else {
            panic!(
                "VM emit states mismatch for function '{}': unexpected extra emit at index {}",
                input.function, emit_index
            );
        };

        assert_eq!(
            roots, expected,
            "VM emit states mismatch for function '{}': emit index {}",
            input.function, emit_index
        );

        emit_index += 1;
    }

    if expected_iter.next().is_some() {
        panic!(
            "VM emit states mismatch for function '{}': missing emit at index {}",
            input.function, emit_index
        );
    }

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

    let mut vm = make_vm(program)?;
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
        bytecode: {
            let num_instructions = input.instructions.len();
            bex_vm_types::Bytecode {
                meta: vec![
                    bex_vm_types::bytecode::InstructionMeta { operand: None };
                    num_instructions
                ],
                instructions: input.instructions,
                constants: input.constants,
                resolved_constants: Vec::new(),
                jump_tables: Vec::new(),
                line_table: if num_instructions == 0 {
                    Vec::new()
                } else {
                    vec![bex_vm_types::bytecode::LineTableEntry {
                        pc: 0,
                        span: baml_base::Span::fake(),
                        line: 1,
                        sequence_point: true,
                        discriminator: 0,
                    }]
                },
            }
        },
        kind: bex_vm_types::FunctionKind::Bytecode,
        local_names: {
            let mut names = Vec::with_capacity(input.arity + input.real_local_count);
            names.resize_with(names.capacity(), String::new);
            names
        },
        debug_locals: Vec::new(),
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

    let mut vm = make_vm(program)?;
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
